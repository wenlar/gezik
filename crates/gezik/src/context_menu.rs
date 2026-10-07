//! Which Gezik items a context menu shows, and what they do.
//!
//! On Windows, rows and sidebar entries get the Explorer menu with Gezik's items on top;
//! tabs (and everything on macOS/Linux) get a Slint menu.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use gezik_core::drag::Effect;
use gezik_core::nav::Location;
use gezik_core::view::{ColumnKey, ColumnState, GridSize, SortDir, SortKey, SortSpec, ViewMode, ViewSettings};
use gezik_platform::MenuTarget;
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::navigation::Navigator;
use crate::operations::Operations;
use crate::popup::Anchor;
use crate::preview::Preview;
use crate::sidebar::{SECTION_PINNED, Sidebar};
use crate::view::View;
use crate::{AppWindow, MenuEntry, MenuSub};

pub const OPEN_IN_NEW_TAB: u32 = 1;
pub const PIN: u32 = 2;
pub const UNPIN: u32 = 3;
pub const MOVE_UP: u32 = 4;
pub const MOVE_DOWN: u32 = 5;
pub const DUPLICATE_TAB: u32 = 6;
pub const CLOSE_TAB: u32 = 7;
pub const CLOSE_OTHER_TABS: u32 = 8;
/// macOS/Linux only.
pub const OPEN: u32 = 9;
/// macOS/Linux only.
pub const OPEN_DEFAULT: u32 = 10;
pub const LOCK_TAB: u32 = 11;
pub const UNLOCK_TAB: u32 = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// A row in the file list.
    Row {
        is_dir: bool,
        pinned: bool,
    },
    /// Several selected rows.
    Rows,
    /// A sidebar entry; `pinned_section` = it is in the PINNED section.
    Sidebar {
        pinned_section: bool,
        pinned: bool,
        first: bool,
        last: bool,
    },
    Tab {
        only_tab: bool,
        locked: bool,
    },
}

/// Gezik's items for `place`. `native_shell` = the Windows Explorer menu is shown below them.
pub fn items(place: Place, native_shell: bool) -> Vec<(u32, &'static str)> {
    let mut out = Vec::new();
    match place {
        Place::Row { is_dir: true, pinned } => {
            out.push((OPEN_IN_NEW_TAB, "Open in new tab"));
            out.push(pin_toggle(pinned));
        }
        Place::Row { is_dir: false, .. } => {
            // On Windows the Explorer menu already opens files ("Open", "Open with").
            if !native_shell {
                out.push((OPEN, "Open"));
                out.push((OPEN_DEFAULT, "Open with default app"));
            }
        }
        // The Explorer menu acts on all of them; elsewhere Gezik can open them.
        Place::Rows => {
            if !native_shell {
                out.push((OPEN, "Open"));
            }
        }
        Place::Sidebar { pinned_section, pinned, first, last } => {
            out.push((OPEN_IN_NEW_TAB, "Open in new tab"));
            out.push(pin_toggle(pinned));
            if pinned_section && !first {
                out.push((MOVE_UP, "Move up"));
            }
            if pinned_section && !last {
                out.push((MOVE_DOWN, "Move down"));
            }
        }
        Place::Tab { only_tab, locked } => {
            out.push((DUPLICATE_TAB, "Duplicate"));
            if locked {
                out.push((UNLOCK_TAB, "Unlock tab"));
            } else {
                out.push((LOCK_TAB, "Lock tab"));
                out.push((CLOSE_TAB, "Close"));
            }
            if !only_tab {
                out.push((CLOSE_OTHER_TABS, "Close other tabs"));
            }
        }
    }
    out
}

/// Header menu: 20-23 show/hide the columns in `ColumnKey::ALL` order.
pub const TOGGLE_COLUMN_FIRST: u32 = 20;
pub const RESET_COLUMNS: u32 = 24;

/// The column header's menu: show or hide each column, reset all.
pub fn header_items(columns: &[ColumnState]) -> Vec<(u32, &'static str)> {
    let mut out: Vec<(u32, &'static str)> = columns
        .iter()
        .map(|c| {
            let id = TOGGLE_COLUMN_FIRST + u32::try_from(c.key.index() - 1).unwrap_or(0);
            let title = match (c.key, c.visible) {
                (ColumnKey::Modified, true) => "Hide Modified",
                (ColumnKey::Modified, false) => "Show Modified",
                (ColumnKey::Created, true) => "Hide Created",
                (ColumnKey::Created, false) => "Show Created",
                (ColumnKey::Type, true) => "Hide Type",
                (ColumnKey::Type, false) => "Show Type",
                (ColumnKey::Size, true) => "Hide Size",
                (ColumnKey::Size, false) => "Show Size",
            };
            (id, title)
        })
        .collect();
    out.push((RESET_COLUMNS, "Reset columns"));
    out
}

pub const VIEW_LIST: u32 = 30;
pub const VIEW_GRID: u32 = 31;
pub const GRID_SMALL: u32 = 32;
pub const GRID_MEDIUM: u32 = 33;
pub const GRID_LARGE: u32 = 34;
/// 35–39: sort by the keys in `SortKey::ALL` order.
pub const SORT_BY_NAME: u32 = 35;
pub const SORT_BY_MODIFIED: u32 = 36;
pub const SORT_BY_CREATED: u32 = 37;
pub const SORT_BY_TYPE: u32 = 38;
pub const SORT_BY_SIZE: u32 = 39;
pub const SORT_ASC: u32 = 40;
pub const SORT_DESC: u32 = 41;
pub const PREVIEW_PANE: u32 = 42;
pub const APPLY_TO_ALL: u32 = 43;
pub const RESET_FOLDER: u32 = 44;

pub const UNDO: u32 = 50;
pub const REDO: u32 = 51;
pub const PASTE: u32 = 52;
pub const NEW_FOLDER: u32 = 53;
pub const NEW_FILE: u32 = 54;
pub const REFRESH: u32 = 55;
pub const CUT: u32 = 56;
pub const COPY: u32 = 57;
pub const DUPLICATE: u32 = 58;
pub const RENAME: u32 = 59;
pub const TRASH: u32 = 60;
pub const DELETE_PERMANENTLY: u32 = 61;
pub const PASTE_INTO: u32 = 62;
/// Windows only: "Rename N items…" (Explorer's menu has no Rename for several).
pub const BATCH_RENAME: u32 = 63;

/// 70–73: the conflict row menu, in `conflicts::DECISIONS` order.
pub const CONFLICT_FIRST: u32 = 70;

/// The menu after a drag with the right button (drag.rs).
pub const COPY_HERE: u32 = 80;
pub const MOVE_HERE: u32 = 81;
pub const CANCEL_DROP: u32 = 82;

/// 90-99: "Add rule" in the batch rename layer, in `gezik_core::batch::rules::KINDS` order.
pub const ADD_RULE_FIRST: u32 = 90;
/// 100-299: saved rule sets; 300 "Save current rules as…"; 400-599 delete one (ids stay
/// below the Shell's, which start at `GEZIK_IDS_END`).
pub const PRESET_FIRST: u32 = 100;
pub const PRESET_SAVE: u32 = 300;
pub const PRESET_DELETE_FIRST: u32 = 400;
/// How many saved sets the menu lists (and can delete).
pub const PRESET_MAX: u32 = 200;

/// 600-609: archives (archives.rs builds the items).
pub const EXTRACT_HERE: u32 = 600;
pub const EXTRACT_TO_OWN: u32 = 601;
pub const EXTRACT_TO: u32 = 602;
pub const COMPRESS: u32 = 603;
pub const COMPRESS_TO: u32 = 604;
/// After a drag with the right button onto an archive.
pub const ADD_TO_ARCHIVE: u32 = 605;

/// "Convert…" (convert.rs builds the conversion items).
pub const CONVERT: u32 = 610;
/// "Images to PDF…": the Convert layer opened on its PDF choice.
pub const IMAGES_TO_PDF: u32 = 611;
/// 620-659: the Convert layer's "From" encodings (the first: detect); 660-699: its "To"
/// encodings (the first: keep each file's), in `gezik_batch::convert::text::encodings` order.
pub const ENCODING_FROM_FIRST: u32 = 620;
pub const ENCODING_TO_FIRST: u32 = 660;
pub const ENCODING_MAX: u32 = 40;
/// 700-799: "Commands ▸", settings.toml's `[[commands]]` by index.
pub const COMMAND_FIRST: u32 = 700;
pub const COMMAND_MAX: u32 = 100;
/// 800-899: the Convert layer's preset list, by index.
pub const CONVERT_PRESET_FIRST: u32 = 800;
pub const CONVERT_PRESET_MAX: u32 = 100;
/// 900-929: the filter bar's saved filters, by index; 930 "Save as…"; 940-969 delete one
/// (ids stay below the Shell's, which start at `GEZIK_IDS_END`).
pub const FILTER_FIRST: u32 = 900;
pub const FILTER_MAX: u32 = 30;
pub const FILTER_SAVE: u32 = 930;
pub const FILTER_DELETE_FIRST: u32 = 940;
/// 970: a heading inside "Commands ▸" (`menu = "…"`), greyed, never chosen.
pub const COMMAND_GROUP: u32 = 970;
/// Group headings in a Slint menu: shown greyed, never chosen.
pub const HEADING: u32 = 0;
/// Gezik's menu ids are below this; the Explorer menu's start here (gezik_platform's
/// `FIRST_SHELL_ID` on Windows). 1000-4095 are new ranges (spec 11.1).
#[cfg_attr(not(windows), allow(dead_code))]
pub const GEZIK_IDS_END: u32 = 4096;
#[cfg(windows)]
const _: () = assert!(gezik_platform::FIRST_SHELL_ID == GEZIK_IDS_END, "Gezik's ids end where Explorer's begin");
/// Most submenus a menu shows: app.slint and popup-menu.slint have this many places.
pub const MAX_SUBMENUS: usize = 4;

/// A submenu among a menu's items ("Commands ▸"): its title, its place among the items (0:
/// first), and its items (id, title, enabled).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Submenu {
    pub title: String,
    pub at: usize,
    pub items: Vec<(u32, String, bool)>,
}

/// Gezik's file items for rows on macOS and Linux; Windows has them in its own menu (and
/// Gezik takes them over, see `Menus::run_verb`).
pub fn file_items(single: bool, folder: bool, can_paste: bool) -> Vec<(u32, &'static str)> {
    let mut out = vec![(CUT, "Cut"), (COPY, "Copy")];
    if folder && can_paste {
        out.push((PASTE_INTO, "Paste into folder"));
    }
    out.push((DUPLICATE, "Duplicate"));
    out.push((RENAME, if single { "Rename" } else { "Rename items…" }));
    out.push((TRASH, "Move to Trash"));
    out.push((DELETE_PERMANENTLY, "Delete permanently"));
    out
}

/// The items for empty space in a folder: Undo/Redo say what they would do.
pub fn background_items(undo: Option<&str>, redo: Option<&str>, can_paste: bool) -> Vec<(u32, String)> {
    let mut out = Vec::new();
    if let Some(label) = undo {
        out.push((UNDO, format!("Undo {label}")));
    }
    if let Some(label) = redo {
        out.push((REDO, format!("Redo {label}")));
    }
    if can_paste {
        out.push((PASTE, "Paste".to_owned()));
    }
    out.push((NEW_FOLDER, "New folder".to_owned()));
    out.push((NEW_FILE, "New file".to_owned()));
    out.push((REFRESH, "Refresh".to_owned()));
    out
}

/// `items` with owned labels, to add items whose labels are made at run time.
fn owned(items: Vec<(u32, &'static str)>) -> Vec<(u32, String)> {
    items.into_iter().map(|(id, title)| (id, title.to_owned())).collect()
}

/// The View menu; the current choices are marked with a bullet.
pub fn view_items(view: ViewSettings, preview_open: bool) -> Vec<(u32, String)> {
    let mark = |on: bool, title: &str| format!("{}{title}", if on { "• " } else { "    " });
    let grid = view.mode == ViewMode::Grid;
    let mut out = vec![(VIEW_LIST, mark(!grid, "List")), (VIEW_GRID, mark(grid, "Grid"))];
    if grid {
        out.push((GRID_SMALL, mark(view.grid_size == GridSize::Small, "Small icons")));
        out.push((GRID_MEDIUM, mark(view.grid_size == GridSize::Medium, "Medium icons")));
        out.push((GRID_LARGE, mark(view.grid_size == GridSize::Large, "Large icons")));
    }
    let sorts = [
        (SORT_BY_NAME, SortKey::Name, "Sort by name"),
        (SORT_BY_MODIFIED, SortKey::Modified, "Sort by date modified"),
        (SORT_BY_CREATED, SortKey::Created, "Sort by date created"),
        (SORT_BY_TYPE, SortKey::Type, "Sort by type"),
        (SORT_BY_SIZE, SortKey::Size, "Sort by size"),
    ];
    for (id, key, title) in sorts {
        out.push((id, mark(view.sort.key == key, title)));
    }
    out.push((SORT_ASC, mark(view.sort.dir == SortDir::Asc, "Ascending")));
    out.push((SORT_DESC, mark(view.sort.dir == SortDir::Desc, "Descending")));
    out.push((PREVIEW_PANE, mark(preview_open, "Preview pane")));
    out.push((APPLY_TO_ALL, "Apply to all folders".to_owned()));
    out.push((RESET_FOLDER, "Reset this folder".to_owned()));
    out
}

/// "Presets ▾": each saved set, Save, then a Delete item for each (up to `PRESET_MAX` each).
pub fn preset_items(names: &[String]) -> Vec<(u32, String)> {
    let mut list: Vec<(u32, String)> =
        names.iter().enumerate().take(PRESET_MAX as usize).map(|(i, n)| (PRESET_FIRST + i as u32, n.clone())).collect();
    list.push((PRESET_SAVE, "Save current rules as…".to_owned()));
    list.extend(
        names
            .iter()
            .enumerate()
            .take(PRESET_MAX as usize)
            .map(|(i, n)| (PRESET_DELETE_FIRST + i as u32, format!("Delete \"{n}\""))),
    );
    list
}

/// The filter bar's ▾ menu: each saved filter, "Save as…" (only with a good, non-empty text),
/// then a Delete item for each (up to `FILTER_MAX` each).
pub fn filter_items(names: &[String], can_save: bool) -> Vec<(u32, String, bool)> {
    let shown = names.iter().take(FILTER_MAX as usize).enumerate();
    let mut list: Vec<(u32, String, bool)> =
        shown.clone().map(|(i, name)| (FILTER_FIRST + i as u32, name.clone(), true)).collect();
    list.push((FILTER_SAVE, "Save as…".to_owned(), can_save));
    list.extend(shown.map(|(i, name)| (FILTER_DELETE_FIRST + i as u32, format!("Delete \"{name}\""), true)));
    list
}

fn pin_toggle(pinned: bool) -> (u32, &'static str) {
    if pinned { (UNPIN, "Unpin from sidebar") } else { (PIN, "Pin to sidebar") }
}

/// What a menu was opened for, captured when it opens. Items run later (the Slint menu
/// stays open while other things happen), so nothing here is an index that could point
/// elsewhere by then: rows and sidebar entries are kept by path, tabs by id.
#[derive(Debug, Clone)]
enum Subject {
    Row(PathBuf),
    Rows(Vec<PathBuf>),
    SidebarEntry(PathBuf),
    Tab(u64),
    /// Empty space in this folder.
    Background(PathBuf),
    Header,
    View,
    Conflict(usize),
    /// Files dropped with the right button, the folder they were dropped on, and the
    /// archive if they were dropped on one.
    Drop(Vec<PathBuf>, PathBuf, Option<PathBuf>),
    /// The batch rename layer's menus, with the preset names shown (items are by index).
    BatchRename(Vec<String>),
    /// The Convert layer's menus (presets, encodings).
    Convert,
    /// The filter bar's ▾ menu, with the saved filters' names shown (items are by index).
    Filter(Vec<String>),
}

/// Lets one native menu be pending or open at a time, so two right-clicks in quick
/// succession (within the delay before it opens, or queued behind its modal loop) do not
/// show two menus one after the other.
#[derive(Clone, Default)]
#[cfg_attr(not(windows), allow(dead_code))]
struct MenuGate(Rc<Cell<bool>>);

#[cfg_attr(not(windows), allow(dead_code))]
impl MenuGate {
    /// Claims the gate until the returned claim is dropped; `None` while a menu already
    /// holds it.
    fn claim(&self) -> Option<MenuClaim> {
        (!self.0.replace(true)).then(|| MenuClaim(self.0.clone()))
    }
}

struct MenuClaim(Rc<Cell<bool>>);

impl Drop for MenuClaim {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

/// Opens context menus and runs their Gezik items.
#[derive(Clone)]
pub struct Menus {
    window: slint::Weak<AppWindow>,
    nav: Navigator,
    view: View,
    preview: Preview,
    sidebar: Sidebar,
    ops: Operations,
    /// What the open Slint menu is for.
    subject: Rc<RefCell<Option<Subject>>>,
    /// The rows a row menu was opened for (path, is a folder), for its archive items.
    rows: Rc<RefCell<Vec<(PathBuf, bool)>>>,
    #[cfg_attr(not(windows), allow(dead_code))]
    native_menu: MenuGate,
}

impl Menus {
    pub fn new(
        window: &AppWindow,
        nav: Navigator,
        view: View,
        preview: Preview,
        sidebar: Sidebar,
        ops: Operations,
    ) -> Menus {
        let menus = Menus {
            window: window.as_weak(),
            nav,
            view,
            preview,
            sidebar,
            ops,
            subject: Rc::default(),
            rows: Rc::default(),
            native_menu: MenuGate::default(),
        };
        window.on_menu_activated({
            let menus = menus.clone();
            move |id| {
                let subject = menus.subject.borrow_mut().take();
                if let (Ok(id), Some(subject)) = (u32::try_from(id), subject) {
                    menus.run(id, subject);
                    // An item of a submenu leaves the keyboard nowhere once the menus close
                    // (Slint gives it back to the parent menu, which is gone): the list (or
                    // the layer over it) takes it, unless a question came up.
                    if from_submenu(id) {
                        let weak = menus.window.clone();
                        slint::Timer::single_shot(std::time::Duration::ZERO, move || {
                            if let Some(window) = weak.upgrade()
                                && !window.get_dialog_open()
                            {
                                window.invoke_focus_list();
                            }
                        });
                    }
                }
            }
        });
        menus
    }

    /// Right-click on file list row `index` (already selected), at window position `x`, `y`.
    pub fn row(&self, index: i32, x: f32, y: f32) {
        self.row_menu(index, x, y, false);
    }

    /// Right-click on empty space in the file list, at window position `x`, `y`.
    pub fn background(&self, x: f32, y: f32) {
        self.background_menu(None, x, y);
    }

    /// Shift+F10 or the Menu key on the file list: the menu of row `index` (selected and
    /// scrolled into view), or of the folder's background if it is -1, at window position
    /// `x`, `y` (also for the Windows menu, which a right-click opens at the cursor).
    pub fn keyboard(&self, index: i32, x: f32, y: f32) {
        if index >= 0 {
            self.row_menu(index, x, y, true);
        } else {
            self.background_menu(Some((x, y)), x, y);
        }
    }

    fn row_menu(&self, index: i32, x: f32, y: f32, at_position: bool) {
        let Ok(i) = usize::try_from(index) else { return };
        let at = at_position.then_some((x, y));
        let native = cfg!(windows);
        self.ops.clipboard_check();
        if self.view.is_selected(i) && self.view.selection_count() > 1 {
            let rows = self.view.selected_items();
            let paths: Vec<PathBuf> = rows.iter().map(|(path, _)| path.clone()).collect();
            let mut list = owned(items(Place::Rows, native));
            let mut subs = Vec::new();
            self.add_file_tools(&mut list, &mut subs, rows, native);
            list.extend(self.file_extras(false, false, native));
            if native && !self.view.shows_drives() {
                list.push((BATCH_RENAME, format!("Rename {} items…", paths.len())));
            }
            return self.open(Subject::Rows(paths.clone()), list, subs, MenuTarget::Items(paths), x, y, at);
        }
        let Some((path, is_dir)) = self.view.entry_path(i) else { return };
        let place = Place::Row { is_dir, pinned: is_dir && self.sidebar.is_pinned(&path) };
        let mut list = owned(items(place, native));
        let mut subs = Vec::new();
        self.add_file_tools(&mut list, &mut subs, vec![(path.clone(), is_dir)], native);
        list.extend(self.file_extras(true, is_dir, native));
        self.open(Subject::Row(path.clone()), list, subs, MenuTarget::Item(path), x, y, at);
    }

    /// Extract, Compress, Convert and Commands ▸ for `rows`: first on Windows (above the
    /// Explorer menu's own items, moving the places of `subs` down), after the row's other
    /// items elsewhere. Not for drives.
    fn add_file_tools(
        &self,
        list: &mut Vec<(u32, String)>,
        subs: &mut Vec<Submenu>,
        rows: Vec<(PathBuf, bool)>,
        native: bool,
    ) {
        if self.view.shows_drives() {
            return;
        }
        let mut extra = crate::archives::menu_items(&rows);
        let (convert, commands) = crate::convert::menu_items(&rows);
        extra.extend(convert);
        *self.rows.borrow_mut() = rows;
        let start = if native { 0 } else { list.len() };
        if native {
            for sub in subs.iter_mut() {
                sub.at += extra.len();
            }
        }
        subs.extend(commands.map(|items| Submenu { title: "Commands".to_owned(), at: start + extra.len(), items }));
        if native {
            list.splice(0..0, extra);
        } else {
            list.extend(extra);
        }
    }

    /// File items after the row's own: Windows already has Cut, Copy, Delete... (taken over in
    /// `run_verb`), so only Duplicate is added there.
    fn file_extras(&self, single: bool, folder: bool, native: bool) -> Vec<(u32, String)> {
        // Drives (This PC) are not files: no copying, deleting or renaming them.
        if self.view.shows_drives() {
            Vec::new()
        } else if native {
            vec![(DUPLICATE, "Duplicate".to_owned())]
        } else {
            owned(file_items(single, folder, self.ops.can_paste()))
        }
    }

    fn background_menu(&self, at: Option<(f32, f32)>, x: f32, y: f32) {
        let Location::Path(dir) = self.nav.active_location() else { return };
        self.ops.clipboard_check();
        let list =
            background_items(self.ops.undo_label().as_deref(), self.ops.redo_label().as_deref(), self.ops.can_paste());
        self.open(Subject::Background(dir.clone()), list, Vec::new(), MenuTarget::Background(dir), x, y, at);
    }

    /// Right-click on sidebar entry (`section`, `index`), at window position `x`, `y`.
    pub fn sidebar_entry(&self, section: i32, index: i32, x: f32, y: f32) {
        let Some(Location::Path(path)) = self.sidebar.location_of(section, index) else { return };
        let pinned_section = section == SECTION_PINNED;
        let count = if pinned_section { self.sidebar.visible_pinned_count() } else { 0 };
        let index = usize::try_from(index).unwrap_or(0);
        let place = Place::Sidebar {
            pinned_section,
            pinned: self.sidebar.is_pinned(&path),
            first: index == 0,
            last: index + 1 >= count,
        };
        self.open(
            Subject::SidebarEntry(path.clone()),
            owned(items(place, cfg!(windows))),
            Vec::new(),
            MenuTarget::Item(path),
            x,
            y,
            None,
        );
    }

    /// Right-click on tab `index`, at window position `x`, `y`. Tabs get Gezik's own menu
    /// everywhere.
    pub fn tab(&self, index: usize, x: f32, y: f32) {
        let Some(id) = self.nav.tab_id(index) else { return };
        let place = Place::Tab { only_tab: self.nav.tab_count() == 1, locked: self.nav.is_tab_locked(index) };
        *self.subject.borrow_mut() = Some(Subject::Tab(id));
        self.open_slint(&items(place, false), Anchor::point(x, y));
    }

    /// Right-click on the column header, at window position `x`, `y`.
    pub fn header(&self, x: f32, y: f32) {
        *self.subject.borrow_mut() = Some(Subject::Header);
        self.open_slint(&header_items(&self.view.columns()), Anchor::point(x, y));
    }

    /// The View button's menu, under it.
    pub fn view_menu(&self, at: Anchor) {
        *self.subject.borrow_mut() = Some(Subject::View);
        self.open_slint(&view_items(self.view.view_settings(), self.preview.is_pane_open()), at);
    }

    /// `subs`: submenus among `items`. `at`: where the Windows menu opens (window position),
    /// else at the cursor.
    #[allow(clippy::too_many_arguments, reason = "what the menu is for, what it shows, and where")]
    fn open(
        &self,
        subject: Subject,
        items: Vec<(u32, String)>,
        subs: Vec<Submenu>,
        target: MenuTarget,
        x: f32,
        y: f32,
        at: Option<(f32, f32)>,
    ) {
        if cfg!(windows) {
            self.open_native(Some(subject), target, items, subs, at);
        } else {
            *self.subject.borrow_mut() = Some(subject);
            let entries: Vec<(u32, String, bool)> = items.into_iter().map(|(id, title)| (id, title, true)).collect();
            self.open_slint_entries(&entries, subs, Anchor::point(x, y));
        }
    }

    /// Shows the Explorer menu at window position `at` (else at the cursor), with `items` on
    /// top. Its modal loop blocks the UI thread, so it opens once the click is fully handled
    /// and the selection is drawn. A request while another menu is pending or open is dropped.
    #[cfg(windows)]
    fn open_native(
        &self,
        subject: Option<Subject>,
        target: MenuTarget,
        items: Vec<(u32, String)>,
        subs: Vec<Submenu>,
        at: Option<(f32, f32)>,
    ) {
        let Some(claim) = self.native_menu.claim() else { return };
        let menus = self.clone();
        slint::Timer::single_shot(std::time::Duration::from_millis(16), move || {
            let Some(window) = menus.window.upgrade() else { return };
            let handle = window.window().window_handle();
            let scale = window.window().scale_factor();
            let at = at.map(|(x, y)| ((x * scale).round() as i32, (y * scale).round() as i32));
            let items: Vec<(u32, &str)> = items.iter().map(|(id, title)| (*id, title.as_str())).collect();
            let sub_items: Vec<Vec<(u32, &str, bool)>> = subs
                .iter()
                .map(|sub| sub.items.iter().map(|(id, title, on)| (*id, title.as_str(), *on)).collect())
                .collect();
            let shell_subs: Vec<gezik_platform::ShellSubmenu> = subs
                .iter()
                .zip(&sub_items)
                .map(|(sub, items)| gezik_platform::ShellSubmenu { title: sub.title.as_str(), at: sub.at, items })
                .collect();
            // Gezik renames in place, only a single row of a folder listing (see run_verb).
            let can_rename = matches!(subject, Some(Subject::Row(_))) && !menus.view.shows_drives();
            let outcome = gezik_platform::show_shell_menu(&handle, &target, &items, &shell_subs, at, can_rename);
            release_stale_modifiers(&window);
            drop(claim);
            match outcome {
                Ok(gezik_platform::MenuOutcome::Gezik(id)) => {
                    if let Some(subject) = subject {
                        menus.run(id, subject);
                    }
                }
                Ok(gezik_platform::MenuOutcome::SystemCommandRan) => {
                    // The command may have created, renamed or deleted anything, pinned
                    // folders included.
                    menus.nav.reload();
                    menus.sidebar.refresh();
                }
                Ok(gezik_platform::MenuOutcome::Verb(verb)) => menus.run_verb(verb, subject),
                Ok(gezik_platform::MenuOutcome::Dismissed) => {}
                Err(err) => window.set_status(format!("Cannot show the menu: {err}").into()),
            }
        });
    }

    #[cfg(not(windows))]
    fn open_native(
        &self,
        _subject: Option<Subject>,
        _target: MenuTarget,
        _items: Vec<(u32, String)>,
        _subs: Vec<Submenu>,
        _at: Option<(f32, f32)>,
    ) {
    }

    fn open_slint<S: AsRef<str>>(&self, items: &[(u32, S)], at: Anchor) {
        let entries: Vec<(u32, String, bool)> =
            items.iter().map(|(id, title)| (*id, title.as_ref().to_owned(), true)).collect();
        self.open_slint_entries(&entries, Vec::new(), at);
    }

    /// Shows `items` (id, title, enabled), with `subs` among them, at `anchor`.
    /// (Slint shows these as native menus on Windows, where `&` marks the access key, and on
    /// macOS; elsewhere Gezik draws its own, which popup.rs keeps inside the window.)
    fn open_slint_entries(&self, items: &[(u32, String, bool)], subs: Vec<Submenu>, anchor: Anchor) {
        let Some(window) = self.window.upgrade() else { return };
        if items.is_empty() && subs.iter().all(|sub| sub.items.is_empty()) {
            return;
        }
        let entry = |(id, title, enabled): &(u32, String, bool)| MenuEntry {
            id: i32::try_from(*id).unwrap_or(0),
            title: menu_title(title).into(),
            enabled: *enabled,
        };
        let model = |entries: Vec<MenuEntry>| ModelRc::new(VecModel::from(entries));
        let (before, parts) = split_menu(items, &subs);
        let before: Vec<MenuEntry> = before.iter().map(entry).collect();
        let parts: Vec<(String, Vec<MenuEntry>, Vec<MenuEntry>)> = parts
            .into_iter()
            .map(|(sub, after)| {
                (menu_title(&sub.title), sub.items.iter().map(entry).collect(), after.iter().map(entry).collect())
            })
            .collect();
        let slots: Vec<MenuSub> = parts
            .iter()
            .map(|(title, inner, after)| MenuSub {
                title: title.as_str().into(),
                entries: model(inner.clone()),
                after: model(after.clone()),
            })
            .collect();
        window.set_menu_entries(model(before.clone()));
        window.set_menu_subs(ModelRc::new(VecModel::from(slots)));
        if !window.get_native_menus() {
            let lines = parts.into_iter().map(|(title, _, after)| (title, after)).collect();
            window.set_menu_lines(model(menu_lines(before, lines)));
        }
        window.invoke_show_menu(anchor.x, anchor.y, anchor.flip_x, anchor.flip_y);
    }

    /// One of the Convert layer's menus (convert.rs builds it): `items` (id, title, enabled)
    /// under its button.
    pub fn convert_menu(&self, items: Vec<(u32, String, bool)>, at: Anchor) {
        *self.subject.borrow_mut() = Some(Subject::Convert);
        self.open_slint_entries(&items, Vec::new(), at);
    }

    /// Copy here / Move here / Cancel for files dropped with the right button on `dir`, at
    /// window position `x`, `y`; only the effects that make sense there are offered.
    /// `archive`: they were dropped on one, which "Add to archive" adds them to.
    #[allow(clippy::too_many_arguments, reason = "what was dropped where, and what it may do")]
    pub fn drop_menu(
        &self,
        paths: Vec<PathBuf>,
        dir: PathBuf,
        archive: Option<PathBuf>,
        can_copy: bool,
        can_move: bool,
        x: f32,
        y: f32,
    ) {
        let mut list = Vec::new();
        if archive.is_some() {
            list.push((ADD_TO_ARCHIVE, "Add to archive"));
        }
        if can_copy {
            list.push((COPY_HERE, "Copy here"));
        }
        if can_move {
            list.push((MOVE_HERE, "Move here"));
        }
        if list.is_empty() {
            return;
        }
        list.push((CANCEL_DROP, "Cancel"));
        *self.subject.borrow_mut() = Some(Subject::Drop(paths, dir, archive));
        self.open_slint(&list, Anchor::point(x, y));
    }

    /// The decision menu of conflict row `row`, at window position `x`, `y`.
    pub fn conflict(&self, row: i32, x: f32, y: f32) {
        let Ok(row) = usize::try_from(row) else { return };
        let conflicts = self.ops.conflicts();
        let list: Vec<(u32, &'static str)> = crate::conflicts::DECISIONS
            .iter()
            .enumerate()
            .filter(|(_, d)| conflicts.choices_for(row).contains(d))
            .map(|(i, d)| (CONFLICT_FIRST + i as u32, d.label()))
            .collect();
        if list.is_empty() {
            return;
        }
        *self.subject.borrow_mut() = Some(Subject::Conflict(row));
        self.open_slint(&list, Anchor::point(x, y));
    }
    /// "Add rule ▾" of the batch rename layer, under it.
    pub fn add_rule(&self, at: Anchor) {
        let list: Vec<(u32, &str)> = gezik_core::batch::rules::KINDS
            .iter()
            .enumerate()
            .map(|(i, (_, label))| (ADD_RULE_FIRST + i as u32, *label))
            .collect();
        *self.subject.borrow_mut() = Some(Subject::BatchRename(Vec::new()));
        self.open_slint(&list, at);
    }

    /// "Presets ▾": the saved sets, save, delete.
    pub fn presets(&self, at: Anchor) {
        let names = crate::batch_rename::preset_names();
        let list = preset_items(&names);
        *self.subject.borrow_mut() = Some(Subject::BatchRename(names));
        self.open_slint(&list, at);
    }

    /// The filter bar's ▾ menu, under its button: the saved filters, "Save as…", Delete.
    pub fn filter_menu(&self, at: Anchor) {
        let names: Vec<String> = crate::filter::saved().into_iter().map(|f| f.name).collect();
        let mut can_save = false;
        crate::filter::with_current(|filter| can_save = filter.can_save());
        let items = filter_items(&names, can_save);
        *self.subject.borrow_mut() = Some(Subject::Filter(names));
        self.open_slint_entries(&items, Vec::new(), at);
    }

    fn run(&self, id: u32, subject: Subject) {
        match (id, subject) {
            // By name: settings.toml may have been reloaded since the menu opened.
            (id, Subject::Filter(names)) if (FILTER_FIRST..FILTER_FIRST + FILTER_MAX).contains(&id) => {
                if let Some(name) = names.get((id - FILTER_FIRST) as usize) {
                    crate::filter::with_current(|filter| filter.apply_saved(name));
                }
            }
            (FILTER_SAVE, Subject::Filter(_)) => crate::filter::with_current(crate::filter::Filter::ask_save),
            (id, Subject::Filter(names)) if (FILTER_DELETE_FIRST..FILTER_DELETE_FIRST + FILTER_MAX).contains(&id) => {
                if let Some(name) = names.get((id - FILTER_DELETE_FIRST) as usize) {
                    crate::filter::with_current(|filter| filter.delete_saved(name));
                }
            }
            (id, Subject::BatchRename(_)) if (ADD_RULE_FIRST..ADD_RULE_FIRST + 10).contains(&id) => {
                if let Some((kind, _)) = gezik_core::batch::rules::KINDS.get((id - ADD_RULE_FIRST) as usize) {
                    crate::batch_rename::with_current(|layer| layer.add_rule(kind));
                }
            }
            // By name: settings.toml may have been reloaded since the menu opened.
            (id, Subject::BatchRename(names)) if (PRESET_FIRST..PRESET_FIRST + PRESET_MAX).contains(&id) => {
                if let Some(name) = names.get((id - PRESET_FIRST) as usize) {
                    crate::batch_rename::with_current(|layer| layer.apply_preset(name));
                }
            }
            (PRESET_SAVE, Subject::BatchRename(_)) => {
                crate::batch_rename::with_current(|layer| layer.ask_preset_name());
            }
            (id, Subject::BatchRename(names))
                if (PRESET_DELETE_FIRST..PRESET_DELETE_FIRST + PRESET_MAX).contains(&id) =>
            {
                if let Some(name) = names.get((id - PRESET_DELETE_FIRST) as usize) {
                    crate::batch_rename::with_current(|layer| layer.delete_preset(name));
                }
            }
            (id, Subject::Conflict(row)) if (CONFLICT_FIRST..CONFLICT_FIRST + 4).contains(&id) => {
                if let Some(decision) = crate::conflicts::DECISIONS.get((id - CONFLICT_FIRST) as usize) {
                    self.ops.conflicts().decide_row(row, *decision);
                }
            }
            (COPY_HERE, Subject::Drop(paths, dir, _)) => self.ops.transfer(paths, dir, Effect::Copy),
            (MOVE_HERE, Subject::Drop(paths, dir, _)) => self.ops.transfer(paths, dir, Effect::Move),
            (ADD_TO_ARCHIVE, Subject::Drop(paths, _, Some(archive))) => {
                crate::archives::with_current(|archives| archives.add_to(archive, paths, None));
            }
            (id, Subject::Convert) => crate::convert::with_current(|convert| convert.menu_chosen(id)),
            (CONVERT, Subject::Row(_) | Subject::Rows(_)) => {
                let rows = std::mem::take(&mut *self.rows.borrow_mut());
                crate::convert::with_current(|convert| convert.open(rows));
            }
            (IMAGES_TO_PDF, Subject::Row(_) | Subject::Rows(_)) => {
                let rows = std::mem::take(&mut *self.rows.borrow_mut());
                let choice = crate::convert::Choice::Pdf(gezik_core::batch::pdf::PdfOp::ImagesToPdf);
                crate::convert::with_current(|convert| convert.open_with(rows, Some(choice)));
            }
            (id, Subject::Row(_) | Subject::Rows(_)) if (COMMAND_FIRST..COMMAND_FIRST + COMMAND_MAX).contains(&id) => {
                let rows = std::mem::take(&mut *self.rows.borrow_mut());
                crate::convert::run_menu_command((id - COMMAND_FIRST) as usize, rows);
            }
            (EXTRACT_HERE..=COMPRESS_TO, Subject::Row(_) | Subject::Rows(_)) => {
                let rows = std::mem::take(&mut *self.rows.borrow_mut());
                let paths: Vec<PathBuf> = rows.iter().map(|(path, _)| path.clone()).collect();
                crate::archives::with_current(|archives| match id {
                    EXTRACT_HERE => archives.extract_here(paths),
                    EXTRACT_TO_OWN => archives.extract_to_own(paths),
                    EXTRACT_TO => archives.extract_to_asked(paths),
                    COMPRESS => archives.open_compress(rows),
                    _ => archives.compress_to(rows),
                });
            }
            (OPEN_IN_NEW_TAB, Subject::Row(path) | Subject::SidebarEntry(path)) => {
                self.nav.open_tab(Location::Path(path), false);
            }
            (PIN, Subject::Row(path) | Subject::SidebarEntry(path)) => self.sidebar.pin(path),
            (UNPIN, Subject::Row(path) | Subject::SidebarEntry(path)) => self.sidebar.unpin_path(&path),
            (MOVE_UP, Subject::SidebarEntry(path)) => {
                if let Some(i) = self.sidebar.visible_pinned_index(&path)
                    && i > 0
                {
                    self.sidebar.move_pinned(i, i - 1);
                }
            }
            (MOVE_DOWN, Subject::SidebarEntry(path)) => {
                // `move_pinned` clamps, so the last entry stays put.
                if let Some(i) = self.sidebar.visible_pinned_index(&path) {
                    self.sidebar.move_pinned(i, i + 1);
                }
            }
            (DUPLICATE_TAB, Subject::Tab(id)) => {
                if let Some(i) = self.nav.tab_index(id) {
                    self.nav.duplicate_tab(i);
                }
            }
            (CLOSE_TAB, Subject::Tab(id)) => {
                // After the menu is fully done: closing the last tab closes the window.
                let nav = self.nav.clone();
                slint::Timer::single_shot(std::time::Duration::ZERO, move || nav.close_tab_by_id(id));
            }
            (LOCK_TAB | UNLOCK_TAB, Subject::Tab(id)) => {
                if let Some(i) = self.nav.tab_index(id) {
                    self.nav.toggle_tab_lock(i);
                }
            }
            (CLOSE_OTHER_TABS, Subject::Tab(id)) => {
                if let Some(i) = self.nav.tab_index(id) {
                    self.nav.close_other_tabs(i);
                }
            }
            (OPEN | OPEN_DEFAULT, Subject::Row(path)) => {
                if let Err(err) = open::that_detached(&path)
                    && let Some(window) = self.window.upgrade()
                {
                    let why = gezik_platform::fs::describe(&err);
                    window.set_status(format!("Cannot open {}: {why}", path.display()).into());
                }
            }
            (OPEN, Subject::Rows(paths)) => {
                let paths = match crate::view::limit_open(paths) {
                    Ok(paths) => paths,
                    Err(message) => {
                        if let Some(window) = self.window.upgrade() {
                            window.set_status(message.into());
                        }
                        return;
                    }
                };
                for path in paths {
                    if let Err(err) = open::that_detached(&path)
                        && let Some(window) = self.window.upgrade()
                    {
                        let why = gezik_platform::fs::describe(&err);
                        window.set_status(format!("Cannot open {}: {why}", path.display()).into());
                    }
                }
            }
            (id, Subject::Header) if (TOGGLE_COLUMN_FIRST..TOGGLE_COLUMN_FIRST + 4).contains(&id) => {
                if let Some(key) = ColumnKey::ALL.get((id - TOGGLE_COLUMN_FIRST) as usize) {
                    self.view.toggle_column(*key);
                }
            }
            (RESET_COLUMNS, Subject::Header) => self.view.reset_columns(),
            (VIEW_LIST, Subject::View) => self.view.set_mode(ViewMode::List),
            (VIEW_GRID, Subject::View) => self.view.set_mode(ViewMode::Grid),
            (GRID_SMALL, Subject::View) => self.view.set_grid_size(GridSize::Small),
            (GRID_MEDIUM, Subject::View) => self.view.set_grid_size(GridSize::Medium),
            (GRID_LARGE, Subject::View) => self.view.set_grid_size(GridSize::Large),
            (id, Subject::View) if (SORT_BY_NAME..=SORT_BY_SIZE).contains(&id) => {
                let key = SortKey::ALL[(id - SORT_BY_NAME) as usize];
                self.view.set_sort(SortSpec { key, dir: self.view.sort().dir });
            }
            (SORT_ASC, Subject::View) => self.view.set_sort(SortSpec { dir: SortDir::Asc, ..self.view.sort() }),
            (SORT_DESC, Subject::View) => self.view.set_sort(SortSpec { dir: SortDir::Desc, ..self.view.sort() }),
            (PREVIEW_PANE, Subject::View) => self.preview.toggle_pane(),
            (APPLY_TO_ALL, Subject::View) => self.view.apply_to_all(),
            (RESET_FOLDER, Subject::View) => self.view.reset_folder(),
            (CUT | COPY, Subject::Row(path)) => self.ops.copy_paths(vec![path], id == CUT),
            (CUT | COPY, Subject::Rows(paths)) => self.ops.copy_paths(paths, id == CUT),
            (PASTE_INTO, Subject::Row(path)) => self.ops.paste(Some(path), false),
            (DUPLICATE, Subject::Row(_) | Subject::Rows(_)) => self.ops.duplicate(),
            (RENAME, Subject::Row(_) | Subject::Rows(_)) => self.ops.rename_start(),
            (BATCH_RENAME, Subject::Rows(_)) => self.ops.batch_rename(),
            (TRASH | DELETE_PERMANENTLY, Subject::Row(path)) => {
                self.ops.trash_paths(vec![path], id == DELETE_PERMANENTLY)
            }
            (TRASH | DELETE_PERMANENTLY, Subject::Rows(paths)) => self.ops.trash_paths(paths, id == DELETE_PERMANENTLY),
            (UNDO, Subject::Background(_)) => self.ops.undo(),
            (REDO, Subject::Background(_)) => self.ops.redo(),
            (PASTE, Subject::Background(dir)) => self.ops.paste(Some(dir), false),
            (NEW_FOLDER, Subject::Background(dir)) => self.ops.new_folder(Some(dir)),
            (NEW_FILE, Subject::Background(dir)) => self.ops.new_file(Some(dir)),
            (REFRESH, Subject::Background(_)) => self.nav.reload(),
            _ => {}
        }
    }

    /// Explorer's own Cut, Copy, Paste, Delete and Rename, done by Gezik (its engine, its
    /// conflict list, its undo).
    #[cfg(windows)]
    fn run_verb(&self, verb: gezik_platform::ShellVerb, subject: Option<Subject>) {
        use gezik_platform::ShellVerb;
        let paths = match &subject {
            Some(Subject::Row(path) | Subject::SidebarEntry(path)) => vec![path.clone()],
            Some(Subject::Rows(paths)) => paths.clone(),
            _ => Vec::new(),
        };
        match verb {
            ShellVerb::Cut => self.ops.copy_paths(paths, true),
            ShellVerb::Copy => self.ops.copy_paths(paths, false),
            ShellVerb::Paste => {
                let into = match subject {
                    // A row is a target only if it is a folder, else the shown folder gets it.
                    Some(Subject::Row(path)) => self.view.is_folder_row(&path).then_some(path),
                    Some(Subject::SidebarEntry(path) | Subject::Background(path)) => Some(path),
                    _ => None,
                };
                self.ops.paste(into, false);
            }
            ShellVerb::Delete => {
                let keys = gezik_platform::modifier_keys_down();
                let permanent = keys.left_shift || keys.right_shift;
                if matches!(subject, Some(Subject::SidebarEntry(_))) {
                    // A pinned folder is not what is selected in the list: always ask first.
                    self.ops.trash_asking(paths, permanent);
                } else {
                    self.ops.trash_paths(paths, permanent);
                }
            }
            ShellVerb::Rename => {
                if matches!(subject, Some(Subject::Row(_))) {
                    self.ops.rename_start();
                }
            }
        }
    }
}

/// Tells Slint that the modifier keys not down now were released. The native menu's modal
/// loop takes the key releases made while it is open (Shift after Shift+F10, or a modifier
/// held for a right-click), and Slint knows modifiers only from key events, so it would go
/// on treating plain clicks as Shift+clicks. Releasing a key Slint already counts as up
/// changes nothing.
#[cfg(windows)]
fn release_stale_modifiers(window: &AppWindow) {
    for key in released_modifiers(gezik_platform::modifier_keys_down()) {
        window.window().dispatch_event(slint::platform::WindowEvent::KeyReleased { text: key.into() });
    }
}

/// The Slint modifier keys that are up, given the keys `down`.
#[cfg(windows)]
fn released_modifiers(down: gezik_platform::ModifierKeys) -> Vec<slint::platform::Key> {
    use slint::platform::Key;
    [
        (down.left_shift, Key::Shift),
        (down.right_shift, Key::ShiftR),
        (down.left_control, Key::Control),
        (down.right_control, Key::ControlR),
        // Slint reports either Alt as `Alt`, and right Alt as `AltGr` on layouts that have it.
        (down.alt, Key::Alt),
        (down.right_alt, Key::AltGr),
        (down.left_meta, Key::Meta),
        (down.right_meta, Key::MetaR),
    ]
    .into_iter()
    .filter_map(|(is_down, key)| (!is_down).then_some(key))
    .collect()
}

/// `items` cut at `subs`' places: the items before the first submenu, then each submenu (by
/// place, empty ones left out, at most `MAX_SUBMENUS`) with the items after it.
fn split_menu<'a, T: Clone>(items: &[T], subs: &'a [Submenu]) -> (Vec<T>, Vec<(&'a Submenu, Vec<T>)>) {
    let mut kept: Vec<&Submenu> = subs.iter().filter(|sub| !sub.items.is_empty()).collect();
    kept.sort_by_key(|sub| sub.at);
    kept.truncate(MAX_SUBMENUS);
    let place = |sub: &Submenu| sub.at.min(items.len());
    let first = kept.first().map_or(items.len(), |sub| place(sub));
    let parts = kept
        .iter()
        .enumerate()
        .map(|(i, sub)| {
            let end = kept.get(i + 1).map_or(items.len(), |next| place(next));
            (*sub, items[place(sub)..end].to_vec())
        })
        .collect();
    (items[..first].to_vec(), parts)
}

/// Gezik's own menu in one list (widgets/popup-menu.slint): `before`, then for each submenu
/// its opener (id -1 for the first, -2 for the second…, titled as it) and the items after it.
fn menu_lines(before: Vec<MenuEntry>, subs: Vec<(String, Vec<MenuEntry>)>) -> Vec<MenuEntry> {
    let mut lines = before;
    for (k, (title, after)) in subs.into_iter().enumerate() {
        lines.push(MenuEntry { id: -1 - k as i32, title: title.into(), enabled: true });
        lines.extend(after);
    }
    lines
}

/// Whether `id` is an item of a submenu: once one is chosen, Slint leaves the keyboard
/// nowhere (it gives it to the parent menu, which is gone).
fn from_submenu(id: u32) -> bool {
    (COMMAND_FIRST..COMMAND_FIRST + COMMAND_MAX).contains(&id)
}

/// Whether Slint shows its menus as the system's: on Windows and macOS (through muda),
/// unless `SLINT_NO_MUDA` turns that off. Elsewhere Gezik draws its own.
pub fn native_menus() -> bool {
    cfg!(any(windows, target_os = "macos")) && std::env::var_os("SLINT_NO_MUDA").is_none()
}

/// A title for Gezik's own menus: on Windows and macOS they are native menus (muda), which
/// take `&` as the access key mark (and drop it on macOS), so it is doubled to show as itself.
pub fn menu_title(title: &str) -> String {
    if native_menus() { title.replace('&', "&&") } else { title.to_owned() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gezik_menus_list_the_submenus_among_the_items() {
        let entry = |id: i32, title: &str| MenuEntry { id, title: title.into(), enabled: true };
        let lines = menu_lines(
            vec![entry(1, "Open")],
            vec![("Copy path as".to_owned(), vec![entry(2, "Cut")]), ("Commands".to_owned(), Vec::new())],
        );
        let shown: Vec<(i32, &str)> = lines.iter().map(|l| (l.id, l.title.as_str())).collect();
        assert_eq!(shown, [(1, "Open"), (-1, "Copy path as"), (2, "Cut"), (-2, "Commands")]);
    }

    #[test]
    fn a_menu_is_cut_at_its_submenus() {
        let sub = |title: &str, at: usize, n: usize| Submenu {
            title: title.into(),
            at,
            items: vec![(1, "x".to_owned(), true); n],
        };
        let items = ["a", "b", "c", "d"];
        let subs = [sub("Late", 9, 1), sub("Empty", 1, 0), sub("Mid", 2, 1), sub("First", 0, 2)];
        let (before, parts) = split_menu(&items, &subs);
        assert!(before.is_empty());
        let shown: Vec<(&str, Vec<&str>)> = parts.iter().map(|(s, after)| (s.title.as_str(), after.clone())).collect();
        assert_eq!(shown, [("First", vec!["a", "b"]), ("Mid", vec!["c", "d"]), ("Late", vec![])]);
        let many: Vec<Submenu> = (0..6).map(|i| sub("S", i, 1)).collect();
        let (_, parts) = split_menu(&items, &many);
        assert_eq!(parts.len(), MAX_SUBMENUS);
        assert_eq!(parts.last().unwrap().1, ["d"], "the items after one left out stay");
        let (before, parts) = split_menu(&items, &[]);
        assert_eq!((before.len(), parts.len()), (4, 0));
    }

    #[cfg(windows)]
    #[test]
    fn gezik_ids_end_where_explorers_begin() {
        assert_eq!(gezik_platform::FIRST_SHELL_ID, GEZIK_IDS_END);
    }

    #[test]
    fn an_ampersand_shows_as_itself() {
        let shown = if native_menus() { "Copy && keep" } else { "Copy & keep" };
        assert_eq!(menu_title("Copy & keep"), shown);
    }

    #[test]
    fn drop_menu_ids_are_their_own() {
        let others = [
            OPEN_IN_NEW_TAB,
            PIN,
            UNPIN,
            MOVE_UP,
            MOVE_DOWN,
            DUPLICATE_TAB,
            CLOSE_TAB,
            CLOSE_OTHER_TABS,
            OPEN,
            OPEN_DEFAULT,
            RESET_COLUMNS,
            VIEW_LIST,
            VIEW_GRID,
            GRID_SMALL,
            GRID_MEDIUM,
            GRID_LARGE,
            SORT_BY_NAME,
            SORT_BY_MODIFIED,
            SORT_BY_CREATED,
            SORT_BY_TYPE,
            SORT_BY_SIZE,
            SORT_ASC,
            SORT_DESC,
            PREVIEW_PANE,
            APPLY_TO_ALL,
            RESET_FOLDER,
            UNDO,
            REDO,
            PASTE,
            NEW_FOLDER,
            NEW_FILE,
            REFRESH,
            CUT,
            COPY,
            DUPLICATE,
            RENAME,
            TRASH,
            DELETE_PERMANENTLY,
            PASTE_INTO,
            BATCH_RENAME,
            LOCK_TAB,
            UNLOCK_TAB,
            COMMAND_GROUP,
        ];
        let ranges = [TOGGLE_COLUMN_FIRST..RESET_COLUMNS, CONFLICT_FIRST..CONFLICT_FIRST + 4];
        let archives = [EXTRACT_HERE, EXTRACT_TO_OWN, EXTRACT_TO, COMPRESS, COMPRESS_TO, ADD_TO_ARCHIVE];
        for id in [COPY_HERE, MOVE_HERE, CANCEL_DROP, ADD_RULE_FIRST, ADD_RULE_FIRST + 9, PRESET_FIRST, PRESET_SAVE] {
            assert!(!others.contains(&id) && !ranges.iter().any(|r| r.contains(&id)), "{id} is taken");
        }
        // The archive items are their own, distinct, and meet no other range.
        for (i, id) in archives.iter().enumerate() {
            assert!(!archives[..i].contains(id), "{id} twice");
            assert!(!others.contains(id) && !ranges.iter().any(|r| r.contains(id)), "{id} is taken");
            assert!(![COPY_HERE, MOVE_HERE, CANCEL_DROP, PRESET_SAVE].contains(id), "{id} is taken");
            assert!(!(ADD_RULE_FIRST..ADD_RULE_FIRST + 10).contains(id), "{id} is a rule id");
        }
        // The preset ranges meet nothing else, nor each other, and stay below the Shell's ids.
        let presets = [PRESET_FIRST..PRESET_FIRST + PRESET_MAX, PRESET_DELETE_FIRST..PRESET_DELETE_FIRST + PRESET_MAX];
        let singles = others.iter().chain(&[COPY_HERE, MOVE_HERE, CANCEL_DROP, PRESET_SAVE]).chain(&archives);
        for id in singles.copied().chain(ADD_RULE_FIRST..ADD_RULE_FIRST + 10).chain(CONFLICT_FIRST..CONFLICT_FIRST + 4)
        {
            assert!(!presets.iter().any(|r| r.contains(&id)), "{id} is in a preset range");
        }
        assert!(presets[0].end <= PRESET_DELETE_FIRST && presets[1].end < GEZIK_IDS_END);
        assert!(archives.iter().all(|id| (presets[1].end..GEZIK_IDS_END).contains(id)), "below the Shell's ids");
    }

    #[test]
    fn conversion_ids_meet_no_others() {
        // Every range of ids, with the single ids as ranges of one.
        let singles = [
            OPEN_IN_NEW_TAB,
            PIN,
            UNPIN,
            MOVE_UP,
            MOVE_DOWN,
            DUPLICATE_TAB,
            CLOSE_TAB,
            CLOSE_OTHER_TABS,
            OPEN,
            OPEN_DEFAULT,
            RESET_COLUMNS,
            VIEW_LIST,
            VIEW_GRID,
            GRID_SMALL,
            GRID_MEDIUM,
            GRID_LARGE,
            SORT_BY_NAME,
            SORT_BY_MODIFIED,
            SORT_BY_CREATED,
            SORT_BY_TYPE,
            SORT_BY_SIZE,
            SORT_ASC,
            SORT_DESC,
            PREVIEW_PANE,
            APPLY_TO_ALL,
            RESET_FOLDER,
            UNDO,
            REDO,
            PASTE,
            NEW_FOLDER,
            NEW_FILE,
            REFRESH,
            CUT,
            COPY,
            DUPLICATE,
            RENAME,
            TRASH,
            DELETE_PERMANENTLY,
            PASTE_INTO,
            BATCH_RENAME,
            COPY_HERE,
            MOVE_HERE,
            CANCEL_DROP,
            PRESET_SAVE,
            EXTRACT_HERE,
            EXTRACT_TO_OWN,
            EXTRACT_TO,
            COMPRESS,
            COMPRESS_TO,
            ADD_TO_ARCHIVE,
            CONVERT,
            IMAGES_TO_PDF,
            LOCK_TAB,
            UNLOCK_TAB,
            FILTER_SAVE,
            COMMAND_GROUP,
        ];
        let mut ranges: Vec<std::ops::Range<u32>> = singles.iter().map(|id| *id..id + 1).collect();
        ranges.extend([
            TOGGLE_COLUMN_FIRST..RESET_COLUMNS,
            CONFLICT_FIRST..CONFLICT_FIRST + 4,
            ADD_RULE_FIRST..ADD_RULE_FIRST + 10,
            PRESET_FIRST..PRESET_FIRST + PRESET_MAX,
            PRESET_DELETE_FIRST..PRESET_DELETE_FIRST + PRESET_MAX,
            ENCODING_FROM_FIRST..ENCODING_FROM_FIRST + ENCODING_MAX,
            ENCODING_TO_FIRST..ENCODING_TO_FIRST + ENCODING_MAX,
            COMMAND_FIRST..COMMAND_FIRST + COMMAND_MAX,
            CONVERT_PRESET_FIRST..CONVERT_PRESET_FIRST + CONVERT_PRESET_MAX,
            FILTER_FIRST..FILTER_FIRST + FILTER_MAX,
            FILTER_DELETE_FIRST..FILTER_DELETE_FIRST + FILTER_MAX,
        ]);
        for (i, a) in ranges.iter().enumerate() {
            assert!(a.start >= 1 && a.end <= GEZIK_IDS_END, "{a:?}: 1..4096 (0 is a heading, 4096 on the Shell's)");
            for b in &ranges[i + 1..] {
                assert!(a.end <= b.start || b.end <= a.start, "{a:?} meets {b:?}");
            }
        }
        assert!(!ranges.iter().any(|r| r.contains(&HEADING)), "a heading is never an item");
        let encodings = gezik_batch::convert::text::encodings().len() as u32;
        assert!(encodings < ENCODING_MAX, "{encodings} encodings and the first choice fit");
    }

    fn ids(v: Vec<(u32, &str)>) -> Vec<u32> {
        v.into_iter().map(|(id, _)| id).collect()
    }

    #[test]
    fn file_items_depend_on_the_selection() {
        assert_eq!(ids(file_items(true, false, true)), [CUT, COPY, DUPLICATE, RENAME, TRASH, DELETE_PERMANENTLY]);
        let several = ids(file_items(false, true, true));
        assert_eq!(several, [CUT, COPY, PASTE_INTO, DUPLICATE, RENAME, TRASH, DELETE_PERMANENTLY]);
        assert!(!ids(file_items(true, true, false)).contains(&PASTE_INTO));
        assert!(file_items(false, false, false).contains(&(RENAME, "Rename items…")));
    }

    #[test]
    fn the_filter_menu_lists_saves_and_deletes() {
        let names = vec!["Resimler".to_owned(), "Belgeler".to_owned()];
        let items = filter_items(&names, true);
        let ids: Vec<u32> = items.iter().map(|(id, _, _)| *id).collect();
        assert_eq!(ids, [FILTER_FIRST, FILTER_FIRST + 1, FILTER_SAVE, FILTER_DELETE_FIRST, FILTER_DELETE_FIRST + 1]);
        assert_eq!(items[2], (FILTER_SAVE, "Save as…".to_owned(), true));
        assert_eq!(items[3].1, "Delete \"Resimler\"");
        assert!(!filter_items(&[], false)[0].2, "nothing to save: greyed");
    }

    #[test]
    fn preset_menu_lists_saves_and_deletes() {
        let names = ["Photos".to_owned(), "Music".to_owned()];
        let list = preset_items(&names);
        let ids: Vec<u32> = list.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, [PRESET_FIRST, PRESET_FIRST + 1, PRESET_SAVE, PRESET_DELETE_FIRST, PRESET_DELETE_FIRST + 1]);
        assert_eq!(list[3].1, "Delete \"Photos\"");
        assert_eq!(preset_items(&[]), [(PRESET_SAVE, "Save current rules as…".to_owned())]);
        let many: Vec<String> = (0..PRESET_MAX + 5).map(|i| format!("Set {i}")).collect();
        let list = preset_items(&many);
        assert_eq!(list.len(), 2 * PRESET_MAX as usize + 1);
        assert_eq!(list.last().unwrap().0, PRESET_DELETE_FIRST + PRESET_MAX - 1);
        assert!(gezik_core::batch::rules::KINDS.len() <= 10, "Add rule has ids 90-99");
    }

    #[test]
    fn background_items_say_what_undo_does() {
        let items = background_items(Some("Copy 3 items"), None, true);
        assert_eq!(items[0], (UNDO, "Undo Copy 3 items".to_owned()));
        let ids: Vec<u32> = items.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, [UNDO, PASTE, NEW_FOLDER, NEW_FILE, REFRESH]);
        let bare: Vec<u32> = background_items(None, None, false).iter().map(|(id, _)| *id).collect();
        assert_eq!(bare, [NEW_FOLDER, NEW_FILE, REFRESH]);
    }

    #[cfg(windows)]
    #[test]
    fn keys_up_after_a_native_menu_are_released() {
        use gezik_platform::ModifierKeys;
        use slint::platform::Key;
        let all_up = released_modifiers(ModifierKeys::default());
        assert_eq!(all_up.len(), 8);
        assert!(all_up.contains(&Key::Shift) && all_up.contains(&Key::ShiftR) && all_up.contains(&Key::AltGr));
        // Shift still held when the menu closes: its own release comes later as usual.
        let held = released_modifiers(ModifierKeys { left_shift: true, alt: true, ..Default::default() });
        assert!(!held.contains(&Key::Shift) && !held.contains(&Key::Alt));
        assert!(held.contains(&Key::ShiftR) && held.contains(&Key::Control));
    }

    #[test]
    fn folder_rows_offer_new_tab_and_pin_toggle() {
        assert_eq!(ids(items(Place::Row { is_dir: true, pinned: false }, true)), [OPEN_IN_NEW_TAB, PIN]);
        assert_eq!(ids(items(Place::Row { is_dir: true, pinned: true }, true)), [OPEN_IN_NEW_TAB, UNPIN]);
    }

    #[test]
    fn file_rows_rely_on_the_system_menu_on_windows() {
        assert!(items(Place::Row { is_dir: false, pinned: false }, true).is_empty());
        assert_eq!(ids(items(Place::Row { is_dir: false, pinned: false }, false)), [OPEN, OPEN_DEFAULT]);
    }

    #[test]
    fn several_rows_get_the_system_menu_or_open() {
        assert!(items(Place::Rows, true).is_empty());
        assert_eq!(ids(items(Place::Rows, false)), [OPEN]);
    }

    #[test]
    fn pinned_sidebar_rows_can_move_within_bounds() {
        let first = items(Place::Sidebar { pinned_section: true, pinned: true, first: true, last: false }, true);
        assert_eq!(ids(first), [OPEN_IN_NEW_TAB, UNPIN, MOVE_DOWN]);
        let middle = items(Place::Sidebar { pinned_section: true, pinned: true, first: false, last: false }, true);
        assert_eq!(ids(middle), [OPEN_IN_NEW_TAB, UNPIN, MOVE_UP, MOVE_DOWN]);
        let folder = items(Place::Sidebar { pinned_section: false, pinned: false, first: false, last: false }, true);
        assert_eq!(ids(folder), [OPEN_IN_NEW_TAB, PIN]);
    }

    #[test]
    fn one_native_menu_at_a_time() {
        let gate = MenuGate::default();
        let first = gate.claim();
        assert!(first.is_some());
        assert!(gate.claim().is_none(), "a second right-click while one is pending is dropped");
        drop(first);
        assert!(gate.claim().is_some(), "the next menu opens once the first is closed");
    }

    #[test]
    fn header_menu_toggles_each_column_and_resets() {
        use gezik_core::view::default_columns;
        let got = header_items(&default_columns());
        assert_eq!(
            got,
            [
                (TOGGLE_COLUMN_FIRST, "Hide Modified"),
                (TOGGLE_COLUMN_FIRST + 1, "Show Created"),
                (TOGGLE_COLUMN_FIRST + 2, "Hide Type"),
                (TOGGLE_COLUMN_FIRST + 3, "Hide Size"),
                (RESET_COLUMNS, "Reset columns"),
            ]
        );
    }

    #[test]
    fn the_tab_menu_offers_the_lock_and_no_close_on_a_locked_tab() {
        let tab = |only_tab, locked| ids(items(Place::Tab { only_tab, locked }, false));
        assert_eq!(tab(false, false), [DUPLICATE_TAB, LOCK_TAB, CLOSE_TAB, CLOSE_OTHER_TABS]);
        assert_eq!(tab(true, false), [DUPLICATE_TAB, LOCK_TAB, CLOSE_TAB]);
        assert_eq!(tab(false, true), [DUPLICATE_TAB, UNLOCK_TAB, CLOSE_OTHER_TABS]);
        assert_eq!(tab(true, true), [DUPLICATE_TAB, UNLOCK_TAB]);
    }

    #[test]
    fn view_menu_marks_the_current_choices() {
        use gezik_core::view::{GridSize, SortDir, SortKey, SortSpec, ViewMode, ViewSettings};
        let list = view_items(ViewSettings::default(), false);
        let ids: Vec<u32> = list.iter().map(|(id, _)| *id).collect();
        assert_eq!(
            ids,
            [
                VIEW_LIST,
                VIEW_GRID,
                SORT_BY_NAME,
                SORT_BY_MODIFIED,
                SORT_BY_CREATED,
                SORT_BY_TYPE,
                SORT_BY_SIZE,
                SORT_ASC,
                SORT_DESC,
                PREVIEW_PANE,
                APPLY_TO_ALL,
                RESET_FOLDER
            ]
        );
        assert!(list[0].1.starts_with("• ") && !list[1].1.starts_with("• "));
        let grid = ViewSettings {
            mode: ViewMode::Grid,
            sort: SortSpec { key: SortKey::Size, dir: SortDir::Desc },
            grid_size: GridSize::Large,
        };
        let items = view_items(grid, false);
        let marked: Vec<&str> =
            items.iter().filter(|(_, t)| t.starts_with("• ")).map(|(_, t)| t.trim_start_matches("• ")).collect();
        assert_eq!(marked, ["Grid", "Large icons", "Sort by size", "Descending"]);
        assert!(
            view_items(ViewSettings::default(), true).iter().any(|(id, t)| *id == PREVIEW_PANE && t.starts_with("• "))
        );
    }
}
