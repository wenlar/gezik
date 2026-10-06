//! cargo run --release -p gezik-batch --example pdf_bench -- [work folder] [photos]
//! Makes `photos` (100) JPEG photos of 12 MP (4000×3000) and 5 PNGs of 12 MP (in a child
//! process, so that this one's peak memory is the PDF work's), then through the engine as the
//! app does:
//! - "Images to PDF" of the JPEGs: time, output size, this process's peak memory;
//! - "Images to PDF" of the PNGs: time;
//! - with `GEZIK_TEST_PDFIUM` (the pdfium library): "Split PDF" each page of the 100-page PDF,
//!   and "PDF to images" PNG at 150 dpi, timed only (the worker is another process), each
//!   followed by a count of the worker processes still running (there should be none).
//!
//! The worker is this program (`--pdf-worker`), or `PDF_BENCH_WORKER` (e.g. a release
//! `gezik.exe`, run with `--pdf-worker`). The work folder is removed at the end.
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gezik_batch::pdf::client::Worker;
use gezik_batch::tasks::{ImagesToPdfTask, PdfTools, PdfWork, pdf_chain, pdf_label};
use gezik_core::batch::pdf::{PageImage, PageOptions, Split};
use gezik_ops::{Answer, Engine, Event, JobId, Settings};

/// Runs `job` to the end; conflicts take their defaults, questions are cancelled. Returns
/// the failures and the skipped inputs.
fn finish(engine: &Engine, job: JobId) -> (usize, usize) {
    loop {
        for event in engine.drain() {
            match event {
                Event::Conflicts { job: j, conflicts } if j == job => {
                    engine.decide(job, conflicts.iter().map(|c| c.decision).collect());
                }
                Event::Question { job: j, id, .. } if j == job => engine.answer(job, id, Answer::Cancel),
                Event::Finished { job: j, report } if j == job => {
                    return (report.failures.len(), report.skipped.len());
                }
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// A photo-like `w`×`h` picture's RGB samples: smooth colour gradients with grain (a picture
/// without grain compresses far better than a camera's), different for each `seed`.
fn samples(seed: u32, w: usize, h: usize) -> Vec<u8> {
    let mut pixels = vec![0u8; w * h * 3];
    let mut state = seed.wrapping_mul(2_654_435_761) | 1;
    for (y, row) in pixels.chunks_exact_mut(w * 3).enumerate() {
        for (x, px) in row.as_chunks_mut::<3>().0.iter_mut().enumerate() {
            // xorshift32 grain, ±16.
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let grain = (state & 31) as i32 - 16;
            let base = [(x * 255 / w) as i32, (y * 255 / h) as i32, (((x + y + seed as usize * 97) / 23) % 256) as i32];
            for (c, b) in px.iter_mut().zip(base) {
                *c = (b + grain).clamp(0, 255) as u8;
            }
        }
    }
    pixels
}

const W: u16 = 4000;
const H: u16 = 3000;

fn jpeg_names(dir: &Path, count: u32) -> Vec<PathBuf> {
    (0..count).map(|n| dir.join(format!("IMG_{n:04}.jpg"))).collect()
}

fn png_names(dir: &Path) -> Vec<PathBuf> {
    (0..5).map(|n| dir.join(format!("scan_{n}.png"))).collect()
}

/// The child: writes the pictures on every core.
fn make(dir: &Path, count: u32) {
    std::fs::create_dir_all(dir).unwrap();
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    let mut jobs: Vec<(PathBuf, u32)> = jpeg_names(dir, count).into_iter().zip(0..).collect();
    jobs.extend(png_names(dir).into_iter().zip(1000..));
    let chunk = jobs.len().div_ceil(cores).max(1);
    std::thread::scope(|scope| {
        for part in jobs.chunks(chunk) {
            scope.spawn(move || {
                for (path, seed) in part {
                    let pixels = samples(*seed, usize::from(W), usize::from(H));
                    if path.extension().is_some_and(|e| e == "png") {
                        image::save_buffer(path, &pixels, W.into(), H.into(), image::ExtendedColorType::Rgb8).unwrap();
                    } else {
                        let encoder = jpeg_encoder::Encoder::new_file(path, 92).unwrap();
                        encoder.encode(&pixels, W, H, jpeg_encoder::ColorType::Rgb).unwrap();
                    }
                }
            });
        }
    });
}

fn size(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

fn folder(path: &Path) -> (usize, u64) {
    std::fs::read_dir(path)
        .map(|list| list.map(|e| size(&e.unwrap().path())).fold((0, 0), |(n, s), b| (n + 1, s + b)))
        .unwrap_or((0, 0))
}

/// This process's peak memory in MiB (peak working set on Windows, `ru_maxrss` on Unix).
#[cfg(windows)]
fn peak_mib() -> f64 {
    #[repr(C)]
    #[derive(Default)]
    struct Counters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }
    unsafe extern "system" {
        fn GetCurrentProcess() -> isize;
        fn K32GetProcessMemoryInfo(process: isize, counters: *mut Counters, cb: u32) -> i32;
    }
    let mut c = Counters { cb: size_of::<Counters>() as u32, ..Default::default() };
    // SAFETY: a valid pseudo handle and a counters struct of the size given.
    unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) };
    c.peak_working_set_size as f64 / 1_048_576.0
}

#[cfg(unix)]
fn peak_mib() -> f64 {
    /// `struct rusage` on 64-bit Linux and macOS: two `timeval`s, then 14 `long`s.
    #[repr(C)]
    struct Usage {
        times: [i64; 4],
        longs: [i64; 14],
    }
    unsafe extern "C" {
        fn getrusage(who: i32, usage: *mut Usage) -> i32;
    }
    let mut usage = Usage { times: [0; 4], longs: [0; 14] };
    // SAFETY: RUSAGE_SELF (0) and a struct of rusage's size.
    unsafe { getrusage(0, &mut usage) };
    let maxrss = usage.longs[0] as f64;
    // Kilobytes on Linux, bytes on macOS.
    if cfg!(target_os = "macos") { maxrss / 1_048_576.0 } else { maxrss / 1024.0 }
}

/// How many processes run with `--pdf-worker` (the worker of a finished job must be gone).
fn workers_running() -> String {
    // Split so that the asking process's own command line does not match.
    let pattern = ["--pdf", "-worker"].concat();
    #[cfg(windows)]
    let out = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!(
                "@(Get-CimInstance Win32_Process | Where-Object {{ $_.CommandLine -like ('*' + '{}' + '{}' + '*') }}).Count",
                &pattern[..5],
                &pattern[5..]
            ),
        ])
        .output();
    #[cfg(unix)]
    let out = std::process::Command::new("ps").args(["-eo", "args"]).output().map(|mut o| {
        let text = String::from_utf8_lossy(&o.stdout).into_owned();
        o.stdout = text.lines().filter(|l| l.contains(&pattern)).count().to_string().into_bytes();
        o
    });
    match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_owned(),
        Err(e) => format!("unknown ({e})"),
    }
}

fn main() {
    let mut args = std::env::args_os().skip(1);
    let first = args.next();
    if first.as_deref() == Some("--pdf-worker".as_ref()) {
        std::process::exit(gezik_batch::pdf::worker::main());
    }
    if first.as_deref() == Some("--make".as_ref()) {
        let dir = PathBuf::from(args.next().unwrap());
        let count = args.next().unwrap().to_string_lossy().parse().unwrap();
        return make(&dir, count);
    }
    let work = first.map(PathBuf::from).unwrap_or_else(|| std::env::temp_dir().join("gezik-pdf-bench"));
    let photos: u32 = args.next().map_or(100, |n| n.to_string_lossy().parse().expect("photos: a number"));
    let _ = std::fs::remove_dir_all(&work);
    let pictures = work.join("pictures");

    let started = Instant::now();
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("--make")
        .arg(&pictures)
        .arg(photos.to_string())
        .status()
        .unwrap();
    assert!(status.success(), "making the pictures failed");
    let jpegs = jpeg_names(&pictures, photos);
    let pngs = png_names(&pictures);
    let total = |list: &[PathBuf]| list.iter().map(|p| size(p)).sum::<u64>() as f64 / 1e6;
    println!(
        "made {photos} JPEGs {W}x{H} ({:.1} MB) and 5 PNGs ({:.1} MB) in {} ms; peak before the work {:.1} MiB",
        total(&jpegs),
        total(&pngs),
        started.elapsed().as_millis(),
        peak_mib()
    );

    let engine =
        Engine::new(Settings { pending_deletes: Some(work.join("pending-deletes")), ..Settings::default() }, || {});
    let a4 = PageOptions { size: gezik_core::batch::pdf::PageSize::A4, ..PageOptions::DEFAULT };

    let pdf = work.join("photos.pdf");
    let started = Instant::now();
    let (failed, _) = finish(&engine, engine.submit(Box::new(ImagesToPdfTask::new(jpegs, pdf.clone(), a4))));
    println!(
        "images to PDF, {photos} JPEGs (A4): {} ms, {:.1} MB out ({failed} failures); peak {:.1} MiB",
        started.elapsed().as_millis(),
        size(&pdf) as f64 / 1e6,
        peak_mib()
    );

    let scans = work.join("scans.pdf");
    let started = Instant::now();
    let (failed, _) =
        finish(&engine, engine.submit(Box::new(ImagesToPdfTask::new(pngs, scans.clone(), PageOptions::DEFAULT))));
    println!(
        "images to PDF, 5 PNGs: {} ms, {:.1} MB out ({failed} failures); peak {:.1} MiB",
        started.elapsed().as_millis(),
        size(&scans) as f64 / 1e6,
        peak_mib()
    );

    if let Some(library) = std::env::var_os("GEZIK_TEST_PDFIUM").filter(|l| !l.is_empty()) {
        let worker = match std::env::var_os("PDF_BENCH_WORKER").filter(|w| !w.is_empty()) {
            Some(program) => Worker { program: program.into(), args: vec!["--pdf-worker".into()] },
            None => Worker::this_exe().unwrap(),
        };
        println!("worker: {}", worker.program.display());
        let tools = PdfTools { worker, library: library.into() };
        // The PDF alone in its folder, so that the outputs can be counted.
        let split_dir = work.join("split");
        std::fs::create_dir_all(&split_dir).unwrap();
        let input = split_dir.join("photos.pdf");
        std::fs::rename(&pdf, &input).unwrap();
        for (what, job) in [
            ("split each page", PdfWork::Split(Split::EachPage)),
            ("PDF to images, PNG 150 dpi", PdfWork::Render { dpi: 150, image: PageImage::Png }),
        ] {
            let before = folder(&split_dir);
            let label = pdf_label(&job, std::slice::from_ref(&input));
            let started = Instant::now();
            let id = engine.submit_chain(pdf_chain(job, vec![input.clone()], tools.clone()), Some(label));
            let (failed, skipped) = finish(&engine, id);
            let took = started.elapsed();
            let after = folder(&split_dir);
            println!(
                "{what}: {} ms, {} files, {:.1} MB ({failed} failures, {skipped} skipped); worker processes left: {}",
                took.as_millis(),
                after.0 - before.0,
                (after.1 - before.1) as f64 / 1e6,
                workers_running()
            );
        }
    }
    drop(engine);
    let _ = std::fs::remove_dir_all(&work);
}
