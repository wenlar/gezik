//! The config folder on disk: layout, first-run files, reading and resolving.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::paths::{Dirs, location, write_atomic};
use crate::pins::PinEntry;
use crate::settings::{Density, Settings, State};
use crate::settings_writer::{Done, SettingsChange, SettingsWriter};
use crate::state_store::StateCell;
use crate::sync_folder::{self, CACHE, LOCAL_SETTINGS, OFFLINE_NOTE};
use crate::theme::{ResolvedTheme, ThemeError, resolve_theme};
use crate::views_file::parse_views;
use crate::views_writer::ViewsWriter;
use crate::{Warning, lock};
use gezik_core::view::ViewSettings;
use gezik_core::view_memory::ViewMemory;

/// The commented `settings.toml` written on the first run (the settings window reads its comments).
pub const SETTINGS_TEMPLATE: &str = include_str!("../templates/settings.toml");
const THEME_TEMPLATE: &str = include_str!("../templates/example.toml");

/// Raw contents of the config folder. Reading does I/O, so it happens off the UI thread;
/// resolving it ([`resolve`]) is cheap.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConfigFiles {
    pub settings: Option<String>,
    /// Lowercase theme id (file name without `.toml`) → TOML text.
    pub themes: HashMap<String, String>,
    /// Problems found while reading: unreadable files, reserved names.
    pub warnings: Vec<Warning>,
    /// settings.local.toml, read over settings.toml (only with a sync folder; spec 14 §8.2).
    pub local_settings: Option<String>,
    /// The sync folder could not be read: these are its last copy (spec 14 §8.4).
    pub offline: bool,
    /// The lang folder's languages (spec 15 §11.1): code (`pl` for `lang/pl.po`) and texts.
    pub languages: Vec<(String, Arc<gezik_i18n::Layer>)>,
    /// The sync services' conflicting copies in the sync folder (`settings (1).toml`,
    /// `themes/nord (1).toml`), as named there.
    pub conflicts: Vec<String>,
}

/// The config folder. Its clones share one `state.toml` in memory and its writer thread, one
/// `settings.toml` writer thread, one `views.toml` writer thread, and where the shared folder is.
#[derive(Debug, Clone)]
pub struct ConfigStore {
    /// The shared folder moves when a sync folder is set or left (spec 14 §8.1).
    dirs: Arc<Mutex<Dirs>>,
    local: PathBuf,
    state: Arc<StateCell>,
    settings: Arc<SettingsWriter>,
    views: Arc<ViewsWriter>,
}

impl ConfigStore {
    /// A store in one folder.
    pub fn new(dir: PathBuf) -> Self {
        Self::with_dirs(Dirs::one(dir))
    }

    pub fn with_dirs(dirs: Dirs) -> Self {
        let state = Arc::new(StateCell::new(dirs.local.join("state.toml")));
        let views = Arc::new(ViewsWriter::new(dirs.local.join("views.toml")));
        let local = dirs.local.clone();
        let dirs = Arc::new(Mutex::new(dirs));
        let settings = Arc::new(SettingsWriter::new(dirs.clone()));
        Self { dirs, local, state, settings, views }
    }

    /// Where the files are now.
    pub fn dirs(&self) -> Dirs {
        lock(&self.dirs).clone()
    }

    /// Settings, themes and templates from now on in sync folder `folder`, or (`None`) in the
    /// local folder again. The caller has copied what has to be there, and reloads.
    pub fn set_shared(&self, folder: Option<PathBuf>) {
        let mut dirs = lock(&self.dirs);
        dirs.synced = folder.is_some();
        dirs.shared = folder.unwrap_or_else(|| self.local.clone());
    }

    /// Whether settings.toml is in a sync folder.
    pub fn is_synced(&self) -> bool {
        lock(&self.dirs).synced
    }

    /// The store where [`location`] says (portable or the per-user folder), if there is one.
    pub fn system() -> Option<Self> {
        location().dirs.clone().map(Self::with_dirs)
    }

    /// settings.toml, the themes and the templates.
    pub fn shared_dir(&self) -> PathBuf {
        lock(&self.dirs).shared.clone()
    }

    /// This machine's files: state, views, tags, backups, tools, system changes.
    pub fn local_dir(&self) -> &Path {
        &self.local
    }

    pub fn is_portable(&self) -> bool {
        lock(&self.dirs).portable
    }

    /// The folder downloaded tools go in: `tools/`, or in portable mode `tools/<os>-<arch>/`
    /// (one drive can be used on three systems; spec 14 §9.2).
    pub fn tools_dir(&self) -> PathBuf {
        tools_dir(&self.dirs())
    }

    fn settings_path(&self) -> PathBuf {
        self.shared_dir().join("settings.toml")
    }

    fn themes_dir(&self) -> PathBuf {
        self.shared_dir().join("themes")
    }

    /// The user's templates for New ▸ (spec 8.1).
    pub fn templates_dir(&self) -> PathBuf {
        self.shared_dir().join("templates")
    }

    /// Whether `path` is the templates folder or in it: New ▸ reads the folder again (and
    /// settings are not reloaded for it).
    pub fn is_template_path(&self, path: &Path) -> bool {
        path.strip_prefix(self.shared_dir())
            .is_ok_and(|rest| rest.components().next().is_some_and(|first| first.as_os_str() == "templates"))
    }

    /// Creates the folder layout (and the templates folder) and commented starter files on the true first run, when
    /// the config folder does not exist yet. Later runs never recreate files the user
    /// deleted. A sync folder is never made here (it may only be missing for now).
    pub fn ensure_initialized(&self) -> io::Result<()> {
        let dirs = self.dirs();
        if dirs.synced {
            return std::fs::create_dir_all(&dirs.local);
        }
        let first_run = !dirs.shared.exists();
        std::fs::create_dir_all(self.themes_dir())?;
        if dirs.local != dirs.shared {
            std::fs::create_dir_all(&dirs.local)?;
        }
        if first_run {
            std::fs::create_dir_all(self.templates_dir())?;
            write_if_missing(&self.settings_path(), SETTINGS_TEMPLATE)?;
            write_if_missing(&self.themes_dir().join("example.toml"), THEME_TEMPLATE)?;
        }
        Ok(())
    }

    /// settings.toml and the themes (with a sync folder: theirs, or their last copy when the
    /// folder cannot be read, and settings.local.toml). I/O: off the UI thread but at start.
    pub fn read_files(&self) -> ConfigFiles {
        let dirs = self.dirs();
        if !dirs.synced {
            return read_folder(&dirs.shared, None);
        }
        let cache = self.local.join(CACHE);
        let mut files = read_folder(&dirs.shared, Some(&sync_folder::machines(&dirs.shared)));
        if files.settings.is_some() {
            keep_copy(&dirs.shared, &cache, &files);
        } else {
            // Not there (a drive not plugged in, Dropbox not running): the last copy, read-only.
            let warnings = std::mem::take(&mut files.warnings);
            files = read_folder(&cache, None);
            files.warnings.splice(0..0, warnings);
            files.warnings.push(Warning::new("sync", OFFLINE_NOTE));
            files.offline = true;
            // With the changes made since (kept for the folder): as this computer last showed it.
            if let Some(text) = self.settings.unsaved_text() {
                files.settings = Some(text);
            }
        }
        match read_text(&self.local.join(LOCAL_SETTINGS)) {
            Ok(text) => files.local_settings = Some(text),
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => files.warnings.push(Warning::new(LOCAL_SETTINGS, format!("cannot read: {err}"))),
        }
        files
    }

    /// Hands the settings changes not written yet (the sync folder was away) to the writer
    /// again: after a reload that found the folder.
    pub fn retry_unsaved(&self) {
        self.settings.retry_unsaved();
    }

    /// How many settings changes wait for the sync folder.
    pub fn unsaved(&self) -> usize {
        self.settings.unsaved()
    }

    fn views_path(&self) -> PathBuf {
        self.local.join("views.toml")
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

    /// `state.toml` and `views.toml` are no longer written by this Gezik (a second window's,
    /// spec 5.3): its old copy must not overwrite the first window's. `settings.toml` is
    /// still written: its writer merges each change into the file.
    pub fn keep_state_unwritten(&self) {
        self.state.keep_unwritten();
        self.views.keep_unwritten();
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
    pub fn save_pinned(&self, pinned: &[PinEntry]) -> Result<(), Warning> {
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

    /// Writes the tab sets into `settings.toml` now.
    pub fn save_tab_sets(&self, sets: &[crate::settings::TabSet]) -> Result<(), Warning> {
        self.save_settings(&SettingsChange::TabSets(sets.to_vec()))
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
    /// file, but not `state.toml` or editor temp files. With a sync folder also the folder itself
    /// (it came back), a conflicting copy at its top, and settings.local.toml.
    pub fn is_config_file(&self, path: &Path) -> bool {
        let dirs = self.dirs();
        if dirs.synced && (path == dirs.shared || path == self.local.join(LOCAL_SETTINGS)) {
            return true;
        }
        let Ok(relative) = path.strip_prefix(&dirs.shared) else { return false };
        let parts: Vec<String> = relative.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
        match parts.as_slice() {
            [name] if dirs.synced => name.ends_with(".toml") && !name.starts_with('.'),
            [name] => name == "settings.toml",
            [dir, name] => {
                let kind = (dir == "themes" && name.ends_with(".toml")) || (dir == "lang" && name.ends_with(".po"));
                kind && !name.starts_with('.')
            }
            _ => false,
        }
    }
}

/// [`ConfigStore::tools_dir`].
fn tools_dir(dirs: &Dirs) -> PathBuf {
    let tools = dirs.local.join("tools");
    if dirs.portable { tools.join(format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)) } else { tools }
}

/// The settings and themes in folder `dir`. `machines`: it is a sync folder (whose computers
/// these are); its conflicting copies are listed and left out of the themes.
fn read_folder(dir: &Path, machines: Option<&[String]>) -> ConfigFiles {
    let mut files = ConfigFiles::default();
    let settings = dir.join("settings.toml");
    match read_text(&settings) {
        Ok(text) => files.settings = Some(text),
        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
        Err(err) => files.warnings.push(Warning::new("settings.toml", format!("cannot read: {err}"))),
    }
    let conflict_of = |name: &str| machines.and_then(|m| sync_folder::conflict_of(name, m));
    if let (Some(entries), Some(settings)) = (machines.and_then(|_| std::fs::read_dir(dir).ok()), &files.settings) {
        for name in entries.filter_map(Result::ok).filter_map(|e| e.file_name().into_string().ok()) {
            let Some(copy) = conflict_of(&name).filter(|c| c.original == "settings.toml") else { continue };
            // Only a number in its name (`settings (1).toml`): a copy only when it differs.
            if !copy.numbered || read_text(&dir.join(&name)).is_ok_and(|text| text != *settings) {
                files.conflicts.push(name);
            }
        }
    }

    files.languages = read_languages(&dir.join("lang"), &mut files.warnings);
    let Ok(entries) = std::fs::read_dir(dir.join("themes")) else { return note_conflicts(files) };
    let names: Vec<PathBuf> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
    let stems: Vec<String> =
        names.iter().filter_map(|p| p.file_name()).map(|n| n.to_string_lossy().to_lowercase()).collect();
    for path in names {
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else { continue };
        let file = format!("{stem}.toml");
        // A theme's copy only by a service's mark: `Dracula 2.toml` may be a theme of its own.
        let copy = conflict_of(&file).filter(|c| !c.numbered && stems.contains(&c.original.to_lowercase()));
        if copy.is_some() {
            files.conflicts.push(format!("themes/{file}"));
            continue;
        }
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
    note_conflicts(files)
}

/// At most this many problems of one lang folder file are listed.
const LANGUAGE_WARNINGS: usize = 10;

/// The lang folder's `<code>.po` files read into layers (here, off the UI thread), with their
/// problems as warnings: syntax and placeholder errors by line, and how many texts are missing.
fn read_languages(dir: &Path, warnings: &mut Vec<Warning>) -> Vec<(String, Arc<gezik_i18n::Layer>)> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut paths: Vec<PathBuf> =
        entries.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "po")).collect();
    paths.sort();
    let mut out = Vec::new();
    for path in paths {
        let Some(code) = path.file_stem().and_then(|s| s.to_str()).filter(|c| gezik_i18n::lang::valid_code(c)) else {
            continue;
        };
        let file = format!("lang/{code}.po");
        if std::fs::metadata(&path).is_ok_and(|m| m.len() > gezik_i18n::po::MAX_BYTES as u64) {
            warnings.push(Warning::new(file, "larger than 4 MB; not read"));
            continue;
        }
        let text = match read_text(&path) {
            Ok(text) => text,
            Err(err) => {
                warnings.push(Warning::new(file, format!("cannot read: {err}")));
                continue;
            }
        };
        let (layer, report) = gezik_i18n::check::load(&text, code, gezik_i18n::keys());
        for issue in report.errors.iter().take(LANGUAGE_WARNINGS) {
            warnings.push(Warning { file: file.clone(), line: Some(issue.line), message: issue.message.clone() });
        }
        if let Some(more) = report.errors.len().checked_sub(LANGUAGE_WARNINGS).filter(|&n| n > 0) {
            warnings.push(Warning::new(file.clone(), format!("{more} more problems")));
        }
        if report.missing > 0 {
            warnings.push(Warning::new(file, format!("{} of {} texts missing", report.missing, report.total)));
        }
        out.extend(layer.map(|layer| (code.to_owned(), Arc::new(layer))));
    }
    out
}

/// Each conflicting copy said in the status bar (spec 14 §8.3).
fn note_conflicts(mut files: ConfigFiles) -> ConfigFiles {
    files.conflicts.sort();
    for name in &files.conflicts {
        let of = if name.starts_with("themes/") { "a theme" } else { "settings.toml" };
        files.warnings.push(Warning::new(
            name.clone(),
            format!("a conflicting copy of {of} — Review it in Settings > Sync and Backup"),
        ));
    }
    files
}

/// Keeps the sync folder's settings.toml and themes in `cache` (spec 14 §8.4): only files that
/// changed are written. A theme keeps its file's real name (`Dracula 2.toml`, not its id): a
/// case-sensitive system finds it under that name, and leaving the folder copies it so. A failure
/// is not said (the folder itself was read).
// shortcut: the lang folder is not kept, so while the sync folder is away a dropped language
// shows English; copy `lang/*.po` here too if someone misses it.
fn keep_copy(shared: &Path, cache: &Path, files: &ConfigFiles) {
    let keep = |path: &Path, text: &str| {
        if read_text(path).ok().as_deref() != Some(text) {
            let _ = path.parent().map(std::fs::create_dir_all);
            let _ = write_atomic(path, text);
        }
    };
    if let Some(text) = &files.settings {
        keep(&cache.join("settings.toml"), text);
    }
    let themes = cache.join("themes");
    let mut kept = Vec::new();
    for entry in std::fs::read_dir(shared.join("themes")).into_iter().flatten().filter_map(Result::ok) {
        let path = entry.path();
        let id = path.file_stem().map(|s| s.to_string_lossy().to_lowercase());
        let text = id.filter(|_| path.extension().is_some_and(|e| e == "toml")).and_then(|id| files.themes.get(&id));
        if let Some(text) = text {
            kept.push((entry.file_name(), text));
        }
    }
    // First what the folder lost, or an older copy named by the id (`dracula 2.toml`): on a
    // case-insensitive system that one is the same file, written again under its real name.
    for entry in std::fs::read_dir(&themes).into_iter().flatten().filter_map(Result::ok) {
        if !kept.iter().any(|(name, _)| *name == entry.file_name()) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    for (name, text) in kept {
        keep(&themes.join(name), text);
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
    let local = files.local_settings.as_deref();
    let settings = files
        .settings
        .as_deref()
        .map(|text| Settings::parse_layered("settings.toml", text, local, &mut warnings))
        .unwrap_or_default();

    let codes: Vec<&str> = gezik_i18n::builtins()
        .iter()
        .map(|b| b.code)
        .chain(files.languages.iter().map(|(code, _)| code.as_str()))
        .collect();
    if !gezik_i18n::lang::known(&settings.language, &codes) {
        let message = format!("language \"{}\": no such language, using the system's", settings.language);
        warnings.push(Warning::new("settings.toml", message));
    }

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
        let path = store.shared_dir().join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn one_folder_keeps_every_file_where_it_was_and_portable_tools_go_by_system() {
        let store = store("one-folder");
        assert_eq!(store.shared_dir(), store.local_dir());
        assert_eq!(store.tools_dir(), store.local_dir().join("tools"), "as before 14e: no move");
        assert!(!store.is_portable());
        store.save_state(&State::default()).unwrap();
        store.save_views(&ViewMemory::default()).unwrap();
        store.save_pinned(&[PinEntry::plain("/a")]).unwrap();
        for name in ["state.toml", "views.toml", "settings.toml"] {
            assert!(store.shared_dir().join(name).is_file(), "{name}");
        }
        let data = PathBuf::from("/usb/gezik-data");
        let portable = ConfigStore::with_dirs(Dirs { portable: true, ..Dirs::one(data.clone()) });
        let system = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
        assert_eq!(portable.tools_dir(), data.join("tools").join(system));
    }

    #[test]
    fn a_split_store_puts_each_file_in_its_folder() {
        let root = crate::test_dir("split");
        let dirs = Dirs { shared: root.join("shared"), local: root.join("local"), portable: false, synced: false };
        let store = ConfigStore::with_dirs(dirs);
        store.ensure_initialized().unwrap();
        store.save_state(&State::default()).unwrap();
        store.save_views(&ViewMemory::default()).unwrap();
        assert!(root.join("shared/settings.toml").is_file() && root.join("shared/themes/example.toml").is_file());
        assert!(root.join("local/state.toml").is_file() && root.join("local/views.toml").is_file());
        assert!(!root.join("shared/state.toml").exists() && !root.join("local/settings.toml").exists());
        assert!(store.is_config_file(&root.join("shared/settings.toml")));
        assert!(!store.is_config_file(&root.join("local/settings.toml")));
    }

    #[test]
    fn the_templates_folder_comes_with_the_first_run_only() {
        let store = fresh_store("templates");
        store.ensure_initialized().unwrap();
        assert!(store.templates_dir().is_dir());
        std::fs::remove_dir(store.templates_dir()).unwrap();
        store.ensure_initialized().unwrap();
        assert!(!store.templates_dir().exists(), "a deleted templates folder does not come back");
    }

    #[test]
    fn recognizes_template_paths() {
        let store = store("templates-watch");
        let root = store.shared_dir();
        assert!(store.is_template_path(&root.join("templates")));
        assert!(store.is_template_path(&root.join("templates").join("Report.docx")));
        assert!(store.is_template_path(&root.join("templates").join("Project").join("a.txt")));
        assert!(!store.is_template_path(&root.join("settings.toml")));
        assert!(!store.is_template_path(&root.join("themes").join("templates")));
        assert!(!store.is_config_file(&root.join("templates").join("x.toml")), "no settings reload for a template");
    }

    #[test]
    fn first_run_creates_starter_files_once() {
        let store = fresh_store("init");
        store.ensure_initialized().unwrap();
        assert!(store.shared_dir().join("settings.toml").is_file());
        assert!(store.shared_dir().join("themes/example.toml").is_file());

        std::fs::remove_file(store.shared_dir().join("themes/example.toml")).unwrap();
        store.ensure_initialized().unwrap();
        assert!(!store.shared_dir().join("themes/example.toml").exists(), "deleted starter file came back");

        write(&store, "settings.toml", "theme = \"light\"\n");
        store.ensure_initialized().unwrap();
        assert_eq!(std::fs::read_to_string(store.shared_dir().join("settings.toml")).unwrap(), "theme = \"light\"\n");
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
        assert_eq!(theme.metrics.row_height, 20.0);
        assert_eq!(theme.metrics.inset, 4.0);
    }

    #[test]
    fn state_round_trips_through_disk() {
        let store = store("state");
        assert_eq!(store.load_state(), State::default());
        let state = State {
            window: Some(WindowState { width: 1000, height: 700, x: Some(10), y: Some(20), maximized: false }),
            sidebar_width: None,
            ..State::default()
        };
        store.save_state(&state).unwrap();
        assert_eq!(store.load_state(), state);
        // Read again from the file by a new store; a change written by the writer thread.
        let again = ConfigStore::new(store.shared_dir().to_path_buf());
        assert_eq!(again.load_state(), state);
        again.update_state(|state| state.preview_open = true);
        again.flush_state();
        assert!(ConfigStore::new(store.shared_dir().to_path_buf()).load_state().preview_open);
        let names: Vec<_> = std::fs::read_dir(store.shared_dir()).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names, ["state.toml"]);
    }

    #[test]
    fn recognizes_config_files() {
        let store = store("watch");
        let root = store.shared_dir();
        assert!(store.is_config_file(&root.join("settings.toml")));
        assert!(store.is_config_file(&root.join("themes").join("nord.toml")));
        assert!(!store.is_config_file(&root.join("state.toml")));
        assert!(!store.is_config_file(&root.join(".settings.toml.tmp")));
        assert!(!store.is_config_file(&root.join("themes").join(".nord.toml.tmp")));
        assert!(!store.is_config_file(&root.join("themes").join(".nord.toml.swp")));
        assert!(!store.is_config_file(&root.join("themes").join("nord.toml~")));
        assert!(!store.is_config_file(&root.join("themes").join("old").join("nord.toml")));
        assert!(!store.is_config_file(Path::new("/elsewhere/settings.toml")));
        assert!(store.is_config_file(&root.join("lang").join("pl.po")));
        assert!(!store.is_config_file(&root.join("lang").join(".pl.po.swp")));
        assert!(!store.is_config_file(&root.join("lang").join("pl.toml")));
    }

    #[test]
    fn languages_are_read_with_their_problems() {
        let store = store("lang");
        // A text of the template (the language row's), translated without its placeholder.
        let pl = "msgid \"\"\nmsgstr \"X-Gezik-Name: Polski\\n\"\n\nmsgid \"{} % translated\"\nmsgstr \"procent\"\n";
        write(&store, "lang/pl.po", pl);
        write(&store, "lang/xx.po", "msgid \"a\"\nmsgstr b\n");
        write(&store, "lang/notes.txt", "");
        let files = store.read_files();
        let codes: Vec<_> = files.languages.iter().map(|(code, layer)| (code.as_str(), layer.name.as_str())).collect();
        assert_eq!(codes, [("pl", "Polski")], "a file that does not read is left out");
        let said: Vec<String> = files.warnings.iter().map(ToString::to_string).collect();
        let total = gezik_i18n::keys().len();
        assert_eq!(
            said,
            [
                "lang/pl.po line 4: \"{0}\" missing in the translation".to_owned(),
                format!("lang/pl.po: {total} of {total} texts missing"),
                "lang/xx.po line 2: expected a string in double quotes".to_owned(),
            ]
        );
        write(&store, "settings.toml", "language = \"pl\"\n");
        assert_eq!(resolve(&store.read_files(), true).warnings.len(), 3, "pl is known");
        write(&store, "settings.toml", "language = \"de\"\n");
        let loaded = resolve(&store.read_files(), true);
        let last = loaded.warnings.last().unwrap().to_string();
        assert_eq!(last, "settings.toml: language \"de\": no such language, using the system's");
    }

    #[test]
    fn save_pinned_keeps_user_comments() {
        let store = store("pin-save");
        write(&store, "settings.toml", "# mine\ntheme = \"dark\"\n");
        store.save_pinned(&[PinEntry::plain("/a")]).unwrap();
        let text = std::fs::read_to_string(store.shared_dir().join("settings.toml")).unwrap();
        assert!(text.contains("# mine"));
        let loaded = resolve(&store.read_files(), true);
        assert_eq!(crate::pins::paths(&loaded.settings.pinned), ["/a"]);
    }

    #[test]
    fn save_pinned_creates_missing_file_from_template() {
        let store = store("pin-create");
        store.save_pinned(&[PinEntry::plain("/a")]).unwrap();
        let loaded = resolve(&store.read_files(), true);
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        assert_eq!(crate::pins::paths(&loaded.settings.pinned), ["/a"]);
    }

    #[test]
    fn save_pinned_refuses_a_broken_file() {
        let store = store("pin-broken");
        write(&store, "settings.toml", "theme = \n");
        let err = store.save_pinned(&[PinEntry::plain("/a")]).unwrap_err();
        assert!(err.message.starts_with("Fix settings.toml first"), "{}", err.message);
        assert_eq!(std::fs::read_to_string(store.shared_dir().join("settings.toml")).unwrap(), "theme = \n");
    }

    #[test]
    fn save_pinned_handles_bom() {
        let store = store("pin-bom");
        write(&store, "settings.toml", "\u{feff}# mine\ntheme = \"dark\"\n");
        store.save_pinned(&[PinEntry::plain("/a")]).unwrap();
        // Read raw file to verify no BOM is written
        let text = std::fs::read_to_string(store.shared_dir().join("settings.toml")).unwrap();
        assert!(!text.starts_with('\u{feff}'), "BOM should not be written");
        assert!(text.contains("# mine"), "user comment should be kept");
        let loaded = resolve(&store.read_files(), true);
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        assert_eq!(crate::pins::paths(&loaded.settings.pinned), ["/a"]);
    }

    #[test]
    fn save_pinned_refuses_non_array_pinned() {
        let store = store("pin-non-array");
        write(&store, "settings.toml", "pinned = \"not a list\"\n");
        let err = store.save_pinned(&[PinEntry::plain("/a")]).unwrap_err();
        assert!(err.message.starts_with("Fix settings.toml first"), "{}", err.message);
        assert!(err.message.contains("pinned must be a list"), "{}", err.message);
        // File must not be modified
        assert_eq!(
            std::fs::read_to_string(store.shared_dir().join("settings.toml")).unwrap(),
            "pinned = \"not a list\"\n"
        );
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
    fn a_store_kept_unwritten_writes_no_state_nor_views() {
        use gezik_core::view::ViewSettings;
        let store = store("views-unwritten");
        write(&store, "views.toml", "");
        write(&store, "state.toml", "");
        store.clone().keep_state_unwritten();
        let mut memory = ViewMemory::default();
        memory.set("/pics", ViewSettings::default());
        store.write_views(&memory);
        store.flush_views();
        store.save_views(&memory).unwrap();
        store.update_state(|state| state.preview_open = true);
        store.flush_state();
        assert_eq!(std::fs::read_to_string(store.shared_dir().join("views.toml")).unwrap(), "");
        assert_eq!(std::fs::read_to_string(store.shared_dir().join("state.toml")).unwrap(), "");
        // settings.toml is still written (merged), as for the first window.
        store.save_pinned(&[PinEntry::plain("/a")]).unwrap();
        assert!(std::fs::read_to_string(store.shared_dir().join("settings.toml")).unwrap().contains("/a"));
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
        let text = std::fs::read_to_string(store.shared_dir().join("settings.toml")).unwrap();
        assert!(text.contains("# mine"));
        assert_eq!(resolve(&store.read_files(), true).settings.view.view.mode, ViewMode::Grid);
        write(&store, "settings.toml", "theme = \n");
        let err = store.save_view_defaults(&ViewSettings::default()).unwrap_err();
        assert!(err.message.starts_with("Fix settings.toml first"), "{}", err.message);
    }

    #[test]
    fn views_file_is_not_a_config_file() {
        let store = store("views-watch");
        assert!(!store.is_config_file(&store.shared_dir().join("views.toml")));
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
        let text = std::fs::read_to_string(store.shared_dir().join("settings.toml")).unwrap();
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

    #[test]
    fn tab_sets_are_written_into_settings() {
        use crate::settings::TabSet;
        let store = store("save-tab-sets");
        write(&store, "settings.toml", "# mine\ntheme = \"dark\"\n");
        let set =
            TabSet { name: "Release".into(), tabs: vec!["{downloads}".into(), "drives".into()], right: Vec::new() };
        store.save_tab_sets(std::slice::from_ref(&set)).unwrap();
        let text = std::fs::read_to_string(store.shared_dir().join("settings.toml")).unwrap();
        assert!(text.contains("# mine"));
        assert_eq!(resolve(&store.read_files(), true).settings.tab_sets, [set]);
        store.save_tab_sets(&[]).unwrap();
        assert!(resolve(&store.read_files(), true).settings.tab_sets.is_empty());
    }

    /// A store whose settings are in sync folder `root/Dropbox/Gezik`, its own files in `root/local`.
    fn synced_store(root: &Path) -> ConfigStore {
        let dirs =
            Dirs { shared: root.join("Dropbox/Gezik"), local: root.join("local"), portable: false, synced: true };
        ConfigStore::with_dirs(dirs)
    }

    #[test]
    fn a_sync_folder_is_read_with_this_computers_layer_and_kept_as_a_last_copy() {
        let root = crate::test_dir("sync-read");
        let store = synced_store(&root);
        store.ensure_initialized().unwrap();
        assert!(!root.join("Dropbox").exists(), "a sync folder is never made at start");
        assert!(root.join("local").is_dir());
        write(&store, "settings.toml", "theme = \"nord\"\n[layout]\ndensity = \"compact\"\n");
        write(&store, "themes/nord.toml", "[colors]\naccent = \"#88c0d0\"\n");
        std::fs::write(root.join("local").join(LOCAL_SETTINGS), "[layout]\ndensity = \"comfortable\"\n").unwrap();
        let files = store.read_files();
        assert!(!files.offline && files.conflicts.is_empty());
        let loaded = resolve(&files, true);
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        assert_eq!(loaded.settings.density, Density::Comfortable, "this computer's value wins");
        assert_eq!(loaded.theme.unwrap().id, "nord", "the rest is the shared file's");
        let cache = root.join("local").join(CACHE);
        assert_eq!(std::fs::read_to_string(cache.join("settings.toml")).unwrap(), files.settings.unwrap());
        assert!(cache.join("themes/nord.toml").is_file());

        // The folder is gone (Dropbox not running): the last copy, said, and nothing made.
        std::fs::remove_dir_all(root.join("Dropbox")).unwrap();
        let files = store.read_files();
        assert!(files.offline);
        assert!(files.warnings.iter().any(|w| w.message == OFFLINE_NOTE), "{:?}", files.warnings);
        let loaded = resolve(&files, true);
        assert_eq!(loaded.theme.unwrap().id, "nord");
        assert_eq!(loaded.settings.density, Density::Comfortable);
        let pin = store.save_pinned(&[PinEntry::plain("/a")]).unwrap_err();
        assert_eq!(pin.message, crate::settings_writer::NOT_SAVED);
        assert!(!root.join("Dropbox").exists());
        assert_eq!(store.unsaved(), 1);

        // A theme the folder lost leaves the copy too.
        write(&store, "settings.toml", "theme = \"dark\"\n");
        store.read_files();
        assert!(!cache.join("themes/nord.toml").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// `Dracula 2.toml` is theme "dracula 2" and its last copy keeps the real name (a
    /// case-sensitive system finds it under that name only; leaving the folder copies it so).
    #[test]
    fn a_theme_with_capitals_is_kept_under_its_real_name() {
        let root = crate::test_dir("sync-theme-name");
        let store = synced_store(&root);
        write(
            &store,
            "settings.toml",
            "theme = \"dracula 2\"
",
        );
        write(
            &store,
            "themes/Dracula 2.toml",
            "[colors]
accent = \"#bd93f9\"
",
        );
        let cache = root.join("local").join(CACHE).join("themes");
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(cache.join("dracula 2.toml"), "an older copy named by its id").unwrap();
        let files = store.read_files();
        assert_eq!(files.themes.keys().collect::<Vec<_>>(), ["dracula 2"]);
        assert_eq!(resolve(&files, true).theme.unwrap().id, "dracula 2");
        let names: Vec<String> =
            std::fs::read_dir(&cache).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["Dracula 2.toml"], "the real name, and the copy named by the id is gone");

        std::fs::remove_dir_all(root.join("Dropbox")).unwrap();
        let files = store.read_files();
        assert!(files.offline);
        assert_eq!(resolve(&files, true).theme.unwrap().id, "dracula 2", "the last copy still has it");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn conflicting_copies_are_listed_and_said_and_are_not_themes() {
        let root = crate::test_dir("sync-conflicts");
        let store = synced_store(&root);
        write(&store, "settings.toml", "theme = \"dark\"\n");
        write(&store, "settings (Ann's conflicted copy 2026-10-12).toml", "theme = \"nord\"\n");
        write(&store, "settings-LAPTOP.toml", "theme = \"nord\"\n");
        write(&store, "settings-old.toml", "");
        write(&store, "themes/nord.toml", "");
        write(&store, "themes/nord (Ann's conflicted copy 2026-10-12).toml", "");
        write(&store, "themes/nord (1).toml", "");
        write(&store, "themes/Dracula.toml", "");
        write(&store, "themes/Dracula 2.toml", "");
        // Numbered only: a copy of settings.toml when it differs, never a theme's.
        write(&store, "settings (1).toml", "theme = \"dark\"\n");
        write(&store, "settings 2.toml", "theme = \"light\"\n");
        std::fs::create_dir_all(store.shared_dir().join("machines")).unwrap();
        let files = store.read_files();
        assert_eq!(
            files.conflicts,
            [
                "settings (Ann's conflicted copy 2026-10-12).toml",
                "settings 2.toml",
                "themes/nord (Ann's conflicted copy 2026-10-12).toml"
            ]
        );
        for theme in ["nord (1)", "dracula 2", "dracula"] {
            assert!(files.themes.contains_key(theme), "{theme}: a theme of its own");
        }
        let said: Vec<&Warning> = files.warnings.iter().filter(|w| w.message.contains("conflicting copy")).collect();
        assert_eq!(said.len(), 3, "{:?}", files.warnings);
        // A computer that joined the folder: OneDrive's copy named by it is one too.
        crate::sync_folder::add_machine(&store.shared_dir(), "LAPTOP").unwrap();
        assert_eq!(store.read_files().conflicts.len(), 4);
        assert!(store.is_config_file(&store.shared_dir().join("settings (1).toml")));
        assert!(store.is_config_file(&store.shared_dir()), "the folder coming back");
        assert!(store.is_config_file(&root.join("local").join(LOCAL_SETTINGS)));
        assert!(!store.is_config_file(&root.join("local").join("state.toml")));
        assert!(!store.is_config_file(&store.shared_dir().join("machines").join("LAPTOP")));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_shared_folder_moves_and_comes_back() {
        let root = crate::test_dir("sync-move");
        let store = ConfigStore::new(root.join("local"));
        store.ensure_initialized().unwrap();
        let clone = store.clone();
        store.set_shared(Some(root.join("Sync")));
        assert!(clone.is_synced() && clone.shared_dir() == root.join("Sync"), "every clone follows");
        assert_eq!(clone.templates_dir(), root.join("Sync").join("templates"));
        assert_eq!(clone.local_dir(), root.join("local"));
        // Not there: the change waits; the settings.toml left behind is not written.
        assert!(clone.save_pinned(&[PinEntry::plain("/a")]).is_err());
        store.set_shared(None);
        assert!(!clone.is_synced() && clone.shared_dir() == root.join("local"));
        // Without a sync folder settings.local.toml is not read.
        std::fs::write(root.join("local").join(LOCAL_SETTINGS), "theme = \"nord\"\n").unwrap();
        assert!(store.read_files().local_settings.is_none());
        assert!(!store.is_config_file(&root.join("local").join(LOCAL_SETTINGS)));
        let _ = std::fs::remove_dir_all(&root);
    }
}
