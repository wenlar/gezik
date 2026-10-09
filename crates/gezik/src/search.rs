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
use gezik_search::walk::Walk;
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

/// Puts `len` rows in `summary`'s count; whether that changed it.
fn recounted(summary: &mut Summary, len: usize) -> bool {
    if summary.found == len || summary.limit_reached {
        return false;
    }
    summary.found = len;
    true
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
        Location::Path(_) | Location::Drives | Location::Trash => None,
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
    /// Everything answers here but not this name (it would walk at every pause): Enter searches.
    Enter,
    /// A network folder: Enter searches.
    Never,
    /// No cache for this scope and these rules (none yet, dropped when idle, or another key):
    /// read it, then follow.
    Warm,
}

/// `asks`: Everything can be asked for the name typed (`everything::translate`).
fn live_step(names: &Names, key: &CacheKey, asks: bool) -> Live {
    match names {
        Names::Ready(k, _) if k == key => Live::Now,
        Names::NoCache(k, true) if k == key && asks => Live::Now,
        Names::NoCache(k, true) if k == key => Live::Enter,
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
    summary: Option<Summary>,
    changes: Option<JobChange>,
    /// When it was kept (`Inner::kept_count`): past `MAX_KEPT_ROWS` the oldest go first.
    order: u64,
}

/// Most rows all tabs keep off screen together (~60 MB); past it the oldest kept results go
/// and their tab searches again when it shows (spec 3.7 counts one tab's 250,000).
pub const MAX_KEPT_ROWS: usize = 500_000;

/// The tabs whose kept results go so that the rest fit in `cap` rows: the oldest first.
/// `kept`: (tab, when it was kept, rows).
pub fn over_cap(mut kept: Vec<(u64, u64, usize)>, cap: usize) -> Vec<u64> {
    let mut total: usize = kept.iter().map(|k| k.2).sum();
    kept.sort_by_key(|k| k.1);
    let mut gone = Vec::new();
    for (tab, _, rows) in kept {
        if total <= cap {
            break;
        }
        total -= rows;
        gone.push(tab);
    }
    gone
}

/// The results on screen, and the jobs to check them for once they are.
struct Showing {
    tab: Option<u64>,
    spec: SearchSpec,
    /// How it ended (the status bar); `None` while it runs.
    summary: Option<Summary>,
    complete: bool,
    changes: Option<JobChange>,
}

/// Which results a tab shows: its tab and its search (a job's check is for these).
pub type ResultsKey = (Option<u64>, SearchSpec);

/// A job's effects to check results for (spec 4.7).
#[derive(Debug, Clone)]
pub struct JobChange {
    pub dirs: Vec<PathBuf>,
    /// What jobs started elsewhere made: rows only if this search would find them.
    pub paths: Vec<PathBuf>,
    /// What jobs started from these results made (or hid): rows whether the search would find
    /// them or not.
    pub own_paths: Vec<PathBuf>,
    /// What it moved or renamed (from, to), as it really went: rows follow these.
    pub moves: Vec<(PathBuf, PathBuf)>,
    /// How many jobs this is (`merge`).
    pub jobs: usize,
}

impl JobChange {
    fn size(&self) -> usize {
        self.dirs.len() + self.paths.len() + self.own_paths.len()
    }
}

/// Most jobs a tab's kept results wait for, and most paths: past either they are searched
/// again when they show.
pub const MAX_QUEUED_JOBS: usize = 16;
pub const MAX_QUEUED_PATHS: usize = 20_000;

/// `change` added to what a tab's kept results wait for (`queued`): one check for all. The
/// moves go (which way rows went across jobs is not known: their rows are looked at anew), as
/// do a single job's past the limit. `None`: too much to check, the search runs again.
pub fn merge(queued: Option<JobChange>, change: JobChange) -> Option<JobChange> {
    let mut merged = match queued {
        None => change,
        Some(mut queued) => {
            queued.dirs.extend(change.dirs);
            queued.dirs.sort();
            queued.dirs.dedup();
            queued.paths.extend(change.paths);
            queued.own_paths.extend(change.own_paths);
            queued.moves.clear();
            queued.jobs += change.jobs;
            queued
        }
    };
    if merged.size() + merged.moves.len() > MAX_QUEUED_PATHS {
        merged.moves = Vec::new();
    }
    (merged.jobs <= MAX_QUEUED_JOBS && merged.size() <= MAX_QUEUED_PATHS).then_some(merged)
}

/// A path a check found, with what the disk says of it.
pub type FoundPath = (PathBuf, gezik_core::Entry);

/// `added` split into what `own` (paths of jobs started from the results) holds and the rest,
/// which the search's criteria decide.
pub fn split_own(added: Vec<FoundPath>, own: &[PathBuf]) -> (Vec<FoundPath>, Vec<FoundPath>) {
    let own: std::collections::HashSet<&PathBuf> = own.iter().collect();
    added.into_iter().partition(|(path, _)| own.contains(path))
}

/// Whether this search would find `found` (made by a job started elsewhere): its criteria,
/// its text in files, and the walk's hidden and skip rules.
fn search_finds(query: &Query, walk: &Walk, (path, e): &FoundPath) -> bool {
    gezik_search::everything::kept(path, &walk.roots, e.flags, &walk.rules, &std::collections::HashSet::new())
        && query.passes(&e.name, e.is_dir, e.size, e.modified)
        && query.content().is_none_or(|content| {
            content.reads(&e.name, e.size)
                && content
                    .find_in_file(path, &AtomicBool::new(false), &gezik_platform::decode_ansi)
                    .is_ok_and(|found| found.is_some())
        })
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
    /// Results kept so far (`Kept::order`).
    kept_count: Cell<u64>,
    names: RefCell<Names>,
    /// A key was typed and the results follow once the cache is ready.
    live_waiting: Cell<bool>,
    typing: slint::Timer,
    idle: slint::Timer,
    scopes: RefCell<Vec<Scope>>,
    problems: RefCell<Vec<(PathBuf, String)>>,
    /// The saved searches the ▾ menu listed, by place.
    menu_names: RefCell<Vec<String>>,
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
            kept_count: Cell::new(0),
            names: RefCell::new(Names::None),
            live_waiting: Cell::new(false),
            typing: slint::Timer::default(),
            idle: slint::Timer::default(),
            scopes: RefCell::new(Vec::new()),
            problems: RefCell::new(Vec::new()),
            menu_names: RefCell::new(Vec::new()),
        }));
        window.on_search_edited(|text| with_current(|s| s.edited(&text)));
        window.on_search_content_edited(|text| with_current(|s| s.content_edited(&text)));
        window.on_search_content_toggle(|| with_current(Searches::content_toggle));
        window.on_search_go(|| with_current(Searches::button));
        window.on_filter_search(|| with_current(Searches::filter_to_search));
        // Called after every edit of the results too (a delete, a job's check).
        searches.0.view.on_selection_changed(|| with_current(Searches::recount));
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
            Location::Drives | Location::Trash => SearchSpec::new(Scope::AllDrives),
        };
        self.show_bar(spec, location.folder().map(Path::to_path_buf));
        self.focus_later();
        self.warm();
    }

    /// The palette's "Search for …" (spec 7.2): `text` as the name, under the place shown.
    pub fn search_for(&self, text: &str) {
        let location = self.0.nav.active_location();
        let scope = match &location {
            Location::Search(spec) => spec.scope.clone(),
            Location::Path(folder) | Location::Flat(folder) => Scope::Folder(folder.clone()),
            Location::Drives | Location::Trash => Scope::AllDrives,
        };
        let spec = SearchSpec { pattern: text.to_owned(), ..SearchSpec::new(scope) };
        self.show_bar(spec, location.folder().map(Path::to_path_buf));
        self.sync_bar();
        self.go(false);
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
        let mut draft = self.0.draft.borrow_mut();
        draft.pattern = text.to_owned();
        // A saved search changed is no longer it: its tab is titled by the name typed.
        draft.name = None;
        drop(draft);
        self.sync_bar();
        self.follow_typing();
    }

    fn content_edited(&self, text: &str) {
        let mut draft = self.0.draft.borrow_mut();
        draft.content = text.to_owned();
        draft.name = None;
        drop(draft);
        self.sync_bar();
    }

    fn content_toggle(&self) {
        let open = !self.0.content_open.get();
        self.0.content_open.set(open);
        if !open {
            let mut draft = self.0.draft.borrow_mut();
            draft.content.clear();
            draft.name = None;
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
        // Only the name decides; any root stands in for every drive's.
        let root = draft.scope.folder().unwrap_or(Path::new("/")).to_path_buf();
        let asks = gezik_search::everything::translate(&draft, &[root]).is_some();
        let step = live_step(&self.0.names.borrow(), &key, asks);
        match step {
            Live::Now => {
                self.0.typing.start(slint::TimerMode::SingleShot, TYPING, || with_current(|s| s.run(false, true)))
            }
            Live::Wait => self.0.live_waiting.set(true),
            Live::Large => self.0.view.note("Large folder: press Enter to search".to_owned()),
            Live::Enter => self.0.view.note("Press Enter to search".to_owned()),
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
            // The same search again: run anew (spec 4.7), from the name cache as it is. Set
            // after `show_again` leaves the results (`leaving` clears it: a load overtaken by
            // another place must not make a later one fresh).
            self.0.nav.show_again();
            self.0.fresh.set(true);
        } else if live && matches!(current, Location::Search(_)) {
            self.0.replacing.set(true);
            self.0.nav.replace_location(location);
        } else {
            self.0.nav.go(location);
        }
    }

    /// A saved search (spec 8): the bar shows it, the results come in the active tab or a new
    /// one, the tab titled by its name.
    pub fn run_saved(&self, spec: SearchSpec, new_tab: bool) {
        let origin = spec.scope.folder().map(Path::to_path_buf);
        self.show_bar(spec.clone(), origin);
        *self.0.sent.borrow_mut() = Some(spec.clone());
        let location = Location::Search(Box::new(spec));
        if new_tab {
            self.0.nav.open_tab(location, true);
        } else if self.0.nav.active_location() == location {
            self.0.nav.show_again();
            self.0.fresh.set(true);
        } else {
            self.0.nav.go(location);
        }
    }

    /// `save-search` and the ▾ menu's "Save search…": the bar's search, else the results'.
    pub fn save_current(&self) {
        let spec = if self.0.open.get() {
            Some(self.0.draft.borrow().clone())
        } else {
            spec_of(&self.0.nav.active_location()).filter(|spec| !spec.flat)
        };
        match spec.filter(SearchSpec::is_query) {
            Some(spec) => crate::saved_searches::with_current(|s| s.ask_save(spec)),
            None => self.0.view.note("Open a search to save it".to_owned()),
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
                Some(Showing { tab, spec: kept.spec, summary: kept.summary, complete: true, changes: kept.changes });
            return Listing::Results(kept.results);
        }
        let root = spec.scope.folder().map(Path::to_path_buf).unwrap_or_default();
        let content = !spec.content.is_empty();
        *self.0.pending.borrow_mut() = Some(spec.clone());
        *self.0.showing.borrow_mut() = Some(Showing { tab, spec, summary: None, complete: false, changes: None });
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
                    Some(s) => (s.summary.as_ref().map(done_text), s.changes.take()),
                    None => (None, None),
                };
                self.0.view.set_results_status(status);
                // Jobs that ended while these results were kept (spec 4.7): one check.
                if let (Some(key), Some(change)) = (self.results_key(), changes) {
                    self.check(key, change);
                }
            }
        }
    }

    /// The saved search `old` is now called `new` (the tabs too): the results shown and kept
    /// for it stay theirs.
    pub fn rename_saved(&self, old: &str, new: &str) {
        let (mut showing, mut kept) = (self.0.showing.borrow_mut(), self.0.kept.borrow_mut());
        for spec in kept.values_mut().map(|k| &mut k.spec).chain(showing.as_mut().map(|s| &mut s.spec)) {
            if spec.name.as_deref() == Some(old) {
                spec.name = Some(new.to_owned());
            }
        }
    }

    /// The navigator leaves what is on screen (another place, another tab, a reload): a
    /// running search stops (its later events are dropped); whole results stay with their tab.
    pub fn leaving(&self) {
        self.0.fresh.set(false);
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
            let order = self.0.kept_count.get() + 1;
            self.0.kept_count.set(order);
            let kept = Kept { spec: showing.spec, results, summary: showing.summary, changes: showing.changes, order };
            let mut all = self.0.kept.borrow_mut();
            all.insert(tab, kept);
            let sizes = all.iter().map(|(tab, kept)| (*tab, kept.order, kept.results.len())).collect();
            for tab in over_cap(sizes, MAX_KEPT_ROWS) {
                all.remove(&tab);
            }
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
    pub fn job_done(
        &self,
        origin: Option<&ResultsKey>,
        dirs: Vec<PathBuf>,
        paths: Vec<PathBuf>,
        moves: Vec<(PathBuf, PathBuf)>,
    ) {
        let change = |key: &ResultsKey| {
            let own = origin == Some(key);
            JobChange {
                dirs: dirs.clone(),
                paths: if own { Vec::new() } else { paths.clone() },
                own_paths: if own { paths.clone() } else { Vec::new() },
                moves: moves.clone(),
                jobs: 1,
            }
        };
        let touched: Vec<ResultsKey> = self
            .0
            .kept
            .borrow()
            .iter()
            .filter(|(_, kept)| touches(kept.spec.scope.folder(), &dirs, &paths))
            .map(|(tab, kept)| (Some(*tab), kept.spec.clone()))
            .collect();
        for key in touched {
            self.queue(&key, change(&key));
        }
        // The name cache holds the scope as it was read: a job of Gezik's in it (a trash, a
        // rename, a copy into it) would let typing show the old names again. Read anew.
        let stale = match &*self.0.names.borrow() {
            Names::Ready(k, _) | Names::Building(k, _) => touches(k.scope.folder(), &dirs, &paths),
            _ => false,
        };
        if stale {
            self.drop_names();
            if self.0.open.get() {
                self.warm();
            }
        }
        if let Some(key) = self.results_key() {
            let change = change(&key);
            self.check(key, change);
        }
    }

    /// Adds `change` to what `key`'s tab keeps its results for (`merge`); too much, and the
    /// kept results go: the search runs again when the tab shows it.
    fn queue(&self, key: &ResultsKey, change: JobChange) {
        let Some(tab) = key.0 else { return };
        let mut kept = self.0.kept.borrow_mut();
        let Some(entry) = kept.get_mut(&tab).filter(|entry| entry.spec == key.1) else { return };
        match merge(entry.changes.take(), change) {
            Some(change) => entry.changes = Some(change),
            None => {
                kept.remove(&tab);
            }
        }
    }

    /// Checks on another thread what `change` did to the results on screen (`key`), then
    /// shows it if they still are, or keeps it for their tab.
    fn check(&self, key: ResultsKey, change: JobChange) {
        let Some(change) = self.after_the_search(change) else { return };
        let all: Vec<PathBuf> = change.own_paths.iter().chain(&change.paths).cloned().collect();
        let Some(probe) = self.0.view.results_probe(&change.dirs, &all) else { return };
        drop(all);
        let (max_size, max_results, skip) = {
            let settings = self.0.settings.borrow();
            (settings.content_max_size, settings.max_results, settings.skip.clone())
        };
        let shown = Searches::view_shown();
        let weak = self.0.window.clone();
        let spawned = std::thread::Builder::new().name("gezik-results-check".into()).spawn(move || {
            gezik_platform::priority::lower_this_thread();
            let verified = probe.verify();
            let (mut added, mut others) = split_own(verified.added, &change.own_paths);
            if !others.is_empty() {
                // Not made from these results: only what this search would find.
                match Query::compile(&key.1, &QueryOptions::local(max_size, max_results)) {
                    Ok(query) => {
                        let walk = plan_walk(&key.1, &skip, shown);
                        others.retain(|found| search_finds(&query, &walk, found));
                    }
                    Err(_) => others.clear(),
                }
            }
            added.extend(others);
            // Rows still there take what the disk has now.
            added.extend(verified.rows);
            let gone = verified.gone;
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
            // A search started since: checked again once it ends.
            Verdict::Apply if self.0.running.borrow().is_some() => {
                let _ = self.after_the_search(change);
            }
            Verdict::Apply => self.0.view.results_changed(&gone, added, &change.moves),
            Verdict::Queue => self.queue(&key, change),
            Verdict::Drop => {}
        }
    }

    /// Rows taken out or added after the search ended: the status bar's count follows them
    /// ("Stopped at N results" names the limit and stays).
    fn recount(&self) {
        let Some(len) = self.0.view.results().map(|set| set.len()) else { return };
        let text = {
            let mut showing = self.0.showing.borrow_mut();
            let Some(summary) = showing.as_mut().and_then(|s| s.summary.as_mut()) else { return };
            if !recounted(summary, len) {
                return;
            }
            done_text(summary)
        };
        self.0.view.set_results_status(Some(text));
    }

    /// While the search on screen runs, a job's check waits for its end (`event`): the walk
    /// may still find what the job made (a row twice), and its batches number their folders
    /// after the ones it sent, which a check adding a folder would shift. `None`: it waits.
    fn after_the_search(&self, change: JobChange) -> Option<JobChange> {
        if self.0.running.borrow().is_none() {
            return Some(change);
        }
        if let Some(showing) = self.0.showing.borrow_mut().as_mut() {
            // Too much to wait for: the rows the search finds stay as they are.
            showing.changes = merge(showing.changes.take(), change);
        }
        None
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
                self.0.view.set_results_status(Some(text));
                let changes = match self.0.showing.borrow_mut().as_mut() {
                    Some(showing) => {
                        showing.complete = !summary.cancelled;
                        showing.summary = Some(summary);
                        showing.changes.take()
                    }
                    None => None,
                };
                if let Some(window) = self.0.window.upgrade() {
                    window.set_search_running(false);
                }
                // Jobs that ended while it ran: one check now.
                if let (Some(key), Some(change)) = (self.results_key(), changes) {
                    self.check(key, change);
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
        // Whatever the read gave, the typing it waited for is answered now (too large, a
        // network folder: Enter searches).
        let waiting = self.0.live_waiting.replace(false);
        if ready {
            self.0.idle.start(slint::TimerMode::SingleShot, CACHE_IDLE, || with_current(Searches::drop_names));
            if waiting && self.0.open.get() {
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
            SearchMenu::More => {
                let names = crate::saved_searches::names();
                let items = ids::search_more_items(self.0.problems.borrow().len(), &names, draft.is_query());
                *self.0.menu_names.borrow_mut() = names;
                (items, Vec::new())
            }
        }
    }

    /// A search menu item; a criterion changed on results already shown runs the search again.
    pub fn menu_chosen(&self, id: u32) {
        match id {
            ids::SEARCH_NEW_TAB => return self.go(true),
            ids::SEARCH_PROBLEMS => return self.show_problems(),
            ids::SIZE_MIN | ids::SIZE_MAX | ids::MODIFIED_BETWEEN => return self.ask_criterion(id),
            ids::SAVE_SEARCH => return self.save_current(),
            id if (ids::SAVED_SEARCH_FIRST..ids::SAVED_SEARCH_DELETE_FIRST + ids::SAVED_SEARCH_MAX).contains(&id) => {
                // By name: settings.toml may have been read again since the menu opened.
                let delete = id >= ids::SAVED_SEARCH_DELETE_FIRST;
                let first = if delete { ids::SAVED_SEARCH_DELETE_FIRST } else { ids::SAVED_SEARCH_FIRST };
                let name = self.0.menu_names.borrow().get((id - first) as usize).cloned();
                if let Some(name) = name {
                    crate::saved_searches::with_current(|s| if delete { s.delete(&name) } else { s.run(&name, false) });
                }
                return;
            }
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
            draft.name = None;
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
                    draft.name = None;
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
    fn a_delete_in_the_results_recounts_them() {
        let mut done = Summary { found: 2, elapsed: Duration::from_millis(50), ..Summary::default() };
        assert!(!recounted(&mut done, 2));
        assert!(recounted(&mut done, 1));
        assert_eq!(done_text(&done), "1 result in 0.1 s");
        let mut limit = Summary { found: 250_000, limit_reached: true, ..Summary::default() };
        assert!(!recounted(&mut limit, 249_999), "the limit is not a count");
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
        assert_eq!(live_step(&Names::None, &here, true), Live::Warm, "dropped when idle: read again");
        let cache = Arc::new(NameCache::default());
        assert_eq!(live_step(&Names::Ready(here.clone(), cache.clone()), &here, true), Live::Now);
        assert_eq!(live_step(&Names::Ready(key("/x"), cache), &here, true), Live::Warm, "another scope");
        let hidden = CacheKey { shown: Some((true, true)), ..here.clone() };
        assert_eq!(live_step(&Names::Ready(hidden.clone(), Arc::default()), &here, true), Live::Warm, "other rules");
        let flag = Arc::new(AtomicBool::new(false));
        assert_eq!(live_step(&Names::Building(here.clone(), flag.clone()), &here, true), Live::Wait);
        assert_eq!(live_step(&Names::Building(hidden, flag), &here, true), Live::Warm);
        assert_eq!(live_step(&Names::TooLarge(here.clone()), &here, true), Live::Large);
        assert_eq!(live_step(&Names::NoCache(here.clone(), true), &here, true), Live::Now, "Everything");
        let walks = live_step(&Names::NoCache(here.clone(), true), &here, false);
        assert_eq!(walks, Live::Enter, "a name Everything cannot ask for would walk at every pause");
        assert_eq!(live_step(&Names::NoCache(here.clone(), false), &here, true), Live::Never, "network");
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
    fn queued_checks_merge_into_one_and_have_a_limit() {
        let job = |dir: &str, own: bool| {
            let made = vec![PathBuf::from(dir).join("n")];
            JobChange {
                dirs: vec![PathBuf::from(dir)],
                paths: if own { Vec::new() } else { made.clone() },
                own_paths: if own { made } else { Vec::new() },
                moves: vec![(PathBuf::from(dir).join("a"), PathBuf::from(dir).join("b"))],
                jobs: 1,
            }
        };
        let one = merge(None, job("/w/a", false)).unwrap();
        assert_eq!(one.moves.len(), 1, "one job keeps its moves");
        let two = merge(Some(one), job("/w/b", true)).unwrap();
        assert_eq!((two.dirs.len(), two.paths.len(), two.own_paths.len(), two.jobs), (2, 1, 1, 2));
        assert!(two.moves.is_empty());
        // Only the other job's path meets the search's criteria.
        let entry = |name: &str| gezik_core::Entry {
            name: name.to_owned(),
            is_dir: false,
            flags: 0,
            size: 0,
            modified: None,
            created: None,
        };
        let found = vec![(PathBuf::from("/w/a/n"), entry("n")), (PathBuf::from("/w/b/n"), entry("n"))];
        let (own, others) = split_own(found, &two.own_paths);
        assert_eq!((own[0].0.clone(), others[0].0.clone()), (PathBuf::from("/w/b/n"), PathBuf::from("/w/a/n")));
        let same = merge(Some(two), job("/w/a", false)).unwrap();
        assert_eq!(same.dirs.len(), 2, "a folder once");
        let mut queued = Some(same);
        for _ in 3..MAX_QUEUED_JOBS {
            queued = merge(queued, job("/w/c", false));
        }
        assert_eq!(queued.as_ref().map(|q| q.jobs), Some(MAX_QUEUED_JOBS));
        assert!(merge(queued, job("/w/d", false)).is_none(), "past the limit: search again");
        let huge = JobChange { paths: vec![PathBuf::from("/w/x"); MAX_QUEUED_PATHS + 1], ..job("/w", false) };
        assert!(merge(None, huge).is_none());
        // A large move: its moves go past the limit, the job's check stays.
        let moves =
            (0..MAX_QUEUED_PATHS).map(|i| (PathBuf::from(format!("/w/a{i}")), PathBuf::from(format!("/w/b{i}"))));
        let large = merge(None, JobChange { moves: moves.collect(), ..job("/w", false) }).unwrap();
        assert!(large.moves.is_empty() && large.paths.len() == 1);
    }

    #[test]
    fn a_check_keeps_only_what_the_walk_would_find() {
        let spec = SearchSpec { pattern: "*.txt".into(), ..SearchSpec::new(Scope::Folder("/w".into())) };
        let query = Query::compile(&spec, &QueryOptions::local(0, 100)).unwrap();
        let rules = gezik_search::walk::WalkRules::new(Some((false, false)), &["skipped".to_owned()], Vec::new());
        let walk = Walk::new(vec![PathBuf::from("/w")], false, 1, rules);
        let found = |path: &str, flags: u8| {
            let path = PathBuf::from(path);
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            (path, gezik_core::Entry { name, is_dir: false, flags, size: 0, modified: None, created: None })
        };
        assert!(search_finds(&query, &walk, &found("/w/a/x.txt", 0)));
        assert!(!search_finds(&query, &walk, &found("/w/a/x.pdf", 0)), "the criteria");
        assert!(!search_finds(&query, &walk, &found("/w/.git/x.txt", 0)), "under a dot folder the view hides");
        assert!(!search_finds(&query, &walk, &found("/w/skipped/x.txt", 0)), "under a skipped folder");
        assert!(!search_finds(&query, &walk, &found("/w/x.txt", gezik_core::Entry::HIDDEN)), "hidden");
    }

    #[test]
    fn kept_results_past_the_cap_go_oldest_first() {
        assert!(over_cap(vec![(1, 1, 200_000), (2, 2, 250_000)], MAX_KEPT_ROWS).is_empty());
        let kept = vec![(7, 3, 250_000), (1, 1, 200_000), (2, 2, 250_000)];
        assert_eq!(over_cap(kept, MAX_KEPT_ROWS), [1], "the oldest goes, the rest fit");
        let kept = vec![(1, 1, 10), (2, 2, 10), (3, 3, MAX_KEPT_ROWS)];
        assert_eq!(over_cap(kept, MAX_KEPT_ROWS), [1, 2]);
        assert!(over_cap(Vec::new(), MAX_KEPT_ROWS).is_empty());
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
