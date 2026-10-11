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

/// Whether `name` is a checksum file Verify Checksums reads: `.md5`, `.sha1`, `.sha256`,
/// `.sha512`, `.sfv`, `.crc`, or `MD5SUMS` … `SHA512SUMS` (any case).
pub fn recognize(name: &str) -> Option<FileKind> {
    let lower = name.to_ascii_lowercase();
    if matches!(lower.as_str(), "md5sums" | "sha1sums" | "sha256sums" | "sha512sums") {
        return Some(FileKind::Sums);
    }
    let (stem, ext) = lower.rsplit_once('.')?;
    if stem.is_empty() {
        return None;
    }
    match ext {
        "md5" | "sha1" | "sha256" | "sha512" => Some(FileKind::Sums),
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

/// SFV lines: `<path> <CRC32 in capitals>`.
pub fn write_sfv<'a>(rows: impl IntoIterator<Item = (&'a str, &'a str)>) -> String {
    rows.into_iter().map(|(path, hex)| format!("{path} {}\n", hex.to_ascii_uppercase())).collect()
}

/// BSD lines (`SHA256 (<path>) = <hex>`): several algorithms in one text.
pub fn write_bsd<'a>(rows: impl IntoIterator<Item = (Algo, &'a str, &'a str)>) -> String {
    rows.into_iter().map(|(algo, path, hex)| format!("{} ({path}) = {hex}\n", algo.bsd_tag())).collect()
}

/// The clipboard's text when it is one hex value of a known length (spaces around it are
/// fine): its algorithm and the value in lowercase.
pub fn clipboard_hex(text: &str) -> Option<(Algo, String)> {
    let text = text.trim();
    let algo = Algo::from_hex_len(text.len())?;
    hex_of(text, algo).map(|hex| (algo, hex))
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

/// Where a checksum file's `path` is: relative to the file's folder `base` (absolute paths and
/// `..` are kept, as GNU does); on Windows `/` and `\` both separate.
pub fn resolve(base: &Path, path: &str) -> PathBuf {
    if cfg!(windows) { base.join(path.replace('/', "\\")) } else { base.join(path) }
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
        assert_eq!(resolve(base, "../up.txt"), base.join(if cfg!(windows) { "..\\up.txt" } else { "../up.txt" }));
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
        assert_eq!(clipboard_hex("CBF43926"), Some((Algo::Crc32, "cbf43926".into())));
        assert_eq!(clipboard_hex(MD5_ABC).map(|(a, _)| a), Some(Algo::Md5));
        assert_eq!(clipboard_hex(&"a".repeat(40)).map(|(a, _)| a), Some(Algo::Sha1));
        assert_eq!(clipboard_hex(&"b".repeat(128)).map(|(a, _)| a), Some(Algo::Sha512));
        assert_eq!(clipboard_hex(&"a".repeat(33)), None);
        assert_eq!(clipboard_hex("cbf4392g"), None);
        assert_eq!(clipboard_hex(&format!("{MD5_ABC} {MD5_ABC}")), None, "two values");
        assert_eq!(clipboard_hex(""), None);
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
