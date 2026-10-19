use xlsxpdf_memory::MemoryError;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("ZIP error: {0}")]
    Zip(String),

    #[error("XML error: {0}")]
    Xml(String),

    #[error("invalid XLSX structure: {0}")]
    InvalidStructure(String),

    #[error("corrupt input: {0}")]
    CorruptInput(String),

    #[error("unsupported feature: {0}")]
    Unsupported(String),

    #[error("resource limit: {0}")]
    ResourceLimit(String),

    #[error(transparent)]
    Memory(#[from] MemoryError),
}

pub type Result<T> = std::result::Result<T, Error>;
