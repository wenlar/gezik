//! A new, empty folder or file, or a file with given contents (a pasted picture or text).

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use gezik_core::ops::conflict::{Decision, Facts};

use crate::task::{Outcome, PlanItem, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work, facts_after};

/// Makes a new file's bytes, on the job's thread (a pasted picture encoded as PNG). Called
/// again if the item is tried again (a full disk).
type Contents = Box<dyn Fn() -> io::Result<Vec<u8>> + Send + Sync>;

pub struct NewTask {
    dir: PathBuf,
    name: String,
    folder: bool,
    contents: Option<Contents>,
}

impl NewTask {
    fn new(dir: &Path, name: &str, folder: bool) -> NewTask {
        NewTask { dir: dir.to_path_buf(), name: name.to_owned(), folder, contents: None }
    }

    pub fn folder(dir: &Path) -> NewTask {
        NewTask::new(dir, "New folder", true)
    }

    pub fn file(dir: &Path) -> NewTask {
        NewTask::new(dir, "New file.txt", false)
    }

    /// An empty Markdown file (spec 8.1).
    pub fn markdown(dir: &Path) -> NewTask {
        NewTask::new(dir, "New document.md", false)
    }

    /// A file named `name` (a taken name gets a number) with what `contents` makes.
    pub fn with_contents(
        dir: &Path,
        name: &str,
        contents: impl Fn() -> io::Result<Vec<u8>> + Send + Sync + 'static,
    ) -> NewTask {
        NewTask { contents: Some(Box::new(contents)), ..NewTask::new(dir, name, false) }
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
            .target(self.dir.join(&self.name))
            .checked()
            .top(0)
            .preset(Some(Decision::KeepBoth));
        sink.item(item);
    }

    fn run(&self, item: &PlanItem, _cx: &RunCx<'_>) -> io::Result<Outcome> {
        let Some(target) = &item.target else { return Ok(Outcome::Nothing) };
        if self.folder {
            std::fs::create_dir(target)?;
        } else if let Some(contents) = &self.contents {
            // Made first: a picture that cannot be read leaves no file behind.
            let bytes = contents()?;
            let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(target)?;
            if let Err(err) = file.write_all(&bytes) {
                drop(file);
                let _ = std::fs::remove_file(target);
                return Err(err);
            }
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
    fn undo_takes_a_new_folder_only_while_it_holds_no_files() {
        let dir = test_dir("new-folder-undo");
        let engine = engine();
        finish(&engine, engine.submit(Box::new(NewTask::folder(&dir))), defaults);
        finish(&engine, engine.undo().unwrap(), defaults);
        assert!(!dir.join("New folder").exists(), "still empty: it goes");
        finish(&engine, engine.submit(Box::new(NewTask::folder(&dir))), defaults);
        std::fs::write(dir.join("New folder/saved.txt"), "kept").unwrap();
        finish(&engine, engine.undo().unwrap(), defaults);
        assert!(dir.join("New folder/saved.txt").exists(), "a file put in since keeps it");
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

    #[test]
    fn a_markdown_file_is_new_document_and_empty() {
        let dir = test_dir("new-markdown");
        let engine = engine();
        let (report, _) = finish(&engine, engine.submit(Box::new(NewTask::markdown(&dir))), defaults);
        assert_eq!(report.results, [dir.join("New document.md")]);
        assert_eq!(std::fs::metadata(dir.join("New document.md")).unwrap().len(), 0);
        assert_eq!(engine.undo_label().as_deref(), Some("New file"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_with_contents_is_written_whole_and_numbered() {
        let dir = test_dir("new-contents");
        std::fs::write(dir.join("Pasted text.txt"), "taken").unwrap();
        let engine = engine();
        let task = NewTask::with_contents(&dir, "Pasted text.txt", || Ok(b"line 1\r\nline 2".to_vec()));
        let (report, _) = finish(&engine, engine.submit(Box::new(task)), defaults);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(report.results, [dir.join("Pasted text (2).txt")]);
        assert_eq!(std::fs::read(dir.join("Pasted text (2).txt")).unwrap(), b"line 1\r\nline 2");
        assert_eq!(std::fs::read(dir.join("Pasted text.txt")).unwrap(), b"taken");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn contents_that_cannot_be_made_leave_no_file() {
        let dir = test_dir("new-contents-bad");
        let engine = engine();
        let task = NewTask::with_contents(&dir, "x.png", || Err(io::Error::other("not a picture")));
        let (report, _) = finish(&engine, engine.submit(Box::new(task)), defaults);
        assert_eq!(report.failures.len(), 1);
        assert!(!dir.join("x.png").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
