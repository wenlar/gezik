//! Bringing items back from the trash.

use std::io;
use std::path::PathBuf;

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
        for (root, (trashed, original)) in self.pairs.iter().enumerate() {
            match std::fs::symlink_metadata(trashed) {
                Ok(meta) => {
                    let item = PlanItem::new(Stage::Parallel, facts_of(&meta))
                        .source(trashed)
                        .target(original)
                        .checked()
                        .top(root);
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
}
