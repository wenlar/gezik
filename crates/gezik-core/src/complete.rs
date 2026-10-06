//! The address bar's suggestions (spec 6.1): where the last part of a typed path starts, and
//! which sub-folder names fit it, in what order. Pure; the app reads the folder on a thread.

use crate::pattern::fold_text;

/// At most this many folders are suggested.
pub const MAX_SUGGESTIONS: usize = 12;

/// `C:` and the like.
fn is_drive(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

/// `text` (already expanded) split at its last separator: the folder part, with its
/// separator (empty for a name alone), and the start of a name in it. On Windows `X:` alone
/// is that drive's root.
pub fn split_typed(text: &str, windows: bool) -> (String, String) {
    if windows && is_drive(text) {
        return (format!("{text}\\"), String::new());
    }
    let separators: &[char] = if windows { &['/', '\\'] } else { &['/'] };
    match text.rfind(separators) {
        Some(i) => (text[..=i].to_owned(), text[i + 1..].to_owned()),
        None => (String::new(), text.to_owned()),
    }
}

/// Whether the typed text asks for the history rather than a folder's names (spec 6.2):
/// nothing, `~` or a root (`/`; on Windows also `\`, `C:`, `C:\`, `C:/`).
pub fn shows_history(text: &str, windows: bool) -> bool {
    let text = text.trim();
    if matches!(text, "" | "~" | "/") {
        return true;
    }
    let bytes = text.as_bytes();
    let drive_root =
        bytes.len() == 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && matches!(bytes[2], b'\\' | b'/');
    windows && (text == "\\" || is_drive(text) || drive_root)
}

/// Which of `names` (in their order) fit `typed`, best first: those starting with it, then
/// those holding it; case and the Turkish i ignored. A name starting with a dot only when
/// `typed` does. At most `MAX_SUGGESTIONS`.
pub fn rank(names: &[String], typed: &str) -> Vec<usize> {
    let needle = fold_text(typed);
    let dots = typed.starts_with('.');
    let mut starts = Vec::new();
    let mut holds = Vec::new();
    for (i, name) in names.iter().enumerate() {
        if name.starts_with('.') && !dots {
            continue;
        }
        let folded = fold_text(name);
        if folded.starts_with(&needle) {
            starts.push(i);
        } else if folded.contains(&needle) {
            holds.push(i);
        }
    }
    starts.extend(holds);
    starts.truncate(MAX_SUGGESTIONS);
    starts
}

/// Folder names in the order suggestions list them: case and the Turkish i ignored, then as
/// written.
pub fn sort_names(names: &mut [String]) {
    names.sort_by_cached_key(|name| (fold_text(name), name.clone()));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    fn pair(a: &str, b: &str) -> (String, String) {
        (a.to_owned(), b.to_owned())
    }

    #[test]
    fn the_last_part_is_split_off() {
        assert_eq!(split_typed("/home/ali/Pro", false), pair("/home/ali/", "Pro"));
        assert_eq!(split_typed("/home/ali/", false), pair("/home/ali/", ""));
        assert_eq!(split_typed("Pro", false), pair("", "Pro"));
        assert_eq!(split_typed(r"a\b", false), pair("", r"a\b"), "a backslash is part of a name on Unix");
        assert_eq!(split_typed(r"C:\Users\al", true), pair(r"C:\Users\", "al"));
        assert_eq!(split_typed("C:/Users/al", true), pair("C:/Users/", "al"));
        assert_eq!(split_typed("D:", true), pair(r"D:\", ""));
        assert_eq!(split_typed(r"\\server\share\do", true), pair(r"\\server\share\", "do"));
    }

    #[test]
    fn history_shows_for_nothing_home_or_a_root() {
        for text in ["", "  ", "~", "/"] {
            assert!(shows_history(text, false), "{text:?}");
        }
        for text in ["\\", "C:", "c:\\", "D:/"] {
            assert!(shows_history(text, true), "{text:?}");
        }
        for text in ["~/", "/a", "C:", "aş", "ş/"] {
            assert!(!shows_history(text, false), "{text:?}");
        }
        assert!(!shows_history(r"C:\a", true));
        assert!(!shows_history("şx", true), "three bytes, no drive");
    }

    #[test]
    fn names_starting_with_the_text_come_first() {
        let list = names(&["Alpha", "Beta", "alpine", "Kalabalık", ".alcohol", "İndirilenler"]);
        let shown = |typed: &str| rank(&list, typed).into_iter().map(|i| list[i].clone()).collect::<Vec<_>>();
        assert_eq!(shown("al"), ["Alpha", "alpine", "Kalabalık"]);
        assert_eq!(shown("ind"), ["İndirilenler"]);
        assert_eq!(shown(".al"), [".alcohol"]);
        assert_eq!(shown("").len(), 5, "everything but the dot name");
        assert!(shown("zz").is_empty());
    }

    #[test]
    fn at_most_twelve() {
        let list: Vec<String> = (0..30).map(|i| format!("dir{i:02}")).collect();
        assert_eq!(rank(&list, "dir").len(), MAX_SUGGESTIONS);
        assert_eq!(rank(&list, "dir")[0], 0);
    }

    #[test]
    fn names_sort_ignoring_case() {
        let mut list = names(&["beta", "Alpha", "İz", "alpine", "Alpha2"]);
        sort_names(&mut list);
        assert_eq!(list, ["Alpha", "Alpha2", "alpine", "beta", "İz"]);
    }
}
