use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Info,
    Warning,
    Error,
    Unsupported,
    CorruptInput,
    ResourceLimit,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Info => write!(f, "INFO"),
            Self::Warning => write!(f, "WARNING"),
            Self::Error => write!(f, "ERROR"),
            Self::Unsupported => write!(f, "UNSUPPORTED"),
            Self::CorruptInput => write!(f, "CORRUPT_INPUT"),
            Self::ResourceLimit => write!(f, "RESOURCE_LIMIT"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Location {
    pub sheet: Option<String>,
    pub cell: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub code: u32,
    pub severity: Severity,
    pub location: Location,
    pub message: String,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{:04}: ", self.severity_prefix(), self.code)?;
        if let Some(sheet) = &self.location.sheet {
            write!(f, "Sheet \"{sheet}\"")?;
            if let Some(cell) = &self.location.cell {
                write!(f, ", cell {cell}")?;
            }
            write!(f, ": ")?;
        }
        write!(f, "{}", self.message)
    }
}

impl Diagnostic {
    fn severity_prefix(&self) -> char {
        match self.severity {
            Severity::Info => 'I',
            Severity::Warning => 'W',
            Severity::Error => 'E',
            Severity::Unsupported => 'U',
            Severity::CorruptInput => 'C',
            Severity::ResourceLimit => 'R',
        }
    }
}

#[derive(Debug, Default)]
pub struct DiagnosticCollector {
    diagnostics: Vec<Diagnostic>,
}

impl DiagnosticCollector {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn emit(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    pub fn warn(&mut self, code: u32, message: String) {
        self.emit(Diagnostic {
            code,
            severity: Severity::Warning,
            location: Location {
                sheet: None,
                cell: None,
            },
            message,
        });
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| matches!(d.severity, Severity::Error | Severity::CorruptInput))
    }

    pub fn drain(&mut self) -> Vec<Diagnostic> {
        std::mem::take(&mut self.diagnostics)
    }
}
