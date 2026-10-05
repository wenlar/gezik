//! The text formats Linux programs exchange files in, on the clipboard and in drag and drop:
//! `file://` URIs (RFC 8089), `text/uri-list` (RFC 2483), GNOME's
//! `x-special/gnome-copied-files` (`copy` or `cut`, then URIs) and KDE's
//! `application/x-kde-cutselection` (`1` for cut). Compiled everywhere so it is tested here.

use std::path::{Path, PathBuf};

/// The bytes of `path` (on Linux a path is bytes; elsewhere, for the tests, its UTF-8).
fn path_bytes(path: &Path) -> Vec<u8> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        path.as_os_str().as_bytes().to_vec()
    }
    #[cfg(not(unix))]
    {
        path.to_string_lossy().into_owned().into_bytes()
    }
}

fn path_from_bytes(bytes: Vec<u8>) -> Option<PathBuf> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        Some(PathBuf::from(std::ffi::OsString::from_vec(bytes)))
    }
    #[cfg(not(unix))]
    {
        String::from_utf8(bytes).ok().map(PathBuf::from)
    }
}

/// `path` as a `file:///` URI, with every byte outside the URI path characters percent-encoded.
pub fn file_uri(path: &Path) -> String {
    let mut out = String::from("file://");
    for byte in path_bytes(path) {
        // RFC 3986 path characters: unreserved, sub-delims, ':', '@', and the separator.
        if byte.is_ascii_alphanumeric() || b"-._~!$&'()*+,;=:@/".contains(&byte) {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// The local path of a `file:` URI (`file:///…` or `file://localhost/…`); None for another
/// host, another scheme or a broken escape.
pub fn path_from_uri(uri: &str) -> Option<PathBuf> {
    let scheme = uri.get(..7)?;
    if !scheme.eq_ignore_ascii_case("file://") {
        return None;
    }
    let rest = &uri[7..];
    let slash = rest.find('/')?;
    if !matches!(&rest[..slash], "" | "localhost") {
        return None;
    }
    let encoded = &rest.as_bytes()[slash..];
    let mut bytes = Vec::with_capacity(encoded.len());
    let mut i = 0;
    while i < encoded.len() {
        if encoded[i] == b'%' {
            let hex = std::str::from_utf8(encoded.get(i + 1..i + 3)?).ok()?;
            bytes.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            bytes.push(encoded[i]);
            i += 1;
        }
    }
    path_from_bytes(bytes)
}

/// `text/uri-list`: one URI per line, each ending in CRLF.
pub fn uri_list(paths: &[PathBuf]) -> String {
    paths.iter().map(|path| file_uri(path) + "\r\n").collect()
}

/// The local paths in a `text/uri-list` (comments, blank lines and other URIs are skipped).
pub fn parse_uri_list(text: &str) -> Vec<PathBuf> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(path_from_uri)
        .collect()
}

/// `x-special/gnome-copied-files`: `copy` or `cut`, then one URI per line.
pub fn gnome_copied_files(paths: &[PathBuf], cut: bool) -> String {
    let mut out = String::from(if cut { "cut" } else { "copy" });
    for path in paths {
        out.push('\n');
        out.push_str(&file_uri(path));
    }
    out
}

/// Whether a `x-special/gnome-copied-files` text is a cut, and its paths; None if it is not
/// one (or names no local file).
pub fn parse_gnome_copied_files(text: &str) -> Option<(bool, Vec<PathBuf>)> {
    let mut lines = text.lines().map(str::trim);
    let cut = match lines.next()? {
        "cut" => true,
        "copy" => false,
        _ => return None,
    };
    let paths: Vec<PathBuf> = lines.filter(|line| !line.is_empty()).filter_map(path_from_uri).collect();
    (!paths.is_empty()).then_some((cut, paths))
}

/// `application/x-kde-cutselection`.
pub fn kde_cut(cut: bool) -> &'static [u8] {
    if cut { b"1" } else { b"0" }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(text: &str) -> PathBuf {
        PathBuf::from(text)
    }

    #[test]
    fn file_uris_round_trip_unicode_and_reserved_characters() {
        let path = p("/home/ş é/a#b%c.txt");
        let uri = file_uri(&path);
        assert_eq!(uri, "file:///home/%C5%9F%20%C3%A9/a%23b%25c.txt");
        assert_eq!(path_from_uri(&uri), Some(path));
        assert_eq!(file_uri(&p("/a/b(1)+c,d=e;f@g:h~_-.")), "file:///a/b(1)+c,d=e;f@g:h~_-.");
        assert_eq!(file_uri(&p("/q?x")), "file:///q%3Fx");
    }

    #[test]
    fn uris_from_other_programs_are_read() {
        assert_eq!(path_from_uri("file://localhost/etc/hosts"), Some(p("/etc/hosts")));
        assert_eq!(path_from_uri("file:///tmp/a%2fb"), Some(p("/tmp/a/b")), "an escaped slash is a slash");
        assert_eq!(path_from_uri("FILE:///tmp/x"), Some(p("/tmp/x")), "the scheme ignores case");
        assert_eq!(path_from_uri("file://server/share/x"), None, "another host");
        assert_eq!(path_from_uri("https://example.org/x"), None);
        assert_eq!(path_from_uri("file:///bad%zz"), None);
        assert_eq!(path_from_uri("file:///cut%C"), None);
    }

    #[test]
    fn uri_lists_skip_comments_and_foreign_hosts() {
        let text = "# from a browser\r\nfile:///a/x.txt\r\n\r\nhttps://example.org/y\r\nfile:///b%20c\n";
        assert_eq!(parse_uri_list(text), vec![p("/a/x.txt"), p("/b c")]);
        assert_eq!(uri_list(&[p("/a"), p("/b c")]), "file:///a\r\nfile:///b%20c\r\n");
        assert_eq!(parse_uri_list(&uri_list(&[p("/ç/d")])), vec![p("/ç/d")]);
    }

    #[test]
    fn gnome_copied_files_round_trip() {
        let paths = vec![p("/a b"), p("/c")];
        let copied = gnome_copied_files(&paths, false);
        assert_eq!(copied, "copy\nfile:///a%20b\nfile:///c");
        assert_eq!(parse_gnome_copied_files(&copied), Some((false, paths.clone())));
        assert_eq!(parse_gnome_copied_files(&gnome_copied_files(&paths, true)), Some((true, paths)));
        assert_eq!(parse_gnome_copied_files("cut\r\nfile:///x\r\n"), Some((true, vec![p("/x")])));
        assert_eq!(parse_gnome_copied_files("move\nfile:///x"), None);
        assert_eq!(parse_gnome_copied_files("copy\nhttps://example.org"), None, "no local file");
    }

    #[test]
    fn kde_marks_a_cut_with_one() {
        assert_eq!(kde_cut(true), b"1");
        assert_eq!(kde_cut(false), b"0");
    }

    #[cfg(unix)]
    #[test]
    fn paths_that_are_not_utf8_survive() {
        use std::os::unix::ffi::OsStrExt;
        let path = PathBuf::from(std::ffi::OsStr::from_bytes(b"/tmp/\xff\xfe"));
        let uri = file_uri(&path);
        assert_eq!(uri, "file:///tmp/%FF%FE");
        assert_eq!(path_from_uri(&uri), Some(path));
    }
}
