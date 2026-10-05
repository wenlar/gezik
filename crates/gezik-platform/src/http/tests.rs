//! Downloads from a small HTTP server on the loopback address (plain http: `allow_http`).

use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
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
    body: Vec<u8>,
    chunk: usize,
    pause: Duration,
}

impl Reply {
    fn ok(body: Vec<u8>) -> Reply {
        Reply { status: "200 OK", location: None, body, chunk: 64 * 1024, pause: Duration::ZERO }
    }

    fn slow(mut self, chunk: usize, pause: Duration) -> Reply {
        self.chunk = chunk;
        self.pause = pause;
        self
    }
}

/// What the server saw: requests, and whether a whole body went out.
#[derive(Default)]
struct Seen {
    requests: AtomicUsize,
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
    let text = String::from_utf8_lossy(&request);
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
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;
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
            "-O",
            "a.part",
            "https://example.com/a.zip",
        ];
        assert_eq!(args, expected);
        let http = imp::wget_arguments("http://127.0.0.1:1/a.zip", OsStr::new("a.part"), true);
        assert!(!http.iter().any(|arg| arg == "--https-only"));
        assert_eq!(imp::wget_message(Some(8)), "wget: the server answered with an error");
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
            let (base, _) = serve(move |_, path| match path {
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
            let _ = std::fs::remove_dir_all(&d);
        }
    }
}
