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

/// Returns `text` with `[view]`'s `mode`, `sort`, `sort-dir`, `grid-size` and `group` set from
/// `view` ("Apply to all folders"); other `[view]` keys and the rest of the file stay.
pub fn with_view_defaults(text: &str, view: &gezik_core::view::ViewSettings) -> Result<String, String> {
    edit_table(
        text,
        "view",
        vec![
            ("mode", view.mode.as_str().into()),
            ("sort", view.sort.key.as_str().into()),
            ("sort-dir", view.sort.dir.as_str().into()),
            ("grid-size", view.grid_size.as_str().into()),
            ("group", view.group.as_str().into()),
        ],
    )
}

/// Returns `text` with `[view]`'s `option` set (the View menu, toggle-hidden; spec 7.2).
pub fn with_view_option(text: &str, option: crate::settings::ViewOption) -> Result<String, String> {
    use crate::settings::ViewOption;
    let value: toml_edit::Value = match option {
        ViewOption::HideExtensions(on)
        | ViewOption::FoldersFirst(on)
        | ViewOption::SingleClickOpen(on)
        | ViewOption::ShowHidden(on)
        | ViewOption::ShowSystem(on) => on.into(),
        ViewOption::DateFormat(format) => format.as_str().into(),
        ViewOption::SizeFormat(format) => format.as_str().into(),
    };
    edit_table(text, "view", vec![(option.key(), value)])
}

/// Returns `text` with `[system]`'s `key` set (System Integration's tray and shortcut rows);
/// the old value's comment and the rest of the file stay.
pub fn with_system(text: &str, key: &str, value: toml_edit::Value) -> Result<String, String> {
    edit_table(text, "system", vec![(key, value)])
}

/// Returns `text` with `[panes] confirm` set (the F5/F6 question's Don't Ask Again).
pub fn with_panes_confirm(text: &str, on: bool) -> Result<String, String> {
    edit_table(text, "panes", vec![("confirm", on.into())])
}

/// Returns `text` with one value set (the settings window, spec 14 §3.2): `key` in `[table]`,
/// or at the top with no table. `literal` is the value as TOML (`"compact"`, `true`, `120`);
/// `None` takes the key out (a key the template only mentions in a comment, back to its
/// default). The old value's line and comment stay; nothing else changes.
pub fn with_value(text: &str, table: Option<&str>, key: &str, literal: Option<&str>) -> Result<String, String> {
    let value = match literal {
        Some(literal) => {
            let mut value = literal.parse::<toml_edit::Value>().map_err(|err| err.to_string().trim().to_owned())?;
            value.decor_mut().clear();
            Some(value)
        }
        None => None,
    };
    match (table, value) {
        (Some(table), Some(value)) => edit_table(text, table, vec![(key, value)]),
        (table, value) => {
            let mut doc = text.parse::<toml_edit::DocumentMut>().map_err(|err| err.to_string().trim().to_owned())?;
            // The comment lines above a key taken out, when no key after it in its table takes them.
            let mut orphan = None;
            let target = match table {
                None => doc.as_table_mut() as &mut dyn toml_edit::TableLike,
                Some(name) => match doc.get_mut(name).and_then(|item| item.as_table_like_mut()) {
                    Some(target) => target,
                    None => return Ok(doc.to_string()),
                },
            };
            match value {
                None => {
                    // toml_edit keeps the comment lines above a key in the key's own decor:
                    // they stay, above the next value of the table (or the next table).
                    let prefix = target.key(key).and_then(|k| k.leaf_decor().prefix()).and_then(|p| p.as_str());
                    let prefix = prefix.filter(|p| p.contains('#')).map(str::to_owned);
                    let values: Vec<String> =
                        target.iter().filter(|(_, item)| item.is_value()).map(|(k, _)| k.to_owned()).collect();
                    let next = values.iter().skip_while(|k| k.as_str() != key).nth(1).cloned();
                    if target.remove(key).is_some()
                        && let Some(prefix) = prefix
                    {
                        match next.and_then(|next| target.key_mut(&next)) {
                            Some(mut next) => {
                                let own = next.leaf_decor().prefix().and_then(|p| p.as_str()).unwrap_or_default();
                                let joined = format!("{prefix}{own}");
                                next.leaf_decor_mut().set_prefix(joined);
                            }
                            None => orphan = Some(prefix),
                        }
                    }
                }
                Some(value) => match target.get_mut(key).and_then(|item| item.as_value_mut()) {
                    Some(old) => {
                        let decor = old.decor().clone();
                        *old = value;
                        *old.decor_mut() = decor;
                    }
                    None => {
                        target.insert(key, toml_edit::Item::Value(value));
                    }
                },
            }
            if let Some(orphan) = orphan {
                keep_before_next_table(&mut doc, table, &orphan);
            }
            Ok(doc.to_string())
        }
    }
}

/// Puts `comments` (lines from the end of table `after`, `None` the top) before the table that
/// follows it in the file, or at the file's end.
fn keep_before_next_table(doc: &mut toml_edit::DocumentMut, after: Option<&str>, comments: &str) {
    let from = after.and_then(|name| doc.get(name)).and_then(|item| item.as_table()).and_then(|t| t.position());
    let from = from.unwrap_or(-1);
    let mut next: Option<(isize, String)> = None;
    for (name, item) in doc.iter() {
        let position = match item {
            toml_edit::Item::Table(table) => table.position(),
            toml_edit::Item::ArrayOfTables(array) => array.get(0).and_then(toml_edit::Table::position),
            _ => None,
        };
        if let Some(position) = position.filter(|p| *p > from)
            && next.as_ref().is_none_or(|(best, _)| position < *best)
        {
            next = Some((position, name.to_owned()));
        }
    }
    let table = next.and_then(|(_, name)| match doc.get_mut(&name) {
        Some(toml_edit::Item::Table(table)) => Some(table),
        Some(toml_edit::Item::ArrayOfTables(array)) => array.get_mut(0),
        _ => None,
    });
    match table {
        Some(table) => {
            let own = table.decor().prefix().and_then(|p| p.as_str()).unwrap_or_default().to_owned();
            table.decor_mut().set_prefix(format!("{comments}{own}"));
        }
        None => {
            let trailing = doc.trailing().as_str().unwrap_or_default().to_owned();
            doc.set_trailing(format!("{comments}{trailing}"));
        }
    }
}

/// Adds an empty `[name]` at the end. Comments after the file's last value are the document's
/// trailing part, which toml_edit writes after every table: they go before the new table, so
/// they stay where they were.
fn add_table(doc: &mut toml_edit::DocumentMut, name: &str) {
    let mut trailing = doc.trailing().as_str().unwrap_or_default().to_owned();
    doc.set_trailing("");
    // A blank line between it and the last value ([[commands]]'s too); a comment there keeps
    // its place right above.
    if trailing.is_empty() && !doc.as_table().is_empty() {
        trailing.push('\n');
    }
    let mut table = toml_edit::Table::new();
    table.decor_mut().set_prefix(trailing);
    doc[name] = toml_edit::Item::Table(table);
}

/// Returns `text` with `[tags] key` set (the list, the dots).
pub fn with_tags(text: &str, key: &str, value: toml_edit::Value) -> Result<String, String> {
    edit_table(text, "tags", vec![(key, value)])
}

/// Returns `text` with `tag` at the end of `[tags] list`: only that entry is added, so the
/// list's comments and the entries Gezik left out stay. With no `list` there, `list` (the one in
/// effect, the defaults and the new tag) is written.
pub fn with_tag_added(
    text: &str,
    tag: &gezik_core::meta::Tag,
    list: &[gezik_core::meta::Tag],
) -> Result<String, String> {
    let mut doc = text.parse::<toml_edit::DocumentMut>().map_err(|err| err.to_string().trim().to_owned())?;
    if doc.get("tags").is_none() {
        add_table(&mut doc, "tags");
    }
    let Some(table) = doc["tags"].as_table_like_mut() else { return Err("tags must be a table".to_owned()) };
    match table.get_mut("list").and_then(|item| item.as_value_mut()) {
        Some(toml_edit::Value::Array(array)) => {
            let toml_edit::Value::Array(one) = crate::tags::list_value(std::slice::from_ref(tag)) else {
                unreachable!("a list is an array")
            };
            // A list one tag a line: the old last line's comment (the array's trailing part)
            // stays on that line, the new tag goes on its own. An inline list stays inline.
            let trailing = array.trailing().as_str().unwrap_or_default().to_owned();
            let one_a_line = trailing.contains('\n');
            array.set_trailing_comma(true);
            array.extend(one);
            if let Some(last) = array.iter_mut().last() {
                last.decor_mut().set_prefix(if one_a_line { format!("{trailing}  ") } else { " ".to_owned() });
            }
            if one_a_line {
                array.set_trailing("\n");
            }
        }
        Some(_) => return Err("tags.list must be a list".to_owned()),
        None => {
            table.insert("list", toml_edit::Item::Value(crate::tags::list_value(list)));
        }
    }
    Ok(doc.to_string())
}

/// Returns `text` with `edit` made to `[tags] list` (spec 12 §7.3): the entry is found by its
/// name and changed where it is, so the list's comments, its layout and the entries Gezik left
/// out stay. With no `list` there, the list in effect (`list`) with the edit is written.
pub fn with_tag_list_edit(
    text: &str,
    edit: &crate::tags::TagListEdit,
    list: &[gezik_core::meta::Tag],
) -> Result<String, String> {
    use crate::tags::{TagListEdit, color_name, entry_name};
    use gezik_core::meta::same_name;
    let mut doc = text.parse::<toml_edit::DocumentMut>().map_err(|err| err.to_string().trim().to_owned())?;
    if doc.get("tags").is_none() {
        add_table(&mut doc, "tags");
    }
    let Some(table) = doc["tags"].as_table_like_mut() else { return Err("tags must be a table".to_owned()) };
    let array = match table.get_mut("list").and_then(|item| item.as_value_mut()) {
        Some(toml_edit::Value::Array(array)) => array,
        Some(_) => return Err("tags.list must be a list".to_owned()),
        None => {
            let edited = crate::tags::edited(list, edit);
            table.insert("list", toml_edit::Item::Value(crate::tags::list_value(&edited)));
            return Ok(doc.to_string());
        }
    };
    let find = |array: &toml_edit::Array, name: &str| {
        array.iter().position(|value| entry_name(value).is_some_and(|n| same_name(n, name)))
    };
    let Some(at) = find(array, edit.name()) else {
        return Err(format!("tags.list has no tag \"{}\"", edit.name()));
    };
    // A new value in an old one's place keeps its decor (the line and the comment before it).
    let put = |array: &mut toml_edit::Array, at: usize, mut value: toml_edit::Value| {
        if let Some(old) = array.get_mut(at) {
            *value.decor_mut() = old.decor().clone();
            *old = value;
        }
    };
    let entry = array.get(at).cloned().unwrap_or_else(|| toml_edit::Value::from(""));
    match edit {
        TagListEdit::Rename(_, to) => {
            let value = match entry {
                toml_edit::Value::InlineTable(mut inline) => {
                    if let Some(name) = inline.get_mut("name") {
                        let decor = name.decor().clone();
                        *name = (**to).into();
                        *name.decor_mut() = decor;
                    }
                    toml_edit::Value::InlineTable(inline)
                }
                _ => (**to).into(),
            };
            put(array, at, value);
        }
        TagListEdit::Color(_, color) => {
            let mut inline = match entry {
                toml_edit::Value::InlineTable(inline) => inline,
                other => {
                    let mut inline = toml_edit::InlineTable::new();
                    inline.insert("name", entry_name(&other).unwrap_or_default().into());
                    inline
                }
            };
            match inline.get_mut("color") {
                Some(old) => {
                    let decor = old.decor().clone();
                    *old = color_name(*color).into();
                    *old.decor_mut() = decor;
                }
                None => {
                    inline.insert("color", color_name(*color).into());
                }
            }
            put(array, at, toml_edit::Value::InlineTable(inline));
        }
        TagListEdit::Move(name, up) => {
            // Its neighbour in the list in effect (the file may hold entries Gezik left out).
            let k = list.iter().position(|t| same_name(&t.name, name));
            let neighbour = k.and_then(|k| if *up { k.checked_sub(1) } else { Some(k + 1) }).and_then(|k| list.get(k));
            let Some(other) = neighbour.and_then(|t| find(array, &t.name)) else { return Ok(doc.to_string()) };
            let (mine, theirs) = (entry, array.get(other).cloned().unwrap_or_else(|| toml_edit::Value::from("")));
            // Each slot keeps the end of the line before it (a comment on the previous entry's
            // line); the comment lines above an entry go with it.
            let parts = |value: &toml_edit::Value| -> (String, String) {
                let prefix = value.decor().prefix().and_then(|p| p.as_str()).unwrap_or_default().to_owned();
                match prefix.find('\n') {
                    Some(end) => (prefix[..=end].to_owned(), prefix[end + 1..].to_owned()),
                    None => (prefix, String::new()),
                }
            };
            let ((mine_head, mine_tail), (their_head, their_tail)) = (parts(&mine), parts(&theirs));
            let moved = |mut value: toml_edit::Value, head: &str, tail: &str| {
                value.decor_mut().set_prefix(format!("{head}{tail}"));
                value
            };
            let (to_at, to_other) =
                (moved(theirs.clone(), &mine_head, &their_tail), moved(mine.clone(), &their_head, &mine_tail));
            if let Some(slot) = array.get_mut(at) {
                *slot = to_at;
            }
            if let Some(slot) = array.get_mut(other) {
                *slot = to_other;
            }
        }
        TagListEdit::Remove(_) => {
            array.remove(at);
        }
    }
    Ok(doc.to_string())
}

/// Sets `entries` in `[name]` (adding the table and the missing keys), keeping each old value's
/// decor so inline comments (`# list | grid`) survive.
fn edit_table(text: &str, name: &str, entries: Vec<(&str, toml_edit::Value)>) -> Result<String, String> {
    let mut doc = text.parse::<toml_edit::DocumentMut>().map_err(|err| err.to_string().trim().to_owned())?;
    if doc.get(name).is_none() {
        add_table(&mut doc, name);
    }
    let Some(table) = doc[name].as_table_like_mut() else { return Err(format!("{name} must be a table")) };
    for (key, value) in entries {
        set_keeping_decor(table, key, value);
    }
    Ok(doc.to_string())
}

/// Sets `key` in `table`; an old value's decor (its inline comment) stays.
fn set_keeping_decor(table: &mut dyn toml_edit::TableLike, key: &str, value: toml_edit::Value) {
    if let Some(old) = table.get_mut(key).and_then(|item| item.as_value_mut()) {
        let decor = old.decor().clone();
        *old = value;
        *old.decor_mut() = decor;
    } else {
        table.insert(key, toml_edit::Item::Value(value));
    }
}

/// Returns `text` with the `shortcut` of the `[[commands]]` entry named `name` set (the settings
/// window, spec 14 §5); `""` is no key. Nothing else changes.
pub fn with_command_shortcut(text: &str, name: &str, shortcut: &str) -> Result<String, String> {
    let mut doc = text.parse::<toml_edit::DocumentMut>().map_err(|err| err.to_string().trim().to_owned())?;
    let command = doc
        .get_mut("commands")
        .and_then(|item| item.as_array_of_tables_mut())
        .and_then(|commands| commands.iter_mut().find(|c| c.get("name").and_then(|n| n.as_str()) == Some(name)));
    let Some(command) = command else { return Err(format!("settings.toml has no [[commands]] named \"{name}\"")) };
    set_keeping_decor(command, "shortcut", shortcut.into());
    Ok(doc.to_string())
}

/// Returns `text` with `[[rename-presets]]` replaced by `presets`; the rest stays. Entries
/// that do not parse as a preset (hand-written, broken) are not Gezik's to drop: they are
/// written back unchanged after `presets`.
pub fn with_rename_presets(text: &str, presets: &[crate::settings::RenamePreset]) -> Result<String, String> {
    let tables = presets.iter().map(crate::settings::preset_to_toml).collect();
    with_tables(text, "rename-presets", tables, |item| {
        crate::settings::parse_preset(item).ok().map(|p| crate::settings::preset_to_toml(&p))
    })
}

/// Returns `text` with `[[filters]]` replaced by `filters`; the rest stays, and so do the
/// entries that do not read as a filter (see `with_rename_presets`).
pub fn with_filters(text: &str, filters: &[crate::settings::SavedFilter]) -> Result<String, String> {
    let tables = filters.iter().map(crate::settings::filter_to_toml).collect();
    let mut name = read_once(usize::MAX);
    with_tables(text, "filters", tables, move |item| {
        let filter = crate::settings::parse_filter(item).ok()?;
        name(filter.name.clone()).then(|| crate::settings::filter_to_toml(&filter))
    })
}

/// Returns `text` with `[[tab-sets]]` replaced by `sets`; the rest stays, and so do the
/// entries that do not read as a tab set (see `with_rename_presets`).
pub fn with_tab_sets(text: &str, sets: &[crate::settings::TabSet]) -> Result<String, String> {
    let tables = sets.iter().map(crate::settings::tab_set_to_toml).collect();
    let mut name = read_once(usize::MAX);
    with_tables(text, "tab-sets", tables, move |item| {
        let set = crate::settings::parse_tab_set(item).ok()?;
        name(set.name.clone()).then(|| crate::settings::tab_set_to_toml(&set))
    })
}

/// Returns `text` with `[[searches]]` replaced by `searches`; the entries that do not read as a
/// saved search stay as written (see `with_rename_presets`).
pub fn with_searches(text: &str, searches: &[crate::settings::SavedSearch]) -> Result<String, String> {
    let tables = searches.iter().map(crate::settings::saved_search_to_toml).collect();
    let mut name = read_once(crate::settings::SEARCHES_MAX);
    with_tables(text, "searches", tables, move |item| {
        let search = crate::settings::parse_saved_search(item).ok()?;
        name(search.name.clone()).then(|| crate::settings::saved_search_to_toml(&search))
    })
}

/// Returns `text` with `[[sync-profiles]]` replaced by `profiles` (see `with_rename_presets`).
pub fn with_sync_profiles(text: &str, profiles: &[crate::sync_profiles::SyncProfile]) -> Result<String, String> {
    use crate::sync_profiles::{SYNC_PROFILES_MAX, parse, to_toml};
    let tables = profiles.iter().map(to_toml).collect();
    let mut name = read_once(SYNC_PROFILES_MAX);
    with_tables(text, "sync-profiles", tables, move |item| {
        let profile = parse(item).ok()?;
        name(profile.name.clone()).then(|| to_toml(&profile))
    })
}

/// Whether a readable entry named `name` is one `Settings::parse` keeps: among the first `max`,
/// its name not already used (ignoring case). The others stay as written, like broken ones.
fn read_once(max: usize) -> impl FnMut(String) -> bool {
    let mut seen: Vec<String> = Vec::new();
    move |name| {
        let name = name.to_lowercase();
        let kept = seen.len() < max && !seen.contains(&name);
        if kept {
            seen.push(name);
        }
        kept
    }
}

/// Replaces the `[[key]]` tables that `valid` accepts (asked once per entry, in order; it gives
/// the entry as Gezik writes it) with `tables`; the entries it rejects are written back
/// unchanged after them. A table equal to an entry already there is that entry as written: its
/// comments and layout stay.
fn with_tables(
    text: &str,
    key: &str,
    tables: Vec<toml::Table>,
    mut valid: impl FnMut(&toml::Value) -> Option<toml::Table>,
) -> Result<String, String> {
    let mut doc = text.parse::<toml_edit::DocumentMut>().map_err(|err| err.to_string().trim().to_owned())?;
    let plain = text.parse::<toml::Table>().map_err(|err| err.to_string().trim().to_owned())?;
    // Whether each existing entry is valid, by the same rules as `Settings::parse`.
    let valid: Vec<Option<toml::Table>> = plain
        .get(key)
        .and_then(|v| v.as_array())
        .map(|items| items.iter().map(&mut valid).collect())
        .unwrap_or_default();
    let mut broken: Vec<toml_edit::Table> = Vec::new();
    // The valid entries as Gezik writes them, with the entry as written.
    let mut written: Vec<(toml::Table, toml_edit::Table)> = Vec::new();
    // Comments above the entries that are replaced: the end of the table before them (see below).
    let mut comments = String::new();
    match doc.remove(key) {
        None => {}
        Some(toml_edit::Item::ArrayOfTables(array)) => {
            for (i, table) in array.into_iter().enumerate() {
                match valid.get(i).cloned().flatten() {
                    None => broken.push(table),
                    Some(normal) => written.push((normal, table)),
                }
            }
        }
        Some(toml_edit::Item::Value(toml_edit::Value::Array(array))) => {
            for (i, value) in array.into_iter().enumerate() {
                if valid.get(i).is_some_and(Option::is_some) {
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
    // Each table equal to an entry already there is that entry as written.
    let kept: Vec<Option<toml_edit::Table>> = tables
        .iter()
        .map(|table| written.iter().position(|(normal, _)| normal == table).map(|at| written.remove(at).1))
        .collect();
    // The comments above the entries that go (or change) stay, above the first table.
    for (_, table) in &written {
        if let Some(prefix) = table.decor().prefix().and_then(|p| p.as_str())
            && prefix.contains('#')
        {
            comments.push_str(prefix);
        }
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
        for (table, kept) in tables.iter().zip(kept) {
            if let Some(kept) = kept {
                array.push(kept);
                continue;
            }
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

    #[test]
    fn a_new_table_follows_a_blank_line() {
        let dots = || toml_edit::Value::from("auto");
        let text = "[[commands]]\nname = \"a\"\nrun = [\"x\"]\n";
        assert_eq!(with_tags(text, "dots", dots()).unwrap(), format!("{text}\n[tags]\ndots = \"auto\"\n"));
        let commented = "x = 1\n# a note\n";
        assert_eq!(
            with_tags(commented, "dots", dots()).unwrap(),
            format!("{commented}[tags]\ndots = \"auto\"\n"),
            "a comment stays right above"
        );
        assert_eq!(with_tags("", "dots", dots()).unwrap(), "[tags]\ndots = \"auto\"\n", "no blank line at the top");
    }

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
            group: gezik_core::group::GroupBy::Size,
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
        assert_eq!(v["group"].as_str(), Some("size"));
        assert_eq!(v["icons"].as_str(), Some("gezik"));

        let added = with_view_defaults("theme = \"auto\"\n", &view).unwrap();
        assert_eq!(added.parse::<toml::Table>().unwrap()["view"]["mode"].as_str(), Some("grid"));
        assert!(with_view_defaults("view = 3\n", &view).is_err());
        assert!(with_view_defaults("theme = \n", &view).is_err());
    }

    #[test]
    fn a_view_option_is_written_keeping_its_comment() {
        use crate::settings::{Settings, ViewOption};
        use gezik_core::view::DateFormat;
        let template = include_str!("../templates/settings.toml");
        let out = with_view_option(template, ViewOption::DateFormat(DateFormat::Relative)).unwrap();
        let line = out.lines().find(|l| l.starts_with("date-format")).unwrap();
        assert!(
            line.starts_with("date-format = \"relative\"") && line.ends_with("# relative | short | iso | system"),
            "{line}"
        );
        // show-hidden is a comment in the template: the key is added, the comment stays.
        let out = with_view_option(template, ViewOption::ShowHidden(false)).unwrap();
        assert!(out.contains("# show-hidden = true"), "{out}");
        let settings = Settings::parse("settings.toml", &out, &mut Vec::new());
        assert!(!settings.view.options.show_hidden);
        assert_eq!(settings.view.view, gezik_core::view::ViewSettings::default(), "the rest of [view] stays");
        let added = with_view_option(
            "theme = \"auto\"
",
            ViewOption::HideExtensions(true),
        )
        .unwrap();
        assert_eq!(added.parse::<toml::Table>().unwrap()["view"]["hide-extensions"].as_bool(), Some(true));
        assert!(
            with_view_option(
                "view = 3
",
                ViewOption::HideExtensions(true)
            )
            .is_err()
        );
        assert!(
            with_view_option(
                "theme = 
",
                ViewOption::HideExtensions(true)
            )
            .is_err()
        );
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
        let set = TabSet {
            name: "Release".into(),
            tabs: vec!["{downloads}".into(), "drives".into()],
            right: vec!["{home}".into()],
        };
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
        let one = [TabSet { name: "X".into(), tabs: vec!["/x".into()], right: Vec::new() }];
        let back = with_tab_sets(&with_tab_sets(template, &one).unwrap(), &[]).unwrap();
        for line in template.lines().filter(|l| l.starts_with('#')) {
            assert!(back.lines().any(|b| b == line), "{line} lost:\n{back}");
        }
    }

    #[test]
    fn hand_written_searches_survive_a_save() {
        let text = "# my searches\n[[searches]]\nname = \"Old\"\nfolder = \"{home}\"\npattern = \"*.log\"\n\n\
                    [[searches]]\nname = \"Broken\"\nfolder = \"{home}\"\nsize-min = \"lots\"\n";
        let mut spec = gezik_core::search::SearchSpec::new(gezik_core::search::Scope::AllDrives);
        spec.pattern = "*.mp4".into();
        let new = crate::settings::SavedSearch { name: "Videos".into(), folder: "{here}".into(), spec };
        let out = with_searches(text, &[new]).unwrap();
        assert!(out.contains("name = \"Videos\""), "{out}");
        assert!(
            out.contains("name = \"Broken\"") && out.contains("size-min = \"lots\""),
            "a bad entry stays as written: {out}"
        );
        assert!(!out.contains("name = \"Old\""), "a good one is replaced by the list: {out}");
        assert!(out.contains("# my searches"), "{out}");
    }

    #[test]
    fn an_unchanged_entry_keeps_its_comments() {
        use crate::settings::Settings;
        let text = "[[searches]]
name = \"Rust files\"   # keep this comment
folder = \"{home}\"
pattern = \"*.rs\"

                    [[searches]]
name = \"Logs\" # changes
folder = \"{home}\"
pattern = \"*.log\"
";
        let mut read = Settings::parse("settings.toml", text, &mut Vec::new()).searches;
        read[1].spec.pattern = "*.txt".into();
        let out = with_searches(text, &read).unwrap();
        assert!(
            out.contains(
                "name = \"Rust files\"   # keep this comment
"
            ),
            "{out}"
        );
        assert!(out.contains("pattern = \"*.txt\"") && !out.contains("*.log"), "{out}");
        assert_eq!(Settings::parse("settings.toml", &out, &mut Vec::new()).searches, read);
        let tatil = [tatil()];
        let hand = with_rename_presets("", &tatil).unwrap().replace("name = \"Tatil\"", "name = \"Tatil\" # mine");
        assert!(with_rename_presets(&hand, &tatil).unwrap().contains("# mine"), "presets too");
    }

    #[test]
    fn entries_the_settings_leave_out_survive_a_save() {
        use crate::settings::{SEARCHES_MAX, SavedFilter, Settings};
        let mut text = String::new();
        for i in 0..=SEARCHES_MAX {
            text.push_str(&format!(
                "[[searches]]
name = \"n{i}\"
folder = \"{{here}}\"
"
            ));
        }
        text.push_str(
            "[[searches]]
name = \"N0\"
folder = \"drives\"
",
        );
        let mut warnings = Vec::new();
        let read = Settings::parse("settings.toml", &text, &mut warnings).searches;
        let out = with_searches(&text, &read[1..]).unwrap();
        assert!(!out.contains("name = \"n0\""), "a read one is replaced by the list: {out}");
        let over = format!("name = \"n{SEARCHES_MAX}\"");
        assert!(out.contains(&over) && out.contains("name = \"N0\""), "the 31st and the copy stay: {out}");
        let text = "[[filters]]
name = \"a\"
pattern = \"x\"
[[filters]]
name = \"A\"
pattern = \"y\"
";
        let out = with_filters(text, &[SavedFilter { name: "b".into(), pattern: "z".into() }]).unwrap();
        assert!(!out.contains("\"a\"") && out.contains("name = \"A\""), "the copy stays: {out}");
    }

    #[test]
    fn with_system_keeps_comments_and_other_keys() {
        let text = "[system]\n# mine\nsingle-instance = true\ntray = false   # the icon\n";
        let on = with_system(text, "tray", true.into()).unwrap();
        assert_eq!(on, "[system]\n# mine\nsingle-instance = true\ntray = true   # the icon\n");
        let shortcut = with_system(&on, "hotkey", "win+shift+e".into()).unwrap();
        assert!(shortcut.ends_with("hotkey = \"win+shift+e\"\n"), "{shortcut}");
        let fresh = with_system("pinned = []\n", "tray", true.into()).unwrap();
        assert!(fresh.contains("[system]\ntray = true"), "{fresh}");
        assert!(with_system("system = 3\n", "tray", true.into()).is_err());
    }

    #[test]
    fn the_tag_list_and_dots_are_written_into_tags() {
        let text = "[tags]\n# yours\ndots = \"auto\"   # auto, true or false\n";
        let list = vec![gezik_core::meta::Tag::new("Work", gezik_core::meta::TagColor::Blue)];
        let written = with_tag_added(text, &list[0], &list).unwrap();
        let off = with_tags(&written, "dots", crate::tags::dots_value(crate::tags::Dots::Off)).unwrap();
        assert!(off.contains("# yours\ndots = false   # auto, true or false\n"), "{off}");
        let mut warnings = Vec::new();
        let settings = crate::settings::Settings::parse("settings.toml", &off, &mut warnings);
        assert!(warnings.is_empty(), "{warnings:?}\n{off}");
        assert_eq!(settings.tags.list, list);
        assert_eq!(settings.tags.dots, crate::tags::Dots::Off);
    }

    #[test]
    fn a_new_tags_table_goes_after_the_files_last_comment() {
        use gezik_core::meta::{Tag, TagColor};
        let text = "[panes]\nconfirm = true\n\n# my notes at the end\n";
        let list = [Tag::new("Proj", TagColor::None)];
        let added = with_tag_added(text, &list[0], &list).unwrap();
        let comment = added.find("# my notes at the end").unwrap();
        let table = added.find("[tags]").unwrap();
        assert!(comment < table, "{added}");
        assert!(added.contains("[panes]\nconfirm = true\n"), "{added}");
        let dots = with_tags(text, "dots", crate::tags::dots_value(crate::tags::Dots::On)).unwrap();
        assert!(dots.find("# my notes").unwrap() < dots.find("[tags]").unwrap(), "{dots}");
    }

    #[test]
    fn list_edits_keep_the_comments_and_what_gezik_left_out() {
        use crate::tags::TagListEdit;
        use gezik_core::meta::{Tag, TagColor};
        let text = "# my settings\n[tags]\nlist = [\n  # mine first\n  { name = \"Work\", color = \"blue\" },   # the job\n  { name = \"a,b\" },   # broken, left out\n  \"Home\",\n  { name = \"Late\", color = \"red\" },\n]\ndots = \"auto\"   # auto, true or false\n";
        let list =
            vec![Tag::new("Work", TagColor::Blue), Tag::new("Home", TagColor::None), Tag::new("Late", TagColor::Red)];
        let read = |text: &str| {
            let table: toml::Table = text.parse().unwrap();
            let mut warnings = Vec::new();
            crate::tags::parse(table["tags"].as_table().unwrap(), "s", &mut warnings).list
        };
        let names = |text: &str| read(text).iter().map(|t| t.name.to_string()).collect::<Vec<_>>();
        let renamed = with_tag_list_edit(text, &TagListEdit::Rename("work".into(), "Job".into()), &list).unwrap();
        assert_eq!(names(&renamed), ["Job", "Home", "Late"]);
        let colored = with_tag_list_edit(&renamed, &TagListEdit::Color("Home".into(), TagColor::Green), &list).unwrap();
        assert_eq!(read(&colored)[1].color, TagColor::Green, "a bare name becomes a table");
        let effect = read(&colored);
        let moved = with_tag_list_edit(&colored, &TagListEdit::Move("Home".into(), true), &effect).unwrap();
        assert_eq!(names(&moved), ["Home", "Job", "Late"], "past the entry Gezik left out");
        let removed = with_tag_list_edit(&moved, &TagListEdit::Remove("Late".into()), &list).unwrap();
        assert_eq!(names(&removed), ["Home", "Job"]);
        for kept in
            ["# my settings", "# mine first", "# broken, left out", "{ name = \"a,b\" }", "# auto, true or false"]
        {
            assert!(removed.contains(kept), "{kept:?} in {removed}");
        }
        assert!(with_tag_list_edit(text, &TagListEdit::Remove("Nope".into()), &list).is_err());
        // No list in the file: the list in effect, edited.
        let fresh = with_tag_list_edit("[tags]\n", &TagListEdit::Remove("Home".into()), &list).unwrap();
        assert_eq!(names(&fresh), ["Work", "Late"]);
    }

    #[test]
    fn a_moved_tag_takes_its_comment_lines_along() {
        use crate::tags::TagListEdit;
        use gezik_core::meta::{Tag, TagColor};
        let text = "[tags]\nlist = [\n  # work first\n  { name = \"Work\", color = \"blue\" },\n  # home stuff\n  # more\n  { name = \"Home\" },\n]\n";
        let list = vec![Tag::new("Work", TagColor::Blue), Tag::new("Home", TagColor::None)];
        let moved = with_tag_list_edit(text, &TagListEdit::Move("Home".into(), true), &list).unwrap();
        let home = moved.find("# home stuff").unwrap();
        assert!(home < moved.find("# more").unwrap() && moved.find("# more").unwrap() < moved.find("Home").unwrap());
        assert!(moved.find("Home").unwrap() < moved.find("# work first").unwrap(), "{moved}");
        assert!(moved.find("# work first").unwrap() < moved.find("Work").unwrap(), "{moved}");
        let back =
            with_tag_list_edit(&moved, &TagListEdit::Move("Home".into(), false), &[list[1].clone(), list[0].clone()])
                .unwrap();
        assert_eq!(back, text, "and back as it was");
    }

    #[test]
    fn a_new_tag_is_added_and_the_rest_of_the_list_stays_as_written() {
        use gezik_core::meta::{Tag, TagColor};
        let text = "[tags]\nlist = [\n  # mine first\n  { name = \"Work\", color = \"blue\" },\n  { name = \"a,b\" },   # broken, left out\n]\n";
        let added = with_tag_added(text, &Tag::new("Proj", TagColor::None), &[]).unwrap();
        assert!(added.contains("  # mine first\n  { name = \"Work\", color = \"blue\" },\n"), "{added}");
        assert!(added.contains("{ name = \"a,b\" },   # broken, left out"), "{added}");
        assert!(added.contains("{ name = \"Proj\", color = \"none\" }"), "{added}");
        let mut warnings = Vec::new();
        let settings = crate::settings::Settings::parse("settings.toml", &added, &mut warnings);
        let names: Vec<&str> = settings.tags.list.iter().map(|t| &*t.name).collect();
        assert_eq!(names, ["Work", "Proj"]);
        assert_eq!(warnings.len(), 1, "the broken entry is still there: {warnings:?}");
        assert!(with_tag_added("[tags]\nlist = 3\n", &Tag::new("X", TagColor::None), &[]).is_err());
    }

    #[test]
    fn dont_ask_again_turns_the_pane_question_off() {
        let text = "[panes]\nconfirm = true           # F5/F6 ask\n";
        assert_eq!(with_panes_confirm(text, false).unwrap(), "[panes]\nconfirm = false           # F5/F6 ask\n");
        assert!(with_panes_confirm("", false).unwrap().contains("[panes]\nconfirm = false"));
    }

    /// The lines of `after` that differ from `before`'s, which must have as many.
    fn changed_lines(before: &str, after: &str) -> Vec<String> {
        assert_eq!(before.lines().count(), after.lines().count(), "{after}");
        before.lines().zip(after.lines()).filter(|(a, b)| a != b).map(|(_, b)| b.to_owned()).collect()
    }

    #[test]
    fn a_value_changes_only_its_own_line() {
        let template = include_str!("../templates/settings.toml");
        // Commented, with an inline hint, at the top, a number.
        let out = with_value(template, Some("layout"), "density", Some("\"compact\"")).unwrap();
        assert_eq!(changed_lines(template, &out), ["density = \"compact\""]);
        let out = with_value(template, Some("view"), "sort", Some("\"size\"")).unwrap();
        assert_eq!(
            changed_lines(template, &out),
            ["sort = \"size\"          # name | modified | created | type | size | tags | rating"]
        );
        let out = with_value(template, None, "theme", Some("\"nord\"")).unwrap();
        assert_eq!(changed_lines(template, &out), ["theme = \"nord\""]);
        let out = with_value(template, Some("search"), "max-results", Some("5000")).unwrap();
        assert_eq!(changed_lines(template, &out), ["max-results = 5000       # 1000-2000000; a search stops there"]);
        // Back to what it was: the same bytes.
        let back = with_value(&out, Some("search"), "max-results", Some("250000")).unwrap();
        assert_eq!(back, template);
    }

    #[test]
    fn a_missing_key_or_table_is_added_and_the_rest_stays() {
        let out = with_value("# mine\n[layout]\nsidebar = \"left\"\n", Some("layout"), "density", Some("\"compact\""))
            .unwrap();
        assert_eq!(out, "# mine\n[layout]\nsidebar = \"left\"\ndensity = \"compact\"\n");
        let out = with_value("theme = \"auto\"\n# the end\n", Some("panes"), "confirm", Some("false")).unwrap();
        assert_eq!(out, "theme = \"auto\"\n# the end\n[panes]\nconfirm = false\n");
        let out = with_value("[layout]\ndensity = \"compact\"\n", None, "max-fps", Some("60")).unwrap();
        assert_eq!(out, "max-fps = 60\n[layout]\ndensity = \"compact\"\n");
        let out = with_value("[view]\nunknown = 1 # mine\nmode = \"list\"\n", Some("view"), "mode", Some("\"grid\""))
            .unwrap();
        assert_eq!(out, "[view]\nunknown = 1 # mine\nmode = \"grid\"\n", "unknown keys stay");
    }

    #[test]
    fn inline_tables_and_dotted_keys_are_edited_where_they_are() {
        let out = with_value(
            "layout = { density = \"compact\", sidebar = \"left\" }\n",
            Some("layout"),
            "density",
            Some("\"comfortable\""),
        )
        .unwrap();
        assert_eq!(out, "layout = { density = \"comfortable\", sidebar = \"left\" }\n");
        let out =
            with_value("layout.density = \"compact\" # dense\n", Some("layout"), "density", Some("\"comfortable\""))
                .unwrap();
        assert_eq!(out, "layout.density = \"comfortable\" # dense\n");
    }

    #[test]
    fn a_value_taken_out_and_a_broken_file() {
        // The comment above a key taken out stays, above the next key or the next table.
        let text = "[view]\n# hidden things\nshow-hidden = false # mine\nmode = \"list\"\n";
        let out = with_value(text, Some("view"), "show-hidden", None).unwrap();
        assert_eq!(out, "[view]\n# hidden things\nmode = \"list\"\n");
        let text = "[view]\nmode = \"list\"\n# about hidden\nshow-hidden = false\n\n[tags]\ndots = true\n";
        let out = with_value(text, Some("view"), "show-hidden", None).unwrap();
        assert_eq!(out, "[view]\nmode = \"list\"\n# about hidden\n\n[tags]\ndots = true\n");
        let text = "[view]\nmode = \"list\"\n# about hidden\nshow-hidden = false\n";
        let out = with_value(text, Some("view"), "show-hidden", None).unwrap();
        assert_eq!(out, "[view]\nmode = \"list\"\n# about hidden\n", "the last table: the file's end");
        let text = "# fps\nmax-fps = 60\n\n[view]\nmode = \"list\"\n";
        let out = with_value(text, None, "max-fps", None).unwrap();
        assert_eq!(out, "# fps\n\n[view]\nmode = \"list\"\n", "the top: before the first table");
        let template = include_str!("../templates/settings.toml");
        let out = with_value(template, Some("layout"), "density", None).unwrap();
        assert!(out.contains("# compact | comfortable\n# true: no fades"), "a template comment stays");
        let text = "[view]\nshow-hidden = false\nmode = \"list\"\n";
        assert_eq!(with_value(text, Some("view"), "show-hidden", None).unwrap(), "[view]\nmode = \"list\"\n");
        assert_eq!(with_value("[view]\n", Some("tags"), "dots", None).unwrap(), "[view]\n", "nothing to take out");
        assert!(with_value("[view\n", Some("view"), "mode", Some("\"grid\"")).is_err());
        assert!(with_value("[view]\n", Some("view"), "mode", Some("grid")).is_err(), "not a TOML value");
    }

    #[test]
    fn a_commands_shortcut_is_set_in_its_own_entry() {
        let text = "# mine
[[commands]]
name = \"A\"
run = [\"a\"]
shortcut = \"ctrl+k\"  # mine too

[[commands]]
name = \"B\"
run = [\"b\"]
";
        let a = with_command_shortcut(text, "A", "").unwrap();
        assert_eq!(a, text.replace("\"ctrl+k\"  #", "\"\"  #"), "the comment stays");
        let b = with_command_shortcut(text, "B", "ctrl+alt+b").unwrap();
        assert_eq!(
            b,
            format!(
                "{text}shortcut = \"ctrl+alt+b\"
"
            )
        );
        assert!(with_command_shortcut(text, "C", "f9").is_err());
        assert!(
            with_command_shortcut(
                "[x
", "A", "f9"
            )
            .is_err()
        );
    }

    #[test]
    fn a_shortcut_is_one_line_in_the_template() {
        let template = crate::store::SETTINGS_TEMPLATE;
        let edited = with_value(template, Some("shortcuts"), "copy-path", Some("\"mod+alt+c\"")).unwrap();
        let mut lines: Vec<&str> = edited.lines().collect();
        let at = lines.iter().position(|l| *l == "copy-path = \"mod+alt+c\"").expect("written");
        assert_eq!(lines[at - 1], "[shortcuts]");
        lines.remove(at);
        assert_eq!(lines, template.lines().collect::<Vec<_>>(), "nothing else changes");
    }
}
