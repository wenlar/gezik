//! A round's merge (spec 17 §5.2): the base (what both sides had after the last round), this
//! computer's files and the cloud's, made into one set, and what that means for each side.

use std::collections::BTreeMap;

use crate::sync_folder::three_way;

/// Files by their path in the shared folder (`themes/nord.toml`).
pub type Files = BTreeMap<String, Vec<u8>>;

/// The one file merged key by key.
pub const SETTINGS: &str = "settings.toml";

/// What a round does.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Plan {
    /// The set both sides hold after the round (and the next round's base).
    pub merged: Files,
    /// Files to write here, new or changed (the conflict copies among them).
    pub write: Files,
    /// Files to remove here.
    pub delete: Vec<String>,
    /// The conflict copies made, each the cloud's version of a file both sides changed.
    pub conflicts: Vec<String>,
    /// Whether the cloud's copy differs from `merged` and must be sent.
    pub push: bool,
}

/// The merge of `local` and `remote` since `base`. settings.toml goes key by key ([`three_way`]):
/// a key both changed differently keeps this computer's value, and the cloud's whole file is
/// kept beside it as a conflict copy. Any other file goes by its bytes: changed on one side
/// only, that side's; changed differently on both, ours, with the cloud's as a conflict copy;
/// removed on one side and untouched on the other, removed; removed on one side and changed
/// on the other, kept. `date` (`2026-10-15`) names the conflict copies.
pub fn plan(base: &Files, local: &Files, remote: &Files, date: &str) -> Plan {
    let mut merged = Files::new();
    let mut copies: Vec<(String, Vec<u8>)> = Vec::new();
    let paths: std::collections::BTreeSet<&String> = base.keys().chain(local.keys()).chain(remote.keys()).collect();
    for path in paths {
        let (b, l, r) = (base.get(path), local.get(path), remote.get(path));
        let kept = if l == r || r == b {
            l
        } else if l == b {
            r
        } else {
            match (l, r) {
                (Some(ours), Some(theirs)) => {
                    let (text, conflict) = match settings_merge(path, b, ours, theirs) {
                        Some((text, conflict)) => (Some(text), conflict),
                        None => (None, true),
                    };
                    if conflict {
                        copies.push((path.clone(), theirs.clone()));
                    }
                    if let Some(text) = text {
                        merged.insert(path.clone(), text);
                        continue;
                    }
                    l
                }
                // Removed on one side, changed on the other: kept.
                _ => l.or(r),
            }
        };
        if let Some(bytes) = kept {
            merged.insert(path.clone(), bytes.clone());
        }
    }
    let mut conflicts = Vec::new();
    for (path, bytes) in copies {
        let name =
            (1..).map(|n| conflict_name(&path, date, n)).find(|name| !merged.contains_key(name)).unwrap_or_default();
        merged.insert(name.clone(), bytes);
        conflicts.push(name);
    }
    let write =
        merged.iter().filter(|(path, bytes)| local.get(*path) != Some(bytes)).map(|(p, b)| (p.clone(), b.clone()));
    Plan {
        write: write.collect(),
        delete: local.keys().filter(|path| !merged.contains_key(*path)).cloned().collect(),
        push: &merged != remote,
        merged,
        conflicts,
    }
}

/// settings.toml both sides changed: the cloud's changes made on ours, and whether a key both
/// changed is left as ours has it. `None` for another file, or a settings.toml that does not
/// read (then it is a conflict as a whole).
fn settings_merge(path: &str, base: Option<&Vec<u8>>, ours: &[u8], theirs: &[u8]) -> Option<(Vec<u8>, bool)> {
    if path != SETTINGS {
        return None;
    }
    let text = |bytes: &[u8]| std::str::from_utf8(bytes).ok().map(str::to_owned);
    let base = base.map_or(Some(String::new()), |b| text(b))?;
    let merged = three_way(&base, &text(theirs)?, &text(ours)?).ok()?;
    Some((merged.text.into_bytes(), !merged.conflicts.is_empty()))
}

/// The name of the cloud's conflicting copy of `path` made on `date`, the `n`th that day:
/// `themes/nord.cloud-conflict-2026-10-15.toml`, then `…-2026-10-15-2.toml`.
pub fn conflict_name(path: &str, date: &str, n: u32) -> String {
    let (folder, name) = path.rsplit_once('/').map_or(("", path), |(folder, name)| (folder, name));
    let tag = if n > 1 { format!("cloud-conflict-{date}-{n}") } else { format!("cloud-conflict-{date}") };
    let name = match name.rsplit_once('.').filter(|(stem, _)| !stem.is_empty()) {
        Some((stem, ext)) => format!("{stem}.{tag}.{ext}"),
        None => format!("{name}.{tag}"),
    };
    if folder.is_empty() { name } else { format!("{folder}/{name}") }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync_folder::{Conflict, conflict_of};

    const DATE: &str = "2026-10-15";

    fn set(entries: &[(&str, &str)]) -> Files {
        entries.iter().map(|(path, text)| (path.to_string(), text.as_bytes().to_vec())).collect()
    }

    /// One file, `t.toml`, in each state on each side: what is kept, what this computer
    /// writes or removes, the conflict copy and whether the cloud gets a new copy.
    #[test]
    fn each_file_state_on_each_side() {
        const COPY: &str = "t.cloud-conflict-2026-10-15.toml";
        // (base, local, remote) → (merged, written here, removed here, conflict, push)
        type Sides = (Option<&'static str>, Option<&'static str>, Option<&'static str>);
        type Entries = &'static [(&'static str, &'static str)];
        type Case = (&'static str, Sides, Entries, Entries, &'static [&'static str], bool, bool);
        let cases: &[Case] = &[
            ("all the same", (Some("a"), Some("a"), Some("a")), &[("t.toml", "a")], &[], &[], false, false),
            ("changed here", (Some("a"), Some("b"), Some("a")), &[("t.toml", "b")], &[], &[], false, true),
            (
                "changed there",
                (Some("a"), Some("a"), Some("b")),
                &[("t.toml", "b")],
                &[("t.toml", "b")],
                &[],
                false,
                false,
            ),
            ("changed alike", (Some("a"), Some("b"), Some("b")), &[("t.toml", "b")], &[], &[], false, false),
            (
                "changed differently",
                (Some("a"), Some("b"), Some("c")),
                &[("t.toml", "b"), (COPY, "c")],
                &[(COPY, "c")],
                &[],
                true,
                true,
            ),
            ("added here", (None, Some("a"), None), &[("t.toml", "a")], &[], &[], false, true),
            ("added there", (None, None, Some("a")), &[("t.toml", "a")], &[("t.toml", "a")], &[], false, false),
            ("added alike", (None, Some("a"), Some("a")), &[("t.toml", "a")], &[], &[], false, false),
            (
                "added differently",
                (None, Some("a"), Some("b")),
                &[("t.toml", "a"), (COPY, "b")],
                &[(COPY, "b")],
                &[],
                true,
                true,
            ),
            ("removed here", (Some("a"), None, Some("a")), &[], &[], &[], false, true),
            ("removed there", (Some("a"), Some("a"), None), &[], &[], &["t.toml"], false, false),
            ("removed on both", (Some("a"), None, None), &[], &[], &[], false, false),
            (
                "removed here, changed there",
                (Some("a"), None, Some("b")),
                &[("t.toml", "b")],
                &[("t.toml", "b")],
                &[],
                false,
                false,
            ),
            ("changed here, removed there", (Some("a"), Some("b"), None), &[("t.toml", "b")], &[], &[], false, true),
        ];
        let one = |text: Option<&str>| text.map(|t| set(&[("t.toml", t)])).unwrap_or_default();
        for (name, (b, l, r), merged, write, delete, conflict, push) in cases {
            let plan = plan(&one(*b), &one(*l), &one(*r), DATE);
            assert_eq!(plan.merged, set(merged), "{name}");
            assert_eq!(plan.write, set(write), "{name}");
            assert_eq!(plan.delete, delete.to_vec(), "{name}");
            assert_eq!(plan.conflicts, if *conflict { vec![COPY.to_owned()] } else { vec![] }, "{name}");
            assert_eq!(plan.push, *push, "{name}");
        }
    }

    #[test]
    fn settings_merge_key_by_key() {
        let base = set(&[(SETTINGS, "theme = \"nord\"\nfont_size = 13\n")]);
        let local = set(&[(SETTINGS, "theme = \"dracula\"\nfont_size = 13\n")]);
        let remote = set(&[(SETTINGS, "theme = \"nord\"\nfont_size = 15\n")]);
        let plan = plan(&base, &local, &remote, DATE);
        let both = "theme = \"dracula\"\nfont_size = 15\n";
        assert_eq!(plan.merged, set(&[(SETTINGS, both)]));
        assert_eq!(plan.write, plan.merged);
        assert!(plan.conflicts.is_empty());
        assert!(plan.push);
    }

    #[test]
    fn a_settings_key_changed_on_both_keeps_ours_and_copies_theirs() {
        let base = set(&[(SETTINGS, "theme = \"nord\"\nfont_size = 13\n")]);
        let local = set(&[(SETTINGS, "theme = \"dracula\"\nfont_size = 13\n")]);
        let theirs = "theme = \"solarized\"\nfont_size = 15\n";
        let remote = set(&[(SETTINGS, theirs)]);
        let plan = plan(&base, &local, &remote, DATE);
        let copy = "settings.cloud-conflict-2026-10-15.toml";
        // Their other change still comes in.
        let ours = "theme = \"dracula\"\nfont_size = 15\n";
        assert_eq!(plan.merged, set(&[(SETTINGS, ours), (copy, theirs)]));
        assert_eq!(plan.write, plan.merged);
        assert_eq!(plan.conflicts, [copy]);
        assert!(plan.push);
        // A settings.toml that does not read is a conflict as a whole.
        let broken = set(&[(SETTINGS, "theme = [\n")]);
        let plan = super::plan(&base, &local, &broken, DATE);
        assert_eq!(plan.merged, set(&[(SETTINGS, "theme = \"dracula\"\nfont_size = 13\n"), (copy, "theme = [\n")]));
    }

    #[test]
    fn conflict_copies_take_a_free_name_and_keep_their_folder() {
        assert_eq!(conflict_name("themes/nord.toml", DATE, 1), "themes/nord.cloud-conflict-2026-10-15.toml");
        assert_eq!(conflict_name("templates/Makefile", DATE, 2), "templates/Makefile.cloud-conflict-2026-10-15-2");
        assert_eq!(conflict_name(".editorconfig", DATE, 1), ".editorconfig.cloud-conflict-2026-10-15");
        // A copy from earlier that day is not written over.
        let base = set(&[("t.toml", "a")]);
        let earlier = "t.cloud-conflict-2026-10-15.toml";
        let local = set(&[("t.toml", "b"), (earlier, "old")]);
        let remote = set(&[("t.toml", "c"), (earlier, "old")]);
        let plan = plan(&base, &local, &remote, DATE);
        assert_eq!(plan.conflicts, ["t.cloud-conflict-2026-10-15-2.toml"]);
        assert_eq!(plan.merged.get(earlier).map(Vec::as_slice), Some(b"old".as_slice()));
    }

    #[test]
    fn the_sync_folder_finds_the_cloud_conflict_copies() {
        let of = |name: &str| conflict_of(name, &[]);
        let named = |original: &str| Some(Conflict { original: original.to_owned(), numbered: false });
        for n in [1, 2] {
            assert_eq!(of(&conflict_name(SETTINGS, DATE, n)), named(SETTINGS));
            assert_eq!(of(&conflict_name("nord.toml", DATE, n)), named("nord.toml"));
        }
    }
}
