//! Checksum files (spec 11 §4.2, §4.4): which names are ones, reading their lines (GNU, BSD,
//! SFV and Total Commander's `.crc`) and writing them (GNU with the binary mark, SFV, BSD).

use std::path::{Path, PathBuf};

/// A checksum algorithm Gezik computes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Algo {
    Crc32,
    Md5,
    Sha1,
    Sha256,
    Sha512,
}

impl Algo {
    /// In the order the layer's boxes and columns show them.
    pub const ALL: [Algo; 5] = [Algo::Crc32, Algo::Md5, Algo::Sha1, Algo::Sha256, Algo::Sha512];

    /// Its place in [`Algo::ALL`].
    pub fn index(self) -> usize {
        self as usize
    }

    /// What the layer calls it: `SHA-256`.
    pub fn name(self) -> &'static str {
        match self {
            Algo::Crc32 => "CRC32",
            Algo::Md5 => "MD5",
            Algo::Sha1 => "SHA-1",
            Algo::Sha256 => "SHA-256",
            Algo::Sha512 => "SHA-512",
        }
    }

    /// state.toml's name: `sha256`.
    pub fn key(self) -> &'static str {
        match self {
            Algo::Crc32 => "crc32",
            Algo::Md5 => "md5",
            Algo::Sha1 => "sha1",
            Algo::Sha256 => "sha256",
            Algo::Sha512 => "sha512",
        }
    }

    /// The ending of the file Save writes: CRC32 goes to an `.sfv`.
    pub fn extension(self) -> &'static str {
        match self {
            Algo::Crc32 => "sfv",
            other => other.key(),
        }
    }

    /// How many hex digits its value has.
    pub fn hex_len(self) -> usize {
        match self {
            Algo::Crc32 => 8,
            Algo::Md5 => 32,
            Algo::Sha1 => 40,
            Algo::Sha256 => 64,
            Algo::Sha512 => 128,
        }
    }

    /// The tag of a BSD line (`SHA256 (name) = …`), as `shasum --tag` and `md5` write it.
    pub fn bsd_tag(self) -> &'static str {
        match self {
            Algo::Crc32 => "CRC32",
            Algo::Md5 => "MD5",
            Algo::Sha1 => "SHA1",
            Algo::Sha256 => "SHA256",
            Algo::Sha512 => "SHA512",
        }
    }

    pub fn from_key(key: &str) -> Option<Algo> {
        Algo::ALL.into_iter().find(|a| a.key().eq_ignore_ascii_case(key))
    }

    /// The algorithm whose values have `len` hex digits (each length is one algorithm's).
    pub fn from_hex_len(len: usize) -> Option<Algo> {
        Algo::ALL.into_iter().find(|a| a.hex_len() == len)
    }

    fn from_bsd_tag(tag: &str) -> Option<Algo> {
        let tag = tag.trim();
        Algo::ALL.into_iter().find(|a| a.bsd_tag().eq_ignore_ascii_case(tag) || a.name().eq_ignore_ascii_case(tag))
    }
}

/// How a checksum file's lines are written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    /// GNU (`sha256sum`) or BSD (`shasum --tag`) lines.
    Sums,
    /// `name CRC32` lines, `;` comments.
    Sfv,
    /// Total Commander's split file record: `filename=`, `size=`, `crc32=`.
    Crc,
}

/// The digest names checksum files go by: `md5`, `sha256` …
const SUM_NAMES: [&str; 4] = ["md5", "sha1", "sha256", "sha512"];

/// `md5sum`, `sha256sums` … (lowercase).
fn sums_name(text: &str) -> bool {
    let text = text.strip_suffix('s').unwrap_or(text);
    text.strip_suffix("sum").is_some_and(|algo| SUM_NAMES.contains(&algo))
}

/// Whether `name` is a checksum file Verify Checksums reads: `.md5`, `.sha1`, `.sha256`,
/// `.sha512` (also `.sha256sum` …), `.sfv`, `.crc`, or `MD5SUM(S)` … `SHA512SUM(S)` (any case).
pub fn recognize(name: &str) -> Option<FileKind> {
    let lower = name.to_ascii_lowercase();
    if sums_name(&lower) {
        return Some(FileKind::Sums);
    }
    let (stem, ext) = lower.rsplit_once('.')?;
    if stem.is_empty() {
        return None;
    }
    match ext {
        ext if SUM_NAMES.contains(&ext) || sums_name(ext) => Some(FileKind::Sums),
        "sfv" => Some(FileKind::Sfv),
        "crc" => Some(FileKind::Crc),
        _ => None,
    }
}

/// One file a checksum file names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// As written (relative to the checksum file's folder, or absolute).
    pub path: String,
    pub algo: Algo,
    /// Lowercase hex.
    pub hex: String,
    /// The size it must have (only a `.crc` says).
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Parsed {
    pub entries: Vec<Entry>,
    /// Lines that are neither empty nor comments and could not be read.
    pub bad: usize,
}

/// A checksum file's bytes as text: UTF-16 with its BOM (Windows tools write it), else UTF-8
/// (a BOM is dropped by `parse`, a broken byte becomes U+FFFD).
pub fn decode(bytes: &[u8]) -> String {
    let utf16 = |rest: &[u8], le: bool| -> String {
        let units = rest
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&pair| if le { u16::from_le_bytes(pair) } else { u16::from_be_bytes(pair) });
        char::decode_utf16(units).map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER)).collect()
    };
    match bytes {
        [0xFF, 0xFE, rest @ ..] => utf16(rest, true),
        [0xFE, 0xFF, rest @ ..] => utf16(rest, false),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// Reads checksum file `name` from its bytes. A `.sha256` (…) holding only a value is that of
/// the file of the same name without the ending (`a.iso.sha256` → `a.iso`), as many download
/// pages give them.
pub fn parse_file(name: &str, bytes: &[u8]) -> Parsed {
    let kind = recognize(name).unwrap_or(FileKind::Sums);
    let text = decode(bytes);
    let parsed = parse(&text, kind);
    let mut lines = text.trim_start_matches('\u{feff}').lines().map(str::trim).filter(|l| !l.is_empty());
    if let (Some(line), None) = (lines.next(), lines.next())
        && parsed.entries.is_empty()
        && kind == FileKind::Sums
        && let Some((stem, _)) = name.rsplit_once('.').filter(|(stem, _)| !stem.is_empty())
        && let Some(algo) = Algo::from_hex_len(line.len())
        && let Some(hex) = hex_of(line, algo)
    {
        return Parsed { entries: vec![Entry { path: stem.to_owned(), algo, hex, size: None }], bad: 0 };
    }
    parsed
}

/// Reads a checksum file's text (a UTF-8 BOM and CRLF line ends are fine).
pub fn parse(text: &str, kind: FileKind) -> Parsed {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut parsed = Parsed::default();
    if kind == FileKind::Crc {
        return parse_crc(text);
    }
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.trim().is_empty() || (kind == FileKind::Sfv && line.starts_with(';')) {
            continue;
        }
        let entry = match kind {
            FileKind::Sfv => sfv_line(line),
            _ => bsd_line(line).or_else(|| gnu_line(line)),
        };
        match entry {
            Some(entry) => parsed.entries.push(entry),
            None => parsed.bad += 1,
        }
    }
    parsed
}

/// `hex` in lowercase if it is `algo`'s length and only hex digits.
fn hex_of(hex: &str, algo: Algo) -> Option<String> {
    (hex.len() == algo.hex_len() && hex.bytes().all(|b| b.is_ascii_hexdigit())).then(|| hex.to_ascii_lowercase())
}

/// `<hex>  <path>` (text mode) or `<hex> *<path>` (binary); a line that starts with `\` has
/// `\\` and `\n` in its path.
fn gnu_line(line: &str) -> Option<Entry> {
    let (escaped, line) = match line.strip_prefix('\\') {
        Some(rest) => (true, rest),
        None => (false, line),
    };
    let (hex, rest) = line.split_once(' ')?;
    let path = rest.strip_prefix(' ').or_else(|| rest.strip_prefix('*'))?;
    if path.is_empty() {
        return None;
    }
    let algo = Algo::from_hex_len(hex.len())?;
    let hex = hex_of(hex, algo)?;
    let path = if escaped { unescape(path)? } else { path.to_owned() };
    Some(Entry { path, algo, hex, size: None })
}

/// GNU's escapes: `\\` and `\n` (and `\r`, which newer coreutils write).
fn unescape(path: &str) -> Option<String> {
    let mut out = String::with_capacity(path.len());
    let mut chars = path.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next()? {
            '\\' => out.push('\\'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            _ => return None,
        }
    }
    Some(out)
}

/// `SHA256 (<path>) = <hex>`; the path may hold `) = ` itself, so the last one ends it.
fn bsd_line(line: &str) -> Option<Entry> {
    let line = line.strip_prefix('\\').unwrap_or(line);
    let (tag, rest) = line.split_once(" (")?;
    let algo = Algo::from_bsd_tag(tag)?;
    let (path, hex) = rest.rsplit_once(") = ")?;
    let hex = hex_of(hex.trim(), algo)?;
    (!path.is_empty()).then(|| Entry { path: path.to_owned(), algo, hex, size: None })
}

/// `<path> <CRC32>` (the path may hold spaces: the last word is the value).
fn sfv_line(line: &str) -> Option<Entry> {
    let line = line.trim_end();
    let (path, hex) = line.rsplit_once([' ', '\t'])?;
    let path = path.trim_end();
    let hex = hex_of(hex, Algo::Crc32)?;
    (!path.is_empty()).then(|| Entry { path: path.to_owned(), algo: Algo::Crc32, hex, size: None })
}

/// Total Commander's `.crc`: one file, its size and CRC32.
fn parse_crc(text: &str) -> Parsed {
    let (mut path, mut size, mut hex) = (None, None, None);
    let mut bad = 0;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            bad += 1;
            continue;
        };
        match key.trim().to_ascii_lowercase().as_str() {
            "filename" => path = Some(value.trim().to_owned()).filter(|p| !p.is_empty()),
            "size" => size = value.trim().parse::<u64>().ok(),
            "crc32" => hex = hex_of(value.trim(), Algo::Crc32),
            _ => {}
        }
    }
    match (path, hex) {
        (Some(path), Some(hex)) => Parsed { entries: vec![Entry { path, algo: Algo::Crc32, hex, size }], bad },
        _ => Parsed { entries: Vec::new(), bad: bad.max(1) },
    }
}

/// GNU lines with the binary mark (`<hex> *<path>`), LF ends: `sha256sum -c` and
/// `shasum -a 256 -c` read them. A path with `\` or a line break is escaped GNU's way.
pub fn write_gnu<'a>(rows: impl IntoIterator<Item = (&'a str, &'a str)>) -> String {
    let mut out = String::new();
    for (path, hex) in rows {
        if path.contains(['\\', '\n', '\r']) {
            let escaped = path.replace('\\', "\\\\").replace('\n', "\\n").replace('\r', "\\r");
            out.push_str(&format!("\\{hex} *{escaped}\n"));
        } else {
            out.push_str(&format!("{hex} *{path}\n"));
        }
    }
    out
}

/// SFV lines: `<path> <CRC32 in capitals>`. A name starting with `;` would read as a comment:
/// it is written `./;name`, the same file.
pub fn write_sfv<'a>(rows: impl IntoIterator<Item = (&'a str, &'a str)>) -> String {
    rows.into_iter()
        .map(|(path, hex)| {
            let lead = if path.starts_with(';') { "./" } else { "" };
            format!("{lead}{path} {}\n", hex.to_ascii_uppercase())
        })
        .collect()
}

/// BSD lines (`SHA256 (<path>) = <hex>`): several algorithms in one text.
pub fn write_bsd<'a>(rows: impl IntoIterator<Item = (Algo, &'a str, &'a str)>) -> String {
    rows.into_iter().map(|(algo, path, hex)| format!("{} ({path}) = {hex}\n", algo.bsd_tag())).collect()
}

/// A typed value of a known length (spaces around it are fine): its algorithm and the value
/// in lowercase.
pub fn typed_hex(text: &str) -> Option<(Algo, String)> {
    let text = text.trim();
    let algo = Algo::from_hex_len(text.len())?;
    hex_of(text, algo).map(|hex| (algo, hex))
}

/// The clipboard's text taken as the expected value without asking: only an MD5 or longer
/// (32+ digits) with a letter in it, so a date (`20261011`), a number or `deadbeef` never fills
/// it. A dashless UUID (32) still reads as MD5 and a git commit id (40) as SHA-1: the layer then
/// just says no file matches.
pub fn clipboard_hex(text: &str) -> Option<(Algo, String)> {
    typed_hex(text).filter(|(algo, hex)| algo.hex_len() >= 32 && hex.bytes().any(|b| b.is_ascii_alphabetic()))
}

/// `path` relative to `base` with `/` between its parts, as a checksum file writes it; the
/// whole path when it is not under `base`.
pub fn relative_slash(path: &Path, base: &Path) -> String {
    match path.strip_prefix(base) {
        Ok(relative) if !relative.as_os_str().is_empty() => {
            relative.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/")
        }
        _ => path.to_string_lossy().into_owned(),
    }
}

/// Where a checksum file's `path` is, in the file's folder `base`; on Windows `/` and `\` both
/// separate. `None` for a path that leaves the folder: absolute, a drive or a share (a
/// `\\host\x` read sends the user's NTLM hash to that host), or with `..` (spec 11 §4.4).
pub fn resolve(base: &Path, path: &str) -> Option<PathBuf> {
    use std::path::Component;
    let path = if cfg!(windows) { path.replace('/', "\\") } else { path.to_owned() };
    let relative = Path::new(&path);
    let inside = relative.components().all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
    (inside && relative.components().next().is_some()).then(|| base.join(relative))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA256_ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    const MD5_ABC: &str = "900150983cd24fb0d6963f7d28e17f72";

    fn entry(path: &str, algo: Algo, hex: &str) -> Entry {
        Entry { path: path.into(), algo, hex: hex.into(), size: None }
    }

    #[test]
    fn names_and_endings_are_recognized_in_any_case() {
        assert_eq!(recognize("ubuntu.iso.SHA256"), Some(FileKind::Sums));
        assert_eq!(recognize("a.md5"), Some(FileKind::Sums));
        assert_eq!(recognize("SHA256SUMS"), Some(FileKind::Sums));
        assert_eq!(recognize("sha512sums"), Some(FileKind::Sums));
        assert_eq!(recognize("disc.SFV"), Some(FileKind::Sfv));
        assert_eq!(recognize("film.mkv.crc"), Some(FileKind::Crc));
        assert_eq!(recognize(".sha256"), None, "no name before the ending");
        assert_eq!(recognize("sha256.txt"), None);
        assert_eq!(recognize("SHA256SUMS.gpg"), None);
        assert_eq!(Algo::from_key("SHA256"), Some(Algo::Sha256));
        assert_eq!(Algo::Crc32.extension(), "sfv");
        assert_eq!(Algo::ALL.map(Algo::index), [0, 1, 2, 3, 4]);
    }

    #[test]
    fn gnu_lines_in_both_modes_with_bom_and_crlf() {
        let text = format!("\u{feff}{SHA256_ABC}  a b.txt\r\n{MD5_ABC} *sub/c.bin\r\n\r\n");
        let parsed = parse(&text, FileKind::Sums);
        assert_eq!(parsed.bad, 0);
        assert_eq!(
            parsed.entries,
            [entry("a b.txt", Algo::Sha256, SHA256_ABC), entry("sub/c.bin", Algo::Md5, MD5_ABC)]
        );
    }

    #[test]
    fn escaped_gnu_lines_and_bsd_lines() {
        let text = format!(
            "\\{SHA256_ABC} *dir\\\\x\\ny\nSHA256 (we) = ird) = {SHA256_ABC}\nMD5 (m.txt) = {}\n",
            MD5_ABC.to_uppercase()
        );
        let parsed = parse(&text, FileKind::Sums);
        assert_eq!(parsed.bad, 0);
        assert_eq!(
            parsed.entries,
            [
                entry("dir\\x\ny", Algo::Sha256, SHA256_ABC),
                entry("we) = ird", Algo::Sha256, SHA256_ABC),
                entry("m.txt", Algo::Md5, MD5_ABC),
            ]
        );
    }

    #[test]
    fn broken_lines_are_counted_and_left_out() {
        let text = format!(
            "{SHA256_ABC}  ok.txt\nnot a line\n{}  short.txt\n{SHA256_ABC}x  long.txt\n{SHA256_ABC}\nSHA256 (x) = {MD5_ABC}\n\\{SHA256_ABC}  bad\\escape\n",
            &SHA256_ABC[..60]
        );
        let parsed = parse(&text, FileKind::Sums);
        assert_eq!(parsed.entries, [entry("ok.txt", Algo::Sha256, SHA256_ABC)]);
        assert_eq!(parsed.bad, 6);
    }

    #[test]
    fn relative_parent_and_absolute_paths_are_kept() {
        let text = format!("{MD5_ABC}  ../up.txt\n{MD5_ABC}  /etc/abs.txt\n{MD5_ABC}  C:\\w\\x.txt\n");
        let paths: Vec<String> = parse(&text, FileKind::Sums).entries.into_iter().map(|e| e.path).collect();
        assert_eq!(paths, ["../up.txt", "/etc/abs.txt", "C:\\w\\x.txt"]);
        let base = Path::new("base");
        assert_eq!(resolve(base, "../up.txt"), None, "it leaves the folder");
        assert_eq!(resolve(base, "a/../../x"), None);
        assert_eq!(resolve(base, "/etc/abs.txt"), None);
        assert_eq!(resolve(base, ""), None);
        assert_eq!(resolve(base, "sub/x.txt"), Some(base.join("sub").join("x.txt")));
        assert_eq!(resolve(base, ";x"), Some(base.join(";x")));
        if cfg!(windows) {
            for outside in ["C:\\w\\x.txt", "\\\\host\\share\\x", "C:x", "\\x", "//host/share/x", "x:stream"] {
                assert_eq!(resolve(base, outside), None, "{outside}");
            }
        }
    }

    #[test]
    fn sfv_lines_and_comments() {
        let text = "; made by something\r\nfile one.bin CBF43926\nsecond.txt\t00000000\nbad line\n";
        let parsed = parse(text, FileKind::Sfv);
        assert_eq!(
            parsed.entries,
            [entry("file one.bin", Algo::Crc32, "cbf43926"), entry("second.txt", Algo::Crc32, "00000000")]
        );
        assert_eq!(parsed.bad, 1);
    }

    #[test]
    fn total_commanders_crc_names_one_file_with_its_size() {
        let parsed = parse("filename=film.mkv\r\nsize=1234\r\ncrc32=CBF43926\r\n", FileKind::Crc);
        assert_eq!(
            parsed.entries,
            [Entry { path: "film.mkv".into(), algo: Algo::Crc32, hex: "cbf43926".into(), size: Some(1234) }]
        );
        assert_eq!(parsed.bad, 0);
        assert_eq!(parse("size=3\n", FileKind::Crc), Parsed { entries: Vec::new(), bad: 1 });
    }

    #[test]
    fn written_files_read_back() {
        let rows = [("a.txt", SHA256_ABC), ("sub dir/b c.txt", SHA256_ABC), ("odd\\name", SHA256_ABC)];
        let gnu = write_gnu(rows);
        assert!(gnu.starts_with(&format!("{SHA256_ABC} *a.txt\n")), "{gnu}");
        assert!(gnu.contains(&format!("\\{SHA256_ABC} *odd\\\\name\n")), "{gnu}");
        let back: Vec<(String, String)> =
            parse(&gnu, FileKind::Sums).entries.into_iter().map(|e| (e.path, e.hex)).collect();
        let want: Vec<(String, String)> = rows.iter().map(|(p, h)| ((*p).to_owned(), (*h).to_owned())).collect();
        assert_eq!(back, want);
        let sfv = write_sfv([("x y.bin", "cbf43926")]);
        assert_eq!(sfv, "x y.bin CBF43926\n");
        assert_eq!(parse(&sfv, FileKind::Sfv).entries, [entry("x y.bin", Algo::Crc32, "cbf43926")]);
        let bsd = write_bsd([(Algo::Md5, "a.txt", MD5_ABC), (Algo::Sha256, "a.txt", SHA256_ABC)]);
        assert_eq!(bsd, format!("MD5 (a.txt) = {MD5_ABC}\nSHA256 (a.txt) = {SHA256_ABC}\n"));
        assert_eq!(parse(&bsd, FileKind::Sums).entries.len(), 2);
    }

    #[test]
    fn the_clipboard_gives_a_value_of_a_known_length() {
        assert_eq!(
            clipboard_hex(&format!("  {}\n", SHA256_ABC.to_uppercase())),
            Some((Algo::Sha256, SHA256_ABC.into()))
        );
        assert_eq!(clipboard_hex(MD5_ABC).map(|(a, _)| a), Some(Algo::Md5));
        assert_eq!(clipboard_hex(&"b".repeat(128)).map(|(a, _)| a), Some(Algo::Sha512));
        assert_eq!(clipboard_hex(&"a".repeat(33)), None);
        assert_eq!(clipboard_hex(&format!("{MD5_ABC} {MD5_ABC}")), None, "two values");
        assert_eq!(clipboard_hex(""), None);
        // Never a CRC32 or anything without a letter: a date, a number, a word.
        for text in ["20261011", "deadbeef", "CBF43926", &"1".repeat(32), &"7".repeat(40)] {
            assert_eq!(clipboard_hex(text), None, "{text}");
        }
        // Taken as they read (decided: no way to tell them apart): a dashless UUID as MD5, a
        // git commit id as SHA-1.
        assert_eq!(clipboard_hex("3f2b8c1e9d4a4b7e8f0a1c2d3e4f5a6b").map(|(a, _)| a), Some(Algo::Md5));
        assert_eq!(clipboard_hex("e78236d9a1b2c3d4e5f60718293a4b5c6d7e8f90").map(|(a, _)| a), Some(Algo::Sha1));
        // Typed into the field, any known length counts.
        assert_eq!(typed_hex("CBF43926"), Some((Algo::Crc32, "cbf43926".into())));
        assert_eq!(typed_hex("cbf4392g"), None);
    }

    #[test]
    fn utf16_files_bare_values_and_more_names() {
        let text = format!("{MD5_ABC}  a.txt\r\n");
        let le: Vec<u8> = [0xFF, 0xFE].into_iter().chain(text.encode_utf16().flat_map(u16::to_le_bytes)).collect();
        let be: Vec<u8> = [0xFE, 0xFF].into_iter().chain(text.encode_utf16().flat_map(u16::to_be_bytes)).collect();
        for bytes in [le, be] {
            assert_eq!(parse_file("x.md5", &bytes).entries, [entry("a.txt", Algo::Md5, MD5_ABC)]);
        }
        let bare = parse_file("ubuntu.iso.sha256", format!("{}\n", SHA256_ABC.to_uppercase()).as_bytes());
        assert_eq!(bare, Parsed { entries: vec![entry("ubuntu.iso", Algo::Sha256, SHA256_ABC)], bad: 0 });
        assert_eq!(parse_file("SHA256SUMS", SHA256_ABC.as_bytes()).bad, 1, "no name to take it from");
        for name in ["a.sha256sum", "a.MD5SUM", "a.sha1sum", "MD5SUM", "SHA256SUM", "sha512sums"] {
            assert_eq!(recognize(name), Some(FileKind::Sums), "{name}");
        }
        assert_eq!(recognize("checksum"), None);
        assert_eq!(recognize("a.sum"), None);
    }

    #[test]
    fn an_sfv_name_starting_with_a_semicolon_is_kept_from_reading_as_a_comment() {
        let sfv = write_sfv([(";odd.bin", "cbf43926")]);
        assert_eq!(sfv, "./;odd.bin CBF43926\n");
        assert_eq!(parse(&sfv, FileKind::Sfv).entries, [entry("./;odd.bin", Algo::Crc32, "cbf43926")]);
        assert_eq!(resolve(Path::new("b"), "./;odd.bin"), Some(Path::new("b").join(";odd.bin")));
    }

    #[test]
    fn relative_paths_use_slashes() {
        let base = Path::new("root");
        assert_eq!(relative_slash(&base.join("a").join("b.txt"), base), "a/b.txt");
        assert_eq!(relative_slash(&base.join("c.txt"), base), "c.txt");
        let elsewhere = Path::new("other").join("d.txt");
        assert_eq!(relative_slash(&elsewhere, base), elsewhere.to_string_lossy());
    }
}
