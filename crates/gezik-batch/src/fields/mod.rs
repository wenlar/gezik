//! Reading a file's metadata fields (spec 13 §7.3): one reader per format, each reading its own
//! bounded part of the file once and filling every field it knows. File contents are not
//! trusted: no indexing (a panic closes Gezik, `panic = "abort"`), every read is bounded, and no
//! length from the file sizes an allocation before its bytes are there.

#![deny(clippy::indexing_slicing)]

mod audio;
mod document;
pub mod exif;
mod image;
mod mp4;
mod pdf;
mod video;

use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use gezik_core::fields::{self as f, FieldId, FieldSet, Fields, Reader, Value};

/// A text value is cut to this many characters.
const TEXT_MAX: usize = 1000;
/// The most read for one tag frame, box or chunk that is parsed in memory.
const FRAME_MAX: u64 = 1024 * 1024;
/// The most read for an embedded cover picture.
pub const COVER_MAX: u64 = 8 * 1024 * 1024;

/// The fields of `path` among `want`: only the readers of its extension that give one of them
/// run (none: nothing is opened); `stop` is asked between readers. A file only in the cloud is
/// not read (that would download it).
pub fn read(path: &Path, want: FieldSet, stop: &dyn Fn() -> bool) -> Fields {
    let ext = extension(path);
    let readers: Vec<Reader> = f::readers_for(&ext, want).collect();
    if readers.is_empty() || gezik_platform::only_in_cloud(path) {
        return Fields::default();
    }
    let Ok(file) = std::fs::File::open(path) else { return Fields::default() };
    let len = file.metadata().map_or(0, |m| m.len());
    let mut out = Out::new(want);
    read_from(&mut BufReader::new(file), len, &ext, &readers, &mut out, stop);
    out.finish(len)
}

/// The embedded cover of a sound file (ID3 `APIC`, MP4 `covr`, FLAC `PICTURE`), at most
/// [`COVER_MAX`]: the poster before playing (spec 13 §6.1). The caller checks the cloud.
pub fn cover(path: &Path) -> Option<Vec<u8>> {
    let ext = extension(path);
    if !matches!(ext.as_str(), "mp3" | "flac" | "m4a" | "aac" | "wav") {
        return None;
    }
    let file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    audio::cover(&mut BufReader::new(file), len, &ext)
}

fn extension(path: &Path) -> String {
    path.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase()
}

fn read_from<R: BufRead + Seek>(
    r: &mut R,
    len: u64,
    ext: &str,
    readers: &[Reader],
    out: &mut Out,
    stop: &dyn Fn() -> bool,
) {
    for reader in readers {
        if stop() || r.seek(SeekFrom::Start(0)).is_err() {
            return;
        }
        match reader {
            Reader::Image => image::read(r, ext, out),
            Reader::Exif => exif::read(r, out),
            Reader::Audio => audio::read(r, len, ext, out, None),
            Reader::Video => video::read(r, len, ext, out),
            Reader::Document => document::read(r, out),
            Reader::Pdf => pdf::read(r, len, out),
        }
    }
}

/// What the readers found; the first value of a field stays.
struct Out {
    want: FieldSet,
    values: Vec<(FieldId, Value)>,
}

impl Out {
    fn new(want: FieldSet) -> Out {
        Out { want, values: Vec::new() }
    }

    fn wants(&self, id: FieldId) -> bool {
        self.want.has(id) && !self.has(id)
    }

    fn has(&self, id: FieldId) -> bool {
        self.values.iter().any(|(i, _)| *i == id)
    }

    fn put(&mut self, id: FieldId, value: Value) {
        if self.wants(id) {
            self.values.push((id, value));
        }
    }

    fn text(&mut self, id: FieldId, text: &str) {
        if self.wants(id)
            && let Some(text) = clean(text)
        {
            self.values.push((id, Value::Text(text)));
        }
    }

    /// A count or size: 0 means unknown.
    fn int(&mut self, id: FieldId, n: u64) {
        if let Ok(n @ 1..) = i64::try_from(n) {
            self.put(id, Value::Int(n));
        }
    }

    /// The number a text starts with: `3/12` is track 3, `2024-05-01` is year 2024.
    fn leading_int(&mut self, id: FieldId, text: &str) {
        let digits = text.trim_start();
        let end = digits.find(|c: char| !c.is_ascii_digit()).unwrap_or(digits.len());
        self.int(id, digits.get(..end).and_then(|d| d.parse().ok()).unwrap_or(0));
    }

    fn real(&mut self, id: FieldId, x: f64) {
        if x.is_finite() && x > 0.0 {
            self.put(id, Value::Real(x));
        }
    }

    /// `units` of `scale` per second (`scale` 0: unknown).
    fn duration(&mut self, units: u64, scale: u64) {
        if scale > 0 && units > 0 {
            let ms = u128::from(units) * 1000 / u128::from(scale);
            self.put(f::DURATION, Value::Duration(u64::try_from(ms).unwrap_or(u64::MAX)));
        }
    }

    /// The values found, with what follows from them: dimensions from width and height, and a
    /// bit rate from the length when the format gave none.
    fn finish(mut self, len: u64) -> Fields {
        if let (Some(Value::Int(w)), Some(Value::Int(h))) = (self.get(f::WIDTH), self.get(f::HEIGHT)) {
            let pixels = w.saturating_mul(*h);
            self.put(f::DIMENSIONS, Value::Int(pixels));
        }
        if let Some(Value::Duration(ms @ 1..)) = self.get(f::DURATION) {
            let kbps = len.saturating_mul(8) / ms;
            self.int(f::BITRATE, kbps);
        }
        self.values.retain(|(id, _)| self.want.has(*id));
        Fields::new(self.values)
    }

    fn get(&self, id: FieldId) -> Option<&Value> {
        self.values.iter().find(|(i, _)| *i == id).map(|(_, v)| v)
    }
}

/// A text value without control characters, trimmed and cut to [`TEXT_MAX`] characters.
fn clean(text: &str) -> Option<Box<str>> {
    let text: String = text.chars().filter(|c| !c.is_control()).take(TEXT_MAX).collect();
    let text = text.trim();
    (!text.is_empty()).then(|| text.into())
}

/// Exactly `len` bytes, if `len` is at most `max` and the file has them. The buffer grows with
/// the bytes read, so a huge length in a small file allocates nothing much.
fn bytes<R: Read>(r: &mut R, len: u64, max: u64) -> Option<Vec<u8>> {
    if len > max {
        return None;
    }
    let mut data = Vec::new();
    r.take(len).read_to_end(&mut data).ok()?;
    (data.len() as u64 == len).then_some(data)
}

/// Up to `max` bytes from `at` (fewer at the end of the file).
fn some_bytes<R: Read + Seek>(r: &mut R, at: u64, max: u64) -> Vec<u8> {
    let mut data = Vec::new();
    if r.seek(SeekFrom::Start(at)).is_ok() {
        let _ = r.take(max).read_to_end(&mut data);
    }
    data
}

fn array<const N: usize>(b: &[u8], at: usize) -> Option<[u8; N]> {
    b.get(at..at.checked_add(N)?)?.try_into().ok()
}

fn be16(b: &[u8], at: usize) -> Option<u16> {
    array(b, at).map(u16::from_be_bytes)
}

fn be32(b: &[u8], at: usize) -> Option<u32> {
    array(b, at).map(u32::from_be_bytes)
}

fn be64(b: &[u8], at: usize) -> Option<u64> {
    array(b, at).map(u64::from_be_bytes)
}

fn le16(b: &[u8], at: usize) -> Option<u16> {
    array(b, at).map(u16::from_le_bytes)
}

fn le32(b: &[u8], at: usize) -> Option<u32> {
    array(b, at).map(u32::from_le_bytes)
}

/// A codec's usual name from a container's code (`avc1`, `V_MPEG4/ISO/AVC`, `XVID`); other
/// printable codes as they are.
fn codec_name(code: &str) -> Option<String> {
    let code = code.trim_matches(|c: char| c == '\0' || c.is_whitespace());
    let upper = code.to_ascii_uppercase();
    let name = match upper.as_str() {
        "AVC1" | "AVC3" | "H264" | "X264" | "V_MPEG4/ISO/AVC" => "H.264",
        "HVC1" | "HEV1" | "HEVC" | "H265" | "V_MPEGH/ISO/HEVC" => "H.265",
        "AV01" | "V_AV1" => "AV1",
        "VP09" | "V_VP9" => "VP9",
        "VP08" | "V_VP8" => "VP8",
        "MP4V" | "XVID" | "DIVX" | "DX50" | "FMP4" | "V_MPEG4/ISO/ASP" | "V_MPEG4/ISO/SP" => "MPEG-4",
        "MJPG" | "JPEG" | "V_MJPEG" => "Motion JPEG",
        "APCN" | "APCH" | "APCS" | "APCO" | "AP4H" | "V_PRORES" => "ProRes",
        "MP4A" | "A_AAC" => "AAC",
        "AC-3" | "A_AC3" => "AC-3",
        "EC-3" | "A_EAC3" => "E-AC-3",
        "OPUS" | "A_OPUS" => "Opus",
        "A_VORBIS" => "Vorbis",
        "ALAC" | "A_ALAC" => "ALAC",
        "FLAC" | "A_FLAC" => "FLAC",
        ".MP3" | "A_MPEG/L3" => "MP3",
        "LPCM" | "SOWT" | "TWOS" | "IN24" | "IN32" | "FL32" | "A_PCM/INT/LIT" | "A_PCM/INT/BIG"
        | "A_PCM/FLOAT/IEEE" => "PCM",
        _ if upper.starts_with("A_AAC") => "AAC",
        _ => {
            return (!code.is_empty() && code.len() <= 32 && code.chars().all(|c| c.is_ascii_graphic()))
                .then(|| code.to_owned());
        }
    };
    Some(name.to_owned())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    /// A small random number source (no crate for it).
    pub struct XorShift(pub u64);

    impl XorShift {
        pub fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

        pub fn below(&mut self, n: usize) -> usize {
            (self.next() % n.max(1) as u64) as usize
        }
    }

    /// 10,000 hostile inputs for a reader: `seeds` cut short and with bytes changed (lengths
    /// among them), and random bytes after each seed's first `keep` bytes. None may panic, and
    /// each may take at most a little memory (a huge length is not allocated: the test would
    /// abort).
    pub fn fuzz(seeds: &[Vec<u8>], keep: usize, run: &dyn Fn(&mut Cursor<Vec<u8>>, u64)) {
        let mut rng = XorShift(0x9E37_79B9_7F4A_7C15);
        for i in 0..10_000 {
            let seed = seeds.get(i % seeds.len()).expect("a seed");
            let mut data = if i % 3 == 0 {
                let mut data = seed.get(..keep.min(seed.len())).unwrap_or_default().to_vec();
                let more = rng.below(4096);
                data.extend((0..more).map(|_| rng.next() as u8));
                data
            } else {
                let mut data = seed.clone();
                for _ in 0..1 + rng.below(8) {
                    let at = rng.below(data.len());
                    if let Some(byte) = data.get_mut(at) {
                        // Often 0xFF: lengths become huge.
                        *byte = if rng.next().is_multiple_of(2) { 0xFF } else { rng.next() as u8 };
                    }
                }
                data.truncate(rng.below(data.len() + 1).max(data.len() / 2));
                data
            };
            if i % 7 == 0 {
                data.truncate(rng.below(data.len() + 1));
            }
            let len = data.len() as u64;
            run(&mut Cursor::new(data), len);
        }
    }

    #[test]
    fn values_are_cleaned_and_cut() {
        assert_eq!(clean(" a\u{0}b\n "), Some("ab".into()));
        assert_eq!(clean("\u{1}\u{2}"), None);
        assert_eq!(clean(&"x".repeat(5000)).map(|t| t.len()), Some(TEXT_MAX));
    }

    #[test]
    fn huge_lengths_are_not_allocated() {
        let mut r = Cursor::new(vec![1u8; 10]);
        assert!(bytes(&mut r, u64::MAX, u64::MAX).is_none(), "only what is there");
        assert!(bytes(&mut r, FRAME_MAX + 1, FRAME_MAX).is_none());
        let mut r = Cursor::new(vec![1u8; 10]);
        assert_eq!(bytes(&mut r, 4, 4), Some(vec![1; 4]));
        assert_eq!(be32(&[0, 0, 1, 0], 0), Some(256));
        assert_eq!(be32(&[0, 0, 1], 0), None);
        assert_eq!(le16(&[1, 0], usize::MAX), None);
    }

    #[test]
    fn the_first_value_stays_and_unwanted_fields_go() {
        let mut out = Out::new(FieldSet::of(&[f::ARTIST, f::WIDTH, f::HEIGHT, f::DIMENSIONS, f::DURATION, f::BITRATE]));
        out.text(f::ARTIST, "One");
        out.text(f::ARTIST, "Two");
        out.text(f::ALBUM, "Not wanted");
        out.int(f::WIDTH, 40);
        out.int(f::HEIGHT, 30);
        out.int(f::TRACK, 0);
        out.duration(90, 1);
        let fields = out.finish(90_000 * 128 / 8);
        assert_eq!(fields.text(f::ARTIST).as_deref(), Some("One"));
        assert!(fields.get(f::ALBUM).is_none());
        assert_eq!(fields.get(f::DIMENSIONS), Some(&Value::Int(1200)));
        assert_eq!(fields.text(f::DURATION).as_deref(), Some("1:30"));
        assert_eq!(fields.get(f::BITRATE), Some(&Value::Int(128)));
    }

    #[test]
    fn codecs_have_their_usual_names() {
        assert_eq!(codec_name("avc1").as_deref(), Some("H.264"));
        assert_eq!(codec_name("V_MPEGH/ISO/HEVC").as_deref(), Some("H.265"));
        assert_eq!(codec_name("A_AAC/MPEG4/LC").as_deref(), Some("AAC"));
        assert_eq!(codec_name("abcd").as_deref(), Some("abcd"));
        assert_eq!(codec_name("\0\0\0\0"), None);
        assert_eq!(codec_name("a\u{1}"), None);
    }

    #[test]
    fn files_of_other_types_and_missing_files_give_nothing() {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        assert!(read(&manifest.join("Cargo.toml"), FieldSet::ALL, &|| false).is_empty());
        assert!(read(&manifest.join("missing.mp3"), FieldSet::ALL, &|| false).is_empty());
        assert!(cover(&manifest.join("tests/data/exif.jpg")).is_none());
    }
}
