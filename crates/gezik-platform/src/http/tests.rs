//! Downloads and requests to a small HTTP server on the loopback address (plain http: `allow_http`).

use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::*;

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("gezik-http-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// One answer of the test server.
struct Reply {
    status: &'static str,
    location: Option<String>,
    headers: Vec<(&'static str, &'static str)>,
    body: Vec<u8>,
    chunk: usize,
    pause: Duration,
    /// Says nothing for this long: before the head, or after it (before the body).
    stall: Option<(Stall, Duration)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stall {
    BeforeHead,
    AfterHead,
}

impl Reply {
    fn ok(body: Vec<u8>) -> Reply {
        Reply {
            status: "200 OK",
            location: None,
            headers: Vec::new(),
            body,
            chunk: 64 * 1024,
            pause: Duration::ZERO,
            stall: None,
        }
    }

    fn slow(mut self, chunk: usize, pause: Duration) -> Reply {
        self.chunk = chunk;
        self.pause = pause;
        self
    }

    fn stalled(mut self, when: Stall) -> Reply {
        self.stall = Some((when, Duration::from_secs(20)));
        self
    }
}

/// What the server saw: requests, the last one's head and body, and whether a whole body
/// went out.
#[derive(Default)]
struct Seen {
    requests: AtomicUsize,
    last: Mutex<Option<(String, Vec<u8>)>>,
    sent: AtomicBool,
}

type Answer = dyn Fn(&str, &str) -> Reply + Send + Sync;

/// A server answering each GET with `answer(base, path)`; gives its base address (no
/// trailing slash) and what it saw.
fn serve(answer: impl Fn(&str, &str) -> Reply + Send + Sync + 'static) -> (String, Arc<Seen>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Seen::default());
    let answer: Arc<Answer> = Arc::new(answer);
    let (server_base, server_seen) = (base.clone(), seen.clone());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let (base, seen, answer) = (server_base.clone(), server_seen.clone(), answer.clone());
            std::thread::spawn(move || {
                if respond(stream, &base, &seen, &*answer).is_ok() {
                    seen.sent.store(true, Ordering::SeqCst);
                }
            });
        }
    });
    (base, seen)
}

fn respond(mut stream: TcpStream, base: &str, seen: &Seen, answer: &Answer) -> io::Result<()> {
    let mut request = Vec::new();
    let mut byte = [0u8; 1];
    while !request.ends_with(b"\r\n\r\n") {
        if stream.read(&mut byte)? == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        request.push(byte[0]);
    }
    seen.requests.fetch_add(1, Ordering::SeqCst);
    let text = String::from_utf8_lossy(&request).into_owned();
    let lower = text.to_ascii_lowercase();
    let length = lower
        .lines()
        .find_map(|line| line.strip_prefix("content-length:"))
        .and_then(|n| n.trim().parse::<usize>().ok())
        .unwrap_or(0);
    // curl asks first before sending a large body.
    if lower.contains("expect: 100-continue") {
        stream.write_all(b"HTTP/1.1 100 Continue\r\n\r\n")?;
    }
    let mut sent = vec![0u8; length];
    stream.read_exact(&mut sent)?;
    *seen.last.lock().unwrap() = Some((text.clone(), sent));
    let path = text.split_whitespace().nth(1).unwrap_or("/").to_owned();
    let reply = answer(base, &path);
    let mut head = format!(
        "HTTP/1.1 {}\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n",
        reply.status,
        reply.body.len()
    );
    if let Some(location) = &reply.location {
        head.push_str(&format!("Location: {location}\r\n"));
    }
    for (name, value) in &reply.headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("\r\n");
    let stall = |when: Stall| {
        if let Some((at, long)) = reply.stall
            && at == when
        {
            std::thread::sleep(long);
        }
    };
    stall(Stall::BeforeHead);
    stream.write_all(head.as_bytes())?;
    stream.flush()?;
    stall(Stall::AfterHead);
    for piece in reply.body.chunks(reply.chunk.max(1)) {
        stream.write_all(piece)?;
        stream.flush()?;
        if !reply.pause.is_zero() {
            std::thread::sleep(reply.pause);
        }
    }
    Ok(())
}

fn body(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 251) as u8).collect()
}

fn never() -> bool {
    false
}

#[test]
fn downloads_with_progress() {
    let d = dir("progress");
    let data = body(1024 * 1024);
    let sent = data.clone();
    // 1 MiB in 32 KiB pieces 20 ms apart: about 0.6 s, so progress is told several times.
    let (base, _) = serve(move |_, _| Reply::ok(sent.clone()).slow(32 * 1024, Duration::from_millis(20)));
    let dest = d.join("file.bin");
    let mut told = Vec::new();
    download(&format!("{base}/file.bin"), &dest, 2 * 1024 * 1024, true, &mut |bytes| told.push(bytes), &never).unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), data);
    assert!(told.len() >= 2, "{told:?}");
    assert!(told.windows(2).all(|pair| pair[0] < pair[1]), "{told:?}");
    assert!(told.iter().all(|&bytes| bytes <= data.len() as u64), "{told:?}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_stop_mid_body_interrupts_and_leaves_no_file() {
    let d = dir("stop");
    // 4 MiB at 64 KiB per 100 ms: about 6.4 s if it ran to the end.
    let (base, seen) = serve(|_, _| Reply::ok(body(4 * 1024 * 1024)).slow(64 * 1024, Duration::from_millis(100)));
    let dest = d.join("file.bin");
    let got_some = AtomicBool::new(false);
    let started = Instant::now();
    let err = download(
        &format!("{base}/file.bin"),
        &dest,
        8 * 1024 * 1024,
        true,
        &mut |bytes| got_some.store(bytes > 0, Ordering::SeqCst),
        &|| got_some.load(Ordering::SeqCst),
    )
    .unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::Interrupted, "{err}");
    assert!(started.elapsed() < Duration::from_secs(5), "{:?}", started.elapsed());
    assert!(!dest.exists());
    std::thread::sleep(Duration::from_millis(500));
    assert!(!seen.sent.load(Ordering::SeqCst), "the whole body went out");
    let _ = std::fs::remove_dir_all(&d);
}

/// A server that stops answering: a stop still ends the download at once.
#[test]
fn a_stop_while_the_server_stalls_returns_quickly() {
    let d = dir("stall");
    for when in [Stall::BeforeHead, Stall::AfterHead] {
        let (base, _) = serve(move |_, _| Reply::ok(body(1000)).stalled(when));
        let dest = d.join("file.bin");
        let started = Instant::now();
        let stop_at = started + Duration::from_millis(500);
        let err =
            download(&format!("{base}/file.bin"), &dest, 1 << 20, true, &mut |_| {}, &|| Instant::now() >= stop_at)
                .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::Interrupted, "{err}");
        let took = started.elapsed();
        assert!(took < Duration::from_millis(1500), "{took:?}");
        assert!(!dest.exists());
    }
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_body_over_the_limit_fails_and_leaves_no_file() {
    let d = dir("too-big");
    let (base, _) = serve(|_, _| Reply::ok(body(300 * 1024)));
    let dest = d.join("file.bin");
    let err = download(&format!("{base}/file.bin"), &dest, 100 * 1024, true, &mut |_| {}, &never).unwrap_err();
    assert_ne!(err.kind(), io::ErrorKind::Interrupted, "{err}");
    assert!(!dest.exists());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_404_fails_and_leaves_no_file() {
    let d = dir("404");
    let (base, _) = serve(|_, _| Reply { status: "404 Not Found", ..Reply::ok(b"no such file".to_vec()) });
    let dest = d.join("file.bin");
    let err = download(&format!("{base}/file.bin"), &dest, 1024, true, &mut |_| {}, &never).unwrap_err();
    assert!(err.to_string().contains("404"), "{err}");
    assert!(!dest.exists());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn refused_connection_says_so() {
    let d = dir("refused");
    // Nothing listens on the port of a listener that is gone.
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let dest = d.join("file.bin");
    let err =
        download(&format!("http://127.0.0.1:{port}/file.bin"), &dest, 1024, true, &mut |_| {}, &never).unwrap_err();
    assert!(err.to_string().to_lowercase().contains("connect"), "{err}");
    assert!(!dest.exists());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn plain_http_is_refused_unless_allowed() {
    let d = dir("http");
    let (base, seen) = serve(|_, _| Reply::ok(b"x".to_vec()));
    let dest = d.join("file.bin");
    let err = download(&format!("{base}/file.bin"), &dest, 1024, false, &mut |_| {}, &never).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    for url in ["ftp://example.com/a", "file:///etc/passwd", "example.com/a", "HTTP://127.0.0.1/a"] {
        let err = download(url, &dest, 1024, false, &mut |_| {}, &never).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput, "{url}");
    }
    assert_eq!(seen.requests.load(Ordering::SeqCst), 0);
    assert!(!dest.exists());
    assert!(allowed("HTTPS://example.com/a", false));
    let _ = std::fs::remove_dir_all(&d);
}

/// `/hop/N` redirects to `/hop/N-1`; `/hop/0` is the file.
fn hops(base: &str, path: &str) -> Reply {
    match path.strip_prefix("/hop/").and_then(|n| n.parse::<u32>().ok()) {
        Some(0) => Reply::ok(b"arrived".to_vec()),
        Some(n) => {
            Reply { status: "302 Found", location: Some(format!("{base}/hop/{}", n - 1)), ..Reply::ok(Vec::new()) }
        }
        None => Reply { status: "404 Not Found", ..Reply::ok(Vec::new()) },
    }
}

#[test]
fn redirects_are_followed_up_to_five() {
    let d = dir("redirects");
    let (base, _) = serve(hops);
    let dest = d.join("file.bin");
    download(&format!("{base}/hop/5"), &dest, 1024, true, &mut |_| {}, &never).unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), b"arrived");
    let err = download(&format!("{base}/hop/6"), &dest, 1024, true, &mut |_| {}, &never).unwrap_err();
    assert_ne!(err.kind(), io::ErrorKind::Interrupted, "{err}");
    assert!(!dest.exists());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn requests_send_their_headers_and_body_and_give_any_status() {
    let (base, seen) = serve(|_, path| match path {
        "/v1/box" => Reply { headers: vec![("ETag", "\"7\"")], ..Reply::ok(b"stored".to_vec()) },
        "/v1/race" => Reply { status: "412 Precondition Failed", ..Reply::ok(Vec::new()) },
        "/v1/busy" => {
            Reply { status: "429 Too Many Requests", headers: vec![("Retry-After", "30")], ..Reply::ok(Vec::new()) }
        }
        "/v1/gone" => Reply { status: "204 No Content", ..Reply::ok(Vec::new()) },
        _ => Reply { status: "404 Not Found", ..Reply::ok(Vec::new()) },
    });
    let last = || seen.last.lock().unwrap().take().unwrap();
    let sent = body(300 * 1024);
    let headers = [("Authorization", "Bearer abc"), ("If-Match", "\"6\"")];
    let answer = request("PUT", &format!("{base}/v1/box"), &headers, Some(&sent), 1024).unwrap();
    let stored = Response { status: 200, etag: Some("\"7\"".to_owned()), retry_after: None, body: b"stored".to_vec() };
    assert_eq!(answer, stored);
    let (head, got) = last();
    assert!(head.starts_with("PUT /v1/box "), "{head}");
    let head = head.to_ascii_lowercase();
    assert!(head.contains("authorization: bearer abc\r\n"), "{head}");
    assert!(head.contains("if-match: \"6\"\r\n"), "{head}");
    assert!(got == sent, "the body came through changed");

    let answer = request("PUT", &format!("{base}/v1/race"), &[("If-Match", "\"1\"")], Some(b"x"), 1024).unwrap();
    assert_eq!((answer.status, answer.etag), (412, None));
    let answer = request("GET", &format!("{base}/v1/missing"), &[], None, 1024).unwrap();
    assert_eq!(answer.status, 404);
    assert!(last().0.starts_with("GET /v1/missing "));
    let answer = request("GET", &format!("{base}/v1/busy"), &[], None, 1024).unwrap();
    assert_eq!((answer.status, answer.retry_after), (429, Some(30)));
    let answer = request("DELETE", &format!("{base}/v1/gone"), &[], None, 1024).unwrap();
    assert_eq!((answer.status, answer.body.len()), (204, 0));
    assert!(last().0.starts_with("DELETE /v1/gone "));
    let err = request("GET", &format!("{base}/v1/box"), &[], None, 3).unwrap_err();
    assert_eq!(err.to_string(), too_big().to_string());
}

#[test]
fn requests_follow_no_redirect() {
    let (base, seen) = serve(hops);
    let answer = request("GET", &format!("{base}/hop/1"), &[], None, 1024).unwrap();
    assert_eq!(answer.status, 302);
    assert_eq!(seen.requests.load(Ordering::SeqCst), 1);
}

#[test]
fn requests_go_over_plain_http_only_to_this_computer() {
    for url in
        ["http://example.com/v1", "http://127.0.0.1.example.com/v1", "http://x@127.0.0.1/v1", "ftp://127.0.0.1/a"]
    {
        let err = request("GET", url, &[], None, 1024).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput, "{url}");
    }
    for url in ["http://127.0.0.1:8787/v1/x", "HTTP://localhost/v1", "http://LOCALHOST:1"] {
        assert!(loopback(url), "{url}");
    }
    let err = request("GET", "https://example.com/", &[("X-A", "b\r\nEvil: 1")], None, 1).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
}

#[test]
fn the_hint_is_only_for_systems_without_a_downloader() {
    #[cfg(any(windows, target_os = "macos"))]
    assert_eq!(tool_missing_hint(), None);
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let path = std::env::var_os("PATH").unwrap_or_default();
        let has_one = imp::find("curl", &path).is_some() || imp::find("wget", &path).is_some();
        assert_eq!(tool_missing_hint().is_none(), has_one);
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod unix {
    use std::ffi::OsStr;

    use super::*;
    use crate::http::imp::{self, Downloader};

    #[test]
    fn curl_arguments_are_5bs() {
        let args = imp::curl_arguments("https://example.com/a.zip", OsStr::new(".gezik-copying-1-0"), 1000, false);
        let expected = [
            "--disable",
            "--fail",
            "--location",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--max-redirs",
            "5",
            "--max-filesize",
            "1000",
            "--connect-timeout",
            "30",
            "--silent",
            "--show-error",
            "--output",
            ".gezik-copying-1-0",
            "https://example.com/a.zip",
        ];
        assert_eq!(args, expected);
        let http = imp::curl_arguments("http://127.0.0.1:1/a.zip", OsStr::new("a"), 1000, true);
        assert_eq!((http[4].to_str(), http[6].to_str()), (Some("=http,https"), Some("=http,https")));
        assert_eq!(
            imp::curl_message("curl: (22) The requested URL returned error: 404\n"),
            "curl: (22) The requested URL returned error: 404"
        );
        assert_eq!(imp::curl_message(""), "the download failed");
    }

    #[test]
    fn wget_arguments() {
        let args = imp::wget_arguments("https://example.com/a.zip", OsStr::new("a.part"), false);
        let expected = [
            "--no-config",
            "--https-only",
            "--max-redirect=5",
            "--timeout=60",
            "--tries=1",
            "--quiet",
            "--server-response",
            "-O",
            "a.part",
            "https://example.com/a.zip",
        ];
        assert_eq!(args, expected);
        let http = imp::wget_arguments("http://127.0.0.1:1/a.zip", OsStr::new("a.part"), true);
        assert!(!http.iter().any(|arg| arg == "--https-only"));
        assert_eq!(imp::wget_message(Some(8), ""), "wget: the server answered with an error");
        let headers = "  HTTP/1.1 302 Found
  Location: https://a.example/x
  HTTP/1.1 404 Not Found
  Content-Length: 0
";
        assert_eq!(imp::wget_message(Some(8), headers), "wget: the server answered 404 Not Found");
        assert!(imp::wget_message(Some(1), "").contains("curl"));
    }

    /// A sample of what `wget --quiet --server-response` (GNU Wget 1.25) printed for a redirect.
    #[test]
    fn insecure_redirects_are_seen_in_wgets_headers() {
        let sample = "  HTTP/1.0 302 Found
  Server: BaseHTTP/0.6 Python/3.13.5
  Date: Mon, 05 Oct 2026 21:46:10 GMT
  Location: http://127.0.0.1:8000/f
  Content-Length: 0
  HTTP/1.0 200 OK
  Server: BaseHTTP/0.6 Python/3.13.5
  Content-Length: 3
";
        assert!(imp::insecure_redirect(sample));
        assert!(imp::insecure_redirect(&sample.replace("http://", "HTTP://")));
        assert!(imp::insecure_redirect(&sample.replace("http://", "ftp://")));
        assert!(!imp::insecure_redirect(&sample.replace("http://", "https://")));
        // Relative: same scheme as before.
        assert!(!imp::insecure_redirect(
            "  Location: /f
  location: other/f [following]
"
        ));
        // The verbose form ("[following]") is read too.
        assert!(imp::insecure_redirect(
            "Location: http://a.example/f [following]
"
        ));
        assert!(!imp::insecure_redirect(
            "  HTTP/1.1 200 OK
  Content-Length: 3
"
        ));
    }

    #[test]
    fn curls_request_arguments_and_answer() {
        let args = imp::request_arguments("PUT", "https://sync.example/v1/a", true, 100);
        assert!(!args.iter().any(|arg| arg == "--location" || arg.contains("Bearer")), "{args:?}");
        assert_eq!(args[..5], ["--disable", "--silent", "--show-error", "--proto", "=https"]);
        assert!(args.ends_with(&[
            "--data-binary".to_owned(),
            "@send".to_owned(),
            "https://sync.example/v1/a".to_owned()
        ]));
        let get = imp::request_arguments("GET", "http://127.0.0.1:1/v1/a", false, 100);
        assert_eq!(get[4], "=http,https");
        assert!(!get.iter().any(|arg| arg == "--data-binary"));
        let head =
            "HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 412 Precondition Failed\r\netag: \"3\"\r\nRetry-After: 5\r\n\r\n";
        let answer = imp::parse_head(head, b"x".to_vec()).unwrap();
        let expected =
            Response { status: 412, etag: Some("\"3\"".to_owned()), retry_after: Some(5), body: b"x".to_vec() };
        assert_eq!(answer, expected);
        assert_eq!(imp::parse_head("", Vec::new()), None);
    }

    #[test]
    fn programs_are_found_on_the_path() {
        let d = dir("find");
        let program = d.join("curl");
        std::fs::write(&program, "#!/bin/sh\n").unwrap();
        let path = std::env::join_paths([PathBuf::from("relative"), d.clone()]).unwrap();
        // Not runnable yet.
        assert_eq!(imp::find("curl", &path), None);
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(imp::find("curl", &path), Some(program));
        assert_eq!(imp::find("wget", &path), None);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Each program the system has downloads, stops and keeps to the limit.
    #[test]
    fn each_downloader_works() {
        let path = std::env::var_os("PATH").unwrap_or_default();
        for (name, which) in [("curl", Downloader::Curl), ("wget", Downloader::Wget)] {
            let Some(program) = imp::find(name, &path) else { continue };
            let d = dir(&format!("program-{name}"));
            let data = body(512 * 1024);
            let sent = data.clone();
            let (base, _) = serve(move |base, path| match path {
                "/to-http" => {
                    Reply { status: "302 Found", location: Some(format!("{base}/file")), ..Reply::ok(Vec::new()) }
                }
                "/slow" => Reply::ok(body(4 * 1024 * 1024)).slow(64 * 1024, Duration::from_millis(100)),
                "/missing" => Reply { status: "404 Not Found", ..Reply::ok(Vec::new()) },
                _ => Reply::ok(sent.clone()).slow(32 * 1024, Duration::from_millis(10)),
            });
            let dest = d.join("file.bin");
            let run = |url: &str, max: u64, progress: &mut dyn FnMut(u64), stop: &dyn Fn() -> bool| {
                let result = imp::run(&program, which, url, &dest, max, true, progress, stop);
                if result.is_err() {
                    let _ = std::fs::remove_file(&dest);
                }
                result
            };
            let mut told = 0;
            run(&format!("{base}/file"), 1024 * 1024, &mut |bytes| told = bytes, &never).unwrap();
            assert_eq!(std::fs::read(&dest).unwrap(), data, "{name}");
            assert!(told > 0, "{name}");
            let err = run(&format!("{base}/missing"), 1024, &mut |_| {}, &never).unwrap_err();
            assert!(!err.to_string().is_empty(), "{name}");
            let err = run(&format!("{base}/file"), 100 * 1024, &mut |_| {}, &never).unwrap_err();
            assert_ne!(err.kind(), io::ErrorKind::Interrupted, "{name}: {err}");
            assert!(!dest.exists(), "{name}");
            let got_some = AtomicBool::new(false);
            let err = run(
                &format!("{base}/slow"),
                8 * 1024 * 1024,
                &mut |bytes| got_some.store(bytes > 0, Ordering::SeqCst),
                &|| got_some.load(Ordering::SeqCst),
            )
            .unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::Interrupted, "{name}: {err}");
            // Without `allow_http` a redirect to http is refused (curl does not ask; wget does
            // and its headers give it away).
            let err = imp::run(&program, which, &format!("{base}/to-http"), &dest, 1 << 20, false, &mut |_| {}, &never)
                .unwrap_err();
            if which == Downloader::Wget {
                assert_eq!(err.to_string(), "the download was redirected to an insecure address");
            }
            let _ = std::fs::remove_file(&dest);
            let _ = std::fs::remove_dir_all(&d);
        }
    }
}
