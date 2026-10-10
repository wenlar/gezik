//! macOS: files another program promises (Mail's attachments, Photos, Safari's pictures).
//! At the drop `NSFilePromiseReceiver` has each source write its files into a private folder
//! under `$TMPDIR`, on a background queue; the job waits for them there, then takes them into
//! the drop folder. The folder goes when the job ends (spec 9 §8.1).

use std::cell::OnceCell;
use std::ffi::{CStr, CString, OsStr};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::ptr::NonNull;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use block2::RcBlock;
use objc2::ClassType;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{NSDraggingInfo, NSFilePromiseReceiver};
use objc2_foundation::{NSArray, NSDictionary, NSError, NSOperationQueue, NSString, NSURL};

use crate::dnd::{VirtualEntry, VirtualFiles, VirtualSource};

/// The type a `.webloc` is promised as: a link, not a file (spec 9 §17 decision 22).
const LINK_TYPE: &str = "com.apple.web-internet-location";

/// The most items one drop lists (a promised folder is walked).
const MOST: usize = 10_000;

thread_local! {
    /// The queue the sources write on; made at the first promised drop.
    static QUEUE: OnceCell<Retained<NSOperationQueue>> = const { OnceCell::new() };
}

/// The pasteboard types promised files come by.
pub(super) fn types() -> Retained<NSArray<NSString>> {
    NSFilePromiseReceiver::readableDraggedTypes()
}

/// Whether a promise names only links.
pub(super) fn is_link_only(types: &[String]) -> bool {
    !types.is_empty() && types.iter().all(|t| t == LINK_TYPE)
}

/// The drag's promises that are files.
fn receivers(info: &ProtocolObject<dyn NSDraggingInfo>) -> Vec<Retained<NSFilePromiseReceiver>> {
    let classes = NSArray::from_slice(&[NSFilePromiseReceiver::class()]);
    let Some(objects) = (unsafe { info.draggingPasteboard().readObjectsForClasses_options(&classes, None) }) else {
        return Vec::new();
    };
    objects
        .iter()
        .filter_map(|object| object.downcast::<NSFilePromiseReceiver>().ok())
        .filter(|receiver| {
            let types: Vec<String> = receiver.fileTypes().iter().map(|t| t.to_string()).collect();
            !is_link_only(&types)
        })
        .collect()
}

/// How many files `receivers` promise (one that names none counts as one).
fn expected(receivers: &[Retained<NSFilePromiseReceiver>]) -> usize {
    receivers.iter().map(|receiver| receiver.fileNames().count().max(1)).sum()
}

/// How many files the drag promises.
pub(super) fn count(info: &ProtocolObject<dyn NSDraggingInfo>) -> usize {
    expected(&receivers(info))
}

/// What the sources wrote, as they report it (on the queue).
#[derive(Default)]
struct Arrivals {
    got: Mutex<Vec<Result<PathBuf, String>>>,
    ready: Condvar,
}

struct Promised {
    /// The private folder, as the system names the open folder (`/private/var/…`).
    dir: PathBuf,
    expected: usize,
    arrivals: Arc<Arrivals>,
    /// Where each of `entries`' items is, once listed.
    staged: Mutex<Vec<PathBuf>>,
}

/// Asks the sources to write their files: on the main thread, at the drop. Returns at once;
/// the sources write on the queue.
pub(super) fn receive(info: &ProtocolObject<dyn NSDraggingInfo>) -> Option<VirtualFiles> {
    let receivers = receivers(info);
    if receivers.is_empty() {
        return None;
    }
    let dir = private_dir().ok()?;
    let url = NSURL::fileURLWithPath_isDirectory(&NSString::from_str(dir.to_str()?), true);
    let arrivals = Arc::new(Arrivals::default());
    let queue = QUEUE.with(|q| q.get_or_init(NSOperationQueue::new).clone());
    for receiver in &receivers {
        let arrivals = arrivals.clone();
        let reader = RcBlock::new(move |file: NonNull<NSURL>, error: *mut NSError| {
            let got = match unsafe { error.as_ref() } {
                Some(error) => Err(error.localizedDescription().to_string()),
                None => unsafe { file.as_ref() }
                    .path()
                    .map(|path| PathBuf::from(path.to_string()))
                    .ok_or_else(|| "The program gave no file".to_owned()),
            };
            if let Ok(mut all) = arrivals.got.lock() {
                all.push(got);
            }
            arrivals.ready.notify_all();
        });
        unsafe {
            receiver.receivePromisedFilesAtDestination_options_operationQueue_reader(
                &url,
                &NSDictionary::new(),
                &queue,
                &reader,
            );
        }
    }
    Some(VirtualFiles(Arc::new(Promised { dir, expected: expected(&receivers), arrivals, staged: Mutex::default() })))
}

/// A new folder only this user can open: `$TMPDIR/gezik-drop-<pid>-<nanos>-<n>`. Never one
/// that was there (`mkdir` fails on any existing name, a link included).
fn private_dir() -> io::Result<PathBuf> {
    use std::os::unix::fs::DirBuilderExt;
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos());
    for n in 0..100u32 {
        let dir = std::env::temp_dir().join(format!("gezik-drop-{}-{stamp}-{n}", std::process::id()));
        match std::fs::DirBuilder::new().mode(0o700).create(&dir) {
            // Named as the open folder is: the name `write` checks files against.
            Ok(()) => return open_path(&dir, libc::O_DIRECTORY).ok_or_else(|| io::Error::other("lost the folder")),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {}
            Err(err) => return Err(err),
        }
    }
    Err(io::Error::from(io::ErrorKind::AlreadyExists))
}

/// Opens `path` without following a link at its end (and without waiting on a pipe).
fn open(path: &Path, flags: i32) -> io::Result<File> {
    OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | flags).open(path)
}

/// Where the open `file` really is, links in its folders resolved.
fn real_path(file: &File) -> Option<PathBuf> {
    let mut buffer = [0u8; libc::PATH_MAX as usize];
    if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETPATH, buffer.as_mut_ptr()) } != 0 {
        return None;
    }
    let path = CStr::from_bytes_until_nul(&buffer).ok()?;
    Some(PathBuf::from(OsStr::from_bytes(path.to_bytes())))
}

fn open_path(path: &Path, flags: i32) -> Option<PathBuf> {
    real_path(&open(path, flags).ok()?)
}

/// A lock a panicking reader left poisoned (any guard type).
fn lost<T>(_: T) -> io::Error {
    io::Error::other("A promised file's report was lost")
}

fn refused(why: &str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, why)
}

fn failed(name: String, why: &str) -> VirtualEntry {
    VirtualEntry { name, is_dir: false, size: None, failed: Some(why.to_owned()) }
}

impl Promised {
    /// Lists what arrived at `top` (and in it, for a folder: a folder before its contents),
    /// named under the private folder. A link, or a file the source put elsewhere, is refused.
    fn list(&self, top: &Path, entries: &mut Vec<VirtualEntry>, staged: &mut Vec<PathBuf>) {
        let name = top.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let inside = top.parent().and_then(|p| open_path(p, libc::O_DIRECTORY)).is_some_and(|p| p == self.dir);
        if !inside {
            entries.push(failed(name, "The program put the file outside Gezik's folder"));
            staged.push(PathBuf::new());
            return;
        }
        let mut todo = vec![(top.to_path_buf(), name)];
        while let Some((path, name)) = todo.pop() {
            if entries.len() >= MOST {
                entries.push(failed(name, "Too many items; the rest were left out"));
                staged.push(PathBuf::new());
                return;
            }
            let meta = match std::fs::symlink_metadata(&path) {
                Ok(meta) if meta.file_type().is_symlink() => Err("A link; not taken"),
                Ok(meta) => Ok(meta),
                Err(_) => Err("It did not arrive"),
            };
            let meta = match meta {
                Ok(meta) => meta,
                Err(why) => {
                    entries.push(failed(name, why));
                    staged.push(PathBuf::new());
                    continue;
                }
            };
            entries.push(VirtualEntry {
                name: name.clone(),
                is_dir: meta.is_dir(),
                size: (!meta.is_dir()).then_some(meta.len()),
                failed: None,
            });
            staged.push(path.clone());
            if meta.is_dir()
                && let Ok(read) = std::fs::read_dir(&path)
            {
                for child in read.flatten() {
                    todo.push((child.path(), format!("{name}/{}", child.file_name().to_string_lossy())));
                }
            }
        }
    }
}

/// Puts the open `source` at `to`, a new file: an APFS clone when it can (instant), else a copy.
fn copy_out(
    source: &mut File,
    to: &Path,
    mode: u32,
    size: u64,
    progress: &mut dyn FnMut(u64) -> bool,
) -> io::Result<()> {
    let target_name = CString::new(to.as_os_str().as_bytes())?;
    if unsafe { libc::fclonefileat(source.as_raw_fd(), libc::AT_FDCWD, target_name.as_ptr(), 0) } == 0 {
        if progress(size) {
            return Ok(());
        }
        let _ = std::fs::remove_file(to);
        return Err(crate::fs::cancelled());
    }
    // Another volume (`$TMPDIR` is on the system's), or one that cannot clone.
    let mut target = OpenOptions::new().write(true).create_new(true).mode(mode & 0o777).open(to)?;
    let mut buffer = vec![0u8; 1 << 20];
    let mut done = 0u64;
    let copied = loop {
        let n = match source.read(&mut buffer) {
            Ok(0) => break Ok(()),
            Ok(n) => n,
            Err(err) => break Err(err),
        };
        if let Err(err) = target.write_all(&buffer[..n]) {
            break Err(err);
        }
        done += n as u64;
        if !progress(done) {
            break Err(crate::fs::cancelled());
        }
    };
    if copied.is_err() {
        drop(target);
        let _ = std::fs::remove_file(to);
    }
    copied
}

impl VirtualSource for Promised {
    fn entries(&self, stop: &dyn Fn() -> bool) -> io::Result<Vec<VirtualEntry>> {
        let mut got = self.arrivals.got.lock().map_err(lost)?;
        // shortcut: a receiver that names no files counts as one; should it write several, only
        // the first is waited for (the rest go with the folder). Upgrade if a source does that.
        while got.len() < self.expected {
            if stop() {
                return Err(crate::fs::cancelled());
            }
            got = self.arrivals.ready.wait_timeout(got, Duration::from_millis(200)).map_err(lost)?.0;
        }
        let arrived = got.clone();
        drop(got);
        let (mut entries, mut staged) = (Vec::new(), Vec::new());
        for result in arrived {
            match result {
                Ok(path) => self.list(&path, &mut entries, &mut staged),
                Err(why) => {
                    entries.push(failed(String::new(), &why));
                    staged.push(PathBuf::new());
                }
            }
        }
        *self.staged.lock().map_err(lost)? = staged;
        Ok(entries)
    }

    fn write(&self, index: usize, to: &Path, progress: &mut dyn FnMut(u64) -> bool) -> io::Result<u64> {
        let from = self
            .staged
            .lock()
            .map_err(lost)?
            .get(index)
            .cloned()
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
        // Read through what was opened: a link at its end is refused, and the open file must
        // really be in the private folder (a folder on the way swapped for a link is caught).
        let mut source = open(&from, 0)?;
        let meta = source.metadata()?;
        if !meta.is_file() {
            return Err(refused("Not a plain file; not taken"));
        }
        if !real_path(&source).is_some_and(|path| path.starts_with(&self.dir)) {
            return Err(refused("The file is outside Gezik's folder"));
        }
        copy_out(&mut source, to, meta.mode(), meta.len(), progress)?;
        // Taken: the copy in the private folder is not needed (kept until then for a retry).
        drop(source);
        let _ = std::fs::remove_file(&from);
        Ok(meta.len())
    }
}

impl Drop for Promised {
    fn drop(&mut self) {
        // What was not taken (a cancelled or failed job, a refused name) goes with it; links in
        // it are removed, not followed.
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_link_promises_are_left_out() {
        assert!(is_link_only(&[LINK_TYPE.to_owned()]));
        assert!(!is_link_only(&[LINK_TYPE.to_owned(), "public.jpeg".to_owned()]));
        assert!(!is_link_only(&["com.apple.mail.email".to_owned()]));
        assert!(!is_link_only(&[]), "a promise that names no type may be anything");
    }

    #[test]
    fn the_private_folder_is_new_and_only_ours() {
        use std::os::unix::fs::PermissionsExt;
        let dir = private_dir().unwrap();
        assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
        let other = private_dir().unwrap();
        assert_ne!(other, dir, "never the same folder twice");
        std::fs::remove_dir(other).unwrap();
        std::fs::write(dir.join("a.txt"), b"abc").unwrap();
        std::os::unix::fs::symlink(dir.join("a.txt"), dir.join("link")).unwrap();
        let promised = Promised {
            dir: dir.clone(),
            expected: 0,
            arrivals: Arc::default(),
            staged: Mutex::new(vec![dir.join("a.txt"), dir.join("link")]),
        };
        let out = std::env::temp_dir().join(format!("gezik-drop-test-{}", std::process::id()));
        let _ = std::fs::remove_file(&out);
        assert!(promised.write(1, &out, &mut |_| true).is_err(), "a link is not followed");
        assert_eq!(promised.write(0, &out, &mut |_| true).unwrap(), 3);
        assert_eq!(std::fs::read(&out).unwrap(), b"abc");
        assert!(promised.write(0, &out, &mut |_| true).is_err(), "taken once");
        drop(promised);
        assert!(!dir.exists(), "the folder goes with the source");
        let _ = std::fs::remove_file(&out);
    }
}
