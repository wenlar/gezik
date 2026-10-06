//! The pure decisions of the PDF group: page ranges, split parts, output names, page
//! placement for "Images to PDF", the EXIF orientation matrix and the render size limit.
//! Reading and writing PDFs is the `gezik-batch` crate's job.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::convert::{image_inputs, needs_ffmpeg_to_read};

/// The PDF group of the Convert layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfOp {
    ImagesToPdf,
    Merge,
    Split,
    ToImages,
    Extract,
}

impl PdfOp {
    pub const ALL: [PdfOp; 5] = [PdfOp::ImagesToPdf, PdfOp::Merge, PdfOp::Split, PdfOp::ToImages, PdfOp::Extract];

    pub fn id(self) -> &'static str {
        match self {
            PdfOp::ImagesToPdf => "images-to-pdf",
            PdfOp::Merge => "merge-pdfs",
            PdfOp::Split => "split-pdf",
            PdfOp::ToImages => "pdf-to-images",
            PdfOp::Extract => "extract-pages",
        }
    }

    pub fn from_id(id: &str) -> Option<PdfOp> {
        PdfOp::ALL.into_iter().find(|op| op.id() == id)
    }

    pub fn label(self) -> &'static str {
        match self {
            PdfOp::ImagesToPdf => "Images to PDF",
            PdfOp::Merge => "Merge PDFs",
            PdfOp::Split => "Split PDF",
            PdfOp::ToImages => "PDF to images",
            PdfOp::Extract => "Extract pages",
        }
    }

    /// Whether it reads PDFs, which needs the pdfium library.
    pub fn needs_pdfium(self) -> bool {
        self != PdfOp::ImagesToPdf
    }

    /// Whether the new PDF leaves out what is not page content (bookmarks, forms, links).
    pub fn loses_extras(self) -> bool {
        matches!(self, PdfOp::Merge | PdfOp::Split | PdfOp::Extract)
    }
}

/// Whether `name` is a PDF, by its extension (any case).
pub fn is_pdf(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(stem, ext)| !stem.is_empty() && ext.eq_ignore_ascii_case("pdf"))
}

/// Whether `name` is a picture "Images to PDF" reads itself (not the ffmpeg-only ones).
pub fn is_pdf_picture(name: &str) -> bool {
    image_inputs(name) && !needs_ffmpeg_to_read(name)
}

/// The PDF operations a selection of `pictures` and `pdfs` gets, in [`PdfOp::ALL`] order.
pub fn ops_for(pictures: usize, pdfs: usize) -> Vec<PdfOp> {
    PdfOp::ALL
        .into_iter()
        .filter(|op| match op {
            PdfOp::ImagesToPdf => pictures >= 1,
            PdfOp::Merge => pdfs >= 2,
            PdfOp::Split | PdfOp::ToImages | PdfOp::Extract => pdfs >= 1,
        })
        .collect()
}

/// A range of pages, 1-based; `last` is `None` up to the end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageRange {
    pub first: u32,
    pub last: Option<u32>,
}

fn page_number(text: &str, part: &str) -> Result<u32, String> {
    let not_a_page = || format!("\"{part}\" is not a page or a range");
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(not_a_page());
    }
    match text.parse::<u32>() {
        Ok(0) => Err("Pages start at 1".to_string()),
        Ok(n) => Ok(n),
        Err(_) => Err(not_a_page()),
    }
}

/// Parses "1-3, 5, 8-": comma-separated `N`, `A-B`, `A-` (to the end) and `-B` (from the start).
pub fn parse_ranges(text: &str) -> Result<Vec<PageRange>, String> {
    let mut ranges = Vec::new();
    for part in text.split(',').map(str::trim).filter(|part| !part.is_empty()) {
        let range = match part.split_once('-') {
            None => {
                let n = page_number(part, part)?;
                PageRange { first: n, last: Some(n) }
            }
            Some((a, b)) => {
                let (a, b) = (a.trim(), b.trim());
                let first = if a.is_empty() { 1 } else { page_number(a, part)? };
                let last = if b.is_empty() { None } else { Some(page_number(b, part)?) };
                if last.is_some_and(|last| last < first) {
                    return Err(format!("{part} runs backwards"));
                }
                PageRange { first, last }
            }
        };
        ranges.push(range);
    }
    if ranges.is_empty() {
        return Err("Type the pages, e.g. 1-3, 5, 8-".to_string());
    }
    Ok(ranges)
}

/// The 0-based pages of each range in a document of `pages` pages.
pub fn resolve(ranges: &[PageRange], pages: u32) -> Result<Vec<Vec<u32>>, String> {
    let past_the_end = |page: u32| format!("Page {page} is past the end ({pages} pages)");
    ranges
        .iter()
        .map(|range| {
            if range.first > pages {
                return Err(past_the_end(range.first));
            }
            let last = range.last.unwrap_or(pages);
            if last > pages {
                return Err(past_the_end(last));
            }
            Ok((range.first - 1..last).collect())
        })
        .collect()
}

/// How "Split PDF" cuts the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Split {
    EachPage,
    Every(u32),
    Ranges(String),
}

fn check_split(split: &Split, pages: u32) -> Result<(), String> {
    if *split == Split::Every(0) {
        return Err("Every 0 pages: type 1 or more".to_string());
    }
    if pages == 0 {
        return Err("This PDF has no pages".to_string());
    }
    Ok(())
}

/// The 0-based pages of each part `split` makes of a document of `pages` pages.
pub fn split_parts(split: &Split, pages: u32) -> Result<Vec<Vec<u32>>, String> {
    check_split(split, pages)?;
    match split {
        Split::EachPage => Ok((0..pages).map(|page| vec![page]).collect()),
        Split::Every(n) => Ok((0..pages).collect::<Vec<u32>>().chunks(*n as usize).map(<[u32]>::to_vec).collect()),
        Split::Ranges(text) => resolve(&parse_ranges(text)?, pages),
    }
}

/// How many files `split` writes, without building the parts of the simple splits.
pub fn split_file_count(split: &Split, pages: u32) -> Result<usize, String> {
    check_split(split, pages)?;
    match split {
        Split::EachPage => Ok(pages as usize),
        Split::Every(n) => Ok(pages.div_ceil(*n) as usize),
        Split::Ranges(text) => resolve(&parse_ranges(text)?, pages).map(|parts| parts.len()),
    }
}

/// The 0-based pages "Extract pages" copies, in the order typed (repeats included).
pub fn extract_pages(ranges: &str, pages: u32) -> Result<Vec<u32>, String> {
    Ok(resolve(&parse_ranges(ranges)?, pages)?.into_iter().flatten().collect())
}

/// "page 3", "pages 1-3" or "pages 1-3, 5, 8-14" for 0-based `pages`.
pub fn pages_label(pages: &[u32]) -> String {
    let mut runs: Vec<(u32, u32)> = Vec::new();
    for &page in pages {
        match runs.last_mut() {
            Some((_, last)) if *last + 1 == page => *last = page,
            _ => runs.push((page, page)),
        }
    }
    let text: Vec<String> =
        runs.iter().map(|&(a, b)| if a == b { (a + 1).to_string() } else { format!("{}-{}", a + 1, b + 1) }).collect();
    if pages.len() == 1 { format!("page {}", text[0]) } else { format!("pages {}", text.join(", ")) }
}

fn stem_of(path: &Path) -> OsString {
    path.file_stem().map(OsString::from).unwrap_or_default()
}

/// "a (merged).pdf" for the merge of PDFs starting with `first`.
pub fn merged_name(first: &Path) -> OsString {
    let mut name = stem_of(first);
    name.push(" (merged).pdf");
    name
}

/// "a - page 3.pdf"; a label longer than 40 characters gives the count: "a - 23 pages.pdf".
pub fn part_name(input: &Path, pages: &[u32]) -> OsString {
    let mut label = pages_label(pages);
    if label.chars().count() > 40 {
        label = format!("{} pages", pages.len());
    }
    let mut name = stem_of(input);
    name.push(format!(" - {label}.pdf"));
    name
}

/// The picture format "PDF to images" writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageImage {
    Png,
    Jpeg,
}

pub const JPEG_QUALITY: u8 = 90;

/// "a - page 3.png" (or ".jpg") for the 0-based `page`.
pub fn page_image_name(input: &Path, page: u32, image: PageImage) -> OsString {
    let ext = match image {
        PageImage::Png => "png",
        PageImage::Jpeg => "jpg",
    };
    let mut name = stem_of(input);
    name.push(format!(" - page {}.{ext}", page + 1));
    name
}

/// Where "Images to PDF" writes: next to a single picture under its name, otherwise in the
/// first picture's folder under the folder's name ("Pictures" for a root).
pub fn pictures_pdf_name(pictures: &[PathBuf]) -> Option<PathBuf> {
    let first = pictures.first()?;
    let dir = first.parent().unwrap_or(Path::new(""));
    let mut name = if pictures.len() == 1 {
        stem_of(first)
    } else {
        dir.file_name().map(OsString::from).unwrap_or_else(|| OsString::from("Pictures"))
    };
    name.push(".pdf");
    Some(dir.join(name))
}

pub const RENDER_DPIS: [u32; 3] = [72, 150, 300];
pub const MAX_RENDER_PIXELS: u64 = 64_000_000;
pub const MAX_RENDER_SIDE: u32 = 65_535;

/// The size a page renders at: `dpi` is lowered when the picture would be too big.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderSize {
    pub dpi: f32,
    pub width: u32,
    pub height: u32,
    pub lowered: bool,
}

fn pixels(points: f32, dpi: f64) -> u32 {
    (f64::from(points) * dpi / 72.0).ceil().clamp(1.0, f64::from(u32::MAX)) as u32
}

/// The pixel size of a page of `width_pt` × `height_pt` points at `dpi`, kept within
/// [`MAX_RENDER_PIXELS`] and [`MAX_RENDER_SIDE`].
pub fn render_size(width_pt: f32, height_pt: f32, dpi: u32) -> RenderSize {
    let dpi = f64::from(dpi);
    let (w, h) = (pixels(width_pt, dpi), pixels(height_pt, dpi));
    let area = u64::from(w) * u64::from(h);
    let side = w.max(h);
    if area <= MAX_RENDER_PIXELS && side <= MAX_RENDER_SIDE {
        return RenderSize { dpi: dpi as f32, width: w, height: h, lowered: false };
    }
    let by_area = (MAX_RENDER_PIXELS as f64 / area as f64).sqrt();
    let by_side = f64::from(MAX_RENDER_SIDE) / f64::from(side);
    let lowered_dpi = dpi * by_area.min(by_side) * 0.999;
    RenderSize {
        dpi: lowered_dpi as f32,
        width: pixels(width_pt, lowered_dpi),
        height: pixels(height_pt, lowered_dpi),
        lowered: true,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageSize {
    Picture,
    A4,
    Letter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Margin {
    None,
    Small,
    Large,
}

impl Margin {
    pub fn points(self) -> f32 {
        match self {
            Margin::None => 0.0,
            Margin::Small => 18.0,
            Margin::Large => 36.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageOptions {
    pub size: PageSize,
    pub margin: Margin,
}

impl PageOptions {
    pub const DEFAULT: PageOptions = PageOptions { size: PageSize::Picture, margin: Margin::None };
}

/// Acrobat's page limit: 200 inches.
pub const MAX_PAGE_POINTS: f32 = 14_400.0;

/// A page and the rectangle of the picture on it, in points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub page_width: f32,
    pub page_height: f32,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Where a picture of `shown_w` × `shown_h` pixels (as shown, orientation applied) goes.
pub fn place(shown_w: u32, shown_h: u32, dpi: Option<(f32, f32)>, options: &PageOptions) -> Placement {
    let (dw, dh) = (shown_w as f32, shown_h as f32);
    let m = options.margin.points();
    match options.size {
        PageSize::Picture => {
            let (dx, dy) = dpi.unwrap_or((72.0, 72.0));
            let (mut w, mut h) = (dw * 72.0 / dx, dh * 72.0 / dy);
            // Acrobat's page limit is 14400 pt; scale down beyond it.
            let k = (MAX_PAGE_POINTS - 2.0 * m) / w.max(h);
            if k < 1.0 {
                w *= k;
                h *= k;
            }
            Placement { page_width: w + 2.0 * m, page_height: h + 2.0 * m, x: m, y: m, width: w, height: h }
        }
        PageSize::A4 | PageSize::Letter => {
            let (mut pw, mut ph) = if options.size == PageSize::A4 { (595.276, 841.89) } else { (612.0, 792.0) };
            // Landscape pictures get a landscape sheet.
            if dw > dh {
                std::mem::swap(&mut pw, &mut ph);
            }
            let k = ((pw - 2.0 * m) / dw).min((ph - 2.0 * m) / dh);
            let (w, h) = (dw * k, dh * k);
            Placement { page_width: pw, page_height: ph, x: (pw - w) / 2.0, y: (ph - h) / 2.0, width: w, height: h }
        }
    }
}

/// The size a picture of `w` × `h` stored pixels shows at: EXIF orientations 5-8 swap them.
pub fn shown_size(w: u32, h: u32, exif_orientation: u8) -> (u32, u32) {
    if (5..=8).contains(&exif_orientation) { (h, w) } else { (w, h) }
}

/// The matrix `[a b c d e f]` applied to the unit square so that the stored picture shows
/// upright; an unknown orientation is the identity.
pub fn orient_matrix(exif_orientation: u8) -> [f32; 6] {
    match exif_orientation {
        2 => [-1.0, 0.0, 0.0, 1.0, 1.0, 0.0],
        3 => [-1.0, 0.0, 0.0, -1.0, 1.0, 1.0],
        4 => [1.0, 0.0, 0.0, -1.0, 0.0, 1.0],
        5 => [0.0, -1.0, -1.0, 0.0, 1.0, 1.0],
        6 => [0.0, -1.0, 1.0, 0.0, 0.0, 1.0],
        7 => [0.0, 1.0, 1.0, 0.0, 0.0, 0.0],
        8 => [0.0, 1.0, -1.0, 0.0, 1.0, 0.0],
        _ => [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn r(first: u32, last: Option<u32>) -> PageRange {
        PageRange { first, last }
    }

    #[test]
    fn ranges_parse_in_gezik_syntax() {
        assert_eq!(parse_ranges("1-3, 5, 8-").unwrap(), vec![r(1, Some(3)), r(5, Some(5)), r(8, None)]);
        assert_eq!(parse_ranges(" -2 ,, 4 ").unwrap(), vec![r(1, Some(2)), r(4, Some(4))]);
        assert_eq!(parse_ranges("-").unwrap(), vec![r(1, None)]);
        assert_eq!(parse_ranges("").unwrap_err(), "Type the pages, e.g. 1-3, 5, 8-");
        assert_eq!(parse_ranges(" , ").unwrap_err(), "Type the pages, e.g. 1-3, 5, 8-");
        assert_eq!(parse_ranges("x").unwrap_err(), "\"x\" is not a page or a range");
        assert_eq!(parse_ranges("1-2-3").unwrap_err(), "\"1-2-3\" is not a page or a range");
        assert_eq!(parse_ranges("0-2").unwrap_err(), "Pages start at 1");
        assert_eq!(parse_ranges("5-3").unwrap_err(), "5-3 runs backwards");
    }

    #[test]
    fn ranges_resolve_against_the_page_count() {
        // The probe's case (özet §2.3).
        let parts = resolve(&parse_ranges("1-3, 5, 8-").unwrap(), 14).unwrap();
        assert_eq!(parts, vec![vec![0, 1, 2], vec![4], (7..14).collect::<Vec<u32>>()]);
        assert_eq!(resolve(&parse_ranges("1-99").unwrap(), 14).unwrap_err(), "Page 99 is past the end (14 pages)");
        assert_eq!(resolve(&parse_ranges("15-").unwrap(), 14).unwrap_err(), "Page 15 is past the end (14 pages)");
        assert_eq!(extract_pages("3, 3, 1", 5).unwrap(), vec![2, 2, 0]);
    }

    #[test]
    fn splits_make_their_parts() {
        assert_eq!(split_parts(&Split::EachPage, 3).unwrap(), vec![vec![0], vec![1], vec![2]]);
        assert_eq!(split_parts(&Split::Every(4), 10).unwrap(), vec![vec![0, 1, 2, 3], vec![4, 5, 6, 7], vec![8, 9]]);
        assert_eq!(split_parts(&Split::Every(0), 10).unwrap_err(), "Every 0 pages: type 1 or more");
        assert_eq!(split_parts(&Split::Ranges("2-, 1".into()), 3).unwrap(), vec![vec![1, 2], vec![0]]);
        assert_eq!(split_parts(&Split::EachPage, 0).unwrap_err(), "This PDF has no pages");
        assert_eq!(split_file_count(&Split::EachPage, 20_000).unwrap(), 20_000);
        assert_eq!(split_file_count(&Split::Every(3), 10).unwrap(), 4);
    }

    #[test]
    fn names_follow_the_spec() {
        let a = Path::new("d").join("rapor ş.pdf");
        assert_eq!(merged_name(&a), "rapor ş (merged).pdf");
        assert_eq!(part_name(&a, &[2]), "rapor ş - page 3.pdf");
        assert_eq!(part_name(&a, &[0, 1, 2]), "rapor ş - pages 1-3.pdf");
        assert_eq!(part_name(&a, &[0, 1, 2, 4, 7, 8, 9]), "rapor ş - pages 1-3, 5, 8-10.pdf");
        // A label longer than 40 characters gives the page count instead.
        let scattered: Vec<u32> = (0..40).step_by(2).collect();
        assert_eq!(part_name(&a, &scattered), "rapor ş - 20 pages.pdf");
        assert_eq!(page_image_name(&a, 0, PageImage::Png), "rapor ş - page 1.png");
        assert_eq!(page_image_name(&a, 11, PageImage::Jpeg), "rapor ş - page 12.jpg");
        assert_eq!(pages_label(&[4]), "page 5");
        assert_eq!(pages_label(&[2, 2]), "pages 3, 3");
    }

    #[test]
    fn a_pictures_pdf_is_named_after_the_picture_or_the_folder() {
        let dir = Path::new("x").join("Tatil");
        assert_eq!(pictures_pdf_name(&[dir.join("a.jpg")]), Some(dir.join("a.pdf")));
        assert_eq!(pictures_pdf_name(&[dir.join("a.jpg"), dir.join("b.png")]), Some(dir.join("Tatil.pdf")));
        assert_eq!(
            pictures_pdf_name(&[PathBuf::from("/a.jpg"), PathBuf::from("/b.jpg")]),
            Some(PathBuf::from("/Pictures.pdf"))
        );
        assert_eq!(pictures_pdf_name(&[]), None);
    }

    #[test]
    fn which_ops_a_selection_gets() {
        assert_eq!(ops_for(3, 0), vec![PdfOp::ImagesToPdf]);
        assert_eq!(ops_for(0, 1), vec![PdfOp::Split, PdfOp::ToImages, PdfOp::Extract]);
        assert_eq!(ops_for(2, 2), PdfOp::ALL.to_vec());
        assert!(is_pdf("A.PDF") && !is_pdf(".pdf") && !is_pdf("a.pdfx"));
        assert!(is_pdf_picture("a.jpg") && !is_pdf_picture("a.heic") && !is_pdf_picture("a.pdf"));
        for op in PdfOp::ALL {
            assert_eq!(PdfOp::from_id(op.id()), Some(op));
        }
        assert!(!PdfOp::ImagesToPdf.needs_pdfium() && PdfOp::Merge.loses_extras() && !PdfOp::ToImages.loses_extras());
    }

    #[test]
    fn render_size_stays_under_64_megapixels_and_65535_px() {
        let a4 = render_size(595.276, 841.89, 300);
        assert_eq!((a4.width, a4.height, a4.lowered), (2481, 3508, false));
        // The probe's huge page (özet §4.2): 14400 pt square at 300 dpi would be 60000 × 60000.
        let huge = render_size(14_400.0, 14_400.0, 300);
        assert!(huge.lowered);
        assert!(u64::from(huge.width) * u64::from(huge.height) <= MAX_RENDER_PIXELS, "{huge:?}");
        assert!(huge.dpi > 39.0 && huge.dpi < 40.0, "{huge:?}");
        // A long thin strip hits the side limit first.
        let strip = render_size(30_000.0, 10.0, 300);
        assert!(strip.lowered && strip.width <= MAX_RENDER_SIDE, "{strip:?}");
        assert!(render_size(0.5, 0.5, 72).width >= 1);
    }

    #[test]
    fn pages_are_placed_as_the_probe_placed_them() {
        // Picture size at 72 dpi, no margin: 640 × 480 pt.
        let p = place(640, 480, None, &PageOptions::DEFAULT);
        assert_eq!((p.page_width, p.page_height, p.x, p.y, p.width, p.height), (640.0, 480.0, 0.0, 0.0, 640.0, 480.0));
        // A 300 dpi JFIF density: 640 px → 153.6 pt.
        assert!((place(640, 480, Some((300.0, 300.0)), &PageOptions::DEFAULT).width - 153.6).abs() < 0.01);
        // A landscape picture gets a landscape A4 (özet §1.5).
        let a4 = PageOptions { size: PageSize::A4, margin: Margin::Small };
        let p = place(4000, 3000, None, &a4);
        assert_eq!((p.page_width, p.page_height), (841.89, 595.276));
        assert!((p.height - (595.276 - 36.0)).abs() < 0.01 && (p.x - (841.89 - p.width) / 2.0).abs() < 0.01);
        // Beyond 14400 pt it is scaled down.
        let big = place(30_000, 10_000, None, &PageOptions::DEFAULT);
        assert!(big.page_width <= MAX_PAGE_POINTS + 0.01);
        let letter = place(300, 400, None, &PageOptions { size: PageSize::Letter, margin: Margin::None });
        assert_eq!((letter.page_width, letter.page_height), (612.0, 792.0));
    }

    #[test]
    fn exif_orientations_turn_the_unit_square() {
        assert_eq!(orient_matrix(1), [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
        assert_eq!(orient_matrix(6), [0.0, -1.0, 1.0, 0.0, 0.0, 1.0]);
        assert_eq!(orient_matrix(8), [0.0, 1.0, -1.0, 0.0, 1.0, 0.0]);
        assert_eq!(orient_matrix(9), orient_matrix(1));
        assert_eq!(shown_size(640, 480, 6), (480, 640));
        assert_eq!(shown_size(640, 480, 3), (640, 480));
    }
}
