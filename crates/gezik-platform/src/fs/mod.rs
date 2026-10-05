//! What the system does best for file operations: copying with progress, deleting, moving
//! without replacing, and what kind of drive a path is on.

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod freedesktop;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

use std::io;
use std::path::{Path, PathBuf};

pub use gezik_core::ops::threads::DiskKind;
#[cfg(unix)]
pub use unix::{
    clear_hidden, copy_file, delete, drive_facts, drive_root, is_hidden_attr, move_entry, restore, set_hidden, trash,
};
#[cfg(windows)]
pub use windows::{
    clear_hidden, copy_file, delete, drive_facts, drive_root, is_hidden_attr, move_entry, restore, set_hidden, trash,
};

/// What the engine needs to know about the drive a path is on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriveFacts {
    /// Same id, same drive: a volume serial number, a device number or a network share.
    pub id: String,
    pub kind: DiskKind,
    /// Whether deleting can go to the trash (Recycle Bin).
    pub trash: bool,
}

/// Whether the trash can take `path` by its name. The Windows Shell parses a path like
/// Explorer and drops a trailing dot or space from each of its parts, so `x.` (or anything
/// in `a.\`) would reach the sibling `x` (or `a\`): such paths cannot go to the Recycle Bin.
pub fn can_trash_name(path: &Path) -> bool {
    !cfg!(windows)
        || !path.components().any(|part| match part {
            std::path::Component::Normal(name) => name.to_string_lossy().ends_with(['.', ' ']),
            _ => false,
        })
}

/// Files at least this big are copied past the system file cache, so a large copy does not
/// push everything else out of it.
pub const BIG_FILE: u64 = 256 * 1024 * 1024;

#[cfg(windows)]
const DISK_FULL_CODES: [i32; 2] = [112, 39];
#[cfg(unix)]
const DISK_FULL_CODES: [i32; 1] = [libc::ENOSPC];

/// Whether `err` means the disk is full.
pub fn is_disk_full(err: &io::Error) -> bool {
    err.kind() == io::ErrorKind::StorageFull || err.raw_os_error().is_some_and(|code| DISK_FULL_CODES.contains(&code))
}

/// `path` or its nearest ancestor that exists.
pub fn nearest_existing(path: &Path) -> Option<PathBuf> {
    path.ancestors().find(|p| std::fs::symlink_metadata(p).is_ok()).map(Path::to_path_buf)
}

/// The error a cancelled copy returns.
pub(crate) fn cancelled() -> io::Error {
    io::Error::new(io::ErrorKind::Interrupted, "cancelled")
}

#[cfg(test)]
pub(crate) fn test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gezik-fs-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_copies_and_reports_progress() {
        let dir = test_dir("copy");
        let (from, to) = (dir.join("a.bin"), dir.join("b.bin"));
        let data: Vec<u8> = (0..3_000_000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(&from, &data).unwrap();
        let mut seen = Vec::new();
        copy_file(&from, &to, data.len() as u64, &mut |done| {
            seen.push(done);
            true
        })
        .unwrap();
        assert_eq!(std::fs::read(&to).unwrap(), data);
        assert_eq!(seen.last().copied(), Some(data.len() as u64));
        assert!(seen.windows(2).all(|w| w[0] <= w[1]), "progress only grows");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn copy_never_overwrites() {
        let dir = test_dir("no-overwrite");
        let (from, to) = (dir.join("a.txt"), dir.join("b.txt"));
        std::fs::write(&from, "new").unwrap();
        std::fs::write(&to, "old").unwrap();
        let err = copy_file(&from, &to, 3, &mut |_| true).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists, "{err}");
        assert_eq!(std::fs::read_to_string(&to).unwrap(), "old");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_cancelled_copy_leaves_nothing() {
        let dir = test_dir("cancel");
        let (from, to) = (dir.join("a.bin"), dir.join("b.bin"));
        std::fs::write(&from, vec![7u8; 4_000_000]).unwrap();
        let err = copy_file(&from, &to, 4_000_000, &mut |_| false).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::Interrupted, "{err}");
        assert!(!to.exists(), "the half-written copy is removed");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn move_entry_never_overwrites() {
        let dir = test_dir("move");
        let (a, b, c) = (dir.join("a.txt"), dir.join("b.txt"), dir.join("c.txt"));
        std::fs::write(&a, "a").unwrap();
        std::fs::write(&b, "b").unwrap();
        let err = move_entry(&a, &b).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists, "{err}");
        assert_eq!(std::fs::read_to_string(&b).unwrap(), "b");
        move_entry(&a, &c).unwrap();
        assert!(!a.exists() && std::fs::read_to_string(&c).unwrap() == "a");
        let sub = dir.join("sub");
        std::fs::create_dir(&sub).unwrap();
        move_entry(&sub, &dir.join("sub2")).unwrap();
        assert!(dir.join("sub2").is_dir());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_removes_files_read_only_files_and_empty_folders() {
        let dir = test_dir("delete");
        let file = dir.join("a.txt");
        std::fs::write(&file, "x").unwrap();
        let mut permissions = std::fs::metadata(&file).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&file, permissions).unwrap();
        delete(&file).unwrap();
        assert!(!file.exists());
        let full = dir.join("full");
        std::fs::create_dir(&full).unwrap();
        std::fs::write(full.join("x"), "x").unwrap();
        assert!(delete(&full).is_err(), "a folder with something in it stays");
        std::fs::remove_file(full.join("x")).unwrap();
        delete(&full).unwrap();
        assert!(!full.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn drive_facts_name_the_same_drive_alike() {
        let dir = test_dir("drive");
        let a = drive_facts(&dir).unwrap();
        let b = drive_facts(&dir.join("not-yet-there.txt")).unwrap_or_else(|_| a.clone());
        assert!(!a.id.is_empty());
        assert_eq!(a.id, b.id);
        assert!(drive_root(&dir).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn disk_full_is_recognized() {
        assert!(is_disk_full(&io::Error::from(io::ErrorKind::StorageFull)));
        assert!(is_disk_full(&io::Error::from_raw_os_error(DISK_FULL_CODES[0])));
        assert!(!is_disk_full(&io::Error::from(io::ErrorKind::NotFound)));
    }

    #[test]
    fn nearest_existing_walks_up() {
        let dir = test_dir("nearest");
        assert_eq!(nearest_existing(&dir.join("a").join("b")), Some(dir.clone()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn trash_and_restore_round_trip() {
        let dir = test_dir("trash");
        let file = dir.join("çöp testi.txt");
        std::fs::write(&file, "keep me").unwrap();
        let trashed = trash(&file).unwrap().expect("the temp folder has a trash");
        assert!(!file.exists());
        assert!(std::fs::symlink_metadata(&trashed).is_ok(), "{}", trashed.display());
        restore(&trashed, &file).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "keep me");
        assert!(std::fs::symlink_metadata(&trashed).is_err());
        #[cfg(windows)]
        {
            let name = trashed.file_name().unwrap().to_str().unwrap();
            assert!(name.starts_with("$R") && name.ends_with(".txt"), "{}", trashed.display());
            let info = trashed.with_file_name(format!("$I{}", &name[2..]));
            assert!(std::fs::symlink_metadata(&info).is_err(), "the $I record is gone: {}", info.display());
        }

        let folder = dir.join("sub");
        std::fs::create_dir(&folder).unwrap();
        std::fs::write(folder.join("x"), "x").unwrap();
        let trashed = trash(&folder).unwrap().unwrap();
        std::fs::write(&folder, "in the way").ok();
        assert_eq!(restore(&trashed, &folder).unwrap_err().kind(), io::ErrorKind::AlreadyExists);
        std::fs::remove_file(&folder).unwrap();
        restore(&trashed, &folder).unwrap();
        assert!(folder.join("x").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn trashing_what_is_not_there_fails() {
        let dir = test_dir("trash-missing");
        assert!(trash(&dir.join("nothing-here")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn a_folder_ending_in_a_dot_never_trashes_the_file_in_its_sibling() {
        let dir = test_dir("trash-dot-parent");
        let (real, dotted) = (dir.join("a"), PathBuf::from(format!(r"\\?\{}\a.", dir.display())));
        std::fs::create_dir(&real).unwrap();
        std::fs::create_dir(&dotted).unwrap();
        std::fs::write(real.join("x"), "real").unwrap();
        std::fs::write(dotted.join("x"), "dotted").unwrap();
        let path = PathBuf::from(format!(r"{}\a.\x", dir.display()));
        let err = trash(&path).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput, "{err}");
        assert_eq!(std::fs::read_to_string(real.join("x")).unwrap(), "real");
        assert_eq!(std::fs::read_to_string(dotted.join("x")).unwrap(), "dotted");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn a_name_ending_in_a_dot_never_trashes_its_sibling() {
        let dir = test_dir("trash-dot");
        let sibling = dir.join("x");
        let dotted = PathBuf::from(format!(r"\\?\{}\x.", dir.display()));
        std::fs::write(&sibling, "sibling").unwrap();
        std::fs::write(&dotted, "dotted").unwrap();
        // The Shell drops a trailing dot and would take `x`; neither spelling reaches `x.`.
        for path in [PathBuf::from(format!(r"{}\x.", dir.display())), dotted.clone()] {
            let err = trash(&path).unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::InvalidInput, "{err}");
            assert_eq!(std::fs::read_to_string(&sibling).unwrap(), "sibling");
            assert_eq!(std::fs::read_to_string(&dotted).unwrap(), "dotted");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
