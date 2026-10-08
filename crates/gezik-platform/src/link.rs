//! Making links (spec 9.2): Windows shortcuts (`.lnk`, as Explorer makes them), junctions and
//! symbolic links; symbolic links elsewhere.

use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicU8, Ordering};

pub use gezik_core::templates::LinkKind;

/// Whether symbolic links can be made here: 0 not known yet, 1 yes, 2 no. Windows asks for
/// Developer Mode or an administrator.
static SYMLINKS: AtomicU8 = AtomicU8::new(0);

/// Whether the menus offer symbolic links: always off Windows; on Windows once
/// `probe_symlinks` found that they can be made.
pub fn symlinks_allowed() -> bool {
    !cfg!(windows) || SYMLINKS.load(Ordering::Relaxed) == 1
}

/// Tries once whether a symbolic link can be made (Windows: one in the temp folder, removed
/// at once); off Windows nothing to try. Called once on a thread of its own at start.
pub fn probe_symlinks() {
    #[cfg(windows)]
    {
        let dir = std::env::temp_dir();
        let link = dir.join(format!("gezik-symlink-probe-{}", std::process::id()));
        let _ = std::fs::remove_file(&link);
        // std tries with SYMBOLIC_LINK_FLAG_ALLOW_UNPRIVILEGED_CREATE (Developer Mode) first.
        let made = std::os::windows::fs::symlink_file(dir.join("gezik-symlink-probe-target"), &link).is_ok();
        if made {
            let _ = std::fs::remove_file(&link);
        }
        SYMLINKS.store(if made { 1 } else { 2 }, Ordering::Relaxed);
    }
}

/// Whether a junction can be made in `dir`: Windows, a local NTFS or ReFS drive. Reads the
/// volume: not for the UI thread.
pub fn junctions_supported(dir: &Path) -> bool {
    imp::junctions_supported(dir)
}

/// Makes a link of `kind` at `at` to `target` (absolute; `target_is_dir`: a folder).
pub fn create(kind: LinkKind, target: &Path, at: &Path, target_is_dir: bool) -> io::Result<()> {
    let target = std::path::absolute(target)?;
    match kind {
        LinkKind::Symlink => symlink(&target, at, target_is_dir),
        LinkKind::Shortcut => imp::shortcut(&target, at),
        LinkKind::Junction => imp::junction(&target, at),
    }
}

#[cfg(windows)]
pub use imp::read_shortcut;

#[cfg(unix)]
fn symlink(target: &Path, at: &Path, _target_is_dir: bool) -> io::Result<()> {
    std::os::unix::fs::symlink(target, at)
}

#[cfg(windows)]
fn symlink(target: &Path, at: &Path, target_is_dir: bool) -> io::Result<()> {
    if target_is_dir {
        std::os::windows::fs::symlink_dir(target, at)
    } else {
        std::os::windows::fs::symlink_file(target, at)
    }
}

#[cfg(not(windows))]
mod imp {
    use std::io;
    use std::path::Path;

    fn only_windows() -> io::Error {
        io::Error::new(io::ErrorKind::Unsupported, "Only on Windows")
    }

    pub(super) fn shortcut(_target: &Path, _at: &Path) -> io::Result<()> {
        Err(only_windows())
    }

    pub(super) fn junction(_target: &Path, _at: &Path) -> io::Result<()> {
        Err(only_windows())
    }

    pub(super) fn junctions_supported(_dir: &Path) -> bool {
        false
    }
}

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;
    use std::io;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::path::{Path, PathBuf};

    use windows::Win32::Foundation::{CloseHandle, GENERIC_WRITE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
        GetVolumeInformationW, OPEN_EXISTING,
    };
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, IPersistFile, STGM_READ,
    };
    use windows::Win32::System::IO::DeviceIoControl;
    use windows::Win32::System::Ioctl::FSCTL_SET_REPARSE_POINT;
    use windows::Win32::System::SystemServices::IO_REPARSE_TAG_MOUNT_POINT;
    use windows::Win32::UI::Shell::{IShellLinkW, SLGP_RAWPATH, ShellLink};
    use windows::core::{HSTRING, Interface};

    use crate::fs::{io_error, verbatim};

    /// The biggest reparse data Windows takes (MAXIMUM_REPARSE_DATA_BUFFER_SIZE).
    const MAX_REPARSE: usize = 16 * 1024;

    fn shell_link() -> io::Result<IShellLinkW> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).map_err(io_error)
        }
    }

    /// A shortcut as Explorer's "Create shortcut" makes it: the target and its folder as the
    /// working folder.
    pub(super) fn shortcut(target: &Path, at: &Path) -> io::Result<()> {
        let link = shell_link()?;
        unsafe {
            link.SetPath(&HSTRING::from(target.as_os_str())).map_err(io_error)?;
            if let Some(dir) = target.parent() {
                link.SetWorkingDirectory(&HSTRING::from(dir.as_os_str())).map_err(io_error)?;
            }
            let file: IPersistFile = link.cast().map_err(io_error)?;
            file.Save(&HSTRING::from(at.as_os_str()), true).map_err(io_error)
        }
    }

    /// Where shortcut `lnk` points.
    pub fn read_shortcut(lnk: &Path) -> io::Result<PathBuf> {
        let link = shell_link()?;
        let mut buffer = vec![0u16; 32 * 1024];
        unsafe {
            let file: IPersistFile = link.cast().map_err(io_error)?;
            file.Load(&HSTRING::from(lnk.as_os_str()), STGM_READ).map_err(io_error)?;
            link.GetPath(&mut buffer, std::ptr::null_mut(), SLGP_RAWPATH.0 as u32).map_err(io_error)?;
        }
        let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        Ok(PathBuf::from(std::ffi::OsString::from_wide(&buffer[..end])))
    }

    /// A junction: a new empty folder made a mount point that leads to `target`.
    pub(super) fn junction(target: &Path, at: &Path) -> io::Result<()> {
        let buffer = mount_point_buffer(target)?;
        std::fs::create_dir(at)?;
        let set = set_reparse_point(at, &buffer);
        if set.is_err() {
            let _ = std::fs::remove_dir(at);
        }
        set
    }

    /// The REPARSE_DATA_BUFFER of a mount point to `target` (`C:\…`, or `\\?\C:\…`): the
    /// substitute name `\??\C:\…` and the print name `C:\…`, each ending in a NUL.
    pub(crate) fn mount_point_buffer(target: &Path) -> io::Result<Vec<u8>> {
        let mut print: Vec<u16> = target.as_os_str().encode_wide().collect();
        let verbatim_prefix: Vec<u16> = r"\\?\".encode_utf16().collect();
        if print.starts_with(&verbatim_prefix) && print.get(5) == Some(&u16::from(b':')) {
            print.drain(..verbatim_prefix.len());
        }
        if print.get(1) != Some(&u16::from(b':')) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "A junction can only point to a local folder"));
        }
        let substitute: Vec<u16> = r"\??\".encode_utf16().chain(print.iter().copied()).collect();
        let (substitute_bytes, print_bytes) = (substitute.len() * 2, print.len() * 2);
        // The four offsets and lengths, then both names with their NULs.
        let data_length = 8 + substitute_bytes + 2 + print_bytes + 2;
        if 8 + data_length > MAX_REPARSE {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "The folder's path is too long for a junction"));
        }
        let half = |n: usize| (n as u16).to_le_bytes();
        let mut out = Vec::with_capacity(8 + data_length);
        out.extend_from_slice(&IO_REPARSE_TAG_MOUNT_POINT.to_le_bytes());
        out.extend_from_slice(&half(data_length));
        out.extend_from_slice(&half(0)); // Reserved
        out.extend_from_slice(&half(0)); // SubstituteNameOffset
        out.extend_from_slice(&half(substitute_bytes));
        out.extend_from_slice(&half(substitute_bytes + 2)); // PrintNameOffset
        out.extend_from_slice(&half(print_bytes));
        for unit in substitute.iter().chain(&[0]).chain(&print).chain(&[0]) {
            out.extend_from_slice(&unit.to_le_bytes());
        }
        Ok(out)
    }

    fn set_reparse_point(at: &Path, buffer: &[u8]) -> io::Result<()> {
        let handle = unsafe {
            CreateFileW(
                &verbatim(at),
                GENERIC_WRITE.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                None,
            )
        }
        .map_err(io_error)?;
        let mut returned = 0u32;
        let set = unsafe {
            DeviceIoControl(
                handle,
                FSCTL_SET_REPARSE_POINT,
                Some(buffer.as_ptr() as *const c_void),
                buffer.len() as u32,
                None,
                0,
                Some(&mut returned),
                None,
            )
        };
        let _ = unsafe { CloseHandle(handle) };
        set.map_err(io_error)
    }

    pub(super) fn junctions_supported(dir: &Path) -> bool {
        let Some(root) = crate::fs::drive_root(dir) else { return false };
        if root.to_string_lossy().starts_with(r"\\") || crate::fs::is_network(dir).unwrap_or(true) {
            return false;
        }
        let mut name = [0u16; 64];
        let read =
            unsafe { GetVolumeInformationW(&HSTRING::from(root.as_os_str()), None, None, None, None, Some(&mut name)) };
        let end = name.iter().position(|&c| c == 0).unwrap_or(name.len());
        read.is_ok() && matches!(String::from_utf16_lossy(&name[..end]).to_ascii_uppercase().as_str(), "NTFS" | "REFS")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gezik-link-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("target")).unwrap();
        std::fs::write(dir.join("target/inside.txt"), "kept").unwrap();
        dir
    }

    #[cfg(windows)]
    #[test]
    fn a_shortcut_points_at_its_target() {
        let dir = dir("shortcut");
        let lnk = dir.join("target - Shortcut.lnk");
        create(LinkKind::Shortcut, &dir.join("target"), &lnk, true).unwrap();
        assert_eq!(read_shortcut(&lnk).unwrap(), dir.join("target"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn a_junction_leads_into_its_folder_and_goes_alone() {
        let dir = dir("junction");
        assert!(junctions_supported(&dir), "the temp folder is on a local NTFS drive");
        let link = dir.join("Link to target");
        create(LinkKind::Junction, &dir.join("target"), &link, true).unwrap();
        assert_eq!(std::fs::read_to_string(link.join("inside.txt")).unwrap(), "kept");
        crate::fs::delete(&link).unwrap();
        assert!(std::fs::symlink_metadata(&link).is_err());
        assert_eq!(std::fs::read_to_string(dir.join("target/inside.txt")).unwrap(), "kept");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn a_junction_buffer_names_the_folder_twice() {
        let buffer = imp::mount_point_buffer(Path::new(r"C:\a")).unwrap();
        let word = |at: usize| u16::from_le_bytes([buffer[at], buffer[at + 1]]);
        assert_eq!(&buffer[..4], &0xA000_0003u32.to_le_bytes(), "IO_REPARSE_TAG_MOUNT_POINT");
        // \??\C:\a (8 units) + NUL, C:\a (4 units) + NUL, after 8 bytes of offsets.
        assert_eq!(usize::from(word(4)), 8 + (8 + 1 + 4 + 1) * 2, "ReparseDataLength");
        assert_eq!((word(8), word(10), word(12), word(14)), (0, 16, 18, 8));
        assert_eq!(buffer.len(), 8 + usize::from(word(4)));
        assert!(imp::mount_point_buffer(Path::new(r"\\server\share\x")).is_err(), "only local folders");
        let verbatim = imp::mount_point_buffer(Path::new(r"\\?\C:\a")).unwrap();
        assert_eq!(verbatim, buffer, "a \\\\?\\ path is the same folder");
    }

    #[test]
    fn a_symbolic_link_leads_to_its_target_when_allowed() {
        probe_symlinks();
        if !symlinks_allowed() {
            eprintln!("symbolic links need Developer Mode here: skipped");
            return;
        }
        let dir = dir("symlink");
        let link = dir.join("Link to target");
        create(LinkKind::Symlink, &dir.join("target"), &link, true).unwrap();
        assert_eq!(std::fs::read_to_string(link.join("inside.txt")).unwrap(), "kept");
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(not(windows))]
    #[test]
    fn only_symbolic_links_off_windows() {
        let dir = dir("unix-kinds");
        assert!(create(LinkKind::Shortcut, &dir.join("target"), &dir.join("x.lnk"), true).is_err());
        assert!(create(LinkKind::Junction, &dir.join("target"), &dir.join("j"), true).is_err());
        assert!(!junctions_supported(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
