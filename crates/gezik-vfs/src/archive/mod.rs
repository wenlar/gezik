//! An archive as a place (spec 16 §5): its index read once when it is opened, entries taken
//! out on demand by Gezik's own readers or by 7-Zip. The password, once given, is kept while
//! it is open (never written anywhere).

pub mod index;
pub mod sevenzip_list;

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::SystemTime;

use gezik_batch::archive::{self, Entry as Listed, ExtractCx};
use gezik_core::Entry;
use gezik_core::vfs::VfsRoot;

use self::index::Index;
use crate::Vfs;

pub struct ArchiveVfs {
    root: VfsRoot,
    /// The archive's file: on disk, or taken out of the archive it is in.
    file: PathBuf,
    /// Its size and change time when it was read, to see it changed.
    seen: (u64, Option<SystemTime>),
    index: Index,
    /// Set when it is read through 7-Zip.
    seven_zip: Option<PathBuf>,
    password: Mutex<Option<String>>,
    /// Where its entries are taken out to.
    stage: PathBuf,
    /// The archive it is in: kept open while this one is (its file is in that one's folder).
    _outer: Option<Arc<dyn Vfs>>,
}

impl ArchiveVfs {
    /// Reads the archive at `file` (Gezik's readers, else 7-Zip if `seven_zip` gives one).
    pub fn open(
        root: VfsRoot,
        file: PathBuf,
        outer: Option<Arc<dyn Vfs>>,
        cx: &dyn ExtractCx,
        seven_zip: &dyn Fn() -> Option<PathBuf>,
    ) -> io::Result<ArchiveVfs> {
        let seen = facts(&file)?;
        let password = Keeper { cx, password: Mutex::new(None) };
        // Formats, or methods in a known format, that only 7-Zip reads: found before anything is listed.
        let own = archive::open(&file).and_then(|mut source| archive::list_all(source.as_mut(), &password));
        let (entries, seven_zip) = match own {
            Ok(entries) => (entries, None),
            Err(err) if err.kind() == io::ErrorKind::Unsupported => {
                let program = seven_zip().ok_or_else(seven_zip_needed)?;
                let mut kept = password.password.lock().unwrap_or_else(PoisonError::into_inner).take();
                let entries = sevenzip_list::list(&program, &file, cx, &mut kept)?;
                *password.password.lock().unwrap_or_else(PoisonError::into_inner) = kept;
                (entries, Some(program))
            }
            Err(err) => return Err(err),
        };
        let index = Index::build(entries);
        let password = password.password.into_inner().unwrap_or_else(PoisonError::into_inner);
        Ok(ArchiveVfs {
            root,
            file,
            seen,
            index,
            seven_zip,
            password: Mutex::new(password),
            stage: crate::stage::new_folder(),
            _outer: outer,
        })
    }

    /// Whether its file is still as it was read.
    pub fn unchanged(&self) -> bool {
        facts(&self.file).is_ok_and(|now| now == self.seen)
    }
}

impl Vfs for ArchiveVfs {
    fn root(&self) -> &VfsRoot {
        &self.root
    }

    fn list(&self, dir: &str) -> Option<Vec<Entry>> {
        self.index.list(dir)
    }

    fn is_file(&self, path: &str) -> bool {
        self.index.is_file(path)
    }

    fn is_dir(&self, path: &str) -> bool {
        self.index.is_dir(path)
    }

    fn packed(&self, path: &str) -> Option<(Option<u64>, bool)> {
        self.index.packed(path)
    }

    fn stage(&self, items: &[String], dest: &Path, cx: &dyn ExtractCx) -> io::Result<()> {
        std::fs::create_dir_all(dest)?;
        let mut kept = self.password.lock().unwrap_or_else(PoisonError::into_inner).clone();
        let result = match &self.seven_zip {
            Some(program) => sevenzip_list::extract_some(program, &self.file, items, dest, cx, &mut kept),
            None => {
                let keeper = Keeper { cx, password: Mutex::new(kept.clone()) };
                let result = archive::open(&self.file).and_then(|mut source| source.extract_some(items, dest, &keeper));
                kept = keeper.password.into_inner().unwrap_or_else(PoisonError::into_inner);
                result
            }
        };
        if kept.is_some() {
            *self.password.lock().unwrap_or_else(PoisonError::into_inner) = kept;
        }
        result
    }

    fn staged(&self, path: &str) -> PathBuf {
        path.split('/').filter(|part| !part.is_empty()).fold(self.stage.clone(), |at, part| at.join(part))
    }

    fn unchanged(&self) -> bool {
        ArchiveVfs::unchanged(self)
    }

    fn len(&self) -> usize {
        self.index.len()
    }
}

impl Drop for ArchiveVfs {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.stage);
    }
}

fn facts(file: &Path) -> io::Result<(u64, Option<SystemTime>)> {
    let meta = std::fs::metadata(file)?;
    if meta.is_dir() {
        return Err(io::ErrorKind::IsADirectory.into());
    }
    Ok((meta.len(), meta.modified().ok()))
}

/// The error that offers 7-Zip (as extracting says it).
fn seven_zip_needed() -> io::Error {
    io::Error::other(gezik_batch::tasks::SevenZipNeeded)
}

/// `cx` asking for the password only when the one kept is missing or wrong, keeping the
/// answer.
struct Keeper<'a> {
    cx: &'a dyn ExtractCx,
    password: Mutex<Option<String>>,
}

impl ExtractCx for Keeper<'_> {
    fn add_bytes(&self, n: u64) {
        self.cx.add_bytes(n);
    }
    fn entry_done(&self) {
        self.cx.entry_done();
    }
    fn stopped(&self) -> bool {
        self.cx.stopped()
    }
    fn password(&self, retry: bool) -> Option<String> {
        let mut kept = self.password.lock().unwrap_or_else(PoisonError::into_inner);
        if !retry && let Some(password) = kept.as_ref() {
            return Some(password.clone());
        }
        let answer = self.cx.password(retry && kept.is_some());
        if answer.is_some() {
            kept.clone_from(&answer);
        }
        answer
    }
    fn entry_failed(&self, name: &str, error: &io::Error) {
        self.cx.entry_failed(name, error);
    }
    fn entry_skipped(&self, name: &str, why: &io::Error) {
        self.cx.entry_skipped(name, why);
    }
    fn wants(&self, entry: &Listed) -> bool {
        self.cx.wants(entry)
    }
}
