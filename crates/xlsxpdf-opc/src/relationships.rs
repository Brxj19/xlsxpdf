use xlsxpdf_core::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationshipType {
    Worksheet,
    SharedStrings,
    Styles,
    Theme,
    Drawing,
    Image,
    Chart,
    Workbook,
    CoreProperties,
    ExtendedProperties,
    Unknown,
}

impl RelationshipType {
    pub fn from_uri(uri: &str) -> Self {
        if uri.ends_with("/worksheet") {
            Self::Worksheet
        } else if uri.ends_with("/sharedStrings") {
            Self::SharedStrings
        } else if uri.ends_with("/styles") {
            Self::Styles
        } else if uri.ends_with("/theme") {
            Self::Theme
        } else if uri.ends_with("/drawing") {
            Self::Drawing
        } else if uri.ends_with("/image") {
            Self::Image
        } else if uri.ends_with("/chart") || uri.ends_with("/chartsheet") {
            Self::Chart
        } else if uri.ends_with("/officeDocument") {
            Self::Workbook
        } else if uri.ends_with("/core-properties") || uri.ends_with("/metadata/core-properties") {
            Self::CoreProperties
        } else if uri.ends_with("/extended-properties") {
            Self::ExtendedProperties
        } else {
            Self::Unknown
        }
    }
}

#[derive(Debug, Clone)]
pub struct Relationship {
    pub id: String,
    pub rel_type: RelationshipType,
    pub target: String,
    pub type_uri: String,
}

#[derive(Debug, Default)]
pub struct Relationships {
    pub rels: Vec<Relationship>,
}

impl Relationships {
    pub fn parse(xml: &[u8]) -> Result<Self> {
        let xml_str = std::str::from_utf8(xml)
            .map_err(|e| Error::Xml(format!("invalid UTF-8 in relationships: {e}")))?;

        let mut rels = Vec::new();
        let mut pos = 0;
        let bytes = xml_str.as_bytes();

        while let Some(start) = find_tag_start(bytes, pos, b"<Relationship ") {
            let end = find_byte(bytes, start, b'>')
                .ok_or_else(|| Error::Xml("unclosed Relationship tag".into()))?;
            let tag = &xml_str[start..=end];

            let id = extract_attr(tag, "Id").unwrap_or_default();
            let type_uri = extract_attr(tag, "Type").unwrap_or_default();
            let target = extract_attr(tag, "Target").unwrap_or_default();

            let rel_type = RelationshipType::from_uri(&type_uri);

            rels.push(Relationship {
                id,
                rel_type,
                target,
                type_uri,
            });

            pos = end + 1;
        }

        Ok(Self { rels })
    }

    pub fn find_by_type(&self, rel_type: RelationshipType) -> Vec<&Relationship> {
        self.rels.iter().filter(|r| r.rel_type == rel_type).collect()
    }

    pub fn find_by_id(&self, id: &str) -> Option<&Relationship> {
        self.rels.iter().find(|r| r.id == id)
    }
}

fn find_tag_start(bytes: &[u8], from: usize, tag: &[u8]) -> Option<usize> {
    bytes[from..]
        .windows(tag.len())
        .position(|w| w == tag)
        .map(|p| p + from)
}

fn find_byte(bytes: &[u8], from: usize, byte: u8) -> Option<usize> {
    bytes[from..].iter().position(|&b| b == byte).map(|p| p + from)
}

fn extract_attr(tag: &str, name: &str) -> Option<String> {
    let pattern = format!("{name}=\"");
    let start = tag.find(&pattern)? + pattern.len();
    let end = tag[start..].find('"')? + start;
    Some(unescape_xml(&tag[start..end]))
}

fn unescape_xml(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}
