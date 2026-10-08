//! Color arithmetic for themes, and (Task 2) the rules that work out the colors a theme
//! leaves out. Rounds exactly like the design's `gezik-tokens.js`.

use crate::Color;

pub const WHITE: Color = Color { r: 255, g: 255, b: 255, a: 255 };
pub const BLACK: Color = Color { r: 0, g: 0, b: 0, a: 255 };

/// `a + (b - a) * t` per channel, rounded half up; the result is opaque.
pub fn mix(a: Color, b: Color, t: f64) -> Color {
    let channel = |x: u8, y: u8| {
        let (x, y) = (f64::from(x), f64::from(y));
        (x + (y - x) * t).round().clamp(0.0, 255.0) as u8
    };
    Color { r: channel(a.r, b.r), g: channel(a.g, b.g), b: channel(a.b, b.b), a: 255 }
}

/// `c` with opacity `p` (0..=1).
pub fn with_alpha(c: Color, p: f64) -> Color {
    Color { a: (p * 255.0).round().clamp(0.0, 255.0) as u8, ..c }
}

/// WCAG relative luminance; the alpha is ignored.
pub fn luminance(c: Color) -> f64 {
    let linear = |v: u8| {
        let v = f64::from(v) / 255.0;
        if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * linear(c.r) + 0.7152 * linear(c.g) + 0.0722 * linear(c.b)
}

/// WCAG contrast ratio of two opaque colors.
pub fn contrast(a: Color, b: Color) -> f64 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// `c` drawn over `background`, as one opaque color.
pub fn flatten(c: Color, background: Color) -> Color {
    if c.a == 255 { c } else { mix(background, Color { a: 255, ..c }, f64::from(c.a) / 255.0) }
}

/// Whether rules use their dark variant for a theme with this background.
pub fn is_dark(background: Color) -> bool {
    luminance(background) < 0.2
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(text: &str) -> Color {
        Color::parse(text).unwrap()
    }

    #[test]
    fn mix_rounds_like_the_design() {
        assert_eq!(mix(hex("#ffffff"), hex("#c2410c"), 0.15), hex("#f6e3db"));
        assert_eq!(mix(hex("#1b1c20"), hex("#ff8f57"), 0.30), hex("#5f3f31"));
        assert_eq!(mix(hex("#c2410c"), BLACK, 0.12), hex("#ab390b"));
        assert_eq!(mix(hex("#ff8f57"), WHITE, 0.24), hex("#ffaa7f"));
    }

    #[test]
    fn alpha_is_rounded_to_a_byte() {
        assert_eq!(with_alpha(hex("#c2410c"), 0.14), hex("#c2410c24"));
        assert_eq!(with_alpha(hex("#ff8f57"), 0.28), hex("#ff8f5747"));
        assert_eq!(with_alpha(WHITE, 0.06), hex("#ffffff0f"));
    }

    #[test]
    fn luminance_and_contrast_follow_wcag() {
        assert_eq!(luminance(WHITE), 1.0);
        assert_eq!(luminance(BLACK), 0.0);
        assert!((contrast(WHITE, BLACK) - 21.0).abs() < 1e-9);
        assert!((contrast(hex("#16171a"), WHITE) - 17.92).abs() < 0.01);
        assert_eq!(contrast(hex("#575b65"), WHITE), contrast(WHITE, hex("#575b65")));
    }

    #[test]
    fn translucent_colors_are_flattened_on_the_background() {
        assert_eq!(flatten(hex("#0000000a"), WHITE), hex("#f5f5f5"));
        assert_eq!(flatten(hex("#123456"), WHITE), hex("#123456"));
    }

    #[test]
    fn the_dark_threshold_is_a_fifth_of_white() {
        assert!(is_dark(hex("#7b7b7b")));
        assert!(!is_dark(hex("#7c7c7c")));
        assert!(is_dark(hex("#1b1c20")));
        assert!(!is_dark(WHITE));
    }
}
