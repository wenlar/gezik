//! Windows: CopyFileExW with progress, POSIX deletes, volume facts.

use std::ffi::{OsString, c_void};
use std::io;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Storage::FileSystem::{
    COPY_FILE_COPY_SYMLINK, COPY_FILE_FAIL_IF_EXISTS, COPY_FILE_NO_BUFFERING, COPYPROGRESSROUTINE_PROGRESS,
    CopyFileExW, CreateFileW, DELETE, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_READONLY,
    FILE_DISPOSITION_FLAG_DELETE, FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE,
    FILE_DISPOSITION_FLAG_POSIX_SEMANTICS, FILE_DISPOSITION_INFO_EX, FILE_DISPOSITION_INFO_EX_FLAGS,
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_DELETE,
    FILE_SHARE_READ, FILE_SHARE_WRITE, FileDispositionInfoEx, GetDriveTypeW, GetFileAttributesW, GetVolumeInformationW,
    GetVolumePathNameW, INVALID_FILE_ATTRIBUTES, LPPROGRESS_ROUTINE_CALLBACK_REASON, MOVE_FILE_FLAGS, MoveFileExW,
    OPEN_EXISTING, PROGRESS_CANCEL, PROGRESS_CONTINUE, SetFileAttributesW, SetFileInformationByHandle,
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

pub(crate) fn verbatim(path: &Path) -> HSTRING {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let wide: Vec<u16> = absolute.as_os_str().encode_wide().collect();
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
        if done.is_ok() {
            return Ok(());
        }
    }
    // FAT, exFAT and many network shares know no POSIX deletes: the old way.
    let attributes = unsafe { GetFileAttributesW(&wide) };
    if attributes != INVALID_FILE_ATTRIBUTES && attributes & FILE_ATTRIBUTE_READONLY.0 != 0 {
        let _ =
            unsafe { SetFileAttributesW(&wide, FILE_FLAGS_AND_ATTRIBUTES(attributes & !FILE_ATTRIBUTE_READONLY.0)) };
    }
    let is_dir = attributes != INVALID_FILE_ATTRIBUTES && attributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0;
    if is_dir { std::fs::remove_dir(path) } else { std::fs::remove_file(path) }
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
