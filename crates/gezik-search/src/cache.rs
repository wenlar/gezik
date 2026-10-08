//! The name cache (spec 3.5): every item of the scope, read once in the background while the
//! search bar is open and kept compact (names in one string, 32 bytes an item besides; a folder
//! is the item that names it, so no folder path is kept), so names, sizes, dates and types are
//! matched again as one types without going to the disk. At most `CACHE_LIMIT` items; past
//! that the search streams (each Enter walks).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use gezik_core::Entry;
use gezik_platform::fs::DirItem;

use crate::query::Query;
use crate::results::{ResultSet, relative_key};
use crate::walk::{Visit, Walk, WalkStats};

pub const CACHE_LIMIT: usize = 500_000;

const IS_DIR: u8 = 4;
/// No time (`u32` seconds since 1970; earlier times are kept as 1970, later than 2106 as 2106).
const NO_TIME: u32 = u32::MAX;
/// A folder record that is a whole path (`paths`) rather than an item.
const WHOLE_PATH: u32 = 1 << 31;

/// One item: its name (a span of `names`), its folder, its flags (`Entry`'s and `IS_DIR`),
/// size and times (whole seconds).
#[derive(Debug, Clone, Copy)]
struct Item {
    name: u32,
    parent: u32,
    size: u64,
    modified: u32,
    created: u32,
    len: u16,
    flags: u8,
}

#[derive(Debug, Default)]
pub struct NameCache {
    root: PathBuf,
    /// Per folder: the item that names it, or `WHOLE_PATH | i` for `paths[i]` (a root).
    folders: Vec<u32>,
    /// The roots' folder texts (`""` under a folder scope; the whole path for every drive).
    paths: Vec<Box<str>>,
    names: String,
    items: Vec<Item>,
}

fn secs(time: Option<SystemTime>) -> u32 {
    match time.map(|t| t.duration_since(SystemTime::UNIX_EPOCH)) {
        Some(Ok(after)) => after.as_secs().min(u64::from(NO_TIME - 1)) as u32,
        Some(Err(_)) => 0,
        None => NO_TIME,
    }
}

fn time(secs: u32) -> Option<SystemTime> {
    (secs != NO_TIME).then(|| SystemTime::UNIX_EPOCH + Duration::from_secs(u64::from(secs)))
}

impl NameCache {
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// About how much memory it holds (spec 3.5: 500,000 items ≤ 35 MB).
    pub fn heap_bytes(&self) -> usize {
        self.items.capacity() * std::mem::size_of::<Item>()
            + self.names.capacity()
            + self.folders.capacity() * std::mem::size_of::<u32>()
            + self.paths.iter().map(|path| path.len() + std::mem::size_of::<Box<str>>()).sum::<usize>()
    }

    fn name(&self, item: &Item) -> &str {
        &self.names[item.name as usize..item.name as usize + usize::from(item.len)]
    }

    /// Folder `folder`'s text as the walk gave it (`relative_key` from its root down).
    fn folder_text(&self, mut folder: u32) -> String {
        let mut names = Vec::new();
        let start = loop {
            match self.folders.get(folder as usize) {
                Some(&record) if record & WHOLE_PATH != 0 => {
                    break self.paths.get((record & !WHOLE_PATH) as usize).map_or("", |path| &**path);
                }
                Some(&record) => {
                    let item = &self.items[record as usize];
                    names.push(self.name(item));
                    folder = item.parent;
                }
                None => break "",
            }
        };
        names.iter().rev().fold(start.to_owned(), |text, name| relative_key(&text, name))
    }

    /// The items `query` lets through (not its content: see sapma 5), as results with only the
    /// folders they use; whether `max_results` stopped it. `None` once `cancel` is set (looked
    /// at every 4,096 items).
    pub fn select(&self, query: &Query, cancel: &AtomicBool) -> Option<(ResultSet, bool)> {
        let mut used: Vec<u32> = vec![u32::MAX; self.folders.len()];
        let mut texts: Vec<Box<str>> = Vec::new();
        let mut entries = Vec::new();
        for (i, item) in self.items.iter().enumerate() {
            if i % 4096 == 0 && cancel.load(Ordering::Relaxed) {
                return None;
            }
            let (name, is_dir) = (self.name(item), item.flags & IS_DIR != 0);
            if !query.passes(name, is_dir, item.size, time(item.modified)) {
                continue;
            }
            if entries.len() >= query.max_results() {
                return Some((self.results(texts, entries), true));
            }
            let parent = match used.get_mut(item.parent as usize) {
                Some(slot) if *slot != u32::MAX => *slot,
                Some(slot) => {
                    *slot = texts.len() as u32;
                    texts.push(self.folder_text(item.parent).into());
                    *slot
                }
                None => continue,
            };
            let entry = Entry {
                name: name.to_owned(),
                is_dir,
                flags: item.flags & !IS_DIR,
                size: item.size,
                modified: time(item.modified),
                created: time(item.created),
            };
            entries.push((entry, parent));
        }
        Some((self.results(texts, entries), false))
    }

    fn results(&self, folders: Vec<Box<str>>, entries: Vec<(Entry, u32)>) -> ResultSet {
        let mut set = ResultSet::with_folders(self.root.clone(), folders);
        for (entry, parent) in entries {
            set.push_entry(entry, parent);
        }
        set
    }
}

pub enum CacheOutcome {
    Ready(Arc<NameCache>),
    /// More than the limit: the search streams.
    TooLarge,
    Cancelled,
}

#[derive(Default)]
struct Building {
    cache: NameCache,
    /// Folders handed over but not read yet, by their text: the item that names each.
    pending: HashMap<Box<str>, u32>,
}

struct Builder {
    building: Mutex<Building>,
    limit: usize,
    /// Past the limit: the walk stops (its own flag, not the caller's cancel).
    over: AtomicBool,
}

impl Visit for Builder {
    fn wants_meta(&self, _: &str, _: bool) -> bool {
        true
    }

    fn folder(&self, _: &Path, relative: &str, items: Vec<DirItem>) {
        let mut building = self.building.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let Building { cache, pending } = &mut *building;
        if cache.items.len() + items.len() > self.limit {
            self.over.store(true, Ordering::SeqCst);
            return;
        }
        let parent = cache.folders.len() as u32;
        // The walk read this folder's parent first: the item that names it is known.
        let record = pending.remove(relative).unwrap_or_else(|| {
            cache.paths.push(relative.into());
            WHOLE_PATH | (cache.paths.len() - 1) as u32
        });
        cache.folders.push(record);
        for item in items {
            if item.is_dir && !item.is_link {
                pending.insert(relative_key(relative, &item.name).into(), cache.items.len() as u32);
            }
            // A name is at most 255 UTF-16 units (765 bytes): `u16` always holds it.
            let Ok(len) = u16::try_from(item.name.len()) else { continue };
            let name = cache.names.len() as u32;
            cache.names.push_str(&item.name);
            cache.items.push(Item {
                name,
                parent,
                size: item.size,
                modified: secs(item.modified),
                created: secs(item.created),
                len,
                flags: item.flags | if item.is_dir { IS_DIR } else { 0 },
            });
        }
    }

    fn stopped(&self) -> bool {
        self.over.load(Ordering::Relaxed)
    }
}

/// Reads the whole scope into a cache of at most `limit` items (`CACHE_LIMIT`).
pub fn build(walk: Walk, cancel: Arc<AtomicBool>, limit: usize) -> (CacheOutcome, WalkStats) {
    let root = if walk.absolute { PathBuf::new() } else { walk.roots.first().cloned().unwrap_or_default() };
    let builder = Arc::new(Builder {
        building: Mutex::new(Building { cache: NameCache { root, ..NameCache::default() }, ..Building::default() }),
        limit,
        over: AtomicBool::new(false),
    });
    let stats = crate::walk::run(walk, cancel, builder.clone());
    if builder.over.load(Ordering::SeqCst) {
        return (CacheOutcome::TooLarge, stats);
    }
    if stats.cancelled {
        return (CacheOutcome::Cancelled, stats);
    }
    let building = std::mem::take(&mut *builder.building.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
    let mut cache = building.cache;
    cache.names.shrink_to_fit();
    cache.items.shrink_to_fit();
    cache.folders.shrink_to_fit();
    (CacheOutcome::Ready(Arc::new(cache)), stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::QueryOptions;
    use crate::walk::{Walk, WalkRules};
    use gezik_core::search::{Scope, SearchSpec};

    fn tree(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gezik-cache-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn walk(root: &Path) -> Walk {
        Walk::new(vec![root.to_path_buf()], false, 4, WalkRules::new(None, &[], Vec::new()))
    }

    fn query(root: &Path, pattern: &str) -> Query {
        let mut spec = SearchSpec::new(Scope::Folder(root.to_path_buf()));
        spec.pattern = pattern.into();
        Query::compile(&spec, &QueryOptions::local(1024, 100)).unwrap()
    }

    #[test]
    fn matching_again_does_not_go_to_the_disk() {
        let root = tree("again");
        std::fs::create_dir_all(root.join("a/b")).unwrap();
        std::fs::write(root.join("a/b/rapor.pdf"), "12345").unwrap();
        std::fs::write(root.join("a/notes.txt"), "x").unwrap();
        let (outcome, stats) = build(walk(&root), Arc::default(), CACHE_LIMIT);
        let CacheOutcome::Ready(cache) = outcome else { panic!("not ready") };
        assert_eq!((cache.len(), stats.folders), (4, 3));
        std::fs::remove_dir_all(&root).unwrap();
        let (set, full) = cache.select(&query(&root, "*.pdf"), &AtomicBool::new(false)).unwrap();
        assert!(!full);
        assert_eq!(set.len(), 1);
        assert_eq!(set.path_at(0), Some(root.join("a").join("b").join("rapor.pdf")));
        assert_eq!(set.entry(0).map(|e| e.size), Some(5), "sizes and dates come from the cache too");
        assert!(set.entry(0).unwrap().modified.is_some());
        assert_eq!(cache.select(&query(&root, "zzz"), &AtomicBool::new(false)).unwrap().0.len(), 0);
        assert!(cache.select(&query(&root, ""), &AtomicBool::new(true)).is_none(), "cancelled");
    }

    #[test]
    fn a_tree_over_the_limit_is_too_large() {
        let root = tree("limit");
        for i in 0..20 {
            std::fs::write(root.join(format!("f{i}")), "").unwrap();
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let (outcome, stats) = build(walk(&root), cancel.clone(), 10);
        assert!(matches!(outcome, CacheOutcome::TooLarge));
        assert!(!cancel.load(Ordering::SeqCst) && !stats.cancelled, "the caller's flag is left alone");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn folders_are_named_by_their_items_and_only_the_used_ones_come_back() {
        let root = tree("folders");
        std::fs::create_dir_all(root.join("a/b/c")).unwrap();
        std::fs::create_dir_all(root.join("x/y")).unwrap();
        std::fs::write(root.join("a/b/c/deep.txt"), "").unwrap();
        std::fs::write(root.join("x/y/other.md"), "").unwrap();
        std::fs::write(root.join("top.txt"), "").unwrap();
        let CacheOutcome::Ready(cache) = build(walk(&root), Arc::default(), CACHE_LIMIT).0 else { panic!() };
        let (set, _) = cache.select(&query(&root, "*.txt"), &AtomicBool::new(false)).unwrap();
        let mut paths: Vec<PathBuf> = (0..set.len()).filter_map(|i| set.path_at(i)).collect();
        paths.sort();
        assert_eq!(paths, [root.join("a").join("b").join("c").join("deep.txt"), root.join("top.txt")]);
        let folders: Vec<&str> = (0..set.len()).filter_map(|i| set.folder(i)).collect();
        assert!(folders.contains(&"") && folders.iter().all(|f| !f.contains('x')));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn every_drive_keeps_whole_folder_paths() {
        let root = tree("absolute");
        std::fs::create_dir_all(root.join("a")).unwrap();
        std::fs::write(root.join("a/f.txt"), "").unwrap();
        let walk = Walk::new(vec![root.clone()], true, 2, WalkRules::new(None, &[], Vec::new()));
        let CacheOutcome::Ready(cache) = build(walk, Arc::default(), CACHE_LIMIT).0 else { panic!() };
        let (set, _) = cache.select(&query(&root, "f.txt"), &AtomicBool::new(false)).unwrap();
        assert_eq!(set.path_at(0), Some(root.join("a").join("f.txt")));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn times_keep_their_seconds() {
        let at = SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_123);
        assert_eq!(time(secs(Some(at))), Some(at));
        assert_eq!(time(secs(None)), None);
        assert_eq!(time(secs(Some(SystemTime::UNIX_EPOCH - Duration::from_secs(5)))), Some(SystemTime::UNIX_EPOCH));
    }

    #[test]
    fn a_selection_stops_at_the_limit() {
        let root = tree("max");
        for i in 0..30 {
            std::fs::write(root.join(format!("f{i}.txt")), "").unwrap();
        }
        let CacheOutcome::Ready(cache) = build(walk(&root), Arc::default(), CACHE_LIMIT).0 else { panic!() };
        let mut spec = SearchSpec::flat_view(root.clone());
        spec.pattern = "f".into();
        let q = Query::compile(&spec, &QueryOptions::local(1024, 10)).unwrap();
        let (set, full) = cache.select(&q, &AtomicBool::new(false)).unwrap();
        assert!(full && set.len() == 10);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_item_is_small() {
        // Spec 3.5: ~28 bytes and the name, 500,000 items ≤ 35 MB.
        assert!(std::mem::size_of::<Item>() <= 32, "{}", std::mem::size_of::<Item>());
    }
}
