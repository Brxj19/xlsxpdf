use xlsxpdf_fonts::FontMetrics;

use crate::measure::TextMeasurer;

#[derive(Debug, Clone)]
pub struct WrapResult {
    pub lines: Vec<String>,
    pub total_height_pt: f64,
    pub max_width_pt: f64,
}

pub fn wrap_text(
    text: &str,
    available_width: f64,
    metrics: &FontMetrics,
    size_pt: f64,
) -> WrapResult {
    let measurer = TextMeasurer::new(metrics, size_pt);
    let line_height = measurer.line_height();

    let mut lines = Vec::new();
    let mut max_width: f64 = 0.0;

    // Split on explicit newlines first
    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            lines.push(String::new());
            continue;
        }

        let wrapped = wrap_paragraph(paragraph, available_width, &measurer);
        for line in &wrapped {
            let w = measurer.string_width(line);
            if w > max_width {
                max_width = w;
            }
        }
        lines.extend(wrapped);
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    let total_height = lines.len() as f64 * line_height;

    WrapResult {
        lines,
        total_height_pt: total_height,
        max_width_pt: max_width,
    }
}

fn wrap_paragraph(text: &str, available_width: f64, measurer: &TextMeasurer) -> Vec<String> {
    if available_width <= 0.0 {
        return vec![text.to_string()];
    }

    let full_width = measurer.string_width(text);
    if full_width <= available_width {
        return vec![text.to_string()];
    }

    let mut lines = Vec::new();
    let words: Vec<&str> = text.split_inclusive(|c: char| c.is_whitespace() || c == '-')
        .collect();

    if words.is_empty() {
        return vec![text.to_string()];
    }

    let mut current_line = String::new();
    let mut current_width: f64 = 0.0;

    for word in &words {
        let word_width = measurer.string_width(word);

        if current_line.is_empty() {
            // First word on line — always take it even if it overflows
            current_line.push_str(word);
            current_width = word_width;
        } else if current_width + word_width <= available_width {
            current_line.push_str(word);
            current_width += word_width;
        } else {
            lines.push(current_line.trim_end().to_string());
            current_line = word.trim_start().to_string();
            current_width = measurer.string_width(&current_line);
        }
    }

    if !current_line.is_empty() {
        lines.push(current_line.trim_end().to_string());
    }

    if lines.is_empty() {
        lines.push(text.to_string());
    }

    lines
}

pub fn compute_shrink_scale(natural_width: f64, available_width: f64) -> f64 {
    if natural_width <= available_width || natural_width <= 0.0 {
        1.0
    } else {
        (available_width / natural_width).max(0.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_wrap_fits() {
        let metrics = FontMetrics::calibri();
        let result = wrap_text("Hello", 200.0, &metrics, 11.0);
        assert_eq!(result.lines.len(), 1);
        assert_eq!(result.lines[0], "Hello");
    }

    #[test]
    fn test_wrap_long_text() {
        let metrics = FontMetrics::calibri();
        let text = "The quick brown fox jumps over the lazy dog and keeps running";
        let result = wrap_text(text, 100.0, &metrics, 11.0);
        assert!(result.lines.len() > 1);
        assert!(result.total_height_pt > 0.0);
    }

    #[test]
    fn test_explicit_newlines() {
        let metrics = FontMetrics::calibri();
        let text = "Line one\nLine two\nLine three";
        let result = wrap_text(text, 500.0, &metrics, 11.0);
        assert_eq!(result.lines.len(), 3);
        assert_eq!(result.lines[0], "Line one");
        assert_eq!(result.lines[1], "Line two");
        assert_eq!(result.lines[2], "Line three");
    }

    #[test]
    fn test_shrink_scale() {
        assert_eq!(compute_shrink_scale(50.0, 100.0), 1.0);
        assert!((compute_shrink_scale(200.0, 100.0) - 0.5).abs() < 0.01);
    }
}
