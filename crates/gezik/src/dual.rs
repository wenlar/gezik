//! The second pane (spec 10 §4): opening and closing it, which pane is active (one folder
//! watcher, the active pane's), and what state.toml keeps of them.

use std::cell::RefCell;

use gezik_config::store::ConfigStore;
use gezik_core::nav::{Location, Session};
use slint::ComponentHandle;

use crate::AppWindow;
use crate::panes::{self, Pane, PaneId};

/// Narrower than this the second pane does not open (spec 10 §4.1).
const MIN_WIDTH: f32 = 640.0;

#[derive(Default)]
struct Dual {
    window: slint::Weak<AppWindow>,
    /// Where state.toml is; none for a second window (it writes no state) or without a config
    /// folder.
    store: Option<ConfigStore>,
    /// `[session] restore`: off, state.toml keeps no tabs of either pane.
    restore: bool,
    /// The right pane's tabs while it is closed (none before it was first opened).
    right: Option<Session>,
    /// The left pane's share of the two panes' width (0.2–0.8).
    split: f32,
}

thread_local! {
    static DUAL: RefCell<Dual> = RefCell::default();
}

fn window() -> Option<AppWindow> {
    DUAL.with(|d| d.borrow().window.upgrade())
}

/// Call once the left pane is installed: `right` is the right pane's tabs of last time,
/// `split` the left pane's share in thousandths.
pub fn install(window: &AppWindow, store: Option<ConfigStore>, restore: bool, right: Session, split: Option<u16>) {
    let right = (!right.is_empty()).then_some(right);
    let split = split.map_or(0.5, |split| f32::from(split) / 1000.0);
    window.set_pane_split(split * 100.0);
    window.set_right_split(100.0 - split * 100.0);
    DUAL.with(|d| *d.borrow_mut() = Dual { window: window.as_weak(), store, restore, right, split });
}

/// `[session] restore` changed: off, state.toml forgets the right pane's tabs too.
pub fn set_restore(restore: bool) {
    let changed = DUAL.with(|d| std::mem::replace(&mut d.borrow_mut().restore, restore) != restore);
    if changed {
        save();
    }
}

pub fn is_open() -> bool {
    panes::count() > 1
}

/// `toggle-dual-pane`.
pub fn toggle() {
    if is_open() { close() } else { open(None) }
}

/// Opens the right pane with the tabs it had when it closed (else at the left pane's folder);
/// it becomes the active one, unless `start` (the session of last time) says which is.
pub fn open(start: Option<usize>) {
    let Some(window) = window() else { return };
    if is_open() {
        return;
    }
    let scale = window.window().scale_factor();
    if start.is_none() && (window.window().size().width as f32 / scale) < MIN_WIDTH {
        window.set_view_two_panes(false);
        return panes::active_view().note("The window is too narrow for two panes".to_owned());
    }
    let left = panes::active();
    let session = DUAL.with(|d| d.borrow_mut().right.take());
    let session = session.unwrap_or_else(|| Session::single(left.nav.active_location()));
    let right = make(&left, session.clone());
    connect(&right);
    panes::install(right.clone());
    let (store, restore) = DUAL.with(|d| (d.borrow().store.clone(), d.borrow().restore));
    if let Some(store) = store {
        right.nav.keep_session(session, restore, move |session| {
            let session = session.clone();
            store.update_state(move |state| state.right_session = session);
        });
    }
    right.nav.install();
    window.set_dual(true);
    window.set_view_two_panes(true);
    activate(start.unwrap_or(1));
    save();
}

/// Closes the right pane: everything of it goes but its tabs (kept here and in state.toml).
/// Its locked tabs do not keep it open.
pub fn close() {
    let Some(window) = window() else { return };
    let Some(right) = panes::at(1) else { return };
    activate(0);
    let session = release(right.id);
    DUAL.with(|d| d.borrow_mut().right = session);
    window.set_dual(false);
    window.set_view_two_panes(false);
    // A field of the closed pane may have had the keyboard: it went with the pane.
    window.invoke_focus_list();
    save();
}

/// Takes pane `id` out after stopping what it runs: its search, folder sizes, watcher and
/// media requests. Its tabs.
fn release(id: PaneId) -> Option<Session> {
    let pane = panes::with_id(id, Pane::clone)?;
    pane.search.leaving();
    pane.folder_sizes.shown(&Location::Drives);
    pane.nav.unwatch();
    pane.view.clear();
    let session = pane.nav.session();
    drop(pane);
    panes::remove(id);
    Some(session)
}

/// A pane like `like` (its settings, media, folder view memory, dialogs, history) showing
/// `session`'s tabs; it loads nothing until its navigator is installed.
fn make(like: &Pane, session: Session) -> Pane {
    let id = panes::next_id();
    let view = like.view.for_pane(id);
    let nav = like.nav.for_pane(id, view.clone(), session);
    Pane {
        id,
        folder_sizes: like.folder_sizes.for_pane(id, view.clone()),
        path_box: like.path_box.for_pane(id, nav.clone()),
        filter: like.filter.for_pane(id, view.clone()),
        search: like.search.for_pane(id, nav.clone(), view.clone()),
        nav,
        view,
    }
}

/// What a pane tells the window's parts: its search follows its location; the sidebar, the
/// preview and the file operations hear only the active pane.
pub fn connect(pane: &Pane) {
    let id = pane.id;
    pane.nav.on_changed(move |location| {
        panes::with_id(id, |p| p.search.location_changed(location));
        if panes::is_active(id) {
            crate::sidebar::with_current(|s| s.follow(location));
        }
    });
    pane.view.on_selection_changed(move || {
        if panes::is_active(id) {
            crate::preview::with_current(crate::preview::Preview::schedule);
        }
    });
    pane.view.on_shown(move || {
        if panes::is_active(id) {
            crate::operations::with_current(crate::operations::Operations::shown);
        }
    });
}

/// Makes the pane at place `index` the active one: a rename or address typed in the other one
/// ends, the watcher moves over and the newly active pane's folder is read again (quietly);
/// the window's title, status line, sidebar and preview follow it.
pub fn activate(index: usize) {
    if index == panes::active_index() || index >= panes::count() {
        return;
    }
    crate::operations::with_current(crate::operations::Operations::end_rename_for_switch);
    let old = panes::active();
    if panes::mirror(old.id).path_editing.get() {
        panes::edit(old.id, |d| panes::path_editing(d, false));
    }
    old.nav.unwatch();
    panes::set_active(index);
    let pane = panes::active();
    if let Some(window) = window() {
        window.set_active_pane(i32::try_from(index).unwrap_or(0));
        window.set_address_box(panes::mirror(pane.id).geometry.borrow().clone());
    }
    pane.nav.update_chrome();
    pane.view.update_status();
    pane.nav.rewatch();
    crate::preview::with_current(crate::preview::Preview::schedule);
}

/// A click or an action in the pane on row `row` (a Slint callback's) makes it the active one.
pub fn pick(row: i32) {
    if let Ok(index) = usize::try_from(row) {
        activate(index);
    }
}

/// `focus-other-pane`: false (the key goes on) with one pane.
pub fn focus_other() -> bool {
    if !is_open() {
        return false;
    }
    activate(1 - panes::active_index());
    true
}

/// The splitter between the panes was dragged to `share` of their room.
pub fn split_moved(share: f32) {
    let split = share.clamp(0.2, 0.8);
    DUAL.with(|d| d.borrow_mut().split = split);
    if let Some(window) = window() {
        window.set_pane_split(split * 100.0);
        window.set_right_split(100.0 - split * 100.0);
    }
}

/// A double-click on the splitter: the panes share the room evenly.
pub fn split_reset() {
    split_moved(0.5);
    split_done();
}

/// The splitter was let go: state.toml keeps the share.
pub fn split_done() {
    let (store, split) = DUAL.with(|d| (d.borrow().store.clone(), d.borrow().split));
    let split = (split * 1000.0).round() as u16;
    if let Some(store) = store {
        store.update_state(move |state| state.pane_split = (split != 500).then_some(split));
    }
}

/// Two panes and the preview came down to their narrowest: the preview closes first.
pub fn cramped() {
    crate::preview::with_current(|preview| {
        if preview.is_pane_open() {
            preview.set_pane_open(false);
            panes::active_view().note("The preview pane closed to keep room for two panes".to_owned());
        }
    });
}

/// Writes whether the second pane is open and which pane is active, and the right pane's
/// tabs while it is closed; with `[session] restore` off, none of them. Also at quit: a pane
/// switch writes nothing by itself.
pub fn save() {
    let (store, restore, right) = DUAL.with(|d| {
        let d = d.borrow();
        (d.store.clone(), d.restore, d.right.clone())
    });
    let Some(store) = store else { return };
    let open = is_open();
    let (dual, active) = if restore { (open, panes::active_index()) } else { (false, 0) };
    store.update_state(move |state| {
        state.dual = dual;
        state.active_pane = active;
        if !open {
            state.right_session = right.filter(|_| restore).unwrap_or_default();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::Model;

    /// A pane as main.rs makes the left one, with no window.
    fn test_pane() -> Pane {
        let id = panes::next_id();
        let window = slint::Weak::default();
        let dialogs = crate::dialog::Dialogs::detached();
        let memory = std::rc::Rc::default();
        let view = crate::view::View::new(id, window.clone(), crate::media::Media::idle(), memory, None);
        let drives = Session::single(Location::Drives);
        let nav =
            crate::navigation::Navigator::new(id, window.clone(), view.clone(), drives, Vec::new(), Location::Drives);
        Pane {
            id,
            folder_sizes: crate::folder_sizes::FolderSizes::new(id, window.clone(), view.clone()),
            path_box: crate::path_box::PathBox::new(id, window.clone(), nav.clone(), None, Vec::new()),
            filter: crate::filter::Filter::new(id, window.clone(), view.clone(), dialogs.clone(), None),
            search: crate::search::Searches::new(id, window, nav.clone(), view.clone(), dialogs),
            nav,
            view,
        }
    }

    #[test]
    fn a_closed_pane_lets_go_of_everything_but_its_tabs() {
        let left = test_pane();
        connect(&left);
        panes::install(left.clone());
        let mut tabs = Session::single(Location::Trash);
        tabs.tabs.push(gezik_core::nav::SessionTab { location: Location::Drives, locked: true });
        let right = make(&left, tabs.clone());
        connect(&right);
        panes::install(right.clone());
        assert_eq!(panes::count(), 2);
        let (view, nav, id) = (right.view.downgrade(), right.nav.downgrade(), right.id);
        drop(right);
        // Locked tabs do not keep it open; its tabs are what is left of it.
        assert_eq!(release(id), Some(tabs));
        assert!(view.upgrade().is_none(), "the right pane's view is gone");
        assert!(nav.upgrade().is_none(), "the right pane's navigator is gone");
        assert_eq!(panes::count(), 1);
        assert_eq!(panes::model().row_count(), 1, "its row is gone too");
        assert_eq!(panes::active_id(), Some(left.id));
    }
}
