//! Opens one (possibly corrupt) PDF and renders every page at 72 dpi. Exit 0 = handled.
use std::path::Path;

use pdfium_render::prelude::*;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let pdfium = pdfprobe::pdfops::bind(Path::new(&a[1])).unwrap();
    let Ok(doc) = pdfium.load_pdf_from_file(&a[2], None) else {
        println!("open error");
        return;
    };
    let mut ok = 0;
    for page in doc.pages().iter() {
        let k = (1_000_000.0 / (page.width().value * page.height().value).max(1.0)).sqrt().min(1.0);
        if page.render_with_config(&PdfRenderConfig::new().scale_page_by_factor(k)).is_ok() {
            ok += 1;
        }
    }
    println!("rendered {ok}/{}", doc.pages().len());
}
