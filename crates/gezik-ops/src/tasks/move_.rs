//! Moving and renaming: one rename on the same drive; across drives each file is copied, then
//! its original removed.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use gezik_core::ops::conflict::Facts;
use gezik_core::ops::paths::{is_within, same_path};
use gezik_platform::fs;

use super::{same_drive, what};
use crate::task::{
    Outcome, PlanItem, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work, changed_since, facts_after, unchanged,
};
use crate::walk::{Step, facts_of, walk};

const RENAME: u8 = 0;
/// Only the case of the name changes: through a temporary name.
const CASE: u8 = 1;
const COPY_DELETE: u8 = 2;
const MKDIR: u8 = 3;
const RMDIR: u8 = 4;
/// A folder on the way to a target, made before what goes into it (move with folders).
const MAKE_PARENT: u8 = 5;

pub struct MoveTask {
    /// (where it is, where it goes) per chosen item.
    pairs: Vec<(PathBuf, PathBuf)>,
    /// How each chosen item must still look (undo); `None`: anything.
    expect: Vec<Option<Facts>>,
    /// Undo: missing parent folders are made again.
    back: bool,
    /// Places new things (from a staging folder): what lands counts as made, so undo trashes it.
    placing: bool,
    /// `Move`, or the kind of the job that places.
    kind: TaskKind,
    /// Tests only: plan as if the target were on another drive.
    cross: bool,
    /// Folders to make before the items (move with folders, spec 4.6), shallowest first.
    parents: Vec<PathBuf>,
    /// Move with folders: items whose relative path would leave the target folder.
    refused: Vec<PathBuf>,
}

impl MoveTask {
    fn with(pairs: Vec<(PathBuf, PathBuf)>, expect: Vec<Option<Facts>>, back: bool) -> MoveTask {
        MoveTask {
            pairs,
            expect,
            back,
            placing: false,
            kind: TaskKind::Move,
            cross: false,
            parents: Vec::new(),
            refused: Vec::new(),
        }
    }

    /// Moves each source to `dir` joined with its path under a search's scope (spec 4.6), making
    /// the folders on the way; undo moves them back first, then trashes the folders it made
    /// (only if they then hold no files: `inverse::build`). An item that would land on itself
    /// (moved back into the search's folder) stays where it is.
    pub fn with_folders(items: Vec<(PathBuf, PathBuf)>, dir: &Path) -> MoveTask {
        let super::Relative { mut pairs, parents, refused } = super::relative_targets(items, dir);
        pairs.retain(|(source, target)| !same_path(source, target));
        let expect = vec![None; pairs.len()];
        MoveTask { parents, refused, ..MoveTask::with(pairs, expect, false) }
    }

    pub fn into(sources: Vec<PathBuf>, dir: &Path) -> MoveTask {
        let pairs: Vec<(PathBuf, PathBuf)> = sources
            .into_iter()
            .map(|source| {
                let target = dir.join(source.file_name().unwrap_or_default());
                (source, target)
            })
            .collect();
        let expect = vec![None; pairs.len()];
        MoveTask::with(pairs, expect, false)
    }

    /// Moves items back where they came from: (where it is, where it was, how it must look).
    pub(crate) fn back(items: Vec<(PathBuf, PathBuf, Option<Facts>)>) -> MoveTask {
        let (pairs, expect) = items.into_iter().map(|(now, was, facts)| ((now, was), facts)).unzip();
        MoveTask::with(pairs, expect, true)
    }

    /// Moves what a job made in a staging folder to where it goes (a folder that is there
    /// already is merged); undo trashes what was placed. `kind`: the job's own kind.
    pub fn placing(pairs: Vec<(PathBuf, PathBuf)>, kind: TaskKind) -> MoveTask {
        // Rebuilt from their parts: a path joined with `/` (an archive entry's name) has the
        // system's separators then, which the Windows trash needs for undo.
        let tidy = |path: PathBuf| path.components().collect::<PathBuf>();
        let pairs: Vec<(PathBuf, PathBuf)> = pairs.into_iter().map(|(from, to)| (tidy(from), tidy(to))).collect();
        let expect = vec![None; pairs.len()];
        let mut task = MoveTask::with(pairs, expect, false);
        task.placing = true;
        task.kind = kind;
        task
    }

    /// What doing `source` → `target` made: new (placing), or moved from `source`.
    fn made(&self, source: &Path, target: &Path, facts: Facts) -> Outcome {
        let from = (!self.placing).then(|| source.to_path_buf());
        Outcome::Created { path: target.to_path_buf(), facts, from }
    }

    fn sources(&self) -> Vec<PathBuf> {
        self.pairs.iter().map(|(source, _)| source.clone()).collect()
    }

    fn check(&self, item: &PlanItem, source: &Path) -> io::Result<()> {
        if item.is_root && !unchanged(source, self.expect.get(item.root).copied().flatten()) {
            return Err(changed_since());
        }
        Ok(())
    }

    fn make_parent(&self, target: &Path) -> io::Result<()> {
        if self.back
            && let Some(parent) = target.parent()
        {
            std::fs::create_dir_all(parent)?;
        }
        Ok(())
    }
}

impl Task for MoveTask {
    fn kind(&self) -> TaskKind {
        self.kind
    }

    fn title(&self) -> String {
        let sources = self.sources();
        match self.pairs.first() {
            _ if self.back => format!("Moving {} back", what(&sources)),
            Some((_, to)) => {
                let dir = to.parent().map(|p| p.display().to_string()).unwrap_or_default();
                format!("Moving {} to {dir}", what(&sources))
            }
            None => "Moving".to_owned(),
        }
    }

    fn count(&self) -> usize {
        self.pairs.len()
    }

    fn resources(&self) -> Resources {
        let mut paths = self.sources();
        paths.extend(self.pairs.iter().filter_map(|(_, target)| target.parent().map(Path::to_path_buf)));
        Resources { paths, work: Work::Disk }
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        super::refuse_outside(sink, &self.refused);
        // The folders on the way first: Before items run as they are planned.
        for parent in &self.parents {
            if std::fs::symlink_metadata(parent).is_err() {
                let item = PlanItem::new(Stage::Before, Facts { is_dir: true, ..Facts::default() })
                    .target(parent)
                    .uncounted()
                    .tag(MAKE_PARENT);
                if !sink.item(item) {
                    return;
                }
            }
        }
        for (root, (source, target)) in self.pairs.iter().enumerate() {
            if super::refuse_root(sink, source, "move") {
                continue;
            }
            if source == target {
                continue;
            }
            let meta = match std::fs::symlink_metadata(source) {
                Ok(meta) => meta,
                Err(err) => {
                    sink.failed(source, err);
                    continue;
                }
            };
            let facts = facts_of(&meta);
            let planned = if same_path(source, target) {
                sink.item(PlanItem::new(Stage::Parallel, facts).source(source).target(target).top(root).tag(CASE))
            } else if facts.is_dir && is_within(target, source) {
                sink.failed(source, io::Error::new(io::ErrorKind::InvalidInput, "Cannot move a folder into itself"));
                true
            } else if !self.cross && same_drive(source, target.parent().unwrap_or(target)) {
                plan_rename(sink, source, target, facts, root, true)
            } else {
                plan_cross(sink, source, target, facts, root)
            };
            if !planned {
                return;
            }
        }
    }

    fn run(&self, item: &PlanItem, cx: &RunCx<'_>) -> io::Result<Outcome> {
        if item.tag == MAKE_PARENT {
            let Some(target) = &item.target else { return Ok(Outcome::Nothing) };
            return match std::fs::create_dir(target) {
                Ok(()) => Ok(Outcome::MadeParent { path: target.clone() }),
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists && target.is_dir() => Ok(Outcome::Nothing),
                Err(err) => Err(err),
            };
        }
        let Some(source) = &item.source else { return Ok(Outcome::Nothing) };
        if item.tag == RMDIR {
            // Nothing was made at the target (held, skipped or failed): the folder stays.
            if !item.target.as_ref().is_some_and(|target| target.is_dir()) {
                return Ok(Outcome::Nothing);
            }
            return match fs::delete(source) {
                Ok(()) => Ok(Outcome::Deleted { path: source.clone() }),
                // Something inside was skipped or failed: the folder stays.
                Err(err) if matches!(err.kind(), io::ErrorKind::DirectoryNotEmpty | io::ErrorKind::NotFound) => {
                    Ok(Outcome::Nothing)
                }
                Err(err) => Err(err),
            };
        }
        let Some(target) = &item.target else { return Ok(Outcome::Nothing) };
        match item.tag {
            RENAME => {
                self.check(item, source)?;
                self.make_parent(target)?;
                fs::move_entry(source, target)?;
                let facts = facts_after(target, item.facts.is_dir);
                if self.placing && item.facts.is_dir {
                    return Ok(Outcome::Placed { path: target.clone() });
                }
                if self.placing {
                    return Ok(self.made(source, target, facts));
                }
                Ok(Outcome::Moved { from: source.clone(), to: target.clone(), facts })
            }
            CASE => {
                self.check(item, source)?;
                static NEXT: AtomicUsize = AtomicUsize::new(0);
                let temp = source.with_file_name(format!(
                    ".gezik-rename-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::move_entry(source, &temp)?;
                if let Err(err) = fs::move_entry(&temp, target) {
                    if let Err(restore) = fs::move_entry(&temp, source) {
                        return Err(io::Error::new(
                            err.kind(),
                            format!(
                                "{}; could not restore the name, it is at {}: {}",
                                fs::describe(&err),
                                temp.display(),
                                fs::describe(&restore)
                            ),
                        ));
                    }
                    return Err(err);
                }
                Ok(Outcome::Moved {
                    from: source.clone(),
                    to: target.clone(),
                    facts: facts_after(target, item.facts.is_dir),
                })
            }
            COPY_DELETE => {
                self.check(item, source)?;
                self.make_parent(target)?;
                cx.copy_file(source, target, item.facts.size)?;
                let facts = facts_after(target, false);
                if let Err(err) = fs::delete(source) {
                    return Err(io::Error::new(
                        err.kind(),
                        format!("Copied, but could not remove the original: {}", fs::describe(&err)),
                    ));
                }
                Ok(self.made(source, target, facts))
            }
            MKDIR => {
                self.make_parent(target)?;
                match std::fs::create_dir(target) {
                    Ok(()) => Ok(self.made(source, target, facts_after(target, true))),
                    Err(err) if err.kind() == io::ErrorKind::AlreadyExists && target.is_dir() => Ok(Outcome::Nothing),
                    Err(err) => Err(err),
                }
            }
            _ => Ok(Outcome::Nothing),
        }
    }
}

/// Same drive: one rename, or, onto a folder that is already there, its contents one by one
/// and then the emptied source folder removed. False once the job is cancelled.
fn plan_rename(sink: &mut dyn ScanSink, source: &Path, target: &Path, facts: Facts, root: usize, top: bool) -> bool {
    if facts.is_dir && std::fs::symlink_metadata(target).is_ok_and(|meta| meta.is_dir()) {
        // Before what is inside: the After items run in reverse, so the folder goes last.
        if !sink.item(PlanItem::new(Stage::After, facts).source(source).target(target).under(root).tag(RMDIR)) {
            return false;
        }
        let entries = match std::fs::read_dir(source) {
            Ok(entries) => entries,
            Err(err) => {
                sink.failed(source, err);
                return true;
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    sink.failed(source, err);
                    continue;
                }
            };
            let child = entry.path();
            let meta = match entry.metadata() {
                Ok(meta) => meta,
                Err(err) => {
                    sink.failed(&child, err);
                    continue;
                }
            };
            if !plan_rename(sink, &child, &target.join(entry.file_name()), facts_of(&meta), root, false) {
                return false;
            }
        }
        return true;
    }
    let item = PlanItem::new(Stage::Parallel, facts).source(source).target(target).checked().tag(RENAME);
    sink.item(if top { item.top(root) } else { item.under(root) })
}

/// Another drive: folders made first, files copied then removed, emptied folders removed last.
fn plan_cross(sink: &mut dyn ScanSink, source: &Path, target: &Path, facts: Facts, root: usize) -> bool {
    if !facts.is_dir {
        return sink.item(
            PlanItem::new(Stage::Parallel, facts).source(source).target(target).checked().top(root).tag(COPY_DELETE),
        );
    }
    if !sink.item(PlanItem::new(Stage::Before, facts).source(source).target(target).checked().top(root).tag(MKDIR))
        || !sink.item(PlanItem::new(Stage::After, facts).source(source).target(target).under(root).tag(RMDIR))
    {
        return false;
    }
    walk(source, &mut |step| match step {
        Step::Entry { path, relative, facts } => {
            let target = target.join(relative);
            if facts.is_dir {
                sink.item(
                    PlanItem::new(Stage::Before, facts).source(path).target(&target).checked().under(root).tag(MKDIR),
                ) && sink.item(PlanItem::new(Stage::After, facts).source(path).target(&target).under(root).tag(RMDIR))
            } else {
                sink.item(
                    PlanItem::new(Stage::Parallel, facts)
                        .source(path)
                        .target(target)
                        .checked()
                        .under(root)
                        .tag(COPY_DELETE),
                )
            }
        }
        Step::Failed { path, error } => {
            sink.failed(path, error);
            true
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ConflictItem;
    use crate::testing::{defaults, engine, finish, read, test_dir, write};
    use gezik_core::ops::conflict::Decision;

    fn no_conflicts(_: &[ConflictItem]) -> Vec<Decision> {
        panic!("no conflicts expected")
    }

    #[test]
    fn a_move_on_the_same_drive_renames() {
        let dir = test_dir("move-rename");
        write(&dir.join("src/a.txt"), "a");
        std::fs::create_dir(dir.join("dst")).unwrap();
        let engine = engine();
        let job = engine.submit(Box::new(MoveTask::into(vec![dir.join("src/a.txt")], &dir.join("dst"))));
        let (report, _) = finish(&engine, job, no_conflicts);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(read(&dir.join("dst/a.txt")), "a");
        assert!(!dir.join("src/a.txt").exists());
        assert_eq!(report.results, [dir.join("dst/a.txt")]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn move_onto_itself_does_nothing() {
        let dir = test_dir("move-itself");
        write(&dir.join("a.txt"), "a");
        let engine = engine();
        let job = engine.submit(Box::new(MoveTask::into(vec![dir.join("a.txt")], &dir)));
        let (report, _) = finish(&engine, job, no_conflicts);
        assert!(report.failures.is_empty());
        assert_eq!(read(&dir.join("a.txt")), "a");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn moving_a_folder_into_itself_fails() {
        let dir = test_dir("move-into-itself");
        write(&dir.join("f/sub/x.txt"), "x");
        let engine = engine();
        let job = engine.submit(Box::new(MoveTask::into(vec![dir.join("f")], &dir.join("f/sub"))));
        let (report, _) = finish(&engine, job, no_conflicts);
        assert_eq!(report.failures.len(), 1);
        assert_eq!(read(&dir.join("f/sub/x.txt")), "x");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn moving_into_an_existing_folder_merges() {
        let dir = test_dir("move-merge");
        write(&dir.join("src/d/a.txt"), "a");
        write(&dir.join("dst/d/b.txt"), "b");
        let engine = engine();
        let job = engine.submit(Box::new(MoveTask::into(vec![dir.join("src/d")], &dir.join("dst"))));
        let (report, _) = finish(&engine, job, no_conflicts);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(read(&dir.join("dst/d/a.txt")), "a");
        assert_eq!(read(&dir.join("dst/d/b.txt")), "b");
        assert!(!dir.join("src/d").exists(), "the emptied source folder is removed");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_nested_merge_removes_every_emptied_folder() {
        let dir = test_dir("move-merge-nested");
        write(&dir.join("src/d/s/x.txt"), "x");
        write(&dir.join("dst/d/s/y.txt"), "y");
        let engine = engine();
        let job = engine.submit(Box::new(MoveTask::into(vec![dir.join("src/d")], &dir.join("dst"))));
        let (report, _) = finish(&engine, job, no_conflicts);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(read(&dir.join("dst/d/s/x.txt")), "x");
        assert_eq!(read(&dir.join("dst/d/s/y.txt")), "y");
        assert!(!dir.join("src/d").exists(), "no emptied source folder is left");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_skipped_cross_drive_folder_is_not_removed() {
        let dir = test_dir("move-cross-skip");
        write(&dir.join("src/x/f.txt"), "f");
        std::fs::create_dir_all(dir.join("src/x/e")).unwrap();
        write(&dir.join("dst/x"), "a file");
        let engine = engine();
        let mut task = MoveTask::into(vec![dir.join("src/x")], &dir.join("dst"));
        task.cross = true;
        let job = engine.submit(Box::new(task));
        let (report, _) = finish(&engine, job, defaults);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(read(&dir.join("dst/x")), "a file");
        assert!(dir.join("src/x/e").is_dir());
        assert_eq!(read(&dir.join("src/x/f.txt")), "f");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_cross_drive_folder_moves_and_is_removed() {
        let dir = test_dir("move-cross");
        write(&dir.join("src/x/a/f.txt"), "f");
        std::fs::create_dir_all(dir.join("src/x/e")).unwrap();
        std::fs::create_dir(dir.join("dst")).unwrap();
        let engine = engine();
        let mut task = MoveTask::into(vec![dir.join("src/x")], &dir.join("dst"));
        task.cross = true;
        let job = engine.submit(Box::new(task));
        let (report, _) = finish(&engine, job, no_conflicts);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(read(&dir.join("dst/x/a/f.txt")), "f");
        assert!(dir.join("dst/x/e").is_dir());
        assert!(!dir.join("src/x").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn placing_reports_created_so_undo_trashes() {
        let dir = test_dir("move-placing");
        write(&dir.join("stage/x/a.txt"), "a");
        write(&dir.join("dst/x/b.txt"), "b");
        let engine = engine();
        let task = MoveTask::placing(vec![(dir.join("stage/x"), dir.join("dst/x"))], TaskKind::Extract);
        let job = engine.submit(Box::new(task));
        let (report, _) = finish(&engine, job, no_conflicts);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(read(&dir.join("dst/x/a.txt")), "a");
        finish(&engine, engine.undo().unwrap(), no_conflicts);
        assert!(!dir.join("dst/x/a.txt").exists(), "the placed file went to the trash");
        assert_eq!(read(&dir.join("dst/x/b.txt")), "b", "what was there stays");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_folder_placed_whole_is_undone_whole() {
        let dir = test_dir("move-placing-whole");
        write(&dir.join("stage/x/a.txt"), "a");
        std::fs::create_dir(dir.join("dst")).unwrap();
        let engine = engine();
        let task = MoveTask::placing(vec![(dir.join("stage/x"), dir.join("dst/x"))], TaskKind::Extract);
        finish(&engine, engine.submit(Box::new(task)), no_conflicts);
        assert_eq!(read(&dir.join("dst/x/a.txt")), "a");
        let (report, _) = finish(&engine, engine.undo().unwrap(), no_conflicts);
        assert!(report.failures.is_empty() && report.skipped_changed == 0, "{report:?}");
        assert!(!dir.join("dst/x").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_skipped_file_keeps_its_folder() {
        let dir = test_dir("move-merge-skip");
        write(&dir.join("src/d/a.txt"), "new");
        write(&dir.join("dst/d/a.txt"), "old");
        let engine = engine();
        let job = engine.submit(Box::new(MoveTask::into(vec![dir.join("src/d")], &dir.join("dst"))));
        let (report, _) = finish(&engine, job, defaults);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(read(&dir.join("dst/d/a.txt")), "old");
        assert_eq!(read(&dir.join("src/d/a.txt")), "new", "skipped: still where it was");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn moving_with_folders_comes_back_whole_on_undo() {
        let dir = test_dir("move-with-folders");
        write(&dir.join("src/a/b/x.txt"), "x");
        let engine = engine();
        let items = vec![(dir.join("src/a/b/x.txt"), PathBuf::from("a").join("b").join("x.txt"))];
        let (report, _) =
            finish(&engine, engine.submit(Box::new(MoveTask::with_folders(items, &dir.join("dst")))), defaults);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(read(&dir.join("dst/a/b/x.txt")), "x");
        assert!(!dir.join("src/a/b/x.txt").exists());
        let (undo, _) = finish(&engine, engine.undo().unwrap(), defaults);
        assert!(undo.failures.is_empty(), "{:?}", undo.failures);
        assert_eq!(read(&dir.join("src/a/b/x.txt")), "x");
        assert!(!dir.join("dst").exists(), "the folders made for it are gone, dst too (it was not there)");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn moving_with_folders_onto_itself_does_nothing() {
        let dir = test_dir("move-with-folders-onto-itself");
        write(&dir.join("a/x.txt"), "x");
        let engine = engine();
        let items = vec![(dir.join("a/x.txt"), PathBuf::from("a").join("x.txt"))];
        let task = MoveTask::with_folders(items, &dir);
        assert_eq!(task.count(), 0);
        let (report, _) = finish(&engine, engine.submit(Box::new(task)), no_conflicts);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(read(&dir.join("a/x.txt")), "x");
        assert_eq!(std::fs::read_dir(dir.join("a")).unwrap().count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn undoing_a_move_with_folders_keeps_a_made_folder_that_holds_a_file_since() {
        let dir = test_dir("move-with-folders-failed");
        write(&dir.join("src/a/x.txt"), "x");
        let dst = dir.join("dst");
        std::fs::create_dir(&dst).unwrap();
        // `gone.txt` vanished after it was found: its folder is made, nothing goes into it.
        let items = vec![
            (dir.join("src/a/x.txt"), PathBuf::from("a").join("x.txt")),
            (dir.join("src/c/gone.txt"), PathBuf::from("c").join("gone.txt")),
        ];
        let engine = engine();
        let (report, _) = finish(&engine, engine.submit(Box::new(MoveTask::with_folders(items, &dst))), defaults);
        assert_eq!(report.failures.len(), 1, "{:?}", report.failures);
        assert!(dst.join("c").is_dir());
        write(&dst.join("c/user.txt"), "mine");
        let (undo, _) = finish(&engine, engine.undo().unwrap(), defaults);
        assert!(undo.failures.is_empty(), "{:?}", undo.failures);
        assert_eq!(read(&dir.join("src/a/x.txt")), "x", "the moved file came back");
        assert!(!dst.join("a").exists(), "the made folder left empty went");
        assert_eq!(read(&dst.join("c/user.txt")), "mine", "a file put there since stays");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
