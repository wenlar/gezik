//! The freedesktop thumbnail cache (Thumbnail Managing Standard 0.9), read only (spec 9 §8.5,
//! decision 29): the thumbnail another program made for a file, if it is still current. Gezik
//! runs no thumbnailer and writes nothing here. Only the cache and the file's metadata are
//! read, never the file: a cloud or network file is not fetched. Compiled everywhere.

use std::io::Read;
use std::path::Path;

use crate::Rgba;

/// Cache files bigger than this are not read (a 1024 px thumbnail is about 1-2 MB).
pub const MAX_THUMB_BYTES: u64 = 8 * 1024 * 1024;
/// The cache's size folders and the side each holds.
pub const SIZES: [(&str, u32); 4] = [("normal", 128), ("large", 256), ("x-large", 512), ("xx-large", 1024)];
/// The most `tEXt` chunks read from one PNG.
const MAX_TEXTS: usize = 64;

const S: [u32; 64] = [
    7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20,
    4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15,
    21,
];
const K: [u32; 64] = [
    0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501, 0x698098d8,
    0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821, 0xf61e2562, 0xc040b340,
    0x265e5a51, 0xe9b6c7aa, 0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8, 0x21e1cde6, 0xc33707d6, 0xf4d50d87,
    0x455a14ed, 0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a, 0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c,
    0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70, 0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05, 0xd9d4d039,
    0xe6db99e5, 0x1fa27cf8, 0xc4ac5665, 0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92,
    0xffeff47d, 0x85845dd1, 0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb,
    0xeb86d391,
];

/// MD5 (RFC 1321): only for the cache's file names, never for security.
pub fn md5(data: &[u8]) -> [u8; 16] {
    let (mut a0, mut b0, mut c0, mut d0) = (0x6745_2301u32, 0xefcd_ab89u32, 0x98ba_dcfeu32, 0x1032_5476u32);
    let mut message = data.to_vec();
    let bits = (data.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bits.to_le_bytes());
    for block in message.as_chunks::<64>().0 {
        let m: [u32; 16] = std::array::from_fn(|i| {
            u32::from_le_bytes([block[4 * i], block[4 * i + 1], block[4 * i + 2], block[4 * i + 3]])
        });
        let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let f = f.wrapping_add(a).wrapping_add(K[i]).wrapping_add(m[g]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(f.rotate_left(S[i]));
        }
        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }
    let mut out = [0u8; 16];
    for (i, word) in [a0, b0, c0, d0].iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&word.to_le_bytes());
    }
    out
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A file's URI as GLib makes it for the cache (decision 6): `file_uri` with `;` escaped too.
pub fn cache_uri(path: &Path) -> String {
    crate::linux::uri::file_uri(path).replace(';', "%3B")
}

/// The cache file name of `path`: the MD5 of its URI in hex, `.png`.
pub fn cache_name(path: &Path) -> String {
    format!("{}.png", hex(&md5(cache_uri(path).as_bytes())))
}

/// The size folders to look in for `px` (decision 5): the smallest big enough, then the
/// bigger ones, then the smaller ones from the biggest down.
pub fn folders_for(px: u32) -> Vec<&'static str> {
    let first = SIZES.iter().position(|(_, side)| *side >= px).unwrap_or(SIZES.len());
    SIZES[first..].iter().chain(SIZES[..first].iter().rev()).map(|(name, _)| *name).collect()
}

/// The `tEXt` chunks of a PNG (keyword, Latin-1 text), wherever they are; at most 64; empty
/// if it is not a PNG. Stops at the first chunk that runs past the end. CRCs are not checked
/// (`image` checks the picture's own).
pub fn png_texts(bytes: &[u8]) -> Vec<(String, String)> {
    const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut out = Vec::new();
    if bytes.get(..8) != Some(&SIGNATURE[..]) {
        return out;
    }
    let mut at = 8usize;
    while let Some(head) = at.checked_add(8).and_then(|end| bytes.get(at..end)) {
        let len = u32::from_be_bytes([head[0], head[1], head[2], head[3]]) as usize;
        let kind = &head[4..8];
        let start = at.saturating_add(8);
        let Some(end) = start.checked_add(len) else { break };
        let Some(data) = bytes.get(start..end) else { break };
        if kind == b"tEXt" {
            if out.len() == MAX_TEXTS {
                break;
            }
            if let Some(nul) = data.iter().position(|&b| b == 0) {
                out.push((super::from_latin1(&data[..nul]), super::from_latin1(&data[nul + 1..])));
            }
        }
        if kind == b"IEND" {
            break;
        }
        match end.checked_add(4) {
            Some(next) => at = next,
            None => break,
        }
    }
    out
}

/// Whether the texts say the thumbnail is of a file last changed at `mtime` (seconds).
pub fn is_current(texts: &[(String, String)], mtime: u64) -> bool {
    texts.iter().find(|(key, _)| key == "Thumb::MTime").is_some_and(|(_, value)| {
        let value = value.trim();
        let seconds = value
            .parse::<u64>()
            .ok()
            .or_else(|| value.parse::<f64>().ok().filter(|f| f.is_finite() && *f >= 0.0).map(|f| f as u64));
        seconds == Some(mtime)
    })
}

/// The bytes of the cache file `name` in `root/folder`, if it is a plain file (no link: the
/// cache is read, never what a link in it points to) in a plain folder, at most
/// [`MAX_THUMB_BYTES`].
fn read_cache_file(root: &Path, folder: &str, name: &str) -> Option<Vec<u8>> {
    let file = root.join(folder).join(name);
    let meta = std::fs::symlink_metadata(&file).ok()?;
    if !meta.is_file() || meta.len() > MAX_THUMB_BYTES {
        return None;
    }
    if !std::fs::symlink_metadata(root.join(folder)).ok()?.is_dir() {
        return None;
    }
    let mut bytes = Vec::new();
    // Bounded again: the file may have grown since the check.
    std::fs::File::open(&file).ok()?.take(MAX_THUMB_BYTES + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() as u64 <= MAX_THUMB_BYTES).then_some(bytes)
}

/// A current thumbnail of `path` from the cache at `root` (`~/.cache/thumbnails`), at most
/// `px` on its longer side; `None` if there is none. Reads `path`'s metadata, never its data.
pub fn cached(path: &Path, px: u32, root: &Path) -> Option<Rgba> {
    if !path.has_root() {
        return None;
    }
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    let mtime = modified.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    let name = cache_name(path);
    for folder in folders_for(px) {
        let Some(bytes) = read_cache_file(root, folder, &name) else { continue };
        if !is_current(&png_texts(&bytes), mtime) {
            continue;
        }
        let mut reader = image::ImageReader::with_format(std::io::Cursor::new(&bytes), image::ImageFormat::Png);
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(crate::picture::MAX_DECODE_SIDE);
        limits.max_image_height = Some(crate::picture::MAX_DECODE_SIDE);
        limits.max_alloc = Some(crate::picture::MAX_DECODE_ALLOC);
        reader.limits(limits);
        let Ok(picture) = reader.decode() else { continue };
        let rgba = picture.into_rgba8();
        let image = Rgba { width: rgba.width(), height: rgba.height(), pixels: rgba.into_raw() };
        return Some(crate::icons::shrink_to(image, px));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_with(texts: &[(&str, &str)], side: u32) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, side, side);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            for (key, text) in texts {
                encoder.add_text_chunk((*key).to_owned(), (*text).to_owned()).unwrap();
            }
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&vec![200; (side * side * 4) as usize]).unwrap();
        }
        out
    }

    /// A chunk with a zero CRC (the text reader does not check it).
    fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut out = (data.len() as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        out.extend_from_slice(&[0; 4]);
        out
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("gezik-9b10-thumbs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn mtime_of(file: &Path) -> u64 {
        std::fs::metadata(file).unwrap().modified().unwrap().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()
    }

    #[test]
    fn md5_matches_rfc_1321() {
        let k: [u32; 64] = std::array::from_fn(|i| ((i as f64 + 1.0).sin().abs() * 4_294_967_296.0) as u32);
        assert_eq!(k, K, "the table is the sine rule's");
        for (text, want) in [
            ("", "d41d8cd98f00b204e9800998ecf8427e"),
            ("a", "0cc175b9c0f1b6a831c399e269772661"),
            ("abc", "900150983cd24fb0d6963f7d28e17f72"),
            ("message digest", "f96b697d7cb7938d525a2f31aaf161d0"),
            ("abcdefghijklmnopqrstuvwxyz", "c3fcd3d76192e4007dfb496cca67e13b"),
            (
                "12345678901234567890123456789012345678901234567890123456789012345678901234567890",
                "57edf4a22be3c955ac49da2e2107b67a",
            ),
        ] {
            assert_eq!(hex(&md5(text.as_bytes())), want, "{text:?}");
        }
    }

    #[test]
    fn the_cache_name_is_the_md5_of_glibs_uri() {
        let path = Path::new("/home/u/Resimler/ağaç ş;1.png");
        assert_eq!(cache_uri(path), "file:///home/u/Resimler/a%C4%9Fa%C3%A7%20%C5%9F%3B1.png");
        assert_eq!(cache_uri(Path::new("/a/b!$&'()*+,=:@~_-.c")), "file:///a/b!$&'()*+,=:@~_-.c");
        assert_eq!(cache_name(path), format!("{}.png", hex(&md5(cache_uri(path).as_bytes()))));
        assert_eq!(cache_name(path).len(), 36);
    }

    #[test]
    fn folders_go_big_enough_then_bigger_then_smaller() {
        assert_eq!(folders_for(96), ["normal", "large", "x-large", "xx-large"]);
        assert_eq!(folders_for(128), ["normal", "large", "x-large", "xx-large"]);
        assert_eq!(folders_for(200), ["large", "x-large", "xx-large", "normal"]);
        assert_eq!(folders_for(600), ["xx-large", "x-large", "large", "normal"]);
        assert_eq!(folders_for(4000), ["xx-large", "x-large", "large", "normal"]);
    }

    #[test]
    fn png_texts_are_read_anywhere_and_garbage_is_safe() {
        let png = png_with(&[("Thumb::MTime", "1700000000"), ("Software", "GNOME::ThumbnailFactory")], 2);
        let texts = png_texts(&png);
        assert!(texts.contains(&("Thumb::MTime".into(), "1700000000".into())));
        // A tEXt after IDAT (before IEND) is read too.
        let mut late = png[..png.len() - 12].to_vec();
        late.extend(chunk(b"tEXt", b"Thumb::URI\0file:///x"));
        late.extend_from_slice(&png[png.len() - 12..]);
        assert!(png_texts(&late).contains(&("Thumb::URI".into(), "file:///x".into())));
        assert!(png_texts(b"not a png").is_empty());
        assert!(png_texts(&png[..20]).is_empty(), "cut short");
        let mut huge = png[..8].to_vec();
        huge.extend_from_slice(&u32::MAX.to_be_bytes());
        huge.extend_from_slice(b"tEXt");
        assert!(png_texts(&huge).is_empty(), "a length past the end");
        let mut many = png[..8].to_vec();
        for _ in 0..100 {
            many.extend(chunk(b"tEXt", b"k\0v"));
        }
        assert_eq!(png_texts(&many).len(), 64, "at most 64");
        // Every prefix of a real PNG and a text without a NUL are safe.
        for end in 0..png.len() {
            let _ = png_texts(&png[..end]);
        }
        let mut no_nul = png[..8].to_vec();
        no_nul.extend(chunk(b"tEXt", b"no separator"));
        assert!(png_texts(&no_nul).is_empty());
        assert!(is_current(&texts, 1_700_000_000));
        assert!(!is_current(&texts, 1_700_000_001));
        assert!(
            is_current(&[("Thumb::MTime".into(), "1700000000.75".into())], 1_700_000_000),
            "a fraction rounds down"
        );
        assert!(!is_current(&[], 1), "no MTime: not current");
        assert!(!is_current(&[("Thumb::MTime".into(), "-5".into())], 0));
        assert!(!is_current(&[("Thumb::MTime".into(), "NaN".into())], 0));
    }

    #[test]
    fn a_current_thumbnail_is_found_and_shrunk() {
        let dir = scratch("found");
        let file = dir.join("rapor ş;1.pdf");
        std::fs::write(&file, b"%PDF").unwrap();
        let mtime = mtime_of(&file);
        let root = dir.join("thumbnails");
        std::fs::create_dir_all(root.join("large")).unwrap();
        let png = png_with(&[("Thumb::URI", &cache_uri(&file)), ("Thumb::MTime", &mtime.to_string())], 256);
        std::fs::write(root.join("large").join(cache_name(&file)), &png).unwrap();
        let found = cached(&file, 64, &root).unwrap();
        assert_eq!((found.width, found.height), (64, 64));
        assert_eq!(cached(&file, 512, &root).map(|r| r.width), Some(256), "a smaller one when no bigger is there");
        assert!(cached(Path::new("relative.pdf"), 64, &root).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stale_broken_and_big_thumbnails_are_skipped() {
        let dir = scratch("skipped");
        let file = dir.join("a.mp4");
        std::fs::write(&file, b"x").unwrap();
        let mtime = mtime_of(&file);
        let root = dir.join("thumbnails");
        for folder in ["normal", "large"] {
            std::fs::create_dir_all(root.join(folder)).unwrap();
        }
        let name = cache_name(&file);
        std::fs::write(root.join("normal").join(&name), png_with(&[("Thumb::MTime", &(mtime + 1).to_string())], 8))
            .unwrap();
        assert!(cached(&file, 64, &root).is_none(), "stale");
        let good = png_with(&[("Thumb::MTime", &mtime.to_string())], 8);
        std::fs::write(root.join("normal").join(&name), &good[..good.len() / 2]).unwrap();
        assert!(cached(&file, 64, &root).is_none(), "cut short");
        std::fs::write(root.join("normal").join(&name), vec![0u8; (MAX_THUMB_BYTES + 1) as usize]).unwrap();
        assert!(cached(&file, 64, &root).is_none(), "too big to read");
        // A PNG that says it is 100000×100000: refused by the limits, not allocated.
        let mut giant = good.clone();
        giant[16..20].copy_from_slice(&100_000u32.to_be_bytes());
        giant[20..24].copy_from_slice(&100_000u32.to_be_bytes());
        std::fs::write(root.join("normal").join(&name), &giant).unwrap();
        assert!(cached(&file, 64, &root).is_none(), "too large a picture");
        std::fs::write(root.join("large").join(&name), &good).unwrap();
        assert!(cached(&file, 64, &root).is_some(), "the next folder's is used");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn links_in_the_cache_are_not_followed() {
        let dir = scratch("links");
        let file = dir.join("a.mp4");
        std::fs::write(&file, b"x").unwrap();
        let root = dir.join("thumbnails");
        std::fs::create_dir_all(root.join("normal")).unwrap();
        let outside = dir.join("outside.png");
        std::fs::write(&outside, png_with(&[("Thumb::MTime", &mtime_of(&file).to_string())], 8)).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("normal").join(cache_name(&file))).unwrap();
        assert!(cached(&file, 64, &root).is_none(), "a linked file");
        let elsewhere = dir.join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::fs::copy(&outside, elsewhere.join(cache_name(&file))).unwrap();
        std::os::unix::fs::symlink(&elsewhere, root.join("large")).unwrap();
        assert!(cached(&file, 64, &root).is_none(), "a linked folder");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
