//! Linux and other Unix systems: the system's `curl`, else `wget`, run as a program (no shell)
//! in the destination's folder; the progress is the growing size of the file they write.

use std::cell::{Cell, RefCell};
use std::ffi::{OsStr, OsString};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::{MAX_REDIRECTS, Throttle, interrupted, too_big};
use crate::ChildProcess;

/// How often the file's size is looked at.
const POLL: Duration = Duration::from_millis(100);

/// What is said when neither program is there.
const HINT: &str = "Install curl with your package manager (sudo apt install curl)";

/// The programs that download.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Downloader {
    Curl,
    Wget,
}

pub(super) fn download(
    url: &str,
    dest: &Path,
    max_bytes: u64,
    allow_http: bool,
    progress: &mut dyn FnMut(u64),
    stop: &dyn Fn() -> bool,
) -> io::Result<()> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let (program, which) = match (find("curl", &path), find("wget", &path)) {
        (Some(curl), _) => (curl, Downloader::Curl),
        (None, Some(wget)) => (wget, Downloader::Wget),
        (None, None) => return Err(io::Error::new(io::ErrorKind::NotFound, HINT)),
    };
    run(&program, which, url, dest, max_bytes, allow_http, progress, stop)
}

pub(super) fn tool_missing_hint() -> Option<String> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    match find("curl", &path).or_else(|| find("wget", &path)) {
        Some(_) => None,
        None => Some(HINT.to_owned()),
    }
}

/// Runs `program` (a `which`) into `dest`, watching the size it reached.
#[allow(clippy::too_many_arguments)]
pub(super) fn run(
    program: &Path,
    which: Downloader,
    url: &str,
    dest: &Path,
    max_bytes: u64,
    allow_http: bool,
    progress: &mut dyn FnMut(u64),
    stop: &dyn Fn() -> bool,
) -> io::Result<()> {
    // The program runs in the destination's folder and is given its name: no non-ASCII path
    // in the arguments.
    let folder = dest.parent().filter(|folder| !folder.as_os_str().is_empty());
    let name = dest.file_name().unwrap_or(dest.as_os_str());
    let args = match which {
        Downloader::Curl => curl_arguments(url, name, max_bytes, allow_http),
        Downloader::Wget => wget_arguments(url, name, allow_http),
    };
    let mut child = ChildProcess::spawn(program, args, folder)?;
    let size = || std::fs::metadata(dest).map_or(0, |meta| meta.len());
    // `wait_or_stop` takes a `Fn`: what it learns is kept in cells.
    let throttle = RefCell::new(Throttle::new(progress));
    let looked = Cell::new(Instant::now());
    let bigger = Cell::new(false);
    let status = child.wait_or_stop(|| {
        if looked.get().elapsed() >= POLL {
            looked.set(Instant::now());
            let now = size();
            if now > max_bytes {
                bigger.set(true);
                return true;
            }
            throttle.borrow_mut().update(now);
        }
        stop()
    })?;
    if bigger.get() {
        return Err(too_big());
    }
    let Some(status) = status else { return Err(interrupted()) };
    if !status.success() {
        let text = match which {
            Downloader::Curl => curl_message(&child.stderr_text()),
            Downloader::Wget => wget_message(status.code()),
        };
        return Err(io::Error::other(text));
    }
    // wget has no limit of its own.
    if size() > max_bytes {
        return Err(too_big());
    }
    Ok(())
}

/// `name` as a program on `path_var` that may be run.
pub(super) fn find(name: &str, path_var: &OsStr) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    std::env::split_paths(path_var)
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join(name))
        .find(|path| std::fs::metadata(path).is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0))
}

/// curl's arguments: no `.curlrc` (`--disable`, which must come first), HTTPS only for the
/// address and every redirect (plain HTTP too when `allow_http`, which only tests give), at
/// most `max_bytes`, into `name`.
pub(super) fn curl_arguments(url: &str, name: &OsStr, max_bytes: u64, allow_http: bool) -> Vec<OsString> {
    let protocols = if allow_http { "=http,https" } else { "=https" };
    let max = max_bytes.to_string();
    let redirects = MAX_REDIRECTS.to_string();
    let mut args: Vec<OsString> = [
        "--disable",
        "--fail",
        "--location",
        "--proto",
        protocols,
        "--proto-redir",
        protocols,
        "--max-redirs",
        &redirects,
        "--max-filesize",
        &max,
        "--connect-timeout",
        "30",
        "--silent",
        "--show-error",
        "--output",
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    args.push(name.to_owned());
    args.push(url.into());
    args
}

/// wget's arguments: no `.wgetrc`, HTTPS only (unless `allow_http`) for the address and every
/// redirect, one try, into `name`.
pub(super) fn wget_arguments(url: &str, name: &OsStr, allow_http: bool) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec!["--no-config".into()];
    if !allow_http {
        args.push("--https-only".into());
    }
    let redirects = format!("--max-redirect={MAX_REDIRECTS}");
    for arg in [redirects.as_str(), "--timeout=60", "--tries=1", "--quiet", "-O"] {
        args.push(arg.into());
    }
    args.push(name.to_owned());
    args.push(url.into());
    args
}

/// What curl said went wrong, in a line.
pub(super) fn curl_message(errors: &str) -> String {
    let lines: Vec<&str> = errors.lines().map(str::trim).filter(|line| !line.is_empty()).collect();
    if lines.is_empty() { "the download failed".to_owned() } else { lines.join(" ") }
}

/// What wget's exit code says (it is quiet).
pub(super) fn wget_message(code: Option<i32>) -> String {
    let text = match code {
        Some(3) => "could not write the file",
        Some(4) => "could not connect to the server (network failure)",
        Some(5) => "the server's certificate could not be verified",
        Some(6) => "the server asked for a login",
        Some(7) => "the server's answer was not understood",
        Some(8) => "the server answered with an error",
        _ => "the download failed",
    };
    format!("wget: {text}")
}
