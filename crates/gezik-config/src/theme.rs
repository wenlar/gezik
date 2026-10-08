//! Theme files: parsing, validation and (in `resolve_theme`) `base` inheritance.

use crate::{Color, Warning};
use std::collections::HashMap;

/// Old color names still read, for themes written before the icon colors were split.
/// The new name wins when a theme sets both.
const COLOR_ALIASES: [(&str, &str); 2] = [("folder-icon", "icon-folder"), ("file-icon", "icon-other")];

/// Numeric `[metrics]` keys with their allowed range (inclusive).
pub const METRIC_RANGES: [(&str, f32, f32); 6] = [
    ("font-size", 8.0, 32.0),
    ("row-height", 16.0, 64.0),
    ("icon-size", 12.0, 48.0),
    ("radius", 0.0, 16.0),
    ("spacing", 0.0, 24.0),
    ("inset", 0.0, 16.0),
];

const DARK: &str = include_str!("../themes/dark.toml");
const LIGHT: &str = include_str!("../themes/light.toml");
const CLASSIC_DARK: &str = include_str!("../themes/classic-dark.toml");
const CLASSIC_LIGHT: &str = include_str!("../themes/classic-light.toml");

pub(crate) fn builtin_source(id: &str) -> Option<&'static str> {
    match id {
        "dark" => Some(DARK),
        "light" => Some(LIGHT),
        "classic-dark" => Some(CLASSIC_DARK),
        "classic-light" => Some(CLASSIC_LIGHT),
        _ => None,
    }
}

/// One list makes the `[colors]` keys, the struct fields and their lookup, so they cannot
/// drift apart.
macro_rules! theme_colors {
    ($($(#[$doc:meta])* $field:ident = $key:literal,)*) => {
        /// Every color a theme can set, as written in `[colors]`.
        pub const COLOR_KEYS: [&str; 43] = [$($key),*];

        #[derive(Debug, Clone, Default, PartialEq)]
        pub struct ThemeColors {
            $($(#[$doc])* pub $field: Color,)*
        }

        impl ThemeColors {
            /// `key` must be one of [`COLOR_KEYS`].
            pub fn get(&self, key: &str) -> Color {
                match key {
                    $($key => self.$field,)*
                    _ => unreachable!("not a color key: {key}"),
                }
            }

            /// `key` must be one of [`COLOR_KEYS`].
            pub(crate) fn set(&mut self, key: &str, color: Color) {
                let slot = match key {
                    $($key => &mut self.$field,)*
                    _ => unreachable!("not a color key: {key}"),
                };
                *slot = color;
            }
        }
    };
}

theme_colors! {
    /// The file list (later: the content sheet), the active tab.
    background = "background",
    /// Panels: operations, preview, column header.
    surface = "surface",
    foreground = "foreground",
    foreground_muted = "foreground-muted",
    border = "border",
    accent = "accent",
    accent_foreground = "accent-foreground",
    selection = "selection",
    selection_foreground = "selection-foreground",
    hover = "hover",
    icon_folder = "icon-folder",
    icon_image = "icon-image",
    icon_video = "icon-video",
    icon_audio = "icon-audio",
    icon_archive = "icon-archive",
    icon_document = "icon-document",
    icon_code = "icon-code",
    icon_other = "icon-other",
    focus_ring = "focus-ring",
    /// Fill of the rubber-band selection rectangle (usually translucent).
    marquee = "marquee",
    danger = "danger",
    /// Progress bars of running file operations.
    progress = "progress",
    /// A paused operation (waiting for decisions, a full disk, the user).
    progress_paused = "progress-paused",
    progress_error = "progress-error",
    /// A folder, tab or place a dragged file would be dropped on.
    drop_target = "drop-target",
    /// Menus, popups, layers.
    surface_raised = "surface-raised",
    /// Window, tab strip, toolbar, sidebar, status bar.
    chrome = "chrome",
    /// Edges of fields (at least 3:1).
    border_strong = "border-strong",
    accent_hover = "accent-hover",
    accent_pressed = "accent-pressed",
    /// Secondary columns on a selected row.
    selection_foreground_muted = "selection-foreground-muted",
    /// The selection of a list without the keyboard focus.
    selection_inactive = "selection-inactive",
    pressed = "pressed",
    tab_active = "tab-active",
    tab_inactive = "tab-inactive",
    /// The address bar and text fields.
    input_background = "input-background",
    danger_background = "danger-background",
    success = "success",
    warning = "warning",
    /// Under popups.
    shadow = "shadow",
    /// Behind modal layers.
    overlay = "overlay",
    scrollbar = "scrollbar",
    scrollbar_hover = "scrollbar-hover",
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
    /// Gap between the chrome and the content sheet, and the rows' inset from its edge; 0 = edge to edge, square corners.
    pub inset: f32,
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
            "inset" => &mut self.inset,
            _ => unreachable!("not a metric key: {key}"),
        };
        *slot = value;
    }

    /// Compact density: rows, spacing and inset at 80% rounded down (the design's rule), rows
    /// never below the allowed minimum; the radius stays.
    pub fn compact(&self) -> Metrics {
        Metrics {
            row_height: (self.row_height * 0.8).floor().max(16.0),
            spacing: (self.spacing * 0.8).floor(),
            inset: (self.inset * 0.8).floor(),
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
/// leaves out from its `base` chain. Colors a user file leaves out but that follow from ones it sets are worked out by `theme_rules::RULES`. A theme without `base` extends the built-in of the same id if there is one, else `dark`; a missing or cyclic base falls back to `dark`.
pub fn resolve_theme(
    id: &str,
    user_themes: &HashMap<String, String>,
    warnings: &mut Vec<Warning>,
) -> Result<ResolvedTheme, ThemeError> {
    let id = id.to_lowercase();
    let (source, text) = lookup(&id, user_themes, &[]).ok_or(ThemeError::NotFound)?;
    let first = parse_theme(&label(&id, source), text, warnings).ok_or(ThemeError::Invalid)?;
    let name = first.display_name.clone().unwrap_or_else(|| id.clone());

    let mut chain = vec![(first, source)];
    let mut visited = vec![(id.clone(), source)];
    loop {
        let (current, current_source) = visited.last().cloned().expect("chain is never empty");
        let base = chain.last().and_then(|(theme, _)| theme.base.clone());
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
            chain.push((builtin("dark"), Source::Builtin));
            break;
        };
        match parse_theme(&label(&base, source), text, warnings) {
            Some(theme) => {
                chain.push((theme, source));
                visited.push((base, source));
            }
            None => {
                chain.push((builtin("dark"), Source::Builtin));
                break;
            }
        }
    }

    let mut resolved = ResolvedTheme { id, name, colors: ThemeColors::default(), metrics: Metrics::default() };
    // Keys a user file sets: the rules leave them alone and work out what follows from them.
    let mut written: Vec<&'static str> = Vec::new();
    for (theme, source) in chain.iter().rev() {
        for (key, color) in &theme.colors {
            resolved.colors.set(key, *color);
            if *source == Source::User && !written.contains(key) {
                written.push(key);
            }
        }
        if let Some(family) = &theme.font_family {
            resolved.metrics.font_family = family.clone();
        }
        for (key, value) in &theme.numbers {
            resolved.metrics.set(key, *value);
        }
    }
    crate::theme_rules::derive(&mut resolved.colors, &written);
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
        for (old, new) in COLOR_ALIASES {
            if colors.contains_key(new) {
                continue;
            }
            let Some(value) = colors.get(old) else { continue };
            match value.as_str().and_then(Color::parse) {
                Some(color) => theme.colors.push((new, color)),
                None => warnings.push(Warning::new(
                    file,
                    format!("colors.{old}: expected \"#rrggbb\" or \"#rrggbbaa\", got {value}"),
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
    fn progress_colors_are_read() {
        let (theme, warnings) = parse("[colors]\nprogress = \"#112233\"\nprogress-paused = \"#445566\"\n");
        assert!(warnings.is_empty(), "{warnings:?}");
        let theme = theme.unwrap();
        assert!(theme.colors.iter().any(|(k, _)| *k == "progress"));
        assert!(theme.colors.iter().any(|(k, _)| *k == "progress-paused"));
    }

    #[test]
    fn old_icon_color_names_still_work() {
        let (theme, warnings) = parse(
            "[colors]
folder-icon = \"#112233\"
file-icon = \"#445566\"
",
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(theme.unwrap().colors, [("icon-folder", hex("#112233")), ("icon-other", hex("#445566"))]);
    }

    #[test]
    fn new_icon_color_names_win_over_old_ones() {
        let (theme, _) = parse(
            "[colors]
folder-icon = \"#112233\"
icon-folder = \"#abcdef\"
",
        );
        assert_eq!(theme.unwrap().colors, [("icon-folder", hex("#abcdef"))]);
    }

    #[test]
    fn bad_old_icon_color_warns_under_its_own_name() {
        let (_, warnings) = parse(
            "[colors]
file-icon = \"blue\"
",
        );
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.starts_with("colors.file-icon:"), "{}", warnings[0].message);
    }

    #[test]
    fn builtin_themes_define_every_value() {
        for id in ["dark", "light", "classic-dark", "classic-light"] {
            let (theme, warnings) = parse(builtin_source(id).unwrap());
            let theme = theme.unwrap();
            assert!(warnings.is_empty(), "{id}: {warnings:?}");
            assert_eq!(theme.colors.len(), COLOR_KEYS.len(), "{id}");
            assert_eq!(COLOR_KEYS.len(), 43);
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
        assert_eq!(theme.colors.background, hex("#1b1c20"));
        assert_eq!(theme.metrics.row_height, 26.0);
        assert_eq!(builtin_dark(), theme);
    }

    #[test]
    fn partial_theme_inherits_from_its_base() {
        let themes = user(&[("nord", "base = \"light\"\n[colors]\naccent = \"#88c0d0\"\n")]);
        let theme = resolve_theme("nord", &themes, &mut Vec::new()).unwrap();
        assert_eq!(theme.name, "nord");
        assert_eq!(theme.colors.accent, hex("#88c0d0"));
        assert_eq!(theme.colors.background, hex("#ffffff"));
        assert_eq!(theme.metrics.font_size, 13.0);
    }

    #[test]
    fn default_base_is_dark() {
        let themes = user(&[("mine", "[colors]\naccent = \"#ff0000\"\n")]);
        let theme = resolve_theme("mine", &themes, &mut Vec::new()).unwrap();
        assert_eq!(theme.colors.background, hex("#1b1c20"));
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
        assert_eq!(theme.colors.background, hex("#ffffff"));
    }

    #[test]
    fn user_theme_shadows_a_builtin_and_extends_it() {
        let themes = user(&[("dark", "[colors]\naccent = \"#ff00ff\"\n")]);
        let mut warnings = Vec::new();
        let theme = resolve_theme("dark", &themes, &mut warnings).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(theme.colors.accent, hex("#ff00ff"));
        assert_eq!(theme.colors.background, hex("#1b1c20"));
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
        assert_eq!(theme.colors.background, hex("#ffffff"));
    }

    #[test]
    fn cycle_falls_back_to_dark_with_warning() {
        let themes =
            user(&[("a", "base = \"b\"\nname = \"A\"\n[colors]\naccent = \"#aaaaaa\"\n"), ("b", "base = \"a\"\n")]);
        let mut warnings = Vec::new();
        let theme = resolve_theme("a", &themes, &mut warnings).unwrap();
        assert_eq!(theme.name, "A");
        assert_eq!(theme.colors.accent, hex("#aaaaaa"));
        assert_eq!(theme.colors.background, hex("#1b1c20"));
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].file, "b.toml");
        assert!(warnings[0].message.contains("\"a\""), "{}", warnings[0].message);
    }

    #[test]
    fn missing_base_falls_back_to_dark_with_warning() {
        let themes = user(&[("a", "base = \"gone\"\n")]);
        let mut warnings = Vec::new();
        let theme = resolve_theme("a", &themes, &mut warnings).unwrap();
        assert_eq!(theme.colors.background, hex("#1b1c20"));
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("\"gone\""));
    }

    #[test]
    fn broken_base_falls_back_to_dark() {
        let themes = user(&[("a", "base = \"bad\"\n"), ("bad", "[colors\n")]);
        let mut warnings = Vec::new();
        let theme = resolve_theme("a", &themes, &mut warnings).unwrap();
        assert_eq!(theme.colors.background, hex("#1b1c20"));
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
    fn builtin_names_and_insets() {
        for (id, name, inset) in [
            ("light", "Light", 6.0),
            ("dark", "Dark", 6.0),
            ("classic-light", "Classic Light", 0.0),
            ("classic-dark", "Classic Dark", 0.0),
        ] {
            let theme = resolve_theme(id, &HashMap::new(), &mut Vec::new()).unwrap();
            assert_eq!(theme.name, name);
            assert_eq!(theme.metrics.inset, inset, "{id}");
        }
    }

    #[test]
    fn classic_themes_keep_the_old_colors() {
        let old = include_str!("../tests/themes/old-light.toml");
        let (old, _) = parse(old);
        let classic = resolve_theme("classic-light", &HashMap::new(), &mut Vec::new()).unwrap();
        for (key, color) in old.unwrap().colors {
            assert_eq!(classic.colors.get(key), color, "{key}");
        }
        let (old, _) = parse(include_str!("../tests/themes/old-dark.toml"));
        let classic = resolve_theme("classic-dark", &HashMap::new(), &mut Vec::new()).unwrap();
        for (key, color) in old.unwrap().colors {
            assert_eq!(classic.colors.get(key), color, "{key}");
        }
    }

    #[test]
    fn get_reads_what_set_wrote() {
        let mut colors = ThemeColors::default();
        for (i, key) in COLOR_KEYS.iter().enumerate() {
            colors.set(key, Color { r: i as u8, g: 1, b: 2, a: 255 });
        }
        for (i, key) in COLOR_KEYS.iter().enumerate() {
            assert_eq!(colors.get(key).r, i as u8, "{key}");
        }
    }

    #[test]
    fn inset_out_of_range_warns() {
        let (theme, warnings) = parse("[metrics]\ninset = 20\n");
        assert!(theme.unwrap().numbers.is_empty());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.starts_with("metrics.inset:") && warnings[0].message.contains("0 to 16"));
        let (_, warnings) = parse("[metrics]\ninset = \"wide\"\n");
        assert!(warnings[0].message.starts_with("metrics.inset:"));
        let themes = user(&[("mine", "[metrics]\ninset = 20\n")]);
        assert_eq!(resolve_theme("mine", &themes, &mut Vec::new()).unwrap().metrics.inset, 6.0);
    }

    #[test]
    fn compact_floors_rows_spacing_and_inset() {
        let metrics = builtin_dark().metrics;
        let compact = metrics.compact();
        assert_eq!(compact.row_height, 20.0);
        assert_eq!(compact.spacing, 4.0);
        assert_eq!(compact.inset, 4.0);
        assert_eq!(compact.radius, metrics.radius);
        assert_eq!(compact.font_size, metrics.font_size);
        let tiny = Metrics { row_height: 16.0, inset: 1.0, ..metrics.clone() };
        assert_eq!(tiny.compact().row_height, 16.0);
        assert_eq!(tiny.compact().inset, 0.0);
        let flat = Metrics { inset: 0.0, ..metrics };
        assert_eq!(flat.compact().inset, 0.0);
    }

    fn colors_of(theme: &ResolvedTheme, expected: &[(&str, &str)]) {
        for (key, value) in expected {
            assert_eq!(theme.colors.get(key), hex(value), "{key}");
        }
    }

    #[test]
    fn builtins_resolve_to_exactly_their_files() {
        for id in ["light", "dark", "classic-light", "classic-dark"] {
            let theme = resolve_theme(id, &HashMap::new(), &mut Vec::new()).unwrap();
            let (file, _) = parse(builtin_source(id).unwrap());
            for (key, color) in file.unwrap().colors {
                assert_eq!(theme.colors.get(key), color, "{id} {key}");
            }
        }
    }

    #[test]
    fn an_accent_alone_recolors_what_follows_it() {
        let themes = user(&[("magenta", "base = \"light\"\n[colors]\naccent = \"#b0158f\"\n")]);
        let mut warnings = Vec::new();
        let theme = resolve_theme("magenta", &themes, &mut warnings).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        colors_of(
            &theme,
            &[
                ("accent-foreground", "#ffffff"),
                ("accent-hover", "#9b127e"),
                ("accent-pressed", "#86106d"),
                ("selection", "#f3dcee"),
                ("focus-ring", "#b0158f"),
                ("progress", "#b0158f"),
                ("marquee", "#b0158f24"),
                ("drop-target", "#b0158f33"),
                // Neutrals stay the designer's.
                ("chrome", "#e8e9ed"),
                ("selection-inactive", "#ececed"),
                ("border-strong", "#868b95"),
                ("hover", "#0000000a"),
            ],
        );
    }

    #[test]
    fn a_user_file_named_like_a_builtin_also_derives() {
        let themes = user(&[("light", "[colors]\naccent = \"#b0158f\"\n")]);
        let theme = resolve_theme("light", &themes, &mut Vec::new()).unwrap();
        colors_of(&theme, &[("selection", "#f3dcee"), ("focus-ring", "#b0158f")]);
    }

    #[test]
    fn a_background_alone_reworks_its_followers_in_a_chain() {
        let themes = user(&[("paper", "base = \"light\"\n[colors]\nbackground = \"#fdf6e3\"\n")]);
        let theme = resolve_theme("paper", &themes, &mut Vec::new()).unwrap();
        colors_of(
            &theme,
            &[
                ("chrome", "#e6e0cf"),
                ("tab-inactive", "#e6e0cf"),
                ("tab-active", "#fdf6e3"),
                ("surface-raised", "#fdf6e3"),
                ("input-background", "#fdf6e3"),
                ("selection", "#f4dbc3"),
                ("selection-inactive", "#ebe4d3"),
                ("danger-background", "#f6e3d1"),
                ("hover", "#0000000a"),
                ("pressed", "#00000014"),
                ("shadow", "#0000001f"),
                ("overlay", "#14141859"),
                // Not made from the background: the designer's values.
                ("border-strong", "#868b95"),
                ("accent-hover", "#ab390b"),
                ("selection-foreground-muted", "#2d2f34"),
            ],
        );
    }

    #[test]
    fn a_dark_theme_made_from_light_gets_dark_overlays() {
        let themes =
            user(&[("night", "base = \"light\"\n[colors]\nbackground = \"#202020\"\nforeground = \"#eeeeee\"\n")]);
        let theme = resolve_theme("night", &themes, &mut Vec::new()).unwrap();
        colors_of(
            &theme,
            &[
                ("hover", "#ffffff0f"),
                ("pressed", "#ffffff1c"),
                ("shadow", "#00000070"),
                ("overlay", "#0000008c"),
                ("chrome", "#161616"),
                ("tab-inactive", "#161616"),
                ("tab-active", "#202020"),
                ("surface-raised", "#2c2c2c"),
                ("input-background", "#181818"),
                ("selection", "#512a1a"),
                ("selection-foreground", "#eeeeee"),
                ("border-strong", "#e1e3e6"),
                ("scrollbar", "#eeeeee38"),
            ],
        );
    }

    #[test]
    fn the_dark_threshold_sits_at_a_fifth() {
        let themes = user(&[
            ("dim", "base = \"light\"\n[colors]\nbackground = \"#7b7b7b\"\n"),
            ("pale", "base = \"light\"\n[colors]\nbackground = \"#7c7c7c\"\n"),
        ]);
        assert_eq!(resolve_theme("dim", &themes, &mut Vec::new()).unwrap().colors.hover, hex("#ffffff0f"));
        assert_eq!(resolve_theme("pale", &themes, &mut Vec::new()).unwrap().colors.hover, hex("#0000000a"));
    }

    #[test]
    fn a_written_value_always_wins() {
        let themes = user(&[(
            "mine",
            "base = \"light\"\n[colors]\naccent = \"#b0158f\"\nselection = \"#123456\"\nchrome = \"#abcdef\"\nbackground = \"#fdf6e3\"\n",
        )]);
        let theme = resolve_theme("mine", &themes, &mut Vec::new()).unwrap();
        colors_of(&theme, &[("selection", "#123456"), ("chrome", "#abcdef"), ("tab-inactive", "#abcdef")]);
    }

    #[test]
    fn every_user_file_in_a_chain_counts_as_the_user() {
        let themes = user(&[
            ("a", "base = \"b\"\n[colors]\naccent = \"#0b7a69\"\n"),
            ("b", "base = \"light\"\n[colors]\nbackground = \"#fdf6e3\"\n"),
        ]);
        let theme = resolve_theme("a", &themes, &mut Vec::new()).unwrap();
        colors_of(
            &theme,
            &[("selection", "#d9e3d1"), ("chrome", "#e6e0cf"), ("tab-active", "#fdf6e3"), ("focus-ring", "#0b7a69")],
        );
        let themes = user(&[
            ("a", "base = \"b\"\n[colors]\naccent = \"#0b7a69\"\n"),
            ("b", "base = \"light\"\n[colors]\nselection = \"#123456\"\n"),
        ]);
        assert_eq!(resolve_theme("a", &themes, &mut Vec::new()).unwrap().colors.selection, hex("#123456"));
    }

    #[test]
    fn base_can_be_a_classic_theme() {
        let themes = user(&[("old", "base = \"classic-dark\"\n[colors]\naccent = \"#ff8f57\"\n")]);
        let theme = resolve_theme("old", &themes, &mut Vec::new()).unwrap();
        assert_eq!(theme.colors.background, hex("#1c1c1c"));
        assert_eq!(theme.metrics.inset, 0.0);
        assert_eq!(theme.colors.focus_ring, hex("#ff8f57"));
    }
}
