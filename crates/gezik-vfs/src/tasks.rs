//! Copying out of a place to a folder on disk (spec 16 §4.6): one job of two steps, the
//! chosen entries taken out into a staging folder in the target folder, then placed there as
//! any extraction is (conflicts asked, one undo).

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use gezik_core::vfs::{VfsPath, VfsRoot};
use gezik_ops::{Answer, Facts, Outcome, PlanItem, Question, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work};

use crate::Cx;

/// The folder inside the staging folder the entries are written to.
const CONTENT: &str = "x";

/// The tasks that copy `items` (all in one folder of one place) into `dest`, and what Undo
/// says. Submit them as one chain.
pub fn copy_out(items: &[VfsPath], dest: PathBuf) -> (Vec<Box<dyn Task>>, String) {
    let title = match items {
        [one] => gezik_i18n::t!("Copying {}", one.name()),
        many => gezik_i18n::tn!("Copying {n} item" | "Copying {n} items", many.len()),
    };
    let label = match items {
        [one] => gezik_i18n::t!("Copy {}", one.name()),
        many => gezik_i18n::tn!("Copy {n} item" | "Copy {n} items", many.len()),
    };
    let stage = Arc::new(Mutex::new(None));
    let root = items.first().map(|item| item.root.clone());
    let take = root.map(|root| TakeOutTask {
        root,
        items: items.iter().map(|item| item.inner.clone()).collect(),
        dir: items.first().and_then(VfsPath::parent).map(|dir| dir.inner).unwrap_or_default(),
        dest: dest.clone(),
        stage: stage.clone(),
        title: title.clone(),
    });
    let mut tasks: Vec<Box<dyn Task>> = Vec::new();
    if let Some(take) = take {
        tasks.push(Box::new(take));
        tasks.push(gezik_batch::tasks::placing(dest, stage, TaskKind::Copy, title));
    }
    (tasks, label)
}

/// Takes the entries out into a staging folder in the target folder (so placing them is a
/// rename on one drive).
struct TakeOutTask {
    root: VfsRoot,
    items: Vec<String>,
    /// The folder they are in, inside the place.
    dir: String,
    dest: PathBuf,
    /// Set to the staged folder holding them; the placing step reads it.
    stage: Arc<Mutex<Option<PathBuf>>>,
    title: String,
}

impl Task for TakeOutTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Copy
    }

    fn title(&self) -> String {
        self.title.clone()
    }

    fn count(&self) -> usize {
        self.items.len()
    }

    fn resources(&self) -> Resources {
        // The archive's file stands for the place: jobs on it and on the target's drive queue.
        let mut paths = vec![self.dest.clone()];
        paths.extend(VfsPath { root: self.root.clone(), inner: String::new() }.disk().map(Path::to_path_buf));
        Resources { paths, work: Work::Disk }
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        let facts = Facts { is_dir: false, size: 0, modified: None };
        sink.item(PlanItem::new(Stage::Parallel, facts).source(&self.dest).top(0).uncounted());
    }

    fn run(&self, _item: &PlanItem, run: &RunCx<'_>) -> io::Result<Outcome> {
        let cx = Engine { run, place: VfsPath { root: self.root.clone(), inner: String::new() }.text() };
        let vfs = crate::mount(&self.root, &cx)?;
        // Copy To… may name a folder that is not there yet. shortcut: undo leaves it.
        std::fs::create_dir_all(&self.dest)?;
        let staging = run.staging_dir(&self.dest.join(CONTENT))?;
        let content = staging.join(CONTENT);
        std::fs::create_dir(&content)?;
        vfs.stage(&self.items, &content, &cx)?;
        let held = self.dir.split('/').filter(|part| !part.is_empty()).fold(content, |at, part| at.join(part));
        *self.stage.lock().unwrap_or_else(PoisonError::into_inner) = Some(held);
        // What lands is reported by the placing step.
        Ok(Outcome::Nothing)
    }
}

/// Taking out's needs, met by the engine: progress, cancel, the password, failure rows.
struct Engine<'a, 'r> {
    run: &'a RunCx<'r>,
    /// The place's text, for the password question and failure rows.
    place: String,
}

impl Cx for Engine<'_, '_> {
    fn add_bytes(&self, n: u64) {
        self.run.add_bytes(n);
    }

    fn entry_done(&self) {}

    fn stopped(&self) -> bool {
        self.run.stopped()
    }

    fn password(&self, retry: bool) -> Option<String> {
        match self.run.ask(Question::Password { archive: PathBuf::from(&self.place), retry }) {
            Answer::Text(password) => Some(password),
            _ => None,
        }
    }

    fn entry_failed(&self, name: &str, error: &io::Error) {
        self.run.fail(&Path::new(&self.place).join(name), error);
    }

    fn entry_skipped(&self, name: &str, why: &io::Error) {
        self.run.skip(&Path::new(&self.place).join(name), why);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_copy_out_takes_out_then_places_in_one_job() {
        let at = VfsPath::archive("/w/a.zip".into()).join("docs");
        let (tasks, label) = copy_out(&[at.join("x.txt"), at.join("sub")], "/out".into());
        assert_eq!((tasks.len(), label.as_str()), (2, "Copy 2 items"));
        assert_eq!(tasks[0].title(), "Copying 2 items");
        let paths = tasks[0].resources().paths;
        assert_eq!(paths, [PathBuf::from("/out"), PathBuf::from("/w/a.zip")]);
        let (_, one) = copy_out(&[at.join("x.txt")], "/out".into());
        assert_eq!(one, "Copy x.txt");
    }
}
