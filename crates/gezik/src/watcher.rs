//! Watches the config folder and reports bursts of changes as a single reload.

use std::sync::mpsc;
use std::time::Duration;

use gezik_config::store::ConfigStore;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

/// Editors often write a file several times per save; wait this long for quiet.
const DEBOUNCE: Duration = Duration::from_millis(150);

/// Calls `on_change` on a background thread once per burst of changes to
/// `settings.toml` or a theme file. Keep the returned watcher alive while watching.
pub fn watch_config(store: &ConfigStore, on_change: impl Fn() + Send + 'static) -> notify::Result<RecommendedWatcher> {
    let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
    let mut watcher = notify::recommended_watcher(tx)?;
    watcher.watch(store.dir(), RecursiveMode::Recursive)?;

    let store = store.clone();
    std::thread::spawn(move || {
        while let Ok(event) = rx.recv() {
            let relevant = event.is_ok_and(|e| e.paths.iter().any(|path| store.is_config_file(path)));
            if !relevant {
                continue;
            }
            // Swallow the rest of the burst, then reload once.
            while rx.recv_timeout(DEBOUNCE).is_ok() {}
            on_change();
        }
    });
    Ok(watcher)
}
