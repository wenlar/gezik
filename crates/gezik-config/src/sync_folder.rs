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

/// The file a sync service's conflicting copy `name` is of (`settings (Ann's conflicted copy
/// 2026-10-12).toml` → `settings.toml`), if its name has one of their forms: Dropbox and
/// Nextcloud (`(… conflicted copy …)`, in Turkish `çakışan kopya`), Google Drive and Windows
/// (`(1)`), Syncthing (`.sync-conflict-…`), ownCloud (`_conflict-…`), iCloud (`settings 2`), and
/// OneDrive (`settings-<COMPUTER>`, only for the computers in `machines`). The caller checks
/// that the original is there.
pub fn conflict_of(name: &str, machines: &[String]) -> Option<String> {
    let (stem, ext) = name.rsplit_once('.')?;
    let original = |base: &str| (!base.is_empty() && !base.starts_with('.')).then(|| format!("{base}.{ext}"));
    if let Some((base, rest)) = stem.split_once(".sync-conflict-") {
        return original(base).filter(|_| rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
    }
    if let Some((base, rest)) = stem.rsplit_once("_conflict-") {
        return original(base).filter(|_| rest.chars().all(|c| c.is_ascii_digit() || c == '-'));
    }
    if let Some((base, inside)) = stem.strip_suffix(')').and_then(|s| s.rsplit_once(" (")) {
        let lower = inside.to_lowercase();
        let numbered = !inside.is_empty() && inside.chars().all(|c| c.is_ascii_digit());
        let copy = lower.contains("conflict") || lower.contains("çakış");
        return original(base).filter(|_| numbered || copy);
    }
    if let Some((base, n)) = stem.rsplit_once(' ') {
        return original(base).filter(|_| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
    }
    // OneDrive: `-<COMPUTER>`, maybe `-<COMPUTER>-2`.
    let unnumbered = match stem.rsplit_once('-') {
        Some((rest, n)) if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) => rest,
        _ => stem,
    };
    machines.iter().find_map(|machine| {
        let suffix = format!("-{machine}");
        [stem, unnumbered].into_iter().find_map(|s| strip_suffix_ignoring_case(s, &suffix)).and_then(original)
    })
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
        let settings = Some("settings.toml".to_owned());
        // Dropbox, English and Turkish.
        assert_eq!(of("settings (Ann's conflicted copy 2026-10-12).toml"), settings);
        assert_eq!(of("settings (Teoman adlı kişinin çakışan kopyası 2026-10-12).toml"), settings);
        // Nextcloud, ownCloud.
        assert_eq!(of("settings (conflicted copy 2026-10-12 101010).toml"), settings);
        assert_eq!(of("settings_conflict-20261012-101010.toml"), settings);
        // Google Drive and Windows; iCloud.
        assert_eq!(of("settings (1).toml"), settings);
        assert_eq!(of("settings 2.toml"), settings);
        // Syncthing.
        assert_eq!(of("settings.sync-conflict-20261012-101010-ABCDEFG.toml"), settings);
        // OneDrive: this computer's and the known ones', any case, numbered too.
        assert_eq!(of("settings-DESKTOP-AB12.toml"), settings);
        assert_eq!(of("settings-desktop-ab12-2.toml"), settings);
        assert_eq!(of("settings-TEOMAN-MASAÜSTÜ.toml"), settings);
        // A theme's.
        assert_eq!(of("nord (1).toml"), Some("nord.toml".to_owned()));
        assert_eq!(of("my theme.sync-conflict-20261012-1-X.toml"), Some("my theme.toml".to_owned()));
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
