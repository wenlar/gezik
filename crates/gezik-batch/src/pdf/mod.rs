//! The PDF group. Writing pictures into a PDF happens here, in pure Rust (pdf-writer); a JPEG
//! goes in as it is, without being decoded. Everything else needs pdfium, which runs in a
//! separate worker process ([`client`] starts it, [`worker`] is it).

pub mod client;
pub mod images;
pub mod jpeg;
pub mod pdfium;
pub mod worker;
