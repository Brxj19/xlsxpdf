use std::io::Read;

pub struct DecodedPng {
    pub width: u32,
    pub height: u32,
    pub rgb_data: Vec<u8>,
    pub alpha_data: Option<Vec<u8>>,
}

pub fn decode_png_to_rgb(data: &[u8]) -> Option<DecodedPng> {
    if data.len() < 8 || &data[0..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }

    let mut pos = 8;
    let mut width: u32 = 0;
    let mut height: u32 = 0;
    let mut bit_depth: u8 = 0;
    let mut color_type: u8 = 0;
    let mut idat_chunks: Vec<u8> = Vec::new();
    let mut palette: Vec<(u8, u8, u8)> = Vec::new();
    let mut _trns_gray: Option<u16> = None;
    let mut _trns_rgb: Option<(u16, u16, u16)> = None;
    let mut _trns_palette: Vec<u8> = Vec::new();

    while pos + 12 <= data.len() {
        let chunk_len = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        let chunk_type = &data[pos + 4..pos + 8];
        let chunk_data_start = pos + 8;

        if chunk_data_start + chunk_len > data.len() {
            break;
        }

        match chunk_type {
            b"IHDR" => {
                if chunk_len < 13 {
                    return None;
                }
                let d = &data[chunk_data_start..];
                width = u32::from_be_bytes([d[0], d[1], d[2], d[3]]);
                height = u32::from_be_bytes([d[4], d[5], d[6], d[7]]);
                bit_depth = d[8];
                color_type = d[9];
                let interlace = d[12];
                if interlace != 0 {
                    return None;
                }
            }
            b"PLTE" => {
                let d = &data[chunk_data_start..chunk_data_start + chunk_len];
                for i in (0..d.len()).step_by(3) {
                    if i + 2 < d.len() {
                        palette.push((d[i], d[i + 1], d[i + 2]));
                    }
                }
            }
            b"tRNS" => {
                let d = &data[chunk_data_start..chunk_data_start + chunk_len];
                match color_type {
                    0 if d.len() >= 2 => {
                        _trns_gray = Some(u16::from_be_bytes([d[0], d[1]]));
                    }
                    2 if d.len() >= 6 => {
                        _trns_rgb = Some((
                            u16::from_be_bytes([d[0], d[1]]),
                            u16::from_be_bytes([d[2], d[3]]),
                            u16::from_be_bytes([d[4], d[5]]),
                        ));
                    }
                    3 => {
                        _trns_palette = d.to_vec();
                    }
                    _ => {}
                }
            }
            b"IDAT" => {
                idat_chunks.extend_from_slice(&data[chunk_data_start..chunk_data_start + chunk_len]);
            }
            b"IEND" => break,
            _ => {}
        }

        pos = chunk_data_start + chunk_len + 4; // +4 for CRC
    }

    if width == 0 || height == 0 || idat_chunks.is_empty() {
        return None;
    }

    // Only support 8-bit and 16-bit (downsampled to 8)
    if bit_depth != 8 && bit_depth != 16 && !(bit_depth <= 8 && color_type == 3) && !(bit_depth <= 8 && color_type == 0) {
        return None;
    }

    let mut decoder = flate2::read::ZlibDecoder::new(&idat_chunks[..]);
    let mut raw = Vec::new();
    if decoder.read_to_end(&mut raw).is_err() {
        return None;
    }

    let channels: usize = match color_type {
        0 => 1, // grayscale
        2 => 3, // RGB
        3 => 1, // indexed
        4 => 2, // grayscale + alpha
        6 => 4, // RGBA
        _ => return None,
    };

    let sample_bytes = if bit_depth == 16 { 2 } else { 1 };
    let stride = width as usize * channels * sample_bytes;
    let bpp = channels * sample_bytes; // bytes per pixel for filtering

    // For sub-byte depths (1, 2, 4 bit), compute row stride differently
    let row_stride = if bit_depth < 8 {
        (width as usize * bit_depth as usize + 7) / 8
    } else {
        stride
    };

    let expected_len = height as usize * (1 + row_stride);
    if raw.len() < expected_len {
        return None;
    }

    // Unfilter in place
    let mut prev_row = vec![0u8; row_stride];
    let mut unfiltered = Vec::with_capacity(height as usize * row_stride);

    for y in 0..height as usize {
        let row_start = y * (1 + row_stride);
        let filter_type = raw[row_start];
        let row_data = &raw[row_start + 1..row_start + 1 + row_stride];

        let mut current_row = row_data.to_vec();

        match filter_type {
            0 => {} // None
            1 => {
                // Sub
                for i in bpp..row_stride {
                    current_row[i] = current_row[i].wrapping_add(current_row[i - bpp]);
                }
            }
            2 => {
                // Up
                for i in 0..row_stride {
                    current_row[i] = current_row[i].wrapping_add(prev_row[i]);
                }
            }
            3 => {
                // Average
                for i in 0..row_stride {
                    let a = if i >= bpp { current_row[i - bpp] as u16 } else { 0 };
                    let b = prev_row[i] as u16;
                    current_row[i] = current_row[i].wrapping_add(((a + b) / 2) as u8);
                }
            }
            4 => {
                // Paeth
                for i in 0..row_stride {
                    let a = if i >= bpp { current_row[i - bpp] as i32 } else { 0 };
                    let b = prev_row[i] as i32;
                    let c = if i >= bpp { prev_row[i - bpp] as i32 } else { 0 };
                    current_row[i] = current_row[i].wrapping_add(paeth(a, b, c) as u8);
                }
            }
            _ => return None,
        }

        unfiltered.extend_from_slice(&current_row);
        prev_row = current_row;
    }

    // Convert to RGB, optionally extracting alpha
    let pixel_count = (width * height) as usize;
    let mut rgb = Vec::with_capacity(pixel_count * 3);
    let mut alpha: Option<Vec<u8>> = if color_type == 4 || color_type == 6 {
        Some(Vec::with_capacity(pixel_count))
    } else {
        None
    };

    match color_type {
        0 if bit_depth == 8 => {
            for &g in &unfiltered {
                rgb.push(g);
                rgb.push(g);
                rgb.push(g);
            }
        }
        0 if bit_depth == 16 => {
            for i in (0..unfiltered.len()).step_by(2) {
                let g = unfiltered[i]; // high byte
                rgb.push(g);
                rgb.push(g);
                rgb.push(g);
            }
        }
        0 if bit_depth < 8 => {
            let max_val = (1u16 << bit_depth) - 1;
            for y in 0..height as usize {
                for x in 0..width as usize {
                    let byte_idx = y * row_stride + (x * bit_depth as usize) / 8;
                    let bit_offset = 8 - bit_depth as usize - (x * bit_depth as usize) % 8;
                    let val = (unfiltered[byte_idx] >> bit_offset) & (max_val as u8);
                    let g = (val as u16 * 255 / max_val) as u8;
                    rgb.push(g);
                    rgb.push(g);
                    rgb.push(g);
                }
            }
        }
        2 if bit_depth == 8 => {
            rgb = unfiltered;
        }
        2 if bit_depth == 16 => {
            for i in (0..unfiltered.len()).step_by(6) {
                rgb.push(unfiltered[i]);     // R high byte
                rgb.push(unfiltered[i + 2]); // G high byte
                rgb.push(unfiltered[i + 4]); // B high byte
            }
        }
        3 => {
            // Indexed color
            if palette.is_empty() {
                return None;
            }
            if bit_depth == 8 {
                for &idx in &unfiltered {
                    let (r, g, b) = palette.get(idx as usize).copied().unwrap_or((0, 0, 0));
                    rgb.push(r);
                    rgb.push(g);
                    rgb.push(b);
                }
            } else {
                let max_val = (1u16 << bit_depth) - 1;
                for y in 0..height as usize {
                    for x in 0..width as usize {
                        let byte_idx = y * row_stride + (x * bit_depth as usize) / 8;
                        let bit_offset = 8 - bit_depth as usize - (x * bit_depth as usize) % 8;
                        let idx = (unfiltered[byte_idx] >> bit_offset) & (max_val as u8);
                        let (r, g, b) = palette.get(idx as usize).copied().unwrap_or((0, 0, 0));
                        rgb.push(r);
                        rgb.push(g);
                        rgb.push(b);
                    }
                }
            }
        }
        4 if bit_depth == 8 => {
            let alpha_buf = alpha.get_or_insert_with(|| Vec::with_capacity(pixel_count));
            for i in (0..unfiltered.len()).step_by(2) {
                let g = unfiltered[i];
                rgb.push(g);
                rgb.push(g);
                rgb.push(g);
                alpha_buf.push(unfiltered[i + 1]);
            }
        }
        4 if bit_depth == 16 => {
            let alpha_buf = alpha.get_or_insert_with(|| Vec::with_capacity(pixel_count));
            for i in (0..unfiltered.len()).step_by(4) {
                let g = unfiltered[i];
                rgb.push(g);
                rgb.push(g);
                rgb.push(g);
                alpha_buf.push(unfiltered[i + 2]);
            }
        }
        6 if bit_depth == 8 => {
            let alpha_buf = alpha.get_or_insert_with(|| Vec::with_capacity(pixel_count));
            for i in (0..unfiltered.len()).step_by(4) {
                rgb.push(unfiltered[i]);
                rgb.push(unfiltered[i + 1]);
                rgb.push(unfiltered[i + 2]);
                alpha_buf.push(unfiltered[i + 3]);
            }
        }
        6 if bit_depth == 16 => {
            let alpha_buf = alpha.get_or_insert_with(|| Vec::with_capacity(pixel_count));
            for i in (0..unfiltered.len()).step_by(8) {
                rgb.push(unfiltered[i]);     // R high byte
                rgb.push(unfiltered[i + 2]); // G high byte
                rgb.push(unfiltered[i + 4]); // B high byte
                alpha_buf.push(unfiltered[i + 6]); // A high byte
            }
        }
        _ => return None,
    }

    if rgb.len() != pixel_count * 3 {
        return None;
    }

    // Drop alpha if all pixels are fully opaque
    let final_alpha = alpha.and_then(|a| {
        if a.iter().all(|&v| v == 255) {
            None
        } else {
            Some(a)
        }
    });

    Some(DecodedPng {
        width,
        height,
        rgb_data: rgb,
        alpha_data: final_alpha,
    })
}

fn paeth(a: i32, b: i32, c: i32) -> i32 {
    let p = a + b - c;
    let pa = (p - a).abs();
    let pb = (p - b).abs();
    let pc = (p - c).abs();
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_not_png() {
        assert!(decode_png_to_rgb(b"not a png").is_none());
        assert!(decode_png_to_rgb(b"").is_none());
    }

    #[test]
    fn test_truncated_png() {
        assert!(decode_png_to_rgb(b"\x89PNG\r\n\x1a\n").is_none());
    }

    #[test]
    fn test_minimal_valid_png() {
        // 1x1 red pixel PNG (8-bit RGB, filter=None)
        let png = create_test_png(1, 1, &[0, 255, 0, 0]); // filter byte + RGB
        let result = decode_png_to_rgb(&png);
        assert!(result.is_some());
        let decoded = result.unwrap();
        assert_eq!(decoded.width, 1);
        assert_eq!(decoded.height, 1);
        assert_eq!(decoded.rgb_data, vec![255, 0, 0]);
    }

    fn create_test_png(width: u32, height: u32, raw_data: &[u8]) -> Vec<u8> {
        use std::io::Write;
        let mut out = Vec::new();

        // PNG signature
        out.extend_from_slice(b"\x89PNG\r\n\x1a\n");

        // IHDR chunk
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&width.to_be_bytes());
        ihdr.extend_from_slice(&height.to_be_bytes());
        ihdr.push(8); // bit depth
        ihdr.push(2); // color type (RGB)
        ihdr.push(0); // compression
        ihdr.push(0); // filter
        ihdr.push(0); // interlace
        write_chunk(&mut out, b"IHDR", &ihdr);

        // IDAT chunk - zlib compress the raw data
        let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(raw_data).unwrap();
        let compressed = encoder.finish().unwrap();
        write_chunk(&mut out, b"IDAT", &compressed);

        // IEND chunk
        write_chunk(&mut out, b"IEND", &[]);

        out
    }

    fn write_chunk(out: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(chunk_type);
        out.extend_from_slice(data);
        let mut crc_data = Vec::new();
        crc_data.extend_from_slice(chunk_type);
        crc_data.extend_from_slice(data);
        let crc = crc32_compute(&crc_data);
        out.extend_from_slice(&crc.to_be_bytes());
    }

    fn crc32_compute(data: &[u8]) -> u32 {
        let mut crc: u32 = 0xFFFFFFFF;
        for &byte in data {
            crc ^= byte as u32;
            for _ in 0..8 {
                if crc & 1 != 0 {
                    crc = (crc >> 1) ^ 0xEDB88320;
                } else {
                    crc >>= 1;
                }
            }
        }
        !crc
    }

    #[test]
    fn test_2x2_rgb_png() {
        // 2x2 RGB image: red, green, blue, white
        let mut raw = Vec::new();
        // Row 0: filter=0, then R G B R G B
        raw.push(0); raw.extend_from_slice(&[255, 0, 0, 0, 255, 0]);
        // Row 1: filter=0
        raw.push(0); raw.extend_from_slice(&[0, 0, 255, 255, 255, 255]);

        let png = create_test_png(2, 2, &raw);
        let decoded = decode_png_to_rgb(&png).unwrap();
        assert_eq!(decoded.width, 2);
        assert_eq!(decoded.height, 2);
        assert_eq!(decoded.rgb_data, vec![
            255, 0, 0, 0, 255, 0,
            0, 0, 255, 255, 255, 255,
        ]);
    }
}
