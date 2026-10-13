//! Places that are no folder on disk (spec 16 §4): inside archives now, phones (16f) and
//! remote places (18) later. A place is opened ("mounted") when a tab goes there and kept only
//! while something holds it: the registry here keeps a weak reference, so the last tab that
//! leaves lets it go (its index, password and temporary files with it).

pub mod archive;
pub mod motw;
pub mod stage;
pub mod tasks;

use std::cell::Cell;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, PoisonError, Weak};

use gezik_core::Entry;
use gezik_core::vfs::{Container, MAX_DEPTH, VfsRoot};

pub use gezik_batch::archive::ExtractCx as Cx;

use self::archive::ArchiveVfs;

pub trait Vfs: Send + Sync {
    fn root(&self) -> &VfsRoot;
    /// What folder `dir` holds (`""`: the top); `None` if it is no folder there.
    fn list(&self, dir: &str) -> Option<Vec<Entry>>;
    /// Everything under folder `dir` with its folder under `dir` (a search, spec 16 §5.5);
    /// `each` returns false to stop.
    fn walk(&self, _dir: &str, _each: &mut dyn FnMut(&str, Entry) -> bool) {}
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

/// Takes the file `path` out to its place in the temporary folder unless it is there already;
/// that place. It is written into a fresh private folder first and moved into place only once
/// whole: a file there is always complete (a cancel, a data error or 7-Zip stopped half way
/// leave nothing). Two threads asking for one file take it out once, the second waiting.
pub fn stage_here(vfs: &dyn Vfs, path: &str, cx: &dyn Cx) -> io::Result<PathBuf> {
    let at = vfs.staged(path);
    let _turn = Claim::take(&at);
    if at.is_file() {
        return Ok(at);
    }
    let fresh = Fresh(stage::new_folder()?);
    stage_into(vfs, &[path.to_owned()], &fresh.0, cx)?;
    let made = under(&fresh.0, path);
    if !made.is_file() {
        return Err(io::ErrorKind::NotFound.into());
    }
    if let Some(folder) = at.parent() {
        std::fs::create_dir_all(folder)?;
    }
    std::fs::rename(&made, &at)?;
    Ok(at)
}

/// Takes `items` out under `dest` (a folder of its own), each at its path inside; an error
/// unless every one of them came out whole (no entry failed or was skipped).
pub fn stage_into(vfs: &dyn Vfs, items: &[String], dest: &Path, cx: &dyn Cx) -> io::Result<()> {
    let watch = Watch { cx, failed: Cell::new(false) };
    vfs.stage(items, dest, &watch)?;
    if watch.failed.get() || !items.iter().all(|item| under(dest, item).exists()) {
        return Err(io::Error::other(gezik_i18n::t!("Some items could not be taken out of the archive")));
    }
    Ok(())
}

/// `inner` (parts split by `/`) under `dest`.
pub fn under(dest: &Path, inner: &str) -> PathBuf {
    inner.split('/').filter(|part| !part.is_empty()).fold(dest.to_path_buf(), |at, part| at.join(part))
}

/// Whether `a` and `b` are one root: the same text, ignoring case where the file system does.
pub fn same_root(a: &VfsRoot, b: &VfsRoot) -> bool {
    a == b || (cfg!(windows) && gezik_core::ops::paths::same_path(Path::new(&a.text()), Path::new(&b.text())))
}

/// A fresh folder, removed with what is left in it.
struct Fresh(PathBuf);

impl Drop for Fresh {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The places being taken out now: one thread at a time per place.
static TAKING: (Mutex<Vec<PathBuf>>, Condvar) = (Mutex::new(Vec::new()), Condvar::new());

struct Claim(PathBuf);

impl Claim {
    fn take(at: &Path) -> Claim {
        let mut taking = TAKING.0.lock().unwrap_or_else(PoisonError::into_inner);
        while taking.iter().any(|p| p == at) {
            taking = TAKING.1.wait(taking).unwrap_or_else(PoisonError::into_inner);
        }
        taking.push(at.to_path_buf());
        Claim(at.to_path_buf())
    }
}

impl Drop for Claim {
    fn drop(&mut self) {
        TAKING.0.lock().unwrap_or_else(PoisonError::into_inner).retain(|p| *p != self.0);
        TAKING.1.notify_all();
    }
}

/// `cx`, noting whether an entry failed or was left out.
struct Watch<'a> {
    cx: &'a dyn Cx,
    failed: Cell<bool>,
}

impl Cx for Watch<'_> {
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
        self.cx.password(retry)
    }
    fn entry_failed(&self, name: &str, error: &io::Error) {
        self.failed.set(true);
        self.cx.entry_failed(name, error);
    }
    fn entry_skipped(&self, name: &str, why: &io::Error) {
        self.failed.set(true);
        self.cx.entry_skipped(name, why);
    }
    fn wants(&self, entry: &gezik_batch::archive::Entry) -> bool {
        self.cx.wants(entry)
    }
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
    open.iter().find(|(r, _)| same_root(r, root)).and_then(|(_, weak)| weak.upgrade())
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
    open.retain(|(r, weak)| weak.strong_count() > 0 && !same_root(r, root));
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

    /// Says nothing, whatever fails.
    struct Calm;

    impl Cx for Calm {
        fn add_bytes(&self, _: u64) {}
        fn entry_done(&self) {}
        fn stopped(&self) -> bool {
            false
        }
        fn password(&self, _: bool) -> Option<String> {
            None
        }
        fn entry_failed(&self, _: &str, _: &io::Error) {}
    }

    /// A place whose `stage` writes half of `x.txt` and then fails (`whole: false`), or writes
    /// it whole, counting the calls.
    struct Flaky {
        root: VfsRoot,
        stage: PathBuf,
        whole: bool,
        calls: std::sync::atomic::AtomicUsize,
    }

    impl Vfs for Flaky {
        fn root(&self) -> &VfsRoot {
            &self.root
        }
        fn list(&self, _: &str) -> Option<Vec<Entry>> {
            None
        }
        fn is_file(&self, _: &str) -> bool {
            true
        }
        fn is_dir(&self, _: &str) -> bool {
            false
        }
        fn packed(&self, _: &str) -> Option<(Option<u64>, bool)> {
            None
        }
        fn stage(&self, items: &[String], dest: &Path, cx: &dyn Cx) -> io::Result<()> {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            std::thread::sleep(std::time::Duration::from_millis(50));
            for item in items {
                std::fs::create_dir_all(under(dest, item).parent().unwrap())?;
                std::fs::write(under(dest, item), if self.whole { "whole" } else { "ha" })?;
                if !self.whole {
                    cx.entry_failed(item, &io::ErrorKind::InvalidData.into());
                }
            }
            if self.whole { Ok(()) } else { Err(io::ErrorKind::InvalidData.into()) }
        }
        fn staged(&self, path: &str) -> PathBuf {
            under(&self.stage, path)
        }
        fn unchanged(&self) -> bool {
            true
        }
        fn len(&self) -> usize {
            1
        }
    }

    fn flaky(name: &str, whole: bool) -> Flaky {
        let root = gezik_core::vfs::VfsPath::archive(format!("/{name}.zip").into()).root;
        Flaky { root, stage: temp(name), whole, calls: Default::default() }
    }

    #[test]
    fn a_staged_file_is_whole_or_absent() {
        let bad = flaky("half", false);
        // A preview of another entry made the folder already: no reason to trust it.
        std::fs::create_dir_all(bad.staged("docs")).unwrap();
        assert!(stage_here(&bad, "docs/x.txt", &Calm).is_err());
        assert!(!bad.staged("docs/x.txt").exists(), "nothing half written is left in place");
        let fresh = stage::new_folder().unwrap();
        assert!(stage_into(&bad, &["docs/x.txt".into()], &fresh, &Calm).is_err(), "a failed entry fails it");
        let good = flaky("whole", true);
        let at = stage_here(&good, "docs/x.txt", &Calm).unwrap();
        assert_eq!(std::fs::read_to_string(at).unwrap(), "whole");
        let _ = std::fs::remove_dir_all(&bad.stage);
        let _ = std::fs::remove_dir_all(&good.stage);
        let _ = std::fs::remove_dir_all(&fresh);
    }

    #[test]
    fn two_threads_take_one_file_out_once() {
        let good = Arc::new(flaky("twice", true));
        let threads: Vec<_> = (0..2)
            .map(|_| {
                let good = good.clone();
                std::thread::spawn(move || stage_here(good.as_ref(), "a/x.txt", &Calm).unwrap())
            })
            .collect();
        for thread in threads {
            assert_eq!(std::fs::read_to_string(thread.join().unwrap()).unwrap(), "whole");
        }
        assert_eq!(good.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        let _ = std::fs::remove_dir_all(&good.stage);
    }

    #[cfg(windows)]
    #[test]
    fn a_root_is_one_root_whatever_its_case() {
        let d = temp("case");
        zip(&d.join("A.zip"), &[("x.txt", b"x")]);
        let upper = gezik_core::vfs::VfsPath::archive(d.join("A.zip")).root;
        let lower = gezik_core::vfs::VfsPath::archive(d.join("a.ZIP")).root;
        let vfs = mount(&upper, &Quiet).unwrap();
        assert!(mounted(&lower).is_some_and(|other| Arc::ptr_eq(&vfs, &other)));
        drop(vfs);
        let _ = std::fs::remove_dir_all(&d);
    }
}
