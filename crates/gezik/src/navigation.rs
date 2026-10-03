//! Connects the navigation model (tabs + history) to background folder loading and Slint.
//! Only the active tab's listing is kept in memory.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use gezik_core::nav::{Closed, Location, Tabs, ViewState, crumbs, nearest_existing};
use gezik_core::{Entry, format_size, list_dir};
use gezik_platform::Drive;
use slint::{ComponentHandle, Model, ModelNotify, ModelRc, ModelTracker, VecModel};

use crate::places::{Places, PlacesPart};
use crate::{AppWindow, CrumbItem, FileRow, TabItem};

/// Address bar parts shown before older ones collapse into "…".
const MAX_CRUMBS: usize = 4;

/// How long after showing a listing its saved scroll offset is applied again (see
/// `show_listing`): about three frames.
const SCROLL_RESTORE_DELAY: std::time::Duration = std::time::Duration::from_millis(50);

/// Lists files to the UI without a second copy: it shares the navigator's entries, and a
/// `FileRow` is built only for the rows on screen.
pub struct EntryModel {
    pub entries: Rc<Vec<Entry>>,
    notify: ModelNotify,
}

impl Model for EntryModel {
    type Data = FileRow;

    fn row_count(&self) -> usize {
        self.entries.len()
    }

    fn row_data(&self, row: usize) -> Option<FileRow> {
        self.entries.get(row).map(|e| FileRow {
            name: e.name.as_str().into(),
            is_dir: e.is_dir,
            size: if e.is_dir { "".into() } else { format_size(e.size).into() },
        })
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// What a background load produces; `Send`, so it can cross back to the UI thread.
#[derive(Debug)]
enum LoadResult {
    Files(PathBuf, Vec<Entry>),
    Drives(Vec<Drive>),
    /// The shown folder no longer exists; `fallback` is its nearest existing ancestor
    /// (looked up on the loading thread: it may wait on a dead network share).
    Gone {
        fallback: Location,
    },
    Failed(std::io::Error),
}

/// Lists `location`. Runs on a background thread.
fn list(location: &Location, mode: Mode) -> LoadResult {
    match location {
        Location::Drives => LoadResult::Drives(gezik_platform::drives()),
        Location::Path(path) => match list_dir(path) {
            Ok(entries) => LoadResult::Files(path.clone(), entries),
            Err(err) if mode == Mode::Show && err.kind() == std::io::ErrorKind::NotFound => {
                LoadResult::Gone { fallback: nearest_existing(location, |p| p.is_dir()) }
            }
            Err(err) => LoadResult::Failed(err),
        },
    }
}

/// The listing to show after a load of `location` failed. A failed `Show` leaves the tab at
/// a location whose contents are unknown, so nothing listed for another folder (or tab) may
/// stay on screen: it shows an empty listing for `location`. A failed move (navigate, back,
/// forward) changes nothing, so the current listing stays.
fn listing_after_failure(mode: Mode, location: &Location) -> Option<Listing> {
    (mode == Mode::Show).then(|| match location {
        Location::Path(path) => Listing::Files(path.clone(), Rc::default()),
        Location::Drives => Listing::Drives(Vec::new()),
    })
}

/// What the active tab currently shows.
enum Listing {
    Files(PathBuf, Rc<Vec<Entry>>),
    Drives(Vec<Drive>),
}

impl Listing {
    fn name_at(&self, index: usize) -> Option<String> {
        match self {
            Listing::Files(_, entries) => entries.get(index).map(|e| e.name.clone()),
            Listing::Drives(drives) => drives.get(index).map(|d| d.label.clone()),
        }
    }

    fn index_of(&self, name: &str) -> Option<usize> {
        match self {
            Listing::Files(_, entries) => entries.iter().position(|e| e.name == name),
            Listing::Drives(drives) => drives.iter().position(|d| d.label == name),
        }
    }
}

/// How a successful load updates the history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// A new location: push it.
    Navigate,
    Back,
    Forward,
    /// The active tab's current location (reload, tab switch): no history change.
    Show,
}

/// Called when the active location is shown.
type Listener = Rc<dyn Fn(&Location)>;

struct Inner {
    window: slint::Weak<AppWindow>,
    tabs: Tabs,
    listing: Listing,
    /// The listing was cleared for a tab switch and the active tab's own listing is not
    /// shown yet, so what is on screen says nothing about that tab's selection and scroll.
    cleared: bool,
    places: Places,
    /// The tab bar's model, updated in place (see [`sync_model`]).
    tab_model: Rc<VecModel<TabItem>>,
    start: Location,
    /// Bumped on every load so that results of an overtaken load are dropped.
    generation: Arc<AtomicU64>,
    on_changed: Vec<Listener>,
}

thread_local! {
    /// The navigator of this (UI) thread, so background results can reach it.
    static CURRENT: RefCell<Option<Navigator>> = const { RefCell::new(None) };
}

/// Runs `f` with this UI thread's navigator, if one is installed.
pub fn with_current(f: impl FnOnce(&Navigator)) {
    if let Some(nav) = CURRENT.with(|c| c.borrow().clone()) {
        f(&nav);
    }
}

#[derive(Clone)]
pub struct Navigator(Rc<RefCell<Inner>>);

impl Navigator {
    /// The first tab opens at `first` with `select` selected; new tabs open at `start`.
    /// Does not load anything: call [`install`](Self::install) next.
    pub fn new(window: &AppWindow, first: Location, select: Option<String>, start: Location) -> Navigator {
        let mut tabs = Tabs::new(first);
        tabs.active_mut().set_view(ViewState { selected: select, scroll: 0.0 });
        let tab_model = Rc::new(VecModel::default());
        window.set_tabs(ModelRc::from(tab_model.clone()));
        Navigator(Rc::new(RefCell::new(Inner {
            window: window.as_weak(),
            tabs,
            listing: Listing::Files(PathBuf::new(), Rc::default()),
            cleared: false,
            places: Places::default(),
            tab_model,
            start,
            generation: Arc::default(),
            on_changed: Vec::new(),
        })))
    }

    /// Makes this navigator reachable from background-load callbacks and shows the first
    /// tab. Call once, right after `new`.
    pub fn install(&self) {
        CURRENT.with(|c| *c.borrow_mut() = Some(self.clone()));
        self.load(self.active_location(), Mode::Show, None);
    }

    pub fn active_location(&self) -> Location {
        self.0.borrow().tabs.active().location().clone()
    }

    /// Where new tabs open (`start-folder`).
    pub fn start(&self) -> Location {
        self.0.borrow().start.clone()
    }

    pub fn set_start(&self, start: Location) {
        self.0.borrow_mut().start = start;
    }

    pub fn places(&self) -> Places {
        self.0.borrow().places.clone()
    }

    /// Takes in known folders or drives (whichever `part` carries) and retitles.
    pub fn set_places(&self, part: PlacesPart) {
        self.0.borrow_mut().places.apply(part);
        self.update_chrome();
    }

    /// Calls `f` whenever the active location is shown (changed, reloaded or tab switched).
    pub fn on_changed(&self, f: impl Fn(&Location) + 'static) {
        self.0.borrow_mut().on_changed.push(Rc::new(f));
    }

    /// Runs `f` on the tab set. Call [`after_tabs_changed`](Self::after_tabs_changed) afterwards
    /// if the active tab may have changed.
    /// Drops any pending load, so it cannot apply to a different active tab.
    /// `f` runs while the navigator is borrowed: it must touch only the tabs.
    pub fn with_tabs<R>(&self, f: impl FnOnce(&mut Tabs) -> R) -> R {
        self.save_view();
        self.0.borrow_mut().generation.fetch_add(1, Ordering::SeqCst);
        self.keep_active_tab(f)
    }

    /// Runs `f` on the tab set when `f` keeps the same tab active (possibly at another
    /// index), so a pending load of that tab still applies. Same borrow rule as `with_tabs`.
    fn keep_active_tab<R>(&self, f: impl FnOnce(&mut Tabs) -> R) -> R {
        f(&mut self.0.borrow_mut().tabs)
    }

    /// Shows the (possibly new) active tab. Until its listing is loaded the file list is
    /// empty, so no other tab's files appear under its address.
    pub fn after_tabs_changed(&self) {
        {
            let mut inner = self.0.borrow_mut();
            inner.listing = Listing::Files(PathBuf::new(), Rc::default());
            inner.cleared = true;
            if let Some(window) = inner.window.upgrade() {
                window.set_rows(ModelRc::default());
                window.set_selected(-1);
                window.set_list_scroll(0.0);
            }
        }
        self.update_chrome();
        self.load(self.active_location(), Mode::Show, None);
    }

    /// Opens a tab at `location` right after the active one; `activate` switches to it.
    pub fn open_tab(&self, location: Location, activate: bool) {
        if activate {
            self.with_tabs(|tabs| tabs.open(location, true));
            self.after_tabs_changed();
        } else {
            self.keep_active_tab(|tabs| tabs.open(location, false));
            self.update_chrome();
        }
    }

    pub fn activate_tab(&self, index: usize) {
        // Checked first: `with_tabs` would drop a pending load of the active tab.
        let changes = {
            let tabs = &self.0.borrow().tabs;
            index < tabs.len() && index != tabs.active_index()
        };
        if changes {
            self.with_tabs(|tabs| tabs.activate(index));
            self.after_tabs_changed();
        }
    }

    pub fn next_tab(&self) {
        if self.0.borrow().tabs.len() > 1 {
            self.with_tabs(Tabs::next);
            self.after_tabs_changed();
        }
    }

    pub fn prev_tab(&self) {
        if self.0.borrow().tabs.len() > 1 {
            self.with_tabs(Tabs::prev);
            self.after_tabs_changed();
        }
    }

    /// Closes tab `index`. Closing the last tab closes the window (state is saved as for
    /// any close request).
    pub fn close_tab(&self, index: usize) {
        if index != self.0.borrow().tabs.active_index() {
            self.keep_active_tab(|tabs| tabs.close(index));
            return self.update_chrome();
        }
        match self.with_tabs(|tabs| tabs.close(index)) {
            Closed::LastTab => {
                let window = self.0.borrow().window.upgrade();
                if let Some(window) = window
                    && let Err(err) =
                        window.window().dispatch_event_with_result(slint::platform::WindowEvent::CloseRequested)
                {
                    eprintln!("gezik: cannot close the window: {err}");
                }
            }
            Closed::Remaining => self.after_tabs_changed(),
        }
    }

    /// The stable id of tab `index`, to act on that tab later even if tabs move or close.
    pub fn tab_id(&self, index: usize) -> Option<u64> {
        self.0.borrow().tabs.id(index)
    }

    /// Where the tab with `id` is now; `None` once it is closed.
    pub fn tab_index(&self, id: u64) -> Option<usize> {
        self.0.borrow().tabs.index_of(id)
    }

    pub fn tab_count(&self) -> usize {
        self.0.borrow().tabs.len()
    }

    /// Closes the tab with `id`, wherever it is now; does nothing if it is already closed.
    pub fn close_tab_by_id(&self, id: u64) {
        if let Some(index) = self.tab_index(id) {
            self.close_tab(index);
        }
    }

    pub fn close_other_tabs(&self, index: usize) {
        if index == self.0.borrow().tabs.active_index() {
            self.keep_active_tab(|tabs| tabs.close_others(index));
            self.update_chrome();
        } else if index < self.0.borrow().tabs.len() {
            self.with_tabs(|tabs| tabs.close_others(index));
            self.after_tabs_changed();
        }
    }

    pub fn duplicate_tab(&self, index: usize) {
        self.keep_active_tab(|tabs| tabs.duplicate(index));
        self.update_chrome();
    }

    /// Moves tab `from` to position `to`; the active tab stays active.
    pub fn move_tab(&self, from: usize, to: usize) {
        self.keep_active_tab(|tabs| tabs.move_tab(from, to));
        self.update_chrome();
    }

    pub fn go(&self, location: Location) {
        self.save_view();
        self.load(location, Mode::Navigate, None);
    }

    pub fn back(&self) {
        self.save_view();
        let target = self.0.borrow().tabs.active().back_target().map(|e| e.location.clone());
        if let Some(target) = target {
            self.load(target, Mode::Back, None);
        }
    }

    pub fn forward(&self) {
        self.save_view();
        let target = self.0.borrow().tabs.active().forward_target().map(|e| e.location.clone());
        if let Some(target) = target {
            self.load(target, Mode::Forward, None);
        }
    }

    pub fn up(&self) {
        if let Some(parent) = self.active_location().parent() {
            self.go(parent);
        }
    }

    pub fn reload(&self) {
        self.save_view();
        self.load(self.active_location(), Mode::Show, None);
    }

    /// Path of row `index` and whether it is a folder (drives count as folders).
    pub fn entry_path(&self, index: i32) -> Option<(PathBuf, bool)> {
        let inner = self.0.borrow();
        let index = usize::try_from(index).ok()?;
        match &inner.listing {
            Listing::Files(dir, entries) => entries.get(index).map(|e| (dir.join(&e.name), e.is_dir)),
            Listing::Drives(drives) => drives.get(index).map(|d| (d.path.clone(), true)),
        }
    }

    pub fn open_row(&self, index: i32) {
        let Some((path, is_dir)) = self.entry_path(index) else { return };
        if is_dir {
            self.go(Location::Path(path));
        } else if let Err(err) = open::that_detached(&path) {
            self.status(format!("Cannot open {}: {err}", path.display()));
        }
    }

    /// Goes to a typed path. A relative path is taken from the working folder, and on
    /// Windows `..` parts are resolved, so the address bar parts stay right.
    pub fn navigate_text(&self, text: String) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        let path = PathBuf::from(text);
        self.go(Location::Path(std::path::absolute(&path).unwrap_or(path)));
    }

    pub fn crumb_clicked(&self, index: i32) {
        let location = self.active_location();
        let crumb = usize::try_from(index).ok().and_then(|i| crumbs(&location, MAX_CRUMBS).into_iter().nth(i));
        if let Some(crumb) = crumb {
            self.go(crumb.location);
        }
    }

    fn status(&self, text: String) {
        if let Some(window) = self.0.borrow().window.upgrade() {
            window.set_status(text.into());
        }
    }

    /// Stores the active tab's selection and scroll before leaving it.
    fn save_view(&self) {
        let mut inner = self.0.borrow_mut();
        if inner.cleared {
            return;
        }
        let Some(window) = inner.window.upgrade() else { return };
        let selected = usize::try_from(window.get_selected()).ok().and_then(|i| inner.listing.name_at(i));
        let view = ViewState { selected, scroll: window.get_list_scroll() };
        inner.tabs.active_mut().set_view(view);
    }

    /// Lists `location` off the UI thread, then applies `mode` and shows it. Results of a
    /// load overtaken by a newer one are dropped. `note`, if any, replaces the item count
    /// in the status bar once the listing is shown.
    fn load(&self, location: Location, mode: Mode, note: Option<String>) {
        let (window, generation) = {
            let inner = self.0.borrow();
            (inner.window.clone(), inner.generation.clone())
        };
        let ticket = generation.fetch_add(1, Ordering::SeqCst) + 1;
        if let Some(w) = window.upgrade() {
            w.set_status("Loading…".into());
        }
        std::thread::spawn(move || {
            let result = list(&location, mode);
            let _ = window.upgrade_in_event_loop(move |_| {
                if generation.load(Ordering::SeqCst) != ticket {
                    return;
                }
                with_current(|nav| nav.finish_load(location, mode, result, note));
            });
        });
    }

    fn finish_load(&self, location: Location, mode: Mode, result: LoadResult, note: Option<String>) {
        let shown = match &location {
            Location::Path(p) => p.display().to_string(),
            Location::Drives => "This PC".to_owned(),
        };
        let listing = match result {
            LoadResult::Files(path, entries) => Listing::Files(path, Rc::new(entries)),
            LoadResult::Drives(drives) => Listing::Drives(drives),
            LoadResult::Gone { fallback } => {
                // The active tab's folder is gone: go to the nearest folder that still exists.
                self.show_failed(mode, &location, String::new());
                self.load(fallback, Mode::Navigate, Some(format!("{shown} no longer exists")));
                return;
            }
            LoadResult::Failed(err) => return self.show_failed(mode, &location, format!("Cannot open {shown}: {err}")),
        };
        {
            let mut inner = self.0.borrow_mut();
            let history = inner.tabs.active_mut();
            match mode {
                Mode::Navigate => {
                    history.navigate(location);
                }
                Mode::Back => {
                    history.back();
                }
                Mode::Forward => {
                    history.forward();
                }
                Mode::Show => {}
            }
            inner.listing = listing;
            inner.cleared = false;
        }
        self.show_listing(note);
        self.update_chrome();
    }

    /// Shows `message` for a failed load; see [`listing_after_failure`].
    fn show_failed(&self, mode: Mode, location: &Location, message: String) {
        let Some(listing) = listing_after_failure(mode, location) else { return self.status(message) };
        {
            let mut inner = self.0.borrow_mut();
            inner.listing = listing;
            inner.cleared = false;
        }
        self.show_listing(Some(message));
        self.update_chrome();
    }

    fn show_listing(&self, note: Option<String>) {
        let inner = self.0.borrow();
        let Some(window) = inner.window.upgrade() else { return };
        let view = inner.tabs.active().view().clone();
        let count = match &inner.listing {
            Listing::Files(_, entries) => {
                window.set_rows(ModelRc::new(EntryModel { entries: entries.clone(), notify: ModelNotify::default() }));
                entries.len()
            }
            Listing::Drives(drives) => {
                let rows: Vec<FileRow> = drives
                    .iter()
                    .map(|d| FileRow { name: d.label.as_str().into(), is_dir: true, size: "".into() })
                    .collect();
                window.set_rows(ModelRc::new(VecModel::from(rows)));
                drives.len()
            }
        };
        let selected = view
            .selected
            .as_deref()
            .and_then(|n| inner.listing.index_of(n))
            .and_then(|i| i32::try_from(i).ok())
            .unwrap_or(-1);
        window.set_selected(selected);
        window.set_list_scroll(view.scroll);
        // A new model makes the ListView re-place its rows on its next layout: until then the
        // offset may be pulled back into the old list's bounds, and the first placement snaps
        // it to a row boundary. Set it again once that frame is done, unless another load
        // came first. (A zero timer would run before the frame.) The top needs no second pass.
        if view.scroll != 0.0 && count > 0 {
            let (weak, generation, scroll) = (inner.window.clone(), inner.generation.clone(), view.scroll);
            let ticket = generation.load(Ordering::SeqCst);
            slint::Timer::single_shot(SCROLL_RESTORE_DELAY, move || {
                if generation.load(Ordering::SeqCst) == ticket
                    && let Some(window) = weak.upgrade()
                {
                    window.set_list_scroll(scroll);
                }
            });
        }
        window.set_status(note.unwrap_or_else(|| format!("{count} items")).into());
    }

    /// Updates everything that depends on the active location: buttons, address bar, titles.
    pub fn update_chrome(&self) {
        let location = self.active_location();
        let listeners = {
            let inner = self.0.borrow();
            let Some(window) = inner.window.upgrade() else { return };
            let history = inner.tabs.active();
            window.set_can_go_back(history.can_go_back());
            window.set_can_go_forward(history.can_go_forward());
            window.set_can_go_up(location.parent().is_some());
            let parts: Vec<CrumbItem> =
                crumbs(&location, MAX_CRUMBS).into_iter().map(|c| CrumbItem { label: c.label.into() }).collect();
            window.set_crumbs(ModelRc::new(VecModel::from(parts)));
            window.set_current_path(match &location {
                Location::Path(p) => p.display().to_string().into(),
                Location::Drives => "".into(),
            });
            window.set_title_text(format!("{} — Gezik", inner.places.title_for(&location)).into());
            let active = inner.tabs.active_index();
            let tabs = inner
                .tabs
                .iter()
                .enumerate()
                .map(|(i, h)| TabItem { title: inner.places.title_for(h.location()).into(), active: i == active });
            sync_model(&inner.tab_model, tabs);
            inner.on_changed.clone()
        };
        // Called with no borrow held, so listeners may use the navigator.
        for f in &listeners {
            f(&location);
        }
    }
}

/// Makes `model` hold `items`, changing only rows that differ. Replacing the model would
/// rebuild every element (tab, sidebar row), including the one whose click or middle-click
/// is still being handled.
pub fn sync_model<T: Clone + PartialEq + 'static>(model: &VecModel<T>, items: impl Iterator<Item = T>) {
    let mut len = 0;
    for (i, item) in items.enumerate() {
        len = i + 1;
        if i >= model.row_count() {
            model.push(item);
        } else if model.row_data(i).as_ref() != Some(&item) {
            model.set_row_data(i, item);
        }
    }
    while model.row_count() > len {
        model.remove(model.row_count() - 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh folder under the system temp folder, removed on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> TempDir {
            let dir = std::env::temp_dir().join(format!("gezik-nav-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("create temp dir");
            TempDir(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn tab(title: &str, active: bool) -> TabItem {
        TabItem { title: title.into(), active }
    }

    fn titles(model: &VecModel<TabItem>) -> Vec<(String, bool)> {
        model.iter().map(|t| (t.title.to_string(), t.active)).collect()
    }

    #[test]
    fn sync_model_grows_shrinks_and_updates_in_place() {
        let model = VecModel::default();
        sync_model(&model, [tab("a", true), tab("b", false)].into_iter());
        assert_eq!(titles(&model), [("a".into(), true), ("b".into(), false)]);
        sync_model(&model, [tab("a", false), tab("b", true), tab("c", false)].into_iter());
        assert_eq!(titles(&model), [("a".into(), false), ("b".into(), true), ("c".into(), false)]);
        sync_model(&model, [tab("c", true)].into_iter());
        assert_eq!(titles(&model), [("c".into(), true)]);
    }

    #[test]
    fn list_reads_folders() {
        let tmp = TempDir::new("list");
        std::fs::write(tmp.0.join("a.txt"), "x").expect("write");
        match list(&Location::Path(tmp.0.clone()), Mode::Navigate) {
            LoadResult::Files(path, entries) => {
                assert_eq!(path, tmp.0);
                assert_eq!(entries.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["a.txt"]);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn a_vanished_folder_on_show_falls_back_to_its_nearest_existing_ancestor() {
        let tmp = TempDir::new("gone");
        let gone = Location::Path(tmp.0.join("one").join("two"));
        match list(&gone, Mode::Show) {
            LoadResult::Gone { fallback } => assert_eq!(fallback, Location::Path(tmp.0.clone())),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn a_missing_folder_on_navigate_is_an_error() {
        let tmp = TempDir::new("missing");
        let missing = Location::Path(tmp.0.join("nope"));
        for mode in [Mode::Navigate, Mode::Back, Mode::Forward] {
            match list(&missing, mode) {
                LoadResult::Failed(err) => assert_eq!(err.kind(), std::io::ErrorKind::NotFound),
                other => panic!("unexpected {other:?} for {mode:?}"),
            }
        }
    }

    #[test]
    fn a_file_is_not_listed_as_a_folder() {
        let tmp = TempDir::new("file");
        let file = tmp.0.join("f.txt");
        std::fs::write(&file, "x").expect("write");
        assert!(matches!(list(&Location::Path(file), Mode::Show), LoadResult::Failed(_)));
    }

    #[test]
    fn a_failed_show_empties_the_listing_for_that_location() {
        let place = Location::Path(PathBuf::from("/x/locked"));
        match listing_after_failure(Mode::Show, &place) {
            Some(Listing::Files(path, entries)) => {
                assert_eq!(path, PathBuf::from("/x/locked"));
                assert!(entries.is_empty());
            }
            _ => panic!("expected an empty listing"),
        }
        assert!(
            matches!(listing_after_failure(Mode::Show, &Location::Drives), Some(Listing::Drives(d)) if d.is_empty())
        );
    }

    #[test]
    fn a_failed_move_keeps_the_current_listing() {
        let place = Location::Path(PathBuf::from("/x/locked"));
        for mode in [Mode::Navigate, Mode::Back, Mode::Forward] {
            assert!(listing_after_failure(mode, &place).is_none(), "{mode:?}");
        }
    }
}
