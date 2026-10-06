//! cargo run --release -p gezik-batch --example convert_bench -- [work folder] [photos] [text MB] [12|24]
//! Makes `photos` (100) JPEG photos of 12 MP (4000×3000) or 24 MP (6000×4000) and converts
//! them with "Resize photos (JPEG 1920 px)" into a subfolder, then makes a `text MB` (100) MB
//! Windows-1254 text and converts it to UTF-8 (its encoding detected), both through the engine
//! as the app does (pictures on every core), and prints the times. The work folder is removed
//! at the end.
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gezik_batch::tasks::{ConvertTask, ConvertTools, ConvertWhat};
use gezik_core::batch::convert::{Eol, Output, PresetWhat, TextOptions, preset};
use gezik_ops::{Answer, Engine, Event, JobId, Settings};

/// Runs `job` to the end; conflicts take their defaults, questions are cancelled.
fn finish(engine: &Engine, job: JobId) -> usize {
    loop {
        for event in engine.drain() {
            match event {
                Event::Conflicts { job: j, conflicts } if j == job => {
                    engine.decide(job, conflicts.iter().map(|c| c.decision).collect());
                }
                Event::Question { job: j, id, .. } if j == job => engine.answer(job, id, Answer::Cancel),
                Event::Finished { job: j, report } if j == job => return report.failures.len(),
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// A photo-like `w`×`h` picture: smooth colour gradients with grain (a picture without
/// grain compresses far better than a camera's), different for each `seed`.
fn photo(path: &Path, seed: u32, (width, height): (u16, u16)) {
    let (w, h) = (usize::from(width), usize::from(height));
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
    let encoder = jpeg_encoder::Encoder::new_file(path, 92).unwrap();
    encoder.encode(&pixels, width, height, jpeg_encoder::ColorType::Rgb).unwrap();
}

/// `mb` MB of Turkish text in Windows-1254, CRLF lines.
fn text(path: &Path, mb: usize) {
    let line = "Çağlar boyunca İstanbul'un sokaklarında yürüyen gölgeler, şehrin ışığını öğrendi.\r\n";
    let (bytes, _, unmappable) = encoding_rs::WINDOWS_1254.encode(line);
    assert!(!unmappable);
    let mut out = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    let mut written = 0;
    while written < mb * 1_000_000 {
        out.write_all(&bytes).unwrap();
        written += bytes.len();
    }
    out.flush().unwrap();
}

fn size(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

fn folder_size(path: &Path) -> u64 {
    std::fs::read_dir(path).map(|list| list.map(|e| size(&e.unwrap().path())).sum()).unwrap_or(0)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let work = args.next().map(PathBuf::from).unwrap_or_else(|| std::env::temp_dir().join("gezik-convert-bench"));
    let photos: u32 = args.next().map_or(100, |n| n.parse().expect("photos: a number"));
    let mb: usize = args.next().map_or(100, |n| n.parse().expect("text MB: a number"));
    let dims: (u16, u16) = match args.next().as_deref() {
        None | Some("12") => (4000, 3000),
        Some("24") => (6000, 4000),
        Some(other) => panic!("megapixels: 12 or 24, not {other}"),
    };
    let (width, height) = dims;
    let _ = std::fs::remove_dir_all(&work);
    let pictures = work.join("photos");
    std::fs::create_dir_all(&pictures).unwrap();
    let engine =
        Engine::new(Settings { pending_deletes: Some(work.join("pending-deletes")), ..Settings::default() }, || {});
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());

    // The photos, made on every core.
    let started = Instant::now();
    let inputs: Vec<PathBuf> = (0..photos).map(|n| pictures.join(format!("IMG_{n:04}.jpg"))).collect();
    std::thread::scope(|scope| {
        for (t, chunk) in inputs.chunks(inputs.len().div_ceil(cores).max(1)).enumerate() {
            let first = t * inputs.len().div_ceil(cores).max(1);
            scope.spawn(move || {
                for (n, path) in chunk.iter().enumerate() {
                    photo(path, (first + n) as u32, dims);
                }
            });
        }
    });
    println!(
        "made {photos} photos {width}x{height} ({:.1} MB) in {} ms",
        folder_size(&pictures) as f64 / 1e6,
        started.elapsed().as_millis()
    );

    let resize = preset("resize-photos").unwrap();
    let PresetWhat::Image(options) = resize.what else { unreachable!() };
    let task = ConvertTask::new(inputs, ConvertWhat::Image(options), Output::Subfolder, ConvertTools::default());
    let started = Instant::now();
    let failed = finish(&engine, engine.submit(Box::new(task)));
    let took = started.elapsed();
    let out = pictures.join("converted");
    let made = std::fs::read_dir(&out).map(|list| list.count()).unwrap_or(0);
    println!(
        "resize photos (JPEG 1920 px, {cores} cores): {} ms, {:.1} ms per photo ({failed} failures, {made} files, {:.1} MB)",
        took.as_millis(),
        took.as_secs_f64() * 1000.0 / f64::from(photos.max(1)),
        folder_size(&out) as f64 / 1e6
    );

    let txt = work.join("big.txt");
    text(&txt, mb);
    let options = TextOptions {
        from: None,
        to: "UTF-8".into(),
        bom: false,
        eol: Eol::Keep,
        trim_trailing: false,
        final_newline: false,
    };
    let task =
        ConvertTask::new(vec![txt.clone()], ConvertWhat::Text(options), Output::SameFolder, ConvertTools::default());
    let started = Instant::now();
    let failed = finish(&engine, engine.submit(Box::new(task)));
    let took = started.elapsed();
    let utf8 = work.join("big (converted).txt");
    println!(
        "text {:.1} MB Windows-1254 -> UTF-8: {} ms, {:.0} MB/s ({failed} failures, {:.1} MB out)",
        size(&txt) as f64 / 1e6,
        took.as_millis(),
        size(&txt) as f64 / 1e6 / took.as_secs_f64(),
        size(&utf8) as f64 / 1e6
    );
    drop(engine);
    let _ = std::fs::remove_dir_all(&work);
}
