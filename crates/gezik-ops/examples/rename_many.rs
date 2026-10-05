//! cargo run --release -p gezik-ops --example rename_many -- <folder with files> [--case]
//!
//! Renames every file in the folder to `r-<name>`, or with `--case` to its name in upper case
//! (through a temporary name each where the file system ignores case). Restore notes go to a
//! `pending-deletes` file in the system's temp folder, as the app keeps them.
fn main() {
    let mut args = std::env::args().skip(1);
    let dir = std::path::PathBuf::from(args.next().expect("a folder"));
    let case = args.any(|arg| arg == "--case");
    let pairs: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| {
            let p = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            let new = if case { name.to_uppercase() } else { format!("r-{name}") };
            let new = p.with_file_name(new);
            (p, new)
        })
        .filter(|(p, new)| p != new)
        .collect();
    let pending = std::env::temp_dir().join(format!("gezik-rename-many-{}", std::process::id()));
    let settings = gezik_ops::Settings { pending_deletes: Some(pending.clone()), ..gezik_ops::Settings::default() };
    let engine = gezik_ops::Engine::new(settings, || {});
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
    if pending.exists() {
        eprintln!("notes left in {}", pending.display());
    }
}
