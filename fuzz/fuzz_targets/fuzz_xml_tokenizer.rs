#![no_main]
use libfuzzer_sys::fuzz_target;
use xlsxpdf_xml::{XmlEvent, XmlReader};

fuzz_target!(|data: &[u8]| {
    let mut reader = XmlReader::new(data);
    loop {
        match reader.next_event() {
            Ok(XmlEvent::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
});
