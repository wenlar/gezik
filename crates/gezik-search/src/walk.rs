//! The scanner (spec 3.4): folders read in parallel from one shared queue by low-priority
//! threads; links never gone into; hidden and skipped folders left out; one file system on
//! Unix; at most `MAX_DEPTH` levels; a cancel seen before every folder. A thread stuck in a
//! folder read (a dead network share) is left behind: `run` returns, and what it reads later
//! is dropped by the visitor.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use gezik_core::pattern::fold_text;
use gezik_platform::fs::{DirItem, describe, read_dir_items};

pub const MAX_DEPTH: usize = 256;
pub const MAX_PROBLEMS_KEPT: usize = 50;
/// How often `run` looks at the cancel flag while it waits.
const WAKE: Duration = Duration::from_millis(20);

/// What the walk goes into.
#[derive(Debug, Clone)]
pub struct WalkRules {
    /// `[view] show-hidden` and `show-system`: items they hide are neither handed over nor gone
    /// into. `None`: everything ("Include hidden items").
    pub shown: Option<(bool, bool)>,
    /// Folder names not gone into (`[search] skip`, folded as the pattern language folds).
    pub skip: Vec<String>,
    /// Unix: the devices a folder may be on (the scope's; on macOS also its data volume).
    /// Empty: no rule.
    pub devices: Vec<u64>,
    pub max_depth: usize,
    /// Whether a folder whose data is not on this disk (a cloud placeholder, an offline folder:
    /// `DirItem::offline`) is gone into. Listing it may fetch it from the cloud: folder sizes
    /// leave it (spec 6, sapma 13); a search goes in, as in 8a.
    pub enter_offline: bool,
}

impl WalkRules {
    pub fn new(shown: Option<(bool, bool)>, skip: &[String], devices: Vec<u64>) -> WalkRules {
        WalkRules {
            shown,
            skip: skip.iter().map(|name| fold_text(name)).collect(),
            devices,
            max_depth: MAX_DEPTH,
            enter_offline: true,
        }
    }

    fn skips(&self, name: &str) -> bool {
        !self.skip.is_empty() && self.skip.contains(&fold_text(name))
    }
}

/// Whether the walk goes into `item`: a folder that is no link (spec 3.4), and no cloud
/// folder unless `rules` allow it.
pub fn goes_into(item: &DirItem, rules: &WalkRules) -> bool {
    item.is_dir && !item.is_link && (rules.enter_offline || !item.offline)
}

/// Whether a folder on `device` is on the scope's file system. With a rule (Unix), an unknown
/// device (0: its `lstat` failed, as on a dead network mount) is not gone into.
pub fn same_device(devices: &[u64], device: u64) -> bool {
    devices.is_empty() || (device != 0 && devices.contains(&device))
}

/// The roots that are not inside another one (Unix "every drive": `/home` is walked from `/`,
/// once); the same root twice is kept once.
pub fn outermost(roots: Vec<PathBuf>) -> Vec<PathBuf> {
    use gezik_core::ops::paths::{is_within, same_path};
    let inside = |i: usize, root: &Path| {
        roots
            .iter()
            .enumerate()
            .any(|(j, other)| j != i && is_within(root, other) && (j < i || !same_path(root, other)))
    };
    roots.iter().enumerate().filter(|(i, root)| !inside(*i, root)).map(|(_, root)| root.clone()).collect()
}

/// macOS: `/System/Volumes/Data` is the second view of the files `/` shows (spec 3.4).
pub fn walked_twice(path: &Path) -> bool {
    path == Path::new("/System/Volumes/Data")
}

/// Folders that could not be read: how many, and the first ones with why.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Problems {
    pub count: usize,
    pub first: Vec<(PathBuf, String)>,
}

#[derive(Debug, Clone, Default)]
pub struct WalkStats {
    pub folders: usize,
    /// Folders `skip` kept out.
    pub skipped: usize,
    pub problems: Problems,
    pub cancelled: bool,
}

/// What the walk hands over.
pub trait Visit: Send + Sync {
    /// Whether an item needs its size and times (Unix reads them one by one).
    fn wants_meta(&self, name: &str, is_dir: bool) -> bool;
    /// The items `rules` show of folder `dir`; `relative`: its path under the root (`""`: the
    /// root itself; every drive: the whole path).
    fn folder(&self, dir: &Path, relative: &str, items: Vec<DirItem>);
    /// Whether the visitor wants no more folders (the name cache past its limit). The walk then
    /// ends as on a cancel, but `WalkStats::cancelled` stays as the caller's flag says.
    fn stopped(&self) -> bool {
        false
    }
}

pub struct Walk {
    pub roots: Vec<PathBuf>,
    /// Every drive: the `relative` paths are whole paths.
    pub absolute: bool,
    pub threads: usize,
    pub rules: WalkRules,
    /// Folders read so far (the status bar's count).
    pub read: Arc<AtomicUsize>,
}

impl Walk {
    pub fn new(roots: Vec<PathBuf>, absolute: bool, threads: usize, rules: WalkRules) -> Walk {
        Walk { roots, absolute, threads: threads.max(1), rules, read: Arc::default() }
    }
}

/// Threads for a walk: 2 on a network folder, else one per core up to 8 (spec 3.4).
pub fn threads_for(network: bool) -> usize {
    if network { 2 } else { std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(2, 8) }
}

struct Job {
    dir: PathBuf,
    relative: String,
    depth: usize,
}

#[derive(Default)]
struct Queue {
    pending: VecDeque<Job>,
    busy: usize,
    done: bool,
}

struct Shared {
    queue: Mutex<Queue>,
    wake: Condvar,
    cancel: Arc<AtomicBool>,
    rules: WalkRules,
    visit: Arc<dyn Visit>,
    read: Arc<AtomicUsize>,
    skipped: AtomicUsize,
    problems: Mutex<Problems>,
}

impl Shared {
    fn problem(&self, path: PathBuf, why: String) {
        let mut problems = self.problems.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        problems.count += 1;
        if problems.first.len() < MAX_PROBLEMS_KEPT {
            problems.first.push((path, why));
        }
    }

    /// Cancelled, or the visitor wants no more.
    fn halted(&self) -> bool {
        self.cancel.load(Ordering::Relaxed) || self.visit.stopped()
    }

    /// The next folder to read (depth first: the newest, so the queue stays short); `None`
    /// once everything is read or the walk is cancelled.
    fn take(&self) -> Option<Job> {
        let mut queue = self.queue.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        loop {
            if queue.done || self.halted() {
                return None;
            }
            if let Some(job) = queue.pending.pop_back() {
                queue.busy += 1;
                return Some(job);
            }
            if queue.busy == 0 {
                queue.done = true;
                self.wake.notify_all();
                return None;
            }
            queue = self.wake.wait_timeout(queue, WAKE).unwrap_or_else(std::sync::PoisonError::into_inner).0;
        }
    }

    fn finished(&self, found: Vec<Job>) {
        let mut queue = self.queue.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        queue.busy -= 1;
        queue.pending.extend(found);
        self.wake.notify_all();
    }

    fn work(&self, job: &Job) -> Vec<Job> {
        if self.halted() {
            return Vec::new();
        }
        let visit = &self.visit;
        let items = match read_dir_items(&job.dir, &|name, is_dir| visit.wants_meta(name, is_dir)) {
            Ok(items) => items,
            Err(err) => {
                self.problem(job.dir.clone(), describe(&err));
                return Vec::new();
            }
        };
        if self.halted() {
            return Vec::new();
        }
        self.read.fetch_add(1, Ordering::Relaxed);
        let rules = &self.rules;
        let mut shown = Vec::with_capacity(items.len());
        let mut below = Vec::new();
        for item in items {
            if let Some((hidden, system)) = rules.shown
                && !gezik_core::is_shown_name(&item.name, item.flags, hidden, system)
            {
                continue;
            }
            if goes_into(&item, rules) {
                let path = job.dir.join(&item.name);
                if rules.skips(&item.name) {
                    self.skipped.fetch_add(1, Ordering::Relaxed);
                } else if job.depth >= rules.max_depth {
                    self.problem(path, "Too deep".to_owned());
                } else if same_device(&rules.devices, item.device)
                    && !(cfg!(target_os = "macos") && walked_twice(&path))
                {
                    let relative = crate::results::relative_key(&job.relative, &item.name);
                    below.push(Job { dir: path, relative, depth: job.depth + 1 });
                }
            }
            shown.push(item);
        }
        self.visit.folder(&job.dir, &job.relative, shown);
        below
    }
}

/// Walks `walk.roots` on `walk.threads` low-priority threads, handing each folder's items to
/// `visit`. Returns when every folder is read or soon after `cancel` is set.
pub fn run(walk: Walk, cancel: Arc<AtomicBool>, visit: Arc<dyn Visit>) -> WalkStats {
    let roots: VecDeque<Job> = walk
        .roots
        .iter()
        .map(|root| Job {
            dir: root.clone(),
            relative: if walk.absolute { root.display().to_string() } else { String::new() },
            depth: 0,
        })
        .collect();
    let shared = Arc::new(Shared {
        queue: Mutex::new(Queue { pending: roots, busy: 0, done: false }),
        wake: Condvar::new(),
        cancel: cancel.clone(),
        rules: walk.rules,
        visit,
        read: walk.read,
        skipped: AtomicUsize::new(0),
        problems: Mutex::default(),
    });
    let mut started = 0;
    for _ in 0..walk.threads {
        let shared = shared.clone();
        let spawned = std::thread::Builder::new().name("gezik-search".into()).spawn(move || {
            gezik_platform::priority::lower_this_thread();
            while let Some(job) = shared.take() {
                // A panic in one folder (a visitor's bug) must not leave it busy forever.
                let below = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| shared.work(&job))) {
                    Ok(below) => below,
                    Err(_) => {
                        shared.problem(job.dir.clone(), "The search failed in this folder".to_owned());
                        Vec::new()
                    }
                };
                shared.finished(below);
            }
        });
        if spawned.is_err() {
            break;
        }
        started += 1;
    }
    {
        let mut queue = shared.queue.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        // No thread to read anything: nothing would ever finish the queue.
        if started == 0 {
            queue.done = true;
            let root = walk.roots.first().cloned().unwrap_or_default();
            shared.problem(root, "Could not start the search".to_owned());
        }
        while !(queue.done || queue.pending.is_empty() && queue.busy == 0) && !shared.halted() {
            queue = shared.wake.wait_timeout(queue, WAKE).unwrap_or_else(std::sync::PoisonError::into_inner).0;
        }
        queue.done = true;
        shared.wake.notify_all();
    }
    let problems = shared.problems.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone();
    WalkStats {
        folders: shared.read.load(Ordering::Relaxed),
        skipped: shared.skipped.load(Ordering::Relaxed),
        problems,
        cancelled: cancel.load(Ordering::Relaxed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    fn tree(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gezik-walk-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "x").unwrap();
    }

    /// Every item seen, as `relative/name`.
    #[derive(Default)]
    struct Seen(Mutex<Vec<String>>, Duration);

    impl Visit for Seen {
        fn wants_meta(&self, _: &str, _: bool) -> bool {
            true
        }
        fn folder(&self, _: &Path, relative: &str, items: Vec<DirItem>) {
            std::thread::sleep(self.1);
            let mut seen = self.0.lock().unwrap();
            for item in items {
                seen.push(crate::results::relative_key(relative, &item.name).replace('\\', "/"));
            }
        }
    }

    fn walk_all(root: &Path, rules: WalkRules) -> (Vec<String>, WalkStats) {
        let seen = Arc::new(Seen::default());
        let stats = run(Walk::new(vec![root.to_path_buf()], false, 4, rules), Arc::default(), seen.clone());
        let mut names = seen.0.lock().unwrap().clone();
        names.sort();
        (names, stats)
    }

    fn rules(skip: &[&str]) -> WalkRules {
        let skip: Vec<String> = skip.iter().map(|s| (*s).to_owned()).collect();
        WalkRules::new(Some((true, false)), &skip, Vec::new())
    }

    fn item(is_dir: bool, is_link: bool, offline: bool) -> DirItem {
        DirItem {
            name: "x".into(),
            is_dir,
            is_link,
            is_file: !is_dir && !is_link,
            offline,
            flags: 0,
            size: 0,
            modified: None,
            created: None,
            device: 0,
            has_meta: true,
        }
    }

    #[test]
    fn a_cloud_folder_is_not_gone_into() {
        let mut rules = rules(&[]);
        assert!(goes_into(&item(true, false, false), &rules));
        assert!(goes_into(&item(true, false, true), &rules), "a search goes into it, as in 8a");
        rules.enter_offline = false;
        assert!(!goes_into(&item(true, false, true), &rules), "folder sizes never fill a cloud folder");
        assert!(!goes_into(&item(true, true, false), &rules), "never a link");
        assert!(!goes_into(&item(false, false, false), &rules));
    }

    #[test]
    fn skipped_folders_are_named_but_not_entered() {
        let root = tree("skip");
        write(&root.join(".git/config"));
        write(&root.join("src/node_modules/x/index.js"));
        write(&root.join("src/main.rs"));
        let (names, stats) = walk_all(&root, rules(&[".git", "NODE_MODULES"]));
        assert_eq!(names, [".git", "src", "src/main.rs", "src/node_modules"]);
        assert_eq!(stats.skipped, 2);
        let (all, stats) = walk_all(&root, rules(&[]));
        assert!(
            all.contains(&"src/node_modules/x/index.js".to_owned()) && stats.skipped == 0,
            "Include skipped folders"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn hidden_items_follow_the_view() {
        let root = tree("hidden");
        write(&root.join(".cache/x.txt"));
        write(&root.join("a.txt"));
        let hide = WalkRules::new(Some((false, false)), &[], Vec::new());
        assert_eq!(walk_all(&root, hide).0, ["a.txt"], "a dot folder is neither shown nor entered");
        let all = WalkRules::new(None, &[], Vec::new());
        assert_eq!(walk_all(&root, all).0, [".cache", ".cache/x.txt", "a.txt"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn depth_is_capped() {
        let root = tree("depth");
        write(&root.join("a/b/c/d/x.txt"));
        let mut capped = rules(&[]);
        capped.max_depth = 2;
        let (names, stats) = walk_all(&root, capped);
        assert_eq!(names, ["a", "a/b", "a/b/c"], "the third level is named, not entered");
        assert_eq!(stats.problems.count, 1);
        assert_eq!(stats.problems.first[0].1, "Too deep");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_folder_that_cannot_be_read_is_counted() {
        let root = tree("unreadable");
        let (names, stats) = walk_all(&root.join("missing"), rules(&[]));
        assert!(names.is_empty());
        assert_eq!((stats.problems.count, stats.folders), (1, 0));
        assert_eq!(stats.problems.first[0].0, root.join("missing"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn a_link_loop_ends() {
        let root = tree("loop");
        write(&root.join("a/x.txt"));
        std::os::unix::fs::symlink(&root, root.join("a/up")).unwrap();
        let (names, _) = walk_all(&root, rules(&[]));
        assert_eq!(names, ["a", "a/up", "a/x.txt"], "the link is a result, never entered");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(windows)]
    #[test]
    fn a_junction_loop_ends() {
        use gezik_core::templates::LinkKind;
        let root = tree("junction-loop");
        write(&root.join("a/x.txt"));
        gezik_platform::link::create(LinkKind::Junction, &root, &root.join("a").join("up"), true).unwrap();
        let (names, _) = walk_all(&root, rules(&[]));
        assert_eq!(names, ["a", "a/up", "a/x.txt"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn allowed_devices_keep_one_file_system() {
        assert!(same_device(&[], 99), "no rule (Windows)");
        assert!(!same_device(&[7], 0), "a folder whose lstat failed (a dead mount)");
        assert!(same_device(&[], 0), "no rule: a device is not needed");
        assert!(same_device(&[7, 9], 9), "macOS: the data volume of /");
        assert!(!same_device(&[7], 8), "/proc, a mounted disk");
    }

    #[test]
    fn the_data_volume_is_not_walked_twice() {
        assert!(walked_twice(Path::new("/System/Volumes/Data")));
        assert!(!walked_twice(Path::new("/System/Volumes")));
        assert!(!walked_twice(Path::new("/Users/a/System/Volumes/Data")));
    }

    #[test]
    fn a_root_inside_another_is_walked_from_it() {
        let paths = |list: &[&str]| list.iter().map(PathBuf::from).collect::<Vec<_>>();
        assert_eq!(outermost(paths(&["/", "/home", "/mnt/data"])), paths(&["/"]), "Unix: every drive is under /");
        assert_eq!(outermost(paths(&["/home", "/"])), paths(&["/"]));
        assert_eq!(
            outermost(paths(&["/a", "/a", "/ab"])),
            paths(&["/a", "/ab"]),
            "the same root once; /ab is not in /a"
        );
        if cfg!(windows) {
            assert_eq!(outermost(paths(&[r"C:\", r"D:\"])), paths(&[r"C:\", r"D:\"]));
        }
    }

    /// Panics in the folder named `boom`.
    struct Panics;

    impl Visit for Panics {
        fn wants_meta(&self, _: &str, _: bool) -> bool {
            true
        }
        fn folder(&self, dir: &Path, _: &str, _: Vec<DirItem>) {
            assert!(dir.file_name().is_none_or(|name| name != "boom"), "a visitor's bug");
        }
    }

    #[test]
    fn a_panic_in_one_folder_ends_as_a_problem() {
        let root = tree("panic");
        write(&root.join("boom/x.txt"));
        write(&root.join("fine/y.txt"));
        let started = Instant::now();
        let stats = run(Walk::new(vec![root.clone()], false, 2, rules(&[])), Arc::default(), Arc::new(Panics));
        assert!(started.elapsed() < Duration::from_secs(5), "the walk ends");
        assert_eq!(stats.problems.count, 1);
        assert_eq!(stats.problems.first[0], (root.join("boom"), "The search failed in this folder".to_owned()));
        assert!(!stats.cancelled);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Wants no more after the first folder.
    #[derive(Default)]
    struct StopsAtOnce(AtomicUsize);

    impl Visit for StopsAtOnce {
        fn wants_meta(&self, _: &str, _: bool) -> bool {
            false
        }
        fn folder(&self, _: &Path, _: &str, _: Vec<DirItem>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
        fn stopped(&self) -> bool {
            self.0.load(Ordering::SeqCst) > 0
        }
    }

    #[test]
    fn a_visitor_can_stop_the_walk_without_cancelling_it() {
        let root = tree("stop");
        for i in 0..20 {
            write(&root.join(format!("d{i}/x.txt")));
        }
        let visit = Arc::new(StopsAtOnce::default());
        let cancel = Arc::new(AtomicBool::new(false));
        let stats = run(Walk::new(vec![root.clone()], false, 1, rules(&[])), cancel.clone(), visit.clone());
        assert_eq!(visit.0.load(Ordering::SeqCst), 1);
        assert!(!stats.cancelled && !cancel.load(Ordering::SeqCst), "the caller's flag is not touched");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A busy machine (a build, a game) may hold a thread back for a while: the normal run
    /// checks that a cancel stops the walk with a wide bound; the spec's 100 ms is checked by
    /// `cargo test -- --ignored` on a quiet machine.
    #[test]
    fn cancelling_stops_the_walk() {
        cancel_within("cancel", Duration::from_secs(2));
    }

    #[test]
    #[ignore = "timing: run on a quiet machine"]
    fn cancelling_stops_within_a_tenth_of_a_second() {
        cancel_within("cancel-strict", Duration::from_millis(100));
    }

    fn cancel_within(name: &str, bound: Duration) {
        let root = tree(name);
        for i in 0..300 {
            write(&root.join(format!("d{i}/x.txt")));
        }
        let seen = Arc::new(Seen(Mutex::default(), Duration::from_millis(5)));
        let cancel = Arc::new(AtomicBool::new(false));
        let walk = Walk::new(vec![root.clone()], false, 4, rules(&[]));
        let (flag, started) = (cancel.clone(), Instant::now());
        let stopper = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            flag.store(true, Ordering::SeqCst);
            Instant::now()
        });
        let stats = run(walk, cancel, seen);
        let returned = Instant::now();
        let cancelled_at = stopper.join().unwrap();
        assert!(stats.cancelled);
        assert!(returned.duration_since(cancelled_at) < bound, "{:?}", returned - cancelled_at);
        assert!(returned.duration_since(started) < bound + Duration::from_secs(1));
        let _ = std::fs::remove_dir_all(&root);
    }
}
