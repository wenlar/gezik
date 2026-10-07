//! Pinned folders (`pinned` in settings.toml, spec 6): each a path, or an inline table
//! `{ path, name, group }` for an alias and a group. Reading and writing them, and the changes
//! the sidebar makes to the list. Pure: the paths are text with `{home}`-style tokens.

/// One pinned folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinEntry {
    /// With `{home}`-style tokens and `/` separators.
    pub path: String,
    /// The alias the sidebar shows instead of the folder's name.
    pub name: Option<String>,
    pub group: Option<String>,
}

impl PinEntry {
    /// A pin without alias and group (written as plain text).
    pub fn plain(path: impl Into<String>) -> PinEntry {
        PinEntry { path: path.into(), name: None, group: None }
    }
}

/// Text that is empty once trimmed is none.
fn trimmed(text: Option<&str>) -> Option<String> {
    text.map(str::trim).filter(|t| !t.is_empty()).map(str::to_owned)
}

/// One item of `pinned`; the error is why it is left out (settings.toml warns, and Gezik
/// keeps it in the file as it is).
pub(crate) fn parse_pin(value: &toml::Value) -> Result<PinEntry, String> {
    let pin = match value {
        toml::Value::String(path) => PinEntry::plain(path.as_str()),
        toml::Value::Table(table) => {
            if let Some(key) = table.keys().find(|k| !matches!(k.as_str(), "path" | "name" | "group")) {
                return Err(format!("unknown key \"{key}\" in {value}"));
            }
            let text = |key: &str| match table.get(key) {
                None => Ok(None),
                Some(toml::Value::String(text)) => Ok(Some(text.as_str())),
                Some(other) => Err(format!("{key} must be text, got {other}")),
            };
            let path = text("path")?.unwrap_or_default().to_owned();
            PinEntry { path, name: trimmed(text("name")?), group: trimmed(text("group")?) }
        }
        other => return Err(format!("expected text or {{ path, name, group }}, got {other}")),
    };
    if pin.path.trim().is_empty() {
        return Err(format!("path is missing in {value}"));
    }
    if crate::paths::has_parent_segment(&pin.path) {
        return Err(format!("\"{}\" must not contain \"..\"", pin.path));
    }
    Ok(pin)
}

/// `pin` as settings.toml keeps it: plain text without alias and group, else an inline table
/// (`path`, `name`, `group`; the ones it has).
pub(crate) fn pin_to_value(pin: &PinEntry) -> toml_edit::Value {
    if pin.name.is_none() && pin.group.is_none() {
        return toml_edit::Value::from(pin.path.as_str());
    }
    let mut table = toml_edit::InlineTable::new();
    table.insert("path", pin.path.as_str().into());
    if let Some(name) = &pin.name {
        table.insert("name", name.as_str().into());
    }
    if let Some(group) = &pin.group {
        table.insert("group", group.as_str().into());
    }
    table.fmt();
    toml_edit::Value::InlineTable(table)
}

/// Whether two pinned paths are one folder's: ignoring case (also of non-ASCII letters like
/// Ç) on Windows, exactly elsewhere.
pub fn same_path_text(a: &str, b: &str) -> bool {
    a == b || (cfg!(windows) && a.to_lowercase() == b.to_lowercase())
}

/// Whether two group names are one group: ignoring case.
pub fn same_group(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

/// Where `path` is pinned in `list`.
pub fn find(list: &[PinEntry], path: &str) -> Option<usize> {
    list.iter().position(|pin| same_path_text(&pin.path, path))
}

/// The paths of `list`, in order.
pub fn paths(list: &[PinEntry]) -> Vec<&str> {
    list.iter().map(|pin| pin.path.as_str()).collect()
}

/// `list` in the order the sidebar shows it (spec 6.2): the pins without a group first, then
/// each group in the order of its first pin, its pins in their order. Aliases and groups are
/// trimmed (empty: none); a group's pins take the spelling of its first one.
pub fn normalize(list: Vec<PinEntry>) -> Vec<PinEntry> {
    let mut ungrouped = Vec::new();
    let mut names: Vec<String> = Vec::new();
    let mut members: Vec<Vec<PinEntry>> = Vec::new();
    for mut pin in list {
        pin.name = trimmed(pin.name.as_deref());
        pin.group = trimmed(pin.group.as_deref());
        let Some(group) = pin.group.clone() else {
            ungrouped.push(pin);
            continue;
        };
        let i = match names.iter().position(|name| same_group(name, &group)) {
            Some(i) => i,
            None => {
                names.push(group);
                members.push(Vec::new());
                names.len() - 1
            }
        };
        pin.group = Some(names[i].clone());
        members[i].push(pin);
    }
    ungrouped.into_iter().chain(members.into_iter().flatten()).collect()
}

/// The groups of `list`, in the order of their first pins.
pub fn groups(list: &[PinEntry]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for group in list.iter().filter_map(|pin| pin.group.as_deref()) {
        if !out.iter().any(|name| same_group(name, group)) {
            out.push(group.to_owned());
        }
    }
    out
}

fn tidy(list: &mut Vec<PinEntry>) {
    *list = normalize(std::mem::take(list));
}

fn in_group(pin: &PinEntry, group: &str) -> bool {
    pin.group.as_deref().is_some_and(|own| same_group(own, group))
}

/// Pins `path` (no alias, no group) unless it is pinned already. Returns whether it was added.
pub fn pin(list: &mut Vec<PinEntry>, path: String) -> bool {
    if find(list, &path).is_some() {
        return false;
    }
    list.push(PinEntry::plain(path));
    tidy(list);
    true
}

pub fn unpin(list: &mut Vec<PinEntry>, index: usize) -> bool {
    if index >= list.len() {
        return false;
    }
    list.remove(index);
    true
}

/// Puts the folders `paths` next to pin `anchor` (before it, or after it with `after`), in its
/// group: one already pinned moves there with its alias, a new one is added. Returns whether
/// the list changed (a pin dropped on itself changes nothing).
pub fn place(list: &mut Vec<PinEntry>, paths: Vec<String>, anchor: usize, after: bool) -> bool {
    let Some(target) = list.get(anchor).cloned() else { return false };
    if paths.iter().any(|path| same_path_text(path, &target.path)) {
        return false;
    }
    let before = list.clone();
    let mut moving: Vec<PinEntry> = Vec::new();
    for path in paths {
        let pin = match find(list, &path) {
            Some(i) => list.remove(i),
            None => PinEntry::plain(path),
        };
        if !moving.iter().any(|m| same_path_text(&m.path, &pin.path)) {
            moving.push(PinEntry { group: target.group.clone(), ..pin });
        }
    }
    let Some(at) = find(list, &target.path) else { return false };
    let at = if after { at + 1 } else { at };
    list.splice(at..at, moving);
    tidy(list);
    *list != before
}

/// Moves pin `index` into `group` (none: no group), at the group's end; a new group goes last.
pub fn set_group(list: &mut Vec<PinEntry>, index: usize, group: Option<&str>) -> bool {
    let group = trimmed(group);
    let Some(pin) = list.get(index) else { return false };
    let same = match (&pin.group, &group) {
        (None, None) => true,
        (Some(own), Some(new)) => same_group(own, new),
        _ => false,
    };
    if same {
        return false;
    }
    let mut pin = list.remove(index);
    pin.group = group;
    list.push(pin);
    tidy(list);
    true
}

/// Gives pin `index` the alias `name` (none or empty: the folder's own name again).
pub fn set_name(list: &mut [PinEntry], index: usize, name: Option<&str>) -> bool {
    let name = trimmed(name);
    match list.get_mut(index) {
        Some(pin) if pin.name != name => {
            pin.name = name;
            true
        }
        _ => false,
    }
}

/// Swaps `group`'s pins with the group before it (`up`) or after it.
pub fn move_group(list: &mut Vec<PinEntry>, group: &str, up: bool) -> bool {
    let mut order = groups(list);
    let Some(i) = order.iter().position(|name| same_group(name, group)) else { return false };
    let Some(j) = (if up { i.checked_sub(1) } else { Some(i + 1) }).filter(|j| *j < order.len()) else {
        return false;
    };
    order.swap(i, j);
    let all = std::mem::take(list);
    let mut out: Vec<PinEntry> = all.iter().filter(|pin| pin.group.is_none()).cloned().collect();
    for name in &order {
        out.extend(all.iter().filter(|pin| in_group(pin, name)).cloned());
    }
    *list = out;
    true
}

/// Renames group `from` to `to` (trimmed, not empty); a name another group has joins the two.
pub fn rename_group(list: &mut Vec<PinEntry>, from: &str, to: &str) -> bool {
    let to = to.trim();
    if to.is_empty() {
        return false;
    }
    let mut changed = false;
    for pin in list.iter_mut().filter(|pin| in_group(pin, from)) {
        if pin.group.as_deref() != Some(to) {
            pin.group = Some(to.to_owned());
            changed = true;
        }
    }
    if changed {
        tidy(list);
    }
    changed
}

/// Takes `group`'s pins out of it: they go after the pins without a group.
pub fn ungroup(list: &mut Vec<PinEntry>, group: &str) -> bool {
    let mut changed = false;
    for pin in list.iter_mut().filter(|pin| in_group(pin, group)) {
        pin.group = None;
        changed = true;
    }
    if changed {
        tidy(list);
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, group: Option<&str>) -> PinEntry {
        PinEntry { path: path.into(), name: None, group: group.map(str::to_owned) }
    }

    fn plain(list: &[&str]) -> Vec<PinEntry> {
        list.iter().map(|path| PinEntry::plain(*path)).collect()
    }

    /// `Group:/path`, or `/path` without a group.
    fn shown(list: &[PinEntry]) -> Vec<String> {
        list.iter()
            .map(|pin| match &pin.group {
                Some(group) => format!("{group}:{}", pin.path),
                None => pin.path.clone(),
            })
            .collect()
    }

    #[test]
    fn normalize_puts_groups_together_and_trims() {
        let list = vec![
            entry("/w1", Some("Work")),
            entry("/a", None),
            entry("/m", Some("Media")),
            entry("/w2", Some(" work ")),
            PinEntry { path: "/b".into(), name: Some("  ".into()), group: Some(String::new()) },
        ];
        let tidy = normalize(list);
        assert_eq!(shown(&tidy), ["/a", "/b", "Work:/w1", "Work:/w2", "Media:/m"]);
        assert_eq!(tidy[1], PinEntry::plain("/b"), "an empty alias and group are none");
        assert_eq!(groups(&tidy), ["Work", "Media"]);
        assert_eq!(normalize(tidy.clone()), tidy, "a second time changes nothing");
    }

    #[test]
    fn place_moves_and_groups() {
        let mut list = plain(&["/a", "/b", "/c"]);
        assert!(place(&mut list, vec!["/x".into(), "/y".into()], 1, false));
        assert_eq!(paths(&list), ["/a", "/x", "/y", "/b", "/c"]);
        assert!(place(&mut list, vec!["/c".into()], 0, false));
        assert_eq!(paths(&list), ["/c", "/a", "/x", "/y", "/b"], "a pinned one moves");
        assert!(place(&mut list, vec!["/a".into()], 4, true));
        assert_eq!(paths(&list), ["/c", "/x", "/y", "/b", "/a"], "after the anchor, the gap counted");
        assert!(!place(&mut list, vec!["/b".into()], 3, false), "onto itself: nothing");
        assert!(!place(&mut list, vec!["/z".into()], 99, false), "no such anchor");

        let mut list = vec![
            PinEntry { path: "/a".into(), name: Some("Alias".into()), group: None },
            entry("/w1", Some("Work")),
            entry("/w2", Some("Work")),
            entry("/gone", Some("Media")),
            entry("/m", Some("Media")),
        ];
        assert!(place(&mut list, vec!["/a".into()], 2, false));
        assert_eq!(shown(&list), ["Work:/w1", "Work:/a", "Work:/w2", "Media:/gone", "Media:/m"]);
        assert_eq!(list[1].name.as_deref(), Some("Alias"), "the alias goes along");
        assert!(place(&mut list, vec!["/new".into()], 4, true));
        assert_eq!(shown(&list).last().map(String::as_str), Some("Media:/new"));
    }

    #[test]
    fn groups_move_rename_and_dissolve() {
        let mut list =
            vec![entry("/a", None), entry("/w", Some("Work")), entry("/m", Some("Media")), entry("/p", Some("Photos"))];
        assert!(move_group(&mut list, "media", true));
        assert_eq!(groups(&list), ["Media", "Work", "Photos"]);
        assert!(!move_group(&mut list, "Media", true), "the first goes no higher");
        assert!(!move_group(&mut list, "Photos", false), "the last no lower");
        assert!(rename_group(&mut list, "Work", "Jobs"));
        assert_eq!(groups(&list), ["Media", "Jobs", "Photos"]);
        assert!(!rename_group(&mut list, "Jobs", "  "), "no empty name");
        assert!(rename_group(&mut list, "Photos", "media"), "a name taken joins that group");
        assert_eq!(shown(&list), ["/a", "Media:/m", "Media:/p", "Jobs:/w"]);
        assert!(ungroup(&mut list, "MEDIA"));
        assert_eq!(shown(&list), ["/a", "/m", "/p", "Jobs:/w"]);
        assert!(!ungroup(&mut list, "Nope"));
    }

    #[test]
    fn a_pin_changes_group_and_alias() {
        let mut list = vec![entry("/a", None), entry("/b", None), entry("/w", Some("Work"))];
        assert!(set_group(&mut list, 0, Some("work")));
        assert_eq!(shown(&list), ["/b", "Work:/w", "Work:/a"], "at the group's end, spelled as it is");
        assert!(set_group(&mut list, 0, Some("New")));
        assert_eq!(shown(&list), ["Work:/w", "Work:/a", "New:/b"], "a new group goes last");
        assert!(!set_group(&mut list, 0, Some("WORK")), "its own group");
        assert!(set_group(&mut list, 2, None));
        assert_eq!(shown(&list), ["/b", "Work:/w", "Work:/a"]);
        assert!(set_name(&mut list, 0, Some(" Bee ")));
        assert_eq!(list[0].name.as_deref(), Some("Bee"));
        assert!(set_name(&mut list, 0, Some("")) && list[0].name.is_none());
        assert!(!set_name(&mut list, 0, None));
    }

    #[test]
    fn pinning_twice_is_once() {
        let mut list = plain(&["/a"]);
        assert!(!pin(&mut list, "/a".into()));
        assert!(pin(&mut list, "/b".into()));
        assert_eq!(paths(&list), ["/a", "/b"]);
        let mut grouped = vec![entry("/w", Some("Work"))];
        assert!(pin(&mut grouped, "/n".into()));
        assert_eq!(paths(&grouped), ["/n", "/w"], "a new pin has no group: before the groups");
        assert!(unpin(&mut grouped, 1) && !unpin(&mut grouped, 5));
    }

    #[cfg(windows)]
    #[test]
    fn paths_match_ignoring_case_on_windows() {
        let mut list = plain(&["C:/ÇALIŞMA"]);
        assert!(!pin(&mut list, "c:/çalişma".into()));
        assert_eq!(find(&list, "c:/ÇaLiŞma"), Some(0));
    }
}
