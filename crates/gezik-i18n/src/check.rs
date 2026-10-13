//! Reading a `.po` file into a layer, with what is wrong with it (spec 15 §11.2): Gezik shows
//! the problems of a dropped file in its warnings, `gezik-lang check` and the tests those of
//! the built-in ones. A translation with a problem is left out (its English shows).

use std::collections::HashMap;

use crate::blob;
use crate::catalog::Layer;
use crate::format::placeholders;
use crate::plural::{self, Plural};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub line: usize,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// Syntax, placeholders, plural forms: the translation is not used.
    pub errors: Vec<Issue>,
    /// Translations over twice as long as the English.
    pub warnings: Vec<Issue>,
    /// Texts of the template translated, and of those not reviewed (`fuzzy`).
    pub translated: usize,
    pub fuzzy: usize,
    /// Texts of the template with no translation.
    pub missing: usize,
    /// Entries the template does not have.
    pub unused: usize,
    /// The template's texts (0: no template known).
    pub total: usize,
    /// `X-Gezik-Review` is not `machine`.
    pub human: bool,
}

/// Reads `text`, the `.po` file of language `code`, against the template's sorted key hashes
/// `keys` (empty: every entry counts).
pub fn load(text: &str, code: &str, keys: &[u64]) -> (Option<Layer>, Report) {
    let mut report = Report { total: keys.len(), ..Report::default() };
    let file = match crate::po::parse(text) {
        Ok(file) => file,
        Err(err) => {
            report.errors.push(Issue { line: err.line, message: err.message });
            return (None, report);
        }
    };
    report.human = file.field("X-Gezik-Review") != Some("machine");
    let language = file.field("Language").unwrap_or(code);
    let table = || plural::builtin(language).or(plural::builtin(code)).and_then(|p| Plural::parse(p).ok());
    let plural = match file.field("Plural-Forms").map(Plural::parse) {
        Some(Ok(plural)) => plural,
        Some(Err(why)) => {
            report.errors.push(Issue { line: 1, message: format!("Plural-Forms: {why}") });
            table().unwrap_or_default()
        }
        None => table().unwrap_or_default(),
    };
    let mut map = HashMap::new();
    for entry in &file.entries {
        let key = entry.key();
        if !keys.is_empty() && keys.binary_search(&blob::hash(&key)).is_err() {
            report.unused += 1;
            continue;
        }
        if !entry.translated() {
            continue;
        }
        let errors = report.errors.len();
        let issue = |message: String| Issue { line: entry.line, message };
        let source = entry.plural.as_deref().unwrap_or(&entry.id);
        let mut wanted = placeholders(source);
        if entry.plural.is_some() {
            wanted.remove("{n}");
            if entry.strs.len() != plural.count {
                let message = format!("{} plural forms, Plural-Forms says {}", entry.strs.len(), plural.count);
                report.errors.push(issue(message));
            }
        }
        for form in &entry.strs {
            let mut have = placeholders(form);
            if entry.plural.is_some() {
                have.remove("{n}");
            }
            for missing in wanted.difference(&have) {
                report.errors.push(issue(format!("\"{missing}\" missing in the translation")));
            }
            for extra in have.difference(&wanted) {
                report.errors.push(issue(format!("\"{extra}\" is not in the English text")));
            }
            if form.chars().count() > 2 * source.chars().count().max(4) {
                report.warnings.push(issue("over twice as long as the English".to_owned()));
            }
        }
        if report.errors.len() == errors {
            report.translated += 1;
            report.fuzzy += usize::from(entry.fuzzy);
            map.insert(key, entry.strs.clone());
        }
    }
    report.missing = keys.len().saturating_sub(report.translated);
    let name = file.field("X-Gezik-Name").or(crate::lang::endonym(code)).unwrap_or(code).to_owned();
    let mut layer = Layer::from_map(name, report.human, plural, map);
    layer.fuzzy = report.fuzzy;
    (Some(layer), report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Catalog;
    use std::sync::Arc;

    const PL: &str = r#"msgid ""
msgstr ""
"Language: pl\n"
"Plural-Forms: nplurals=3; plural=(n==1 ? 0 : n%10>=2 && n%10<=4 && (n%100<10 || n%100>=20) ? 1 : 2);\n"
"X-Gezik-Name: Polski\n"

msgid "Open"
msgstr "Otwórz"

#, fuzzy
msgid "Moved {} to {}"
msgstr "{1} ← {0}"

msgid "Copy {0}"
msgstr "Kopiuj"

msgid "{n} file"
msgid_plural "{n} files"
msgstr[0] "jeden plik"
msgstr[1] "{n} pliki"
msgstr[2] "{n} plików"

msgid "{n} item in {}"
msgid_plural "{n} items in {}"
msgstr[0] "{n} element"
msgstr[1] "{n} elementy w {}"
msgstr[2] "{n} elementów w {}"

msgid "Two forms"
msgid_plural "Two forms"
msgstr[0] "a"
msgstr[1] "b"

msgid "Not yet"
msgstr ""

msgid "Short"
msgstr "Bardzo bardzo długi"

msgid "Gone"
msgstr "Zniknął"
"#;

    fn keys(texts: &[&str]) -> Vec<u64> {
        let mut keys: Vec<u64> = texts.iter().map(|t| blob::hash(t)).collect();
        keys.sort_unstable();
        keys
    }

    #[test]
    fn a_dropped_file_with_its_problems() {
        let pot = keys(&[
            "Open",
            "Moved {} to {}",
            "Copy {0}",
            "{n} file",
            "{n} item in {}",
            "Two forms",
            "Not yet",
            "Short",
            "New",
        ]);
        let (layer, report) = load(PL, "pl", &pot);
        let layer = layer.unwrap();
        assert_eq!(layer.name, "Polski");
        let lines = |issues: &[Issue]| issues.iter().map(|i| (i.line, i.message.clone())).collect::<Vec<_>>();
        assert_eq!(
            lines(&report.errors),
            [
                (14, "\"{0}\" missing in the translation".to_owned()),
                (23, "\"{0}\" missing in the translation".to_owned()),
                (29, "2 plural forms, Plural-Forms says 3".to_owned()),
            ]
        );
        assert_eq!(lines(&report.warnings), [(37, "over twice as long as the English".to_owned())]);
        assert_eq!((report.translated, report.fuzzy, report.missing, report.unused, report.total), (4, 1, 5, 1, 9));
        assert!(report.human);
        let catalog = Catalog::layered("pl", vec![Arc::new(layer)]).unwrap();
        assert_eq!(catalog.get("", "Moved {} to {}"), "{1} ← {0}", "fuzzy entries are used");
        assert_eq!(catalog.get("", "Copy {0}"), "Copy {0}", "a broken one is not");
        assert_eq!(catalog.nget("", "{n} file", "{n} files", 1), "jeden plik", "{{n}} may be left out");
        assert_eq!(catalog.nget("", "{n} file", "{n} files", 3), "{n} pliki");
    }

    #[test]
    fn syntax_errors_and_headers() {
        let (layer, report) = load("msgid \"a\"\nmsgstr b\n", "pl", &[]);
        assert!(layer.is_none());
        assert_eq!(report.errors, [Issue { line: 2, message: "expected a string in double quotes".into() }]);
        let bad_rule = "msgid \"\"\nmsgstr \"Plural-Forms: nplurals=2; plural=n ? 1;\\nX-Gezik-Review: machine\\n\"\n";
        let (layer, report) = load(bad_rule, "fr", &[]);
        assert_eq!(report.errors.len(), 1, "{report:?}");
        assert!(!report.human);
        assert_eq!(layer.unwrap().name, "Français", "the built-in's name when the file has none");
        let (_, report) = load("", "xx", &keys(&["a", "b"]));
        assert_eq!((report.missing, report.total), (2, 2));
    }
}
