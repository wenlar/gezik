//! macOS and Linux: copying (clone, copy_file_range, buffered), moving without replacing,
//! device facts.

use std::fs::{File, FileTimes, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use super::{DiskKind, DriveFacts, cancelled, nearest_existing};

pub fn copy_file(from: &Path, to: &Path, size: u64, progress: &mut dyn FnMut(u64) -> bool) -> io::Result<()> {
    let meta = std::fs::symlink_metadata(from)?;
    if meta.file_type().is_symlink() {
        // A link stays a link; `symlink` fails if `to` exists.
        std::os::unix::fs::symlink(std::fs::read_link(from)?, to)?;
        return Ok(());
    }
    if !progress(0) {
        return Err(cancelled());
    }
    #[cfg(target_os = "macos")]
    if clone_file(from, to).is_ok() {
        return if progress(size) {
            Ok(())
        } else {
            let _ = std::fs::remove_file(to);
            Err(cancelled())
        };
    }
    let mut source = File::open(from)?;
    let mut target = OpenOptions::new().write(true).create_new(true).mode(meta.mode()).open(to)?;
    let copied = copy_contents(&mut source, &mut target, size, progress);
    let copied = copied.and_then(|()| {
        let times = FileTimes::new().set_modified(meta.modified()?).set_accessed(meta.accessed()?);
        target.set_times(times)
    });
    if copied.is_err() {
        drop(target);
        let _ = std::fs::remove_file(to);
    }
    copied
}

/// APFS clones: the copy shares the blocks until either side changes, so it is instant.
#[cfg(target_os = "macos")]
fn clone_file(from: &Path, to: &Path) -> io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let (from, to) = (CString::new(from.as_os_str().as_bytes())?, CString::new(to.as_os_str().as_bytes())?);
    if unsafe { libc::clonefile(from.as_ptr(), to.as_ptr(), 0) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(target_os = "linux")]
fn copy_contents(
    source: &mut File,
    target: &mut File,
    size: u64,
    progress: &mut dyn FnMut(u64) -> bool,
) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    // Btrfs and XFS clone in one call; other file systems refuse and we copy.
    if unsafe { libc::ioctl(target.as_raw_fd(), libc::FICLONE, source.as_raw_fd()) } == 0 {
        return if progress(size) { Ok(()) } else { Err(cancelled()) };
    }
    let mut done = 0u64;
    loop {
        let n = unsafe {
            libc::copy_file_range(
                source.as_raw_fd(),
                std::ptr::null_mut(),
                target.as_raw_fd(),
                std::ptr::null_mut(),
                8 << 20,
                0,
            )
        };
        if n < 0 {
            let err = io::Error::last_os_error();
            let unsupported = matches!(
                err.raw_os_error(),
                Some(libc::EXDEV | libc::ENOSYS | libc::EINVAL | libc::EOPNOTSUPP | libc::EPERM | libc::EBADF)
            );
            return if done == 0 && unsupported { buffered(source, target, progress) } else { Err(err) };
        }
        if n == 0 {
            // Some kernels and file systems answer 0 ("end of file") for a file they cannot
            // copy this way; a non-empty file with nothing copied gets the plain loop.
            return if done == 0 && size > 0 { buffered(source, target, progress) } else { Ok(()) };
        }
        done += n as u64;
        if !progress(done) {
            return Err(cancelled());
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn copy_contents(
    source: &mut File,
    target: &mut File,
    _size: u64,
    progress: &mut dyn FnMut(u64) -> bool,
) -> io::Result<()> {
    buffered(source, target, progress)
}

fn buffered(source: &mut File, target: &mut File, progress: &mut dyn FnMut(u64) -> bool) -> io::Result<()> {
    let mut buffer = vec![0u8; 1 << 20];
    let mut done = 0u64;
    loop {
        let n = source.read(&mut buffer)?;
        if n == 0 {
            return Ok(());
        }
        target.write_all(&buffer[..n])?;
        done += n as u64;
        if !progress(done) {
            return Err(cancelled());
        }
    }
}

pub fn move_entry(from: &Path, to: &Path) -> io::Result<()> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    use std::ffi::CString;
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    use std::os::unix::ffi::OsStrExt;
    #[cfg(target_os = "linux")]
    {
        let (c_from, c_to) = (CString::new(from.as_os_str().as_bytes())?, CString::new(to.as_os_str().as_bytes())?);
        let done = unsafe {
            libc::renameat2(libc::AT_FDCWD, c_from.as_ptr(), libc::AT_FDCWD, c_to.as_ptr(), libc::RENAME_NOREPLACE)
        };
        if done == 0 {
            return Ok(());
        }
        let err = io::Error::last_os_error();
        if !matches!(err.raw_os_error(), Some(libc::ENOSYS | libc::EINVAL)) {
            return Err(err);
        }
        // renameat2 is not available here. A hard link fails atomically if `to` exists, so
        // for anything but a folder: link, then remove the old name.
        if !std::fs::symlink_metadata(from)?.is_dir() {
            match std::fs::hard_link(from, to) {
                Ok(()) => {
                    if let Err(err) = std::fs::remove_file(from) {
                        let _ = std::fs::remove_file(to);
                        return Err(err);
                    }
                    return Ok(());
                }
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists || err.raw_os_error() == Some(libc::EXDEV) => {
                    return Err(err);
                }
                // No hard links on this file system: fall through to the check below.
                Err(_) => {}
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        let (c_from, c_to) = (CString::new(from.as_os_str().as_bytes())?, CString::new(to.as_os_str().as_bytes())?);
        let done = unsafe { libc::renamex_np(c_from.as_ptr(), c_to.as_ptr(), libc::RENAME_EXCL) };
        if done == 0 {
            return Ok(());
        }
        let err = io::Error::last_os_error();
        // ENOTSUP: this volume cannot rename exclusively; fall through to the check below.
        if !matches!(err.raw_os_error(), Some(libc::ENOTSUP | libc::EINVAL)) {
            return Err(err);
        }
    }
    // Folders (and other platforms) have no atomic "do not replace" here: check first. A small
    // window remains between the check and the rename; this is a known limitation.
    if std::fs::symlink_metadata(to).is_ok() {
        return Err(io::Error::from(io::ErrorKind::AlreadyExists));
    }
    std::fs::rename(from, to)
}

pub fn delete(path: &Path) -> io::Result<()> {
    let meta = std::fs::symlink_metadata(path)?;
    if meta.is_dir() { std::fs::remove_dir(path) } else { std::fs::remove_file(path) }
}

pub fn drive_root(path: &Path) -> Option<PathBuf> {
    let start = nearest_existing(path)?;
    let dev = std::fs::metadata(&start).ok()?.dev();
    let mut root = start.clone();
    for parent in start.ancestors().skip(1) {
        match std::fs::metadata(parent) {
            Ok(meta) if meta.dev() == dev => root = parent.to_path_buf(),
            _ => break,
        }
    }
    Some(root)
}

pub fn drive_facts(path: &Path) -> io::Result<DriveFacts> {
    let existing = nearest_existing(path).ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
    let dev = std::fs::metadata(&existing)?.dev();
    Ok(DriveFacts { id: format!("{dev:x}"), kind: disk_kind(dev), trash: true })
}

/// glibc's encoding of a device number's major part.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn dev_major(dev: u64) -> u64 {
    ((dev >> 8) & 0xfff) | ((dev >> 32) & !0xfff)
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn dev_minor(dev: u64) -> u64 {
    (dev & 0xff) | ((dev >> 12) & !0xff)
}

#[cfg(target_os = "linux")]
fn disk_kind(dev: u64) -> DiskKind {
    let (major, minor) = (dev_major(dev), dev_minor(dev));
    let base = PathBuf::from(format!("/sys/dev/block/{major}:{minor}"));
    // A partition's own folder has no queue; its disk (the parent) does.
    for candidate in [base.join("queue/rotational"), base.join("../queue/rotational")] {
        if let Ok(text) = std::fs::read_to_string(candidate) {
            return if text.trim() == "1" { DiskKind::Hdd } else { DiskKind::Ssd };
        }
    }
    // No block device behind it (NFS, SMB, FUSE).
    if major == 0 { DiskKind::Network } else { DiskKind::Unknown }
}

#[cfg(not(target_os = "linux"))]
fn disk_kind(_dev: u64) -> DiskKind {
    // Every Mac Gezik runs on boots from an SSD; external spinning disks are rare.
    DiskKind::Ssd
}

/// Unix hides by name only, so there is no attribute to clear.
pub fn clear_hidden(_path: &Path) -> io::Result<()> {
    Ok(())
}

/// Whether the hidden attribute is set (never, on Unix).
pub fn is_hidden_attr(_path: &Path) -> bool {
    false
}

pub fn set_hidden(_path: &Path) -> io::Result<()> {
    // A leading dot already hides it.
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn trash(path: &Path) -> io::Result<Option<PathBuf>> {
    use objc2_foundation::{NSFileManager, NSString, NSURL};
    let text = path.to_str().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "the path is not UTF-8"))?;
    let url = NSURL::fileURLWithPath(&NSString::from_str(text));
    let mut resulting = None;
    NSFileManager::defaultManager()
        .trashItemAtURL_resultingItemURL_error(&url, Some(&mut resulting))
        .map_err(|err| io::Error::other(err.localizedDescription().to_string()))?;
    Ok(resulting.and_then(|url| url.path()).map(|p| PathBuf::from(p.to_string())))
}

#[cfg(target_os = "linux")]
pub fn trash(path: &Path) -> io::Result<Option<PathBuf>> {
    use super::freedesktop::{free_name, info_text};
    use std::os::unix::ffi::{OsStrExt, OsStringExt};

    let absolute = std::path::absolute(path)?;
    let dev = std::fs::symlink_metadata(&absolute)?.dev();
    let trash_dir = trash_dir_for(&absolute, dev)?;
    let (files, info) = (trash_dir.join("files"), trash_dir.join("info"));
    // Private, so other users cannot read or swap what is in the trash.
    for dir in [&files, &info] {
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    }
    let name = absolute.file_name().ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?.as_bytes().to_vec();
    let info_of = |n: &[u8]| {
        let mut file = n.to_vec();
        file.extend_from_slice(b".trashinfo");
        info.join(std::ffi::OsString::from_vec(file))
    };
    // The info file is made first, with create_new: it claims the name.
    let (chosen, info_path) = loop {
        let candidate = free_name(&name, |n| {
            info_of(n).exists() || std::fs::symlink_metadata(files.join(std::ffi::OsStr::from_bytes(n))).is_ok()
        });
        let info_path = info_of(&candidate);
        match OpenOptions::new().write(true).create_new(true).open(&info_path) {
            Ok(mut file) => {
                if let Err(err) = file.write_all(info_text(absolute.as_os_str().as_bytes(), &local_now()).as_bytes()) {
                    drop(file);
                    let _ = std::fs::remove_file(&info_path);
                    return Err(err);
                }
                break (candidate, info_path);
            }
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err),
        }
    };
    let target = files.join(std::ffi::OsString::from_vec(chosen));
    if let Err(err) = move_entry(&absolute, &target) {
        let _ = std::fs::remove_file(&info_path);
        return Err(err);
    }
    Ok(Some(target))
}

/// The home trash if `path` is on the same device, else `.Trash-<uid>` at its mount point.
#[cfg(target_os = "linux")]
fn trash_dir_for(path: &Path, dev: u64) -> io::Result<PathBuf> {
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| dirs::home_dir().map(|home| home.join(".local/share")))
        .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
    let home_trash = data_home.join("Trash");
    let home_dev = nearest_existing(&home_trash).and_then(|p| std::fs::metadata(p).ok()).map(|m| m.dev());
    if home_dev == Some(dev) {
        return Ok(home_trash);
    }
    let top = drive_root(path).ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
    let uid = unsafe { libc::getuid() };
    let top_trash = top.join(format!(".Trash-{uid}"));
    match std::fs::symlink_metadata(&top_trash) {
        Ok(meta) if !meta.file_type().is_dir() || meta.uid() != uid => {
            Err(io::Error::new(io::ErrorKind::PermissionDenied, "unsafe trash folder"))
        }
        Ok(_) => Ok(top_trash),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(top_trash),
        Err(err) => Err(err),
    }
}

#[cfg(target_os = "linux")]
fn local_now() -> String {
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&now, &mut tm) };
    super::freedesktop::format_date(tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday, tm.tm_hour, tm.tm_min, tm.tm_sec)
}

pub fn restore(trashed: &Path, original: &Path) -> io::Result<()> {
    if let Some(parent) = original.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Never replaces: an existing `original` is AlreadyExists.
    move_entry(trashed, original)?;
    // freedesktop: drop `info/NAME.trashinfo` next to `files/NAME`.
    if cfg!(target_os = "linux")
        && let (Some(files), Some(name)) = (trashed.parent(), trashed.file_name())
        && files.file_name() == Some(std::ffi::OsStr::new("files"))
        && let Some(root) = files.parent()
    {
        let mut info = name.to_os_string();
        info.push(".trashinfo");
        let _ = std::fs::remove_file(root.join("info").join(info));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_numbers_split_like_glibc() {
        // makedev(8, 1) and makedev(259, 3).
        assert_eq!((dev_major(0x801), dev_minor(0x801)), (8, 1));
        let nvme = (259u64 << 8) | 3;
        assert_eq!((dev_major(nvme), dev_minor(nvme)), (259, 3));
    }
}
