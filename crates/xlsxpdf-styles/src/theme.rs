use crate::types::Color;

pub struct ThemePalette {
    colors: [Color; 12],
}

impl ThemePalette {
    pub fn parse(xml: &[u8]) -> Option<Self> {
        let text = std::str::from_utf8(xml).ok()?;
        let order = [
            "dk1", "lt1", "dk2", "lt2",
            "accent1", "accent2", "accent3", "accent4", "accent5", "accent6",
            "hlink", "folHlink",
        ];

        let mut colors = [Color::BLACK; 12];
        for (i, key) in order.iter().enumerate() {
            if let Some(c) = extract_scheme_color(text, key) {
                colors[i] = c;
            }
        }

        Some(Self { colors })
    }

    pub fn get(&self, theme_index: u32) -> Color {
        let mapped = match theme_index {
            0 => 1,  // lt1
            1 => 0,  // dk1
            2 => 3,  // lt2
            3 => 2,  // dk2
            4 => 4,  // accent1
            5 => 5,  // accent2
            6 => 6,  // accent3
            7 => 7,  // accent4
            8 => 8,  // accent5
            9 => 9,  // accent6
            10 => 10, // hlink
            11 => 11, // folHlink
            _ => return Color::BLACK,
        };
        self.colors[mapped]
    }
}

impl Default for ThemePalette {
    fn default() -> Self {
        Self {
            colors: [
                Color { r: 0, g: 0, b: 0, a: 255 },        // dk1
                Color { r: 255, g: 255, b: 255, a: 255 },  // lt1
                Color { r: 68, g: 84, b: 106, a: 255 },    // dk2
                Color { r: 231, g: 230, b: 230, a: 255 },  // lt2
                Color { r: 68, g: 114, b: 196, a: 255 },   // accent1
                Color { r: 237, g: 125, b: 49, a: 255 },   // accent2
                Color { r: 165, g: 165, b: 165, a: 255 },  // accent3
                Color { r: 255, g: 192, b: 0, a: 255 },    // accent4
                Color { r: 91, g: 155, b: 213, a: 255 },   // accent5
                Color { r: 112, g: 173, b: 71, a: 255 },   // accent6
                Color { r: 5, g: 99, b: 193, a: 255 },     // hlink
                Color { r: 149, g: 79, b: 114, a: 255 },   // folHlink
            ],
        }
    }
}

fn extract_scheme_color(text: &str, key: &str) -> Option<Color> {
    let open_tag = format!("<a:{key}>");
    let alt_open = format!("<a:{key} ");
    let close_tag = format!("</a:{key}>");

    let start = text.find(&open_tag)
        .or_else(|| text.find(&alt_open))?;
    let end = text[start..].find(&close_tag)? + start;
    let fragment = &text[start..end];

    if let Some(val) = extract_attr_value(fragment, "srgbClr", "val") {
        return Color::from_rgb_hex(&val);
    }
    if let Some(val) = extract_attr_value(fragment, "sysClr", "lastClr") {
        return Color::from_rgb_hex(&val);
    }
    None
}

fn extract_attr_value(fragment: &str, element: &str, attr: &str) -> Option<String> {
    let tag_prefix = format!("<a:{element} ");
    let alt_prefix = format!("<{element} ");

    let idx = fragment.find(&tag_prefix)
        .or_else(|| fragment.find(&alt_prefix))?;
    let after = &fragment[idx..];
    let end = after.find('>')? + idx;
    let tag = &fragment[idx..=end];

    let pattern = format!("{attr}=\"");
    let attr_start = tag.find(&pattern)? + pattern.len();
    let attr_end = tag[attr_start..].find('"')? + attr_start;
    Some(tag[attr_start..attr_end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_theme() {
        let xml = br#"<?xml version="1.0"?>
<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
  <a:themeElements>
    <a:clrScheme name="Custom">
      <a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1>
      <a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1>
      <a:dk2><a:srgbClr val="44546A"/></a:dk2>
      <a:lt2><a:srgbClr val="E7E6E6"/></a:lt2>
      <a:accent1><a:srgbClr val="4472C4"/></a:accent1>
      <a:accent2><a:srgbClr val="ED7D31"/></a:accent2>
      <a:accent3><a:srgbClr val="A5A5A5"/></a:accent3>
      <a:accent4><a:srgbClr val="FFC000"/></a:accent4>
      <a:accent5><a:srgbClr val="5B9BD5"/></a:accent5>
      <a:accent6><a:srgbClr val="70AD47"/></a:accent6>
      <a:hlink><a:srgbClr val="0563C1"/></a:hlink>
      <a:folHlink><a:srgbClr val="954F72"/></a:folHlink>
    </a:clrScheme>
  </a:themeElements>
</a:theme>"#;

        let pal = ThemePalette::parse(xml).unwrap();

        // theme 0 = lt1 = white
        let c0 = pal.get(0);
        assert_eq!((c0.r, c0.g, c0.b), (255, 255, 255));

        // theme 1 = dk1 = black
        let c1 = pal.get(1);
        assert_eq!((c1.r, c1.g, c1.b), (0, 0, 0));

        // theme 4 = accent1
        let c4 = pal.get(4);
        assert_eq!((c4.r, c4.g, c4.b), (0x44, 0x72, 0xC4));

        // theme 9 = accent6
        let c9 = pal.get(9);
        assert_eq!((c9.r, c9.g, c9.b), (0x70, 0xAD, 0x47));
    }

    #[test]
    fn test_theme_index_mapping() {
        let pal = ThemePalette::default();
        // theme 0 -> lt1, theme 1 -> dk1
        let lt1 = pal.get(0);
        assert_eq!(lt1.r, 255); // white
        let dk1 = pal.get(1);
        assert_eq!(dk1.r, 0); // black
    }
}
