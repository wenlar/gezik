//! A photo's EXIF data (JPEG, TIFF, HEIC, WebP, PNG, raw formats): when it was taken, the
//! camera and its settings, the place. A JPEG's EXIF is at its start, so only the start is read;
//! HEIF is found by its boxes; the other formats are read in order up to a bound, so a large file
//! without EXIF is never read whole.

use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use gezik_core::batch::date::DateParts;
use gezik_core::fields::{self as f, Value};

use super::Out;

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
    let file = std::fs::File::open(path).ok()?;
    date_of(&parse(&mut BufReader::new(file))?)
}

fn parse<R: BufRead + Seek>(r: &mut R) -> Option<exif::Exif> {
    let mut head = Vec::with_capacity(64);
    r.by_ref().take(64).read_to_end(&mut head).ok()?;
    r.seek(SeekFrom::Start(0)).ok()?;
    let reader = exif::Reader::new();
    if is_heif(&head) {
        // Read box by box, skipping the image data; its `meta` is read whole, so it must be small.
        let len = r.seek(SeekFrom::End(0)).ok()?;
        if !super::mp4::heif_meta_within(r, len, super::FRAME_MAX) {
            return None;
        }
        r.seek(SeekFrom::Start(0)).ok()?;
        reader.read_from_container(r)
    } else {
        let max = if head.starts_with(&[0xFF, 0xD8]) { JPEG_MAX } else { OTHER_MAX };
        reader.read_from_container(&mut BufReader::new(r.by_ref().take(max)))
    }
    .ok()
}

fn date_of(exif: &exif::Exif) -> Option<DateParts> {
    [exif::Tag::DateTimeOriginal, exif::Tag::DateTime].into_iter().find_map(|tag| parse_date(&ascii(exif, tag)?))
}

fn ascii(exif: &exif::Exif, tag: exif::Tag) -> Option<String> {
    match &exif.get_field(tag, exif::In::PRIMARY)?.value {
        exif::Value::Ascii(parts) => Some(String::from_utf8_lossy(parts.first()?).into_owned()),
        _ => None,
    }
}

/// The `i`th number of a tag, whatever its type.
fn number(exif: &exif::Exif, tag: exif::Tag, i: usize) -> Option<f64> {
    match &exif.get_field(tag, exif::In::PRIMARY)?.value {
        exif::Value::Rational(v) => v.get(i).filter(|r| r.denom != 0).map(exif::Rational::to_f64),
        exif::Value::SRational(v) => v.get(i).filter(|r| r.denom != 0).map(|r| f64::from(r.num) / f64::from(r.denom)),
        value => value.get_uint(i).map(f64::from),
    }
}

/// The photo fields (spec 13 §7.2): date, camera (make and model, the make once), lens,
/// exposure, aperture, ISO, focal length, place.
pub(super) fn read<R: BufRead + Seek>(r: &mut R, out: &mut Out) {
    let Some(exif) = parse(r) else { return };
    if let Some(date) = date_of(&exif) {
        out.put(f::TAKEN, Value::Date(date));
    }
    let make = ascii(&exif, exif::Tag::Make).unwrap_or_default();
    let model = ascii(&exif, exif::Tag::Model).unwrap_or_default();
    out.text(f::CAMERA, &camera(&make, &model));
    out.text(f::LENS, &ascii(&exif, exif::Tag::LensModel).unwrap_or_default());
    out.real(f::EXPOSURE, number(&exif, exif::Tag::ExposureTime, 0).unwrap_or(0.0));
    out.real(f::APERTURE, number(&exif, exif::Tag::FNumber, 0).unwrap_or(0.0));
    out.int(f::ISO, number(&exif, exif::Tag::PhotographicSensitivity, 0).unwrap_or(0.0) as u64);
    out.real(f::FOCAL_LENGTH, number(&exif, exif::Tag::FocalLength, 0).unwrap_or(0.0));
    let degrees = |tag, sign: exif::Tag, negative: &str| {
        let d = number(&exif, tag, 0)?
            + number(&exif, tag, 1).unwrap_or(0.0) / 60.0
            + number(&exif, tag, 2).unwrap_or(0.0) / 3600.0;
        let flip = ascii(&exif, sign).is_some_and(|s| s.trim().eq_ignore_ascii_case(negative));
        Some(if flip { -d } else { d })
    };
    if let (Some(lat), Some(lon)) = (
        degrees(exif::Tag::GPSLatitude, exif::Tag::GPSLatitudeRef, "S"),
        degrees(exif::Tag::GPSLongitude, exif::Tag::GPSLongitudeRef, "W"),
    ) && lat.abs() <= 90.0
        && lon.abs() <= 180.0
        // Cameras without a fix write zeros.
        && (lat, lon) != (0.0, 0.0)
    {
        out.put(f::GPS, Value::Geo(lat, lon));
    }
}

/// Whether `head` (a file's start) is a HEIF file (HEIC, AVIF, ...): an `ftyp` box with
/// one of their brands.
pub(super) fn is_heif(head: &[u8]) -> bool {
    const BRANDS: [&[u8; 4]; 10] =
        [b"heic", b"heix", b"heim", b"heis", b"hevc", b"hevx", b"mif1", b"msf1", b"avif", b"avis"];
    if head.get(4..8) != Some(b"ftyp") {
        return false;
    }
    let size = super::be32(head, 0).map_or(0, |s| s as usize);
    // The major brand, the minor version (not a brand), then the compatible ones.
    let brands = head.get(8..size.min(head.len())).unwrap_or_default();
    brands
        .as_chunks::<4>()
        .0
        .iter()
        .enumerate()
        .any(|(i, brand)| i != 1 && BRANDS.iter().any(|b| b.as_slice() == brand))
}

/// Make and model, the make once: `Canon` + `Canon EOS R5` is `Canon EOS R5`, `NIKON
/// CORPORATION` + `NIKON Z 6` is `NIKON Z 6`, `Apple` + `iPhone 15 Pro` is `Apple iPhone 15 Pro`.
fn camera(make: &str, model: &str) -> String {
    let (make, model) = (make.trim(), model.trim());
    let brand = make.split_whitespace().next().unwrap_or_default();
    if model.to_lowercase().starts_with(&brand.to_lowercase()) { model.to_owned() } else { format!("{make} {model}") }
}

/// `2024:07:01 09:30:00`.
fn parse_date(text: &str) -> Option<DateParts> {
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
#[allow(clippy::indexing_slicing, reason = "tests of files they make")]
mod tests {
    use super::*;

    #[test]
    fn parses_exif_dates() {
        let parts = parse_date("2024:07:01 09:30:05").unwrap();
        assert_eq!(
            (parts.year, parts.month, parts.day, parts.hour, parts.minute, parts.second),
            (2024, 7, 1, 9, 30, 5)
        );
        assert!(parse_date("0000:00:00 00:00:00").is_none(), "cameras write zeros when unset");
        assert!(parse_date("garbage").is_none());
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
    fn the_make_is_named_once() {
        assert_eq!(camera("Canon", "Canon EOS R5"), "Canon EOS R5");
        assert_eq!(camera("NIKON CORPORATION", "NIKON Z 6"), "NIKON Z 6");
        assert_eq!(camera("Apple", "iPhone 15 Pro"), "Apple iPhone 15 Pro");
        assert_eq!(camera("", "X100V"), "X100V");
        assert_eq!(camera("", ""), "");
    }

    #[test]
    fn a_heif_meta_to_the_end_is_not_read_whole() {
        let dir = std::env::temp_dir().join(format!("gezik-exif-heif-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut data = b"\0\0\0\x18ftypheic\0\0\0\0mif1heic".to_vec();
        data.extend(0u32.to_be_bytes());
        data.extend(b"meta");
        data.resize(4 * 1024 * 1024, 0);
        std::fs::write(dir.join("big.heic"), data).unwrap();
        assert!(taken(&dir.join("big.heic")).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hostile_photos_do_not_panic() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
        let seeds = [std::fs::read(dir.join("exif.jpg")).unwrap(), std::fs::read(dir.join("exif.png")).unwrap()];
        super::super::tests::fuzz(&seeds, 64, &|r, _| {
            let mut out = Out::new(f::FieldSet::ALL);
            read(r, &mut out);
        });
    }

    #[test]
    fn files_without_exif() {
        assert!(taken(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml").as_path()).is_none());
        assert!(may_have_exif("IMG.JPG") && !may_have_exif("a.txt"));
    }
}
