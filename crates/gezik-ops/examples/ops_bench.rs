//! Times the engine on a generated tree of small files: copy, then an instant delete.
//! `cargo run -p gezik-ops --release --example ops_bench -- <work folder> [files]`

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gezik_ops::{CopyTask, DeleteTask, Engine, Event, JobId, Settings, Task};

fn make_tree(dir: &Path, files: usize) {
    let folders = (files / 100).max(1);
    for f in 0..folders {
        let folder = dir.join(format!("folder{f:03}"));
        std::fs::create_dir_all(&folder).unwrap();
        // Exactly `files` in all: the first `files % folders` folders get one more.
        for i in 0..files / folders + usize::from(f < files % folders) {
            // 4–64 KB, different sizes.
            let size = 4096 + (i * 7919 + f * 104_729) % (60 * 1024);
            std::fs::write(folder.join(format!("file{i:04}.bin")), vec![(i % 251) as u8; size]).unwrap();
        }
    }
}

const MARKER: &str = ".gezik-ops-bench";

/// Removes `work` only if it holds the marker this tool writes; refuses any other folder.
fn clean(work: &Path) {
    if !work.exists() {
        return;
    }
    if !work.join(MARKER).exists() {
        eprintln!(
            "refusing to delete {}: it was not created by ops_bench (no {MARKER} inside); pick a new folder",
            work.display()
        );
        std::process::exit(2);
    }
    std::fs::remove_dir_all(work).unwrap();
}

fn run(engine: &Engine, task: Box<dyn Task>) -> Duration {
    let started = Instant::now();
    let job: JobId = engine.submit(task);
    loop {
        for event in engine.drain() {
            match event {
                Event::Conflicts { job: j, conflicts } if j == job => {
                    engine.decide(job, conflicts.iter().map(|c| c.decision).collect());
                }
                Event::Finished { job: j, report } if j == job => {
                    assert!(report.failures.is_empty(), "{:?}", report.failures);
                    return started.elapsed();
                }
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let work = PathBuf::from(args.next().expect("usage: ops_bench <work folder> [files]"));
    let files: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(10_000);
    clean(&work);
    std::fs::create_dir_all(&work).unwrap();
    std::fs::write(work.join(MARKER), b"").unwrap();
    let (src, dst) = (work.join("src"), work.join("dst"));
    make_tree(&src, files);
    std::fs::create_dir_all(&dst).unwrap();
    let engine =
        Engine::new(Settings { pending_deletes: Some(work.join("pending-deletes")), ..Settings::default() }, || {});

    let copy = run(&engine, Box::new(CopyTask::into(vec![src.clone()], &dst)));
    println!("copy {files} files: {} ms", copy.as_millis());

    let copied = dst.join("src");
    let started = Instant::now();
    let pending = engine.pending_deletes();
    let job = engine.submit(Box::new(DeleteTask::new(vec![copied.clone()], pending)));
    let deadline = Instant::now() + Duration::from_secs(60);
    while copied.exists() {
        assert!(Instant::now() < deadline, "the deleted folder was still there after 60 s");
        std::thread::sleep(Duration::from_millis(1));
    }
    println!("delete: gone from the folder after {} ms", started.elapsed().as_millis());
    loop {
        let report = engine.drain().into_iter().find_map(|e| match e {
            Event::Finished { job: j, report } if j == job => Some(report),
            _ => None,
        });
        if let Some(report) = report {
            assert!(report.failures.is_empty(), "{:?}", report.failures);
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    println!("delete {files} files: {} ms in all", started.elapsed().as_millis());
    clean(&work);
}
