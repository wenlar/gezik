//! Sound files: MP3 (ID3v2, ID3v1, the first frame's Xing/VBRI header or a constant bit rate),
//! AAC (ADTS), FLAC (STREAMINFO, Vorbis comment, PICTURE), Ogg Vorbis and Opus (the first two
//! packets and the last page's granule), M4A (`mp4.rs`), WAV (`fmt `, `data`, `LIST/INFO`) and
//! AIFF (`COMM`, `NAME`, `AUTH`). Found by their first bytes; the extension only chose this
//! reader.

use std::io::{Read, Seek, SeekFrom};

use encoding_rs::{UTF_16BE, UTF_16LE, WINDOWS_1252};
use gezik_core::fields::{self as f, FieldId};

use super::{FRAME_MAX, Out, be16, be32, bytes, le16, le32, some_bytes};

/// Where a reader puts the cover it finds (`None`: not asked for).
type Cover<'a> = Option<&'a mut Option<Vec<u8>>>;

pub(super) fn read<R: Read + Seek>(r: &mut R, len: u64, ext: &str, out: &mut Out, mut cover: Cover) {
    let head = some_bytes(r, 0, 12);
    if head.starts_with(b"RIFF") && head.get(8..12) == Some(b"WAVE") {
        return wav(r, len, out, cover);
    }
    if head.starts_with(b"FORM") && matches!(head.get(8..12), Some(b"AIFF" | b"AIFC")) {
        return aiff(r, len, out, cover);
    }
    if head.starts_with(b"OggS") {
        return ogg(r, len, out);
    }
    if head.get(4..8) == Some(b"ftyp") {
        return super::mp4::read(r, len, out);
    }
    // MP3, AAC and FLAC may start with an ID3v2 tag.
    let start = id3v2(r, 0, out, &mut cover);
    if some_bytes(r, start, 4) == b"fLaC" {
        return flac(r, start, out, cover);
    }
    let v1 = id3v1(r, len, out);
    let end = if v1 { len - 128 } else { len };
    if ext == "aac" { adts(r, start, out) } else { mpeg(r, start, end, out) }
}

/// The embedded cover (spec 13 §6.1).
pub(super) fn cover<R: Read + Seek>(r: &mut R, len: u64, ext: &str) -> Option<Vec<u8>> {
    if some_bytes(r, 4, 4) == b"ftyp" {
        return super::mp4::cover(r, len);
    }
    let mut cover = None;
    read(r, len, ext, &mut Out::new(f::FieldSet::NONE), Some(&mut cover));
    cover
}

/// Text in ID3's encodings (0 Latin-1, 1 UTF-16 with a mark, 2 UTF-16BE, 3 UTF-8), several
/// values (NUL between them) joined with "; ".
fn id3_text(encoding: u8, bytes: &[u8]) -> String {
    let text = match encoding {
        1 => UTF_16LE.decode(bytes).0,
        2 => UTF_16BE.decode_without_bom_handling(bytes).0,
        3 => String::from_utf8_lossy(bytes),
        _ => WINDOWS_1252.decode_without_bom_handling(bytes).0,
    };
    text.trim_end_matches('\0').replace('\0', "; ")
}

/// An ID3 or Vorbis genre; `(17)` and `17` (the old numbered list) are left out.
fn genre(out: &mut Out, text: &str) {
    let text = text.trim();
    let named = if text.starts_with('(') { text.split_once(')').map_or("", |(_, rest)| rest) } else { text };
    if !named.chars().all(|c| c.is_ascii_digit()) {
        out.text(f::GENRE, named);
    }
}

/// Reads the ID3v2 tag at `at`, if there is one; where the data after it starts.
fn id3v2<R: Read + Seek>(r: &mut R, at: u64, out: &mut Out, cover: &mut Cover) -> u64 {
    let head = some_bytes(r, at, 10);
    if !head.starts_with(b"ID3") {
        return at;
    }
    let (Some(&version), Some(&flags)) = (head.get(3), head.get(5)) else { return at };
    let syncsafe = |b: &[u8], i: usize| {
        b.get(i..i + 4).map_or(0, |b| b.iter().fold(0u64, |n, byte| (n << 7) | u64::from(byte & 0x7F)))
    };
    // The header, the tag and (flag 0x10) a footer.
    let end = at + 10 + syncsafe(&head, 6) + if flags & 0x10 != 0 { 10 } else { 0 };
    let mut pos = at + 10;
    if flags & 0x40 != 0 && version >= 3 {
        let ext = some_bytes(r, pos, 4);
        pos += if version == 3 { u64::from(be32(&ext, 0).unwrap_or(0)) + 4 } else { syncsafe(&ext, 0) };
    }
    let header = if version == 2 { 6 } else { 10 };
    // A tag of many tiny frames is given up on.
    for _ in 0..512 {
        let frame = some_bytes(r, pos, header);
        if frame.len() < header as usize || frame.first().is_none_or(|b| *b == 0) {
            break;
        }
        let (id, size): (&[u8], u64) = match version {
            2 => (
                frame.get(..3).unwrap_or_default(),
                u64::from(be32(&[&[0], frame.get(3..6).unwrap_or_default()].concat(), 0).unwrap_or(0)),
            ),
            3 => (frame.get(..4).unwrap_or_default(), u64::from(be32(&frame, 4).unwrap_or(0))),
            _ => (frame.get(..4).unwrap_or_default(), syncsafe(&frame, 4)),
        };
        let body_at = pos + header;
        let next = body_at.saturating_add(size);
        if next > end {
            break;
        }
        let format = frame.get(9).copied().unwrap_or(0);
        // Compressed or encrypted frames are skipped; a data length before the frame (2.4) too.
        let (skip, prefix) = match version {
            3 => (format & 0xC0 != 0, 0),
            4 => (format & 0x0C != 0, if format & 0x01 != 0 { 4 } else { 0 }),
            _ => (false, 0),
        };
        let field = match id {
            b"TIT2" | b"TT2" => Some(f::TITLE),
            b"TPE1" | b"TP1" => Some(f::ARTIST),
            b"TALB" | b"TAL" => Some(f::ALBUM),
            b"TPE2" | b"TP2" => Some(f::ALBUM_ARTIST),
            b"TCON" | b"TCO" => Some(f::GENRE),
            b"TCOM" | b"TCM" => Some(f::COMPOSER),
            b"TRCK" | b"TRK" => Some(f::TRACK),
            b"TPOS" | b"TPA" => Some(f::DISC),
            b"TYER" | b"TYE" | b"TDRC" => Some(f::YEAR),
            _ => None,
        };
        let picture = matches!(id, b"APIC" | b"PIC") && cover.as_ref().is_some_and(|c| c.is_none());
        if !skip && (field.is_some_and(|id| out.wants(id)) || picture) {
            let max = if picture { super::COVER_MAX + 4096 } else { FRAME_MAX };
            let _ = r.seek(SeekFrom::Start(body_at));
            let body = bytes(r, size, max).unwrap_or_default();
            let body = body.get(prefix..).unwrap_or_default();
            if let (Some(field), Some((&encoding, text))) = (field, body.split_first()) {
                let text = id3_text(encoding, text);
                match field {
                    f::GENRE => genre(out, &text),
                    f::TRACK | f::DISC | f::YEAR => out.leading_int(field, &text),
                    _ => out.text(field, &text),
                }
            } else if picture
                && let Some(data) = id3_picture(body, version == 2)
                && let Some(cover) = cover.as_deref_mut()
            {
                *cover = Some(data.to_vec());
            }
        }
        pos = next;
    }
    end
}

/// The picture data of an `APIC` (or 2.2's `PIC`) frame.
fn id3_picture(body: &[u8], v22: bool) -> Option<&[u8]> {
    let (&encoding, rest) = body.split_first()?;
    // The MIME type (2.2: a 3-letter format), then the picture type.
    let rest = if v22 { rest.get(3..)? } else { rest.get(rest.iter().position(|b| *b == 0)? + 1..)? };
    let rest = rest.get(1..)?;
    // The description, ended by a NUL (two in UTF-16).
    let data_at = if matches!(encoding, 1 | 2) {
        rest.as_chunks::<2>().0.iter().position(|pair| *pair == [0, 0])? * 2 + 2
    } else {
        rest.iter().position(|b| *b == 0)? + 1
    };
    rest.get(data_at..).filter(|data| !data.is_empty() && data.len() as u64 <= super::COVER_MAX)
}

/// ID3v1 in the last 128 bytes (Latin-1; only what ID3v2 did not give); whether it is there.
fn id3v1<R: Read + Seek>(r: &mut R, len: u64, out: &mut Out) -> bool {
    let Some(at) = len.checked_sub(128) else { return false };
    let tag = some_bytes(r, at, 128);
    if !tag.starts_with(b"TAG") {
        return false;
    }
    let text = |from: usize, to: usize| {
        let bytes = tag.get(from..to).unwrap_or_default();
        let bytes = bytes.split(|b| *b == 0).next().unwrap_or_default();
        WINDOWS_1252.decode_without_bom_handling(bytes).0.trim().to_owned()
    };
    out.text(f::TITLE, &text(3, 33));
    out.text(f::ARTIST, &text(33, 63));
    out.text(f::ALBUM, &text(63, 93));
    out.leading_int(f::YEAR, &text(93, 97));
    // ID3v1.1: a track number after a NUL at the comment's end.
    if tag.get(125) == Some(&0) {
        out.int(f::TRACK, tag.get(126).copied().unwrap_or(0).into());
    }
    true
}

/// An MPEG audio frame header.
struct Frame {
    kbps: u32,
    rate: u32,
    mono: bool,
    layer: u8,
    mpeg1: bool,
    len: usize,
}

fn frame(b: &[u8], at: usize) -> Option<Frame> {
    let h = super::array::<4>(b, at)?;
    let [b0, b1, b2, b3] = h;
    if b0 != 0xFF || b1 & 0xE0 != 0xE0 {
        return None;
    }
    // Version: 0 MPEG 2.5, 2 MPEG 2, 3 MPEG 1; layer: 1 III, 2 II, 3 I.
    let (version, layer_bits) = ((b1 >> 3) & 3, (b1 >> 1) & 3);
    if version == 1 || layer_bits == 0 {
        return None;
    }
    let layer = 4 - layer_bits;
    let mpeg1 = version == 3;
    const V1: [[u16; 15]; 3] = [
        [0, 32, 64, 96, 128, 160, 192, 224, 256, 288, 320, 352, 384, 416, 448],
        [0, 32, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384],
        [0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320],
    ];
    const V2: [[u16; 15]; 2] = [
        [0, 32, 48, 56, 64, 80, 96, 112, 128, 144, 160, 176, 192, 224, 256],
        [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160],
    ];
    let row = if mpeg1 { V1.get(usize::from(layer - 1))? } else { V2.get(usize::from(layer.min(2) - 1))? };
    let kbps = u32::from(*row.get(usize::from(b2 >> 4)).filter(|k| **k > 0)?);
    let base = *[44_100u32, 48_000, 32_000].get(usize::from((b2 >> 2) & 3))?;
    let rate = match version {
        3 => base,
        2 => base / 2,
        _ => base / 4,
    };
    let pad = u32::from((b2 >> 1) & 1);
    let len = match layer {
        1 => (12 * kbps * 1000 / rate + pad) * 4,
        3 if !mpeg1 => 72 * kbps * 1000 / rate + pad,
        _ => 144 * kbps * 1000 / rate + pad,
    } as usize;
    Some(Frame { kbps, rate, mono: b3 >> 6 == 3, layer, mpeg1, len })
}

/// The first MPEG frame after `start` (one that the next frame confirms), and from it the
/// length: frames in a Xing/Info or VBRI header, else a constant bit rate over the data.
fn mpeg<R: Read + Seek>(r: &mut R, start: u64, end: u64, out: &mut Out) {
    let buf = some_bytes(r, start, 64 * 1024);
    let Some((at, head)) = (0..buf.len()).find_map(|i| {
        let first = frame(&buf, i)?;
        let next = i + first.len;
        (first.len > 4 && (next + 4 > buf.len() || frame(&buf, next).is_some())).then_some((i, first))
    }) else {
        return;
    };
    out.text(f::AUDIO_CODEC, ["MP1", "MP2", "MP3"].get(usize::from(head.layer - 1)).unwrap_or(&"MP3"));
    out.int(f::SAMPLE_RATE, head.rate.into());
    out.int(f::CHANNELS, if head.mono { 1 } else { 2 });
    let samples = match (head.layer, head.mpeg1) {
        (1, _) => 384,
        (3, false) => 576,
        _ => 1152,
    };
    let side = match (head.mpeg1, head.mono) {
        (true, false) => 32,
        (true, true) | (false, false) => 17,
        (false, true) => 9,
    };
    let xing = at + 4 + side;
    let frames = match buf.get(xing..xing + 4) {
        Some(b"Xing" | b"Info") if be32(&buf, xing + 4).is_some_and(|flags| flags & 1 != 0) => be32(&buf, xing + 8),
        _ if buf.get(at + 36..at + 40) == Some(b"VBRI") => be32(&buf, at + 36 + 14),
        _ => None,
    };
    let data = end.saturating_sub(start + at as u64);
    match frames {
        Some(frames) => {
            out.duration(u64::from(frames) * samples, head.rate.into());
            if let Some(gezik_core::fields::Value::Duration(ms @ 1..)) = out.get(f::DURATION).cloned() {
                out.int(f::BITRATE, data * 8 / ms);
            }
        }
        None => {
            out.int(f::BITRATE, head.kbps.into());
            out.duration(data * 8, u64::from(head.kbps) * 1000);
        }
    }
}

/// An ADTS (raw AAC) header after `start`: its sample rate and channels.
fn adts<R: Read + Seek>(r: &mut R, start: u64, out: &mut Out) {
    let head = some_bytes(r, start, 7);
    let (Some(sync), Some(&b2), Some(&b3)) = (be16(&head, 0), head.get(2), head.get(3)) else { return };
    if sync & 0xFFF6 != 0xFFF0 {
        return;
    }
    out.text(f::AUDIO_CODEC, "AAC");
    const RATES: [u32; 13] =
        [96_000, 88_200, 64_000, 48_000, 44_100, 32_000, 24_000, 22_050, 16_000, 12_000, 11_025, 8_000, 7_350];
    out.int(f::SAMPLE_RATE, RATES.get(usize::from((b2 >> 2) & 0xF)).copied().unwrap_or(0).into());
    out.int(f::CHANNELS, u64::from(((b2 & 1) << 2) | (b3 >> 6)));
}

/// FLAC's metadata blocks from `at` (after an ID3v2 tag, if any).
fn flac<R: Read + Seek>(r: &mut R, at: u64, out: &mut Out, mut cover: Cover) {
    out.text(f::AUDIO_CODEC, "FLAC");
    let mut pos = at + 4;
    for _ in 0..128 {
        let head = some_bytes(r, pos, 4);
        let (Some(&kind), Some(size)) = (head.first(), be32(&head, 0).map(|n| u64::from(n & 0xFF_FFFF))) else {
            return;
        };
        match kind & 0x7F {
            0 => {
                let b = bytes(r, size, FRAME_MAX).unwrap_or_default();
                if let (Some(&[r0, r1, r2, r3]), Some(low)) = (super::array::<4>(&b, 10).as_ref(), be32(&b, 14)) {
                    let rate = (u32::from(r0) << 12) | (u32::from(r1) << 4) | (u32::from(r2) >> 4);
                    out.int(f::SAMPLE_RATE, rate.into());
                    out.int(f::CHANNELS, u64::from((r2 >> 1) & 7) + 1);
                    out.duration((u64::from(r3 & 0xF) << 32) | u64::from(low), rate.into());
                }
            }
            4 => vorbis_comments(&bytes(r, size, FRAME_MAX).unwrap_or_default(), out),
            6 if cover.as_ref().is_some_and(|c| c.is_none()) => {
                let block = bytes(r, size, super::COVER_MAX + 4096).unwrap_or_default();
                if let (Some(data), Some(cover)) = (flac_picture(&block), cover.as_deref_mut()) {
                    *cover = Some(data.to_vec());
                }
            }
            _ => {}
        }
        if kind & 0x80 != 0 {
            return;
        }
        pos += 4 + size;
    }
}

/// A FLAC `PICTURE` block's data: after the type, MIME, description and four numbers.
fn flac_picture(b: &[u8]) -> Option<&[u8]> {
    let mime = be32(b, 4)? as usize;
    let desc_at = 8usize.checked_add(mime)?;
    let desc = be32(b, desc_at)? as usize;
    let len_at = desc_at.checked_add(4)?.checked_add(desc)?.checked_add(16)?;
    let len = be32(b, len_at)? as usize;
    b.get(len_at + 4..len_at.checked_add(4)?.checked_add(len)?)
}

/// A Vorbis comment (FLAC, Ogg Vorbis, Opus): `KEY=value` pairs after the vendor string.
fn vorbis_comments(b: &[u8], out: &mut Out) {
    let Some(mut pos) = le32(b, 0).and_then(|n| (n as usize).checked_add(4)) else { return };
    let count = le32(b, pos).unwrap_or(0);
    pos += 4;
    for _ in 0..count.min(1000) {
        let Some(len) = le32(b, pos).map(|n| n as usize) else { return };
        let Some(entry) = pos.checked_add(4).and_then(|from| b.get(from..from.checked_add(len)?)) else { return };
        pos += 4 + len;
        let entry = String::from_utf8_lossy(entry);
        let Some((key, value)) = entry.split_once('=') else { continue };
        let field: Option<FieldId> = match key.to_ascii_uppercase().as_str() {
            "TITLE" => Some(f::TITLE),
            "ARTIST" => Some(f::ARTIST),
            "ALBUM" => Some(f::ALBUM),
            "ALBUMARTIST" | "ALBUM ARTIST" | "ALBUM_ARTIST" => Some(f::ALBUM_ARTIST),
            "COMPOSER" => Some(f::COMPOSER),
            "GENRE" => {
                genre(out, value);
                None
            }
            "TRACKNUMBER" => Some(f::TRACK),
            "DISCNUMBER" => Some(f::DISC),
            "DATE" | "YEAR" => Some(f::YEAR),
            _ => None,
        };
        match field {
            Some(id @ (f::TRACK | f::DISC | f::YEAR)) => out.leading_int(id, value),
            Some(id) => out.text(id, value),
            None => {}
        }
    }
}

/// Ogg Vorbis or Opus: the first two packets of the first stream (identification, comments),
/// and the length from the last page's granule position.
fn ogg<R: Read + Seek>(r: &mut R, len: u64, out: &mut Out) {
    let (mut packets, mut packet, mut serial, mut pos, mut total) = (Vec::new(), Vec::new(), None, 0u64, 0u64);
    'pages: for _ in 0..512 {
        let head = some_bytes(r, pos, 27);
        if !head.starts_with(b"OggS") {
            break;
        }
        let count = u64::from(head.get(26).copied().unwrap_or(0));
        let segments = some_bytes(r, pos + 27, count);
        let size: u64 = segments.iter().map(|s| u64::from(*s)).sum();
        let page_serial = le32(&head, 14);
        if serial.is_none() {
            serial = page_serial;
        }
        if page_serial == serial {
            total += size;
            if total > FRAME_MAX {
                break;
            }
            let data = some_bytes(r, pos + 27 + count, size);
            let mut at = 0;
            for &segment in &segments {
                let segment = usize::from(segment);
                packet.extend_from_slice(data.get(at..at + segment).unwrap_or_default());
                at += segment;
                if segment < 255 {
                    packets.push(std::mem::take(&mut packet));
                    if packets.len() == 2 {
                        break 'pages;
                    }
                }
            }
        }
        pos += 27 + count + size;
    }
    let (Some(id), Some(serial)) = (packets.first(), serial) else { return };
    let comments = packets.get(1).map(Vec::as_slice).unwrap_or_default();
    // The granule position's rate and its start (Opus' pre-skip).
    let (rate, skip) = if id.starts_with(b"\x01vorbis") {
        out.text(f::AUDIO_CODEC, "Vorbis");
        let rate = le32(id, 12).unwrap_or(0);
        out.int(f::SAMPLE_RATE, rate.into());
        out.int(f::CHANNELS, id.get(11).copied().unwrap_or(0).into());
        out.int(f::BITRATE, u64::try_from(le32(id, 20).unwrap_or(0) as i32).unwrap_or(0) / 1000);
        if let Some(rest) = comments.strip_prefix(b"\x03vorbis") {
            vorbis_comments(rest, out);
        }
        (u64::from(rate), 0)
    } else if id.starts_with(b"OpusHead") {
        out.text(f::AUDIO_CODEC, "Opus");
        out.int(f::CHANNELS, id.get(9).copied().unwrap_or(0).into());
        let input = le32(id, 12).unwrap_or(0);
        out.int(f::SAMPLE_RATE, if input > 0 { input } else { 48_000 }.into());
        if let Some(rest) = comments.strip_prefix(b"OpusTags") {
            vorbis_comments(rest, out);
        }
        (48_000, u64::from(le16(id, 10).unwrap_or(0)))
    } else {
        return;
    };
    let from = len.saturating_sub(64 * 1024);
    let tail = some_bytes(r, from, 64 * 1024);
    let last = (0..tail.len().saturating_sub(27)).rev().find_map(|i| {
        let page = tail.get(i..)?;
        (page.starts_with(b"OggS") && le32(page, 14) == Some(serial))
            .then(|| super::array::<8>(page, 6).map(i64::from_le_bytes))
            .flatten()
            .filter(|g| *g >= 0)
    });
    if let Some(granule) = last {
        out.duration((granule as u64).saturating_sub(skip), rate);
    }
}

/// Text of a RIFF or AIFF chunk: UTF-8 if it is, else Latin-1; up to the first NUL.
fn chunk_text(b: &[u8]) -> String {
    let b = b.split(|c| *c == 0).next().unwrap_or_default();
    std::str::from_utf8(b).map_or_else(|_| WINDOWS_1252.decode_without_bom_handling(b).0.into_owned(), str::to_owned)
}

/// The chunks of a RIFF (`le`) or AIFF file after its 12-byte header, each as its id, size and
/// where its body starts.
fn chunks<R: Read + Seek>(r: &mut R, len: u64, le: bool, mut each: impl FnMut(&mut R, [u8; 4], u64, u64)) {
    let mut pos = 12u64;
    for _ in 0..256 {
        let head = some_bytes(r, pos, 8);
        let (Some(id), Some(size)) = (super::array::<4>(&head, 0), if le { le32(&head, 4) } else { be32(&head, 4) })
        else {
            return;
        };
        let size = u64::from(size);
        each(r, id, size.min(len.saturating_sub(pos + 8)), pos + 8);
        pos += 8 + size + (size & 1);
        if pos >= len {
            return;
        }
    }
}

fn wav<R: Read + Seek>(r: &mut R, len: u64, out: &mut Out, mut cover: Cover) {
    let (mut byte_rate, mut data) = (0u32, None);
    chunks(r, len, true, |r, id, size, at| match &id {
        b"fmt " => {
            let b = some_bytes(r, at, 16);
            out.text(
                f::AUDIO_CODEC,
                match le16(&b, 0) {
                    Some(1 | 3 | 0xFFFE) => "PCM",
                    Some(0x55) => "MP3",
                    _ => "",
                },
            );
            out.int(f::CHANNELS, le16(&b, 2).unwrap_or(0).into());
            out.int(f::SAMPLE_RATE, le32(&b, 4).unwrap_or(0).into());
            byte_rate = le32(&b, 8).unwrap_or(0);
        }
        b"data" => data = Some(size),
        b"LIST" => {
            let b = some_bytes(r, at, size.min(FRAME_MAX));
            if b.starts_with(b"INFO") {
                let mut pos = 4;
                while let (Some(id), Some(n)) = (super::array::<4>(&b, pos), le32(&b, pos + 4).map(|n| n as usize)) {
                    let text = chunk_text(b.get(pos + 8..(pos + 8).saturating_add(n)).unwrap_or_default());
                    match &id {
                        b"INAM" => out.text(f::TITLE, &text),
                        b"IART" => out.text(f::ARTIST, &text),
                        b"IPRD" => out.text(f::ALBUM, &text),
                        b"IGNR" => genre(out, &text),
                        b"ICRD" => out.leading_int(f::YEAR, &text),
                        b"ITRK" | b"IPRT" => out.leading_int(f::TRACK, &text),
                        _ => {}
                    }
                    pos = pos.saturating_add(8 + n + (n & 1));
                }
            }
        }
        b"id3 " | b"ID3 " => _ = id3v2(r, at, out, &mut cover),
        _ => {}
    });
    if let Some(data) = data {
        out.duration(data, byte_rate.into());
    }
}

fn aiff<R: Read + Seek>(r: &mut R, len: u64, out: &mut Out, mut cover: Cover) {
    chunks(r, len, false, |r, id, size, at| match &id {
        b"COMM" => {
            let b = some_bytes(r, at, 22);
            out.int(f::CHANNELS, be16(&b, 0).unwrap_or(0).into());
            // An 80-bit extended float.
            let (Some(top), Some(mantissa)) = (be16(&b, 8), super::be64(&b, 10)) else { return };
            let exponent = i32::from(top & 0x7FFF) - 16383 - 63;
            let rate = if top & 0x8000 == 0 { mantissa as f64 * 2f64.powi(exponent) } else { 0.0 };
            if (1.0..1e7).contains(&rate) {
                out.int(f::SAMPLE_RATE, rate as u64);
                out.duration(be32(&b, 2).unwrap_or(0).into(), rate as u64);
            }
            match b.get(18..22) {
                None | Some(b"NONE" | b"sowt" | b"twos" | b"fl32" | b"in24") => out.text(f::AUDIO_CODEC, "PCM"),
                Some(other) => out.text(f::AUDIO_CODEC, &String::from_utf8_lossy(other)),
            }
        }
        b"NAME" => out.text(f::TITLE, &chunk_text(&some_bytes(r, at, size.min(FRAME_MAX)))),
        b"AUTH" => out.text(f::ARTIST, &chunk_text(&some_bytes(r, at, size.min(FRAME_MAX)))),
        b"ID3 " | b"id3 " => _ = id3v2(r, at, out, &mut cover),
        _ => {}
    });
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/fields").join(name)
    }

    fn fields(name: &str) -> gezik_core::fields::Fields {
        super::super::read(&fixture(name), f::FieldSet::ALL, &|| false)
    }

    fn assert_tags(name: &str) {
        let fields = fields(name);
        let shown = |id| fields.text(id).unwrap_or_default();
        assert_eq!(shown(f::TITLE), "Ağıt Şarkısı", "{name}");
        assert_eq!(shown(f::ARTIST), "Çiğdem Öztürk", "{name}");
        assert_eq!(shown(f::ALBUM), "Gölge", "{name}");
        assert_eq!(shown(f::ALBUM_ARTIST), "Öztürk Ailesi", "{name}");
        assert_eq!(shown(f::GENRE), "Halk", "{name}");
        assert_eq!(shown(f::COMPOSER), "Anonim", "{name}");
        assert_eq!(shown(f::TRACK), "3", "{name}");
        assert_eq!(shown(f::DISC), "1", "{name}");
        assert_eq!(shown(f::YEAR), "2024", "{name}");
        assert_eq!(shown(f::DURATION), "0:02", "{name}");
        assert_eq!(shown(f::CHANNELS), "1", "{name}");
        assert!(fields.get(f::SAMPLE_RATE).is_some() && fields.get(f::BITRATE).is_some(), "{name}: {fields:?}");
    }

    #[test]
    fn tags_of_every_sound_format() {
        for name in ["id3v23.mp3", "id3v24.mp3", "tags.flac", "tags.opus", "tags.ogg"] {
            assert_tags(name);
        }
        assert_eq!(fields("id3v23.mp3").text(f::AUDIO_CODEC).as_deref(), Some("MP3"));
        assert_eq!(fields("tags.flac").text(f::SAMPLE_RATE).as_deref(), Some("8,000 Hz"));
        assert_eq!(fields("tags.opus").text(f::AUDIO_CODEC).as_deref(), Some("Opus"));
        assert_eq!(fields("tags.ogg").text(f::AUDIO_CODEC).as_deref(), Some("Vorbis"));
    }

    #[test]
    fn id3v1_and_a_constant_bit_rate() {
        let fields = fields("id3v1.mp3");
        let shown = |id| fields.text(id).unwrap_or_default();
        assert_eq!(shown(f::TITLE), "Gölge");
        assert_eq!(shown(f::ARTIST), "Öztürk");
        assert_eq!(shown(f::ALBUM), "Ünlü");
        assert_eq!(shown(f::YEAR), "1999");
        assert_eq!(shown(f::TRACK), "7");
        assert_eq!(shown(f::BITRATE), "32 kb/s");
        assert_eq!(shown(f::DURATION), "0:02");
    }

    #[test]
    fn wav_and_aiff() {
        let wav = fields("tags.wav");
        assert_eq!(wav.text(f::TITLE).as_deref(), Some("Dalga"));
        assert_eq!(wav.text(f::ARTIST).as_deref(), Some("Öztürk"));
        assert_eq!(wav.get(f::DURATION), Some(&gezik_core::fields::Value::Duration(500)));
        assert_eq!(wav.text(f::AUDIO_CODEC).as_deref(), Some("PCM"));
        assert_eq!(wav.text(f::SAMPLE_RATE).as_deref(), Some("8,000 Hz"));
        let aiff = fields("plain.aiff");
        assert_eq!(aiff.get(f::DURATION), Some(&gezik_core::fields::Value::Duration(250)));
        assert_eq!(aiff.text(f::CHANNELS).as_deref(), Some("2"));
        assert_eq!(aiff.text(f::SAMPLE_RATE).as_deref(), Some("8,000 Hz"));
    }

    #[test]
    fn covers() {
        for name in ["id3v24.mp3", "tags.flac", "tags.m4a"] {
            let cover = super::super::cover(&fixture(name)).unwrap_or_else(|| panic!("{name} has a cover"));
            assert!(cover.starts_with(b"\x89PNG"), "{name}");
        }
        assert!(super::super::cover(&fixture("id3v23.mp3")).is_none());
    }

    #[test]
    fn numbered_genres_are_left_out() {
        let mut out = Out::new(f::FieldSet::ALL);
        genre(&mut out, "(17)");
        genre(&mut out, "17");
        assert!(!out.has(f::GENRE));
        genre(&mut out, "(17)Rock");
        assert_eq!(out.get(f::GENRE), Some(&gezik_core::fields::Value::Text("Rock".into())));
    }

    #[test]
    fn hostile_sound_files_do_not_panic() {
        for (names, ext) in [
            (&["id3v23.mp3", "id3v24.mp3", "id3v1.mp3"][..], "mp3"),
            (&["tags.flac"][..], "flac"),
            (&["tags.opus", "tags.ogg"][..], "ogg"),
            (&["tags.wav", "plain.aiff"][..], "wav"),
        ] {
            let seeds: Vec<Vec<u8>> = names.iter().map(|n| std::fs::read(fixture(n)).unwrap()).collect();
            super::super::tests::fuzz(&seeds, 12, &|r, len| {
                read(r, len, ext, &mut Out::new(f::FieldSet::ALL), Some(&mut None));
            });
        }
    }
}
