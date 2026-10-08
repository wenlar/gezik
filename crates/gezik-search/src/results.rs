//! The results of one search or flat view (spec 3.7): the entries, the folder each is in
//! (relative to the scope, each folder once), and a content search's matching lines. The
//! scanner sends them in batches; the list, its filter and the background sort share them.

use std::path::{MAIN_SEPARATOR, MAIN_SEPARATOR_STR, Path, PathBuf};

use gezik_core::Entry;
use gezik_core::ops::paths::is_within;
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

/// Where a job's path is in the results: its folder text (`None`: outside the scope), the
/// folder's number (`None`: not here yet) and whether it is a row already.
type Place = (Option<String>, Option<u32>, bool);

/// The separator a folder text is split at (`/` too on Windows, as `relative_key` reads it).
fn is_separator(c: char) -> bool {
    c == '/' || c == MAIN_SEPARATOR
}

/// Whether folder text `folder` is `dir` (`Some(true)`) or inside it (`Some(false)`); case is
/// ignored where the file system ignores it (ASCII only: no allocation per folder).
fn within_text(folder: &str, dir: &str) -> Option<bool> {
    const IGNORE_CASE: bool = cfg!(any(windows, target_os = "macos"));
    let head = folder.as_bytes().get(..dir.len())?;
    let same = if IGNORE_CASE { head.eq_ignore_ascii_case(dir.as_bytes()) } else { head == dir.as_bytes() };
    if !same {
        return None;
    }
    let rest = &folder[dir.len()..];
    if rest.is_empty() {
        Some(true)
    } else if dir.is_empty() || dir.ends_with(is_separator) || rest.starts_with(is_separator) {
        Some(false)
    } else {
        None
    }
}

impl ResultSet {
    /// For each of `paths`, its place here: one pass over the folders, one over the rows of the
    /// folders they are in (no search per path: thousands of them in 250,000 rows stay quick).
    fn places(&self, paths: &[PathBuf]) -> Vec<Place> {
        use std::collections::{HashMap, HashSet};
        let texts: Vec<Option<String>> = paths.iter().map(|path| folder_text(&self.root, path)).collect();
        let mut numbers: HashMap<&str, u32> = texts.iter().flatten().map(|t| (t.as_str(), u32::MAX)).collect();
        if !numbers.is_empty() {
            for (i, folder) in self.folders.iter().enumerate() {
                if let Some(number) = numbers.get_mut(&**folder) {
                    *number = i as u32;
                }
            }
        }
        let mut touched = vec![false; self.folders.len()];
        for &number in numbers.values() {
            if let Some(slot) = touched.get_mut(number as usize) {
                *slot = true;
            }
        }
        let present: HashSet<(u32, &str)> = (0..self.entries.len())
            .filter(|&i| touched[self.parent[i] as usize])
            .map(|i| (self.parent[i], self.entries[i].name.as_str()))
            .collect();
        paths
            .iter()
            .zip(&texts)
            .map(|(path, text)| {
                let number = text.as_deref().and_then(|t| numbers.get(t)).copied().filter(|n| *n != u32::MAX);
                let name = path.file_name().map(|n| n.to_string_lossy());
                let row = number.zip(name).is_some_and(|(n, name)| present.contains(&(n, &*name)));
                (text.clone(), number, row)
            })
            .collect()
    }

    /// A job's effects (spec 4.7): rows at `gone` leave; each of `added` not already a row goes
    /// at the end, or, with `pair` (a rename), takes the place of a gone row of its folder; a
    /// renamed folder's rows follow it. Returns where each row left (but the new ones at the
    /// end) was before.
    pub fn apply_changes(&mut self, gone: &[PathBuf], added: Vec<(PathBuf, Entry)>, pair: bool) -> Vec<usize> {
        use std::collections::{HashMap, HashSet};
        let gone_rows = self.rows_of(gone);
        let mut seen = HashSet::new();
        let added: Vec<(PathBuf, Entry)> = added.into_iter().filter(|(path, _)| seen.insert(path.clone())).collect();
        let paths: Vec<PathBuf> = added.iter().map(|(path, _)| path.clone()).collect();
        let places = self.places(&paths);
        // By folder, the gone rows free to take (last first), and whether there were several.
        let mut free: HashMap<u32, (Vec<usize>, bool)> = HashMap::new();
        if pair {
            for &row in gone_rows.iter().rev() {
                free.entry(self.parent[row]).or_default().0.push(row);
            }
            for slot in free.values_mut() {
                slot.1 = slot.0.len() > 1;
            }
        }
        let mut taken = vec![false; self.entries.len()];
        // Renamed folders: their old folder text, and the new one.
        let mut renamed: HashMap<String, String> = HashMap::new();
        let mut appended = Vec::new();
        let mut new_folders: HashMap<String, u32> = HashMap::new();
        for ((_, entry), (text, number, row)) in added.into_iter().zip(places) {
            if row || text.is_none() {
                continue;
            }
            match number.and_then(|n| free.get_mut(&n)).and_then(|(rows, several)| Some((rows.pop()?, *several))) {
                Some((row, several)) => {
                    // Several gone there: which old row this one was is not known, nor its line.
                    if several && let Some(matches) = &mut self.matches {
                        matches[row] = None;
                    }
                    let folder = &self.folders[self.parent[row] as usize];
                    if entry.is_dir && self.entries[row].is_dir {
                        renamed
                            .insert(relative_key(folder, &self.entries[row].name), relative_key(folder, &entry.name));
                    }
                    self.entries[row] = entry;
                    taken[row] = true;
                    self.sorted = None;
                }
                None => appended.push((text, number, entry)),
            }
        }
        // The rows inside a renamed folder stay, under its new name.
        let mut moved = vec![false; if renamed.is_empty() { 0 } else { self.folders.len() }];
        if !renamed.is_empty() {
            for (i, folder) in self.folders.iter_mut().enumerate() {
                let ends = folder.match_indices(is_separator).map(|(at, _)| at).chain([folder.len()]);
                let found = ends.filter_map(|end| Some((end, renamed.get(&folder[..end])?))).next();
                if let Some((end, new)) = found {
                    *folder = format!("{new}{}", &folder[end..]).into();
                    moved[i] = true;
                }
            }
        }
        let removed: Vec<usize> = gone_rows
            .into_iter()
            .filter(|&row| !taken[row] && !moved.get(self.parent[row] as usize).copied().unwrap_or(false))
            .collect();
        let kept: Vec<usize> = (0..self.entries.len()).filter(|i| removed.binary_search(i).is_err()).collect();
        self.remove(&removed);
        for (text, number, entry) in appended {
            let Some(text) = text else { continue };
            let parent = match number.or_else(|| new_folders.get(&text).copied()) {
                Some(parent) => parent,
                None => {
                    self.folders.push(text.as_str().into());
                    let parent = (self.folders.len() - 1) as u32;
                    new_folders.insert(text, parent);
                    parent
                }
            };
            self.push_entry(entry, parent);
        }
        kept
    }

    /// What a job's check reads of the results (spec 4.7), taken on the UI thread: the rows of
    /// `dirs` (the job's changed folders), the folders inside them with their rows, and those
    /// of `paths` (its results, the rows it hid) under the scope that are not rows.
    pub fn probe(&self, dirs: &[PathBuf], paths: &[PathBuf]) -> Probe {
        let texts: Vec<(String, bool)> = dirs
            .iter()
            .filter_map(|dir| {
                if self.root.as_os_str().is_empty() {
                    return Some((dir.display().to_string(), false));
                }
                if is_within(dir, &self.root) {
                    let parts: Vec<String> = dir
                        .components()
                        .skip(self.root.components().count())
                        .map(|c| c.as_os_str().to_string_lossy().into_owned())
                        .collect();
                    return Some((parts.join(MAIN_SEPARATOR_STR), false));
                }
                // The scope's own folder is in a changed one: every folder is looked at.
                is_within(&self.root, dir).then(|| (String::new(), true))
            })
            .collect();
        // Per folder: 0 untouched, 1 a changed folder itself, 2 inside one.
        let state: Vec<u8> = self
            .folders
            .iter()
            .map(|folder| {
                let mut state = 0;
                for (dir, above) in &texts {
                    match within_text(folder, dir) {
                        Some(true) if !above => return 1,
                        Some(_) => state = 2,
                        None => {}
                    }
                }
                state
            })
            .collect();
        let mut group = vec![usize::MAX; self.folders.len()];
        let mut inside: Vec<(PathBuf, Vec<PathBuf>)> = Vec::new();
        let mut rows = Vec::new();
        for i in 0..self.entries.len() {
            let parent = self.parent[i] as usize;
            match state[parent] {
                1 => rows.extend(self.path_at(i)),
                2 => {
                    if group[parent] == usize::MAX {
                        let folder = &self.folders[parent];
                        group[parent] = inside.len();
                        inside.push((
                            if folder.is_empty() { self.root.clone() } else { self.root.join(&**folder) },
                            Vec::new(),
                        ));
                    }
                    inside[group[parent]].1.extend(self.path_at(i));
                }
                _ => {}
            }
        }
        let mut seen = std::collections::HashSet::new();
        let paths: Vec<PathBuf> = paths.iter().filter(|path| seen.insert(*path)).cloned().collect();
        let places = self.places(&paths);
        let paths = paths
            .into_iter()
            .zip(places)
            .filter(|(_, (text, _, row))| text.is_some() && !row)
            .map(|(path, _)| path)
            .collect();
        Probe { rows, inside, paths }
    }
}

/// What a job's check reads (`ResultSet::probe`): no part of the results is held while the
/// disk is read.
#[derive(Debug, Default)]
pub struct Probe {
    /// Rows in a changed folder itself: each one is looked at.
    rows: Vec<PathBuf>,
    /// Folders inside a changed one, with their rows: a folder gone takes them all.
    inside: Vec<(PathBuf, Vec<PathBuf>)>,
    /// The job's paths under the scope that are not rows.
    paths: Vec<PathBuf>,
}

impl Probe {
    /// The rows gone from the disk, and the paths that are there with what the disk says of
    /// them. Reads the disk: off the UI thread.
    pub fn verify(self) -> (Vec<PathBuf>, Vec<(PathBuf, Entry)>) {
        let there = |path: &Path| std::fs::symlink_metadata(path).is_ok();
        let mut gone: Vec<PathBuf> = self.rows.into_iter().filter(|path| !there(path)).collect();
        for (folder, rows) in self.inside {
            if !there(&folder) {
                gone.extend(rows);
            }
        }
        let added = self
            .paths
            .into_iter()
            .filter_map(|path| {
                let meta = std::fs::symlink_metadata(&path).ok()?;
                let entry = Entry {
                    name: path.file_name()?.to_string_lossy().into_owned(),
                    is_dir: meta.is_dir(),
                    flags: gezik_core::attribute_flags(&meta),
                    size: if meta.is_file() { meta.len() } else { 0 },
                    modified: meta.modified().ok(),
                    created: meta.created().ok(),
                };
                Some((path, entry))
            })
            .collect();
        (gone, added)
    }
}

/// What changed for the results after a job (spec 4.7): `set.probe(dirs, paths).verify()`.
pub fn verify(set: &ResultSet, dirs: &[PathBuf], paths: &[PathBuf]) -> (Vec<PathBuf>, Vec<(PathBuf, Entry)>) {
    set.probe(dirs, paths).verify()
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
    fn a_renamed_result_keeps_its_place_and_new_ones_go_last() {
        let mut set = sample();
        let old = PathBuf::from("/w").join("x.txt");
        let new = PathBuf::from("/w").join("renamed.txt");
        let came = PathBuf::from("/w").join("c").join("back.txt");
        let from = set.apply_changes(
            &[old],
            vec![(new.clone(), entry("renamed.txt")), (came.clone(), entry("back.txt"))],
            true,
        );
        assert_eq!(set.len(), 4);
        assert_eq!(set.path_at(0), Some(new), "the same folder's gone row takes the new name in place");
        assert_eq!(set.path_at(3), Some(came), "nothing gone there: at the end");
        assert_eq!(from, [0, 1, 2], "no row left; the selection follows by place");
        let from = set.apply_changes(&[PathBuf::from("/w").join(format!("a{SEP}b")).join("y.txt")], Vec::new(), true);
        assert_eq!((set.len(), from), (3, vec![0, 2, 3]));
    }

    #[test]
    fn after_a_job_the_disk_says_what_changed() {
        let root = std::env::temp_dir().join(format!("gezik-verify-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("a")).unwrap();
        std::fs::write(root.join("a").join("kept.txt"), "k").unwrap();
        std::fs::write(root.join("a").join("new.txt"), "12").unwrap();
        let mut set = ResultSet::new(root.clone(), false);
        set.append(Batch {
            folders: vec!["a".into()],
            entries: vec![entry("kept.txt"), entry("gone.txt")],
            parent: vec![0, 0],
            matches: vec![None, None],
        });
        let (gone, added) = verify(
            &set,
            &[root.join("a")],
            &[root.join("a").join("new.txt"), root.join("a").join("kept.txt"), PathBuf::from("/elsewhere/x")],
        );
        assert_eq!(gone, [root.join("a").join("gone.txt")]);
        assert_eq!(added.len(), 1, "a row already there and a path outside are left out");
        assert_eq!((added[0].0.clone(), added[0].1.size), (root.join("a").join("new.txt"), 2));
        let _ = std::fs::remove_dir_all(&root);
    }

    fn folder_entry(name: &str) -> Entry {
        Entry { is_dir: true, ..entry(name) }
    }

    #[test]
    fn only_a_rename_pairs_and_copies_go_last() {
        let mut set = sample();
        let new = PathBuf::from("/w").join("moved.txt");
        let from =
            set.apply_changes(&[PathBuf::from("/w").join("x.txt")], vec![(new.clone(), entry("moved.txt"))], false);
        assert_eq!(from, [1, 2]);
        assert_eq!(set.path_at(2), Some(new));
        let twice = vec![(PathBuf::from("/w").join("n"), entry("n")), (PathBuf::from("/w").join("n"), entry("n"))];
        set.apply_changes(&[], twice, false);
        assert_eq!(set.len(), 4, "a path given twice is one row");
    }

    #[test]
    fn a_renamed_folder_takes_its_rows_along() {
        let root = PathBuf::from("/w");
        let mut set = ResultSet::new(root.clone(), true);
        set.append(Batch {
            folders: vec!["".into(), "d".into(), format!("d{SEP}e").into(), "dd".into()],
            entries: vec![folder_entry("d"), entry("in.txt"), entry("deep.txt"), entry("other.txt")],
            parent: vec![0, 1, 2, 3],
            matches: vec![None, Some((1, "in".into())), None, None],
        });
        set.set_sorted_by(Some((SortSpec::default(), true)));
        let gone = [root.join("d"), root.join("d").join("in.txt"), root.join("d").join("e").join("deep.txt")];
        let from = set.apply_changes(&gone, vec![(root.join("r"), folder_entry("r"))], true);
        assert_eq!(from, [0, 1, 2, 3], "nothing left");
        assert_eq!(set.path_at(1), Some(root.join("r").join("in.txt")));
        assert_eq!(set.path_at(2), Some(root.join("r").join("e").join("deep.txt")));
        assert_eq!(set.path_at(3), Some(root.join("dd").join("other.txt")), "not a folder inside it");
        assert_eq!(set.found(1).map(|f| f.0), Some(1));
        assert_eq!(set.sorted_by(), None, "a name changed in place");
    }

    #[test]
    fn folders_inside_a_changed_one_count_by_whether_they_are_still_there() {
        let root = std::env::temp_dir().join(format!("gezik-inside-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("a").join("kept")).unwrap();
        std::fs::write(root.join("a").join("kept").join("k.txt"), "k").unwrap();
        let mut set = ResultSet::new(root.clone(), false);
        set.append(Batch {
            folders: vec!["a".into(), format!("a{SEP}kept").into(), format!("a{SEP}gone{SEP}deep").into(), "ab".into()],
            entries: vec![folder_entry("kept"), entry("k.txt"), entry("x.txt"), entry("y.txt")],
            parent: vec![0, 1, 2, 3],
            matches: vec![None; 4],
        });
        let (gone, added) = verify(&set, &[root.join("a")], &[]);
        assert_eq!(gone, [root.join("a").join("gone").join("deep").join("x.txt")], "ab is not inside a");
        assert!(added.is_empty());
        let (gone, _) = verify(&set, std::slice::from_ref(&root), &[]);
        assert_eq!(gone.len(), 2, "the scope itself changed: ab is gone too");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn thousands_of_renames_apply_by_lookup() {
        let root = PathBuf::from("/w");
        let mut set = ResultSet::new(root.clone(), false);
        let folders: Vec<Box<str>> = (0..50).map(|f| format!("f{f}").into()).collect();
        let entries: Vec<Entry> = (0..5000).map(|i| entry(&format!("n{i}.txt"))).collect();
        let parent: Vec<u32> = (0..5000).map(|i| i % 50).collect();
        set.append(Batch { folders, entries, parent, matches: vec![None; 5000] });
        let path = |i: u32, name: &str| root.join(format!("f{}", i % 50)).join(name);
        let gone: Vec<PathBuf> = (0..2000).map(|i| path(i, &format!("n{i}.txt"))).collect();
        let added: Vec<(PathBuf, Entry)> =
            (0..2000).map(|i| (path(i, &format!("r{i}.txt")), entry(&format!("r{i}.txt")))).collect();
        let started = std::time::Instant::now();
        let from = set.apply_changes(&gone, added, true);
        assert!(started.elapsed() < std::time::Duration::from_secs(2), "{:?}", started.elapsed());
        assert_eq!((set.len(), from.len()), (5000, 5000));
        assert!(set.key_at(0).is_some_and(|key| key.ends_with(".txt") && key.contains('r')));
        assert_eq!(set.index_of_path(&path(4999, "n4999.txt")), Some(4999));
        let names: std::collections::HashSet<String> = set.entries().iter().map(|e| e.name.clone()).collect();
        assert!((0..2000).all(|i| names.contains(&format!("r{i}.txt"))) && names.len() == 5000);
    }

    #[test]
    fn an_entry_is_no_larger_than_the_spec_counts() {
        // Spec 3.7: ~72 bytes on 64-bit, plus 4 for its folder.
        if cfg!(target_pointer_width = "64") {
            assert!(std::mem::size_of::<Entry>() <= 72, "{}", std::mem::size_of::<Entry>());
        }
    }
}
