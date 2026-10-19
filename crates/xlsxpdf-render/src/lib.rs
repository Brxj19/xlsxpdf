use std::collections::HashMap;
use std::io::{Read, Seek, Write};

use xlsxpdf_core::coord::parse_cell_ref;
use xlsxpdf_diagnostics::DiagnosticCollector;
use xlsxpdf_drawing::{DrawingInfo, parse_drawings};
use xlsxpdf_fonts::FontMetrics;
use xlsxpdf_layout::{SheetLayout, ExcelMetricProfile, resolve_row_heights};
use xlsxpdf_layout::CellContent as LayoutCellContent;
use xlsxpdf_memory::{MemoryGovernor, Subsystem};
use xlsxpdf_model::cell::CellValue;
use xlsxpdf_model::merge::MergeMap;
use xlsxpdf_model::print_settings::PrintSettings;
use xlsxpdf_model::sheet_geometry::SheetGeometry;
use xlsxpdf_model::sheet_parser::SheetParser;
use xlsxpdf_opc::{OpcPackage, SheetInfo, PartPath};
use xlsxpdf_opc::relationships::RelationshipType;
use xlsxpdf_pagination::{compile_pagination, compute_centering_offset, PageRange};
use xlsxpdf_pdf::{ContentBuilder, PdfWriter, decode_png_to_rgb};
use xlsxpdf_styles::{format_number, StyleSheet, ThemePalette, BorderInfo, BorderSide, BorderStyle, FillPattern, HorizontalAlignment, VerticalAlignment};
use xlsxpdf_text::{wrap_text, compute_shrink_scale};
use xlsxpdf_xml::{XmlEvent, XmlReader};

pub trait RenderCallbacks {
    fn on_sheet_start(&self, name: &str);
    fn on_sheet_stats(&self, rows: usize, cols: usize, cells: usize, merges: usize);
    fn on_pages(&self, count: usize, scale: f64);
    fn on_progress(&self, pages_written: usize);
    fn on_warning(&self, message: &str);
    fn on_image_bytes(&self, _bytes: u64) {}
}

pub struct ConvertResult {
    pub total_pages: u32,
    pub total_image_bytes: u64,
}

struct CellData {
    row: u32,
    col: u16,
    display_text: String,
    style_id: u32,
    is_numeric: bool,
    rich_runs: Vec<TextRun>,
}

#[derive(Clone)]
struct TextRun {
    text: String,
    bold: bool,
    italic: bool,
    font_size: f64,
    color: Option<(u8, u8, u8)>,
}

const CELL_PADDING: f64 = 2.0;
const DEFAULT_FONT_SIZE: f64 = 11.0;
const LINE_LEADING: f64 = 1.5;

fn get_font_size(stylesheet: &Option<StyleSheet>, style_id: u32) -> f64 {
    if let Some(ss) = stylesheet {
        if let Some(xf) = ss.get_style(style_id) {
            if let Some(font) = ss.get_font(xf.font_id) {
                let sz = font.size_hundredths as f64 / 100.0;
                if sz > 0.5 {
                    return sz;
                }
            }
        }
    }
    DEFAULT_FONT_SIZE
}

fn get_pdf_font_name(stylesheet: &Option<StyleSheet>, style_id: u32) -> &'static str {
    if let Some(ss) = stylesheet {
        if let Some(xf) = ss.get_style(style_id) {
            if let Some(font) = ss.get_font(xf.font_id) {
                return match (font.bold, font.italic) {
                    (true, true) => "F5",
                    (true, false) => "F2",
                    (false, true) => "F4",
                    (false, false) => "F1",
                };
            }
        }
    }
    "F1"
}

fn is_wrap_text(stylesheet: &Option<StyleSheet>, style_id: u32) -> bool {
    if let Some(ss) = stylesheet {
        if let Some(xf) = ss.get_style(style_id) {
            return xf.alignment.wrap_text;
        }
    }
    false
}

fn is_shrink_to_fit(stylesheet: &Option<StyleSheet>, style_id: u32) -> bool {
    if let Some(ss) = stylesheet {
        if let Some(xf) = ss.get_style(style_id) {
            return xf.alignment.shrink_to_fit;
        }
    }
    false
}

fn get_text_rotation(stylesheet: &Option<StyleSheet>, style_id: u32) -> u16 {
    if let Some(ss) = stylesheet {
        if let Some(xf) = ss.get_style(style_id) {
            return xf.alignment.text_rotation;
        }
    }
    0
}

pub fn parse_simple_sst(data: &[u8]) -> Vec<String> {
    let (strings, _) = parse_sst_with_runs(data);
    strings
}

fn parse_sst_with_runs(data: &[u8]) -> (Vec<String>, Vec<Vec<TextRun>>) {
    let mut reader = XmlReader::new(data);
    let mut strings = Vec::new();
    let mut all_runs: Vec<Vec<TextRun>> = Vec::new();
    let mut in_si = false;
    let mut in_t = false;
    let mut in_r = false;
    let mut in_r_t = false;
    let mut in_r_pr = false;
    let mut current = String::new();
    let mut current_runs: Vec<TextRun> = Vec::new();
    let mut run_text = String::new();
    let mut run_bold = false;
    let mut run_italic = false;
    let mut run_font_size: f64 = 0.0;
    let mut run_color: Option<(u8, u8, u8)> = None;

    loop {
        match reader.next_event() {
            Ok(XmlEvent::StartElement { local_name, attrs }) => match local_name.as_str() {
                "si" => {
                    in_si = true;
                    current.clear();
                    current_runs.clear();
                }
                "t" if in_si && !in_r => {
                    in_t = true;
                }
                "r" if in_si => {
                    in_r = true;
                    run_text.clear();
                    run_bold = false;
                    run_italic = false;
                    run_font_size = 0.0;
                    run_color = None;
                }
                "t" if in_r => {
                    in_r_t = true;
                }
                "rPr" if in_r => {
                    in_r_pr = true;
                }
                "b" if in_r_pr => {
                    let val = attrs.iter().find(|(k, _)| k == "val").map(|(_, v)| v.as_str());
                    run_bold = !matches!(val, Some("0") | Some("false"));
                }
                "i" if in_r_pr => {
                    let val = attrs.iter().find(|(k, _)| k == "val").map(|(_, v)| v.as_str());
                    run_italic = !matches!(val, Some("0") | Some("false"));
                }
                "sz" if in_r_pr => {
                    if let Some((_, v)) = attrs.iter().find(|(k, _)| k == "val") {
                        run_font_size = v.parse().unwrap_or(0.0);
                    }
                }
                "color" if in_r_pr => {
                    if let Some((_, v)) = attrs.iter().find(|(k, _)| k == "rgb") {
                        run_color = parse_argb_color(v);
                    }
                }
                _ => {}
            },
            Ok(XmlEvent::Text(text)) => {
                if in_r_t {
                    run_text.push_str(&text);
                } else if in_t {
                    current.push_str(&text);
                }
            }
            Ok(XmlEvent::EndElement { local_name }) => match local_name.as_str() {
                "t" if in_r => {
                    in_r_t = false;
                }
                "t" => {
                    in_t = false;
                }
                "rPr" => {
                    in_r_pr = false;
                }
                "r" if in_si => {
                    current.push_str(&run_text);
                    current_runs.push(TextRun {
                        text: std::mem::take(&mut run_text),
                        bold: run_bold,
                        italic: run_italic,
                        font_size: run_font_size,
                        color: run_color,
                    });
                    in_r = false;
                }
                "si" => {
                    strings.push(std::mem::take(&mut current));
                    let runs = if current_runs.len() > 1 {
                        std::mem::take(&mut current_runs)
                    } else {
                        current_runs.clear();
                        Vec::new()
                    };
                    all_runs.push(runs);
                    in_si = false;
                }
                _ => {}
            },
            Ok(XmlEvent::Eof) => break,
            Err(_) => break,
        }
    }

    (strings, all_runs)
}

fn parse_argb_color(s: &str) -> Option<(u8, u8, u8)> {
    let hex = s.trim();
    if hex.len() == 8 {
        let r = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let g = u8::from_str_radix(&hex[4..6], 16).ok()?;
        let b = u8::from_str_radix(&hex[6..8], 16).ok()?;
        Some((r, g, b))
    } else if hex.len() == 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        Some((r, g, b))
    } else {
        None
    }
}

fn merged_cell_rect(
    layout: &SheetLayout,
    merge_map: &MergeMap,
    row: usize,
    col: usize,
) -> (f64, f64, f64, f64) {
    if let Some(mr) = merge_map.get_merge(row as u32, col as u16) {
        let x = layout.col_x[col];
        let y = layout.row_y[row];
        let end_col = (mr.col1 as usize + 1).min(layout.col_x.len() - 1);
        let end_row = (mr.row1 as usize + 1).min(layout.row_y.len() - 1);
        let w = layout.col_x[end_col] - x;
        let h = layout.row_y[end_row] - y;
        (x, y, w, h)
    } else {
        let rect = layout.cell_rect(row, col).unwrap();
        (rect.x, rect.y, rect.width, rect.height)
    }
}

fn load_sheet_drawings<R: Read + Seek>(
    package: &mut OpcPackage<R>,
    sheet_info: &SheetInfo,
) -> xlsxpdf_core::error::Result<Option<DrawingInfo>> {
    let sheet_rels = package.read_sheet_rels(sheet_info)?;
    let drawing_rels = sheet_rels.find_by_type(RelationshipType::Drawing);

    if drawing_rels.is_empty() {
        return Ok(None);
    }

    let sheet_part = PartPath(sheet_info.part_path.clone());
    let drawing_path = sheet_part.resolve_relative(&drawing_rels[0].target).0;

    match package.read_part(&drawing_path) {
        Ok(drawing_data) => {
            let info = parse_drawings(&drawing_data)?;
            if info.anchors.is_empty() {
                Ok(None)
            } else {
                Ok(Some(info))
            }
        }
        Err(_) => Ok(None),
    }
}

fn load_image_data<R: Read + Seek>(
    package: &mut OpcPackage<R>,
    sheet_info: &SheetInfo,
    rel_id: &str,
) -> Option<Vec<u8>> {
    let sheet_rels = package.read_sheet_rels(sheet_info).ok()?;
    let drawing_rels = sheet_rels.find_by_type(RelationshipType::Drawing);
    if drawing_rels.is_empty() {
        return None;
    }

    let sheet_part = PartPath(sheet_info.part_path.clone());
    let drawing_path = sheet_part.resolve_relative(&drawing_rels[0].target).0;
    let drawing_rels_path = format!(
        "{}/_rels/{}.rels",
        drawing_path.rsplit_once('/').map_or("", |(p, _)| p),
        drawing_path
            .rsplit_once('/')
            .map_or(drawing_path.as_str(), |(_, f)| f),
    );

    let drawing_part = PartPath(drawing_path);
    let rels_data = package.read_part(&drawing_rels_path).ok()?;
    let rels = xlsxpdf_opc::relationships::Relationships::parse(&rels_data).ok()?;
    let rel = rels.find_by_id(rel_id)?;
    let image_path = drawing_part.resolve_relative(&rel.target).0;
    package.read_part(&image_path).ok()
}

fn simple_hash(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn is_jpeg(data: &[u8]) -> bool {
    data.len() >= 2 && data[0] == 0xFF && data[1] == 0xD8
}

fn guess_image_dimensions(data: &[u8]) -> (u32, u32) {
    if is_jpeg(data) {
        return parse_jpeg_dimensions(data).unwrap_or((100, 100));
    }
    if data.len() >= 24 && &data[0..8] == b"\x89PNG\r\n\x1a\n" {
        let w = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
        let h = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
        return (w, h);
    }
    (100, 100)
}

fn parse_jpeg_dimensions(data: &[u8]) -> Option<(u32, u32)> {
    let mut pos = 2;
    while pos + 4 < data.len() {
        if data[pos] != 0xFF {
            pos += 1;
            continue;
        }
        let marker = data[pos + 1];
        if marker == 0xC0 || marker == 0xC2 {
            if pos + 9 < data.len() {
                let h = u16::from_be_bytes([data[pos + 5], data[pos + 6]]) as u32;
                let w = u16::from_be_bytes([data[pos + 7], data[pos + 8]]) as u32;
                return Some((w, h));
            }
        }
        if marker == 0xD9 || marker == 0xDA {
            break;
        }
        let len = u16::from_be_bytes([data[pos + 2], data[pos + 3]]) as usize;
        pos += 2 + len;
    }
    None
}

fn draw_cell_borders(
    cb: &mut ContentBuilder,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    border: &BorderInfo,
) {
    draw_border_side(cb, &border.top, x, y, x + w, y);
    draw_border_side(cb, &border.bottom, x, y + h, x + w, y + h);
    draw_border_side(cb, &border.left, x, y, x, y + h);
    draw_border_side(cb, &border.right, x + w, y, x + w, y + h);
}

fn draw_border_side(
    cb: &mut ContentBuilder,
    side: &BorderSide,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
) {
    let line_width = match side.style {
        BorderStyle::None => return,
        BorderStyle::Thin | BorderStyle::Hair | BorderStyle::Dotted | BorderStyle::Dashed => 0.5,
        BorderStyle::Medium | BorderStyle::Double => 1.0,
        BorderStyle::Thick => 1.5,
        BorderStyle::Other(_) => 0.5,
    };

    let (r, g, b) = if let Some(color) = &side.color {
        (
            color.r as f64 / 255.0,
            color.g as f64 / 255.0,
            color.b as f64 / 255.0,
        )
    } else {
        (0.0, 0.0, 0.0)
    };

    cb.draw_line(x1, y1, x2, y2, line_width, r, g, b);
}

pub fn convert<R: Read + Seek, W: Write>(
    package: &mut OpcPackage<R>,
    writer: W,
    governor: &MemoryGovernor,
    callbacks: &dyn RenderCallbacks,
) -> xlsxpdf_core::error::Result<ConvertResult> {
    let (shared_strings, shared_rich_runs) = if let Some(sst_path) = package.shared_strings_path.clone() {
        let sst_data = package.read_part(&sst_path)?;
        let (sst, runs) = parse_sst_with_runs(&sst_data);
        let sst_bytes: u64 = sst.iter().map(|s| s.len() as u64).sum();
        governor
            .try_allocate(Subsystem::Sst, sst_bytes)
            .map_err(|e| xlsxpdf_core::error::Error::ResourceLimit(e.to_string()))?;
        (sst, runs)
    } else {
        (Vec::new(), Vec::new())
    };

    let theme_palette = if let Some(theme_path) = package.theme_path.clone() {
        match package.read_part(&theme_path) {
            Ok(theme_data) => ThemePalette::parse(&theme_data),
            Err(_) => None,
        }
    } else {
        None
    };

    let stylesheet = if let Some(styles_path) = package.styles_path.clone() {
        let styles_data = package.read_part(&styles_path)?;
        StyleSheet::parse_with_theme(&styles_data, theme_palette.as_ref()).ok()
    } else {
        None
    };

    let sst_lookup = |id: u64| -> Option<String> { shared_strings.get(id as usize).cloned() };
    let default_font = FontMetrics::calibri();

    let sheets: Vec<_> = package.sheets.clone();
    if sheets.is_empty() {
        return Ok(ConvertResult {
            total_pages: 0,
            total_image_bytes: 0,
        });
    }

    let wb_path = package.workbook_path.clone();
    let print_areas = match package.read_part(&wb_path) {
        Ok(wb_data) => parse_print_areas(&wb_data),
        Err(_) => HashMap::new(),
    };

    let mut pdf = PdfWriter::new(writer)?;
    let font_dict_id = pdf.write_font_resources()?;
    let mut total_pages = 0u32;
    let mut image_count = 0u32;
    let mut total_image_bytes = 0u64;

    for (sheet_idx, sheet_info) in sheets.iter().enumerate() {
        if sheet_info.hidden {
            continue;
        }
        callbacks.on_sheet_start(&sheet_info.name);

        let sheet_data = package.read_part(&sheet_info.part_path)?;
        let mut diagnostics = DiagnosticCollector::new();

        let print_settings = PrintSettings::parse(&sheet_data).unwrap_or_default();
        let geom = SheetGeometry::parse(&sheet_data)?;
        let merge_map = MergeMap::parse(&sheet_data).unwrap_or_default();
        let drawing_info = load_sheet_drawings(package, sheet_info)?;

        let mut cells: Vec<CellData> = Vec::new();
        let mut max_row: u32 = 0;
        let mut max_col: u16 = 0;

        SheetParser::stream_cells(&sheet_data, &mut diagnostics, |cell| {
            let is_empty = matches!(cell.value, CellValue::Empty);

            if is_empty {
                let dominated = has_styled_fill_or_border(&stylesheet, cell.style_id);
                if !dominated {
                    return;
                }
            }

            let display_text = if is_empty {
                String::new()
            } else {
                match &cell.value {
                    CellValue::Number(n) => {
                        if let Some(ss) = &stylesheet {
                            if let Some(xf) = ss.get_style(cell.style_id) {
                                let custom = ss.get_num_fmt(xf.num_fmt_id);
                                format_number(*n, xf.num_fmt_id, custom)
                            } else {
                                format_number(*n, 0, None)
                            }
                        } else {
                            format_number(*n, 0, None)
                        }
                    }
                    _ => cell.value.display_value(&sst_lookup),
                }
            };

            let row = cell.reference.row;
            let col = cell.reference.col;
            if row > max_row {
                max_row = row;
            }
            if col > max_col {
                max_col = col;
            }

            let is_numeric = matches!(cell.value, CellValue::Number(_));

            let rich_runs = match &cell.value {
                CellValue::SharedString(id) => {
                    shared_rich_runs.get(*id as usize).cloned().unwrap_or_default()
                }
                _ => Vec::new(),
            };

            cells.push(CellData {
                row,
                col,
                display_text,
                style_id: cell.style_id,
                is_numeric,
                rich_runs,
            });
        })?;

        // P0-5: Expand used range to include drawing anchors
        if let Some(di) = &drawing_info {
            for anchor in &di.anchors {
                let end_col = if let Some(to) = &anchor.to {
                    to.col
                } else {
                    anchor.from.col + 1
                };
                let end_row = if let Some(to) = &anchor.to {
                    to.row
                } else {
                    anchor.from.row + 1
                };
                if end_col > max_col {
                    max_col = end_col;
                }
                if end_row > max_row {
                    max_row = end_row;
                }
            }
        }

        // Apply Print_Area filter if defined for this sheet
        if let Some(&(pa_row0, pa_col0, pa_row1, pa_col1)) = print_areas.get(&(sheet_idx as u32)) {
            cells.retain(|c| {
                c.row >= pa_row0 && c.row <= pa_row1 && c.col >= pa_col0 && c.col <= pa_col1
            });
            max_row = max_row.min(pa_row1);
            max_col = max_col.min(pa_col1);
            if !cells.is_empty() {
                max_row = max_row.max(pa_row0);
                max_col = max_col.max(pa_col0);
            }
        }

        let has_drawings = drawing_info.as_ref().map_or(false, |di| !di.anchors.is_empty());

        if cells.is_empty() && !has_drawings {
            for diag in diagnostics.diagnostics() {
                callbacks.on_warning(&diag.message);
            }
            continue;
        }

        let sheet_layout_bytes: u64 = cells
            .iter()
            .map(|c| c.display_text.len() as u64 + 48)
            .sum();
        if let Err(e) = governor.try_allocate(Subsystem::LayoutArena, sheet_layout_bytes) {
            callbacks.on_warning(&format!("skipping sheet due to memory: {e}"));
            continue;
        }

        let num_cols = (max_col as usize) + 1;
        let num_rows = (max_row as usize) + 1;

        callbacks.on_sheet_stats(
            num_rows,
            num_cols,
            cells.len(),
            merge_map.rects.len(),
        );

        let initial_layout =
            SheetLayout::from_geometry(&geom, num_cols, num_rows, &default_font, DEFAULT_FONT_SIZE);

        let layout_cells: Vec<LayoutCellContent> = cells
            .iter()
            .filter(|c| {
                // P1-7: Skip merged cells for row auto-height (height spans multiple rows)
                if let Some(mr) = merge_map.get_merge(c.row, c.col) {
                    mr.row0 == mr.row1
                } else {
                    true
                }
            })
            .map(|c| LayoutCellContent {
                row: c.row,
                col: c.col,
                text: c.display_text.clone(),
                wrap_text: is_wrap_text(&stylesheet, c.style_id),
                font_size_pt: get_font_size(&stylesheet, c.style_id),
            })
            .collect();

        let resolved_heights = resolve_row_heights(
            &geom,
            &layout_cells,
            &initial_layout.col_widths,
            &default_font,
            &ExcelMetricProfile::default(),
            num_rows,
        );

        let layout = SheetLayout::new(initial_layout.col_widths.clone(), resolved_heights);

        let plan = compile_pagination(
            &layout.col_widths,
            &layout.row_heights,
            &print_settings,
        );
        let page_w = plan.page_width_pt;
        let page_h = plan.page_height_pt;
        let scale = plan.scale;

        callbacks.on_pages(plan.pages.len(), scale);

        // Register images in PDF (with content-based dedup)
        let mut image_names: HashMap<String, String> = HashMap::new();
        let mut image_content_hashes: HashMap<u64, String> = HashMap::new();
        if let Some(di) = &drawing_info {
            for anchor in &di.anchors {
                if let Some(rel_id) = &anchor.image_rel_id {
                    if image_names.contains_key(rel_id) {
                        continue;
                    }
                    if let Some(img_data) = load_image_data(package, sheet_info, rel_id) {
                        let content_hash = simple_hash(&img_data);
                        if let Some(existing_name) = image_content_hashes.get(&content_hash) {
                            image_names.insert(rel_id.clone(), existing_name.clone());
                            continue;
                        }
                        let img_name = format!("Im{}", image_count);
                        image_count += 1;
                        let img_bytes = img_data.len() as u64;
                        if let Err(e) =
                            governor.try_allocate(Subsystem::ImageProcessing, img_bytes)
                        {
                            callbacks.on_warning(&format!("skipping image {rel_id}: {e}"));
                            continue;
                        }
                        total_image_bytes += img_bytes;
                        if is_jpeg(&img_data) {
                            let (w, h) = guess_image_dimensions(&img_data);
                            let _ = pdf.write_jpeg_xobject(&img_name, &img_data, w, h);
                            image_names.insert(rel_id.clone(), img_name.clone());
                        } else if let Some(decoded) = decode_png_to_rgb(&img_data) {
                            if let Some(alpha) = &decoded.alpha_data {
                                let _ = pdf.write_raw_image_xobject_with_smask(
                                    &img_name,
                                    &decoded.rgb_data,
                                    alpha,
                                    decoded.width,
                                    decoded.height,
                                );
                            } else {
                                let _ = pdf.write_raw_image_xobject(
                                    &img_name,
                                    &decoded.rgb_data,
                                    decoded.width,
                                    decoded.height,
                                );
                            }
                            image_names.insert(rel_id.clone(), img_name.clone());
                        } else {
                            callbacks.on_warning(&format!(
                                "unsupported image format for {rel_id}, skipping"
                            ));
                        }
                        image_content_hashes.insert(content_hash, img_name);
                        governor.release(Subsystem::ImageProcessing, img_bytes);
                    }
                }
            }
        }

        // Build cell lookup
        let mut cell_map: HashMap<(u32, u16), usize> = HashMap::new();
        for (i, c) in cells.iter().enumerate() {
            cell_map.insert((c.row, c.col), i);
        }

        let margin_left = print_settings.margin_left_pt();
        let margin_top = print_settings.margin_top_pt();

        let band_content_w: f64 = layout.col_widths.iter().sum();
        let band_content_h: f64 = layout.row_heights.iter().sum();
        let center_x_offset = compute_centering_offset(
            band_content_w,
            print_settings.printable_width_pt(),
            scale,
            print_settings.center_horizontally,
        );
        let center_y_offset = compute_centering_offset(
            band_content_h,
            print_settings.printable_height_pt(),
            scale,
            print_settings.center_vertically,
        );

        // P2-6: Precompute which pages have cells (avoids O(pages×cells))
        let mut pages_with_cells = vec![false; plan.pages.len()];
        {
            let mut page_map: HashMap<(usize, usize), usize> = HashMap::new();
            for (pi, pr) in plan.pages.iter().enumerate() {
                page_map.insert((pr.start_row, pr.start_col), pi);
            }
            let mut row_breaks: Vec<usize> = plan.pages.iter().map(|p| p.start_row).collect();
            let mut col_breaks: Vec<usize> = plan.pages.iter().map(|p| p.start_col).collect();
            row_breaks.sort_unstable();
            row_breaks.dedup();
            col_breaks.sort_unstable();
            col_breaks.dedup();
            for c in &cells {
                let rb = row_breaks.partition_point(|&b| b <= c.row as usize);
                let cb = col_breaks.partition_point(|&b| b <= c.col as usize);
                if rb > 0 && cb > 0 {
                    let rk = row_breaks[rb - 1];
                    let ck = col_breaks[cb - 1];
                    if let Some(&pi) = page_map.get(&(rk, ck)) {
                        pages_with_cells[pi] = true;
                    }
                }
            }
        }

        for (page_idx, page_range) in plan.pages.iter().enumerate() {
            let has_cells = pages_with_cells[page_idx];

            let has_page_drawings = if let Some(di) = &drawing_info {
                di.anchors.iter().any(|anchor| {
                    let (ix, iy, iw, ih) = anchor.resolve_rect(&layout.col_x, &layout.row_y);
                    let page_x0 = layout.col_x[page_range.start_col];
                    let page_y0 = layout.row_y[page_range.start_row];
                    let page_x1 = layout.col_x[page_range.end_col];
                    let page_y1 = layout.row_y[page_range.end_row];
                    ix + iw > page_x0 && ix < page_x1 && iy + ih > page_y0 && iy < page_y1
                })
            } else {
                false
            };

            if !has_cells && !has_page_drawings {
                continue;
            }

            let mut cb = ContentBuilder::new();
            cb.save_state();

            let origin_x = margin_left + center_x_offset;
            let origin_y = margin_top + center_y_offset;

            if (scale - 1.0).abs() > 0.001 {
                cb.set_ctm(
                    scale,
                    0.0,
                    0.0,
                    scale,
                    origin_x * (1.0 - scale),
                    (page_h - origin_y) * (1.0 - scale),
                );
            }

            let band_col_offset = layout.col_x[page_range.start_col];
            let band_row_offset = layout.row_y[page_range.start_row];

            // P1-6: Clip to page band so content can't bleed outside
            let band_w = layout.col_x[page_range.end_col] - band_col_offset;
            let band_h = layout.row_y[page_range.end_row] - band_row_offset;
            let clip_pdf_y = page_h - origin_y - band_h;
            cb.clip_rect(origin_x, clip_pdf_y, band_w, band_h);

            // Draw cell fills
            for row in page_range.start_row..page_range.end_row {
                for col in page_range.start_col..page_range.end_col {
                    if merge_map.is_hidden(row as u32, col as u16) {
                        continue;
                    }

                    if let Some(&idx) = cell_map.get(&(row as u32, col as u16)) {
                        let cell = &cells[idx];
                        if let Some(ss) = &stylesheet {
                            if let Some(xf) = ss.get_style(cell.style_id) {
                                if let Some(fill) = ss.get_fill(xf.fill_id) {
                                    if fill.pattern == FillPattern::Solid {
                                        if let Some(color) = &fill.fg_color {
                                            let (rx, ry, rw, rh) =
                                                merged_cell_rect(&layout, &merge_map, row, col);
                                            let x = origin_x + rx - band_col_offset;
                                            let y_top = origin_y + ry - band_row_offset;
                                            let pdf_y = page_h - y_top - rh;
                                            cb.draw_fill_rect(
                                                x,
                                                pdf_y,
                                                rw,
                                                rh,
                                                color.r as f64 / 255.0,
                                                color.g as f64 / 255.0,
                                                color.b as f64 / 255.0,
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Draw fills for merged cells spanning from outside this page
            for mr in &merge_map.rects {
                let origin_in_page = (mr.row0 as usize) >= page_range.start_row
                    && (mr.row0 as usize) < page_range.end_row
                    && (mr.col0 as usize) >= page_range.start_col
                    && (mr.col0 as usize) < page_range.end_col;
                if origin_in_page {
                    continue;
                }
                if (mr.row1 as usize) < page_range.start_row
                    || (mr.row0 as usize) >= page_range.end_row
                    || (mr.col1 as usize) < page_range.start_col
                    || (mr.col0 as usize) >= page_range.end_col
                {
                    continue;
                }
                if let Some(&idx) = cell_map.get(&(mr.row0, mr.col0)) {
                    let cell = &cells[idx];
                    if let Some(ss) = &stylesheet {
                        if let Some(xf) = ss.get_style(cell.style_id) {
                            if let Some(fill) = ss.get_fill(xf.fill_id) {
                                if fill.pattern == FillPattern::Solid {
                                    if let Some(color) = &fill.fg_color {
                                        let (cx, cy, cw, ch) = clip_merge_to_page(
                                            &layout, mr, page_range,
                                        );
                                        let x = origin_x + cx - band_col_offset;
                                        let y_top = origin_y + cy - band_row_offset;
                                        let pdf_y = page_h - y_top - ch;
                                        cb.draw_fill_rect(
                                            x, pdf_y, cw, ch,
                                            color.r as f64 / 255.0,
                                            color.g as f64 / 255.0,
                                            color.b as f64 / 255.0,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Draw grid lines (only when printOptions says so)
            if print_settings.print_gridlines {
                cb.set_stroke_color_rgb(0.8, 0.8, 0.8);
                cb.set_line_width(0.25);

                for row in page_range.start_row..=page_range.end_row {
                    let y_top = origin_y + layout.row_y[row] - band_row_offset;
                    let pdf_y = page_h - y_top;
                    let x1 = origin_x;
                    let x2 = origin_x + layout.col_x[page_range.end_col] - band_col_offset;
                    cb.move_to(x1, pdf_y);
                    cb.line_to(x2, pdf_y);
                    cb.stroke();
                }

                for col in page_range.start_col..=page_range.end_col {
                    let x = origin_x + layout.col_x[col] - band_col_offset;
                    let y1_top = origin_y;
                    let y2_top = origin_y + layout.row_y[page_range.end_row] - band_row_offset;
                    cb.move_to(x, page_h - y1_top);
                    cb.line_to(x, page_h - y2_top);
                    cb.stroke();
                }
            }

            // Draw styled borders
            for row in page_range.start_row..page_range.end_row {
                for col in page_range.start_col..page_range.end_col {
                    if merge_map.is_hidden(row as u32, col as u16) {
                        continue;
                    }
                    if let Some(&idx) = cell_map.get(&(row as u32, col as u16)) {
                        let cell = &cells[idx];
                        if let Some(ss) = &stylesheet {
                            if let Some(xf) = ss.get_style(cell.style_id) {
                                if let Some(border) = ss.get_border(xf.border_id) {
                                    let (rx, ry, rw, rh) =
                                        merged_cell_rect(&layout, &merge_map, row, col);
                                    let x = origin_x + rx - band_col_offset;
                                    let y_top = origin_y + ry - band_row_offset;
                                    let pdf_y = page_h - y_top - rh;
                                    draw_cell_borders(&mut cb, x, pdf_y, rw, rh, border);
                                }
                            }
                        }
                    }
                }
            }

            // Draw borders for merged cells spanning from outside this page
            for mr in &merge_map.rects {
                let origin_in_page = (mr.row0 as usize) >= page_range.start_row
                    && (mr.row0 as usize) < page_range.end_row
                    && (mr.col0 as usize) >= page_range.start_col
                    && (mr.col0 as usize) < page_range.end_col;
                if origin_in_page {
                    continue;
                }
                if (mr.row1 as usize) < page_range.start_row
                    || (mr.row0 as usize) >= page_range.end_row
                    || (mr.col1 as usize) < page_range.start_col
                    || (mr.col0 as usize) >= page_range.end_col
                {
                    continue;
                }
                if let Some(&idx) = cell_map.get(&(mr.row0, mr.col0)) {
                    let cell = &cells[idx];
                    if let Some(ss) = &stylesheet {
                        if let Some(xf) = ss.get_style(cell.style_id) {
                            if let Some(border) = ss.get_border(xf.border_id) {
                                let (cx, cy, cw, ch) = clip_merge_to_page(
                                    &layout, mr, page_range,
                                );
                                let x = origin_x + cx - band_col_offset;
                                let y_top = origin_y + cy - band_row_offset;
                                let pdf_y = page_h - y_top - ch;
                                let page_y1 = layout.row_y[page_range.start_row];
                                let page_y2 = layout.row_y[page_range.end_row];
                                let page_x1 = layout.col_x[page_range.start_col];
                                let page_x2 = layout.col_x[page_range.end_col];
                                let merge_y = layout.row_y[mr.row0 as usize];
                                let merge_y2 = layout.row_y[(mr.row1 as usize + 1).min(layout.row_y.len() - 1)];
                                let merge_x = layout.col_x[mr.col0 as usize];
                                let merge_x2 = layout.col_x[(mr.col1 as usize + 1).min(layout.col_x.len() - 1)];
                                let partial = BorderInfo {
                                    top: if merge_y >= page_y1 { border.top.clone() } else { Default::default() },
                                    bottom: if merge_y2 <= page_y2 { border.bottom.clone() } else { Default::default() },
                                    left: if merge_x >= page_x1 { border.left.clone() } else { Default::default() },
                                    right: if merge_x2 <= page_x2 { border.right.clone() } else { Default::default() },
                                };
                                draw_cell_borders(&mut cb, x, pdf_y, cw, ch, &partial);
                            }
                        }
                    }
                }
            }

            // Draw text
            for row in page_range.start_row..page_range.end_row {
                for col in page_range.start_col..page_range.end_col {
                    if merge_map.is_hidden(row as u32, col as u16) {
                        continue;
                    }

                    if let Some(&idx) = cell_map.get(&(row as u32, col as u16)) {
                        let cell = &cells[idx];
                        if cell.display_text.is_empty() {
                            continue;
                        }

                        let (rx, ry, rw, rh) =
                            merged_cell_rect(&layout, &merge_map, row, col);
                        let cell_x = origin_x + rx - band_col_offset;
                        let cell_y_top = origin_y + ry - band_row_offset;
                        let mut font_size = get_font_size(&stylesheet, cell.style_id);
                        let font_name = get_pdf_font_name(&stylesheet, cell.style_id);
                        let should_wrap = is_wrap_text(&stylesheet, cell.style_id);
                        let available_width = rw - 2.0 * CELL_PADDING;

                        if is_shrink_to_fit(&stylesheet, cell.style_id) && !should_wrap {
                            let text_w = default_font.string_width_pt(&cell.display_text, font_size);
                            if text_w > available_width && available_width > 0.0 {
                                let scale_factor = compute_shrink_scale(text_w, available_width);
                                font_size *= scale_factor;
                            }
                        }

                        let h_align = get_horizontal_alignment(&stylesheet, cell.style_id);
                        let mut effective_align = match h_align {
                            HorizontalAlignment::General => {
                                if cell.is_numeric {
                                    HorizontalAlignment::Right
                                } else {
                                    HorizontalAlignment::Left
                                }
                            }
                            other => other,
                        };

                        let lines: Vec<String> = if should_wrap && available_width > 0.0 {
                            let result = wrap_text(
                                &cell.display_text,
                                available_width,
                                &default_font,
                                font_size,
                            );
                            result.lines
                        } else if cell.display_text.contains('\n') {
                            cell.display_text
                                .split('\n')
                                .map(|s| s.to_string())
                                .collect()
                        } else {
                            let text_width =
                                default_font.string_width_pt(&cell.display_text, font_size);
                            if text_width <= available_width || available_width <= 0.0 {
                                vec![cell.display_text.clone()]
                            } else if cell.is_numeric {
                                effective_align = HorizontalAlignment::Left;
                                let hash_w = default_font.glyph_advance_pt('#', font_size);
                                let count = (available_width / hash_w).floor().max(1.0) as usize;
                                vec!["#".repeat(count)]
                            } else if matches!(effective_align, HorizontalAlignment::Left) {
                                let spill_extra = calculate_right_spill_width(
                                    &layout,
                                    &cell_map,
                                    &cells,
                                    &merge_map,
                                    cell.row,
                                    cell.col,
                                    page_range.end_col,
                                );
                                let extended_width = available_width + spill_extra;
                                if text_width <= extended_width {
                                    vec![cell.display_text.clone()]
                                } else {
                                    let mut truncated = String::new();
                                    let mut w = 0.0;
                                    for ch in cell.display_text.chars() {
                                        let cw = default_font.glyph_advance_pt(ch, font_size);
                                        if w + cw > extended_width {
                                            break;
                                        }
                                        truncated.push(ch);
                                        w += cw;
                                    }
                                    vec![truncated]
                                }
                            } else {
                                let mut truncated = String::new();
                                let mut w = 0.0;
                                for ch in cell.display_text.chars() {
                                    let cw = default_font.glyph_advance_pt(ch, font_size);
                                    if w + cw > available_width {
                                        break;
                                    }
                                    truncated.push(ch);
                                    w += cw;
                                }
                                vec![truncated]
                            }
                        };

                        let line_height = font_size * 1.2;
                        let total_text_height = lines.len() as f64 * line_height
                            + (lines.len() as f64 - 1.0).max(0.0) * LINE_LEADING;

                        let v_align = get_vertical_alignment(&stylesheet, cell.style_id);
                        let text_start_y_topdown = match v_align {
                            VerticalAlignment::Top => {
                                cell_y_top + CELL_PADDING + font_size * 0.85
                            }
                            VerticalAlignment::Center | VerticalAlignment::Justify | VerticalAlignment::Distributed => {
                                cell_y_top + (rh - total_text_height) / 2.0 + font_size * 0.85
                            }
                            VerticalAlignment::Bottom => {
                                cell_y_top + rh - CELL_PADDING - total_text_height + font_size * 0.85
                            }
                        };

                        // P0-2: Set explicit text colour
                        let (tr, tg, tb) = resolve_font_color(&stylesheet, cell.style_id);

                        let needs_clip = total_text_height > rh;
                        if needs_clip {
                            cb.save_state();
                            let clip_pdf_y = page_h - cell_y_top - rh;
                            cb.clip_rect(cell_x, clip_pdf_y, rw, rh);
                        }

                        let use_rich = !cell.rich_runs.is_empty()
                            && lines.len() == 1
                            && !should_wrap;

                        let text_rot = get_text_rotation(&stylesheet, cell.style_id);
                        let rot_rad = if text_rot == 255 || text_rot == 0 {
                            0.0
                        } else if text_rot <= 90 {
                            (text_rot as f64).to_radians()
                        } else {
                            -((text_rot as f64 - 90.0).to_radians())
                        };
                        let tm_cos = rot_rad.cos();
                        let tm_sin = rot_rad.sin();

                        cb.begin_text();

                        if use_rich {
                            let total_width: f64 = cell.rich_runs.iter().map(|run| {
                                let sz = if run.font_size > 0.5 { run.font_size } else { font_size };
                                default_font.string_width_pt(&run.text, sz)
                            }).sum();
                            let lx = match effective_align {
                                HorizontalAlignment::Center | HorizontalAlignment::CenterContinuous => {
                                    cell_x + CELL_PADDING + (available_width - total_width) / 2.0
                                }
                                HorizontalAlignment::Right => {
                                    cell_x + rw - CELL_PADDING - total_width
                                }
                                _ => cell_x + CELL_PADDING,
                            };
                            let pdf_ly = page_h - text_start_y_topdown;
                            let mut cursor_x = lx;
                            for run in &cell.rich_runs {
                                if run.text.is_empty() {
                                    continue;
                                }
                                let run_font = match (run.bold, run.italic) {
                                    (true, true) => "F5",
                                    (true, false) => "F2",
                                    (false, true) => "F4",
                                    (false, false) => "F1",
                                };
                                let run_size = if run.font_size > 0.5 { run.font_size } else { font_size };
                                let (rr, rg, rb) = if let Some((cr, cg, cb)) = run.color {
                                    (cr as f64 / 255.0, cg as f64 / 255.0, cb as f64 / 255.0)
                                } else {
                                    (tr, tg, tb)
                                };
                                cb.set_font(run_font, run_size);
                                cb.set_fill_color_rgb(rr, rg, rb);
                                cb.set_text_matrix(tm_cos, tm_sin, -tm_sin, tm_cos, cursor_x, pdf_ly);
                                cb.show_text(&run.text);
                                cursor_x += default_font.string_width_pt(&run.text, run_size);
                            }
                        } else {
                            cb.set_font(font_name, font_size);
                            cb.set_fill_color_rgb(tr, tg, tb);

                            for (line_idx, line) in lines.iter().enumerate() {
                                if line.is_empty() {
                                    continue;
                                }
                                let line_width = default_font.string_width_pt(line, font_size);
                                let lx = match effective_align {
                                    HorizontalAlignment::Center | HorizontalAlignment::CenterContinuous => {
                                        cell_x + CELL_PADDING + (available_width - line_width) / 2.0
                                    }
                                    HorizontalAlignment::Right => {
                                        cell_x + rw - CELL_PADDING - line_width
                                    }
                                    _ => cell_x + CELL_PADDING,
                                };
                                let ly_topdown = text_start_y_topdown
                                    + line_idx as f64 * (line_height + LINE_LEADING);
                                let pdf_ly = page_h - ly_topdown;
                                cb.set_text_matrix(tm_cos, tm_sin, -tm_sin, tm_cos, lx, pdf_ly);
                                cb.show_text(line);
                            }
                        }

                        cb.end_text();

                        if needs_clip {
                            cb.restore_state();
                        }
                    }
                }
            }

            // Draw images on this page
            if let Some(di) = &drawing_info {
                for anchor in &di.anchors {
                    if let Some(rel_id) = &anchor.image_rel_id {
                        if let Some(img_name) = image_names.get(rel_id) {
                            let (ix, iy, iw, ih) =
                                anchor.resolve_rect(&layout.col_x, &layout.row_y);

                            let page_x0 = layout.col_x[page_range.start_col];
                            let page_y0 = layout.row_y[page_range.start_row];
                            let page_x1 = layout.col_x[page_range.end_col];
                            let page_y1 = layout.row_y[page_range.end_row];

                            if ix + iw > page_x0
                                && ix < page_x1
                                && iy + ih > page_y0
                                && iy < page_y1
                            {
                                let draw_x = origin_x + ix - band_col_offset;
                                let draw_y_top = origin_y + iy - band_row_offset;
                                let pdf_draw_y = page_h - draw_y_top - ih;
                                cb.save_state();
                                let clip_w = page_x1 - page_x0;
                                let clip_h = page_y1 - page_y0;
                                let pdf_clip_y = page_h - origin_y - clip_h;
                                cb.clip_rect(origin_x, pdf_clip_y, clip_w, clip_h);
                                cb.draw_image(img_name, draw_x, pdf_draw_y, iw, ih);
                                cb.restore_state();
                            }
                        }
                    }
                }
            }

            cb.restore_state();

            let content = cb.into_bytes();
            pdf.write_page(&content, page_w, page_h, font_dict_id)?;

            total_pages += 1;
            if total_pages % 50 == 0 {
                callbacks.on_progress(total_pages as usize);
            }
        }

        governor.release(Subsystem::LayoutArena, sheet_layout_bytes);

        for diag in diagnostics.diagnostics() {
            callbacks.on_warning(&diag.message);
        }
    }

    pdf.finish()?;

    callbacks.on_image_bytes(total_image_bytes);

    Ok(ConvertResult {
        total_pages,
        total_image_bytes,
    })
}

fn get_horizontal_alignment(stylesheet: &Option<StyleSheet>, style_id: u32) -> HorizontalAlignment {
    if let Some(ss) = stylesheet {
        if let Some(xf) = ss.get_style(style_id) {
            return xf.alignment.horizontal;
        }
    }
    HorizontalAlignment::General
}

fn get_vertical_alignment(stylesheet: &Option<StyleSheet>, style_id: u32) -> VerticalAlignment {
    if let Some(ss) = stylesheet {
        if let Some(xf) = ss.get_style(style_id) {
            return xf.alignment.vertical;
        }
    }
    VerticalAlignment::Bottom
}

fn has_styled_fill_or_border(stylesheet: &Option<StyleSheet>, style_id: u32) -> bool {
    if let Some(ss) = stylesheet {
        if let Some(xf) = ss.get_style(style_id) {
            if let Some(fill) = ss.get_fill(xf.fill_id) {
                if fill.pattern == FillPattern::Solid && fill.fg_color.is_some() {
                    return true;
                }
            }
            if let Some(border) = ss.get_border(xf.border_id) {
                if border.top.style != BorderStyle::None
                    || border.bottom.style != BorderStyle::None
                    || border.left.style != BorderStyle::None
                    || border.right.style != BorderStyle::None
                {
                    return true;
                }
            }
        }
    }
    false
}

fn clip_merge_to_page(
    layout: &SheetLayout,
    mr: &xlsxpdf_model::merge::MergeRect,
    page: &PageRange,
) -> (f64, f64, f64, f64) {
    let merge_x1 = layout.col_x[mr.col0 as usize];
    let merge_y1 = layout.row_y[mr.row0 as usize];
    let end_col = (mr.col1 as usize + 1).min(layout.col_x.len() - 1);
    let end_row = (mr.row1 as usize + 1).min(layout.row_y.len() - 1);
    let merge_x2 = layout.col_x[end_col];
    let merge_y2 = layout.row_y[end_row];

    let page_x1 = layout.col_x[page.start_col];
    let page_y1 = layout.row_y[page.start_row];
    let page_x2 = layout.col_x[page.end_col];
    let page_y2 = layout.row_y[page.end_row];

    let x = merge_x1.max(page_x1);
    let y = merge_y1.max(page_y1);
    let w = merge_x2.min(page_x2) - x;
    let h = merge_y2.min(page_y2) - y;
    (x, y, w.max(0.0), h.max(0.0))
}

fn calculate_right_spill_width(
    layout: &SheetLayout,
    cell_map: &HashMap<(u32, u16), usize>,
    cells: &[CellData],
    merge_map: &MergeMap,
    row: u32,
    col: u16,
    page_end_col: usize,
) -> f64 {
    let mut extra = 0.0;
    let mut c = col as usize + 1;
    while c < page_end_col && c < layout.col_widths.len() {
        if merge_map.is_hidden(row, c as u16) || merge_map.get_merge(row, c as u16).is_some() {
            break;
        }
        if let Some(&idx) = cell_map.get(&(row, c as u16)) {
            if !cells[idx].display_text.is_empty() {
                break;
            }
        }
        extra += layout.col_widths[c];
        c += 1;
    }
    extra
}

fn parse_print_areas(workbook_data: &[u8]) -> HashMap<u32, (u32, u16, u32, u16)> {
    let mut reader = XmlReader::new(workbook_data);
    let mut areas = HashMap::new();
    let mut in_defined_name = false;
    let mut current_name = String::new();
    let mut current_sheet_id: Option<u32> = None;
    let mut current_text = String::new();

    loop {
        match reader.next_event() {
            Ok(XmlEvent::StartElement { local_name, attrs }) => {
                if local_name == "definedName" {
                    current_name.clear();
                    current_sheet_id = None;
                    current_text.clear();
                    for (key, val) in &attrs {
                        match key.as_str() {
                            "name" => current_name = val.clone(),
                            "localSheetId" => current_sheet_id = val.parse().ok(),
                            _ => {}
                        }
                    }
                    in_defined_name = true;
                }
            }
            Ok(XmlEvent::Text(text)) => {
                if in_defined_name {
                    current_text.push_str(&text);
                }
            }
            Ok(XmlEvent::EndElement { local_name }) => {
                if local_name == "definedName" && in_defined_name {
                    if current_name == "_xlnm.Print_Area" {
                        if let Some(sheet_id) = current_sheet_id {
                            if let Some(range) = parse_area_ref(&current_text) {
                                areas.insert(sheet_id, range);
                            }
                        }
                    }
                    in_defined_name = false;
                }
            }
            Ok(XmlEvent::Eof) => break,
            Err(_) => break,
        }
    }

    areas
}

fn parse_area_ref(text: &str) -> Option<(u32, u16, u32, u16)> {
    let range_part = text.rsplit_once('!')?.1;
    let colon = range_part.find(':')?;
    let from_str = &range_part[..colon];
    let to_str = &range_part[colon + 1..];
    let (from_ref, _) = parse_cell_ref(from_str.as_bytes())?;
    let (to_ref, _) = parse_cell_ref(to_str.as_bytes())?;
    Some((from_ref.row, from_ref.col, to_ref.row, to_ref.col))
}

fn resolve_font_color(stylesheet: &Option<StyleSheet>, style_id: u32) -> (f64, f64, f64) {
    if let Some(ss) = stylesheet {
        if let Some(xf) = ss.get_style(style_id) {
            if let Some(font) = ss.get_font(xf.font_id) {
                if let Some(color) = &font.color {
                    return (
                        color.r as f64 / 255.0,
                        color.g as f64 / 255.0,
                        color.b as f64 / 255.0,
                    );
                }
            }
        }
    }
    (0.0, 0.0, 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_area_ref() {
        let r = parse_area_ref("Sheet1!$A$1:$D$10").unwrap();
        assert_eq!(r, (0, 0, 9, 3));

        let r = parse_area_ref("'Sheet Name'!$B$2:$F$50").unwrap();
        assert_eq!(r, (1, 1, 49, 5));

        assert!(parse_area_ref("invalid").is_none());
        assert!(parse_area_ref("Sheet1!A1").is_none());
    }

    #[test]
    fn test_parse_print_areas() {
        let xml = br#"<?xml version="1.0"?>
<workbook>
  <sheets>
    <sheet name="Sheet1" sheetId="1" r:id="rId1"/>
  </sheets>
  <definedNames>
    <definedName name="_xlnm.Print_Area" localSheetId="0">Sheet1!$A$1:$D$10</definedName>
  </definedNames>
</workbook>"#;
        let areas = parse_print_areas(xml);
        assert_eq!(areas.len(), 1);
        assert_eq!(areas[&0], (0, 0, 9, 3));
    }

    #[test]
    fn test_parse_print_areas_none() {
        let xml = br#"<?xml version="1.0"?>
<workbook><sheets/></workbook>"#;
        let areas = parse_print_areas(xml);
        assert!(areas.is_empty());
    }
}
