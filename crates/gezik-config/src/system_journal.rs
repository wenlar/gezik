//! `system-changes.toml` (spec 11.1): every change Gezik makes to the system, written before
//! it is made, with the value it replaced. Read only when asked (the panel, a command,
//! `--unregister`; never at start, spec 11.3). Every read-modify-write holds
//! `system-changes.lock`: two Gezik processes, or `--unregister` while a window runs, never
//! lose each other's entries. A file that cannot be read is an error, never "empty": it is
//! not written over.

use gezik_i18n::t;
use std::fmt;
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

use gezik_core::system_change::{Change, Kind, RegType, Value};
use toml_edit::{ArrayOfTables, DocumentMut, InlineTable, Item, Table, value};

use crate::paths::write_atomic;

pub const FILE: &str = "system-changes.toml";
const LOCK: &str = "system-changes.lock";
pub const VERSION: i64 = 1;
/// Larger is not a file Gezik wrote.
const MAX_BYTES: u64 = 1 << 20;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Journal {
    /// The exe of the last Gezik that wrote (spec 6.3's moved check).
    pub exe: String,
    /// The host name of the computer that wrote, for people (portable mode; "" otherwise).
    pub machine: String,
    pub changes: Vec<Change>,
}

#[derive(Debug)]
pub enum JournalError {
    Io(io::Error),
    Bad(String),
}

impl fmt::Display for JournalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JournalError::Io(err) => write!(f, "{FILE}: {err}"),
            JournalError::Bad(why) => write!(f, "{FILE} cannot be read: {why}"),
        }
    }
}

impl From<io::Error> for JournalError {
    fn from(err: io::Error) -> Self {
        JournalError::Io(err)
    }
}

pub fn parse(text: &str) -> Result<Journal, String> {
    let table: toml::Table = text.parse().map_err(|err: toml::de::Error| {
        err.to_string().lines().next().map_or_else(|| t!("not TOML").into_owned(), str::to_owned)
    })?;
    match table.get("version").and_then(toml::Value::as_integer) {
        Some(VERSION) => {}
        Some(other) => return Err(t!("version {} is not one this Gezik knows", other)),
        None => return Err(t!("it has no version").into()),
    }
    let exe = table.get("exe").and_then(toml::Value::as_str).unwrap_or_default().to_owned();
    let machine = table.get("machine").and_then(toml::Value::as_str).unwrap_or_default().to_owned();
    let mut changes = Vec::new();
    if let Some(list) = table.get("change") {
        let list = list.as_array().ok_or_else(|| t!("{} is not a list", "change"))?;
        for (i, item) in list.iter().enumerate() {
            let entry = item.as_table().ok_or_else(|| t!("change {} is not a table", i + 1))?;
            changes.push(parse_change(entry).map_err(|why| format!("change {}: {why}", i + 1))?);
        }
    }
    Ok(Journal { exe, machine, changes })
}

fn parse_change(table: &toml::Table) -> Result<Change, String> {
    let text = |key: &str| -> Result<String, String> {
        match table.get(key) {
            None => Ok(String::new()),
            Some(v) => v.as_str().map(str::to_owned).ok_or_else(|| t!("{} is not text", key)),
        }
    };
    let feature = text("feature")?;
    if feature.is_empty() || !feature.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        return Err(t!("{} is not a name", "feature"));
    }
    let kind = Kind::from_name(&text("kind")?).ok_or_else(|| t!("unknown kind"))?;
    let place = text("where")?;
    if place.is_empty() || place.contains('\0') {
        return Err(t!("{} is empty", "where"));
    }
    let entry = text("entry")?;
    if kind == Kind::PathEntry && entry.is_empty() {
        return Err(t!("a path-entry without its entry").into());
    }
    let done = match table.get("done") {
        None => false,
        Some(v) => v.as_bool().ok_or_else(|| t!("{} is not true or false", "done"))?,
    };
    Ok(Change {
        feature,
        kind,
        place,
        name: text("name")?,
        entry,
        before: parse_value(table.get("before"))?,
        after: parse_value(table.get("after"))?,
        done,
    })
}

fn parse_value(value: Option<&toml::Value>) -> Result<Value, String> {
    let table = value.and_then(toml::Value::as_table).ok_or_else(|| t!("before or after is missing"))?;
    let flag = |key: &str| table.get(key).and_then(toml::Value::as_bool) == Some(true);
    let text = |key: &str| table.get(key).and_then(toml::Value::as_str).map(str::to_owned);
    Ok(if flag("absent") {
        Value::Absent
    } else if flag("present") {
        Value::Present
    } else if flag("other") {
        Value::Other
    } else if let (Some(ty), Some(data)) = (text("type"), text("data")) {
        Value::Reg { ty: RegType::from_name(&ty).ok_or_else(|| t!("unknown registry type"))?, data }
    } else if let Some(text) = text("text") {
        Value::Text(text)
    } else if let Some(target) = text("target") {
        Value::Link(target)
    } else {
        return Err(t!("a value of no known shape").into());
    })
}

pub fn to_text(journal: &Journal) -> String {
    let mut doc = DocumentMut::new();
    doc["version"] = value(VERSION);
    doc["exe"] = value(journal.exe.as_str());
    if !journal.machine.is_empty() {
        doc["machine"] = value(journal.machine.as_str());
    }
    let mut list = ArrayOfTables::new();
    for change in &journal.changes {
        let mut table = Table::new();
        table["feature"] = value(change.feature.as_str());
        table["kind"] = value(change.kind.name());
        table["where"] = value(change.place.as_str());
        if matches!(change.kind, Kind::RegistryValue | Kind::PathEntry | Kind::Mimeapps) {
            table["name"] = value(change.name.as_str());
        }
        if !change.entry.is_empty() {
            table["entry"] = value(change.entry.as_str());
        }
        table["before"] = value(inline(&change.before));
        table["after"] = value(inline(&change.after));
        table["done"] = value(change.done);
        list.push(table);
    }
    if !list.is_empty() {
        doc["change"] = Item::ArrayOfTables(list);
    }
    doc.to_string()
}

fn inline(v: &Value) -> InlineTable {
    let mut table = InlineTable::new();
    match v {
        Value::Absent => drop(table.insert("absent", true.into())),
        Value::Present => drop(table.insert("present", true.into())),
        Value::Other => drop(table.insert("other", true.into())),
        Value::Reg { ty, data } => {
            table.insert("type", ty.name().into());
            table.insert("data", data.as_str().into());
        }
        Value::Text(text) => drop(table.insert("text", text.as_str().into())),
        Value::Link(target) => drop(table.insert("target", target.as_str().into())),
    }
    table
}

/// The key a portable journal is named by (spec 14 decision 14): a short hash of the
/// computer's stable id (it survives a rename and a new network name), else of its host name;
/// `None` when neither is known.
pub fn machine_key(id: Option<&str>, host: Option<&str>) -> Option<String> {
    let source = [id, host].into_iter().flatten().map(str::trim).find(|s| !s.is_empty())?;
    let hash = source
        .to_lowercase()
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3));
    Some(format!("{:012x}", hash >> 16))
}

/// The journal's name: `system-changes.toml`, or in portable mode one per computer,
/// `system-changes-<key>.toml` (a drive opened on another computer must not list this one's
/// changes).
pub fn file_name(key: Option<&str>) -> String {
    key.map_or_else(|| FILE.to_owned(), |key| format!("system-changes-{key}.toml"))
}

/// Whether `name` is a journal's file name (one of the portable ones too).
fn is_journal_name(name: &str) -> bool {
    name == FILE || (name.starts_with("system-changes-") && name.ends_with(".toml"))
}

/// The name 14e's first build gave a portable journal: the host name itself.
fn host_name(host: &str) -> String {
    let safe: String = host.trim().chars().map(|c| if c.is_alphanumeric() || c == '-' { c } else { '_' }).collect();
    format!("system-changes-{safe}.toml")
}

/// The portable journal to use (pure): `ours` if it is there; else the one other journal this
/// computer wrote, told by the host name in it (or in its name, from 14e's first build), when
/// there is exactly one: the id could not be read then or now. Never another computer's.
/// `found`: each journal file in the folder, with the `machine` written in it.
pub fn pick_name(ours: &str, host: Option<&str>, found: &[(String, String)]) -> String {
    if found.iter().any(|(name, _)| name == ours) {
        return ours.to_owned();
    }
    let Some(host) = host.map(str::trim).filter(|h| !h.is_empty()) else { return ours.to_owned() };
    let legacy = host_name(host);
    let mine: Vec<&String> = found
        .iter()
        .filter(|(name, machine)| name != FILE && (machine.eq_ignore_ascii_case(host) || *name == legacy))
        .map(|(name, _)| name)
        .collect();
    match mine.as_slice() {
        [one] => (*one).clone(),
        _ => ours.to_owned(),
    }
}

/// `system-changes.toml` in a config folder.
#[derive(Debug, Clone)]
pub struct JournalFile {
    dir: PathBuf,
    name: String,
    /// Written into the journal as `machine` (portable mode).
    host: String,
}

impl JournalFile {
    pub fn new(dir: PathBuf) -> JournalFile {
        JournalFile { dir, name: FILE.to_owned(), host: String::new() }
    }

    /// Portable mode's journal in `dir` for the computer of `id` and `host` ([`pick_name`]):
    /// reads the folder and the `machine` of each other journal in it.
    pub fn portable(dir: PathBuf, id: Option<&str>, host: Option<&str>) -> JournalFile {
        let ours = file_name(machine_key(id, host).as_deref());
        let found: Vec<(String, String)> = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter(|name| is_journal_name(name))
            .map(|name| {
                let path = dir.join(&name);
                let small = path.symlink_metadata().is_ok_and(|m| m.is_file() && m.len() <= MAX_BYTES);
                let text = if small { std::fs::read_to_string(&path).unwrap_or_default() } else { String::new() };
                let machine = parse(&text).map(|journal| journal.machine).unwrap_or_default();
                (name, machine)
            })
            .collect();
        let name = pick_name(&ours, host, &found);
        JournalFile { dir, name, host: host.unwrap_or_default().trim().to_owned() }
    }

    pub fn path(&self) -> PathBuf {
        self.dir.join(&self.name)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// One `metadata` call: whether there is a journal at all.
    pub fn exists(&self) -> bool {
        self.path().symlink_metadata().is_ok()
    }

    /// Takes the lock (waiting for another Gezik) and reads the journal. Only a plain file
    /// of at most 1 MB that parses is read; a link is not followed.
    pub fn lock(&self) -> Result<Locked, JournalError> {
        std::fs::create_dir_all(&self.dir)?;
        let lock = File::options().create(true).truncate(false).write(true).open(self.dir.join(LOCK))?;
        lock.lock()?;
        let path = self.path();
        let journal = match std::fs::symlink_metadata(&path) {
            Err(err) if err.kind() == io::ErrorKind::NotFound => Journal::default(),
            Err(err) => return Err(err.into()),
            Ok(meta) if !meta.is_file() => return Err(JournalError::Bad(t!("it is not a plain file").into())),
            Ok(meta) if meta.len() > MAX_BYTES => return Err(JournalError::Bad(t!("it is larger than 1 MB").into())),
            Ok(_) => {
                let text = std::fs::read_to_string(&path).map_err(|err| JournalError::Bad(err.to_string()))?;
                parse(&text).map_err(JournalError::Bad)?
            }
        };
        Ok(Locked { _lock: lock, path, journal, host: self.host.clone() })
    }
}

/// The journal read under its lock; the lock goes with it.
pub struct Locked {
    _lock: File,
    path: PathBuf,
    journal: Journal,
    host: String,
}

impl Locked {
    pub fn journal(&self) -> &Journal {
        &self.journal
    }

    /// The folder the journal is in.
    pub fn dir(&self) -> &Path {
        self.path.parent().unwrap_or(&self.path)
    }

    /// Write-ahead (spec 11.1): `change` is written with `done = false`, then `make` runs,
    /// then it is marked done. If `make` fails the entry stays unfinished: the undo looks
    /// at what is there and takes back only what was made.
    pub fn record(
        &mut self,
        change: Change,
        exe: &str,
        make: impl FnOnce(&Change) -> io::Result<()>,
    ) -> io::Result<()> {
        exe.clone_into(&mut self.journal.exe);
        if !self.host.is_empty() {
            self.host.clone_into(&mut self.journal.machine);
        }
        self.journal.changes.push(Change { done: false, ..change });
        if let Err(err) = self.save() {
            // Not on disk, so not made: the list stays what the file holds.
            self.journal.changes.pop();
            return Err(err);
        }
        let last = self.journal.changes.len() - 1;
        make(&self.journal.changes[last])?;
        self.journal.changes[last].done = true;
        self.save()
    }

    /// Keeps the changes `keep` says yes to (by their place in the list) and writes the
    /// file; with none left the file goes.
    pub fn retain(&mut self, mut keep: impl FnMut(usize, &Change) -> bool) -> io::Result<()> {
        let mut i = 0;
        self.journal.changes.retain(|change| {
            let kept = keep(i, change);
            i += 1;
            kept
        });
        self.save()
    }

    fn save(&self) -> io::Result<()> {
        if self.journal.changes.is_empty() {
            return match std::fs::remove_file(&self.path) {
                Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
                other => other,
            };
        }
        write_atomic(&self.path, &to_text(&self.journal))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_portable_journal_is_named_by_the_machine_id() {
        assert_eq!(file_name(None), FILE);
        assert_eq!(machine_key(None, None), None);
        assert_eq!(machine_key(Some(" "), Some("")), None);
        let id = machine_key(Some("4c4c4544-0042-3510-8051-b4c04f4d4e31"), Some("OFİS-PC")).unwrap();
        assert_eq!(id.len(), 12);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
        // The id wins: a new host name (DHCP, a rename) keeps the name.
        assert_eq!(machine_key(Some("4C4C4544-0042-3510-8051-B4C04F4D4E31"), Some("other")).unwrap(), id);
        // No id: the host name.
        let by_host = machine_key(None, Some("mac")).unwrap();
        assert_ne!(by_host, id);
        assert_eq!(file_name(Some(&id)), format!("system-changes-{id}.toml"));
    }

    #[test]
    fn only_this_computers_journal_is_taken_over() {
        let found = |list: &[(&str, &str)]| -> Vec<(String, String)> {
            list.iter().map(|(n, m)| ((*n).to_owned(), (*m).to_owned())).collect()
        };
        let ours = "system-changes-aaaaaaaaaaaa.toml";
        let pick = |list: &[(&str, &str)]| pick_name(ours, Some("PC1"), &found(list));
        assert_eq!(pick(&[]), ours);
        assert_eq!(pick(&[(ours, "PC1"), ("system-changes-bbbbbbbbbbbb.toml", "PC1")]), ours);
        // The id could not be read before (or now): the one written by this host is used.
        assert_eq!(pick(&[("system-changes-bbbbbbbbbbbb.toml", "pc1")]), "system-changes-bbbbbbbbbbbb.toml");
        // 14e's first build named it by the host.
        assert_eq!(pick(&[("system-changes-PC1.toml", "")]), "system-changes-PC1.toml");
        // Another computer's is never taken, even when it is the only one.
        assert_eq!(pick(&[("system-changes-cccccccccccc.toml", "LAPTOP")]), ours);
        // Two of this host's: not a guess.
        assert_eq!(pick(&[("system-changes-b.toml", "PC1"), ("system-changes-PC1.toml", "")]), ours);
        assert_eq!(pick(&[(FILE, "PC1")]), ours, "the installed Gezik's is not a portable one");
        assert_eq!(pick_name(ours, None, &found(&[("system-changes-b.toml", "")])), ours);
    }

    #[test]
    fn a_portable_journal_keeps_its_host_and_is_found_again() {
        let dir = crate::test_dir("journal-portable");
        let file = JournalFile::portable(dir.clone(), None, Some("PC1"));
        let change = sample().changes[0].clone();
        file.lock().unwrap().record(change, "x", |_| Ok(())).unwrap();
        let text = std::fs::read_to_string(file.path()).unwrap();
        assert!(text.contains("machine = \"PC1\""), "{text}");
        assert_eq!(parse(&text).unwrap().machine, "PC1");
        // Now the id can be read: the journal written by host name is still this one.
        let again = JournalFile::portable(dir.clone(), Some("an-id"), Some("PC1"));
        assert_eq!(again.path(), file.path());
        // Another computer starts its own.
        let other = JournalFile::portable(dir.clone(), Some("other-id"), Some("PC2"));
        assert_ne!(other.path(), file.path());
        assert!(!other.exists());
    }

    fn sample() -> Journal {
        let base = Change {
            feature: "path".into(),
            kind: Kind::Folder,
            place: r"C:\Users\Ömer\AppData\Local\Gezik".into(),
            name: String::new(),
            entry: String::new(),
            before: Value::Absent,
            after: Value::Present,
            done: true,
        };
        Journal {
            exe: r"C:\Tools\Gezik 'x'\gezik.exe".into(),
            machine: "PC1".into(),
            changes: vec![
                base.clone(),
                Change {
                    kind: Kind::File,
                    place: r"C:\G\bin\gezik.cmd".into(),
                    after: Value::Text("@echo off\r\nstart \"\" gezik.exe %*\r\n".into()),
                    ..base.clone()
                },
                Change {
                    kind: Kind::PathEntry,
                    place: r"HKCU\Environment".into(),
                    name: "Path".into(),
                    entry: r"C:\G\bin".into(),
                    before: Value::Reg { ty: RegType::ExpandSz, data: r"%USERPROFILE%\x;;".into() },
                    after: Value::Reg { ty: RegType::ExpandSz, data: r"%USERPROFILE%\x;;C:\G\bin".into() },
                    done: false,
                    ..base.clone()
                },
                Change {
                    kind: Kind::Mimeapps,
                    place: "/h/.config/mimeapps.list".into(),
                    name: "inode/directory".into(),
                    before: Value::Text("org.gnome.Nautilus.desktop;".into()),
                    after: Value::Text("gezik.desktop;".into()),
                    ..base.clone()
                },
                Change {
                    kind: Kind::MacDefault,
                    place: "public.folder".into(),
                    before: Value::Text("com.apple.finder".into()),
                    after: Value::Text("com.wenlar.gezik".into()),
                    ..base.clone()
                },
                Change {
                    kind: Kind::Symlink,
                    place: "/h/.local/bin/gezik".into(),
                    after: Value::Link("/opt/gé zik".into()),
                    ..base
                },
            ],
        }
    }

    #[test]
    fn every_kind_and_value_round_trips() {
        let journal = sample();
        let text = to_text(&journal);
        assert!(text.starts_with("version = 1\n"), "{text}");
        assert!(text.contains("[[change]]") && text.contains("where = "), "{text}");
        assert_eq!(parse(&text), Ok(journal));
    }

    #[test]
    fn files_gezik_did_not_write_are_refused() {
        for bad in [
            "not toml [",
            "exe = 'x'",
            "version = 2",
            "version = 1\n[[change]]\nfeature = 'path'\nkind = 'registry'\nwhere = 'x'\nbefore = { absent = true }\nafter = { absent = true }",
            "version = 1\n[[change]]\nfeature = 'path'\nkind = 'path-entry'\nwhere = 'x'\nbefore = { absent = true }\nafter = { absent = true }",
            "version = 1\n[[change]]\nfeature = 'path'\nkind = 'file'\nwhere = 'x'\nbefore = { absent = true }\nafter = { colour = 'red' }",
            "version = 1\n[[change]]\nfeature = 'Path!'\nkind = 'file'\nwhere = 'x'\nbefore = { absent = true }\nafter = { absent = true }",
            "version = 1\n[[change]]\nfeature = 'path'\nkind = 'file'\nwhere = ''\nbefore = { absent = true }\nafter = { absent = true }",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn record_writes_before_it_makes() {
        let dir = crate::test_dir("journal-record");
        let file = JournalFile::new(dir.clone());
        let mut locked = file.lock().unwrap();
        let change = sample().changes[0].clone();
        locked
            .record(change.clone(), "/gezik", |made| {
                let on_disk = parse(&std::fs::read_to_string(dir.join(FILE)).unwrap()).unwrap();
                assert_eq!(on_disk.changes, [Change { done: false, ..made.clone() }], "written, not done yet");
                Ok(())
            })
            .unwrap();
        let on_disk = parse(&std::fs::read_to_string(dir.join(FILE)).unwrap()).unwrap();
        assert!(on_disk.changes[0].done);
        assert_eq!(on_disk.exe, "/gezik");
    }

    #[test]
    fn a_failed_make_keeps_the_entry_unfinished() {
        let dir = crate::test_dir("journal-failed");
        let file = JournalFile::new(dir.clone());
        let mut locked = file.lock().unwrap();
        let err = locked.record(sample().changes[0].clone(), "/g", |_| Err(io::Error::other("denied"))).unwrap_err();
        assert_eq!(err.to_string(), "denied");
        drop(locked);
        let on_disk = file.lock().unwrap();
        assert_eq!(on_disk.journal().changes.len(), 1);
        assert!(!on_disk.journal().changes[0].done, "the undo decides from what is there");
    }

    #[test]
    fn the_last_entry_gone_removes_the_file() {
        let dir = crate::test_dir("journal-empty");
        let file = JournalFile::new(dir.clone());
        let mut locked = file.lock().unwrap();
        locked.record(sample().changes[0].clone(), "/g", |_| Ok(())).unwrap();
        assert!(file.exists());
        locked.retain(|_, _| false).unwrap();
        assert!(!file.exists());
    }

    #[test]
    fn a_bad_file_is_never_written_over() {
        let dir = crate::test_dir("journal-bad");
        std::fs::write(dir.join(FILE), "version = 7\n").unwrap();
        let file = JournalFile::new(dir.clone());
        assert!(matches!(file.lock(), Err(JournalError::Bad(why)) if why.contains("version 7")));
        assert_eq!(std::fs::read_to_string(dir.join(FILE)).unwrap(), "version = 7\n");
        std::fs::write(dir.join(FILE), vec![b'#'; 2 << 20]).unwrap();
        assert!(matches!(file.lock(), Err(JournalError::Bad(_))), "larger than 1 MB");
    }

    #[test]
    fn a_lock_file_left_by_a_crashed_gezik_does_not_block() {
        // The OS lock goes with the process; the file staying behind means nothing.
        let dir = crate::test_dir("journal-stale");
        std::fs::write(dir.join(LOCK), "left over").unwrap();
        assert!(JournalFile::new(dir).lock().unwrap().journal().changes.is_empty());
    }

    #[test]
    fn two_writers_lose_nothing() {
        let dir = crate::test_dir("journal-two");
        let threads: Vec<_> = (0..8)
            .map(|n| {
                let file = JournalFile::new(dir.clone());
                std::thread::spawn(move || {
                    for i in 0..10 {
                        let mut locked = file.lock().unwrap();
                        let change = Change { place: format!("/p/{n}/{i}"), ..sample().changes[0].clone() };
                        locked.record(change, "/g", |_| Ok(())).unwrap();
                    }
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        assert_eq!(JournalFile::new(dir).lock().unwrap().journal().changes.len(), 80);
    }
}
