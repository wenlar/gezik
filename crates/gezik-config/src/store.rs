//! The config folder on disk: layout, first-run files, reading and resolving.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::Warning;
use crate::paths::{config_dir, write_atomic};
use crate::settings::{Density, Settings, State};
use crate::settings_writer::{Done, SettingsChange, SettingsWriter};
use crate::state_store::StateCell;
use crate::theme::{ResolvedTheme, ThemeError, resolve_theme};
use crate::views_file::parse_views;
use crate::views_writer::ViewsWriter;
use gezik_core::view::ViewSettings;
use gezik_core::view_memory::ViewMemory;

pub(crate) const SETTINGS_TEMPLATE: &str = include_str!("../templates/settings.toml");
const THEME_TEMPLATE: &str = include_str!("../templates/example.toml");

/// Raw contents of the config folder. Reading does I/O, so it happens off the UI thread;
/// resolving it ([`resolve`]) is cheap.
#[derive(Debug, Clone, Default)]
pub struct ConfigFiles {
    pub settings: Option<String>,
    /// Lowercase theme id (file name without `.toml`) → TOML text.
    pub themes: HashMap<String, String>,
    /// Problems found while reading: unreadable files, reserved names.
    pub warnings: Vec<Warning>,
}

/// The config folder. Its clones share one `state.toml` in memory and its writer thread, one
/// `settings.toml` writer thread and one `views.toml` writer thread.
#[derive(Debug, Clone)]
pub struct ConfigStore {
    dir: PathBuf,
    state: Arc<StateCell>,
    settings: Arc<SettingsWriter>,
    views: Arc<ViewsWriter>,
}

impl ConfigStore {
    pub fn new(dir: PathBuf) -> Self {
        let state = Arc::new(StateCell::new(dir.join("state.toml")));
        let settings = Arc::new(SettingsWriter::new(dir.clone()));
        let views = Arc::new(ViewsWriter::new(dir.join("views.toml")));
        Self { dir, state, settings, views }
    }

    /// The store in the standard per-user location, if the OS has one.
    pub fn system() -> Option<Self> {
        config_dir().map(Self::new)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn settings_path(&self) -> PathBuf {
        self.dir.join("settings.toml")
    }

    fn themes_dir(&self) -> PathBuf {
        self.dir.join("themes")
    }

    /// Creates the folder layout and commented starter files on the true first run, when
    /// the config folder does not exist yet. Later runs never recreate files the user
    /// deleted.
    pub fn ensure_initialized(&self) -> io::Result<()> {
        let first_run = !self.dir.exists();
        std::fs::create_dir_all(self.themes_dir())?;
        if first_run {
            write_if_missing(&self.settings_path(), SETTINGS_TEMPLATE)?;
            write_if_missing(&self.themes_dir().join("example.toml"), THEME_TEMPLATE)?;
        }
        Ok(())
    }

    pub fn read_files(&self) -> ConfigFiles {
        let mut files = ConfigFiles::default();
        match read_text(&self.settings_path()) {
            Ok(text) => files.settings = Some(text),
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => files.warnings.push(Warning::new("settings.toml", format!("cannot read: {err}"))),
        }

        let Ok(entries) = std::fs::read_dir(self.themes_dir()) else { return files };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else { continue };
            let file = format!("{stem}.toml");
            let id = stem.to_lowercase();
            if id == "auto" {
                files.warnings.push(Warning::new(file, "\"auto\" is a reserved name; rename this theme"));
                continue;
            }
            match read_text(&path) {
                Ok(text) => {
                    files.themes.insert(id, text);
                }
                Err(err) => files.warnings.push(Warning::new(file, format!("cannot read: {err}"))),
            }
        }
        files
    }

    fn views_path(&self) -> PathBuf {
        self.dir.join("views.toml")
    }

    /// The last state: read from `state.toml` the first time (at start), then kept in memory.
    pub fn load_state(&self) -> State {
        self.state.get()
    }

    /// Changes the state in memory; one thread of its own writes `state.toml` soon after
    /// (the UI thread never waits for the disk). Changes from anywhere, close together, all
    /// reach the file.
    pub fn update_state(&self, change: impl FnOnce(&mut State)) {
        self.state.update(change);
    }

    /// Waits (up to 5 s) until every change of the state is written: before quitting.
    pub fn flush_state(&self) {
        self.state.flush();
    }

    /// Sets the state and writes it now, on this thread.
    pub fn save_state(&self, state: &State) -> io::Result<()> {
        self.state.save(state)
    }

    /// Has the one `settings.toml` writer thread make `change` (keeping everything else in the
    /// file), after every change sent before it; `done` gets the result on that thread. The
    /// UI writes settings only this way: it never waits for the disk, and no change is lost.
    pub fn write_settings(&self, change: SettingsChange, done: impl FnOnce(Result<(), Warning>) + Send + 'static) {
        let done: Done = Box::new(done);
        self.settings.send(change, done);
    }

    /// Waits (up to 5 s) until every settings change sent so far is written: before quitting.
    pub fn flush_settings(&self) {
        self.settings.flush();
    }

    /// Makes `change` to `settings.toml` now, on this thread (one at a time with the writer
    /// thread). Creates the file from the template if it does not exist; refuses to touch a
    /// broken file.
    pub fn save_settings(&self, change: &SettingsChange) -> Result<(), Warning> {
        self.settings.save(change)
    }

    /// Writes the pinned folders into `settings.toml` now, keeping everything else.
    pub fn save_pinned(&self, pinned: &[String]) -> Result<(), Warning> {
        self.save_settings(&SettingsChange::Pinned(pinned.to_vec()))
    }

    /// Writes `view` as the `[view]` defaults ("Apply to all folders") now.
    pub fn save_view_defaults(&self, view: &ViewSettings) -> Result<(), Warning> {
        self.save_settings(&SettingsChange::ViewDefaults(*view))
    }

    /// Writes the saved rename rule sets into `settings.toml` now.
    pub fn save_rename_presets(&self, presets: &[crate::settings::RenamePreset]) -> Result<(), Warning> {
        self.save_settings(&SettingsChange::RenamePresets(presets.to_vec()))
    }

    /// Writes the saved filters into `settings.toml` now.
    pub fn save_filters(&self, filters: &[crate::settings::SavedFilter]) -> Result<(), Warning> {
        self.save_settings(&SettingsChange::Filters(filters.to_vec()))
    }

    /// The saved folder views. A missing file means none; an unreadable or broken one
    /// starts over, with a warning.
    pub fn load_views(&self) -> (ViewMemory, Option<Warning>) {
        match read_text(&self.views_path()) {
            Ok(text) => match parse_views(&text) {
                Ok(folders) => (ViewMemory::from_folders(folders), None),
                Err(err) => {
                    (ViewMemory::default(), Some(Warning::new("views.toml", format!("{err}; folder views start over"))))
                }
            },
            Err(err) if err.kind() == io::ErrorKind::NotFound => (ViewMemory::default(), None),
            Err(err) => (
                ViewMemory::default(),
                Some(Warning::new("views.toml", format!("cannot read: {err}; folder views start over"))),
            ),
        }
    }

    /// Writes the folder views now, on this thread.
    pub fn save_views(&self, memory: &ViewMemory) -> io::Result<()> {
        self.views.save(memory.folders())
    }

    /// Has the one `views.toml` writer thread write the folder views as they are now; a burst
    /// of them is written once. The UI writes views only this way: it never waits for the
    /// disk. A failed write is said on the error output.
    pub fn write_views(&self, memory: &ViewMemory) {
        self.views.send(memory.folders().to_vec());
    }

    /// Waits (up to 5 s) until the folder views handed over so far are written: before
    /// quitting.
    pub fn flush_views(&self) {
        self.views.flush();
    }

    /// Whether a change to `path` should reload the config: `settings.toml` or a theme
    /// file, but not `state.toml` or editor temp files.
    pub fn is_config_file(&self, path: &Path) -> bool {
        let Ok(relative) = path.strip_prefix(&self.dir) else { return false };
        let parts: Vec<String> = relative.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
        match parts.as_slice() {
            [name] => name == "settings.toml",
            [dir, name] => dir == "themes" && name.ends_with(".toml") && !name.starts_with('.'),
            _ => false,
        }
    }
}

/// Everything the UI needs after (re)loading the config folder.
#[derive(Debug, Clone)]
pub struct Loaded {
    pub settings: Settings,
    /// `None` when the selected theme has a syntax error: keep showing the current one.
    pub theme: Option<ResolvedTheme>,
    pub warnings: Vec<Warning>,
}

/// Reads a text file, dropping the UTF-8 byte order mark some Windows editors add.
pub(crate) fn read_text(path: &Path) -> io::Result<String> {
    let text = std::fs::read_to_string(path)?;
    Ok(match text.strip_prefix('\u{feff}') {
        Some(rest) => rest.to_owned(),
        None => text,
    })
}

fn write_if_missing(path: &Path, contents: &str) -> io::Result<()> {
    if path.exists() {
        return Ok(());
    }
    write_atomic(path, contents)
}

pub fn resolve(files: &ConfigFiles, system_dark: bool) -> Loaded {
    let mut warnings = files.warnings.clone();
    let settings =
        files.settings.as_deref().map(|text| Settings::parse("settings.toml", text, &mut warnings)).unwrap_or_default();

    let id = settings.active_theme(system_dark).to_owned();
    let theme = match resolve_theme(&id, &files.themes, &mut warnings) {
        Ok(theme) => Some(theme),
        Err(ThemeError::Invalid) => None,
        Err(ThemeError::NotFound) => {
            warnings.push(Warning::new("settings.toml", format!("theme \"{id}\" not found; using \"dark\"")));
            resolve_theme("dark", &files.themes, &mut warnings).ok()
        }
    };
    let theme = theme.map(|mut theme| {
        if settings.density == Density::Compact {
            theme.metrics = theme.metrics.compact();
        }
        theme
    });
    Loaded { settings, theme, warnings }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Color;
    use crate::settings::WindowState;

    fn store(name: &str) -> ConfigStore {
        ConfigStore::new(crate::test_dir(name))
    }

    /// A store whose folder does not exist yet (a true first run).
    fn fresh_store(name: &str) -> ConfigStore {
        ConfigStore::new(crate::test_dir(name).join("gezik"))
    }

    fn write(store: &ConfigStore, relative: &str, text: &str) {
        let path = store.dir().join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn first_run_creates_starter_files_once() {
        let store = fresh_store("init");
        store.ensure_initialized().unwrap();
        assert!(store.dir().join("settings.toml").is_file());
        assert!(store.dir().join("themes/example.toml").is_file());

        std::fs::remove_file(store.dir().join("themes/example.toml")).unwrap();
        store.ensure_initialized().unwrap();
        assert!(!store.dir().join("themes/example.toml").exists(), "deleted starter file came back");

        write(&store, "settings.toml", "theme = \"light\"\n");
        store.ensure_initialized().unwrap();
        assert_eq!(std::fs::read_to_string(store.dir().join("settings.toml")).unwrap(), "theme = \"light\"\n");
    }

    #[test]
    fn starter_files_are_valid() {
        let store = fresh_store("starter");
        store.ensure_initialized().unwrap();
        let files = store.read_files();
        let loaded = resolve(&files, false);
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        assert_eq!(loaded.settings.view, crate::settings::ViewDefaults::default());
        assert_eq!(loaded.theme.unwrap().id, "light");

        let mut warnings = Vec::new();
        let example = resolve_theme("example", &files.themes, &mut warnings).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(example.name, "Example (Nord)");
    }

    #[test]
    fn strips_utf8_bom_and_matches_theme_names_case_insensitively() {
        let store = store("bom");
        write(&store, "settings.toml", "\u{feff}theme = \"nord\"\n");
        write(&store, "themes/Nord.toml", "\u{feff}[colors]\naccent = \"#88c0d0\"\n");
        let loaded = resolve(&store.read_files(), true);
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        let theme = loaded.theme.unwrap();
        assert_eq!(theme.id, "nord");
        assert_eq!(theme.colors.accent, Color::parse("#88c0d0").unwrap());
    }

    #[test]
    fn reserved_auto_theme_is_skipped_with_warning() {
        let store = store("auto");
        write(&store, "themes/auto.toml", "");
        let files = store.read_files();
        assert!(!files.themes.contains_key("auto"));
        assert_eq!(files.warnings.len(), 1);
        assert_eq!(files.warnings[0].file, "auto.toml");
    }

    #[test]
    fn missing_settings_file_means_defaults() {
        let loaded = resolve(&store("nosettings").read_files(), true);
        assert!(loaded.warnings.is_empty());
        assert_eq!(loaded.settings, Settings::default());
        assert_eq!(loaded.theme.unwrap().id, "dark");
    }

    #[test]
    fn missing_selected_theme_falls_back_to_dark() {
        let store = store("missing");
        write(&store, "settings.toml", "theme = \"gone\"\n");
        let loaded = resolve(&store.read_files(), false);
        assert_eq!(loaded.theme.unwrap().id, "dark");
        assert_eq!(loaded.warnings.len(), 1);
        assert!(loaded.warnings[0].message.contains("\"gone\""));
    }

    #[test]
    fn broken_selected_theme_keeps_current() {
        let store = store("broken");
        write(&store, "settings.toml", "theme = \"bad\"\n");
        write(&store, "themes/bad.toml", "[colors\n");
        let loaded = resolve(&store.read_files(), false);
        assert!(loaded.theme.is_none());
        assert_eq!(loaded.warnings.len(), 1);
        assert_eq!(loaded.warnings[0].file, "bad.toml");
        assert_eq!(loaded.warnings[0].line, Some(1));
    }

    #[test]
    fn compact_density_is_applied() {
        let store = store("density");
        write(&store, "settings.toml", "[layout]\ndensity = \"compact\"\n");
        let theme = resolve(&store.read_files(), true).theme.unwrap();
        assert_eq!(theme.metrics.row_height, 21.0);
    }

    #[test]
    fn state_round_trips_through_disk() {
        let store = store("state");
        assert_eq!(store.load_state(), State::default());
        let state = State {
            window: Some(WindowState { width: 1000, height: 700, x: Some(10), y: Some(20) }),
            sidebar_width: None,
            ..State::default()
        };
        store.save_state(&state).unwrap();
        assert_eq!(store.load_state(), state);
        // Read again from the file by a new store; a change written by the writer thread.
        let again = ConfigStore::new(store.dir().to_path_buf());
        assert_eq!(again.load_state(), state);
        again.update_state(|state| state.preview_open = true);
        again.flush_state();
        assert!(ConfigStore::new(store.dir().to_path_buf()).load_state().preview_open);
        let names: Vec<_> = std::fs::read_dir(store.dir()).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names, ["state.toml"]);
    }

    #[test]
    fn recognizes_config_files() {
        let store = store("watch");
        let root = store.dir();
        assert!(store.is_config_file(&root.join("settings.toml")));
        assert!(store.is_config_file(&root.join("themes").join("nord.toml")));
        assert!(!store.is_config_file(&root.join("state.toml")));
        assert!(!store.is_config_file(&root.join(".settings.toml.tmp")));
        assert!(!store.is_config_file(&root.join("themes").join(".nord.toml.tmp")));
        assert!(!store.is_config_file(&root.join("themes").join(".nord.toml.swp")));
        assert!(!store.is_config_file(&root.join("themes").join("nord.toml~")));
        assert!(!store.is_config_file(&root.join("themes").join("old").join("nord.toml")));
        assert!(!store.is_config_file(Path::new("/elsewhere/settings.toml")));
    }

    #[test]
    fn save_pinned_keeps_user_comments() {
        let store = store("pin-save");
        write(&store, "settings.toml", "# mine\ntheme = \"dark\"\n");
        store.save_pinned(&["/a".to_owned()]).unwrap();
        let text = std::fs::read_to_string(store.dir().join("settings.toml")).unwrap();
        assert!(text.contains("# mine"));
        let loaded = resolve(&store.read_files(), true);
        assert_eq!(loaded.settings.pinned, ["/a"]);
    }

    #[test]
    fn save_pinned_creates_missing_file_from_template() {
        let store = store("pin-create");
        store.save_pinned(&["/a".to_owned()]).unwrap();
        let loaded = resolve(&store.read_files(), true);
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        assert_eq!(loaded.settings.pinned, ["/a"]);
    }

    #[test]
    fn save_pinned_refuses_a_broken_file() {
        let store = store("pin-broken");
        write(&store, "settings.toml", "theme = \n");
        let err = store.save_pinned(&["/a".to_owned()]).unwrap_err();
        assert!(err.message.starts_with("Fix settings.toml first"), "{}", err.message);
        assert_eq!(std::fs::read_to_string(store.dir().join("settings.toml")).unwrap(), "theme = \n");
    }

    #[test]
    fn save_pinned_handles_bom() {
        let store = store("pin-bom");
        write(&store, "settings.toml", "\u{feff}# mine\ntheme = \"dark\"\n");
        store.save_pinned(&["/a".to_owned()]).unwrap();
        // Read raw file to verify no BOM is written
        let text = std::fs::read_to_string(store.dir().join("settings.toml")).unwrap();
        assert!(!text.starts_with('\u{feff}'), "BOM should not be written");
        assert!(text.contains("# mine"), "user comment should be kept");
        let loaded = resolve(&store.read_files(), true);
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        assert_eq!(loaded.settings.pinned, ["/a"]);
    }

    #[test]
    fn save_pinned_refuses_non_array_pinned() {
        let store = store("pin-non-array");
        write(&store, "settings.toml", "pinned = \"not a list\"\n");
        let err = store.save_pinned(&["/a".to_owned()]).unwrap_err();
        assert!(err.message.starts_with("Fix settings.toml first"), "{}", err.message);
        assert!(err.message.contains("pinned must be a list"), "{}", err.message);
        // File must not be modified
        assert_eq!(std::fs::read_to_string(store.dir().join("settings.toml")).unwrap(), "pinned = \"not a list\"\n");
    }

    #[test]
    fn views_round_trip_through_disk() {
        use gezik_core::view::{ViewMode, ViewSettings};
        let store = store("views");
        let (memory, warning) = store.load_views();
        assert!(memory.folders().is_empty() && warning.is_none(), "missing file: empty, quietly");
        let mut memory = memory;
        memory.set("/pics", ViewSettings { mode: ViewMode::Grid, ..ViewSettings::default() });
        store.save_views(&memory).unwrap();
        let (mut back, warning) = store.load_views();
        assert!(warning.is_none());
        assert_eq!(back.get("/pics").map(|v| v.mode), Some(ViewMode::Grid));
    }

    #[test]
    fn views_handed_to_the_writer_are_on_disk_after_a_flush() {
        use gezik_core::view::{ViewMode, ViewSettings};
        let store = store("views-handed-over");
        let mut memory = ViewMemory::default();
        for i in 0..10 {
            memory.set(&format!("/f{i}"), ViewSettings { mode: ViewMode::Grid, ..ViewSettings::default() });
            store.write_views(&memory);
        }
        // As on quitting: a clone of the store (the window's) flushes what the view sent.
        store.clone().flush_views();
        let (mut back, warning) = store.load_views();
        assert!(warning.is_none());
        assert_eq!(back.folders().len(), 10);
        assert_eq!(back.get("/f9").map(|v| v.mode), Some(ViewMode::Grid));
    }

    #[test]
    fn broken_views_file_starts_empty_with_a_warning() {
        let store = store("views-broken");
        write(&store, "views.toml", "[[folder]\n");
        let (memory, warning) = store.load_views();
        assert!(memory.folders().is_empty());
        let warning = warning.unwrap();
        assert_eq!(warning.file, "views.toml");
        assert!(warning.message.contains("start over"), "{}", warning.message);
    }

    #[test]
    fn view_defaults_are_written_into_settings() {
        use gezik_core::view::{ViewMode, ViewSettings};
        let store = store("view-defaults");
        write(&store, "settings.toml", "# mine\ntheme = \"dark\"\n");
        store.save_view_defaults(&ViewSettings { mode: ViewMode::Grid, ..ViewSettings::default() }).unwrap();
        let text = std::fs::read_to_string(store.dir().join("settings.toml")).unwrap();
        assert!(text.contains("# mine"));
        assert_eq!(resolve(&store.read_files(), true).settings.view.view.mode, ViewMode::Grid);
        write(&store, "settings.toml", "theme = \n");
        let err = store.save_view_defaults(&ViewSettings::default()).unwrap_err();
        assert!(err.message.starts_with("Fix settings.toml first"), "{}", err.message);
    }

    #[test]
    fn views_file_is_not_a_config_file() {
        let store = store("views-watch");
        assert!(!store.is_config_file(&store.dir().join("views.toml")));
    }

    #[test]
    fn saved_filters_are_written_into_settings() {
        use crate::settings::SavedFilter;
        let store = store("save-filters");
        write(
            &store,
            "settings.toml",
            "# mine
theme = \"dark\"
",
        );
        let filter = SavedFilter { name: "Resimler".into(), pattern: "*.jpg;*.png".into() };
        store.save_filters(std::slice::from_ref(&filter)).unwrap();
        let text = std::fs::read_to_string(store.dir().join("settings.toml")).unwrap();
        assert!(text.contains("# mine"));
        assert_eq!(resolve(&store.read_files(), true).settings.filters, [filter]);
        write(
            &store,
            "settings.toml",
            "theme = 
",
        );
        let err = store.save_filters(&[]).unwrap_err();
        assert!(err.message.starts_with("Fix settings.toml first"), "{}", err.message);
    }
}
