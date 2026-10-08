//! Copying files and folders, also next to themselves (Duplicate).

use std::io;
use std::path::{Path, PathBuf};

use gezik_core::ops::conflict::{Decision, Facts};
use gezik_core::ops::paths::{is_within, same_path};

use super::what;
use crate::task::{Outcome, PlanItem, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work, facts_after};
use crate::walk::{Step, facts_of, walk};

pub struct CopyTask {
    /// (source, target) per chosen item.
    pairs: Vec<(PathBuf, PathBuf)>,
    /// Keep both without asking: a copy into the source's own folder.
    presets: Vec<Option<Decision>>,
    /// Where they go, for the title; `None` for Duplicate.
    dir: Option<PathBuf>,
    /// A template copied in (spec 8.1): its kind (New file, New folder); `None`: a copy.
    new: Option<TaskKind>,
}

impl CopyTask {
    /// Copies `sources` into `dir`. A source already in `dir` becomes `name (2)`.
    pub fn into(sources: Vec<PathBuf>, dir: &Path) -> CopyTask {
        let presets = sources
            .iter()
            .map(|source| source.parent().is_some_and(|parent| same_path(parent, dir)).then_some(Decision::KeepBoth))
            .collect();
        let pairs = sources
            .into_iter()
            .map(|source| {
                let target = dir.join(source.file_name().unwrap_or_default());
                (source, target)
            })
            .collect();
        CopyTask { pairs, presets, dir: Some(dir.to_path_buf()), new: None }
    }

    /// Copies each source next to itself as `name (2)`.
    pub fn duplicate(sources: Vec<PathBuf>) -> CopyTask {
        let presets = vec![Some(Decision::KeepBoth); sources.len()];
        CopyTask {
            pairs: sources.into_iter().map(|source| (source.clone(), source)).collect(),
            presets,
            dir: None,
            new: None,
        }
    }

    /// The user's template `source` copied into `dir` under its own name (a taken name gets a
    /// number), undone as a new file or folder (spec 8.1).
    pub fn template(source: PathBuf, dir: &Path, is_dir: bool) -> CopyTask {
        let target = dir.join(source.file_name().unwrap_or_default());
        let kind = if is_dir { TaskKind::NewFolder } else { TaskKind::NewFile };
        CopyTask {
            pairs: vec![(source, target)],
            presets: vec![Some(Decision::KeepBoth)],
            dir: Some(dir.to_path_buf()),
            new: Some(kind),
        }
    }

    fn sources(&self) -> Vec<PathBuf> {
        self.pairs.iter().map(|(source, _)| source.clone()).collect()
    }
}

/// A folder copied into itself would copy forever.
fn into_itself(source: &Path, target: &Path, facts: Facts) -> bool {
    facts.is_dir && is_within(target, source) && !same_path(target, source)
}

impl Task for CopyTask {
    fn kind(&self) -> TaskKind {
        self.new.unwrap_or(TaskKind::Copy)
    }

    fn title(&self) -> String {
        if self.new.is_some()
            && let Some((_, target)) = self.pairs.first()
        {
            return format!("Creating {}", super::name(target));
        }
        match &self.dir {
            Some(dir) => format!("Copying {} to {}", what(&self.sources()), dir.display()),
            None => format!("Duplicating {}", what(&self.sources())),
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
        for (root, ((source, target), preset)) in self.pairs.iter().zip(&self.presets).enumerate() {
            if super::refuse_root(sink, source, "copy") {
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
            if into_itself(source, target, facts) {
                sink.failed(source, io::Error::new(io::ErrorKind::InvalidInput, "Cannot copy a folder into itself"));
                continue;
            }
            let stage = if facts.is_dir { Stage::Before } else { Stage::Parallel };
            let item = PlanItem::new(stage, facts).source(source).target(target).checked().top(root).preset(*preset);
            if !sink.item(item) {
                return;
            }
            if facts.is_dir && !plan_tree(sink, source, target, root) {
                return;
            }
        }
    }

    fn run(&self, item: &PlanItem, cx: &RunCx<'_>) -> io::Result<Outcome> {
        let (Some(source), Some(target)) = (&item.source, &item.target) else { return Ok(Outcome::Nothing) };
        if item.facts.is_dir {
            return match std::fs::create_dir(target) {
                Ok(()) => Ok(Outcome::Created { path: target.clone(), facts: facts_after(target, true), from: None }),
                // Merged into a folder that was already there.
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists && target.is_dir() => Ok(Outcome::Nothing),
                Err(err) => Err(err),
            };
        }
        cx.copy_file(source, target, item.facts.size)?;
        Ok(Outcome::Created { path: target.clone(), facts: facts_after(target, false), from: None })
    }
}

/// Plans copying what is inside `source` into `target`; false once the job is cancelled.
fn plan_tree(sink: &mut dyn ScanSink, source: &Path, target: &Path, root: usize) -> bool {
    walk(source, &mut |step| match step {
        Step::Entry { path, relative, facts } => {
            let stage = if facts.is_dir { Stage::Before } else { Stage::Parallel };
            sink.item(PlanItem::new(stage, facts).source(path).target(target.join(relative)).checked().under(root))
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
    use gezik_core::ops::conflict::ConflictKind;

    fn no_conflicts(_: &[ConflictItem]) -> Vec<Decision> {
        panic!("no conflicts expected")
    }

    #[test]
    fn copies_a_tree() {
        let dir = test_dir("copy-tree");
        let (src, dst) = (dir.join("src"), dir.join("dst"));
        write(&src.join("a/b/c.txt"), "c");
        write(&src.join("a/d.txt"), "dd");
        std::fs::create_dir_all(src.join("a/empty")).unwrap();
        std::fs::create_dir(&dst).unwrap();
        let engine = engine();
        let job = engine.submit(Box::new(CopyTask::into(vec![src.join("a")], &dst)));
        let (report, _) = finish(&engine, job, no_conflicts);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(read(&dst.join("a/b/c.txt")), "c");
        assert_eq!(read(&dst.join("a/d.txt")), "dd");
        assert!(dst.join("a/empty").is_dir());
        assert_eq!(read(&src.join("a/d.txt")), "dd", "the source stays");
        assert_eq!(report.results, [dst.join("a")]);
        assert!(report.changed_dirs.contains(&dst));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn copy_into_itself_fails() {
        let dir = test_dir("copy-itself");
        write(&dir.join("a/x.txt"), "x");
        let engine = engine();
        let job = engine.submit(Box::new(CopyTask::into(vec![dir.join("a")], &dir.join("a"))));
        let (report, _) = finish(&engine, job, no_conflicts);
        assert_eq!(report.failures.len(), 1);
        assert!(report.failures[0].message.contains("into itself"), "{}", report.failures[0].message);
        assert!(!dir.join("a/a").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn paste_into_the_same_folder_numbers_the_copy() {
        let dir = test_dir("copy-same");
        write(&dir.join("x.txt"), "x");
        write(&dir.join("x (2).txt"), "taken");
        let engine = engine();
        let job = engine.submit(Box::new(CopyTask::into(vec![dir.join("x.txt")], &dir)));
        let (report, _) = finish(&engine, job, no_conflicts);
        assert_eq!(read(&dir.join("x (3).txt")), "x");
        assert_eq!(report.results, [dir.join("x (3).txt")]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn duplicate_numbers_a_folder_and_keeps_its_contents_inside() {
        let dir = test_dir("duplicate");
        write(&dir.join("f/a.txt"), "a");
        write(&dir.join("f/sub/b.txt"), "b");
        let engine = engine();
        let job = engine.submit(Box::new(CopyTask::duplicate(vec![dir.join("f")])));
        let (report, _) = finish(&engine, job, no_conflicts);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(read(&dir.join("f (2)/a.txt")), "a");
        assert_eq!(read(&dir.join("f (2)/sub/b.txt")), "b");
        assert!(!dir.join("f/f").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn conflicts_are_asked_once_and_skip_is_the_default() {
        let dir = test_dir("copy-conflict");
        let (src, dst) = (dir.join("src"), dir.join("dst"));
        write(&src.join("a.txt"), "new");
        write(&src.join("b.txt"), "b");
        write(&dst.join("a.txt"), "old");
        let engine = engine();
        let job = engine.submit(Box::new(CopyTask::into(vec![src.join("a.txt"), src.join("b.txt")], &dst)));
        let (_, events) = finish(&engine, job, |conflicts| {
            assert_eq!(conflicts.len(), 1);
            assert_eq!((conflicts[0].kind, conflicts[0].decision), (ConflictKind::File, Decision::Skip));
            assert_eq!(conflicts[0].target, dst.join("a.txt"));
            defaults(conflicts)
        });
        assert_eq!(events.iter().filter(|e| matches!(e, crate::Event::Conflicts { .. })).count(), 1);
        assert_eq!(read(&dst.join("a.txt")), "old");
        assert_eq!(read(&dst.join("b.txt")), "b");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn keep_both_names_the_new_copy() {
        let dir = test_dir("copy-keep-both");
        let (src, dst) = (dir.join("src"), dir.join("dst"));
        write(&src.join("a.txt"), "new");
        write(&dst.join("a.txt"), "old");
        let engine = engine();
        let job = engine.submit(Box::new(CopyTask::into(vec![src.join("a.txt")], &dst)));
        finish(&engine, job, |c| vec![Decision::KeepBoth; c.len()]);
        assert_eq!(read(&dst.join("a.txt")), "old");
        assert_eq!(read(&dst.join("a (2).txt")), "new");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn replace_writes_the_new_file() {
        let dir = test_dir("copy-replace");
        let (src, dst) = (dir.join("src"), dir.join("dst"));
        write(&src.join("a.txt"), "new");
        write(&dst.join("a.txt"), "old");
        let engine = engine();
        let job = engine.submit(Box::new(CopyTask::into(vec![src.join("a.txt")], &dst)));
        let (report, _) = finish(&engine, job, |c| vec![Decision::Replace; c.len()]);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(read(&dst.join("a.txt")), "new");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_folder_meeting_a_file_waits_with_its_contents() {
        let dir = test_dir("copy-mismatch");
        let (src, dst) = (dir.join("src"), dir.join("dst"));
        write(&src.join("x/1.txt"), "1");
        write(&src.join("x/2.txt"), "2");
        write(&dst.join("x"), "a file");
        let engine = engine();
        let job = engine.submit(Box::new(CopyTask::into(vec![src.join("x")], &dst)));
        let (report, _) = finish(&engine, job, |conflicts| {
            assert_eq!(conflicts.len(), 1, "the folder only, not what is inside it");
            assert_eq!(conflicts[0].kind, ConflictKind::Mismatch);
            vec![Decision::KeepBoth]
        });
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(read(&dst.join("x")), "a file");
        assert_eq!(read(&dst.join("x (2)/1.txt")), "1");
        assert_eq!(read(&dst.join("x (2)/2.txt")), "2");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn skipping_a_held_folder_skips_its_contents() {
        let dir = test_dir("copy-mismatch-skip");
        let (src, dst) = (dir.join("src"), dir.join("dst"));
        write(&src.join("x/1.txt"), "1");
        write(&dst.join("x"), "a file");
        let engine = engine();
        let job = engine.submit(Box::new(CopyTask::into(vec![src.join("x")], &dst)));
        let (report, _) = finish(&engine, job, defaults);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(read(&dst.join("x")), "a file");
        assert!(!dst.join("x (2)").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn folders_merge_without_asking() {
        let dir = test_dir("copy-merge");
        let (src, dst) = (dir.join("src"), dir.join("dst"));
        write(&src.join("d/new.txt"), "new");
        write(&dst.join("d/old.txt"), "old");
        let engine = engine();
        let job = engine.submit(Box::new(CopyTask::into(vec![src.join("d")], &dst)));
        finish(&engine, job, no_conflicts);
        assert_eq!(read(&dst.join("d/new.txt")), "new");
        assert_eq!(read(&dst.join("d/old.txt")), "old");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_template_is_copied_under_its_own_name_and_numbered() {
        let dir = test_dir("copy-template");
        write(&dir.join("templates/Report.docx"), "r");
        write(&dir.join("templates/Project/src/main.rs"), "m");
        std::fs::create_dir(dir.join("here")).unwrap();
        let engine = engine();
        for _ in 0..2 {
            let task = CopyTask::template(dir.join("templates/Report.docx"), &dir.join("here"), false);
            assert_eq!(task.title(), "Creating Report.docx");
            finish(&engine, engine.submit(Box::new(task)), no_conflicts);
        }
        assert_eq!(read(&dir.join("here/Report.docx")), "r");
        assert_eq!(read(&dir.join("here/Report (2).docx")), "r");
        assert_eq!(engine.undo_label().as_deref(), Some("New file"));
        let task = CopyTask::template(dir.join("templates/Project"), &dir.join("here"), true);
        let (report, _) = finish(&engine, engine.submit(Box::new(task)), no_conflicts);
        assert_eq!(report.results, [dir.join("here/Project")]);
        assert_eq!(read(&dir.join("here/Project/src/main.rs")), "m");
        assert_eq!(engine.undo_label().as_deref(), Some("New folder"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_numbered_folder_template_keeps_its_contents() {
        let dir = test_dir("copy-template-numbered");
        write(&dir.join("templates/Project/src/main.rs"), "m");
        write(&dir.join("templates/Project/README.md"), "r");
        write(&dir.join("here/Project/old.txt"), "old");
        let engine = engine();
        let task = CopyTask::template(dir.join("templates/Project"), &dir.join("here"), true);
        let (report, _) = finish(&engine, engine.submit(Box::new(task)), no_conflicts);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(report.results, [dir.join("here/Project (2)")]);
        assert_eq!(read(&dir.join("here/Project (2)/src/main.rs")), "m");
        assert_eq!(read(&dir.join("here/Project (2)/README.md")), "r");
        assert_eq!(read(&dir.join("here/Project/old.txt")), "old");
        assert!(!dir.join("here/Project/README.md").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
