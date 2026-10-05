//! What went wrong, in words for people: "It is open in another program" rather than
//! "The process cannot access the file because it is being used by another process.
//! (os error 32)".

use std::io;

/// A short sentence for `err`: known system errors in plain words, Gezik's own messages as
/// they are, anything else in the system's words without the error number.
pub fn describe(err: &io::Error) -> String {
    if let Some(text) = err.raw_os_error().and_then(by_code) {
        return text.to_owned();
    }
    if err.raw_os_error().is_none()
        && let Some(text) = by_kind(err.kind())
    {
        // Gezik's own errors carry their text; only a bare kind is put in words.
        if err.get_ref().is_none() {
            return text.to_owned();
        }
    }
    let text = strip_code(&err.to_string());
    // A result code Windows has no text for ("0x80270035").
    let bare = text.strip_prefix("0x").is_some_and(|hex| !hex.is_empty() && hex.chars().all(|c| c.is_ascii_hexdigit()));
    if text.is_empty() || bare {
        return format!("Windows could not do it ({})", if text.is_empty() { "no reason given" } else { &text });
    }
    text
}

/// "Access is denied. (os error 5)" → "Access is denied".
fn strip_code(text: &str) -> String {
    let text = match text.rfind(" (os error ") {
        Some(at) if text.ends_with(')') => &text[..at],
        _ => text,
    };
    text.trim_end().trim_end_matches('.').to_owned()
}

fn by_kind(kind: io::ErrorKind) -> Option<&'static str> {
    use io::ErrorKind::*;
    Some(match kind {
        NotFound => "It no longer exists",
        PermissionDenied => "Access denied",
        AlreadyExists => "An item with this name already exists",
        StorageFull => "The disk is full",
        Interrupted => "Cancelled",
        TimedOut => "It took too long to answer",
        DirectoryNotEmpty => "The folder is not empty",
        InvalidFilename => "The name is not valid",
        ReadOnlyFilesystem => "The disk is write-protected",
        CrossesDevices => "It is on another drive",
        _ => return None,
    })
}

#[cfg(windows)]
fn by_code(code: i32) -> Option<&'static str> {
    Some(match code {
        2 => "It no longer exists",
        3 => "The folder it is in no longer exists",
        5 => "Access denied",
        15 => "The drive cannot be found",
        17 => "It is on another drive",
        19 => "The disk is write-protected",
        21 => "The drive is not ready",
        23 => "The disk could not be read (it may be damaged)",
        32 => "It is open in another program",
        33 => "Part of it is locked by another program",
        39 | 112 => "The disk is full",
        53 | 67 | 1231 => "The network location cannot be reached",
        59 | 64 | 1236 => "The network connection was lost",
        80 | 183 => "An item with this name already exists",
        123 => "The name is not valid",
        145 => "The folder is not empty",
        206 => "The name or path is too long",
        223 => "It is too big for this drive",
        225 => "Windows Security blocked it (it found a threat)",
        1392 => "The file is damaged and cannot be read",
        _ => return None,
    })
}

#[cfg(unix)]
fn by_code(code: i32) -> Option<&'static str> {
    Some(match code {
        libc::ENOENT => "It no longer exists",
        libc::EACCES | libc::EPERM => "Access denied",
        libc::EBUSY | libc::ETXTBSY => "It is in use",
        libc::ENOSPC => "The disk is full",
        libc::EDQUOT => "The disk quota is used up",
        libc::EROFS => "The disk is write-protected",
        libc::EEXIST => "An item with this name already exists",
        libc::ENOTEMPTY => "The folder is not empty",
        libc::ENAMETOOLONG => "The name or path is too long",
        libc::EXDEV => "It is on another drive",
        libc::EIO => "The disk could not be read or written",
        libc::ENOTCONN | libc::ETIMEDOUT | libc::EHOSTUNREACH | libc::ENETUNREACH => {
            "The network location cannot be reached"
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_system_errors_are_put_in_words() {
        // ERROR_FILE_NOT_FOUND on Windows, ENOENT elsewhere.
        assert_eq!(describe(&io::Error::from_raw_os_error(2)), "It no longer exists");
        if cfg!(windows) {
            assert_eq!(describe(&io::Error::from_raw_os_error(32)), "It is open in another program");
            assert_eq!(describe(&io::Error::from_raw_os_error(112)), "The disk is full");
        }
    }

    #[test]
    fn unknown_ones_lose_only_the_number() {
        let text = describe(&io::Error::from_raw_os_error(9999));
        assert!(!text.contains("os error"), "{text}");
        assert!(!text.is_empty());
        assert_eq!(strip_code("Access is denied. (os error 5)"), "Access is denied");
    }

    #[test]
    fn gezik_s_own_messages_stay_and_bare_kinds_get_words() {
        let own =
            io::Error::new(io::ErrorKind::InvalidInput, "names ending in a dot or space cannot go to the Recycle Bin");
        assert_eq!(describe(&own), "names ending in a dot or space cannot go to the Recycle Bin");
        assert_eq!(describe(&io::Error::from(io::ErrorKind::NotFound)), "It no longer exists");
        assert_eq!(describe(&io::Error::other("changed since; skipped")), "changed since; skipped");
    }
}
