//! Converting text files between encodings (encoding_rs, every WHATWG encoding) and line
//! endings, with an optional trailing-space trim and a single final line ending.
//!
//! Detection: a BOM first, then chardetng on the first [`SNIFF`] bytes. A file with a NUL in
//! its first 8 KB and no BOM is binary and left alone (BOM-less UTF-16 looks binary too).
//! encoding_rs follows WHATWG, where ISO-8859-1 and ISO-8859-9 are Windows-1252 and
//! Windows-1254; the list shows them as aliases. UTF-16 is written by hand (encoding_rs only
//! decodes it).
//!
//! Conversion streams in 64 KB reads, whatever the file's size: decoder and encoder state, a
//! CR at the end of a read and a character split between reads carry over. Bytes that are not
//! valid in the source encoding, and characters the target cannot hold, fail the file with
//! the line they are on; nothing is left at the output then.

use crate::archive::cancelled;
use crate::archive::io::read_full;
use std::fmt;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;

use chardetng::{EncodingDetector, Iso2022JpDetection, Utf8Detection};
use encoding_rs::{
    DecoderResult, Encoder, EncoderResult, Encoding, REPLACEMENT, UTF_8, UTF_16BE, UTF_16LE, X_USER_DEFINED,
};
use gezik_core::batch::convert::{Eol, TextOptions};

/// How many bytes from the start of a file [`detect`] looks at.
pub const SNIFF: usize = 64 * 1024;

/// How much of the start of a file is searched for a NUL byte.
const BINARY_SNIFF: usize = 8 * 1024;

/// The read size while converting.
const CHUNK: usize = 64 * 1024;

/// What a file's start says it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detected {
    /// Text in this encoding; `true` when it starts with that encoding's BOM. Plain ASCII is
    /// UTF-8.
    Text(&'static Encoding, bool),
    /// A NUL byte in the first 8 KB and no BOM.
    Binary,
}

/// Detects the encoding of a file from its first bytes (up to [`SNIFF`]; fewer means that is
/// the whole file).
pub fn detect(head: &[u8]) -> Detected {
    if let Some((encoding, _)) = Encoding::for_bom(head) {
        // Before the NUL test: UTF-16 text is full of NULs.
        return Detected::Text(encoding, true);
    }
    if head[..head.len().min(BINARY_SNIFF)].contains(&0) {
        return Detected::Binary;
    }
    if head.is_ascii() {
        return Detected::Text(UTF_8, false);
    }
    let mut detector = EncodingDetector::new(Iso2022JpDetection::Deny);
    // Not the last bytes when cut at SNIFF: a UTF-8 sequence split there is not an error.
    detector.feed(head, head.len() < SNIFF);
    Detected::Text(detector.guess(None, Utf8Detection::Allow), false)
}

/// The encodings offered, as (label, name shown): encoding_rs name, display name, list name.
/// The common ones first, then the rest of WHATWG's (without "replacement" and
/// "x-user-defined", which are not text encodings one picks).
const ENCODINGS: &[(&str, &str, &str)] = &[
    ("UTF-8", "UTF-8", "UTF-8"),
    ("UTF-16LE", "UTF-16 LE", "UTF-16 LE"),
    ("UTF-16BE", "UTF-16 BE", "UTF-16 BE"),
    ("windows-1254", "Windows-1254", "Windows-1254 (ISO-8859-9)"),
    ("windows-1252", "Windows-1252", "Windows-1252 (ISO-8859-1)"),
    ("windows-1251", "Windows-1251", "Windows-1251"),
    ("windows-1250", "Windows-1250", "Windows-1250"),
    ("ISO-8859-15", "ISO-8859-15", "ISO-8859-15"),
    ("KOI8-R", "KOI8-R", "KOI8-R"),
    ("Shift_JIS", "Shift_JIS", "Shift_JIS"),
    ("GBK", "GBK", "GBK"),
    ("Big5", "Big5", "Big5"),
    ("windows-1253", "Windows-1253", "Windows-1253"),
    ("windows-1255", "Windows-1255", "Windows-1255"),
    ("windows-1256", "Windows-1256", "Windows-1256"),
    ("windows-1257", "Windows-1257", "Windows-1257"),
    ("windows-1258", "Windows-1258", "Windows-1258"),
    ("windows-874", "Windows-874", "Windows-874"),
    ("ISO-8859-2", "ISO-8859-2", "ISO-8859-2"),
    ("ISO-8859-3", "ISO-8859-3", "ISO-8859-3"),
    ("ISO-8859-4", "ISO-8859-4", "ISO-8859-4"),
    ("ISO-8859-5", "ISO-8859-5", "ISO-8859-5"),
    ("ISO-8859-6", "ISO-8859-6", "ISO-8859-6"),
    ("ISO-8859-7", "ISO-8859-7", "ISO-8859-7"),
    ("ISO-8859-8", "ISO-8859-8", "ISO-8859-8"),
    ("ISO-8859-8-I", "ISO-8859-8-I", "ISO-8859-8-I"),
    ("ISO-8859-10", "ISO-8859-10", "ISO-8859-10"),
    ("ISO-8859-13", "ISO-8859-13", "ISO-8859-13"),
    ("ISO-8859-14", "ISO-8859-14", "ISO-8859-14"),
    ("ISO-8859-16", "ISO-8859-16", "ISO-8859-16"),
    ("KOI8-U", "KOI8-U", "KOI8-U"),
    ("IBM866", "IBM866", "IBM866"),
    ("macintosh", "Mac Roman", "Mac Roman"),
    ("x-mac-cyrillic", "Mac Cyrillic", "Mac Cyrillic"),
    ("gb18030", "GB18030", "GB18030"),
    ("EUC-JP", "EUC-JP", "EUC-JP"),
    ("ISO-2022-JP", "ISO-2022-JP", "ISO-2022-JP"),
    ("EUC-KR", "EUC-KR", "EUC-KR"),
];

/// The encodings to choose from, as (label for [`TextOptions`], name for the UI), the common
/// ones first. ISO-8859-1 and ISO-8859-9 are shown as aliases of Windows-1252 and
/// Windows-1254, which is what encoding_rs makes of them.
pub fn encodings() -> Vec<(&'static str, &'static str)> {
    ENCODINGS.iter().map(|&(label, _, shown)| (label, shown)).collect()
}

/// An encoding's name in messages ("Windows-1254", "UTF-16 LE").
pub fn name(encoding: &'static Encoding) -> &'static str {
    ENCODINGS.iter().find(|row| row.0 == encoding.name()).map_or(encoding.name(), |row| row.1)
}

/// A character the target encoding has no code for, carried in an `io::Error`
/// (`io::Error::other`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unmappable {
    pub ch: char,
    /// 1-based, in the output (after the line-ending changes).
    pub line: u64,
    /// The target's name ([`name`]).
    pub encoding: &'static str,
}

impl fmt::Display for Unmappable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "can't encode '{}' in {} (line {})", self.ch, self.encoding, self.line)
    }
}

impl std::error::Error for Unmappable {}

/// Bytes that are not valid in the source encoding, carried in an `io::Error`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Malformed {
    /// 1-based.
    pub line: u64,
    /// The source's name ([`name`]).
    pub encoding: &'static str,
}

impl fmt::Display for Malformed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "not valid {} text (line {})", self.encoding, self.line)
    }
}

impl std::error::Error for Malformed {}

/// The file looks binary (see [`Detected::Binary`]), carried in an `io::Error`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LooksBinary;

impl fmt::Display for LooksBinary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "looks binary")
    }
}

impl std::error::Error for LooksBinary {}

/// Whether a conversion failed because the file looks binary (to skip it rather than fail).
pub fn is_binary(err: &io::Error) -> bool {
    err.get_ref().is_some_and(|inner| inner.is::<LooksBinary>())
}

/// An encoding by label; "replacement" and "x-user-defined" are not taken.
fn by_label(label: &str) -> io::Result<&'static Encoding> {
    Encoding::for_label(label.trim().as_bytes())
        .filter(|&e| e != REPLACEMENT && e != X_USER_DEFINED)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, format!("unknown encoding '{label}'")))
}

/// Converts `input` into `output` (a temporary name the caller renames) as `options` say.
/// With no source encoding given, it is detected ([`detect`]); a binary file fails with
/// [`LooksBinary`] (see [`is_binary`]). With an empty target each file keeps its encoding and
/// its BOM. `stop` is asked before each 64 KB read; then the answer is `Interrupted`. On any
/// failure nothing is left at `output`.
pub fn convert_text(input: &Path, output: &Path, options: &TextOptions, stop: &dyn Fn() -> bool) -> io::Result<()> {
    let result = convert(input, output, options, stop);
    if result.is_err() {
        let _ = std::fs::remove_file(output);
    }
    result
}

fn convert(input: &Path, output: &Path, options: &TextOptions, stop: &dyn Fn() -> bool) -> io::Result<()> {
    if stop() {
        return Err(cancelled());
    }
    let mut src = File::open(input)?;
    let mut head = vec![0u8; SNIFF];
    let n = read_full(&mut src, &mut head)?;
    head.truncate(n);
    let detected = detect(&head);
    let source = match &options.from {
        Some(label) => by_label(label)?,
        None => match detected {
            Detected::Text(encoding, _) => encoding,
            Detected::Binary => return Err(io::Error::other(LooksBinary)),
        },
    };
    // A source given by hand is still checked, except UTF-16, which may come without a BOM.
    if detected == Detected::Binary && source != UTF_16LE && source != UTF_16BE {
        return Err(io::Error::other(LooksBinary));
    }
    let had_bom = Encoding::for_bom(&head).is_some_and(|(e, _)| e == source);
    let (target, bom) =
        if options.keeps_encoding() { (source, had_bom || options.bom) } else { (by_label(&options.to)?, options.bom) };

    let mut sink = Sink::new(target, bom);
    let mut dst = BufWriter::with_capacity(256 * 1024, File::create(output)?);
    sink.start(&mut dst)?;

    let mut decoder = source.new_decoder_with_bom_removal();
    let mut lines = Lines::new(options);
    let mut pending = head; // the bytes read for detection come first
    let mut inbuf = vec![0u8; CHUNK];
    let mut decoded = String::new();
    let mut processed = String::new();
    let mut marks = Vec::new();
    loop {
        let last;
        let chunk: &[u8] = if pending.is_empty() {
            if stop() {
                return Err(cancelled());
            }
            let n = read_full(&mut src, &mut inbuf)?;
            last = n == 0;
            &inbuf[..n]
        } else {
            last = false;
            &pending
        };
        let mut rest = chunk;
        loop {
            decoded.clear();
            // decode_to_string never grows the String: reserve the worst case first.
            let worst = decoder
                .max_utf8_buffer_length_without_replacement(rest.len())
                .ok_or_else(|| io::Error::other("text chunk too large"))?;
            decoded.reserve(worst);
            let (result, read) = decoder.decode_to_string_without_replacement(rest, &mut decoded, last);
            rest = &rest[read..];
            processed.clear();
            marks.clear();
            let start_line = lines.line;
            lines.push(&decoded, &mut processed, &mut marks);
            if let DecoderResult::Malformed(_, _) = result {
                return Err(io::Error::other(Malformed { line: lines.current_line(), encoding: name(source) }));
            }
            let done = result == DecoderResult::InputEmpty;
            if last && done {
                lines.finish(&mut processed, &mut marks);
            }
            sink.write(&processed, last && done, &marks, start_line, &mut dst)?;
            if done {
                break;
            }
        }
        if last {
            break;
        }
        if !pending.is_empty() {
            pending = Vec::new();
        }
    }
    dst.flush()?;
    Ok(())
}

/// Where decoded text goes: UTF-8, UTF-16 (by hand), or an encoding_rs encoder.
enum Sink {
    Utf8 { bom: bool },
    Utf16 { le: bool, bom: bool, buf: Vec<u8> },
    Other { encoder: Encoder, name: &'static str, buf: Vec<u8> },
}

impl Sink {
    fn new(target: &'static Encoding, bom: bool) -> Sink {
        if target == UTF_8 {
            Sink::Utf8 { bom }
        } else if target == UTF_16LE || target == UTF_16BE {
            Sink::Utf16 { le: target == UTF_16LE, bom, buf: Vec::new() }
        } else {
            Sink::Other { encoder: target.new_encoder(), name: name(target), buf: vec![0u8; 256 * 1024] }
        }
    }

    fn start(&self, dst: &mut impl Write) -> io::Result<()> {
        match self {
            Sink::Utf8 { bom: true } => dst.write_all(b"\xEF\xBB\xBF"),
            Sink::Utf16 { le: true, bom: true, .. } => dst.write_all(&[0xFF, 0xFE]),
            Sink::Utf16 { le: false, bom: true, .. } => dst.write_all(&[0xFE, 0xFF]),
            _ => Ok(()),
        }
    }

    /// Writes one processed chunk; `marks` and `start_line` give the line of each byte of `s`.
    fn write(
        &mut self,
        s: &str,
        last: bool,
        marks: &[(usize, u64)],
        start_line: u64,
        dst: &mut impl Write,
    ) -> io::Result<()> {
        match self {
            Sink::Utf8 { .. } => dst.write_all(s.as_bytes()),
            Sink::Utf16 { le, buf, .. } => {
                buf.clear();
                for unit in s.encode_utf16() {
                    buf.extend_from_slice(&if *le { unit.to_le_bytes() } else { unit.to_be_bytes() });
                }
                dst.write_all(buf)
            }
            Sink::Other { encoder, name, buf } => {
                let mut rest = s;
                let mut consumed = 0;
                loop {
                    let (result, read, written) = encoder.encode_from_utf8_without_replacement(rest, buf, last);
                    dst.write_all(&buf[..written])?;
                    consumed += read;
                    rest = &rest[read..];
                    match result {
                        EncoderResult::InputEmpty => return Ok(()),
                        EncoderResult::OutputFull => {}
                        EncoderResult::Unmappable(ch) => {
                            let at = consumed - ch.len_utf8();
                            let line = marks.iter().rev().find(|m| m.0 <= at).map_or(start_line, |m| m.1);
                            return Err(io::Error::other(Unmappable { ch, line, encoding: name }));
                        }
                    }
                }
            }
        }
    }
}

/// Line endings, trailing spaces and the final line ending, on chunks cut anywhere.
struct Lines {
    eol: Eol,
    trim: bool,
    final_newline: bool,
    /// A CR was the last character; whether a LF follows is not known yet.
    pending_cr: bool,
    /// Spaces and tabs not written yet (trimming: dropped if the line ends here).
    spaces: String,
    /// Line endings held back (final newline: dropped if only blank lines follow), as runs.
    held: Vec<(&'static str, u64)>,
    /// The first line ending in the file (for a final one with `Eol::Keep`).
    first_eol: Option<&'static str>,
    /// 1-based line of the next character.
    line: u64,
    /// Whether anything but line endings was written.
    any: bool,
}

impl Lines {
    fn new(options: &TextOptions) -> Lines {
        Lines {
            eol: options.eol,
            trim: options.trim_trailing,
            final_newline: options.final_newline,
            pending_cr: false,
            spaces: String::new(),
            held: Vec::new(),
            first_eol: None,
            line: 1,
            any: false,
        }
    }

    /// The line the next character is on (a pending CR has ended the current one).
    fn current_line(&self) -> u64 {
        self.line + u64::from(self.pending_cr)
    }

    fn eol_for(&self, original: &'static str) -> &'static str {
        match self.eol {
            Eol::Keep => original,
            Eol::Lf => "\n",
            Eol::Crlf => "\r\n",
            Eol::Cr => "\r",
        }
    }

    /// Writes the held line endings and spaces before content.
    fn content(&mut self, out: &mut String) {
        for (eol, count) in self.held.drain(..) {
            for _ in 0..count {
                out.push_str(eol);
            }
        }
        out.push_str(&self.spaces);
        self.spaces.clear();
        self.any = true;
    }

    fn end_line(&mut self, original: &'static str, out: &mut String, marks: &mut Vec<(usize, u64)>) {
        // Spaces are only held back when trimming, and then they go.
        self.spaces.clear();
        self.first_eol.get_or_insert(original);
        let eol = self.eol_for(original);
        if self.final_newline {
            match self.held.last_mut() {
                Some((held, count)) if *held == eol => *count += 1,
                _ => self.held.push((eol, 1)),
            }
        } else {
            out.push_str(eol);
        }
        self.line += 1;
        marks.push((out.len(), self.line));
    }

    /// Processes one decoded chunk into `out`; `marks` gets (offset in `out`, line) where each
    /// line starts.
    fn push(&mut self, s: &str, out: &mut String, marks: &mut Vec<(usize, u64)>) {
        for c in s.chars() {
            if self.pending_cr {
                self.pending_cr = false;
                if c == '\n' {
                    self.end_line("\r\n", out, marks);
                    continue;
                }
                self.end_line("\r", out, marks);
            }
            match c {
                '\r' => self.pending_cr = true,
                '\n' => self.end_line("\n", out, marks),
                ' ' | '\t' if self.trim => self.spaces.push(c),
                _ => {
                    self.content(out);
                    out.push(c);
                }
            }
        }
    }

    fn finish(&mut self, out: &mut String, marks: &mut Vec<(usize, u64)>) {
        if self.pending_cr {
            self.pending_cr = false;
            self.end_line("\r", out, marks);
        }
        // Trailing spaces at the very end: trimmed, or kept as they are.
        self.spaces.clear();
        if self.final_newline {
            // Exactly one line ending after the last content; blank lines after it go.
            let eol = match self.held.first() {
                Some(&(eol, _)) => eol,
                None => self.eol_for(self.first_eol.unwrap_or("\n")),
            };
            self.held.clear();
            if self.any {
                out.push_str(eol);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str, eol: Eol, trim: bool, final_newline: bool) -> String {
        let options =
            TextOptions { from: None, to: String::new(), bom: false, eol, trim_trailing: trim, final_newline };
        let mut lines = Lines::new(&options);
        let (mut out, mut marks) = (String::new(), Vec::new());
        // One character at a time: every chunk boundary.
        for c in text.chars() {
            lines.push(c.encode_utf8(&mut [0; 4]), &mut out, &mut marks);
        }
        lines.finish(&mut out, &mut marks);
        out
    }

    #[test]
    fn line_endings_and_cleanup() {
        let text = "a  \r\nb\t\rc \nd\r\n\r\n\r\n";
        assert_eq!(lines(text, Eol::Keep, false, false), text);
        assert_eq!(lines(text, Eol::Lf, false, false), "a  \nb\t\nc \nd\n\n\n");
        assert_eq!(lines(text, Eol::Crlf, true, true), "a\r\nb\r\nc\r\nd\r\n");
        assert_eq!(lines(text, Eol::Cr, true, false), "a\rb\rc\rd\r\r\r");
        assert_eq!(lines(text, Eol::Lf, false, true), "a  \nb\t\nc \nd\n");
    }

    #[test]
    fn final_newline_keeps_spaces_on_their_line() {
        assert_eq!(lines("a\n  \n\n", Eol::Keep, false, true), "a\n  \n");
        assert_eq!(lines("a\n\n  ", Eol::Keep, false, true), "a\n\n  \n");
        assert_eq!(lines("a\n\n  ", Eol::Keep, true, true), "a\n");
        assert_eq!(lines("a\r\nb", Eol::Keep, false, true), "a\r\nb\r\n");
        assert_eq!(lines("a", Eol::Keep, false, true), "a\n");
        assert_eq!(lines("\n\n", Eol::Keep, false, true), "");
        assert_eq!(lines("x  ", Eol::Keep, true, false), "x");
    }

    #[test]
    fn detects_boms_text_and_binary() {
        assert_eq!(detect(b"\xEF\xBB\xBFabc"), Detected::Text(UTF_8, true));
        assert_eq!(detect(b"\xFF\xFEa\0b\0"), Detected::Text(UTF_16LE, true));
        assert_eq!(detect(b"\xFE\xFF\0a\0b"), Detected::Text(UTF_16BE, true));
        assert_eq!(detect(b"a\0b\0"), Detected::Binary);
        assert_eq!(detect(b"plain"), Detected::Text(UTF_8, false));
        assert_eq!(detect("Işık ağaç".as_bytes()), Detected::Text(UTF_8, false));
        let (cp1254, _, _) = encoding_rs::WINDOWS_1254.encode("Işık ağaç şöyle güzel çiçekler\n");
        assert_eq!(detect(&cp1254), Detected::Text(encoding_rs::WINDOWS_1254, false));
    }

    #[test]
    fn every_listed_label_is_its_encoding() {
        let list = encodings();
        assert_eq!(list.len(), 38);
        for (label, _) in &list {
            let encoding = Encoding::for_label(label.as_bytes()).unwrap();
            assert_eq!(encoding.name(), *label);
        }
        assert_eq!(list[3], ("windows-1254", "Windows-1254 (ISO-8859-9)"));
        assert_eq!(by_label("iso-8859-9").unwrap(), encoding_rs::WINDOWS_1254);
        assert_eq!(name(encoding_rs::WINDOWS_1252), "Windows-1252");
        assert!(by_label("replacement").is_err() && by_label("nope").is_err());
    }
}
