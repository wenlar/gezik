//! Git status (spec 16 §8.3): `git status --porcelain=v2 -z --branch` read into marks, and the
//! marks of the names in the folder shown, a folder's being the most important one inside it.
//! Pure: the run itself is the app's (`gezik/src/git.rs`).

use std::borrow::Cow;
use std::collections::HashMap;

use gezik_i18n::{t, tn, tx};

/// A row's Git mark; the numbers are the row's `git` in the UI, a higher one more important
/// inside a folder (no mark: 0).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum GitMark {
    /// Drawn faint; never counts for its folder.
    Ignored = 1,
    Untracked = 2,
    Added = 3,
    Renamed = 4,
    Staged = 5,
    Modified = 6,
    StagedModified = 7,
    Conflict = 8,
}

impl GitMark {
    /// The Git column's text and the group's name (translated where shown, context `git`).
    pub fn label(self) -> &'static str {
        match self {
            GitMark::Ignored => tx!("git" => "Ignored"),
            GitMark::Untracked => tx!("git" => "Untracked"),
            GitMark::Added => tx!("git" => "Added"),
            GitMark::Renamed => tx!("git" => "Renamed"),
            GitMark::Staged => tx!("git" => "Staged"),
            GitMark::Modified => tx!("git" => "Modified"),
            GitMark::StagedModified => tx!("git" => "Staged, modified"),
            GitMark::Conflict => tx!("git" => "Conflict"),
        }
    }

    /// What it makes of the folder it is in: conflict > modified > staged > added > untracked;
    /// an ignored file nothing.
    pub fn for_folder(self) -> Option<GitMark> {
        match self {
            GitMark::Ignored => None,
            GitMark::Modified | GitMark::StagedModified => Some(GitMark::Modified),
            GitMark::Staged | GitMark::Renamed => Some(GitMark::Staged),
            other => Some(other),
        }
    }
}

/// The place of a row's mark in a sort or among the groups: conflicts first, then changes,
/// untracked, unchanged, ignored last.
pub fn rank(mark: Option<GitMark>) -> u8 {
    match mark {
        Some(GitMark::Conflict) => 0,
        Some(GitMark::Modified) => 1,
        Some(GitMark::StagedModified) => 2,
        Some(GitMark::Staged) => 3,
        Some(GitMark::Renamed) => 4,
        Some(GitMark::Added) => 5,
        Some(GitMark::Untracked) => 6,
        None => 7,
        Some(GitMark::Ignored) => 8,
    }
}

/// The text of a row's mark (empty: unchanged), translated.
pub fn label(mark: Option<GitMark>) -> String {
    mark.map(|m| gezik_i18n::t("git", m.label()).into_owned()).unwrap_or_default()
}

/// What one `git status` said.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitStatus {
    /// The branch checked out; `None` when HEAD is detached.
    pub branch: Option<String>,
    /// HEAD's commit; `None` before the first one.
    pub oid: Option<String>,
    /// Commits ahead of and behind the upstream (as of the last fetch); `None` without one.
    pub ahead_behind: Option<(u32, u32)>,
    /// Paths from the repository's root with `/`, a folder's without its last `/`.
    pub entries: Vec<(String, GitMark)>,
    /// Paths deleted in the work tree or the index (no row shows them).
    pub deleted: Vec<String>,
}

/// Reads `git status --porcelain=v2 -z --branch` output. Lines it does not know are passed over.
pub fn parse_status(out: &[u8]) -> GitStatus {
    let mut status = GitStatus::default();
    let mut records = out.split(|b| *b == 0).map(String::from_utf8_lossy);
    while let Some(line) = records.next() {
        let path = |rest: Option<&str>| rest.unwrap_or("").trim_end_matches('/').to_owned();
        let xy = |field: Option<&str>| {
            let b = field.unwrap_or("..").as_bytes();
            (b.first().copied().unwrap_or(b'.'), b.get(1).copied().unwrap_or(b'.'))
        };
        match line.as_bytes().first() {
            Some(b'#') => {
                let mut words = line[1..].trim_start().splitn(2, ' ');
                let (key, value) = (words.next().unwrap_or(""), words.next().unwrap_or("").trim());
                match key {
                    "branch.oid" => status.oid = (value != "(initial)").then(|| value.to_owned()),
                    "branch.head" => status.branch = (value != "(detached)").then(|| value.to_owned()),
                    "branch.ab" => {
                        let mut n = value.split(' ').map(|w| w.trim_start_matches(['+', '-']).parse::<u32>().ok());
                        if let (Some(Some(ahead)), Some(Some(behind))) = (n.next(), n.next()) {
                            status.ahead_behind = Some((ahead, behind));
                        }
                    }
                    _ => {}
                }
            }
            Some(b'1') => {
                let mut f = line.splitn(9, ' ');
                let (x, y) = xy(f.nth(1));
                status.push(path(f.nth(6)), changed(x, y, false));
            }
            Some(b'2') => {
                let mut f = line.splitn(10, ' ');
                let (x, y) = xy(f.nth(1));
                status.push(path(f.nth(7)), changed(x, y, true));
                // The path it came from, a record of its own.
                records.next();
            }
            Some(b'u') => {
                let mut f = line.splitn(11, ' ');
                status.entries.push((path(f.nth(10)), GitMark::Conflict));
            }
            Some(b'?') => status.entries.push((path(line.get(2..)), GitMark::Untracked)),
            Some(b'!') => status.entries.push((path(line.get(2..)), GitMark::Ignored)),
            _ => {}
        }
    }
    status
}

impl GitStatus {
    fn push(&mut self, path: String, mark: Option<GitMark>) {
        match mark {
            Some(mark) => self.entries.push((path, mark)),
            None => self.deleted.push(path),
        }
    }
}

/// The mark of a changed tracked entry from its index (`x`) and work tree (`y`) letters;
/// `None`: deleted in either (not on disk).
fn changed(x: u8, y: u8, renamed: bool) -> Option<GitMark> {
    if x == b'D' || y == b'D' {
        return None;
    }
    let (staged, modified) = (x != b'.', y != b'.');
    Some(match () {
        _ if x == b'A' && !modified => GitMark::Added,
        _ if renamed && !modified => GitMark::Renamed,
        _ if staged && modified => GitMark::StagedModified,
        _ if staged => GitMark::Staged,
        _ => GitMark::Modified,
    })
}

/// The marks of the names in one folder of the repository.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FolderMarks {
    /// Names with a mark only.
    pub names: HashMap<Box<str>, GitMark>,
    /// The folder itself is untracked or ignored as a whole: every row is.
    pub whole: Option<GitMark>,
    /// Paths under the folder (from it, with `/`) deleted.
    pub deleted: Vec<String>,
    /// Changes under the folder: entries not ignored, and the deleted.
    pub changed: usize,
    /// Conflicts under the folder.
    pub conflicts: usize,
    /// How names are folded (`fold_for`): keys and the names looked up alike.
    case: bool,
    nfc: bool,
}

impl FolderMarks {
    /// Name `name`'s mark (`None`: unchanged).
    pub fn of(&self, name: &str) -> Option<GitMark> {
        self.whole.or_else(|| self.names.get(&*fold_for(name, self.case, self.nfc)).copied())
    }
}

/// A name as `case` and `nfc` fold it: lower case where the file system ignores case (a name on
/// disk may differ in case from the index's), composed (NFC) on macOS, where git composes the
/// names it reads and a name on disk may be decomposed.
pub fn fold_for(name: &str, case: bool, nfc: bool) -> Cow<'_, str> {
    let mut out = Cow::Borrowed(name);
    if nfc && !name.is_ascii() {
        out = Cow::Owned(icu_normalizer::ComposingNormalizerBorrowed::new_nfc().normalize(name).into_owned());
    }
    if case && out.chars().any(char::is_uppercase) {
        out = Cow::Owned(out.to_lowercase());
    }
    out
}

/// The marks of `status` for the folder `prefix` (from the root, with `/`; "" for the root),
/// names folded as this system's file systems compare them.
pub fn folder_marks(status: &GitStatus, prefix: &str) -> FolderMarks {
    let macos = cfg!(target_os = "macos");
    folder_marks_for(status, prefix, cfg!(windows) || macos, macos)
}

/// `folder_marks` with names folded by `fold_for(_, case, nfc)`.
pub fn folder_marks_for(status: &GitStatus, prefix: &str, case: bool, nfc: bool) -> FolderMarks {
    let mut out = FolderMarks { case, nfc, ..FolderMarks::default() };
    let prefix = &*fold_for(prefix, case, nfc);
    let under = |path: &str| -> Option<String> {
        if prefix.is_empty() {
            return Some(path.to_owned());
        }
        path.strip_prefix(prefix).and_then(|rest| rest.strip_prefix('/')).map(str::to_owned)
    };
    for (path, mark) in &status.entries {
        let path = &fold_for(path, case, nfc).into_owned();
        // The folder shown is the entry or inside it (an ignored `build/` opened).
        let contains = path == prefix || prefix.strip_prefix(path.as_str()).is_some_and(|r| r.starts_with('/'));
        if !path.is_empty() && contains {
            out.whole = Some(*mark);
            out.changed += usize::from(*mark != GitMark::Ignored);
            continue;
        }
        let Some(rest) = under(path) else { continue };
        out.changed += usize::from(*mark != GitMark::Ignored);
        out.conflicts += usize::from(*mark == GitMark::Conflict);
        match rest.split_once('/') {
            None => {
                out.names.insert(rest.into(), *mark);
            }
            Some((first, _)) => {
                let Some(inside) = mark.for_folder() else { continue };
                let slot = out.names.entry(first.into()).or_insert(inside);
                *slot = (*slot).max(inside);
            }
        }
    }
    out.deleted = status.deleted.iter().filter_map(|p| under(&fold_for(p, case, nfc))).collect();
    out.changed += out.deleted.len();
    out
}

/// The status bar's Git part: `⎇ main ↑2 ↓1 · 5 changed · 1 deleted`; a detached HEAD
/// `⎇ (a1b2c3d)`.
pub fn status_text(status: &GitStatus, marks: &FolderMarks) -> String {
    let mut text = match (&status.branch, &status.oid) {
        (Some(branch), _) => format!("⎇ {branch}"),
        (None, Some(oid)) => format!("⎇ ({})", oid.get(..7).unwrap_or(oid)),
        (None, None) => "⎇".to_owned(),
    };
    match status.ahead_behind {
        Some((0, 0)) | None => {}
        Some((ahead, behind)) => {
            for (n, arrow) in [(ahead, '↑'), (behind, '↓')] {
                if n > 0 {
                    text.push_str(&format!(" {arrow}{n}"));
                }
            }
        }
    }
    let changes = marks.changed - marks.deleted.len();
    if changes > 0 {
        // TRANSLATORS: the status bar in a Git repository: changes under the folder shown.
        text.push_str(&format!(" · {}", tn!("{n} changed" | "{n} changed", changes)));
    }
    if !marks.deleted.is_empty() {
        // TRANSLATORS: the status bar in a Git repository: files deleted under the folder shown.
        text.push_str(&format!(" · {}", tn!("{n} deleted" | "{n} deleted", marks.deleted.len())));
    }
    if marks.conflicts > 0 {
        // TRANSLATORS: the status bar in a Git repository: files in conflict under the folder shown.
        text.push_str(&format!(" · {}", tn!("{n} conflict" | "{n} conflicts", marks.conflicts)));
    }
    text
}

/// The status bar's text when no `git` was found (once).
pub fn missing_text() -> String {
    t!("Install Git to see repository status").into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn z(lines: &[&str]) -> Vec<u8> {
        let mut out = Vec::new();
        for line in lines {
            out.extend_from_slice(line.as_bytes());
            out.push(0);
        }
        out
    }

    const H: &str =
        "100644 100644 100644 1111111111111111111111111111111111111111 2222222222222222222222222222222222222222";

    #[test]
    fn reads_every_kind_of_line() {
        let out = z(&[
            "# branch.oid 0123456789abcdef0123456789abcdef01234567",
            "# branch.head main",
            "# branch.upstream origin/main",
            "# branch.ab +2 -1",
            &format!("1 .M N... {H} src/a b.rs"),
            &format!("1 M. N... {H} staged.txt"),
            &format!("1 MM N... {H} both.txt"),
            &format!("1 A. N... {H} new.txt"),
            &format!("1 AM N... {H} new-edited.txt"),
            &format!("1 .D N... {H} gone.txt"),
            &format!("1 D. N... {H} removed.txt"),
            &format!("1 .M SC.. {H} vendor/sub"),
            &format!("2 R. N... {H} R100 docs/ünicode new.md"),
            "docs/old.md",
            "u UU N... 100644 100644 100644 100644 1111111111111111111111111111111111111111 2222222222222222222222222222222222222222 3333333333333333333333333333333333333333 clash.txt",
            "? notes/",
            "? with\nnewline.txt",
            "! target/",
        ]);
        let s = parse_status(&out);
        assert_eq!(s.branch.as_deref(), Some("main"));
        assert_eq!(s.ahead_behind, Some((2, 1)));
        assert!(s.oid.as_deref().unwrap().starts_with("0123456"));
        let mark = |p: &str| s.entries.iter().find(|(path, _)| path == p).map(|(_, m)| *m);
        assert_eq!(mark("src/a b.rs"), Some(GitMark::Modified));
        assert_eq!(mark("staged.txt"), Some(GitMark::Staged));
        assert_eq!(mark("both.txt"), Some(GitMark::StagedModified));
        assert_eq!(mark("new.txt"), Some(GitMark::Added));
        assert_eq!(mark("new-edited.txt"), Some(GitMark::StagedModified));
        assert_eq!(mark("vendor/sub"), Some(GitMark::Modified), "a submodule's new commits");
        assert_eq!(mark("docs/ünicode new.md"), Some(GitMark::Renamed));
        assert_eq!(mark("docs/old.md"), None, "the old name is no row");
        assert_eq!(mark("clash.txt"), Some(GitMark::Conflict));
        assert_eq!(mark("notes"), Some(GitMark::Untracked), "a folder without its slash");
        assert_eq!(mark("with\nnewline.txt"), Some(GitMark::Untracked));
        assert_eq!(mark("target"), Some(GitMark::Ignored));
        assert_eq!(s.deleted, ["gone.txt", "removed.txt"]);
    }

    #[test]
    fn a_detached_head_a_new_repository_and_no_upstream() {
        let s = parse_status(&z(&["# branch.oid abcdef1234567", "# branch.head (detached)"]));
        assert_eq!((s.branch.as_deref(), s.ahead_behind), (None, None));
        assert_eq!(status_text(&s, &FolderMarks::default()), "⎇ (abcdef1)");
        let s = parse_status(&z(&["# branch.oid (initial)", "# branch.head main"]));
        assert_eq!((s.oid, s.branch.as_deref()), (None, Some("main")));
        assert_eq!(parse_status(b""), GitStatus::default());
    }

    #[test]
    fn a_folder_takes_the_most_important_mark_inside() {
        let mut s = GitStatus::default();
        for (path, mark) in [
            ("a/x.txt", GitMark::Untracked),
            ("a/deep/y.txt", GitMark::Added),
            ("b/x.txt", GitMark::Renamed),
            ("b/y.txt", GitMark::StagedModified),
            ("c/x.txt", GitMark::Conflict),
            ("c/y.txt", GitMark::Modified),
            ("d/x.log", GitMark::Ignored),
            ("e", GitMark::Ignored),
            ("f.txt", GitMark::Staged),
        ] {
            s.entries.push((path.into(), mark));
        }
        s.deleted.push("a/gone.txt".into());
        let m = folder_marks(&s, "");
        assert_eq!(m.of("a"), Some(GitMark::Added));
        assert_eq!(m.of("b"), Some(GitMark::Modified), "staged + modified counts as modified");
        assert_eq!(m.of("c"), Some(GitMark::Conflict));
        assert_eq!(m.of("d"), None, "ignored files inside do not mark a folder");
        assert_eq!(m.of("e"), Some(GitMark::Ignored), "a folder ignored whole");
        assert_eq!(m.of("f.txt"), Some(GitMark::Staged));
        assert_eq!(m.changed, 8, "seven not ignored, one deleted");
        let a = folder_marks(&s, "a");
        assert_eq!((a.of("x.txt"), a.of("deep")), (Some(GitMark::Untracked), Some(GitMark::Added)));
        assert_eq!(a.deleted, ["gone.txt"]);
        assert_eq!(a.changed, 3);
        assert_eq!(folder_marks(&s, "ab").changed, 0, "a/ is not under ab/");
    }

    #[test]
    fn names_differing_in_case_or_composition_find_their_mark() {
        let s = GitStatus {
            entries: vec![
                ("Docs/ReadMe.md".into(), GitMark::Modified),
                ("Docs/\u{e9}t\u{e9}.txt".into(), GitMark::Untracked),
            ],
            ..GitStatus::default()
        };
        // Windows: the folder and file as on disk, in another case.
        let windows = folder_marks_for(&s, "docs", true, false);
        assert_eq!(windows.of("README.md"), Some(GitMark::Modified));
        // macOS: a decomposed name from disk, git's composed one.
        let macos = folder_marks_for(&s, "Docs", true, true);
        assert_eq!(macos.of("e\u{301}te\u{301}.txt"), Some(GitMark::Untracked));
        assert_eq!(macos.of("readme.md"), Some(GitMark::Modified));
        // Linux: names are bytes.
        let linux = folder_marks_for(&s, "Docs", false, false);
        assert_eq!((linux.of("readme.md"), linux.of("ReadMe.md")), (None, Some(GitMark::Modified)));
    }

    #[test]
    fn a_folder_inside_an_ignored_or_untracked_one_marks_every_row() {
        let s = GitStatus { entries: vec![("build".into(), GitMark::Ignored)], ..GitStatus::default() };
        assert_eq!(folder_marks(&s, "build").of("anything"), Some(GitMark::Ignored));
        assert_eq!(folder_marks(&s, "build/x64/debug").whole, Some(GitMark::Ignored));
        assert_eq!(folder_marks(&s, "buildings").whole, None);
    }

    #[test]
    fn the_status_bar_says_branch_ahead_behind_and_changes() {
        let mut s = parse_status(&z(&["# branch.head main", "# branch.ab +2 -0"]));
        s.entries.push(("a".into(), GitMark::Modified));
        s.entries.push(("b".into(), GitMark::Ignored));
        s.deleted.push("c".into());
        let m = folder_marks(&s, "");
        assert_eq!(status_text(&s, &m), "⎇ main ↑2 · 1 changed · 1 deleted");
        s.entries.push(("d".into(), GitMark::Conflict));
        let m = folder_marks(&s, "");
        assert_eq!(status_text(&s, &m), "⎇ main ↑2 · 2 changed · 1 deleted · 1 conflict");
        s.ahead_behind = Some((0, 3));
        assert_eq!(status_text(&s, &FolderMarks::default()), "⎇ main ↓3");
    }

    #[test]
    fn conflicts_rank_first_and_ignored_last() {
        let order = [
            Some(GitMark::Conflict),
            Some(GitMark::Modified),
            Some(GitMark::StagedModified),
            Some(GitMark::Staged),
            Some(GitMark::Renamed),
            Some(GitMark::Added),
            Some(GitMark::Untracked),
            None,
            Some(GitMark::Ignored),
        ];
        assert!(order.windows(2).all(|w| rank(w[0]) < rank(w[1])));
    }
}
