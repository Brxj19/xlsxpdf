/// EMU (English Metric Units) — the authoritative coordinate type.
/// 1 inch = 914400 EMU, 1 cm = 360000 EMU, 1 pt = 12700 EMU.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Emu(pub i64);

impl Emu {
    pub const ZERO: Self = Self(0);
    pub const PER_INCH: i64 = 914_400;
    pub const PER_CM: i64 = 360_000;
    pub const PER_POINT: i64 = 12_700;

    pub fn from_inches(inches: f64) -> Self {
        Self((inches * Self::PER_INCH as f64) as i64)
    }

    pub fn from_points(points: f64) -> Self {
        Self((points * Self::PER_POINT as f64) as i64)
    }

    pub fn to_points(self) -> f64 {
        self.0 as f64 / Self::PER_POINT as f64
    }
}

/// 26.6 fixed-point for typographic metrics (FreeType compatible).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Fixed26_6(pub i32);

impl Fixed26_6 {
    pub const ZERO: Self = Self(0);

    pub fn from_f64(v: f64) -> Self {
        Self((v * 64.0) as i32)
    }

    pub fn to_f64(self) -> f64 {
        self.0 as f64 / 64.0
    }

    pub fn ceil_to_pixels(self) -> i32 {
        (self.0 + 63) >> 6
    }
}

/// Excel logical column width in 1/256 character units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ColumnWidth256(pub u32);
