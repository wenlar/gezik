//! Linux and other Unix systems: the system's `curl`, else `wget`, run as a program (no shell)
//! in the destination's folder; the progress is the growing size of the file they write.

use std::cell::{Cell, RefCell};
use std::ffi::{OsStr, OsString};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::{MAX_REDIRECTS, Response, Throttle, interrupted, loopback, retry_seconds, too_big};
use crate::ChildProcess;

/// How often the file's size is looked at.
const POLL: Duration = Duration::from_millis(100);

/// What is said when neither program is there.
const HINT: &str = gezik_i18n::tx!("Install curl with your package manager (sudo apt install curl)");

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
        (None, None) => return Err(io::Error::new(io::ErrorKind::NotFound, gezik_i18n::t("", HINT).into_owned())),
    };
    run(&program, which, url, dest, max_bytes, allow_http, progress, stop)
}

pub(super) fn tool_missing_hint() -> Option<String> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    match find("curl", &path).or_else(|| find("wget", &path)) {
        Some(_) => None,
        None => Some(gezik_i18n::t("", HINT).into_owned()),
    }
}

/// curl only: wget cannot be given the headers without putting the secret on its command line.
pub(super) fn request(
    method: &str,
    url: &str,
    headers: &[(&str, &str)],
    body: Option<&[u8]>,
    max_bytes: u64,
) -> io::Result<Response> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let Some(curl) = find("curl", &path) else {
        return Err(io::Error::new(io::ErrorKind::NotFound, gezik_i18n::t("", HINT).into_owned()));
    };
    let dir = private_dir()?;
    let result = curl_request(&curl, &dir, method, url, headers, body, max_bytes);
    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// A new folder only this user may enter, for a request's files: the headers carry the code's
/// proof, which must not be on curl's command line, where other users' tools can see it.
fn private_dir() -> io::Result<PathBuf> {
    use std::os::unix::fs::DirBuilderExt;
    use std::sync::atomic::{AtomicU32, Ordering};
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "gezik-request-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    // Fails if it is there already: nobody else's folder (or link) is used.
    std::fs::DirBuilder::new().mode(0o700).create(&dir)?;
    Ok(dir)
}

/// Runs curl in `dir`, with the headers and the body in files there, and reads what it wrote.
fn curl_request(
    curl: &Path,
    dir: &Path,
    method: &str,
    url: &str,
    headers: &[(&str, &str)],
    body: Option<&[u8]>,
    max_bytes: u64,
) -> io::Result<Response> {
    let lines: String = headers.iter().map(|(name, value)| format!("{name}: {value}\n")).collect();
    std::fs::write(dir.join("headers"), lines)?;
    if let Some(body) = body {
        std::fs::write(dir.join("send"), body)?;
    }
    let mut child = ChildProcess::spawn(curl, request_arguments(method, url, body.is_some(), max_bytes), Some(dir))?;
    let status = child.wait_or_stop(|| false)?.ok_or_else(interrupted)?;
    if !status.success() {
        // 63: over --max-filesize.
        return Err(if status.code() == Some(63) {
            too_big()
        } else {
            io::Error::other(curl_message(&child.stderr_text()))
        });
    }
    let body = std::fs::read(dir.join("body")).unwrap_or_default();
    if body.len() as u64 > max_bytes {
        return Err(too_big());
    }
    let head = std::fs::read_to_string(dir.join("head"))?;
    parse_head(&head, body)
        .ok_or_else(|| io::Error::other(gezik_i18n::t!("the server's answer was not understood").into_owned()))
}

/// curl's arguments for [`request`]: no `.curlrc`, https only (http too for this computer), no
/// redirects, the headers from the file `headers`, the body from `send`, the answer's head
/// into `head` and its body into `body`.
pub(super) fn request_arguments(method: &str, url: &str, send: bool, max_bytes: u64) -> Vec<String> {
    let protocols = if loopback(url) { "=http,https" } else { "=https" };
    let mut args: Vec<String> = [
        "--disable",
        "--silent",
        "--show-error",
        "--proto",
        protocols,
        "--max-filesize",
        &max_bytes.to_string(),
        "--connect-timeout",
        "30",
        // A minute with nothing arriving ends it.
        "--speed-limit",
        "1",
        "--speed-time",
        "60",
        "--request",
        method,
        "--header",
        "@headers",
        "--dump-header",
        "head",
        "--output",
        "body",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    if send {
        args.extend(["--data-binary".to_owned(), "@send".to_owned()]);
    }
    args.push(url.to_owned());
    args
}

/// The status and headers of the last answer in curl's `--dump-header` text (a `100
/// Continue` or a proxy's `CONNECT` answer comes before it).
pub(super) fn parse_head(head: &str, body: Vec<u8>) -> Option<Response> {
    let last = head.replace("\r\n", "\n");
    let last = last.split("\n\n").filter(|block| block.trim_start().starts_with("HTTP/")).last()?;
    let mut lines = last.trim_start().lines();
    let status = lines.next()?.split_whitespace().nth(1)?.parse().ok()?;
    let mut response = Response { status, etag: None, retry_after: None, body };
    for line in lines {
        let Some((name, value)) = line.split_once(':') else { continue };
        if name.trim().eq_ignore_ascii_case("etag") {
            response.etag = Some(value.trim().to_owned());
        } else if name.trim().eq_ignore_ascii_case("retry-after") {
            response.retry_after = retry_seconds(value);
        }
    }
    Some(response)
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
    let errors = child.stderr_text();
    // wget's `--https-only` only holds in recursive mode: where its redirects led is read
    // from the headers it printed (`--server-response`). The request went out, but nothing
    // it brought is kept.
    if which == Downloader::Wget && !allow_http && insecure_redirect(&errors) {
        return Err(io::Error::other(
            gezik_i18n::t!("the download was redirected to an insecure address").into_owned(),
        ));
    }
    if !status.success() {
        let text = match which {
            Downloader::Curl => curl_message(&errors),
            Downloader::Wget => wget_message(status.code(), &errors),
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

/// wget's arguments: no `.wgetrc`, at most 5 redirects, one try, the server's headers on the
/// error output (for `insecure_redirect`), into `name`. `--https-only` is given too, though
/// it only holds for recursive downloads.
pub(super) fn wget_arguments(url: &str, name: &OsStr, allow_http: bool) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec!["--no-config".into()];
    if !allow_http {
        args.push("--https-only".into());
    }
    let redirects = format!("--max-redirect={MAX_REDIRECTS}");
    for arg in [redirects.as_str(), "--timeout=60", "--tries=1", "--quiet", "--server-response", "-O"] {
        args.push(arg.into());
    }
    args.push(name.to_owned());
    args.push(url.into());
    args
}

/// What curl said went wrong, in a line.
pub(super) fn curl_message(errors: &str) -> String {
    let lines: Vec<&str> = errors.lines().map(str::trim).filter(|line| !line.is_empty()).collect();
    if lines.is_empty() { gezik_i18n::t!("the download failed").into_owned() } else { lines.join(" ") }
}

/// Whether a `Location:` header in wget's `--server-response` output leads anywhere but
/// https (a relative one stays where it was).
pub(super) fn insecure_redirect(server_response: &str) -> bool {
    server_response
        .lines()
        .filter_map(|line| {
            let (name, value) = line.trim().split_once(':')?;
            name.eq_ignore_ascii_case("location").then(|| value.trim().to_ascii_lowercase())
        })
        .any(|target| target.contains("://") && !target.starts_with("https://"))
}

/// What wget's exit code says (it is quiet but for the headers in `server_response`).
pub(super) fn wget_message(code: Option<i32>, server_response: &str) -> String {
    // The last status line is the answer that failed: "HTTP/1.1 404 Not Found".
    let answer = server_response.lines().map(str::trim).rfind(|line| line.starts_with("HTTP/"));
    if code == Some(8)
        && let Some((_, status)) = answer.and_then(|line| line.split_once(' '))
    {
        return gezik_i18n::t!("wget: the server answered {}", status);
    }
    let text = match code {
        // BusyBox's wget knows none of the options: its usage message ends it with 1.
        Some(1) | Some(2) => gezik_i18n::tx!("the download failed (if this wget is BusyBox's, install curl)"),
        Some(3) => gezik_i18n::tx!("could not write the file"),
        Some(4) => gezik_i18n::tx!("could not connect to the server (network failure)"),
        Some(5) => gezik_i18n::tx!("the server's certificate could not be verified"),
        Some(6) => gezik_i18n::tx!("the server asked for a login"),
        Some(7) => gezik_i18n::tx!("the server's answer was not understood"),
        Some(8) => gezik_i18n::tx!("the server answered with an error"),
        _ => gezik_i18n::tx!("the download failed"),
    };
    format!("wget: {}", gezik_i18n::t("", text))
}
