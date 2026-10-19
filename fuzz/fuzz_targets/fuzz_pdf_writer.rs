#![no_main]
use libfuzzer_sys::fuzz_target;
use xlsxpdf_pdf::{ContentBuilder, PdfWriter};

fuzz_target!(|data: &[u8]| {
    if data.len() < 4 {
        return;
    }
    let page_count = (data[0] as usize % 5) + 1;
    let content_data = &data[1..];

    let buf = Vec::new();
    let mut writer = match PdfWriter::new(buf) {
        Ok(w) => w,
        Err(_) => return,
    };
    let font_id = match writer.write_font_resources() {
        Ok(id) => id,
        Err(_) => return,
    };

    for i in 0..page_count {
        let chunk_size = content_data.len() / page_count;
        let start = i * chunk_size;
        let end = if i == page_count - 1 {
            content_data.len()
        } else {
            (i + 1) * chunk_size
        };
        let chunk = &content_data[start..end];
        let _ = writer.write_page(chunk, 612.0, 792.0, font_id);
    }

    let _ = writer.finish();
});
