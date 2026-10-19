use std::fmt::Write;

pub struct ContentBuilder {
    buf: String,
}

impl ContentBuilder {
    pub fn new() -> Self {
        Self {
            buf: String::with_capacity(4096),
        }
    }

    pub fn save_state(&mut self) -> &mut Self {
        self.buf.push_str("q\n");
        self
    }

    pub fn restore_state(&mut self) -> &mut Self {
        self.buf.push_str("Q\n");
        self
    }

    pub fn set_ctm(&mut self, a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) -> &mut Self {
        let _ = write!(self.buf, "{a:.4} {b:.4} {c:.4} {d:.4} {e:.2} {f:.2} cm\n");
        self
    }

    pub fn set_fill_color_rgb(&mut self, r: f64, g: f64, b: f64) -> &mut Self {
        let _ = write!(self.buf, "{r:.3} {g:.3} {b:.3} rg\n");
        self
    }

    pub fn set_stroke_color_rgb(&mut self, r: f64, g: f64, b: f64) -> &mut Self {
        let _ = write!(self.buf, "{r:.3} {g:.3} {b:.3} RG\n");
        self
    }

    pub fn set_line_width(&mut self, w: f64) -> &mut Self {
        let _ = write!(self.buf, "{w:.2} w\n");
        self
    }

    pub fn rect(&mut self, x: f64, y: f64, w: f64, h: f64) -> &mut Self {
        let _ = write!(self.buf, "{x:.2} {y:.2} {w:.2} {h:.2} re\n");
        self
    }

    pub fn fill(&mut self) -> &mut Self {
        self.buf.push_str("f\n");
        self
    }

    pub fn stroke(&mut self) -> &mut Self {
        self.buf.push_str("S\n");
        self
    }

    pub fn move_to(&mut self, x: f64, y: f64) -> &mut Self {
        let _ = write!(self.buf, "{x:.2} {y:.2} m\n");
        self
    }

    pub fn line_to(&mut self, x: f64, y: f64) -> &mut Self {
        let _ = write!(self.buf, "{x:.2} {y:.2} l\n");
        self
    }

    pub fn begin_text(&mut self) -> &mut Self {
        self.buf.push_str("BT\n");
        self
    }

    pub fn end_text(&mut self) -> &mut Self {
        self.buf.push_str("ET\n");
        self
    }

    pub fn set_font(&mut self, font_name: &str, size_pt: f64) -> &mut Self {
        let _ = write!(self.buf, "/{font_name} {size_pt:.1} Tf\n");
        self
    }

    pub fn text_position(&mut self, x: f64, y: f64) -> &mut Self {
        let _ = write!(self.buf, "{x:.2} {y:.2} Td\n");
        self
    }

    pub fn set_text_matrix(&mut self, a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) -> &mut Self {
        let _ = write!(self.buf, "{a:.4} {b:.4} {c:.4} {d:.4} {e:.2} {f:.2} Tm\n");
        self
    }

    pub fn show_text(&mut self, text: &str) -> &mut Self {
        self.buf.push('(');
        for ch in text.chars() {
            match unicode_to_cp1252(ch) {
                Some(b) => match b {
                    b'(' => self.buf.push_str("\\("),
                    b')' => self.buf.push_str("\\)"),
                    b'\\' => self.buf.push_str("\\\\"),
                    0..=31 | 128..=255 => {
                        let _ = write!(self.buf, "\\{b:03o}");
                    }
                    _ => self.buf.push(b as char),
                },
                None => self.buf.push('?'),
            }
        }
        self.buf.push_str(") Tj\n");
        self
    }

    pub fn clip_rect(&mut self, x: f64, y: f64, w: f64, h: f64) -> &mut Self {
        self.rect(x, y, w, h);
        self.buf.push_str("W n\n");
        self
    }

    pub fn draw_fill_rect(
        &mut self,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        r: f64,
        g: f64,
        b: f64,
    ) -> &mut Self {
        self.set_fill_color_rgb(r, g, b);
        self.rect(x, y, w, h);
        self.fill();
        self
    }

    pub fn draw_line(
        &mut self,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        line_width: f64,
        r: f64,
        g: f64,
        b: f64,
    ) -> &mut Self {
        self.set_stroke_color_rgb(r, g, b);
        self.set_line_width(line_width);
        self.move_to(x1, y1);
        self.line_to(x2, y2);
        self.stroke();
        self
    }

    pub fn draw_image(
        &mut self,
        name: &str,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    ) -> &mut Self {
        self.save_state();
        let _ = write!(self.buf, "{w:.2} 0 0 {h:.2} {x:.2} {y:.2} cm\n");
        let _ = write!(self.buf, "/{name} Do\n");
        self.restore_state();
        self
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.buf.into_bytes()
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.buf.as_bytes()
    }
}

impl Default for ContentBuilder {
    fn default() -> Self {
        Self::new()
    }
}

fn unicode_to_cp1252(ch: char) -> Option<u8> {
    let cp = ch as u32;
    match cp {
        0x0000..=0x007F => Some(cp as u8),
        0x00A0..=0x00FF => Some(cp as u8),
        0x20AC => Some(0x80), // €
        0x201A => Some(0x82), // ‚
        0x0192 => Some(0x83), // ƒ
        0x201E => Some(0x84), // „
        0x2026 => Some(0x85), // …
        0x2020 => Some(0x86), // †
        0x2021 => Some(0x87), // ‡
        0x02C6 => Some(0x88), // ˆ
        0x2030 => Some(0x89), // ‰
        0x0160 => Some(0x8A), // Š
        0x2039 => Some(0x8B), // ‹
        0x0152 => Some(0x8C), // Œ
        0x017D => Some(0x8E), // Ž
        0x2018 => Some(0x91), // '
        0x2019 => Some(0x92), // '
        0x201C => Some(0x93), // "
        0x201D => Some(0x94), // "
        0x2022 => Some(0x95), // •
        0x2013 => Some(0x96), // –
        0x2014 => Some(0x97), // —
        0x02DC => Some(0x98), // ˜
        0x2122 => Some(0x99), // ™
        0x0161 => Some(0x9A), // š
        0x203A => Some(0x9B), // ›
        0x0153 => Some(0x9C), // œ
        0x017E => Some(0x9E), // ž
        0x0178 => Some(0x9F), // Ÿ
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_builder() {
        let mut cb = ContentBuilder::new();
        cb.save_state()
            .set_ctm(1.0, 0.0, 0.0, -1.0, 0.0, 792.0)
            .set_fill_color_rgb(1.0, 1.0, 0.0)
            .rect(72.0, 72.0, 100.0, 20.0)
            .fill()
            .begin_text()
            .set_font("F1", 10.0)
            .text_position(74.0, 74.0)
            .show_text("Hello")
            .end_text()
            .restore_state();

        let s = String::from_utf8(cb.into_bytes()).unwrap();
        assert!(s.contains("q\n"));
        assert!(s.contains("1.000 0.000 rg"));
        assert!(s.contains("/F1 10.0 Tf"));
        assert!(s.contains("(Hello) Tj"));
        assert!(s.contains("Q\n"));
    }

    #[test]
    fn test_text_escaping() {
        let mut cb = ContentBuilder::new();
        cb.show_text("a(b)c\\d");
        let s = String::from_utf8(cb.into_bytes()).unwrap();
        assert!(s.contains("(a\\(b\\)c\\\\d) Tj"));
    }

    // M14: Hardening tests

    #[test]
    fn test_empty_text() {
        let mut cb = ContentBuilder::new();
        cb.show_text("");
        let s = String::from_utf8(cb.into_bytes()).unwrap();
        assert!(s.contains("() Tj"));
    }

    #[test]
    fn test_control_characters() {
        let mut cb = ContentBuilder::new();
        cb.show_text("\x00\x01\x1f");
        let s = String::from_utf8(cb.into_bytes()).unwrap();
        assert!(s.contains("\\000\\001\\037"));
    }

    #[test]
    fn test_cp1252_latin1_range() {
        let mut cb = ContentBuilder::new();
        cb.show_text("\u{00e9}"); // é → CP1252 0xE9
        let s = String::from_utf8(cb.into_bytes()).unwrap();
        assert!(s.contains("\\351"), "é should be octal 351 (0xE9)");
    }

    #[test]
    fn test_cp1252_special_chars() {
        let mut cb = ContentBuilder::new();
        cb.show_text("\u{2013}\u{2019}\u{20AC}"); // – ' €
        let s = String::from_utf8(cb.into_bytes()).unwrap();
        assert!(s.contains("\\226"), "en-dash should be octal 226 (0x96)");
        assert!(s.contains("\\222"), "right single quote should be octal 222 (0x92)");
        assert!(s.contains("\\200"), "euro sign should be octal 200 (0x80)");
    }

    #[test]
    fn test_unmappable_char_replaced() {
        let mut cb = ContentBuilder::new();
        cb.show_text("a\u{4e16}b"); // 世 (CJK) not in CP1252
        let s = String::from_utf8(cb.into_bytes()).unwrap();
        assert!(s.contains("(a?b) Tj"), "unmappable char replaced with ?");
    }

    #[test]
    fn test_zero_dimensions() {
        let mut cb = ContentBuilder::new();
        cb.rect(0.0, 0.0, 0.0, 0.0);
        cb.draw_image("Im0", 0.0, 0.0, 0.0, 0.0);
        let bytes = cb.into_bytes();
        assert!(!bytes.is_empty());
    }

    #[test]
    fn test_negative_coordinates() {
        let mut cb = ContentBuilder::new();
        cb.draw_fill_rect(-10.0, -10.0, 5.0, 5.0, 0.5, 0.5, 0.5);
        cb.draw_line(-10.0, -10.0, 10.0, 10.0, 1.0, 0.0, 0.0, 0.0);
        let bytes = cb.into_bytes();
        assert!(!bytes.is_empty());
    }

    #[test]
    fn test_large_values() {
        let mut cb = ContentBuilder::new();
        cb.set_ctm(1e10, 0.0, 0.0, 1e10, 0.0, 0.0);
        cb.text_position(1e10, 1e10);
        let bytes = cb.into_bytes();
        assert!(!bytes.is_empty());
    }
}
