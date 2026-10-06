//! The real PDF worker, as `gezik --pdf-worker` runs it, without building the `gezik` exe:
//! `tests/pdf_worker.rs` runs this with pdfium. Not shipped.

fn main() {
    std::process::exit(gezik_batch::pdf::worker::main());
}
