//! Times a search, a content search, the flat view and the name cache on a tree (spec 12):
//! `cargo run --release -p gezik-search --example search_bench -- <tree> [pattern] [text]`.
//! `scripts/perf/search.ps1` makes the tree and reads the peak memory.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use gezik_core::search::{Scope, SearchSpec};
use gezik_core::sort::SortSpec;
use gezik_search::cache::{CACHE_LIMIT, CacheOutcome, build};
use gezik_search::query::{Query, QueryOptions};
use gezik_search::results::ResultSet;
use gezik_search::run::{Event, plan_walk, start};

/// Runs `spec`, printing the time to the first batch and to the end.
fn timed(label: &str, root: &Path, spec: &SearchSpec) -> ResultSet {
    let options = QueryOptions::local(64 * 1024 * 1024, 250_000);
    let query = Query::compile(spec, &options).expect("a good search");
    let walk = plan_walk(spec, &[".git".into(), "node_modules".into()], (true, false));
    let (tx, rx) = mpsc::channel();
    let started = Instant::now();
    start(walk, query, move |event| {
        let _ = tx.send(event);
    });
    let mut set = ResultSet::new(root.to_path_buf(), !spec.content.is_empty());
    let mut first = None;
    loop {
        match rx.recv_timeout(Duration::from_secs(600)).expect("the search ends") {
            Event::Batch(batch) => {
                first.get_or_insert_with(|| started.elapsed());
                set.append(batch);
            }
            Event::Progress { .. } => {}
            Event::Done(summary) => {
                println!(
                    "{label}: {} found, first {:?}, all {:?}, {} folders, limit {}",
                    summary.found,
                    first.unwrap_or_default(),
                    started.elapsed(),
                    summary.folders,
                    summary.limit_reached
                );
                memory(label);
                return set;
            }
        }
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let root = PathBuf::from(args.next().expect("a tree"));
    let pattern = args.next().unwrap_or_else(|| "file_12*".to_owned());
    let text = args.next().unwrap_or_else(|| "needle".to_owned());
    let mut spec = SearchSpec::new(Scope::Folder(root.clone()));
    spec.pattern = pattern;
    let found = timed("name", &root, &spec);
    let bytes = found.len() * (std::mem::size_of::<gezik_core::Entry>() + 4 + 24);
    println!("name: about {} KB of results", bytes / 1024);
    let mut content = SearchSpec::new(Scope::Folder(root.join("text")));
    content.content = text;
    timed("content", &root, &content);
    let mut flat = timed("flat", &root, &SearchSpec::flat_view(root.clone()));
    println!("flat: {} results", flat.len());
    // The list sorts the results when the search ends (by name here, as it opens).
    let started = Instant::now();
    let order = gezik_core::sort::sort_order(flat.entries(), SortSpec::default(), true, |_| String::new(), &|i| {
        flat.folder(i).unwrap_or("")
    });
    flat.apply_order(&order);
    println!("sort: {:?}", started.elapsed());
    memory("sort");
    let walk = plan_walk(&SearchSpec::flat_view(root.join("half")), &[], (true, false));
    let started = Instant::now();
    match build(walk, Arc::default(), CACHE_LIMIT).0 {
        CacheOutcome::Ready(cache) => {
            println!("cache: {} items in {:?}, {} KB", cache.len(), started.elapsed(), cache.heap_bytes() / 1024);
            let mut again = SearchSpec::new(Scope::Folder(root.join("half")));
            again.pattern = "file_3*".to_owned();
            let query = Query::compile(&again, &QueryOptions::local(0, 250_000)).expect("a good search");
            let started = Instant::now();
            let (set, _) = cache.select(&query, &AtomicBool::new(false)).expect("not cancelled");
            println!("cache select: {} in {:?}", set.len(), started.elapsed());
            memory("cache");
        }
        CacheOutcome::TooLarge => println!("cache: too large"),
        CacheOutcome::Cancelled => println!("cache: cancelled"),
    }
}

/// This process's working set and its peak so far (Windows; nothing elsewhere).
#[cfg(windows)]
fn memory(label: &str) {
    #[repr(C)]
    #[derive(Default)]
    struct Counters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        rest: [usize; 6],
    }
    unsafe extern "system" {
        fn GetCurrentProcess() -> isize;
        fn K32GetProcessMemoryInfo(process: isize, counters: *mut Counters, cb: u32) -> i32;
    }
    let mut c = Counters { cb: size_of::<Counters>() as u32, ..Default::default() };
    // SAFETY: a valid pseudo handle and a counters struct of the size given.
    unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) };
    let mib = |b: usize| b as f64 / 1_048_576.0;
    println!("{label}: working set {:.1} MB, peak {:.1} MB", mib(c.working_set_size), mib(c.peak_working_set_size));
}

#[cfg(not(windows))]
fn memory(_: &str) {}
