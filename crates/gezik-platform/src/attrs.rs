//! Reading and writing what the Info window shows (spec 9 §4.4): permissions, owner, group and
//! the macOS Hidden and Locked flags, never through a link; the names of users and groups.
//! macOS and Linux; elsewhere every call says it is not there (Windows has its Properties).

use std::io;
use std::path::Path;

use gezik_core::attrs::{Attrs, Entry};

#[cfg(unix)]
pub use unix::{groups, has_acl, my_groups, read, users, write};

#[cfg(not(unix))]
fn unsupported() -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, "Windows has its own Properties window")
}
#[cfg(not(unix))]
pub fn read(_path: &Path) -> io::Result<Entry> {
    Err(unsupported())
}
#[cfg(not(unix))]
pub fn write(_path: &Path, _from: Attrs, _to: Attrs) -> io::Result<()> {
    Err(unsupported())
}
#[cfg(not(unix))]
pub fn users() -> Vec<(String, u32)> {
    Vec::new()
}
#[cfg(not(unix))]
pub fn groups() -> Vec<(String, u32)> {
    Vec::new()
}
#[cfg(not(unix))]
pub fn my_groups() -> Vec<u32> {
    Vec::new()
}
#[cfg(not(unix))]
pub fn has_acl(_path: &Path) -> bool {
    false
}

#[cfg(unix)]
mod unix {
    use std::ffi::{CStr, CString};
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::MetadataExt;
    use std::sync::Mutex;

    use gezik_core::attrs::{HIDDEN, Identity, LOCKED, PERMS, SPECIAL, flags_first};

    use super::*;

    /// getpwent and getgrent keep their place in the C library's own state: one walk at a time.
    static WALKING: Mutex<()> = Mutex::new(());
    /// The most names read (a directory service may list a whole company).
    const MAX_NAMES: usize = 10_000;

    fn c_path(path: &Path) -> io::Result<CString> {
        CString::new(path.as_os_str().as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "The path holds a NUL byte"))
    }

    #[cfg(target_os = "macos")]
    fn own_flags(meta: &std::fs::Metadata) -> u32 {
        use std::os::macos::fs::MetadataExt as _;
        meta.st_flags()
    }

    #[cfg(not(target_os = "macos"))]
    fn own_flags(_meta: &std::fs::Metadata) -> u32 {
        0
    }

    pub fn read(path: &Path) -> io::Result<Entry> {
        let meta = std::fs::symlink_metadata(path)?;
        Ok(Entry {
            id: Identity { dev: meta.dev(), ino: meta.ino() },
            attrs: Attrs {
                mode: meta.mode() & 0o7777,
                uid: meta.uid(),
                gid: meta.gid(),
                flags: own_flags(&meta) & (HIDDEN | LOCKED),
            },
            is_dir: meta.is_dir(),
            is_link: meta.file_type().is_symlink(),
        })
    }

    /// Writes what differs between `from` and `to`, never through a link: an unlock first, then
    /// owner and group with `lchown`, then permissions with `fchmodat(AT_SYMLINK_NOFOLLOW)` (a
    /// link's are refused), then a lock. chown clears setuid and setgid, so the mode is checked
    /// after it and the special bits the item had (and `to` keeps) are put back; none is added.
    pub fn write(path: &Path, from: Attrs, to: Attrs) -> io::Result<()> {
        let meta = std::fs::symlink_metadata(path)?;
        let is_link = meta.file_type().is_symlink();
        if is_link && from.mode & PERMS != to.mode & PERMS {
            return Err(io::Error::new(io::ErrorKind::Unsupported, "A link's permissions cannot be changed"));
        }
        let c = c_path(path)?;
        let flags_now = own_flags(&meta);
        let unlock_first = flags_first(from, to);
        if unlock_first {
            set_flags(&c, flags_now, to.flags)?;
        }
        let uid = (from.uid != to.uid).then_some(to.uid);
        let gid = (from.gid != to.gid).then_some(to.gid);
        if uid.is_some() || gid.is_some() {
            std::os::unix::fs::lchown(path, uid, gid)?;
        }
        if !is_link {
            let mode = (meta.mode() & to.mode & SPECIAL) | (to.mode & PERMS);
            let now = || std::fs::symlink_metadata(path).map(|m| m.mode() & 0o7777);
            if now()? != mode {
                set_mode(path, &c, mode)?;
                if now()? != mode {
                    // chmod by someone outside the item's group drops setgid without an error.
                    return Err(io::Error::other("The system turned setuid or setgid off"));
                }
            }
        }
        if !unlock_first && from.flags != to.flags {
            set_flags(&c, flags_now, to.flags)?;
        }
        Ok(())
    }

    fn set_mode(path: &Path, c: &CStr, mode: u32) -> io::Result<()> {
        // SAFETY: `c` is a valid C string for the call; AT_SYMLINK_NOFOLLOW never follows a link.
        if unsafe { libc::fchmodat(libc::AT_FDCWD, c.as_ptr(), mode as libc::mode_t, libc::AT_SYMLINK_NOFOLLOW) } == 0 {
            return Ok(());
        }
        let err = io::Error::last_os_error();
        // glibc before 2.32 (or no /proc) refuses AT_SYMLINK_NOFOLLOW for every path.
        // shortcut: an lstat check, then a chmod that would follow a link put there in between; upgrade to fchmodat2 (Linux 6.6) once it is the floor.
        #[cfg(target_os = "linux")]
        if err.raw_os_error() == Some(libc::EOPNOTSUPP) && !std::fs::symlink_metadata(path)?.file_type().is_symlink() {
            return std::fs::set_permissions(
                path,
                <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(mode),
            );
        }
        let _ = path;
        Err(err)
    }

    #[cfg(target_os = "macos")]
    fn set_flags(c: &CStr, now: u32, wanted: u32) -> io::Result<()> {
        // Every other flag (iCloud's, compression) as it is.
        let flags = (now & !(HIDDEN | LOCKED)) | (wanted & (HIDDEN | LOCKED));
        // SAFETY: `c` is a valid C string; lchflags never follows a link.
        if unsafe { libc::lchflags(c.as_ptr(), flags as _) } == 0 { Ok(()) } else { Err(io::Error::last_os_error()) }
    }

    #[cfg(not(target_os = "macos"))]
    fn set_flags(_c: &CStr, _now: u32, wanted: u32) -> io::Result<()> {
        if wanted == 0 {
            Ok(())
        } else {
            Err(io::Error::new(io::ErrorKind::Unsupported, "Hidden and Locked are macOS flags"))
        }
    }

    fn sorted(mut names: Vec<(String, u32)>) -> Vec<(String, u32)> {
        names.sort();
        names.dedup_by(|a, b| a.0 == b.0);
        names
    }

    /// Every user the system lists (name, uid).
    pub fn users() -> Vec<(String, u32)> {
        let _one = WALKING.lock().unwrap_or_else(|e| e.into_inner());
        let mut out = Vec::new();
        // SAFETY: each record is copied before the next call; the lock keeps other walks out.
        unsafe {
            libc::setpwent();
            while out.len() < MAX_NAMES {
                let entry = libc::getpwent();
                if entry.is_null() {
                    break;
                }
                out.push((CStr::from_ptr((*entry).pw_name).to_string_lossy().into_owned(), (*entry).pw_uid));
            }
            libc::endpwent();
        }
        sorted(out)
    }

    /// Every group the system lists (name, gid).
    pub fn groups() -> Vec<(String, u32)> {
        let _one = WALKING.lock().unwrap_or_else(|e| e.into_inner());
        let mut out = Vec::new();
        // SAFETY: as in `users`.
        unsafe {
            libc::setgrent();
            while out.len() < MAX_NAMES {
                let entry = libc::getgrent();
                if entry.is_null() {
                    break;
                }
                out.push((CStr::from_ptr((*entry).gr_name).to_string_lossy().into_owned(), (*entry).gr_gid));
            }
            libc::endgrent();
        }
        sorted(out)
    }

    /// The groups this user is in: a group change to one of these needs no administrator.
    pub fn my_groups() -> Vec<u32> {
        // SAFETY: the first call asks for the count, the second fills a buffer that large.
        let count = unsafe { libc::getgroups(0, std::ptr::null_mut()) };
        let mut groups: Vec<libc::gid_t> = vec![0; usize::try_from(count).unwrap_or(0)];
        // SAFETY: as above.
        let got = unsafe { libc::getgroups(count.max(0), groups.as_mut_ptr()) };
        groups.truncate(usize::try_from(got).unwrap_or(0));
        // SAFETY: no arguments.
        groups.push(unsafe { libc::getegid() });
        groups.sort_unstable();
        groups.dedup();
        groups
    }

    /// Whether `path` (not what a link leads to) has access control entries beyond its mode.
    #[cfg(target_os = "linux")]
    pub fn has_acl(path: &Path) -> bool {
        let Ok(c) = c_path(path) else { return false };
        [c"system.posix_acl_access", c"system.posix_acl_default"].iter().any(|name| {
            // SAFETY: a size query (no buffer) on valid C strings; lgetxattr never follows a link.
            unsafe { libc::lgetxattr(c.as_ptr(), name.as_ptr(), std::ptr::null_mut(), 0) > 0 }
        })
    }

    #[cfg(target_os = "macos")]
    pub fn has_acl(path: &Path) -> bool {
        use std::ffi::{c_char, c_int, c_void};
        unsafe extern "C" {
            fn acl_get_link_np(path: *const c_char, kind: c_int) -> *mut c_void;
            fn acl_get_entry(acl: *mut c_void, entry_id: c_int, entry: *mut *mut c_void) -> c_int;
            fn acl_free(object: *mut c_void) -> c_int;
        }
        const ACL_TYPE_EXTENDED: c_int = 0x0000_0100;
        const ACL_FIRST_ENTRY: c_int = 0;
        let Ok(c) = c_path(path) else { return false };
        // SAFETY: a valid C string; the ACL is freed once its first entry was asked for.
        unsafe {
            let acl = acl_get_link_np(c.as_ptr(), ACL_TYPE_EXTENDED);
            if acl.is_null() {
                return false;
            }
            let mut entry = std::ptr::null_mut();
            let has = acl_get_entry(acl, ACL_FIRST_ENTRY, &mut entry) == 0;
            acl_free(acl);
            has
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use gezik_core::attrs::{Change, PERMS};

    use super::*;
    use crate::fs::test_dir;

    #[test]
    fn writes_without_following_links() {
        use std::os::unix::fs::PermissionsExt;
        let dir = test_dir("attrs-link");
        let file = dir.join("f.txt");
        std::fs::write(&file, "x").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        let link = dir.join("l");
        std::os::unix::fs::symlink(&file, &link).unwrap();
        let before = read(&file).unwrap();
        assert_eq!(before.attrs.mode & PERMS, 0o644);
        write(&file, before.attrs, Change::bit(0o020, true).apply(before.attrs)).unwrap();
        assert_eq!(read(&file).unwrap().attrs.mode & PERMS, 0o664);
        let l = read(&link).unwrap();
        assert!(l.is_link);
        assert!(write(&link, l.attrs, Change::bit(0o002, true).apply(l.attrs)).is_err(), "a link's permissions");
        assert_eq!(read(&file).unwrap().attrs.mode & PERMS, 0o664, "what it leads to is untouched");
        let gid = my_groups()[0];
        write(&link, l.attrs, Change::group(gid).apply(l.attrs)).unwrap();
        assert_eq!(read(&link).unwrap().attrs.gid, gid, "the link's own group");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_group_change_keeps_setuid_and_adds_none() {
        use std::os::unix::fs::PermissionsExt;
        let dir = test_dir("attrs-setuid");
        let (suid, plain) = (dir.join("suid"), dir.join("plain"));
        for (file, mode) in [(&suid, 0o4755), (&plain, 0o755)] {
            std::fs::write(file, "x").unwrap();
            std::fs::set_permissions(file, std::fs::Permissions::from_mode(mode)).unwrap();
        }
        // SAFETY: no arguments.
        let gid = unsafe { libc::getegid() };
        for (file, mode) in [(&suid, 0o4750), (&plain, 0o750)] {
            let a = read(file).unwrap().attrs;
            let to = Change::mode(0o750).apply(Change::group(gid).apply(a));
            // A `from` group that differs makes it call lchown even if the group stays the same.
            write(file, Attrs { gid: !gid, ..a }, to).unwrap();
            assert_eq!(read(file).unwrap().attrs.mode, mode, "{}", file.display());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn names_include_this_user() {
        // SAFETY: no arguments.
        let (uid, gid) = unsafe { (libc::getuid(), libc::getegid()) };
        assert!(users().iter().any(|(_, id)| *id == uid) || users().is_empty(), "a directory service may list none");
        assert!(my_groups().contains(&gid));
        assert!(!groups().is_empty());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_locked_file_is_unlocked_before_its_permissions() {
        use gezik_core::attrs::LOCKED;
        let dir = test_dir("attrs-locked");
        let file = dir.join("f");
        std::fs::write(&file, "x").unwrap();
        let a = read(&file).unwrap().attrs;
        let locked = Change::flag(LOCKED, true).apply(a);
        write(&file, a, locked).unwrap();
        assert!(write(&file, locked, Change::bit(0o020, true).apply(locked)).is_err(), "locked: no chmod");
        let both = Change::flag(LOCKED, false).apply(Change::bit(0o020, true).apply(locked));
        write(&file, locked, both).unwrap();
        assert_eq!(read(&file).unwrap().attrs, both);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
