//! The preview's PDF view worker (`gezik --pdf-view`) with real pdfium, in this process: the
//! password questions, the page count, a fitted page and a zoomed region no larger than asked.
//! Needs `GEZIK_TEST_PDFIUM` (a pdfium library, chromium/7881 or later); returns at once without.

use std::path::{Path, PathBuf};

use gezik_batch::pdf::view::serve;
use gezik_core::batch::pdf_view::{Command, Reply, read_hello, read_reply};

#[test]
fn an_encrypted_pdf_asks_then_shows_its_pages_where_asked() {
    let Some(library) = std::env::var_os("GEZIK_TEST_PDFIUM").map(PathBuf::from) else { return };
    let pdf = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/pdf/enc_aes256.pdf");
    let commands = [
        Command::Open { library, pdf },
        Command::Password("wrong".into()),
        Command::Password("pw".into()),
        Command::Render { page: 2, scale: 0.0, x: 0, y: 0, w: 300, h: 300 },
        Command::Render { page: 3, scale: 20.0, x: 100, y: 50, w: 200, h: 120 },
        Command::Render { page: 9, scale: 1.0, x: 0, y: 0, w: 10, h: 10 },
        Command::Close,
        Command::Render { page: 1, scale: 1.0, x: 0, y: 0, w: 10, h: 10 },
    ];
    let input: String = commands.iter().map(|c| c.to_line() + "\n").collect();
    let mut out = Vec::new();
    assert_eq!(serve(&mut input.as_bytes(), &mut out), 0);
    let mut out = &out[..];
    read_hello(&mut out).unwrap();
    assert_eq!(read_reply(&mut out).unwrap(), Reply::NeedsPassword);
    assert_eq!(read_reply(&mut out).unwrap(), Reply::WrongPassword);
    assert_eq!(read_reply(&mut out).unwrap(), Reply::Pages(3));
    let Reply::Image { page: 2, width, height, page_w, page_h, rgba } = read_reply(&mut out).unwrap() else {
        panic!("page 2 fitted")
    };
    assert!(width <= 300 && height <= 300 && (width == 300 || height == 300), "{width} × {height}");
    assert!(page_w > 0.0 && page_h > 0.0);
    assert_eq!(rgba.len(), (width * height * 4) as usize);
    let Reply::Image { page: 3, width, height, rgba, .. } = read_reply(&mut out).unwrap() else { panic!("a region") };
    assert_eq!((width, height), (200, 120), "only the region asked");
    assert!(rgba.chunks(4).any(|px| px[0] != 255 || px[1] != 255 || px[2] != 255), "something drawn");
    assert!(matches!(read_reply(&mut out).unwrap(), Reply::Failed(m) if m.contains("no page 9")));
    assert!(matches!(read_reply(&mut out).unwrap(), Reply::Failed(m) if m == "no document is open"));
    assert!(out.is_empty());
}
