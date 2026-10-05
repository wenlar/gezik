//! Rename rules as data: what the rename layer edits, settings save and `gezik-batch`
//! applies, in order, each to what the one before made.

use super::case::CaseMode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleEntry {
    /// Unchecked rules are kept but skipped.
    pub enabled: bool,
    pub rule: Rule,
}

impl RuleEntry {
    pub fn new(rule: Rule) -> RuleEntry {
        RuleEntry { enabled: true, rule }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rule {
    Replace(ReplaceRule),
    Number(NumberRule),
    Case(CaseMode),
    /// Text before and after the name; may use template fields.
    AddText {
        prefix: String,
        suffix: String,
    },
    Extension(ExtensionRule),
    /// The whole name from a template.
    Template(String),
    Clean(CleanRule),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReplaceRule {
    pub find: String,
    pub with: String,
    pub regex: bool,
    pub case_sensitive: bool,
    /// Every match, else only the first.
    pub all: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberAt {
    Start,
    End,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NumberRule {
    pub start: i64,
    pub step: i64,
    /// Zero-padded to at least this many digits (1-9).
    pub digits: u8,
    pub at: NumberAt,
    /// Between the number and the name.
    pub separator: String,
    /// Counting starts again in each folder.
    pub per_folder: bool,
}

impl Default for NumberRule {
    fn default() -> Self {
        NumberRule { start: 1, step: 1, digits: 3, at: NumberAt::End, separator: " ".to_owned(), per_folder: false }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtensionRule {
    /// A new extension, without the dot (`jpg`); empty: none.
    Set(String),
    Lower,
    Upper,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CleanRule {
    /// Spaces at the start and end go.
    pub trim: bool,
    /// Runs of spaces become one.
    pub collapse_spaces: bool,
    /// `_`, `.` and `-` become spaces.
    pub separators_to_spaces: bool,
    /// Spaces become `_`.
    pub spaces_to_underscores: bool,
}

/// The kinds the "Add rule" menu offers, in its order: (name in files, label).
pub const KINDS: [(&str, &str); 7] = [
    ("replace", "Replace text"),
    ("number", "Number"),
    ("case", "Change case"),
    ("add-text", "Add text"),
    ("extension", "Extension"),
    ("template", "Name from template"),
    ("clean", "Clean up"),
];

impl Rule {
    pub fn kind_name(&self) -> &'static str {
        match self {
            Rule::Replace(_) => "replace",
            Rule::Number(_) => "number",
            Rule::Case(_) => "case",
            Rule::AddText { .. } => "add-text",
            Rule::Extension(_) => "extension",
            Rule::Template(_) => "template",
            Rule::Clean(_) => "clean",
        }
    }

    /// A new rule of `kind` (a name in [`KINDS`]) as the "Add rule" menu makes it.
    pub fn default_of(kind: &str) -> Option<Rule> {
        Some(match kind {
            "replace" => Rule::Replace(ReplaceRule { all: true, ..ReplaceRule::default() }),
            "number" => Rule::Number(NumberRule::default()),
            "case" => Rule::Case(CaseMode::Lower),
            "add-text" => Rule::AddText { prefix: String::new(), suffix: String::new() },
            "extension" => Rule::Extension(ExtensionRule::Lower),
            "template" => Rule::Template("{name}".to_owned()),
            "clean" => Rule::Clean(CleanRule { trim: true, collapse_spaces: true, ..CleanRule::default() }),
            _ => return None,
        })
    }

    /// One line for the rule list: `Replace "IMG_" → "Tatil "`.
    pub fn summary(&self) -> String {
        match self {
            Rule::Replace(r) => {
                let how = if r.regex { " (regex)" } else { "" };
                format!("Replace \"{}\" → \"{}\"{how}", r.find, r.with)
            }
            Rule::Number(n) => {
                let sample = format!("{:0width$}", n.start, width = usize::from(n.digits.clamp(1, 9)));
                let at = match n.at {
                    NumberAt::Start => "at start",
                    NumberAt::End => "at end",
                };
                format!("Number {sample}, {at}")
            }
            Rule::Case(mode) => format!("Case: {}", mode.label()),
            Rule::AddText { prefix, suffix } => format!("Add \"{prefix}\" … \"{suffix}\""),
            Rule::Extension(ExtensionRule::Set(ext)) if ext.is_empty() => "Remove extension".to_owned(),
            Rule::Extension(ExtensionRule::Set(ext)) => format!("Extension: .{ext}"),
            Rule::Extension(ExtensionRule::Lower) => "Extension: lower case".to_owned(),
            Rule::Extension(ExtensionRule::Upper) => "Extension: UPPER CASE".to_owned(),
            Rule::Template(text) => format!("Name: {text}"),
            Rule::Clean(_) => "Clean up spaces and separators".to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_a_default_and_a_summary() {
        for (kind, _) in KINDS {
            let rule = Rule::default_of(kind).unwrap();
            assert_eq!(rule.kind_name(), kind);
            assert!(!rule.summary().is_empty());
        }
        assert!(Rule::default_of("nope").is_none());
    }

    #[test]
    fn summaries() {
        let replace = Rule::Replace(ReplaceRule { find: "IMG_".into(), with: "Tatil ".into(), ..Default::default() });
        assert_eq!(replace.summary(), "Replace \"IMG_\" → \"Tatil \"");
        assert_eq!(Rule::Number(NumberRule::default()).summary(), "Number 001, at end");
        assert_eq!(Rule::Extension(ExtensionRule::Set(String::new())).summary(), "Remove extension");
    }
}
