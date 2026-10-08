//! The search bar and the flat view (spec 4, 5): the bar opened on the folder shown, its
//! fields and menus, a search as a step in the tab's history (`Location::Search`), its batches
//! into the results list, the name cache (spec 3.5), and each tab's last results (Karar 12).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use gezik_config::settings::SearchSettings;
use gezik_core::nav::{DRIVES_NAME, Location};
use gezik_core::search::{DateRange, HiddenRule, KindFilter, Scope, SearchSpec, parse_size, size_text};
use gezik_search::cache::{CACHE_LIMIT, CacheOutcome, NameCache};
use gezik_search::content::ContentMatcher;
use gezik_search::name::NameMatcher;
use gezik_search::query::{Query, QueryError, QueryOptions};
use gezik_search::results::ResultSet;
use gezik_search::run::{Event, Running, Summary, plan_walk, send_whole, start_with};
use slint::ComponentHandle;

use crate::AppWindow;
use crate::context_menu::{self as ids, Submenu};
use crate::dialog::Dialogs;
use crate::navigation::Navigator;
use crate::preview::with_commas;
use crate::view::{Listing, View};

/// Quiet time after a key before the results follow what is typed (spec 3.5, sapma 15).
const TYPING: Duration = Duration::from_millis(150);
/// An unused name cache goes after this long (spec 3.5).
const CACHE_IDLE: Duration = Duration::from_secs(120);

thread_local! {
    static CURRENT: RefCell<Option<Searches>> = const { RefCell::new(None) };
}

/// Runs `f` with this UI thread's searches, if set up.
pub fn with_current(f: impl FnOnce(&Searches)) {
    if let Some(searches) = CURRENT.with(|c| c.borrow().clone()) {
        f(&searches);
    }
}

/// Whether a search runs (Esc on the list stops it first).
pub fn running() -> bool {
    CURRENT.with(|c| c.borrow().as_ref().is_some_and(|s| s.0.running.borrow().is_some()))
}

/// The status bar while a search runs: `Searching… 12,345 found · 48,210 folders`.
pub fn progress_text(found: usize, folders: usize) -> String {
    match folders {
        0 => format!("Searching… {} found", with_commas(found)),
        n => format!("Searching… {} found · {} folders", with_commas(found), with_commas(n)),
    }
}

/// The status bar once it ended (spec 4.3).
pub fn done_text(summary: &Summary) -> String {
    let found = with_commas(summary.found);
    let results = if summary.found == 1 { "result" } else { "results" };
    let mut text = if summary.limit_reached {
        format!("Stopped at {found} results. Narrow the search.")
    } else if summary.cancelled {
        format!("Stopped · {found} {results}")
    } else {
        format!("{found} {results} in {:.1} s", summary.elapsed.as_secs_f64())
    };
    if summary.everything {
        text.push_str(" via Everything");
    }
    match summary.skipped {
        0 => {}
        1 => text.push_str(" · Skipped 1 folder (search.skip)"),
        n => text.push_str(&format!(" · Skipped {} folders (search.skip)", with_commas(n))),
    }
    match summary.problems.count {
        0 => {}
        1 => text.push_str(" · 1 folder could not be read"),
        n => text.push_str(&format!(" · {} folders could not be read", with_commas(n))),
    }
    text
}

fn name_of(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string())
}

/// The scope menu's choices (spec 4.1): the folder the bar opened on, up to five folders above
/// it, its whole drive, This PC.
pub fn scope_choices(origin: Option<&Path>) -> Vec<(Scope, String)> {
    let mut out = Vec::new();
    if let Some(folder) = origin {
        out.push((Scope::Folder(folder.to_path_buf()), format!("This folder ({})", name_of(folder))));
        let root = folder.ancestors().last().unwrap_or(folder);
        for above in folder.ancestors().skip(1).filter(|p| p.parent().is_some()).take(5) {
            out.push((Scope::Folder(above.to_path_buf()), name_of(above)));
        }
        if root != folder {
            let drive = root.display().to_string();
            let drive = if drive.len() > 1 { drive.trim_end_matches(['\\', '/']).to_owned() } else { drive };
            out.push((Scope::Folder(root.to_path_buf()), format!("Whole drive ({drive})")));
        }
    }
    out.push((Scope::AllDrives, DRIVES_NAME.to_owned()));
    out
}

/// The scope button's text: `in Work`, `in This PC`.
pub fn scope_label(scope: &Scope) -> String {
    match scope {
        Scope::Folder(folder) => format!("in {}", name_of(folder)),
        Scope::AllDrives => format!("in {DRIVES_NAME}"),
    }
}

/// The search a location shows: its own, or the flat view's.
pub fn spec_of(location: &Location) -> Option<SearchSpec> {
    match location {
        Location::Search(spec) => Some((**spec).clone()),
        Location::Flat(folder) => Some(SearchSpec::flat_view(folder.clone())),
        Location::Path(_) | Location::Drives => None,
    }
}

/// The search bar's menus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMenu {
    Scope,
    Filters,
    More,
}

impl SearchMenu {
    /// By the number the bar's `menu` callback gives: 1 the scope, 2 Filters, else ▾.
    pub fn from_index(which: i32) -> SearchMenu {
        match which {
            1 => SearchMenu::Scope,
            2 => SearchMenu::Filters,
            _ => SearchMenu::More,
        }
    }
}

/// What the name cache is for: the scope and the rules it read with.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CacheKey {
    scope: Scope,
    shown: Option<(bool, bool)>,
    skip: Vec<String>,
}

enum Names {
    None,
    Building(CacheKey, Arc<AtomicBool>),
    Ready(CacheKey, Arc<NameCache>),
    /// More than the cache holds: each Enter walks (spec 3.5).
    TooLarge(CacheKey),
    /// No cache: Everything answers names here (`true`), or a network folder (`false`).
    NoCache(CacheKey, bool),
}

/// What the bar's errors depend on: the name, the text in files and their options.
type Checked = (String, bool, String, bool, bool);

/// What a key typed in the name field does with the name cache in the state `names` for `key`.
#[derive(Debug, PartialEq, Eq)]
enum Live {
    /// The cache (or Everything) answers: the results follow the typing.
    Now,
    /// The cache is being read: the results follow once it is.
    Wait,
    /// More than the cache holds: Enter searches.
    Large,
    /// A network folder: Enter searches.
    Never,
    /// No cache for this scope and these rules (none yet, dropped when idle, or another key):
    /// read it, then follow.
    Warm,
}

fn live_step(names: &Names, key: &CacheKey) -> Live {
    match names {
        Names::Ready(k, _) | Names::NoCache(k, true) if k == key => Live::Now,
        Names::Building(k, _) if k == key => Live::Wait,
        Names::TooLarge(k) if k == key => Live::Large,
        Names::NoCache(k, false) if k == key => Live::Never,
        _ => Live::Warm,
    }
}

/// Whether `names` is still the cache read under `cancel` (a later read of the same scope has
/// its own flag: an older one's answer must not end it).
fn this_build(names: &Names, cancel: &Arc<AtomicBool>) -> bool {
    matches!(names, Names::Building(_, own) if Arc::ptr_eq(own, cancel))
}

/// What reading the scope gave.
enum Warmed {
    Ready(Arc<NameCache>),
    TooLarge,
    Everything,
    Network,
    Cancelled,
}

/// A tab's last results (Karar 12), and the jobs to check them for when they show again.
struct Kept {
    spec: SearchSpec,
    results: Arc<ResultSet>,
    status: String,
    changes: Vec<JobChange>,
}

/// The results on screen, and the jobs to check them for once they are.
struct Showing {
    tab: Option<u64>,
    spec: SearchSpec,
    status: String,
    complete: bool,
    changes: Vec<JobChange>,
}

/// Which results a tab shows: its tab and its search (a job's check is for these).
pub type ResultsKey = (Option<u64>, SearchSpec);

/// A job's effects to check results for (spec 4.7).
#[derive(Debug, Clone)]
pub struct JobChange {
    pub dirs: Vec<PathBuf>,
    pub paths: Vec<PathBuf>,
    /// A rename: new names take the places of the old.
    pub rename: bool,
    /// Started from these results: what it made shows whether the search would find it or not.
    pub own: bool,
}

/// Where a check's verdict goes.
#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    Apply,
    /// The tab keeps its results off screen: checked again when they show.
    Queue,
    Drop,
}

/// The results a check was for (`key`) against what is on screen and what its tab keeps.
pub fn verdict_for(key: &ResultsKey, showing: Option<&ResultsKey>, kept: Option<&SearchSpec>) -> Verdict {
    if showing == Some(key) {
        Verdict::Apply
    } else if kept == Some(&key.1) {
        Verdict::Queue
    } else {
        Verdict::Drop
    }
}

/// Whether a job that changed `dirs` and made `paths` may change results under `scope`
/// (`None`: every drive).
pub fn touches(scope: Option<&Path>, dirs: &[PathBuf], paths: &[PathBuf]) -> bool {
    use gezik_core::ops::paths::is_within;
    let Some(root) = scope else { return true };
    dirs.iter().chain(paths).any(|path| is_within(path, root) || is_within(root, path))
}

struct Run {
    handle: Running,
}

struct Inner {
    window: slint::Weak<AppWindow>,
    nav: Navigator,
    view: View,
    dialogs: Dialogs,
    settings: RefCell<SearchSettings>,
    /// The search the bar holds (its texts are the window's fields).
    draft: RefCell<SearchSpec>,
    /// The search the bar last went to: its location coming back keeps what was typed since.
    sent: RefCell<Option<SearchSpec>>,
    /// The folder the bar opened on ("This folder").
    origin: RefCell<Option<PathBuf>>,
    /// The location the bar last followed: the chrome updates often, the bar only on a move.
    followed: RefCell<Option<Location>>,
    /// The texts and options the bar's errors were worked out for, and the errors (the
    /// matchers are compiled once per change, not once per use).
    checked: RefCell<Option<(Checked, String, String)>>,
    /// The next load of the search on screen runs it anew (Enter on the same search), not
    /// from the tab's kept results; the name cache stays (only F5 reads it again).
    fresh: Cell<bool>,
    /// The location is being replaced by live typing: the results left are not kept.
    replacing: Cell<bool>,
    open: Cell<bool>,
    content_open: Cell<bool>,
    running: RefCell<Option<Run>>,
    /// Bumped by each search started and by leaving one: older events are dropped.
    generation: Cell<u64>,
    /// The search a load showed fresh results for: it starts once they are on screen.
    pending: RefCell<Option<SearchSpec>>,
    showing: RefCell<Option<Showing>>,
    kept: RefCell<HashMap<u64, Kept>>,
    names: RefCell<Names>,
    /// A key was typed and the results follow once the cache is ready.
    live_waiting: Cell<bool>,
    typing: slint::Timer,
    idle: slint::Timer,
    scopes: RefCell<Vec<Scope>>,
    problems: RefCell<Vec<(PathBuf, String)>>,
}

#[derive(Clone)]
pub struct Searches(Rc<Inner>);

impl Searches {
    pub fn new(window: &AppWindow, nav: Navigator, view: View, dialogs: Dialogs) -> Searches {
        let searches = Searches(Rc::new(Inner {
            window: window.as_weak(),
            nav,
            view,
            dialogs,
            settings: RefCell::new(SearchSettings::default()),
            draft: RefCell::new(SearchSpec::new(Scope::AllDrives)),
            sent: RefCell::new(None),
            origin: RefCell::new(None),
            followed: RefCell::new(None),
            checked: RefCell::new(None),
            fresh: Cell::new(false),
            replacing: Cell::new(false),
            open: Cell::new(false),
            content_open: Cell::new(false),
            running: RefCell::new(None),
            generation: Cell::new(0),
            pending: RefCell::new(None),
            showing: RefCell::new(None),
            kept: RefCell::new(HashMap::new()),
            names: RefCell::new(Names::None),
            live_waiting: Cell::new(false),
            typing: slint::Timer::default(),
            idle: slint::Timer::default(),
            scopes: RefCell::new(Vec::new()),
            problems: RefCell::new(Vec::new()),
        }));
        window.on_search_edited(|text| with_current(|s| s.edited(&text)));
        window.on_search_content_edited(|text| with_current(|s| s.content_edited(&text)));
        window.on_search_content_toggle(|| with_current(Searches::content_toggle));
        window.on_search_go(|| with_current(Searches::button));
        window.on_filter_search(|| with_current(Searches::filter_to_search));
        CURRENT.with(|c| *c.borrow_mut() = Some(searches.clone()));
        searches
    }

    /// `[search]` (every resolve).
    pub fn set_settings(&self, settings: SearchSettings) {
        *self.0.settings.borrow_mut() = settings;
    }

    fn view_shown() -> (bool, bool) {
        let options = crate::view_options::current();
        (options.show_hidden, options.show_system)
    }

    fn cache_key(&self, spec: &SearchSpec) -> CacheKey {
        CacheKey {
            scope: spec.scope.clone(),
            shown: (spec.hidden == HiddenRule::FollowView).then(Searches::view_shown),
            skip: if spec.skipped { Vec::new() } else { self.0.settings.borrow().skip.clone() },
        }
    }

    // ---- The bar ----

    /// `search` (Ctrl+Shift+F, F3): the bar on the folder shown (This PC: every drive); on an
    /// open bar, its name field with the text selected.
    pub fn open(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        if self.0.open.get() {
            window.invoke_select_search_text();
            return;
        }
        let location = self.0.nav.active_location();
        let spec = match &location {
            Location::Search(spec) => (**spec).clone(),
            Location::Path(folder) | Location::Flat(folder) => SearchSpec::new(Scope::Folder(folder.clone())),
            Location::Drives => SearchSpec::new(Scope::AllDrives),
        };
        self.show_bar(spec, location.folder().map(Path::to_path_buf));
        self.focus_later();
        self.warm();
    }

    /// "Search in this folder…": the bar on `folder`, empty.
    pub fn open_in(&self, folder: PathBuf) {
        self.show_bar(SearchSpec::new(Scope::Folder(folder.clone())), Some(folder));
        self.focus_later();
        self.warm();
    }

    fn show_bar(&self, spec: SearchSpec, origin: Option<PathBuf>) {
        self.0.content_open.set(!spec.content.is_empty());
        if let Some(window) = self.0.window.upgrade() {
            window.set_search_content_focus(false);
        }
        *self.0.draft.borrow_mut() = spec;
        *self.0.origin.borrow_mut() = origin;
        self.0.open.set(true);
        self.sync_bar();
    }

    fn close_bar(&self) {
        self.0.open.set(false);
        self.0.typing.stop();
        self.0.live_waiting.set(false);
        if matches!(&*self.0.names.borrow(), Names::Building(..)) {
            self.drop_names();
        }
        self.sync_bar();
        self.release_cache_if_unused();
    }

    /// The name cache goes with the bar and the last results tab (spec 3.5).
    fn release_cache_if_unused(&self) {
        let nav = &self.0.nav;
        let results = (0..nav.tab_count()).any(|i| nav.tab_location(i).is_some_and(|l| l.is_results()));
        if !self.0.open.get() && !results && !matches!(&*self.0.names.borrow(), Names::None) {
            self.drop_names();
        }
    }

    /// The bar as the draft is.
    fn sync_bar(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        let draft = self.0.draft.borrow();
        window.set_search_open(self.0.open.get());
        window.set_search_scope(scope_label(&draft.scope).into());
        if window.get_search_text().as_str() != draft.pattern {
            window.set_search_text(draft.pattern.as_str().into());
        }
        if window.get_search_content().as_str() != draft.content {
            window.set_search_content(draft.content.as_str().into());
        }
        window.set_search_content_open(self.0.content_open.get());
        window.set_search_filters(match draft.filter_count() {
            0 => "Filters".into(),
            n => format!("Filters ({n})").into(),
        });
        window.set_search_running(self.0.running.borrow().is_some());
        let checked: Checked =
            (draft.pattern.clone(), draft.name_regex, draft.content.clone(), draft.content_regex, draft.match_case);
        let mut last = self.0.checked.borrow_mut();
        if last.as_ref().is_none_or(|(c, _, _)| *c != checked) {
            let name_error = NameMatcher::compile(&draft.pattern, draft.name_regex, draft.match_case).err();
            let content_error = (!draft.content.is_empty())
                .then(|| ContentMatcher::compile(&draft.content, draft.content_regex, draft.match_case, 0).err())
                .flatten();
            *last = Some((checked, name_error.unwrap_or_default(), content_error.unwrap_or_default()));
        }
        if let Some((_, name_error, content_error)) = last.as_ref() {
            window.set_search_error(name_error.as_str().into());
            window.set_search_content_error(content_error.as_str().into());
        }
    }

    /// Whether the name field's text can be used (`sync_bar` worked it out).
    fn name_ok(&self) -> bool {
        self.0.checked.borrow().as_ref().is_none_or(|(_, name_error, _)| name_error.is_empty())
    }

    fn focus_later(&self) {
        let weak = self.0.window.clone();
        slint::Timer::single_shot(Duration::ZERO, move || {
            if let Some(window) = weak.upgrade()
                && window.get_search_open()
            {
                window.invoke_select_search_text();
            }
        });
    }

    fn edited(&self, text: &str) {
        self.0.draft.borrow_mut().pattern = text.to_owned();
        self.sync_bar();
        self.follow_typing();
    }

    fn content_edited(&self, text: &str) {
        self.0.draft.borrow_mut().content = text.to_owned();
        self.sync_bar();
    }

    fn content_toggle(&self) {
        let open = !self.0.content_open.get();
        self.0.content_open.set(open);
        if !open {
            self.0.draft.borrow_mut().content.clear();
        }
        self.sync_bar();
        if let Some(window) = self.0.window.upgrade() {
            if open {
                // The field is made as Content opens and takes the keyboard then.
                window.set_search_content_focus(true);
            } else {
                window.invoke_select_search_text();
            }
        }
    }

    /// Names, sizes, dates and types follow the typing (150 ms) when the cache or Everything
    /// can answer at once; text in files and a scope too large for the cache wait for Enter.
    fn follow_typing(&self) {
        let draft = self.0.draft.borrow().clone();
        if !draft.content.is_empty() || !draft.is_query() {
            return;
        }
        if !self.name_ok() {
            return;
        }
        let key = self.cache_key(&draft);
        let step = live_step(&self.0.names.borrow(), &key);
        match step {
            Live::Now => {
                self.0.typing.start(slint::TimerMode::SingleShot, TYPING, || with_current(|s| s.run(false, true)))
            }
            Live::Wait => self.0.live_waiting.set(true),
            Live::Large => self.0.view.note("Large folder: press Enter to search".to_owned()),
            Live::Never => {}
            Live::Warm => {
                self.0.live_waiting.set(true);
                self.warm();
            }
        }
    }

    /// Enter in a field; Alt+Enter: in a new tab (Karar 2).
    pub fn go(&self, new_tab: bool) {
        self.0.typing.stop();
        self.run(new_tab, false);
    }

    /// Search / Stop.
    fn button(&self) {
        if self.0.running.borrow().is_some() {
            self.stop();
        } else {
            self.go(false);
        }
    }

    fn run(&self, new_tab: bool, live: bool) {
        let spec = self.0.draft.borrow().clone();
        if !spec.is_query() {
            return self.0.view.note("Type something to search".to_owned());
        }
        if let Some(window) = self.0.window.upgrade()
            && (!window.get_search_error().is_empty() || !window.get_search_content_error().is_empty())
        {
            return;
        }
        *self.0.sent.borrow_mut() = Some(spec.clone());
        let location = Location::Search(Box::new(spec));
        let current = self.0.nav.active_location();
        if new_tab {
            self.0.nav.open_tab(location, true);
        } else if current == location {
            // The same search again: run anew (spec 4.7), from the name cache as it is.
            self.0.fresh.set(true);
            self.0.nav.show_again();
        } else if live && matches!(current, Location::Search(_)) {
            self.0.replacing.set(true);
            self.0.nav.replace_location(location);
        } else {
            self.0.nav.go(location);
        }
    }

    /// Stop: the walk ends; its results so far stay, the status bar says so.
    pub fn stop(&self) {
        if let Some(run) = self.0.running.borrow().as_ref() {
            run.handle.cancel();
        }
    }

    /// Esc in the bar: stops a running search, else closes the bar (the results stay) and
    /// gives the list the keyboard.
    pub fn escape(&self) {
        if self.0.running.borrow().is_some() {
            return self.stop();
        }
        self.close_bar();
        if let Some(window) = self.0.window.upgrade() {
            window.invoke_focus_list();
        }
    }

    /// The filter bar's Shift+Enter or "Search subfolders" (spec 4.1): its text as the name.
    pub fn filter_to_search(&self) {
        let Some(pattern) = self.0.view.filter_text().filter(|t| !t.trim().is_empty()) else { return };
        let location = self.0.nav.active_location();
        let Some(folder) = location.folder().map(Path::to_path_buf) else { return };
        crate::filter::with_current(crate::filter::Filter::close);
        let mut spec = SearchSpec::new(Scope::Folder(folder.clone()));
        spec.pattern = pattern;
        self.show_bar(spec, Some(folder));
        self.go(false);
    }

    /// `flat-view` (Ctrl+B, spec 5): every file under the folder; again, back to the folder
    /// with the focused file selected in its own folder.
    pub fn flat_view(&self) {
        match self.0.nav.active_location() {
            Location::Flat(folder) => {
                let focused = self.0.view.focus().and_then(|i| self.0.view.entry_path(i));
                match focused {
                    Some((path, false)) => match (path.parent(), path.file_name()) {
                        (Some(parent), Some(name)) => {
                            self.0.nav.go_selecting(parent.to_path_buf(), vec![name.to_string_lossy().into_owned()])
                        }
                        _ => self.0.nav.go(Location::Path(folder)),
                    },
                    _ => self.0.nav.go(Location::Path(folder)),
                }
            }
            location => match location.folder() {
                Some(folder) => self.0.nav.go(Location::Flat(folder.to_path_buf())),
                None => self.0.view.note("Flat view works in folders".to_owned()),
            },
        }
    }

    // ---- Loading a search ----

    /// The listing a load of `location` shows (the navigator, for `LoadResult::Results`): the
    /// tab's last results if they are this search's and whole, else empty ones (the search
    /// starts in `shown`).
    pub fn listing_for(&self, location: &Location, tab: Option<u64>) -> Listing {
        let Some(spec) = spec_of(location) else { return Listing::default() };
        let fresh = self.0.fresh.replace(false);
        let kept = tab.and_then(|tab| {
            let mut kept = self.0.kept.borrow_mut();
            let usable = !fresh && kept.get(&tab).is_some_and(|k| k.spec == spec);
            if usable || fresh { kept.remove(&tab).filter(|_| usable) } else { None }
        });
        if let Some(kept) = kept {
            *self.0.pending.borrow_mut() = None;
            *self.0.showing.borrow_mut() =
                Some(Showing { tab, spec: kept.spec, status: kept.status, complete: true, changes: kept.changes });
            return Listing::Results(kept.results);
        }
        let root = spec.scope.folder().map(Path::to_path_buf).unwrap_or_default();
        let content = !spec.content.is_empty();
        *self.0.pending.borrow_mut() = Some(spec.clone());
        *self.0.showing.borrow_mut() =
            Some(Showing { tab, spec, status: String::new(), complete: false, changes: Vec::new() });
        Listing::Results(Arc::new(ResultSet::new(root, content)))
    }

    /// The results `listing_for` gave are on screen: a fresh search starts; kept ones get their
    /// status back.
    pub fn shown(&self) {
        let pending = self.0.pending.borrow_mut().take();
        match pending {
            Some(spec) => self.start(spec),
            None => {
                let (status, changes) = match self.0.showing.borrow_mut().as_mut() {
                    Some(s) => (Some(s.status.clone()), std::mem::take(&mut s.changes)),
                    None => (None, Vec::new()),
                };
                self.0.view.set_results_status(status.filter(|s| !s.is_empty()));
                // Jobs that ended while these results were kept (spec 4.7).
                if let Some(key) = self.results_key() {
                    for change in changes {
                        self.check(key.clone(), change);
                    }
                }
            }
        }
    }

    /// The navigator leaves what is on screen (another place, another tab, a reload): a
    /// running search stops (its later events are dropped); whole results stay with their tab.
    pub fn leaving(&self) {
        let run = self.0.running.borrow_mut().take();
        if let Some(run) = run {
            run.handle.cancel();
            self.0.generation.set(self.0.generation.get() + 1);
            self.0.view.set_searching(false);
            if let Some(window) = self.0.window.upgrade() {
                window.set_search_running(false);
            }
        }
        let replacing = self.0.replacing.replace(false);
        let Some(showing) = self.0.showing.borrow_mut().take() else { return };
        if replacing {
            // Typing refines the search in place: what it showed before is not coming back.
            if let Some(tab) = showing.tab {
                self.0.kept.borrow_mut().remove(&tab);
            }
            return;
        }
        if let (Some(tab), true, Some(results)) = (showing.tab, showing.complete, self.0.view.results()) {
            let kept = Kept { spec: showing.spec, results, status: showing.status, changes: showing.changes };
            self.0.kept.borrow_mut().insert(tab, kept);
        }
    }

    /// The results on screen, if any.
    pub fn results_key(&self) -> Option<ResultsKey> {
        if !self.0.view.shows_results() {
            return None;
        }
        self.0.showing.borrow().as_ref().map(|s| (s.tab, s.spec.clone()))
    }

    /// A job ended (spec 4.7): the results on screen are checked now, a tab's kept results it
    /// may have changed when they show again. `origin`: the results it was started from.
    pub fn job_done(&self, origin: Option<&ResultsKey>, dirs: Vec<PathBuf>, paths: Vec<PathBuf>, rename: bool) {
        let change =
            |key: ResultsKey| JobChange { dirs: dirs.clone(), paths: paths.clone(), rename, own: origin == Some(&key) };
        for (tab, kept) in self.0.kept.borrow_mut().iter_mut() {
            if touches(kept.spec.scope.folder(), &dirs, &paths) {
                kept.changes.push(change((Some(*tab), kept.spec.clone())));
            }
        }
        if let Some(key) = self.results_key() {
            self.check(key.clone(), change(key));
        }
    }

    /// Checks on another thread what `change` did to the results on screen (`key`), then
    /// shows it if they still are, or keeps it for their tab.
    fn check(&self, key: ResultsKey, change: JobChange) {
        let Some(probe) = self.0.view.results_probe(&change.dirs, &change.paths) else { return };
        let (max_size, max_results) = {
            let settings = self.0.settings.borrow();
            (settings.content_max_size, settings.max_results)
        };
        let weak = self.0.window.clone();
        let spawned = std::thread::Builder::new().name("gezik-results-check".into()).spawn(move || {
            let (gone, mut added) = probe.verify();
            if !change.own && !added.is_empty() {
                // Not made from these results: only what this search would find.
                match Query::compile(&key.1, &QueryOptions::local(max_size, max_results)) {
                    Ok(query) => added.retain(|(path, e)| {
                        query.passes(&e.name, e.is_dir, e.size, e.modified)
                            && query.content().is_none_or(|content| {
                                content.reads(&e.name, e.size)
                                    && content
                                        .find_in_file(path, &AtomicBool::new(false), &gezik_platform::decode_ansi)
                                        .is_ok_and(|found| found.is_some())
                            })
                    }),
                    Err(_) => added.clear(),
                }
            }
            let _ = weak.upgrade_in_event_loop(move |_| {
                with_current(|searches| searches.checked(key, change, gone, added));
            });
        });
        if spawned.is_err() {
            eprintln!("gezik: cannot check the search results");
        }
    }

    fn checked(
        &self,
        key: ResultsKey,
        change: JobChange,
        gone: Vec<PathBuf>,
        added: Vec<(PathBuf, gezik_core::Entry)>,
    ) {
        let showing = self.results_key();
        let kept = key.0.and_then(|tab| self.0.kept.borrow().get(&tab).map(|k| k.spec.clone()));
        match verdict_for(&key, showing.as_ref(), kept.as_ref()) {
            Verdict::Apply => self.0.view.results_changed(&gone, added, change.rename),
            Verdict::Queue => {
                let mut kept = self.0.kept.borrow_mut();
                if let Some(kept) = key.0.and_then(|tab| kept.get_mut(&tab)) {
                    kept.changes.push(change);
                }
            }
            Verdict::Drop => {}
        }
    }

    /// F5 on results (spec 4.7): the tab's results and the name cache are forgotten.
    pub fn forget(&self, tab: Option<u64>) {
        if let Some(tab) = tab {
            self.0.kept.borrow_mut().remove(&tab);
        }
        self.drop_names();
        if self.0.open.get() {
            self.warm();
        }
    }

    fn drop_names(&self) {
        if let Names::Building(_, cancel) = &*self.0.names.borrow() {
            cancel.store(true, Ordering::SeqCst);
        }
        *self.0.names.borrow_mut() = Names::None;
        self.0.idle.stop();
    }

    /// A tab, its location or the tabs changed (`Navigator::on_changed`): the bar follows a
    /// search's location; it closes on a folder other than its own; closed tabs' results go.
    pub fn location_changed(&self, location: &Location) {
        let nav = &self.0.nav;
        let alive: Vec<u64> = (0..nav.tab_count()).filter_map(|i| nav.tab_id(i)).collect();
        self.0.kept.borrow_mut().retain(|tab, _| alive.contains(tab));
        self.release_cache_if_unused();
        // The same place again (a tab renamed, locked, a reload): a bar closed with Esc stays closed.
        if self.0.followed.borrow().as_ref() == Some(location) {
            return;
        }
        *self.0.followed.borrow_mut() = Some(location.clone());
        match location {
            Location::Search(spec) => {
                // The search the bar went to: what was typed since stays in it (once: Back and
                // Forward later show their own search).
                let own = self.0.sent.borrow().as_ref() == Some(&**spec);
                if own {
                    self.0.sent.borrow_mut().take();
                }
                if !self.0.open.get() || (!own && *self.0.draft.borrow() != **spec) {
                    self.show_bar((**spec).clone(), spec.scope.folder().map(Path::to_path_buf));
                    self.warm();
                }
            }
            Location::Flat(_) => {
                if self.0.open.get() {
                    self.close_bar();
                }
            }
            other => {
                let elsewhere = self.0.origin.borrow().as_deref() != other.folder();
                if self.0.open.get() && elsewhere {
                    self.close_bar();
                }
            }
        }
    }

    // ---- Running ----

    fn start(&self, spec: SearchSpec) {
        let old = self.0.running.borrow_mut().take();
        if let Some(old) = old {
            old.handle.cancel();
        }
        let generation = self.0.generation.get() + 1;
        self.0.generation.set(generation);
        if !spec.is_query() {
            return self.0.view.set_results_status(Some("Type something to search".to_owned()));
        }
        let settings = self.0.settings.borrow().clone();
        let options = QueryOptions::local(settings.content_max_size, settings.max_results);
        let query = match Query::compile(&spec, &options) {
            Ok(query) => query,
            Err(QueryError::Name(why) | QueryError::Content(why)) => {
                return self.0.view.set_results_status(Some(why));
            }
        };
        self.0.view.set_searching(true);
        self.0.view.set_results_status(Some(progress_text(0, 0)));
        let running = Running::default();
        *self.0.running.borrow_mut() = Some(Run { handle: running.clone() });
        if let Some(window) = self.0.window.upgrade() {
            window.set_search_running(true);
        }
        let sink = self.sink(generation);
        let started = Instant::now();
        let cache = spec.content.is_empty().then(|| self.ready_cache(&spec)).flatten();
        let flag = running.flag();
        let (shown, skip, on) = (Searches::view_shown(), settings.skip.clone(), settings.everything);
        let spawned = std::thread::Builder::new().name("gezik-search-start".into()).spawn(move || {
            if let Some(cache) = cache {
                match cache.select(&query, &flag) {
                    Some((set, full)) => send_whole(set, full, started, false, &sink),
                    None => sink(Event::Done(Summary { cancelled: true, ..Summary::default() })),
                }
                return;
            }
            let walk = plan_walk(&spec, &skip, shown);
            match gezik_search::everything::search(&spec, &walk, &query, on, &flag, &sink) {
                Ok(()) => {}
                Err(gezik_search::everything::Fallback::Cancelled) => {
                    sink(Event::Done(Summary { cancelled: true, ..Summary::default() }))
                }
                Err(_) => start_with(walk, query, running, sink),
            }
        });
        if spawned.is_err() {
            self.0.running.borrow_mut().take();
            self.0.view.set_searching(false);
            self.0.view.set_results_status(Some("Cannot start the search".to_owned()));
            if let Some(window) = self.0.window.upgrade() {
                window.set_search_running(false);
            }
        }
    }

    /// Events of search `generation`, brought to the UI thread.
    fn sink(&self, generation: u64) -> impl Fn(Event) + Send + Sync + Clone + 'static {
        let weak = self.0.window.clone();
        move |event| {
            let _ = weak.upgrade_in_event_loop(move |_| with_current(|s| s.event(generation, event)));
        }
    }

    fn event(&self, generation: u64, event: Event) {
        if generation != self.0.generation.get() {
            return;
        }
        match event {
            Event::Batch(batch) => self.0.view.append_results(batch),
            Event::Progress { found, folders } => self.0.view.set_results_status(Some(progress_text(found, folders))),
            Event::Done(summary) => {
                let text = done_text(&summary);
                *self.0.problems.borrow_mut() = summary.problems.first.clone();
                self.0.running.borrow_mut().take();
                self.0.view.set_searching(false);
                self.0.view.set_results_status(Some(text.clone()));
                if let Some(showing) = self.0.showing.borrow_mut().as_mut() {
                    showing.status = text;
                    showing.complete = !summary.cancelled;
                }
                if let Some(window) = self.0.window.upgrade() {
                    window.set_search_running(false);
                }
            }
        }
    }

    // ---- The name cache ----

    /// The cache for `spec`, if it is read and holds what `spec` asks for; its idle time starts over.
    fn ready_cache(&self, spec: &SearchSpec) -> Option<Arc<NameCache>> {
        let key = self.cache_key(spec);
        let cache = match &*self.0.names.borrow() {
            Names::Ready(k, cache) if *k == key => Some(cache.clone()),
            _ => None,
        };
        if cache.is_some() {
            self.0.idle.start(slint::TimerMode::SingleShot, CACHE_IDLE, || with_current(Searches::drop_names));
        }
        cache
    }

    /// Reads the draft's scope into the cache in the background, unless that is under way or
    /// done; Everything or a network folder keeps none (spec 3.5).
    fn warm(&self) {
        let spec = self.0.draft.borrow().clone();
        let key = self.cache_key(&spec);
        let same = match &*self.0.names.borrow() {
            Names::Building(k, _) | Names::Ready(k, _) | Names::TooLarge(k) | Names::NoCache(k, _) => *k == key,
            Names::None => false,
        };
        if same {
            return;
        }
        self.drop_names();
        let cancel = Arc::new(AtomicBool::new(false));
        *self.0.names.borrow_mut() = Names::Building(key.clone(), cancel.clone());
        let (skip, on) = {
            let settings = self.0.settings.borrow();
            (settings.skip.clone(), settings.everything)
        };
        let shown = Searches::view_shown();
        let weak = self.0.window.clone();
        let own = cancel.clone();
        let spawned = std::thread::Builder::new().name("gezik-search-cache".into()).spawn(move || {
            gezik_platform::priority::lower_this_thread();
            let walk = plan_walk(&spec, &skip, shown);
            let warmed = if gezik_search::everything::usable(&walk.roots, on) {
                Warmed::Everything
            } else if walk.roots.iter().any(|root| gezik_platform::fs::is_network(root).unwrap_or(false)) {
                Warmed::Network
            } else {
                match gezik_search::cache::build(walk, cancel, CACHE_LIMIT).0 {
                    CacheOutcome::Ready(cache) => Warmed::Ready(cache),
                    CacheOutcome::TooLarge => Warmed::TooLarge,
                    CacheOutcome::Cancelled => Warmed::Cancelled,
                }
            };
            let _ = weak.upgrade_in_event_loop(move |_| with_current(|s| s.warmed(key, &own, warmed)));
        });
        if spawned.is_err() {
            *self.0.names.borrow_mut() = Names::None;
        }
    }

    fn warmed(&self, key: CacheKey, cancel: &Arc<AtomicBool>, warmed: Warmed) {
        if !this_build(&self.0.names.borrow(), cancel) {
            return;
        }
        let names = match warmed {
            Warmed::Ready(cache) => Names::Ready(key, cache),
            Warmed::TooLarge => Names::TooLarge(key),
            Warmed::Everything => Names::NoCache(key, true),
            Warmed::Network => Names::NoCache(key, false),
            Warmed::Cancelled => Names::None,
        };
        let ready = matches!(names, Names::Ready(..) | Names::NoCache(_, true));
        *self.0.names.borrow_mut() = names;
        if ready {
            self.0.idle.start(slint::TimerMode::SingleShot, CACHE_IDLE, || with_current(Searches::drop_names));
            if self.0.live_waiting.replace(false) && self.0.open.get() {
                self.follow_typing();
            }
        }
    }

    // ---- Menus ----

    /// The items of one of the bar's menus (the scope menu's places are kept for its answer).
    pub fn menu(&self, which: SearchMenu) -> (Vec<(u32, String, bool)>, Vec<Submenu>) {
        let draft = self.0.draft.borrow().clone();
        match which {
            SearchMenu::Scope => {
                let choices = scope_choices(self.0.origin.borrow().as_deref());
                let items = ids::search_scope_items(&choices, &draft.scope);
                *self.0.scopes.borrow_mut() = choices.into_iter().map(|(scope, _)| scope).collect();
                (items, Vec::new())
            }
            SearchMenu::Filters => ids::search_filter_items(&draft),
            SearchMenu::More => (ids::search_more_items(self.0.problems.borrow().len()), Vec::new()),
        }
    }

    /// A search menu item; a criterion changed on results already shown runs the search again.
    pub fn menu_chosen(&self, id: u32) {
        match id {
            ids::SEARCH_NEW_TAB => return self.go(true),
            ids::SEARCH_PROBLEMS => return self.show_problems(),
            ids::SIZE_MIN | ids::SIZE_MAX | ids::MODIFIED_BETWEEN => return self.ask_criterion(id),
            _ => {}
        }
        let mut rerun = true;
        {
            let mut draft = self.0.draft.borrow_mut();
            match id {
                ids::SEARCH_CLEAR => {
                    let scope = draft.scope.clone();
                    *draft = SearchSpec::new(scope);
                    self.0.content_open.set(false);
                    rerun = false;
                }
                ids::FILTERS_CLEAR => {
                    let (scope, pattern, content) = (draft.scope.clone(), draft.pattern.clone(), draft.content.clone());
                    *draft = SearchSpec { pattern, content, ..SearchSpec::new(scope) };
                }
                id if (ids::SCOPE_FIRST..ids::SCOPE_FIRST + ids::SCOPE_MAX).contains(&id) => {
                    let Some(scope) = self.0.scopes.borrow().get((id - ids::SCOPE_FIRST) as usize).cloned() else {
                        return;
                    };
                    draft.scope = scope;
                }
                id if (ids::MODIFIED_FIRST..ids::SIZE_MIN).contains(&id) => {
                    if let Some(range) = ids::modified_for(id) {
                        draft.modified = range;
                    }
                }
                id if (ids::KIND_FIRST..ids::KIND_FIRST + KindFilter::ALL.len() as u32).contains(&id) => {
                    draft.kind = KindFilter::ALL[(id - ids::KIND_FIRST) as usize];
                }
                id if (ids::OPTION_FIRST..ids::OPTION_FIRST + 5).contains(&id) => match id - ids::OPTION_FIRST {
                    0 => draft.name_regex = !draft.name_regex,
                    1 => draft.content_regex = !draft.content_regex,
                    2 => draft.match_case = !draft.match_case,
                    3 => {
                        draft.hidden = match draft.hidden {
                            HiddenRule::FollowView => HiddenRule::Include,
                            HiddenRule::Include => HiddenRule::FollowView,
                        }
                    }
                    _ => draft.skipped = !draft.skipped,
                },
                _ => return,
            }
        }
        self.criteria_changed(rerun);
    }

    fn criteria_changed(&self, rerun: bool) {
        self.sync_bar();
        self.warm();
        // Only a change from the search on screen runs it again (a criterion chosen twice does not).
        let changed = match self.0.nav.active_location() {
            Location::Search(shown) => *shown != *self.0.draft.borrow(),
            _ => false,
        };
        if rerun && changed && self.0.draft.borrow().is_query() {
            self.go(false);
        }
    }

    /// Size at least / at most and Modified between, asked in the question box with a note
    /// under the field (sapma 2).
    fn ask_criterion(&self, id: u32) {
        let draft = self.0.draft.borrow().clone();
        let between = id == ids::MODIFIED_BETWEEN;
        let (title, message, initial) = match id {
            ids::SIZE_MIN => {
                ("Size at least", "Smallest size, like 500 MB (empty: any):", draft.size.min.map(size_text))
            }
            ids::SIZE_MAX => ("Size at most", "Largest size, like 4 GB (empty: any):", draft.size.max.map(size_text)),
            _ => (
                "Modified between",
                "Two days, like 2026-01-01..2026-06-30:",
                matches!(draft.modified, DateRange::Between(..)).then(|| draft.modified.text()),
            ),
        };
        let check = move |text: &str| -> Result<(), String> {
            let text = text.trim();
            if text.is_empty() {
                return Ok(());
            }
            if between {
                match DateRange::parse(text)? {
                    DateRange::Between(..) => Ok(()),
                    _ => Err("Two days, like 2026-01-01..2026-06-30".to_owned()),
                }
            } else {
                parse_size(text).map(|_| ())
            }
        };
        let searches = self.clone();
        self.0.dialogs.ask_text_noted(
            title,
            message,
            initial.unwrap_or_default(),
            &["OK", "Cancel"],
            move |text| match check(text) {
                Ok(()) => (String::new(), false),
                Err(why) => (why, true),
            },
            move |text| {
                let Some(text) = text.map(|t| t.trim().to_owned()) else { return };
                if check(&text).is_err() {
                    return;
                }
                {
                    let mut draft = searches.0.draft.borrow_mut();
                    match id {
                        ids::SIZE_MIN => draft.size.min = parse_size(&text).ok(),
                        ids::SIZE_MAX => draft.size.max = parse_size(&text).ok(),
                        _ => draft.modified = DateRange::parse(&text).unwrap_or(DateRange::Any),
                    }
                }
                searches.criteria_changed(true);
            },
        );
    }

    /// The folders the last search could not read, with why (the first 50).
    fn show_problems(&self) {
        let lines: Vec<String> =
            self.0.problems.borrow().iter().map(|(path, why)| format!("{}: {why}", path.display())).collect();
        self.0.dialogs.ask("Folders that could not be read", lines.join("\n"), &["Close"], |_| {});
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gezik_search::walk::Problems;

    #[test]
    fn the_status_bar_says_how_far_and_how_it_ended() {
        assert_eq!(progress_text(12_345, 48_210), "Searching… 12,345 found · 48,210 folders");
        assert_eq!(progress_text(3, 0), "Searching… 3 found");
        let mut done = Summary { found: 1234, elapsed: Duration::from_millis(2400), ..Summary::default() };
        assert_eq!(done_text(&done), "1,234 results in 2.4 s");
        done.everything = true;
        done.skipped = 14;
        done.problems = Problems { count: 3, first: Vec::new() };
        assert_eq!(
            done_text(&done),
            "1,234 results in 2.4 s via Everything · Skipped 14 folders (search.skip) · 3 folders could not be read"
        );
        let limit = Summary { found: 250_000, limit_reached: true, ..Summary::default() };
        assert_eq!(done_text(&limit), "Stopped at 250,000 results. Narrow the search.");
        let stopped = Summary { found: 1, cancelled: true, ..Summary::default() };
        assert_eq!(done_text(&stopped), "Stopped · 1 result");
        let one = Summary { found: 1, skipped: 1, elapsed: Duration::from_millis(50), ..Summary::default() };
        assert_eq!(done_text(&one), "1 result in 0.1 s · Skipped 1 folder (search.skip)");
    }

    #[test]
    fn the_scope_menu_climbs_to_the_drive_and_this_pc() {
        let folder = std::env::temp_dir().join("a").join("b");
        let choices = scope_choices(Some(&folder));
        assert_eq!(choices[0], (Scope::Folder(folder.clone()), "This folder (b)".to_owned()));
        assert_eq!(choices[1].0, Scope::Folder(folder.parent().unwrap().to_path_buf()));
        let root = folder.ancestors().last().unwrap().to_path_buf();
        let drive = choices.iter().rev().nth(1).unwrap();
        assert_eq!(drive.0, Scope::Folder(root));
        assert!(drive.1.starts_with("Whole drive ("), "{}", drive.1);
        assert_eq!(choices.last().unwrap(), &(Scope::AllDrives, gezik_core::nav::DRIVES_NAME.to_owned()));
        let parents = choices.len() - 3;
        assert!(parents <= 5, "at most five folders above");
        assert_eq!(scope_choices(None), [(Scope::AllDrives, gezik_core::nav::DRIVES_NAME.to_owned())]);
        assert_eq!(scope_label(&Scope::Folder(folder)), "in b");
        assert_eq!(scope_label(&Scope::AllDrives), format!("in {}", gezik_core::nav::DRIVES_NAME));
    }

    #[test]
    fn a_location_carries_its_search() {
        let mut spec = SearchSpec::new(Scope::Folder("/w".into()));
        spec.pattern = "x".into();
        assert_eq!(spec_of(&Location::Search(Box::new(spec.clone()))), Some(spec));
        assert_eq!(spec_of(&Location::Flat("/w".into())), Some(SearchSpec::flat_view("/w".into())));
        assert_eq!(spec_of(&Location::Path("/w".into())), None);
    }

    fn key(folder: &str) -> CacheKey {
        CacheKey { scope: Scope::Folder(folder.into()), shown: None, skip: Vec::new() }
    }

    #[test]
    fn typing_reads_the_cache_again_when_it_is_gone_or_for_other_rules() {
        let here = key("/w");
        assert_eq!(live_step(&Names::None, &here), Live::Warm, "dropped when idle: read again");
        let cache = Arc::new(NameCache::default());
        assert_eq!(live_step(&Names::Ready(here.clone(), cache.clone()), &here), Live::Now);
        assert_eq!(live_step(&Names::Ready(key("/x"), cache), &here), Live::Warm, "another scope");
        let hidden = CacheKey { shown: Some((true, true)), ..here.clone() };
        assert_eq!(live_step(&Names::Ready(hidden.clone(), Arc::default()), &here), Live::Warm, "other rules");
        let flag = Arc::new(AtomicBool::new(false));
        assert_eq!(live_step(&Names::Building(here.clone(), flag.clone()), &here), Live::Wait);
        assert_eq!(live_step(&Names::Building(hidden, flag), &here), Live::Warm);
        assert_eq!(live_step(&Names::TooLarge(here.clone()), &here), Live::Large);
        assert_eq!(live_step(&Names::NoCache(here.clone(), true), &here), Live::Now, "Everything");
        assert_eq!(live_step(&Names::NoCache(here.clone(), false), &here), Live::Never, "network");
    }

    #[test]
    fn only_the_newest_read_of_a_scope_ends_it() {
        let (older, newer) = (Arc::new(AtomicBool::new(true)), Arc::new(AtomicBool::new(false)));
        let names = Names::Building(key("/w"), newer.clone());
        assert!(!this_build(&names, &older), "a cancelled read of the same scope answers late");
        assert!(this_build(&names, &newer));
        assert!(!this_build(&Names::None, &newer));
    }

    #[test]
    fn a_check_lands_only_on_the_results_it_was_for() {
        let spec = SearchSpec::new(Scope::Folder("/w".into()));
        let other = SearchSpec::new(Scope::Folder("/v".into()));
        let key = (Some(1), spec.clone());
        assert_eq!(verdict_for(&key, Some(&key), None), Verdict::Apply);
        assert_eq!(verdict_for(&key, Some(&(Some(2), spec.clone())), Some(&spec)), Verdict::Queue, "another tab shows");
        assert_eq!(verdict_for(&key, None, Some(&spec)), Verdict::Queue, "a folder shows in its tab");
        assert_eq!(verdict_for(&key, Some(&(Some(1), other.clone())), Some(&other)), Verdict::Drop, "another search");
        assert_eq!(verdict_for(&key, None, None), Verdict::Drop, "nothing kept");
        let dirs = [PathBuf::from("/w/a")];
        assert!(touches(Some(Path::new("/w")), &dirs, &[]));
        assert!(touches(Some(Path::new("/w/a/b")), &dirs, &[]), "the scope is inside the changed folder");
        assert!(!touches(Some(Path::new("/v")), &dirs, &[PathBuf::from("/x/y")]));
        assert!(touches(Some(Path::new("/v")), &[], &[PathBuf::from("/v/new")]));
        assert!(touches(None, &dirs, &[]), "every drive");
    }

    #[test]
    fn the_bar_names_its_menus_by_number() {
        assert_eq!(SearchMenu::from_index(1), SearchMenu::Scope);
        assert_eq!(SearchMenu::from_index(2), SearchMenu::Filters);
        assert_eq!(SearchMenu::from_index(0), SearchMenu::More);
    }

    #[test]
    fn deep_folders_offer_five_folders_above() {
        let mut folder = std::env::temp_dir();
        for part in ["1", "2", "3", "4", "5", "6", "7"] {
            folder.push(part);
        }
        let choices = scope_choices(Some(&folder));
        assert_eq!(choices.len(), 1 + 5 + 2, "this folder, five above, the drive, This PC");
    }
}
