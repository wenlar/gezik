//! Times the filter's pure part on 100,000 entries, keystroke by keystroke, the way the view
//! runs it: the pattern compiled, the entries it lets through copied out of the full list,
//! then the selection carried over by name (a `HashSet` of the selected names, as
//! `Listing::indices_of` does). Twice: with nothing selected (the view skips the names) and
//! after Ctrl+A (every name shown is selected). Prints the worst and median keystroke.
//! `cargo run -p gezik-core --release --example filter_bench [-- entries]`

use std::collections::HashSet;
use std::time::Instant;

use gezik_core::Entry;
use gezik_core::pattern::{Pattern, matching_entries};

/// `entries` entries: 10 % folders, 20 % `rapor N.pdf`, the rest `IMG_000123 Tatil ş3.jpg`;
/// folders first, as a listing has them.
fn entries(count: usize) -> Vec<Entry> {
    let entry = |name: String, is_dir: bool| Entry { name, is_dir, size: 1234, modified: None, created: None };
    let folders = count / 10;
    let reports = count / 5;
    let mut out = Vec::with_capacity(count);
    out.extend((0..folders).map(|i| entry(format!("Klasör {i}"), true)));
    out.extend((0..reports).map(|i| entry(format!("rapor {i}.pdf"), false)));
    out.extend((0..count - folders - reports).map(|i| entry(format!("IMG_{i:06} Tatil ş{}.jpg", i % 7), false)));
    out
}

/// What the view does for one keystroke: the new listing and the indices still selected.
fn keystroke(full: &[Entry], shown: &[Entry], selected: &[usize], text: &str) -> (Vec<Entry>, Vec<usize>) {
    let pattern = Pattern::compile(text).unwrap_or_default();
    let next = matching_entries(full, &pattern);
    // `capture_all`: the selected names; `selection_after_filter`: those shown again.
    let indices = if selected.is_empty() {
        Vec::new()
    } else {
        let names: Vec<String> = selected.iter().map(|&i| shown[i].name.clone()).collect();
        let wanted: HashSet<&str> = names.iter().map(String::as_str).collect();
        (0..next.len()).filter(|&i| wanted.contains(next[i].name.as_str())).collect()
    };
    (next, indices)
}

/// Types `typed` letter by letter from no filter; the time of each keystroke in ms.
fn run(full: &[Entry], typed: &str, select_all: bool) -> Vec<f64> {
    let mut shown = full.to_vec();
    let mut selected: Vec<usize> = if select_all { (0..shown.len()).collect() } else { Vec::new() };
    let mut times = Vec::new();
    for end in typed.char_indices().map(|(i, c)| i + c.len_utf8()) {
        let start = Instant::now();
        let (next, indices) = keystroke(full, &shown, &selected, &typed[..end]);
        times.push(start.elapsed().as_secs_f64() * 1000.0);
        shown = next;
        selected = indices;
    }
    times
}

fn report(label: &str, mut times: Vec<f64>) -> f64 {
    times.sort_by(f64::total_cmp);
    let worst = *times.last().unwrap();
    let median = times[times.len() / 2];
    println!("{label:<34} worst {worst:6.2} ms   median {median:6.2} ms   ({} keystrokes)", times.len());
    worst
}

fn main() {
    let count = std::env::args().nth(1).and_then(|n| n.parse().ok()).unwrap_or(100_000);
    let full = entries(count);
    println!("{count} entries");
    for select_all in [false, true] {
        let mut worst: f64 = 0.0;
        let mut all = Vec::new();
        for typed in ["img_01234", "*.pdf;!rapor 9*", "tatil"] {
            // A first run warms the allocator; the second is timed.
            run(&full, typed, select_all);
            let times = run(&full, typed, select_all);
            let label = format!("{typed:?}{}", if select_all { " after Ctrl+A" } else { "" });
            worst = worst.max(report(&label, times.clone()));
            all.extend(times);
        }
        let label = if select_all { "all, after Ctrl+A" } else { "all, nothing selected" };
        report(label, all);
        println!("{label}: {}", if worst <= 15.0 { "within the 15 ms budget" } else { "OVER the 15 ms budget" });
    }
}
