//! The filter's `git:` words (spec 16 §8.4), as `tag:` is (`meta_filter`): `git:changed`,
//! `git:modified`, `git:staged`, `git:untracked`, `git:conflict`, `git:ignored`; `-git:…` leaves
//! those rows out. A folder counts with the mark it shows (the most important one inside).

use crate::git::GitMark;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitWord {
    /// Any change: not unchanged, not ignored.
    Changed,
    Modified,
    Staged,
    Untracked,
    Conflict,
    Ignored,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GitTerm {
    pub word: GitWord,
    /// `-git:…`: the rows it names are left out.
    pub not: bool,
}

fn word(text: &str) -> Option<GitWord> {
    [
        ("changed", GitWord::Changed),
        ("modified", GitWord::Modified),
        ("staged", GitWord::Staged),
        ("untracked", GitWord::Untracked),
        ("conflict", GitWord::Conflict),
        ("ignored", GitWord::Ignored),
    ]
    .into_iter()
    .find(|(name, _)| name.eq_ignore_ascii_case(text))
    .map(|(_, w)| w)
}

/// `text` without its `git:` words, and those words. A word starts the text or follows a space
/// or `;`; one not known (half typed) is left out, matching everything.
pub fn split(text: &str) -> (String, Vec<GitTerm>) {
    let (mut rest, mut terms) = (String::with_capacity(text.len()), Vec::new());
    let mut i = 0;
    while let Some(c) = text[i..].chars().next() {
        let here = &text[i..];
        let starts = i == 0 || text[..i].ends_with([' ', ';']);
        let not = here.starts_with('-');
        let body = &here[usize::from(not)..];
        if starts && body.get(..4).is_some_and(|head| head.eq_ignore_ascii_case("git:")) {
            let value = &body[4..];
            let end = value.find([' ', ';']).unwrap_or(value.len());
            terms.extend(word(&value[..end]).map(|word| GitTerm { word, not }));
            i += usize::from(not) + 4 + end;
            continue;
        }
        rest.push(c);
        i += c.len_utf8();
    }
    (rest, terms)
}

/// Whether a row with `mark` (`None`: unchanged) passes every term.
pub fn matches(terms: &[GitTerm], mark: Option<GitMark>) -> bool {
    terms.iter().all(|term| {
        let named = match term.word {
            GitWord::Changed => mark.is_some_and(|m| m != GitMark::Ignored),
            GitWord::Modified => matches!(mark, Some(GitMark::Modified | GitMark::StagedModified)),
            GitWord::Staged => {
                matches!(mark, Some(GitMark::Staged | GitMark::StagedModified | GitMark::Added | GitMark::Renamed))
            }
            GitWord::Untracked => mark == Some(GitMark::Untracked),
            GitWord::Conflict => mark == Some(GitMark::Conflict),
            GitWord::Ignored => mark == Some(GitMark::Ignored),
        };
        named != term.not
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_come_out_of_the_text() {
        let (rest, terms) = split("*.rs git:Modified -git:ignored;budget git:");
        assert_eq!(rest, "*.rs  ;budget ");
        assert_eq!(
            terms,
            [GitTerm { word: GitWord::Modified, not: false }, GitTerm { word: GitWord::Ignored, not: true }]
        );
        assert_eq!(split("legit:changed").1, [], "only at a word's start");
        assert_eq!(split("git:chan").1, [], "half typed: nothing");
        assert_eq!(split("ünï git:staged").0, "ünï ");
    }

    #[test]
    fn each_word_names_its_marks() {
        let t = |text: &str, mark| matches(&split(text).1, mark);
        assert!(t("git:changed", Some(GitMark::Untracked)));
        assert!(!t("git:changed", Some(GitMark::Ignored)) && !t("git:changed", None));
        assert!(t("git:modified", Some(GitMark::StagedModified)) && !t("git:modified", Some(GitMark::Staged)));
        assert!(t("git:staged", Some(GitMark::Added)) && t("git:staged", Some(GitMark::Renamed)));
        assert!(t("git:conflict", Some(GitMark::Conflict)) && t("git:ignored", Some(GitMark::Ignored)));
        assert!(t("git:untracked", Some(GitMark::Untracked)));
        assert!(!t("-git:ignored", Some(GitMark::Ignored)) && t("-git:ignored", None));
        assert!(t("", None), "no words: everything");
    }
}
