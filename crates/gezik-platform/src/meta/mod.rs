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
pub use windows::{read, volume, write};

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
