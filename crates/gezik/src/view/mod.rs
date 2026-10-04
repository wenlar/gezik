//! The file view: what the active tab shows, the selection, and how both reach Slint.
//! Navigation hands it each loaded listing and takes the selection back as a `ViewState`.

mod listing;
mod model;

pub use listing::Listing;

use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gezik_core::format_size;
use gezik_core::layout::{Geometry, Move, Rect};
use gezik_core::nav::ViewState;
use gezik_core::selection::Selection;
use slint::{ComponentHandle, ModelRc};

use crate::AppWindow;
use model::{ItemsModel, ViewData};

/// How long after showing a listing (or a far jump) its scroll offset is applied again:
/// about three frames. A new model makes the ListView re-place its lines on its next
/// layout, which can pull the offset back or snap it to a line boundary.
pub const SCROLL_RESTORE_DELAY: Duration = Duration::from_millis(50);

/// At most this many selected names are kept per history entry; beyond, only the focus.
const MAX_REMEMBERED: usize = 1000;

type Listener = Rc<dyn Fn()>;

struct Inner {
    window: slint::Weak<AppWindow>,
    data: Rc<RefCell<ViewData>>,
    model: Rc<ItemsModel>,
    /// Bumped on every `show` and `clear`, so a delayed scroll restore of an older
    /// listing is dropped.
    shown: Cell<u64>,
    /// Bumped whenever the view scrolls on purpose (`reveal`), so a delayed scroll restore
    /// after a model reset does not undo it.
    revealed: Cell<u64>,
    /// Shown instead of the item count until the selection changes (e.g. "… no longer exists").
    note: RefCell<Option<String>>,
    on_selection: RefCell<Vec<Listener>>,
}

#[derive(Clone)]
pub struct View(Rc<Inner>);

impl View {
    pub fn new(window: &AppWindow) -> View {
        let data = Rc::new(RefCell::new(ViewData::default()));
        let model = Rc::new(ItemsModel::new(data.clone()));
        window.set_items(ModelRc::from(model.clone()));
        View(Rc::new(Inner {
            window: window.as_weak(),
            data,
            model,
            shown: Cell::new(0),
            revealed: Cell::new(0),
            note: RefCell::new(None),
            on_selection: RefCell::new(Vec::new()),
        }))
    }

    /// Calls `f` whenever the selection or the focus changes (also when a listing is shown).
    #[allow(dead_code, reason = "used by later view tasks (grid, preview)")]
    pub fn on_selection_changed(&self, f: impl Fn() + 'static) {
        self.0.on_selection.borrow_mut().push(Rc::new(f));
    }

    /// Shows `listing` with the selection, focus and scroll `state` remembers. `note`, if
    /// any, replaces the item count in the status bar until the selection changes.
    pub fn show(&self, listing: Listing, state: &ViewState, note: Option<String>) {
        let selection = restore_selection(&listing, state);
        let count = listing.len();
        {
            let mut data = self.0.data.borrow_mut();
            data.listing = listing;
            data.selection = selection;
            data.marquee_base = None;
        }
        self.0.model.notify.reset();
        let shown = self.0.shown.get() + 1;
        self.0.shown.set(shown);
        *self.0.note.borrow_mut() = note;
        let Some(window) = self.0.window.upgrade() else { return };
        self.sync_focus(&window);
        window.set_list_scroll(state.scroll);
        if state.scroll != 0.0 && count > 0 {
            let (view, scroll) = (self.clone(), state.scroll);
            slint::Timer::single_shot(SCROLL_RESTORE_DELAY, move || {
                if view.0.shown.get() == shown
                    && let Some(window) = view.0.window.upgrade()
                {
                    window.set_list_scroll(scroll);
                }
            });
        }
        self.update_status();
        self.notify_listeners();
    }

    /// Empties the view (a tab switch while the new tab loads). The status bar is left to
    /// the caller ("Loading…").
    pub fn clear(&self) {
        {
            let mut data = self.0.data.borrow_mut();
            data.listing = Listing::default();
            data.selection = Selection::new(0);
            data.marquee_base = None;
        }
        self.0.model.notify.reset();
        self.0.shown.set(self.0.shown.get() + 1);
        self.0.note.borrow_mut().take();
        if let Some(window) = self.0.window.upgrade() {
            self.sync_focus(&window);
            window.set_list_scroll(0.0);
        }
        self.notify_listeners();
    }

    /// The selection, focus and scroll, by name, for the history.
    pub fn capture(&self) -> ViewState {
        let data = self.0.data.borrow();
        let name = |i: usize| data.listing.name_at(i).map(str::to_owned);
        let selected = if data.selection.count() > MAX_REMEMBERED {
            Vec::new()
        } else {
            data.selection.iter().filter_map(name).collect()
        };
        let scroll = self.0.window.upgrade().map_or(0.0, |w| w.get_list_scroll());
        ViewState { selected, focus: data.selection.focus().and_then(name), scroll }
    }

    #[allow(dead_code, reason = "used by later view tasks (grid, preview)")]
    pub fn len(&self) -> usize {
        self.0.data.borrow().listing.len()
    }

    pub fn focus(&self) -> Option<usize> {
        self.0.data.borrow().selection.focus()
    }

    pub fn is_selected(&self, index: usize) -> bool {
        self.0.data.borrow().selection.is_selected(index)
    }

    pub fn selection_count(&self) -> usize {
        self.0.data.borrow().selection.count()
    }

    /// Path of entry `index` and whether it is a folder (drives count as folders).
    pub fn entry_path(&self, index: usize) -> Option<(PathBuf, bool)> {
        self.0.data.borrow().listing.path_at(index)
    }

    /// The selected entries' paths, and whether each is a folder, in list order.
    pub fn selected_items(&self) -> Vec<(PathBuf, bool)> {
        let data = self.0.data.borrow();
        data.selection.iter().filter_map(|i| data.listing.path_at(i)).collect()
    }

    pub fn selected_paths(&self) -> Vec<PathBuf> {
        self.selected_items().into_iter().map(|(path, _)| path).collect()
    }

    pub fn find_prefix(&self, typed: &str) -> Option<usize> {
        self.0.data.borrow().listing.find_prefix(typed)
    }

    /// A left press on entry `index`: plain selects only it, Ctrl flips it, Shift selects
    /// the range from the anchor (Ctrl+Shift adds that range).
    pub fn press(&self, index: usize, ctrl: bool, shift: bool) {
        let changes = {
            let mut data = self.0.data.borrow_mut();
            match (ctrl, shift) {
                (_, true) => data.selection.extend_to(index, ctrl),
                (true, false) => data.selection.toggle(index),
                (false, false) => data.selection.select_only(index),
            }
        };
        self.after_selection(&changes);
    }

    /// A right-click on entry `index`: an entry outside the selection becomes the only
    /// selected one; inside it, the selection stays (the menu is for all of it).
    pub fn prepare_menu(&self, index: usize) {
        if !self.is_selected(index) {
            let changes = self.0.data.borrow_mut().selection.select_only(index);
            self.after_selection(&changes);
        }
    }

    pub fn clear_selection(&self) {
        let changes = self.0.data.borrow_mut().selection.clear();
        self.after_selection(&changes);
    }

    pub fn select_all(&self) {
        let changes = self.0.data.borrow_mut().selection.select_all();
        self.after_selection(&changes);
    }

    /// Ctrl+Space.
    pub fn toggle_focus(&self) {
        let changes = self.0.data.borrow_mut().selection.toggle_focus();
        self.after_selection(&changes);
    }

    /// Type-ahead found entry `index`: it becomes the only selected one, in view.
    pub fn jump_to(&self, index: usize) {
        let changes = self.0.data.borrow_mut().selection.select_only(index);
        self.after_selection(&changes);
        self.reveal(index);
    }

    /// An arrow, page or Home/End key. Plain moves the selection, Shift extends it from
    /// the anchor (Ctrl+Shift adds), Ctrl moves only the focus. Returns whether the key
    /// was used.
    pub fn key_move(&self, mv: Move, shift: bool, ctrl: bool, page_rows: usize) -> bool {
        let geometry = self.geometry();
        let (target, changes) = {
            let mut data = self.0.data.borrow_mut();
            let len = data.listing.len();
            let Some(target) = geometry.step(data.selection.focus(), mv, len, page_rows) else { return false };
            let changes = if shift {
                data.selection.extend_to(target, ctrl)
            } else if ctrl {
                data.selection.set_focus(target)
            } else {
                data.selection.select_only(target)
            };
            (target, changes)
        };
        self.after_selection(&changes);
        self.reveal(target);
        true
    }

    /// A rubber-band drag over `rect` (content coordinates): selects what it touches, added
    /// to the selection at the drag's start if `additive` (Ctrl).
    pub fn marquee(&self, rect: Rect, additive: bool) {
        let geometry = self.geometry();
        let changes = {
            let mut data = self.0.data.borrow_mut();
            let ViewData { listing, selection, marquee_base } = &mut *data;
            let base = marquee_base
                .get_or_insert_with(|| if additive { selection.clone() } else { Selection::new(listing.len()) });
            let hits = geometry.items_in_rect(rect, listing.len());
            selection.set_rect(base, &hits)
        };
        self.after_selection(&changes);
    }

    pub fn marquee_done(&self) {
        self.0.data.borrow_mut().marquee_base = None;
    }

    /// Where entries are on screen.
    fn geometry(&self) -> Geometry {
        let row_height = self.0.window.upgrade().map_or(26.0, |w| w.get_item_height());
        Geometry::List { row_height }
    }

    fn after_selection(&self, changes: &[Range<usize>]) {
        if changes.is_empty() {
            return;
        }
        let scroll = self.0.window.upgrade().map(|w| w.get_list_scroll());
        if self.0.model.entries_changed(changes)
            && let Some(scroll) = scroll
        {
            self.keep_scroll_after_reset(scroll);
        }
        self.0.note.borrow_mut().take();
        if let Some(window) = self.0.window.upgrade() {
            self.sync_focus(&window);
        }
        self.update_status();
        self.notify_listeners();
    }

    /// A model reset (Ctrl+A in a large folder) makes the ListView re-place its lines on its
    /// next layout, which can pull or snap the offset: `scroll` is applied again once that
    /// frame is done, unless a listing was shown or an entry revealed meanwhile.
    fn keep_scroll_after_reset(&self, scroll: f32) {
        let (view, shown, revealed) = (self.clone(), self.0.shown.get(), self.0.revealed.get());
        slint::Timer::single_shot(SCROLL_RESTORE_DELAY, move || {
            if view.0.shown.get() == shown
                && view.0.revealed.get() == revealed
                && let Some(window) = view.0.window.upgrade()
                && window.get_list_scroll() != scroll
            {
                window.set_list_scroll(scroll);
            }
        });
    }

    fn notify_listeners(&self) {
        let listeners = self.0.on_selection.borrow().clone();
        for f in &listeners {
            f();
        }
    }

    fn sync_focus(&self, window: &AppWindow) {
        let data = self.0.data.borrow();
        let focus = data.selection.focus();
        window.set_focus_row(focus.and_then(|f| i32::try_from(f).ok()).unwrap_or(-1));
        window.set_focus_selected(focus.is_some_and(|f| data.selection.is_selected(f)));
    }

    fn update_status(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        if let Some(note) = self.0.note.borrow().clone() {
            window.set_status(note.into());
            return;
        }
        let data = self.0.data.borrow();
        let mut size = None;
        for i in data.selection.iter().filter(|i| !data.listing.is_dir(*i)) {
            *size.get_or_insert(0) += data.listing.file_size(i);
        }
        window.set_status(status_text(data.listing.len(), data.selection.count(), size).into());
    }

    /// Scrolls entry `index` fully into view. After a far jump (End, type-ahead) Slint's
    /// ListView snaps the offset to a line boundary on its next layout, which can leave the
    /// target line at the bottom edge only partly visible; the offset is set again once
    /// that frame is done (see [`crate::keys::scroll_was_snapped`]).
    fn reveal(&self, index: usize) {
        let Some(window) = self.0.window.upgrade() else { return };
        self.0.revealed.set(self.0.revealed.get() + 1);
        let index = i32::try_from(index).unwrap_or(i32::MAX);
        window.invoke_ensure_visible(index);
        let (target, line_height) = (window.get_list_scroll(), window.get_item_height());
        let weak = window.as_weak();
        slint::Timer::single_shot(SCROLL_RESTORE_DELAY, move || {
            if let Some(window) = weak.upgrade()
                && window.get_focus_row() == index
                && crate::keys::scroll_was_snapped(window.get_list_scroll(), target, line_height)
            {
                window.set_list_scroll(target);
            }
        });
    }
}

/// At most this many items are opened with their default apps at once (Enter, the menu's
/// "Open"); Ctrl+A and Enter in a large folder would otherwise start thousands of apps.
pub const MAX_OPEN_AT_ONCE: usize = 15;

/// `items` if there are few enough to open at once, else the status bar message.
pub fn limit_open<T>(items: Vec<T>) -> Result<Vec<T>, String> {
    if items.len() > MAX_OPEN_AT_ONCE {
        Err(format!("Select at most {MAX_OPEN_AT_ONCE} items to open"))
    } else {
        Ok(items)
    }
}

/// The selection `state` remembers, by name, in `listing`: names gone from the folder are
/// dropped; the focus falls back to the first selected entry still there.
fn restore_selection(listing: &Listing, state: &ViewState) -> Selection {
    let indices = listing.indices_of(&state.selected);
    let focus = state.focus.as_deref().and_then(|name| listing.index_of(name)).or_else(|| indices.first().copied());
    Selection::from_indices(listing.len(), indices, focus)
}

/// The status bar: `120 items`, or `120 items · 3 selected (1.2 MB)`; the size counts the
/// selected files (`None`: no files selected).
pub fn status_text(count: usize, selected: usize, selected_size: Option<u64>) -> String {
    let items = if count == 1 { "1 item".to_owned() } else { format!("{count} items") };
    match (selected, selected_size) {
        (0, _) => items,
        (n, Some(size)) => format!("{items} · {n} selected ({})", format_size(size)),
        (n, None) => format!("{items} · {n} selected"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use listing::files;

    #[test]
    fn status_counts_items_and_selected_files() {
        assert_eq!(status_text(0, 0, None), "0 items");
        assert_eq!(status_text(1, 0, None), "1 item");
        assert_eq!(status_text(120, 3, Some(1536)), "120 items · 3 selected (1.5 KB)");
        assert_eq!(status_text(5, 2, None), "5 items · 2 selected");
    }

    #[test]
    fn opening_is_capped() {
        assert_eq!(limit_open(Vec::<u8>::new()), Ok(vec![]));
        assert_eq!(limit_open(vec![0; MAX_OPEN_AT_ONCE]).map(|v| v.len()), Ok(MAX_OPEN_AT_ONCE));
        assert_eq!(limit_open(vec![0; MAX_OPEN_AT_ONCE + 1]), Err("Select at most 15 items to open".to_owned()));
    }

    #[test]
    fn restore_selection_keeps_names_that_still_exist() {
        let listing = files("/x", &["a", "c", "d"]);
        let state =
            ViewState { selected: vec!["b".into(), "c".into(), "d".into()], focus: Some("b".into()), scroll: 0.0 };
        let selection = restore_selection(&listing, &state);
        assert_eq!(selection.iter().collect::<Vec<_>>(), [1, 2]);
        assert_eq!(selection.focus(), Some(1), "the focused entry is gone: first selected one");
        let only_focus = ViewState { selected: vec![], focus: Some("d".into()), scroll: 0.0 };
        let selection = restore_selection(&listing, &only_focus);
        assert_eq!((selection.count(), selection.focus()), (0, Some(2)));
    }
}
