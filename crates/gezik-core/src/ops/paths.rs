//! Comparing paths the way the file system does, and which drives a job touches.

use std::path::{Component, Path};

/// Windows and macOS file systems ignore case by default.
const IGNORE_CASE: bool = cfg!(any(windows, target_os = "macos"));

fn parts(path: &Path) -> Vec<String> {
    path.components()
        .filter(|c| !matches!(c, Component::CurDir))
        .map(|c| {
            let text = c.as_os_str().to_string_lossy();
            if IGNORE_CASE { text.to_lowercase() } else { text.into_owned() }
        })
        .collect()
}

/// Whether `a` and `b` name the same entry (case-insensitive on Windows and macOS; a trailing
/// separator does not matter).
pub fn same_path(a: &Path, b: &Path) -> bool {
    parts(a) == parts(b)
}

/// The roots (`c:\`, `\\server\share\`, `/`) of `paths`, read from the text alone: known at
/// once, even when the drive behind a root does not answer (a lost network drive).
pub fn lexical_roots<'a>(paths: impl IntoIterator<Item = &'a Path>) -> DriveSet {
    DriveSet::new(paths.into_iter().map(|path| {
        let root: String = path
            .components()
            .take_while(|c| matches!(c, Component::Prefix(_) | Component::RootDir))
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        if IGNORE_CASE { root.to_lowercase() } else { root }
    }))
}

/// Whether `path` is `dir` itself or inside it.
pub fn is_within(path: &Path, dir: &Path) -> bool {
    let (path, dir) = (parts(path), parts(dir));
    path.len() >= dir.len() && path[..dir.len()] == dir[..]
}

/// The drives (volumes) a job touches, by id. Two jobs that share a drive wait for each other.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DriveSet(Vec<String>);

impl DriveSet {
    pub fn new(ids: impl IntoIterator<Item = String>) -> DriveSet {
        let mut ids: Vec<String> = ids.into_iter().collect();
        ids.sort();
        ids.dedup();
        DriveSet(ids)
    }

    pub fn intersects(&self, other: &DriveSet) -> bool {
        self.0.iter().any(|id| other.0.contains(id))
    }

    pub fn ids(&self) -> &[String] {
        &self.0
    }
}

/// The items not inside another item's path: trashing a created folder covers what is in it.
pub fn cover<T>(mut items: Vec<T>, path: impl Fn(&T) -> &Path) -> Vec<T> {
    // A parent sorts before what is inside it.
    items.sort_by_cached_key(|item| parts(path(item)));
    let mut kept: Vec<T> = Vec::new();
    for item in items {
        let inside = kept.last().is_some_and(|last| is_within(path(&item), path(last)));
        if !inside {
            kept.push(item);
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn roots_come_from_the_path_text() {
        let roots = lexical_roots([Path::new("/a/b"), Path::new("/c")]);
        assert_eq!(roots.ids(), [std::path::MAIN_SEPARATOR_STR]);
        if cfg!(windows) {
            let roots = lexical_roots([
                Path::new(r"C:\Users\a"),
                Path::new(r"c:\temp"),
                Path::new(r"\\Server\Share\x"),
                Path::new(r"Z:\"),
            ]);
            assert_eq!(roots.ids(), [r"\\server\share\", r"c:\", r"z:\"]);
            assert!(!roots.intersects(&lexical_roots([Path::new(r"D:\x")])));
        }
    }

    #[test]
    fn within_compares_whole_parts() {
        assert!(is_within(Path::new("/a/b/c"), Path::new("/a/b")));
        assert!(is_within(Path::new("/a/b"), Path::new("/a/b/")));
        assert!(!is_within(Path::new("/a/bc"), Path::new("/a/b")));
        assert!(!is_within(Path::new("/a"), Path::new("/a/b")));
        assert!(same_path(Path::new("/a/./b"), Path::new("/a/b/")));
    }

    #[cfg(windows)]
    #[test]
    fn windows_ignores_case_and_separators() {
        assert!(same_path(Path::new(r"C:\Users\A"), Path::new("c:/users/a/")));
        assert!(is_within(Path::new(r"C:\Data\Sub"), Path::new(r"c:\data")));
    }

    #[test]
    fn drive_sets_meet_on_a_shared_drive() {
        let c = DriveSet::new(["c".to_owned()]);
        let cd = DriveSet::new(["d".to_owned(), "c".to_owned(), "c".to_owned()]);
        let e = DriveSet::new(["e".to_owned()]);
        assert!(c.intersects(&cd) && cd.intersects(&c));
        assert!(!c.intersects(&e));
        assert_eq!(cd.ids(), ["c", "d"]);
    }

    #[test]
    fn cover_keeps_only_the_outermost() {
        let items = vec![PathBuf::from("/x/a/b"), PathBuf::from("/x/a"), PathBuf::from("/x/c"), PathBuf::from("/x/ab")];
        let kept = cover(items, |p| p.as_path());
        assert_eq!(kept, [PathBuf::from("/x/a"), PathBuf::from("/x/ab"), PathBuf::from("/x/c")]);
    }
}
