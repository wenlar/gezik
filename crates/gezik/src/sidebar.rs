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
use gezik_core::tree::{self, Listed, NodeId, Read, RootKey};
use slint::{ComponentHandle, ModelRc};

use crate::navigation::Navigator;
use crate::places::Places;
use crate::sidebar_model::{Line, Shown, SidebarModel, base_at, change_plan, row_of_base, scroll_to_show};
use crate::{AppWindow, SidebarRow};

pub const SECTION_FOLDERS: i32 = 0;
pub const SECTION_PINNED: i32 = 1;
pub const SECTION_DRIVES: i32 = 2;
pub const SECTION_GROUP: i32 = 3;
pub const SECTION_SEARCHES: i32 = 4;
/// The trash's one row, under the drives (spec 7.1).
pub const SECTION_TRASH: i32 = 5;
/// The cloud roots (spec 9 §7.2), after the pinned part.
pub const SECTION_CLOUD: i32 = 6;
/// A folder of the sidebar tree (spec 10 §5), `index` its node.
pub const SECTION_TREE: i32 = 7;
/// A capped branch's "… n more" line, `index` the branch's node: it opens that folder.
pub const SECTION_TREE_MORE: i32 = 8;

/// `SidebarRow.icon`: the glyph sidebar.slint draws on a place (0: none, on headings).
const ICON_HOME: i32 = 1;
pub(crate) const ICON_FOLDER: i32 = 2;
const ICON_PIN: i32 = 3;
const ICON_ALIAS: i32 = 4;
const ICON_DRIVE: i32 = 5;
const ICON_SEARCH: i32 = 6;
const ICON_TRASH: i32 = 7;
const ICON_CLOUD: i32 = 8;

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

/// The CLOUD section's rows: a heading and one row per root; nothing without a root or when
/// turned off (`[sidebar] cloud`).
pub fn cloud_rows(roots: &[gezik_platform::cloud::CloudRoot], on: bool) -> Vec<SidebarRow> {
    if !on || roots.is_empty() {
        return Vec::new();
    }
    let mut rows = vec![SidebarRow {
        header: true,
        label: "CLOUD".into(),
        section: SECTION_CLOUD,
        index: -1,
        active: false,
        tip: "".into(),
        icon: 0,
        depth: 0,
        arrow: 0,
    }];
    rows.extend(roots.iter().enumerate().map(|(i, root)| {
        let path = root.path.display().to_string();
        let tip = if root.account.is_empty() { path } else { format!("{}\n{path}", root.account) };
        SidebarRow {
            header: false,
            label: root.label.as_str().into(),
            section: SECTION_CLOUD,
            index: i32::try_from(i).unwrap_or(i32::MAX),
            active: false,
            tip: tip.into(),
            icon: ICON_CLOUD,
            depth: 0,
            arrow: 0,
        }
    }));
    rows
}

/// The folder of place (`section`, `index`): a known folder, a shown pin, a drive or a cloud
/// root; the root of its tree branch (spec 10 §5.1). Not a saved search, nor the trash.
fn place_path(places: &Places, pins: &[Pin], section: i32, index: i32) -> Option<PathBuf> {
    let index = usize::try_from(index).ok()?;
    match section {
        SECTION_FOLDERS => places.known.get(index).map(|f| f.path.clone()),
        SECTION_PINNED => pins.get(index).map(|pin| pin.path.clone()),
        SECTION_DRIVES => places.drives.get(index).map(|d| d.path.clone()),
        SECTION_CLOUD => places.cloud.get(index).map(|r| r.path.clone()),
        _ => None,
    }
}

/// Reads a branch's sub-folders for `read`, on a worker (spec 10 §5.2): a `\\server`'s shares,
/// else the sub-folders the list would show (nothing opened, no cloud file downloaded), sorted
/// and capped here, and where the folder really is for the loop guard (`canonicalize` for
/// every branch, so all the real paths compared have one form). The tree's only read.
fn read_branch(path: &Path, options: gezik_core::view::ViewOptions) -> Result<Listed, String> {
    let names = match crate::navigation::server_of(path) {
        Some(server) => gezik_platform::network::shares(&server),
        None => crate::path_box::subfolder_names(path, options.show_hidden, options.show_system),
    };
    let names = names.map_err(|err| gezik_platform::fs::describe(&err))?;
    Ok(tree::prepare(names, std::fs::canonicalize(path).ok()))
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
    /// What the sidebar shows (places and open branches), and its model for Slint.
    shown: Rc<RefCell<Shown>>,
    model: Rc<SidebarModel>,
    poll: slint::Timer,
    /// Pin lists sent to the settings writer and not yet written (or failed).
    pins_on_their_way: usize,
    /// The pinned list settings.toml has, as last read or written.
    pins_in_file: Vec<PinEntry>,
    /// The pinned part as last drawn: its lines, the base row (`Shown::base`) of its first and
    /// of the one after its last, the groups shown.
    pin_lines: Vec<PinLine>,
    first_pin_base: Option<usize>,
    end_pin_base: Option<usize>,
    groups: Vec<String>,
    dialogs: crate::dialog::Dialogs,
    /// `[sidebar] cloud`.
    show_cloud: bool,
    /// `[sidebar] tree-follow`.
    follow: bool,
    /// The folder last shown (`None`: not a folder), so a reload of the same one follows nothing.
    followed: Option<PathBuf>,
    /// The folder the tree is opening down to, and whether the user asked (Show in Sidebar Tree).
    pending: Option<(PathBuf, bool)>,
}

/// Where an opening of the tree got to (`Sidebar::continue_reveal`).
enum RevealStep {
    Place(RootKey),
    Node(NodeId),
    Wait(Option<Read>),
    Missing,
    /// Under no place (or the places not loaded yet).
    Unplaced,
}

/// The row of the place or folder an opening reached.
fn reveal_row(shown: &Shown, step: &RevealStep) -> Option<usize> {
    match step {
        RevealStep::Place(key) => {
            let k = shown.keys.iter().position(|k| k.as_ref() == Some(key))?;
            row_of_base(&shown.base_rows, k)
        }
        RevealStep::Node(id) => shown.lines.iter().position(|line| *line == Line::Tree(tree::Line::Node(*id))),
        RevealStep::Wait(_) | RevealStep::Missing | RevealStep::Unplaced => None,
    }
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
        let shown = Rc::new(RefCell::new(Shown::default()));
        let model = Rc::new(SidebarModel::new(shown.clone()));
        window.set_sidebar_rows(ModelRc::from(model.clone()));
        let sidebar = Sidebar(Rc::new(RefCell::new(Inner {
            window: window.as_weak(),
            nav: nav.clone(),
            store,
            dirs: KnownDirs::system(),
            pins: PinState::default(),
            drive_signature: gezik_platform::drive_signature(),
            shown,
            model,
            poll: slint::Timer::default(),
            pins_on_their_way: 0,
            pins_in_file: Vec::new(),
            pin_lines: Vec::new(),
            first_pin_base: None,
            end_pin_base: None,
            groups: Vec::new(),
            dialogs,
            show_cloud: true,
            follow: false,
            followed: None,
            pending: None,
        })));
        // Re-highlight on location changes, relabel after places reload. No I/O here: this
        // runs on every navigation (tree-follow's reads are on workers).
        let weak = Rc::downgrade(&sidebar.0);
        nav.on_changed(move |location| {
            if let Some(inner) = weak.upgrade() {
                let sidebar = Sidebar(inner);
                sidebar.update_rows();
                sidebar.location_changed(location);
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
            if let Some(inner) = weak.upgrade() {
                Sidebar(inner).check_drives(false);
            }
        });
    }

    /// Reads the drives again if their signature changed, or always with `force` (after Gezik
    /// connected or ejected one: a macOS or gvfs network mount may not change the signature).
    pub fn check_drives(&self, force: bool) {
        let signature = gezik_platform::drive_signature();
        let (changed, window) = {
            let mut inner = self.0.borrow_mut();
            let changed = force || signature != inner.drive_signature;
            inner.drive_signature = signature;
            (changed, inner.window.clone())
        };
        if changed {
            // A pinned folder may have appeared or gone with the drive.
            self.refresh();
            crate::places::load_in_background(window, |part| {
                crate::navigation::with_current(|nav| nav.set_places(part));
            });
        }
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
        let Line::Base(k) = *inner.shown.try_borrow().ok()?.lines.get(row)? else { return None };
        match inner.pin_lines.get(k.checked_sub(inner.first_pin_base?)?)? {
            PinLine::Pin(n) => inner.pins.stored_index(*n),
            PinLine::Header(_) => None,
        }
    }

    /// Where a pin dropped on the line above sidebar row `row` goes: next to which pin of the
    /// list, and whether after it. Rows of an open branch count as the place after them.
    fn anchor_at_row(&self, row: usize) -> Option<(usize, bool)> {
        let inner = self.0.borrow();
        let k = base_at(&inner.shown.try_borrow().ok()?.base_rows, row);
        let slot = slot_at(&inner.pin_lines, k.checked_sub(inner.first_pin_base?)?)?;
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
        self.say_text(warning.to_string());
    }

    fn say_text(&self, text: String) {
        let window = self.0.borrow().window.upgrade();
        if let Some(window) = window {
            window.set_status(text.into());
        }
    }

    /// Where sidebar item (`section`, `index`) leads.
    pub fn location_of(&self, section: i32, index: i32) -> Option<Location> {
        let inner = self.0.borrow();
        match section {
            SECTION_TRASH => (index == 0).then_some(Location::Trash),
            // A tree folder, or a capped branch's folder (its "… n more" line).
            SECTION_TREE | SECTION_TREE_MORE => {
                let shown = inner.shown.try_borrow().ok()?;
                shown.tree.node(u32::try_from(index).ok()?).map(|node| Location::Path(node.path.clone()))
            }
            _ => place_path(&inner.nav.places(), &inner.pins.visible, section, index).map(Location::Path),
        }
    }

    /// Changes what the sidebar shows with `change` (which notes the tree nodes whose own row
    /// changed), lays the rows out again if the tree or the places changed, and tells Slint only
    /// that: the rows that came or went, and the rows to draw again. The pinned part's first and
    /// end rows follow.
    fn publish<R>(&self, change: impl FnOnce(&mut Shown, &mut Vec<NodeId>) -> R) -> R {
        let (shown, model, window, first, end) = {
            let inner = self.0.borrow();
            (inner.shown.clone(), inner.model.clone(), inner.window.clone(), inner.first_pin_base, inner.end_pin_base)
        };
        let (result, plan, rows) = {
            let mut shown = shown.borrow_mut();
            let was = shown.current.clone();
            let mut touched = Vec::new();
            let result = change(&mut shown, &mut touched);
            let old = shown.relayout();
            // The highlight moved: the folders shown before and now are drawn again.
            let moved = was != shown.current && !shown.tree.is_empty();
            let lit = |id: NodeId| {
                moved
                    && shown.tree.node(id).is_some_and(|node| {
                        [&was, &shown.current].into_iter().flatten().any(|c| same_path(c, &node.path))
                    })
            };
            let scan = moved || !touched.is_empty();
            let plan = change_plan(
                old.as_deref(),
                &shown.lines,
                &shown.base_rows,
                &|id| touched.contains(&id) || lit(id),
                scan,
            );
            let row = |k: Option<usize>| k.and_then(|k| row_of_base(&shown.base_rows, k));
            let first_row = row(first);
            let end_row = first_row.map(|_| row(end).unwrap_or(shown.lines.len()));
            (result, plan, (first_row, end_row))
        };
        model.apply(plan);
        if let Some(window) = window.upgrade() {
            let as_row = |row: Option<usize>| row.map_or(-1, |r| i32::try_from(r).unwrap_or(i32::MAX));
            window.set_sidebar_pinned_first_row(as_row(rows.0));
            window.set_sidebar_pinned_end_row(as_row(rows.1));
        }
        result
    }

    /// The arrow of row `row` (spec 10 §5.1): its branch opens (read in the background) or closes.
    pub fn toggle_row(&self, row: usize) {
        // The user's own arrow ends an opening on its way (sapma 16).
        self.0.borrow_mut().pending = None;
        let read = self.publish(|shown, touched| match shown.lines.get(row).copied() {
            Some(Line::Base(k)) => shown.keys.get(k).cloned().flatten().and_then(|key| shown.tree.toggle_root(key)),
            Some(Line::Tree(tree::Line::Node(id))) => {
                touched.push(id);
                shown.tree.toggle(id)
            }
            _ => None,
        });
        self.start_reads(read.into_iter().collect());
    }

    /// Reads each folder off the UI thread, one thread each (a slow share holds only its own).
    fn start_reads(&self, reads: Vec<Read>) {
        if reads.is_empty() {
            return;
        }
        let window = self.0.borrow().window.clone();
        let options = crate::view_options::current();
        for read in reads {
            let window = window.clone();
            std::thread::spawn(move || {
                let found = read_branch(&read.path, options);
                let _ = window.upgrade_in_event_loop(move |_| with_current(|sidebar| sidebar.branch_read(read, found)));
            });
        }
    }

    /// A branch's read came back; a failed first read says why (spec 10 §5.2, sapma 6).
    fn branch_read(&self, read: Read, found: Result<Listed, String>) {
        let failed = found.as_ref().err().cloned();
        let changed = self.publish(|shown, touched| {
            touched.push(read.node);
            shown.tree.loaded(&read, found.ok())
        });
        if let Some(why) = failed {
            // A failed read ends an opening on its way (no loop opening the same folder again).
            self.0.borrow_mut().pending = None;
            if changed {
                self.say_text(format!("Cannot open {}: {why}", read.path.display()));
            }
        }
        self.continue_reveal();
    }

    /// A pane listed `folder` (spec 10 §5.2): its open branch shows the listing's sub-folders
    /// (the list's own hidden rule, `Entry::is_shown`), no read of its own. Costs nothing unless
    /// the tree has a branch there.
    pub fn listed(&self, folder: &Path, entries: &[gezik_core::Entry]) {
        let shown = self.0.borrow().shown.clone();
        if !shown.try_borrow().is_ok_and(|s| s.tree.has_branch_at(folder, &same_path)) {
            return;
        }
        let options = crate::view_options::current();
        let names = entries
            .iter()
            .filter(|e| e.is_dir && e.is_shown(options.show_hidden, options.show_system))
            .map(|e| e.name.clone())
            .collect();
        let found = tree::prepare(names, None);
        self.publish(|shown, touched| touched.extend(shown.tree.listed(folder, &found, &same_path)));
    }

    /// Gezik's own job changed `dirs` (spec 10 §5.2): their open branches are read again.
    pub fn folders_changed(&self, dirs: &[PathBuf]) {
        let shown = self.0.borrow().shown.clone();
        if shown.try_borrow().is_ok_and(|s| s.tree.is_empty()) {
            return;
        }
        let reads = self.publish(|shown, touched| {
            let (reads, rearmed) = shown.tree.rereads(dirs, &same_path);
            touched.extend(rearmed);
            reads
        });
        self.start_reads(reads);
    }

    /// Hidden or system items come or go: the open branches are read again.
    pub fn options_changed(&self) {
        let reads = self.publish(|shown, _| shown.tree.reread_all());
        self.start_reads(reads);
    }

    /// `[sidebar] tree-follow` (at start and on every settings reload); turned on, the tree
    /// opens down to the folder shown now.
    pub fn set_tree_follow(&self, on: bool) {
        let location = {
            let mut inner = self.0.borrow_mut();
            let was = std::mem::replace(&mut inner.follow, on);
            if !on || was {
                return;
            }
            inner.followed = None;
            inner.nav.active_location()
        };
        self.location_changed(&location);
    }

    /// The folder shown changed: with tree-follow on the tree opens down to it (spec 10 §5.3);
    /// an opening on its way for another folder ends either way. The same folder again (a
    /// reload, a listing that came in) changes nothing.
    fn location_changed(&self, location: &Location) {
        let path = match location {
            Location::Path(path) => Some(path),
            _ => None,
        };
        let follow = {
            let mut inner = self.0.borrow_mut();
            let same = match (path, &inner.followed) {
                (Some(a), Some(b)) => same_path(a, b),
                (a, b) => a.is_none() && b.is_none(),
            };
            if same {
                return;
            }
            inner.followed = path.cloned();
            inner.pending = None;
            inner.follow
        };
        if follow && let Some(path) = path {
            self.reveal(path.clone(), false);
        }
    }

    /// Opens the tree down to `target` under the nearest place (spec 10 §5.3), each closed
    /// folder above it read in turn, then scrolls its row into view. `asked`: the user asked
    /// (Show in Sidebar Tree), so a folder the tree cannot show is said.
    pub fn reveal(&self, target: PathBuf, asked: bool) {
        self.0.borrow_mut().pending = Some((target, asked));
        self.continue_reveal();
    }

    /// One step of the opening on its way, if any (again after each read).
    fn continue_reveal(&self) {
        let Some((target, asked)) = self.0.borrow().pending.clone() else { return };
        let step = self.publish(|shown, touched| {
            let places: Vec<RootKey> = shown.keys.iter().flatten().cloned().collect();
            let Some((key, rest)) = tree::nearest_place(&places, &target, &same_path) else {
                return RevealStep::Unplaced;
            };
            match shown.tree.reveal(key.clone(), &rest, &same_path) {
                tree::Reveal::Place => RevealStep::Place(key),
                tree::Reveal::Shown(id) => RevealStep::Node(id),
                tree::Reveal::Wait(read) => {
                    touched.extend(read.as_ref().map(|r| r.node));
                    RevealStep::Wait(read)
                }
                tree::Reveal::Missing => RevealStep::Missing,
            }
        });
        if let RevealStep::Wait(read) = step {
            return self.start_reads(read.into_iter().collect());
        }
        let row = {
            let mut inner = self.0.borrow_mut();
            inner.pending = None;
            if matches!(step, RevealStep::Unplaced) {
                // Followed again on the next change: the places may still be loading (at start).
                inner.followed = None;
            }
            inner.shown.try_borrow().ok().and_then(|shown| reveal_row(&shown, &step))
        };
        match row {
            Some(row) => self.scroll_to_row(row),
            None if asked => self
                .say_text(format!("{} is not in the sidebar tree (hidden, or under no place there)", target.display())),
            None => {}
        }
    }

    /// Show in Sidebar Tree (spec 10 §5.3): once, whatever tree-follow says.
    pub fn reveal_current(&self) {
        let (window, location) = {
            let inner = self.0.borrow();
            (inner.window.upgrade(), inner.nav.active_location())
        };
        let Some(window) = window else { return };
        if window.get_sidebar_position() == 2 {
            return self.say_text("The sidebar is hidden (settings.toml [layout] sidebar)".to_owned());
        }
        match location {
            Location::Path(path) => self.reveal(path, true),
            _ => self.say_text("Only a folder can be shown in the sidebar tree".to_owned()),
        }
    }

    /// Scrolls the sidebar so that row `row` shows whole.
    fn scroll_to_row(&self, row: usize) {
        let Some(window) = self.0.borrow().window.upgrade() else { return };
        let theme = window.global::<crate::Theme>();
        let height = window.get_drop_geometry().sidebar_height - 2.0 * theme.get_spacing();
        window.set_sidebar_scroll(scroll_to_show(row, theme.get_row_height(), window.get_sidebar_scroll(), height));
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

    /// `[sidebar] cloud` (at start and on every settings reload).
    pub fn set_show_cloud(&self, on: bool) {
        let changed = std::mem::replace(&mut self.0.borrow_mut().show_cloud, on) != on;
        if changed {
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
        let (rows, keys, current_path, lines, first, end, groups) = {
            let inner = self.0.borrow();
            if inner.window.upgrade().is_none() {
                return;
            }
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
                depth: 0,
                arrow: 0,
            };
            let item = |label: &str, section, i, path: &Path, icon: i32| SidebarRow {
                header: false,
                label: label.into(),
                section,
                index: index(i),
                active: is_current(path),
                tip: "".into(),
                icon,
                depth: 0,
                arrow: 0,
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
            let mut cloud = cloud_rows(&places.cloud, inner.show_cloud);
            for row in cloud.iter_mut().filter(|r| !r.header) {
                row.active = places.cloud.get(row.index as usize).is_some_and(|root| is_current(&root.path));
            }
            rows.extend(cloud);
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
                    depth: 0,
                    arrow: 0,
                }));
            }
            rows.push(header(DRIVES_HEADER, SECTION_DRIVES, -1));
            rows.extend(
                places.drives.iter().enumerate().map(|(i, d)| item(&d.label, SECTION_DRIVES, i, &d.path, ICON_DRIVE)),
            );
            rows.push(SidebarRow {
                header: false,
                label: gezik_core::nav::TRASH_NAME.into(),
                section: SECTION_TRASH,
                index: 0,
                active: current == Location::Trash,
                tip: "".into(),
                icon: ICON_TRASH,
                depth: 0,
                arrow: 0,
            });

            let current_path = match &current {
                Location::Path(path) => Some(path.clone()),
                _ => None,
            };
            let keys: Vec<Option<RootKey>> = rows
                .iter()
                .map(|row| {
                    let path = (!row.header).then(|| place_path(&places, visible, row.section, row.index)).flatten();
                    path.map(|path| (row.section, path))
                })
                .collect();
            (rows, keys, current_path, lines, first, end, groups)
        };
        {
            let mut inner = self.0.borrow_mut();
            inner.pin_lines = lines;
            inner.first_pin_base = first;
            inner.end_pin_base = end;
            inner.groups = groups;
        }
        let live: Vec<RootKey> = keys.iter().flatten().cloned().collect();
        self.publish(|shown, _| {
            shown.set_base(rows, keys);
            shown.current = current_path;
            // A place gone (an ejected drive, an unpinned folder) takes its branch.
            shown.tree.keep_roots(&live);
        });
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
    fn cloud_rows_come_under_their_heading_or_not_at_all() {
        let roots = [gezik_platform::cloud::CloudRoot {
            path: PathBuf::from("/u/OneDrive"),
            label: "OneDrive".into(),
            account: "Personal".into(),
        }];
        assert!(cloud_rows(&[], true).is_empty(), "no root: no heading");
        assert!(cloud_rows(&roots, false).is_empty(), "turned off");
        let rows = cloud_rows(&roots, true);
        assert_eq!(rows.len(), 2);
        assert!(rows[0].header && rows[0].label == "CLOUD" && rows[0].section == SECTION_CLOUD);
        assert_eq!((rows[1].label.as_str(), rows[1].index, rows[1].icon), ("OneDrive", 0, ICON_CLOUD));
        assert_eq!(rows[1].tip.as_str(), format!("Personal\n{}", Path::new("/u/OneDrive").display()).as_str());
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
    fn a_place_leads_to_its_folder() {
        let places = Places {
            known: vec![gezik_platform::KnownFolder { name: "Home".into(), path: PathBuf::from("/h") }],
            drives: Vec::new(),
            cloud: Vec::new(),
        };
        let pins = [pin("/w", "/w")];
        assert_eq!(place_path(&places, &pins, SECTION_FOLDERS, 0), Some(PathBuf::from("/h")));
        assert_eq!(place_path(&places, &pins, SECTION_PINNED, 0), Some(PathBuf::from("/w")));
        assert_eq!(place_path(&places, &pins, SECTION_PINNED, 1), None);
        assert_eq!(place_path(&places, &pins, SECTION_SEARCHES, 0), None, "no tree under a saved search");
        assert_eq!(place_path(&places, &pins, SECTION_TRASH, 0), None, "nor under the trash");
    }

    #[test]
    fn an_opening_ends_on_the_row_of_its_place_or_folder() {
        let place = |label: &str| SidebarRow { label: label.into(), ..SidebarRow::default() };
        let home: RootKey = (SECTION_FOLDERS, PathBuf::from("/h"));
        let mut shown = Shown::default();
        shown.set_base(vec![place("FOLDERS"), place("Home"), place("C")], vec![None, Some(home.clone()), None]);
        let read = shown.tree.toggle_root(home.clone()).unwrap();
        shown.tree.loaded(&read, Some(tree::prepare(vec!["a".into(), "b".into()], None)));
        shown.relayout();
        let target = Path::new("/h/b");
        let (key, rest) = tree::nearest_place(std::slice::from_ref(&home), target, &same_path).unwrap();
        let tree::Reveal::Shown(b) = shown.tree.reveal(key, &rest, &same_path) else { panic!("open down to it") };
        assert_eq!(reveal_row(&shown, &RevealStep::Node(b)), Some(3), "FOLDERS, Home, a, b");
        assert_eq!(reveal_row(&shown, &RevealStep::Place(home)), Some(1));
        assert_eq!(reveal_row(&shown, &RevealStep::Place((SECTION_DRIVES, PathBuf::from("/x")))), None);
        assert_eq!(reveal_row(&shown, &RevealStep::Unplaced), None);
    }

    #[test]
    fn a_pins_tip_is_its_path_and_its_key() {
        let path = Path::new("/home/a/Projects");
        assert_eq!(pin_tip(path, Some("Alt+1")), format!("{}\nAlt+1", path.display()));
        assert_eq!(pin_tip(path, None), path.display().to_string());
    }
}
