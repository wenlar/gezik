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
        self.bits == 8 && matches!(self.sof, 0xC0 | 0xC1 | 0xC2) && matches!(self.components, 1 | 3 | 4)
    }
}

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
