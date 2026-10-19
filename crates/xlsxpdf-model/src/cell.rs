use xlsxpdf_core::coord::CellRef;

#[derive(Debug, Clone, PartialEq)]
pub enum CellValue {
    Empty,
    Number(f64),
    String(String),
    SharedString(u64),
    Boolean(bool),
    Error(String),
    InlineString(String),
}

impl CellValue {
    pub fn display_value(&self, sst_lookup: &dyn Fn(u64) -> Option<String>) -> String {
        match self {
            Self::Empty => String::new(),
            Self::Number(n) => {
                if *n == (*n as i64) as f64 && n.abs() < 1e15 {
                    format!("{}", *n as i64)
                } else {
                    format!("{n}")
                }
            }
            Self::String(s) => s.clone(),
            Self::SharedString(id) => sst_lookup(*id).unwrap_or_else(|| format!("[SST:{id}]")),
            Self::Boolean(b) => if *b { "TRUE" } else { "FALSE" }.to_string(),
            Self::Error(e) => e.clone(),
            Self::InlineString(s) => s.clone(),
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::Number(_) => "number",
            Self::String(_) => "string",
            Self::SharedString(_) => "shared_string",
            Self::Boolean(_) => "boolean",
            Self::Error(_) => "error",
            Self::InlineString(_) => "inline_string",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Cell {
    pub reference: CellRef,
    pub value: CellValue,
    pub style_id: u32,
}
