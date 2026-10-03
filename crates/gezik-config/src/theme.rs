//! Theme files: parsing, validation and (in `resolve_theme`) `base` inheritance.

use crate::{Color, Warning};

/// Every color a theme can set, as written in `[colors]`.
pub const COLOR_KEYS: [&str; 13] = [
    "background",
    "surface",
    "foreground",
    "foreground-muted",
    "border",
    "accent",
    "accent-foreground",
    "selection",
    "selection-foreground",
    "hover",
    "folder-icon",
    "file-icon",
    "danger",
];

/// Numeric `[metrics]` keys with their allowed range (inclusive).
pub const METRIC_RANGES: [(&str, f32, f32); 5] = [
    ("font-size", 8.0, 32.0),
    ("row-height", 16.0, 64.0),
    ("icon-size", 12.0, 48.0),
    ("radius", 0.0, 16.0),
    ("spacing", 0.0, 24.0),
];

#[allow(dead_code)] // used by resolve_theme (Task 4)
const DARK: &str = include_str!("../themes/dark.toml");
#[allow(dead_code)] // used by resolve_theme (Task 4)
const LIGHT: &str = include_str!("../themes/light.toml");

#[allow(dead_code)] // used by resolve_theme (Task 4)
pub(crate) fn builtin_source(id: &str) -> Option<&'static str> {
    match id {
        "dark" => Some(DARK),
        "light" => Some(LIGHT),
        _ => None,
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ThemeColors {
    pub background: Color,
    pub surface: Color,
    pub foreground: Color,
    pub foreground_muted: Color,
    pub border: Color,
    pub accent: Color,
    pub accent_foreground: Color,
    pub selection: Color,
    pub selection_foreground: Color,
    pub hover: Color,
    pub folder_icon: Color,
    pub file_icon: Color,
    pub danger: Color,
}

impl ThemeColors {
    /// `key` must be one of [`COLOR_KEYS`].
    #[allow(dead_code)] // used by resolve_theme (Task 4)
    pub(crate) fn set(&mut self, key: &str, color: Color) {
        let slot = match key {
            "background" => &mut self.background,
            "surface" => &mut self.surface,
            "foreground" => &mut self.foreground,
            "foreground-muted" => &mut self.foreground_muted,
            "border" => &mut self.border,
            "accent" => &mut self.accent,
            "accent-foreground" => &mut self.accent_foreground,
            "selection" => &mut self.selection,
            "selection-foreground" => &mut self.selection_foreground,
            "hover" => &mut self.hover,
            "folder-icon" => &mut self.folder_icon,
            "file-icon" => &mut self.file_icon,
            "danger" => &mut self.danger,
            _ => unreachable!("not a color key: {key}"),
        };
        *slot = color;
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Metrics {
    /// Empty means the system font.
    pub font_family: String,
    pub font_size: f32,
    pub row_height: f32,
    pub icon_size: f32,
    pub radius: f32,
    pub spacing: f32,
}

impl Metrics {
    /// `key` must be one of [`METRIC_RANGES`].
    #[allow(dead_code)] // used by resolve_theme (Task 4)
    pub(crate) fn set(&mut self, key: &str, value: f32) {
        let slot = match key {
            "font-size" => &mut self.font_size,
            "row-height" => &mut self.row_height,
            "icon-size" => &mut self.icon_size,
            "radius" => &mut self.radius,
            "spacing" => &mut self.spacing,
            _ => unreachable!("not a metric key: {key}"),
        };
        *slot = value;
    }
}

/// One theme file as written: only the values it sets.
#[derive(Debug, Clone, Default)]
#[allow(dead_code)] // used by resolve_theme (Task 4)
pub(crate) struct PartialTheme {
    pub display_name: Option<String>,
    pub base: Option<String>,
    pub colors: Vec<(&'static str, Color)>,
    pub font_family: Option<String>,
    pub numbers: Vec<(&'static str, f32)>,
}

/// Parses one theme file. A syntax error returns `None`; a single bad value is skipped
/// with a warning and the rest of the file still loads. Unknown keys are ignored so
/// themes written for newer versions keep working.
#[allow(dead_code)] // used by resolve_theme (Task 4)
pub(crate) fn parse_theme(file: &str, text: &str, warnings: &mut Vec<Warning>) -> Option<PartialTheme> {
    let table = match text.parse::<toml::Table>() {
        Ok(table) => table,
        Err(err) => {
            warnings.push(Warning::from_toml_error(file, text, &err));
            return None;
        }
    };
    let mut theme = PartialTheme {
        display_name: table.get("name").and_then(|v| v.as_str()).map(str::to_owned),
        base: table.get("base").and_then(|v| v.as_str()).map(str::to_owned),
        ..PartialTheme::default()
    };

    if let Some(colors) = table.get("colors").and_then(|v| v.as_table()) {
        for key in COLOR_KEYS {
            let Some(value) = colors.get(key) else { continue };
            match value.as_str().and_then(Color::parse) {
                Some(color) => theme.colors.push((key, color)),
                None => warnings.push(Warning::new(
                    file,
                    format!("colors.{key}: expected \"#rrggbb\" or \"#rrggbbaa\", got {value}"),
                )),
            }
        }
    }

    if let Some(metrics) = table.get("metrics").and_then(|v| v.as_table()) {
        if let Some(value) = metrics.get("font-family") {
            match value.as_str() {
                Some(family) => theme.font_family = Some(family.to_owned()),
                None => warnings.push(Warning::new(file, format!("metrics.font-family: expected text, got {value}"))),
            }
        }
        for (key, min, max) in METRIC_RANGES {
            let Some(value) = metrics.get(key) else { continue };
            let number = value.as_integer().map(|n| n as f32).or_else(|| value.as_float().map(|f| f as f32));
            match number {
                Some(n) if (min..=max).contains(&n) => theme.numbers.push((key, n)),
                _ => warnings.push(Warning::new(
                    file,
                    format!("metrics.{key}: expected a number from {min} to {max}, got {value}"),
                )),
            }
        }
    }
    Some(theme)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> (Option<PartialTheme>, Vec<Warning>) {
        let mut warnings = Vec::new();
        let theme = parse_theme("t.toml", text, &mut warnings);
        (theme, warnings)
    }

    #[test]
    fn builtin_themes_define_every_value() {
        for id in ["dark", "light"] {
            let (theme, warnings) = parse(builtin_source(id).unwrap());
            let theme = theme.unwrap();
            assert!(warnings.is_empty(), "{id}: {warnings:?}");
            assert_eq!(theme.colors.len(), COLOR_KEYS.len(), "{id}");
            assert_eq!(theme.numbers.len(), METRIC_RANGES.len(), "{id}");
            assert!(theme.font_family.is_some(), "{id}");
            assert!(theme.base.is_none(), "{id}");
        }
    }

    #[test]
    fn reads_values() {
        let (theme, warnings) = parse(
            "name = \"Nord\"\nbase = \"light\"\n[colors]\naccent = \"#88c0d0\"\n\
             [metrics]\nrow-height = 28\nradius = 4.5\nfont-family = \"Inter\"\n",
        );
        let theme = theme.unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(theme.display_name.as_deref(), Some("Nord"));
        assert_eq!(theme.base.as_deref(), Some("light"));
        assert_eq!(theme.colors, [("accent", Color::parse("#88c0d0").unwrap())]);
        assert_eq!(theme.numbers, [("row-height", 28.0), ("radius", 4.5)]);
        assert_eq!(theme.font_family.as_deref(), Some("Inter"));
    }

    #[test]
    fn invalid_values_are_skipped_with_warnings() {
        let (theme, warnings) = parse(
            "[colors]\naccent = \"#zzz\"\nborder = \"#123456\"\n\
             [metrics]\nrow-height = 500\nfont-size = \"big\"\nfont-family = 3\n",
        );
        let theme = theme.unwrap();
        assert_eq!(theme.colors, [("border", Color::parse("#123456").unwrap())]);
        assert!(theme.numbers.is_empty());
        assert!(theme.font_family.is_none());
        let messages: Vec<_> = warnings.iter().map(|w| w.message.as_str()).collect();
        assert_eq!(warnings.len(), 4, "{messages:?}");
        assert!(messages[0].starts_with("colors.accent:"), "{messages:?}");
        assert!(messages.iter().any(|m| m.starts_with("metrics.row-height:") && m.contains("16 to 64")));
        assert!(messages.iter().any(|m| m.starts_with("metrics.font-size:")));
        assert!(messages.iter().any(|m| m.starts_with("metrics.font-family:")));
    }

    #[test]
    fn unknown_keys_are_ignored_silently() {
        let (theme, warnings) = parse("future = 1\n[colors]\nglow = \"#ffffff\"\n[sounds]\nclick = \"x\"\n");
        assert!(theme.unwrap().colors.is_empty());
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    #[test]
    fn syntax_error_returns_none_with_line() {
        let (theme, warnings) = parse("name = \"x\"\n[colors\n");
        assert!(theme.is_none());
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, Some(2));
        assert_eq!(warnings[0].file, "t.toml");
    }

    #[test]
    fn empty_file_is_valid_and_empty() {
        let (theme, warnings) = parse("");
        let theme = theme.unwrap();
        assert!(theme.colors.is_empty() && theme.numbers.is_empty() && theme.base.is_none());
        assert!(warnings.is_empty());
    }
}
