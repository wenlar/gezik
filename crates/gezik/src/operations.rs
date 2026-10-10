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
use gezik_core::ops::paths::same_path;
use gezik_core::ops::rate::{Rate, format_eta, format_rate};
use gezik_core::templates::{LinkKind, PasteKind, Template, pasted_name};
use gezik_ops::{
    Answer, CopyTask, DeleteTask, Engine, Event, GroupTask, JobId, JobState, LinkTask, MaterializeTask, MoveTask,
    NewTask, PauseReason, Progress, Question, Report, RestoreTask, Settings, Task, TrashTask,
};
use gezik_platform::clipboard::{self, ClipboardError, ClipboardFiles};
use gezik_platform::dnd::VirtualFiles;
use gezik_platform::taskbar::{Taskbar, TaskbarState};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::dialog::Dialogs;
use crate::navigation::sync_model;
use crate::sidebar::Sidebar;
use crate::view::hidden_note;
use crate::{AppWindow, OpRow};

/// A job shows in the panel only if it still runs after this long.
const SHOW_AFTER: Duration = Duration::from_secs(1);
/// A finished row without problems stays this long.
const DONE_FOR: Duration = Duration::from_secs(3);
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
            let size = crate::view_options::size_text(progress.bytes_total);
            (RowState::Running, format!("Scanning… {} items · {size}", progress.items_total), -1.0)
        }
        JobState::Deciding => (RowState::Deciding, "Waiting for your decisions".to_owned(), done),
        JobState::Paused(PauseReason::User) => (RowState::Paused, "Paused".to_owned(), done),
        JobState::Paused(PauseReason::DiskFull) => (RowState::Paused, "The disk is full".to_owned(), done),
        JobState::Paused(PauseReason::ManyFailures) => (RowState::Paused, "Paused after errors".to_owned(), done),
        JobState::Running => {
            let mut text = format!("{}%", (done * 100.0).floor() as u32);
            if let Some(speed) = speed.filter(|s| *s > 0.0 && progress.bytes_total > 0) {
                text.push_str(&format!(" · {}", format_rate(speed, crate::view_options::current().size_format)));
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

/// The status bar when a job needs a folder and search results show (spec 4.6).
pub const NOT_HERE: &str = "Not in search results: open a folder first";

/// Whether "Copy/Cut with folders" still owns the clipboard: the change number it got (`saved`)
/// is the clipboard's now, or, where there are none (0), the clipboard holds its paths.
pub fn with_folders_pastes(saved: u64, now: u64, saved_paths: &[PathBuf], clipboard: Option<&[PathBuf]>) -> bool {
    if saved != 0 || now != 0 {
        return saved == now;
    }
    clipboard.is_some_and(|paths| paths == saved_paths)
}

/// "Copy/Cut with folders" (spec 4.6): the items with their paths under the scope.
struct WithFolders {
    items: Vec<(PathBuf, PathBuf)>,
    cut: bool,
    sequence: u64,
}

/// The names in `folder` that `results` are or are inside (a copy with folders made
/// `a/b/x`: `a` is selected), each once.
pub fn first_level_names(results: &[PathBuf], folder: &Path) -> Vec<String> {
    let depth = folder.components().count();
    let mut names: Vec<String> = Vec::new();
    for path in results.iter().filter(|path| gezik_core::ops::paths::is_within(path, folder)) {
        if let Some(name) = path.components().nth(depth).map(|c| c.as_os_str().to_string_lossy().into_owned())
            && !names.contains(&name)
        {
            names.push(name);
        }
    }
    names
}

/// The moves of `moved` that brought one of `paths` (the job's results) where it is: a folder
/// moved across drives records each file in it, which no row needs.
pub fn relevant_moves(moved: &[(PathBuf, PathBuf)], paths: &[PathBuf]) -> Vec<(PathBuf, PathBuf)> {
    let paths: HashSet<&PathBuf> = paths.iter().collect();
    moved.iter().filter(|(_, to)| paths.contains(to)).cloned().collect()
}

/// A job that works on files and folders: in search results, these only.
pub fn only_in_results(what: &str) -> String {
    format!("{what} works in search results")
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

/// Whether ending a rename gives the keyboard back to the list. Enter does; Tab does not
/// (the next entry's field takes it); a blur leaves it where it went, unless it went nowhere:
/// the window lost the focus (switching windows). Slint then keeps the field as the window's
/// focused item, the field goes away with the rename, and once the window is active again no
/// key would reach anything. A dialog over the window keeps the keyboard.
fn refocus_after(how: Commit, window_active: bool, dialog_open: bool) -> bool {
    match how {
        Commit::Enter => true,
        Commit::Tab => false,
        Commit::Blur => !window_active && !dialog_open,
    }
}

/// Whether the window has the system's keyboard focus (true where that is unknown).
fn window_active(window: &AppWindow) -> bool {
    use slint::winit_030::WinitWindowAccessor;
    window.window().with_winit_window(|native| native.has_focus()).unwrap_or(true)
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

pub(crate) type Retry = Rc<dyn Fn() -> Box<dyn Task>>;
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
    /// Search result rows hidden for the job (trash, delete): checked again when it ends.
    hidden_paths: Vec<PathBuf>,
    /// The search results it was started from, if any (spec 4.7).
    origin: Option<crate::search::ResultsKey>,
    /// Said after the detail in the panel ([`CANT_UNDO`]).
    note: Option<&'static str>,
    /// Works in the bins (Put Back, delete from the trash): the system hears when it ends.
    bins: bool,
}

/// The note of a `{files}` run in the panel (spec 7).
pub const CANT_UNDO: &str = "can't be undone";

/// A row's detail with the job's note after it: "Done · can't be undone".
pub fn with_note(detail: String, note: Option<&str>) -> String {
    match note {
        Some(note) => format!("{detail} · {note}"),
        None => detail,
    }
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
            hidden_paths: Vec::new(),
            origin: None,
            note: None,
            bins: false,
        }
    }

    /// The job ended with `report`. A row with failures or notes (skipped items, the job's own
    /// note) is shown even if the job ended before its row came up: else it would stay, hidden,
    /// until restart, or the note would go unread.
    fn finish(&mut self, report: Report) {
        if !report.cancelled && (!report.failures.is_empty() || !report.skipped.is_empty() || self.note.is_some()) {
            self.shown = true;
        }
        self.report = Some(report);
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
            detail: with_note(detail, self.note).into(),
            progress,
            state: state as i32,
            can_pause: !finished && matches!(state, RowState::Running),
            can_resume: !finished && paused_by_user,
            can_start_now: !finished && state == RowState::Waiting && self.progress.is_some(),
            can_retry: failed && (self.retry.is_some() || self.again.is_some()),
            can_details: failed
                || self
                    .report
                    .as_ref()
                    .is_some_and(|r| !r.cancelled && (!r.skipped.is_empty() || !r.unchecked.is_empty())),
            finished,
            ..OpRow::default()
        }
    }
}

struct Inner {
    window: slint::Weak<AppWindow>,
    engine: Engine,
    sidebar: Sidebar,
    dialogs: Dialogs,
    rows: Rc<VecModel<OpRow>>,
    jobs: RefCell<Vec<JobView>>,
    files: Cell<FilesSettings>,
    collapsed: Cell<bool>,
    /// The panel's tab: 0 Current, 1 History.
    tab: Cell<i32>,
    /// Finished jobs, newest first (allocated on the first one).
    history: RefCell<crate::op_history::History>,
    history_rows: Rc<VecModel<OpRow>>,
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
    /// The last "Copy/Cut with folders", while it may own the clipboard.
    with_folders: RefCell<Option<WithFolders>>,
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
        // Only an `Arc`: nothing starts until the user asks for an administrator operation.
        engine.set_elevator(Arc::new(crate::admin::SystemElevator::new(window)));
        let conflicts = crate::conflicts::Conflicts::new(window, engine.clone());
        let rows = Rc::new(VecModel::default());
        window.set_op_rows(ModelRc::from(rows.clone()));
        let history_rows = Rc::new(VecModel::default());
        window.set_history_rows(ModelRc::from(history_rows.clone()));
        let ops = Operations(Rc::new(Inner {
            window: window.as_weak(),
            engine,
            sidebar,
            dialogs,
            rows,
            jobs: RefCell::default(),
            files: Cell::new(files),
            collapsed: Cell::new(collapsed),
            tab: Cell::new(0),
            history: RefCell::default(),
            history_rows,
            taskbar: RefCell::default(),
            asked: RefCell::default(),
            rename_when_shown: RefCell::default(),
            clip: RefCell::default(),
            cut: RefCell::default(),
            clip_sequence: Cell::new(0),
            with_folders: RefCell::default(),
            conflicts,
            store,
            batch_last: RefCell::new(batch_last),
        }));
        CURRENT.with(|c| *c.borrow_mut() = Some(ops.clone()));
        ops
    }

    /// `[files]` changed.
    pub fn set_files(&self, files: FilesSettings) {
        self.0.files.set(files);
        self.0.engine.set_threads(files.copy_threads);
    }

    pub fn dialogs(&self) -> &Dialogs {
        &self.0.dialogs
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

    /// Keeps the selection shown for keypad / (`View::restore_remembered`) when a job is
    /// about to run on `sources`, if they are all in the folder shown: a paste or a drop from
    /// elsewhere, or a job for a folder left meanwhile, leaves what was remembered alone.
    pub fn remember_for(&self, sources: &[PathBuf]) {
        crate::panes::active_view().remember_selection_for(sources);
    }

    /// Runs `task`; `retry` runs the same operation again from its row, `after` says what to
    /// do with the results. The selection is not remembered here: see `remember_for`.
    pub fn submit(&self, task: Box<dyn Task>, retry: Option<Retry>, after: After) -> JobId {
        let title = task.title();
        let id = self.0.engine.submit(task);
        let mut job = JobView::new(id, title);
        crate::panes::with_active(|p| job.origin = p.search.results_key());
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
        crate::panes::with_active(|p| job.origin = p.search.results_key());
        job.again = again;
        job.after = after;
        self.0.jobs.borrow_mut().push(job);
        self.show_later(id);
        id
    }

    /// Puts `note` after job `id`'s detail in the panel.
    pub fn set_note(&self, id: JobId, note: &'static str) {
        if let Some(job) = self.0.jobs.borrow_mut().iter_mut().find(|job| job.id == id) {
            job.note = Some(note);
        }
    }

    /// A line in the status bar (the administrator's refusals).
    pub fn status(&self, text: String) {
        crate::panes::active_view().note(text);
    }

    /// The active pane shows a listing: a new entry waiting for it is renamed, cut items fade.
    pub fn shown(&self) {
        self.folder_shown();
        self.update_cut();
    }

    /// A folder is on screen: start a rename that waited for it (a new folder).
    fn folder_shown(&self) {
        let Some(path) = self.0.rename_when_shown.borrow().clone() else { return };
        let Some(folder) = crate::panes::active_view().folder() else { return };
        if !path.parent().is_some_and(|parent| same_path(parent, &folder)) {
            // Another folder is on screen: the wait is over.
            self.0.rename_when_shown.borrow_mut().take();
            return;
        }
        // A rename is open: wait for it to end.
        if crate::panes::active_view().renaming().is_some() {
            return;
        }
        self.0.rename_when_shown.borrow_mut().take();
        if let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) {
            // A new folder or file is named in the whole folder: the filter (which would most
            // likely hide "New folder") closes first.
            if crate::panes::active_view().filter_text().is_some() {
                crate::panes::with_active(|p| p.filter.close());
            }
            crate::panes::active_view().begin_rename_by_name(&name);
        }
    }

    /// F2: renames the selected entry in place, or opens batch rename for two or more.
    pub fn rename_start(&self) {
        if self.refused_in_trash() {
            return;
        }
        if crate::panes::active_view().shows_drives() {
            return;
        }
        let view = &crate::panes::active_view();
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
        if self.refused_in_trash() {
            return;
        }
        if crate::panes::active_view().shows_drives() {
            return;
        }
        let items = crate::panes::active_view().selected_entries();
        if items.is_empty() {
            return;
        }
        // Search results: the layer reads each item's folder itself (spec 4.6).
        let others = (!crate::panes::active_view().shows_results()).then(|| crate::panes::active_view().all_names());
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
            store.update_state(|saved| saved.batch_rename = Some(state));
        }
    }

    /// The saved rule sets, written to settings.toml by the settings writer thread; a failure
    /// is said in the status bar.
    pub fn save_rename_presets(&self, presets: &[gezik_config::settings::RenamePreset]) {
        let Some(store) = &self.0.store else { return };
        let change = gezik_config::settings_writer::SettingsChange::RenamePresets(presets.to_vec());
        store.write_settings(change, |result| {
            if let Err(warning) = result {
                let _ = slint::invoke_from_event_loop(move || {
                    crate::panes::with_active(|p| p.view.note(warning.to_string()));
                });
            }
        });
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
        crate::panes::edit(crate::panes::active_view().pane_id(), |d| d.rename_error = error.into());
    }

    /// While typing: say at once what is wrong with the name.
    pub fn rename_edited(&self, typed: &str) {
        let Some((index, old)) = crate::panes::active_view().renaming() else { return };
        let error = rename_check(typed, &old, |name| crate::panes::active_view().has_other_named(name, index))
            .err()
            .unwrap_or_default();
        self.set_rename_error(&error);
    }

    /// Ends renaming with `typed`: the entry's index if the field closed (unchanged or
    /// renamed), `None` if the name cannot be used (the field stays, with the problem shown).
    fn commit_rename(&self, typed: &str, how: Commit) -> Option<usize> {
        self.commit_rename_then(typed, how, how != Commit::Tab)
    }

    /// [`Operations::commit_rename`]; `select`: the renamed entry is selected once the rename
    /// is done.
    fn commit_rename_then(&self, typed: &str, how: Commit, select: bool) -> Option<usize> {
        let view = &crate::panes::active_view();
        let (index, old) = view.renaming()?;
        // Enter and Esc leave the keyboard with the list; a blur or Tab does not, but for a
        // window switch.
        let refocus = self
            .0
            .window
            .upgrade()
            .is_some_and(|window| refocus_after(how, window_active(&window), window.get_dialog_open()));
        match rename_check(typed, &old, |name| view.has_other_named(name, index)) {
            Err(_) if how == Commit::Blur => {
                // The field lost the focus with a name that cannot be used: keep the old one.
                view.end_rename(refocus);
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
                    self.remember_for(std::slice::from_ref(&path));
                    // Going on to another entry: its refresh must not pull the selection away.
                    let after = if select { After::Select } else { After::Nothing };
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
        if generation != crate::panes::active_view().rename_generation() {
            return;
        }
        self.commit_rename(&typed, Commit::Blur);
    }

    /// For a key or press while a rename may be open: if its field has lost the keyboard (it
    /// was destroyed with its row, which fires no blur), ends it like a blur. Returns whether
    /// a rename is open with its field focused.
    pub fn end_unfocused_rename(&self) -> bool {
        if self.0.window.upgrade().is_none() || crate::panes::active_view().renaming().is_none() {
            return false;
        }
        let mirror = crate::panes::mirror(crate::panes::active_view().pane_id());
        if mirror.focus.borrow().rename {
            return true;
        }
        // The press or key goes on (a row selected, type-ahead): the rename done later must
        // not pull the selection back to the renamed entry.
        let typed = mirror.rename_text.borrow().to_string();
        self.commit_rename_then(&typed, Commit::Blur, false);
        false
    }

    /// The active pane changes: a rename open in it ends as a blur ends it (spec 10 §4.10).
    pub fn end_rename_for_switch(&self) {
        let view = crate::panes::active_view();
        if view.renaming().is_none() {
            return;
        }
        let typed = crate::panes::mirror(view.pane_id()).rename_text.borrow().to_string();
        self.commit_rename_then(&typed, Commit::Blur, false);
    }

    /// Enter (`keep`) or Esc while a rename is open but its field is off screen: the typed
    /// name is kept, or the old one. Returns whether there was such a rename.
    pub fn off_screen_rename_key(&self, keep: bool) -> bool {
        if self.0.window.upgrade().is_none() || crate::panes::active_view().renaming().is_none() {
            return false;
        }
        let mirror = crate::panes::mirror(crate::panes::active_view().pane_id());
        if mirror.focus.borrow().rename {
            return false;
        }
        if keep {
            let typed = mirror.rename_text.borrow().to_string();
            // A name that cannot be used keeps the field: it comes back on screen with the
            // problem under it (and takes the keyboard again).
            if self.commit_rename(&typed, Commit::Enter).is_none()
                && let Some((index, _)) = crate::panes::active_view().renaming()
            {
                crate::panes::active_view().reveal(index);
            }
        } else {
            self.rename_cancelled();
        }
        true
    }

    pub fn rename_cancelled(&self) {
        crate::panes::active_view().end_rename(true);
    }

    /// Tab / Shift+Tab: keep the name and rename the next / previous entry.
    pub fn rename_tab(&self, typed: String, back: bool) {
        let Some(index) = self.commit_rename(&typed, Commit::Tab) else { return };
        let next = if back { index.checked_sub(1) } else { Some(index + 1) };
        if !next.is_some_and(|next| crate::panes::active_view().begin_rename(next))
            && let Some(window) = self.0.window.upgrade()
        {
            window.invoke_focus_list();
        }
    }

    /// A new folder in `dir` (else the folder shown), renamed right away.
    pub fn new_folder(&self, dir: Option<PathBuf>) {
        match dir.or_else(|| crate::panes::active_view().folder()) {
            Some(dir) => {
                self.submit(Box::new(NewTask::folder(&dir)), None, After::Rename);
            }
            None => self.not_here(),
        }
    }

    /// Ctrl+C / Ctrl+X on the selection.
    pub fn copy(&self, cut: bool) {
        if self.refused_in_trash() {
            return;
        }
        if crate::panes::active_view().shows_drives() {
            return;
        }
        // Pasted elsewhere, keypad / brings this selection back here.
        crate::panes::active_view().remember_selection();
        self.copy_paths(crate::panes::active_view().selected_paths(), cut);
    }

    pub fn copy_paths(&self, paths: Vec<PathBuf>, cut: bool) {
        self.0.with_folders.borrow_mut().take();
        let paths = self.without_roots(paths, if cut { "cut" } else { "copy" });
        if paths.is_empty() {
            return;
        }
        match clipboard::write_files(&paths, cut) {
            Ok(()) | Err(ClipboardError::Unsupported) => {}
            Err(ClipboardError::Failed(why)) => {
                return crate::panes::active_view().note(format!("Cannot use the clipboard: {why}"));
            }
        }
        *self.0.cut.borrow_mut() = if cut { paths.clone() } else { Vec::new() };
        *self.0.clip.borrow_mut() = Some(ClipboardFiles { paths, cut });
        self.0.clip_sequence.set(clipboard::sequence());
        self.update_cut();
    }

    /// `copy-with-folders` / `cut-with-folders` and the row menu (spec 4.6): the system's
    /// clipboard gets the plain paths (pasted elsewhere they land flat); Gezik's paste keeps the
    /// folders under the search's scope.
    pub fn copy_with_folders(&self, cut: bool) {
        if self.refused_in_trash() {
            return;
        }
        if !crate::panes::active_view().shows_results() {
            return crate::panes::active_view().note(only_in_results("Copy with folders"));
        }
        let items = crate::panes::active_view().selected_relative();
        if items.is_empty() {
            return;
        }
        let paths: Vec<PathBuf> = items.iter().map(|(path, _)| path.clone()).collect();
        self.copy_paths(paths.clone(), cut);
        // Only once the clipboard took them: else a paste would take these for what is there.
        if self.0.clip.borrow().as_ref().is_some_and(|clip| clip.paths == paths) {
            *self.0.with_folders.borrow_mut() = Some(WithFolders { items, cut, sequence: clipboard::sequence() });
        }
    }

    /// The copy with folders a paste takes, if it still owns the clipboard.
    fn with_folders_to_paste(&self) -> Option<(Vec<(PathBuf, PathBuf)>, bool)> {
        let saved = self.0.with_folders.borrow();
        let saved = saved.as_ref()?;
        let paths: Vec<PathBuf> = saved.items.iter().map(|(path, _)| path.clone()).collect();
        let clipboard = self.clipboard().map(|files| files.paths);
        with_folders_pastes(saved.sequence, clipboard::sequence(), &paths, clipboard.as_deref())
            .then(|| (saved.items.clone(), saved.cut))
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
        let Some(dir) = into.or_else(|| crate::panes::active_view().folder()) else { return self.not_here() };
        // A copy with folders keeps them (spec 4.6).
        if let Some((items, cut)) = self.with_folders_to_paste() {
            let sources: Vec<PathBuf> = items.iter().map(|(path, _)| path.clone()).collect();
            self.remember_for(&sources);
            let moving = cut || force_move;
            let retry: Retry = Rc::new(move || -> Box<dyn Task> {
                if moving {
                    Box::new(MoveTask::with_folders(items.clone(), &dir))
                } else {
                    Box::new(CopyTask::with_folders(items.clone(), &dir))
                }
            });
            self.submit(retry(), Some(retry), After::Select);
            if cut {
                let _ = clipboard::clear();
                self.0.clip.borrow_mut().take();
                self.0.cut.borrow_mut().clear();
                self.0.with_folders.borrow_mut().take();
                self.0.clip_sequence.set(clipboard::sequence());
                self.update_cut();
            }
            return;
        }
        // No files: the picture or text as a new file (spec 9.1).
        let Some(ClipboardFiles { paths, cut }) = self.clipboard() else { return self.paste_as_file(dir) };
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

    /// Says why a job that needs a folder does nothing while search results show.
    fn not_here(&self) {
        if crate::panes::active_view().shows_results() {
            crate::panes::active_view().note(NOT_HERE.to_owned());
        }
    }

    /// Copies, moves or links `paths` into folder `dir` (a paste or a drop), as one undoable
    /// job.
    pub fn transfer(&self, paths: Vec<PathBuf>, dir: PathBuf, effect: Effect) {
        self.transfer_job(paths, dir, effect);
    }

    /// Like `transfer`, giving the job (the drop stack follows a Move here by it).
    pub fn transfer_job(&self, paths: Vec<PathBuf>, dir: PathBuf, effect: Effect) -> JobId {
        self.remember_for(&paths);
        let retry: Retry = Rc::new(move || -> Box<dyn Task> {
            match effect {
                Effect::Move => Box::new(MoveTask::into(paths.clone(), &dir)),
                Effect::Copy => Box::new(CopyTask::into(paths.clone(), &dir)),
                Effect::Link => Box::new(LinkTask::into(paths.clone(), &dir, LinkKind::for_drops())),
            }
        });
        self.submit(retry(), Some(retry), After::Select)
    }

    /// Items another program offers with no file behind them (an attachment, a browser's
    /// picture, a promised file), written into `dir` as one job; Ctrl+Z trashes what it made
    /// (spec 9 §8.1). No retry: the source may be gone by then.
    pub fn materialize(&self, files: VirtualFiles, dir: PathBuf, count: usize) {
        self.submit(Box::new(MaterializeTask::new(files, &dir, count)), None, After::Select);
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
        crate::panes::active_view().set_cut(&self.0.cut.borrow());
    }

    /// Delete / Shift+Delete on the selection.
    pub fn trash(&self, permanent: bool) {
        if crate::panes::active_view().shows_drives() {
            return;
        }
        if crate::panes::active_view().shows_trash() {
            return crate::trash_view::delete_selection(&crate::panes::active_view());
        }
        self.trash_paths(crate::panes::active_view().selected_paths(), permanent);
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
        // Search results: their rows go at once; the check after the job brings back what stayed.
        if crate::panes::active_view().shows_results() {
            self.remember_for(paths);
            crate::panes::active_view().hide_paths(paths);
            return None;
        }
        let folder = crate::panes::active_view().folder()?;
        // Before the names go: the job is submitted after this.
        self.remember_for(paths);
        crate::panes::active_view().hide_names(&result_names(paths, &folder));
        Some(folder)
    }

    /// `paths` without drive roots (no copying, moving or deleting those); says so when that
    /// leaves nothing.
    fn without_roots(&self, paths: Vec<PathBuf>, what: &str) -> Vec<PathBuf> {
        let asked = paths.len();
        let kept: Vec<PathBuf> = paths.into_iter().filter(|path| !is_root(path)).collect();
        if kept.is_empty() && asked > 0 {
            crate::panes::active_view().note(format!("Cannot {what} a drive"));
        }
        kept
    }

    /// Says so and returns true while the trash is shown: what acts on the selection by its
    /// names (copy, rename, …) would act on `$R…` entries there (spec 7.1).
    fn refused_in_trash(&self) -> bool {
        let refused = crate::panes::active_view().shows_trash();
        if refused {
            crate::panes::active_view().note(crate::trash_view::not_here());
        }
        refused
    }

    fn trash_now(&self, paths: Vec<PathBuf>) {
        let retry: Retry = {
            let paths = paths.clone();
            Rc::new(move || -> Box<dyn Task> { Box::new(TrashTask::new(paths.clone())) })
        };
        self.run_hiding(paths, retry);
    }

    fn delete_now(&self, paths: Vec<PathBuf>) {
        let pending = self.0.engine.pending_deletes();
        let retry: Retry = {
            let paths = paths.clone();
            Rc::new(move || -> Box<dyn Task> { Box::new(DeleteTask::new(paths.clone(), pending.clone())) })
        };
        self.run_hiding(paths, retry);
    }

    /// Runs `retry`'s task; the rows of `paths` go at once and come back if it changes nothing.
    pub fn run_hiding(&self, paths: Vec<PathBuf>, retry: Retry) -> JobId {
        let hidden_in = self.hide(&paths);
        let id = self.submit(retry(), Some(retry), After::Nothing);
        let results = crate::panes::active_view().shows_results();
        self.with_job(id, |job| {
            job.hidden_in = hidden_in;
            if results {
                job.hidden_paths = paths;
            }
        });
        id
    }

    /// Put Back (spec 7.1): each (entry in the bin, where it goes). A name taken there is a
    /// conflict (never replaced unasked); a missing folder is made again.
    pub fn restore_from_trash(&self, pairs: Vec<(PathBuf, PathBuf)>) {
        let paths = pairs.iter().map(|(trashed, _)| trashed.clone()).collect();
        let retry: Retry = Rc::new(move || -> Box<dyn Task> { Box::new(RestoreTask::new(pairs.clone())) });
        let id = self.run_hiding(paths, retry);
        self.with_job(id, |job| job.bins = true);
    }

    /// Deletes items in the trash for good, each with its record (`items`: entry, record). The
    /// only way Gezik deletes from a bin: `DeleteTask::from_trash` refuses anything else.
    pub fn delete_from_trash(&self, items: Vec<(PathBuf, Option<PathBuf>)>) {
        let paths = items.iter().map(|(trashed, _)| trashed.clone()).collect();
        let pending = self.0.engine.pending_deletes();
        let retry: Retry =
            Rc::new(move || -> Box<dyn Task> { Box::new(DeleteTask::from_trash(items.clone(), pending.clone())) });
        let id = self.run_hiding(paths, retry);
        self.with_job(id, |job| job.bins = true);
    }

    /// A copy of each selected item next to it.
    pub fn duplicate(&self) {
        if self.refused_in_trash() {
            return;
        }
        if crate::panes::active_view().shows_drives() {
            return;
        }
        let paths = self.without_roots(crate::panes::active_view().selected_paths(), "duplicate");
        if paths.is_empty() {
            return;
        }
        self.remember_for(&paths);
        let retry: Retry = Rc::new(move || -> Box<dyn Task> { Box::new(CopyTask::duplicate(paths.clone())) });
        self.submit(retry(), Some(retry), After::Select);
    }

    /// An empty Markdown file in `dir`, renamed right away (spec 8.1).
    pub fn new_markdown(&self, dir: PathBuf) {
        self.submit(Box::new(NewTask::markdown(&dir)), None, After::Rename);
    }

    /// A copy of the user's `template` in `dir`, renamed right away (spec 8.1).
    pub fn new_from_template(&self, dir: PathBuf, template: &Template) {
        let Some(templates) = crate::templates::dir() else { return };
        let (source, is_dir) = (templates.join(&template.name), template.is_dir);
        let retry: Retry =
            Rc::new(move || -> Box<dyn Task> { Box::new(CopyTask::template(source.clone(), &dir, is_dir)) });
        self.submit(retry(), Some(retry), After::Rename);
    }

    /// `new-folder-with-selection`: the selected items into a new folder next to them.
    pub fn new_folder_with_selection(&self) {
        if crate::panes::active_view().shows_results() {
            return self.not_here();
        }
        if crate::panes::active_view().shows_drives() {
            return;
        }
        self.new_folder_with(crate::panes::active_view().selected_paths());
    }

    /// `paths` (those in the folder shown) moved into a new "New folder" there, as one job,
    /// and the folder renamed right away (spec 8.2).
    pub fn new_folder_with(&self, paths: Vec<PathBuf>) {
        let Some(dir) = crate::panes::active_view().folder() else { return self.not_here() };
        let paths: Vec<PathBuf> = self
            .without_roots(paths, "move")
            .into_iter()
            .filter(|path| path.parent().is_some_and(|parent| same_path(parent, &dir)))
            .collect();
        if paths.is_empty() {
            return crate::panes::active_view().note("Select the items to put in a new folder".to_owned());
        }
        self.remember_for(&paths);
        let retry: Retry = Rc::new(move || -> Box<dyn Task> { Box::new(GroupTask::new(paths.clone(), &dir)) });
        self.submit(retry(), Some(retry), After::Rename);
    }

    /// What paste would write as a file: nothing while there are files to paste (`can_paste`,
    /// which the caller has asked already: one clipboard query per menu).
    pub fn paste_as(&self, can_paste: bool) -> Option<PasteKind> {
        if can_paste { None } else { clipboard::paste_kind() }
    }

    /// The clipboard's picture (before its text: a browser's picture often carries both) as a
    /// new `Pasted image … .png` in `dir`, else its text as `Pasted text … .txt` (spec 9.1).
    /// The data is read here; the picture is encoded in the job.
    pub fn paste_as_file(&self, dir: PathBuf) {
        let Some(at) = gezik_platform::local_date_parts(std::time::SystemTime::now()) else { return };
        let failed = |err: ClipboardError| match err {
            ClipboardError::Failed(why) => Some(format!("Cannot use the clipboard: {why}")),
            ClipboardError::Unsupported => None,
        };
        // A picture that cannot be read does not end it: the text may still be there. What went
        // wrong is told only when nothing was pasted.
        let mut problem = None;
        match clipboard::read_image() {
            Ok(Some(image)) => {
                let name = pasted_name(PasteKind::Image, &at);
                self.submit(
                    Box::new(NewTask::with_contents(&dir, &name, move || image.png_bytes())),
                    None,
                    After::Select,
                );
                return;
            }
            Ok(None) => {}
            Err(err) => problem = failed(err),
        }
        match clipboard::read_text() {
            Ok(Some(text)) if !text.is_empty() => {
                let (name, bytes) = (pasted_name(PasteKind::Text, &at), text.into_bytes());
                self.submit(
                    Box::new(NewTask::with_contents(&dir, &name, move || Ok(bytes.clone()))),
                    None,
                    After::Select,
                );
                return;
            }
            Ok(_) => {}
            Err(err) => problem = failed(err).or(problem),
        }
        crate::panes::active_view().note(problem.unwrap_or_else(|| "Nothing to paste".to_owned()));
    }

    /// An alias whose original is gone (spec 9 §4.2): Finder's question; Delete Alias moves it
    /// to the Trash (Ctrl+Z brings it back) without asking a second time.
    pub fn missing_alias(&self, alias: PathBuf) {
        let name = alias.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let ops = self.clone();
        self.0.dialogs.ask(
            "The original item can't be found",
            format!("The alias \"{name}\" can't be opened."),
            &["Delete Alias", "OK"],
            move |chosen| {
                if chosen == Some(0) {
                    ops.trash_now(vec![alias]);
                }
            },
        );
    }

    /// Make Alias (⌃⌘A): an alias of each selected item (the focused one when none), next to it.
    /// macOS only: elsewhere there are no aliases to make.
    pub fn make_alias_of_selection(&self) {
        let view = &crate::panes::active_view();
        if !cfg!(target_os = "macos") || view.shows_drives() {
            return;
        }
        let mut items = view.selected_items();
        if items.is_empty() {
            items.extend(view.focus().and_then(|i| view.entry_path(i)));
        }
        self.create_links(items.into_iter().map(|(path, _)| path).collect(), LinkKind::Alias);
    }

    /// A link of `kind` next to each of `paths` ("Create link ▸", Explorer's "Create shortcut").
    pub fn create_links(&self, paths: Vec<PathBuf>, kind: LinkKind) {
        let paths = self.without_roots(paths, "link to");
        if paths.is_empty() {
            return;
        }
        let retry: Retry = Rc::new(move || -> Box<dyn Task> { Box::new(LinkTask::beside(paths.clone(), kind)) });
        self.submit(retry(), Some(retry), After::Select);
    }

    pub fn new_file(&self, dir: Option<PathBuf>) {
        match dir.or_else(|| crate::panes::active_view().folder()) {
            Some(dir) => {
                self.submit(Box::new(NewTask::file(&dir)), None, After::Rename);
            }
            None => self.not_here(),
        }
    }

    pub fn undo(&self) {
        if !self.ask_admin(false) && self.0.engine.undo().is_none() {
            crate::panes::active_view().note("Nothing to undo".to_owned());
        }
    }

    pub fn redo(&self) {
        if !self.ask_admin(true) && self.0.engine.redo().is_none() {
            crate::panes::active_view().note("Nothing to redo".to_owned());
        }
    }

    /// An undo (`redo`: redo) that runs as administrator asks first, every time, naming what it
    /// sends; only that very list runs. Returns whether it asked.
    fn ask_admin(&self, redo: bool) -> bool {
        let agreed = self.0.engine.admin_ops(redo);
        if agreed.is_empty() {
            return false;
        }
        let (title, button) = if redo { ("Redo as administrator", "Redo") } else { ("Undo as administrator", "Undo") };
        let text = crate::admin::undo_text(redo, &agreed, crate::admin::exposed());
        let (engine, view) = (self.0.engine.clone(), crate::panes::active_view().clone());
        self.0.dialogs.ask(title, text, &[button, "Cancel"], move |choice| {
            if choice == Some(0) && engine.undo_agreed(redo, &agreed).is_none() {
                view.note("Not done: something else was done meanwhile".to_owned());
            }
        });
        true
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
                    crate::panes::active_nav().refresh_showing(&dirs, &[], None);
                    crate::sidebar::with_current(|s| s.folders_changed(&dirs));
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

    fn finished(&self, id: JobId, mut report: Report) {
        // Only the chosen items' moves are kept (a folder moved across drives lists every file).
        report.moved = relevant_moves(&report.moved, &report.results);
        self.0.conflicts.close_if(id);
        // Its questions are moot now (answering one is harmless: nothing waits for it).
        self.0.dialogs.forget_job(id);
        self.0.asked.borrow_mut().remove(&id);
        let problems = !report.cancelled && !report.failures.is_empty();
        let mut after = After::Nothing;
        let mut hidden_in = None;
        let mut hidden_paths = Vec::new();
        let mut origin = None;
        let mut title = String::new();
        let mut bins = false;
        self.with_job(id, |job| {
            if !report.unchecked.is_empty() {
                job.note = Some(gezik_ops::UNCHECKED);
            }
            after = job.after;
            bins = job.bins;
            hidden_in = job.hidden_in.take();
            hidden_paths = std::mem::take(&mut job.hidden_paths);
            origin = job.origin.take();
            title = job.title.clone();
            job.finish(report.clone());
        });
        let time = gezik_platform::local_date_parts(std::time::SystemTime::now())
            .map(|t| format!("{:02}:{:02}:{:02}", t.hour, t.minute, t.second))
            .unwrap_or_default();
        self.0.history.borrow_mut().push(time, title, &report);
        if bins {
            // Windows redraws the Recycle Bin's icon.
            gezik_platform::trash::changed();
        }
        self.sync_history();
        if problems {
            // Something failed: the panel opens by itself.
            self.0.collapsed.set(false);
        }
        if after == After::Rename {
            *self.0.rename_when_shown.borrow_mut() = report.results.first().cloned();
        }
        // A rename is open (Tab went on): the selection stays with it.
        let after = if after == After::Select && crate::panes::active_view().renaming().is_some() {
            After::Nothing
        } else {
            after
        };
        let select = match (after, crate::panes::active_view().folder()) {
            (After::Nothing, _) | (_, None) => Vec::new(),
            (_, Some(folder)) => first_level_names(&report.results, &folder),
        };
        // Rows hidden for this job come back if it changed nothing (failed, cancelled, no trash).
        // Folder sizes above anything the job touched are out of date (a move's sources too).
        let mut touched = report.changed_dirs.clone();
        touched.extend(report.results.iter().cloned());
        touched.extend(report.moved.iter().map(|(from, _)| from.clone()));
        crate::panes::with_active(|p| p.folder_sizes.forget(&touched));
        let mut dirs = report.changed_dirs.clone();
        dirs.extend(hidden_in);
        let skipped = (report.skipped_changed > 0)
            .then(|| format!("{} changed since; skipped", crate::stack::count_text(report.skipped_changed)));
        // New items here the filter hides (a paste, a drop, an extract): the filter stays, the
        // status bar says so. A new folder's rename closes the filter instead.
        let hidden = if after == After::Rename {
            None
        } else {
            hidden_note(crate::panes::active_view().hidden_by_filter(&select))
        };
        let note = match (skipped, hidden) {
            (Some(a), Some(b)) => Some(format!("{a} · {b}")),
            (a, b) => a.or(b),
        };
        let reloading = crate::panes::active_nav().refresh_showing(&dirs, &select, note.clone());
        // The sidebar tree's open branches the job touched are read again (spec 10 §5.2).
        crate::sidebar::with_current(|s| s.folders_changed(&report.changed_dirs));
        // Search results follow Gezik's own jobs (spec 4.7), those kept by a tab too.
        let mut paths = report.results.clone();
        paths.extend(hidden_paths);
        crate::panes::with_active(|p| {
            p.search.job_done(origin.as_ref(), report.changed_dirs.clone(), paths, report.moved.clone());
        });
        self.0.sidebar.refresh();
        if let (false, Some(note)) = (reloading, note) {
            crate::panes::active_view().note(note);
        }
        if !report.no_trash.is_empty() {
            self.ask_delete_for_good(report.no_trash.clone());
        }
        // A row with skipped items stays until closed, for its Details.
        let notes = !report.cancelled && (!report.skipped.is_empty() || !report.unchecked.is_empty());
        if !problems && !notes {
            let ops = self.clone();
            slint::Timer::single_shot(DONE_FOR, move || ops.remove(id));
        }
        // An archive that needs 7-Zip, a download that is done, a conversion that needs ffmpeg.
        crate::archives::with_current(|archives| archives.job_finished(id, &report));
        crate::convert::with_current(|convert| convert.job_finished(id, &report));
        crate::stack::with_current(|stack| stack.job_finished(id, &report));
        crate::info::with_current(|info| info.job_finished(id, &report));
    }

    /// `show-in-folder` (spec 4.6): the focused result's folder with it selected; Back comes
    /// back to the results.
    pub fn show_in_folder(&self, new_tab: bool) {
        if !crate::panes::active_view().shows_results() {
            return crate::panes::active_view().note(only_in_results("Show in folder"));
        }
        if let Some((path, _)) =
            crate::panes::active_view().focus().and_then(|i| crate::panes::active_view().entry_path(i))
        {
            self.show_path_in_folder(&path, new_tab);
        }
    }

    /// `path`'s folder with it selected, here or in a new tab in front.
    pub fn show_path_in_folder(&self, path: &Path, new_tab: bool) {
        let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else { return };
        let names = vec![name.to_string_lossy().into_owned()];
        if new_tab {
            crate::panes::active_nav().open_tab_selecting(dir.to_path_buf(), names);
        } else {
            crate::panes::active_nav().go_selecting(dir.to_path_buf(), names);
        }
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
        let history = self.0.tab.get() == 1;
        window.set_ops_panel_open((!rows.is_empty() || history) && !self.0.collapsed.get());
        window.set_ops_tab(self.0.tab.get());
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

    /// The History's rows and the status bar's button.
    fn sync_history(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        let history = self.0.history.borrow();
        sync_model(&self.0.history_rows, history.records().map(history_row));
        window.set_history_available(true);
    }

    /// `show-history`, View ▸ Operation history: the panel opens on its History.
    pub fn show_history(&self) {
        self.0.tab.set(1);
        self.0.collapsed.set(false);
        self.update();
    }

    /// The status bar's History button: opens the History, or closes it if it is shown.
    pub fn history_toggle(&self) {
        let shown = self.0.tab.get() == 1 && !self.0.collapsed.get();
        self.0.tab.set(if shown { 0 } else { 1 });
        self.0.collapsed.set(false);
        self.update();
    }

    pub fn choose_tab(&self, tab: i32) {
        self.0.tab.set(tab.clamp(0, 1));
        self.update();
    }

    /// "Show in folder" of record `id`.
    pub fn history_show(&self, id: i32) {
        let show = u64::try_from(id).ok().and_then(|id| self.0.history.borrow().get(id).and_then(|r| r.show.clone()));
        if let Some((dir, names)) = show {
            crate::panes::active_nav().go_selecting(dir, names);
        }
    }

    /// "Details" of record `id`: what failed or was skipped.
    pub fn history_details(&self, id: i32) {
        let record = u64::try_from(id).ok().and_then(|id| self.0.history.borrow().get(id).cloned());
        if let Some(record) = record {
            self.0.dialogs.ask(record.title, record.details.unwrap_or_default(), &["Close"], |_| {});
        }
    }

    /// The failures of a row, with Retry, and Retry as administrator when the system refused
    /// items the administrator can redo (spec 9 §10.1); done items Gezik could not check.
    pub fn details(&self, id: i32) {
        let id = Self::id(id);
        let (title, message, can_retry, admin) = {
            let jobs = self.0.jobs.borrow();
            let Some(job) = jobs.iter().find(|j| j.id == id) else { return };
            let Some(report) = &job.report else { return };
            let lines = [crate::op_history::details_text(report), crate::admin::unchecked_text(&report.unchecked)];
            let message = lines.into_iter().flatten().collect::<Vec<_>>().join(
                "

",
            );
            let can_retry = !report.failures.is_empty() && (job.retry.is_some() || job.again.is_some());
            (job.title.clone(), message, can_retry, report.as_admin.clone())
        };
        let ops = self.clone();
        let buttons = crate::admin::detail_buttons(!admin.is_empty(), can_retry);
        let chosen = buttons.clone();
        self.0.dialogs.ask(title, message, &buttons, move |choice| match choice.and_then(|i| chosen.get(i)).copied() {
            Some(crate::admin::RETRY_AS_ADMIN) => {
                // The row goes once the new job is there (it stays if the user cancels).
                let row = ops.clone();
                crate::admin::start(&ops, admin, move |_| row.remove(id));
            }
            Some("Retry") => ops.retry(i32::try_from(id).unwrap_or(0)),
            _ => {}
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

/// A History record as a panel row: the Current tab's row, marked as History (a time, no bar,
/// no Close; its buttons act on the record).
fn history_row(record: &crate::op_history::Record) -> OpRow {
    OpRow {
        id: i32::try_from(record.id).unwrap_or(i32::MAX),
        history: true,
        time: record.time.as_str().into(),
        title: record.title.as_str().into(),
        detail: record.result.as_str().into(),
        state: if record.failed { RowState::Failed } else { RowState::Done } as i32,
        can_show: record.show.is_some(),
        can_details: record.details.is_some(),
        finished: true,
        ..OpRow::default()
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

    #[test]
    fn a_history_row_is_marked_as_history_even_without_a_time() {
        // `finished` gives an empty time when the clock cannot be read: the row must still act
        // on the History (no Close that would dismiss a running job with the same number).
        let mut history = crate::op_history::History::default();
        history.push(String::new(), "Copying 2 items".into(), &report(1, false));
        let row = history_row(history.records().next().expect("a record"));
        assert!(row.history);
        assert!(row.time.is_empty());
        assert!(row.finished);
        assert_eq!(row.state, RowState::Failed as i32);
        assert!(!row.can_pause && !row.can_resume && !row.can_start_now && !row.can_retry);
    }

    fn progress(state: JobState, items: (u64, u64), bytes: (u64, u64)) -> Progress {
        Progress { state, items_done: items.0, items_total: items.1, bytes_done: bytes.0, bytes_total: bytes.1 }
    }

    fn report(failures: usize, cancelled: bool) -> Report {
        Report {
            kind: TaskKind::Copy,
            cancelled,
            failures: (0..failures)
                .map(|i| Failure { path: format!("/f{i}").into(), message: "x".into(), denied: false })
                .collect(),
            skipped: Vec::new(),
            skipped_changed: 0,
            no_trash: Vec::new(),
            unchecked: Vec::new(),
            results: Vec::new(),
            changed_dirs: Vec::new(),
            moved: Vec::new(),
            as_admin: Vec::new(),
        }
    }

    #[test]
    fn a_quick_job_with_only_notes_still_shows_its_row() {
        // `huge.pdf`: rendered at a lower dpi, done before the row came up; its note stays.
        let mut notes = report(0, false);
        notes.skipped.push(Failure {
            path: "/huge - page 1.jpg".into(),
            message: "page 1 was made at 40 dpi: at 300 dpi it would be too large".into(),
            denied: false,
        });
        let mut job = JobView::new(1, "PDF to images".into());
        job.finish(notes.clone());
        assert!(job.shown);
        assert!(job.row().can_details);
        assert_eq!(job.row().detail, "Done · 1 item skipped");
        // A failure shows too; a clean or cancelled job that ended first does not.
        let mut failed = JobView::new(2, "x".into());
        failed.finish(report(1, false));
        assert!(failed.shown);
        let mut clean = JobView::new(3, "x".into());
        clean.finish(report(0, false));
        assert!(!clean.shown);
        let mut cancelled = JobView::new(4, "x".into());
        notes.cancelled = true;
        cancelled.finish(notes);
        assert!(!cancelled.shown);
    }

    #[test]
    fn a_rename_ended_by_switching_windows_gives_the_list_the_keyboard() {
        // Enter: always; Tab: never (the next field takes it).
        assert!(refocus_after(Commit::Enter, true, false));
        assert!(!refocus_after(Commit::Tab, true, false));
        assert!(!refocus_after(Commit::Tab, false, false));
        // A click elsewhere in the active window: the keyboard stays where it went.
        assert!(!refocus_after(Commit::Blur, true, false));
        // The window lost the focus: nothing else holds the keyboard, and the field goes away.
        assert!(refocus_after(Commit::Blur, false, false));
        // But a dialog keeps it.
        assert!(!refocus_after(Commit::Blur, false, true));
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
        skipped.skipped.push(Failure { path: "/a.zip".into(), message: "no password".into(), denied: false });
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
    fn only_the_moves_of_the_chosen_items_follow_a_job() {
        let dir = PathBuf::from("/d");
        let mut moved: Vec<(PathBuf, PathBuf)> =
            (0..10_000).map(|i| (PathBuf::from(format!("/s/f/{i}")), dir.join("f").join(i.to_string()))).collect();
        moved.push((PathBuf::from("/s/f"), dir.join("f")));
        assert_eq!(relevant_moves(&moved, &[dir.join("f")]), [(PathBuf::from("/s/f"), dir.join("f"))]);
    }

    #[test]
    fn a_paste_with_folders_selects_what_it_made_here() {
        let dir = PathBuf::from("/t");
        let results =
            [dir.join("a").join("b").join("x"), dir.join("a").join("y"), dir.join("z"), PathBuf::from("/o/q")];
        assert_eq!(first_level_names(&results, &dir), ["a", "z"]);
    }

    #[test]
    fn a_copy_with_folders_pastes_only_while_it_is_on_the_clipboard() {
        let paths = [PathBuf::from("/w/a/x"), PathBuf::from("/w/b/y")];
        assert!(with_folders_pastes(7, 7, &paths, None), "the same clipboard");
        assert!(!with_folders_pastes(7, 8, &paths, Some(&paths)), "copied again since");
        // No change numbers (Linux, Gezik's own clipboard): by what it holds.
        assert!(with_folders_pastes(0, 0, &paths, Some(&paths)));
        assert!(!with_folders_pastes(0, 0, &paths, Some(&paths[..1])));
        assert!(!with_folders_pastes(0, 0, &paths, None));
    }

    #[test]
    fn only_results_in_the_folder_are_selected() {
        let results = [PathBuf::from("/a/x.txt"), PathBuf::from("/b/y.txt"), PathBuf::from("/a/sub")];
        assert_eq!(result_names(&results, Path::new("/a")), ["x.txt", "sub"]);
    }

    #[test]
    fn a_note_follows_the_detail() {
        assert_eq!(with_note("Done".to_owned(), Some(CANT_UNDO)), "Done · can't be undone");
        assert_eq!(with_note("45%".to_owned(), None), "45%");
    }

    #[test]
    fn a_job_with_a_note_shows_it_even_when_quick() {
        // A `{files}` run done before its row came up: the row shows, saying so.
        let mut job = JobView::new(1, "List them".into());
        job.note = Some(CANT_UNDO);
        job.finish(report(0, false));
        assert!(job.shown);
        assert!(job.row().detail.ends_with("· can't be undone"), "{}", job.row().detail);
        // Cancelled, it did nothing to warn about.
        let mut cancelled = JobView::new(2, "List them".into());
        cancelled.note = Some(CANT_UNDO);
        cancelled.finish(report(0, true));
        assert!(!cancelled.shown);
    }
}
