#![no_main]
use libfuzzer_sys::fuzz_target;
use xlsxpdf_model::sheet_geometry::SheetGeometry;

fuzz_target!(|data: &[u8]| {
    let _ = SheetGeometry::parse(data);
});
