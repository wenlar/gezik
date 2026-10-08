//! Watches the config folder and reports bursts of changes as a single reload.

use std::sync::mpsc;
use std::time::Duration;

use gezik_config::store::ConfigStore;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

/// Editors often write a file several times per save; wait this long for quiet.
const DEBOUNCE: Duration = Duration::from_millis(150);

/// Calls `on_change` on a background thread once per burst of changes to `settings.toml` or a
/// theme file, and `on_templates` once per burst in the templates folder. Keep the returned
/// watcher alive while watching.
pub fn watch_config(
    store: &ConfigStore,
    on_change: impl Fn() + Send + 'static,
    on_templates: impl Fn() + Send + 'static,
) -> notify::Result<RecommendedWatcher> {
    let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
    let mut watcher = notify::recommended_watcher(tx)?;
    watcher.watch(store.dir(), RecursiveMode::Recursive)?;

    let store = store.clone();
    std::thread::spawn(move || {
        // (settings or a theme, a template) a change touched.
        let touched = |event: &notify::Result<notify::Event>| match event {
            Ok(e) => (
                e.paths.iter().any(|path| store.is_config_file(path)),
                e.paths.iter().any(|path| store.is_template_path(path)),
            ),
            Err(_) => (false, false),
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
                on_change();
            }
            if templates {
                on_templates();
            }
        }
    });
    Ok(watcher)
}
