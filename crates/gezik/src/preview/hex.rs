//! The hex view (spec 13 §4.4): any file of any size, paged by byte offset; only the rows
//! shown are read.

#![deny(clippy::indexing_slicing)]

use std::fmt::Write;
use std::path::{Path, PathBuf};

use gezik_core::syntax::Class;

use super::scroller;
use super::text::Row;

/// The most read for one page.
const MAX_PAGE: u64 = 4096;

pub struct HexView {
    path: PathBuf,
    size: u64,
    /// The offset of the top row.
    top: u64,
    cache: scroller::Cache,
}

impl HexView {
    pub fn new(path: &Path, size: u64) -> HexView {
        HexView { path: path.to_path_buf(), size, top: 0, cache: scroller::Cache::default() }
    }

    /// Bytes per row in `cols` columns (the offsets' column is not in them): 16, 8 or 4, the
    /// most whose row fits (a narrower place scrolls sideways).
    pub fn per_row(cols: usize) -> u64 {
        [16, 8].into_iter().find(|&per| cols >= row_width(per) + per).unwrap_or(4) as u64
    }

    /// Opens at `offset` (where the text was).
    pub fn starting_at(mut self, offset: u64) -> HexView {
        self.top = offset.min(self.size);
        self
    }

    /// Hex digits of the offsets: enough for the size, at least 8. Never by the row's width:
    /// the offsets' column decides the room the rows get.
    pub fn gutter(&self) -> usize {
        (64 - self.size.max(1).leading_zeros() as usize).div_ceil(4).max(8)
    }

    pub fn rows(&self, count: usize, cols: usize) -> Vec<Row> {
        let per = Self::per_row(cols);
        let top = self.top - self.top % per;
        let len = (count as u64 * per).min(MAX_PAGE);
        let bytes = self.cache.read(&self.path, top, usize::try_from(len).unwrap_or(0));
        let digits = self.gutter();
        bytes
            .chunks(per as usize)
            .enumerate()
            .map(|(k, chunk)| {
                let (hex, ascii) = hex_row(chunk, per as usize);
                Row {
                    number: format!("{:0digits$X}", top + k as u64 * per),
                    parts: vec![(0, hex, Class::Plain), (row_width(per as usize), ascii, Class::Comment)],
                }
            })
            .collect()
    }

    fn last_top(&self, per: u64, page: usize) -> u64 {
        let rows = self.size.div_ceil(per);
        rows.saturating_sub(page.max(1) as u64) * per
    }

    pub fn scroll(&mut self, delta: i64, cols: usize, page: usize) {
        let per = Self::per_row(cols);
        let top = self.top - self.top % per;
        let moved = if delta < 0 {
            top.saturating_sub(delta.unsigned_abs().saturating_mul(per))
        } else {
            top.saturating_add(delta.unsigned_abs().saturating_mul(per))
        };
        self.top = moved.min(self.last_top(per, page));
    }

    pub fn home(&mut self) {
        self.top = 0;
    }

    pub fn place(&self) -> u64 {
        self.top
    }

    pub fn set_place(&mut self, top: u64) {
        self.top = top.min(self.size);
    }

    /// The file's length again (it may have grown or shrunk); false, and the top at the
    /// start, if it is gone.
    fn refresh_length(&mut self) -> bool {
        self.cache.clear();
        match scroller::length(&self.path) {
            Some(length) => self.size = length,
            None => self.size = 0,
        }
        if self.size == 0 {
            self.top = 0;
        }
        self.size > 0
    }

    pub fn end(&mut self, cols: usize, page: usize) {
        if self.refresh_length() {
            self.top = self.last_top(Self::per_row(cols), page);
        }
    }

    /// The bar's track spans the places a top can have: 0 to the last page.
    pub fn jump(&mut self, fraction: f32, cols: usize, page: usize) {
        if !self.refresh_length() {
            return;
        }
        let per = Self::per_row(cols);
        let at = scroller::place(fraction, 0, self.last_top(per, page));
        self.top = at - at % per;
    }

    pub fn scroll_bar(&self, page: usize, cols: usize) -> (f32, f32) {
        let per = Self::per_row(cols);
        let shown = page as u64 * per;
        let last = self.last_top(per, page);
        let at = if last == 0 { 0.0 } else { scroller::fraction(self.top, 0, last) };
        (at, (shown as f32 / self.size.max(1) as f32).clamp(0.02, 1.0))
    }

    /// The top row's offset: `0x0004_A3F0`.
    pub fn status(&self) -> String {
        let digits = format!("{:08X}", self.top);
        let mut text = String::from("0x");
        for (k, c) in digits.chars().enumerate() {
            if k > 0 && (digits.len() - k) % 4 == 0 {
                text.push('_');
            }
            text.push(c);
        }
        text
    }
}

/// Columns of a row's hex part, with the gap before its characters.
fn row_width(per: usize) -> usize {
    per * 3 + usize::from(per > 8) + 1
}

/// `48 65 6C …  6F 72` (a gap after 8 in rows of 16; a short last row padded) and its
/// characters, `.` for all but printable ASCII.
fn hex_row(bytes: &[u8], per: usize) -> (String, String) {
    let mut hex = String::with_capacity(row_width(per));
    for k in 0..per {
        if k == 8 {
            hex.push(' ');
        }
        match bytes.get(k) {
            Some(b) => {
                let _ = write!(hex, "{b:02X} ");
            }
            None => hex.push_str("   "),
        }
    }
    let ascii = bytes.iter().map(|&b| if (0x20..0x7F).contains(&b) { char::from(b) } else { '.' }).collect();
    (hex, ascii)
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn file(name: &str, bytes: &[u8]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gezik-hex-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.bin");
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn rows_of_16_and_8_bytes() {
        let (hex, ascii) = hex_row(b"Hello, world\n\0\0\0", 16);
        assert_eq!(
            format!("00004A30  {hex} {ascii}"),
            "00004A30  48 65 6C 6C 6F 2C 20 77  6F 72 6C 64 0A 00 00 00  Hello, world...."
        );
        let (hex, ascii) = hex_row(b"Hi", 8);
        assert_eq!((hex.as_str(), ascii.as_str()), ("48 69                   ", "Hi"), "a short last row is padded");
        assert_eq!(hex.len() + 1, row_width(8));
        assert_eq!(hex_row(&[0u8; 16], 16).0.len() + 1, row_width(16));
    }

    #[test]
    fn pages_through_a_file() {
        let bytes: Vec<u8> = (0..1000u32).map(|i| i as u8).collect();
        let path = file("pages", &bytes);
        let mut v = HexView::new(&path, 1000);
        let rows = v.rows(4, 80);
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[1].number, "00000010");
        assert!(rows[0].parts[0].1.starts_with("00 01 02"));
        v.scroll(2, 80, 4);
        assert_eq!(v.rows(1, 80)[0].number, "00000020");
        assert_eq!(v.status(), "0x0000_0020");
        v.end(80, 4);
        let rows = v.rows(4, 80);
        assert_eq!(rows.last().unwrap().number, "000003E0");
        assert_eq!(rows.last().unwrap().parts[1].1.len(), 8, "the last 8 bytes");
        v.scroll(1000, 80, 4);
        assert_eq!(v.rows(1, 80)[0].number, "000003B0", "never past the last page");
        v.jump(0.5, 40, 4);
        let rows = v.rows(1, 40);
        assert_eq!(rows[0].number, "000001E0", "8 bytes a row in a narrow place, halfway to the last page");
        v.end(80, 4);
        assert_eq!(v.scroll_bar(4, 80).0, 1.0, "the thumb reaches the bottom");
        v.jump(1.0, 80, 4);
        assert_eq!(v.rows(1, 80)[0].number, "000003B0", "the bottom is the last page");
        v.home();
        assert_eq!(v.rows(1, 80)[0].number, "00000000");
        assert_eq!(HexView::new(&path, 1 << 40).gutter(), 11);
        std::fs::remove_file(&path).expect("no handle is held");
    }

    #[test]
    fn random_moves_never_panic() {
        let path = file("random", &[7u8; 300]);
        let mut seed = 0x1234_5678_9ABC_DEF1u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for _ in 0..10_000 {
            let size = next() % 1000;
            let mut v = HexView::new(&path, size);
            let cols = (next() % 120) as usize;
            let page = (next() % 60) as usize;
            match next() % 4 {
                0 => v.scroll(next() as i64, cols, page),
                1 => v.jump(f32::from_bits(next() as u32), cols, page),
                2 => v.end(cols, page),
                _ => v.scroll(-(next() as i64 % 1000), cols, page),
            }
            assert!(v.top <= v.size.max(size).max(1));
            let _ = (v.scroll_bar(page, cols), v.status());
        }
        let _ = v_rows_dont_overread(&path);
    }

    #[test]
    fn the_row_width_settles_for_every_room() {
        // The pane gives the rows its width less the offsets' column: the column must not
        // depend on the row's bytes, or 8 and 16 would take turns forever.
        let v = HexView::new(Path::new("x"), 5000);
        for width in 0..300usize {
            let cols = |gutter: usize| width.saturating_sub(gutter);
            let first = HexView::per_row(cols(v.gutter()));
            let again = HexView::per_row(cols(v.gutter()));
            assert_eq!(first, again, "{width}");
            assert_eq!(v.gutter(), 8);
        }
        assert_eq!(
            (HexView::per_row(66), HexView::per_row(65), HexView::per_row(33), HexView::per_row(32)),
            (16, 8, 8, 4)
        );
    }

    fn v_rows_dont_overread(path: &Path) -> usize {
        let v = HexView::new(path, 300);
        let rows = v.rows(10_000, 80);
        assert_eq!(rows.len(), 19, "300 bytes in rows of 16");
        rows.len()
    }
}
