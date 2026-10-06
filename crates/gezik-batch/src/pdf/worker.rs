//! The PDF worker: what `gezik --pdf-worker` runs (and `examples/pdf_worker.rs` for the tests).
//! It reads one [`Request`] from its input, does it with pdfium, and answers on its output one
//! [`Reply`] per line, each flushed at once. It behaves as `examples/fake_pdf_worker.rs` does
//! for the same request: every input is opened first (a password question or damage ends it
//! there), then the job reports `steps` and a `step` per page, and `done` comes last.
//!
//! Exit codes: 0 done or a password asked, 1 failed, 2 a bad request.
//!
//! A password never reaches an error: the request is never shown with `Debug` here, a
//! password pdfium-render would panic on (one with a NUL) is answered as wrong before pdfium
//! loads, and a panic prints a fixed text with its place only, never its payload.

use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use gezik_core::batch::pdf::{extract_pages, merged_name, page_image_name, part_name, split_parts};
use gezik_core::batch::pdf_worker::{Failure, Reply, Request, WorkerJob};
use gezik_core::ops::names::next_free;
use pdfium_render::prelude::{PdfDocument, PdfiumError};

use super::pdfium::{OpenError, bind, open, pages_to_new, render_page};

/// The highest dpi "PDF to images" takes. The Convert layer offers 72, 150 and 300; anything
/// beyond this is a mistake, not a wish (the 64 MP limit would lower it anyway).
pub const MAX_DPI: u32 = 2400;

/// The worker's whole life: reads the request from stdin, does it, writes replies to stdout;
/// the exit code (0 done or a password asked, 1 failed, 2 a bad request).
pub fn main() -> i32 {
    gezik_platform::process::quiet_crashes();
    // The default hook prints the payload, which could quote what a library was given (a
    // password among it). The place is enough to find the bug.
    std::panic::set_hook(Box::new(|info| {
        let place = info.location().map_or_else(String::new, |l| format!(" at {}:{}", l.file(), l.line()));
        eprintln!("the PDF worker panicked{place}");
    }));
    let mut bytes = Vec::new();
    let read = std::io::stdin().lock().read_to_end(&mut bytes);
    let mut out = std::io::stdout().lock();
    match (read, String::from_utf8(bytes)) {
        (Ok(_), Ok(text)) => serve(&text, &mut out),
        (Err(e), _) => bad_request(&mut out, &format!("cannot read it ({e})")),
        (_, Err(_)) => bad_request(&mut out, "it is not UTF-8"),
    }
}

fn say(out: &mut dyn Write, reply: &Reply) {
    // A parent that has gone away cannot be told anything; the work ends with the process.
    let _ = writeln!(out, "{}", reply.to_line());
    let _ = out.flush();
}

fn bad_request(out: &mut dyn Write, why: &str) -> i32 {
    say(out, &Reply::Failed { input: None, why: Failure::Other, message: format!("bad request: {why}") });
    2
}

fn fail(out: &mut dyn Write, input: Option<usize>, why: Failure, message: String) -> i32 {
    say(out, &Reply::Failed { input, why, message });
    1
}

/// What the parser lets through but no worker should do.
fn check(request: &Request) -> Result<(), String> {
    if let WorkerJob::Render { dpi, .. } = request.job
        && !(1..=MAX_DPI).contains(&dpi)
    {
        return Err(format!("dpi {dpi} is out of range (1 to {MAX_DPI})"));
    }
    Ok(())
}

/// The first input whose password pdfium-render cannot take: it makes a C string of it, and a
/// NUL inside panics with the password in the message. Such a password opens nothing anyway.
fn unusable_password(request: &Request) -> Option<usize> {
    (0..request.inputs.len()).find(|&i| request.password(i).is_some_and(|p| p.contains('\0')))
}

/// `dir/name`, or `dir/name (2)`… when an earlier output of this request took it (a split into
/// "1-2, 1-2", or two labels that both fall back to "N pages"). The same numbering as placing
/// ([`next_free`]).
fn free_name(dir: &Path, name: OsString) -> PathBuf {
    let taken = |name: &std::ffi::OsStr| std::fs::symlink_metadata(dir.join(name)).is_ok();
    if !taken(&name) {
        return dir.join(name);
    }
    if let Some(text) = name.to_str() {
        return dir.join(next_free(text, false, |candidate| taken(candidate.as_ref())));
    }
    // Not Unicode: worker names are "<stem> - <label>.pdf", whose label never ends in "(n)", so
    // `next_free` would give "<stem> - <label> (2).pdf"; the same, built on the OS string.
    let path = Path::new(&name);
    let (stem, ext) = (path.file_stem().unwrap_or_default(), path.extension());
    let mut n = 2u32;
    loop {
        let mut candidate = stem.to_os_string();
        candidate.push(format!(" ({n})"));
        if let Some(ext) = ext {
            candidate.push(".");
            candidate.push(ext);
        }
        if !taken(&candidate) || n == u32::MAX {
            return dir.join(candidate);
        }
        n += 1;
    }
}

/// "page N has no size" for the 0-based page `i` of `width` × `height` points, when it has none.
fn no_size(i: u32, width: f32, height: f32) -> Option<String> {
    (!(width > 0.0 && height > 0.0)).then(|| format!("page {} has no size", i + 1))
}

fn page_count(doc: &PdfDocument) -> u32 {
    u32::try_from(doc.pages().len()).unwrap_or(0)
}

/// Saves `doc` as `path`; the failure names the file.
fn save(doc: &PdfDocument, path: &Path) -> Result<(), String> {
    doc.save_to_file(path).map_err(|e| {
        let why = match e {
            PdfiumError::IoError(e) => e.to_string(),
            e => format!("{e:?}"),
        };
        format!("cannot write {}: {why}", path.file_name().unwrap_or_default().to_string_lossy())
    })
}

/// Does `request_text` (a [`Request`]), writing the replies to `out`; the exit code.
pub fn serve(request_text: &str, out: &mut dyn Write) -> i32 {
    let request = match Request::parse(request_text).and_then(|r| check(&r).map(|()| r)) {
        Ok(request) => request,
        Err(why) => return bad_request(out, &why),
    };
    if let Some(i) = unusable_password(&request) {
        say(out, &Reply::WrongPassword(i));
        return 0;
    }
    let pdfium = match bind(&request.library) {
        Ok(pdfium) => pdfium,
        Err(why) => return fail(out, None, Failure::Library, why),
    };
    let mut docs = Vec::with_capacity(request.inputs.len());
    for (i, input) in request.inputs.iter().enumerate() {
        match open(&pdfium, input, request.password(i)) {
            Ok(doc) => docs.push(doc),
            Err(OpenError::NeedsPassword) => {
                say(out, &Reply::NeedsPassword(i));
                return 0;
            }
            Err(OpenError::WrongPassword) => {
                say(out, &Reply::WrongPassword(i));
                return 0;
            }
            Err(OpenError::Damaged) => {
                return fail(out, Some(i), Failure::Damaged, "pdfium cannot read it".to_string());
            }
            Err(OpenError::Other(why)) => return fail(out, Some(i), Failure::Io, why),
        }
    }
    let input = &request.inputs[0];
    let dir = &request.dir;
    let first = &docs[0];
    let steps = |out: &mut dyn Write, n: u32| {
        for _ in 0..n {
            say(out, &Reply::Step);
        }
    };
    match &request.job {
        WorkerJob::Count => say(out, &Reply::Pages { input: 0, pages: page_count(first) }),
        WorkerJob::Merge => {
            say(out, &Reply::Steps(docs.iter().map(|d| u64::from(page_count(d))).sum()));
            let mut merged = match pdfium.create_new_pdf() {
                Ok(doc) => doc,
                Err(e) => return fail(out, None, Failure::Other, format!("{e:?}")),
            };
            for (i, doc) in docs.iter().enumerate() {
                let pages = page_count(doc);
                if pages > 0
                    && let Err(e) = merged.pages_mut().append(doc)
                {
                    return fail(out, Some(i), Failure::Other, format!("cannot copy its pages: {e:?}"));
                }
                steps(out, pages);
            }
            if let Err(why) = save(&merged, &free_name(dir, merged_name(input))) {
                return fail(out, None, Failure::Io, why);
            }
        }
        WorkerJob::Split(split) => {
            let parts = match split_parts(split, page_count(first)) {
                Ok(parts) => parts,
                Err(why) => return fail(out, Some(0), Failure::Ranges, why),
            };
            say(out, &Reply::Steps(parts.iter().map(|p| p.len() as u64).sum()));
            for part in &parts {
                let doc = match pages_to_new(&pdfium, first, part) {
                    Ok(doc) => doc,
                    Err(e) => return fail(out, Some(0), Failure::Other, format!("cannot copy its pages: {e:?}")),
                };
                if let Err(why) = save(&doc, &free_name(dir, part_name(input, part))) {
                    return fail(out, None, Failure::Io, why);
                }
                steps(out, part.len() as u32);
            }
        }
        WorkerJob::Extract(ranges) => {
            let pages = match extract_pages(ranges, page_count(first)) {
                Ok(pages) => pages,
                Err(why) => return fail(out, Some(0), Failure::Ranges, why),
            };
            say(out, &Reply::Steps(pages.len() as u64));
            let doc = match pages_to_new(&pdfium, first, &pages) {
                Ok(doc) => doc,
                Err(e) => return fail(out, Some(0), Failure::Other, format!("cannot copy its pages: {e:?}")),
            };
            steps(out, pages.len() as u32);
            if let Err(why) = save(&doc, &free_name(dir, part_name(input, &pages))) {
                return fail(out, None, Failure::Io, why);
            }
        }
        WorkerJob::Render { dpi, image } => {
            let pages = first.pages();
            say(out, &Reply::Steps(u64::from(page_count(first))));
            for (i, page) in pages.iter().enumerate() {
                let i = i as u32;
                if let Some(why) = no_size(i, page.width().value, page.height().value) {
                    return fail(out, Some(0), Failure::Other, why);
                }
                let path = dir.join(page_image_name(input, i, *image));
                match render_page(&page, *dpi, *image, &path) {
                    Ok(size) if size.lowered => {
                        say(out, &Reply::Lowered { page: i + 1, dpi: size.dpi.round() as u32 });
                    }
                    Ok(_) => {}
                    Err(e) => return fail(out, Some(0), Failure::Io, format!("page {}: {e}", i + 1)),
                }
                say(out, &Reply::Step);
            }
        }
    }
    say(out, &Reply::Done);
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use gezik_core::batch::pdf::PageImage;
    use std::path::PathBuf;

    fn served(text: &str) -> (i32, String) {
        let mut out = Vec::new();
        let code = serve(text, &mut out);
        (code, String::from_utf8(out).unwrap())
    }

    fn render(dpi: u32) -> String {
        Request {
            library: PathBuf::from("nope"),
            dir: PathBuf::from("."),
            job: WorkerJob::Render { dpi, image: PageImage::Png },
            inputs: vec![PathBuf::from("a.pdf")],
            passwords: Vec::new(),
        }
        .to_text()
    }

    #[test]
    fn bad_requests_end_before_pdfium_loads() {
        assert_eq!(served("hello"), (2, "failed\t-\tother\tbad request: not a gezik-pdf 1 request\n".into()));
        assert_eq!(
            served(&render(0)),
            (2, "failed\t-\tother\tbad request: dpi 0 is out of range (1 to 2400)\n".into())
        );
        assert_eq!(served(&render(MAX_DPI + 1)).0, 2);
    }

    #[test]
    fn a_password_with_a_nul_is_wrong_and_never_echoed() {
        // pdfium-render would panic on it (`CString::new(..).unwrap()`), with the password bytes
        // in the panic text; it is refused first, before pdfium even loads.
        let mut request = Request::parse(&render(72)).unwrap();
        request.job = WorkerJob::Merge;
        request.inputs.push(PathBuf::from("b.pdf"));
        request.passwords = vec![(0, "fine".into()), (1, "se\0cret".into())];
        let (code, text) = served(&request.to_text());
        assert_eq!((code, text.as_str()), (0, "wrong-password\t1\n"));
    }

    #[test]
    fn a_taken_name_gets_the_next_number() {
        let dir = std::env::temp_dir().join(format!("gezik-pdf-worker-free-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let name = std::ffi::OsString::from("a - pages 1-2.pdf");
        assert_eq!(free_name(&dir, name.clone()), dir.join("a - pages 1-2.pdf"));
        std::fs::write(dir.join(&name), b"").unwrap();
        assert_eq!(free_name(&dir, name.clone()), dir.join("a - pages 1-2 (2).pdf"));
        std::fs::write(dir.join("a - pages 1-2 (2).pdf"), b"").unwrap();
        assert_eq!(free_name(&dir, name), dir.join("a - pages 1-2 (3).pdf"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn a_taken_name_that_is_not_unicode_gets_a_number_too() {
        use std::os::windows::ffi::OsStringExt;
        let dir = std::env::temp_dir().join(format!("gezik-pdf-worker-free-w-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let stem = std::ffi::OsString::from_wide(&[0x72, 0xD800]);
        let mut name = stem.clone();
        name.push(" - page 1.pdf");
        std::fs::write(dir.join(&name), b"").unwrap();
        let mut want = stem;
        want.push(" - page 1 (2).pdf");
        assert_eq!(free_name(&dir, name), dir.join(want));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_page_without_a_size_says_so() {
        assert_eq!(no_size(2, 0.0, 10.0).as_deref(), Some("page 3 has no size"));
        assert_eq!(no_size(0, 10.0, f32::NAN).as_deref(), Some("page 1 has no size"));
        assert_eq!(no_size(0, 0.5, 0.5), None);
    }

    #[test]
    fn a_library_that_does_not_load_is_a_library_failure() {
        let (code, text) = served(&render(MAX_DPI));
        assert_eq!(code, 1);
        assert!(text.starts_with("failed\t-\tlibrary\t"), "{text}");
        assert_eq!(text.lines().count(), 1, "{text}");
    }
}
