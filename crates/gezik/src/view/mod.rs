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
use gezik_core::kind::{fallback_type_name, own_type_name};
use gezik_core::layout::{Geometry, Move, Rect};
use gezik_core::nav::ViewState;
use gezik_core::ops::names::rename_selection;
use gezik_core::pattern::Pattern;
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
use listing::{filtered_listing, name_taken};
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
    /// Files whose names start with a dot are listed (macOS hides them, as Finder does).
    show_hidden: Cell<bool>,
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
    /// The filter bar (`None`: closed).
    filter: RefCell<Option<FilterState>>,
    /// The view was emptied (`clear`) since the last `show`: the next listing takes its filter
    /// from the state it comes with, not from the bar.
    cleared: Cell<bool>,
    /// The selection before the last file operation: its folder and the selected names
    /// (`restore_remembered`).
    remembered: RefCell<Option<(PathBuf, Vec<String>)>>,
}

/// The filter bar's text, the pattern the list shows, and what is wrong with the text.
#[derive(Debug)]
struct FilterState {
    text: String,
    pattern: Pattern,
    error: Option<String>,
}

impl FilterState {
    /// `text` compiled; a text with an error keeps `before`'s pattern (the list does not
    /// jump while a part is half typed).
    fn new(text: &str, before: Option<&FilterState>) -> FilterState {
        match Pattern::compile(text) {
            Ok(pattern) => FilterState { text: text.to_owned(), pattern, error: None },
            Err(error) => FilterState {
                text: text.to_owned(),
                pattern: before.map(|b| b.pattern.clone()).unwrap_or_default(),
                error: Some(error),
            },
        }
    }
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
            show_hidden: Cell::new(!cfg!(target_os = "macos")),
            filter: RefCell::new(None),
            cleared: Cell::new(false),
            remembered: RefCell::new(None),
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

    /// Asks for the system name of every type in `entries` (to sort by type).
    fn request_type_names(&self, entries: &[Entry]) {
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
        let listing = if self.0.show_hidden.get() { listing } else { listing.without_dotfiles() };
        let listing = self.sorted(listing, true);
        // A reload of the folder on screen keeps the bar as it is now (the text may have
        // changed while it loaded); so does a move to it (its breadcrumb or sidebar entry, its
        // path typed again), which counts as a refresh. Otherwise the filter the place had (a
        // tab switch). Only folders have one.
        let cleared = self.0.cleared.replace(false);
        let live = same_folder && !cleared;
        let (full, listing, rows) = match listing {
            Listing::Files(dir, full) if !dir.as_os_str().is_empty() => {
                let filter = if live {
                    self.0.filter.borrow_mut().take()
                } else {
                    state.filter.as_deref().map(|text| FilterState::new(text, None))
                };
                let (shown, rows) = match &filter {
                    Some(filter) => filtered_listing(&dir, &full, &filter.pattern),
                    None => (Listing::Files(dir, full.clone()), None),
                };
                *self.0.filter.borrow_mut() = filter;
                (full, shown, rows)
            }
            // "This PC", or nothing (a folder that cannot be listed).
            other => {
                self.0.filter.borrow_mut().take();
                let full = match &other {
                    Listing::Files(_, full) => full.clone(),
                    Listing::Drives(_) => Rc::default(),
                };
                (full, other, None)
            }
        };
        let selection = restore_selection(&listing, state);
        let count = listing.len();
        {
            let mut data = self.0.data.borrow_mut();
            data.listing = listing;
            data.full = full;
            data.rows = rows;
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
        // A reload of the folder on screen (the watcher, right after an operation) leaves a note
        // standing ("2 items hidden by the filter"): it goes when the selection changes.
        if note.is_some() || !live {
            *self.0.note.borrow_mut() = note;
        }
        let Some(window) = self.0.window.upgrade() else { return };
        self.sync_focus(&window);
        self.sync_filter_bar(&window);
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
    /// In a folder the entries the filter hides count too.
    pub fn has_other_named(&self, name: &str, except: usize) -> bool {
        let data = self.0.data.borrow();
        if let Listing::Files(..) = data.listing {
            return name_taken(&data.full, name, data.listing.name_at(except).unwrap_or_default());
        }
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

    /// The names of every entry in the folder, the ones the filter hides too ("This PC": its
    /// drives).
    pub fn all_names(&self) -> Vec<String> {
        let data = self.0.data.borrow();
        if let Listing::Files(..) = data.listing {
            return data.full.iter().map(|e| e.name.clone()).collect();
        }
        (0..data.listing.len()).filter_map(|i| data.listing.name_at(i).map(str::to_owned)).collect()
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
            data.full = Rc::default();
            data.rows = None;
            data.selection = Selection::new(0);
            data.marquee_base = None;
            data.pending = Default::default();
        }
        self.0.model.notify.reset();
        self.0.shown.set(self.0.shown.get() + 1);
        self.0.note.borrow_mut().take();
        self.0.filter.borrow_mut().take();
        self.0.cleared.set(true);
        if let Some(window) = self.0.window.upgrade() {
            self.sync_focus(&window);
            window.set_list_scroll(0.0);
            self.sync_filter_bar(&window);
        }
        self.notify_listeners();
    }

    /// The selection, focus and scroll, by name, for the history.
    pub fn capture(&self) -> ViewState {
        self.capture_capped(MAX_REMEMBERED)
    }

    fn capture_capped(&self, max: usize) -> ViewState {
        let data = self.0.data.borrow();
        let name = |i: usize| data.listing.name_at(i).map(str::to_owned);
        let selected =
            if data.selection.count() > max { Vec::new() } else { data.selection.iter().filter_map(name).collect() };
        let scroll = self.0.window.upgrade().map_or(0.0, |w| w.get_list_scroll());
        ViewState { selected, focus: data.selection.focus().and_then(name), scroll, filter: self.filter_text() }
    }

    /// How many of `names` (entries of the folder shown) the filter hides.
    pub fn hidden_by_filter(&self, names: &[String]) -> usize {
        hidden_count(self.0.filter.borrow().as_ref().map(|f| &f.pattern), names)
    }

    /// The filter bar's text; `None` while it is closed.
    pub fn filter_text(&self) -> Option<String> {
        self.0.filter.borrow().as_ref().map(|f| f.text.clone())
    }

    /// Filters the folder by `text` as typed in the filter bar (opening it); `None` closes the
    /// filter. A text with an error keeps the list as the last good pattern showed it. Nothing
    /// in "This PC".
    pub fn set_filter(&self, text: Option<&str>) {
        let Some(window) = self.0.window.upgrade() else { return };
        let dir = match &self.0.data.borrow().listing {
            Listing::Files(dir, _) => dir.clone(),
            Listing::Drives(_) => PathBuf::new(),
        };
        if dir.as_os_str().is_empty() {
            // "This PC" and the empty listing: no filter.
            self.0.filter.borrow_mut().take();
            return self.sync_filter_bar(&window);
        }
        let old_pattern = self.0.filter.borrow().as_ref().map(|f| f.pattern.clone()).unwrap_or_default();
        let next = text.map(|text| FilterState::new(text, self.0.filter.borrow().as_ref()));
        let new_pattern = next.as_ref().map(|f| f.pattern.clone()).unwrap_or_default();
        let closing = next.is_none();
        *self.0.filter.borrow_mut() = next;
        // The same entries show (the bar opens empty, a space is added, a part is half typed):
        // the list, its selection and scroll stay.
        if new_pattern != old_pattern {
            // The indices change: a rename in progress cannot follow its entry.
            if self.0.renaming.borrow().is_some() {
                self.end_rename(false);
            }
            let full = self.0.data.borrow().full.clone();
            let (shown, rows) = filtered_listing(&dir, &full, &new_pattern);
            // What was selected and still shows stays selected, carried by position through
            // the full list (no names: Ctrl+A in 100k entries, then typing, stays fast). Closing
            // keeps the focused entry; a new pattern starts at the first entry it shows.
            let selection = {
                let data = self.0.data.borrow();
                if closing {
                    carry(&data.selection, data.rows.as_deref(), full.len(), None)
                } else if data.selection.count() == 0 {
                    Selection::new(shown.len()).focused_at(Some(0))
                } else {
                    carry(&data.selection, data.rows.as_deref(), full.len(), rows.as_deref()).focused_at(Some(0))
                }
            };
            {
                let mut data = self.0.data.borrow_mut();
                data.listing = shown;
                data.rows = rows;
                data.selection = selection;
                data.marquee_base = None;
                data.pending = Default::default();
            }
            self.0.model.notify.reset();
            // A delayed scroll restore of the listing before must not undo this.
            self.0.shown.set(self.0.shown.get() + 1);
            self.0.note.borrow_mut().take();
            self.sync_focus(&window);
            match self.focus() {
                Some(focus) if closing => self.reveal(focus),
                _ => window.set_list_scroll(0.0),
            }
            self.update_status();
            self.notify_listeners();
        }
        self.sync_filter_bar(&window);
    }

    /// The filter bar as the filter is: open or closed, its text (only if it differs, so the
    /// cursor does not jump while typing), the counter and the error.
    fn sync_filter_bar(&self, window: &AppWindow) {
        let (text, count, error) = {
            let filter = self.0.filter.borrow();
            let data = self.0.data.borrow();
            match filter.as_ref() {
                Some(f) => (
                    Some(f.text.clone()),
                    filter_count_text(data.listing.len(), data.full.len()),
                    f.error.clone().unwrap_or_default(),
                ),
                None => (None, String::new(), String::new()),
            }
        };
        // A closing bar takes the keyboard with it: the list gets it.
        if text.is_none() && window.get_filter_focused() {
            window.invoke_focus_list();
        }
        window.set_filter_open(text.is_some());
        let text = text.unwrap_or_default();
        if window.get_filter_text().as_str() != text {
            window.set_filter_text(text.into());
        }
        window.set_filter_count(count.into());
        window.set_filter_error(error.into());
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

    /// The selected files and folders (the focused one if none is selected), in list order,
    /// with their paths: what the listing knows of them, without asking the file system.
    pub fn selected_entries(&self) -> Vec<(PathBuf, Entry)> {
        let data = self.0.data.borrow();
        let Listing::Files(dir, entries) = &data.listing else { return Vec::new() };
        let mut indices: Vec<usize> = data.selection.iter().collect();
        if indices.is_empty() {
            indices.extend(data.selection.focus());
        }
        indices.into_iter().filter_map(|i| entries.get(i)).map(|e| (dir.join(&e.name), e.clone())).collect()
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

    /// Selected entries become unselected and the others selected; the focus stays.
    pub fn invert_selection(&self) {
        let changes = invert_in(&mut self.0.data.borrow_mut().selection);
        self.after_selection(&changes);
    }

    /// How many entries shown (the filter's hidden ones not) have names `pattern` matches.
    pub fn count_matching(&self, pattern: &Pattern) -> usize {
        count_matching_in(&self.0.data.borrow().listing, pattern)
    }

    /// Selects (`on`) or unselects the entries shown whose names `pattern` matches; the others
    /// stay as they are.
    pub fn select_matching(&self, pattern: &Pattern, on: bool) {
        let changes = {
            let mut data = self.0.data.borrow_mut();
            let ViewData { listing, selection, .. } = &mut *data;
            select_matching_in(listing, selection, pattern, on)
        };
        self.after_selection(&changes);
    }

    /// Adds every entry of the focused entry's type (its ending, or folders) to the
    /// selection; nothing without a focus.
    pub fn select_same_type(&self) {
        let changes = {
            let mut data = self.0.data.borrow_mut();
            let ViewData { listing, selection, .. } = &mut *data;
            let Some(focus) = selection.focus() else { return };
            selection.set_where(true, |i| listing.is_same_type(focus, i))
        };
        self.after_selection(&changes);
    }

    /// Keeps the folder shown and its selected names, for `restore_remembered` (a file
    /// operation is about to start). In "This PC", or with nothing selected, the last one
    /// stays.
    pub fn remember_selection(&self) {
        let data = self.0.data.borrow();
        let Some(folder) = data.listing.folder() else { return };
        if data.selection.count() == 0 {
            return;
        }
        let names = data.selection.iter().filter_map(|i| data.listing.name_at(i).map(str::to_owned)).collect();
        *self.0.remembered.borrow_mut() = Some((folder.to_path_buf(), names));
    }

    /// `remember_selection`, for a job on `sources`: only if they are all in the folder shown
    /// (a paste or a drop from elsewhere must not replace another folder's selection).
    pub fn remember_selection_for(&self, sources: &[PathBuf]) {
        let here = sources_in(self.0.data.borrow().listing.folder(), sources);
        if here {
            self.remember_selection();
        }
    }

    /// Selects again what `remember_selection` kept, if its folder is shown: the names still
    /// shown, focused on the first. Elsewhere, or if none of them shows any more (renamed,
    /// deleted, filtered out), the selection stays as it is.
    pub fn restore_remembered(&self) {
        let changes = {
            let remembered = self.0.remembered.borrow();
            let Some((folder, names)) = remembered.as_ref() else { return };
            let mut data = self.0.data.borrow_mut();
            if !data.listing.folder().is_some_and(|f| gezik_core::ops::paths::same_path(f, folder)) {
                return;
            }
            let ViewData { listing, selection, .. } = &mut *data;
            let Some(changes) = restore_in(listing, selection, names) else { return };
            changes
        };
        self.after_selection(&changes);
        if let Some(focus) = self.focus() {
            self.reveal(focus);
        }
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
    /// folder's own view is forgotten, once the settings writer thread has written it into
    /// settings.toml (a failure is said in the status bar, and nothing changes).
    pub fn apply_to_all(&self) {
        let view = self.0.current.get();
        let Some(store) = &self.0.store else { return self.applied_to_all(view) };
        store.write_settings(gezik_config::settings_writer::SettingsChange::ViewDefaults(view), move |result| {
            let _ = slint::invoke_from_event_loop(move || {
                with_current(|this| match result {
                    Ok(()) => this.applied_to_all(view),
                    Err(warning) => this.set_note(warning.to_string()),
                });
            });
        });
    }

    fn applied_to_all(&self, view: ViewSettings) {
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

    /// Shows the files whose names start with a dot if they were hidden, or hides them; the
    /// folder must be listed again to take effect.
    pub fn toggle_hidden(&self) {
        self.0.show_hidden.set(!self.0.show_hidden.get());
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
        let scroll = self.0.window.upgrade().map_or(0.0, |w| w.get_list_scroll());
        let (listing, full, old_rows, selection) = self.take_listing();
        let (listing, full, rows, selection) = match listing {
            Listing::Files(dir, _) => {
                let kept: Vec<usize> = (0..full.len()).filter(|&i| !hidden.contains(full[i].name.as_str())).collect();
                // The selection by position: into the full list, then what the filter shows.
                let in_full = carry(&selection, old_rows.as_deref(), full.len(), Some(&kept));
                let full = Rc::new(kept.iter().map(|&i| full[i].clone()).collect::<Vec<Entry>>());
                let (shown, rows) = self.filtered(&dir, &full);
                let selection = carry(&in_full, None, full.len(), rows.as_deref());
                (shown, full, rows, selection)
            }
            other => (other, full, old_rows, selection),
        };
        {
            let mut data = self.0.data.borrow_mut();
            data.listing = listing;
            data.full = full;
            data.rows = rows;
            data.selection = selection;
        }
        self.0.model.notify.reset();
        if let Some(window) = self.0.window.upgrade() {
            self.sync_focus(&window);
            window.set_list_scroll(scroll);
            self.keep_scroll_after_reset(scroll);
            self.sync_filter_bar(&window);
        }
        self.update_status();
        self.notify_listeners();
    }

    /// What the filter lets through of `full`, the entries of `dir`, and where they are in it.
    fn filtered(&self, dir: &Path, full: &Rc<Vec<Entry>>) -> (Listing, Option<Vec<usize>>) {
        match self.0.filter.borrow().as_ref() {
            Some(filter) => filtered_listing(dir, full, &filter.pattern),
            None => (Listing::Files(dir.to_path_buf(), full.clone()), None),
        }
    }

    /// Takes the listing, the full list, the rows and the selection out of the view, to put
    /// back changed.
    fn take_listing(&self) -> (Listing, Rc<Vec<Entry>>, Option<Vec<usize>>, Selection) {
        let mut data = self.0.data.borrow_mut();
        let listing = std::mem::take(&mut data.listing);
        (listing, std::mem::take(&mut data.full), data.rows.take(), std::mem::take(&mut data.selection))
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
        match listing {
            Listing::Files(dir, entries) if !(by_name && self.sort() == SortSpec::default()) => {
                Listing::Files(dir, self.sort_now(entries).0)
            }
            other => other,
        }
    }

    /// `entries` in the current sort order (copied only if shared), and where each came from
    /// (`sort_entries`).
    fn sort_now(&self, entries: Rc<Vec<Entry>>) -> (Rc<Vec<Entry>>, Vec<usize>) {
        let spec = self.sort();
        if spec.key == SortKey::Type {
            self.request_type_names(&entries);
        }
        let mut entries = Rc::unwrap_or_clone(entries);
        let order = sort_entries(&mut entries, spec, |e| self.type_name_of(e));
        (Rc::new(entries), order)
    }

    /// The Type column's text for sorting: as the column shows it (`model::type_name_for`),
    /// the system's name if known, else `PNG File`.
    fn type_name_of(&self, entry: &Entry) -> String {
        own_type_name(&entry.name, entry.is_dir)
            .or_else(|| self.0.media.known_type_name(&entry.extension().to_lowercase(), entry.is_dir))
            .unwrap_or_else(|| fallback_type_name(&entry.name, entry.is_dir))
    }

    /// Sorts the current listing again, keeping the selection (by position, through the order
    /// the sort gives); then scrolls the focus into view (`reveal`) or stays at the same scroll
    /// position.
    fn resort(&self, reveal: bool) {
        let scroll = self.0.window.upgrade().map_or(0.0, |w| w.get_list_scroll());
        let (listing, full, old_rows, selection) = self.take_listing();
        // The full list is sorted (no longer shared with the shown one, so not copied), then
        // filtered again. The drives are not sorted.
        let (listing, full, rows, selection) = match listing {
            Listing::Files(dir, shown) => {
                drop(shown);
                let full_len = full.len();
                let (full, order) = self.sort_now(full);
                let in_full = carry(&selection, old_rows.as_deref(), full_len, Some(&order));
                let (shown, rows) = self.filtered(&dir, &full);
                let selection = carry(&in_full, None, full.len(), rows.as_deref());
                (shown, full, rows, selection)
            }
            drives => (drives, full, old_rows, selection),
        };
        {
            let mut data = self.0.data.borrow_mut();
            data.listing = listing;
            data.full = full;
            data.rows = rows;
            data.selection = selection;
        }
        self.0.model.notify.reset();
        if let Some(window) = self.0.window.upgrade() {
            self.sync_focus(&window);
            if !reveal {
                window.set_list_scroll(scroll);
                self.keep_scroll_after_reset(scroll);
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

/// Every entry flips; the rows that changed (all of them).
fn invert_in(selection: &mut Selection) -> Vec<Range<usize>> {
    selection.invert()
}

/// How many entries of `listing` have names `pattern` matches. No allocation per name.
fn count_matching_in(listing: &Listing, pattern: &Pattern) -> usize {
    (0..listing.len()).filter(|&i| listing.name_at(i).is_some_and(|n| pattern.matches(n))).count()
}

/// Selects (`on`) or unselects the entries of `listing` whose names `pattern` matches; the
/// rows that changed.
fn select_matching_in(listing: &Listing, selection: &mut Selection, pattern: &Pattern, on: bool) -> Vec<Range<usize>> {
    selection.set_where(on, |i| listing.name_at(i).is_some_and(|n| pattern.matches(n)))
}

/// Only `names` (those `listing` shows) selected, focused on the first; the rows that changed.
/// `None`, the selection untouched, if none of them shows.
fn restore_in(listing: &Listing, selection: &mut Selection, names: &[String]) -> Option<Vec<Range<usize>>> {
    let indices = listing.indices_of(names);
    let first = *indices.first()?;
    Some(selection.replace(Selection::from_indices(listing.len(), indices, Some(first))))
}

/// `selection` of a list showing entries `old_rows` of a full list of `full_len` entries
/// (`None`: all of it), carried to a list showing entries `new_rows` of it: by position,
/// through a bitset of the full list, with no name compared. The focus follows its entry, or
/// goes to the first selected one if it is gone.
fn carry(selection: &Selection, old_rows: Option<&[usize]>, full_len: usize, new_rows: Option<&[usize]>) -> Selection {
    let in_full = match old_rows {
        Some(rows) => selection.spread(rows, full_len),
        None => selection.clone(),
    };
    match new_rows {
        Some(rows) => in_full.carried(rows),
        None => in_full,
    }
}

/// Whether every one of `sources` (at least one) is an entry of `folder`.
fn sources_in(folder: Option<&Path>, sources: &[PathBuf]) -> bool {
    let Some(folder) = folder else { return false };
    !sources.is_empty()
        && sources.iter().all(|s| s.parent().is_some_and(|parent| gezik_core::ops::paths::same_path(parent, folder)))
}

/// How many of `names` `pattern` hides (none without a filter). No allocation per name.
fn hidden_count(pattern: Option<&Pattern>, names: &[String]) -> usize {
    pattern.map_or(0, |pattern| names.iter().filter(|name| !pattern.matches(name)).count())
}

/// The status bar's note for new items (a paste, a drop, an extract) the filter hides.
pub fn hidden_note(hidden: usize) -> Option<String> {
    match hidden {
        0 => None,
        1 => Some("1 item hidden by the filter".to_owned()),
        n => Some(format!("{n} items hidden by the filter")),
    }
}

/// The filter bar's counter: shown of all, `1,234 / 100,000`.
pub fn filter_count_text(shown: usize, total: usize) -> String {
    use crate::preview::with_commas;
    format!("{} / {}", with_commas(shown), with_commas(total))
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
    #[allow(clippy::single_range_in_vec_init, reason = "one changed range of entries")]
    fn select_matching_adds_or_takes_away_only_the_shown_matches() {
        let listing = files("/x", &["sub/", "a.jpg", "b.JPG", "c.png", "d.txt"]);
        let jpg = Pattern::compile("*.jpg").unwrap();
        assert_eq!(count_matching_in(&listing, &jpg), 2);
        assert_eq!(count_matching_in(&listing, &Pattern::default()), 5, "empty: everything shown");
        let mut selection = Selection::from_indices(5, [3], Some(3));
        let rows = select_matching_in(&listing, &mut selection, &jpg, true);
        assert_eq!(rows, [1..3], "only the rows that changed");
        assert_eq!(selection.iter().collect::<Vec<_>>(), [1, 2, 3]);
        assert_eq!(selection.focus(), Some(3), "the focus stays");
        let rows = select_matching_in(&listing, &mut selection, &Pattern::compile("a*").unwrap(), false);
        assert_eq!(rows, [1..2]);
        assert_eq!(selection.iter().collect::<Vec<_>>(), [2, 3]);
        assert!(select_matching_in(&listing, &mut selection, &jpg, true).len() == 1, "b.JPG was on already");
    }

    #[test]
    #[allow(clippy::single_range_in_vec_init, reason = "one changed range of entries")]
    fn invert_selection_flips_every_shown_entry() {
        let mut selection = Selection::from_indices(4, [0, 2], Some(2));
        assert_eq!(invert_in(&mut selection), [0..4]);
        assert_eq!(selection.iter().collect::<Vec<_>>(), [1, 3]);
        assert_eq!(selection.focus(), Some(2));
        assert!(invert_in(&mut Selection::new(0)).is_empty(), "nothing shown");
    }

    #[test]
    fn restoring_pushes_only_what_changed_and_keeps_the_selection_if_nothing_shows() {
        let names: Vec<String> = (0..1000).map(|i| format!("f{i:04}")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let listing = files("/x", &refs);
        let mut selection = Selection::from_indices(1000, [500], Some(500));
        let rows = restore_in(&listing, &mut selection, &["f0010".into(), "f0011".into(), "gone".into()]);
        assert_eq!(rows, Some(vec![10..12, 500..501]), "not the whole listing");
        assert_eq!(selection.iter().collect::<Vec<_>>(), [10, 11]);
        assert_eq!(selection.focus(), Some(10), "on the first");
        let before = selection.clone();
        assert_eq!(restore_in(&listing, &mut selection, &["renamed".into()]), None);
        assert_eq!(selection, before, "none of them shows: unchanged");
    }

    /// `count_matching` runs on every key typed in the pattern box: Task 1's budget for the
    /// matcher (15 ms for 100,000 names), as counting adds nothing per name. Measured 4-10 ms.
    /// Run: `cargo test --release -p gezik count_matching -- --ignored`.
    #[test]
    #[ignore = "timing; run in release with --ignored"]
    fn count_matching_a_hundred_thousand_names_within_budget() {
        let names: Vec<String> = (0..100_000).map(|i| format!("IMG_{i:06} Tatil ş{}.jpg", i % 7)).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let listing = files("/x", &refs);
        for text in ["i", "img_0", "*.jpg;*.png", "tatil;!*ş3.jpg", "zzz"] {
            let pattern = Pattern::compile(text).unwrap();
            let started = std::time::Instant::now();
            let n = count_matching_in(&listing, &pattern);
            let took = started.elapsed();
            assert!(took < Duration::from_millis(15), "{text}: {took:?} ({n} match)");
        }
    }

    #[test]
    fn a_bad_filter_keeps_the_last_good_pattern() {
        let first = FilterState::new("*.jpg", None);
        assert_eq!(first.error, None);
        let bad = FilterState::new("*.jpg;!", Some(&first));
        assert_eq!(bad.error.as_deref(), Some("Type a name after \"!\""));
        assert_eq!(bad.pattern, first.pattern, "the list keeps showing the jpgs");
        assert_eq!(bad.text, "*.jpg;!");
        assert!(FilterState::new("!", None).pattern.is_empty(), "nothing good before: everything shows");
    }

    #[test]
    fn new_items_the_filter_hides_are_counted() {
        let names: Vec<String> = ["a.jpg", "b.txt", "C.JPG", "d.png"].map(String::from).into();
        let jpg = Pattern::compile("*.jpg").unwrap();
        assert_eq!(hidden_count(Some(&jpg), &names), 2);
        assert_eq!(hidden_count(None, &names), 0, "no filter: nothing hidden");
        assert_eq!(hidden_count(Some(&Pattern::default()), &names), 0, "an empty bar hides nothing");
        assert_eq!(hidden_note(0), None);
        assert_eq!(hidden_note(1).as_deref(), Some("1 item hidden by the filter"));
        assert_eq!(hidden_note(3).as_deref(), Some("3 items hidden by the filter"));
    }

    #[test]
    fn only_a_job_on_the_folder_shown_remembers_its_selection() {
        let a = Path::new("/a");
        let in_a = [PathBuf::from("/a/x.txt"), PathBuf::from("/a/sub")];
        assert!(sources_in(Some(a), &in_a), "Ctrl+C, Delete, rename, convert: from here");
        // Ctrl+C in /a, Ctrl+V in /b: the paste's sources are in /a, /b is shown.
        assert!(!sources_in(Some(Path::new("/b")), &in_a), "a paste from elsewhere");
        assert!(!sources_in(Some(a), &[PathBuf::from("/a/x.txt"), PathBuf::from("/c/y.txt")]), "partly elsewhere");
        assert!(!sources_in(Some(a), &[PathBuf::from("/a/sub/deeper.txt")]), "in a subfolder");
        assert!(!sources_in(Some(a), &[]), "new folder: no sources");
        assert!(!sources_in(None, &in_a), "This PC");
    }

    #[test]
    fn the_counter_reads_well() {
        assert_eq!(filter_count_text(12, 340), "12 / 340");
        assert_eq!(filter_count_text(1234, 100_000), "1,234 / 100,000");
    }

    #[test]
    fn the_selection_is_carried_through_the_full_list_by_position() {
        // Full: a.txt b.jpg c.txt d.jpg e.jpg; "*.jpg" shows rows [1, 3, 4]; b and e selected.
        let shown = Selection::from_indices(3, [0, 2], Some(2));
        // "d;e" shows [3, 4]: only e stays selected, and the focus follows it.
        let next = carry(&shown, Some(&[1, 3, 4]), 5, Some(&[3, 4]));
        assert_eq!((next.iter().collect::<Vec<_>>(), next.focus()), (vec![1], Some(1)));
        // Closing shows the full list: b and e, focus on e.
        let all = carry(&shown, Some(&[1, 3, 4]), 5, None);
        assert_eq!((all.iter().collect::<Vec<_>>(), all.focus()), (vec![1, 4], Some(4)));
        // Ctrl+A with no filter, then a filter: what it shows stays selected.
        let mut everything = Selection::new(5);
        everything.select_all();
        assert_eq!(carry(&everything, None, 5, Some(&[1, 3, 4])).count(), 3);
        assert_eq!(carry(&Selection::new(0), None, 0, Some(&[])).focus(), None);
    }

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
        let state = ViewState { selected: names.clone(), focus: Some("f1500".into()), scroll: 0.0, filter: None };
        let selection = restore_selection(&listing, &state);
        assert_eq!((selection.count(), selection.focus()), (2000, Some(1500)));
    }

    #[test]
    fn restore_selection_keeps_names_that_still_exist() {
        let listing = files("/x", &["a", "c", "d"]);
        let state = ViewState {
            selected: vec!["b".into(), "c".into(), "d".into()],
            focus: Some("b".into()),
            scroll: 0.0,
            filter: None,
        };
        let selection = restore_selection(&listing, &state);
        assert_eq!(selection.iter().collect::<Vec<_>>(), [1, 2]);
        assert_eq!(selection.focus(), Some(1), "the focused entry is gone: first selected one");
        let only_focus = ViewState { selected: vec![], focus: Some("d".into()), scroll: 0.0, filter: None };
        let selection = restore_selection(&listing, &only_focus);
        assert_eq!((selection.count(), selection.focus()), (0, Some(2)));
    }
}
