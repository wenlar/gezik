//! Edits `settings.toml` in place: only `pinned`, the `[view]` values or the saved tables change; the user's
//! comments, key order and formatting stay as they were.

use crate::pins::{PinEntry, parse_pin, pin_to_value};

/// Returns `text` with `pinned` set to `pinned`: plain text for a pin without alias and group,
/// an inline table for the others. The entries already there that Gezik cannot read (an unknown
/// key, no path, `..`) stay, after `pinned`. All plain: one line, as before 7b; with a table:
/// one entry per line (spec 6.1). Errors with the parser's message if the file is not valid
/// TOML (the caller must then leave the file alone).
pub fn with_pinned(text: &str, pinned: &[PinEntry]) -> Result<String, String> {
    let mut doc = text.parse::<toml_edit::DocumentMut>().map_err(|err| err.to_string().trim().to_owned())?;
    if let Some(existing) = doc.get("pinned")
        && !existing.is_array()
    {
        return Err("pinned must be a list".to_owned());
    }
    let plain = text.parse::<toml::Table>().map_err(|err| err.to_string().trim().to_owned())?;
    let unreadable: Vec<usize> = plain
        .get("pinned")
        .and_then(|value| value.as_array())
        .map(|items| (0..items.len()).filter(|&i| parse_pin(&items[i]).is_err()).collect())
        .unwrap_or_default();
    let kept: Vec<toml_edit::Value> = doc
        .get("pinned")
        .and_then(|item| item.as_array())
        .map(|array| unreadable.iter().filter_map(|&i| array.get(i).cloned()).collect())
        .unwrap_or_default();
    let mut values: Vec<toml_edit::Value> = pinned.iter().map(pin_to_value).collect();
    values.extend(kept);
    let one_per_line = values.iter().any(|value| !value.is_str());
    if doc.get("pinned").is_none() {
        doc["pinned"] = toml_edit::value(toml_edit::Array::new());
    }
    let Some(array) = doc["pinned"].as_array_mut() else { return Err("pinned must be a list".to_owned()) };
    array.clear();
    for value in values {
        array.push_formatted(value);
    }
    array.fmt();
    if one_per_line {
        for value in array.iter_mut() {
            value.decor_mut().set_prefix("\n  ");
            value.decor_mut().set_suffix("");
        }
        array.set_trailing("\n");
        array.set_trailing_comma(true);
    }
    Ok(doc.to_string())
}

/// Returns `text` with `[view]`'s `mode`, `sort`, `sort-dir` and `grid-size` set from
/// `view` ("Apply to all folders"); other `[view]` keys and the rest of the file stay.
pub fn with_view_defaults(text: &str, view: &gezik_core::view::ViewSettings) -> Result<String, String> {
    let mut doc = text.parse::<toml_edit::DocumentMut>().map_err(|err| err.to_string().trim().to_owned())?;
    if doc.get("view").is_none() {
        doc["view"] = toml_edit::table();
    }
    let Some(table) = doc["view"].as_table_like_mut() else { return Err("view must be a table".to_owned()) };
    let entries = [
        ("mode", view.mode.as_str()),
        ("sort", view.sort.key.as_str()),
        ("sort-dir", view.sort.dir.as_str()),
        ("grid-size", view.grid_size.as_str()),
    ];
    for (key, text) in entries {
        // Keep the old value's decor so inline comments (`# list | grid`) survive.
        if let Some(old) = table.get_mut(key).and_then(|item| item.as_value_mut()) {
            let decor = old.decor().clone();
            *old = toml_edit::Value::from(text);
            *old.decor_mut() = decor;
        } else {
            table.insert(key, toml_edit::value(text));
        }
    }
    Ok(doc.to_string())
}

/// Returns `text` with `[[rename-presets]]` replaced by `presets`; the rest stays. Entries
/// that do not parse as a preset (hand-written, broken) are not Gezik's to drop: they are
/// written back unchanged after `presets`.
pub fn with_rename_presets(text: &str, presets: &[crate::settings::RenamePreset]) -> Result<String, String> {
    let tables = presets.iter().map(crate::settings::preset_to_toml).collect();
    with_tables(text, "rename-presets", tables, |item| crate::settings::parse_preset(item).is_ok())
}

/// Returns `text` with `[[filters]]` replaced by `filters`; the rest stays, and so do the
/// entries that do not read as a filter (see `with_rename_presets`).
pub fn with_filters(text: &str, filters: &[crate::settings::SavedFilter]) -> Result<String, String> {
    let tables = filters.iter().map(crate::settings::filter_to_toml).collect();
    with_tables(text, "filters", tables, |item| crate::settings::parse_filter(item).is_ok())
}

/// Returns `text` with `[[tab-sets]]` replaced by `sets`; the rest stays, and so do the
/// entries that do not read as a tab set (see `with_rename_presets`).
pub fn with_tab_sets(text: &str, sets: &[crate::settings::TabSet]) -> Result<String, String> {
    let tables = sets.iter().map(crate::settings::tab_set_to_toml).collect();
    with_tables(text, "tab-sets", tables, |item| crate::settings::parse_tab_set(item).is_ok())
}

/// Replaces the `[[key]]` tables that `valid` accepts with `tables`; the entries it rejects
/// are written back unchanged after them.
fn with_tables(
    text: &str,
    key: &str,
    tables: Vec<toml::Table>,
    valid: impl Fn(&toml::Value) -> bool,
) -> Result<String, String> {
    let mut doc = text.parse::<toml_edit::DocumentMut>().map_err(|err| err.to_string().trim().to_owned())?;
    let plain = text.parse::<toml::Table>().map_err(|err| err.to_string().trim().to_owned())?;
    // Whether each existing entry is valid, by the same rules as `Settings::parse`.
    let valid: Vec<bool> =
        plain.get(key).and_then(|v| v.as_array()).map(|items| items.iter().map(&valid).collect()).unwrap_or_default();
    let mut broken: Vec<toml_edit::Table> = Vec::new();
    // Comments above the presets that are replaced: the end of the table before them (see below).
    let mut comments = String::new();
    match doc.remove(key) {
        None => {}
        Some(toml_edit::Item::ArrayOfTables(array)) => {
            for (i, table) in array.into_iter().enumerate() {
                if !valid.get(i).copied().unwrap_or(false) {
                    broken.push(table);
                } else if let Some(prefix) = table.decor().prefix().and_then(|p| p.as_str())
                    && prefix.contains('#')
                {
                    comments.push_str(prefix);
                }
            }
        }
        Some(toml_edit::Item::Value(toml_edit::Value::Array(array))) => {
            for (i, value) in array.into_iter().enumerate() {
                if valid.get(i).copied().unwrap_or(false) {
                    continue;
                }
                match value {
                    toml_edit::Value::InlineTable(table) => broken.push(table.into_table()),
                    _ => return Err(format!("{key} must be [[{key}]] tables")),
                }
            }
        }
        Some(_) => return Err(format!("{key} must be [[{key}]] tables")),
    }
    // Comments at the end of the file belong to its last table (the template's commented
    // `[shortcuts]` examples): they stay above the presets, which go after it.
    let trailing = doc.trailing().as_str().unwrap_or("").to_owned();
    if trailing.contains('#') {
        comments.push_str(&trailing);
        doc.set_trailing("");
    }
    let comments = end_with_one_line_break(&comments);
    if !tables.is_empty() || !broken.is_empty() {
        let mut array = toml_edit::ArrayOfTables::new();
        for table in &tables {
            let text = toml::to_string(&table).map_err(|err| err.to_string())?;
            let parsed = text.parse::<toml_edit::DocumentMut>().map_err(|err| err.to_string())?;
            array.push(parsed.as_table().clone());
        }
        for table in broken {
            array.push(table);
        }
        // The tables come from other documents: their positions would scatter them (and
        // their `[[rename-presets.rules]]`) among the file's tables. They go at the end.
        let mut next = last_position(doc.as_table()) + 1;
        for table in array.iter_mut() {
            place(table, &mut next);
        }
        if let Some(first) = array.get_mut(0)
            && !comments.is_empty()
        {
            let own = first.decor().prefix().and_then(|p| p.as_str()).unwrap_or("").to_owned();
            first.decor_mut().set_prefix(format!("{comments}\n{}", own.trim_start_matches(['\r', '\n'])));
        }
        doc.insert(key, toml_edit::Item::ArrayOfTables(array));
    } else if !comments.is_empty() {
        doc.set_trailing(comments);
    }
    Ok(doc.to_string())
}

/// `text` without the blank lines at its end; whitespace only becomes empty.
fn end_with_one_line_break(text: &str) -> String {
    let body = text.trim_end();
    if body.is_empty() {
        return String::new();
    }
    let rest = &text[body.len()..];
    let line_break = if rest.starts_with("\r\n") { "\r\n" } else { "\n" };
    format!("{body}{line_break}")
}

/// The highest position of `table` and the tables in it.
fn last_position(table: &toml_edit::Table) -> isize {
    let mut last = table.position().unwrap_or(0);
    for (_, item) in table.iter() {
        match item {
            toml_edit::Item::Table(t) => last = last.max(last_position(t)),
            toml_edit::Item::ArrayOfTables(a) => {
                for t in a.iter() {
                    last = last.max(last_position(t));
                }
            }
            _ => {}
        }
    }
    last
}

/// Gives `table` and the tables in it the positions from `next` on, in order.
fn place(table: &mut toml_edit::Table, next: &mut isize) {
    table.set_position(Some(*next));
    *next += 1;
    for (_, item) in table.iter_mut() {
        match item {
            toml_edit::Item::Table(t) => place(t, next),
            toml_edit::Item::ArrayOfTables(a) => {
                for t in a.iter_mut() {
                    place(t, next);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pins(list: &[&str]) -> Vec<PinEntry> {
        list.iter().map(|s| PinEntry::plain(*s)).collect()
    }

    #[test]
    fn old_lists_are_written_as_they_were() {
        let text = "# mine\npinned = [\"{documents}/Projects\", \"D:/Work\"]\n";
        let out = with_pinned(text, &pins(&["{documents}/Projects", "D:/Work", "/new"])).unwrap();
        assert_eq!(out, "# mine\npinned = [\"{documents}/Projects\", \"D:/Work\", \"/new\"]\n");
    }

    #[test]
    fn aliases_and_groups_are_inline_tables_one_per_line() {
        use crate::settings::Settings;
        let list = [
            PinEntry::plain("{documents}/Projects"),
            PinEntry { path: "D:/Work/gezik".into(), name: Some("Gezik".into()), group: Some("Work".into()) },
            PinEntry { path: "//nas/foto".into(), name: None, group: Some("Media".into()) },
        ];
        let out = with_pinned("pinned = []\n\n[layout]\nsidebar = \"left\"\n", &list).unwrap();
        // The spec's example (6.1), as written.
        assert!(
            out.starts_with(
                "pinned = [\n  \"{documents}/Projects\",\n  { path = \"D:/Work/gezik\", name = \"Gezik\", group = \"Work\" },\n  \
                 { path = \"//nas/foto\", group = \"Media\" },\n]\n"
            ),
            "{out}"
        );
        let mut warnings = Vec::new();
        assert_eq!(Settings::parse("settings.toml", &out, &mut warnings).pinned, list);
        assert!(warnings.is_empty(), "{warnings:?}");
        let back = with_pinned(&out, &pins(&["/a"])).unwrap();
        assert!(back.starts_with("pinned = [\"/a\"]\n"), "no alias or group left: one line again: {back}");
    }

    #[test]
    fn unreadable_pins_are_kept_after_ours() {
        use crate::settings::Settings;
        let text = "pinned = [\"/a\", { path = \"/b\", icon = \"star\" }, \"{home}/../x\", { name = \"no path\" }]\n";
        let out = with_pinned(text, &pins(&["/c"])).unwrap();
        let parsed = out.parse::<toml::Table>().unwrap();
        let items = parsed["pinned"].as_array().unwrap();
        assert_eq!(items.len(), 4, "{out}");
        assert_eq!(items[0].as_str(), Some("/c"));
        assert_eq!(items[1]["icon"].as_str(), Some("star"));
        assert_eq!(items[2].as_str(), Some("{home}/../x"));
        assert_eq!(items[3]["name"].as_str(), Some("no path"));
        let mut warnings = Vec::new();
        assert_eq!(Settings::parse("settings.toml", &out, &mut warnings).pinned, pins(&["/c"]));
        assert_eq!(warnings.len(), 3, "the kept ones still warn: {warnings:?}");
    }

    #[test]
    fn keeps_comments_and_other_keys() {
        let text = "# my settings\ntheme = \"nord\" # favourite\npinned = [\"/a\"]\n\n[layout]\n# keep me\nsidebar = \"right\"\n";
        let out = with_pinned(text, &pins(&["/a", "/b"])).unwrap();
        assert!(out.contains("# my settings"));
        assert!(out.contains("theme = \"nord\" # favourite"));
        assert!(out.contains("# keep me"));
        assert!(out.contains("sidebar = \"right\""));
        let parsed = out.parse::<toml::Table>().unwrap();
        let pinned: Vec<_> = parsed["pinned"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(pinned, ["/a", "/b"]);
    }

    #[test]
    fn adds_pinned_as_a_top_level_key_when_missing() {
        let text = "theme = \"auto\"\n\n[layout]\ndensity = \"compact\"\n";
        let out = with_pinned(text, &pins(&["{home}/x"])).unwrap();
        let parsed = out.parse::<toml::Table>().unwrap();
        assert_eq!(parsed["pinned"].as_array().unwrap()[0].as_str(), Some("{home}/x"));
        assert_eq!(parsed["layout"]["density"].as_str(), Some("compact"));
    }

    #[test]
    fn empty_list_is_written() {
        let out = with_pinned("pinned = [\"/a\"]\n", &[]).unwrap();
        assert!(out.parse::<toml::Table>().unwrap()["pinned"].as_array().unwrap().is_empty());
    }

    #[test]
    fn invalid_toml_is_an_error() {
        assert!(with_pinned("theme = \n[layout", &pins(&["/a"])).is_err());
    }

    #[test]
    fn keeps_inline_comment_on_pinned_line() {
        let text = "pinned = [\"/a\"] # my note\n";
        let out = with_pinned(text, &pins(&["/b"])).unwrap();
        assert!(out.contains("# my note"), "inline comment lost");
        let parsed = out.parse::<toml::Table>().unwrap();
        let pinned: Vec<_> = parsed["pinned"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(pinned, ["/b"]);
    }

    #[test]
    fn pinned_appears_before_sections() {
        let text = "theme = \"auto\"\n\n[layout]\ndensity = \"compact\"\n";
        let out = with_pinned(text, &pins(&["{home}/x"])).unwrap();
        let pinned_pos = out.find("pinned").expect("pinned not found");
        let layout_pos = out.find("[layout]").expect("[layout] not found");
        assert!(pinned_pos < layout_pos, "pinned should appear before [layout]");
    }

    #[test]
    fn rejects_pinned_table() {
        let text = "[pinned]\nfolder = \"/a\"\n";
        let err = with_pinned(text, &pins(&["/b"])).unwrap_err();
        assert_eq!(err, "pinned must be a list");
    }

    #[test]
    fn rejects_pinned_array_of_tables() {
        let text = "theme = \"nord\"\n\n[[pinned]]\npath = \"/a\"\n";
        assert!(with_pinned(text, &pins(&["/b"])).is_err());
    }

    #[test]
    fn rejects_pinned_string() {
        let text = "pinned = \"/a\"\n";
        let err = with_pinned(text, &pins(&["/b"])).unwrap_err();
        assert_eq!(err, "pinned must be a list");
    }

    #[test]
    fn writes_view_defaults_keeping_the_rest() {
        use gezik_core::view::{GridSize, SortDir, SortKey, SortSpec, ViewMode, ViewSettings};
        let view = ViewSettings {
            mode: ViewMode::Grid,
            sort: SortSpec { key: SortKey::Size, dir: SortDir::Desc },
            grid_size: GridSize::Large,
        };
        let text = "# mine\ntheme = \"nord\"\n\n[view]\n# keep me\nicons = \"gezik\"\nmode = \"list\"\n";
        let out = with_view_defaults(text, &view).unwrap();
        assert!(out.contains("# mine") && out.contains("# keep me"), "{out}");
        let parsed = out.parse::<toml::Table>().unwrap();
        let v = parsed["view"].as_table().unwrap();
        assert_eq!(v["mode"].as_str(), Some("grid"));
        assert_eq!(v["sort"].as_str(), Some("size"));
        assert_eq!(v["sort-dir"].as_str(), Some("desc"));
        assert_eq!(v["grid-size"].as_str(), Some("large"));
        assert_eq!(v["icons"].as_str(), Some("gezik"));

        let added = with_view_defaults("theme = \"auto\"\n", &view).unwrap();
        assert_eq!(added.parse::<toml::Table>().unwrap()["view"]["mode"].as_str(), Some("grid"));
        assert!(with_view_defaults("view = 3\n", &view).is_err());
        assert!(with_view_defaults("theme = \n", &view).is_err());
    }

    #[test]
    fn view_defaults_keep_inline_comments_of_the_template() {
        use gezik_core::view::{GridSize, ViewMode, ViewSettings};
        let template = include_str!("../templates/settings.toml");
        let view = ViewSettings { mode: ViewMode::Grid, grid_size: GridSize::Large, ..ViewSettings::default() };
        let out = with_view_defaults(template, &view).unwrap();
        for hint in
            ["# list | grid", "# name | modified | created | type | size", "# asc | desc", "# small | medium | large"]
        {
            assert!(
                out.contains(hint),
                "{hint} lost:
{out}"
            );
        }
        assert!(out.contains("mode = \"grid\""), "{out}");
    }

    #[test]
    fn rename_presets_are_written_keeping_the_rest() {
        use crate::settings::{RenamePreset, Settings};
        use gezik_core::batch::rules::{Rule, RuleEntry};
        let preset = RenamePreset {
            name: "Tatil".into(),
            include_extension: false,
            rules: vec![RuleEntry::new(Rule::Template("{n:03}".into()))],
        };
        let text = "# mine\ntheme = \"nord\"\n\n[[rename-presets]]\nname = \"old\"\n";
        let out = with_rename_presets(text, std::slice::from_ref(&preset)).unwrap();
        assert!(out.contains("# mine"), "{out}");
        let settings = Settings::parse("settings.toml", &out, &mut Vec::new());
        assert_eq!(settings.rename_presets, [preset]);
        let out = with_rename_presets(&out, &[]).unwrap();
        assert!(!out.contains("rename-presets"), "{out}");
    }

    fn tatil() -> crate::settings::RenamePreset {
        use gezik_core::batch::rules::{Rule, RuleEntry};
        crate::settings::RenamePreset {
            name: "Tatil".into(),
            include_extension: false,
            rules: vec![RuleEntry::new(Rule::Template("{n:03}".into()))],
        }
    }

    #[test]
    fn broken_hand_written_presets_are_kept() {
        use crate::settings::Settings;
        let text = "[[rename-presets]]
name = \"good\"

[[rename-presets]]
name = \"broken\" # mine
rules = 5
";
        let out = with_rename_presets(text, &[tatil()]).unwrap();
        assert!(out.contains("name = \"broken\" # mine"), "{out}");
        assert!(out.contains("rules = 5"), "{out}");
        assert!(!out.contains("\"good\""), "a valid one is replaced: {out}");
        let mut warnings = Vec::new();
        let settings = Settings::parse("settings.toml", &out, &mut warnings);
        assert_eq!(settings.rename_presets, [tatil()]);
        assert_eq!(warnings.len(), 1, "the broken one still warns: {warnings:?}");
        // Deleting every valid one keeps the broken one too.
        let out = with_rename_presets(&out, &[]).unwrap();
        assert!(out.contains("rules = 5"), "{out}");
    }

    #[test]
    fn presets_parse_back_next_to_other_tables() {
        use crate::settings::Settings;
        let text = "theme = \"nord\"

[view]
mode = \"grid\"

[files]
confirm-trash = true

[[rename-presets]]
name = \"old\"
";
        let out = with_rename_presets(text, &[tatil()]).unwrap();
        let mut warnings = Vec::new();
        let settings = Settings::parse("settings.toml", &out, &mut warnings);
        assert_eq!(settings.rename_presets, [tatil()], "{out}");
        assert!(warnings.is_empty(), "{warnings:?}");
        let parsed = out.parse::<toml::Table>().unwrap();
        assert_eq!(parsed["view"]["mode"].as_str(), Some("grid"));
        assert_eq!(parsed["files"]["confirm-trash"].as_bool(), Some(true));
    }

    #[test]
    fn presets_go_after_the_template_comments() {
        use crate::settings::Settings;
        let template = include_str!("../templates/settings.toml");
        let header = |text: &str| text.lines().position(|l| l.trim() == "[[rename-presets]]");
        // The commented shortcut examples: the lines under `[shortcuts]` like `# up = "alt+up"`.
        let start = template.lines().position(|l| l.trim() == "[shortcuts]").unwrap();
        let examples: Vec<&str> = template
            .lines()
            .skip(start)
            .take_while(|l| !l.starts_with("# Saved rule sets"))
            .filter(|l| l.starts_with("# ") && l.contains(" = \""))
            .collect();
        assert!(examples.len() > 10, "{examples:?}");

        let out = with_rename_presets(template, &[tatil()]).unwrap();
        let at = header(&out).unwrap_or_else(|| panic!("no header: {out}"));
        let lines: Vec<&str> = out.lines().collect();
        for example in &examples {
            let line = lines.iter().position(|l| l == example).unwrap_or_else(|| panic!("{example} lost: {out}"));
            assert!(line < at, "{example} is under the presets: {out}");
        }
        let mut warnings = Vec::new();
        let settings = Settings::parse("settings.toml", &out, &mut warnings);
        assert_eq!(settings.rename_presets, [tatil()], "{out}");
        assert!(warnings.is_empty(), "{warnings:?}");

        // Saving again keeps the layout; removing them all gives the template back.
        let again = with_rename_presets(&out, &[tatil()]).unwrap();
        assert_eq!(again, out);
        let back = with_rename_presets(&out, &[]).unwrap();
        // toml_edit writes `\n` line breaks (the checkout may have `\r\n`).
        assert_eq!(back, template.replace("\r\n", "\n"));
    }

    #[test]
    fn filters_are_written_keeping_the_rest() {
        use crate::settings::{SavedFilter, Settings};
        let filter = SavedFilter { name: "Resimler".into(), pattern: "*.jpg;*.png".into() };
        let text = "# mine
theme = \"nord\"

[[filters]]
name = \"old\"
pattern = \"x\"

[[filters]]
name = \"broken\" # mine
";
        let out = with_filters(text, std::slice::from_ref(&filter)).unwrap();
        assert!(out.contains("# mine") && out.contains("name = \"broken\" # mine"), "{out}");
        assert!(!out.contains("\"old\""), "a valid one is replaced: {out}");
        let mut warnings = Vec::new();
        let settings = Settings::parse("settings.toml", &out, &mut warnings);
        assert_eq!(settings.filters, [filter]);
        assert_eq!(warnings.len(), 1, "the broken one still warns: {warnings:?}");
    }

    #[test]
    fn filters_and_presets_together_leave_the_template_as_it_was() {
        let template = include_str!("../templates/settings.toml");
        let filter = crate::settings::SavedFilter { name: "Resimler".into(), pattern: "*.jpg".into() };
        let with_both = with_rename_presets(&with_filters(template, &[filter]).unwrap(), &[tatil()]).unwrap();
        let mut warnings = Vec::new();
        let settings = crate::settings::Settings::parse("settings.toml", &with_both, &mut warnings);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!((settings.filters.len(), settings.rename_presets.len()), (1, 1));
        // Removing both keeps every comment of the template (the examples move, they are not lost).
        let back = with_rename_presets(&with_filters(&with_both, &[]).unwrap(), &[]).unwrap();
        let settings = crate::settings::Settings::parse("settings.toml", &back, &mut warnings);
        assert!(warnings.is_empty() && settings.filters.is_empty() && settings.rename_presets.is_empty(), "{back}");
        for line in template.lines().filter(|l| l.starts_with('#')) {
            assert!(
                back.lines().any(|b| b == line),
                "{line} lost:
{back}"
            );
        }
    }

    #[test]
    fn tab_sets_are_written_keeping_the_rest() {
        use crate::settings::{Settings, TabSet};
        let set = TabSet { name: "Release".into(), tabs: vec!["{downloads}".into(), "drives".into()] };
        let text = "# mine\n\n[[tab-sets]]\nname = \"old\"\ntabs = [\"/a\"]\n\n\
                    [[tab-sets]]\nname = \"broken\" # mine\ntabs = [\"{home}/../x\"]\n";
        let out = with_tab_sets(text, std::slice::from_ref(&set)).unwrap();
        assert!(out.contains("# mine") && out.contains("name = \"broken\" # mine"), "{out}");
        assert!(!out.contains("\"old\""), "a valid one is replaced: {out}");
        let mut warnings = Vec::new();
        let settings = Settings::parse("settings.toml", &out, &mut warnings);
        assert_eq!(settings.tab_sets, [set]);
        assert_eq!(warnings.len(), 1, "the broken one still warns: {warnings:?}");
        let template = include_str!("../templates/settings.toml");
        let one = [TabSet { name: "X".into(), tabs: vec!["/x".into()] }];
        let back = with_tab_sets(&with_tab_sets(template, &one).unwrap(), &[]).unwrap();
        for line in template.lines().filter(|l| l.starts_with('#')) {
            assert!(back.lines().any(|b| b == line), "{line} lost:\n{back}");
        }
    }
}
