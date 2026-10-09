//! "Always keep on this device" and "Free up space" (spec 9 §7.3, part 9b5): Gezik asks the
//! cloud app (Windows: the cloud filter's pin state; macOS: NSFileManager) and never reads,
//! deletes or moves a file's data itself. Freeing up asks only for items known to be fully
//! synced; the rest are left on this device and said so.

use std::fmt;
use std::io;
use std::path::Path;

pub const NOT_SYNCED: &str = "Not known to be synced yet; left on this device";
pub const NOT_CLOUD: &str = "Not in a cloud folder; left as it is";
pub const A_LINK: &str = "A link; left as it is";

/// What may be asked of the cloud app for an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    Go,
    NotCloud,
    NotSynced,
}

/// An item Gezik chose not to ask about: a note in the job's report, not a failure.
#[derive(Debug)]
struct LeftAlone(&'static str);

impl fmt::Display for LeftAlone {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for LeftAlone {}

#[cfg_attr(not(any(windows, target_os = "macos", test)), allow(dead_code))]
fn left_alone(verdict: Verdict) -> io::Error {
    let text = if verdict == Verdict::NotSynced { NOT_SYNCED } else { NOT_CLOUD };
    io::Error::other(LeftAlone(text))
}

/// Whether `err` says Gezik left the item alone on purpose (not synced, not the cloud app's,
/// a link).
pub fn is_left_alone(err: &io::Error) -> bool {
    err.get_ref().is_some_and(|inner| inner.is::<LeftAlone>())
}

/// The error a not-synced item gives (the job's tests answer with it).
#[doc(hidden)]
pub fn not_synced_error() -> io::Error {
    left_alone(Verdict::NotSynced)
}

/// A selected link is never followed to wherever it leads (std counts junctions as links).
fn refuse_link(path: &Path) -> io::Result<()> {
    if std::fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err(io::Error::other(LeftAlone(A_LINK)));
    }
    Ok(())
}

/// Windows: a cloud filter placeholder (or the sync root) may be pinned; it may be unpinned
/// only when in sync (`CF_PLACEHOLDER_STATE`).
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn windows_verdict(state: u32, free_up: bool) -> Verdict {
    const PLACEHOLDER: u32 = 0x1;
    const SYNC_ROOT: u32 = 0x2;
    const IN_SYNC: u32 = 0x8;
    if state == u32::MAX || state & (PLACEHOLDER | SYNC_ROOT) == 0 {
        Verdict::NotCloud
    } else if free_up && state & IN_SYNC == 0 {
        Verdict::NotSynced
    } else {
        Verdict::Go
    }
}

/// macOS, freeing up: uploaded, not uploading, no conflicts; anything not known is a no.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn mac_verdict(uploaded: Option<bool>, uploading: Option<bool>, conflicts: Option<bool>) -> Verdict {
    if uploaded == Some(true) && uploading != Some(true) && conflicts != Some(true) {
        Verdict::Go
    } else {
        Verdict::NotSynced
    }
}

/// Asks the cloud app to keep `path` (and what is in it) on this device.
pub fn keep(path: &Path) -> io::Result<()> {
    refuse_link(path)?;
    imp::set(path, false)
}

/// Asks the cloud app to free up `path`'s space, only if it is known to be fully synced.
pub fn free_up(path: &Path) -> io::Result<()> {
    refuse_link(path)?;
    imp::set(path, true)
}

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;
    use std::io;
    use std::path::Path;

    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::Storage::CloudFilters::{
        CF_PIN_STATE_PINNED, CF_PIN_STATE_UNPINNED, CF_SET_PIN_FLAG_RECURSE, CfGetPlaceholderStateFromFileInfo,
        CfSetPinState,
    };
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_TAG_INFO, FILE_FLAG_BACKUP_SEMANTICS, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE,
        FILE_SHARE_READ, FILE_SHARE_WRITE, FileAttributeTagInfo, GetFileInformationByHandleEx, OPEN_EXISTING,
    };

    use super::{Verdict, left_alone, windows_verdict};
    use crate::fs::{io_error, verbatim};

    /// Opens the item itself for its attributes only: CfSetPinState takes "an attribute or
    /// no-access handle" (the READ_DATA or WRITE_DAC it names is the caller's right on the
    /// file, checked by the filter). No data access, so the open downloads nothing; no
    /// OPEN_REPARSE_POINT, so the cloud filter sees the open (links were refused before).
    /// Then reads the placeholder state from the handle and, if the verdict allows, sets the
    /// pin state of the item and everything in it.
    // shortcut: the link check and this open are two steps; a swap in between is the user's own cloud folder.
    pub(super) fn set(path: &Path, free_up: bool) -> io::Result<()> {
        // SAFETY: a path that ends with NUL; the handle is closed below.
        let handle = unsafe {
            CreateFileW(
                &verbatim(path),
                FILE_READ_ATTRIBUTES.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                None,
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS,
                None,
            )
        }
        .map_err(io_error)?;
        let result = (|| {
            let mut info = FILE_ATTRIBUTE_TAG_INFO::default();
            // SAFETY: `info` is the buffer FileAttributeTagInfo fills.
            unsafe {
                GetFileInformationByHandleEx(
                    handle,
                    FileAttributeTagInfo,
                    (&mut info as *mut FILE_ATTRIBUTE_TAG_INFO).cast::<c_void>(),
                    size_of::<FILE_ATTRIBUTE_TAG_INFO>() as u32,
                )
            }
            .map_err(io_error)?;
            // SAFETY: `info` was filled above for this class.
            let state = unsafe {
                CfGetPlaceholderStateFromFileInfo(
                    (&info as *const FILE_ATTRIBUTE_TAG_INFO).cast(),
                    FileAttributeTagInfo,
                )
            };
            match windows_verdict(state.0, free_up) {
                Verdict::Go => {}
                other => return Err(left_alone(other)),
            }
            let pin = if free_up { CF_PIN_STATE_UNPINNED } else { CF_PIN_STATE_PINNED };
            // SAFETY: an open handle; no OVERLAPPED: the call returns once the state is set.
            unsafe { CfSetPinState(handle, pin, CF_SET_PIN_FLAG_RECURSE, None) }.map_err(io_error)
        })();
        // SAFETY: opened above, closed once.
        unsafe {
            let _ = CloseHandle(handle);
        }
        result
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use std::io;
    use std::path::Path;

    use objc2::rc::{Retained, autoreleasepool};
    use objc2::runtime::AnyObject;
    use objc2_foundation::{
        NSFileManager, NSNumber, NSString, NSURL, NSURLResourceKey, NSURLUbiquitousItemHasUnresolvedConflictsKey,
        NSURLUbiquitousItemIsUploadedKey, NSURLUbiquitousItemIsUploadingKey,
    };

    use super::{Verdict, left_alone, mac_verdict};

    /// A yes/no resource value; `None` when the system gives none.
    fn flag(url: &NSURL, key: &NSURLResourceKey) -> Option<bool> {
        let mut value: Option<Retained<AnyObject>> = None;
        // SAFETY: the keys asked for here have NSNumber values.
        unsafe { url.getResourceValue_forKey_error(&mut value, key) }.ok()?;
        value?.downcast::<NSNumber>().ok().map(|n| n.boolValue())
    }

    pub(super) fn set(path: &Path, free_up: bool) -> io::Result<()> {
        let text = path.to_str().ok_or_else(|| io::Error::other("The path is not Unicode text"))?;
        autoreleasepool(|_| {
            let url = NSURL::fileURLWithPath(&NSString::from_str(text));
            let manager = NSFileManager::defaultManager();
            if free_up {
                // SAFETY: extern statics of Foundation.
                let verdict = unsafe {
                    mac_verdict(
                        flag(&url, NSURLUbiquitousItemIsUploadedKey),
                        flag(&url, NSURLUbiquitousItemIsUploadingKey),
                        flag(&url, NSURLUbiquitousItemHasUnresolvedConflictsKey),
                    )
                };
                if verdict != Verdict::Go {
                    return Err(left_alone(verdict));
                }
                manager.evictUbiquitousItemAtURL_error(&url)
            } else {
                manager.startDownloadingUbiquitousItemAtURL_error(&url)
            }
            .map_err(|err| io::Error::other(err.localizedDescription().to_string()))
        })
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod imp {
    use std::io;
    use std::path::Path;

    pub(super) fn set(_path: &Path, _free_up: bool) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "Cloud folders on Linux have no download commands"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLACEHOLDER: u32 = 0x1;
    const SYNC_ROOT: u32 = 0x2;
    const IN_SYNC: u32 = 0x8;
    const INVALID: u32 = 0xFFFF_FFFF;

    #[test]
    fn free_up_needs_in_sync() {
        assert_eq!(windows_verdict(PLACEHOLDER | IN_SYNC, true), Verdict::Go);
        assert_eq!(windows_verdict(PLACEHOLDER, true), Verdict::NotSynced, "local changes not uploaded");
        assert_eq!(windows_verdict(SYNC_ROOT | IN_SYNC, true), Verdict::Go, "the root itself");
        assert_eq!(windows_verdict(IN_SYNC, true), Verdict::NotCloud, "no placeholder: not the cloud app's");
        assert_eq!(windows_verdict(0, true), Verdict::NotCloud);
        assert_eq!(windows_verdict(INVALID, true), Verdict::NotCloud);
    }

    #[test]
    fn keeping_needs_only_a_placeholder() {
        assert_eq!(windows_verdict(PLACEHOLDER, false), Verdict::Go, "not in sync: still fine to download");
        assert_eq!(windows_verdict(0, false), Verdict::NotCloud);
        assert_eq!(windows_verdict(INVALID, false), Verdict::NotCloud);
    }

    #[test]
    fn mac_free_up_needs_uploaded() {
        assert_eq!(mac_verdict(Some(true), Some(false), Some(false)), Verdict::Go);
        assert_eq!(mac_verdict(Some(true), None, None), Verdict::Go, "the two others unknown: no sign of trouble");
        assert_eq!(mac_verdict(Some(false), None, None), Verdict::NotSynced);
        assert_eq!(mac_verdict(None, None, None), Verdict::NotSynced, "not known: left alone");
        assert_eq!(mac_verdict(Some(true), Some(true), None), Verdict::NotSynced, "still uploading");
        assert_eq!(mac_verdict(Some(true), Some(false), Some(true)), Verdict::NotSynced, "a conflict");
    }

    #[test]
    fn left_alone_errors_are_known_by_their_kind() {
        assert!(is_left_alone(&left_alone(Verdict::NotSynced)));
        assert!(is_left_alone(&left_alone(Verdict::NotCloud)));
        assert!(!is_left_alone(&io::Error::other("Not fully synced yet")), "the text alone is not enough");
        assert!(!is_left_alone(&io::ErrorKind::PermissionDenied.into()));
        assert_eq!(left_alone(Verdict::NotSynced).to_string(), NOT_SYNCED);
    }

    /// A plain file and folder (not a cloud placeholder) are refused as not the cloud app's,
    /// for both commands, and left exactly as they were.
    #[cfg(windows)]
    #[test]
    fn a_plain_file_is_not_the_cloud_apps() {
        let dir = crate::fs::test_dir("cloud-pin-plain");
        let file = dir.join("a.txt");
        std::fs::write(&file, "kept").unwrap();
        let before = std::fs::metadata(&file).unwrap();
        for path in [&file, &dir] {
            for result in [keep(path), free_up(path)] {
                let err = result.unwrap_err();
                assert!(is_left_alone(&err), "{err}");
                assert_eq!(err.to_string(), NOT_CLOUD);
            }
        }
        let after = std::fs::metadata(&file).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "kept");
        assert_eq!(after.modified().unwrap(), before.modified().unwrap());
        assert_eq!(
            std::os::windows::fs::MetadataExt::file_attributes(&after),
            std::os::windows::fs::MetadataExt::file_attributes(&before)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_item_is_an_error_not_a_note() {
        let err = keep(Path::new("/gezik-cloud-pin-missing/x")).unwrap_err();
        assert!(!is_left_alone(&err));
    }
}
