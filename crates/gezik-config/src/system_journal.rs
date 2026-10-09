//! `system-changes.toml` (spec 11.1): every change Gezik makes to the system, written before
//! it is made, with the value it replaced. Read only when asked (the panel, a command,
//! `--unregister`; never at start, spec 11.3). Every read-modify-write holds
//! `system-changes.lock`: two Gezik processes, or `--unregister` while a window runs, never
//! lose each other's entries. A file that cannot be read is an error, never "empty": it is
//! not written over.

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
    let table: toml::Table =
        text.parse().map_err(|err: toml::de::Error| err.to_string().lines().next().unwrap_or("not TOML").to_owned())?;
    match table.get("version").and_then(toml::Value::as_integer) {
        Some(VERSION) => {}
        Some(other) => return Err(format!("version {other} is not one this Gezik knows")),
        None => return Err("it has no version".into()),
    }
    let exe = table.get("exe").and_then(toml::Value::as_str).unwrap_or_default().to_owned();
    let mut changes = Vec::new();
    if let Some(list) = table.get("change") {
        let list = list.as_array().ok_or("change is not a list")?;
        for (i, item) in list.iter().enumerate() {
            let entry = item.as_table().ok_or_else(|| format!("change {} is not a table", i + 1))?;
            changes.push(parse_change(entry).map_err(|why| format!("change {}: {why}", i + 1))?);
        }
    }
    Ok(Journal { exe, changes })
}

fn parse_change(table: &toml::Table) -> Result<Change, String> {
    let text = |key: &str| -> Result<String, String> {
        match table.get(key) {
            None => Ok(String::new()),
            Some(v) => v.as_str().map(str::to_owned).ok_or_else(|| format!("{key} is not text")),
        }
    };
    let feature = text("feature")?;
    if feature.is_empty() || !feature.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        return Err("feature is not a name".into());
    }
    let kind = Kind::from_name(&text("kind")?).ok_or("unknown kind")?;
    let place = text("where")?;
    if place.is_empty() || place.contains('\0') {
        return Err("where is empty".into());
    }
    let entry = text("entry")?;
    if kind == Kind::PathEntry && entry.is_empty() {
        return Err("a path-entry without its entry".into());
    }
    let done = match table.get("done") {
        None => false,
        Some(v) => v.as_bool().ok_or("done is not true or false")?,
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
    let table = value.and_then(toml::Value::as_table).ok_or("before or after is missing")?;
    let flag = |key: &str| table.get(key).and_then(toml::Value::as_bool) == Some(true);
    let text = |key: &str| table.get(key).and_then(toml::Value::as_str).map(str::to_owned);
    Ok(if flag("absent") {
        Value::Absent
    } else if flag("present") {
        Value::Present
    } else if flag("other") {
        Value::Other
    } else if let (Some(ty), Some(data)) = (text("type"), text("data")) {
        Value::Reg { ty: RegType::from_name(&ty).ok_or("unknown registry type")?, data }
    } else if let Some(text) = text("text") {
        Value::Text(text)
    } else if let Some(target) = text("target") {
        Value::Link(target)
    } else {
        return Err("a value of no known shape".into());
    })
}

pub fn to_text(journal: &Journal) -> String {
    let mut doc = DocumentMut::new();
    doc["version"] = value(VERSION);
    doc["exe"] = value(journal.exe.as_str());
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

/// `system-changes.toml` in a config folder.
#[derive(Debug, Clone)]
pub struct JournalFile {
    dir: PathBuf,
}

impl JournalFile {
    pub fn new(dir: PathBuf) -> JournalFile {
        JournalFile { dir }
    }

    pub fn path(&self) -> PathBuf {
        self.dir.join(FILE)
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
            Ok(meta) if !meta.is_file() => return Err(JournalError::Bad("it is not a plain file".into())),
            Ok(meta) if meta.len() > MAX_BYTES => return Err(JournalError::Bad("it is larger than 1 MB".into())),
            Ok(_) => {
                let text = std::fs::read_to_string(&path).map_err(|err| JournalError::Bad(err.to_string()))?;
                parse(&text).map_err(JournalError::Bad)?
            }
        };
        Ok(Locked { _lock: lock, path, journal })
    }
}

/// The journal read under its lock; the lock goes with it.
pub struct Locked {
    _lock: File,
    path: PathBuf,
    journal: Journal,
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
