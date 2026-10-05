//! What the engine runs: a `Task` lists its items (the plan), then does them one by one.

use std::cell::Cell;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use gezik_core::ops::conflict::{Decision, Facts};

use crate::control::Control;
use crate::pending::{PendingDeletes, copying_name};
use crate::walk::facts_of;

/// Files at least this big are copied under a temporary name and renamed when complete.
pub(crate) const TEMP_COPY_MIN: u64 = 64 * 1024 * 1024;

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
}

impl TaskKind {
    fn verb(self) -> &'static str {
        match self {
            TaskKind::Copy => "Copy",
            TaskKind::Move => "Move",
            TaskKind::Rename => "Rename",
            TaskKind::Trash => "Delete",
            TaskKind::Delete => "Delete permanently",
            TaskKind::Restore => "Restore",
            TaskKind::NewFolder => "New folder",
            TaskKind::NewFile => "New file",
        }
    }

    /// What the Undo item says: "Copy 3 items", "Rename".
    pub fn label(self, count: usize) -> String {
        let verb = self.verb();
        match self {
            TaskKind::Rename | TaskKind::NewFolder | TaskKind::NewFile => verb.to_owned(),
            _ if count == 1 => format!("{verb} 1 item"),
            _ => format!("{verb} {count} items"),
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
    /// Nothing changed (a folder that was already there).
    Nothing,
}

impl Outcome {
    /// Where the result is now, to select it afterwards.
    pub fn result(&self) -> Option<&Path> {
        match self {
            Outcome::Created { path, .. }
            | Outcome::Restored { original: path, .. }
            | Outcome::Moved { to: path, .. } => Some(path),
            _ => None,
        }
    }
}

/// Where a task's plan goes.
pub trait ScanSink {
    /// Adds an item; false once the job is cancelled (stop planning).
    fn item(&mut self, item: PlanItem) -> bool;
    /// Something could not be read; it is reported as failed.
    fn failed(&mut self, path: &Path, error: io::Error);
}

pub trait Task: Send + Sync {
    fn kind(&self) -> TaskKind;
    /// The panel's line while it runs: "Copying 3 items to D:\Yedek".
    fn title(&self) -> String;
    /// How many things the user chose (for "Copy 3 items").
    fn count(&self) -> usize;
    fn resources(&self) -> Resources;
    /// Lists what to do; a folder before what is in it.
    fn plan(&self, sink: &mut dyn ScanSink);
    /// Does `item`. Its target is free: conflicts are settled before.
    fn run(&self, item: &PlanItem, cx: &RunCx<'_>) -> io::Result<Outcome>;
    /// Called once the job ends.
    fn done(&self, _cancelled: bool) {}
}

/// What `Task::run` may use: progress, pause and cancel, the drive's trash.
pub struct RunCx<'a> {
    pub(crate) control: &'a Control,
    pub(crate) trash: &'a dyn Fn(&Path) -> bool,
    /// Bytes this item counted so far (the engine counts the rest when it is done).
    pub(crate) added: Cell<u64>,
    /// Where leftovers to delete at the next start are noted (`None`: nowhere).
    pub(crate) pending: Option<&'a PendingDeletes>,
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

    pub fn has_trash(&self, path: &Path) -> bool {
        (self.trash)(path)
    }

    /// Copies a file, counting its bytes and stopping when the job is cancelled. A large file
    /// is copied under a temporary name and renamed when complete: the system makes the copy
    /// its full size at once, so if Gezik is killed meanwhile the leftover must not look like
    /// a complete file (it is noted and deleted at the next start).
    pub fn copy_file(&self, from: &Path, to: &Path, size: u64) -> io::Result<()> {
        let Some(parent) = to.parent().filter(|_| size >= TEMP_COPY_MIN) else {
            return self.copy_to(from, to, size);
        };
        let temp = parent.join(copying_name());
        let noted = self.pending.filter(|pending| pending.add(&temp).is_ok());
        let result = self.copy_to(from, &temp, size).and_then(|()| gezik_platform::fs::move_entry(&temp, to));
        if result.is_err() {
            let _ = gezik_platform::fs::delete(&temp);
        }
        if let Some(pending) = noted {
            pending.remove(&temp);
        }
        result
    }

    fn copy_to(&self, from: &Path, to: &Path, size: u64) -> io::Result<()> {
        let mut counted = 0u64;
        gezik_platform::fs::copy_file(from, to, size, &mut |done| {
            self.add_bytes(done.saturating_sub(counted));
            counted = counted.max(done);
            !self.stopped()
        })
    }
}

/// The item changed after the operation being undone: it is left alone.
#[derive(Debug)]
pub struct ChangedSince;

impl fmt::Display for ChangedSince {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "changed since; skipped")
    }
}

impl std::error::Error for ChangedSince {}

/// The item's drive has no trash; the user is asked whether to delete it for good.
#[derive(Debug)]
pub struct NoTrash;

impl fmt::Display for NoTrash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if cfg!(windows) { write!(f, "This drive has no Recycle Bin") } else { write!(f, "This drive has no trash") }
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

    fn big_file(path: &Path) -> u64 {
        let data: Vec<u8> = (0..TEMP_COPY_MIN + 1000).map(|i| (i % 251) as u8).collect();
        std::fs::write(path, &data).unwrap();
        data.len() as u64
    }

    #[test]
    fn a_large_copy_lands_under_its_name_only_when_complete() {
        let dir = test_dir("temp-copy");
        std::fs::create_dir(dir.join("to")).unwrap();
        let size = big_file(&dir.join("big.bin"));
        let pending = PendingDeletes::new(dir.join("pending-deletes"));
        let control = Control::default();
        control.pause(PauseReason::User);
        let no_bin = |_: &Path| false;
        let (from, to) = (dir.join("big.bin"), dir.join("to/big.bin"));
        std::thread::scope(|scope| {
            let copy = scope.spawn(|| {
                let cx = RunCx { control: &control, trash: &no_bin, added: Cell::new(0), pending: Some(&pending) };
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
            assert!(during[0].starts_with(crate::pending::COPYING_PREFIX), "{during:?}");
            assert_eq!(pending.load(), [dir.join("to").join(&during[0])], "noted for the next start");
            control.resume();
            copy.join().unwrap().unwrap();
        });
        assert_eq!(names(&dir.join("to")), ["big.bin"]);
        assert_eq!(std::fs::read(&to).unwrap(), std::fs::read(&from).unwrap());
        assert!(pending.load().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_cancelled_large_copy_leaves_nothing() {
        let dir = test_dir("temp-copy-cancel");
        std::fs::create_dir(dir.join("to")).unwrap();
        let size = big_file(&dir.join("big.bin"));
        let pending = PendingDeletes::new(dir.join("pending-deletes"));
        let control = Control::default();
        control.cancel();
        let no_bin = |_: &Path| false;
        let cx = RunCx { control: &control, trash: &no_bin, added: Cell::new(0), pending: Some(&pending) };
        assert!(cx.copy_file(&dir.join("big.bin"), &dir.join("to/big.bin"), size).is_err());
        assert!(names(&dir.join("to")).is_empty());
        assert!(pending.load().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn labels_count_items() {
        assert_eq!(TaskKind::Copy.label(3), "Copy 3 items");
        assert_eq!(TaskKind::Trash.label(1), "Delete 1 item");
        assert_eq!(TaskKind::Rename.label(1), "Rename");
        assert_eq!(TaskKind::NewFolder.label(1), "New folder");
    }

    #[test]
    fn markers_are_recognized() {
        assert!(is_marker::<ChangedSince>(&changed_since()));
        assert!(!is_marker::<NoTrash>(&changed_since()));
        assert!(!is_marker::<NoTrash>(&io::Error::other("x")));
    }
}
