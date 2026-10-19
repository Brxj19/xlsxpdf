#[derive(Debug, Clone)]
pub struct FontMetrics {
    pub units_per_em: u16,
    pub ascent: i16,
    pub descent: i16,
    pub line_gap: i16,
    pub avg_char_width: i16,
    pub max_char_width: i16,
    pub cap_height: i16,
    pub x_height: i16,
    pub char_widths: CharWidthTable,
}

impl FontMetrics {
    pub fn char_width_pt(&self, size_pt: f64) -> f64 {
        size_pt * self.avg_char_width as f64 / self.units_per_em as f64
    }

    pub fn glyph_advance_pt(&self, ch: char, size_pt: f64) -> f64 {
        let units = self.char_widths.advance(ch) as f64;
        size_pt * units / self.units_per_em as f64
    }

    pub fn line_height_pt(&self, size_pt: f64) -> f64 {
        let total = self.ascent as f64 - self.descent as f64 + self.line_gap as f64;
        size_pt * total / self.units_per_em as f64
    }

    pub fn ascent_pt(&self, size_pt: f64) -> f64 {
        size_pt * self.ascent as f64 / self.units_per_em as f64
    }

    pub fn descent_pt(&self, size_pt: f64) -> f64 {
        size_pt * (-self.descent as f64) / self.units_per_em as f64
    }

    pub fn string_width_pt(&self, text: &str, size_pt: f64) -> f64 {
        let mut total: u32 = 0;
        for ch in text.chars() {
            total += self.char_widths.advance(ch) as u32;
        }
        size_pt * total as f64 / self.units_per_em as f64
    }

    pub fn digit_zero_width_pt(&self, size_pt: f64) -> f64 {
        self.glyph_advance_pt('0', size_pt)
    }
}

#[derive(Debug, Clone)]
pub struct CharWidthTable {
    ascii: [u16; 128],
    default_width: u16,
}

impl CharWidthTable {
    pub fn advance(&self, ch: char) -> u16 {
        let cp = ch as u32;
        if cp < 128 {
            self.ascii[cp as usize]
        } else {
            self.default_width
        }
    }
}

impl FontMetrics {
    pub fn calibri() -> Self {
        // Calibri proportional widths (units per em = 2048)
        // Derived from Calibri font metric tables
        let mut ascii = [507u16; 128]; // default for control chars
        // Control chars = 0 width
        for w in ascii.iter_mut().take(32) {
            *w = 0;
        }
        ascii[32] = 459;  // space
        ascii[33] = 367;  // !
        ascii[34] = 495;  // "
        ascii[35] = 702;  // #
        ascii[36] = 600;  // $
        ascii[37] = 873;  // %
        ascii[38] = 770;  // &
        ascii[39] = 291;  // '
        ascii[40] = 367;  // (
        ascii[41] = 367;  // )
        ascii[42] = 532;  // *
        ascii[43] = 702;  // +
        ascii[44] = 319;  // ,
        ascii[45] = 418;  // -
        ascii[46] = 319;  // .
        ascii[47] = 424;  // /
        ascii[48] = 642;  // 0
        ascii[49] = 642;  // 1
        ascii[50] = 642;  // 2
        ascii[51] = 642;  // 3
        ascii[52] = 642;  // 4
        ascii[53] = 642;  // 5
        ascii[54] = 642;  // 6
        ascii[55] = 642;  // 7
        ascii[56] = 642;  // 8
        ascii[57] = 642;  // 9
        ascii[58] = 319;  // :
        ascii[59] = 319;  // ;
        ascii[60] = 702;  // <
        ascii[61] = 702;  // =
        ascii[62] = 702;  // >
        ascii[63] = 486;  // ?
        ascii[64] = 1031; // @
        ascii[65] = 698;  // A
        ascii[66] = 668;  // B
        ascii[67] = 621;  // C
        ascii[68] = 742;  // D
        ascii[69] = 586;  // E
        ascii[70] = 536;  // F
        ascii[71] = 734;  // G
        ascii[72] = 736;  // H
        ascii[73] = 279;  // I
        ascii[74] = 428;  // J
        ascii[75] = 648;  // K
        ascii[76] = 536;  // L
        ascii[77] = 903;  // M
        ascii[78] = 736;  // N
        ascii[79] = 770;  // O
        ascii[80] = 642;  // P
        ascii[81] = 770;  // Q
        ascii[82] = 680;  // R
        ascii[83] = 564;  // S
        ascii[84] = 594;  // T
        ascii[85] = 716;  // U
        ascii[86] = 652;  // V
        ascii[87] = 978;  // W
        ascii[88] = 634;  // X
        ascii[89] = 596;  // Y
        ascii[90] = 618;  // Z
        ascii[91] = 367;  // [
        ascii[92] = 424;  // backslash
        ascii[93] = 367;  // ]
        ascii[94] = 702;  // ^
        ascii[95] = 498;  // _
        ascii[96] = 507;  // `
        ascii[97] = 579;  // a
        ascii[98] = 620;  // b
        ascii[99] = 494;  // c
        ascii[100] = 620; // d
        ascii[101] = 582; // e
        ascii[102] = 357; // f
        ascii[103] = 596; // g
        ascii[104] = 618; // h
        ascii[105] = 257; // i
        ascii[106] = 279; // j
        ascii[107] = 546; // k
        ascii[108] = 257; // l
        ascii[109] = 929; // m
        ascii[110] = 618; // n
        ascii[111] = 608; // o
        ascii[112] = 620; // p
        ascii[113] = 620; // q
        ascii[114] = 391; // r
        ascii[115] = 484; // s
        ascii[116] = 382; // t
        ascii[117] = 618; // u
        ascii[118] = 536; // v
        ascii[119] = 812; // w
        ascii[120] = 536; // x
        ascii[121] = 536; // y
        ascii[122] = 494; // z
        ascii[123] = 367; // {
        ascii[124] = 367; // |
        ascii[125] = 367; // }
        ascii[126] = 702; // ~
        ascii[127] = 0;

        Self {
            units_per_em: 2048,
            ascent: 1536,
            descent: -512,
            line_gap: 0,
            avg_char_width: 536,
            max_char_width: 1328,
            cap_height: 1360,
            x_height: 950,
            char_widths: CharWidthTable {
                ascii,
                default_width: 600,
            },
        }
    }

    pub fn arial() -> Self {
        // Arial/Helvetica proportional widths (units per em = 2048)
        let mut ascii = [569u16; 128];
        for w in ascii.iter_mut().take(32) {
            *w = 0;
        }
        ascii[32] = 569;  // space
        ascii[33] = 569;  // !
        ascii[34] = 727;  // "
        ascii[35] = 1139; // #
        ascii[36] = 1139; // $
        ascii[37] = 1821; // %
        ascii[38] = 1366; // &
        ascii[39] = 391;  // '
        ascii[40] = 682;  // (
        ascii[41] = 682;  // )
        ascii[42] = 797;  // *
        ascii[43] = 1196; // +
        ascii[44] = 569;  // ,
        ascii[45] = 682;  // -
        ascii[46] = 569;  // .
        ascii[47] = 569;  // /
        ascii[48] = 1139; // 0
        ascii[49] = 1139; // 1
        ascii[50] = 1139; // 2
        ascii[51] = 1139; // 3
        ascii[52] = 1139; // 4
        ascii[53] = 1139; // 5
        ascii[54] = 1139; // 6
        ascii[55] = 1139; // 7
        ascii[56] = 1139; // 8
        ascii[57] = 1139; // 9
        ascii[58] = 569;  // :
        ascii[59] = 569;  // ;
        ascii[60] = 1196; // <
        ascii[61] = 1196; // =
        ascii[62] = 1196; // >
        ascii[63] = 1139; // ?
        ascii[64] = 2079; // @
        ascii[65] = 1366; // A
        ascii[66] = 1366; // B
        ascii[67] = 1479; // C
        ascii[68] = 1479; // D
        ascii[69] = 1366; // E
        ascii[70] = 1251; // F
        ascii[71] = 1593; // G
        ascii[72] = 1479; // H
        ascii[73] = 569;  // I
        ascii[74] = 1024; // J
        ascii[75] = 1366; // K
        ascii[76] = 1139; // L
        ascii[77] = 1706; // M
        ascii[78] = 1479; // N
        ascii[79] = 1593; // O
        ascii[80] = 1366; // P
        ascii[81] = 1593; // Q
        ascii[82] = 1479; // R
        ascii[83] = 1366; // S
        ascii[84] = 1251; // T
        ascii[85] = 1479; // U
        ascii[86] = 1366; // V
        ascii[87] = 1933; // W
        ascii[88] = 1366; // X
        ascii[89] = 1366; // Y
        ascii[90] = 1251; // Z
        ascii[91] = 569;  // [
        ascii[92] = 569;  // backslash
        ascii[93] = 569;  // ]
        ascii[94] = 961;  // ^
        ascii[95] = 1139; // _
        ascii[96] = 682;  // `
        ascii[97] = 1139; // a
        ascii[98] = 1139; // b
        ascii[99] = 1024; // c
        ascii[100] = 1139; // d
        ascii[101] = 1139; // e
        ascii[102] = 569;  // f
        ascii[103] = 1139; // g
        ascii[104] = 1139; // h
        ascii[105] = 455;  // i
        ascii[106] = 455;  // j
        ascii[107] = 1024; // k
        ascii[108] = 455;  // l
        ascii[109] = 1706; // m
        ascii[110] = 1139; // n
        ascii[111] = 1139; // o
        ascii[112] = 1139; // p
        ascii[113] = 1139; // q
        ascii[114] = 682;  // r
        ascii[115] = 1024; // s
        ascii[116] = 569;  // t
        ascii[117] = 1139; // u
        ascii[118] = 1024; // v
        ascii[119] = 1479; // w
        ascii[120] = 1024; // x
        ascii[121] = 1024; // y
        ascii[122] = 1024; // z
        ascii[123] = 684;  // {
        ascii[124] = 532;  // |
        ascii[125] = 684;  // }
        ascii[126] = 1196; // ~
        ascii[127] = 0;

        Self {
            units_per_em: 2048,
            ascent: 1854,
            descent: -434,
            line_gap: 67,
            avg_char_width: 904,
            max_char_width: 2000,
            cap_height: 1467,
            x_height: 1062,
            char_widths: CharWidthTable {
                ascii,
                default_width: 1139,
            },
        }
    }

    pub fn times_new_roman() -> Self {
        let mut ascii = [512u16; 128];
        for w in ascii.iter_mut().take(32) {
            *w = 0;
        }
        ascii[32] = 512;  // space
        ascii[33] = 682;  // !
        ascii[34] = 836;  // "
        ascii[35] = 1024; // #
        ascii[36] = 1024; // $
        ascii[37] = 1706; // %
        ascii[38] = 1593; // &
        ascii[39] = 369;  // '
        ascii[40] = 682;  // (
        ascii[41] = 682;  // )
        ascii[42] = 1024; // *
        ascii[43] = 1196; // +
        ascii[44] = 512;  // ,
        ascii[45] = 682;  // -
        ascii[46] = 512;  // .
        ascii[47] = 569;  // /
        ascii[48] = 1024; // 0
        ascii[49] = 1024; // 1
        ascii[50] = 1024; // 2
        ascii[51] = 1024; // 3
        ascii[52] = 1024; // 4
        ascii[53] = 1024; // 5
        ascii[54] = 1024; // 6
        ascii[55] = 1024; // 7
        ascii[56] = 1024; // 8
        ascii[57] = 1024; // 9
        ascii[58] = 569;  // :
        ascii[59] = 569;  // ;
        ascii[60] = 1196; // <
        ascii[61] = 1196; // =
        ascii[62] = 1196; // >
        ascii[63] = 909;  // ?
        ascii[64] = 1866; // @
        ascii[65] = 1479; // A
        ascii[66] = 1366; // B
        ascii[67] = 1366; // C
        ascii[68] = 1479; // D
        ascii[69] = 1251; // E
        ascii[70] = 1139; // F
        ascii[71] = 1479; // G
        ascii[72] = 1479; // H
        ascii[73] = 682;  // I
        ascii[74] = 797;  // J
        ascii[75] = 1479; // K
        ascii[76] = 1251; // L
        ascii[77] = 1821; // M
        ascii[78] = 1479; // N
        ascii[79] = 1479; // O
        ascii[80] = 1139; // P
        ascii[81] = 1479; // Q
        ascii[82] = 1366; // R
        ascii[83] = 1139; // S
        ascii[84] = 1251; // T
        ascii[85] = 1479; // U
        ascii[86] = 1479; // V
        ascii[87] = 1933; // W
        ascii[88] = 1479; // X
        ascii[89] = 1479; // Y
        ascii[90] = 1251; // Z
        ascii[91] = 682;  // [
        ascii[92] = 569;  // backslash
        ascii[93] = 682;  // ]
        ascii[94] = 961;  // ^
        ascii[95] = 1024; // _
        ascii[96] = 682;  // `
        ascii[97] = 909;  // a
        ascii[98] = 1024; // b
        ascii[99] = 909;  // c
        ascii[100] = 1024; // d
        ascii[101] = 909;  // e
        ascii[102] = 682;  // f
        ascii[103] = 1024; // g
        ascii[104] = 1024; // h
        ascii[105] = 569;  // i
        ascii[106] = 569;  // j
        ascii[107] = 1024; // k
        ascii[108] = 569;  // l
        ascii[109] = 1593; // m
        ascii[110] = 1024; // n
        ascii[111] = 1024; // o
        ascii[112] = 1024; // p
        ascii[113] = 1024; // q
        ascii[114] = 682;  // r
        ascii[115] = 797;  // s
        ascii[116] = 569;  // t
        ascii[117] = 1024; // u
        ascii[118] = 1024; // v
        ascii[119] = 1479; // w
        ascii[120] = 1024; // x
        ascii[121] = 1024; // y
        ascii[122] = 909;  // z
        ascii[123] = 983;  // {
        ascii[124] = 410;  // |
        ascii[125] = 983;  // }
        ascii[126] = 1196; // ~
        ascii[127] = 0;

        Self {
            units_per_em: 2048,
            ascent: 1825,
            descent: -443,
            line_gap: 87,
            avg_char_width: 819,
            max_char_width: 2000,
            cap_height: 1356,
            x_height: 916,
            char_widths: CharWidthTable {
                ascii,
                default_width: 1024,
            },
        }
    }

    pub fn fallback() -> Self {
        Self::calibri()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proportional_width() {
        let m = FontMetrics::calibri();
        let w_i = m.glyph_advance_pt('i', 11.0);
        let w_m = m.glyph_advance_pt('M', 11.0);
        assert!(w_m > w_i * 2.0, "M should be much wider than i");
    }

    #[test]
    fn test_string_width_proportional() {
        let m = FontMetrics::calibri();
        let w_narrow = m.string_width_pt("iii", 11.0);
        let w_wide = m.string_width_pt("MMM", 11.0);
        assert!(w_wide > w_narrow * 2.0);
    }

    #[test]
    fn test_digit_widths_uniform() {
        let m = FontMetrics::calibri();
        let w0 = m.glyph_advance_pt('0', 11.0);
        let w5 = m.glyph_advance_pt('5', 11.0);
        assert!((w0 - w5).abs() < 0.01, "digits should be monospaced");
    }
}
