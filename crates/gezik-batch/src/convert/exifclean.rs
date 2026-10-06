//! Edits of an EXIF block (its TIFF payload, `II*\0…` or `MM\0*…` without the `Exif\0\0`
//! prefix, as `ImageDecoder::exif_metadata` gives it) that never move bytes, so every offset
//! in it stays valid and the length stays the same: remove the GPS data, drop the IFD1
//! thumbnail, set the pixel dimensions. And the location removal of a JPEG file without
//! re-encoding it, which also leaves out XMP, IPTC and whatever follows the main picture
//! (MPF secondary pictures such as an Ultra HDR gain map, a motion photo's video). A
//! damaged block is left as it is (the edit answers `false`); nothing here panics on bad
//! input.

use std::ops::Range;

#[derive(Clone, Copy)]
struct Tiff {
    le: bool,
}

impl Tiff {
    fn new(b: &[u8]) -> Option<Tiff> {
        match b.get(0..4)? {
            [0x49, 0x49, 42, 0] => Some(Tiff { le: true }),
            [0x4d, 0x4d, 0, 42] => Some(Tiff { le: false }),
            _ => None,
        }
    }

    fn u16(self, b: &[u8], at: usize) -> Option<u16> {
        let s: [u8; 2] = b.get(at..at.checked_add(2)?)?.try_into().ok()?;
        Some(if self.le { u16::from_le_bytes(s) } else { u16::from_be_bytes(s) })
    }

    fn u32(self, b: &[u8], at: usize) -> Option<u32> {
        let s: [u8; 4] = b.get(at..at.checked_add(4)?)?.try_into().ok()?;
        Some(if self.le { u32::from_le_bytes(s) } else { u32::from_be_bytes(s) })
    }

    /// Callers check that the bytes are there.
    fn put_u16(self, b: &mut [u8], at: usize, v: u16) {
        b[at..at + 2].copy_from_slice(&if self.le { v.to_le_bytes() } else { v.to_be_bytes() });
    }

    /// Callers check that the bytes are there.
    fn put_u32(self, b: &mut [u8], at: usize, v: u32) {
        b[at..at + 4].copy_from_slice(&if self.le { v.to_le_bytes() } else { v.to_be_bytes() });
    }
}

/// The size of one value of a TIFF field type.
fn type_size(t: u16) -> usize {
    match t {
        1 | 2 | 6 | 7 => 1,
        3 | 8 => 2,
        4 | 9 | 11 => 4,
        5 | 10 | 12 => 8,
        _ => 0,
    }
}

/// One IFD entry: where its 12 bytes start, its tag, type and count.
#[derive(Clone, Copy)]
struct Entry {
    at: usize,
    tag: u16,
    typ: u16,
    count: u32,
}

/// The entries of the IFD at `ifd`, and where its next-IFD field is; `None` unless all of it
/// (entries and the next-IFD field) lies inside `b`.
fn entries(t: Tiff, b: &[u8], ifd: usize) -> Option<(Vec<Entry>, usize)> {
    let n = t.u16(b, ifd)? as usize;
    let next = ifd.checked_add(2 + n * 12)?;
    b.get(next..next.checked_add(4)?)?;
    let list = (0..n)
        .map(|i| {
            let at = ifd + 2 + i * 12;
            Some(Entry { at, tag: t.u16(b, at)?, typ: t.u16(b, at + 2)?, count: t.u32(b, at + 4)? })
        })
        .collect::<Option<Vec<_>>>()?;
    Some((list, next))
}

/// Zeroes an entry's out-of-line data (if it has any and it lies inside `b`) and its 12 bytes.
fn wipe_entry(t: Tiff, b: &mut [u8], e: Entry) {
    let size = type_size(e.typ).saturating_mul(e.count as usize);
    if size > 4
        && let Some(off) = t.u32(b, e.at + 8)
    {
        let start = off as usize;
        if let Some(data) = start.checked_add(size).and_then(|end| b.get_mut(start..end)) {
            data.fill(0);
        }
    }
    b[e.at..e.at + 12].fill(0);
}

fn ifd0(t: Tiff, b: &[u8]) -> Option<usize> {
    Some(t.u32(b, 4)? as usize)
}

/// The offset an IFD0 pointer entry (`tag`: 0x8769 Exif, 0x8825 GPS) points to.
fn sub_ifd(t: Tiff, b: &[u8], tag: u16) -> Option<usize> {
    let (list, _) = entries(t, b, ifd0(t, b)?)?;
    let e = list.iter().find(|e| e.tag == tag)?;
    Some(t.u32(b, e.at + 8)? as usize)
}

/// The smallest offset an IFD can have: right after the 8-byte header.
const FIRST_IFD: usize = 8;

/// Whether the block has a GPS IFD pointer; `None` when its IFD0 cannot be read, or has more
/// than one GPS pointer (then it may hold a location nothing here can find).
pub fn has_gps(b: &[u8]) -> Option<bool> {
    let t = Tiff::new(b)?;
    let (list, _) = entries(t, b, ifd0(t, b)?)?;
    match list.iter().filter(|e| e.tag == 0x8825).count() {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
}

/// Removes the location: every GPS value (and its out-of-line data) is zeroed, and the GPS
/// IFD keeps one entry, `GPSVersionID = 2.2.0.0`, so that strict readers still find a valid
/// IFD where IFD0 points. Answers whether it did; `false` also when there is no GPS IFD, or
/// when it cannot be read (then nothing is changed: see [`has_gps`]).
pub fn strip_gps(b: &mut [u8]) -> bool {
    (|| {
        let t = Tiff::new(b)?;
        has_gps(b)?;
        let gps = sub_ifd(t, b, 0x8825)?;
        if gps < FIRST_IFD || gps == ifd0(t, b)? {
            return None;
        }
        let (list, next) = entries(t, b, gps)?;
        // Room for what is written at the end, the one entry and the next-IFD field after
        // it (2 + 12 + 4 bytes), checked before anything changes.
        b.get(gps..gps.checked_add(18)?)?;
        for e in list {
            wipe_entry(t, b, e);
        }
        b[next..next + 4].fill(0);
        t.put_u16(b, gps, 1);
        let e0 = gps + 2;
        t.put_u16(b, e0, 0x0000); // GPSVersionID
        t.put_u16(b, e0 + 2, 1); // BYTE
        t.put_u32(b, e0 + 4, 4);
        b[e0 + 8..e0 + 12].copy_from_slice(&[2, 2, 0, 0]);
        b[e0 + 12..e0 + 16].fill(0); // no next IFD
        Some(())
    })()
    .is_some()
}

/// Drops IFD1, the embedded thumbnail (stale once the picture is rotated or resized): its
/// bytes and entries are zeroed and IFD0's next pointer is set to 0. Answers whether there
/// was one.
pub fn drop_thumbnail(b: &mut [u8]) -> bool {
    (|| {
        let t = Tiff::new(b)?;
        let (_, next0) = entries(t, b, ifd0(t, b)?)?;
        let ifd1 = t.u32(b, next0)? as usize;
        if ifd1 < FIRST_IFD || ifd1 == ifd0(t, b)? {
            return None;
        }
        let (list, next1) = entries(t, b, ifd1)?;
        let value = |tag: u16| list.iter().find(|e| e.tag == tag).and_then(|e| t.u32(b, e.at + 8));
        if let (Some(off), Some(len)) = (value(0x0201), value(0x0202)) {
            let start = off as usize;
            if let Some(data) = start.checked_add(len as usize).and_then(|end| b.get_mut(start..end)) {
                data.fill(0);
            }
        }
        for e in list {
            wipe_entry(t, b, e);
        }
        b[ifd1..ifd1 + 2].fill(0);
        b[next1..next1 + 4].fill(0);
        b[next0..next0 + 4].fill(0);
        Some(())
    })()
    .is_some()
}

/// Sets the Exif IFD's PixelXDimension and PixelYDimension (SHORT or LONG, always inline; a
/// SHORT keeps at most 65535). Answers whether there was an Exif IFD.
pub fn set_pixel_dims(b: &mut [u8], w: u32, h: u32) -> bool {
    (|| {
        let t = Tiff::new(b)?;
        let exif_ifd = sub_ifd(t, b, 0x8769)?;
        let (list, _) = entries(t, b, exif_ifd)?;
        for e in list {
            let v = match e.tag {
                0xA002 => w,
                0xA003 => h,
                _ => continue,
            };
            match e.typ {
                3 => t.put_u16(b, e.at + 8, v.min(u32::from(u16::MAX)) as u16),
                4 => t.put_u32(b, e.at + 8, v),
                _ => {}
            }
        }
        Some(())
    })()
    .is_some()
}

/// The start of XMP's APP1 payload, and of the extended XMP that continues a long one.
const XMP: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
const XMP_EXTENSION: &[u8] = b"http://ns.adobe.com/xmp/extension/\0";
const EXIF: &[u8] = b"Exif\0\0";

/// One marker segment before the image data: its marker, its whole range (`FF xx`, the
/// length and the payload) and its payload's.
struct Segment {
    marker: u8,
    range: Range<usize>,
    body: Range<usize>,
}

/// The marker segments of a JPEG up to (not including) the start of scan, and where the
/// start of scan is; `None` when `jpeg` is not a JPEG this can walk.
fn segments(jpeg: &[u8]) -> Option<(Vec<Segment>, usize)> {
    if jpeg.get(0..2)? != [0xFF, 0xD8] {
        return None;
    }
    let mut list = Vec::new();
    let mut p = 2;
    loop {
        if *jpeg.get(p)? != 0xFF {
            return None;
        }
        // Fill bytes: any number of 0xFF before a marker.
        let mut m = p + 1;
        while *jpeg.get(m)? == 0xFF {
            m += 1;
        }
        let marker = jpeg[m];
        match marker {
            0xDA => return Some((list, p)),
            // EOI before any image data, or a marker that does not belong here.
            0xD9 | 0x00 | 0xD8 => return None,
            // Markers without a length.
            0x01 | 0xD0..=0xD7 => p = m + 1,
            _ => {
                let len = usize::from(u16::from_be_bytes([*jpeg.get(m + 1)?, *jpeg.get(m + 2)?]));
                if len < 2 {
                    return None;
                }
                let end = m + 1 + len;
                jpeg.get(..end)?;
                list.push(Segment { marker, range: p..end, body: m + 3..end });
                p = end;
            }
        }
    }
}

/// The TIFF payload of an `Exif\0\0` APP1 segment.
fn exif_payload(jpeg: &[u8], segment: &Segment) -> Option<Range<usize>> {
    (segment.marker == 0xE1 && jpeg[segment.body.clone()].starts_with(EXIF))
        .then(|| segment.body.start + EXIF.len()..segment.body.end)
}

/// Where the TIFF payload of the first `Exif\0\0` APP1 segment is in a JPEG file, so that it
/// can be edited in place; `None` without one.
pub fn jpeg_exif_range(jpeg: &[u8]) -> Option<Range<usize>> {
    let (list, _) = segments(jpeg)?;
    list.iter().find_map(|s| exif_payload(jpeg, s))
}

/// Whether a segment is left out of a JPEG without its location: XMP (and the extended
/// XMP that continues a long one), Photoshop's APP13 (IPTC, which names the city and the
/// place), and the MPF index of the secondary pictures, which are left out too.
fn left_out(segment: &Segment, jpeg: &[u8]) -> bool {
    let body = &jpeg[segment.body.clone()];
    match segment.marker {
        0xE1 => body.starts_with(XMP) || body.starts_with(XMP_EXTENSION),
        0xE2 => body.starts_with(b"MPF\0"),
        0xED => true,
        _ => false,
    }
}

/// Where the main picture ends (after its EOI), given where its first scan starts; the end
/// of the file when it has no EOI. What comes after is a trailer: MPF's secondary pictures
/// (Ultra HDR's gain map, a camera's preview), a motion photo's video. `None` (fail closed)
/// when something between the scans could hold metadata that is not looked at (an APPn
/// segment), or when another picture starts before this one ended (SOI, or the TEM marker
/// that has no place there): a truncated main picture must not pass on what follows.
fn picture_end(jpeg: &[u8], scan: usize) -> Option<usize> {
    let mut p = scan;
    while p + 1 < jpeg.len() {
        if jpeg[p] != 0xFF {
            p += 1;
            continue;
        }
        match jpeg[p + 1] {
            0xD9 => return Some(p + 2),
            // Stuffed 0xFF, a restart marker, or a fill byte: still image data.
            0x00 | 0xD0..=0xD7 | 0xFF => p += 1,
            0xD8 | 0x01 | 0xE0..=0xEF => return None,
            // A segment between scans (SOS, DHT…): skip it; the data after it is scanned on.
            _ => match jpeg.get(p + 2..p + 4) {
                Some(len) => p += 2 + usize::from(u16::from_be_bytes([len[0], len[1]])),
                None => break,
            },
        }
    }
    Some(jpeg.len())
}

/// A JPEG file without its location, the picture itself untouched (no re-encode): the GPS
/// data in each EXIF block is removed in place ([`strip_gps`]); XMP and IPTC (APP13), which
/// can hold the location too, are left out; so is everything after the main picture (MPF
/// secondary pictures with their own EXIF, a motion photo's video, which can carry a
/// location of its own, and whose XMP description is gone anyway). Everything else is
/// copied byte for byte. `None` when `jpeg` is not a JPEG this can read, or has an EXIF
/// block whose location cannot be removed this way (then it has to be re-encoded).
pub fn jpeg_without_location(jpeg: &[u8]) -> Option<Vec<u8>> {
    let (list, scan) = segments(jpeg)?;
    let end = picture_end(jpeg, scan)?;
    let mut out = Vec::with_capacity(end);
    out.extend_from_slice(&jpeg[..2]);
    for segment in list.iter().filter(|s| !left_out(s, jpeg)) {
        let at = out.len();
        out.extend_from_slice(&jpeg[segment.range.clone()]);
        if let Some(tiff) = exif_payload(jpeg, segment) {
            let start = at + (tiff.start - segment.range.start);
            let block = &mut out[start..start + tiff.len()];
            // Fail closed: a block whose GPS cannot be found or removed is not passed on.
            if !has_gps(block)? {
                continue;
            }
            if !strip_gps(block) {
                return None;
            }
        }
    }
    out.extend_from_slice(&jpeg[scan..end]);
    Some(out)
}
