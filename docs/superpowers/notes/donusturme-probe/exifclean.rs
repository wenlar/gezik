//! In-place EXIF (TIFF payload) edits that never move bytes, so every offset stays valid:
//! remove the GPS IFD, drop the IFD1 thumbnail, update PixelX/YDimension.
//! Input is the raw TIFF payload as `ImageDecoder::exif_metadata()` returns it
//! (`II*\0...` / `MM\0*...`, no `Exif\0\0` prefix).

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
        let s: [u8; 2] = b.get(at..at + 2)?.try_into().ok()?;
        Some(if self.le { u16::from_le_bytes(s) } else { u16::from_be_bytes(s) })
    }
    fn u32(self, b: &[u8], at: usize) -> Option<u32> {
        let s: [u8; 4] = b.get(at..at + 4)?.try_into().ok()?;
        Some(if self.le { u32::from_le_bytes(s) } else { u32::from_be_bytes(s) })
    }
    fn put_u16(self, b: &mut [u8], at: usize, v: u16) {
        b[at..at + 2].copy_from_slice(&if self.le { v.to_le_bytes() } else { v.to_be_bytes() });
    }
    fn put_u32(self, b: &mut [u8], at: usize, v: u32) {
        b[at..at + 4].copy_from_slice(&if self.le { v.to_le_bytes() } else { v.to_be_bytes() });
    }
}

fn type_size(t: u16) -> usize {
    match t {
        1 | 2 | 6 | 7 => 1,
        3 | 8 => 2,
        4 | 9 | 11 => 4,
        5 | 10 | 12 => 8,
        _ => 0,
    }
}

/// (entry offset, tag, type, count) for each entry of the IFD at `ifd`; also the next-IFD field offset.
fn entries(t: Tiff, b: &[u8], ifd: usize) -> Option<(Vec<(usize, u16, u16, u32)>, usize)> {
    let n = t.u16(b, ifd)? as usize;
    let mut v = Vec::with_capacity(n);
    for i in 0..n {
        let e = ifd + 2 + i * 12;
        v.push((e, t.u16(b, e)?, t.u16(b, e + 2)?, t.u32(b, e + 4)?));
    }
    let next = ifd + 2 + n * 12;
    b.get(next..next + 4)?;
    Some((v, next))
}

/// Zero an entry's out-of-line data (if any) and its whole 12-byte record.
fn wipe_entry(t: Tiff, b: &mut [u8], e: usize, typ: u16, count: u32) {
    let size = type_size(typ).saturating_mul(count as usize);
    if size > 4 {
        if let Some(off) = t.u32(b, e + 8) {
            let (s, end) = (off as usize, (off as usize).saturating_add(size));
            if end <= b.len() {
                b[s..end].fill(0);
            }
        }
    }
    b[e..e + 12].fill(0);
}

fn ifd0(t: Tiff, b: &[u8]) -> Option<usize> {
    Some(t.u32(b, 4)? as usize)
}

/// Removes all location data: the GPS IFD keeps one entry, GPSVersionID = 2.2.0.0, and every
/// other GPS value (and its out-of-line data) is zeroed. Returns true if a GPS IFD was found.
pub fn strip_gps(b: &mut [u8]) -> bool {
    (|| {
        let t = Tiff::new(b)?;
        let (ents, _) = entries(t, b, ifd0(t, b)?)?;
        let &(e, _, _, _) = ents.iter().find(|x| x.1 == 0x8825)?;
        let gps = t.u32(b, e + 8)? as usize;
        let (gents, next) = entries(t, b, gps)?;
        for (ge, _, typ, count) in gents {
            wipe_entry(t, b, ge, typ, count);
        }
        b[next..next + 4].fill(0);
        // One harmless entry so readers that dislike empty IFDs stay happy.
        t.put_u16(b, gps, 1);
        let e0 = gps + 2;
        t.put_u16(b, e0, 0x0000); // GPSVersionID
        t.put_u16(b, e0 + 2, 1); // BYTE
        t.put_u32(b, e0 + 4, 4);
        b[e0 + 8..e0 + 12].copy_from_slice(&[2, 2, 0, 0]);
        if b.get(e0 + 12..e0 + 16).is_some() {
            b[e0 + 12..e0 + 16].fill(0); // next IFD = 0 (inside the wiped area)
        }
        Some(())
    })()
    .is_some()
}

/// Drops IFD1 (the embedded thumbnail, stale after rotate/resize): zeroes the thumbnail bytes and
/// IFD1, and sets IFD0's next pointer to 0.
pub fn drop_thumbnail(b: &mut [u8]) -> bool {
    (|| {
        let t = Tiff::new(b)?;
        let (_, next0) = entries(t, b, ifd0(t, b)?)?;
        let ifd1 = t.u32(b, next0)? as usize;
        if ifd1 == 0 {
            return None;
        }
        let (ents, next1) = entries(t, b, ifd1)?;
        let off = ents.iter().find(|x| x.1 == 0x0201).and_then(|x| t.u32(b, x.0 + 8));
        let len = ents.iter().find(|x| x.1 == 0x0202).and_then(|x| t.u32(b, x.0 + 8));
        if let (Some(o), Some(l)) = (off, len) {
            let (s, e) = (o as usize, o as usize + l as usize);
            if e <= b.len() {
                b[s..e].fill(0);
            }
        }
        for (e, _, typ, count) in ents {
            wipe_entry(t, b, e, typ, count);
        }
        b[ifd1..ifd1 + 2].fill(0);
        b[next1..next1 + 4].fill(0);
        b[next0..next0 + 4].fill(0);
        Some(())
    })()
    .is_some()
}

/// Updates ExifIFD PixelXDimension/PixelYDimension (SHORT or LONG, always inline).
pub fn set_pixel_dims(b: &mut [u8], w: u32, h: u32) -> bool {
    (|| {
        let t = Tiff::new(b)?;
        let (ents, _) = entries(t, b, ifd0(t, b)?)?;
        let &(e, _, _, _) = ents.iter().find(|x| x.1 == 0x8769)?;
        let exif_ifd = t.u32(b, e + 8)? as usize;
        let (xents, _) = entries(t, b, exif_ifd)?;
        for (xe, tag, typ, _) in xents {
            let v = match tag {
                0xA002 => w,
                0xA003 => h,
                _ => continue,
            };
            match typ {
                3 => t.put_u16(b, xe + 8, v.min(u16::MAX as u32) as u16),
                4 => t.put_u32(b, xe + 8, v),
                _ => {}
            }
        }
        Some(())
    })()
    .is_some()
}

/// Finds the APP1 "Exif\0\0" payload range inside a JPEG file, so the TIFF can be patched in
/// place (same length) without re-encoding the image.
pub fn jpeg_exif_range(jpeg: &[u8]) -> Option<std::ops::Range<usize>> {
    if jpeg.get(0..2)? != [0xFF, 0xD8] {
        return None;
    }
    let mut p = 2;
    loop {
        if *jpeg.get(p)? != 0xFF {
            return None;
        }
        let m = *jpeg.get(p + 1)?;
        if m == 0xD9 || m == 0xDA {
            return None; // EOI / start of scan: no more metadata segments
        }
        let len = u16::from_be_bytes([*jpeg.get(p + 2)?, *jpeg.get(p + 3)?]) as usize;
        let body = p + 4..p + 2 + len;
        if m == 0xE1 && jpeg.get(body.start..body.start + 6)? == b"Exif\0\0" {
            return Some(body.start + 6..body.end);
        }
        p += 2 + len;
    }
}
