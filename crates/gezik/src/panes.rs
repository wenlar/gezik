//! The panes of the window (spec 10 §3.1): a pane's navigator, view, filter, search, address
//! bar and folder sizes, together. One pane today; window callbacks and other modules reach the
//! active one, a pane's own background results and timers reach theirs by id (a closed pane's
//! are dropped).
//!
//! What the window shows of a pane is its row of the window's `panes` model (`PaneData`,
//! written with [`edit`]); what its `PaneView` keeps and only tells (the scroll offset, the
//! field texts, where its parts are) Rust keeps a [`Mirror`] of (spec 10 §3.2).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use slint::{Model, ModelRc, SharedString, VecModel};

use crate::{PaneData, PaneFocus, PaneGeometry};

use crate::filter::Filter;
use crate::folder_sizes::FolderSizes;
use crate::navigation::Navigator;
use crate::path_box::PathBox;
use crate::search::Searches;
use crate::view::View;
use crate::windows::{self, Slot, WindowId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaneId(u32);

#[derive(Clone)]
pub struct Pane {
    pub id: PaneId,
    pub nav: Navigator,
    pub view: View,
    pub filter: Filter,
    pub search: Searches,
    pub path_box: PathBox,
    pub folder_sizes: FolderSizes,
}

/// What Rust knows of the state a pane's `PaneView` keeps (each changes there, and the pane
/// says so): read where the window's properties were read before 10a-3.
#[derive(Default)]
pub struct Mirror {
    /// The file list's scroll offset (content-y).
    pub scroll: Cell<f32>,
    pub path_editing: Cell<bool>,
    pub rename_text: RefCell<SharedString>,
    pub filter_text: RefCell<SharedString>,
    pub search_text: RefCell<SharedString>,
    pub search_content: RefCell<SharedString>,
    pub focus: RefCell<PaneFocus>,
    pub geometry: RefCell<PaneGeometry>,
}

/// A window's panes (10g): its list, the active one, and its `panes` model with a row for
/// each pane, made with its id (its parts write to it before it is installed), in the order
/// of `rows`.
#[derive(Default)]
pub struct Panes {
    list: RefCell<Vec<Pane>>,
    /// The index in `list` of the pane the keyboard and the window's bars act on.
    active: Cell<usize>,
    model: Rc<VecModel<PaneData>>,
    rows: RefCell<Vec<(PaneId, Rc<Mirror>)>>,
}

thread_local! {
    /// Each window's panes.
    pub(crate) static PANES: Slot<Rc<Panes>> = const { Slot::new() };
    /// Pane ids are the process's: one window has the pane of an id.
    static NEXT_ID: Cell<u32> = const { Cell::new(0) };
}

/// The current window's panes (made with its first pane id).
fn here() -> Rc<Panes> {
    PANES.with(|s| s.current_or(Rc::default))
}

/// The window that has pane `id`, its panes and the pane's row there.
fn row_of(id: PaneId) -> Option<(WindowId, Rc<Panes>, usize)> {
    // Only the matching window's panes are cloned out; the borrow ends before any call out.
    PANES.with(|s| {
        s.find_map(|window, panes| {
            let row = panes.rows.borrow().iter().position(|(of, _)| *of == id)?;
            Some((window, panes.clone(), row))
        })
    })
}

/// A new pane's id, for its parts to carry before the pane is installed, and its row.
pub fn next_id() -> PaneId {
    let id = PaneId(NEXT_ID.with(|n| n.replace(n.get() + 1)));
    let panes = here();
    panes.rows.borrow_mut().push((id, Rc::default()));
    panes.model.push(new_data());
    id
}

/// What a pane shows before anything is written: the window's old property defaults.
fn new_data() -> PaneData {
    PaneData {
        focus_row: -1,
        grid_size: 96.0,
        renaming_index: -1,
        search_filters: gezik_i18n::t!("Filters").as_ref().into(),
        reveal_index: -1,
        ..Default::default()
    }
}

/// The window's `panes` model.
pub fn model() -> ModelRc<PaneData> {
    ModelRc::from(here().model.clone())
}

/// What the window shows of pane `id` now.
pub fn data(id: PaneId) -> Option<PaneData> {
    let (_, panes, row) = row_of(id)?;
    panes.model.row_data(row)
}

/// Changes what the window shows of pane `id` with `f`, in one write (a write makes every
/// binding of the pane on its row look again): nothing is written if nothing changed.
pub fn edit(id: PaneId, f: impl FnOnce(&mut PaneData)) {
    let Some((_, panes, row)) = row_of(id) else { return };
    let model = panes.model.clone();
    let Some(old) = model.row_data(row) else { return };
    let mut new = old.clone();
    f(&mut new);
    if new != old {
        model.set_row_data(row, new);
    }
}

/// Pane `id`'s mirror (an empty one for a pane that is gone).
pub fn mirror(id: PaneId) -> Rc<Mirror> {
    row_of(id).and_then(|(_, panes, row)| panes.rows.borrow().get(row).map(|(_, m)| m.clone())).unwrap_or_default()
}

/// The mirror of the pane on row `index` (Slint callbacks name a pane by its row).
pub fn mirror_at(index: i32) -> Rc<Mirror> {
    let row = usize::try_from(index).ok();
    row.and_then(|i| here().rows.borrow().get(i).map(|(_, m)| m.clone())).unwrap_or_default()
}

fn bump(n: &mut i32) {
    *n = n.wrapping_add(1);
}

/// Asks the pane to scroll its list to `to`; a reveal asked before in the same write is
/// dropped (it came first).
pub fn scroll_to(d: &mut PaneData, to: f32) {
    bump(&mut d.scroll_seq);
    d.scroll_to = to;
    d.reveal_index = -1;
    bump(&mut d.scroll_request);
}

/// Asks the pane to scroll entry `index` into view, after any scroll asked before.
pub fn reveal(d: &mut PaneData, index: i32) {
    d.reveal_index = index;
    bump(&mut d.scroll_request);
}

/// Asks the address bar to start (`edit`, the whole path selected) or end typing.
pub fn path_editing(d: &mut PaneData, edit: bool) {
    d.path_edit = edit;
    bump(&mut d.path_request);
}

/// Asks the address field to show `text`, the cursor at byte `at`.
pub fn path_text(d: &mut PaneData, text: &str, at: i32) {
    d.path_text = text.into();
    d.path_text_at = at;
    bump(&mut d.path_text_request);
}

/// Asks the filter bar's field for the keyboard, the cursor at byte `at` (-1: all selected).
pub fn focus_filter(d: &mut PaneData, at: i32) {
    d.filter_focus_at = at;
    bump(&mut d.filter_focus_request);
}

/// Asks the search bar's name field for the keyboard, its text selected.
pub fn focus_search(d: &mut PaneData) {
    bump(&mut d.search_focus_request);
}

/// Asks the pane for its keyboard menu.
pub fn keyboard_menu(d: &mut PaneData) {
    bump(&mut d.menu_request);
}

/// Puts `text` in pane `id`'s filter field if it shows something else (so the cursor does
/// not jump while typing).
pub fn set_filter_text(id: PaneId, text: &str) {
    set_field(id, text, |m| &m.filter_text, |d| (&mut d.filter_text, &mut d.filter_text_request));
}

pub fn set_search_text(id: PaneId, text: &str) {
    set_field(id, text, |m| &m.search_text, |d| (&mut d.search_text, &mut d.search_text_request));
}

pub fn set_search_content(id: PaneId, text: &str) {
    set_field(id, text, |m| &m.search_content, |d| (&mut d.search_content, &mut d.search_content_request));
}

fn set_field(
    id: PaneId,
    text: &str,
    shown: impl Fn(&Mirror) -> &RefCell<SharedString>,
    field: impl FnOnce(&mut PaneData) -> (&mut SharedString, &mut i32),
) {
    let mirror = mirror(id);
    if shown(&mirror).borrow().as_str() == text {
        return;
    }
    *shown(&mirror).borrow_mut() = text.into();
    edit(id, |d| {
        let (value, request) = field(d);
        *value = text.into();
        bump(request);
    });
}

/// Makes `pane` reachable once all its parts exist (in the order of their rows: left first).
pub fn install(pane: Pane) {
    here().list.borrow_mut().push(pane);
}

/// Takes pane `id` out with its row (the window drops its `PaneView`); the active pane stays
/// the same one. The pane, if it was there.
pub fn remove(id: PaneId) -> Option<Pane> {
    let (window, panes, row) = row_of(id)?;
    windows::enter(window, || {
        let active = active_id();
        let pane = {
            let mut list = panes.list.borrow_mut();
            let at = list.iter().position(|p| p.id == id)?;
            list.remove(at)
        };
        panes.rows.borrow_mut().remove(row);
        panes.model.remove(row);
        let at = active.and_then(index_of).unwrap_or(0);
        panes.active.set(at);
        Some(pane)
    })
}

/// How many panes are open.
pub fn count() -> usize {
    here().list.borrow().len()
}

/// The place (0 left, 1 right) of the active pane.
pub fn active_index() -> usize {
    here().active.get()
}

/// Makes the pane at place `index` the active one (nothing else: see `dual::activate`).
pub fn set_active(index: usize) {
    if index < count() {
        here().active.set(index);
    }
}

pub fn active_id() -> Option<PaneId> {
    with_active(|p| p.id)
}

pub fn is_active(id: PaneId) -> bool {
    active_id() == Some(id)
}

/// The place of pane `id`.
pub fn index_of(id: PaneId) -> Option<usize> {
    here().list.borrow().iter().position(|p| p.id == id)
}

/// The active pane (its parts are handles). main.rs installs the left pane before anything
/// that asks.
/// Not inlined: hundreds of places ask.
#[inline(never)]
pub fn active() -> Pane {
    with_active(Pane::clone).expect("the left pane is installed first")
}

#[inline(never)]
pub fn active_nav() -> Navigator {
    with_active(|p| p.nav.clone()).expect("the left pane is installed first")
}

#[inline(never)]
pub fn active_view() -> View {
    with_active(|p| p.view.clone()).expect("the left pane is installed first")
}

/// The pane at place `index` (a Slint callback's pane number).
pub fn at(index: i32) -> Option<Pane> {
    with_row(index, Pane::clone)
}

/// Runs `f` with the pane at place `index` (a Slint callback's pane number), if it is open.
pub fn with_row<R>(index: i32, f: impl FnOnce(&Pane) -> R) -> Option<R> {
    let index = usize::try_from(index).ok()?;
    with_picked(&here().list, |panes| panes.get(index), f)
}

/// Every open pane of the window, left first.
pub fn all() -> Vec<Pane> {
    here().list.borrow().clone()
}

/// Every open pane of every window (a settings change).
pub fn everywhere() -> Vec<Pane> {
    PANES.with(Slot::all).into_iter().flat_map(|(_, panes)| panes.list.borrow().clone()).collect()
}

/// Runs `f` with the active pane, if one is installed.
pub fn with_active<R>(f: impl FnOnce(&Pane) -> R) -> Option<R> {
    let panes = here();
    with_picked(&panes.list, |list| list.get(panes.active.get()), f)
}

/// Runs `f` with pane `id`, if it is still open, in its window: a pane's own background
/// results and timers act in the window the pane is in.
pub fn with_id<R>(id: PaneId, f: impl FnOnce(&Pane) -> R) -> Option<R> {
    let (window, panes, _) = row_of(id)?;
    windows::enter(window, || with_picked(&panes.list, |list| list.iter().find(|p| p.id == id), f))
}

/// Runs `f` with a clone of what `pick` takes from `list`, taken out of the borrow: `f` may
/// reach the list again (a second borrow would abort the release build).
fn with_picked<T: Clone, R>(
    list: &RefCell<Vec<T>>,
    pick: impl FnOnce(&[T]) -> Option<&T>,
    f: impl FnOnce(&T) -> R,
) -> Option<R> {
    let item = pick(&list.borrow()).cloned()?;
    Some(f(&item))
}

#[cfg(test)]
mod tests {
    use super::*;

    thread_local! {
        static LIST: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
    }

    fn by_id(id: u32, f: impl FnOnce(&u32) -> u32) -> Option<u32> {
        LIST.with(|list| with_picked(list, |l| l.iter().find(|&&x| x == id), f))
    }

    fn first(f: impl FnOnce(&u32) -> u32) -> Option<u32> {
        LIST.with(|list| with_picked(list, |l| l.first(), f))
    }

    #[test]
    fn nothing_before_install() {
        assert_eq!(first(|x| *x), None);
        assert!(with_active(|p| p.id).is_none());
    }

    #[test]
    fn unknown_id_is_none() {
        LIST.with(|l| l.borrow_mut().extend([1, 2]));
        assert_eq!(by_id(2, |x| *x), Some(2));
        assert_eq!(by_id(7, |x| *x), None);
        assert!(with_id(PaneId(7), |p| p.id).is_none());
    }

    #[test]
    fn edit_writes_a_panes_row() {
        let (a, b) = (next_id(), next_id());
        edit(b, |d| d.focus_row = 4);
        assert_eq!(data(b).map(|d| d.focus_row), Some(4));
        assert_eq!(data(a).map(|d| d.focus_row), Some(-1));
        // A pane that is not there: nothing written, nothing read.
        edit(PaneId(99), |d| d.focus_row = 1);
        assert!(data(PaneId(99)).is_none());
        assert_eq!(model().row_count(), 2);
    }

    #[test]
    fn scroll_and_reveal_keep_their_order() {
        let mut d = new_data();
        scroll_to(&mut d, -40.0);
        reveal(&mut d, 7);
        // The pane scrolls, then reveals.
        assert_eq!((d.scroll_seq, d.scroll_to, d.reveal_index, d.scroll_request), (1, -40.0, 7, 2));
        // A scroll after a reveal drops the reveal; a reveal alone scrolls nowhere first.
        scroll_to(&mut d, 0.0);
        assert_eq!((d.scroll_seq, d.reveal_index, d.scroll_request), (2, -1, 3));
        reveal(&mut d, 2);
        assert_eq!((d.scroll_seq, d.reveal_index, d.scroll_request), (2, 2, 4));
    }

    #[test]
    fn a_field_is_set_only_when_it_shows_something_else() {
        let id = next_id();
        set_filter_text(id, "");
        assert_eq!(data(id).map(|d| d.filter_text_request), Some(0));
        set_filter_text(id, "*.rs");
        assert_eq!(data(id).map(|d| (d.filter_text.to_string(), d.filter_text_request)), Some(("*.rs".into(), 1)));
        // Typed in the field (the pane says so): the same text is not sent back.
        *mirror(id).filter_text.borrow_mut() = "*.rst".into();
        set_filter_text(id, "*.rst");
        assert_eq!(data(id).map(|d| d.filter_text_request), Some(1));
    }

    #[test]
    fn nested_calls_do_not_borrow_twice() {
        LIST.with(|l| l.borrow_mut().push(3));
        assert_eq!(first(|outer| first(|inner| outer + inner).unwrap()), Some(6));
        // The list is free inside `f`: even a write works.
        first(|x| {
            LIST.with(|l| l.borrow_mut().push(*x));
            0
        });
        assert_eq!(LIST.with(|l| l.borrow().len()), 2);
    }

    /// 10g: each window has its own panes; a pane's own results find it from any window and
    /// act in its window.
    #[test]
    fn each_window_has_its_own_panes() {
        use crate::dual::tests::test_pane;
        crate::windows::tests::platform();
        let a = windows::install(crate::AppWindow::new().expect("a headless window"));
        let b = windows::install(crate::AppWindow::new().expect("a headless window"));
        let in_window = |window| {
            windows::enter(window, || {
                let pane = test_pane();
                install(pane.clone());
                pane.id
            })
        };
        let (left_a, left_b) = (in_window(a), in_window(b));
        assert_eq!(windows::enter(a, || at(0).map(|p| p.id)), Some(left_a));
        assert_eq!(windows::enter(b, || at(0).map(|p| p.id)), Some(left_b));
        assert_eq!(windows::enter(a, count), 1);
        assert_eq!(windows::enter(a, || with_id(left_b, |_| windows::current())), Some(b));
        assert_eq!(windows::enter(b, || with_id(left_a, |p| p.id)), Some(left_a));
        assert!(windows::enter(b, || data(left_a).is_some()), "its row is found from the other window");
        assert_eq!(everywhere().len(), 2);
    }
}
