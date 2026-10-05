//! cargo run --release -p gezik-batch --example archive_bench -- <folder> [work folder]
//! Compresses the folder to a zip (Normal) and extracts that zip, both through the engine as
//! the app does, and prints the times.
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gezik_batch::tasks::{CompressOptions, CompressTask, ExtractTo, Level, OutFormat, extract_chain, extract_label};
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

fn main() {
    let mut args = std::env::args().skip(1);
    let folder = PathBuf::from(args.next().expect("usage: archive_bench <folder> [work folder]"));
    let work = args.next().map(PathBuf::from).unwrap_or_else(|| std::env::temp_dir().join("gezik-archive-bench"));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let engine =
        Engine::new(Settings { pending_deletes: Some(work.join("pending-deletes")), ..Settings::default() }, || {});

    let sources: Vec<PathBuf> = std::fs::read_dir(&folder).unwrap().map(|e| e.unwrap().path()).collect();
    let zip = work.join("bench.zip");
    let options = CompressOptions {
        format: OutFormat::Zip,
        level: Level::Normal,
        password: None,
        encrypt_names: false,
        split: None,
    };
    let start = Instant::now();
    let task = CompressTask::new(sources, zip.clone(), options);
    let failed = finish(&engine, engine.submit(Box::new(task)));
    let compress = start.elapsed();
    println!("gezik compress: {} ms ({failed} failures, {} bytes)", compress.as_millis(), size(&zip));

    let out = work.join("out");
    std::fs::create_dir_all(&out).unwrap();
    let archives = vec![zip.clone()];
    let label = extract_label(&archives);
    let start = Instant::now();
    let job = engine.submit_chain(extract_chain(archives, ExtractTo::Into(out.clone()), None), Some(label));
    let failed = finish(&engine, job);
    println!("gezik extract: {} ms ({failed} failures)", start.elapsed().as_millis());
    let _ = std::fs::remove_dir_all(&work);
}

fn size(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}
