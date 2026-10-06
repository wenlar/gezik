//! The parent's side of the PDF worker: pdfium runs in a separate process (`gezik
//! --pdf-worker`) so a crash in it, or its memory, never takes Gezik down. One [`Request`] goes
//! to the worker's input (the passwords with it: never on its command line), and its replies
//! come back line by line ([`Reply`]).
//!
//! What the worker says is bounded both ways: the request may be at most 4 MB, and a reply
//! line that reaches 16 KB (where `ChildProcess` cuts a line) ends the worker as a protocol
//! failure. Every text that comes from the worker (a failure's message, the end of its error
//! output) is cleaned of control characters here, before it reaches the caller or the UI.

use std::cell::{Cell, RefCell};
use std::ffi::OsString;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use gezik_core::batch::pdf_worker::{Failure, Reply, Request, WorkerJob};
use gezik_platform::ChildProcess;

use crate::convert::ffmpeg::stderr_tail;

/// The argument that starts Gezik as its PDF worker.
pub const WORKER_ARG: &str = "--pdf-worker";

/// The longest reply line read. `ChildProcess` passes at most 16 KB of a line on (it drops the
/// rest), so a line this long may have been cut: it is a protocol failure, never parsed.
/// Real replies are a few dozen bytes; a failure's message a few hundred.
const MAX_LINE: usize = 16 * 1024;

/// The largest request written to the worker's input (paths and passwords; a merge of a few
/// thousand files fits).
const MAX_REQUEST: usize = 4 * 1024 * 1024;

/// The program that does PDF work, and its arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worker {
    pub program: PathBuf,
    pub args: Vec<OsString>,
}

impl Worker {
    /// This program in worker mode (`current_exe() --pdf-worker`).
    pub fn this_exe() -> io::Result<Worker> {
        Ok(Worker { program: std::env::current_exe()?, args: vec![WORKER_ARG.into()] })
    }
}

/// How a request ended without failing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ended {
    Done,
    /// Input `n` is encrypted and no password was given for it.
    NeedsPassword(usize),
    /// The password given for input `n` does not open it.
    WrongPassword(usize),
}

/// The worker said it could not finish (a `failed` reply).
#[derive(Debug)]
pub struct PdfFailed {
    pub input: Option<usize>,
    pub why: Failure,
    /// The worker's message, without control characters.
    pub message: String,
}

impl fmt::Display for PdfFailed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.why {
            Failure::Damaged => write!(f, "the PDF is damaged ({})", self.message),
            Failure::Library => write!(f, "pdfium could not be loaded ({})", self.message),
            _ => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for PdfFailed {}

impl PdfFailed {
    fn kind(&self) -> io::ErrorKind {
        match self.why {
            Failure::Damaged => io::ErrorKind::InvalidData,
            Failure::Ranges => io::ErrorKind::InvalidInput,
            Failure::Library | Failure::Io | Failure::Other => io::ErrorKind::Other,
        }
    }
}

/// "PDF engine stopped (exit code 3)" or "PDF engine stopped (killed)", then ": " and the end
/// of its error output when there is one (control characters other than line breaks become
/// spaces).
pub fn engine_stopped_text(code: Option<i32>, tail: &str) -> String {
    let how = code.map_or_else(|| "killed".to_string(), |code| format!("exit code {code}"));
    let tail = clean(tail.trim(), true);
    if tail.is_empty() { format!("PDF engine stopped ({how})") } else { format!("PDF engine stopped ({how}): {tail}") }
}

/// `text` with every control character (and every bidirectional override, which can make a
/// name read backwards) turned into a space; line breaks stay when `keep_lines`.
fn clean(text: &str, keep_lines: bool) -> String {
    text.chars()
        .map(|c| {
            let bidi = matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{200E}' | '\u{200F}');
            if (c.is_control() && !(keep_lines && c == '\n')) || bidi { ' ' } else { c }
        })
        .collect()
}

/// What the replies read so far say.
struct Replies<'a> {
    on_reply: &'a mut dyn FnMut(&Reply),
    /// The first password question or failure.
    end: Option<Result<Ended, PdfFailed>>,
    done: bool,
    /// A line reached [`MAX_LINE`]: nothing after it is read.
    too_long: bool,
}

impl Replies<'_> {
    fn line(&mut self, line: String) {
        if self.too_long {
            return;
        }
        if line.len() >= MAX_LINE {
            self.too_long = true;
            return;
        }
        let Some(mut reply) = Reply::parse(&line) else { return };
        if let Reply::Failed { message, .. } = &mut reply {
            *message = clean(message, false);
        }
        match &reply {
            Reply::NeedsPassword(i) => _ = self.end.get_or_insert(Ok(Ended::NeedsPassword(*i))),
            Reply::WrongPassword(i) => _ = self.end.get_or_insert(Ok(Ended::WrongPassword(*i))),
            Reply::Failed { input, why, message } => {
                _ = self
                    .end
                    .get_or_insert_with(|| Err(PdfFailed { input: *input, why: *why, message: message.clone() }))
            }
            Reply::Done => self.done = true,
            _ => {}
        }
        (self.on_reply)(&reply);
    }
}

/// Runs one request: writes it to the worker's input (then closes it), passes every reply to
/// `on_reply` as it comes, and looks every 50 ms whether to `stop` (then the worker is ended
/// with all it started: `Interrupted`). A `failed` reply becomes the error (`PdfFailed`); an
/// exit without `done` (a crash) is "PDF engine stopped (…)" with the end of its error output.
///
/// The worker runs with no window (`ChildProcess`: `CREATE_NO_WINDOW` and a job object on
/// Windows, its own process group on Unix). A request over 4 MB is refused before the worker
/// starts; a reply line of 16 KB or more ends it (`InvalidData`).
pub fn run(
    worker: &Worker,
    request: &Request,
    on_reply: &mut dyn FnMut(&Reply),
    stop: &dyn Fn() -> bool,
) -> io::Result<Ended> {
    let text = request.to_text();
    if text.len() > MAX_REQUEST {
        // Never the request itself in the message: it holds the passwords.
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("too much to pass to the PDF engine at once ({} MB)", text.len() / (1024 * 1024)),
        ));
    }
    let mut child = ChildProcess::spawn_with_input(&worker.program, &worker.args, None, Some(text.into_bytes()))?;
    let mut lines = child.stdout_lines();
    let replies = RefCell::new(Replies { on_reply, end: None, done: false, too_long: false });
    let stopped = Cell::new(false);
    let status = {
        let read = || {
            let mut replies = replies.borrow_mut();
            for line in lines.ready() {
                replies.line(line);
            }
        };
        child.wait_or_stop(|| {
            read();
            if stop() {
                stopped.set(true);
                return true;
            }
            replies.borrow().too_long
        })?
    };
    if stopped.get() {
        return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
    }
    let mut replies = replies.into_inner();
    if let Some(status) = status {
        // It has ended: the rest of its output comes once the reader passes it on.
        for line in lines.by_ref() {
            replies.line(line);
        }
        if !replies.too_long {
            match replies.end {
                Some(Ok(ended)) => return Ok(ended),
                Some(Err(failed)) => return Err(io::Error::new(failed.kind(), failed)),
                None if replies.done && status.success() => return Ok(Ended::Done),
                None => {
                    let tail = stderr_tail(&child.stderr_text());
                    return Err(io::Error::other(engine_stopped_text(status.code(), &tail)));
                }
            }
        }
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        format!("PDF engine stopped: a reply line was too long (over {} KB)", MAX_LINE / 1024),
    ))
}

/// The page count of `input`; `None` when it needs a password.
pub fn count_pages(worker: &Worker, library: &Path, input: &Path, stop: &dyn Fn() -> bool) -> io::Result<Option<u32>> {
    let request = Request {
        library: library.to_path_buf(),
        dir: PathBuf::new(),
        job: WorkerJob::Count,
        inputs: vec![input.to_path_buf()],
        passwords: Vec::new(),
    };
    let mut pages = None;
    let ended = run(
        worker,
        &request,
        &mut |reply| {
            if let Reply::Pages { input: 0, pages: n } = reply {
                pages = Some(*n);
            }
        },
        stop,
    )?;
    match ended {
        Ended::Done => pages.map(Some).ok_or_else(|| io::Error::other("the PDF engine did not say how many pages")),
        Ended::NeedsPassword(_) | Ended::WrongPassword(_) => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_engine_stop_says_how_and_why() {
        assert_eq!(engine_stopped_text(Some(3), ""), "PDF engine stopped (exit code 3)");
        assert_eq!(engine_stopped_text(None, "  \n"), "PDF engine stopped (killed)");
        assert_eq!(
            engine_stopped_text(Some(101), "thread panicked\n\u{1b}[31mboom\r"),
            "PDF engine stopped (exit code 101): thread panicked\n [31mboom"
        );
    }

    #[test]
    fn failures_are_named_by_why() {
        let failed = |why, message: &str| PdfFailed { input: Some(0), why, message: message.into() }.to_string();
        assert_eq!(failed(Failure::Damaged, "bad xref"), "the PDF is damaged (bad xref)");
        assert_eq!(failed(Failure::Library, "LoadLibraryError"), "pdfium could not be loaded (LoadLibraryError)");
        assert_eq!(failed(Failure::Ranges, "Page 9 is past the end"), "Page 9 is past the end");
        assert_eq!(failed(Failure::Io, "access denied"), "access denied");
    }

    #[test]
    fn replies_are_cleaned_and_a_long_line_stops_reading() {
        let mut seen = Vec::new();
        let mut on_reply = |r: &Reply| seen.push(r.clone());
        let mut replies = Replies { on_reply: &mut on_reply, end: None, done: false, too_long: false };
        replies.line("failed\t0\tio\ta\\tb\\x1b\u{202E}c".into());
        replies.line("wrong-password\t0".into());
        replies.line("x".repeat(MAX_LINE));
        replies.line("done".into());
        assert!(replies.too_long && !replies.done);
        let Some(Err(failed)) = &replies.end else { panic!("no failure") };
        assert_eq!(failed.message, "a b  c");
        assert_eq!(seen.len(), 2);
    }
}
