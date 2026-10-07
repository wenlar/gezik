//! Times the filter's pure part on 100,000 entries, keystroke by keystroke, the way the view
//! runs it: the pattern compiled, the rows it lets through found in the full list and their
//! entries copied out, then the selection carried over by position (spread into a bitset of
//! the full list, then taken back for the new rows; skipped with nothing selected). Twice: with
//! nothing selected and after Ctrl+A (every entry shown is selected). For comparison it also
//! times the carry by name the view did before (a `HashSet` of the selected names). Prints the
//! worst and median keystroke.
//! `cargo run -p gezik-core --release --example filter_bench [-- entries]`

use std::collections::HashSet;
use std::time::Instant;

use gezik_core::Entry;
use gezik_core::pattern::{Pattern, matching_rows};
use gezik_core::selection::Selection;

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

/// What is shown after a keystroke: the entries, where they are in the full list, and the
/// selection.
struct Shown {
    entries: Vec<Entry>,
    rows: Option<Vec<usize>>,
    selection: Selection,
}

#[derive(Clone, Copy, PartialEq)]
enum Carry {
    ByPosition,
    ByName,
}

/// What the view does for one keystroke.
fn keystroke(full: &[Entry], shown: &Shown, text: &str, carry: Carry) -> Shown {
    let pattern = Pattern::compile(text).unwrap_or_default();
    let rows = matching_rows(full, &pattern);
    let entries: Vec<Entry> = rows.iter().map(|&i| full[i].clone()).collect();
    let selection = if shown.selection.count() == 0 {
        Selection::new(entries.len())
    } else if carry == Carry::ByPosition {
        let in_full = match &shown.rows {
            Some(old) => shown.selection.spread(old, full.len()),
            None => shown.selection.clone(),
        };
        in_full.carried(&rows)
    } else {
        // Before: `capture_all` copied the selected names, `indices_of` hashed them.
        let names: Vec<String> = shown.selection.iter().map(|i| shown.entries[i].name.clone()).collect();
        let wanted: HashSet<&str> = names.iter().map(String::as_str).collect();
        let indices = (0..entries.len()).filter(|&i| wanted.contains(entries[i].name.as_str()));
        Selection::from_indices(entries.len(), indices, None)
    };
    Shown { entries, rows: Some(rows), selection }
}

/// Types `typed` letter by letter from no filter; the time of each keystroke in ms.
fn run(full: &[Entry], typed: &str, select_all: bool, carry: Carry) -> Vec<f64> {
    let mut selection = Selection::new(full.len());
    if select_all {
        selection.select_all();
    }
    let mut shown = Shown { entries: full.to_vec(), rows: None, selection };
    let mut times = Vec::new();
    for end in typed.char_indices().map(|(i, c)| i + c.len_utf8()) {
        let start = Instant::now();
        let next = keystroke(full, &shown, &typed[..end], carry);
        times.push(start.elapsed().as_secs_f64() * 1000.0);
        shown = next;
    }
    times
}

fn report(label: &str, mut times: Vec<f64>) -> f64 {
    times.sort_by(f64::total_cmp);
    let worst = *times.last().unwrap();
    let median = times[times.len() / 2];
    println!("{label:<42} worst {worst:6.2} ms   median {median:6.2} ms   ({} keystrokes)", times.len());
    worst
}

fn main() {
    let count = std::env::args().nth(1).and_then(|n| n.parse().ok()).unwrap_or(100_000);
    let full = entries(count);
    println!("{count} entries");
    for (select_all, carry) in [(false, Carry::ByPosition), (true, Carry::ByPosition), (true, Carry::ByName)] {
        let mut worst: f64 = 0.0;
        let mut all = Vec::new();
        let case = match (select_all, carry) {
            (false, _) => "nothing selected",
            (true, Carry::ByPosition) => "after Ctrl+A",
            (true, Carry::ByName) => "after Ctrl+A, old carry by name",
        };
        for typed in ["img_01234", "*.pdf;!rapor 9*", "tatil"] {
            // A first run warms the allocator; the second is timed.
            run(&full, typed, select_all, carry);
            let times = run(&full, typed, select_all, carry);
            worst = worst.max(report(&format!("{typed:?} {case}"), times.clone()));
            all.extend(times);
        }
        let label = format!("all, {case}");
        report(&label, all);
        println!("{label}: {}", if worst <= 15.0 { "within the 15 ms budget" } else { "OVER the 15 ms budget" });
    }
}
