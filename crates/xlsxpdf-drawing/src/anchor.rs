use xlsxpdf_core::error::Result;
use xlsxpdf_xml::{XmlEvent, XmlReader};

const EMU_PER_PT: f64 = 12700.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorType {
    TwoCell,
    OneCell,
    Absolute,
}

#[derive(Debug, Clone, Default)]
pub struct AnchorMarker {
    pub col: u16,
    pub col_off_emu: i64,
    pub row: u32,
    pub row_off_emu: i64,
}

impl AnchorMarker {
    pub fn col_off_pt(&self) -> f64 {
        self.col_off_emu as f64 / EMU_PER_PT
    }

    pub fn row_off_pt(&self) -> f64 {
        self.row_off_emu as f64 / EMU_PER_PT
    }
}

#[derive(Debug, Clone)]
pub struct DrawingAnchor {
    pub anchor_type: AnchorType,
    pub from: AnchorMarker,
    pub to: Option<AnchorMarker>,
    pub ext_cx_emu: i64,
    pub ext_cy_emu: i64,
    pub image_rel_id: Option<String>,
    pub rotation_degrees: f64,
}

impl DrawingAnchor {
    pub fn ext_width_pt(&self) -> f64 {
        self.ext_cx_emu as f64 / EMU_PER_PT
    }

    pub fn ext_height_pt(&self) -> f64 {
        self.ext_cy_emu as f64 / EMU_PER_PT
    }

    pub fn resolve_rect(
        &self,
        col_positions: &[f64],
        row_positions: &[f64],
    ) -> (f64, f64, f64, f64) {
        let x0 = col_pos(col_positions, self.from.col) + self.from.col_off_pt();
        let y0 = row_pos(row_positions, self.from.row) + self.from.row_off_pt();

        match self.anchor_type {
            AnchorType::TwoCell => {
                if let Some(to) = &self.to {
                    let x1 = col_pos(col_positions, to.col) + to.col_off_pt();
                    let y1 = row_pos(row_positions, to.row) + to.row_off_pt();
                    (x0, y0, x1 - x0, y1 - y0)
                } else {
                    (x0, y0, self.ext_width_pt(), self.ext_height_pt())
                }
            }
            AnchorType::OneCell | AnchorType::Absolute => {
                (x0, y0, self.ext_width_pt(), self.ext_height_pt())
            }
        }
    }
}

fn col_pos(positions: &[f64], col: u16) -> f64 {
    let idx = col as usize;
    if idx < positions.len() {
        positions[idx]
    } else if positions.len() >= 2 {
        let last = positions[positions.len() - 1];
        let prev = positions[positions.len() - 2];
        let step = last - prev;
        last + step * (idx - (positions.len() - 1)) as f64
    } else if let Some(&last) = positions.last() {
        last
    } else {
        0.0
    }
}

fn row_pos(positions: &[f64], row: u32) -> f64 {
    let idx = row as usize;
    if idx < positions.len() {
        positions[idx]
    } else if positions.len() >= 2 {
        let last = positions[positions.len() - 1];
        let prev = positions[positions.len() - 2];
        let step = last - prev;
        last + step * (idx - (positions.len() - 1)) as f64
    } else if let Some(&last) = positions.last() {
        last
    } else {
        0.0
    }
}

#[derive(Debug, Clone)]
pub struct DrawingInfo {
    pub anchors: Vec<DrawingAnchor>,
}

pub fn parse_drawings(xml_data: &[u8]) -> Result<DrawingInfo> {
    let mut reader = XmlReader::new(xml_data);
    let mut anchors = Vec::new();

    loop {
        match reader.next_event()? {
            XmlEvent::StartElement { local_name, .. } => match local_name.as_str() {
                "twoCellAnchor" => {
                    if let Some(a) = parse_two_cell_anchor(&mut reader)? {
                        anchors.push(a);
                    }
                }
                "oneCellAnchor" => {
                    if let Some(a) = parse_one_cell_anchor(&mut reader)? {
                        anchors.push(a);
                    }
                }
                "absoluteAnchor" => {
                    if let Some(a) = parse_absolute_anchor(&mut reader)? {
                        anchors.push(a);
                    }
                }
                _ => {}
            },
            XmlEvent::EndElement { .. } => {}
            XmlEvent::Text(_) => {}
            XmlEvent::Eof => break,
        }
    }

    Ok(DrawingInfo { anchors })
}

fn parse_marker(reader: &mut XmlReader, end_tag: &str) -> Result<AnchorMarker> {
    let mut marker = AnchorMarker::default();
    let mut current_elem = String::new();

    loop {
        match reader.next_event()? {
            XmlEvent::StartElement { local_name, .. } => {
                current_elem = local_name;
            }
            XmlEvent::Text(text) => match current_elem.as_str() {
                "col" => {
                    marker.col = text.trim().parse().unwrap_or(0);
                }
                "colOff" => {
                    marker.col_off_emu = text.trim().parse().unwrap_or(0);
                }
                "row" => {
                    marker.row = text.trim().parse().unwrap_or(0);
                }
                "rowOff" => {
                    marker.row_off_emu = text.trim().parse().unwrap_or(0);
                }
                _ => {}
            },
            XmlEvent::EndElement { local_name } => {
                if local_name == end_tag {
                    break;
                }
                current_elem.clear();
            }
            XmlEvent::Eof => break,
        }
    }

    Ok(marker)
}

fn parse_two_cell_anchor(reader: &mut XmlReader) -> Result<Option<DrawingAnchor>> {
    let mut from = AnchorMarker::default();
    let mut to = AnchorMarker::default();
    let mut image_rel_id = None;
    let mut ext_cx: i64 = 0;
    let mut ext_cy: i64 = 0;

    loop {
        match reader.next_event()? {
            XmlEvent::StartElement { local_name, attrs } => match local_name.as_str() {
                "from" => {
                    from = parse_marker(reader, "from")?;
                }
                "to" => {
                    to = parse_marker(reader, "to")?;
                }
                "blipFill" | "blip" => {
                    for (key, val) in &attrs {
                        if key == "embed" || key == "r:embed" {
                            image_rel_id = Some(val.clone());
                        }
                    }
                }
                "ext" => {
                    for (key, val) in &attrs {
                        match key.as_str() {
                            "cx" => ext_cx = val.parse().unwrap_or(0),
                            "cy" => ext_cy = val.parse().unwrap_or(0),
                            _ => {}
                        }
                    }
                }
                _ => {
                    for (key, val) in &attrs {
                        if key == "embed" || key == "r:embed" {
                            image_rel_id = Some(val.clone());
                        }
                    }
                }
            },
            XmlEvent::EndElement { local_name } => {
                if local_name == "twoCellAnchor" {
                    break;
                }
            }
            XmlEvent::Text(_) => {}
            XmlEvent::Eof => break,
        }
    }

    Ok(Some(DrawingAnchor {
        anchor_type: AnchorType::TwoCell,
        from,
        to: Some(to),
        ext_cx_emu: ext_cx,
        ext_cy_emu: ext_cy,
        image_rel_id,
        rotation_degrees: 0.0,
    }))
}

fn parse_one_cell_anchor(reader: &mut XmlReader) -> Result<Option<DrawingAnchor>> {
    let mut from = AnchorMarker::default();
    let mut image_rel_id = None;
    let mut ext_cx: i64 = 0;
    let mut ext_cy: i64 = 0;

    loop {
        match reader.next_event()? {
            XmlEvent::StartElement { local_name, attrs } => match local_name.as_str() {
                "from" => {
                    from = parse_marker(reader, "from")?;
                }
                "ext" => {
                    for (key, val) in &attrs {
                        match key.as_str() {
                            "cx" => ext_cx = val.parse().unwrap_or(0),
                            "cy" => ext_cy = val.parse().unwrap_or(0),
                            _ => {}
                        }
                    }
                }
                _ => {
                    for (key, val) in &attrs {
                        if key == "embed" || key == "r:embed" {
                            image_rel_id = Some(val.clone());
                        }
                    }
                }
            },
            XmlEvent::EndElement { local_name } => {
                if local_name == "oneCellAnchor" {
                    break;
                }
            }
            XmlEvent::Text(_) => {}
            XmlEvent::Eof => break,
        }
    }

    Ok(Some(DrawingAnchor {
        anchor_type: AnchorType::OneCell,
        from,
        to: None,
        ext_cx_emu: ext_cx,
        ext_cy_emu: ext_cy,
        image_rel_id,
        rotation_degrees: 0.0,
    }))
}

fn parse_absolute_anchor(reader: &mut XmlReader) -> Result<Option<DrawingAnchor>> {
    let mut image_rel_id = None;
    let mut pos_x: i64 = 0;
    let mut pos_y: i64 = 0;
    let mut ext_cx: i64 = 0;
    let mut ext_cy: i64 = 0;

    loop {
        match reader.next_event()? {
            XmlEvent::StartElement { local_name, attrs } => match local_name.as_str() {
                "pos" => {
                    for (key, val) in &attrs {
                        match key.as_str() {
                            "x" => pos_x = val.parse().unwrap_or(0),
                            "y" => pos_y = val.parse().unwrap_or(0),
                            _ => {}
                        }
                    }
                }
                "ext" => {
                    for (key, val) in &attrs {
                        match key.as_str() {
                            "cx" => ext_cx = val.parse().unwrap_or(0),
                            "cy" => ext_cy = val.parse().unwrap_or(0),
                            _ => {}
                        }
                    }
                }
                _ => {
                    for (key, val) in &attrs {
                        if key == "embed" || key == "r:embed" {
                            image_rel_id = Some(val.clone());
                        }
                    }
                }
            },
            XmlEvent::EndElement { local_name } => {
                if local_name == "absoluteAnchor" {
                    break;
                }
            }
            XmlEvent::Text(_) => {}
            XmlEvent::Eof => break,
        }
    }

    Ok(Some(DrawingAnchor {
        anchor_type: AnchorType::Absolute,
        from: AnchorMarker {
            col: 0,
            col_off_emu: pos_x,
            row: 0,
            row_off_emu: pos_y,
        },
        to: None,
        ext_cx_emu: ext_cx,
        ext_cy_emu: ext_cy,
        image_rel_id,
        rotation_degrees: 0.0,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_two_cell_anchor() {
        let xml = br#"<?xml version="1.0"?>
<xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing"
          xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
          xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <xdr:twoCellAnchor>
    <xdr:from>
      <xdr:col>1</xdr:col>
      <xdr:colOff>190500</xdr:colOff>
      <xdr:row>2</xdr:row>
      <xdr:rowOff>63500</xdr:rowOff>
    </xdr:from>
    <xdr:to>
      <xdr:col>5</xdr:col>
      <xdr:colOff>0</xdr:colOff>
      <xdr:row>10</xdr:row>
      <xdr:rowOff>127000</xdr:rowOff>
    </xdr:to>
    <xdr:pic>
      <xdr:blipFill>
        <a:blip r:embed="rId1"/>
      </xdr:blipFill>
    </xdr:pic>
  </xdr:twoCellAnchor>
</xdr:wsDr>"#;

        let info = parse_drawings(xml).unwrap();
        assert_eq!(info.anchors.len(), 1);
        let a = &info.anchors[0];
        assert_eq!(a.anchor_type, AnchorType::TwoCell);
        assert_eq!(a.from.col, 1);
        assert_eq!(a.from.col_off_emu, 190500);
        assert_eq!(a.from.row, 2);
        assert_eq!(a.to.as_ref().unwrap().col, 5);
        assert_eq!(a.to.as_ref().unwrap().row, 10);
        assert_eq!(a.image_rel_id.as_deref(), Some("rId1"));
        assert!((a.from.col_off_pt() - 15.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_one_cell_anchor() {
        let xml = br#"<?xml version="1.0"?>
<wsDr>
  <oneCellAnchor>
    <from>
      <col>0</col>
      <colOff>0</colOff>
      <row>0</row>
      <rowOff>0</rowOff>
    </from>
    <ext cx="2540000" cy="1270000"/>
    <pic>
      <blipFill>
        <blip embed="rId2"/>
      </blipFill>
    </pic>
  </oneCellAnchor>
</wsDr>"#;

        let info = parse_drawings(xml).unwrap();
        assert_eq!(info.anchors.len(), 1);
        let a = &info.anchors[0];
        assert_eq!(a.anchor_type, AnchorType::OneCell);
        assert!((a.ext_width_pt() - 200.0).abs() < 0.01);
        assert!((a.ext_height_pt() - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_resolve_rect() {
        let anchor = DrawingAnchor {
            anchor_type: AnchorType::TwoCell,
            from: AnchorMarker {
                col: 1,
                col_off_emu: 0,
                row: 0,
                row_off_emu: 0,
            },
            to: Some(AnchorMarker {
                col: 3,
                col_off_emu: 0,
                row: 2,
                row_off_emu: 0,
            }),
            ext_cx_emu: 0,
            ext_cy_emu: 0,
            image_rel_id: None,
            rotation_degrees: 0.0,
        };

        // col positions: [0, 64, 128, 192, 256]
        // row positions: [0, 15, 30, 45]
        let col_pos = vec![0.0, 64.0, 128.0, 192.0, 256.0];
        let row_pos = vec![0.0, 15.0, 30.0, 45.0];

        let (x, y, w, h) = anchor.resolve_rect(&col_pos, &row_pos);
        assert!((x - 64.0).abs() < 0.01);
        assert!((y - 0.0).abs() < 0.01);
        assert!((w - 128.0).abs() < 0.01);
        assert!((h - 30.0).abs() < 0.01);
    }

    // M14: Hardening tests

    #[test]
    fn test_empty_drawing_xml() {
        let xml = br#"<?xml version="1.0"?><wsDr></wsDr>"#;
        let info = parse_drawings(xml).unwrap();
        assert!(info.anchors.is_empty());
    }

    #[test]
    fn test_empty_input() {
        let info = parse_drawings(b"");
        assert!(info.is_ok());
        assert!(info.unwrap().anchors.is_empty());
    }

    #[test]
    fn test_garbage_input() {
        let info = parse_drawings(b"not xml at all");
        assert!(info.is_ok());
        assert!(info.unwrap().anchors.is_empty());
    }

    #[test]
    fn test_anchor_with_no_image() {
        let xml = br#"<?xml version="1.0"?>
<wsDr>
  <twoCellAnchor>
    <from><col>0</col><colOff>0</colOff><row>0</row><rowOff>0</rowOff></from>
    <to><col>1</col><colOff>0</colOff><row>1</row><rowOff>0</rowOff></to>
    <sp/>
  </twoCellAnchor>
</wsDr>"#;
        let info = parse_drawings(xml).unwrap();
        assert_eq!(info.anchors.len(), 1);
        assert!(info.anchors[0].image_rel_id.is_none());
    }

    #[test]
    fn test_resolve_rect_out_of_bounds() {
        let anchor = DrawingAnchor {
            anchor_type: AnchorType::TwoCell,
            from: AnchorMarker { col: 100, col_off_emu: 0, row: 100, row_off_emu: 0 },
            to: Some(AnchorMarker { col: 200, col_off_emu: 0, row: 200, row_off_emu: 0 }),
            ext_cx_emu: 0, ext_cy_emu: 0,
            image_rel_id: None, rotation_degrees: 0.0,
        };
        let col_pos = vec![0.0, 64.0];
        let row_pos = vec![0.0, 15.0];
        let (x, y, w, h) = anchor.resolve_rect(&col_pos, &row_pos);
        assert!(w.is_finite() && h.is_finite());
        let _ = (x, y);
    }

    #[test]
    fn test_negative_emu_offsets() {
        let anchor = DrawingAnchor {
            anchor_type: AnchorType::OneCell,
            from: AnchorMarker { col: 0, col_off_emu: -12700, row: 0, row_off_emu: -12700 },
            to: None,
            ext_cx_emu: 254000, ext_cy_emu: 127000,
            image_rel_id: None, rotation_degrees: 0.0,
        };
        let (x, y, w, h) = anchor.resolve_rect(&[0.0, 64.0], &[0.0, 15.0]);
        assert!(x < 0.0);
        assert!(y < 0.0);
        assert!(w > 0.0 && h > 0.0);
    }
}
