//! The sync folder (spec 14 §8): where `location.toml` says settings.toml, the themes and the
//! templates are; the `settings.local.toml` layer for this computer's own values; the sync
//! services' conflicting copies and the key-by-key differences a Review shows. Pure but for
//! the small reads and writes named so.

use std::io;
use std::path::{Path, PathBuf};

use toml::{Table, Value};

use crate::paths::KnownDirs;

/// In the local folder: where the shared folder is (`shared = "{home}/Dropbox/Gezik"`).
pub const LOCATION_FILE: &str = "location.toml";
/// In the local folder: this computer's values over settings.toml's (read only with a sync folder).
pub const LOCAL_SETTINGS: &str = "settings.local.toml";
/// In the local folder: the last copy of the sync folder's settings.toml and themes.
pub const CACHE: &str = "sync-cache";
/// In the sync folder: an empty file per computer that uses it, named by the computer (OneDrive
/// names its conflicting copies `settings-<COMPUTER>.toml`).
pub const MACHINES: &str = "machines";

/// The status bar's note when the sync folder cannot be read at start.
pub const OFFLINE_NOTE: &str =
    "Settings folder not available — using the last copy (changes are not saved until it is back)";

/// The folder `location.toml`'s text names (its `{token}` expanded), if it names one.
pub fn parse_location(text: &str, known: &KnownDirs) -> Option<PathBuf> {
    let table = text.parse::<Table>().ok()?;
    let shared = table.get("shared")?.as_str()?.trim();
    (!shared.is_empty()).then(|| known.expand_checked(shared)).flatten()
}

/// `location.toml`'s text for `folder`: under a known folder it is written with its `{token}`,
/// so the same Dropbox found elsewhere on another system is still found.
pub fn location_text(folder: &Path, known: &KnownDirs) -> String {
    let shared = Value::String(known.collapse(folder));
    format!(
        "# Where Gezik keeps settings.toml, the themes and the templates (Settings > Sync and Backup).\nshared = {shared}\n"
    )
}

/// The sync folder `local`'s `location.toml` names: one `open` when there is none.
pub fn read_location(local: &Path, known: &KnownDirs) -> Option<PathBuf> {
    let text = std::fs::read_to_string(local.join(LOCATION_FILE)).ok()?;
    parse_location(&text, known)
}

/// A conflicting copy's original file (`settings.toml`), and whether only a number in its name
/// says it is one (`settings (1).toml`, `settings 2.toml`): a file of the user's own may be named
/// so, so such a name counts only for settings.toml, only when its contents differ, and such a
/// copy is never moved to the trash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub original: String,
    pub numbered: bool,
}

/// The file a sync service's conflicting copy `name` is of (`settings (Ann's conflicted copy
/// 2026-10-12).toml` → `settings.toml`), if its name has one of their forms: Dropbox and
/// Nextcloud (`(… conflicted copy …)`, in Turkish `çakışan kopya`), Syncthing
/// (`.sync-conflict-…`), ownCloud (`_conflict-…`), OneDrive (`settings-<COMPUTER>`, only for the
/// computers in `machines`), Gezik's own (`(Gezik conflicted copy)`); numbered only: Google
/// Drive and Windows (`(1)`), iCloud (`settings 2`). The caller checks that the original is there.
pub fn conflict_of(name: &str, machines: &[String]) -> Option<Conflict> {
    let (stem, ext) = name.rsplit_once('.')?;
    let found = |base: &str, numbered: bool| {
        (!base.is_empty() && !base.starts_with('.')).then(|| Conflict { original: format!("{base}.{ext}"), numbered })
    };
    let digits = |n: &str| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit());
    if let Some((base, rest)) = stem.split_once(".sync-conflict-") {
        return found(base, false).filter(|_| rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
    }
    if let Some((base, rest)) = stem.rsplit_once("_conflict-") {
        return found(base, false).filter(|_| rest.chars().all(|c| c.is_ascii_digit() || c == '-'));
    }
    if let Some((base, inside)) = stem.strip_suffix(')').and_then(|s| s.rsplit_once(" (")) {
        let lower = inside.to_lowercase();
        if lower.contains("conflict") || lower.contains("çakış") {
            return found(base, false);
        }
        return found(base, true).filter(|_| digits(inside));
    }
    if let Some((base, n)) = stem.rsplit_once(' ') {
        return found(base, true).filter(|_| digits(n));
    }
    // OneDrive: `-<COMPUTER>`, maybe `-<COMPUTER>-2`.
    let unnumbered = match stem.rsplit_once('-') {
        Some((rest, n)) if digits(n) => rest,
        _ => stem,
    };
    machines.iter().find_map(|machine| {
        let suffix = format!("-{machine}");
        let base = [stem, unnumbered].into_iter().find_map(|s| strip_suffix_ignoring_case(s, &suffix))?;
        found(base, false)
    })
}

/// A free name for Gezik's own conflicting copy of `name` next to it (`settings (Gezik conflicted
/// copy).toml`, then `… copy 2)…`).
pub fn copy_name(name: &str, taken: impl Fn(&str) -> bool) -> String {
    let (stem, ext) = name.rsplit_once('.').unwrap_or((name, "toml"));
    (1..)
        .map(|n| match n {
            1 => format!("{stem} (Gezik conflicted copy).{ext}"),
            n => format!("{stem} (Gezik conflicted copy {n}).{ext}"),
        })
        .find(|candidate| !taken(candidate))
        .unwrap_or_default()
}

/// `text` without `suffix` at its end, told apart without regard to case.
fn strip_suffix_ignoring_case<'a>(text: &'a str, suffix: &str) -> Option<&'a str> {
    let count = suffix.chars().count();
    let at = text.char_indices().rev().nth(count.checked_sub(1)?)?.0;
    (text[at..].to_lowercase() == suffix.to_lowercase()).then(|| &text[..at])
}

/// The computers that use sync folder `shared` (the names in its `machines/`).
pub fn machines(shared: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(shared.join(MACHINES)) else { return Vec::new() };
    entries.filter_map(Result::ok).filter_map(|e| e.file_name().into_string().ok()).collect()
}

/// Puts `machine`'s name in sync folder `shared`'s `machines/` (once).
pub fn add_machine(shared: &Path, machine: &str) -> io::Result<()> {
    let safe: String = machine.chars().filter(|c| !"/\\:*?\"<>|".contains(*c) && !c.is_control()).collect();
    if safe.trim().is_empty() || safe.starts_with('.') {
        return Ok(());
    }
    let folder = shared.join(MACHINES);
    std::fs::create_dir_all(&folder)?;
    let path = folder.join(safe.trim());
    if !path.exists() {
        std::fs::write(path, "")?;
    }
    Ok(())
}

/// `over`'s keys put into `base` (spec 14 §8.2): tables are merged key by key, all the way down;
/// anything else (a value, a list, a list of tables) is replaced whole.
pub fn layer(base: &mut Table, over: Table) {
    for (key, value) in over {
        match (base.get_mut(&key), value) {
            (Some(Value::Table(mine)), Value::Table(theirs)) => layer(mine, theirs),
            (_, value) => {
                base.insert(key, value);
            }
        }
    }
}

/// A key the two copies of a file differ in: `path` down to it (`["layout", "density"]`), and its
/// value in each, as shown (`None`: not in that copy).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Difference {
    pub path: Vec<String>,
    pub this: Option<String>,
    pub other: Option<String>,
}

impl Difference {
    pub fn key(&self) -> String {
        self.path.join(".")
    }
}

/// The keys `this` and `other` (two copies of a TOML file) differ in, in `this`'s order then the
/// other's new keys. Tables are compared key by key; a list (`[[commands]]`) is one key.
pub fn differences(this: &str, other: &str) -> Result<Vec<Difference>, String> {
    let read = |text: &str, which: &str| text.parse::<Table>().map_err(|err| format!("{which} does not read: {err}"));
    let (mut a, mut b) = (Vec::new(), Vec::new());
    leaves(&mut Vec::new(), &read(this, "This copy")?, &mut a);
    leaves(&mut Vec::new(), &read(other, "The other copy")?, &mut b);
    let mut out: Vec<Difference> = Vec::new();
    for (path, value) in &a {
        let theirs = b.iter().find(|(p, _)| p == path).map(|(_, v)| v);
        if theirs != Some(value) {
            out.push(Difference { path: path.clone(), this: Some(shown(value)), other: theirs.map(shown) });
        }
    }
    for (path, value) in &b {
        if !a.iter().any(|(p, _)| p == path) {
            out.push(Difference { path: path.clone(), this: None, other: Some(shown(value)) });
        }
    }
    Ok(out)
}

fn leaves(prefix: &mut Vec<String>, table: &Table, out: &mut Vec<(Vec<String>, Value)>) {
    for (key, value) in table {
        prefix.push(key.clone());
        match value {
            Value::Table(inner) => leaves(prefix, inner, out),
            value => out.push((prefix.clone(), value.clone())),
        }
        prefix.pop();
    }
}

fn shown(value: &Value) -> String {
    match value {
        Value::Array(items) if !items.is_empty() && items.iter().all(Value::is_table) => {
            format!("{} entries", items.len())
        }
        value => crate::bundle::short(&crate::bundle::shown(value)),
    }
}

/// `text` with the key at `path` as `other` has it (taken out when `other` has none): a Review's
/// "Other copy". Everything else in `text`, comments too, stays.
pub fn with_item_from(text: &str, other: &str, path: &[String]) -> Result<String, String> {
    let theirs = other.parse::<toml_edit::DocumentMut>().map_err(|err| err.to_string().trim().to_owned())?;
    let mut item = Some(theirs.as_item());
    for key in path {
        item = item.and_then(|i| i.as_table_like()).and_then(|t| t.get(key));
    }
    let item = item.cloned();
    // A value one or two keys down: as the settings window writes it (its line and comment stay).
    if path.len() <= 2 && item.as_ref().is_none_or(toml_edit::Item::is_value) {
        let literal = item.as_ref().and_then(toml_edit::Item::as_value).map(|v| {
            let mut v = v.clone();
            v.decor_mut().clear();
            v.to_string()
        });
        let (table, key) = match path {
            [key] => (None, key),
            [table, key] => (Some(table.as_str()), key),
            _ => return Err("No key".to_owned()),
        };
        return crate::settings_edit::with_value(text, table, key, literal.as_deref());
    }
    set_item(text, path, item)
}

/// `text` with `item` at `path` (taken out when `None`), the tables on the way made.
fn set_item(text: &str, path: &[String], item: Option<toml_edit::Item>) -> Result<String, String> {
    let mut doc = text.parse::<toml_edit::DocumentMut>().map_err(|err| err.to_string().trim().to_owned())?;
    let (last, parents) = path.split_last().ok_or("No key")?;
    let mut table: &mut dyn toml_edit::TableLike = doc.as_table_mut();
    for key in parents {
        table = table
            .entry(key)
            .or_insert_with(toml_edit::table)
            .as_table_like_mut()
            .ok_or_else(|| format!("{key} is not a table"))?;
    }
    match item {
        Some(item) => {
            table.insert(last, item);
        }
        None => {
            table.remove(last);
        }
    }
    Ok(doc.to_string())
}

/// `text` without the table `name` when it has no key left (and without it whole when `all`).
pub fn without_empty(text: &str, name: &str) -> Result<String, String> {
    let doc = text.parse::<toml_edit::DocumentMut>().map_err(|err| err.to_string())?;
    match doc.get(name).and_then(toml_edit::Item::as_table_like) {
        Some(table) if table.is_empty() => set_item(text, &[name.to_owned()], None),
        _ => Ok(text.to_owned()),
    }
}

/// `text` without its table (or key) `name`: this computer's `[system]` taken out of the copy that
/// goes to the sync folder (it is in settings.local.toml; decision 12).
pub fn without(text: &str, name: &str) -> Result<String, String> {
    set_item(text, &[name.to_owned()], None)
}

/// `text` with `local`'s keys put in, one by one (tables key by key, lists whole), its comments
/// kept: settings.local.toml over settings.toml, as one file.
pub fn with_layer(text: &str, local: &str) -> Result<String, String> {
    let over = local.parse::<Table>().map_err(|err| format!("{LOCAL_SETTINGS} does not read: {err}"))?;
    let mut paths = Vec::new();
    leaves(&mut Vec::new(), &over, &mut paths);
    paths.into_iter().try_fold(text.to_owned(), |text, (path, _)| with_item_from(&text, local, &path))
}

/// [`three_way`]'s outcome: the file as it is now with our changes in it, and the keys both
/// computers changed differently (left as the file has them).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Merged {
    pub text: String,
    pub conflicts: Vec<Vec<String>>,
}

/// Our change (`base` → `ours`) made again on `theirs`, the file as it is now (another
/// computer's change came in meanwhile): each key we changed takes our value where `theirs`
/// still has the base's; a list both changed gets the items we added, removed or moved
/// ([`merge_list`]); any other key both changed is a conflict and stays as `theirs` has it.
pub fn three_way(base: &str, ours: &str, theirs: &str) -> Result<Merged, String> {
    let read = |text: &str| text.parse::<Table>().map_err(|err| err.to_string());
    let (b, o, t) = (read(base)?, read(ours)?, read(theirs)?);
    let (mut in_base, mut in_ours) = (Vec::new(), Vec::new());
    leaves(&mut Vec::new(), &b, &mut in_base);
    leaves(&mut Vec::new(), &o, &mut in_ours);
    let mut paths: Vec<Vec<String>> = Vec::new();
    for (path, _) in in_base.into_iter().chain(in_ours) {
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
    let mut text = theirs.to_owned();
    let mut conflicts = Vec::new();
    for path in paths {
        let (was, mine, now) = (get(&b, &path), get(&o, &path), get(&t, &path));
        if mine == was || now == mine {
            continue;
        }
        if now == was {
            text = with_item_from(&text, ours, &path)?;
            continue;
        }
        let list = |value: Option<&Value>| match value {
            None => Some(Vec::new()),
            Some(Value::Array(items)) => Some(items.clone()),
            Some(_) => None,
        };
        match (list(was), list(mine), list(now)) {
            (Some(was), Some(mine), Some(now)) => match merge_list(&was, &mine, &now) {
                Some(merged) => text = with_list(&text, &path, merged)?,
                None => conflicts.push(path),
            },
            _ => conflicts.push(path),
        }
    }
    Ok(Merged { text, conflicts })
}

fn get<'a>(table: &'a Table, path: &[String]) -> Option<&'a Value> {
    let (last, parents) = path.split_last()?;
    let mut table = table;
    for key in parents {
        table = table.get(key)?.as_table()?;
    }
    table.get(last)
}

/// `text` with the list at `path` made `items`, in the form it has there (`[[x]]` or `[…]`).
fn with_list(text: &str, path: &[String], items: Vec<Value>) -> Result<String, String> {
    let doc = text.parse::<toml_edit::DocumentMut>().map_err(|err| err.to_string())?;
    let mut current = Some(doc.as_item());
    for key in path {
        current = current.and_then(|i| i.as_table_like()).and_then(|t| t.get(key));
    }
    let tables = current.is_some_and(toml_edit::Item::is_array_of_tables)
        || (current.is_none() && !items.is_empty() && items.iter().all(Value::is_table));
    let item = if tables {
        let mut holder = Table::new();
        holder.insert("x".to_owned(), Value::Array(items));
        let rendered = toml::to_string(&holder).map_err(|err| err.to_string())?;
        let mut doc = rendered.parse::<toml_edit::DocumentMut>().map_err(|err| err.to_string())?;
        doc.remove("x").ok_or("No list")?
    } else {
        let value = Value::Array(items).to_string().parse::<toml_edit::Value>().map_err(|err| err.to_string())?;
        toml_edit::Item::Value(value)
    };
    // In a document of its own at `path`, then through `with_item_from` (a value one or two
    // keys down keeps its line and comment).
    let mut source = toml_edit::DocumentMut::new();
    let (last, parents) = path.split_last().ok_or("No key")?;
    let mut table: &mut dyn toml_edit::TableLike = source.as_table_mut();
    for key in parents {
        table = table.entry(key).or_insert_with(toml_edit::table).as_table_like_mut().ok_or("No table")?;
    }
    table.insert(last, item);
    with_item_from(text, &source.to_string(), path)
}

/// Our edit of a list (`base` → `ours`) made on `theirs`, the list as it is now: the items we
/// took out go, those we added come in after the item before them in ours (at the end when they
/// were added at the end); items we only moved take our order if `theirs` is still the base.
/// `None` when it cannot be said that way (we moved items and theirs changed too).
pub fn merge_list(base: &[Value], ours: &[Value], theirs: &[Value]) -> Option<Vec<Value>> {
    let removed: Vec<&Value> = base.iter().filter(|v| !ours.contains(v)).collect();
    let added: Vec<usize> = (0..ours.len()).filter(|&i| !base.contains(&ours[i])).collect();
    let kept: Vec<&Value> = ours.iter().filter(|v| base.contains(v)).collect();
    let base_kept: Vec<&Value> = base.iter().filter(|v| ours.contains(v)).collect();
    if kept != base_kept {
        // Moved: only onto the list we moved them in.
        return (removed.is_empty() && added.is_empty() && theirs == base).then(|| ours.to_vec());
    }
    let mut out: Vec<Value> = theirs.iter().filter(|v| !removed.contains(v)).cloned().collect();
    for i in added {
        if out.contains(&ours[i]) {
            continue;
        }
        let at_end = ours[i..].iter().all(|v| !base.contains(v));
        let at = if at_end {
            out.len()
        } else {
            ours[..i].iter().rev().find_map(|before| out.iter().position(|v| v == before)).map_or(0, |p| p + 1)
        };
        out.insert(at, ours[i].clone());
    }
    Some(out)
}

/// settings.local.toml's text (`local`) with `settings`' `[system]` table in it, when `settings`
/// has one and `local` none: a computer that takes up a sync folder keeps its own tray and
/// shortcut (decision 12).
pub fn local_with_system(settings: &str, local: &str) -> Option<String> {
    let has = |text: &str| text.parse::<Table>().is_ok_and(|t| t.get("system").is_some_and(Value::is_table));
    (has(settings) && !has(local)).then(|| with_item_from(local, settings, &["system".to_owned()]).ok()).flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn location_keeps_its_token_and_comes_back_on_another_system() {
        let windows = KnownDirs::new(vec![("home", PathBuf::from("/c/Users/ann"))]);
        let text = location_text(Path::new("/c/Users/ann/Dropbox/Gezik"), &windows);
        assert!(text.contains("shared = \"{home}/Dropbox/Gezik\""), "{text}");
        let mac = KnownDirs::new(vec![("home", PathBuf::from("/Users/ann"))]);
        assert_eq!(parse_location(&text, &mac), Some(PathBuf::from("/Users/ann/Dropbox/Gezik")));
        let usb = location_text(Path::new("/media/usb/Gezik"), &mac);
        assert_eq!(parse_location(&usb, &mac), Some(PathBuf::from("/media/usb/Gezik")), "outside: as written");
        assert_eq!(parse_location("shared = \"\"\n", &mac), None);
        assert_eq!(parse_location("shared = \"{home}/../x\"\n", &mac), None, "no climbing out");
        assert_eq!(parse_location("shared = [\n", &mac), None, "broken: no sync folder");
        assert_eq!(parse_location("other = 1\n", &mac), None);
    }

    #[test]
    fn the_location_file_is_read_from_the_local_folder() {
        let dir = crate::test_dir("location");
        let known = KnownDirs::new(vec![("home", dir.clone())]);
        assert_eq!(read_location(&dir, &known), None, "none: no sync folder");
        std::fs::write(dir.join(LOCATION_FILE), location_text(&dir.join("Sync/Gezik"), &known)).unwrap();
        assert_eq!(read_location(&dir, &known), Some(dir.join("Sync").join("Gezik")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn each_services_conflicting_copy_is_recognized() {
        let machines = names(&["DESKTOP-AB12", "TEOMAN-MASAÜSTÜ"]);
        let of = |name: &str| conflict_of(name, &machines);
        let named = |original: &str| Some(Conflict { original: original.to_owned(), numbered: false });
        let numbered = |original: &str| Some(Conflict { original: original.to_owned(), numbered: true });
        // Dropbox, English and Turkish.
        assert_eq!(of("settings (Ann's conflicted copy 2026-10-12).toml"), named("settings.toml"));
        assert_eq!(of("settings (Teoman adlı kişinin çakışan kopyası 2026-10-12).toml"), named("settings.toml"));
        // Nextcloud, ownCloud, Gezik's own.
        assert_eq!(of("settings (conflicted copy 2026-10-12 101010).toml"), named("settings.toml"));
        assert_eq!(of("settings_conflict-20261012-101010.toml"), named("settings.toml"));
        assert_eq!(of("settings (Gezik conflicted copy 2).toml"), named("settings.toml"));
        // Syncthing.
        assert_eq!(of("settings.sync-conflict-20261012-101010-ABCDEFG.toml"), named("settings.toml"));
        // OneDrive: this computer's and the known ones', any case, numbered too.
        assert_eq!(of("settings-DESKTOP-AB12.toml"), named("settings.toml"));
        assert_eq!(of("settings-desktop-ab12-2.toml"), named("settings.toml"));
        assert_eq!(of("settings-TEOMAN-MASAÜSTÜ.toml"), named("settings.toml"));
        // Google Drive and Windows, iCloud: only a number says so.
        assert_eq!(of("settings (1).toml"), numbered("settings.toml"));
        assert_eq!(of("settings 2.toml"), numbered("settings.toml"));
        // A theme's.
        assert_eq!(of("nord (Ann's conflicted copy 2026-10-12).toml"), named("nord.toml"));
        assert_eq!(of("my theme.sync-conflict-20261012-1-X.toml"), named("my theme.toml"));
        assert_eq!(of("Dracula 2.toml"), numbered("Dracula.toml"), "the caller does not take it for a theme");
    }

    #[test]
    fn gezik_names_its_own_copy_freely() {
        assert_eq!(copy_name("settings.toml", |_| false), "settings (Gezik conflicted copy).toml");
        let taken = |n: &str| n == "nord (Gezik conflicted copy).toml";
        assert_eq!(copy_name("nord.toml", taken), "nord (Gezik conflicted copy 2).toml");
    }

    fn list(items: &[&str]) -> Vec<Value> {
        items.iter().map(|s| Value::String((*s).to_owned())).collect()
    }

    #[test]
    fn a_list_change_is_made_again_item_by_item() {
        let (x, w, y, z) = ("x", "w", "y", "z");
        // A pinned w on [x] while B pinned y and z: all four stay.
        assert_eq!(merge_list(&list(&[x]), &list(&[x, w]), &list(&[x, y, z])), Some(list(&[x, y, z, w])));
        // Taken out here, added there.
        assert_eq!(merge_list(&list(&[x, y]), &list(&[x]), &list(&[x, y, z])), Some(list(&[x, z])));
        // Edited (renamed) in place: after the item before it.
        assert_eq!(
            merge_list(&list(&[x, y, z]), &list(&[x, w, z]), &list(&[x, y, z, "v"])),
            Some(list(&[x, w, z, "v"]))
        );
        // Moved: only onto the list as it was.
        assert_eq!(merge_list(&list(&[x, y]), &list(&[y, x]), &list(&[x, y])), Some(list(&[y, x])));
        assert_eq!(merge_list(&list(&[x, y]), &list(&[y, x]), &list(&[x, y, z])), None);
    }

    #[test]
    fn a_change_made_on_an_old_copy_keeps_the_newer_file() {
        let base = "# mine\ntheme = \"dark\"\npinned = [\"x\"]\n[layout]\ndensity = \"compact\"\n";
        let ours = "# mine\ntheme = \"nord\"\npinned = [\"x\", \"w\"]\n[layout]\ndensity = \"compact\"\n";
        let theirs = "# mine\ntheme = \"dark\"\npinned = [\"x\", \"y\", \"z\"]\n[layout]\ndensity = \"comfortable\"\n";
        let merged = three_way(base, ours, theirs).unwrap();
        assert!(merged.conflicts.is_empty());
        let table: Table = merged.text.parse().unwrap();
        assert_eq!(table["theme"].as_str(), Some("nord"));
        assert_eq!(table["pinned"], Value::Array(list(&["x", "y", "z", "w"])));
        assert_eq!(table["layout"]["density"].as_str(), Some("comfortable"), "theirs, not touched by us");
        assert!(merged.text.starts_with("# mine"));
        // Both changed the theme: a conflict, theirs stays.
        let other = "theme = \"light\"\npinned = [\"x\"]\n[layout]\ndensity = \"compact\"\n";
        let merged = three_way(base, ours, other).unwrap();
        assert_eq!(merged.conflicts, [vec!["theme".to_owned()]]);
        assert!(merged.text.contains("theme = \"light\"") && merged.text.contains("\"w\""));
        // Lists of tables keep their form.
        let base = "[[filters]]\nname = \"A\"\n";
        let ours = "[[filters]]\nname = \"A\"\n[[filters]]\nname = \"B\"\n";
        let theirs = "[[filters]]\nname = \"A\"\n[[filters]]\nname = \"C\"\n";
        let merged = three_way(base, ours, theirs).unwrap().text;
        assert_eq!(merged.matches("[[filters]]").count(), 3, "{merged}");
    }

    #[test]
    fn the_local_layer_is_written_into_a_file_key_by_key() {
        let shared = "# shared\ntheme = \"dark\"\n[system]\ntray = true\n[layout]\ndensity = \"compact\"\n";
        let local = "[system]\ntray = false\nhotkey = \"\"\n[layout]\nsidebar = \"right\"\n";
        let out = with_layer(shared, local).unwrap();
        let table: Table = out.parse().unwrap();
        assert_eq!(table["system"]["tray"].as_bool(), Some(false));
        assert_eq!(table["layout"]["density"].as_str(), Some("compact"));
        assert_eq!(table["layout"]["sidebar"].as_str(), Some("right"));
        assert!(out.starts_with("# shared"));
        assert!(with_layer(shared, "[x\n").is_err());
    }

    #[test]
    fn names_that_only_look_alike_are_not_conflicts() {
        let machines = names(&["DESKTOP-AB12"]);
        for name in [
            "settings.toml",
            "settings-old.toml",
            "settings-backup.toml",
            "settings-LAPTOP.toml",
            "settings (old).toml",
            "settings (copy).toml",
            "settings (1b).toml",
            "settings 2b.toml",
            "settings ().toml",
            "(1).toml",
            ".settings.toml.123.tmp",
            "settings",
        ] {
            assert_eq!(conflict_of(name, &machines), None, "{name}");
        }
    }

    #[test]
    fn the_local_layer_wins_table_by_table_and_lists_whole() {
        let mut base: Table = "theme = \"dark\"\npinned = [\"a\", \"b\"]\n[layout]\ndensity = \"compact\"\nsidebar = \"left\"\n[[commands]]\nname = \"A\"\n[[commands]]\nname = \"B\"\n".parse().unwrap();
        let over: Table =
            "pinned = [\"c\"]\n[layout]\nsidebar = \"right\"\n[system]\ntray = true\n[[commands]]\nname = \"C\"\n"
                .parse()
                .unwrap();
        layer(&mut base, over);
        let want: Table = "theme = \"dark\"\npinned = [\"c\"]\n[layout]\ndensity = \"compact\"\nsidebar = \"right\"\n[system]\ntray = true\n[[commands]]\nname = \"C\"\n".parse().unwrap();
        assert_eq!(base, want);
    }

    #[test]
    fn differences_are_listed_key_by_key_with_lists_whole() {
        let this = "theme = \"nord\"\n[layout]\ndensity = \"compact\"\n[[commands]]\nname = \"A\"\n";
        let other = "theme = \"dark\"\n[layout]\ndensity = \"compact\"\nsidebar = \"right\"\n[[commands]]\nname = \"A\"\n[[commands]]\nname = \"B\"\n";
        let found = differences(this, other).unwrap();
        let keys: Vec<String> = found.iter().map(Difference::key).collect();
        assert_eq!(keys, ["theme", "commands", "layout.sidebar"]);
        assert_eq!((found[0].this.as_deref(), found[0].other.as_deref()), (Some("nord"), Some("dark")));
        assert_eq!(found[1].other.as_deref(), Some("2 entries"));
        assert_eq!((found[2].this.as_deref(), found[2].other.as_deref()), (None, Some("right")));
        assert!(differences(this, this).unwrap().is_empty());
        assert!(differences("[x\n", this).unwrap_err().starts_with("This copy"));
    }

    #[test]
    fn this_computers_system_table_goes_to_the_local_file_once() {
        let settings = "theme = \"dark\"\n[system]\ntray = true\nhotkey = \"win+e\"\n";
        let local = local_with_system(settings, "# mine\n").unwrap();
        let table: Table = local.parse().unwrap();
        assert_eq!(table["system"]["tray"].as_bool(), Some(true));
        assert!(local.contains("# mine"), "{local}");
        assert_eq!(local_with_system(settings, &local), None, "has one already");
        assert_eq!(local_with_system("theme = \"dark\"\n", ""), None, "none to copy");
    }

    #[test]
    fn the_other_copys_key_is_written_and_the_rest_stays() {
        let this = "# mine\ntheme = \"nord\" # pretty\n[layout]\ndensity = \"compact\"\n\n[[commands]]\nname = \"A\"\n";
        let other = "theme = \"dark\"\n[layout]\nsidebar = \"right\"\n[preview.text]\nwrap = true\n[[commands]]\nname = \"B\"\n";
        let path = |p: &[&str]| names(p);
        let out = with_item_from(this, other, &path(&["theme"])).unwrap();
        assert!(out.starts_with("# mine\ntheme = \"dark\" # pretty\n"), "{out}");
        let out = with_item_from(&out, other, &path(&["layout", "density"])).unwrap();
        assert!(!out.contains("density"), "not in the other copy: taken out\n{out}");
        let out = with_item_from(&out, other, &path(&["layout", "sidebar"])).unwrap();
        let out = with_item_from(&out, other, &path(&["preview", "text", "wrap"])).unwrap();
        let out = with_item_from(&out, other, &path(&["commands"])).unwrap();
        assert!(differences(&out, other).unwrap().is_empty(), "{out}");
        assert!(out.contains("# mine"), "{out}");
    }
}
