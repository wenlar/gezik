//! The folder compare (11d, spec 11 §7): two folder trees side by side in the shared tool table
//! (11a's `ToolTable`, so no layer of its own), with TC's show buttons, the options as boxes,
//! a filter, and the layer hidden to a status-bar chip while the panes are looked at. The two
//! sides are read on two low-priority threads at once, then (asked) the same-size pairs read
//! byte by byte; all of it stops on Esc or when the layer goes. Nothing of it exists until it
//! opens. Also `select-differences`: the panes' shown lists, nothing read.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant, SystemTime};

use gezik_config::shortcuts::{Chord, Key, Platform};
use gezik_core::compare::{self, Dir, Kind, NO_TIME, Node, Options, Row, SHOW, State, Tree};
use gezik_core::file_filter::FileFilter;
use gezik_search::tree_scan::{self, Rules, Scan};
use slint::{ComponentHandle, Model, ModelNotify, ModelRc, ModelTracker, SharedString, VecModel};

use crate::windows::{Slot, WindowId};
use crate::{AppWindow, ToolRow, ToolToggle};

/// Linux tells `Docs` from `docs`; Windows and macOS do not (spec 11 §7.3).
const CASE_SENSITIVE: bool = cfg!(target_os = "linux");
/// The boxes after the six show buttons.
const BOXES: [&str; 4] = ["Subfolders", "Content", "Ignore dates", "Ignore 1 h"];
const BUTTONS: [&str; 10] = [
    "Compare Again",
    "Swap",
    "Mode",
    "Direction",
    "Synchronize…",
    "Save Profile…",
    "Show Left",
    "Show Right",
    "Hide",
    "Close",
];
/// How often the reading threads say how far they are.
const PROGRESS_EVERY: Duration = Duration::from_millis(250);
const SIZE_WIDTH: usize = 10;
const DATE_WIDTH: usize = 17;

thread_local! {
    /// Each window's compare: open, or hidden behind the chip.
    pub(crate) static CURRENT: Slot<FolderCompare> = const { Slot::new() };
}

/// Runs `f` with the current window's compare (open or hidden).
pub fn with_current(f: impl FnOnce(&FolderCompare)) {
    if let Some(layer) = CURRENT.with(Slot::current) {
        f(&layer);
    }
}

/// Runs `f` with the current window's compare while it shows (it has the tool table).
pub fn with_shown(f: impl FnOnce(&FolderCompare)) {
    if let Some(layer) = CURRENT.with(Slot::current).filter(|l| !l.0.hidden.get()) {
        f(&layer);
    }
}

/// The chip's callbacks.
pub fn install(window: &AppWindow) {
    window.on_tool_chip_open(|| with_current(FolderCompare::restore));
    window.on_tool_chip_close(|| with_current(FolderCompare::close));
}

/// Window `id` closes: its compare stops and goes.
pub fn forget(id: WindowId) {
    if let Some(layer) = CURRENT.with(|s| s.get(id)) {
        layer.0.stop.borrow().store(true, Ordering::Relaxed);
    }
    CURRENT.with(|s| s.forget(id));
}

/// Job `id` ended: a compare whose sync it was marks its rows; one whose folders hold a folder
/// the job changed may be out of date.
pub fn job_finished(id: gezik_ops::JobId, report: &gezik_ops::Report) {
    for (window, layer) in CURRENT.with(Slot::all) {
        let mine = layer.0.syncing.borrow().as_ref().is_some_and(|(job, ..)| *job == id);
        if mine {
            crate::windows::enter(window, || layer.sync_finished(report));
        } else if layer.touched_by(&report.changed_dirs) {
            crate::windows::enter(window, || layer.show_summary());
        }
    }
}

/// Whether one of `dirs` is in (or is) one of the compared folders.
fn touches(dirs: &[PathBuf], roots: &(PathBuf, PathBuf)) -> bool {
    use gezik_core::ops::paths::is_within;
    dirs.iter().any(|dir| is_within(dir, &roots.0) || is_within(dir, &roots.1))
}

const NESTED: &str = "Cannot synchronize a folder with itself or with a folder inside it";

/// Whether one folder is the other or inside it (a sync would copy into what it reads).
fn nested(a: &Path, b: &Path) -> bool {
    use gezik_core::ops::paths::is_within;
    is_within(a, b) || is_within(b, a)
}

/// Whether `path`'s drive has a trash (deletes go nowhere else).
fn has_bin(path: &Path) -> bool {
    gezik_platform::fs::drive_facts(path).is_ok_and(|facts| facts.trash)
}

/// Compares `left` and `right` in the current window (a compare hidden there goes); `told` is
/// said first (the compare program was not found).
pub fn open(left: PathBuf, right: PathBuf, told: String) {
    let options = crate::windows::config().map(|c| c.load_state().compare).unwrap_or_default();
    open_with(left, right, told, options, (String::new(), None));
}

/// Saved sync `name` (the palette's `Sync: <name>`): its folders compared with its options and
/// filter; nothing is synchronized before the summary question.
pub fn open_profile(name: &str) {
    let view = crate::panes::active_view();
    let Some(p) = crate::sync::profiles().into_iter().find(|p| p.name == name) else { return };
    let dirs = gezik_config::paths::KnownDirs::system();
    let folder = |text: &str| dirs.expand_checked(text).filter(|p| p.is_absolute());
    let (Some(left), Some(right)) = (folder(&p.left), folder(&p.right)) else {
        return view.note(format!("Sync profile \"{name}\" has a folder that is not on this computer"));
    };
    if nested(&left, &right) {
        return view.note(format!("Sync profile \"{name}\": {NESTED}"));
    }
    let shown = crate::windows::config().map(|c| c.load_state().compare.show).unwrap_or(Options::default().show);
    open_with(left, right, String::new(), Options { show: shown, ..p.options }, (p.filter, Some(p.name)));
}

/// `open` with `options` and (filter, the profile it came from).
fn open_with(
    left: PathBuf,
    right: PathBuf,
    told: String,
    options: Options,
    (filter, profile): (String, Option<String>),
) {
    let Some(window) = crate::windows::with_window(crate::windows::current(), |w| w.as_weak()) else { return };
    let Some(window_ref) = window.upgrade() else { return };
    if window_ref.get_tool_open() && CURRENT.with(Slot::current).is_none_or(|l| l.0.hidden.get()) {
        return crate::panes::active_view().note("Close the open tool first".to_owned());
    }
    if let Some(old) = CURRENT.with(Slot::current) {
        old.0.stop.borrow().store(true, Ordering::Relaxed);
    }
    let layer = FolderCompare::new(window, (left, right), options, told);
    *layer.0.filter_text.borrow_mut() = filter;
    *layer.0.profile.borrow_mut() = profile;
    CURRENT.with(|s| s.set(crate::windows::current(), layer.clone()));
    layer.show();
    layer.start();
}

/// `select-differences` (spec 11 §7.5): in both panes, what the other pane's list lacks or has
/// older. Only the lists shown: nothing is read.
pub fn select_differences() {
    let view = crate::panes::active_view();
    let (Some(left), Some(right)) = (crate::panes::at(0), crate::panes::at(1)) else {
        return view.note("Select Differences needs two panes (F3)".to_owned());
    };
    let folder = |v: &crate::view::View| v.folder().is_some() && !v.shows_results() && !v.shows_trash();
    if !folder(&left.view) || !folder(&right.view) {
        return view.note("Select Differences compares the folders the two panes show".to_owned());
    }
    let items = |v: &crate::view::View| -> Vec<compare::Item> {
        v.shown_entries()
            .iter()
            .map(|e| (tree_scan::name_key(&e.name, CASE_SENSITIVE), e.is_dir, compare::seconds(e.modified)))
            .collect()
    };
    let (l, r) = compare::differences(&items(&left.view), &items(&right.view));
    left.view.select_only(&l);
    right.view.select_only(&r);
    view.note(format!("Selected {} on the left, {} on the right", group(l.len()), group(r.len())));
}

/// What a finished compare found.
struct Done {
    left: Tree,
    right: Tree,
    rows: Vec<Row>,
    /// Why a pair could not be read for its content (by row).
    unreadable: HashMap<u32, String>,
    links: usize,
    online: usize,
    /// Whether each side's drive has a trash (asked on the reading thread, not the UI's).
    bins: [bool; 2],
    /// The compare saw all of each folder: no filter, hidden and system items shown. Only then
    /// does a sync trash a folder whole.
    whole: bool,
}

/// The rows shown: built only for the rows on screen.
#[derive(Default)]
struct Rows {
    done: RefCell<Option<Rc<Done>>>,
    /// The row indices shown, in order.
    shown: RefCell<Vec<u32>>,
    /// Open folders (by row).
    open: RefCell<Vec<bool>>,
    selected: RefCell<BTreeSet<u32>>,
    roots: RefCell<(PathBuf, PathBuf)>,
    /// Each row's sync direction, the rows whose direction the user chose, and what became of
    /// the rows a sync took (`None`: done; else why not).
    dirs: RefCell<Vec<Dir>>,
    user: RefCell<BTreeSet<u32>>,
    after: RefCell<HashMap<u32, Option<String>>>,
    notify: ModelNotify,
}

impl Model for Rows {
    type Data = ToolRow;

    fn row_count(&self) -> usize {
        self.shown.borrow().len()
    }

    fn row_data(&self, i: usize) -> Option<ToolRow> {
        let at = *self.shown.borrow().get(i)?;
        let done = self.done.borrow().clone()?;
        let open = self.open.borrow().get(at as usize).copied().unwrap_or(false);
        let mark = Mark {
            dir: self.dirs.borrow().get(at as usize).copied().unwrap_or_default(),
            user: self.user.borrow().contains(&at),
            after: self.after.borrow().get(&at).cloned(),
        };
        Some(tool_row(&done, at, open, self.selected.borrow().contains(&at), mark))
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }
}

/// A side's size and date text (none for a folder or a missing item).
fn side(node: Option<&Node>) -> (String, String) {
    match node {
        Some(n) if matches!(n.kind, Kind::File | Kind::OnlineFile) => {
            let date = if n.modified == NO_TIME {
                String::new()
            } else {
                let at = u64::try_from(n.modified)
                    .map_or(SystemTime::UNIX_EPOCH, |s| SystemTime::UNIX_EPOCH + Duration::from_secs(s));
                crate::view_options::date_text(at)
            };
            (crate::view_options::size_text(n.size), date)
        }
        _ => (String::new(), String::new()),
    }
}

/// A row's sync: its direction, whether the user chose it, what became of it.
#[derive(Default)]
struct Mark {
    dir: Dir,
    user: bool,
    after: Option<Option<String>>,
}

/// Row `at` as the table shows it: a counted row's middle column is its direction.
fn tool_row(done: &Done, at: u32, open: bool, selected: bool, sync: Mark) -> ToolRow {
    let row = done.rows[at as usize];
    let l = row.left.map(|i| &done.left.nodes[i as usize]);
    let r = row.right.map(|i| &done.right.nodes[i as usize]);
    let named = l.or(r).map_or("", |n| &n.name);
    // Plain characters: the UI font lacks ▾ and ▸ (a row's name is text, nothing is drawn).
    let mark = match (row.folder, open) {
        (false, _) => "",
        (true, true) => "- ",
        (true, false) => "+ ",
    };
    let slash = if row.folder { std::path::MAIN_SEPARATOR_STR } else { "" };
    let name = format!("{}{mark}{named}{slash}", "   ".repeat(usize::from(row.depth)));
    let ((ls, ld), (rs, rd)) = (side(l), side(r));
    let middle = match &sync.after {
        Some(None) => "=",
        Some(Some(_)) => "!",
        None if row.unit => compare::act(&row, sync.dir).map_or(" ", compare::Act::symbol),
        None => row.state.symbol(),
    };
    let values = format!("{ls:>SIZE_WIDTH$}  {ld:<DATE_WIDTH$}  {middle:<2} {rs:>SIZE_WIDTH$}  {rd}");
    let status = match row.state {
        State::Unreadable => {
            let why = done
                .unreadable
                .get(&at)
                .map(String::as_str)
                .or_else(|| row.left.and_then(|i| done.left.problem(i)))
                .or_else(|| row.right.and_then(|i| done.right.problem(i)));
            match why {
                Some(why) => format!("Unreadable: {why}"),
                None => "Unreadable".to_owned(),
            }
        }
        State::Equal if row.dates_differ => "Same content".to_owned(),
        state if row.check => format!("{} (content not read)", state.label()),
        state => state.label().to_owned(),
    };
    let failed = matches!(sync.after, Some(Some(_)));
    let status = match (sync.after, sync.user) {
        (Some(None), _) => "Synchronized".to_owned(),
        (Some(Some(why)), _) => format!("Not done: {why}"),
        // Short: the status column is narrow.
        (None, true) => format!("{status} *"),
        (None, false) => status,
    };
    let tone = match row.state {
        _ if failed => 2,
        State::Equal => 1,
        State::Mismatch | State::Unreadable => 2,
        _ => 0,
    };
    ToolRow { name: name.into(), values: values.trim_end().into(), status: status.into(), tone, selected }
}

struct Inner {
    window: slint::Weak<AppWindow>,
    /// The folders compared (left, right).
    paths: RefCell<(PathBuf, PathBuf)>,
    options: Cell<Options>,
    filter_text: RefCell<String>,
    /// The filter the shown result was read with.
    scanned_filter: RefCell<String>,
    rows: Rc<Rows>,
    done: RefCell<Option<Rc<Done>>>,
    /// Bumped by each run: a late answer of an earlier one is dropped.
    generation: Cell<u64>,
    stop: RefCell<Arc<AtomicBool>>,
    running: Cell<bool>,
    progress: RefCell<String>,
    /// Said before the summary until a result comes.
    told: RefCell<String>,
    /// Behind the chip.
    hidden: Cell<bool>,
    /// One of Gezik's jobs ended since the result came.
    stale: Cell<bool>,
    anchor: Cell<Option<u32>>,
    /// Said in the summary until the next read: directions reset, what a sync did.
    note: RefCell<String>,
    /// The saved sync it was opened from (Save Profile… offers its name).
    profile: RefCell<Option<String>>,
    /// The sync job running from it, its steps, and the result they were planned from.
    syncing: RefCell<Option<(gezik_ops::JobId, crate::sync::Planned, Rc<Done>)>>,
}

#[derive(Clone)]
pub struct FolderCompare(Rc<Inner>);

/// What the reading threads send.
enum Message {
    Progress(String),
    Done(Box<Result<Done, String>>),
}

impl FolderCompare {
    fn new(window: slint::Weak<AppWindow>, paths: (PathBuf, PathBuf), options: Options, told: String) -> Self {
        FolderCompare(Rc::new(Inner {
            window,
            paths: RefCell::new(paths),
            options: Cell::new(options),
            filter_text: RefCell::default(),
            scanned_filter: RefCell::default(),
            rows: Rc::default(),
            done: RefCell::default(),
            generation: Cell::new(0),
            stop: RefCell::new(Arc::new(AtomicBool::new(false))),
            running: Cell::new(false),
            progress: RefCell::default(),
            told: RefCell::new(told),
            hidden: Cell::new(false),
            stale: Cell::new(false),
            anchor: Cell::new(None),
            note: RefCell::default(),
            profile: RefCell::default(),
            syncing: RefCell::default(),
        }))
    }

    /// A job changed `dirs`: marks the result out of date if they are in its folders (and says
    /// whether it did).
    fn touched_by(&self, dirs: &[PathBuf]) -> bool {
        let touched = self.0.done.borrow().is_some() && touches(dirs, &self.0.rows.roots.borrow());
        if touched {
            self.0.stale.set(true);
        }
        touched
    }

    /// Puts the compare into the tool table and shows it.
    fn show(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        self.0.hidden.set(false);
        let (left, right) = self.0.paths.borrow().clone();
        window.set_tool_title(format!("Compare  {}  ⇄  {}", left.display(), right.display()).into());
        window.set_tool_field_label("Filter".into());
        window.set_tool_field_text(self.0.filter_text.borrow().as_str().into());
        window.set_tool_field_focused(false);
        self.show_buttons();
        window.set_tool_columns(
            format!("{:>SIZE_WIDTH$}  {:<DATE_WIDTH$}  ⇄  {:>SIZE_WIDTH$}  Modified", "Size", "Modified", "Size")
                .into(),
        );
        window.set_tool_rows(ModelRc::from(self.0.rows.clone()));
        window.set_tool_chip(SharedString::new());
        self.show_toggles();
        self.show_summary();
        window.set_tool_open(true);
    }

    /// The buttons; Mode says the mode.
    fn show_buttons(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        let mode = self.0.options.get().mode;
        let labels: Vec<SharedString> = BUTTONS
            .iter()
            .map(|b| if *b == "Mode" { format!("Mode: {}", mode.label()).into() } else { SharedString::from(*b) })
            .collect();
        // Changed in place: Mode is clicked from inside this strip, and a new model would destroy
        // the button whose click is still running (the window then aborted on the next Close).
        let current = window.get_tool_buttons();
        if current.row_count() == labels.len() {
            for (i, label) in labels.into_iter().enumerate() {
                if current.row_data(i).as_ref() != Some(&label) {
                    current.set_row_data(i, label);
                }
            }
        } else {
            window.set_tool_buttons(ModelRc::new(VecModel::from(labels)));
        }
    }

    /// The boxes: the show buttons with their numbers, then the options.
    fn show_toggles(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        let o = self.0.options.get();
        let counts = self.0.done.borrow().as_ref().map(|d| compare::counts(&d.rows));
        let mut toggles: Vec<ToolToggle> = SHOW
            .iter()
            .enumerate()
            .map(|(i, b)| ToolToggle {
                label: counts.map_or_else(|| (*b).to_owned(), |n| format!("{b} {}", group(n[i]))).into(),
                checked: o.show[i],
            })
            .collect();
        for (label, checked) in BOXES.iter().zip([o.subfolders, o.content, o.ignore_dates, o.ignore_hour]) {
            toggles.push(ToolToggle { label: (*label).into(), checked });
        }
        // Changed in place: a new model would rebuild the boxes and drop the keyboard's one.
        let current = window.get_tool_toggles();
        if current.row_count() == toggles.len() {
            for (i, toggle) in toggles.into_iter().enumerate() {
                if current.row_data(i).as_ref() != Some(&toggle) {
                    current.set_row_data(i, toggle);
                }
            }
        } else {
            window.set_tool_toggles(ModelRc::new(VecModel::from(toggles)));
        }
    }

    fn show_summary(&self) {
        if self.0.hidden.get() {
            return self.show_chip();
        }
        let Some(window) = self.0.window.upgrade() else { return };
        let (text, tone) = self.summary();
        let told = self.0.told.borrow();
        let text = if told.is_empty() { text } else { format!("{told}. {text}") };
        window.set_tool_summary(text.into());
        window.set_tool_summary_tone(tone);
    }

    fn summary(&self) -> (String, i32) {
        if self.0.running.get() {
            return (self.0.progress.borrow().clone(), 0);
        }
        let typed = self.0.filter_text.borrow().clone();
        if let Err(err) = FileFilter::parse(&typed, false) {
            return (err, 2);
        }
        let Some(done) = self.0.done.borrow().clone() else { return (self.0.progress.borrow().clone(), 0) };
        let n = compare::counts(&done.rows);
        let mut parts = vec![format!("{} compared", items_text(n[4] + n[5]))];
        for (i, what) in [(0, "→"), (3, "←"), (2, "≠")] {
            if n[i] > 0 {
                parts.push(format!("{} {what}", group(n[i])));
            }
        }
        if n[4] > 0 {
            parts.push(format!("{} on one side only", group(n[4])));
        }
        if done.links > 0 {
            parts.push(format!("{} left out (links)", group(done.links)));
        }
        if done.online > 0 {
            parts.push(format!("{} online only, not read", group(done.online)));
        }
        let unread = done.rows.iter().filter(|r| r.check).count();
        if unread > 0 {
            parts.push(format!("content of {} not read", items_text(unread)));
        }
        let to_do = self.acted_rows();
        if to_do > 0 {
            let keys = if cfg!(target_os = "macos") { "⌘↩" } else { "Ctrl+Enter" };
            parts.push(format!("{} to synchronize ({keys})", items_text(to_do)));
        }
        let note = self.0.note.borrow().clone();
        if !note.is_empty() {
            parts.push(note);
        }
        let selected = self.0.rows.selected.borrow().len();
        if selected > 0 {
            parts.push(format!("Selected {}", group(selected)));
        }
        if typed != *self.0.scanned_filter.borrow() {
            parts.push("Compare Again (F5) applies the filter".to_owned());
        }
        if self.0.stale.get() {
            parts.push("Files may have changed: Compare Again (F5)".to_owned());
        }
        (parts.join(" · "), 0)
    }

    /// Whether a sync from this compare runs (say so): it must not be read again meanwhile.
    fn syncing(&self) -> bool {
        let busy = self.0.syncing.borrow().is_some();
        if busy {
            self.say("Wait until the sync is done");
        }
        busy
    }

    /// Reads both sides (again) with the options and filter set now.
    fn start(&self) {
        if self.syncing() {
            return;
        }
        let filter = match FileFilter::parse(&self.0.filter_text.borrow(), false) {
            Ok(filter) => filter,
            Err(_) => return self.show_summary(),
        };
        self.0.stop.borrow().store(true, Ordering::Relaxed);
        let stop = Arc::new(AtomicBool::new(false));
        *self.0.stop.borrow_mut() = stop.clone();
        // Numbered across the process: a later compare in this window never takes an earlier
        // one's late answer for its own.
        static RUNS: AtomicU64 = AtomicU64::new(0);
        let generation = RUNS.fetch_add(1, Ordering::Relaxed) + 1;
        self.0.generation.set(generation);
        self.0.running.set(true);
        *self.0.scanned_filter.borrow_mut() = self.0.filter_text.borrow().clone();
        *self.0.progress.borrow_mut() = "Reading…".to_owned();
        self.0.stale.set(false);
        self.0.note.borrow_mut().clear();
        let (left, right) = self.0.paths.borrow().clone();
        let options = self.0.options.get();
        let shown = {
            let o = crate::view_options::current();
            (o.show_hidden, o.show_system)
        };
        let window = crate::windows::current();
        let job = Job { left, right, options, filter, shown, stop };
        let spawned = std::thread::Builder::new().name("gezik-folder-compare".into()).spawn(move || {
            gezik_platform::priority::lower_this_thread();
            let answer = run(&job, &mut |text| {
                crate::windows::post(window, move || with_current(|c| c.heard(generation, Message::Progress(text))));
            });
            let message = Message::Done(Box::new(answer));
            crate::windows::post(window, move || with_current(|c| c.heard(generation, message)));
        });
        if let Err(err) = spawned {
            self.0.running.set(false);
            *self.0.progress.borrow_mut() = format!("Cannot compare: {err}");
        }
        self.show_summary();
    }

    fn heard(&self, generation: u64, message: Message) {
        if generation != self.0.generation.get() {
            return;
        }
        match message {
            Message::Progress(text) => *self.0.progress.borrow_mut() = text,
            Message::Done(answer) => {
                self.0.running.set(false);
                match *answer {
                    Ok(done) => {
                        let open: Vec<bool> = done.rows.iter().map(|r| r.folder && !r.one_sided()).collect();
                        let done = Rc::new(done);
                        *self.0.done.borrow_mut() = Some(done.clone());
                        *self.0.rows.done.borrow_mut() = Some(done);
                        *self.0.rows.open.borrow_mut() = open;
                        *self.0.rows.roots.borrow_mut() = self.0.paths.borrow().clone();
                        self.0.rows.selected.borrow_mut().clear();
                        self.reset_dirs(true);
                        self.0.anchor.set(None);
                        self.refilter();
                    }
                    Err(text) => {
                        // The earlier result is not this one's: it goes, the reason shows.
                        *self.0.progress.borrow_mut() = text;
                        *self.0.done.borrow_mut() = None;
                        *self.0.rows.done.borrow_mut() = None;
                        self.0.rows.selected.borrow_mut().clear();
                        self.0.stale.set(false);
                        self.refilter();
                    }
                }
            }
        }
        if self.0.hidden.get() {
            self.show_chip();
        } else {
            self.show_toggles();
            self.show_summary();
        }
    }

    /// The shown rows again (a button, a folder opened or closed); the cursor to the first.
    fn refilter(&self) {
        let shown = match &*self.0.rows.done.borrow() {
            Some(done) => compare::visible(&done.rows, &self.0.options.get().show, &self.0.rows.open.borrow()),
            None => Vec::new(),
        };
        *self.0.rows.shown.borrow_mut() = shown;
        self.0.rows.notify.reset();
        if let Some(window) = self.0.window.upgrade()
            && !self.0.hidden.get()
        {
            window.set_tool_cursor(-1);
        }
    }

    /// Box `i`: a show button, or an option (Subfolders, Content, the dates: read again).
    pub(crate) fn toggle(&self, i: i32) {
        let Ok(i) = usize::try_from(i) else { return };
        if i >= 6 && self.syncing() {
            return;
        }
        let mut o = self.0.options.get();
        let again = match i {
            0..6 => {
                o.show[i] = !o.show[i];
                false
            }
            6 => toggled(&mut o.subfolders),
            7 => toggled(&mut o.content),
            8 => toggled(&mut o.ignore_dates),
            9 => toggled(&mut o.ignore_hour),
            _ => return,
        };
        self.0.options.set(o);
        if let Some(config) = crate::windows::config() {
            config.update_state(move |state| state.compare = o);
        }
        self.show_toggles();
        if again {
            self.start();
        } else {
            self.refilter();
            self.show_summary();
        }
    }

    pub(crate) fn button(&self, i: i32) {
        match usize::try_from(i).ok().and_then(|i| BUTTONS.get(i)) {
            Some(&"Compare Again") => self.start(),
            Some(&"Swap") => self.swap(),
            Some(&"Mode") => self.next_mode(),
            Some(&"Direction") => self.set_dirs(compare::cycle, true),
            Some(&"Synchronize…") => self.synchronize(),
            Some(&"Save Profile…") => self.save_profile(),
            Some(&"Show Left") => self.show_in_pane(false),
            Some(&"Show Right") => self.show_in_pane(true),
            Some(&"Hide") => self.hide(),
            Some(_) => self.close(),
            None => {}
        }
    }

    pub(crate) fn field_edited(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        *self.0.filter_text.borrow_mut() = window.get_tool_field_text().to_string();
        self.show_summary();
    }

    fn swap(&self) {
        if self.syncing() {
            return;
        }
        {
            let mut paths = self.0.paths.borrow_mut();
            let (l, r) = paths.clone();
            *paths = (r, l);
        }
        self.show();
        self.start();
    }

    /// The row the cursor is on (a row index).
    fn cursor_row(&self) -> Option<u32> {
        let window = self.0.window.upgrade()?;
        let at = usize::try_from(window.get_tool_cursor()).ok()?;
        self.0.rows.shown.borrow().get(at).copied()
    }

    /// Row `at`'s path on one side (`None`: that side lacks it).
    fn path(&self, at: u32, right_side: bool) -> Option<PathBuf> {
        let done = self.0.done.borrow().clone()?;
        let row = done.rows.get(at as usize)?;
        let roots = self.0.rows.roots.borrow();
        if right_side {
            row.right.map(|i| tree_scan::path_of(&roots.1, &done.right, i))
        } else {
            row.left.map(|i| tree_scan::path_of(&roots.0, &done.left, i))
        }
    }

    /// Show Left / Show Right: that pane (opened if needed) goes to the row's folder there with
    /// it selected (a side lacking it: the nearest folder it has), and the layer hides.
    fn show_in_pane(&self, right_side: bool) {
        let Some(at) = self.cursor_row().or_else(|| self.0.rows.selected.borrow().first().copied()) else {
            return self.say("Select a row first");
        };
        let target = self.path(at, right_side);
        let (dir, names) = match target {
            Some(path) => {
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
                (path.parent().map(Path::to_path_buf).unwrap_or_default(), name.into_iter().collect())
            }
            None => {
                let other = self.path(at, !right_side).unwrap_or_default();
                let (from, to) = self.0.rows.roots.borrow().clone();
                let (from, to) = if right_side { (from, to) } else { (to, from) };
                let mut dir = to.join(other.strip_prefix(&from).unwrap_or(Path::new("")));
                while !dir.is_dir() && dir.pop() {}
                (dir, Vec::new())
            }
        };
        self.hide();
        if !crate::dual::is_open() {
            crate::dual::open(None);
        }
        if crate::dual::is_open() {
            crate::dual::activate(usize::from(right_side));
        }
        crate::panes::active_nav().go_selecting(dir, names);
    }

    fn show_chip(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        let name = self.0.paths.borrow().0.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let differs = self.0.done.borrow().as_ref().map(|d| {
            let n = compare::counts(&d.rows);
            n[0] + n[2] + n[3]
        });
        let text = match (self.0.running.get(), differs) {
            (true, _) => format!("Compare: {name} (…)"),
            (false, Some(n)) => format!("Compare: {name} ({} ≠)", group(n)),
            (false, None) => format!("Compare: {name}"),
        };
        window.set_tool_chip(text.into());
    }

    /// The layer goes behind a status-bar chip; everything it found stays.
    fn hide(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        self.0.hidden.set(true);
        window.set_tool_open(false);
        window.set_tool_rows(ModelRc::default());
        window.set_tool_toggles(ModelRc::default());
        self.show_chip();
        if !window.get_dialog_open() && !window.get_conflicts_open() {
            window.invoke_focus_list();
        }
    }

    /// The chip was clicked: the layer is back (unless another tool has the table).
    pub fn restore(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        if !self.0.hidden.get() {
            return;
        }
        if window.get_tool_open() {
            return crate::panes::active_view().note("Close the open tool first".to_owned());
        }
        self.show();
        self.refilter();
    }

    /// Closes the compare: its threads stop, its rows go.
    pub fn close(&self) {
        self.0.stop.borrow().store(true, Ordering::Relaxed);
        self.0.generation.set(0);
        let hidden = self.0.hidden.get();
        CURRENT.with(|s| s.forget(crate::windows::current()));
        let Some(window) = self.0.window.upgrade() else { return };
        window.set_tool_chip(SharedString::new());
        // Hidden, the table may be another tool's now.
        if hidden {
            return;
        }
        window.set_tool_open(false);
        window.set_tool_rows(ModelRc::default());
        window.set_tool_toggles(ModelRc::default());
        window.set_tool_buttons(ModelRc::default());
        window.set_tool_field_text(SharedString::new());
        if !window.get_dialog_open() && !window.get_conflicts_open() {
            window.invoke_focus_list();
        }
    }

    fn say(&self, text: &str) {
        if let Some(window) = self.0.window.upgrade() {
            window.set_tool_summary(text.into());
            window.set_tool_summary_tone(0);
        }
    }

    /// A click on shown row `i`: it alone, with Ctrl added or taken away, with Shift the span.
    /// A plain click on the folder the cursor is on opens or closes it.
    pub(crate) fn pressed(&self, i: i32, ctrl: bool, shift: bool) {
        let shown = self.0.rows.shown.borrow().clone();
        let Some(&at) = usize::try_from(i).ok().and_then(|i| shown.get(i)) else { return };
        let Some(window) = self.0.window.upgrade() else { return };
        if !ctrl && !shift && window.get_tool_cursor() == i && self.is_folder(at) {
            return self.open_folder(at, None);
        }
        {
            let mut selected = self.0.rows.selected.borrow_mut();
            match (ctrl, shift, self.0.anchor.get().and_then(|a| shown.iter().position(|s| *s == a))) {
                (_, true, Some(from)) => {
                    let to = usize::try_from(i).unwrap_or(0);
                    if !ctrl {
                        selected.clear();
                    }
                    selected.extend(shown[from.min(to)..=from.max(to)].iter().copied());
                }
                (true, false, _) => {
                    if !selected.remove(&at) {
                        selected.insert(at);
                    }
                    self.0.anchor.set(Some(at));
                }
                _ => {
                    selected.clear();
                    selected.insert(at);
                    self.0.anchor.set(Some(at));
                }
            }
        }
        window.set_tool_cursor(i);
        self.0.rows.notify.reset();
        self.show_summary();
    }

    fn is_folder(&self, at: u32) -> bool {
        self.0.done.borrow().as_ref().is_some_and(|d| d.rows.get(at as usize).is_some_and(|r| r.folder))
    }

    /// Opens (`Some(true)`), closes (`Some(false)`) or flips (`None`) folder row `at`; the
    /// cursor stays on it.
    fn open_folder(&self, at: u32, to: Option<bool>) {
        {
            let mut open = self.0.rows.open.borrow_mut();
            let Some(slot) = open.get_mut(at as usize) else { return };
            *slot = to.unwrap_or(!*slot);
        }
        self.refilter();
        let index = self.0.rows.shown.borrow().iter().position(|s| *s == at);
        if let (Some(window), Some(index)) = (self.0.window.upgrade(), index) {
            window.set_tool_cursor(i32::try_from(index).unwrap_or(-1));
        }
    }

    /// Enter: a folder opens or closes; two files open in the file compare (over this layer).
    fn enter(&self) {
        let Some(at) = self.cursor_row() else { return };
        if self.is_folder(at) {
            return self.open_folder(at, None);
        }
        match (self.path(at, false), self.path(at, true)) {
            (Some(l), Some(r)) if l.is_file() && r.is_file() => crate::diff::with_current(|d| d.open(l, r)),
            _ => self.say("Enter compares two files: this one is not on both sides"),
        }
    }

    /// Ctrl+C: the selected rows' paths (the left one, else the right).
    fn copy(&self) {
        let selected: Vec<u32> = self.0.rows.selected.borrow().iter().copied().collect();
        let picked = if selected.is_empty() { self.cursor_row().into_iter().collect() } else { selected };
        let paths: Vec<String> = picked
            .into_iter()
            .filter_map(|at| self.path(at, false).or_else(|| self.path(at, true)))
            .map(|p| p.display().to_string())
            .collect();
        if paths.is_empty() {
            return self.say("Nothing to copy");
        }
        let text = paths.join(if cfg!(windows) { "\r\n" } else { "\n" });
        match gezik_platform::clipboard::write_text(&text) {
            Ok(()) => self.say(&format!(
                "Copied {}",
                if paths.len() == 1 { "1 path".to_owned() } else { format!("{} paths", group(paths.len())) }
            )),
            Err(_) => self.say("Cannot use the clipboard"),
        }
    }

    fn select_all(&self) {
        let shown = self.0.rows.shown.borrow().clone();
        self.0.rows.selected.borrow_mut().extend(shown);
        self.0.rows.notify.reset();
        self.show_summary();
    }

    /// The selected rows, else the one the cursor is on.
    fn targets(&self) -> Vec<u32> {
        let selected: Vec<u32> = self.0.rows.selected.borrow().iter().copied().collect();
        if selected.is_empty() { self.cursor_row().into_iter().collect() } else { selected }
    }

    /// Each row's direction as the mode says; the user's choices go (`Your direction choices
    /// were reset`). The rows a sync took keep what became of them unless `fresh` (a new read).
    fn reset_dirs(&self, fresh: bool) {
        let mode = self.0.options.get().mode;
        let after = if fresh { HashMap::new() } else { self.0.rows.after.borrow().clone() };
        let dirs = self.0.done.borrow().as_ref().map_or_else(Vec::new, |d| {
            let skip = |i: usize| after.contains_key(&u32::try_from(i).unwrap_or(u32::MAX));
            d.rows
                .iter()
                .enumerate()
                .map(|(i, r)| if skip(i) { Dir::Skip } else { compare::default_dir(r, mode) })
                .collect()
        });
        *self.0.rows.dirs.borrow_mut() = dirs;
        *self.0.rows.after.borrow_mut() = after;
        let had = !self.0.rows.user.borrow().is_empty();
        self.0.rows.user.borrow_mut().clear();
        if had {
            *self.0.note.borrow_mut() = "Your direction choices were reset".to_owned();
        }
    }

    /// The counted rows a sync would act on now.
    fn acted_rows(&self) -> usize {
        let Some(done) = self.0.done.borrow().clone() else { return 0 };
        let dirs = self.0.rows.dirs.borrow();
        done.rows.iter().zip(dirs.iter()).filter(|(row, dir)| compare::act(row, **dir).is_some()).count()
    }

    /// Mode: the next mode; the directions follow it.
    fn next_mode(&self) {
        let mut o = self.0.options.get();
        o.mode = o.mode.next();
        self.0.options.set(o);
        if let Some(config) = crate::windows::config() {
            config.update_state(move |state| state.compare = o);
        }
        self.0.note.borrow_mut().clear();
        self.reset_dirs(false);
        self.show_buttons();
        self.0.rows.notify.reset();
        self.show_summary();
    }

    /// Gives the selected rows (a folder both sides have: every counted row under it) the
    /// direction `to` says for each; with `turn`, all take the one `to` says for the first.
    fn set_dirs(&self, to: impl Fn(&Row, Dir) -> Dir, turn: bool) {
        let Some(done) = self.0.done.borrow().clone() else { return };
        if self.0.running.get() {
            return;
        }
        let units: Vec<u32> =
            self.targets().into_iter().flat_map(|at| compare::units_under(&done.rows, at as usize)).collect();
        let after = self.0.rows.after.borrow().clone();
        let units: Vec<u32> = units.into_iter().filter(|at| !after.contains_key(at)).collect();
        let Some(&first) = units.first() else { return self.say("This row takes no direction") };
        let mode = self.0.options.get().mode;
        {
            let mut dirs = self.0.rows.dirs.borrow_mut();
            let mut user = self.0.rows.user.borrow_mut();
            let turned = turn.then(|| to(&done.rows[first as usize], dirs[first as usize]));
            for at in units {
                let (row, i) = (&done.rows[at as usize], at as usize);
                let want = turned.unwrap_or_else(|| to(row, dirs[i]));
                dirs[i] = if compare::allowed(row, want) { want } else { Dir::Skip };
                if dirs[i] == compare::default_dir(row, mode) {
                    user.remove(&at);
                } else {
                    user.insert(at);
                }
            }
        }
        self.0.rows.notify.reset();
        self.show_summary();
    }

    /// Shift+Space: the selected rows' directions as the mode says.
    fn reset_selected(&self) {
        let mode = self.0.options.get().mode;
        self.set_dirs(move |row, _| compare::default_dir(row, mode), false);
    }

    /// Synchronize… (Ctrl+Enter): the summary question, then one job.
    fn synchronize(&self) {
        if self.0.running.get() {
            return self.say("Wait until the compare is done");
        }
        if self.0.syncing.borrow().is_some() {
            return self.say("A sync from this compare is running");
        }
        let Some(done) = self.0.done.borrow().clone() else { return self.say("Nothing to synchronize") };
        let roots = self.0.rows.roots.borrow().clone();
        if nested(&roots.0, &roots.1) {
            return self.say(NESTED);
        }
        let dirs = self.0.rows.dirs.borrow().clone();
        let planned = crate::sync::plan(&done.left, &done.right, &done.rows, &dirs, (&roots.0, &roots.1), done.whole);
        if planned.steps.is_empty() {
            return self.say("Nothing to synchronize");
        }
        let bins = done.bins;
        let all = crate::sync::summary(&planned);
        let strict = crate::sync::summary(&crate::sync::keep_safe(planned.clone(), bins, [false; 2]));
        let refused = crate::sync::Summary {
            replacing: [0, 1].map(|i| all.replacing[i] - strict.replacing[i]),
            trashed: [0, 1].map(|i| all.trashed[i] - strict.trashed[i]),
            ..Default::default()
        };
        let mut lines = crate::sync::lines(&strict, [&roots.0, &roots.1], bins, &refused);
        if planned.unread > 0 {
            lines.push(format!("{} that could not be read left out", folders_text(planned.unread)));
        }
        if planned.listed_only > 0 {
            lines.push(format!(
                "{} to move to {} go file by file: the compare did not see all of {} (a filter, or hidden items)",
                folders_text(planned.listed_only),
                crate::sync::bin_name(),
                if planned.listed_only == 1 { "it" } else { "them" }
            ));
        }
        let ask_anyway = [0, 1].map(|i| refused.replacing[i] > 0);
        if strict.files == [0, 0] && strict.trashed == [0, 0] && ask_anyway == [false; 2] {
            return self.say(&format!("Nothing to synchronize: {}", lines.join(" · ")));
        }
        // A side whose other side is empty would lose everything (an unplugged disk, a wrong
        // folder): said, and only with its box ticked.
        let trees = [&done.left, &done.right];
        let emptied = [0, 1].map(|i| strict.trashed[i] > 0 && trees[1 - i].nodes.len() <= 1);
        let roots_of = [&roots.0, &roots.1];
        // (side, what the box does): 0 replace anyway on that side, 1 empty that side, 2 verify
        // the copies (side unused).
        let mut boxes: Vec<(usize, u8)> = Vec::new();
        let mut checks = Vec::new();
        if strict.files != [0, 0] || ask_anyway != [false; 2] {
            let mut verify = false;
            crate::operations::with_current(|ops| verify = ops.verify_default());
            boxes.push((0, 2));
            checks.push(("Verify copies".to_owned(), verify));
        }
        for i in [1, 0] {
            if ask_anyway[i] {
                boxes.push((i, 0));
                checks.push((format!("Replace them anyway on {}", roots_of[i].display()), false));
            }
            if emptied[i] {
                lines.push(format!(
                    "{} is empty: everything synchronized on {} goes to {}",
                    roots_of[1 - i].display(),
                    roots_of[i].display(),
                    crate::sync::bin_name()
                ));
                boxes.push((i, 1));
                checks.push((
                    format!("Yes, move everything on {} to {}", roots_of[i].display(), crate::sync::bin_name()),
                    false,
                ));
            }
        }
        let message = format!("Synchronize {} and {}\n\n{}", roots.0.display(), roots.1.display(), lines.join("\n"));
        let mut dialogs = None;
        crate::operations::with_current(|ops| dialogs = Some(ops.dialogs()));
        let Some(dialogs) = dialogs else { return };
        let this = self.clone();
        dialogs.ask_checks("Synchronize", message, checks, &["Synchronize", "Cancel"], move |answer| {
            let Some((0, marks)) = answer else { return };
            let ticked =
                |side: usize, what: u8| boxes.iter().zip(marks.iter()).any(|(b, on)| *b == (side, what) && *on);
            if (0..2).any(|i| emptied[i] && !ticked(i, 1)) {
                return this.say("Not synchronized: the box for the empty folder was not ticked");
            }
            this.run_sync(planned, bins, [ticked(0, 0), ticked(1, 0)], ticked(0, 2), done);
        });
    }

    /// The sync the user said yes to: one job, its steps kept to mark the rows when it ends.
    fn run_sync(
        &self,
        planned: crate::sync::Planned,
        bins: [bool; 2],
        anyway: [bool; 2],
        verify: bool,
        from: Rc<Done>,
    ) {
        // Read again or closed meanwhile: these steps belong to another result.
        if !self.0.done.borrow().as_ref().is_some_and(|d| Rc::ptr_eq(d, &from)) || self.0.running.get() {
            return self.say("The compare changed; Synchronize again");
        }
        let planned = crate::sync::keep_safe(planned, bins, anyway);
        if planned.steps.is_empty() {
            return self.say("Nothing to synchronize");
        }
        let (left, right) = self.0.rows.roots.borrow().clone();
        let task = gezik_ops::SyncTask::new(left.clone(), right, planned.steps.clone(), anyway);
        let mut id = None;
        crate::operations::with_current(|ops| {
            let label = Some(gezik_ops::sync_label(&left));
            let after = crate::operations::After::Nothing;
            id = Some(ops.submit_chain_verifying(vec![Box::new(task)], label, None, after, Some(verify)));
        });
        let Some(id) = id else { return };
        *self.0.syncing.borrow_mut() = Some((id, planned, from));
        *self.0.note.borrow_mut() = "Synchronizing…".to_owned();
        self.show_summary();
    }

    /// The sync job ended: each row it took says what became of it, and takes no direction.
    fn sync_finished(&self, report: &gezik_ops::Report) {
        let Some((_, planned, from)) = self.0.syncing.borrow_mut().take() else { return };
        // A newer result than the one the sync was planned from: its rows are other rows.
        if !self.0.done.borrow().as_ref().is_some_and(|d| Rc::ptr_eq(d, &from)) {
            self.0.stale.set(true);
            return;
        }
        let outcomes = crate::sync::outcomes(&planned, report);
        let failed = outcomes.values().filter(|o| o.is_some()).count();
        let synced = outcomes.len() - failed;
        {
            let mut dirs = self.0.rows.dirs.borrow_mut();
            let mut user = self.0.rows.user.borrow_mut();
            for at in outcomes.keys() {
                if let Some(dir) = dirs.get_mut(*at as usize) {
                    *dir = Dir::Skip;
                }
                user.remove(at);
            }
        }
        self.0.rows.after.borrow_mut().extend(outcomes);
        *self.0.note.borrow_mut() = match failed {
            0 => format!("Synchronized {}", items_text(synced)),
            n => format!("Synchronized {} · {} not done (see their rows)", items_text(synced), group(n)),
        };
        if self.0.hidden.get() {
            self.show_chip();
        } else {
            self.0.rows.notify.reset();
            self.show_summary();
        }
    }

    /// Save Profile…: the two folders, the mode, the options and the filter under a name
    /// (settings.toml `[[sync-profiles]]`); Delete removes the profile of the name typed.
    fn save_profile(&self) {
        let mut dialogs = None;
        crate::operations::with_current(|ops| dialogs = Some(ops.dialogs()));
        let Some(dialogs) = dialogs else { return };
        let left = self.0.paths.borrow().0.clone();
        let name = self.0.profile.borrow().clone().unwrap_or_else(|| {
            left.file_name().map_or_else(|| left.display().to_string(), |n| n.to_string_lossy().into_owned())
        });
        let this = self.clone();
        let note =
            |text: &str| if text.trim().is_empty() { ("Type a name".to_owned(), true) } else { (String::new(), false) };
        let message = "A name for the two folders, the mode, the options and the filter. The palette opens it as \
                       \"Sync: <name>\", where it only compares.";
        let more = dialogs.clone();
        dialogs.ask_text_choice("Save Profile", message, name, &["Save", "Delete", "Cancel"], note, move |answer| {
            let Some((button, name)) = answer else { return };
            let name = name.trim().to_owned();
            if name.is_empty() {
                return this.say("A profile needs a name");
            }
            let list = crate::sync::profiles();
            let same = |p: &gezik_config::sync_profiles::SyncProfile| p.name.to_lowercase() == name.to_lowercase();
            if button == 1 {
                if !list.iter().any(same) {
                    return this.say(&format!("No sync profile is called \"{name}\""));
                }
                let list = list.into_iter().filter(|p| !same(p)).collect();
                crate::sync::write_profiles(list);
                return this.say(&format!("Deleted the sync profile \"{name}\""));
            }
            let opened = this.0.profile.borrow().as_ref().is_some_and(|p| p.to_lowercase() == name.to_lowercase());
            if list.iter().any(same) && !opened {
                let again = this.clone();
                let asked = format!("A sync profile called \"{name}\" exists. Replace it?");
                return more.ask("Save Profile", asked, &["Replace", "Cancel"], move |choice| {
                    if choice == Some(0) {
                        again.write_profile(name);
                    }
                });
            }
            this.write_profile(name);
        });
    }

    fn write_profile(&self, name: String) {
        let dirs = gezik_config::paths::KnownDirs::system();
        let (left, right) = self.0.paths.borrow().clone();
        if nested(&left, &right) {
            return self.say(NESTED);
        }
        let profile = gezik_config::sync_profiles::SyncProfile {
            name: name.clone(),
            left: dirs.collapse(&left),
            right: dirs.collapse(&right),
            options: self.0.options.get(),
            filter: self.0.filter_text.borrow().clone(),
        };
        let Some(list) = crate::sync::with_profile(crate::sync::profiles(), profile) else {
            return self.say("At most 40 sync profiles: delete one first");
        };
        crate::sync::write_profiles(list);
        *self.0.profile.borrow_mut() = Some(name.clone());
        self.say(&format!("Saved the sync profile \"{name}\""));
    }

    /// The layer's keys: Esc stops a running read, else closes; the row keys, → / ← open and
    /// close, Enter, F5 / Ctrl+R again, Ctrl+C, Ctrl+A (⌘ on macOS); the field keeps its own.
    pub fn chord(&self, chord: &Chord, field_focused: bool) -> bool {
        let primary = crate::keys::is_primary(chord, Platform::current());
        let plain = !primary && !chord.alt;
        match chord.key {
            Key::Escape if plain && !chord.shift => {
                if self.0.running.get() {
                    self.0.stop.borrow().store(true, Ordering::Relaxed);
                } else {
                    self.close();
                }
            }
            Key::F(5) if plain && !chord.shift => self.start(),
            Key::Char('r') if primary && !chord.shift => self.start(),
            // Enter in the filter applies it.
            Key::Enter if field_focused && plain && !chord.shift => self.start(),
            Key::Enter if primary && !chord.alt && !chord.shift => self.synchronize(),
            _ if field_focused => return false,
            Key::Right if primary && !chord.alt && !chord.shift => self.set_dirs(|_, _| Dir::ToRight, false),
            Key::Left if primary && !chord.alt && !chord.shift => self.set_dirs(|_, _| Dir::ToLeft, false),
            // Marks a one-sided item for the trash (⌫ on a Mac keyboard); a pair has no delete.
            Key::Delete | Key::Backspace if plain && !chord.shift => self.set_dirs(
                |row, dir| match (row.left, row.right) {
                    (Some(_), None) => Dir::ToLeft,
                    (None, Some(_)) => Dir::ToRight,
                    _ => dir,
                },
                false,
            ),
            Key::Space if plain && !chord.shift => self.set_dirs(|_, _| Dir::Skip, false),
            Key::Space if plain => self.reset_selected(),
            Key::Up | Key::Down | Key::PageUp | Key::PageDown | Key::Home | Key::End if plain => {
                let Some(window) = self.0.window.upgrade() else { return true };
                let len = self.0.rows.shown.borrow().len();
                if let Some(to) =
                    crate::checksums::stepped(chord.key, usize::try_from(window.get_tool_cursor()).ok(), len)
                {
                    self.pressed(i32::try_from(to).unwrap_or(0), false, chord.shift);
                }
            }
            Key::Right | Key::Left if plain && !chord.shift => {
                if let Some(at) = self.cursor_row().filter(|at| self.is_folder(*at)) {
                    self.open_folder(at, Some(chord.key == Key::Right));
                }
            }
            Key::Enter if plain && !chord.shift => self.enter(),
            Key::Char('c') if primary && !chord.shift => self.copy(),
            Key::Char('a') if primary && !chord.shift => self.select_all(),
            _ => return false,
        }
        true
    }
}

fn toggled(flag: &mut bool) -> bool {
    *flag = !*flag;
    true
}

/// `1,234`.
fn group(n: usize) -> String {
    crate::preview::with_commas(n)
}

fn folders_text(n: usize) -> String {
    if n == 1 { "1 folder".to_owned() } else { format!("{} folders", group(n)) }
}

fn items_text(n: usize) -> String {
    if n == 1 { "1 item".to_owned() } else { format!("{} items", group(n)) }
}

/// What the reading thread is given.
struct Job {
    left: PathBuf,
    right: PathBuf,
    options: Options,
    filter: FileFilter,
    shown: (bool, bool),
    stop: Arc<AtomicBool>,
}

/// Reads both sides at once, compares them and (asked) reads the same-size pairs; `say` hears
/// how far it is (at most every 250 ms). Err: what to say instead of rows.
fn run(job: &Job, say: &mut dyn FnMut(String)) -> Result<Done, String> {
    // Each side by its own folder's name (11i's `top_folder`): a taken folder pattern it does
    // not match leaves its own files out; a left-out pattern it matches leaves it all out.
    let rules = |root: &Path| {
        let name = root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        job.filter.top_folder(&name).map(|root_taken| Rules {
            subfolders: job.options.subfolders,
            filter: &job.filter,
            root_taken,
            shown: job.shown,
            case_sensitive: CASE_SENSITIVE,
            max_items: tree_scan::MAX_ITEMS,
        })
    };
    let read = AtomicUsize::new(0);
    let (left, right) = std::thread::scope(|s| {
        let scan_side = |root: &Path| {
            let (rules, stop, read) = (rules(root), &job.stop, &read);
            let root = root.to_path_buf();
            s.spawn(move || match rules {
                Some(rules) => {
                    gezik_platform::priority::lower_this_thread();
                    tree_scan::scan(&root, &rules, stop, read)
                }
                None => tree_scan::left_out(),
            })
        };
        let (l, r) = (scan_side(&job.left), scan_side(&job.right));
        let mut last = Instant::now();
        // Looked at often enough that a close is heard within 100 ms.
        while !(l.is_finished() && r.is_finished()) {
            std::thread::sleep(Duration::from_millis(50));
            if last.elapsed() >= PROGRESS_EVERY {
                last = Instant::now();
                say(format!("Reading… {} folders", group(read.load(Ordering::Relaxed))));
            }
        }
        // A panic aborts the process (panic=abort): a join has nothing else to give.
        (l.join().unwrap_or_default(), r.join().unwrap_or_default())
    });
    scanned(&job.left, &left).and(scanned(&job.right, &right))?;
    let mut rows = compare::build(&left.tree, &right.tree, &job.options);
    let unreadable = read_contents(job, &left.tree, &right.tree, &mut rows, say);
    compare::settle_folders(&mut rows);
    let done = Done {
        left: left.tree,
        right: right.tree,
        rows,
        unreadable,
        links: left.links + right.links,
        online: left.online + right.online,
        bins: [has_bin(&job.left), has_bin(&job.right)],
        whole: job.filter.is_empty() && job.shown == (true, true),
    };
    Ok(done)
}

/// A side's scan as an error to say, if it is one.
fn scanned(root: &Path, scan: &Scan) -> Result<(), String> {
    if scan.stopped {
        return Err("Stopped. Compare Again (F5) reads both folders again".to_owned());
    }
    if scan.too_many {
        return Err(format!(
            "More than {} items in {}; narrow the filter or compare subfolders",
            group(tree_scan::MAX_ITEMS),
            root.display()
        ));
    }
    if scan.tree.nodes.first().is_some_and(|n| n.kind == Kind::Unread) {
        let why = scan.tree.problem(0).unwrap_or("it cannot be read");
        return Err(format!("Cannot read {}: {why}", root.display()));
    }
    Ok(())
}

/// Reads the pairs `rows` marks byte by byte (`hash::same_content`); the answers go into the
/// rows. A stop leaves the rest marked. Returns why a pair could not be read, by row.
fn read_contents(
    job: &Job,
    left: &Tree,
    right: &Tree,
    rows: &mut [Row],
    say: &mut dyn FnMut(String),
) -> HashMap<u32, String> {
    let mut problems = HashMap::new();
    let total: u64 = rows.iter().filter(|r| r.check).filter_map(|r| r.left).map(|i| left.nodes[i as usize].size).sum();
    let total_text = crate::view_options::size_text(total);
    let mut done = 0u64;
    let mut last = Instant::now();
    for (at, row) in rows.iter_mut().enumerate() {
        if !row.check {
            continue;
        }
        if job.stop.load(Ordering::Relaxed) {
            break;
        }
        let (Some(l), Some(r)) = (row.left, row.right) else { continue };
        let a = tree_scan::path_of(&job.left, left, l);
        let b = tree_scan::path_of(&job.right, right, r);
        let mut progress = |n: u64| {
            done += n;
            if last.elapsed() >= PROGRESS_EVERY {
                last = Instant::now();
                say(format!("Comparing content… {} of {total_text}", crate::view_options::size_text(done)));
            }
            !job.stop.load(Ordering::Relaxed)
        };
        match gezik_ops::hash::same_content(&a, &b, false, &mut progress) {
            Ok(first) => compare::content_result(row, first.is_none()),
            Err(_) if job.stop.load(Ordering::Relaxed) => break,
            Err(err) => {
                row.check = false;
                row.state = State::Unreadable;
                problems.insert(u32::try_from(at).unwrap_or(u32::MAX), gezik_platform::fs::describe(&err));
            }
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gezik-folder-compare-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, text: &str, seconds: u64) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
        let file = std::fs::File::options().write(true).open(path).unwrap();
        file.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)).unwrap();
    }

    fn job(left: &Path, right: &Path, options: Options, filter: &str) -> Job {
        Job {
            left: left.to_path_buf(),
            right: right.to_path_buf(),
            options,
            filter: FileFilter::parse(filter, false).unwrap(),
            shown: (true, true),
            stop: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Each row's name and state.
    fn found(done: &Done) -> Vec<(String, State)> {
        done.rows
            .iter()
            .map(|r| {
                let names = match (r.left, r.right) {
                    (Some(i), _) => done.left.names(i),
                    (_, Some(i)) => done.right.names(i),
                    _ => Vec::new(),
                };
                (names.join("/"), r.state)
            })
            .collect()
    }

    const T: u64 = 1_700_000_000;

    #[test]
    fn two_trees_are_compared_with_their_content_when_asked() {
        let root = dir("content");
        let (left, right) = (root.join("L"), root.join("R"));
        write(&left.join("same.txt"), "abc", T);
        write(&right.join("same.txt"), "abc", T + 1);
        write(&left.join("moved.txt"), "abc", T + 600);
        write(&right.join("moved.txt"), "abc", T);
        write(&left.join("edited.txt"), "abc", T + 600);
        write(&right.join("edited.txt"), "abd", T);
        write(&left.join("sub/only.txt"), "x", T);
        write(&right.join("sub/keep.txt"), "x", T);
        write(&right.join("skip/x.tmp"), "x", T);
        let plain = run(&job(&left, &right, Options::default(), "| skip\\"), &mut |_| {}).unwrap();
        assert_eq!(
            found(&plain),
            [
                ("edited.txt".to_owned(), State::LeftNewer),
                ("moved.txt".to_owned(), State::LeftNewer),
                ("same.txt".to_owned(), State::Equal),
                ("sub".to_owned(), State::Differs),
                ("sub/keep.txt".to_owned(), State::RightOnly),
                ("sub/only.txt".to_owned(), State::LeftOnly),
            ],
            "the filter's folder is not there"
        );
        let content = Options { content: true, ..Options::default() };
        let mut heard = Vec::new();
        let done = run(&job(&left, &right, content, "| skip\\"), &mut |text| heard.push(text)).unwrap();
        let states = found(&done);
        assert_eq!(states[0].1, State::LeftNewer, "other bytes keep the date's state");
        assert_eq!(states[1].1, State::Equal, "the same bytes are the same");
        assert!(done.rows[1].dates_differ);
        assert_eq!(tool_row(&done, 1, false, false, Mark::default()).status, "Same content");
        assert!(done.rows.iter().all(|r| !r.check));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_stop_says_so_and_a_missing_folder_is_said() {
        let root = dir("stop");
        write(&root.join("L/a.txt"), "a", T);
        write(&root.join("R/a.txt"), "a", T);
        let stopped = job(&root.join("L"), &root.join("R"), Options::default(), "");
        stopped.stop.store(true, Ordering::Relaxed);
        assert!(run(&stopped, &mut |_| {}).err().unwrap().starts_with("Stopped"));
        let missing = job(&root.join("L"), &root.join("nope"), Options::default(), "");
        assert!(run(&missing, &mut |_| {}).err().unwrap().starts_with("Cannot read"));
        // A stop while contents are read leaves the rest marked.
        let content = Options { content: true, ignore_dates: true, ..Options::default() };
        let late = job(&root.join("L"), &root.join("R"), content, "");
        let (left, right) = (scan_for(&late, "L", &root), scan_for(&late, "R", &root));
        let mut rows = compare::build(&left, &right, &content);
        assert!(rows[0].check);
        late.stop.store(true, Ordering::Relaxed);
        read_contents(&late, &left, &right, &mut rows, &mut |_| {});
        assert!(rows[0].check, "not read");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn each_side_takes_the_root_filter_by_its_own_name() {
        let root = dir("roots");
        let (left, right) = (root.join("src"), root.join("lib"));
        write(&left.join("a.txt"), "a", T);
        write(&right.join("a.txt"), "a", T);
        write(&right.join("src/b.txt"), "b", T);
        write(&right.join("docs/c.txt"), "c", T);
        // `src` is taken: the left (named src) gives its files, the right only its src folder.
        let done = run(&job(&left, &right, Options::default(), "src\\"), &mut |_| {}).unwrap();
        assert_eq!(
            found(&done),
            [
                ("a.txt".to_owned(), State::LeftOnly),
                ("src".to_owned(), State::RightOnly),
                ("src/b.txt".to_owned(), State::RightOnly)
            ]
        );
        // `src` left out: the left folder is out as a whole.
        let done = run(&job(&left, &right, Options::default(), "| src\\"), &mut |_| {}).unwrap();
        assert_eq!(
            found(&done),
            [
                ("a.txt".to_owned(), State::RightOnly),
                ("docs".to_owned(), State::RightOnly),
                ("docs/c.txt".to_owned(), State::RightOnly)
            ]
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_failed_run_drops_the_earlier_result() {
        let root = dir("failed");
        write(&root.join("L/a.txt"), "a", T);
        write(&root.join("R/b.txt"), "b", T);
        let layer = FolderCompare::new(
            slint::Weak::default(),
            (root.join("L"), root.join("R")),
            Options::default(),
            String::new(),
        );
        let ok = run(&job(&root.join("L"), &root.join("R"), Options::default(), ""), &mut |_| {});
        layer.0.generation.set(1);
        layer.heard(1, Message::Done(Box::new(ok)));
        assert_eq!(layer.0.rows.row_count(), 2);
        assert!(layer.summary().0.contains("2 items compared"));
        for why in
            ["Stopped. Compare Again (F5) reads both folders again", "Cannot read x: gone", "More than 2,000,000 items"]
        {
            layer.heard(1, Message::Done(Box::new(Err(why.to_owned()))));
            assert_eq!(layer.summary().0, why);
            assert_eq!(layer.0.rows.row_count(), 0, "the old rows are not shown as this run's");
            assert!(layer.0.done.borrow().is_none());
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn esc_on_a_running_compare_stops_it_and_says_so() {
        let layer =
            FolderCompare::new(slint::Weak::default(), ("/l".into(), "/r".into()), Options::default(), String::new());
        layer.0.running.set(true);
        layer.0.generation.set(7);
        let stop = layer.0.stop.borrow().clone();
        let esc = Chord { ctrl: false, alt: false, shift: false, meta: false, key: Key::Escape };
        assert!(layer.chord(&esc, false));
        assert!(stop.load(Ordering::Relaxed), "the reading threads are told to stop");
        assert!(layer.0.running.get(), "the layer stays: the threads answer");
        // What the stopped threads answer is what shows.
        let root = dir("esc");
        write(&root.join("L/a.txt"), "a", T);
        write(&root.join("R/a.txt"), "a", T);
        let stopped = job(&root.join("L"), &root.join("R"), Options::default(), "");
        stopped.stop.store(true, Ordering::Relaxed);
        layer.heard(7, Message::Done(Box::new(run(&stopped, &mut |_| {}))));
        assert!(!layer.0.running.get());
        assert!(layer.summary().0.starts_with("Stopped"), "{}", layer.summary().0);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn only_a_job_in_the_compared_folders_makes_it_stale() {
        let roots = (PathBuf::from("/w/left"), PathBuf::from("/x/right"));
        assert!(touches(&[PathBuf::from("/w/left")], &roots));
        assert!(touches(&[PathBuf::from("/x/right/sub/deep")], &roots));
        assert!(!touches(&[PathBuf::from("/w/leftover"), PathBuf::from("/x")], &roots), "next to it, above it");
        assert!(!touches(&[], &roots), "a job that changed no folder (Checksums)");
    }

    fn scan_for(job: &Job, side: &str, root: &Path) -> Tree {
        let rules = Rules {
            subfolders: true,
            filter: &job.filter,
            root_taken: true,
            shown: (true, true),
            case_sensitive: true,
            max_items: tree_scan::MAX_ITEMS,
        };
        tree_scan::scan(&root.join(side), &rules, &AtomicBool::new(false), &AtomicUsize::new(0)).tree
    }

    #[test]
    fn a_row_shows_its_direction_and_what_became_of_it() {
        let root = dir("marks");
        write(&root.join("L/a.txt"), "a", T + 600);
        write(&root.join("R/a.txt"), "b", T);
        write(&root.join("R/z.txt"), "z", T);
        let done = run(&job(&root.join("L"), &root.join("R"), Options::default(), ""), &mut |_| {}).unwrap();
        let values = |row: ToolRow| row.values.to_string();
        let mark = |dir, user, after| Mark { dir, user, after };
        let a = tool_row(&done, 0, false, false, mark(Dir::ToRight, false, None));
        assert!(values(a.clone()).contains(" → "), "{}", a.values);
        assert_eq!(a.status, "Left newer");
        let z = tool_row(&done, 1, false, false, mark(Dir::ToRight, true, None));
        assert!(values(z.clone()).contains("×→"), "{}", z.values);
        assert_eq!(z.status, "Only right *");
        let skipped = tool_row(&done, 1, false, false, mark(Dir::Skip, false, None));
        assert!(!values(skipped).contains('←'), "an empty direction shows nothing");
        let synced = tool_row(&done, 0, false, false, mark(Dir::Skip, false, Some(None)));
        assert_eq!((synced.status.as_str(), synced.tone), ("Synchronized", 0));
        let failed = tool_row(&done, 0, false, false, mark(Dir::Skip, false, Some(Some("Access denied".into()))));
        assert_eq!((failed.status.as_str(), failed.tone), ("Not done: Access denied", 2));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_sync_needs_two_separate_folders_and_knows_what_the_compare_saw() {
        let root = dir("whole");
        write(&root.join("L/a.txt"), "a", T);
        write(&root.join("R/a.txt"), "a", T);
        assert!(nested(&root.join("L"), &root.join("L")));
        assert!(nested(&root.join("L"), &root.join("L").join("sub")));
        assert!(nested(&root.join("L").join("sub"), &root.join("L")));
        assert!(!nested(&root.join("L"), &root.join("R")));
        let plain = run(&job(&root.join("L"), &root.join("R"), Options::default(), ""), &mut |_| {}).unwrap();
        assert!(plain.whole);
        assert_eq!(plain.bins, [has_bin(&root), has_bin(&root)]);
        let filtered = run(&job(&root.join("L"), &root.join("R"), Options::default(), "*.txt"), &mut |_| {}).unwrap();
        assert!(!filtered.whole, "a filter: folders go to the trash file by file");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The 11e screen test's abort: Mode (clicked inside the button strip) replaced the strip's
    /// model, destroying the running button; a later Close aborted. The strip now changes in place.
    #[test]
    fn mode_and_the_sync_buttons_keep_the_button_strip_and_close_cleanly() {
        crate::windows::tests::platform();
        let window = crate::AppWindow::new().expect("a headless window");
        let id = crate::windows::install(window.clone_strong());
        crate::checksums::install(&window);
        let root = dir("strip");
        write(&root.join("L/a.txt"), "a", T + 600);
        write(&root.join("R/a.txt"), "b", T);
        write(&root.join("R/z.txt"), "z", T);
        let (l, r) = (root.join("L"), root.join("R"));
        crate::windows::enter(id, || {
            let layer = FolderCompare::new(window.as_weak(), (l.clone(), r.clone()), Options::default(), String::new());
            CURRENT.with(|s| s.set(id, layer.clone()));
            layer.show();
            layer.0.generation.set(3);
            layer.0.running.set(true);
            layer.heard(3, Message::Done(Box::new(run(&job(&l, &r, Options::default(), ""), &mut |_| {}))));
            let strip = window.get_tool_buttons();
            let at = |name: &str| i32::try_from(BUTTONS.iter().position(|b| *b == name).unwrap()).unwrap();
            window.invoke_tool_button(at("Mode"));
            assert!(window.get_tool_buttons() == strip, "the same model");
            assert_eq!(strip.row_data(2).unwrap(), "Mode: Left to right");
            window.invoke_tool_button(at("Mode"));
            assert_eq!(strip.row_data(2).unwrap(), "Mode: Mirror");
            window.set_tool_cursor(1);
            window.invoke_tool_button(at("Direction"));
            window.invoke_tool_button(at("Synchronize…"));
            window.invoke_tool_button(at("Close"));
            assert!(!window.get_tool_open());
            assert!(CURRENT.with(|s| s.get(id)).is_none());
        });
        let _ = std::fs::remove_dir_all(&root);
    }
}
