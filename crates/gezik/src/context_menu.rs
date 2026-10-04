//! Which Gezik items a context menu shows, and what they do.
//!
//! On Windows, rows and sidebar entries get the Explorer menu with Gezik's items on top;
//! tabs (and everything on macOS/Linux) get a Slint menu.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use gezik_core::nav::Location;
use gezik_platform::MenuTarget;
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::navigation::Navigator;
use crate::sidebar::{SECTION_PINNED, Sidebar};
use crate::view::View;
use crate::{AppWindow, MenuEntry};

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
        Place::Tab { only_tab } => {
            out.push((DUPLICATE_TAB, "Duplicate"));
            out.push((CLOSE_TAB, "Close"));
            if !only_tab {
                out.push((CLOSE_OTHER_TABS, "Close other tabs"));
            }
        }
    }
    out
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
    sidebar: Sidebar,
    /// What the open Slint menu is for.
    subject: Rc<RefCell<Option<Subject>>>,
    #[cfg_attr(not(windows), allow(dead_code))]
    native_menu: MenuGate,
}

impl Menus {
    pub fn new(window: &AppWindow, nav: Navigator, view: View, sidebar: Sidebar) -> Menus {
        let menus = Menus {
            window: window.as_weak(),
            nav,
            view,
            sidebar,
            subject: Rc::default(),
            native_menu: MenuGate::default(),
        };
        window.on_menu_activated({
            let menus = menus.clone();
            move |id| {
                let subject = menus.subject.borrow_mut().take();
                if let (Ok(id), Some(subject)) = (u32::try_from(id), subject) {
                    menus.run(id, subject);
                }
            }
        });
        menus
    }

    /// Right-click on file list row `index` (already selected), at window position `x`, `y`.
    pub fn row(&self, index: i32, x: f32, y: f32) {
        self.row_menu(index, x, y, false);
    }

    /// Right-click on empty space in the file list. Only the Windows menu has items for it.
    pub fn background(&self) {
        self.background_menu(None);
    }

    /// Shift+F10 or the Menu key on the file list: the menu of row `index` (selected and
    /// scrolled into view), or of the folder's background if it is -1, at window position
    /// `x`, `y` (also for the Windows menu, which a right-click opens at the cursor).
    pub fn keyboard(&self, index: i32, x: f32, y: f32) {
        if index >= 0 {
            self.row_menu(index, x, y, true);
        } else {
            self.background_menu(Some((x, y)));
        }
    }

    fn row_menu(&self, index: i32, x: f32, y: f32, at_position: bool) {
        let Ok(i) = usize::try_from(index) else { return };
        let at = at_position.then_some((x, y));
        if self.view.is_selected(i) && self.view.selection_count() > 1 {
            let paths = self.view.selected_paths();
            return self.open(Subject::Rows(paths.clone()), Place::Rows, MenuTarget::Items(paths), x, y, at);
        }
        let Some((path, is_dir)) = self.view.entry_path(i) else { return };
        let place = Place::Row { is_dir, pinned: is_dir && self.sidebar.is_pinned(&path) };
        self.open(Subject::Row(path.clone()), place, MenuTarget::Item(path), x, y, at);
    }

    fn background_menu(&self, at: Option<(f32, f32)>) {
        let Location::Path(dir) = self.nav.active_location() else { return };
        if cfg!(windows) {
            self.open_native(None, MenuTarget::Background(dir), Vec::new(), at);
        }
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
        self.open(Subject::SidebarEntry(path.clone()), place, MenuTarget::Item(path), x, y, None);
    }

    /// Right-click on tab `index`, at window position `x`, `y`. Tabs get Gezik's own menu
    /// everywhere.
    pub fn tab(&self, index: usize, x: f32, y: f32) {
        let Some(id) = self.nav.tab_id(index) else { return };
        let place = Place::Tab { only_tab: self.nav.tab_count() == 1 };
        *self.subject.borrow_mut() = Some(Subject::Tab(id));
        self.open_slint(&items(place, false), x, y);
    }

    /// `at`: where the Windows menu opens (window position), else at the cursor.
    fn open(&self, subject: Subject, place: Place, target: MenuTarget, x: f32, y: f32, at: Option<(f32, f32)>) {
        if cfg!(windows) {
            self.open_native(Some(subject), target, items(place, true), at);
        } else {
            *self.subject.borrow_mut() = Some(subject);
            self.open_slint(&items(place, false), x, y);
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
        items: Vec<(u32, &'static str)>,
        at: Option<(f32, f32)>,
    ) {
        let Some(claim) = self.native_menu.claim() else { return };
        let menus = self.clone();
        slint::Timer::single_shot(std::time::Duration::from_millis(16), move || {
            let Some(window) = menus.window.upgrade() else { return };
            let handle = window.window().window_handle();
            let scale = window.window().scale_factor();
            let at = at.map(|(x, y)| ((x * scale).round() as i32, (y * scale).round() as i32));
            let outcome = gezik_platform::show_shell_menu(&handle, &target, &items, at);
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
        _items: Vec<(u32, &'static str)>,
        _at: Option<(f32, f32)>,
    ) {
    }

    fn open_slint(&self, items: &[(u32, &str)], x: f32, y: f32) {
        let Some(window) = self.window.upgrade() else { return };
        if items.is_empty() {
            return;
        }
        let entries: Vec<MenuEntry> = items
            .iter()
            .map(|(id, title)| MenuEntry { id: i32::try_from(*id).unwrap_or(0), title: (*title).into() })
            .collect();
        window.set_menu_entries(ModelRc::new(VecModel::from(entries)));
        window.invoke_show_menu(x, y);
    }

    fn run(&self, id: u32, subject: Subject) {
        match (id, subject) {
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
            (CLOSE_OTHER_TABS, Subject::Tab(id)) => {
                if let Some(i) = self.nav.tab_index(id) {
                    self.nav.close_other_tabs(i);
                }
            }
            (OPEN | OPEN_DEFAULT, Subject::Row(path)) => {
                if let Err(err) = open::that_detached(&path)
                    && let Some(window) = self.window.upgrade()
                {
                    window.set_status(format!("Cannot open {}: {err}", path.display()).into());
                }
            }
            (OPEN, Subject::Rows(paths)) => {
                for path in paths {
                    if let Err(err) = open::that_detached(&path)
                        && let Some(window) = self.window.upgrade()
                    {
                        window.set_status(format!("Cannot open {}: {err}", path.display()).into());
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(v: Vec<(u32, &str)>) -> Vec<u32> {
        v.into_iter().map(|(id, _)| id).collect()
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
    fn tab_menu_hides_close_others_for_a_single_tab() {
        assert_eq!(ids(items(Place::Tab { only_tab: false }, true)), [DUPLICATE_TAB, CLOSE_TAB, CLOSE_OTHER_TABS]);
        assert_eq!(ids(items(Place::Tab { only_tab: true }, true)), [DUPLICATE_TAB, CLOSE_TAB]);
    }
}
