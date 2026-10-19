use xlsxpdf_model::print_settings::{PrintSettings, PageOrder};

#[derive(Debug, Clone)]
pub struct PagePlan {
    pub pages: Vec<PageRange>,
    pub scale: f64,
    pub page_width_pt: f64,
    pub page_height_pt: f64,
}

#[derive(Debug, Clone)]
pub struct PageRange {
    pub start_row: usize,
    pub end_row: usize,
    pub start_col: usize,
    pub end_col: usize,
}

pub fn compile_pagination(
    col_widths: &[f64],
    row_heights: &[f64],
    settings: &PrintSettings,
) -> PagePlan {
    let page_w = settings.page_width_pt();
    let page_h = settings.page_height_pt();
    let printable_w = settings.printable_width_pt();
    let printable_h = settings.printable_height_pt();

    let total_content_w: f64 = col_widths.iter().sum();
    let total_content_h: f64 = row_heights.iter().sum();

    // Determine scale
    let scale = compute_scale(
        settings,
        total_content_w,
        total_content_h,
        printable_w,
        printable_h,
    );

    // Work in unscaled worksheet coordinates
    let capacity_x = printable_w / scale;
    let capacity_y = printable_h / scale;

    // Build column bands
    let col_bands = if settings.col_breaks.is_empty() || settings.use_fit_to_page {
        compute_bands_simple(col_widths, capacity_x)
    } else {
        compute_bands_with_break_indices(
            col_widths,
            capacity_x,
            settings.col_breaks.iter().map(|&b| b as usize),
        )
    };

    // Build row bands
    let row_bands = if settings.row_breaks.is_empty() || settings.use_fit_to_page {
        compute_bands_simple(row_heights, capacity_y)
    } else {
        compute_bands_with_break_indices(
            row_heights,
            capacity_y,
            settings.row_breaks.iter().map(|&b| b as usize),
        )
    };

    // Enumerate pages based on page order
    let mut pages = Vec::new();
    match settings.page_order {
        PageOrder::DownThenOver => {
            for col_band in &col_bands {
                for row_band in &row_bands {
                    pages.push(PageRange {
                        start_row: row_band.0,
                        end_row: row_band.1,
                        start_col: col_band.0,
                        end_col: col_band.1,
                    });
                }
            }
        }
        PageOrder::OverThenDown => {
            for row_band in &row_bands {
                for col_band in &col_bands {
                    pages.push(PageRange {
                        start_row: row_band.0,
                        end_row: row_band.1,
                        start_col: col_band.0,
                        end_col: col_band.1,
                    });
                }
            }
        }
    }

    PagePlan {
        pages,
        scale,
        page_width_pt: page_w,
        page_height_pt: page_h,
    }
}

fn compute_scale(
    settings: &PrintSettings,
    content_w: f64,
    content_h: f64,
    printable_w: f64,
    printable_h: f64,
) -> f64 {
    if settings.use_fit_to_page {
        let mut candidates = Vec::new();

        if let Some(nw) = settings.fit_to_width {
            if content_w > 0.0 {
                candidates.push(nw as f64 * printable_w / content_w);
            }
        }
        if let Some(nh) = settings.fit_to_height {
            if content_h > 0.0 {
                candidates.push(nh as f64 * printable_h / content_h);
            }
        }

        if candidates.is_empty() {
            return 1.0;
        }

        let s = candidates.iter().cloned().fold(f64::INFINITY, f64::min);
        s.min(1.0).max(0.1)
    } else {
        let s = settings.scale as f64 / 100.0;
        s.clamp(0.1, 4.0)
    }
}

fn compute_bands_simple(sizes: &[f64], capacity: f64) -> Vec<(usize, usize)> {
    if sizes.is_empty() {
        return vec![];
    }

    let tolerance = capacity * 1e-9;
    let mut bands = Vec::new();
    let mut start = 0;

    while start < sizes.len() {
        let mut end = start;
        let mut span = 0.0;

        while end < sizes.len() {
            let next = span + sizes[end];
            if next > capacity + tolerance && end > start {
                break;
            }
            span = next;
            end += 1;
        }

        bands.push((start, end));
        start = end;
    }

    bands
}

fn compute_bands_with_break_indices(
    sizes: &[f64],
    capacity: f64,
    break_indices: impl Iterator<Item = usize>,
) -> Vec<(usize, usize)> {
    let break_set: std::collections::HashSet<usize> = break_indices.collect();

    if sizes.is_empty() {
        return vec![];
    }

    let mut bands = Vec::new();
    let mut start = 0;

    while start < sizes.len() {
        let mut end = start;
        let mut span = 0.0;

        while end < sizes.len() {
            // Force break if we hit a manual break point
            if break_set.contains(&end) && end > start {
                break;
            }

            let next = span + sizes[end];
            if next > capacity && end > start {
                break;
            }
            span = next;
            end += 1;
        }

        bands.push((start, end));
        start = end;
    }

    bands
}

pub fn compute_centering_offset(
    content_extent: f64,
    printable_extent: f64,
    scale: f64,
    center: bool,
) -> f64 {
    if !center {
        return 0.0;
    }
    let rendered = content_extent * scale;
    if rendered >= printable_extent {
        return 0.0;
    }
    (printable_extent - rendered) / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_pagination() {
        let col_widths = vec![100.0; 8];
        let row_heights = vec![20.0; 50];
        let settings = PrintSettings::default();

        let plan = compile_pagination(&col_widths, &row_heights, &settings);
        assert!(plan.pages.len() > 1);
        assert!((plan.scale - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_fit_to_width() {
        let col_widths = vec![100.0; 10]; // 1000pt total
        let row_heights = vec![20.0; 5];
        let mut settings = PrintSettings::default();
        settings.use_fit_to_page = true;
        settings.fit_to_width = Some(1);

        let plan = compile_pagination(&col_widths, &row_heights, &settings);
        assert!(plan.scale < 1.0);
        // All cols should fit on one column band
        let col_bands: Vec<_> = plan.pages.iter()
            .map(|p| (p.start_col, p.end_col))
            .collect();
        // First page should span all columns
        assert_eq!(col_bands[0], (0, 10));
    }

    #[test]
    fn test_landscape() {
        let col_widths = vec![50.0; 3];
        let row_heights = vec![15.0; 5];
        let mut settings = PrintSettings::default();
        settings.orientation = xlsxpdf_model::print_settings::Orientation::Landscape;

        let plan = compile_pagination(&col_widths, &row_heights, &settings);
        assert!((plan.page_width_pt - 792.0).abs() < 0.01);
        assert!((plan.page_height_pt - 612.0).abs() < 0.01);
    }

    #[test]
    fn test_centering() {
        let offset = compute_centering_offset(200.0, 540.0, 1.0, true);
        assert!((offset - 170.0).abs() < 0.01);

        let offset = compute_centering_offset(200.0, 540.0, 1.0, false);
        assert!((offset - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_page_breaks() {
        let row_heights = vec![20.0; 30];
        let bands = compute_bands_with_break_indices(
            &row_heights,
            10000.0, // huge capacity
            vec![10usize, 20].into_iter(),
        );
        assert_eq!(bands.len(), 3);
        assert_eq!(bands[0], (0, 10));
        assert_eq!(bands[1], (10, 20));
        assert_eq!(bands[2], (20, 30));
    }

    #[test]
    fn test_down_then_over() {
        let col_widths = vec![300.0; 4]; // forces 2+ col bands
        let row_heights = vec![200.0; 6]; // forces 2+ row bands
        let mut settings = PrintSettings::default();
        settings.page_order = PageOrder::DownThenOver;

        let plan = compile_pagination(&col_widths, &row_heights, &settings);
        assert!(plan.pages.len() > 1);

        // In DownThenOver, first pages should have the same col band
        if plan.pages.len() >= 2 {
            assert_eq!(plan.pages[0].start_col, plan.pages[1].start_col);
        }
    }
}
