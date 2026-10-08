//! What the active tab shows: a folder's entries, the drives ("This PC") or a search's results.

use std::borrow::Cow;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use gezik_core::Entry;
use gezik_core::kind::Kind;
use gezik_core::pattern::{Pattern, matching_rows};
use gezik_platform::Drive;
use gezik_search::results::ResultSet;

pub enum Listing {
    Files(PathBuf, Rc<Vec<Entry>>),
    Drives(Vec<Drive>),
    /// A search's or the flat view's results (spec 3.7): no folder of its own.
    Results(Arc<ResultSet>),
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
            Listing::Results(set) => set.len(),
        }
    }

    /// Entry `index` of a folder or of the results (none for drives).
    pub fn entry(&self, index: usize) -> Option<&Entry> {
        match self {
            Listing::Files(_, entries) => entries.get(index),
            Listing::Results(set) => set.entry(index),
            Listing::Drives(_) => None,
        }
    }

    pub fn name_at(&self, index: usize) -> Option<&str> {
        match self {
            Listing::Drives(drives) => drives.get(index).map(|d| d.label.as_str()),
            _ => self.entry(index).map(|e| e.name.as_str()),
        }
    }

    /// What the history keeps of a row: its name, or in the results its path under the scope
    /// (names repeat there, spec 3.7).
    pub fn key_at(&self, index: usize) -> Option<Cow<'_, str>> {
        match self {
            Listing::Results(set) => set.key_at(index).map(Cow::Owned),
            _ => self.name_at(index).map(Cow::Borrowed),
        }
    }

    /// The row whose key is `key`.
    pub fn index_of(&self, key: &str) -> Option<usize> {
        match self {
            Listing::Results(set) => {
                let name = key.rsplit(std::path::MAIN_SEPARATOR).next().unwrap_or(key);
                (0..set.len()).find(|&i| self.name_at(i) == Some(name) && set.key_at(i).as_deref() == Some(key))
            }
            _ => (0..self.len()).find(|&i| self.name_at(i) == Some(key)),
        }
    }

    /// Where each of `keys` is, ascending; keys not found are skipped. One pass, so restoring
    /// thousands of selected names stays fast in a large folder; in the results a key is built
    /// only for a row whose name is one of theirs.
    pub fn indices_of(&self, keys: &[String]) -> Vec<usize> {
        if keys.is_empty() {
            return Vec::new();
        }
        let wanted: HashSet<&str> = keys.iter().map(String::as_str).collect();
        match self {
            Listing::Results(set) => {
                let names: HashSet<&str> =
                    keys.iter().map(|k| k.rsplit(std::path::MAIN_SEPARATOR).next().unwrap_or(k)).collect();
                (0..set.len())
                    .filter(|&i| self.name_at(i).is_some_and(|n| names.contains(n)))
                    .filter(|&i| set.key_at(i).is_some_and(|k| wanted.contains(k.as_str())))
                    .collect()
            }
            _ => (0..self.len()).filter(|&i| self.name_at(i).is_some_and(|n| wanted.contains(n))).collect(),
        }
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
            Listing::Results(set) => set.path_at(index).zip(set.entry(index).map(|e| e.is_dir)),
        }
    }

    /// The folder listed; `None` for the drives, the results and the empty listing.
    pub fn folder(&self) -> Option<&Path> {
        match self {
            Listing::Files(dir, _) if !dir.as_os_str().is_empty() => Some(dir),
            _ => None,
        }
    }

    pub fn is_dir(&self, index: usize) -> bool {
        match self {
            Listing::Drives(_) => true,
            _ => self.entry(index).is_some_and(|e| e.is_dir),
        }
    }

    /// A file's size; 0 for folders and drives.
    pub fn file_size(&self, index: usize) -> u64 {
        self.entry(index).filter(|e| !e.is_dir).map_or(0, |e| e.size)
    }

    /// Without what `[view]` hides: dot names and hidden items unless `show_hidden`, protected
    /// system items unless `show_system` (`Entry::is_shown`). Shares the entries when nothing
    /// is left out. The results stay as they are: the scanner followed the rule.
    pub fn without_hidden(self, show_hidden: bool, show_system: bool) -> Listing {
        match self {
            Listing::Files(dir, entries) if entries.iter().any(|e| !e.is_shown(show_hidden, show_system)) => {
                let kept = entries.iter().filter(|e| e.is_shown(show_hidden, show_system)).cloned().collect();
                Listing::Files(dir, Rc::new(kept))
            }
            other => other,
        }
    }

    /// Whether entries `a` and `b` are of one type: both folders, or files with the same
    /// ending (ignoring case; no ending is a type too). Drives are all one type.
    pub fn is_same_type(&self, a: usize, b: usize) -> bool {
        if let Listing::Drives(drives) = self {
            return a < drives.len() && b < drives.len();
        }
        match (self.entry(a), self.entry(b)) {
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
        }
    }

    pub fn kind(&self, index: usize) -> Kind {
        match self {
            Listing::Drives(_) => Kind::Folder,
            _ => self.entry(index).map_or(Kind::File, |e| Kind::of(&e.name, e.is_dir)),
        }
    }
}

/// The results `pattern` lets through of `full`, and where each is in it: the same set (no
/// copy, `None`) for an empty pattern ("search within results", spec 4.5).
pub fn filtered_results(full: &Arc<ResultSet>, pattern: &Pattern) -> (Listing, Option<Vec<usize>>) {
    if pattern.is_empty() {
        return (Listing::Results(full.clone()), None);
    }
    let rows = matching_rows(full.entries(), pattern);
    (Listing::Results(Arc::new(full.subset(&rows))), Some(rows))
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
pub(crate) fn results(rows: &[(&str, &str)]) -> Listing {
    use gezik_search::results::{Batch, ResultSet};
    let mut folders: Vec<&str> = Vec::new();
    let mut batch = Batch::default();
    for (folder, name) in rows {
        let parent = match folders.iter().position(|f| f == folder) {
            Some(i) => i,
            None => {
                folders.push(folder);
                batch.folders.push((*folder).into());
                folders.len() - 1
            }
        };
        batch.entries.push(Entry {
            name: (*name).to_owned(),
            is_dir: name.ends_with('/'),
            flags: 0,
            size: 10,
            modified: None,
            created: None,
        });
        batch.parent.push(parent as u32);
        batch.matches.push(None);
    }
    let mut set = ResultSet::new(PathBuf::from("/w"), false);
    set.append(batch);
    Listing::Results(Arc::new(set))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_results_with_one_name_keep_their_own_selection() {
        let sep = std::path::MAIN_SEPARATOR;
        let listing = results(&[("a", "x.txt"), ("b", "x.txt"), ("", "y.txt")]);
        let second = format!("b{sep}x.txt");
        assert_eq!(listing.key_at(1).as_deref(), Some(second.as_str()));
        assert_eq!(listing.key_at(2).as_deref(), Some("y.txt"));
        assert_eq!(listing.index_of(&second), Some(1));
        assert_eq!(
            listing.indices_of(&[second.clone(), "y.txt".into(), "x.txt".into()]),
            [1, 2],
            "a bare name is no key here"
        );
        assert_eq!(listing.name_at(1), Some("x.txt"));
        assert_eq!(listing.path_at(1), Some((PathBuf::from("/w").join("b").join("x.txt"), false)));
        assert_eq!(listing.folder(), None, "no folder: nothing goes \"here\"");
        assert_eq!(listing.find_prefix("y"), Some(2), "type-ahead goes by the name");
        assert!(listing.is_same_type(0, 1));
    }

    #[test]
    fn filtered_results_share_the_set_without_a_pattern() {
        let Listing::Results(full) = results(&[("a", "x.jpg"), ("b", "y.txt")]) else { unreachable!() };
        let (all, rows) = filtered_results(&full, &Pattern::default());
        assert!(matches!(&all, Listing::Results(set) if Arc::ptr_eq(set, &full)) && rows.is_none());
        let (jpgs, rows) = filtered_results(&full, &Pattern::compile("*.jpg").unwrap());
        assert_eq!((jpgs.len(), rows), (1, Some(vec![0])));
    }

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
    fn hidden_and_protected_items_can_be_left_out() {
        let listing = || {
            let Listing::Files(dir, entries) =
                files("/x", &[".DS_Store", ".git/", "a.txt", "b/", "desktop.ini", "notes.txt"])
            else {
                unreachable!()
            };
            let mut entries = Rc::unwrap_or_clone(entries);
            entries[4].flags = Entry::HIDDEN | Entry::SYSTEM;
            entries[5].flags = Entry::HIDDEN;
            Listing::Files(dir, Rc::new(entries))
        };
        let names = |l: Listing| (0..l.len()).filter_map(|i| l.name_at(i).map(str::to_owned)).collect::<Vec<_>>();
        assert_eq!(names(listing().without_hidden(false, false)), ["a.txt", "b/"]);
        assert_eq!(names(listing().without_hidden(true, false)), [".DS_Store", ".git/", "a.txt", "b/", "notes.txt"]);
        assert_eq!(names(listing().without_hidden(false, true)), ["a.txt", "b/", "desktop.ini"]);
        assert_eq!(names(listing().without_hidden(true, true)).len(), 6);
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
