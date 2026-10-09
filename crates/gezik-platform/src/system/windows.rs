//! The registry under HKEY_CURRENT_USER (spec 8.2, 10.5): there is no way here to name
//! another root. Keys are relative to HKCU (`Environment`, `Software\Microsoft\…`); values
//! are text (REG_SZ, REG_EXPAND_SZ), read raw: `%VAR%` stays as written.

use std::io;

use ::windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_SUCCESS, LPARAM, WIN32_ERROR, WPARAM};
#[cfg(test)]
use ::windows::Win32::System::Registry::REG_DWORD;
use ::windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_EXPAND_SZ, REG_OPTION_NON_VOLATILE, REG_SAM_FLAGS,
    REG_SZ, REG_VALUE_TYPE, RRF_NOEXPAND, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ, RegCloseKey, RegCreateKeyExW,
    RegDeleteKeyW, RegDeleteValueW, RegGetValueW, RegOpenKeyExW, RegQueryInfoKeyW, RegSetValueExW,
};
use ::windows::Win32::UI::WindowsAndMessaging::{
    HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
};
use ::windows::core::PCWSTR;
use gezik_core::system_change::RegType;

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

fn check(code: WIN32_ERROR) -> io::Result<()> {
    if code == ERROR_SUCCESS { Ok(()) } else { Err(io::Error::from_raw_os_error(code.0 as i32)) }
}

/// An open key, closed when dropped.
struct Key(HKEY);

impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY: a handle this module opened, closed once.
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

fn open(key: &str, access: REG_SAM_FLAGS) -> io::Result<Key> {
    let name = wide(key);
    let mut handle = HKEY::default();
    // SAFETY: `name` ends with a NUL and lives across the call.
    check(unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(name.as_ptr()), None, access, &mut handle) })?;
    Ok(Key(handle))
}

/// A text value as stored; `None` if the key or the value is not there. Another type is an
/// error (`ERROR_UNSUPPORTED_TYPE`), and so is text that is not valid UTF-16 or holds a NUL
/// (it could not be written back exactly, decision 19).
pub fn read_value(key: &str, name: &str) -> io::Result<Option<(RegType, String)>> {
    let (key_w, name_w) = (wide(key), wide(name));
    let flags = RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | RRF_NOEXPAND;
    // The value may grow between asking its size and reading it: a few tries.
    for _ in 0..4 {
        let mut size = 0u32;
        // SAFETY: names end with NUL; only the size is asked for.
        let code = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                PCWSTR(key_w.as_ptr()),
                PCWSTR(name_w.as_ptr()),
                flags,
                None,
                None,
                Some(&mut size),
            )
        };
        if code == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        check(code)?;
        let mut buf = vec![0u16; (size as usize).div_ceil(2) + 1];
        let mut got = u32::try_from(buf.len() * 2).unwrap_or(u32::MAX);
        let mut ty = REG_VALUE_TYPE::default();
        // SAFETY: `buf` holds `got` bytes.
        let code = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                PCWSTR(key_w.as_ptr()),
                PCWSTR(name_w.as_ptr()),
                flags,
                Some(&mut ty),
                Some(buf.as_mut_ptr().cast()),
                Some(&mut got),
            )
        };
        if code == ERROR_MORE_DATA {
            continue;
        }
        if code == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        check(code)?;
        buf.truncate(got as usize / 2);
        // RegGetValueW ends the text with one NUL.
        if buf.last() == Some(&0) {
            buf.pop();
        }
        let bad = || io::Error::new(io::ErrorKind::InvalidData, "the value is not valid text");
        let data = String::from_utf16(&buf).map_err(|_| bad())?;
        if data.contains('\0') {
            return Err(bad());
        }
        let ty = if ty == REG_EXPAND_SZ { RegType::ExpandSz } else { RegType::Sz };
        return Ok(Some((ty, data)));
    }
    Err(io::Error::new(io::ErrorKind::Interrupted, "the value kept changing while it was read"))
}

/// Writes a text value in one call (the old one stays if it fails); the key must be there.
pub fn write_value(key: &str, name: &str, ty: RegType, data: &str) -> io::Result<()> {
    if data.contains('\0') {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "the text holds a NUL"));
    }
    let handle = open(key, KEY_SET_VALUE)?;
    let name = wide(name);
    let bytes: Vec<u8> = wide(data).iter().flat_map(|unit| unit.to_le_bytes()).collect();
    let ty = match ty {
        RegType::Sz => REG_SZ,
        RegType::ExpandSz => REG_EXPAND_SZ,
    };
    // SAFETY: `name` ends with NUL; `bytes` is the text with its NUL.
    check(unsafe { RegSetValueExW(handle.0, PCWSTR(name.as_ptr()), None, ty, Some(&bytes)) })
}

/// Deletes a value; not there (nor its key) is fine.
pub fn delete_value(key: &str, name: &str) -> io::Result<()> {
    let handle = match open(key, KEY_SET_VALUE) {
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        other => other?,
    };
    let name = wide(name);
    // SAFETY: `name` ends with NUL.
    let code = unsafe { RegDeleteValueW(handle.0, PCWSTR(name.as_ptr())) };
    if code == ERROR_FILE_NOT_FOUND { Ok(()) } else { check(code) }
}

pub fn key_exists(key: &str) -> io::Result<bool> {
    match open(key, KEY_QUERY_VALUE) {
        Ok(_) => Ok(true),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err),
    }
}

/// Makes one key. Its parent must be there: every level Gezik makes is a journal entry of
/// its own (decision 7), so none is made unseen here.
pub fn create_key(key: &str) -> io::Result<()> {
    if let Some((parent, _)) = key.rsplit_once('\\')
        && !key_exists(parent)?
    {
        return Err(io::Error::new(io::ErrorKind::NotFound, format!(r"HKCU\{parent} is not there")));
    }
    let name = wide(key);
    let mut handle = HKEY::default();
    // SAFETY: `name` ends with NUL; the handle is closed by `Key`.
    check(unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(name.as_ptr()),
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_QUERY_VALUE,
            None,
            &mut handle,
            None,
        )
    })?;
    drop(Key(handle));
    Ok(())
}

/// Deletes a key with no values and no subkeys; one with something in it is an error of
/// kind `DirectoryNotEmpty`; one not there is fine.
pub fn delete_empty_key(key: &str) -> io::Result<()> {
    let handle = match open(key, KEY_QUERY_VALUE) {
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        other => other?,
    };
    let (mut keys, mut values) = (0u32, 0u32);
    // SAFETY: only the two counts are asked for.
    check(unsafe {
        RegQueryInfoKeyW(
            handle.0,
            None,
            None,
            None,
            Some(&mut keys),
            None,
            None,
            Some(&mut values),
            None,
            None,
            None,
            None,
        )
    })?;
    drop(handle);
    if keys > 0 || values > 0 {
        return Err(io::Error::new(io::ErrorKind::DirectoryNotEmpty, "the key is not empty"));
    }
    let name = wide(key);
    // SAFETY: `name` ends with NUL.
    check(unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, PCWSTR(name.as_ptr())) })
}

pub(super) fn environment_changed() {
    let what = wide("Environment");
    // SAFETY: `what` lives across the call; SMTO_ABORTIFHUNG skips hung windows.
    unsafe {
        let _ = SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            WPARAM(0),
            LPARAM(what.as_ptr() as isize),
            SMTO_ABORTIFHUNG,
            2000,
            None,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A test's own key, HKCU\Software\GezikTest-<pid>-<name>; its whole tree is taken away
    /// when dropped, so a failing test leaves nothing behind either.
    struct Root(String);

    impl Drop for Root {
        fn drop(&mut self) {
            let name = wide(&self.0);
            // SAFETY: `name` ends with NUL; only a GezikTest-* key is named.
            unsafe {
                let _ = ::windows::Win32::System::Registry::RegDeleteTreeW(HKEY_CURRENT_USER, PCWSTR(name.as_ptr()));
            }
        }
    }

    fn root(name: &str) -> Root {
        let root = format!(r"Software\GezikTest-{}-{name}", std::process::id());
        create_key(&root).unwrap();
        Root(root)
    }

    #[test]
    fn values_round_trip_raw_with_their_type() {
        let guard = root("values");
        let root = guard.0.as_str();
        let raw = r"%USERPROFILE%\x;;C:\ç ğ\;";
        write_value(root, "Path", RegType::ExpandSz, raw).unwrap();
        assert_eq!(read_value(root, "Path").unwrap(), Some((RegType::ExpandSz, raw.to_owned())), "not expanded");
        write_value(root, "", RegType::Sz, r"C:\Tools\gezik.exe").unwrap();
        assert_eq!(read_value(root, "").unwrap(), Some((RegType::Sz, r"C:\Tools\gezik.exe".to_owned())));
        let long: String = (0..2_000).map(|i| format!(r"C:\T{i};")).collect();
        write_value(root, "Long", RegType::Sz, &long).unwrap();
        assert_eq!(read_value(root, "Long").unwrap(), Some((RegType::Sz, long)));
        assert!(write_value(root, "Nul", RegType::Sz, "a\0b").is_err());
        for name in ["Path", "", "Long"] {
            delete_value(root, name).unwrap();
        }
        assert_eq!(read_value(root, "Path").unwrap(), None);
        delete_value(root, "Path").unwrap();
        delete_empty_key(root).unwrap();
        assert!(!key_exists(root).unwrap());
        assert_eq!(read_value(root, "Path").unwrap(), None, "a missing key reads as no value");
    }

    #[test]
    fn keys_are_made_one_level_at_a_time_and_taken_only_if_empty() {
        let guard = root("keys");
        let root = guard.0.as_str();
        let deep = format!(r"{root}\a\b");
        assert_eq!(create_key(&deep).unwrap_err().kind(), io::ErrorKind::NotFound, "no parent: not made");
        create_key(&format!(r"{root}\a")).unwrap();
        create_key(&deep).unwrap();
        assert_eq!(delete_empty_key(&format!(r"{root}\a")).unwrap_err().kind(), io::ErrorKind::DirectoryNotEmpty);
        write_value(&deep, "", RegType::Sz, "x").unwrap();
        assert_eq!(delete_empty_key(&deep).unwrap_err().kind(), io::ErrorKind::DirectoryNotEmpty);
        delete_value(&deep, "").unwrap();
        delete_empty_key(&deep).unwrap();
        delete_empty_key(&format!(r"{root}\a")).unwrap();
        delete_empty_key(root).unwrap();
        delete_empty_key(root).unwrap();
    }

    #[test]
    fn other_value_types_are_not_read_as_text() {
        let guard = root("types");
        let root = guard.0.as_str();
        let name = wide("N");
        let handle = open(root, KEY_SET_VALUE).unwrap();
        // SAFETY: a 4-byte REG_DWORD for the test.
        check(unsafe { RegSetValueExW(handle.0, PCWSTR(name.as_ptr()), None, REG_DWORD, Some(&7u32.to_le_bytes())) })
            .unwrap();
        drop(handle);
        assert!(read_value(root, "N").is_err());
        delete_value(root, "N").unwrap();
        delete_empty_key(root).unwrap();
    }
}
