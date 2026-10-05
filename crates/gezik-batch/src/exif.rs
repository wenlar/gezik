//! When a photo was taken, from its EXIF data (JPEG, TIFF, HEIC, WebP, PNG). A JPEG's EXIF
//! is at its start, so only the start is read; the other formats are read as far as needed.

use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use gezik_core::batch::date::DateParts;

/// Formats that can carry EXIF, by extension.
pub fn may_have_exif(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [".jpg", ".jpeg", ".tif", ".tiff", ".heic", ".heif", ".webp", ".png"].iter().any(|ext| lower.ends_with(ext))
}

/// The date the photo was taken (`DateTimeOriginal`, else `DateTime`), as the camera wrote it
/// (local time). `None` if the file has none or cannot be read.
pub fn taken(path: &Path) -> Option<DateParts> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut magic = [0u8; 2];
    let jpeg = file.read_exact(&mut magic).is_ok() && magic == [0xFF, 0xD8];
    file.seek(SeekFrom::Start(0)).ok()?;
    // A JPEG's EXIF is in its first segments: 256 KB is plenty and bounds the read. The
    // other containers can keep it after the image data, so they are read from a plain file.
    let exif = if jpeg {
        exif::Reader::new().read_from_container(&mut BufReader::new(file.take(256 * 1024)))
    } else {
        exif::Reader::new().read_from_container(&mut BufReader::new(file))
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
    fn files_without_exif() {
        assert!(taken(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml").as_path()).is_none());
        assert!(may_have_exif("IMG.JPG") && !may_have_exif("a.txt"));
    }
}
