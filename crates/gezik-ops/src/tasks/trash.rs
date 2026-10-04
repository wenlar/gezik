//! Moving to the trash (Recycle Bin).

use std::io;
use std::path::PathBuf;

use gezik_core::ops::conflict::Facts;
use gezik_platform::fs;

use super::what;
use crate::task::{
    Outcome, PlanItem, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work, changed_since, no_trash, unchanged,
};
use crate::walk::facts_of;

pub struct TrashTask {
    /// Each item, and how it must still look (undo); `None`: anything.
    items: Vec<(PathBuf, Option<Facts>)>,
    /// An undo: on a drive without a trash, an empty folder or file is simply deleted.
    undoing: bool,
}

impl TrashTask {
    pub fn new(paths: Vec<PathBuf>) -> TrashTask {
        TrashTask { items: paths.into_iter().map(|path| (path, None)).collect(), undoing: false }
    }

    /// Undo of a copy or of something new: items changed since are left alone.
    pub(crate) fn checked(items: Vec<(PathBuf, Option<Facts>)>) -> TrashTask {
        TrashTask { items, undoing: true }
    }

    fn paths(&self) -> Vec<PathBuf> {
        self.items.iter().map(|(path, _)| path.clone()).collect()
    }
}

fn is_empty_dir(path: &std::path::Path) -> bool {
    std::fs::read_dir(path).is_ok_and(|mut entries| entries.next().is_none())
}

impl Task for TrashTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Trash
    }

    fn title(&self) -> String {
        let bin = if cfg!(windows) { "the Recycle Bin" } else { "the Trash" };
        format!("Moving {} to {bin}", what(&self.paths()))
    }

    fn count(&self) -> usize {
        self.items.len()
    }

    fn resources(&self) -> Resources {
        Resources { paths: self.paths(), work: Work::Disk }
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        for (root, (path, _)) in self.items.iter().enumerate() {
            if super::refuse_root(sink, path, "move to the trash") {
                continue;
            }
            match std::fs::symlink_metadata(path) {
                Ok(meta) => {
                    if !sink.item(PlanItem::new(Stage::Parallel, facts_of(&meta)).source(path).top(root)) {
                        return;
                    }
                }
                Err(err) => sink.failed(path, err),
            }
        }
    }

    fn run(&self, item: &PlanItem, cx: &RunCx<'_>) -> io::Result<Outcome> {
        let Some(path) = &item.source else { return Ok(Outcome::Nothing) };
        let expected = self.items.get(item.root).and_then(|(_, facts)| *facts);
        if !unchanged(path, expected) {
            return Err(changed_since());
        }
        if !cx.has_trash(path) {
            // Looked at now, not at plan time: a file filled since must not be deleted.
            let empty = match std::fs::symlink_metadata(path) {
                Ok(meta) if meta.is_dir() => is_empty_dir(path),
                Ok(meta) => meta.len() == 0,
                Err(err) => return Err(err),
            };
            if self.undoing && empty {
                fs::delete(path)?;
                return Ok(Outcome::Deleted { path: path.clone() });
            }
            return Err(no_trash());
        }
        Ok(match fs::trash(path)? {
            Some(trashed) => Outcome::Trashed { original: path.clone(), trashed },
            None => Outcome::Deleted { path: path.clone() },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::Control;
    use crate::task::is_marker;
    use crate::testing::{defaults, engine, finish, test_dir, write};

    #[test]
    fn trash_takes_files_and_folders() {
        let dir = test_dir("trash");
        write(&dir.join("a.txt"), "a");
        write(&dir.join("f/b.txt"), "b");
        let engine = engine();
        let job = engine.submit(Box::new(TrashTask::new(vec![dir.join("a.txt"), dir.join("f")])));
        let (report, _) = finish(&engine, job, defaults);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert!(!dir.join("a.txt").exists() && !dir.join("f").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn without_a_trash_only_an_empty_undo_item_is_deleted() {
        let dir = test_dir("trash-none");
        write(&dir.join("full.txt"), "x");
        std::fs::create_dir(dir.join("empty")).unwrap();
        let control = Control::default();
        let no_bin = |_: &std::path::Path| false;
        let cx = RunCx { control: &control, trash: &no_bin, added: std::cell::Cell::new(0) };
        let item = |path: PathBuf| {
            let facts = facts_of(&std::fs::symlink_metadata(&path).unwrap());
            PlanItem::new(Stage::Parallel, facts).source(path).top(0)
        };
        let user = TrashTask::new(vec![dir.join("empty")]);
        assert!(is_marker::<crate::NoTrash>(&user.run(&item(dir.join("empty")), &cx).unwrap_err()));
        let undo = TrashTask::checked(vec![(dir.join("empty"), None)]);
        assert!(matches!(undo.run(&item(dir.join("empty")), &cx).unwrap(), Outcome::Deleted { .. }));
        let undo = TrashTask::checked(vec![(dir.join("full.txt"), None)]);
        assert!(is_marker::<crate::NoTrash>(&undo.run(&item(dir.join("full.txt")), &cx).unwrap_err()));
        assert!(dir.join("full.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
