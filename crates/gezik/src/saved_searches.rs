//! Saved searches (`[[searches]]`, spec 8): run from the search bar's ▾ menu, the palette and the
//! sidebar's SEARCHES; saved with "Save search…" (a name, then this folder or `{here}`). Written
//! through settings.toml's one writer; entries written by hand that do not read are kept.

use std::cell::RefCell;
use std::rc::Rc;

use gezik_config::Warning;
use gezik_config::paths::KnownDirs;
use gezik_config::settings::{HERE, SEARCHES_MAX, SavedSearch};
use gezik_config::settings_writer::SettingsChange;
use gezik_config::store::ConfigStore;
use gezik_core::nav::Location;
use gezik_core::search::{Scope, SearchSpec};
use slint::ComponentHandle;

use crate::AppWindow;
use crate::dialog::Dialogs;
use crate::navigation::Navigator;
use crate::view::View;

thread_local! {
    static SAVED: RefCell<Vec<SavedSearch>> = const { RefCell::new(Vec::new()) };
    /// Saves on their way: the last one's number, and the list after it while any is on its way.
    static WRITING: RefCell<(u64, Option<Vec<SavedSearch>>)> = const { RefCell::new((0, None)) };
    static CURRENT: RefCell<Option<SavedSearches>> = const { RefCell::new(None) };
}

/// settings.toml changed (every resolve); the sidebar's SEARCHES follows.
pub fn set_settings(searches: Vec<SavedSearch>) {
    SAVED.with(|s| *s.borrow_mut() = searches);
    crate::sidebar::with_current(crate::sidebar::Sidebar::searches_changed);
}

pub fn saved() -> Vec<SavedSearch> {
    SAVED.with(|s| s.borrow().clone())
}

pub fn names() -> Vec<String> {
    SAVED.with(|s| s.borrow().iter().map(|search| search.name.clone()).collect())
}

/// The list as the next edit sees it: after the saves on their way, if any.
fn latest() -> Vec<SavedSearch> {
    WRITING.with(|w| w.borrow().1.clone()).unwrap_or_else(saved)
}

fn same_name(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

/// Whether `name` can be saved next to `names`: a name there (ignoring case) is replaced; a new
/// one needs fewer than [`SEARCHES_MAX`].
fn room_for(names: &[String], name: &str) -> bool {
    names.iter().any(|n| same_name(n, name)) || names.len() < SEARCHES_MAX
}

/// Where a saved search with folder `folder` looks when run now, `here` being on screen
/// (sapma 17).
pub fn resolve(folder: &str, here: &Location, dirs: &KnownDirs) -> Result<Scope, String> {
    match folder {
        HERE => Ok(match here {
            Location::Search(spec) => spec.scope.clone(),
            Location::Path(path) | Location::Flat(path) => Scope::Folder(path.clone()),
            Location::Drives => Scope::AllDrives,
        }),
        "drives" => Ok(Scope::AllDrives),
        text => {
            let path = dirs.expand_checked(text).ok_or_else(|| format!("\"{text}\" must not contain \"..\""))?;
            if path.is_absolute() {
                Ok(Scope::Folder(path))
            } else {
                Err(format!("\"{text}\" is not a folder on this computer"))
            }
        }
    }
}

pub fn with_current(f: impl FnOnce(&SavedSearches)) {
    if let Some(searches) = CURRENT.with(|c| c.borrow().clone()) {
        f(&searches);
    }
}

struct Inner {
    window: slint::Weak<AppWindow>,
    nav: Navigator,
    view: View,
    dialogs: Dialogs,
    /// Where settings.toml is (none without a config folder: the searches live in memory then).
    store: Option<ConfigStore>,
}

#[derive(Clone)]
pub struct SavedSearches(Rc<Inner>);

impl SavedSearches {
    pub fn new(
        window: &AppWindow,
        nav: Navigator,
        view: View,
        dialogs: Dialogs,
        store: Option<ConfigStore>,
    ) -> SavedSearches {
        let searches = SavedSearches(Rc::new(Inner { window: window.as_weak(), nav, view, dialogs, store }));
        CURRENT.with(|c| *c.borrow_mut() = Some(searches.clone()));
        searches
    }

    /// Runs the saved search `name` in the active tab, or a new one; its tab is titled by it.
    pub fn run(&self, name: &str, new_tab: bool) {
        let Some(saved) = saved().into_iter().find(|s| s.name == name) else { return };
        let here = self.0.nav.active_location();
        match resolve(&saved.folder, &here, &KnownDirs::system()) {
            Ok(scope) => {
                let spec = SearchSpec { scope, name: Some(saved.name.clone()), ..saved.spec };
                crate::search::with_current(|s| s.run_saved(spec, new_tab));
            }
            Err(why) => self.0.view.note(format!("Saved search \"{name}\": {why}")),
        }
    }

    /// "Save search…": a name (Replace? if it is there), then this folder or any folder.
    pub fn ask_save(&self, spec: SearchSpec) {
        let this = self.clone();
        self.0.dialogs.ask_text("Save search", "Name for this search:", "", &["Save", "Cancel"], move |name| {
            let Some(name) = name.map(|n| n.trim().to_owned()).filter(|n| !n.is_empty()) else { return };
            let names: Vec<String> = latest().into_iter().map(|s| s.name).collect();
            if !room_for(&names, &name) {
                return this.0.view.note(format!("Up to {SEARCHES_MAX} saved searches"));
            }
            match names.iter().find(|n| same_name(n, &name)).cloned() {
                Some(old) => {
                    let again = this.clone();
                    let message = format!("A search called \"{old}\" already exists. Replace it?");
                    this.0.dialogs.ask("Save search", message, &["Replace", "Cancel"], move |choice| {
                        if choice == Some(0) {
                            again.ask_scope(name, spec);
                        }
                    });
                }
                None => this.ask_scope(name, spec),
            }
        });
    }

    fn ask_scope(&self, name: String, spec: SearchSpec) {
        let folder = match &spec.scope {
            Scope::Folder(path) => KnownDirs::system().collapse(path),
            Scope::AllDrives => "drives".to_owned(),
        };
        let this = self.clone();
        let buttons = ["Save with this folder", "Save for any folder ({here})", "Cancel"];
        let message = format!("Save \"{name}\" with {folder}, or for the folder shown when it runs?");
        // Esc: Cancel (the last button).
        self.0.dialogs.ask("Save search", message, &buttons, move |choice| {
            let folder = match choice {
                Some(0) => folder,
                Some(1) => HERE.to_owned(),
                _ => return,
            };
            let spec = SearchSpec { name: None, flat: false, ..spec };
            let mut list = latest();
            let entry = SavedSearch { name: name.clone(), folder, spec };
            match list.iter().position(|s| same_name(&s.name, &name)) {
                Some(i) => list[i] = entry,
                None => list.push(entry),
            }
            this.write(list);
        });
    }

    /// The sidebar's "Rename…".
    pub fn ask_rename(&self, name: &str) {
        let (this, old) = (self.clone(), name.to_owned());
        self.0.dialogs.ask_text("Rename search", "New name:", name, &["Rename", "Cancel"], move |new| {
            let Some(new) = new.map(|n| n.trim().to_owned()).filter(|n| !n.is_empty()) else { return };
            let mut list = latest();
            if list.iter().any(|s| same_name(&s.name, &new) && !same_name(&s.name, &old)) {
                return this.0.view.note(format!("A search called \"{new}\" already exists"));
            }
            if let Some(search) = list.iter_mut().find(|s| s.name == old) {
                search.name = new;
                this.write(list);
            }
        });
    }

    pub fn delete(&self, name: &str) {
        let mut list = latest();
        let before = list.len();
        list.retain(|s| s.name != name);
        if list.len() != before {
            self.write(list);
        }
    }

    /// Sends `list` to settings.toml; the menus and the sidebar show it once written.
    fn write(&self, list: Vec<SavedSearch>) {
        let Some(store) = &self.0.store else { return set_settings(list) };
        let seq = WRITING.with(|w| {
            let mut w = w.borrow_mut();
            w.0 += 1;
            w.1 = Some(list.clone());
            w.0
        });
        let window = self.0.window.clone();
        store.write_settings(SettingsChange::Searches(list.clone()), move |result| {
            let _ = window.upgrade_in_event_loop(move |_| with_current(|s| s.written(seq, list, result)));
        });
    }

    fn written(&self, seq: u64, list: Vec<SavedSearch>, result: Result<(), Warning>) {
        WRITING.with(|w| {
            let mut w = w.borrow_mut();
            if w.0 == seq {
                w.1 = None;
            }
        });
        match result {
            Ok(()) => set_settings(list),
            Err(warning) => self.0.view.note(warning.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn dirs() -> KnownDirs {
        KnownDirs::new(vec![("home", PathBuf::from(if cfg!(windows) { r"C:\Users\a" } else { "/home/a" }))])
    }

    #[test]
    fn a_saved_folder_resolves_where_it_runs() {
        let here_dir = PathBuf::from(if cfg!(windows) { r"D:\Work" } else { "/work" });
        let here = Location::Path(here_dir.clone());
        assert_eq!(resolve(HERE, &here, &dirs()), Ok(Scope::Folder(here_dir.clone())));
        assert_eq!(resolve(HERE, &Location::Drives, &dirs()), Ok(Scope::AllDrives), "This PC: every drive");
        assert_eq!(resolve(HERE, &Location::Flat(here_dir.clone()), &dirs()), Ok(Scope::Folder(here_dir)));
        assert_eq!(resolve("drives", &here, &dirs()), Ok(Scope::AllDrives));
        let videos = resolve("{home}/Videos", &here, &dirs()).unwrap();
        assert_eq!(videos, Scope::Folder(dirs().expand("{home}/Videos")));
        assert!(resolve("relative/path", &here, &dirs()).is_err(), "not a full path on this computer");
    }

    #[test]
    fn the_more_menu_lists_saved_searches_and_their_deletes() {
        use crate::context_menu::*;
        let names = vec!["A".to_owned(), "B".to_owned()];
        let ids: Vec<u32> = search_more_items(0, &names, true).iter().map(|i| i.0).collect();
        assert_eq!(
            ids,
            [
                SEARCH_NEW_TAB,
                SEARCH_CLEAR,
                SAVE_SEARCH,
                SAVED_SEARCH_FIRST,
                SAVED_SEARCH_FIRST + 1,
                SAVED_SEARCH_DELETE_FIRST,
                SAVED_SEARCH_DELETE_FIRST + 1
            ]
        );
    }

    #[test]
    fn a_new_name_needs_room_and_a_known_one_is_replaced() {
        let list: Vec<String> = (0..SEARCHES_MAX).map(|i| format!("s{i}")).collect();
        assert!(room_for(&list, "S3"), "replacing, ignoring case");
        assert!(!room_for(&list, "new"));
        assert!(room_for(&list[..3], "new"));
    }
}
