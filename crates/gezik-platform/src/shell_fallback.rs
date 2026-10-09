//! Windows: handing a place Gezik cannot show to Explorer (spec 6.3; decisions 2, 3). Not
//! through a shell verb: another file manager may own `Folder\shell\open` in HKCU (the 9b4
//! probe found Files there), so `explorer.exe` is started by its full path. Any app can start
//! `gezik --shell X`, so only folder-like places reach Explorer as they are; any other file
//! goes as `/select,`, which shows it and never runs it. Fallbacks are counted across
//! processes in a file, whatever the place, so no form of it can make an endless loop.

use std::io;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use ::windows::Win32::System::SystemInformation::GetSystemWindowsDirectoryW;
use ::windows::Win32::UI::WindowsAndMessaging::{IDYES, MB_ICONWARNING, MB_YESNO, MessageBoxW};
use ::windows::core::{PCWSTR, w};

const THIS_PC: &str = "::{20D04FE0-3AEA-1069-A2D8-08002B30309D}";
/// `%TEMP%\gezik-fallback.txt`: when the last fallbacks were (unix seconds, a line each).
pub const GUARD_FILE: &str = "gezik-fallback.txt";
const WINDOW_S: u64 = 10;
/// Enough to tell the third fallback in the window from later ones.
const KEEP: usize = 4;

/// What one fallback may do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Guard {
    /// Hand the place to Explorer.
    Go,
    /// A loop: hand nothing on, offer to restore Explorer (once per loop).
    Ask,
    /// Still the same loop: hand nothing on, ask nothing.
    Stop,
}

/// The guard file's next text and what this fallback (at `now`) may do, from the file's text
/// (`record`; garbage and old or future times are dropped). Two fallbacks in 10 s go on (one
/// opening places quickly); a third is a loop between Gezik and Explorer, whatever the places.
pub fn guard(record: &str, now: u64) -> (String, Guard) {
    let mut recent: Vec<u64> = record
        .lines()
        .filter_map(|line| line.trim().parse().ok())
        .filter(|&when| when <= now && now - when < WINDOW_S)
        .collect();
    recent.push(now);
    let guard = match recent.len() {
        ..=2 => Guard::Go,
        3 => Guard::Ask,
        _ => Guard::Stop,
    };
    let text: Vec<String> = recent.iter().rev().take(KEEP).rev().map(u64::to_string).collect();
    (text.join("\n"), guard)
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
        // `shell:Common Startup` is one argument.
        let quoted = if name.contains(' ') { format!("\"{text}\"") } else { text.to_owned() };
        return (plain || clsids(name)).then_some(quoted);
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
    // Explorer reads its own command line: the argument goes as is.
    Command::new(windows_dir()?.join("explorer.exe")).raw_arg(argument).spawn().map(drop)
}

/// The Windows folder, from the system rather than the inherited environment.
fn windows_dir() -> io::Result<PathBuf> {
    let mut buffer = [0u16; 260];
    // SAFETY: the slice carries its own length.
    let len = unsafe { GetSystemWindowsDirectoryW(Some(&mut buffer)) } as usize;
    if len == 0 || len >= buffer.len() {
        return Err(io::Error::last_os_error());
    }
    Ok(PathBuf::from(String::from_utf16_lossy(&buffer[..len])))
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

    /// Runs fallbacks at `times` from an empty file; what each may do.
    fn rounds(times: &[u64]) -> Vec<Guard> {
        let mut file = String::new();
        times
            .iter()
            .map(|&now| {
                let (next, guard) = guard(&file, now);
                assert!(next.lines().count() <= KEEP, "the file stays tiny");
                file = next;
                guard
            })
            .collect()
    }

    #[test]
    fn a_loop_in_any_form_is_bounded_and_asked_about_once() {
        // `::{X}`, `shell:::{X}`, `A`, `B`, ...: the places do not matter, only the count.
        let mut want = vec![Guard::Go, Guard::Go, Guard::Ask];
        want.extend([Guard::Stop; 5]);
        assert_eq!(rounds(&[1_000; 8]), want);
    }

    #[test]
    fn two_quick_opens_go_on_and_old_ones_expire() {
        assert_eq!(rounds(&[1_000, 1_009]), [Guard::Go, Guard::Go], "the same place twice in 10 s");
        assert_eq!(rounds(&[1_000, 1_005, 1_010, 1_015, 1_020]), [Guard::Go; 5], "older than 10 s: gone");
        assert_eq!(rounds(&[1_000, 1_001, 1_002, 1_020]), [Guard::Go, Guard::Go, Guard::Ask, Guard::Go], "a new round");
    }

    #[test]
    fn a_corrupt_or_missing_guard_file_is_an_empty_one() {
        assert_eq!(guard("", 50), ("50".to_owned(), Guard::Go));
        assert_eq!(guard("garbage\n\u{0}\n-3\n99999999999999999999", 50).1, Guard::Go);
        assert_eq!(guard("999\n999", 50).1, Guard::Go, "times after now (a clock set back) are dropped");
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
        assert_eq!(arg("shell:Common Startup"), some(r#""shell:Common Startup""#));
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
