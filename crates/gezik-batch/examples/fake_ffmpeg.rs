//! A stand-in for ffmpeg and ffprobe in `tests/ffmpeg.rs` (built by `cargo test` with the
//! other examples). Not shipped.
//!
//! - `-version`: prints `banner.txt` from its own folder (a gyan.dev 9.0.2 banner without one).
//! - `-show_entries …` (as ffprobe): prints the text of the file given last.
//! - Anything else (as ffmpeg): wants `-progress`, then writes `--fake-blocks` progress blocks
//!   (`--fake-step-us` apart, `--fake-sleep-ms` between them; `--fake-na` makes the first one
//!   `N/A`), hangs until killed with `--fake-hang`, writes the file given last, prints
//!   `--fake-stderr-lines` numbered error lines and exits with `--fake-exit`. While a file
//!   named as its input (`-i`) with `.hold` added exists, it waits after the progress blocks,
//!   having added its process id as a line to `<input>.log` (for the engine's tests, whose
//!   arguments come from a preset).

use std::io::Write;
use std::time::Duration;

const BANNER: &str = "ffmpeg version 9.0.2-essentials_build-www.gyan.dev Copyright (c) 2000-2026 the FFmpeg developers\n\
                      built with gcc 15.2.0 (Rev8, Built by MSYS2 project)\n";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |flag: &str| args.iter().any(|a| a == flag);
    let value = |flag: &str, default: u64| -> u64 {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    };
    if has("-version") {
        let exe = std::env::current_exe().unwrap();
        let banner = std::fs::read_to_string(exe.with_file_name("banner.txt")).unwrap_or_else(|_| BANNER.to_string());
        print!("{banner}");
        return;
    }
    if has("-show_entries") {
        let text = std::fs::read_to_string(args.last().unwrap()).unwrap_or_default();
        println!("{}", text.trim());
        return;
    }
    if !has("-progress") {
        eprintln!("fake ffmpeg: no -progress");
        std::process::exit(9);
    }
    let blocks = value("--fake-blocks", 4);
    let step = value("--fake-step-us", 250_000);
    let sleep = Duration::from_millis(value("--fake-sleep-ms", 20));
    let mut out = std::io::stdout().lock();
    for n in 1..=blocks {
        let time = if n == 1 && has("--fake-na") { "N/A".to_string() } else { (n * step).to_string() };
        let end = if n == blocks && !has("--fake-hang") { "end" } else { "continue" };
        let _ = write!(out, "frame={n}\nout_time_us={time}\nout_time_ms={time}\nspeed=1x\nprogress={end}\n");
        let _ = out.flush();
        std::thread::sleep(sleep);
    }
    drop(out);
    if let Some(input) = args.iter().position(|a| a == "-i").and_then(|i| args.get(i + 1)) {
        let hold = std::path::PathBuf::from(format!("{input}.hold"));
        if hold.exists() {
            let mut log = std::fs::OpenOptions::new().create(true).append(true).open(format!("{input}.log")).unwrap();
            let _ = writeln!(log, "{}", std::process::id());
            drop(log);
            while hold.exists() {
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
    if has("--fake-hang") {
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    if let Some(output) = args.last() {
        let _ = std::fs::write(output, b"fake output");
    }
    for n in 1..=value("--fake-stderr-lines", 0) {
        eprintln!("error line {n}");
    }
    std::process::exit(value("--fake-exit", 0) as i32);
}
