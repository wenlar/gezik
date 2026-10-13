//! Changing tags, rating and comment (spec 12 §3.5): every file read and written by the
//! engine's tag store, one undo for the whole job. Undo writes back what was there, but only
//! to a file whose tags are still what the job left (Finder or another program may have
//! changed them since).

use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use gezik_core::meta::{Meta, MetaEdit};
use gezik_core::ops::conflict::Facts;

use super::what;
use crate::meta::Where;
use crate::task::{Outcome, PlanItem, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work, changed_since};

enum Change {
    /// The same edits on each file's own tags.
    Edit(Arc<[MetaEdit]>),
    /// Back to `to`, if the file's tags are still `expect`.
    Set { expect: Meta, to: Meta },
}

pub struct TagsTask {
    items: Vec<(PathBuf, Change)>,
}

impl TagsTask {
    /// `edits` done to each of `paths`.
    pub fn new(paths: Vec<PathBuf>, edits: Vec<MetaEdit>) -> TagsTask {
        let edits: Arc<[MetaEdit]> = edits.into();
        TagsTask { items: paths.into_iter().map(|path| (path, Change::Edit(edits.clone()))).collect() }
    }

    /// Undo: each path back to its `to`, if its tags are still `expect` (path, expect, to).
    pub fn restore(items: Vec<(PathBuf, Meta, Meta)>) -> TagsTask {
        TagsTask { items: items.into_iter().map(|(path, expect, to)| (path, Change::Set { expect, to })).collect() }
    }

    fn paths(&self) -> Vec<PathBuf> {
        self.items.iter().map(|(path, _)| path.clone()).collect()
    }
}

impl Task for TagsTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Tags
    }

    fn title(&self) -> String {
        gezik_i18n::t!("Changing the tags of {}", what(&self.paths()))
    }

    fn count(&self) -> usize {
        self.items.len()
    }

    fn resources(&self) -> Resources {
        Resources { paths: self.paths(), work: Work::Disk }
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        for (root, (path, _)) in self.items.iter().enumerate() {
            if !sink.item(PlanItem::new(Stage::Parallel, Facts::default()).source(path).top(root)) {
                return;
            }
        }
    }

    fn run(&self, item: &PlanItem, cx: &RunCx<'_>) -> io::Result<Outcome> {
        let (Some(path), Some((_, change))) = (&item.source, self.items.get(item.root)) else {
            return Ok(Outcome::Nothing);
        };
        let store = cx.meta().ok_or_else(|| io::Error::other(gezik_i18n::t!("No tag store")))?;
        let before = store.read(path)?;
        let after = match change {
            Change::Edit(edits) => {
                let mut after = before.clone();
                for edit in edits.iter() {
                    after.apply(edit);
                }
                after
            }
            Change::Set { expect, .. } if !expect.matches(&before) => return Err(changed_since()),
            Change::Set { to, .. } => to.clone(),
        };
        if after == before {
            return Ok(Outcome::Nothing);
        }
        if let Where::Store(Some(why)) = store.write(path, &after)? {
            cx.note(path, &gezik_i18n::t!("Kept in Gezik's tag store: {}", why));
        }
        Ok(Outcome::MetaChanged { path: path.clone(), before, after })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{Engine, Settings};
    use crate::testing::{finish, test_dir};
    use gezik_core::meta::{Tag, TagColor};

    fn engine(dir: &std::path::Path) -> Engine {
        Engine::new(Settings { tags_store: Some(dir.join("config/tags-store.tsv")), ..Settings::default() }, || {})
    }

    fn red() -> Tag {
        Tag::new("Red", TagColor::Red)
    }

    /// Spec 12 §1: 10,000 files tagged in one job within 3 s on Windows NTFS, 1 s elsewhere (SSD).
    #[test]
    #[ignore = "timing: run on a quiet machine (cargo test -p gezik-ops ten_thousand -- --ignored --nocapture)"]
    fn ten_thousand_files_are_tagged_in_time() {
        let dir = test_dir("tags-10k");
        let paths: Vec<_> = (0..10_000).map(|i| dir.join(format!("f{i}.txt"))).collect();
        for path in &paths {
            std::fs::write(path, "").unwrap();
        }
        let engine = engine(&dir);
        let started = std::time::Instant::now();
        let job = engine.submit(Box::new(TagsTask::new(paths.clone(), vec![MetaEdit::AddTags(vec![red()])])));
        let (report, _) = finish(&engine, job, |_| Vec::new());
        let took = started.elapsed();
        eprintln!("10,000 files tagged in {took:?}");
        assert!(report.failures.is_empty(), "{:?}", report.failures.first());
        assert!(engine.meta().read(&paths[9_999]).unwrap().has_tag("Red"));
        let limit = if cfg!(windows) { 3.0 } else { 1.0 };
        assert!(took.as_secs_f64() <= limit, "{took:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tags_change_as_one_job_and_come_back_with_undo() {
        let dir = test_dir("tags-undo");
        let engine = engine(&dir);
        let (a, b) = (dir.join("a.txt"), dir.join("b.txt"));
        std::fs::write(&a, "a").unwrap();
        std::fs::write(&b, "b").unwrap();
        let store = engine.meta();
        store.write(&b, &Meta { tags: vec![red()], rating: 5, ..Meta::default() }).unwrap();
        let job = engine.submit(Box::new(TagsTask::new(
            vec![a.clone(), b.clone()],
            vec![MetaEdit::AddTags(vec![red(), Tag::new("Project X", TagColor::None)]), MetaEdit::Rating(3)],
        )));
        let (report, events) = finish(&engine, job, |_| Vec::new());
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert!(report.changed_dirs.is_empty(), "no folder to read again: {:?}", report.changed_dirs);
        assert!(!events.iter().any(|e| matches!(e, crate::Event::Changed { .. })), "{events:?}");
        assert_eq!(engine.undo_label().as_deref(), Some("Change tags of 2 items"));
        assert_eq!(store.read(&a).unwrap().tags.len(), 2);
        assert_eq!(store.read(&b).unwrap().rating, 3);
        assert_eq!(std::fs::read(&a).unwrap(), b"a", "contents untouched");
        let undo = engine.undo().unwrap();
        let (report, _) = finish(&engine, undo, |_| Vec::new());
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(store.read(&a).unwrap(), Meta::default());
        assert_eq!(
            store.read(&b).unwrap(),
            crate::testing::as_kept(Meta { tags: vec![red()], rating: 5, ..Meta::default() })
        );
        let redo = engine.redo().unwrap();
        finish(&engine, redo, |_| Vec::new());
        assert_eq!(store.read(&b).unwrap().rating, 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn undo_leaves_a_file_changed_since_alone() {
        let dir = test_dir("tags-changed");
        let engine = engine(&dir);
        let a = dir.join("a.txt");
        std::fs::write(&a, "a").unwrap();
        let job = engine.submit(Box::new(TagsTask::new(vec![a.clone()], vec![MetaEdit::AddTags(vec![red()])])));
        finish(&engine, job, |_| Vec::new());
        // Finder or Dolphin changes it meanwhile.
        let theirs = Meta { comment: "theirs".into(), ..Meta::default() };
        engine.meta().write(&a, &theirs).unwrap();
        let undo = engine.undo().unwrap();
        let (report, _) = finish(&engine, undo, |_| Vec::new());
        assert_eq!(report.skipped_changed, 1);
        assert_eq!(engine.meta().read(&a).unwrap(), theirs);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_job_that_changes_nothing_has_no_undo() {
        let dir = test_dir("tags-nothing");
        let engine = engine(&dir);
        let a = dir.join("a.txt");
        std::fs::write(&a, "a").unwrap();
        let job = engine.submit(Box::new(TagsTask::new(vec![a.clone()], vec![MetaEdit::ClearTags])));
        let (report, _) = finish(&engine, job, |_| Vec::new());
        assert!(report.failures.is_empty());
        assert_eq!(engine.undo_label(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_copy_and_a_move_to_a_drive_without_records_keep_their_tags() {
        let dir = test_dir("tags-copy");
        let engine = engine(&dir);
        // Paths with `fat` in them are on a volume that holds no records (FAT, exFAT).
        engine.meta().fake_holds(|path| !path.to_string_lossy().contains("fat"));
        std::fs::create_dir(dir.join("fat")).unwrap();
        let a = dir.join("a.txt");
        std::fs::write(&a, "a").unwrap();
        engine.meta().write(&a, &Meta { tags: vec![red()], ..Meta::default() }).unwrap();
        let job = engine.submit(Box::new(crate::tasks::CopyTask::into(vec![a.clone()], &dir.join("fat"))));
        let (report, _) = finish(&engine, job, |_| Vec::new());
        assert!(report.failures.is_empty() && report.notes.is_empty(), "{report:?}");
        let kept = crate::testing::as_kept(Meta { tags: vec![red()], ..Meta::default() }).tags;
        assert_eq!(engine.meta().read(&dir.join("fat/a.txt")).unwrap().tags, kept);
        let store = std::fs::read_to_string(dir.join("config/tags-store.tsv")).unwrap();
        assert!(store.contains("fat/a.txt"), "saved when the job ended: {store}");
        // Renamed by Gezik on that drive: the entry follows.
        let job = engine.submit(Box::new(crate::tasks::RenameTask::one(dir.join("fat/a.txt"), "b.txt")));
        finish(&engine, job, |_| Vec::new());
        assert_eq!(engine.meta().read(&dir.join("fat/b.txt")).unwrap().tags, kept);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_file_fails_and_is_not_made() {
        let dir = test_dir("tags-missing");
        let engine = engine(&dir);
        let gone = dir.join("gone.txt");
        let job = engine.submit(Box::new(TagsTask::new(vec![gone.clone()], vec![MetaEdit::AddTags(vec![red()])])));
        let (report, _) = finish(&engine, job, |_| Vec::new());
        assert_eq!(report.failures.len(), 1);
        assert!(!gone.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
