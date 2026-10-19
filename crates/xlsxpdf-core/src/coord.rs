pub const MAX_ROWS: u32 = 1_048_576;
pub const MAX_COLS: u16 = 16_384;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CellRef {
    pub row: u32,
    pub col: u16,
}

impl CellRef {
    pub fn new(row: u32, col: u16) -> Self {
        Self { row, col }
    }

    pub fn to_key(self) -> u64 {
        ((self.row as u64) << 14) | self.col as u64
    }

    pub fn from_key(key: u64) -> Self {
        Self {
            row: (key >> 14) as u32,
            col: (key & 0x3FFF) as u16,
        }
    }

    pub fn to_a1(&self) -> String {
        let mut col_str = String::new();
        let mut c = self.col as u32;
        loop {
            col_str.insert(0, (b'A' + (c % 26) as u8) as char);
            if c < 26 {
                break;
            }
            c = c / 26 - 1;
        }
        format!("{}{}", col_str, self.row + 1)
    }
}

pub fn parse_cell_ref(input: &[u8]) -> Option<(CellRef, usize)> {
    if input.is_empty() {
        return None;
    }

    let mut pos = 0;

    // Skip optional '$' for absolute column
    if pos < input.len() && input[pos] == b'$' {
        pos += 1;
    }

    // Parse column letters: A=0, B=1, ..., Z=25, AA=26, ...
    let col_start = pos;
    while pos < input.len() && input[pos].is_ascii_alphabetic() {
        pos += 1;
    }

    if pos == col_start {
        return None;
    }

    let mut col: u32 = 0;
    for &b in &input[col_start..pos] {
        let c = b.to_ascii_uppercase() - b'A';
        col = col.checked_mul(26)?.checked_add(c as u32 + 1)?;
    }
    let col = col.checked_sub(1)?;

    if col >= MAX_COLS as u32 {
        return None;
    }

    // Skip optional '$' for absolute row
    if pos < input.len() && input[pos] == b'$' {
        pos += 1;
    }

    // Parse row number
    let row_start = pos;
    let mut row: u32 = 0;
    while pos < input.len() && input[pos].is_ascii_digit() {
        row = row.checked_mul(10)?.checked_add((input[pos] - b'0') as u32)?;
        pos += 1;
    }

    if pos == row_start || row == 0 {
        return None;
    }

    let row = row - 1;
    if row >= MAX_ROWS {
        return None;
    }

    Some((CellRef::new(row, col as u16), pos))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_a1() {
        let (cell, len) = parse_cell_ref(b"A1").unwrap();
        assert_eq!(cell, CellRef::new(0, 0));
        assert_eq!(len, 2);
    }

    #[test]
    fn test_parse_aa500() {
        let (cell, _) = parse_cell_ref(b"AA500").unwrap();
        assert_eq!(cell.col, 26);
        assert_eq!(cell.row, 499);
    }

    #[test]
    fn test_parse_xfd_max() {
        let (cell, _) = parse_cell_ref(b"XFD1048576").unwrap();
        assert_eq!(cell.col, 16383);
        assert_eq!(cell.row, 1_048_575);
    }

    #[test]
    fn test_key_roundtrip() {
        let cell = CellRef::new(100_000, 500);
        let key = cell.to_key();
        assert_eq!(CellRef::from_key(key), cell);
    }

    #[test]
    fn test_to_a1() {
        assert_eq!(CellRef::new(0, 0).to_a1(), "A1");
        assert_eq!(CellRef::new(499, 26).to_a1(), "AA500");
    }

    // M14: Hardening tests

    #[test]
    fn test_empty_input() {
        assert!(parse_cell_ref(b"").is_none());
    }

    #[test]
    fn test_only_letters() {
        assert!(parse_cell_ref(b"ABC").is_none());
    }

    #[test]
    fn test_only_digits() {
        assert!(parse_cell_ref(b"123").is_none());
    }

    #[test]
    fn test_zero_row() {
        assert!(parse_cell_ref(b"A0").is_none());
    }

    #[test]
    fn test_row_too_large() {
        assert!(parse_cell_ref(b"A1048577").is_none());
    }

    #[test]
    fn test_col_too_large() {
        assert!(parse_cell_ref(b"XFD1").is_some()); // XFD = 16383, max valid
        assert!(parse_cell_ref(b"XFE1").is_none()); // XFE = 16384, over limit
    }

    #[test]
    fn test_absolute_refs() {
        let (cell, _) = parse_cell_ref(b"$A$1").unwrap();
        assert_eq!(cell, CellRef::new(0, 0));
        let (cell, _) = parse_cell_ref(b"$B$2").unwrap();
        assert_eq!(cell, CellRef::new(1, 1));
    }

    #[test]
    fn test_special_characters() {
        assert!(parse_cell_ref(b"A1:B2").is_some()); // stops at ':'
        assert!(parse_cell_ref(b"!A1").is_none());
        assert!(parse_cell_ref(b" A1").is_none());
    }

    #[test]
    fn test_overflow_digits() {
        assert!(parse_cell_ref(b"A99999999999999999").is_none());
    }

    #[test]
    fn test_overflow_letters() {
        assert!(parse_cell_ref(b"ZZZZZ1").is_none());
    }
}
