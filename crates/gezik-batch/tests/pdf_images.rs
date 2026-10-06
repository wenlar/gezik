//! Images to PDF: pages, JPEG passthrough, EXIF orientation by matrix, alpha masks, 16-bit
//! samples, A4 sheets, unreadable pictures and stopping. The pictures are made here; a small
//! reader checks every xref offset of the result.

use std::path::{Path, PathBuf};

use gezik_batch::pdf::images::write_pdf;
use gezik_core::batch::pdf::{Margin, PageOptions, PageSize};
use image::{ImageBuffer, Rgba, RgbaImage};

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("gezik-pdf-images-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn never() -> bool {
    false
}

// ---- Pictures ---------------------------------------------------------------------------

fn jpeg_bytes(w: u16, h: u16, color: [u8; 3], exif: Option<&[u8]>) -> Vec<u8> {
    let mut rgb = Vec::new();
    for _ in 0..u32::from(w) * u32::from(h) {
        rgb.extend_from_slice(&color);
    }
    let mut out = Vec::new();
    let mut encoder = jpeg_encoder::Encoder::new(&mut out, 90);
    if let Some(exif) = exif {
        encoder.add_exif_metadata(exif).unwrap();
    }
    encoder.encode(&rgb, w, h, jpeg_encoder::ColorType::Rgb).unwrap();
    out
}

fn jpeg(path: &Path, w: u16, h: u16, color: [u8; 3]) -> PathBuf {
    std::fs::write(path, jpeg_bytes(w, h, color, None)).unwrap();
    path.to_path_buf()
}

/// A little-endian TIFF holding only the orientation tag.
fn orientation_exif(orientation: u16) -> Vec<u8> {
    let mut t = b"II*\0".to_vec();
    t.extend_from_slice(&8u32.to_le_bytes());
    t.extend_from_slice(&1u16.to_le_bytes());
    t.extend_from_slice(&0x0112u16.to_le_bytes());
    t.extend_from_slice(&3u16.to_le_bytes());
    t.extend_from_slice(&1u32.to_le_bytes());
    t.extend_from_slice(&orientation.to_le_bytes());
    t.extend_from_slice(&[0, 0]);
    t.extend_from_slice(&0u32.to_le_bytes());
    t
}

fn jpeg_with_exif(path: &Path, orientation: u16) -> PathBuf {
    std::fs::write(path, jpeg_bytes(64, 48, [30, 120, 200], Some(&orientation_exif(orientation)))).unwrap();
    path.to_path_buf()
}

fn cmyk_jpeg(path: &Path) -> PathBuf {
    let (w, h) = (16u16, 16u16);
    let cmyk: Vec<u8> = (0..u32::from(w) * u32::from(h)).flat_map(|i| [(i % 256) as u8, 40, 80, 10]).collect();
    let mut out = Vec::new();
    jpeg_encoder::Encoder::new(&mut out, 90).encode(&cmyk, w, h, jpeg_encoder::ColorType::Cmyk).unwrap();
    std::fs::write(path, out).unwrap();
    path.to_path_buf()
}

fn png_rgba(path: &Path) -> PathBuf {
    let img = RgbaImage::from_fn(32, 24, |x, y| Rgba([x as u8 * 8, y as u8 * 10, 90, (x * 8) as u8]));
    img.save(path).unwrap();
    path.to_path_buf()
}

fn png16(path: &Path) -> PathBuf {
    let img: ImageBuffer<Rgba<u16>, Vec<u16>> =
        ImageBuffer::from_fn(20, 10, |x, y| Rgba([x as u16 * 3000, y as u16 * 6000, 1234, u16::MAX]));
    img.save(path).unwrap();
    path.to_path_buf()
}

// ---- Reading the PDF ----------------------------------------------------------------------

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    hay.get(from..)?.windows(needle.len()).position(|w| w == needle).map(|p| p + from)
}

fn number_after(text: &str, key: &str) -> usize {
    let at = text.find(key).unwrap_or_else(|| panic!("no {key} in {text}")) + key.len();
    text[at..].trim_start().split(|c: char| !c.is_ascii_digit()).next().unwrap().parse().unwrap()
}

/// One indirect object: its dictionary text and its stream bytes, if any.
struct Object {
    dict: String,
    stream: Option<Vec<u8>>,
}

struct Parsed {
    objects: Vec<Object>,
}

impl Parsed {
    /// Reads `path`, checking that every in-use xref entry points at "<id> 0 obj".
    fn read(path: &Path) -> Parsed {
        let bytes = std::fs::read(path).unwrap();
        assert!(bytes.starts_with(b"%PDF-1.7\n"));
        let tail = String::from_utf8_lossy(&bytes[bytes.len().saturating_sub(64)..]).into_owned();
        let at = tail.rfind("startxref\n").expect("startxref");
        let xref: usize = tail[at + 10..].lines().next().unwrap().trim().parse().unwrap();
        assert!(bytes[xref..].starts_with(b"xref\n0 "), "xref at {xref}");
        let head_end = find(&bytes, b"\n", xref + 5).unwrap();
        let size: usize = std::str::from_utf8(&bytes[xref + 7..head_end]).unwrap().parse().unwrap();
        let entries = head_end + 1;
        let trailer = String::from_utf8_lossy(&bytes[entries + size * 20..]).into_owned();
        assert!(trailer.starts_with("trailer\n"), "{trailer}");
        assert_eq!(number_after(&trailer, "/Size"), size);
        let mut objects = Vec::new();
        for id in 0..size {
            let line = std::str::from_utf8(&bytes[entries + id * 20..entries + id * 20 + 20]).unwrap();
            assert!(line.ends_with(" \n") || line.ends_with("\r\n"), "entry {id}: {line:?}");
            if id == 0 || line.as_bytes()[17] == b'f' {
                assert_eq!(&line[..18], "0000000000 65535 f", "entry {id}");
                continue;
            }
            assert_eq!(&line[10..18], " 00000 n", "entry {id}");
            let off: usize = line[..10].parse().unwrap();
            let header = format!("{id} 0 obj");
            assert!(bytes[off..].starts_with(header.as_bytes()), "object {id} is not at {off}");
            objects.push(Self::object(&bytes, off));
        }
        Parsed { objects }
    }

    fn object(bytes: &[u8], off: usize) -> Object {
        let end = find(bytes, b"endobj", off).unwrap();
        match find(bytes, b"stream\n", off).filter(|&s| s < end) {
            None => Object { dict: String::from_utf8_lossy(&bytes[off..end]).into_owned(), stream: None },
            Some(s) => {
                let dict = String::from_utf8_lossy(&bytes[off..s]).into_owned();
                let len = number_after(&dict, "/Length");
                let data = bytes[s + 7..s + 7 + len].to_vec();
                assert!(bytes[s + 7 + len..].starts_with(b"\nendstream"), "stream length of {dict}");
                Object { dict, stream: Some(data) }
            }
        }
    }

    fn pages(&self) -> usize {
        self.objects.iter().filter(|o| o.dict.contains("/Type /Page") && !o.dict.contains("/Type /Pages")).count()
    }

    /// Every dictionary, with the unfiltered streams (page contents) as text.
    fn text(&self) -> String {
        let mut text = String::new();
        for o in &self.objects {
            text.push_str(&o.dict);
            if let Some(s) = o.stream.as_ref().filter(|_| !o.dict.contains("/Filter")) {
                text.push_str(&String::from_utf8_lossy(s));
            }
            text.push('\n');
        }
        text
    }

    fn stream_equal_to_file(&self, path: &Path) -> bool {
        let file = std::fs::read(path).unwrap();
        self.objects.iter().any(|o| o.stream.as_deref() == Some(&file[..]))
    }
}

// ---- Tests --------------------------------------------------------------------------------

#[test]
fn three_pictures_make_three_pages_with_a_valid_xref() {
    let d = dir("three");
    let pics = [
        jpeg(&d.join("a.jpg"), 64, 48, [200, 30, 30]),
        png_rgba(&d.join("b.png")),
        jpeg_with_exif(&d.join("c.jpg"), 6),
    ];
    let out = d.join("out.pdf");
    let report = write_pdf(&pics, &out, &PageOptions::DEFAULT, &mut |_, _| {}, &never).unwrap();
    assert_eq!((report.pages, report.left_out.len()), (3, 0));
    let pdf = Parsed::read(&out); // checks every xref offset
    assert_eq!(pdf.pages(), 3);
    // The JPEG went in byte for byte (DCTDecode passthrough).
    assert!(pdf.stream_equal_to_file(&pics[0]));
    // EXIF 6: shown upright through the matrix, on a 48 × 64 pt page.
    assert!(pdf.text().contains("0 -1 1 0 0 1 cm"));
    assert!(pdf.text().contains("/MediaBox [0 0 48 64]"));
    // Alpha went to an SMask.
    assert!(pdf.text().contains("/SMask"));
}

#[test]
fn cmyk_jpegs_from_photoshop_get_the_decode_array() {
    let d = dir("cmyk");
    let cmyk = cmyk_jpeg(&d.join("cmyk.jpg"));
    assert!(
        gezik_batch::pdf::jpeg::parse(&std::fs::read(&cmyk).unwrap()).unwrap().adobe.is_some(),
        "jpeg-encoder writes APP14"
    );
    let out = d.join("o.pdf");
    write_pdf(&[cmyk], &out, &PageOptions::DEFAULT, &mut |_, _| {}, &never).unwrap();
    let text = Parsed::read(&out).text();
    assert!(text.contains("/DeviceCMYK") && text.contains("/Decode [1 0 1 0 1 0 1 0]"), "{text}");
}

#[test]
fn sixteen_bits_stay_and_opaque_alpha_needs_no_mask() {
    let d = dir("png16");
    let out = d.join("o.pdf");
    write_pdf(&[png16(&d.join("a.png"))], &out, &PageOptions::DEFAULT, &mut |_, _| {}, &never).unwrap();
    let text = Parsed::read(&out).text();
    assert!(
        text.contains("/BitsPerComponent 16") && text.contains("/Predictor 15") && !text.contains("/SMask"),
        "{text}"
    );
}

#[test]
fn a4_pages_turn_for_landscape_pictures() {
    let d = dir("a4");
    let pics = [jpeg(&d.join("wide.jpg"), 400, 300, [0, 0, 255]), jpeg(&d.join("tall.jpg"), 300, 400, [0, 255, 0])];
    let out = d.join("o.pdf");
    let a4 = PageOptions { size: PageSize::A4, margin: Margin::Small };
    write_pdf(&pics, &out, &a4, &mut |_, _| {}, &never).unwrap();
    let text = Parsed::read(&out).text();
    assert!(
        text.contains("/MediaBox [0 0 841.89 595.276]") && text.contains("/MediaBox [0 0 595.276 841.89]"),
        "{text}"
    );
}

#[test]
fn unreadable_pictures_are_left_out_and_numbers_stay_valid() {
    let d = dir("bad");
    let bad = d.join("bad.png");
    std::fs::write(&bad, b"not a png").unwrap();
    let good = jpeg(&d.join("g.jpg"), 8, 8, [1, 2, 3]);
    let out = d.join("o.pdf");
    let report = write_pdf(&[bad.clone(), good], &out, &PageOptions::DEFAULT, &mut |_, _| {}, &never).unwrap();
    assert_eq!(report.pages, 1);
    assert_eq!(report.left_out[0].0, bad);
    Parsed::read(&out); // xref still checks out
    // Nothing readable: an error, and no file.
    let none = d.join("none.pdf");
    assert!(write_pdf(&[bad], &none, &PageOptions::DEFAULT, &mut |_, _| {}, &never).is_err());
    assert!(!none.exists());
}

#[test]
fn stop_removes_the_file() {
    let d = dir("stop");
    let pics: Vec<PathBuf> = (0..3).map(|i| jpeg(&d.join(format!("{i}.jpg")), 8, 8, [0, 0, 0])).collect();
    let out = d.join("o.pdf");
    let seen = std::cell::Cell::new(0);
    let err = write_pdf(&pics, &out, &PageOptions::DEFAULT, &mut |_, _| seen.set(seen.get() + 1), &|| seen.get() >= 1)
        .unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::Interrupted);
    assert!(!out.exists());
}

#[test]
fn a_panic_removes_the_file_and_an_existing_one_is_never_written_over() {
    let d = dir("panic");
    let pics: Vec<PathBuf> = (0..2).map(|i| jpeg(&d.join(format!("{i}.jpg")), 8, 8, [0, 0, 0])).collect();
    let out = d.join("o.pdf");
    let seen = std::cell::Cell::new(0);
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        write_pdf(&pics, &out, &PageOptions::DEFAULT, &mut |_, _| seen.set(seen.get() + 1), &|| {
            assert!(seen.get() < 1, "a decoder panics");
            false
        })
    }));
    assert!(panicked.is_err());
    assert!(!out.exists());
    // A file already there is left as it is.
    std::fs::write(&out, b"mine").unwrap();
    let err = write_pdf(&pics, &out, &PageOptions::DEFAULT, &mut |_, _| {}, &never).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(std::fs::read(&out).unwrap(), b"mine");
}

#[test]
fn a_jpeg_too_large_to_hold_is_not_passed_through() {
    let d = dir("huge");
    // A baseline header stating 60000 x 60000 pixels, and no picture.
    let mut huge = vec![0xFF, 0xD8, 0xFF, 0xC0, 0x00, 0x11, 8];
    huge.extend_from_slice(&60_000u16.to_be_bytes());
    huge.extend_from_slice(&60_000u16.to_be_bytes());
    huge.extend_from_slice(&[3, 1, 0x11, 0, 2, 0x11, 0, 3, 0x11, 0]);
    huge.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x08, 1, 1, 0, 0, 0x3F, 0, 0xFF, 0xD9]);
    let big = d.join("big.jpg");
    std::fs::write(&big, huge).unwrap();
    let good = jpeg(&d.join("g.jpg"), 8, 8, [1, 2, 3]);
    let out = d.join("o.pdf");
    let report = write_pdf(&[big.clone(), good], &out, &PageOptions::DEFAULT, &mut |_, _| {}, &never).unwrap();
    assert_eq!(report.pages, 1);
    assert_eq!(report.left_out.len(), 1);
    assert_eq!(report.left_out[0].0, big);
    Parsed::read(&out);
}
