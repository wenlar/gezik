//! Text encoding conversion: chardetng 1.0 detection, encoding_rs 0.8 decode/encode,
//! manual UTF-16 output, line endings, trailing-space cleanup, streaming in fixed chunks.
use std::io::{Read, Write};

use chardetng::{EncodingDetector, Iso2022JpDetection, Utf8Detection};
use encoding_rs::{
    DecoderResult, EncoderResult, Encoding, UTF_16BE, UTF_16LE, UTF_8,
};

pub const SNIFF: usize = 64 * 1024;

#[derive(Debug, PartialEq)]
pub enum Detected {
    /// BOM found; `bom_len` bytes to skip.
    Bom(&'static Encoding, usize),
    /// Only ASCII bytes in the sniffed part (valid as UTF-8 and every ASCII-compatible encoding).
    Ascii,
    /// chardetng's guess (never UTF-16; it can be UTF-8).
    Guess(&'static Encoding),
    /// NUL byte in the first 8 KB and no UTF-16 BOM.
    Binary,
}

/// `head` = the first SNIFF bytes of the file; `whole` = true if that is the whole file.
/// `tld` = lower-case country code from the UI locale (e.g. b"tr"), biases legacy guesses.
pub fn detect(head: &[u8], whole: bool, tld: Option<&[u8]>) -> Detected {
    if let Some((enc, bom_len)) = Encoding::for_bom(head) {
        return Detected::Bom(enc, bom_len); // UTF-8 / UTF-16LE / UTF-16BE BOMs
    }
    if head[..head.len().min(8192)].contains(&0) {
        return Detected::Binary;
    }
    if head.is_ascii() {
        return Detected::Ascii;
    }
    let mut det = EncodingDetector::new(Iso2022JpDetection::Deny);
    det.feed(head, whole); // last=false when truncated: a split UTF-8 sequence is not an error
    Detected::Guess(det.guess(tld, Utf8Detection::Allow))
}

#[derive(Clone, Copy, Debug)]
pub enum Eol {
    AsIs,
    Lf,
    Crlf,
    Cr,
}

#[derive(Clone, Copy, Debug)]
pub enum Target {
    /// Any encoding_rs encoding except UTF-16 (whose `output_encoding()` is UTF-8).
    Enc(&'static Encoding),
    Utf8Bom,
    Utf16Le,
    Utf16Be,
}

#[derive(Debug)]
pub enum TextError {
    /// Source bytes are not valid in the source encoding (1-based line).
    Malformed { line: u64 },
    /// Character can't be encoded in the target (1-based line).
    Unmappable { ch: char, line: u64, target: &'static str },
    Io(std::io::Error),
}
impl From<std::io::Error> for TextError {
    fn from(e: std::io::Error) -> Self {
        TextError::Io(e)
    }
}

pub struct Options {
    pub eol: Eol,
    pub strip_trailing: bool,
    pub single_final_newline: bool,
}

/// Line-ending / whitespace processor that works on arbitrary chunk boundaries.
struct Lines {
    opts_eol: Eol,
    strip: bool,
    single_final: bool,
    pending_cr: bool,        // saw '\r', waiting to know if '\n' follows
    ws: String,              // trailing-space candidate in the current line
    newlines: Vec<&'static str>, // newlines not yet written (for single_final_newline)
    line: u64,               // 1-based line of the next character written
    any: bool,               // wrote a non-blank character (chunks reuse their out buffer)
}

impl Lines {
    fn new(o: &Options) -> Self {
        Lines {
            opts_eol: o.eol,
            strip: o.strip_trailing,
            single_final: o.single_final_newline,
            pending_cr: false,
            ws: String::new(),
            newlines: Vec::new(),
            line: 1,
            any: false,
        }
    }
    fn eol_str(&self, orig: &'static str) -> &'static str {
        match self.opts_eol {
            Eol::AsIs => orig,
            Eol::Lf => "\n",
            Eol::Crlf => "\r\n",
            Eol::Cr => "\r",
        }
    }
    fn newline(&mut self, orig: &'static str, out: &mut String) {
        if !self.strip {
            out.push_str(&self.ws);
        }
        self.ws.clear();
        let nl = self.eol_str(orig);
        if self.single_final {
            self.newlines.push(nl); // held back until more content arrives
        } else {
            out.push_str(nl);
        }
        self.line += 1;
    }
    fn flush_newlines(&mut self, out: &mut String) {
        for nl in self.newlines.drain(..) {
            out.push_str(nl);
        }
    }
    /// Processes one decoded chunk. Returns, for each output char range, nothing; the caller
    /// uses `line_of(byte_index)` via the `marks` vector: (byte offset in `out`, line).
    fn push(&mut self, s: &str, out: &mut String, marks: &mut Vec<(usize, u64)>) {
        for c in s.chars() {
            if self.pending_cr {
                self.pending_cr = false;
                if c == '\n' {
                    self.newline("\r\n", out);
                    marks.push((out.len(), self.line));
                    continue;
                }
                self.newline("\r", out);
                marks.push((out.len(), self.line));
            }
            match c {
                '\r' => self.pending_cr = true,
                '\n' => {
                    self.newline("\n", out);
                    marks.push((out.len(), self.line));
                }
                ' ' | '\t' => self.ws.push(c),
                _ => {
                    self.flush_newlines(out);
                    out.push_str(&self.ws);
                    self.ws.clear();
                    out.push(c);
                    self.any = true;
                }
            }
        }
    }
    fn finish(&mut self, out: &mut String, marks: &mut Vec<(usize, u64)>) {
        if self.pending_cr {
            self.pending_cr = false;
            self.newline("\r", out);
            marks.push((out.len(), self.line));
        }
        if self.single_final {
            // Exactly one line ending at the end; held-back blank lines are dropped.
            self.newlines.clear();
            if !self.strip {
                out.push_str(&self.ws);
            }
            self.ws.clear();
            if self.any {
                out.push_str(self.eol_str("
"));
            }
        } else if !self.strip {
            out.push_str(&self.ws);
            self.ws.clear();
        }
    }
}

fn line_at(marks: &[(usize, u64)], start_line: u64, idx: usize) -> u64 {
    // marks: (offset where line L begins, L). Find the last mark at or before idx.
    match marks.iter().rposition(|m| m.0 <= idx) {
        Some(i) => marks[i].1,
        None => start_line,
    }
}

/// Streaming conversion: reads `src` in 64 KB chunks, never holds the whole file.
/// `from` = source encoding (BOM bytes are skipped by `new_decoder_with_bom_removal`).
pub fn convert(
    mut src: impl Read,
    mut dst: impl Write,
    from: &'static Encoding,
    to: Target,
    opts: &Options,
) -> Result<(), TextError> {
    let mut decoder = from.new_decoder_with_bom_removal();
    let mut encoder = match to {
        Target::Enc(e) => Some(e.new_encoder()),
        _ => None,
    };
    match to {
        Target::Utf8Bom => dst.write_all(b"\xEF\xBB\xBF")?,
        Target::Utf16Le => dst.write_all(&[0xFF, 0xFE])?,
        Target::Utf16Be => dst.write_all(&[0xFE, 0xFF])?,
        Target::Enc(_) => {}
    }
    let mut lines = Lines::new(opts);
    let mut inbuf = vec![0u8; 64 * 1024];
    let mut decoded = String::with_capacity(128 * 1024);
    let mut processed = String::with_capacity(128 * 1024);
    let mut outbuf = vec![0u8; 256 * 1024];
    let mut src_line: u64 = 1; // for malformed-input errors
    let mut eof = false;
    while !eof {
        let n = src.read(&mut inbuf)?;
        eof = n == 0;
        let mut input = &inbuf[..n];
        loop {
            decoded.clear();
            // decode_to_string never grows the String: reserve the worst case first.
            decoded.reserve(decoder.max_utf8_buffer_length_without_replacement(input.len()).unwrap());
            let (res, read) =
                decoder.decode_to_string_without_replacement(input, &mut decoded, eof);
            src_line += decoded.matches('\n').count() as u64;
            input = &input[read..];
            if let DecoderResult::Malformed(_, _) = res {
                return Err(TextError::Malformed { line: src_line });
            }
            processed.clear();
            let start_line = lines.line;
            let mut marks = Vec::new();
            lines.push(&decoded, &mut processed, &mut marks);
            if eof && res == DecoderResult::InputEmpty {
                lines.finish(&mut processed, &mut marks);
            }
            // Encode `processed`.
            match (&mut encoder, to) {
                (Some(enc), Target::Enc(e)) => {
                    let mut s = processed.as_str();
                    let mut consumed = 0usize;
                    loop {
                        let (r, read, written) = enc.encode_from_utf8_without_replacement(
                            s,
                            &mut outbuf,
                            eof && res == DecoderResult::InputEmpty,
                        );
                        dst.write_all(&outbuf[..written])?;
                        consumed += read;
                        s = &s[read..];
                        match r {
                            EncoderResult::InputEmpty => break,
                            EncoderResult::OutputFull => continue,
                            EncoderResult::Unmappable(ch) => {
                                let at = consumed - ch.len_utf8();
                                return Err(TextError::Unmappable {
                                    ch,
                                    line: line_at(&marks, start_line, at),
                                    target: e.name(),
                                });
                            }
                        }
                    }
                }
                (_, Target::Utf8Bom) => dst.write_all(processed.as_bytes())?,
                (_, Target::Utf16Le) => {
                    let b: Vec<u8> = processed.encode_utf16().flat_map(u16::to_le_bytes).collect();
                    dst.write_all(&b)?
                }
                (_, Target::Utf16Be) => {
                    let b: Vec<u8> = processed.encode_utf16().flat_map(u16::to_be_bytes).collect();
                    dst.write_all(&b)?
                }
                _ => unreachable!(),
            }
            if res == DecoderResult::InputEmpty {
                break;
            }
            // OutputFull cannot happen after the reserve above; loop defensively anyway.
        }
    }
    dst.flush()?;
    Ok(())
}

/// The names Gezik would show for the spec's list, resolved through encoding_rs labels.
pub fn label(l: &str) -> Option<&'static Encoding> {
    Encoding::for_label(l.as_bytes())
}

pub fn utf16le() -> &'static Encoding {
    UTF_16LE
}
pub fn utf16be() -> &'static Encoding {
    UTF_16BE
}
pub fn utf8() -> &'static Encoding {
    UTF_8
}
