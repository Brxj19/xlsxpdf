#![no_main]
use libfuzzer_sys::fuzz_target;
use xlsxpdf_model::print_settings::PrintSettings;

fuzz_target!(|data: &[u8]| {
    let _ = PrintSettings::parse(data);
});
