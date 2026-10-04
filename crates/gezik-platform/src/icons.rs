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

/// The system's icon for `target`, at most `px` wide: smaller sizes come as the system has
/// them (the UI scales them), bigger ones are shrunk to `px`. `None` where there are no
/// system icons: macOS and Linux use Gezik's own icons for now.
pub fn icon(target: &IconTarget, px: u32) -> Option<Rgba> {
    imp::icon(target, px)
}

/// The width and height of the part of `image` that is not fully transparent, measured
/// from the top-left corner (where the shell puts small icons in a big canvas); `(0, 0)`
/// if all of it is transparent.
pub(crate) fn drawn_extent(image: &Rgba) -> (u32, u32) {
    let (mut right, mut bottom) = (0, 0);
    let width = image.width as usize;
    if width == 0 {
        return (0, 0);
    }
    for (row, line) in image.pixels.chunks(width * 4).enumerate() {
        if let Some(last) = line.as_chunks::<4>().0.iter().rposition(|p| p[3] != 0) {
            right = right.max(last as u32 + 1);
            bottom = row as u32 + 1;
        }
    }
    (right, bottom)
}

/// Whether a 256 px ("jumbo") icon is really a small one in a corner of a transparent
/// canvas: then the 48 px icon looks better.
pub(crate) fn is_small_in_big_canvas(image: &Rgba) -> bool {
    let (width, height) = drawn_extent(image);
    width <= 48 && height <= 48
}

/// `image` shrunk to fit `px` (keeping its aspect), or as it is if it already fits.
/// Shrinks premultiplied, so transparent pixels do not darken the edges.
pub(crate) fn shrink_to(image: Rgba, px: u32) -> Rgba {
    let px = px.max(1);
    if image.width <= px && image.height <= px {
        return image;
    }
    let (width, height) = (image.width, image.height);
    let Some(mut buffer) = image::RgbaImage::from_raw(width, height, image.pixels) else {
        return Rgba { width: 0, height: 0, pixels: Vec::new() };
    };
    for pixel in buffer.pixels_mut() {
        let a = u16::from(pixel[3]);
        for c in &mut pixel.0[..3] {
            *c = ((u16::from(*c) * a + 127) / 255) as u8;
        }
    }
    let scale = f64::from(px) / f64::from(width.max(height));
    let (w, h) = (
        ((f64::from(width) * scale).round() as u32).clamp(1, px),
        ((f64::from(height) * scale).round() as u32).clamp(1, px),
    );
    let mut small = image::imageops::resize(&buffer, w, h, image::imageops::FilterType::CatmullRom);
    for pixel in small.pixels_mut() {
        let a = u16::from(pixel[3]);
        for c in &mut pixel.0[..3] {
            if let Some(straight) = (u16::from(*c) * 255 + a / 2).checked_div(a) {
                *c = straight.min(255) as u8;
            }
        }
    }
    Rgba { width: w, height: h, pixels: small.into_raw() }
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

    /// Looks up `name`'s place in the system image list (0 if it fails).
    fn icon_index(
        name: &HSTRING,
        attributes: FILE_FLAGS_AND_ATTRIBUTES,
        flags: SHGFI_FLAGS,
        info: &mut SHFILEINFOW,
    ) -> usize {
        // The first lookup sets up the system image list; lookups racing it on other
        // threads fail, so the first one runs alone.
        static FIRST: std::sync::Once = std::sync::Once::new();
        FIRST.call_once(|| {
            let (name, attributes, flags) = query(&IconTarget::Folder);
            let mut info = SHFILEINFOW::default();
            // SAFETY: as below.
            unsafe {
                SHGetFileInfoW(
                    &name,
                    attributes,
                    Some(&mut info),
                    size_of::<SHFILEINFOW>() as u32,
                    flags | SHGFI_SYSICONINDEX,
                );
            }
        });
        // SAFETY: `info` is a live SHFILEINFOW and its size is passed.
        unsafe {
            SHGetFileInfoW(name, attributes, Some(info), size_of::<SHFILEINFOW>() as u32, flags | SHGFI_SYSICONINDEX)
        }
    }

    pub fn icon(target: &IconTarget, px: u32) -> Option<Rgba> {
        let (name, attributes, flags) = query(target);
        let mut info = SHFILEINFOW::default();
        if icon_index(&name, attributes, flags, &mut info) == 0 {
            return None;
        }
        let which = match px {
            0..=16 => SHIL_SMALL,
            17..=32 => SHIL_LARGE,
            33..=48 => SHIL_EXTRALARGE,
            _ => SHIL_JUMBO,
        };
        let icon = from_list(which, info.iIcon)?;
        if which != SHIL_JUMBO {
            return Some(icon);
        }
        // Jumbo icons are always 256 px; old icons sit small in their top-left corner.
        if super::is_small_in_big_canvas(&icon)
            && let Some(large) = from_list(SHIL_EXTRALARGE, info.iIcon)
        {
            return Some(large);
        }
        Some(super::shrink_to(icon, px))
    }

    /// Image `index` of the system image list `which`.
    fn from_list(which: u32, index: i32) -> Option<Rgba> {
        // SAFETY: the icon is ours to destroy once converted.
        unsafe {
            let images: IImageList = SHGetImageList(which as i32).ok()?;
            let icon = images.GetIcon(index, ILD_TRANSPARENT.0).ok()?;
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

    #[test]
    fn icons_load_on_several_threads_at_once() {
        let threads: Vec<_> = (0..4)
            .map(|t| {
                std::thread::spawn(move || {
                    init_thread();
                    (0..8).all(|i| icon(&IconTarget::Extension("txt".into()), [16, 32, 48, 96][(i + t) % 4]).is_some())
                })
            })
            .collect();
        assert!(threads.into_iter().all(|t| t.join().unwrap()));
    }

    #[test]
    fn big_icons_come_at_the_requested_size() {
        init_thread();
        for target in [IconTarget::Folder, IconTarget::Extension("txt".into())] {
            let icon = icon(&target, 96).unwrap_or_else(|| panic!("no icon for {target:?}"));
            assert!(icon.width <= 96 && icon.height <= 96 && icon.width >= 48, "{target:?}: {icon:?}");
            assert_eq!(icon.pixels.len(), (icon.width * icon.height * 4) as usize);
        }
    }
}

#[cfg(test)]
mod size_tests {
    use super::*;

    /// A `size`×`size` transparent canvas with an opaque `w`×`h` block at the top left.
    fn canvas(size: u32, w: u32, h: u32) -> Rgba {
        let mut pixels = vec![0u8; (size * size * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let i = ((y * size + x) * 4) as usize;
                pixels[i..i + 4].copy_from_slice(&[10, 20, 30, 255]);
            }
        }
        Rgba { width: size, height: size, pixels }
    }

    #[test]
    fn measures_what_is_drawn() {
        assert_eq!(drawn_extent(&canvas(256, 32, 40)), (32, 40));
        assert_eq!(drawn_extent(&canvas(256, 0, 0)), (0, 0));
        assert_eq!(drawn_extent(&canvas(256, 256, 256)), (256, 256));
        assert_eq!(drawn_extent(&Rgba { width: 0, height: 0, pixels: Vec::new() }), (0, 0));
    }

    #[test]
    fn small_icons_in_a_big_canvas_fall_back() {
        assert!(is_small_in_big_canvas(&canvas(256, 48, 48)));
        assert!(is_small_in_big_canvas(&canvas(256, 32, 32)));
        assert!(is_small_in_big_canvas(&canvas(256, 0, 0)));
        assert!(!is_small_in_big_canvas(&canvas(256, 49, 20)));
        assert!(!is_small_in_big_canvas(&canvas(256, 200, 256)));
    }

    #[test]
    fn shrinks_to_the_requested_size() {
        let small = shrink_to(canvas(256, 256, 256), 96);
        assert_eq!((small.width, small.height), (96, 96));
        assert_eq!(small.pixels.len(), 96 * 96 * 4);
        assert_eq!(&small.pixels[..4], &[10, 20, 30, 255]);
        let wide = shrink_to(Rgba { width: 200, height: 100, pixels: vec![255; 200 * 100 * 4] }, 50);
        assert_eq!((wide.width, wide.height), (50, 25));
        let fits = shrink_to(canvas(48, 48, 48), 96);
        assert_eq!((fits.width, fits.height), (48, 48), "never enlarged");
    }
}
