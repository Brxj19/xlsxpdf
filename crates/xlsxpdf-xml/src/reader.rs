use xlsxpdf_core::error::{Error, Result};

const MAX_ELEMENT_DEPTH: usize = 256;
const MAX_ATTR_SIZE: usize = 1 << 20;

#[derive(Debug, Clone, PartialEq)]
pub enum XmlEvent {
    StartElement {
        local_name: String,
        attrs: Vec<(String, String)>,
    },
    EndElement {
        local_name: String,
    },
    Text(String),
    Eof,
}

pub struct XmlReader<'a> {
    data: &'a [u8],
    pos: usize,
    depth: usize,
    pending_end: Option<String>,
}

impl<'a> XmlReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            depth: 0,
            pending_end: None,
        }
    }

    pub fn next_event(&mut self) -> Result<XmlEvent> {
        if let Some(name) = self.pending_end.take() {
            return Ok(XmlEvent::EndElement { local_name: name });
        }

        self.skip_whitespace_and_text_prefix();

        if self.pos >= self.data.len() {
            return Ok(XmlEvent::Eof);
        }

        // Check if there's text content before the next tag
        if self.data[self.pos] != b'<' {
            return self.read_text();
        }

        // Skip XML declaration, processing instructions, comments, DTD
        loop {
            if self.pos >= self.data.len() {
                return Ok(XmlEvent::Eof);
            }
            if self.pos + 1 >= self.data.len() {
                return Ok(XmlEvent::Eof);
            }

            if self.data[self.pos] == b'<' && self.data[self.pos + 1] == b'?' {
                self.skip_until(b"?>")?;
                self.skip_whitespace();
                continue;
            }

            if self.pos + 3 < self.data.len()
                && &self.data[self.pos..self.pos + 4] == b"<!--"
            {
                self.skip_until(b"-->")?;
                self.skip_whitespace();
                continue;
            }

            if self.pos + 1 < self.data.len()
                && self.data[self.pos] == b'<'
                && self.data[self.pos + 1] == b'!'
            {
                self.skip_until(b">")?;
                self.skip_whitespace();
                continue;
            }

            break;
        }

        if self.pos >= self.data.len() {
            return Ok(XmlEvent::Eof);
        }

        if self.data[self.pos] != b'<' {
            return self.read_text();
        }

        self.pos += 1;

        if self.pos < self.data.len() && self.data[self.pos] == b'/' {
            self.pos += 1;
            return self.read_end_element();
        }

        self.read_start_element()
    }

    fn read_start_element(&mut self) -> Result<XmlEvent> {
        if self.depth >= MAX_ELEMENT_DEPTH {
            return Err(Error::Xml(format!("element depth exceeds {MAX_ELEMENT_DEPTH}")));
        }

        let name = self.read_name()?;
        let local_name = strip_prefix(&name);
        let mut attrs = Vec::new();

        loop {
            self.skip_whitespace();
            if self.pos >= self.data.len() {
                return Err(Error::Xml("unexpected end in element".into()));
            }

            if self.data[self.pos] == b'/' {
                self.pos += 1;
                self.expect(b'>')?;
                let name = local_name.to_string();
                self.pending_end = Some(name.clone());
                return Ok(XmlEvent::StartElement {
                    local_name: name,
                    attrs,
                });
            }

            if self.data[self.pos] == b'>' {
                self.pos += 1;
                self.depth += 1;
                return Ok(XmlEvent::StartElement {
                    local_name: local_name.to_string(),
                    attrs,
                });
            }

            let attr_name = self.read_name()?;
            let attr_local = strip_prefix(&attr_name);
            self.skip_whitespace();
            self.expect(b'=')?;
            self.skip_whitespace();
            let value = self.read_attr_value()?;
            attrs.push((attr_local.to_string(), value));
        }
    }

    fn read_end_element(&mut self) -> Result<XmlEvent> {
        let name = self.read_name()?;
        let local_name = strip_prefix(&name);
        self.skip_whitespace();
        self.expect(b'>')?;
        self.depth = self.depth.saturating_sub(1);
        Ok(XmlEvent::EndElement {
            local_name: local_name.to_string(),
        })
    }

    fn read_text(&mut self) -> Result<XmlEvent> {
        let start = self.pos;
        while self.pos < self.data.len() && self.data[self.pos] != b'<' {
            self.pos += 1;
        }
        let raw = &self.data[start..self.pos];
        let text = unescape_xml_text(raw)?;
        Ok(XmlEvent::Text(text))
    }

    fn read_name(&mut self) -> Result<String> {
        let start = self.pos;
        while self.pos < self.data.len() {
            let b = self.data[self.pos];
            if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r'
                || b == b'>' || b == b'/' || b == b'='
            {
                break;
            }
            self.pos += 1;
        }
        if self.pos == start {
            return Err(Error::Xml("expected element/attribute name".into()));
        }
        Ok(String::from_utf8_lossy(&self.data[start..self.pos]).into_owned())
    }

    fn read_attr_value(&mut self) -> Result<String> {
        if self.pos >= self.data.len() {
            return Err(Error::Xml("expected attribute value".into()));
        }
        let quote = self.data[self.pos];
        if quote != b'"' && quote != b'\'' {
            return Err(Error::Xml("expected quote for attribute value".into()));
        }
        self.pos += 1;
        let start = self.pos;
        while self.pos < self.data.len() && self.data[self.pos] != quote {
            self.pos += 1;
            if self.pos - start > MAX_ATTR_SIZE {
                return Err(Error::Xml(format!("attribute value exceeds {MAX_ATTR_SIZE} bytes")));
            }
        }
        if self.pos >= self.data.len() {
            return Err(Error::Xml("unterminated attribute value".into()));
        }
        let value = unescape_xml_text(&self.data[start..self.pos])?;
        self.pos += 1;
        Ok(value)
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.data.len() {
            match self.data[self.pos] {
                b' ' | b'\t' | b'\n' | b'\r' => self.pos += 1,
                _ => break,
            }
        }
    }

    fn skip_whitespace_and_text_prefix(&mut self) {
        // Only skip leading whitespace before tags, not text content
    }

    fn skip_until(&mut self, pattern: &[u8]) -> Result<()> {
        while self.pos + pattern.len() <= self.data.len() {
            if &self.data[self.pos..self.pos + pattern.len()] == pattern {
                self.pos += pattern.len();
                return Ok(());
            }
            self.pos += 1;
        }
        Err(Error::Xml(format!(
            "unterminated construct, expected '{}'",
            String::from_utf8_lossy(pattern)
        )))
    }

    fn expect(&mut self, byte: u8) -> Result<()> {
        if self.pos >= self.data.len() || self.data[self.pos] != byte {
            return Err(Error::Xml(format!("expected '{}'", byte as char)));
        }
        self.pos += 1;
        Ok(())
    }
}

fn strip_prefix(name: &str) -> &str {
    name.rsplit_once(':').map_or(name, |(_, local)| local)
}

fn unescape_xml_text(raw: &[u8]) -> Result<String> {
    let s = std::str::from_utf8(raw)
        .map_err(|e| Error::Xml(format!("invalid UTF-8: {e}")))?;

    if !s.contains('&') {
        return Ok(s.to_string());
    }

    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c != '&' {
            result.push(c);
            continue;
        }

        let mut entity = String::new();
        for ec in chars.by_ref() {
            if ec == ';' {
                break;
            }
            entity.push(ec);
        }

        match entity.as_str() {
            "amp" => result.push('&'),
            "lt" => result.push('<'),
            "gt" => result.push('>'),
            "quot" => result.push('"'),
            "apos" => result.push('\''),
            s if s.starts_with('#') => {
                let codepoint = if s.starts_with("#x") || s.starts_with("#X") {
                    u32::from_str_radix(&s[2..], 16).ok()
                } else {
                    s[1..].parse::<u32>().ok()
                };
                match codepoint.and_then(char::from_u32) {
                    Some(c) => result.push(c),
                    None => {
                        return Err(Error::Xml(format!("invalid character reference: &{entity};")));
                    }
                }
            }
            _ => {
                return Err(Error::Xml(format!("unknown entity: &{entity};")));
            }
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_element() {
        let xml = b"<root><child attr=\"value\">text</child></root>";
        let mut reader = XmlReader::new(xml);

        let e = reader.next_event().unwrap();
        assert!(matches!(e, XmlEvent::StartElement { ref local_name, .. } if local_name == "root"));

        let e = reader.next_event().unwrap();
        match e {
            XmlEvent::StartElement { local_name, attrs } => {
                assert_eq!(local_name, "child");
                assert_eq!(attrs, vec![("attr".to_string(), "value".to_string())]);
            }
            _ => panic!("expected StartElement"),
        }

        let e = reader.next_event().unwrap();
        assert_eq!(e, XmlEvent::Text("text".into()));

        let e = reader.next_event().unwrap();
        assert!(matches!(e, XmlEvent::EndElement { ref local_name } if local_name == "child"));

        let e = reader.next_event().unwrap();
        assert!(matches!(e, XmlEvent::EndElement { ref local_name } if local_name == "root"));
    }

    #[test]
    fn test_self_closing() {
        let xml = b"<item key=\"val\"/>";
        let mut reader = XmlReader::new(xml);

        let e = reader.next_event().unwrap();
        match e {
            XmlEvent::StartElement { local_name, attrs } => {
                assert_eq!(local_name, "item");
                assert_eq!(attrs.len(), 1);
            }
            _ => panic!("expected StartElement"),
        }

        let e = reader.next_event().unwrap();
        assert!(matches!(e, XmlEvent::EndElement { ref local_name } if local_name == "item"));

        let e = reader.next_event().unwrap();
        assert_eq!(e, XmlEvent::Eof);
    }

    #[test]
    fn test_namespace_stripping() {
        let xml = b"<x:sheet xmlns:x=\"foo\"><x:row/></x:sheet>";
        let mut reader = XmlReader::new(xml);

        let e = reader.next_event().unwrap();
        assert!(matches!(e, XmlEvent::StartElement { ref local_name, .. } if local_name == "sheet"));

        let e = reader.next_event().unwrap();
        assert!(matches!(e, XmlEvent::StartElement { ref local_name, .. } if local_name == "row"));
    }

    // M14: Hardening tests

    #[test]
    fn test_empty_input() {
        let mut reader = XmlReader::new(b"");
        assert_eq!(reader.next_event().unwrap(), XmlEvent::Eof);
    }

    #[test]
    fn test_truncated_tag() {
        let mut reader = XmlReader::new(b"<foo");
        let _ = reader.next_event(); // should not panic
    }

    #[test]
    fn test_unclosed_attribute() {
        let mut reader = XmlReader::new(b"<foo bar=\"baz");
        let _ = reader.next_event(); // should return error, not panic
    }

    #[test]
    fn test_deeply_nested() {
        let mut xml = Vec::new();
        for _ in 0..300 {
            xml.extend_from_slice(b"<a>");
        }
        let mut reader = XmlReader::new(&xml);
        let mut got_error = false;
        for _ in 0..300 {
            match reader.next_event() {
                Err(_) => { got_error = true; break; }
                Ok(XmlEvent::Eof) => break,
                _ => {}
            }
        }
        assert!(got_error, "should reject excessive nesting");
    }

    #[test]
    fn test_null_bytes() {
        let xml = b"<foo>\x00\x00\x00</foo>";
        let mut reader = XmlReader::new(xml);
        loop {
            match reader.next_event() {
                Ok(XmlEvent::Eof) | Err(_) => break,
                _ => {}
            }
        }
    }

    #[test]
    fn test_invalid_utf8() {
        let xml = b"<a>\xff\xfe</a>";
        let mut reader = XmlReader::new(xml);
        loop {
            match reader.next_event() {
                Ok(XmlEvent::Eof) | Err(_) => break,
                _ => {}
            }
        }
    }

    #[test]
    fn test_entity_bomb() {
        let xml = b"<a>&amp;&amp;&amp;&amp;&amp;&amp;&amp;&amp;&amp;&amp;</a>";
        let mut reader = XmlReader::new(xml);
        loop {
            match reader.next_event() {
                Ok(XmlEvent::Eof) | Err(_) => break,
                _ => {}
            }
        }
    }

    #[test]
    fn test_malformed_comment() {
        let xml = b"<!-- unclosed comment";
        let mut reader = XmlReader::new(xml);
        let _ = reader.next_event(); // should not hang
    }

    #[test]
    fn test_malformed_pi() {
        let xml = b"<?xml version";
        let mut reader = XmlReader::new(xml);
        let _ = reader.next_event(); // should not hang
    }
}
