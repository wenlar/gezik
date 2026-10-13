//! What the engine runs: a `Task` lists its items (the plan), then does them one by one.

use std::cell::Cell;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use gezik_core::ops::conflict::{Decision, Facts};
use gezik_i18n::{t, tn};

use crate::control::Control;
use crate::engine::{Event, Job, Shared, lock};
use crate::journal::Journal;
use crate::pending::{COPYING_PREFIX, HIDDEN_PREFIX, PendingDeletes, copy_prefix};
use crate::walk::facts_of;

/// Files at least this big are copied under a temporary name and renamed when complete;
/// smaller ones under their own name, noted in the job's journal (a rename per small file
/// would double the time of a copy of many of them).
pub(crate) const TEMP_COPY_MIN: u64 = 64 * 1024 * 1024;

/// How one job keeps what it copies from looking finished if Gezik is killed: temporary names
/// for large files (`prefix` and a number, their folders noted in `pending-deletes`), a
/// journal for the others. Both are cleaned up at the next start.
pub(crate) struct TempCopies {
    prefix: String,
    pending: Option<std::sync::Arc<PendingDeletes>>,
    noted: std::sync::Mutex<std::collections::HashSet<PathBuf>>,
    next: std::sync::atomic::AtomicU64,
    journal: Journal,
}

impl TempCopies {
    pub fn new(pending: Option<std::sync::Arc<PendingDeletes>>) -> TempCopies {
        let prefix = copy_prefix();
        let id = prefix.trim_start_matches(COPYING_PREFIX).trim_end_matches('-').to_owned();
        let journal = Journal::new(pending.as_ref().map(|pending| pending.journal_dir()), &id);
        TempCopies { prefix, pending, noted: Default::default(), next: Default::default(), journal }
    }

    /// A fresh temporary path next to `target`; its folder is noted before anything is written.
    pub(crate) fn next_to(&self, target: &Path) -> Option<PathBuf> {
        let folder = target.parent()?;
        if let Some(pending) = &self.pending {
            // Held while noting: no other item writes into the folder before the note is there.
            let mut noted = lock(&self.noted);
            if !noted.contains(folder) {
                pending.add_copies(std::process::id(), folder, &self.prefix);
                noted.insert(folder.to_path_buf());
            }
        }
        let number = self.next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Some(folder.join(format!("{}{number}", self.prefix)))
    }

    /// The job ended: every file it copied is finished or gone.
    pub fn done(&self) {
        if let Some(pending) = &self.pending
            && !lock(&self.noted).is_empty()
        {
            pending.remove_copies(&self.prefix);
        }
        self.journal.done();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskKind {
    Copy,
    Move,
    Rename,
    Trash,
    Delete,
    Restore,
    NewFolder,
    NewFile,
    Extract,
    Compress,
    AddToArchive,
    Download,
    Convert,
    Command,
    /// A PDF job (merge, split, extract, to pictures, pictures to PDF). PDF jobs bring their
    /// own Undo label; the verb is only a fallback.
    Pdf,
    /// New folder with selection: a new folder and the items moved into it (spec 8.2).
    NewFolderWith,
    /// Shortcuts, junctions and symbolic links (spec 9.2).
    Link,
    /// Permissions, owner, group, flags (the Info window, spec 9 §4.4).
    Attributes,
    /// Keep on this device / Free up space (spec 9 §7.3).
    KeepOnDevice,
    FreeUpSpace,
    /// Done by the one-shot administrator helper (spec 9 §10).
    Elevated,
    /// Checksums computed or verified (spec 11 §4.3): reads only, never undone.
    Checksum,
    /// A folder sync (spec 11 §8.2): copies both ways, replaced and trashed files, one undo.
    Sync,
    /// Tags, rating or comment changed (spec 12 §3.5).
    Tags,
}

impl TaskKind {
    /// What the Undo item says: "Copy 3 items", "Rename".
    pub fn label(self, count: usize) -> String {
        let n = count;
        match self {
            TaskKind::Copy => tn!("Copy {n} item" | "Copy {n} items", n),
            TaskKind::Move => tn!("Move {n} item" | "Move {n} items", n),
            TaskKind::Rename if n <= 1 => t!("Rename").into_owned(),
            TaskKind::Rename => tn!("Rename {n} item" | "Rename {n} items", n),
            TaskKind::Trash => tn!("Delete {n} item" | "Delete {n} items", n),
            TaskKind::Delete => tn!("Delete permanently {n} item" | "Delete permanently {n} items", n),
            TaskKind::Restore => tn!("Restore {n} item" | "Restore {n} items", n),
            TaskKind::NewFolder => t!("New folder").into_owned(),
            TaskKind::NewFile => t!("New file").into_owned(),
            TaskKind::Extract => tn!("Extract {n} item" | "Extract {n} items", n),
            TaskKind::Compress => tn!("Compress {n} item" | "Compress {n} items", n),
            TaskKind::AddToArchive => tn!("Add to archive {n} item" | "Add to archive {n} items", n),
            TaskKind::Download => tn!("Download {n} item" | "Download {n} items", n),
            TaskKind::Convert => tn!("Convert {n} item" | "Convert {n} items", n),
            TaskKind::Command => tn!("Run command on {n} item" | "Run command on {n} items", n),
            TaskKind::Pdf => tn!("Make PDF from {n} item" | "Make PDF from {n} items", n),
            TaskKind::NewFolderWith => tn!("New folder with {n} item" | "New folder with {n} items", n),
            TaskKind::Link if n <= 1 => t!("Create link").into_owned(),
            TaskKind::Link => tn!("Create {n} link" | "Create {n} links", n),
            TaskKind::Attributes => tn!("Change attributes of {n} item" | "Change attributes of {n} items", n),
            TaskKind::KeepOnDevice => tn!("Keep {n} item on this device" | "Keep {n} items on this device", n),
            TaskKind::FreeUpSpace => tn!("Free up space of {n} item" | "Free up space of {n} items", n),
            // Its jobs always bring their own label; this is a fallback.
            TaskKind::Elevated => tn!("Do as administrator {n} item" | "Do as administrator {n} items", n),
            TaskKind::Checksum => tn!("Compute checksums of {n} item" | "Compute checksums of {n} items", n),
            TaskKind::Sync => tn!("Synchronize {n} item" | "Synchronize {n} items", n),
            TaskKind::Tags => tn!("Change tags of {n} item" | "Change tags of {n} items", n),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Work {
    /// Bound by the disk: as many workers as the disk kind suits.
    Disk,
    /// Bound by the processor (converting pictures, packing archives): one per core.
    Cpu,
    /// Runs other programs (ffmpeg): a couple at a time.
    External,
}

#[derive(Debug, Clone)]
pub struct Resources {
    /// Paths on every drive the task touches (sources and target folders).
    pub paths: Vec<PathBuf>,
    pub work: Work,
}

/// When an item runs. `Before` items run one by one as they are planned (folders to make,
/// before what goes in them); `Parallel` items on the workers; `After` items one by one once
/// everything else is done, in reverse plan order (folders to remove: the deepest first).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Before,
    Parallel,
    After,
}

#[derive(Debug, Clone)]
pub struct PlanItem {
    /// What it acts on; `None` for something new.
    pub source: Option<PathBuf>,
    /// Where the result goes; checked for an existing entry if `check_target`.
    pub target: Option<PathBuf>,
    /// The source's facts (for something new: only `is_dir`).
    pub facts: Facts,
    pub stage: Stage,
    pub check_target: bool,
    /// Settles a conflict without asking (a copy into its own folder keeps both).
    pub preset: Option<Decision>,
    /// One of the things the user chose (not something inside one).
    pub is_root: bool,
    /// Which of the task's roots it belongs to.
    pub root: usize,
    /// The task's own note on what to do with it.
    pub tag: u8,
    /// The existing target goes to the trash first (set by the engine for "Replace").
    pub(crate) replace: bool,
    /// Counts in the job's progress (a rename's step to a temporary name does not).
    pub(crate) counted: bool,
    /// A `Before` folder meeting a folder merges into it without asking; `false`: it is a
    /// conflict like any other (a folder coming back from the trash).
    pub(crate) merges: bool,
    /// Bound where an earlier item of the job goes: runs after every other item, in the order
    /// these were planned (set by the engine).
    pub(crate) waits: bool,
}

impl PlanItem {
    pub fn new(stage: Stage, facts: Facts) -> PlanItem {
        PlanItem {
            source: None,
            target: None,
            facts,
            stage,
            check_target: false,
            preset: None,
            is_root: false,
            root: 0,
            tag: 0,
            replace: false,
            counted: true,
            merges: true,
            waits: false,
        }
    }

    pub fn source(mut self, path: impl Into<PathBuf>) -> PlanItem {
        self.source = Some(path.into());
        self
    }

    pub fn target(mut self, path: impl Into<PathBuf>) -> PlanItem {
        self.target = Some(path.into());
        self
    }

    pub fn checked(mut self) -> PlanItem {
        self.check_target = true;
        self
    }

    /// Root `root` itself.
    pub fn top(mut self, root: usize) -> PlanItem {
        self.is_root = true;
        self.root = root;
        self
    }

    /// Something inside root `root`.
    pub fn under(mut self, root: usize) -> PlanItem {
        self.root = root;
        self
    }

    pub fn preset(mut self, decision: Option<Decision>) -> PlanItem {
        self.preset = decision;
        self
    }

    pub fn tag(mut self, tag: u8) -> PlanItem {
        self.tag = tag;
        self
    }

    /// A folder that never merges into one already there: see `merges`.
    pub fn no_merge(mut self) -> PlanItem {
        self.merges = false;
        self
    }

    /// Left out of the job's progress: a step on the way, not an item of its own.
    pub fn uncounted(mut self) -> PlanItem {
        self.counted = false;
        self
    }

    /// The path a failure names: the source, else the target.
    pub fn path(&self) -> &Path {
        self.source.as_deref().or(self.target.as_deref()).unwrap_or(Path::new(""))
    }
}

/// What doing an item changed; the engine builds undo from these.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// `path` was made: copied (`from` none), moved across drives (`from` the original), or new.
    Created {
        path: PathBuf,
        facts: Facts,
        from: Option<PathBuf>,
    },
    /// A folder placed with what is in it in one step (extracted, a command's output): undo
    /// takes it away whole. A folder `Created` empty goes only if it still holds no files.
    Placed {
        path: PathBuf,
    },
    /// Renamed or moved in one step.
    Moved {
        from: PathBuf,
        to: PathBuf,
        facts: Facts,
    },
    Trashed {
        original: PathBuf,
        trashed: PathBuf,
    },
    Restored {
        original: PathBuf,
        facts: Facts,
    },
    Deleted {
        path: PathBuf,
    },
    /// A folder made on the way to the items (copy or move with folders): undo takes it away
    /// only if it then holds no files, after what went into it was undone (spec 8.2).
    MadeParent {
        path: PathBuf,
    },
    /// Permissions, owner, group or flags changed: undo writes `before` back if the item is
    /// still `id` and still `after`.
    AttributesChanged {
        path: PathBuf,
        id: gezik_core::attrs::Identity,
        before: gezik_core::attrs::Attrs,
        after: gezik_core::attrs::Attrs,
    },
    /// Tags, rating or comment changed: undo writes `before` back if they are still `after`.
    MetaChanged {
        path: PathBuf,
        before: gezik_core::meta::Meta,
        after: gezik_core::meta::Meta,
    },
    /// Done by the administrator helper and seen on disk: undo runs `undo`, as administrator too.
    AsAdmin {
        undo: gezik_core::elevated::Op,
    },
    /// Nothing changed (a folder that was already there).
    Nothing,
    /// One item did several of these (an archive replaced: the old one trashed, the new made).
    Several(Vec<Outcome>),
}

impl Outcome {
    /// Where the result is now, to select it afterwards.
    pub fn result(&self) -> Option<&Path> {
        match self {
            Outcome::Created { path, .. }
            | Outcome::Placed { path }
            | Outcome::Restored { original: path, .. }
            | Outcome::Moved { to: path, .. } => Some(path),
            Outcome::Several(outcomes) => outcomes.iter().find_map(Outcome::created),
            _ => None,
        }
    }

    /// The first path made, looking inside `Several` too.
    fn created(&self) -> Option<&Path> {
        match self {
            Outcome::Created { path, .. } | Outcome::Placed { path } => Some(path),
            Outcome::Several(outcomes) => outcomes.iter().find_map(Outcome::created),
            _ => None,
        }
    }
}

/// What a job asks the user while it runs; it waits for an [`Answer`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Question {
    /// The archive is encrypted; `retry`: the last password was wrong.
    Password { archive: PathBuf, retry: bool },
    /// A choice between `buttons`.
    Confirm { title: String, message: String, buttons: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    Text(String),
    /// Which of the question's buttons.
    Button(usize),
    Cancel,
}

/// Where a task's plan goes.
pub trait ScanSink {
    /// Adds an item; false once the job is cancelled (stop planning).
    fn item(&mut self, item: PlanItem) -> bool;
    /// Something could not be read; it is reported as failed.
    fn failed(&mut self, path: &Path, error: io::Error);
    /// Whether the job was cancelled, for a plan that waits (the files a macOS drop promised).
    fn cancelled(&self) -> bool {
        false
    }
}

pub trait Task: Send + Sync {
    fn kind(&self) -> TaskKind;
    /// The panel's line while it runs: "Copying 3 items to D:\Yedek".
    fn title(&self) -> String;
    /// How many things the user chose (for "Copy 3 items").
    fn count(&self) -> usize;
    fn resources(&self) -> Resources;
    /// How many items run at once, when the task knows better than its kind of work (one
    /// ffmpeg at a time, a user command's `parallel`); `None`: as `Work` says.
    fn workers(&self) -> Option<usize> {
        None
    }
    /// Lists what to do; a folder before what is in it.
    fn plan(&self, sink: &mut dyn ScanSink);
    /// Does `item`. Its target is free: conflicts are settled before.
    fn run(&self, item: &PlanItem, cx: &RunCx<'_>) -> io::Result<Outcome>;
    /// Called once the job ends.
    fn done(&self, _cancelled: bool) {}
    /// Whether two of its items may be bound for one place (two binned `x.txt` from one
    /// folder): the engine then holds the later one as a conflict, at 8 bytes per item.
    fn same_targets(&self) -> bool {
        false
    }
    /// The chosen items whose work the system refused (`denied`: the failures' paths), as
    /// operations the administrator helper can do (spec 9 §10.1: Retry as administrator). Only
    /// the plain kinds offer it; the rest offer nothing.
    fn as_admin(&self, _denied: &[PathBuf]) -> Vec<gezik_core::elevated::Op> {
        Vec::new()
    }
    /// What it sends the administrator helper (an `ElevatedTask`'s list): the app shows it
    /// before an undo or redo asks for the prompt.
    fn elevated_ops(&self) -> &[gezik_core::elevated::Op] {
        &[]
    }
}

/// What `Task::run` may use: progress, pause and cancel, the drive's trash, questions.
pub struct RunCx<'a> {
    pub(crate) control: &'a Control,
    pub(crate) trash: &'a dyn Fn(&Path) -> bool,
    /// Bytes this item counted so far (the engine counts the rest when it is done).
    pub(crate) added: Cell<u64>,
    /// The job's temporary names for copies.
    pub(crate) temp: &'a TempCopies,
    /// The engine and the job it runs in (`None` in tests that run an item alone).
    pub(crate) job: Option<(&'a Shared, &'a Job)>,
}

impl RunCx<'_> {
    pub fn add_bytes(&self, bytes: u64) {
        self.added.set(self.added.get() + bytes);
        self.control.add_bytes(bytes);
    }

    /// Waits while the job is paused; true once it is cancelled.
    pub fn stopped(&self) -> bool {
        self.control.stopped()
    }

    /// The user said Cancel to this item's question: the whole job ends as cancelled.
    pub fn cancel_job(&self) {
        self.control.cancel();
    }

    /// Whether the job is cancelled; does not wait.
    pub fn cancelled(&self) -> bool {
        self.control.cancelled()
    }

    /// Whether the job is paused now; does not wait. An item that runs a program ends it
    /// then (the program would go on using the machine), removes what it wrote and returns
    /// [`restart`]: the engine waits until the job is resumed and does the item again.
    pub fn paused(&self) -> bool {
        self.control.paused()
    }

    /// The bytes this item counted so far (with `add_bytes`), over all its tries: an item
    /// tried again after a full disk counts only what goes past them. An item that returned
    /// [`restart`] starts again from 0 (the engine took back what it counted).
    pub fn counted(&self) -> u64 {
        self.added.get()
    }

    pub fn has_trash(&self, path: &Path) -> bool {
        (self.trash)(path)
    }

    /// Asks the user and waits for the answer (`Cancel` once the job is cancelled). One
    /// question at a time: other items asking wait their turn.
    pub fn ask(&self, question: Question) -> Answer {
        let Some((shared, job)) = self.job else { return Answer::Cancel };
        let control = self.control;
        let _turn = lock(&control.asking_turn);
        if control.cancelled() {
            return Answer::Cancel;
        }
        let id = control.new_question();
        control.asking.store(true, Ordering::SeqCst);
        shared.push([Event::Question { job: job.id, id, question }]);
        let answer = control.wait_answer();
        control.asking.store(false, Ordering::SeqCst);
        answer.unwrap_or(Answer::Cancel)
    }

    /// Work found while running (the entries of an archive): added to the job's totals.
    pub fn found(&self, items: u64, bytes: u64) {
        self.control.add_total(items, bytes);
    }

    /// One piece of found work is done.
    pub fn one_done(&self, bytes: u64) {
        self.control.item_done();
        self.add_bytes(bytes);
    }

    /// Reports `path` as failed; the item goes on.
    pub fn fail(&self, path: &Path, err: &io::Error) {
        if let Some((_, job)) = self.job {
            job.fail(path, err);
        }
    }

    /// The folders holding `paths` changed (an item that works in many places: the
    /// administrator's list).
    pub fn touched<'p>(&self, paths: impl IntoIterator<Item = &'p Path>) {
        if let Some((_, job)) = self.job {
            job.touch(paths, true);
        }
    }

    /// `path` was done by the administrator helper, but Gezik cannot look there to check it.
    pub(crate) fn unchecked(&self, path: &Path) {
        if let Some((_, job)) = self.job {
            job.unchecked(path);
        }
    }

    /// How administrator operations run here, if the engine was given a way.
    pub(crate) fn elevator(&self) -> Option<std::sync::Arc<dyn crate::tasks::Elevator>> {
        self.job.and_then(|(shared, _)| lock(&shared.elevator).clone())
    }

    /// Notes that `path` was left out on purpose (`why`); not a failure.
    pub fn skip(&self, path: &Path, why: &io::Error) {
        if let Some((_, job)) = self.job {
            job.skip(path, why);
        }
    }

    /// Notes something about `path` the user should know that is neither a failure nor a skip
    /// (its tags went to Gezik's store).
    pub fn note(&self, path: &Path, message: &str) {
        if let Some((_, job)) = self.job {
            job.note(path, message);
        }
    }

    /// Where tags are read and written (`None` in tests that run an item alone).
    pub fn meta(&self) -> Option<&crate::meta::MetaStore> {
        self.job.map(|(shared, _)| &*shared.meta)
    }

    /// A new hidden folder next to `near` to build things in. It is noted for removal if
    /// Gezik stops, and removed (with what is left in it) when the job ends.
    pub fn staging_dir(&self, near: &Path) -> io::Result<PathBuf> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let parent = near.parent().unwrap_or(Path::new(""));
        loop {
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = parent.join(format!("{HIDDEN_PREFIX}x-{}-{n}", std::process::id()));
            // Noted first: a crash between the note and the folder leaves a path that does not
            // exist, which recovery drops. A path no line can hold stays hidden after a crash.
            if let Some(pending) = self.pending().filter(|_| crate::pending::can_hold(&path)) {
                pending.add(&path)?;
            }
            // Private to the user on Unix: an archive may be unpacked here (decrypted).
            #[cfg_attr(not(unix), allow(unused_mut))]
            let mut builder = std::fs::DirBuilder::new();
            #[cfg(unix)]
            std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
            match builder.create(&path) {
                Ok(()) => {
                    let _ = gezik_platform::fs::set_hidden(&path);
                    if let Some((_, job)) = self.job {
                        lock(&job.staging).push(path.clone());
                    }
                    return Ok(path);
                }
                // A leftover of an earlier run with the same process id: it stays noted (it
                // goes at the next start) and the next name is tried.
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {}
                Err(err) => {
                    if let Some(pending) = self.pending() {
                        pending.remove(&path);
                    }
                    return Err(err);
                }
            }
        }
    }

    /// A temporary name next to `target` to write it under until it is complete; a leftover
    /// is deleted at the next start.
    pub fn temp_file_for(&self, target: &Path) -> PathBuf {
        self.temp.next_to(target).unwrap_or_else(|| {
            let number = self.temp.next.fetch_add(1, Ordering::Relaxed);
            target.with_file_name(format!("{}{number}", self.temp.prefix))
        })
    }

    /// A name next to `path` to set it aside under for a while (`.gezik-rn-…`: no temporary
    /// file cleanup deletes it).
    pub fn aside_name(&self, path: &Path) -> PathBuf {
        crate::pending::renaming_name(path)
    }

    /// Notes items set aside, as (where it is now, its own name): if Gezik stops before
    /// `forget_aside`, the next start puts each back under its own name (or a free `name (n)`).
    pub fn note_aside(&self, pairs: &[(PathBuf, PathBuf)]) {
        if let Some(pending) = self.pending() {
            let restores: Vec<crate::pending::Restore> = pairs
                .iter()
                .map(|(aside, original)| crate::pending::Restore {
                    hidden: aside.clone(),
                    original: original.clone(),
                    was_hidden: true,
                })
                .collect();
            pending.add_restores(&restores);
        }
    }

    /// The items set aside at `asides` are back, trashed or deleted: nothing to put back.
    pub fn forget_aside(&self, asides: &[&Path]) {
        if let Some(pending) = self.pending() {
            pending.remove_restores(asides);
        }
    }

    /// Where the job notes what to put back if Gezik stops (`None`: nowhere).
    pub(crate) fn pending(&self) -> Option<&std::sync::Arc<PendingDeletes>> {
        self.temp.pending.as_ref()
    }

    /// Copies a file, counting its bytes and stopping when the job is cancelled. The system
    /// makes the copy its full size at once, so if Gezik is killed meanwhile the leftover must
    /// not pass for a finished file: a large one is copied under a temporary name and renamed
    /// when complete, a small one is noted in the job's journal first (see `TempCopies`), unless
    /// the journal cannot hold its names: it takes the temporary name too. With the job's
    /// verification on, every copy takes a temporary name and is checked before it takes its
    /// own (spec 11 §5.1): a copy that differs never stands under the final name. A link is
    /// copied as a link and not checked (reading it would read what it points to).
    /// Its tags go along where the system does not take them (spec 12 §10): a failure there
    /// is a note, the copy stands.
    pub fn copy_file(&self, from: &Path, to: &Path, size: u64) -> io::Result<()> {
        self.copy_bytes(from, to, size)?;
        self.carry_tags(from, to);
        Ok(())
    }

    /// After `from` was copied to `to` (under its final name): its tags go along where the
    /// system did not take them; a failure is a note.
    pub(crate) fn carry_tags(&self, from: &Path, to: &Path) {
        if let Some(meta) = self.meta()
            && let Err(err) = meta.carry(from, to)
        {
            self.note(to, &t!("Tags could not be carried: {}", gezik_platform::fs::describe(&err)));
        }
    }

    /// [`copy_file`](Self::copy_file) without the tags, for a copy made under a temporary name
    /// and renamed by its task: [`carry_tags`](Self::carry_tags) once it has its own name.
    pub(crate) fn copy_bytes(&self, from: &Path, to: &Path, size: u64) -> io::Result<()> {
        let verify = self.control.verify.load(Ordering::Relaxed)
            && !std::fs::symlink_metadata(from).is_ok_and(|meta| meta.file_type().is_symlink());
        let temp =
            if verify || size >= TEMP_COPY_MIN || !Journal::can_note(from, to) { self.temp.next_to(to) } else { None };
        let Some(temp) = temp else {
            self.temp.journal.note(from, to);
            return self.copy_checked(from, to, size, verify);
        };
        let mut kept = false;
        let result = self
            .copy_checked(from, &temp, size, verify)
            .inspect_err(|err| kept = is_marker::<Leftover>(err))
            .and_then(|()| gezik_platform::fs::move_entry(&temp, to));
        if result.is_err() && !kept {
            let _ = gezik_platform::fs::delete(&temp);
        }
        result
    }

    /// Copies `from` to `to`; if `verify`, reads the copy back past the system's cache and
    /// compares it with `from`. One that differs is deleted and copied once more; a second
    /// difference (or a failed check) fails the item with no copy left, and one that cannot be
    /// deleted fails it at once naming where it stays (a [`Leftover`]).
    fn copy_checked(&self, from: &Path, to: &Path, size: u64, verify: bool) -> io::Result<()> {
        let mut again = false;
        loop {
            let cloned = self.copy_to(from, to, size)?;
            if cloned || !verify {
                return Ok(());
            }
            #[cfg(test)]
            if self.control.spoil.try_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1)).is_ok() {
                spoil(to);
            }
            let differs = self.compare(from, to, size);
            if let Ok(None) = differs {
                return Ok(());
            }
            let deleted = gezik_platform::fs::delete(to);
            let at = differs?.unwrap_or_default();
            let at = gezik_core::group_digits(at);
            if let Err(err) = deleted {
                return Err(io::Error::other(Leftover(t!(
                    "The copy differs from the original at byte {0} and could not be deleted ({1}); it is at {2}",
                    at,
                    gezik_platform::fs::describe(&err),
                    to.display()
                ))));
            }
            if again {
                return Err(io::Error::other(t!("The copy differs from the original at byte {}", at)));
            }
            again = true;
            // The copy is done once more: the bar counts it.
            self.control.add_total(0, size);
        }
    }

    /// Where the copy at `to` first differs from `from` (`None`: nowhere), counted in the job's
    /// progress as one more pass over the file.
    fn compare(&self, from: &Path, to: &Path, size: u64) -> io::Result<Option<u64>> {
        let control = self.control;
        control.add_total(0, size);
        control.verifying.fetch_add(1, Ordering::SeqCst);
        let mut read = 0u64;
        let result = crate::hash::same_content(from, to, true, &mut |n| {
            read += n;
            control.add_bytes(n);
            !self.stopped()
        });
        control.verifying.fetch_sub(1, Ordering::SeqCst);
        // The time spent reading was not spent copying: the limit saves none of it up.
        let speed = control.speed.load(Ordering::Relaxed);
        if speed > 0 {
            lock(&control.bucket).idle(speed, Instant::now());
        }
        // A short or failed check still leaves the bar where the pass would have ended.
        control.add_bytes(size.saturating_sub(read));
        result
    }

    /// Copies `from` to `to` held to the job's speed limit; true if it was cloned.
    fn copy_to(&self, from: &Path, to: &Path, size: u64) -> io::Result<bool> {
        let mut counted = 0u64;
        gezik_platform::fs::copy_file(from, to, size, &mut |done| {
            let new = done.saturating_sub(counted);
            self.add_bytes(new);
            counted = counted.max(done);
            self.throttle(new);
            !self.stopped()
        })
    }

    /// Waits until `bytes` more are due at the job's speed limit, in slices short enough that
    /// a pause, a cancel or a changed limit is heard within 100 ms (the wait left is worked out
    /// again at the new limit).
    fn throttle(&self, bytes: u64) {
        const SLICE: Duration = Duration::from_millis(100);
        let control = self.control;
        let mut speed = control.speed.load(Ordering::Relaxed);
        if speed == 0 || bytes == 0 {
            return;
        }
        let mut wait = lock(&control.bucket).take(speed, bytes, Instant::now());
        while !wait.is_zero() {
            std::thread::sleep(wait.min(SLICE));
            if control.cancelled() || control.paused() {
                return;
            }
            speed = control.speed.load(Ordering::Relaxed);
            if speed == 0 {
                return;
            }
            wait = lock(&control.bucket).take(speed, 0, Instant::now());
        }
    }
}

/// A copy that differs and could not be deleted: it is left where the message says.
#[derive(Debug)]
struct Leftover(String);

impl fmt::Display for Leftover {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Leftover {}

/// Tests: changes the first byte of `path`.
#[cfg(test)]
fn spoil(path: &Path) {
    let mut data = std::fs::read(path).unwrap();
    if let Some(first) = data.first_mut() {
        *first ^= 0xFF;
    }
    std::fs::write(path, data).unwrap();
}

/// The item changed after the operation being undone: it is left alone.
#[derive(Debug)]
pub struct ChangedSince;

impl fmt::Display for ChangedSince {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&t!("changed since; skipped"))
    }
}

impl std::error::Error for ChangedSince {}

/// The item was cut short because the job was paused (the program it ran was ended and what
/// it wrote removed): it is done again from the start once the job is resumed.
#[derive(Debug)]
pub struct Restart;

impl fmt::Display for Restart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&t!("paused; it starts again when resumed"))
    }
}

impl std::error::Error for Restart {}

/// What `Task::run` returns when a pause cut its item short (see [`RunCx::paused`]).
pub fn restart() -> io::Error {
    io::Error::new(io::ErrorKind::Interrupted, Restart)
}

/// Whether `err` is [`restart`]'s.
pub fn is_restart(err: &io::Error) -> bool {
    is_marker::<Restart>(err)
}

/// The item's drive has no trash; the user is asked whether to delete it for good.
#[derive(Debug)]
pub struct NoTrash;

impl fmt::Display for NoTrash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&if cfg!(windows) { t!("This drive has no Recycle Bin") } else { t!("This drive has no trash") })
    }
}

impl std::error::Error for NoTrash {}

pub fn changed_since() -> io::Error {
    io::Error::other(ChangedSince)
}

pub fn no_trash() -> io::Error {
    io::Error::other(NoTrash)
}

/// Whether `err` carries the marker `E`.
pub(crate) fn is_marker<E: std::error::Error + 'static>(err: &io::Error) -> bool {
    err.get_ref().is_some_and(|inner| inner.is::<E>())
}

/// Whether `path` still looks as `expected` says (no expectation: yes).
pub fn unchanged(path: &Path, expected: Option<Facts>) -> bool {
    let Some(expected) = expected else { return true };
    match std::fs::symlink_metadata(path) {
        Ok(meta) => {
            let now = facts_of(&meta);
            now.size == expected.size && now.modified == expected.modified
        }
        Err(_) => false,
    }
}

/// `path`'s facts right after an item made it.
pub fn facts_after(path: &Path, is_dir: bool) -> Facts {
    std::fs::symlink_metadata(path).map(|m| facts_of(&m)).unwrap_or(Facts { is_dir, ..Facts::default() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::PauseReason;
    use crate::pending::PendingDeletes;
    use crate::testing::test_dir;

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> =
            std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names
    }

    fn some_file(path: &Path) -> u64 {
        let data: Vec<u8> = (0..TEMP_COPY_MIN + 1000).map(|i| (i % 251) as u8).collect();
        std::fs::write(path, &data).unwrap();
        data.len() as u64
    }

    #[test]
    fn a_large_copy_lands_under_its_name_only_when_complete() {
        let dir = test_dir("temp-copy");
        std::fs::create_dir(dir.join("to")).unwrap();
        let size = some_file(&dir.join("big.bin"));
        let pending = std::sync::Arc::new(PendingDeletes::new(dir.join("pending-deletes")));
        let temp = TempCopies::new(Some(pending.clone()));
        let control = Control::default();
        control.pause(PauseReason::User);
        let no_bin = |_: &Path| false;
        let (from, to) = (dir.join("big.bin"), dir.join("to/big.bin"));
        // A failed check below must not leave the copy paused: the scope would wait forever.
        struct Resume<'a>(&'a Control);
        impl Drop for Resume<'_> {
            fn drop(&mut self) {
                self.0.resume();
            }
        }
        std::thread::scope(|scope| {
            let _resume = Resume(&control);
            let copy = scope.spawn(|| {
                let cx = RunCx { control: &control, trash: &no_bin, added: Cell::new(0), temp: &temp, job: None };
                cx.copy_file(&from, &to, size)
            });
            // Held by the pause in the middle of the copy: only a noted leftover is there.
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while names(&dir.join("to")).is_empty() {
                assert!(std::time::Instant::now() < deadline, "the copy did not start");
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            let during = names(&dir.join("to"));
            assert_eq!(during.len(), 1);
            let notes = pending.copies();
            assert_eq!(notes.len(), 1, "the folder is noted for the next start");
            assert_eq!(notes[0].folder, dir.join("to"));
            assert!(during[0].starts_with(&notes[0].prefix), "{during:?}");
            control.resume();
            copy.join().unwrap().unwrap();
        });
        assert_eq!(names(&dir.join("to")), ["big.bin"]);
        assert_eq!(std::fs::read(&to).unwrap(), std::fs::read(&from).unwrap());
        temp.done();
        assert!(pending.copies().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_cancelled_large_copy_leaves_nothing() {
        let dir = test_dir("temp-copy-cancel");
        std::fs::create_dir(dir.join("to")).unwrap();
        let size = some_file(&dir.join("big.bin"));
        let pending = std::sync::Arc::new(PendingDeletes::new(dir.join("pending-deletes")));
        let temp = TempCopies::new(Some(pending.clone()));
        let control = Control::default();
        control.cancel();
        let no_bin = |_: &Path| false;
        let cx = RunCx { control: &control, trash: &no_bin, added: Cell::new(0), temp: &temp, job: None };
        assert!(cx.copy_file(&dir.join("big.bin"), &dir.join("to/big.bin"), size).is_err());
        assert!(names(&dir.join("to")).is_empty());
        temp.done();
        assert!(pending.copies().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_small_copy_whose_name_the_journal_cannot_hold_takes_a_temporary_name() {
        #[cfg(unix)]
        let name = {
            use std::os::unix::ffi::OsStringExt;
            std::ffi::OsString::from_vec(b"r\xfcz.txt".to_vec())
        };
        #[cfg(windows)]
        let name = {
            use std::os::windows::ffi::OsStringExt;
            std::ffi::OsString::from_wide(&[0x72, 0xD800, 0x2E, 0x74])
        };
        let dir = test_dir("temp-copy-name");
        std::fs::create_dir(dir.join("to")).unwrap();
        if !crate::testing::can_hold_name(&dir.join("to"), &name) {
            return;
        }
        std::fs::write(dir.join("small.txt"), "x").unwrap();
        let pending = std::sync::Arc::new(PendingDeletes::new(dir.join("pending-deletes")));
        let temp = TempCopies::new(Some(pending.clone()));
        let control = Control::default();
        let no_bin = |_: &Path| false;
        let cx = RunCx { control: &control, trash: &no_bin, added: Cell::new(0), temp: &temp, job: None };
        cx.copy_file(&dir.join("small.txt"), &dir.join("to").join(&name), 1).unwrap();
        assert_eq!(std::fs::read(dir.join("to").join(&name)).unwrap(), b"x");
        assert_eq!(pending.copies().len(), 1, "its folder is noted for the temporary name");
        assert!(!dir.join("copying").exists(), "no journal line");
        temp.done();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_item_set_aside_is_noted_until_forgotten() {
        let dir = test_dir("aside");
        let pending = std::sync::Arc::new(PendingDeletes::new(dir.join("pending-deletes")));
        let temp = TempCopies::new(Some(pending.clone()));
        let control = Control::default();
        let no_bin = |_: &Path| false;
        let cx = RunCx { control: &control, trash: &no_bin, added: Cell::new(0), temp: &temp, job: None };
        let original = dir.join("x.7z.001");
        let aside = cx.aside_name(&original);
        assert_eq!(crate::pending::renaming_pid(&aside), Some(std::process::id()), "no temp cleanup takes it");
        assert_ne!(cx.aside_name(&original), aside);
        cx.note_aside(&[(aside.clone(), original.clone())]);
        let restores = pending.restores();
        assert_eq!(restores.len(), 1);
        assert_eq!((&restores[0].hidden, &restores[0].original), (&aside, &original));
        cx.forget_aside(&[&aside]);
        assert!(pending.restores().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn labels_count_items() {
        assert_eq!(TaskKind::Copy.label(3), "Copy 3 items");
        assert_eq!(TaskKind::Trash.label(1), "Delete 1 item");
        assert_eq!(TaskKind::Rename.label(1), "Rename");
        assert_eq!(TaskKind::Rename.label(24), "Rename 24 items");
        assert_eq!(TaskKind::NewFolder.label(1), "New folder");
        assert_eq!(TaskKind::Convert.label(12), "Convert 12 items");
        assert_eq!(TaskKind::Command.label(1), "Run command on 1 item");
        assert_eq!(TaskKind::Pdf.label(3), "Make PDF from 3 items");
        assert_eq!(TaskKind::NewFolderWith.label(3), "New folder with 3 items");
        assert_eq!(TaskKind::NewFolderWith.label(1), "New folder with 1 item");
        assert_eq!(TaskKind::Link.label(1), "Create link");
        assert_eq!(TaskKind::Link.label(4), "Create 4 links");
        assert_eq!(TaskKind::Attributes.label(3), "Change attributes of 3 items");
        assert_eq!(TaskKind::KeepOnDevice.label(2), "Keep 2 items on this device");
        assert_eq!(TaskKind::KeepOnDevice.label(1), "Keep 1 item on this device");
        assert_eq!(TaskKind::FreeUpSpace.label(2), "Free up space of 2 items");
        assert_eq!(TaskKind::Checksum.label(2), "Compute checksums of 2 items");
        assert_eq!(TaskKind::Tags.label(1), "Change tags of 1 item");
    }

    #[test]
    fn markers_are_recognized() {
        assert!(is_marker::<ChangedSince>(&changed_since()));
        assert!(!is_marker::<NoTrash>(&changed_since()));
        assert!(!is_marker::<NoTrash>(&io::Error::other("x")));
    }

    fn verifying_cx<'a>(control: &'a Control, temp: &'a TempCopies, no_bin: &'a dyn Fn(&Path) -> bool) -> RunCx<'a> {
        // A clone (APFS) is neither read back nor slowed down: these test a byte copy.
        gezik_platform::fs::CLONES.store(false, Ordering::SeqCst);
        control.verify.store(true, Ordering::SeqCst);
        RunCx { control, trash: no_bin, added: Cell::new(0), temp, job: None }
    }

    #[test]
    fn a_verified_copy_counts_its_bytes_twice() {
        let dir = test_dir("verify-count");
        std::fs::write(dir.join("a.bin"), vec![5u8; 300_000]).unwrap();
        let (control, temp, no_bin) = (Control::default(), TempCopies::new(None), |_: &Path| false);
        let cx = verifying_cx(&control, &temp, &no_bin);
        cx.copy_file(&dir.join("a.bin"), &dir.join("b.bin"), 300_000).unwrap();
        assert_eq!(std::fs::read(dir.join("b.bin")).unwrap(), vec![5u8; 300_000]);
        // The plan counts the copy; the check adds a pass of its own.
        assert_eq!(control.bytes_total.load(Ordering::SeqCst), 300_000);
        assert_eq!(control.bytes_done.load(Ordering::SeqCst), 600_000);
        assert_eq!(cx.counted(), 300_000, "the item's own count is the copy");
        assert_eq!(control.verifying.load(Ordering::SeqCst), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_copy_that_differs_twice_fails_with_where_and_leaves_nothing() {
        let dir = test_dir("verify-fail");
        std::fs::write(dir.join("a.txt"), "abc").unwrap();
        let (control, temp, no_bin) = (Control::default(), TempCopies::new(None), |_: &Path| false);
        let cx = verifying_cx(&control, &temp, &no_bin);
        control.spoil.store(2, Ordering::SeqCst);
        let err = cx.copy_file(&dir.join("a.txt"), &dir.join("b.txt"), 3).unwrap_err();
        assert_eq!(err.to_string(), "The copy differs from the original at byte 0");
        assert!(!dir.join("b.txt").exists());
        // Copy, check, copy again, check again: four passes, all counted.
        assert_eq!(control.bytes_done.load(Ordering::SeqCst), 12);
        assert_eq!(control.bytes_total.load(Ordering::SeqCst), 9, "with the plan's 3: 12");
        // Off: nothing is read back, a spoiled copy passes unseen.
        control.verify.store(false, Ordering::SeqCst);
        cx.copy_file(&dir.join("a.txt"), &dir.join("b.txt"), 3).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("b.txt")).unwrap(), "abc");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_limited_copy_hears_a_cancel_at_once() {
        let dir = test_dir("limit-cancel");
        let size = 4_000_000u64;
        std::fs::write(dir.join("a.bin"), vec![1u8; size as usize]).unwrap();
        let (control, temp, no_bin) = (Control::default(), TempCopies::new(None), |_: &Path| false);
        // A clone (APFS) is neither read back nor slowed down: these test a byte copy.
        gezik_platform::fs::CLONES.store(false, Ordering::SeqCst);
        control.speed.store(1000, Ordering::SeqCst);
        let started = std::time::Instant::now();
        std::thread::scope(|scope| {
            let copy = scope.spawn(|| {
                let cx = RunCx { control: &control, trash: &no_bin, added: Cell::new(0), temp: &temp, job: None };
                cx.copy_file(&dir.join("a.bin"), &dir.join("b.bin"), size)
            });
            std::thread::sleep(Duration::from_millis(200));
            control.cancel();
            assert!(copy.join().unwrap().is_err(), "cancelled");
        });
        // At 1 KB/s the copy would take over an hour; the cancel is heard within a slice.
        assert!(started.elapsed() < Duration::from_secs(2), "{:?}", started.elapsed());
        assert!(!dir.join("b.bin").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_link_is_copied_as_a_link_and_not_read_back() {
        let dir = test_dir("verify-link");
        std::fs::write(dir.join("target.txt"), "pointed at").unwrap();
        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_file(dir.join("target.txt"), dir.join("link.txt"));
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(dir.join("target.txt"), dir.join("link.txt"));
        if made.is_err() {
            // Windows without the right to make links (no developer mode): nothing to try.
            let _ = std::fs::remove_dir_all(&dir);
            return;
        }
        let (control, temp, no_bin) = (Control::default(), TempCopies::new(None), |_: &Path| false);
        let cx = verifying_cx(&control, &temp, &no_bin);
        // Spoiling would change what the link points to: a link must not get there.
        control.spoil.store(2, Ordering::SeqCst);
        cx.copy_file(&dir.join("link.txt"), &dir.join("copy.txt"), 0).unwrap();
        assert!(std::fs::symlink_metadata(dir.join("copy.txt")).unwrap().file_type().is_symlink());
        assert_eq!(control.spoil.load(Ordering::SeqCst), 2, "not verified");
        assert_eq!(std::fs::read_to_string(dir.join("target.txt")).unwrap(), "pointed at");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_raised_limit_ends_a_long_wait() {
        let control = Control::default();
        let (temp, no_bin) = (TempCopies::new(None), |_: &Path| false);
        control.speed.store(1000, Ordering::SeqCst);
        let started = std::time::Instant::now();
        std::thread::scope(|scope| {
            let waiter = scope.spawn(|| {
                let cx = RunCx { control: &control, trash: &no_bin, added: Cell::new(0), temp: &temp, job: None };
                // 10 s at 1 KB/s.
                cx.throttle(10_000);
            });
            std::thread::sleep(Duration::from_millis(150));
            control.speed.store(1_000_000_000, Ordering::SeqCst);
            waiter.join().unwrap();
        });
        assert!(started.elapsed() < Duration::from_secs(1), "{:?}", started.elapsed());
    }
}
