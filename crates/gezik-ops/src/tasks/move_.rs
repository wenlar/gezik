//! Moving and renaming: one rename on the same drive; across drives each file is copied, then
//! its original removed.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use gezik_core::ops::conflict::Facts;
use gezik_core::ops::paths::{is_within, same_path};
use gezik_platform::fs;

use super::{name, same_drive, what};
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

pub struct MoveTask {
    /// (where it is, where it goes) per chosen item.
    pairs: Vec<(PathBuf, PathBuf)>,
    /// How each chosen item must still look (undo); `None`: anything.
    expect: Vec<Option<Facts>>,
    kind: TaskKind,
    /// Undo: missing parent folders are made again.
    back: bool,
    /// Tests only: plan as if the target were on another drive.
    cross: bool,
}

impl MoveTask {
    pub fn into(sources: Vec<PathBuf>, dir: &Path) -> MoveTask {
        let pairs: Vec<(PathBuf, PathBuf)> = sources
            .into_iter()
            .map(|source| {
                let target = dir.join(source.file_name().unwrap_or_default());
                (source, target)
            })
            .collect();
        MoveTask { expect: vec![None; pairs.len()], pairs, kind: TaskKind::Move, back: false, cross: false }
    }

    /// Renames `path` to `name` in its folder.
    pub fn rename(path: PathBuf, name: &str) -> MoveTask {
        let target = path.with_file_name(name);
        MoveTask { pairs: vec![(path, target)], expect: vec![None], kind: TaskKind::Rename, back: false, cross: false }
    }

    /// Moves items back where they came from: (where it is, where it was, how it must look).
    #[expect(dead_code, reason = "first used by undo (Task 11)")]
    pub(crate) fn back(items: Vec<(PathBuf, PathBuf, Option<Facts>)>) -> MoveTask {
        let (pairs, expect) = items.into_iter().map(|(now, was, facts)| ((now, was), facts)).unzip();
        MoveTask { pairs, expect, kind: TaskKind::Move, back: true, cross: false }
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
        match (self.kind, self.pairs.first()) {
            (TaskKind::Rename, Some((from, to))) => format!("Renaming {} to {}", name(from), name(to)),
            _ if self.back => format!("Moving {} back", what(&sources)),
            (_, Some((_, to))) => {
                let dir = to.parent().map(|p| p.display().to_string()).unwrap_or_default();
                format!("Moving {} to {dir}", what(&sources))
            }
            (_, None) => "Moving".to_owned(),
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
        for (root, (source, target)) in self.pairs.iter().enumerate() {
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
                Ok(Outcome::Moved {
                    from: source.clone(),
                    to: target.clone(),
                    facts: facts_after(target, item.facts.is_dir),
                })
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
                            format!("{err}; could not restore the name, it is at {}: {restore}", temp.display()),
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
                        format!("Copied, but could not remove the original: {err}"),
                    ));
                }
                Ok(Outcome::Created { path: target.clone(), facts, from: Some(source.clone()) })
            }
            MKDIR => {
                self.make_parent(target)?;
                match std::fs::create_dir(target) {
                    Ok(()) => Ok(Outcome::Created {
                        path: target.clone(),
                        facts: facts_after(target, true),
                        from: Some(source.clone()),
                    }),
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
    fn rename_changes_the_name_and_also_only_its_case() {
        let dir = test_dir("rename");
        write(&dir.join("a.txt"), "a");
        let engine = engine();
        let job = engine.submit(Box::new(MoveTask::rename(dir.join("a.txt"), "b.txt")));
        finish(&engine, job, no_conflicts);
        assert_eq!(read(&dir.join("b.txt")), "a");
        let job = engine.submit(Box::new(MoveTask::rename(dir.join("b.txt"), "B.txt")));
        let (report, _) = finish(&engine, job, no_conflicts);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        let names: Vec<String> =
            std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["B.txt"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn renaming_onto_a_taken_name_asks() {
        let dir = test_dir("rename-taken");
        write(&dir.join("a.txt"), "a");
        write(&dir.join("b.txt"), "b");
        let engine = engine();
        let job = engine.submit(Box::new(MoveTask::rename(dir.join("a.txt"), "b.txt")));
        let (_, events) = finish(&engine, job, defaults);
        assert!(events.iter().any(|e| matches!(e, crate::Event::Conflicts { .. })));
        assert_eq!((read(&dir.join("a.txt")), read(&dir.join("b.txt"))), ("a".into(), "b".into()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
