// Hide the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod theme_bridge;
mod watcher;
mod window_state;

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use gezik_config::Warning;
use gezik_config::store::{self, ConfigFiles, ConfigStore};
use gezik_config::theme;
use gezik_core::{Entry, format_size, list_dir};
use slint::{Model, ModelNotify, ModelRc, ModelTracker};

slint::include_modules!();

/// Exposes the listed entries to the UI without copying them into a second list:
/// a `FileRow` is built only when the ListView asks for a row that is on screen.
struct EntryModel {
    entries: Vec<Entry>,
    notify: ModelNotify,
}

impl Model for EntryModel {
    type Data = FileRow;

    fn row_count(&self) -> usize {
        self.entries.len()
    }

    fn row_data(&self, row: usize) -> Option<FileRow> {
        self.entries.get(row).map(|e| FileRow {
            name: e.name.as_str().into(),
            is_dir: e.is_dir,
            size: if e.is_dir { "".into() } else { format_size(e.size).into() },
        })
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

fn entry_at(window: &AppWindow, index: i32) -> Option<Entry> {
    let rows = window.get_rows();
    let model = rows.as_any().downcast_ref::<EntryModel>()?;
    model.entries.get(usize::try_from(index).ok()?).cloned()
}

#[derive(Default)]
struct Nav {
    current: Option<PathBuf>,
    back: Vec<PathBuf>,
}

/// Shared between the UI thread and the background loaders.
#[derive(Clone)]
struct Ctx {
    window: slint::Weak<AppWindow>,
    nav: Arc<Mutex<Nav>>,
    /// Bumped on every navigation so that results of an outdated load are dropped.
    generation: Arc<AtomicU64>,
}

impl Ctx {
    /// Lists `path` on a background thread and shows it when done. The UI thread never
    /// waits on the file system. `record` pushes the current folder onto the back stack.
    fn navigate(&self, path: PathBuf, record: bool) {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        if let Some(window) = self.window.upgrade() {
            window.set_status("Loading…".into());
        }

        let ctx = self.clone();
        std::thread::spawn(move || {
            let result = list_dir(&path);
            let window = ctx.window.clone();
            let _ = window.upgrade_in_event_loop(move |window| {
                if ctx.generation.load(Ordering::SeqCst) != generation {
                    return;
                }
                match result {
                    Ok(entries) => ctx.show(&window, path, entries, record),
                    Err(err) => window.set_status(format!("Cannot open {}: {err}", path.display()).into()),
                }
            });
        });
    }

    fn show(&self, window: &AppWindow, path: PathBuf, entries: Vec<Entry>, record: bool) {
        let mut nav = self.nav.lock().unwrap();
        if record && let Some(previous) = nav.current.take() {
            nav.back.push(previous);
        }

        window.set_status(format!("{} items", entries.len()).into());
        window.set_rows(ModelRc::new(EntryModel { entries, notify: ModelNotify::default() }));
        window.set_current_path(path.display().to_string().into());
        window.set_selected(-1);
        window.set_can_go_back(!nav.back.is_empty());

        nav.current = Some(path);
    }
}

fn home_dir() -> PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// Resolves settings + theme from `files` and shows them. No I/O, so it runs on the UI
/// thread at startup, after config files change and when the system theme flips.
fn apply_config(window: &AppWindow, files: &ConfigFiles) {
    let loaded = store::resolve(files, window.get_system_dark());
    if let Some(theme) = &loaded.theme {
        theme_bridge::apply(window, theme);
    }
    for warning in &loaded.warnings {
        eprintln!("gezik: {warning}");
    }
    window.set_notice(notice_text(&loaded.warnings).into());
}

/// The first warning, plus how many more there are.
fn notice_text(warnings: &[Warning]) -> String {
    match warnings {
        [] => String::new(),
        [only] => only.to_string(),
        [first, rest @ ..] => format!("{first} (+{} more)", rest.len()),
    }
}

/// The native window only exists once the event loop runs (not yet on the first tick),
/// so retry shortly until it does, then move it on screen if needed.
fn keep_on_screen(window: slint::Weak<AppWindow>, attempt: u32) {
    slint::Timer::single_shot(std::time::Duration::from_millis(10), move || {
        let Some(strong) = window.upgrade() else { return };
        if !window_state::ensure_visible(&strong) && attempt < 200 {
            keep_on_screen(window, attempt + 1);
        }
    });
}

fn main() -> Result<(), slint::PlatformError> {
    let window = AppWindow::new()?;

    let config = ConfigStore::system();
    let init_error = config.as_ref().and_then(|store| store.ensure_initialized().err().map(|e| (store, e)));
    let mut files = config.as_ref().map(ConfigStore::read_files).unwrap_or_default();
    if let Some((store, err)) = init_error {
        files.warnings.push(Warning::new(store.dir().display().to_string(), format!("cannot create config folder: {err}")));
    }
    // Release builds hide stderr, so config problems also go to the status bar.
    if config.is_none() {
        eprintln!("gezik: no config folder available; using defaults");
        files.warnings.push(Warning::new("config", "no config folder available; using default settings"));
    }
    // Something sensible is on screen even if the selected theme cannot be read.
    theme_bridge::apply(&window, &theme::builtin_dark());
    apply_config(&window, &files);

    // The latest config files, so a system light/dark switch can re-resolve without I/O.
    let files = Arc::new(Mutex::new(files));

    window.on_system_scheme_changed({
        let weak = window.as_weak();
        let files = files.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                apply_config(&window, &files.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
            }
        }
    });

    // Reading happens on the watcher thread; resolving and applying on the UI thread.
    let files_for_warning = files.clone();
    let _watcher = config.as_ref().and_then(|store| {
        let reader = store.clone();
        let weak = window.as_weak();
        let files = files.clone();
        watcher::watch_config(store, move || {
            let fresh = reader.read_files();
            let files = files.clone();
            let _ = weak.upgrade_in_event_loop(move |window| {
                let mut current = files.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                *current = fresh;
                apply_config(&window, &current);
            });
        })
        .map_err(|err| {
            eprintln!("gezik: cannot watch {}: {err}", store.dir().display());
            let warning = Warning::new("config", format!("cannot watch the config folder for changes: {err}"));
            let mut current = files_for_warning.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            current.warnings.push(warning);
        })
        .ok()
    });
    apply_config(&window, &files.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
    if let Some(store) = &config {
        window_state::restore(&window, &store.load_state());
    }
    keep_on_screen(window.as_weak(), 0);
    window.window().on_close_requested({
        let weak = window.as_weak();
        let store = config.clone();
        move || {
            // A minimized or maximized window has no meaningful normal rect: keep the old state.
            if let (Some(window), Some(store)) = (weak.upgrade(), &store)
                && !window.window().is_minimized()
                && !window.window().is_maximized()
                && let Err(err) = store.save_state(&window_state::capture(&window))
            {
                eprintln!("gezik: cannot save window state: {err}");
            }
            slint::CloseRequestResponse::HideWindow
        }
    });
    let ctx = Ctx {
        window: window.as_weak(),
        nav: Arc::default(),
        generation: Arc::default(),
    };

    window.on_open_row({
        let ctx = ctx.clone();
        move |index| {
            let Some(window) = ctx.window.upgrade() else { return };
            let Some(entry) = entry_at(&window, index) else { return };
            let Some(dir) = ctx.nav.lock().unwrap().current.clone() else { return };
            let path = dir.join(&entry.name);
            if entry.is_dir {
                ctx.navigate(path, true);
            } else if let Err(err) = open::that_detached(&path) {
                window.set_status(format!("Cannot open {}: {err}", entry.name).into());
            }
        }
    });

    window.on_go_back({
        let ctx = ctx.clone();
        move || {
            let previous = ctx.nav.lock().unwrap().back.pop();
            if let Some(path) = previous {
                ctx.navigate(path, false);
            }
        }
    });

    window.on_go_up({
        let ctx = ctx.clone();
        move || {
            let parent = ctx.nav.lock().unwrap().current.as_ref().and_then(|p| p.parent()).map(PathBuf::from);
            if let Some(path) = parent {
                ctx.navigate(path, true);
            }
        }
    });

    window.on_navigate({
        let ctx = ctx.clone();
        move |text| ctx.navigate(PathBuf::from(text.trim()), true)
    });

    let start = std::env::args_os().nth(1).map(PathBuf::from).unwrap_or_else(home_dir);
    ctx.navigate(start, false);
    window.run()
}
