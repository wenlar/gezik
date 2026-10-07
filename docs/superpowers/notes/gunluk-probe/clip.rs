//! Q3: clipboard image and text. Windows runs; macOS and Linux (see clip_x11 / clip_wayland)
//! are checked or run elsewhere.
//!
//! Windows: `clip` runs every scenario (and puts the old clipboard text back);
//! `clip read` only reads what is there (for images put by other programs).

use std::time::Instant;

/// RGBA pixels to PNG, timing each compression level.
pub fn encode_png(rgba: &image::RgbaImage, level: image::codecs::png::CompressionType) -> Vec<u8> {
    use image::ImageEncoder;
    use image::codecs::png::{FilterType, PngEncoder};
    let mut out = Vec::new();
    PngEncoder::new_with_quality(&mut out, level, FilterType::Adaptive)
        .write_image(rgba.as_raw(), rgba.width(), rgba.height(), image::ExtendedColorType::Rgba8)
        .unwrap();
    out
}

/// A screenshot's alpha is often all 0 (a 32-bit BI_RGB DIB seen through CF_DIBV5's alpha
/// mask): such an image is opaque, not invisible.
pub fn fix_alpha(rgba: &mut image::RgbaImage) -> bool {
    let first = rgba.as_raw()[3];
    if rgba.pixels().all(|p| p.0[3] == first) && first != 255 {
        rgba.pixels_mut().for_each(|p| p.0[3] = 255);
        return true;
    }
    false
}

pub fn measure(label: &str, rgba: &image::RgbaImage) {
    use image::codecs::png::CompressionType;
    for (name, level) in [
        ("Fast", CompressionType::Fast),
        ("Default", CompressionType::Default),
        ("Best", CompressionType::Best),
    ] {
        let t = Instant::now();
        let png = encode_png(rgba, level);
        println!("  {label} PNG {name}: {:?}, {:.2} MB", t.elapsed(), png.len() as f64 / 1e6);
    }
}

/// A 3840x2160 picture shaped like a screenshot: flat panels, rows of "text", a photo-like
/// gradient with noise in one quarter. BGRA, top-down.
pub fn fake_screenshot() -> Vec<u8> {
    let (w, h) = (3840usize, 2160usize);
    let mut px = vec![0u8; w * h * 4];
    let mut seed = 0x1234_5678u32;
    let mut rnd = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed
    };
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 4;
            let (b, g, r) = if x < 600 {
                (40, 36, 32) // sidebar
            } else if y < 80 {
                (60, 60, 60) // toolbar
            } else if x > 2400 && y > 1000 {
                let n = (rnd() & 31) as u8;
                ((x / 8) as u8 ^ n, (y / 6) as u8 ^ n, ((x + y) / 10) as u8 ^ n) // photo
            } else if (y / 28) % 2 == 0 && (x / 9 + y / 3) % 5 != 0 && y % 28 > 6 && y % 28 < 22 {
                (220, 220, 220) // text strokes
            } else {
                (30, 30, 30)
            };
            px[i..i + 4].copy_from_slice(&[b, g, r, 0]);
        }
    }
    px
}

fn main() {
    #[cfg(windows)]
    win::main();
    #[cfg(target_os = "macos")]
    mac::main();
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let t = Instant::now();
        let bgra = fake_screenshot();
        let mut rgba = image::RgbaImage::from_raw(3840, 2160, bgra).unwrap();
        rgba.pixels_mut().for_each(|p| {
            p.0.swap(0, 2);
            p.0[3] = 255
        });
        println!("made in {:?}", t.elapsed());
        let png = encode_png(&rgba, image::codecs::png::CompressionType::Fast);
        std::fs::write(std::env::args().nth(1).unwrap_or("/tmp/fake4k.png".into()), png).unwrap();
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use objc2::rc::Retained;
    use objc2_app_kit::{
        NSBitmapImageFileType, NSBitmapImageRep, NSPasteboard, NSPasteboardTypePNG, NSPasteboardTypeString,
        NSPasteboardTypeTIFF,
    };
    use objc2_foundation::{NSData, NSDictionary};

    /// PNG bytes of the pasteboard's picture: public.png as is, else public.tiff converted by
    /// AppKit (the image crate here has no TIFF decoder).
    pub fn read_png() -> Option<Vec<u8>> {
        let pasteboard = NSPasteboard::generalPasteboard();
        unsafe {
            if let Some(png) = pasteboard.dataForType(NSPasteboardTypePNG) {
                return Some(png.to_vec());
            }
            let tiff: Retained<NSData> = pasteboard.dataForType(NSPasteboardTypeTIFF)?;
            let rep = NSBitmapImageRep::imageRepWithData(&tiff)?;
            let png = rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())?;
            Some(png.to_vec())
        }
    }

    pub fn read_text() -> Option<String> {
        let pasteboard = NSPasteboard::generalPasteboard();
        unsafe { pasteboard.stringForType(NSPasteboardTypeString) }.map(|s| s.to_string())
    }

    /// Which picture types are on offer, without reading them.
    pub fn has_image() -> bool {
        let pasteboard = NSPasteboard::generalPasteboard();
        let Some(types) = pasteboard.types() else { return false };
        types.iter().any(|t| unsafe { &*t == NSPasteboardTypePNG || &*t == NSPasteboardTypeTIFF })
    }

    pub fn main() {
        let t = std::time::Instant::now();
        println!("has image: {}", has_image());
        if let Some(png) = read_png() {
            println!("png {} bytes in {:?}", png.len(), t.elapsed());
            let _ = std::fs::write("/tmp/probe7-clip.png", png);
        }
        println!("text: {:?}", read_text());
    }
}

#[cfg(windows)]
mod win {
    use std::time::Instant;

    use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData, GetClipboardFormatNameW,
        IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock};
    use windows::Win32::System::Ole::{CF_DIB, CF_DIBV5, CF_TEXT, CF_UNICODETEXT};
    use windows::core::w;

    use super::{encode_png, fake_screenshot, fix_alpha, measure};

    struct Open;
    impl Open {
        fn new() -> Open {
            for _ in 0..50 {
                if unsafe { OpenClipboard(None) }.is_ok() {
                    return Open;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            panic!("clipboard busy");
        }
    }
    impl Drop for Open {
        fn drop(&mut self) {
            let _ = unsafe { CloseClipboard() };
        }
    }

    fn png_format() -> u32 {
        unsafe { RegisterClipboardFormatW(w!("PNG")) }
    }

    fn get(format: u32) -> Option<Vec<u8>> {
        unsafe {
            let handle = GetClipboardData(format).ok()?;
            let memory = HGLOBAL(handle.0);
            let size = GlobalSize(memory);
            let source = GlobalLock(memory) as *const u8;
            if source.is_null() {
                return None;
            }
            let bytes = std::slice::from_raw_parts(source, size).to_vec();
            let _ = GlobalUnlock(memory);
            Some(bytes)
        }
    }

    fn put(format: u32, bytes: &[u8]) {
        unsafe {
            let memory = GlobalAlloc(GMEM_MOVEABLE, bytes.len()).unwrap();
            let target = GlobalLock(memory) as *mut u8;
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), target, bytes.len());
            let _ = GlobalUnlock(memory);
            if SetClipboardData(format, Some(HANDLE(memory.0))).is_err() {
                let _ = GlobalFree(Some(memory));
                panic!("SetClipboardData");
            }
        }
    }

    fn formats() -> Vec<String> {
        let mut out = Vec::new();
        let mut f = 0;
        loop {
            f = unsafe { EnumClipboardFormats(f) };
            if f == 0 {
                break;
            }
            let mut name = [0u16; 128];
            let n = unsafe { GetClipboardFormatNameW(f, &mut name) };
            out.push(if n > 0 { format!("{f}:{}", String::from_utf16_lossy(&name[..n as usize])) } else { f.to_string() });
        }
        out
    }

    /// The text on the clipboard (CF_UNICODETEXT; Windows makes it from CF_TEXT if needed).
    pub fn read_text() -> Option<String> {
        let _open = Open::new();
        let bytes = get(u32::from(CF_UNICODETEXT.0))?;
        let units: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
        Some(String::from_utf16_lossy(&units[..end]))
    }

    pub enum Picture {
        Png(Vec<u8>),
        Pixels(image::RgbaImage, &'static str),
    }

    /// The clipboard's picture: "PNG" as is, else CF_DIBV5, else CF_DIB, decoded. The
    /// clipboard is open only while the bytes are copied.
    pub fn read_image() -> Option<Picture> {
        let (dibv5, dib) = {
            let _open = Open::new();
            if unsafe { IsClipboardFormatAvailable(png_format()) }.is_ok()
                && let Some(png) = get(png_format())
            {
                return Some(Picture::Png(png));
            }
            let dibv5 = get(u32::from(CF_DIBV5.0));
            let dib = if dibv5.is_none() { get(u32::from(CF_DIB.0)) } else { None };
            (dibv5, dib)
        };
        // Decoded with the clipboard closed again.
        if let Some(rgba) = dibv5.as_deref().and_then(decode_dib) {
            return Some(Picture::Pixels(rgba, "CF_DIBV5"));
        }
        decode_dib(&dib?).map(|rgba| Picture::Pixels(rgba, "CF_DIB"))
    }

    /// CF_DIB / CF_DIBV5 bytes (no BITMAPFILEHEADER) to RGBA. A BITMAPFILEHEADER is put in
    /// front so the pixel offset is ours: image 0.25's `new_without_file_header` assumes three
    /// masks after a V4/V5 header with BI_BITFIELDS, which not every program writes.
    pub fn decode_dib(bytes: &[u8]) -> Option<image::RgbaImage> {
        let file = with_file_header(bytes)?;
        let decoder = image::codecs::bmp::BmpDecoder::new(std::io::Cursor::new(file)).ok()?;
        let mut rgba = image::DynamicImage::from_decoder(decoder).ok()?.into_rgba8();
        fix_alpha(&mut rgba);
        Some(rgba)
    }

    /// "BM" header whose bfOffBits points at the pixels: for uncompressed DIBs the pixels are
    /// the last stride * height bytes; otherwise header + masks + palette.
    pub fn with_file_header(dib: &[u8]) -> Option<Vec<u8>> {
        let u32_at = |i: usize| Some(u32::from_le_bytes(dib.get(i..i + 4)?.try_into().ok()?));
        let header = u32_at(0)? as usize;
        let width = u32_at(4)? as i32;
        let height = (u32_at(8)? as i32).unsigned_abs() as usize;
        let bits = u16::from_le_bytes(dib.get(14..16)?.try_into().ok()?) as usize;
        let compression = u32_at(16)?;
        let colors = u32_at(32)? as usize;
        let stride = (width.unsigned_abs() as usize * bits).div_ceil(32) * 4;
        let pixels_at = if compression == 0 || compression == 3 {
            dib.len().checked_sub(stride * height)?
        } else {
            let masks = if compression == 3 && header == 40 { 12 } else { 0 };
            let palette = if bits <= 8 { (if colors == 0 { 1 << bits } else { colors }) * 4 } else { 0 };
            header + masks + palette
        };
        if pixels_at < header {
            return None;
        }
        let mut file = Vec::with_capacity(14 + dib.len());
        file.extend_from_slice(b"BM");
        file.extend_from_slice(&((14 + dib.len()) as u32).to_le_bytes());
        file.extend_from_slice(&0u32.to_le_bytes());
        file.extend_from_slice(&((14 + pixels_at) as u32).to_le_bytes());
        file.extend_from_slice(dib);
        Some(file)
    }

    /// BITMAPINFOHEADER (40 bytes), 32 bpp BI_RGB, bottom-up: what PrintScreen leaves.
    fn dib_from_bgra(w: u32, h: u32, bgra: &[u8]) -> Vec<u8> {
        let mut b = Vec::with_capacity(40 + bgra.len());
        b.extend_from_slice(&40u32.to_le_bytes());
        b.extend_from_slice(&(w as i32).to_le_bytes());
        b.extend_from_slice(&(h as i32).to_le_bytes()); // positive: bottom-up
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&32u16.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
        b.extend_from_slice(&(bgra.len() as u32).to_le_bytes());
        b.extend_from_slice(&[0u8; 16]);
        let stride = w as usize * 4;
        for row in bgra.chunks_exact(stride).rev() {
            b.extend_from_slice(row);
        }
        b
    }

    pub fn main() {
        let read_only = std::env::args().nth(1).as_deref() == Some("read");
        if read_only {
            let o = Open::new();
            println!("formats: {:?}", formats());
            for (name, f) in [("CF_DIB", CF_DIB.0), ("CF_DIBV5", CF_DIBV5.0)] {
                if let Some(b) = get(u32::from(f)) {
                    let h: Vec<u32> = b[..40].chunks(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect();
                    let bits = h[3] >> 16;
                    let first_alpha = with_file_header(&b)
                        .and_then(|file| {
                            image::DynamicImage::from_decoder(image::codecs::bmp::BmpDecoder::new(std::io::Cursor::new(file)).ok()?).ok()
                        })
                        .map(|d| d.into_rgba8().as_raw()[3]);
                    println!(
                        "  {name}: header {} bytes, {}x{}, {bits} bpp, compression {}, {} bytes, alpha of first pixel before fix {first_alpha:?}",
                        h[0], h[1] as i32, h[2] as i32, h[4], b.len()
                    );
                }
            }
            drop(o);
            report_read();
            println!("text: {:?}", read_text().map(|t| t.chars().take(80).collect::<String>()));
            return;
        }
        let backup = read_text();

        // 1. Text with Turkish letters, CJK and an emoji; then ANSI only (Windows converts).
        {
            let _o = Open::new();
            unsafe { EmptyClipboard() }.unwrap();
            let text = "Merhaba ğüşİı 日本 🙂\r\nikinci satır";
            let mut wide: Vec<u8> = text.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect();
            wide.shrink_to_fit();
            put(u32::from(CF_UNICODETEXT.0), &wide);
        }
        println!("text round trip: {:?}", read_text());
        {
            let _o = Open::new();
            unsafe { EmptyClipboard() }.unwrap();
            put(u32::from(CF_TEXT.0), b"plain ANSI\0");
        }
        println!("CF_TEXT only -> CF_UNICODETEXT: {:?}", read_text());

        // 2. A 4K "PrintScreen": CF_DIB only, 32 bpp BI_RGB, alpha bytes 0.
        let bgra = fake_screenshot();
        let dib = dib_from_bgra(3840, 2160, &bgra);
        {
            let _o = Open::new();
            unsafe { EmptyClipboard() }.unwrap();
            let t = Instant::now();
            put(u32::from(CF_DIB.0), &dib);
            println!("\nput CF_DIB {:.1} MB in {:?}", dib.len() as f64 / 1e6, t.elapsed());
        }
        {
            let _o = Open::new();
            println!("formats now (synthesized ones too): {:?}", formats());
        }
        // Raw CF_DIBV5 synthesized from it: decode without the alpha fix to show the trap.
        let v5 = {
            let _o = Open::new();
            let t = Instant::now();
            let v5 = get(u32::from(CF_DIBV5.0)).unwrap();
            println!("CF_DIBV5 (synthesized) read {:.1} MB in {:?}", v5.len() as f64 / 1e6, t.elapsed());
            v5
        };
        let header: Vec<u32> = v5[..124].chunks(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect();
        println!(
            "  V5 header: size {} compression {} masks r {:#x} g {:#x} b {:#x} a {:#x}",
            header[0], header[4], header[10], header[11], header[12], header[13]
        );
        let raw = image::DynamicImage::from_decoder(
            image::codecs::bmp::BmpDecoder::new_without_file_header(std::io::Cursor::new(&v5)).unwrap(),
        )
        .unwrap()
        .into_rgba8();
        println!("  decoded as is: alpha of first pixel = {} (0 = invisible PNG without the fix)", raw.as_raw()[3]);
        report_read();

        // 2b. CF_DIBV5 with BI_BITFIELDS and an alpha mask, alpha bytes 0 (some programs);
        // 2c. CF_DIB 40-byte header + three masks (BI_BITFIELDS).
        for (label, v5, extra) in [
            ("V5 BI_BITFIELDS alpha 0", true, false),
            ("V5 BI_BITFIELDS alpha 0 + 3 masks after header", true, true),
            ("DIB 40 + masks", false, false),
        ] {
            let mut b = Vec::new();
            let size: u32 = if v5 { 124 } else { 40 };
            b.extend_from_slice(&size.to_le_bytes());
            b.extend_from_slice(&3840i32.to_le_bytes());
            b.extend_from_slice(&(-2160i32).to_le_bytes()); // top-down
            b.extend_from_slice(&1u16.to_le_bytes());
            b.extend_from_slice(&32u16.to_le_bytes());
            b.extend_from_slice(&3u32.to_le_bytes()); // BI_BITFIELDS
            b.extend_from_slice(&(bgra.len() as u32).to_le_bytes());
            b.extend_from_slice(&[0u8; 16]);
            for m in [0x00ff_0000u32, 0x0000_ff00, 0x0000_00ff] {
                b.extend_from_slice(&m.to_le_bytes());
            }
            if v5 {
                b.extend_from_slice(&0xff00_0000u32.to_le_bytes());
                b.extend_from_slice(&0x7352_4742u32.to_le_bytes()); // LCS_sRGB
                b.resize(124, 0);
                if extra {
                    for m in [0x00ff_0000u32, 0x0000_ff00, 0x0000_00ff] {
                        b.extend_from_slice(&m.to_le_bytes());
                    }
                }
            }
            b.extend_from_slice(&bgra);
            let direct = image::codecs::bmp::BmpDecoder::new_without_file_header(std::io::Cursor::new(&b))
                .and_then(image::DynamicImage::from_decoder)
                .map(|_| "ok");
            println!("{label}: new_without_file_header -> {direct:?}");
            let t = Instant::now();
            let raw = with_file_header(&b)
                .ok_or("no offset".to_string())
                .and_then(|f| {
                    image::codecs::bmp::BmpDecoder::new(std::io::Cursor::new(f))
                        .and_then(image::DynamicImage::from_decoder)
                        .map_err(|e| e.to_string())
                })
                .map(|d| d.into_rgba8());
            match raw {
                Ok(mut rgba) => {
                    let before = rgba.as_raw()[3];
                    let fixed = fix_alpha(&mut rgba);
                    println!(
                        "{label}: decoded in {:?}, pixel (r,g,b) {:?}, alpha before fix {before}, fixed {fixed}",
                        t.elapsed(),
                        &rgba.as_raw()[0..3]
                    );
                }
                Err(e) => println!("{label}: {e}"),
            }
        }

        // 3. Like a browser or the Snipping Tool: "PNG" next to CF_DIBV5.
        let mut rgba = image::RgbaImage::from_raw(3840, 2160, bgra.clone()).unwrap();
        rgba.pixels_mut().for_each(|p| {
            p.0.swap(0, 2);
            p.0[3] = 255;
        });
        let png = encode_png(&rgba, image::codecs::png::CompressionType::Fast);
        {
            let _o = Open::new();
            unsafe { EmptyClipboard() }.unwrap();
            put(png_format(), &png);
            put(u32::from(CF_DIB.0), &dib);
        }
        println!("\nwith \"PNG\" on offer:");
        report_read();

        // 4. Encoding cost, also for a real desktop capture (kept in memory only).
        println!("\nfake 4K screenshot:");
        measure("fake", &rgba);
        if let Some(desk) = capture_desktop() {
            println!("real desktop capture {}x{}:", desk.width(), desk.height());
            measure("desktop", &desk);
            // Tiled up to 3840x2160 when the screen is smaller.
            if desk.width() < 3840 {
                let mut big = image::RgbaImage::new(3840, 2160);
                for y in (0..2160).step_by(desk.height() as usize) {
                    for x in (0..3840).step_by(desk.width() as usize) {
                        image::imageops::replace(&mut big, &desk, x as i64, y as i64);
                    }
                }
                measure("desktop tiled to 4K", &big);
            }
        }

        // Put the old text back.
        let _o = Open::new();
        unsafe { EmptyClipboard() }.unwrap();
        if let Some(text) = backup {
            let wide: Vec<u8> = text.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect();
            put(u32::from(CF_UNICODETEXT.0), &wide);
            println!("\nclipboard text restored");
        }
    }

    fn report_read() {
        let t = Instant::now();
        match read_image() {
            Some(Picture::Png(png)) => {
                let read = t.elapsed();
                let dims = image::ImageReader::new(std::io::Cursor::new(&png)).with_guessed_format().unwrap().into_dimensions();
                println!("  read \"PNG\" {:.2} MB in {read:?}, {dims:?} (written as is: no encode)", png.len() as f64 / 1e6);
            }
            Some(Picture::Pixels(rgba, from)) => {
                let read = t.elapsed();
                println!(
                    "  read+decode {from} {}x{} in {read:?}; alpha after fix {}",
                    rgba.width(),
                    rgba.height(),
                    rgba.as_raw()[3]
                );
                let t = Instant::now();
                let png = encode_png(&rgba, image::codecs::png::CompressionType::Fast);
                println!("  PNG Fast {:.2} MB in {:?}", png.len() as f64 / 1e6, t.elapsed());
            }
            None => println!("  no picture"),
        }
    }

    /// The desktop through GDI (BitBlt), as RGBA.
    fn capture_desktop() -> Option<image::RgbaImage> {
        use windows::Win32::Graphics::Gdi::*;
        use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
        unsafe {
            let (w, h) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
            let screen = GetDC(None);
            let mem = CreateCompatibleDC(Some(screen));
            let bmp = CreateCompatibleBitmap(screen, w, h);
            let old = SelectObject(mem, bmp.into());
            BitBlt(mem, 0, 0, w, h, Some(screen), 0, 0, SRCCOPY).ok()?;
            let mut info = BITMAPINFO::default();
            info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
            info.bmiHeader.biWidth = w;
            info.bmiHeader.biHeight = -h;
            info.bmiHeader.biPlanes = 1;
            info.bmiHeader.biBitCount = 32;
            let mut px = vec![0u8; (w * h * 4) as usize];
            GetDIBits(mem, bmp, 0, h as u32, Some(px.as_mut_ptr() as _), &mut info, DIB_RGB_COLORS);
            SelectObject(mem, old);
            let _ = DeleteObject(bmp.into());
            let _ = DeleteDC(mem);
            ReleaseDC(None, screen);
            px.chunks_exact_mut(4).for_each(|p| {
                p.swap(0, 2);
                p[3] = 255;
            });
            image::RgbaImage::from_raw(w as u32, h as u32, px)
        }
    }
}
