//! `settings.toml` (portable, may be synced) and `state.toml` (this machine only).

use crate::Warning;
use crate::shortcuts::{Platform, Shortcuts};
use gezik_core::ops::threads::{COPY_THREADS_RANGE, CopyThreads};
use gezik_core::view::{
    ColumnKey, ColumnState, GridSize, IconMode, SortDir, SortKey, ViewMode, ViewSettings, normalize_columns,
};

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

/// `[view]`: how folders look unless the user changed one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewDefaults {
    pub view: ViewSettings,
    pub icons: IconMode,
    /// Pictures and videos show a thumbnail in the grid.
    pub thumbnails: bool,
}

impl Default for ViewDefaults {
    fn default() -> Self {
        ViewDefaults { view: ViewSettings::default(), icons: IconMode::System, thumbnails: true }
    }
}

/// `[files]`: file operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FilesSettings {
    /// Ask before moving to the trash.
    pub confirm_trash: bool,
    pub copy_threads: CopyThreads,
}

/// A saved set of rename rules (`[[rename-presets]]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenamePreset {
    pub name: String,
    pub include_extension: bool,
    pub rules: Vec<gezik_core::batch::rules::RuleEntry>,
}

/// The rename layer's last rules (state.toml `[batch-rename]`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BatchRenameState {
    pub include_extension: bool,
    pub rules: Vec<gezik_core::batch::rules::RuleEntry>,
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
    pub view: ViewDefaults,
    pub files: FilesSettings,
    /// Most frames drawn per second (`MAX_FPS_RANGE`); 0 = as many as the display shows.
    pub max_fps: u32,
    /// Saved rename rule sets.
    pub rename_presets: Vec<RenamePreset>,
}

/// Allowed `max-fps` values besides 0 (no limit).
pub const MAX_FPS_RANGE: std::ops::RangeInclusive<u32> = 10..=1000;

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
            view: ViewDefaults::default(),
            files: FilesSettings::default(),
            max_fps: 120,
            rename_presets: Vec::new(),
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
        if let Some(value) = table.get("max-fps") {
            match value.as_integer().and_then(|n| u32::try_from(n).ok()) {
                Some(n) if n == 0 || MAX_FPS_RANGE.contains(&n) => settings.max_fps = n,
                _ => warnings.push(Warning::new(
                    file,
                    format!(
                        "max-fps: expected 0 (no limit) or a number from {} to {}, got {value}",
                        MAX_FPS_RANGE.start(),
                        MAX_FPS_RANGE.end()
                    ),
                )),
            }
        }
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
        match table.get("view") {
            None => {}
            Some(value) => match value.as_table() {
                Some(view) => settings.view = parse_view(view, file, warnings),
                None => warnings.push(Warning::new(file, format!("view: expected a table, got {value}"))),
            },
        }
        match table.get("files") {
            None => {}
            Some(value) => match value.as_table() {
                Some(files) => settings.files = parse_files(files, file, warnings),
                None => warnings.push(Warning::new(file, format!("files: expected a table, got {value}"))),
            },
        }
        if let Some(value) = table.get("rename-presets") {
            match value.as_array() {
                None => warnings.push(Warning::new(
                    file,
                    format!("rename-presets: expected [[rename-presets]] tables, got {value}"),
                )),
                Some(items) => {
                    for (i, item) in items.iter().enumerate() {
                        match parse_preset(item) {
                            Ok(preset) => settings.rename_presets.push(preset),
                            Err(err) => warnings.push(Warning::new(file, format!("rename-presets[{}]: {err}", i + 1))),
                        }
                    }
                }
            }
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

fn parse_view(table: &toml::Table, file: &str, warnings: &mut Vec<Warning>) -> ViewDefaults {
    let mut out = ViewDefaults::default();
    if let Some(mode) = view_choice(table, "mode", "\"list\" or \"grid\"", ViewMode::parse, file, warnings) {
        out.view.mode = mode;
    }
    let keys = "\"name\", \"modified\", \"created\", \"type\" or \"size\"";
    if let Some(key) = view_choice(table, "sort", keys, SortKey::parse, file, warnings) {
        out.view.sort.key = key;
    }
    if let Some(dir) = view_choice(table, "sort-dir", "\"asc\" or \"desc\"", SortDir::parse, file, warnings) {
        out.view.sort.dir = dir;
    }
    let sizes = "\"small\", \"medium\" or \"large\"";
    if let Some(size) = view_choice(table, "grid-size", sizes, GridSize::parse, file, warnings) {
        out.view.grid_size = size;
    }
    if let Some(icons) = view_choice(table, "icons", "\"system\" or \"gezik\"", IconMode::parse, file, warnings) {
        out.icons = icons;
    }
    if let Some(value) = table.get("thumbnails") {
        match value.as_bool() {
            Some(on) => out.thumbnails = on,
            None => warnings.push(Warning::new(file, format!("view.thumbnails: expected true or false, got {value}"))),
        }
    }
    out
}

/// `[view].key` read with `parse`: `None` if missing; a bad value also warns.
fn view_choice<T>(
    table: &toml::Table,
    key: &str,
    options: &str,
    parse: impl Fn(&str) -> Option<T>,
    file: &str,
    warnings: &mut Vec<Warning>,
) -> Option<T> {
    let value = table.get(key)?;
    let parsed = value.as_str().and_then(&parse);
    if parsed.is_none() {
        warnings.push(Warning::new(file, format!("view.{key}: expected {options}, got {value}")));
    }
    parsed
}

fn parse_rules(value: Option<&toml::Value>) -> Result<Vec<gezik_core::batch::rules::RuleEntry>, String> {
    let Some(value) = value else { return Ok(Vec::new()) };
    let items = value.as_array().ok_or_else(|| format!("rules: expected a list, got {value}"))?;
    items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let table = item.as_table().ok_or_else(|| format!("rules[{}]: expected a table", i + 1))?;
            crate::batch_toml::rule_from_toml(table).map_err(|err| format!("rules[{}]: {err}", i + 1))
        })
        .collect()
}

fn parse_preset(value: &toml::Value) -> Result<RenamePreset, String> {
    let table = value.as_table().ok_or("expected a table")?;
    let name = table.get("name").and_then(|v| v.as_str()).filter(|n| !n.trim().is_empty()).ok_or("name is missing")?;
    let include_extension = table.get("include-extension").and_then(|v| v.as_bool()).unwrap_or(false);
    Ok(RenamePreset { name: name.to_owned(), include_extension, rules: parse_rules(table.get("rules"))? })
}

pub fn preset_to_toml(preset: &RenamePreset) -> toml::Table {
    let mut table = toml::Table::new();
    table.insert("name".into(), toml::Value::String(preset.name.clone()));
    table.insert("include-extension".into(), toml::Value::Boolean(preset.include_extension));
    let rules = preset.rules.iter().map(|r| toml::Value::Table(crate::batch_toml::rule_to_toml(r))).collect();
    table.insert("rules".into(), toml::Value::Array(rules));
    table
}

fn parse_files(table: &toml::Table, file: &str, warnings: &mut Vec<Warning>) -> FilesSettings {
    let mut out = FilesSettings::default();
    if let Some(value) = table.get("confirm-trash") {
        match value.as_bool() {
            Some(on) => out.confirm_trash = on,
            None => {
                warnings.push(Warning::new(file, format!("files.confirm-trash: expected true or false, got {value}")))
            }
        }
    }
    if let Some(value) = table.get("copy-threads") {
        let parsed = match value {
            toml::Value::String(text) if text.eq_ignore_ascii_case("auto") => Some(CopyThreads::Auto),
            toml::Value::Integer(n) => {
                u8::try_from(*n).ok().filter(|n| COPY_THREADS_RANGE.contains(n)).map(CopyThreads::Fixed)
            }
            _ => None,
        };
        match parsed {
            Some(threads) => out.copy_threads = threads,
            None => warnings.push(Warning::new(
                file,
                format!(
                    "files.copy-threads: expected \"auto\" or a number from {} to {}, got {value}",
                    COPY_THREADS_RANGE.start(),
                    COPY_THREADS_RANGE.end()
                ),
            )),
        }
    }
    out
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
    /// The list's columns; `None` until first saved (the defaults are used then).
    pub columns: Option<Vec<ColumnState>>,
    pub preview_open: bool,
    /// Preview pane width in logical pixels (200–600).
    pub preview_width: Option<u32>,
    /// The operations panel is folded into the status bar.
    pub operations_collapsed: bool,
    /// The rename layer's last rules.
    pub batch_rename: Option<BatchRenameState>,
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
        let columns = table.get("columns").and_then(|v| v.as_array()).map(|items| {
            let saved: Vec<ColumnState> = items
                .iter()
                .filter_map(|item| {
                    let item = item.as_table()?;
                    Some(ColumnState {
                        key: ColumnKey::parse(item.get("key")?.as_str()?)?,
                        visible: item.get("visible").and_then(|v| v.as_bool()).unwrap_or(true),
                        width: u32::try_from(item.get("width")?.as_integer()?).ok()?,
                    })
                })
                .collect();
            normalize_columns(&saved)
        });
        let preview = table.get("preview").and_then(|v| v.as_table());
        let preview_open = preview.and_then(|p| p.get("open")).and_then(|v| v.as_bool()).unwrap_or(false);
        let preview_width = preview
            .and_then(|p| p.get("width"))
            .and_then(|v| v.as_integer())
            .and_then(|w| u32::try_from(w).ok())
            .filter(|w| (200..=600).contains(w));
        let operations_collapsed = table
            .get("operations")
            .and_then(|v| v.as_table())
            .and_then(|o| o.get("panel-collapsed"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let batch_rename = table.get("batch-rename").and_then(|v| v.as_table()).map(|t| BatchRenameState {
            include_extension: t.get("include-extension").and_then(|v| v.as_bool()).unwrap_or(false),
            // The app wrote it: a broken rule just drops the list.
            rules: parse_rules(t.get("rules")).unwrap_or_default(),
        });
        State { window, sidebar_width, columns, preview_open, preview_width, operations_collapsed, batch_rename }
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
        if let Some(columns) = &self.columns {
            let items = columns
                .iter()
                .map(|c| {
                    let mut column = toml::Table::new();
                    column.insert("key".into(), toml::Value::String(c.key.as_str().into()));
                    column.insert("visible".into(), toml::Value::Boolean(c.visible));
                    column.insert("width".into(), toml::Value::Integer(c.width.into()));
                    toml::Value::Table(column)
                })
                .collect();
            root.insert("columns".into(), toml::Value::Array(items));
        }
        if self.preview_open || self.preview_width.is_some() {
            let mut preview = toml::Table::new();
            preview.insert("open".into(), toml::Value::Boolean(self.preview_open));
            if let Some(width) = self.preview_width {
                preview.insert("width".into(), toml::Value::Integer(width.into()));
            }
            root.insert("preview".into(), toml::Value::Table(preview));
        }
        if self.operations_collapsed {
            let mut operations = toml::Table::new();
            operations.insert("panel-collapsed".into(), toml::Value::Boolean(true));
            root.insert("operations".into(), toml::Value::Table(operations));
        }
        if let Some(batch) = &self.batch_rename {
            let mut table = toml::Table::new();
            table.insert("include-extension".into(), toml::Value::Boolean(batch.include_extension));
            let rules = batch.rules.iter().map(|r| toml::Value::Table(crate::batch_toml::rule_to_toml(r))).collect();
            table.insert("rules".into(), toml::Value::Array(rules));
            root.insert("batch-rename".into(), toml::Value::Table(table));
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
            ..State::default()
        };
        assert_eq!(State::parse(&state.to_toml()), state);
        let no_position = State {
            window: Some(WindowState { width: 800, height: 600, x: None, y: None }),
            sidebar_width: None,
            ..State::default()
        };
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
    fn max_fps_defaults_to_120_and_zero_means_unlimited() {
        assert_eq!(Settings::default().max_fps, 120);
        let (settings, warnings) = parse("max-fps = 60\n");
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(settings.max_fps, 60);
        assert_eq!(parse("max-fps = 0\n").0.max_fps, 0);
    }

    #[test]
    fn bad_max_fps_keeps_the_default_with_a_warning() {
        for text in ["max-fps = 5\n", "max-fps = -1\n", "max-fps = \"fast\"\n", "max-fps = 100000\n"] {
            let (settings, warnings) = parse(text);
            assert_eq!(settings.max_fps, 120, "{text}");
            assert_eq!(warnings.len(), 1, "{text}");
            assert!(warnings[0].message.starts_with("max-fps:"), "{}", warnings[0].message);
        }
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
        let state = State { window: None, sidebar_width: Some(260), ..State::default() };
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

    #[test]
    fn reads_the_view_table() {
        use gezik_core::view::{GridSize, IconMode, SortDir, SortKey, ViewMode};
        let (settings, warnings) = parse(
            "[view]\nmode = \"grid\"\nsort = \"size\"\nsort-dir = \"desc\"\ngrid-size = \"large\"\n\
             icons = \"gezik\"\nthumbnails = false\n",
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        let v = settings.view;
        assert_eq!((v.view.mode, v.view.sort.key, v.view.sort.dir), (ViewMode::Grid, SortKey::Size, SortDir::Desc));
        assert_eq!((v.view.grid_size, v.icons, v.thumbnails), (GridSize::Large, IconMode::Gezik, false));
    }

    #[test]
    fn bad_view_values_keep_defaults_with_warnings() {
        let (settings, warnings) =
            parse("[view]\nmode = \"tiles\"\nsort-dir = 1\nthumbnails = \"yes\"\nicons = \"system\"\n");
        assert_eq!(settings.view, ViewDefaults::default());
        let messages: Vec<_> = warnings.iter().map(|w| w.message.as_str()).collect();
        assert_eq!(messages.len(), 3, "{messages:?}");
        assert!(messages[0].starts_with("view.mode:") && messages[0].contains("\"tiles\""));
        assert!(messages[1].starts_with("view.sort-dir:"));
        assert!(messages[2].starts_with("view.thumbnails:"));
        let (_, warnings) = parse("view = 3\n");
        assert!(warnings[0].message.starts_with("view: expected a table"));
    }

    #[test]
    fn columns_and_preview_round_trip() {
        use gezik_core::view::{ColumnKey, default_columns};
        let mut columns = default_columns();
        columns[1].visible = true;
        columns[3].width = 120;
        let state =
            State { columns: Some(columns.clone()), preview_open: true, preview_width: Some(320), ..State::default() };
        let back = State::parse(&state.to_toml());
        assert_eq!(back, state);
        assert_eq!(back.columns.unwrap()[3].key, ColumnKey::Size);
    }

    #[test]
    fn broken_columns_and_preview_values_are_repaired() {
        let state = State::parse(
            "[[columns]]\nkey = \"size\"\nwidth = 3\n\n[[columns]]\nkey = \"bogus\"\nwidth = 100\n\n\
             [[columns]]\nwidth = 100\n\n[preview]\nopen = true\nwidth = 9000\n",
        );
        let columns = state.columns.unwrap();
        assert_eq!(columns.len(), 4);
        assert_eq!((columns[3].width, columns[3].visible), (gezik_core::view::MIN_COLUMN_WIDTH, true));
        assert!(state.preview_open);
        assert_eq!(state.preview_width, None);
        assert_eq!(State::parse("").columns, None);
    }

    #[test]
    fn reads_the_files_table() {
        use gezik_core::ops::threads::CopyThreads;
        let (settings, warnings) = parse("[files]\nconfirm-trash = true\ncopy-threads = 3\n");
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(settings.files, FilesSettings { confirm_trash: true, copy_threads: CopyThreads::Fixed(3) });
        let (settings, _) = parse("[files]\ncopy-threads = \"Auto\"\n");
        assert_eq!(settings.files.copy_threads, CopyThreads::Auto);
        assert_eq!(Settings::default().files, FilesSettings::default());
    }

    #[test]
    fn bad_files_values_keep_defaults_with_warnings() {
        let (settings, warnings) = parse("[files]\nconfirm-trash = \"yes\"\ncopy-threads = 40\n");
        assert_eq!(settings.files, FilesSettings::default());
        let messages: Vec<_> = warnings.iter().map(|w| w.message.as_str()).collect();
        assert_eq!(messages.len(), 2, "{messages:?}");
        assert!(messages[0].starts_with("files.confirm-trash:"));
        assert!(messages[1].starts_with("files.copy-threads:") && messages[1].contains("1 to 16"));
        let (_, warnings) = parse("files = 1\n");
        assert!(warnings[0].message.starts_with("files: expected a table"));
    }

    #[test]
    fn operations_panel_state_round_trips() {
        let state = State { operations_collapsed: true, ..State::default() };
        assert_eq!(State::parse(&state.to_toml()), state);
        assert!(!State::default().to_toml().contains("operations"), "the default is not written");
    }

    #[test]
    fn rename_presets_are_read_and_bad_ones_warned() {
        let text = r#"
[[rename-presets]]
name = "Tatil"
rules = [{ kind = "template", text = "{taken} {n:03}" }, { kind = "case", mode = "lower" }]

[[rename-presets]]
rules = []
"#;
        let mut warnings = Vec::new();
        let settings = Settings::parse("settings.toml", text, &mut warnings);
        assert_eq!(settings.rename_presets.len(), 1);
        assert_eq!(settings.rename_presets[0].rules.len(), 2);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].to_string().contains("rename-presets[2]: name is missing"), "{}", warnings[0]);
    }

    #[test]
    fn last_rename_rules_survive_state() {
        use gezik_core::batch::rules::{Rule, RuleEntry};
        let state = State {
            batch_rename: Some(BatchRenameState {
                include_extension: true,
                rules: vec![RuleEntry::new(Rule::Template("{name}".into()))],
            }),
            ..State::default()
        };
        assert_eq!(State::parse(&state.to_toml()), state);
    }
}
