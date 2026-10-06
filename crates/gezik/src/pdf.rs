//! The PDF group of the Convert layer (convert.rs runs it): its choices as state.toml keeps
//! them, which items each operation takes, and the notes the layer shows before a split or an
//! extract ("Makes 14 files"). Pure: the page counts come from a thread (convert.rs).

use std::path::{Path, PathBuf};

use gezik_core::batch::pdf::{
    Margin, PageImage, PageOptions, PageSize, PdfOp, RENDER_DPIS, Split, extract_pages, is_pdf, is_pdf_picture,
    parse_ranges, split_file_count,
};

/// The PDF group's choices, kept in state.toml `[convert] pdf` (never a page range: those are
/// for one document).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfChoices {
    pub op: PdfOp,
    pub split: SplitChoice,
    /// "Every [N] pages".
    pub every: u32,
    /// "PDF to images".
    pub dpi: u32,
    pub image: PageImage,
    /// "Images to PDF".
    pub page: PageOptions,
}

/// How "Split PDF" cuts, as its buttons offer it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitChoice {
    EachPage,
    Every,
    Ranges,
}

impl PdfChoices {
    pub const DEFAULT: PdfChoices = PdfChoices {
        op: PdfOp::Split,
        split: SplitChoice::EachPage,
        every: 10,
        dpi: 150,
        image: PageImage::Png,
        page: PageOptions::DEFAULT,
    };
}

/// The largest "Every N pages" typed (a document has fewer pages anyway).
pub const MAX_EVERY: u32 = 1_000_000;

const OPS: [(PdfOp, &str); 5] = [
    (PdfOp::ImagesToPdf, "images-to-pdf"),
    (PdfOp::Merge, "merge"),
    (PdfOp::Split, "split"),
    (PdfOp::ToImages, "to-images"),
    (PdfOp::Extract, "extract"),
];
const SPLITS: [(SplitChoice, &str); 3] =
    [(SplitChoice::EachPage, "each"), (SplitChoice::Every, "every"), (SplitChoice::Ranges, "ranges")];
const IMAGES: [(PageImage, &str); 2] = [(PageImage::Png, "png"), (PageImage::Jpeg, "jpeg")];
pub const SIZES: [(PageSize, &str); 3] =
    [(PageSize::Picture, "picture"), (PageSize::A4, "a4"), (PageSize::Letter, "letter")];
pub const MARGINS: [(Margin, &str); 3] = [(Margin::None, "none"), (Margin::Small, "small"), (Margin::Large, "large")];

fn key_of<T: PartialEq + Copy>(table: &[(T, &'static str)], value: T) -> &'static str {
    table.iter().find(|(v, _)| *v == value).map_or("", |(_, key)| key)
}

fn value_of<T: Copy>(table: &[(T, &'static str)], key: &str) -> Option<T> {
    table.iter().find(|(_, k)| *k == key).map(|(v, _)| *v)
}

/// The choices as state.toml keeps them: `op=split split=every every=10 dpi=150 image=png
/// size=a4 margin=small`.
pub fn choices_text(c: &PdfChoices) -> String {
    format!(
        "op={} split={} every={} dpi={} image={} size={} margin={}",
        key_of(&OPS, c.op),
        key_of(&SPLITS, c.split),
        c.every,
        c.dpi,
        key_of(&IMAGES, c.image),
        key_of(&SIZES, c.page.size),
        key_of(&MARGINS, c.page.margin),
    )
}

/// The choices read back from [`choices_text`]; unknown keys and bad values stay as in `base`.
pub fn choices_from(text: &str, base: PdfChoices) -> PdfChoices {
    let mut c = base;
    for (key, value) in text.split_whitespace().filter_map(|pair| pair.split_once('=')) {
        match key {
            "op" => c.op = value_of(&OPS, value).unwrap_or(c.op),
            "split" => c.split = value_of(&SPLITS, value).unwrap_or(c.split),
            "every" => {
                if let Some(n) = value.parse::<u32>().ok().filter(|n| (1..=MAX_EVERY).contains(n)) {
                    c.every = n;
                }
            }
            "dpi" => {
                if let Some(dpi) = value.parse::<u32>().ok().filter(|d| RENDER_DPIS.contains(d)) {
                    c.dpi = dpi;
                }
            }
            "image" => c.image = value_of(&IMAGES, value).unwrap_or(c.image),
            "size" => c.page.size = value_of(&SIZES, value).unwrap_or(c.page.size),
            "margin" => c.page.margin = value_of(&MARGINS, value).unwrap_or(c.page.margin),
            _ => {}
        }
    }
    c
}

fn name_of(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Whether `op` takes the file `name`: pictures Gezik reads itself for "Images to PDF", PDFs
/// for the others.
pub fn takes(op: PdfOp, name: &str) -> bool {
    if op == PdfOp::ImagesToPdf { is_pdf_picture(name) } else { is_pdf(name) }
}

/// The items `op` takes (in their order) and how many it leaves out.
pub fn pdf_inputs(items: &[(PathBuf, bool)], op: PdfOp) -> (Vec<PathBuf>, usize) {
    let taken: Vec<PathBuf> =
        items.iter().filter(|(path, is_dir)| !is_dir && takes(op, &name_of(path))).map(|(p, _)| p.clone()).collect();
    let left = items.len() - taken.len();
    (taken, left)
}

/// The pictures and PDFs among `items`, for [`gezik_core::batch::pdf::ops_for`].
pub fn pdf_counts(items: &[(PathBuf, bool)]) -> (usize, usize) {
    let files = || items.iter().filter(|(_, is_dir)| !is_dir).map(|(p, _)| name_of(p));
    (files().filter(|n| is_pdf_picture(n)).count(), files().filter(|n| is_pdf(n)).count())
}

/// The footer's "3 items skipped: not PDFs"; empty for none.
pub fn skipped_text(count: usize, op: PdfOp) -> String {
    let (many, one) = if op == PdfOp::ImagesToPdf {
        ("not pictures Gezik reads itself", "not a picture Gezik reads itself")
    } else {
        ("not PDFs", "not a PDF")
    };
    match count {
        0 => String::new(),
        1 => format!("1 item skipped: {one}"),
        n => format!("{n} items skipped: {many}"),
    }
}

/// What merging, splitting and extracting leave behind (the layer says it under them).
pub const LOSSES: &str = "Bookmarks, forms and document info are not kept. An encrypted PDF gives an unencrypted copy.";

/// `20000` as "20,000".
pub fn grouped(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn plural(n: usize, one: &str, many: &str) -> String {
    if n == 1 { format!("1 {one}") } else { format!("{} {many}", grouped(n)) }
}

/// What the layer says before a split: "Makes 14 files", "Makes 37 files, more from 1
/// encrypted PDF". `counts`: each PDF's pages, `None` for one that needs a password; nothing
/// is said while no count is known (an empty `counts`: not counted). The ranges and "every"
/// are checked even then; with the counts, every range must be in each PDF.
pub fn files_note(split: &Split, counts: &[Option<u32>]) -> Result<String, String> {
    match split {
        Split::Every(0) => return Err("Every 0 pages: type 1 or more".to_owned()),
        Split::Ranges(text) => _ = parse_ranges(text)?,
        _ => {}
    }
    let mut files = 0usize;
    let mut known = 0usize;
    for pages in counts.iter().flatten() {
        files += split_file_count(split, *pages)?;
        known += 1;
    }
    if known == 0 {
        return Ok(String::new());
    }
    let mut note = format!("Makes {}", plural(files, "file", "files"));
    let encrypted = counts.len() - known;
    if encrypted > 0 {
        note.push_str(&format!(", more from {}", plural(encrypted, "encrypted PDF", "encrypted PDFs")));
    }
    Ok(note)
}

/// What the layer says before an extract: "Makes 1 PDF of 11 pages"; with several PDFs "Makes
/// 3 PDFs of 4 pages each" or "… of 2-4 pages". `counts` as for [`files_note`].
pub fn extract_note(ranges: &str, counts: &[Option<u32>]) -> Result<String, String> {
    parse_ranges(ranges)?;
    let mut sizes = Vec::new();
    for pages in counts.iter().flatten() {
        sizes.push(extract_pages(ranges, *pages)?.len());
    }
    let (Some(least), Some(most)) = (sizes.iter().min().copied(), sizes.iter().max().copied()) else {
        return Ok(String::new());
    };
    let pdfs = counts.len();
    let mut note = if pdfs == 1 {
        format!("Makes 1 PDF of {}", plural(least, "page", "pages"))
    } else if least == most && sizes.len() == pdfs {
        format!("Makes {} PDFs of {} each", grouped(pdfs), plural(least, "page", "pages"))
    } else if least == most {
        format!("Makes {} PDFs, {} each where known", grouped(pdfs), plural(least, "page", "pages"))
    } else {
        format!("Makes {} PDFs of {}-{} pages", grouped(pdfs), grouped(least), grouped(most))
    };
    let encrypted = pdfs - sizes.len();
    if encrypted > 0 {
        note.push_str(&format!(" (page count unknown for {})", plural(encrypted, "encrypted PDF", "encrypted PDFs")));
    }
    Ok(note)
}

/// What the layer says when no PDF could be counted because each needs a password.
pub fn unknown_note(counts: &[Option<u32>]) -> &'static str {
    if !counts.is_empty() && counts.iter().all(Option::is_none) { "Page count unknown (encrypted PDF)" } else { "" }
}

/// `note` with what it leaves out said: "Makes 3 files. 1 PDF could not be read" (`unread`:
/// PDFs whose pages could not be counted, damaged or not PDFs at all).
pub fn with_unread(note: &str, unread: usize) -> String {
    let unread_text = match unread {
        0 => return note.to_owned(),
        1 => "1 PDF could not be read".to_owned(),
        n => format!("{} PDFs could not be read", grouped(n)),
    };
    if note.is_empty() { unread_text } else { format!("{note}. {unread_text}") }
}

/// What to do when pdfium fails to load in a job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfiumFailed {
    /// Offer to download it again (the first time).
    Download,
    /// It was downloaded again and still does not load: say so, no download.
    Explain,
}

/// After a fresh download (`resubmitted`) a second failure gets no more downloads.
pub fn after_pdfium_failed(resubmitted: bool) -> PdfiumFailed {
    if resubmitted { PdfiumFailed::Explain } else { PdfiumFailed::Download }
}

/// The longest reason the plain box quotes.
const REASON_MAX: usize = 160;

/// A short reason from "pdfium could not be loaded (…)": the loader's own words (`desc:
/// "…"`) when the text has them, else what is in the brackets, at most 160 characters.
pub fn pdfium_reason(message: &str) -> String {
    let inner = message
        .split_once("pdfium could not be loaded (")
        .map_or(message, |(_, rest)| rest.strip_suffix(')').unwrap_or(rest));
    let reason =
        inner.split_once("desc: \"").and_then(|(_, rest)| rest.split_once('"')).map_or(inner, |(desc, _)| desc).trim();
    let reason = if reason.is_empty() { "unknown error" } else { reason };
    if reason.chars().count() > REASON_MAX {
        format!("{}…", reason.chars().take(REASON_MAX).collect::<String>())
    } else {
        reason.to_owned()
    }
}

/// The plain box after a second failure: "pdfium does not load on this system: <reason>".
pub fn pdfium_explained(message: &str) -> String {
    format!("pdfium does not load on this system: {}", pdfium_reason(message))
}

/// The row dragged from `from` lands at `to` (the others shift).
pub fn move_in_order(order: &mut Vec<PathBuf>, from: usize, to: usize) {
    if from >= order.len() || to >= order.len() || from == to {
        return;
    }
    let item = order.remove(from);
    order.insert(to, item);
}

/// The worker's "pdfium could not be loaded (…)": the library is there and broken.
pub fn says_pdfium_failed(message: &str) -> bool {
    message.contains("pdfium could not be loaded")
}

/// What a row's details say instead of pdfium's own text when it could not be loaded.
pub const PDFIUM_FAILED: &str = "Could not load pdfium. Download it again?";

/// A failure's message as the panel's details show it.
pub fn failure_shown(message: &str) -> &str {
    if says_pdfium_failed(message) { PDFIUM_FAILED } else { message }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str) -> (PathBuf, bool) {
        (PathBuf::from("d").join(name), false)
    }

    fn folder(name: &str) -> (PathBuf, bool) {
        (PathBuf::from("d").join(name), true)
    }

    #[test]
    fn choices_round_trip_through_state() {
        let c = PdfChoices {
            op: PdfOp::Split,
            split: SplitChoice::Every,
            every: 7,
            dpi: 300,
            image: PageImage::Jpeg,
            page: PageOptions { size: PageSize::A4, margin: Margin::Small },
        };
        assert_eq!(choices_from(&choices_text(&c), PdfChoices::DEFAULT), c);
        assert_eq!(choices_from("dpi=999 every=0 nonsense", PdfChoices::DEFAULT), PdfChoices::DEFAULT);
        assert_eq!(
            choices_text(&PdfChoices::DEFAULT),
            "op=split split=each every=10 dpi=150 image=png size=picture margin=none"
        );
        for op in PdfOp::ALL {
            let c = PdfChoices { op, ..PdfChoices::DEFAULT };
            assert_eq!(choices_from(&choices_text(&c), PdfChoices::DEFAULT).op, op);
        }
        let c = choices_from("op=merge size=letter margin=large split=ranges", PdfChoices::DEFAULT);
        assert_eq!(
            (c.op, c.page.size, c.page.margin, c.split),
            (PdfOp::Merge, PageSize::Letter, Margin::Large, SplitChoice::Ranges)
        );
    }

    #[test]
    fn inputs_are_taken_by_operation() {
        let items = vec![file("a.pdf"), file("b.PDF"), file("c.jpg"), file("d.heic"), folder("e")];
        let d = |name: &str| PathBuf::from("d").join(name);
        assert_eq!(pdf_inputs(&items, PdfOp::Merge), (vec![d("a.pdf"), d("b.PDF")], 3));
        assert_eq!(pdf_inputs(&items, PdfOp::ImagesToPdf), (vec![d("c.jpg")], 4));
        assert_eq!(skipped_text(4, PdfOp::ImagesToPdf), "4 items skipped: not pictures Gezik reads itself");
        assert_eq!(skipped_text(1, PdfOp::Split), "1 item skipped: not a PDF");
        assert_eq!(skipped_text(3, PdfOp::Extract), "3 items skipped: not PDFs");
        assert_eq!(skipped_text(0, PdfOp::Merge), "");
        assert_eq!(pdf_counts(&items), (1, 2));
        // A folder named like a PDF is not one.
        assert_eq!(pdf_counts(&[folder("x.pdf")]), (0, 0));
    }

    #[test]
    fn the_layer_says_how_many_files_come_out() {
        assert_eq!(files_note(&Split::EachPage, &[Some(20_000)]).unwrap(), "Makes 20,000 files");
        assert_eq!(
            files_note(&Split::Every(5), &[Some(14), None]).unwrap(),
            "Makes 3 files, more from 1 encrypted PDF"
        );
        assert_eq!(
            files_note(&Split::Ranges("1-99".into()), &[Some(14)]).unwrap_err(),
            "Page 99 is past the end (14 pages)"
        );
        assert_eq!(files_note(&Split::EachPage, &[None]).unwrap(), "");
        assert_eq!(extract_note("1-3, 5", &[Some(14)]).unwrap(), "Makes 1 PDF of 4 pages");

        assert_eq!(files_note(&Split::EachPage, &[Some(1)]).unwrap(), "Makes 1 file");
        assert_eq!(files_note(&Split::Every(10), &[Some(14), Some(3)]).unwrap(), "Makes 3 files");
        assert_eq!(files_note(&Split::Ranges("1-3, 5".into()), &[Some(14)]).unwrap(), "Makes 2 files");
        // Not counted (no pdfium yet): only the typing is checked.
        assert_eq!(files_note(&Split::Ranges("1-99".into()), &[]).unwrap(), "");
        assert_eq!(files_note(&Split::Ranges("3-1".into()), &[]).unwrap_err(), "3-1 runs backwards");
        assert!(files_note(&Split::Every(0), &[]).is_err());
        assert_eq!(files_note(&Split::EachPage, &[None, None]).unwrap(), "");
        assert_eq!(unknown_note(&[None]), "Page count unknown (encrypted PDF)");
        assert_eq!(unknown_note(&[None, Some(3)]), "");
        assert_eq!(unknown_note(&[]), "");

        assert_eq!(extract_note("2", &[Some(14)]).unwrap(), "Makes 1 PDF of 1 page");
        assert_eq!(extract_note("1-2", &[Some(14), Some(3)]).unwrap(), "Makes 2 PDFs of 2 pages each");
        assert_eq!(extract_note("2-", &[Some(14), Some(3)]).unwrap(), "Makes 2 PDFs of 2-13 pages");
        assert_eq!(
            extract_note("1", &[Some(3), None]).unwrap(),
            "Makes 2 PDFs, 1 page each where known (page count unknown for 1 encrypted PDF)"
        );
        assert_eq!(extract_note("5", &[Some(3)]).unwrap_err(), "Page 5 is past the end (3 pages)");
        assert_eq!(extract_note("", &[]).unwrap_err(), "Type the pages, e.g. 1-3, 5, 8-");
        assert_eq!(extract_note("1", &[None]).unwrap(), "");
        assert_eq!(grouped(1_234_567), "1,234,567");
        assert_eq!(grouped(999), "999");
    }

    #[test]
    fn the_order_follows_the_drag() {
        let mut order: Vec<PathBuf> = ["a", "b", "c"].iter().map(PathBuf::from).collect();
        move_in_order(&mut order, 0, 2);
        assert_eq!(order, ["b", "c", "a"].iter().map(PathBuf::from).collect::<Vec<_>>());
        move_in_order(&mut order, 2, 0);
        assert_eq!(order, ["a", "b", "c"].iter().map(PathBuf::from).collect::<Vec<_>>());
        move_in_order(&mut order, 1, 9);
        assert_eq!(order, ["a", "b", "c"].iter().map(PathBuf::from).collect::<Vec<_>>());
    }

    #[test]
    fn unreadable_pdfs_are_said() {
        assert_eq!(with_unread("Makes 3 files", 1), "Makes 3 files. 1 PDF could not be read");
        assert_eq!(with_unread("", 2), "2 PDFs could not be read");
        assert_eq!(with_unread("Makes 3 files", 0), "Makes 3 files");
    }

    #[test]
    fn a_second_load_failure_offers_no_download() {
        assert_eq!(after_pdfium_failed(false), PdfiumFailed::Download);
        assert_eq!(after_pdfium_failed(true), PdfiumFailed::Explain);
        let raw =
            "pdfium could not be loaded (LoadLibraryError(DlOpen { desc: \"/x/libpdfium.so: invalid ELF header\" }))";
        assert_eq!(pdfium_explained(raw), "pdfium does not load on this system: /x/libpdfium.so: invalid ELF header");
        assert_eq!(pdfium_reason("pdfium could not be loaded (Unknown)"), "Unknown");
        assert_eq!(pdfium_reason("pdfium could not be loaded ()"), "unknown error");
        let long = format!("pdfium could not be loaded ({})", "x".repeat(300));
        assert_eq!(pdfium_reason(&long).chars().count(), REASON_MAX + 1);
    }

    #[test]
    fn a_broken_pdfium_reads_well() {
        let raw = "the PDF is damaged (x)";
        assert_eq!(failure_shown(raw), raw);
        let library = "pdfium could not be loaded (LoadLibraryError(DlOpen { desc: \"x\" }))";
        assert!(says_pdfium_failed(library));
        assert_eq!(failure_shown(library), PDFIUM_FAILED);
    }
}
