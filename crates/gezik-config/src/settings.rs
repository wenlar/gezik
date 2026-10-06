//! `settings.toml` (portable, may be synced) and `state.toml` (this machine only).

use crate::Warning;
use crate::shortcuts::{Platform, Shortcuts};
use gezik_core::batch::convert::{CommandSpec, check_command};
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

/// `[tools]`: the programs Gezik may download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolsSettings {
    /// Offer to download 7-Zip (later ffmpeg and pdfium) when needed; off, only say so.
    pub download: bool,
    /// The 7-Zip to use (`seven-zip`); `None`: Gezik's download, then PATH.
    pub seven_zip: Option<String>,
}

impl Default for ToolsSettings {
    fn default() -> Self {
        ToolsSettings { download: true, seven_zip: None }
    }
}

/// `[convert]`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ConvertSettings {
    /// The ffmpeg to use (`ffmpeg`), an absolute path; `None`: Gezik's download, then PATH.
    pub ffmpeg: Option<String>,
}

/// The Convert layer's last choices (state.toml `[convert]`). The options are the layer's own
/// text for them, read back by the layer.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ConvertState {
    pub last_preset: Option<String>,
    pub image: Option<String>,
    pub text: Option<String>,
    pub media: Option<String>,
    /// The output choice (empty: none saved).
    pub last_output: String,
    pub last_folder: Option<String>,
}

/// What double-clicking an archive does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DoubleClick {
    /// Opens it with the system's program for it.
    #[default]
    System,
    /// Extracts it next to itself.
    ExtractHere,
}

/// `[archives]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ArchivesSettings {
    pub double_click: DoubleClick,
}

/// The Compress layer's last choices and the last "Extract to…" folder (state.toml
/// `[archive]`). Never a password.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ArchiveState {
    /// The format's ending: `zip`, `7z`, `tar`, `tar.gz`, `tar.xz`, `gz` or `xz`.
    pub format: Option<String>,
    /// `store`, `fast`, `normal` or `best`.
    pub level: Option<String>,
    /// 7z parts of this many bytes. Still read, but always cleared now (Compress sets it to
    /// `None`, which is not written): the layer opens with Split off.
    pub split: Option<u64>,
    pub last_extract_to: Option<String>,
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
    pub archives: ArchivesSettings,
    pub tools: ToolsSettings,
    pub convert: ConvertSettings,
    /// User commands (`[[commands]]`); invalid ones are left out.
    pub commands: Vec<CommandSpec>,
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
            archives: ArchivesSettings::default(),
            tools: ToolsSettings::default(),
            convert: ConvertSettings::default(),
            commands: Vec::new(),
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
        match table.get("archives") {
            None => {}
            Some(value) => match value.as_table() {
                Some(archives) => settings.archives = parse_archives(archives, file, warnings),
                None => warnings.push(Warning::new(file, format!("archives: expected a table, got {value}"))),
            },
        }
        match table.get("tools") {
            None => {}
            Some(value) => match value.as_table() {
                Some(tools) => settings.tools = parse_tools(tools, file, warnings),
                None => warnings.push(Warning::new(file, format!("tools: expected a table, got {value}"))),
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
        match table.get("convert") {
            None => {}
            Some(value) => match value.as_table() {
                Some(convert) => settings.convert = parse_convert(convert, file, warnings),
                None => warnings.push(Warning::new(file, format!("convert: expected a table, got {value}"))),
            },
        }
        if let Some(value) = table.get("commands") {
            match value.as_array() {
                None => {
                    warnings.push(Warning::new(file, format!("commands: expected [[commands]] tables, got {value}")))
                }
                Some(items) => {
                    for (i, item) in items.iter().enumerate() {
                        match parse_command(item) {
                            Ok(command) => settings.commands.push(command),
                            Err(err) => warnings.push(Warning::new(file, format!("commands[{}]: {err}", i + 1))),
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

pub(crate) fn parse_preset(value: &toml::Value) -> Result<RenamePreset, String> {
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

fn parse_archives(table: &toml::Table, file: &str, warnings: &mut Vec<Warning>) -> ArchivesSettings {
    let mut out = ArchivesSettings::default();
    if let Some(value) = table.get("double-click") {
        match value.as_str() {
            Some("system") => out.double_click = DoubleClick::System,
            Some("extract-here") => out.double_click = DoubleClick::ExtractHere,
            _ => warnings.push(Warning::new(
                file,
                format!("archives.double-click: expected \"system\" or \"extract-here\", got {value}"),
            )),
        }
    }
    out
}

fn parse_tools(table: &toml::Table, file: &str, warnings: &mut Vec<Warning>) -> ToolsSettings {
    let mut out = ToolsSettings::default();
    if let Some(value) = table.get("download") {
        match value.as_bool() {
            Some(on) => out.download = on,
            None => warnings.push(Warning::new(file, format!("tools.download: expected true or false, got {value}"))),
        }
    }
    if let Some(value) = table.get("seven-zip") {
        match value.as_str() {
            // Empty: none set.
            Some(path) if path.trim().is_empty() => {}
            // A relative path would depend on the folder Gezik was started in.
            Some(path) if !std::path::Path::new(path).is_absolute() => warnings
                .push(Warning::new(file, format!("tools.seven-zip: \"{path}\" must be a full path (it is ignored)"))),
            Some(path) => out.seven_zip = Some(path.to_owned()),
            None => warnings.push(Warning::new(file, format!("tools.seven-zip: expected text, got {value}"))),
        }
    }
    out
}

fn parse_convert(table: &toml::Table, file: &str, warnings: &mut Vec<Warning>) -> ConvertSettings {
    let mut out = ConvertSettings::default();
    if let Some(value) = table.get("ffmpeg") {
        match value.as_str() {
            Some(path) if path.trim().is_empty() => {}
            Some(path) if !std::path::Path::new(path).is_absolute() => warnings
                .push(Warning::new(file, format!("convert.ffmpeg: \"{path}\" must be a full path (it is ignored)"))),
            Some(path) => out.ffmpeg = Some(path.to_owned()),
            None => warnings.push(Warning::new(file, format!("convert.ffmpeg: expected text, got {value}"))),
        }
    }
    out
}

/// A list of strings, or an error naming the key.
fn string_list(value: &toml::Value, key: &str) -> Result<Vec<String>, String> {
    let items = value.as_array().ok_or_else(|| format!("{key} must be a list of text, got {value}"))?;
    items
        .iter()
        .map(|item| item.as_str().map(str::to_owned).ok_or_else(|| format!("{key} must be a list of text, got {item}")))
        .collect()
}

/// One `[[commands]]` entry; the error is the reason it is left out.
fn parse_command(value: &toml::Value) -> Result<CommandSpec, String> {
    let table = value.as_table().ok_or_else(|| format!("expected a table, got {value}"))?;
    // A misspelt key would be passed over and widen what the command runs on (`type` for
    // `types`: every file): the command is left out instead.
    const KEYS: [&str; 6] = ["name", "run", "output", "types", "folders", "parallel"];
    if let Some(key) = table.keys().find(|key| !KEYS.contains(&key.as_str())) {
        return Err(format!("unknown key \"{key}\" (known: {})", KEYS.join(", ")));
    }
    let name = match table.get("name") {
        None => return Err("name is missing".to_owned()),
        Some(v) => v.as_str().filter(|n| !n.trim().is_empty()).ok_or("name must be text that is not empty")?,
    };
    let run = match table.get("run") {
        None => return Err("run is missing".to_owned()),
        Some(v) => string_list(v, "run")?,
    };
    if run.is_empty() {
        return Err("run has no program".to_owned());
    }
    let output = match table.get("output") {
        None => None,
        Some(v) => Some(v.as_str().ok_or_else(|| format!("output must be text, got {v}"))?.to_owned()),
    };
    let types = match table.get("types") {
        None => Vec::new(),
        Some(v) => string_list(v, "types")?,
    };
    // Endings without the dot ("jpg"; a leading dot is dropped, "tar.gz" is fine). A pattern
    // or a path would never match: the command is left out rather than shown for nothing.
    let types = types
        .iter()
        .map(|t| {
            let t = t.trim();
            let t = t.strip_prefix('.').unwrap_or(t);
            if t.is_empty() || t.contains(['*', '?', '/', '\\']) {
                Err(format!("types: \"{t}\" is not a file ending (write \"jpg\", no * or ?)"))
            } else {
                Ok(t.to_owned())
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    let folders = match table.get("folders") {
        None => false,
        Some(v) => v.as_bool().ok_or_else(|| format!("folders must be true or false, got {v}"))?,
    };
    let parallel = match table.get("parallel") {
        None => 1,
        Some(v) => v
            .as_integer()
            .and_then(|n| u8::try_from(n).ok())
            .filter(|n| (1..=16).contains(n))
            .ok_or_else(|| format!("parallel must be 1-16, got {v}"))?,
    };
    let spec = CommandSpec { name: name.to_owned(), run, output, types, folders, parallel };
    check_command(&spec)?;
    Ok(spec)
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
    pub archive: ArchiveState,
    pub convert: ConvertState,
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
        let archive = table.get("archive").and_then(|v| v.as_table()).map_or_else(ArchiveState::default, |t| {
            let text = |key: &str| t.get(key).and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(str::to_owned);
            let split = t.get("split").and_then(|v| v.as_integer()).and_then(|n| u64::try_from(n).ok());
            ArchiveState {
                format: text("format"),
                level: text("level"),
                split: split.filter(|n| *n > 0),
                last_extract_to: text("last-extract-to"),
            }
        });
        let convert = table.get("convert").and_then(|v| v.as_table()).map_or_else(ConvertState::default, |t| {
            let text = |key: &str| t.get(key).and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(str::to_owned);
            ConvertState {
                last_preset: text("last-preset"),
                image: text("image"),
                text: text("text"),
                media: text("media"),
                last_output: text("last-output").unwrap_or_default(),
                last_folder: text("last-folder"),
            }
        });
        State {
            window,
            sidebar_width,
            columns,
            preview_open,
            preview_width,
            operations_collapsed,
            batch_rename,
            archive,
            convert,
        }
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
        if self.archive != ArchiveState::default() {
            let archive = &self.archive;
            let mut table = toml::Table::new();
            let texts =
                [("format", &archive.format), ("level", &archive.level), ("last-extract-to", &archive.last_extract_to)];
            for (key, value) in texts {
                if let Some(value) = value {
                    table.insert(key.into(), toml::Value::String(value.clone()));
                }
            }
            if let Some(split) = archive.split.and_then(|n| i64::try_from(n).ok()) {
                table.insert("split".into(), toml::Value::Integer(split));
            }
            root.insert("archive".into(), toml::Value::Table(table));
        }
        if self.convert != ConvertState::default() {
            let convert = &self.convert;
            let mut table = toml::Table::new();
            let texts = [
                ("last-preset", &convert.last_preset),
                ("image", &convert.image),
                ("text", &convert.text),
                ("media", &convert.media),
                ("last-folder", &convert.last_folder),
            ];
            for (key, value) in texts {
                if let Some(value) = value {
                    table.insert(key.into(), toml::Value::String(value.clone()));
                }
            }
            if !convert.last_output.is_empty() {
                table.insert("last-output".into(), toml::Value::String(convert.last_output.clone()));
            }
            root.insert("convert".into(), toml::Value::Table(table));
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

    #[test]
    fn reads_archives_and_tools() {
        let program = if cfg!(windows) { "C:/7-Zip/7z.exe" } else { "/opt/7-Zip/7zz" };
        let (settings, warnings) = parse(&format!(
            "[archives]\ndouble-click = \"extract-here\"\n[tools]\ndownload = false\nseven-zip = \"{program}\"\n"
        ));
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(settings.archives.double_click, DoubleClick::ExtractHere);
        assert_eq!(settings.tools, ToolsSettings { download: false, seven_zip: Some(program.to_owned()) });
        let defaults = Settings::default();
        assert_eq!((defaults.archives.double_click, defaults.tools.download), (DoubleClick::System, true));
        // Empty means none set.
        let (settings, warnings) = parse("[tools]\nseven-zip = \"\"\n");
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(settings.tools.seven_zip, None);
        // A relative path is ignored with a warning.
        let (settings, warnings) = parse("[tools]\nseven-zip = \"7-Zip/7z.exe\"\n");
        assert_eq!(settings.tools.seven_zip, None);
        assert!(warnings[0].message.contains("must be a full path"), "{warnings:?}");
    }

    #[test]
    fn bad_archives_and_tools_values_keep_defaults_with_warnings() {
        let (settings, warnings) =
            parse("[archives]\ndouble-click = \"open\"\n[tools]\ndownload = \"yes\"\nseven-zip = 7\n");
        assert_eq!(settings.archives, ArchivesSettings::default());
        assert_eq!(settings.tools, ToolsSettings::default());
        let messages: Vec<_> = warnings.iter().map(|w| w.message.as_str()).collect();
        assert_eq!(messages.len(), 3, "{messages:?}");
        assert!(messages[0].starts_with("archives.double-click:") && messages[0].contains("\"open\""));
        assert!(messages[1].starts_with("tools.download:"));
        assert!(messages[2].starts_with("tools.seven-zip:"));
        let (_, warnings) = parse("tools = 1\narchives = \"x\"\n");
        let messages: Vec<_> = warnings.iter().map(|w| w.message.as_str()).collect();
        assert!(messages.iter().any(|m| m.starts_with("tools: expected a table")), "{messages:?}");
        assert!(messages.iter().any(|m| m.starts_with("archives: expected a table")), "{messages:?}");
    }

    #[test]
    fn the_template_reads_with_the_defaults() {
        let (settings, warnings) = parse(include_str!("../templates/settings.toml"));
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(settings.archives, ArchivesSettings::default());
        assert_eq!(settings.tools, ToolsSettings::default());
    }

    #[test]
    fn archive_state_round_trips() {
        let state = State {
            archive: ArchiveState {
                format: Some("7z".to_owned()),
                level: Some("best".to_owned()),
                split: Some(100 * 1024 * 1024),
                last_extract_to: Some("D:/Out".to_owned()),
            },
            ..State::default()
        };
        assert_eq!(State::parse(&state.to_toml()), state);
        assert!(!State::default().to_toml().contains("archive"), "the default is not written");
        let broken = State::parse("[archive]\nformat = 3\nsplit = -5\nlevel = \"\"\n");
        assert_eq!(broken.archive, ArchiveState::default());
    }

    #[test]
    fn reads_convert_and_commands() {
        let program = if cfg!(windows) { "C:/ffmpeg/ffmpeg.exe" } else { "/opt/ffmpeg/ffmpeg" };
        let (settings, warnings) = parse(&format!(
            "[convert]\nffmpeg = \"{program}\"\n\n[[commands]]\nname = \"Small\"\nrun = [\"magick\", \"{{in}}\", \"{{out}}\"]\n\
             output = \"{{name}}-small.{{ext}}\"\ntypes = [\"jpg\", \"png\"]\nfolders = true\nparallel = 4\n\n\
             [[commands]]\nname = \"Min\"\nrun = [\"touch\", \"{{in}}\"]\n"
        ));
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(settings.convert.ffmpeg.as_deref(), Some(program));
        assert_eq!(settings.commands.len(), 2);
        assert_eq!(
            settings.commands[0],
            CommandSpec {
                name: "Small".to_owned(),
                run: vec!["magick".to_owned(), "{in}".to_owned(), "{out}".to_owned()],
                output: Some("{name}-small.{ext}".to_owned()),
                types: vec!["jpg".to_owned(), "png".to_owned()],
                folders: true,
                parallel: 4,
            }
        );
        let min = &settings.commands[1];
        assert_eq!((min.output.as_deref(), min.types.len(), min.folders, min.parallel), (None, 0, false, 1));
        let defaults = Settings::default();
        assert_eq!((defaults.convert.ffmpeg, defaults.commands.len()), (None, 0));
        let (settings, warnings) = parse("[convert]\nffmpeg = \"\"\n");
        assert!(warnings.is_empty() && settings.convert.ffmpeg.is_none(), "{warnings:?}");
    }

    #[test]
    fn relative_or_bad_ffmpeg_warns() {
        let (settings, warnings) = parse("[convert]\nffmpeg = \"bin/ffmpeg\"\n");
        assert_eq!(settings.convert.ffmpeg, None);
        assert!(warnings[0].message.contains("convert.ffmpeg") && warnings[0].message.contains("full path"));
        let (_, warnings) = parse("[convert]\nffmpeg = 3\n");
        assert!(warnings[0].message.starts_with("convert.ffmpeg: expected text"), "{warnings:?}");
        let (_, warnings) = parse("convert = 1\ncommands = 2\n");
        let messages: Vec<_> = warnings.iter().map(|w| w.message.as_str()).collect();
        assert!(messages.iter().any(|m| m.starts_with("convert: expected a table")), "{messages:?}");
        assert!(messages.iter().any(|m| m.starts_with("commands: expected [[commands]]")), "{messages:?}");
    }

    #[test]
    fn invalid_commands_warn_and_are_skipped() {
        let bad = [
            ("run = [\"x\"]", "name is missing"),
            ("name = \"\"\nrun = [\"x\"]", "name must be"),
            ("name = \"A\"", "run is missing"),
            ("name = \"A\"\nrun = []", "run has no program"),
            ("name = \"A\"\nrun = \"x\"", "run must be a list of text"),
            ("name = \"A\"\nrun = [\"x\", 3]", "run must be a list of text"),
            ("name = \"A\"\nrun = [\"x\"]\noutput = 5", "output must be text"),
            ("name = \"A\"\nrun = [\"x\"]\ntypes = \"jpg\"", "types must be a list of text"),
            ("name = \"A\"\nrun = [\"x\"]\nfolders = \"yes\"", "folders must be true or false"),
            ("name = \"A\"\nrun = [\"x\"]\nparallel = 0", "parallel must be 1-16"),
            ("name = \"A\"\nrun = [\"x\"]\nparallel = 17", "parallel must be 1-16"),
            ("name = \"A\"\nrun = [\"x\"]\nparallel = \"2\"", "parallel must be 1-16"),
            ("name = \"A\"\nrun = [\"x\", \"{nope}\"]", "unknown placeholder"),
            ("name = \"A\"\nrun = [\"x\", \"{out}\"]", "{out} needs an output"),
            ("name = \"A\"\nrun = [\"x\"]\noutput = \"a/b\"", "file name"),
            ("name = \"A\"\nrun = [\"x\"]\ntype = [\"jpg\"]", "unknown key \"type\""),
            ("name = \"A\"\nrun = [\"x\"]\nparalel = 2", "unknown key \"paralel\""),
            ("name = \"A\"\nrun = [\"x\"]\ntypes = [\"*.jpg\"]", "types: \"*.jpg\""),
            ("name = \"A\"\nrun = [\"x\"]\ntypes = [\"jpg\", \"photos/x\"]", "is not a file ending"),
            ("name = \"A\"\nrun = [\"x\"]\ntypes = [\".\"]", "is not a file ending"),
        ];
        for (body, expected) in bad {
            let (settings, warnings) = parse(&format!("[[commands]]\n{body}\n"));
            assert!(settings.commands.is_empty(), "{body}");
            assert_eq!(warnings.len(), 1, "{body}: {warnings:?}");
            let message = &warnings[0].message;
            assert!(message.starts_with("commands[1]: ") && message.contains(expected), "{body}: {message}");
        }
        // The good one stays; the bad one is numbered by its place.
        let (settings, warnings) = parse("[[commands]]\nname = \"Ok\"\nrun = [\"x\"]\n[[commands]]\nname = \"Bad\"\n");
        assert_eq!(settings.commands.len(), 1);
        assert!(warnings[0].message.starts_with("commands[2]: "), "{warnings:?}");
    }

    #[test]
    fn the_template_documents_escaped_braces() {
        let template = include_str!("../templates/settings.toml");
        assert!(template.contains("\"{{\" and \"}}\""));
        assert!(template.contains("--outdir") && template.contains("ebook-convert") && template.contains("magick"));
    }

    #[test]
    fn the_template_command_examples_read_without_warnings_once_uncommented() {
        let template = include_str!("../templates/settings.toml");
        let start = template.find("# [[commands]]").expect("the template has command examples");
        let examples: String = template[start..]
            .lines()
            .take_while(|line| line.starts_with('#'))
            .map(|line| line.strip_prefix("# ").or_else(|| line.strip_prefix('#')).unwrap_or(line))
            .map(|line| format!("{line}\n"))
            .collect();
        let (settings, warnings) = parse(&examples);
        assert!(warnings.is_empty(), "{warnings:?}\n{examples}");
        let names: Vec<&str> = settings.commands.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Resize to 50% (ImageMagick)", "Office to PDF (LibreOffice)", "E-book to EPUB (Calibre)"]);
    }

    #[test]
    fn a_leading_dot_in_types_is_dropped() {
        let (settings, warnings) = parse("[[commands]]\nname = \"A\"\nrun = [\"x\"]\ntypes = [\".JPG\", \"tar.gz\"]\n");
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(settings.commands[0].types, ["JPG", "tar.gz"]);
    }

    #[test]
    fn convert_state_round_trips() {
        let state = State {
            convert: ConvertState {
                last_preset: Some("Web".to_owned()),
                image: Some("jpeg;q=85".to_owned()),
                text: Some("utf-8;lf".to_owned()),
                media: Some("mp3-192".to_owned()),
                last_output: "folder".to_owned(),
                last_folder: Some("D:/Out".to_owned()),
            },
            ..State::default()
        };
        assert_eq!(State::parse(&state.to_toml()), state);
        assert!(!State::default().to_toml().contains("convert"), "the default is not written");
        let broken = State::parse("[convert]\nimage = 3\nlast-output = 4\nlast-folder = \"\"\n");
        assert_eq!(broken.convert, ConvertState::default());
    }
}
