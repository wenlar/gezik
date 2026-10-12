//! Byte-offset reading shared by the paged text and the hex view (spec 13 §4.1, §4.4): a file
//! is opened for each read only (never held while it is shown), and every read is bounded.

#![deny(clippy::indexing_slicing)]

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// The most read at once: a text window, or a hex page.
pub const WINDOW: usize = 64 * 1024;

#[cfg(test)]
thread_local! {
    /// Reads made on this thread (tests: nothing is read until shown).
    pub static READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// At most `len` (≤ [`WINDOW`] unless the caller knows better) bytes of `path` at `offset`;
/// fewer at its end, none if it cannot be read.
pub fn read_at(path: &Path, offset: u64, len: usize) -> Vec<u8> {
    #[cfg(test)]
    READS.with(|r| r.set(r.get() + 1));
    // Reading a file only in the cloud would download it.
    if gezik_platform::only_in_cloud(path) {
        return Vec::new();
    }
    let Ok(mut file) = std::fs::File::open(path) else { return Vec::new() };
    if file.seek(SeekFrom::Start(offset)).is_err() {
        return Vec::new();
    }
    let mut bytes = Vec::with_capacity(len.min(WINDOW));
    let _ = file.take(len as u64).read_to_end(&mut bytes);
    bytes
}

/// The last read, so a redraw at the same place (wheel at the end, resize, keys that do not
/// move) does not open the file again.
#[derive(Default)]
pub struct Cache(std::cell::RefCell<Option<(u64, usize, Vec<u8>)>>);

impl Cache {
    pub fn read(&self, path: &Path, offset: u64, len: usize) -> Vec<u8> {
        if let Some((at, n, bytes)) = &*self.0.borrow()
            && (*at, *n) == (offset, len)
        {
            return bytes.clone();
        }
        let bytes = read_at(path, offset, len);
        *self.0.borrow_mut() = Some((offset, len, bytes.clone()));
        bytes
    }

    pub fn clear(&self) {
        *self.0.borrow_mut() = None;
    }
}

/// The file's length now; `None` if it is gone.
pub fn length(path: &Path) -> Option<u64> {
    std::fs::metadata(path).ok().map(|m| m.len())
}

/// How far `at` is from `start` to `end`, 0–1.
pub fn fraction(at: u64, start: u64, end: u64) -> f32 {
    if end <= start { 0.0 } else { (at.saturating_sub(start) as f64 / (end - start) as f64).clamp(0.0, 1.0) as f32 }
}

/// The place `fraction` of the way from `start` to `end`.
pub fn place(fraction: f32, start: u64, end: u64) -> u64 {
    let span = end.saturating_sub(start);
    start + ((f64::from(fraction.clamp(0.0, 1.0)) * span as f64) as u64).min(span)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_are_bounded_and_place_and_fraction_agree() {
        let dir = std::env::temp_dir().join(format!("gezik-scroller-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.bin");
        std::fs::write(&path, b"0123456789").unwrap();
        assert_eq!(read_at(&path, 3, 4), b"3456");
        assert_eq!(read_at(&path, 8, 100), b"89");
        assert!(read_at(&path, 50, 4).is_empty());
        assert!(read_at(&dir.join("missing"), 0, 4).is_empty());
        assert_eq!(place(0.5, 2, 12), 7);
        assert_eq!(place(2.0, 2, 12), 12);
        assert_eq!(fraction(7, 2, 12), 0.5);
        assert_eq!(fraction(7, 12, 12), 0.0);
        std::fs::remove_file(&path).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
