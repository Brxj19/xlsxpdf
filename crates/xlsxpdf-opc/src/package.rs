use std::io::{Read, Seek};
use xlsxpdf_core::error::{Error, Result};

use crate::limits::PackageLimits;
use crate::relationships::{RelationshipType, Relationships};
use crate::zip_reader::ZipArchive;

#[derive(Debug, Clone)]
pub struct PartPath(pub String);

impl PartPath {
    pub fn resolve_relative(&self, target: &str) -> Self {
        if target.starts_with('/') {
            return Self(target[1..].to_string());
        }

        let base = if let Some(pos) = self.0.rfind('/') {
            &self.0[..pos + 1]
        } else {
            ""
        };

        let mut parts: Vec<&str> = base.split('/').filter(|s| !s.is_empty()).collect();
        for segment in target.split('/') {
            match segment {
                ".." => { parts.pop(); }
                "." | "" => {}
                s => parts.push(s),
            }
        }

        Self(parts.join("/"))
    }
}

pub struct OpcPackage<R> {
    pub archive: ZipArchive<R>,
    pub root_rels: Relationships,
    pub workbook_path: String,
    pub workbook_rels: Relationships,
    pub sheets: Vec<SheetInfo>,
    pub shared_strings_path: Option<String>,
    pub styles_path: Option<String>,
    pub theme_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SheetInfo {
    pub name: String,
    pub sheet_id: u32,
    pub rel_id: String,
    pub part_path: String,
    pub drawing_path: Option<String>,
    pub hidden: bool,
}

impl<R: Read + Seek> OpcPackage<R> {
    pub fn open(mut archive: ZipArchive<R>, limits: &PackageLimits) -> Result<Self> {
        let _ = limits;

        let root_rels_data = archive.read_entry("_rels/.rels")?;
        let root_rels = Relationships::parse(&root_rels_data)?;

        let workbook_rel = root_rels
            .find_by_type(RelationshipType::Workbook)
            .into_iter()
            .next()
            .ok_or_else(|| Error::InvalidStructure("no workbook relationship found".into()))?;

        let workbook_path = workbook_rel.target.trim_start_matches('/').to_string();

        let wb_rels_path = make_rels_path(&workbook_path);
        let wb_rels_data = archive.read_entry(&wb_rels_path)?;
        let workbook_rels = Relationships::parse(&wb_rels_data)?;

        let workbook_data = archive.read_entry(&workbook_path)?;
        let sheets = parse_workbook_sheets(&workbook_data, &workbook_rels, &workbook_path)?;

        let wb_part = PartPath(workbook_path.clone());

        let shared_strings_path = workbook_rels
            .find_by_type(RelationshipType::SharedStrings)
            .first()
            .map(|r| wb_part.resolve_relative(&r.target).0);

        let styles_path = workbook_rels
            .find_by_type(RelationshipType::Styles)
            .first()
            .map(|r| wb_part.resolve_relative(&r.target).0);

        let theme_path = workbook_rels
            .find_by_type(RelationshipType::Theme)
            .first()
            .map(|r| wb_part.resolve_relative(&r.target).0);

        Ok(Self {
            archive,
            root_rels,
            workbook_path,
            workbook_rels,
            sheets,
            shared_strings_path,
            styles_path,
            theme_path,
        })
    }

    pub fn read_part(&mut self, path: &str) -> Result<Vec<u8>> {
        self.archive.read_entry(path)
    }

    pub fn read_sheet_rels(&mut self, sheet: &SheetInfo) -> Result<Relationships> {
        let rels_path = make_rels_path(&sheet.part_path);
        match self.archive.read_entry(&rels_path) {
            Ok(data) => Relationships::parse(&data),
            Err(Error::Zip(_)) => Ok(Relationships::default()),
            Err(e) => Err(e),
        }
    }
}

fn make_rels_path(part_path: &str) -> String {
    if let Some(pos) = part_path.rfind('/') {
        format!("{}/_rels/{}.rels", &part_path[..pos], &part_path[pos + 1..])
    } else {
        format!("_rels/{part_path}.rels")
    }
}

fn parse_workbook_sheets(
    xml: &[u8],
    rels: &Relationships,
    workbook_path: &str,
) -> Result<Vec<SheetInfo>> {
    let xml_str = std::str::from_utf8(xml)
        .map_err(|e| Error::Xml(format!("invalid UTF-8 in workbook: {e}")))?;

    let wb_part = PartPath(workbook_path.to_string());
    let mut sheets = Vec::new();
    let bytes = xml_str.as_bytes();
    let mut pos = 0;

    while let Some(start) = find_tag(bytes, pos, b"<sheet ") {
        let end = find_byte(bytes, start, b'>')
            .ok_or_else(|| Error::Xml("unclosed sheet tag".into()))?;
        let tag = &xml_str[start..=end];

        let name = extract_attr(tag, "name").unwrap_or_default();
        let sheet_id: u32 = extract_attr(tag, "sheetId")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let rel_id = extract_attr(tag, "r:id").unwrap_or_default();
        let state = extract_attr(tag, "state").unwrap_or_default();
        let hidden = state == "hidden" || state == "veryHidden";

        let part_path = rels
            .find_by_id(&rel_id)
            .map(|r| wb_part.resolve_relative(&r.target).0)
            .unwrap_or_default();

        sheets.push(SheetInfo {
            name,
            sheet_id,
            rel_id,
            part_path,
            drawing_path: None,
            hidden,
        });

        pos = end + 1;
    }

    Ok(sheets)
}

fn find_tag(bytes: &[u8], from: usize, tag: &[u8]) -> Option<usize> {
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
    Some(tag[start..end].to_string())
}
