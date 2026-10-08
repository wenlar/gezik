//! Bringing items back from the trash.

use std::io;
use std::path::PathBuf;

use gezik_core::ops::paths::{is_within, same_path};
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
        let mut order: Vec<usize> = (0..self.pairs.len()).collect();
        order.sort_by_key(|&i| self.pairs[i].1.components().count());
        let holds = |dir: &PathBuf| self.pairs.iter().any(|(_, other)| is_within(other, dir) && !same_path(other, dir));
        for root in order {
            let (trashed, original) = &self.pairs[root];
            match std::fs::symlink_metadata(trashed) {
                Ok(meta) => {
                    // Before items run as they are planned, outer ones first.
                    let stage = if holds(original) { Stage::Before } else { Stage::Parallel };
                    let item =
                        PlanItem::new(stage, facts_of(&meta)).source(trashed).target(original).checked().top(root);
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
}
