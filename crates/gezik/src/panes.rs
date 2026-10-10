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
use std::thread::LocalKey;

use slint::{Model, ModelRc, SharedString, VecModel};

use crate::{PaneData, PaneFocus, PaneGeometry};

use crate::filter::Filter;
use crate::folder_sizes::FolderSizes;
use crate::navigation::Navigator;
use crate::path_box::PathBox;
use crate::search::Searches;
use crate::view::View;

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

thread_local! {
    /// The panes of this (UI) thread.
    static PANES: RefCell<Vec<Pane>> = const { RefCell::new(Vec::new()) };
    /// The index in `PANES` of the pane the keyboard and the window's bars act on.
    static ACTIVE: Cell<usize> = const { Cell::new(0) };
    static NEXT_ID: Cell<u32> = const { Cell::new(0) };
    /// The window's `panes`: a row for each pane, made with its id (its parts write to it
    /// before it is installed), in the order of `ROWS`.
    static MODEL: Rc<VecModel<PaneData>> = Rc::default();
    static ROWS: RefCell<Vec<(PaneId, Rc<Mirror>)>> = const { RefCell::new(Vec::new()) };
}

/// A new pane's id, for its parts to carry before the pane is installed, and its row.
pub fn next_id() -> PaneId {
    let id = PaneId(NEXT_ID.with(|n| n.replace(n.get() + 1)));
    ROWS.with(|r| r.borrow_mut().push((id, Rc::default())));
    MODEL.with(|m| m.push(new_data()));
    id
}

/// What a pane shows before anything is written: the window's old property defaults.
fn new_data() -> PaneData {
    PaneData {
        focus_row: -1,
        grid_size: 96.0,
        renaming_index: -1,
        search_filters: "Filters".into(),
        reveal_index: -1,
        ..Default::default()
    }
}

/// The window's `panes` model.
pub fn model() -> ModelRc<PaneData> {
    ModelRc::from(MODEL.with(Rc::clone))
}

fn row_of(id: PaneId) -> Option<usize> {
    ROWS.with(|r| r.borrow().iter().position(|(of, _)| *of == id))
}

/// What the window shows of pane `id` now.
pub fn data(id: PaneId) -> Option<PaneData> {
    MODEL.with(|m| m.row_data(row_of(id)?))
}

/// Changes what the window shows of pane `id` with `f`, in one write (a write makes every
/// binding of the pane on its row look again): nothing is written if nothing changed.
pub fn edit(id: PaneId, f: impl FnOnce(&mut PaneData)) {
    let Some(row) = row_of(id) else { return };
    let model = MODEL.with(Rc::clone);
    let Some(old) = model.row_data(row) else { return };
    let mut new = old.clone();
    f(&mut new);
    if new != old {
        model.set_row_data(row, new);
    }
}

/// Pane `id`'s mirror (an empty one for a pane that is gone).
pub fn mirror(id: PaneId) -> Rc<Mirror> {
    ROWS.with(|r| r.borrow().iter().find(|(of, _)| *of == id).map(|(_, m)| m.clone())).unwrap_or_default()
}

/// The mirror of the pane on row `index` (Slint callbacks name a pane by its row).
pub fn mirror_at(index: i32) -> Rc<Mirror> {
    let row = usize::try_from(index).ok();
    ROWS.with(|r| row.and_then(|i| r.borrow().get(i).map(|(_, m)| m.clone()))).unwrap_or_default()
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

/// Makes `pane` reachable once all its parts exist.
pub fn install(pane: Pane) {
    PANES.with(|p| p.borrow_mut().push(pane));
}

/// Runs `f` with the active pane, if one is installed.
pub fn with_active<R>(f: impl FnOnce(&Pane) -> R) -> Option<R> {
    with_picked(&PANES, |panes| panes.get(ACTIVE.with(Cell::get)), f)
}

/// Runs `f` with pane `id`, if it is still open.
pub fn with_id<R>(id: PaneId, f: impl FnOnce(&Pane) -> R) -> Option<R> {
    with_picked(&PANES, |panes| panes.iter().find(|p| p.id == id), f)
}

/// Runs `f` with a clone of what `pick` takes from `list`, taken out of the borrow: `f` may
/// reach the list again (a second borrow would abort the release build).
fn with_picked<T: Clone + 'static, R>(
    list: &'static LocalKey<RefCell<Vec<T>>>,
    pick: impl FnOnce(&[T]) -> Option<&T>,
    f: impl FnOnce(&T) -> R,
) -> Option<R> {
    let item = list.with(|l| pick(&l.borrow()).cloned())?;
    Some(f(&item))
}

#[cfg(test)]
mod tests {
    use super::*;

    thread_local! {
        static LIST: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
    }

    fn by_id(id: u32, f: impl FnOnce(&u32) -> u32) -> Option<u32> {
        with_picked(&LIST, |l| l.iter().find(|&&x| x == id), f)
    }

    fn first(f: impl FnOnce(&u32) -> u32) -> Option<u32> {
        with_picked(&LIST, |l| l.first(), f)
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
}
