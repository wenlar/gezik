//! `secure` on Windows: every handle opened on the item itself (`FILE_FLAG_OPEN_REPARSE_POINT`),
//! shared for reading and writing but never for deleting (while held, neither it nor a folder
//! above it can be renamed), and checked to be where its path says
//! (`GetFinalPathNameByHandleW`): a junction, symlink, mount point, short name or mapped drive on
//! the way stops the work. Deleting and renaming go through the handle.

use std::ffi::c_void;
use std::fs::File;
use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::path::{Component, Path, Prefix};

use windows::Win32::Foundation::{FILETIME, GENERIC_READ, GENERIC_WRITE, HANDLE};
use windows::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, CREATE_NEW, CreateDirectoryW, CreateFileW, DELETE, FILE_ATTRIBUTE_DIRECTORY,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_CREATION_DISPOSITION, FILE_DISPOSITION_FLAG_DELETE,
    FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE, FILE_DISPOSITION_FLAG_POSIX_SEMANTICS, FILE_DISPOSITION_INFO,
    FILE_DISPOSITION_INFO_EX, FILE_DISPOSITION_INFO_EX_FLAGS, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_LIST_DIRECTORY, FILE_NAME_NORMALIZED, FILE_READ_ATTRIBUTES, FILE_RENAME_INFO, FILE_SHARE_READ,
    FILE_SHARE_WRITE, FILE_TRAVERSE, FILE_WRITE_ATTRIBUTES, FileDispositionInfo, FileDispositionInfoEx, FileRenameInfo,
    GETFINALPATHNAMEBYHANDLE_FLAGS, GetFileInformationByHandle, GetFileTime, GetFinalPathNameByHandleW, OPEN_EXISTING,
    SYNCHRONIZE, SetFileInformationByHandle, SetFileTime, VOLUME_NAME_DOS,
};

use super::secure::{
    CHANGED, FOLDER_THERE, Guard, LINK_COPY, LINK_ON_THE_WAY, MAX_DEPTH, NOT_A_FOLDER, NOT_PLAIN, SEVERAL_NAMES,
    TOO_DEEP, random_name, refused,
};
use super::windows::{io_error, verbatim};

/// Volume serial number and file index.
pub(crate) type Id = (u32, u64);

const LOOK: u32 = FILE_READ_ATTRIBUTES.0 | SYNCHRONIZE.0;
const FOLDER: u32 = FILE_LIST_DIRECTORY.0 | FILE_TRAVERSE.0 | LOOK;
const REMOVE: u32 = DELETE.0 | FILE_WRITE_ATTRIBUTES.0 | LOOK;

fn h(file: &File) -> HANDLE {
    HANDLE(file.as_raw_handle())
}

/// `C:\…` and nothing else: no network path, no `\\?\` or `\\.\`, no `..`.
fn plain(path: &Path) -> io::Result<()> {
    let mut parts = path.components();
    let ok = matches!(parts.next(), Some(Component::Prefix(p)) if matches!(p.kind(), Prefix::Disk(_)))
        && matches!(parts.next(), Some(Component::RootDir))
        && parts.all(|part| matches!(part, Component::Normal(_)));
    if ok { Ok(()) } else { Err(refused(NOT_PLAIN)) }
}

/// The folder `path` is in, for an item that has a name.
fn parent(path: &Path) -> io::Result<&Path> {
    plain(path)?;
    path.parent().filter(|_| path.file_name().is_some()).ok_or_else(|| refused("Not an item in a folder"))
}

/// `path` itself (a link is opened, not followed), for `access`.
fn open(path: &Path, access: u32, how: FILE_CREATION_DISPOSITION) -> io::Result<File> {
    // SAFETY: the path lives through the call; the handle is owned by the File.
    let handle = unsafe {
        CreateFileW(
            &verbatim(path),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            how,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            None,
        )
    }
    .map_err(io_error)?;
    Ok(unsafe { File::from_raw_handle(handle.0) })
}

/// Where the handle really is, as `C:\…` (a network drive comes back as `UNC\…`).
fn final_path(file: &File) -> io::Result<String> {
    let mut buffer = vec![0u16; 512];
    loop {
        // SAFETY: the buffer holds its length.
        let len = unsafe {
            GetFinalPathNameByHandleW(
                h(file),
                &mut buffer,
                GETFINALPATHNAMEBYHANDLE_FLAGS(FILE_NAME_NORMALIZED.0 | VOLUME_NAME_DOS.0),
            )
        } as usize;
        if len == 0 {
            return Err(io::Error::last_os_error());
        }
        if len < buffer.len() {
            buffer.truncate(len);
            break;
        }
        buffer.resize(len + 1, 0);
    }
    let text = String::from_utf16(&buffer).map_err(|_| refused(LINK_ON_THE_WAY))?;
    Ok(text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned())
}

/// The handle is at `path` itself: no junction, symlink, short name or mapped drive on the way.
fn same_place(file: &File, path: &Path) -> io::Result<()> {
    let wanted = path.to_str().ok_or_else(|| refused(LINK_ON_THE_WAY))?;
    if final_path(file)?.to_lowercase() == wanted.to_lowercase() { Ok(()) } else { Err(refused(LINK_ON_THE_WAY)) }
}

/// The attributes, identity and number of names of what the handle holds.
fn info(file: &File) -> io::Result<BY_HANDLE_FILE_INFORMATION> {
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: `info` is written by the call.
    unsafe { GetFileInformationByHandle(h(file), &mut info) }.map_err(io_error)?;
    Ok(info)
}

fn is_dir(info: &BY_HANDLE_FILE_INFORMATION) -> bool {
    info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0
}

fn is_link(info: &BY_HANDLE_FILE_INFORMATION) -> bool {
    info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0
}

fn id(info: &BY_HANDLE_FILE_INFORMATION) -> Id {
    (info.dwVolumeSerialNumber, (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow))
}

pub(crate) fn id_of(path: &Path) -> Option<Id> {
    let found = info(&open(path, LOOK, OPEN_EXISTING).ok()?).ok()?;
    (!is_link(&found)).then(|| id(&found))
}

/// The folder `dir`, held and checked: a real folder (not a junction) where its path says.
fn pin(dir: &Path) -> io::Result<File> {
    let folder = open(dir, FOLDER, OPEN_EXISTING)?;
    let found = info(&folder)?;
    if !is_dir(&found) || is_link(&found) {
        return Err(refused(LINK_ON_THE_WAY));
    }
    same_place(&folder, dir)?;
    Ok(folder)
}

/// `path`'s folder pinned, and `path` itself opened for `access`, checked.
fn item(path: &Path, access: u32) -> io::Result<(File, File)> {
    let folder = pin(parent(path)?)?;
    let file = open(path, access, OPEN_EXISTING)?;
    same_place(&file, path)?;
    Ok((folder, file))
}

/// What is at `to` must not stop a replace: a folder (or a junction) is never replaced, a file
/// with several names never changed.
fn replaceable(to: &Path) -> io::Result<()> {
    let Ok(there) = open(to, LOOK, OPEN_EXISTING) else { return Ok(()) };
    let there = info(&there)?;
    if is_dir(&there) {
        return Err(refused(FOLDER_THERE));
    }
    if there.nNumberOfLinks > 1 {
        return Err(refused(SEVERAL_NAMES));
    }
    Ok(())
}

/// Deletes what `file` is (an empty folder, a file, a link) through its handle: POSIX semantics
/// (the name goes at once, read-only or not); FAT and many network drives know none: the old way.
fn dispose(file: &File) -> io::Result<()> {
    let ex = FILE_DISPOSITION_INFO_EX {
        Flags: FILE_DISPOSITION_INFO_EX_FLAGS(
            FILE_DISPOSITION_FLAG_DELETE.0
                | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS.0
                | FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE.0,
        ),
    };
    // SAFETY: `ex` is the size the class asks for.
    let done = unsafe {
        SetFileInformationByHandle(
            h(file),
            FileDispositionInfoEx,
            (&raw const ex).cast::<c_void>(),
            size_of::<FILE_DISPOSITION_INFO_EX>() as u32,
        )
    };
    match done.map_err(io_error) {
        Ok(()) => Ok(()),
        // ERROR_INVALID_FUNCTION, ERROR_NOT_SUPPORTED, ERROR_INVALID_PARAMETER: no POSIX deletes.
        Err(err) if err.raw_os_error().is_some_and(|code| [1, 50, 87].contains(&code)) => {
            let old = FILE_DISPOSITION_INFO { DeleteFile: true };
            // SAFETY: as above. shortcut: a read-only file on FAT fails here; clear the attribute
            // first if that comes up.
            unsafe {
                SetFileInformationByHandle(
                    h(file),
                    FileDispositionInfo,
                    (&raw const old).cast::<c_void>(),
                    size_of::<FILE_DISPOSITION_INFO>() as u32,
                )
            }
            .map_err(io_error)
        }
        Err(err) => Err(err),
    }
}

pub(crate) fn delete(path: &Path, guard: &Guard) -> io::Result<()> {
    let (_folder, file) = item(path, REMOVE | FILE_LIST_DIRECTORY.0)?;
    remove(path, file, guard, 0)
}

/// Deletes `path` only if it is still the item `own` (a folder this helper made): one swapped in
/// since is refused, not deleted. Checked on the handle the delete goes through.
pub(crate) fn delete_own(path: &Path, own: Id) -> io::Result<()> {
    let (_folder, file) = item(path, REMOVE | FILE_LIST_DIRECTORY.0)?;
    if id(&info(&file)?) != own {
        return Err(refused(CHANGED));
    }
    remove(path, file, &Guard::none(), 0)
}

fn remove(path: &Path, file: File, guard: &Guard, depth: usize) -> io::Result<()> {
    let found = info(&file)?;
    guard.check(id(&found))?;
    if is_dir(&found) && !is_link(&found) {
        if depth >= MAX_DEPTH {
            return Err(refused(TOO_DEEP));
        }
        // Read while `file` holds the folder (it cannot be renamed): the names are this folder's.
        for entry in std::fs::read_dir(path)? {
            let child = path.join(entry?.file_name());
            let handle = open(&child, REMOVE | FILE_LIST_DIRECTORY.0, OPEN_EXISTING)?;
            same_place(&handle, &child)?;
            remove(&child, handle, guard, depth + 1)?;
        }
    }
    dispose(&file)
}

pub(crate) fn rmdir(path: &Path, guard: &Guard) -> io::Result<()> {
    let (_folder, file) = item(path, REMOVE)?;
    let found = info(&file)?;
    guard.check(id(&found))?;
    if !is_dir(&found) || is_link(&found) {
        return Err(refused(NOT_A_FOLDER));
    }
    dispose(&file)
}

pub(crate) fn mkdir(path: &Path) -> io::Result<()> {
    let _folder = pin(parent(path)?)?;
    // SAFETY: the path lives through the call. Whatever is at the name (a link too) makes it fail.
    unsafe { CreateDirectoryW(&verbatim(path), None) }.map_err(io_error)
}

/// Gives the item of `file` the path `to` (its folder held by the caller); `replace`: a file
/// there is replaced. The name goes in its NT form (`\??\C:\…`), which the call takes as it is
/// (the tests rename into another folder this way).
fn set_name(file: &File, to: &Path, replace: bool) -> io::Result<()> {
    let text = to.to_str().ok_or_else(|| refused(CHANGED))?;
    let name: Vec<u16> = format!(r"\??\{text}").encode_utf16().collect();
    let size = size_of::<FILE_RENAME_INFO>() + name.len() * 2;
    // u64s: FILE_RENAME_INFO wants pointer alignment.
    let mut buffer = vec![0u64; size.div_ceil(8)];
    let rename = buffer.as_mut_ptr().cast::<FILE_RENAME_INFO>();
    // SAFETY: the buffer holds the struct and the name after it.
    unsafe {
        (*rename).Anonymous.ReplaceIfExists = replace;
        (*rename).FileNameLength = (name.len() * 2) as u32;
        std::ptr::copy_nonoverlapping(name.as_ptr(), (&raw mut (*rename).FileName).cast::<u16>(), name.len());
        SetFileInformationByHandle(h(file), FileRenameInfo, rename.cast::<c_void>(), size as u32)
    }
    .map_err(io_error)
}

pub(crate) fn rename(path: &Path, name: &str, guard: &Guard) -> io::Result<()> {
    let (_folder, file) = item(path, DELETE.0 | LOOK)?;
    guard.check(id(&info(&file)?))?;
    set_name(&file, &path.with_file_name(name), false)
}

pub(crate) fn move_to(from: &Path, to: &Path, replace: bool, guard: &Guard) -> io::Result<()> {
    let (folder, file) = item(from, DELETE.0 | LOOK)?;
    guard.check(id(&info(&file)?))?;
    let target = pin(parent(to)?)?;
    if replace {
        replaceable(to)?;
    }
    match set_name(&file, to, replace) {
        // ERROR_NOT_SAME_DEVICE: another drive.
        Err(err) if err.raw_os_error() == Some(17) => {
            drop((folder, file, target));
            copy(from, to, replace)?;
            delete(from, guard)
        }
        other => other,
    }
}

pub(crate) fn copy(from: &Path, to: &Path, replace: bool) -> io::Result<()> {
    // Only a folder this copy made itself is taken back (a file it made is disposed of through
    // its handle in `copy_item`): a name someone else took in the meantime makes the make fail,
    // and what they put there stays.
    let mut made = None;
    let result = copy_item(from, to, replace, 0, &mut made);
    if result.is_err()
        && let Some(own) = made
    {
        let _ = delete_own(to, own);
    }
    result
}

/// `made`: the identity of the folder `to` once this call made it itself.
fn copy_item(from: &Path, to: &Path, replace: bool, depth: usize, made: &mut Option<Id>) -> io::Result<()> {
    let (_from_folder, source) = item(from, GENERIC_READ.0 | LOOK)?;
    let found = info(&source)?;
    if is_link(&found) {
        return Err(refused(LINK_COPY));
    }
    let _to_folder = pin(parent(to)?)?;
    if is_dir(&found) {
        if depth >= MAX_DEPTH {
            return Err(refused(TOO_DEEP));
        }
        match open(to, LOOK, OPEN_EXISTING) {
            Ok(there) => {
                same_place(&there, to)?;
                let there = info(&there)?;
                if !is_dir(&there) || is_link(&there) {
                    return Err(io::ErrorKind::AlreadyExists.into());
                }
            }
            // SAFETY: the path lives through the call.
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                // Fails if the name was taken since: then it is not ours.
                unsafe { CreateDirectoryW(&verbatim(to), None) }.map_err(io_error)?;
                // shortcut: a folder swapped in between the make and this open is taken for
                // ours (needs write access to the folder above); NtCreateFile would close it.
                let ours = open(to, LOOK, OPEN_EXISTING)?;
                same_place(&ours, to)?;
                *made = Some(id(&info(&ours)?));
            }
            Err(err) => return Err(err),
        }
        for entry in std::fs::read_dir(from)? {
            let name = entry?.file_name();
            copy_item(&from.join(&name), &to.join(&name), replace, depth + 1, &mut None)?;
        }
        return Ok(());
    }
    let write_to = if replace {
        replaceable(to)?;
        to.with_file_name(format!(".gezik-{}", random_name()))
    } else {
        to.to_path_buf()
    };
    // CREATE_NEW never follows anything: whatever is at the name makes it fail.
    let out = open(&write_to, GENERIC_WRITE.0 | DELETE.0 | LOOK, CREATE_NEW)?;
    let done = (|| {
        std::io::copy(&mut &source, &mut &out)?;
        let (mut created, mut accessed, mut written) = (FILETIME::default(), FILETIME::default(), FILETIME::default());
        // SAFETY: both handles are open; the times are written by the first call.
        unsafe {
            GetFileTime(h(&source), Some(&mut created), Some(&mut accessed), Some(&mut written)).map_err(io_error)?;
            SetFileTime(h(&out), Some(&created), Some(&accessed), Some(&written)).map_err(io_error)?;
        }
        if replace { set_name(&out, to, true) } else { Ok(()) }
    })();
    if done.is_err() {
        let _ = dispose(&out);
    }
    done
}
