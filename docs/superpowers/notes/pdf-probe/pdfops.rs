//! Merge / split / extract / render with pdfium-render 0.9.4 (dynamic, no `thread_safe`,
//! no `image_*` features).
use std::path::Path;

use pdfium_render::prelude::*;

pub fn bind(dir: &Path) -> Result<Pdfium, PdfiumError> {
    let lib = Pdfium::pdfium_platform_library_name_at_path(dir);
    Ok(Pdfium::new(Pdfium::bind_to_library(lib)?))
}

/// What the password question layer needs to know.
#[derive(Debug, PartialEq)]
pub enum OpenError {
    NeedsPassword, // no password given and the file is encrypted
    WrongPassword, // a password was given and it does not open the file
    Damaged,       // FPDF_ERR_FORMAT
    Other(String),
}

pub fn open<'a>(p: &'a Pdfium, path: &Path, password: Option<&str>) -> Result<PdfDocument<'a>, OpenError> {
    p.load_pdf_from_file(path, password).map_err(|e| match e {
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => {
            if password.is_none() { OpenError::NeedsPassword } else { OpenError::WrongPassword }
        }
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::FormatError) => OpenError::Damaged,
        e => OpenError::Other(format!("{e:?}")),
    })
}

/// Parses Gezik's range syntax, `1-3, 5, 8-` (1-based, open end = last page), into 0-based
/// indexes. Returns None for out-of-range or malformed parts.
pub fn parse_ranges(s: &str, pages: i32) -> Option<Vec<i32>> {
    let mut out = Vec::new();
    for part in s.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let (a, b) = match part.split_once('-') {
            Some((a, b)) => {
                let a: i32 = if a.trim().is_empty() { 1 } else { a.trim().parse().ok()? };
                let b: i32 = if b.trim().is_empty() { pages } else { b.trim().parse().ok()? };
                (a, b)
            }
            None => {
                let n: i32 = part.parse().ok()?;
                (n, n)
            }
        };
        if a == 0 || b < a || b > pages {
            return None;
        }
        out.extend(a - 1..b);
    }
    (!out.is_empty()).then_some(out)
}

/// Copies the given 0-based pages of `src` (in this order) into a new document.
pub fn pages_to_new<'a>(p: &'a Pdfium, src: &PdfDocument, idx: &[i32]) -> Result<PdfDocument<'a>, PdfiumError> {
    let mut out = p.create_new_pdf()?;
    // Consecutive runs go through FPDF_ImportPagesByIndex in one call each.
    let mut i = 0;
    while i < idx.len() {
        let mut j = i;
        while j + 1 < idx.len() && idx[j + 1] == idx[j] + 1 {
            j += 1;
        }
        let at = out.pages().len();
        out.pages_mut().copy_page_range_from_document(src, idx[i]..=idx[j], at)?;
        i = j + 1;
    }
    Ok(out)
}

pub fn merge(p: &Pdfium, inputs: &[&Path], out: &Path) -> Result<i32, PdfiumError> {
    let mut dst = p.create_new_pdf()?;
    for path in inputs {
        let src = p.load_pdf_from_file(path, None)?;
        dst.pages_mut().append(&src)?;
        // `src` drops here: FPDF_CloseDocument; the imported pages stay valid in `dst`.
    }
    dst.save_to_file(out)?;
    Ok(dst.pages().len())
}

pub enum Fmt {
    Png,
    Jpeg(u8),
}

/// Renders every page at `dpi`, capping each page at `max_px` pixels (the DPI drops for that
/// page instead). Returns (w, h, effective dpi) per page.
pub fn render_all(doc: &PdfDocument, dpi: f32, max_px: u64, fmt: &Fmt, out_stem: &Path) -> Result<Vec<(u32, u32, f32)>, Box<dyn std::error::Error>> {
    let mut res = Vec::new();
    for (i, page) in doc.pages().iter().enumerate() {
        let (wpt, hpt) = (page.width().value, page.height().value);
        let mut d = dpi;
        let px = (wpt * d / 72.0).ceil() as u64 * (hpt * d / 72.0).ceil() as u64;
        if px > max_px {
            d *= (max_px as f64 / px as f64).sqrt() as f32 * 0.999;
        }
        let cfg = PdfRenderConfig::new().scale_page_by_factor(d / 72.0);
        let bmp = page.render_with_config(&cfg)?;
        let (w, h) = (bmp.width() as u32, bmp.height() as u32);
        let rgba = bmp.as_rgba_bytes();
        let path = out_stem.with_file_name(format!(
            "{} - page {}.{}",
            out_stem.file_name().unwrap().to_string_lossy(),
            i + 1,
            if matches!(fmt, Fmt::Png) { "png" } else { "jpg" }
        ));
        match fmt {
            Fmt::Png => image::save_buffer(&path, &rgba, w, h, image::ExtendedColorType::Rgba8)?,
            Fmt::Jpeg(q) => {
                // Page is opaque (white clear colour): drop alpha, encode with jpeg-encoder.
                let rgb: Vec<u8> = rgba.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
                let mut enc = jpeg_encoder::Encoder::new_file(&path, *q)?;
                let dd = d.round() as u16;
                enc.set_density(jpeg_encoder::PixelDensity { density: (dd, dd), unit: jpeg_encoder::PixelDensityUnit::Inches });
                enc.encode(&rgb, w as u16, h as u16, jpeg_encoder::ColorType::Rgb)?;
            }
        }
        res.push((w, h, d));
    }
    Ok(res)
}
