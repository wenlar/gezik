//! `secure` on macOS and Linux: every path opened from `/` part by part with
//! `O_DIRECTORY | O_NOFOLLOW`, every change relative to an open folder (`unlinkat`, `renameat`,
//! `mkdirat`, `openat(O_CREAT | O_EXCL | O_NOFOLLOW)`, `fchmod` on what was opened).

use std::ffi::{CStr, CString, OsStr};
use std::fs::File;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path};

use super::secure::{
    A_LINK, CHANGED, FOLDER_THERE, Guard, LINK_ON_THE_WAY, MAX_DEPTH, NOT_A_FOLDER, NOT_PLAIN, OTHER_DRIVE,
    SEVERAL_NAMES, SPECIAL, STICKY_FILE, TOO_DEEP, random_name, refused,
};

pub(crate) type Id = (u64, u64);

const DIR: libc::c_int = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
/// A file opened to read or change: never through a link, never hanging on a pipe swapped in.
const FILE: libc::c_int = libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK | libc::O_NOCTTY;

fn cvt(result: libc::c_int) -> io::Result<libc::c_int> {
    if result < 0 { Err(io::Error::last_os_error()) } else { Ok(result) }
}

fn c(name: &OsStr) -> io::Result<CString> {
    CString::new(name.as_bytes()).map_err(|_| refused("A NUL byte in a name"))
}

fn open_at(dir: RawFd, name: &CStr, flags: libc::c_int) -> io::Result<OwnedFd> {
    // SAFETY: `name` is a valid C string; the descriptor is owned from here on.
    let fd = cvt(unsafe { libc::openat(dir, name.as_ptr(), flags, 0o600 as libc::c_uint) })?;
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

/// A link (ELOOP), or something not a folder where a folder was needed (ENOTDIR), on the way.
fn on_the_way(err: io::Error) -> io::Error {
    match err.raw_os_error() {
        Some(libc::ELOOP | libc::ENOTDIR) => refused(LINK_ON_THE_WAY),
        _ => err,
    }
}

/// The folder `path`, opened from `/` part by part, never through a link.
fn open_dir(path: &Path) -> io::Result<OwnedFd> {
    if !path.has_root() {
        return Err(refused(NOT_PLAIN));
    }
    let mut fd = open_at(libc::AT_FDCWD, c"/", DIR)?;
    for part in path.components() {
        match part {
            Component::RootDir => {}
            Component::Normal(name) => fd = open_at(fd.as_raw_fd(), &c(name)?, DIR).map_err(on_the_way)?,
            _ => return Err(refused(NOT_PLAIN)),
        }
    }
    Ok(fd)
}

/// The open folder holding `path`, and `path`'s name in it.
fn parent_of(path: &Path) -> io::Result<(OwnedFd, CString)> {
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return Err(refused("Not an item in a folder"));
    };
    Ok((open_dir(parent)?, c(name)?))
}

fn stat_at(dir: RawFd, name: &CStr) -> io::Result<libc::stat> {
    // SAFETY: a zeroed stat is valid; fstatat fills it and never follows a link here.
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    cvt(unsafe { libc::fstatat(dir, name.as_ptr(), &mut st, libc::AT_SYMLINK_NOFOLLOW) })?;
    Ok(st)
}

fn fstat(fd: RawFd) -> io::Result<libc::stat> {
    // SAFETY: as in `stat_at`.
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    cvt(unsafe { libc::fstat(fd, &mut st) })?;
    Ok(st)
}

fn kind(st: &libc::stat) -> libc::mode_t {
    st.st_mode & libc::S_IFMT
}

// The types differ by system (macOS `st_dev` is an i32).
#[allow(clippy::unnecessary_cast)]
fn id(st: &libc::stat) -> Id {
    (st.st_dev as u64, st.st_ino as u64)
}

/// A file with other names (hard links): changing it changes them all, wherever they are.
fn several_names(st: &libc::stat) -> bool {
    kind(st) != libc::S_IFDIR && st.st_nlink > 1
}

pub(crate) fn id_of(path: &Path) -> Option<Id> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::symlink_metadata(path).ok()?;
    (!meta.file_type().is_symlink()).then(|| (meta.dev(), meta.ino()))
}

/// The names in an open folder (`.` and `..` left out). A read error ends the list early; the
/// folder's own removal then fails (not empty), which is the error reported.
fn names(dir: &OwnedFd) -> io::Result<Vec<CString>> {
    // SAFETY: a copy of the descriptor for the stream, closed with it.
    let copy = cvt(unsafe { libc::dup(dir.as_raw_fd()) })?;
    let stream = unsafe { libc::fdopendir(copy) };
    if stream.is_null() {
        let err = io::Error::last_os_error();
        unsafe { libc::close(copy) };
        return Err(err);
    }
    let mut out = Vec::new();
    loop {
        // SAFETY: `stream` is open; the entry lives until the next readdir.
        let entry = unsafe { libc::readdir(stream) };
        if entry.is_null() {
            break;
        }
        let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) };
        if name.to_bytes() != b"." && name.to_bytes() != b".." {
            out.push(name.to_owned());
        }
    }
    unsafe { libc::closedir(stream) };
    Ok(out)
}

pub(crate) fn delete(path: &Path, guard: &Guard) -> io::Result<()> {
    let (dir, name) = parent_of(path)?;
    let parent = fstat(dir.as_raw_fd())?;
    remove(dir.as_raw_fd(), &name, &parent, guard, 0)
}

fn remove(dir: RawFd, name: &CStr, parent: &libc::stat, guard: &Guard, depth: usize) -> io::Result<()> {
    let st = stat_at(dir, name)?;
    guard.check(id(&st))?;
    if kind(&st) != libc::S_IFDIR {
        // A file (one of its names, if it has several), a link (the link itself) or anything
        // else: one name goes.
        cvt(unsafe { libc::unlinkat(dir, name.as_ptr(), 0) })?;
        return Ok(());
    }
    if st.st_dev != parent.st_dev {
        return Err(refused(OTHER_DRIVE));
    }
    if depth >= MAX_DEPTH {
        return Err(refused(TOO_DEEP));
    }
    let fd = open_at(dir, name, DIR).map_err(on_the_way)?;
    let now = fstat(fd.as_raw_fd())?;
    if id(&now) != id(&st) {
        return Err(refused(CHANGED));
    }
    for child in names(&fd)? {
        remove(fd.as_raw_fd(), &child, &now, guard, depth + 1)?;
    }
    cvt(unsafe { libc::unlinkat(dir, name.as_ptr(), libc::AT_REMOVEDIR) })?;
    Ok(())
}

pub(crate) fn rmdir(path: &Path, guard: &Guard) -> io::Result<()> {
    let (dir, name) = parent_of(path)?;
    let st = stat_at(dir.as_raw_fd(), &name)?;
    guard.check(id(&st))?;
    if kind(&st) != libc::S_IFDIR {
        return Err(refused(NOT_A_FOLDER));
    }
    cvt(unsafe { libc::unlinkat(dir.as_raw_fd(), name.as_ptr(), libc::AT_REMOVEDIR) })?;
    Ok(())
}

pub(crate) fn mkdir(path: &Path) -> io::Result<()> {
    let (dir, name) = parent_of(path)?;
    cvt(unsafe { libc::mkdirat(dir.as_raw_fd(), name.as_ptr(), 0o755) })?;
    Ok(())
}

/// `renameat`; without `replace` a name that is taken fails (Linux `RENAME_NOREPLACE`, macOS
/// `RENAME_EXCL`), with it a file there is replaced.
fn rename_at(from_dir: RawFd, from: &CStr, to_dir: RawFd, to: &CStr, replace: bool) -> io::Result<()> {
    if replace {
        cvt(unsafe { libc::renameat(from_dir, from.as_ptr(), to_dir, to.as_ptr()) })?;
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    {
        const RENAME_NOREPLACE: libc::c_uint = 1;
        // SAFETY: the renameat2 system call with valid C strings. shortcut: a file system without
        // RENAME_NOREPLACE says EINVAL and the rename fails; add a link-and-unlink way if one shows up.
        let result = unsafe {
            libc::syscall(libc::SYS_renameat2, from_dir, from.as_ptr(), to_dir, to.as_ptr(), RENAME_NOREPLACE)
        };
        cvt(result as libc::c_int)?;
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        // SAFETY: valid C strings and descriptors.
        cvt(unsafe { libc::renameatx_np(from_dir, from.as_ptr(), to_dir, to.as_ptr(), libc::RENAME_EXCL) })?;
        Ok(())
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (from_dir, from, to_dir, to);
        Err(io::ErrorKind::Unsupported.into())
    }
}

pub(crate) fn rename(path: &Path, name: &str, guard: &Guard) -> io::Result<()> {
    let (dir, old) = parent_of(path)?;
    guard.check(id(&stat_at(dir.as_raw_fd(), &old)?))?;
    let new = c(OsStr::new(name))?;
    rename_at(dir.as_raw_fd(), &old, dir.as_raw_fd(), &new, false)
}

/// What is at `to` in `dir` must not stop a replace: a folder is never replaced, a file with
/// several names never changed.
fn replaceable(dir: RawFd, to: &CStr) -> io::Result<()> {
    match stat_at(dir, to) {
        Ok(st) if kind(&st) == libc::S_IFDIR => Err(refused(FOLDER_THERE)),
        Ok(st) if several_names(&st) => Err(refused(SEVERAL_NAMES)),
        _ => Ok(()),
    }
}

pub(crate) fn move_to(from: &Path, to: &Path, replace: bool, guard: &Guard) -> io::Result<()> {
    let (from_dir, from_name) = parent_of(from)?;
    guard.check(id(&stat_at(from_dir.as_raw_fd(), &from_name)?))?;
    let (to_dir, to_name) = parent_of(to)?;
    if replace {
        replaceable(to_dir.as_raw_fd(), &to_name)?;
    }
    match rename_at(from_dir.as_raw_fd(), &from_name, to_dir.as_raw_fd(), &to_name, replace) {
        Err(err) if err.raw_os_error() == Some(libc::EXDEV) => {
            copy(from, to, replace)?;
            delete(from, guard)
        }
        other => other,
    }
}

pub(crate) fn copy(from: &Path, to: &Path, replace: bool) -> io::Result<()> {
    let (src_dir, src_name) = parent_of(from)?;
    let (dst_dir, dst_name) = parent_of(to)?;
    let made = matches!(stat_at(dst_dir.as_raw_fd(), &dst_name), Err(err) if err.kind() == io::ErrorKind::NotFound);
    let result = copy_at(src_dir.as_raw_fd(), &src_name, dst_dir.as_raw_fd(), &dst_name, replace, 0);
    if result.is_err() && made {
        let parent = fstat(dst_dir.as_raw_fd())?;
        let _ = remove(dst_dir.as_raw_fd(), &dst_name, &parent, &Guard::none(), 0);
    }
    result
}

fn copy_at(src: RawFd, name: &CStr, dst: RawFd, to: &CStr, replace: bool, depth: usize) -> io::Result<()> {
    let st = stat_at(src, name)?;
    match kind(&st) {
        libc::S_IFLNK => {
            let target = read_link_at(src, name)?;
            place(dst, to, replace, |at| {
                // SAFETY: valid C strings; symlinkat never follows the new name.
                cvt(unsafe { libc::symlinkat(target.as_ptr(), dst, at.as_ptr()) }).map(drop)
            })
        }
        libc::S_IFREG => {
            let from = File::from(open_at(src, name, FILE)?);
            let now = fstat(from.as_raw_fd())?;
            if id(&now) != id(&st) || kind(&now) != libc::S_IFREG {
                return Err(refused(CHANGED));
            }
            place(dst, to, replace, |at| {
                let flags = libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC;
                let out = File::from(open_at(dst, at, flags)?);
                std::io::copy(&mut &from, &mut &out)?;
                // Never setuid or setgid on a copy (spec §10.5).
                cvt(unsafe { libc::fchmod(out.as_raw_fd(), st.st_mode & 0o777) })?;
                let times = [
                    libc::timespec { tv_sec: st.st_atime, tv_nsec: st.st_atime_nsec as _ },
                    libc::timespec { tv_sec: st.st_mtime, tv_nsec: st.st_mtime_nsec as _ },
                ];
                // SAFETY: two timespecs, as futimens wants.
                let _ = unsafe { libc::futimens(out.as_raw_fd(), times.as_ptr()) };
                #[cfg(target_os = "macos")]
                xattrs(&from, &out);
                Ok(())
            })
        }
        libc::S_IFDIR => {
            if depth >= MAX_DEPTH {
                return Err(refused(TOO_DEEP));
            }
            let from = open_at(src, name, DIR).map_err(on_the_way)?;
            if id(&fstat(from.as_raw_fd())?) != id(&st) {
                return Err(refused(CHANGED));
            }
            let made = match stat_at(dst, to) {
                Ok(there) if kind(&there) == libc::S_IFDIR => false,
                Ok(_) => return Err(io::ErrorKind::AlreadyExists.into()),
                Err(err) if err.kind() == io::ErrorKind::NotFound => {
                    cvt(unsafe { libc::mkdirat(dst, to.as_ptr(), 0o700) })?;
                    true
                }
                Err(err) => return Err(err),
            };
            let out = open_at(dst, to, DIR).map_err(on_the_way)?;
            for child in names(&from)? {
                copy_at(from.as_raw_fd(), &child, out.as_raw_fd(), &child, replace, depth + 1)?;
            }
            if made {
                // The folder's own permissions last (0700 kept it Gezik's while it filled); sticky
                // stays, setuid and setgid do not.
                cvt(unsafe { libc::fchmod(out.as_raw_fd(), st.st_mode & 0o1777) })?;
            }
            Ok(())
        }
        _ => Err(refused(SPECIAL)),
    }
}

/// Makes `to` in `dir` with `make`: straight under its name (which fails if taken), or with
/// `replace` under a temporary name moved over it.
fn place(dir: RawFd, to: &CStr, replace: bool, make: impl FnOnce(&CStr) -> io::Result<()>) -> io::Result<()> {
    if !replace {
        return make(to);
    }
    replaceable(dir, to)?;
    let temp = CString::new(format!(".gezik-{}", random_name())).map_err(|_| refused(CHANGED))?;
    let done = make(&temp).and_then(|()| rename_at(dir, &temp, dir, to, true));
    if done.is_err() {
        // SAFETY: a valid C string; only the temporary name goes.
        unsafe { libc::unlinkat(dir, temp.as_ptr(), 0) };
    }
    done
}

fn read_link_at(dir: RawFd, name: &CStr) -> io::Result<CString> {
    let mut buffer = vec![0u8; 4096];
    // SAFETY: the buffer holds `len` bytes.
    let len = unsafe { libc::readlinkat(dir, name.as_ptr(), buffer.as_mut_ptr().cast(), buffer.len()) };
    if len < 0 {
        return Err(io::Error::last_os_error());
    }
    let len = len as usize;
    if len >= buffer.len() {
        return Err(refused("A link too long to copy"));
    }
    buffer.truncate(len);
    CString::new(buffer).map_err(|_| refused(CHANGED))
}

#[cfg(target_os = "macos")]
fn xattrs(from: &File, to: &File) {
    unsafe extern "C" {
        fn fcopyfile(from: libc::c_int, to: libc::c_int, state: *mut libc::c_void, flags: u32) -> libc::c_int;
    }
    const COPYFILE_XATTR: u32 = 1 << 2;
    // SAFETY: two open descriptors, no state. Extended attributes only: the data is copied.
    let _ = unsafe { fcopyfile(from.as_raw_fd(), to.as_raw_fd(), std::ptr::null_mut(), COPYFILE_XATTR) };
}

/// `path` itself opened for a permission, owner or flag change: a file or folder (never a link,
/// device, pipe or socket, nor a file with several names), the same one that was looked at.
fn open_item(path: &Path, guard: &Guard) -> io::Result<(File, libc::stat)> {
    let (dir, name) = parent_of(path)?;
    let st = stat_at(dir.as_raw_fd(), &name)?;
    guard.check(id(&st))?;
    let k = kind(&st);
    if k == libc::S_IFLNK {
        return Err(refused(A_LINK));
    }
    if k != libc::S_IFREG && k != libc::S_IFDIR {
        return Err(refused(SPECIAL));
    }
    let flags = if k == libc::S_IFDIR { DIR } else { FILE };
    let file = File::from(open_at(dir.as_raw_fd(), &name, flags)?);
    let now = fstat(file.as_raw_fd())?;
    if id(&now) != id(&st) || kind(&now) != k {
        return Err(refused(CHANGED));
    }
    // Checked on what was opened: a name linked to it in between counts too.
    if several_names(&now) {
        return Err(refused(SEVERAL_NAMES));
    }
    Ok((file, now))
}

pub(crate) fn chmod(path: &Path, mode: u32, guard: &Guard) -> io::Result<()> {
    let (file, st) = open_item(path, guard)?;
    if mode & 0o1000 != 0 && kind(&st) != libc::S_IFDIR {
        return Err(refused(STICKY_FILE));
    }
    cvt(unsafe { libc::fchmod(file.as_raw_fd(), (mode & 0o1777) as libc::mode_t) })?;
    Ok(())
}

pub(crate) fn chown(path: &Path, uid: u32, gid: u32, guard: &Guard) -> io::Result<()> {
    let (file, _) = open_item(path, guard)?;
    cvt(unsafe { libc::fchown(file.as_raw_fd(), uid, gid) })?;
    Ok(())
}

#[cfg(target_os = "macos")]
pub(crate) fn chflags(path: &Path, set: u32, clear: u32, guard: &Guard) -> io::Result<()> {
    let (file, st) = open_item(path, guard)?;
    // Every other flag (iCloud's, compression) as it is.
    let flags = (st.st_flags & !clear) | set;
    cvt(unsafe { libc::fchflags(file.as_raw_fd(), flags) })?;
    Ok(())
}
