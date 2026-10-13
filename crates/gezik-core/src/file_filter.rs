//! The filter of a filtered copy (spec 11 §7.6, §12.2): `*.jpg *.png | thumbs\ *.tmp`.
//!
//! Patterns are split by spaces or `;`. Those before `|` are taken (none: every file), those
//! after it are left out. A pattern ending in `\` or `/` is a folder's: taken, the walk goes
//! down only into such folders (and everything under one); left out, such a folder is never
//! read. `A>B` (copies only): the files B matches that share the stem (the name before its
//! last dot) of a file A takes go along with it. A pattern matches the whole name with `*` and
//! `?`, case ignored as 6a's patterns ignore it.

use crate::pattern::{Token, exact_tokens, glob_case};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileFilter {
    files: Vec<Vec<Token>>,
    folders: Vec<Vec<Token>>,
    skip_files: Vec<Vec<Token>>,
    skip_folders: Vec<Vec<Token>>,
    /// `B` of `A>B`.
    related: Vec<Vec<Token>>,
}

fn any(patterns: &[Vec<Token>], name: &str) -> bool {
    patterns.iter().any(|tokens| glob_case(tokens, name, true))
}

/// The name before its last dot (`IMG_1.jpg` → `IMG_1`); a name starting with its only dot is
/// all stem.
fn stem(name: &str) -> &str {
    match name.rfind('.') {
        None | Some(0) => name,
        Some(at) => &name[..at],
    }
}

/// Folds a stem for comparing: Linux tells `IMG_1` from `img_1`, Windows and macOS do not.
fn stem_key(name: &str) -> String {
    if cfg!(any(windows, target_os = "macos")) { crate::pattern::fold_text(stem(name)) } else { stem(name).to_owned() }
}

impl FileFilter {
    /// Reads `text`; `>` only where `related` is allowed (copies). The error is shown under
    /// the field as it is.
    pub fn parse(text: &str, related: bool) -> Result<FileFilter, String> {
        let (take, leave) = text.split_once('|').unwrap_or((text, ""));
        if leave.contains('|') {
            return Err(gezik_i18n::t!("Only one \"|\": what follows it is left out").into_owned());
        }
        if leave.contains('>') {
            return Err(gezik_i18n::t!("Related files (>) go before \"|\"").into_owned());
        }
        let mut filter = FileFilter::default();
        let (take, along) = match take.split_once('>') {
            Some(_) if !related => return Err(gezik_i18n::t!("Related files (>) work only when copying").into_owned()),
            Some((a, b)) => {
                if b.contains('>') {
                    return Err(gezik_i18n::t!("Only one \">\"").into_owned());
                }
                (a, Some(b))
            }
            None => (take, None),
        };
        words(take, &mut filter.files, &mut filter.folders)?;
        words(leave, &mut filter.skip_files, &mut filter.skip_folders)?;
        if let Some(along) = along {
            let mut folders = Vec::new();
            words(along, &mut filter.related, &mut folders)?;
            if !folders.is_empty() {
                return Err(gezik_i18n::t!("Related files (>) are files, not folders").into_owned());
            }
            if filter.files.is_empty() || filter.related.is_empty() {
                return Err(gezik_i18n::t!("Type a pattern on both sides of \">\": *.jpg>*.cr2").into_owned());
            }
        }
        Ok(filter)
    }

    /// Whether the filter lets everything through.
    pub fn is_empty(&self) -> bool {
        *self == FileFilter::default()
    }

    /// Whether it has `A>B` (the walk then looks at a chosen file's neighbours).
    pub fn has_related(&self) -> bool {
        !self.related.is_empty()
    }

    /// Whether a file of this name is taken by itself (not as a related file).
    pub fn takes_file(&self, name: &str) -> bool {
        (self.files.is_empty() || any(&self.files, name)) && !any(&self.skip_files, name)
    }

    /// A row of a highlight rule (spec 12 §9.1): a file by the file patterns, a folder by the
    /// folder ones (`name/`); patterns of only one sort take nothing of the other.
    pub fn takes(&self, name: &str, is_dir: bool) -> bool {
        if is_dir {
            (self.files.is_empty() || !self.folders.is_empty()) && self.top_folder(name) == Some(true)
        } else {
            (self.folders.is_empty() || !self.files.is_empty()) && self.takes_file(name)
        }
    }

    /// A chosen folder: `None` if it is left out, else whether its files are taken (no taken
    /// folder patterns, or it matches one).
    pub fn top_folder(&self, name: &str) -> Option<bool> {
        if any(&self.skip_folders, name) {
            return None;
        }
        Some(self.folders.is_empty() || any(&self.folders, name))
    }

    /// A folder met on the walk, `inside` a taken folder or not: `None` if the walk does not
    /// go into it, else whether its files are taken.
    pub fn enters(&self, name: &str, inside: bool) -> Option<bool> {
        if any(&self.skip_folders, name) {
            return None;
        }
        (inside || self.folders.is_empty() || any(&self.folders, name)).then_some(true)
    }

    /// Of `others` (the other files of the folder), those going along with the `taken` ones
    /// (`A>B`), in their order.
    pub fn related<'a>(&self, taken: &[&str], others: &[&'a str]) -> Vec<&'a str> {
        if self.related.is_empty() || taken.is_empty() {
            return Vec::new();
        }
        let stems: std::collections::HashSet<String> = taken.iter().map(|name| stem_key(name)).collect();
        others
            .iter()
            .filter(|name| any(&self.related, name) && !any(&self.skip_files, name))
            .filter(|name| stems.contains(&stem_key(name)))
            .copied()
            .collect()
    }
}

/// Splits `text` into file and folder patterns.
fn words(text: &str, files: &mut Vec<Vec<Token>>, folders: &mut Vec<Vec<Token>>) -> Result<(), String> {
    for word in text.split([' ', '\t', ';']).filter(|w| !w.is_empty()) {
        let body = word.trim_end_matches(['/', '\\']);
        if body.contains(['/', '\\']) {
            return Err(gezik_i18n::t!("\"{}\": a folder pattern is one name, like thumbs\\", word));
        }
        if body.is_empty() {
            continue;
        }
        let tokens = exact_tokens(body, true);
        if body.len() < word.len() { folders.push(tokens) } else { files.push(tokens) }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> FileFilter {
        FileFilter::parse(text, true).unwrap()
    }

    #[test]
    fn taken_and_left_out_files() {
        let f = parse("*.jpg *.PNG | *.tmp");
        assert!(f.takes_file("a.jpg") && f.takes_file("B.JPG") && f.takes_file("c.png"));
        assert!(!f.takes_file("a.txt") && !f.takes_file("x.tmp"));
        let f = parse("| *.tmp; *.bak");
        assert!(f.takes_file("a.txt") && !f.takes_file("a.bak") && !f.takes_file("a.tmp"));
        assert!(parse("").is_empty() && parse(" | ").is_empty());
        // A whole name, not a part of one (TC, not the list filter's "anywhere").
        let f = parse("report");
        assert!(f.takes_file("Report") && !f.takes_file("report.pdf"));
        assert!(parse("ırmak.txt").takes_file("IRMAK.TXT"), "6a's folding");
    }

    #[test]
    fn folder_patterns_steer_the_walk() {
        let f = parse("*.jpg | thumbs\\ .git/");
        assert_eq!(f.top_folder("photos"), Some(true));
        assert_eq!(f.top_folder("Thumbs"), None, "a chosen folder left out is not read either");
        assert_eq!(f.enters("thumbs", true), None);
        assert_eq!(f.enters(".git", true), None);
        assert_eq!(f.enters(".github", true), Some(true));
        let f = parse("src\\ | target\\");
        assert_eq!(f.top_folder("work"), Some(false), "entered, its own files not taken");
        assert_eq!(f.enters("docs", false), None, "only into taken folders");
        assert_eq!(f.enters("src", false), Some(true));
        assert_eq!(f.enters("deep", true), Some(true), "under a taken folder: everything");
        assert_eq!(f.enters("target", true), None);
        assert!(f.takes_file("main.rs"), "no file pattern: every file (inside a taken folder)");
        assert_eq!(f.top_folder("src"), Some(true));
    }

    #[test]
    fn related_files_share_the_stem() {
        let f = parse("*.jpg>*.cr2 *.xmp | *.tmp");
        assert!(f.takes_file("IMG_1.jpg") && !f.takes_file("IMG_1.cr2"));
        let others = ["IMG_1.cr2", "IMG_1.xmp", "IMG_2.cr2", "IMG_1.txt", "IMG_1.jpg.cr2"];
        assert_eq!(f.related(&["IMG_1.jpg"], &others), ["IMG_1.cr2", "IMG_1.xmp"]);
        assert!(f.related(&[], &others).is_empty());
        // The pattern ignores case; the stem does where the system does.
        let upper = f.related(&["IMG_1.jpg"], &["IMG_1.CR2", "img_1.cr2"]);
        if cfg!(any(windows, target_os = "macos")) {
            assert_eq!(upper, ["IMG_1.CR2", "img_1.cr2"]);
        } else {
            assert_eq!(upper, ["IMG_1.CR2"], "Linux: img_1 is another stem");
        }
        assert!(parse("*.jpg").related(&["a.jpg"], &["a.cr2"]).is_empty());
        // The last dot: `a.b.jpg`'s stem is `a.b`.
        assert_eq!(f.related(&["a.b.jpg"], &["a.b.cr2", "a.cr2"]), ["a.b.cr2"]);
    }

    #[test]
    fn mistakes_are_said() {
        let err = |text: &str| FileFilter::parse(text, true).unwrap_err();
        assert!(FileFilter::parse("*.jpg>*.cr2", false).unwrap_err().contains("only when copying"));
        assert!(err("a | b | c").contains("Only one"));
        assert!(err("*.jpg>*.cr2>*.xmp").contains("Only one"));
        assert!(err(">*.cr2").contains("both sides"));
        assert!(err("*.jpg>").contains("both sides"));
        assert!(err("*.jpg>raw\\").contains("not folders"));
        assert!(err("| *.jpg>*.cr2").contains("before"));
        assert!(err("a\\b\\").contains("one name"));
    }
}
