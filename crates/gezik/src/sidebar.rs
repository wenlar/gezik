//! The sidebar: known folders, pinned folders and drives, kept up to date.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use gezik_config::Warning;
use gezik_config::paths::KnownDirs;
use gezik_config::pins::{self, PinEntry};
use gezik_config::settings_writer::SettingsChange;
use gezik_config::store::ConfigStore;
use gezik_core::nav::Location;
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::navigation::{Navigator, sync_model};
use crate::{AppWindow, SidebarRow};

pub const SECTION_FOLDERS: i32 = 0;
pub const SECTION_PINNED: i32 = 1;
pub const SECTION_DRIVES: i32 = 2;

/// The drives' section title: Finder calls it "Locations".
const DRIVES_HEADER: &str = if cfg!(target_os = "macos") { "LOCATIONS" } else { "DRIVES" };

/// How often the (cheap) drive signature is checked.
const DRIVE_POLL: Duration = Duration::from_secs(3);

/// Whether two names are equal as the file system sees them: ignoring case (also of
/// non-ASCII letters like Ç) on Windows, exactly elsewhere.
fn same_text(a: &str, b: &str) -> bool {
    a == b || (cfg!(windows) && a.to_lowercase() == b.to_lowercase())
}

/// Whether `a` and `b` name the same location (see [`same_text`]). Separators and
/// redundant `/` or `.` parts do not matter.
pub fn same_path(a: &Path, b: &Path) -> bool {
    a == b
        || (cfg!(windows)
            && a.components().count() == b.components().count()
            && a.components()
                .zip(b.components())
                .all(|(x, y)| same_text(&x.as_os_str().to_string_lossy(), &y.as_os_str().to_string_lossy())))
}

/// A pinned entry that exists on this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pin {
    /// As settings.toml has it (the path with tokens, the alias, the group).
    pub entry: PinEntry,
    pub path: PathBuf,
}

/// The pinned entries that exist, in `pinned` order. `exists` checks the file system, so
/// this runs off the UI thread.
pub fn check_pins(dirs: &KnownDirs, pinned: &[PinEntry], exists: impl Fn(&Path) -> bool) -> Vec<Pin> {
    pinned
        .iter()
        .filter_map(|entry| {
            let path = dirs.expand_checked(&entry.path).filter(|p| exists(p))?;
            Some(Pin { entry: entry.clone(), path })
        })
        .collect()
}

/// The pinned list and what the sidebar shows of it. Checking which pins exist can be
/// slow (a dead network drive may take many seconds), so it runs in the background;
/// meanwhile `visible` keeps the entries already known to exist, in the new order.
#[derive(Debug, Default)]
struct PinState {
    /// As settings.toml has it, in the order the sidebar shows it (`pins::normalize`).
    pinned: Vec<PinEntry>,
    /// What the PINNED section shows; its indexes are the section's row indexes.
    visible: Vec<Pin>,
    /// Bumped by every check, so that the result of an overtaken check is dropped.
    generation: u64,
}

impl PinState {
    /// Takes a new pinned list. Returns false if it is unchanged.
    fn set(&mut self, pinned: Vec<PinEntry>) -> bool {
        let pinned = pins::normalize(pinned);
        if self.pinned == pinned {
            return false;
        }
        let known = std::mem::take(&mut self.visible);
        self.visible = pinned
            .iter()
            .filter_map(|entry| {
                let pin = known.iter().find(|pin| pin.entry.path == entry.path)?;
                Some(Pin { entry: entry.clone(), path: pin.path.clone() })
            })
            .collect();
        self.pinned = pinned;
        true
    }

    /// Starts a check of the current list; returns its ticket.
    fn begin_check(&mut self) -> u64 {
        self.generation += 1;
        self.generation
    }

    /// Applies a check's result unless a newer check started since. Returns whether it did.
    fn finish_check(&mut self, ticket: u64, visible: Vec<Pin>) -> bool {
        if ticket != self.generation {
            return false;
        }
        self.visible = visible;
        true
    }

    /// The index in `pinned` of shown row `index`.
    fn stored_index(&self, index: usize) -> Option<usize> {
        let pin = self.visible.get(index)?;
        pins::find(&self.pinned, &pin.entry.path)
    }
}

struct Inner {
    window: slint::Weak<AppWindow>,
    nav: Navigator,
    store: Option<ConfigStore>,
    dirs: KnownDirs,
    pins: PinState,
    drive_signature: u64,
    /// The rows' model, updated in place (see [`sync_model`]).
    rows: Rc<VecModel<SidebarRow>>,
    poll: slint::Timer,
    /// Pin lists sent to the settings writer and not yet written (or failed).
    pins_on_their_way: usize,
    /// The pinned list settings.toml has, as last read or written.
    pins_in_file: Vec<PinEntry>,
}

thread_local! {
    /// The sidebar of this (UI) thread, so the config watcher and background checks can
    /// reach it.
    static CURRENT: RefCell<Option<Sidebar>> = const { RefCell::new(None) };
}

/// Runs `f` with this UI thread's sidebar, if one is installed.
pub fn with_current(f: impl FnOnce(&Sidebar)) {
    if let Some(sidebar) = CURRENT.with(|c| c.borrow().clone()) {
        f(&sidebar);
    }
}

#[derive(Clone)]
pub struct Sidebar(Rc<RefCell<Inner>>);

impl Sidebar {
    pub fn new(window: &AppWindow, nav: Navigator, store: Option<ConfigStore>) -> Sidebar {
        let rows = Rc::new(VecModel::default());
        window.set_sidebar_rows(ModelRc::from(rows.clone()));
        let sidebar = Sidebar(Rc::new(RefCell::new(Inner {
            window: window.as_weak(),
            nav: nav.clone(),
            store,
            dirs: KnownDirs::system(),
            pins: PinState::default(),
            drive_signature: gezik_platform::drive_signature(),
            rows,
            poll: slint::Timer::default(),
            pins_on_their_way: 0,
            pins_in_file: Vec::new(),
        })));
        // Re-highlight on location changes, relabel after places reload. No I/O here: this
        // runs on every navigation.
        let weak = Rc::downgrade(&sidebar.0);
        nav.on_changed(move |_| {
            if let Some(inner) = weak.upgrade() {
                Sidebar(inner).update_rows();
            }
        });
        sidebar.start_drive_polling();
        sidebar.update_rows();
        sidebar
    }

    /// Makes this sidebar reachable from the config watcher and background checks. Call
    /// once, right after `new`, before any pins are set.
    pub fn install(&self) {
        CURRENT.with(|c| *c.borrow_mut() = Some(self.clone()));
    }

    /// Checks the drive signature (cheap) every few seconds; reloads places only if it
    /// changed.
    fn start_drive_polling(&self) {
        let weak = Rc::downgrade(&self.0);
        self.0.borrow().poll.start(slint::TimerMode::Repeated, DRIVE_POLL, move || {
            let Some(inner) = weak.upgrade() else { return };
            let signature = gezik_platform::drive_signature();
            let (changed, window) = {
                let mut inner = inner.borrow_mut();
                let changed = signature != inner.drive_signature;
                inner.drive_signature = signature;
                (changed, inner.window.clone())
            };
            if changed {
                // A pinned folder may have appeared or gone with the drive.
                Sidebar(inner).refresh();
                crate::places::load_in_background(window, |part| {
                    crate::navigation::with_current(|nav| nav.set_places(part));
                });
            }
        });
    }

    /// Sets the pinned entries as written in settings.toml (at start and on every settings
    /// reload). Does nothing if they are unchanged, so the settings reload that follows our own
    /// save costs nothing; nor while our own saves are on their way (the list shown is what
    /// they write, and the next edit builds on it).
    pub fn set_pinned(&self, pinned: Vec<PinEntry>) {
        let writing = {
            let mut inner = self.0.borrow_mut();
            inner.pins_in_file = pinned.clone();
            inner.pins_on_their_way > 0
        };
        if !writing {
            self.show_pinned(pinned);
        }
    }

    fn show_pinned(&self, pinned: Vec<PinEntry>) {
        let changed = self.0.borrow_mut().pins.set(pinned);
        if changed {
            self.update_rows();
            self.refresh();
        }
    }

    pub fn pinned(&self) -> Vec<PinEntry> {
        self.0.borrow().pins.pinned.clone()
    }

    /// Whether `path` is shown in the PINNED section.
    pub fn is_pinned(&self, path: &Path) -> bool {
        self.visible_pinned_index(path).is_some()
    }

    /// Changes the pinned list with `change`; saves it if `change` says it changed.
    fn edit(&self, change: impl FnOnce(&mut Vec<PinEntry>) -> bool) {
        let mut pinned = self.pinned();
        if change(&mut pinned) {
            self.save(pinned);
        }
    }

    pub fn pin(&self, path: PathBuf) {
        let entry = self.0.borrow().dirs.collapse(&path);
        self.edit(|list| pins::pin(list, entry));
    }

    /// Pins the folders `paths` at shown position `position` of the PINNED section (dropped
    /// between two pinned folders); a folder already pinned moves there.
    pub fn pin_at(&self, paths: &[PathBuf], position: usize) {
        let (entries, anchor) = {
            let inner = self.0.borrow();
            let entries: Vec<String> = paths.iter().map(|path| inner.dirs.collapse(path)).collect();
            let last = inner.pins.visible.len().checked_sub(1);
            let anchor = match inner.pins.stored_index(position) {
                Some(i) => Some((i, false)),
                None => last.and_then(|last| inner.pins.stored_index(last)).map(|i| (i, true)),
            };
            (entries, anchor)
        };
        self.edit(|list| match anchor {
            Some((at, after)) => pins::place(list, entries, at, after),
            None => entries.into_iter().fold(false, |changed, entry| pins::pin(list, entry) | changed),
        });
    }

    /// How many rows the PINNED section shows.
    pub fn visible_pinned_count(&self) -> usize {
        self.0.borrow().pins.visible.len()
    }

    /// The PINNED section row that shows `path`, if any.
    pub fn visible_pinned_index(&self, path: &Path) -> Option<usize> {
        self.0.borrow().pins.visible.iter().position(|pin| same_path(&pin.path, path))
    }

    /// Unpins the shown pinned row `index` (index within the PINNED section).
    pub fn unpin(&self, index: usize) {
        let stored = self.0.borrow().pins.stored_index(index);
        if let Some(stored) = stored {
            self.edit(|list| pins::unpin(list, stored));
        }
    }

    /// Unpins the shown pinned entry whose folder is `path`.
    pub fn unpin_path(&self, path: &Path) {
        if let Some(index) = self.visible_pinned_index(path) {
            self.unpin(index);
        }
    }

    /// Moves shown pinned row `from` to shown position `to` (clamped).
    pub fn move_pinned(&self, from: usize, to: usize) {
        let target = {
            let pins = &self.0.borrow().pins;
            let to = to.min(pins.visible.len().saturating_sub(1));
            pins.stored_index(from).zip(pins.stored_index(to)).map(|(f, t)| (f, t, to > from))
        };
        let Some((from, to, after)) = target else { return };
        self.edit(|list| {
            let path = list[from].path.clone();
            pins::place(list, vec![path], to, after)
        });
    }

    /// Shows `pinned` at once and has the settings writer thread put it into settings.toml;
    /// the config watcher reloads settings afterwards with the same list, which `set_pinned`
    /// ignores. If the write fails, the status bar says why and the list goes back to what the
    /// file has.
    fn save(&self, pinned: Vec<PinEntry>) {
        let Some(store) = self.0.borrow().store.clone() else {
            return self.say(&Warning::new("settings.toml", "no config folder; pins are not saved"));
        };
        self.0.borrow_mut().pins_on_their_way += 1;
        self.show_pinned(pinned.clone());
        store.write_settings(SettingsChange::Pinned(pinned.clone()), move |result| {
            let _ = slint::invoke_from_event_loop(move || with_current(|sidebar| sidebar.pins_written(pinned, result)));
        });
    }

    /// The settings writer is done with one pin list.
    fn pins_written(&self, pinned: Vec<PinEntry>, result: Result<(), Warning>) {
        let (last, in_file) = {
            let mut inner = self.0.borrow_mut();
            inner.pins_on_their_way = inner.pins_on_their_way.saturating_sub(1);
            if result.is_ok() {
                inner.pins_in_file = pinned;
            }
            (inner.pins_on_their_way == 0, inner.pins_in_file.clone())
        };
        if let Err(warning) = &result {
            self.say(warning);
        }
        // The last one written (or failed): the list shown is the file's.
        if last {
            self.show_pinned(in_file);
        }
    }

    fn say(&self, warning: &Warning) {
        let window = self.0.borrow().window.upgrade();
        if let Some(window) = window {
            window.set_status(warning.to_string().into());
        }
    }

    /// Where sidebar item (`section`, `index`) leads.
    pub fn location_of(&self, section: i32, index: i32) -> Option<Location> {
        let inner = self.0.borrow();
        let index = usize::try_from(index).ok()?;
        let places = inner.nav.places();
        match section {
            SECTION_FOLDERS => places.known.get(index).map(|f| Location::Path(f.path.clone())),
            SECTION_PINNED => inner.pins.visible.get(index).map(|pin| Location::Path(pin.path.clone())),
            SECTION_DRIVES => places.drives.get(index).map(|d| Location::Path(d.path.clone())),
            _ => None,
        }
    }

    /// Checks on a background thread which pinned entries exist on this machine, then
    /// shows them. Until then the PINNED section keeps what it shows.
    pub fn refresh(&self) {
        let (ticket, dirs, pinned, window) = {
            let mut inner = self.0.borrow_mut();
            let ticket = inner.pins.begin_check();
            (ticket, inner.dirs.clone(), inner.pins.pinned.clone(), inner.window.clone())
        };
        std::thread::spawn(move || {
            let visible = check_pins(&dirs, &pinned, Path::is_dir);
            let _ = window.upgrade_in_event_loop(move |_| {
                with_current(|sidebar| sidebar.finish_check(ticket, visible));
            });
        });
    }

    fn finish_check(&self, ticket: u64, visible: Vec<Pin>) {
        let applied = self.0.borrow_mut().pins.finish_check(ticket, visible);
        if applied {
            self.update_rows();
        }
    }

    /// Rebuilds the rows: sections, labels and the highlight of the exact current location.
    /// No file system access.
    fn update_rows(&self) {
        let inner = self.0.borrow();
        let Some(window) = inner.window.upgrade() else { return };
        let places = inner.nav.places();
        let current = inner.nav.active_location();
        let is_current = |path: &Path| matches!(&current, Location::Path(p) if same_path(p, path));
        let index = |i: usize| i32::try_from(i).unwrap_or(i32::MAX);
        let header =
            |label: &str, section| SidebarRow { header: true, label: label.into(), section, index: -1, active: false };
        let item = |label: &str, section, i, path: &Path| SidebarRow {
            header: false,
            label: label.into(),
            section,
            index: index(i),
            active: is_current(path),
        };

        let mut rows = vec![header("FOLDERS", SECTION_FOLDERS)];
        rows.extend(places.known.iter().enumerate().map(|(i, f)| item(&f.name, SECTION_FOLDERS, i, &f.path)));
        let visible = &inner.pins.visible;
        let mut pinned_first_row = -1;
        if !visible.is_empty() {
            rows.push(header("PINNED", SECTION_PINNED));
            pinned_first_row = index(rows.len());
            rows.extend(visible.iter().enumerate().map(|(i, pin)| {
                let label =
                    pin.entry.name.clone().unwrap_or_else(|| places.title_for(&Location::Path(pin.path.clone())));
                item(&label, SECTION_PINNED, i, &pin.path)
            }));
        }
        rows.push(header(DRIVES_HEADER, SECTION_DRIVES));
        rows.extend(places.drives.iter().enumerate().map(|(i, d)| item(&d.label, SECTION_DRIVES, i, &d.path)));

        window.set_sidebar_pinned_first_row(pinned_first_row);
        window.set_sidebar_pinned_count(index(visible.len()));
        sync_model(&inner.rows, rows.into_iter());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(items: &[&str]) -> Vec<PinEntry> {
        items.iter().map(|s| PinEntry::plain(*s)).collect()
    }

    fn pin(entry: &str, path: &str) -> Pin {
        Pin { entry: PinEntry::plain(entry), path: PathBuf::from(path) }
    }

    #[cfg(windows)]
    #[test]
    fn paths_match_ignoring_case_on_windows() {
        assert!(same_path(Path::new(r"C:\Users\Ali\Masaüstü"), Path::new("c:/users/ali/MASAÜSTÜ")));
        assert!(!same_path(Path::new(r"C:\Users\Ali"), Path::new(r"C:\Users\Ali\Docs")));
        assert!(!same_path(Path::new(r"C:\Users\Ali"), Path::new(r"C:\Users\Veli")));
    }

    #[cfg(not(windows))]
    #[test]
    fn paths_match_exactly_elsewhere() {
        assert!(same_path(Path::new("/home/a"), Path::new("/home/a/")));
        assert!(!same_path(Path::new("/home/a"), Path::new("/home/A")));
    }

    #[test]
    fn check_pins_keeps_existing_entries_in_order() {
        let dirs = KnownDirs::new(vec![("documents", PathBuf::from("/u/docs"))]);
        let pinned = plain(&["/gone", "{documents}", "/x/../y", "/work"]);
        let visible = check_pins(&dirs, &pinned, |p| p != Path::new("/gone"));
        // Entries with `..` are never expanded.
        assert_eq!(visible, [pin("{documents}", "/u/docs"), pin("/work", "/work")]);
    }

    #[test]
    fn a_new_list_keeps_known_pins_until_checked() {
        let mut pins = PinState::default();
        assert!(pins.set(plain(&["/a", "/b"])));
        assert!(pins.visible.is_empty(), "nothing is shown before the first check");
        let ticket = pins.begin_check();
        assert!(pins.finish_check(ticket, vec![pin("/a", "/a"), pin("/b", "/b")]));
        // Reordered with one added: known pins show at once in the new order, the new one
        // only once checked.
        assert!(pins.set(plain(&["/c", "/b", "/a"])));
        assert_eq!(pins.visible, [pin("/b", "/b"), pin("/a", "/a")]);
        assert_eq!(pins.stored_index(0), Some(1));
        assert_eq!(pins.stored_index(1), Some(2));
        assert_eq!(pins.stored_index(2), None);
        assert!(!pins.set(plain(&["/c", "/b", "/a"])), "an unchanged list is ignored");
    }

    #[test]
    fn an_overtaken_check_is_dropped() {
        let mut pins = PinState::default();
        pins.set(plain(&["/a"]));
        let old = pins.begin_check();
        pins.set(plain(&["/a", "/b"]));
        let new = pins.begin_check();
        assert!(!pins.finish_check(old, vec![pin("/a", "/a")]));
        assert!(pins.visible.is_empty());
        assert!(pins.finish_check(new, vec![pin("/a", "/a"), pin("/b", "/b")]));
        assert_eq!(pins.stored_index(1), Some(1));
    }

    #[test]
    fn a_new_alias_shows_at_once() {
        let mut pins = PinState::default();
        pins.set(plain(&["/a"]));
        let ticket = pins.begin_check();
        pins.finish_check(ticket, vec![pin("/a", "/a")]);
        let named = PinEntry { path: "/a".into(), name: Some("Alpha".into()), group: None };
        assert!(pins.set(vec![named.clone()]));
        assert_eq!(pins.visible, [Pin { entry: named, path: PathBuf::from("/a") }], "no check needed");
    }
}
