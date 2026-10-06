//! A stand-in for `gezik --pdf-worker` in `tests/pdf_client.rs` and the PDF task tests (built
//! by `cargo test` with the other examples). Not shipped.
//!
//! It reads the request from its input like the real worker and answers in the same protocol.
//! Next to every input it writes its arguments, one per line, to `<input>.args` and adds its
//! process id as a line to `<input>.log`. Every input file is a script, one word per line:
//!
//! - `pages N`: the page count (3 without it);
//! - `password P`: it is encrypted with `P`;
//! - `damaged`: opening it fails as damaged (with control characters in the message);
//! - `crash`: after the first step (or the page count), exit code 101 without `done`;
//! - `hang`: after the steps, wait until killed;
//! - `hold`: after opening, wait while `<input>.hold` exists;
//! - `slow MS`: wait MS milliseconds after each step;
//! - `lowered PAGE DPI`: rendering page PAGE (1-based) reports `lowered PAGE DPI`;
//! - `long`: after opening, write a reply line of 70 000 bytes;
//! - `stall`: after writing every output, wait while `<input>.hold` exists;
//! - `late-failure`: after writing every output, fail (exit code 1) instead of `done`.
//!
//! Inputs are all opened first (a missing password: `needs-password i`, a wrong one:
//! `wrong-password i`, both exit 0; damage: `failed i damaged …`, exit 1); then the job writes
//! `steps`/`step` and its outputs into the request's folder under their real names, each
//! holding `fake <job> <pages>`; `count` answers `pages 0 N`. Last comes `done`.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use gezik_core::batch::pdf::{extract_pages, merged_name, page_image_name, part_name, split_parts};
use gezik_core::batch::pdf_worker::{Failure, Reply, Request, WorkerJob};

#[derive(Default)]
struct Script {
    pages: u32,
    password: Option<String>,
    damaged: bool,
    crash: bool,
    hang: bool,
    hold: bool,
    slow: u64,
    lowered: Vec<(u32, u32)>,
    long: bool,
    stall: bool,
    late_failure: bool,
}

fn script(input: &Path) -> Script {
    let mut s = Script { pages: 3, ..Script::default() };
    for line in std::fs::read_to_string(input).unwrap_or_default().lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        match words.as_slice() {
            ["pages", n] => s.pages = n.parse().unwrap(),
            ["password", p] => s.password = Some((*p).to_string()),
            ["damaged"] => s.damaged = true,
            ["crash"] => s.crash = true,
            ["hang"] => s.hang = true,
            ["hold"] => s.hold = true,
            ["slow", ms] => s.slow = ms.parse().unwrap(),
            ["lowered", page, dpi] => s.lowered.push((page.parse().unwrap(), dpi.parse().unwrap())),
            ["long"] => s.long = true,
            ["stall"] => s.stall = true,
            ["late-failure"] => s.late_failure = true,
            _ => {}
        }
    }
    s
}

fn reply(reply: &Reply) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{}", reply.to_line());
    let _ = out.flush();
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

fn crash() -> ! {
    eprintln!("fake worker crashed\u{1b}[31m in red\r");
    std::process::exit(101);
}

fn main() {
    let mut text = String::new();
    let _ = std::io::stdin().read_to_string(&mut text);
    let request = match Request::parse(&text) {
        Ok(request) => request,
        Err(why) => {
            reply(&Reply::Failed { input: None, why: Failure::Other, message: format!("bad request: {why}") });
            std::process::exit(2);
        }
    };
    let args: Vec<String> = std::env::args().collect();
    let scripts: Vec<Script> = request.inputs.iter().map(|input| script(input)).collect();
    for input in &request.inputs {
        let _ = std::fs::write(with_suffix(input, ".args"), args.join("\n") + "\n");
        let mut log = std::fs::OpenOptions::new().create(true).append(true).open(with_suffix(input, ".log")).unwrap();
        let _ = writeln!(log, "{}", std::process::id());
    }
    for (i, s) in scripts.iter().enumerate() {
        if let Some(password) = &s.password {
            match request.password(i) {
                None => {
                    reply(&Reply::NeedsPassword(i));
                    std::process::exit(0);
                }
                Some(given) if given != password => {
                    reply(&Reply::WrongPassword(i));
                    std::process::exit(0);
                }
                Some(_) => {}
            }
        }
        if s.damaged {
            let message = "bad xref\ttable\u{1b}[2J\nat offset 0".to_string();
            reply(&Reply::Failed { input: Some(i), why: Failure::Damaged, message });
            std::process::exit(1);
        }
    }
    for (input, s) in request.inputs.iter().zip(&scripts) {
        let hold = with_suffix(input, ".hold");
        while s.hold && hold.exists() {
            std::thread::sleep(Duration::from_millis(20));
        }
        if s.long {
            println!("{}", "x".repeat(70_000));
        }
    }
    let crashes = scripts.iter().any(|s| s.crash);
    let slow = scripts.iter().map(|s| s.slow).max().unwrap_or(0);
    let step = || {
        reply(&Reply::Step);
        if crashes {
            crash();
        }
        std::thread::sleep(Duration::from_millis(slow));
    };
    let ranges_failed = |message: String| -> ! {
        reply(&Reply::Failed { input: Some(0), why: Failure::Ranges, message });
        std::process::exit(1);
    };
    let write = |name: std::ffi::OsString, job: &str, pages: usize| {
        std::fs::write(request.dir.join(name), format!("fake {job} {pages}")).unwrap();
    };
    let input = &request.inputs[0];
    let first = &scripts[0];
    match &request.job {
        WorkerJob::Count => {
            reply(&Reply::Pages { input: 0, pages: first.pages });
            if crashes {
                crash();
            }
        }
        WorkerJob::Merge => {
            let total: u32 = scripts.iter().map(|s| s.pages).sum();
            reply(&Reply::Steps(u64::from(total)));
            for s in &scripts {
                for _ in 0..s.pages {
                    step();
                }
            }
            write(merged_name(input), "merge", total as usize);
        }
        WorkerJob::Split(split) => {
            let parts = split_parts(split, first.pages).unwrap_or_else(|e| ranges_failed(e));
            reply(&Reply::Steps(parts.iter().map(|p| p.len() as u64).sum()));
            for part in &parts {
                write(part_name(input, part), "split", part.len());
                for _ in part {
                    step();
                }
            }
        }
        WorkerJob::Extract(ranges) => {
            let pages = extract_pages(ranges, first.pages).unwrap_or_else(|e| ranges_failed(e));
            reply(&Reply::Steps(pages.len() as u64));
            for _ in &pages {
                step();
            }
            write(part_name(input, &pages), "extract", pages.len());
        }
        WorkerJob::Render { image, .. } => {
            reply(&Reply::Steps(u64::from(first.pages)));
            for page in 0..first.pages {
                write(page_image_name(input, page, *image), "render", 1);
                if let Some(&(page, dpi)) = first.lowered.iter().find(|(p, _)| *p == page + 1) {
                    reply(&Reply::Lowered { page, dpi });
                }
                step();
            }
        }
    }
    for (input, s) in request.inputs.iter().zip(&scripts) {
        let hold = with_suffix(input, ".hold");
        while s.stall && hold.exists() {
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    if scripts.iter().any(|s| s.late_failure) {
        reply(&Reply::Failed { input: Some(0), why: Failure::Other, message: "failed after writing".into() });
        std::process::exit(1);
    }
    if scripts.iter().any(|s| s.hang) {
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    reply(&Reply::Done);
}
