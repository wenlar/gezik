//! cargo run --release -p gezik-ops --example rename_many -- <folder with files>
fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("a folder"));
    let pairs: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| {
            let p = e.path();
            let new = p.with_file_name(format!("r-{}", e.file_name().to_string_lossy()));
            (p, new)
        })
        .collect();
    let engine = gezik_ops::Engine::new(gezik_ops::Settings::default(), || {});
    let start = std::time::Instant::now();
    let job = engine.submit(Box::new(gezik_ops::RenameTask::many(pairs)));
    let deadline = start + std::time::Duration::from_secs(120);
    'wait: loop {
        for event in engine.drain() {
            if let gezik_ops::Event::Finished { job: j, report } = event
                && j == job
            {
                for failure in &report.failures {
                    eprintln!("failed: {failure:?}");
                }
                break 'wait;
            }
        }
        assert!(std::time::Instant::now() < deadline, "rename job did not finish in 120 s");
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    println!("renamed in {} ms", start.elapsed().as_millis());
}
