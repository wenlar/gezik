//! The line viewer (spec 13 §4.1–§4.3): text up to [`MEMORY_LIMIT`] decoded in memory with
//! line numbers and colors, larger files read by window around a byte offset. Both scroll with
//! the same code: the top row is a line's start (a byte offset in the source) and which of its
//! wrapped parts; lines come from windows of [`WINDOW`] bytes, a longer line is cut at a
//! character boundary. File contents are untrusted: nothing here indexes.

#![deny(clippy::indexing_slicing)]

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use gezik_core::syntax::{self, Class, LangDef};
use gezik_core::text::{Encoding, display_width};

use super::scroller::{self, WINDOW, read_at};

/// Larger text is paged, not colored and not numbered.
pub const MEMORY_LIMIT: u64 = 2 * 1024 * 1024;
/// Unwrapped lines are cut after this many columns.
const LONG_LINE: usize = 4096;
const TAB: usize = 4;

/// One row on screen: its line's number on the line's first row ("" otherwise), and the
/// colored parts at their columns.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub number: String,
    pub parts: Vec<(usize, String, Class)>,
}

enum Source {
    /// Decoded text: all of the file, or its start while `head` (the file goes on).
    Memory { text: String, starts: Vec<u32>, states: Vec<u8>, head: bool },
    /// A file over [`MEMORY_LIMIT`], read where it is shown.
    Paged { size: u64, encoding: Encoding, bom: u64, ansi: bool },
}

struct Line {
    start: u64,
    text: String,
}

pub struct TextView {
    path: PathBuf,
    /// The file's size when it was selected.
    size: u64,
    source: Source,
    lang: Option<&'static LangDef>,
    /// The top row: its line's start and which wrapped part of it.
    top: (u64, usize),
    pub wrap: bool,
    cache: scroller::Cache,
}

impl TextView {
    /// The view of `head`, the decoded start of `path` (the preview's first look); the rest
    /// is read once it is scrolled to.
    pub fn from_head(path: &Path, size: u64, head: String, wrap: (bool, bool)) -> TextView {
        let lang = path.file_name().and_then(|n| n.to_str()).and_then(syntax::lang_for);
        let partial = size > super::TEXT_BYTES as u64;
        TextView {
            path: path.to_path_buf(),
            size,
            source: memory(head, lang, partial),
            lang,
            top: (0, 0),
            cache: scroller::Cache::default(),
            wrap: if lang.is_some_and(LangDef::is_code) { wrap.1 } else { wrap.0 },
        }
    }

    /// Wrap on or off; the top row's line stays.
    pub fn set_wrap(&mut self, wrap: bool) {
        self.wrap = wrap;
        self.top.1 = 0;
    }

    /// Whether this is code (its wrap setting is `wrap-code`).
    pub fn is_code(&self) -> bool {
        self.lang.is_some_and(LangDef::is_code)
    }

    fn origin(&self) -> u64 {
        match self.source {
            Source::Memory { .. } => 0,
            Source::Paged { bom, .. } => bom,
        }
    }

    fn limit(&self) -> u64 {
        match &self.source {
            Source::Memory { text, .. } => text.len() as u64,
            Source::Paged { size, .. } => *size,
        }
    }

    /// Bytes per code unit, and big endian.
    fn unit(&self) -> (usize, bool) {
        match self.source {
            Source::Paged { encoding: Encoding::Utf16Le, .. } => (2, false),
            Source::Paged { encoding: Encoding::Utf16Be, .. } => (2, true),
            _ => (1, false),
        }
    }

    /// Paged: at most `len` bytes at `offset`. In memory: all the rest (a line longer than a
    /// window stays one line).
    fn read(&self, offset: u64, len: usize) -> Cow<'_, [u8]> {
        match &self.source {
            Source::Memory { text, .. } => {
                let start = usize::try_from(offset).unwrap_or(usize::MAX);
                Cow::Borrowed(text.as_bytes().get(start..).unwrap_or_default())
            }
            Source::Paged { .. } => Cow::Owned(self.cache.read(&self.path, offset, len)),
        }
    }

    fn decode(&self, raw: &[u8]) -> String {
        match self.source {
            Source::Memory { .. } => String::from_utf8_lossy(raw).into_owned(),
            Source::Paged { encoding: Encoding::Utf8, ansi, .. } => {
                let ansi = |b: &[u8]| if ansi { gezik_platform::decode_ansi(b) } else { None };
                gezik_core::text::utf8_or_ansi(raw, &ansi)
            }
            Source::Paged { encoding, .. } => gezik_core::text::utf16_text(raw, encoding),
        }
    }

    /// Splits `buf` (read at `base`) into lines, at most up to `max` in all; `eof`: nothing
    /// follows `buf`. Returns how much of `buf` was used.
    fn split(&self, buf: &[u8], base: u64, eof: bool, out: &mut Vec<Line>, max: usize) -> usize {
        let (unit, be) = self.unit();
        let mut i = 0;
        while out.len() < max {
            let rest = buf.get(i..).unwrap_or_default();
            if rest.is_empty() {
                break;
            }
            let (end, next) = match next_break(rest, unit, be, eof) {
                Some(found) => found,
                None if eof => (rest.len(), rest.len()),
                None if i == 0 => {
                    let cut = cut(rest, unit, be);
                    (cut, cut)
                }
                None => break,
            };
            out.push(Line { start: base + i as u64, text: self.decode(rest.get(..end).unwrap_or_default()) });
            i += next;
        }
        i
    }

    /// Up to `max` lines from `offset` on.
    fn lines(&self, mut offset: u64, max: usize) -> Vec<Line> {
        let mut out = Vec::new();
        let end = self.limit();
        while out.len() < max && offset < end {
            let buf = self.read(offset, WINDOW);
            let eof = buf.len() != WINDOW || offset + buf.len() as u64 >= end;
            let used = self.split(&buf, offset, eof, &mut out, max);
            if used == 0 {
                break;
            }
            offset += used as u64;
        }
        out
    }

    /// The lines that end at `offset` (a line start), from at most a window before it.
    fn lines_before(&self, offset: u64) -> Vec<Line> {
        let origin = self.origin();
        if offset <= origin {
            return Vec::new();
        }
        if let Source::Memory { starts, .. } = &self.source {
            // The index knows where they start.
            let before = starts.partition_point(|&s| u64::from(s) < offset);
            let from = before.saturating_sub(256);
            let first = starts.get(from).map_or(0, |&s| u64::from(s));
            let mut lines = self.lines(first, before - from);
            lines.retain(|line| line.start < offset);
            return lines;
        }
        let (unit, _) = self.unit();
        let mut start = offset.saturating_sub(WINDOW as u64).max(origin);
        start -= (start - origin) % unit as u64;
        let mut buf = self.read(start, usize::try_from(offset - start).unwrap_or(WINDOW)).into_owned();
        if start > origin && unit == 1 {
            // Not in the middle of a character.
            let skip = buf.iter().take(3).take_while(|b| (0x80..0xC0).contains(*b)).count();
            buf.drain(..skip);
            start += skip as u64;
        }
        let mut lines = Vec::new();
        self.split(&buf, start, true, &mut lines, usize::MAX);
        if start > origin && lines.len() > 1 {
            // Only part of it is in the window: it comes with the next one.
            lines.remove(0);
        }
        lines
    }

    /// The line's rows: colored (in memory) and laid out.
    fn layout(&self, line: &Line, cols: usize) -> Vec<Vec<(usize, String, Class)>> {
        let mut spans = Vec::new();
        if let (Source::Memory { starts, states, .. }, Some(lang)) = (&self.source, self.lang)
            && let Some(&state) = index_of(starts, line.start).and_then(|i| states.get(i))
        {
            syntax::line(lang, state, &line.text, Some(&mut spans));
        }
        layout(&line.text, &spans, self.wrap.then_some(cols.max(1)))
    }

    fn number(&self, start: u64) -> String {
        match &self.source {
            Source::Memory { starts, .. } => index_of(starts, start).map(|i| (i + 1).to_string()).unwrap_or_default(),
            Source::Paged { .. } => String::new(),
        }
    }

    /// The rows from the top, at most `count`, `cols` wide; each with its place.
    fn placed_rows(&self, count: usize, cols: usize) -> Vec<((u64, usize), Row)> {
        let (top, skip) = self.top;
        let mut rows = Vec::new();
        for line in self.lines(top, count + 1) {
            for (part, parts) in self.layout(&line, cols).into_iter().enumerate() {
                if line.start == top && part < skip {
                    continue;
                }
                let number = if part == 0 { self.number(line.start) } else { String::new() };
                rows.push(((line.start, part), Row { number, parts }));
                if rows.len() == count {
                    return rows;
                }
            }
        }
        rows
    }

    /// What is shown: `count` rows `cols` wide.
    pub fn rows(&self, count: usize, cols: usize) -> Vec<Row> {
        self.placed_rows(count, cols).into_iter().map(|(_, row)| row).collect()
    }

    /// Scrolls by `delta` rows (down if positive); `page` rows are shown.
    pub fn scroll(&mut self, delta: i64, cols: usize, page: usize) {
        let n = usize::try_from(delta.unsigned_abs()).unwrap_or(usize::MAX);
        if delta < 0 {
            self.up(n, cols);
        } else {
            self.down(n, cols, page);
        }
    }

    fn down(&mut self, n: usize, cols: usize, page: usize) {
        let rows = self.placed_rows(n.saturating_add(page), cols);
        if rows.len() < n.saturating_add(page) && self.is_head() {
            // The start ran out: on in all of the file.
            self.load_rest();
            return self.down(n, cols, page);
        }
        match rows.get(n) {
            Some(&(place, _)) if rows.len() - n >= page => self.top = place,
            _ => self.last_page(cols, page),
        }
    }

    fn up(&mut self, n: usize, cols: usize) {
        let (mut offset, part) = self.top;
        if part >= n {
            self.top.1 -= n;
            return;
        }
        let mut need = n - part;
        self.top.1 = 0;
        while need > 0 {
            let lines = self.lines_before(offset);
            if lines.is_empty() {
                return;
            }
            for line in lines.iter().rev() {
                let count = self.layout(line, cols).len().max(1);
                if count >= need {
                    self.top = (line.start, count - need);
                    return;
                }
                need -= count;
                offset = line.start;
                self.top = (offset, 0);
            }
        }
    }

    pub fn home(&mut self) {
        self.top = (self.origin(), 0);
    }

    /// The last rows (a log's tail): the end, `page` rows up.
    pub fn end(&mut self, cols: usize, page: usize) {
        self.load_rest();
        if self.refresh_length() {
            self.last_page(cols, page);
        }
    }

    /// A paged file's length again (a log grows); false, and the top at the start, if it is
    /// gone or empty now.
    fn refresh_length(&mut self) -> bool {
        if let Source::Paged { size, bom, .. } = &mut self.source {
            self.cache.clear();
            match scroller::length(&self.path) {
                Some(length) if length > *bom => (*size, self.size) = (length, length),
                _ => {
                    self.top = (*bom, 0);
                    return false;
                }
            }
        }
        true
    }

    /// The bytes the page from the top shows; `None` when it reaches the end.
    fn shown(&self, cols: usize, page: usize) -> Option<u64> {
        let rows = self.placed_rows(page + 1, cols);
        rows.get(page).map(|&((start, _), _)| start.saturating_sub(self.top.0))
    }

    fn last_page(&mut self, cols: usize, page: usize) {
        self.top = (self.limit(), 0);
        self.up(page, cols);
    }

    /// To `fraction` of the way down (the scroll bar).
    pub fn jump(&mut self, fraction: f32, cols: usize, page: usize) {
        self.load_rest();
        if !self.refresh_length() {
            return;
        }
        if fraction >= 1.0 {
            return self.last_page(cols, page);
        }
        // The bar's track spans the places a top can have: the end minus a page.
        let at = match &self.source {
            Source::Memory { starts, .. } => {
                let span = starts.len().saturating_sub(page);
                let line = (fraction.clamp(0.0, 1.0) * span as f32).round() as usize;
                starts.get(line.min(starts.len().saturating_sub(1))).map_or(0, |&s| u64::from(s))
            }
            Source::Paged { .. } => {
                let (unit, _) = self.unit();
                let shown = self.shown(cols, page).unwrap_or_else(|| self.limit().saturating_sub(self.top.0));
                let at =
                    scroller::place(fraction, self.origin(), self.limit().saturating_sub(shown).max(self.origin()));
                let at = at - (at - self.origin()) % unit as u64;
                // The line `at` is in.
                self.lines_before((at + unit as u64).min(self.limit())).last().map_or(at, |line| line.start)
            }
        };
        self.top = (at, 0);
        self.down(0, cols, page);
    }

    fn is_head(&self) -> bool {
        matches!(self.source, Source::Memory { head: true, .. })
    }

    /// The start is all that was read: reads the rest, in memory up to [`MEMORY_LIMIT`], else
    /// paged; the top row stays.
    // shortcut: reads up to 2 MB on the UI thread, once per file; move to the preview worker
    // if network drives make it slow.
    fn load_rest(&mut self) {
        let Source::Memory { text: head, starts, head: true, .. } = &self.source else { return };
        let line = index_of(starts, self.top.0).unwrap_or(0);
        if self.size <= MEMORY_LIMIT {
            let bytes = read_at(&self.path, 0, MEMORY_LIMIT as usize + 1);
            if bytes.len() as u64 <= MEMORY_LIMIT
                && let Some(text) = gezik_core::text::decode(&bytes, gezik_platform::decode_ansi)
            {
                let source = memory(text, self.lang, false);
                if let Source::Memory { starts, .. } = &source {
                    self.top.0 = starts.get(line).or(starts.last()).map_or(0, |&s| u64::from(s));
                }
                self.source = source;
                return;
            }
        }
        let (detected, ansi, at) = self.in_file(head);
        let part = self.top.1;
        self.source = Source::Paged {
            size: self.size.max(std::fs::metadata(&self.path).map_or(0, |m| m.len())),
            encoding: detected.encoding,
            bom: detected.bom as u64,
            ansi,
        };
        self.top = (at, part);
    }

    /// The file's encoding, whether it is in the ANSI code page, and where the top line of
    /// `decoded` (the text in memory) starts in the file.
    fn in_file(&self, decoded: &str) -> (gezik_core::text::Detected, bool, u64) {
        let probe = read_at(&self.path, 0, WINDOW);
        let detected =
            gezik_core::text::detect(&probe).unwrap_or(gezik_core::text::Detected { encoding: Encoding::Utf8, bom: 0 });
        let body = probe.get(detected.bom..).unwrap_or_default();
        let ansi = detected.encoding == Encoding::Utf8
            && std::str::from_utf8(body).is_err_and(|e| e.error_len().is_some())
            && gezik_platform::decode_ansi(b"a").is_some();
        let before = decoded.get(..usize::try_from(self.top.0).unwrap_or(0)).unwrap_or_default();
        let bytes = match detected.encoding {
            Encoding::Utf8 if ansi => before.chars().count(),
            Encoding::Utf8 => before.len(),
            _ => before.encode_utf16().count() * 2,
        };
        (detected, ansi, detected.bom as u64 + bytes as u64)
    }

    /// Where the top line starts in the file (hex opens there).
    pub fn file_offset(&self) -> u64 {
        match &self.source {
            Source::Memory { text, .. } => self.in_file(text).2,
            Source::Paged { .. } => self.top.0,
        }
    }

    /// The scroll bar: where the top is (0–1) and how much is shown.
    /// The scroll bar: where the top is (0–1, 1 at the last page) and how much is shown.
    pub fn scroll_bar(&self, cols: usize, page: usize) -> (f32, f32) {
        let shown = self.shown(cols, page);
        match &self.source {
            Source::Memory { starts, .. } => {
                let lines = starts.len().max(1);
                let size = (page as f32 / lines as f32).min(1.0);
                let line = index_of(starts, self.top.0).unwrap_or(0) as f32;
                let at = match shown {
                    None if size >= 1.0 && self.top.0 == 0 => 0.0,
                    None => 1.0,
                    Some(_) => (line / lines.saturating_sub(page).max(1) as f32).min(1.0),
                };
                (at, size)
            }
            Source::Paged { .. } => match shown {
                None => (1.0, 0.02),
                Some(shown) => {
                    let last = self.limit().saturating_sub(shown).max(self.origin() + 1);
                    (scroller::fraction(self.top.0, self.origin(), last), 0.02)
                }
            },
        }
    }

    /// The bar's right side: the number of lines, or the size and how far down.
    pub fn status(&self, cols: usize, page: usize) -> String {
        match &self.source {
            Source::Memory { starts, head: false, .. } => match starts.len() {
                1 => "1 line".to_owned(),
                n => format!("{} lines", gezik_core::group_digits(n as u64)),
            },
            // Only the start is read: its lines and more.
            Source::Memory { starts, .. } => format!("{}+ lines", gezik_core::group_digits(starts.len() as u64)),
            Source::Paged { .. } => {
                let (at, _) = self.scroll_bar(cols, page);
                format!("{} · {}%", crate::view_options::size_text(self.size), (at * 100.0).round())
            }
        }
    }

    /// Why a code file shows without colors.
    pub fn note(&self) -> &'static str {
        if self.lang.is_some() && matches!(self.source, Source::Paged { .. }) {
            "Plain text: the file is larger than 2 MB"
        } else {
            ""
        }
    }

    /// The line number column's width in characters (none when paged).
    pub fn gutter(&self) -> usize {
        match &self.source {
            Source::Memory { starts, .. } => starts.len().to_string().len(),
            Source::Paged { .. } => 0,
        }
    }
}

/// `text` in memory: where its lines start, and the colorer's state at each.
fn memory(text: String, lang: Option<&LangDef>, head: bool) -> Source {
    let mut starts = vec![0u32];
    let mut states = vec![0u8];
    let mut state = 0;
    let mut i = 0;
    while let Some((end, next)) = next_break(text.as_bytes().get(i..).unwrap_or_default(), 1, false, true) {
        if let Some(lang) = lang {
            state = syntax::line(lang, state, text.get(i..i + end).unwrap_or_default(), None);
        }
        i += next;
        if i >= text.len() {
            break;
        }
        starts.push(u32::try_from(i).unwrap_or(u32::MAX));
        states.push(state);
    }
    Source::Memory { text, starts, states, head }
}

fn index_of(starts: &[u32], offset: u64) -> Option<usize> {
    starts.binary_search(&u32::try_from(offset).ok()?).ok()
}

/// The first line break in `b` (`\n`, `\r\n` or `\r` in `unit`-byte code units): where the
/// line ends and where the next starts. A `\r` at the very end is no break unless `eof`: its
/// `\n` may follow.
fn next_break(b: &[u8], unit: usize, be: bool, eof: bool) -> Option<(usize, usize)> {
    let code = |at: usize| -> Option<u16> {
        if unit == 1 {
            return b.get(at).map(|&c| u16::from(c));
        }
        let pair: [u8; 2] = b.get(at..at + 2)?.try_into().ok()?;
        Some(if be { u16::from_be_bytes(pair) } else { u16::from_le_bytes(pair) })
    };
    let mut at = 0;
    while let Some(c) = code(at) {
        match c {
            0x0A => return Some((at, at + unit)),
            0x0D => {
                return match code(at + unit) {
                    Some(0x0A) => Some((at, at + 2 * unit)),
                    Some(_) => Some((at, at + unit)),
                    None if eof => Some((at, at + unit)),
                    None => None,
                };
            }
            _ => at += unit,
        }
    }
    None
}

/// Where to cut a line longer than a window: at a whole code unit and character.
fn cut(b: &[u8], unit: usize, be: bool) -> usize {
    let mut at = b.len() - b.len() % unit;
    if unit == 1 {
        let back = b.iter().rev().take(3).take_while(|c| (0x80..0xC0).contains(*c)).count();
        let lead = b.len().saturating_sub(back + 1);
        let length = match b.get(lead) {
            Some(0xC0..=0xDF) => 2,
            Some(0xE0..=0xEF) => 3,
            Some(0xF0..) => 4,
            _ => 1,
        };
        // A character the window cut goes to the next line whole.
        if back + 1 < length {
            at = lead;
        }
    } else if let Some(&[a, z]) = b.get(at.saturating_sub(2)..at) {
        let unit = if be { u16::from_be_bytes([a, z]) } else { u16::from_le_bytes([a, z]) };
        if (0xD800..0xDC00).contains(&unit) {
            at -= 2;
        }
    }
    if at == 0 { b.len() } else { at }
}

/// Lays `line` out in rows: tabs to the next multiple of 4, control characters as `·`, wide
/// characters 2 columns; wrapped at `wrap` columns, else cut after [`LONG_LINE`] with `…`.
/// `spans` are where the color changes (byte offsets); each row is its colored parts.
fn layout(line: &str, spans: &[(usize, Class)], wrap: Option<usize>) -> Vec<Vec<(usize, String, Class)>> {
    let mut rows = vec![Vec::new()];
    let mut col = 0;
    let mut class = Class::Plain;
    let mut next_span = 0;
    for (at, c) in line.char_indices() {
        while let Some(&(start, span_class)) = spans.get(next_span)
            && start <= at
        {
            class = span_class;
            next_span += 1;
        }
        let mut width = match c {
            '\t' => TAB - col % TAB,
            c if c.is_control() => 1,
            c => display_width(c),
        };
        if wrap.is_some_and(|cols| col + width > cols && col > 0) {
            rows.push(Vec::new());
            col = 0;
            if c == '\t' {
                width = TAB.min(wrap.unwrap_or(TAB));
            }
        }
        if wrap.is_none() && col >= LONG_LINE {
            push(&mut rows, col, "…", Class::Plain);
            break;
        }
        match c {
            '\t' => push(&mut rows, col, &" ".repeat(width), class),
            c if c.is_control() => push(&mut rows, col, "·", class),
            c => push(&mut rows, col, c.encode_utf8(&mut [0; 4]), class),
        }
        col += width;
    }
    rows
}

/// Whether a part is of wide characters (by its first).
pub fn is_wide(text: &str) -> bool {
    text.chars().next().is_some_and(|c| display_width(c) == 2)
}

fn push(rows: &mut [Vec<(usize, String, Class)>], col: usize, text: &str, class: Class) {
    let Some(row) = rows.last_mut() else { return };
    match row.last_mut() {
        // Wide characters go in parts of their own (another font draws them).
        Some((_, part, last)) if *last == class && is_wide(part) == is_wide(text) => part.push_str(text),
        _ => row.push((col, text.to_owned(), class)),
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gezik-text-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn texts(rows: &[Row]) -> Vec<String> {
        rows.iter().map(|r| r.parts.iter().map(|p| p.1.as_str()).collect()).collect()
    }

    /// A view of `path` as the preview makes it: from its first 64 KB.
    fn view(path: &Path) -> TextView {
        let size = std::fs::metadata(path).unwrap().len();
        let head = super::super::read_text(path).unwrap();
        TextView::from_head(path, size, head, (false, false))
    }

    fn numbered(lines: usize, width: usize) -> String {
        (1..=lines).map(|n| format!("{n:0width$}\n")).collect()
    }

    #[test]
    fn lines_split_on_every_kind_of_break() {
        assert_eq!(next_break(b"ab\ncd", 1, false, false), Some((2, 3)));
        assert_eq!(next_break(b"ab\r\ncd", 1, false, false), Some((2, 4)));
        assert_eq!(next_break(b"ab\rcd", 1, false, false), Some((2, 3)));
        assert_eq!(next_break(b"ab\r", 1, false, false), None, "its \\n may follow");
        assert_eq!(next_break(b"ab\r", 1, false, true), Some((2, 3)));
        assert_eq!(next_break(b"a\0\n\0", 2, false, false), Some((2, 4)));
        assert_eq!(next_break(b"\0a\0\r\0\n", 2, true, false), Some((2, 6)));
        assert_eq!(next_break(b"abc", 1, false, true), None);
    }

    #[test]
    fn long_lines_are_cut_at_characters() {
        assert_eq!(cut(b"abc", 1, false), 3);
        assert_eq!(cut("abç".as_bytes(), 1, false), 4, "a whole character at the end");
        assert_eq!(cut(&"abç".as_bytes()[..3], 1, false), 2, "a lead byte goes on");
        assert_eq!(cut(&"a😀".as_bytes()[..4], 1, false), 1);
        let utf16: Vec<u8> = "a😀".encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert_eq!(cut(&utf16[..4], 2, false), 2, "half a surrogate pair goes on");
        assert_eq!(cut(&utf16[..5], 2, false), 2);
    }

    #[test]
    fn layout_expands_tabs_wide_characters_and_wraps() {
        let rows = layout("\tab\x01", &[], None);
        assert_eq!(rows, [vec![(0, "    ab·".to_owned(), Class::Plain)]]);
        let rows = layout("日本語テキスト", &[], Some(5));
        let rows: Vec<String> = rows.iter().map(|r| r.iter().map(|p| p.1.as_str()).collect()).collect();
        assert_eq!(rows, ["日本", "語テ", "キス", "ト"], "wide characters take 2 of 5 columns");
        let rows = layout("abcd\tx", &[], Some(5));
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1], [(0, "    x".to_owned(), Class::Plain)], "a wrapped tab starts its row");
        let long = "x".repeat(LONG_LINE + 10);
        let rows = layout(&long, &[], None);
        assert_eq!(rows.len(), 1);
        assert!(rows[0][0].1.ends_with('…') && rows[0][0].1.chars().count() == LONG_LINE + 1);
        let rows = layout("fn x", &[(0, Class::Keyword), (2, Class::Plain)], None);
        assert_eq!(rows[0], [(0, "fn".to_owned(), Class::Keyword), (2, " x".to_owned(), Class::Plain)]);
    }

    #[test]
    fn small_files_scroll_with_line_numbers_and_colors() {
        let dir = temp("small");
        let path = dir.join("a.rs");
        std::fs::write(&path, "fn main() {\n    /* a\n b */\n}\n").unwrap();
        let mut v = view(&path);
        let rows = v.rows(10, 80);
        assert_eq!(texts(&rows), ["fn main() {", "    /* a", " b */", "}"]);
        assert_eq!(rows.iter().map(|r| r.number.as_str()).collect::<Vec<_>>(), ["1", "2", "3", "4"]);
        assert_eq!(rows[0].parts[0], (0, "fn ".to_owned(), Class::Keyword));
        assert_eq!(rows[2].parts[0].2, Class::Comment, "the block comment goes on");
        assert_eq!((v.status(80, 2), v.gutter(), v.note()), ("4 lines".to_owned(), 1, ""));
        v.scroll(2, 80, 2);
        assert_eq!(texts(&v.rows(2, 80)), [" b */", "}"]);
        v.scroll(5, 80, 2);
        assert_eq!(texts(&v.rows(2, 80)), [" b */", "}"], "the last row stays at the bottom");
        v.scroll(-1, 80, 2);
        assert_eq!(texts(&v.rows(1, 80)), ["    /* a"]);
        v.home();
        assert_eq!(v.rows(1, 80)[0].number, "1");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scrolling_past_the_start_keeps_the_place() {
        let dir = temp("head");
        let path = dir.join("big.txt");
        std::fs::write(&path, numbered(20_000, 6)).unwrap(); // 140,000 bytes: over 64 KB
        let mut v = view(&path);
        assert!(v.is_head());
        v.scroll(9_000, 80, 10);
        assert!(v.is_head(), "9,362 lines of 7 bytes are in the first 64 KB");
        let first_page = 9_360;
        v.scroll(360, 80, 10);
        assert!(!v.is_head(), "read all of it");
        let rows = v.rows(10, 80);
        assert_eq!(rows[0].number, (first_page + 1).to_string());
        assert_eq!(texts(&rows)[0], format!("{:06}", first_page + 1));
        assert_eq!(v.status(80, 10), "20,000 lines");
        v.end(80, 10);
        assert_eq!(texts(&v.rows(10, 80)).last().unwrap(), "020000");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn files_over_2_mb_are_paged_and_one_byte_less_is_not() {
        let dir = temp("limit");
        for (size, paged) in [(MEMORY_LIMIT, false), (MEMORY_LIMIT + 1, true)] {
            let path = dir.join(format!("{size}.rs"));
            let mut text = numbered((size / 8) as usize - 1, 7);
            text.push_str(&"x".repeat(size as usize - text.len()));
            std::fs::write(&path, &text).unwrap();
            let mut v = view(&path);
            v.end(80, 3);
            assert_eq!(matches!(v.source, Source::Paged { .. }), paged, "{size}");
            assert_eq!(v.note().is_empty(), !paged);
            let rows = v.rows(3, 80);
            assert!(texts(&rows)[2].starts_with('x'), "the last line shows");
            assert_eq!(rows[0].number.is_empty(), paged);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn paged_files_scroll_by_lines_across_windows() {
        let dir = temp("paged");
        let path = dir.join("log.txt");
        let text = numbered(400_000, 7); // 3.2 MB
        std::fs::write(&path, &text).unwrap();
        let mut v = view(&path);
        v.scroll(9_000, 80, 20); // past the first 64 KB
        assert!(matches!(v.source, Source::Paged { .. }));
        assert_eq!(texts(&v.rows(1, 80)), ["0009001"]);
        v.scroll(30_000, 80, 20);
        assert_eq!(texts(&v.rows(1, 80)), ["0039001"]);
        v.scroll(-20_000, 80, 20);
        assert_eq!(texts(&v.rows(1, 80)), ["0019001"]);
        v.end(80, 20);
        let rows = texts(&v.rows(20, 80));
        assert_eq!((rows[0].as_str(), rows[19].as_str()), ("0399981", "0400000"));
        v.jump(0.5, 80, 20);
        assert_eq!(texts(&v.rows(1, 80)), ["0199991"], "the middle of the places a top can have");
        assert!(v.status(80, 20).ends_with("· 50%"), "{}", v.status(80, 20));
        v.home();
        assert_eq!(texts(&v.rows(1, 80)), ["0000001"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn paged_windows_keep_characters_and_code_units_whole() {
        let dir = temp("units");
        // UTF-8: lines of 3-byte characters with no break for over a window.
        let path = dir.join("wide.txt");
        let line = "ç".repeat(WINDOW); // 2 bytes each: a line twice a window
        std::fs::write(&path, format!("{line}\n{line}\n").repeat(10)).unwrap();
        let mut v = view(&path);
        v.end(10_000, 1);
        v.scroll(-3, 10_000, 1);
        for row in texts(&v.rows(4, 100_000)) {
            assert!(row.chars().all(|c| c == 'ç' || c == '…'), "no broken character");
        }
        // UTF-16 with a mark.
        let path = dir.join("utf16.txt");
        let mut bytes = vec![0xFF, 0xFE];
        bytes.extend(numbered(300_000, 6).replace('\n', "\r\n").encode_utf16().flat_map(u16::to_le_bytes));
        std::fs::write(&path, &bytes).unwrap();
        let mut v = view(&path);
        v.scroll(10_000, 80, 5);
        assert_eq!(texts(&v.rows(1, 80)), ["010001"]);
        v.end(80, 2);
        assert_eq!(texts(&v.rows(2, 80)), ["299999", "300000"]);
        v.jump(0.5, 80, 5);
        assert!(texts(&v.rows(1, 80))[0].chars().all(|c| c.is_ascii_digit()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn paged_ansi_text_is_read_in_the_code_page() {
        let dir = temp("ansi");
        let path = dir.join("ansi.txt");
        let line = b"Sat\xFDr \x97 x\r\n";
        std::fs::write(&path, line.repeat(300_000)).unwrap();
        let mut v = view(&path);
        v.end(80, 1);
        let shown = texts(&v.rows(1, 80)).remove(0);
        assert_eq!(Some(shown), gezik_platform::decode_ansi(&line[..line.len() - 2]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_line_longer_than_a_window_stays_one_line_in_memory() {
        let dir = temp("longline");
        let path = dir.join("min.json");
        let long = "x".repeat(100 * 1024);
        std::fs::write(&path, format!("a\n{long}\nb\nc\n")).unwrap();
        let mut v = view(&path);
        v.end(80, 2);
        assert!(!v.is_head());
        assert_eq!(texts(&v.rows(2, 80)), ["b", "c"]);
        let numbers = |v: &TextView| v.rows(4, 80).iter().map(|r| r.number.clone()).collect::<Vec<_>>();
        v.scroll(-1, 80, 2);
        assert_eq!(numbers(&v), ["2", "3", "4"], "no row without a number: the long line is one");
        v.scroll(-1, 80, 2);
        assert_eq!(numbers(&v), ["1", "2", "3", "4"]);
        v.scroll(1, 80, 2);
        assert_eq!(numbers(&v)[0], "2", "scrolling down from it does not reset");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_scroll_thumb_reaches_the_bottom_at_the_end() {
        let dir = temp("thumb");
        let small = dir.join("small.txt");
        std::fs::write(&small, numbered(100, 3)).unwrap();
        let big = dir.join("big.txt");
        std::fs::write(&big, numbered(400_000, 7)).unwrap();
        for path in [small, big] {
            let mut v = view(&path);
            v.end(80, 10);
            assert_eq!(v.scroll_bar(80, 10).0, 1.0, "{}", path.display());
            v.home();
            assert_eq!(v.scroll_bar(80, 10).0, 0.0);
            v.jump(1.0, 80, 10);
            assert_eq!(
                texts(&v.rows(10, 80)).last().map(|t| t.trim_start_matches('0').to_owned()).as_deref(),
                Some(if path.ends_with("big.txt") { "400000" } else { "100" })
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wide_characters_get_parts_of_their_own_and_hex_opens_at_the_top_line() {
        let rows = layout("a日本b", &[], None);
        let parts: Vec<(usize, &str)> = rows[0].iter().map(|p| (p.0, p.1.as_str())).collect();
        assert_eq!(parts, [(0, "a"), (1, "日本"), (5, "b")]);
        assert!(is_wide("日本") && !is_wide("a"));
        let dir = temp("offset");
        let path = dir.join("a.txt");
        std::fs::write(&path, "\u{FEFF}ç1\nç2\nç3\n").unwrap();
        let mut v = view(&path);
        v.scroll(2, 80, 1);
        assert_eq!((v.top, v.file_offset()), ((8, 0), 3 + 8), "after the mark and two lines of 4 bytes");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_vanished_file_goes_back_to_the_start() {
        let dir = temp("vanish");
        let path = dir.join("log.txt");
        std::fs::write(&path, numbered(400_000, 7)).unwrap();
        let mut v = view(&path);
        v.end(80, 5);
        std::fs::remove_file(&path).unwrap();
        v.end(80, 5);
        assert_eq!(v.top, (0, 0));
        assert!(v.rows(5, 80).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wrapped_rows_scroll_by_row() {
        let dir = temp("wrap");
        let path = dir.join("a.txt");
        std::fs::write(&path, "abcdefgh\nij\n").unwrap();
        let mut v = view(&path);
        v.wrap = true;
        assert_eq!(texts(&v.rows(5, 3)), ["abc", "def", "gh", "ij"]);
        assert_eq!(v.rows(5, 3).iter().map(|r| r.number.as_str()).collect::<Vec<_>>(), ["1", "", "", "2"]);
        v.scroll(2, 3, 2);
        assert_eq!(texts(&v.rows(2, 3)), ["gh", "ij"]);
        v.scroll(-1, 3, 2);
        assert_eq!(texts(&v.rows(1, 3)), ["def"]);
        v.end(3, 3);
        assert_eq!(texts(&v.rows(3, 3)), ["def", "gh", "ij"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nothing_is_read_until_scrolled_and_no_handle_stays() {
        let dir = temp("handle");
        let path = dir.join("big.log");
        std::fs::write(&path, numbered(400_000, 7)).unwrap();
        let mut v = view(&path);
        let before = scroller::READS.with(std::cell::Cell::get);
        let _ = v.rows(20, 80);
        assert_eq!(scroller::READS.with(std::cell::Cell::get), before, "the first look is the worker's");
        v.end(80, 20);
        assert!(scroller::READS.with(std::cell::Cell::get) > before);
        std::fs::remove_file(&path).expect("no handle is held, even while shown");
        drop(v);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn random_bytes_page_without_panicking() {
        let dir = temp("random");
        let mut seed = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for round in 0..40 {
            let path = dir.join(format!("{round}.txt"));
            let len = 1 + (next() % 200_000) as usize;
            let bytes: Vec<u8> = (0..len)
                .map(|_| match next() % 8 {
                    0 => b'\n',
                    1 => b'\r',
                    2 => 0xC3,
                    3 => 0xA7,
                    _ => b'a' + (next() % 26) as u8,
                })
                .collect();
            std::fs::write(&path, &bytes).unwrap();
            let mut v = TextView::from_head(
                &path,
                len as u64,
                String::from_utf8_lossy(&bytes[..len.min(65536)]).into(),
                (round % 2 == 0, false),
            );
            for _ in 0..20 {
                match next() % 5 {
                    0 => v.scroll((next() % 5000) as i64, 1 + (next() % 100) as usize, 1 + (next() % 50) as usize),
                    1 => v.scroll(-((next() % 5000) as i64), 1 + (next() % 100) as usize, 1 + (next() % 50) as usize),
                    2 => v.jump((next() % 1000) as f32 / 1000.0, 40, 10),
                    3 => v.end(40, 10),
                    _ => v.home(),
                }
                let _ = v.rows(30, 40);
                let _ = v.status(40, 10);
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
