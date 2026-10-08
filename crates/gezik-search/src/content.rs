//! Text in files (spec 3.3): which files are read, how their text is decoded, and their first
//! matching line. Read in 256 KB chunks, cut at line ends; a line over 1 MB is cut there (a
//! match across that cut can be missed: the documented limit; `^` and `$` also match at the
//! cut points of such a line). Stops at the first match.

use std::io::{self, Read};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use gezik_core::kind::Kind;
use gezik_core::text::{self, Encoding};
use regex::{Regex, RegexBuilder};

/// A 1-based line number and about [`EXCERPT_CHARS`] characters around the match.
pub type Found = (u32, Box<str>);

pub const CHUNK: usize = 256 * 1024;
pub const MAX_LINE: usize = 1024 * 1024;
pub const EXCERPT_CHARS: usize = 160;
/// How much of the line before the match the excerpt keeps.
const BEFORE: usize = 40;
/// `[search] content-max-size`'s default.
pub const DEFAULT_MAX_SIZE: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct ContentMatcher {
    regex: Regex,
    max_size: u64,
}

impl ContentMatcher {
    /// Plain text is matched as it is; without `match_case` it ignores case and takes İ for i
    /// (and ı for I). A regular expression only gets `(?i)` then.
    pub fn compile(text: &str, regex: bool, match_case: bool, max_size: u64) -> Result<ContentMatcher, String> {
        let source = match (regex, match_case) {
            (true, true) => text.to_owned(),
            (true, false) => format!("(?i){text}"),
            (false, true) => regex::escape(text),
            (false, false) => format!("(?i){}", crate::name::turkish_i(&regex::escape(text))),
        };
        let regex = RegexBuilder::new(&source).build().map_err(|err| crate::name::regex_error(&err))?;
        Ok(ContentMatcher { regex, max_size })
    }

    /// Whether it matches an empty text (`a*`, `^`): such a pattern says nothing about what a
    /// file holds, so a search takes it as no content criterion.
    pub fn matches_empty(&self) -> bool {
        self.regex.is_match("")
    }

    /// Whether a file is read at all: not over the size limit, not a kind that holds no plain
    /// text (by its name: nothing is opened to know).
    pub fn reads(&self, name: &str, size: u64) -> bool {
        size <= self.max_size && !skipped_kind(name)
    }

    pub fn find_in_file(
        &self,
        path: &Path,
        cancel: &AtomicBool,
        ansi: &dyn Fn(&[u8]) -> Option<String>,
    ) -> io::Result<Option<Found>> {
        // Never a FIFO, device or link, never a cloud placeholder (spec 3.3).
        let Some((file, size)) = gezik_platform::fs::open_regular(path)? else { return Ok(None) };
        // A small file needs no 256 KB buffer.
        let buffer = usize::try_from(size.saturating_add(1)).map_or(CHUNK, |size| size.min(CHUNK));
        self.find_with(file.take(self.max_size), buffer, cancel, ansi)
    }

    /// The first line of `reader` that matches; `None` for none, binary data (a NUL byte in
    /// 8-bit text, in any chunk), or a cancelled search (looked at before every chunk).
    pub fn find(
        &self,
        reader: impl Read,
        cancel: &AtomicBool,
        ansi: &dyn Fn(&[u8]) -> Option<String>,
    ) -> io::Result<Option<Found>> {
        self.find_with(reader, CHUNK, cancel, ansi)
    }

    /// `find` reading `buffer` bytes at a time (at most `CHUNK`).
    fn find_with(
        &self,
        mut reader: impl Read,
        buffer: usize,
        cancel: &AtomicBool,
        ansi: &dyn Fn(&[u8]) -> Option<String>,
    ) -> io::Result<Option<Found>> {
        let mut buf = vec![0u8; buffer.clamp(1, CHUNK)];
        let mut filled = read_full(&mut reader, &mut buf)?;
        let Some(detected) = text::detect(&buf[..filled]) else { return Ok(None) };
        let mut decoder = match detected.encoding {
            Encoding::Utf8 => Decoder::Utf8,
            utf16 => Decoder::Utf16(utf16),
        };
        let mut lines = Lines { regex: &self.regex, number: 0, partial: String::new() };
        let mut carry = Vec::new();
        let mut from = detected.bom;
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Ok(None);
            }
            let last = filled < buf.len();
            // Text in an 8-bit encoding has no NUL: a file that starts like text and holds one
            // later (an archive, a library) is binary.
            if !matches!(decoder, Decoder::Utf16(_)) && buf[from..filled].contains(&0) {
                return Ok(None);
            }
            let text = decoder.decode(&mut carry, &buf[from..filled], ansi, last);
            if let Some(found) = lines.feed(&text) {
                return Ok(Some(found));
            }
            if last {
                break;
            }
            filled = read_full(&mut reader, &mut buf)?;
            from = 0;
            if filled == 0 {
                let rest = decoder.decode(&mut carry, &[], ansi, true);
                if let Some(found) = lines.feed(&rest) {
                    return Ok(Some(found));
                }
                break;
            }
        }
        Ok(lines.finish())
    }
}

/// Build outputs and data files that start like text but are not (an `ar` archive's header).
const BINARY_ENDINGS: [&str; 19] = [
    "rlib", "rmeta", "o", "obj", "a", "lib", "pdb", "ilk", "pch", "idb", "exp", "so", "dylib", "class", "pyc", "wasm",
    "db", "sqlite", "bin",
];

/// Kinds that hold no plain text, by their ending (CSV is text, unlike the other sheets).
pub fn skipped_kind(name: &str) -> bool {
    if let Some((_, ending)) = name.rsplit_once('.')
        && BINARY_ENDINGS.iter().any(|binary| ending.eq_ignore_ascii_case(binary))
    {
        return true;
    }
    match Kind::of(name, false) {
        Kind::Image
        | Kind::Video
        | Kind::Audio
        | Kind::Archive
        | Kind::DiskImage
        | Kind::Pdf
        | Kind::Document
        | Kind::Presentation
        | Kind::Font
        | Kind::Executable => true,
        Kind::Spreadsheet => !name.to_ascii_lowercase().ends_with(".csv"),
        _ => false,
    }
}

/// About [`EXCERPT_CHARS`] characters of `line` around the match starting at byte `start`
/// (some of what comes before it, then the match and what follows), leading spaces trimmed.
pub fn excerpt(line: &str, start: usize) -> Box<str> {
    let before = line[..start].chars().count();
    let text: String = line.chars().skip(before.saturating_sub(BEFORE)).take(EXCERPT_CHARS).collect();
    text.trim_start().into()
}

/// Reads until `buf` is full or the end; how much it got.
fn read_full(reader: &mut impl Read, buf: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
            Err(err) => return Err(err),
        }
    }
    Ok(filled)
}

/// How many bytes at the end of `data` start a UTF-8 character that the end cuts short.
fn incomplete_tail(data: &[u8]) -> usize {
    for i in 1..=data.len().min(3) {
        let byte = data[data.len() - i];
        match byte {
            0x80..=0xBF => continue,
            0xC0..=0xDF => return if i < 2 { i } else { 0 },
            0xE0..=0xEF => return if i < 3 { i } else { 0 },
            0xF0..=0xF7 => return if i < 4 { i } else { 0 },
            _ => return 0,
        }
    }
    0
}

enum Decoder {
    /// UTF-8 until a chunk holds bytes that are not (then [`Decoder::Ansi`]: the decision is
    /// made per chunk, so a code-page file whose first 256 KB is ASCII is still read right).
    Utf8,
    /// The user's code page where the bytes are not UTF-8 (a multi-byte code page can lose a
    /// character at a chunk's end).
    Ansi,
    Utf16(Encoding),
}

impl Decoder {
    /// Text of what `carry` kept and `bytes`, keeping back an end a chunk cut (part of a UTF-8
    /// character, an odd byte, half a surrogate pair) unless `last`.
    fn decode(
        &mut self,
        carry: &mut Vec<u8>,
        bytes: &[u8],
        ansi: &dyn Fn(&[u8]) -> Option<String>,
        last: bool,
    ) -> String {
        carry.extend_from_slice(bytes);
        let data = std::mem::take(carry);
        match self {
            Decoder::Utf8 | Decoder::Ansi => {
                let keep = data.len() - if last { 0 } else { incomplete_tail(&data) };
                carry.extend_from_slice(&data[keep..]);
                let body = &data[..keep];
                if matches!(self, Decoder::Utf8) {
                    if std::str::from_utf8(body).is_ok() {
                        return String::from_utf8_lossy(body).into_owned();
                    }
                    *self = Decoder::Ansi;
                }
                text::utf8_or_ansi(body, ansi)
            }
            Decoder::Utf16(encoding) => {
                let mut end = data.len() & !1;
                if !last && end >= 2 {
                    let pair = [data[end - 2], data[end - 1]];
                    let unit = if *encoding == Encoding::Utf16Be {
                        u16::from_be_bytes(pair)
                    } else {
                        u16::from_le_bytes(pair)
                    };
                    if (0xD800..0xDC00).contains(&unit) {
                        end -= 2;
                    }
                }
                if !last {
                    carry.extend_from_slice(&data[end..]);
                }
                text::utf16_text(&data[..end], *encoding)
            }
        }
    }
}

/// Decoded text cut into lines, counted from 1.
struct Lines<'a> {
    regex: &'a Regex,
    /// Whole lines seen so far.
    number: u32,
    /// The line still being read.
    partial: String,
}

impl Lines<'_> {
    fn feed(&mut self, text: &str) -> Option<Found> {
        self.partial.push_str(text);
        let mut start = 0;
        while let Some(end) = self.partial[start..].find('\n') {
            self.number = self.number.saturating_add(1);
            if let Some(found) = check(self.regex, &self.partial[start..start + end], self.number) {
                return Some(found);
            }
            start += end + 1;
        }
        self.partial.drain(..start);
        if self.partial.len() > MAX_LINE {
            let mut cut = MAX_LINE;
            while !self.partial.is_char_boundary(cut) {
                cut -= 1;
            }
            // The line goes on: its number is the next one, not counted yet.
            if let Some(found) = check(self.regex, &self.partial[..cut], self.number.saturating_add(1)) {
                return Some(found);
            }
            self.partial.drain(..cut);
        }
        None
    }

    /// The last line, without a line end.
    fn finish(&mut self) -> Option<Found> {
        if self.partial.is_empty() {
            return None;
        }
        check(self.regex, &self.partial, self.number.saturating_add(1))
    }
}

fn check(regex: &Regex, line: &str, number: u32) -> Option<Found> {
    let line = line.strip_suffix('\r').unwrap_or(line);
    let found = regex.find(line)?;
    Some((number, excerpt(line, found.start())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    fn none(_: &[u8]) -> Option<String> {
        None
    }

    fn matcher(text: &str, regex: bool, case: bool) -> ContentMatcher {
        ContentMatcher::compile(text, regex, case, DEFAULT_MAX_SIZE).unwrap()
    }

    fn find(matcher: &ContentMatcher, bytes: &[u8]) -> Option<Found> {
        matcher.find(bytes, &AtomicBool::new(false), &none).unwrap()
    }

    fn utf16(text: &str, little: bool) -> Vec<u8> {
        text.encode_utf16().flat_map(|u| if little { u.to_le_bytes() } else { u.to_be_bytes() }).collect()
    }

    #[test]
    fn the_first_matching_line_and_its_number() {
        let text = b"one\ntwo fatura here\nthree fatura\n";
        let (line, excerpt) = find(&matcher("fatura", false, false), text).unwrap();
        assert_eq!((line, &*excerpt), (2, "two fatura here"));
        assert_eq!(find(&matcher("nothing", false, false), text), None);
    }

    #[test]
    fn crlf_lines_have_their_numbers() {
        let (line, excerpt) = find(&matcher("b", false, false), b"a\r\nb\r\nc").unwrap();
        assert_eq!((line, &*excerpt), (2, "b"), "the \\r is not part of the line");
        assert_eq!(find(&matcher("c$", true, false), b"a\r\nc\r\n").map(|f| f.0), Some(2));
    }

    #[test]
    fn all_the_encodings_are_read() {
        let m = matcher("şehir", false, false);
        let mut bom8 = b"\xEF\xBB\xBF".to_vec();
        bom8.extend("x\nŞEHİR".as_bytes());
        assert_eq!(find(&m, &bom8).map(|f| f.0), Some(2), "UTF-8 with a mark");
        for little in [true, false] {
            let mut marked = if little { vec![0xFF, 0xFE] } else { vec![0xFE, 0xFF] };
            marked.extend(utf16("bir satır\nbu şehir", little));
            assert_eq!(find(&m, &marked).map(|f| f.0), Some(2), "UTF-16 with a mark, little {little}");
        }
    }

    #[test]
    fn utf16_without_a_mark_is_read() {
        let bytes = utf16("ilk satır biraz uzun\nikinci satırda ŞEHİR var", true);
        assert_eq!(find(&matcher("şehir", false, false), &bytes).map(|f| f.0), Some(2));
    }

    #[test]
    fn binary_files_are_skipped() {
        let mut bytes = b"MZ\x00\x00 fatura".to_vec();
        bytes.extend([0u8; 100]);
        assert_eq!(find(&matcher("fatura", false, false), &bytes), None);
    }

    #[test]
    fn a_line_across_two_chunks_is_matched_whole() {
        let mut text = "x".repeat(CHUNK - 4);
        text.push_str("\nab fatura cd\nlast");
        let (line, excerpt) = find(&matcher("fatura", false, false), text.as_bytes()).unwrap();
        assert_eq!((line, &*excerpt), (2, "ab fatura cd"));
        // A two-byte letter cut by the chunk's end.
        let mut cut = "y".repeat(CHUNK - 1);
        cut.push_str("ş fatura");
        assert_eq!(find(&matcher("ş fatura", false, false), cut.as_bytes()).map(|f| f.0), Some(1));
    }

    #[test]
    fn a_very_long_line_is_cut_but_searched() {
        let mut text = "fatura ".to_owned();
        text.push_str(&"z".repeat(MAX_LINE + MAX_LINE / 2));
        text.push_str("\nnext");
        let (line, excerpt) = find(&matcher("fatura", false, false), text.as_bytes()).unwrap();
        assert_eq!(line, 1);
        assert!(excerpt.starts_with("fatura zzz"));
        let mut tail = "z".repeat(MAX_LINE + 10);
        tail.push_str("needle");
        assert_eq!(
            find(&matcher("needle", false, false), tail.as_bytes()).map(|f| f.0),
            Some(1),
            "the rest of the cut line"
        );
    }

    #[test]
    fn case_and_the_turkish_i_fold_unless_asked() {
        assert!(find(&matcher("istanbul", false, false), "İSTANBUL'da".as_bytes()).is_some());
        assert!(find(&matcher("ılık", false, false), "ILIK su".as_bytes()).is_some());
        assert!(find(&matcher("ILIK", false, false), "ılık su".as_bytes()).is_some());
        assert!(find(&matcher("istanbul", false, true), "İSTANBUL'da".as_bytes()).is_none(), "match case");
        assert!(find(&matcher("f.tura", false, false), b"fatura").is_none(), "plain text is no regex");
        assert!(find(&matcher("f.tura", true, false), b"FATURA").is_some(), "a regex ignores case");
        assert!(ContentMatcher::compile("(", true, false, 1).is_err());
    }

    #[test]
    fn the_encoding_is_decided_per_chunk() {
        // Windows-1254-like bytes (0xFD, 0xDE) after more than a chunk of ASCII.
        let mut bytes = b"a".repeat(CHUNK + 10);
        bytes.extend(b"\nSat\xFDr \xDEehir fatura\n");
        let ansi = |b: &[u8]| Some(b.iter().map(|&c| if c >= 0x80 { '\u{131}' } else { c as char }).collect());
        let found = matcher("fatura", false, false).find(&bytes[..], &AtomicBool::new(false), &ansi).unwrap();
        assert_eq!(found.map(|f| f.0), Some(2));
        // A UTF-8 character cut by the chunk's end, in ANSI mode: carried, none lost.
        for cut in 1..3 {
            let mut data = b"\xFD\n".to_vec();
            data.extend(b"x".repeat(CHUNK - 2 - cut));
            data.extend("€ fatura".as_bytes());
            let found = matcher("€ fatura", false, false).find(&data[..], &AtomicBool::new(false), &ansi).unwrap();
            assert_eq!(found.map(|f| f.0), Some(2), "cut {cut}");
        }
    }

    #[test]
    fn the_excerpt_is_short_and_trimmed() {
        let line = format!("    {}MATCH{}", "a".repeat(300), "b".repeat(300));
        let (_, shown) = find(&matcher("match", false, false), line.as_bytes()).unwrap();
        assert!(shown.chars().count() <= EXCERPT_CHARS);
        assert!(shown.contains("MATCH") && !shown.starts_with(' '));
        assert_eq!(&*excerpt("   hello", 3), "hello");
    }

    #[test]
    fn pictures_archives_and_big_files_are_not_read() {
        let m = ContentMatcher::compile("x", false, false, 100).unwrap();
        for name in
            ["a.jpg", "a.mp4", "a.mp3", "a.zip", "a.iso", "a.pdf", "a.docx", "a.xlsx", "a.pptx", "a.ttf", "a.exe"]
        {
            assert!(!m.reads(name, 10), "{name}");
        }
        for name in ["a.txt", "a.md", "main.rs", "data.json", "log", "a.csv"] {
            assert!(m.reads(name, 10), "{name}");
        }
        assert!(!m.reads("a.txt", 101), "over content-max-size");
    }

    #[test]
    fn build_outputs_and_data_files_are_not_read() {
        let m = ContentMatcher::compile("x", false, false, 100).unwrap();
        for name in ["libx.rlib", "a.rmeta", "a.o", "A.PDB", "x.so", "c.class", "d.sqlite", "e.bin", "f.wasm", "g.lib"]
        {
            assert!(!m.reads(name, 10), "{name}");
        }
        assert!(m.reads("notes.bin.txt", 10) && m.reads("lib", 10));
    }

    #[test]
    fn a_nul_in_a_later_chunk_makes_the_file_binary() {
        let mut bytes = b"x\n".repeat(CHUNK / 2);
        bytes.extend(b"\0 needle\n");
        assert_eq!(find(&matcher("needle", false, false), &bytes), None);
        let mut early = b"needle\n".to_vec();
        early.extend(&bytes);
        assert_eq!(find(&matcher("needle", false, false), &early).map(|f| f.0), Some(1), "found before the NUL");
    }

    #[test]
    fn files_are_read_with_a_buffer_their_size() {
        let dir = std::env::temp_dir().join(format!("gezik-content-file-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let m = matcher("fatura", false, false);
        let cancel = AtomicBool::new(false);
        std::fs::write(dir.join("small.txt"), "one\ntwo fatura").unwrap();
        let found = m.find_in_file(&dir.join("small.txt"), &cancel, &none).unwrap();
        assert_eq!(found.map(|f| f.0), Some(2));
        let mut big = "x\n".repeat(CHUNK);
        big.push_str("fatura\n");
        std::fs::write(dir.join("big.txt"), &big).unwrap();
        let found = m.find_in_file(&dir.join("big.txt"), &cancel, &none).unwrap();
        assert_eq!(found.map(|f| f.0), Some(CHUNK as u32 + 1));
        std::fs::write(dir.join("empty.txt"), "").unwrap();
        assert_eq!(m.find_in_file(&dir.join("empty.txt"), &cancel, &none).unwrap(), None);
        assert_eq!(m.find_in_file(&dir, &cancel, &none).unwrap(), None, "a folder is not read");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_cancelled_search_reads_no_further() {
        let text = "x\n".repeat(CHUNK);
        let cancel = AtomicBool::new(true);
        assert_eq!(matcher("x", false, false).find(text.as_bytes(), &cancel, &none).unwrap(), None);
    }

    #[cfg(windows)]
    #[test]
    fn text_in_the_ansi_code_page_is_read() {
        // "Satır — fatura" in Windows-1254 (0x97: the em dash in every Windows code page).
        let bytes = b"Sat\xFDr \x97 fatura";
        let m = matcher("fatura", false, false);
        let found = m.find(&bytes[..], &AtomicBool::new(false), &gezik_platform::decode_ansi).unwrap().unwrap();
        assert!(found.1.ends_with("\u{2014} fatura"), "{}", found.1);
    }
}
