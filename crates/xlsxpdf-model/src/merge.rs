use std::collections::HashSet;
use xlsxpdf_core::coord::parse_cell_ref;
use xlsxpdf_core::error::Result;
use xlsxpdf_xml::{XmlEvent, XmlReader};

#[derive(Debug, Clone)]
pub struct MergeRect {
    pub row0: u32,
    pub row1: u32,
    pub col0: u16,
    pub col1: u16,
}

impl MergeRect {
    pub fn contains(&self, row: u32, col: u16) -> bool {
        row >= self.row0 && row <= self.row1 && col >= self.col0 && col <= self.col1
    }

    pub fn is_top_left(&self, row: u32, col: u16) -> bool {
        row == self.row0 && col == self.col0
    }

    pub fn col_span(&self) -> u16 {
        self.col1 - self.col0 + 1
    }

    pub fn row_span(&self) -> u32 {
        self.row1 - self.row0 + 1
    }
}

#[derive(Debug, Clone, Default)]
pub struct MergeMap {
    pub rects: Vec<MergeRect>,
    hidden: HashSet<(u32, u16)>,
}

impl MergeMap {
    pub fn parse(xml_data: &[u8]) -> Result<Self> {
        let mut reader = XmlReader::new(xml_data);
        let mut rects = Vec::new();
        let mut in_merge_cells = false;

        loop {
            match reader.next_event()? {
                XmlEvent::StartElement { local_name, attrs } => match local_name.as_str() {
                    "mergeCells" => {
                        in_merge_cells = true;
                    }
                    "mergeCell" if in_merge_cells => {
                        for (key, val) in &attrs {
                            if key == "ref" {
                                if let Some(rect) = parse_merge_ref(val) {
                                    rects.push(rect);
                                }
                            }
                        }
                    }
                    _ => {}
                },
                XmlEvent::EndElement { local_name } => {
                    if local_name == "mergeCells" {
                        break;
                    }
                }
                XmlEvent::Text(_) => {}
                XmlEvent::Eof => break,
            }
        }

        let mut hidden = HashSet::new();
        for rect in &rects {
            for r in rect.row0..=rect.row1 {
                for c in rect.col0..=rect.col1 {
                    if r != rect.row0 || c != rect.col0 {
                        hidden.insert((r, c));
                    }
                }
            }
        }

        Ok(Self { rects, hidden })
    }

    pub fn is_hidden(&self, row: u32, col: u16) -> bool {
        self.hidden.contains(&(row, col))
    }

    pub fn get_merge(&self, row: u32, col: u16) -> Option<&MergeRect> {
        self.rects
            .iter()
            .find(|r| r.is_top_left(row, col))
    }

    pub fn is_empty(&self) -> bool {
        self.rects.is_empty()
    }
}

fn parse_merge_ref(ref_str: &str) -> Option<MergeRect> {
    let colon = ref_str.find(':')?;
    let from_str = &ref_str[..colon];
    let to_str = &ref_str[colon + 1..];

    let (from, _) = parse_cell_ref(from_str.as_bytes())?;
    let (to, _) = parse_cell_ref(to_str.as_bytes())?;

    Some(MergeRect {
        row0: from.row,
        row1: to.row,
        col0: from.col,
        col1: to.col,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_merge_cells() {
        let xml = br#"<?xml version="1.0"?>
<worksheet>
  <sheetData/>
  <mergeCells count="2">
    <mergeCell ref="A1:C1"/>
    <mergeCell ref="B3:B5"/>
  </mergeCells>
</worksheet>"#;

        let map = MergeMap::parse(xml).unwrap();
        assert_eq!(map.rects.len(), 2);

        let r0 = &map.rects[0];
        assert_eq!((r0.row0, r0.col0, r0.row1, r0.col1), (0, 0, 0, 2));
        assert_eq!(r0.col_span(), 3);
        assert_eq!(r0.row_span(), 1);

        let r1 = &map.rects[1];
        assert_eq!((r1.row0, r1.col0, r1.row1, r1.col1), (2, 1, 4, 1));

        assert!(!map.is_hidden(0, 0)); // top-left of merge
        assert!(map.is_hidden(0, 1)); // hidden by first merge
        assert!(map.is_hidden(0, 2)); // hidden by first merge
        assert!(!map.is_hidden(2, 1)); // top-left of second merge
        assert!(map.is_hidden(3, 1)); // hidden by second merge

        assert!(map.get_merge(0, 0).is_some());
        assert!(map.get_merge(0, 1).is_none());
    }

    #[test]
    fn test_empty_merge() {
        let xml = br#"<?xml version="1.0"?>
<worksheet><sheetData/></worksheet>"#;
        let map = MergeMap::parse(xml).unwrap();
        assert!(map.is_empty());
    }

    // M14: Hardening tests

    #[test]
    fn test_malformed_merge_ref() {
        let xml = br#"<?xml version="1.0"?>
<worksheet><sheetData/>
  <mergeCells count="3">
    <mergeCell ref="INVALID"/>
    <mergeCell ref=""/>
    <mergeCell ref="A1:"/>
  </mergeCells>
</worksheet>"#;
        let map = MergeMap::parse(xml).unwrap();
        assert!(map.is_empty());
    }

    #[test]
    fn test_no_ref_attribute() {
        let xml = br#"<?xml version="1.0"?>
<worksheet><sheetData/>
  <mergeCells count="1">
    <mergeCell/>
  </mergeCells>
</worksheet>"#;
        let map = MergeMap::parse(xml).unwrap();
        assert!(map.is_empty());
    }

    #[test]
    fn test_garbage_xml() {
        let map = MergeMap::parse(b"not xml at all {{{");
        assert!(map.is_ok());
        assert!(map.unwrap().is_empty());
    }

    #[test]
    fn test_empty_input() {
        let map = MergeMap::parse(b"");
        assert!(map.is_ok());
        assert!(map.unwrap().is_empty());
    }
}
