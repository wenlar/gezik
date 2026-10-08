//! Color arithmetic for themes, and (Task 2) the rules that work out the colors a theme
//! leaves out. Rounds exactly like the design's `gezik-tokens.js`.

use crate::Color;
use crate::theme::ThemeColors;

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

/// How a color a theme leaves out is made from the ones it sets. `compute` gets the colors
/// worked out so far and whether the theme is dark.
pub(crate) struct Rule {
    pub key: &'static str,
    pub inputs: &'static [&'static str],
    pub compute: fn(&ThemeColors, bool) -> Color,
}

const NEAR_BLACK: Color = Color { r: 0x10, g: 0x10, b: 0x10, a: 255 };
const SCRIM: Color = Color { r: 0x14, g: 0x14, b: 0x18, a: 255 };

fn ink(dark: bool) -> Color {
    if dark { WHITE } else { BLACK }
}

/// In dependency order: a rule only reads keys that are not derived or come earlier.
pub(crate) static RULES: [Rule; 26] = [
    Rule {
        key: "chrome",
        inputs: &["background", "foreground"],
        compute: |c, dark| {
            if dark { mix(c.background, BLACK, 0.30) } else { mix(c.background, c.foreground, 0.10) }
        },
    },
    Rule {
        key: "surface-raised",
        inputs: &["background", "foreground"],
        compute: |c, dark| if dark { mix(c.background, c.foreground, 0.06) } else { c.background },
    },
    Rule {
        key: "input-background",
        inputs: &["surface-raised", "background"],
        compute: |c, dark| if dark { mix(c.background, BLACK, 0.25) } else { c.surface_raised },
    },
    Rule { key: "border-strong", inputs: &["border", "foreground"], compute: |c, _| mix(c.border, c.foreground, 0.40) },
    Rule {
        key: "accent-foreground",
        inputs: &["accent"],
        compute: |c, _| {
            if contrast(c.accent, WHITE) >= contrast(c.accent, NEAR_BLACK) { WHITE } else { mix(c.accent, BLACK, 0.86) }
        },
    },
    Rule { key: "accent-hover", inputs: &["accent"], compute: |c, dark| mix(c.accent, ink(dark), 0.12) },
    Rule { key: "accent-pressed", inputs: &["accent"], compute: |c, dark| mix(c.accent, ink(dark), 0.24) },
    Rule {
        key: "selection",
        inputs: &["background", "accent"],
        compute: |c, dark| mix(c.background, c.accent, if dark { 0.30 } else { 0.15 }),
    },
    Rule { key: "selection-foreground", inputs: &["foreground"], compute: |c, _| c.foreground },
    Rule {
        key: "selection-foreground-muted",
        inputs: &["foreground", "foreground-muted"],
        compute: |c, _| mix(c.foreground, c.foreground_muted, 0.35),
    },
    Rule {
        key: "selection-inactive",
        inputs: &["background", "foreground"],
        compute: |c, dark| mix(c.background, c.foreground, if dark { 0.11 } else { 0.08 }),
    },
    Rule {
        key: "hover",
        inputs: &["background"],
        compute: |_, dark| with_alpha(ink(dark), if dark { 0.06 } else { 0.04 }),
    },
    Rule {
        key: "pressed",
        inputs: &["background"],
        compute: |_, dark| with_alpha(ink(dark), if dark { 0.11 } else { 0.08 }),
    },
    Rule { key: "focus-ring", inputs: &["accent"], compute: |c, _| c.accent },
    Rule { key: "progress", inputs: &["accent"], compute: |c, _| c.accent },
    Rule {
        key: "marquee",
        inputs: &["accent"],
        compute: |c, dark| with_alpha(c.accent, if dark { 0.20 } else { 0.14 }),
    },
    Rule {
        key: "drop-target",
        inputs: &["accent"],
        compute: |c, dark| with_alpha(c.accent, if dark { 0.28 } else { 0.20 }),
    },
    Rule { key: "tab-active", inputs: &["background"], compute: |c, _| c.background },
    Rule { key: "tab-inactive", inputs: &["chrome"], compute: |c, _| c.chrome },
    Rule {
        key: "danger-background",
        inputs: &["background", "danger"],
        compute: |c, dark| mix(c.background, c.danger, if dark { 0.16 } else { 0.09 }),
    },
    Rule { key: "progress-paused", inputs: &["warning"], compute: |c, _| c.warning },
    Rule { key: "progress-error", inputs: &["danger"], compute: |c, _| c.danger },
    Rule {
        key: "shadow",
        inputs: &["background"],
        compute: |_, dark| with_alpha(BLACK, if dark { 0.44 } else { 0.12 }),
    },
    Rule {
        key: "overlay",
        inputs: &["background"],
        compute: |_, dark| if dark { with_alpha(BLACK, 0.55) } else { with_alpha(SCRIM, 0.35) },
    },
    Rule { key: "scrollbar", inputs: &["foreground"], compute: |c, _| with_alpha(c.foreground, 0.22) },
    Rule { key: "scrollbar-hover", inputs: &["foreground"], compute: |c, _| with_alpha(c.foreground, 0.42) },
];

/// Works out the derived colors no user file wrote: a rule runs when one of its inputs was
/// written by the user or itself worked out here; otherwise the built-in value stays.
pub(crate) fn derive(colors: &mut ThemeColors, written: &[&str]) {
    let dark = is_dark(colors.background);
    let mut touched: Vec<&str> = written.to_vec();
    for rule in &RULES {
        if written.contains(&rule.key) {
            continue;
        }
        if rule.inputs.iter().any(|input| touched.contains(input)) {
            let value = (rule.compute)(colors, dark);
            colors.set(rule.key, value);
            touched.push(rule.key);
        }
    }
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

    use crate::theme::{COLOR_KEYS, resolve_theme};
    use std::collections::HashMap;

    /// Neutrals the design tuned by hand; their rules only approximate them.
    const HAND_TUNED: [&str; 3] = ["chrome", "surface-raised", "border-strong"];

    #[test]
    fn rules_reproduce_the_graphite_tables() {
        for id in ["light", "dark"] {
            let theme = resolve_theme(id, &HashMap::new(), &mut Vec::new()).unwrap();
            let dark = is_dark(theme.colors.background);
            assert_eq!(dark, id == "dark");
            for rule in RULES.iter().filter(|r| !HAND_TUNED.contains(&r.key)) {
                assert_eq!((rule.compute)(&theme.colors, dark), theme.colors.get(rule.key), "{id} {}", rule.key);
            }
        }
    }

    #[test]
    fn classic_new_keys_follow_the_rules_from_the_old_values() {
        // chrome is the old surface on purpose; success and warning have no rule.
        const NEW: [&str; 15] = [
            "surface-raised",
            "border-strong",
            "accent-hover",
            "accent-pressed",
            "selection-foreground-muted",
            "selection-inactive",
            "pressed",
            "tab-active",
            "tab-inactive",
            "input-background",
            "danger-background",
            "shadow",
            "overlay",
            "scrollbar",
            "scrollbar-hover",
        ];
        for id in ["classic-light", "classic-dark"] {
            let theme = resolve_theme(id, &HashMap::new(), &mut Vec::new()).unwrap();
            let dark = is_dark(theme.colors.background);
            for key in NEW {
                let rule = RULES.iter().find(|r| r.key == key).unwrap();
                assert_eq!((rule.compute)(&theme.colors, dark), theme.colors.get(key), "{id} {key}");
            }
        }
    }

    #[test]
    fn rules_come_in_dependency_order_and_cover_26_keys() {
        assert_eq!(RULES.len(), 26);
        for (i, rule) in RULES.iter().enumerate() {
            assert!(COLOR_KEYS.contains(&rule.key), "{}", rule.key);
            for input in rule.inputs {
                assert!(COLOR_KEYS.contains(input), "{input}");
                // A derived input must be worked out before the rule that reads it.
                if let Some(j) = RULES.iter().position(|r| r.key == *input) {
                    assert!(j < i, "{} reads {input} before it is derived", rule.key);
                }
            }
        }
    }

    #[test]
    fn accent_foreground_picks_the_stronger_contrast() {
        let mut c = ThemeColors::default();
        let rule = RULES.iter().find(|r| r.key == "accent-foreground").unwrap();
        c.accent = hex("#c2410c");
        assert_eq!((rule.compute)(&c, false), WHITE);
        c.accent = hex("#ff8f57");
        assert_eq!((rule.compute)(&c, true), hex("#24140c"));
    }
}
