//! A new, empty folder or file.

use std::io;
use std::path::{Path, PathBuf};

use gezik_core::ops::conflict::{Decision, Facts};

use crate::task::{Outcome, PlanItem, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work, facts_after};

pub struct NewTask {
    dir: PathBuf,
    name: &'static str,
    folder: bool,
}

impl NewTask {
    pub fn folder(dir: &Path) -> NewTask {
        NewTask { dir: dir.to_path_buf(), name: "New folder", folder: true }
    }

    pub fn file(dir: &Path) -> NewTask {
        NewTask { dir: dir.to_path_buf(), name: "New file.txt", folder: false }
    }
}

impl Task for NewTask {
    fn kind(&self) -> TaskKind {
        if self.folder { TaskKind::NewFolder } else { TaskKind::NewFile }
    }

    fn title(&self) -> String {
        format!("Creating {}", self.name)
    }

    fn count(&self) -> usize {
        1
    }

    fn resources(&self) -> Resources {
        Resources { paths: vec![self.dir.clone()], work: Work::Disk }
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        let facts = Facts { is_dir: self.folder, ..Facts::default() };
        // A taken name becomes "New folder (2)" without asking.
        let item = PlanItem::new(Stage::Parallel, facts)
            .target(self.dir.join(self.name))
            .checked()
            .top(0)
            .preset(Some(Decision::KeepBoth));
        sink.item(item);
    }

    fn run(&self, item: &PlanItem, _cx: &RunCx<'_>) -> io::Result<Outcome> {
        let Some(target) = &item.target else { return Ok(Outcome::Nothing) };
        if self.folder {
            std::fs::create_dir(target)?;
        } else {
            std::fs::OpenOptions::new().write(true).create_new(true).open(target)?;
        }
        Ok(Outcome::Created { path: target.clone(), facts: facts_after(target, self.folder), from: None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{defaults, engine, finish, test_dir};

    #[test]
    fn a_new_folder_takes_the_next_free_name() {
        let dir = test_dir("new-folder");
        std::fs::create_dir(dir.join("New folder")).unwrap();
        let engine = engine();
        let job = engine.submit(Box::new(NewTask::folder(&dir)));
        let (report, _) = finish(&engine, job, defaults);
        assert!(dir.join("New folder (2)").is_dir());
        assert_eq!(report.results, [dir.join("New folder (2)")]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_new_file_is_empty() {
        let dir = test_dir("new-file");
        let engine = engine();
        let job = engine.submit(Box::new(NewTask::file(&dir)));
        finish(&engine, job, defaults);
        assert_eq!(std::fs::metadata(dir.join("New file.txt")).unwrap().len(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
