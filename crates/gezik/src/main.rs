// Hide the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

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

fn main() -> Result<(), slint::PlatformError> {
    let window = AppWindow::new()?;
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
