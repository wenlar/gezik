//! Downloading a file with the system's own HTTP client, so that the system's proxy settings,
//! certificates and TLS updates apply and Gezik carries no TLS code of its own (2026-10-06
//! decision): WinHTTP on Windows, `NSURLSession` on macOS, `curl` or else `wget` elsewhere.

use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod imp;
#[cfg(all(unix, not(target_os = "macos")))]
#[path = "unix.rs"]
mod imp;
#[cfg(windows)]
#[path = "windows.rs"]
mod imp;

#[cfg(test)]
mod tests;

/// How often `progress` is told at most.
const PROGRESS_EVERY: Duration = Duration::from_millis(100);

/// How many redirects are followed at most.
const MAX_REDIRECTS: u32 = 5;

/// Downloads `url` (https; http only when `allow_http`) into `dest`, calling `progress(bytes_so_far)`
/// at most every 100 ms; stops and returns `Interrupted` when `stop()` turns true; follows at
/// most 5 redirects, all https (unless `allow_http`); fails if the body exceeds `max_bytes`.
/// On any failure (a stop too) `dest` is removed.
pub fn download(
    url: &str,
    dest: &Path,
    max_bytes: u64,
    allow_http: bool,
    progress: &mut dyn FnMut(u64),
    stop: &dyn Fn() -> bool,
) -> io::Result<()> {
    if !allowed(url, allow_http) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "only https addresses are downloaded"));
    }
    let result = imp::download(url, dest, max_bytes, allow_http, progress, stop);
    if result.is_err() {
        let _ = std::fs::remove_file(dest);
    }
    result
}

/// What to install for downloading to work, when nothing that downloads is there (Linux and
/// other Unix systems without `curl` or `wget`); `None` elsewhere.
pub fn tool_missing_hint() -> Option<String> {
    imp::tool_missing_hint()
}

/// Whether `url`'s scheme may be asked for (case-insensitive, as URLs are).
fn allowed(url: &str, allow_http: bool) -> bool {
    let scheme = url.split_once("://").map(|(scheme, _)| scheme.to_ascii_lowercase());
    match scheme.as_deref() {
        Some("https") => true,
        Some("http") => allow_http,
        _ => false,
    }
}

/// The error for a stop.
fn interrupted() -> io::Error {
    io::Error::new(io::ErrorKind::Interrupted, "cancelled")
}

/// The error for a body larger than allowed.
fn too_big() -> io::Error {
    io::Error::new(io::ErrorKind::FileTooLarge, "the download is larger than expected")
}

/// Passes the byte count on to `progress`, at most every `PROGRESS_EVERY` and only when it grew.
struct Throttle<'a> {
    progress: &'a mut dyn FnMut(u64),
    told: u64,
    at: Instant,
}

impl<'a> Throttle<'a> {
    fn new(progress: &'a mut dyn FnMut(u64)) -> Throttle<'a> {
        Throttle { progress, told: 0, at: Instant::now() }
    }

    fn update(&mut self, bytes: u64) {
        if bytes > self.told && self.at.elapsed() >= PROGRESS_EVERY {
            self.at = Instant::now();
            self.told = bytes;
            (self.progress)(bytes);
        }
    }
}
