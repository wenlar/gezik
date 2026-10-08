//! Watches the folder on screen (not what is inside its subfolders) and says when it changed.
//! Setting a watch up or down happens on a thread of its own, so a network folder that does
//! not answer never holds up the window.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use gezik_config::lock;
use gezik_core::ops::paths::same_path;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

#[derive(Clone)]
pub struct FolderWatch(Arc<Shared>);

struct Shared {
    /// Bumped by every `watch`: events of an older watch are dropped.
    generation: AtomicU64,
    /// A change came that `take_change` has not taken yet.
    changed: AtomicBool,
    /// The watch of the current generation, once set up.
    watcher: Mutex<Option<(u64, RecommendedWatcher)>>,
    /// Called (on the watcher's thread) when the folder changes after `take_change`.
    on_change: Box<dyn Fn() + Send + Sync>,
}

impl FolderWatch {
    pub fn new(on_change: impl Fn() + Send + Sync + 'static) -> FolderWatch {
        FolderWatch(Arc::new(Shared {
            generation: AtomicU64::new(0),
            changed: AtomicBool::new(false),
            watcher: Mutex::default(),
            on_change: Box::new(on_change),
        }))
    }

    /// Watches `folder` from now on (`None`: nothing).
    pub fn watch(&self, folder: Option<&Path>) {
        let generation = self.0.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.0.changed.store(false, Ordering::SeqCst);
        let shared = self.0.clone();
        let folder = folder.map(Path::to_path_buf);
        std::thread::spawn(move || {
            // The old watch goes first (stopping it may wait on its folder). A newer watch
            // started meanwhile owns the slot: leave it (and the dropping) to that one.
            let old = {
                let mut slot = lock(&shared.watcher);
                if shared.generation.load(Ordering::SeqCst) != generation {
                    return;
                }
                slot.take()
            };
            drop(old);
            let Some(folder) = folder else { return };
            if let Some(watcher) = start(&shared, generation, &folder) {
                let mut slot = lock(&shared.watcher);
                if shared.generation.load(Ordering::SeqCst) == generation {
                    *slot = Some((generation, watcher));
                }
            }
        });
    }

    /// Whether the folder changed since the last call; the next change calls `on_change` again.
    pub fn take_change(&self) -> bool {
        self.0.changed.swap(false, Ordering::SeqCst)
    }

    /// Stops watching right away, on this thread: the drive is about to be removed and must
    /// not be held. The watcher's own thread closes its handles a moment later.
    pub fn stop_now(&self) {
        self.0.generation.fetch_add(1, Ordering::SeqCst);
        self.0.changed.store(false, Ordering::SeqCst);
        let watcher = lock(&self.0.watcher).take();
        drop(watcher);
    }
}

/// Watches what is in `folder`, and its parent for the folder itself being deleted or renamed
/// (which changes nothing inside it).
fn start(shared: &Arc<Shared>, generation: u64, folder: &Path) -> Option<RecommendedWatcher> {
    let real = real_path(folder);
    let folder = real.as_path();
    let events = Arc::downgrade(shared);
    let watched = folder.to_path_buf();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
        let Some(shared) = events.upgrade() else { return };
        // An error (lost events) may hide anything.
        let relevant = event.map_or(true, |event| concerns(&event, &watched));
        if relevant
            && shared.generation.load(Ordering::SeqCst) == generation
            && !shared.changed.swap(true, Ordering::SeqCst)
        {
            (shared.on_change)();
        }
    })
    .ok()?;
    // Cannot be watched (no permission, a drive that does not support it): no watch.
    watcher.watch(folder, RecursiveMode::NonRecursive).ok()?;
    if let Some(parent) = folder.parent() {
        // Without it, only a deletion or rename of the folder goes unseen.
        let _ = watcher.watch(parent, RecursiveMode::NonRecursive);
    }
    Some(watcher)
}

/// The path FSEvents reports for `folder`: it names files by their real path, so a folder
/// opened through a symlink (`/tmp` is `/private/tmp`) would never match its own events.
#[cfg(target_os = "macos")]
fn real_path(folder: &Path) -> std::path::PathBuf {
    std::fs::canonicalize(folder).unwrap_or_else(|_| folder.to_path_buf())
}

/// Other systems report the path as it was watched.
#[cfg(not(target_os = "macos"))]
fn real_path(folder: &Path) -> std::path::PathBuf {
    folder.to_path_buf()
}

/// Whether `event` changes what the list of `folder` shows: something in it, or the folder
/// itself (its parent's events about siblings do not). Reading a file changes nothing.
fn concerns(event: &Event, folder: &Path) -> bool {
    if matches!(event.kind, EventKind::Access(_)) {
        return false;
    }
    // No path: say it changed rather than miss something.
    event.paths.is_empty()
        || event
            .paths
            .iter()
            .any(|path| path.parent().is_some_and(|parent| same_path(parent, folder)) || same_path(path, folder))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::mpsc;
    use std::time::Duration;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gezik-watch-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Waits until the watch is set up (it starts on a thread), by touching the folder until a
    /// change shows.
    fn wait_until_watching(watch: &FolderWatch, folder: &Path, rx: &mpsc::Receiver<()>) {
        for i in 0..100 {
            std::fs::write(folder.join(format!("probe{i}")), "").unwrap();
            // A late word from the watch before (sent before `watch` reset the flag) is no
            // sign of this one: only a word with the flag set is.
            if rx.recv_timeout(Duration::from_millis(100)).is_ok() && watch.take_change() {
                // Late events of the probes.
                std::thread::sleep(Duration::from_millis(200));
                while rx.try_recv().is_ok() {}
                watch.take_change();
                return;
            }
        }
        panic!("the watch never started");
    }

    #[test]
    fn a_change_is_told_once_until_taken() {
        let dir = temp("change");
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let watch = FolderWatch::new(move || lock(&tx).send(()).unwrap());
        watch.watch(Some(&dir));
        wait_until_watching(&watch, &dir, &rx);

        std::fs::write(dir.join("a.txt"), "a").unwrap();
        std::fs::write(dir.join("b.txt"), "b").unwrap();
        rx.recv_timeout(Duration::from_secs(5)).expect("told about the change");
        std::thread::sleep(Duration::from_millis(200));
        assert!(rx.try_recv().is_err(), "told once while not taken");
        assert!(watch.take_change());
        std::fs::remove_file(dir.join("a.txt")).unwrap();
        rx.recv_timeout(Duration::from_secs(5)).expect("told again once taken");
        watch.watch(None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_old_folder_says_nothing_once_another_is_watched() {
        let (old, new) = (temp("old"), temp("new"));
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let watch = FolderWatch::new(move || lock(&tx).send(()).unwrap());
        watch.watch(Some(&old));
        wait_until_watching(&watch, &old, &rx);
        watch.watch(Some(&new));
        wait_until_watching(&watch, &new, &rx);

        std::fs::write(old.join("x.txt"), "x").unwrap();
        assert!(rx.recv_timeout(Duration::from_millis(500)).is_err(), "the old folder is not watched");
        assert!(!watch.take_change());
        watch.watch(None);
        let _ = std::fs::remove_dir_all(&old);
        let _ = std::fs::remove_dir_all(&new);
    }

    /// Deleting or renaming the folder itself changes nothing inside it (an empty one says
    /// nothing at all): its parent tells.
    #[test]
    fn the_folder_itself_going_away_is_told() {
        let parent = temp("parent");
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let watch = FolderWatch::new(move || lock(&tx).send(()).unwrap());
        for case in ["deleted", "renamed"] {
            let folder = parent.join(case);
            std::fs::create_dir(&folder).unwrap();
            watch.watch(Some(&folder));
            wait_until_watching(&watch, &folder, &rx);
            for i in 0..100 {
                let _ = std::fs::remove_file(folder.join(format!("probe{i}")));
            }
            std::thread::sleep(Duration::from_millis(300));
            while rx.try_recv().is_ok() {}
            watch.take_change();

            std::fs::write(parent.join("sibling.txt"), case).unwrap();
            assert!(rx.recv_timeout(Duration::from_millis(500)).is_err(), "{case}: a sibling is not the folder");
            if case == "deleted" {
                std::fs::remove_dir(&folder).unwrap();
            } else {
                std::fs::rename(&folder, parent.join("elsewhere")).unwrap();
            }
            rx.recv_timeout(Duration::from_secs(5)).unwrap_or_else(|_| panic!("{case}: told"));
            assert!(watch.take_change());
        }
        watch.watch(None);
        let _ = std::fs::remove_dir_all(&parent);
    }

    /// A folder opened through a symlink (as `/tmp` is on macOS) still hears its changes.
    #[cfg(unix)]
    #[test]
    fn a_folder_opened_through_a_symlink_is_watched() {
        let dir = temp("real");
        let link = temp("link-parent").join("link");
        std::os::unix::fs::symlink(&dir, &link).unwrap();
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let watch = FolderWatch::new(move || lock(&tx).send(()).unwrap());
        watch.watch(Some(&link));
        wait_until_watching(&watch, &link, &rx);

        std::fs::write(link.join("a.txt"), "a").unwrap();
        rx.recv_timeout(Duration::from_secs(5)).expect("told about the change");
        assert!(watch.take_change());
        watch.watch(None);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(link.parent().unwrap());
    }

    #[test]
    fn a_folder_that_cannot_be_watched_is_not() {
        let watch = FolderWatch::new(|| {});
        watch.watch(Some(Path::new("/definitely/not/here/gezik")));
        std::thread::sleep(Duration::from_millis(200));
        assert!(!watch.take_change());
    }
}
