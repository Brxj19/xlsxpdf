use xlsxpdf_fonts::FontMetrics;

pub struct TextMeasurer<'a> {
    metrics: &'a FontMetrics,
    size_pt: f64,
}

#[derive(Debug, Clone)]
pub struct MeasuredLine {
    pub text: String,
    pub width_pt: f64,
    pub ascent_pt: f64,
    pub descent_pt: f64,
}

impl<'a> TextMeasurer<'a> {
    pub fn new(metrics: &'a FontMetrics, size_pt: f64) -> Self {
        Self { metrics, size_pt }
    }

    pub fn string_width(&self, text: &str) -> f64 {
        self.metrics.string_width_pt(text, self.size_pt)
    }

    pub fn char_width(&self, ch: char) -> f64 {
        self.metrics.glyph_advance_pt(ch, self.size_pt)
    }

    pub fn line_height(&self) -> f64 {
        self.metrics.line_height_pt(self.size_pt)
    }

    pub fn ascent(&self) -> f64 {
        self.metrics.ascent_pt(self.size_pt)
    }

    pub fn descent(&self) -> f64 {
        self.metrics.descent_pt(self.size_pt)
    }

    pub fn digit_width(&self) -> f64 {
        self.metrics.digit_zero_width_pt(self.size_pt)
    }

    pub fn measure_line(&self, text: &str) -> MeasuredLine {
        MeasuredLine {
            text: text.to_string(),
            width_pt: self.string_width(text),
            ascent_pt: self.ascent(),
            descent_pt: self.descent(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_measurer() {
        let metrics = FontMetrics::calibri();
        let m = TextMeasurer::new(&metrics, 11.0);

        let w = m.string_width("Hello");
        assert!(w > 0.0);
        assert!(w < 50.0);

        let line = m.measure_line("Hello");
        assert_eq!(line.text, "Hello");
        assert!(line.ascent_pt > 0.0);
        assert!(line.descent_pt > 0.0);
    }

    #[test]
    fn test_proportional_measurement() {
        let metrics = FontMetrics::calibri();
        let m = TextMeasurer::new(&metrics, 11.0);

        let w_iii = m.string_width("iii");
        let w_mmm = m.string_width("MMM");
        assert!(w_mmm > w_iii * 2.0);
    }
}
