use std::io::Write;
use std::path::Path;

use flate2::write::DeflateEncoder;
use flate2::Compression;

use xlsxpdf_core::error::{Error, Result};
use xlsxpdf_xml::{XmlEvent, XmlReader};

use crate::rich_string::{RichRun, RichString};
use crate::store::{SstBlockDir, SstStore};

const DEFAULT_BLOCK_SIZE: usize = 256 * 1024;

pub struct SstCompiler {
    block_size: usize,
}

impl SstCompiler {
    pub fn new() -> Self {
        Self {
            block_size: DEFAULT_BLOCK_SIZE,
        }
    }

    pub fn with_block_size(mut self, block_size: usize) -> Self {
        self.block_size = block_size;
        self
    }

    pub fn compile(
        &self,
        sst_xml: &[u8],
        frames_path: &Path,
        idx_path: &Path,
    ) -> Result<CompileStats> {
        let mut frames_file = std::fs::File::create(frames_path)?;
        SstStore::write_frames_header(&mut frames_file)?;
        let mut file_offset: u64 = 32; // after header

        let mut directory: Vec<SstBlockDir> = Vec::new();
        let mut stats = CompileStats::default();

        let strings = parse_sst_xml(sst_xml)?;
        stats.total_strings = strings.len() as u64;

        let mut block_strings: Vec<&RichString> = Vec::new();
        let mut block_raw_size: usize = 0;
        let mut first_string_id: u64 = 0;

        for (i, s) in strings.iter().enumerate() {
            let encoded_size = s.encode().len();

            if block_raw_size + encoded_size > self.block_size && !block_strings.is_empty() {
                let dir = write_block(
                    &mut frames_file,
                    &block_strings,
                    first_string_id,
                    file_offset,
                )?;
                file_offset += dir.compressed_size as u64;
                stats.total_blocks += 1;
                stats.total_compressed += dir.compressed_size as u64;
                stats.total_raw += dir.raw_size as u64;
                directory.push(dir);

                first_string_id = i as u64;
                block_strings.clear();
                block_raw_size = 0;
            }

            block_strings.push(s);
            block_raw_size += encoded_size;
        }

        if !block_strings.is_empty() {
            let dir = write_block(&mut frames_file, &block_strings, first_string_id, file_offset)?;
            stats.total_blocks += 1;
            stats.total_compressed += dir.compressed_size as u64;
            stats.total_raw += dir.raw_size as u64;
            directory.push(dir);
        }

        let mut idx_file = std::fs::File::create(idx_path)?;
        SstStore::write_idx(&mut idx_file, &directory)?;

        Ok(stats)
    }
}

impl Default for SstCompiler {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Default)]
pub struct CompileStats {
    pub total_strings: u64,
    pub total_blocks: u64,
    pub total_raw: u64,
    pub total_compressed: u64,
}

fn write_block(
    writer: &mut impl Write,
    strings: &[&RichString],
    first_string_id: u64,
    file_offset: u64,
) -> Result<SstBlockDir> {
    let count = strings.len() as u32;
    let encoded: Vec<Vec<u8>> = strings.iter().map(|s| s.encode()).collect();

    // Block layout: count (u32) + offsets[count+1] (u32 each) + string payloads
    let offsets_size = (count as usize + 1) * 4;
    let header_size = 4 + offsets_size;
    let payload_size: usize = encoded.iter().map(|e| e.len()).sum();
    let raw_size = header_size + payload_size;

    let mut raw = Vec::with_capacity(raw_size);
    raw.extend_from_slice(&count.to_le_bytes());

    // Write offsets
    let mut offset = header_size as u32;
    for enc in &encoded {
        raw.extend_from_slice(&offset.to_le_bytes());
        offset += enc.len() as u32;
    }
    raw.extend_from_slice(&offset.to_le_bytes()); // end sentinel

    // Write payloads
    for enc in &encoded {
        raw.extend_from_slice(enc);
    }

    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&raw).map_err(|e| Error::Io(e))?;
    let compressed = encoder.finish().map_err(|e| Error::Io(e))?;

    writer.write_all(&compressed)?;

    Ok(SstBlockDir {
        first_string_id,
        file_offset,
        string_count: count,
        compressed_size: compressed.len() as u32,
        raw_size: raw.len() as u32,
        flags: 0,
    })
}

fn parse_sst_xml(xml: &[u8]) -> Result<Vec<RichString>> {
    let mut reader = XmlReader::new(xml);
    let mut strings = Vec::new();
    let mut in_si = false;
    let mut in_t = false;
    let mut in_r = false;
    let mut in_r_t = false;
    let mut current_text = String::new();
    let mut current_runs: Vec<RichRun> = Vec::new();
    let mut run_text = String::new();
    let mut run_font_id: u32 = 0;
    let mut in_r_pr = false;

    loop {
        match reader.next_event()? {
            XmlEvent::StartElement { local_name, attrs: _ } => match local_name.as_str() {
                "si" => {
                    in_si = true;
                    current_text.clear();
                    current_runs.clear();
                }
                "t" if in_si && !in_r => {
                    in_t = true;
                }
                "r" if in_si => {
                    in_r = true;
                    run_text.clear();
                    run_font_id = 0;
                }
                "t" if in_r => {
                    in_r_t = true;
                }
                "rPr" if in_r => {
                    in_r_pr = true;
                }
                "sz" if in_r_pr => {
                    // Font size — could extract but we use font_id = 0 for now
                }
                "rFont" if in_r_pr => {
                    // Font name — could extract
                }
                _ => {}
            },
            XmlEvent::Text(text) => {
                if in_t {
                    current_text.push_str(&text);
                } else if in_r_t {
                    run_text.push_str(&text);
                }
            }
            XmlEvent::EndElement { local_name } => match local_name.as_str() {
                "si" => {
                    if current_runs.is_empty() {
                        strings.push(RichString::plain(std::mem::take(&mut current_text)));
                    } else {
                        strings.push(RichString {
                            text: std::mem::take(&mut current_text),
                            runs: std::mem::take(&mut current_runs),
                        });
                    }
                    in_si = false;
                }
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
                    let start = current_text.len() as u32;
                    current_text.push_str(&run_text);
                    current_runs.push(RichRun {
                        start,
                        font_id: run_font_id,
                    });
                    in_r = false;
                }
                _ => {}
            },
            XmlEvent::Eof => break,
        }
    }

    Ok(strings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_compile_and_read() {
        let sst_xml = br#"<?xml version="1.0" encoding="UTF-8"?>
<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="3" uniqueCount="3">
  <si><t>Hello</t></si>
  <si><t>World</t></si>
  <si><t>Test string number three</t></si>
</sst>"#;

        let dir = TempDir::new().unwrap();
        let frames = dir.path().join("sst.frames");
        let idx = dir.path().join("sst.idx");

        let compiler = SstCompiler::new();
        let stats = compiler.compile(sst_xml, &frames, &idx).unwrap();
        assert_eq!(stats.total_strings, 3);
        assert_eq!(stats.total_blocks, 1);

        let mut store = SstStore::open(&frames, &idx).unwrap();
        assert_eq!(store.total_strings(), 3);

        let s0 = store.get_string(0).unwrap();
        assert_eq!(s0.text, "Hello");
        assert!(s0.is_plain());

        let s1 = store.get_string(1).unwrap();
        assert_eq!(s1.text, "World");

        let s2 = store.get_string(2).unwrap();
        assert_eq!(s2.text, "Test string number three");
    }

    #[test]
    fn test_multiple_blocks() {
        let mut xml = String::from(
            r#"<?xml version="1.0"?><sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">"#,
        );
        for i in 0..1000 {
            xml.push_str(&format!("<si><t>String number {i} with some padding text to make it longer for block splitting purposes abcdefghijklmnop</t></si>"));
        }
        xml.push_str("</sst>");

        let dir = TempDir::new().unwrap();
        let frames = dir.path().join("sst.frames");
        let idx = dir.path().join("sst.idx");

        let compiler = SstCompiler::new().with_block_size(4096);
        let stats = compiler.compile(xml.as_bytes(), &frames, &idx).unwrap();
        assert_eq!(stats.total_strings, 1000);
        assert!(stats.total_blocks > 1);

        let mut store = SstStore::open(&frames, &idx).unwrap();
        assert_eq!(store.total_strings(), 1000);

        let s0 = store.get_string(0).unwrap();
        assert_eq!(s0.text, "String number 0 with some padding text to make it longer for block splitting purposes abcdefghijklmnop");

        let s999 = store.get_string(999).unwrap();
        assert!(s999.text.starts_with("String number 999"));

        // Test batched lookup
        let batch = store.get_strings_batched(&[0, 500, 999]).unwrap();
        assert_eq!(batch.len(), 3);
        assert_eq!(batch[0].0, 0);
        assert_eq!(batch[2].0, 999);
    }
}
