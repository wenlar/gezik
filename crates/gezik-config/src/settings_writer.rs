//! `settings.toml` written by one thread of its own: every change the UI makes to it (the
//! saved filters, the tab sets, the pinned folders, the rename rule sets, the `[view]`
//! defaults, the view options) goes through one queue, so the UI thread never touches the
//! file, changes are written in the order they were made, and two of them never read, edit
//! and write the file over each other. With a sync folder (spec 14 §8) another computer's
//! change may land between the read and the write: the edit is then made again on the new text,
//! and what cannot be written (the folder is gone, the file keeps changing) is kept in memory
//! and written at the next reload.

use std::fmt;
use std::io;
use std::path::Path;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gezik_core::view::ViewSettings;

use crate::Warning;
use crate::lock;
use crate::paths::{Dirs, write_atomic};
use crate::pins::PinEntry;
use crate::settings::{RenamePreset, SavedFilter, SavedSearch, TabSet, ViewOption};
use crate::settings_edit::{
    with_command_shortcut, with_filters, with_panes_confirm, with_pinned, with_rename_presets, with_searches,
    with_sync_profiles, with_system, with_tab_sets, with_tag_added, with_tag_list_edit, with_tags, with_value,
    with_view_defaults, with_view_option,
};
use crate::store::{SETTINGS_TEMPLATE, read_text};

/// How long a flush waits for the writer.
const FLUSH_WAIT: Duration = Duration::from_secs(5);

/// One change to `settings.toml`; everything else in the file (comments too) stays.
#[derive(Debug, Clone, PartialEq)]
pub enum SettingsChange {
    /// The pinned folders (`pinned`).
    Pinned(Vec<PinEntry>),
    /// The `[view]` defaults ("Apply to all folders").
    ViewDefaults(ViewSettings),
    /// One `[view]` option (the View menu, toggle-hidden).
    ViewOption(ViewOption),
    /// The saved rename rule sets.
    RenamePresets(Vec<RenamePreset>),
    /// The saved filters (`[[filters]]`).
    Filters(Vec<SavedFilter>),
    /// The tab sets (`[[tab-sets]]`).
    TabSets(Vec<TabSet>),
    /// The saved searches (`[[searches]]`).
    Searches(Vec<SavedSearch>),
    /// The saved folder syncs (`[[sync-profiles]]`).
    SyncProfiles(Vec<crate::sync_profiles::SyncProfile>),
    /// A `[system]` value (the tray icon, the global shortcut).
    System(SystemValue),
    /// `[panes] confirm` (the F5/F6 question's Don't Ask Again).
    PanesConfirm(bool),
    /// `[tags] list` (the tag picker's Create "…").
    /// The tag added at the end; the whole list in effect, written only when the file has none.
    TagAdded(gezik_core::meta::Tag, Vec<gezik_core::meta::Tag>),
    /// `[tags] dots` (View ▸ Tag dots).
    TagDots(crate::tags::Dots),
    /// `[tags] list` edited from the sidebar (spec 12 §7.3), and the list in effect before it
    /// (written whole when the file has none).
    TagList(crate::tags::TagListEdit, Vec<gezik_core::meta::Tag>),
    /// One value from the settings window (spec 14 §3.2): `key` in `[table]` (`None`: at the
    /// top), as TOML text; `None` takes the key out.
    Value { table: Option<String>, key: String, value: Option<String> },
    /// The key of the `[[commands]]` entry named `name` (the settings window, spec 14 §5);
    /// `""` is none.
    CommandShortcut { name: String, shortcut: String },
    /// "Only on This Computer" (spec 14 §8.2): `key` in `[table]` written to settings.local.toml
    /// (`None`: taken out of it, the shared value again).
    Local { table: Option<String>, key: String, value: Option<String> },
    /// A Review's "Other copy" (spec 14 §8.3): the key at `path` as `other`, a sync service's
    /// conflicting copy of settings.toml, has it.
    FromCopy { path: Vec<String>, other: String },
}

/// A `[system]` value System Integration writes (9b9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SystemValue {
    Tray(bool),
    /// Gezik's text for the shortcut (`hotkey_text`), or "" for off.
    Hotkey(String),
}

impl SettingsChange {
    fn apply(&self, text: &str) -> Result<String, String> {
        let edited = match self {
            SettingsChange::Pinned(pinned) => with_pinned(text, pinned),
            SettingsChange::ViewDefaults(view) => with_view_defaults(text, view),
            SettingsChange::ViewOption(option) => with_view_option(text, *option),
            SettingsChange::RenamePresets(presets) => with_rename_presets(text, presets),
            SettingsChange::Filters(filters) => with_filters(text, filters),
            SettingsChange::TabSets(sets) => with_tab_sets(text, sets),
            SettingsChange::Searches(searches) => with_searches(text, searches),
            SettingsChange::SyncProfiles(profiles) => with_sync_profiles(text, profiles),
            SettingsChange::System(SystemValue::Tray(on)) => with_system(text, "tray", (*on).into()),
            SettingsChange::System(SystemValue::Hotkey(chord)) => with_system(text, "hotkey", chord.as_str().into()),
            SettingsChange::PanesConfirm(on) => with_panes_confirm(text, *on),
            SettingsChange::TagAdded(tag, list) => with_tag_added(text, tag, list),
            SettingsChange::TagDots(dots) => with_tags(text, "dots", crate::tags::dots_value(*dots)),
            SettingsChange::TagList(edit, list) => with_tag_list_edit(text, edit, list),
            SettingsChange::Value { table, key, value } => with_value(text, table.as_deref(), key, value.as_deref()),
            SettingsChange::CommandShortcut { name, shortcut } => with_command_shortcut(text, name, shortcut),
            SettingsChange::Local { table, key, value } => with_value(text, table.as_deref(), key, value.as_deref()),
            SettingsChange::FromCopy { path, other } => crate::sync_folder::with_item_from(text, other, path),
        }?;
        // toml_edit writes every line break as \n: a file written with \r\n (Notepad) keeps
        // them, so only the changed lines change.
        Ok(if text.contains("\r\n") { edited.replace("\r\n", "\n").replace('\n', "\r\n") } else { edited })
    }
}

/// Told the result of a change, on the writer thread.
pub type Done = Box<dyn FnOnce(Result<(), Warning>) + Send>;

enum Job {
    Write(SettingsChange, Done),
    /// Answered once every change sent before it is written.
    Flush(Sender<()>),
}

/// What the writer and its thread share.
struct Inner {
    /// Where settings.toml is (the store's: a sync folder set or left moves it).
    dirs: Arc<Mutex<Dirs>>,
    /// Held while the file is read, edited and written: one change at a time, whoever makes it.
    writing: Mutex<()>,
    /// Changes not written yet: the sync folder was not there, or the file kept changing.
    unsaved: Mutex<Vec<SettingsChange>>,
}

pub(crate) struct SettingsWriter {
    inner: Arc<Inner>,
    /// The queue to the writer thread, once it was started.
    queue: Mutex<Option<Sender<Job>>>,
}

impl fmt::Debug for SettingsWriter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SettingsWriter").field("dirs", &*lock(&self.inner.dirs)).finish_non_exhaustive()
    }
}

impl SettingsWriter {
    pub fn new(dirs: Arc<Mutex<Dirs>>) -> SettingsWriter {
        let inner = Inner { dirs, writing: Mutex::new(()), unsaved: Mutex::new(Vec::new()) };
        SettingsWriter { inner: Arc::new(inner), queue: Mutex::new(None) }
    }

    /// Makes `change` now, on this thread (tests, and the writer thread itself).
    pub fn save(&self, change: &SettingsChange) -> Result<(), Warning> {
        edit(&self.inner, change)
    }

    /// Hands the changes not written yet to the writer thread again (a reload found the sync
    /// folder there). Those that fail again are kept again.
    pub fn retry_unsaved(&self) {
        let unsaved = std::mem::take(&mut *lock(&self.inner.unsaved));
        for change in unsaved {
            self.send(change, Box::new(|_| {}));
        }
    }

    /// How many changes are not written yet.
    pub fn unsaved(&self) -> usize {
        lock(&self.inner.unsaved).len()
    }

    /// Has the writer thread make `change` after every change sent before it; `done` gets the
    /// result on that thread.
    pub fn send(&self, change: SettingsChange, done: Done) {
        let mut queue = lock(&self.queue);
        if queue.is_none() {
            *queue = self.start();
        }
        let job = Job::Write(change, done);
        let unsent = match queue.as_ref() {
            Some(sender) => sender.send(job).err().map(|err| err.0),
            None => Some(job),
        };
        drop(queue);
        // No thread could be started (or it stopped): made here, still one at a time.
        if let Some(Job::Write(change, done)) = unsent {
            done(self.save(&change));
        }
    }

    /// Waits (up to 5 s) until every change sent so far is written: before quitting.
    pub fn flush(&self) {
        let queue = lock(&self.queue);
        let Some(sender) = queue.as_ref() else { return };
        let (done, written) = mpsc::channel();
        if sender.send(Job::Flush(done)).is_ok() {
            drop(queue);
            let _ = written.recv_timeout(FLUSH_WAIT);
        }
    }

    /// The writer thread: the jobs one after the other, in the order they came. It ends with
    /// the writer (whose sender is the only one).
    fn start(&self) -> Option<Sender<Job>> {
        let (sender, jobs) = mpsc::channel::<Job>();
        let inner = self.inner.clone();
        std::thread::Builder::new()
            .name("gezik-settings".into())
            .spawn(move || {
                for job in jobs {
                    match job {
                        Job::Write(change, done) => done(edit(&inner, &change)),
                        Job::Flush(done) => {
                            let _ = done.send(());
                        }
                    }
                }
            })
            .ok()
            .map(|_| sender)
    }
}

/// Why [`rewrite`] wrote nothing.
#[derive(Debug)]
pub enum Failed {
    Read(io::Error),
    /// The edit cannot be made (the file does not read).
    Edit(String),
    Write(io::Error),
    /// The file changed between each read and write.
    Changing,
}

impl Failed {
    fn warning(self, file: &str) -> Warning {
        let message = match self {
            Failed::Read(err) => format!("cannot read: {err}"),
            Failed::Edit(err) => format!("Fix {file} first ({})", err.lines().next().unwrap_or_default()),
            Failed::Write(err) => format!("cannot write: {err}"),
            Failed::Changing => KEPT_CHANGING.to_owned(),
        };
        Warning::new(file, message)
    }
}

/// How many times an edit is made again on a file that changed under it (spec 14 §8.3).
pub const RETRIES: usize = 3;
/// Said for a change kept in memory while the sync folder is not there.
pub const NOT_SAVED: &str = "the settings folder is not available: not saved yet (written when it is back)";
const KEPT_CHANGING: &str =
    "kept changing while Gezik wrote it (a sync service?): the change is kept and written at the next reload";

/// Whether `warning` says a change was kept to be written later (it is not lost: what shows it
/// may keep showing it).
pub fn kept(warning: &Warning) -> bool {
    warning.message == NOT_SAVED || warning.message == KEPT_CHANGING
}

/// The file's size and time, to tell whether it changed since.
fn stamp(path: &Path) -> Option<(u64, Option<std::time::SystemTime>)> {
    std::fs::metadata(path).ok().map(|meta| (meta.len(), meta.modified().ok()))
}

/// Rewrites `path` with `edit` made to its text (`missing`: the text when there is no file). If
/// the file changed between the read and the write (another computer's change synced in), the
/// edit is made again on the new text, up to [`RETRIES`] times: an edit is made key by key, so
/// the other change stays. `between` runs before each write (tests stand for the syncer there).
pub fn rewrite(
    path: &Path,
    missing: &str,
    edit: &dyn Fn(&str) -> Result<String, String>,
    between: &dyn Fn(),
) -> Result<(), Failed> {
    for _ in 0..=RETRIES {
        let before = stamp(path);
        let text = match read_text(path) {
            Ok(text) => text,
            Err(err) if err.kind() == io::ErrorKind::NotFound => missing.to_owned(),
            Err(err) => return Err(Failed::Read(err)),
        };
        let edited = edit(&text).map_err(Failed::Edit)?;
        between();
        if stamp(path) != before {
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(Failed::Write)?;
        }
        return write_atomic(path, &edited).map_err(Failed::Write);
    }
    Err(Failed::Changing)
}

/// Whether `change` goes to settings.local.toml (`local`) while a sync folder is used: "Only on
/// This Computer", `[system]` (a tray on one computer is not the others'; decision 12), and a
/// value already there.
fn goes_local(change: &SettingsChange, local: &Path) -> bool {
    match change {
        SettingsChange::Local { .. } | SettingsChange::System(_) => true,
        SettingsChange::Value { table, key, .. } => {
            let Ok(text) = read_text(local) else { return false };
            let Ok(file) = text.parse::<toml::Table>() else { return false };
            let holder = match table {
                None => Some(&file),
                Some(name) => file.get(name).and_then(toml::Value::as_table),
            };
            holder.is_some_and(|holder| holder.contains_key(key))
        }
        _ => false,
    }
}

/// Applies `change` to settings.toml (or, with a sync folder, settings.local.toml). Creates the
/// file from the template if it does not exist; refuses to touch a broken file. With a sync
/// folder that is not there, or a file that keeps changing, the change is kept for later.
fn edit(inner: &Inner, change: &SettingsChange) -> Result<(), Warning> {
    edit_with(inner, change, &|| {})
}

fn edit_with(inner: &Inner, change: &SettingsChange, between: &dyn Fn()) -> Result<(), Warning> {
    let _turn = lock(&inner.writing);
    let dirs = lock(&inner.dirs).clone();
    let apply = |text: &str| change.apply(text);
    let local = dirs.local.join(crate::sync_folder::LOCAL_SETTINGS);
    if dirs.synced && goes_local(change, &local) {
        let file = crate::sync_folder::LOCAL_SETTINGS;
        return rewrite(&local, "", &apply, between).map_err(|failed| failed.warning(file));
    }
    let path = dirs.shared.join("settings.toml");
    // The sync folder (or its settings.toml) is not there: not written, nor made from the template.
    if dirs.synced && !path.is_file() {
        lock(&inner.unsaved).push(change.clone());
        return Err(Warning::new("settings.toml", NOT_SAVED));
    }
    match rewrite(&path, SETTINGS_TEMPLATE, &apply, between) {
        Ok(()) => Ok(()),
        Err(Failed::Changing) => {
            lock(&inner.unsaved).push(change.clone());
            Err(Failed::Changing.warning("settings.toml"))
        }
        Err(failed) => Err(failed.warning("settings.toml")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    fn one(dir: &Path) -> Arc<Mutex<Dirs>> {
        Arc::new(Mutex::new(Dirs::one(dir.to_path_buf())))
    }

    fn read(dir: &Path) -> Settings {
        let text = std::fs::read_to_string(dir.join("settings.toml")).unwrap();
        Settings::parse("settings.toml", &text, &mut Vec::new())
    }

    #[test]
    fn system_values_round_trip_and_keep_comments() {
        let text = "# mine\n[system]\n# keep\nsingle-instance = true\n";
        let on = SettingsChange::System(SystemValue::Tray(true)).apply(text).unwrap();
        let on = SettingsChange::System(SystemValue::Hotkey("win+shift+e".into())).apply(&on).unwrap();
        assert!(on.contains("# mine") && on.contains("# keep"), "{on}");
        let mut warnings = Vec::new();
        let settings = Settings::parse("settings.toml", &on, &mut warnings);
        assert!(warnings.is_empty() && settings.system.tray && settings.system.hotkey.is_some(), "{on}");
        let off = SettingsChange::System(SystemValue::Hotkey(String::new())).apply(&on).unwrap();
        let settings = Settings::parse("settings.toml", &off, &mut warnings);
        assert!(warnings.is_empty() && settings.system.hotkey.is_none() && settings.system.tray, "{off}");
    }

    #[test]
    fn a_file_with_crlf_line_breaks_keeps_them() {
        let text = "theme = \"auto\"\r\n\r\n[layout]\r\ndensity = \"comfortable\" # c\r\n# the end\r\n";
        let change = SettingsChange::Value {
            table: Some("layout".into()),
            key: "density".into(),
            value: Some("\"compact\"".into()),
        };
        let out = change.apply(text).unwrap();
        assert_eq!(out, "theme = \"auto\"\r\n\r\n[layout]\r\ndensity = \"compact\" # c\r\n# the end\r\n");
        let lf = change.apply("[layout]\ndensity = \"comfortable\"\n").unwrap();
        assert_eq!(lf, "[layout]\ndensity = \"compact\"\n");
    }

    #[test]
    fn changes_of_every_kind_close_together_are_all_written_in_order() {
        let dir = crate::test_dir("settings-writer");
        std::fs::write(dir.join("settings.toml"), "# mine\n[sidebar]\npinned = []\n").unwrap();
        let writer = Arc::new(SettingsWriter::new(one(&dir)));
        let (results, told) = mpsc::channel();
        let done = |results: &Sender<_>| -> Done {
            let results = results.clone();
            Box::new(move |result| results.send(result).unwrap())
        };
        // From two threads at once, as the filter bar and the sidebar might.
        let threads: Vec<_> = (0..2)
            .map(|n| {
                let (writer, results) = (writer.clone(), results.clone());
                std::thread::spawn(move || {
                    for i in 0..20 {
                        let change = if n == 0 {
                            SettingsChange::Pinned(vec![PinEntry::plain(format!("/p{i}"))])
                        } else {
                            SettingsChange::Filters(vec![SavedFilter { name: format!("F{i}"), pattern: "x".into() }])
                        };
                        writer.send(change, done(&results));
                    }
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        let view = ViewSettings { mode: gezik_core::view::ViewMode::Grid, ..ViewSettings::default() };
        writer.send(SettingsChange::ViewDefaults(view), done(&results));
        let preset = RenamePreset { name: "P".into(), include_extension: false, rules: Vec::new() };
        writer.send(SettingsChange::RenamePresets(vec![preset.clone()]), done(&results));
        writer.flush();
        drop(results);
        let told: Vec<_> = told.iter().collect();
        assert_eq!(told.len(), 42, "every change is told its result");
        assert!(told.iter().all(Result::is_ok));
        let settings = read(&dir);
        assert_eq!(crate::pins::paths(&settings.pinned), ["/p19"], "the last of each kind wins: none is lost");
        assert_eq!(settings.filters, [SavedFilter { name: "F19".into(), pattern: "x".into() }]);
        assert_eq!(settings.view.view.mode, gezik_core::view::ViewMode::Grid);
        assert_eq!(settings.rename_presets, [preset]);
        let text = std::fs::read_to_string(dir.join("settings.toml")).unwrap();
        assert!(text.contains("# mine"), "comments stay: {text}");
        let names: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names, ["settings.toml"], "no temporary file is left");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_broken_file_is_left_alone_and_said() {
        let dir = crate::test_dir("settings-writer-broken");
        std::fs::write(dir.join("settings.toml"), "[sidebar\n").unwrap();
        let writer = SettingsWriter::new(one(&dir));
        let (results, told) = mpsc::channel();
        writer.send(SettingsChange::Pinned(vec![PinEntry::plain("/a")]), Box::new(move |r| results.send(r).unwrap()));
        let warning = told.recv_timeout(FLUSH_WAIT).unwrap().unwrap_err();
        assert!(warning.message.starts_with("Fix settings.toml first"), "{}", warning.message);
        assert_eq!(std::fs::read_to_string(dir.join("settings.toml")).unwrap(), "[sidebar\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn value(table: Option<&str>, key: &str, value: &str) -> SettingsChange {
        SettingsChange::Value { table: table.map(str::to_owned), key: key.to_owned(), value: Some(value.to_owned()) }
    }

    fn synced(root: &Path) -> Arc<Mutex<Dirs>> {
        let dirs = Dirs { shared: root.join("Dropbox"), local: root.join("local"), portable: false, synced: true };
        std::fs::create_dir_all(&dirs.local).unwrap();
        Arc::new(Mutex::new(dirs))
    }

    #[test]
    fn a_change_synced_in_between_the_read_and_the_write_stays_with_ours() {
        let dir = crate::test_dir("settings-race");
        let path = dir.join("settings.toml");
        let inner = Inner { dirs: one(&dir), writing: Mutex::new(()), unsaved: Mutex::new(Vec::new()) };
        for round in 0..1000 {
            std::fs::write(&path, "# mine\n[layout]\ndensity = \"comfortable\"\n").unwrap();
            let syncs = std::cell::Cell::new(round % 4);
            // The "syncer": another computer's change to another key lands after Gezik read.
            let syncer = || {
                if syncs.get() > 0 {
                    syncs.set(syncs.get() - 1);
                    let text = std::fs::read_to_string(&path).unwrap();
                    let theirs = SettingsChange::Value {
                        table: None,
                        key: "max-fps".into(),
                        // A length of its own each time: a time stamp may not tell two writes apart.
                        value: Some(format!("{}{}", 30 + round, "0".repeat(syncs.get()))),
                    };
                    std::fs::write(&path, theirs.apply(&text).unwrap()).unwrap();
                }
            };
            let ours = value(Some("layout"), "density", "\"compact\"");
            edit_with(&inner, &ours, &syncer).unwrap();
            let settings = read(&dir);
            assert_eq!(settings.density, crate::settings::Density::Compact, "round {round}: ours");
            if round % 4 > 0 {
                let text = std::fs::read_to_string(&path).unwrap();
                assert!(text.contains(&format!("max-fps = {}\n", 30 + round)), "round {round}: theirs stays\n{text}");
            }
            assert!(std::fs::read_to_string(&path).unwrap().contains("# mine\n[layout]"), "comments stay");
        }
        assert_eq!(lock(&inner.unsaved).len(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_that_keeps_changing_is_left_and_the_change_kept_for_later() {
        let dir = crate::test_dir("settings-race-lost");
        let path = dir.join("settings.toml");
        std::fs::write(&path, "max-fps = 60\n").unwrap();
        let inner = Inner { dirs: one(&dir), writing: Mutex::new(()), unsaved: Mutex::new(Vec::new()) };
        let tries = std::cell::Cell::new(0);
        let syncer = || {
            tries.set(tries.get() + 1);
            std::fs::write(&path, format!("max-fps = 60{}\n", "0".repeat(tries.get()))).unwrap();
        };
        let ours = value(Some("layout"), "density", "\"compact\"");
        let warning = edit_with(&inner, &ours, &syncer).unwrap_err();
        assert_eq!(tries.get(), RETRIES + 1, "made once and again three times");
        assert!(warning.message.contains("kept changing"), "{}", warning.message);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "max-fps = 600000\n", "theirs is not written over");
        assert_eq!(*lock(&inner.unsaved), [ours]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn with_a_sync_folder_system_and_this_computers_values_go_to_the_local_file() {
        let root = crate::test_dir("settings-local");
        let dirs = synced(&root);
        let (shared, local) = (root.join("Dropbox"), root.join("local").join(crate::sync_folder::LOCAL_SETTINGS));
        std::fs::create_dir_all(&shared).unwrap();
        std::fs::write(shared.join("settings.toml"), "# shared\n[layout]\ndensity = \"comfortable\"\n").unwrap();
        let writer = SettingsWriter::new(dirs.clone());
        writer.save(&SettingsChange::System(SystemValue::Tray(true))).unwrap();
        let only_here = SettingsChange::Local {
            table: Some("layout".into()),
            key: "sidebar".into(),
            value: Some("\"right\"".into()),
        };
        writer.save(&only_here).unwrap();
        // A key in the local file is changed there; any other in the shared one.
        writer.save(&value(Some("layout"), "sidebar", "\"hidden\"")).unwrap();
        writer.save(&value(Some("layout"), "density", "\"compact\"")).unwrap();
        let local_text = std::fs::read_to_string(&local).unwrap();
        let shared_text = std::fs::read_to_string(shared.join("settings.toml")).unwrap();
        assert!(local_text.contains("tray = true") && local_text.contains("sidebar = \"hidden\""), "{local_text}");
        assert!(!shared_text.contains("tray") && !shared_text.contains("sidebar"), "{shared_text}");
        assert!(shared_text.contains("density = \"compact\""), "{shared_text}");
        let layered = Settings::parse_layered("settings.toml", &shared_text, Some(&local_text), &mut Vec::new());
        assert!(layered.system.tray);
        assert_eq!(layered.sidebar, crate::settings::SidebarPosition::Hidden);
        // Use the Shared Value: out of the local file.
        writer
            .save(&SettingsChange::Local { table: Some("layout".into()), key: "sidebar".into(), value: None })
            .unwrap();
        assert!(!std::fs::read_to_string(&local).unwrap().contains("sidebar"));
        // Without a sync folder the local file is not used.
        lock(&dirs).synced = false;
        writer.save(&SettingsChange::System(SystemValue::Tray(false))).unwrap();
        assert!(std::fs::read_to_string(shared.join("settings.toml")).unwrap().contains("tray = false"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_sync_folder_is_not_made_and_its_changes_wait_for_it() {
        let root = crate::test_dir("settings-offline");
        let writer = SettingsWriter::new(synced(&root));
        let shared = root.join("Dropbox");
        let pin = SettingsChange::Pinned(vec![PinEntry::plain("/a")]);
        let warning = writer.save(&pin).unwrap_err();
        assert_eq!(warning.message, NOT_SAVED);
        assert!(kept(&warning));
        assert!(!shared.exists(), "neither the folder nor a template settings.toml is made");
        assert_eq!(writer.unsaved(), 1);
        // The folder is back: the next reload hands the change over again.
        std::fs::create_dir_all(&shared).unwrap();
        std::fs::write(shared.join("settings.toml"), "# theirs\n").unwrap();
        writer.retry_unsaved();
        writer.flush();
        assert_eq!(writer.unsaved(), 0);
        assert_eq!(crate::pins::paths(&read(&shared).pinned), ["/a"]);
        let _ = std::fs::remove_dir_all(&root);
    }
}
