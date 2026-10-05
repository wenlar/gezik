//! The user's language, for letter case rules (Turkish i/İ).

/// The user's locale name (`tr-TR` on Windows, `tr_TR.UTF-8` elsewhere); empty if unknown.
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
    #[cfg(not(windows))]
    {
        // LC_ALL overrides LC_CTYPE overrides LANG (POSIX); macOS apps get LANG from the system.
        ["LC_ALL", "LC_CTYPE", "LANG"]
            .iter()
            .find_map(|key| std::env::var(key).ok().filter(|v| !v.is_empty() && v != "C" && v != "POSIX"))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn language_does_not_fail() {
        let lang = super::language();
        assert!(lang.len() < 85, "{lang}");
    }
}
