#[derive(Debug, Clone, PartialEq)]
pub struct RichRun {
    pub start: u32,
    pub font_id: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RichString {
    pub text: String,
    pub runs: Vec<RichRun>,
}

impl RichString {
    pub fn plain(text: String) -> Self {
        Self {
            text,
            runs: Vec::new(),
        }
    }

    pub fn is_plain(&self) -> bool {
        self.runs.is_empty()
    }

    pub fn encode(&self) -> Vec<u8> {
        let text_bytes = self.text.as_bytes();
        let run_count = self.runs.len() as u32;

        let mut buf = Vec::with_capacity(4 + text_bytes.len() + 4 + run_count as usize * 8);
        buf.extend_from_slice(&(text_bytes.len() as u32).to_le_bytes());
        buf.extend_from_slice(text_bytes);
        buf.extend_from_slice(&run_count.to_le_bytes());
        for run in &self.runs {
            buf.extend_from_slice(&run.start.to_le_bytes());
            buf.extend_from_slice(&run.font_id.to_le_bytes());
        }
        buf
    }

    pub fn decode(data: &[u8]) -> Option<(Self, usize)> {
        if data.len() < 4 {
            return None;
        }
        let text_len = u32::from_le_bytes(data[0..4].try_into().ok()?) as usize;
        if data.len() < 4 + text_len + 4 {
            return None;
        }
        let text = String::from_utf8_lossy(&data[4..4 + text_len]).into_owned();
        let run_count =
            u32::from_le_bytes(data[4 + text_len..8 + text_len].try_into().ok()?) as usize;
        let runs_start = 8 + text_len;
        if data.len() < runs_start + run_count * 8 {
            return None;
        }
        let mut runs = Vec::with_capacity(run_count);
        for i in 0..run_count {
            let off = runs_start + i * 8;
            let start = u32::from_le_bytes(data[off..off + 4].try_into().ok()?);
            let font_id = u32::from_le_bytes(data[off + 4..off + 8].try_into().ok()?);
            runs.push(RichRun { start, font_id });
        }
        let total = runs_start + run_count * 8;
        Some((Self { text, runs }, total))
    }
}
