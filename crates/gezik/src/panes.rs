//! The panes of the window (spec 10 §3.1): a pane's navigator, view, filter, search, address
//! bar and folder sizes, together. One pane today; window callbacks and other modules reach the
//! active one, a pane's own background results and timers reach theirs by id (a closed pane's
//! are dropped).

use std::cell::{Cell, RefCell};
use std::thread::LocalKey;

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

thread_local! {
    /// The panes of this (UI) thread.
    static PANES: RefCell<Vec<Pane>> = const { RefCell::new(Vec::new()) };
    /// The index in `PANES` of the pane the keyboard and the window's bars act on.
    static ACTIVE: Cell<usize> = const { Cell::new(0) };
    static NEXT_ID: Cell<u32> = const { Cell::new(0) };
}

/// A new pane's id, for its parts to carry before the pane is installed.
pub fn next_id() -> PaneId {
    PaneId(NEXT_ID.with(|n| n.replace(n.get() + 1)))
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
