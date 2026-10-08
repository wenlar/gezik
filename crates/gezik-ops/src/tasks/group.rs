//! New folder with selection (spec 8.2): a new folder next to the items and the items moved
//! into it, as one job and one undo.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use gezik_core::ops::conflict::{Decision, Facts};
use gezik_platform::fs;

use super::what;
use crate::task::{Outcome, PlanItem, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work, facts_after};
use crate::walk::facts_of;

/// The new folder's name; a taken one becomes "New folder (2)".
const FOLDER_NAME: &str = "New folder";
const MAKE: u8 = 0;
const MOVE: u8 = 1;

pub struct GroupTask {
    dir: PathBuf,
    items: Vec<PathBuf>,
    /// Set when making the folder failed: nothing is sent into it.
    make_failed: AtomicBool,
}

impl GroupTask {
    /// Moves `items` (in `dir`) into a new folder in `dir`.
    pub fn new(items: Vec<PathBuf>, dir: &Path) -> GroupTask {
        GroupTask { dir: dir.to_path_buf(), items, make_failed: AtomicBool::new(false) }
    }
}

impl Task for GroupTask {
    fn kind(&self) -> TaskKind {
        TaskKind::NewFolderWith
    }

    fn title(&self) -> String {
        format!("Moving {} into a new folder", what(&self.items))
    }

    fn count(&self) -> usize {
        self.items.len()
    }

    fn resources(&self) -> Resources {
        Resources { paths: vec![self.dir.clone()], work: Work::Disk }
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        let folder = self.dir.join(FOLDER_NAME);
        // Made as it is planned (a Before item), before anything goes in. A taken name gets a
        // number, and the items' targets follow it (the engine's renames). Only the folder is
        // a result: it is selected and renamed afterwards.
        let make = PlanItem::new(Stage::Before, Facts { is_dir: true, ..Facts::default() })
            .target(&folder)
            .checked()
            .top(0)
            .preset(Some(Decision::KeepBoth))
            .tag(MAKE);
        if !sink.item(make) {
            return;
        }
        // The folder is made by now (a Before item runs as it is planned). Without it every
        // move would fail with a misleading "not found": the one real failure is enough.
        if self.make_failed.load(Ordering::Relaxed) {
            return;
        }
        for item in &self.items {
            if super::refuse_root(sink, item, "move") {
                continue;
            }
            let meta = match std::fs::symlink_metadata(item) {
                Ok(meta) => meta,
                Err(err) => {
                    sink.failed(item, err);
                    continue;
                }
            };
            let Some(name) = item.file_name() else { continue };
            let planned = PlanItem::new(Stage::Parallel, facts_of(&meta))
                .source(item)
                .target(folder.join(name))
                .under(0)
                .tag(MOVE);
            if !sink.item(planned) {
                return;
            }
        }
    }

    fn run(&self, item: &PlanItem, _cx: &RunCx<'_>) -> io::Result<Outcome> {
        let Some(target) = &item.target else { return Ok(Outcome::Nothing) };
        match (item.tag, &item.source) {
            (MAKE, _) => {
                if let Err(err) = std::fs::create_dir(target) {
                    self.make_failed.store(true, Ordering::Relaxed);
                    return Err(err);
                }
                Ok(Outcome::Created { path: target.clone(), facts: facts_after(target, true), from: None })
            }
            // Same folder, same drive: one rename each.
            (_, Some(source)) => {
                fs::move_entry(source, target)?;
                let facts = facts_after(target, item.facts.is_dir);
                Ok(Outcome::Moved { from: source.clone(), to: target.clone(), facts })
            }
            _ => Ok(Outcome::Nothing),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{defaults, engine, finish, read, test_dir, write};

    #[test]
    fn the_items_go_into_a_new_folder_and_one_undo_brings_them_back() {
        let dir = test_dir("group");
        write(&dir.join("a.txt"), "a");
        write(&dir.join("b/c.txt"), "c");
        let engine = engine();
        let job = engine.submit(Box::new(GroupTask::new(vec![dir.join("a.txt"), dir.join("b")], &dir)));
        let (report, _) = finish(&engine, job, defaults);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        let folder = dir.join("New folder");
        assert_eq!(report.results, std::slice::from_ref(&folder), "the folder is selected, not what went in");
        assert_eq!(read(&folder.join("a.txt")), "a");
        assert_eq!(read(&folder.join("b/c.txt")), "c");
        assert_eq!(engine.undo_label().as_deref(), Some("New folder with 2 items"));
        let (undo, _) = finish(&engine, engine.undo().unwrap(), defaults);
        assert!(undo.failures.is_empty() && undo.skipped_changed == 0, "{:?}", undo.failures);
        assert_eq!(read(&dir.join("a.txt")), "a");
        assert_eq!(read(&dir.join("b/c.txt")), "c");
        assert!(!folder.exists(), "the emptied folder went to the trash");
        let (redo, _) = finish(&engine, engine.redo().unwrap(), defaults);
        assert!(redo.failures.is_empty(), "{:?}", redo.failures);
        assert_eq!(read(&folder.join("a.txt")), "a");
        assert_eq!(read(&folder.join("b/c.txt")), "c");
        assert!(!dir.join("a.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_folder_that_cannot_be_made_fails_once_and_moves_nothing() {
        let dir = test_dir("group-nomake");
        write(&dir.join("a.txt"), "a");
        write(&dir.join("b.txt"), "b");
        let engine = engine();
        let missing = dir.join("gone");
        let items = vec![dir.join("a.txt"), dir.join("b.txt")];
        let (report, _) = finish(&engine, engine.submit(Box::new(GroupTask::new(items, &missing))), defaults);
        assert_eq!(report.failures.len(), 1, "{:?}", report.failures);
        assert_eq!(read(&dir.join("a.txt")), "a");
        assert_eq!(read(&dir.join("b.txt")), "b");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_selected_item_named_new_folder_goes_inside_the_numbered_one() {
        let dir = test_dir("group-named");
        write(&dir.join("New folder/x.txt"), "x");
        write(&dir.join("y.txt"), "y");
        let engine = engine();
        let items = vec![dir.join("New folder"), dir.join("y.txt")];
        let (report, _) = finish(&engine, engine.submit(Box::new(GroupTask::new(items, &dir))), defaults);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        let folder = dir.join("New folder (2)");
        assert_eq!(report.results, std::slice::from_ref(&folder));
        assert_eq!(read(&folder.join("New folder/x.txt")), "x");
        assert_eq!(read(&folder.join("y.txt")), "y");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_item_changed_since_keeps_the_folder_out_of_the_trash() {
        let dir = test_dir("group-changed");
        write(&dir.join("a.txt"), "a");
        write(&dir.join("b.txt"), "b");
        let engine = engine();
        let items = vec![dir.join("a.txt"), dir.join("b.txt")];
        finish(&engine, engine.submit(Box::new(GroupTask::new(items, &dir))), defaults);
        let folder = dir.join("New folder");
        std::fs::write(folder.join("b.txt"), "edited after the move").unwrap();
        let (undo, _) = finish(&engine, engine.undo().unwrap(), defaults);
        assert!(undo.failures.is_empty(), "{:?}", undo.failures);
        assert_eq!(undo.skipped_changed, 2, "the edited file and the folder holding it stay");
        assert_eq!(read(&dir.join("a.txt")), "a");
        assert_eq!(read(&folder.join("b.txt")), "edited after the move");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
