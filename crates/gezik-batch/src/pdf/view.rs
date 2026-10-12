//! The PDF view worker: what `gezik --pdf-view` runs for the preview (spec 13 §5.1). Unlike the
//! one-request worker it stays: it greets, then answers one command per line until its input
//! closes or says `quit` (Gezik ends it 10 s after the preview leaves PDFs). pdfium is bound
//! once; one document is open at a time; a page is drawn only where asked, never larger than
//! [`MAX_SIDE`] a side.
//!
//! A password is never echoed: commands are not shown with `Debug`, a password with a NUL (on
//! which pdfium-render would panic) is answered as wrong, and a panic prints its place only.

use std::cell::OnceCell;
use std::io::{BufRead, Read, Write};
use std::path::PathBuf;

use gezik_core::batch::pdf_view::{Command, HELLO, MAX_COMMAND, MAX_SIDE, Reply, write_reply};
use pdfium_render::prelude::{PdfDocument, Pdfium};

use super::pdfium::{OpenError, bind, open, render_region};

/// The argument that starts Gezik as the preview's PDF worker.
pub const VIEW_ARG: &str = "--pdf-view";

/// The worker's whole life on stdin and stdout; the exit code (0, or 2 for a broken input).
pub fn main() -> i32 {
    gezik_platform::process::quiet_crashes();
    std::panic::set_hook(Box::new(|info| {
        let place = info.location().map_or_else(String::new, |l| format!(" at {}:{}", l.file(), l.line()));
        eprintln!("the PDF view worker panicked{place}");
    }));
    serve(&mut std::io::stdin().lock(), &mut std::io::stdout().lock())
}

/// The next command line without its break; `None` at the end of the input; an error for a
/// line over [`MAX_COMMAND`] or not UTF-8.
fn next_line(input: &mut dyn BufRead) -> Result<Option<String>, ()> {
    let mut line = Vec::new();
    let read = input.take(MAX_COMMAND as u64 + 1).read_until(b'\n', &mut line).map_err(|_| ())?;
    if read == 0 {
        return Ok(None);
    }
    if line.pop() != Some(b'\n') {
        return Err(());
    }
    String::from_utf8(line).map(Some).map_err(|_| ())
}

/// Answers commands from `input` on `out` until the input ends; the exit code.
pub fn serve(input: &mut dyn BufRead, out: &mut dyn Write) -> i32 {
    if writeln!(out, "{HELLO}").and_then(|()| out.flush()).is_err() {
        return 0;
    }
    let pdfium: OnceCell<Pdfium> = OnceCell::new();
    let mut file: Option<PathBuf> = None;
    let mut doc: Option<PdfDocument> = None;
    loop {
        let line = match next_line(input) {
            Ok(Some(line)) => line,
            Ok(None) => return 0,
            Err(()) => return 2,
        };
        let reply = match Command::parse(&line) {
            Err(why) => Reply::Failed(format!("bad command: {why}")),
            Ok(Command::Quit) => return 0,
            Ok(Command::Close) => {
                (doc, file) = (None, None);
                continue;
            }
            Ok(Command::Open { library, pdf }) => {
                doc = None;
                file = Some(pdf);
                if pdfium.get().is_none() {
                    match bind(&library) {
                        Ok(bound) => _ = pdfium.set(bound),
                        Err(why) => {
                            if write_reply(out, &Reply::Failed(format!("pdfium could not be loaded ({why})"))).is_err()
                            {
                                return 0;
                            }
                            continue;
                        }
                    }
                }
                opened(pdfium.get(), file.as_ref(), None, &mut doc)
            }
            Ok(Command::Password(text)) if text.contains('\0') => Reply::WrongPassword,
            Ok(Command::Password(text)) => opened(pdfium.get(), file.as_ref(), Some(&text), &mut doc),
            Ok(Command::Render { page, scale, x, y, w, h }) => match &doc {
                Some(doc) => render(doc, page, scale, (x, y, w, h)),
                None => Reply::Failed("no document is open".into()),
            },
        };
        if write_reply(out, &reply).is_err() {
            return 0;
        }
    }
}

/// Opens `file` into `doc`; the reply says how it went.
fn opened<'a>(
    pdfium: Option<&'a Pdfium>,
    file: Option<&PathBuf>,
    password: Option<&str>,
    doc: &mut Option<PdfDocument<'a>>,
) -> Reply {
    *doc = None;
    let (Some(pdfium), Some(file)) = (pdfium, file) else { return Reply::Failed("no document is open".into()) };
    match open(pdfium, file, password) {
        Ok(opened) => {
            let pages = u32::try_from(opened.pages().len()).unwrap_or(0);
            if pages == 0 {
                return Reply::Failed("it has no pages".into());
            }
            *doc = Some(opened);
            Reply::Pages(pages)
        }
        Err(OpenError::NeedsPassword) => Reply::NeedsPassword,
        Err(OpenError::WrongPassword) => Reply::WrongPassword,
        Err(OpenError::Damaged) => Reply::Failed("pdfium cannot read it".into()),
        Err(OpenError::Other(why)) => Reply::Failed(why),
    }
}

/// Where the asked region of a `page_w` × `page_h` point page lands: the scale and the
/// region `(x, y, w, h)` of the scaled page, clipped to it (`scale` 0: the whole page fitted
/// into `w` × `h`). Never larger than asked, never empty.
pub fn region(page_w: f32, page_h: f32, scale: f32, (x, y, w, h): (u32, u32, u32, u32)) -> (f32, (u32, u32, u32, u32)) {
    // Float to integer casts saturate: a huge page is clipped, never wraps.
    if scale == 0.0 {
        let s = (w as f32 / page_w).min(h as f32 / page_h);
        let fit = |side: f32, most: u32| ((side * s).round() as u32).clamp(1, most);
        return (s, (0, 0, fit(page_w, w), fit(page_h, h)));
    }
    let full_w = ((page_w * scale).round() as u32).max(1);
    let full_h = ((page_h * scale).round() as u32).max(1);
    let x = x.min(full_w - 1);
    let y = y.min(full_h - 1);
    (scale, (x, y, w.min(full_w - x), h.min(full_h - y)))
}

fn render(doc: &PdfDocument, page: u32, scale: f32, asked: (u32, u32, u32, u32)) -> Reply {
    let Some(index) = page.checked_sub(1).and_then(|i| i32::try_from(i).ok()) else {
        return Reply::Failed(format!("there is no page {page}"));
    };
    let shown = match doc.pages().get(index) {
        Ok(shown) => shown,
        Err(_) => return Reply::Failed(format!("there is no page {page}")),
    };
    let (page_w, page_h) = (shown.width().value, shown.height().value);
    if !(page_w.is_finite() && page_h.is_finite() && page_w > 0.0 && page_h > 0.0) {
        return Reply::Failed(format!("page {page} has no size"));
    }
    let (scale, (x, y, w, h)) = region(page_w, page_h, scale, asked);
    debug_assert!(w <= MAX_SIDE && h <= MAX_SIDE);
    match render_region(&shown, scale, (x, y, w, h)) {
        Ok(rgba) => Reply::Image { page, width: w, height: h, page_w, page_h, rgba },
        Err(why) => Reply::Failed(why),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gezik_core::batch::pdf_view::{read_hello, read_reply};

    fn served(lines: &str) -> (i32, Vec<u8>) {
        let mut out = Vec::new();
        let code = serve(&mut lines.as_bytes(), &mut out);
        (code, out)
    }

    #[test]
    fn it_greets_answers_bad_commands_and_ends_with_its_input() {
        let (code, out) = served("hello\nrender\t1\t1\t0\t0\t10\t10\nclose\n");
        assert_eq!(code, 0);
        let mut out = &out[..];
        read_hello(&mut out).unwrap();
        assert!(matches!(read_reply(&mut out).unwrap(), Reply::Failed(m) if m.starts_with("bad command")));
        assert!(matches!(read_reply(&mut out).unwrap(), Reply::Failed(m) if m == "no document is open"));
        assert!(out.is_empty(), "close says nothing");
    }

    #[test]
    fn quit_ends_it_and_a_long_line_is_a_broken_input() {
        let (code, out) = served("quit\nclose\n");
        assert_eq!((code, out.len()), (0, HELLO.len() + 1));
        assert_eq!(served(&"x".repeat(MAX_COMMAND + 10)).0, 2);
        assert_eq!(served("close").0, 2, "a line without its break");
    }

    #[test]
    fn a_library_that_does_not_load_says_so_and_a_nul_password_is_wrong() {
        let open = Command::Open { library: "no-such-pdfium".into(), pdf: "a.pdf".into() }.to_line();
        let password = Command::Password("se\0cret".into()).to_line();
        let (_, out) = served(&format!("{open}\n{password}\n"));
        let mut out = &out[..];
        read_hello(&mut out).unwrap();
        assert!(
            matches!(read_reply(&mut out).unwrap(), Reply::Failed(m) if m.starts_with("pdfium could not be loaded"))
        );
        assert_eq!(read_reply(&mut out).unwrap(), Reply::WrongPassword);
        assert!(!String::from_utf8_lossy(out).contains("cret"));
    }

    #[test]
    fn the_region_is_clipped_to_the_page_and_never_larger_than_asked() {
        // A Letter page fitted into 400 × 400: 400 tall, 309 wide.
        let (s, r) = region(612.0, 792.0, 0.0, (0, 0, 400, 400));
        assert!((s - 400.0 / 792.0).abs() < 1e-6);
        assert_eq!(r, (0, 0, 309, 400));
        // At 2 px per point the page is 1224 × 1584; a region past its end is cut.
        assert_eq!(region(612.0, 792.0, 2.0, (1000, 1500, 800, 600)).1, (1000, 1500, 224, 84));
        assert_eq!(region(612.0, 792.0, 2.0, (5000, 5000, 800, 600)).1, (1223, 1583, 1, 1));
        // A huge page at a large scale stays within what was asked.
        let (_, (_, _, w, h)) = region(1e9, 1e9, 64.0, (u32::MAX, 0, MAX_SIDE, MAX_SIDE));
        assert!(w <= MAX_SIDE && h <= MAX_SIDE && w >= 1);
        let (_, (_, _, w, h)) = region(0.01, 1e9, 0.0, (0, 0, 10, 10));
        assert!((1..=10).contains(&w) && (1..=10).contains(&h));
    }
}
