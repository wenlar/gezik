//! `settings.toml` written by one thread of its own: every change the UI makes to it (the
//! saved filters, the tab sets, the pinned folders, the rename rule sets, the `[view]`
//! defaults, the view options) goes through one queue, so the UI thread never touches the
//! file, changes are written in the order they were made, and two of them never read, edit
//! and write the file over each other.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gezik_core::view::ViewSettings;

use crate::Warning;
use crate::lock;
use crate::paths::write_atomic;
use crate::pins::PinEntry;
use crate::settings::{RenamePreset, SavedFilter, SavedSearch, TabSet, ViewOption};
use crate::settings_edit::{
    with_filters, with_pinned, with_rename_presets, with_searches, with_system, with_tab_sets, with_view_defaults,
    with_view_option,
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
    /// A `[system]` value (the tray icon, the global shortcut).
    System(SystemValue),
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
        match self {
            SettingsChange::Pinned(pinned) => with_pinned(text, pinned),
            SettingsChange::ViewDefaults(view) => with_view_defaults(text, view),
            SettingsChange::ViewOption(option) => with_view_option(text, *option),
            SettingsChange::RenamePresets(presets) => with_rename_presets(text, presets),
            SettingsChange::Filters(filters) => with_filters(text, filters),
            SettingsChange::TabSets(sets) => with_tab_sets(text, sets),
            SettingsChange::Searches(searches) => with_searches(text, searches),
            SettingsChange::System(SystemValue::Tray(on)) => with_system(text, "tray", (*on).into()),
            SettingsChange::System(SystemValue::Hotkey(chord)) => with_system(text, "hotkey", chord.as_str().into()),
        }
    }
}

/// Told the result of a change, on the writer thread.
pub type Done = Box<dyn FnOnce(Result<(), Warning>) + Send>;

enum Job {
    Write(SettingsChange, Done),
    /// Answered once every change sent before it is written.
    Flush(Sender<()>),
}

pub(crate) struct SettingsWriter {
    dir: PathBuf,
    /// Held while the file is read, edited and written: one change at a time, whoever makes it.
    writing: Arc<Mutex<()>>,
    /// The queue to the writer thread, once it was started.
    queue: Mutex<Option<Sender<Job>>>,
}

impl fmt::Debug for SettingsWriter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SettingsWriter").field("dir", &self.dir).finish_non_exhaustive()
    }
}

impl SettingsWriter {
    pub fn new(dir: PathBuf) -> SettingsWriter {
        SettingsWriter { dir, writing: Arc::new(Mutex::new(())), queue: Mutex::new(None) }
    }

    /// Makes `change` now, on this thread (tests, and the writer thread itself).
    pub fn save(&self, change: &SettingsChange) -> Result<(), Warning> {
        edit(&self.dir, &self.writing, change)
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
        let (dir, writing) = (self.dir.clone(), self.writing.clone());
        std::thread::Builder::new()
            .name("gezik-settings".into())
            .spawn(move || {
                for job in jobs {
                    match job {
                        Job::Write(change, done) => done(edit(&dir, &writing, &change)),
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

/// Applies `change` to `settings.toml` in `dir`. Creates the file from the template if it does
/// not exist; refuses to touch a broken file.
fn edit(dir: &Path, writing: &Mutex<()>, change: &SettingsChange) -> Result<(), Warning> {
    let _turn = lock(writing);
    let path = dir.join("settings.toml");
    let text = match read_text(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == io::ErrorKind::NotFound => SETTINGS_TEMPLATE.to_owned(),
        Err(err) => return Err(Warning::new("settings.toml", format!("cannot read: {err}"))),
    };
    let edited = change.apply(&text).map_err(|err| {
        let first_line = err.lines().next().unwrap_or_default().to_owned();
        Warning::new("settings.toml", format!("Fix settings.toml first ({first_line})"))
    })?;
    std::fs::create_dir_all(dir).map_err(|err| Warning::new("settings.toml", format!("cannot write: {err}")))?;
    write_atomic(&path, &edited).map_err(|err| Warning::new("settings.toml", format!("cannot write: {err}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

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
    fn changes_of_every_kind_close_together_are_all_written_in_order() {
        let dir = crate::test_dir("settings-writer");
        std::fs::write(dir.join("settings.toml"), "# mine\n[sidebar]\npinned = []\n").unwrap();
        let writer = Arc::new(SettingsWriter::new(dir.clone()));
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
        let writer = SettingsWriter::new(dir.clone());
        let (results, told) = mpsc::channel();
        writer.send(SettingsChange::Pinned(vec![PinEntry::plain("/a")]), Box::new(move |r| results.send(r).unwrap()));
        let warning = told.recv_timeout(FLUSH_WAIT).unwrap().unwrap_err();
        assert!(warning.message.starts_with("Fix settings.toml first"), "{}", warning.message);
        assert_eq!(std::fs::read_to_string(dir.join("settings.toml")).unwrap(), "[sidebar\n");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
