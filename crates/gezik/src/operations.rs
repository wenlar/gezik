//! File operations in the app: the engine, the operations panel and its summary, the taskbar,
//! and what happens when a job ends (refresh, select, report problems).

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use gezik_config::settings::{BatchRenameState, FilesSettings};
use gezik_config::store::ConfigStore;
use gezik_core::drag::Effect;
use gezik_core::format_size;
use gezik_core::ops::paths::same_path;
use gezik_core::ops::rate::{Rate, format_eta, format_rate};
use gezik_ops::{
    Answer, CopyTask, DeleteTask, Engine, Event, Failure, JobId, JobState, MoveTask, NewTask, PauseReason, Progress,
    Question, Report, Settings, Task, TrashTask,
};
use gezik_platform::clipboard::{self, ClipboardError, ClipboardFiles};
use gezik_platform::taskbar::{Taskbar, TaskbarState};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::dialog::Dialogs;
use crate::navigation::{Navigator, sync_model};
use crate::sidebar::Sidebar;
use crate::view::View;
use crate::{AppWindow, OpRow};

/// A job shows in the panel only if it still runs after this long.
const SHOW_AFTER: Duration = Duration::from_secs(1);
/// A finished row without problems stays this long.
const DONE_FOR: Duration = Duration::from_secs(3);
/// "Details" lists at most this many failures.
const MAX_DETAILS: usize = 50;

/// How a row looks; the numbers are `OpRow.state` in ops-panel.slint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowState {
    Running = 0,
    Waiting = 1,
    Paused = 2,
    Deciding = 3,
    Done = 4,
    Failed = 5,
}

/// How far a job is, 0–1: by bytes when it has any, else by items.
fn fraction(progress: &Progress) -> f32 {
    let ratio = |done: u64, total: u64| (done as f64 / total as f64).clamp(0.0, 1.0) as f32;
    if progress.bytes_total > 0 {
        ratio(progress.bytes_done, progress.bytes_total)
    } else if progress.items_total > 0 {
        ratio(progress.items_done, progress.items_total)
    } else {
        0.0
    }
}

/// A row's state, its text after the title, and its bar (below 0: not known yet).
pub fn describe(
    progress: Option<&Progress>,
    report: Option<&Report>,
    speed: Option<f64>,
    left: Option<Duration>,
) -> (RowState, String, f32) {
    if let Some(report) = report {
        return match report.failures.len() {
            _ if report.cancelled => (RowState::Done, "Cancelled".to_owned(), 1.0),
            // Left out on purpose (no password given, a link): a note, not a failure.
            0 => match report.skipped.len() {
                0 => (RowState::Done, "Done".to_owned(), 1.0),
                1 => (RowState::Done, "Done · 1 item skipped".to_owned(), 1.0),
                n => (RowState::Done, format!("Done · {n} items skipped"), 1.0),
            },
            1 => (RowState::Failed, "1 item failed".to_owned(), 1.0),
            n => (RowState::Failed, format!("{n} items failed"), 1.0),
        };
    }
    let Some(progress) = progress else { return (RowState::Waiting, "Starting".to_owned(), -1.0) };
    let done = fraction(progress);
    match progress.state {
        JobState::Waiting => (RowState::Waiting, "Waiting for the drive".to_owned(), -1.0),
        JobState::Scanning => {
            let size = format_size(progress.bytes_total);
            (RowState::Running, format!("Scanning… {} items · {size}", progress.items_total), -1.0)
        }
        JobState::Deciding => (RowState::Deciding, "Waiting for your decisions".to_owned(), done),
        JobState::Paused(PauseReason::User) => (RowState::Paused, "Paused".to_owned(), done),
        JobState::Paused(PauseReason::DiskFull) => (RowState::Paused, "The disk is full".to_owned(), done),
        JobState::Paused(PauseReason::ManyFailures) => (RowState::Paused, "Paused after errors".to_owned(), done),
        JobState::Running => {
            let mut text = format!("{}%", (done * 100.0).floor() as u32);
            if let Some(speed) = speed.filter(|s| *s > 0.0 && progress.bytes_total > 0) {
                text.push_str(&format!(" · {}", format_rate(speed)));
            }
            if let Some(left) = left {
                text.push_str(&format!(" · {}", format_eta(left)));
            }
            (RowState::Running, text, done)
        }
    }
}

/// The status bar's line for the panel: `2 operations · 61%`; `3 items failed` once only
/// failed rows are left; empty when the panel has nothing.
pub fn summary_text(running: usize, done: Option<f32>, failed_items: usize, rows: usize) -> String {
    if rows == 0 {
        return String::new();
    }
    if running == 0 {
        return match failed_items {
            0 => "Operations done".to_owned(),
            1 => "1 item failed".to_owned(),
            n => format!("{n} items failed"),
        };
    }
    let what = if running == 1 { "1 operation".to_owned() } else { format!("{running} operations") };
    match done {
        Some(done) => format!("{what} · {}%", (done * 100.0).floor() as u32),
        None => what,
    }
}

/// The taskbar button for these rows (state, bar): paused if any waits for the user, red if
/// any failed; off when nothing runs or failed.
pub fn taskbar_progress(rows: &[(RowState, f32)]) -> (TaskbarState, u64, u64) {
    let active: Vec<&(RowState, f32)> = rows.iter().filter(|(state, _)| *state != RowState::Done).collect();
    if active.is_empty() {
        return (TaskbarState::Off, 0, 0);
    }
    let state = if active.iter().any(|(s, _)| *s == RowState::Failed) {
        TaskbarState::Error
    } else if active.iter().any(|(s, _)| matches!(s, RowState::Paused | RowState::Deciding)) {
        TaskbarState::Paused
    } else {
        TaskbarState::Normal
    };
    let total = active.len() as u64 * 1000;
    let done: u64 = active.iter().map(|(_, f)| (f.clamp(0.0, 1.0) * 1000.0) as u64).sum();
    (state, done, total)
}

/// Whether `typed` can replace the name `old`: `Ok(None)` if it stays the same, `Ok(Some(name))`
/// with surrounding spaces trimmed, or the problem to show. `taken` says whether another entry
/// in the folder already has a name.
pub fn rename_check(typed: &str, old: &str, taken: impl Fn(&str) -> bool) -> Result<Option<String>, String> {
    let name = typed.trim();
    if name == old {
        return Ok(None);
    }
    gezik_core::ops::names::validate_name(name, gezik_core::ops::names::NameRules::current())
        .map_err(|err| err.to_string())?;
    if taken(name) {
        return Err("A file with this name already exists".to_owned());
    }
    Ok(Some(name.to_owned()))
}

/// The names in `folder` among `results` (to select them there).
pub fn result_names(results: &[PathBuf], folder: &Path) -> Vec<String> {
    results
        .iter()
        .filter(|path| path.parent().is_some_and(|parent| same_path(parent, folder)))
        .filter_map(|path| path.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect()
}

/// A drive or volume root (`C:`, `/`): it has no parent or no name.
pub fn is_root(path: &Path) -> bool {
    path.parent().is_none() || path.file_name().is_none()
}

/// `notlar.txt`, or `3 items` (for questions like "Delete 3 items permanently?").
pub fn items_text(paths: &[PathBuf]) -> String {
    match paths {
        [one] => one.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| one.display().to_string()),
        many => format!("{} items", many.len()),
    }
}

thread_local! {
    static CURRENT: RefCell<Option<Operations>> = const { RefCell::new(None) };
}

/// Runs `f` with this UI thread's operations, if set up.
pub fn with_current(f: impl FnOnce(&Operations)) {
    if let Some(ops) = CURRENT.with(|c| c.borrow().clone()) {
        f(&ops);
    }
}

/// What ended a rename.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Commit {
    Enter,
    Tab,
    Blur,
}

/// What to do with a job's results once their folder shows them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum After {
    #[default]
    Select,
    /// Select and rename the first result (a new folder or file).
    Rename,
    Nothing,
}

type Retry = Rc<dyn Fn() -> Box<dyn Task>>;
/// Runs a failed operation again however it was started (a chain of tasks).
type Again = Rc<dyn Fn()>;

struct JobView {
    id: JobId,
    title: String,
    background: bool,
    shown: bool,
    progress: Option<Progress>,
    rate: Rate,
    report: Option<Report>,
    retry: Option<Retry>,
    again: Option<Again>,
    after: After,
    /// The folder rows were hidden in (trash, delete): reloaded when the job ends, whatever it did.
    hidden_in: Option<PathBuf>,
}

impl JobView {
    fn new(id: JobId, title: String) -> JobView {
        JobView {
            id,
            title,
            background: false,
            shown: false,
            progress: None,
            rate: Rate::new(Duration::from_secs(5)),
            report: None,
            retry: None,
            again: None,
            after: After::Nothing,
            hidden_in: None,
        }
    }

    fn row(&self) -> OpRow {
        let left = self.progress.as_ref().and_then(|p| self.rate.remaining(p.bytes_total.saturating_sub(p.bytes_done)));
        let (state, detail, progress) =
            describe(self.progress.as_ref(), self.report.as_ref(), self.rate.per_second(), left);
        let finished = self.report.is_some();
        let failed = state == RowState::Failed;
        let paused_by_user =
            matches!(self.progress.as_ref().map(|p| p.state), Some(JobState::Paused(PauseReason::User)));
        OpRow {
            id: i32::try_from(self.id).unwrap_or(i32::MAX),
            title: self.title.clone().into(),
            detail: detail.into(),
            progress,
            state: state as i32,
            can_pause: !finished && matches!(state, RowState::Running),
            can_resume: !finished && paused_by_user,
            can_start_now: !finished && state == RowState::Waiting && self.progress.is_some(),
            can_retry: failed && (self.retry.is_some() || self.again.is_some()),
            can_details: failed || self.report.as_ref().is_some_and(|r| !r.cancelled && !r.skipped.is_empty()),
            finished,
        }
    }
}

struct Inner {
    window: slint::Weak<AppWindow>,
    engine: Engine,
    nav: Navigator,
    view: View,
    sidebar: Sidebar,
    dialogs: Dialogs,
    rows: Rc<VecModel<OpRow>>,
    jobs: RefCell<Vec<JobView>>,
    files: Cell<FilesSettings>,
    collapsed: Cell<bool>,
    taskbar: RefCell<Option<Taskbar>>,
    /// Jobs whose pause already has a question (several workers may report the same pause).
    asked: RefCell<HashSet<JobId>>,
    /// A new entry to rename once its folder shows it.
    rename_when_shown: RefCell<Option<PathBuf>>,
    /// What Gezik put on the clipboard (the only clipboard where the system has none for files).
    clip: RefCell<Option<ClipboardFiles>>,
    /// Paths on the clipboard as cut (faded in the list), and the clipboard's change number
    /// when that was read.
    cut: RefCell<Vec<PathBuf>>,
    clip_sequence: Cell<u64>,
    conflicts: crate::conflicts::Conflicts,
    /// Where state.toml is (none: nothing is saved).
    store: Option<ConfigStore>,
    /// The batch rename layer's last rules.
    batch_last: RefCell<BatchRenameState>,
}

#[derive(Clone)]
pub struct Operations(Rc<Inner>);

impl Operations {
    #[allow(clippy::too_many_arguments, reason = "the parts of the app it works with")]
    pub fn new(
        window: &AppWindow,
        nav: Navigator,
        view: View,
        sidebar: Sidebar,
        dialogs: Dialogs,
        settings: Settings,
        files: FilesSettings,
        collapsed: bool,
        store: Option<ConfigStore>,
        batch_last: BatchRenameState,
    ) -> Operations {
        // One wake-up for a burst of events: the flag is cleared just before draining.
        let waiting = Arc::new(AtomicBool::new(false));
        let engine = Engine::new(settings, {
            let waiting = waiting.clone();
            move || {
                if !waiting.swap(true, Ordering::SeqCst) {
                    let waiting = waiting.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        waiting.store(false, Ordering::SeqCst);
                        with_current(|ops| ops.drain());
                    });
                }
            }
        });
        let conflicts = crate::conflicts::Conflicts::new(window, engine.clone());
        let rows = Rc::new(VecModel::default());
        window.set_op_rows(ModelRc::from(rows.clone()));
        let ops = Operations(Rc::new(Inner {
            window: window.as_weak(),
            engine,
            nav,
            view,
            sidebar,
            dialogs,
            rows,
            jobs: RefCell::default(),
            files: Cell::new(files),
            collapsed: Cell::new(collapsed),
            taskbar: RefCell::default(),
            asked: RefCell::default(),
            rename_when_shown: RefCell::default(),
            clip: RefCell::default(),
            cut: RefCell::default(),
            clip_sequence: Cell::new(0),
            conflicts,
            store,
            batch_last: RefCell::new(batch_last),
        }));
        ops.0.view.on_shown({
            let weak = Rc::downgrade(&ops.0);
            move || {
                if let Some(inner) = weak.upgrade() {
                    let ops = Operations(inner);
                    ops.folder_shown();
                    ops.update_cut();
                }
            }
        });
        CURRENT.with(|c| *c.borrow_mut() = Some(ops.clone()));
        ops
    }

    /// `[files]` changed.
    pub fn set_files(&self, files: FilesSettings) {
        self.0.files.set(files);
        self.0.engine.set_threads(files.copy_threads);
    }

    pub fn conflicts(&self) -> &crate::conflicts::Conflicts {
        &self.0.conflicts
    }

    pub fn collapsed(&self) -> bool {
        self.0.collapsed.get()
    }

    pub fn toggle_collapsed(&self) {
        self.0.collapsed.set(!self.0.collapsed.get());
        self.update();
    }

    /// Runs `task`; `retry` runs the same operation again from its row, `after` says what to
    /// do with the results.
    pub fn submit(&self, task: Box<dyn Task>, retry: Option<Retry>, after: After) -> JobId {
        let title = task.title();
        let id = self.0.engine.submit(task);
        let mut job = JobView::new(id, title);
        job.retry = retry;
        job.after = after;
        self.0.jobs.borrow_mut().push(job);
        self.show_later(id);
        id
    }

    /// Runs `tasks` as one job, undone as one action (`label`: what Undo says); `again` runs
    /// it again from its row.
    pub fn submit_chain(
        &self,
        tasks: Vec<Box<dyn Task>>,
        label: Option<String>,
        again: Option<Again>,
        after: After,
    ) -> JobId {
        let title = tasks.first().map(|task| task.title()).unwrap_or_default();
        let id = self.0.engine.submit_chain(tasks, label);
        let mut job = JobView::new(id, title);
        job.again = again;
        job.after = after;
        self.0.jobs.borrow_mut().push(job);
        self.show_later(id);
        id
    }

    /// A folder is on screen: start a rename that waited for it (a new folder).
    fn folder_shown(&self) {
        let Some(path) = self.0.rename_when_shown.borrow().clone() else { return };
        let Some(folder) = self.0.view.folder() else { return };
        if !path.parent().is_some_and(|parent| same_path(parent, &folder)) {
            // Another folder is on screen: the wait is over.
            self.0.rename_when_shown.borrow_mut().take();
            return;
        }
        // A rename is open: wait for it to end.
        if self.0.view.renaming().is_some() {
            return;
        }
        self.0.rename_when_shown.borrow_mut().take();
        if let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) {
            self.0.view.begin_rename_by_name(&name);
        }
    }

    /// F2: renames the selected entry in place, or opens batch rename for two or more.
    pub fn rename_start(&self) {
        if self.0.view.shows_drives() {
            return;
        }
        let view = &self.0.view;
        if view.selection_count() >= 2 {
            self.batch_rename();
            return;
        }
        if let Some(index) = view.single_selected().or_else(|| view.focus()) {
            view.begin_rename(index);
        }
    }

    /// The batch rename layer for the selection (also for one item: the batch-rename shortcut).
    pub fn batch_rename(&self) {
        if self.0.view.shows_drives() {
            return;
        }
        let items = self.0.view.selected_entries();
        if items.is_empty() {
            return;
        }
        let others = self.0.view.all_names();
        let last = self.0.batch_last.borrow().clone();
        crate::batch_rename::with_current(|layer| {
            // Already open (a menu over it): keep what is being edited.
            if !layer.is_open() {
                layer.open(items, others, last);
            }
        });
    }

    /// The rules used last; written to state.toml now (the layer may be used once per run).
    pub fn save_batch_rename(&self, state: BatchRenameState) {
        *self.0.batch_last.borrow_mut() = state.clone();
        if let Some(store) = &self.0.store {
            let mut saved = store.load_state();
            saved.batch_rename = Some(state);
            if let Err(err) = store.save_state(&saved) {
                eprintln!("gezik: cannot save the rename rules: {err}");
            }
        }
    }

    /// The saved rule sets, written to settings.toml.
    pub fn save_rename_presets(&self, presets: &[gezik_config::settings::RenamePreset]) {
        if let Some(store) = &self.0.store
            && let Err(warning) = store.save_rename_presets(presets)
        {
            self.0.view.note(warning.to_string());
        }
    }

    /// Asks for a text over the window; `f` gets it when Save is chosen.
    pub fn ask_text(&self, title: &str, message: &str, f: impl FnOnce(String) + 'static) {
        self.0.dialogs.ask_text(title, message, "", &["Save", "Cancel"], move |text| {
            if let Some(text) = text {
                f(text);
            }
        });
    }

    fn set_rename_error(&self, error: &str) {
        if let Some(window) = self.0.window.upgrade() {
            window.set_rename_error(error.into());
        }
    }

    /// While typing: say at once what is wrong with the name.
    pub fn rename_edited(&self, typed: &str) {
        let Some((index, old)) = self.0.view.renaming() else { return };
        let error =
            rename_check(typed, &old, |name| self.0.view.has_other_named(name, index)).err().unwrap_or_default();
        self.set_rename_error(&error);
    }

    /// Ends renaming with `typed`: the entry's index if the field closed (unchanged or
    /// renamed), `None` if the name cannot be used (the field stays, with the problem shown).
    fn commit_rename(&self, typed: &str, how: Commit) -> Option<usize> {
        let view = &self.0.view;
        let (index, old) = view.renaming()?;
        // Enter and Esc leave the keyboard with the list; a blur or Tab does not.
        let refocus = how == Commit::Enter;
        match rename_check(typed, &old, |name| view.has_other_named(name, index)) {
            Err(_) if how == Commit::Blur => {
                // The field lost the focus with a name that cannot be used: keep the old one.
                view.end_rename(false);
                None
            }
            Err(error) => {
                self.set_rename_error(&error);
                None
            }
            Ok(None) => {
                if how == Commit::Tab {
                    view.end_rename_for_next()
                } else {
                    view.end_rename(refocus)
                }
                Some(index)
            }
            Ok(Some(name)) => {
                let path = view.entry_path(index).map(|(path, _)| path);
                if how == Commit::Tab {
                    view.end_rename_for_next()
                } else {
                    view.end_rename(refocus)
                };
                if let Some(path) = path {
                    // Going on to another entry: its refresh must not pull the selection away.
                    let after = if how == Commit::Tab { After::Nothing } else { After::Select };
                    self.submit(Box::new(gezik_ops::RenameTask::one(path, &name)), None, after);
                }
                Some(index)
            }
        }
    }

    pub fn rename_accepted(&self, typed: String) {
        self.commit_rename(&typed, Commit::Enter);
    }

    /// The field lost the keyboard to something else: keep a usable name, else the old one.
    pub fn rename_blurred(&self, typed: String, generation: i32) {
        // A late blur of an earlier rename's field (Tab went on) is not this rename's.
        if generation != self.0.view.rename_generation() {
            return;
        }
        self.commit_rename(&typed, Commit::Blur);
    }

    /// For a key or press while a rename may be open: if its field has lost the keyboard (it
    /// was destroyed with its row, which fires no blur), ends it like a blur. Returns whether
    /// a rename is open with its field focused.
    pub fn end_unfocused_rename(&self) -> bool {
        let Some(window) = self.0.window.upgrade() else { return false };
        if self.0.view.renaming().is_none() {
            return false;
        }
        if window.get_rename_focused() {
            return true;
        }
        self.rename_blurred(window.get_rename_text().into(), self.0.view.rename_generation());
        false
    }

    pub fn rename_cancelled(&self) {
        self.0.view.end_rename(true);
    }

    /// Tab / Shift+Tab: keep the name and rename the next / previous entry.
    pub fn rename_tab(&self, typed: String, back: bool) {
        let Some(index) = self.commit_rename(&typed, Commit::Tab) else { return };
        let next = if back { index.checked_sub(1) } else { Some(index + 1) };
        if !next.is_some_and(|next| self.0.view.begin_rename(next))
            && let Some(window) = self.0.window.upgrade()
        {
            window.invoke_focus_list();
        }
    }

    /// A new folder in `dir` (else the folder shown), renamed right away.
    pub fn new_folder(&self, dir: Option<PathBuf>) {
        if let Some(dir) = dir.or_else(|| self.0.view.folder()) {
            self.submit(Box::new(NewTask::folder(&dir)), None, After::Rename);
        }
    }

    /// Ctrl+C / Ctrl+X on the selection.
    pub fn copy(&self, cut: bool) {
        if self.0.view.shows_drives() {
            return;
        }
        self.copy_paths(self.0.view.selected_paths(), cut);
    }

    pub fn copy_paths(&self, paths: Vec<PathBuf>, cut: bool) {
        let paths = self.without_roots(paths, if cut { "cut" } else { "copy" });
        if paths.is_empty() {
            return;
        }
        match clipboard::write_files(&paths, cut) {
            Ok(()) | Err(ClipboardError::Unsupported) => {}
            Err(ClipboardError::Failed(why)) => return self.0.view.note(format!("Cannot use the clipboard: {why}")),
        }
        *self.0.cut.borrow_mut() = if cut { paths.clone() } else { Vec::new() };
        *self.0.clip.borrow_mut() = Some(ClipboardFiles { paths, cut });
        self.0.clip_sequence.set(clipboard::sequence());
        self.update_cut();
    }

    /// What paste would take: the system clipboard, or Gezik's own where the system has none.
    fn clipboard(&self) -> Option<ClipboardFiles> {
        match clipboard::read_files() {
            Ok(files) => files,
            Err(ClipboardError::Unsupported) => self.0.clip.borrow().clone(),
            Err(ClipboardError::Failed(_)) => None,
        }
    }

    pub fn can_paste(&self) -> bool {
        self.clipboard().is_some()
    }

    /// Ctrl+V: into `into` (a folder's menu) or the folder shown. Cut items move; `force_move`
    /// moves copied ones too (macOS Cmd+Option+V).
    pub fn paste(&self, into: Option<PathBuf>, force_move: bool) {
        let Some(dir) = into.or_else(|| self.0.view.folder()) else { return };
        let Some(ClipboardFiles { paths, cut }) = self.clipboard() else { return };
        self.transfer(paths, dir, if cut || force_move { Effect::Move } else { Effect::Copy });
        if cut {
            // Pasted: cut items are no longer waiting anywhere (also Explorer's own).
            let _ = clipboard::clear();
            self.0.clip.borrow_mut().take();
            self.0.cut.borrow_mut().clear();
            self.0.clip_sequence.set(clipboard::sequence());
            self.update_cut();
        }
    }

    /// Copies or moves `paths` into folder `dir` (a paste or a drop), as one undoable job.
    pub fn transfer(&self, paths: Vec<PathBuf>, dir: PathBuf, effect: Effect) {
        let retry: Retry = Rc::new(move || -> Box<dyn Task> {
            match effect {
                Effect::Move => Box::new(MoveTask::into(paths.clone(), &dir)),
                Effect::Copy => Box::new(CopyTask::into(paths.clone(), &dir)),
            }
        });
        self.submit(retry(), Some(retry), After::Select);
    }

    /// The clipboard may have changed in another program: re-read what is cut there (when
    /// the window gets the focus, and before a menu).
    pub fn clipboard_check(&self) {
        let sequence = clipboard::sequence();
        if sequence == 0 || sequence == self.0.clip_sequence.get() {
            return;
        }
        self.0.clip_sequence.set(sequence);
        let cut = match clipboard::read_files() {
            Ok(Some(files)) if files.cut => files.paths,
            _ => Vec::new(),
        };
        *self.0.cut.borrow_mut() = cut;
        self.update_cut();
    }

    /// Fades the cut items of the folder shown.
    fn update_cut(&self) {
        let names: HashSet<String> = match self.0.view.folder() {
            Some(folder) => result_names(&self.0.cut.borrow(), &folder).into_iter().collect(),
            None => HashSet::new(),
        };
        self.0.view.set_cut_names(names);
    }

    /// Delete / Shift+Delete on the selection.
    pub fn trash(&self, permanent: bool) {
        if self.0.view.shows_drives() {
            return;
        }
        self.trash_paths(self.0.view.selected_paths(), permanent);
    }

    pub fn trash_paths(&self, paths: Vec<PathBuf>, permanent: bool) {
        self.trash_with(paths, permanent, false);
    }

    /// Like `trash_paths`, but moving to the bin asks first even if the setting says not to
    /// (for what was not picked in the list: Explorer's Delete on a sidebar entry).
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn trash_asking(&self, paths: Vec<PathBuf>, permanent: bool) {
        self.trash_with(paths, permanent, true);
    }

    fn trash_with(&self, paths: Vec<PathBuf>, permanent: bool, always_ask: bool) {
        let paths = self.without_roots(paths, if permanent { "delete" } else { "trash" });
        if paths.is_empty() {
            return;
        }
        let what = items_text(&paths);
        let ops = self.clone();
        if permanent {
            self.0.dialogs.ask(
                format!("Delete {what} permanently?"),
                "This cannot be undone.",
                &["Delete", "Cancel"],
                move |choice| {
                    if choice == Some(0) {
                        ops.delete_now(paths);
                    }
                },
            );
        } else if always_ask || self.0.files.get().confirm_trash {
            let bin = if cfg!(windows) { "Recycle Bin" } else { "Trash" };
            self.0.dialogs.ask(format!("Move {what} to the {bin}?"), "", &["Trash", "Cancel"], move |choice| {
                if choice == Some(0) {
                    ops.trash_now(paths);
                }
            });
        } else {
            self.trash_now(paths);
        }
    }

    /// Hides the rows of `paths` in the folder shown; the folder, to reload when the job ends.
    fn hide(&self, paths: &[PathBuf]) -> Option<PathBuf> {
        let folder = self.0.view.folder()?;
        self.0.view.hide_names(&result_names(paths, &folder));
        Some(folder)
    }

    /// `paths` without drive roots (no copying, moving or deleting those); says so when that
    /// leaves nothing.
    fn without_roots(&self, paths: Vec<PathBuf>, what: &str) -> Vec<PathBuf> {
        let asked = paths.len();
        let kept: Vec<PathBuf> = paths.into_iter().filter(|path| !is_root(path)).collect();
        if kept.is_empty() && asked > 0 {
            self.0.view.note(format!("Cannot {what} a drive"));
        }
        kept
    }

    fn trash_now(&self, paths: Vec<PathBuf>) {
        let hidden_in = self.hide(&paths);
        let retry: Retry = {
            let paths = paths.clone();
            Rc::new(move || -> Box<dyn Task> { Box::new(TrashTask::new(paths.clone())) })
        };
        let id = self.submit(retry(), Some(retry), After::Nothing);
        self.with_job(id, |job| job.hidden_in = hidden_in);
    }

    fn delete_now(&self, paths: Vec<PathBuf>) {
        let hidden_in = self.hide(&paths);
        let pending = self.0.engine.pending_deletes();
        let retry: Retry = {
            let paths = paths.clone();
            Rc::new(move || -> Box<dyn Task> { Box::new(DeleteTask::new(paths.clone(), pending.clone())) })
        };
        let id = self.submit(retry(), Some(retry), After::Nothing);
        self.with_job(id, |job| job.hidden_in = hidden_in);
    }

    /// A copy of each selected item next to it.
    pub fn duplicate(&self) {
        if self.0.view.shows_drives() {
            return;
        }
        let paths = self.without_roots(self.0.view.selected_paths(), "duplicate");
        if paths.is_empty() {
            return;
        }
        let retry: Retry = Rc::new(move || -> Box<dyn Task> { Box::new(CopyTask::duplicate(paths.clone())) });
        self.submit(retry(), Some(retry), After::Select);
    }

    pub fn new_file(&self, dir: Option<PathBuf>) {
        if let Some(dir) = dir.or_else(|| self.0.view.folder()) {
            self.submit(Box::new(NewTask::file(&dir)), None, After::Rename);
        }
    }

    pub fn undo(&self) {
        if self.0.engine.undo().is_none() {
            self.0.view.note("Nothing to undo".to_owned());
        }
    }

    pub fn redo(&self) {
        if self.0.engine.redo().is_none() {
            self.0.view.note("Nothing to redo".to_owned());
        }
    }

    pub fn undo_label(&self) -> Option<String> {
        self.0.engine.undo_label()
    }

    pub fn redo_label(&self) -> Option<String> {
        self.0.engine.redo_label()
    }

    /// Finishes deletes an earlier run left unfinished.
    pub fn recover(&self) {
        let engine = self.0.engine.clone();
        std::thread::spawn(move || {
            let _ = engine.recover_deletes();
        });
    }

    fn show_later(&self, id: JobId) {
        let ops = self.clone();
        slint::Timer::single_shot(SHOW_AFTER, move || {
            if let Some(job) = ops.0.jobs.borrow_mut().iter_mut().find(|j| j.id == id && j.report.is_none()) {
                job.shown = true;
            }
            ops.update();
        });
    }

    fn with_job(&self, id: JobId, f: impl FnOnce(&mut JobView)) {
        if let Some(job) = self.0.jobs.borrow_mut().iter_mut().find(|j| j.id == id) {
            f(job);
        }
    }

    /// Handles everything the engine reported.
    pub fn drain(&self) {
        for event in self.0.engine.drain() {
            match event {
                Event::Added { job, title, background, .. } => {
                    let known = self.0.jobs.borrow().iter().any(|j| j.id == job);
                    if !known {
                        let mut view = JobView::new(job, title);
                        view.background = background;
                        self.0.jobs.borrow_mut().push(view);
                        self.show_later(job);
                    } else {
                        self.with_job(job, |j| j.background = background);
                    }
                }
                Event::Progress { job, progress } => self.with_job(job, |j| {
                    j.rate.record(Instant::now(), progress.bytes_done);
                    j.progress = Some(progress);
                }),
                Event::Conflicts { job, conflicts } => self.show_conflicts(job, conflicts),
                Event::Paused { job, reason, path } => self.paused(job, reason, path),
                Event::Question { job, id, question } => self.question(job, id, question),
                Event::Finished { job, report } => self.finished(job, report),
                Event::Changed { dirs } => {
                    self.0.nav.refresh_showing(&dirs, &[], None);
                }
                Event::History => {}
            }
        }
        self.update();
    }

    /// A job found existing items: the list shows them, the job waits for Start.
    fn show_conflicts(&self, job: JobId, conflicts: Vec<gezik_ops::ConflictItem>) {
        let mut title = String::new();
        self.with_job(job, |j| {
            j.shown = true;
            title = j.title.clone();
        });
        if self.0.collapsed.get() {
            self.0.collapsed.set(false);
        }
        self.0.conflicts.open(job, &title, conflicts);
    }

    /// A job asks something (an archive's password, whether to go on); its answer goes back
    /// to it. Several questions wait their turn.
    fn question(&self, job: JobId, id: u64, question: Question) {
        self.with_job(job, |j| j.shown = true);
        let engine = self.0.engine.clone();
        match question {
            Question::Password { archive, retry } => {
                let (title, message) = crate::archives::password_text(&archive, retry);
                self.0.dialogs.ask_password(job, title, message, &["OK", "Skip"], move |text| {
                    engine.answer(job, id, text.map_or(Answer::Cancel, Answer::Text));
                });
            }
            Question::Confirm { title, message, buttons } => {
                let labels: Vec<&str> = buttons.iter().map(String::as_str).collect();
                let escape = labels.len().saturating_sub(1);
                self.0.dialogs.ask_for_job(job, title, message, &labels, escape, move |choice| {
                    engine.answer(job, id, choice.map_or(Answer::Cancel, Answer::Button));
                });
            }
        }
    }

    fn paused(&self, job: JobId, reason: PauseReason, path: Option<PathBuf>) {
        self.with_job(job, |j| j.shown = true);
        let (title, message) = match reason {
            PauseReason::User => return,
            PauseReason::DiskFull => (
                "The disk is full".to_owned(),
                format!(
                    "Free some space{}, then resume.",
                    path.as_deref().map(|p| format!(" for {}", p.display())).unwrap_or_default()
                ),
            ),
            PauseReason::ManyFailures => (
                "Many items failed in a row".to_owned(),
                "The drive may have been disconnected. Resume to go on, or cancel.".to_owned(),
            ),
        };
        if !self.0.asked.borrow_mut().insert(job) {
            return;
        }
        // Waiting for the user: the panel opens by itself.
        self.0.collapsed.set(false);
        let ops = self.clone();
        self.0.dialogs.ask_escape(title, message, &["Resume", "Cancel"], 0, move |choice| {
            ops.0.asked.borrow_mut().remove(&job);
            match choice {
                Some(0) => ops.0.engine.resume(job),
                _ => ops.0.engine.cancel(job),
            }
        });
    }

    fn finished(&self, id: JobId, report: Report) {
        self.0.conflicts.close_if(id);
        // Its questions are moot now (answering one is harmless: nothing waits for it).
        self.0.dialogs.forget_job(id);
        self.0.asked.borrow_mut().remove(&id);
        let problems = !report.cancelled && !report.failures.is_empty();
        let mut after = After::Nothing;
        let mut hidden_in = None;
        self.with_job(id, |job| {
            after = job.after;
            hidden_in = job.hidden_in.take();
            if problems {
                job.shown = true;
            }
            job.report = Some(report.clone());
        });
        if problems {
            // Something failed: the panel opens by itself.
            self.0.collapsed.set(false);
        }
        if after == After::Rename {
            *self.0.rename_when_shown.borrow_mut() = report.results.first().cloned();
        }
        // A rename is open (Tab went on): the selection stays with it.
        let after = if after == After::Select && self.0.view.renaming().is_some() { After::Nothing } else { after };
        let select = match (after, self.0.view.folder()) {
            (After::Nothing, _) | (_, None) => Vec::new(),
            (_, Some(folder)) => result_names(&report.results, &folder),
        };
        // Rows hidden for this job come back if it changed nothing (failed, cancelled, no trash).
        let mut dirs = report.changed_dirs.clone();
        dirs.extend(hidden_in);
        let note = (report.skipped_changed > 0).then(|| {
            let n = report.skipped_changed;
            let what = if n == 1 { "1 item".to_owned() } else { format!("{n} items") };
            format!("{what} changed since; skipped")
        });
        let reloading = self.0.nav.refresh_showing(&dirs, &select, note.clone());
        self.0.sidebar.refresh();
        if let (false, Some(note)) = (reloading, note) {
            self.0.view.note(note);
        }
        if !report.no_trash.is_empty() {
            self.ask_delete_for_good(report.no_trash.clone());
        }
        // A row with skipped items stays until closed, for its Details.
        let notes = !report.cancelled && !report.skipped.is_empty();
        if !problems && !notes {
            let ops = self.clone();
            slint::Timer::single_shot(DONE_FOR, move || ops.remove(id));
        }
        // An archive that needs 7-Zip, a download that is done.
        crate::archives::with_current(|archives| archives.job_finished(id, &report));
    }

    /// Items the trash cannot take (no trash on their drive, or a name it cannot take): delete
    /// them for good?
    fn ask_delete_for_good(&self, paths: Vec<PathBuf>) {
        let bin = if cfg!(windows) { "Recycle Bin" } else { "trash" };
        let title = if paths.len() == 1 {
            format!("{} cannot go to the {bin}", paths[0].file_name().map(|n| n.to_string_lossy()).unwrap_or_default())
        } else {
            format!("{} items cannot go to the {bin}", paths.len())
        };
        let ops = self.clone();
        self.0.dialogs.ask(
            title,
            format!("{} Delete permanently? This cannot be undone.", no_trash_reason(&paths, bin)),
            &["Delete", "Cancel"],
            move |choice| {
                if choice == Some(0) {
                    let pending = ops.0.engine.pending_deletes();
                    ops.submit(Box::new(DeleteTask::new(paths, pending)), None, After::Nothing);
                }
            },
        );
    }

    /// Takes a finished job's row away (its operation runs again in another).
    pub fn forget(&self, id: JobId) {
        self.remove(id);
    }

    fn remove(&self, id: JobId) {
        self.0.jobs.borrow_mut().retain(|j| j.id != id);
        self.update();
    }

    /// Redraws the panel, the summary and the taskbar.
    fn update(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        let jobs = self.0.jobs.borrow();
        let shown: Vec<&JobView> = jobs.iter().filter(|j| j.shown).collect();
        let rows: Vec<OpRow> = shown.iter().map(|j| j.row()).collect();
        let states: Vec<(RowState, f32)> = rows
            .iter()
            .map(|r| {
                let state = match r.state {
                    0 => RowState::Running,
                    1 => RowState::Waiting,
                    2 => RowState::Paused,
                    3 => RowState::Deciding,
                    5 => RowState::Failed,
                    _ => RowState::Done,
                };
                (state, r.progress.max(0.0))
            })
            .collect();
        let running: Vec<&(RowState, f32)> =
            states.iter().filter(|(s, _)| !matches!(s, RowState::Done | RowState::Failed)).collect();
        let overall =
            (!running.is_empty()).then(|| running.iter().map(|(_, f)| *f).sum::<f32>() / running.len() as f32);
        let failed_items: usize = shown.iter().filter_map(|j| j.report.as_ref()).map(|r| r.failures.len()).sum();
        window.set_ops_summary(summary_text(running.len(), overall, failed_items, rows.len()).into());
        window.set_ops_collapsed(self.0.collapsed.get());
        window.set_ops_panel_open(!rows.is_empty() && !self.0.collapsed.get());
        sync_model(&self.0.rows, rows.into_iter());
        let (state, done, total) = taskbar_progress(&states);
        let mut taskbar = self.0.taskbar.borrow_mut();
        if taskbar.is_none() && state != TaskbarState::Off {
            *taskbar = Some(Taskbar::new(&window.window().window_handle()));
        }
        if let Some(taskbar) = taskbar.as_ref() {
            taskbar.set(state, done, total);
        }
    }

    fn id(id: i32) -> JobId {
        JobId::try_from(id).unwrap_or(0)
    }

    pub fn pause(&self, id: i32) {
        self.0.engine.pause(Self::id(id));
    }

    pub fn resume(&self, id: i32) {
        self.0.asked.borrow_mut().remove(&Self::id(id));
        self.0.engine.resume(Self::id(id));
    }

    pub fn cancel(&self, id: i32) {
        self.0.engine.cancel(Self::id(id));
    }

    pub fn start_now(&self, id: i32) {
        self.0.engine.start_now(Self::id(id));
    }

    pub fn dismiss(&self, id: i32) {
        self.remove(Self::id(id));
    }

    /// Runs a failed row's operation again (what is done already shows as identical and is
    /// skipped).
    pub fn retry(&self, id: i32) {
        let id = Self::id(id);
        let (retry, again) = match self.0.jobs.borrow().iter().find(|j| j.id == id) {
            Some(job) => (job.retry.clone(), job.again.clone()),
            None => return,
        };
        if let Some(retry) = retry {
            self.remove(id);
            self.submit(retry(), Some(retry.clone()), After::Select);
        } else if let Some(again) = again {
            self.remove(id);
            again();
        }
    }

    /// The failures of a row, with Retry.
    pub fn details(&self, id: i32) {
        let id = Self::id(id);
        let (title, message, can_retry) = {
            let jobs = self.0.jobs.borrow();
            let Some(job) = jobs.iter().find(|j| j.id == id) else { return };
            let Some(report) = &job.report else { return };
            let named = |f: &Failure| {
                format!("{}: {}", f.path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default(), f.message)
            };
            let mut lines: Vec<String> = report.failures.iter().take(MAX_DETAILS).map(named).collect();
            // What was left out on purpose comes after, under its own heading.
            let room = MAX_DETAILS.saturating_sub(lines.len());
            if !report.skipped.is_empty() && room > 0 {
                if !lines.is_empty() {
                    lines.push(String::new());
                }
                lines.push("Skipped:".to_owned());
                lines.extend(report.skipped.iter().take(room).map(named));
            }
            let all = report.failures.len() + report.skipped.len();
            if all > MAX_DETAILS {
                lines.push(format!("…and {} more", all - MAX_DETAILS));
            }
            let can_retry = !report.failures.is_empty() && (job.retry.is_some() || job.again.is_some());
            (job.title.clone(), lines.join("\n"), can_retry)
        };
        let ops = self.clone();
        let buttons: &[&str] = if can_retry { &["Retry", "Close"] } else { &["Close"] };
        self.0.dialogs.ask(title, message, buttons, move |choice| {
            if can_retry && choice == Some(0) {
                ops.retry(i32::try_from(id).unwrap_or(0));
            }
        });
    }

    /// Asks before closing while operations run; `quit` runs if the user cancels them.
    /// Returns whether the window must stay open for now.
    pub fn confirm_close(&self, quit: impl FnOnce() + 'static) -> bool {
        if !self.0.engine.busy() {
            return false;
        }
        let running = self.0.jobs.borrow().iter().filter(|j| j.report.is_none() && !j.background).count().max(1);
        let title = if running == 1 {
            "1 operation is running".to_owned()
        } else {
            format!("{running} operations are running")
        };
        let engine = self.0.engine.clone();
        self.0.dialogs.ask_escape(
            title,
            "Quitting cancels them. What is already copied or moved stays.",
            &["Keep open", "Cancel them and quit"],
            0,
            move |choice| {
                if choice == Some(1) {
                    engine.cancel_all();
                    engine.wait_idle(Duration::from_secs(3));
                    quit();
                }
            },
        );
        true
    }
}

/// Why `paths` cannot go to the trash (`bin`): their drive has none, or (Windows) their
/// names end in a dot or a space.
fn no_trash_reason(paths: &[PathBuf], bin: &str) -> String {
    let named = paths.iter().filter(|path| !gezik_platform::fs::can_trash_name(path)).count();
    if named == 0 {
        format!("This drive has no {bin}.")
    } else if named == paths.len() {
        format!("The {bin} cannot take names that end in a dot or a space.")
    } else {
        format!("Some are on a drive without a {bin}, some have names that end in a dot or a space.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gezik_ops::{Failure, TaskKind};

    #[test]
    fn no_trash_reasons() {
        let bin = "Recycle Bin";
        assert_eq!(no_trash_reason(&[PathBuf::from("/usb/a.txt")], bin), "This drive has no Recycle Bin.");
        if cfg!(windows) {
            let dotted = PathBuf::from(r"C:\x.");
            assert_eq!(
                no_trash_reason(std::slice::from_ref(&dotted), bin),
                "The Recycle Bin cannot take names that end in a dot or a space."
            );
            assert!(no_trash_reason(&[dotted, PathBuf::from(r"E:\a")], bin).starts_with("Some are"));
        }
    }

    fn progress(state: JobState, items: (u64, u64), bytes: (u64, u64)) -> Progress {
        Progress { state, items_done: items.0, items_total: items.1, bytes_done: bytes.0, bytes_total: bytes.1 }
    }

    fn report(failures: usize, cancelled: bool) -> Report {
        Report {
            kind: TaskKind::Copy,
            cancelled,
            failures: (0..failures).map(|i| Failure { path: format!("/f{i}").into(), message: "x".into() }).collect(),
            skipped: Vec::new(),
            skipped_changed: 0,
            no_trash: Vec::new(),
            results: Vec::new(),
            changed_dirs: Vec::new(),
        }
    }

    #[test]
    fn rename_checks() {
        let taken = |name: &str| name == "b.txt";
        assert_eq!(rename_check("a.txt", "a.txt", taken), Ok(None));
        assert_eq!(rename_check("  c.txt ", "a.txt", taken), Ok(Some("c.txt".into())));
        assert_eq!(rename_check("b.txt", "a.txt", taken), Err("A file with this name already exists".into()));
        assert_eq!(rename_check("", "a.txt", taken), Err("Type a name".into()));
        assert!(rename_check("a/b", "a.txt", taken).is_err());
    }

    #[test]
    fn running_rows_show_percent_speed_and_time_left() {
        let p = progress(JobState::Running, (3, 10), (61, 100));
        let (state, text, bar) = describe(Some(&p), None, Some(84.0 * 1024.0 * 1024.0), Some(Duration::from_secs(42)));
        assert_eq!(state, RowState::Running);
        assert!(text.starts_with("61% · ") && text.ends_with(" · ~0:42"), "{text}");
        assert!((bar - 0.61).abs() < 0.001);
        let items_only = progress(JobState::Running, (1, 4), (0, 0));
        assert_eq!(describe(Some(&items_only), None, None, None).1, "25%");
    }

    #[test]
    fn other_states() {
        let scanning = progress(JobState::Scanning, (0, 12400), (0, 3 * 1024 * 1024 * 1024));
        let (_, text, bar) = describe(Some(&scanning), None, None, None);
        assert!(text.starts_with("Scanning… 12400 items · "), "{text}");
        assert!(bar < 0.0);
        let waiting = progress(JobState::Waiting, (0, 0), (0, 0));
        assert_eq!(describe(Some(&waiting), None, None, None).0, RowState::Waiting);
        let full = progress(JobState::Paused(PauseReason::DiskFull), (1, 2), (0, 0));
        assert_eq!(describe(Some(&full), None, None, None).1, "The disk is full");
        assert_eq!(describe(None, Some(&report(0, false)), None, None).1, "Done");
        assert_eq!(
            describe(None, Some(&report(3, false)), None, None),
            (RowState::Failed, "3 items failed".into(), 1.0)
        );
        assert_eq!(describe(None, Some(&report(3, true)), None, None).1, "Cancelled");
        let mut skipped = report(0, false);
        skipped.skipped.push(Failure { path: "/a.zip".into(), message: "no password".into() });
        assert_eq!(describe(None, Some(&skipped), None, None), (RowState::Done, "Done · 1 item skipped".into(), 1.0));
    }

    #[test]
    fn summary_line() {
        assert_eq!(summary_text(0, None, 0, 0), "");
        assert_eq!(summary_text(2, Some(0.615), 0, 2), "2 operations · 61%");
        assert_eq!(summary_text(1, None, 0, 1), "1 operation");
        assert_eq!(summary_text(0, None, 3, 1), "3 items failed");
        assert_eq!(summary_text(0, None, 0, 1), "Operations done");
    }

    #[test]
    fn taskbar_follows_the_rows() {
        assert_eq!(taskbar_progress(&[]), (TaskbarState::Off, 0, 0));
        assert_eq!(taskbar_progress(&[(RowState::Done, 1.0)]).0, TaskbarState::Off);
        assert_eq!(
            taskbar_progress(&[(RowState::Running, 0.5), (RowState::Running, 1.0)]),
            (TaskbarState::Normal, 1500, 2000)
        );
        assert_eq!(taskbar_progress(&[(RowState::Running, 0.5), (RowState::Deciding, 0.1)]).0, TaskbarState::Paused);
        assert_eq!(taskbar_progress(&[(RowState::Paused, 0.5), (RowState::Failed, 1.0)]).0, TaskbarState::Error);
    }

    #[test]
    fn items_read_well_in_questions() {
        assert_eq!(items_text(&[PathBuf::from("/a/notlar.txt")]), "notlar.txt");
        assert_eq!(items_text(&[PathBuf::from("/a"), PathBuf::from("/b")]), "2 items");
    }

    #[test]
    fn roots_are_not_files_to_work_on() {
        assert!(is_root(Path::new("/")));
        assert!(!is_root(Path::new("/a")));
        assert!(!is_root(Path::new("/a/b.txt")));
    }

    #[test]
    fn only_results_in_the_folder_are_selected() {
        let results = [PathBuf::from("/a/x.txt"), PathBuf::from("/b/y.txt"), PathBuf::from("/a/sub")];
        assert_eq!(result_names(&results, Path::new("/a")), ["x.txt", "sub"]);
    }
}
