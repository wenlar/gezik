//! The results of one search or flat view (spec 3.7): the entries, the folder each is in
//! (relative to the scope, each folder once), and a content search's matching lines. The
//! scanner sends them in batches; the list, its filter and the background sort share them.

use std::path::{MAIN_SEPARATOR, MAIN_SEPARATOR_STR, Path, PathBuf};
use std::time::{Duration, SystemTime};

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
    entries: Rows,
    /// `entries[i]`'s place in `folders`.
    parent: Vec<u32>,
    /// A content search's first matching line per entry; `None` without content.
    matches: Option<Vec<Option<Found>>>,
    /// The sort (and folders-first) the entries are in; `None` once new ones came unsorted.
    sorted: Option<(SortSpec, bool)>,
}

// Above `Entry`'s flag bits (`HIDDEN`, `SYSTEM`, `SIZE_FLAGS`), as in the name cache.
const IS_DIR: u8 = 128;
/// No time.
const NO_TIME: i64 = i64::MIN;
/// A time's unit: Windows keeps 100 ns ticks (so every time there fits exactly), others ns.
const TICK: u128 = if cfg!(windows) { 100 } else { 1 };

/// One row, compact (spec 1, 3.7): its name a span of `Rows::names`, its length and flags
/// (`Entry`'s and `IS_DIR`) in `meta` (`len << 8 | flags`), size and times (`TICK`s since 1970).
#[derive(Debug, Clone, Copy)]
struct Row {
    name: u32,
    meta: u32,
    size: u64,
    modified: i64,
    created: i64,
}

/// The rows of a result set: 32 bytes each and their names in one string, instead of an
/// `Entry` (64 bytes) and its own allocated name. A name replaced or removed stays in
/// `names` until the set is dropped (a job's few edits; a filter's subset starts afresh).
#[derive(Debug, Clone, Default)]
struct Rows {
    names: String,
    rows: Vec<Row>,
}

fn ticks(time: Option<SystemTime>) -> i64 {
    let count = |d: Duration| i64::try_from(d.as_nanos() / TICK).unwrap_or(i64::MAX);
    match time.map(|t| t.duration_since(SystemTime::UNIX_EPOCH)) {
        None => NO_TIME,
        Some(Ok(after)) => count(after),
        // shortcut: times beyond ±292 years of 1970 clamp (Unix ns only; Windows' FILETIME fits).
        Some(Err(before)) => (-count(before.duration())).max(NO_TIME + 1),
    }
}

fn time(ticks: i64) -> Option<SystemTime> {
    if ticks == NO_TIME {
        return None;
    }
    let per_sec = (1_000_000_000 / TICK) as u64;
    let abs = ticks.unsigned_abs();
    let d = Duration::new(abs / per_sec, ((abs % per_sec) as u128 * TICK) as u32);
    if ticks >= 0 { SystemTime::UNIX_EPOCH.checked_add(d) } else { SystemTime::UNIX_EPOCH.checked_sub(d) }
}

impl Rows {
    fn len(&self) -> usize {
        self.rows.len()
    }

    fn row(&self, entry: &Entry) -> Row {
        // File names are at most 255 UTF-16 units (≤ 1,020 bytes): far below 2^24.
        debug_assert!(entry.name.len() < 1 << 24);
        let start = u32::try_from(self.names.len()).expect("result names under 4 GB");
        Row {
            name: start,
            meta: (entry.name.len() as u32) << 8 | u32::from(entry.flags | if entry.is_dir { IS_DIR } else { 0 }),
            size: entry.size,
            modified: ticks(entry.modified),
            created: ticks(entry.created),
        }
    }

    fn push(&mut self, entry: &Entry) {
        let row = self.row(entry);
        self.names.push_str(&entry.name);
        self.rows.push(row);
    }

    fn set(&mut self, i: usize, entry: &Entry) {
        let mut row = self.row(entry);
        // An unchanged name (a size or time refresh) keeps its bytes: the arena grows only on renames.
        if self.name(i) == entry.name {
            row.name = self.rows[i].name;
        } else {
            self.names.push_str(&entry.name);
        }
        self.rows[i] = row;
    }

    /// Row `i` of `from`, its name copied.
    fn push_from(&mut self, from: &Rows, i: usize) {
        let mut row = from.rows[i];
        row.name = u32::try_from(self.names.len()).expect("result names under 4 GB");
        self.names.push_str(from.name(i));
        self.rows.push(row);
    }

    fn name(&self, i: usize) -> &str {
        let row = &self.rows[i];
        &self.names[row.name as usize..row.name as usize + (row.meta >> 8) as usize]
    }

    fn is_dir(&self, i: usize) -> bool {
        self.rows[i].meta as u8 & IS_DIR != 0
    }

    fn get(&self, i: usize) -> Entry {
        let row = &self.rows[i];
        Entry {
            name: self.name(i).to_owned(),
            is_dir: self.is_dir(i),
            flags: row.meta as u8 & !IS_DIR,
            size: row.size,
            modified: time(row.modified),
            created: time(row.created),
        }
    }

    fn heap_bytes(&self) -> usize {
        self.rows.capacity() * std::mem::size_of::<Row>() + self.names.capacity()
    }
}

/// A result's key in the list (spec 3.7): its path under the scope.
pub fn relative_key(folder: &str, name: &str) -> String {
    if folder.is_empty() {
        name.to_owned()
    } else if folder.ends_with(is_separator) {
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
            entries: Rows::default(),
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
        self.entries.push(&entry);
        self.parent.push(parent);
        if let Some(matches) = &mut self.matches {
            matches.push(None);
        }
    }

    /// All of it as one batch (a cache selection or an Everything answer, spec 4.3).
    pub fn into_batch(self) -> Batch {
        let count = self.entries.len();
        Batch {
            entries: (0..count).map(|i| self.entries.get(i)).collect(),
            folders: self.folders,
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
        self.entries.len() == 0
    }

    /// Entry `i`, made from its compact row (its name allocated): for a row at a time.
    pub fn entry(&self, i: usize) -> Option<Entry> {
        (i < self.len()).then(|| self.entries.get(i))
    }

    /// Entry `i`'s name, without making the entry.
    pub fn name(&self, i: usize) -> Option<&str> {
        (i < self.len()).then(|| self.entries.name(i))
    }

    pub fn is_dir(&self, i: usize) -> bool {
        i < self.len() && self.entries.is_dir(i)
    }

    /// About how much memory the rows hold (their folders and matching lines aside).
    pub fn heap_bytes(&self) -> usize {
        self.entries.heap_bytes() + self.parent.capacity() * std::mem::size_of::<u32>()
    }

    /// The order the rows sort in (`gezik_core::sort::sort_rows`); `type_name` as there.
    pub fn sort_order(&self, spec: SortSpec, folders_first: bool, type_name: impl Fn(&Entry) -> String) -> Vec<usize> {
        gezik_core::sort::sort_rows(
            self.len(),
            &|i| std::borrow::Cow::Owned(self.entries.get(i)),
            &|i| self.entries.name(i),
            spec,
            folders_first,
            type_name,
            &|i| self.folder(i).unwrap_or(""),
        )
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
        let (folder, name) = (self.folder(i)?, self.name(i)?);
        Some(if folder.is_empty() { self.root.join(name) } else { self.root.join(folder).join(name) })
    }

    pub fn key_at(&self, i: usize) -> Option<String> {
        Some(relative_key(self.folder(i)?, self.name(i)?))
    }

    pub fn append(&mut self, batch: Batch) {
        debug_assert_eq!(batch.entries.len(), batch.parent.len());
        debug_assert!(self.matches.is_none() || batch.matches.len() == batch.entries.len());
        if !batch.entries.is_empty() {
            self.sorted = None;
        }
        self.folders.extend(batch.folders);
        for entry in &batch.entries {
            self.entries.push(entry);
        }
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
        gezik_core::sort::apply_order(&mut self.entries.rows, order);
        gezik_core::sort::apply_order(&mut self.parent, order);
        if let Some(matches) = &mut self.matches {
            gezik_core::sort::apply_order(matches, order);
        }
    }

    /// Entries `rows` (the filter's), with all the folders.
    pub fn subset(&self, rows: &[usize]) -> ResultSet {
        let rows: Vec<usize> = rows.iter().copied().filter(|&i| i < self.len()).collect();
        let mut entries = Rows::default();
        for &i in &rows {
            entries.push_from(&self.entries, i);
        }
        ResultSet {
            root: self.root.clone(),
            folders: self.folders.clone(),
            entries,
            parent: rows.iter().map(|&i| self.parent[i]).collect(),
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
            let Some(&parent) = from.parent.get(i).filter(|_| i < from.len()) else { continue };
            self.entries.push_from(&from.entries, i);
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
            .filter(|&i| names.contains(std::ffi::OsStr::new(self.entries.name(i))))
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
        self.entries.rows.retain(|_| (keep[index], index += 1).0);
        index = 0;
        self.parent.retain(|_| (keep[index], index += 1).0);
        if let Some(matches) = &mut self.matches {
            index = 0;
            matches.retain(|_| (keep[index], index += 1).0);
        }
    }
}

/// Whether two times are the same; at whole seconds when one has nothing finer (the name
/// cache keeps whole seconds, the disk has more).
fn same_time(a: Option<SystemTime>, b: Option<SystemTime>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) if a != b => {
            let (Ok(a), Ok(b)) = (a.duration_since(SystemTime::UNIX_EPOCH), b.duration_since(SystemTime::UNIX_EPOCH))
            else {
                return false;
            };
            (a.subsec_nanos() == 0 || b.subsec_nanos() == 0) && a.as_secs() == b.as_secs()
        }
        (a, b) => a == b,
    }
}

/// Whether a row's entry and what the disk says of it now show the same (name aside).
fn same_facts(old: &Entry, new: &Entry) -> bool {
    (old.is_dir, old.flags, old.size) == (new.is_dir, new.flags, new.size)
        && same_time(old.modified, new.modified)
        && same_time(old.created, new.created)
}

/// Where a job's path is in the results: its folder text (`None`: outside the scope), the
/// folder's number (`None`: not here yet) and its row if it is one already.
type Place = (Option<String>, Option<u32>, Option<usize>);

/// The separator a folder text is split at (`/` too on Windows; `\` only there: on Unix it is
/// a letter a name may have).
fn is_separator(c: char) -> bool {
    c == '/' || c == MAIN_SEPARATOR
}

/// A folder text as a lookup key: case is ignored where the file system ignores it (ASCII
/// only, as the paths compare).
fn folded(text: &str) -> String {
    if cfg!(any(windows, target_os = "macos")) { text.to_ascii_lowercase() } else { text.to_owned() }
}

impl ResultSet {
    /// For each of `paths`, its place here: one pass over the folders, one over the rows of the
    /// folders they are in (no search per path: thousands of them in 250,000 rows stay quick).
    fn places(&self, paths: &[PathBuf]) -> Vec<Place> {
        use std::collections::HashMap;
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
        let present: HashMap<(u32, &str), usize> = (0..self.entries.len())
            .filter(|&i| touched[self.parent[i] as usize])
            .map(|i| ((self.parent[i], self.entries.name(i)), i))
            .collect();
        paths
            .iter()
            .zip(&texts)
            .map(|(path, text)| {
                let number = text.as_deref().and_then(|t| numbers.get(t)).copied().filter(|n| *n != u32::MAX);
                let name = path.file_name().map(|n| n.to_string_lossy());
                let row = number.zip(name).and_then(|(n, name)| present.get(&(n, &*name)).copied());
                (text.clone(), number, row)
            })
            .collect()
    }

    /// A job's effects (spec 4.7): rows at `gone` leave; each of `added` that is a row already
    /// takes what the disk says now (a changed file loses its matching line); another goes at
    /// the end, or, if the job's `moves` (from, to) say which gone row it was, takes that
    /// row's place, the rows inside a moved folder following it. Only real moves pair: a guess
    /// could make a row name another file. Returns where each row left (but the new ones at
    /// the end) was before.
    pub fn apply_changes(
        &mut self,
        gone: &[PathBuf],
        added: Vec<(PathBuf, Entry)>,
        moves: &[(PathBuf, PathBuf)],
    ) -> Vec<usize> {
        use std::collections::{HashMap, HashSet};
        let gone_rows = self.rows_of(gone);
        let mut seen = HashSet::new();
        let added: Vec<(PathBuf, Entry)> = added.into_iter().filter(|(path, _)| seen.insert(path.clone())).collect();
        let paths: Vec<PathBuf> = added.iter().map(|(path, _)| path.clone()).collect();
        let places = self.places(&paths);
        // Where each new path was (the job's own moves), and the gone rows by path.
        let came_from: HashMap<&Path, &Path> = moves.iter().map(|(from, to)| (to.as_path(), from.as_path())).collect();
        let gone_at: HashMap<PathBuf, usize> = if came_from.is_empty() {
            HashMap::new()
        } else {
            gone_rows.iter().filter_map(|&row| Some((self.path_at(row)?, row))).collect()
        };
        let mut taken = vec![false; self.entries.len()];
        // Moved folders: their old folder text, and the new one.
        let mut renamed: HashMap<String, String> = HashMap::new();
        let mut appended = Vec::new();
        let mut new_folders: HashMap<String, u32> = HashMap::new();
        for ((path, entry), (text, number, row)) in added.into_iter().zip(places) {
            if let Some(row) = row {
                let old = self.entries.get(row);
                let changed = old.size != entry.size || !same_time(old.modified, entry.modified);
                if changed || !same_facts(&old, &entry) {
                    if changed && let Some(matches) = &mut self.matches {
                        matches[row] = None;
                    }
                    self.entries.set(row, &entry);
                    self.sorted = None;
                }
                continue;
            }
            let Some(text) = text else { continue };
            let was = came_from.get(path.as_path()).and_then(|from| gone_at.get(*from)).copied();
            let Some(row) = was.filter(|&row| !taken[row]) else {
                appended.push((text, number, entry));
                continue;
            };
            let parent = match number.or_else(|| new_folders.get(&text).copied()) {
                Some(parent) => parent,
                None => {
                    self.folders.push(text.as_str().into());
                    let parent = (self.folders.len() - 1) as u32;
                    new_folders.insert(text.clone(), parent);
                    parent
                }
            };
            if entry.is_dir && self.entries.is_dir(row) {
                let old = relative_key(&self.folders[self.parent[row] as usize], self.entries.name(row));
                renamed.insert(old, relative_key(&text, &entry.name));
            }
            self.entries.set(row, &entry);
            self.parent[row] = parent;
            taken[row] = true;
            self.sorted = None;
        }
        // The rows inside a moved folder stay, under its new path (the deepest moved folder a
        // folder is in decides).
        let mut moved = vec![false; if renamed.is_empty() { 0 } else { self.folders.len() }];
        if !renamed.is_empty() {
            for (i, folder) in self.folders.iter_mut().enumerate() {
                let mut ends: Vec<usize> = folder.match_indices(is_separator).map(|(at, _)| at).collect();
                ends.push(folder.len());
                let found = ends.into_iter().rev().find_map(|end| Some((end, renamed.get(&folder[..end])?)));
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
    /// of `paths` (its results, the rows it hid) under the scope that are not rows. Names
    /// only: the paths are made on the check's thread.
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
        // Per folder: 0 untouched, 1 a changed folder itself, 2 inside one (a changed folder's
        // text is the folder's up to or through one of its separators: one lookup each).
        let every = texts.iter().any(|(_, above)| *above);
        let changed: std::collections::HashSet<String> =
            texts.iter().filter(|(_, above)| !above).map(|(dir, _)| folded(dir)).collect();
        let state: Vec<u8> = self
            .folders
            .iter()
            .map(|folder| {
                let folder = folded(folder);
                if changed.contains(&folder) {
                    return 1;
                }
                let inside = every
                    || changed.contains("")
                    || folder
                        .match_indices(is_separator)
                        .any(|(at, _)| changed.contains(&folder[..at]) || changed.contains(&folder[..=at]));
                if inside { 2 } else { 0 }
            })
            .collect();
        let mut group = vec![usize::MAX; self.folders.len()];
        let mut folders: Vec<(Box<str>, bool, Vec<Entry>)> = Vec::new();
        for i in 0..self.entries.len() {
            let parent = self.parent[i] as usize;
            if state[parent] == 0 {
                continue;
            }
            if group[parent] == usize::MAX {
                group[parent] = folders.len();
                folders.push((self.folders[parent].clone(), state[parent] == 2, Vec::new()));
            }
            folders[group[parent]].2.push(self.entries.get(i));
        }
        let mut seen = std::collections::HashSet::new();
        let paths: Vec<PathBuf> = paths.iter().filter(|path| seen.insert(*path)).cloned().collect();
        let places = self.places(&paths);
        let paths = paths
            .into_iter()
            .zip(places)
            .filter(|(_, (text, _, row))| text.is_some() && row.is_none())
            .map(|(path, _)| path)
            .collect();
        Probe { root: self.root.clone(), folders, paths }
    }
}

/// What a job's check reads (`ResultSet::probe`): no part of the results is held while the
/// disk is read.
#[derive(Debug, Default)]
pub struct Probe {
    root: PathBuf,
    /// Changed folders (each row looked at) and folders inside one (`true`: gone, it takes all
    /// its rows), by their text, with their rows' entries.
    folders: Vec<(Box<str>, bool, Vec<Entry>)>,
    /// The job's paths under the scope that are not rows.
    paths: Vec<PathBuf>,
}

impl Probe {
    /// The rows gone from the disk; the paths that are there with what the disk says of them;
    /// and the rows of the changed folders that are still there but differ now, read again (a
    /// name swapped with another's has its size and date). Reads the disk: off the UI thread.
    pub fn verify(self) -> Verified {
        let mut verified = Verified::default();
        for (folder, inside, rows) in self.folders {
            let dir = if folder.is_empty() { self.root.clone() } else { self.root.join(&*folder) };
            if !inside {
                for old in rows {
                    let path = dir.join(&old.name);
                    match read_entry(&path) {
                        // The same as the row: nothing to change (most rows, most jobs).
                        Some(entry) if same_facts(&old, &entry) => {}
                        Some(entry) => verified.rows.push((path, entry)),
                        None => verified.gone.push(path),
                    }
                }
            } else if std::fs::symlink_metadata(&dir).is_err() {
                verified.gone.extend(rows.iter().map(|old| dir.join(&old.name)));
            }
        }
        verified.added =
            self.paths.into_iter().filter_map(|path| read_entry(&path).map(|entry| (path, entry))).collect();
        verified
    }
}

/// What a job's check found (`Probe::verify`).
#[derive(Debug, Default)]
pub struct Verified {
    /// Rows gone from the disk.
    pub gone: Vec<PathBuf>,
    /// The job's paths that are there and not rows.
    pub added: Vec<(PathBuf, Entry)>,
    /// Rows of the changed folders still there that differ now, as the disk has them.
    pub rows: Vec<(PathBuf, Entry)>,
}

/// `path` as a list entry, from the disk; `None` if it is not there.
fn read_entry(path: &Path) -> Option<Entry> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    Some(Entry {
        name: path.file_name()?.to_string_lossy().into_owned(),
        is_dir: meta.is_dir(),
        flags: gezik_core::attribute_flags(&meta),
        size: if meta.is_file() { meta.len() } else { 0 },
        modified: meta.modified().ok(),
        created: meta.created().ok(),
    })
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
            set.entries.push(&entry);
            set.parent.push(parent);
            if let Some(matches) = &mut set.matches {
                matches.push(found);
            }
        }
        set
    }
}

#[cfg(test)]
impl ResultSet {
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
        (0..self.entries.len()).find(|&i| self.parent[i] == parent && self.entries.name(i) == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::MAIN_SEPARATOR as SEP;

    /// What changed for the results after a job (spec 4.7): `set.probe(dirs, paths).verify()`,
    /// gone rows and added paths.
    fn verify(set: &ResultSet, dirs: &[PathBuf], paths: &[PathBuf]) -> (Vec<PathBuf>, Vec<(PathBuf, Entry)>) {
        let verified = set.probe(dirs, paths).verify();
        (verified.gone, verified.added)
    }

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
            std::slice::from_ref(&old),
            vec![(new.clone(), entry("renamed.txt")), (came.clone(), entry("back.txt"))],
            &[(old.clone(), new.clone())],
        );
        assert_eq!(set.len(), 4);
        assert_eq!(set.path_at(0), Some(new), "the same folder's gone row takes the new name in place");
        assert_eq!(set.path_at(3), Some(came), "nothing gone there: at the end");
        assert_eq!(from, [0, 1, 2], "no row left; the selection follows by place");
        let from = set.apply_changes(&[PathBuf::from("/w").join(format!("a{SEP}b")).join("y.txt")], Vec::new(), &[]);
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
    fn only_real_moves_pair_and_copies_go_last() {
        let mut set = sample();
        let new = PathBuf::from("/w").join("moved.txt");
        let from =
            set.apply_changes(&[PathBuf::from("/w").join("x.txt")], vec![(new.clone(), entry("moved.txt"))], &[]);
        assert_eq!(from, [1, 2]);
        assert_eq!(set.path_at(2), Some(new));
        let twice = vec![(PathBuf::from("/w").join("n"), entry("n")), (PathBuf::from("/w").join("n"), entry("n"))];
        set.apply_changes(&[], twice, &[]);
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
        let from =
            set.apply_changes(&gone, vec![(root.join("r"), folder_entry("r"))], &[(root.join("d"), root.join("r"))]);
        assert_eq!(from, [0, 1, 2, 3], "nothing left");
        assert_eq!(set.path_at(1), Some(root.join("r").join("in.txt")));
        assert_eq!(set.path_at(2), Some(root.join("r").join("e").join("deep.txt")));
        assert_eq!(set.path_at(3), Some(root.join("dd").join("other.txt")), "not a folder inside it");
        assert_eq!(set.found(1).map(|f| f.0), Some(1));
        assert_eq!(set.sorted_by(), None, "a name changed in place");
    }

    #[test]
    fn folders_follow_their_real_moves_not_the_order_they_came_in() {
        // Two folders renamed in one job, finished in the other order, each with a file of the
        // same name inside: every row must name the file that really is there.
        let root = std::env::temp_dir().join(format!("gezik-moves-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (dir, text) in [("r1", "one"), ("r2", "two")] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
            std::fs::write(root.join(dir).join("IMG_0001.jpg"), text).unwrap();
        }
        let mut set = ResultSet::new(root.clone(), false);
        set.append(Batch {
            folders: vec!["".into(), "d1".into(), "d2".into()],
            entries: vec![folder_entry("d1"), folder_entry("d2"), entry("IMG_0001.jpg"), entry("IMG_0001.jpg")],
            parent: vec![0, 0, 1, 2],
            matches: vec![None; 4],
        });
        let (gone, added) = verify(&set, std::slice::from_ref(&root), &[root.join("r2"), root.join("r1")]);
        assert_eq!(gone.len(), 4, "both folders and what is in them");
        let moves = [(root.join("d2"), root.join("r2")), (root.join("d1"), root.join("r1"))];
        set.apply_changes(&gone, added, &moves);
        assert_eq!(set.len(), 4);
        assert_eq!(set.path_at(0), Some(root.join("r1")));
        assert_eq!(set.path_at(1), Some(root.join("r2")));
        let read = |i| std::fs::read_to_string(set.path_at(i).unwrap()).unwrap();
        assert_eq!((read(2), read(3)), ("one".to_owned(), "two".to_owned()));
        // Without the moves nothing is guessed: the rows inside go, the folders come at the end.
        let mut guess = ResultSet::new(root.clone(), false);
        guess.append(Batch {
            folders: vec!["".into(), "d1".into(), "d2".into()],
            entries: vec![folder_entry("d1"), folder_entry("d2"), entry("IMG_0001.jpg"), entry("IMG_0001.jpg")],
            parent: vec![0, 0, 1, 2],
            matches: vec![None; 4],
        });
        let (gone, added) = verify(&guess, std::slice::from_ref(&root), &[root.join("r2"), root.join("r1")]);
        guess.apply_changes(&gone, added, &[]);
        assert_eq!((guess.len(), guess.path_at(0)), (2, Some(root.join("r2"))));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn rows_still_there_take_what_the_disk_says_now() {
        let root = std::env::temp_dir().join(format!("gezik-fresh-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.txt"), "four").unwrap();
        let mut set = ResultSet::new(root.clone(), true);
        set.append(Batch {
            folders: vec!["".into()],
            entries: vec![entry("a.txt")],
            parent: vec![0],
            matches: vec![Some((1, "old line".into()))],
        });
        let verified = set.probe(std::slice::from_ref(&root), &[]).verify();
        assert!(verified.gone.is_empty());
        assert_eq!(verified.rows.len(), 1);
        set.apply_changes(&verified.gone, verified.rows, &[]);
        assert_eq!(set.entry(0).map(|e| e.size), Some(4), "the size the disk has");
        assert_eq!(set.found(0), None, "the file changed: its line may be another");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn times_compare_at_whole_seconds_when_one_has_no_more() {
        let at = |secs: u64, nanos: u32| Some(SystemTime::UNIX_EPOCH + std::time::Duration::new(secs, nanos));
        assert!(same_time(at(100, 0), at(100, 0)));
        assert!(same_time(at(100, 0), at(100, 999_000_000)), "the name cache keeps whole seconds");
        assert!(same_time(at(100, 5_000), at(100, 0)));
        assert!(!same_time(at(100, 5_000), at(100, 6_000)), "both precise: they differ");
        assert!(!same_time(at(100, 0), at(101, 0)));
        assert!(!same_time(at(100, 0), None) && same_time(None, None));
    }

    #[test]
    fn rows_the_job_left_alone_are_not_read_back() {
        let root = std::env::temp_dir().join(format!("gezik-same-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.txt"), "four").unwrap();
        std::fs::write(root.join("b.txt"), "four").unwrap();
        let disk = read_entry(&root.join("a.txt")).unwrap();
        // As the name cache has it: whole seconds.
        let whole = |t: Option<SystemTime>| {
            t.map(|t| {
                let secs = t.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();
                SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs)
            })
        };
        let cached = Entry { modified: whole(disk.modified), created: whole(disk.created), ..disk.clone() };
        let b = read_entry(&root.join("b.txt")).unwrap();
        let mut set = ResultSet::new(root.clone(), false);
        set.append(Batch {
            folders: vec!["".into()],
            entries: vec![cached, b.clone()],
            parent: vec![0, 0],
            matches: vec![None, None],
        });
        set.set_sorted_by(Some((SortSpec::default(), true)));
        let verified = set.probe(std::slice::from_ref(&root), &[]).verify();
        assert!(verified.gone.is_empty() && verified.added.is_empty());
        assert!(verified.rows.is_empty(), "nothing changed: nothing to edit, {:?}", verified.rows);
        // Read back anyway (a race), a whole-second row is not taken as changed.
        set.apply_changes(&[], vec![(root.join("a.txt"), disk), (root.join("b.txt"), b)], &[]);
        assert!(set.sorted_by().is_some(), "no row changed: still sorted");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn a_backslash_is_a_letter_on_unix() {
        assert_eq!(relative_key("a\\", "b"), r"a\/b");
        assert_eq!(relative_key("a/", "b"), "a/b");
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
        if cfg!(any(windows, target_os = "macos")) {
            assert_eq!(verify(&set, &[root.join("A")], &[]).0, gone, "case is ignored");
        }
        let (gone, _) = verify(&set, std::slice::from_ref(&root), &[]);
        assert_eq!(gone.len(), 2, "the scope itself changed: ab is gone too");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn thousands_of_renames_apply_by_lookup() {
        renames_within(std::time::Duration::from_secs(60));
    }

    /// The bound a quiet machine meets (debug build); a loaded one may not.
    #[test]
    #[ignore = "timing: run on a quiet machine"]
    fn thousands_of_renames_apply_quickly() {
        renames_within(std::time::Duration::from_secs(2));
    }

    fn renames_within(bound: std::time::Duration) {
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
        let moves: Vec<(PathBuf, PathBuf)> =
            (0..2000).map(|i| (path(i, &format!("n{i}.txt")), path(i, &format!("r{i}.txt")))).collect();
        let started = std::time::Instant::now();
        let from = set.apply_changes(&gone, added, &moves);
        assert!(started.elapsed() < bound, "{:?}", started.elapsed());
        assert_eq!((set.len(), from.len()), (5000, 5000));
        assert!(set.key_at(0).is_some_and(|key| key.ends_with(".txt") && key.contains('r')));
        assert_eq!(set.index_of_path(&path(4999, "n4999.txt")), Some(4999));
        let names: std::collections::HashSet<&str> = (0..set.len()).filter_map(|i| set.name(i)).collect();
        assert!((0..2000).all(|i| names.contains(format!("r{i}.txt").as_str())) && names.len() == 5000);
    }

    #[test]
    fn a_row_is_32_bytes() {
        // Spec 1, 3.7: with its folder number and its name's bytes, ~36 B + the name a row.
        assert_eq!(std::mem::size_of::<Row>(), 32);
    }

    #[test]
    fn the_rows_sort_as_their_entries_would() {
        let set = sample();
        let entries: Vec<Entry> = (0..set.len()).filter_map(|i| set.entry(i)).collect();
        for key in [gezik_core::sort::SortKey::Name, gezik_core::sort::SortKey::Folder] {
            for dir in [gezik_core::sort::SortDir::Asc, gezik_core::sort::SortDir::Desc] {
                let spec = SortSpec { key, dir };
                let folder = |i: usize| set.folder(i).unwrap_or("");
                let expected = gezik_core::sort::sort_order(&entries, spec, true, |_| String::new(), &folder);
                assert_eq!(set.sort_order(spec, true, |_| String::new()), expected);
            }
        }
    }

    #[test]
    fn rows_give_back_the_entries_they_were_made_of() {
        let at = |secs: i64, nanos: u32| {
            let d = std::time::Duration::new(secs.unsigned_abs(), nanos);
            Some(if secs >= 0 { SystemTime::UNIX_EPOCH + d } else { SystemTime::UNIX_EPOCH - d })
        };
        let make = |name: &str, is_dir, flags, size, modified, created| Entry {
            name: name.to_owned(),
            is_dir,
            flags,
            size,
            modified,
            created,
        };
        let long = "uzun ad ".repeat(30) + "ş.txt";
        let entries = vec![
            make("İstanbul ılık ğüşöç.txt", false, 0, 7, at(1_700_000_000, 123_456_700), None),
            make(&long, false, Entry::HIDDEN | Entry::SYSTEM, u64::MAX, None, at(0, 0)),
            make(
                "klasör",
                true,
                Entry::SIZED | Entry::SIZE_PARTIAL | Entry::SIZE_STALE,
                1 << 40,
                at(-1_900_000_000, 100),
                at(4_000_000_000, 999_999_900),
            ),
            make("bekleyen", true, Entry::SIZE_PENDING, 0, at(1, 0), at(1, 0)),
            make("", false, 0, 0, None, None),
        ];
        let mut rows = Rows::default();
        for e in &entries {
            rows.push(e);
        }
        let facts = |e: &Entry| (e.name.clone(), e.is_dir, e.flags, e.size, e.modified, e.created);
        for (i, e) in entries.iter().enumerate() {
            assert_eq!(facts(&rows.get(i)), facts(e));
            assert_eq!((rows.name(i), rows.is_dir(i)), (e.name.as_str(), e.is_dir));
        }
        let mut copy = Rows::default();
        copy.push_from(&rows, 2);
        rows.set(0, &entries[3]);
        assert_eq!(facts(&copy.get(0)), facts(&entries[2]));
        assert_eq!(facts(&rows.get(0)), facts(&entries[3]), "a replaced row");
        assert_eq!(facts(&rows.get(1)), facts(&entries[1]), "the next keeps its name");
        let used = rows.names.len();
        let mut refreshed = entries[1].clone();
        refreshed.size = refreshed.size.wrapping_sub(1);
        rows.set(1, &refreshed);
        assert_eq!((facts(&rows.get(1)), rows.names.len()), (facts(&refreshed), used), "same name: no new bytes");
        assert_eq!(time(ticks(None)), None);
    }
}
