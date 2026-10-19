use std::path::PathBuf;

pub struct ConvertOptions {
    pub memory_limit_bytes: u64,
    pub temp_limit_bytes: u64,
    pub temp_directory: Option<PathBuf>,
    pub font_directory: Option<PathBuf>,
    pub strict_mode: bool,
}

impl Default for ConvertOptions {
    fn default() -> Self {
        Self {
            memory_limit_bytes: 256 << 20,
            temp_limit_bytes: 4 << 30,
            temp_directory: None,
            font_directory: None,
            strict_mode: false,
        }
    }
}
