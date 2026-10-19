use xlsxpdf_fonts::FontMetrics;
use xlsxpdf_model::sheet_geometry::SheetGeometry;
use xlsxpdf_text::{TextMeasurer, wrap_text};

#[derive(Debug, Clone)]
pub struct ExcelMetricProfile {
    pub cell_left_inset: f64,
    pub cell_right_inset: f64,
    pub cell_top_inset: f64,
    pub cell_bottom_inset: f64,
    pub line_leading: f64,
    pub min_row_height: f64,
}

impl Default for ExcelMetricProfile {
    fn default() -> Self {
        Self {
            cell_left_inset: 2.0,
            cell_right_inset: 2.0,
            cell_top_inset: 1.0,
            cell_bottom_inset: 1.0,
            line_leading: 0.5,
            min_row_height: 6.0,
        }
    }
}

pub struct CellContent {
    pub row: u32,
    pub col: u16,
    pub text: String,
    pub wrap_text: bool,
    pub font_size_pt: f64,
}

pub fn resolve_row_heights(
    geom: &SheetGeometry,
    cells: &[CellContent],
    col_widths_pt: &[f64],
    font: &FontMetrics,
    profile: &ExcelMetricProfile,
    num_rows: usize,
) -> Vec<f64> {
    let mut heights: Vec<f64> = (0..num_rows)
        .map(|r| geom.row_height_pt(r as u32))
        .collect();

    // For rows with custom heights from the workbook, keep them
    // For rows without, auto-size based on content
    for cell in cells {
        let row = cell.row as usize;
        let col = cell.col as usize;
        if row >= num_rows || col >= col_widths_pt.len() {
            continue;
        }

        // Skip if the workbook specifies a custom height for this row
        if let Some(info) = geom.row_heights.get(&cell.row) {
            if info.custom_height {
                continue;
            }
            if info.hidden {
                heights[row] = 0.0;
                continue;
            }
        }

        let measurer = TextMeasurer::new(font, cell.font_size_pt);
        let line_height = measurer.line_height();

        let needed_height = if cell.wrap_text && !cell.text.is_empty() {
            let available = col_widths_pt[col] - profile.cell_left_inset - profile.cell_right_inset;
            if available > 0.0 {
                let result = wrap_text(&cell.text, available, font, cell.font_size_pt);
                let text_height = result.lines.len() as f64 * (line_height + profile.line_leading);
                text_height + profile.cell_top_inset + profile.cell_bottom_inset
            } else {
                line_height + profile.cell_top_inset + profile.cell_bottom_inset
            }
        } else if cell.text.contains('\n') {
            let line_count = cell.text.matches('\n').count() + 1;
            line_count as f64 * (line_height + profile.line_leading)
                + profile.cell_top_inset
                + profile.cell_bottom_inset
        } else {
            line_height + profile.cell_top_inset + profile.cell_bottom_inset
        };

        if needed_height > heights[row] {
            heights[row] = needed_height;
        }
    }

    for h in &mut heights {
        if *h < profile.min_row_height && *h > 0.0 {
            *h = profile.min_row_height;
        }
    }

    heights
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_heights() {
        let geom = SheetGeometry::default();
        let heights = resolve_row_heights(
            &geom,
            &[],
            &[64.0; 3],
            &FontMetrics::calibri(),
            &ExcelMetricProfile::default(),
            5,
        );
        assert_eq!(heights.len(), 5);
        for h in &heights {
            assert!((*h - 15.0).abs() < 0.01);
        }
    }

    #[test]
    fn test_wrap_increases_height() {
        let geom = SheetGeometry::default();
        let cells = vec![CellContent {
            row: 0,
            col: 0,
            text: "This is a very long text that should wrap to multiple lines in the cell".into(),
            wrap_text: true,
            font_size_pt: 11.0,
        }];
        let heights = resolve_row_heights(
            &geom,
            &cells,
            &[60.0, 60.0],
            &FontMetrics::calibri(),
            &ExcelMetricProfile::default(),
            3,
        );
        assert!(heights[0] > 15.0, "wrapped text should increase row height");
        assert!((heights[1] - 15.0).abs() < 0.01, "non-affected row stays default");
    }

    #[test]
    fn test_newline_increases_height() {
        let geom = SheetGeometry::default();
        let cells = vec![CellContent {
            row: 0,
            col: 0,
            text: "Line 1\nLine 2\nLine 3".into(),
            wrap_text: false,
            font_size_pt: 11.0,
        }];
        let heights = resolve_row_heights(
            &geom,
            &cells,
            &[200.0],
            &FontMetrics::calibri(),
            &ExcelMetricProfile::default(),
            2,
        );
        assert!(heights[0] > 30.0, "3 lines should be taller than default");
    }
}
