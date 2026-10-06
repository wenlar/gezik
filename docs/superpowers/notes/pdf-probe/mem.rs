//! Peak working set / private bytes on Windows (probe only).
#[repr(C)]
#[derive(Default)]
struct Pmc {
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
    fn K32GetProcessMemoryInfo(p: isize, c: *mut Pmc, cb: u32) -> i32;
}
/// (working set, peak working set, private bytes, peak private bytes) in MiB.
pub fn mem() -> (f64, f64, f64, f64) {
    let mut c = Pmc { cb: std::mem::size_of::<Pmc>() as u32, ..Default::default() };
    unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) };
    let m = |v: usize| v as f64 / 1048576.0;
    (m(c.working_set_size), m(c.peak_working_set_size), m(c.pagefile_usage), m(c.peak_pagefile_usage))
}
