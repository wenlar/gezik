//! Places that are no folder on disk (spec 16 §4): inside archives now, phones (16f) and
//! remote places (18) later. A place is opened ("mounted") when a tab goes there and kept only
//! while something holds it: the registry here keeps a weak reference, so the last tab that
//! leaves lets it go (its index, password and temporary files with it).

pub mod archive;
pub mod stage;
pub mod tasks;

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError, Weak};

use gezik_core::Entry;
use gezik_core::vfs::{Container, MAX_DEPTH, VfsRoot};

pub use gezik_batch::archive::ExtractCx as Cx;

use self::archive::ArchiveVfs;

pub trait Vfs: Send + Sync {
    fn root(&self) -> &VfsRoot;
    /// What folder `dir` holds (`""`: the top); `None` if it is no folder there.
    fn list(&self, dir: &str) -> Option<Vec<Entry>>;
    fn is_file(&self, path: &str) -> bool;
    fn is_dir(&self, path: &str) -> bool;
    /// The bytes `path` takes packed (if known) and whether it is encrypted.
    fn packed(&self, path: &str) -> Option<(Option<u64>, bool)>;
    /// Writes `items` (folders with what they hold) under `dest`, each at its path inside.
    fn stage(&self, items: &[String], dest: &Path, cx: &dyn Cx) -> io::Result<()>;
    /// Where `path` is once taken out to its own temporary folder (`stage_here`).
    fn staged(&self, path: &str) -> PathBuf;
    /// Whether what it was read from is still as it was.
    fn unchanged(&self) -> bool;
    /// How many entries it has.
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl std::fmt::Debug for dyn Vfs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Vfs({})", self.root().text())
    }
}

/// Takes `path` out to its temporary folder unless it is there already; its place there.
pub fn stage_here(vfs: &dyn Vfs, path: &str, cx: &dyn Cx) -> io::Result<PathBuf> {
    let at = vfs.staged(path);
    if !at.exists() {
        let top = vfs.staged("");
        vfs.stage(&[path.to_owned()], &top, cx)?;
        if !at.exists() {
            return Err(io::ErrorKind::NotFound.into());
        }
    }
    Ok(at)
}

/// The places open now, by root.
static OPEN: Mutex<Vec<(VfsRoot, Weak<dyn Vfs>)>> = Mutex::new(Vec::new());

/// Where 7-Zip is looked for (the folder of Gezik's downloads, the path in settings).
static SEVEN_ZIP: Mutex<Option<(PathBuf, Option<PathBuf>)>> = Mutex::new(None);

/// Tells where to look for 7-Zip (set at start and when settings change).
pub fn set_seven_zip(tools_dir: PathBuf, configured: Option<PathBuf>) {
    *SEVEN_ZIP.lock().unwrap_or_else(PoisonError::into_inner) = Some((tools_dir, configured));
}

fn find_seven_zip() -> Option<PathBuf> {
    let (dir, configured) = SEVEN_ZIP.lock().unwrap_or_else(PoisonError::into_inner).clone()?;
    gezik_batch::tools::find(gezik_core::batch::tools::Tool::SevenZip, &dir, configured.as_deref())
}

/// The place at `root` if it is open now.
pub fn mounted(root: &VfsRoot) -> Option<Arc<dyn Vfs>> {
    let open = OPEN.lock().unwrap_or_else(PoisonError::into_inner);
    open.iter().find(|(r, _)| r == root).and_then(|(_, weak)| weak.upgrade())
}

/// The place at `root`, opened if it is not open (or its archive changed since); an archive
/// in an archive opens the outer one first. Runs off the UI thread (it reads the archive).
pub fn mount(root: &VfsRoot, cx: &dyn Cx) -> io::Result<Arc<dyn Vfs>> {
    if let Some(open) = mounted(root).filter(|vfs| vfs.unchanged()) {
        return Ok(open);
    }
    let vfs: Arc<dyn Vfs> = match root {
        VfsRoot::Archive(Container::Disk(file)) => {
            Arc::new(ArchiveVfs::open(root.clone(), file.clone(), None, cx, &find_seven_zip)?)
        }
        VfsRoot::Archive(Container::Inside(at)) => {
            if at.depth() >= MAX_DEPTH {
                return Err(io::Error::other(gezik_i18n::t!(
                    "Archives nested more than {} deep are not opened",
                    MAX_DEPTH
                )));
            }
            let outer = mount(&at.root, cx)?;
            if !outer.is_file(&at.inner) {
                return Err(io::ErrorKind::NotFound.into());
            }
            let file = stage_here(outer.as_ref(), &at.inner, cx)?;
            Arc::new(ArchiveVfs::open(root.clone(), file, Some(outer), cx, &find_seven_zip)?)
        }
        // Phones come with 16f.
        VfsRoot::Device { .. } => return Err(io::ErrorKind::Unsupported.into()),
    };
    let mut open = OPEN.lock().unwrap_or_else(PoisonError::into_inner);
    open.retain(|(r, weak)| weak.strong_count() > 0 && r != root);
    open.push((root.clone(), Arc::downgrade(&vfs)));
    Ok(vfs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    pub(crate) struct Quiet;

    impl Cx for Quiet {
        fn add_bytes(&self, _: u64) {}
        fn entry_done(&self) {}
        fn stopped(&self) -> bool {
            false
        }
        fn password(&self, _: bool) -> Option<String> {
            None
        }
        fn entry_failed(&self, name: &str, error: &io::Error) {
            panic!("{name}: {error}");
        }
    }

    pub(crate) fn zip(path: &Path, files: &[(&str, &[u8])]) {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        for (name, data) in files {
            zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap();
    }

    pub(crate) fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("gezik-vfs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_place_is_let_go_of_with_its_last_holder_and_an_inner_archive_opens() {
        let d = temp("mount");
        let inner = d.join("inner.zip");
        zip(&inner, &[("deep/x.txt", b"x")]);
        zip(&d.join("outer.zip"), &[("a/inner.zip", &std::fs::read(&inner).unwrap()), ("b.txt", b"bb")]);
        let root = gezik_core::vfs::VfsPath::archive(d.join("outer.zip")).root;
        let vfs = mount(&root, &Quiet).unwrap();
        assert_eq!(vfs.list("").unwrap().len(), 2);
        assert!(Arc::ptr_eq(&vfs, &mount(&root, &Quiet).unwrap()), "open once while held");
        let at = gezik_core::vfs::VfsPath::archive(d.join("outer.zip")).join("a/inner.zip");
        let nested = mount(&gezik_core::vfs::VfsPath::inside(at).root, &Quiet).unwrap();
        assert_eq!(nested.list("deep").unwrap()[0].name, "x.txt");
        let staged = stage_here(nested.as_ref(), "deep/x.txt", &Quiet).unwrap();
        assert_eq!(std::fs::read(&staged).unwrap(), b"x");
        let folder = vfs.staged("");
        drop(vfs);
        assert!(mounted(&root).is_some(), "the inner one holds the outer one");
        drop(nested);
        assert!(mounted(&root).is_none(), "the last holder let go of both");
        assert!(!folder.exists() && !staged.exists(), "their temporary files went with them");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_changed_archive_is_read_again() {
        let d = temp("changed");
        let file = d.join("a.zip");
        zip(&file, &[("one.txt", b"1")]);
        let root = gezik_core::vfs::VfsPath::archive(file.clone()).root;
        let first = mount(&root, &Quiet).unwrap();
        zip(&file, &[("one.txt", b"1"), ("two.txt", b"22")]);
        let second = mount(&root, &Quiet).unwrap();
        assert_eq!((first.len(), second.len()), (1, 2));
        drop((first, second));
        let _ = std::fs::remove_dir_all(&d);
    }
}
