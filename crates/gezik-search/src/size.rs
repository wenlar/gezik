//! Folder sizes (spec 6): a folder's files added up by the walk (links never followed, cloud
//! folders never gone into, one file system on Unix, low priority), and the totals kept in
//! memory: at most `CACHE_MAX` folders, the least used dropped first, never written to disk.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime};

use gezik_core::ops::paths::path_key;
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
    // Unix: the folder's own file system only (spec 3.4); unknown: no rule, the reads fail.
    let devices = if cfg!(unix) {
        gezik_platform::fs::device_of(folder).map(|d| vec![d]).unwrap_or_default()
    } else {
        Vec::new()
    };
    let mut rules = WalkRules::new(None, &[], devices);
    rules.enter_offline = false;
    let adder = Arc::new(Adder::default());
    let stats = walk::run(Walk::new(vec![folder.to_path_buf()], false, threads, rules), cancel.clone(), adder.clone());
    (!stats.cancelled).then(|| adder.total(&stats))
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
        assert_eq!(total.bytes, 4 + std::fs::symlink_metadata(root.join("a/up")).unwrap().len());
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
