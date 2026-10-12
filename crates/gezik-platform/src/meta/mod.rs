//! A file's own tag record (spec 12 §4): Gezik's `:gezik.meta` stream on Windows, freedesktop
//! and Baloo attributes on Linux, Finder's attributes on macOS. Only raw reads and writes and
//! what a volume can hold; the central store and the choice between them are `gezik-ops`'.

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

use std::io;
use std::path::Path;

use gezik_core::meta::Meta;

#[cfg(unix)]
pub(crate) use unix::copy_xattrs;
#[cfg(unix)]
pub use unix::{read, volume, write};
#[cfg(windows)]
pub use windows::{explorer, read, volume, write};

/// `work`'s answer if it comes within `limit`; else `None`, and `work` is left to finish on its
/// own thread (a property handler that hangs on a broken file cannot be stopped; spec 12 §8.3).
/// The `within` calls running (those left behind too).
static LIVE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Tests that use `within` run one at a time, each after the calls an earlier one left behind.
#[cfg(test)]
pub(crate) fn within_alone() -> std::sync::MutexGuard<'static, ()> {
    static ALONE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let guard = ALONE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let start = std::time::Instant::now();
    while LIVE.load(std::sync::atomic::Ordering::SeqCst) > 0 && start.elapsed().as_secs() < 5 {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    guard
}

/// At most `MAX_LIVE` such calls run at once (those left behind count until they end); `None`
/// when that many already do.
pub fn within<T: Send + 'static>(limit: std::time::Duration, work: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    use std::sync::atomic::Ordering;
    const MAX_LIVE: usize = 4;
    if LIVE.fetch_add(1, Ordering::SeqCst) >= MAX_LIVE {
        LIVE.fetch_sub(1, Ordering::SeqCst);
        return None;
    }
    let (send, answer) = std::sync::mpsc::sync_channel(1);
    let spawned = std::thread::Builder::new().name("gezik-props".into()).spawn(move || {
        let _ = send.send(work());
        LIVE.fetch_sub(1, Ordering::SeqCst);
    });
    if spawned.is_err() {
        LIVE.fetch_sub(1, Ordering::SeqCst);
        return None;
    }
    answer.recv_timeout(limit).ok()
}

/// Sets each attribute of `changes` (`None`: removed) that differs from what `old` reads, the
/// largest first (a full disk refuses the comment before anything else is written); when one
/// fails, those set before it get their old values back, so a record is never left half
/// written. Linux and macOS (Windows writes one stream).
#[cfg_attr(windows, allow(dead_code))]
pub(crate) fn set_all(
    mut changes: Vec<(&str, Option<Vec<u8>>)>,
    old: impl Fn(&str) -> io::Result<Option<Vec<u8>>>,
    mut set: impl FnMut(&str, Option<&[u8]>) -> io::Result<()>,
) -> io::Result<()> {
    changes.sort_by_key(|(_, value)| std::cmp::Reverse(value.as_ref().map_or(0, Vec::len)));
    let mut done: Vec<(&str, Option<Vec<u8>>)> = Vec::new();
    for (name, value) in changes {
        let before = old(name)?;
        if before == value {
            continue;
        }
        if let Err(err) = set(name, value.as_deref()) {
            for (name, before) in done.iter().rev() {
                let _ = set(name, before.as_deref());
            }
            return Err(err);
        }
        done.push((name, before));
    }
    Ok(())
}

/// What a file's own record holds.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Own {
    pub meta: Meta,
    /// False for a record Gezik cannot read in full (another shape, a later version): it is
    /// shown as far as read, never written over.
    pub writable: bool,
}

impl Own {
    fn none() -> Own {
        Own { meta: Meta::default(), writable: true }
    }
}

/// A volume as the tag store names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Volume {
    /// The same on every mount of it: a serial number or UUID, a share's name.
    pub key: String,
    /// Whether its files hold their own record (NTFS streams, extended attributes); FAT,
    /// exFAT, read-only volumes and some shares do not.
    pub holds: bool,
}

/// A stable number for the file at `path` where the system keeps one (NTFS, ext4, APFS).
pub fn file_id(path: &Path) -> Option<u64> {
    // An NTFS id fits in 64 bits; ReFS's 128-bit ids keep their low half (Unix: already 64).
    #[allow(clippy::unnecessary_cast)]
    crate::fs::entry_id(path).ok().map(|(_, id)| id as u64)
}

/// The volume refused the record (no streams or attributes there, read only): every file on
/// it goes to the central store for the rest of the session.
pub fn refused_by_volume(err: &io::Error) -> bool {
    #[cfg(windows)]
    {
        // ERROR_INVALID_FUNCTION, ERROR_WRITE_PROTECT, ERROR_NOT_SUPPORTED, ERROR_INVALID_NAME
        // (FAT reads `a.txt:gezik.meta` as a bad name), ERROR_INVALID_PARAMETER.
        matches!(err.raw_os_error(), Some(1 | 19 | 50 | 87 | 123))
    }
    #[cfg(unix)]
    {
        err.raw_os_error().is_some_and(|code| [libc::ENOTSUP, libc::EOPNOTSUPP, libc::EROFS].contains(&code))
    }
}

/// The file refused it (read only, a link on Linux, no rights): that file goes to the central
/// store.
pub fn refused_by_file(err: &io::Error) -> bool {
    err.kind() == io::ErrorKind::PermissionDenied
}

#[cfg(test)]
mod tests {
    use super::*;
    use gezik_core::meta::{Tag, TagColor};

    fn test_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("gezik-meta-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_call_that_hangs_is_left_behind_after_its_time() {
        use std::time::{Duration, Instant};
        let _alone = within_alone();
        assert_eq!(within(Duration::from_secs(2), || 7), Some(7));
        let start = Instant::now();
        let hung = within(Duration::from_millis(50), || std::thread::sleep(Duration::from_millis(1500)));
        assert_eq!(hung, None);
        assert!(start.elapsed() < Duration::from_secs(1), "not waited for: {:?}", start.elapsed());
    }

    #[test]
    fn a_failed_attribute_puts_back_those_written_before_it() {
        use std::cell::RefCell;
        use std::collections::HashMap;
        let disk = RefCell::new(HashMap::from([("tags", b"Red".to_vec()), ("rating", b"6".to_vec())]));
        let old = |name: &str| Ok(disk.borrow().get(name).cloned());
        let writes = RefCell::new(Vec::new());
        // The disk is full for the rating: the comment (largest, first) and the tags go back.
        let set = |name: &str, value: Option<&[u8]>| {
            writes.borrow_mut().push(name.to_owned());
            if name == "rating" && value.is_some() {
                return Err(io::Error::from_raw_os_error(28));
            }
            match value {
                Some(v) => disk.borrow_mut().insert(name.to_string().leak(), v.to_vec()),
                None => disk.borrow_mut().remove(name),
            };
            Ok(())
        };
        let changes = vec![
            ("tags", Some(b"Red,Blue".to_vec())),
            ("rating", Some(b"8".to_vec())),
            ("comment", Some(b"A long comment".to_vec())),
        ];
        assert_eq!(set_all(changes, old, set).unwrap_err().raw_os_error(), Some(28));
        assert_eq!(writes.borrow()[..3], ["comment", "tags", "rating"], "the largest first");
        let disk = disk.into_inner();
        assert_eq!(disk.get("tags").map(Vec::as_slice), Some(&b"Red"[..]), "tags put back");
        assert_eq!(disk.get("rating").map(Vec::as_slice), Some(&b"6"[..]), "the half star stays");
        assert!(!disk.contains_key("comment"), "the new comment removed again");
    }

    #[test]
    fn calls_left_behind_are_capped() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::time::Duration;
        let _alone = within_alone();
        let ran = Arc::new(AtomicUsize::new(0));
        for _ in 0..8 {
            let ran = ran.clone();
            let _ = within(Duration::from_millis(5), move || {
                ran.fetch_add(1, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(500));
            });
        }
        // A spawned call may not have started yet under load; wait for it, not a fixed time.
        let until = std::time::Instant::now() + Duration::from_secs(5);
        while ran.load(Ordering::SeqCst) < 4 && std::time::Instant::now() < until {
            std::thread::sleep(Duration::from_millis(5));
        }
        let ran = ran.load(Ordering::SeqCst);
        assert_eq!(ran, 4, "at most four hang at once");
    }

    fn sample() -> Meta {
        Meta {
            tags: vec![Tag::new("Red", TagColor::Red), Tag::new("Öncelikli", TagColor::None)],
            rating: 4,
            comment: "Ready for review".into(),
        }
    }

    fn holds(dir: &Path) -> bool {
        volume(&crate::fs::drive_root(dir).unwrap()).unwrap().holds
    }

    #[test]
    fn a_record_round_trips_and_leaves_the_contents_alone() {
        let dir = test_dir("round");
        if !holds(&dir) {
            return;
        }
        let file = dir.join("a.txt");
        std::fs::write(&file, "contents").unwrap();
        assert_eq!(read(&file).unwrap(), Own::none(), "no record yet");
        write(&file, &sample()).unwrap();
        let back = read(&file).unwrap();
        // Linux keeps no colours (they come from the user's list).
        let colours = cfg!(not(target_os = "linux"));
        assert_eq!(back.meta.tags.len(), 2);
        assert_eq!(back.meta.tags[0].color == TagColor::Red, colours);
        assert_eq!((back.meta.rating, &*back.meta.comment), (4, "Ready for review"));
        assert_eq!(std::fs::read(&file).unwrap(), b"contents", "the contents are never touched");
        write(&file, &Meta::default()).unwrap();
        assert_eq!(read(&file).unwrap(), Own::none(), "an empty record goes");
        assert_eq!(std::fs::read(&file).unwrap(), b"contents");
        // A folder holds one too.
        write(&dir, &sample()).unwrap();
        assert_eq!(read(&dir).unwrap().meta.rating, 4);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn writing_keeps_the_modified_time() {
        let dir = test_dir("mtime");
        if !holds(&dir) {
            return;
        }
        let file = dir.join("a.txt");
        std::fs::write(&file, "x").unwrap();
        let old = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_500_000_000);
        std::fs::File::options().write(true).open(&file).unwrap().set_modified(old).unwrap();
        write(&file, &sample()).unwrap();
        assert_eq!(std::fs::metadata(&file).unwrap().modified().unwrap(), old);
        write(&file, &Meta::default()).unwrap();
        assert_eq!(std::fs::metadata(&file).unwrap().modified().unwrap(), old, "removing it too");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_file_is_not_made() {
        let dir = test_dir("missing");
        let file = dir.join("not-there.txt");
        assert!(write(&file, &sample()).is_err());
        assert!(!file.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn a_read_only_file_refuses_and_keeps_its_attribute() {
        let dir = test_dir("readonly");
        let file = dir.join("a.txt");
        std::fs::write(&file, "x").unwrap();
        let mut permissions = std::fs::metadata(&file).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&file, permissions.clone()).unwrap();
        let err = write(&file, &sample()).unwrap_err();
        assert!(refused_by_file(&err), "{err:?}");
        assert!(!refused_by_volume(&err));
        assert!(std::fs::metadata(&file).unwrap().permissions().readonly(), "the attribute stays");
        assert_eq!(read(&file).unwrap(), Own::none());
        #[allow(clippy::permissions_set_readonly_false, reason = "Windows only: clears the read-only attribute")]
        permissions.set_readonly(false);
        std::fs::set_permissions(&file, permissions).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn unknown_lines_are_kept_and_a_later_version_is_not_written() {
        let dir = test_dir("unknown");
        let file = dir.join("a.txt");
        std::fs::write(&file, "x").unwrap();
        let stream = dir.join("a.txt:gezik.meta");
        std::fs::write(&stream, "gezik-meta 1\ntag 6 Red\nfuture thing\n").unwrap();
        write(&file, &Meta { rating: 2, ..Meta::default() }).unwrap();
        assert_eq!(std::fs::read_to_string(&stream).unwrap(), "gezik-meta 1\nrating 2\nfuture thing\n");
        std::fs::write(&stream, "gezik-meta 2\ntag 6 Red\n").unwrap();
        let own = read(&file).unwrap();
        assert!(!own.writable);
        assert_eq!(own.meta.tags.len(), 1);
        let err = write(&file, &Meta::default()).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert_eq!(std::fs::read_to_string(&stream).unwrap(), "gezik-meta 2\ntag 6 Red\n", "left as it was");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn a_stream_that_would_grow_too_big_is_not_written() {
        let dir = test_dir("big");
        let file = dir.join("a.txt");
        std::fs::write(&file, "x").unwrap();
        let stream = dir.join("a.txt:gezik.meta");
        let old = format!(
            "gezik-meta 1
{}
",
            "z".repeat(16_340)
        );
        std::fs::write(&stream, &old).unwrap();
        let err = write(&file, &sample()).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(std::fs::read_to_string(&stream).unwrap(), old);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_half_star_rating_stays_unless_the_stars_change() {
        let dir = test_dir("baloo");
        let file = dir.join("a.txt");
        std::fs::write(&file, "x").unwrap();
        if write(&file, &sample()).is_err() {
            return;
        }
        let path = c_path_for_tests(&file);
        let set = |v: &[u8]| unsafe {
            libc::lsetxattr(path.as_ptr(), c"user.baloo.rating".as_ptr(), v.as_ptr().cast(), v.len(), 0)
        };
        assert_eq!(set(b"7"), 0);
        write(&file, &Meta { rating: 4, ..sample() }).unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"x");
        let mut buf = [0u8; 8];
        let n = unsafe {
            libc::lgetxattr(path.as_ptr(), c"user.baloo.rating".as_ptr(), buf.as_mut_ptr().cast(), buf.len())
        };
        assert_eq!(&buf[..n as usize], b"7", "Dolphin's half star is kept");
        write(&file, &Meta { rating: 2, ..sample() }).unwrap();
        assert_eq!(read(&file).unwrap().meta.rating, 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(target_os = "linux")]
    fn c_path_for_tests(path: &Path) -> std::ffi::CString {
        use std::os::unix::ffi::OsStrExt;
        std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap()
    }

    /// Spec 12 §4.2, the first check of 12a: CopyFileExW takes the stream along to NTFS.
    #[cfg(windows)]
    #[test]
    fn a_copy_keeps_the_stream_on_ntfs() {
        let dir = test_dir("copy");
        if !holds(&dir) {
            return;
        }
        let file = dir.join("a.txt");
        std::fs::write(&file, "x").unwrap();
        write(&file, &sample()).unwrap();
        crate::fs::copy_file(&file, &dir.join("b.txt"), 1, &mut |_| true).unwrap();
        assert_eq!(read(&dir.join("b.txt")).unwrap().meta, sample());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn a_copy_keeps_the_attributes() {
        let dir = test_dir("copy");
        if !holds(&dir) {
            return;
        }
        let file = dir.join("a.txt");
        std::fs::write(&file, "x".repeat(10_000)).unwrap();
        if write(&file, &sample()).is_err() {
            // tmpfs before Linux 6.6 holds no user attributes.
            return;
        }
        crate::fs::copy_file(&file, &dir.join("b.txt"), 10_000, &mut |_| true).unwrap();
        assert_eq!(read(&dir.join("b.txt")).unwrap().meta.tags.len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_volume_has_a_key() {
        let dir = test_dir("volume");
        let root = crate::fs::drive_root(&dir).unwrap();
        let volume = volume(&root).unwrap();
        assert!(!volume.key.is_empty());
        assert_eq!(volume, super::volume(&root).unwrap(), "the same each time");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Finder sees what Gezik wrote (macOS checklist 341): run with `--ignored`, then look at
    /// `~/gezik-tag-probe.txt` in Finder: red, tagged Red and Gezik Probe.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore]
    fn finder_probe() {
        let file = dirs::home_dir().unwrap().join("gezik-tag-probe.txt");
        std::fs::write(&file, "Gezik tag probe\n").unwrap();
        let meta = Meta {
            tags: vec![Tag::new("Gezik Probe", TagColor::None), Tag::new("Red", TagColor::Red)],
            rating: 3,
            comment: "Written by Gezik".into(),
        };
        write(&file, &meta).unwrap();
        assert_eq!(read(&file).unwrap().meta, meta);
    }

    /// Reads what Finder wrote (macOS checklist 342): tag `~/gezik-tag-probe.txt` Green in
    /// Finder first.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore]
    fn finder_wrote_green() {
        let file = dirs::home_dir().unwrap().join("gezik-tag-probe.txt");
        let own = read(&file).unwrap();
        assert!(own.writable);
        assert!(own.meta.tags.iter().any(|t| t.color == TagColor::Green), "{:?}", own.meta.tags);
    }

    /// A copy onto an exFAT or FAT disk keeps its tags (macOS checklist 343; macOS keeps them in
    /// `._` files there): `GEZIK_TEST_TAG_DIR=/Volumes/STICK cargo test -p gezik-platform meta
    /// -- --ignored`.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore]
    fn a_copy_to_an_exfat_disk_keeps_its_tags() {
        let Some(target) = std::env::var_os("GEZIK_TEST_TAG_DIR").map(std::path::PathBuf::from) else { return };
        let dir = test_dir("exfat");
        let file = dir.join("a.txt");
        std::fs::write(&file, "x").unwrap();
        write(&file, &sample()).unwrap();
        let copy = target.join("gezik-tag-copy.txt");
        let _ = std::fs::remove_file(&copy);
        crate::fs::copy_file(&file, &copy, 1, &mut |_| true).unwrap();
        let back = read(&copy).unwrap().meta;
        let _ = std::fs::remove_file(&copy);
        assert_eq!(back, sample());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Dolphin sees what Gezik wrote (Linux checklist 250): run with `--ignored`, then open
    /// `~/gezik-tag-probe.txt`'s Information panel in Dolphin.
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore]
    fn dolphin_probe() {
        let file = dirs::home_dir().unwrap().join("gezik-tag-probe.txt");
        std::fs::write(&file, "Gezik tag probe\n").unwrap();
        write(&file, &sample()).unwrap();
        let back = read(&file).unwrap().meta;
        assert_eq!((back.tags.len(), back.rating, &*back.comment), (2, 4, "Ready for review"));
    }
}
