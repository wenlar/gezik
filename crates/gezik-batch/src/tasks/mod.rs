//! The engine tasks of archives: extracting runs as a chain of an `ExtractTask` (unpack into
//! a staging folder) and a `PlaceTask` (move what came out to where it goes) per archive,
//! undone as one action. `CompressTask` makes an archive, `AddToArchiveTask` adds to one.

mod add;
mod compress;
mod external;
mod extract;
mod place;

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use gezik_core::batch::archive::archive_stem;

use gezik_ops::RunCx;

pub use self::add::AddToArchiveTask;
pub use self::compress::{CompressTask, default_name};
use self::extract::ExtractTask;
use self::place::PlaceTask;
pub use crate::archive::write::{CompressOptions, Level, OutFormat};

/// Where the extracted files go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtractTo {
    /// Into `dir`: a single top-level item goes in as it is, several go into `dir/<stem>/`.
    Smart(PathBuf),
    /// Into `dir/<stem>/` always.
    Folder(PathBuf),
    /// Into `dir` as it is.
    Into(PathBuf),
}

impl ExtractTo {
    /// The folder it goes into (or makes its folder in).
    pub fn dir(&self) -> &Path {
        match self {
            ExtractTo::Smart(dir) | ExtractTo::Folder(dir) | ExtractTo::Into(dir) => dir,
        }
    }
}

/// The tasks of one "Extract": unpack each archive into a staging folder, then place it.
/// Submit them with `Engine::submit_chain(tasks, Some(extract_label(&archives)))`.
pub fn extract_chain(
    archives: Vec<PathBuf>,
    to: ExtractTo,
    seven_zip: Option<PathBuf>,
) -> Vec<Box<dyn gezik_ops::Task>> {
    let title = match archives.as_slice() {
        [one] => format!("Extracting {}", file_name(one)),
        many => format!("Extracting {} archives", many.len()),
    };
    let mut tasks: Vec<Box<dyn gezik_ops::Task>> = Vec::new();
    for archive in archives {
        // Where the archive's staging folder is, once it is unpacked.
        let stage = Arc::new(Mutex::new(None));
        let title = if tasks.is_empty() { title.clone() } else { format!("Extracting {}", file_name(&archive)) };
        tasks.push(Box::new(ExtractTask::new(
            archive.clone(),
            to.dir().to_path_buf(),
            seven_zip.clone(),
            stage.clone(),
            title,
        )));
        tasks.push(Box::new(PlaceTask::new(archive, to.clone(), stage)));
    }
    tasks
}

/// What Undo says for extracting `archives`: "Extract a.zip", "Extract 3 archives".
pub fn extract_label(archives: &[PathBuf]) -> String {
    match archives {
        [one] => format!("Extract {}", file_name(one)),
        many => format!("Extract {} archives", many.len()),
    }
}

/// The archive's format is one only 7-Zip reads, and 7-Zip is not there.
#[derive(Debug)]
pub struct SevenZipNeeded;

impl fmt::Display for SevenZipNeeded {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "7-Zip needed to open this kind of archive")
    }
}

impl std::error::Error for SevenZipNeeded {}

fn seven_zip_needed() -> io::Error {
    io::Error::other(SevenZipNeeded)
}

/// Whether `err` says 7-Zip is needed (the UI then offers to download it).
pub fn is_seven_zip_needed(err: &io::Error) -> bool {
    err.get_ref().is_some_and(|inner| inner.is::<SevenZipNeeded>())
}

/// The name of the folder an archive goes into (`a` for `a.tar.gz`).
fn stem_of(archive: &Path) -> String {
    let stem = archive_stem(&file_name(archive)).to_owned();
    if stem.is_empty() { "Archive".to_owned() } else { stem }
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string())
}

fn cancelled() -> io::Error {
    io::Error::new(io::ErrorKind::Interrupted, "cancelled")
}

/// `rapor.pdf`, or `3 items`.
fn what(paths: &[PathBuf]) -> String {
    match paths {
        [one] => file_name(one),
        many => format!("{} items", many.len()),
    }
}

/// How many threads compress zip entries: one per core.
fn workers() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get())
}

/// Writing's needs, met by the engine: progress, cancel, failure rows (kept, to see what
/// was left out).
struct Writing<'a, 'r> {
    run: &'a RunCx<'r>,
    failed: std::cell::RefCell<Vec<PathBuf>>,
}

impl<'a, 'r> Writing<'a, 'r> {
    fn new(run: &'a RunCx<'r>) -> Self {
        Writing { run, failed: Default::default() }
    }
}

impl crate::archive::write::WriteCx for Writing<'_, '_> {
    fn add_bytes(&self, n: u64) {
        self.run.add_bytes(n);
    }

    fn entry_done(&self) {
        self.run.one_done(0);
    }

    fn entry_failed(&self, path: &Path, error: &io::Error) {
        self.failed.borrow_mut().push(path.to_path_buf());
        self.run.fail(path, error);
        self.run.one_done(0);
    }

    fn stopped(&self) -> bool {
        self.run.stopped()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_and_stems() {
        assert_eq!(extract_label(&[PathBuf::from("a.zip")]), "Extract a.zip");
        assert_eq!(extract_label(&[PathBuf::from("a.zip"), "b.7z".into(), "c.rar".into()]), "Extract 3 archives");
        assert_eq!(stem_of(Path::new("x/a.tar.gz")), "a");
        assert_eq!(stem_of(Path::new(".zip")), "Archive");
        assert_eq!(extract_chain(vec!["a.zip".into(), "b.zip".into()], ExtractTo::Into("d".into()), None).len(), 4);
    }

    #[test]
    fn seven_zip_needed_is_recognized() {
        assert!(is_seven_zip_needed(&seven_zip_needed()));
        assert_eq!(seven_zip_needed().to_string(), "7-Zip needed to open this kind of archive");
        assert!(!is_seven_zip_needed(&io::Error::new(io::ErrorKind::Unsupported, "7-Zip needed")));
    }
}
