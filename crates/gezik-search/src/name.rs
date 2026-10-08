//! The name field (spec 3.2): the filter's pattern language, or a regular expression. Only
//! the name is matched, never the path.

use gezik_core::pattern::Pattern;
use regex::{Regex, RegexBuilder};

#[derive(Debug, Clone)]
pub enum NameMatcher {
    Pattern(Pattern),
    Regex(Regex),
}

impl NameMatcher {
    /// `text` as a pattern (it always ignores case and folds the Turkish i), or as a regular
    /// expression (`(?i)` and the Turkish i class unless `match_case`). The errors read as the
    /// field shows them.
    pub fn compile(text: &str, regex: bool, match_case: bool) -> Result<NameMatcher, String> {
        if !regex || text.is_empty() {
            return Pattern::compile(text).map(NameMatcher::Pattern);
        }
        let source = if match_case { text.to_owned() } else { format!("(?i){}", turkish_i(text)) };
        RegexBuilder::new(&source).build().map(NameMatcher::Regex).map_err(|err| regex_error(&err))
    }

    pub fn matches(&self, name: &str) -> bool {
        match self {
            NameMatcher::Pattern(pattern) => pattern.matches(name),
            NameMatcher::Regex(regex) => regex.is_match(name),
        }
    }

    /// Whether every name passes (an empty field).
    pub fn lets_all_through(&self) -> bool {
        matches!(self, NameMatcher::Pattern(pattern) if pattern.is_empty())
    }
}

/// `regex`'s complaint in one line: `Not a regular expression: unclosed group`.
pub fn regex_error(err: &regex::Error) -> String {
    match err {
        regex::Error::Syntax(text) => {
            let why = text.lines().last().unwrap_or_default().trim().trim_start_matches("error: ");
            format!("Not a regular expression: {why}")
        }
        regex::Error::CompiledTooBig(_) => "The regular expression is too large".to_owned(),
        _ => "Not a regular expression".to_owned(),
    }
}

/// The letters i, I, İ and ı as one class (`[iIİı]`) where they stand for themselves: not in
/// a `[...]` class, not escaped (`\i`, and `\x{..}` / `\p{..}` with their braces), not in a
/// group's flags or name (`(?i)`, `(?P<file>`). Unicode's simple folding does not take İ for
/// i; the pattern language does (spec 3.2).
pub fn turkish_i(source: &str) -> String {
    let mut out = String::with_capacity(source.len() + 16);
    let mut chars = source.chars().peekable();
    let mut class = 0usize;
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                out.push(c);
                let Some(escaped) = chars.next() else { break };
                out.push(escaped);
                // `\x{69}`, `\p{Latin}`, `\u{130}`: the braces belong to the escape.
                if escaped.is_ascii_alphabetic() && chars.peek() == Some(&'{') {
                    for inner in chars.by_ref() {
                        out.push(inner);
                        if inner == '}' {
                            break;
                        }
                    }
                }
            }
            '[' => {
                class += 1;
                out.push(c);
            }
            ']' if class > 0 => {
                class -= 1;
                out.push(c);
            }
            '(' if class == 0 && chars.peek() == Some(&'?') => {
                out.push(c);
                // Flags and a group's name, up to where the group's own pattern starts.
                for inner in chars.by_ref() {
                    out.push(inner);
                    if matches!(inner, ':' | ')' | '>') {
                        break;
                    }
                }
            }
            'i' | 'I' | 'İ' | 'ı' if class == 0 => out.push_str("[iIİı]"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(text: &str, regex: bool, case: bool, name: &str) -> bool {
        NameMatcher::compile(text, regex, case).unwrap().matches(name)
    }

    #[test]
    fn the_pattern_language_is_the_filters() {
        assert!(m("*.pdf;!*draft*", false, false, "Rapor.PDF"));
        assert!(!m("*.pdf;!*draft*", false, false, "rapor draft.pdf"));
        assert!(m("istanbul", false, true, "İSTANBUL.txt"), "a pattern always folds, match case or not");
        assert_eq!(NameMatcher::compile("!", false, false).unwrap_err(), "Type a name after \"!\"");
        assert!(NameMatcher::compile("", false, false).unwrap().lets_all_through());
        assert!(NameMatcher::compile("", true, false).unwrap().lets_all_through());
        assert!(!NameMatcher::compile("x", false, false).unwrap().lets_all_through());
    }

    #[test]
    fn regex_names_fold_the_turkish_i_unless_case_matters() {
        assert!(m("^ist.*\\.txt$", true, false, "İSTANBUL.TXT"));
        assert!(m("ılık", true, false, "ILIK.doc") && m("ILIK", true, false, "ılık.doc"));
        assert!(!m("^ist", true, true, "İSTANBUL.txt"), "match case: as typed");
        assert!(m("^İST", true, true, "İSTANBUL.txt"));
        assert!(m("rapor", true, false, "RAPOR 2024.pdf"));
    }

    #[test]
    fn turkish_i_leaves_classes_groups_and_escapes_alone() {
        assert_eq!(turkish_i("ali"), "al[iIİı]");
        assert_eq!(turkish_i("[a-i]i"), "[a-i][iIİı]", "inside a class: as typed");
        assert_eq!(turkish_i("[[:alpha:]i]x"), "[[:alpha:]i]x", "a nested class");
        assert_eq!(turkish_i("(?i)x"), "(?i)x", "flags");
        assert_eq!(turkish_i("(?P<file>i)"), "(?P<file>[iIİı])", "a group's name");
        assert_eq!(turkish_i("(?<file>i)"), "(?<file>[iIİı])");
        assert_eq!(turkish_i(r"\p{Latin}i"), r"\p{Latin}[iIİı]", "a Unicode class");
        assert_eq!(turkish_i(r"\x{69}\i"), r"\x{69}\i", "escapes");
        for source in ["(?i)x", "(?P<file>i)", r"\wi", "[a-i]+"] {
            assert!(regex::Regex::new(&turkish_i(source)).is_ok(), "{source}");
        }
    }

    #[test]
    fn a_bad_regex_says_why() {
        let error = NameMatcher::compile("(", true, false).unwrap_err();
        assert!(error.starts_with("Not a regular expression: "), "{error}");
        assert!(!error.contains('\n'), "{error}");
    }
}
