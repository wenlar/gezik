//! What the settings window shows of `settings.toml` (spec 14 §3.3): every single setting, its
//! page and its kind of control. The descriptions and defaults come from the template's
//! comments and values (already in the exe), the values from the file's text; the tests bind
//! the three (the template, this list and `Settings::parse`), so a setting added to the template
//! fails them until it is placed here.

mod table;

use std::borrow::Cow;

use gezik_i18n::{t, tn};

use crate::Warning;
use crate::settings::Settings;
use table::template_lines;
pub use table::{FIELDS, Field, Kind, Page};

/// What the template says of a field.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Described {
    /// The comment lines above its line and the comment after its value.
    pub description: String,
    /// The default as the window shows it ("" for a list).
    pub default: String,
    /// The default as TOML, written by Reset to Default; `None` for a key the template only
    /// names in a comment (its default is the key's absence: Reset takes it out).
    pub literal: Option<String>,
}

/// A value as the window shows it: text bare, a list or table by its count.
fn shown(value: &toml::Value) -> String {
    let count = |n: usize| match n {
        0 => t!("none").into_owned(),
        n => tn!("{n} entry" | "{n} entries", n),
    };
    match value {
        toml::Value::String(text) => text.clone(),
        toml::Value::Array(items) => count(items.len()),
        toml::Value::Table(table) => count(table.len()),
        other => other.to_string(),
    }
}

/// The default of a key the template only names in a comment, where it differs by system.
fn platform_default(field: &Field) -> Option<&'static str> {
    (field.path() == "view.show-hidden").then_some(if cfg!(target_os = "macos") { "false" } else { "true" })
}

/// What the template says of each of `FIELDS`, in order. Read when the window opens.
pub fn describe(template: &str) -> Vec<Described> {
    let lines = template_lines(template);
    FIELDS
        .iter()
        .map(|field| {
            let Some(line) = table::line_of(&lines, field) else { return Described::default() };
            let description = table::description(line);
            let value = line
                .body
                .parse::<toml_edit::DocumentMut>()
                .ok()
                .and_then(|doc| doc.get(field.key).and_then(|item| item.as_value()).cloned());
            let mut literal = None;
            let mut default = String::new();
            if let Some(mut value) = value {
                value.decor_mut().clear();
                let text = value.to_string();
                default = text.parse::<toml::Value>().map(|v| shown(&v)).unwrap_or_default();
                literal = (!line.commented).then_some(text);
            }
            if let Some(platform) = platform_default(field) {
                default = platform.to_owned();
            }
            if field.kind == Kind::List {
                default.clear();
            }
            Described { description, default, literal }
        })
        .collect()
}

/// The values `text` (settings.toml) gives `FIELDS`, in order (`None`: not in the file, the
/// default); a syntax error with its line.
pub fn values(text: &str) -> Result<Vec<Option<String>>, Warning> {
    let table = text.parse::<toml::Table>().map_err(|err| Warning::from_toml_error("settings.toml", text, &err))?;
    Ok(FIELDS
        .iter()
        .map(|field| {
            let holder = match field.table {
                None => Some(&table),
                Some(name) => table.get(name).and_then(toml::Value::as_table),
            };
            holder?.get(field.key).map(shown)
        })
        .collect())
}

/// How many keys of `text` Gezik does not know (they stay in the file; spec 14 §4.2).
pub fn unknown_keys(text: &str) -> usize {
    let Ok(table) = text.parse::<toml::Table>() else { return 0 };
    let known = |table: Option<&str>, key: &str| FIELDS.iter().any(|f| f.table == table && f.key == key);
    table
        .iter()
        .map(|(name, value)| match value {
            // A table of settings: its keys one by one.
            toml::Value::Table(inner) if FIELDS.iter().any(|f| f.table == Some(name)) => {
                inner.keys().filter(|key| !known(Some(name), key)).count()
            }
            _ => usize::from(!known(None, name)),
        })
        .sum()
}

/// What to write for `input` (a typed or chosen value) as TOML; `None` when it cannot be one
/// of `kind` (a number field with letters).
pub fn literal(kind: Kind, input: &str) -> Option<String> {
    let input = input.trim();
    match kind {
        Kind::Bool => matches!(input, "true" | "false").then(|| input.to_owned()),
        Kind::Int => input.parse::<i64>().ok().map(|n| n.to_string()),
        Kind::Choice(options) if !options.contains(&input) => None,
        Kind::Choice(_) if matches!(input, "true" | "false") || input.bytes().all(|b| b.is_ascii_digit()) => {
            Some(input.to_owned())
        }
        Kind::Choice(_) | Kind::Text | Kind::Path | Kind::Theme | Kind::Language => {
            Some(toml::Value::String(input.to_owned()).to_string())
        }
        Kind::Integration | Kind::List => None,
    }
}

/// Whether Gezik reads `literal` for `field` without a warning (the parser decides, so the
/// window never writes what the next reload would refuse); the warning's text if not.
pub fn check(field: &Field, literal: &str) -> Result<(), String> {
    let text = match field.table {
        Some(table) => format!("[{table}]\n{} = {literal}\n", field.key),
        None => format!("{} = {literal}\n", field.key),
    };
    let mut warnings = Vec::new();
    Settings::parse("settings.toml", &text, &mut warnings);
    match warnings.into_iter().next() {
        None => Ok(()),
        Some(warning) => Err(warning.message),
    }
}

/// Whether the search box's `query` finds `field`: in its title, its key or its description
/// (case ignored; every word somewhere).
pub fn matches(field: &Field, described: &Described, query: &str) -> bool {
    let (title, context) = (field.title(), field.context());
    // The English texts, and the translated ones when they differ (spec 15).
    let shown = [t(&context, &title), t(&context, &described.description)];
    let shown = shown.iter().filter_map(|s| matches!(s, Cow::Owned(_)).then_some(s.as_ref()));
    let haystack: String =
        [title.as_str(), &field.path(), &described.description].into_iter().chain(shown).collect::<Vec<_>>().join(" ");
    let haystack = haystack.to_lowercase();
    query.to_lowercase().split_whitespace().all(|word| haystack.contains(word))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::SETTINGS_TEMPLATE;

    fn fields() -> impl Iterator<Item = (usize, &'static Field)> {
        FIELDS.iter().enumerate()
    }

    #[test]
    fn every_template_setting_is_in_the_window() {
        for line in template_lines(SETTINGS_TEMPLATE) {
            let mine = FIELDS.iter().any(|f| f.table == line.table && f.key == line.key);
            // A key in a table that is itself a list setting (`[shortcuts]`'s actions).
            let in_list = FIELDS.iter().any(|f| f.kind == Kind::List && f.table.is_none() && Some(f.key) == line.table);
            assert!(mine || in_list, "{:?}.{} is not in FIELDS", line.table, line.key);
        }
    }

    #[test]
    fn every_field_is_in_the_template_with_a_description() {
        let described = describe(SETTINGS_TEMPLATE);
        let lines = template_lines(SETTINGS_TEMPLATE);
        for (i, field) in fields() {
            let path = field.path();
            // The `[[…]]` lists are commented examples in the template: their header is there.
            let listed = SETTINGS_TEMPLATE.contains(&format!("# [[{}]]", field.key))
                || SETTINGS_TEMPLATE.contains(&format!(
                    "
[{}]
",
                    field.key
                ));
            let line = lines.iter().any(|l| l.table == field.table && l.key == field.key);
            assert!(line || listed, "{path} is not in the template");
            if line {
                assert!(!described[i].description.is_empty(), "{path} has no comment in the template");
            }
            if matches!(field.kind, Kind::Bool | Kind::Choice(_) | Kind::Int | Kind::Theme | Kind::Language) {
                assert!(!described[i].default.is_empty(), "{path} has no default");
            }
        }
        let paths: Vec<String> = fields().map(|(_, f)| f.path()).collect();
        for (i, path) in paths.iter().enumerate() {
            assert!(!paths[..i].contains(path), "{path} twice");
        }
    }

    #[test]
    fn every_choice_and_default_reads_back_without_a_warning() {
        let described = describe(SETTINGS_TEMPLATE);
        for (i, field) in fields() {
            if let Kind::Choice(options) = field.kind {
                for option in options {
                    let text = literal(field.kind, option).unwrap();
                    assert_eq!(check(field, &text), Ok(()), "{} = {text}", field.path());
                }
                assert!(options.contains(&described[i].default.as_str()), "{}: {:?}", field.path(), described[i]);
            }
            if let Some(default) = &described[i].literal {
                assert_eq!(check(field, default), Ok(()), "{} = {default}", field.path());
            }
            if !matches!(field.kind, Kind::List | Kind::Integration) {
                let text = literal(field.kind, &described[i].default).unwrap();
                assert_eq!(check(field, &text), Ok(()), "{}'s default as the window writes it", field.path());
            }
        }
    }

    #[test]
    fn wrong_values_are_refused_by_the_parser() {
        let field = |path: &str| fields().map(|(_, f)| f).find(|f| f.path() == path).unwrap();
        assert!(check(field("max-fps"), "5").unwrap_err().contains("max-fps"), "out of range");
        assert!(check(field("search.max-results"), "10").is_err());
        assert!(check(field("files.max-speed"), "\"fast\"").is_err());
        assert_eq!(check(field("files.max-speed"), "\"20 MB/s\""), Ok(()));
        assert!(check(field("start-folder"), "\"{home}/../x\"").is_err());
        assert_eq!(literal(Kind::Int, "12a"), None);
        assert_eq!(literal(Kind::Int, " 60 "), Some("60".to_owned()));
        assert_eq!(literal(Kind::Bool, "yes"), None);
        assert_eq!(literal(field("files.copy-threads").kind, "4"), Some("4".to_owned()), "a number");
        assert_eq!(literal(field("tags.dots").kind, "false"), Some("false".to_owned()), "a boolean");
        assert_eq!(literal(field("layout.density").kind, "tight"), None);
        assert_eq!(literal(Kind::Path, "C:\\Tools \"x\""), Some("'C:\\Tools \"x\"'".to_owned()));
        assert_eq!(literal(Kind::List, "x"), None);
    }

    #[test]
    fn descriptions_and_defaults_come_from_the_template() {
        let described = describe(SETTINGS_TEMPLATE);
        let get = |path: &str| {
            let i = FIELDS.iter().position(|f| f.path() == path).unwrap();
            described[i].clone()
        };
        let density = get("layout.density");
        assert_eq!((density.description.as_str(), density.default.as_str()), ("compact | comfortable", "comfortable"));
        assert!(!described.iter().any(|d| d.description.contains('\u{22ef}')), "the UI font has no ⋯");
        assert_eq!(density.literal.as_deref(), Some("\"comfortable\""));
        let sort = get("view.sort");
        assert!(sort.description == "name | modified | created | type | size | tags | rating", "{sort:?}");
        let hide = get("view.hide-extensions").description;
        assert_eq!(
            hide,
            "Show names without their extension. The same in every folder; the View menu changes these here too."
        );
        let hidden = get("view.show-hidden");
        assert_eq!(hidden.literal, None, "commented: Reset takes it out");
        assert_eq!(hidden.default, if cfg!(target_os = "macos") { "false" } else { "true" });
        assert!(hidden.description.starts_with("Dot names"), "{hidden:?}");
        assert_eq!(get("max-fps").default, "120");
        assert_eq!(get("pinned").default, "", "a list shows its count only");
        assert!(get("pinned").description.starts_with("Folders pinned to the sidebar"), "{:?}", get("pinned"));
    }

    #[test]
    fn values_are_what_the_file_says() {
        let text = "theme = \"nord\"\npinned = [\"/a\", \"/b\"]\n[layout]\ndensity = \"compact\"\n[shortcuts]\nnew-tab = \"ctrl+t\"\n";
        let values = values(text).unwrap();
        let get = |path: &str| values[FIELDS.iter().position(|f| f.path() == path).unwrap()].clone();
        assert_eq!(get("theme").as_deref(), Some("nord"));
        assert_eq!(get("layout.density").as_deref(), Some("compact"));
        assert_eq!(get("layout.sidebar"), None, "not in the file: the default");
        assert_eq!(get("pinned").as_deref(), Some("2 entries"));
        assert_eq!(get("shortcuts").as_deref(), Some("1 entry"));
        assert_eq!(get("max-fps"), None);
        let broken = super::values("theme = \"a\"\n[layout\n").unwrap_err();
        assert_eq!(broken.line, Some(2), "{broken}");
        assert!(broken.to_string().starts_with("settings.toml line 2: "), "{broken}");
    }

    #[test]
    fn unknown_keys_are_counted() {
        assert_eq!(unknown_keys(SETTINGS_TEMPLATE), 0);
        let text = "colour = 1\n[layout]\ndensity = \"compact\"\nwidth = 3\n[mine]\na = 1\nb = 2\n";
        assert_eq!(unknown_keys(text), 3, "colour, layout.width and the table mine");
        assert_eq!(unknown_keys("[broken\n"), 0);
    }

    #[test]
    fn the_search_finds_titles_keys_and_descriptions() {
        let described = describe(SETTINGS_TEMPLATE);
        let found = |query: &str| -> Vec<String> {
            fields().filter(|(i, f)| matches(f, &described[*i], query)).map(|(_, f)| f.path()).collect()
        };
        assert_eq!(found("density"), ["layout.density"]);
        assert!(found("REDUCE motion").contains(&"layout.reduce-motion".to_owned()));
        assert!(found("7-zip").contains(&"tools.seven-zip".to_owned()), "by its description");
        assert!(found("").len() > 60, "an empty search shows everything");
        assert!(found("no such setting").is_empty());
    }

    /// Spec 15 §13.2: the window's titles, descriptions, choices and pages, and every action's
    /// title, are in the translations' template (`gezik-lang extract` puts them there); the
    /// pseudo language frames an action's title.
    #[test]
    fn every_shown_text_is_in_the_pot() {
        let pot = gezik_i18n::po::parse(include_str!("../../gezik-i18n/lang/gezik.pot")).unwrap();
        let keys: std::collections::HashSet<String> = pot.entries.iter().map(|e| e.key()).collect();
        let has = |context: &str, id: &str| keys.contains(&gezik_i18n::po::key(context, id));
        let described = describe(SETTINGS_TEMPLATE);
        for (i, field) in fields() {
            assert!(has(&field.context(), &field.title()), "{}'s title", field.path());
            let description = &described[i].description;
            assert!(description.is_empty() || has(&field.context(), description), "{}'s description", field.path());
            if let Kind::Choice(options) = field.kind {
                for option in options.iter().filter(|o| o.parse::<u32>().is_err()) {
                    assert!(has(&field.choice_context(), option), "{}'s {option}", field.path());
                }
            }
        }
        assert!(Page::ALL.iter().all(|p| has("", p.title())));
        let pseudo = gezik_i18n::Catalog::pseudo();
        for action in crate::shortcuts::Action::ALL {
            assert!(has("", action.title()), "{}", action.name());
            assert!(pseudo.get("", action.title()).starts_with('«'), "{}", action.name());
        }
    }

    /// The comment after a value is read without toml_edit (gezik-lang reads it too): the same
    /// as toml_edit's for every line of the template.
    #[test]
    fn the_comment_after_a_value_is_toml_edits() {
        for line in template_lines(SETTINGS_TEMPLATE) {
            // A value over several lines (`list = [`) has no comment after it on its line.
            let Ok(doc) = line.body.parse::<toml_edit::DocumentMut>() else { continue };
            let value = doc.get(line.key).and_then(|item| item.as_value()).unwrap();
            let hint = value.decor().suffix().and_then(|s| s.as_str()).unwrap_or_default();
            let hint = hint.trim().trim_start_matches('#').trim();
            let description = table::description(&line);
            assert_eq!(description.get(1..hint.len()), hint.get(1..), "{:?}.{}", line.table, line.key);
            if hint.is_empty() {
                assert_eq!(description, line.above.join(" "));
            }
        }
    }

    #[test]
    fn titles_and_paths() {
        let field = Field { table: Some("layout"), key: "reduce-motion", page: Page::Appearance, kind: Kind::Bool };
        assert_eq!((field.title(), field.path()), ("Reduce motion".to_owned(), "layout.reduce-motion".to_owned()));
        assert_eq!(Page::ALL.map(Page::title)[2], "Files and Tools");
    }
}
