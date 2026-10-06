//! Can pdfium be unloaded and loaded again in one process? pdfium-render cannot (its
//! bindings live in a process-wide OnceCell), so this goes through libloading directly.
use std::ffi::{CString, c_char, c_int, c_void};
use std::path::Path;

use libloading::{Library, Symbol};
use pdfprobe::mem::mem;

unsafe extern "system" {
    fn GetModuleHandleW(name: *const u16) -> *mut c_void;
}

fn loaded() -> bool {
    let n: Vec<u16> = "pdfium.dll\0".encode_utf16().collect();
    !unsafe { GetModuleHandleW(n.as_ptr()) }.is_null()
}

fn main() {
    let dir = std::env::args().nth(1).expect("pdfium dir");
    let path = Path::new(&dir).join("pdfium.dll");
    let file = CString::new("out/merged.pdf").unwrap();
    for round in 1..=3 {
        unsafe {
            let lib = Library::new(&path).unwrap();
            let init: Symbol<unsafe extern "C" fn()> = lib.get(b"FPDF_InitLibrary").unwrap();
            let destroy: Symbol<unsafe extern "C" fn()> = lib.get(b"FPDF_DestroyLibrary").unwrap();
            let load: Symbol<unsafe extern "C" fn(*const c_char, *const c_char) -> *mut c_void> = lib.get(b"FPDF_LoadDocument").unwrap();
            let count: Symbol<unsafe extern "C" fn(*mut c_void) -> c_int> = lib.get(b"FPDF_GetPageCount").unwrap();
            let close: Symbol<unsafe extern "C" fn(*mut c_void)> = lib.get(b"FPDF_CloseDocument").unwrap();
            init();
            let doc = load(file.as_ptr(), std::ptr::null());
            let n = count(doc);
            close(doc);
            destroy();
            let (ws, _, pb, _) = mem();
            println!("round {round}: pages={n} loaded={} ws={ws:.1} MiB private={pb:.1} MiB", loaded());
            drop(destroy);
            drop(init);
            drop(load);
            drop(count);
            drop(close);
            lib.close().unwrap();
        }
        let (ws, _, pb, _) = mem();
        println!("  after FreeLibrary: loaded={} ws={ws:.1} MiB private={pb:.1} MiB", loaded());
    }
}
