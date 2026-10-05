//! Converting text: encodings (detected and given), BOMs, UTF-16 output, line endings,
//! characters the target cannot hold and bytes the source does not allow (the file fails
//! with the line and nothing is written), binary files, stopping, and streaming large files.

use std::io;
use std::path::{Path, PathBuf};

use encoding_rs::{UTF_8, WINDOWS_1254};
use gezik_batch::convert::text::{self, Detected, Malformed, Unmappable, convert_text, is_binary};
use gezik_core::batch::convert::{Eol, TextOptions};

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("gezik-convert-text-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn options(to: &str) -> TextOptions {
    TextOptions {
        from: None,
        to: to.to_owned(),
        bom: false,
        eol: Eol::Keep,
        trim_trailing: false,
        final_newline: false,
    }
}

fn never() -> bool {
    false
}

/// Writes `bytes` as the input, converts, and answers the output's bytes.
fn run(d: &Path, bytes: &[u8], options: &TextOptions) -> io::Result<Vec<u8>> {
    let input = d.join("in.txt");
    let output = d.join("out.tmp");
    std::fs::write(&input, bytes).unwrap();
    let _ = std::fs::remove_file(&output);
    let result = convert_text(&input, &output, options, &never);
    match result {
        Ok(()) => Ok(std::fs::read(&output).unwrap()),
        Err(e) => {
            assert!(!output.exists(), "output left behind after: {e}");
            Err(e)
        }
    }
}

fn inner<T: std::error::Error + 'static>(err: &io::Error) -> &T {
    err.get_ref().and_then(|e| e.downcast_ref::<T>()).unwrap_or_else(|| panic!("unexpected error: {err}"))
}

const TURKISH: &str = "Işık ılık süt içer.\r\nÇağrı, şöyle güzel ağaçlar gördü.\r\nĞĞ ÜÜ ŞŞ İİ ÖÖ ÇÇ\r\n";

fn cp1254(s: &str) -> Vec<u8> {
    let (bytes, _, unmappable) = WINDOWS_1254.encode(s);
    assert!(!unmappable);
    bytes.into_owned()
}

#[test]
fn windows_1254_turkish_to_utf8_exact_bytes() {
    let d = dir("1254");
    let source = cp1254(TURKISH);
    assert_eq!(text::detect(&source), Detected::Text(WINDOWS_1254, false));
    assert_eq!(run(&d, &source, &options("UTF-8")).unwrap(), TURKISH.as_bytes());
    // Given by hand, by its ISO name.
    let mut given = options("utf-8");
    given.from = Some("ISO-8859-9".into());
    assert_eq!(run(&d, &source, &given).unwrap(), TURKISH.as_bytes());
    // With a BOM.
    let mut bom = options("UTF-8");
    bom.bom = true;
    assert_eq!(run(&d, &source, &bom).unwrap(), [b"\xEF\xBB\xBF", TURKISH.as_bytes()].concat());
    // And back.
    assert_eq!(run(&d, TURKISH.as_bytes(), &options("windows-1254")).unwrap(), source);
}

#[test]
fn unmappable_character_fails_with_char_and_line_and_writes_nothing() {
    let d = dir("unmappable");
    let source = "first line\nsecond\r\nthird\rfourth has ş here\n";
    let err = run(&d, source.as_bytes(), &options("windows-1252")).unwrap_err();
    assert_eq!(inner::<Unmappable>(&err), &Unmappable { ch: 'ş', line: 4, encoding: "Windows-1252" });
    assert_eq!(err.to_string(), "can't encode 'ş' in Windows-1252 (line 4)");
    // The line is the output's: blank lines dropped by the final newline do not count…
    let mut final_newline = options("windows-1252");
    final_newline.final_newline = true;
    let err = run(&d, "a\n\n\nb ğ\n".as_bytes(), &final_newline).unwrap_err();
    assert_eq!(inner::<Unmappable>(&err).line, 4);
    // …and the target has no ISO-8859-9 alias for Turkish letters it lacks.
    let err = run(&d, "Ş".as_bytes(), &options("iso-8859-1")).unwrap_err();
    assert_eq!(err.to_string(), "can't encode 'Ş' in Windows-1252 (line 1)");
}

#[test]
fn malformed_source_fails_with_the_line() {
    let d = dir("malformed");
    let mut utf8 = options("windows-1254");
    utf8.from = Some("UTF-8".into());
    let err = run(&d, b"ok\r\nbad \xFF here\n", &utf8).unwrap_err();
    assert_eq!(inner::<Malformed>(&err), &Malformed { line: 2, encoding: "UTF-8" });
    assert_eq!(err.to_string(), "not valid UTF-8 text (line 2)");
    // A CR just before the bad byte has ended the line.
    let err = run(&d, b"a\r\xFF", &utf8).unwrap_err();
    assert_eq!(inner::<Malformed>(&err).line, 2);
    // A sequence cut off by the end of the file.
    let err = run(&d, b"x\ny\n\xC5", &utf8).unwrap_err();
    assert_eq!(inner::<Malformed>(&err).line, 3);
}

#[test]
fn utf16_output_by_hand_with_and_without_bom() {
    let d = dir("utf16");
    let mut le = options("UTF-16LE");
    le.bom = true;
    let expected: Vec<u8> =
        [0xFF, 0xFE].into_iter().chain("Işık\n😀".encode_utf16().flat_map(u16::to_le_bytes)).collect();
    assert_eq!(run(&d, "Işık\n😀".as_bytes(), &le).unwrap(), expected);
    // Read back: the BOM is detected and dropped.
    assert_eq!(run(&d, &expected, &options("UTF-8")).unwrap(), "Işık\n😀".as_bytes());

    let be = options("utf-16be");
    let expected: Vec<u8> = "Işık".encode_utf16().flat_map(u16::to_be_bytes).collect();
    assert_eq!(run(&d, &cp1254("Işık"), &be).unwrap(), expected);
    // BOM-less UTF-16 looks binary unless its encoding is given.
    let err = run(&d, &expected, &options("UTF-8")).unwrap_err();
    assert!(is_binary(&err));
    let mut given = options("UTF-8");
    given.from = Some("UTF-16BE".into());
    assert_eq!(run(&d, &expected, &given).unwrap(), "Işık".as_bytes());
}

#[test]
fn line_endings_keep_the_encoding() {
    let d = dir("eol");
    let source = cp1254(TURKISH);
    let mut lf = options("");
    lf.eol = Eol::Lf;
    assert_eq!(run(&d, &source, &lf).unwrap(), cp1254(&TURKISH.replace("\r\n", "\n")));

    let mut crlf = options("");
    crlf.eol = Eol::Crlf;
    crlf.trim_trailing = true;
    crlf.final_newline = true;
    assert_eq!(run(&d, b"a \nb\t\rc\n\n\n", &crlf).unwrap(), b"a\r\nb\r\nc\r\n");

    // A UTF-8 file with a BOM keeps it; UTF-16 stays UTF-16.
    let mut keep = options("");
    keep.eol = Eol::Cr;
    assert_eq!(run(&d, "\u{FEFF}ş\r\nx".as_bytes(), &keep).unwrap(), "\u{FEFF}ş\rx".as_bytes());
    let utf16: Vec<u8> = [0xFE, 0xFF].into_iter().chain("a\nb".encode_utf16().flat_map(u16::to_be_bytes)).collect();
    let crlf16: Vec<u8> = [0xFE, 0xFF].into_iter().chain("a\r\nb".encode_utf16().flat_map(u16::to_be_bytes)).collect();
    let mut to_crlf = options("");
    to_crlf.eol = Eol::Crlf;
    assert_eq!(run(&d, &utf16, &to_crlf).unwrap(), crlf16);
}

#[test]
fn binary_files_are_skipped() {
    let d = dir("binary");
    let mut bytes = b"MZ\x90\0\x03\0\0\0".to_vec();
    bytes.extend_from_slice(&[0x41; 100]);
    assert_eq!(text::detect(&bytes), Detected::Binary);
    let err = run(&d, &bytes, &options("UTF-8")).unwrap_err();
    assert!(is_binary(&err));
    assert_eq!(err.to_string(), "looks binary");
    // Also when a single-byte source encoding is given.
    let mut given = options("UTF-8");
    given.from = Some("windows-1254".into());
    assert!(is_binary(&run(&d, &bytes, &given).unwrap_err()));
    // A NUL after the first 8 KB is not looked for.
    let mut late = vec![b'a'; 9000];
    late.push(0);
    assert_eq!(run(&d, &late, &options("UTF-8")).unwrap(), late);
}

#[test]
fn unknown_encodings_and_empty_files() {
    let d = dir("labels");
    let err = run(&d, b"x", &options("klingon")).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    assert_eq!(run(&d, b"", &options("UTF-16LE")).unwrap(), b"");
    let mut final_newline = options("");
    final_newline.final_newline = true;
    assert_eq!(run(&d, b"", &final_newline).unwrap(), b"");
    assert_eq!(text::detect(b""), Detected::Text(UTF_8, false));
}

#[test]
fn stop_leaves_no_output() {
    let d = dir("stop");
    let input = d.join("in.txt");
    let output = d.join("out.tmp");
    std::fs::write(&input, "line\n".repeat(100_000)).unwrap();
    let calls = std::cell::Cell::new(0);
    let stop = || {
        calls.set(calls.get() + 1);
        calls.get() > 2
    };
    let err = convert_text(&input, &output, &options("UTF-16LE"), &stop).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::Interrupted);
    assert!(!output.exists());
}

/// A Windows-1254 file of about `mb` MB with CRLF endings, so CRs and Turkish letters fall
/// on every 64 KB read boundary somewhere.
fn big_1254(mb: usize) -> (Vec<u8>, String) {
    let mut text = String::new();
    let mut i = 0u64;
    while text.len() < mb * 1024 * 1024 {
        text.push_str(&format!("{i} Işık ılık süt içer, çağrı şöyle güzel. ĞÜŞİÖÇ\r\n"));
        i += 1;
    }
    (cp1254(&text), text)
}

fn streams(mb: usize) {
    let d = dir(&format!("big{mb}"));
    let (source, text) = big_1254(mb);
    let mut lf = options("UTF-8");
    lf.eol = Eol::Lf;
    let out = run(&d, &source, &lf).unwrap();
    assert!(out == text.replace("\r\n", "\n").as_bytes());

    let mut le = options("UTF-16LE");
    le.bom = true;
    let out = run(&d, &source, &le).unwrap();
    assert_eq!(out.len(), 2 + text.encode_utf16().count() * 2);

    // A character Windows-1252 lacks, deep in the file.
    let lines = text.matches('\n').count() as u64;
    let mut utf8 = text.into_bytes();
    utf8.extend_from_slice("x\ny\nlast ş\n".as_bytes());
    let err = run(&d, &utf8, &options("windows-1252")).unwrap_err();
    let unmappable = inner::<Unmappable>(&err);
    // The first unmappable character is the first line's 'ş' ("Işık").
    assert_eq!((unmappable.ch, unmappable.line), ('ş', 1));
    let tail = format!("{}x\ny\nlast ş\n", "plain line\r\n".repeat(lines as usize));
    let err = run(&d, tail.as_bytes(), &options("windows-1252")).unwrap_err();
    assert_eq!(inner::<Unmappable>(&err), &Unmappable { ch: 'ş', line: lines + 3, encoding: "Windows-1252" });
}

#[test]
fn streams_a_5_mb_file() {
    streams(5);
}

#[test]
#[ignore = "100 MB on disk; run with --ignored"]
fn streams_a_100_mb_file() {
    streams(100);
}
