//! The sidebar: known folders, pinned folders and drives, kept up to date.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use gezik_config::Warning;
use gezik_config::paths::KnownDirs;
use gezik_config::store::ConfigStore;
use gezik_core::nav::Location;
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::navigation::{Navigator, sync_model};
use crate::{AppWindow, SidebarRow};

pub const SECTION_FOLDERS: i32 = 0;
pub const SECTION_PINNED: i32 = 1;
pub const SECTION_DRIVES: i32 = 2;

/// How often the (cheap) drive signature is checked.
const DRIVE_POLL: Duration = Duration::from_secs(3);

/// Adds `entry` (tokenized) unless an equal entry is already pinned (ignoring case on
/// Windows). Returns whether it was added.
pub fn pin_entry(pinned: &mut Vec<String>, entry: String) -> bool {
    let same = |a: &str, b: &str| if cfg!(windows) { a.eq_ignore_ascii_case(b) } else { a == b };
    if pinned.iter().any(|p| same(p, &entry)) {
        return false;
    }
    pinned.push(entry);
    true
}

/// Moves pinned item `from` to `to` (clamped). An out-of-range `from` does nothing.
pub fn move_entry(pinned: &mut [String], from: usize, to: usize) {
    if from >= pinned.len() {
        return;
    }
    let to = to.min(pinned.len() - 1);
    if from < to {
        pinned[from..=to].rotate_left(1);
    } else {
        pinned[to..=from].rotate_right(1);
    }
}

struct Inner {
    window: slint::Weak<AppWindow>,
    nav: Navigator,
    store: Option<ConfigStore>,
    dirs: KnownDirs,
    /// As written in settings.toml.
    pinned: Vec<String>,
    /// Pinned entries that exist on this machine: (index into `pinned`, path).
    visible_pinned: Vec<(usize, PathBuf)>,
    drive_signature: u64,
    /// The rows' model, updated in place (see [`sync_model`]).
    rows: Rc<VecModel<SidebarRow>>,
    poll: slint::Timer,
}

thread_local! {
    /// The sidebar of this (UI) thread, so the config watcher can reach it.
    static CURRENT: RefCell<Option<Sidebar>> = const { RefCell::new(None) };
}

/// Runs `f` with this UI thread's sidebar, if one is installed.
pub fn with_current(f: impl FnOnce(&Sidebar)) {
    if let Some(sidebar) = CURRENT.with(|c| c.borrow().clone()) {
        f(&sidebar);
    }
}

#[derive(Clone)]
pub struct Sidebar(Rc<RefCell<Inner>>);

impl Sidebar {
    pub fn new(window: &AppWindow, nav: Navigator, store: Option<ConfigStore>) -> Sidebar {
        let rows = Rc::new(VecModel::default());
        window.set_sidebar_rows(ModelRc::from(rows.clone()));
        let sidebar = Sidebar(Rc::new(RefCell::new(Inner {
            window: window.as_weak(),
            nav: nav.clone(),
            store,
            dirs: KnownDirs::system(),
            pinned: Vec::new(),
            visible_pinned: Vec::new(),
            drive_signature: gezik_platform::drive_signature(),
            rows,
            poll: slint::Timer::default(),
        })));
        // Re-highlight on location changes, relabel after places reload. No I/O here: this
        // runs on every navigation.
        let weak = Rc::downgrade(&sidebar.0);
        nav.on_changed(move |_| {
            if let Some(inner) = weak.upgrade() {
                Sidebar(inner).update_rows();
            }
        });
        sidebar.start_drive_polling();
        sidebar.update_rows();
        sidebar
    }

    /// Makes this sidebar reachable from the config watcher. Call once, right after `new`.
    pub fn install(&self) {
        CURRENT.with(|c| *c.borrow_mut() = Some(self.clone()));
    }

    /// Checks the drive signature (cheap) every few seconds; reloads places only if it
    /// changed.
    fn start_drive_polling(&self) {
        let weak = Rc::downgrade(&self.0);
        self.0.borrow().poll.start(slint::TimerMode::Repeated, DRIVE_POLL, move || {
            let Some(inner) = weak.upgrade() else { return };
            let signature = gezik_platform::drive_signature();
            let (changed, window) = {
                let mut inner = inner.borrow_mut();
                let changed = signature != inner.drive_signature;
                inner.drive_signature = signature;
                (changed, inner.window.clone())
            };
            if changed {
                // A pinned folder may have appeared or gone with the drive.
                Sidebar(inner).refresh();
                crate::places::load_in_background(window, |places| {
                    crate::navigation::with_current(|nav| nav.set_places(places));
                });
            }
        });
    }

    /// Sets the pinned entries as written in settings.toml. Does nothing if they are
    /// unchanged, so the settings reload that follows our own save costs nothing.
    pub fn set_pinned(&self, pinned: Vec<String>) {
        if self.0.borrow().pinned == pinned {
            return;
        }
        self.0.borrow_mut().pinned = pinned;
        self.refresh();
    }

    pub fn pinned(&self) -> Vec<String> {
        self.0.borrow().pinned.clone()
    }

    /// Whether `path` is shown in the PINNED section.
    #[allow(dead_code)] // Used by the context menus (Task 10).
    pub fn is_pinned(&self, path: &Path) -> bool {
        self.0.borrow().visible_pinned.iter().any(|(_, p)| p == path)
    }

    #[allow(dead_code)] // Used by the context menus (Task 10).
    pub fn pin(&self, path: PathBuf) {
        let entry = self.0.borrow().dirs.collapse(&path);
        let mut pinned = self.pinned();
        if pin_entry(&mut pinned, entry) {
            self.save(pinned);
        }
    }

    /// Unpins the visible pinned row `index` (index within the PINNED section).
    #[allow(dead_code)] // Used by the sidebar menu (Task 10).
    pub fn unpin(&self, index: usize) {
        let stored = self.0.borrow().visible_pinned.get(index).map(|(stored, _)| *stored);
        let Some(stored) = stored else { return };
        let mut pinned = self.pinned();
        if stored < pinned.len() {
            pinned.remove(stored);
            self.save(pinned);
        }
    }

    /// Moves visible pinned row `from` to visible position `to` (clamped).
    pub fn move_pinned(&self, from: usize, to: usize) {
        let stored = {
            let inner = self.0.borrow();
            let visible = &inner.visible_pinned;
            let to = to.min(visible.len().saturating_sub(1));
            visible.get(from).zip(visible.get(to)).map(|((a, _), (b, _))| (*a, *b))
        };
        let Some((from, to)) = stored else { return };
        if from == to {
            return;
        }
        let mut pinned = self.pinned();
        move_entry(&mut pinned, from, to);
        self.save(pinned);
    }

    /// Writes `pinned` to settings.toml and shows it at once; the config watcher reloads
    /// settings afterwards with the same list, which `set_pinned` ignores.
    fn save(&self, pinned: Vec<String>) {
        let store = self.0.borrow().store.clone();
        let result = match &store {
            Some(store) => store.save_pinned(&pinned),
            None => Err(Warning::new("settings.toml", "no config folder; pins are not saved")),
        };
        match result {
            Ok(()) => self.set_pinned(pinned),
            Err(warning) => {
                let window = self.0.borrow().window.upgrade();
                if let Some(window) = window {
                    window.set_status(warning.to_string().into());
                }
            }
        }
    }

    /// Where sidebar item (`section`, `index`) leads.
    pub fn location_of(&self, section: i32, index: i32) -> Option<Location> {
        let inner = self.0.borrow();
        let index = usize::try_from(index).ok()?;
        let places = inner.nav.places();
        match section {
            SECTION_FOLDERS => places.known.get(index).map(|f| Location::Path(f.path.clone())),
            SECTION_PINNED => inner.visible_pinned.get(index).map(|(_, p)| Location::Path(p.clone())),
            SECTION_DRIVES => places.drives.get(index).map(|d| Location::Path(d.path.clone())),
            _ => None,
        }
    }

    /// Checks which pinned entries exist on this machine (file system access), then
    /// rebuilds the rows.
    pub fn refresh(&self) {
        {
            let mut guard = self.0.borrow_mut();
            let inner = &mut *guard;
            inner.visible_pinned = inner
                .pinned
                .iter()
                .enumerate()
                .filter_map(|(i, text)| inner.dirs.expand_checked(text).filter(|p| p.is_dir()).map(|p| (i, p)))
                .collect();
        }
        self.update_rows();
    }

    /// Rebuilds the rows: sections, labels and the highlight of the exact current location.
    /// No file system access.
    fn update_rows(&self) {
        let inner = self.0.borrow();
        let Some(window) = inner.window.upgrade() else { return };
        let places = inner.nav.places();
        let current = inner.nav.active_location();
        let is_current = |path: &Path| matches!(&current, Location::Path(p) if p == path);
        let index = |i: usize| i32::try_from(i).unwrap_or(i32::MAX);
        let header =
            |label: &str, section| SidebarRow { header: true, label: label.into(), section, index: -1, active: false };
        let item = |label: &str, section, i, path: &Path| SidebarRow {
            header: false,
            label: label.into(),
            section,
            index: index(i),
            active: is_current(path),
        };

        let mut rows = vec![header("FOLDERS", SECTION_FOLDERS)];
        rows.extend(places.known.iter().enumerate().map(|(i, f)| item(&f.name, SECTION_FOLDERS, i, &f.path)));
        let mut pinned_first_row = -1;
        if !inner.visible_pinned.is_empty() {
            rows.push(header("PINNED", SECTION_PINNED));
            pinned_first_row = index(rows.len());
            rows.extend(inner.visible_pinned.iter().enumerate().map(|(i, (_, path))| {
                let label = places.title_for(&Location::Path(path.clone()));
                item(&label, SECTION_PINNED, i, path)
            }));
        }
        rows.push(header("DRIVES", SECTION_DRIVES));
        rows.extend(places.drives.iter().enumerate().map(|(i, d)| item(&d.label, SECTION_DRIVES, i, &d.path)));

        window.set_sidebar_pinned_first_row(pinned_first_row);
        window.set_sidebar_pinned_count(index(inner.visible_pinned.len()));
        sync_model(&inner.rows, rows.into_iter());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_is_idempotent() {
        let mut pinned = vec!["/a".to_owned()];
        assert!(!pin_entry(&mut pinned, "/a".to_owned()));
        assert!(pin_entry(&mut pinned, "/b".to_owned()));
        assert_eq!(pinned, ["/a", "/b"]);
    }

    #[cfg(windows)]
    #[test]
    fn pin_ignores_case_on_windows() {
        let mut pinned = vec!["C:/Work".to_owned()];
        assert!(!pin_entry(&mut pinned, "c:/work".to_owned()));
    }

    #[test]
    fn move_entry_reorders() {
        let mut pinned = vec!["a".to_owned(), "b".to_owned(), "c".to_owned()];
        move_entry(&mut pinned, 0, 2);
        assert_eq!(pinned, ["b", "c", "a"]);
        move_entry(&mut pinned, 2, 0);
        assert_eq!(pinned, ["a", "b", "c"]);
        move_entry(&mut pinned, 1, 99);
        assert_eq!(pinned, ["a", "c", "b"]);
        move_entry(&mut pinned, 9, 0);
        assert_eq!(pinned, ["a", "c", "b"]);
    }
}
