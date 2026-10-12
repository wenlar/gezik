//! Video files: MP4 and MOV (`mp4.rs`), Matroska and WebM (EBML: the segment's `Info` and
//! `Tracks` before the first cluster), AVI (`hdrl`: `avih`, each stream's `strh` and `strf`).

use std::io::{Read, Seek};

use gezik_core::fields as f;

use super::{FRAME_MAX, Out, be32, bytes, le16, le32, some_bytes};

pub(super) fn read<R: Read + Seek>(r: &mut R, len: u64, ext: &str, out: &mut Out) {
    let head = some_bytes(r, 0, 12);
    if head.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        matroska(r, len, out);
    } else if head.starts_with(b"RIFF") && head.get(8..12) == Some(b"AVI ") {
        avi(r, len, out);
    } else if head.get(4..8) == Some(b"ftyp") || matches!(ext, "mov" | "mp4" | "m4v") {
        super::mp4::read(r, len, out);
    }
}

/// An EBML variable-length number at `at`: an element id keeps its length marker, a size does
/// not (and all ones is "unknown", `u64::MAX`). Its value and length.
fn vint(b: &[u8], at: usize, id: bool) -> Option<(u64, usize)> {
    let first = *b.get(at)?;
    let len = first.leading_zeros() as usize + 1;
    if len > if id { 4 } else { 8 } {
        return None;
    }
    let mut value = if id { u64::from(first) } else { u64::from(first) & (0xFF >> len) };
    for i in 1..len {
        value = (value << 8) | u64::from(*b.get(at + i)?);
    }
    let unknown = !id && value == (1u64 << (7 * len)) - 1;
    Some((if unknown { u64::MAX } else { value }, len))
}

/// The elements in `data`, each as its id and body; ends at the first broken one.
fn elements(data: &[u8]) -> impl Iterator<Item = (u64, &[u8])> {
    let mut at = 0;
    std::iter::from_fn(move || {
        let (id, n) = vint(data, at, true)?;
        let (size, m) = vint(data, at + n, false)?;
        let from = at + n + m;
        let to = from.checked_add(usize::try_from(size).ok()?)?;
        let body = data.get(from..to)?;
        at = to;
        Some((id, body))
    })
}

fn uint(b: &[u8]) -> u64 {
    b.iter().take(8).fold(0, |n, byte| (n << 8) | u64::from(*byte))
}

fn float(b: &[u8]) -> f64 {
    match b.len() {
        4 => f64::from(f32::from_bits(be32(b, 0).unwrap_or(0))),
        8 => f64::from_bits(super::be64(b, 0).unwrap_or(0)),
        _ => 0.0,
    }
}

const SEGMENT: u64 = 0x1853_8067;
const INFO: u64 = 0x1549_A966;
const TRACKS: u64 = 0x1654_AE6B;
const CLUSTER: u64 = 0x1F43_B675;

fn matroska<R: Read + Seek>(r: &mut R, len: u64, out: &mut Out) {
    // The EBML header, then the segment.
    let mut pos = 0u64;
    let mut segment_end = len;
    for step in 0..256 {
        let head = some_bytes(r, pos, 12);
        let (Some((id, n)), true) = (vint(&head, 0, true), pos < segment_end) else { return };
        let Some((size, m)) = vint(&head, n, false) else { return };
        let body = pos + (n + m) as u64;
        match id {
            SEGMENT => {
                if size != u64::MAX {
                    segment_end = body.saturating_add(size).min(len);
                }
                pos = body;
                continue;
            }
            CLUSTER => return,
            INFO | TRACKS => {
                let Some(data) = r.seek(std::io::SeekFrom::Start(body)).ok().and_then(|_| bytes(r, size, FRAME_MAX))
                else {
                    return;
                };
                if id == INFO { info(&data, out) } else { tracks(&data, out) }
            }
            _ if step == 0 && id != 0x1A45_DFA3 => return,
            _ => {}
        }
        if size == u64::MAX {
            return;
        }
        pos = body.saturating_add(size);
    }
}

fn info(data: &[u8], out: &mut Out) {
    let (mut scale, mut duration) = (1_000_000u64, 0.0);
    for (id, body) in elements(data) {
        match id {
            0x2A_D7B1 => scale = uint(body),
            0x4489 => duration = float(body),
            0x7BA9 => out.text(f::TITLE, &String::from_utf8_lossy(body)),
            _ => {}
        }
    }
    // The duration counts `scale` nanoseconds.
    let ns = duration * scale as f64;
    if ns.is_finite() && ns > 0.0 && ns < 1e21 {
        out.duration(ns as u64, 1_000_000_000);
    }
}

fn tracks(data: &[u8], out: &mut Out) {
    for (_, entry) in elements(data).filter(|(id, _)| *id == 0xAE) {
        let (mut kind, mut codec, mut frame_ns) = (0, String::new(), 0);
        let (mut size, mut sound) = ((0, 0), (0.0, 0));
        for (id, body) in elements(entry) {
            match id {
                0x83 => kind = uint(body),
                0x86 => codec = String::from_utf8_lossy(body).into_owned(),
                0x23_E383 => frame_ns = uint(body),
                0xE0 => {
                    for (id, body) in elements(body) {
                        match id {
                            0xB0 => size.0 = uint(body),
                            0xBA => size.1 = uint(body),
                            _ => {}
                        }
                    }
                }
                0xE1 => {
                    for (id, body) in elements(body) {
                        match id {
                            0xB5 => sound.0 = float(body),
                            0x9F => sound.1 = uint(body),
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        let codec = super::codec_name(&codec);
        match kind {
            1 if !out.has(f::VIDEO_CODEC) => {
                if let Some(codec) = codec {
                    out.text(f::VIDEO_CODEC, &codec);
                }
                out.int(f::WIDTH, size.0);
                out.int(f::HEIGHT, size.1);
                if frame_ns > 0 {
                    out.real(f::FRAME_RATE, 1e9 / frame_ns as f64);
                }
            }
            2 if !out.has(f::AUDIO_CODEC) => {
                if let Some(codec) = codec {
                    out.text(f::AUDIO_CODEC, &codec);
                }
                if sound.0.is_finite() && sound.0 >= 1.0 && sound.0 < 1e7 {
                    out.int(f::SAMPLE_RATE, sound.0 as u64);
                }
                out.int(f::CHANNELS, sound.1);
            }
            _ => {}
        }
    }
}

/// The RIFF chunks in `data`, each as its id and body (sizes padded to even).
fn riff(data: &[u8]) -> impl Iterator<Item = ([u8; 4], &[u8])> {
    let mut at = 0usize;
    std::iter::from_fn(move || {
        let id = super::array::<4>(data, at)?;
        let size = le32(data, at + 4)? as usize;
        let body = data.get(at + 8..(at + 8).checked_add(size)?)?;
        at = at + 8 + size + (size & 1);
        Some((id, body))
    })
}

fn avi<R: Read + Seek>(r: &mut R, len: u64, out: &mut Out) {
    // `hdrl` is the first list after the header.
    let head = some_bytes(r, 12, 12);
    if head.get(..4) != Some(b"LIST") || head.get(8..12) != Some(b"hdrl") {
        return;
    }
    let size = u64::from(le32(&head, 4).unwrap_or(0)).min(len);
    let Some(hdrl) = bytes(r, size.saturating_sub(4), FRAME_MAX) else { return };
    for (id, body) in riff(&hdrl) {
        match &id {
            b"avih" => {
                let us = le32(body, 0).unwrap_or(0);
                out.duration(u64::from(le32(body, 16).unwrap_or(0)) * u64::from(us), 1_000_000);
                out.int(f::WIDTH, le32(body, 32).unwrap_or(0).into());
                out.int(f::HEIGHT, le32(body, 36).unwrap_or(0).into());
            }
            b"LIST" if body.starts_with(b"strl") => stream(body.get(4..).unwrap_or_default(), out),
            _ => {}
        }
    }
}

/// One AVI stream: `strh` says its kind and rate, `strf` its format.
fn stream(strl: &[u8], out: &mut Out) {
    let Some((_, strh)) = riff(strl).find(|(id, _)| id == b"strh") else { return };
    let strf = riff(strl).find(|(id, _)| id == b"strf").map(|(_, b)| b).unwrap_or_default();
    match strh.get(..4) {
        Some(b"vids") if !out.has(f::VIDEO_CODEC) => {
            // The bitmap header's compression, else the stream's handler.
            let code = strf.get(16..20).filter(|c| c.iter().any(|b| *b != 0)).or_else(|| strh.get(4..8));
            if let Some(codec) = code.and_then(|c| super::codec_name(&String::from_utf8_lossy(c))) {
                out.text(f::VIDEO_CODEC, &codec);
            }
            if let (Some(scale @ 1..), Some(rate)) = (le32(strh, 20), le32(strh, 24)) {
                out.real(f::FRAME_RATE, f64::from(rate) / f64::from(scale));
            }
        }
        Some(b"auds") if !out.has(f::AUDIO_CODEC) => {
            let codec = match le16(strf, 0) {
                Some(1) => "PCM",
                Some(0x50) => "MP2",
                Some(0x55) => "MP3",
                Some(0xFF | 0x1610) => "AAC",
                Some(0x2000) => "AC-3",
                _ => "",
            };
            out.text(f::AUDIO_CODEC, codec);
            out.int(f::CHANNELS, le16(strf, 2).unwrap_or(0).into());
            out.int(f::SAMPLE_RATE, le32(strf, 4).unwrap_or(0).into());
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/fields").join(name)
    }

    #[test]
    fn matroska_and_avi() {
        let mkv = super::super::read(&fixture("video.mkv"), f::FieldSet::ALL, &|| false);
        let shown = |id| mkv.text(id).unwrap_or_default();
        assert_eq!(shown(f::DIMENSIONS), "64 × 48");
        assert_eq!(shown(f::VIDEO_CODEC), "H.264");
        assert_eq!(shown(f::AUDIO_CODEC), "Opus");
        assert_eq!(shown(f::DURATION), "0:02");
        assert_eq!(shown(f::TITLE), "Deneme");
        assert_eq!(shown(f::CHANNELS), "1");
        let avi = super::super::read(&fixture("video.avi"), f::FieldSet::ALL, &|| false);
        let shown = |id| avi.text(id).unwrap_or_default();
        assert_eq!(shown(f::DIMENSIONS), "64 × 48");
        assert_eq!(shown(f::VIDEO_CODEC), "MPEG-4");
        assert_eq!(shown(f::AUDIO_CODEC), "MP3");
        assert_eq!(shown(f::FRAME_RATE), "10 fps");
        assert_eq!(shown(f::DURATION), "0:02");
        assert_eq!(shown(f::SAMPLE_RATE), "8,000 Hz");
    }

    #[test]
    fn ebml_numbers() {
        assert_eq!(vint(&[0x81], 0, false), Some((1, 1)));
        assert_eq!(vint(&[0x40, 0x02], 0, false), Some((2, 2)));
        assert_eq!(vint(&[0xFF], 0, false), Some((u64::MAX, 1)), "unknown size");
        assert_eq!(vint(&[0x1A, 0x45, 0xDF, 0xA3], 0, true), Some((0x1A45_DFA3, 4)));
        assert_eq!(vint(&[0x00, 1], 0, false), None);
        assert_eq!(vint(&[0x10], 0, true), None, "cut short");
    }

    #[test]
    fn hostile_videos_do_not_panic() {
        let seeds = [std::fs::read(fixture("video.mkv")).unwrap(), std::fs::read(fixture("video.avi")).unwrap()];
        super::super::tests::fuzz(&seeds, 12, &|r, len| read(r, len, "mkv", &mut Out::new(f::FieldSet::ALL)));
    }
}
