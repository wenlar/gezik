//! pdfium through pdfium-render 0.9.4, loaded at run time (`pdfium_7881` bindings: the
//! library must be chromium/7881 or later). Only the worker process uses this module: the
//! bindings live in a process-wide cell that can never be emptied, and pdfium is not thread
//! safe, so one worker process does one request on one thread and exits.
//!
//! Files are opened and saved through Rust's `File` (pdfium-render's `load_pdf_from_file` and
//! `save_to_file` do so), so any name the system takes works, Unicode or not.
//!
//! What pdfium copies is the pages and their resources: bookmarks, named destinations, forms,
//! the document information and the encryption are left behind (özet §2.2, §2.5). The Convert
//! layer says so; nothing here tries to keep them.

use std::io;
use std::path::Path;

use gezik_core::batch::pdf::{JPEG_QUALITY, PageImage, RenderSize, render_size};
use pdfium_render::prelude::*;

/// Loads the pdfium library file `library` (every function it binds is looked up at once:
/// an older library fails here).
pub fn bind(library: &Path) -> Result<Pdfium, String> {
    Pdfium::bind_to_library(library).map(Pdfium::new).map_err(|e| format!("{e:?}"))
}

/// Why a PDF did not open.
#[derive(Debug, PartialEq, Eq)]
pub enum OpenError {
    /// It is encrypted and no password was given.
    NeedsPassword,
    /// A password was given and it does not open the file (pdfium has one error for both).
    WrongPassword,
    /// pdfium could not make sense of it.
    Damaged,
    /// Anything else, such as the file not being there.
    Other(String),
}

/// Opens `path`, with `password` when one is given.
pub fn open<'a>(p: &'a Pdfium, path: &Path, password: Option<&str>) -> Result<PdfDocument<'a>, OpenError> {
    p.load_pdf_from_file(path, password).map_err(|e| match e {
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => {
            if password.is_none() {
                OpenError::NeedsPassword
            } else {
                OpenError::WrongPassword
            }
        }
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::FormatError) => OpenError::Damaged,
        PdfiumError::IoError(e) => OpenError::Other(e.to_string()),
        e => OpenError::Other(format!("{e:?}")),
    })
}

/// A new document of the 0-based pages `idx` of `src`, in this order (repeats allowed). Each
/// run of consecutive pages is one `FPDF_ImportPagesByIndex` call.
pub fn pages_to_new<'a>(p: &'a Pdfium, src: &PdfDocument, idx: &[u32]) -> Result<PdfDocument<'a>, PdfiumError> {
    let index = |page: u32| PdfPageIndex::try_from(page).map_err(|_| PdfiumError::PageIndexOutOfBounds);
    let mut out = p.create_new_pdf()?;
    let mut i = 0;
    while i < idx.len() {
        let mut j = i;
        while j + 1 < idx.len() && idx[j].checked_add(1) == Some(idx[j + 1]) {
            j += 1;
        }
        let at = out.pages().len();
        out.pages_mut().copy_page_range_from_document(src, index(idx[i])?..=index(idx[j])?, at)?;
        i = j + 1;
    }
    Ok(out)
}

/// Renders `page` at `dpi` (lowered when the picture would pass 64 MP or 65,535 px a side) to
/// `path` as `image`, and says the size it used.
///
/// Memory: pdfium's bitmap and one RGBA copy of it exist together for a moment; the bitmap
/// goes at once and the copy is packed to RGB in place, so the peak stays at two full
/// pictures (about 512 MB at 64 MP).
pub fn render_page(page: &PdfPage, dpi: u32, image: PageImage, path: &Path) -> io::Result<RenderSize> {
    let size = render_size(page.width().value, page.height().value, dpi);
    let cfg = PdfRenderConfig::new().scale_page_by_factor(size.dpi / 72.0);
    let bmp = page.render_with_config(&cfg).map_err(|e| io::Error::other(format!("rendering failed: {e:?}")))?;
    let (w, h) = (bmp.width() as usize, bmp.height() as usize);
    let mut buf = bmp.as_rgba_bytes();
    drop(bmp);
    if w == 0 || h == 0 || buf.len() < 4 * w * h {
        return Err(io::Error::other(format!("pdfium gave a {w} × {h} picture of {} bytes", buf.len())));
    }
    for i in 0..w * h {
        buf.copy_within(4 * i..4 * i + 3, 3 * i);
    }
    buf.truncate(3 * w * h);
    let (w, h) = (w as u32, h as u32);
    match image {
        PageImage::Png => {
            ::image::save_buffer(path, &buf, w, h, ::image::ExtendedColorType::Rgb8).map_err(io::Error::other)?
        }
        PageImage::Jpeg => {
            let (jw, jh) = (u16::try_from(w), u16::try_from(h));
            let (Ok(jw), Ok(jh)) = (jw, jh) else {
                return Err(io::Error::other(format!("{w} × {h} is too large for a JPEG")));
            };
            let mut encoder = jpeg_encoder::Encoder::new_file(path, JPEG_QUALITY).map_err(io::Error::other)?;
            let d = size.dpi.round() as u16;
            encoder.set_density(jpeg_encoder::PixelDensity {
                density: (d, d),
                unit: jpeg_encoder::PixelDensityUnit::Inches,
            });
            encoder.encode(&buf, jw, jh, jpeg_encoder::ColorType::Rgb).map_err(io::Error::other)?;
        }
    }
    Ok(size)
}
