//! `settings.toml` (portable, may be synced) and `state.toml` (this machine only).

use crate::Warning;
use crate::pins::{PinEntry, find as find_pin, parse_pin};
use crate::shortcuts::{KeyOwner, Platform, Shortcuts, fixed_owner, parse_chord};
use gezik_core::batch::convert::{CommandSpec, check_command};
use gezik_core::group::GroupBy;
use gezik_core::history::Visit;
use gezik_core::nav::{Location, Session, SessionTab};
use gezik_core::ops::threads::{COPY_THREADS_RANGE, CopyThreads};
use gezik_core::search::{DateRange, HiddenRule, KindFilter, Scope, SearchSpec, parse_size, size_text};
use gezik_core::view::{
    ColumnKey, ColumnState, DateFormat, GridSize, IconMode, SizeFormat, SortDir, SortKey, ViewMode, ViewOptions,
    ViewSettings, normalize_columns,
};
use std::path::PathBuf;

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
    /// The options that are the same in every folder (spec 7.1).
    pub options: ViewOptions,
}

impl Default for ViewDefaults {
    fn default() -> Self {
        ViewDefaults {
            view: ViewSettings::default(),
            icons: IconMode::System,
            thumbnails: true,
            options: ViewOptions::default(),
        }
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
    /// The PDF group's choices (`op=split split=every every=10 dpi=150 …`); never a page range.
    pub pdf: Option<String>,
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

/// What Space shows on macOS (`[system] quick-look`, spec 9 §4.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QuickLookMode {
    /// The system's Quick Look panel.
    #[default]
    System,
    /// Gezik's own quick look window (always so on Windows and Linux).
    Gezik,
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

/// What typing a letter on the file list does (`[keyboard] typing`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Typing {
    /// Go to the first name starting with what was typed.
    #[default]
    Jump,
    /// Open the filter with it.
    Filter,
}

/// `[keyboard]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KeyboardSettings {
    pub typing: Typing,
}

/// `[history]`: the folders the address bar remembers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistorySettings {
    /// Keep the folders opened (in state.toml); off also forgets them.
    pub remember: bool,
}

impl Default for HistorySettings {
    fn default() -> Self {
        HistorySettings { remember: true }
    }
}

/// `[session]`: the tabs of last time at start (kept in state.toml).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionSettings {
    pub restore: bool,
}

impl Default for SessionSettings {
    fn default() -> Self {
        SessionSettings { restore: true }
    }
}

/// `[system]` (spec 13.1): how Gezik meets the system: the single instance (9b1) and what
/// Space shows on macOS (9a2); the tray, the hotkey and start at login come with their parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemSettings {
    /// A second `gezik` opens in the running window (spec 5.2). Read at start only.
    pub single_instance: bool,
    pub quick_look: QuickLookMode,
}

impl Default for SystemSettings {
    fn default() -> Self {
        SystemSettings { single_instance: true, quick_look: QuickLookMode::default() }
    }
}

/// `[terminal]`: the command "Open terminal" runs instead of the one Gezik finds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TerminalSettings {
    /// A program and its arguments; `{dir}` is the folder.
    pub command: Option<Vec<String>>,
}

/// A tab set (`[[tab-sets]]`): tabs opened together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabSet {
    pub name: String,
    /// Paths with `{home}`-style tokens, or "drives" (This PC).
    pub tabs: Vec<String>,
}

/// A saved filter (`[[filters]]`): a name and a pattern of the filter's own language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedFilter {
    pub name: String,
    pub pattern: String,
}

/// A saved search (`[[searches]]`, spec 8): its name, its folder as written (`{here}`: the place
/// shown when it runs; `drives`: This PC; else a path, tokens like `{home}` allowed) and the
/// search, whose scope the one who runs it sets (`Scope::AllDrives` until then).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedSearch {
    pub name: String,
    pub folder: String,
    pub spec: SearchSpec,
}

pub const SEARCHES_MAX: usize = 30;
/// A saved search's folder that is the place shown when it runs.
pub const HERE: &str = "{here}";
const SEARCH_KEYS: [&str; 13] = [
    "name",
    "folder",
    "pattern",
    "content",
    "name-regex",
    "content-regex",
    "match-case",
    "size-min",
    "size-max",
    "modified",
    "type",
    "hidden",
    "skipped",
];

/// One `[[searches]]` entry; the error is why it is left out (sapma 17: whether the folder is a
/// full path on this computer is seen when it runs).
pub(crate) fn parse_saved_search(value: &toml::Value) -> Result<SavedSearch, String> {
    let table = value.as_table().ok_or_else(|| format!("expected a table, got {value}"))?;
    let name =
        table.get("name").and_then(|v| v.as_str()).map(str::trim).filter(|n| !n.is_empty()).ok_or("name is missing")?;
    let folder = table
        .get("folder")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|f| !f.is_empty())
        .ok_or("folder is missing")?;
    if crate::paths::has_parent_segment(folder) {
        return Err(format!("folder: \"{folder}\" must not contain \"..\""));
    }
    if let Some(key) = table.keys().find(|key| !SEARCH_KEYS.contains(&key.as_str())) {
        return Err(format!("unknown key \"{key}\""));
    }
    // The rest reads as a search tab does; "drives" stands in for the folder until it runs.
    let mut rest = table.clone();
    rest.remove("name");
    rest.insert("folder".into(), toml::Value::String("drives".into()));
    let spec = search_from_toml(&rest)?;
    Ok(SavedSearch { name: name.to_owned(), folder: folder.to_owned(), spec })
}

/// `search` as its `[[searches]]` table: name, folder, then what differs from a new search.
pub fn saved_search_to_toml(search: &SavedSearch) -> toml::Table {
    let mut table = toml::Table::new();
    table.insert("name".into(), toml::Value::String(search.name.clone()));
    for (key, value) in search_to_toml(&search.spec) {
        let value = if key == "folder" { toml::Value::String(search.folder.clone()) } else { value };
        table.insert(key, value);
    }
    table
}

/// The last pattern of "Select by pattern" (state.toml `[selection]`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SelectionState {
    pub last_pattern: Option<String>,
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
    /// No fades on hover and popups (`[layout] reduce-motion`).
    pub reduce_motion: bool,
    /// `"drives"`, or a path (with `{home}`-style tokens) to open new tabs in.
    pub start_folder: String,
    /// Pinned folders as written (tokenized, `/` separators), in the file's order (the sidebar shows
    /// them grouped: `pins::normalize`).
    pub pinned: Vec<PinEntry>,
    pub shortcuts: Shortcuts,
    pub view: ViewDefaults,
    pub folder_sizes: FolderSizeMode,
    pub files: FilesSettings,
    /// Most frames drawn per second (`MAX_FPS_RANGE`); 0 = as many as the display shows.
    pub max_fps: u32,
    /// Saved rename rule sets.
    pub rename_presets: Vec<RenamePreset>,
    pub keyboard: KeyboardSettings,
    pub history: HistorySettings,
    pub session: SessionSettings,
    pub system: SystemSettings,
    /// `[sidebar] cloud`: the CLOUD section (spec 13.1); roots are found either way.
    pub sidebar_cloud: bool,
    /// `[sidebar] tree-follow`: the sidebar tree opens down to the folder shown (spec 10 §5.3).
    pub sidebar_tree_follow: bool,
    pub terminal: TerminalSettings,
    pub search: SearchSettings,
    /// Tab sets (`[[tab-sets]]`); invalid ones are left out.
    pub tab_sets: Vec<TabSet>,
    /// Saved filters; invalid ones are left out.
    pub filters: Vec<SavedFilter>,
    /// Saved searches (`[[searches]]`); invalid ones are left out, at most `SEARCHES_MAX`.
    pub searches: Vec<SavedSearch>,
    pub archives: ArchivesSettings,
    pub tools: ToolsSettings,
    pub convert: ConvertSettings,
    /// User commands (`[[commands]]`); invalid ones are left out.
    pub commands: Vec<CommandSpec>,
    /// View rules (`[[view-rules]]`, spec 10 §8); invalid ones are left out.
    pub view_rules: Vec<gezik_core::view_rules::RuleSpec>,
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
            reduce_motion: false,
            start_folder: "{home}".to_owned(),
            pinned: Vec::new(),
            shortcuts: Shortcuts::default(),
            view: ViewDefaults::default(),
            folder_sizes: FolderSizeMode::Off,
            files: FilesSettings::default(),
            max_fps: 120,
            rename_presets: Vec::new(),
            keyboard: KeyboardSettings::default(),
            history: HistorySettings::default(),
            session: SessionSettings::default(),
            system: SystemSettings::default(),
            sidebar_cloud: true,
            sidebar_tree_follow: false,
            terminal: TerminalSettings::default(),
            search: SearchSettings::default(),
            tab_sets: Vec::new(),
            filters: Vec::new(),
            searches: Vec::new(),
            archives: ArchivesSettings::default(),
            tools: ToolsSettings::default(),
            convert: ConvertSettings::default(),
            commands: Vec::new(),
            view_rules: Vec::new(),
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
            if let Some(value) = layout.get("reduce-motion") {
                match value.as_bool() {
                    Some(on) => settings.reduce_motion = on,
                    None => warnings
                        .push(Warning::new(file, format!("layout.reduce-motion: expected true or false, got {value}"))),
                }
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
                        match parse_pin(item) {
                            Ok(pin) if find_pin(&settings.pinned, &pin.path).is_some() => warnings.push(Warning::new(
                                file,
                                format!("pinned: \"{}\" is pinned twice; the second is left out", pin.path),
                            )),
                            Ok(pin) => settings.pinned.push(pin),
                            Err(err) => warnings.push(Warning::new(file, format!("pinned: {err}"))),
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
                Some(view) => {
                    settings.view = parse_view(view, file, warnings);
                    let options = "\"off\", \"local\" or \"all\"";
                    if let Some(mode) =
                        view_choice(view, "folder-sizes", options, FolderSizeMode::parse, file, warnings)
                    {
                        settings.folder_sizes = mode;
                    }
                }
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
        match table.get("keyboard") {
            None => {}
            Some(value) => match value.as_table() {
                Some(keyboard) => settings.keyboard = parse_keyboard(keyboard, file, warnings),
                None => warnings.push(Warning::new(file, format!("keyboard: expected a table, got {value}"))),
            },
        }
        match table.get("history") {
            None => {}
            Some(value) => match value.as_table() {
                Some(history) => settings.history = parse_history(history, file, warnings),
                None => warnings.push(Warning::new(file, format!("history: expected a table, got {value}"))),
            },
        }
        match table.get("search") {
            None => {}
            Some(value) => match value.as_table() {
                Some(search) => settings.search = parse_search(search, file, warnings),
                None => warnings.push(Warning::new(file, format!("search: expected a table, got {value}"))),
            },
        }
        match table.get("session") {
            None => {}
            Some(value) => match value.as_table() {
                Some(session) => settings.session = parse_session(session, file, warnings),
                None => warnings.push(Warning::new(file, format!("session: expected a table, got {value}"))),
            },
        }
        match table.get("system") {
            None => {}
            Some(value) => match value.as_table() {
                Some(system) => settings.system = parse_system(system, file, warnings),
                None => warnings.push(Warning::new(file, format!("system: expected a table, got {value}"))),
            },
        }
        match table.get("sidebar") {
            None => {}
            Some(value) => match value.as_table() {
                Some(sidebar) => {
                    if let Some(value) = sidebar.get("cloud") {
                        match value.as_bool() {
                            Some(on) => settings.sidebar_cloud = on,
                            None => warnings.push(Warning::new(
                                file,
                                format!("sidebar.cloud: expected true or false, got {value}"),
                            )),
                        }
                    }
                    if let Some(value) = sidebar.get("tree-follow") {
                        match value.as_bool() {
                            Some(on) => settings.sidebar_tree_follow = on,
                            None => warnings.push(Warning::new(
                                file,
                                format!("sidebar.tree-follow: expected true or false, got {value}"),
                            )),
                        }
                    }
                }
                None => warnings.push(Warning::new(file, format!("sidebar: expected a table, got {value}"))),
            },
        }
        match table.get("terminal") {
            None => {}
            Some(value) => match value.as_table() {
                Some(terminal) => settings.terminal = parse_terminal(terminal, file, warnings),
                None => warnings.push(Warning::new(file, format!("terminal: expected a table, got {value}"))),
            },
        }
        if let Some(value) = table.get("filters") {
            match value.as_array() {
                None => warnings.push(Warning::new(file, format!("filters: expected [[filters]] tables, got {value}"))),
                Some(items) => {
                    for (i, item) in items.iter().enumerate() {
                        match parse_filter(item) {
                            // Names are told apart ignoring case, as the ▾ menu and Save as… do.
                            Ok(filter) if settings.filters.iter().any(|f| same_filter_name(&f.name, &filter.name)) => {
                                warnings.push(Warning::new(
                                    file,
                                    format!(
                                        "filters[{}]: \"{}\" is already used; this one is left out",
                                        i + 1,
                                        filter.name
                                    ),
                                ));
                            }
                            Ok(filter) => settings.filters.push(filter),
                            Err(err) => warnings.push(Warning::new(file, format!("filters[{}]: {err}", i + 1))),
                        }
                    }
                }
            }
        }
        if let Some(value) = table.get("searches") {
            match value.as_array() {
                None => {
                    warnings.push(Warning::new(file, format!("searches: expected [[searches]] tables, got {value}")))
                }
                Some(items) => {
                    for (i, item) in items.iter().enumerate() {
                        match parse_saved_search(item) {
                            Ok(_) if settings.searches.len() == SEARCHES_MAX => {
                                warnings.push(Warning::new(
                                    file,
                                    format!(
                                        "searches[{}]: at most {SEARCHES_MAX} saved searches; this one is left out",
                                        i + 1
                                    ),
                                ));
                            }
                            // Names are told apart ignoring case, as the filters' are.
                            Ok(search) if settings.searches.iter().any(|s| same_filter_name(&s.name, &search.name)) => {
                                warnings.push(Warning::new(
                                    file,
                                    format!(
                                        "searches[{}]: \"{}\" is already used; this one is left out",
                                        i + 1,
                                        search.name
                                    ),
                                ));
                            }
                            Ok(search) => settings.searches.push(search),
                            Err(err) => warnings.push(Warning::new(file, format!("searches[{}]: {err}", i + 1))),
                        }
                    }
                }
            }
        }
        if let Some(value) = table.get("tab-sets") {
            match value.as_array() {
                None => {
                    warnings.push(Warning::new(file, format!("tab-sets: expected [[tab-sets]] tables, got {value}")))
                }
                Some(items) => {
                    for (i, item) in items.iter().enumerate() {
                        match parse_tab_set(item) {
                            // Names are told apart ignoring case, as the menu and "Save tabs as…" do.
                            Ok(set) if settings.tab_sets.iter().any(|s| same_filter_name(&s.name, &set.name)) => {
                                warnings.push(Warning::new(
                                    file,
                                    format!(
                                        "tab-sets[{}]: \"{}\" is already used; this one is left out",
                                        i + 1,
                                        set.name
                                    ),
                                ));
                            }
                            Ok(set) => settings.tab_sets.push(set),
                            Err(err) => warnings.push(Warning::new(file, format!("tab-sets[{}]: {err}", i + 1))),
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
                            Ok(command) => {
                                bind_command_key(&mut settings, &command, i + 1, file, warnings);
                                settings.commands.push(command);
                            }
                            Err(err) => warnings.push(Warning::new(file, format!("commands[{}]: {err}", i + 1))),
                        }
                    }
                }
            }
        }
        if let Some(value) = table.get("view-rules") {
            settings.view_rules = crate::view_rules::parse_view_rules(value, file, warnings);
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
    // `folder` is the search results' own sort (views.toml keeps it under `<results>`).
    let folder_key = |text: &str| SortKey::parse(text).filter(|key| *key != SortKey::Folder);
    if let Some(key) = view_choice(table, "sort", keys, folder_key, file, warnings) {
        out.view.sort.key = key;
    }
    if let Some(dir) = view_choice(table, "sort-dir", "\"asc\" or \"desc\"", SortDir::parse, file, warnings) {
        out.view.sort.dir = dir;
    }
    let sizes = "\"small\", \"medium\" or \"large\"";
    if let Some(size) = view_choice(table, "grid-size", sizes, GridSize::parse, file, warnings) {
        out.view.grid_size = size;
    }
    let groups = "\"none\", \"type\", \"date\" or \"size\"";
    if let Some(group) = view_choice(table, "group", groups, GroupBy::parse, file, warnings) {
        out.view.group = group;
    }
    if let Some(icons) = view_choice(table, "icons", "\"system\" or \"gezik\"", IconMode::parse, file, warnings) {
        out.icons = icons;
    }
    if let Some(on) = view_bool(table, "thumbnails", file, warnings) {
        out.thumbnails = on;
    }
    let options = &mut out.options;
    if let Some(on) = view_bool(table, "hide-extensions", file, warnings) {
        options.hide_extensions = on;
    }
    if let Some(on) = view_bool(table, "folders-first", file, warnings) {
        options.folders_first = on;
    }
    let dates = "\"relative\", \"short\", \"iso\" or \"system\"";
    if let Some(format) = view_choice(table, "date-format", dates, DateFormat::parse, file, warnings) {
        options.date_format = format;
    }
    let sizes = "\"binary\" or \"decimal\"";
    if let Some(format) = view_choice(table, "size-format", sizes, SizeFormat::parse, file, warnings) {
        options.size_format = format;
    }
    if let Some(on) = view_bool(table, "single-click-open", file, warnings) {
        options.single_click_open = on;
    }
    if let Some(on) = view_bool(table, "show-hidden", file, warnings) {
        options.show_hidden = on;
    }
    if let Some(on) = view_bool(table, "show-system", file, warnings) {
        options.show_system = on;
    }
    out
}

/// `[view].key` as true or false: `None` if missing; a bad value also warns.
fn view_bool(table: &toml::Table, key: &str, file: &str, warnings: &mut Vec<Warning>) -> Option<bool> {
    let value = table.get(key)?;
    let on = value.as_bool();
    if on.is_none() {
        warnings.push(Warning::new(file, format!("view.{key}: expected true or false, got {value}")));
    }
    on
}

/// `[view] folder-sizes` (spec 6.1): where the folders of the folder shown get their size by
/// themselves; `calculate-folder-sizes` works everywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FolderSizeMode {
    Off,
    /// Local fixed and removable drives, not network folders.
    #[default]
    Local,
    All,
}

impl FolderSizeMode {
    pub fn parse(text: &str) -> Option<FolderSizeMode> {
        match text {
            "off" => Some(FolderSizeMode::Off),
            "local" => Some(FolderSizeMode::Local),
            "all" => Some(FolderSizeMode::All),
            _ => None,
        }
    }
}

/// `[search]` (spec 9.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchSettings {
    /// Windows: ask Everything for names when it runs (`"auto"`); `false` (`"off"`): never.
    pub everything: bool,
    /// Folder names a search does not go into.
    pub skip: Vec<String>,
    pub max_results: usize,
    /// Larger files are not read for text.
    pub content_max_size: u64,
}

impl Default for SearchSettings {
    fn default() -> Self {
        SearchSettings {
            everything: true,
            skip: vec![".git".to_owned(), "node_modules".to_owned()],
            max_results: 250_000,
            content_max_size: 64 * 1024 * 1024,
        }
    }
}

/// Allowed `max-results` numbers.
pub const MAX_RESULTS_RANGE: std::ops::RangeInclusive<usize> = 1_000..=2_000_000;

fn parse_search(table: &toml::Table, file: &str, warnings: &mut Vec<Warning>) -> SearchSettings {
    let mut out = SearchSettings::default();
    let mut warn = |text: String| warnings.push(Warning::new(file, text));
    if let Some(value) = table.get("everything") {
        match value.as_str() {
            Some("auto") => out.everything = true,
            Some("off") => out.everything = false,
            _ => warn(format!("search.everything: expected \"auto\" or \"off\", got {value}")),
        }
    }
    if let Some(value) = table.get("skip") {
        let names: Option<Vec<String>> =
            value.as_array().and_then(|items| items.iter().map(|i| i.as_str().map(|s| s.trim().to_owned())).collect());
        match names {
            Some(names) if names.iter().all(|n| !n.is_empty() && !n.contains(['/', '\\'])) => out.skip = names,
            _ => warn(format!("search.skip: expected a list of folder names, got {value}")),
        }
    }
    if let Some(value) = table.get("max-results") {
        match value.as_integer().and_then(|n| usize::try_from(n).ok()).filter(|n| MAX_RESULTS_RANGE.contains(n)) {
            Some(n) => out.max_results = n,
            None => warn(format!("search.max-results: expected a number from 1000 to 2000000, got {value}")),
        }
    }
    if let Some(value) = table.get("content-max-size") {
        match value.as_str().map(parse_size) {
            Some(Ok(bytes)) => out.content_max_size = bytes,
            Some(Err(err)) => warn(format!("search.content-max-size: {err}")),
            None => warn(format!("search.content-max-size: expected a size like \"500 MB\", got {value}")),
        }
    }
    out
}

/// A search as a table: a search tab in state.toml (spec 9.2) and, in 8b, `[[searches]]`. Only
/// what differs from a new search is written. A flat view is `Location::Flat`, never a spec
/// here: `flat` is not written (a flat spec would read back as a plain search).
pub fn search_to_toml(spec: &SearchSpec) -> toml::Table {
    debug_assert!(!spec.flat, "a flat view is saved as Location::Flat");
    let mut table = toml::Table::new();
    let text = |s: &str| toml::Value::String(s.to_owned());
    let folder = match &spec.scope {
        Scope::Folder(path) => text(&path.to_string_lossy()),
        Scope::AllDrives => text("drives"),
    };
    table.insert("folder".into(), folder);
    if !spec.pattern.is_empty() {
        table.insert("pattern".into(), text(&spec.pattern));
    }
    if !spec.content.is_empty() {
        table.insert("content".into(), text(&spec.content));
    }
    for (key, on) in [
        ("name-regex", spec.name_regex),
        ("content-regex", spec.content_regex),
        ("match-case", spec.match_case),
        ("hidden", spec.hidden == HiddenRule::Include),
        ("skipped", spec.skipped),
    ] {
        if on {
            table.insert(key.into(), toml::Value::Boolean(true));
        }
    }
    if let Some(min) = spec.size.min {
        table.insert("size-min".into(), text(&size_text(min)));
    }
    if let Some(max) = spec.size.max {
        table.insert("size-max".into(), text(&size_text(max)));
    }
    if spec.modified != DateRange::Any {
        table.insert("modified".into(), text(&spec.modified.text()));
    }
    if spec.kind != KindFilter::Any {
        table.insert("type".into(), text(spec.kind.as_str()));
    }
    table
}

/// A search from its table; the first bad key says why. `folder` is a full path or "drives";
/// other keys (a saved search's `name`) are the caller's.
pub fn search_from_toml(table: &toml::Table) -> Result<SearchSpec, String> {
    let text = |key: &str| -> Result<Option<&str>, String> {
        match table.get(key) {
            None => Ok(None),
            Some(value) => value.as_str().map(Some).ok_or_else(|| format!("{key}: expected text, got {value}")),
        }
    };
    let flag = |key: &str| -> Result<bool, String> {
        match table.get(key) {
            None => Ok(false),
            Some(value) => value.as_bool().ok_or_else(|| format!("{key}: expected true or false, got {value}")),
        }
    };
    let scope = match text("folder")? {
        None => return Err("folder is missing".to_owned()),
        Some("drives") => Scope::AllDrives,
        Some(path) => {
            let path = PathBuf::from(path);
            if !path.is_absolute() {
                return Err(format!("folder: \"{}\" is not a full path", path.display()));
            }
            Scope::Folder(path)
        }
    };
    let mut spec = SearchSpec::new(scope);
    spec.pattern = text("pattern")?.unwrap_or_default().to_owned();
    spec.content = text("content")?.unwrap_or_default().to_owned();
    spec.name_regex = flag("name-regex")?;
    spec.content_regex = flag("content-regex")?;
    spec.match_case = flag("match-case")?;
    spec.hidden = if flag("hidden")? { HiddenRule::Include } else { HiddenRule::FollowView };
    spec.skipped = flag("skipped")?;
    if let Some(size) = text("size-min")? {
        spec.size.min = Some(parse_size(size).map_err(|err| format!("size-min: {err}"))?);
    }
    if let Some(size) = text("size-max")? {
        spec.size.max = Some(parse_size(size).map_err(|err| format!("size-max: {err}"))?);
    }
    if let Some(modified) = text("modified")? {
        spec.modified = DateRange::parse(modified).map_err(|err| format!("modified: {err}"))?;
    }
    if let Some(kind) = text("type")? {
        spec.kind = KindFilter::parse(kind).ok_or_else(|| {
            format!(
                "type: expected files, folders, pictures, videos, audio, documents, archives or code, got \"{kind}\""
            )
        })?;
    }
    Ok(spec)
}

/// `[[columns]]`-shaped items, made complete by `normalize`.
fn columns_of(
    value: Option<&toml::Value>,
    normalize: fn(&[ColumnState]) -> Vec<ColumnState>,
) -> Option<Vec<ColumnState>> {
    let items = value?.as_array()?;
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
    Some(normalize(&saved))
}

fn columns_toml(columns: &[ColumnState]) -> toml::Value {
    toml::Value::Array(
        columns
            .iter()
            .map(|c| {
                let mut column = toml::Table::new();
                column.insert("key".into(), toml::Value::String(c.key.as_str().into()));
                column.insert("visible".into(), toml::Value::Boolean(c.visible));
                column.insert("width".into(), toml::Value::Integer(c.width.into()));
                toml::Value::Table(column)
            })
            .collect(),
    )
}

/// One `[view]` option as the View menu (or toggle-hidden) sets it, written into settings.toml
/// with `SettingsChange::ViewOption` (spec 7.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewOption {
    HideExtensions(bool),
    FoldersFirst(bool),
    DateFormat(DateFormat),
    SizeFormat(SizeFormat),
    SingleClickOpen(bool),
    ShowHidden(bool),
    ShowSystem(bool),
}

impl ViewOption {
    /// Its key in `[view]`.
    pub fn key(self) -> &'static str {
        match self {
            ViewOption::HideExtensions(_) => "hide-extensions",
            ViewOption::FoldersFirst(_) => "folders-first",
            ViewOption::DateFormat(_) => "date-format",
            ViewOption::SizeFormat(_) => "size-format",
            ViewOption::SingleClickOpen(_) => "single-click-open",
            ViewOption::ShowHidden(_) => "show-hidden",
            ViewOption::ShowSystem(_) => "show-system",
        }
    }

    pub fn apply(self, options: &mut ViewOptions) {
        match self {
            ViewOption::HideExtensions(on) => options.hide_extensions = on,
            ViewOption::FoldersFirst(on) => options.folders_first = on,
            ViewOption::DateFormat(format) => options.date_format = format,
            ViewOption::SizeFormat(format) => options.size_format = format,
            ViewOption::SingleClickOpen(on) => options.single_click_open = on,
            ViewOption::ShowHidden(on) => options.show_hidden = on,
            ViewOption::ShowSystem(on) => options.show_system = on,
        }
    }
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

fn parse_keyboard(table: &toml::Table, file: &str, warnings: &mut Vec<Warning>) -> KeyboardSettings {
    let mut out = KeyboardSettings::default();
    if let Some(value) = table.get("typing") {
        match value.as_str() {
            Some("jump") => out.typing = Typing::Jump,
            Some("filter") => out.typing = Typing::Filter,
            _ => warnings
                .push(Warning::new(file, format!("keyboard.typing: expected \"jump\" or \"filter\", got {value}"))),
        }
    }
    out
}

fn parse_history(table: &toml::Table, file: &str, warnings: &mut Vec<Warning>) -> HistorySettings {
    let mut out = HistorySettings::default();
    if let Some(value) = table.get("remember") {
        match value.as_bool() {
            Some(on) => out.remember = on,
            None => warnings.push(Warning::new(file, format!("history.remember: expected true or false, got {value}"))),
        }
    }
    out
}

fn parse_session(table: &toml::Table, file: &str, warnings: &mut Vec<Warning>) -> SessionSettings {
    let mut out = SessionSettings::default();
    if let Some(value) = table.get("restore") {
        match value.as_bool() {
            Some(on) => out.restore = on,
            None => warnings.push(Warning::new(file, format!("session.restore: expected true or false, got {value}"))),
        }
    }
    out
}

fn parse_system(table: &toml::Table, file: &str, warnings: &mut Vec<Warning>) -> SystemSettings {
    let mut out = SystemSettings::default();
    if let Some(value) = table.get("single-instance") {
        match value.as_bool() {
            Some(on) => out.single_instance = on,
            None => warnings
                .push(Warning::new(file, format!("system.single-instance: expected true or false, got {value}"))),
        }
    }
    if let Some(value) = table.get("quick-look") {
        match value.as_str() {
            Some("system") => out.quick_look = QuickLookMode::System,
            Some("gezik") => out.quick_look = QuickLookMode::Gezik,
            _ => warnings
                .push(Warning::new(file, format!("system.quick-look: expected \"system\" or \"gezik\", got {value}"))),
        }
    }
    out
}

/// `[terminal] command`: a list of text with `{dir}` the only placeholder; an empty list is
/// none (Gezik finds a terminal).
fn parse_terminal(table: &toml::Table, file: &str, warnings: &mut Vec<Warning>) -> TerminalSettings {
    let Some(value) = table.get("command") else { return TerminalSettings::default() };
    let Ok(command) = string_list(value, "command") else {
        warnings.push(Warning::new(file, format!("terminal.command: expected a list of text, got {value}")));
        return TerminalSettings::default();
    };
    if command.is_empty() {
        return TerminalSettings::default();
    }
    match gezik_core::batch::convert::check_dir_command(&command) {
        Ok(()) => TerminalSettings { command: Some(command) },
        Err(err) => {
            warnings.push(Warning::new(file, format!("terminal.command: {err}; Gezik finds a terminal")));
            TerminalSettings::default()
        }
    }
}

/// One `[[tab-sets]]` entry; the error is why it is left out (a path with `..` leaves out the
/// whole set, which Gezik then keeps in the file as it is).
pub(crate) fn parse_tab_set(value: &toml::Value) -> Result<TabSet, String> {
    let table = value.as_table().ok_or_else(|| format!("expected a table, got {value}"))?;
    let name =
        table.get("name").and_then(|v| v.as_str()).map(str::trim).filter(|n| !n.is_empty()).ok_or("name is missing")?;
    let tabs = match table.get("tabs") {
        None => return Err("tabs is missing".to_owned()),
        Some(value) => string_list(value, "tabs")?,
    };
    if tabs.is_empty() {
        return Err("tabs is missing".to_owned());
    }
    if tabs.iter().any(|tab| tab.trim().is_empty()) {
        return Err("tabs has an empty path".to_owned());
    }
    if let Some(bad) = tabs.iter().find(|tab| crate::paths::has_parent_segment(tab)) {
        return Err(format!("\"{bad}\" must not contain \"..\""));
    }
    Ok(TabSet { name: name.to_owned(), tabs })
}

pub fn tab_set_to_toml(set: &TabSet) -> toml::Table {
    let mut table = toml::Table::new();
    table.insert("name".into(), toml::Value::String(set.name.clone()));
    table.insert("tabs".into(), toml::Value::Array(set.tabs.iter().cloned().map(toml::Value::String).collect()));
    table
}

/// Whether two saved filters' names are one, ignoring case.
fn same_filter_name(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

pub(crate) fn parse_filter(value: &toml::Value) -> Result<SavedFilter, String> {
    let table = value.as_table().ok_or_else(|| format!("expected a table, got {value}"))?;
    let name =
        table.get("name").and_then(|v| v.as_str()).map(str::trim).filter(|n| !n.is_empty()).ok_or("name is missing")?;
    let pattern =
        table.get("pattern").and_then(|v| v.as_str()).filter(|p| !p.trim().is_empty()).ok_or("pattern is missing")?;
    gezik_core::pattern::Pattern::compile(pattern).map_err(|err| format!("pattern: {err}"))?;
    Ok(SavedFilter { name: name.to_owned(), pattern: pattern.to_owned() })
}

pub fn filter_to_toml(filter: &SavedFilter) -> toml::Table {
    let mut table = toml::Table::new();
    table.insert("name".into(), toml::Value::String(filter.name.clone()));
    table.insert("pattern".into(), toml::Value::String(filter.pattern.clone()));
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
    const KEYS: [&str; 9] = ["name", "run", "output", "types", "folders", "parallel", "shortcut", "menu", "ask"];
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
    let shortcut = match table.get("shortcut") {
        None => None,
        Some(v) => Some(v.as_str().ok_or_else(|| format!("shortcut must be text, got {v}"))?.to_owned()),
    };
    let menu = match table.get("menu") {
        None => None,
        Some(v) => {
            let menu = v.as_str().ok_or_else(|| format!("menu must be text, got {v}"))?.trim();
            (!menu.is_empty()).then(|| menu.to_owned())
        }
    };
    let ask = match table.get("ask") {
        None => false,
        Some(v) => v.as_bool().ok_or_else(|| format!("ask must be true or false, got {v}"))?,
    };
    let spec = CommandSpec { name: name.to_owned(), run, output, types, folders, parallel, shortcut, menu, ask };
    check_command(&spec)?;
    Ok(spec)
}

/// Gives `spec` (the `n`th `[[commands]]` entry, about to join `settings.commands`) its
/// shortcut, or warns why it has none (spec 7: the command stays, its key is left out).
fn bind_command_key(settings: &mut Settings, spec: &CommandSpec, n: usize, file: &str, warnings: &mut Vec<Warning>) {
    let Some(text) = &spec.shortcut else { return };
    let index = settings.commands.len();
    let chord = match parse_chord(text, Platform::current()) {
        Ok(Some(chord)) => chord,
        Ok(None) => return,
        Err(err) => {
            warnings.push(Warning::new(file, format!("commands[{n}]: shortcut: {err}; the command has no key")));
            return;
        }
    };
    if !chord.leaves_typing_alone() {
        warnings.push(Warning::new(
            file,
            format!("commands[{n}]: shortcut \"{text}\" needs Ctrl, Alt or Cmd (or an F key); the command has no key"),
        ));
        return;
    }
    let taken = match fixed_owner(&chord, Platform::current()) {
        Some(owner) => Err(owner.to_owned()),
        None => settings.shortcuts.bind_command(index, chord).map_err(|owner| match owner {
            KeyOwner::Action(action) => action.name().to_owned(),
            KeyOwner::Command(other) => format!("\"{}\"", settings.commands[other].name),
        }),
    };
    if let Err(who) = taken {
        warnings.push(Warning::new(
            file,
            format!("commands[{n}]: shortcut \"{text}\" is already used by {who}; the command has no key"),
        ));
    }
}

fn parse_visit(value: &toml::Value) -> Option<Visit> {
    let table = value.as_table()?;
    let path = table.get("path")?.as_str().filter(|path| !path.is_empty())?;
    let count = match table.get("count") {
        None => 1,
        Some(count) => u32::try_from(count.as_integer()?).ok().filter(|n| *n > 0)?,
    };
    let last = table.get("last").and_then(|v| v.as_integer()).and_then(|n| u64::try_from(n).ok()).unwrap_or(0);
    Some(Visit { path: std::path::PathBuf::from(path), count, last })
}

/// Size in logical pixels, position in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowState {
    pub width: u32,
    pub height: u32,
    pub x: Option<i32>,
    pub y: Option<i32>,
    /// Opens maximized; the size and position above are then its normal (restored) rect.
    pub maximized: bool,
}

/// Connect to Server's addresses kept in state.toml `[servers] recent` (spec 9 §7.4); never a password.
pub const SERVERS_MAX: usize = 10;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct State {
    pub window: Option<WindowState>,
    /// Sidebar width in logical pixels (120–480).
    pub sidebar_width: Option<u32>,
    /// The list's columns; `None` until first saved (the defaults are used then).
    pub columns: Option<Vec<ColumnState>>,
    /// The search results' columns (apart from the folders'); `None` until first saved.
    pub result_columns: Option<Vec<ColumnState>>,
    pub preview_open: bool,
    /// Preview pane width in logical pixels (200–600).
    pub preview_width: Option<u32>,
    /// The operations panel is folded into the status bar.
    pub operations_collapsed: bool,
    /// The rename layer's last rules.
    pub batch_rename: Option<BatchRenameState>,
    pub archive: ArchiveState,
    pub convert: ConvertState,
    pub selection: SelectionState,
    /// The folders visited (`[history] folders`), for the address bar's lists.
    pub history: Vec<Visit>,
    /// The tabs of last time (`[session]`), opened at start if `[session] restore`.
    pub session: Session,
    /// The palette's items used last, newest first (spec 7.3).
    pub palette_recent: Vec<String>,
    /// Connect to Server's addresses, newest first (`[servers] recent`).
    pub servers_recent: Vec<String>,
}

/// state.toml's `[session]` (spec 5.1): the tabs in order and the one in front. An entry
/// without a path (or `drives = true`), or with a relative one, is left out, and `active`
/// counts the kept ones (the first if its entry was left out).
fn session_state(value: Option<&toml::Value>) -> Session {
    let Some(table) = value.and_then(|v| v.as_table()) else { return Session::default() };
    let wanted = table.get("active").and_then(|v| v.as_integer()).and_then(|n| usize::try_from(n).ok()).unwrap_or(0);
    let mut session = Session::default();
    let mut active = None;
    for (i, item) in table.get("tabs").and_then(|v| v.as_array()).into_iter().flatten().enumerate() {
        let Some(tab) = item.as_table() else { continue };
        let location = if tab.get("drives").and_then(|v| v.as_bool()) == Some(true) {
            Location::Drives
        } else if tab.get("trash").and_then(|v| v.as_bool()) == Some(true) {
            Location::Trash
        } else if let Some(search) = tab.get("search").and_then(|v| v.as_table()) {
            // One written wrong is left out; an older Gezik leaves these tabs out (spec 9.2).
            match search_from_toml(search) {
                Ok(spec) => Location::Search(Box::new(spec)),
                Err(_) => continue,
            }
        } else if let Some(flat) = tab.get("flat").and_then(|v| v.as_str()).map(PathBuf::from) {
            if !flat.is_absolute() {
                continue;
            }
            Location::Flat(flat)
        } else {
            match tab.get("path").and_then(|v| v.as_str()).map(PathBuf::from) {
                Some(path) if path.is_absolute() => Location::Path(path),
                _ => continue,
            }
        };
        if i == wanted {
            active = Some(session.tabs.len());
        }
        let locked = tab.get("locked").and_then(|v| v.as_bool()).unwrap_or(false);
        session.tabs.push(SessionTab { location, locked });
    }
    session.active = active.unwrap_or(0);
    session
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
                maximized: window.get("maximized").and_then(|v| v.as_bool()).unwrap_or(false),
            })
        });
        let sidebar_width = table
            .get("sidebar")
            .and_then(|v| v.as_table())
            .and_then(|s| s.get("width"))
            .and_then(|v| v.as_integer())
            .and_then(|w| u32::try_from(w).ok())
            .filter(|w| (120..=480).contains(w));
        let columns = columns_of(table.get("columns"), normalize_columns);
        let result_columns = columns_of(table.get("result-columns"), gezik_core::view::normalize_result_columns);
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
                pdf: text("pdf"),
                last_output: text("last-output").unwrap_or_default(),
                last_folder: text("last-folder"),
            }
        });
        let selection = SelectionState {
            last_pattern: table
                .get("selection")
                .and_then(|v| v.as_table())
                .and_then(|t| t.get("last-pattern"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_owned),
        };
        let history = table
            .get("history")
            .and_then(|v| v.as_table())
            .and_then(|t| t.get("folders"))
            .and_then(|v| v.as_array())
            .map(|items| items.iter().filter_map(parse_visit).collect())
            .unwrap_or_default();
        let recent = |section: &str, max: usize| -> Vec<String> {
            table
                .get(section)
                .and_then(|v| v.as_table())
                .and_then(|p| p.get("recent"))
                .and_then(|v| v.as_array())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|v| v.as_str())
                        .filter(|s| !s.is_empty())
                        .take(max)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default()
        };
        let palette_recent = recent("palette", gezik_core::palette::RECENT_MAX);
        let servers_recent = recent("servers", SERVERS_MAX);
        State {
            window,
            sidebar_width,
            columns,
            result_columns,
            preview_open,
            preview_width,
            operations_collapsed,
            batch_rename,
            archive,
            convert,
            selection,
            history,
            session: session_state(table.get("session")),
            palette_recent,
            servers_recent,
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
            if w.maximized {
                window.insert("maximized".into(), toml::Value::Boolean(true));
            }
            root.insert("window".into(), toml::Value::Table(window));
        }
        if let Some(width) = self.sidebar_width {
            let mut sidebar = toml::Table::new();
            sidebar.insert("width".into(), toml::Value::Integer(width.into()));
            root.insert("sidebar".into(), toml::Value::Table(sidebar));
        }
        if let Some(columns) = &self.columns {
            root.insert("columns".into(), columns_toml(columns));
        }
        if let Some(columns) = &self.result_columns {
            root.insert("result-columns".into(), columns_toml(columns));
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
                ("pdf", &convert.pdf),
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
        if let Some(pattern) = &self.selection.last_pattern {
            let mut table = toml::Table::new();
            table.insert("last-pattern".into(), toml::Value::String(pattern.clone()));
            root.insert("selection".into(), toml::Value::Table(table));
        }
        if !self.history.is_empty() {
            let folders = self
                .history
                .iter()
                .map(|visit| {
                    let mut table = toml::Table::new();
                    table.insert("path".into(), toml::Value::String(visit.path.to_string_lossy().into_owned()));
                    table.insert("count".into(), toml::Value::Integer(visit.count.into()));
                    table.insert("last".into(), toml::Value::Integer(i64::try_from(visit.last).unwrap_or(i64::MAX)));
                    toml::Value::Table(table)
                })
                .collect();
            let mut history = toml::Table::new();
            history.insert("folders".into(), toml::Value::Array(folders));
            root.insert("history".into(), toml::Value::Table(history));
        }
        if !self.palette_recent.is_empty() {
            let recent = self
                .palette_recent
                .iter()
                .take(gezik_core::palette::RECENT_MAX)
                .cloned()
                .map(toml::Value::String)
                .collect();
            let mut palette = toml::Table::new();
            palette.insert("recent".into(), toml::Value::Array(recent));
            root.insert("palette".into(), toml::Value::Table(palette));
        }
        if !self.servers_recent.is_empty() {
            let recent = self.servers_recent.iter().take(SERVERS_MAX).cloned().map(toml::Value::String).collect();
            let mut servers = toml::Table::new();
            servers.insert("recent".into(), toml::Value::Array(recent));
            root.insert("servers".into(), toml::Value::Table(servers));
        }
        if !self.session.is_empty() {
            let tabs = self
                .session
                .tabs
                .iter()
                .map(|tab| {
                    let mut table = toml::Table::new();
                    match &tab.location {
                        Location::Path(path) => {
                            table.insert("path".into(), toml::Value::String(path.to_string_lossy().into_owned()));
                        }
                        Location::Drives => {
                            table.insert("drives".into(), toml::Value::Boolean(true));
                        }
                        Location::Trash => {
                            table.insert("trash".into(), toml::Value::Boolean(true));
                        }
                        Location::Search(spec) => {
                            table.insert("search".into(), toml::Value::Table(search_to_toml(spec)));
                        }
                        Location::Flat(path) => {
                            table.insert("flat".into(), toml::Value::String(path.to_string_lossy().into_owned()));
                        }
                    }
                    if tab.locked {
                        table.insert("locked".into(), toml::Value::Boolean(true));
                    }
                    toml::Value::Table(table)
                })
                .collect();
            let mut session = toml::Table::new();
            session.insert("active".into(), toml::Value::Integer(i64::try_from(self.session.active).unwrap_or(0)));
            session.insert("tabs".into(), toml::Value::Array(tabs));
            root.insert("session".into(), toml::Value::Table(session));
        }
        root.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pins::{self, PinEntry};

    fn parse(text: &str) -> (Settings, Vec<Warning>) {
        let mut warnings = Vec::new();
        let settings = Settings::parse("settings.toml", text, &mut warnings);
        (settings, warnings)
    }

    #[test]
    fn a_folder_cannot_be_sorted_by_folder() {
        let (settings, warnings) = parse("[view]\nsort = \"folder\"\n");
        assert_eq!(settings.view.view.sort.key, SortKey::Name);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].to_string().contains("view.sort"), "{}", warnings[0]);
    }

    #[test]
    fn the_cloud_section_is_on_unless_turned_off() {
        assert!(Settings::default().sidebar_cloud);
        let (settings, warnings) = parse("[sidebar]\ncloud = false\n");
        assert!(!settings.sidebar_cloud);
        assert!(warnings.is_empty());
        let (settings, warnings) = parse("[sidebar]\ncloud = \"no\"\n");
        assert!(settings.sidebar_cloud, "a bad value keeps the default");
        assert_eq!(warnings[0].message, "sidebar.cloud: expected true or false, got \"no\"");
        let (_, warnings) = parse("sidebar = 3\n");
        assert_eq!(warnings[0].message, "sidebar: expected a table, got 3");
    }

    #[test]
    fn the_tree_follows_the_folder_only_when_asked() {
        assert!(!Settings::default().sidebar_tree_follow, "spec 10 §5.3: off by default");
        let (settings, warnings) = parse("[sidebar]\ntree-follow = true\n");
        assert!(settings.sidebar_tree_follow);
        assert!(warnings.is_empty());
        let (settings, warnings) = parse("[sidebar]\ntree-follow = 1\n");
        assert!(!settings.sidebar_tree_follow, "a bad value keeps the default");
        assert_eq!(warnings[0].message, "sidebar.tree-follow: expected true or false, got 1");
    }

    #[test]
    fn defaults_when_empty() {
        let (settings, warnings) = parse("");
        assert_eq!(settings, Settings::default());
        assert!(warnings.is_empty());
    }

    #[test]
    fn reduce_motion_is_read_and_checked() {
        let (settings, warnings) = parse(
            "[layout]
reduce-motion = true
",
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(settings.reduce_motion);
        let (settings, warnings) = parse(
            "[layout]
reduce-motion = \"yes\"
",
        );
        assert!(!settings.reduce_motion);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.starts_with("layout.reduce-motion:"), "{}", warnings[0].message);
        assert!(!Settings::default().reduce_motion);
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
            window: Some(WindowState { width: 1000, height: 700, x: Some(-50), y: Some(30), maximized: true }),
            sidebar_width: None,
            ..State::default()
        };
        assert_eq!(State::parse(&state.to_toml()), state);
        let no_position = State {
            window: Some(WindowState { width: 800, height: 600, x: None, y: None, maximized: false }),
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
        assert_eq!(pins::paths(&settings.pinned), ["{documents}/Projects", "D:/Work"]);
    }

    #[test]
    fn pinned_skips_duplicates_and_parent_segments() {
        let (settings, warnings) = parse(
            "pinned = [\"/a\", \"/a\", \"{home}/../etc\", 3]
",
        );
        assert_eq!(pins::paths(&settings.pinned), ["/a"]);
        let messages: Vec<_> = warnings.iter().map(|w| w.message.as_str()).collect();
        assert_eq!(messages.len(), 3, "{messages:?}");
        assert!(messages.iter().any(|m| m.contains("pinned twice")));
        assert!(messages.iter().any(|m| m.contains("..")));
        assert!(messages.iter().any(|m| m.contains("expected text")));
    }

    #[test]
    fn pins_of_both_forms_are_read_and_bad_ones_warned() {
        let (settings, warnings) = parse(
            "pinned = [\n  \"{documents}/Projects\",\n  { path = \"D:/Work/gezik\", name = \" Gezik \", group = \"Work\" },\n  \
             { path = \"//nas/foto\", group = \"Media\" },\n  { path = \"/x\", icon = \"star\" },\n  { name = \"no path\" },\n  \
             \"\",\n  \"{home}/../etc\",\n  3,\n  \"{documents}/Projects\",\n  { path = \"/n\", name = 5 },\n]\n",
        );
        assert_eq!(
            settings.pinned,
            [
                PinEntry::plain("{documents}/Projects"),
                PinEntry { path: "D:/Work/gezik".into(), name: Some("Gezik".into()), group: Some("Work".into()) },
                PinEntry { path: "//nas/foto".into(), name: None, group: Some("Media".into()) },
            ]
        );
        let messages: Vec<&str> = warnings.iter().map(|w| w.message.as_str()).collect();
        assert_eq!(messages.len(), 7, "{messages:?}");
        assert!(messages[0].starts_with("pinned: unknown key \"icon\" in "), "{messages:?}");
        assert!(messages[1].starts_with("pinned: path is missing in "), "{messages:?}");
        assert_eq!(messages[2], "pinned: path is missing in \"\"");
        assert_eq!(messages[3], "pinned: \"{home}/../etc\" must not contain \"..\"");
        assert_eq!(messages[4], "pinned: expected text or { path, name, group }, got 3");
        assert_eq!(messages[5], "pinned: \"{documents}/Projects\" is pinned twice; the second is left out");
        assert_eq!(messages[6], "pinned: name must be text, got 5");
    }

    #[cfg(windows)]
    #[test]
    fn a_pin_twice_in_other_case_is_one_on_windows() {
        let (settings, warnings) = parse("pinned = [\"C:/Work\", \"c:/work\"]\n");
        assert_eq!(pins::paths(&settings.pinned), ["C:/Work"]);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
    }

    #[test]
    fn the_template_pinned_example_reads_once_uncommented() {
        let template = include_str!("../templates/settings.toml");
        let line =
            template.lines().find(|l| l.starts_with("# pinned = [")).expect("the template shows a pinned example");
        let (settings, warnings) = parse(&format!("{}\n", &line[2..]));
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(settings.pinned.len(), 2);
        assert_eq!(settings.pinned[1].group.as_deref(), Some("Work"));
    }

    #[test]
    fn pinned_keeps_entries_that_do_not_exist_here() {
        let (settings, _) = parse(
            "pinned = [\"Z:/not/on/this/machine\"]
",
        );
        assert_eq!(pins::paths(&settings.pinned), ["Z:/not/on/this/machine"]);
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
             group = \"date\"\nicons = \"gezik\"\nthumbnails = false\n",
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        let v = settings.view;
        assert_eq!((v.view.mode, v.view.sort.key, v.view.sort.dir), (ViewMode::Grid, SortKey::Size, SortDir::Desc));
        assert_eq!((v.view.grid_size, v.icons, v.thumbnails), (GridSize::Large, IconMode::Gezik, false));
        assert_eq!(v.view.group, gezik_core::group::GroupBy::Date);
    }

    #[test]
    fn bad_view_values_keep_defaults_with_warnings() {
        let (settings, warnings) = parse(
            "[view]\nmode = \"tiles\"\nsort-dir = 1\ngroup = \"tag\"\nthumbnails = \"yes\"\nicons = \"system\"\n",
        );
        assert_eq!(settings.view, ViewDefaults::default());
        let messages: Vec<_> = warnings.iter().map(|w| w.message.as_str()).collect();
        assert_eq!(messages.len(), 4, "{messages:?}");
        assert!(messages[0].starts_with("view.mode:") && messages[0].contains("\"tiles\""));
        assert!(messages[1].starts_with("view.sort-dir:"));
        assert!(
            messages[2].starts_with("view.group: expected \"none\", \"type\", \"date\" or \"size\""),
            "{messages:?}"
        );
        assert!(messages[3].starts_with("view.thumbnails:"));
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
    fn reads_the_system_section() {
        assert_eq!(Settings::default().system.quick_look, QuickLookMode::System);
        let (settings, warnings) = parse(
            "[system]
quick-look = \"gezik\"
",
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(settings.system.quick_look, QuickLookMode::Gezik);
        let (settings, warnings) = parse(
            "[system]
quick-look = \"finder\"
",
        );
        assert_eq!(settings.system.quick_look, QuickLookMode::System, "a bad value keeps the default");
        assert_eq!(warnings[0].message, "system.quick-look: expected \"system\" or \"gezik\", got \"finder\"");
        let (_, warnings) = parse(
            "system = 3
",
        );
        assert_eq!(warnings[0].message, "system: expected a table, got 3");
    }

    #[test]
    fn the_template_reads_with_the_defaults() {
        let (settings, warnings) = parse(include_str!("../templates/settings.toml"));
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(settings.archives, ArchivesSettings::default());
        assert_eq!(settings.system, SystemSettings::default());
        assert_eq!(settings.tools, ToolsSettings::default());
        assert_eq!(settings.keyboard, KeyboardSettings::default());
        assert_eq!(settings.history, HistorySettings::default());
        assert!(settings.filters.is_empty());
        assert_eq!(settings.session, SessionSettings::default());
        assert_eq!(settings.terminal, TerminalSettings::default());
        assert!(settings.tab_sets.is_empty());
        assert_eq!(settings.view, ViewDefaults::default());
        assert!(settings.pinned.is_empty());
    }

    #[test]
    fn view_options_are_read() {
        use gezik_core::view::{DateFormat, SizeFormat, ViewOptions};
        let (settings, warnings) = parse(
            "[view]
hide-extensions = true
folders-first = false
date-format = \"relative\"
size-format = \"decimal\"
single-click-open = true
show-hidden = false
show-system = true
",
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(
            settings.view.options,
            ViewOptions {
                hide_extensions: true,
                folders_first: false,
                date_format: DateFormat::Relative,
                size_format: SizeFormat::Decimal,
                single_click_open: true,
                show_hidden: false,
                show_system: true,
            }
        );
        assert_eq!(ViewDefaults::default().options, ViewOptions::default());
    }

    #[test]
    fn bad_view_options_keep_defaults_with_warnings() {
        let (settings, warnings) = parse(
            "[view]
date-format = \"weekday\"
size-format = 1024
hide-extensions = \"yes\"
",
        );
        assert_eq!(settings.view.options, gezik_core::view::ViewOptions::default());
        let messages: Vec<&str> = warnings.iter().map(|w| w.message.as_str()).collect();
        assert_eq!(
            messages,
            [
                "view.hide-extensions: expected true or false, got \"yes\"",
                "view.date-format: expected \"relative\", \"short\", \"iso\" or \"system\", got \"weekday\"",
                "view.size-format: expected \"binary\" or \"decimal\", got 1024",
            ]
        );
    }

    #[test]
    fn a_view_option_sets_its_own_field() {
        use gezik_core::view::{DateFormat, SizeFormat, ViewOptions};
        let mut options = ViewOptions::default();
        for option in [
            ViewOption::HideExtensions(true),
            ViewOption::FoldersFirst(false),
            ViewOption::DateFormat(DateFormat::Iso),
            ViewOption::SizeFormat(SizeFormat::Decimal),
            ViewOption::SingleClickOpen(true),
            ViewOption::ShowHidden(false),
            ViewOption::ShowSystem(true),
        ] {
            option.apply(&mut options);
        }
        assert!(options.hide_extensions && !options.folders_first && options.single_click_open);
        assert!(!options.show_hidden && options.show_system);
        assert_eq!((options.date_format, options.size_format), (DateFormat::Iso, SizeFormat::Decimal));
        assert_eq!(ViewOption::SingleClickOpen(true).key(), "single-click-open");
        assert_eq!(ViewOption::DateFormat(DateFormat::Iso).key(), "date-format");
    }

    #[test]
    fn keyboard_typing_is_read() {
        let (settings, warnings) = parse(
            "[keyboard]
typing = \"filter\"
",
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(settings.keyboard.typing, Typing::Filter);
        let (settings, warnings) = parse(
            "[keyboard]
typing = \"search\"
",
        );
        assert_eq!(settings.keyboard.typing, Typing::Jump);
        assert!(warnings[0].message.starts_with("keyboard.typing: expected \"jump\" or \"filter\""), "{warnings:?}");
    }

    #[test]
    fn saved_filters_are_read_and_bad_ones_warned() {
        let text = "[[filters]]
name = \"Resimler\"
pattern = \"*.jpg;*.png\"
                    [[filters]]
name = \"\"
pattern = \"x\"
                    [[filters]]
name = \"Boş\"
pattern = \" \"
                    [[filters]]
name = \"Kötü\"
pattern = \"a;!\"
                    [[filters]]
name = \"resimler\"
pattern = \"*.gif\"
";
        let (settings, warnings) = parse(text);
        assert_eq!(settings.filters, [SavedFilter { name: "Resimler".into(), pattern: "*.jpg;*.png".into() }]);
        let messages: Vec<&str> = warnings.iter().map(|w| w.message.as_str()).collect();
        assert_eq!(
            messages,
            [
                "filters[2]: name is missing",
                "filters[3]: pattern is missing",
                "filters[4]: pattern: Type a name after \"!\"",
                "filters[5]: \"resimler\" is already used; this one is left out",
            ]
        );
    }

    #[test]
    fn selection_state_round_trips() {
        let state = State { selection: SelectionState { last_pattern: Some("*.jpg;!a*".into()) }, ..State::default() };
        assert_eq!(State::parse(&state.to_toml()), state);
        assert!(!State::default().to_toml().contains("selection"));
        assert_eq!(
            State::parse(
                "[selection]
last-pattern = \"\"
"
            )
            .selection,
            SelectionState::default()
        );
    }

    #[test]
    fn the_template_filter_example_reads_once_uncommented() {
        let template = include_str!("../templates/settings.toml");
        let start = template.find("# [[filters]]").expect("the template has a filter example");
        let example: String = template[start..]
            .lines()
            .take_while(|line| line.starts_with('#'))
            .map(|line| {
                format!(
                    "{}
",
                    line.strip_prefix("# ").unwrap_or(line)
                )
            })
            .collect();
        let (settings, warnings) = parse(&example);
        assert!(
            warnings.is_empty(),
            "{warnings:?}
{example}"
        );
        assert_eq!(settings.filters.len(), 1);
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
                shortcut: None,
                menu: None,
                ask: false,
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
    fn the_template_shortcut_examples_are_the_defaults_off_macos() {
        let template = include_str!("../templates/settings.toml");
        let start = template.find("[shortcuts]").expect("the template has a [shortcuts] table");
        let examples: String = template[start..]
            .lines()
            .skip(1)
            .take_while(|line| !line.trim().is_empty())
            .filter_map(|line| line.strip_prefix("# "))
            .filter(|line| line.contains(" = "))
            .map(|line| format!("{line}\n"))
            .collect();
        let table: toml::Table = examples.parse().unwrap_or_else(|e| panic!("{e}\n{examples}"));
        assert!(table.contains_key("select-pattern") && table.contains_key("toggle-tab-lock"), "{examples}");
        let mut warnings = Vec::new();
        let shortcuts = Shortcuts::from_table(Some(&table), Platform::Other, "settings.toml", &mut warnings);
        assert!(warnings.is_empty(), "{warnings:?}");
        let defaults = Shortcuts::defaults(Platform::Other);
        for action in crate::shortcuts::Action::ALL {
            if action.tab_number().is_some_and(|n| n > 1) || action.pin_number().is_some_and(|n| n > 1) {
                continue; // "tab-2 … tab-8 and pin-2 … pin-9 likewise"
            }
            assert_eq!(shortcuts.chord_for(action), defaults.chord_for(action), "{}", action.name());
        }
    }

    #[test]
    fn the_template_documents_escaped_braces() {
        let template = include_str!("../templates/settings.toml");
        assert!(template.contains("\"{{\" and \"}}\""));
        assert!(template.contains("--outdir") && template.contains("ebook-convert") && template.contains("magick"));
    }

    #[test]
    fn the_template_warns_against_dir_inside_shell_code() {
        let template = include_str!("../templates/settings.toml");
        let terminal = &template[template.find("[terminal]").expect("the template has a [terminal] table")..];
        let block = &terminal[..terminal.find("\n\n").unwrap_or(terminal.len())];
        assert!(block.contains("\"-Command\", \"cd {dir}\"") && block.contains("\"bash\", \"-c\""), "{block}");
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
        assert_eq!(
            names,
            [
                "Resize to 50% (ImageMagick)",
                "Office to PDF (LibreOffice)",
                "E-book to EPUB (Calibre)",
                "Zip together (7-Zip)"
            ]
        );
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
                pdf: Some("op=split split=every every=7 dpi=300 image=jpeg size=a4 margin=small".to_owned()),
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

    #[test]
    fn commands_read_their_shortcut_group_and_question() {
        let (settings, warnings) = parse(
            "[[commands]]\nname = \"Zip\"\nrun = [\"7z\", \"a\", \"x.zip\", \"{files}\"]\nshortcut = \"ctrl+alt+z\"\n\
             menu = \" Archives \"\nask = true\n\
             [[commands]]\nname = \"Plain\"\nrun = [\"x\", \"{in}\"]\nmenu = \"\"\n",
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        let zip = &settings.commands[0];
        assert_eq!(
            (zip.shortcut.as_deref(), zip.menu.as_deref(), zip.ask),
            (Some("ctrl+alt+z"), Some("Archives"), true)
        );
        let plain = &settings.commands[1];
        assert_eq!((plain.shortcut.as_deref(), plain.menu.as_deref(), plain.ask), (None, None, false));
        let chord = parse_chord("ctrl+alt+z", Platform::current()).unwrap().unwrap();
        assert_eq!(settings.shortcuts.command_for(&chord), Some(0));
        assert_eq!(settings.shortcuts.command_chord(0), Some(chord));
        assert_eq!(settings.shortcuts.command_chord(1), None);
    }

    #[test]
    fn a_command_key_taken_by_an_action_or_an_earlier_command_is_left_out() {
        let (settings, warnings) = parse(
            "[shortcuts]\nrefresh = \"ctrl+alt+r\"\n\n\
             [[commands]]\nname = \"A\"\nrun = [\"x\"]\nshortcut = \"mod+t\"\n\
             [[commands]]\nname = \"B\"\nrun = [\"x\"]\nshortcut = \"ctrl+alt+r\"\n\
             [[commands]]\nname = \"C\"\nrun = [\"x\"]\nshortcut = \"ctrl+alt+k\"\n\
             [[commands]]\nname = \"D\"\nrun = [\"x\"]\nshortcut = \"CTRL+ALT+K\"\n\
             [[commands]]\nname = \"E\"\nrun = [\"x\"]\nshortcut = \"ctrl+q+\"\n\
             [[commands]]\nname = \"F\"\nrun = [\"x\"]\nshortcut = \"shift+f10\"\n",
        );
        assert_eq!(settings.commands.len(), 6, "every command stays, only its key goes");
        let messages: Vec<&str> = warnings.iter().map(|w| w.message.as_str()).collect();
        assert_eq!(
            messages,
            [
                "commands[1]: shortcut \"mod+t\" is already used by new-tab; the command has no key",
                "commands[2]: shortcut \"ctrl+alt+r\" is already used by refresh; the command has no key",
                "commands[4]: shortcut \"CTRL+ALT+K\" is already used by \"C\"; the command has no key",
                "commands[5]: shortcut: missing key after \"+\"; the command has no key",
                "commands[6]: shortcut \"shift+f10\" is already used by the file list; the command has no key",
            ]
        );
        let k = parse_chord("ctrl+alt+k", Platform::current()).unwrap().unwrap();
        assert_eq!(settings.shortcuts.command_for(&k), Some(2));
        let t = parse_chord("mod+t", Platform::current()).unwrap().unwrap();
        assert_eq!(
            settings.shortcuts.action_for(&t),
            Some(crate::shortcuts::Action::NewTab),
            "the action keeps its key"
        );
    }

    #[test]
    fn a_command_key_needs_a_modifier_or_an_f_key() {
        // A bare key would take a letter from type-ahead and the filter: refused, with a warning.
        let (settings, warnings) = parse(
            "[[commands]]
name = \"A\"
run = [\"x\"]
shortcut = \"f\"
             [[commands]]
name = \"B\"
run = [\"x\"]
shortcut = \"shift+b\"
             [[commands]]
name = \"C\"
run = [\"x\"]
shortcut = \"delete\"
             [[commands]]
name = \"D\"
run = [\"x\"]
shortcut = \"f7\"
             [[commands]]
name = \"E\"
run = [\"x\"]
shortcut = \"alt+e\"
             [[commands]]
name = \"F\"
run = [\"x\"]
shortcut = \"shift+f8\"
",
        );
        assert_eq!(settings.commands.len(), 6, "every command stays, only its key goes");
        let messages: Vec<&str> = warnings.iter().map(|w| w.message.as_str()).collect();
        let refused = |n: usize, text: &str| {
            format!("commands[{n}]: shortcut \"{text}\" needs Ctrl, Alt or Cmd (or an F key); the command has no key")
        };
        assert_eq!(messages, [refused(1, "f"), refused(2, "shift+b"), refused(3, "delete")]);
        for (i, text) in [(0, "f"), (1, "shift+b"), (2, "delete")] {
            let chord = parse_chord(text, Platform::current()).unwrap().unwrap();
            assert_eq!(settings.shortcuts.command_for(&chord), None, "{text}");
            assert_eq!(settings.shortcuts.command_chord(i), None, "{text}");
        }
        for (i, text) in [(3, "f7"), (4, "alt+e"), (5, "shift+f8")] {
            let chord = parse_chord(text, Platform::current()).unwrap().unwrap();
            assert_eq!(settings.shortcuts.command_for(&chord), Some(i), "{text}");
        }
    }

    #[test]
    fn bad_command_extras_leave_the_command_out() {
        for (body, expected) in [
            ("shortcut = 3", "shortcut must be text"),
            ("menu = true", "menu must be text"),
            ("ask = \"yes\"", "ask must be true or false"),
            ("asks = true", "unknown key \"asks\""),
        ] {
            let (settings, warnings) = parse(&format!("[[commands]]\nname = \"A\"\nrun = [\"x\"]\n{body}\n"));
            assert!(settings.commands.is_empty(), "{body}");
            assert!(warnings[0].message.contains(expected), "{body}: {warnings:?}");
        }
        let (settings, warnings) = parse("[[commands]]\nname = \"A\"\nrun = [\"x\", \"{files}\", \"{in}\"]\n");
        assert!(settings.commands.is_empty());
        assert!(warnings[0].message.contains(gezik_core::batch::convert::FILES_ALONE), "{warnings:?}");
    }

    /// A `[[commands]]` entry written in 5c, before `shortcut`, `menu` and `ask`, still reads
    /// as it did: no key, no group, no question.
    #[test]
    fn a_5c_command_without_the_new_keys_still_reads() {
        let (settings, warnings) = parse(
            "[[commands]]\nname = \"Resize to 50% (ImageMagick)\"\n\
             run = [\"magick\", \"{in}\", \"-resize\", \"50%\", \"{out}\"]\noutput = \"{name}-small.{ext}\"\n\
             types = [\"jpg\", \"jpeg\", \"png\"]\nparallel = 4\n",
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        let command = &settings.commands[0];
        assert_eq!((command.shortcut.as_deref(), command.menu.as_deref(), command.ask), (None, None, false));
        assert_eq!((command.parallel, command.types.len()), (4, 3));
        assert_eq!(settings.shortcuts.command_chord(0), None);
        assert_eq!(settings.shortcuts, Shortcuts::default(), "no command key was added");
    }

    #[test]
    fn history_settings_are_read() {
        assert!(Settings::default().history.remember);
        let (settings, warnings) = parse("[history]\nremember = false\n");
        assert!(warnings.is_empty() && !settings.history.remember, "{warnings:?}");
        let (settings, warnings) = parse("[history]\nremember = \"no\"\n");
        assert!(settings.history.remember);
        assert!(warnings[0].message.starts_with("history.remember: expected true or false"), "{warnings:?}");
        let (_, warnings) = parse("history = 1\n");
        assert!(warnings[0].message.starts_with("history: expected a table"), "{warnings:?}");
    }

    #[test]
    fn folder_history_round_trips_in_state() {
        use gezik_core::history::Visit;
        let state = State {
            history: vec![
                Visit { path: "D:/Work".into(), count: 3, last: 1_800_000_000 },
                Visit { path: "/srv/ş x".into(), count: 1, last: 0 },
            ],
            ..State::default()
        };
        assert_eq!(State::parse(&state.to_toml()), state);
        assert!(!State::default().to_toml().contains("history"));
        let broken = State::parse(
            "[history]\nfolders = [{ path = \"/a\", count = 0 }, { count = 2 }, { path = \"\" }, \
             { path = \"/b\", count = 2, last = -5 }, { path = \"/c\" }]\n",
        );
        assert_eq!(
            broken.history,
            [Visit { path: "/b".into(), count: 2, last: 0 }, Visit { path: "/c".into(), count: 1, last: 0 }]
        );
    }

    #[test]
    fn session_terminal_and_tab_sets_are_read() {
        let (settings, warnings) = parse(
            "[session]\nrestore = false\n\n[terminal]\ncommand = [\"wezterm\", \"start\", \"--cwd\", \"{dir}\"]\n\n\
             [[tab-sets]]\nname = \" Release \"\ntabs = [\"D:/Work/gezik\", \"{downloads}\", \"drives\"]\n",
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(!settings.session.restore);
        assert_eq!(settings.terminal.command.as_ref().map(Vec::len), Some(4));
        assert_eq!(
            settings.tab_sets,
            [TabSet {
                name: "Release".into(),
                tabs: vec!["D:/Work/gezik".into(), "{downloads}".into(), "drives".into()]
            }]
        );
        let defaults = Settings::default();
        assert!(defaults.session.restore && defaults.terminal.command.is_none() && defaults.tab_sets.is_empty());
    }

    #[test]
    fn bad_session_terminal_and_tab_sets_warn() {
        let (settings, warnings) = parse(
            "[session]\nrestore = \"yes\"\n\n[terminal]\ncommand = [\"x\", \"{foo}\"]\n\n\
             [[tab-sets]]\nname = \"\"\ntabs = [\"/a\"]\n\
             [[tab-sets]]\nname = \"Up\"\ntabs = [\"{home}/../x\"]\n\
             [[tab-sets]]\nname = \"None\"\ntabs = []\n\
             [[tab-sets]]\nname = \"Good\"\ntabs = [\"/a\"]\n\
             [[tab-sets]]\nname = \"GOOD\"\ntabs = [\"/b\"]\n\
             [[tab-sets]]\nname = \"Num\"\ntabs = [3]\n",
        );
        assert!(settings.session.restore, "a bad value keeps the default");
        assert_eq!(settings.terminal.command, None);
        assert_eq!(settings.tab_sets.len(), 1);
        let messages: Vec<&str> = warnings.iter().map(|w| w.message.as_str()).collect();
        assert_eq!(
            messages,
            [
                "session.restore: expected true or false, got \"yes\"",
                "terminal.command: unknown placeholder {foo}; Gezik finds a terminal",
                "tab-sets[1]: name is missing",
                "tab-sets[2]: \"{home}/../x\" must not contain \"..\"",
                "tab-sets[3]: tabs is missing",
                "tab-sets[5]: \"GOOD\" is already used; this one is left out",
                "tab-sets[6]: tabs must be a list of text, got 3",
            ]
        );
        let (_, warnings) = parse("terminal = 1\n");
        assert!(warnings[0].message.starts_with("terminal: expected a table"), "{warnings:?}");
        let (_, warnings) = parse("[terminal]\ncommand = \"wt\"\n");
        assert!(warnings[0].message.starts_with("terminal.command: expected a list of text"), "{warnings:?}");
        let (settings, warnings) = parse("[terminal]\ncommand = []\n");
        assert!(warnings.is_empty() && settings.terminal.command.is_none());
    }

    #[test]
    fn the_session_round_trips_in_state() {
        use gezik_core::nav::{Location, Session, SessionTab};
        let state = State {
            session: Session {
                tabs: vec![
                    SessionTab { location: Location::Path(std::env::temp_dir().join("gezik ş 'x'")), locked: true },
                    SessionTab { location: Location::Drives, locked: false },
                ],
                active: 1,
            },
            ..State::default()
        };
        assert_eq!(State::parse(&state.to_toml()), state);
        assert!(!State::default().to_toml().contains("session"));
    }

    #[test]
    fn broken_session_entries_are_left_out() {
        use gezik_core::nav::{Location, Session, SessionTab};
        let abs = std::env::temp_dir().join("a");
        let text = format!(
            "[session]\nactive = 3\n\n[[session.tabs]]\npath = \"relative/x\"\n\n[[session.tabs]]\nlocked = true\n\n\
             [[session.tabs]]\ndrives = true\n\n[[session.tabs]]\npath = {}\nlocked = \"yes\"\n",
            toml::Value::String(abs.to_string_lossy().into_owned())
        );
        let session = State::parse(&text).session;
        assert_eq!(
            session.tabs,
            [
                SessionTab { location: Location::Drives, locked: false },
                SessionTab { location: Location::Path(abs), locked: false }
            ]
        );
        assert_eq!(session.active, 1, "active counts the kept entries");
        let gone = State::parse(
            "[session]\nactive = 0\n\n[[session.tabs]]\npath = \"x\"\n\n[[session.tabs]]\ndrives = true\n",
        );
        assert_eq!(gone.session.active, 0, "the active entry was left out: the first");
        assert_eq!(State::parse("[session]\nactive = -2\n").session, Session::default());
    }

    #[test]
    fn folder_sizes_are_read_and_bad_values_warned() {
        let (settings, warnings) = parse("[view]\nfolder-sizes = \"all\"\n");
        assert_eq!((settings.folder_sizes, warnings.len()), (FolderSizeMode::All, 0));
        assert_eq!(Settings::default().folder_sizes, FolderSizeMode::Off);
        let (settings, warnings) = parse("[view]\nfolder-sizes = \"yes\"\n");
        assert_eq!(settings.folder_sizes, FolderSizeMode::Off);
        assert_eq!(warnings[0].message, "view.folder-sizes: expected \"off\", \"local\" or \"all\", got \"yes\"");
    }

    #[test]
    fn saved_searches_are_read() {
        let text = r#"
            [[searches]]
            name = "Large videos"
            folder = "{home}"
            pattern = "*.mp4;*.mkv"
            size-min = "500 MB"
            modified = "30d"
            type = "videos"

            [[searches]]
            name = "Invoices here"
            folder = "{here}"
            content = "fatura"
        "#;
        let (settings, warnings) = parse(text);
        assert!(warnings.is_empty(), "{warnings:?}");
        let videos = &settings.searches[0];
        assert_eq!((videos.name.as_str(), videos.folder.as_str()), ("Large videos", "{home}"));
        assert_eq!(videos.spec.pattern, "*.mp4;*.mkv");
        assert_eq!(videos.spec.size.min, Some(500 * 1024 * 1024));
        assert_eq!(videos.spec.modified, DateRange::LastDays(30));
        assert_eq!(videos.spec.kind, KindFilter::Videos);
        assert_eq!(
            (settings.searches[1].folder.as_str(), settings.searches[1].spec.content.as_str()),
            (HERE, "fatura")
        );
    }

    #[test]
    fn bad_saved_searches_are_warned_and_left_out() {
        let mut text = String::from(
            "[[searches]]\nfolder = \"{home}\"\n\
             [[searches]]\nname = \"a\"\n\
             [[searches]]\nname = \"b\"\nfolder = \"{home}/../x\"\n\
             [[searches]]\nname = \"c\"\nfolder = \"{home}\"\nsize-min = \"lots\"\n\
             [[searches]]\nname = \"d\"\nfolder = \"{home}\"\ncolour = \"red\"\n\
             [[searches]]\nname = \"E\"\nfolder = \"drives\"\n\
             [[searches]]\nname = \"e\"\nfolder = \"{home}\"\n",
        );
        for i in 0..30 {
            text.push_str(&format!("[[searches]]\nname = \"n{i}\"\nfolder = \"{{here}}\"\n"));
        }
        let (settings, warnings) = parse(&text);
        let messages: Vec<&str> = warnings.iter().map(|w| w.message.as_str()).collect();
        assert_eq!(
            &messages[..6],
            [
                "searches[1]: name is missing",
                "searches[2]: folder is missing",
                "searches[3]: folder: \"{home}/../x\" must not contain \"..\"",
                "searches[4]: size-min: expected a size like \"500 MB\", got \"lots\"",
                "searches[5]: unknown key \"colour\"",
                "searches[7]: \"e\" is already used; this one is left out",
            ]
        );
        assert_eq!(messages[6], "searches[37]: at most 30 saved searches; this one is left out");
        assert_eq!(settings.searches.len(), SEARCHES_MAX);
    }

    #[test]
    fn a_saved_search_writes_back_as_it_reads() {
        let mut spec = SearchSpec::new(Scope::AllDrives);
        spec.pattern = "*.pdf".into();
        spec.content = "fatura".into();
        spec.modified = DateRange::LastDays(7);
        let search = SavedSearch { name: "Faturalar".into(), folder: "{documents}/Muhasebe".into(), spec };
        let table = saved_search_to_toml(&search);
        let keys: Vec<&str> = table.keys().map(String::as_str).collect();
        assert_eq!(keys, ["name", "folder", "pattern", "content", "modified"], "name and folder first");
        assert_eq!(parse_saved_search(&toml::Value::Table(table)).unwrap(), search);
    }

    #[test]
    fn the_palette_remembers_twenty() {
        let state = State { palette_recent: (0..25).map(|i| format!("action:{i}")).collect(), ..State::default() };
        let back = State::parse(&state.to_toml());
        assert_eq!(back.palette_recent.len(), 20);
        assert_eq!(back.palette_recent[0], "action:0");
        assert!(State::parse("[palette]\nrecent = [1, \"tab:2\"]\n").palette_recent == ["tab:2"]);
    }

    #[test]
    fn the_servers_list_keeps_ten() {
        let state = State { servers_recent: (0..12).map(|i| format!(r"\\nas\s{i}")).collect(), ..State::default() };
        let text = state.to_toml();
        assert!(text.contains("[servers]"));
        let back = State::parse(&text);
        assert_eq!(back.servers_recent.len(), SERVERS_MAX);
        assert_eq!(back.servers_recent[0], r"\\nas\s0");
        assert_eq!(State::parse("[servers]\nrecent = [1, \"\", \"smb://nas/a\"]\n").servers_recent, ["smb://nas/a"]);
        assert!(!State::default().to_toml().contains("[servers]"), "nothing written while empty");
    }

    #[test]
    fn the_template_saved_search_example_reads_once_uncommented() {
        let template = include_str!("../templates/settings.toml");
        let start = template.find("# [[searches]]").expect("the template has a saved search example");
        let example: String = template[start..]
            .lines()
            .take_while(|line| line.starts_with('#'))
            .map(|line| format!("{}\n", line.strip_prefix("# ").unwrap_or(line)))
            .collect();
        let (settings, warnings) = parse(&example);
        assert!(warnings.is_empty(), "{warnings:?}\n{example}");
        assert_eq!(settings.searches.len(), 1);
    }

    #[test]
    fn the_template_tab_set_example_reads_once_uncommented() {
        let template = include_str!("../templates/settings.toml");
        let start = template.find("# [[tab-sets]]").expect("the template has a tab set example");
        let example: String = template[start..]
            .lines()
            .take_while(|line| line.starts_with('#'))
            .map(|line| format!("{}\n", line.strip_prefix("# ").unwrap_or(line)))
            .collect();
        let (settings, warnings) = parse(&example);
        assert!(warnings.is_empty(), "{warnings:?}\n{example}");
        assert_eq!(settings.tab_sets.len(), 1);
    }

    /// Files written before 7a (no `[session]`, `[terminal]` or `[[tab-sets]]`) still load
    /// as they did, and the new parts take their defaults.
    #[test]
    fn pre_7a_settings_and_state_still_load() {
        let (settings, warnings) = parse(
            "theme = \"dark\"\nstart-folder = \"{home}\"\n\n[history]\nremember = false\n\n\
             [keyboard]\ntyping = \"filter\"\n\n[shortcuts]\nclear-history = \"ctrl+shift+h\"\n\n\
             [[filters]]\nname = \"Pictures\"\npattern = \"*.jpg\"\n",
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(!settings.history.remember);
        assert_eq!(settings.filters.len(), 1);
        assert_eq!(settings.session, SessionSettings::default());
        assert_eq!(settings.terminal, TerminalSettings::default());
        assert!(settings.tab_sets.is_empty());
        let state = State::parse(
            "[window]\nwidth = 1000\nheight = 700\n\n[preview]\nopen = true\n\n\
             [history]\nfolders = [{ path = \"/a\", count = 2, last = 5 }]\n",
        );
        assert_eq!(state.window.map(|w| w.width), Some(1000));
        assert!(state.preview_open);
        assert_eq!(state.history.len(), 1);
        assert!(state.session.is_empty());
    }

    #[test]
    fn search_settings_read_with_their_defaults_and_warnings() {
        let (settings, warnings) = parse(
            "[search]\neverything = \"off\"\nskip = [\".git\", \"target\"]\nmax-results = 5000\ncontent-max-size = \"10 MB\"\n",
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(
            settings.search,
            SearchSettings {
                everything: false,
                skip: vec![".git".into(), "target".into()],
                max_results: 5000,
                content_max_size: 10 * 1024 * 1024
            }
        );
        let (bad, warnings) =
            parse("[search]\neverything = \"yes\"\nskip = [\"a/b\"]\nmax-results = 10\ncontent-max-size = \"lots\"\n");
        assert_eq!(bad.search, SearchSettings::default());
        let texts: Vec<String> = warnings.iter().map(|w| w.to_string()).collect();
        assert_eq!(texts.len(), 4, "{texts:?}");
        assert!(texts[0].contains("search.everything: expected \"auto\" or \"off\""), "{}", texts[0]);
        assert!(texts[1].contains("search.skip: expected a list of folder names"), "{}", texts[1]);
        assert!(texts[2].contains("search.max-results: expected a number from 1000 to 2000000"), "{}", texts[2]);
        assert!(texts[3].contains("search.content-max-size: expected a size like"), "{}", texts[3]);
        let defaults = SearchSettings::default();
        assert!(
            defaults.everything && defaults.max_results == 250_000 && defaults.content_max_size == 64 * 1024 * 1024
        );
        assert_eq!(defaults.skip, [".git", "node_modules"]);
    }

    #[test]
    fn a_search_round_trips_through_its_table() {
        use gezik_core::search::{DateRange, Day, HiddenRule, KindFilter, Scope, SearchSpec};
        let mut spec = SearchSpec::new(Scope::Folder(std::env::temp_dir().join("Work")));
        spec.pattern = "*.pdf".into();
        spec.content = "fatura".into();
        spec.match_case = true;
        spec.size.min = Some(500 * 1024 * 1024);
        spec.modified = DateRange::Between(Day { year: 2026, month: 1, day: 1 }, Day { year: 2026, month: 6, day: 30 });
        spec.kind = KindFilter::Videos;
        spec.hidden = HiddenRule::Include;
        spec.skipped = true;
        let table = search_to_toml(&spec);
        assert_eq!(table.get("size-min").and_then(|v| v.as_str()), Some("500 MB"));
        assert_eq!(table.get("type").and_then(|v| v.as_str()), Some("videos"));
        assert_eq!(search_from_toml(&table), Ok(spec));
        let drives = SearchSpec::new(Scope::AllDrives);
        assert_eq!(search_to_toml(&drives).get("folder").and_then(|v| v.as_str()), Some("drives"));
        assert_eq!(search_from_toml(&search_to_toml(&drives)), Ok(drives));
        let bad: toml::Table = "folder = \"relative\"".parse().unwrap();
        assert!(search_from_toml(&bad).unwrap_err().contains("not a full path"));
        let bad: toml::Table = "folder = \"drives\"\nmodified = \"soon\"".parse().unwrap();
        assert!(search_from_toml(&bad).unwrap_err().starts_with("modified: "));
        assert_eq!(search_from_toml(&toml::Table::new()).unwrap_err(), "folder is missing");
    }

    #[test]
    fn search_and_flat_tabs_come_back_and_bad_ones_are_left_out() {
        use gezik_core::nav::{Location, Session, SessionTab};
        use gezik_core::search::{Scope, SearchSpec};
        let folder = std::env::temp_dir().join("Work");
        let mut spec = SearchSpec::new(Scope::Folder(folder.clone()));
        spec.pattern = "*.pdf".into();
        let state = State {
            session: Session {
                tabs: vec![
                    SessionTab { location: Location::Search(Box::new(spec)), locked: false },
                    SessionTab { location: Location::Flat(folder.clone()), locked: true },
                ],
                active: 1,
            },
            ..State::default()
        };
        assert_eq!(State::parse(&state.to_toml()).session, state.session);
        let shown = folder.display().to_string();
        let text = format!(
            "[session]\nactive = 2\n\n[[session.tabs]]\nsearch = {{ folder = \"x\" }}\n\n[[session.tabs]]\npath = {shown:?}\n\n[[session.tabs]]\nflat = {shown:?}\n"
        );
        let session = State::parse(&text).session;
        assert_eq!(session.tabs.len(), 2, "a search without a full path is left out");
        assert_eq!(session.active, 1);
        assert_eq!(session.tabs[1].location, Location::Flat(folder));
    }

    #[test]
    fn a_trash_tab_is_saved_and_read_back() {
        use gezik_core::nav::{Location, Session, SessionTab};
        let state = State {
            session: Session { tabs: vec![SessionTab { location: Location::Trash, locked: false }], active: 0 },
            ..State::default()
        };
        let text = state.to_toml();
        assert!(text.contains("trash = true"), "{text}");
        assert_eq!(State::parse(&text).session.tabs[0].location, Location::Trash);
    }

    #[test]
    fn result_columns_are_kept_apart_from_the_folders() {
        let mut columns = gezik_core::view::default_result_columns();
        columns[0].width = 333;
        let state = State { result_columns: Some(columns.clone()), ..State::default() };
        let back = State::parse(&state.to_toml());
        assert_eq!(back.result_columns, Some(columns));
        assert_eq!(back.columns, None);
    }

    #[test]
    fn single_instance_is_on_unless_turned_off() {
        assert!(Settings::default().system.single_instance);
        let (settings, warnings) = parse("[system]\nsingle-instance = false\n");
        assert!(!settings.system.single_instance);
        assert!(warnings.is_empty());
        let (settings, warnings) = parse("[system]\nsingle-instance = \"no\"\n");
        assert!(settings.system.single_instance, "a bad value keeps the default");
        assert_eq!(warnings[0].message, "system.single-instance: expected true or false, got \"no\"");
        let (_, warnings) = parse("system = 3\n");
        assert_eq!(warnings[0].message, "system: expected a table, got 3");
    }
}
