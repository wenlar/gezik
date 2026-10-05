//! Watches the folder on screen (not what is inside its subfolders) and says when it changed.
//! Setting a watch up or down happens on a thread of its own, so a network folder that does
//! not answer never holds up the window.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

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
            // The old watch goes first (stopping it may wait on its folder).
            let old = lock(&shared.watcher).take();
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
}

fn start(shared: &Arc<Shared>, generation: u64, folder: &Path) -> Option<RecommendedWatcher> {
    let events = Arc::downgrade(shared);
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
        let Some(shared) = events.upgrade() else { return };
        // Reading a file changes nothing on screen; an error (lost events) may hide anything.
        let relevant = event.map_or(true, |event| !matches!(event.kind, EventKind::Access(_)));
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
    Some(watcher)
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
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
            if rx.recv_timeout(Duration::from_millis(100)).is_ok() {
                assert!(watch.take_change());
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

    #[test]
    fn a_folder_that_cannot_be_watched_is_not() {
        let watch = FolderWatch::new(|| {});
        watch.watch(Some(Path::new("/definitely/not/here/gezik")));
        std::thread::sleep(Duration::from_millis(200));
        assert!(!watch.take_change());
    }
}
