//! Theme files: parsing, validation and (in `resolve_theme`) `base` inheritance.

use crate::{Color, Warning};
use std::collections::HashMap;

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

const DARK: &str = include_str!("../themes/dark.toml");
const LIGHT: &str = include_str!("../themes/light.toml");

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

    /// Compact density: rows and spacing at 80%, never below the allowed minimum.
    pub fn compact(&self) -> Metrics {
        Metrics {
            row_height: (self.row_height * 0.8).round().max(16.0),
            spacing: (self.spacing * 0.8).round(),
            ..self.clone()
        }
    }
}

/// One theme file as written: only the values it sets.
#[derive(Debug, Clone, Default)]
pub(crate) struct PartialTheme {
    pub display_name: Option<String>,
    pub base: Option<String>,
    pub colors: Vec<(&'static str, Color)>,
    pub font_family: Option<String>,
    pub numbers: Vec<(&'static str, f32)>,
}

/// A theme with every value filled in, ready for the UI.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedTheme {
    /// Lowercase file name without extension (`nord`), or `dark` / `light`.
    pub id: String,
    /// The theme's `name`, or its id when it has none.
    pub name: String,
    pub colors: ThemeColors,
    pub metrics: Metrics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeError {
    /// No user theme or built-in theme has this id.
    NotFound,
    /// The theme file has a syntax error (already reported as a warning).
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Source {
    User,
    Builtin,
}

/// Finds a theme, preferring the user's file over a built-in with the same id. A theme
/// already in the chain is skipped, which lets a user `dark` extend the built-in `dark`
/// and turns real cycles into "not found".
fn lookup<'a>(
    id: &str,
    user_themes: &'a HashMap<String, String>,
    visited: &[(String, Source)],
) -> Option<(Source, &'a str)> {
    let seen = |source| visited.iter().any(|(v, s)| v == id && *s == source);
    if let Some(text) = user_themes.get(id)
        && !seen(Source::User)
    {
        return Some((Source::User, text));
    }
    builtin_source(id).filter(|_| !seen(Source::Builtin)).map(|text| (Source::Builtin, text))
}

fn label(id: &str, source: Source) -> String {
    match source {
        Source::User => format!("{id}.toml"),
        Source::Builtin => format!("built-in theme \"{id}\""),
    }
}

fn builtin(id: &str) -> PartialTheme {
    let text = builtin_source(id).expect("known built-in theme");
    parse_theme(id, text, &mut Vec::new()).expect("built-in themes are valid")
}

/// Resolves a theme by id (case-insensitive) from the user's theme files
/// (`id → TOML text`, ids lowercase) and the built-ins, filling every value the theme
/// leaves out from its `base` chain. A theme without `base` extends the built-in of the same id if there is one, else `dark`; a missing or cyclic base falls back to `dark`.
pub fn resolve_theme(
    id: &str,
    user_themes: &HashMap<String, String>,
    warnings: &mut Vec<Warning>,
) -> Result<ResolvedTheme, ThemeError> {
    let id = id.to_lowercase();
    let (source, text) = lookup(&id, user_themes, &[]).ok_or(ThemeError::NotFound)?;
    let first = parse_theme(&label(&id, source), text, warnings).ok_or(ThemeError::Invalid)?;
    let name = first.display_name.clone().unwrap_or_else(|| id.clone());

    let mut chain = vec![first];
    let mut visited = vec![(id.clone(), source)];
    loop {
        let (current, current_source) = visited.last().cloned().expect("chain is never empty");
        let base = chain.last().and_then(|theme| theme.base.clone());
        // Built-ins define every value, so the chain ends at one without a base.
        if current_source == Source::Builtin && base.is_none() {
            break;
        }
        // Without a `base`, a theme extends the built-in it shadows, otherwise `dark`.
        let default_base = if builtin_source(&current).is_some() { current.as_str() } else { "dark" };
        let base = base.unwrap_or_else(|| default_base.to_owned()).to_lowercase();
        let Some((source, text)) = lookup(&base, user_themes, &visited) else {
            warnings.push(Warning::new(
                label(&current, current_source),
                format!("base theme \"{base}\" not found or forms a cycle; using \"dark\" instead"),
            ));
            chain.push(builtin("dark"));
            break;
        };
        match parse_theme(&label(&base, source), text, warnings) {
            Some(theme) => {
                chain.push(theme);
                visited.push((base, source));
            }
            None => {
                chain.push(builtin("dark"));
                break;
            }
        }
    }

    let mut resolved = ResolvedTheme { id, name, colors: ThemeColors::default(), metrics: Metrics::default() };
    for theme in chain.iter().rev() {
        for (key, color) in &theme.colors {
            resolved.colors.set(key, *color);
        }
        if let Some(family) = &theme.font_family {
            resolved.metrics.font_family = family.clone();
        }
        for (key, value) in &theme.numbers {
            resolved.metrics.set(key, *value);
        }
    }
    Ok(resolved)
}

/// The built-in dark theme: what the UI shows before any config is read.
pub fn builtin_dark() -> ResolvedTheme {
    resolve_theme("dark", &HashMap::new(), &mut Vec::new()).expect("built-in dark theme resolves")
}

/// Parses one theme file. A syntax error returns `None`; a single bad value is skipped
/// with a warning and the rest of the file still loads. Unknown keys are ignored so
/// themes written for newer versions keep working.
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

    fn user(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(id, text)| (id.to_string(), text.to_string())).collect()
    }

    fn hex(text: &str) -> Color {
        Color::parse(text).unwrap()
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

    #[test]
    fn builtin_dark_resolves_completely() {
        let mut warnings = Vec::new();
        let theme = resolve_theme("dark", &HashMap::new(), &mut warnings).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(theme.id, "dark");
        assert_eq!(theme.name, "Dark");
        assert_eq!(theme.colors.background, hex("#1c1c1c"));
        assert_eq!(theme.metrics.row_height, 26.0);
        assert_eq!(builtin_dark(), theme);
    }

    #[test]
    fn partial_theme_inherits_from_its_base() {
        let themes = user(&[("nord", "base = \"light\"\n[colors]\naccent = \"#88c0d0\"\n")]);
        let theme = resolve_theme("nord", &themes, &mut Vec::new()).unwrap();
        assert_eq!(theme.name, "nord");
        assert_eq!(theme.colors.accent, hex("#88c0d0"));
        assert_eq!(theme.colors.background, hex("#fafafa"));
        assert_eq!(theme.metrics.font_size, 13.0);
    }

    #[test]
    fn default_base_is_dark() {
        let themes = user(&[("mine", "[colors]\naccent = \"#ff0000\"\n")]);
        let theme = resolve_theme("mine", &themes, &mut Vec::new()).unwrap();
        assert_eq!(theme.colors.background, hex("#1c1c1c"));
    }

    #[test]
    fn follows_multi_level_chains() {
        let themes = user(&[
            ("a", "base = \"b\"\n[colors]\naccent = \"#aaaaaa\"\n"),
            ("b", "base = \"light\"\n[colors]\nborder = \"#bbbbbb\"\naccent = \"#000000\"\n"),
        ]);
        let theme = resolve_theme("a", &themes, &mut Vec::new()).unwrap();
        assert_eq!(theme.colors.accent, hex("#aaaaaa"));
        assert_eq!(theme.colors.border, hex("#bbbbbb"));
        assert_eq!(theme.colors.background, hex("#fafafa"));
    }

    #[test]
    fn user_theme_shadows_a_builtin_and_extends_it() {
        let themes = user(&[("dark", "[colors]\naccent = \"#ff00ff\"\n")]);
        let mut warnings = Vec::new();
        let theme = resolve_theme("dark", &themes, &mut warnings).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(theme.colors.accent, hex("#ff00ff"));
        assert_eq!(theme.colors.background, hex("#1c1c1c"));
    }

    #[test]
    fn user_light_shadows_and_extends_builtin_light() {
        let themes = user(&[(
            "light",
            "[colors]
accent = \"#ff00ff\"
",
        )]);
        let mut warnings = Vec::new();
        let theme = resolve_theme("light", &themes, &mut warnings).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(theme.colors.accent, hex("#ff00ff"));
        assert_eq!(theme.colors.background, hex("#fafafa"));
    }

    #[test]
    fn cycle_falls_back_to_dark_with_warning() {
        let themes =
            user(&[("a", "base = \"b\"\nname = \"A\"\n[colors]\naccent = \"#aaaaaa\"\n"), ("b", "base = \"a\"\n")]);
        let mut warnings = Vec::new();
        let theme = resolve_theme("a", &themes, &mut warnings).unwrap();
        assert_eq!(theme.name, "A");
        assert_eq!(theme.colors.accent, hex("#aaaaaa"));
        assert_eq!(theme.colors.background, hex("#1c1c1c"));
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].file, "b.toml");
        assert!(warnings[0].message.contains("\"a\""), "{}", warnings[0].message);
    }

    #[test]
    fn missing_base_falls_back_to_dark_with_warning() {
        let themes = user(&[("a", "base = \"gone\"\n")]);
        let mut warnings = Vec::new();
        let theme = resolve_theme("a", &themes, &mut warnings).unwrap();
        assert_eq!(theme.colors.background, hex("#1c1c1c"));
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("\"gone\""));
    }

    #[test]
    fn broken_base_falls_back_to_dark() {
        let themes = user(&[("a", "base = \"bad\"\n"), ("bad", "[colors\n")]);
        let mut warnings = Vec::new();
        let theme = resolve_theme("a", &themes, &mut warnings).unwrap();
        assert_eq!(theme.colors.background, hex("#1c1c1c"));
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].file, "bad.toml");
        assert_eq!(warnings[0].line, Some(1));
    }

    #[test]
    fn unknown_theme_is_not_found() {
        assert_eq!(resolve_theme("nope", &HashMap::new(), &mut Vec::new()), Err(ThemeError::NotFound));
    }

    #[test]
    fn broken_theme_is_invalid() {
        let themes = user(&[("bad", "[colors\n")]);
        let mut warnings = Vec::new();
        assert_eq!(resolve_theme("bad", &themes, &mut warnings), Err(ThemeError::Invalid));
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn ids_are_case_insensitive() {
        let themes = user(&[("nord", "[colors]\naccent = \"#88c0d0\"\n")]);
        let theme = resolve_theme("Nord", &themes, &mut Vec::new()).unwrap();
        assert_eq!(theme.id, "nord");
        assert_eq!(theme.colors.accent, hex("#88c0d0"));
    }

    #[test]
    fn compact_scales_rows_and_spacing_with_a_floor() {
        let metrics = builtin_dark().metrics;
        let compact = metrics.compact();
        assert_eq!(compact.row_height, 21.0);
        assert_eq!(compact.spacing, 5.0);
        assert_eq!(compact.font_size, metrics.font_size);
        let tiny = Metrics { row_height: 16.0, ..metrics };
        assert_eq!(tiny.compact().row_height, 16.0);
    }
}
