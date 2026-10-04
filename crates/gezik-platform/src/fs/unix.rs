//! macOS and Linux: copying (clone, copy_file_range, buffered), moving without replacing,
//! device facts.

use std::fs::{File, FileTimes, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
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

pub fn set_hidden(_path: &Path) -> io::Result<()> {
    // A leading dot already hides it.
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
