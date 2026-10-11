//! Folder sizes (spec 6): a folder's files added up by the walk (links never followed, cloud
//! folders never gone into, one file system on Unix, low priority), and the totals kept in
//! memory: at most `CACHE_MAX` folders, the least used dropped first, never written to disk.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use gezik_core::ops::paths::path_key;
use gezik_core::treemap::SizeTree;
use gezik_platform::fs::DirItem;

use crate::walk::{self, Visit, Walk, WalkRules, WalkStats};

/// What a folder holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FolderTotal {
    /// The logical sizes of its files (a hard link counted wherever it is seen).
    pub bytes: u64,
    /// Files and folders under it; `None` when Everything gave the size (it does not count).
    pub counts: Option<(u64, u64)>,
    /// Something could not be read, or a cloud folder was left: at least `bytes` (`≥`).
    pub partial: bool,
}

/// Adds up what the walk hands over.
#[derive(Default)]
struct Adder {
    bytes: AtomicU64,
    files: AtomicU64,
    folders: AtomicU64,
    partial: AtomicBool,
}

impl Visit for Adder {
    fn wants_meta(&self, _: &str, is_dir: bool) -> bool {
        !is_dir
    }

    fn folder(&self, _: &Path, _: &str, items: Vec<DirItem>) {
        let (mut bytes, mut files, mut folders) = (0u64, 0u64, 0u64);
        for item in &items {
            if !item.is_dir {
                files += 1;
                bytes = bytes.saturating_add(item.size);
            } else if !item.is_link {
                folders += 1;
                if item.offline {
                    self.partial.store(true, Ordering::Relaxed);
                }
            }
        }
        self.bytes.fetch_add(bytes, Ordering::Relaxed);
        self.files.fetch_add(files, Ordering::Relaxed);
        self.folders.fetch_add(folders, Ordering::Relaxed);
    }
}

impl Adder {
    fn total(&self, stats: &WalkStats) -> FolderTotal {
        FolderTotal {
            bytes: self.bytes.load(Ordering::Relaxed),
            counts: Some((self.files.load(Ordering::Relaxed), self.folders.load(Ordering::Relaxed))),
            partial: self.partial.load(Ordering::Relaxed) || stats.problems.count > 0,
        }
    }
}

/// Adds up `folder` on `threads` low-priority threads (2; 1 on a network folder). `None` when
/// `cancel` was set before it ended (a thread stuck on a dead share is left behind, as the
/// search leaves it). Reads folder listings only: no file is opened. Off the UI thread.
pub fn measure(folder: &Path, threads: usize, cancel: &Arc<AtomicBool>) -> Option<FolderTotal> {
    let rules = size_rules(folder);
    let adder = Arc::new(Adder::default());
    let stats = walk::run(Walk::new(vec![folder.to_path_buf()], false, threads, rules), cancel.clone(), adder.clone());
    (!stats.cancelled).then(|| adder.total(&stats))
}

/// The walk's rules for adding up `folder`: hidden items too, no cloud folder gone into, the
/// folder's own file system on Unix (spec 3.4; unknown: no rule, the reads fail).
fn size_rules(folder: &Path) -> WalkRules {
    let devices = if cfg!(unix) {
        gezik_platform::fs::device_of(folder).map(|d| vec![d]).unwrap_or_default()
    } else {
        Vec::new()
    };
    let mut rules = WalkRules::new(None, &[], devices);
    rules.enter_offline = false;
    rules
}

/// How often a scan going on says it has more (spec 11 §10.1: at most twice a second).
const TICK_EVERY: Duration = Duration::from_millis(500);

/// Builds the size map's tree as the walk hands folders over.
struct TreeBuilder {
    tree: Arc<Mutex<SizeTree>>,
    /// The folders found and not read yet, by their path under the root.
    waiting: Mutex<HashMap<String, u32>>,
    rules: WalkRules,
    tick: Box<dyn Fn() + Send + Sync>,
    ticked: Mutex<Instant>,
}

impl Visit for TreeBuilder {
    fn wants_meta(&self, _: &str, is_dir: bool) -> bool {
        !is_dir
    }

    fn folder(&self, dir: &Path, relative: &str, items: Vec<DirItem>) {
        {
            let mut waiting = self.waiting.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            let Some(id) = waiting.remove(relative) else { return };
            let mut tree = self.tree.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            for item in items {
                if !item.is_dir {
                    let node = tree.node_mut(id);
                    node.own_bytes = node.own_bytes.saturating_add(item.size);
                    node.own_files += 1;
                    if id == 0 {
                        tree.files.push((item.name, item.size));
                    }
                } else if item.is_link {
                    // Neither gone into nor counted, as `Adder` does.
                } else if walk::goes_into(&item, &self.rules)
                    && walk::same_device(&self.rules.devices, item.device)
                    && !(cfg!(target_os = "macos") && walk::walked_twice(&dir.join(&item.name)))
                {
                    let child = tree.add(id, &item.name, item.modified);
                    waiting.insert(crate::results::relative_key(relative, &item.name), child);
                } else {
                    // A folder the walk does not go into still counts as one (`Adder`); a
                    // cloud folder makes the total partial.
                    let node = tree.node_mut(id);
                    node.other_folders += 1;
                    node.unread |= item.offline;
                }
            }
        }
        let mut ticked = self.ticked.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if ticked.elapsed() >= TICK_EVERY {
            *ticked = Instant::now();
            drop(ticked);
            (self.tick)();
        }
    }
}

/// Walks `folder` into `tree` (a new one, of the folder) as `measure` adds it up (spec 11
/// §10.1): folders only, with their files added up; the folder's own files one by one.
/// `tick` is called at most every 500 ms while it goes on. Whether it ended (not cancelled);
/// the tree's totals are added up then. Off the UI thread.
pub fn scan_tree(
    folder: &Path,
    threads: usize,
    cancel: &Arc<AtomicBool>,
    tree: Arc<Mutex<SizeTree>>,
    tick: Box<dyn Fn() + Send + Sync>,
) -> bool {
    tree.lock().unwrap_or_else(std::sync::PoisonError::into_inner).node_mut(0).stamp =
        std::fs::metadata(folder).and_then(|m| m.modified()).ok();
    let rules = size_rules(folder);
    let builder = Arc::new(TreeBuilder {
        tree: tree.clone(),
        waiting: Mutex::new(HashMap::from([(String::new(), 0)])),
        rules: rules.clone(),
        tick,
        ticked: Mutex::new(Instant::now()),
    });
    let stats =
        walk::run(Walk::new(vec![folder.to_path_buf()], false, threads, rules), cancel.clone(), builder.clone());
    let waiting = std::mem::take(&mut *builder.waiting.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
    let mut tree = tree.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    // Not read: it could not be, or was too deep.
    for id in waiting.into_values() {
        tree.node_mut(id).unread = true;
    }
    tree.sum();
    !stats.cancelled
}

pub const CACHE_MAX: usize = 50_000;
/// An older total is shown faint and worked out again (spec 6.4).
pub const STALE_AFTER: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Known {
    Fresh(FolderTotal),
    Stale(FolderTotal),
    Unknown,
}

/// One folder's record.
#[derive(Debug, Clone, Copy)]
struct Slot {
    total: FolderTotal,
    at: Instant,
    /// The folder's modified time when it was added up: another one now means something right
    /// in it was added, removed or renamed (sapma 10).
    stamp: Option<SystemTime>,
    /// The key of the folder above it (for F5 on that one).
    parent: u64,
    /// When it was last looked at (`SizeCache::clock`).
    used: u64,
}

/// The totals worked out so far, by folder.
pub struct SizeCache {
    // shortcut: keyed by a 64-bit hash of the path, a collision would show one folder's size
    // for another; key by the path text if that is ever seen (spec 12 wants ≤ 5 MB).
    map: HashMap<u64, Slot>,
    clock: u64,
    max: usize,
}

fn key_of(parts: &[String]) -> u64 {
    let mut hasher = DefaultHasher::new();
    parts.hash(&mut hasher);
    hasher.finish()
}

impl SizeCache {
    pub fn new(max: usize) -> SizeCache {
        SizeCache { map: HashMap::new(), clock: 0, max: max.max(10) }
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// What is known of `folder`, whose modified time is `stamp` now.
    pub fn look(&mut self, folder: &Path, stamp: Option<SystemTime>, now: Instant) -> Known {
        let key = key_of(&path_key(folder));
        self.clock += 1;
        let clock = self.clock;
        let Some(slot) = self.map.get_mut(&key) else { return Known::Unknown };
        if slot.stamp != stamp {
            self.map.remove(&key);
            return Known::Unknown;
        }
        slot.used = clock;
        if now.saturating_duration_since(slot.at) >= STALE_AFTER {
            Known::Stale(slot.total)
        } else {
            Known::Fresh(slot.total)
        }
    }

    /// The total of `folder` as it is kept (the preview), however old.
    pub fn get(&self, folder: &Path) -> Option<FolderTotal> {
        self.map.get(&key_of(&path_key(folder))).map(|slot| slot.total)
    }

    pub fn put(&mut self, folder: &Path, total: FolderTotal, stamp: Option<SystemTime>, now: Instant) {
        let parts = path_key(folder);
        let parent = key_of(&parts[..parts.len().saturating_sub(1)]);
        self.clock += 1;
        self.map.insert(key_of(&parts), Slot { total, at: now, stamp, parent, used: self.clock });
        if self.map.len() > self.max {
            // The least used tenth goes at once, so a full cache does not scan on every put.
            let mut used: Vec<u64> = self.map.values().map(|slot| slot.used).collect();
            let cut = self.map.len() / 10;
            used.select_nth_unstable(cut);
            let keep_from = used[cut];
            self.map.retain(|_, slot| slot.used >= keep_from);
        }
    }

    /// A job changed `paths`: every folder above each (and each itself) is out of date.
    pub fn forget_with_ancestors(&mut self, paths: &[PathBuf]) {
        for path in paths {
            let parts = path_key(path);
            for end in 1..=parts.len() {
                self.map.remove(&key_of(&parts[..end]));
            }
        }
    }

    /// F5 on `folder`: the totals of the folders right in it.
    pub fn forget_children(&mut self, folder: &Path) {
        let parent = key_of(&path_key(folder));
        self.map.retain(|_, slot| slot.parent != parent);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gezik-size-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, bytes: usize) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, vec![b'x'; bytes]).unwrap();
    }

    #[test]
    fn a_folder_is_its_files_added_up() {
        let root = tree("sum");
        write(&root.join("a.txt"), 10);
        write(&root.join("sub/b.txt"), 20);
        write(&root.join("sub/deeper/c.txt"), 30);
        std::fs::create_dir_all(root.join("empty")).unwrap();
        let total = measure(&root, 2, &Arc::default()).unwrap();
        assert_eq!(total, FolderTotal { bytes: 60, counts: Some((3, 3)), partial: false });
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_tree_scan_adds_up_as_measure_does() {
        let root = tree("map");
        write(&root.join("a.txt"), 10);
        write(&root.join("sub/b.txt"), 20);
        write(&root.join("sub/deeper/c.txt"), 30);
        std::fs::create_dir_all(root.join("empty")).unwrap();
        let map = Arc::new(Mutex::new(SizeTree::new("map")));
        assert!(scan_tree(&root, 2, &Arc::default(), map.clone(), Box::new(|| {})));
        let map = map.lock().unwrap();
        let top = map.node(0);
        let measured = measure(&root, 2, &Arc::default()).unwrap();
        assert_eq!((top.bytes, Some((top.files, top.folders)), top.partial), (measured.bytes, measured.counts, false));
        assert_eq!(map.files, [("a.txt".to_owned(), 10)], "the folder's own files, one by one");
        let sub = map.children(0).find(|&c| map.name(c) == "sub").unwrap();
        assert_eq!((map.node(sub).bytes, map.node(sub).own_bytes, map.node(sub).files), (50, 20, 2));
        assert!(map.node(sub).stamp.is_some(), "the cache's stamp");
        assert_eq!(map.len(), 4, "folders only");
        drop(map);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_cloud_folder_counts_in_the_tree_as_in_measure() {
        let builder = TreeBuilder {
            tree: Arc::new(Mutex::new(SizeTree::new("x"))),
            waiting: Mutex::new(HashMap::from([(String::new(), 0)])),
            rules: size_rules(Path::new("x")),
            tick: Box::new(|| {}),
            ticked: Mutex::new(Instant::now()),
        };
        let cloud = DirItem { name: "OneDrive folder".into(), is_dir: true, offline: true, ..file_item(0) };
        let link = DirItem { name: "link".into(), is_dir: true, is_link: true, ..file_item(0) };
        builder.folder(Path::new("x"), "", vec![file_item(5), cloud.clone(), link.clone()]);
        let mut tree = builder.tree.lock().unwrap();
        tree.sum();
        let adder = Adder::default();
        adder.folder(Path::new("x"), "", vec![file_item(5), cloud, link]);
        let measured = adder.total(&walk::WalkStats::default());
        let top = tree.node(0);
        assert_eq!((top.bytes, Some((top.files, top.folders)), top.partial), (measured.bytes, measured.counts, true));
        assert_eq!(tree.len(), 1, "no node for either");
    }

    #[test]
    fn a_cancelled_or_unreadable_tree_scan() {
        let map = Arc::new(Mutex::new(SizeTree::new("x")));
        assert!(!scan_tree(&std::env::temp_dir(), 1, &Arc::new(AtomicBool::new(true)), map, Box::new(|| {})));
        let map = Arc::new(Mutex::new(SizeTree::new("x")));
        let gone = std::env::temp_dir().join("gezik-size-no-such-folder");
        assert!(scan_tree(&gone, 1, &Arc::default(), map.clone(), Box::new(|| {})));
        assert!(map.lock().unwrap().node(0).partial);
    }

    #[test]
    fn a_cancelled_measure_gives_nothing() {
        let root = tree("cancel");
        write(&root.join("a.txt"), 1);
        assert_eq!(measure(&root, 2, &Arc::new(AtomicBool::new(true))), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_folder_that_cannot_be_read_makes_the_total_partial() {
        let total = measure(&std::env::temp_dir().join("gezik-size-no-such-folder"), 1, &Arc::default()).unwrap();
        assert!(total.partial);
        assert_eq!(total.bytes, 0);
    }

    #[test]
    fn a_cloud_folder_makes_the_total_partial() {
        let adder = Adder::default();
        let cloud = DirItem { name: "OneDrive folder".into(), is_dir: true, offline: true, ..file_item(0) };
        adder.folder(Path::new("x"), "", vec![file_item(5), cloud]);
        let total = adder.total(&walk::WalkStats::default());
        assert_eq!(total, FolderTotal { bytes: 5, counts: Some((1, 1)), partial: true });
    }

    fn file_item(size: u64) -> DirItem {
        DirItem {
            name: "f".into(),
            is_dir: false,
            is_link: false,
            is_file: true,
            offline: false,
            flags: 0,
            size,
            modified: None,
            created: None,
            device: 0,
            has_meta: true,
        }
    }

    #[test]
    fn a_placeholder_file_counts_by_its_listed_size() {
        let adder = Adder::default();
        adder.folder(Path::new("x"), "", vec![DirItem { offline: true, ..file_item(1000) }]);
        assert_eq!(adder.total(&walk::WalkStats::default()).bytes, 1000, "never opened, never downloaded");
    }

    #[cfg(unix)]
    #[test]
    fn a_link_loop_is_added_up_once() {
        let root = tree("loop");
        write(&root.join("a/x.txt"), 4);
        std::os::unix::fs::symlink(&root, root.join("a/up")).unwrap();
        let total = measure(&root, 2, &Arc::default()).unwrap();
        // The link is not followed, and its own few bytes are not counted (`read_dir_items`
        // sizes regular files only), so only the file's 4 bytes add up.
        assert_eq!(total.bytes, 4);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(windows)]
    #[test]
    fn a_link_loop_is_added_up_once() {
        use gezik_core::templates::LinkKind;
        let root = tree("junction-loop");
        write(&root.join("a/x.txt"), 4);
        gezik_platform::link::create(LinkKind::Junction, &root, &root.join("a").join("up"), true).unwrap();
        let total = measure(&root, 2, &Arc::default()).unwrap();
        assert_eq!((total.bytes, total.counts), (4, Some((1, 1))), "the junction is neither entered nor a folder");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_cache_knows_fresh_old_and_changed_folders() {
        let mut cache = SizeCache::new(10);
        let now = Instant::now();
        let stamp = Some(SystemTime::UNIX_EPOCH + Duration::from_secs(100));
        let total = FolderTotal { bytes: 9, counts: Some((1, 0)), partial: false };
        cache.put(Path::new("/w/a"), total, stamp, now);
        assert_eq!(cache.look(Path::new("/w/a"), stamp, now), Known::Fresh(total));
        assert_eq!(cache.look(Path::new("/w/a"), stamp, now + STALE_AFTER), Known::Stale(total));
        let touched = Some(SystemTime::UNIX_EPOCH + Duration::from_secs(101));
        assert_eq!(cache.look(Path::new("/w/a"), touched, now), Known::Unknown, "something in it changed");
        assert_eq!(cache.len(), 0, "and the record is gone");
    }

    #[test]
    fn a_job_forgets_every_folder_above_what_it_changed() {
        let mut cache = SizeCache::new(10);
        let now = Instant::now();
        for path in ["/w", "/w/a", "/w/a/b", "/w/c"] {
            cache.put(Path::new(path), FolderTotal::default(), None, now);
        }
        cache.forget_with_ancestors(&[PathBuf::from("/w/a/b/new.txt")]);
        assert_eq!(cache.get(Path::new("/w/c")), Some(FolderTotal::default()));
        for gone in ["/w", "/w/a", "/w/a/b"] {
            assert_eq!(cache.get(Path::new(gone)), None, "{gone}");
        }
    }

    #[test]
    fn refresh_forgets_the_folders_shown() {
        let mut cache = SizeCache::new(10);
        let now = Instant::now();
        for path in ["/w", "/w/a", "/w/b", "/w/a/deep"] {
            cache.put(Path::new(path), FolderTotal::default(), None, now);
        }
        cache.forget_children(Path::new("/w"));
        assert_eq!(cache.len(), 2, "/w and /w/a/deep stay");
        assert!(cache.get(Path::new("/w/a")).is_none() && cache.get(Path::new("/w/a/deep")).is_some());
    }

    #[test]
    fn a_full_cache_drops_the_least_used_tenth() {
        let mut cache = SizeCache::new(20);
        let now = Instant::now();
        for i in 0..20 {
            cache.put(&PathBuf::from(format!("/w/{i}")), FolderTotal::default(), None, now);
        }
        let _ = cache.look(Path::new("/w/0"), None, now);
        cache.put(Path::new("/w/new"), FolderTotal::default(), None, now);
        assert_eq!(cache.len(), 19, "two of 21 go");
        assert!(cache.get(Path::new("/w/0")).is_some(), "the one just looked at stays");
        assert!(cache.get(Path::new("/w/1")).is_none() && cache.get(Path::new("/w/2")).is_none());
    }

    #[test]
    fn a_record_is_small() {
        // 50,000 records within ~5 MB (spec 12) with the map's own overhead.
        assert!(std::mem::size_of::<(u64, Slot)>() <= 96, "{}", std::mem::size_of::<(u64, Slot)>());
    }
}
