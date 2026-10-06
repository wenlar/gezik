//! Reads what a PDF needs from a JPEG's markers, without decoding a pixel.
use image::metadata::Orientation;

#[derive(Debug)]
pub struct JpegInfo {
    pub width: u32,
    pub height: u32,
    /// 1 = gray, 3 = YCbCr/RGB, 4 = CMYK/YCCK.
    pub components: u8,
    pub bits: u8,
    /// SOF marker: 0xC0 baseline, 0xC1 extended, 0xC2 progressive; others (lossless,
    /// arithmetic, hierarchical) are refused for passthrough.
    pub sof: u8,
    /// Adobe APP14 present: CMYK data is stored inverted (Photoshop convention).
    pub adobe: Option<u8>,
    pub orientation: Orientation,
    /// ICC profile reassembled from APP2 `ICC_PROFILE` chunks.
    pub icc: Option<Vec<u8>>,
    /// JFIF density as dots per inch, if the file states one.
    pub dpi: Option<(f32, f32)>,
}

impl JpegInfo {
    /// DCTDecode in PDF readers handles 8-bit baseline/extended/progressive Huffman JPEGs.
    pub fn passthrough_ok(&self) -> bool {
        self.bits == 8 && matches!(self.sof, 0xC0..=0xC2) && matches!(self.components, 1 | 3 | 4)
    }
}

/// Reads the markers up to the first scan; `None` for what is not a JPEG or states no size.
pub fn parse(b: &[u8]) -> Option<JpegInfo> {
    if b.len() < 4 || b[0] != 0xFF || b[1] != 0xD8 {
        return None;
    }
    let mut i = 2;
    let mut info = JpegInfo {
        width: 0,
        height: 0,
        components: 0,
        bits: 0,
        sof: 0,
        adobe: None,
        orientation: Orientation::NoTransforms,
        icc: None,
        dpi: None,
    };
    let mut icc_chunks: Vec<(u8, &[u8])> = Vec::new();
    loop {
        // Skip fill bytes.
        while i < b.len() && b[i] == 0xFF && b.get(i + 1) == Some(&0xFF) {
            i += 1;
        }
        if i + 4 > b.len() || b[i] != 0xFF {
            return None;
        }
        let m = b[i + 1];
        if m == 0xDA || m == 0xD9 {
            break; // SOS: headers are done
        }
        if (0xD0..=0xD7).contains(&m) || m == 0x01 {
            i += 2;
            continue;
        }
        let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
        if len < 2 || i + 2 + len > b.len() {
            return None;
        }
        let seg = &b[i + 4..i + 2 + len];
        match m {
            0xC0..=0xCF if m != 0xC4 && m != 0xC8 && m != 0xCC => {
                if seg.len() < 6 {
                    return None;
                }
                info.sof = m;
                info.bits = seg[0];
                info.height = u16::from_be_bytes([seg[1], seg[2]]) as u32;
                info.width = u16::from_be_bytes([seg[3], seg[4]]) as u32;
                info.components = seg[5];
            }
            0xE0 if seg.starts_with(b"JFIF\0") && seg.len() >= 12 => {
                let units = seg[7];
                let x = u16::from_be_bytes([seg[8], seg[9]]) as f32;
                let y = u16::from_be_bytes([seg[10], seg[11]]) as f32;
                info.dpi = match units {
                    1 if x > 0.0 && y > 0.0 => Some((x, y)),
                    2 if x > 0.0 && y > 0.0 => Some((x * 2.54, y * 2.54)),
                    _ => None,
                };
            }
            0xE1 if seg.starts_with(b"Exif\0\0") => {
                if let Some(o) = Orientation::from_exif_chunk(&seg[6..]) {
                    info.orientation = o;
                }
            }
            0xE2 if seg.starts_with(b"ICC_PROFILE\0") && seg.len() > 14 => {
                icc_chunks.push((seg[12], &seg[14..]));
            }
            0xEE if seg.starts_with(b"Adobe") && seg.len() >= 12 => {
                info.adobe = Some(seg[11]);
            }
            _ => {}
        }
        i += 2 + len;
    }
    if !icc_chunks.is_empty() {
        icc_chunks.sort_by_key(|c| c.0);
        info.icc = Some(icc_chunks.iter().flat_map(|c| c.1.iter().copied()).collect());
    }
    (info.width > 0 && info.height > 0).then_some(info)
}

/// Whether an ICC profile is for `components` colour channels: bytes 16-19 of its header
/// name the colour space ("GRAY" 1, "RGB " 3, "CMYK" 4). A profile that does not fit the
/// samples is left out (the samples then go as Device grey, RGB or CMYK).
pub fn icc_fits(icc: &[u8], components: u8) -> bool {
    let space: &[u8] = match components {
        1 => b"GRAY",
        3 => b"RGB ",
        4 => b"CMYK",
        _ => return false,
    };
    icc.get(16..20) == Some(space)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A JPEG header: SOI, the segments, then SOS (no picture data: parsing stops there).
    fn header(segments: &[(u8, Vec<u8>)]) -> Vec<u8> {
        let mut b = vec![0xFF, 0xD8];
        for (marker, payload) in segments {
            b.extend_from_slice(&[0xFF, *marker]);
            b.extend_from_slice(&(payload.len() as u16 + 2).to_be_bytes());
            b.extend_from_slice(payload);
        }
        b.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x08, 1, 1, 0, 0, 0x3F, 0]);
        b
    }

    fn sof(marker: u8, bits: u8, components: u8) -> (u8, Vec<u8>) {
        let mut payload = vec![bits, 0, 48, 0, 64, components];
        for c in 0..components {
            payload.extend_from_slice(&[c + 1, 0x11, 0]);
        }
        (marker, payload)
    }

    fn info(segments: &[(u8, Vec<u8>)]) -> JpegInfo {
        parse(&header(segments)).expect("parses")
    }

    #[test]
    fn baseline_extended_and_progressive_pass_through() {
        for marker in [0xC0, 0xC1, 0xC2] {
            let i = info(&[sof(marker, 8, 3)]);
            assert_eq!((i.width, i.height, i.components, i.bits), (64, 48, 3, 8));
            assert!(i.passthrough_ok(), "{marker:#x}");
        }
        assert!(info(&[sof(0xC0, 8, 1)]).passthrough_ok());
        assert!(info(&[sof(0xC2, 8, 4)]).passthrough_ok());
    }

    #[test]
    fn lossless_arithmetic_and_twelve_bit_jpegs_are_decoded_instead() {
        assert!(!info(&[sof(0xC3, 8, 3)]).passthrough_ok(), "SOF3 lossless");
        assert!(!info(&[sof(0xC9, 8, 3)]).passthrough_ok(), "SOF9 arithmetic");
        assert!(!info(&[sof(0xC1, 12, 3)]).passthrough_ok(), "12-bit extended");
        assert!(!info(&[sof(0xC0, 8, 2)]).passthrough_ok(), "two components");
    }

    #[test]
    fn icc_chunks_join_in_their_sequence_order() {
        let chunk = |seq: u8, data: &[u8]| {
            let mut p = b"ICC_PROFILE\0".to_vec();
            p.extend_from_slice(&[seq, 2]);
            p.extend_from_slice(data);
            (0xE2, p)
        };
        let i = info(&[chunk(2, b"world"), chunk(1, b"hello"), sof(0xC0, 8, 3)]);
        assert_eq!(i.icc.as_deref(), Some(&b"helloworld"[..]));
    }

    #[test]
    fn jfif_density_in_centimetres_becomes_dots_per_inch() {
        let jfif = |units: u8, x: u16, y: u16| {
            let mut p = b"JFIF\0".to_vec();
            p.extend_from_slice(&[1, 2, units]);
            p.extend_from_slice(&x.to_be_bytes());
            p.extend_from_slice(&y.to_be_bytes());
            p.extend_from_slice(&[0, 0]);
            (0xE0, p)
        };
        let (x, y) = info(&[jfif(2, 100, 200), sof(0xC0, 8, 3)]).dpi.unwrap();
        assert!((x - 254.0).abs() < 1e-3 && (y - 508.0).abs() < 1e-3, "{x} {y}");
        assert_eq!(info(&[jfif(1, 300, 300), sof(0xC0, 8, 3)]).dpi, Some((300.0, 300.0)));
        // Units 0 is an aspect ratio, and a zero density says nothing.
        assert_eq!(info(&[jfif(0, 1, 1), sof(0xC0, 8, 3)]).dpi, None);
        assert_eq!(info(&[jfif(1, 0, 72), sof(0xC0, 8, 3)]).dpi, None);
    }

    #[test]
    fn adobe_segment_and_broken_headers() {
        let mut adobe = b"Adobe".to_vec();
        adobe.extend_from_slice(&[0, 100, 0, 0, 0, 0, 2]);
        assert_eq!(info(&[(0xEE, adobe), sof(0xC0, 8, 4)]).adobe, Some(2));
        assert!(parse(b"not a jpeg").is_none());
        assert!(parse(&header(&[])).is_none(), "no SOF: no size");
        let mut cut = header(&[sof(0xC0, 8, 3)]);
        cut.truncate(8);
        assert!(parse(&cut).is_none(), "a segment past the end");
    }

    #[test]
    fn icc_fits_by_the_header_colour_space() {
        let profile = |space: &[u8; 4]| {
            let mut p = vec![0u8; 128];
            p[16..20].copy_from_slice(space);
            p
        };
        assert!(icc_fits(&profile(b"GRAY"), 1));
        assert!(icc_fits(&profile(b"RGB "), 3));
        assert!(icc_fits(&profile(b"CMYK"), 4));
        assert!(!icc_fits(&profile(b"RGB "), 1));
        assert!(!icc_fits(&profile(b"GRAY"), 3));
        assert!(!icc_fits(&profile(b"RGB "), 4));
        assert!(!icc_fits(&profile(b"CMYK"), 3));
        assert!(!icc_fits(&profile(b"RGB "), 2));
        assert!(!icc_fits(b"short", 3));
    }
}
