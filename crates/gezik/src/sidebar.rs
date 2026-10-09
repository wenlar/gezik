//! The sidebar: known folders, pinned folders and drives, kept up to date.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use gezik_config::Warning;
use gezik_config::paths::KnownDirs;
use gezik_config::pins::{self, PinEntry};
use gezik_config::settings_writer::SettingsChange;
use gezik_config::shortcuts::{Action, Platform};
use gezik_config::store::ConfigStore;
use gezik_core::nav::Location;
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::navigation::{Navigator, sync_model};
use crate::{AppWindow, SidebarRow};

pub const SECTION_FOLDERS: i32 = 0;
pub const SECTION_PINNED: i32 = 1;
pub const SECTION_DRIVES: i32 = 2;
pub const SECTION_GROUP: i32 = 3;
pub const SECTION_SEARCHES: i32 = 4;

/// `SidebarRow.icon`: the glyph sidebar.slint draws on a place (0: none, on headings).
const ICON_HOME: i32 = 1;
const ICON_FOLDER: i32 = 2;
const ICON_PIN: i32 = 3;
const ICON_ALIAS: i32 = 4;
const ICON_DRIVE: i32 = 5;
const ICON_SEARCH: i32 = 6;

/// A known folder's icon: the house for the home folder, which `known_folders` puts first
/// and names "Home" (when it exists).
fn known_icon(index: usize, name: &str) -> i32 {
    if index == 0 && name == "Home" { ICON_HOME } else { ICON_FOLDER }
}

/// The drives' section title: Finder calls it "Locations".
const DRIVES_HEADER: &str = if cfg!(target_os = "macos") { "LOCATIONS" } else { "DRIVES" };

/// How often the (cheap) drive signature is checked.
const DRIVE_POLL: Duration = Duration::from_secs(3);

/// Whether `a` and `b` name the same location (see [`pins::same_path_text`]). Separators and
/// redundant `/` or `.` parts do not matter.
pub fn same_path(a: &Path, b: &Path) -> bool {
    a == b
        || (cfg!(windows)
            && a.components().count() == b.components().count()
            && a.components()
                .zip(b.components())
                .all(|(x, y)| pins::same_path_text(&x.as_os_str().to_string_lossy(), &y.as_os_str().to_string_lossy())))
}

/// The alias an answer to Rename… gives: none for an empty one or the folder's own name.
pub fn alias_for(answer: &str, folder_name: &str) -> Option<String> {
    let answer = answer.trim();
    (!answer.is_empty() && answer != folder_name).then(|| answer.to_owned())
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

/// A row of the pinned part of the sidebar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PinLine {
    /// "PINNED" (`None`: the pins without a group) or a group's heading.
    Header(Option<String>),
    /// Shown pin `n` (an index into the shown pins).
    Pin(usize),
}

/// The pinned part's rows for the shown pins (in `pins::normalize` order): a heading before
/// each run of one group, none before nothing (spec 6.2: a group shows only with a pin in it).
pub fn pin_lines(visible: &[Pin]) -> Vec<PinLine> {
    let mut lines = Vec::new();
    let mut current: Option<Option<&str>> = None;
    for (n, pin) in visible.iter().enumerate() {
        let group = pin.entry.group.as_deref();
        if current != Some(group) {
            lines.push(PinLine::Header(group.map(str::to_owned)));
            current = Some(group);
        }
        lines.push(PinLine::Pin(n));
    }
    lines
}

/// Where a pin dropped on a line goes: next to shown pin `pin`, before it or `after` it, in
/// its group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinSlot {
    pub pin: usize,
    pub after: bool,
}

/// The slot of the line above row `line` of the pinned part (`lines.len()`: below the last):
/// above a pin, before it; above a heading or below the last, after the pin before it.
pub fn slot_at(lines: &[PinLine], line: usize) -> Option<PinSlot> {
    if let Some(PinLine::Pin(n)) = lines.get(line) {
        return Some(PinSlot { pin: *n, after: false });
    }
    match line.checked_sub(1).and_then(|i| lines.get(i)) {
        Some(PinLine::Pin(n)) => Some(PinSlot { pin: *n, after: true }),
        _ => None,
    }
}

/// What a pinned row shows when the pointer rests on it: its full path, and the key of its
/// number (`pin-N`, the first nine).
pub fn pin_tip(path: &Path, key: Option<&str>) -> String {
    match key {
        Some(key) => format!("{}\n{key}", path.display()),
        None => path.display().to_string(),
    }
}

/// The shown pin next to shown pin `n` (before it with `up`) if it is in the same group.
fn group_neighbour(visible: &[Pin], n: usize, up: bool) -> Option<usize> {
    let m = if up { n.checked_sub(1)? } else { n + 1 };
    let (this, other) = (visible.get(n)?, visible.get(m)?);
    (this.entry.group == other.entry.group).then_some(m)
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
    /// The pinned part as last drawn: its rows, the sidebar row of its first, the groups shown.
    lines: Vec<PinLine>,
    first_pin_row: Option<usize>,
    groups: Vec<String>,
    dialogs: crate::dialog::Dialogs,
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
    /// The pinned folders shown with their labels (the palette's Pinned).
    pub fn pinned_places(&self) -> Vec<(String, PathBuf)> {
        let inner = self.0.borrow();
        let places = inner.nav.places();
        inner
            .pins
            .visible
            .iter()
            .map(|pin| {
                let label =
                    pin.entry.name.clone().unwrap_or_else(|| places.title_for(&Location::Path(pin.path.clone())));
                (label, pin.path.clone())
            })
            .collect()
    }

    pub fn new(
        window: &AppWindow,
        nav: Navigator,
        store: Option<ConfigStore>,
        dialogs: crate::dialog::Dialogs,
    ) -> Sidebar {
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
            lines: Vec::new(),
            first_pin_row: None,
            groups: Vec::new(),
            dialogs,
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

    /// The shown pin on sidebar row `row`, as its index in the pinned list.
    fn pin_on_row(&self, row: usize) -> Option<usize> {
        let inner = self.0.borrow();
        match inner.lines.get(row.checked_sub(inner.first_pin_row?)?)? {
            PinLine::Pin(n) => inner.pins.stored_index(*n),
            PinLine::Header(_) => None,
        }
    }

    /// Where a pin dropped on the line above sidebar row `row` goes: next to which pin of the
    /// list, and whether after it.
    fn anchor_at_row(&self, row: usize) -> Option<(usize, bool)> {
        let inner = self.0.borrow();
        let slot = slot_at(&inner.lines, row.checked_sub(inner.first_pin_row?)?)?;
        Some((inner.pins.stored_index(slot.pin)?, slot.after))
    }

    /// The pinned row on sidebar row `from_row` was dragged to the line above row `line_row`: it
    /// goes there, into that group (spec 6.2).
    pub fn drop_pinned(&self, from_row: usize, line_row: usize) {
        let (Some(pin), Some((anchor, after))) = (self.pin_on_row(from_row), self.anchor_at_row(line_row)) else {
            return;
        };
        self.edit(|list| {
            let path = list[pin].path.clone();
            pins::place(list, vec![path], anchor, after)
        });
    }

    /// Folders dropped on the line above sidebar row `row` (drag.rs `Hit::PinAt`) are pinned
    /// there, in that group; a folder already pinned moves there.
    pub fn pin_at_row(&self, paths: &[PathBuf], row: usize) {
        let entries: Vec<String> = {
            let inner = self.0.borrow();
            paths.iter().map(|path| inner.dirs.collapse(path)).collect()
        };
        let anchor = self.anchor_at_row(row);
        self.edit(|list| match anchor {
            Some((at, after)) => pins::place(list, entries, at, after),
            None => entries.into_iter().fold(false, |changed, entry| pins::pin(list, entry) | changed),
        });
    }

    /// Whether the shown pin of `path` is the first and the last of its group (Move up/down).
    pub fn group_ends(&self, path: &Path) -> (bool, bool) {
        let inner = self.0.borrow();
        let visible = &inner.pins.visible;
        let Some(n) = visible.iter().position(|pin| same_path(&pin.path, path)) else { return (true, true) };
        (group_neighbour(visible, n, true).is_none(), group_neighbour(visible, n, false).is_none())
    }

    /// Move up / Move down: past the shown pin before or after it, in its group (the pins not on
    /// this machine stay where they are).
    pub fn move_in_group(&self, path: &Path, up: bool) {
        let target = {
            let inner = self.0.borrow();
            let visible = &inner.pins.visible;
            let n = visible.iter().position(|pin| same_path(&pin.path, path));
            n.and_then(|n| group_neighbour(visible, n, up).zip(Some(n)))
                .and_then(|(m, n)| inner.pins.stored_index(n).zip(inner.pins.stored_index(m)))
        };
        let Some((from, to)) = target else { return };
        self.edit(|list| {
            let path = list[from].path.clone();
            pins::place(list, vec![path], to, !up)
        });
    }

    /// The groups shown, in order (a group heading's `index` is its place here).
    pub fn shown_groups(&self) -> Vec<String> {
        self.0.borrow().groups.clone()
    }

    /// The group of the shown pin of `path`.
    pub fn group_of(&self, path: &Path) -> Option<String> {
        let inner = self.0.borrow();
        inner.pins.visible.iter().find(|pin| same_path(&pin.path, path)).and_then(|pin| pin.entry.group.clone())
    }

    /// The shown pin of `path`: its path as settings.toml has it.
    fn entry_of(&self, path: &Path) -> Option<String> {
        let inner = self.0.borrow();
        inner.pins.visible.iter().find(|pin| same_path(&pin.path, path)).map(|pin| pin.entry.path.clone())
    }

    /// Rename… on a pin: its alias (spec 6.3); empty, or the folder's own name, takes it away.
    pub fn ask_alias(&self, path: &Path) {
        let found = {
            let inner = self.0.borrow();
            inner.pins.visible.iter().find(|pin| same_path(&pin.path, path)).map(|pin| {
                let folder = inner.nav.places().title_for(&Location::Path(pin.path.clone()));
                (pin.entry.path.clone(), pin.entry.name.clone().unwrap_or_else(|| folder.clone()), folder)
            })
        };
        let Some((entry, initial, folder)) = found else { return };
        let (this, dialogs) = (self.clone(), self.0.borrow().dialogs.clone());
        let message = "Name in the sidebar (empty: the folder's own):";
        dialogs.ask_text("Rename", message, initial, &["Rename", "Cancel"], move |answer| {
            let Some(answer) = answer else { return };
            let name = alias_for(&answer, &folder);
            this.edit(|list| pins::find(list, &entry).is_some_and(|i| pins::set_name(list, i, name.as_deref())));
        });
    }

    /// Move to group ▸ New group…: asks for its name; a name a group has joins that group.
    pub fn ask_new_group(&self, path: &Path) {
        let Some(entry) = self.entry_of(path) else { return };
        let (this, dialogs) = (self.clone(), self.0.borrow().dialogs.clone());
        dialogs.ask_text("New group", "Name for the group:", "", &["Create", "Cancel"], move |answer| {
            let Some(name) = answer.filter(|name| !name.trim().is_empty()) else { return };
            this.edit(|list| pins::find(list, &entry).is_some_and(|i| pins::set_group(list, i, Some(&name))));
        });
    }

    /// Move to group ▸ a group, or No group (`None`).
    pub fn set_group(&self, path: &Path, group: Option<&str>) {
        let Some(entry) = self.entry_of(path) else { return };
        self.edit(|list| pins::find(list, &entry).is_some_and(|i| pins::set_group(list, i, group)));
    }

    /// Move group up/down: past groups with no pin shown, so the heading always moves.
    pub fn move_group(&self, group: &str, up: bool) {
        let shown = self.shown_groups();
        self.edit(|list| pins::move_group(list, group, up, &shown));
    }

    /// Rename group…: asks for the new name; a name another group has joins the two.
    pub fn ask_group_name(&self, group: &str) {
        let (this, dialogs, old) = (self.clone(), self.0.borrow().dialogs.clone(), group.to_owned());
        dialogs.ask_text("Rename group", "Name for the group:", group, &["Rename", "Cancel"], move |answer| {
            if let Some(name) = answer {
                this.edit(|list| pins::rename_group(list, &old, &name));
            }
        });
    }

    pub fn ungroup(&self, group: &str) {
        self.edit(|list| pins::ungroup(list, group));
    }

    /// Where `pin-N` goes: shown pin `n` (0-based), in the sidebar's order (spec 6.4).
    pub fn pin_location(&self, n: usize) -> Option<Location> {
        self.0.borrow().pins.visible.get(n).map(|pin| Location::Path(pin.path.clone()))
    }

    /// The pins' tips name their keys: the rows again once the shortcuts changed.
    pub fn relabel(&self) {
        self.update_rows();
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

    /// The saved searches changed: SEARCHES follows.
    pub fn searches_changed(&self) {
        self.update_rows();
    }

    /// Rebuilds the rows: sections, labels, the pinned part's headings and tips, and the
    /// highlight of the exact current location. No file system access.
    fn update_rows(&self) {
        let (lines, first, groups) = {
            let inner = self.0.borrow();
            let Some(window) = inner.window.upgrade() else { return };
            let places = inner.nav.places();
            let current = inner.nav.active_location();
            let is_current = |path: &Path| matches!(&current, Location::Path(p) if same_path(p, path));
            let index = |i: usize| i32::try_from(i).unwrap_or(i32::MAX);
            let header = |label: &str, section, i| SidebarRow {
                header: true,
                label: label.into(),
                section,
                index: i,
                active: false,
                tip: "".into(),
                icon: 0,
            };
            let item = |label: &str, section, i, path: &Path, icon: i32| SidebarRow {
                header: false,
                label: label.into(),
                section,
                index: index(i),
                active: is_current(path),
                tip: "".into(),
                icon,
            };

            let mut rows = vec![header("FOLDERS", SECTION_FOLDERS, -1)];
            rows.extend(
                places
                    .known
                    .iter()
                    .enumerate()
                    .map(|(i, f)| item(&f.name, SECTION_FOLDERS, i, &f.path, known_icon(i, &f.name))),
            );
            let visible = &inner.pins.visible;
            let lines = pin_lines(visible);
            let first = (!lines.is_empty()).then_some(rows.len());
            let mut groups: Vec<String> = Vec::new();
            for line in &lines {
                rows.push(match line {
                    PinLine::Header(None) => header("PINNED", SECTION_PINNED, -1),
                    PinLine::Header(Some(group)) => {
                        groups.push(group.clone());
                        header(group.as_str(), SECTION_GROUP, index(groups.len() - 1))
                    }
                    PinLine::Pin(n) => {
                        let pin = &visible[*n];
                        let label = pin
                            .entry
                            .name
                            .clone()
                            .unwrap_or_else(|| places.title_for(&Location::Path(pin.path.clone())));
                        let key = Action::pin(n + 1)
                            .and_then(crate::keys::chord_for)
                            .map(|chord| crate::keys::chord_label(&chord, Platform::current()));
                        SidebarRow {
                            tip: pin_tip(&pin.path, key.as_deref()).into(),
                            ..item(
                                &label,
                                SECTION_PINNED,
                                *n,
                                &pin.path,
                                if pin.entry.name.is_some() { ICON_ALIAS } else { ICON_PIN },
                            )
                        }
                    }
                });
            }
            let end = first.map(|_| rows.len());
            let searches = crate::saved_searches::names();
            if !searches.is_empty() {
                rows.push(header("SEARCHES", SECTION_SEARCHES, -1));
                rows.extend(searches.iter().enumerate().map(|(i, name)| SidebarRow {
                    header: false,
                    label: name.as_str().into(),
                    section: SECTION_SEARCHES,
                    index: index(i),
                    active: false,
                    tip: "".into(),
                    icon: ICON_SEARCH,
                }));
            }
            rows.push(header(DRIVES_HEADER, SECTION_DRIVES, -1));
            rows.extend(
                places.drives.iter().enumerate().map(|(i, d)| item(&d.label, SECTION_DRIVES, i, &d.path, ICON_DRIVE)),
            );

            let as_row = |row: Option<usize>| row.map_or(-1, index);
            window.set_sidebar_pinned_first_row(as_row(first));
            window.set_sidebar_pinned_end_row(as_row(end));
            sync_model(&inner.rows, rows.into_iter());
            (lines, first, groups)
        };
        let mut inner = self.0.borrow_mut();
        inner.lines = lines;
        inner.first_pin_row = first;
        inner.groups = groups;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_first_known_folder_called_home_gets_the_house() {
        assert_eq!(known_icon(0, "Home"), ICON_HOME);
        assert_eq!(known_icon(0, "Desktop"), ICON_FOLDER, "no home folder on this machine");
        assert_eq!(known_icon(3, "Home"), ICON_FOLDER);
    }

    #[test]
    fn an_alias_is_dropped_when_empty_or_the_folders_own_name() {
        assert_eq!(alias_for(" Gezik ", "gezik").as_deref(), Some("Gezik"));
        assert_eq!(alias_for("   ", "gezik"), None);
        assert_eq!(alias_for("gezik", "gezik"), None);
    }

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

    fn grouped(path: &str, group: Option<&str>) -> Pin {
        Pin {
            entry: PinEntry { path: path.into(), name: None, group: group.map(str::to_owned) },
            path: PathBuf::from(path),
        }
    }

    fn entry(path: &str, group: Option<&str>) -> PinEntry {
        PinEntry { path: path.into(), name: None, group: group.map(str::to_owned) }
    }

    #[test]
    fn pin_lines_skip_hidden_pins_and_empty_groups() {
        // The stored list has a pin that is not on this machine ("/gone") and a group whose only
        // pin is not either ("Media"); the check leaves the rest, as the sidebar sees it.
        let mut state = PinState::default();
        state.set(vec![
            entry("/a", None),
            entry("/gone", None),
            entry("/w1", Some("Work")),
            entry("/w2", Some("Work")),
            entry("/m", Some("Media")),
            entry("/p", Some("Photos")),
        ]);
        let ticket = state.begin_check();
        let found = [
            grouped("/a", None),
            grouped("/w1", Some("Work")),
            grouped("/w2", Some("Work")),
            grouped("/p", Some("Photos")),
        ];
        assert!(state.finish_check(ticket, found.to_vec()));
        let lines = pin_lines(&state.visible);
        assert_eq!(
            lines,
            [
                PinLine::Header(None),
                PinLine::Pin(0),
                PinLine::Header(Some("Work".into())),
                PinLine::Pin(1),
                PinLine::Pin(2),
                PinLine::Header(Some("Photos".into())),
                PinLine::Pin(3),
            ]
        );
        assert!(!lines.contains(&PinLine::Header(Some("Media".into()))), "no heading for a group with nothing shown");
        // Every line's pin is a shown one; the hidden ones have none and the numbers skip them.
        let shown: Vec<&str> = lines
            .iter()
            .filter_map(|line| match line {
                PinLine::Pin(n) => Some(state.visible[*n].entry.path.as_str()),
                PinLine::Header(_) => None,
            })
            .collect();
        assert_eq!(shown, ["/a", "/w1", "/w2", "/p"]);
        assert_eq!(state.stored_index(3), Some(5), "the shown /p is the sixth stored pin");
        let only_grouped = [grouped("/w", Some("Work"))];
        assert_eq!(
            pin_lines(&only_grouped),
            [PinLine::Header(Some("Work".into())), PinLine::Pin(0)],
            "no PINNED heading without a pin under it"
        );
        assert!(pin_lines(&[]).is_empty());
    }

    #[test]
    fn a_line_finds_its_slot() {
        let lines = pin_lines(&[grouped("/a", None), grouped("/w1", Some("Work")), grouped("/w2", Some("Work"))]);
        // 0 PINNED, 1 /a, 2 Work, 3 /w1, 4 /w2; a line is drawn above row n (5: below the last).
        assert_eq!(slot_at(&lines, 0), None, "above the first heading");
        assert_eq!(slot_at(&lines, 1), Some(PinSlot { pin: 0, after: false }));
        assert_eq!(
            slot_at(&lines, 2),
            Some(PinSlot { pin: 0, after: true }),
            "above a heading: after the pin before it"
        );
        assert_eq!(slot_at(&lines, 3), Some(PinSlot { pin: 1, after: false }), "the group's first place");
        assert_eq!(slot_at(&lines, 5), Some(PinSlot { pin: 2, after: true }), "below the last");
        assert_eq!(slot_at(&lines, 6), None);
    }

    #[test]
    fn a_drop_past_the_end_joins_the_last_pins_group() {
        // The last section is a group: below its last pin is that group's end (spec 6.2).
        let stored = vec![entry("/a", None), entry("/w1", Some("Work")), entry("/w2", Some("Work"))];
        let mut state = PinState::default();
        state.set(stored.clone());
        let ticket = state.begin_check();
        state.finish_check(
            ticket,
            vec![grouped("/a", None), grouped("/w1", Some("Work")), grouped("/w2", Some("Work"))],
        );
        let lines = pin_lines(&state.visible);
        let slot = slot_at(&lines, lines.len()).unwrap();
        assert_eq!(slot, PinSlot { pin: 2, after: true });
        let mut list = state.pinned.clone();
        assert!(pins::place(&mut list, vec!["/new".into()], state.stored_index(slot.pin).unwrap(), slot.after));
        assert_eq!(list.last().unwrap(), &entry("/new", Some("Work")), "in the last group, after its last pin");
        // The line above the heading of that group is the end of the group before it instead.
        let slot = slot_at(&lines, 2).unwrap();
        let mut list = state.pinned.clone();
        assert!(pins::place(&mut list, vec!["/new".into()], state.stored_index(slot.pin).unwrap(), slot.after));
        assert_eq!(list[1], entry("/new", None), "after /a, ungrouped");
    }

    #[test]
    fn a_neighbour_must_share_the_group() {
        let visible = [grouped("/a", None), grouped("/w1", Some("Work")), grouped("/w2", Some("Work"))];
        assert_eq!(group_neighbour(&visible, 0, true), None);
        assert_eq!(group_neighbour(&visible, 0, false), None, "the next pin is in another group");
        assert_eq!(group_neighbour(&visible, 1, true), None);
        assert_eq!(group_neighbour(&visible, 1, false), Some(2));
        assert_eq!(group_neighbour(&visible, 2, true), Some(1));
        assert_eq!(group_neighbour(&visible, 2, false), None);
    }

    #[test]
    fn a_pins_tip_is_its_path_and_its_key() {
        let path = Path::new("/home/a/Projects");
        assert_eq!(pin_tip(path, Some("Alt+1")), format!("{}\nAlt+1", path.display()));
        assert_eq!(pin_tip(path, None), path.display().to_string());
    }
}
