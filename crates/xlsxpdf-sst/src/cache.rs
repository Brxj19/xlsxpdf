use std::collections::HashMap;

use crate::rich_string::RichString;
use crate::store::SstStore;
use xlsxpdf_core::error::Result;

struct CacheEntry {
    data: Vec<u8>,
    access_count: u64,
}

pub struct SstCache {
    store: SstStore,
    cache: HashMap<usize, CacheEntry>,
    cache_size: u64,
    cache_budget: u64,
    access_counter: u64,
}

impl SstCache {
    pub fn new(store: SstStore, cache_budget: u64) -> Self {
        Self {
            store,
            cache: HashMap::new(),
            cache_size: 0,
            cache_budget,
            access_counter: 0,
        }
    }

    pub fn total_strings(&self) -> u64 {
        self.store.total_strings()
    }

    pub fn get_string(&mut self, string_id: u64) -> Result<RichString> {
        let block_idx = self
            .store
            .find_block(string_id)
            .ok_or_else(|| {
                xlsxpdf_core::error::Error::InvalidStructure(format!(
                    "SST string {string_id} not found"
                ))
            })?;

        let dir = &self.store.directory()[block_idx];
        let local_idx = (string_id - dir.first_string_id) as usize;

        let block_data = self.get_block(block_idx)?;
        extract_from_block(&block_data, local_idx)
    }

    pub fn get_strings_batched(&mut self, ids: &[u64]) -> Result<Vec<(u64, RichString)>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        // Group by block, sorted by block offset for sequential I/O
        let mut requests: Vec<(usize, u64, usize)> = Vec::with_capacity(ids.len());
        for &id in ids {
            if let Some(block_idx) = self.store.find_block(id) {
                let dir = &self.store.directory()[block_idx];
                let local_idx = (id - dir.first_string_id) as usize;
                requests.push((block_idx, id, local_idx));
            }
        }
        requests.sort_by_key(|(block_idx, _, _)| *block_idx);

        let mut results = Vec::with_capacity(requests.len());
        let mut current_block: Option<(usize, Vec<u8>)> = None;

        for (block_idx, id, local_idx) in requests {
            let block_data = match &current_block {
                Some((idx, data)) if *idx == block_idx => data,
                _ => {
                    let data = self.get_block(block_idx)?;
                    current_block = Some((block_idx, data));
                    &current_block.as_ref().unwrap().1
                }
            };

            let s = extract_from_block(block_data, local_idx)?;
            results.push((id, s));
        }

        Ok(results)
    }

    fn get_block(&mut self, block_idx: usize) -> Result<Vec<u8>> {
        self.access_counter += 1;

        if let Some(entry) = self.cache.get_mut(&block_idx) {
            entry.access_count = self.access_counter;
            return Ok(entry.data.clone());
        }

        let data = self.store.read_block(block_idx)?;
        let data_size = data.len() as u64;

        // Evict if needed
        while self.cache_size + data_size > self.cache_budget && !self.cache.is_empty() {
            self.evict_lru();
        }

        if data_size <= self.cache_budget {
            self.cache.insert(
                block_idx,
                CacheEntry {
                    data: data.clone(),
                    access_count: self.access_counter,
                },
            );
            self.cache_size += data_size;
        }

        Ok(data)
    }

    fn evict_lru(&mut self) {
        if let Some((&key, _)) = self
            .cache
            .iter()
            .min_by_key(|(_, entry)| entry.access_count)
        {
            if let Some(entry) = self.cache.remove(&key) {
                self.cache_size -= entry.data.len() as u64;
            }
        }
    }

    pub fn cache_stats(&self) -> (u64, u64, usize) {
        (self.cache_size, self.cache_budget, self.cache.len())
    }
}

fn extract_from_block(block: &[u8], local_idx: usize) -> Result<RichString> {
    use xlsxpdf_core::error::Error;

    if block.len() < 4 {
        return Err(Error::CorruptInput("SST block too small".into()));
    }
    let count = u32::from_le_bytes(block[0..4].try_into().unwrap()) as usize;
    if local_idx >= count {
        return Err(Error::CorruptInput(format!(
            "SST local index {local_idx} >= count {count}"
        )));
    }

    let offsets_start = 4;
    let offsets_end = offsets_start + (count + 1) * 4;
    if block.len() < offsets_end {
        return Err(Error::CorruptInput("SST block offsets truncated".into()));
    }

    let start = u32::from_le_bytes(
        block[offsets_start + local_idx * 4..offsets_start + local_idx * 4 + 4]
            .try_into()
            .unwrap(),
    ) as usize;
    let end = u32::from_le_bytes(
        block[offsets_start + (local_idx + 1) * 4..offsets_start + (local_idx + 1) * 4 + 4]
            .try_into()
            .unwrap(),
    ) as usize;

    if end > block.len() || start > end {
        return Err(Error::CorruptInput("SST string offset out of range".into()));
    }

    RichString::decode(&block[start..end])
        .map(|(s, _)| s)
        .ok_or_else(|| Error::CorruptInput("failed to decode SST rich string".into()))
}
