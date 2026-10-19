#![no_main]
use libfuzzer_sys::fuzz_target;
use xlsxpdf_core::coord::CellRef;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = CellRef::from_a1(s);
    }
});
