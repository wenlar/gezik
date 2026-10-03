//! `settings.toml` (portable, may be synced) and `state.toml` (this machine only).

use crate::Warning;
use crate::shortcuts::{Platform, Shortcuts};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThemeChoice {
    /// Follow the system light/dark mode.
    Auto,
    Named(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarPosition {
    Left,
    Right,
    Hidden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Density {
    Compact,
    Comfortable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub theme: ThemeChoice,
    pub theme_light: String,
    pub theme_dark: String,
    pub sidebar: SidebarPosition,
    pub density: Density,
    /// `"drives"`, or a path (with `{home}`-style tokens) to open new tabs in.
    pub start_folder: String,
    /// Pinned folders as written (tokenized, `/` separators), in display order.
    pub pinned: Vec<String>,
    pub shortcuts: Shortcuts,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemeChoice::Auto,
            theme_light: "light".to_owned(),
            theme_dark: "dark".to_owned(),
            sidebar: SidebarPosition::Left,
            density: Density::Comfortable,
            start_folder: "{home}".to_owned(),
            pinned: Vec::new(),
            shortcuts: Shortcuts::default(),
        }
    }
}

impl Settings {
    /// Reads `settings.toml`. Anything missing or invalid keeps its default; problems
    /// become warnings. A syntax error yields all defaults.
    pub fn parse(file: &str, text: &str, warnings: &mut Vec<Warning>) -> Settings {
        let mut settings = Settings::default();
        let table = match text.parse::<toml::Table>() {
            Ok(table) => table,
            Err(err) => {
                warnings.push(Warning::from_toml_error(file, text, &err));
                return settings;
            }
        };
        let mut text_value = |table: &toml::Table, key: &str, path: &str| -> Option<String> {
            let value = table.get(key)?;
            let text = value.as_str().map(str::to_owned);
            if text.is_none() {
                warnings.push(Warning::new(file, format!("{path}: expected text, got {value}")));
            }
            text
        };

        if let Some(theme) = text_value(&table, "theme", "theme") {
            settings.theme =
                if theme.eq_ignore_ascii_case("auto") { ThemeChoice::Auto } else { ThemeChoice::Named(theme) };
        }
        if let Some(theme) = text_value(&table, "theme-light", "theme-light") {
            settings.theme_light = theme;
        }
        if let Some(theme) = text_value(&table, "theme-dark", "theme-dark") {
            settings.theme_dark = theme;
        }

        let start_folder = text_value(&table, "start-folder", "start-folder");

        if let Some(layout) = table.get("layout").and_then(|v| v.as_table()) {
            let sidebar = text_value(layout, "sidebar", "layout.sidebar");
            let density = text_value(layout, "density", "layout.density");
            match sidebar.as_deref() {
                None => {}
                Some("left") => settings.sidebar = SidebarPosition::Left,
                Some("right") => settings.sidebar = SidebarPosition::Right,
                Some("hidden") => settings.sidebar = SidebarPosition::Hidden,
                Some(other) => warnings.push(Warning::new(
                    file,
                    format!("layout.sidebar: expected \"left\", \"right\" or \"hidden\", got \"{other}\""),
                )),
            }
            match density.as_deref() {
                None => {}
                Some("compact") => settings.density = Density::Compact,
                Some("comfortable") => settings.density = Density::Comfortable,
                Some(other) => warnings.push(Warning::new(
                    file,
                    format!("layout.density: expected \"compact\" or \"comfortable\", got \"{other}\""),
                )),
            }
        }

        // `text_value` borrows `warnings`; it is no longer used from here on.
        if let Some(start) = start_folder {
            if crate::paths::has_parent_segment(&start) {
                warnings.push(Warning::new(file, format!("start-folder: \"{start}\" must not contain \"..\"")));
            } else {
                settings.start_folder = start;
            }
        }
        if let Some(value) = table.get("pinned") {
            match value.as_array() {
                None => warnings.push(Warning::new(file, format!("pinned: expected a list of folders, got {value}"))),
                Some(items) => {
                    for item in items {
                        let Some(text) = item.as_str() else {
                            warnings.push(Warning::new(file, format!("pinned: expected text, got {item}")));
                            continue;
                        };
                        if crate::paths::has_parent_segment(text) {
                            warnings.push(Warning::new(file, format!("pinned: \"{text}\" must not contain \"..\"")));
                        } else if !settings.pinned.iter().any(|p| p == text) {
                            settings.pinned.push(text.to_owned());
                        }
                    }
                }
            }
        }
        match table.get("shortcuts") {
            None => {}
            Some(value) => match value.as_table() {
                Some(shortcuts) => {
                    settings.shortcuts = Shortcuts::from_table(Some(shortcuts), Platform::current(), file, warnings)
                }
                None => warnings.push(Warning::new(file, format!("shortcuts: expected a table, got {value}"))),
            },
        }
        settings
    }

    /// The theme id to show right now.
    pub fn active_theme(&self, system_dark: bool) -> &str {
        match &self.theme {
            ThemeChoice::Auto if system_dark => &self.theme_dark,
            ThemeChoice::Auto => &self.theme_light,
            ThemeChoice::Named(id) => id,
        }
    }
}

/// Size in logical pixels, position in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowState {
    pub width: u32,
    pub height: u32,
    pub x: Option<i32>,
    pub y: Option<i32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct State {
    pub window: Option<WindowState>,
    /// Sidebar width in logical pixels (120–480).
    pub sidebar_width: Option<u32>,
}

impl State {
    /// The app writes this file itself, so a damaged one just yields the defaults.
    pub fn parse(text: &str) -> State {
        let Ok(table) = text.parse::<toml::Table>() else { return State::default() };
        let window = table.get("window").and_then(|v| v.as_table()).and_then(|window| {
            let int = |key: &str| window.get(key).and_then(|v| v.as_integer());
            let coord = |key: &str| int(key).and_then(|v| i32::try_from(v).ok());
            Some(WindowState {
                width: u32::try_from(int("width")?).ok().filter(|w| *w >= 200)?,
                height: u32::try_from(int("height")?).ok().filter(|h| *h >= 150)?,
                x: coord("x"),
                y: coord("y"),
            })
        });
        let sidebar_width = table
            .get("sidebar")
            .and_then(|v| v.as_table())
            .and_then(|s| s.get("width"))
            .and_then(|v| v.as_integer())
            .and_then(|w| u32::try_from(w).ok())
            .filter(|w| (120..=480).contains(w));
        State { window, sidebar_width }
    }

    pub fn to_toml(&self) -> String {
        let mut root = toml::Table::new();
        if let Some(w) = self.window {
            let mut window = toml::Table::new();
            window.insert("width".into(), toml::Value::Integer(w.width.into()));
            window.insert("height".into(), toml::Value::Integer(w.height.into()));
            if let (Some(x), Some(y)) = (w.x, w.y) {
                window.insert("x".into(), toml::Value::Integer(x.into()));
                window.insert("y".into(), toml::Value::Integer(y.into()));
            }
            root.insert("window".into(), toml::Value::Table(window));
        }
        if let Some(width) = self.sidebar_width {
            let mut sidebar = toml::Table::new();
            sidebar.insert("width".into(), toml::Value::Integer(width.into()));
            root.insert("sidebar".into(), toml::Value::Table(sidebar));
        }
        root.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> (Settings, Vec<Warning>) {
        let mut warnings = Vec::new();
        let settings = Settings::parse("settings.toml", text, &mut warnings);
        (settings, warnings)
    }

    #[test]
    fn defaults_when_empty() {
        let (settings, warnings) = parse("");
        assert_eq!(settings, Settings::default());
        assert!(warnings.is_empty());
    }

    #[test]
    fn reads_all_values() {
        let (settings, warnings) = parse(
            "theme = \"nord\"\ntheme-light = \"paper\"\ntheme-dark = \"ink\"\n\
             [layout]\nsidebar = \"right\"\ndensity = \"compact\"\n",
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(
            settings,
            Settings {
                theme: ThemeChoice::Named("nord".to_owned()),
                theme_light: "paper".to_owned(),
                theme_dark: "ink".to_owned(),
                sidebar: SidebarPosition::Right,
                density: Density::Compact,
                ..Settings::default()
            }
        );
    }

    #[test]
    fn invalid_values_keep_defaults_with_warnings() {
        let (settings, warnings) = parse("theme = 5\n[layout]\nsidebar = \"top\"\ndensity = \"tiny\"\n");
        assert_eq!(settings, Settings::default());
        let messages: Vec<_> = warnings.iter().map(|w| w.message.as_str()).collect();
        assert_eq!(messages.len(), 3, "{messages:?}");
        assert!(messages[0].starts_with("theme:"));
        assert!(messages[1].starts_with("layout.sidebar:") && messages[1].contains("\"top\""));
        assert!(messages[2].starts_with("layout.density:"));
    }

    #[test]
    fn syntax_error_gives_defaults_with_line() {
        let (settings, warnings) = parse("theme = \"nord\"\n[layout\n");
        assert_eq!(settings, Settings::default());
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, Some(2));
    }

    #[test]
    fn auto_mode_follows_the_system() {
        let settings = Settings::default();
        assert_eq!(settings.active_theme(true), "dark");
        assert_eq!(settings.active_theme(false), "light");
    }

    #[test]
    fn auto_is_case_insensitive() {
        let (settings, warnings) = parse(
            "theme = \"Auto\"
",
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(settings.theme, ThemeChoice::Auto);
    }

    #[test]
    fn named_theme_ignores_the_system() {
        let settings = Settings { theme: ThemeChoice::Named("nord".to_owned()), ..Settings::default() };
        assert_eq!(settings.active_theme(true), "nord");
        assert_eq!(settings.active_theme(false), "nord");
    }

    #[test]
    fn state_round_trips() {
        let state = State {
            window: Some(WindowState { width: 1000, height: 700, x: Some(-50), y: Some(30) }),
            sidebar_width: None,
        };
        assert_eq!(State::parse(&state.to_toml()), state);
        let no_position =
            State { window: Some(WindowState { width: 800, height: 600, x: None, y: None }), sidebar_width: None };
        assert_eq!(State::parse(&no_position.to_toml()), no_position);
    }

    #[test]
    fn state_rejects_tiny_or_broken_values() {
        assert_eq!(State::parse("[window]\nwidth = 10\nheight = 10\n"), State::default());
        assert_eq!(State::parse("[window]\nwidth = -900\nheight = 600\n"), State::default());
        assert_eq!(State::parse("garbage ["), State::default());
        assert_eq!(State::parse(""), State::default());
    }

    #[test]
    fn reads_start_folder_and_pinned() {
        let (settings, warnings) = parse(
            "start-folder = \"drives\"
pinned = [\"{documents}/Projects\", \"D:/Work\"]
",
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(settings.start_folder, "drives");
        assert_eq!(settings.pinned, ["{documents}/Projects", "D:/Work"]);
    }

    #[test]
    fn pinned_skips_duplicates_and_parent_segments() {
        let (settings, warnings) = parse(
            "pinned = [\"/a\", \"/a\", \"{home}/../etc\", 3]
",
        );
        assert_eq!(settings.pinned, ["/a"]);
        let messages: Vec<_> = warnings.iter().map(|w| w.message.as_str()).collect();
        assert_eq!(messages.len(), 2, "{messages:?}");
        assert!(messages.iter().any(|m| m.contains("..")));
        assert!(messages.iter().any(|m| m.contains("expected text")));
    }

    #[test]
    fn pinned_keeps_entries_that_do_not_exist_here() {
        let (settings, _) = parse(
            "pinned = [\"Z:/not/on/this/machine\"]
",
        );
        assert_eq!(settings.pinned, ["Z:/not/on/this/machine"]);
    }

    #[test]
    fn invalid_start_folder_and_pinned_types_warn() {
        let (settings, warnings) = parse(
            "start-folder = \"{home}/../x\"
pinned = \"nope\"
",
        );
        assert_eq!(settings.start_folder, "{home}");
        assert!(settings.pinned.is_empty());
        assert_eq!(warnings.len(), 2, "{warnings:?}");
    }

    #[test]
    fn shortcuts_table_is_read() {
        use crate::shortcuts::{Action, parse_chord};
        let (settings, warnings) = parse(
            "[shortcuts]
refresh = \"ctrl+r\"
",
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        let chord = parse_chord("ctrl+r", Platform::current()).unwrap().unwrap();
        assert_eq!(settings.shortcuts.action_for(&chord), Some(Action::Refresh));
    }

    #[test]
    fn sidebar_width_round_trips_and_is_bounded() {
        let state = State { window: None, sidebar_width: Some(260) };
        assert_eq!(State::parse(&state.to_toml()), state);
        assert_eq!(
            State::parse(
                "[sidebar]
width = 50
"
            )
            .sidebar_width,
            None
        );
        assert_eq!(
            State::parse(
                "[sidebar]
width = 900
"
            )
            .sidebar_width,
            None
        );
    }
}
