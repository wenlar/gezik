//! Names of dropped items that come from another program (spec 9 §8.1): an e-mail
//! attachment's, a browser's picture's, a zip view's file's, a macOS file promise's. They are
//! untrusted: each becomes a relative path to create under the drop folder, or is refused.

use std::path::PathBuf;

pub const EMPTY: &str = "The program gave the item no name";
pub const CONTROL: &str = "The item's name holds control characters";
pub const ABSOLUTE: &str = "The item's name is a full path";
pub const PARTS: &str = "The item's name has an empty, \".\" or \"..\" part";

/// The longest part: 255 UTF-16 units on Windows, 255 bytes elsewhere.
const MOST: usize = 255;
/// A cut name keeps an extension of up to this many characters.
const LONGEST_EXTENSION: usize = 16;

/// `name` as a path relative to the drop folder (`Ekler\a.txt`: a folder and a file in it), or
/// why it is refused: an absolute path, a drive or server, an empty, `.` or `..` part, a
/// control character. Bidirectional marks and invisible characters become `_` (a name must not
/// read backwards). On Windows (`windows`) `\` and `/` both separate, characters Windows does
/// not allow become `_`, trailing dots and spaces go (a part left empty is refused) and
/// reserved names get a `_` in front (`CON` → `_CON`); elsewhere only `/` separates. Each part
/// is cut to 255 units, keeping a short extension. Different names can come out the same
/// (`a<b`, `a>b`): the caller never writes over what is there.
pub fn safe_relative(name: &str, windows: bool) -> Result<PathBuf, &'static str> {
    if name.chars().any(char::is_control) {
        return Err(CONTROL);
    }
    let separators: &[char] = if windows { &['\\', '/'] } else { &['/'] };
    if name.starts_with(separators) {
        return Err(ABSOLUTE);
    }
    let mut chars = name.chars();
    if windows && chars.next().is_some_and(|c| c.is_ascii_alphabetic()) && chars.next() == Some(':') {
        return Err(ABSOLUTE);
    }
    let name = name.trim_end_matches(separators);
    if name.is_empty() {
        return Err(EMPTY);
    }
    let mut path = PathBuf::new();
    for part in name.split(separators) {
        let part = if windows { part.trim_end_matches(['.', ' ']) } else { part };
        if part.is_empty() || part == "." || part == ".." {
            return Err(PARTS);
        }
        let part: String = part.chars().map(|c| if is_hidden(c) { '_' } else { c }).collect();
        path.push(if windows { windows_part(&part) } else { cut(&part, false) });
    }
    Ok(path)
}

/// Bidirectional marks and overrides, line and paragraph separators, zero-width spaces: they
/// hide what a name says. The zero-width (non-)joiners stay: emoji and Persian spell with them.
fn is_hidden(c: char) -> bool {
    matches!(
        c,
        '\u{202A}'..='\u{202E}'
            | '\u{2066}'..='\u{2069}'
            | '\u{200B}'
            | '\u{200E}'
            | '\u{200F}'
            | '\u{061C}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{FEFF}'
    )
}

/// `part` with what Windows does not allow in a name made `_`, cut, and a reserved name
/// (`CON`, `com1.txt`) given a `_` in front. Reserved is checked after the cut: a cut can end
/// a name in spaces Windows drops (`CON   x.txt` → `CON.txt`).
fn windows_part(part: &str) -> String {
    let clean: String =
        part.chars().map(|c| if matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*') { '_' } else { c }).collect();
    let clean = cut(&clean, true);
    let base = clean.split('.').next().unwrap_or_default().trim_end_matches(' ');
    if is_reserved(base) { cut(&format!("_{clean}"), true) } else { clean }
}

fn is_reserved(base: &str) -> bool {
    let upper = base.to_ascii_uppercase();
    matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$")
        || ["COM", "LPT"].iter().any(|prefix| {
            upper.strip_prefix(prefix).is_some_and(|rest| {
                matches!(rest, "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³")
            })
        })
}

/// `part` cut to 255 units (UTF-16 on Windows, bytes elsewhere) at a character's end,
/// keeping an extension of up to 16 characters.
fn cut(part: &str, windows: bool) -> String {
    let units = |text: &str| if windows { text.encode_utf16().count() } else { text.len() };
    if units(part) <= MOST {
        return part.to_owned();
    }
    // `at` is where `rfind` found a `.`: a character boundary, so the split cannot panic.
    let (stem, extension) = match part.rfind('.') {
        Some(at) if at > 0 && part[at..].chars().count() <= LONGEST_EXTENSION + 1 => part.split_at(at),
        _ => (part, ""),
    };
    let room = MOST - units(extension);
    let mut kept = String::new();
    let mut used = 0;
    for c in stem.chars() {
        let size = if windows { c.len_utf16() } else { c.len_utf8() };
        if used + size > room {
            break;
        }
        used += size;
        kept.push(c);
    }
    if windows {
        // Windows would drop them, and the name on disk would not be the one checked.
        let end = kept.trim_end_matches(['.', ' ']).len();
        kept.truncate(end);
    }
    if kept.is_empty() && extension.is_empty() {
        kept.push('_');
    }
    kept + extension
}

/// An internet shortcut a browser makes of a dragged link (`.url`, `.webloc`): a link, not a
/// file (spec 9 §17 decision 22).
pub fn is_link_file(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".url") || lower.ends_with(".webloc")
}

/// `name` as a failure in the job's report names it: separators, `:`, control and hidden
/// characters made `_`, never empty, `.` or `..` (it is joined to the drop folder only to be
/// shown).
pub fn shown(name: &str) -> String {
    let text: String = name
        .chars()
        .map(|c| if matches!(c, '/' | '\\' | ':') || c.is_control() || is_hidden(c) { '_' } else { c })
        .collect();
    if matches!(text.as_str(), "" | "." | "..") { "_".to_owned() } else { text }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn ok(name: &str, windows: bool) -> PathBuf {
        safe_relative(name, windows).unwrap_or_else(|why| panic!("{name:?}: {why}"))
    }

    fn path(parts: &[&str]) -> PathBuf {
        parts.iter().collect()
    }

    #[test]
    fn plain_names_and_folders_pass() {
        for windows in [true, false] {
            assert_eq!(ok("rapor.pdf", windows), path(&["rapor.pdf"]));
            assert_eq!(ok("Fotoğraf çekimi.jpg", windows), path(&["Fotoğraf çekimi.jpg"]));
            assert_eq!(ok("Ekler/a.txt", windows), path(&["Ekler", "a.txt"]));
            assert_eq!(ok("Ekler/", windows), path(&["Ekler"]), "a trailing separator is no part");
            assert_eq!(ok(" lead", windows), path(&[" lead"]));
            assert_eq!(ok(".gitignore", windows), path(&[".gitignore"]));
            assert_eq!(
                ok("👨\u{200D}👩\u{200D}👧.png", windows),
                path(&["👨\u{200D}👩\u{200D}👧.png"]),
                "joiners stay"
            );
        }
        assert_eq!(ok(r"Ekler\iç\a.txt", true), path(&["Ekler", "iç", "a.txt"]));
        assert_eq!(ok(r"a\b.txt", false), path(&[r"a\b.txt"]), "elsewhere a backslash is a character");
        assert_eq!(ok("a:b", false), path(&["a:b"]));
        assert_eq!(ok("C:x", false), path(&["C:x"]), "no drives elsewhere");
        assert_eq!(ok("CON", false), path(&["CON"]));
        assert_eq!(ok("a.", false), path(&["a."]));
        assert_eq!(ok("1:x", true), path(&["1_x"]), "only a letter makes a drive");
    }

    #[test]
    fn names_that_leave_the_folder_are_refused() {
        for windows in [true, false] {
            for name in [
                "",
                "/abs.txt",
                "..",
                "../x",
                "a/../../x",
                "a/..",
                ".",
                "./x",
                "a//b",
                "a/./b",
                "x\0y",
                "x\u{7}y",
                "x\ny",
                "x\u{85}y",
                "/",
                "//",
                "///x",
            ] {
                assert!(safe_relative(name, windows).is_err(), "{name:?} (windows: {windows})");
            }
        }
        for name in [
            r"\x",
            r"C:\x",
            "C:x",
            "c:",
            "z:/x",
            r"\\server\share\x",
            r"\\?\C:\x",
            r"a\..\..\x",
            r"a/..\..\x",
            "...",
            ". .",
            r"a\ \b",
            r"a\.\b",
            r"a\..",
            r"a\\b",
            r"..\x",
            r"a\...\b",
        ] {
            assert!(safe_relative(name, true).is_err(), "{name:?}");
        }
        assert_eq!(safe_relative("", true), Err(EMPTY));
        assert_eq!(safe_relative("/", false), Err(ABSOLUTE));
        assert_eq!(safe_relative(r"C:\x", true), Err(ABSOLUTE));
        assert_eq!(safe_relative("x\u{7}", true), Err(CONTROL));
        assert_eq!(safe_relative("a/../b", false), Err(PARTS));
    }

    #[test]
    fn whatever_passes_stays_under_the_folder() {
        use std::path::Component;
        let names = [
            "a",
            "a/b",
            r"a\b",
            "..a",
            "a..",
            "a/..b",
            "..../x",
            "x/....",
            "C:",
            ":x",
            "~",
            "~/x",
            "$HOME",
            r"%TEMP%\x",
            "a.txt:evil",
            "a\u{202E}gpj.exe",
            "COM1/x",
        ];
        // The host reads the path: `C:` is a drive only to Windows, which never gets `windows: false`.
        let windows = cfg!(windows);
        for name in names {
            if let Ok(relative) = safe_relative(name, windows) {
                assert!(relative.components().all(|c| matches!(c, Component::Normal(_))), "{name:?} -> {relative:?}");
                assert!(relative.components().count() > 0);
            }
        }
    }

    #[test]
    fn windows_names_are_made_safe() {
        assert_eq!(ok(r#"a<b>:"|?*.txt"#, true), path(&["a_b______.txt"]));
        assert_eq!(ok("CON", true), path(&["_CON"]));
        assert_eq!(ok("con.txt", true), path(&["_con.txt"]));
        assert_eq!(ok("CON .txt", true), path(&["_CON .txt"]));
        assert_eq!(ok("con.tar.gz", true), path(&["_con.tar.gz"]));
        assert_eq!(ok("COM1.log", true), path(&["_COM1.log"]));
        assert_eq!(ok("com0", true), path(&["_com0"]));
        assert_eq!(ok("LPT9.txt", true), path(&["_LPT9.txt"]));
        assert_eq!(ok("lpt¹", true), path(&["_lpt¹"]));
        assert_eq!(ok("COM²", true), path(&["_COM²"]));
        assert_eq!(ok("com³.x", true), path(&["_com³.x"]));
        assert_eq!(ok("CONIN$", true), path(&["_CONIN$"]));
        assert_eq!(ok("conout$.txt", true), path(&["_conout$.txt"]));
        assert_eq!(ok("PRN", true), path(&["_PRN"]));
        assert_eq!(ok("aux", true), path(&["_aux"]));
        assert_eq!(ok("Klasör/nul", true), path(&["Klasör", "_nul"]));
        assert_eq!(ok("NUL./x", true), path(&["_NUL", "x"]), "a reserved folder, trailing dot gone");
        assert_eq!(ok("CONSOLE.txt", true), path(&["CONSOLE.txt"]));
        assert_eq!(ok("COM10", true), path(&["COM10"]));
        assert_eq!(ok("COM⁴", true), path(&["COM⁴"]));
        assert_eq!(ok("_CON", true), path(&["_CON"]));
        assert_eq!(ok("name. . ", true), path(&["name"]), "trailing dots and spaces go");
        assert_eq!(ok("a.", true), path(&["a"]));
        assert_eq!(ok("a.txt:evil", true), path(&["a.txt_evil"]), "no alternate stream");
        assert_eq!(ok("a.txt::$DATA", true), path(&["a.txt__$DATA"]));
    }

    #[test]
    fn hidden_marks_are_made_visible() {
        for windows in [true, false] {
            assert_eq!(ok("report\u{202E}fdp.exe", windows), path(&["report_fdp.exe"]), "no name read backwards");
            assert_eq!(ok("a\u{2066}b\u{2069}", windows), path(&["a_b_"]));
            assert_eq!(ok("\u{FEFF}a\u{200B}b\u{200E}c\u{061C}d", windows), path(&["_a_b_c_d"]));
            assert_eq!(ok("a\u{2028}b", windows), path(&["a_b"]));
            assert_eq!(ok("می\u{200C}خواهم", windows), path(&["می\u{200C}خواهم"]), "a non-joiner is spelling");
        }
    }

    #[test]
    fn long_names_are_cut_keeping_the_extension() {
        let long = format!("{}.docx", "ş".repeat(300));
        let cut = ok(&long, true).to_string_lossy().into_owned();
        assert!(cut.encode_utf16().count() <= 255 && cut.ends_with(".docx"), "{cut}");
        let cut = ok(&long, false).to_string_lossy().into_owned();
        assert!(cut.len() <= 255 && cut.ends_with(".docx"), "bytes, whole characters: {cut}");
        let no_extension = "a".repeat(400);
        assert_eq!(ok(&no_extension, true).to_string_lossy().len(), 255);
        let long_extension = format!("a.{}", "b".repeat(300));
        assert_eq!(ok(&long_extension, false).to_string_lossy().len(), 255, "a long extension is cut like the rest");
        let exact = "a".repeat(255);
        assert_eq!(ok(&exact, true), path(&[&exact]), "255 is not cut");
        // Astral characters are two UTF-16 units and never split.
        let astral = format!("{}.jpg", "😀".repeat(200));
        let cut = ok(&astral, true).to_string_lossy().into_owned();
        assert_eq!(cut.encode_utf16().count(), 254, "{cut}");
        assert!(cut.ends_with(".jpg"));
        // Each folder part is cut on its own.
        let deep = ok(&format!("{}/{}.txt", "d".repeat(300), "f".repeat(300)), true);
        assert!(deep.iter().all(|part| part.to_string_lossy().encode_utf16().count() <= 255));
        assert!(deep.to_string_lossy().ends_with(".txt"));
    }

    #[test]
    fn a_cut_never_makes_a_name_windows_would_change() {
        // Cut in the middle of spaces: what is left must not end in one, nor become `CON.txt`.
        let spaced = format!("CON{}x.txt", " ".repeat(300));
        assert_eq!(ok(&spaced, true), path(&["_CON.txt"]));
        let dotted = format!("a{}b", ".".repeat(300));
        let cut = ok(&dotted, true).to_string_lossy().into_owned();
        assert!(!cut.ends_with('.') && cut.encode_utf16().count() <= 255, "{cut}");
        let reserved_long = format!("con.{}", "x".repeat(300));
        let cut = ok(&reserved_long, true).to_string_lossy().into_owned();
        assert!(cut.starts_with("_con.") && cut.encode_utf16().count() <= 255, "{cut}");
    }

    #[test]
    fn different_names_can_meet_after_cleaning() {
        // They are not told apart here: the job's no-overwrite rule and conflict list meet them.
        assert_eq!(ok("a<b.txt", true), ok("a>b.txt", true));
        assert_eq!(ok(r"Ekler\a", true), ok("Ekler/a", true));
        assert_eq!(ok("a.", true), ok("a", true));
        assert_eq!(ok(&format!("{}1.txt", "a".repeat(300)), true), ok(&format!("{}2.txt", "a".repeat(300)), true));
    }

    #[test]
    fn link_files_are_known() {
        assert!(is_link_file("Gezik – GitHub.url"));
        assert!(is_link_file("page.URL"));
        assert!(is_link_file("x.webloc"));
        assert!(is_link_file("x.WebLoc"));
        assert!(!is_link_file("curl"));
        assert!(!is_link_file("rapor.pdf"));
        assert!(!is_link_file("url"));
    }

    #[test]
    fn shown_names_hold_no_path() {
        assert_eq!(shown(r"..\..\x"), ".._.._x");
        assert_eq!(shown("C:/x\u{7}"), "C__x_");
        assert_eq!(shown("a\u{202E}b"), "a_b");
        assert_eq!(shown(""), "_");
        assert_eq!(shown("."), "_");
        assert_eq!(shown(".."), "_");
    }
}
