//! PDF: the `/Info` dictionary (Title, Author, Subject) and the page count of the page tree's
//! root, looked for in the first and last 64 KB only. A PDF that keeps them in compressed
//! object streams gives nothing (spec 13 §16).

use std::io::{Read, Seek};

use encoding_rs::{UTF_16BE, WINDOWS_1252};
use gezik_core::fields as f;

use super::{Out, some_bytes};

const PART: u64 = 64 * 1024;

pub(super) fn read<R: Read + Seek>(r: &mut R, len: u64, out: &mut Out) {
    let head = some_bytes(r, 0, PART);
    if !head.starts_with(b"%PDF-") {
        return;
    }
    let tail = if len > PART { some_bytes(r, len - PART, PART) } else { Vec::new() };
    // The last trailer's /Info wins (an updated file appends a new one).
    let info_ref = [&tail, &head].into_iter().find_map(|part| {
        let at = rfind(part, b"/Info")?;
        reference(part.get(at + 5..)?)
    });
    if let Some((number, generation)) = info_ref {
        let header = format!("{number} {generation} obj");
        let dict = [&tail, &head].into_iter().find_map(|part| {
            let at = object_at(part, header.as_bytes())?;
            let body = part.get(at..)?;
            body.get(..find(body, b"endobj").unwrap_or(body.len()))
        });
        if let Some(dict) = dict {
            for (key, id) in [(&b"/Title"[..], f::TITLE), (b"/Author", f::AUTHOR), (b"/Subject", f::SUBJECT)] {
                if let Some(text) = string_after(dict, key) {
                    out.text(id, &text);
                }
            }
        }
    }
    // The page tree's root: a /Pages node without a /Parent; the largest count. Objects are
    // told apart at each `j` (of `obj` and `endobj`): a page tree node has none of its own.
    let pages = [&head, &tail]
        .into_iter()
        .flat_map(|part| part.split(|b| *b == b'j'))
        .filter(|object| is_pages_root(object))
        .filter_map(|object| number_after(object, b"/Count"))
        .max();
    out.int(f::PAGES, pages.unwrap_or(0));
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn rfind(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).rposition(|w| w == needle)
}

/// `12 0 R` at the start (after spaces): the object number and generation.
fn reference(b: &[u8]) -> Option<(u64, u64)> {
    let text: String = b.iter().take(32).map(|c| char::from(*c)).collect();
    let mut words = text.split_ascii_whitespace();
    let number = words.next()?.parse().ok()?;
    let generation = words.next()?.parse().ok()?;
    words.next()?.starts_with('R').then_some((number, generation))
}

/// Where `N G obj` starts as a whole object header (not `1N G obj`).
fn object_at(part: &[u8], header: &[u8]) -> Option<usize> {
    let mut from = 0;
    while let Some(i) = find(part.get(from..)?, header) {
        let at = from + i;
        if at == 0 || part.get(at - 1).is_some_and(|b| !b.is_ascii_digit()) {
            return Some(at + header.len());
        }
        from = at + 1;
    }
    None
}

/// A dictionary piece is the tree's root: `/Type /Pages` and no `/Parent`.
fn is_pages_root(object: &[u8]) -> bool {
    let Some(at) = find(object, b"/Type") else { return false };
    let rest = object.get(at + 5..).unwrap_or_default();
    let rest = rest.get(rest.iter().take_while(|b| b.is_ascii_whitespace()).count()..).unwrap_or_default();
    rest.starts_with(b"/Pages")
        && rest.get(6).is_none_or(|b| !b.is_ascii_alphanumeric())
        && find(object, b"/Parent").is_none()
}

fn number_after(object: &[u8], key: &[u8]) -> Option<u64> {
    let rest = object.get(find(object, key)? + key.len()..)?;
    let digits: String = rest
        .iter()
        .skip_while(|b| b.is_ascii_whitespace())
        .take_while(|b| b.is_ascii_digit())
        .map(|b| char::from(*b))
        .collect();
    digits.parse().ok()
}

/// The string value of `key` in a dictionary: `(literal)` or `<hex>`, UTF-16BE with its mark or
/// PDFDocEncoding (read as Latin-1).
fn string_after(dict: &[u8], key: &[u8]) -> Option<String> {
    let mut at = find(dict, key)? + key.len();
    // `/Title` is not `/TitleX`.
    if dict.get(at).is_some_and(u8::is_ascii_alphanumeric) {
        return None;
    }
    while dict.get(at).is_some_and(u8::is_ascii_whitespace) {
        at += 1;
    }
    let raw = match dict.get(at)? {
        b'(' => literal(dict.get(at + 1..)?),
        b'<' => hex(dict.get(at + 1..)?),
        _ => return None,
    };
    Some(match raw.strip_prefix(&[0xFE, 0xFF]) {
        Some(utf16) => UTF_16BE.decode_without_bom_handling(utf16).0.into_owned(),
        None => WINDOWS_1252.decode_without_bom_handling(&raw).0.into_owned(),
    })
}

/// A literal string's bytes up to its closing parenthesis: nesting and escapes.
fn literal(b: &[u8]) -> Vec<u8> {
    let (mut out, mut depth, mut i) = (Vec::new(), 0usize, 0usize);
    while let Some(&c) = b.get(i) {
        i += 1;
        match c {
            b'\\' => {
                let Some(&next) = b.get(i) else { break };
                i += 1;
                match next {
                    b'n' => out.push(b'\n'),
                    b'r' => out.push(b'\r'),
                    b't' => out.push(b'\t'),
                    b'b' => out.push(8),
                    b'f' => out.push(12),
                    b'0'..=b'7' => {
                        let mut value = u32::from(next - b'0');
                        for _ in 0..2 {
                            match b.get(i) {
                                Some(d @ b'0'..=b'7') => {
                                    value = value * 8 + u32::from(d - b'0');
                                    i += 1;
                                }
                                _ => break,
                            }
                        }
                        out.push(value as u8);
                    }
                    // A line break after a backslash continues the string.
                    b'\r' | b'\n' => {}
                    other => out.push(other),
                }
            }
            b'(' => {
                depth += 1;
                out.push(c);
            }
            b')' if depth == 0 => break,
            b')' => {
                depth -= 1;
                out.push(c);
            }
            _ => out.push(c),
        }
        if out.len() > 4 * super::TEXT_MAX {
            break;
        }
    }
    out
}

/// A hex string's bytes up to `>`; an odd last digit is followed by 0.
fn hex(b: &[u8]) -> Vec<u8> {
    let digits: Vec<u8> = b
        .iter()
        .take_while(|c| **c != b'>')
        .filter_map(|c| char::from(*c).to_digit(16))
        .map(|d| d as u8)
        .take(8 * super::TEXT_MAX)
        .collect();
    digits.chunks(2).map(|pair| (pair.first().copied().unwrap_or(0) << 4) | pair.get(1).copied().unwrap_or(0)).collect()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn fixture() -> Vec<u8> {
        std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/fields/info.pdf")).unwrap()
    }

    #[test]
    fn info_and_pages() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/fields/info.pdf");
        let fields = super::super::read(&path, f::FieldSet::ALL, &|| false);
        assert_eq!(fields.text(f::TITLE).as_deref(), Some("Yıllık Özet"));
        assert_eq!(fields.text(f::AUTHOR).as_deref(), Some("Ay(s)e A"));
        assert_eq!(fields.text(f::SUBJECT).as_deref(), Some("Bütçe"));
        assert_eq!(fields.text(f::PAGES).as_deref(), Some("2"));
    }

    #[test]
    fn strings() {
        assert_eq!(literal(b"a(b)c\\)d) rest"), b"a(b)c)d");
        assert_eq!(literal(b"\\101\\7x\\\nz)"), b"A\x07xz");
        assert_eq!(hex(b"FEFF00 41 4>"), [0xFE, 0xFF, 0x00, 0x41, 0x40]);
        assert_eq!(reference(b" 12 0 R /Root"), Some((12, 0)));
        assert_eq!(reference(b" 12 0 obj"), None);
        assert!(is_pages_root(b" 2 0 obj << /Type /Pages /Count 3 >> endob"));
        assert!(!is_pages_root(b"<< /Type /Pages /Parent 2 0 R /Count 3 >>"));
        assert!(!is_pages_root(b"<< /Type /PagesX >>"));
        assert_eq!(string_after(b"/TitleX (no) /Title (yes)", b"/Title"), None, "the first match only");
    }

    #[test]
    fn hostile_pdfs_do_not_panic() {
        super::super::tests::fuzz(&[fixture()], 8, &|r, len| read(r, len, &mut Out::new(f::FieldSet::ALL)));
    }
}
