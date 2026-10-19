use xlsxpdf_core::error::Result;
use xlsxpdf_xml::{XmlEvent, XmlReader};

use crate::theme::ThemePalette;
use crate::types::*;

#[derive(Debug)]
pub struct StyleSheet {
    pub fonts: Vec<FontInfo>,
    pub fills: Vec<FillInfo>,
    pub borders: Vec<BorderInfo>,
    pub num_fmts: Vec<(u32, String)>,
    pub cell_xfs: Vec<CanonicalStyle>,
}

impl StyleSheet {
    pub fn parse(xml: &[u8]) -> Result<Self> {
        Self::parse_with_theme(xml, None)
    }

    pub fn parse_with_theme(xml: &[u8], theme: Option<&ThemePalette>) -> Result<Self> {
        let mut reader = XmlReader::new(xml);
        let mut sheet = StyleSheet {
            fonts: Vec::new(),
            fills: Vec::new(),
            borders: Vec::new(),
            num_fmts: Vec::new(),
            cell_xfs: Vec::new(),
        };

        let mut section = Section::None;
        let mut current_font = FontInfo::default();
        let mut current_fill = FillInfo::default();
        let mut current_border = BorderInfo::default();
        let mut current_border_side = BorderSideTarget::None;
        let mut current_xf = XfBuilder::default();
        let mut _in_alignment = false;

        loop {
            match reader.next_event()? {
                XmlEvent::StartElement { local_name, attrs } => {
                    match local_name.as_str() {
                        "fonts" => section = Section::Fonts,
                        "fills" => section = Section::Fills,
                        "borders" => section = Section::Borders,
                        "numFmts" => section = Section::NumFmts,
                        "cellXfs" => section = Section::CellXfs,

                        "font" if section == Section::Fonts => {
                            current_font = FontInfo::default();
                        }
                        "sz" if section == Section::Fonts => {
                            if let Some(v) = get_attr(&attrs, "val") {
                                if let Ok(f) = v.parse::<f64>() {
                                    current_font.size_hundredths = (f * 100.0) as u32;
                                }
                            }
                        }
                        "name" | "rFont" if section == Section::Fonts => {
                            if let Some(v) = get_attr(&attrs, "val") {
                                current_font.name = v;
                            }
                        }
                        "b" if section == Section::Fonts => {
                            current_font.bold = get_attr(&attrs, "val")
                                .map_or(true, |v| v != "0" && v != "false");
                        }
                        "i" if section == Section::Fonts => {
                            current_font.italic = get_attr(&attrs, "val")
                                .map_or(true, |v| v != "0" && v != "false");
                        }
                        "u" if section == Section::Fonts => {
                            current_font.underline = true;
                        }
                        "strike" if section == Section::Fonts => {
                            current_font.strikethrough = true;
                        }
                        "color" if section == Section::Fonts => {
                            current_font.color = parse_color(&attrs, theme);
                        }

                        "fill" if section == Section::Fills => {
                            current_fill = FillInfo::default();
                        }
                        "patternFill" if section == Section::Fills => {
                            let pattern = get_attr(&attrs, "patternType").unwrap_or_default();
                            current_fill.pattern = match pattern.as_str() {
                                "none" => FillPattern::None,
                                "solid" => FillPattern::Solid,
                                "gray125" => FillPattern::Other(1),
                                _ => FillPattern::Other(2),
                            };
                        }
                        "fgColor" if section == Section::Fills => {
                            current_fill.fg_color = parse_color(&attrs, theme);
                        }
                        "bgColor" if section == Section::Fills => {
                            current_fill.bg_color = parse_color(&attrs, theme);
                        }

                        "border" if section == Section::Borders => {
                            current_border = BorderInfo::default();
                        }
                        "left" if section == Section::Borders => {
                            current_border_side = BorderSideTarget::Left;
                            if let Some(style) = get_attr(&attrs, "style") {
                                current_border.left.style = BorderStyle::from_str(&style);
                            }
                        }
                        "right" if section == Section::Borders => {
                            current_border_side = BorderSideTarget::Right;
                            if let Some(style) = get_attr(&attrs, "style") {
                                current_border.right.style = BorderStyle::from_str(&style);
                            }
                        }
                        "top" if section == Section::Borders => {
                            current_border_side = BorderSideTarget::Top;
                            if let Some(style) = get_attr(&attrs, "style") {
                                current_border.top.style = BorderStyle::from_str(&style);
                            }
                        }
                        "bottom" if section == Section::Borders => {
                            current_border_side = BorderSideTarget::Bottom;
                            if let Some(style) = get_attr(&attrs, "style") {
                                current_border.bottom.style = BorderStyle::from_str(&style);
                            }
                        }
                        "color" if section == Section::Borders => {
                            let color = parse_color(&attrs, theme);
                            match current_border_side {
                                BorderSideTarget::Left => current_border.left.color = color,
                                BorderSideTarget::Right => current_border.right.color = color,
                                BorderSideTarget::Top => current_border.top.color = color,
                                BorderSideTarget::Bottom => current_border.bottom.color = color,
                                BorderSideTarget::None => {}
                            }
                        }

                        "numFmt" if section == Section::NumFmts => {
                            let id: u32 = get_attr(&attrs, "numFmtId")
                                .and_then(|v| v.parse().ok())
                                .unwrap_or(0);
                            let code = get_attr(&attrs, "formatCode").unwrap_or_default();
                            sheet.num_fmts.push((id, code));
                        }

                        "xf" if section == Section::CellXfs => {
                            current_xf = XfBuilder::default();
                            current_xf.font_id = get_attr(&attrs, "fontId")
                                .and_then(|v| v.parse().ok())
                                .unwrap_or(0);
                            current_xf.fill_id = get_attr(&attrs, "fillId")
                                .and_then(|v| v.parse().ok())
                                .unwrap_or(0);
                            current_xf.border_id = get_attr(&attrs, "borderId")
                                .and_then(|v| v.parse().ok())
                                .unwrap_or(0);
                            current_xf.num_fmt_id = get_attr(&attrs, "numFmtId")
                                .and_then(|v| v.parse().ok())
                                .unwrap_or(0);
                        }
                        "alignment" if section == Section::CellXfs => {
                            _in_alignment = true;
                            if let Some(h) = get_attr(&attrs, "horizontal") {
                                current_xf.alignment.horizontal = HorizontalAlignment::from_str(&h);
                            }
                            if let Some(v) = get_attr(&attrs, "vertical") {
                                current_xf.alignment.vertical = VerticalAlignment::from_str(&v);
                            }
                            if get_attr(&attrs, "wrapText").as_deref() == Some("1") {
                                current_xf.alignment.wrap_text = true;
                            }
                            if get_attr(&attrs, "shrinkToFit").as_deref() == Some("1") {
                                current_xf.alignment.shrink_to_fit = true;
                            }
                            if let Some(rot) = get_attr(&attrs, "textRotation") {
                                current_xf.alignment.text_rotation =
                                    rot.parse().unwrap_or(0);
                            }
                            if let Some(ind) = get_attr(&attrs, "indent") {
                                current_xf.alignment.indent = ind.parse().unwrap_or(0);
                            }
                        }

                        _ => {}
                    }
                }
                XmlEvent::EndElement { local_name } => match local_name.as_str() {
                    "font" if section == Section::Fonts => {
                        sheet.fonts.push(current_font.clone());
                    }
                    "fill" if section == Section::Fills => {
                        sheet.fills.push(current_fill.clone());
                    }
                    "border" if section == Section::Borders => {
                        sheet.borders.push(current_border.clone());
                        current_border_side = BorderSideTarget::None;
                    }
                    "left" | "right" | "top" | "bottom" if section == Section::Borders => {
                        current_border_side = BorderSideTarget::None;
                    }
                    "xf" if section == Section::CellXfs => {
                        sheet.cell_xfs.push(CanonicalStyle {
                            font_id: current_xf.font_id,
                            fill_id: current_xf.fill_id,
                            border_id: current_xf.border_id,
                            num_fmt_id: current_xf.num_fmt_id,
                            alignment: current_xf.alignment.clone(),
                        });
                    }
                    "alignment" => {
                        _in_alignment = false;
                    }
                    "fonts" | "fills" | "borders" | "numFmts" | "cellXfs" => {
                        section = Section::None;
                    }
                    _ => {}
                },
                XmlEvent::Text(_) => {}
                XmlEvent::Eof => break,
            }
        }

        Ok(sheet)
    }

    pub fn get_style(&self, style_id: u32) -> Option<&CanonicalStyle> {
        self.cell_xfs.get(style_id as usize)
    }

    pub fn get_font(&self, font_id: u32) -> Option<&FontInfo> {
        self.fonts.get(font_id as usize)
    }

    pub fn get_fill(&self, fill_id: u32) -> Option<&FillInfo> {
        self.fills.get(fill_id as usize)
    }

    pub fn get_border(&self, border_id: u32) -> Option<&BorderInfo> {
        self.borders.get(border_id as usize)
    }

    pub fn get_num_fmt(&self, num_fmt_id: u32) -> Option<&str> {
        self.num_fmts
            .iter()
            .find(|(id, _)| *id == num_fmt_id)
            .map(|(_, code)| code.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    None,
    Fonts,
    Fills,
    Borders,
    NumFmts,
    CellXfs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BorderSideTarget {
    None,
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Default)]
struct XfBuilder {
    font_id: u32,
    fill_id: u32,
    border_id: u32,
    num_fmt_id: u32,
    alignment: AlignmentInfo,
}

fn get_attr(attrs: &[(String, String)], name: &str) -> Option<String> {
    attrs
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.clone())
}

fn parse_color(attrs: &[(String, String)], theme_palette: Option<&ThemePalette>) -> Option<Color> {
    if let Some(rgb) = get_attr(attrs, "rgb") {
        return Color::from_rgb_hex(&rgb);
    }
    if let Some(theme_str) = get_attr(attrs, "theme") {
        let theme_id: u32 = theme_str.parse().ok()?;
        let base = if let Some(pal) = theme_palette {
            pal.get(theme_id)
        } else {
            theme_color_fallback(theme_id)
        };
        if let Some(tint_str) = get_attr(attrs, "tint") {
            if let Ok(tint) = tint_str.parse::<f64>() {
                return Some(apply_tint(base, tint));
            }
        }
        return Some(base);
    }
    if let Some(indexed) = get_attr(attrs, "indexed") {
        let idx: u32 = indexed.parse().ok()?;
        return Some(indexed_color(idx));
    }
    None
}

fn theme_color_fallback(id: u32) -> Color {
    match id {
        0 => Color { r: 255, g: 255, b: 255, a: 255 }, // lt1 (white)
        1 => Color { r: 0, g: 0, b: 0, a: 255 },       // dk1 (black)
        2 => Color { r: 31, g: 73, b: 125, a: 255 },    // dk2
        3 => Color { r: 238, g: 236, b: 225, a: 255 },  // lt2
        4 => Color { r: 79, g: 129, b: 189, a: 255 },   // accent1
        5 => Color { r: 192, g: 80, b: 77, a: 255 },    // accent2
        6 => Color { r: 155, g: 187, b: 89, a: 255 },   // accent3
        7 => Color { r: 128, g: 100, b: 162, a: 255 },  // accent4
        8 => Color { r: 75, g: 172, b: 198, a: 255 },   // accent5
        9 => Color { r: 247, g: 150, b: 70, a: 255 },   // accent6
        10 => Color { r: 0, g: 0, b: 255, a: 255 },     // hlink
        11 => Color { r: 128, g: 0, b: 128, a: 255 },   // folHlink
        _ => Color::BLACK,
    }
}

fn apply_tint(color: Color, tint: f64) -> Color {
    fn to_hsl(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let l = (max + min) / 2.0;

        if (max - min).abs() < 1e-10 {
            return (0.0, 0.0, l);
        }

        let d = max - min;
        let s = if l > 0.5 {
            d / (2.0 - max - min)
        } else {
            d / (max + min)
        };

        let h = if (max - r).abs() < 1e-10 {
            let mut h = (g - b) / d;
            if g < b { h += 6.0; }
            h
        } else if (max - g).abs() < 1e-10 {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        };

        (h / 6.0, s, l)
    }

    fn from_hsl(h: f64, s: f64, l: f64) -> (f64, f64, f64) {
        if s.abs() < 1e-10 {
            return (l, l, l);
        }
        let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
        let p = 2.0 * l - q;
        let hue_to_rgb = |t: f64| -> f64 {
            let mut t = t;
            if t < 0.0 { t += 1.0; }
            if t > 1.0 { t -= 1.0; }
            if t < 1.0 / 6.0 { return p + (q - p) * 6.0 * t; }
            if t < 1.0 / 2.0 { return q; }
            if t < 2.0 / 3.0 { return p + (q - p) * (2.0 / 3.0 - t) * 6.0; }
            p
        };
        (hue_to_rgb(h + 1.0 / 3.0), hue_to_rgb(h), hue_to_rgb(h - 1.0 / 3.0))
    }

    let (h, s, l) = to_hsl(color.r as f64 / 255.0, color.g as f64 / 255.0, color.b as f64 / 255.0);
    let new_l = if tint < 0.0 {
        l * (1.0 + tint)
    } else {
        l * (1.0 - tint) + tint
    };
    let (r, g, b) = from_hsl(h, s, new_l.clamp(0.0, 1.0));
    Color {
        r: (r * 255.0).round() as u8,
        g: (g * 255.0).round() as u8,
        b: (b * 255.0).round() as u8,
        a: color.a,
    }
}

fn indexed_color(idx: u32) -> Color {
    let (r, g, b) = match idx {
        0 => (0, 0, 0),
        1 => (255, 255, 255),
        2 => (255, 0, 0),
        3 => (0, 255, 0),
        4 => (0, 0, 255),
        5 => (255, 255, 0),
        6 => (255, 0, 255),
        7 => (0, 255, 255),
        8 => (0, 0, 0),
        9 => (255, 255, 255),
        10 => (255, 0, 0),
        11 => (0, 255, 0),
        12 => (0, 0, 255),
        13 => (255, 255, 0),
        14 => (255, 0, 255),
        15 => (0, 255, 255),
        16 => (128, 0, 0),
        17 => (0, 128, 0),
        18 => (0, 0, 128),
        19 => (128, 128, 0),
        20 => (128, 0, 128),
        21 => (0, 128, 128),
        22 => (192, 192, 192),
        23 => (128, 128, 128),
        24 => (153, 153, 255),
        25 => (153, 51, 102),
        26 => (255, 255, 204),
        27 => (204, 255, 255),
        28 => (102, 0, 102),
        29 => (255, 128, 128),
        30 => (0, 102, 204),
        31 => (204, 204, 255),
        32 => (0, 0, 128),
        33 => (255, 0, 255),
        34 => (255, 255, 0),
        35 => (0, 255, 255),
        36 => (128, 0, 128),
        37 => (128, 0, 0),
        38 => (0, 128, 128),
        39 => (0, 0, 255),
        40 => (0, 204, 255),
        41 => (204, 255, 255),
        42 => (204, 255, 204),
        43 => (255, 255, 153),
        44 => (153, 204, 255),
        45 => (255, 153, 204),
        46 => (204, 153, 255),
        47 => (255, 204, 153),
        48 => (51, 102, 255),
        49 => (51, 204, 204),
        50 => (153, 204, 0),
        51 => (255, 204, 0),
        52 => (255, 153, 0),
        53 => (255, 102, 0),
        54 => (102, 102, 153),
        55 => (150, 150, 150),
        56 => (0, 51, 102),
        57 => (51, 153, 102),
        58 => (0, 51, 0),
        59 => (51, 51, 0),
        60 => (153, 51, 0),
        61 => (153, 51, 51),
        62 => (51, 51, 153),
        63 => (51, 51, 51),
        64 => (0, 0, 0),       // system foreground
        65 => (255, 255, 255), // system background
        _ => (0, 0, 0),
    };
    Color { r, g, b, a: 255 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_styles() {
        let xml = br##"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <numFmts count="1">
    <numFmt numFmtId="164" formatCode="#,##0.00"/>
  </numFmts>
  <fonts count="2">
    <font>
      <sz val="11"/>
      <name val="Calibri"/>
    </font>
    <font>
      <b/>
      <sz val="14"/>
      <name val="Arial"/>
      <color rgb="FF0000FF"/>
    </font>
  </fonts>
  <fills count="2">
    <fill><patternFill patternType="none"/></fill>
    <fill><patternFill patternType="solid"><fgColor rgb="FFFFFF00"/></patternFill></fill>
  </fills>
  <borders count="2">
    <border>
      <left/><right/><top/><bottom/>
    </border>
    <border>
      <left style="thin"><color rgb="FF000000"/></left>
      <right style="thin"><color rgb="FF000000"/></right>
      <top style="medium"><color rgb="FF000000"/></top>
      <bottom style="medium"><color rgb="FF000000"/></bottom>
    </border>
  </borders>
  <cellXfs count="2">
    <xf fontId="0" fillId="0" borderId="0" numFmtId="0"/>
    <xf fontId="1" fillId="1" borderId="1" numFmtId="164">
      <alignment horizontal="center" vertical="center" wrapText="1"/>
    </xf>
  </cellXfs>
</styleSheet>"##;

        let sheet = StyleSheet::parse(xml).unwrap();
        assert_eq!(sheet.fonts.len(), 2);
        assert_eq!(sheet.fonts[0].name, "Calibri");
        assert_eq!(sheet.fonts[1].name, "Arial");
        assert!(sheet.fonts[1].bold);
        assert_eq!(sheet.fonts[1].size_hundredths, 1400);

        assert_eq!(sheet.fills.len(), 2);
        assert_eq!(sheet.fills[1].pattern, FillPattern::Solid);

        assert_eq!(sheet.borders.len(), 2);
        assert_eq!(sheet.borders[1].left.style, BorderStyle::Thin);
        assert_eq!(sheet.borders[1].top.style, BorderStyle::Medium);

        assert_eq!(sheet.num_fmts.len(), 1);
        assert_eq!(sheet.num_fmts[0].0, 164);
        assert_eq!(sheet.num_fmts[0].1, "#,##0.00");

        assert_eq!(sheet.cell_xfs.len(), 2);
        assert_eq!(sheet.cell_xfs[1].font_id, 1);
        assert!(sheet.cell_xfs[1].alignment.wrap_text);
        assert_eq!(
            sheet.cell_xfs[1].alignment.horizontal,
            HorizontalAlignment::Center
        );
    }
}
