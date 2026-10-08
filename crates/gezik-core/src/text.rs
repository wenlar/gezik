//! Whether bytes are text and in which encoding (spec 3.3), for the preview and the content
//! search: UTF-8 and UTF-16 by their byte order marks; UTF-16 without one when one byte of
//! nearly every pair is NUL; binary when the start holds a NUL; else UTF-8, and text that is
//! not valid UTF-8 in the user's legacy code page (the caller's `ansi`: gezik-core asks no
//! system).

/// How much of the start decides.
pub const PROBE: usize = 8 * 1024;
/// Shorter starts are never taken for UTF-16 without a mark: a few bytes of a program's header
/// would pass the test.
const MIN_UNMARKED_UTF16: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf16Le,
    Utf16Be,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Detected {
    pub encoding: Encoding,
    /// Bytes of byte order mark to skip.
    pub bom: usize,
}

/// The encoding of text starting with `start`; `None` for binary data.
pub fn detect(start: &[u8]) -> Option<Detected> {
    let marked = |encoding, bom| Some(Detected { encoding, bom });
    if start.starts_with(b"\xEF\xBB\xBF") {
        return marked(Encoding::Utf8, 3);
    }
    if start.starts_with(b"\xFF\xFE") {
        return marked(Encoding::Utf16Le, 2);
    }
    if start.starts_with(b"\xFE\xFF") {
        return marked(Encoding::Utf16Be, 2);
    }
    let probe = &start[..start.len().min(PROBE)];
    if let Some(encoding) = unmarked_utf16(probe) {
        return marked(encoding, 0);
    }
    if probe.contains(&0) {
        return None;
    }
    marked(Encoding::Utf8, 0)
}

/// UTF-16 without a mark: at least 40% of the odd bytes NUL and at most 5% of the even ones
/// (little endian, as Latin text is), or the other way round.
fn unmarked_utf16(probe: &[u8]) -> Option<Encoding> {
    if probe.len() < MIN_UNMARKED_UTF16 {
        return None;
    }
    let pairs = probe.len() / 2;
    let nuls = |offset: usize| probe[offset..].iter().step_by(2).take(pairs).filter(|b| **b == 0).count();
    let percent = |n: usize| n * 100 / pairs;
    let (even, odd) = (percent(nuls(0)), percent(nuls(1)));
    if odd >= 40 && even <= 5 {
        Some(Encoding::Utf16Le)
    } else if even >= 40 && odd <= 5 {
        Some(Encoding::Utf16Be)
    } else {
        None
    }
}

/// Text from byte pairs; an odd last byte and half a surrogate pair at the end (cut by a read
/// limit) are left out.
pub fn utf16_text(bytes: &[u8], encoding: Encoding) -> String {
    let (pairs, _) = bytes.as_chunks::<2>();
    let unit =
        |pair: [u8; 2]| if encoding == Encoding::Utf16Be { u16::from_be_bytes(pair) } else { u16::from_le_bytes(pair) };
    let mut units: Vec<u16> = pairs.iter().map(|&pair| unit(pair)).collect();
    if units.last().is_some_and(|u| (0xD800..0xDC00).contains(u)) {
        units.pop();
    }
    String::from_utf16_lossy(&units)
}

/// UTF-8 text, a character cut by the end left out; bytes that are not UTF-8 go to `ansi`
/// (lossy UTF-8 where it gives nothing).
pub fn utf8_or_ansi(bytes: &[u8], ansi: &dyn Fn(&[u8]) -> Option<String>) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_owned(),
        Err(err) if err.error_len().is_none() => String::from_utf8_lossy(&bytes[..err.valid_up_to()]).into_owned(),
        Err(_) => ansi(bytes).unwrap_or_else(|| String::from_utf8_lossy(bytes).into_owned()),
    }
}

/// `bytes` (the start of a file) as text; `None` for binary data.
pub fn decode(bytes: &[u8], ansi: impl Fn(&[u8]) -> Option<String>) -> Option<String> {
    let detected = detect(bytes)?;
    let body = &bytes[detected.bom..];
    Some(match detected.encoding {
        Encoding::Utf8 => utf8_or_ansi(body, &ansi),
        utf16 => utf16_text(body, utf16),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none(_: &[u8]) -> Option<String> {
        None
    }

    fn utf16(text: &str, little: bool) -> Vec<u8> {
        text.encode_utf16().flat_map(|u| if little { u.to_le_bytes() } else { u.to_be_bytes() }).collect()
    }

    #[test]
    fn marks_decide_first() {
        assert_eq!(detect(b"\xEF\xBB\xBFx"), Some(Detected { encoding: Encoding::Utf8, bom: 3 }));
        assert_eq!(detect(b"\xFF\xFEx\x00"), Some(Detected { encoding: Encoding::Utf16Le, bom: 2 }));
        assert_eq!(detect(b"\xFE\xFF\x00x"), Some(Detected { encoding: Encoding::Utf16Be, bom: 2 }));
        assert_eq!(detect(b"plain"), Some(Detected { encoding: Encoding::Utf8, bom: 0 }));
        assert_eq!(detect(b""), Some(Detected { encoding: Encoding::Utf8, bom: 0 }));
    }

    #[test]
    fn utf16_without_a_mark_is_told_by_its_nuls() {
        let le = utf16("İstanbul ılık bir şehir", true);
        let be = utf16("İstanbul ılık bir şehir", false);
        assert_eq!(detect(&le), Some(Detected { encoding: Encoding::Utf16Le, bom: 0 }));
        assert_eq!(detect(&be), Some(Detected { encoding: Encoding::Utf16Be, bom: 0 }));
        assert_eq!(decode(&le, none).as_deref(), Some("İstanbul ılık bir şehir"));
        assert_eq!(decode(&be, none).as_deref(), Some("İstanbul ılık bir şehir"));
    }

    #[test]
    fn binary_starts_are_not_text() {
        assert_eq!(detect(b"MZ\x90\x00\x03\x00"), None, "too short to be taken for UTF-16");
        let mut program = b"MZ\x90\x00\x03\x00\x00\x00\x04\x00\x00\x00\xFF\xFF\x00\x00".to_vec();
        program.extend([0u8; 64]);
        assert_eq!(detect(&program), None, "NULs in both halves of the pairs");
        assert_eq!(decode(b"a\x00b", none), None);
    }

    #[test]
    fn decoding_keeps_whole_characters() {
        assert_eq!(decode(b"Sat\xC4", none).as_deref(), Some("Sat"), "a character cut by the end");
        let mut odd = vec![0xFF, 0xFE];
        odd.extend(utf16("ab", true));
        odd.push(b'c');
        assert_eq!(decode(&odd, none).as_deref(), Some("ab"), "an odd last byte");
        assert_eq!(utf16_text(&utf16("x😀", true)[..4], Encoding::Utf16Le), "x", "half a surrogate pair");
        assert_eq!(decode(b"\xEF\xBB\xBFSat\xC4\xB1r", none).as_deref(), Some("Sat\u{131}r"));
    }

    #[test]
    fn text_that_is_not_utf8_goes_to_the_code_page() {
        let ansi = |bytes: &[u8]| Some(format!("ansi:{}", bytes.len()));
        assert_eq!(decode(b"Sat\xFDr x", ansi).as_deref(), Some("ansi:7"));
        assert_eq!(decode(b"Sat\xFDr", none).as_deref(), Some("Sat\u{FFFD}r"), "lossy without one");
    }
}
