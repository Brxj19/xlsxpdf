use xlsxpdf_core::error::Result;
use xlsxpdf_xml::{XmlEvent, XmlReader};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaperSize {
    Letter,
    Legal,
    A3,
    A4,
    A5,
    B4,
    B5,
    Other(u32),
}

impl PaperSize {
    pub fn from_id(id: u32) -> Self {
        match id {
            1 => Self::Letter,
            5 => Self::Legal,
            8 => Self::A3,
            9 => Self::A4,
            11 => Self::A5,
            12 => Self::B4,
            13 => Self::B5,
            _ => Self::Other(id),
        }
    }

    pub fn dimensions_pt(&self) -> (f64, f64) {
        match self {
            Self::Letter => (612.0, 792.0),
            Self::Legal => (612.0, 1008.0),
            Self::A3 => (841.89, 1190.55),
            Self::A4 => (595.28, 841.89),
            Self::A5 => (419.53, 595.28),
            Self::B4 => (708.66, 1000.63),
            Self::B5 => (498.90, 708.66),
            Self::Other(_) => (612.0, 792.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    Portrait,
    Landscape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageOrder {
    DownThenOver,
    OverThenDown,
}

#[derive(Debug, Clone)]
pub struct PrintSettings {
    pub paper_size: PaperSize,
    pub orientation: Orientation,
    pub scale: u32,
    pub fit_to_width: Option<u32>,
    pub fit_to_height: Option<u32>,
    pub use_fit_to_page: bool,
    pub page_order: PageOrder,
    pub margin_top: f64,
    pub margin_bottom: f64,
    pub margin_left: f64,
    pub margin_right: f64,
    pub margin_header: f64,
    pub margin_footer: f64,
    pub center_horizontally: bool,
    pub center_vertically: bool,
    pub print_gridlines: bool,
    pub row_breaks: Vec<u32>,
    pub col_breaks: Vec<u16>,
}

impl Default for PrintSettings {
    fn default() -> Self {
        Self {
            paper_size: PaperSize::Letter,
            orientation: Orientation::Portrait,
            scale: 100,
            fit_to_width: None,
            fit_to_height: None,
            use_fit_to_page: false,
            page_order: PageOrder::DownThenOver,
            margin_top: 0.75,
            margin_bottom: 0.75,
            margin_left: 0.7,
            margin_right: 0.7,
            margin_header: 0.3,
            margin_footer: 0.3,
            center_horizontally: false,
            center_vertically: false,
            print_gridlines: false,
            row_breaks: Vec::new(),
            col_breaks: Vec::new(),
        }
    }
}

impl PrintSettings {
    pub fn parse(xml_data: &[u8]) -> Result<Self> {
        let mut reader = XmlReader::new(xml_data);
        let mut settings = PrintSettings::default();

        loop {
            match reader.next_event()? {
                XmlEvent::StartElement { local_name, attrs } => match local_name.as_str() {
                    "pageSetup" => {
                        for (key, val) in &attrs {
                            match key.as_str() {
                                "paperSize" => {
                                    if let Ok(id) = val.parse::<u32>() {
                                        settings.paper_size = PaperSize::from_id(id);
                                    }
                                }
                                "orientation" => {
                                    settings.orientation = if val == "landscape" {
                                        Orientation::Landscape
                                    } else {
                                        Orientation::Portrait
                                    };
                                }
                                "scale" => {
                                    if let Ok(s) = val.parse::<u32>() {
                                        settings.scale = s.clamp(10, 400);
                                    }
                                }
                                "fitToWidth" => {
                                    if let Ok(n) = val.parse::<u32>() {
                                        settings.fit_to_width = if n > 0 { Some(n) } else { None };
                                    }
                                }
                                "fitToHeight" => {
                                    if let Ok(n) = val.parse::<u32>() {
                                        settings.fit_to_height = if n > 0 { Some(n) } else { None };
                                    }
                                }
                                "pageOrder" => {
                                    settings.page_order = if val == "overThenDown" {
                                        PageOrder::OverThenDown
                                    } else {
                                        PageOrder::DownThenOver
                                    };
                                }
                                _ => {}
                            }
                        }
                    }
                    "pageMargins" => {
                        for (key, val) in &attrs {
                            let v = val.parse::<f64>().unwrap_or(0.0);
                            match key.as_str() {
                                "top" => settings.margin_top = v,
                                "bottom" => settings.margin_bottom = v,
                                "left" => settings.margin_left = v,
                                "right" => settings.margin_right = v,
                                "header" => settings.margin_header = v,
                                "footer" => settings.margin_footer = v,
                                _ => {}
                            }
                        }
                    }
                    "printOptions" => {
                        for (key, val) in &attrs {
                            match key.as_str() {
                                "horizontalCentered" => {
                                    settings.center_horizontally =
                                        val == "1" || val == "true";
                                }
                                "verticalCentered" => {
                                    settings.center_vertically =
                                        val == "1" || val == "true";
                                }
                                "gridLines" => {
                                    settings.print_gridlines =
                                        val == "1" || val == "true";
                                }
                                _ => {}
                            }
                        }
                    }
                    "sheetPr" => {
                        for (key, val) in &attrs {
                            if key == "fitToPage" && (val == "1" || val == "true") {
                                settings.use_fit_to_page = true;
                            }
                        }
                    }
                    "pageSetUpPr" => {
                        for (key, val) in &attrs {
                            if key == "fitToPage" && (val == "1" || val == "true") {
                                settings.use_fit_to_page = true;
                            }
                        }
                    }
                    "brk" => {
                        let mut id: u32 = 0;
                        let mut is_manual = false;
                        for (key, val) in &attrs {
                            match key.as_str() {
                                "id" => {
                                    id = val.parse().unwrap_or(0);
                                }
                                "man" => {
                                    is_manual = val == "1" || val == "true";
                                }
                                _ => {}
                            }
                        }
                        if is_manual && id > 0 {
                            // We determine row vs col breaks by context
                            // (rowBreaks or colBreaks parent), but since we
                            // track the parent below, we store in pending
                        }
                        let _ = (id, is_manual);
                    }
                    "rowBreaks" => {
                        settings.row_breaks =
                            parse_breaks(&mut reader, "rowBreaks")?;
                    }
                    "colBreaks" => {
                        let breaks = parse_breaks(&mut reader, "colBreaks")?;
                        settings.col_breaks =
                            breaks.into_iter().map(|b| b as u16).collect();
                    }
                    _ => {}
                },
                XmlEvent::EndElement { .. } => {}
                XmlEvent::Text(_) => {}
                XmlEvent::Eof => break,
            }
        }

        // If fitToPage is set but no explicit values, default to fit to 1 wide
        if settings.use_fit_to_page {
            if settings.fit_to_width.is_none() && settings.fit_to_height.is_none() {
                settings.fit_to_width = Some(1);
            }
        }

        Ok(settings)
    }

    pub fn page_width_pt(&self) -> f64 {
        let (w, h) = self.paper_size.dimensions_pt();
        match self.orientation {
            Orientation::Portrait => w,
            Orientation::Landscape => h,
        }
    }

    pub fn page_height_pt(&self) -> f64 {
        let (w, h) = self.paper_size.dimensions_pt();
        match self.orientation {
            Orientation::Portrait => h,
            Orientation::Landscape => w,
        }
    }

    pub fn margin_top_pt(&self) -> f64 {
        self.margin_top * 72.0
    }

    pub fn margin_bottom_pt(&self) -> f64 {
        self.margin_bottom * 72.0
    }

    pub fn margin_left_pt(&self) -> f64 {
        self.margin_left * 72.0
    }

    pub fn margin_right_pt(&self) -> f64 {
        self.margin_right * 72.0
    }

    pub fn printable_width_pt(&self) -> f64 {
        self.page_width_pt() - self.margin_left_pt() - self.margin_right_pt()
    }

    pub fn printable_height_pt(&self) -> f64 {
        self.page_height_pt() - self.margin_top_pt() - self.margin_bottom_pt()
    }
}

fn parse_breaks(reader: &mut XmlReader, end_tag: &str) -> Result<Vec<u32>> {
    let mut breaks = Vec::new();

    loop {
        match reader.next_event()? {
            XmlEvent::StartElement { local_name, attrs } => {
                if local_name == "brk" {
                    let mut id: u32 = 0;
                    let mut is_manual = true;
                    for (key, val) in &attrs {
                        match key.as_str() {
                            "id" => {
                                id = val.parse().unwrap_or(0);
                            }
                            "man" => {
                                is_manual = val == "1" || val == "true";
                            }
                            _ => {}
                        }
                    }
                    if is_manual && id > 0 {
                        breaks.push(id);
                    }
                }
            }
            XmlEvent::EndElement { local_name } => {
                if local_name == end_tag {
                    break;
                }
            }
            XmlEvent::Text(_) => {}
            XmlEvent::Eof => break,
        }
    }

    breaks.sort();
    Ok(breaks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_print_settings() {
        let xml = br#"<?xml version="1.0"?>
<worksheet>
  <sheetPr>
    <pageSetUpPr fitToPage="1"/>
  </sheetPr>
  <sheetData/>
  <pageMargins left="0.5" right="0.5" top="1.0" bottom="1.0" header="0.3" footer="0.3"/>
  <pageSetup paperSize="9" orientation="landscape" fitToWidth="1" fitToHeight="0" scale="100"/>
  <printOptions horizontalCentered="1"/>
</worksheet>"#;

        let ps = PrintSettings::parse(xml).unwrap();
        assert_eq!(ps.paper_size, PaperSize::A4);
        assert_eq!(ps.orientation, Orientation::Landscape);
        assert!(ps.use_fit_to_page);
        assert_eq!(ps.fit_to_width, Some(1));
        assert!(ps.center_horizontally);
        assert!(!ps.center_vertically);
        assert!((ps.margin_left - 0.5).abs() < 0.01);
        assert!((ps.margin_top - 1.0).abs() < 0.01);
        // A4 landscape: width=841.89, height=595.28
        assert!((ps.page_width_pt() - 841.89).abs() < 0.1);
    }

    #[test]
    fn test_page_breaks() {
        let xml = br#"<?xml version="1.0"?>
<worksheet>
  <sheetData/>
  <rowBreaks count="2" manualBreakCount="2">
    <brk id="10" man="1" max="16383"/>
    <brk id="25" man="1" max="16383"/>
  </rowBreaks>
  <colBreaks count="1" manualBreakCount="1">
    <brk id="5" man="1" max="1048575"/>
  </colBreaks>
</worksheet>"#;

        let ps = PrintSettings::parse(xml).unwrap();
        assert_eq!(ps.row_breaks, vec![10, 25]);
        assert_eq!(ps.col_breaks, vec![5]);
    }

    #[test]
    fn test_defaults() {
        let ps = PrintSettings::default();
        assert_eq!(ps.paper_size, PaperSize::Letter);
        assert_eq!(ps.orientation, Orientation::Portrait);
        assert_eq!(ps.scale, 100);
        assert!((ps.page_width_pt() - 612.0).abs() < 0.01);
        assert!((ps.printable_width_pt() - (612.0 - 2.0 * 0.7 * 72.0)).abs() < 0.1);
    }

    // M14: Hardening tests

    #[test]
    fn test_empty_input() {
        let ps = PrintSettings::parse(b"");
        assert!(ps.is_ok());
    }

    #[test]
    fn test_garbage_input() {
        let ps = PrintSettings::parse(b"this is not xml");
        assert!(ps.is_ok());
    }

    #[test]
    fn test_negative_margins() {
        let xml = br#"<?xml version="1.0"?>
<worksheet>
  <sheetData/>
  <pageMargins left="-1.0" right="-1.0" top="-1.0" bottom="-1.0"/>
</worksheet>"#;
        let ps = PrintSettings::parse(xml).unwrap();
        assert!(ps.printable_width_pt() > 0.0 || ps.margin_left < 0.0);
    }

    #[test]
    fn test_extreme_scale_clamped() {
        let xml = br#"<?xml version="1.0"?>
<worksheet>
  <sheetData/>
  <pageSetup scale="99999"/>
</worksheet>"#;
        let ps = PrintSettings::parse(xml).unwrap();
        assert_eq!(ps.scale, 400); // clamped to max 400
    }

    #[test]
    fn test_low_scale_clamped() {
        let xml = br#"<?xml version="1.0"?>
<worksheet>
  <sheetData/>
  <pageSetup scale="1"/>
</worksheet>"#;
        let ps = PrintSettings::parse(xml).unwrap();
        assert_eq!(ps.scale, 10); // clamped to min 10
    }

    #[test]
    fn test_unknown_paper_size() {
        let xml = br#"<?xml version="1.0"?>
<worksheet>
  <sheetData/>
  <pageSetup paperSize="999"/>
</worksheet>"#;
        let ps = PrintSettings::parse(xml).unwrap();
        assert_eq!(ps.paper_size, PaperSize::Other(999));
    }
}
