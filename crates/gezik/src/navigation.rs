//! Connects the navigation model (tabs + history) to background folder loading and Slint.
//! Only the active tab's listing is kept in memory.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use gezik_core::nav::{Location, Tabs, ViewState, crumbs, nearest_existing};
use gezik_core::{Entry, format_size, list_dir};
use gezik_platform::Drive;
use slint::{ComponentHandle, Model, ModelNotify, ModelRc, ModelTracker, VecModel};

use crate::places::Places;
use crate::{AppWindow, CrumbItem, FileRow};

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
enum LoadResult {
    Files(PathBuf, Vec<Entry>),
    Drives(Vec<Drive>),
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
#[derive(Clone, Copy, PartialEq, Eq)]
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
    places: Places,
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

// Parts of this interface are for the tab bar, sidebar and shortcuts (later tasks).
#[allow(dead_code)]
impl Navigator {
    /// Does not load anything: call [`install`](Self::install) next.
    pub fn new(window: &AppWindow, start: Location) -> Navigator {
        Navigator(Rc::new(RefCell::new(Inner {
            window: window.as_weak(),
            tabs: Tabs::new(start.clone()),
            listing: Listing::Files(PathBuf::new(), Rc::default()),
            places: Places::default(),
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

    pub fn set_places(&self, places: Places) {
        self.0.borrow_mut().places = places;
        self.update_chrome();
    }

    /// Calls `f` whenever the active location is shown (changed, reloaded or tab switched).
    pub fn on_changed(&self, f: impl Fn(&Location) + 'static) {
        self.0.borrow_mut().on_changed.push(Rc::new(f));
    }

    /// Runs `f` on the tab set. Call [`after_tabs_changed`](Self::after_tabs_changed) afterwards
    /// if the active tab may have changed.
    pub fn with_tabs<R>(&self, f: impl FnOnce(&mut Tabs) -> R) -> R {
        self.save_view();
        f(&mut self.0.borrow_mut().tabs)
    }

    /// Shows the (possibly new) active tab.
    pub fn after_tabs_changed(&self) {
        self.load(self.active_location(), Mode::Show, None);
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
            let result = match &location {
                Location::Drives => Ok(LoadResult::Drives(gezik_platform::drives())),
                Location::Path(path) => list_dir(path).map(|entries| LoadResult::Files(path.clone(), entries)),
            };
            let _ = window.upgrade_in_event_loop(move |_| {
                if generation.load(Ordering::SeqCst) != ticket {
                    return;
                }
                with_current(|nav| nav.finish_load(location, mode, result, note));
            });
        });
    }

    fn finish_load(&self, location: Location, mode: Mode, result: std::io::Result<LoadResult>, note: Option<String>) {
        let listing = match result {
            Ok(LoadResult::Files(path, entries)) => Listing::Files(path, Rc::new(entries)),
            Ok(LoadResult::Drives(drives)) => Listing::Drives(drives),
            Err(err) => return self.load_failed(location, mode, err),
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
        }
        self.show_listing(note);
        self.update_chrome();
    }

    fn load_failed(&self, location: Location, mode: Mode, err: std::io::Error) {
        let shown = match &location {
            Location::Path(p) => p.display().to_string(),
            Location::Drives => "This PC".to_owned(),
        };
        if mode == Mode::Show && err.kind() == std::io::ErrorKind::NotFound {
            // The active tab's folder is gone: go to the nearest folder that still exists.
            let fallback = nearest_existing(&location, |p| p.is_dir());
            self.load(fallback, Mode::Navigate, Some(format!("{shown} no longer exists")));
            return;
        }
        self.status(format!("Cannot open {shown}: {err}"));
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
        if view.scroll != 0.0 {
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
            inner.on_changed.clone()
        };
        // Called with no borrow held, so listeners may use the navigator.
        for f in &listeners {
            f(&location);
        }
    }
}
