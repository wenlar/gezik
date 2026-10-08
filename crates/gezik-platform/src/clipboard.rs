//! The system clipboard, for files: copy or cut in Gezik and paste in Explorer or Finder, and
//! the other way round; text (paths copied as text, spec 4.2); and pictures and text read to paste as files (spec 9.1). On Linux, X11's or
//! Wayland's (see `linux`).

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

pub use gezik_core::templates::PasteKind;
pub use imp::{clear, paste_kind, read_files, read_image, read_text, sequence, write_files, write_text};

/// A picture on the clipboard, as it was read (the UI thread only reads; encoding happens in
/// the job that writes the file).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardImage {
    /// A PNG file's bytes (Windows' "PNG" format, macOS, `image/png` on Linux).
    Png(Vec<u8>),
    /// A BMP file's bytes, made from Windows' `CF_DIBV5` / `CF_DIB`.
    Bmp(Vec<u8>),
    /// A TIFF file's bytes, as macOS copies pictures; `png_bytes` converts it (so the UI
    /// thread only reads).
    #[cfg(target_os = "macos")]
    Tiff(Vec<u8>),
}

/// The most memory decoding a clipboard picture may use.
const MAX_DECODE_BYTES: u64 = 256 << 20;

impl ClipboardImage {
    /// The picture as a PNG file's bytes.
    pub fn png_bytes(&self) -> std::io::Result<Vec<u8>> {
        match self {
            ClipboardImage::Png(bytes) => Ok(bytes.clone()),
            ClipboardImage::Bmp(bytes) => {
                use image::ImageDecoder;
                let unreadable = |err: &dyn std::fmt::Display| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, format!("The picture cannot be read: {err}"))
                };
                // The BMP decoder's own pixels, without image's general conversions; the png
                // crate writes them as they are (a BMP decodes to RGB8 or RGBA8).
                let decoder =
                    image::codecs::bmp::BmpDecoder::new(std::io::Cursor::new(bytes)).map_err(|e| unreadable(&e))?;
                let (width, height) = decoder.dimensions();
                let color = match decoder.color_type() {
                    image::ColorType::Rgb8 => png::ColorType::Rgb,
                    image::ColorType::Rgba8 => png::ColorType::Rgba,
                    other => return Err(unreadable(&format!("unexpected colour type {other:?}"))),
                };
                // The decoder does not enforce an allocation limit itself: check before allocating.
                if decoder.total_bytes() > MAX_DECODE_BYTES {
                    return Err(unreadable(&"the picture is too large"));
                }
                let mut data = vec![0; usize::try_from(decoder.total_bytes()).map_err(|e| unreadable(&e))?];
                decoder.read_image(&mut data).map_err(|e| unreadable(&e))?;
                let mut out = Vec::new();
                let mut encoder = png::Encoder::new(&mut out, width, height);
                encoder.set_color(color);
                encoder.set_depth(png::BitDepth::Eight);
                encoder
                    .write_header()
                    .and_then(|mut writer| writer.write_image_data(&data))
                    .map_err(std::io::Error::other)?;
                Ok(out)
            }
            #[cfg(target_os = "macos")]
            ClipboardImage::Tiff(bytes) => imp::tiff_to_png(bytes).ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "The picture cannot be read: not a TIFF picture")
            }),
        }
    }
}

/// Text read from the clipboard: without a leading byte order mark, None if nothing is left.
pub(crate) fn clean_text(text: String) -> Option<String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    (!text.is_empty()).then(|| text.to_owned())
}

/// A BMP file from clipboard DIB data (a BITMAPINFOHEADER or a later one, maybe color masks
/// and a color table, then the pixels): the 14-byte file header goes in front, saying where
/// the pixels start. None for data too short to be a DIB.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn bmp_from_dib(dib: &[u8]) -> Option<Vec<u8>> {
    let u32_at = |at: usize| dib.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let header = usize::try_from(u32_at(0)?).ok()?;
    if dib.len() < 40 || !(40..=dib.len()).contains(&header) {
        return None;
    }
    let width = i32::from_le_bytes([dib[4], dib[5], dib[6], dib[7]]);
    let height = i32::from_le_bytes([dib[8], dib[9], dib[10], dib[11]]);
    let bit_count = u16::from_le_bytes([dib[14], dib[15]]);
    let compression = u32_at(16)?;
    // A header may declare a huge picture with next to no data: refuse before anyone allocates.
    if width <= 0 || height == 0 {
        return None;
    }
    let (width, rows) = (u64::from(width.unsigned_abs()), u64::from(height.unsigned_abs()));
    if width.checked_mul(rows)? > MAX_DECODE_BYTES / 4 {
        return None;
    }
    let colors_used = usize::try_from(u32_at(32)?).ok()?;
    // BI_BITFIELDS (3) and BI_ALPHABITFIELDS (6) after a 40-byte header: the masks follow it
    // (later headers hold them inside).
    let masks = match (header, compression) {
        (40, 3) => 12,
        (40, 6) => 16,
        _ => 0,
    };
    let colors = if colors_used > 0 {
        colors_used
    } else if bit_count <= 8 {
        1usize << bit_count
    } else {
        0
    };
    let mut offset = 14 + header + masks + colors * 4;
    // Windows' own V4/V5 DIBs may carry the BI_BITFIELDS masks again after the header: when
    // the data is exactly header + masks + colors + all padded rows, the pixels start later.
    if header > 40 && matches!(compression, 3 | 6) {
        let trailing = if compression == 3 { 12 } else { 16 };
        let stride = (width.checked_mul(u64::from(bit_count))?.checked_add(31)? / 32).checked_mul(4)?;
        let full = stride.checked_mul(rows)?;
        let exact = u64::try_from(header + trailing + colors * 4).ok()?.checked_add(full)?;
        if exact == u64::try_from(dib.len()).ok()? {
            offset += trailing;
        }
    }
    if offset > 14 + dib.len() {
        return None;
    }
    // Uncompressed pixels must all be there (rows are padded to 4 bytes; the last row may
    // come without its padding).
    if matches!(compression, 0 | 3 | 6) {
        let stride = (width.checked_mul(u64::from(bit_count))?.checked_add(31)? / 32).checked_mul(4)?;
        let row_bytes = width.checked_mul(u64::from(bit_count))?.checked_add(7)? / 8;
        let wanted = stride.checked_mul(rows.checked_sub(1)?)?.checked_add(row_bytes)?;
        if wanted > u64::try_from(dib.len() + 14 - offset).ok()? {
            return None;
        }
    }
    let mut out = Vec::with_capacity(14 + dib.len());
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&u32::try_from(14 + dib.len()).ok()?.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&u32::try_from(offset).ok()?.to_le_bytes());
    out.extend_from_slice(dib);
    Some(out)
}

/// `CF_UNICODETEXT` data as text: UTF-16 up to its NUL, line ends as they are.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn text_from_unicode(bytes: &[u8]) -> String {
    let units: Vec<u16> =
        bytes.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).take_while(|&unit| unit != 0).collect();
    String::from_utf16_lossy(&units)
}

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

/// `CF_UNICODETEXT` data: UTF-16 with a closing NUL.
#[cfg(windows)]
fn unicode_text(text: &str) -> Vec<u8> {
    text.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect()
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
        CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber, IsClipboardFormatAvailable,
        OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock};
    use windows::Win32::System::Ole::{CF_DIB, CF_DIBV5, CF_HDROP, CF_UNICODETEXT, DROPEFFECT_COPY, DROPEFFECT_MOVE};
    use windows::Win32::UI::Shell::{CFSTR_PREFERREDDROPEFFECT, HDROP};

    use super::{ClipboardError, ClipboardFiles, ClipboardImage, PasteKind};

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

    pub fn write_text(text: &str) -> Result<(), ClipboardError> {
        let bytes = super::unicode_text(text);
        let _open = Open::new()?;
        unsafe { EmptyClipboard() }.map_err(failed)?;
        put(u32::from(CF_UNICODETEXT.0), &bytes)
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

    fn png_format() -> u32 {
        unsafe { RegisterClipboardFormatW(windows::core::w!("PNG")) }
    }

    fn available(format: u32) -> bool {
        unsafe { IsClipboardFormatAvailable(format) }.is_ok()
    }

    /// The bytes of clipboard memory `memory` (while the clipboard is open).
    fn global_bytes(memory: HGLOBAL) -> Option<Vec<u8>> {
        unsafe {
            let size = GlobalSize(memory);
            let source = GlobalLock(memory) as *const u8;
            if source.is_null() {
                return None;
            }
            let bytes = std::slice::from_raw_parts(source, size).to_vec();
            let _ = GlobalUnlock(memory);
            (!bytes.is_empty()).then_some(bytes)
        }
    }

    fn data(format: u32) -> Option<Vec<u8>> {
        let handle = unsafe { GetClipboardData(format) }.ok()?;
        global_bytes(HGLOBAL(handle.0))
    }

    /// Formats only: the clipboard is not opened, no data is read.
    pub fn paste_kind() -> Option<PasteKind> {
        if available(u32::from(CF_HDROP.0)) {
            return None;
        }
        if available(png_format()) || available(u32::from(CF_DIBV5.0)) || available(u32::from(CF_DIB.0)) {
            return Some(PasteKind::Image);
        }
        available(u32::from(CF_UNICODETEXT.0)).then_some(PasteKind::Text)
    }

    /// PNG as it is, else the DIB (V5 first: it keeps the alpha) as a BMP file.
    pub fn read_image() -> Result<Option<ClipboardImage>, ClipboardError> {
        let _open = Open::new()?;
        if let Some(png) = data(png_format()) {
            return Ok(Some(ClipboardImage::Png(png)));
        }
        for format in [CF_DIBV5, CF_DIB] {
            if let Some(bmp) = data(u32::from(format.0)).and_then(|dib| super::bmp_from_dib(&dib)) {
                return Ok(Some(ClipboardImage::Bmp(bmp)));
            }
        }
        Ok(None)
    }

    pub fn read_text() -> Result<Option<String>, ClipboardError> {
        let _open = Open::new()?;
        Ok(data(u32::from(CF_UNICODETEXT.0)).and_then(|bytes| super::clean_text(super::text_from_unicode(&bytes))))
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
    use objc2_app_kit::{
        NSBitmapImageFileType, NSBitmapImageRep, NSPasteboard, NSPasteboardTypeFileURL, NSPasteboardTypeString,
        NSPasteboardTypeTIFF, NSPasteboardWriting,
    };
    use objc2_foundation::{NSArray, NSData, NSDictionary, NSString, NSURL};

    use super::{ClipboardError, ClipboardFiles, ClipboardImage, PasteKind};

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

    pub fn write_text(text: &str) -> Result<(), ClipboardError> {
        let pasteboard = NSPasteboard::generalPasteboard();
        pasteboard.clearContents();
        CUT_AT.set(None);
        // NSPasteboardTypeString is an extern static.
        let written =
            unsafe { pasteboard.setString_forType(&NSString::from_str(text), objc2_app_kit::NSPasteboardTypeString) };
        if written { Ok(()) } else { Err(ClipboardError::Failed("the pasteboard refused the text".into())) }
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

    /// The pasteboard's types: files first, then a picture, then text.
    pub fn paste_kind() -> Option<PasteKind> {
        let pasteboard = NSPasteboard::generalPasteboard();
        let types = pasteboard.types()?;
        let has = |wanted: &NSString| types.iter().any(|t| &*t == wanted);
        if has(unsafe { NSPasteboardTypeFileURL }) {
            None
        } else if has(&NSString::from_str("public.png")) || has(unsafe { NSPasteboardTypeTIFF }) {
            Some(PasteKind::Image)
        } else {
            has(unsafe { NSPasteboardTypeString }).then_some(PasteKind::Text)
        }
    }

    /// `public.png` as it is; else the TIFF every picture copy carries (spec 9.1), converted
    /// to PNG later, off the UI thread.
    pub fn read_image() -> Result<Option<ClipboardImage>, ClipboardError> {
        let pasteboard = NSPasteboard::generalPasteboard();
        if let Some(png) = pasteboard.dataForType(&NSString::from_str("public.png")) {
            return Ok(Some(ClipboardImage::Png(png.to_vec())));
        }
        let Some(tiff) = pasteboard.dataForType(unsafe { NSPasteboardTypeTIFF }) else { return Ok(None) };
        Ok(Some(ClipboardImage::Tiff(tiff.to_vec())))
    }

    /// A TIFF picture as PNG file bytes (run in the job that writes the file).
    pub fn tiff_to_png(tiff: &[u8]) -> Option<Vec<u8>> {
        // Autoreleased objects of a job thread are freed here, not when the thread ends.
        objc2::rc::autoreleasepool(|_| {
            let rep = NSBitmapImageRep::imageRepWithData(&NSData::with_bytes(tiff))?;
            let png =
                unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new()) };
            png.map(|data| data.to_vec())
        })
    }

    pub fn read_text() -> Result<Option<String>, ClipboardError> {
        let pasteboard = NSPasteboard::generalPasteboard();
        Ok(pasteboard
            .stringForType(unsafe { NSPasteboardTypeString })
            .and_then(|text| super::clean_text(text.to_string())))
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

    use super::{ClipboardError, ClipboardFiles, ClipboardImage, PasteKind};
    use crate::linux::backend;

    pub fn write_files(paths: &[PathBuf], cut: bool) -> Result<(), ClipboardError> {
        backend().ok_or(ClipboardError::Unsupported)?.write_files(paths, cut)
    }

    pub fn write_text(text: &str) -> Result<(), ClipboardError> {
        backend().ok_or(ClipboardError::Unsupported)?.write_text(text)
    }

    pub fn read_files() -> Result<Option<ClipboardFiles>, ClipboardError> {
        backend().ok_or(ClipboardError::Unsupported)?.read_files()
    }

    pub fn paste_kind() -> Option<PasteKind> {
        backend()?.paste_kind()
    }

    pub fn read_image() -> Result<Option<ClipboardImage>, ClipboardError> {
        backend().ok_or(ClipboardError::Unsupported)?.read_image()
    }

    pub fn read_text() -> Result<Option<String>, ClipboardError> {
        backend().ok_or(ClipboardError::Unsupported)?.read_text()
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
    use super::*;

    /// A 2×1 picture as clipboard DIB data: red then green, 24 bits, bottom-up.
    fn dib() -> Vec<u8> {
        let mut out = Vec::new();
        for field in [40u32, 2, 1] {
            out.extend_from_slice(&field.to_le_bytes()); // biSize, biWidth, biHeight
        }
        out.extend_from_slice(&1u16.to_le_bytes()); // biPlanes
        out.extend_from_slice(&24u16.to_le_bytes()); // biBitCount
        for field in [0u32, 8, 0, 0, 0, 0] {
            out.extend_from_slice(&field.to_le_bytes()); // compression, size, ppm × 2, colors × 2
        }
        out.extend_from_slice(&[0, 0, 255, 0, 255, 0, 0, 0]); // BGR BGR, padded to 4 bytes
        out
    }

    #[test]
    fn a_dib_becomes_a_bmp_file() {
        let bmp = bmp_from_dib(&dib()).unwrap();
        assert_eq!(&bmp[..2], b"BM");
        assert_eq!(u32::from_le_bytes(bmp[2..6].try_into().unwrap()) as usize, bmp.len());
        assert_eq!(u32::from_le_bytes(bmp[10..14].try_into().unwrap()), 14 + 40, "the pixels follow the header");
        let picture = image::load_from_memory_with_format(&bmp, image::ImageFormat::Bmp).unwrap().to_rgb8();
        assert_eq!((picture.width(), picture.height()), (2, 1));
        assert_eq!(picture.get_pixel(0, 0).0, [255, 0, 0]);
        assert_eq!(picture.get_pixel(1, 0).0, [0, 255, 0]);
        assert_eq!(bmp_from_dib(&dib()[..20]), None, "too short");
        // 8 bits with no count given: a full 256-color table sits before the pixels.
        let mut paletted = dib();
        paletted[14..16].copy_from_slice(&8u16.to_le_bytes());
        paletted.resize(40 + 256 * 4 + 4, 0);
        let bmp = bmp_from_dib(&paletted).unwrap();
        assert_eq!(u32::from_le_bytes(bmp[10..14].try_into().unwrap()), 14 + 40 + 1024);
    }

    /// A 2×2 picture of 32-bit pixels in storage order: red, green, then blue, white.
    fn dib32(header: u32, height: i32, compression: u32) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&header.to_le_bytes());
        out.extend_from_slice(&2i32.to_le_bytes());
        out.extend_from_slice(&height.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&compression.to_le_bytes());
        out.resize(header as usize, 0);
        // BI_BITFIELDS: the masks follow a 40-byte header, a later one holds them itself.
        let masks: [u32; 4] = [0x00FF_0000, 0x0000_FF00, 0x0000_00FF, 0xFF00_0000];
        let at = if header == 40 { out.len() } else { 40 };
        if compression == 3 || header > 40 {
            let all: Vec<u8> = masks.iter().flat_map(|m| m.to_le_bytes()).collect();
            let count = if header == 40 { 12 } else { 16 };
            if header == 40 {
                out.extend_from_slice(&all[..count]);
            } else {
                out[at..at + count].copy_from_slice(&all[..count]);
            }
        }
        for bgra in [[0, 0, 255, 255], [0, 255, 0, 255], [255, 0, 0, 255], [255, 255, 255, 255]] {
            out.extend_from_slice(&bgra);
        }
        out
    }

    #[test]
    fn a_v5_dib_with_trailing_masks_starts_its_pixels_after_them() {
        // Windows' CF_DIBV5: a 124-byte header, then 12 mask bytes again, then the pixels.
        let plain = dib32(124, 2, 3);
        let mut trailing = plain[..124].to_vec();
        for mask in [0x00FF_0000u32, 0x0000_FF00, 0x0000_00FF] {
            trailing.extend_from_slice(&mask.to_le_bytes());
        }
        trailing.extend_from_slice(&plain[124..]);
        let bmp = bmp_from_dib(&trailing).unwrap();
        assert_eq!(u32::from_le_bytes(bmp[10..14].try_into().unwrap()), 14 + 124 + 12);
        assert_eq!(top_left(&trailing), [0, 0, 255], "the first pixel is at (0, 0), not shifted");
        assert_eq!(top_left(&plain), [0, 0, 255], "without trailing masks as before");
    }

    fn top_left(dib: &[u8]) -> [u8; 3] {
        let bmp = bmp_from_dib(dib).expect("a DIB");
        let png = ClipboardImage::Bmp(bmp).png_bytes().unwrap();
        image::load_from_memory_with_format(&png, image::ImageFormat::Png).unwrap().to_rgb8().get_pixel(0, 0).0
    }

    #[test]
    fn dibs_of_every_header_kind_are_read() {
        assert_eq!(top_left(&dib32(40, 2, 0)), [0, 0, 255], "bottom-up: the last stored row is on top");
        assert_eq!(top_left(&dib32(40, -2, 0)), [255, 0, 0], "top-down: the first stored row is on top");
        assert_eq!(top_left(&dib32(40, 2, 3)), [0, 0, 255], "BI_BITFIELDS with its masks after the header");
        assert_eq!(top_left(&dib32(124, 2, 3)), [0, 0, 255], "a V5 header");
        assert_eq!(top_left(&dib32(124, -2, 3)), [255, 0, 0], "a top-down V5");
    }

    #[test]
    fn a_last_row_without_its_padding_is_accepted() {
        let mut short = dib();
        short.truncate(short.len() - 2); // 6 bytes of pixels: no padding after the row
        assert!(bmp_from_dib(&short).is_some());
        short.pop();
        assert_eq!(bmp_from_dib(&short), None, "a pixel is missing");
        // Two rows of one 24-bit pixel each (3 bytes + 1 padding): the second may lack its padding.
        let mut two = dib();
        two[4..8].copy_from_slice(&1i32.to_le_bytes());
        two[8..12].copy_from_slice(&2i32.to_le_bytes());
        two.truncate(40);
        two.extend_from_slice(&[1, 2, 3, 0, 4, 5, 6]);
        assert!(bmp_from_dib(&two).is_some());
        two.pop();
        assert_eq!(bmp_from_dib(&two), None);
    }

    #[test]
    fn a_header_claiming_more_than_the_data_is_refused() {
        let mut huge = dib();
        huge[4..8].copy_from_slice(&100_000i32.to_le_bytes());
        huge[8..12].copy_from_slice(&100_000i32.to_le_bytes());
        assert_eq!(bmp_from_dib(&huge), None, "10 billion pixels in a few bytes");
        let mut tall = dib();
        tall[8..12].copy_from_slice(&5i32.to_le_bytes());
        assert_eq!(bmp_from_dib(&tall), None, "five rows with one row of data");
        for (width, height) in [(0i32, 1i32), (-2, 1), (2, 0)] {
            let mut bad = dib();
            bad[4..8].copy_from_slice(&width.to_le_bytes());
            bad[8..12].copy_from_slice(&height.to_le_bytes());
            assert_eq!(bmp_from_dib(&bad), None, "{width} x {height}");
        }
        let mut wrapping = dib();
        wrapping[4..8].copy_from_slice(&i32::MIN.to_le_bytes());
        wrapping[8..12].copy_from_slice(&i32::MIN.to_le_bytes());
        assert_eq!(bmp_from_dib(&wrapping), None);
    }

    #[test]
    fn dibs_become_pngs_with_the_same_pixels() {
        fn header(bits: u16, height: i32) -> Vec<u8> {
            let mut h = Vec::new();
            h.extend_from_slice(&40u32.to_le_bytes());
            h.extend_from_slice(&2i32.to_le_bytes());
            h.extend_from_slice(&height.to_le_bytes());
            h.extend_from_slice(&1u16.to_le_bytes());
            h.extend_from_slice(&bits.to_le_bytes());
            h.extend_from_slice(&[0; 24]);
            h
        }
        fn decode(dib: &[u8]) -> (png::ColorType, Vec<u8>) {
            let png = ClipboardImage::Bmp(bmp_from_dib(dib).unwrap()).png_bytes().unwrap();
            let mut reader = png::Decoder::new(std::io::Cursor::new(png)).read_info().unwrap();
            let mut buf = vec![0; reader.output_buffer_size().unwrap()];
            let info = reader.next_frame(&mut buf).unwrap();
            buf.truncate(info.buffer_size());
            (info.color_type, buf)
        }
        // 24-bit, bottom-up, 2x2: rows padded to 8 bytes; BGR in the file.
        let mut dib24 = header(24, 2);
        dib24.extend_from_slice(&[255, 0, 0, 0, 255, 0, 0, 0]); // bottom: blue, green
        dib24.extend_from_slice(&[0, 0, 255, 255, 255, 255, 0, 0]); // top: red, white
        let (color, pixels) = decode(&dib24);
        assert_eq!(color, png::ColorType::Rgb);
        assert_eq!(pixels, [255, 0, 0, 255, 255, 255, 0, 0, 255, 0, 255, 0]);
        // 32-bit with alpha (a V4 header with ARGB masks), bottom-up: the last row in the file is on top.
        let mut dib32 = dib32(108, 2, 3);
        let at = dib32.len() - 16;
        for (i, bgra) in [[255, 0, 0, 64], [10, 20, 30, 0], [0, 0, 255, 128], [0, 255, 0, 255]].iter().enumerate() {
            dib32[at + i * 4..at + i * 4 + 4].copy_from_slice(bgra);
        }
        let (color, pixels) = decode(&dib32);
        assert_eq!(color, png::ColorType::Rgba);
        assert_eq!(pixels, [255, 0, 0, 128, 0, 255, 0, 255, 0, 0, 255, 64, 30, 20, 10, 0]);
        // 32-bit BI_RGB: the alpha byte is 0 but unused, so the PNG is opaque RGB.
        let mut plain32 = header(32, 2);
        for bgra in [[255, 0, 0, 0], [0, 255, 0, 0], [0, 0, 255, 0], [10, 20, 30, 0]] {
            plain32.extend_from_slice(&bgra);
        }
        let (color, pixels) = decode(&plain32);
        assert_eq!(color, png::ColorType::Rgb);
        assert_eq!(pixels, [255, 0, 0, 30, 20, 10, 0, 0, 255, 0, 255, 0]);
        // 8-bit palettized: palette red, blue; rows padded to 4 bytes, bottom row first.
        let mut dib8 = header(8, 2);
        dib8[32..36].copy_from_slice(&2u32.to_le_bytes());
        dib8.extend_from_slice(&[0, 0, 255, 0, 255, 0, 0, 0]);
        dib8.extend_from_slice(&[1, 0, 0, 0, 0, 1, 0, 0]);
        let (color, pixels) = decode(&dib8);
        assert_eq!(color, png::ColorType::Rgb);
        assert_eq!(pixels, [255, 0, 0, 0, 0, 255, 0, 0, 255, 255, 0, 0]);
    }

    #[test]
    fn a_picture_is_written_as_png() {
        let png = ClipboardImage::Bmp(bmp_from_dib(&dib()).unwrap()).png_bytes().unwrap();
        let back = image::load_from_memory_with_format(&png, image::ImageFormat::Png).unwrap().to_rgb8();
        assert_eq!(back.get_pixel(0, 0).0, [255, 0, 0]);
        assert_eq!(ClipboardImage::Png(png.clone()).png_bytes().unwrap(), png, "PNG data as it came");
        assert!(ClipboardImage::Bmp(b"BM nonsense".to_vec()).png_bytes().is_err());
    }

    #[test]
    fn unicode_text_ends_at_its_nul() {
        let bytes: Vec<u8> = "a \u{15f}\r\nb\0junk".encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert_eq!(text_from_unicode(&bytes), "a \u{15f}\r\nb", "line ends kept as they are");
    }

    /// Uses the real clipboard: run by hand (`cargo test -p gezik-platform -- --ignored`).
    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    #[ignore = "replaces the user's clipboard"]
    fn text_comes_back_as_text_to_paste() {
        write_text(
            "satır 1
satır 2",
        )
        .unwrap();
        assert_eq!(paste_kind(), Some(PasteKind::Text));
        assert_eq!(
            read_text().unwrap().as_deref(),
            Some(
                "satır 1
satır 2"
            )
        );
        write_files(&[std::env::temp_dir().join("a")], false).unwrap();
        assert_eq!(paste_kind(), None, "files paste as files");
        clear().unwrap();
        assert_eq!(paste_kind(), None);
    }

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

    #[cfg(windows)]
    #[test]
    fn unicode_text_is_utf16_with_a_nul() {
        assert_eq!(unicode_text("aş"), [0x61, 0, 0x5F, 0x01, 0, 0]);
    }

    /// Uses the real clipboard: run by hand (`cargo test -p gezik-platform -- --ignored`).
    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    #[ignore = "replaces the user's clipboard"]
    fn text_replaces_files_on_the_clipboard() {
        write_files(&[std::env::temp_dir().join("a")], true).unwrap();
        let before = sequence();
        write_text("C:\\a b\\ş.txt").unwrap();
        assert_ne!(sequence(), before);
        assert_eq!(read_files().unwrap(), None, "no files once text is copied");
        clear().unwrap();
    }
}
