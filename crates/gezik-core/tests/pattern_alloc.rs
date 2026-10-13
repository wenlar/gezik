//! Matching a name allocates nothing (spec 3.2), counted by a global allocator in this test
//! binary only, on the thread that matches (the test harness's own threads allocate too).

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use gezik_core::pattern::Pattern;

struct Counting;

thread_local! {
    // Const-initialised and without a destructor: reading it never allocates.
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.with(|n| n.set(n.get() + 1));
        // SAFETY: forwarded unchanged to the system allocator.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` came from `System.alloc` with this layout.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

#[test]
fn matching_allocates_nothing() {
    let pattern = Pattern::compile("*.jpg; foto ; !*ş?.tmp; İstanbul").unwrap();
    let names: Vec<String> = (0..2000)
        .map(|i| match i % 4 {
            0 => format!("IMG_{i}.JPG"),
            1 => format!("Foto şehir {i}.png"),
            2 => format!("rapor ş{}.tmp", i % 10),
            _ => format!("ISTANBUL {i}.txt"),
        })
        .collect();
    let before = ALLOCATIONS.with(Cell::get);
    let hits = names.iter().filter(|name| pattern.matches(name)).count();
    let after = ALLOCATIONS.with(Cell::get);
    assert_eq!(after - before, 0, "matching {} names allocated", names.len());
    // `rapor ş5.tmp` passes no including part (and `!*ş?.tmp` leaves it out): 500 of 2000.
    assert_eq!(hits, 1500);
}
