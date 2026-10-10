//! The real PDF worker (`examples/pdf_worker.rs`, the same code as `gezik --pdf-worker`)
//! through the client: merge, split, extract and render with pdfium, encrypted and damaged
//! files, the 64 MP render limit, Turkish and odd names, and pictures written by Gezik
//! rendering like the pictures themselves.
//!
//! The pdfium tests need `GEZIK_TEST_PDFIUM` to name a pdfium library (chromium/7881 or later);
//! without it they return at once. The others always run.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use gezik_batch::pdf::client::{Ended, WORKER_ARG, Worker, count_pages, run};
use gezik_batch::pdf::images::write_pdf;
use gezik_core::batch::pdf::{MAX_RENDER_PIXELS, PageImage, PageOptions, Split};
use gezik_core::batch::pdf_worker::{Reply, Request, WorkerJob};
use image::{DynamicImage, ImageDecoder, ImageReader, Rgb, RgbImage};

fn library() -> Option<PathBuf> {
    std::env::var_os("GEZIK_TEST_PDFIUM").map(PathBuf::from)
}

/// The folder all tests share, with the worker copied in first (see `tests/pdf_client.rs`:
/// a copy made while another thread starts a process can fail with "Text file busy" on Linux).
fn root() -> &'static Path {
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    ROOT.get_or_init(|| {
        let root = std::env::temp_dir().join(format!("gezik-pdf-worker-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let name = format!("pdf_worker{}", std::env::consts::EXE_SUFFIX);
        let deps = std::env::current_exe().unwrap().parent().unwrap().to_path_buf();
        let built = deps.parent().unwrap().join("examples").join(&name);
        assert!(
            built.is_file(),
            "{} is missing: run `cargo test -p gezik-batch` (it builds the examples; `--test pdf_worker` alone does not)",
            built.display()
        );
        std::fs::copy(&built, root.join(&name)).unwrap();
        root
    })
}

fn real_worker() -> Worker {
    let program = root().join(format!("pdf_worker{}", std::env::consts::EXE_SUFFIX));
    Worker { program, args: vec![WORKER_ARG.into()] }
}

fn dir(name: &str) -> PathBuf {
    let d = root().join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A new empty folder `name` in `d`.
fn fresh(d: &Path, name: &str) -> PathBuf {
    let out = d.join(name);
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).unwrap();
    out
}

fn data(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join(name)
}

fn never() -> bool {
    false
}

fn req(lib: &Path, out: &Path, job: WorkerJob, inputs: &[&Path]) -> Request {
    Request {
        library: lib.to_path_buf(),
        dir: out.to_path_buf(),
        job,
        inputs: inputs.iter().map(|p| p.to_path_buf()).collect(),
        passwords: Vec::new(),
    }
}

fn count(lib: &Path, pdf: &Path) -> u32 {
    count_pages(&real_worker(), lib, pdf, &never).unwrap().expect("not encrypted")
}

/// A PDF named `name` in `d` (in place of one there), written by Gezik (`write_pdf`): `pages`
/// pictures of 64 × 48 px, with no density, so each page is 64 × 48 pt.
fn pdf_of(d: &Path, name: impl AsRef<std::ffi::OsStr>, pages: u8) -> PathBuf {
    let pics: Vec<PathBuf> = (0..pages)
        .map(|i| {
            let png = d.join(format!("pic{i}.png"));
            RgbImage::from_fn(64, 48, |x, y| Rgb([x as u8 * 4, y as u8 * 5, i * 60])).save(&png).unwrap();
            png
        })
        .collect();
    let out = d.join(name.as_ref());
    // `write_pdf` never writes over a file: an earlier one of this name goes first.
    let _ = std::fs::remove_file(&out);
    write_pdf(&pics, &out, &PageOptions::DEFAULT, &mut |_, _| {}, &never).unwrap();
    out
}

fn three_page_pdf(d: &Path) -> PathBuf {
    pdf_of(d, "a.pdf", 3)
}

/// One empty 14400 × 14400 pt page (Acrobat's largest).
fn huge_page_pdf(d: &Path) -> PathBuf {
    use pdf_writer::{Pdf, Rect, Ref};
    let (catalog, tree, page, content) = (Ref::new(1), Ref::new(2), Ref::new(3), Ref::new(4));
    let mut pdf = Pdf::new();
    pdf.catalog(catalog).pages(tree);
    pdf.pages(tree).kids([page]).count(1);
    pdf.page(page).parent(tree).media_box(Rect::new(0.0, 0.0, 14_400.0, 14_400.0)).contents(content);
    pdf.stream(content, b"");
    let path = d.join("huge.pdf");
    std::fs::write(&path, pdf.finish()).unwrap();
    path
}

// ---- Pictures (as in tests/pdf_images.rs) -------------------------------------------------

/// A little-endian TIFF holding only the orientation tag.
fn orientation_exif(orientation: u16) -> Vec<u8> {
    let mut t = b"II*\0".to_vec();
    t.extend_from_slice(&8u32.to_le_bytes());
    t.extend_from_slice(&1u16.to_le_bytes());
    t.extend_from_slice(&0x0112u16.to_le_bytes());
    t.extend_from_slice(&3u16.to_le_bytes());
    t.extend_from_slice(&1u32.to_le_bytes());
    t.extend_from_slice(&orientation.to_le_bytes());
    t.extend_from_slice(&[0, 0]);
    t.extend_from_slice(&0u32.to_le_bytes());
    t
}

/// A 64 × 48 JPEG whose corners all differ (so a wrong turn shows), with EXIF orientation `o`.
fn jpeg_with_exif(path: &Path, o: u16) -> PathBuf {
    let (w, h) = (64u16, 48u16);
    let mut rgb = Vec::new();
    for y in 0..h {
        for x in 0..w {
            rgb.extend_from_slice(&[(x * 4) as u8, (y * 5) as u8, if x < 32 { 40 } else { 200 }]);
        }
    }
    let mut out = Vec::new();
    let mut encoder = jpeg_encoder::Encoder::new(&mut out, 95);
    encoder.add_exif_metadata(&orientation_exif(o)).unwrap();
    encoder.encode(&rgb, w, h, jpeg_encoder::ColorType::Rgb).unwrap();
    std::fs::write(path, out).unwrap();
    path.to_path_buf()
}

fn cmyk_jpeg(path: &Path) -> PathBuf {
    let (w, h) = (16u16, 16u16);
    let cmyk: Vec<u8> = (0..u32::from(w) * u32::from(h)).flat_map(|i| [(i % 256) as u8, 40, 80, 10]).collect();
    let mut out = Vec::new();
    jpeg_encoder::Encoder::new(&mut out, 90).encode(&cmyk, w, h, jpeg_encoder::ColorType::Cmyk).unwrap();
    std::fs::write(path, out).unwrap();
    path.to_path_buf()
}

/// The picture as `image` shows it: decoded and turned by its EXIF orientation.
fn upright(path: &Path) -> RgbImage {
    let mut decoder = ImageReader::open(path).unwrap().with_guessed_format().unwrap().into_decoder().unwrap();
    let orientation = decoder.orientation().unwrap();
    let mut img = DynamicImage::from_decoder(decoder).unwrap();
    img.apply_orientation(orientation);
    img.to_rgb8()
}

fn mean_abs_diff(a: &RgbImage, b: &RgbImage) -> f64 {
    assert_eq!(a.dimensions(), b.dimensions());
    let sum: u64 = a.as_raw().iter().zip(b.as_raw()).map(|(x, y)| u64::from(x.abs_diff(*y))).sum();
    sum as f64 / a.as_raw().len() as f64
}

// ---- Tests --------------------------------------------------------------------------------

#[test]
fn a_missing_library_is_a_library_failure() {
    let d = dir("nolib");
    let pdf = three_page_pdf(&d);
    let req = Request {
        library: d.join("nope.dll"),
        dir: d.clone(),
        job: WorkerJob::Count,
        inputs: vec![pdf],
        passwords: vec![],
    };
    let err = run(&real_worker(), &req, &mut |_| {}, &never).unwrap_err();
    assert!(err.to_string().starts_with("pdfium could not be loaded"), "{err}");
}

#[test]
fn a_bad_request_is_refused_before_pdfium_loads() {
    let d = dir("badreq");
    let pdf = three_page_pdf(&d);
    for dpi in [0, 100_000] {
        let r = req(&d.join("nope.dll"), &d, WorkerJob::Render { dpi, image: PageImage::Png }, &[&pdf]);
        let err = run(&real_worker(), &r, &mut |_| {}, &never).unwrap_err();
        assert!(err.to_string().starts_with(&format!("bad request: dpi {dpi}")), "{err}");
    }
    // The worker itself: a request that does not parse is exit code 2.
    let mut out = Vec::new();
    assert_eq!(gezik_batch::pdf::worker::serve("hello", &mut out), 2);
    assert_eq!(String::from_utf8(out).unwrap(), "failed\t-\tother\tbad request: not a gezik-pdf 1 request\n");
}

/// A password pdfium-render cannot take (it holds a NUL) is a wrong password: never a crash
/// whose error output, password and all, would reach "PDF engine stopped (…)".
#[test]
fn a_password_with_a_nul_never_reaches_an_error() {
    let d = dir("nulpw");
    let pdf = three_page_pdf(&d);
    // Before pdfium loads, so no library is needed.
    let mut r = req(&d.join("nope.dll"), &d, WorkerJob::Count, &[&pdf]);
    r.passwords = vec![(0, "geheim\0LEAK".into())];
    assert_eq!(run(&real_worker(), &r, &mut |_| {}, &never).unwrap(), Ended::WrongPassword(0));
    // With pdfium and a really encrypted file: the same, and no failure text at all.
    let Some(lib) = library() else { return };
    let mut r = req(&lib, &d, WorkerJob::Split(Split::EachPage), &[&data("pdf/enc_aes256.pdf")]);
    r.passwords = vec![(0, "pw\0LEAK".into())];
    match run(&real_worker(), &r, &mut |_| {}, &never) {
        Ok(ended) => assert_eq!(ended, Ended::WrongPassword(0)),
        Err(e) => panic!("failed (password shown: {}): {e}", e.to_string().contains("LEAK")),
    }
}

/// Two parts with the same label ("1-2, 1-2") get "(2)", as placing does (`next_free`).
#[test]
fn parts_with_the_same_name_are_numbered() {
    let Some(lib) = library() else { return };
    let d = dir("dupes");
    let a = three_page_pdf(&d);
    let out = fresh(&d, "out");
    let split = WorkerJob::Split(Split::Ranges("1-2, 1-2, 3".into()));
    run(&real_worker(), &req(&lib, &out, split, &[&a]), &mut |_| {}, &never).unwrap();
    assert_eq!(count(&lib, &out.join("a - pages 1-2.pdf")), 2);
    assert_eq!(count(&lib, &out.join("a - pages 1-2 (2).pdf")), 2);
    assert_eq!(count(&lib, &out.join("a - page 3.pdf")), 1);
    assert_eq!(std::fs::read_dir(&out).unwrap().count(), 3);
}

/// A page whose box is empty: pdfium gives it a Letter size (612 × 792 pt), so it renders;
/// it never ends in a "0 × 0 picture" error.
#[test]
fn a_page_with_an_empty_box_renders_or_says_it_has_no_size() {
    let Some(lib) = library() else { return };
    let d = dir("nosize");
    let pdf = {
        use pdf_writer::{Pdf, Rect, Ref};
        let mut p = Pdf::new();
        p.catalog(Ref::new(1)).pages(Ref::new(2));
        p.pages(Ref::new(2)).kids([Ref::new(3)]).count(1);
        p.page(Ref::new(3)).parent(Ref::new(2)).media_box(Rect::new(0.0, 0.0, 0.0, 0.0));
        let path = d.join("flat.pdf");
        std::fs::write(&path, p.finish()).unwrap();
        path
    };
    let out = fresh(&d, "out");
    let render = WorkerJob::Render { dpi: 72, image: PageImage::Png };
    match run(&real_worker(), &req(&lib, &out, render, &[&pdf]), &mut |_| {}, &never) {
        Ok(_) => assert!(image::open(out.join("flat - page 1.png")).unwrap().width() > 0),
        Err(e) => assert_eq!(e.to_string(), "page 1 has no size"),
    }
}

#[test]
fn merge_split_extract_and_render_with_pdfium() {
    let Some(lib) = library() else { return };
    let d = dir("ops");
    let a = three_page_pdf(&d);
    let b = pdf_of(&d, "rapor ş.pdf", 2);
    let out = fresh(&d, "merge");
    let mut replies = Vec::new();
    run(&real_worker(), &req(&lib, &out, WorkerJob::Merge, &[&a, &b]), &mut |r| replies.push(r.clone()), &never)
        .unwrap();
    assert_eq!(replies.first(), Some(&Reply::Steps(5)));
    // One line per document, not one per page.
    assert_eq!(
        replies.iter().filter(|r| matches!(r, Reply::Stepped(_))).collect::<Vec<_>>(),
        [&Reply::Stepped(3), &Reply::Stepped(2)]
    );
    assert_eq!(replies.last(), Some(&Reply::Done));
    assert_eq!(count(&lib, &out.join("a (merged).pdf")), 5);
    let out = fresh(&d, "split");
    run(&real_worker(), &req(&lib, &out, WorkerJob::Split(Split::Ranges("1-2, 3".into())), &[&a]), &mut |_| {}, &never)
        .unwrap();
    assert_eq!(count(&lib, &out.join("a - pages 1-2.pdf")), 2);
    assert_eq!(count(&lib, &out.join("a - page 3.pdf")), 1);
    let out = fresh(&d, "extract");
    run(&real_worker(), &req(&lib, &out, WorkerJob::Extract("2-".into()), &[&b]), &mut |_| {}, &never).unwrap();
    assert_eq!(count(&lib, &out.join("rapor ş - page 2.pdf")), 1);
    let out = fresh(&d, "render");
    run(
        &real_worker(),
        &req(&lib, &out, WorkerJob::Render { dpi: 72, image: PageImage::Png }, &[&a]),
        &mut |_| {},
        &never,
    )
    .unwrap();
    let png = image::open(out.join("a - page 1.png")).unwrap();
    assert_eq!((png.width(), png.height()), (64, 48)); // the 64 × 48 pt page at 72 dpi
    // The page is the picture: pixel (40, 30) of page 3 is (160, 150, 120).
    let page3 = image::open(out.join("a - page 3.png")).unwrap().to_rgb8();
    let px = page3.get_pixel(40, 30).0;
    assert!(px.iter().zip([160u8, 150, 120]).all(|(a, b)| a.abs_diff(b) <= 3), "{px:?}");
    // Ranges past the end are the worker's to refuse.
    let out = fresh(&d, "past");
    let err = run(&real_worker(), &req(&lib, &out, WorkerJob::Extract("2-9".into()), &[&a]), &mut |_| {}, &never)
        .unwrap_err();
    assert_eq!(err.to_string(), "Page 9 is past the end (3 pages)");
}

#[test]
fn encrypted_pdfs_ask_and_give_an_unencrypted_copy() {
    let Some(lib) = library() else { return };
    let enc = data("pdf/enc_aes256.pdf");
    assert_eq!(count_pages(&real_worker(), &lib, &enc, &never).unwrap(), None);
    let d = dir("enc");
    let mut r = req(&lib, &d, WorkerJob::Split(Split::EachPage), &[&enc]);
    r.passwords = vec![(0, "nope".into())];
    assert_eq!(run(&real_worker(), &r, &mut |_| {}, &never).unwrap(), Ended::WrongPassword(0));
    r.passwords = vec![(0, "pw".into())];
    assert_eq!(run(&real_worker(), &r, &mut |_| {}, &never).unwrap(), Ended::Done);
    // The part opens without any password (özet §2.5).
    assert_eq!(count_pages(&real_worker(), &lib, &d.join("enc_aes256 - page 1.pdf"), &never).unwrap(), Some(1));
}

#[test]
fn a_huge_page_is_rendered_at_a_lower_dpi() {
    let Some(lib) = library() else { return };
    let d = dir("huge");
    let huge = huge_page_pdf(&d);
    let out = fresh(&d, "out");
    let mut lowered = None;
    run(
        &real_worker(),
        &req(&lib, &out, WorkerJob::Render { dpi: 300, image: PageImage::Jpeg }, &[&huge]),
        &mut |r| {
            if let Reply::Lowered { page, dpi } = r {
                lowered = Some((*page, *dpi))
            }
        },
        &never,
    )
    .unwrap();
    assert_eq!(lowered.map(|l| l.0), Some(1));
    assert_eq!(lowered.map(|l| l.1), Some(40), "64 MP of a 14400 pt square is about 40 dpi");
    let jpg = image::open(out.join("huge - page 1.jpg")).unwrap();
    assert!(u64::from(jpg.width()) * u64::from(jpg.height()) <= MAX_RENDER_PIXELS);
}

#[test]
fn damaged_files_are_named_damaged() {
    let Some(lib) = library() else { return };
    let d = dir("damaged");
    let bytes = std::fs::read(three_page_pdf(&d)).unwrap();
    let cut = d.join("cut.pdf");
    std::fs::write(&cut, &bytes[..bytes.len() / 3]).unwrap();
    let err = count_pages(&real_worker(), &lib, &cut, &never).unwrap_err();
    assert!(err.to_string().starts_with("the PDF is damaged"), "{err}");
}

/// Review focus 4: names with Turkish letters, quotes and shell characters (and, where the
/// system allows, a name that is not Unicode) open, and the outputs are named after them.
#[test]
fn odd_names_open_and_name_their_outputs() {
    let Some(lib) = library() else { return };
    let d = dir("names");
    // Windows forbids `"` in names; everything else is the same.
    let shown = if cfg!(windows) { "rapor ş 'a'; $ & ğüİ 😀" } else { "rapor ş \"a\"; $ & ğüİ 😀" };
    let mut stems: Vec<OsString> = vec![shown.into()];
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        stems.push(OsString::from_wide(&[0x72, 0xD800, 0x78])); // "r", a lone surrogate, "x"
    }
    // APFS and HFS+ (macOS) refuse names that are not UTF-8.
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        use std::os::unix::ffi::OsStringExt;
        stems.push(OsString::from_vec(vec![b'r', 0xFF, b'x'])); // not UTF-8
    }
    for stem in stems {
        let mut name = stem.clone();
        name.push(".pdf");
        let pdf = pdf_of(&d, &name, 2);
        assert_eq!(count(&lib, &pdf), 2);
        let out = fresh(&d, "out");
        run(&real_worker(), &req(&lib, &out, WorkerJob::Split(Split::EachPage), &[&pdf]), &mut |_| {}, &never).unwrap();
        let mut part = stem.clone();
        part.push(" - page 2.pdf");
        assert_eq!(count(&lib, &out.join(&part)), 1, "{part:?}");
        let out = fresh(&d, "img");
        let render = WorkerJob::Render { dpi: 72, image: PageImage::Jpeg };
        run(&real_worker(), &req(&lib, &out, render, &[&pdf]), &mut |_| {}, &never).unwrap();
        let mut jpg = stem.clone();
        jpg.push(" - page 1.jpg");
        assert_eq!(image::open(out.join(&jpg)).unwrap().width(), 64, "{jpg:?}");
        let out = fresh(&d, "merge");
        let other = pdf_of(&d, "b.pdf", 1);
        run(&real_worker(), &req(&lib, &out, WorkerJob::Merge, &[&pdf, &other]), &mut |_| {}, &never).unwrap();
        let mut merged = stem.clone();
        merged.push(" (merged).pdf");
        assert_eq!(count(&lib, &out.join(&merged)), 3, "{merged:?}");
    }
}

#[test]
fn pictures_written_by_gezik_render_like_the_pictures() {
    let Some(lib) = library() else { return };
    let d = dir("pictures");
    // Each EXIF orientation: the page shows the picture upright (özet §1.3: 0.16 on photos).
    for o in 1..=8u16 {
        let jpg = jpeg_with_exif(&d.join(format!("o{o}.jpg")), o);
        let pdf = d.join(format!("o{o}.pdf"));
        write_pdf(std::slice::from_ref(&jpg), &pdf, &PageOptions::DEFAULT, &mut |_, _| {}, &never).unwrap();
        let out = fresh(&d, &format!("o{o}"));
        let render = WorkerJob::Render { dpi: 72, image: PageImage::Png };
        run(&real_worker(), &req(&lib, &out, render, &[&pdf]), &mut |_| {}, &never).unwrap();
        let page = image::open(out.join(format!("o{o} - page 1.png"))).unwrap().to_rgb8();
        let want = upright(&jpg);
        let diff = mean_abs_diff(&page, &want);
        assert!(diff < 2.0, "orientation {o}: mean |diff| {diff}");
    }
    // A Photoshop-style CMYK JPEG keeps its colours (özet §1.6: 15.93 on the probe's sample).
    let cmyk = cmyk_jpeg(&d.join("cmyk.jpg"));
    let pdf = d.join("cmyk.pdf");
    write_pdf(std::slice::from_ref(&cmyk), &pdf, &PageOptions::DEFAULT, &mut |_, _| {}, &never).unwrap();
    let out = fresh(&d, "cmyk");
    let render = WorkerJob::Render { dpi: 72, image: PageImage::Png };
    run(&real_worker(), &req(&lib, &out, render, &[&pdf]), &mut |_| {}, &never).unwrap();
    let page = image::open(out.join("cmyk - page 1.png")).unwrap().to_rgb8();
    let diff = mean_abs_diff(&page, &upright(&cmyk));
    assert!(diff < 20.0, "cmyk: mean |diff| {diff}");
}
