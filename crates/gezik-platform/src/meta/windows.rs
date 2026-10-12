//! Windows: the `:gezik.meta` stream (spec 12 §4.2), written with the file's modification
//! time put back (§3.5).

use std::ffi::OsString;
use std::fs::{File, FileTimes, OpenOptions};
use std::io::{self, Read, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use gezik_core::meta::{Meta, StreamRecord, decode_stream, encode_stream};
use windows::Win32::Storage::FileSystem::{
    DeleteFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_WRITE_ATTRIBUTES,
    GetDriveTypeW, GetVolumeInformationW,
};
use windows::core::HSTRING;

use super::{Own, Volume};
use crate::fs::{io_error, verbatim};

/// The stream is small; a bigger one is not Gezik's and is not read.
const MAX_STREAM: u64 = 16 * 1024;
const SHARE_ALL: u32 = 7;
const DRIVE_REMOTE: u32 = 4;
// GetVolumeInformationW's file system flags.
const FILE_NAMED_STREAMS: u32 = 0x0004_0000;
const FILE_READ_ONLY_VOLUME: u32 = 0x0008_0000;

fn stream_path(path: &Path) -> PathBuf {
    let mut text: OsString = path.as_os_str().to_owned();
    text.push(":gezik.meta");
    PathBuf::from(text)
}

/// The stream's record; `None` if there is none.
fn read_stream(path: &Path) -> io::Result<Option<StreamRecord>> {
    let opened = OpenOptions::new()
        .read(true)
        .share_mode(SHARE_ALL)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(stream_path(path));
    let file = match opened {
        Ok(file) => file,
        // No stream (or a volume that has none: ERROR_INVALID_NAME, ERROR_INVALID_PARAMETER).
        Err(err) if err.kind() == io::ErrorKind::NotFound || matches!(err.raw_os_error(), Some(87 | 123)) => {
            return Ok(None);
        }
        Err(err) => return Err(err),
    };
    let mut bytes = Vec::new();
    file.take(MAX_STREAM + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_STREAM {
        return Ok(Some(StreamRecord { writable: false, ..StreamRecord::default() }));
    }
    let text = String::from_utf8_lossy(&bytes);
    Ok(Some(decode_stream(&text)))
}

pub fn read(path: &Path) -> io::Result<Own> {
    Ok(match read_stream(path)? {
        Some(record) => Own { meta: record.meta, writable: record.writable },
        None => Own::none(),
    })
}

/// Writes `meta` into `path`'s stream (an empty one removes it), keeping the lines it does
/// not know and the file's modification and access times. A file that is not there is not
/// made; a record of a later version is left alone (`InvalidData`).
pub fn write(path: &Path, meta: &Meta) -> io::Result<()> {
    // Opened first so the stream cannot make a file that is not there, and to put its times
    // back: NTFS counts a stream write as a change to the file.
    let base = OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES.0 | FILE_WRITE_ATTRIBUTES.0)
        .share_mode(SHARE_ALL)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(path)?;
    let facts = base.metadata()?;
    let current = read_stream(path)?;
    if current.as_ref().is_some_and(|record| !record.writable) {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "A tag record Gezik does not know; left alone"));
    }
    let unknown = current.map(|record| record.unknown).unwrap_or_default();
    let written = if meta.is_empty() && unknown.is_empty() {
        match unsafe { DeleteFileW(&verbatim(&stream_path(path))) }.map_err(io_error) {
            Err(err) if err.kind() != io::ErrorKind::NotFound => Err(err),
            _ => Ok(()),
        }
    } else {
        write_stream(path, encode_stream(meta, &unknown).as_bytes())
    };
    let times = FileTimes::new().set_modified(facts.modified()?).set_accessed(facts.accessed()?);
    let restored = base.set_times(times);
    written.and(restored)
}

fn write_stream(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file: File = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .share_mode(0)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(stream_path(path))?;
    file.write_all(bytes)
}

/// The volume whose root is `root` (`C:\`, `\\server\share\`).
pub fn volume(root: &Path) -> io::Result<Volume> {
    let root_text = root.to_string_lossy().into_owned();
    let root_wide = HSTRING::from(root.as_os_str());
    let remote = unsafe { GetDriveTypeW(&root_wide) } == DRIVE_REMOTE || root_text.starts_with(r"\\");
    let mut serial = 0u32;
    let mut flags = 0u32;
    unsafe { GetVolumeInformationW(&root_wide, None, Some(&mut serial), None, Some(&mut flags), None) }
        .map_err(io_error)?;
    let holds = flags & FILE_NAMED_STREAMS != 0 && flags & FILE_READ_ONLY_VOLUME == 0;
    // A share by its name (a mapped letter by the share it stands for); a local volume by its
    // serial number, the same under any letter.
    let key = if remote {
        let letter = root_text.chars().next().filter(|_| root_text.len() <= 3);
        letter.and_then(crate::fs::mapped_remote).unwrap_or(root_text).trim_end_matches('\\').to_lowercase()
    } else {
        format!("{serial:08x}")
    };
    Ok(Volume { key, holds })
}
