//! `pending-deletes`: the hidden folders an instant delete has not finished, one per line
//! (`deleting<TAB>pid<TAB>path`; an older Gezik wrote the path alone). Read at start, so a
//! delete cut short (Gezik closed or crashed) finishes later, unless the process that noted
//! it still runs (another Gezik window is deleting it). A
//! line starting with `restore` and a tab is instead a hidden folder that could not be put
//! back under its own name (something held it open): at start it is put back, not deleted.
//! A line starting with `copies` and a tab is a folder a running copy writes into: what it
//! left under its temporary names (files starting with its prefix) is deleted at start, unless
//! the process that wrote the line still runs.

use std::hash::{BuildHasher, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::engine::lock;

/// How the folders an instant delete hides are named.
pub const HIDDEN_PREFIX: &str = ".gezik-deleting-";

/// How a file being copied is named until it is complete (a leftover is deleted).
pub const COPYING_PREFIX: &str = ".gezik-copying-";

/// How an item being renamed through a temporary name is named meanwhile (a leftover is put
/// back under its own name).
pub const RENAMING_PREFIX: &str = ".gezik-rn-";

/// Starts a copies line: `copies<TAB>pid<TAB>folder<TAB>prefix`.
const COPIES: &str = "copies\t";

/// Starts a delete line: `deleting<TAB>pid<TAB>path`.
const DELETING: &str = "deleting\t";

/// A folder a copy writes its files into under temporary names starting with `prefix`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyNote {
    /// The process that copies.
    pub pid: u32,
    pub folder: PathBuf,
    pub prefix: String,
}

/// Starts a restore line: `restore<TAB>hidden<TAB>original<TAB>0|1` (was it hidden before).
const RESTORE: &str = "restore\t";

/// A hidden folder to put back under its own name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Restore {
    pub hidden: PathBuf,
    pub original: PathBuf,
    /// The folder had the hidden attribute before it was hidden.
    pub was_hidden: bool,
}

pub struct PendingDeletes {
    file: PathBuf,
    guard: Mutex<()>,
}

impl PendingDeletes {
    pub fn new(file: PathBuf) -> PendingDeletes {
        PendingDeletes { file, guard: Mutex::new(()) }
    }

    pub fn add(&self, path: &Path) -> io::Result<()> {
        self.add_all(&[path])
    }

    /// [`Self::add`] for many at once, in one write.
    pub fn add_all(&self, paths: &[&Path]) -> io::Result<()> {
        let _guard = lock(&self.guard);
        let mut list = self.read();
        let mut at: std::collections::HashMap<PathBuf, usize> = list
            .iter()
            .enumerate()
            .filter_map(|(index, line)| delete_of(line).map(|(_, noted)| (noted, index)))
            .collect();
        for path in paths {
            let line = PathBuf::from(format!("{DELETING}{}\t{}", std::process::id(), path.display()));
            match at.get(*path) {
                // Noted already: it keeps its place, under this process.
                Some(&index) => list[index] = line,
                None => {
                    at.insert(path.to_path_buf(), list.len());
                    list.push(line);
                }
            }
        }
        self.write(&list)
    }

    pub fn remove(&self, path: &Path) {
        self.remove_all(&[path]);
    }

    /// [`Self::remove`] for many at once, in one write.
    pub fn remove_all(&self, paths: &[&Path]) {
        if paths.is_empty() {
            return;
        }
        let _guard = lock(&self.guard);
        let paths: std::collections::HashSet<&Path> = paths.iter().copied().collect();
        let mut list = self.read();
        let before = list.len();
        list.retain(|line| delete_of(line).is_none_or(|(_, noted)| !paths.contains(noted.as_path())));
        if list.len() != before {
            let _ = self.write(&list);
        }
    }

    /// `restore.hidden` is to be put back, never deleted: its delete note becomes a restore
    /// note.
    pub fn add_restore(&self, restore: &Restore) {
        self.add_restores(std::slice::from_ref(restore));
    }

    /// [`Self::add_restore`] for many at once, in one write.
    pub fn add_restores(&self, restores: &[Restore]) {
        if restores.is_empty() {
            return;
        }
        let _guard = lock(&self.guard);
        let hidden: std::collections::HashSet<&Path> = restores.iter().map(|r| r.hidden.as_path()).collect();
        let mut list = self.read();
        list.retain(|p| {
            delete_of(p).is_none_or(|(_, noted)| !hidden.contains(noted.as_path()))
                && restore_of(p).is_none_or(|r| !hidden.contains(r.hidden.as_path()))
        });
        list.extend(restores.iter().map(|restore| {
            PathBuf::from(format!(
                "{RESTORE}{}\t{}\t{}",
                restore.hidden.display(),
                restore.original.display(),
                u8::from(restore.was_hidden)
            ))
        }));
        let _ = self.write(&list);
    }

    pub fn remove_restore(&self, hidden: &Path) {
        self.remove_restores(&[hidden]);
    }

    /// [`Self::remove_restore`] for many at once, in one write.
    pub fn remove_restores(&self, hidden: &[&Path]) {
        if hidden.is_empty() {
            return;
        }
        let _guard = lock(&self.guard);
        let hidden: std::collections::HashSet<&Path> = hidden.iter().copied().collect();
        let mut list = self.read();
        let before = list.len();
        list.retain(|p| restore_of(p).is_none_or(|r| !hidden.contains(r.hidden.as_path())));
        if list.len() != before {
            let _ = self.write(&list);
        }
    }

    /// The listed restores of folders Gezik hid, each back into the folder it is in.
    pub fn restores(&self) -> Vec<Restore> {
        let _guard = lock(&self.guard);
        self.read().iter().filter_map(|line| restore_of(line)).collect()
    }

    /// `folder` gets files named `prefix…` from process `pid` until the copy ends.
    pub fn add_copies(&self, pid: u32, folder: &Path, prefix: &str) {
        let _guard = lock(&self.guard);
        let mut list = self.read();
        let line = PathBuf::from(format!("{COPIES}{pid}\t{}\t{prefix}", folder.display()));
        if !list.contains(&line) {
            list.push(line);
            let _ = self.write(&list);
        }
    }

    /// The copy with `prefix` ended: none of its files are left under temporary names.
    pub fn remove_copies(&self, prefix: &str) {
        let _guard = lock(&self.guard);
        let mut list = self.read();
        let before = list.len();
        list.retain(|p| copies_of(p).is_none_or(|note| note.prefix != prefix));
        if list.len() != before {
            let _ = self.write(&list);
        }
    }

    pub fn copies(&self) -> Vec<CopyNote> {
        let _guard = lock(&self.guard);
        self.read().iter().filter_map(|line| copies_of(line)).collect()
    }

    /// Where copies keep their journals (see `journal.rs`).
    pub fn journal_dir(&self) -> PathBuf {
        self.file.with_file_name("copying")
    }

    /// The listed folders Gezik itself hid; anything else in the file is ignored, so a damaged
    /// or edited file can never make Gezik delete other things.
    pub fn load(&self) -> Vec<PathBuf> {
        let _guard = lock(&self.guard);
        self.read().iter().filter_map(|line| delete_of(line)).map(|(_, path)| path).collect()
    }

    /// Like [`Self::load`], without what a running Gezik process (this one too) is deleting.
    pub fn load_unowned(&self) -> Vec<PathBuf> {
        let _guard = lock(&self.guard);
        self.read()
            .iter()
            .filter_map(|line| delete_of(line))
            .filter(|(pid, _)| pid.is_none_or(|pid| !gezik_platform::process_alive(pid)))
            .map(|(_, path)| path)
            .collect()
    }

    fn read(&self) -> Vec<PathBuf> {
        std::fs::read_to_string(&self.file)
            .unwrap_or_default()
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(PathBuf::from)
            .collect()
    }

    fn write(&self, list: &[PathBuf]) -> io::Result<()> {
        if list.is_empty() {
            return match std::fs::remove_file(&self.file) {
                Err(err) if err.kind() != io::ErrorKind::NotFound => Err(err),
                _ => Ok(()),
            };
        }
        if let Some(parent) = self.file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text: String = list.iter().map(|path| format!("{}\n", path.display())).collect();
        let temp = self.file.with_extension("tmp");
        std::fs::write(&temp, text)?;
        std::fs::rename(&temp, &self.file)
    }
}

/// A restore line, if `line` is one that only renames a folder Gezik hid within its folder.
fn restore_of(line: &Path) -> Option<Restore> {
    let mut parts = line.to_str()?.strip_prefix(RESTORE)?.split('\t');
    let (hidden, original, was_hidden) = (parts.next()?, parts.next()?, parts.next()?);
    let (hidden, original) = (PathBuf::from(hidden), PathBuf::from(original));
    let was_hidden = match was_hidden {
        "0" => false,
        "1" => true,
        _ => return None,
    };
    (parts.next().is_none()
        && is_hidden(&hidden)
        && original.is_absolute()
        && original.file_name().is_some()
        && original.parent() == hidden.parent())
    .then_some(Restore { hidden, original, was_hidden })
}

/// A delete line, if `line` is one for something Gezik hid: the process that noted it (an
/// older Gezik noted none) and the path.
fn delete_of(line: &Path) -> Option<(Option<u32>, PathBuf)> {
    let text = line.to_str()?;
    let (pid, path) = match text.strip_prefix(DELETING) {
        Some(rest) => {
            let (pid, path) = rest.split_once('\t')?;
            (Some(pid.parse().ok()?), PathBuf::from(path))
        }
        None => (None, line.to_path_buf()),
    };
    is_hidden(&path).then_some((pid, path))
}

/// A copies line, if `line` is one whose prefix can only match files a copy named.
fn copies_of(line: &Path) -> Option<CopyNote> {
    let mut parts = line.to_str()?.strip_prefix(COPIES)?.split('\t');
    let (pid, folder, prefix) = (parts.next()?.parse().ok()?, PathBuf::from(parts.next()?), parts.next()?);
    (parts.next().is_none()
        && folder.is_absolute()
        && prefix.len() > COPYING_PREFIX.len()
        && prefix.starts_with(COPYING_PREFIX)
        && !prefix.contains(['/', '\\']))
    .then(|| CopyNote { pid, folder, prefix: prefix.to_owned() })
}

/// Whether `path` is an absolute path to a folder an instant delete hid, to a file a copy
/// had not finished, or to an item a rename holds under a temporary name.
pub fn is_hidden(path: &Path) -> bool {
    path.is_absolute() && path.file_name().and_then(|name| name.to_str()).is_some_and(is_internal_name)
}

/// Whether `name` is one Gezik gives what a copy, delete or rename is still working on (lists
/// leave those out).
pub fn is_internal_name(name: &str) -> bool {
    [HIDDEN_PREFIX, COPYING_PREFIX, RENAMING_PREFIX]
        .iter()
        .any(|prefix| name.len() > prefix.len() && name.starts_with(prefix))
}

/// The process that holds `path` under a rename's temporary name (`.gezik-rn-{pid}-{n}`).
pub fn renaming_pid(path: &Path) -> Option<u32> {
    let rest = path.file_name()?.to_str()?.strip_prefix(RENAMING_PREFIX)?;
    rest.split_once('-')?.0.parse().ok()
}

/// A fresh temporary name next to `path` for an item set aside for a while
/// (`.gezik-rn-{pid}-{n}`): a leftover is put back under its own name, never deleted.
pub fn renaming_name(path: &Path) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    path.with_file_name(format!("{RENAMING_PREFIX}{}-{n}", std::process::id()))
}

/// A fresh hidden name.
pub fn hidden_name() -> String {
    format!("{HIDDEN_PREFIX}{}", unique())
}

/// A fresh prefix for the temporary names of one copy's files.
pub fn copy_prefix() -> String {
    format!("{COPYING_PREFIX}{}-", unique())
}

fn unique() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
    hasher.write_u64(nanos);
    hasher.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    format!("{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::test_dir;

    #[test]
    fn adds_removes_and_deletes_the_file_when_empty() {
        let dir = test_dir("pending");
        let pending = PendingDeletes::new(dir.join("pending-deletes"));
        let (a, b) = (dir.join(hidden_name()), dir.join(hidden_name()));
        assert_ne!(a, b);
        pending.add(&a).unwrap();
        pending.add(&b).unwrap();
        pending.add(&a).unwrap();
        assert_eq!(pending.load(), [a.clone(), b.clone()]);
        pending.remove(&a);
        assert_eq!(pending.load(), std::slice::from_ref(&b));
        pending.remove(&b);
        assert!(!dir.join("pending-deletes").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn many_are_added_and_removed_in_one_go() {
        let dir = test_dir("pending-many");
        let pending = PendingDeletes::new(dir.join("pending-deletes"));
        let (a, b, c) = (dir.join(hidden_name()), dir.join(hidden_name()), dir.join(hidden_name()));
        pending.add(&b).unwrap();
        pending.add_all(&[&a, &b, &c, &a]).unwrap();
        assert_eq!(pending.load(), [b.clone(), a.clone(), c.clone()], "each once, a noted one keeps its place");
        pending.remove_all(&[&a, &c]);
        assert_eq!(pending.load(), std::slice::from_ref(&b));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_delete_note_names_its_process_and_an_old_plain_one_still_counts() {
        let dir = test_dir("pending-owner");
        let file = dir.join("pending-deletes");
        let pending = PendingDeletes::new(file.clone());
        let (mine, old) = (dir.join(hidden_name()), dir.join(hidden_name()));
        pending.add(&mine).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert_eq!(text, format!("{DELETING}{}\t{}\n", std::process::id(), mine.display()));
        std::fs::write(&file, format!("{text}{}\n", old.display())).unwrap();
        assert_eq!(pending.load(), [mine.clone(), old.clone()]);
        // This process noted `mine` and runs; another start must leave it to this one.
        assert_eq!(pending.load_unowned(), std::slice::from_ref(&old));
        pending.remove(&mine);
        pending.remove(&old);
        assert!(!file.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_restore_note_replaces_the_delete_note() {
        let dir = test_dir("pending-restore");
        let pending = PendingDeletes::new(dir.join("pending-deletes"));
        let hidden = dir.join(hidden_name());
        pending.add(&hidden).unwrap();
        pending.add_restore(&Restore { hidden: hidden.clone(), original: dir.join("Photos"), was_hidden: true });
        assert!(pending.load().is_empty(), "never deleted later");
        let restores = pending.restores();
        assert_eq!(restores, [Restore { hidden: hidden.clone(), original: dir.join("Photos"), was_hidden: true }]);
        pending.remove_restore(&hidden);
        assert!(pending.restores().is_empty());
        assert!(!dir.join("pending-deletes").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_restore_note_only_renames_within_its_folder() {
        let dir = test_dir("pending-restore-foreign");
        let file = dir.join("pending-deletes");
        let hidden = dir.join(format!("{HIDDEN_PREFIX}abc"));
        let text = format!(
            "{RESTORE}{}\t{}\t0\n{RESTORE}{}\t{}\t0\n{RESTORE}{}\t{}\t1\n",
            hidden.display(),
            dir.join("elsewhere").join("x").display(),
            dir.join("Documents").display(),
            dir.join("y").display(),
            hidden.display(),
            dir.join("z").display(),
        );
        std::fs::write(&file, text).unwrap();
        assert_eq!(
            PendingDeletes::new(file).restores(),
            [Restore { hidden, original: dir.join("z"), was_hidden: true }]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn copy_notes_are_added_once_and_removed_by_prefix() {
        let dir = test_dir("pending-copies");
        let pending = PendingDeletes::new(dir.join("pending-deletes"));
        let (mine, other) = (copy_prefix(), copy_prefix());
        assert_ne!(mine, other);
        pending.add_copies(7, &dir.join("a"), &mine);
        pending.add_copies(7, &dir.join("a"), &mine);
        pending.add_copies(7, &dir.join("b"), &mine);
        pending.add_copies(8, &dir.join("a"), &other);
        let note = |pid, folder: &str, prefix: &str| CopyNote { pid, folder: dir.join(folder), prefix: prefix.into() };
        assert_eq!(pending.copies(), [note(7, "a", &mine), note(7, "b", &mine), note(8, "a", &other)]);
        assert!(pending.load().is_empty(), "nothing to delete as a whole");
        pending.remove_copies(&mine);
        assert_eq!(pending.copies(), [note(8, "a", &other)]);
        pending.remove_copies(&other);
        assert!(!dir.join("pending-deletes").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_copy_note_needs_a_gezik_prefix_and_an_absolute_folder() {
        let dir = test_dir("pending-copies-foreign");
        let file = dir.join("pending-deletes");
        let good = format!("{COPYING_PREFIX}abc-");
        let text = format!(
            "{COPIES}1\t{}\t\n{COPIES}1\t{}\tdoc\n{COPIES}1\trelative\t{good}\n{COPIES}x\t{}\t{good}\n{COPIES}1\t{}\t{COPYING_PREFIX}a\\b\n{COPIES}2\t{}\t{good}\n",
            dir.display(),
            dir.display(),
            dir.display(),
            dir.display(),
            dir.display(),
        );
        std::fs::write(&file, text).unwrap();
        assert_eq!(PendingDeletes::new(file).copies(), [CopyNote { pid: 2, folder: dir.clone(), prefix: good }]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_gezik_hidden_folders_are_loaded() {
        let dir = test_dir("pending-foreign");
        let file = dir.join("pending-deletes");
        let ours = dir.join(format!("{HIDDEN_PREFIX}abc"));
        let text = format!(
            "{}\n{}\n{HIDDEN_PREFIX}relative\n\n{}\n",
            dir.join("Documents").display(),
            ours.display(),
            dir.join(HIDDEN_PREFIX).display()
        );
        std::fs::write(&file, text).unwrap();
        assert_eq!(PendingDeletes::new(file).load(), [ours]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
