//! When a photo was taken, from its EXIF data (JPEG, TIFF, HEIC, WebP, PNG). A JPEG's EXIF
//! is at its start, so only the start is read; HEIF is found by its boxes; the other formats
//! are read in order up to a bound, so a large file without EXIF is never read whole.

use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use gezik_core::batch::date::DateParts;

/// How much of a JPEG is read: its EXIF is in its first segments.
const JPEG_MAX: u64 = 256 * 1024;

/// How much of a TIFF, PNG or WebP is read at most.
const OTHER_MAX: u64 = 16 * 1024 * 1024;

/// Formats that can carry EXIF, by extension.
pub fn may_have_exif(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [".jpg", ".jpeg", ".tif", ".tiff", ".heic", ".heif", ".webp", ".png"].iter().any(|ext| lower.ends_with(ext))
}

/// The date the photo was taken (`DateTimeOriginal`, else `DateTime`), as the camera wrote it
/// (local time). `None` if the file has none or cannot be read.
pub fn taken(path: &Path) -> Option<DateParts> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut head = Vec::with_capacity(64);
    (&mut file).take(64).read_to_end(&mut head).ok()?;
    file.seek(SeekFrom::Start(0)).ok()?;
    let reader = exif::Reader::new();
    let exif = if is_heif(&head) {
        // Read box by box, skipping the image data.
        reader.read_from_container(&mut BufReader::new(file))
    } else {
        let max = if head.starts_with(&[0xFF, 0xD8]) { JPEG_MAX } else { OTHER_MAX };
        reader.read_from_container(&mut BufReader::new(file.take(max)))
    }
    .ok()?;
    [exif::Tag::DateTimeOriginal, exif::Tag::DateTime].into_iter().find_map(|tag| {
        let field = exif.get_field(tag, exif::In::PRIMARY)?;
        match &field.value {
            exif::Value::Ascii(parts) => parse(std::str::from_utf8(parts.first()?).ok()?),
            _ => None,
        }
    })
}

/// Whether `head` (a file's start) is a HEIF file (HEIC, AVIF, ...): an `ftyp` box with
/// one of their brands.
fn is_heif(head: &[u8]) -> bool {
    const BRANDS: [&[u8; 4]; 10] =
        [b"heic", b"heix", b"heim", b"heis", b"hevc", b"hevx", b"mif1", b"msf1", b"avif", b"avis"];
    if head.len() < 12 || &head[4..8] != b"ftyp" {
        return false;
    }
    let size = u32::from_be_bytes([head[0], head[1], head[2], head[3]]) as usize;
    // The major brand, then (after the minor version) the compatible ones.
    let brands = std::iter::once(&head[8..12]).chain(head.get(16..size.min(head.len())).unwrap_or(&[]).chunks_exact(4));
    brands.into_iter().any(|brand| BRANDS.iter().any(|known| brand == known.as_slice()))
}

/// `2024:07:01 09:30:00`.
fn parse(text: &str) -> Option<DateParts> {
    let text = text.trim();
    let (date, time) = text.split_once(' ')?;
    let mut d = date.split(':').map(|p| p.parse::<i32>().ok());
    let mut t = time.split(':').map(|p| p.parse::<u8>().ok());
    let parts = DateParts {
        year: d.next()??,
        month: u8::try_from(d.next()??).ok()?,
        day: u8::try_from(d.next()??).ok()?,
        hour: t.next()??,
        minute: t.next()??,
        second: t.next().flatten().unwrap_or(0),
    };
    (parts.year > 0 && (1..=12).contains(&parts.month) && (1..=31).contains(&parts.day)).then_some(parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_exif_dates() {
        let parts = parse("2024:07:01 09:30:05").unwrap();
        assert_eq!(
            (parts.year, parts.month, parts.day, parts.hour, parts.minute, parts.second),
            (2024, 7, 1, 9, 30, 5)
        );
        assert!(parse("0000:00:00 00:00:00").is_none(), "cameras write zeros when unset");
        assert!(parse("garbage").is_none());
    }

    #[test]
    fn reads_a_real_photo() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/exif.jpg");
        let parts = taken(&path).expect("tests/data/exif.jpg has a DateTimeOriginal");
        assert_eq!((parts.year, parts.month, parts.day), (2024, 7, 1));
    }

    #[test]
    fn reads_exif_kept_after_the_image_data() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/exif.png");
        let parts = taken(&path).expect("tests/data/exif.png has an eXIf chunk");
        assert_eq!((parts.year, parts.month, parts.day), (2024, 7, 1));
    }

    #[test]
    fn heif_is_known_by_its_brand() {
        assert!(is_heif(b"\0\0\0\x18ftypheic\0\0\0\0mif1heic"));
        assert!(is_heif(b"\0\0\0\x14ftypXXXX\0\0\0\0avif"));
        assert!(!is_heif(b"\0\0\0\x14ftypisom\0\0\0\0mp41"));
        assert!(!is_heif(PNG_SIG));
    }

    const PNG_SIG: &[u8] = b"\x89PNG\r\n\x1a\n";

    /// A PNG with a chunk of `size` bytes before its EXIF.
    fn png_with_a_big_chunk(path: &Path, size: usize) {
        let png = std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/exif.png")).unwrap();
        let mut data = png[..8].to_vec();
        data.extend_from_slice(&(size as u32).to_be_bytes());
        data.extend_from_slice(b"zzZz");
        data.resize(data.len() + size + 4, 0);
        data.extend_from_slice(&png[8..]);
        std::fs::write(path, data).unwrap();
    }

    #[test]
    fn a_large_file_is_read_only_up_to_the_bound() {
        let dir = std::env::temp_dir().join(format!("gezik-exif-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // No EXIF in 20 MB: given up at the bound.
        let mut zeros = PNG_SIG.to_vec();
        zeros.resize(20 * 1024 * 1024, 0);
        std::fs::write(dir.join("zeros.png"), zeros).unwrap();
        let start = std::time::Instant::now();
        assert!(taken(&dir.join("zeros.png")).is_none());
        assert!(start.elapsed() < std::time::Duration::from_secs(5));
        // EXIF past the bound is not reached; before it, it is.
        png_with_a_big_chunk(&dir.join("far.png"), OTHER_MAX as usize);
        assert!(taken(&dir.join("far.png")).is_none());
        png_with_a_big_chunk(&dir.join("near.png"), 1024 * 1024);
        assert!(taken(&dir.join("near.png")).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn files_without_exif() {
        assert!(taken(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml").as_path()).is_none());
        assert!(may_have_exif("IMG.JPG") && !may_have_exif("a.txt"));
    }
}
