//! Settings export and import (spec 14 §7): which zip entries may come in, what an import
//! changes in settings.toml, which settings run programs, and which backups are kept. Pure:
//! gezik's `settings_backup.rs` reads and writes the zip and the files.

use toml::{Table, Value};

/// The export's own file: `format`, the Gezik version, the OS and the date it was made on.
pub const MANIFEST: &str = "gezik-export.toml";
/// The format this Gezik writes and reads.
pub const FORMAT: i64 = 1;
/// The largest zip read, unpacked bytes in all too.
pub const MAX_ZIP: u64 = 300 << 20;
/// The most files a zip may have.
pub const MAX_ENTRIES: usize = 4096;
/// The largest file in it.
pub const MAX_FILE: u64 = 64 << 20;
/// The largest templates folder exported.
pub const MAX_TEMPLATES: u64 = 256 << 20;
/// The backups kept in `backups/`.
pub const KEEP_BACKUPS: usize = 10;
pub const BACKUP_PREFIX: &str = "gezik-settings-";

/// What a file in an export is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Settings,
    Themes,
    Templates,
    Views,
}

impl Group {
    pub const ALL: [Group; 4] = [Group::Settings, Group::Themes, Group::Templates, Group::Views];

    pub fn title(self) -> &'static str {
        match self {
            Group::Settings => "settings.toml",
            Group::Themes => "Themes",
            Group::Templates => "Templates",
            Group::Views => "Folder views",
        }
    }
}

/// The group of zip entry `name`, or `None` when it may not come in: only `settings.toml`,
/// `views.toml`, `themes/<name>.toml` and `templates/…` do, with no `..`, no absolute path, no
/// drive, no backslash and no name Windows cannot hold.
pub fn group_of(name: &str) -> Option<Group> {
    match name {
        "settings.toml" => Some(Group::Settings),
        "views.toml" => Some(Group::Views),
        _ => {
            if let Some(theme) = name.strip_prefix("themes/") {
                (safe_segment(theme) && theme.ends_with(".toml") && !theme.starts_with('.')).then_some(Group::Themes)
            } else {
                let template = name.strip_prefix("templates/")?;
                template.split('/').all(safe_segment).then_some(Group::Templates)
            }
        }
    }
}

/// A file or folder name that is the same on every system and leads nowhere else.
fn safe_segment(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or_default().to_ascii_uppercase();
    let numbered =
        stem.len() == 4 && (stem.starts_with("COM") || stem.starts_with("LPT")) && stem.as_bytes()[3].is_ascii_digit();
    !name.is_empty()
        && name.len() <= 255
        && !name.ends_with(['.', ' '])
        && !name.starts_with(' ')
        && !name.chars().any(|c| c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        && !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        && !numbered
}

/// The manifest of an export made now.
pub fn manifest(version: &str, os: &str, date: &str) -> String {
    format!("format = {FORMAT}\ngezik = \"{version}\"\nos = \"{os}\"\ndate = \"{date}\"\n")
}

/// The OS a manifest says the export was made on, or why the zip is not one this Gezik reads.
pub fn read_manifest(text: Option<&str>) -> Result<String, String> {
    let text = text.ok_or("Not a Gezik settings export (it has no gezik-export.toml)")?;
    let table: Table = toml::from_str(text).map_err(|_| "Its gezik-export.toml does not read")?;
    match table.get("format").and_then(Value::as_integer) {
        Some(FORMAT) => Ok(table.get("os").and_then(Value::as_str).unwrap_or_default().to_owned()),
        Some(_) => Err("Made by a newer Gezik: update Gezik to import it".to_owned()),
        None => Err("Its gezik-export.toml has no format".to_owned()),
    }
}

/// The settings in `imported` that run programs on this computer (sync profiles: write and
/// delete where they say), one line each, when they are not the same in `current` (spec 14
/// §7.2, decision 10).
pub fn programs(current: &str, imported: &str) -> Vec<String> {
    let before = program_settings(current);
    program_settings(imported).into_iter().filter(|line| !before.contains(line)).collect()
}

fn program_settings(text: &str) -> Vec<String> {
    let Ok(table) = toml::from_str::<Table>(text) else { return Vec::new() };
    let value = |table_name: &str, key: &str| table.get(table_name).and_then(|t| t.get(key)).filter(|v| !blank(v));
    let mut lines = Vec::new();
    if let Some(command) = value("terminal", "command") {
        lines.push(format!("Terminal: {}", shown(command)));
    }
    if let Some(diff) = value("tools", "diff") {
        let args = value("tools", "diff-args").map(|a| format!(" {}", shown(a))).unwrap_or_default();
        lines.push(format!("Compare program: {}{args}", shown(diff)));
    }
    for (table_name, key, title) in [("tools", "seven-zip", "7-Zip"), ("convert", "ffmpeg", "ffmpeg")] {
        if let Some(path) = value(table_name, key) {
            lines.push(format!("{title}: {}", shown(path)));
        }
    }
    let each = |key: &str| -> Vec<Table> {
        let tables = table.get(key).and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
        tables.iter().filter_map(Value::as_table).cloned().collect()
    };
    let text_of = |t: &Table, key: &str| t.get(key).map(shown).unwrap_or_default();
    for command in each("commands") {
        lines.push(format!("Command \"{}\": {}", text_of(&command, "name"), text_of(&command, "run")));
    }
    for profile in each("sync-profiles") {
        lines.push(format!(
            "Sync profile \"{}\": {} → {} ({})",
            text_of(&profile, "name"),
            text_of(&profile, "left"),
            text_of(&profile, "right"),
            text_of(&profile, "mode")
        ));
    }
    lines
}

fn blank(value: &Value) -> bool {
    match value {
        Value::String(s) => s.trim().is_empty(),
        Value::Array(a) => a.is_empty(),
        _ => false,
    }
}

/// A value as a line shows it: text without quotes, a list of texts joined by spaces.
pub(crate) fn shown(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Array(items) if items.iter().all(Value::is_str) => {
            items.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" ")
        }
        other => other.to_string(),
    }
}

/// The settings `imported` changes in `current`, one `key: old → new` each (a list, as
/// `[[commands]]`, is one value); "(default)" stands for a key that is not in the file.
pub fn changed_keys(current: &str, imported: &str) -> Vec<String> {
    let (mut before, mut after) = (Vec::new(), Vec::new());
    flatten("", &toml::from_str(current).unwrap_or_default(), &mut before);
    flatten("", &toml::from_str(imported).unwrap_or_default(), &mut after);
    let old = |key: &str| before.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());
    let mut lines: Vec<String> = after
        .iter()
        .filter(|(key, value)| old(key) != Some(value.as_str()))
        .map(|(key, value)| format!("{key}: {} → {}", short(old(key).unwrap_or("(default)")), short(value)))
        .collect();
    lines.extend(
        before
            .iter()
            .filter(|(key, _)| !after.iter().any(|(k, _)| k == key))
            .map(|(key, value)| format!("{key}: {} → (default)", short(value))),
    );
    lines
}

fn flatten(prefix: &str, table: &Table, out: &mut Vec<(String, String)>) {
    for (key, value) in table {
        let path = if prefix.is_empty() { key.clone() } else { format!("{prefix}.{key}") };
        match value {
            Value::Table(inner) => flatten(&path, inner, out),
            value => out.push((path, shown(value))),
        }
    }
}

pub(crate) fn short(text: &str) -> String {
    match text.char_indices().nth(40) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text.to_owned(),
    }
}

/// `changed_keys` in a sentence: "3 settings change: theme: dark → nord, …".
pub fn summary(changes: &[String]) -> String {
    const SHOWN: usize = 4;
    let more = match changes.len().saturating_sub(SHOWN) {
        0 => String::new(),
        n => format!(" and {n} more"),
    };
    match changes.len() {
        0 => "No setting changes".to_owned(),
        1 => format!("1 setting changes: {}", changes[0]),
        n => format!("{n} settings change: {}{more}", changes[..n.min(SHOWN)].join(", ")),
    }
}

/// The backups among `names` (files of `backups/`), newest first (their names hold the time).
pub fn newest_first(names: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut backups: Vec<String> =
        names.into_iter().filter(|n| n.starts_with(BACKUP_PREFIX) && n.ends_with(".zip")).collect();
    backups.sort_unstable_by(|a, b| b.cmp(a));
    backups
}

/// The backups to throw away so that `KEEP_BACKUPS` are left: the oldest.
pub fn backups_to_drop(names: impl IntoIterator<Item = String>) -> Vec<String> {
    newest_first(names).into_iter().skip(KEEP_BACKUPS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_allowed_names_come_in() {
        for (name, group) in [
            ("settings.toml", Some(Group::Settings)),
            ("views.toml", Some(Group::Views)),
            ("themes/nord.toml", Some(Group::Themes)),
            ("templates/Letter.docx", Some(Group::Templates)),
            ("templates/work/Report.xlsx", Some(Group::Templates)),
            ("state.toml", None),
            ("system-changes.toml", None),
            ("tags-store.tsv", None),
            ("backups/gezik-settings-1.zip", None),
            ("themes/nord.txt", None),
            ("themes/sub/nord.toml", None),
            ("themes/.nord.toml", None),
            ("templates/../../evil.exe", None),
            ("templates/a/../b", None),
            ("/etc/passwd", None),
            ("templates//x", None),
            ("templates/", None),
            ("C:/Windows/x", None),
            ("templates/C:x", None),
            ("templates\\..\\x", None),
            ("templates/con.txt", None),
            ("templates/COM1", None),
            ("templates/x. ", None),
            ("templates/a\u{0}b", None),
            ("../settings.toml", None),
        ] {
            assert_eq!(group_of(name), group, "{name}");
        }
        assert_eq!(group_of("templates/Comet.txt"), Some(Group::Templates), "only COM and a digit is reserved");
    }

    #[test]
    fn the_manifest_says_the_format() {
        let made = manifest("0.14.0", "windows", "2026-10-12");
        assert_eq!(read_manifest(Some(&made)), Ok("windows".to_owned()));
        assert!(read_manifest(None).unwrap_err().starts_with("Not a Gezik settings export"));
        assert!(read_manifest(Some("format = 2\n")).unwrap_err().contains("newer Gezik"));
        assert!(read_manifest(Some("format = [\n")).is_err());
    }

    #[test]
    fn settings_that_run_programs_are_listed_when_new_or_changed() {
        let current = "[terminal]\ncommand = [\"wt\", \"-d\", \"{dir}\"]\n\n[[commands]]\nname = \"Zip\"\nrun = [\"7z\", \"a\", \"{files}\"]\n";
        let imported = "theme = \"nord\"\n[terminal]\ncommand = [\"wt\", \"-d\", \"{dir}\"]\n[tools]\ndiff = \"meld\"\nseven-zip = \"\"\n[convert]\nffmpeg = \"C:/x/ffmpeg.exe\"\n\n[[commands]]\nname = \"Zip\"\nrun = [\"7z\", \"a\", \"{files}\"]\n\n[[commands]]\nname = \"Wipe\"\nrun = [\"cmd\", \"/c\", \"del\", \"{in}\"]\n\n[[sync-profiles]]\nname = \"Out\"\nleft = \"{home}\"\nright = \"E:/\"\nmode = \"mirror\"\n";
        assert_eq!(
            programs(current, imported),
            [
                "Compare program: meld",
                "ffmpeg: C:/x/ffmpeg.exe",
                "Command \"Wipe\": cmd /c del {in}",
                "Sync profile \"Out\": {home} → E:/ (mirror)",
            ],
            "the same terminal and Zip command are not listed; an empty 7-Zip path runs nothing"
        );
        let changed = imported.replace("\"-d\"", "\"-x\"");
        assert_eq!(programs(imported, &changed), ["Terminal: wt -x {dir}"]);
        assert!(programs(imported, imported).is_empty());
        assert!(programs("", "[terminal\n").is_empty(), "a broken file is refused before this");
        let args = "[tools]\ndiff = \"meld\"\ndiff-args = [\"--a\", \"{left}\"]\n";
        assert_eq!(programs("[tools]\ndiff = \"meld\"\n", args), ["Compare program: meld --a {left}"]);
    }

    #[test]
    fn the_changed_keys_are_summed_up() {
        let current = "theme = \"dark\"\nmax-fps = 120\n[layout]\ndensity = \"compact\"\n";
        let imported = "theme = \"nord\"\nmax-fps = 120\n[layout]\nsidebar = \"right\"\n";
        let changes = changed_keys(current, imported);
        assert_eq!(
            changes,
            ["theme: dark → nord", "layout.sidebar: (default) → right", "layout.density: compact → (default)"]
        );
        assert_eq!(summary(&changes), format!("3 settings change: {}", changes.join(", ")));
        assert_eq!(summary(&[]), "No setting changes");
        let many: Vec<String> = (0..6).map(|i| format!("k{i}")).collect();
        assert_eq!(summary(&many), "6 settings change: k0, k1, k2, k3 and 2 more");
        let long = format!("start-folder = \"{}\"\n", "x".repeat(60));
        assert!(changed_keys("", &long)[0].ends_with("…"));
    }

    #[test]
    fn the_ten_newest_backups_are_kept() {
        let names: Vec<String> = (1..=12)
            .map(|i| format!("{BACKUP_PREFIX}2026-10-{i:02}-120000-000.zip"))
            .chain(["notes.txt".to_owned(), "other.zip".to_owned()])
            .collect();
        assert_eq!(newest_first(names.clone())[0], format!("{BACKUP_PREFIX}2026-10-12-120000-000.zip"));
        assert_eq!(
            backups_to_drop(names),
            [format!("{BACKUP_PREFIX}2026-10-02-120000-000.zip"), format!("{BACKUP_PREFIX}2026-10-01-120000-000.zip")],
            "other files stay"
        );
        assert!(backups_to_drop(vec![format!("{BACKUP_PREFIX}1.zip")]).is_empty());
    }
}
