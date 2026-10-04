//! Windows: CopyFileExW with progress, POSIX deletes, volume facts.

use std::ffi::{OsString, c_void};
use std::io;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Storage::FileSystem::{
    COPY_FILE_COPY_SYMLINK, COPY_FILE_FAIL_IF_EXISTS, COPY_FILE_NO_BUFFERING, COPYPROGRESSROUTINE_PROGRESS,
    CopyFileExW, CreateFileW, DELETE, DeleteFileW, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_HIDDEN,
    FILE_ATTRIBUTE_READONLY, FILE_DISPOSITION_FLAG_DELETE, FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE,
    FILE_DISPOSITION_FLAG_POSIX_SEMANTICS, FILE_DISPOSITION_INFO_EX, FILE_DISPOSITION_INFO_EX_FLAGS,
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_DELETE,
    FILE_SHARE_READ, FILE_SHARE_WRITE, FileDispositionInfoEx, GetDriveTypeW, GetFileAttributesW, GetVolumeInformationW,
    GetVolumePathNameW, INVALID_FILE_ATTRIBUTES, LPPROGRESS_ROUTINE_CALLBACK_REASON, MOVE_FILE_FLAGS, MoveFileExW,
    OPEN_EXISTING, PROGRESS_CANCEL, PROGRESS_CONTINUE, RemoveDirectoryW, SetFileAttributesW,
    SetFileInformationByHandle,
};
use windows::Win32::System::IO::DeviceIoControl;
use windows::Win32::System::Ioctl::{
    DEVICE_SEEK_PENALTY_DESCRIPTOR, IOCTL_STORAGE_QUERY_PROPERTY, PropertyStandardQuery, STORAGE_PROPERTY_QUERY,
    StorageDeviceSeekPenaltyProperty,
};
use windows::core::HSTRING;

use super::{BIG_FILE, DiskKind, DriveFacts, cancelled};

const DRIVE_FIXED: u32 = 3;
const DRIVE_REMOTE: u32 = 4;

/// A Win32 error as an `io::Error` with its code (so `kind()` works).
pub(crate) fn io_error(err: windows::core::Error) -> io::Error {
    let hr = err.code().0 as u32;
    // HRESULT_FROM_WIN32: 0x8007xxxx carries the Win32 error code.
    if hr & 0xFFFF_0000 == 0x8007_0000 {
        io::Error::from_raw_os_error((hr & 0xFFFF) as i32)
    } else {
        io::Error::other(err)
    }
}

/// `wide` with the `\\?\` prefix (`\\?\UNC\` for shares), so Win32 calls take paths longer
/// than 260 characters. Already prefixed paths stay as they are.
fn verbatim_wide(wide: &[u16]) -> Vec<u16> {
    let text: Vec<u16> = wide.to_vec();
    let starts = |prefix: &str| {
        let prefix: Vec<u16> = prefix.encode_utf16().collect();
        text.starts_with(&prefix)
    };
    if starts(r"\\?\") || starts(r"\\.\") {
        text
    } else if starts(r"\\") {
        let mut out: Vec<u16> = r"\\?\UNC".encode_utf16().collect();
        out.extend_from_slice(&text[1..]);
        out
    } else {
        let mut out: Vec<u16> = r"\\?\".encode_utf16().collect();
        out.extend_from_slice(&text);
        out
    }
}

/// Whether `wide` starts with `\\?\` or `\\.\`.
fn is_device_path(wide: &[u16]) -> bool {
    let starts = |prefix: &str| wide.starts_with(&prefix.encode_utf16().collect::<Vec<u16>>());
    starts(r"\\?\") || starts(r"\\.\")
}

/// Resolves `.` and `..` of an absolute path lexically, with `\` as the only separator.
/// Unlike `GetFullPathNameW` this keeps trailing dots and spaces (`foo.` and `foo ` are
/// names of their own, not `foo`). `None` for relative paths and for paths that already have
/// a `\\?\` or `\\.\` prefix.
fn normalize_absolute(wide: &[u16]) -> Option<Vec<u16>> {
    let sep = u16::from(b'\\');
    let text: Vec<u16> = wide.iter().map(|&c| if c == u16::from(b'/') { sep } else { c }).collect();
    let is_drive = text.len() >= 3
        && text[1] == u16::from(b':')
        && text[2] == sep
        && char::from_u32(u32::from(text[0])).is_some_and(|c| c.is_ascii_alphabetic());
    let is_unc = text.len() >= 2 && text[0] == sep && text[1] == sep;
    if !(is_drive || is_unc) || is_device_path(&text) {
        return None;
    }
    let mut parts = text.split(|&c| c == sep).filter(|part| !part.is_empty());
    // What `..` can never remove: `C:` or `\\server\share`.
    let mut out: Vec<u16> = Vec::with_capacity(text.len());
    if is_drive {
        out.extend_from_slice(parts.next()?);
    } else {
        out.extend_from_slice(&[sep, sep]);
        out.extend_from_slice(parts.next()?);
        if let Some(share) = parts.next() {
            out.push(sep);
            out.extend_from_slice(share);
        }
    }
    let mut names: Vec<&[u16]> = Vec::new();
    for part in parts {
        if part == [u16::from(b'.')] {
            continue;
        } else if part == [u16::from(b'.'), u16::from(b'.')] {
            names.pop();
        } else {
            names.push(part);
        }
    }
    for name in &names {
        out.push(sep);
        out.extend_from_slice(name);
    }
    if names.is_empty() && is_drive {
        out.push(sep);
    }
    Some(out)
}

pub(crate) fn verbatim(path: &Path) -> HSTRING {
    let wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    let wide = if let Some(normal) = normalize_absolute(&wide) {
        normal
    } else if is_device_path(&wide) {
        wide
    } else {
        // Relative: only now is it safe to let Windows resolve it against the current folder.
        let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        absolute.as_os_str().encode_wide().collect()
    };
    HSTRING::from_wide(&verbatim_wide(&wide))
}

pub fn copy_file(from: &Path, to: &Path, size: u64, progress: &mut dyn FnMut(u64) -> bool) -> io::Result<()> {
    struct Context<'a> {
        progress: &'a mut dyn FnMut(u64) -> bool,
        cancelled: bool,
    }

    #[allow(clippy::too_many_arguments, reason = "the CopyFileExW callback signature")]
    unsafe extern "system" fn routine(
        _total: i64,
        transferred: i64,
        _stream_size: i64,
        _stream_transferred: i64,
        _stream: u32,
        _reason: LPPROGRESS_ROUTINE_CALLBACK_REASON,
        _source: HANDLE,
        _target: HANDLE,
        data: *const c_void,
    ) -> COPYPROGRESSROUTINE_PROGRESS {
        // SAFETY: `data` is the `Context` below, alive for the whole CopyFileExW call.
        let context = unsafe { &mut *(data as *mut Context<'_>) };
        if (context.progress)(u64::try_from(transferred).unwrap_or(0)) {
            PROGRESS_CONTINUE
        } else {
            context.cancelled = true;
            PROGRESS_CANCEL
        }
    }

    let mut flags = COPY_FILE_FAIL_IF_EXISTS | COPY_FILE_COPY_SYMLINK;
    if size >= BIG_FILE {
        flags |= COPY_FILE_NO_BUFFERING;
    }
    let mut context = Context { progress, cancelled: false };
    let data = &mut context as *mut Context<'_> as *const c_void;
    // CopyFileExW removes a half-written copy itself when cancelled or failed.
    let result = unsafe { CopyFileExW(&verbatim(from), &verbatim(to), Some(routine), Some(data), None, flags) };
    match result {
        Ok(()) => Ok(()),
        Err(_) if context.cancelled => Err(cancelled()),
        Err(err) => Err(io_error(err)),
    }
}

pub fn move_entry(from: &Path, to: &Path) -> io::Result<()> {
    // No MOVEFILE_REPLACE_EXISTING (an existing target fails) and no MOVEFILE_COPY_ALLOWED
    // (another drive fails): this is a rename, nothing else.
    unsafe { MoveFileExW(&verbatim(from), &verbatim(to), MOVE_FILE_FLAGS(0)) }.map_err(io_error)
}

pub fn delete(path: &Path) -> io::Result<()> {
    let wide = verbatim(path);
    let opened = unsafe {
        CreateFileW(
            &wide,
            DELETE.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            None,
        )
    };
    if let Ok(handle) = opened {
        // POSIX semantics: the name goes at once, even if another program still has the file
        // open, so the folder can be removed right after.
        let info = FILE_DISPOSITION_INFO_EX {
            Flags: FILE_DISPOSITION_INFO_EX_FLAGS(
                FILE_DISPOSITION_FLAG_DELETE.0
                    | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS.0
                    | FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE.0,
            ),
        };
        let done = unsafe {
            SetFileInformationByHandle(
                handle,
                FileDispositionInfoEx,
                &info as *const FILE_DISPOSITION_INFO_EX as *const c_void,
                size_of::<FILE_DISPOSITION_INFO_EX>() as u32,
            )
        };
        let _ = unsafe { CloseHandle(handle) };
        match done {
            Ok(()) => return Ok(()),
            Err(err) => {
                let err = io_error(err);
                // ERROR_INVALID_FUNCTION, ERROR_NOT_SUPPORTED, ERROR_INVALID_PARAMETER: the file
                // system knows no POSIX deletes. Anything else (sharing violation, folder not
                // empty, access denied) is the real answer.
                if !err.raw_os_error().is_some_and(|code| [1, 50, 87].contains(&code)) {
                    return Err(err);
                }
            }
        }
    }
    // FAT, exFAT and many network shares know no POSIX deletes: the old way.
    let attributes = unsafe { GetFileAttributesW(&wide) };
    let cleared = attributes != INVALID_FILE_ATTRIBUTES
        && attributes & FILE_ATTRIBUTE_READONLY.0 != 0
        && unsafe { SetFileAttributesW(&wide, FILE_FLAGS_AND_ATTRIBUTES(attributes & !FILE_ATTRIBUTE_READONLY.0)) }
            .is_ok();
    let is_dir = attributes != INVALID_FILE_ATTRIBUTES && attributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0;
    let result = if is_dir { unsafe { RemoveDirectoryW(&wide) } } else { unsafe { DeleteFileW(&wide) } };
    if result.is_err() && cleared {
        let _ = unsafe { SetFileAttributesW(&wide, FILE_FLAGS_AND_ATTRIBUTES(attributes)) };
    }
    result.map_err(io_error)
}

pub fn drive_root(path: &Path) -> Option<PathBuf> {
    let mut buffer = [0u16; 1024];
    unsafe { GetVolumePathNameW(&HSTRING::from(path.as_os_str()), &mut buffer) }.ok()?;
    let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    Some(PathBuf::from(OsString::from_wide(&buffer[..end])))
}

pub fn drive_facts(path: &Path) -> io::Result<DriveFacts> {
    let root = drive_root(path).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no drive for this path"))?;
    let root_text = root.to_string_lossy().into_owned();
    let root_wide = HSTRING::from(root.as_os_str());
    let drive_type = unsafe { GetDriveTypeW(&root_wide) };
    if drive_type == DRIVE_REMOTE || root_text.starts_with(r"\\") {
        return Ok(DriveFacts { id: root_text.to_lowercase(), kind: DiskKind::Network, trash: false });
    }
    let mut serial = 0u32;
    unsafe { GetVolumeInformationW(&root_wide, None, Some(&mut serial), None, None, None) }.map_err(io_error)?;
    let kind = match seek_penalty(&root_text) {
        Some(true) => DiskKind::Hdd,
        Some(false) => DiskKind::Ssd,
        None => DiskKind::Unknown,
    };
    // Removable drives (USB sticks) and optical drives have no Recycle Bin.
    Ok(DriveFacts { id: format!("{serial:08x}"), kind, trash: drive_type == DRIVE_FIXED })
}

/// Whether the disk behind drive `root` (`C:\`) has to seek, like a spinning disk does.
/// `None` for folders mounted as drives and when Windows does not say.
fn seek_penalty(root: &str) -> Option<bool> {
    let letter = root.strip_suffix('\\').unwrap_or(root);
    if letter.len() != 2 || !letter.ends_with(':') {
        return None;
    }
    let device = HSTRING::from(format!(r"\\.\{letter}"));
    // No access rights needed: the query only reads properties.
    let handle = unsafe {
        CreateFileW(
            &device,
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            None,
        )
    }
    .ok()?;
    let query = STORAGE_PROPERTY_QUERY {
        PropertyId: StorageDeviceSeekPenaltyProperty,
        QueryType: PropertyStandardQuery,
        AdditionalParameters: [0],
    };
    let mut answer = DEVICE_SEEK_PENALTY_DESCRIPTOR::default();
    let mut returned = 0u32;
    let asked = unsafe {
        DeviceIoControl(
            handle,
            IOCTL_STORAGE_QUERY_PROPERTY,
            Some(&query as *const STORAGE_PROPERTY_QUERY as *const c_void),
            size_of::<STORAGE_PROPERTY_QUERY>() as u32,
            Some(&mut answer as *mut DEVICE_SEEK_PENALTY_DESCRIPTOR as *mut c_void),
            size_of::<DEVICE_SEEK_PENALTY_DESCRIPTOR>() as u32,
            Some(&mut returned),
            None,
        )
    };
    let _ = unsafe { CloseHandle(handle) };
    asked.ok()?;
    if (returned as usize) < size_of::<DEVICE_SEEK_PENALTY_DESCRIPTOR>() {
        return None;
    }
    Some(answer.IncursSeekPenalty)
}

pub fn set_hidden(path: &Path) -> io::Result<()> {
    let wide = verbatim(path);
    let attributes = unsafe { GetFileAttributesW(&wide) };
    if attributes == INVALID_FILE_ATTRIBUTES {
        return Err(io::Error::last_os_error());
    }
    unsafe { SetFileAttributesW(&wide, FILE_FLAGS_AND_ATTRIBUTES(attributes | FILE_ATTRIBUTE_HIDDEN.0)) }
        .map_err(io_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }

    #[test]
    fn verbatim_prefixes_local_and_share_paths_once() {
        assert_eq!(verbatim_wide(&wide(r"C:\a\b")), wide(r"\\?\C:\a\b"));
        assert_eq!(verbatim_wide(&wide(r"\\server\share\x")), wide(r"\\?\UNC\server\share\x"));
        assert_eq!(verbatim_wide(&wide(r"\\?\C:\a")), wide(r"\\?\C:\a"));
    }

    #[test]
    fn normalize_resolves_dots_lexically_and_keeps_trailing_dots_and_spaces() {
        let n = |text: &str| normalize_absolute(&wide(text)).map(|w| String::from_utf16(&w).unwrap());
        assert_eq!(n(r"C:\x\foo.").as_deref(), Some(r"C:\x\foo."));
        assert_eq!(n(r"C:\x\foo ").as_deref(), Some(r"C:\x\foo "));
        assert_eq!(n(r"C:/x/./y/../z").as_deref(), Some(r"C:\x\z"));
        assert_eq!(n(r"C:\..").as_deref(), Some(r"C:\"));
        assert_eq!(n(r"\\server\share\a\..\b").as_deref(), Some(r"\\server\share\b"));
        assert_eq!(n(r"\\?\C:\a\.."), None);
        assert_eq!(n(r"relative\x"), None);
    }

    #[test]
    fn names_with_trailing_dots_and_spaces_are_their_own_names() {
        let dir = super::super::test_dir("trailing");
        let root = verbatim(&dir).to_string();
        let (plain_dot, dotted) = (PathBuf::from(format!(r"{root}\a")), PathBuf::from(format!(r"{root}\a.")));
        let (plain_space, spaced) = (PathBuf::from(format!(r"{root}\b")), PathBuf::from(format!(r"{root}\b ")));
        std::fs::write(&plain_dot, "plain a").unwrap();
        std::fs::write(&dotted, "dotted a").unwrap();
        std::fs::write(&plain_space, "plain b").unwrap();
        std::fs::write(&spaced, "spaced b").unwrap();
        // The same names the way a user's path spells them: without the `\\?\` prefix.
        let (user_dotted, user_spaced) = (dir.join("a."), dir.join("b "));
        let copy = PathBuf::from(format!(r"{root}\copy"));
        copy_file(&user_dotted, &copy, 8, &mut |_| true).unwrap();
        assert_eq!(std::fs::read_to_string(&copy).unwrap(), "dotted a");
        delete(&user_dotted).unwrap();
        delete(&user_spaced).unwrap();
        assert!(std::fs::metadata(&dotted).is_err() && std::fs::metadata(&spaced).is_err());
        assert_eq!(std::fs::read_to_string(&plain_dot).unwrap(), "plain a");
        assert_eq!(std::fs::read_to_string(&plain_space).unwrap(), "plain b");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_read_only_folder_with_something_in_it_fails_and_stays_read_only() {
        let dir = super::super::test_dir("readonly-folder");
        let folder = dir.join("locked");
        std::fs::create_dir(&folder).unwrap();
        std::fs::write(folder.join("x"), "x").unwrap();
        let mut permissions = std::fs::metadata(&folder).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&folder, permissions).unwrap();
        assert!(delete(&folder).is_err());
        assert!(std::fs::metadata(&folder).unwrap().permissions().readonly(), "still read-only");
        assert!(folder.join("x").exists());
        let mut permissions = std::fs::metadata(&folder).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false, reason = "Windows only: this clears the attribute")]
        permissions.set_readonly(false);
        std::fs::set_permissions(&folder, permissions).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn long_paths_work() {
        let dir = super::super::test_dir("long");
        let mut deep = dir.clone();
        for i in 0..12 {
            deep.push(format!("{i:02}-{}", "x".repeat(24)));
        }
        std::fs::create_dir_all(&deep).unwrap();
        let (from, to) = (deep.join("a.txt"), deep.join("b.txt"));
        assert!(to.as_os_str().len() > 300);
        std::fs::write(&from, "x").unwrap();
        copy_file(&from, &to, 1, &mut |_| true).unwrap();
        delete(&to).unwrap();
        assert!(!to.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_temp_folder_is_on_a_fixed_drive_with_a_recycle_bin() {
        let facts = drive_facts(&std::env::temp_dir()).unwrap();
        assert!(facts.trash, "{facts:?}");
        assert_eq!(facts.id.len(), 8);
    }
}
