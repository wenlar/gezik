//! Text in the user's legacy ("ANSI") code page, for files that are not UTF-8.

/// `bytes` decoded with the ANSI code page of the user's locale (Windows-1254 for Turkish),
/// else the system's; `None` where there is no such code page or the call fails.
pub fn decode_ansi(bytes: &[u8]) -> Option<String> {
    #[cfg(windows)]
    {
        win::decode(bytes)
    }
    #[cfg(not(windows))]
    {
        let _ = bytes;
        None
    }
}

#[cfg(windows)]
mod win {
    use windows::Win32::Globalization::{
        CP_ACP, GetLocaleInfoEx, LOCALE_IDEFAULTANSICODEPAGE, LOCALE_RETURN_NUMBER, MULTI_BYTE_TO_WIDE_CHAR_FLAGS,
        MultiByteToWideChar,
    };
    use windows::core::PCWSTR;

    /// The user's locale's ANSI code page. It can differ from the system's (`CP_ACP`, set by
    /// the "language for non-Unicode programs"): a Turkish user on an English system has
    /// 1254 here but 1252 there, and their Turkish files are in 1254.
    fn code_page() -> u32 {
        let mut number = [0u16; 2];
        // SAFETY: a null name means the user's default locale; with LOCALE_RETURN_NUMBER the
        // value is a u32 written into the two-unit buffer.
        let len = unsafe {
            GetLocaleInfoEx(PCWSTR::null(), LOCALE_IDEFAULTANSICODEPAGE | LOCALE_RETURN_NUMBER, Some(&mut number))
        };
        let page = u32::from(number[0]) | (u32::from(number[1]) << 16);
        // 0 means the locale has no ANSI code page (Unicode-only locales).
        if len == 2 && page != 0 { page } else { CP_ACP }
    }

    pub fn decode(bytes: &[u8]) -> Option<String> {
        if bytes.is_empty() {
            return Some(String::new());
        }
        // The input length must fit an i32; callers pass at most a preview's worth.
        i32::try_from(bytes.len()).ok()?;
        let flags = MULTI_BYTE_TO_WIDE_CHAR_FLAGS(0);
        let page = code_page();
        // SAFETY: the slices are valid for their lengths; a `None` output only measures.
        let len = unsafe { MultiByteToWideChar(page, flags, bytes, None) };
        if len <= 0 {
            return None;
        }
        let mut wide = vec![0u16; len as usize];
        // SAFETY: as above; `wide` has room for the measured length.
        let written = unsafe { MultiByteToWideChar(page, flags, bytes, Some(&mut wide)) };
        if written <= 0 {
            return None;
        }
        wide.truncate(written as usize);
        Some(String::from_utf16_lossy(&wide))
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn ascii_and_empty_pass_through() {
        assert_eq!(decode_ansi(b"").as_deref(), Some(""));
        assert_eq!(decode_ansi(b"plain text").as_deref(), Some("plain text"));
    }
}
