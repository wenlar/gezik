//! Windows: handing a place Gezik cannot show to Explorer (spec 6.3; decisions 2, 3). Not
//! through a shell verb: another file manager may own `Folder\shell\open` in HKCU (the 9b4
//! probe found Files there), so `explorer.exe` is started by its full path. Any app can start
//! `gezik --shell X`, so only folder-like places reach Explorer as they are; any other file
//! goes as `/select,`, which shows it and never runs it. A place sent back within 10 s is not
//! handed on again (a file: it holds across processes).

use std::io;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use ::windows::Win32::UI::WindowsAndMessaging::{IDYES, MB_ICONWARNING, MB_YESNO, MessageBoxW};
use ::windows::core::{PCWSTR, w};

const THIS_PC: &str = "::{20D04FE0-3AEA-1069-A2D8-08002B30309D}";
/// `%TEMP%\gezik-fallback.txt`: the last place handed on and when (`<unix s>\n<place>`).
pub const GUARD_FILE: &str = "gezik-fallback.txt";

pub fn record(target: &str, now: u64) -> String {
    format!("{now}\n{target}")
}

/// Whether `record` (the guard file's text) says `target` was handed on less than 10 s ago.
pub fn seen_recently(record: &str, target: &str, now: u64) -> bool {
    let Some((when, place)) = record.split_once('\n') else { return false };
    when.parse::<u64>().is_ok_and(|when| now.saturating_sub(when) < 10) && place == target
}

/// Explorer's command line for `target` ("" = This PC), or None for anything that is not a
/// path, a `shell:` name or a chain of `::{CLSID}`s. `is_dir` looks at the disk.
pub fn explorer_argument(target: &str, is_dir: impl Fn(&Path) -> bool) -> Option<String> {
    let text = target.trim().trim_end_matches('"');
    if text.is_empty() {
        return Some(THIS_PC.to_owned());
    }
    // A quote or a control character could end the argument early.
    if text.chars().any(|c| c == '"' || c.is_control()) {
        return None;
    }
    if let Some(name) = text.get(..6).filter(|p| p.eq_ignore_ascii_case("shell:")).map(|_| &text[6..]) {
        // A bare name only: `shell:Downloads\x.exe` would open a file.
        let plain = !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == ' ');
        return (plain || clsids(name)).then(|| text.to_owned());
    }
    if clsids(text) {
        return Some(text.to_owned());
    }
    let path = match text.as_bytes() {
        [letter, b':'] if letter.is_ascii_alphabetic() => format!("{text}\\"),
        _ => text.to_owned(),
    };
    let absolute =
        matches!(path.as_bytes(), [l, b':', b'\\', ..] if l.is_ascii_alphabetic()) || path.starts_with(r"\\");
    if !absolute {
        return None;
    }
    // A bare `\\server` is no file either.
    let server = path.strip_prefix(r"\\").is_some_and(|s| !s.trim_end_matches('\\').contains('\\'));
    let folder_like = server || is_dir(&PathBuf::from(&path)) || path.to_ascii_lowercase().ends_with(".library-ms");
    // Explorer splits its arguments at commas. `"C:\"` would read as an escaped quote: only
    // names with a space or a comma are quoted, and those are never a root.
    let quoted = if path.contains([' ', ',']) { format!("\"{}\"", path.trim_end_matches('\\')) } else { path };
    Some(if folder_like { quoted } else { format!("/select,{quoted}") })
}

/// `::{…}` or `::{…}\::{…}…`, each a GUID.
fn clsids(text: &str) -> bool {
    text.split('\\').all(|part| {
        part.strip_prefix("::{").and_then(|p| p.strip_suffix('}')).is_some_and(|guid| {
            guid.len() == 36
                && guid
                    .char_indices()
                    .all(|(i, c)| if [8, 13, 18, 23].contains(&i) { c == '-' } else { c.is_ascii_hexdigit() })
        })
    })
}

/// Opens `target` ("" = This PC) in Explorer; refused (`InvalidInput`) unless
/// [`explorer_argument`] takes it.
pub fn open_in_explorer(target: &str) -> io::Result<()> {
    let argument = explorer_argument(target, Path::is_dir).ok_or(io::ErrorKind::InvalidInput)?;
    let windows = std::env::var_os("SystemRoot").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
    // Explorer reads its own command line: the argument goes as is.
    Command::new(windows.join("explorer.exe")).raw_arg(argument).spawn().map(drop)
}

/// The loop question (decision 3), with no window of Gezik's: yes = restore Explorer.
pub fn ask_restore(text: &str) -> bool {
    let text: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    // SAFETY: both texts end with NUL and live across the call.
    unsafe { MessageBoxW(None, PCWSTR(text.as_ptr()), w!("Gezik"), MB_YESNO | MB_ICONWARNING) == IDYES }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fallback_guard_stops_a_second_round() {
        let place = r"::{26EE0668-A00A-44D7-9371-BEB064C98683}";
        let first = record(place, 1_000);
        assert!(seen_recently(&first, place, 1_009));
        assert!(!seen_recently(&first, place, 1_011), "10 s later: a new round");
        assert!(!seen_recently(&first, r"D:\x", 1_001), "another place");
        assert!(!seen_recently("garbage", "", 0) && !seen_recently("", "", 0));
    }

    #[test]
    fn only_places_reach_explorer_and_files_are_only_shown() {
        let is_dir = |p: &Path| matches!(p.to_str(), Some(r"C:\" | r"D:\My Work" | r"D:\My Work\" | r"\\srv\share"));
        let arg = |t: &str| explorer_argument(t, is_dir);
        let some = |s: &str| Some(s.to_owned());
        assert_eq!(arg(""), some(THIS_PC), "Win+E");
        assert_eq!(arg("C:"), some(r"C:\"));
        assert_eq!(arg(r#"C:""#), some(r"C:\"));
        assert_eq!(arg(r"D:\My Work\"), some(r#""D:\My Work""#));
        assert_eq!(arg(r"\\srv\share"), some(r"\\srv\share"));
        assert_eq!(arg(r"\\srv"), some(r"\\srv"), "a server");
        assert_eq!(
            arg(r"D:\a,C:\evil.exe"),
            some(r#"/select,"D:\a,C:\evil.exe""#),
            "commas split Explorer's arguments"
        );
        assert_eq!(arg(r"D:\Docs.library-ms"), some(r"D:\Docs.library-ms"));
        assert_eq!(arg(r"D:\a b\evil.exe"), some(r#"/select,"D:\a b\evil.exe""#), "a file is never run");
        assert_eq!(arg(r"\\?\C:\x.exe"), some(r"/select,\\?\C:\x.exe"));
        let chain = r"::{20D04FE0-3AEA-1069-A2D8-08002B30309D}\::{F02C1A0D-BE21-4350-88B0-7367FC96EF3C}";
        assert_eq!(arg(chain), some(chain));
        assert_eq!(arg("shell:Downloads"), some("shell:Downloads"));
        assert_eq!(
            arg("shell:::{26EE0668-A00A-44D7-9371-BEB064C98683}"),
            some("shell:::{26EE0668-A00A-44D7-9371-BEB064C98683}")
        );
        for refused in [
            "relative",
            "calc.exe",
            "/select,C:\\x",
            r"shell:Downloads\x.exe",
            "shell:",
            r#"D:\a" /root,C:\"#,
            "D:\\a\nb",
            "::{not-a-guid}",
            r"::{26EE0668-A00A-44D7-9371-BEB064C98683}\x.exe",
            "https://example.com",
        ] {
            assert_eq!(arg(refused), None, "{refused}");
        }
    }
}
