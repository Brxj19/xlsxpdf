use std::collections::HashMap;
use xlsxpdf_core::error::Result;
use xlsxpdf_xml::{XmlEvent, XmlReader};

#[derive(Debug, Clone)]
pub struct SheetGeometry {
    pub default_row_height: f64,
    pub default_col_width: f64,
    pub col_widths: Vec<ColInfo>,
    pub row_heights: HashMap<u32, RowInfo>,
}

#[derive(Debug, Clone)]
pub struct ColInfo {
    pub min: u16,
    pub max: u16,
    pub width: f64,
    pub custom_width: bool,
    pub hidden: bool,
}

#[derive(Debug, Clone)]
pub struct RowInfo {
    pub height: f64,
    pub custom_height: bool,
    pub hidden: bool,
}

impl Default for SheetGeometry {
    fn default() -> Self {
        Self {
            default_row_height: 15.0,
            default_col_width: 8.43,
            col_widths: Vec::new(),
            row_heights: HashMap::new(),
        }
    }
}

impl SheetGeometry {
    pub fn parse(xml_data: &[u8]) -> Result<Self> {
        let mut reader = XmlReader::new(xml_data);
        let mut geom = SheetGeometry::default();

        loop {
            match reader.next_event()? {
                XmlEvent::StartElement { local_name, attrs } => match local_name.as_str() {
                    "sheetFormatPr" => {
                        for (key, val) in &attrs {
                            match key.as_str() {
                                "defaultRowHeight" => {
                                    if let Ok(h) = val.parse::<f64>() {
                                        geom.default_row_height = h;
                                    }
                                }
                                "defaultColWidth" => {
                                    if let Ok(w) = val.parse::<f64>() {
                                        geom.default_col_width = w;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    "col" => {
                        let mut info = ColInfo {
                            min: 1,
                            max: 1,
                            width: geom.default_col_width,
                            custom_width: false,
                            hidden: false,
                        };
                        for (key, val) in &attrs {
                            match key.as_str() {
                                "min" => {
                                    info.min = val.parse().unwrap_or(1);
                                }
                                "max" => {
                                    info.max = val.parse().unwrap_or(1);
                                }
                                "width" => {
                                    if let Ok(w) = val.parse::<f64>() {
                                        info.width = w;
                                    }
                                }
                                "customWidth" => {
                                    info.custom_width = val == "1" || val == "true";
                                }
                                "hidden" => {
                                    info.hidden = val == "1" || val == "true";
                                }
                                _ => {}
                            }
                        }
                        geom.col_widths.push(info);
                    }
                    "row" => {
                        let mut row_num: u32 = 0;
                        let mut info = RowInfo {
                            height: geom.default_row_height,
                            custom_height: false,
                            hidden: false,
                        };
                        for (key, val) in &attrs {
                            match key.as_str() {
                                "r" => {
                                    row_num = val.parse().unwrap_or(0);
                                }
                                "ht" => {
                                    if let Ok(h) = val.parse::<f64>() {
                                        info.height = h;
                                    }
                                }
                                "customHeight" => {
                                    info.custom_height = val == "1" || val == "true";
                                }
                                "hidden" => {
                                    info.hidden = val == "1" || val == "true";
                                }
                                _ => {}
                            }
                        }
                        if row_num > 0 && (info.custom_height || info.hidden) {
                            geom.row_heights.insert(row_num - 1, info);
                        }
                    }
                    "sheetData" => {}
                    _ => {}
                },
                XmlEvent::EndElement { local_name } => {
                    if local_name == "sheetData" {
                        break;
                    }
                }
                XmlEvent::Text(_) => {}
                XmlEvent::Eof => break,
            }
        }

        Ok(geom)
    }

    pub fn col_width_chars(&self, col: u16) -> f64 {
        let col_1based = col + 1;
        for info in &self.col_widths {
            if col_1based >= info.min && col_1based <= info.max {
                if info.hidden {
                    return 0.0;
                }
                return info.width;
            }
        }
        self.default_col_width
    }

    pub fn row_height_pt(&self, row: u32) -> f64 {
        if let Some(info) = self.row_heights.get(&row) {
            if info.hidden {
                return 0.0;
            }
            return info.height;
        }
        self.default_row_height
    }
}

/// Excel column width (in character units) to points.
/// Uses the ECMA-376 pixel formula:
///   MDW_px = max_digit_width_pt * 96/72
///   px = trunc(((256 * w + trunc(128/MDW_px)) / 256) * MDW_px)
///   pt = px * 0.75
pub fn excel_col_width_to_pt(width_chars: f64, max_digit_width_pt: f64) -> f64 {
    if width_chars <= 0.0 {
        return 0.0;
    }
    let mdw_px = max_digit_width_pt * 96.0 / 72.0;
    if mdw_px < 1.0 {
        return width_chars * max_digit_width_pt;
    }
    let px = (((256.0 * width_chars + (128.0 / mdw_px).floor()) / 256.0) * mdw_px).floor();
    px * 0.75
}

/// Row height in Excel is already in points.
pub fn excel_row_height_to_pt(height: f64) -> f64 {
    height
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_geometry() {
        let xml = br#"<?xml version="1.0"?>
<worksheet>
  <sheetFormatPr defaultRowHeight="14.5" defaultColWidth="9.14"/>
  <cols>
    <col min="1" max="1" width="20.5" customWidth="1"/>
    <col min="2" max="5" width="12.0" customWidth="1"/>
  </cols>
  <sheetData>
    <row r="1" ht="20.25" customHeight="1">
      <c r="A1"><v>1</v></c>
    </row>
    <row r="2">
      <c r="A2"><v>2</v></c>
    </row>
  </sheetData>
</worksheet>"#;

        let geom = SheetGeometry::parse(xml).unwrap();
        assert!((geom.default_row_height - 14.5).abs() < 0.01);
        assert!((geom.default_col_width - 9.14).abs() < 0.01);
        assert_eq!(geom.col_widths.len(), 2);
        assert!((geom.col_width_chars(0) - 20.5).abs() < 0.01);
        assert!((geom.col_width_chars(1) - 12.0).abs() < 0.01);
        assert!((geom.col_width_chars(4) - 12.0).abs() < 0.01);
        assert!((geom.col_width_chars(5) - 9.14).abs() < 0.01);
        assert!(geom.row_heights.contains_key(&0));
        assert!((geom.row_height_pt(0) - 20.25).abs() < 0.01);
        assert!((geom.row_height_pt(1) - 14.5).abs() < 0.01);
    }

    #[test]
    fn test_col_width_conversion() {
        let digit_w = 3.45; // Calibri 11pt '0' width in points
        let pt = excel_col_width_to_pt(8.43, digit_w);
        // MDW_px = 3.45 * 96/72 = 4.6
        // px = floor(((256*8.43 + floor(128/4.6)) / 256) * 4.6)
        //    = floor(((2158.08 + 27) / 256) * 4.6)
        //    = floor(8.535 * 4.6) = floor(39.26) = 39
        // pt = 39 * 0.75 = 29.25
        assert!((pt - 29.25).abs() < 0.01, "got {pt}");
    }

    #[test]
    fn test_col_width_zero() {
        assert_eq!(excel_col_width_to_pt(0.0, 3.45), 0.0);
        assert_eq!(excel_col_width_to_pt(-1.0, 3.45), 0.0);
    }
}
