//! Tab sets (spec 5.2): the open tabs saved under a name in settings.toml (`[[tab-sets]]`,
//! paths with `{home}`-style tokens, "drives" for This PC) and opened again from a tab's menu
//! or the macOS Window menu. Saving and deleting go through the one settings.toml writer, as
//! the saved filters do (filter.rs).

use std::cell::RefCell;

use gezik_config::Warning;
use gezik_config::paths::KnownDirs;
use gezik_config::settings::TabSet;
use gezik_config::settings_writer::SettingsChange;
use gezik_config::store::ConfigStore;
use gezik_core::nav::Location;
use slint::ComponentHandle;

use crate::AppWindow;
use crate::context_menu::{TAB_SET_DELETE_FIRST, TAB_SET_MAX, TAB_SET_OPEN_FIRST, TAB_SET_REPLACE_FIRST};
use crate::dialog::Dialogs;
use crate::navigation::Navigator;
use crate::view::View;

thread_local! {
    /// The tab sets of this (UI) thread, for the menus and the actions.
    static CURRENT: RefCell<Option<TabSets>> = const { RefCell::new(None) };
    /// `[[tab-sets]]` of the settings in effect.
    static SETS: RefCell<Vec<TabSet>> = const { RefCell::new(Vec::new()) };
    /// Saves on their way to settings.toml: the last one's number, and the list after it while
    /// any is on its way (the next edit builds on it).
    static WRITING: RefCell<(u64, Option<Vec<TabSet>>)> = const { RefCell::new((0, None)) };
}

/// settings.toml changed (every resolve).
pub fn set_settings(sets: Vec<TabSet>) {
    SETS.with(|s| *s.borrow_mut() = sets);
}

/// The tab sets, in settings.toml's order.
pub fn saved() -> Vec<TabSet> {
    SETS.with(|s| s.borrow().clone())
}

/// Their names, for the menus.
pub fn names() -> Vec<String> {
    SETS.with(|s| s.borrow().iter().map(|set| set.name.clone()).collect())
}

/// The sets as the next edit sees them: after the saves on their way, if any.
fn latest() -> Vec<TabSet> {
    WRITING.with(|w| w.borrow().1.clone()).unwrap_or_else(saved)
}

fn queue_write(sets: Vec<TabSet>) -> u64 {
    WRITING.with(|w| {
        let mut w = w.borrow_mut();
        w.0 += 1;
        w.1 = Some(sets);
        w.0
    })
}

/// Save `seq` of `sets` ended with `result`: written, they are in effect; not, they stay as
/// settings.toml has them and the error is returned, to be said.
fn finish_write(seq: u64, sets: Vec<TabSet>, result: Result<(), Warning>) -> Option<String> {
    WRITING.with(|w| {
        let mut w = w.borrow_mut();
        if w.0 == seq {
            w.1 = None;
        }
    });
    match result {
        Ok(()) => {
            set_settings(sets);
            None
        }
        Err(warning) => Some(warning.to_string()),
    }
}

/// The set called `name`, ignoring case (names are unique that way).
fn find(sets: &[TabSet], name: &str) -> Option<usize> {
    let name = name.to_lowercase();
    sets.iter().position(|set| set.name.to_lowercase() == name)
}

/// Whether `name` can be saved next to `sets`: a name already there is replaced; a new one
/// needs fewer than `TAB_SET_MAX` (what the menu lists).
fn room_for(sets: &[TabSet], name: &str) -> bool {
    find(sets, name).is_some() || sets.len() < TAB_SET_MAX as usize
}

/// Puts `set` in the place of the one of its name (which takes this spelling), or at the end.
fn put(sets: &mut Vec<TabSet>, set: TabSet) {
    match find(sets, &set.name) {
        Some(i) => sets[i] = set,
        None => sets.push(set),
    }
}

/// Removes the set called exactly `name`; whether it was there.
fn remove(sets: &mut Vec<TabSet>, name: &str) -> bool {
    let Some(i) = sets.iter().position(|set| set.name == name) else { return false };
    sets.remove(i);
    true
}

/// A tab's place as a set keeps it: "drives", or the path with tokens.
pub fn text_of(location: &Location, dirs: &KnownDirs) -> String {
    match location {
        Location::Drives => "drives".to_owned(),
        Location::Path(path) => dirs.collapse(path).trim().to_owned(),
    }
}

/// A set's entry as a place; one with `..` is none (settings.toml leaves those sets out).
pub fn location_of(text: &str, dirs: &KnownDirs) -> Option<Location> {
    let text = text.trim();
    if text.eq_ignore_ascii_case("drives") {
        return Some(Location::Drives);
    }
    dirs.expand_checked(text).map(Location::Path)
}

/// The note for set `name` when none of its tabs can be opened here (unknown tokens, `..`).
fn no_folders_text(name: &str) -> String {
    format!("Tab set \"{name}\" has no folders to open")
}

/// What a tab set menu item does, by its id: open, replace the tabs with, or delete set N.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetItem {
    Open(usize),
    Replace(usize),
    Delete(usize),
}

pub fn set_item(id: u32) -> Option<SetItem> {
    let index = |first: u32| (first..first + TAB_SET_MAX).contains(&id).then(|| (id - first) as usize);
    index(TAB_SET_OPEN_FIRST)
        .map(SetItem::Open)
        .or_else(|| index(TAB_SET_REPLACE_FIRST).map(SetItem::Replace))
        .or_else(|| index(TAB_SET_DELETE_FIRST).map(SetItem::Delete))
}

/// Runs `f` with this UI thread's tab sets, if there are any yet.
pub fn with_current(f: impl FnOnce(&TabSets)) {
    if let Some(sets) = CURRENT.with(|c| c.borrow().clone()) {
        f(&sets);
    }
}

#[derive(Clone)]
pub struct TabSets {
    window: slint::Weak<AppWindow>,
    nav: Navigator,
    view: View,
    dialogs: Dialogs,
    /// Where settings.toml is (none without a config folder: the sets live in memory then).
    store: Option<ConfigStore>,
}

impl TabSets {
    pub fn new(
        window: &AppWindow,
        nav: Navigator,
        view: View,
        dialogs: Dialogs,
        store: Option<ConfigStore>,
    ) -> TabSets {
        let sets = TabSets { window: window.as_weak(), nav, view, dialogs, store };
        CURRENT.with(|c| *c.borrow_mut() = Some(sets.clone()));
        sets
    }

    /// Tab set menu item `id` was chosen; `names` are the sets the menu listed, by place.
    pub fn chosen(&self, id: u32, names: &[String]) {
        let Some(item) = set_item(id) else { return };
        let (SetItem::Open(i) | SetItem::Replace(i) | SetItem::Delete(i)) = item;
        let Some(name) = names.get(i) else { return };
        match item {
            SetItem::Open(_) => self.open(name, false),
            SetItem::Replace(_) => self.open(name, true),
            SetItem::Delete(_) => self.delete(name),
        }
    }

    /// Opens set `name` (if it is still there) after the open tabs; `replace` closes the
    /// unlocked ones first.
    pub fn open(&self, name: &str, replace: bool) {
        let sets = saved();
        let Some(i) = find(&sets, name) else { return };
        let dirs = KnownDirs::system();
        let locations: Vec<Location> = sets[i].tabs.iter().filter_map(|text| location_of(text, &dirs)).collect();
        if locations.is_empty() {
            return self.view.note(no_folders_text(&sets[i].name));
        }
        self.nav.open_tab_set(locations, replace);
    }

    /// "Save tabs as…": asks for a name, then saves the tabs open now under it; a name already
    /// there (ignoring case) is replaced only if the answer says so.
    pub fn ask_save(&self) {
        let dirs = KnownDirs::system();
        let tabs: Vec<String> = self.nav.tab_locations().iter().map(|location| text_of(location, &dirs)).collect();
        let this = self.clone();
        self.dialogs.ask_text("Save tabs", "Name for these tabs:", "", &["Save", "Cancel"], move |name| {
            let Some(name) = name.map(|n| n.trim().to_owned()).filter(|n| !n.is_empty()) else { return };
            let sets = latest();
            match find(&sets, &name) {
                Some(i) => {
                    let message = format!("A tab set called \"{}\" already exists. Replace it?", sets[i].name);
                    let again = this.clone();
                    this.dialogs.ask("Save tabs", message, &["Replace", "Cancel"], move |choice| {
                        if choice == Some(0) {
                            again.save(TabSet { name, tabs });
                        }
                    });
                }
                None => this.save(TabSet { name, tabs }),
            }
        });
    }

    fn save(&self, set: TabSet) {
        let mut sets = latest();
        if !room_for(&sets, &set.name) {
            return self.view.note(format!("Up to {TAB_SET_MAX} tab sets"));
        }
        put(&mut sets, set);
        self.write(sets);
    }

    /// Deletes set `name`, if it is still there.
    pub fn delete(&self, name: &str) {
        let mut sets = latest();
        if remove(&mut sets, name) {
            self.write(sets);
        }
    }

    /// Sends `sets` to settings.toml; the menus list them once written.
    fn write(&self, sets: Vec<TabSet>) {
        let Some(store) = &self.store else { return set_settings(sets) };
        let seq = queue_write(sets.clone());
        let window = self.window.clone();
        store.write_settings(SettingsChange::TabSets(sets.clone()), move |result| {
            let _ = window.upgrade_in_event_loop(move |_| with_current(|t| t.written(seq, sets, result)));
        });
    }

    fn written(&self, seq: u64, sets: Vec<TabSet>, result: Result<(), Warning>) {
        if let Some(note) = finish_write(seq, sets, result) {
            self.view.note(note);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(name: &str, tabs: &[&str]) -> TabSet {
        TabSet { name: name.into(), tabs: tabs.iter().map(|t| (*t).to_owned()).collect() }
    }

    fn home() -> std::path::PathBuf {
        std::env::temp_dir().join("u")
    }

    fn dirs() -> KnownDirs {
        KnownDirs::new(vec![("home", home()), ("downloads", home().join("İndirilenler"))])
    }

    #[test]
    fn a_set_with_nothing_to_open_says_so() {
        let dirs = dirs();
        let broken = set("Work", &["../up", "{home}/a/../b"]);
        assert!(broken.tabs.iter().all(|text| location_of(text, &dirs).is_none()));
        assert_eq!(no_folders_text(&broken.name), "Tab set \"Work\" has no folders to open");
    }

    #[test]
    fn tabs_are_kept_with_tokens_and_drives() {
        let dirs = dirs();
        let downloads = home().join("İndirilenler");
        assert_eq!(text_of(&Location::Path(downloads.clone()), &dirs), "{downloads}");
        assert_eq!(text_of(&Location::Drives, &dirs), "drives");
        assert_eq!(location_of("{downloads}", &dirs), Some(Location::Path(downloads)));
        assert_eq!(location_of(" Drives ", &dirs), Some(Location::Drives));
        assert_eq!(location_of("{home}/../x", &dirs), None);
    }

    #[test]
    fn saving_replaces_the_same_name_ignoring_case() {
        let mut sets = vec![set("Work", &["/a"]), set("Media", &["/m"])];
        put(&mut sets, set("work", &["/b"]));
        assert_eq!(sets, [set("work", &["/b"]), set("Media", &["/m"])]);
        put(&mut sets, set("New", &["drives"]));
        assert_eq!(sets.len(), 3);
        assert!(remove(&mut sets, "Media") && !remove(&mut sets, "Media"));
    }

    #[test]
    fn thirty_sets_at_most() {
        let full: Vec<TabSet> = (0..TAB_SET_MAX).map(|i| set(&format!("S{i}"), &["/a"])).collect();
        assert!(!room_for(&full, "New"));
        assert!(room_for(&full, "s3"), "replacing one needs no room");
    }

    #[test]
    fn menu_ids_name_the_item() {
        assert_eq!(set_item(TAB_SET_OPEN_FIRST + 2), Some(SetItem::Open(2)));
        assert_eq!(set_item(TAB_SET_REPLACE_FIRST), Some(SetItem::Replace(0)));
        assert_eq!(set_item(TAB_SET_DELETE_FIRST + TAB_SET_MAX - 1), Some(SetItem::Delete(29)));
        assert_eq!(set_item(TAB_SET_DELETE_FIRST + TAB_SET_MAX), None);
        assert_eq!(set_item(crate::context_menu::SAVE_TAB_SET), None);
    }

    #[test]
    fn a_save_reaches_settings_toml() {
        let dir = std::env::temp_dir().join(format!("gezik-tab-sets-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store = ConfigStore::new(dir.clone());
        set_settings(Vec::new());
        let wanted = vec![set("Release", &["{downloads}", "drives"])];
        let seq = queue_write(wanted.clone());
        assert_eq!(latest(), wanted, "the next edit builds on the save on its way");
        assert_eq!(finish_write(seq, wanted.clone(), store.save_tab_sets(&wanted)), None);
        assert_eq!(saved(), wanted);
        let text = std::fs::read_to_string(dir.join("settings.toml")).unwrap();
        assert!(text.contains("[[tab-sets]]") && text.contains("name = \"Release\""), "{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
