//! Watches the config folder and reports bursts of changes as a single reload. With a sync
//! folder (spec 14 §8) also its parent folder (to see it come back) and this computer's folder
//! (settings.local.toml); nothing polls.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gezik_config::lock;
use gezik_config::store::ConfigStore;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

/// Editors often write a file several times per save; wait this long for quiet.
const DEBOUNCE: Duration = Duration::from_millis(150);

type Reload = Arc<dyn Fn() + Send + Sync>;

struct Watch {
    watcher: RecommendedWatcher,
    store: ConfigStore,
    /// The folders watched, and whether with their subfolders.
    watched: Vec<(PathBuf, bool)>,
    /// (real path, as given) of the watched folders where they differ: FSEvents names the real
    /// folder (`/private/tmp/…` for `/tmp/…` on macOS); its events are read back as given.
    real: Arc<Mutex<Vec<(PathBuf, PathBuf)>>>,
    reload: Reload,
}

static WATCH: Mutex<Option<Watch>> = Mutex::new(None);

/// Calls `on_change` on a background thread once per burst of changes to `settings.toml` or a
/// theme file, and `on_templates` once per burst in the templates folder. Watches for the rest
/// of the process.
pub fn watch_config(
    store: &ConfigStore,
    on_change: impl Fn() + Send + Sync + 'static,
    on_templates: impl Fn() + Send + 'static,
) -> notify::Result<()> {
    let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
    let watcher = notify::recommended_watcher(tx)?;
    let real: Arc<Mutex<Vec<(PathBuf, PathBuf)>>> = Arc::default();
    let reload: Reload = Arc::new(on_change);
    let watch =
        Watch { watcher, store: store.clone(), watched: Vec::new(), real: real.clone(), reload: reload.clone() };
    *lock(&WATCH) = Some(watch);
    follow()?;

    let store = store.clone();
    std::thread::spawn(move || {
        let as_given = |path: &Path| {
            let real = lock(&real);
            let found = real.iter().find_map(|(real, given)| path.strip_prefix(real).ok().map(|rest| given.join(rest)));
            found.unwrap_or_else(|| path.to_path_buf())
        };
        // (settings or a theme, a template) a change touched.
        // Opens and closes are no change: inotify reports the reload's own reads, which would
        // start the next reload.
        let touched = |event: &notify::Result<notify::Event>| match event {
            Ok(e) if !matches!(e.kind, EventKind::Access(_)) => (
                e.paths.iter().any(|path| store.is_config_file(&as_given(path))),
                e.paths.iter().any(|path| store.is_template_path(&as_given(path))),
            ),
            _ => (false, false),
        };
        while let Ok(event) = rx.recv() {
            let (mut config, mut templates) = touched(&event);
            if !config && !templates {
                continue;
            }
            // Gather the rest of the burst, then reload once.
            while let Ok(event) = rx.recv_timeout(DEBOUNCE) {
                let (c, t) = touched(&event);
                config |= c;
                templates |= t;
            }
            if config {
                reload();
            }
            if templates {
                on_templates();
            }
        }
    });
    Ok(())
}

/// Watches what the store needs now: the shared folder and its subfolders when it is there; with
/// a sync folder also its parent (it comes back) and this computer's folder. After a reload,
/// and after the shared folder moved. The first folder that cannot be watched is the error.
pub fn follow() -> notify::Result<()> {
    let mut guard = lock(&WATCH);
    let Some(watch) = guard.as_mut() else { return Ok(()) };
    let dirs = watch.store.dirs();
    let mut wanted = Vec::new();
    if dirs.shared.is_dir() || !dirs.synced {
        wanted.push((dirs.shared.clone(), true));
    }
    if dirs.synced {
        wanted.extend(dirs.shared.parent().filter(|p| p.is_dir()).map(|p| (p.to_path_buf(), false)));
        wanted.push((dirs.local.clone(), false));
    }
    for (path, _) in watch.watched.iter().filter(|w| !wanted.contains(w)) {
        let _ = watch.watcher.unwatch(path);
    }
    let mut failed = None;
    let mut watched = Vec::new();
    for (path, recursive) in wanted {
        let mode = if recursive { RecursiveMode::Recursive } else { RecursiveMode::NonRecursive };
        let there = watch.watched.contains(&(path.clone(), recursive));
        match if there { Ok(()) } else { watch.watcher.watch(&path, mode) } {
            Ok(()) => watched.push((path, recursive)),
            Err(err) => {
                failed.get_or_insert(err);
            }
        }
    }
    *lock(&watch.real) = watched
        .iter()
        .filter_map(|(given, _)| {
            std::fs::canonicalize(given).ok().filter(|real| real != given).map(|real| (real, given.clone()))
        })
        .collect();
    watch.watched = watched;
    failed.map_or(Ok(()), Err)
}

/// Reads the config again now, on a thread (the shared folder moved; Check Again).
pub fn reload() {
    let Some(reload) = lock(&WATCH).as_ref().map(|w| w.reload.clone()) else { return };
    let _ = std::thread::Builder::new().name("gezik-config-reload".into()).spawn(move || reload());
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[test]
    fn a_reload_does_not_wake_the_watcher_again() {
        let root = std::env::temp_dir().join(format!("gezik-watch-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (shared, local) = (root.join("Sync").join("Gezik"), root.join("local"));
        std::fs::create_dir_all(&shared).unwrap();
        std::fs::create_dir_all(&local).unwrap();
        std::fs::write(shared.join("settings.toml"), "theme = \"dark\"\n").unwrap();
        let dirs = gezik_config::paths::Dirs { shared: shared.clone(), local, portable: false, synced: true };
        let store = ConfigStore::with_dirs(dirs);
        let reloads = Arc::new(AtomicUsize::new(0));
        let (count, reader) = (reloads.clone(), store.clone());
        // As main's reload: read (keeping the last copy), write what waits, watch again.
        watch_config(
            &store,
            move || {
                count.fetch_add(1, Ordering::SeqCst);
                let _ = reader.read_files();
                reader.retry_unsaved();
                let _ = follow();
            },
            || {},
        )
        .unwrap();
        std::thread::sleep(Duration::from_millis(300));
        store.update_state(|state| state.preview_open = true);
        store.flush_state();
        std::fs::write(shared.join("settings.toml"), "theme = \"nord\"\n").unwrap();
        std::thread::sleep(Duration::from_secs(2));
        let seen = reloads.load(Ordering::SeqCst);
        assert!((1..=2).contains(&seen), "{seen} reloads for one change");
        let _ = std::fs::remove_dir_all(&root);
    }
}
