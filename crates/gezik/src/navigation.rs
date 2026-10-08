//! Connects the navigation model (tabs + history) to background folder loading and Slint.
//! Only the active tab's listing is kept in memory.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use gezik_core::nav::{Closed, Location, Session, Step, Tabs, ViewState, crumbs, nearest_existing};
use gezik_core::ops::paths::same_path;
use gezik_core::refresh::{QUIET, RefreshPace};
use gezik_core::{Entry, list_dir};
use gezik_platform::Drive;
use slint::{ComponentHandle, Model, ModelRc, VecModel};

use crate::folder_watch::FolderWatch;
use crate::places::{Places, PlacesPart};
use crate::view::{Listing, View};
use crate::{AppWindow, CrumbItem, TabItem};

/// Address bar parts shown before older ones collapse into "…".
const MAX_CRUMBS: usize = 4;

/// The status line when a locked tab is asked to close.
pub const LOCKED_TAB: &str = "This tab is locked";

/// The status line when "close other tabs" left `n` locked tabs open.
pub fn locked_kept_text(n: usize) -> String {
    if n == 1 { "1 locked tab stays open".to_owned() } else { format!("{n} locked tabs stay open") }
}

/// How long letting go of a drive about to be removed may take before Windows tries it.
const REMOVAL_GRACE: std::time::Duration = std::time::Duration::from_millis(150);
/// After that, how often (and how many times) to look whether the drive went.
const REMOVAL_CHECK: std::time::Duration = std::time::Duration::from_millis(500);
const REMOVAL_CHECKS: u32 = 20;

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
            Ok(mut entries) => {
                // What a copy or delete is still working on under a temporary name.
                entries.retain(|entry| !gezik_ops::pending::is_internal_name(&entry.name));
                LoadResult::Files(path.clone(), entries)
            }
            Err(err) if *mode == Mode::Show && err.kind() == std::io::ErrorKind::NotFound => {
                LoadResult::Gone { fallback: nearest_existing(location, |p| p.is_dir()) }
            }
            Err(err) => LoadResult::Failed(err),
        },
        // Task 7: a search or the flat view reads no folder (`search::Searches` runs it).
        Location::Search(_) | Location::Flat(_) => {
            LoadResult::Failed(std::io::Error::other("Searching is not ready yet"))
        }
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
        Location::Search(_) | Location::Flat(_) => Listing::default(),
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
pub(crate) fn resolve_typed(text: &str, base: Option<&Path>) -> PathBuf {
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

/// The view to show once a load is done: a move to another place starts without the filter.
/// A move that lands on the folder already on screen (its breadcrumb or sidebar entry clicked,
/// its path typed again) counts as a refresh and keeps the filter: `View::show` keeps the bar
/// as it is for the same folder, whatever the state says.
fn view_to_show(mode: &Mode, saved: &ViewState) -> ViewState {
    match mode {
        Mode::Show => saved.clone(),
        Mode::Move(_) => ViewState { filter: None, ..saved.clone() },
    }
}

/// `state` with `names` selected and the first of them focused ("Show in folder"); `state`
/// as it is when there are none.
fn with_selection(mut state: ViewState, names: Option<Vec<String>>) -> ViewState {
    if let Some(names) = names.filter(|names| !names.is_empty()) {
        state.focus = names.first().cloned();
        state.selected = names;
    }
    state
}

/// Whether a finished load is a visit (spec 6.2): a move, or the first show of a tab opened
/// there; not a reload, a tab switch, or the `fallback` from a folder found gone.
fn is_visit(mode: &Mode, opened: bool, fallback: bool) -> bool {
    (matches!(mode, Mode::Move(_)) || opened) && !fallback
}

/// Called when the active location is shown.
type Listener = Rc<dyn Fn(&Location)>;
/// Called with each folder gone to.
type VisitListener = Rc<dyn Fn(&Path)>;
/// Told the tabs as the session keeps them.
type SessionSink = Rc<dyn Fn(&Session)>;

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
    /// The generation of a load the user started (navigation, tab switch); a quiet refresh
    /// must not overtake it.
    user_load: Option<u64>,
    on_changed: Vec<Listener>,
    /// Watches the folder on screen; its changes reload it, paced by `pace`.
    watch: FolderWatch,
    watched: Option<PathBuf>,
    pace: RefreshPace,
    refresh_timer: slint::Timer,
    /// Told when the watched folder's drive is about to be removed (Windows), to let go of it.
    removal: Option<gezik_platform::RemovalWatch>,
    on_visited: Vec<VisitListener>,
    /// Names to select once the next move is shown ("Show in folder"). Dropped when that
    /// load fails or is overtaken, and by any other load the user starts.
    select_next: Option<Vec<String>>,
    /// A tab was opened in front (its id): its first show is a visit.
    visit_next_show: Option<u64>,
    /// The load (its generation) going to the nearest folder of one found gone: no visit.
    fallback: Option<u64>,
    /// Told the tabs as the session keeps them when they change (main.rs: state.toml).
    session_sink: Option<SessionSink>,
    /// What the sink was told last (at first: what state.toml had).
    session_sent: Session,
    /// `[session] restore`: off, the sink is told an empty session (state.toml forgets it).
    session_on: bool,
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
    /// The tabs of `session` open, the one in front with `select` selected; new tabs open at
    /// `start`. Does not load anything: call [`install`](Self::install) next.
    pub fn new(window: &AppWindow, view: View, session: Session, select: Option<String>, start: Location) -> Navigator {
        let mut tabs = Tabs::from_session(&session).unwrap_or_else(|| Tabs::new(start.clone()));
        tabs.active_mut().set_view(ViewState {
            selected: select.iter().cloned().collect(),
            focus: select,
            scroll: 0.0,
            filter: None,
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
            user_load: None,
            on_changed: Vec::new(),
            watch: FolderWatch::new(|| {
                let _ = slint::invoke_from_event_loop(|| with_current(Navigator::folder_changed));
            }),
            watched: None,
            pace: RefreshPace::new(),
            refresh_timer: slint::Timer::default(),
            removal: None,
            on_visited: Vec::new(),
            select_next: None,
            visit_next_show: None,
            fallback: None,
            session_sink: None,
            session_sent: Session::default(),
            session_on: false,
        })))
    }

    /// The watched folder changed on disk.
    fn folder_changed(&self) {
        {
            let mut inner = self.0.borrow_mut();
            if inner.watch.take_change() {
                inner.pace.changed(Instant::now());
            }
        }
        self.schedule_refresh();
    }

    /// Sets the timer for the reload the watched folder's changes call for, if any.
    fn schedule_refresh(&self) {
        let inner = self.0.borrow();
        if let Some(at) = inner.pace.next() {
            let delay = at.saturating_duration_since(Instant::now());
            inner.refresh_timer.start(slint::TimerMode::SingleShot, delay, || with_current(Navigator::refresh_due));
        }
    }

    /// Reloads the watched folder for its changes, quietly; later while the user loads
    /// something, drags a selection rectangle or renames.
    fn refresh_due(&self) {
        let watched = {
            let inner = self.0.borrow();
            // A reload rebuilds the rows: it would end a rubber-band drag, and put a rename's
            // caret back at the start.
            let busy = inner.user_load.is_some()
                || inner.cleared
                || inner.view.marquee_active()
                || inner.view.renaming().is_some();
            if busy {
                inner.refresh_timer.start(slint::TimerMode::SingleShot, QUIET, || with_current(Navigator::refresh_due));
                return;
            }
            inner.watched.clone()
        };
        let Some(watched) = watched else { return };
        if !matches!(self.active_location(), Location::Path(ref path) if same_path(path, &watched)) {
            return;
        }
        self.save_view();
        self.load_with(Location::Path(watched), Mode::Show, None, false);
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
        self.after_tabs_changed_noted(None);
    }

    /// [`after_tabs_changed`](Self::after_tabs_changed); `note`, if any, shows in the status bar
    /// once the tab's listing is back (instead of its item count).
    fn after_tabs_changed_noted(&self, note: Option<String>) {
        let view = {
            let mut inner = self.0.borrow_mut();
            inner.cleared = true;
            inner.view.clone()
        };
        // Not while borrowed: the view calls its selection listeners.
        view.clear();
        self.update_chrome();
        self.load(self.active_location(), Mode::Show, note);
    }

    /// Opens a tab at `location` right after the active one; `activate` switches to it.
    pub fn open_tab(&self, location: Location, activate: bool) {
        if activate {
            let index = self.with_tabs(|tabs| tabs.open(location, true));
            self.0.borrow_mut().visit_next_show = self.tab_id(index);
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
        if self.0.borrow().tabs.is_locked(index) {
            return self.status(LOCKED_TAB.to_owned());
        }
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
            // Caught above.
            Closed::Locked => {}
        }
    }

    /// The stable id of tab `index`, to act on that tab later even if tabs move or close.
    pub fn tab_id(&self, index: usize) -> Option<u64> {
        self.0.borrow().tabs.id(index)
    }

    /// The location tab `index` shows.
    pub fn tab_location(&self, index: usize) -> Option<Location> {
        self.0.borrow().tabs.get(index).map(|history| history.location().clone())
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

    /// Closes every tab but `index` and the locked ones. The status bar says how many locked
    /// tabs stayed; when `index` was not active, only once its listing is back (the listing's
    /// item count would replace it).
    pub fn close_other_tabs(&self, index: usize) {
        let note = |locked: usize| (locked > 0).then(|| locked_kept_text(locked));
        if index == self.0.borrow().tabs.active_index() {
            let locked = self.keep_active_tab(|tabs| tabs.close_others(index));
            self.update_chrome();
            if let Some(note) = note(locked) {
                self.status(note);
            }
        } else if index < self.0.borrow().tabs.len() {
            let locked = self.with_tabs(|tabs| tabs.close_others(index));
            self.after_tabs_changed_noted(note(locked));
        }
    }

    /// Opens a tab set (spec 5.2): `locations` as tabs at the end, the first in front (a
    /// visit); `replace` first closes the unlocked tabs, and the status bar then says how many
    /// locked ones stayed. A folder gone on this computer falls back when it is shown.
    pub fn open_tab_set(&self, locations: Vec<Location>, replace: bool) {
        if locations.is_empty() {
            return;
        }
        let locked = self.with_tabs(|tabs| tabs.open_set(locations, replace));
        let front = self.tab_id(self.active_index());
        self.0.borrow_mut().visit_next_show = front;
        self.after_tabs_changed_noted((locked > 0).then(|| locked_kept_text(locked)));
    }

    /// Every tab's location, in tab order.
    pub fn tab_locations(&self) -> Vec<Location> {
        self.0.borrow().tabs.iter().map(|history| history.location().clone()).collect()
    }

    /// Opens the last closed tab again where it was, with its history and its view (selection,
    /// scroll, filter); nothing if no tab was closed.
    pub fn reopen_tab(&self) {
        if self.0.borrow().tabs.closed_count() == 0 {
            return;
        }
        self.with_tabs(Tabs::reopen);
        self.after_tabs_changed();
    }

    /// Locks tab `index` if it is unlocked, and the other way round.
    pub fn toggle_tab_lock(&self, index: usize) {
        self.keep_active_tab(|tabs| tabs.set_locked(index, !tabs.is_locked(index)));
        self.update_chrome();
    }

    pub fn is_tab_locked(&self, index: usize) -> bool {
        self.0.borrow().tabs.is_locked(index)
    }

    /// Every tab's title and path ("" for This PC), in tab order.
    pub fn tab_list(&self) -> Vec<(String, String)> {
        let inner = self.0.borrow();
        inner
            .tabs
            .iter()
            .map(|history| {
                let location = history.location();
                let path = match location {
                    Location::Path(path) => path.display().to_string(),
                    Location::Drives => String::new(),
                    // Task 7 places these.
                    Location::Search(_) | Location::Flat(_) => {
                        location.folder().map(|p| p.display().to_string()).unwrap_or_default()
                    }
                };
                (inner.places.title_for(location), path)
            })
            .collect()
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

    /// Goes to `dir` and selects `names` there ("Show in folder"); the folder shown is
    /// reloaded with them selected.
    pub fn go_selecting(&self, dir: PathBuf, names: Vec<String>) {
        if matches!(self.active_location(), Location::Path(ref current) if same_path(current, &dir))
            && self.refresh_showing(std::slice::from_ref(&dir), &names, None)
        {
            return;
        }
        self.go(Location::Path(dir));
        // After the go, which drops any older names: these belong to the load it started.
        self.0.borrow_mut().select_next = Some(names);
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
    /// `note`, if any, shows in the status bar once the listing is back. Returns whether a
    /// reload started; none does while a load the user started is under way (it would
    /// overtake it).
    pub fn refresh_showing(&self, dirs: &[PathBuf], select: &[String], note: Option<String>) -> bool {
        let Location::Path(current) = self.active_location() else { return false };
        {
            let inner = self.0.borrow();
            let user_loading = inner.user_load == Some(inner.generation.load(Ordering::SeqCst));
            if inner.cleared || user_loading || !dirs.iter().any(|dir| gezik_core::ops::paths::same_path(dir, &current))
            {
                return false;
            }
        }
        self.save_view();
        if !select.is_empty() {
            let mut inner = self.0.borrow_mut();
            let mut view = inner.tabs.active().view().clone();
            view.selected = select.to_vec();
            view.focus = select.first().cloned();
            inner.tabs.active_mut().set_view(view);
        }
        self.load_with(Location::Path(current), Mode::Show, note, false);
        true
    }

    /// Path of entry `index` and whether it is a folder (drives count as folders).
    pub fn entry_path(&self, index: i32) -> Option<(PathBuf, bool)> {
        let index = usize::try_from(index).ok()?;
        self.0.borrow().view.entry_path(index)
    }

    /// Opens the file with its default app, or goes into the folder.
    pub fn open_item(&self, path: PathBuf, is_dir: bool) {
        if is_dir {
            self.go(Location::Path(path));
        } else if let Err(err) = open::that_detached(&path) {
            self.status(format!("Cannot open {}: {}", path.display(), gezik_platform::fs::describe(&err)));
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
                self.status(format!("Cannot open {}: {}", path.display(), gezik_platform::fs::describe(&err)));
            }
        }
        if let Some((folder, _)) = items.into_iter().find(|(_, is_dir)| *is_dir) {
            self.go(Location::Path(folder));
        }
    }

    /// Goes to a typed path: `~` and environment variables put in, then [`resolve_typed`].
    pub fn navigate_text(&self, text: String) {
        let text = crate::path_box::expand(text.trim());
        if text.is_empty() {
            return;
        }
        let path = match self.active_location().folder() {
            Some(base) => resolve_typed(&text, Some(base)),
            None => resolve_typed(&text, None),
        };
        self.go(Location::Path(path));
    }

    pub fn crumb_clicked(&self, index: i32) {
        if let Some(location) = usize::try_from(index).ok().and_then(|i| self.crumb_location(i)) {
            self.go(location);
        }
    }

    /// Where address bar part `index` leads.
    pub fn crumb_location(&self, index: usize) -> Option<Location> {
        crumbs(&self.active_location(), MAX_CRUMBS).into_iter().nth(index).map(|crumb| crumb.location)
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
            inner.user_load = loading_text.then_some(ticket);
            if loading_text {
                inner.select_next = None;
            }
            // Any load of the watched folder covers its changes so far.
            if let Location::Path(path) = &location
                && inner.watched.as_deref().is_some_and(|watched| same_path(watched, path))
            {
                inner.pace.started(Instant::now());
            }
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
        let select_next;
        // This was the pending load (an overtaken one never gets here). A tab opened on a
        // folder that fails is no visit, nor is its next reload.
        let (opened, fallback) = {
            let mut inner = self.0.borrow_mut();
            let ticket = inner.pending.take().map(|(ticket, _)| ticket);
            // Names for this load only: a failed one drops them too.
            select_next = inner.select_next.take();
            inner.user_load = None;
            inner.pace.finished(Instant::now());
            let active = inner.tabs.id(inner.tabs.active_index());
            let opened = inner.visit_next_show.take().is_some_and(|id| Some(id) == active);
            let fallback = inner.fallback.take();
            (opened, ticket.is_some() && fallback == ticket)
        };
        let shown = match &location {
            Location::Path(p) => p.display().to_string(),
            Location::Drives => gezik_core::nav::DRIVES_NAME.to_owned(),
            Location::Search(_) | Location::Flat(_) => {
                location.folder().map(|p| p.display().to_string()).unwrap_or_default()
            }
        };
        let listing = match result {
            LoadResult::Files(path, entries) => Listing::Files(path, Rc::new(entries)),
            LoadResult::Drives(drives) => Listing::Drives(drives),
            LoadResult::Gone { fallback } => {
                // The active tab's folder is gone: go to the nearest folder that still exists.
                self.show_failed(&mode, &location, String::new());
                let step = Step::Navigate(fallback.clone());
                self.load(fallback, Mode::Move(vec![step]), Some(format!("{shown} no longer exists")));
                let mut inner = self.0.borrow_mut();
                inner.fallback = inner.pending.as_ref().map(|(ticket, _)| *ticket);
                return;
            }
            LoadResult::Failed(err) => {
                let why = gezik_platform::fs::describe(&err);
                return self.show_failed(&mode, &location, format!("Cannot open {shown}: {why}"));
            }
        };
        let (view, state) = {
            let mut inner = self.0.borrow_mut();
            if let Mode::Move(steps) = &mode {
                inner.tabs.active_mut().apply_steps(steps);
            }
            inner.cleared = false;
            let state = with_selection(view_to_show(&mode, inner.tabs.active().view()), select_next);
            (inner.view.clone(), state)
        };
        self.watch_shown(&location);
        view.show(listing, &state, note);
        self.update_chrome();
        self.schedule_refresh();
        let visited = self.0.borrow().on_visited.clone();
        if is_visit(&mode, opened, fallback)
            && let Location::Path(path) = &location
        {
            for f in &visited {
                f(path);
            }
        }
    }

    /// Calls `f` with each folder the user goes to (spec 6.2).
    pub fn on_visited(&self, f: impl Fn(&Path) + 'static) {
        self.0.borrow_mut().on_visited.push(Rc::new(f));
    }

    /// Watches the folder now on screen (none for This PC).
    fn watch_shown(&self, location: &Location) {
        // Results are not watched (spec 4.7).
        let folder = match location {
            Location::Path(path) => Some(path.clone()),
            Location::Drives | Location::Search(_) | Location::Flat(_) => None,
        };
        let mut inner = self.0.borrow_mut();
        let same = match (&folder, &inner.watched) {
            (Some(a), Some(b)) => same_path(a, b),
            (None, None) => true,
            _ => false,
        };
        if !same {
            inner.watch.watch(folder.as_deref());
            inner.removal = None;
            inner.removal = folder.as_deref().and_then(|folder| {
                let window = inner.window.upgrade()?;
                gezik_platform::watch_removal(&window.window().window_handle(), folder, || {
                    with_current(Navigator::drive_removal_asked);
                })
            });
            inner.watched = folder;
            inner.pace.reset();
            inner.refresh_timer.stop();
        }
    }

    /// The watched folder's drive is about to be removed: let go of it now. Then see, for a
    /// while, whether it went (the list moves to the nearest folder still there) or stayed
    /// (watched again).
    fn drive_removal_asked(&self) {
        let (removal, folder) = {
            let mut inner = self.0.borrow_mut();
            inner.watch.stop_now();
            inner.refresh_timer.stop();
            (inner.removal.take(), inner.watched.clone())
        };
        // A no-op elsewhere (no removal watch there), and clippy says so on macOS and Linux.
        #[cfg_attr(not(windows), allow(clippy::drop_non_drop))]
        drop(removal);
        // Windows tries the drive as soon as this returns; the watcher's thread closes its
        // handles within moments.
        std::thread::sleep(REMOVAL_GRACE);
        let Some(folder) = folder else { return };
        let checks = Rc::new(std::cell::Cell::new(0u32));
        self.0.borrow().refresh_timer.start(slint::TimerMode::Repeated, REMOVAL_CHECK, move || {
            let gone = !folder.exists();
            checks.set(checks.get() + 1);
            if gone || checks.get() >= REMOVAL_CHECKS {
                with_current(|nav| {
                    nav.0.borrow().refresh_timer.stop();
                    // Still there after all: watch it again (forgotten first, so it starts anew).
                    nav.0.borrow_mut().watched = None;
                    if gone {
                        nav.reload();
                    } else {
                        nav.watch_shown(&nav.active_location());
                    }
                });
            }
        });
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
                Location::Search(_) | Location::Flat(_) => {
                    location.folder().map(|p| p.display().to_string()).unwrap_or_default().into()
                }
            });
            window.set_title_text(format!("{} — Gezik", inner.places.title_for(&location)).into());
            let active = inner.tabs.active_index();
            let tabs = inner.tabs.iter().enumerate().map(|(i, h)| TabItem {
                title: inner.places.title_for(h.location()).into(),
                active: i == active,
                locked: inner.tabs.is_locked(i),
            });
            sync_model(&inner.tab_model, tabs);
            window.set_active_tab_locked(inner.tabs.is_locked(active));
            inner.on_changed.clone()
        };
        // Called with no borrow held, so listeners may use the navigator.
        for f in &listeners {
            f(&location);
        }
        self.send_session();
    }

    /// Tells `sink` the tabs whenever they change, while `restore` is on (spec 5.1); `saved`
    /// is what state.toml has now, so an unchanged session is not written again.
    pub fn keep_session(&self, saved: Session, restore: bool, sink: impl Fn(&Session) + 'static) {
        {
            let mut inner = self.0.borrow_mut();
            inner.session_sink = Some(Rc::new(sink));
            inner.session_sent = saved;
            inner.session_on = restore;
        }
        self.send_session();
    }

    /// `[session] restore` changed (settings.toml was reloaded): written or forgotten now.
    pub fn set_session_restore(&self, restore: bool) {
        self.0.borrow_mut().session_on = restore;
        self.send_session();
    }

    /// Tells the sink the session if it is not what it was told last.
    fn send_session(&self) {
        let (sink, session) = {
            let mut inner = self.0.borrow_mut();
            let Some(sink) = inner.session_sink.clone() else { return };
            let Some(session) = session_to_send(inner.session_on, inner.tabs.session(), &inner.session_sent) else {
                return;
            };
            inner.session_sent = session.clone();
            (sink, session)
        };
        // With no borrow held: the sink may use the navigator.
        sink(&session);
    }
}

/// What state.toml should get, if anything: the tabs while restoring is on, else an empty
/// session; `None` when that is what it was told last.
fn session_to_send(on: bool, tabs: Session, sent: &Session) -> Option<Session> {
    let wanted = if on { tabs } else { Session::default() };
    (wanted != *sent).then_some(wanted)
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

    #[test]
    fn show_in_folder_names_select_and_focus_the_first() {
        let saved =
            ViewState { selected: vec!["old".to_owned()], focus: Some("old".to_owned()), ..ViewState::default() };
        let names = vec!["a".to_owned(), "b".to_owned()];
        let shown = with_selection(saved.clone(), Some(names.clone()));
        assert_eq!((shown.selected, shown.focus), (names, Some("a".to_owned())));
        assert_eq!(with_selection(saved.clone(), None), saved, "no names: the saved view");
        assert_eq!(with_selection(saved.clone(), Some(Vec::new())), saved, "empty names change nothing");
    }

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

    #[test]
    fn a_move_shows_the_place_without_its_filter_and_a_reload_keeps_it() {
        let saved = ViewState { filter: Some("*.jpg".into()), ..ViewState::default() };
        assert_eq!(view_to_show(&Mode::Show, &saved).filter.as_deref(), Some("*.jpg"), "reload, tab switch");
        for mode in moves() {
            assert_eq!(view_to_show(&mode, &saved).filter, None, "{mode:?}");
        }
    }

    #[test]
    fn moves_and_opened_tabs_are_visits_but_reloads_are_not() {
        for mode in moves() {
            assert!(is_visit(&mode, false, false), "{mode:?}");
            assert!(!is_visit(&mode, false, true), "the folder a gone one falls back to: {mode:?}");
        }
        assert!(!is_visit(&Mode::Show, false, false), "a reload or a tab switch");
        assert!(is_visit(&Mode::Show, true, false), "a tab opened there");
    }

    fn tab(title: &str, active: bool) -> TabItem {
        TabItem { title: title.into(), active, locked: false }
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

    /// What a running copy or delete keeps under its own temporary names is not shown.
    #[test]
    fn list_leaves_out_gezik_temporary_names() {
        let tmp = TempDir::new("list-temp");
        std::fs::write(tmp.0.join("a.txt"), "x").expect("write");
        std::fs::write(tmp.0.join(".gezik-copying-0123456789abcdef-0"), "x").expect("write");
        std::fs::create_dir(tmp.0.join(".gezik-deleting-0123456789abcdef")).expect("mkdir");
        std::fs::write(tmp.0.join(".gezik-copying-"), "a user's name, not ours").expect("write");
        match list(&Location::Path(tmp.0.clone()), &Mode::Show) {
            LoadResult::Files(_, entries) => {
                let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
                assert_eq!(names, [".gezik-copying-", "a.txt"]);
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

    #[test]
    fn a_typed_path_is_expanded_before_it_is_resolved() {
        let base = std::path::absolute("/work").unwrap();
        let home = std::path::absolute("/home/ali").unwrap();
        let text = gezik_core::nav::expand_typed("~/x", &home, |_| None, cfg!(windows));
        assert_eq!(resolve_typed(&text, Some(&base)), home.join("x"));
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

    #[test]
    fn locked_tabs_are_counted_in_words() {
        assert_eq!(locked_kept_text(1), "1 locked tab stays open");
        assert_eq!(locked_kept_text(3), "3 locked tabs stay open");
    }

    #[test]
    fn the_session_is_sent_when_it_changed_and_emptied_when_off() {
        use gezik_core::nav::SessionTab;
        let one = Session::single(Location::Path(PathBuf::from("/a")));
        assert_eq!(session_to_send(true, one.clone(), &Session::default()), Some(one.clone()));
        assert_eq!(session_to_send(true, one.clone(), &one), None, "unchanged: not written again");
        assert_eq!(session_to_send(false, one.clone(), &one), Some(Session::default()), "off: forgotten");
        assert_eq!(session_to_send(false, one.clone(), &Session::default()), None, "off and empty: nothing to write");
        let locked = Session { tabs: vec![SessionTab { location: Location::Drives, locked: true }], active: 0 };
        assert!(session_to_send(true, locked, &one).is_some());
    }
}
