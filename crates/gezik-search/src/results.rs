//! The results of one search or flat view (spec 3.7): the entries, the folder each is in
//! (relative to the scope, each folder once), and a content search's matching lines. The
//! scanner sends them in batches; the list, its filter and the background sort share them.

use std::path::{MAIN_SEPARATOR, MAIN_SEPARATOR_STR, Path, PathBuf};

use gezik_core::Entry;
use gezik_core::sort::SortSpec;

use crate::content::Found;

/// What one batch adds (scanner → UI thread). Its folders get the next numbers, in order.
#[derive(Debug, Default)]
pub struct Batch {
    pub folders: Vec<Box<str>>,
    pub entries: Vec<Entry>,
    pub parent: Vec<u32>,
    pub matches: Vec<Option<Found>>,
}

impl Batch {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.folders.is_empty()
    }
}

#[derive(Debug, Clone, Default)]
pub struct ResultSet {
    /// The scope folder; empty for every drive (the folders are then whole paths).
    root: PathBuf,
    /// Folders under `root` (`""`: `root` itself), each once.
    folders: Vec<Box<str>>,
    entries: Vec<Entry>,
    /// `entries[i]`'s place in `folders`.
    parent: Vec<u32>,
    /// A content search's first matching line per entry; `None` without content.
    matches: Option<Vec<Option<Found>>>,
    /// The sort (and folders-first) the entries are in; `None` once new ones came unsorted.
    sorted: Option<(SortSpec, bool)>,
}

/// A result's key in the list (spec 3.7): its path under the scope.
pub fn relative_key(folder: &str, name: &str) -> String {
    if folder.is_empty() {
        name.to_owned()
    } else if folder.ends_with(['/', '\\']) {
        format!("{folder}{name}")
    } else {
        format!("{folder}{MAIN_SEPARATOR}{name}")
    }
}

impl ResultSet {
    /// Empty results under `root`; `content`: they carry matching lines.
    pub fn new(root: PathBuf, content: bool) -> ResultSet {
        ResultSet {
            root,
            folders: Vec::new(),
            entries: Vec::new(),
            parent: Vec::new(),
            matches: content.then(Vec::new),
            sorted: None,
        }
    }

    /// Empty results that will use these folders (the name cache's), no matching lines.
    pub(crate) fn with_folders(root: PathBuf, folders: Vec<Box<str>>) -> ResultSet {
        ResultSet { folders, ..ResultSet::new(root, false) }
    }

    pub(crate) fn push_entry(&mut self, entry: Entry, parent: u32) {
        self.sorted = None;
        self.entries.push(entry);
        self.parent.push(parent);
        if let Some(matches) = &mut self.matches {
            matches.push(None);
        }
    }

    /// All of it as one batch (a cache selection or an Everything answer, spec 4.3).
    pub fn into_batch(self) -> Batch {
        let count = self.entries.len();
        Batch {
            folders: self.folders,
            entries: self.entries,
            parent: self.parent,
            matches: self.matches.unwrap_or_else(|| vec![None; count]),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn entry(&self, i: usize) -> Option<&Entry> {
        self.entries.get(i)
    }

    /// The folder of entry `i` as the Folder column shows it.
    pub fn folder(&self, i: usize) -> Option<&str> {
        let parent = *self.parent.get(i)?;
        self.folders.get(parent as usize).map(|f| &**f)
    }

    pub fn found(&self, i: usize) -> Option<&Found> {
        self.matches.as_ref()?.get(i)?.as_ref()
    }

    /// A content search's results (the Match column).
    pub fn has_matches(&self) -> bool {
        self.matches.is_some()
    }

    pub fn path_at(&self, i: usize) -> Option<PathBuf> {
        let (folder, entry) = (self.folder(i)?, self.entries.get(i)?);
        Some(if folder.is_empty() { self.root.join(&entry.name) } else { self.root.join(folder).join(&entry.name) })
    }

    pub fn key_at(&self, i: usize) -> Option<String> {
        Some(relative_key(self.folder(i)?, &self.entries.get(i)?.name))
    }

    pub fn append(&mut self, batch: Batch) {
        debug_assert_eq!(batch.entries.len(), batch.parent.len());
        debug_assert!(self.matches.is_none() || batch.matches.len() == batch.entries.len());
        if !batch.entries.is_empty() {
            self.sorted = None;
        }
        self.folders.extend(batch.folders);
        self.entries.extend(batch.entries);
        self.parent.extend(batch.parent);
        if let Some(matches) = &mut self.matches {
            matches.extend(batch.matches);
        }
    }

    /// The sort and folders-first the entries are in, if they are sorted (`set_sorted_by`) and
    /// nothing came since.
    pub fn sorted_by(&self) -> Option<(SortSpec, bool)> {
        self.sorted
    }

    pub fn set_sorted_by(&mut self, sorted: Option<(SortSpec, bool)>) {
        self.sorted = sorted;
    }

    /// Puts the entries in `order` (`gezik_core::sort::apply_order`): folders and matching lines
    /// go with them.
    pub fn apply_order(&mut self, order: &[usize]) {
        gezik_core::sort::apply_order(&mut self.entries, order);
        gezik_core::sort::apply_order(&mut self.parent, order);
        if let Some(matches) = &mut self.matches {
            gezik_core::sort::apply_order(matches, order);
        }
    }

    /// Entries `rows` (the filter's), with all the folders.
    pub fn subset(&self, rows: &[usize]) -> ResultSet {
        ResultSet {
            root: self.root.clone(),
            folders: self.folders.clone(),
            entries: rows.iter().filter_map(|&i| self.entries.get(i).cloned()).collect(),
            parent: rows.iter().filter_map(|&i| self.parent.get(i).copied()).collect(),
            matches: self.matches.as_ref().map(|m| rows.iter().filter_map(|&i| m.get(i).cloned()).collect()),
            sorted: self.sorted,
        }
    }

    /// Adds rows `rows` of `from` (the whole set this one is a subset of, grown since), with the
    /// folders it got meanwhile.
    pub fn extend_rows(&mut self, from: &ResultSet, rows: &[usize]) {
        if from.folders.len() > self.folders.len() {
            self.folders.extend_from_slice(&from.folders[self.folders.len()..]);
        }
        for &i in rows {
            let (Some(entry), Some(&parent)) = (from.entries.get(i), from.parent.get(i)) else { continue };
            self.entries.push(entry.clone());
            self.parent.push(parent);
            if let Some(matches) = &mut self.matches {
                matches.push(from.found(i).cloned());
            }
        }
        if !rows.is_empty() {
            self.sorted = None;
        }
    }

    /// Where `paths` are among the entries, ascending: by name first, then the whole path (no
    /// path is built for an entry whose name is none of theirs).
    pub fn rows_of(&self, paths: &[PathBuf]) -> Vec<usize> {
        let names: std::collections::HashSet<&std::ffi::OsStr> = paths.iter().filter_map(|p| p.file_name()).collect();
        let wanted: std::collections::HashSet<&Path> = paths.iter().map(PathBuf::as_path).collect();
        (0..self.entries.len())
            .filter(|&i| names.contains(std::ffi::OsStr::new(&self.entries[i].name)))
            .filter(|&i| self.path_at(i).is_some_and(|path| wanted.contains(path.as_path())))
            .collect()
    }

    /// Takes entries `rows` (ascending) out.
    pub fn remove(&mut self, rows: &[usize]) {
        let mut gone = rows.iter().copied().peekable();
        let keep: Vec<bool> = (0..self.entries.len())
            .map(|i| {
                if gone.peek() == Some(&i) {
                    gone.next();
                    false
                } else {
                    true
                }
            })
            .collect();
        let mut index = 0;
        self.entries.retain(|_| (keep[index], index += 1).0);
        index = 0;
        self.parent.retain(|_| (keep[index], index += 1).0);
        if let Some(matches) = &mut self.matches {
            index = 0;
            matches.retain(|_| (keep[index], index += 1).0);
        }
    }

    /// The folder text and the name `path` has here; `None` outside the scope.
    fn place_of(&self, path: &Path) -> Option<(String, String)> {
        let name = path.file_name()?.to_string_lossy().into_owned();
        Some((folder_text(&self.root, path)?, name))
    }

    /// Adds `path` (under the scope) as a new entry, no matching line; false outside it.
    pub fn push(&mut self, path: &Path, entry: Entry) -> bool {
        let Some((folder, _)) = self.place_of(path) else { return false };
        let parent = match self.folders.iter().position(|f| **f == *folder) {
            Some(i) => i,
            None => {
                self.folders.push(folder.into());
                self.folders.len() - 1
            }
        };
        self.push_entry(entry, parent as u32);
        true
    }

    /// Where `path` is among the entries.
    pub fn index_of_path(&self, path: &Path) -> Option<usize> {
        let (folder, name) = self.place_of(path)?;
        let parent = self.folders.iter().position(|f| **f == *folder)? as u32;
        (0..self.entries.len()).find(|&i| self.parent[i] == parent && self.entries[i].name == name)
    }
}

/// The folder text `path`'s entry has under `root` (empty `root`: every drive, the whole
/// parent); `None` outside it.
pub fn folder_text(root: &Path, path: &Path) -> Option<String> {
    let parent = path.parent()?;
    if root.as_os_str().is_empty() {
        return Some(parent.display().to_string());
    }
    let rest = parent.strip_prefix(root).ok()?;
    Some(rest.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join(MAIN_SEPARATOR_STR))
}

impl ResultSet {
    /// Results from whole paths (Everything's answer), each folder numbered once.
    pub fn collect(
        root: PathBuf,
        content: bool,
        items: impl IntoIterator<Item = (PathBuf, Entry, Option<Found>)>,
    ) -> ResultSet {
        let mut set = ResultSet::new(root, content);
        let mut numbers: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
        for (path, entry, found) in items {
            let Some(folder) = folder_text(&set.root, &path) else { continue };
            let next = set.folders.len() as u32;
            let parent = *numbers.entry(folder).or_insert_with_key(|folder| {
                set.folders.push(folder.as_str().into());
                next
            });
            set.entries.push(entry);
            set.parent.push(parent);
            if let Some(matches) = &mut set.matches {
                matches.push(found);
            }
        }
        set
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::MAIN_SEPARATOR as SEP;

    fn entry(name: &str) -> Entry {
        Entry { name: name.to_owned(), is_dir: false, flags: 0, size: 1, modified: None, created: None }
    }

    fn sample() -> ResultSet {
        let mut set = ResultSet::new(PathBuf::from("/w"), true);
        set.append(Batch {
            folders: vec!["".into(), format!("a{SEP}b").into()],
            entries: vec![entry("x.txt"), entry("y.txt"), entry("x.txt")],
            parent: vec![0, 1, 1],
            matches: vec![None, Some((3, "y line".into())), None],
        });
        set
    }

    #[test]
    fn paths_and_keys_come_from_the_folder_and_the_name() {
        let set = sample();
        assert_eq!(set.len(), 3);
        assert_eq!(set.path_at(0), Some(PathBuf::from("/w").join("x.txt")));
        assert_eq!(set.path_at(2), Some(PathBuf::from("/w").join(format!("a{SEP}b")).join("x.txt")));
        assert_eq!(set.key_at(0).as_deref(), Some("x.txt"));
        assert_eq!(set.key_at(2), Some(format!("a{SEP}b{SEP}x.txt")), "names repeat, keys do not");
        assert_eq!(set.folder(1), Some(format!("a{SEP}b").as_str()));
        assert_eq!(set.found(1).map(|f| f.0), Some(3));
        assert!(set.has_matches());
        assert_eq!(set.path_at(3), None);
    }

    #[test]
    fn all_drives_keep_whole_folders() {
        let mut set = ResultSet::new(PathBuf::new(), false);
        let folder = std::env::temp_dir().display().to_string();
        set.append(Batch {
            folders: vec![folder.clone().into()],
            entries: vec![entry("x")],
            parent: vec![0],
            matches: vec![None],
        });
        assert_eq!(set.path_at(0), Some(PathBuf::from(&folder).join("x")));
        assert_eq!(set.folder(0), Some(folder.as_str()));
        assert!(!set.has_matches() && set.found(0).is_none());
    }

    #[test]
    fn an_order_moves_everything_together() {
        let mut set = sample();
        set.apply_order(&[2, 0, 1]);
        assert_eq!(set.key_at(0), Some(format!("a{SEP}b{SEP}x.txt")));
        assert_eq!(set.found(2).map(|f| f.0), Some(3), "the match follows its entry");
        assert_eq!(set.key_at(1).as_deref(), Some("x.txt"));
    }

    #[test]
    fn subsets_and_removals_keep_the_folders() {
        let set = sample();
        let sub = set.subset(&[1, 2]);
        assert_eq!(sub.len(), 2);
        assert_eq!(sub.key_at(0), Some(format!("a{SEP}b{SEP}y.txt")));
        assert_eq!(sub.found(0).map(|f| f.0), Some(3));
        let mut fewer = set.clone();
        fewer.remove(&[0, 2]);
        assert_eq!(fewer.len(), 1);
        assert_eq!(fewer.key_at(0), Some(format!("a{SEP}b{SEP}y.txt")));
    }

    #[test]
    fn a_path_under_the_root_can_be_added_and_found() {
        let mut set = sample();
        let new = PathBuf::from("/w").join("c").join("z.txt");
        assert!(set.push(&new, entry("z.txt")));
        assert_eq!(set.path_at(3), Some(new.clone()));
        assert_eq!(set.found(3), None);
        assert_eq!(set.index_of_path(&new), Some(3));
        assert!(set.push(&PathBuf::from("/w").join("c").join("q.txt"), entry("q.txt")), "the folder is known now");
        assert!(!set.push(Path::new("/elsewhere/z.txt"), entry("z.txt")), "outside the scope");
        assert_eq!(set.index_of_path(&PathBuf::from("/w").join("x.txt")), Some(0));
    }

    #[test]
    fn whole_paths_become_results() {
        let root = PathBuf::from("/w");
        let items = vec![
            (root.join("a").join("x.txt"), entry("x.txt"), None),
            (root.join("y.txt"), entry("y.txt"), Some((1, "line".into()))),
            (root.join("a").join("z.txt"), entry("z.txt"), None),
        ];
        let set = ResultSet::collect(root.clone(), true, items);
        assert_eq!(set.len(), 3);
        assert_eq!(set.path_at(2), Some(root.join("a").join("z.txt")));
        assert_eq!(set.folder(0), set.folder(2), "one folder, once");
        assert_eq!(set.found(1).map(|f| f.0), Some(1));
        assert_eq!(folder_text(&root, &root.join("a").join("b").join("q")), Some(format!("a{SEP}b")));
        assert_eq!(
            folder_text(Path::new(""), &PathBuf::from("/x").join("q")),
            Some(PathBuf::from("/x").display().to_string())
        );
        assert_eq!(folder_text(&root, Path::new("/elsewhere/q")), None);
    }

    #[test]
    fn a_subset_takes_new_rows_and_folders() {
        let mut full = sample();
        let mut shown = full.subset(&[2]);
        full.append(Batch {
            folders: vec!["c".into()],
            entries: vec![entry("n.txt")],
            parent: vec![2],
            matches: vec![Some((9, "n".into()))],
        });
        shown.extend_rows(&full, &[3]);
        assert_eq!(shown.len(), 2);
        assert_eq!(shown.key_at(1), Some(format!("c{SEP}n.txt")));
        assert_eq!(shown.found(1).map(|f| f.0), Some(9));
    }

    #[test]
    fn rows_are_found_by_their_paths() {
        let set = sample();
        let second = PathBuf::from("/w").join(format!("a{SEP}b")).join("x.txt");
        assert_eq!(set.rows_of(std::slice::from_ref(&second)), [2], "not the other x.txt");
        assert_eq!(set.rows_of(&[PathBuf::from("/w").join("x.txt"), second]), [0, 2]);
        assert!(set.rows_of(&[PathBuf::from("/w/none")]).is_empty());
    }

    #[test]
    fn new_entries_make_the_set_unsorted() {
        let mut set = sample();
        let by_size = (SortSpec { key: gezik_core::sort::SortKey::Size, dir: gezik_core::sort::SortDir::Asc }, true);
        set.set_sorted_by(Some(by_size));
        assert_eq!(set.subset(&[0]).sorted_by(), Some(by_size), "a part keeps the order");
        set.remove(&[0]);
        assert_eq!(set.sorted_by(), Some(by_size), "a removal keeps it too");
        set.append(Batch::default());
        assert_eq!(set.sorted_by(), Some(by_size), "an empty batch adds nothing");
        set.append(Batch { folders: vec![], entries: vec![entry("n")], parent: vec![0], matches: vec![None] });
        assert_eq!(set.sorted_by(), None);
        set.set_sorted_by(Some(by_size));
        assert!(set.push(&PathBuf::from("/w").join("z"), entry("z")));
        assert_eq!(set.sorted_by(), None);
    }

    #[test]
    fn an_entry_is_no_larger_than_the_spec_counts() {
        // Spec 3.7: ~72 bytes on 64-bit, plus 4 for its folder.
        if cfg!(target_pointer_width = "64") {
            assert!(std::mem::size_of::<Entry>() <= 72, "{}", std::mem::size_of::<Entry>());
        }
    }
}
