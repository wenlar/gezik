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
