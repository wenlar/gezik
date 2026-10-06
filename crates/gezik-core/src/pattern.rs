//! The pattern language of the filter, the selection box and the tab picker (spec 3.2).
//!
//! A pattern is parts split by `;`; each part is trimmed and empty ones are skipped. A part
//! starting with `!` leaves out what it matches. A part without `*` or `?` matches anywhere in
//! the name; with them it must match the whole name (`*.jpg`: only names ending in `.jpg`).
//! `[` and `]` are plain characters. A name passes if it matches an including part (or there
//! is none) and no leaving-out part. Case is ignored, character by character (simple folding:
//! no `ß` = `ss`), and so is the Turkish i/İ/ı/I and the Greek final ς/σ. A pattern is compiled
//! once; matching a name allocates nothing.

use crate::Entry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Token {
    /// One character, folded (see [`fold`]).
    Char(char),
    /// `?`: any one character.
    Any,
    /// `*`: any run of characters, none too.
    Star,
}

/// A compiled pattern. The default (like an empty text) lets every name through.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Pattern {
    include: Vec<Vec<Token>>,
    exclude: Vec<Vec<Token>>,
}

impl Pattern {
    /// Compiles `text`. The errors are shown under the field as they are: a `!` with nothing
    /// after it, and a `/` (no name has one).
    pub fn compile(text: &str) -> Result<Pattern, String> {
        let mut pattern = Pattern::default();
        for part in text.split(';') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let (leave_out, body) = match part.strip_prefix('!') {
                Some(rest) => (true, rest.trim_start()),
                None => (false, part),
            };
            if body.is_empty() {
                return Err("Type a name after \"!\"".to_owned());
            }
            if body.contains('/') {
                return Err("Names can't contain \"/\"".to_owned());
            }
            let tokens = tokens(body);
            if leave_out {
                pattern.exclude.push(tokens);
            } else {
                pattern.include.push(tokens);
            }
        }
        Ok(pattern)
    }

    /// Whether the pattern has no parts (it lets every name through).
    pub fn is_empty(&self) -> bool {
        self.include.is_empty() && self.exclude.is_empty()
    }

    /// Whether the pattern has an including part (else it lets through all that no
    /// leaving-out part matches).
    pub fn has_includes(&self) -> bool {
        !self.include.is_empty()
    }

    /// Whether `name` passes. Allocates nothing.
    pub fn matches(&self, name: &str) -> bool {
        self.matches_any(&[name])
    }

    /// Whether a thing known by several `names` (a tab: its title and its path) passes: an
    /// including part matches one of them (or there is none), and no leaving-out part matches
    /// any of them. Allocates nothing.
    pub fn matches_any(&self, names: &[&str]) -> bool {
        let any = |parts: &[Vec<Token>]| parts.iter().any(|part| names.iter().any(|name| glob(part, name)));
        (!self.has_includes() || any(&self.include)) && !any(&self.exclude)
    }
}

/// The entries whose names `pattern` lets through, in their order.
pub fn matching_entries(entries: &[Entry], pattern: &Pattern) -> Vec<Entry> {
    entries.iter().filter(|entry| pattern.matches(&entry.name)).cloned().collect()
}

/// Where the entries whose names `pattern` lets through are in `entries`, ascending.
pub fn matching_rows(entries: &[Entry], pattern: &Pattern) -> Vec<usize> {
    entries.iter().enumerate().filter(|(_, entry)| pattern.matches(&entry.name)).map(|(i, _)| i).collect()
}

/// One part as tokens; without wildcards it is `*part*` (anywhere in the name).
fn tokens(body: &str) -> Vec<Token> {
    let wild = body.contains(['*', '?']);
    let mut out = Vec::with_capacity(body.len() + 2);
    if !wild {
        out.push(Token::Star);
    }
    out.extend(body.chars().map(|c| match c {
        '*' => Token::Star,
        '?' => Token::Any,
        c => Token::Char(fold(c)),
    }));
    if !wild {
        out.push(Token::Star);
    }
    // `**` is `*`.
    out.dedup_by(|a, b| *a == Token::Star && *b == Token::Star);
    out
}

/// A character as compared: lower case, with i, İ, ı and I all one letter, and the Greek
/// final ς one with σ.
fn fold(c: char) -> char {
    match c {
        'I' | 'İ' | 'ı' => 'i',
        'ς' => 'σ',
        c if c.is_ascii() => c.to_ascii_lowercase(),
        c => c.to_lowercase().next().unwrap_or(c),
    }
}

/// `text` as the pattern compares it: lower case, with i, İ, ı and I one letter (for the
/// address bar, which matches by start and by "holds", not by pattern).
pub fn fold_text(text: &str) -> String {
    text.chars().map(fold).collect()
}

/// Whether the whole of `name` matches `tokens`: one pass with a single back-track point for
/// the last `*` (the classic wildcard matcher), on byte offsets into `name`.
fn glob(tokens: &[Token], name: &str) -> bool {
    let (mut t, mut n) = (0, 0);
    // After the last `*`: the token after it and where in the name it was tried from.
    let mut back: Option<(usize, usize)> = None;
    loop {
        let c = name[n..].chars().next();
        match (tokens.get(t), c) {
            (Some(Token::Star), _) => {
                t += 1;
                back = Some((t, n));
                continue;
            }
            (Some(Token::Any), Some(c)) => {
                t += 1;
                n += c.len_utf8();
                continue;
            }
            (Some(Token::Char(want)), Some(c)) if *want == fold(c) => {
                t += 1;
                n += c.len_utf8();
                continue;
            }
            (None, None) => return true,
            _ => {}
        }
        // A mismatch: let the last `*` take one more character, or give up.
        let Some((after_star, from)) = back else { return false };
        let Some(skipped) = name[from..].chars().next() else { return false };
        let from = from + skipped.len_utf8();
        back = Some((after_star, from));
        t = after_star;
        n = from;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn m(pattern: &str, name: &str) -> bool {
        Pattern::compile(pattern).unwrap().matches(name)
    }

    fn entry(name: &str) -> Entry {
        Entry { name: name.to_owned(), is_dir: false, size: 0, modified: None, created: None }
    }

    #[test]
    fn a_part_without_wildcards_matches_anywhere() {
        assert!(m("jpg", "a.jpg"));
        assert!(m("jpg", "jpg notes.txt"));
        assert!(m("tat", "Tatil 2024"));
        assert!(!m("tatil", "tati"));
    }

    #[test]
    fn wildcards_match_the_whole_name() {
        assert!(m("*.jpg", "a.jpg"));
        assert!(!m("*.jpg", "a.jpg.txt"), "with * the whole name must match");
        assert!(m("a?c", "abc") && !m("a?c", "ac") && !m("a?c", "abcd"));
        assert!(m("a*b*c", "aXbYbZc"));
        assert!(m("*a", "aaa") && !m("a*", ""));
        assert!(m("**.txt", "x.txt"), "two stars are one");
        assert!(m("*", "anything") && m("*", ""));
    }

    #[test]
    fn brackets_are_plain_characters() {
        assert!(m("[1]", "foto [1].jpg"));
        assert!(!m("[1]", "foto 1.jpg"));
        assert!(m("*[a-z]*", "x[a-z]y"));
    }

    #[test]
    fn parts_are_split_by_semicolons_and_trimmed() {
        let p = Pattern::compile("*.jpg; *.png ;;").unwrap();
        assert!(p.matches("a.jpg") && p.matches("b.PNG") && !p.matches("c.gif"));
        assert!(m(" tatil ", "Tatil.doc"));
    }

    #[test]
    fn a_bang_leaves_out() {
        assert!(m("!*.tmp", "a.txt") && !m("!*.tmp", "a.tmp"), "only exclusions: everything else");
        let p = Pattern::compile("*.jpg;!*thumb*").unwrap();
        assert!(p.matches("a.jpg") && !p.matches("a thumb.jpg") && !p.matches("a.png"));
        assert!(m("! tmp", "a.txt") && !m("! tmp", "tmp.txt"), "spaces after ! are trimmed");
        assert!(m("!!a", "b") && !m("!!a", "x!a"), "a second ! is a plain character");
    }

    #[test]
    fn several_names_pass_together() {
        let p = Pattern::compile("doc;!Users").unwrap();
        assert!(p.has_includes() && !Pattern::compile("!x").unwrap().has_includes());
        assert!(p.matches_any(&["Documents", r"D:\Docs"]));
        assert!(p.matches_any(&["Other", r"D:\Docs"]), "an include on either name");
        assert!(!p.matches_any(&["Documents", r"C:\Users\a\Documents"]), "an exclusion on either name");
        let only_out = Pattern::compile("!downloads").unwrap();
        assert!(only_out.matches_any(&["Documents", r"C:\Users\a\Documents"]));
        assert!(!only_out.matches_any(&["İndirilenler", r"C:\Users\a\Downloads"]));
        assert!(Pattern::default().matches_any(&["a", "b"]) && Pattern::default().matches_any(&[]));
    }

    #[test]
    fn case_and_turkish_i_are_ignored() {
        assert!(m("istanbul", "İSTANBUL.txt"));
        assert!(m("ISTANBUL", "istanbul.txt"));
        assert!(m("ılık", "ILIK.doc") && m("ILIK", "ılık.doc"));
        assert!(m("ŞEHİR", "şehir.png") && m("şehir", "ŞEHİR.png"));
        assert!(m("*.JPG", "a.jpg") && m("*.jpg", "B.JPG"));
        assert!(m("ğüöç", "ĞÜÖÇ"));
        assert!(m("ΟΔΟΣ", "οδος.txt") && m("οδοσ", "ΟΔΟΣ") && m("*σ.txt", "οδος.txt"), "final sigma");
    }

    #[test]
    fn a_question_mark_takes_one_whole_character() {
        assert!(m("a?c", "aşc") && m("a?c", "a€c") && m("a?c", "a😀c"), "two, three and four bytes");
        assert!(!m("a?c", "aşşc") && !m("a??c", "aşc"));
        assert!(m("??", "şğ") && !m("???", "şğ"));
        assert!(m("*?", "ğ") && m("?*", "ğ") && !m("?", ""));
    }

    #[test]
    fn an_adversarial_glob_stays_linear_enough() {
        // Many stars against a long run that almost matches: the classic matcher backs up to
        // the last star only, so this is quick, not exponential.
        let name = "a".repeat(20_000);
        let started = Instant::now();
        assert!(!m("*a*a*a*a*a*a*a*a*a*a*b", &name));
        assert!(m("*a*a*a*a*a*a*a*a*a*a", &name));
        assert!(!m(&format!("{}b", "*a".repeat(50)), &name));
        assert!(started.elapsed() < Duration::from_secs(2), "{:?}", started.elapsed());
    }

    #[test]
    fn bad_patterns_say_why() {
        assert_eq!(Pattern::compile("!").unwrap_err(), "Type a name after \"!\"");
        assert_eq!(Pattern::compile("*.jpg; ! ").unwrap_err(), "Type a name after \"!\"");
        assert_eq!(Pattern::compile("a/b").unwrap_err(), "Names can't contain \"/\"");
    }

    #[test]
    fn an_empty_pattern_lets_everything_through() {
        for text in ["", "  ", ";", " ; ; "] {
            let p = Pattern::compile(text).unwrap();
            assert!(p.is_empty() && p.matches("a.txt") && p.matches(""), "{text:?}");
        }
        assert!(Pattern::default().matches("x"));
        assert!(!Pattern::compile("!x").unwrap().is_empty());
    }

    #[test]
    fn matching_entries_keeps_the_order() {
        let entries: Vec<Entry> = ["b.jpg", "a.txt", "c.JPG"].map(entry).into();
        let kept = matching_entries(&entries, &Pattern::compile("*.jpg").unwrap());
        assert_eq!(kept.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["b.jpg", "c.JPG"]);
        assert_eq!(matching_rows(&entries, &Pattern::compile("*.jpg").unwrap()), [0, 2]);
    }

    /// The pure part's share of the 30 ms budget (plan sapma 19): compile, match and copy what
    /// passes, for 100,000 names. Run: `cargo test --release -p gezik-core pattern -- --ignored`.
    #[test]
    #[ignore = "timing; run in release with --ignored"]
    fn a_hundred_thousand_names_filter_within_budget() {
        let entries: Vec<Entry> = (0..100_000).map(|i| entry(&format!("IMG_{i:06} Tatil ş{}.jpg", i % 7))).collect();
        for text in ["i", "img_0", "img_01234", "*.jpg;*.png", "tatil;!*ş3.jpg", "zzz"] {
            let started = Instant::now();
            let pattern = Pattern::compile(text).unwrap();
            let kept = matching_entries(&entries, &pattern);
            let took = started.elapsed();
            assert!(took < Duration::from_millis(15), "{text}: {took:?} ({} kept)", kept.len());
        }
    }

    #[test]
    fn text_folds_as_names_do() {
        assert_eq!(fold_text("İndirilenler"), fold_text("indirilenler"));
        assert_eq!(fold_text("ILIK Şehir"), "ilik şehir");
    }
}
