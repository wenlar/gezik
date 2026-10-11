//! The user's language, for letter case rules (Turkish i/İ).

/// The user's locale name (`tr-TR` on Windows and macOS, `tr_TR.UTF-8` elsewhere); empty if
/// unknown.
pub fn language() -> String {
    #[cfg(windows)]
    {
        use windows::Win32::Globalization::GetUserDefaultLocaleName;
        let mut buffer = [0u16; 85];
        // SAFETY: the buffer and its length go together.
        let n = unsafe { GetUserDefaultLocaleName(&mut buffer) };
        if n > 1 {
            return String::from_utf16_lossy(&buffer[..n as usize - 1]);
        }
        String::new()
    }
    #[cfg(target_os = "macos")]
    {
        // An app started from Finder or the Dock has no LANG: the first language in System
        // Settings is the one that counts.
        if let Some(first) = objc2_foundation::NSLocale::preferredLanguages().firstObject() {
            return first.to_string();
        }
        posix_language()
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        posix_language()
    }
}

/// The separator a CSV file takes (spec 11 §11.2): on Windows the user's list separator, as
/// Excel reads it (`;` in Turkish); `,` elsewhere, and for a separator that is no single plain
/// character.
pub fn list_separator() -> char {
    #[cfg(windows)]
    {
        use windows::Win32::Globalization::{GetLocaleInfoEx, LOCALE_SLIST};
        use windows::core::PCWSTR;
        let mut buffer = [0u16; 8];
        // SAFETY: a null name means the user's default locale; the buffer and its length go together.
        let n = unsafe { GetLocaleInfoEx(PCWSTR::null(), LOCALE_SLIST, Some(&mut buffer)) };
        let text = String::from_utf16_lossy(&buffer[..usize::try_from(n).unwrap_or(0).saturating_sub(1)]);
        let mut chars = text.chars();
        if let (Some(c), None) = (chars.next(), chars.next())
            && !c.is_control()
            && !c.is_alphanumeric()
            && c != '"'
        {
            return c;
        }
    }
    ','
}

/// LC_ALL overrides LC_CTYPE overrides LANG (POSIX).
#[cfg(unix)]
fn posix_language() -> String {
    ["LC_ALL", "LC_CTYPE", "LANG"]
        .iter()
        .find_map(|key| std::env::var(key).ok().filter(|v| !v.is_empty() && v != "C" && v != "POSIX"))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    #[test]
    fn language_does_not_fail() {
        let lang = super::language();
        assert!(lang.len() < 85, "{lang}");
    }

    #[test]
    fn the_list_separator_is_one_plain_character() {
        let c = super::list_separator();
        assert!(!c.is_control() && !c.is_alphanumeric() && c != '"', "{c:?}");
        if !cfg!(windows) {
            assert_eq!(c, ',');
        }
    }

    /// The first language in System Settings, whatever LANG says (`cargo test` runs from a
    /// terminal, which sets LANG; Finder does not).
    #[cfg(target_os = "macos")]
    #[test]
    fn language_is_the_first_preferred_one_on_macos() {
        let out = std::process::Command::new("defaults").args(["read", "-g", "AppleLanguages"]).output().unwrap();
        let list = String::from_utf8_lossy(&out.stdout);
        let first = list.split(['(', ',', ')', '"', '\n', ' ']).find(|s| !s.is_empty()).unwrap_or_default();
        assert_eq!(super::language(), first);
    }
}
