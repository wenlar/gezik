//! Pictures: Gezik's own decoder (previews, and thumbnails where the system has none) and
//! the system's thumbnails (Windows' shell, macOS' Quick Look). May block: call from a
//! worker thread that ran `init_thread`.

use std::path::Path;

use crate::Rgba;

/// Files bigger than this are not decoded (the system thumbnail is used instead).
pub const MAX_DECODE_BYTES: u64 = 100 * 1024 * 1024;
/// Pictures wider or taller than this are not decoded.
pub const MAX_DECODE_SIDE: u32 = 8192;
/// The most memory one decode may take (an 8192×8192 RGBA picture just fits).
pub const MAX_DECODE_ALLOC: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct Decoded {
    /// At most `max_px` on its longer side.
    pub image: Rgba,
    /// The picture's own size.
    pub width: u32,
    pub height: u32,
}

/// Whether Gezik decodes this extension itself.
pub fn can_decode(ext: &str) -> bool {
    matches!(ext.to_ascii_lowercase().as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp")
}

/// Decodes a picture (the first frame of a GIF), shrunk to fit `max_px`. Refuses files
/// over [`MAX_DECODE_BYTES`] and pictures over [`MAX_DECODE_SIDE`]; never panics on bad data.
pub fn decode_image(path: &Path, max_px: u32) -> Result<Decoded, String> {
    let size = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    if size > MAX_DECODE_BYTES {
        return Err(format!("{size} bytes is too big to preview"));
    }
    let mut reader =
        image::ImageReader::open(path).map_err(|e| e.to_string())?.with_guessed_format().map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DECODE_SIDE);
    limits.max_image_height = Some(MAX_DECODE_SIDE);
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    reader.limits(limits);
    let picture = reader.decode().map_err(|e| e.to_string())?;
    let (width, height) = (picture.width(), picture.height());
    let max_px = max_px.max(1);
    let picture = if width > max_px || height > max_px { picture.thumbnail(max_px, max_px) } else { picture };
    let rgba = picture.into_rgba8();
    Ok(Decoded { image: Rgba { width: rgba.width(), height: rgba.height(), pixels: rgba.into_raw() }, width, height })
}

/// A thumbnail at most `px` on its longer side: the system's (Windows), else Gezik's own for
/// the formats it decodes, else Quick Look's (macOS). `None` if there is none.
pub fn thumbnail(path: &Path, px: u32) -> Option<Rgba> {
    thumbnail_while(path, px, &|| true)
}

/// [`thumbnail`], given up once `wanted` says it is no longer needed: macOS cancels the Quick
/// Look request; elsewhere nothing waits, and `wanted` is not asked.
pub fn thumbnail_while(path: &Path, px: u32, wanted: &dyn Fn() -> bool) -> Option<Rgba> {
    #[cfg(windows)]
    if let Some(image) = win::thumbnail(path, px) {
        return Some(image);
    }
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or_default();
    if can_decode(ext) {
        return decode_image(path, px).ok().map(|d| d.image);
    }
    #[cfg(target_os = "macos")]
    return crate::mac::thumbs::thumbnail(path, px, wanted);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = wanted;
        None
    }
}

/// Waits for the answer on `receive` while `wanted` says so, at most `limit`, asking `wanted`
/// every `step`. `None` when it gave up or the sender went away: the caller cancels.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn wait_while<T>(
    receive: &std::sync::mpsc::Receiver<T>,
    wanted: &dyn Fn() -> bool,
    limit: std::time::Duration,
    step: std::time::Duration,
) -> Option<T> {
    let deadline = std::time::Instant::now() + limit;
    loop {
        match receive.recv_timeout(step) {
            Ok(answer) => return Some(answer),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) if wanted() && std::time::Instant::now() < deadline => {}
            Err(_) => return None,
        }
    }
}

#[cfg(windows)]
mod win {
    use std::path::Path;

    use windows::Win32::Foundation::SIZE;
    use windows::Win32::Graphics::Gdi::DeleteObject;
    use windows::Win32::System::Com::IBindCtx;
    use windows::Win32::UI::Shell::{IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_THUMBNAILONLY};
    use windows::core::HSTRING;

    use crate::Rgba;

    /// The shell's thumbnail (its cache, or the type's thumbnail handler); `None` if the
    /// type has no thumbnails.
    pub fn thumbnail(path: &Path, px: u32) -> Option<Rgba> {
        let side = px.clamp(1, 1024) as i32;
        // SAFETY: the bitmap is ours to delete once converted.
        unsafe {
            let factory: IShellItemImageFactory =
                SHCreateItemFromParsingName(&HSTRING::from(path.as_os_str()), None::<&IBindCtx>).ok()?;
            let bitmap = factory.GetImage(SIZE { cx: side, cy: side }, SIIGBF_THUMBNAILONLY).ok()?;
            let image = crate::icons::win::bitmap_to_rgba(bitmap);
            let _ = DeleteObject(bitmap.into());
            let mut image = image?;
            // Photos come without alpha: show them opaque.
            if image.pixels.as_chunks::<4>().0.iter().all(|p| p[3] == 0) {
                image.pixels.as_chunks_mut::<4>().0.iter_mut().for_each(|p| p[3] = 255);
            }
            Some(image)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waiting_ends_with_the_answer_or_when_no_longer_wanted() {
        use std::time::{Duration, Instant};
        let step = Duration::from_millis(10);
        let (send, receive) = std::sync::mpsc::sync_channel::<i32>(1);
        send.send(5).unwrap();
        assert_eq!(wait_while(&receive, &|| true, Duration::from_secs(5), step), Some(5));
        let start = Instant::now();
        assert_eq!(wait_while(&receive, &|| false, Duration::from_secs(5), step), None, "no longer wanted");
        assert!(start.elapsed() < Duration::from_secs(1), "gives up at the first look");
        let start = Instant::now();
        assert_eq!(wait_while(&receive, &|| true, Duration::from_millis(50), step), None, "the limit");
        assert!(start.elapsed() >= Duration::from_millis(50));
        drop(send);
        let start = Instant::now();
        assert_eq!(wait_while(&receive, &|| true, Duration::from_secs(5), step), None, "the sender went away");
        assert!(start.elapsed() < Duration::from_secs(1));
    }

    fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("gezik-picture-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn png(path: &Path, width: u32, height: u32) {
        image::RgbaImage::from_pixel(width, height, image::Rgba([200, 30, 30, 255])).save(path).unwrap();
    }

    #[test]
    fn decodes_and_shrinks_keeping_the_aspect() {
        let dir = temp("shrink");
        let path = dir.join("a.png");
        png(&path, 300, 200);
        let decoded = decode_image(&path, 100).unwrap();
        assert_eq!((decoded.width, decoded.height), (300, 200));
        assert_eq!((decoded.image.width, decoded.image.height), (100, 67));
        assert_eq!(&decoded.image.pixels[..4], &[200, 30, 30, 255]);
        let small = decode_image(&path, 1000).unwrap();
        assert_eq!((small.image.width, small.image.height), (300, 200), "never enlarged");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn refuses_huge_and_broken_images() {
        let dir = temp("refuse");
        let wide = dir.join("wide.png");
        png(&wide, MAX_DECODE_SIDE + 1, 1);
        assert!(decode_image(&wide, 64).is_err());
        let broken = dir.join("broken.png");
        std::fs::write(&broken, b"\x89PNG\r\n\x1a\nnot really").unwrap();
        assert!(decode_image(&broken, 64).is_err());
        let text = dir.join("text.jpg");
        std::fs::write(&text, "hello").unwrap();
        assert!(decode_image(&text, 64).is_err());
        assert!(decode_image(&dir.join("missing.png"), 64).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn knows_which_formats_it_decodes() {
        assert!(can_decode("JPG") && can_decode("webp"));
        assert!(!can_decode("heic") && !can_decode(""));
    }

    #[test]
    fn thumbnails_fit_the_requested_size() {
        crate::init_thread();
        let dir = temp("thumb");
        let path = dir.join("t.png");
        png(&path, 400, 100);
        let thumb = thumbnail(&path, 64).expect("a thumbnail");
        assert!(thumb.width <= 64 && thumb.height <= 64 && thumb.width > 0, "{thumb:?}");
        assert!(thumbnail(&dir.join("none.png"), 64).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
