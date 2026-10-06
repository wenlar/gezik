#[cfg(feature = "img2pdf")]
mod img2pdf;
#[cfg(feature = "img2pdf")]
mod jpeg;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let im = image::open(&args[1]).unwrap().to_rgba8();
    println!("{}", im.width());
    #[cfg(feature = "img2pdf")]
    {
        let o = img2pdf::Options { page: img2pdf::PageSize::A4, margin: 10.0, default_dpi: 72.0, level: 6, use_file_dpi: true, embed_icc: true, predictor: true, invert_adobe_cmyk: true };
        let mut p = img2pdf::PdfStream::create(std::path::Path::new(&args[2])).unwrap();
        p.add_image(std::path::Path::new(&args[1]), &o).unwrap();
        p.finish().unwrap();
    }
    #[cfg(feature = "pdfium")]
    {
        use pdfium_render::prelude::*;
        let p = Pdfium::new(Pdfium::bind_to_library(&args[3]).unwrap());
        let src = p.load_pdf_from_file(&args[2], args.get(4).map(|s| s.as_str())).unwrap();
        let mut dst = p.create_new_pdf().unwrap();
        dst.pages_mut().append(&src).unwrap();
        dst.pages_mut().copy_page_range_from_document(&src, 0..=0, 0).unwrap();
        dst.save_to_file("x.pdf").unwrap();
        for page in src.pages().iter() {
            let b = page.render_with_config(&PdfRenderConfig::new().scale_page_by_factor(2.0)).unwrap();
            println!("{} {} {}", b.width(), b.height(), b.as_rgba_bytes().len());
        }
    }
    #[cfg(feature = "raw")]
    unsafe {
        // The ~20 entry points Gezik needs, resolved by hand.
        use std::ffi::{c_char, c_int, c_void, c_ulong, c_float};
        let lib = libloading::Library::new(&args[3]).unwrap();
        let init: libloading::Symbol<unsafe extern "C" fn()> = lib.get(b"FPDF_InitLibrary").unwrap();
        let destroy: libloading::Symbol<unsafe extern "C" fn()> = lib.get(b"FPDF_DestroyLibrary").unwrap();
        let load: libloading::Symbol<unsafe extern "C" fn(*const c_char, *const c_char) -> *mut c_void> = lib.get(b"FPDF_LoadDocument").unwrap();
        let err: libloading::Symbol<unsafe extern "C" fn() -> c_ulong> = lib.get(b"FPDF_GetLastError").unwrap();
        let count: libloading::Symbol<unsafe extern "C" fn(*mut c_void) -> c_int> = lib.get(b"FPDF_GetPageCount").unwrap();
        let newdoc: libloading::Symbol<unsafe extern "C" fn() -> *mut c_void> = lib.get(b"FPDF_CreateNewDocument").unwrap();
        let import: libloading::Symbol<unsafe extern "C" fn(*mut c_void, *mut c_void, *const c_int, c_ulong, c_int) -> c_int> = lib.get(b"FPDF_ImportPagesByIndex").unwrap();
        let close: libloading::Symbol<unsafe extern "C" fn(*mut c_void)> = lib.get(b"FPDF_CloseDocument").unwrap();
        let loadpage: libloading::Symbol<unsafe extern "C" fn(*mut c_void, c_int) -> *mut c_void> = lib.get(b"FPDF_LoadPage").unwrap();
        let pw: libloading::Symbol<unsafe extern "C" fn(*mut c_void) -> c_float> = lib.get(b"FPDF_GetPageWidthF").unwrap();
        let ph: libloading::Symbol<unsafe extern "C" fn(*mut c_void) -> c_float> = lib.get(b"FPDF_GetPageHeightF").unwrap();
        let bmpc: libloading::Symbol<unsafe extern "C" fn(c_int, c_int, c_int, *mut c_void, c_int) -> *mut c_void> = lib.get(b"FPDFBitmap_CreateEx").unwrap();
        let fill: libloading::Symbol<unsafe extern "C" fn(*mut c_void, c_int, c_int, c_int, c_int, c_ulong) -> c_int> = lib.get(b"FPDFBitmap_FillRect").unwrap();
        let render: libloading::Symbol<unsafe extern "C" fn(*mut c_void, *mut c_void, c_int, c_int, c_int, c_int, c_int, c_int)> = lib.get(b"FPDF_RenderPageBitmap").unwrap();
        let buf: libloading::Symbol<unsafe extern "C" fn(*mut c_void) -> *mut u8> = lib.get(b"FPDFBitmap_GetBuffer").unwrap();
        let bdestroy: libloading::Symbol<unsafe extern "C" fn(*mut c_void)> = lib.get(b"FPDFBitmap_Destroy").unwrap();
        let closepage: libloading::Symbol<unsafe extern "C" fn(*mut c_void)> = lib.get(b"FPDF_ClosePage").unwrap();
        init();
        let name = std::ffi::CString::new(args[2].clone()).unwrap();
        let d = load(name.as_ptr(), std::ptr::null());
        if d.is_null() { println!("err {}", err()); return; }
        let n = count(d);
        let out = newdoc();
        let idx: Vec<c_int> = (0..n).collect();
        import(out, d, idx.as_ptr(), n as c_ulong, 0);
        for i in 0..n {
            let pg = loadpage(d, i);
            let (w, h) = ((pw(pg) * 2.0) as c_int, (ph(pg) * 2.0) as c_int);
            let b = bmpc(w, h, 2, std::ptr::null_mut(), 0); // FPDFBitmap_BGR
            fill(b, 0, 0, w, h, 0xFFFFFFFF);
            render(b, pg, 0, 0, w, h, 0, 0x10);
            println!("{}", *buf(b));
            bdestroy(b);
            closepage(pg);
        }
        close(out);
        close(d);
        destroy();
    }
}
