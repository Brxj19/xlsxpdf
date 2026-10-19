#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FontInfo {
    pub name: String,
    pub size_hundredths: u32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub color: Option<Color>,
}

impl Default for FontInfo {
    fn default() -> Self {
        Self {
            name: "Calibri".to_string(),
            size_hundredths: 1100,
            bold: false,
            italic: false,
            underline: false,
            strikethrough: false,
            color: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const BLACK: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };
    pub const WHITE: Self = Self {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };

    pub fn from_rgb_hex(hex: &str) -> Option<Self> {
        let hex = hex.trim_start_matches('#');
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some(Self { r, g, b, a: 255 })
        } else if hex.len() == 8 {
            let a = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let r = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let g = u8::from_str_radix(&hex[4..6], 16).ok()?;
            let b = u8::from_str_radix(&hex[6..8], 16).ok()?;
            Some(Self { r, g, b, a })
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FillPattern {
    None,
    Solid,
    Other(u8),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FillInfo {
    pub pattern: FillPattern,
    pub fg_color: Option<Color>,
    pub bg_color: Option<Color>,
}

impl Default for FillInfo {
    fn default() -> Self {
        Self {
            pattern: FillPattern::None,
            fg_color: None,
            bg_color: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BorderStyle {
    None,
    Thin,
    Medium,
    Thick,
    Dashed,
    Dotted,
    Double,
    Hair,
    Other(u8),
}

impl BorderStyle {
    pub fn from_str(s: &str) -> Self {
        match s {
            "thin" => Self::Thin,
            "medium" => Self::Medium,
            "thick" => Self::Thick,
            "dashed" => Self::Dashed,
            "dotted" => Self::Dotted,
            "double" => Self::Double,
            "hair" => Self::Hair,
            "none" | "" => Self::None,
            _ => Self::Other(0),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BorderSide {
    pub style: BorderStyle,
    pub color: Option<Color>,
}

impl Default for BorderSide {
    fn default() -> Self {
        Self {
            style: BorderStyle::None,
            color: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct BorderInfo {
    pub left: BorderSide,
    pub right: BorderSide,
    pub top: BorderSide,
    pub bottom: BorderSide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HorizontalAlignment {
    General,
    Left,
    Center,
    Right,
    Fill,
    Justify,
    CenterContinuous,
    Distributed,
}

impl HorizontalAlignment {
    pub fn from_str(s: &str) -> Self {
        match s {
            "left" => Self::Left,
            "center" => Self::Center,
            "right" => Self::Right,
            "fill" => Self::Fill,
            "justify" => Self::Justify,
            "centerContinuous" => Self::CenterContinuous,
            "distributed" => Self::Distributed,
            _ => Self::General,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VerticalAlignment {
    Top,
    Center,
    Bottom,
    Justify,
    Distributed,
}

impl VerticalAlignment {
    pub fn from_str(s: &str) -> Self {
        match s {
            "top" => Self::Top,
            "center" => Self::Center,
            "justify" => Self::Justify,
            "distributed" => Self::Distributed,
            _ => Self::Bottom,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AlignmentInfo {
    pub horizontal: HorizontalAlignment,
    pub vertical: VerticalAlignment,
    pub wrap_text: bool,
    pub shrink_to_fit: bool,
    pub text_rotation: u16,
    pub indent: u8,
}

impl Default for AlignmentInfo {
    fn default() -> Self {
        Self {
            horizontal: HorizontalAlignment::General,
            vertical: VerticalAlignment::Bottom,
            wrap_text: false,
            shrink_to_fit: false,
            text_rotation: 0,
            indent: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CanonicalStyle {
    pub font_id: u32,
    pub fill_id: u32,
    pub border_id: u32,
    pub num_fmt_id: u32,
    pub alignment: AlignmentInfo,
}
