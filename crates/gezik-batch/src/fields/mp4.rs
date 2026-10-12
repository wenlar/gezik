//! MP4, MOV and M4A (ISO base media): `moov` is found by the top-level box headers (it may be
//! after the media data, at the end), read whole up to a bound and parsed in memory: length,
//! tracks (codec, size, frame rate, sound) and the `ilst` tags. HEIF's `meta` uses the same box
//! reading (`image.rs`).

use std::io::{Read, Seek, SeekFrom};

use gezik_core::fields as f;

use super::{Out, be16, be32, be64};

/// The most of `moov` read for the fields (spec 13 §7.3).
const MOOV_MAX: u64 = 4 * 1024 * 1024;

pub(super) fn read<R: Read + Seek>(r: &mut R, len: u64, out: &mut Out) {
    if let Some(moov) = find_top(r, len, *b"moov", MOOV_MAX) {
        parse_moov(&moov, out, &mut None);
    }
}

/// The `covr` picture of an M4A.
pub(super) fn cover<R: Read + Seek>(r: &mut R, len: u64) -> Option<Vec<u8>> {
    let moov = find_top(r, len, *b"moov", MOOV_MAX + super::COVER_MAX)?;
    let mut cover = None;
    parse_moov(&moov, &mut Out::new(f::FieldSet::NONE), &mut Some(&mut cover));
    cover
}

/// The body of the first top-level box of type `kind`, found by skipping the boxes before it;
/// `None` if there is none or it is larger than `max`.
pub(super) fn find_top<R: Read + Seek>(r: &mut R, len: u64, kind: [u8; 4], max: u64) -> Option<Vec<u8>> {
    let mut at = 0u64;
    // A file of many tiny boxes is given up on.
    for _ in 0..1024 {
        r.seek(SeekFrom::Start(at)).ok()?;
        let head = super::bytes(r, 16.min(len.checked_sub(at)?), 16)?;
        let (size, header) = match be32(&head, 0)? {
            0 => (len - at, 8),
            1 => (be64(&head, 8)?, 16),
            size => (u64::from(size), 8),
        };
        if size < header {
            return None;
        }
        if super::array::<4>(&head, 4)? == kind {
            r.seek(SeekFrom::Start(at + header)).ok()?;
            return super::bytes(r, size - header, max);
        }
        at = at.checked_add(size)?;
        if at >= len {
            return None;
        }
    }
    None
}

/// The boxes in `data`, each as its type and body; ends at the first broken one.
pub(super) fn boxes(data: &[u8]) -> impl Iterator<Item = ([u8; 4], &[u8])> {
    let mut rest = data;
    std::iter::from_fn(move || {
        let (size, header) = match be32(rest, 0)? {
            0 => (rest.len() as u64, 8),
            1 => (be64(rest, 8)?, 16),
            size => (u64::from(size), 8),
        };
        let kind = super::array::<4>(rest, 4)?;
        let size = usize::try_from(size).ok().filter(|s| *s >= header && *s <= rest.len())?;
        let body = rest.get(header..size)?;
        rest = rest.get(size..)?;
        Some((kind, body))
    })
}

/// A full box's timescale and duration (`mvhd`, `mdhd`; versions 0 and 1).
fn times(b: &[u8]) -> Option<(u64, u64)> {
    let (scale, duration) = match b.first()? {
        1 => (be32(b, 20)?, be64(b, 24)?),
        _ => (be32(b, 12)?, be32(b, 16).map(|d| if d == u32::MAX { u64::MAX } else { d.into() })?),
    };
    // All ones: not known.
    (duration != u64::MAX).then_some((u64::from(scale), duration))
}

fn parse_moov(moov: &[u8], out: &mut Out, cover: &mut Option<&mut Option<Vec<u8>>>) {
    for (kind, body) in boxes(moov) {
        match &kind {
            b"mvhd" => {
                if let Some((scale, duration)) = times(body) {
                    out.duration(duration, scale);
                }
            }
            b"trak" => track(body, out),
            b"udta" => {
                for (kind, body) in boxes(body) {
                    if kind == *b"meta" {
                        meta(body, out, cover);
                    }
                }
            }
            b"meta" => meta(body, out, cover),
            _ => {}
        }
    }
}

/// `meta`: a full box in MP4, a plain one in QuickTime; its `ilst` has the tags.
fn meta(body: &[u8], out: &mut Out, cover: &mut Option<&mut Option<Vec<u8>>>) {
    let body = if be32(body, 0) == Some(0) { body.get(4..).unwrap_or_default() } else { body };
    for (kind, ilst) in boxes(body) {
        if kind != *b"ilst" {
            continue;
        }
        for (item, data) in boxes(ilst) {
            let Some((_, data)) = boxes(data).find(|(kind, _)| kind == b"data") else { continue };
            // The data's type and locale, then the value.
            let Some(value) = data.get(8..) else { continue };
            let text = || String::from_utf8_lossy(value);
            match &item {
                b"\xA9nam" => out.text(f::TITLE, &text()),
                b"\xA9ART" => out.text(f::ARTIST, &text()),
                b"\xA9alb" => out.text(f::ALBUM, &text()),
                b"aART" => out.text(f::ALBUM_ARTIST, &text()),
                b"\xA9gen" => out.text(f::GENRE, &text()),
                b"\xA9wrt" => out.text(f::COMPOSER, &text()),
                b"\xA9day" => out.leading_int(f::YEAR, &text()),
                b"trkn" => out.int(f::TRACK, be16(value, 2).unwrap_or(0).into()),
                b"disk" => out.int(f::DISC, be16(value, 2).unwrap_or(0).into()),
                b"covr" => {
                    if let Some(cover) = cover.as_deref_mut()
                        && cover.is_none()
                        && value.len() as u64 <= super::COVER_MAX
                    {
                        *cover = Some(value.to_vec());
                    }
                }
                _ => {}
            }
        }
    }
}

/// One track: a picture track gives the video codec, size and frame rate, a sound track the
/// audio codec, sample rate and channels (the first of each kind).
fn track(trak: &[u8], out: &mut Out) {
    let (mut handler, mut codec, mut size, mut sound, mut media_times, mut frames) =
        ([0; 4], [0; 4], (0, 0), (0, 0), None, 0u64);
    for (kind, body) in boxes(trak) {
        match &kind {
            // Its last 8 bytes: width and height, 16.16.
            b"tkhd" => {
                let at = body.len().saturating_sub(8);
                size = (be32(body, at).unwrap_or(0) >> 16, be32(body, at + 4).unwrap_or(0) >> 16);
            }
            b"mdia" => {
                for (kind, body) in boxes(body) {
                    match &kind {
                        b"mdhd" => media_times = times(body),
                        b"hdlr" => handler = super::array(body, 8).unwrap_or_default(),
                        b"minf" => {
                            for (_, stbl) in boxes(body).filter(|(kind, _)| kind == b"stbl") {
                                for (kind, body) in boxes(stbl) {
                                    match &kind {
                                        // The first sample entry, after the version, flags and count.
                                        b"stsd" => {
                                            if let Some((format, entry)) =
                                                boxes(body.get(8..).unwrap_or_default()).next()
                                            {
                                                codec = format;
                                                if let (Some(w @ 1..), Some(h @ 1..)) =
                                                    (be16(entry, 24), be16(entry, 26))
                                                {
                                                    size = (w.into(), h.into());
                                                }
                                                sound =
                                                    (be16(entry, 16).unwrap_or(0), be32(entry, 24).unwrap_or(0) >> 16);
                                            }
                                        }
                                        // Samples per run, summed: the frame count.
                                        b"stts" => {
                                            let runs = be32(body, 4).unwrap_or(0) as usize;
                                            frames =
                                                (0..runs).map_while(|i| be32(body, 8 + i * 8)).map(u64::from).sum();
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    let codec = super::codec_name(&String::from_utf8_lossy(&codec));
    match &handler {
        b"vide" if !out.has(f::VIDEO_CODEC) => {
            if let Some(codec) = codec {
                out.text(f::VIDEO_CODEC, &codec);
            }
            out.int(f::WIDTH, size.0.into());
            out.int(f::HEIGHT, size.1.into());
            if let Some((scale, duration @ 1..)) = media_times {
                out.real(f::FRAME_RATE, frames as f64 * scale as f64 / duration as f64);
            }
        }
        b"soun" if !out.has(f::AUDIO_CODEC) => {
            if let Some(codec) = codec {
                out.text(f::AUDIO_CODEC, &codec);
            }
            out.int(f::SAMPLE_RATE, sound.1.into());
            out.int(f::CHANNELS, sound.0.into());
        }
        _ => {}
    }
    if let Some((scale, duration)) = media_times {
        out.duration(duration, scale);
    }
}

#[cfg(test)]
pub(super) mod tests {
    use std::path::Path;

    use super::*;

    pub fn make_box(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut data = u32::try_from(body.len() + 8).unwrap().to_be_bytes().to_vec();
        data.extend(kind);
        data.extend(body);
        data
    }

    fn fixture(name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/fields").join(name)
    }

    #[test]
    fn a_video_with_moov_at_the_end() {
        let fields = super::super::read(&fixture("moov-last.mp4"), f::FieldSet::ALL, &|| false);
        let shown = |id| fields.text(id).unwrap_or_default();
        assert_eq!(shown(f::DIMENSIONS), "64 × 48");
        assert_eq!(shown(f::VIDEO_CODEC), "H.264");
        assert_eq!(shown(f::AUDIO_CODEC), "AAC");
        assert_eq!(shown(f::FRAME_RATE), "10 fps");
        assert_eq!(shown(f::DURATION), "0:02");
        assert_eq!(shown(f::CHANNELS), "1");
        assert_eq!(shown(f::TITLE), "Deneme");
        assert!(fields.get(f::BITRATE).is_some());
    }

    #[test]
    fn m4a_tags_and_cover() {
        let fields = super::super::read(&fixture("tags.m4a"), f::FieldSet::ALL, &|| false);
        let shown = |id| fields.text(id).unwrap_or_default();
        assert_eq!(shown(f::TITLE), "Ağıt Şarkısı");
        assert_eq!(shown(f::ARTIST), "Çiğdem Öztürk");
        assert_eq!(shown(f::ALBUM), "Gölge");
        assert_eq!(shown(f::ALBUM_ARTIST), "Öztürk Ailesi");
        assert_eq!(shown(f::GENRE), "Halk");
        assert_eq!(shown(f::COMPOSER), "Anonim");
        assert_eq!(shown(f::TRACK), "3");
        assert_eq!(shown(f::DISC), "1");
        assert_eq!(shown(f::YEAR), "2024");
        assert_eq!(shown(f::AUDIO_CODEC), "AAC");
        assert_eq!(shown(f::DURATION), "0:02");
        assert!(fields.get(f::VIDEO_CODEC).is_none(), "the cover is not a video track");
        let cover = super::super::cover(&fixture("tags.m4a")).expect("covr");
        assert!(cover.starts_with(b"\x89PNG"));
    }

    #[test]
    fn hostile_mp4s_do_not_panic() {
        let seeds = [std::fs::read(fixture("moov-last.mp4")).unwrap(), std::fs::read(fixture("tags.m4a")).unwrap()];
        super::super::tests::fuzz(&seeds, 32, &|r, len| {
            read(r, len, &mut Out::new(f::FieldSet::ALL));
            let _ = cover(r, len);
        });
        // A box that says it is 4 GB in a small file.
        let mut data = make_box(b"ftyp", b"isom");
        data.extend(u32::MAX.to_be_bytes());
        data.extend(b"moov");
        let len = data.len() as u64;
        assert!(find_top(&mut std::io::Cursor::new(data), len, *b"moov", u64::MAX).is_none());
    }
}
