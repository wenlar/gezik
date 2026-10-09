//! Windows: CopyFileExW with progress, POSIX deletes, volume facts.

use std::cell::{Cell, RefCell};
use std::ffi::{OsString, c_void};
use std::io;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use windows::Win32::Foundation::{CloseHandle, FILETIME, HANDLE};
use windows::Win32::Storage::FileSystem::{
    COPY_FILE_COPY_SYMLINK, COPY_FILE_FAIL_IF_EXISTS, COPY_FILE_NO_BUFFERING, COPYPROGRESSROUTINE_PROGRESS,
    CopyFileExW, CreateFileW, DELETE, DeleteFileW, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_HIDDEN,
    FILE_ATTRIBUTE_READONLY, FILE_ATTRIBUTE_REPARSE_POINT, FILE_DISPOSITION_FLAG_DELETE,
    FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE, FILE_DISPOSITION_FLAG_POSIX_SEMANTICS, FILE_DISPOSITION_INFO_EX,
    FILE_DISPOSITION_INFO_EX_FLAGS, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FIND_FIRST_EX_LARGE_FETCH,
    FileDispositionInfoEx, FindClose, FindExInfoBasic, FindExSearchNameMatch, FindFirstFileExW, FindNextFileW,
    GetDiskFreeSpaceExW, GetDriveTypeW, GetFileAttributesW, GetVolumeInformationW, GetVolumePathNameW,
    INVALID_FILE_ATTRIBUTES, LPPROGRESS_ROUTINE_CALLBACK_REASON, MOVE_FILE_FLAGS, MoveFileExW, OPEN_EXISTING,
    PROGRESS_CANCEL, PROGRESS_CONTINUE, RemoveDirectoryW, SetFileAttributesW, SetFileInformationByHandle,
    WIN32_FIND_DATAW,
};
use windows::Win32::System::IO::DeviceIoControl;
use windows::Win32::System::Ioctl::{
    DEVICE_SEEK_PENALTY_DESCRIPTOR, IOCTL_STORAGE_QUERY_PROPERTY, PropertyStandardQuery, STORAGE_PROPERTY_QUERY,
    StorageDeviceSeekPenaltyProperty,
};

use windows::Win32::Foundation::S_OK;
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
};
use windows::Win32::UI::Shell::{
    FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOCONFIRMMKDIR, FOF_NOERRORUI, FOF_SILENT, FOF_WANTNUKEWARNING,
    FOFX_EARLYFAILURE, FOFX_RECYCLEONDELETE, FileOperation, IFileOperation, IFileOperationProgressSink,
    IFileOperationProgressSink_Impl, IShellItem, SHCreateItemFromParsingName, SIGDN_FILESYSPATH,
};
use windows::core::{HRESULT, HSTRING, PCWSTR, Ref};

use super::{BIG_FILE, DiskKind, DriveFacts, cancelled};

const DRIVE_FIXED: u32 = 3;
const DRIVE_REMOTE: u32 = 4;

/// A Win32 error as an `io::Error` with its code (so `kind()` works). The Shell's own copy
/// engine errors (the Recycle Bin) become the Win32 code they stand for, so they read as
/// plainly; Windows has no text for them.
pub(crate) fn io_error(err: windows::core::Error) -> io::Error {
    let hr = err.code().0 as u32;
    // HRESULT_FROM_WIN32: 0x8007xxxx carries the Win32 error code.
    if hr & 0xFFFF_0000 == 0x8007_0000 {
        io::Error::from_raw_os_error((hr & 0xFFFF) as i32)
    } else if let Some(code) = copy_engine_code(err.code()) {
        io::Error::from_raw_os_error(code)
    } else {
        io::Error::other(err)
    }
}

/// The Win32 error a `COPYENGINE_E_*` result stands for.
fn copy_engine_code(hr: HRESULT) -> Option<i32> {
    use windows::Win32::UI::Shell::*;
    let is = |codes: &[HRESULT]| codes.contains(&hr);
    Some(if is(&[COPYENGINE_E_SHARING_VIOLATION_SRC, COPYENGINE_E_SHARING_VIOLATION_DEST]) {
        32 // ERROR_SHARING_VIOLATION
    } else if is(&[
        COPYENGINE_E_ACCESS_DENIED_SRC,
        COPYENGINE_E_ACCESS_DENIED_DEST,
        COPYENGINE_E_ACCESSDENIED_READONLY,
        COPYENGINE_E_REQUIRES_ELEVATION,
    ]) {
        5 // ERROR_ACCESS_DENIED
    } else if is(&[COPYENGINE_E_PATH_NOT_FOUND_SRC]) {
        2 // ERROR_FILE_NOT_FOUND
    } else if is(&[COPYENGINE_E_PATH_NOT_FOUND_DEST]) {
        3 // ERROR_PATH_NOT_FOUND
    } else if is(&[COPYENGINE_E_NET_DISCONNECT_SRC, COPYENGINE_E_NET_DISCONNECT_DEST]) {
        64 // ERROR_NETNAME_DELETED
    } else if is(&[COPYENGINE_E_DISK_FULL, COPYENGINE_E_DISK_FULL_CLEAN, COPYENGINE_E_REMOVABLE_FULL]) {
        112 // ERROR_DISK_FULL
    } else if is(&[
        COPYENGINE_E_ALREADY_EXISTS_NORMAL,
        COPYENGINE_E_ALREADY_EXISTS_READONLY,
        COPYENGINE_E_ALREADY_EXISTS_SYSTEM,
        COPYENGINE_E_ALREADY_EXISTS_FOLDER,
    ]) {
        183 // ERROR_ALREADY_EXISTS
    } else if is(&[COPYENGINE_E_DIR_NOT_EMPTY]) {
        145 // ERROR_DIR_NOT_EMPTY
    } else if is(&[
        COPYENGINE_E_PATH_TOO_DEEP_SRC,
        COPYENGINE_E_PATH_TOO_DEEP_DEST,
        COPYENGINE_E_NEWFILE_NAME_TOO_LONG,
        COPYENGINE_E_NEWFOLDER_NAME_TOO_LONG,
        COPYENGINE_E_RECYCLE_PATH_TOO_LONG,
    ]) {
        206 // ERROR_FILENAME_EXCED_RANGE
    } else {
        return None;
    })
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

/// The entry's volume serial and file id (the last part not followed, the folders on the way
/// are), for `same_entry`: the 128-bit id (FileIdInfo, unique on ReFS too), else the 64-bit
/// index where the file system knows no FileIdInfo (FAT). A serial or id of 0 is no id.
pub(super) fn entry_id(path: &Path) -> io::Result<(u64, u128)> {
    use windows::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, FILE_ID_INFO, FILE_READ_ATTRIBUTES, FileIdInfo, GetFileInformationByHandle,
        GetFileInformationByHandleEx,
    };
    let handle = unsafe {
        CreateFileW(
            &verbatim(path),
            FILE_READ_ATTRIBUTES.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            None,
        )
    }
    .map_err(io_error)?;
    let read = || -> io::Result<(u64, u128)> {
        let mut ex = FILE_ID_INFO::default();
        let by_ex = unsafe {
            GetFileInformationByHandleEx(
                handle,
                FileIdInfo,
                &mut ex as *mut FILE_ID_INFO as *mut c_void,
                size_of::<FILE_ID_INFO>() as u32,
            )
        };
        if by_ex.is_ok() {
            return Ok((ex.VolumeSerialNumber, u128::from_le_bytes(ex.FileId.Identifier)));
        }
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        unsafe { GetFileInformationByHandle(handle, &mut info) }.map_err(io_error)?;
        let index = (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow);
        Ok((u64::from(info.dwVolumeSerialNumber), u128::from(index)))
    };
    let id = read();
    let _ = unsafe { CloseHandle(handle) };
    match id? {
        (0, _) | (_, 0) => Err(io::Error::new(io::ErrorKind::Unsupported, "no file id")),
        id => Ok(id),
    }
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

/// Bytes this user may still write on the drive `path` is on (`path` must exist).
pub fn free_space(path: &Path) -> io::Result<u64> {
    let mut free = 0u64;
    unsafe { GetDiskFreeSpaceExW(&HSTRING::from(path.as_os_str()), Some(&mut free), None, None) }.map_err(io_error)?;
    Ok(free)
}

/// Whether `path` (which exists) is on a network drive: a share or a drive letter mapped to one.
pub fn is_network(path: &Path) -> io::Result<bool> {
    let root = drive_root(path).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no drive for this path"))?;
    Ok(root.to_string_lossy().starts_with(r"\\")
        || unsafe { GetDriveTypeW(&HSTRING::from(root.as_os_str())) } == DRIVE_REMOTE)
}

pub fn drive_facts(path: &Path) -> io::Result<DriveFacts> {
    let root = drive_root(path).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no drive for this path"))?;
    let root_text = root.to_string_lossy().into_owned();
    let root_wide = HSTRING::from(root.as_os_str());
    let drive_type = unsafe { GetDriveTypeW(&root_wide) };
    if drive_type == DRIVE_REMOTE || root_text.starts_with(r"\\") {
        return Ok(DriveFacts { id: root_text.to_lowercase(), kind: DiskKind::Network, trash: false, max_file: None });
    }
    let mut serial = 0u32;
    let mut file_system = [0u16; 64];
    unsafe { GetVolumeInformationW(&root_wide, None, Some(&mut serial), None, None, Some(&mut file_system)) }
        .map_err(io_error)?;
    let end = file_system.iter().position(|&c| c == 0).unwrap_or(file_system.len());
    let max_file = super::max_file_for(&String::from_utf16_lossy(&file_system[..end]));
    let kind = match seek_penalty(&root_text) {
        Some(true) => DiskKind::Hdd,
        Some(false) => DiskKind::Ssd,
        None => DiskKind::Unknown,
    };
    // Removable drives (USB sticks) and optical drives have no Recycle Bin.
    Ok(DriveFacts { id: format!("{serial:08x}"), kind, trash: drive_type == DRIVE_FIXED, max_file })
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

pub fn clear_hidden(path: &Path) -> io::Result<()> {
    let wide = verbatim(path);
    let attributes = unsafe { GetFileAttributesW(&wide) };
    if attributes == INVALID_FILE_ATTRIBUTES {
        return Err(io::Error::last_os_error());
    }
    unsafe { SetFileAttributesW(&wide, FILE_FLAGS_AND_ATTRIBUTES(attributes & !FILE_ATTRIBUTE_HIDDEN.0)) }
        .map_err(io_error)
}

/// Whether the hidden attribute is set.
pub fn is_hidden_attr(path: &Path) -> bool {
    let attributes = unsafe { GetFileAttributesW(&verbatim(path)) };
    attributes != INVALID_FILE_ATTRIBUTES && attributes & FILE_ATTRIBUTE_HIDDEN.0 != 0
}

/// Hears where each deleted item went in the Recycle Bin.
#[windows_core::implement(IFileOperationProgressSink)]
struct DeleteSink {
    trashed: Rc<RefCell<Option<PathBuf>>>,
    result: Rc<Cell<HRESULT>>,
}

impl IFileOperationProgressSink_Impl for DeleteSink_Impl {
    fn StartOperations(&self) -> windows::core::Result<()> {
        Ok(())
    }
    fn FinishOperations(&self, _: HRESULT) -> windows::core::Result<()> {
        Ok(())
    }
    fn PreRenameItem(&self, _: u32, _: Ref<IShellItem>, _: &PCWSTR) -> windows::core::Result<()> {
        Ok(())
    }
    fn PostRenameItem(
        &self,
        _: u32,
        _: Ref<IShellItem>,
        _: &PCWSTR,
        _: HRESULT,
        _: Ref<IShellItem>,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn PreMoveItem(&self, _: u32, _: Ref<IShellItem>, _: Ref<IShellItem>, _: &PCWSTR) -> windows::core::Result<()> {
        Ok(())
    }
    fn PostMoveItem(
        &self,
        _: u32,
        _: Ref<IShellItem>,
        _: Ref<IShellItem>,
        _: &PCWSTR,
        _: HRESULT,
        _: Ref<IShellItem>,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn PreCopyItem(&self, _: u32, _: Ref<IShellItem>, _: Ref<IShellItem>, _: &PCWSTR) -> windows::core::Result<()> {
        Ok(())
    }
    fn PostCopyItem(
        &self,
        _: u32,
        _: Ref<IShellItem>,
        _: Ref<IShellItem>,
        _: &PCWSTR,
        _: HRESULT,
        _: Ref<IShellItem>,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn PreDeleteItem(&self, _: u32, _: Ref<IShellItem>) -> windows::core::Result<()> {
        Ok(())
    }
    fn PostDeleteItem(
        &self,
        _: u32,
        _: Ref<IShellItem>,
        hr: HRESULT,
        created: Ref<IShellItem>,
    ) -> windows::core::Result<()> {
        self.result.set(hr);
        // `created` is the item in the Recycle Bin; none if it was deleted for good.
        if let Ok(item) = created.ok()
            && let Ok(name) = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH) }
        {
            let text = unsafe { name.to_string() }.unwrap_or_default();
            unsafe { CoTaskMemFree(Some(name.0 as *const c_void)) };
            if !text.is_empty() {
                *self.trashed.borrow_mut() = Some(PathBuf::from(text));
            }
        }
        Ok(())
    }
    fn PreNewItem(&self, _: u32, _: Ref<IShellItem>, _: &PCWSTR) -> windows::core::Result<()> {
        Ok(())
    }
    fn PostNewItem(
        &self,
        _: u32,
        _: Ref<IShellItem>,
        _: &PCWSTR,
        _: &PCWSTR,
        _: u32,
        _: HRESULT,
        _: Ref<IShellItem>,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn UpdateProgress(&self, _: u32, _: u32) -> windows::core::Result<()> {
        Ok(())
    }
    fn ResetTimer(&self) -> windows::core::Result<()> {
        Ok(())
    }
    fn PauseTimer(&self) -> windows::core::Result<()> {
        Ok(())
    }
    fn ResumeTimer(&self) -> windows::core::Result<()> {
        Ok(())
    }
}

/// Moves `path` to the Recycle Bin without any Windows dialog, except the one that asks before
/// deleting an item for good that does not fit in the Recycle Bin.
pub fn trash(path: &Path) -> io::Result<Option<PathBuf>> {
    // Better no trash than the wrong file (and the Shell refuses `\\?\` paths).
    if !super::can_trash_name(path) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "names ending in a dot or space cannot go to the Recycle Bin",
        ));
    }
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let operation: IFileOperation = CoCreateInstance(&FileOperation, None, CLSCTX_ALL).map_err(io_error)?;
        operation
            .SetOperationFlags(
                FOF_ALLOWUNDO
                    | FOF_NOCONFIRMATION
                    | FOF_SILENT
                    | FOF_NOERRORUI
                    | FOF_NOCONFIRMMKDIR
                    | FOF_WANTNUKEWARNING
                    | FOFX_RECYCLEONDELETE
                    | FOFX_EARLYFAILURE,
            )
            .map_err(io_error)?;
        let item: IShellItem = SHCreateItemFromParsingName(&HSTRING::from(path.as_os_str()), None).map_err(io_error)?;
        let trashed = Rc::new(RefCell::new(None));
        let result = Rc::new(Cell::new(S_OK));
        let sink: IFileOperationProgressSink = DeleteSink { trashed: trashed.clone(), result: result.clone() }.into();
        operation.DeleteItem(&item, &sink).map_err(io_error)?;
        operation.PerformOperations().map_err(io_error)?;
        if operation.GetAnyOperationsAborted().map_err(io_error)?.as_bool() {
            return Err(cancelled());
        }
        result.get().ok().map_err(io_error)?;
        let trashed = trashed.borrow_mut().take();
        if trashed.is_none() && std::fs::symlink_metadata(path).is_ok() {
            return Err(io::Error::other("the Recycle Bin did not take the item"));
        }
        Ok(trashed)
    }
}

/// Moves `trashed` (`…\$Recycle.Bin\…\$Rxxxxxx.ext`) back to `original`, never through a
/// junction or link on the way (`trash::check_way_back`).
pub fn restore(trashed: &Path, original: &Path) -> io::Result<()> {
    crate::trash::check_way_back(trashed, original)?;
    if let Some(parent) = original.parent() {
        std::fs::create_dir_all(parent)?;
    }
    crate::trash::check_way_back(trashed, original)?;
    move_entry(trashed, original)?;
    // The bin's record goes with it (without its entry it would list a broken item).
    if let Some(info) = crate::trash::info_file(trashed) {
        let _ = std::fs::remove_file(info);
    }
    Ok(())
}

/// The share a mapped drive letter stands for (`Z` → `\\server\share`); `None` for a drive
/// that is not mapped. A persistent mapping that is not connected now still says its share.
pub fn mapped_remote(letter: char) -> Option<String> {
    use windows::Win32::Foundation::{ERROR_CONNECTION_UNAVAIL, NO_ERROR};
    use windows::Win32::NetworkManagement::WNet::WNetGetConnectionW;
    use windows::core::PWSTR;
    if !letter.is_ascii_alphabetic() {
        return None;
    }
    let local = HSTRING::from(format!("{}:", letter.to_ascii_uppercase()));
    let mut buffer = [0u16; 1024];
    let mut len = buffer.len() as u32;
    let result = unsafe { WNetGetConnectionW(&local, Some(PWSTR(buffer.as_mut_ptr())), &mut len) };
    if result != NO_ERROR && result != ERROR_CONNECTION_UNAVAIL {
        return None;
    }
    let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    (end > 0).then(|| String::from_utf16_lossy(&buffer[..end]))
}

/// A folder's items in one pass (`FindFirstFileExW` with the basic info and large fetches): name,
/// attributes, size and times come with each, no call per item (spec 3.4). `wants_meta` is not
/// needed here.
pub fn read_dir_items(dir: &Path, _wants_meta: &dyn Fn(&str, bool) -> bool) -> io::Result<Vec<super::DirItem>> {
    const ERROR_FILE_NOT_FOUND: u32 = 2;
    let pattern = verbatim(&dir.join("*"));
    let mut data = WIN32_FIND_DATAW::default();
    // SAFETY: `data` is a WIN32_FIND_DATAW, as FindExInfoBasic fills.
    let found = unsafe {
        FindFirstFileExW(
            &pattern,
            FindExInfoBasic,
            (&mut data as *mut WIN32_FIND_DATAW).cast::<c_void>(),
            FindExSearchNameMatch,
            None,
            FIND_FIRST_EX_LARGE_FETCH,
        )
    };
    let handle = match found {
        Ok(handle) => handle,
        // A root with nothing in it (a fresh drive) has no `.` either.
        Err(err) if err.code() == windows::core::HRESULT::from_win32(ERROR_FILE_NOT_FOUND) => return Ok(Vec::new()),
        Err(err) => return Err(io_error(err)),
    };
    let mut items = Vec::new();
    loop {
        let end = data.cFileName.iter().position(|&c| c == 0).unwrap_or(data.cFileName.len());
        // A name with a lone surrogate is left out: a substituted one would act on another file.
        if let Ok(name) = String::from_utf16(&data.cFileName[..end])
            && name != "."
            && name != ".."
        {
            let attributes = data.dwFileAttributes;
            let is_dir = attributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0;
            let size = (u64::from(data.nFileSizeHigh) << 32) | u64::from(data.nFileSizeLow);
            let is_link = is_link_tag(attributes, data.dwReserved0);
            items.push(super::DirItem {
                name,
                is_dir,
                is_link,
                is_file: !is_dir && !is_link,
                offline: is_offline(attributes),
                flags: gezik_core::windows_flags(attributes),
                size: if is_dir { 0 } else { size },
                modified: file_time(data.ftLastWriteTime),
                created: file_time(data.ftCreationTime),
                device: 0,
                has_meta: true,
            });
        }
        // SAFETY: `handle` is open; FindNextFileW fills `data` the same way.
        if let Err(err) = unsafe { FindNextFileW(handle, &mut data) } {
            // SAFETY: opened above, closed once.
            unsafe {
                let _ = FindClose(handle);
            }
            const ERROR_NO_MORE_FILES: u32 = 18;
            if err.code() == windows::core::HRESULT::from_win32(ERROR_NO_MORE_FILES) {
                return Ok(items);
            }
            // A folder read only in part (a share that went away) counts as unread.
            return Err(io_error(err));
        }
    }
}

/// The name-surrogate bit of a reparse tag: the item stands for another place (a symbolic
/// link, a junction, a mounted volume). Cloud placeholders, deduplicated files and container
/// layers have tags without it and are real items.
const TAG_NAME_SURROGATE: u32 = 0x2000_0000;

/// Whether an item with these attributes and reparse tag (`dwReserved0` of the find data) is a
/// link the walk never goes into or reads.
pub(crate) fn is_link_tag(attributes: u32, tag: u32) -> bool {
    attributes & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 && tag & TAG_NAME_SURROGATE != 0
}

/// Whether reading the file would fetch its data from elsewhere first (OneDrive and other
/// cloud placeholders, offline files).
pub(crate) fn is_offline(attributes: u32) -> bool {
    gezik_core::windows_flags(attributes) & gezik_core::Entry::CLOUD_ONLY != 0
}

/// Opens `path` to read its text only if it is a regular file whose data is on this disk: not a
/// link and not a cloud placeholder (whose read would download it), checked before opening.
/// `Ok(None)` for the rest; the size comes with it.
pub fn open_regular(path: &Path) -> io::Result<Option<(std::fs::File, u64)>> {
    use std::os::windows::fs::MetadataExt;
    let meta = std::fs::symlink_metadata(path)?;
    if !meta.is_file() || is_offline(meta.file_attributes()) {
        return Ok(None);
    }
    let file = std::fs::File::open(path)?;
    Ok(Some((file, meta.len())))
}

/// A FILETIME (100 ns since 1601) as a time; `None` for zero.
pub(crate) fn file_time(time: FILETIME) -> Option<std::time::SystemTime> {
    let ticks = (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime);
    // 1601-01-01 to 1970-01-01 in 100 ns ticks.
    const UNIX_EPOCH_TICKS: u64 = 116_444_736_000_000_000;
    if ticks == 0 {
        return None;
    }
    let since = std::time::Duration::from_nanos((ticks.abs_diff(UNIX_EPOCH_TICKS)).saturating_mul(100));
    Some(if ticks >= UNIX_EPOCH_TICKS { std::time::UNIX_EPOCH + since } else { std::time::UNIX_EPOCH - since })
}

/// Windows needs no device rule (a mounted volume is a reparse point, never gone into).
pub fn device_of(_path: &Path) -> io::Result<u64> {
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }

    #[test]
    fn the_shell_s_copy_engine_errors_read_plainly() {
        use windows::Win32::UI::Shell::COPYENGINE_E_SHARING_VIOLATION_SRC;
        let err = io_error(windows::core::Error::from(COPYENGINE_E_SHARING_VIOLATION_SRC));
        assert_eq!(err.raw_os_error(), Some(32));
        assert_eq!(super::super::describe(&err), "It is open in another program");
        // One it does not know still reads as a sentence, not a bare number.
        let unknown = io_error(windows::core::Error::from(HRESULT(0x8027_00FFu32 as i32)));
        assert!(super::super::describe(&unknown).starts_with("Windows could not do it"), "{unknown}");
    }

    #[test]
    fn a_name_with_a_lone_surrogate_is_left_out() {
        use std::os::windows::ffi::OsStringExt;
        let dir = std::env::temp_dir().join(format!("gezik-platform-lone-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(std::ffi::OsString::from_wide(&[0x61, 0xD800])), "").unwrap();
        std::fs::write(dir.join("a\u{FFFD}"), "").unwrap();
        let names: Vec<String> = read_dir_items(&dir, &|_, _| true).unwrap().into_iter().map(|i| i.name).collect();
        assert_eq!(names, ["a\u{FFFD}"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hidden_attribute_can_be_set_and_cleared() {
        let dir = std::env::temp_dir().join(format!("gezik-platform-hidden-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(!is_hidden_attr(&dir));
        set_hidden(&dir).unwrap();
        assert!(is_hidden_attr(&dir));
        clear_hidden(&dir).unwrap();
        assert!(!is_hidden_attr(&dir));
        let _ = std::fs::remove_dir_all(&dir);
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

    #[test]
    fn only_name_surrogate_reparse_points_are_links() {
        const REPARSE: u32 = 0x400;
        assert!(is_link_tag(REPARSE, 0xA000_0003), "a junction or mounted volume");
        assert!(is_link_tag(REPARSE | 0x10, 0xA000_000C), "a symbolic link");
        assert!(!is_link_tag(REPARSE | 0x10, 0x9000_601A), "a OneDrive folder is gone into");
        assert!(!is_link_tag(REPARSE, 0x8000_0013), "a deduplicated file");
        assert!(!is_link_tag(REPARSE, 0x8000_0018), "a container (WCI) layer");
        assert!(!is_link_tag(0x10, 0xA000_0003), "no reparse point: the tag field means nothing");
    }

    #[test]
    fn cloud_placeholders_are_offline() {
        assert!(is_offline(0x0040_0000), "recall on data access (OneDrive online-only)");
        assert!(is_offline(0x0004_0000), "recall on open");
        assert!(is_offline(0x1000), "offline");
        assert!(!is_offline(0x20 | 0x400), "an archive bit and a reparse point alone");
    }
}
