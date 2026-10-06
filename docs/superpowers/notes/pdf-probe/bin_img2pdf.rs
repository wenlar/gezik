//! Writes the probe PDFs into out/ and times 100 x 12 MP JPEGs.
use std::path::Path;
use std::time::Instant;

use pdfprobe::img2pdf::{Options, PageSize, PdfStream};

fn make(out: &str, files: &[&str], opt: &Options) {
    let mut pdf = PdfStream::create(Path::new(out)).unwrap();
    for f in files {
        let how = pdf.add_image(Path::new(f), opt).unwrap();
        println!("  {f:22} {how}");
    }
    let n = pdf.finish().unwrap();
    println!("{out}: {n} B");
}

fn main() {
    std::fs::create_dir_all("out").unwrap();
    let img = Options { page: PageSize::Image, margin: 0.0, default_dpi: 72.0, level: 6, use_file_dpi: false, embed_icc: true, predictor: true, invert_adobe_cmyk: true };
    make("out/three.pdf", &["samples/DSCN0010.jpg", "samples/rgba.png", "samples/landscape_6.jpg"], &img);
    make(
        "out/extra.pdf",
        &[
            "samples/cmyk.jpg", "samples/gray.jpg", "samples/progressive.jpg", "samples/all.jpg",
            "samples/rgba16.png", "samples/rgb16.png", "samples/gray.png", "samples/rgba_icc.png",
        ],
        &img,
    );
    make("out/noicc.pdf", &["samples/landscape_6.jpg", "samples/all.jpg", "samples/rgba_icc.png"], &Options { embed_icc: false, ..img });
    make("out/filedpi.pdf", &["samples/cmyk.jpg"], &Options { use_file_dpi: true, ..img });
    make("out/cmyk_nodecode.pdf", &["samples/cmyk.jpg"], &Options { invert_adobe_cmyk: false, ..img });
    for (o, f) in [(1u8, "o1"), (2, "o2"), (3, "o3"), (4, "o4"), (5, "o5"), (6, "o6"), (7, "o7"), (8, "o8")] {
        let _ = o;
        make(&format!("out/orient_{f}.pdf"), &[&format!("samples/orient/{f}.jpg")], &img);
    }
    let a4 = Options { page: PageSize::A4, margin: 28.35, ..img }; // 10 mm
    make("out/three_a4.pdf", &["samples/DSCN0010.jpg", "samples/rgba.png", "samples/landscape_6.jpg"], &a4);
    let letter = Options { page: PageSize::Letter, margin: 36.0, ..img };
    make("out/three_letter.pdf", &["samples/DSCN0010.jpg", "samples/rgba.png", "samples/landscape_6.jpg"], &letter);

    // 100 x 12 MP JPEG (4000x3000, 3.8 MB each).
    let t = Instant::now();
    let mut pdf = PdfStream::create(Path::new("out/bench100.pdf")).unwrap();
    for _ in 0..100 {
        pdf.add_image(Path::new("samples/big12mp.jpg"), &a4).unwrap();
    }
    let n = pdf.finish().unwrap();
    let (ws, pws, pb, ppb) = pdfprobe::mem::mem();
    println!("bench100.pdf: {n} B in {:?}; working set {ws:.1} MiB (peak {pws:.1}), private {pb:.1} MiB (peak {ppb:.1})", t.elapsed());

    // 12 MP PNG (decode + Flate) for comparison.
    let t = Instant::now();
    let mut pdf = PdfStream::create(Path::new("out/bench_png.pdf")).unwrap();
    for _ in 0..5 {
        pdf.add_image(Path::new("samples/big12mp.png"), &a4).unwrap();
    }
    let n = pdf.finish().unwrap();
    println!("bench_png.pdf (5 x 12 MP PNG, Paeth+Flate 6): {n} B in {:?}", t.elapsed());
    for (pred, lvl) in [(false, 6u8), (true, 1), (true, 9)] {
        let o = Options { predictor: pred, level: lvl, ..a4 };
        let t = Instant::now();
        let mut pdf = PdfStream::create(Path::new("out/bench_png1.pdf")).unwrap();
        pdf.add_image(Path::new("samples/big12mp.png"), &o).unwrap();
        pdf.add_image(Path::new("samples/screen.png"), &o).unwrap();
        let n = pdf.finish().unwrap();
        println!("  12 MP PNG + screenshot, predictor={pred} level={lvl}: {n} B in {:?}", t.elapsed());
    }
}
