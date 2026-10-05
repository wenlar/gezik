//! The file view: what the active tab shows, the selection, and how both reach Slint.
//! Navigation hands it each loaded listing and takes the selection back as a `ViewState`.

mod listing;
mod model;

pub use listing::Listing;

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use gezik_config::settings::ViewDefaults;
use gezik_config::store::ConfigStore;
use gezik_core::kind::fallback_type_name;
use gezik_core::layout::{Geometry, Move, Rect};
use gezik_core::nav::ViewState;
use gezik_core::ops::names::rename_selection;
use gezik_core::selection::Selection;
use gezik_core::sort::{SortDir, SortKey, SortSpec, sort_entries};
use gezik_core::view::{
    ColumnKey, ColumnState, GridSize, MAX_COLUMN_WIDTH, MIN_COLUMN_WIDTH, ViewMode, ViewSettings, default_columns,
    normalize_columns,
};
use gezik_core::view_memory::ViewMemory;
use gezik_core::{Entry, format_size};
use slint::{ComponentHandle, ModelRc};

use crate::media::{Media, Ready};
use crate::{AppWindow, Theme};
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
    /// The entry being renamed: its index and its name before.
    renaming: RefCell<Option<(usize, String)>>,
    /// The folder the rename belongs to (as remembered in `folder`).
    rename_folder: RefCell<Option<String>>,
    /// Bumped for each new rename, so a recreated field does not select the stem again.
    rename_generation: Cell<i32>,
    /// Called after each `show` (a folder loaded or reloaded).
    on_shown: RefCell<Vec<Listener>>,
    defaults: Cell<ViewDefaults>,
    /// The shown folder's view: its own if it has one, else the defaults.
    current: Cell<ViewSettings>,
    memory: RefCell<ViewMemory>,
    store: Option<ConfigStore>,
    /// The shown folder as remembered in `memory`; `None` for "This PC".
    folder: RefCell<Option<String>>,
    /// A `views.toml` write is scheduled.
    save_pending: Cell<bool>,
    columns: RefCell<Vec<ColumnState>>,
    media: Media,
    /// A re-sort by type is scheduled (type names arrive one by one).
    resort_pending: Cell<bool>,
}

thread_local! {
    /// The view of this (UI) thread, for settings changes.
    static CURRENT: RefCell<Option<View>> = const { RefCell::new(None) };
}

/// Runs `f` with this UI thread's view, if there is one yet.
pub fn with_current(f: impl FnOnce(&View)) {
    if let Some(view) = CURRENT.with(|c| c.borrow().clone()) {
        f(&view);
    }
}

#[derive(Clone)]
pub struct View(Rc<Inner>);

impl View {
    pub fn new(window: &AppWindow, memory: ViewMemory, store: Option<ConfigStore>) -> View {
        let media = Media::new();
        media.install();
        let defaults = ViewDefaults::default();
        let data = Rc::new(RefCell::new(ViewData {
            media: media.clone(),
            icons: defaults.icons,
            thumbnails: defaults.thumbnails,
            ..ViewData::default()
        }));
        let model = Rc::new(ItemsModel::new(data.clone()));
        window.set_items(ModelRc::from(model.clone()));
        let view = View(Rc::new(Inner {
            window: window.as_weak(),
            data,
            model,
            media: media.clone(),
            resort_pending: Cell::new(false),
            shown: Cell::new(0),
            revealed: Cell::new(0),
            note: RefCell::new(None),
            on_selection: RefCell::new(Vec::new()),
            renaming: RefCell::new(None),
            rename_folder: RefCell::new(None),
            rename_generation: Cell::new(0),
            on_shown: RefCell::new(Vec::new()),
            defaults: Cell::new(defaults),
            current: Cell::new(defaults.view),
            memory: RefCell::new(memory),
            store,
            folder: RefCell::new(None),
            save_pending: Cell::new(false),
            columns: RefCell::new(default_columns()),
        }));
        // Weak: the media lives inside the view.
        let weak = Rc::downgrade(&view.0);
        media.on_ready(move |entries, ready| {
            if let Some(inner) = weak.upgrade() {
                View(inner).media_ready(entries, ready);
            }
        });
        CURRENT.with(|c| *c.borrow_mut() = Some(view.clone()));
        view.sync_columns();
        view.apply_layout();
        view
    }

    /// `[view]` settings, at startup and whenever settings.toml changes. A folder without
    /// its own view follows the new defaults at once.
    pub fn set_defaults(&self, defaults: ViewDefaults) {
        if self.0.defaults.replace(defaults) == defaults {
            return;
        }
        {
            let mut data = self.0.data.borrow_mut();
            data.icons = defaults.icons;
            data.thumbnails = defaults.thumbnails;
        }
        let own = self.0.folder.borrow().as_deref().is_some_and(|f| self.0.memory.borrow().contains(f));
        if !own {
            self.switch_to(defaults.view);
        }
        // Icons or thumbnails may have changed even if the view did not.
        self.0.model.notify.reset();
    }

    fn media_ready(&self, entries: &[usize], ready: Ready) {
        let mut rows: Vec<Range<usize>> = entries.iter().map(|&i| i..i + 1).collect();
        rows.sort_by_key(|r| r.start);
        self.0.model.entries_changed(&rows);
        if ready == Ready::TypeName && self.sort().key == SortKey::Type {
            self.resort_soon();
        }
    }

    /// Type names arrive one by one: sorts again once they stop for a moment, keeping the
    /// scroll position (the user did not ask for it, so the view does not jump).
    fn resort_soon(&self) {
        if self.0.resort_pending.replace(true) {
            return;
        }
        let view = self.clone();
        slint::Timer::single_shot(Duration::from_millis(150), move || {
            view.0.resort_pending.set(false);
            view.resort(false);
        });
    }

    /// Asks for the system name of every type in `listing` (to sort by type).
    fn request_type_names(&self, listing: &Listing) {
        let Listing::Files(_, entries) = listing else { return };
        let mut seen = HashSet::new();
        for e in entries.iter() {
            let ext = e.extension().to_lowercase();
            if seen.insert((ext.clone(), e.is_dir)) {
                self.0.media.type_name(&ext, e.is_dir, None);
            }
        }
    }

    /// What the preview shows: several selected items, or the selected entry (the focused
    /// one if it is selected).
    pub fn preview_target(&self) -> crate::preview::Target {
        use crate::preview::Target;
        let data = self.0.data.borrow();
        let count = data.selection.count();
        if count > 1 {
            let mut size = None;
            for i in data.selection.iter().filter(|i| !data.listing.is_dir(*i)) {
                *size.get_or_insert(0) += data.listing.file_size(i);
            }
            return Target::Several { count, size };
        }
        let index = match data.selection.focus() {
            Some(f) if data.selection.is_selected(f) => f,
            _ => match data.selection.iter().next() {
                Some(i) => i,
                None => return Target::Nothing,
            },
        };
        let Some((path, is_dir)) = data.listing.path_at(index) else { return Target::Nothing };
        let entry = match &data.listing {
            Listing::Files(_, entries) => entries.get(index),
            Listing::Drives(_) => None,
        };
        Target::Entry {
            name: data.listing.name_at(index).unwrap_or_default().to_owned(),
            path,
            is_dir,
            type_name: model::type_name_for(&data, index),
            size: (entry.is_some() && !is_dir).then(|| data.listing.file_size(index)),
            modified: entry.and_then(|e| e.modified),
            created: entry.and_then(|e| e.created),
            kind: data.listing.kind(index).index(),
        }
    }

    /// Calls `f` whenever the selection or the focus changes (also when a listing is shown).
    pub fn on_selection_changed(&self, f: impl Fn() + 'static) {
        self.0.on_selection.borrow_mut().push(Rc::new(f));
    }

    /// Shows `listing` with the selection, focus and scroll `state` remembers. `note`, if
    /// any, replaces the item count in the status bar until the selection changes.
    pub fn show(&self, listing: Listing, state: &ViewState, note: Option<String>) {
        let folder = listing.folder().map(|p| p.display().to_string());
        // Only a reload of the same folder keeps a rename.
        if self.0.renaming.borrow().is_some() && *self.0.rename_folder.borrow() != folder {
            self.end_rename(false);
        }
        let settings =
            folder.as_deref().and_then(|f| self.0.memory.borrow_mut().get(f)).unwrap_or(self.0.defaults.get().view);
        // A reload of the same folder (it changed on disk) keeps the icons and thumbnails asked
        // for: a folder that changes all the time would never get its slow thumbnails.
        let same_folder = folder.is_some() && *self.0.folder.borrow() == folder;
        *self.0.folder.borrow_mut() = folder;
        self.0.current.set(settings);
        if !same_folder {
            self.0.media.new_generation();
        }
        self.apply_layout();
        let listing = self.sorted(listing, true);
        let selection = restore_selection(&listing, state);
        let count = listing.len();
        {
            let mut data = self.0.data.borrow_mut();
            data.listing = listing;
            data.selection = selection;
            data.marquee_base = None;
            data.pending = Default::default();
        }
        // A refresh must not break a rename: follow the entry, or give up if it is gone.
        let renamed = self.0.renaming.borrow().as_ref().map(|(_, name)| name.clone());
        if let Some(name) = renamed {
            let index = self.0.data.borrow().listing.index_of(&name);
            match index {
                Some(index) => {
                    *self.0.renaming.borrow_mut() = Some((index, name));
                    if let Some(window) = self.0.window.upgrade() {
                        window.set_renaming_index(i32::try_from(index).unwrap_or(-1));
                    }
                }
                None => self.end_rename(false),
            }
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
        let shown = self.0.on_shown.borrow().clone();
        for f in &shown {
            f();
        }
    }

    /// Calls `f` after each folder load or reload is shown.
    pub fn on_shown(&self, f: impl Fn() + 'static) {
        self.0.on_shown.borrow_mut().push(Rc::new(f));
    }

    /// The only selected entry, if exactly one is.
    pub fn single_selected(&self) -> Option<usize> {
        let data = self.0.data.borrow();
        (data.selection.count() == 1).then(|| data.selection.iter().next()).flatten()
    }

    /// Whether an entry other than `except` is called `name` (ignoring case where the file
    /// system does).
    pub fn has_other_named(&self, name: &str, except: usize) -> bool {
        let data = self.0.data.borrow();
        (0..data.listing.len()).filter(|&i| i != except).any(|i| {
            data.listing.name_at(i).is_some_and(|other| {
                if cfg!(any(windows, target_os = "macos")) {
                    other.to_lowercase() == name.to_lowercase()
                } else {
                    other == name
                }
            })
        })
    }

    /// The names of every entry shown (the folder's listing).
    pub fn all_names(&self) -> Vec<String> {
        (0..self.len())
            .filter_map(|i| self.entry_path(i))
            .filter_map(|(path, _)| path.file_name().map(|n| n.to_string_lossy().into_owned()))
            .collect()
    }

    /// Turns entry `index`'s name into a text field (files and folders only, not drives).
    pub fn begin_rename(&self, index: usize) -> bool {
        let (name, is_dir) = {
            let data = self.0.data.borrow();
            if !matches!(data.listing, Listing::Files(..)) {
                return false;
            }
            let Some(name) = data.listing.name_at(index) else { return false };
            (name.to_owned(), data.listing.is_dir(index))
        };
        let changes = self.0.data.borrow_mut().selection.select_only(index);
        self.after_selection(&changes);
        self.reveal(index);
        let (_, end) = rename_selection(&name, is_dir);
        let Some(window) = self.0.window.upgrade() else { return false };
        window.set_rename_text(name.clone().into());
        window.set_rename_select(i32::try_from(end).unwrap_or(0));
        window.set_rename_error("".into());
        let generation = self.0.rename_generation.get().wrapping_add(1);
        self.0.rename_generation.set(generation);
        window.set_rename_generation(generation);
        *self.0.rename_folder.borrow_mut() = self.0.folder.borrow().clone();
        *self.0.renaming.borrow_mut() = Some((index, name));
        window.set_renaming_index(i32::try_from(index).unwrap_or(-1));
        true
    }

    pub fn begin_rename_by_name(&self, name: &str) -> bool {
        let index = self.0.data.borrow().listing.index_of(name);
        index.is_some_and(|index| self.begin_rename(index))
    }

    /// The entry being renamed (index, name before).
    pub fn renaming(&self) -> Option<(usize, String)> {
        self.0.renaming.borrow().clone()
    }

    /// Ends renaming. `refocus`: the field had the keyboard (Enter, Esc), so the list gets
    /// it back; not after a blur, where another control took the focus on purpose.
    pub fn end_rename(&self, refocus: bool) {
        self.finish_rename(refocus, true);
    }

    /// Ends renaming because another entry's rename starts at once: the list gets no focus in
    /// between (the new field takes it).
    pub fn end_rename_for_next(&self) {
        self.finish_rename(false, false);
    }

    /// The current rename's number (a field only answers for its own).
    pub fn rename_generation(&self) -> i32 {
        self.0.rename_generation.get()
    }

    fn finish_rename(&self, refocus: bool, if_focused: bool) {
        if self.0.renaming.borrow_mut().take().is_none() {
            return;
        }
        self.0.rename_folder.borrow_mut().take();
        if let Some(window) = self.0.window.upgrade() {
            // Whether the field held the keyboard last, even if its row scrolled out of view
            // (the field may still be alive and focused, or destroyed with nothing focused):
            // either way the list must get the keyboard back. Cleared so it cannot go stale
            // into the next rename.
            let had_focus =
                window.get_rename_field_focused() && !window.get_path_editing() && !window.get_dialog_open();
            window.set_rename_field_focused(false);
            window.set_renaming_index(-1);
            window.set_rename_error("".into());
            if refocus || (if_focused && had_focus) {
                window.invoke_focus_list();
            }
        }
    }

    /// Empties the view (a tab switch while the new tab loads). The status bar is left to
    /// the caller ("Loading…").
    pub fn clear(&self) {
        self.end_rename(false);
        self.0.media.new_generation();
        {
            let mut data = self.0.data.borrow_mut();
            data.listing = Listing::default();
            data.selection = Selection::new(0);
            data.marquee_base = None;
            data.pending = Default::default();
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
        self.capture_capped(MAX_REMEMBERED)
    }

    /// Like `capture`, but remembers every selected name (for a resort in place).
    fn capture_all(&self) -> ViewState {
        self.capture_capped(usize::MAX)
    }

    fn capture_capped(&self, max: usize) -> ViewState {
        let data = self.0.data.borrow();
        let name = |i: usize| data.listing.name_at(i).map(str::to_owned);
        let selected =
            if data.selection.count() > max { Vec::new() } else { data.selection.iter().filter_map(name).collect() };
        let scroll = self.0.window.upgrade().map_or(0.0, |w| w.get_list_scroll());
        ViewState { selected, focus: data.selection.focus().and_then(name), scroll }
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

    /// Entry `index` as the list draws it (its icon, for the dragged items).
    pub fn file_row(&self, index: usize) -> Option<crate::FileRow> {
        let data = self.0.data.borrow();
        (index < data.listing.len()).then(|| model::file_row(&data, index))
    }

    /// How many entries the view shows.
    pub fn len(&self) -> usize {
        self.0.data.borrow().listing.len()
    }

    /// Where the entries are: the list or the grid, as laid out now.
    pub fn layout_geometry(&self) -> Geometry {
        self.geometry()
    }

    pub fn selected_paths(&self) -> Vec<PathBuf> {
        self.selected_items().into_iter().map(|(path, _)| path).collect()
    }

    pub fn find_prefix(&self, typed: &str) -> Option<usize> {
        self.0.data.borrow().listing.find_prefix(typed)
    }

    /// A left press on entry `index`: plain selects only it, Ctrl flips it, Shift selects
    /// the range from the anchor (Ctrl+Shift adds that range). On an entry already selected
    /// (no Shift) that waits for the release, so the selection can be dragged.
    pub fn press(&self, index: usize, ctrl: bool, shift: bool) {
        let changes = {
            let mut data = self.0.data.borrow_mut();
            let ViewData { selection, pending, .. } = &mut *data;
            pending.press(selection, index, ctrl, shift)
        };
        self.after_selection(&changes);
    }

    /// The left button came up over entry `index` after a press; `dragged`: it became a drag.
    pub fn release(&self, index: usize, dragged: bool) {
        let changes = {
            let mut data = self.0.data.borrow_mut();
            let ViewData { selection, pending, .. } = &mut *data;
            pending.release(selection, index, dragged)
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
            let ViewData { listing, selection, marquee_base, .. } = &mut *data;
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

    /// A rubber-band drag is going on (a reload now would break it).
    pub fn marquee_active(&self) -> bool {
        self.0.data.borrow().marquee_base.is_some()
    }

    pub fn sort(&self) -> SortSpec {
        self.0.current.get().sort
    }

    /// Sorts by `spec`, keeping the selection, focus and its visibility; the folder
    /// remembers it.
    pub fn set_sort(&self, spec: SortSpec) {
        self.change_view(|v| v.sort = spec);
    }

    pub fn view_settings(&self) -> ViewSettings {
        self.0.current.get()
    }

    pub fn set_mode(&self, mode: ViewMode) {
        self.change_view(|v| v.mode = mode);
    }

    pub fn set_grid_size(&self, size: GridSize) {
        self.change_view(|v| v.grid_size = size);
    }

    /// Ctrl+wheel: the next grid size up or down; nothing in the list.
    pub fn zoom(&self, bigger: bool) {
        if self.0.current.get().mode == ViewMode::Grid {
            self.change_view(|v| v.grid_size = if bigger { v.grid_size.bigger() } else { v.grid_size.smaller() });
        }
    }

    /// "Apply to all folders": this folder's view becomes the `[view]` default and every
    /// folder's own view is forgotten.
    pub fn apply_to_all(&self) {
        let view = self.0.current.get();
        if let Some(store) = &self.0.store
            && let Err(warning) = store.save_view_defaults(&view)
        {
            return self.set_note(warning.to_string());
        }
        let mut defaults = self.0.defaults.get();
        defaults.view = view;
        self.0.defaults.set(defaults);
        self.0.memory.borrow_mut().clear();
        self.save_memory_soon();
    }

    /// "Reset this folder": forgets its own view; the defaults apply.
    pub fn reset_folder(&self) {
        let Some(folder) = self.0.folder.borrow().clone() else { return };
        let removed = self.0.memory.borrow_mut().remove(&folder);
        if removed {
            self.save_memory_soon();
        }
        self.switch_to(self.0.defaults.get().view);
    }

    /// The grid's width now fits `columns` cells per line.
    pub fn grid_columns_changed(&self, columns: usize) {
        if self.0.current.get().mode == ViewMode::Grid && columns.max(1) != self.0.model.per_row() {
            self.0.model.set_per_row(columns);
            self.0.model.notify.reset();
            if let Some(focus) = self.focus() {
                self.reveal(focus);
            }
        }
    }

    /// "This PC" is shown: its rows are drives, which no file operation may touch.
    pub fn shows_drives(&self) -> bool {
        matches!(self.0.data.borrow().listing, Listing::Drives(_))
    }

    /// Whether `path` is a folder (or drive) row of the listing shown; found by looking, not
    /// by asking the disk.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn is_folder_row(&self, path: &Path) -> bool {
        match &self.0.data.borrow().listing {
            Listing::Files(dir, entries) => {
                path.parent().is_some_and(|parent| gezik_core::ops::paths::same_path(parent, dir))
                    && entries.iter().any(|e| e.is_dir && path.file_name().is_some_and(|n| n == e.name.as_str()))
            }
            Listing::Drives(drives) => drives.iter().any(|d| d.path == path),
        }
    }

    /// The folder shown; `None` for "This PC".
    pub fn folder(&self) -> Option<PathBuf> {
        self.0.data.borrow().listing.folder().map(Path::to_path_buf)
    }

    /// The names in this folder on the clipboard as cut (they look faded).
    pub fn set_cut_names(&self, names: HashSet<String>) {
        if self.0.data.borrow().cut == names {
            return;
        }
        self.0.data.borrow_mut().cut = names;
        self.0.model.notify.reset();
    }

    /// Takes `names` out of the listing at once (trashed or deleted: the reload after the job
    /// brings back anything that stayed).
    pub fn hide_names(&self, names: &[String]) {
        let hidden: HashSet<&str> = names.iter().map(String::as_str).collect();
        let state = self.capture_all();
        let listing = std::mem::take(&mut self.0.data.borrow_mut().listing);
        let listing = match listing {
            Listing::Files(dir, entries) => {
                let kept: Vec<Entry> = entries.iter().filter(|e| !hidden.contains(e.name.as_str())).cloned().collect();
                Listing::Files(dir, Rc::new(kept))
            }
            other => other,
        };
        let selection = restore_selection(&listing, &state);
        {
            let mut data = self.0.data.borrow_mut();
            data.listing = listing;
            data.selection = selection;
        }
        self.0.model.notify.reset();
        if let Some(window) = self.0.window.upgrade() {
            self.sync_focus(&window);
            window.set_list_scroll(state.scroll);
            self.keep_scroll_after_reset(state.scroll);
        }
        self.update_status();
        self.notify_listeners();
    }

    /// Shows `text` in the status bar until the selection changes.
    pub fn note(&self, text: String) {
        self.set_note(text);
    }

    /// Writes `views.toml` now if a change is waiting (on close).
    pub fn flush_memory(&self) {
        if self.0.save_pending.get() {
            self.save_memory_now();
        }
    }

    /// A View menu or header change in this folder: applied, and remembered for it.
    fn change_view(&self, change: impl FnOnce(&mut ViewSettings)) {
        let mut view = self.0.current.get();
        change(&mut view);
        if view == self.0.current.get() {
            return;
        }
        let folder = self.0.folder.borrow().clone();
        if let Some(folder) = folder {
            self.0.memory.borrow_mut().set(&folder, view);
            self.save_memory_soon();
        }
        self.switch_to(view);
    }

    /// Shows the current listing with `view`.
    fn switch_to(&self, view: ViewSettings) {
        let old = self.0.current.replace(view);
        if view.mode != old.mode || view.grid_size != old.grid_size {
            self.0.media.new_generation();
            self.apply_layout();
            self.0.model.notify.reset();
            if let Some(focus) = self.focus() {
                self.reveal(focus);
            }
        }
        if view.sort != old.sort {
            self.sync_header();
            self.resort(true);
        }
    }

    /// Mode, picture size and entries per line, from the current view.
    fn apply_layout(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        let view = self.0.current.get();
        let grid = view.mode == ViewMode::Grid;
        window.set_view_mode(if grid { 1 } else { 0 });
        window.set_grid_size(view.grid_size.px() as f32);
        let logical = if grid { view.grid_size.px() as f32 } else { window.global::<Theme>().get_icon_size() };
        {
            let mut data = self.0.data.borrow_mut();
            data.mode = view.mode;
            data.icon_px = (logical * window.window().scale_factor()).round().max(1.0) as u32;
        }
        let per_row = if grid { usize::try_from(window.get_grid_columns()).unwrap_or(1) } else { 1 };
        self.0.model.set_per_row(per_row);
        self.sync_header();
    }

    /// Writes `views.toml` a moment after the last change, so a burst of changes is one write.
    fn save_memory_soon(&self) {
        if self.0.store.is_none() || self.0.save_pending.replace(true) {
            return;
        }
        let view = self.clone();
        slint::Timer::single_shot(Duration::from_secs(1), move || view.flush_memory());
    }

    fn save_memory_now(&self) {
        self.0.save_pending.set(false);
        if let Some(store) = &self.0.store
            && let Err(err) = store.save_views(&self.0.memory.borrow())
        {
            eprintln!("gezik: cannot save views.toml: {err}");
        }
    }

    /// Shows `text` in the status bar until the selection changes.
    fn set_note(&self, text: String) {
        *self.0.note.borrow_mut() = Some(text);
        self.update_status();
    }

    /// A click on column header `column` (0 Name, 1-4 `ColumnKey::index`): sorts by it,
    /// ascending; again flips the direction.
    pub fn header_clicked(&self, column: i32) {
        let key = match column {
            0 => SortKey::Name,
            i => match ColumnKey::ALL.into_iter().find(|k| k.index() == i) {
                Some(k) => k.sort_key(),
                None => return,
            },
        };
        let current = self.sort();
        let dir = if current.key == key { current.dir.flipped() } else { SortDir::Asc };
        self.set_sort(SortSpec { key, dir });
    }

    pub fn columns(&self) -> Vec<ColumnState> {
        self.0.columns.borrow().clone()
    }

    pub fn set_columns(&self, columns: Vec<ColumnState>) {
        *self.0.columns.borrow_mut() = normalize_columns(&columns);
        self.sync_columns();
    }

    /// A column edge was dragged: takes the widths from the window.
    pub fn columns_resized(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        for column in self.0.columns.borrow_mut().iter_mut().filter(|c| c.visible) {
            let width = match column.key {
                ColumnKey::Modified => window.get_col_modified(),
                ColumnKey::Created => window.get_col_created(),
                ColumnKey::Type => window.get_col_type(),
                ColumnKey::Size => window.get_col_size(),
            };
            column.width = (width.round().max(0.0) as u32).clamp(MIN_COLUMN_WIDTH, MAX_COLUMN_WIDTH);
        }
        self.sync_columns();
    }

    pub fn toggle_column(&self, key: ColumnKey) {
        if let Some(column) = self.0.columns.borrow_mut().iter_mut().find(|c| c.key == key) {
            column.visible = !column.visible;
        }
        self.sync_columns();
    }

    pub fn reset_columns(&self) {
        *self.0.columns.borrow_mut() = default_columns();
        self.sync_columns();
    }

    fn sync_columns(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        let columns = self.0.columns.borrow();
        let width = |key: ColumnKey| columns.iter().find(|c| c.key == key && c.visible).map_or(0.0, |c| c.width as f32);
        window.set_col_modified(width(ColumnKey::Modified));
        window.set_col_created(width(ColumnKey::Created));
        window.set_col_type(width(ColumnKey::Type));
        window.set_col_size(width(ColumnKey::Size));
    }

    fn sync_header(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        let spec = self.0.current.get().sort;
        let column = match spec.key {
            SortKey::Name => 0,
            SortKey::Modified => ColumnKey::Modified.index(),
            SortKey::Created => ColumnKey::Created.index(),
            SortKey::Type => ColumnKey::Type.index(),
            SortKey::Size => ColumnKey::Size.index(),
        };
        window.set_sort_column(column);
        window.set_sort_desc(spec.dir == SortDir::Desc);
    }

    /// `listing` in the current sort order. A fresh folder load is already sorted by name
    /// (`by_name`), so the default order costs nothing.
    fn sorted(&self, listing: Listing, by_name: bool) -> Listing {
        let spec = self.0.current.get().sort;
        if spec.key == SortKey::Type {
            self.request_type_names(&listing);
        }
        match listing {
            Listing::Files(dir, entries) if !(by_name && spec == SortSpec::default()) => {
                let mut entries = Rc::unwrap_or_clone(entries);
                sort_entries(&mut entries, spec, |e| self.type_name_of(e));
                Listing::Files(dir, Rc::new(entries))
            }
            other => other,
        }
    }

    /// The Type column's text for sorting: the system's name if known, else `PNG File`.
    fn type_name_of(&self, entry: &Entry) -> String {
        self.0
            .media
            .known_type_name(&entry.extension().to_lowercase(), entry.is_dir)
            .unwrap_or_else(|| fallback_type_name(&entry.name, entry.is_dir))
    }

    /// Sorts the current listing again, keeping the selection by name; then scrolls the
    /// focus into view (`reveal`) or stays at the same scroll position.
    fn resort(&self, reveal: bool) {
        let state = self.capture_all();
        let listing = std::mem::take(&mut self.0.data.borrow_mut().listing);
        let listing = self.sorted(listing, false);
        let selection = restore_selection(&listing, &state);
        {
            let mut data = self.0.data.borrow_mut();
            data.listing = listing;
            data.selection = selection;
        }
        self.0.model.notify.reset();
        if let Some(window) = self.0.window.upgrade() {
            self.sync_focus(&window);
            if !reveal {
                window.set_list_scroll(state.scroll);
                self.keep_scroll_after_reset(state.scroll);
            }
        }
        if reveal && let Some(focus) = self.focus() {
            self.reveal(focus);
        }
        self.notify_listeners();
    }

    /// Where entries are on screen.
    fn geometry(&self) -> Geometry {
        let Some(window) = self.0.window.upgrade() else { return Geometry::List { row_height: 26.0 } };
        if self.0.current.get().mode == ViewMode::Grid {
            Geometry::Grid {
                cell_width: window.get_cell_width(),
                cell_height: window.get_cell_height(),
                columns: self.0.model.per_row(),
            }
        } else {
            Geometry::List { row_height: window.get_item_height() }
        }
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
    fn restore_selection_keeps_more_than_the_history_cap() {
        let names: Vec<String> = (0..2000).map(|i| format!("f{i:04}")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let listing = files("/x", &refs);
        let state = ViewState { selected: names.clone(), focus: Some("f1500".into()), scroll: 0.0 };
        let selection = restore_selection(&listing, &state);
        assert_eq!((selection.count(), selection.focus()), (2000, Some(1500)));
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
