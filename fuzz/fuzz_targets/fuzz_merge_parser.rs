#![no_main]
use libfuzzer_sys::fuzz_target;
use xlsxpdf_model::merge::MergeMap;

fuzz_target!(|data: &[u8]| {
    let _ = MergeMap::parse(data);
});
