//! `settings.toml` written by one thread of its own: every change the UI makes to it (the
//! saved filters, the tab sets, the pinned folders, the rename rule sets, the `[view]`
//! defaults, the view options) goes through one queue, so the UI thread never touches the
//! file, changes are written in the order they were made, and two of them never read, edit
//! and write the file over each other. With a sync folder (spec 14 §8) another computer's
//! change may land between the read and the write: the edit is then made again on the new text,
//! and what cannot be written (the folder is gone, the file keeps changing) is kept in memory
//! and written at the next reload.

use gezik_i18n::{t, tx};
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
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
use crate::sync_folder::three_way;

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
            SettingsChange::Local { table, key, value } => {
                let edited = with_value(text, table.as_deref(), key, value.as_deref())?;
                // Use the Shared Value took out the table's last key: no empty `[view]` left.
                match table.as_deref().filter(|_| value.is_none()) {
                    Some(table) => crate::sync_folder::without_empty(&edited, table),
                    None => Ok(edited),
                }
            }
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
    /// The changes not written yet, again ([`SettingsWriter::retry_unsaved`]).
    Retry,
    /// Answered once every change sent before it is written.
    Flush(Sender<()>),
}

/// A change not written yet (spec 14 §8.4): the text of settings.toml it was made on, and that
/// text with it. Made again on the file as it is when it can be written ([`three_way`]), so a
/// list changed elsewhere meanwhile keeps the other computer's items.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pending {
    pub base: String,
    pub ours: String,
}

/// What the writer and its thread share.
struct Inner {
    /// Where settings.toml is (the store's: a sync folder set or left moves it).
    dirs: Arc<Mutex<Dirs>>,
    /// Held while the file is read, edited and written: one change at a time, whoever makes it.
    writing: Mutex<()>,
    /// Changes not written yet, oldest first: the sync folder was not there, or the file kept
    /// changing. Kept in `pending-settings.toml` too (a quit while the folder is away loses none).
    unsaved: Mutex<Vec<Pending>>,
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
    /// With a sync folder, the changes a last run could not write are read back (one `open`).
    pub fn new(dirs: Arc<Mutex<Dirs>>) -> SettingsWriter {
        let unsaved = {
            let dirs = lock(&dirs);
            if dirs.synced { read_pending(&dirs.local.join(PENDING)) } else { Vec::new() }
        };
        let inner = Inner { dirs, writing: Mutex::new(()), unsaved: Mutex::new(unsaved) };
        SettingsWriter { inner: Arc::new(inner), queue: Mutex::new(None) }
    }

    /// Makes `change` now, on this thread (tests, and the writer thread itself).
    pub fn save(&self, change: &SettingsChange) -> Result<(), Warning> {
        edit(&self.inner, change)
    }

    /// Has the writer thread write the changes not written yet, oldest first, each made again on
    /// the file as it is now (a reload found the sync folder there). Those that cannot be written
    /// stay, in order.
    pub fn retry_unsaved(&self) {
        let mut queue = lock(&self.queue);
        if queue.is_none() {
            *queue = self.start();
        }
        if queue.as_ref().is_none_or(|sender| sender.send(Job::Retry).is_err()) {
            drop(queue);
            retry(&self.inner, &|| {});
        }
    }

    /// How many changes are not written yet.
    pub fn unsaved(&self) -> usize {
        lock(&self.inner.unsaved).len()
    }

    /// settings.toml as the changes not written yet make it (`None`: none waits): what this
    /// computer shows while the sync folder is away.
    pub fn unsaved_text(&self) -> Option<String> {
        lock(&self.inner.unsaved).last().map(|p| p.ours.clone())
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
                        Job::Retry => retry(&inner, &|| {}),
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

/// In the local folder: the settings changes not written yet ([`Pending`]).
pub const PENDING: &str = "pending-settings.toml";

fn read_pending(path: &Path) -> Vec<Pending> {
    let Ok(text) = read_text(path) else { return Vec::new() };
    let Ok(table) = text.parse::<toml::Table>() else { return Vec::new() };
    let Some(list) = table.get("change").and_then(toml::Value::as_array) else { return Vec::new() };
    list.iter()
        .filter_map(|entry| {
            let text = |key: &str| entry.get(key).and_then(toml::Value::as_str).map(str::to_owned);
            Some(Pending { base: text("base")?, ours: text("ours")? })
        })
        .collect()
}

/// Keeps `unsaved` in the local folder (the file goes when none is left). A failure is not said:
/// they are still in memory.
fn keep_pending(inner: &Inner, unsaved: &[Pending]) {
    let path = lock(&inner.dirs).local.join(PENDING);
    if unsaved.is_empty() {
        let _ = std::fs::remove_file(path);
        return;
    }
    let list: Vec<toml::Value> = unsaved
        .iter()
        .map(|p| {
            let mut entry = toml::Table::new();
            entry.insert("base".into(), p.base.clone().into());
            entry.insert("ours".into(), p.ours.clone().into());
            toml::Value::Table(entry)
        })
        .collect();
    let mut table = toml::Table::new();
    table.insert("change".into(), toml::Value::Array(list));
    if let Ok(text) = toml::to_string(&table) {
        let _ =
            write_atomic(&path, &t!("# Settings changes Gezik could not write yet; written when it can.\n{}", text));
    }
}

/// Why [`rewrite`] wrote nothing.
#[derive(Debug)]
pub enum Failed {
    Read(io::Error),
    /// The edit cannot be made (the file does not read).
    Edit(String),
    Write(io::Error),
    /// The file changed between each read and write: the change, as last made, for later.
    Changing(Pending),
}

impl Failed {
    fn warning(self, file: &str) -> Warning {
        let message = match self {
            Failed::Read(err) => t!("cannot read: {}", err),
            // TRANSLATORS: {0} is a file name, {1} why it does not read.
            Failed::Edit(err) => t!("Fix {0} first ({1})", file, err.lines().next().unwrap_or_default()),
            Failed::Write(err) => t!("cannot write: {}", err),
            Failed::Changing(_) => t("", KEPT_CHANGING).into_owned(),
        };
        Warning::new(file, message)
    }
}

/// How many times an edit is made again on a file that changed under it (spec 14 §8.3).
pub const RETRIES: usize = 3;
/// Said for a change kept in memory while the sync folder is not there.
pub const NOT_SAVED: &str = tx!("the settings folder is not available: not saved yet (written when it is back)");
const KEPT_CHANGING: &str =
    tx!("kept changing while Gezik wrote it (a sync service?): the change is kept and written at the next reload");
const WAITING: &str = tx!("waits for the changes before it (written with them at the next reload)");
/// Said when a key changed on another computer too: Gezik's own conflicting copy holds ours.
pub const CONFLICTED: &str = tx!(
    "changed on another computer too: your value is in a conflicting copy — Review it in Settings > Sync and Backup"
);

/// Whether `warning` says a change was kept to be written later (it is not lost: what shows it
/// may keep showing it).
pub fn kept(warning: &Warning) -> bool {
    [NOT_SAVED, KEPT_CHANGING, WAITING].iter().any(|said| t("", said) == warning.message.as_str())
}

/// Whether a status line says a change was kept (to clear it once none waits).
pub fn says_kept(text: &str) -> bool {
    [NOT_SAVED, KEPT_CHANGING, WAITING].iter().any(|said| text.contains(t("", said).as_ref()))
}

type Stamp = Option<(u64, Option<std::time::SystemTime>)>;

/// The file's size and time, to tell whether it changed since.
fn stamp(path: &Path) -> Stamp {
    std::fs::metadata(path).ok().map(|meta| (meta.len(), meta.modified().ok()))
}

fn read_or(path: &Path, missing: &str) -> Result<String, Failed> {
    match read_text(path) {
        Ok(text) => Ok(text),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(missing.to_owned()),
        Err(err) => Err(Failed::Read(err)),
    }
}

/// Rewrites `path` with `edit` made to its text (`missing`: the text when there is no file). If
/// the file changed between the read and the write (another computer's change synced in), the
/// change is made again on the new text ([`three_way`]: key by key, lists item by item), up to
/// [`RETRIES`] times. A key both changed stays as the file has it and ours goes to a conflicting
/// copy next to it (its path is returned) for a Review. `between` runs before each write (tests
/// stand for the syncer there).
pub fn rewrite(
    path: &Path,
    missing: &str,
    edit: &dyn Fn(&str) -> Result<String, String>,
    between: &dyn Fn(),
) -> Result<Option<PathBuf>, Failed> {
    let before = stamp(path);
    let base = read_or(path, missing)?;
    let ours = edit(&base).map_err(Failed::Edit)?;
    put(path, Pending { base, ours }, before, between)
}

/// Writes `change.ours` to `path` if the file is still as `before`; else makes the change again on
/// the new text, up to [`RETRIES`] times.
fn put(path: &Path, change: Pending, mut before: Stamp, between: &dyn Fn()) -> Result<Option<PathBuf>, Failed> {
    let Pending { mut base, mut ours } = change;
    let mut copy = None;
    for _ in 0..=RETRIES {
        between();
        let now = stamp(path);
        if now == before {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(Failed::Write)?;
            }
            return write_atomic(path, &ours).map(|()| copy).map_err(Failed::Write);
        }
        let theirs = read_or(path, &base)?;
        let merged = merge_onto(path, &base, &ours, &theirs)?;
        copy = merged.1.or(copy);
        (base, ours, before) = (theirs, merged.0, now);
    }
    Err(Failed::Changing(Pending { base, ours }))
}

/// Our change (`base` → `ours`) made on `theirs`; a key both changed goes to a conflicting copy
/// of `path` (written now, its path returned).
fn merge_onto(path: &Path, base: &str, ours: &str, theirs: &str) -> Result<(String, Option<PathBuf>), Failed> {
    let merged = three_way(base, ours, theirs).map_err(Failed::Edit)?;
    if merged.conflicts.is_empty() {
        return Ok((merged.text, None));
    }
    let copy_text = merged
        .conflicts
        .iter()
        .try_fold(merged.text.clone(), |text, key| crate::sync_folder::with_item_from(&text, ours, key))
        .map_err(Failed::Edit)?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let folder = path.parent().unwrap_or(Path::new("."));
    let copy = folder.join(crate::sync_folder::copy_name(&name, |n| folder.join(n).exists()));
    write_atomic(&copy, &copy_text).map_err(Failed::Write)?;
    Ok((merged.text, Some(copy)))
}

/// Whether `change` goes to settings.local.toml (`local`) while a sync folder is used: "Only on
/// This Computer", `[system]` (a tray on one computer is not the others'; decision 12), and a
/// value already there.
fn goes_local(change: &SettingsChange, local: &Path) -> bool {
    match change {
        SettingsChange::Local { .. } | SettingsChange::System(_) => true,
        SettingsChange::Value { table: Some(table), .. } if table == "system" => true,
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
/// folder that is not there, a file that keeps changing, or changes still waiting before it,
/// the change waits (in order) for the next reload.
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
        return rewrite(&local, "", &apply, between).map(drop).map_err(|failed| failed.warning(file));
    }
    let path = dirs.shared.join("settings.toml");
    let there = path.is_file();
    let mut unsaved = lock(&inner.unsaved);
    // The sync folder (or its settings.toml) is not there, or older changes wait: this one waits
    // behind them, made on the text they leave (not written, nor made from the template).
    if !unsaved.is_empty() || (dirs.synced && !there) {
        let base = match unsaved.last() {
            Some(last) => last.ours.clone(),
            None => read_text(&dirs.local.join(crate::sync_folder::CACHE).join("settings.toml"))
                .unwrap_or_else(|_| SETTINGS_TEMPLATE.to_owned()),
        };
        let ours = apply(&base).map_err(|err| Failed::Edit(err).warning("settings.toml"))?;
        unsaved.push(Pending { base, ours });
        keep_pending(inner, &unsaved);
        let why = if dirs.synced && !there { NOT_SAVED } else { WAITING };
        return Err(Warning::new("settings.toml", t("", why)));
    }
    drop(unsaved);
    match rewrite(&path, SETTINGS_TEMPLATE, &apply, between) {
        Ok(None) => Ok(()),
        Ok(Some(_)) => Err(Warning::new("settings.toml", t("", CONFLICTED))),
        Err(Failed::Changing(pending)) => {
            let mut unsaved = lock(&inner.unsaved);
            unsaved.push(pending);
            keep_pending(inner, &unsaved);
            Err(Warning::new("settings.toml", t("", KEPT_CHANGING)))
        }
        Err(failed) => Err(failed.warning("settings.toml")),
    }
}

/// Writes the changes not written yet, oldest first, each made again on the file as it is now;
/// stops at the first that cannot be written (the folder away, the file changing or broken).
fn retry(inner: &Inner, between: &dyn Fn()) {
    let _turn = lock(&inner.writing);
    let path = lock(&inner.dirs).shared.join("settings.toml");
    loop {
        let Some(first) = lock(&inner.unsaved).first().cloned() else { return };
        if !path.is_file() {
            return;
        }
        let before = stamp(&path);
        let written = read_or(&path, &first.base)
            .and_then(|theirs| merge_onto(&path, &first.base, &first.ours, &theirs).map(|m| (theirs, m.0)))
            .and_then(|(theirs, ours)| put(&path, Pending { base: theirs, ours }, before, between));
        let mut unsaved = lock(&inner.unsaved);
        match written {
            Ok(_) => {
                unsaved.remove(0);
            }
            Err(Failed::Changing(pending)) => {
                unsaved[0] = pending;
                keep_pending(inner, &unsaved);
                return;
            }
            Err(_) => return,
        }
        keep_pending(inner, &unsaved);
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
        let waiting = lock(&inner.unsaved).clone();
        assert_eq!(waiting.len(), 1);
        assert!(waiting[0].ours.contains("density = \"compact\"") && waiting[0].ours.contains("600000"));

        // A newer change waits behind it (not written before it), and both go in order.
        let older = SettingsChange::Pinned(vec![PinEntry::plain("/a")]);
        let newer = SettingsChange::Pinned(vec![PinEntry::plain("/a"), PinEntry::plain("/b")]);
        let warning = edit_with(&inner, &older, &|| {}).unwrap_err();
        assert!(kept(&warning), "{}", warning.message);
        assert!(edit_with(&inner, &newer, &|| {}).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "max-fps = 600000\n", "nothing written out of order");
        assert!(dir.join(PENDING).is_file(), "kept on disk too");
        retry(&inner, &|| {});
        assert!(lock(&inner.unsaved).is_empty() && !dir.join(PENDING).exists());
        let settings = read(&dir);
        assert_eq!(crate::pins::paths(&settings.pinned), ["/a", "/b"], "the newer list, not the older");
        assert_eq!(settings.density, crate::settings::Density::Compact);
        assert!(std::fs::read_to_string(&path).unwrap().contains("max-fps = 600000"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_pin_made_offline_keeps_the_pins_another_computer_made_meanwhile_even_after_a_restart() {
        let root = crate::test_dir("settings-offline-merge");
        let dirs = synced(&root);
        let (shared, local) = (root.join("Dropbox"), root.join("local"));
        // Computer A starts without the folder: its last copy has [x].
        std::fs::create_dir_all(local.join(crate::sync_folder::CACHE)).unwrap();
        std::fs::write(local.join(crate::sync_folder::CACHE).join("settings.toml"), "# mine\npinned = [\"/x\"]\n")
            .unwrap();
        let writer = SettingsWriter::new(dirs.clone());
        let pins = |paths: &[&str]| SettingsChange::Pinned(paths.iter().map(|p| PinEntry::plain(*p)).collect());
        assert!(writer.save(&pins(&["/x", "/w"])).is_err());
        assert!(writer.unsaved_text().unwrap().contains("/w"));
        // Gezik quits and starts again, still without the folder: the change is still there.
        drop(writer);
        let writer = SettingsWriter::new(dirs.clone());
        assert_eq!(writer.unsaved(), 1);
        // Meanwhile computer B pinned y and z; the folder comes back.
        std::fs::create_dir_all(&shared).unwrap();
        std::fs::write(shared.join("settings.toml"), "# mine\npinned = [\"/x\", \"/y\", \"/z\"]\n").unwrap();
        writer.retry_unsaved();
        writer.flush();
        assert_eq!(writer.unsaved(), 0);
        assert_eq!(crate::pins::paths(&read(&shared).pinned), ["/x", "/y", "/z", "/w"]);
        assert!(!local.join(PENDING).exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_key_both_computers_changed_goes_to_a_conflicting_copy() {
        let root = crate::test_dir("settings-offline-conflict");
        let dirs = synced(&root);
        let (shared, local) = (root.join("Dropbox"), root.join("local"));
        std::fs::create_dir_all(local.join(crate::sync_folder::CACHE)).unwrap();
        std::fs::write(local.join(crate::sync_folder::CACHE).join("settings.toml"), "theme = \"dark\"\n").unwrap();
        let writer = SettingsWriter::new(dirs);
        let theme = SettingsChange::Value { table: None, key: "theme".into(), value: Some("\"nord\"".into()) };
        assert!(writer.save(&theme).is_err());
        std::fs::create_dir_all(&shared).unwrap();
        std::fs::write(shared.join("settings.toml"), "theme = \"light\"\n").unwrap();
        writer.retry_unsaved();
        writer.flush();
        assert_eq!(std::fs::read_to_string(shared.join("settings.toml")).unwrap(), "theme = \"light\"\n");
        let copy = shared.join("settings (Gezik conflicted copy).toml");
        assert_eq!(std::fs::read_to_string(copy).unwrap(), "theme = \"nord\"\n", "ours, for the Review");
        let _ = std::fs::remove_dir_all(&root);
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
        let local_text = std::fs::read_to_string(&local).unwrap();
        assert!(!local_text.contains("sidebar") && !local_text.contains("[layout]"), "no empty table: {local_text}");
        // Any [system] value from a row (quick look, Reset to Default) is this computer's too.
        writer.save(&value(Some("system"), "quick-look", "\"gezik\"")).unwrap();
        assert!(std::fs::read_to_string(&local).unwrap().contains("quick-look = \"gezik\""));
        assert!(!std::fs::read_to_string(shared.join("settings.toml")).unwrap().contains("quick-look"));
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
