//! A path as text to paste somewhere else (spec 4): quoted for a shell, with forward slashes,
//! as a `file://` URL (RFC 8089) or through a mapped drive's share. Pure, on text: a Windows
//! path is told by `windows`, so every rule is tested on every system.

use std::fmt::Write as _;

/// One way to write a path ("Copy path as ▸").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathFormat {
    Full,
    Quoted,
    ForwardSlashes,
    Name,
    Folder,
    FileUrl,
    Unc,
}

impl PathFormat {
    /// In menu order (the menu ids follow it).
    pub const ALL: [PathFormat; 7] = [
        PathFormat::Full,
        PathFormat::Quoted,
        PathFormat::ForwardSlashes,
        PathFormat::Name,
        PathFormat::Folder,
        PathFormat::FileUrl,
        PathFormat::Unc,
    ];

    pub fn label(self) -> &'static str {
        match self {
            PathFormat::Full => "Full path",
            PathFormat::Quoted => "Quoted",
            PathFormat::ForwardSlashes => "With forward slashes",
            PathFormat::Name => "Name",
            PathFormat::Folder => "Folder path",
            PathFormat::FileUrl => "file:// URL",
            PathFormat::Unc => "UNC path",
        }
    }

    /// Whether the menu lists it: forward slashes only on Windows (elsewhere they are the full
    /// path), UNC only on Windows when the first item is on a mapped drive (`unc`).
    pub fn offered(self, windows: bool, unc: bool) -> bool {
        match self {
            PathFormat::ForwardSlashes => windows,
            PathFormat::Unc => windows && unc,
            _ => true,
        }
    }
}

fn is_separator(c: char, windows: bool) -> bool {
    c == '/' || (windows && c == '\\')
}

/// Whether `path` is a root: `C:\` (or `C:`) and `\\server\share` on Windows, `/` elsewhere.
fn is_root(path: &str, windows: bool) -> bool {
    let trimmed = path.trim_end_matches(|c| is_separator(c, windows));
    if !windows {
        return trimmed.is_empty() && !path.is_empty();
    }
    let bytes = trimmed.as_bytes();
    if bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return true;
    }
    match trimmed.strip_prefix(r"\\").or_else(|| trimmed.strip_prefix("//")) {
        Some(rest) => rest.split(|c| is_separator(c, true)).filter(|part| !part.is_empty()).count() <= 2,
        None => false,
    }
}

/// The server of a path that names a server alone (`\\nas`, `\\nas\`, `//nas`): Windows lists
/// its shares there (spec 9 §7.4). `None` for a share, a device path (`\\?\`, `\\.\`) or
/// anything else.
pub fn server_only(text: &str) -> Option<&str> {
    let rest = text.strip_prefix(r"\\").or_else(|| text.strip_prefix("//"))?;
    let name = rest.strip_suffix(['\\', '/']).unwrap_or(rest);
    (!name.is_empty() && !name.contains(['\\', '/']) && name != "?" && name != ".").then_some(name)
}

/// The last part of `path` (`rapor.pdf`); a root is its own name.
pub fn file_name(path: &str, windows: bool) -> &str {
    if is_root(path, windows) {
        return path;
    }
    let trimmed = path.trim_end_matches(|c| is_separator(c, windows));
    match trimmed.rfind(|c| is_separator(c, windows)) {
        Some(i) => &trimmed[i + 1..],
        None => trimmed,
    }
}

/// The folder `path` is in (`C:\Users\a`). A root is its own folder; a drive's top folder
/// keeps its separator (`C:\`, `/`).
pub fn folder_of(path: &str, windows: bool) -> String {
    if is_root(path, windows) {
        return path.to_owned();
    }
    let trimmed = path.trim_end_matches(|c| is_separator(c, windows));
    let Some(i) = trimmed.rfind(|c| is_separator(c, windows)) else { return path.to_owned() };
    let parent = &trimmed[..i];
    if parent.is_empty() || (windows && parent.len() == 2 && parent.ends_with(':')) {
        // `/a` → `/`, `C:\a` → `C:\`.
        return trimmed[..=i].to_owned();
    }
    parent.to_owned()
}

/// `text` in POSIX single quotes, safe to paste into bash or zsh: `it's` → `'it'\''s'`.
pub fn posix_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// `path` as a `file://` URL (RFC 8089): its UTF-8 bytes percent-encoded but for
/// `A–Z a–z 0–9 - . _ ~ /`; `C:\a b` → `file:///C:/a%20b` (the drive's colon stays),
/// `\\server\share\x` → `file://server/share/x`, `/home/a` → `file:///home/a`.
pub fn file_url(path: &str, windows: bool) -> String {
    let slashed = if windows { path.replace('\\', "/") } else { path.to_owned() };
    if windows && let Some(rest) = slashed.strip_prefix("//") {
        return format!("file://{}", encode(rest));
    }
    let bytes = slashed.as_bytes();
    if windows && bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return format!("file:///{}{}", &slashed[..2], encode(&slashed[2..]));
    }
    format!("file://{}", encode(&slashed))
}

fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/') {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// `path` on a mapped drive (`Z:\proje\a.txt`) through the share `remote` gives for its
/// letter (`\\server\share`): `\\server\share\proje\a.txt`. `None` when the path has no
/// drive letter or the drive is not mapped.
pub fn unc_path(path: &str, remote: &dyn Fn(char) -> Option<String>) -> Option<String> {
    let mut chars = path.chars();
    let (Some(letter), Some(':')) = (chars.next(), chars.next()) else { return None };
    if !letter.is_ascii_alphabetic() {
        return None;
    }
    let share = remote(letter.to_ascii_uppercase())?;
    let share = share.trim_end_matches('\\');
    let rest = &path[2..];
    Some(if rest.is_empty() { format!("{share}\\") } else { format!("{share}{rest}") })
}

fn format_one(path: &str, kind: PathFormat, windows: bool, remote: &dyn Fn(char) -> Option<String>) -> String {
    match kind {
        PathFormat::Full => path.to_owned(),
        // Windows names have no `"`: cmd, PowerShell and Explorer's "Copy as path" quote so.
        PathFormat::Quoted if windows => format!("\"{path}\""),
        PathFormat::Quoted => posix_quote(path),
        PathFormat::ForwardSlashes if windows => path.replace('\\', "/"),
        PathFormat::ForwardSlashes => path.to_owned(),
        PathFormat::Name => file_name(path, windows).to_owned(),
        PathFormat::Folder => folder_of(path, windows),
        PathFormat::FileUrl => file_url(path, windows),
        PathFormat::Unc => unc_path(path, remote).unwrap_or_else(|| path.to_owned()),
    }
}

/// `paths` written as `kind`, one per line (`\r\n` on Windows, `\n` elsewhere), with no line
/// break after the last.
pub fn format_paths(
    paths: &[String],
    kind: PathFormat,
    windows: bool,
    remote: &dyn Fn(char) -> Option<String>,
) -> String {
    let lines: Vec<String> = paths.iter().map(|path| format_one(path, kind, windows, remote)).collect();
    lines.join(if windows { "\r\n" } else { "\n" })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_only_names_a_bare_server() {
        assert_eq!(server_only(r"\\nas"), Some("nas"));
        assert_eq!(server_only(r"\\nas\"), Some("nas"));
        assert_eq!(server_only("//nas"), Some("nas"));
        assert_eq!(server_only(r"\\nas\foto"), None, "a share");
        assert_eq!(server_only(r"\\?\"), None);
        assert_eq!(server_only(r"\\.\"), None);
        assert_eq!(server_only(r"\\"), None);
        assert_eq!(server_only(r"C:\"), None);
        assert_eq!(server_only("/home"), None);
    }

    fn none(_: char) -> Option<String> {
        None
    }

    fn mapped(letter: char) -> Option<String> {
        (letter == 'Z').then(|| r"\\sunucu\pay".to_owned())
    }

    fn one(path: &str, kind: PathFormat, windows: bool) -> String {
        format_paths(&[path.to_owned()], kind, windows, &mapped)
    }

    #[test]
    fn windows_paths_in_every_format() {
        let p = r"C:\Users\a\rapor ç.pdf";
        assert_eq!(one(p, PathFormat::Full, true), p);
        assert_eq!(one(p, PathFormat::Quoted, true), r#""C:\Users\a\rapor ç.pdf""#);
        assert_eq!(one(p, PathFormat::ForwardSlashes, true), "C:/Users/a/rapor ç.pdf");
        assert_eq!(one(p, PathFormat::Name, true), "rapor ç.pdf");
        assert_eq!(one(p, PathFormat::Folder, true), r"C:\Users\a");
        assert_eq!(one(p, PathFormat::FileUrl, true), "file:///C:/Users/a/rapor%20%C3%A7.pdf");
        assert_eq!(one(p, PathFormat::Unc, true), p, "not on a mapped drive: the full path");
    }

    #[test]
    fn posix_paths_quote_for_the_shell() {
        let p = "/home/a/it's ş.pdf";
        assert_eq!(one(p, PathFormat::Quoted, false), r"'/home/a/it'\''s ş.pdf'");
        assert_eq!(posix_quote("a'b'"), r"'a'\''b'\'''");
        assert_eq!(one(p, PathFormat::ForwardSlashes, false), p);
        assert_eq!(one(p, PathFormat::Name, false), "it's ş.pdf");
        assert_eq!(one(p, PathFormat::Folder, false), "/home/a");
        assert_eq!(one(p, PathFormat::FileUrl, false), "file:///home/a/it%27s%20%C5%9F.pdf");
    }

    #[test]
    fn url_encoding_keeps_only_the_unreserved() {
        assert_eq!(file_url("/a/100% #1 x~y_z-.txt", false), "file:///a/100%25%20%231%20x~y_z-.txt");
        assert_eq!(file_url(r"C:\", true), "file:///C:/");
        assert_eq!(file_url(r"\\nas\foto\yaz 1.jpg", true), "file://nas/foto/yaz%201.jpg");
        assert_eq!(file_url("/", false), "file:///");
    }

    #[test]
    fn roots_are_their_own_name_and_folder() {
        for (path, windows) in
            [(r"C:\", true), ("C:", true), (r"\\nas\foto", true), (r"\\nas\foto\", true), ("/", false)]
        {
            assert_eq!(file_name(path, windows), path);
            assert_eq!(folder_of(path, windows), path);
        }
        assert_eq!(folder_of(r"C:\a", true), r"C:\");
        assert_eq!(folder_of("/a", false), "/");
        assert_eq!(folder_of(r"\\nas\foto\x.jpg", true), r"\\nas\foto");
        assert_eq!(file_name(r"C:\a\sub\", true), "sub");
        assert_eq!(file_name(r"C:\a\b", false), r"C:\a\b", "a backslash is no separator off Windows");
    }

    #[test]
    fn a_mapped_drive_goes_through_its_share() {
        assert_eq!(unc_path(r"Z:\proje\a.txt", &mapped).as_deref(), Some(r"\\sunucu\pay\proje\a.txt"));
        assert_eq!(unc_path(r"z:\proje", &mapped).as_deref(), Some(r"\\sunucu\pay\proje"));
        assert_eq!(unc_path(r"Z:\", &mapped).as_deref(), Some(r"\\sunucu\pay\"));
        assert_eq!(unc_path(r"C:\a", &mapped), None);
        assert_eq!(unc_path(r"\\nas\x", &mapped), None);
        assert_eq!(unc_path("/z:/a", &mapped), None);
        let lines = format_paths(&[r"Z:\a.txt".into(), r"C:\b.txt".into()], PathFormat::Unc, true, &mapped);
        assert_eq!(lines, "\\\\sunucu\\pay\\a.txt\r\nC:\\b.txt", "items not on it keep their full path");
    }

    #[test]
    fn several_items_one_per_line_without_a_last_line_break() {
        let paths = ["/a/x".to_owned(), "/a/y".to_owned()];
        assert_eq!(format_paths(&paths, PathFormat::Name, false, &none), "x\ny");
        let win = [r"C:\x".to_owned(), r"C:\y".to_owned()];
        assert_eq!(format_paths(&win, PathFormat::Full, true, &none), "C:\\x\r\nC:\\y");
        assert_eq!(format_paths(&[], PathFormat::Full, true, &none), "");
    }

    #[test]
    fn the_menu_offers_slashes_and_unc_only_where_they_mean_something() {
        let offered = |windows, unc| -> Vec<&str> {
            PathFormat::ALL.iter().filter(|k| k.offered(windows, unc)).map(|k| k.label()).collect()
        };
        assert_eq!(offered(false, false), ["Full path", "Quoted", "Name", "Folder path", "file:// URL"]);
        assert_eq!(offered(true, false).len(), 6);
        assert_eq!(offered(true, true).last(), Some(&"UNC path"));
    }
}
