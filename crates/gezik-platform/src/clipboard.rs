//! The system clipboard, for files: copy or cut in Gezik and paste in Explorer or Finder, and
//! the other way round. On Linux, X11's or Wayland's (see `linux`).

use std::fmt;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardFiles {
    pub paths: Vec<PathBuf>,
    /// Cut (move on paste) rather than copied.
    pub cut: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardError {
    /// No system clipboard for files on this platform: Gezik keeps its own.
    Unsupported,
    Failed(String),
}

impl fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClipboardError::Unsupported => write!(f, "no system clipboard for files"),
            ClipboardError::Failed(why) => write!(f, "{why}"),
        }
    }
}

pub use imp::{clear, read_files, sequence, write_files};

/// `CF_HDROP` data: a DROPFILES header (wide names) and each path, NUL-terminated, then a NUL.
#[cfg(windows)]
fn dropfiles(paths: &[PathBuf]) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;
    let mut out = Vec::new();
    out.extend_from_slice(&20u32.to_le_bytes()); // pFiles: the names start after the header
    out.extend_from_slice(&0i32.to_le_bytes()); // pt.x
    out.extend_from_slice(&0i32.to_le_bytes()); // pt.y
    out.extend_from_slice(&0i32.to_le_bytes()); // fNC
    out.extend_from_slice(&1i32.to_le_bytes()); // fWide
    for path in paths {
        for unit in path.as_os_str().encode_wide().chain([0]) {
            out.extend_from_slice(&unit.to_le_bytes());
        }
    }
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

/// The paths in `CF_HDROP` data (the clipboard's, or a dropped data object's).
#[cfg(windows)]
pub(crate) fn hdrop_paths(drop: windows::Win32::UI::Shell::HDROP) -> Vec<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows::Win32::UI::Shell::DragQueryFileW;
    let count = unsafe { DragQueryFileW(drop, u32::MAX, None) };
    let mut paths = Vec::new();
    for i in 0..count {
        let len = unsafe { DragQueryFileW(drop, i, None) } as usize;
        let mut buffer = vec![0u16; len + 1];
        let written = unsafe { DragQueryFileW(drop, i, Some(&mut buffer)) } as usize;
        if written == 0 {
            continue;
        }
        paths.push(PathBuf::from(std::ffi::OsString::from_wide(&buffer[..written.min(len)])));
    }
    paths
}

#[cfg(windows)]
mod imp {
    use std::path::PathBuf;

    use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber, OpenClipboard,
        RegisterClipboardFormatW, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock};
    use windows::Win32::System::Ole::{CF_HDROP, DROPEFFECT_COPY, DROPEFFECT_MOVE};
    use windows::Win32::UI::Shell::{CFSTR_PREFERREDDROPEFFECT, HDROP};

    use super::{ClipboardError, ClipboardFiles};

    fn failed(err: impl std::fmt::Display) -> ClipboardError {
        ClipboardError::Failed(err.to_string())
    }

    /// The clipboard, open until dropped. Another program may hold it for a moment: retry.
    struct Open;

    impl Open {
        fn new() -> Result<Open, ClipboardError> {
            for _ in 0..10 {
                if unsafe { OpenClipboard(None) }.is_ok() {
                    return Ok(Open);
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(failed("another program is using the clipboard"))
        }
    }

    impl Drop for Open {
        fn drop(&mut self) {
            let _ = unsafe { CloseClipboard() };
        }
    }

    fn put(format: u32, bytes: &[u8]) -> Result<(), ClipboardError> {
        unsafe {
            let memory = GlobalAlloc(GMEM_MOVEABLE, bytes.len()).map_err(failed)?;
            let target = GlobalLock(memory) as *mut u8;
            if target.is_null() {
                let _ = GlobalFree(Some(memory));
                return Err(failed("out of memory"));
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), target, bytes.len());
            let _ = GlobalUnlock(memory);
            // The clipboard owns the memory once this succeeds.
            if let Err(err) = SetClipboardData(format, Some(HANDLE(memory.0))) {
                let _ = GlobalFree(Some(memory));
                return Err(failed(err));
            }
        }
        Ok(())
    }

    fn effect_format() -> u32 {
        unsafe { RegisterClipboardFormatW(CFSTR_PREFERREDDROPEFFECT) }
    }

    pub fn write_files(paths: &[PathBuf], cut: bool) -> Result<(), ClipboardError> {
        let _open = Open::new()?;
        unsafe { EmptyClipboard() }.map_err(failed)?;
        put(u32::from(CF_HDROP.0), &super::dropfiles(paths))?;
        let effect = if cut { DROPEFFECT_MOVE.0 } else { DROPEFFECT_COPY.0 };
        if let Err(err) = put(effect_format(), &effect.to_le_bytes()) {
            // Never leave files without their effect: a requested cut would read as a copy.
            let _ = unsafe { EmptyClipboard() };
            return Err(err);
        }
        Ok(())
    }

    pub fn read_files() -> Result<Option<ClipboardFiles>, ClipboardError> {
        let _open = Open::new()?;
        let Ok(handle) = (unsafe { GetClipboardData(u32::from(CF_HDROP.0)) }) else { return Ok(None) };
        let paths = super::hdrop_paths(HDROP(handle.0));
        if paths.is_empty() {
            return Ok(None);
        }
        let cut = unsafe { GetClipboardData(effect_format()) }
            .ok()
            .and_then(|h| read_u32(HGLOBAL(h.0)))
            .is_some_and(|effect| effect & DROPEFFECT_MOVE.0 != 0);
        Ok(Some(ClipboardFiles { paths, cut }))
    }

    fn read_u32(memory: HGLOBAL) -> Option<u32> {
        unsafe {
            if GlobalSize(memory) < 4 {
                return None;
            }
            let source = GlobalLock(memory) as *const u8;
            if source.is_null() {
                return None;
            }
            let mut bytes = [0u8; 4];
            std::ptr::copy_nonoverlapping(source, bytes.as_mut_ptr(), 4);
            let _ = GlobalUnlock(memory);
            Some(u32::from_le_bytes(bytes))
        }
    }

    pub fn sequence() -> u64 {
        u64::from(unsafe { GetClipboardSequenceNumber() })
    }

    pub fn clear() -> Result<(), ClipboardError> {
        let _open = Open::new()?;
        unsafe { EmptyClipboard() }.map_err(failed)
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use std::cell::Cell;
    use std::path::PathBuf;

    use objc2::ClassType;
    use objc2::rc::Retained;
    use objc2::runtime::ProtocolObject;
    use objc2_app_kit::{NSPasteboard, NSPasteboardWriting};
    use objc2_foundation::{NSArray, NSString, NSURL};

    use super::{ClipboardError, ClipboardFiles};

    thread_local! {
        /// The pasteboard's change count when Gezik last cut: Finder has no "cut", so Gezik
        /// remembers its own.
        static CUT_AT: Cell<Option<isize>> = const { Cell::new(None) };
    }

    pub fn write_files(paths: &[PathBuf], cut: bool) -> Result<(), ClipboardError> {
        // Check every path before touching the pasteboard, so nothing partial is written.
        let names: Vec<&str> = paths
            .iter()
            .map(|p| p.to_str().ok_or_else(|| ClipboardError::Failed("a path is not valid UTF-8".into())))
            .collect::<Result<_, _>>()?;
        let pasteboard = NSPasteboard::generalPasteboard();
        pasteboard.clearContents();
        let urls: Vec<Retained<ProtocolObject<dyn NSPasteboardWriting>>> = names
            .into_iter()
            .map(|p| ProtocolObject::from_retained(NSURL::fileURLWithPath(&NSString::from_str(p))))
            .collect();
        if !pasteboard.writeObjects(&NSArray::from_retained_slice(&urls)) {
            return Err(ClipboardError::Failed("the pasteboard refused the files".into()));
        }
        CUT_AT.set(cut.then(|| pasteboard.changeCount()));
        Ok(())
    }

    pub fn read_files() -> Result<Option<ClipboardFiles>, ClipboardError> {
        let pasteboard = NSPasteboard::generalPasteboard();
        let classes = NSArray::from_slice(&[NSURL::class()]);
        let Some(objects) = (unsafe { pasteboard.readObjectsForClasses_options(&classes, None) }) else {
            return Ok(None);
        };
        let paths: Vec<PathBuf> = objects
            .iter()
            .filter_map(|object| object.downcast::<NSURL>().ok())
            .filter(|url| url.isFileURL())
            .filter_map(|url| url.path())
            .map(|path| PathBuf::from(path.to_string()))
            .collect();
        if paths.is_empty() {
            return Ok(None);
        }
        let cut = CUT_AT.get() == Some(pasteboard.changeCount());
        Ok(Some(ClipboardFiles { paths, cut }))
    }

    pub fn sequence() -> u64 {
        NSPasteboard::generalPasteboard().changeCount() as u64
    }

    pub fn clear() -> Result<(), ClipboardError> {
        NSPasteboard::generalPasteboard().clearContents();
        CUT_AT.set(None);
        Ok(())
    }
}

/// X11 or Wayland, through the backend of Gezik's window; before the window exists (or
/// without a backend) Gezik keeps its own clipboard.
#[cfg(not(any(windows, target_os = "macos")))]
mod imp {
    use std::path::PathBuf;

    use super::{ClipboardError, ClipboardFiles};
    use crate::linux::backend;

    pub fn write_files(paths: &[PathBuf], cut: bool) -> Result<(), ClipboardError> {
        backend().ok_or(ClipboardError::Unsupported)?.write_files(paths, cut)
    }

    pub fn read_files() -> Result<Option<ClipboardFiles>, ClipboardError> {
        backend().ok_or(ClipboardError::Unsupported)?.read_files()
    }

    pub fn sequence() -> u64 {
        backend().map_or(0, |b| b.sequence())
    }

    pub fn clear() -> Result<(), ClipboardError> {
        backend().ok_or(ClipboardError::Unsupported)?.clear()
    }
}

#[cfg(test)]
mod tests {
    #[cfg(any(windows, target_os = "macos"))]
    use super::*;

    #[cfg(windows)]
    #[test]
    fn dropfiles_layout() {
        let bytes = dropfiles(&[PathBuf::from(r"C:\a"), PathBuf::from(r"C:\ş")]);
        assert_eq!(&bytes[0..4], &20u32.to_le_bytes());
        assert_eq!(&bytes[16..20], &1i32.to_le_bytes(), "wide names");
        let units: Vec<u16> = bytes[20..].chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        let expected: Vec<u16> = "C:\\a\0C:\\ş\0\0".encode_utf16().collect();
        assert_eq!(units, expected);
    }

    /// Uses the real clipboard: run by hand (`cargo test -p gezik-platform -- --ignored`).
    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    #[ignore = "replaces the user's clipboard"]
    fn files_round_trip_through_the_clipboard() {
        let paths = vec![std::env::temp_dir().join("gezik clip a.txt"), std::env::temp_dir().join("b")];
        let before = sequence();
        write_files(&paths, true).unwrap();
        assert_ne!(sequence(), before);
        assert_eq!(read_files().unwrap(), Some(ClipboardFiles { paths: paths.clone(), cut: true }));
        write_files(&paths, false).unwrap();
        assert!(!read_files().unwrap().unwrap().cut);
        clear().unwrap();
        assert_eq!(read_files().unwrap(), None);
    }
}
