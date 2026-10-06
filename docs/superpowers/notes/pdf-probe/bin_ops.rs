//! pdfium-render 0.9.4 (features: pdfium_7881 only) against a downloaded pdfium.
//! Usage: ops <dir with pdfium.dll>
use std::path::Path;
use std::time::Instant;

use pdfium_render::prelude::*;
use pdfprobe::mem::mem;
use pdfprobe::pdfops::{self, Fmt, OpenError};

fn m(tag: &str) {
    let (ws, pws, pb, ppb) = mem();
    println!("[mem] {tag}: working set {ws:.1} MiB (peak {pws:.1}), private {pb:.1} MiB (peak {ppb:.1})");
}

fn main() {
    let dir = std::env::args().nth(1).expect("pdfium dir");
    m("start");
    let t = Instant::now();
    let pdfium = match pdfops::bind(Path::new(&dir)) {
        Ok(p) => p,
        Err(e) => {
            println!("bind failed: {e:?}");
            return;
        }
    };
    println!("bind + FPDF_InitLibrary: {:?}", t.elapsed());
    m("after load");

    // Merge.
    let t = Instant::now();
    let n = pdfops::merge(&pdfium, &[Path::new("out/three.pdf"), Path::new("out/extra.pdf"), Path::new("out/three_a4.pdf")], Path::new("out/merged.pdf")).unwrap();
    println!("merge 3+8+3 -> out/merged.pdf: {n} pages in {:?}", t.elapsed());

    pdfops::merge(&pdfium, &[Path::new("out/outline.pdf"), Path::new("out/three.pdf")], Path::new("out/merged_outline.pdf")).unwrap();

    // Extract "1-3, 5, 8-".
    let src = pdfium.load_pdf_from_file("out/merged.pdf", None).unwrap();
    let idx = pdfops::parse_ranges("1-3, 5, 8-", src.pages().len()).unwrap();
    println!("ranges '1-3, 5, 8-' of {} -> {:?}", src.pages().len(), idx);
    for bad in ["0-2", "5-3", "1-99", "x", ""] {
        println!("  parse_ranges({bad:?}) = {:?}", pdfops::parse_ranges(bad, src.pages().len()));
    }
    let out = pdfops::pages_to_new(&pdfium, &src, &idx).unwrap();
    out.save_to_file("out/extract.pdf").unwrap();
    println!("extract -> out/extract.pdf: {} pages", out.pages().len());
    drop(out);

    // Split: every page, and every 4 pages.
    let t = Instant::now();
    let total = src.pages().len();
    for i in 0..total {
        let d = pdfops::pages_to_new(&pdfium, &src, &[i]).unwrap();
        d.save_to_file(&format!("out/split/merged - page {}.pdf", i + 1)).unwrap();
    }
    for (k, start) in (0..total).step_by(4).enumerate() {
        let idx: Vec<i32> = (start..(start + 4).min(total)).collect();
        let d = pdfops::pages_to_new(&pdfium, &src, &idx).unwrap();
        d.save_to_file(&format!("out/split/merged - part {}.pdf", k + 1)).unwrap();
    }
    println!("split {total} single pages + {} parts of 4 in {:?}", (total + 3) / 4, t.elapsed());

    // Render.
    for dpi in [72.0, 150.0, 300.0] {
        for (fmt, name) in [(Fmt::Png, "png"), (Fmt::Jpeg(90), "jpg")] {
            let t = Instant::now();
            let stem = format!("out/render/merged-{dpi}-{name}");
            let r = pdfops::render_all(&src, dpi, 100_000_000, &fmt, Path::new(&stem)).unwrap();
            println!("render {} pages at {dpi} dpi to {name}: {:?}; page 1 {}x{}, A4 page 12 {}x{}", r.len(), t.elapsed(), r[0].0, r[0].1, r[11].0, r[11].1);
        }
    }
    m("after merge/split/render");
    drop(src);

    // Passwords.
    for (f, pw) in [
        ("out/enc_aes256.pdf", None),
        ("out/enc_aes256.pdf", Some("nope")),
        ("out/enc_aes256.pdf", Some("pw")),
        ("out/enc_aes256.pdf", Some("own")),
        ("out/enc_rc4.pdf", None),
        ("out/enc_rc4.pdf", Some("pw")),
        ("out/enc_owner_only.pdf", None),
        ("out/truncated.pdf", None),
        ("out/garbage.pdf", None),
        ("out/notpdf.pdf", None),
        ("out/missing.pdf", None),
    ] {
        let r = pdfops::open(&pdfium, Path::new(f), pw);
        match r {
            Ok(d) => {
                println!("open {f} pw={pw:?}: OK, {} pages, permissions-ok={:?}", d.pages().len(), d.permissions().security_handler_revision());
                if f == "out/enc_aes256.pdf" && pw == Some("pw") {
                    let o = pdfops::pages_to_new(&pdfium, &d, &[0, 1]).unwrap();
                    o.save_to_file("out/from_encrypted.pdf").unwrap();
                }
            }
            Err(e) => println!("open {f} pw={pw:?}: {e:?}"),
        }
    }
    let _ = OpenError::Damaged;

    // Edge cases: huge page (capped at 100 MP), 20000 pages, fonts.
    let d = pdfium.load_pdf_from_file("out/huge_page.pdf", None).unwrap();
    let p = d.pages().get(0).unwrap();
    println!("huge_page: {}x{} pt", p.width().value, p.height().value);
    let t = Instant::now();
    let r = pdfops::render_all(&d, 300.0, 100_000_000, &Fmt::Jpeg(85), Path::new("out/render/huge")).unwrap();
    println!("  render 300 dpi capped at 100 MP -> {}x{} at {:.1} dpi in {:?}", r[0].0, r[0].1, r[0].2, t.elapsed());
    m("after capped huge render");
    let uncapped = p.render_with_config(&PdfRenderConfig::new().scale_page_by_factor(300.0 / 72.0));
    println!("  uncapped 300 dpi (60000x60000 = 14.4 GB BGRA): {:?}", uncapped.as_ref().map(|b| (b.width(), b.height())).map_err(|e| format!("{e:?}")));
    drop(uncapped);
    drop(p);
    drop(d);
    let uu = pdfium.load_pdf_from_file("out/userunit.pdf", None).unwrap();
    let up = uu.pages().get(0).unwrap();
    println!("userunit.pdf (14400 pt x UserUnit 10): pdfium reports {}x{} pt", up.width().value, up.height().value);
    drop(up);
    drop(uu);

    let t = Instant::now();
    let many = pdfium.load_pdf_from_file("out/many_pages.pdf", None).unwrap();
    println!("many_pages: {} pages, open {:?}", many.pages().len(), t.elapsed());
    let t = Instant::now();
    let mut dst = pdfium.create_new_pdf().unwrap();
    dst.pages_mut().append(&many).unwrap();
    dst.save_to_file("out/many_copy.pdf").unwrap();
    println!("  append all + save: {:?}", t.elapsed());
    let t = Instant::now();
    for i in 0..200 {
        let d = pdfops::pages_to_new(&pdfium, &many, &[i]).unwrap();
        d.save_to_file(&format!("out/split/many - page {}.pdf", i + 1)).unwrap();
    }
    println!("  split first 200 single pages: {:?}", t.elapsed());
    m("after many pages");
    drop(dst);
    drop(many);

    let f = pdfium.load_pdf_from_file("out/fonts.pdf", None).unwrap();
    pdfops::render_all(&f, 150.0, 100_000_000, &Fmt::Png, Path::new("out/render/fonts")).unwrap();
    println!("fonts.pdf rendered to out/render/fonts - page 1.png");
    drop(f);

    // Unload / reload.
    drop(pdfium);
    m("after drop(Pdfium)");
    match Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(&dir)) {
        Ok(_) => println!("rebind after drop: OK"),
        Err(e) => println!("rebind after drop: {e:?}"),
    }
}
