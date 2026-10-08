//! The engine: jobs, the per-drive queue and the events the UI drains.

use std::collections::{BTreeSet, HashMap};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use gezik_core::ops::conflict::{ConflictKind, Decision, Facts};
use gezik_core::ops::history::{Stamp, UndoStack};
use gezik_core::ops::paths::{DriveSet, lexical_roots};
use gezik_core::ops::threads::CopyThreads;
use gezik_platform::fs::{self, DriveFacts};

use crate::control::Control;
use crate::pending::{PendingDeletes, is_hidden};
use crate::task::{Answer, Outcome, Question, Task, TaskKind};
use crate::tasks::DeleteTask;

pub type JobId = u64;

/// Where a job came from: its result goes on the undo or the redo stack. An undo or redo
/// that did nothing puts its action back where it was (`Stamp`: taken when it was popped).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Origin {
    New,
    Undo(Stamp),
    Redo(Stamp),
}

/// One undoable action: its label and the tasks that undo it.
pub(crate) struct Record {
    label: String,
    inverse: Vec<Arc<dyn Task>>,
}

#[cfg(test)]
pub(crate) type DriveQueryHook = Arc<dyn Fn(&Path) + Send + Sync>;

/// How many actions can be undone.
const HISTORY: usize = 100;

/// From `[files]` and the config folder.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    pub threads: CopyThreads,
    /// Where instant deletes note the hidden folders they have not finished; `None`: deletes
    /// run in place.
    pub pending_deletes: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseReason {
    User,
    DiskFull,
    /// Many items failed in a row (a network drive went away).
    ManyFailures,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    /// Waiting for another job on the same drive.
    Waiting,
    Scanning,
    Running,
    /// Waiting for the user's conflict decisions.
    Deciding,
    Paused(PauseReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress {
    pub state: JobState,
    pub items_done: u64,
    pub items_total: u64,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConflictItem {
    pub source: PathBuf,
    pub target: PathBuf,
    pub kind: ConflictKind,
    pub source_facts: Facts,
    pub target_facts: Facts,
    /// What it starts with (Skip, or Merge for folders).
    pub decision: Decision,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub kind: TaskKind,
    pub cancelled: bool,
    pub failures: Vec<Failure>,
    /// Items left out on purpose (the user gave no password, a link Gezik does not make):
    /// notes, not failures.
    pub skipped: Vec<Failure>,
    /// Items an undo left alone because they changed since.
    pub skipped_changed: usize,
    /// Items not trashed because their drive has no trash: the UI offers to delete them.
    pub no_trash: Vec<PathBuf>,
    /// Where the chosen items are now (pasted, renamed, new), to select them.
    pub results: Vec<PathBuf>,
    pub changed_dirs: Vec<PathBuf>,
    /// Items it moved or renamed (from, to), each as it really went: search results follow
    /// them (spec 4.7).
    pub moved: Vec<(PathBuf, PathBuf)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// `background`: an instant delete, which resumes on the next start if Gezik quits.
    Added {
        job: JobId,
        title: String,
        kind: TaskKind,
        background: bool,
    },
    /// At most ten times a second per job, and only when something changed.
    Progress {
        job: JobId,
        progress: Progress,
    },
    /// The job waits for `Engine::decide`, one decision per item, in this order.
    Conflicts {
        job: JobId,
        conflicts: Vec<ConflictItem>,
    },
    /// The job waits for `Engine::answer` with this `id` (or `cancel`).
    Question {
        job: JobId,
        /// Which of the job's questions it is: an answer meant for another is dropped.
        id: u64,
        question: Question,
    },
    /// The job waits for `Engine::resume` (or `cancel`).
    Paused {
        job: JobId,
        reason: PauseReason,
        path: Option<PathBuf>,
    },
    Finished {
        job: JobId,
        report: Report,
    },
    /// Folders whose contents changed (about once a second while a job runs, and at its end).
    Changed {
        dirs: Vec<PathBuf>,
    },
    /// What Undo and Redo would do changed.
    History,
}

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// At most this many changed folders are tracked per job (besides those of chosen items).
const MAX_CHANGED: usize = 256;

/// What a job gathers while it runs.
#[derive(Default)]
pub(crate) struct Acc {
    pub outcomes: Vec<Outcome>,
    pub failures: Vec<Failure>,
    pub skipped: Vec<Failure>,
    pub skipped_changed: usize,
    pub no_trash: Vec<PathBuf>,
    pub results: Vec<PathBuf>,
    pub changed: BTreeSet<PathBuf>,
    /// Changed folders not yet sent in a `Changed` event.
    pub unsent: BTreeSet<PathBuf>,
}

/// One submitted unit of work: its tasks run one after the other (an undo can be several).
pub(crate) struct Job {
    pub id: JobId,
    pub tasks: Vec<Arc<dyn Task>>,
    pub control: Control,
    /// Set on the job's thread before it waits for its turn.
    pub drives: Mutex<DriveSet>,
    /// Whether `drives` is final (an earlier job whose drives are unknown yet blocks later ones
    /// on the same roots).
    pub drives_known: AtomicBool,
    /// The roots of the paths it touches, known at once: a drive query that hangs (a lost
    /// network drive) holds back only the jobs on the same roots.
    pub roots: DriveSet,
    pub background: bool,
    pub origin: Origin,
    /// "Copy 3 items": what Undo will say.
    pub label: String,
    pub done: AtomicBool,
    pub acc: Mutex<Acc>,
    /// Temporary names of the files it copies.
    pub temp: crate::task::TempCopies,
    /// Hidden folders its items build things in; removed when it ends.
    pub staging: Mutex<Vec<PathBuf>>,
}

impl Job {
    pub fn outcome(&self, outcome: Outcome) {
        lock(&self.acc).outcomes.push(outcome);
    }

    pub fn fail(&self, path: &Path, error: &io::Error) {
        lock(&self.acc).failures.push(Failure { path: path.to_path_buf(), message: fs::describe(error) });
    }

    pub fn skip(&self, path: &Path, why: &io::Error) {
        lock(&self.acc).skipped.push(Failure { path: path.to_path_buf(), message: fs::describe(why) });
    }

    pub fn skipped_changed(&self) {
        lock(&self.acc).skipped_changed += 1;
    }

    pub fn no_trash(&self, path: &Path) {
        lock(&self.acc).no_trash.push(path.to_path_buf());
    }

    pub fn result(&self, path: PathBuf) {
        lock(&self.acc).results.push(path);
    }

    /// The folders holding `paths` changed. Those of a chosen item (`root`) always count: they
    /// are the folders on screen, and a delete reaches them last, after every folder inside.
    pub fn touch<'a>(&self, paths: impl IntoIterator<Item = &'a Path>, root: bool) {
        let mut acc = lock(&self.acc);
        for parent in paths.into_iter().filter_map(Path::parent) {
            if (root || acc.changed.len() < MAX_CHANGED) && acc.changed.insert(parent.to_path_buf()) {
                acc.unsent.insert(parent.to_path_buf());
            }
        }
    }

    pub fn snapshot(&self) -> Progress {
        let c = &self.control;
        let state = if let Some(reason) = c.pause_reason() {
            JobState::Paused(reason)
        } else if c.deciding.load(Ordering::SeqCst) || c.asking.load(Ordering::SeqCst) {
            JobState::Deciding
        } else if c.scanning.load(Ordering::SeqCst) {
            JobState::Scanning
        } else if c.running.load(Ordering::SeqCst) {
            JobState::Running
        } else {
            JobState::Waiting
        };
        Progress {
            state,
            items_done: c.items_done.load(Ordering::Relaxed),
            items_total: c.items_total.load(Ordering::Relaxed),
            bytes_done: c.bytes_done.load(Ordering::Relaxed),
            bytes_total: c.bytes_total.load(Ordering::Relaxed),
        }
    }
}

pub(crate) struct Shared {
    settings: Mutex<Settings>,
    /// Jobs not finished yet, in the order they were submitted.
    jobs: Mutex<Vec<Arc<Job>>>,
    turn: Condvar,
    events: Mutex<Vec<Event>>,
    notify: Box<dyn Fn() + Send + Sync>,
    /// Drive facts by drive root.
    drives: Mutex<HashMap<PathBuf, DriveFacts>>,
    reporter: AtomicBool,
    next_id: AtomicU64,
    pub pending: Option<Arc<PendingDeletes>>,
    history: Mutex<UndoStack<Record>>,
    /// Tests: called before each drive query (to make one hang).
    #[cfg(test)]
    pub drive_query_hook: Mutex<Option<DriveQueryHook>>,
}

impl Shared {
    pub fn push(&self, events: impl IntoIterator<Item = Event>) {
        let added = {
            let mut queue = lock(&self.events);
            let before = queue.len();
            queue.extend(events);
            queue.len() > before
        };
        if added {
            (self.notify)();
        }
    }

    pub fn settings(&self) -> Settings {
        lock(&self.settings).clone()
    }

    /// The facts of the drive `path` is on, cached per drive root.
    pub fn drive(&self, path: &Path) -> Option<DriveFacts> {
        #[cfg(test)]
        {
            // Not under the lock: the hook may wait.
            let hook = lock(&self.drive_query_hook).clone();
            if let Some(hook) = hook {
                hook(path);
            }
        }
        let existing = fs::nearest_existing(path).unwrap_or_else(|| path.to_path_buf());
        let root = fs::drive_root(&existing).unwrap_or_else(|| existing.clone());
        if let Some(facts) = lock(&self.drives).get(&root) {
            return Some(facts.clone());
        }
        let facts = fs::drive_facts(&existing).ok()?;
        lock(&self.drives).insert(root, facts.clone());
        Some(facts)
    }

    pub fn has_trash(&self, path: &Path) -> bool {
        self.drive(path).is_some_and(|facts| facts.trash)
    }

    pub fn pause(&self, job: &Job, reason: PauseReason, path: Option<PathBuf>) {
        job.control.pause(reason);
        self.push([Event::Paused { job: job.id, reason, path }]);
    }

    /// Waits until no earlier unfinished job shares a drive with `job` (or it may start now);
    /// an earlier job whose drives are not known yet counts if it shares a root. False if it
    /// is cancelled first.
    pub fn wait_turn(&self, job: &Job) -> bool {
        let mut jobs = lock(&self.jobs);
        loop {
            if job.control.cancelled() {
                return false;
            }
            let drives = lock(&job.drives).clone();
            let earlier = jobs.iter().take_while(|other| other.id != job.id);
            let blocked = !job.control.start_now.load(Ordering::SeqCst)
                && earlier.filter(|other| !other.done.load(Ordering::SeqCst)).any(|other| {
                    if other.drives_known.load(Ordering::SeqCst) {
                        lock(&other.drives).intersects(&drives)
                    } else {
                        other.roots.intersects(&job.roots)
                    }
                });
            if !blocked {
                job.control.running.store(true, Ordering::SeqCst);
                return true;
            }
            jobs = match self.turn.wait_timeout(jobs, Duration::from_millis(200)) {
                Ok((guard, _)) => guard,
                Err(poisoned) => poisoned.into_inner().0,
            };
        }
    }

    /// Sets the drives `job` touches and lets waiting jobs look again.
    pub fn publish_drives(&self, job: &Job, drives: DriveSet) {
        *lock(&job.drives) = drives;
        {
            // Under the jobs lock: a waiter is either before its check (and sees this) or waiting.
            let _jobs = lock(&self.jobs);
            job.drives_known.store(true, Ordering::SeqCst);
        }
        self.turn.notify_all();
    }

    /// Ends `job`: tells its tasks, reports, and lets waiting jobs go.
    pub fn finish(&self, job: &Arc<Job>, cancelled: bool) {
        for task in &job.tasks {
            task.done(cancelled);
        }
        job.temp.done();
        for dir in std::mem::take(&mut *lock(&job.staging)) {
            // Emptied by its items, or not: either way it goes. If it cannot, its note stays
            // and the next start deletes it.
            let gone = fs::delete(&dir).is_ok()
                || std::fs::remove_dir_all(&dir).is_ok()
                || std::fs::symlink_metadata(&dir).is_err_and(|err| err.kind() == io::ErrorKind::NotFound);
            if gone && let Some(pending) = &self.pending {
                pending.remove(&dir);
            }
        }
        let acc = std::mem::take(&mut *lock(&job.acc));
        // A move that replaced its target is `Several` (the target trashed, then the move).
        let mut flat = Vec::new();
        crate::inverse::flatten(&acc.outcomes, &mut flat);
        let moved = flat
            .into_iter()
            .filter_map(|outcome| match outcome {
                Outcome::Moved { from, to, .. } | Outcome::Created { path: to, from: Some(from), .. } => {
                    Some((from.clone(), to.clone()))
                }
                _ => None,
            })
            .collect();
        let inverse = crate::inverse::build(&acc.outcomes);
        let recorded = !inverse.is_empty();
        {
            let mut history = lock(&self.history);
            if recorded {
                let record = Record { label: job.label.clone(), inverse };
                match job.origin {
                    Origin::New => history.push_new(record),
                    Origin::Undo(_) => history.push_undone(record),
                    Origin::Redo(_) => history.push_redone(record),
                }
            } else if cancelled && acc.failures.is_empty() && acc.skipped_changed == 0 && acc.no_trash.is_empty() {
                // An undo or redo cancelled before it did anything: the action stays where it
                // was. One whose items failed or changed since is dropped (trying it again would
                // fail the same way and keep older actions out of reach); its report says why.
                let record = Record { label: job.label.clone(), inverse: job.tasks.clone() };
                match job.origin {
                    Origin::New => {}
                    Origin::Undo(stamp) => history.put_back_undo(record, stamp),
                    Origin::Redo(stamp) => history.put_back_redo(record, stamp),
                }
            }
        }
        let kind = job.tasks.first().map_or(TaskKind::Copy, |task| task.kind());
        let report = Report {
            kind,
            cancelled,
            failures: acc.failures,
            skipped: acc.skipped,
            skipped_changed: acc.skipped_changed,
            no_trash: acc.no_trash,
            results: acc.results,
            changed_dirs: acc.changed.into_iter().collect(),
            moved,
        };
        job.done.store(true, Ordering::SeqCst);
        lock(&self.jobs).retain(|other| other.id != job.id);
        self.turn.notify_all();
        let mut events = Vec::new();
        if !report.changed_dirs.is_empty() {
            events.push(Event::Changed { dirs: report.changed_dirs.clone() });
        }
        if recorded || job.origin != Origin::New {
            events.push(Event::History);
        }
        events.push(Event::Finished { job: job.id, report });
        self.push(events);
    }

    /// Starts the thread that reports progress ten times a second while any job runs.
    fn ensure_reporter(self: &Arc<Self>) {
        if self.reporter.swap(true, Ordering::SeqCst) {
            return;
        }
        let shared = self.clone();
        let spawned = std::thread::Builder::new().name("gezik-progress".into()).spawn(move || {
            let mut last: HashMap<JobId, Progress> = HashMap::new();
            let mut tick = 0u64;
            loop {
                std::thread::sleep(Duration::from_millis(100));
                tick += 1;
                let jobs = {
                    let jobs = lock(&shared.jobs);
                    if jobs.is_empty() {
                        // Under the jobs lock: a job submitted after this starts a new reporter.
                        shared.reporter.store(false, Ordering::SeqCst);
                        return;
                    }
                    jobs.clone()
                };
                let mut events = Vec::new();
                for job in &jobs {
                    let progress = job.snapshot();
                    if last.get(&job.id) != Some(&progress) {
                        last.insert(job.id, progress.clone());
                        events.push(Event::Progress { job: job.id, progress });
                    }
                    if tick.is_multiple_of(10) {
                        let dirs: Vec<PathBuf> = std::mem::take(&mut lock(&job.acc).unsent).into_iter().collect();
                        if !dirs.is_empty() {
                            events.push(Event::Changed { dirs });
                        }
                    }
                }
                last.retain(|id, _| jobs.iter().any(|job| job.id == *id));
                shared.push(events);
            }
        });
        if spawned.is_err() {
            self.reporter.store(false, Ordering::SeqCst);
        }
    }
}

#[derive(Clone)]
pub struct Engine(pub(crate) Arc<Shared>);

impl Engine {
    /// `notify` is called (on any thread) whenever events are waiting in [`Engine::drain`].
    pub fn new(settings: Settings, notify: impl Fn() + Send + Sync + 'static) -> Engine {
        let pending = settings.pending_deletes.clone().map(|file| Arc::new(PendingDeletes::new(file)));
        Engine(Arc::new(Shared {
            settings: Mutex::new(settings),
            jobs: Mutex::default(),
            turn: Condvar::new(),
            events: Mutex::default(),
            notify: Box::new(notify),
            drives: Mutex::default(),
            reporter: AtomicBool::new(false),
            next_id: AtomicU64::new(0),
            pending,
            history: Mutex::new(UndoStack::new(HISTORY)),
            #[cfg(test)]
            drive_query_hook: Mutex::default(),
        }))
    }

    /// The list instant deletes note their hidden folders in (`None`: deletes run in place).
    pub fn pending_deletes(&self) -> Option<Arc<PendingDeletes>> {
        self.0.pending.clone()
    }

    /// Finishes deletes an earlier run left unfinished (call once at start).
    pub fn recover_deletes(&self) -> Option<JobId> {
        let pending = self.0.pending.clone()?;
        // Copies an earlier run cut short go: small ones from their journals, large ones left
        // under temporary names; not those of a copy that still runs (another Gezik window).
        crate::journal::recover(&pending.journal_dir());
        let mut cleaned = Vec::new();
        for note in pending.copies() {
            if gezik_platform::process_alive(note.pid) {
                continue;
            }
            match std::fs::read_dir(&note.folder) {
                Ok(entries) => {
                    for entry in entries.flatten() {
                        let ours = entry.file_name().to_str().is_some_and(|name| name.starts_with(&note.prefix));
                        if ours && entry.file_type().is_ok_and(|kind| kind.is_file()) {
                            let _ = fs::delete(&entry.path());
                        }
                    }
                    cleaned.push(note.prefix);
                }
                Err(err) if err.kind() == io::ErrorKind::NotFound => cleaned.push(note.prefix),
                // Not reachable now (a network drive): tried again at the next start.
                Err(_) => {}
            }
        }
        for prefix in cleaned {
            pending.remove_copies(&prefix);
        }
        // Folders a delete could not put back (something held them open) go back first, and
        // items a rename left under temporary names, unless that rename still runs.
        for restore in pending.restores() {
            if crate::pending::renaming_pid(&restore.hidden).is_some_and(gezik_platform::process_alive) {
                continue;
            }
            if std::fs::symlink_metadata(&restore.hidden).is_err()
                || crate::tasks::restore_hidden(&restore.hidden, &restore.original, restore.was_hidden)
            {
                pending.remove_restore(&restore.hidden);
            }
        }
        let mut roots = Vec::new();
        for path in pending.load_unowned() {
            if is_hidden(&path) && std::fs::symlink_metadata(&path).is_ok() {
                roots.push(path);
            } else {
                pending.remove(&path);
            }
        }
        (!roots.is_empty()).then(|| self.submit(Box::new(DeleteTask::recover(roots, pending))))
    }

    pub fn set_threads(&self, threads: CopyThreads) {
        lock(&self.0.settings).threads = threads;
    }

    pub fn submit(&self, task: Box<dyn Task>) -> JobId {
        self.start(vec![Arc::from(task)], Origin::New, None)
    }

    /// One job of `tasks`, run in order, undone as one action (`label`: what Undo says).
    pub fn submit_chain(&self, tasks: Vec<Box<dyn Task>>, label: Option<String>) -> JobId {
        self.start(tasks.into_iter().map(Arc::from).collect(), Origin::New, label)
    }

    /// Starts a job of `tasks` (run in order) on its own thread. `label`: what Undo says
    /// (default: from the first task); an undo or redo keeps the action's label.
    pub(crate) fn start(&self, tasks: Vec<Arc<dyn Task>>, origin: Origin, label: Option<String>) -> JobId {
        let id = self.0.next_id.fetch_add(1, Ordering::SeqCst) + 1;
        let Some(first) = tasks.first() else { return id };
        let label = label.unwrap_or_else(|| first.kind().label(first.count()));
        let title = match origin {
            Origin::New => first.title(),
            Origin::Undo(_) => format!("Undoing {label}"),
            Origin::Redo(_) => format!("Redoing {label}"),
        };
        let kind = first.kind();
        let background = tasks.iter().all(|task| task.kind() == TaskKind::Delete);
        let paths: Vec<PathBuf> = tasks.iter().flat_map(|task| task.resources().paths).collect();
        let roots = lexical_roots(paths.iter().map(PathBuf::as_path));
        let job = Arc::new(Job {
            id,
            tasks,
            control: Control::default(),
            drives: Mutex::default(),
            drives_known: AtomicBool::new(false),
            roots,
            background,
            origin,
            label,
            done: AtomicBool::new(false),
            acc: Mutex::default(),
            temp: crate::task::TempCopies::new(self.0.pending.clone()),
            staging: Mutex::default(),
        });
        lock(&self.0.jobs).push(job.clone());
        self.0.push([Event::Added { job: id, title, kind, background }]);
        self.0.ensure_reporter();
        let (shared, running) = (self.0.clone(), job.clone());
        let spawned =
            std::thread::Builder::new().name("gezik-job".into()).spawn(move || crate::run::run_job(&shared, &running));
        if let Err(err) = spawned {
            job.fail(Path::new(""), &err);
            self.0.finish(&job, true);
        }
        id
    }

    /// Undoes the last action; its progress shows like any job.
    pub fn undo(&self) -> Option<JobId> {
        let (record, stamp) = {
            let mut history = lock(&self.0.history);
            let stamp = history.stamp();
            (history.pop_undo()?, stamp)
        };
        self.0.push([Event::History]);
        Some(self.start(record.inverse, Origin::Undo(stamp), Some(record.label)))
    }

    pub fn redo(&self) -> Option<JobId> {
        let (record, stamp) = {
            let mut history = lock(&self.0.history);
            let stamp = history.stamp();
            (history.pop_redo()?, stamp)
        };
        self.0.push([Event::History]);
        Some(self.start(record.inverse, Origin::Redo(stamp), Some(record.label)))
    }

    /// "Copy 3 items" if there is something to undo.
    pub fn undo_label(&self) -> Option<String> {
        lock(&self.0.history).peek_undo().map(|record| record.label.clone())
    }

    pub fn redo_label(&self) -> Option<String> {
        lock(&self.0.history).peek_redo().map(|record| record.label.clone())
    }

    /// Puts events back at the front of the queue (test helpers).
    #[cfg(test)]
    pub(crate) fn requeue(&self, events: Vec<Event>) {
        let mut queue = lock(&self.0.events);
        queue.splice(0..0, events);
    }

    pub fn drain(&self) -> Vec<Event> {
        std::mem::take(&mut *lock(&self.0.events))
    }

    fn job(&self, id: JobId) -> Option<Arc<Job>> {
        lock(&self.0.jobs).iter().find(|job| job.id == id).cloned()
    }

    /// The user's decisions for the job's `Conflicts` event, in its order.
    pub fn decide(&self, job: JobId, decisions: Vec<Decision>) {
        if let Some(job) = self.job(job) {
            job.control.set_decisions(decisions);
        }
    }

    /// The user's answer to the job's `Question` event `id`.
    pub fn answer(&self, job: JobId, id: u64, answer: Answer) {
        if let Some(job) = self.job(job) {
            job.control.set_answer(id, answer);
        }
    }

    pub fn pause(&self, job: JobId) {
        if let Some(job) = self.job(job)
            && job.control.pause_reason().is_none()
        {
            job.control.pause(PauseReason::User);
        }
    }

    pub fn resume(&self, job: JobId) {
        if let Some(job) = self.job(job) {
            job.control.resume();
        }
    }

    pub fn cancel(&self, job: JobId) {
        if let Some(job) = self.job(job) {
            job.control.cancel();
        }
        self.0.turn.notify_all();
    }

    /// Lets a job waiting for its drive start right away.
    pub fn start_now(&self, job: JobId) {
        if let Some(job) = self.job(job) {
            job.control.start_now.store(true, Ordering::SeqCst);
        }
        self.0.turn.notify_all();
    }

    pub fn cancel_all(&self) {
        for job in lock(&self.0.jobs).iter() {
            job.control.cancel();
        }
        self.0.turn.notify_all();
    }

    /// Whether a job runs that quitting would lose (instant deletes resume on the next start).
    pub fn busy(&self) -> bool {
        lock(&self.0.jobs).iter().any(|job| !job.background)
    }

    /// Waits until every job has finished; false if `timeout` passes first.
    pub fn wait_idle(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            if lock(&self.0.jobs).is_empty() {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{FakeTask, Gate, defaults, engine, finish, test_dir};
    use crate::testing::{read, write};
    use crate::{CopyTask, DeleteTask, MoveTask, RenameTask, TrashTask};
    use std::sync::atomic::AtomicUsize;

    fn run(engine: &Engine, job: JobId) -> Report {
        finish(engine, job, defaults).0
    }

    #[test]
    fn a_chain_is_one_job_and_one_undo() {
        let dir = test_dir("chain");
        write(&dir.join("a.txt"), "a");
        let engine = engine();
        let tasks: Vec<Box<dyn Task>> = vec![
            Box::new(CopyTask::into(vec![dir.join("a.txt")], &dir.join("b"))),
            Box::new(CopyTask::into(vec![dir.join("a.txt")], &dir.join("c"))),
        ];
        std::fs::create_dir_all(dir.join("b")).unwrap();
        std::fs::create_dir_all(dir.join("c")).unwrap();
        let job = engine.submit_chain(tasks, Some("Copy twice".into()));
        let (report, _) = finish(&engine, job, defaults);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(engine.undo_label().as_deref(), Some("Copy twice"));
        finish(&engine, engine.undo().unwrap(), defaults);
        assert!(!dir.join("b/a.txt").exists() && !dir.join("c/a.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_task_can_ask_and_wait_for_the_answer() {
        // A test task asks once and records the answer in its outcome path.
        let engine = engine();
        let job = engine.submit(Box::new(crate::testing::AskingTask));
        let mut asked = false;
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            for event in engine.drain() {
                if let Event::Question { job: j, id, question: Question::Password { retry, .. } } = event {
                    assert_eq!(j, job);
                    assert!(!retry);
                    asked = true;
                    // An answer to another question is dropped; the right one is taken.
                    engine.answer(job, id + 1, Answer::Text("stale".into()));
                    engine.answer(job, id, Answer::Text("pw".into()));
                }
                if let Event::Finished { job: j, report } = event {
                    assert_eq!(j, job);
                    assert!(asked);
                    assert_eq!(report.results, [PathBuf::from("pw")]);
                    return;
                }
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn cancelling_a_waiting_question_answers_cancel() {
        let engine = engine();
        let job = engine.submit(Box::new(crate::testing::AskingTask));
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            for event in engine.drain() {
                if matches!(event, Event::Question { .. }) {
                    engine.cancel(job);
                }
                if let Event::Finished { report, .. } = event {
                    assert!(report.cancelled);
                    assert!(report.results.is_empty());
                    return;
                }
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn a_staging_folder_is_noted_and_gone_when_the_job_ends() {
        let dir = test_dir("staging");
        let pending = dir.join("pending-deletes");
        let engine = Engine::new(Settings { pending_deletes: Some(pending.clone()), ..Settings::default() }, || {});
        let job = engine.submit(Box::new(crate::testing::StagingTask::new(dir.clone())));
        let (report, _) = finish(&engine, job, defaults);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        let left: Vec<String> =
            std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert!(left.iter().all(|name| !name.starts_with(".gezik-")), "{left:?}");
        assert!(PendingDeletes::new(pending).load_unowned().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn undo_and_redo_a_copy() {
        let dir = test_dir("undo-copy");
        write(&dir.join("src/a.txt"), "a");
        std::fs::create_dir(dir.join("dst")).unwrap();
        let engine = engine();
        run(&engine, engine.submit(Box::new(CopyTask::into(vec![dir.join("src/a.txt")], &dir.join("dst")))));
        assert_eq!(engine.undo_label().as_deref(), Some("Copy 1 item"));
        let report = run(&engine, engine.undo().unwrap());
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert!(!dir.join("dst/a.txt").exists());
        assert_eq!(engine.undo_label(), None);
        assert_eq!(engine.redo_label().as_deref(), Some("Copy 1 item"));
        run(&engine, engine.redo().unwrap());
        assert_eq!(read(&dir.join("dst/a.txt")), "a", "back from the trash");
        assert_eq!(engine.undo_label().as_deref(), Some("Copy 1 item"));
        run(&engine, engine.undo().unwrap());
        assert!(!dir.join("dst/a.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn undo_rename_brings_the_old_name_back() {
        let dir = test_dir("undo-rename");
        write(&dir.join("a.txt"), "a");
        let engine = engine();
        run(&engine, engine.submit(Box::new(RenameTask::one(dir.join("a.txt"), "b.txt"))));
        assert_eq!(engine.undo_label().as_deref(), Some("Rename"));
        run(&engine, engine.undo().unwrap());
        assert_eq!(read(&dir.join("a.txt")), "a");
        assert!(!dir.join("b.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn undo_skips_changed_files() {
        let dir = test_dir("undo-changed");
        write(&dir.join("src/a.txt"), "a");
        std::fs::create_dir(dir.join("dst")).unwrap();
        let engine = engine();
        run(&engine, engine.submit(Box::new(CopyTask::into(vec![dir.join("src/a.txt")], &dir.join("dst")))));
        std::fs::write(dir.join("dst/a.txt"), "edited after the copy").unwrap();
        let report = run(&engine, engine.undo().unwrap());
        assert_eq!(report.skipped_changed, 1);
        assert_eq!(read(&dir.join("dst/a.txt")), "edited after the copy");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A lost network drive can hang its query; jobs on other drives must not wait for it.
    #[cfg(windows)]
    #[test]
    fn a_hanging_drive_query_holds_back_only_jobs_on_that_root() {
        let dir = test_dir("hanging-drive");
        write(&dir.join("src/a.txt"), "a");
        std::fs::create_dir(dir.join("dst")).unwrap();
        let engine = engine();
        let gate = Gate::default();
        let held = gate.clone();
        *lock(&engine.0.drive_query_hook) = Some(Arc::new(move |path: &Path| {
            if path.starts_with(r"Q:\") {
                held.wait();
            }
        }));
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut stuck = FakeTask::new("stuck", 1, &log);
        stuck.paths = vec![PathBuf::from(r"Q:\lost\a")];
        let stuck = engine.submit(Box::new(stuck));
        let copy = engine.submit(Box::new(CopyTask::into(vec![dir.join("src/a.txt")], &dir.join("dst"))));
        let started = Instant::now();
        let report = run(&engine, copy);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert!(started.elapsed() < Duration::from_secs(5), "the copy waited for the hanging drive");
        gate.open();
        run(&engine, stuck);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Writes a partial file, then removes it and fails, or (cancelled) is interrupted.
    struct PartialTask {
        target: PathBuf,
        fail: bool,
    }

    impl Task for PartialTask {
        fn kind(&self) -> TaskKind {
            TaskKind::Copy
        }
        fn title(&self) -> String {
            "partial".into()
        }
        fn count(&self) -> usize {
            1
        }
        fn resources(&self) -> crate::task::Resources {
            crate::task::Resources { paths: vec![self.target.clone()], work: crate::task::Work::Disk }
        }
        fn plan(&self, sink: &mut dyn crate::task::ScanSink) {
            let facts = gezik_core::ops::conflict::Facts { is_dir: false, size: 1, modified: None };
            sink.item(crate::task::PlanItem::new(crate::task::Stage::Parallel, facts).target(&self.target).top(0));
        }
        fn run(&self, _item: &crate::task::PlanItem, cx: &crate::task::RunCx<'_>) -> io::Result<crate::task::Outcome> {
            std::fs::write(&self.target, "part")?;
            if !self.fail {
                while !cx.stopped() {
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
            std::fs::remove_file(&self.target)?;
            Err(if self.fail { io::Error::other("boom") } else { io::Error::from(io::ErrorKind::Interrupted) })
        }
    }

    /// Like a task running a program: its first try writes a partial file and goes on until
    /// the job is paused or cancelled, then removes it and returns `restart` (or is
    /// interrupted); a later try finishes at once.
    struct RestartTask {
        target: PathBuf,
        tries: Arc<AtomicUsize>,
        /// What the item and the job had counted when the second try began.
        counted_on_retry: Arc<Mutex<Option<(u64, u64)>>>,
    }

    impl Task for RestartTask {
        fn kind(&self) -> TaskKind {
            TaskKind::Copy
        }
        fn title(&self) -> String {
            "restart".into()
        }
        fn count(&self) -> usize {
            1
        }
        fn resources(&self) -> crate::task::Resources {
            crate::task::Resources { paths: vec![self.target.clone()], work: crate::task::Work::External }
        }
        fn plan(&self, sink: &mut dyn crate::task::ScanSink) {
            let facts = gezik_core::ops::conflict::Facts { is_dir: false, size: 4, modified: None };
            sink.item(crate::task::PlanItem::new(crate::task::Stage::Parallel, facts).target(&self.target).top(0));
        }
        fn run(&self, _item: &crate::task::PlanItem, cx: &crate::task::RunCx<'_>) -> io::Result<crate::task::Outcome> {
            if self.tries.fetch_add(1, Ordering::SeqCst) > 0 {
                let job_done = cx.control.bytes_done.load(Ordering::Relaxed);
                *self.counted_on_retry.lock().unwrap() = Some((cx.counted(), job_done));
                std::fs::write(&self.target, "done")?;
                let facts = crate::task::facts_after(&self.target, false);
                return Ok(crate::task::Outcome::Created { path: self.target.clone(), facts, from: None });
            }
            std::fs::write(&self.target, "part")?;
            cx.add_bytes(3);
            while !cx.cancelled() && !cx.paused() {
                std::thread::sleep(Duration::from_millis(5));
            }
            std::fs::remove_file(&self.target)?;
            Err(if cx.cancelled() { io::Error::from(io::ErrorKind::Interrupted) } else { crate::task::restart() })
        }
    }

    #[test]
    fn an_item_cut_short_by_a_pause_is_done_again_once_resumed() {
        let dir = test_dir("restart");
        let engine = engine();
        let tries = Arc::new(AtomicUsize::new(0));
        let counted_on_retry = Arc::new(Mutex::new(None));
        let target = dir.join("a.bin");
        let task =
            RestartTask { target: target.clone(), tries: tries.clone(), counted_on_retry: counted_on_retry.clone() };
        let job = engine.submit(Box::new(task));
        wait_for("the first try", || target.exists());
        engine.pause(job);
        wait_for("the partial file to go", || !target.exists());
        // Paused: not tried again until resumed.
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(tries.load(Ordering::SeqCst), 1);
        engine.resume(job);
        let report = run(&engine, job);
        assert!(report.failures.is_empty() && !report.cancelled, "{:?}", report.failures);
        assert_eq!(tries.load(Ordering::SeqCst), 2);
        assert_eq!(read(&target), "done");
        // The bytes the first try counted were taken back: the item and the bar start over.
        assert_eq!(*counted_on_retry.lock().unwrap(), Some((0, 0)));

        // Cancelled while paused: it ends without another try.
        let tries = Arc::new(AtomicUsize::new(0));
        let target = dir.join("b.bin");
        let task = RestartTask { target: target.clone(), tries: tries.clone(), counted_on_retry: Default::default() };
        let job = engine.submit(Box::new(task));
        wait_for("the first try", || target.exists());
        engine.pause(job);
        wait_for("the partial file to go", || !target.exists());
        engine.cancel(job);
        let report = run(&engine, job);
        assert!(report.cancelled);
        assert_eq!(tries.load(Ordering::SeqCst), 1);
        assert!(!target.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// FAT32 holds no file of 4 GB or more, and says "disk full" for one: a file too big for
    /// the target drive fails at once with that reason (it is not paused for space).
    #[test]
    fn a_file_too_big_for_the_target_drive_fails_and_the_rest_goes_on() {
        let dir = test_dir("too-big");
        write(&dir.join("src/big.bin"), "01234567890123456789");
        write(&dir.join("src/small.bin"), "01234");
        std::fs::create_dir(dir.join("dst")).unwrap();
        let engine = engine();
        let root = fs::drive_root(&dir).unwrap();
        let facts = fs::drive_facts(&dir).unwrap();
        lock(&engine.0.drives).insert(root, DriveFacts { max_file: Some(10), ..facts });
        let sources = vec![dir.join("src/big.bin"), dir.join("src/small.bin")];
        let (report, events) =
            finish(&engine, engine.submit(Box::new(CopyTask::into(sources, &dir.join("dst")))), defaults);
        assert_eq!(report.failures.len(), 1, "{:?}", report.failures);
        assert!(report.failures[0].message.contains("too big for this drive"), "{}", report.failures[0].message);
        assert!(!events.iter().any(|e| matches!(e, Event::Paused { .. })), "not paused for space");
        assert!(!dir.join("dst/big.bin").exists());
        assert_eq!(read(&dir.join("dst/small.bin")), "01234");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The list must drop a partial file that a cancelled or failed item left and removed.
    #[test]
    fn a_cancelled_or_failed_item_reports_its_folder_as_changed() {
        let dir = test_dir("partial");
        let engine = engine();
        let failed = engine.submit(Box::new(PartialTask { target: dir.join("a.bin"), fail: true }));
        assert_eq!(run(&engine, failed).changed_dirs, std::slice::from_ref(&dir));
        let cancelled = engine.submit(Box::new(PartialTask { target: dir.join("b.bin"), fail: false }));
        wait_for("the partial file", || dir.join("b.bin").exists());
        engine.cancel(cancelled);
        assert_eq!(run(&engine, cancelled).changed_dirs, std::slice::from_ref(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Deep folders must not use up the changed folders and leave out the one on screen.
    #[test]
    fn the_folder_of_a_chosen_item_is_reported_even_after_many_others() {
        let dir = test_dir("changed-cap");
        for i in 0..(MAX_CHANGED + 50) {
            write(&dir.join(format!("victim/d{i:03}/f.txt")), "x");
        }
        let engine = engine();
        let report = run(&engine, engine.submit(Box::new(DeleteTask::new(vec![dir.join("victim")], None))));
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert!(!dir.join("victim").exists());
        assert!(
            report.changed_dirs.contains(&dir),
            "{} folders reported, not the one shown",
            report.changed_dirs.len()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_finished_copy_leaves_no_journal() {
        let dir = test_dir("journal-engine");
        write(&dir.join("src/a.txt"), "a");
        write(&dir.join("src/sub/b.txt"), "b");
        std::fs::create_dir(dir.join("dst")).unwrap();
        let settings = Settings { pending_deletes: Some(dir.join("config/pending-deletes")), ..Settings::default() };
        let engine = Engine::new(settings, || {});
        let report = run(&engine, engine.submit(Box::new(CopyTask::into(vec![dir.join("src")], &dir.join("dst")))));
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(read(&dir.join("dst/src/sub/b.txt")), "b");
        let journals = std::fs::read_dir(dir.join("config/copying")).map(|e| e.count()).unwrap_or(0);
        assert_eq!(journals, 0, "the journal goes when the copy ends");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_undo_cancelled_before_it_did_anything_stays_undoable() {
        let dir = test_dir("undo-cancelled");
        write(&dir.join("src/a.txt"), "a");
        std::fs::create_dir(dir.join("dst")).unwrap();
        let engine = engine();
        run(&engine, engine.submit(Box::new(CopyTask::into(vec![dir.join("src/a.txt")], &dir.join("dst")))));
        // A job on the same drive keeps the undo waiting; it is cancelled meanwhile.
        let log = Arc::default();
        let gate = Gate::default();
        let mut busy = FakeTask::new("busy", 1, &log);
        busy.paths = vec![dir.clone()];
        busy.gate = Some(gate.clone());
        let busy = engine.submit(Box::new(busy));
        wait_for("busy to start", || lock(&log).contains(&"busy+0".to_owned()));
        let undo = engine.undo().unwrap();
        engine.cancel(undo);
        run(&engine, undo);
        gate.open();
        run(&engine, busy);
        assert!(dir.join("dst/a.txt").exists());
        assert_eq!(engine.undo_label().as_deref(), Some("Copy 1 item"), "the copy is still undoable");
        assert_eq!(engine.redo_label(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_undo_whose_items_fail_does_not_block_older_actions() {
        let dir = test_dir("undo-failed");
        write(&dir.join("a.txt"), "a");
        write(&dir.join("b.txt"), "b");
        let engine = engine();
        run(&engine, engine.submit(Box::new(RenameTask::one(dir.join("a.txt"), "a2.txt"))));
        run(&engine, engine.submit(Box::new(RenameTask::one(dir.join("b.txt"), "b2.txt"))));
        std::fs::remove_file(dir.join("b2.txt")).unwrap();
        let report = run(&engine, engine.undo().unwrap());
        assert_eq!(report.failures.len() + report.skipped_changed, 1, "{report:?}");
        // The next undo reaches the older rename instead of failing on the same one again.
        run(&engine, engine.undo().unwrap());
        assert_eq!(read(&dir.join("a.txt")), "a");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn undo_of_replace_brings_the_old_file_back() {
        let dir = test_dir("undo-replace");
        write(&dir.join("src/a.txt"), "new");
        write(&dir.join("dst/a.txt"), "old");
        let engine = engine();
        let job = engine.submit(Box::new(CopyTask::into(vec![dir.join("src/a.txt")], &dir.join("dst"))));
        finish(&engine, job, |c| vec![Decision::Replace; c.len()]);
        assert_eq!(read(&dir.join("dst/a.txt")), "new");
        let report = run(&engine, engine.undo().unwrap());
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(read(&dir.join("dst/a.txt")), "old");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_move_that_replaced_its_target_is_reported_as_moved() {
        let dir = test_dir("move-replace-report");
        write(&dir.join("src/a.txt"), "new");
        write(&dir.join("dst/a.txt"), "old");
        let engine = engine();
        let job = engine.submit(Box::new(MoveTask::into(vec![dir.join("src/a.txt")], &dir.join("dst"))));
        let (report, _) = finish(&engine, job, |c| vec![Decision::Replace; c.len()]);
        assert_eq!(read(&dir.join("dst/a.txt")), "new");
        assert_eq!(report.moved, [(dir.join("src/a.txt"), dir.join("dst/a.txt"))]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn undo_of_trash_restores() {
        let dir = test_dir("undo-trash");
        write(&dir.join("x/y.txt"), "y");
        let engine = engine();
        run(&engine, engine.submit(Box::new(TrashTask::new(vec![dir.join("x")]))));
        assert!(!dir.join("x").exists());
        assert_eq!(engine.undo_label().as_deref(), Some("Delete 1 item"));
        run(&engine, engine.undo().unwrap());
        assert_eq!(read(&dir.join("x/y.txt")), "y");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn undo_of_a_merging_move_moves_the_files_back() {
        let dir = test_dir("undo-merge");
        write(&dir.join("src/d/a.txt"), "a");
        write(&dir.join("dst/d/b.txt"), "b");
        let engine = engine();
        run(&engine, engine.submit(Box::new(MoveTask::into(vec![dir.join("src/d")], &dir.join("dst")))));
        assert!(!dir.join("src/d").exists());
        run(&engine, engine.undo().unwrap());
        assert_eq!(read(&dir.join("src/d/a.txt")), "a");
        assert_eq!(read(&dir.join("dst/d/b.txt")), "b");
        assert!(!dir.join("dst/d/a.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_new_action_ends_redo_and_deletes_are_not_undoable() {
        let dir = test_dir("undo-new-action");
        write(&dir.join("a.txt"), "a");
        write(&dir.join("b.txt"), "b");
        let engine = engine();
        run(&engine, engine.submit(Box::new(RenameTask::one(dir.join("a.txt"), "c.txt"))));
        run(&engine, engine.undo().unwrap());
        assert!(engine.redo_label().is_some());
        let (_, events) =
            finish(&engine, engine.submit(Box::new(RenameTask::one(dir.join("b.txt"), "d.txt"))), defaults);
        assert!(events.contains(&Event::History));
        assert_eq!(engine.redo_label(), None);
        run(&engine, engine.submit(Box::new(DeleteTask::new(vec![dir.join("d.txt")], None))));
        assert_eq!(engine.undo_label().as_deref(), Some("Rename"), "the delete is not on the undo stack");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn wait_for(what: &str, mut ready: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !ready() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn a_job_reports_added_progress_and_finished() {
        let engine = engine();
        let log = Arc::default();
        let job = engine.submit(Box::new(FakeTask::new("a", 3, &log)));
        let (report, events) = finish(&engine, job, defaults);
        assert!(matches!(events.first(), Some(Event::Added { title, background: false, .. }) if title == "fake a"));
        assert!(!report.cancelled && report.failures.is_empty());
        assert_eq!(lock(&log).len(), 6);
        assert!(engine.wait_idle(Duration::from_secs(5)));
        assert!(!engine.busy());
    }

    #[test]
    fn jobs_on_the_same_drive_wait_their_turn() {
        let dir = test_dir("queue");
        let engine = engine();
        let log = Arc::default();
        let gate = Gate::default();
        let mut first = FakeTask::new("a", 1, &log);
        first.paths = vec![dir.clone()];
        first.gate = Some(gate.clone());
        let mut second = FakeTask::new("b", 1, &log);
        second.paths = vec![dir.clone()];
        let a = engine.submit(Box::new(first));
        let b = engine.submit(Box::new(second));
        wait_for("a to start", || lock(&log).contains(&"a+0".to_owned()));
        std::thread::sleep(Duration::from_millis(100));
        assert!(!lock(&log).iter().any(|e| e.starts_with('b')), "b waits for a: {:?}", lock(&log));
        gate.open();
        finish(&engine, a, defaults);
        finish(&engine, b, defaults);
        assert_eq!(*lock(&log), ["a+0", "a-0", "b+0", "b-0"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn jobs_without_a_shared_drive_run_together() {
        let engine = engine();
        let log = Arc::default();
        let gate = Gate::default();
        let running = Arc::new(AtomicUsize::new(0));
        let make = |name| {
            let mut task = FakeTask::new(name, 1, &log);
            task.gate = Some(gate.clone());
            task.running = running.clone();
            task
        };
        let a = engine.submit(Box::new(make("a")));
        let b = engine.submit(Box::new(make("b")));
        wait_for("both to run at once", || running.load(Ordering::SeqCst) == 2);
        gate.open();
        finish(&engine, a, defaults);
        finish(&engine, b, defaults);
    }

    #[test]
    fn start_now_skips_the_queue() {
        let dir = test_dir("start-now");
        let engine = engine();
        let log = Arc::default();
        let gate = Gate::default();
        let mut first = FakeTask::new("a", 1, &log);
        first.paths = vec![dir.clone()];
        first.gate = Some(gate.clone());
        let mut second = FakeTask::new("b", 1, &log);
        second.paths = vec![dir.clone()];
        let a = engine.submit(Box::new(first));
        let b = engine.submit(Box::new(second));
        wait_for("a to start", || lock(&log).contains(&"a+0".to_owned()));
        engine.start_now(b);
        finish(&engine, b, defaults);
        gate.open();
        finish(&engine, a, defaults);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pause_holds_and_cancel_ends_a_job() {
        let engine = engine();
        let log = Arc::default();
        let gate = Gate::default();
        let mut task = FakeTask::new("a", 50, &log);
        task.gate = Some(gate.clone());
        let job = engine.submit(Box::new(task));
        wait_for("the first item", || !lock(&log).is_empty());
        engine.pause(job);
        gate.open();
        std::thread::sleep(Duration::from_millis(150));
        let done = lock(&log).len();
        std::thread::sleep(Duration::from_millis(150));
        assert_eq!(lock(&log).len(), done, "nothing new starts while paused");
        engine.cancel(job);
        let (report, _) = finish(&engine, job, defaults);
        assert!(report.cancelled);
        assert!(lock(&log).len() < 100, "not every item ran");
    }

    #[test]
    fn many_failures_in_a_row_pause_the_job_until_resumed() {
        let engine = engine();
        engine.set_threads(CopyThreads::Fixed(1));
        let log = Arc::default();
        let mut task = FakeTask::new("a", 30, &log);
        task.fail = (0..30).collect();
        let job = engine.submit(Box::new(task));
        let deadline = Instant::now() + Duration::from_secs(10);
        let report = loop {
            let mut finished = None;
            for event in engine.drain() {
                match event {
                    Event::Paused { job: j, reason: PauseReason::ManyFailures, .. } if j == job => {
                        assert_eq!(lock(&log).len(), 40, "paused after 20 failures");
                        engine.resume(job);
                    }
                    Event::Finished { job: j, report } if j == job => finished = Some(report),
                    _ => {}
                }
            }
            if let Some(report) = finished {
                break report;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(report.failures.len(), 30);
    }

    #[test]
    fn a_full_disk_pauses_and_the_item_is_tried_again() {
        let engine = engine();
        engine.set_threads(CopyThreads::Fixed(1));
        let log = Arc::default();
        let task = FakeTask::new("a", 3, &log);
        *lock(&task.disk_full_once) = vec![1];
        let job = engine.submit(Box::new(task));
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut paused = 0;
        let report = loop {
            let mut finished = None;
            for event in engine.drain() {
                match event {
                    Event::Paused { job: j, reason: PauseReason::DiskFull, .. } if j == job => {
                        paused += 1;
                        engine.resume(job);
                    }
                    Event::Finished { job: j, report } if j == job => finished = Some(report),
                    _ => {}
                }
            }
            if let Some(report) = finished {
                break report;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(paused, 1);
        assert!(report.failures.is_empty());
        assert_eq!(lock(&log).iter().filter(|e| *e == "a+1").count(), 2, "item 1 ran twice");
    }
}
