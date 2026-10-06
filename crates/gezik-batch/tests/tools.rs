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
use gezik_batch::convert::ffmpeg::find_ffmpeg;
use gezik_batch::pdf::client::{WORKER_ARG, Worker, count_pages};
use gezik_batch::pdf::images::write_pdf;
use gezik_batch::tasks::DownloadTask;
use gezik_batch::tools::{find, install_dir};
use gezik_core::batch::pdf::PageOptions;
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
fn run(engine: &Engine, task: DownloadTask, on: impl FnMut(&Event) -> bool) -> Report {
    run_within(engine, task, Duration::from_secs(60), on)
}

/// `run`, failing when it takes longer than `limit`.
fn run_within(engine: &Engine, task: DownloadTask, limit: Duration, mut on: impl FnMut(&Event) -> bool) -> Report {
    let job = engine.submit(Box::new(task));
    let deadline = Instant::now() + limit;
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

/// The ffmpeg downloads are solid 7z archives; on Unix the programs come out executable
/// whatever modes the archive holds (7-Zip on Windows stores none).
#[test]
fn a_7z_download_installs() {
    let d = dir("7z");
    let body = tool_archive(&d, OutFormat::SevenZ);
    let build = build_of(body.len() as u64, hex_sha256(&body), "7z");
    let data = d.join("data");
    let (url, _) = serve(body, 64 * 1024, Duration::ZERO);
    let engine = engine(&d);
    let report = run(&engine, DownloadTask::new(build, data.clone()).with_url(url), |_| false);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    let installed = install_dir(build, &data);
    assert_eq!(std::fs::read(installed.join("7z.exe")).unwrap(), b"not really 7-Zip");
    assert_eq!(std::fs::read(installed.join("readme.txt")).unwrap(), b"hello");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(installed.join("7z.exe")).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o755);
    }
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
fn a_failed_download_says_why() {
    let d = dir("refused");
    let build = build(10, "0".repeat(64));
    let data = d.join("data");
    // Nothing listens on the port of a listener that is gone.
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let engine = engine(&d);
    let task = DownloadTask::new(build, data.clone()).with_url(format!("http://127.0.0.1:{port}/7zip.zip"));
    let report = run(&engine, task, |_| false);
    assert_eq!(report.failures.len(), 1, "{:?}", report.failures);
    // WinHTTP, NSURLSession, curl and wget all say they could not connect.
    let message = &report.failures[0].message;
    assert!(message.to_lowercase().contains("connect"), "{message}");
    assert!(leftovers(&data.join("tools")).is_empty());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn cancel_stops_the_download() {
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
    // The download is gone: the server cannot send the rest.
    std::thread::sleep(Duration::from_millis(500));
    assert!(!sent.load(Ordering::SeqCst), "the whole body went out");
    let _ = std::fs::remove_dir_all(&d);
}

/// A 3-page PDF written by Gezik (`write_pdf`, a picture per page) in `d`.
fn three_page_pdf(d: &Path) -> PathBuf {
    let pics: Vec<PathBuf> = (0..3u8)
        .map(|i| {
            let png = d.join(format!("pic{i}.png"));
            image::RgbImage::from_fn(64, 48, |x, y| image::Rgb([x as u8 * 4, y as u8 * 5, i * 60])).save(&png).unwrap();
            png
        })
        .collect();
    let out = d.join("three.pdf");
    write_pdf(&pics, &out, &PageOptions::DEFAULT, &mut |_, _| {}, &|| false).unwrap();
    out
}

/// The real PDF worker (`examples/pdf_worker.rs`, built by `cargo test -p gezik-batch`).
fn pdf_worker() -> Worker {
    let deps = std::env::current_exe().unwrap().parent().unwrap().to_path_buf();
    let program = deps.parent().unwrap().join("examples").join(format!("pdf_worker{}", std::env::consts::EXE_SUFFIX));
    assert!(program.is_file(), "{} is missing: run `cargo test -p gezik-batch`", program.display());
    Worker { program, args: vec![WORKER_ARG.into()] }
}

/// Every build in MANIFEST whose file is in the folder `scripts/tools/prepare.ps1`,
/// `prepare-ffmpeg.ps1` or `prepare-pdfium.ps1` wrote (GEZIK_TOOLS_DIR) downloads and installs
/// with its programs and licence; this machine's ffmpeg is then found there and is 9.0 or
/// newer, and this machine's pdfium is found there and counts the pages of a PDF through the
/// worker (chromium/8086 loads with the `pdfium_7881` bindings). Skipped without that folder.
#[test]
fn the_prepared_builds_install() {
    let Some(folder) = std::env::var_os("GEZIK_TOOLS_DIR").map(PathBuf::from) else { return };
    let d = dir("prepared");
    let mut tested = 0;
    for build in gezik_core::batch::tools::MANIFEST {
        let name = build.url.rsplit('/').next().unwrap();
        let Ok(body) = std::fs::read(folder.join(name)) else { continue };
        assert_eq!(body.len() as u64, build.size, "{name}");
        assert_eq!(hex_sha256(&body), build.sha256, "{name}");
        let data = d.join(format!("{:?}-{:?}", build.tool, build.platform));
        let (url, _) = serve(body, 1024 * 1024, Duration::ZERO);
        let engine = engine(&d);
        // ffmpeg unpacks to about 400 MB, which takes minutes in a debug build.
        let task = DownloadTask::new(build, data.clone()).with_url(url);
        let report = run_within(&engine, task, Duration::from_secs(1800), |_| false);
        assert!(report.failures.is_empty(), "{name}: {:?}", report.failures);
        let installed = install_dir(build, &data);
        let extra: &[&str] = match build.tool {
            Tool::SevenZip => &["License.txt"],
            Tool::Ffmpeg => &["LICENSE", "SOURCE.txt"],
            Tool::Pdfium => &["LICENSE", "SOURCE.txt", "licenses/pdfium.txt"],
        };
        for file in build.programs.iter().chain(extra) {
            let path = file.split('/').fold(installed.clone(), |path, part| path.join(part));
            assert!(path.is_file(), "{name}: {file}");
        }
        if build.tool == Tool::Ffmpeg && Some(build.platform) == Platform::current() {
            let found = find_ffmpeg(&data, None).expect("the installed ffmpeg is found");
            assert_eq!(found.ffmpeg, installed.join(build.programs[0]));
            assert_eq!(found.ffprobe.as_deref(), Some(installed.join(build.programs[1]).as_path()));
            assert!(found.version.is_some_and(|v| v >= (9, 0)), "{:?}", found.version);
        }
        if build.tool == Tool::Pdfium && Some(build.platform) == Platform::current() {
            let library = find(Tool::Pdfium, &data, None).expect("the installed pdfium is found");
            assert_eq!(library, installed.join(build.programs[0]));
            let pdf = three_page_pdf(&data);
            assert_eq!(count_pages(&pdf_worker(), &library, &pdf, &|| false).unwrap(), Some(3));
        }
        let _ = std::fs::remove_dir_all(&data);
        tested += 1;
    }
    assert!(tested > 0, "no MANIFEST file is in {}", folder.display());
    let _ = std::fs::remove_dir_all(&d);
}
