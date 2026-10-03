/// An RGBA color, written in files as `#rrggbb` or `#rrggbbaa`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub fn parse(text: &str) -> Option<Color> {
        let hex = text.strip_prefix('#')?;
        if !(hex.len() == 6 || hex.len() == 8) || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        Some(Color { r: byte(0)?, g: byte(2)?, b: byte(4)?, a: if hex.len() == 8 { byte(6)? } else { 255 } })
    }
}

#[cfg(test)]
mod tests {
    use super::Color;

    #[test]
    fn parses_rgb_as_opaque() {
        assert_eq!(Color::parse("#2e3440"), Some(Color { r: 0x2e, g: 0x34, b: 0x40, a: 255 }));
    }

    #[test]
    fn parses_rgba() {
        assert_eq!(Color::parse("#3b425280"), Some(Color { r: 0x3b, g: 0x42, b: 0x52, a: 0x80 }));
    }

    #[test]
    fn accepts_uppercase_hex() {
        assert_eq!(Color::parse("#FFFFFF"), Some(Color { r: 255, g: 255, b: 255, a: 255 }));
    }

    #[test]
    fn rejects_invalid_text() {
        for bad in ["", "#", "2e3440", "#fff", "#12345", "#1234567", "#2e344g", "#ÿÿÿ"] {
            assert_eq!(Color::parse(bad), None, "{bad:?} should be rejected");
        }
    }
}
