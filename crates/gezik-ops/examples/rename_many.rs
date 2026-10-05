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
    loop {
        if engine.drain().iter().any(|e| matches!(e, gezik_ops::Event::Finished { job: j, .. } if *j == job)) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    println!("renamed in {} ms", start.elapsed().as_millis());
}
