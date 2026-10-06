//! Images to PDF with pdf-writer and miniz_oxide, streamed to disk one object at a time.
//!
//! pdf-writer's `Pdf` keeps the whole file in one `Vec<u8>` until `finish()`. For 100 photos
//! that is the sum of all the JPEGs in memory, so each indirect object goes into its own
//! `Chunk`, is written to the file at once, and only its offset is kept for the xref table.
//!
//! A JPEG that PDF readers take (8-bit baseline, extended or progressive Huffman) goes in byte
//! for byte as DCTDecode, turned upright by a matrix rather than by re-encoding. Everything
//! else is decoded (with 5c's limits), turned upright, and written as Flate-compressed samples
//! (PNG Paeth predictor), its alpha as a soft mask.

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

use ::image::DynamicImage;
use ::image::metadata::Orientation;
use gezik_core::batch::pdf::{PageOptions, orient_matrix, place, shown_size};
use miniz_oxide::deflate::compress_to_vec_zlib;
use pdf_writer::types::Predictor;
use pdf_writer::{Chunk, Content, Filter, Finish, Name, Rect, Ref, TextStr};

use super::jpeg::{self, JpegInfo};
use crate::convert::image::{MAX_BUFFER, decode, fits, starts_like_jpeg, too_large};

/// The miniz_oxide level for samples and ICC profiles: with the Paeth predictor, level 1 is
/// within 2% of level 9 at a fifth of the time.
pub const FLATE_LEVEL: u8 = 1;

/// What [`write_pdf`] made.
#[derive(Debug)]
pub struct PicturesPdf {
    pub pages: usize,
    /// The pictures that could not be read, with why.
    pub left_out: Vec<(PathBuf, io::Error)>,
}

/// Writes `pictures` (in this order, one page each) as a PDF at `out`, a name nothing has yet
/// (it is made new, never written over). Each picture is read alone; `on_picture(path, its
/// size)` after each page; `stop` between pictures (then `Interrupted`). A picture that cannot
/// be read is left out (in `left_out`); with none left, an error. Whatever ends it early, an
/// error or a panic, the file it made is removed.
pub fn write_pdf(
    pictures: &[PathBuf],
    out: &Path,
    options: &PageOptions,
    on_picture: &mut dyn FnMut(&Path, u64),
    stop: &dyn Fn() -> bool,
) -> io::Result<PicturesPdf> {
    if pictures.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "no pictures"));
    }
    let file = File::options().write(true).create_new(true).open(out)?;
    // Made before the writer, so dropped after it: the file is closed by then.
    let mut made = Unfinished(Some(out));
    let mut pdf = PdfFile::new(file)?;
    let mut left_out = Vec::new();
    for path in pictures {
        if stop() {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
        }
        let (picture, size) = match read(path) {
            Ok(read) => read,
            Err(err) => {
                left_out.push((path.clone(), err));
                continue;
            }
        };
        pdf.add_page(picture, options)?;
        on_picture(path, size);
    }
    let pages = pdf.pages.len();
    if pages == 0 {
        drop(pdf);
        let (_, first) = left_out.swap_remove(0);
        return Err(first);
    }
    pdf.finish()?;
    made.0 = None;
    Ok(PicturesPdf { pages, left_out })
}

/// Removes the file it names when dropped (on an early return or a panic), unless cleared.
struct Unfinished<'a>(Option<&'a Path>);

impl Drop for Unfinished<'_> {
    fn drop(&mut self) {
        if let Some(path) = self.0 {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// A picture ready to be written: everything that can fail on its account has happened.
enum Picture {
    /// The JPEG's own bytes.
    Jpeg { data: Vec<u8>, info: JpegInfo },
    /// Upright samples, compressed.
    Samples {
        width: u32,
        height: u32,
        /// 1 (grey) or 3 (RGB).
        colors: i32,
        bits: i32,
        /// Paeth-filtered, then Flate.
        color: Vec<u8>,
        /// Flate; `None` when every pixel is opaque.
        alpha: Option<Vec<u8>>,
        icc: Option<Vec<u8>>,
    },
}

/// Reads one picture and its size in bytes.
fn read(path: &Path) -> io::Result<(Picture, u64)> {
    let size = std::fs::metadata(path)?.len();
    if starts_like_jpeg(path)? {
        // A JPEG is read whole to go in as it is; one over 512 MiB is no photo.
        if size > MAX_BUFFER {
            return Err(too_large());
        }
        let data = std::fs::read(path)?;
        // One a decoder could not hold (see `fits`) is no photo either: the decode path says
        // it is too large, instead of a PDF viewers may choke on.
        if let Some(info) = jpeg::parse(&data).filter(|i| i.passthrough_ok() && fits(i.width, i.height, 4)) {
            return Ok((Picture::Jpeg { data, info }, size));
        }
    }
    Ok((samples(path)?, size))
}

/// Decodes a picture (5c's reader and limits), turns it upright and compresses its samples.
fn samples(path: &Path) -> io::Result<Picture> {
    let source = decode(path)?;
    let mut img = source.img;
    img.apply_orientation(source.orientation);
    let (w, h) = (img.width(), img.height());
    if w == 0 || h == 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "the picture is damaged (no pixels)"));
    }
    let ct = img.color();
    let sixteen = ct.bytes_per_pixel() / ct.channel_count() == 2;
    let grey = !ct.has_color();
    let has_alpha = ct.has_alpha();
    let (color, alpha) = planes(img, grey, sixteen, has_alpha);
    let bits = if sixteen { 16 } else { 8 };
    let colors = if grey { 1 } else { 3 };
    let bpp = colors as usize * bits as usize / 8;
    let color = compress_to_vec_zlib(&paeth(&color, w as usize * bpp, bpp), FLATE_LEVEL);
    let alpha = alpha.map(|a| compress_to_vec_zlib(&a, FLATE_LEVEL));
    let icc = source.icc.filter(|icc| jpeg::icc_fits(icc, colors as u8));
    Ok(Picture::Samples { width: w, height: h, colors, bits, color, alpha, icc })
}

/// The colour samples and the alpha samples (if not all opaque), 16-bit ones big-endian.
fn planes(img: DynamicImage, grey: bool, sixteen: bool, has_alpha: bool) -> (Vec<u8>, Option<Vec<u8>>) {
    match (grey, sixteen, has_alpha) {
        (true, false, false) => (img.into_luma8().into_raw(), None),
        (true, false, true) => split(img.into_luma_alpha8().as_raw(), 2, 1, true),
        (false, false, false) => (img.into_rgb8().into_raw(), None),
        (false, false, true) => split(img.into_rgba8().as_raw(), 4, 3, true),
        (true, true, false) => (be16(img.into_luma16().as_raw()), None),
        (true, true, true) => split(&be16(img.into_luma_alpha16().as_raw()), 4, 2, true),
        (false, true, false) => (be16(img.into_rgb16().as_raw()), None),
        (false, true, true) => split(&be16(img.into_rgba16().as_raw()), 8, 6, true),
    }
}

/// The EXIF number (1-8) of an orientation.
fn exif_number(o: Orientation) -> u8 {
    match o {
        Orientation::NoTransforms => 1,
        Orientation::FlipHorizontal => 2,
        Orientation::Rotate180 => 3,
        Orientation::FlipVertical => 4,
        Orientation::Rotate90FlipH => 5,
        Orientation::Rotate90 => 6,
        Orientation::Rotate270FlipH => 7,
        Orientation::Rotate270 => 8,
    }
}

/// The density of the picture as shown: orientations 5-8 turn it a quarter, so the stored
/// rows' density becomes the shown columns'.
fn upright_dpi(dpi: Option<(f32, f32)>, exif_orientation: u8) -> Option<(f32, f32)> {
    dpi.map(|(x, y)| if (5..=8).contains(&exif_orientation) { (y, x) } else { (x, y) })
}

const IDENTITY: [f32; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// A PDF written as it is made: each object goes to the file at once, and only its offset
/// is kept.
struct PdfFile {
    out: BufWriter<File>,
    pos: u64,
    /// By object number; 0 for a number never written (it becomes a free xref entry).
    offsets: Vec<u64>,
    next: i32,
    catalog: Ref,
    tree: Ref,
    pages: Vec<Ref>,
}

impl PdfFile {
    fn new(file: File) -> io::Result<Self> {
        let mut out = BufWriter::with_capacity(1 << 20, file);
        // Same header as pdf-writer's Pdf::new (binary marker on line 2).
        let header = b"%PDF-1.7\n%\x80\x80\x80\x80\n\n";
        out.write_all(header)?;
        Ok(Self {
            out,
            pos: header.len() as u64,
            offsets: Vec::new(),
            next: 3,
            catalog: Ref::new(1),
            tree: Ref::new(2),
            pages: Vec::new(),
        })
    }

    fn alloc(&mut self) -> Ref {
        let r = Ref::new(self.next);
        self.next += 1;
        r
    }

    /// Writes one chunk that holds exactly one indirect object.
    fn put(&mut self, id: Ref, chunk: Chunk) -> io::Result<()> {
        let index = id.get() as usize;
        if self.offsets.len() <= index {
            self.offsets.resize(index + 1, 0);
        }
        self.offsets[index] = self.pos;
        let bytes = chunk.as_bytes();
        self.out.write_all(bytes)?;
        self.pos += bytes.len() as u64;
        Ok(())
    }

    /// Adds one page holding `picture`; the numbers are taken only now that it has been read.
    fn add_page(&mut self, picture: Picture, options: &PageOptions) -> io::Result<()> {
        let img_ref = self.alloc();
        let (width, height, orientation, dpi) = match picture {
            Picture::Jpeg { data, info } => {
                let icc = info.icc.as_deref().filter(|icc| jpeg::icc_fits(icc, info.components));
                let icc_ref = icc.map(|_| self.alloc());
                let mut c = Chunk::new();
                let mut x = c.image_xobject(img_ref, &data);
                x.filter(Filter::DctDecode);
                x.width(info.width as i32);
                x.height(info.height as i32);
                x.bits_per_component(8);
                match (icc_ref, info.components) {
                    (Some(r), _) => x.color_space().icc_based(r),
                    (None, 1) => x.color_space().device_gray(),
                    (None, 3) => x.color_space().device_rgb(),
                    (None, _) => x.color_space().device_cmyk(),
                }
                if info.components == 4 && info.adobe.is_some() {
                    // Adobe/Photoshop CMYK JPEGs store inverted ink values.
                    x.decode([1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0]);
                }
                x.finish();
                self.put(img_ref, c)?;
                drop(data);
                if let (Some(r), Some(icc)) = (icc_ref, icc) {
                    self.put_icc(r, icc, i32::from(info.components))?;
                }
                let orientation = exif_number(info.orientation);
                let dpi = upright_dpi(info.dpi.filter(|&(x, y)| x > 0.0 && y > 0.0), orientation);
                (info.width, info.height, orientation, dpi)
            }
            Picture::Samples { width, height, colors, bits, color, alpha, icc } => {
                let icc_ref = icc.as_ref().map(|_| self.alloc());
                let mask_ref = alpha.as_ref().map(|_| self.alloc());
                let mut c = Chunk::new();
                let mut x = c.image_xobject(img_ref, &color);
                x.filter(Filter::FlateDecode);
                x.decode_parms()
                    .predictor(Predictor::PngOptimum)
                    .colors(colors)
                    .bits_per_component(bits)
                    .columns(width as i32);
                x.width(width as i32);
                x.height(height as i32);
                x.bits_per_component(bits);
                match (icc_ref, colors) {
                    (Some(r), _) => x.color_space().icc_based(r),
                    (None, 1) => x.color_space().device_gray(),
                    (None, _) => x.color_space().device_rgb(),
                }
                if let Some(m) = mask_ref {
                    x.s_mask(m);
                }
                x.finish();
                self.put(img_ref, c)?;
                drop(color);
                if let (Some(m), Some(a)) = (mask_ref, &alpha) {
                    let mut c = Chunk::new();
                    let mut s = c.image_xobject(m, a);
                    s.filter(Filter::FlateDecode);
                    s.width(width as i32);
                    s.height(height as i32);
                    s.color_space().device_gray();
                    s.bits_per_component(bits);
                    s.finish();
                    self.put(m, c)?;
                }
                if let (Some(r), Some(icc)) = (icc_ref, &icc) {
                    self.put_icc(r, icc, colors)?;
                }
                // Already upright; the pixels carry no density worth trusting.
                (width, height, 1, None)
            }
        };

        let (shown_w, shown_h) = shown_size(width, height, orientation);
        let at = place(shown_w, shown_h, dpi, options);
        let page_ref = self.alloc();
        let content_ref = self.alloc();
        let mut content = Content::new();
        content.save_state();
        content.transform([at.width, 0.0, 0.0, at.height, at.x, at.y]);
        let o = orient_matrix(orientation);
        if o != IDENTITY {
            content.transform(o);
        }
        content.x_object(Name(b"Im0"));
        content.restore_state();
        let content = content.finish();
        let mut c = Chunk::new();
        c.stream(content_ref, &content);
        self.put(content_ref, c)?;

        let mut c = Chunk::new();
        let mut page = c.page(page_ref);
        page.media_box(Rect::new(0.0, 0.0, at.page_width, at.page_height));
        page.parent(self.tree);
        page.contents(content_ref);
        page.resources().x_objects().pair(Name(b"Im0"), img_ref);
        page.finish();
        self.put(page_ref, c)?;
        self.pages.push(page_ref);
        Ok(())
    }

    fn put_icc(&mut self, r: Ref, icc: &[u8], n: i32) -> io::Result<()> {
        let z = compress_to_vec_zlib(icc, FLATE_LEVEL);
        let mut c = Chunk::new();
        let mut p = c.icc_profile(r, &z);
        p.n(n);
        p.filter(Filter::FlateDecode);
        match n {
            1 => p.alternate().device_gray(),
            3 => p.alternate().device_rgb(),
            _ => p.alternate().device_cmyk(),
        }
        p.finish();
        self.put(r, c)
    }

    /// Page tree, catalog, info, xref table and trailer.
    fn finish(mut self) -> io::Result<()> {
        let mut c = Chunk::new();
        c.pages(self.tree).kids(self.pages.iter().copied()).count(self.pages.len() as i32);
        self.put(self.tree, c)?;
        let mut c = Chunk::new();
        c.indirect(self.catalog).start::<pdf_writer::writers::Catalog>().pages(self.tree);
        self.put(self.catalog, c)?;
        let info = self.alloc();
        let mut c = Chunk::new();
        c.indirect(info).dict().pair(Name(b"Producer"), TextStr("Gezik"));
        self.put(info, c)?;

        let size = self.next;
        let xref = self.pos;
        // Every entry is exactly 20 bytes; a number never written is free.
        let mut t = format!("xref\n0 {size}\n0000000000 65535 f \n");
        for id in 1..size as usize {
            match self.offsets.get(id) {
                Some(&off) if off > 0 => t.push_str(&format!("{off:010} 00000 n \n")),
                _ => t.push_str("0000000000 65535 f \n"),
            }
        }
        t.push_str(&format!(
            "trailer\n<< /Size {size} /Root {} 0 R /Info {} 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            self.catalog.get(),
            info.get()
        ));
        self.out.write_all(t.as_bytes())?;
        self.out.flush()
    }
}

/// Splits interleaved samples into colour and alpha planes. `px` and `color` are byte counts.
fn split(raw: &[u8], px: usize, color: usize, has_alpha: bool) -> (Vec<u8>, Option<Vec<u8>>) {
    if !has_alpha {
        return (raw.chunks_exact(px).flat_map(|p| p[..color].iter().copied()).collect(), None);
    }
    let mut c = Vec::with_capacity(raw.len() / px * color);
    let mut a = Vec::with_capacity(raw.len() / px * (px - color));
    for p in raw.chunks_exact(px) {
        c.extend_from_slice(&p[..color]);
        a.extend_from_slice(&p[color..]);
    }
    // An all-opaque alpha channel needs no SMask (0xFF bytes = 255 or 65535).
    let opaque = a.iter().all(|&v| v == 255);
    (c, (!opaque).then_some(a))
}

/// PNG Paeth filter on every row (filter byte 4); `bpp` = bytes per pixel, rounded up.
fn paeth(raw: &[u8], stride: usize, bpp: usize) -> Vec<u8> {
    let rows = raw.len() / stride;
    let mut out = Vec::with_capacity(raw.len() + rows);
    let zero = vec![0u8; stride];
    for r in 0..rows {
        let cur = &raw[r * stride..][..stride];
        let up = if r == 0 { &zero[..] } else { &raw[(r - 1) * stride..][..stride] };
        out.push(4);
        for i in 0..stride {
            let a = if i >= bpp { cur[i - bpp] as i16 } else { 0 };
            let b = up[i] as i16;
            let c = if i >= bpp { up[i - bpp] as i16 } else { 0 };
            let p = a + b - c;
            let (pa, pb, pc) = ((p - a).abs(), (p - b).abs(), (p - c).abs());
            let pred = if pa <= pb && pa <= pc {
                a
            } else if pb <= pc {
                b
            } else {
                c
            };
            out.push(cur[i].wrapping_sub(pred as u8));
        }
    }
    out
}

/// PDF wants 16-bit samples big-endian.
fn be16(v: &[u16]) -> Vec<u8> {
    v.iter().flat_map(|s| s.to_be_bytes()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exif_numbers_match_the_orientations() {
        let all = [
            Orientation::NoTransforms,
            Orientation::FlipHorizontal,
            Orientation::Rotate180,
            Orientation::FlipVertical,
            Orientation::Rotate90FlipH,
            Orientation::Rotate90,
            Orientation::Rotate270FlipH,
            Orientation::Rotate270,
        ];
        for o in all {
            assert_eq!(Orientation::from_exif(exif_number(o)), Some(o));
        }
    }

    #[test]
    fn a_quarter_turn_swaps_the_density() {
        assert_eq!(upright_dpi(Some((300.0, 150.0)), 1), Some((300.0, 150.0)));
        assert_eq!(upright_dpi(Some((300.0, 150.0)), 3), Some((300.0, 150.0)));
        for o in 5..=8 {
            assert_eq!(upright_dpi(Some((300.0, 150.0)), o), Some((150.0, 300.0)));
        }
        assert_eq!(upright_dpi(None, 6), None);
    }

    #[test]
    fn opaque_alpha_is_dropped_and_sixteen_bits_go_big_endian() {
        assert_eq!(split(&[1, 2, 3, 255, 4, 5, 6, 255], 4, 3, true), (vec![1, 2, 3, 4, 5, 6], None));
        assert_eq!(split(&[1, 2, 3, 7, 4, 5, 6, 255], 4, 3, true), (vec![1, 2, 3, 4, 5, 6], Some(vec![7, 255])));
        assert_eq!(be16(&[0x1234, 0xABCD]), vec![0x12, 0x34, 0xAB, 0xCD]);
    }

    #[test]
    fn paeth_rows_start_with_their_filter_byte() {
        // One grey row of a constant: after the first byte every prediction is exact.
        assert_eq!(paeth(&[9, 9, 9], 3, 1), vec![4, 9, 0, 0]);
        // The second row predicts from the first.
        assert_eq!(paeth(&[1, 2, 1, 2], 2, 1), vec![4, 1, 1, 4, 0, 0]);
    }
}
