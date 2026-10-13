//! What a file keeps besides its bytes, carried onto a new file that takes its place (an
//! archive written anew, spec 16 §6.2): permissions and owner, extended attributes or alternate
//! streams (Gezik's tags, the Mark of the Web, macOS quarantine), the hidden flag and the
//! creation time (Windows). And how many names (hard links) a file has.

use std::io;
use std::path::Path;

/// Gives `to` what `from` has besides its contents. Permissions are required (a private file
/// must not become readable by others); the rest is done as far as the drive allows.
#[cfg(unix)]
pub fn carry_facts(from: &Path, to: &Path) -> io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::metadata(from)?;
    if let (Ok(source), Ok(target)) = (std::fs::File::open(from), std::fs::OpenOptions::new().write(true).open(to)) {
        crate::meta::copy_xattrs(&source, &target);
    }
    // Only the owner itself or root may give a file away; a failure leaves it the user's.
    let _ = std::os::unix::fs::chown(to, Some(meta.uid()), Some(meta.gid()));
    // After chown, which may clear set-id bits.
    std::fs::set_permissions(to, meta.permissions())
}

#[cfg(windows)]
pub fn carry_facts(from: &Path, to: &Path) -> io::Result<()> {
    use std::os::windows::fs::{FileTimesExt, MetadataExt};
    use windows::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_SYSTEM, FILE_FLAGS_AND_ATTRIBUTES, SetFileAttributesW,
    };
    /// Streams larger than this are left behind (tags and marks are small).
    const MAX_STREAM: u64 = 16 << 20;
    let meta = std::fs::metadata(from)?;
    for (name, size) in streams(from) {
        if size > MAX_STREAM {
            continue;
        }
        let (mut source, mut target) = (from.as_os_str().to_owned(), to.as_os_str().to_owned());
        source.push(&name);
        target.push(&name);
        if let Ok(bytes) = std::fs::read(&source) {
            let _ = std::fs::write(&target, bytes);
        }
    }
    if let Ok(created) = meta.created()
        && let Ok(file) = std::fs::OpenOptions::new().write(true).open(to)
    {
        let _ = file.set_times(std::fs::FileTimes::new().set_created(created));
    }
    let kept = meta.file_attributes() & (FILE_ATTRIBUTE_HIDDEN.0 | FILE_ATTRIBUTE_SYSTEM.0);
    if kept != 0 {
        let now = std::fs::metadata(to)?.file_attributes();
        // SAFETY: a valid path string.
        unsafe { SetFileAttributesW(&super::verbatim(to), FILE_FLAGS_AND_ATTRIBUTES(now | kept)) }
            .map_err(super::windows::io_error)?;
    }
    Ok(())
}

/// The alternate data streams of `path` (`:Zone.Identifier`, `:gezik.meta`), not the file's
/// own contents, with their sizes.
#[cfg(windows)]
fn streams(path: &Path) -> Vec<(String, u64)> {
    use windows::Win32::Storage::FileSystem::{
        FindClose, FindFirstStreamW, FindNextStreamW, FindStreamInfoStandard, WIN32_FIND_STREAM_DATA,
    };
    let mut data = WIN32_FIND_STREAM_DATA::default();
    // SAFETY: a valid path string and a buffer of the type the info level asks for.
    let found =
        unsafe { FindFirstStreamW(&super::verbatim(path), FindStreamInfoStandard, (&raw mut data).cast(), None) };
    let Ok(handle) = found else { return Vec::new() };
    let mut out = Vec::new();
    loop {
        let len = data.cStreamName.iter().position(|&c| c == 0).unwrap_or(data.cStreamName.len());
        let name = String::from_utf16_lossy(&data.cStreamName[..len]);
        if let Some(stream) = name.strip_suffix(":$DATA").filter(|s| !s.is_empty() && *s != ":") {
            out.push((stream.to_owned(), u64::try_from(data.StreamSize).unwrap_or(u64::MAX)));
        }
        // SAFETY: the handle FindFirstStreamW gave, the same buffer.
        if unsafe { FindNextStreamW(handle, (&raw mut data).cast()) }.is_err() {
            break;
        }
    }
    // SAFETY: the handle is closed once.
    let _ = unsafe { FindClose(handle) };
    out
}

/// How many names the file at `path` has (hard links).
#[cfg(unix)]
pub fn link_count(path: &Path) -> io::Result<u64> {
    use std::os::unix::fs::MetadataExt;
    Ok(std::fs::metadata(path)?.nlink())
}

#[cfg(windows)]
pub fn link_count(path: &Path) -> io::Result<u64> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle};
    let file = std::fs::File::open(path)?;
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: an open handle and a buffer of the right type.
    unsafe { GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut info) }.map_err(super::windows::io_error)?;
    Ok(u64::from(info.nNumberOfLinks))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("gezik-carry-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_new_file_gets_the_old_ones_facts_and_links_are_counted() {
        let d = dir("facts");
        let (old, new) = (d.join("old.zip"), d.join("new.zip"));
        std::fs::write(&old, b"old").unwrap();
        std::fs::write(&new, b"new").unwrap();
        #[cfg(windows)]
        {
            let mut mark = old.as_os_str().to_owned();
            mark.push(":Zone.Identifier");
            std::fs::write(&mark, b"[ZoneTransfer]\r\nZoneId=3\r\n").unwrap();
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&old, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        carry_facts(&old, &new).unwrap();
        #[cfg(windows)]
        {
            let mut mark = new.as_os_str().to_owned();
            mark.push(":Zone.Identifier");
            assert_eq!(std::fs::read(&mark).unwrap(), b"[ZoneTransfer]\r\nZoneId=3\r\n");
            assert_eq!(
                std::fs::metadata(&new).unwrap().created().ok(),
                std::fs::metadata(&old).unwrap().created().ok()
            );
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&new).unwrap().permissions().mode() & 0o777, 0o600);
        }
        assert_eq!(std::fs::read(&new).unwrap(), b"new", "the contents stay");
        assert_eq!(link_count(&new).unwrap(), 1);
        std::fs::hard_link(&new, d.join("other.zip")).unwrap();
        assert_eq!(link_count(&new).unwrap(), 2);
        let _ = std::fs::remove_dir_all(&d);
    }
}
