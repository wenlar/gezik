//! Whether a duplicates search gives its memory back as a plain search does (spec 11 §9):
//! `cargo run --release -p gezik-search --example dup_memory -- <empty folder>` makes 5,000
//! files (100 pairs) there, then runs each kind three times, its results dropped after each,
//! and prints this process's private bytes (Windows; nothing elsewhere).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use gezik_core::search::{Scope, SearchSpec};
use gezik_search::query::{Query, QueryOptions};
use gezik_search::results::ResultSet;
use gezik_search::run::{Event, Running, plan_walk, start};

fn plain(root: &Path) -> ResultSet {
    let spec = SearchSpec::flat_view(root.to_path_buf());
    let query = Query::compile(&spec, &QueryOptions::local(0, 250_000)).expect("a good search");
    let (tx, rx) = mpsc::channel();
    start(plan_walk(&spec, &[], (true, false)), query, move |event| {
        let _ = tx.send(event);
    });
    let mut set = ResultSet::new(root.to_path_buf(), false);
    loop {
        match rx.recv_timeout(Duration::from_secs(600)).expect("the search ends") {
            Event::Batch(batch) => set.append(batch),
            Event::Done(_) => return set,
            _ => {}
        }
    }
}

fn duplicates(root: &Path) -> ResultSet {
    let spec = SearchSpec { duplicates: true, ..SearchSpec::new(Scope::Folder(root.to_path_buf())) };
    let query = Query::compile(&spec, &QueryOptions::local(0, 250_000)).expect("a good search");
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    let sink = Arc::new(move |event| sink.lock().unwrap().push(event));
    gezik_search::duplicates::find(plan_walk(&spec, &[], (true, false)), query, root, 4, &Running::default(), sink);
    let mut set = ResultSet::duplicates(root.to_path_buf());
    for event in std::mem::take(&mut *events.lock().unwrap()) {
        if let Event::Batch(batch) = event {
            set.append(batch);
        }
    }
    set
}

fn main() {
    let root = PathBuf::from(std::env::args().nth(1).expect("an empty folder"));
    for i in 0..5_000u32 {
        let folder = root.join(format!("d{}", i / 100));
        std::fs::create_dir_all(&folder).unwrap();
        // Files 0..200 are 100 pairs; the rest have sizes of their own.
        let data = if i < 200 { vec![(i / 2) as u8; 4096] } else { vec![1u8; 4096 + i as usize] };
        std::fs::write(folder.join(format!("f{i}.bin")), data).unwrap();
    }
    private("before");
    for round in 1..=3 {
        let set = plain(&root);
        private(&format!("plain {round}: {} rows, kept", set.len()));
        drop(set);
        private(&format!("plain {round}: dropped"));
        let set = duplicates(&root);
        private(&format!("duplicates {round}: {} rows, kept", set.len()));
        drop(set);
        private(&format!("duplicates {round}: dropped"));
    }
}

/// This process's private bytes (Windows; nothing elsewhere).
#[cfg(windows)]
fn private(label: &str) {
    #[repr(C)]
    #[derive(Default)]
    struct Counters {
        cb: u32,
        page_fault_count: u32,
        rest: [usize; 8],
        private_usage: usize,
    }
    unsafe extern "system" {
        fn GetCurrentProcess() -> isize;
        fn K32GetProcessMemoryInfo(process: isize, counters: *mut Counters, cb: u32) -> i32;
    }
    let mut c = Counters { cb: size_of::<Counters>() as u32, ..Default::default() };
    // SAFETY: a valid pseudo handle and a PROCESS_MEMORY_COUNTERS_EX of the size given.
    unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) };
    println!("{label}: private {:.2} MB", c.private_usage as f64 / 1_048_576.0);
}

#[cfg(not(windows))]
fn private(_: &str) {}
