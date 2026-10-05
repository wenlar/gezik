//! Downloading a tool through the engine from a small local HTTP server: the hash checked,
//! the install in its versioned folder, older versions removed, a damaged download and a
//! cancel leaving nothing.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use gezik_batch::archive::write::{self, CompressOptions, Level, OutFormat, WriteCx};
use gezik_batch::tasks::DownloadTask;
use gezik_batch::tools::install_dir;
use gezik_core::batch::tools::{Platform, Tool, ToolBuild};
use gezik_ops::{Engine, Event, PendingDeletes, Report, Settings};
use sha2::{Digest, Sha256};

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("gezik-download-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn engine(d: &Path) -> Engine {
    Engine::new(Settings { pending_deletes: Some(d.join("pending-deletes")), ..Settings::default() }, || {})
}

struct Quiet;

impl WriteCx for Quiet {
    fn add_bytes(&self, _: u64) {}
    fn entry_done(&self) {}
    fn entry_failed(&self, path: &Path, error: &std::io::Error) {
        panic!("{}: {error}", path.display());
    }
    fn stopped(&self) -> bool {
        false
    }
}

/// A zip made by Gezik's writer holding `7z.exe` and `readme.txt`.
fn tool_zip(d: &Path) -> Vec<u8> {
    tool_archive(d, OutFormat::Zip)
}

fn tool_archive(d: &Path, format: OutFormat) -> Vec<u8> {
    let src = d.join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("7z.exe"), b"not really 7-Zip").unwrap();
    std::fs::write(src.join("readme.txt"), b"hello").unwrap();
    let inputs = write::inputs(&[src.join("7z.exe"), src.join("readme.txt")], &mut |path, err| {
        panic!("{}: {err}", path.display())
    });
    let out = d.join("tool.out");
    let options = CompressOptions { format, level: Level::Normal, password: None, encrypt_names: false, split: None };
    write::write(&inputs, &out, &options, 1, &Quiet).unwrap();
    let body = std::fs::read(&out).unwrap();
    std::fs::remove_dir_all(&src).unwrap();
    std::fs::remove_file(&out).unwrap();
    body
}

fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

fn build(size: u64, sha256: String) -> &'static ToolBuild {
    build_of(size, sha256, "zip")
}

fn build_of(size: u64, sha256: String, kind: &'static str) -> &'static ToolBuild {
    Box::leak(Box::new(ToolBuild {
        tool: Tool::SevenZip,
        platform: Platform::current().unwrap(),
        version: "99.01",
        url: "https://example.invalid/7zip.zip",
        size,
        sha256: Box::leak(sha256.into_boxed_str()),
        programs: &["7z.exe"],
        kind,
    }))
}

/// A server answering every GET with `body`, `chunk` bytes at a time with `pause` between;
/// gives its address and a flag set once a whole body went out.
fn serve(body: Vec<u8>, chunk: usize, pause: Duration) -> (String, Arc<AtomicBool>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/7zip.zip", listener.local_addr().unwrap());
    let sent = Arc::new(AtomicBool::new(false));
    let flag = sent.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            if answer(stream, &body, chunk, pause).is_ok() {
                flag.store(true, Ordering::SeqCst);
            }
        }
    });
    (url, sent)
}

fn answer(mut stream: TcpStream, body: &[u8], chunk: usize, pause: Duration) -> std::io::Result<()> {
    let mut request = Vec::new();
    let mut byte = [0u8; 1];
    while !request.ends_with(b"\r\n\r\n") {
        if stream.read(&mut byte)? == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        request.push(byte[0]);
    }
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/zip\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    for piece in body.chunks(chunk) {
        stream.write_all(piece)?;
        stream.flush()?;
        if !pause.is_zero() {
            std::thread::sleep(pause);
        }
    }
    Ok(())
}

/// Runs `task` to the end; `on` sees each event and returns true to cancel.
fn run(engine: &Engine, task: DownloadTask, mut on: impl FnMut(&Event) -> bool) -> Report {
    let job = engine.submit(Box::new(task));
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        for event in engine.drain() {
            if on(&event) {
                engine.cancel(job);
            }
            if let Event::Finished { job: j, report } = event
                && j == job
            {
                return report;
            }
        }
        assert!(Instant::now() < deadline, "the download did not finish");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// The names in `tools` that Gezik's temporary files and staging folders have.
fn leftovers(tools: &Path) -> Vec<String> {
    std::fs::read_dir(tools)
        .map(|listing| {
            listing
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|name| name.starts_with(".gezik-"))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn download_checks_the_hash_and_installs() {
    let d = dir("installs");
    let body = tool_zip(&d);
    let build = build(body.len() as u64, hex_sha256(&body));
    let data = d.join("data");
    let tools = data.join("tools");
    // An older version goes, another tool's folder stays.
    std::fs::create_dir_all(tools.join("7zip-24.09")).unwrap();
    std::fs::write(tools.join("7zip-24.09").join("7z.exe"), b"old").unwrap();
    std::fs::create_dir_all(tools.join("ffmpeg-7.1")).unwrap();
    let (url, _) = serve(body, 64 * 1024, Duration::ZERO);
    let engine = engine(&d);
    let report = run(&engine, DownloadTask::new(build, data.clone()).with_url(url), |_| false);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert!(!report.cancelled);
    let installed = install_dir(build, &data);
    assert_eq!(installed, tools.join("7zip-99.01"));
    assert_eq!(std::fs::read(installed.join("7z.exe")).unwrap(), b"not really 7-Zip");
    assert_eq!(std::fs::read(installed.join("readme.txt")).unwrap(), b"hello");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(installed.join("7z.exe")).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o755);
    }
    assert!(!tools.join("7zip-24.09").exists(), "the older version is removed");
    assert!(tools.join("ffmpeg-7.1").is_dir());
    assert!(leftovers(&tools).is_empty(), "{:?}", leftovers(&tools));
    // Downloading is not undone.
    assert!(engine.undo().is_none());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_tar_xz_download_installs() {
    let d = dir("tar-xz");
    let body = tool_archive(&d, OutFormat::TarXz);
    let build = build_of(body.len() as u64, hex_sha256(&body), "tar.xz");
    let data = d.join("data");
    let (url, _) = serve(body, 64 * 1024, Duration::ZERO);
    let engine = engine(&d);
    let report = run(&engine, DownloadTask::new(build, data.clone()).with_url(url), |_| false);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(std::fs::read(install_dir(build, &data).join("7z.exe")).unwrap(), b"not really 7-Zip");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_bad_hash_leaves_nothing() {
    let d = dir("bad-hash");
    let body = tool_zip(&d);
    let build = build(body.len() as u64, "0".repeat(64));
    let data = d.join("data");
    let (url, _) = serve(body, 64 * 1024, Duration::ZERO);
    let engine = engine(&d);
    let report = run(&engine, DownloadTask::new(build, data.clone()).with_url(url), |_| false);
    assert_eq!(report.failures.len(), 1, "{:?}", report.failures);
    assert_eq!(report.failures[0].message, "download damaged — try again");
    assert!(!install_dir(build, &data).exists());
    let tools = data.join("tools");
    assert!(leftovers(&tools).is_empty(), "{:?}", leftovers(&tools));
    let pending = PendingDeletes::new(d.join("pending-deletes"));
    assert!(pending.load().is_empty() && pending.copies().is_empty());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_failed_download_says_what_curl_said() {
    let d = dir("refused");
    let build = build(10, "0".repeat(64));
    let data = d.join("data");
    // Nothing listens on the port of a listener that is gone.
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let engine = engine(&d);
    let task = DownloadTask::new(build, data.clone()).with_url(format!("http://127.0.0.1:{port}/7zip.zip"));
    let report = run(&engine, task, |_| false);
    assert_eq!(report.failures.len(), 1, "{:?}", report.failures);
    assert!(report.failures[0].message.starts_with("curl: ("), "{}", report.failures[0].message);
    assert!(leftovers(&data.join("tools")).is_empty());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn cancel_kills_curl() {
    let d = dir("cancel");
    // 4 MiB at 64 KiB per 100 ms: about 6.4 s if it ran to the end.
    let body: Vec<u8> = (0..4u32 * 1024 * 1024).map(|i| (i % 251) as u8).collect();
    let build = build(body.len() as u64, hex_sha256(&body));
    let data = d.join("data");
    let (url, sent) = serve(body, 64 * 1024, Duration::from_millis(100));
    let engine = engine(&d);
    let started = Instant::now();
    let report = run(
        &engine,
        DownloadTask::new(build, data.clone()).with_url(url),
        |event| matches!(event, Event::Progress { progress, .. } if progress.bytes_done > 0),
    );
    assert!(report.cancelled);
    assert!(started.elapsed() < Duration::from_secs(5), "{:?}", started.elapsed());
    assert!(!install_dir(build, &data).exists());
    let tools = data.join("tools");
    assert!(leftovers(&tools).is_empty(), "{:?}", leftovers(&tools));
    // curl is gone: the server cannot send the rest.
    std::thread::sleep(Duration::from_millis(500));
    assert!(!sent.load(Ordering::SeqCst), "the whole body went out");
    let _ = std::fs::remove_dir_all(&d);
}

/// Every build in MANIFEST, from the folder `scripts/tools/prepare.ps1` wrote (GEZIK_TOOLS_DIR),
/// downloads and installs with its programs and licence. Skipped without that folder.
#[test]
fn the_prepared_seven_zip_builds_install() {
    let Some(folder) = std::env::var_os("GEZIK_TOOLS_DIR").map(PathBuf::from) else { return };
    let d = dir("prepared");
    for build in gezik_core::batch::tools::MANIFEST {
        let name = build.url.rsplit('/').next().unwrap();
        let body = std::fs::read(folder.join(name)).unwrap_or_else(|err| panic!("{name}: {err}"));
        assert_eq!(body.len() as u64, build.size, "{name}");
        assert_eq!(hex_sha256(&body), build.sha256, "{name}");
        let data = d.join(format!("{:?}", build.platform));
        let (url, _) = serve(body, 64 * 1024, Duration::ZERO);
        let engine = engine(&d);
        let report = run(&engine, DownloadTask::new(build, data.clone()).with_url(url), |_| false);
        assert!(report.failures.is_empty(), "{name}: {:?}", report.failures);
        let installed = install_dir(build, &data);
        for program in build.programs.iter().chain(&["License.txt"]) {
            assert!(installed.join(program).is_file(), "{name}: {program}");
        }
    }
    let _ = std::fs::remove_dir_all(&d);
}
