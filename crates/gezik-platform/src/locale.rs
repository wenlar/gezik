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

/// The user's interface languages, most wanted first (spec 15 §5.2): Windows' display language
/// and its fallbacks (`tr-TR`, `en-US`), macOS' preferred languages (`zh-Hans-CN`, the app's
/// own first), the POSIX variables elsewhere (`de_DE.UTF-8`). Read once at start.
pub fn preferred() -> Vec<String> {
    #[cfg(windows)]
    {
        use windows::Win32::Globalization::{GetUserPreferredUILanguages, MUI_LANGUAGE_NAME};
        use windows::core::PWSTR;
        let (mut count, mut len) = (0u32, 0u32);
        // SAFETY: a null buffer asks for the length; then a buffer of that length.
        let mut buffer = unsafe {
            if GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, None, &mut len).is_err() || len == 0 {
                return Vec::new();
            }
            let mut buffer = vec![0u16; len as usize];
            if GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, Some(PWSTR(buffer.as_mut_ptr())), &mut len)
                .is_err()
            {
                return Vec::new();
            }
            buffer
        };
        buffer.truncate(len as usize);
        // Names split by NULs, the list ended by two.
        buffer.split(|&c| c == 0).filter(|s| !s.is_empty()).map(String::from_utf16_lossy).collect()
    }
    #[cfg(target_os = "macos")]
    {
        objc2_foundation::NSLocale::preferredLanguages().iter().map(|s| s.to_string()).collect()
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        posix_preferred(|key| std::env::var(key).ok())
    }
}

/// gettext's order: the locale is the first of `LC_ALL`, `LC_MESSAGES`, `LANG` that is set;
/// `LANGUAGE` (a `:` list) comes before it unless the locale is `C` (`C` then answers alone).
#[cfg(any(test, all(unix, not(target_os = "macos"))))]
fn posix_preferred(var: impl Fn(&str) -> Option<String>) -> Vec<String> {
    let set = |key: &str| var(key).filter(|v| !v.is_empty());
    let locale = set("LC_ALL").or_else(|| set("LC_MESSAGES")).or_else(|| set("LANG"));
    let plain = |l: &str| matches!(l.split(['.', '@']).next(), Some("C" | "POSIX"));
    match locale {
        Some(l) if plain(&l) => vec![l],
        locale => {
            let list = set("LANGUAGE").unwrap_or_default();
            list.split(':').filter(|s| !s.is_empty()).map(str::to_owned).chain(locale).collect()
        }
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
    fn preferred_languages_are_names() {
        let languages = super::preferred();
        assert!(languages.iter().all(|l| !l.is_empty() && l.len() < 85), "{languages:?}");
    }

    #[test]
    fn posix_variables_in_gettexts_order() {
        let got = |pairs: &[(&str, &str)]| {
            super::posix_preferred(|key| pairs.iter().find(|(k, _)| *k == key).map(|(_, v)| (*v).to_owned()))
        };
        assert_eq!(got(&[("LANGUAGE", "de:en"), ("LANG", "de_DE.UTF-8")]), ["de", "en", "de_DE.UTF-8"]);
        assert_eq!(got(&[("LC_ALL", "tr_TR.UTF-8"), ("LC_MESSAGES", "fr_FR"), ("LANG", "en_US")]), ["tr_TR.UTF-8"]);
        assert_eq!(got(&[("LC_ALL", ""), ("LC_MESSAGES", "fr_FR"), ("LANG", "en_US")]), ["fr_FR"]);
        assert_eq!(got(&[("LANGUAGE", "de"), ("LANG", "C.UTF-8")]), ["C.UTF-8"], "LANGUAGE is not read under C");
        assert_eq!(got(&[("LANG", "C")]), ["C"]);
        assert_eq!(got(&[("LANGUAGE", "tr::de")]), ["tr", "de"]);
        assert!(got(&[]).is_empty());
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
