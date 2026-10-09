//! Keep on this device / Free up space (spec 9 §7.3): one call to the cloud app per chosen
//! item (a folder's contents go with it on Windows), a couple at a time. Nothing to undo: it
//! changes no data, only what the cloud app keeps here. Items Gezik leaves alone (not synced,
//! not the cloud app's, a link) are notes.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gezik_core::ops::conflict::Facts;

use super::what;
use crate::task::{Outcome, PlanItem, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work};

/// The cloud app: the system, or a test's record.
pub(crate) trait PinIo: Send + Sync {
    fn set(&self, path: &Path, free_up: bool) -> io::Result<()>;
}

struct SystemIo;

impl PinIo for SystemIo {
    fn set(&self, path: &Path, free_up: bool) -> io::Result<()> {
        if free_up { gezik_platform::cloud_pin::free_up(path) } else { gezik_platform::cloud_pin::keep(path) }
    }
}

pub struct CloudPinTask {
    paths: Vec<PathBuf>,
    keep: bool,
    io: Arc<dyn PinIo>,
}

impl CloudPinTask {
    /// Keeps `paths` on this device (`keep`), or frees up their space.
    pub fn new(paths: Vec<PathBuf>, keep: bool) -> CloudPinTask {
        CloudPinTask { paths, keep, io: Arc::new(SystemIo) }
    }

    #[cfg(test)]
    pub(crate) fn with_io(mut self, io: Arc<dyn PinIo>) -> CloudPinTask {
        self.io = io;
        self
    }
}

impl Task for CloudPinTask {
    fn kind(&self) -> TaskKind {
        if self.keep { TaskKind::KeepOnDevice } else { TaskKind::FreeUpSpace }
    }

    fn title(&self) -> String {
        if self.keep {
            format!("Keeping {} on this device", what(&self.paths))
        } else {
            format!("Freeing up the space of {}", what(&self.paths))
        }
    }

    fn count(&self) -> usize {
        self.paths.len()
    }

    fn resources(&self) -> Resources {
        Resources { paths: self.paths.clone(), work: Work::External }
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        for (root, path) in self.paths.iter().enumerate() {
            if !sink.item(PlanItem::new(Stage::Parallel, Facts::default()).source(path).top(root)) {
                return;
            }
        }
    }

    fn run(&self, item: &PlanItem, cx: &RunCx<'_>) -> io::Result<Outcome> {
        let Some(path) = &item.source else { return Ok(Outcome::Nothing) };
        match self.io.set(path, !self.keep) {
            Ok(()) => Ok(Outcome::Nothing),
            Err(err) if gezik_platform::cloud_pin::is_left_alone(&err) => {
                cx.skip(path, &err);
                Ok(Outcome::Nothing)
            }
            Err(err) => Err(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::engine::Report;
    use crate::testing::{defaults, engine, finish, test_dir, write};

    /// Records what was asked; answers as told.
    #[derive(Default)]
    struct Fake {
        asked: Mutex<Vec<(PathBuf, bool)>>,
        not_synced: Mutex<Vec<PathBuf>>,
        refused: Mutex<Vec<PathBuf>>,
    }

    impl PinIo for Fake {
        fn set(&self, path: &Path, free_up: bool) -> io::Result<()> {
            if self.refused.lock().unwrap().iter().any(|p| p == path) {
                return Err(io::ErrorKind::PermissionDenied.into());
            }
            if free_up && self.not_synced.lock().unwrap().iter().any(|p| p == path) {
                return Err(gezik_platform::cloud_pin::not_synced_error());
            }
            self.asked.lock().unwrap().push((path.to_path_buf(), free_up));
            Ok(())
        }
    }

    fn run(fake: &Arc<Fake>, task: CloudPinTask) -> Report {
        let engine = engine();
        let job = engine.submit(Box::new(task.with_io(fake.clone())));
        finish(&engine, job, defaults).0
    }

    fn files(name: &str, names: &[&str]) -> (PathBuf, Vec<PathBuf>) {
        let dir = test_dir(name);
        let paths: Vec<PathBuf> = names.iter().map(|n| dir.join(n)).collect();
        paths.iter().for_each(|p| write(p, "x"));
        (dir, paths)
    }

    #[test]
    fn each_chosen_item_is_asked_once() {
        let (dir, paths) = files("cloud-each", &["a.txt", "b.txt"]);
        let fake = Arc::new(Fake::default());
        let report = run(&fake, CloudPinTask::new(paths.clone(), true));
        assert!(report.failures.is_empty() && report.skipped.is_empty(), "{report:?}");
        let mut asked = fake.asked.lock().unwrap().clone();
        asked.sort();
        assert_eq!(asked, [(paths[0].clone(), false), (paths[1].clone(), false)], "keep: not free up");
        assert_eq!(report.kind, TaskKind::KeepOnDevice);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_left_alone_item_is_a_note_not_a_failure() {
        let (dir, paths) = files("cloud-note", &["synced.txt", "edited.txt"]);
        let fake = Arc::new(Fake::default());
        fake.not_synced.lock().unwrap().push(paths[1].clone());
        let report = run(&fake, CloudPinTask::new(paths.clone(), false));
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].path, paths[1]);
        assert_eq!(*fake.asked.lock().unwrap(), [(paths[0].clone(), true)], "only the synced one was freed");
        assert_eq!(report.kind, TaskKind::FreeUpSpace);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_refusal_is_a_failure() {
        let (dir, paths) = files("cloud-fail", &["a.txt"]);
        let fake = Arc::new(Fake::default());
        fake.refused.lock().unwrap().push(paths[0].clone());
        let report = run(&fake, CloudPinTask::new(paths, true));
        assert_eq!(report.failures.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_link_is_left_as_it_is() {
        let dir = test_dir("cloud-link");
        let target = dir.join("elsewhere");
        std::fs::create_dir_all(&target).unwrap();
        let link = dir.join("link");
        #[cfg(windows)]
        let made = gezik_platform::link::create(gezik_platform::link::LinkKind::Junction, &target, &link, true).is_ok();
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(&target, &link).is_ok();
        assert!(made, "a junction needs no privilege; a symbolic link on Unix neither");
        // The real platform call, not the fake: the link check is the platform's.
        let err = gezik_platform::cloud_pin::keep(&link).unwrap_err();
        assert!(gezik_platform::cloud_pin::is_left_alone(&err), "{err}");
        assert_eq!(err.to_string(), gezik_platform::cloud_pin::A_LINK);
        assert!(target.is_dir(), "nothing done to the target");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
