//! Images to PDF with pdf-writer 0.15 + miniz_oxide, streamed to disk one object at a time.
//!
//! pdf-writer's `Pdf` keeps the whole file in one `Vec<u8>` until `finish()`. For 100 photos
//! that is the sum of all JPEGs in RAM, so each indirect object goes into its own `Chunk`,
//! is written to the file at once, and we keep only (id, offset) to write the xref table.
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageReader};
use miniz_oxide::deflate::compress_to_vec_zlib;
use pdf_writer::{Chunk, Content, Filter, Finish, Name, Rect, Ref};

use crate::jpeg;

#[derive(Clone, Copy, Debug)]
pub enum PageSize {
    /// The page is the image (at its JFIF dpi, else `default_dpi`).
    Image,
    A4,
    Letter,
}

#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub page: PageSize,
    /// Margin in points (1/72 in) on every side.
    pub margin: f32,
    /// Used for "image size" pages when the file states no density.
    pub default_dpi: f32,
    /// Level for miniz_oxide (0-10). 6 is zlib's default.
    pub level: u8,
    /// Use the JFIF density for "image size" pages (false: always `default_dpi`).
    pub use_file_dpi: bool,
    /// Embed ICC profiles as ICCBased colour spaces (probe switch).
    pub embed_icc: bool,
    /// PNG Paeth row filter before Flate (DecodeParms /Predictor 15).
    pub predictor: bool,
    /// Probe switch: write the Decode array for Adobe CMYK JPEGs (true is correct).
    pub invert_adobe_cmyk: bool,
}

pub struct PdfStream {
    out: BufWriter<File>,
    pos: u64,
    offsets: Vec<(i32, u64)>,
    next: i32,
    catalog: Ref,
    tree: Ref,
    pages: Vec<Ref>,
}

impl PdfStream {
    pub fn create(path: &Path) -> std::io::Result<Self> {
        let mut out = BufWriter::with_capacity(1 << 20, File::create(path)?);
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
    fn put(&mut self, id: Ref, chunk: Chunk) -> std::io::Result<()> {
        self.offsets.push((id.get(), self.pos));
        let bytes = chunk.as_bytes();
        self.out.write_all(bytes)?;
        self.pos += bytes.len() as u64;
        Ok(())
    }

    /// Adds one page holding `path`. Returns how the image was embedded (for the log).
    pub fn add_image(&mut self, path: &Path, opt: &Options) -> Result<String, Box<dyn std::error::Error>> {
        let data = std::fs::read(path)?;
        let img_ref = self.alloc();
        let (pw, ph, orient, dpi, how);
        if let Some(info) = jpeg::parse(&data).filter(|i| i.passthrough_ok()) {
            // JPEG passthrough: the bytes go in unchanged as DCTDecode.
            let icc_ref = info.icc.as_ref().filter(|_| opt.embed_icc).map(|_| self.alloc());
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
            if info.components == 4 && info.adobe.is_some() && opt.invert_adobe_cmyk {
                // Adobe/Photoshop CMYK JPEGs store inverted ink values.
                x.decode([1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0]);
            }
            x.finish();
            self.put(img_ref, c)?;
            if let (Some(r), Some(icc)) = (icc_ref, &info.icc) {
                self.put_icc(r, icc, info.components as i32, opt.level)?;
            }
            pw = info.width;
            ph = info.height;
            orient = info.orientation;
            dpi = info.dpi.filter(|_| opt.use_file_dpi);
            how = format!(
                "jpeg passthrough sof={:#x} comps={} adobe={:?} icc={} orient={:?}",
                info.sof,
                info.components,
                info.adobe,
                info.icc.as_ref().map_or(0, |v| v.len()),
                info.orientation
            );
        } else {
            // Everything else: decode, orient, Flate the samples, alpha as SMask.
            let mut dec = ImageReader::new(std::io::Cursor::new(&data)).with_guessed_format()?.into_decoder()?;
            let icc = dec.icc_profile()?;
            let o = dec.orientation()?;
            let mut im = DynamicImage::from_decoder(dec)?;
            im.apply_orientation(o);
            let (w, h) = (im.width(), im.height());
            let ct = im.color();
            let sixteen = ct.bytes_per_pixel() / ct.channel_count() == 2;
            let gray = !ct.has_color();
            let (color, alpha): (Vec<u8>, Option<Vec<u8>>) = match (gray, sixteen) {
                (true, false) => {
                    let la = im.to_luma_alpha8();
                    split(la.as_raw(), 2, 1, ct.has_alpha())
                }
                (false, false) => {
                    if ct.has_alpha() {
                        split(im.to_rgba8().as_raw(), 4, 3, true)
                    } else {
                        (im.to_rgb8().into_raw(), None)
                    }
                }
                (true, true) => {
                    let la = im.to_luma_alpha16();
                    split(&be16(la.as_raw()), 4, 2, ct.has_alpha())
                }
                (false, true) => {
                    if ct.has_alpha() {
                        split(&be16(im.to_rgba16().as_raw()), 8, 6, true)
                    } else {
                        (be16(im.to_rgb16().as_raw()), None)
                    }
                }
            };
            let bpc = if sixteen { 16 } else { 8 };
            let n = if gray { 1 } else { 3 };
            let icc_ref = icc.as_ref().filter(|_| opt.embed_icc).map(|_| self.alloc());
            let mask_ref = alpha.as_ref().map(|_| self.alloc());
            let bpp = (n as usize) * (bpc as usize / 8);
            let z = if opt.predictor {
                compress_to_vec_zlib(&paeth(&color, w as usize * bpp, bpp), opt.level)
            } else {
                compress_to_vec_zlib(&color, opt.level)
            };
            let mut c = Chunk::new();
            let mut x = c.image_xobject(img_ref, &z);
            x.filter(Filter::FlateDecode);
            if opt.predictor {
                x.decode_parms().predictor(pdf_writer::types::Predictor::PngOptimum).colors(n).bits_per_component(bpc).columns(w as i32);
            }
            x.width(w as i32);
            x.height(h as i32);
            x.bits_per_component(bpc);
            match (icc_ref, gray) {
                (Some(r), _) => x.color_space().icc_based(r),
                (None, true) => x.color_space().device_gray(),
                (None, false) => x.color_space().device_rgb(),
            }
            if let Some(m) = mask_ref {
                x.s_mask(m);
            }
            x.finish();
            self.put(img_ref, c)?;
            if let (Some(m), Some(a)) = (mask_ref, &alpha) {
                let za = compress_to_vec_zlib(a, opt.level);
                let mut c = Chunk::new();
                let mut s = c.image_xobject(m, &za);
                s.filter(Filter::FlateDecode);
                s.width(w as i32);
                s.height(h as i32);
                s.color_space().device_gray();
                s.bits_per_component(bpc);
                s.finish();
                self.put(m, c)?;
            }
            if let (Some(r), Some(icc)) = (icc_ref, &icc) {
                self.put_icc(r, icc, n, opt.level)?;
            }
            pw = w;
            ph = h;
            orient = Orientation::NoTransforms; // already applied to the pixels
            dpi = None;
            how = format!("flate {ct:?} -> {bpc} bpc n={n} smask={} icc={} ({} -> {} B)", alpha.is_some(), icc.as_ref().map_or(0, |v| v.len()), color.len(), z.len());
        }

        // Displayed size (after EXIF orientation) in pixels.
        let swap = matches!(
            orient,
            Orientation::Rotate90 | Orientation::Rotate270 | Orientation::Rotate90FlipH | Orientation::Rotate270FlipH
        );
        let (dw, dh) = if swap { (ph as f32, pw as f32) } else { (pw as f32, ph as f32) };
        let (page_w, page_h, x, y, w, h) = place(dw, dh, dpi, opt);

        let page_ref = self.alloc();
        let content_ref = self.alloc();
        let mut content = Content::new();
        content.save_state();
        content.transform([w, 0.0, 0.0, h, x, y]);
        let o = orient_matrix(orient);
        if o != [1.0, 0.0, 0.0, 1.0, 0.0, 0.0] {
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
        page.media_box(Rect::new(0.0, 0.0, page_w, page_h));
        page.parent(self.tree);
        page.contents(content_ref);
        page.resources().x_objects().pair(Name(b"Im0"), img_ref);
        page.finish();
        self.put(page_ref, c)?;
        self.pages.push(page_ref);
        Ok(how)
    }

    fn put_icc(&mut self, r: Ref, icc: &[u8], n: i32, level: u8) -> std::io::Result<()> {
        let z = compress_to_vec_zlib(icc, level);
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
    pub fn finish(mut self) -> std::io::Result<u64> {
        let mut c = Chunk::new();
        c.pages(self.tree).kids(self.pages.iter().copied()).count(self.pages.len() as i32);
        self.put(self.tree, c)?;
        let mut c = Chunk::new();
        c.indirect(self.catalog).start::<pdf_writer::writers::Catalog>().pages(self.tree);
        self.put(self.catalog, c)?;
        let info = self.alloc();
        let mut c = Chunk::new();
        c.indirect(info).dict().pair(Name(b"Producer"), pdf_writer::TextStr("Gezik probe"));
        self.put(info, c)?;

        self.offsets.sort_by_key(|o| o.0);
        let size = self.next; // ids 1..next-1 are all written
        assert_eq!(self.offsets.len() as i32, size - 1, "an allocated object id was never written");
        let xref = self.pos;
        let mut t = format!("xref\n0 {size}\n0000000000 65535 f \n");
        for (_, off) in &self.offsets {
            t.push_str(&format!("{off:010} 00000 n \n"));
        }
        t.push_str(&format!(
            "trailer\n<< /Size {size} /Root {} 0 R /Info {} 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            self.catalog.get(),
            info.get()
        ));
        self.out.write_all(t.as_bytes())?;
        self.pos += t.len() as u64;
        self.out.flush()?;
        Ok(self.pos)
    }
}

/// Splits interleaved samples into color and alpha planes. `px` and `color` are byte counts.
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
pub fn paeth(raw: &[u8], stride: usize, bpp: usize) -> Vec<u8> {
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
            let pred = if pa <= pb && pa <= pc { a } else if pb <= pc { b } else { c };
            out.push(cur[i].wrapping_sub(pred as u8));
        }
    }
    out
}

/// PDF wants 16-bit samples big-endian.
fn be16(v: &[u16]) -> Vec<u8> {
    v.iter().flat_map(|s| s.to_be_bytes()).collect()
}

/// Matrix applied to the unit square before scaling, so that the stored (unrotated) image
/// shows upright. `[a b c d e f]` maps (u, v) to (a u + c v + e, b u + d v + f).
pub fn orient_matrix(o: Orientation) -> [f32; 6] {
    match o {
        Orientation::NoTransforms => [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        Orientation::FlipHorizontal => [-1.0, 0.0, 0.0, 1.0, 1.0, 0.0], // EXIF 2
        Orientation::Rotate180 => [-1.0, 0.0, 0.0, -1.0, 1.0, 1.0],     // EXIF 3
        Orientation::FlipVertical => [1.0, 0.0, 0.0, -1.0, 0.0, 1.0],   // EXIF 4
        Orientation::Rotate90FlipH => [0.0, -1.0, -1.0, 0.0, 1.0, 1.0], // EXIF 5 (transpose)
        Orientation::Rotate90 => [0.0, -1.0, 1.0, 0.0, 0.0, 1.0],       // EXIF 6 (90 cw)
        Orientation::Rotate270FlipH => [0.0, 1.0, 1.0, 0.0, 0.0, 0.0],  // EXIF 7 (transverse)
        Orientation::Rotate270 => [0.0, 1.0, -1.0, 0.0, 1.0, 0.0],      // EXIF 8 (90 ccw)
    }
}

/// Page size and the image rectangle (x, y, w, h) in points.
fn place(dw: f32, dh: f32, dpi: Option<(f32, f32)>, opt: &Options) -> (f32, f32, f32, f32, f32, f32) {
    let m = opt.margin;
    match opt.page {
        PageSize::Image => {
            let (dx, dy) = dpi.unwrap_or((opt.default_dpi, opt.default_dpi));
            let (mut w, mut h) = (dw * 72.0 / dx, dh * 72.0 / dy);
            // Acrobat's page limit is 14400 pt (200 in); scale down beyond it.
            let k = (14400.0 - 2.0 * m) / w.max(h);
            if k < 1.0 {
                w *= k;
                h *= k;
            }
            (w + 2.0 * m, h + 2.0 * m, m, m, w, h)
        }
        PageSize::A4 | PageSize::Letter => {
            let (mut pw, mut ph) = if matches!(opt.page, PageSize::A4) { (595.276, 841.89) } else { (612.0, 792.0) };
            // Landscape pictures get a landscape sheet.
            if dw > dh {
                std::mem::swap(&mut pw, &mut ph);
            }
            let k = ((pw - 2.0 * m) / dw).min((ph - 2.0 * m) / dh);
            let (w, h) = (dw * k, dh * k);
            (pw, ph, (pw - w) / 2.0, (ph - h) / 2.0, w, h)
        }
    }
}
