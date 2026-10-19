mod row_height;

pub use row_height::{resolve_row_heights, CellContent, ExcelMetricProfile};

use xlsxpdf_fonts::FontMetrics;
use xlsxpdf_model::sheet_geometry::{SheetGeometry, excel_col_width_to_pt};
use xlsxpdf_text::TextMeasurer;

#[derive(Debug, Clone)]
pub struct PageGeometry {
    pub width_pt: f64,
    pub height_pt: f64,
    pub margin_top: f64,
    pub margin_bottom: f64,
    pub margin_left: f64,
    pub margin_right: f64,
}

impl PageGeometry {
    pub fn letter() -> Self {
        Self {
            width_pt: 612.0,
            height_pt: 792.0,
            margin_top: 36.0,
            margin_bottom: 36.0,
            margin_left: 36.0,
            margin_right: 36.0,
        }
    }

    pub fn a4() -> Self {
        Self {
            width_pt: 595.28,
            height_pt: 841.89,
            margin_top: 36.0,
            margin_bottom: 36.0,
            margin_left: 36.0,
            margin_right: 36.0,
        }
    }

    pub fn content_width(&self) -> f64 {
        self.width_pt - self.margin_left - self.margin_right
    }

    pub fn content_height(&self) -> f64 {
        self.height_pt - self.margin_top - self.margin_bottom
    }
}

impl Default for PageGeometry {
    fn default() -> Self {
        Self::letter()
    }
}

#[derive(Debug, Clone)]
pub struct SheetLayout {
    pub col_widths: Vec<f64>,
    pub row_heights: Vec<f64>,
    pub col_x: Vec<f64>,
    pub row_y: Vec<f64>,
}

impl SheetLayout {
    pub fn new(col_widths: Vec<f64>, row_heights: Vec<f64>) -> Self {
        let mut col_x = Vec::with_capacity(col_widths.len() + 1);
        let mut x = 0.0;
        for &w in &col_widths {
            col_x.push(x);
            x += w;
        }
        col_x.push(x);

        let mut row_y = Vec::with_capacity(row_heights.len() + 1);
        let mut y = 0.0;
        for &h in &row_heights {
            row_y.push(y);
            y += h;
        }
        row_y.push(y);

        Self {
            col_widths,
            row_heights,
            col_x,
            row_y,
        }
    }

    pub fn from_geometry(
        geom: &SheetGeometry,
        num_cols: usize,
        num_rows: usize,
        default_font: &FontMetrics,
        default_font_size: f64,
    ) -> Self {
        let measurer = TextMeasurer::new(default_font, default_font_size);
        let max_digit_width = measurer.digit_width();

        let col_widths: Vec<f64> = (0..num_cols)
            .map(|c| {
                let chars = geom.col_width_chars(c as u16);
                excel_col_width_to_pt(chars, max_digit_width)
            })
            .collect();

        let row_heights: Vec<f64> = (0..num_rows)
            .map(|r| geom.row_height_pt(r as u32))
            .collect();

        Self::new(col_widths, row_heights)
    }

    pub fn total_width(&self) -> f64 {
        *self.col_x.last().unwrap_or(&0.0)
    }

    pub fn total_height(&self) -> f64 {
        *self.row_y.last().unwrap_or(&0.0)
    }

    pub fn cell_rect(&self, row: usize, col: usize) -> Option<CellRect> {
        if row >= self.row_heights.len() || col >= self.col_widths.len() {
            return None;
        }
        Some(CellRect {
            x: self.col_x[col],
            y: self.row_y[row],
            width: self.col_widths[col],
            height: self.row_heights[row],
        })
    }

    pub fn num_cols(&self) -> usize {
        self.col_widths.len()
    }

    pub fn num_rows(&self) -> usize {
        self.row_heights.len()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CellRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

pub struct PagePlan {
    pub pages: Vec<PageRange>,
}

#[derive(Debug, Clone)]
pub struct PageRange {
    pub start_row: usize,
    pub end_row: usize,
    pub start_col: usize,
    pub end_col: usize,
}

pub fn compute_pages(layout: &SheetLayout, page: &PageGeometry) -> PagePlan {
    let content_w = page.content_width();
    let content_h = page.content_height();

    let col_bands = compute_bands(&layout.col_x, content_w, layout.num_cols());
    let row_bands = compute_bands(&layout.row_y, content_h, layout.num_rows());

    let mut pages = Vec::new();
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

    PagePlan { pages }
}

fn compute_bands(positions: &[f64], max_span: f64, count: usize) -> Vec<(usize, usize)> {
    if count == 0 {
        return vec![];
    }

    let mut bands = Vec::new();
    let mut start = 0;

    while start < count {
        let band_origin = positions[start];
        let mut end = start;

        while end < count {
            let span = positions[end + 1] - band_origin;
            if span > max_span && end > start {
                break;
            }
            end += 1;
        }

        bands.push((start, end));
        start = end;
    }

    bands
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sheet_layout() {
        let layout = SheetLayout::new(vec![64.0; 5], vec![15.0; 10]);
        assert_eq!(layout.total_width(), 320.0);
        assert_eq!(layout.total_height(), 150.0);
        let r = layout.cell_rect(2, 3).unwrap();
        assert_eq!(r.x, 192.0);
        assert_eq!(r.y, 30.0);
    }

    #[test]
    fn test_from_geometry() {
        let mut geom = SheetGeometry::default();
        geom.col_widths.push(xlsxpdf_model::sheet_geometry::ColInfo {
            min: 1,
            max: 3,
            width: 15.0,
            custom_width: true,
            hidden: false,
        });
        let font = FontMetrics::calibri();
        let layout = SheetLayout::from_geometry(&geom, 5, 3, &font, 11.0);
        assert_eq!(layout.num_cols(), 5);
        assert_eq!(layout.num_rows(), 3);
        // Cols 0-2 should be wider than cols 3-4
        assert!(layout.col_widths[0] > layout.col_widths[3]);
    }

    #[test]
    fn test_pagination() {
        let layout = SheetLayout::new(vec![100.0; 8], vec![20.0; 50]);
        let page = PageGeometry::letter();
        let plan = compute_pages(&layout, &page);
        assert!(plan.pages.len() > 1);
    }

    #[test]
    fn test_single_page() {
        let layout = SheetLayout::new(vec![50.0; 3], vec![15.0; 5]);
        let page = PageGeometry::letter();
        let plan = compute_pages(&layout, &page);
        assert_eq!(plan.pages.len(), 1);
    }
}
