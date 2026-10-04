//! `pending-deletes`: the hidden folders an instant delete has not finished, one path per
//! line. Read at start, so a delete cut short (Gezik closed or crashed) finishes later.

use std::hash::{BuildHasher, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::engine::lock;

/// How the folders an instant delete hides are named.
pub const HIDDEN_PREFIX: &str = ".gezik-deleting-";

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
