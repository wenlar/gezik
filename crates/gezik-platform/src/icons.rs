//! System icons and type names for the file list. Every function here may block (disk,
//! shell extensions): call them from a worker thread that ran [`init_thread`].

use std::path::PathBuf;

/// An image as straight (not premultiplied) RGBA8, top row first.
#[derive(Clone, PartialEq, Eq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl std::fmt::Debug for Rgba {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Rgba({}×{})", self.width, self.height)
    }
}

/// Whose icon to look up.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IconTarget {
    /// Any file with this extension (lowercase, no dot; `""` for none). No disk access.
    Extension(String),
    /// A plain folder. No disk access.
    Folder,
    /// This very file, folder or drive: programs, shortcuts, customized folders, drives.
    Path(PathBuf),
}

/// Prepares the calling thread for icons, type names and thumbnails (COM on Windows).
pub fn init_thread() {
    #[cfg(windows)]
    win::init_thread();
}

/// The system's name for a type, e.g. `Text Document`; `None` if it has none.
pub fn type_name(ext: &str, is_dir: bool) -> Option<String> {
    imp::type_name(ext, is_dir)
}

/// The system's icon for `target`, at least `px` wide where the system has one that big
/// (the UI scales it). `None` where there are no system icons: macOS and Linux use
/// Gezik's own icons for now.
pub fn icon(target: &IconTarget, px: u32) -> Option<Rgba> {
    imp::icon(target, px)
}

#[cfg(windows)]
use win as imp;

#[cfg(not(windows))]
mod imp {
    use super::{IconTarget, Rgba};

    pub fn icon(_target: &IconTarget, _px: u32) -> Option<Rgba> {
        None
    }

    #[cfg(target_os = "macos")]
    pub fn type_name(_ext: &str, _is_dir: bool) -> Option<String> {
        None
    }

    #[cfg(not(target_os = "macos"))]
    pub fn type_name(ext: &str, is_dir: bool) -> Option<String> {
        super::mime::type_name(ext, is_dir)
    }
}

/// Linux: type names from shared-mime-info.
#[cfg(all(unix, not(target_os = "macos")))]
mod mime {
    use std::collections::HashMap;
    use std::sync::OnceLock;

    const MIME_DIR: &str = "/usr/share/mime";

    /// Extension → MIME type, from `globs2` (it lists higher weights first: the first wins).
    fn globs() -> &'static HashMap<String, String> {
        static GLOBS: OnceLock<HashMap<String, String>> = OnceLock::new();
        GLOBS.get_or_init(|| {
            let text = std::fs::read_to_string(format!("{MIME_DIR}/globs2")).unwrap_or_default();
            let mut map = HashMap::new();
            for line in text.lines().filter(|l| !l.starts_with('#')) {
                let mut parts = line.split(':');
                let (Some(_weight), Some(mime), Some(glob)) = (parts.next(), parts.next(), parts.next()) else {
                    continue;
                };
                if let Some(ext) = glob.strip_prefix("*.")
                    && !ext.contains(['*', '?', '['])
                {
                    map.entry(ext.to_lowercase()).or_insert_with(|| mime.to_owned());
                }
            }
            map
        })
    }

    pub fn type_name(ext: &str, is_dir: bool) -> Option<String> {
        let mime = if is_dir { "inode/directory".to_owned() } else { globs().get(&ext.to_lowercase())?.clone() };
        comment(&std::fs::read_to_string(format!("{MIME_DIR}/{mime}.xml")).ok()?)
    }

    /// The first `<comment>` without a language: the English description.
    fn comment(xml: &str) -> Option<String> {
        let start = xml.find("<comment>")? + "<comment>".len();
        let end = start + xml[start..].find("</comment>")?;
        let text = xml[start..end]
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&amp;", "&");
        Some(text)
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn reads_the_unlocalized_comment() {
            let xml =
                "<mime-type><comment xml:lang=\"tr\">Metin</comment><comment>Plain &amp; simple</comment></mime-type>";
            assert_eq!(super::comment(xml).as_deref(), Some("Plain & simple"));
        }
    }
}

#[cfg(windows)]
pub(crate) mod win {
    use std::mem::size_of;

    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC, DeleteObject,
        GetDIBits, GetObjectW, HBITMAP,
    };
    use windows::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL, FILE_FLAGS_AND_ATTRIBUTES,
    };
    use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
    use windows::Win32::UI::Controls::{IImageList, ILD_TRANSPARENT};
    use windows::Win32::UI::Shell::{
        SHFILEINFOW, SHGFI_FLAGS, SHGFI_SYSICONINDEX, SHGFI_TYPENAME, SHGFI_USEFILEATTRIBUTES, SHGetFileInfoW,
        SHGetImageList, SHIL_EXTRALARGE, SHIL_JUMBO, SHIL_LARGE, SHIL_SMALL,
    };
    use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};
    use windows::core::HSTRING;

    use super::{IconTarget, Rgba};

    pub fn init_thread() {
        // SAFETY: plain COM initialization of this thread; a second call just reports it.
        let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    }

    /// What to ask the shell about `target`: a name, its attributes, and whether the name is
    /// only a pattern (no disk access).
    fn query(target: &IconTarget) -> (HSTRING, FILE_FLAGS_AND_ATTRIBUTES, SHGFI_FLAGS) {
        match target {
            IconTarget::Extension(ext) if ext.is_empty() => {
                (HSTRING::from("file"), FILE_ATTRIBUTE_NORMAL, SHGFI_USEFILEATTRIBUTES)
            }
            IconTarget::Extension(ext) => {
                (HSTRING::from(format!("file.{ext}")), FILE_ATTRIBUTE_NORMAL, SHGFI_USEFILEATTRIBUTES)
            }
            IconTarget::Folder => (HSTRING::from("folder"), FILE_ATTRIBUTE_DIRECTORY, SHGFI_USEFILEATTRIBUTES),
            IconTarget::Path(path) => (HSTRING::from(path.as_os_str()), FILE_FLAGS_AND_ATTRIBUTES(0), SHGFI_FLAGS(0)),
        }
    }

    pub fn type_name(ext: &str, is_dir: bool) -> Option<String> {
        let target = if is_dir { IconTarget::Folder } else { IconTarget::Extension(ext.to_lowercase()) };
        let (name, attributes, flags) = query(&target);
        let mut info = SHFILEINFOW::default();
        // SAFETY: `info` is a live SHFILEINFOW and its size is passed.
        let found = unsafe {
            SHGetFileInfoW(&name, attributes, Some(&mut info), size_of::<SHFILEINFOW>() as u32, flags | SHGFI_TYPENAME)
        };
        if found == 0 {
            return None;
        }
        let end = info.szTypeName.iter().position(|&c| c == 0).unwrap_or(info.szTypeName.len());
        let text = String::from_utf16_lossy(&info.szTypeName[..end]);
        (!text.is_empty()).then_some(text)
    }

    pub fn icon(target: &IconTarget, px: u32) -> Option<Rgba> {
        let (name, attributes, flags) = query(target);
        let mut info = SHFILEINFOW::default();
        // SAFETY: as in `type_name`.
        let list = unsafe {
            SHGetFileInfoW(
                &name,
                attributes,
                Some(&mut info),
                size_of::<SHFILEINFOW>() as u32,
                flags | SHGFI_SYSICONINDEX,
            )
        };
        if list == 0 {
            return None;
        }
        let which = match px {
            0..=16 => SHIL_SMALL,
            17..=32 => SHIL_LARGE,
            33..=48 => SHIL_EXTRALARGE,
            _ => SHIL_JUMBO,
        };
        // SAFETY: the icon is ours to destroy once converted.
        unsafe {
            let images: IImageList = SHGetImageList(which as i32).ok()?;
            let icon = images.GetIcon(info.iIcon, ILD_TRANSPARENT.0).ok()?;
            let rgba = icon_to_rgba(icon);
            let _ = DestroyIcon(icon);
            rgba
        }
    }

    unsafe fn icon_to_rgba(icon: HICON) -> Option<Rgba> {
        let mut info = ICONINFO::default();
        // SAFETY: GetIconInfo hands us two bitmaps, deleted below.
        unsafe { GetIconInfo(icon, &mut info).ok()? };
        let color = unsafe { bitmap_to_rgba(info.hbmColor) };
        let mask = unsafe { bitmap_to_rgba(info.hbmMask) };
        unsafe {
            let _ = DeleteObject(info.hbmColor.into());
            let _ = DeleteObject(info.hbmMask.into());
        }
        let mut color = color?;
        // Old-style icons have no alpha: the mask says what shows (black) and what not.
        if color.pixels.as_chunks::<4>().0.iter().all(|p| p[3] == 0) {
            match mask.filter(|m| m.width == color.width && m.height >= color.height) {
                Some(mask) => {
                    for (pixel, m) in color.pixels.as_chunks_mut::<4>().0.iter_mut().zip(mask.pixels.as_chunks::<4>().0)
                    {
                        pixel[3] = if m[0] == 0 { 255 } else { 0 };
                    }
                }
                None => color.pixels.as_chunks_mut::<4>().0.iter_mut().for_each(|p| p[3] = 255),
            }
        }
        Some(color)
    }

    /// A GDI bitmap as RGBA, top row first. Alpha is passed through as it is; an all-zero
    /// alpha is for the caller to decide (icons: use the mask; thumbnails: opaque).
    pub(crate) unsafe fn bitmap_to_rgba(bitmap: HBITMAP) -> Option<Rgba> {
        if bitmap.is_invalid() {
            return None;
        }
        let mut header = BITMAP::default();
        // SAFETY: `header` is a live BITMAP and its size is passed.
        let got =
            unsafe { GetObjectW(bitmap.into(), size_of::<BITMAP>() as i32, Some((&mut header as *mut BITMAP).cast())) };
        if got == 0 || header.bmWidth <= 0 || header.bmHeight == 0 {
            return None;
        }
        let (width, height) = (header.bmWidth as u32, header.bmHeight.unsigned_abs());
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                // Negative: top-down rows.
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pixels = vec![0u8; width as usize * height as usize * 4];
        // SAFETY: `pixels` holds `height` rows of `width` 32-bit pixels, as `info` says.
        let lines = unsafe {
            let dc = CreateCompatibleDC(None);
            let lines = GetDIBits(dc, bitmap, 0, height, Some(pixels.as_mut_ptr().cast()), &mut info, DIB_RGB_COLORS);
            let _ = DeleteDC(dc);
            lines
        };
        if lines == 0 {
            return None;
        }
        for pixel in pixels.as_chunks_mut::<4>().0.iter_mut() {
            pixel.swap(0, 2); // BGRA → RGBA
        }
        Some(Rgba { width, height, pixels })
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn names_file_types_and_folders() {
        init_thread();
        assert!(type_name("txt", false).is_some_and(|t| !t.is_empty()));
        assert!(type_name("", true).is_some());
    }

    #[test]
    fn draws_type_folder_and_file_icons() {
        init_thread();
        let exe = std::env::current_exe().unwrap();
        for (target, px) in
            [(IconTarget::Extension("txt".into()), 16), (IconTarget::Folder, 48), (IconTarget::Path(exe), 32)]
        {
            let icon = icon(&target, px).unwrap_or_else(|| panic!("no icon for {target:?}"));
            assert!(icon.width >= 16 && icon.height >= 16, "{target:?}: {icon:?}");
            assert_eq!(icon.pixels.len(), (icon.width * icon.height * 4) as usize);
            assert!(icon.pixels.as_chunks::<4>().0.iter().any(|p| p[3] > 0), "{target:?} is all transparent");
        }
    }
}
