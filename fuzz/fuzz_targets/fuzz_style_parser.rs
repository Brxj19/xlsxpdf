#![no_main]
use libfuzzer_sys::fuzz_target;
use xlsxpdf_styles::StyleSheet;

fuzz_target!(|data: &[u8]| {
    let _ = StyleSheet::parse(data);
});
