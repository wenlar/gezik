//! Reading and writing what the Info window shows (spec 9 §4.4): permissions, owner, group and
//! the macOS Hidden and Locked flags, never through a link; the names of users and groups.
//! macOS and Linux; elsewhere every call says it is not there (Windows has its Properties).

use std::io;
use std::path::Path;

use gezik_core::attrs::{Attrs, Entry, Identity};

#[cfg(unix)]
pub use unix::{groups, has_acl, my_groups, read, users, write};

/// The system calls of one write, on one item (a test's fake elsewhere).
#[cfg(any(unix, test))]
trait Steps {
    /// Hidden and Locked become `flags`; every other flag stays.
    fn set_flags(&mut self, flags: u32) -> io::Result<()>;
    fn chown(&mut self, uid: Option<u32>, gid: Option<u32>) -> io::Result<()>;
    /// The permission and special bits now.
    fn mode(&mut self) -> io::Result<u32>;
    fn chmod(&mut self, mode: u32) -> io::Result<()>;
}

/// Writes what differs between `from` and `to`: an unlock first, then owner and group, then
/// permissions, then a lock. chown clears setuid and setgid, so the mode is checked after it
/// and the special bits the item `had` (and `to` keeps) are put back; none is added. `had` is
/// None for a link: its mode is never written. If a step after an unlock fails, the item is
/// locked again.
#[cfg(any(unix, test))]
fn apply(steps: &mut impl Steps, had: Option<u32>, from: Attrs, to: Attrs) -> io::Result<()> {
    use gezik_core::attrs::{PERMS, SPECIAL, flags_first};
    let unlock_first = flags_first(from, to);
    if unlock_first {
        steps.set_flags(to.flags)?;
    }
    let mut rest = || -> io::Result<()> {
        let uid = (from.uid != to.uid).then_some(to.uid);
        let gid = (from.gid != to.gid).then_some(to.gid);
        if uid.is_some() || gid.is_some() {
            steps.chown(uid, gid)?;
        }
        if let Some(had) = had {
            let mode = (had & to.mode & SPECIAL) | (to.mode & PERMS);
            if steps.mode()? != mode {
                steps.chmod(mode)?;
                if steps.mode()? != mode {
                    // chmod by someone outside the item's group drops setgid without an error.
                    return Err(io::Error::other("The system turned setuid or setgid off"));
                }
            }
        }
        Ok(())
    };
    if let Err(err) = rest() {
        if unlock_first {
            let _ = steps.set_flags(from.flags);
        }
        return Err(err);
    }
    if !unlock_first && from.flags != to.flags {
        steps.set_flags(to.flags)?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn unsupported() -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, "Windows has its own Properties window")
}
#[cfg(not(unix))]
pub fn read(_path: &Path) -> io::Result<Entry> {
    Err(unsupported())
}
#[cfg(not(unix))]
pub fn write(_path: &Path, _id: Identity, _from: Attrs, _to: Attrs) -> io::Result<()> {
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

    use gezik_core::attrs::{HIDDEN, LOCKED, PERMS};

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

    /// Writes what differs between `from` and `to` (see `apply`) if `path` is still `id`,
    /// never through a link: owner and group with `lchown`, permissions with
    /// `fchmodat(AT_SYMLINK_NOFOLLOW)` (a link's are refused), flags with `lchflags`.
    pub fn write(path: &Path, id: Identity, from: Attrs, to: Attrs) -> io::Result<()> {
        let meta = std::fs::symlink_metadata(path)?;
        if (Identity { dev: meta.dev(), ino: meta.ino() }) != id {
            return Err(io::Error::new(io::ErrorKind::NotFound, "Another file is there now"));
        }
        let is_link = meta.file_type().is_symlink();
        if is_link && from.mode & PERMS != to.mode & PERMS {
            return Err(io::Error::new(io::ErrorKind::Unsupported, "A link's permissions cannot be changed"));
        }
        let c = c_path(path)?;
        let mut steps = Sys { path, c: &c, flags_now: own_flags(&meta) };
        super::apply(&mut steps, (!is_link).then_some(meta.mode() & 0o7000), from, to)
    }

    struct Sys<'a> {
        path: &'a Path,
        c: &'a CStr,
        /// Every flag as it was (only Hidden and Locked change).
        flags_now: u32,
    }

    impl super::Steps for Sys<'_> {
        fn set_flags(&mut self, flags: u32) -> io::Result<()> {
            set_flags(self.c, self.flags_now, flags)
        }

        fn chown(&mut self, uid: Option<u32>, gid: Option<u32>) -> io::Result<()> {
            std::os::unix::fs::lchown(self.path, uid, gid)
        }

        fn mode(&mut self) -> io::Result<u32> {
            Ok(std::fs::symlink_metadata(self.path)?.mode() & 0o7777)
        }

        fn chmod(&mut self, mode: u32) -> io::Result<()> {
            set_mode(self.path, self.c, mode)
        }
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

#[cfg(test)]
mod step_tests {
    use gezik_core::attrs::{Change, LOCKED};

    use super::*;

    /// One item's attributes in memory; `refuse` names the step that fails.
    struct Fake {
        attrs: Attrs,
        refuse: &'static str,
        /// What chmod really sets (`mode & keep`): the system dropping a bit.
        keep: u32,
    }

    impl Fake {
        fn step(&self, name: &str) -> io::Result<()> {
            if self.refuse == name { Err(io::ErrorKind::PermissionDenied.into()) } else { Ok(()) }
        }
    }

    impl Steps for Fake {
        fn set_flags(&mut self, flags: u32) -> io::Result<()> {
            self.step("flags")?;
            self.attrs.flags = flags;
            Ok(())
        }

        fn chown(&mut self, uid: Option<u32>, gid: Option<u32>) -> io::Result<()> {
            self.step("chown")?;
            self.attrs.uid = uid.unwrap_or(self.attrs.uid);
            self.attrs.gid = gid.unwrap_or(self.attrs.gid);
            self.attrs.mode &= !0o6000; // as the kernel does
            Ok(())
        }

        fn mode(&mut self) -> io::Result<u32> {
            Ok(self.attrs.mode)
        }

        fn chmod(&mut self, mode: u32) -> io::Result<()> {
            self.step("chmod")?;
            self.attrs.mode = mode & self.keep;
            Ok(())
        }
    }

    fn fake(mode: u32, flags: u32, refuse: &'static str) -> Fake {
        Fake { attrs: Attrs { mode, uid: 501, gid: 20, flags }, refuse, keep: 0o7777 }
    }

    #[test]
    fn chown_comes_first_and_setuid_is_put_back_never_added() {
        for (mode, after) in [(0o4755, 0o4750), (0o0755, 0o0750)] {
            let mut f = fake(mode, 0, "");
            let from = f.attrs;
            let to = Change::mode(0o750).apply(Change::group(30).apply(from));
            apply(&mut f, Some(mode & 0o7000), from, to).unwrap();
            assert_eq!(f.attrs, Attrs { mode: after, gid: 30, ..f.attrs });
        }
    }

    #[test]
    fn a_failure_after_an_unlock_locks_it_again() {
        for refuse in ["chown", "chmod"] {
            let mut f = fake(0o644, LOCKED, refuse);
            let from = f.attrs;
            let to = Change::flag(LOCKED, false).apply(Change::mode(0o600).apply(Change::owner(0).apply(from)));
            assert!(apply(&mut f, Some(0), from, to).is_err());
            assert_eq!(f.attrs.flags, LOCKED, "{refuse}");
        }
    }

    #[test]
    fn setgid_dropped_by_the_system_is_an_error_and_relocks() {
        let mut f = Fake { keep: !0o2000, ..fake(0o2755, LOCKED, "") };
        let from = f.attrs;
        let to = Change::flag(LOCKED, false).apply(Change::group(30).apply(from));
        assert!(apply(&mut f, Some(0o2000), from, to).is_err());
        assert_eq!((f.attrs.gid, f.attrs.flags), (30, LOCKED), "the group changed; the lock is back");
    }

    #[test]
    fn a_link_mode_is_never_written() {
        let mut f = fake(0o777, 0, "chmod");
        let from = f.attrs;
        apply(&mut f, None, from, Change::group(30).apply(from)).unwrap();
        assert_eq!(f.attrs.gid, 30);
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
        write(&file, before.id, before.attrs, Change::bit(0o020, true).apply(before.attrs)).unwrap();
        assert_eq!(read(&file).unwrap().attrs.mode & PERMS, 0o664);
        let other = Identity { ino: before.id.ino + 1, ..before.id };
        assert!(write(&file, other, before.attrs, before.attrs).is_err(), "another file is there now");
        let l = read(&link).unwrap();
        assert!(l.is_link);
        assert!(write(&link, l.id, l.attrs, Change::bit(0o002, true).apply(l.attrs)).is_err(), "a link's permissions");
        assert_eq!(read(&file).unwrap().attrs.mode & PERMS, 0o664, "what it leads to is untouched");
        let gid = my_groups()[0];
        write(&link, l.id, l.attrs, Change::group(gid).apply(l.attrs)).unwrap();
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
            let Entry { id, attrs: a, .. } = read(file).unwrap();
            let to = Change::mode(0o750).apply(Change::group(gid).apply(a));
            // A `from` group that differs makes it call lchown even if the group stays the same.
            write(file, id, Attrs { gid: !gid, ..a }, to).unwrap();
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
        let Entry { id, attrs: a, .. } = read(&file).unwrap();
        let locked = Change::flag(LOCKED, true).apply(a);
        write(&file, id, a, locked).unwrap();
        assert!(write(&file, id, locked, Change::bit(0o020, true).apply(locked)).is_err(), "locked: no chmod");
        let both = Change::flag(LOCKED, false).apply(Change::bit(0o020, true).apply(locked));
        write(&file, id, locked, both).unwrap();
        assert_eq!(read(&file).unwrap().attrs, both);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
