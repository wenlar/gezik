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
    // Turned as the EXIF orientation says, so a phone's portrait photo stands upright.
    let mut decoder = reader.into_decoder().map_err(|e| e.to_string())?;
    let orientation =
        image::ImageDecoder::orientation(&mut decoder).unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut picture = image::DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    picture.apply_orientation(orientation);
    let (width, height) = (picture.width(), picture.height());
    let max_px = max_px.max(1);
    let picture = if width > max_px || height > max_px { picture.thumbnail(max_px, max_px) } else { picture };
    let rgba = picture.into_rgba8();
    Ok(Decoded { image: Rgba { width: rgba.width(), height: rgba.height(), pixels: rgba.into_raw() }, width, height })
}

/// A thumbnail at most `px` on its longer side: the system's (Windows; Linux's thumbnail
/// cache), else Gezik's own for the formats it decodes, else Quick Look's (macOS). `None` if
/// there is none.
pub fn thumbnail(path: &Path, px: u32) -> Option<Rgba> {
    thumbnail_while(path, px, &|| true)
}

/// [`thumbnail`], given up once `wanted` says it is no longer needed: macOS cancels the Quick
/// Look request; elsewhere nothing waits, and `wanted` is not asked.
pub fn thumbnail_while(path: &Path, px: u32, wanted: &dyn Fn() -> bool) -> Option<Rgba> {
    let in_cloud = only_in_cloud(path);
    // Windows asks the shell first; for a cloud file only its cache (the cloud app's own
    // thumbnail), never a handler that would read the data (spec 9 §7.3).
    #[cfg(windows)]
    if let Some(image) = win::thumbnail(path, px, in_cloud) {
        return Some(image);
    }
    if in_cloud {
        return None;
    }
    // Linux: a current thumbnail another program left in the freedesktop cache (spec 9 §8.5).
    #[cfg(all(unix, not(target_os = "macos")))]
    if let Some(root) = dirs::cache_dir().map(|cache| cache.join("thumbnails"))
        && let Some(image) = crate::linux::thumbs::cached(path, px, &root)
    {
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

/// Whether the file's data is only in the cloud (Windows placeholders, iCloud, File
/// Provider), so reading it would download it (spec 9 §4.5, §7.3). Follows a link; reads no
/// data. Always `false` on Linux.
pub fn only_in_cloud(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|meta| gezik_core::attribute_flags(&meta) & gezik_core::Entry::CLOUD_ONLY != 0)
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
    use windows::Win32::UI::Shell::{
        IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF, SIIGBF_INCACHEONLY, SIIGBF_THUMBNAILONLY,
    };
    use windows::core::HSTRING;

    use crate::Rgba;

    /// What the shell may do: a cloud file's thumbnail only from the cache, since the type's
    /// handler would read (download) the data.
    pub(super) fn image_flags(in_cloud: bool) -> SIIGBF {
        if in_cloud { SIIGBF_THUMBNAILONLY | SIIGBF_INCACHEONLY } else { SIIGBF_THUMBNAILONLY }
    }

    /// The shell's thumbnail (its cache, or the type's thumbnail handler unless `in_cloud`);
    /// `None` if there is none.
    pub fn thumbnail(path: &Path, px: u32, in_cloud: bool) -> Option<Rgba> {
        let side = px.clamp(1, 1024) as i32;
        // SAFETY: the bitmap is ours to delete once converted.
        unsafe {
            let factory: IShellItemImageFactory =
                SHCreateItemFromParsingName(&HSTRING::from(path.as_os_str()), None::<&IBindCtx>).ok()?;
            let bitmap = factory.GetImage(SIZE { cx: side, cy: side }, image_flags(in_cloud)).ok()?;
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

    #[cfg(windows)]
    #[test]
    fn a_cloud_file_gets_only_a_cached_thumbnail() {
        use windows::Win32::UI::Shell::{SIIGBF_INCACHEONLY, SIIGBF_THUMBNAILONLY};
        assert_eq!(win::image_flags(true), SIIGBF_THUMBNAILONLY | SIIGBF_INCACHEONLY);
        assert_eq!(win::image_flags(false), SIIGBF_THUMBNAILONLY, "a local file may use the handler");
    }

    #[test]
    fn only_in_cloud_reads_no_data() {
        let dir = std::env::temp_dir().join(format!("gezik-cloud-local-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("here.txt");
        std::fs::write(&file, "x").unwrap();
        assert!(!only_in_cloud(&file), "a plain local file");
        assert!(!only_in_cloud(&dir.join("missing")), "nothing there");
        let _ = std::fs::remove_dir_all(&dir);
    }

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
    fn the_exif_orientation_is_applied() {
        // A 4×2 JPEG whose EXIF says "turn 90° clockwise to show" (orientation 6).
        let path = temp("oriented").join("o6.jpg");
        let mut jpeg = Vec::new();
        image::RgbImage::from_pixel(4, 2, image::Rgb([200, 30, 30]))
            .write_to(&mut std::io::Cursor::new(&mut jpeg), image::ImageFormat::Jpeg)
            .unwrap();
        // APP1 Exif, big-endian TIFF, one IFD entry: 0x0112 Orientation = 6.
        let tiff: &[u8] = &[b'M', b'M', 0, 42, 0, 0, 0, 8, 0, 1, 0x01, 0x12, 0, 3, 0, 0, 0, 1, 0, 6, 0, 0, 0, 0, 0, 0];
        let mut app1 = vec![0xFF, 0xE1];
        app1.extend_from_slice(&((2 + 6 + tiff.len()) as u16).to_be_bytes());
        app1.extend_from_slice(b"Exif\0\0");
        app1.extend_from_slice(tiff);
        let mut file = jpeg[..2].to_vec();
        file.extend_from_slice(&app1);
        file.extend_from_slice(&jpeg[2..]);
        std::fs::write(&path, file).unwrap();
        let decoded = decode_image(&path, 100).unwrap();
        assert_eq!((decoded.image.width, decoded.image.height), (2, 4), "turned upright");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
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
