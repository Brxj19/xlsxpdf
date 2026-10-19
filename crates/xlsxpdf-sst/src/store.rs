use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use xlsxpdf_core::error::{Error, Result};

use crate::rich_string::RichString;

const SST_FRAMES_MAGIC: [u8; 4] = *b"SSTF";
const SST_IDX_MAGIC: [u8; 4] = *b"SSTI";
const FORMAT_VERSION: u16 = 1;
const HEADER_SIZE: usize = 32;

#[derive(Debug, Clone)]
pub struct SstBlockDir {
    pub first_string_id: u64,
    pub file_offset: u64,
    pub string_count: u32,
    pub compressed_size: u32,
    pub raw_size: u32,
    pub flags: u32,
}

impl SstBlockDir {
    const SIZE: usize = 32;

    fn encode(&self) -> [u8; 32] {
        let mut buf = [0u8; 32];
        buf[0..8].copy_from_slice(&self.first_string_id.to_le_bytes());
        buf[8..16].copy_from_slice(&self.file_offset.to_le_bytes());
        buf[16..20].copy_from_slice(&self.string_count.to_le_bytes());
        buf[20..24].copy_from_slice(&self.compressed_size.to_le_bytes());
        buf[24..28].copy_from_slice(&self.raw_size.to_le_bytes());
        buf[28..32].copy_from_slice(&self.flags.to_le_bytes());
        buf
    }

    fn decode(data: &[u8; 32]) -> Self {
        Self {
            first_string_id: u64::from_le_bytes(data[0..8].try_into().unwrap()),
            file_offset: u64::from_le_bytes(data[8..16].try_into().unwrap()),
            string_count: u32::from_le_bytes(data[16..20].try_into().unwrap()),
            compressed_size: u32::from_le_bytes(data[20..24].try_into().unwrap()),
            raw_size: u32::from_le_bytes(data[24..28].try_into().unwrap()),
            flags: u32::from_le_bytes(data[28..32].try_into().unwrap()),
        }
    }
}

pub struct SstStore {
    frames_file: File,
    directory: Vec<SstBlockDir>,
    total_strings: u64,
}

impl SstStore {
    pub fn open(frames_path: &Path, idx_path: &Path) -> Result<Self> {
        let mut idx_file = File::open(idx_path)?;
        let mut header = [0u8; HEADER_SIZE];
        idx_file.read_exact(&mut header)?;

        if header[0..4] != SST_IDX_MAGIC {
            return Err(Error::CorruptInput("invalid SST index magic".into()));
        }
        let version = u16::from_le_bytes(header[4..6].try_into().unwrap());
        if version != FORMAT_VERSION {
            return Err(Error::CorruptInput(format!(
                "unsupported SST index version: {version}"
            )));
        }
        let record_count = u64::from_le_bytes(header[8..16].try_into().unwrap());

        let mut directory = Vec::with_capacity(record_count as usize);
        for _ in 0..record_count {
            let mut entry_buf = [0u8; SstBlockDir::SIZE];
            idx_file.read_exact(&mut entry_buf)?;
            directory.push(SstBlockDir::decode(&entry_buf));
        }

        let total_strings = directory
            .last()
            .map(|d| d.first_string_id + d.string_count as u64)
            .unwrap_or(0);

        let frames_file = File::open(frames_path)?;

        Ok(Self {
            frames_file,
            directory,
            total_strings,
        })
    }

    pub fn total_strings(&self) -> u64 {
        self.total_strings
    }

    pub fn block_count(&self) -> usize {
        self.directory.len()
    }

    pub fn directory(&self) -> &[SstBlockDir] {
        &self.directory
    }

    pub fn find_block(&self, string_id: u64) -> Option<usize> {
        self.directory
            .binary_search_by(|dir| {
                if string_id < dir.first_string_id {
                    std::cmp::Ordering::Greater
                } else if string_id >= dir.first_string_id + dir.string_count as u64 {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .ok()
    }

    pub fn read_block(&mut self, block_idx: usize) -> Result<Vec<u8>> {
        let dir = self
            .directory
            .get(block_idx)
            .ok_or_else(|| Error::InvalidStructure(format!("block {block_idx} out of range")))?
            .clone();

        self.frames_file.seek(SeekFrom::Start(dir.file_offset))?;
        let mut compressed = vec![0u8; dir.compressed_size as usize];
        self.frames_file.read_exact(&mut compressed)?;

        let mut decoder = flate2::read::DeflateDecoder::new(&compressed[..]);
        let mut raw = Vec::with_capacity(dir.raw_size as usize);
        decoder.read_to_end(&mut raw).map_err(|e| {
            Error::CorruptInput(format!("SST block {block_idx} decompression failed: {e}"))
        })?;

        Ok(raw)
    }

    pub fn get_string(&mut self, string_id: u64) -> Result<RichString> {
        let block_idx = self.find_block(string_id).ok_or_else(|| {
            Error::InvalidStructure(format!("SST string {string_id} not found"))
        })?;
        let dir = &self.directory[block_idx];
        let local_idx = (string_id - dir.first_string_id) as usize;

        let raw = self.read_block(block_idx)?;
        extract_string_from_block(&raw, local_idx)
    }

    pub fn get_strings_batched(
        &mut self,
        ids: &[u64],
    ) -> Result<Vec<(u64, RichString)>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut block_requests: Vec<(usize, Vec<(u64, usize)>)> = Vec::new();
        for &id in ids {
            let block_idx = match self.find_block(id) {
                Some(b) => b,
                None => continue,
            };
            let local_idx = (id - self.directory[block_idx].first_string_id) as usize;

            if let Some(last) = block_requests.last_mut() {
                if last.0 == block_idx {
                    last.1.push((id, local_idx));
                    continue;
                }
            }
            block_requests.push((block_idx, vec![(id, local_idx)]));
        }

        block_requests.sort_by_key(|(idx, _)| *idx);
        block_requests.dedup_by_key(|(idx, _)| *idx);

        let mut results = Vec::with_capacity(ids.len());

        for (block_idx, string_refs) in &block_requests {
            let raw = self.read_block(*block_idx)?;
            for &(id, local_idx) in string_refs {
                let s = extract_string_from_block(&raw, local_idx)?;
                results.push((id, s));
            }
        }

        Ok(results)
    }

    pub fn write_frames_header(writer: &mut impl Write) -> Result<()> {
        let mut header = [0u8; HEADER_SIZE];
        header[0..4].copy_from_slice(&SST_FRAMES_MAGIC);
        header[4..6].copy_from_slice(&FORMAT_VERSION.to_le_bytes());
        writer.write_all(&header)?;
        Ok(())
    }

    pub fn write_idx(writer: &mut impl Write, directory: &[SstBlockDir]) -> Result<()> {
        let mut header = [0u8; HEADER_SIZE];
        header[0..4].copy_from_slice(&SST_IDX_MAGIC);
        header[4..6].copy_from_slice(&FORMAT_VERSION.to_le_bytes());
        header[8..16].copy_from_slice(&(directory.len() as u64).to_le_bytes());
        writer.write_all(&header)?;

        for entry in directory {
            writer.write_all(&entry.encode())?;
        }
        Ok(())
    }
}

fn extract_string_from_block(block: &[u8], local_idx: usize) -> Result<RichString> {
    if block.len() < 4 {
        return Err(Error::CorruptInput("SST block too small".into()));
    }
    let count = u32::from_le_bytes(block[0..4].try_into().unwrap()) as usize;
    if local_idx >= count {
        return Err(Error::CorruptInput(format!(
            "SST local index {local_idx} >= block count {count}"
        )));
    }

    let offsets_start = 4;
    let offsets_end = offsets_start + (count + 1) * 4;
    if block.len() < offsets_end {
        return Err(Error::CorruptInput("SST block offsets truncated".into()));
    }

    let start_offset = u32::from_le_bytes(
        block[offsets_start + local_idx * 4..offsets_start + local_idx * 4 + 4]
            .try_into()
            .unwrap(),
    ) as usize;
    let end_offset = u32::from_le_bytes(
        block[offsets_start + (local_idx + 1) * 4..offsets_start + (local_idx + 1) * 4 + 4]
            .try_into()
            .unwrap(),
    ) as usize;

    if end_offset > block.len() || start_offset > end_offset {
        return Err(Error::CorruptInput("SST string offset out of range".into()));
    }

    let string_data = &block[start_offset..end_offset];
    match RichString::decode(string_data) {
        Some((s, _)) => Ok(s),
        None => Err(Error::CorruptInput("failed to decode SST string".into())),
    }
}
