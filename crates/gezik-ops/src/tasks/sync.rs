//! A folder sync (spec 11 §8.2) as one job: folders made, files copied both ways (a file
//! replaced only once its new copy is complete), then items moved to the trash, never before
//! every copy is done and only if none failed. Each step carries what the compare saw and is
//! left alone, with a note, if its files changed since. One job: one undo brings it all back.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use gezik_core::compare::{NO_TIME, seconds};
use gezik_core::ops::conflict::Facts;
use gezik_platform::fs;

use super::{make_dir, name, trash_path};
use crate::engine::lock;
use crate::task::{Outcome, PlanItem, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work, facts_after};

const FOLDER: u8 = 1;
const COPY: u8 = 2;
const TRASH: u8 = 3;

/// An item as the compare saw it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Seen {
    pub is_dir: bool,
    /// 0 for a folder.
    pub size: u64,
    /// Unix seconds (`compare::seconds`); `NO_TIME`: not known.
    pub modified: i64,
}

impl Seen {
    /// Whether `path` is still as seen: the same kind, and for a file the same size and time.
    /// A folder's time is not looked at (a thumbnail cache touches it).
    pub fn still(&self, path: &Path) -> bool {
        let Ok(meta) = std::fs::symlink_metadata(path) else { return false };
        if meta.file_type().is_symlink() || meta.is_dir() != self.is_dir {
            return false;
        }
        self.is_dir
            || meta.len() == self.size && (self.modified == NO_TIME || seconds(meta.modified().ok()) == self.modified)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncStep {
    /// A folder the other side lacks, made before what goes in it.
    Folder { path: PathBuf },
    /// `from` copied to `to`; `replaces`: the file at `to` as seen, which goes to the trash once
    /// the copy is complete (`None`: `to` was not there; one there now meets the conflict list).
    Copy { from: PathBuf, to: PathBuf, seen: Seen, replaces: Option<Seen> },
    /// To the trash, after every copy.
    Trash { path: PathBuf, seen: Seen },
}

pub struct SyncTask {
    left: PathBuf,
    right: PathBuf,
    steps: Vec<SyncStep>,
    /// Replace files on a drive without a trash anyway (the old one is then gone for good).
    for_good: bool,
    /// A copy was left alone because it changed: the trash steps wait for another sync.
    troubled: AtomicBool,
    /// Whether the copies went through, settled when the first trash step runs (they run
    /// after every copy): a trash step failing does not hold back the others.
    held: std::sync::OnceLock<bool>,
}

/// Why a step was left alone.
fn note(text: &'static str) -> io::Error {
    io::Error::other(text)
}

const CHANGED: &str = "Changed after the compare; left alone";
const NO_BIN: &str = "This drive has no trash; left alone";
const HELD: &str = "Not moved to the trash: a copy of this sync did not go through";

impl SyncTask {
    pub fn new(left: PathBuf, right: PathBuf, steps: Vec<SyncStep>, for_good: bool) -> SyncTask {
        SyncTask { left, right, steps, for_good, troubled: AtomicBool::new(false), held: std::sync::OnceLock::new() }
    }

    /// Leaves `path` alone with a note; a copy left alone holds back the trash steps.
    fn leave(&self, cx: &RunCx<'_>, path: &Path, why: &'static str, copy: bool) -> io::Result<Outcome> {
        if copy {
            self.troubled.store(true, Ordering::SeqCst);
        }
        cx.skip(path, &note(why));
        Ok(Outcome::Nothing)
    }

    /// Whether a copy failed or was left alone.
    fn troubled(&self, cx: &RunCx<'_>) -> bool {
        *self.held.get_or_init(|| {
            self.troubled.load(Ordering::SeqCst) || cx.job.is_some_and(|(_, job)| !lock(&job.acc).failures.is_empty())
        })
    }

    /// `from` over `old`: copied next to it under a temporary name, `old` checked again and
    /// trashed (on a drive without a trash, deleted only when allowed), then the copy takes its
    /// name. The old file never goes before its replacement is complete.
    fn replace(&self, cx: &RunCx<'_>, (from, to): (&Path, &Path), seen: Seen, old: Seen) -> io::Result<Outcome> {
        if !old.still(to) {
            return self.leave(cx, to, CHANGED, true);
        }
        let bin = cx.has_trash(to) && fs::can_trash_name(to);
        if !bin && !self.for_good {
            return self.leave(cx, to, NO_BIN, true);
        }
        let temp = cx.temp_file_for(to);
        if let Err(err) = cx.copy_file(from, &temp, seen.size) {
            let _ = fs::delete(&temp);
            self.troubled.store(true, Ordering::SeqCst);
            return Err(err);
        }
        if !old.still(to) || !seen.still(from) {
            let _ = fs::delete(&temp);
            return self.leave(cx, to, CHANGED, true);
        }
        let gone = if bin {
            trash_path(to).map(|trashed| Outcome::Trashed { original: to.to_path_buf(), trashed })
        } else {
            fs::delete(to).map(|()| Outcome::Deleted { path: to.to_path_buf() })
        };
        let gone = match gone {
            Ok(gone) => gone,
            Err(err) => {
                let _ = fs::delete(&temp);
                self.troubled.store(true, Ordering::SeqCst);
                return Err(err);
            }
        };
        if let Err(err) = fs::move_entry(&temp, to) {
            let _ = fs::delete(&temp);
            // The old one is in the trash: the undo brings it back.
            if let Some((_, job)) = cx.job {
                job.outcome(gone);
            }
            self.troubled.store(true, Ordering::SeqCst);
            return Err(err);
        }
        Ok(Outcome::Several(vec![
            gone,
            Outcome::Created { path: to.to_path_buf(), facts: facts_after(to, false), from: None },
        ]))
    }
}

impl Task for SyncTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Sync
    }

    fn title(&self) -> String {
        format!("Synchronizing {} and {}", self.left.display(), self.right.display())
    }

    fn count(&self) -> usize {
        self.steps.len()
    }

    fn resources(&self) -> Resources {
        Resources { paths: vec![self.left.clone(), self.right.clone()], work: Work::Disk }
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        for (root, step) in self.steps.iter().enumerate() {
            let item = match step {
                SyncStep::Folder { path } => PlanItem::new(Stage::Before, Facts { is_dir: true, ..Facts::default() })
                    .target(path)
                    .checked()
                    .tag(FOLDER),
                SyncStep::Copy { from, to, seen, replaces } => {
                    let facts = std::fs::symlink_metadata(from).map(|m| crate::walk::facts_of(&m)).unwrap_or(Facts {
                        is_dir: false,
                        size: seen.size,
                        modified: None,
                    });
                    let item = PlanItem::new(Stage::Parallel, Facts { is_dir: false, ..facts }).source(from).target(to);
                    // A file replaced as seen is this task's to settle; one that came since is the user's.
                    (if replaces.is_some() { item } else { item.checked() }).tag(COPY)
                }
                SyncStep::Trash { path, seen } => {
                    let facts = Facts { is_dir: seen.is_dir, size: seen.size, modified: None };
                    PlanItem::new(Stage::After, facts).source(path).tag(TRASH)
                }
            };
            if !sink.item(item.top(root)) {
                return;
            }
        }
    }

    fn run(&self, item: &PlanItem, cx: &RunCx<'_>) -> io::Result<Outcome> {
        let step = self.steps.get(item.root);
        match (item.tag, step, &item.source, &item.target) {
            (FOLDER, _, _, Some(path)) => Ok(if make_dir(path)? {
                Outcome::Created { path: path.clone(), facts: facts_after(path, true), from: None }
            } else {
                Outcome::Nothing
            }),
            (COPY, Some(SyncStep::Copy { seen, replaces, .. }), Some(from), Some(to)) => {
                if !seen.still(from) {
                    return self.leave(cx, from, CHANGED, true);
                }
                match replaces {
                    // `to` may be a free name the conflict list chose.
                    None => {
                        if let Err(err) = cx.copy_file(from, to, seen.size) {
                            self.troubled.store(true, Ordering::SeqCst);
                            return Err(err);
                        }
                        Ok(Outcome::Created { path: to.clone(), facts: facts_after(to, false), from: None })
                    }
                    Some(old) => self.replace(cx, (from, to), *seen, *old),
                }
            }
            (TRASH, Some(SyncStep::Trash { seen, .. }), Some(path), _) => {
                if self.troubled(cx) {
                    return self.leave(cx, path, HELD, false);
                }
                if !seen.still(path) {
                    return self.leave(cx, path, CHANGED, false);
                }
                if !cx.has_trash(path) || !fs::can_trash_name(path) {
                    return self.leave(cx, path, NO_BIN, false);
                }
                Ok(Outcome::Trashed { original: path.clone(), trashed: trash_path(path)? })
            }
            _ => Ok(Outcome::Nothing),
        }
    }
}

/// "foto" for the label of a sync of `left` (Undo: "Synchronize foto").
pub fn sync_label(left: &Path) -> String {
    format!("Synchronize {}", name(left))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::Control;
    use crate::engine::ConflictItem;
    use crate::testing::{defaults, engine, finish, read, test_dir, write};
    use std::cell::Cell;

    fn seen(path: &Path) -> Seen {
        let meta = std::fs::symlink_metadata(path).unwrap();
        Seen {
            is_dir: meta.is_dir(),
            size: if meta.is_dir() { 0 } else { meta.len() },
            modified: seconds(meta.modified().ok()),
        }
    }

    fn copy(from: PathBuf, to: PathBuf) -> SyncStep {
        let replaces = to.exists().then(|| seen(&to));
        SyncStep::Copy { seen: seen(&from), from, to, replaces }
    }

    fn trash(path: PathBuf) -> SyncStep {
        SyncStep::Trash { seen: seen(&path), path }
    }

    fn no_conflicts(_: &[ConflictItem]) -> Vec<gezik_core::ops::conflict::Decision> {
        panic!("no conflicts expected")
    }

    /// Sets `path`'s time to a fixed one some seconds after the epoch.
    fn age(path: &Path, seconds: u64) {
        let file = std::fs::File::options().write(true).open(path).unwrap();
        file.set_modified(std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000 + seconds))
            .unwrap();
    }

    #[test]
    fn one_job_copies_replaces_and_trashes_and_one_undo_brings_it_all_back() {
        let dir = test_dir("sync-undo");
        let (l, r) = (dir.join("L"), dir.join("R"));
        write(&l.join("new.txt"), "new");
        write(&l.join("both.txt"), "left, newer");
        write(&r.join("both.txt"), "right, older");
        write(&l.join("f/x.txt"), "x");
        write(&r.join("gone.txt"), "only right");
        write(&r.join("back.txt"), "to the left");
        let steps = vec![
            copy(l.join("new.txt"), r.join("new.txt")),
            copy(l.join("both.txt"), r.join("both.txt")),
            SyncStep::Folder { path: r.join("f") },
            copy(l.join("f/x.txt"), r.join("f/x.txt")),
            copy(r.join("back.txt"), l.join("back.txt")),
            trash(r.join("gone.txt")),
        ];
        let engine = engine();
        let task = SyncTask::new(l.clone(), r.clone(), steps, false);
        let job = engine.submit_chain(vec![Box::new(task)], Some(sync_label(&l)));
        let (report, _) = finish(&engine, job, no_conflicts);
        assert!(report.failures.is_empty() && report.skipped.is_empty(), "{:?} {:?}", report.failures, report.skipped);
        assert_eq!(read(&r.join("new.txt")), "new");
        assert_eq!(read(&r.join("both.txt")), "left, newer");
        assert_eq!(read(&r.join("f/x.txt")), "x");
        assert_eq!(read(&l.join("back.txt")), "to the left");
        assert!(!r.join("gone.txt").exists());
        let mut results = report.results.clone();
        results.sort();
        let mut copied =
            vec![r.join("new.txt"), r.join("both.txt"), r.join("f"), r.join("f/x.txt"), l.join("back.txt")];
        copied.sort();
        assert_eq!(results, copied, "every copy and folder is a result; the trashed item is not");
        assert_eq!(engine.undo_label().as_deref(), Some("Synchronize L"));
        assert_eq!(std::fs::read_dir(&r).unwrap().count(), 4, "no temporary file is left");

        let (undo, _) = finish(&engine, engine.undo().unwrap(), defaults);
        assert!(undo.failures.is_empty(), "{:?}", undo.failures);
        assert_eq!(read(&r.join("both.txt")), "right, older", "the replaced file is back");
        assert_eq!(read(&r.join("gone.txt")), "only right", "the trashed file is back");
        assert!(!r.join("new.txt").exists() && !r.join("f").exists() && !l.join("back.txt").exists());
        assert_eq!(engine.undo_label(), None, "one action");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn what_changed_after_the_compare_is_left_alone_and_said() {
        let dir = test_dir("sync-changed");
        let (l, r) = (dir.join("L"), dir.join("R"));
        write(&l.join("src.txt"), "a");
        write(&l.join("over.txt"), "new");
        write(&r.join("over.txt"), "old");
        write(&r.join("bin.txt"), "keep");
        let steps = vec![copy(l.join("src.txt"), r.join("src.txt")), copy(l.join("over.txt"), r.join("over.txt"))];
        let bin = trash(r.join("bin.txt"));
        // After the compare: the source grew, the file to replace was edited, the one to trash too.
        write(&l.join("src.txt"), "a, edited");
        write(&r.join("over.txt"), "old, edited");
        write(&r.join("bin.txt"), "keep, edited");
        let engine = engine();
        let task = SyncTask::new(l.clone(), r.clone(), [steps, vec![bin]].concat(), false);
        let (report, _) = finish(&engine, engine.submit(Box::new(task)), no_conflicts);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        let mut skipped: Vec<(PathBuf, String)> =
            report.skipped.iter().map(|s| (s.path.clone(), s.message.clone())).collect();
        skipped.sort();
        assert_eq!(skipped.len(), 3, "{skipped:?}");
        assert!(skipped.iter().all(|(_, why)| why == CHANGED || why == HELD), "{skipped:?}");
        assert!(!r.join("src.txt").exists());
        assert_eq!(read(&r.join("over.txt")), "old, edited");
        assert_eq!(read(&r.join("bin.txt")), "keep, edited");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_that_came_since_meets_the_conflict_list() {
        let dir = test_dir("sync-came");
        let (l, r) = (dir.join("L"), dir.join("R"));
        write(&l.join("a.txt"), "left");
        std::fs::create_dir_all(&r).unwrap();
        let step = copy(l.join("a.txt"), r.join("a.txt"));
        write(&r.join("a.txt"), "came since");
        let engine = engine();
        let asked = Cell::new(0);
        let task = SyncTask::new(l.clone(), r.clone(), vec![step], false);
        finish(&engine, engine.submit(Box::new(task)), |c| {
            asked.set(c.len());
            defaults(c)
        });
        assert_eq!(asked.get(), 1);
        assert_eq!(read(&r.join("a.txt")), "came since", "Skip is the default");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_copy_holds_back_the_trash() {
        let dir = test_dir("sync-held");
        let (l, r) = (dir.join("L"), dir.join("R"));
        write(&l.join("a.txt"), "a");
        write(&r.join("old.txt"), "old");
        // Its folder is not there and no step makes it: the copy fails.
        let step = copy(l.join("a.txt"), r.join("missing/a.txt"));
        let engine = engine();
        let task = SyncTask::new(l.clone(), r.clone(), vec![step, trash(r.join("old.txt"))], false);
        let (report, _) = finish(&engine, engine.submit(Box::new(task)), no_conflicts);
        assert_eq!(report.failures.len(), 1, "{:?}", report.failures);
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].message, HELD);
        assert_eq!(read(&r.join("old.txt")), "old");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn without_a_trash_nothing_is_trashed_and_a_file_is_replaced_only_when_allowed() {
        let dir = test_dir("sync-no-bin");
        let (l, r) = (dir.join("L"), dir.join("R"));
        write(&l.join("a.txt"), "new");
        write(&r.join("a.txt"), "old");
        write(&r.join("b.txt"), "only right");
        age(&r.join("a.txt"), 0);
        let control = Control::default();
        let no_bin = |_: &Path| false;
        let temp = crate::task::TempCopies::new(None);
        let cx = RunCx { control: &control, trash: &no_bin, added: Cell::new(0), temp: &temp, job: None };
        let steps = vec![copy(l.join("a.txt"), r.join("a.txt")), trash(r.join("b.txt"))];
        let item = |task: &SyncTask, root: usize| {
            let mut planned = Vec::new();
            struct Keep<'a>(&'a mut Vec<PlanItem>);
            impl ScanSink for Keep<'_> {
                fn item(&mut self, item: PlanItem) -> bool {
                    self.0.push(item);
                    true
                }
                fn failed(&mut self, _: &Path, err: io::Error) {
                    panic!("{err}")
                }
            }
            task.plan(&mut Keep(&mut planned));
            planned.swap_remove(root)
        };
        let careful = SyncTask::new(l.clone(), r.clone(), steps.clone(), false);
        assert_eq!(careful.run(&item(&careful, 0), &cx).unwrap(), Outcome::Nothing);
        assert_eq!(read(&r.join("a.txt")), "old");
        let trashing = SyncTask::new(l.clone(), r.clone(), steps.clone(), true);
        assert_eq!(trashing.run(&item(&trashing, 1), &cx).unwrap(), Outcome::Nothing);
        assert_eq!(read(&r.join("b.txt")), "only right", "never deleted for good");
        let allowed = SyncTask::new(l.clone(), r.clone(), steps, true);
        match allowed.run(&item(&allowed, 0), &cx).unwrap() {
            Outcome::Several(outcomes) => assert!(matches!(outcomes[0], Outcome::Deleted { .. })),
            other => panic!("{other:?}"),
        }
        assert_eq!(read(&r.join("a.txt")), "new");
        assert_eq!(std::fs::read_dir(&r).unwrap().count(), 2, "no temporary file is left");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
