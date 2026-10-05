//! Placing what an archive unpacked: decides where it goes (as it is, or in a folder of its
//! own), then moves it there through `MoveTask::placing`, so conflicts are asked like any
//! move's and undo trashes what landed.

use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use gezik_core::batch::archive::single_root;
use gezik_ops::{MoveTask, Outcome, PlanItem, Resources, RunCx, ScanSink, Task, TaskKind, Work};

use super::{ExtractTo, file_name, stem_of};

pub(super) struct PlaceTask {
    archive: PathBuf,
    to: ExtractTo,
    /// The unpacked contents, set by the `ExtractTask` before it (none: it did not finish).
    stage: Arc<Mutex<Option<PathBuf>>>,
    /// The move, made when planning (the contents are known only then).
    moves: OnceLock<MoveTask>,
}

impl PlaceTask {
    pub fn new(archive: PathBuf, to: ExtractTo, stage: Arc<Mutex<Option<PathBuf>>>) -> PlaceTask {
        PlaceTask { archive, to, stage, moves: OnceLock::new() }
    }

    /// (where it is, where it goes) for what is in `content`.
    fn pairs(&self, content: PathBuf, sink: &mut dyn ScanSink) -> Vec<(PathBuf, PathBuf)> {
        let mut roots = Vec::new();
        match std::fs::read_dir(&content) {
            Ok(entries) => {
                for entry in entries {
                    match entry {
                        Ok(entry) => {
                            let is_dir = entry.file_type().is_ok_and(|kind| kind.is_dir());
                            roots.push((entry.file_name(), is_dir));
                        }
                        Err(err) => sink.failed(&content, err),
                    }
                }
            }
            Err(err) => {
                sink.failed(&content, err);
                return Vec::new();
            }
        }
        if roots.is_empty() {
            return Vec::new();
        }
        let names: Vec<(String, bool)> =
            roots.iter().map(|(name, is_dir)| (name.to_string_lossy().into_owned(), *is_dir)).collect();
        let each_into = |dir: &std::path::Path| -> Vec<(PathBuf, PathBuf)> {
            roots.iter().map(|(name, _)| (content.join(name), dir.join(name))).collect()
        };
        match &self.to {
            ExtractTo::Smart(dir) if single_root(&names) => each_into(dir),
            // The whole folder becomes `dir/<stem>`: made in one move, or merged into one there.
            ExtractTo::Smart(dir) | ExtractTo::Folder(dir) => vec![(content.clone(), dir.join(stem_of(&self.archive)))],
            ExtractTo::Into(dir) => each_into(dir),
        }
    }
}

impl Task for PlaceTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Extract
    }

    fn title(&self) -> String {
        format!("Extracting {}", file_name(&self.archive))
    }

    fn count(&self) -> usize {
        1
    }

    fn resources(&self) -> Resources {
        Resources { paths: vec![self.to.dir().to_path_buf()], work: Work::Disk }
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        let content = self.stage.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let Some(content) = content else { return };
        let pairs = self.pairs(content, sink);
        if pairs.is_empty() {
            return;
        }
        let moves = self.moves.get_or_init(|| MoveTask::placing(pairs, TaskKind::Extract));
        moves.plan(sink);
    }

    fn run(&self, item: &PlanItem, cx: &RunCx<'_>) -> io::Result<Outcome> {
        match self.moves.get() {
            Some(moves) => moves.run(item, cx),
            None => Ok(Outcome::Nothing),
        }
    }
}
