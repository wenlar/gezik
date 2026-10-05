//! `pending-deletes`: the hidden folders an instant delete has not finished, one path per
//! line. Read at start, so a delete cut short (Gezik closed or crashed) finishes later. A
//! line starting with `restore` and a tab is instead a hidden folder that could not be put
//! back under its own name (something held it open): at start it is put back, not deleted.

use std::hash::{BuildHasher, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::engine::lock;

/// How the folders an instant delete hides are named.
pub const HIDDEN_PREFIX: &str = ".gezik-deleting-";

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
        let _guard = lock(&self.guard);
        let mut list = self.read();
        if !list.iter().any(|p| p == path) {
            list.push(path.to_path_buf());
        }
        self.write(&list)
    }

    pub fn remove(&self, path: &Path) {
        let _guard = lock(&self.guard);
        let mut list = self.read();
        let before = list.len();
        list.retain(|p| p != path);
        if list.len() != before {
            let _ = self.write(&list);
        }
    }

    /// `restore.hidden` is to be put back, never deleted: its delete note becomes a restore
    /// note.
    pub fn add_restore(&self, restore: &Restore) {
        let _guard = lock(&self.guard);
        let mut list = self.read();
        list.retain(|p| p != &restore.hidden && restore_of(p).is_none_or(|r| r.hidden != restore.hidden));
        list.push(PathBuf::from(format!(
            "{RESTORE}{}\t{}\t{}",
            restore.hidden.display(),
            restore.original.display(),
            u8::from(restore.was_hidden)
        )));
        let _ = self.write(&list);
    }

    pub fn remove_restore(&self, hidden: &Path) {
        let _guard = lock(&self.guard);
        let mut list = self.read();
        let before = list.len();
        list.retain(|p| restore_of(p).is_none_or(|r| r.hidden != hidden));
        if list.len() != before {
            let _ = self.write(&list);
        }
    }

    /// The listed restores of folders Gezik hid, each back into the folder it is in.
    pub fn restores(&self) -> Vec<Restore> {
        let _guard = lock(&self.guard);
        self.read().iter().filter_map(|line| restore_of(line)).collect()
    }

    /// The listed folders Gezik itself hid; anything else in the file is ignored, so a damaged
    /// or edited file can never make Gezik delete other things.
    pub fn load(&self) -> Vec<PathBuf> {
        let _guard = lock(&self.guard);
        self.read().into_iter().filter(|path| is_hidden(path)).collect()
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

/// Whether `path` is an absolute path to a folder an instant delete hid.
pub fn is_hidden(path: &Path) -> bool {
    path.is_absolute()
        && path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.len() > HIDDEN_PREFIX.len() && name.starts_with(HIDDEN_PREFIX))
}

/// A fresh hidden name.
pub fn hidden_name() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
    hasher.write_u64(nanos);
    hasher.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    format!("{HIDDEN_PREFIX}{:016x}", hasher.finish())
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
