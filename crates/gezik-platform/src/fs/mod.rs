//! What the system does best for file operations: copying with progress, deleting, moving
//! without replacing, and what kind of drive a path is on.

mod describe;
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) mod freedesktop;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub use describe::describe;
pub use gezik_core::ops::threads::DiskKind;
#[cfg(unix)]
use unix::entry_id;
#[cfg(unix)]
pub use unix::{
    clear_hidden, copy_file, delete, device_of, drive_facts, drive_root, free_space, is_hidden_attr, is_network,
    mapped_remote, move_entry, open_regular, read_dir_items, restore, set_hidden, trash,
};
#[cfg(windows)]
use windows::entry_id;
#[cfg(windows)]
pub use windows::{
    clear_hidden, copy_file, delete, device_of, drive_facts, drive_root, free_space, is_hidden_attr, is_network,
    mapped_remote, move_entry, open_regular, read_dir_items, restore, set_hidden, trash,
};
#[cfg(windows)]
pub(crate) use windows::{io_error, verbatim};

/// One item of a folder as the search reads it, with what the read itself gave (spec 3.4).
#[derive(Debug, Clone)]
pub struct DirItem {
    pub name: String,
    pub is_dir: bool,
    /// A symbolic link or junction (Windows: a name-surrogate reparse point, so a cloud or
    /// deduplicated folder is still gone into): never gone into, never read.
    pub is_link: bool,
    /// A regular file: not a folder, link, FIFO, socket or device. Only these are read for text.
    pub is_file: bool,
    /// Windows: the data is not on this disk (a cloud placeholder, an offline file), so reading
    /// it would download it. A content search leaves it unread.
    pub offline: bool,
    /// `Entry::HIDDEN` and `Entry::SYSTEM` (Windows).
    pub flags: u8,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub created: Option<SystemTime>,
    /// Unix: the folder's device (`st_dev`), to stay on one file system; 0 where unknown.
    pub device: u64,
    /// Size and times are filled (Unix reads them only for what was asked).
    pub has_meta: bool,
}

/// What the engine needs to know about the drive a path is on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriveFacts {
    /// Same id, same drive: a volume serial number, a device number or a network share.
    pub id: String,
    pub kind: DiskKind,
    /// Whether deleting can go to the trash (Recycle Bin).
    pub trash: bool,
    /// The biggest file its file system holds, if that is a limit a copy can hit (FAT32:
    /// 4 GB less a byte; it says "disk full" for a bigger one).
    pub max_file: Option<u64>,
}

/// The biggest file a file system named `name` (as Windows names it) holds, if it is small
/// enough to matter.
pub fn max_file_for(name: &str) -> Option<u64> {
    matches!(name.to_ascii_uppercase().as_str(), "FAT" | "FAT12" | "FAT16" | "FAT32").then_some(u64::from(u32::MAX))
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

/// Whether `a` and `b` name the same entry on disk, however they are spelled (`\\?\`, short
/// 8.3 names, a junction, a symbolic link or `subst` drive on the way, a mapped drive and its
/// share): the same volume and file id (Windows), device and inode (Unix). The last part
/// itself is not followed: a link and what it points to are two entries. `None` when either
/// cannot be read (missing, no access, a file system without ids).
pub fn same_entry(a: &Path, b: &Path) -> Option<bool> {
    Some(entry_id(a).ok()? == entry_id(b).ok()?)
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
    fn same_entry_sees_through_spellings() {
        let dir = test_dir("same-entry");
        std::fs::write(dir.join("x.txt"), "x").unwrap();
        std::fs::write(dir.join("y.txt"), "y").unwrap();
        std::fs::create_dir(dir.join("sub")).unwrap();
        let x = dir.join("x.txt");
        assert_eq!(same_entry(&x, &x), Some(true));
        assert_eq!(same_entry(&x, &dir.join("sub").join("..").join("x.txt")), Some(true));
        assert_eq!(same_entry(&x, &dir.join("y.txt")), Some(false));
        assert_eq!(same_entry(&x, &dir.join("missing.txt")), None);
        #[cfg(windows)]
        {
            let prefixed = PathBuf::from(format!(r"\\?\{}", x.display()));
            assert_eq!(same_entry(&x, &prefixed), Some(true));
            // A junction to the folder: another spelling of the same file.
            let junction = dir.parent().unwrap().join(format!("gezik-fs-same-entry-j-{}", std::process::id()));
            let _ = std::fs::remove_dir(&junction);
            let made = std::process::Command::new("cmd")
                .args(["/c", "mklink", "/J"])
                .arg(&junction)
                .arg(&dir)
                .output()
                .unwrap();
            assert!(made.status.success(), "{made:?}");
            assert_eq!(same_entry(&x, &junction.join("x.txt")), Some(true));
            assert_eq!(same_entry(&dir.join("y.txt"), &junction.join("x.txt")), Some(false));
            std::fs::remove_dir(&junction).unwrap();
        }
        #[cfg(unix)]
        {
            let link = dir.join("link");
            std::os::unix::fs::symlink(&dir, &link).unwrap();
            assert_eq!(same_entry(&x, &link.join("x.txt")), Some(true));
            assert_eq!(same_entry(&dir, &link), Some(false), "the link itself is its own entry");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn folder_items_carry_what_the_read_gave() {
        let dir = test_dir("items");
        std::fs::write(dir.join("a.txt"), "12345").unwrap();
        std::fs::create_dir(dir.join("sub")).unwrap();
        let mut items = read_dir_items(&dir, &|_, _| true).unwrap();
        items.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(items.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), ["a.txt", "sub"]);
        assert!(!items[0].is_dir && items[0].size == 5 && items[0].modified.is_some() && items[0].has_meta);
        assert!(items[1].is_dir && !items[1].is_link && items[1].size == 0);
        assert!(items[0].is_file && !items[0].offline && !items[1].is_file);
        let listed = gezik_core::list_dir(&dir).unwrap();
        assert_eq!(
            listed.iter().find(|e| e.name == "a.txt").unwrap().modified,
            items[0].modified,
            "as list_dir sees it"
        );
        assert!(read_dir_items(&dir.join("missing"), &|_, _| true).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn a_name_that_is_not_utf8_is_left_out() {
        use std::os::unix::ffi::OsStrExt;
        let dir = test_dir("not-utf8");
        // macOS refuses such a name itself.
        if std::fs::write(dir.join(std::ffi::OsStr::from_bytes(b"a\xFF")), "").is_ok() {
            std::fs::write(dir.join("a\u{FFFD}"), "").unwrap();
            let items = read_dir_items(&dir, &|_, _| true).unwrap();
            assert_eq!(items.into_iter().map(|i| i.name).collect::<Vec<_>>(), ["a\u{FFFD}"]);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_regular_files_are_opened_for_their_text() {
        let dir = test_dir("open-regular");
        std::fs::write(dir.join("a.txt"), "12345").unwrap();
        let (_, size) = open_regular(&dir.join("a.txt")).unwrap().expect("a file");
        assert_eq!(size, 5);
        assert!(open_regular(&dir).unwrap().is_none(), "a folder");
        assert!(open_regular(&dir.join("missing")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn a_fifo_or_a_link_is_never_opened() {
        let dir = test_dir("open-fifo");
        let fifo = dir.join("pipe");
        let c = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
        // SAFETY: a valid path string.
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
        assert!(open_regular(&fifo).unwrap().is_none(), "opened without blocking, then refused");
        std::fs::write(dir.join("a.txt"), "x").unwrap();
        std::os::unix::fs::symlink(dir.join("a.txt"), dir.join("link")).unwrap();
        assert!(open_regular(&dir.join("link")).unwrap().is_none(), "a link is not followed");
        let items = read_dir_items(&dir, &|_, _| true).unwrap();
        assert!(items.iter().filter(|i| i.is_file).map(|i| i.name.as_str()).eq(["a.txt"]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn unix_reads_meta_only_when_asked() {
        let dir = test_dir("items-lazy");
        std::fs::write(dir.join("a.txt"), "12345").unwrap();
        std::fs::create_dir(dir.join("sub")).unwrap();
        let items = read_dir_items(&dir, &|_, _| false).unwrap();
        let file = items.iter().find(|i| i.name == "a.txt").unwrap();
        assert!(!file.has_meta && file.size == 0, "no lstat for a file nobody wants");
        let folder = items.iter().find(|i| i.name == "sub").unwrap();
        assert!(folder.has_meta && folder.device == device_of(&dir).unwrap(), "folders always: their device");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn a_junction_is_a_link_folder() {
        use gezik_core::templates::LinkKind;
        let dir = test_dir("items-junction");
        std::fs::create_dir(dir.join("target")).unwrap();
        crate::link::create(LinkKind::Junction, &dir.join("target"), &dir.join("j"), true).unwrap();
        let items = read_dir_items(&dir, &|_, _| true).unwrap();
        let j = items.iter().find(|i| i.name == "j").unwrap();
        assert!(j.is_dir && j.is_link);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fat_holds_files_up_to_4_gb_and_the_others_have_no_limit_that_matters() {
        assert_eq!(max_file_for("FAT32"), Some(4 * 1024 * 1024 * 1024 - 1));
        assert_eq!(max_file_for("FAT"), Some(4 * 1024 * 1024 * 1024 - 1));
        assert_eq!(max_file_for("NTFS"), None);
        assert_eq!(max_file_for("exFAT"), None);
        assert_eq!(max_file_for("ReFS"), None);
    }

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
    fn free_space_is_known_for_an_existing_folder() {
        let dir = test_dir("free");
        assert!(free_space(&dir).unwrap() > 0);
        assert!(free_space(&dir.join("not-there")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn disk_full_is_recognized() {
        assert!(is_disk_full(&io::Error::from(io::ErrorKind::StorageFull)));
        assert!(is_disk_full(&io::Error::from_raw_os_error(DISK_FULL_CODES[0])));
        assert!(!is_disk_full(&io::Error::from(io::ErrorKind::NotFound)));
    }

    #[test]
    fn the_temp_folder_is_on_no_network_share() {
        let dir = test_dir("network");
        assert!(!is_network(&dir).unwrap(), "{}", dir.display());
        let _ = std::fs::remove_dir_all(&dir);
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
