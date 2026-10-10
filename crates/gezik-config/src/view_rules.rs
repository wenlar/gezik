//! `[[view-rules]]` in settings.toml (spec 10 §8.1), read into `RuleSpec`s. A rule with a mistake
//! is left out with a warning (`view-rules[2]: …`); the others stay, keeping their numbers.

use gezik_core::group::GroupBy;
use gezik_core::view::{ColumnKey, GridSize, SortDir, SortKey, ViewMode};
use gezik_core::view_rules::{Columns, Content, ContentClass, PlaceKind, RuleSpec, RuleView};

use crate::Warning;
use crate::paths::TOKENS;

/// More would only slow every folder open; nobody writes this many by hand.
const MAX_RULES: usize = 256;

const KEYS: [&str; 9] = ["path", "kind", "content", "mode", "sort", "sort-dir", "group", "grid-size", "columns"];

pub(crate) fn parse_view_rules(value: &toml::Value, file: &str, warnings: &mut Vec<Warning>) -> Vec<RuleSpec> {
    let Some(items) = value.as_array() else {
        warnings.push(Warning::new(file, format!("view-rules: expected [[view-rules]] tables, got {value}")));
        return Vec::new();
    };
    if items.len() > MAX_RULES {
        warnings.push(Warning::new(file, format!("view-rules: only the first {MAX_RULES} rules are used")));
    }
    let mut rules = Vec::new();
    for (i, item) in items.iter().enumerate().take(MAX_RULES) {
        match parse_rule(item, i + 1) {
            Ok(rule) => rules.push(rule),
            Err(err) => warnings.push(Warning::new(file, format!("view-rules[{}]: {err}", i + 1))),
        }
    }
    rules
}

fn parse_rule(value: &toml::Value, number: usize) -> Result<RuleSpec, String> {
    let table = value.as_table().ok_or_else(|| format!("expected a table, got {value}"))?;
    if let Some(key) = table.keys().find(|key| !KEYS.contains(&key.as_str())) {
        return Err(format!("unknown key \"{key}\" (known: {})", KEYS.join(", ")));
    }
    let path = text(table, "path")?.map(path_text).transpose()?;
    let kind = text(table, "kind")?.map(kind_of).transpose()?;
    let content = text(table, "content")?.map(content_of).transpose()?;
    if path.is_none() && kind.is_none() && content.is_none() {
        return Err("needs path, kind or content".to_owned());
    }
    let keys = "\"name\", \"modified\", \"created\", \"type\" or \"size\"";
    let set = RuleView {
        mode: text(table, "mode")?.map(mode_of).transpose()?,
        // `folder` is the search results' own sort, as in [view].
        sort: text(table, "sort")?
            .map(|t| choice(t, "sort", keys, |t| SortKey::parse(t).filter(|k| *k != SortKey::Folder)))
            .transpose()?,
        sort_dir: text(table, "sort-dir")?
            .map(|t| choice(t, "sort-dir", "\"asc\" or \"desc\"", SortDir::parse))
            .transpose()?,
        group: text(table, "group")?
            .map(|t| choice(t, "group", "\"none\", \"type\", \"date\" or \"size\"", GroupBy::parse))
            .transpose()?,
        grid_size: text(table, "grid-size")?
            .map(|t| choice(t, "grid-size", "\"small\", \"medium\" or \"large\"", GridSize::parse))
            .transpose()?,
        columns: table.get("columns").map(columns_of).transpose()?,
    };
    if set.is_empty() {
        return Err("sets nothing (mode, sort, sort-dir, group, grid-size or columns)".to_owned());
    }
    Ok(RuleSpec { number, path, kind, content, set })
}

fn text<'a>(table: &'a toml::Table, key: &str) -> Result<Option<&'a str>, String> {
    match table.get(key) {
        None => Ok(None),
        Some(value) => value.as_str().map(Some).ok_or_else(|| format!("{key}: expected text, got {value}")),
    }
}

fn choice<T>(text: &str, key: &str, options: &str, parse: impl Fn(&str) -> Option<T>) -> Result<T, String> {
    parse(text).ok_or_else(|| format!("{key}: expected {options}, got \"{text}\""))
}

/// `~` is `{home}`; `\` is `/`; a `{token}` must be one `KnownDirs` knows.
fn path_text(text: &str) -> Result<String, String> {
    let text = text.trim().replace('\\', "/");
    if text.is_empty() {
        return Err("path: empty".to_owned());
    }
    let text = match text.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with('/') => format!("{{home}}{rest}"),
        _ => text,
    };
    let rest = match text.strip_prefix('{').and_then(|inner| inner.split_once('}')) {
        Some((token, rest)) if TOKENS.contains(&token) => rest,
        Some((token, _)) => {
            let known: Vec<String> = TOKENS.iter().map(|t| format!("{{{t}}}")).collect();
            return Err(format!("path: unknown folder {{{token}}} (known: {})", known.join(", ")));
        }
        None => text.as_str(),
    };
    // Only a leading token is a folder; later on it would be matched as a name.
    if let Some((token, _)) = rest.split_once('{').and_then(|(_, after)| after.split_once('}')) {
        return Err(format!("path: {{{token}}} only works at the start"));
    }
    Ok(text)
}

fn kind_of(text: &str) -> Result<PlaceKind, String> {
    if text == "archive-root" {
        return Err("kind: \"archive-root\" is not supported yet (Gezik does not open archives as folders)".to_owned());
    }
    choice(text, "kind", "drives, trash, search, flat, network, removable or cloud", PlaceKind::parse)
}

fn content_of(text: &str) -> Result<Content, String> {
    let classes: Vec<&str> = ContentClass::ALL.iter().map(|c| c.as_str()).collect();
    Content::parse(text)
        .ok_or_else(|| format!("content: expected \"<kind> >= <n>%\" (kinds: {}), got \"{text}\"", classes.join(", ")))
}

fn mode_of(text: &str) -> Result<ViewMode, String> {
    choice(text, "mode", "\"list\", \"grid\" or \"columns\"", ViewMode::parse)
}

/// `columns = ["modified", "size"]`: which columns show; `name` always does.
fn columns_of(value: &toml::Value) -> Result<Columns, String> {
    let bad = || format!("columns: expected a list of modified, created, type, size, folder or match, got {value}");
    let items = value.as_array().ok_or_else(bad)?;
    let mut keys = Vec::new();
    for item in items {
        let name = item.as_str().ok_or_else(bad)?;
        if name != "name" {
            keys.push(ColumnKey::parse(name).ok_or_else(bad)?);
        }
    }
    Ok(Columns::of(&keys))
}

#[cfg(test)]
mod tests {
    use crate::Warning;
    use crate::settings::Settings;
    use gezik_core::group::GroupBy;
    use gezik_core::view::{ColumnKey, GridSize, SortDir, SortKey, ViewMode};
    use gezik_core::view_rules::{Columns, Content, PlaceKind, RuleSpec, RuleView};

    fn parse(text: &str) -> (Vec<RuleSpec>, Vec<String>) {
        let mut warnings: Vec<Warning> = Vec::new();
        let settings = Settings::parse("settings.toml", text, &mut warnings);
        (settings.view_rules, warnings.into_iter().map(|w| w.message).collect())
    }

    #[test]
    fn reads_the_rules_of_the_spec() {
        let (rules, warnings) = parse(
            "[[view-rules]]\npath = \"{pictures}/**\"\nmode = \"grid\"\ngrid-size = \"large\"\n\n\
             [[view-rules]]\nkind = \"network\"\nmode = \"list\"\ncolumns = [\"modified\", \"size\"]\n\n\
             [[view-rules]]\ncontent = \"pictures >= 50%\"\nmode = \"grid\"\n\n\
             [[view-rules]]\npath = \"~/Downloads\"\nsort = \"modified\"\nsort-dir = \"desc\"\ngroup = \"date\"\n",
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        let set = RuleView::default();
        assert_eq!(
            rules,
            [
                RuleSpec {
                    number: 1,
                    path: Some("{pictures}/**".to_owned()),
                    kind: None,
                    content: None,
                    set: RuleView { mode: Some(ViewMode::Grid), grid_size: Some(GridSize::Large), ..set },
                },
                RuleSpec {
                    number: 2,
                    path: None,
                    kind: Some(PlaceKind::Network),
                    content: None,
                    set: RuleView {
                        mode: Some(ViewMode::List),
                        columns: Some(Columns::of(&[ColumnKey::Modified, ColumnKey::Size])),
                        ..set
                    },
                },
                RuleSpec {
                    number: 3,
                    path: None,
                    kind: None,
                    content: Content::parse("pictures >= 50%"),
                    set: RuleView { mode: Some(ViewMode::Grid), ..set },
                },
                RuleSpec {
                    number: 4,
                    path: Some("{home}/Downloads".to_owned()),
                    kind: None,
                    content: None,
                    set: RuleView {
                        sort: Some(SortKey::Modified),
                        sort_dir: Some(SortDir::Desc),
                        group: Some(GroupBy::Date),
                        ..set
                    },
                },
            ]
        );
        assert!(parse("").0.is_empty() && Settings::default().view_rules.is_empty(), "no rules: nothing");
        let (named, warnings) = parse("[[view-rules]]\nkind = \"trash\"\ncolumns = [\"name\", \"size\"]\n");
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(named[0].set.columns, Some(Columns::of(&[ColumnKey::Size])), "name is always shown");
        let (named, warnings) = parse("[[view-rules]]\npath = \"~/Projects/**\"\nmode = \"columns\"\n");
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(named[0].set.mode, Some(ViewMode::Columns));
    }

    #[test]
    fn a_path_takes_tilde_and_folder_tokens() {
        for (written, kept) in [
            ("~", "{home}"),
            ("~/Downloads", "{home}/Downloads"),
            (r"~\Downloads\**", "{home}/Downloads/**"),
            (r"D:\Photos\**", "D:/Photos/**"),
            ("{documents}/Work/*", "{documents}/Work/*"),
            ("~x/y", "~x/y"),
        ] {
            let (rules, warnings) = parse(&format!("[[view-rules]]\npath = '{written}'\nmode = \"grid\"\n"));
            assert!(warnings.is_empty(), "{written}: {warnings:?}");
            assert_eq!(rules[0].path.as_deref(), Some(kept), "{written}");
        }
    }

    #[test]
    fn a_rule_with_a_mistake_is_left_out_and_says_why() {
        let cases = [
            ("mode = \"grid\"", "needs path, kind or content"),
            ("kind = \"trash\"", "sets nothing (mode, sort, sort-dir, group, grid-size or columns)"),
            ("kind = \"trash\"\nmodes = \"grid\"", "unknown key \"modes\""),
            (
                "kind = \"usb\"\nmode = \"grid\"",
                "kind: expected drives, trash, search, flat, network, removable or cloud, got \"usb\"",
            ),
            ("kind = \"archive-root\"\nmode = \"grid\"", "kind: \"archive-root\" is not supported yet"),
            ("content = \"pictures > 50%\"\nmode = \"grid\"", "content: expected \"<kind> >= <n>%\""),
            ("path = \"{photos}/**\"\nmode = \"grid\"", "path: unknown folder {photos}"),
            ("path = \"D:/{photos}/**\"\nmode = \"grid\"", "path: {photos} only works at the start"),
            ("path = \"{home}/{pictures}\"\nmode = \"grid\"", "path: {pictures} only works at the start"),
            ("path = \" \"\nmode = \"grid\"", "path: empty"),
            ("path = 3\nmode = \"grid\"", "path: expected text, got 3"),
            ("kind = \"trash\"\nmode = \"tiles\"", "mode: expected \"list\", \"grid\" or \"columns\""),
            ("kind = \"trash\"\nsort = \"folder\"", "sort: expected"),
            ("kind = \"trash\"\ngrid-size = \"huge\"", "grid-size: expected"),
            ("kind = \"trash\"\ncolumns = [\"modified\", \"colour\"]", "columns: expected a list of"),
            ("kind = \"trash\"\ncolumns = \"size\"", "columns: expected a list of"),
        ];
        for (body, expected) in cases {
            let (rules, warnings) = parse(&format!("[[view-rules]]\n{body}\n"));
            assert!(rules.is_empty(), "{body}");
            assert_eq!(warnings.len(), 1, "{body}: {warnings:?}");
            assert!(
                warnings[0].starts_with("view-rules[1]: ") && warnings[0].contains(expected),
                "{body}: {}",
                warnings[0]
            );
        }
        let (rules, warnings) = parse(
            "[[view-rules]]\nkind = \"usb\"\nmode = \"grid\"\n\n[[view-rules]]\nkind = \"trash\"\nmode = \"grid\"\n",
        );
        assert_eq!((rules.len(), rules[0].number), (1, 2), "a rule keeps its place in the file");
        assert!(warnings[0].starts_with("view-rules[1]: "));
        let (rules, warnings) = parse("view-rules = 3\n");
        assert!(rules.is_empty() && warnings[0].starts_with("view-rules: expected [[view-rules]] tables"));
    }

    #[test]
    fn the_template_rule_examples_read_without_warnings_once_uncommented() {
        let template = include_str!("../templates/settings.toml");
        let start = template.find("# [[view-rules]]").expect("the template has rule examples");
        let examples: String = template[start..]
            .lines()
            .take_while(|line| line.starts_with('#'))
            .map(|line| line.strip_prefix("# ").or_else(|| line.strip_prefix('#')).unwrap_or(line))
            .map(|line| format!("{line}\n"))
            .collect();
        let (rules, warnings) = parse(&examples);
        assert!(warnings.is_empty(), "{warnings:?}\n{examples}");
        assert_eq!(rules.len(), 4);
    }

    #[test]
    fn hostile_rules_are_left_out_one_by_one() {
        let cases = [
            ("", "needs path, kind or content"),
            (
                "kind = 1
mode = \"grid\"",
                "kind: expected text, got 1",
            ),
            (
                "content = [\"pictures >= 50%\"]
mode = \"grid\"",
                "content: expected text",
            ),
            (
                "kind = \"trash\"
mode = true",
                "mode: expected text, got true",
            ),
            (
                "kind = \"trash\"
columns = [1]",
                "columns: expected a list of",
            ),
            (
                "kind = \"trash\"
mode = \"grid\"
[view-rules.x]
y = 1",
                "unknown key \"x\"",
            ),
            (
                "path = \"{}\"
mode = \"grid\"",
                "path: unknown folder {}",
            ),
            (
                "content = \"pictures >= 101%\"
mode = \"grid\"",
                "content: expected",
            ),
        ];
        for (body, expected) in cases {
            let (rules, warnings) = parse(&format!(
                "[[view-rules]]
{body}
"
            ));
            assert!(rules.is_empty(), "{body}");
            assert_eq!(warnings.len(), 1, "{body}: {warnings:?}");
            assert!(
                warnings[0].starts_with("view-rules[1]: ") && warnings[0].contains(expected),
                "{body}: {}",
                warnings[0]
            );
        }
        let (rules, warnings) = parse(
            "view-rules = [1, \"x\", { kind = \"trash\", mode = \"grid\" }]
",
        );
        assert_eq!(rules.iter().map(|r| r.number).collect::<Vec<_>>(), [3]);
        assert!(warnings[0].starts_with("view-rules[1]: expected a table, got 1"), "{warnings:?}");
        assert!(warnings[1].starts_with("view-rules[2]: expected a table"), "{warnings:?}");
        let (rules, warnings) = parse(
            "[[view-rules]]
kind = \"trash\"
columns = []
",
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(rules[0].set.columns, Some(Columns::of(&[])), "an empty list shows the name only");
        // A long list of repeated names is still one small set.
        let many = vec!["\"size\""; 10_000].join(", ");
        let (rules, warnings) = parse(&format!(
            "[[view-rules]]
kind = \"trash\"
columns = [{many}]
"
        ));
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(rules[0].set.columns, Some(Columns::of(&[ColumnKey::Size])));
        // Many rules keep their numbers, up to the cap.
        let rule = "[[view-rules]]
kind = \"trash\"
mode = \"grid\"
";
        let (rules, warnings) = parse(&rule.repeat(256));
        assert!(warnings.is_empty() && rules.len() == 256 && rules[255].number == 256);
        let (rules, warnings) = parse(&rule.repeat(1_000));
        assert_eq!(rules.len(), 256);
        assert_eq!(warnings, ["view-rules: only the first 256 rules are used"]);
    }
}
