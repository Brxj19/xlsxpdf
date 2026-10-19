#![no_main]
use libfuzzer_sys::fuzz_target;
use xlsxpdf_drawing::parse_drawings;

fuzz_target!(|data: &[u8]| {
    let _ = parse_drawings(data);
});
