//! New names (`rapor (2).pdf`) and whether a typed name is valid.

use std::ffi::{OsStr, OsString};
use std::fmt;

/// Endings kept together when a number goes in: `arsiv (2).tar.gz`, not `arsiv.tar (2).gz`.
const DOUBLE_EXTENSIONS: [&str; 4] = [".tar.gz", ".tar.bz2", ".tar.xz", ".tar.zst"];

/// `name` split into the part a number goes after and the rest: `("rapor", ".pdf")`,
/// `("arsiv", ".tar.gz")`, `("Makefile", "")`, `(".gitignore", "")`. A folder is all stem
/// (`v1.2`).
pub fn split_name(name: &str, is_dir: bool) -> (&str, &str) {
    if is_dir {
        return (name, "");
    }
    // ASCII lowercasing keeps every byte where it is, so offsets carry over to `name`.
    let lower = name.to_ascii_lowercase();
    for ext in DOUBLE_EXTENSIONS {
        if lower.len() > ext.len() && lower.ends_with(ext) {
            let at = name.len() - ext.len();
            return (&name[..at], &name[at..]);
        }
    }
    match name.rfind('.') {
        None | Some(0) => (name, ""),
        Some(at) => (&name[..at], &name[at..]),
    }
}

/// `stem` without a trailing ` (n)` (n ≥ 2, no leading zero) and that n; 1 if it has none.
fn strip_number(stem: &str) -> (&str, u32) {
    if let Some(inner) = stem.strip_suffix(')')
        && let Some(open) = inner.rfind(" (")
    {
        let digits = &inner[open + 2..];
        if !digits.starts_with('0')
            && let Ok(n) = digits.parse::<u32>()
            && n >= 2
        {
            return (&inner[..open], n);
        }
    }
    (stem, 1)
}

/// `name` with the number `n`: `rapor.pdf`, 2 → `rapor (2).pdf`; `rapor (2).pdf`, 3 →
/// `rapor (3).pdf`.
pub fn numbered(name: &str, is_dir: bool, n: u32) -> String {
    let (stem, ext) = split_name(name, is_dir);
    let (base, _) = strip_number(stem);
    format!("{base} ({n}){ext}")
}

/// The first of `name (2)`, `name (3)`… that `taken` does not claim. A name that already ends
/// in `(n)` goes on from n + 1. `name` itself counts as taken.
pub fn next_free(name: &str, is_dir: bool, taken: impl Fn(&str) -> bool) -> String {
    let (stem, ext) = split_name(name, is_dir);
    let (base, from) = strip_number(stem);
    let mut n = from.saturating_add(1);
    loop {
        let candidate = format!("{base} ({n}){ext}");
        if !taken(&candidate) || n == u32::MAX {
            return candidate;
        }
        n += 1;
    }
}

/// [`next_free`] on an OS name, which keeps every byte of a name that is not Unicode (a Linux
/// name in another encoding, a Windows name with a lone surrogate): such a name gets ` (n)`
/// before its last extension (`<stem> (2).<ext>`).
pub fn next_free_os(name: &OsStr, is_dir: bool, taken: impl Fn(&OsStr) -> bool) -> OsString {
    if let Some(text) = name.to_str() {
        return OsString::from(next_free(text, is_dir, |candidate| taken(OsStr::new(candidate))));
    }
    let path = std::path::Path::new(name);
    let (stem, ext) = match (is_dir, path.file_stem(), path.extension()) {
        (false, Some(stem), Some(ext)) => (stem, Some(ext)),
        _ => (name, None),
    };
    let mut n = 2u32;
    loop {
        let mut candidate = stem.to_os_string();
        candidate.push(format!(" ({n})"));
        if let Some(ext) = ext {
            candidate.push(".");
            candidate.push(ext);
        }
        if !taken(&candidate) || n == u32::MAX {
            return candidate;
        }
        n += 1;
    }
}

/// What to select when renaming starts: the name without its extension (bytes, for Slint's
/// `set-selection-offsets`); all of it for folders and names like `.gitignore`.
pub fn rename_selection(name: &str, is_dir: bool) -> (usize, usize) {
    let (stem, _) = split_name(name, is_dir);
    (0, stem.len())
}

/// Which file system rules a name must follow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameRules {
    Windows,
    Unix,
}

impl NameRules {
    pub fn current() -> NameRules {
        if cfg!(windows) { NameRules::Windows } else { NameRules::Unix }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameError {
    Empty,
    Dots,
    Character(char),
    /// A Windows device name (`CON`, `COM1`…), in capitals.
    Reserved(String),
    TrailingDotOrSpace,
    TooLong,
}

impl fmt::Display for NameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NameError::Empty => write!(f, "Type a name"),
            NameError::Dots => write!(f, "\".\" and \"..\" cannot be names"),
            NameError::Character(c) if c.is_control() => write!(f, "A name cannot contain control characters"),
            NameError::Character(c) => write!(f, "A name cannot contain {c}"),
            NameError::Reserved(name) => write!(f, "{name} is reserved by Windows"),
            NameError::TrailingDotOrSpace => write!(f, "A name cannot end with a dot or a space"),
            NameError::TooLong => write!(f, "The name is too long"),
        }
    }
}

const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2",
    "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Whether `name` can be a file or folder name under `rules`.
pub fn validate_name(name: &str, rules: NameRules) -> Result<(), NameError> {
    if name.trim().is_empty() {
        return Err(NameError::Empty);
    }
    if name == "." || name == ".." {
        return Err(NameError::Dots);
    }
    let too_long = match rules {
        NameRules::Windows => name.encode_utf16().count() > 255,
        NameRules::Unix => name.len() > 255,
    };
    if too_long {
        return Err(NameError::TooLong);
    }
    for c in name.chars() {
        let bad = match rules {
            NameRules::Windows => c < ' ' || "\\/:*?\"<>|".contains(c),
            NameRules::Unix => c == '/' || c == '\0',
        };
        if bad {
            return Err(NameError::Character(c));
        }
    }
    if rules == NameRules::Windows {
        if name.ends_with('.') || name.ends_with(' ') {
            return Err(NameError::TrailingDotOrSpace);
        }
        let device = name.split('.').next().unwrap_or(name).trim_end().to_ascii_uppercase();
        if RESERVED.contains(&device.as_str()) {
            return Err(NameError::Reserved(device));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_off_the_extension() {
        assert_eq!(split_name("rapor.pdf", false), ("rapor", ".pdf"));
        assert_eq!(split_name("Arsiv.TAR.GZ", false), ("Arsiv", ".TAR.GZ"));
        assert_eq!(split_name("Makefile", false), ("Makefile", ""));
        assert_eq!(split_name(".gitignore", false), (".gitignore", ""));
        assert_eq!(split_name("v1.2", true), ("v1.2", ""));
        assert_eq!(split_name(".tar.gz", false), (".tar", ".gz"), "nothing before the double extension");
        assert_eq!(split_name("şarkı.mp3", false), ("şarkı", ".mp3"));
    }

    #[test]
    fn numbers_go_before_the_extension() {
        assert_eq!(numbered("rapor.pdf", false, 2), "rapor (2).pdf");
        assert_eq!(numbered("arsiv.tar.gz", false, 2), "arsiv (2).tar.gz");
        assert_eq!(numbered("Makefile", false, 3), "Makefile (3)");
        assert_eq!(numbered("v1.2", true, 2), "v1.2 (2)");
        assert_eq!(numbered("rapor (2).pdf", false, 3), "rapor (3).pdf");
        assert_eq!(numbered("Çalışma Notları.txt", false, 2), "Çalışma Notları (2).txt");
    }

    #[test]
    fn next_free_skips_taken_names_and_continues_a_number() {
        let taken = ["rapor (2).pdf", "rapor (3).pdf"];
        assert_eq!(next_free("rapor.pdf", false, |n| taken.contains(&n)), "rapor (4).pdf");
        assert_eq!(next_free("rapor (2).pdf", false, |_| false), "rapor (3).pdf");
        assert_eq!(next_free("New folder", true, |_| false), "New folder (2)");
        // "(1)" and "(02)" are not Gezik numbers: the bracket stays part of the name.
        assert_eq!(next_free("a (1).txt", false, |_| false), "a (1) (2).txt");
        assert_eq!(next_free("a (02).txt", false, |_| false), "a (02) (2).txt");
    }

    #[test]
    fn next_free_os_keeps_names_that_are_not_unicode() {
        let taken = |n: &OsStr| n == "rapor (2).pdf";
        assert_eq!(next_free_os(OsStr::new("rapor.pdf"), false, taken), "rapor (3).pdf");
        #[cfg(unix)]
        {
            use std::os::unix::ffi::{OsStrExt, OsStringExt};
            let name = OsStr::from_bytes(b"r\xfcz.pdf");
            assert_eq!(next_free_os(name, false, |_| false).into_vec(), b"r\xfcz (2).pdf");
            assert_eq!(next_free_os(name, true, |_| false).into_vec(), b"r\xfcz.pdf (2)");
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::{OsStrExt, OsStringExt};
            let wide: Vec<u16> = [0x72, 0xD800].into_iter().chain(".pdf".encode_utf16()).collect();
            let name = OsString::from_wide(&wide);
            let got: Vec<u16> = next_free_os(&name, false, |_| false).encode_wide().collect();
            let want: Vec<u16> = [0x72, 0xD800].into_iter().chain(" (2).pdf".encode_utf16()).collect();
            assert_eq!(got, want);
        }
    }

    #[test]
    fn rename_selects_the_stem() {
        assert_eq!(rename_selection("rapor.pdf", false), (0, 5));
        assert_eq!(rename_selection("şarkı.mp3", false), (0, "şarkı".len()));
        assert_eq!(rename_selection(".gitignore", false), (0, 10));
        assert_eq!(rename_selection("v1.2", true), (0, 4));
    }

    #[test]
    fn empty_dots_and_separators_are_refused_everywhere() {
        for rules in [NameRules::Windows, NameRules::Unix] {
            assert_eq!(validate_name("", rules), Err(NameError::Empty));
            assert_eq!(validate_name("   ", rules), Err(NameError::Empty));
            assert_eq!(validate_name("..", rules), Err(NameError::Dots));
            assert_eq!(validate_name("a/b", rules), Err(NameError::Character('/')));
            assert_eq!(validate_name("notlar.txt", rules), Ok(()));
        }
    }

    #[test]
    fn windows_characters_and_endings() {
        assert_eq!(validate_name("a:b", NameRules::Windows), Err(NameError::Character(':')));
        assert_eq!(validate_name("a\tb", NameRules::Windows), Err(NameError::Character('\t')));
        assert_eq!(validate_name("a.", NameRules::Windows), Err(NameError::TrailingDotOrSpace));
        assert_eq!(validate_name("a ", NameRules::Windows), Err(NameError::TrailingDotOrSpace));
        assert_eq!(validate_name("a:b", NameRules::Unix), Ok(()));
        assert_eq!(validate_name("a.", NameRules::Unix), Ok(()));
    }

    #[test]
    fn windows_reserved_names_are_refused_with_any_extension() {
        assert_eq!(validate_name("con", NameRules::Windows), Err(NameError::Reserved("CON".into())));
        assert_eq!(validate_name("con.txt", NameRules::Windows), Err(NameError::Reserved("CON".into())));
        assert_eq!(validate_name("Com1.tar.gz", NameRules::Windows), Err(NameError::Reserved("COM1".into())));
        assert_eq!(validate_name("console.txt", NameRules::Windows), Ok(()));
        assert_eq!(validate_name("con", NameRules::Unix), Ok(()));
    }

    #[test]
    fn long_names_are_counted_per_file_system() {
        let long = "ş".repeat(200);
        assert_eq!(validate_name(&long, NameRules::Windows), Ok(()), "200 UTF-16 units");
        assert_eq!(validate_name(&long, NameRules::Unix), Err(NameError::TooLong), "400 bytes");
        assert_eq!(validate_name(&"a".repeat(256), NameRules::Windows), Err(NameError::TooLong));
    }

    #[test]
    fn errors_read_well() {
        assert_eq!(NameError::Character(':').to_string(), "A name cannot contain :");
        assert_eq!(NameError::Character('\t').to_string(), "A name cannot contain control characters");
        assert_eq!(NameError::Reserved("CON".into()).to_string(), "CON is reserved by Windows");
    }
}
