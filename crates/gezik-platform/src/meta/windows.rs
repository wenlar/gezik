//! Windows: the `:gezik.meta` stream (spec 12 §4.2), written with the file's modification
//! time put back (§3.5).

use std::ffi::OsString;
use std::fs::{File, FileTimes, OpenOptions};
use std::io::{self, Read, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use gezik_core::meta::{Meta, StreamRecord, decode_stream, encode_stream};
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Storage::FileSystem::{
    DeleteFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_WRITE_ATTRIBUTES,
    GetDriveTypeW, GetVolumeInformationW,
};
use windows::core::{GUID, HSTRING};

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
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            gezik_i18n::t!("A tag record Gezik does not know; left alone").into_owned(),
        ));
    }
    let unknown = current.map(|record| record.unknown).unwrap_or_default();
    let written = if meta.is_empty() && unknown.is_empty() {
        match unsafe { DeleteFileW(&verbatim(&stream_path(path))) }.map_err(io_error) {
            Err(err) if err.kind() != io::ErrorKind::NotFound => Err(err),
            _ => Ok(()),
        }
    } else {
        let text = encode_stream(meta, &unknown);
        if text.len() as u64 > MAX_STREAM {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                gezik_i18n::t!("Too many tags and too long a comment for one file").into_owned(),
            ));
        }
        write_stream(path, text.as_bytes())
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

/// Explorer's Tags (`System.Keywords`) and rating (`System.Rating`, 0–99) of `path`, read
/// through its property handler (spec 12 §8.3): read only, never written. `None` when it has
/// no handler, cannot be read, is a link or only in the cloud now (the handler would download
/// it), or the handler takes over 2 s: that call is left behind and the file is not tried again
/// in this session.
pub fn explorer(path: &Path) -> Option<(Vec<String>, u32)> {
    use std::collections::HashSet;
    use std::sync::{Mutex, PoisonError};
    static HUNG: Mutex<Option<HashSet<PathBuf>>> = Mutex::new(None);
    let hung = || HUNG.lock().unwrap_or_else(PoisonError::into_inner);
    if hung().as_ref().is_some_and(|h| h.contains(path)) {
        return None;
    }
    let facts = std::fs::symlink_metadata(path).ok()?;
    let cloud_only = gezik_core::attribute_flags(&facts) & gezik_core::Entry::CLOUD_ONLY != 0;
    if facts.is_symlink() || !facts.is_file() || cloud_only {
        return None;
    }
    let owned = path.to_path_buf();
    let (send, answer) = std::sync::mpsc::sync_channel(1);
    let started = super::within(std::time::Duration::from_secs(2), move || {
        let _ = send.send(());
        explorer_now(&owned)
    });
    match started {
        Some(found) => found,
        // Started but not done in time (not a refusal for too many calls): never again.
        None if answer.try_recv().is_ok() => {
            hung().get_or_insert_with(HashSet::new).insert(path.to_path_buf());
            None
        }
        None => None,
    }
}

/// `System.Keywords` and `System.Rating`.
const KEYWORDS: PROPERTYKEY = PROPERTYKEY { fmtid: GUID::from_u128(0xf29f85e0_4ff9_1068_ab91_08002b27b3d9), pid: 5 };
const RATING: PROPERTYKEY = PROPERTYKEY { fmtid: GUID::from_u128(0x64440492_4c8b_11d1_8b70_080036b11a03), pid: 9 };

fn explorer_now(path: &Path) -> Option<(Vec<String>, u32)> {
    use windows::Win32::System::Com::StructuredStorage::PropVariantClear;
    use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
    use windows::Win32::System::Variant::{VT_LPWSTR, VT_UI4, VT_VECTOR};
    use windows::Win32::UI::Shell::PropertiesSystem::{GPS_DEFAULT, IPropertyStore, SHGetPropertyStoreFromParsingName};
    // A thread of its own (`within`): the multithreaded apartment needs no message loop.
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let store: IPropertyStore =
        unsafe { SHGetPropertyStoreFromParsingName(&HSTRING::from(path.as_os_str()), None, GPS_DEFAULT) }.ok()?;
    let mut keywords = Vec::new();
    let mut rating = 0;
    // SAFETY: each union field is read only under the type tag that says it holds one.
    unsafe {
        if let Ok(mut value) = store.GetValue(&KEYWORDS) {
            let inner = &value.Anonymous.Anonymous;
            if inner.vt == VT_VECTOR | VT_LPWSTR {
                let list = inner.Anonymous.calpwstr;
                for i in 0..list.cElems as usize {
                    if let Ok(word) = (*list.pElems.add(i)).to_string() {
                        keywords.push(word);
                    }
                }
            } else if inner.vt == VT_LPWSTR
                && let Ok(word) = inner.Anonymous.pwszVal.to_string()
            {
                keywords.push(word);
            }
            let _ = PropVariantClear(&mut value);
        }
        if let Ok(mut value) = store.GetValue(&RATING) {
            let inner = &value.Anonymous.Anonymous;
            if inner.vt == VT_UI4 {
                rating = inner.Anonymous.ulVal.min(99);
            }
            let _ = PropVariantClear(&mut value);
        }
    }
    Some((keywords, rating))
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::Com::StructuredStorage::{
        InitPropVariantFromStringAsVector, PROPVARIANT, PropVariantClear,
    };
    use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
    use windows::Win32::System::Variant::VT_UI4;
    use windows::Win32::UI::Shell::PropertiesSystem::{
        GPS_READWRITE, IPropertyStore, SHGetPropertyStoreFromParsingName,
    };

    #[test]
    fn explorers_tags_and_rating_are_read_and_the_file_is_left_alone() {
        // Listed in docs/superpowers/notes/parts/ci-skips.md.
        if std::env::var_os("CI").is_some() {
            eprintln!("skipped in CI: the runner's service session gives no Explorer property store");
            return;
        }
        let _alone = super::super::within_alone();
        // Written only under %TEMP%\gezik-12d: the test plays Explorer on its own picture.
        let dir = std::env::temp_dir().join("gezik-12d").join(format!("explorer-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("photo.jpg");
        image::RgbImage::from_pixel(16, 16, image::Rgb([200, 120, 40])).save(&file).unwrap();
        assert_eq!(explorer(&file), Some((Vec::new(), 0)), "a picture without them");
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let store: IPropertyStore =
                SHGetPropertyStoreFromParsingName(&HSTRING::from(file.as_os_str()), None, GPS_READWRITE).unwrap();
            let mut keywords = InitPropVariantFromStringAsVector(windows::core::w!("beach;Ayşe")).unwrap();
            store.SetValue(&KEYWORDS, &keywords).unwrap();
            let _ = PropVariantClear(&mut keywords);
            let mut rating = PROPVARIANT::default();
            (*rating.Anonymous.Anonymous).vt = VT_UI4;
            (*rating.Anonymous.Anonymous).Anonymous.ulVal = 75;
            store.SetValue(&RATING, &rating).unwrap();
            store.Commit().unwrap();
        }
        let (bytes, modified) = (std::fs::read(&file).unwrap(), std::fs::metadata(&file).unwrap().modified().unwrap());
        assert_eq!(explorer(&file), Some((vec!["beach".to_owned(), "Ayşe".to_owned()], 75)));
        assert_eq!(std::fs::read(&file).unwrap(), bytes, "read only: the file's bytes stay");
        assert_eq!(std::fs::metadata(&file).unwrap().modified().unwrap(), modified, "and its time");
        assert_eq!(explorer(&dir.join("gone.jpg")), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
