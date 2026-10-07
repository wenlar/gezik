//! What the active tab shows: a folder's entries or the drives ("This PC").

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gezik_core::Entry;
use gezik_core::kind::Kind;
use gezik_core::pattern::{Pattern, matching_rows};
use gezik_platform::Drive;

pub enum Listing {
    Files(PathBuf, Rc<Vec<Entry>>),
    Drives(Vec<Drive>),
}

impl Default for Listing {
    fn default() -> Self {
        Listing::Files(PathBuf::new(), Rc::default())
    }
}

impl Listing {
    pub fn len(&self) -> usize {
        match self {
            Listing::Files(_, entries) => entries.len(),
            Listing::Drives(drives) => drives.len(),
        }
    }

    pub fn name_at(&self, index: usize) -> Option<&str> {
        match self {
            Listing::Files(_, entries) => entries.get(index).map(|e| e.name.as_str()),
            Listing::Drives(drives) => drives.get(index).map(|d| d.label.as_str()),
        }
    }

    pub fn index_of(&self, name: &str) -> Option<usize> {
        (0..self.len()).find(|&i| self.name_at(i) == Some(name))
    }

    /// Where each of `names` is, ascending; names not found are skipped. One pass, so
    /// restoring thousands of selected names stays fast in a large folder.
    pub fn indices_of(&self, names: &[String]) -> Vec<usize> {
        if names.is_empty() {
            return Vec::new();
        }
        let wanted: HashSet<&str> = names.iter().map(String::as_str).collect();
        (0..self.len()).filter(|&i| self.name_at(i).is_some_and(|n| wanted.contains(n))).collect()
    }

    /// The first entry whose name starts with `typed` (lowercase), ignoring case; no
    /// allocation per entry, so it stays fast in a folder of 100k files.
    pub fn find_prefix(&self, typed: &str) -> Option<usize> {
        (0..self.len()).find(|&i| self.name_at(i).is_some_and(|n| crate::keys::starts_with_lowercase(n, typed)))
    }

    /// Path of entry `index` and whether it is a folder (drives count as folders).
    pub fn path_at(&self, index: usize) -> Option<(PathBuf, bool)> {
        match self {
            Listing::Files(dir, entries) => entries.get(index).map(|e| (dir.join(&e.name), e.is_dir)),
            Listing::Drives(drives) => drives.get(index).map(|d| (d.path.clone(), true)),
        }
    }

    /// The folder listed; `None` for the drives and the empty listing.
    pub fn folder(&self) -> Option<&Path> {
        match self {
            Listing::Files(dir, _) if !dir.as_os_str().is_empty() => Some(dir),
            _ => None,
        }
    }

    pub fn is_dir(&self, index: usize) -> bool {
        match self {
            Listing::Files(_, entries) => entries.get(index).is_some_and(|e| e.is_dir),
            Listing::Drives(_) => true,
        }
    }

    /// A file's size; 0 for folders and drives.
    pub fn file_size(&self, index: usize) -> u64 {
        match self {
            Listing::Files(_, entries) => entries.get(index).filter(|e| !e.is_dir).map_or(0, |e| e.size),
            Listing::Drives(_) => 0,
        }
    }

    /// Without the files whose names start with a dot (`.DS_Store`, `.git`).
    pub fn without_dotfiles(self) -> Listing {
        match self {
            Listing::Files(dir, entries) if entries.iter().any(|e| e.name.starts_with('.')) => {
                let kept = entries.iter().filter(|e| !e.name.starts_with('.')).cloned().collect();
                Listing::Files(dir, Rc::new(kept))
            }
            other => other,
        }
    }

    /// Whether entries `a` and `b` are of one type: both folders, or files with the same
    /// ending (ignoring case; no ending is a type too). Drives are all one type.
    pub fn is_same_type(&self, a: usize, b: usize) -> bool {
        match self {
            Listing::Files(_, entries) => match (entries.get(a), entries.get(b)) {
                (Some(a), Some(b)) => match (a.is_dir, b.is_dir) {
                    (true, true) => true,
                    // Character by character: no allocation per entry.
                    (false, false) => a
                        .extension()
                        .chars()
                        .flat_map(char::to_lowercase)
                        .eq(b.extension().chars().flat_map(char::to_lowercase)),
                    _ => false,
                },
                _ => false,
            },
            Listing::Drives(drives) => a < drives.len() && b < drives.len(),
        }
    }

    pub fn kind(&self, index: usize) -> Kind {
        match self {
            Listing::Files(_, entries) => entries.get(index).map_or(Kind::File, |e| Kind::of(&e.name, e.is_dir)),
            Listing::Drives(_) => Kind::Folder,
        }
    }
}

/// The listing of `dir` showing what `pattern` lets through of `full`, and where each of its
/// entries is in `full`: the same entries (no copy, `None`) for an empty pattern.
pub fn filtered_listing(dir: &Path, full: &Rc<Vec<Entry>>, pattern: &Pattern) -> (Listing, Option<Vec<usize>>) {
    if pattern.is_empty() {
        (Listing::Files(dir.to_path_buf(), full.clone()), None)
    } else {
        let rows = matching_rows(full, pattern);
        let entries = rows.iter().map(|&i| full[i].clone()).collect();
        (Listing::Files(dir.to_path_buf(), Rc::new(entries)), Some(rows))
    }
}

/// Whether an entry of `entries` other than `except` is called `name` (ignoring case where
/// the file system does).
pub fn name_taken(entries: &[Entry], name: &str, except: &str) -> bool {
    let fold = cfg!(any(windows, target_os = "macos"));
    entries.iter().filter(|e| e.name != except).any(|e| {
        if fold {
            // Character by character: no allocation per entry.
            e.name.chars().flat_map(char::to_lowercase).eq(name.chars().flat_map(char::to_lowercase))
        } else {
            e.name == name
        }
    })
}

#[cfg(test)]
pub(crate) fn files(dir: &str, names: &[&str]) -> Listing {
    let entries = names
        .iter()
        .map(|n| Entry {
            name: (*n).to_owned(),
            is_dir: n.ends_with('/'),
            flags: 0,
            size: 10,
            modified: None,
            created: None,
        })
        .collect();
    Listing::Files(PathBuf::from(dir), Rc::new(entries))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_filtered_listing_shares_the_entries_without_a_pattern() {
        let Listing::Files(_, full) = files("/x", &["a.jpg", "b.txt", "c.JPG"]) else { unreachable!() };
        let (all, rows) = filtered_listing(Path::new("/x"), &full, &Pattern::default());
        assert!(matches!(&all, Listing::Files(_, shown) if Rc::ptr_eq(shown, &full)), "no copy");
        assert_eq!(rows, None);
        let (jpgs, rows) = filtered_listing(Path::new("/x"), &full, &Pattern::compile("*.jpg").unwrap());
        assert_eq!(rows, Some(vec![0, 2]), "where they are in the full list");
        let names: Vec<&str> = (0..jpgs.len()).filter_map(|i| jpgs.name_at(i)).collect();
        assert_eq!(names, ["a.jpg", "c.JPG"]);
        assert_eq!(jpgs.folder(), Some(Path::new("/x")));
    }

    #[test]
    fn a_hidden_name_is_still_taken() {
        let Listing::Files(_, full) = files("/x", &["a.txt", "b.txt"]) else { unreachable!() };
        assert!(name_taken(&full, "b.txt", "a.txt"), "b.txt is filtered out, but it is there");
        assert!(!name_taken(&full, "a.txt", "a.txt"), "its own name");
        assert!(!name_taken(&full, "c.txt", "a.txt"));
        assert_eq!(name_taken(&full, "B.TXT", "a.txt"), cfg!(any(windows, target_os = "macos")));
    }

    #[test]
    fn dotfiles_can_be_left_out() {
        let listing = files("/x", &[".DS_Store", ".git/", "a.txt", "b/"]).without_dotfiles();
        let names: Vec<&str> = (0..listing.len()).filter_map(|i| listing.name_at(i)).collect();
        assert_eq!(names, ["a.txt", "b/"]);
        assert_eq!(listing.folder(), Some(Path::new("/x")));
    }

    #[test]
    fn type_ahead_search_ignores_case() {
        let listing = files("/x", &["Apple", "Banana", "bandit", "İndir", "şablon"]);
        assert_eq!(listing.find_prefix("ban"), Some(1));
        assert_eq!(listing.find_prefix("band"), Some(2));
        assert_eq!(listing.find_prefix("ş"), Some(4));
        assert_eq!(listing.find_prefix(&"İ".to_lowercase()), Some(3));
        assert_eq!(listing.find_prefix("z"), None);
        assert_eq!(listing.find_prefix("applesauce"), None);
    }

    #[test]
    fn finds_many_names_in_one_pass() {
        let listing = files("/x", &["a", "b", "c", "d"]);
        assert_eq!(listing.indices_of(&["d".into(), "gone".into(), "b".into()]), [1, 3]);
        assert!(listing.indices_of(&[]).is_empty());
    }

    #[test]
    fn same_type_goes_by_the_ending_or_folders() {
        let listing = files("/x", &["sub/", "other/", "a.JPG", "b.jpg", "c.png", "README", "LICENSE"]);
        assert!(listing.is_same_type(0, 1), "two folders");
        assert!(!listing.is_same_type(0, 2));
        assert!(listing.is_same_type(2, 3), "endings ignore case");
        assert!(!listing.is_same_type(2, 4));
        assert!(listing.is_same_type(5, 6), "no ending is a type too");
        assert!(!listing.is_same_type(2, 99));
    }

    #[test]
    fn paths_sizes_and_folders() {
        let listing = files("/x", &["sub/", "f.txt"]);
        assert_eq!(listing.path_at(1), Some((PathBuf::from("/x").join("f.txt"), false)));
        assert!(listing.is_dir(0) && !listing.is_dir(1));
        assert_eq!((listing.file_size(0), listing.file_size(1)), (0, 10));
        assert_eq!(listing.folder(), Some(Path::new("/x")));
        assert_eq!(Listing::default().folder(), None);
        assert_eq!(listing.kind(1), Kind::Text);
    }
}
