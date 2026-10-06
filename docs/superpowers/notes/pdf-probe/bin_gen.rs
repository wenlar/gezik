//! Edge-case inputs: a 200x200 inch page, a page with UserUnit 10 (2000 in), 20000 pages,
//! a page with text in a non-embedded font, and broken files.
use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref, Str};

fn one_page(path: &str, w: f32, h: f32, user_unit: Option<f32>) {
    let mut pdf = Pdf::new();
    pdf.catalog(Ref::new(1)).pages(Ref::new(2));
    pdf.pages(Ref::new(2)).kids([Ref::new(3)]).count(1);
    let mut page = pdf.page(Ref::new(3));
    page.media_box(Rect::new(0.0, 0.0, w, h)).parent(Ref::new(2)).contents(Ref::new(4));
    if let Some(u) = user_unit {
        page.user_unit(u);
    }
    page.finish();
    let mut c = Content::new();
    c.set_fill_rgb(0.2, 0.4, 0.8).rect(10.0, 10.0, w - 20.0, h - 20.0).fill_nonzero();
    pdf.stream(Ref::new(4), &c.finish());
    std::fs::write(path, pdf.finish()).unwrap();
}

fn main() {
    one_page("out/huge_page.pdf", 14400.0, 14400.0, None);
    one_page("out/userunit.pdf", 14400.0, 14400.0, Some(10.0));

    // 20000 pages sharing one content stream.
    let n = 20000;
    let mut pdf = Pdf::new();
    pdf.catalog(Ref::new(1)).pages(Ref::new(2));
    pdf.pages(Ref::new(2)).kids((0..n).map(|i| Ref::new(10 + i))).count(n);
    let mut c = Content::new();
    c.set_fill_rgb(0.8, 0.1, 0.1).rect(50.0, 50.0, 200.0, 100.0).fill_nonzero();
    pdf.stream(Ref::new(3), &c.finish());
    for i in 0..n {
        pdf.page(Ref::new(10 + i)).media_box(Rect::new(0.0, 0.0, 595.0, 842.0)).parent(Ref::new(2)).contents(Ref::new(3));
    }
    std::fs::write("out/many_pages.pdf", pdf.finish()).unwrap();

    // Text in standard-14 and non-embedded TrueType fonts (needs system fonts / substitution).
    let mut pdf = Pdf::new();
    pdf.catalog(Ref::new(1)).pages(Ref::new(2));
    pdf.pages(Ref::new(2)).kids([Ref::new(3)]).count(1);
    let mut page = pdf.page(Ref::new(3));
    page.media_box(Rect::new(0.0, 0.0, 400.0, 200.0)).parent(Ref::new(2)).contents(Ref::new(4));
    let mut res = page.resources();
    let mut fonts = res.fonts();
    fonts.pair(Name(b"F1"), Ref::new(5));
    fonts.pair(Name(b"F2"), Ref::new(6));
    fonts.finish();
    res.finish();
    page.finish();
    pdf.type1_font(Ref::new(5)).base_font(Name(b"Helvetica"));
    pdf.type1_font(Ref::new(6)).base_font(Name(b"Arial,Bold"));
    let mut c = Content::new();
    c.begin_text().set_font(Name(b"F1"), 24.0).next_line(20.0, 140.0).show(Str(b"Helvetica (base 14)")).end_text();
    c.begin_text().set_font(Name(b"F2"), 24.0).next_line(20.0, 60.0).show(Str(b"Arial,Bold not embedded")).end_text();
    pdf.stream(Ref::new(4), &c.finish());
    std::fs::write("out/fonts.pdf", pdf.finish()).unwrap();

    // Two pages with an outline (bookmarks) and document info, to see what merge keeps.
    let mut pdf = Pdf::new();
    let mut cat = pdf.catalog(Ref::new(1));
    cat.pages(Ref::new(2)).outlines(Ref::new(7));
    cat.finish();
    pdf.pages(Ref::new(2)).kids([Ref::new(3), Ref::new(4)]).count(2);
    for (pg, _) in [(3, 0), (4, 1)] {
        pdf.page(Ref::new(pg)).media_box(Rect::new(0.0, 0.0, 300.0, 300.0)).parent(Ref::new(2)).contents(Ref::new(5));
    }
    let mut c = Content::new();
    c.set_fill_rgb(0.1, 0.6, 0.1).rect(20.0, 20.0, 100.0, 100.0).fill_nonzero();
    pdf.stream(Ref::new(5), &c.finish());
    pdf.document_info(Ref::new(6)).title(pdf_writer::TextStr("Outline test"));
    pdf.outline(Ref::new(7)).first(Ref::new(8)).last(Ref::new(9)).count(2);
    pdf.outline_item(Ref::new(8)).title(pdf_writer::TextStr("Chapter 1")).parent(Ref::new(7)).next(Ref::new(9)).dest().page(Ref::new(3)).fit();
    pdf.outline_item(Ref::new(9)).title(pdf_writer::TextStr("Chapter 2")).parent(Ref::new(7)).prev(Ref::new(8)).dest().page(Ref::new(4)).fit();
    std::fs::write("out/outline.pdf", pdf.finish()).unwrap();

    // Broken inputs.
    let good = std::fs::read("out/three.pdf").unwrap();
    std::fs::write("out/truncated.pdf", &good[..good.len() / 2]).unwrap();
    std::fs::write("out/garbage.pdf", b"%PDF-1.7\nthis is not a pdf at all\n").unwrap();
    std::fs::write("out/notpdf.pdf", b"hello").unwrap();
    println!("gen ok");
}
