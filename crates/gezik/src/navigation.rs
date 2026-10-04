//! Connects the navigation model (tabs + history) to background folder loading and Slint.
//! Only the active tab's listing is kept in memory.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use gezik_core::nav::{Closed, Location, Step, Tabs, ViewState, crumbs, nearest_existing};
use gezik_core::{Entry, list_dir};
use gezik_platform::Drive;
use slint::{ComponentHandle, Model, ModelRc, VecModel};

use crate::places::{Places, PlacesPart};
use crate::view::{Listing, View};
use crate::{AppWindow, CrumbItem, TabItem};

/// Address bar parts shown before older ones collapse into "…".
const MAX_CRUMBS: usize = 4;

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
fn list(location: &Location, mode: &Mode) -> LoadResult {
    match location {
        Location::Drives => LoadResult::Drives(gezik_platform::drives()),
        Location::Path(path) => match list_dir(path) {
            Ok(entries) => LoadResult::Files(path.clone(), entries),
            Err(err) if *mode == Mode::Show && err.kind() == std::io::ErrorKind::NotFound => {
                LoadResult::Gone { fallback: nearest_existing(location, |p| p.is_dir()) }
            }
            Err(err) => LoadResult::Failed(err),
        },
    }
}

/// The listing to show after a load of `location` failed. A failed `Show` leaves the tab at
/// a location whose contents are unknown, so nothing listed for another folder (or tab) may
/// stay on screen: it shows an empty listing for `location`. A failed move (navigate, back,
/// forward, up) changes nothing, so the current listing stays.
fn listing_after_failure(mode: &Mode, location: &Location) -> Option<Listing> {
    (*mode == Mode::Show).then(|| match location {
        Location::Path(path) => Listing::Files(path.clone(), Rc::default()),
        Location::Drives => Listing::Drives(Vec::new()),
    })
}

/// [`listing_after_failure`], marking the view `cleared` when it empties it: the empty
/// listing says nothing about the tab's selection and scroll, so the next `save_view`
/// keeps the tab's saved view instead of overwriting it. A failed move leaves `cleared`.
fn apply_failure(cleared: &mut bool, mode: &Mode, location: &Location) -> Option<Listing> {
    let empty = listing_after_failure(mode, location)?;
    *cleared = true;
    Some(empty)
}

/// The path typed into the address bar. A relative path is taken from `base` (the folder
/// on screen) if there is one, else from the working folder; on Windows `..` parts are
/// resolved too, so the address bar parts stay right.
fn resolve_typed(text: &str, base: Option<&Path>) -> PathBuf {
    let path = PathBuf::from(text);
    let path = match base {
        Some(base) if path.is_relative() => base.join(path),
        _ => path,
    };
    std::path::absolute(&path).unwrap_or(path)
}

/// How a successful load updates the history.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Mode {
    /// Moves in the active tab's history, all applied once their final target has loaded
    /// (none if it fails): more than one when back, forward or up came while one loaded.
    Move(Vec<Step>),
    /// The active tab's current location (reload, tab switch): no history change.
    Show,
}

/// Called when the active location is shown.
type Listener = Rc<dyn Fn(&Location)>;

struct Inner {
    window: slint::Weak<AppWindow>,
    tabs: Tabs,
    view: View,
    /// The listing was cleared for a tab switch (or emptied by a failed reload) and the
    /// active tab's own listing is not shown, so what is on screen says nothing about that
    /// tab's selection and scroll.
    cleared: bool,
    places: Places,
    /// The tab bar's model, updated in place (see [`sync_model`]).
    tab_model: Rc<VecModel<TabItem>>,
    start: Location,
    /// Bumped on every load so that results of an overtaken load are dropped.
    generation: Arc<AtomicU64>,
    /// The history moves of the load started at this generation, so that back, forward
    /// and up made while it loads go on from its target. Stale once the generation moved.
    pending: Option<(u64, Vec<Step>)>,
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
    pub fn new(window: &AppWindow, view: View, first: Location, select: Option<String>, start: Location) -> Navigator {
        let mut tabs = Tabs::new(first);
        tabs.active_mut().set_view(ViewState {
            selected: select.iter().cloned().collect(),
            focus: select,
            scroll: 0.0,
        });
        let tab_model = Rc::new(VecModel::default());
        window.set_tabs(ModelRc::from(tab_model.clone()));
        Navigator(Rc::new(RefCell::new(Inner {
            window: window.as_weak(),
            tabs,
            view,
            cleared: false,
            places: Places::default(),
            tab_model,
            start,
            generation: Arc::default(),
            pending: None,
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
        let view = {
            let mut inner = self.0.borrow_mut();
            inner.cleared = true;
            inner.view.clone()
        };
        // Not while borrowed: the view calls its selection listeners.
        view.clear();
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

    /// The active tab's index. Read-only: unlike `with_tabs`, keeps a pending load.
    pub fn active_index(&self) -> usize {
        self.0.borrow().tabs.active_index()
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

    /// Goes to `location` from the location on screen (a pending move is dropped: the
    /// click or typed path was meant for what is shown).
    pub fn go(&self, location: Location) {
        self.save_view();
        self.load(location.clone(), Mode::Move(vec![Step::Navigate(location)]), None);
    }

    pub fn back(&self) {
        self.queue(|_| Some(Step::Back));
    }

    pub fn forward(&self) {
        self.queue(|_| Some(Step::Forward));
    }

    pub fn up(&self) {
        self.queue(|base| base.parent().map(Step::Navigate));
    }

    /// Adds the step `next` makes from `base` (where a pending move leads, else the current
    /// location) after that pending move, and loads where they all lead. So pressing back
    /// three times while a slow folder loads goes back three levels, not one.
    fn queue(&self, next: impl FnOnce(&Location) -> Option<Step>) {
        let mut steps = self.pending_steps();
        let target = {
            let inner = self.0.borrow();
            let history = inner.tabs.active();
            let Some(base) = history.target_after(&steps) else { return };
            let Some(step) = next(&base) else { return };
            steps.push(step);
            history.target_after(&steps)
        };
        if let Some(target) = target {
            self.save_view();
            self.load(target, Mode::Move(steps), None);
        }
    }

    /// The moves of the load still in flight, if its result would still apply.
    fn pending_steps(&self) -> Vec<Step> {
        let inner = self.0.borrow();
        match &inner.pending {
            Some((ticket, steps)) if *ticket == inner.generation.load(Ordering::SeqCst) => steps.clone(),
            _ => Vec::new(),
        }
    }

    pub fn reload(&self) {
        self.save_view();
        self.load(self.active_location(), Mode::Show, None);
    }

    /// Reloads the active tab if it shows one of `dirs` (a file operation changed them),
    /// keeping the scroll and selecting `select` (by name) if given. The status bar does not
    /// flash "Loading…".
    pub fn refresh_showing(&self, dirs: &[PathBuf], select: &[String]) {
        let Location::Path(current) = self.active_location() else { return };
        if self.0.borrow().cleared || !dirs.iter().any(|dir| gezik_core::ops::paths::same_path(dir, &current)) {
            return;
        }
        self.save_view();
        if !select.is_empty() {
            let mut inner = self.0.borrow_mut();
            let mut view = inner.tabs.active().view().clone();
            view.selected = select.to_vec();
            view.focus = select.first().cloned();
            inner.tabs.active_mut().set_view(view);
        }
        self.load_with(Location::Path(current), Mode::Show, None, false);
    }

    /// Path of entry `index` and whether it is a folder (drives count as folders).
    pub fn entry_path(&self, index: i32) -> Option<(PathBuf, bool)> {
        let index = usize::try_from(index).ok()?;
        self.0.borrow().view.entry_path(index)
    }

    pub fn open_row(&self, index: i32) {
        let Some((path, is_dir)) = self.entry_path(index) else { return };
        if is_dir {
            self.go(Location::Path(path));
        } else if let Err(err) = open::that_detached(&path) {
            self.status(format!("Cannot open {}: {err}", path.display()));
        }
    }

    /// Enter: opens the selected files with their default apps and goes into the first
    /// selected folder; with nothing selected, the focused entry.
    pub fn open_selected(&self) {
        let view = self.0.borrow().view.clone();
        let mut items = view.selected_items();
        if items.is_empty() {
            items.extend(view.focus().and_then(|i| view.entry_path(i)));
        }
        // Too many files: nothing is opened, nor is the folder entered (it would hide the
        // message). Folders do not count towards the limit.
        let files = items.iter().filter(|(_, is_dir)| !is_dir).map(|(path, _)| path).collect();
        let files = match crate::view::limit_open(files) {
            Ok(files) => files,
            Err(message) => return self.status(message),
        };
        for path in files {
            if let Err(err) = open::that_detached(path) {
                self.status(format!("Cannot open {}: {err}", path.display()));
            }
        }
        if let Some((folder, _)) = items.into_iter().find(|(_, is_dir)| *is_dir) {
            self.go(Location::Path(folder));
        }
    }

    /// Goes to a typed path; see [`resolve_typed`].
    pub fn navigate_text(&self, text: String) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        let path = match self.active_location() {
            Location::Path(base) => resolve_typed(text, Some(&base)),
            Location::Drives => resolve_typed(text, None),
        };
        self.go(Location::Path(path));
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
        let view = inner.view.capture();
        inner.tabs.active_mut().set_view(view);
    }

    fn load(&self, location: Location, mode: Mode, note: Option<String>) {
        self.load_with(location, mode, note, true);
    }

    /// Lists `location` off the UI thread, then applies `mode` and shows it. Results of a
    /// load overtaken by a newer one are dropped. `note`, if any, replaces the item count
    /// in the status bar once the listing is shown.
    fn load_with(&self, location: Location, mode: Mode, note: Option<String>, loading_text: bool) {
        let (window, generation, ticket) = {
            let mut inner = self.0.borrow_mut();
            let ticket = inner.generation.fetch_add(1, Ordering::SeqCst) + 1;
            inner.pending = match &mode {
                Mode::Move(steps) => Some((ticket, steps.clone())),
                Mode::Show => None,
            };
            (inner.window.clone(), inner.generation.clone(), ticket)
        };
        if loading_text && let Some(w) = window.upgrade() {
            w.set_status("Loading…".into());
        }
        std::thread::spawn(move || {
            let result = list(&location, &mode);
            let _ = window.upgrade_in_event_loop(move |_| {
                if generation.load(Ordering::SeqCst) != ticket {
                    return;
                }
                with_current(|nav| nav.finish_load(location, mode, result, note));
            });
        });
    }

    fn finish_load(&self, location: Location, mode: Mode, result: LoadResult, note: Option<String>) {
        // This was the pending load (an overtaken one never gets here).
        self.0.borrow_mut().pending = None;
        let shown = match &location {
            Location::Path(p) => p.display().to_string(),
            Location::Drives => "This PC".to_owned(),
        };
        let listing = match result {
            LoadResult::Files(path, entries) => Listing::Files(path, Rc::new(entries)),
            LoadResult::Drives(drives) => Listing::Drives(drives),
            LoadResult::Gone { fallback } => {
                // The active tab's folder is gone: go to the nearest folder that still exists.
                self.show_failed(&mode, &location, String::new());
                let step = Step::Navigate(fallback.clone());
                self.load(fallback, Mode::Move(vec![step]), Some(format!("{shown} no longer exists")));
                return;
            }
            LoadResult::Failed(err) => {
                return self.show_failed(&mode, &location, format!("Cannot open {shown}: {err}"));
            }
        };
        let (view, state) = {
            let mut inner = self.0.borrow_mut();
            if let Mode::Move(steps) = &mode {
                inner.tabs.active_mut().apply_steps(steps);
            }
            inner.cleared = false;
            (inner.view.clone(), inner.tabs.active().view().clone())
        };
        view.show(listing, &state, note);
        self.update_chrome();
    }

    /// Shows `message` for a failed load; see [`apply_failure`].
    fn show_failed(&self, mode: &Mode, location: &Location, message: String) {
        let (empty, view, state) = {
            let mut inner = self.0.borrow_mut();
            let empty = apply_failure(&mut inner.cleared, mode, location);
            (empty, inner.view.clone(), inner.tabs.active().view().clone())
        };
        let Some(empty) = empty else { return self.status(message) };
        view.show(empty, &state, Some(message));
        self.update_chrome();
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

    /// One of each kind of history move.
    fn moves() -> [Mode; 4] {
        [
            Mode::Move(vec![Step::Navigate(Location::Drives)]),
            Mode::Move(vec![Step::Back]),
            Mode::Move(vec![Step::Forward]),
            Mode::Move(vec![Step::Back, Step::Back]),
        ]
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
        match list(&Location::Path(tmp.0.clone()), &Mode::Show) {
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
        match list(&gone, &Mode::Show) {
            LoadResult::Gone { fallback } => assert_eq!(fallback, Location::Path(tmp.0.clone())),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn a_missing_folder_on_navigate_is_an_error() {
        let tmp = TempDir::new("missing");
        let missing = Location::Path(tmp.0.join("nope"));
        for mode in moves() {
            match list(&missing, &mode) {
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
        assert!(matches!(list(&Location::Path(file), &Mode::Show), LoadResult::Failed(_)));
    }

    #[test]
    fn a_failed_show_empties_the_listing_for_that_location() {
        let place = Location::Path(PathBuf::from("/x/locked"));
        match listing_after_failure(&Mode::Show, &place) {
            Some(Listing::Files(path, entries)) => {
                assert_eq!(path, PathBuf::from("/x/locked"));
                assert!(entries.is_empty());
            }
            _ => panic!("expected an empty listing"),
        }
        assert!(
            matches!(listing_after_failure(&Mode::Show, &Location::Drives), Some(Listing::Drives(d)) if d.is_empty())
        );
    }

    #[test]
    fn a_failed_show_keeps_the_tabs_saved_view() {
        for was_cleared in [false, true] {
            let mut cleared = was_cleared;
            let empty = apply_failure(&mut cleared, &Mode::Show, &Location::Path("/x/locked".into()));
            assert!(matches!(empty, Some(Listing::Files(_, entries)) if entries.is_empty()));
            assert!(cleared, "was cleared: {was_cleared}");
        }
    }

    #[test]
    fn a_failed_move_changes_nothing() {
        for was_cleared in [false, true] {
            let mut cleared = was_cleared;
            assert!(apply_failure(&mut cleared, &Mode::Move(Vec::new()), &Location::Path("/y".into())).is_none());
            assert_eq!(cleared, was_cleared);
        }
    }

    #[test]
    fn a_typed_relative_path_starts_at_the_folder_on_screen() {
        let base = std::path::absolute("/work/project").unwrap();
        assert_eq!(resolve_typed("src", Some(&base)), base.join("src"));
        assert_eq!(resolve_typed("src/lib", Some(&base)), base.join("src").join("lib"));
        let elsewhere = std::path::absolute("/other").unwrap();
        assert_eq!(resolve_typed(&elsewhere.display().to_string(), Some(&base)), elsewhere);
        // In "This PC" there is no folder: the working folder is used.
        assert_eq!(resolve_typed("src", None), std::path::absolute("src").unwrap());
    }

    #[cfg(windows)]
    #[test]
    fn a_typed_parent_path_is_resolved_on_windows() {
        let base = PathBuf::from(r"C:\Users\someone");
        assert_eq!(resolve_typed(r"..\other", Some(&base)), PathBuf::from(r"C:\Users\other"));
        assert_eq!(resolve_typed(r"D:\data", Some(&base)), PathBuf::from(r"D:\data"));
        assert_eq!(resolve_typed(r"\root", Some(&base)), PathBuf::from(r"C:\root"));
    }

    #[test]
    fn a_failed_move_keeps_the_current_listing() {
        let place = Location::Path(PathBuf::from("/x/locked"));
        for mode in moves() {
            assert!(listing_after_failure(&mode, &place).is_none(), "{mode:?}");
        }
    }
}
