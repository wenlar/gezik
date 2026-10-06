//! Opens the img2pdf outputs with pdfium, renders each page at the image's pixel size and
//! compares it with the image crate's decode (+ EXIF orientation, alpha over white).
use std::path::Path;

use image::{DynamicImage, ImageDecoder, ImageReader};
use pdfium_render::prelude::*;

fn reference(path: &str) -> image::RgbImage {
    let mut dec = ImageReader::open(path).unwrap().with_guessed_format().unwrap().into_decoder().unwrap();
    let o = dec.orientation().unwrap();
    let mut im = DynamicImage::from_decoder(dec).unwrap();
    im.apply_orientation(o);
    let rgba = im.to_rgba8();
    image::RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
        let p = rgba.get_pixel(x, y).0;
        let a = p[3] as u32;
        image::Rgb([0, 1, 2].map(|i| ((p[i] as u32 * a + 255 * (255 - a) + 127) / 255) as u8))
    })
}

fn check(pdfium: &Pdfium, pdf: &str, refs: &[&str]) {
    let doc = pdfium.load_pdf_from_file(pdf, None).unwrap();
    assert_eq!(doc.pages().len() as usize, refs.len(), "{pdf}: page count");
    for (i, (page, r)) in doc.pages().iter().zip(refs).enumerate() {
        let want = reference(r);
        let k = want.width() as f32 / page.width().value;
        let bmp = page.render_with_config(&PdfRenderConfig::new().scale_page_by_factor(k)).unwrap();
        let got = bmp.as_rgba_bytes();
        let (w, h) = (bmp.width() as u32, bmp.height() as u32);
        let mut sum = 0u64;
        let mut n = 0u64;
        for y in 0..h.min(want.height()) {
            for x in 0..w.min(want.width()) {
                let g = &got[((y * w + x) * 4) as usize..][..3];
                let e = want.get_pixel(x, y).0;
                for c in 0..3 {
                    sum += (g[c] as i32 - e[c] as i32).unsigned_abs() as u64;
                    n += 1;
                }
            }
        }
        println!(
            "{pdf} p{}: page {:.1}x{:.1} pt, render {w}x{h}, ref {}x{} ({r}), mean |diff| = {:.2}",
            i + 1, page.width().value, page.height().value, want.width(), want.height(), sum as f64 / n as f64
        );
    }
}

fn main() {
    let dir = std::env::args().nth(1).expect("pdfium dir");
    let pdfium = pdfprobe::pdfops::bind(Path::new(&dir)).unwrap();
    check(&pdfium, "out/three.pdf", &["samples/DSCN0010.jpg", "samples/rgba.png", "samples/landscape_6.jpg"]);
    check(&pdfium, "out/extra.pdf", &[
        "samples/cmyk.jpg", "samples/gray.jpg", "samples/progressive.jpg", "samples/all.jpg",
        "samples/rgba16.png", "samples/rgb16.png", "samples/gray.png", "samples/rgba_icc.png",
    ]);
    check(&pdfium, "out/noicc.pdf", &["samples/landscape_6.jpg", "samples/all.jpg", "samples/rgba_icc.png"]);
    check(&pdfium, "out/cmyk_nodecode.pdf", &["samples/cmyk.jpg"]);
    for o in 1..=8 {
        check(&pdfium, &format!("out/orient_o{o}.pdf"), &[&format!("samples/orient/o{o}.jpg")]);
    }
    for f in ["out/three_a4.pdf", "out/three_letter.pdf", "out/bench100.pdf"] {
        let doc = pdfium.load_pdf_from_file(f, None).unwrap();
        let sizes: Vec<String> = doc.pages().iter().take(3).map(|p| format!("{:.2}x{:.2}", p.width().value, p.height().value)).collect();
        println!("{f}: {} pages, first sizes {sizes:?}", doc.pages().len());
    }
}
