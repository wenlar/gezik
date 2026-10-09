//! What Gezik changed in the system (spec 11): a change with its value before and after, and
//! the rule that undoes it (spec 11.4): newest first, each only where the system still holds
//! what Gezik wrote. Pure: the journal file is gezik-config's `system_journal`; reading and
//! writing the system is behind [`Access`].

use std::io;

/// What a change touched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A text value under HKCU.
    RegistryValue,
    /// A key Gezik made: taken away again only if empty.
    RegistryKey,
    /// A folder Gezik added to a `;` list held in a registry value (the user's PATH).
    PathEntry,
    /// A small text file Gezik wrote.
    File,
    Symlink,
    /// A folder Gezik made: taken away again only if empty.
    Folder,
    /// macOS: the default app for a content type (`place` = `public.folder`), a bundle id.
    MacDefault,
    /// macOS: a key of the global preferences domain (`place` = `NSFileViewer`), a bundle id.
    MacPref,
    /// Linux: one key of a `mimeapps.list` (`place` = the file, `name` = the MIME type).
    Mimeapps,
}

impl Kind {
    pub const ALL: [Kind; 9] = [
        Kind::RegistryValue,
        Kind::RegistryKey,
        Kind::PathEntry,
        Kind::File,
        Kind::Symlink,
        Kind::Folder,
        Kind::MacDefault,
        Kind::MacPref,
        Kind::Mimeapps,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Kind::RegistryValue => "registry-value",
            Kind::RegistryKey => "registry-key",
            Kind::PathEntry => "path-entry",
            Kind::File => "file",
            Kind::Symlink => "symlink",
            Kind::Folder => "folder",
            Kind::MacDefault => "macos-default",
            Kind::MacPref => "macos-pref",
            Kind::Mimeapps => "mimeapps",
        }
    }

    pub fn from_name(name: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|kind| kind.name() == name)
    }
}

/// The registry text types Gezik reads and writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegType {
    Sz,
    ExpandSz,
}

impl RegType {
    pub fn name(self) -> &'static str {
        match self {
            RegType::Sz => "REG_SZ",
            RegType::ExpandSz => "REG_EXPAND_SZ",
        }
    }

    pub fn from_name(name: &str) -> Option<RegType> {
        [RegType::Sz, RegType::ExpandSz].into_iter().find(|ty| ty.name() == name)
    }
}

/// What is at a change's place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Absent,
    /// A key or a folder that is there.
    Present,
    Reg {
        ty: RegType,
        data: String,
    },
    /// A file's whole text (Gezik's files are a few lines).
    Text(String),
    /// A symbolic link's target.
    Link(String),
    /// Something there that this kind of change does not hold (a link where a folder is
    /// wanted, a large or binary file): never Gezik's.
    Other,
}

/// One change Gezik made (or was about to make: `done == false`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// What it is for: `path`, later `default-file-manager`, `start-at-login`, `app-bundle`.
    pub feature: String,
    pub kind: Kind,
    /// `HKCU\Environment`, a key, a file's or folder's path (`where` in the file).
    pub place: String,
    /// A registry value's name; "" is (Default).
    pub name: String,
    /// path-entry only: the folder Gezik added to the list.
    pub entry: String,
    pub before: Value,
    pub after: Value,
    pub done: bool,
}

/// Reads and writes the system for the undo; the app's knows the registry and the disk,
/// tests have a map.
pub trait Access {
    fn current(&self, change: &Change) -> io::Result<Value>;
    /// Puts `value` at the change's place. `Absent` deletes: a key or folder only if empty,
    /// else an error of kind `DirectoryNotEmpty`.
    fn set(&self, change: &Change, value: &Value) -> io::Result<()>;
}

/// What the undo of one change did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Undone,
    /// Already as before: never made, or taken back elsewhere.
    WasBefore,
    /// Only Gezik's entry came out of a list that changed since (the rest kept).
    EntryRemoved,
    /// Changed by something else since: left as is.
    LeftAsIs,
    /// A key or folder with something in it now: left.
    NotEmpty,
    Failed(String),
}

impl Outcome {
    /// Whether the journal can forget the change: done with, or not Gezik's any more.
    pub fn settled(&self) -> bool {
        !matches!(self, Outcome::Failed(_))
    }
}

/// Undoes `changes`, newest first (spec 11.4); `outcomes[i]` is `changes[i]`'s.
pub fn undo(changes: &[Change], access: &dyn Access) -> Vec<Outcome> {
    let mut outcomes = vec![Outcome::WasBefore; changes.len()];
    for (i, change) in changes.iter().enumerate().rev() {
        outcomes[i] = undo_one(change, access);
    }
    outcomes
}

fn undo_one(change: &Change, access: &dyn Access) -> Outcome {
    let now = match access.current(change) {
        Ok(now) => now,
        Err(err) => return Outcome::Failed(err.to_string()),
    };
    let put = |value: &Value, done: Outcome| match access.set(change, value) {
        Ok(()) => done,
        Err(err) if err.kind() == io::ErrorKind::DirectoryNotEmpty => Outcome::NotEmpty,
        // Something not Gezik's took the place meanwhile (a link Gezik will not replace).
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => Outcome::LeftAsIs,
        Err(err) => Outcome::Failed(err.to_string()),
    };
    if now == change.after {
        return put(&change.before, Outcome::Undone);
    }
    if now == change.before {
        return Outcome::WasBefore;
    }
    // The list changed since (decision 5): Gezik's own entry still is Gezik's.
    if change.kind == Kind::PathEntry {
        return match &now {
            Value::Reg { ty, data } => match without_entry(data, &change.entry) {
                Some(rest) => put(&Value::Reg { ty: *ty, data: rest }, Outcome::EntryRemoved),
                None => Outcome::WasBefore,
            },
            // The value is gone or not text since: not Gezik's to touch.
            _ => Outcome::LeftAsIs,
        };
    }
    Outcome::LeftAsIs
}

/// `entry` at the end of the `;` list, everything there kept as it is (no part trimmed,
/// moved or dropped); `None` when it is in it already, also as the user's `%VAR%` spelling
/// that `expand` opens.
pub fn with_entry(list: &str, entry: &str, expand: impl Fn(&str) -> String) -> Option<String> {
    if list.split(';').any(|part| same_entry(part, entry) || same_entry(&expand(part), entry)) {
        return None;
    }
    Some(if list.is_empty() || list.ends_with(';') { format!("{list}{entry}") } else { format!("{list};{entry}") })
}

/// The list without the parts spelled as `entry`, everything else as it was; `None` when
/// no part is.
pub fn without_entry(list: &str, entry: &str) -> Option<String> {
    if !contains_entry(list, entry) {
        return None;
    }
    Some(list.split(';').filter(|part| !same_entry(part, entry)).collect::<Vec<_>>().join(";"))
}

pub fn contains_entry(list: &str, entry: &str) -> bool {
    list.split(';').any(|part| same_entry(part, entry))
}

/// Windows compares folder names without case; a trailing `\` or a pair of quotes around the
/// part (PATH allows them) names the same folder.
fn same_entry(part: &str, entry: &str) -> bool {
    let plain = |text: &str| {
        let unquoted = text.strip_prefix('"').and_then(|t| t.strip_suffix('"')).unwrap_or(text);
        unquoted.trim_end_matches('\\').to_lowercase()
    };
    !part.is_empty() && plain(part) == plain(entry)
}

/// Where a change is: `HKCU\Environment\Path`, `…\gezik.exe\(Default)`, a path.
pub fn what(change: &Change) -> String {
    match change.kind {
        Kind::RegistryValue | Kind::PathEntry => {
            let name = if change.name.is_empty() { "(Default)" } else { &change.name };
            format!(r"{}\{name}", change.place)
        }
        Kind::Mimeapps => format!("{} [{}]", change.place, change.name),
        _ => change.place.clone(),
    }
}

/// A value for people: long text cut at 60 characters.
fn shown(value: &Value) -> String {
    let clip = |text: &str| {
        let mut out: String = text.chars().take(60).collect();
        if text.chars().count() > 60 {
            out.push('…');
        }
        format!("\"{out}\"")
    };
    match value {
        Value::Absent => "(none)".into(),
        Value::Present => "(there)".into(),
        Value::Reg { ty, data } => format!("{} {}", ty.name(), clip(data)),
        Value::Text(text) => format!("a file of {} lines", text.lines().count()),
        Value::Link(target) => format!("a link to {}", clip(target)),
        Value::Other => "(something else)".into(),
    }
}

/// A journal line for Show (spec 3.2: key, value, before, after).
pub fn describe(change: &Change) -> String {
    let unfinished = if change.done { "" } else { " (not finished)" };
    format!(
        "{}: {}: before {}, after {}{unfinished}",
        change.feature,
        what(change),
        shown(&change.before),
        shown(&change.after)
    )
}

/// A line of `--unregister`'s report.
pub fn report_line(change: &Change, outcome: &Outcome) -> String {
    let what = what(change);
    match outcome {
        Outcome::Undone => format!("undone: {what}"),
        Outcome::WasBefore => format!("already as it was: {what}"),
        Outcome::EntryRemoved => format!("removed Gezik's entry; the rest changed since and was kept: {what}"),
        Outcome::LeftAsIs => format!("left as is (changed by something else since): {what}"),
        Outcome::NotEmpty => format!("left (not empty): {what}"),
        Outcome::Failed(why) => format!("failed: {what}: {why}"),
    }
}

/// `--unregister`'s exit code (spec 11.4): 0 all undone, 1 some left as they were, 2 an error.
pub fn exit_code(outcomes: &[Outcome]) -> i32 {
    if outcomes.iter().any(|o| matches!(o, Outcome::Failed(_))) {
        2
    } else if outcomes.iter().any(|o| matches!(o, Outcome::LeftAsIs | Outcome::NotEmpty)) {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    /// The system as a map; `full` places are folders/keys with something in them.
    #[derive(Default)]
    struct Fake {
        values: RefCell<BTreeMap<String, Value>>,
        full: Vec<String>,
        sets: RefCell<Vec<String>>,
    }

    fn key(c: &Change) -> String {
        format!("{}|{}|{}", c.kind.name(), c.place, c.name)
    }

    impl Access for Fake {
        fn current(&self, c: &Change) -> io::Result<Value> {
            Ok(self.values.borrow().get(&key(c)).cloned().unwrap_or(Value::Absent))
        }
        fn set(&self, c: &Change, value: &Value) -> io::Result<()> {
            if *value == Value::Absent && self.full.contains(&key(c)) {
                return Err(io::Error::new(io::ErrorKind::DirectoryNotEmpty, "not empty"));
            }
            self.sets.borrow_mut().push(key(c));
            let mut values = self.values.borrow_mut();
            match value {
                Value::Absent => values.remove(&key(c)),
                other => values.insert(key(c), other.clone()),
            };
            Ok(())
        }
    }

    fn reg(data: &str) -> Value {
        Value::Reg { ty: RegType::Sz, data: data.into() }
    }

    fn change(kind: Kind, place: &str, before: Value, after: Value) -> Change {
        Change {
            feature: "path".into(),
            kind,
            place: place.into(),
            name: String::new(),
            entry: String::new(),
            before,
            after,
            done: true,
        }
    }

    fn path_change(before: Value, after: Value) -> Change {
        Change {
            name: "Path".into(),
            entry: r"C:\G\bin".into(),
            ..change(Kind::PathEntry, r"HKCU\Environment", before, after)
        }
    }

    #[test]
    fn undo_restores_newest_first_where_still_gezik_s() {
        let fake = Fake::default();
        let first = change(Kind::RegistryValue, r"HKCU\A", Value::Absent, reg("x"));
        let second = change(Kind::RegistryValue, r"HKCU\A", reg("x"), reg("y"));
        fake.values.borrow_mut().insert(key(&second), reg("y"));
        let outcomes = undo(&[first.clone(), second], &fake);
        assert_eq!(outcomes, [Outcome::Undone, Outcome::Undone], "y → x first, then x → nothing");
        assert!(fake.values.borrow().is_empty());
        assert_eq!(exit_code(&outcomes), 0);
    }

    #[test]
    fn undo_leaves_what_changed_since() {
        let fake = Fake::default();
        let ours = change(Kind::RegistryValue, r"HKCU\A", Value::Absent, reg("gezik"));
        fake.values.borrow_mut().insert(key(&ours), reg("someone else"));
        let outcomes = undo(std::slice::from_ref(&ours), &fake);
        assert_eq!(outcomes, [Outcome::LeftAsIs]);
        assert_eq!(fake.values.borrow().get(&key(&ours)), Some(&reg("someone else")), "untouched");
        assert!(fake.sets.borrow().is_empty());
        assert_eq!(exit_code(&outcomes), 1);
        assert!(report_line(&ours, &outcomes[0]).starts_with("left as is (changed by something else since): "));
    }

    #[test]
    fn a_change_never_made_is_left() {
        let fake = Fake::default();
        let unfinished =
            Change { done: false, ..change(Kind::File, r"C:\G\bin\gezik.cmd", Value::Absent, Value::Text("x".into())) };
        assert_eq!(undo(&[unfinished], &fake), [Outcome::WasBefore]);
        assert!(fake.sets.borrow().is_empty(), "nothing written for a change that never happened");
    }

    #[test]
    fn a_path_changed_since_loses_only_gezik_entry() {
        let fake = Fake::default();
        let ours = path_change(reg(r"C:\a;"), reg(r"C:\a;C:\G\bin"));
        fake.values.borrow_mut().insert(key(&ours), reg(r"C:\a;C:\G\bin;D:\later"));
        assert_eq!(undo(std::slice::from_ref(&ours), &fake), [Outcome::EntryRemoved]);
        assert_eq!(fake.values.borrow().get(&key(&ours)), Some(&reg(r"C:\a;D:\later")));
        // Already gone: as before.
        assert_eq!(undo(&[ours], &fake), [Outcome::WasBefore]);
    }

    #[test]
    fn a_path_still_as_written_comes_back_byte_for_byte() {
        let fake = Fake::default();
        let before = Value::Reg { ty: RegType::ExpandSz, data: r"%USERPROFILE%\x;;C:\y\;".into() };
        let after = Value::Reg { ty: RegType::ExpandSz, data: r"%USERPROFILE%\x;;C:\y\;C:\G\bin".into() };
        let ours = path_change(before.clone(), after.clone());
        fake.values.borrow_mut().insert(key(&ours), after);
        assert_eq!(undo(std::slice::from_ref(&ours), &fake), [Outcome::Undone]);
        assert_eq!(fake.values.borrow().get(&key(&ours)), Some(&before));
    }

    #[test]
    fn a_folder_with_something_in_it_stays() {
        let made = change(Kind::Folder, "/h/.local/bin", Value::Absent, Value::Present);
        let fake = Fake { full: vec![key(&made)], ..Fake::default() };
        fake.values.borrow_mut().insert(key(&made), Value::Present);
        let outcomes = undo(&[made], &fake);
        assert_eq!(outcomes, [Outcome::NotEmpty]);
        assert!(outcomes[0].settled());
        assert_eq!(exit_code(&outcomes), 1);
    }

    #[test]
    fn a_failure_is_kept_and_exits_2() {
        struct Broken;
        impl Access for Broken {
            fn current(&self, _: &Change) -> io::Result<Value> {
                Err(io::Error::other("no access"))
            }
            fn set(&self, _: &Change, _: &Value) -> io::Result<()> {
                unreachable!()
            }
        }
        let outcomes = undo(&[change(Kind::Folder, "/x", Value::Absent, Value::Present)], &Broken);
        assert!(matches!(&outcomes[0], Outcome::Failed(why) if why.contains("no access")));
        assert!(!outcomes[0].settled());
        assert_eq!(exit_code(&outcomes), 2);
    }

    #[test]
    fn with_entry_keeps_everything_there() {
        let none = |s: &str| s.to_owned();
        let e = r"C:\G\bin";
        assert_eq!(with_entry("", e, none).as_deref(), Some(e));
        assert_eq!(with_entry(r"C:\a", e, none).as_deref(), Some(r"C:\a;C:\G\bin"));
        assert_eq!(with_entry(r"C:\a;", e, none).as_deref(), Some(r"C:\a;C:\G\bin"));
        assert_eq!(
            with_entry(r"%USERPROFILE%\x;;C:\y\ ; C:\z", e, none).as_deref(),
            Some(r"%USERPROFILE%\x;;C:\y\ ; C:\z;C:\G\bin"),
            "no part is trimmed, reordered or deduplicated"
        );
        assert_eq!(with_entry(r"C:\a;c:\g\BIN\", e, none), None, "already there: case and a trailing \\");
        let expand = |s: &str| s.replace("%LOCALAPPDATA%", r"C:\G");
        assert_eq!(with_entry(r"%LOCALAPPDATA%\bin", e, expand), None, "the user's own %VAR% spelling");
    }

    #[test]
    fn without_entry_takes_only_gezik() {
        let e = r"C:\G\bin";
        assert_eq!(without_entry(r"C:\a;C:\G\bin;C:\b", e).as_deref(), Some(r"C:\a;C:\b"));
        assert_eq!(without_entry(r"C:\G\bin", e).as_deref(), Some(""));
        assert_eq!(without_entry(r"C:\a;;C:\G\BIN\;", e).as_deref(), Some(r"C:\a;;"));
        assert_eq!(without_entry(r"C:\a;%LOCALAPPDATA%\bin", e), None, "the user's own spelling stays");
        assert_eq!(without_entry(r"C:\a;C:\G\bin2", e), None);
        assert!(contains_entry(r"x;C:\g\bin", e) && !contains_entry("x;y", e));
    }

    #[test]
    fn a_long_list_round_trips() {
        let list: String = (0..900).map(|i| format!(r"C:\Program Files\Tool {i}\bin;")).collect();
        assert!(list.len() > 10_000);
        let list = list.trim_end_matches(';');
        let added = with_entry(list, r"C:\G\bin", |s| s.to_owned()).unwrap();
        assert_eq!(without_entry(&added, r"C:\G\bin").as_deref(), Some(list));
    }

    #[test]
    fn every_kind_has_a_name_that_reads_back() {
        for kind in Kind::ALL {
            assert_eq!(Kind::from_name(kind.name()), Some(kind));
        }
        assert_eq!(Kind::from_name("macos-default"), Some(Kind::MacDefault));
        assert_eq!(Kind::from_name("macos-pref"), Some(Kind::MacPref));
        assert_eq!(Kind::from_name("mimeapps"), Some(Kind::Mimeapps));
        let pick = Change {
            feature: "default-file-manager".into(),
            kind: Kind::Mimeapps,
            place: "/h/.config/mimeapps.list".into(),
            name: "inode/directory".into(),
            entry: String::new(),
            before: Value::Absent,
            after: Value::Text("gezik.desktop;".into()),
            done: true,
        };
        assert_eq!(what(&pick), "/h/.config/mimeapps.list [inode/directory]");
    }

    #[test]
    fn names_round_trip() {
        for kind in Kind::ALL {
            assert_eq!(Kind::from_name(kind.name()), Some(kind));
        }
        assert_eq!(RegType::from_name("REG_EXPAND_SZ"), Some(RegType::ExpandSz));
        assert_eq!(Kind::from_name("registry"), None);
    }

    #[test]
    fn describe_says_where_and_both_values() {
        let c = path_change(reg(r"C:\a"), reg(r"C:\a;C:\G\bin"));
        assert_eq!(what(&c), r"HKCU\Environment\Path");
        assert_eq!(describe(&c), r#"path: HKCU\Environment\Path: before REG_SZ "C:\a", after REG_SZ "C:\a;C:\G\bin""#);
        let long = "x".repeat(200);
        assert!(describe(&change(Kind::RegistryValue, r"HKCU\A", Value::Absent, reg(&long))).ends_with("…\""));
    }

    #[test]
    fn quoted_entries_and_odd_lists() {
        let none = |s: &str| s.to_owned();
        let e = r"C:\G\bin";
        // A quoted spelling of Gezik's folder is the same folder; other quoted parts stay as typed.
        assert_eq!(with_entry(r#""C:\Program Files\x";"c:\g\bin""#, e, none), None);
        assert_eq!(
            without_entry(r#""C:\Program Files\x";"C:\G\bin\";C:\b"#, e).as_deref(),
            Some(r#""C:\Program Files\x";C:\b"#)
        );
        assert_eq!(with_entry(r#""C:\Program Files\x""#, e, none).as_deref(), Some(r#""C:\Program Files\x";C:\G\bin"#));
        // Only `;` or empty parts: nothing of them is lost.
        assert_eq!(with_entry(";", e, none).as_deref(), Some(r";C:\G\bin"));
        assert_eq!(with_entry(";;", e, none).as_deref(), Some(r";;C:\G\bin"));
        assert_eq!(without_entry(r";C:\G\bin;", e).as_deref(), Some(";"));
        assert_eq!(without_entry("", e), None);
        assert!(!contains_entry(";;", e) && !contains_entry("", e));
        // Every copy of Gezik's own folder comes out; nothing else moves.
        assert_eq!(without_entry(r"C:\G\bin;C:\a;C:\G\bin;C:\a", e).as_deref(), Some(r"C:\a;C:\a"));
        // A folder within Gezik's or a prefix of it is someone else's.
        assert_eq!(without_entry(r"C:\G\bin\sub;C:\G;C:\G\bi", e), None);
        // A trailing `\` still names the folder; a lone `"` is not a quoted entry.
        assert!(contains_entry(r"C:\G\bin\\", e));
        assert!(!contains_entry(r#""C:\G\bin"#, e));
    }

    #[test]
    fn a_path_changed_since_keeps_its_type_and_other_entries() {
        let fake = Fake::default();
        let ours = path_change(
            Value::Reg { ty: RegType::ExpandSz, data: r"%A%\x".into() },
            Value::Reg { ty: RegType::ExpandSz, data: r"%A%\x;C:\G\bin".into() },
        );
        let now = r#"D:\first;%A%\x;"C:\G\bin";;E:\last;"#;
        fake.values.borrow_mut().insert(key(&ours), Value::Reg { ty: RegType::ExpandSz, data: now.into() });
        assert_eq!(undo(std::slice::from_ref(&ours), &fake), [Outcome::EntryRemoved]);
        assert_eq!(
            fake.values.borrow().get(&key(&ours)),
            Some(&Value::Reg { ty: RegType::ExpandSz, data: r"D:\first;%A%\x;;E:\last;".into() })
        );
    }

    #[test]
    fn a_path_gone_or_taken_is_left_as_is() {
        let fake = Fake::default();
        let ours = path_change(reg("a"), reg(r"a;C:\G\bin"));
        assert_eq!(undo(std::slice::from_ref(&ours), &fake), [Outcome::LeftAsIs], "Path deleted since");
        struct Taken;
        impl Access for Taken {
            fn current(&self, _: &Change) -> io::Result<Value> {
                Ok(Value::Link("/opt/gezik".into()))
            }
            fn set(&self, _: &Change, _: &Value) -> io::Result<()> {
                Err(io::Error::new(io::ErrorKind::AlreadyExists, "not Gezik's link"))
            }
        }
        let link = change(Kind::Symlink, "/h/.local/bin/gezik", Value::Absent, Value::Link("/opt/gezik".into()));
        assert_eq!(undo(&[link], &Taken), [Outcome::LeftAsIs]);
    }

    #[test]
    fn exit_codes_and_report_lines() {
        assert_eq!(exit_code(&[]), 0);
        assert_eq!(exit_code(&[Outcome::Undone, Outcome::WasBefore, Outcome::EntryRemoved]), 0);
        assert_eq!(exit_code(&[Outcome::Undone, Outcome::NotEmpty]), 1);
        assert_eq!(exit_code(&[Outcome::LeftAsIs, Outcome::Failed("x".into())]), 2);
        let c = path_change(reg("a"), reg(r"a;C:\G\bin"));
        assert_eq!(report_line(&c, &Outcome::Undone), r"undone: HKCU\Environment\Path");
        assert_eq!(report_line(&c, &Outcome::Failed("denied".into())), r"failed: HKCU\Environment\Path: denied");
        let default = change(Kind::RegistryValue, r"HKCU\K", Value::Absent, reg("v"));
        assert_eq!(what(&default), r"HKCU\K\(Default)");
        assert!(describe(&Change { done: false, ..default }).ends_with("(not finished)"));
    }
}
