use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use xlsxpdf_core::error::{Error, Result};

use crate::limits::PackageLimits;

const ZIP_EOCD_SIGNATURE: u32 = 0x06054b50;
const ZIP_EOCD64_SIGNATURE: u32 = 0x06064b50;
const ZIP_EOCD64_LOCATOR_SIGNATURE: u32 = 0x07064b50;
const ZIP_CD_SIGNATURE: u32 = 0x02014b50;
const ZIP_LOCAL_HEADER_SIGNATURE: u32 = 0x04034b50;

const COMPRESSION_STORED: u16 = 0;
const COMPRESSION_DEFLATED: u16 = 8;

#[derive(Debug, Clone)]
pub struct ZipEntry {
    pub compressed_offset: u64,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
    pub crc32: u32,
    pub compression_method: u16,
    pub flags: u16,
    pub name: String,
}

pub struct ZipArchive<R> {
    reader: R,
    entries: HashMap<String, ZipEntry>,
    entry_order: Vec<String>,
}

impl<R: Read + Seek> ZipArchive<R> {
    pub fn new(mut reader: R, limits: &PackageLimits) -> Result<Self> {
        let eocd = find_eocd(&mut reader)?;
        let (cd_offset, cd_size, entry_count) = parse_eocd(&mut reader, &eocd)?;

        if entry_count > limits.max_entry_count as u64 {
            return Err(Error::Zip(format!(
                "entry count {entry_count} exceeds limit {}",
                limits.max_entry_count
            )));
        }

        let mut entries = HashMap::with_capacity(entry_count as usize);
        let mut entry_order = Vec::with_capacity(entry_count as usize);
        let mut total_uncompressed: u64 = 0;

        reader.seek(SeekFrom::Start(cd_offset))?;
        let mut cd_buf = vec![0u8; cd_size as usize];
        reader.read_exact(&mut cd_buf)?;

        let mut pos = 0;
        for _ in 0..entry_count {
            if pos + 46 > cd_buf.len() {
                return Err(Error::Zip("truncated central directory".into()));
            }

            let sig = u32::from_le_bytes(cd_buf[pos..pos + 4].try_into().unwrap());
            if sig != ZIP_CD_SIGNATURE {
                return Err(Error::Zip("invalid central directory signature".into()));
            }

            let compression_method = u16::from_le_bytes(cd_buf[pos + 10..pos + 12].try_into().unwrap());
            let flags = u16::from_le_bytes(cd_buf[pos + 8..pos + 10].try_into().unwrap());
            let crc32 = u32::from_le_bytes(cd_buf[pos + 16..pos + 20].try_into().unwrap());
            let compressed_size = u32::from_le_bytes(cd_buf[pos + 20..pos + 24].try_into().unwrap()) as u64;
            let uncompressed_size = u32::from_le_bytes(cd_buf[pos + 24..pos + 28].try_into().unwrap()) as u64;
            let name_len = u16::from_le_bytes(cd_buf[pos + 28..pos + 30].try_into().unwrap()) as usize;
            let extra_len = u16::from_le_bytes(cd_buf[pos + 30..pos + 32].try_into().unwrap()) as usize;
            let comment_len = u16::from_le_bytes(cd_buf[pos + 32..pos + 34].try_into().unwrap()) as usize;
            let local_header_offset = u32::from_le_bytes(cd_buf[pos + 42..pos + 46].try_into().unwrap()) as u64;

            let record_len = 46 + name_len + extra_len + comment_len;
            if pos + record_len > cd_buf.len() {
                return Err(Error::Zip("truncated central directory entry".into()));
            }

            let name_bytes = &cd_buf[pos + 46..pos + 46 + name_len];
            let name = String::from_utf8_lossy(name_bytes).into_owned();

            // Handle ZIP64 extended info
            let (compressed_size, uncompressed_size, local_header_offset) =
                parse_zip64_extra(
                    &cd_buf[pos + 46 + name_len..pos + 46 + name_len + extra_len],
                    compressed_size,
                    uncompressed_size,
                    local_header_offset,
                );

            if name.len() > limits.max_path_length {
                return Err(Error::Zip(format!(
                    "entry path too long: {} bytes",
                    name.len()
                )));
            }

            if name.contains("..") {
                return Err(Error::Zip(format!("path traversal detected: {name}")));
            }

            if uncompressed_size > limits.max_single_entry {
                return Err(Error::Zip(format!(
                    "entry '{name}' uncompressed size {uncompressed_size} exceeds limit {}",
                    limits.max_single_entry
                )));
            }

            if compressed_size > 0 {
                let ratio = uncompressed_size.checked_div(compressed_size).unwrap_or(0);
                if ratio > limits.max_compression_ratio as u64 {
                    return Err(Error::Zip(format!(
                        "entry '{name}' compression ratio {ratio} exceeds limit {}",
                        limits.max_compression_ratio
                    )));
                }
            }

            total_uncompressed = total_uncompressed.checked_add(uncompressed_size).ok_or_else(|| {
                Error::Zip("total uncompressed size overflow".into())
            })?;

            if total_uncompressed > limits.max_total_uncompressed {
                return Err(Error::Zip(format!(
                    "total uncompressed size {total_uncompressed} exceeds limit {}",
                    limits.max_total_uncompressed
                )));
            }

            if compression_method != COMPRESSION_STORED && compression_method != COMPRESSION_DEFLATED {
                return Err(Error::Zip(format!(
                    "unsupported compression method {compression_method} for '{name}'"
                )));
            }

            if entries.contains_key(&name) {
                return Err(Error::Zip(format!("duplicate entry: {name}")));
            }

            let entry = ZipEntry {
                compressed_offset: local_header_offset,
                compressed_size,
                uncompressed_size,
                crc32,
                compression_method,
                flags,
                name: name.clone(),
            };

            entry_order.push(name.clone());
            entries.insert(name, entry);
            pos += record_len;
        }

        Ok(Self {
            reader,
            entries,
            entry_order,
        })
    }

    pub fn entry_names(&self) -> &[String] {
        &self.entry_order
    }

    pub fn get_entry(&self, name: &str) -> Option<&ZipEntry> {
        self.entries.get(name)
    }

    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    pub fn read_entry(&mut self, name: &str) -> Result<Vec<u8>> {
        let entry = self.entries.get(name).ok_or_else(|| {
            Error::Zip(format!("entry not found: {name}"))
        })?.clone();

        self.reader.seek(SeekFrom::Start(entry.compressed_offset))?;

        let mut local_header = [0u8; 30];
        self.reader.read_exact(&mut local_header)?;

        let sig = u32::from_le_bytes(local_header[0..4].try_into().unwrap());
        if sig != ZIP_LOCAL_HEADER_SIGNATURE {
            return Err(Error::Zip("invalid local file header".into()));
        }

        let local_name_len = u16::from_le_bytes(local_header[26..28].try_into().unwrap()) as u64;
        let local_extra_len = u16::from_le_bytes(local_header[28..30].try_into().unwrap()) as u64;
        let data_offset = entry.compressed_offset + 30 + local_name_len + local_extra_len;

        self.reader.seek(SeekFrom::Start(data_offset))?;

        let mut compressed = vec![0u8; entry.compressed_size as usize];
        self.reader.read_exact(&mut compressed)?;

        match entry.compression_method {
            COMPRESSION_STORED => {
                verify_crc32(&compressed, entry.crc32, &entry.name)?;
                Ok(compressed)
            }
            COMPRESSION_DEFLATED => {
                let mut decoder = flate2::read::DeflateDecoder::new(&compressed[..]);
                let mut decompressed = Vec::with_capacity(entry.uncompressed_size as usize);
                decoder.read_to_end(&mut decompressed).map_err(|e| {
                    Error::Zip(format!("decompression failed for '{}': {e}", entry.name))
                })?;
                verify_crc32(&decompressed, entry.crc32, &entry.name)?;
                Ok(decompressed)
            }
            _ => Err(Error::Zip(format!(
                "unsupported compression method {}",
                entry.compression_method
            ))),
        }
    }
}

fn verify_crc32(data: &[u8], expected: u32, name: &str) -> Result<()> {
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(data);
    let actual = hasher.finalize();
    if actual != expected {
        return Err(Error::CorruptInput(format!(
            "CRC mismatch for '{name}': expected {expected:#010x}, got {actual:#010x}"
        )));
    }
    Ok(())
}

fn find_eocd<R: Read + Seek>(reader: &mut R) -> Result<u64> {
    let file_len = reader.seek(SeekFrom::End(0))?;
    let search_len = std::cmp::min(file_len, 65536 + 22);
    let search_start = file_len - search_len;

    reader.seek(SeekFrom::Start(search_start))?;
    let mut buf = vec![0u8; search_len as usize];
    reader.read_exact(&mut buf)?;

    for i in (0..buf.len().saturating_sub(3)).rev() {
        let sig = u32::from_le_bytes(buf[i..i + 4].try_into().unwrap());
        if sig == ZIP_EOCD_SIGNATURE {
            return Ok(search_start + i as u64);
        }
    }

    Err(Error::Zip("end of central directory not found".into()))
}

fn parse_eocd<R: Read + Seek>(reader: &mut R, eocd_offset: &u64) -> Result<(u64, u64, u64)> {
    reader.seek(SeekFrom::Start(*eocd_offset))?;
    let mut buf = [0u8; 22];
    reader.read_exact(&mut buf)?;

    let mut cd_size = u32::from_le_bytes(buf[12..16].try_into().unwrap()) as u64;
    let mut cd_offset = u32::from_le_bytes(buf[16..20].try_into().unwrap()) as u64;
    let mut entry_count = u16::from_le_bytes(buf[8..10].try_into().unwrap()) as u64;

    if cd_offset == 0xFFFFFFFF || entry_count == 0xFFFF || cd_size == 0xFFFFFFFF {
        if *eocd_offset >= 20 {
            reader.seek(SeekFrom::Start(eocd_offset - 20))?;
            let mut loc_buf = [0u8; 20];
            reader.read_exact(&mut loc_buf)?;
            let loc_sig = u32::from_le_bytes(loc_buf[0..4].try_into().unwrap());
            if loc_sig == ZIP_EOCD64_LOCATOR_SIGNATURE {
                let eocd64_offset = u64::from_le_bytes(loc_buf[8..16].try_into().unwrap());
                reader.seek(SeekFrom::Start(eocd64_offset))?;
                let mut eocd64_buf = [0u8; 56];
                reader.read_exact(&mut eocd64_buf)?;
                let eocd64_sig = u32::from_le_bytes(eocd64_buf[0..4].try_into().unwrap());
                if eocd64_sig == ZIP_EOCD64_SIGNATURE {
                    entry_count = u64::from_le_bytes(eocd64_buf[32..40].try_into().unwrap());
                    cd_size = u64::from_le_bytes(eocd64_buf[40..48].try_into().unwrap());
                    cd_offset = u64::from_le_bytes(eocd64_buf[48..56].try_into().unwrap());
                }
            }
        }
    }

    Ok((cd_offset, cd_size, entry_count))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_empty_input() {
        let data = vec![];
        let cursor = Cursor::new(data);
        let limits = PackageLimits::default();
        let result = ZipArchive::new(cursor, &limits);
        assert!(result.is_err());
    }

    #[test]
    fn test_too_short_input() {
        let data = vec![0u8; 10];
        let cursor = Cursor::new(data);
        let limits = PackageLimits::default();
        let result = ZipArchive::new(cursor, &limits);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_signature() {
        let data = vec![0u8; 100];
        let cursor = Cursor::new(data);
        let limits = PackageLimits::default();
        let result = ZipArchive::new(cursor, &limits);
        assert!(result.is_err());
    }

    #[test]
    fn test_random_bytes() {
        let data: Vec<u8> = (0..1000).map(|i| (i * 37 + 13) as u8).collect();
        let cursor = Cursor::new(data);
        let limits = PackageLimits::default();
        let result = ZipArchive::new(cursor, &limits);
        assert!(result.is_err());
    }

    #[test]
    fn test_limits_enforcement() {
        let limits = PackageLimits {
            max_entry_count: 0,
            max_total_uncompressed: 0,
            max_single_entry: 0,
            max_compression_ratio: 1,
            max_path_length: 1,
        };
        let data = vec![0u8; 100];
        let cursor = Cursor::new(data);
        let result = ZipArchive::new(cursor, &limits);
        assert!(result.is_err());
    }
}

fn parse_zip64_extra(extra: &[u8], comp_size: u64, uncomp_size: u64, offset: u64) -> (u64, u64, u64) {
    let mut cs = comp_size;
    let mut us = uncomp_size;
    let mut off = offset;

    let mut pos = 0;
    while pos + 4 <= extra.len() {
        let tag = u16::from_le_bytes(extra[pos..pos + 2].try_into().unwrap());
        let size = u16::from_le_bytes(extra[pos + 2..pos + 4].try_into().unwrap()) as usize;
        pos += 4;

        if tag == 0x0001 && pos + size <= extra.len() {
            let data = &extra[pos..pos + size];
            let mut dpos = 0;
            if uncomp_size == 0xFFFFFFFF && dpos + 8 <= data.len() {
                us = u64::from_le_bytes(data[dpos..dpos + 8].try_into().unwrap());
                dpos += 8;
            }
            if comp_size == 0xFFFFFFFF && dpos + 8 <= data.len() {
                cs = u64::from_le_bytes(data[dpos..dpos + 8].try_into().unwrap());
                dpos += 8;
            }
            if offset == 0xFFFFFFFF && dpos + 8 <= data.len() {
                off = u64::from_le_bytes(data[dpos..dpos + 8].try_into().unwrap());
            }
            break;
        }

        pos += size;
    }

    (cs, us, off)
}
