//! Times the view rules' pure part (spec 10 §8.3, §12) the way the view runs it: 20 rules (10
//! path, 5 kind, 5 content), none of which matches, against a folder of `entries` entries (10 %
//! folders, 10 % `report N.pdf`, the rest `IMG_N.jpg`). Prints the content count (once per
//! listing), one evaluation with the count made (median and worst of 10,000), and the heap the
//! 20 rules hold (as settings and compiled), each against its budget; exits 1 if one is over.
//! `cargo run -p gezik-core --release --example rules_bench [-- entries]`

use std::alloc::{GlobalAlloc, Layout, System};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use gezik_core::Entry;
use gezik_core::nav::Location;
use gezik_core::view::ViewMode;
use gezik_core::view_rules::{Content, Counts, Place, PlaceKind, RuleSpec, RuleView, ViewRules};

/// Counts the bytes on the heap, to weigh the rules.
struct Counting;

static LIVE: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        LIVE.fetch_add(layout.size(), Ordering::Relaxed);
        // SAFETY: the caller's layout goes to the system allocator as it came.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        // SAFETY: `ptr` came from `alloc` above with this layout.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static HEAP: Counting = Counting;

fn rule(number: usize, path: Option<String>, kind: Option<PlaceKind>, content: Option<&str>) -> RuleSpec {
    RuleSpec {
        number,
        path,
        kind,
        content: content.and_then(Content::parse),
        set: RuleView { mode: Some(ViewMode::Grid), ..RuleView::default() },
    }
}

fn median(mut times: Vec<Duration>) -> Duration {
    times.sort();
    times[times.len() / 2]
}

fn main() {
    let count: usize = std::env::args().nth(1).and_then(|n| n.parse().ok()).unwrap_or(100_000);
    let home = if cfg!(windows) { r"C:\Users\someone" } else { "/home/someone" };
    let before = LIVE.load(Ordering::Relaxed);
    let mut specs = Vec::new();
    for i in 0..10 {
        specs.push(rule(specs.len() + 1, Some(format!("{{home}}/Folder {i}/**")), None, None));
    }
    for kind in [PlaceKind::Drives, PlaceKind::Trash, PlaceKind::Search, PlaceKind::Network, PlaceKind::Cloud] {
        specs.push(rule(specs.len() + 1, None, Some(kind), None));
    }
    for class in ["videos", "audio", "archives", "code", "documents"] {
        specs.push(rule(specs.len() + 1, None, None, Some(&format!("{class} >= 50%"))));
    }
    let rules = ViewRules::compile(&specs, &|text| PathBuf::from(text.replace("{home}", home)));
    let held = LIVE.load(Ordering::Relaxed) - before;

    let entry =
        |name: String, is_dir: bool| Entry { name, is_dir, flags: 0, size: 1234, modified: None, created: None };
    let entries: Vec<Entry> = (0..count)
        .map(|i| match i % 10 {
            0 => entry(format!("Folder {i}"), true),
            1 => entry(format!("report {i}.pdf"), false),
            _ => entry(format!("IMG_{i:06}.jpg"), false),
        })
        .collect();
    let folder = PathBuf::from(home).join("Pictures").join("2024");
    let place = Place::of(&Location::Path(folder), std::iter::empty(), std::iter::empty());

    let mut counting = Vec::new();
    let mut counts = Counts::default();
    for _ in 0..20 {
        let started = Instant::now();
        counts = Counts::of(&entries);
        counting.push(started.elapsed());
    }
    let mut picks = Vec::new();
    for _ in 0..10_000 {
        let started = Instant::now();
        let picked = rules.pick_by(&place, &mut || Some(counts));
        picks.push(started.elapsed());
        assert_eq!(picked, None);
    }
    let worst = picks.iter().max().copied().unwrap_or_default();
    let (count_ms, pick) = (median(counting), median(picks));
    println!("{} rules, {count} entries", specs.len());
    println!("content count: median {count_ms:?} (budget 2 ms)");
    println!("evaluation:    median {pick:?}, worst {worst:?} (budget 50 µs)");
    println!("rules held:    {held} bytes (budget 20,480)");
    let over = count_ms > Duration::from_millis(2) || pick > Duration::from_micros(50) || held > 20 * 1024;
    if over {
        println!("OVER BUDGET");
        std::process::exit(1);
    }
}
