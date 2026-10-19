#![no_main]
use libfuzzer_sys::fuzz_target;
use xlsxpdf_opc::relationships::Relationships;

fuzz_target!(|data: &[u8]| {
    let _ = Relationships::parse(data);
});
