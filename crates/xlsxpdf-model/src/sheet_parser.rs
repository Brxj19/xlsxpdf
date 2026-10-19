use xlsxpdf_core::coord::{parse_cell_ref, CellRef};
use xlsxpdf_core::error::Result;
use xlsxpdf_diagnostics::DiagnosticCollector;
use xlsxpdf_xml::{XmlEvent, XmlReader};

use crate::cell::{Cell, CellValue};

pub struct SheetStats {
    pub rows_scanned: u64,
    pub cells_scanned: u64,
}

pub struct SheetParser;

impl SheetParser {
    pub fn stream_cells(
        xml_data: &[u8],
        diagnostics: &mut DiagnosticCollector,
        mut callback: impl FnMut(Cell),
    ) -> Result<SheetStats> {
        let mut reader = XmlReader::new(xml_data);
        let mut stats = SheetStats {
            rows_scanned: 0,
            cells_scanned: 0,
        };

        let mut in_row = false;
        let mut in_cell = false;
        let mut in_value = false;
        let mut in_inline_string = false;
        let mut in_is_text = false;

        let mut current_ref = CellRef::new(0, 0);
        let mut current_type = String::new();
        let mut current_style: u32 = 0;
        let mut value_text = String::new();
        let mut inline_text = String::new();

        loop {
            let event = reader.next_event()?;
            match event {
                XmlEvent::StartElement { local_name, attrs } => match local_name.as_str() {
                    "row" => {
                        in_row = true;
                        stats.rows_scanned += 1;
                    }
                    "c" if in_row => {
                        in_cell = true;
                        current_type.clear();
                        current_style = 0;
                        value_text.clear();
                        inline_text.clear();
                        current_ref = CellRef::new(0, 0);

                        for (key, val) in &attrs {
                            match key.as_str() {
                                "r" => {
                                    if let Some((cell, _)) = parse_cell_ref(val.as_bytes()) {
                                        current_ref = cell;
                                    }
                                }
                                "t" => {
                                    current_type = val.clone();
                                }
                                "s" => {
                                    current_style = val.parse().unwrap_or(0);
                                }
                                _ => {}
                            }
                        }
                    }
                    "v" if in_cell => {
                        in_value = true;
                        value_text.clear();
                    }
                    "is" if in_cell => {
                        in_inline_string = true;
                        inline_text.clear();
                    }
                    "t" if in_inline_string => {
                        in_is_text = true;
                    }
                    _ => {}
                },

                XmlEvent::Text(text) => {
                    if in_is_text {
                        inline_text.push_str(&text);
                    } else if in_value {
                        value_text.push_str(&text);
                    }
                }

                XmlEvent::EndElement { local_name } => match local_name.as_str() {
                    "row" => {
                        in_row = false;
                    }
                    "c" if in_cell => {
                        let value = parse_cell_value(
                            &current_type,
                            &value_text,
                            &inline_text,
                            diagnostics,
                            &current_ref,
                        );

                        callback(Cell {
                            reference: current_ref,
                            value,
                            style_id: current_style,
                        });

                        stats.cells_scanned += 1;
                        in_cell = false;
                    }
                    "v" => {
                        in_value = false;
                    }
                    "is" => {
                        in_inline_string = false;
                    }
                    "t" if in_inline_string => {
                        in_is_text = false;
                    }
                    _ => {}
                },

                XmlEvent::Eof => break,
            }
        }

        Ok(stats)
    }
}

fn parse_cell_value(
    cell_type: &str,
    value_text: &str,
    inline_text: &str,
    diagnostics: &mut DiagnosticCollector,
    cell_ref: &CellRef,
) -> CellValue {
    match cell_type {
        "s" => {
            if let Ok(id) = value_text.parse::<u64>() {
                CellValue::SharedString(id)
            } else {
                diagnostics.warn(
                    1001,
                    format!(
                        "invalid shared string reference '{}' at {}",
                        value_text,
                        cell_ref.to_a1()
                    ),
                );
                CellValue::Empty
            }
        }
        "b" => CellValue::Boolean(value_text == "1" || value_text.eq_ignore_ascii_case("true")),
        "e" => CellValue::Error(value_text.to_string()),
        "str" => CellValue::String(value_text.to_string()),
        "inlineStr" => CellValue::InlineString(inline_text.to_string()),
        "" | "n" => {
            if value_text.is_empty() {
                CellValue::Empty
            } else if let Ok(n) = value_text.parse::<f64>() {
                CellValue::Number(n)
            } else {
                CellValue::String(value_text.to_string())
            }
        }
        _ => {
            diagnostics.warn(
                1002,
                format!("unknown cell type '{}' at {}", cell_type, cell_ref.to_a1()),
            );
            CellValue::String(value_text.to_string())
        }
    }
}
