//! A picture's size from its header (the `image` crate), as it shows: a photo turned by its EXIF
//! orientation has width and height swapped. HEIF and AVIF from their `ispe` box.

use std::io::{BufRead, Seek};
use std::path::Path;

use gezik_core::fields as f;

use super::Out;

pub(super) fn read<R: BufRead + Seek>(r: &mut R, path: &Path, ext: &str, out: &mut Out) {
    let size = if matches!(ext, "heic" | "heif" | "avif") { ispe(r) } else { header_size(path) };
    if let Some((w, h)) = size {
        out.int(f::WIDTH, w.into());
        out.int(f::HEIGHT, h.into());
    }
}

/// Through `ImageReader::open`, as the preview decodes: its decoders are in the exe once.
fn header_size(path: &Path) -> Option<(u32, u32)> {
    use image::ImageDecoder;
    use image::metadata::Orientation;
    let mut decoder = image::ImageReader::open(path).ok()?.with_guessed_format().ok()?.into_decoder().ok()?;
    let (w, h) = decoder.dimensions();
    let turned = matches!(
        decoder.orientation(),
        Ok(Orientation::Rotate90 | Orientation::Rotate270 | Orientation::Rotate90FlipH | Orientation::Rotate270FlipH)
    );
    Some(if turned { (h, w) } else { (w, h) })
}

/// The largest `ispe` (image spatial extent) in `meta/iprp/ipco`: a HEIC photo's tiles each
/// have one, the whole picture the largest.
fn ispe<R: BufRead + Seek>(r: &mut R) -> Option<(u32, u32)> {
    let len = r.seek(std::io::SeekFrom::End(0)).ok()?;
    let meta = super::mp4::find_top(r, len, *b"meta", super::FRAME_MAX)?;
    let mut best: Option<(u32, u32)> = None;
    // `meta` is a full box: its children start after the version and flags.
    let body = meta.get(4..)?;
    for (kind, iprp) in super::mp4::boxes(body) {
        if kind != *b"iprp" {
            continue;
        }
        for (kind, ipco) in super::mp4::boxes(iprp) {
            if kind != *b"ipco" {
                continue;
            }
            for (kind, prop) in super::mp4::boxes(ipco) {
                if kind == *b"ispe"
                    && let (Some(w), Some(h)) = (super::be32(prop, 4), super::be32(prop, 8))
                    && best.is_none_or(|(bw, bh)| u64::from(w) * u64::from(h) > u64::from(bw) * u64::from(bh))
                {
                    best = Some((w, h));
                }
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    use super::super::mp4::tests::make_box;

    /// A HEIC's start: `ftyp`, then `meta` with a 512 × 512 tile and the 4032 × 3024 picture.
    pub fn heic() -> Vec<u8> {
        let ispe = |w: u32, h: u32| {
            let mut body = vec![0; 4];
            body.extend(w.to_be_bytes());
            body.extend(h.to_be_bytes());
            make_box(b"ispe", &body)
        };
        let ipco = make_box(b"ipco", &[ispe(512, 512), ispe(4032, 3024)].concat());
        let iprp = make_box(b"iprp", &ipco);
        let mut meta = vec![0; 4];
        meta.extend(make_box(b"hdlr", &[0; 24]));
        meta.extend(iprp);
        let mut file = make_box(b"ftyp", b"heic\0\0\0\0mif1heic");
        file.extend(make_box(b"meta", &meta));
        file.extend(make_box(b"mdat", &[7; 64]));
        file
    }

    #[test]
    fn a_heic_says_its_largest_extent() {
        let mut out = Out::new(f::FieldSet::ALL);
        read(&mut Cursor::new(heic()), Path::new("x.heic"), "heic", &mut out);
        let fields = out.finish(0);
        assert_eq!(fields.text(f::DIMENSIONS).as_deref(), Some("4032 × 3024"));
    }

    #[test]
    fn a_turned_photo_says_its_shown_size() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/fields/photo.jpg");
        let fields = super::super::read(&path, f::FieldSet::ALL, &|| false);
        // 32 × 24 stored, orientation 6: it stands.
        assert_eq!(fields.text(f::DIMENSIONS).as_deref(), Some("24 × 32"));
        assert_eq!(fields.text(f::CAMERA).as_deref(), Some("Apple iPhone 15 Pro"), "Apple is not in the model");
        assert_eq!(fields.text(f::EXPOSURE).as_deref(), Some("1/250 s"));
        assert_eq!(fields.text(f::APERTURE).as_deref(), Some("f/1.8"));
        assert_eq!(fields.text(f::ISO).as_deref(), Some("100"));
        assert_eq!(fields.text(f::FOCAL_LENGTH).as_deref(), Some("6.9 mm"));
        assert_eq!(fields.text(f::GPS).as_deref(), Some("41.0082° N, 28.9784° E"));
        assert_eq!(fields.text(f::TAKEN).as_deref(), Some("2024-07-01 09:30"));
        assert!(fields.text(f::LENS).is_some_and(|l| l.starts_with("iPhone 15 Pro back camera")));
    }

    #[test]
    fn hostile_pictures_do_not_panic() {
        // Other formats' headers are the `image` crate's (the preview decodes them the same way).
        let run = |r: &mut Cursor<Vec<u8>>, _| read(r, Path::new("x.heic"), "heic", &mut Out::new(f::FieldSet::ALL));
        super::super::tests::fuzz(&[heic()], 24, &run);
    }
}
