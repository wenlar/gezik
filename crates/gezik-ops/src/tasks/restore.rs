//! Bringing items back from the trash.

use std::io;
use std::path::PathBuf;

use gezik_core::ops::paths::path_key;
use gezik_platform::fs;

use super::what;
use crate::task::{Outcome, PlanItem, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work, facts_after};
use crate::walk::facts_of;

pub struct RestoreTask {
    /// (where it is in the trash, where it was).
    pairs: Vec<(PathBuf, PathBuf)>,
}

impl RestoreTask {
    pub fn new(pairs: Vec<(PathBuf, PathBuf)>) -> RestoreTask {
        RestoreTask { pairs }
    }
}

impl Task for RestoreTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Restore
    }

    fn title(&self) -> String {
        let originals: Vec<PathBuf> = self.pairs.iter().map(|(_, original)| original.clone()).collect();
        format!("Restoring {}", what(&originals))
    }

    fn count(&self) -> usize {
        self.pairs.len()
    }

    fn resources(&self) -> Resources {
        let mut paths: Vec<PathBuf> = self.pairs.iter().map(|(trashed, _)| trashed.clone()).collect();
        paths.extend(self.pairs.iter().filter_map(|(_, original)| original.parent().map(|p| p.to_path_buf())));
        Resources { paths, work: Work::Disk }
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        // A folder that comes back with items inside it (a folder made on the way, and what was
        // copied into it) comes back first, before any of them: restoring one of them makes
        // the folders above it, and the folder itself then could not come back.
        // Each key once, and every folder above an original once: linear in the items (times
        // their depth), as a trash undo may bring back many thousands.
        let keys: Vec<Vec<String>> = self.pairs.iter().map(|(_, original)| path_key(original)).collect();
        let above: std::collections::HashSet<&[String]> =
            keys.iter().flat_map(|key| (1..key.len()).map(move |n| &key[..n])).collect();
        let mut order: Vec<usize> = (0..self.pairs.len()).collect();
        order.sort_by_key(|&i| keys[i].len());
        for root in order {
            let (trashed, original) = &self.pairs[root];
            let holder = above.contains(keys[root].as_slice());
            match std::fs::symlink_metadata(trashed) {
                Ok(meta) => {
                    // Before items run as they are planned, outer ones first.
                    let stage = if holder { Stage::Before } else { Stage::Parallel };
                    let item =
                        PlanItem::new(stage, facts_of(&meta)).source(trashed).target(original).checked().top(root);
                    // A folder there already is a conflict, as for any restore: never merged.
                    let item = if holder { item.no_merge() } else { item };
                    if !sink.item(item) {
                        return;
                    }
                }
                Err(_) => sink.failed(original, io::Error::new(io::ErrorKind::NotFound, "no longer in the trash")),
            }
        }
    }

    fn run(&self, item: &PlanItem, _cx: &RunCx<'_>) -> io::Result<Outcome> {
        let (Some(trashed), Some(original)) = (&item.source, &item.target) else { return Ok(Outcome::Nothing) };
        fs::restore(trashed, original)?;
        Ok(Outcome::Restored { original: original.clone(), facts: facts_after(original, item.facts.is_dir) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Decision;
    use crate::testing::{engine, finish, read, test_dir, write};

    #[test]
    fn restoring_onto_a_taken_name_asks() {
        let dir = test_dir("restore");
        let original = dir.join("x.txt");
        write(&original, "old");
        let trashed = fs::trash(&original).unwrap().unwrap();
        write(&original, "newer");
        let engine = engine();
        let job = engine.submit(Box::new(RestoreTask::new(vec![(trashed, original.clone())])));
        let (report, _) = finish(&engine, job, |c| vec![Decision::KeepBoth; c.len()]);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(read(&original), "newer");
        assert_eq!(read(&dir.join("x (2).txt")), "old");
        assert_eq!(report.results, [dir.join("x (2).txt")]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_folder_comes_back_before_what_was_inside_it() {
        let dir = test_dir("restore-order");
        let bin = dir.join("bin");
        write(&bin.join("1"), "x");
        std::fs::create_dir_all(bin.join("2")).unwrap();
        std::fs::create_dir_all(bin.join("3/b")).unwrap();
        let dst = dir.join("dst");
        let pairs = vec![
            (bin.join("1"), dst.join("a/b/x.txt")),
            (bin.join("2"), dst.join("a/b")),
            (bin.join("3"), dst.join("a")),
        ];
        let mut sink = crate::testing::CollectSink::default();
        RestoreTask::new(pairs.clone()).plan(&mut sink);
        let planned: Vec<(PathBuf, Stage)> =
            sink.items.iter().map(|item| (item.target.clone().unwrap(), item.stage)).collect();
        assert_eq!(
            planned,
            [
                (dst.join("a"), Stage::Before),
                (dst.join("a/b"), Stage::Before),
                (dst.join("a/b/x.txt"), Stage::Parallel)
            ]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn many_items_plan_by_lookup() {
        plan_many("restore-many", std::time::Duration::from_secs(60));
    }

    /// The bound a quiet machine meets (debug build); a loaded one may not.
    #[test]
    #[ignore = "timing: run on a quiet machine"]
    fn many_items_plan_quickly() {
        plan_many("restore-many-strict", std::time::Duration::from_secs(5));
    }

    fn plan_many(name: &str, bound: std::time::Duration) {
        let dir = test_dir(name);
        let bin = dir.join("bin");
        let dst = dir.join("dst");
        std::fs::create_dir_all(&bin).unwrap();
        let pairs: Vec<(PathBuf, PathBuf)> =
            (0..5000).map(|i| (bin.join(format!("{i}")), dst.join(format!("{i}.txt")))).collect();
        for (trashed, _) in &pairs {
            write(trashed, "x");
        }
        let started = std::time::Instant::now();
        let mut sink = crate::testing::CollectSink::default();
        RestoreTask::new(pairs).plan(&mut sink);
        let took = started.elapsed();
        assert_eq!((sink.items.len(), sink.failed.len()), (5000, 0));
        assert!(sink.items.iter().all(|item| item.stage == Stage::Parallel));
        assert!(took < bound, "{took:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_folder_coming_back_onto_a_folder_asks() {
        let dir = test_dir("restore-folder-taken");
        let bin = dir.join("bin");
        write(&bin.join("1"), "x");
        std::fs::create_dir_all(bin.join("2")).unwrap();
        let dst = dir.join("dst");
        // Made again since: the folder must not merge silently and stay in the trash.
        std::fs::create_dir_all(dst.join("a")).unwrap();
        let pairs = vec![(bin.join("1"), dst.join("a/x.txt")), (bin.join("2"), dst.join("a"))];
        let engine = engine();
        let asked = std::cell::RefCell::new(Vec::new());
        let (report, _) = finish(&engine, engine.submit(Box::new(RestoreTask::new(pairs))), |c| {
            asked.borrow_mut().extend(c.iter().map(|c| c.target.clone()));
            vec![Decision::KeepBoth; c.len()]
        });
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(*asked.borrow(), [dst.join("a")]);
        assert!(dst.join("a (2)").is_dir(), "the folder came back beside it");
        assert_eq!(read(&dst.join("a (2)/x.txt")), "x", "what was inside follows it");
        assert!(!bin.join("2").exists() && !bin.join("1").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn two_items_from_one_place_ask_about_the_second() {
        let dir = test_dir("restore-same-name");
        let bin = dir.join("bin");
        write(&bin.join("$R1.txt"), "first");
        write(&bin.join("$R2.txt"), "second");
        let original = dir.join("dst").join("x.txt");
        let pairs = vec![(bin.join("$R1.txt"), original.clone()), (bin.join("$R2.txt"), original.clone())];
        let engine = engine();
        let asked = std::cell::RefCell::new(Vec::new());
        let (report, _) = finish(&engine, engine.submit(Box::new(RestoreTask::new(pairs))), |c| {
            asked.borrow_mut().extend(c.iter().map(|c| c.target.clone()));
            vec![Decision::KeepBoth; c.len()]
        });
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(*asked.borrow(), std::slice::from_ref(&original));
        assert_eq!(read(&original), "first");
        assert_eq!(read(&dir.join("dst").join("x (2).txt")), "second");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failure_names_the_original_not_the_bin_entry() {
        let dir = test_dir("restore-fail-name");
        let bin = dir.join("bin");
        write(&bin.join("$R1.txt"), "x");
        // A file where its folder should be: it cannot come back.
        write(&dir.join("dst"), "a file");
        let original = dir.join("dst").join("x.txt");
        let engine = engine();
        let job = engine.submit(Box::new(RestoreTask::new(vec![(bin.join("$R1.txt"), original.clone())])));
        let (report, _) = finish(&engine, job, |c| vec![Decision::KeepBoth; c.len()]);
        let failed: Vec<&std::path::Path> = report.failures.iter().map(|f| f.path.as_path()).collect();
        assert_eq!(failed, [original.as_path()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_folder_is_made_again() {
        let dir = test_dir("restore-missing-folder");
        let original = dir.join("gone").join("deeper").join("x.txt");
        write(&original, "x");
        let trashed = fs::trash(&original).unwrap().unwrap();
        std::fs::remove_dir_all(dir.join("gone")).unwrap();
        let engine = engine();
        let job = engine.submit(Box::new(RestoreTask::new(vec![(trashed.clone(), original.clone())])));
        let (report, _) = finish(&engine, job, |c| vec![Decision::KeepBoth; c.len()]);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(read(&original), "x");
        assert!(std::fs::symlink_metadata(&trashed).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
