pub struct PackageLimits {
    pub max_entry_count: u32,
    pub max_total_uncompressed: u64,
    pub max_single_entry: u64,
    pub max_compression_ratio: u32,
    pub max_path_length: usize,
}

impl Default for PackageLimits {
    fn default() -> Self {
        Self {
            max_entry_count: 100_000,
            max_total_uncompressed: 8 << 30,
            max_single_entry: 2 << 30,
            max_compression_ratio: 1000,
            max_path_length: 512,
        }
    }
}
