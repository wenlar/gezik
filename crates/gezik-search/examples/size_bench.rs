//! Adds up the folders right in a folder as a folder sizes run does (2 low-priority threads
//! each) and prints this process's working set before, at the end, after handing the freed
//! memory back and a moment later (spec 12, idle memory):
//! `cargo run --release -p gezik-search --example size_bench -- [folder]` (default: home).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gezik_search::size::{CACHE_MAX, SizeCache, measure};

fn main() {
    let folder = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .or_else(|| std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from))
        .expect("a folder");
    println!("before: {:.1} MiB", working_set_mib());
    let started = Instant::now();
    let mut cache = SizeCache::new(CACHE_MAX);
    let items = gezik_platform::fs::read_dir_items(&folder, &|_, _| false).expect("a readable folder");
    let mut under = 0;
    for item in items.iter().filter(|i| i.is_dir && !i.is_link && !i.offline) {
        let path = folder.join(&item.name);
        if let Some(total) = measure(&path, 2, &Arc::default()) {
            under += total.counts.map_or(0, |(_, folders)| folders);
            cache.put(&path, total, item.modified, Instant::now());
        }
    }
    println!(
        "{} items, {under} folders under them, in {:?}; cache {} records",
        items.len(),
        started.elapsed(),
        cache.len()
    );
    println!("end: {:.1} MiB (peak {:.1} MiB)", working_set_mib(), peak_mib());
    gezik_platform::priority::give_back_memory();
    println!("given back: {:.1} MiB", working_set_mib());
    std::thread::sleep(Duration::from_secs(2));
    println!("2 s later: {:.1} MiB", working_set_mib());
}

#[cfg(windows)]
fn counters() -> (usize, usize) {
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
    (c.working_set_size, c.peak_working_set_size)
}

/// Working set is a Windows measure (`scripts/perf` reads the others').
#[cfg(not(windows))]
fn counters() -> (usize, usize) {
    (0, 0)
}

fn working_set_mib() -> f64 {
    counters().0 as f64 / 1_048_576.0
}

fn peak_mib() -> f64 {
    counters().1 as f64 / 1_048_576.0
}
