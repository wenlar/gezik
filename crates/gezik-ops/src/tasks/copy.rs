//! Copying files and folders, also next to themselves (Duplicate).

use std::io;
use std::path::{Path, PathBuf};

use gezik_core::ops::conflict::{Decision, Facts};
use gezik_core::ops::paths::{is_within, same_path};

use super::what;
use crate::task::{Outcome, PlanItem, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work, facts_after};
use crate::walk::{Step, facts_of, walk};

/// A folder on the way to a target, made before what goes into it (copy with folders).
const MAKE_PARENT: u8 = 1;

pub struct CopyTask {
    /// (source, target) per chosen item.
    pairs: Vec<(PathBuf, PathBuf)>,
    /// Keep both without asking: a copy into the source's own folder.
    presets: Vec<Option<Decision>>,
    /// Where they go, for the title; `None` for Duplicate.
    dir: Option<PathBuf>,
    /// A template copied in (spec 8.1): its kind (New file, New folder); `None`: a copy.
    new: Option<TaskKind>,
    /// Folders to make before the items (copy with folders, spec 4.6), shallowest first.
    parents: Vec<PathBuf>,
    /// Copy with folders: items whose relative path would leave the target folder.
    refused: Vec<PathBuf>,
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
        CopyTask { pairs, presets, dir: Some(dir.to_path_buf()), new: None, parents: Vec::new(), refused: Vec::new() }
    }

    /// Copies each source to `dir` joined with its path under a search's scope, making the
    /// folders on the way that are not there (spec 4.6): they count as made, so one undo takes
    /// them away too (only if they then hold no files). A name already there meets the conflict
    /// list; an item that would land on itself (pasted back into the search's folder) becomes
    /// `name (2)` without asking, like a paste into its own folder.
    pub fn with_folders(items: Vec<(PathBuf, PathBuf)>, dir: &Path) -> CopyTask {
        let super::Relative { pairs, parents, refused } = super::relative_targets(items, dir);
        let presets =
            pairs.iter().map(|(source, target)| same_path(source, target).then_some(Decision::KeepBoth)).collect();
        CopyTask { pairs, presets, dir: Some(dir.to_path_buf()), new: None, parents, refused }
    }

    /// Copies each source next to itself as `name (2)`.
    pub fn duplicate(sources: Vec<PathBuf>) -> CopyTask {
        let presets = vec![Some(Decision::KeepBoth); sources.len()];
        CopyTask {
            pairs: sources.into_iter().map(|source| (source.clone(), source)).collect(),
            presets,
            dir: None,
            new: None,
            parents: Vec::new(),
            refused: Vec::new(),
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
            parents: Vec::new(),
            refused: Vec::new(),
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
        super::refuse_outside(sink, &self.refused);
        if !super::plan_parents(sink, &self.parents, MAKE_PARENT) {
            return;
        }
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
        if item.tag == MAKE_PARENT {
            return super::make_parent_dir(item);
        }
        let (Some(source), Some(target)) = (&item.source, &item.target) else { return Ok(Outcome::Nothing) };
        if item.facts.is_dir {
            // Nothing when merged into a folder that was already there.
            return Ok(if super::make_dir(target)? {
                Outcome::Created { path: target.clone(), facts: facts_after(target, true), from: None }
            } else {
                Outcome::Nothing
            });
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
    fn keep_both_keeps_a_name_that_is_not_unicode() {
        #[cfg(unix)]
        let (name, numbered) = {
            use std::os::unix::ffi::OsStringExt;
            (
                std::ffi::OsString::from_vec(b"r\xfcz.txt".to_vec()),
                std::ffi::OsString::from_vec(b"r\xfcz (2).txt".to_vec()),
            )
        };
        #[cfg(windows)]
        let (name, numbered) = {
            use std::os::windows::ffi::OsStringExt;
            let wide = |tail: &str| {
                std::ffi::OsString::from_wide(
                    &[0x72, 0xD800].into_iter().chain(tail.encode_utf16()).collect::<Vec<u16>>(),
                )
            };
            (wide(".txt"), wide(" (2).txt"))
        };
        let dir = test_dir("copy-keep-both-bytes");
        write(&dir.join(&name), "x");
        let engine = engine();
        finish(&engine, engine.submit(Box::new(CopyTask::into(vec![dir.join(&name)], &dir))), no_conflicts);
        assert_eq!(read(&dir.join(&numbered)), "x");
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

    #[test]
    fn copying_with_folders_makes_only_the_missing_folders_and_undo_takes_them() {
        let dir = test_dir("copy-with-folders");
        write(&dir.join("src/a/b/x.txt"), "x");
        write(&dir.join("src/a/y.txt"), "y");
        write(&dir.join("dst/a/keep.txt"), "k");
        let items = vec![
            (dir.join("src/a/b/x.txt"), PathBuf::from("a").join("b").join("x.txt")),
            (dir.join("src/a/y.txt"), PathBuf::from("a").join("y.txt")),
        ];
        let engine = engine();
        let task = CopyTask::with_folders(items.clone(), &dir.join("dst"));
        let (report, _) = finish(&engine, engine.submit(Box::new(task)), no_conflicts);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(read(&dir.join("dst/a/b/x.txt")), "x");
        assert_eq!(read(&dir.join("dst/a/y.txt")), "y");
        let dst = dir.join("dst");
        // Files copy in parallel: the results come in the order they finish.
        let mut results = report.results.clone();
        results.sort();
        assert_eq!(results, [dst.join("a").join("b").join("x.txt"), dst.join("a").join("y.txt")]);
        let (undo, _) = finish(&engine, engine.undo().unwrap(), defaults);
        assert!(undo.failures.is_empty(), "{:?}", undo.failures);
        assert!(!dir.join("dst/a/b").exists(), "the folder it made went with the undo");
        assert!(!dir.join("dst/a/y.txt").exists());
        assert_eq!(read(&dir.join("dst/a/keep.txt")), "k", "the folder that was there stays");
        // Twice into the same place: the same names meet in the conflict list.
        finish(&engine, engine.submit(Box::new(CopyTask::with_folders(items.clone(), &dst))), no_conflicts);
        let asked = std::cell::Cell::new(0);
        finish(&engine, engine.submit(Box::new(CopyTask::with_folders(items, &dst))), |c| {
            asked.set(c.len());
            defaults(c)
        });
        assert_eq!(asked.get(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn copying_with_folders_back_onto_the_sources_keeps_both() {
        let dir = test_dir("copy-with-folders-onto-itself");
        write(&dir.join("a/x.txt"), "x");
        let items = vec![(dir.join("a/x.txt"), PathBuf::from("a").join("x.txt"))];
        let engine = engine();
        let (report, _) = finish(&engine, engine.submit(Box::new(CopyTask::with_folders(items, &dir))), no_conflicts);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(read(&dir.join("a/x.txt")), "x", "the source is never replaced by itself");
        assert_eq!(read(&dir.join("a/x (2).txt")), "x");
        let (undo, _) = finish(&engine, engine.undo().unwrap(), no_conflicts);
        assert!(undo.failures.is_empty(), "{:?}", undo.failures);
        assert_eq!(read(&dir.join("a/x.txt")), "x");
        assert!(!dir.join("a/x (2).txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_engine_never_replaces_an_item_with_itself() {
        let dir = test_dir("copy-replace-itself");
        write(&dir.join("x.txt"), "x");
        let x = dir.join("x.txt");
        let onto_itself = |preset: Option<Decision>| CopyTask {
            pairs: vec![(x.clone(), x.clone())],
            presets: vec![preset],
            dir: Some(dir.clone()),
            new: None,
            parents: Vec::new(),
            refused: Vec::new(),
        };
        let engine = engine();
        // Replace set beforehand.
        let (report, _) = finish(&engine, engine.submit(Box::new(onto_itself(Some(Decision::Replace)))), no_conflicts);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(read(&x), "x");
        // Replace chosen in the conflict list.
        let asked = std::cell::Cell::new(0);
        let (report, _) = finish(&engine, engine.submit(Box::new(onto_itself(None))), |c| {
            asked.set(c.len());
            vec![Decision::Replace; c.len()]
        });
        assert_eq!(asked.get(), 1);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(read(&x), "x", "still in its place, not in the trash");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1, "nothing else was made");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn undoing_a_copy_with_folders_keeps_files_put_or_changed_there_since() {
        let dir = test_dir("copy-with-folders-changed");
        write(&dir.join("src/a/b/x.txt"), "x");
        write(&dir.join("src/a/b/z.txt"), "z");
        let dst = dir.join("dst");
        let items = vec![
            (dir.join("src/a/b/x.txt"), PathBuf::from("a").join("b").join("x.txt")),
            (dir.join("src/a/b/z.txt"), PathBuf::from("a").join("b").join("z.txt")),
        ];
        let engine = engine();
        finish(&engine, engine.submit(Box::new(CopyTask::with_folders(items, &dst))), no_conflicts);
        // Since then: a new file in a folder the copy made, and a copied file edited.
        write(&dst.join("a/b/user.txt"), "mine");
        write(&dst.join("a/b/x.txt"), "x, edited");
        let (undo, _) = finish(&engine, engine.undo().unwrap(), no_conflicts);
        assert!(undo.failures.is_empty(), "{:?}", undo.failures);
        assert_eq!(read(&dst.join("a/b/user.txt")), "mine", "a file put there since stays");
        assert_eq!(read(&dst.join("a/b/x.txt")), "x, edited", "a copied file changed since stays");
        assert!(!dst.join("a/b/z.txt").exists(), "the untouched copy goes");
        assert!(undo.skipped_changed >= 2, "{}", undo.skipped_changed);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn undoing_a_copy_with_folders_takes_empty_folders_made_inside_since() {
        let dir = test_dir("copy-with-folders-empty-since");
        write(&dir.join("src/a/x.txt"), "x");
        let dst = dir.join("dst");
        let items = vec![(dir.join("src/a/x.txt"), PathBuf::from("a").join("x.txt"))];
        let engine = engine();
        finish(&engine, engine.submit(Box::new(CopyTask::with_folders(items, &dst))), no_conflicts);
        std::fs::create_dir_all(dst.join("a/later/deeper")).unwrap();
        let (undo, _) = finish(&engine, engine.undo().unwrap(), no_conflicts);
        assert!(undo.failures.is_empty(), "{:?}", undo.failures);
        assert!(!dst.exists(), "the folders it made held no files: they went, the empty one inside too");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_relative_path_that_leaves_the_target_folder_is_refused() {
        let dir = test_dir("copy-with-folders-escape");
        write(&dir.join("src/x.txt"), "x");
        write(&dir.join("src/y.txt"), "y");
        write(&dir.join("src/z.txt"), "z");
        write(&dir.join("src/ok.txt"), "ok");
        let dst = dir.join("dst");
        let items = vec![
            (dir.join("src/x.txt"), PathBuf::from("..").join("x.txt")),
            (dir.join("src/y.txt"), dir.join("elsewhere").join("y.txt")),
            (dir.join("src/z.txt"), PathBuf::new()),
            (dir.join("src/ok.txt"), PathBuf::from("ok.txt")),
        ];
        let engine = engine();
        let (report, _) = finish(&engine, engine.submit(Box::new(CopyTask::with_folders(items, &dst))), no_conflicts);
        let mut failed: Vec<PathBuf> = report.failures.iter().map(|f| f.path.clone()).collect();
        failed.sort();
        assert_eq!(failed, [dir.join("src/x.txt"), dir.join("src/y.txt"), dir.join("src/z.txt")]);
        assert!(!dir.join("x.txt").exists() && !dir.join("elsewhere").exists());
        assert_eq!(std::fs::read_dir(&dst).unwrap().count(), 1);
        assert_eq!(read(&dst.join("ok.txt")), "ok");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_item_inside_another_chosen_one_goes_with_it() {
        let dir = test_dir("copy-with-folders-nested");
        write(&dir.join("src/a/x.txt"), "x");
        let dst = dir.join("dst");
        let items =
            vec![(dir.join("src/a/x.txt"), PathBuf::from("a").join("x.txt")), (dir.join("src/a"), PathBuf::from("a"))];
        let engine = engine();
        let task = CopyTask::with_folders(items, &dst);
        assert_eq!(task.count(), 1);
        let (report, _) = finish(&engine, engine.submit(Box::new(task)), no_conflicts);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(read(&dst.join("a/x.txt")), "x");
        assert_eq!(std::fs::read_dir(dst.join("a")).unwrap().count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Undo, redo, undo of a copy with folders, many times: the folders made on the way come
    /// back before what was copied into them, and the last undo leaves nothing behind.
    #[test]
    fn undo_redo_undo_of_a_copy_with_folders_leaves_nothing() {
        let dir = test_dir("copy-with-folders-redo");
        write(&dir.join("src/a/b/x.txt"), "x");
        write(&dir.join("src/a/b/y.txt"), "y");
        write(&dir.join("src/a/z.txt"), "z");
        let dst = dir.join("dst");
        let items = vec![
            (dir.join("src/a/b/x.txt"), PathBuf::from("a").join("b").join("x.txt")),
            (dir.join("src/a/b/y.txt"), PathBuf::from("a").join("b").join("y.txt")),
            (dir.join("src/a/z.txt"), PathBuf::from("a").join("z.txt")),
        ];
        let engine = engine();
        for round in 0..25 {
            let (report, _) =
                finish(&engine, engine.submit(Box::new(CopyTask::with_folders(items.clone(), &dst))), no_conflicts);
            assert!(report.failures.is_empty(), "{round}: {:?}", report.failures);
            let (undo, _) = finish(&engine, engine.undo().unwrap(), no_conflicts);
            assert!(undo.failures.is_empty(), "{round}: {:?}", undo.failures);
            assert!(!dst.exists(), "{round}: the first undo");
            let (redo, _) = finish(&engine, engine.redo().unwrap(), no_conflicts);
            assert!(redo.failures.is_empty(), "{round}: {:?}", redo.failures);
            assert_eq!(read(&dst.join("a/b/x.txt")), "x");
            assert_eq!(read(&dst.join("a/z.txt")), "z");
            let (undo, _) = finish(&engine, engine.undo().unwrap(), no_conflicts);
            assert!(undo.failures.is_empty(), "{round}: {:?}", undo.failures);
            assert_eq!(undo.skipped_changed, 0, "{round}");
            assert!(!dst.exists(), "{round}: nothing is left behind");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The engine's guard knows the same file by what it is, not by how it is spelled.
    #[cfg(windows)]
    #[test]
    fn the_engine_never_replaces_an_item_with_itself_spelled_otherwise() {
        let dir = test_dir("copy-replace-itself-spelled");
        write(&dir.join("f/x.txt"), "x");
        let x = dir.join("f").join("x.txt");
        let junction = dir.join("j");
        let made = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(&junction)
            .arg(dir.join("f"))
            .output()
            .unwrap();
        assert!(made.status.success(), "{made:?}");
        let prefixed = PathBuf::from(format!(r"\\?\{}", x.display()));
        let engine = engine();
        for target in [prefixed, junction.join("x.txt")] {
            for preset in [Some(Decision::Replace), None] {
                let task = CopyTask {
                    pairs: vec![(x.clone(), target.clone())],
                    presets: vec![preset],
                    dir: Some(dir.clone()),
                    new: None,
                    parents: Vec::new(),
                    refused: Vec::new(),
                };
                let (report, _) = finish(&engine, engine.submit(Box::new(task)), |c| vec![Decision::Replace; c.len()]);
                assert!(report.failures.is_empty(), "{target:?}: {:?}", report.failures);
                assert_eq!(read(&x), "x", "{target:?}: still in its place, not in the trash");
            }
        }
        assert_eq!(std::fs::read_dir(dir.join("f")).unwrap().count(), 1, "nothing else was made");
        std::fs::remove_dir(&junction).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
