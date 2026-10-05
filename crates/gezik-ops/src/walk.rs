//! Walking a folder tree: a folder before what is in it; links are not followed.

use std::fs::Metadata;
use std::io;
use std::path::{Path, PathBuf};

use gezik_core::ops::conflict::Facts;

/// Facts from `symlink_metadata` (a link is neither a folder nor sized).
pub fn facts_of(meta: &Metadata) -> Facts {
    Facts { is_dir: meta.is_dir(), size: if meta.is_file() { meta.len() } else { 0 }, modified: meta.modified().ok() }
}

/// What [`walk`] reports.
pub enum Step<'a> {
    /// An entry: its full path, its path relative to the walked folder, its facts.
    Entry {
        path: &'a Path,
        relative: &'a Path,
        facts: Facts,
    },
    Failed {
        path: &'a Path,
        error: io::Error,
    },
}

/// Calls `visit` for everything inside `dir` (not `dir` itself), each folder before its
/// contents. `visit` returns false to stop; `walk` then returns false.
pub fn walk(dir: &Path, visit: &mut dyn FnMut(Step<'_>) -> bool) -> bool {
    // Folders still to read, relative to `dir`.
    let mut pending: Vec<PathBuf> = vec![PathBuf::new()];
    while let Some(relative_dir) = pending.pop() {
        let absolute = dir.join(&relative_dir);
        let entries = match std::fs::read_dir(&absolute) {
            Ok(entries) => entries,
            Err(error) => {
                if !visit(Step::Failed { path: &absolute, error }) {
                    return false;
                }
                continue;
            }
        };
        let mut folders = Vec::new();
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    if !visit(Step::Failed { path: &absolute, error }) {
                        return false;
                    }
                    continue;
                }
            };
            let path = entry.path();
            let relative = relative_dir.join(entry.file_name());
            // DirEntry::metadata does not follow links.
            let meta = match entry.metadata() {
                Ok(meta) => meta,
                Err(error) => {
                    if !visit(Step::Failed { path: &path, error }) {
                        return false;
                    }
                    continue;
                }
            };
            let facts = facts_of(&meta);
            if !visit(Step::Entry { path: &path, relative: &relative, facts }) {
                return false;
            }
            if facts.is_dir {
                folders.push(relative);
            }
        }
        // Depth first: the first folder is read next.
        pending.extend(folders.into_iter().rev());
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{test_dir, write};

    #[test]
    fn parents_come_before_their_contents() {
        let dir = test_dir("walk");
        write(&dir.join("a/b/c.txt"), "c");
        write(&dir.join("a/d.txt"), "dd");
        write(&dir.join("e.txt"), "eee");
        let mut seen: Vec<(PathBuf, bool, u64)> = Vec::new();
        assert!(walk(&dir, &mut |step| {
            if let Step::Entry { relative, facts, .. } = step {
                seen.push((relative.to_path_buf(), facts.is_dir, facts.size));
            }
            true
        }));
        let position = |p: &str| seen.iter().position(|(r, ..)| r == Path::new(p)).unwrap();
        assert_eq!(seen.len(), 5);
        assert!(position("a") < position("a/b") && position("a/b") < position("a/b/c.txt"));
        assert!(position("a") < position("a/d.txt"));
        assert_eq!(seen[position("e.txt")], (PathBuf::from("e.txt"), false, 3));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stops_when_asked() {
        let dir = test_dir("walk-stop");
        write(&dir.join("a.txt"), "a");
        write(&dir.join("b.txt"), "b");
        let mut count = 0;
        assert!(!walk(&dir, &mut |_| {
            count += 1;
            false
        }));
        assert_eq!(count, 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
