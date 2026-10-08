//! The name cache (spec 3.5): every item of the scope, read once in the background while the
//! search bar is open and kept compact (names in one string, ~40 bytes an item), so names,
//! sizes, dates and types are matched again as one types without going to the disk. At most
//! `CACHE_LIMIT` items; past that the search streams (each Enter walks).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use gezik_core::Entry;
use gezik_platform::fs::DirItem;

use crate::query::Query;
use crate::results::ResultSet;
use crate::walk::{Visit, Walk, WalkStats};

pub const CACHE_LIMIT: usize = 500_000;

const IS_DIR: u8 = 4;
const NO_TIME: i64 = i64::MIN;

/// One item: its name (a span of `names`), its folder, its flags (`Entry`'s and `IS_DIR`),
/// size and times (seconds since 1970; display only needs minutes).
#[derive(Debug, Clone, Copy)]
struct Item {
    name: u32,
    len: u32,
    parent: u32,
    flags: u8,
    size: u64,
    modified: i64,
    created: i64,
}

#[derive(Debug, Default)]
pub struct NameCache {
    root: PathBuf,
    folders: Vec<Box<str>>,
    names: String,
    items: Vec<Item>,
}

fn secs(time: Option<SystemTime>) -> i64 {
    match time.map(|t| t.duration_since(SystemTime::UNIX_EPOCH)) {
        Some(Ok(after)) => after.as_secs() as i64,
        Some(Err(before)) => -(before.duration().as_secs() as i64),
        None => NO_TIME,
    }
}

fn time(secs: i64) -> Option<SystemTime> {
    match secs {
        NO_TIME => None,
        s if s >= 0 => Some(SystemTime::UNIX_EPOCH + Duration::from_secs(s as u64)),
        s => Some(SystemTime::UNIX_EPOCH - Duration::from_secs(s.unsigned_abs())),
    }
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

    fn name(&self, item: &Item) -> &str {
        &self.names[item.name as usize..(item.name + item.len) as usize]
    }

    /// The items `query` lets through (not its content: see sapma 5), as results; whether
    /// `max_results` stopped it. `None` once `cancel` is set (looked at every 4,096 items).
    pub fn select(&self, query: &Query, cancel: &AtomicBool) -> Option<(ResultSet, bool)> {
        let mut set = ResultSet::with_folders(self.root.clone(), self.folders.clone());
        for (i, item) in self.items.iter().enumerate() {
            if i % 4096 == 0 && cancel.load(Ordering::Relaxed) {
                return None;
            }
            let (name, is_dir) = (self.name(item), item.flags & IS_DIR != 0);
            if !query.passes(name, is_dir, item.size, time(item.modified)) {
                continue;
            }
            if set.len() >= query.max_results() {
                return Some((set, true));
            }
            let entry = Entry {
                name: name.to_owned(),
                is_dir,
                flags: item.flags & !IS_DIR,
                size: item.size,
                modified: time(item.modified),
                created: time(item.created),
            };
            set.push_entry(entry, item.parent);
        }
        Some((set, false))
    }
}

pub enum CacheOutcome {
    Ready(Arc<NameCache>),
    /// More than the limit: the search streams.
    TooLarge,
    Cancelled,
}

struct Builder {
    cache: Mutex<NameCache>,
    limit: usize,
    over: AtomicBool,
    cancel: Arc<AtomicBool>,
}

impl Visit for Builder {
    fn wants_meta(&self, _: &str, _: bool) -> bool {
        true
    }

    fn folder(&self, _: &Path, relative: &str, items: Vec<DirItem>) {
        let mut cache = self.cache.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if cache.items.len() + items.len() > self.limit {
            self.over.store(true, Ordering::SeqCst);
            self.cancel.store(true, Ordering::SeqCst);
            return;
        }
        let parent = cache.folders.len() as u32;
        cache.folders.push(relative.into());
        for item in items {
            let name = cache.names.len() as u32;
            cache.names.push_str(&item.name);
            cache.items.push(Item {
                name,
                len: item.name.len() as u32,
                parent,
                flags: item.flags | if item.is_dir { IS_DIR } else { 0 },
                size: item.size,
                modified: secs(item.modified),
                created: secs(item.created),
            });
        }
    }
}

/// Reads the whole scope into a cache of at most `limit` items (`CACHE_LIMIT`).
pub fn build(walk: Walk, cancel: Arc<AtomicBool>, limit: usize) -> (CacheOutcome, WalkStats) {
    let root = if walk.absolute { PathBuf::new() } else { walk.roots.first().cloned().unwrap_or_default() };
    let builder = Arc::new(Builder {
        cache: Mutex::new(NameCache { root, ..NameCache::default() }),
        limit,
        over: AtomicBool::new(false),
        cancel: cancel.clone(),
    });
    let stats = crate::walk::run(walk, cancel, builder.clone());
    if builder.over.load(Ordering::SeqCst) {
        return (CacheOutcome::TooLarge, stats);
    }
    if stats.cancelled {
        return (CacheOutcome::Cancelled, stats);
    }
    let mut cache = std::mem::take(&mut *builder.cache.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
    cache.names.shrink_to_fit();
    cache.items.shrink_to_fit();
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
        let (outcome, _) = build(walk(&root), Arc::default(), 10);
        assert!(matches!(outcome, CacheOutcome::TooLarge));
        let _ = std::fs::remove_dir_all(&root);
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
        assert!(std::mem::size_of::<Item>() <= 40, "{}", std::mem::size_of::<Item>());
    }
}
