//! The filter bar above the file list: Ctrl+F or `/` opens it (or a letter, with `[keyboard]
//! typing = "filter"`), typing narrows the list, Esc closes it; its ▾ menu keeps patterns
//! under a name in settings.toml (`[[filters]]`). What it lets through is the view's
//! (`View::set_filter`); this is the keyboard, the bar's focus and the saved filters.

use std::cell::RefCell;
use std::time::Duration;

use gezik_config::Warning;
use gezik_config::settings::{KeyboardSettings, SavedFilter, Typing};
use gezik_config::settings_writer::SettingsChange;
use gezik_config::store::ConfigStore;
use slint::ComponentHandle;

use crate::AppWindow;
use crate::context_menu::FILTER_MAX;
use crate::dialog::Dialogs;
use crate::panes::PaneId;
use crate::view::View;

thread_local! {
    /// `[keyboard]` and `[[filters]]` of the settings in effect.
    static SETTINGS: RefCell<(KeyboardSettings, Vec<SavedFilter>)> = RefCell::default();
    /// Saves on their way to settings.toml: the last one's number, and the list after it while
    /// any is on its way (the next edit builds on it, so quick saves do not undo one another).
    static WRITING: RefCell<(u64, Option<Vec<SavedFilter>>)> = const { RefCell::new((0, None)) };
}

/// settings.toml changed (every resolve): the typing mode and the saved filters.
pub fn set_settings(keyboard: KeyboardSettings, filters: Vec<SavedFilter>) {
    SETTINGS.with(|s| *s.borrow_mut() = (keyboard, filters));
}

/// What a letter typed on the file list does.
pub fn typing() -> Typing {
    SETTINGS.with(|s| s.borrow().0.typing)
}

/// The saved filters, in settings.toml's order.
pub fn saved() -> Vec<SavedFilter> {
    SETTINGS.with(|s| s.borrow().1.clone())
}

fn set_saved(filters: Vec<SavedFilter>) {
    SETTINGS.with(|s| s.borrow_mut().1 = filters);
}

/// The saved filters as the next edit sees them: after the saves on their way, if any.
fn latest() -> Vec<SavedFilter> {
    WRITING.with(|w| w.borrow().1.clone()).unwrap_or_else(saved)
}

/// `filters` is on its way to settings.toml: its number.
fn queue_write(filters: Vec<SavedFilter>) -> u64 {
    WRITING.with(|w| {
        let mut w = w.borrow_mut();
        w.0 += 1;
        w.1 = Some(filters);
        w.0
    })
}

/// Save number `seq` of `filters` ended with `result`: written, the list is in effect; not, the
/// list stays as settings.toml has it and the error is returned, to be said.
fn finish_write(seq: u64, filters: Vec<SavedFilter>, result: Result<(), Warning>) -> Option<String> {
    WRITING.with(|w| {
        let mut w = w.borrow_mut();
        if w.0 == seq {
            w.1 = None;
        }
    });
    match result {
        Ok(()) => {
            set_saved(filters);
            None
        }
        Err(warning) => Some(warning.to_string()),
    }
}

/// Whether `name` can be saved next to `filters`: a name already there (ignoring case) is
/// replaced; a new one needs fewer than [`FILTER_MAX`] (what the menu lists).
fn room_for(filters: &[SavedFilter], name: &str) -> bool {
    find_saved(filters, name).is_some() || filters.len() < FILTER_MAX as usize
}

/// The saved filter called `name`, ignoring case (saved names are unique that way).
fn find_saved(filters: &[SavedFilter], name: &str) -> Option<usize> {
    let name = name.to_lowercase();
    filters.iter().position(|f| f.name.to_lowercase() == name)
}

/// Saves `pattern` under `name`: in the place of the filter of that name (ignoring case), which
/// takes this spelling, or at the end.
fn put_saved(filters: &mut Vec<SavedFilter>, name: &str, pattern: &str) {
    let new = SavedFilter { name: name.to_owned(), pattern: pattern.to_owned() };
    match find_saved(filters, name) {
        Some(i) => filters[i] = new,
        None => filters.push(new),
    }
}

/// Removes the filter called exactly `name`; whether it was there.
fn remove_saved(filters: &mut Vec<SavedFilter>, name: &str) -> bool {
    let Some(i) = filters.iter().position(|f| f.name == name) else { return false };
    filters.remove(i);
    true
}

/// Whether the bar's `text` (`None`: closed) with its `error` can be saved: open, not blank,
/// no error.
fn can_save_text(text: Option<&str>, error: &str) -> bool {
    text.is_some_and(|t| !t.trim().is_empty()) && error.is_empty()
}

#[derive(Clone)]
pub struct Filter {
    id: PaneId,
    window: slint::Weak<AppWindow>,
    view: View,
    dialogs: Dialogs,
    /// Where settings.toml is (none without a config folder); its one writer thread writes
    /// the numbered lists in turn.
    store: Option<ConfigStore>,
}

impl Filter {
    /// The ▾ menu is `Menus::filter_menu`'s (main.rs connects it).
    pub fn new(id: PaneId, window: &AppWindow, view: View, dialogs: Dialogs, store: Option<ConfigStore>) -> Filter {
        let filter = Filter { id, window: window.as_weak(), view, dialogs, store };
        window.on_filter_edited({
            let view = filter.view.clone();
            move |text| view.set_filter(Some(&text))
        });
        filter
    }

    /// Ctrl+F: opens the bar empty and gives it the keyboard; on an open bar, selects its text.
    pub fn open(&self) {
        let Some(window) = self.window.upgrade() else { return };
        if !self.can_filter() {
            return;
        }
        if self.view.filter_text().is_some() {
            // Focused or not, the field gets the keyboard with its text selected.
            window.invoke_select_filter_text();
            return;
        }
        self.view.set_filter(Some(""));
        self.focus_later(0);
    }

    /// Opens the bar with `c`, or adds `c` to the open bar's text; the field gets the keyboard
    /// with the cursor at the end.
    pub fn typed(&self, c: char) {
        if !self.can_filter() {
            return;
        }
        let text = match self.view.filter_text() {
            Some(mut text) => {
                text.push(c);
                text
            }
            None => c.to_string(),
        };
        let was_open = self.view.filter_text().is_some();
        self.view.set_filter(Some(&text));
        let end = i32::try_from(text.len()).unwrap_or(i32::MAX);
        if was_open && let Some(window) = self.window.upgrade() {
            window.invoke_focus_filter(end);
        } else {
            self.focus_later(end);
        }
    }

    /// Whether a folder is on screen to filter; if not, the status bar says why.
    fn can_filter(&self) -> bool {
        match unavailable_note(self.view.shows_drives(), self.view.folder().is_some() || self.view.shows_results()) {
            Some(note) => {
                self.view.note(note.to_owned());
                false
            }
            None => true,
        }
    }

    /// Esc: the whole folder shows again and the list gets the keyboard.
    pub fn close(&self) {
        self.view.set_filter(None);
        if let Some(window) = self.window.upgrade() {
            window.invoke_focus_list();
        }
    }

    /// Whether "Save as…" can save the bar's text: open, not blank, no error.
    pub fn can_save(&self) -> bool {
        let error = self.window.upgrade().map(|w| w.get_filter_error().to_string()).unwrap_or_default();
        can_save_text(self.view.filter_text().as_deref(), &error)
    }

    /// The saved filter `name` (if it is still there): its pattern into the bar (opening it),
    /// applied. The keyboard stays with the list.
    pub fn apply_saved(&self, name: &str) {
        let filters = saved();
        let Some(i) = find_saved(&filters, name) else { return };
        if !self.can_filter() {
            return;
        }
        self.view.set_filter(Some(&filters[i].pattern));
        // Once the menu is gone (it gives the keyboard back to where it was).
        let weak = self.window.clone();
        slint::Timer::single_shot(Duration::ZERO, move || {
            if let Some(window) = weak.upgrade() {
                window.invoke_focus_list();
            }
        });
    }

    /// "Save as…": asks for a name, then saves the bar's text under it; a name already there
    /// (ignoring case) is replaced only if the answer says so.
    pub fn ask_save(&self) {
        let Some(pattern) = self.view.filter_text().filter(|_| self.can_save()) else { return };
        let filter = self.clone();
        self.dialogs.ask_text("Save filter", "Name for this filter:", "", &["Save", "Cancel"], move |name| {
            let Some(name) = name.map(|n| n.trim().to_owned()).filter(|n| !n.is_empty()) else { return };
            let filters = latest();
            match find_saved(&filters, &name) {
                Some(i) => {
                    let message = format!("A filter called \"{}\" already exists. Replace it?", filters[i].name);
                    let again = filter.clone();
                    filter.dialogs.ask("Save filter", message, &["Replace", "Cancel"], move |choice| {
                        if choice == Some(0) {
                            again.save(&name, &pattern);
                        }
                    });
                }
                None => filter.save(&name, &pattern),
            }
        });
    }

    fn save(&self, name: &str, pattern: &str) {
        let mut filters = latest();
        if !room_for(&filters, name) {
            return self.view.note(format!("Up to {FILTER_MAX} saved filters"));
        }
        put_saved(&mut filters, name, pattern);
        self.write(filters);
    }

    /// Deletes the saved filter `name`, if it is still there.
    pub fn delete_saved(&self, name: &str) {
        let mut filters = latest();
        if remove_saved(&mut filters, name) {
            self.write(filters);
        }
    }

    /// Sends `filters` to settings.toml (`[[filters]]`); the menu lists them once they are
    /// written ([`Filter::written`]). Without a config folder they are only kept in memory.
    fn write(&self, filters: Vec<SavedFilter>) {
        let Some(store) = &self.store else { return set_saved(filters) };
        let seq = queue_write(filters.clone());
        let (id, window) = (self.id, self.window.clone());
        store.write_settings(SettingsChange::Filters(filters.clone()), move |result| {
            let _ = window.upgrade_in_event_loop(move |_| {
                crate::panes::with_id(id, |p| p.filter.written(seq, filters, result));
            });
        });
    }

    /// The writer thread is done with save `seq`.
    fn written(&self, seq: u64, filters: Vec<SavedFilter>, result: Result<(), Warning>) {
        if let Some(note) = finish_write(seq, filters, result) {
            self.view.note(note);
        }
    }

    /// Focuses the field once the bar is on screen (an invisible item cannot take the focus).
    fn focus_later(&self, at: i32) {
        let weak = self.window.clone();
        slint::Timer::single_shot(Duration::ZERO, move || {
            if let Some(window) = weak.upgrade()
                && window.get_filter_open()
            {
                window.invoke_focus_filter(at);
            }
        });
    }
}

/// Why the filter cannot open here: "This PC", or no folder listed (it could not be read, or
/// it is still loading). `None` in a folder.
fn unavailable_note(shows_drives: bool, has_folder: bool) -> Option<&'static str> {
    if shows_drives {
        Some("The filter works in folders")
    } else if !has_folder {
        Some("No folder to filter")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_filter_says_why_it_cannot_open() {
        assert_eq!(unavailable_note(true, false), Some("The filter works in folders"));
        assert_eq!(unavailable_note(false, false), Some("No folder to filter"), "a folder that failed to load");
        assert_eq!(unavailable_note(false, true), None);
    }

    fn saved(name: &str, pattern: &str) -> SavedFilter {
        SavedFilter { name: name.to_owned(), pattern: pattern.to_owned() }
    }

    #[test]
    fn saved_names_meet_without_regard_to_case() {
        let list = [saved("Resimler", "*.jpg"), saved("Belgeler", "*.pdf")];
        assert_eq!(find_saved(&list, "belgeler"), Some(1));
        assert_eq!(find_saved(&list, "RESIMLER"), Some(0));
        assert_eq!(find_saved(&list, "Müzik"), None);
    }

    #[test]
    fn saving_replaces_the_same_name_or_adds_at_the_end() {
        let mut list = vec![saved("Resimler", "*.jpg"), saved("Belgeler", "*.pdf")];
        put_saved(&mut list, "resimler", "*.png");
        assert_eq!(list, [saved("resimler", "*.png"), saved("Belgeler", "*.pdf")]);
        put_saved(&mut list, "Müzik", "*.mp3");
        assert_eq!(list.len(), 3);
        assert_eq!(list[2], saved("Müzik", "*.mp3"));
    }

    #[test]
    fn deleting_takes_only_that_name() {
        let mut list = vec![saved("Resimler", "*.jpg"), saved("Belgeler", "*.pdf")];
        assert!(remove_saved(&mut list, "Resimler"));
        assert_eq!(list, [saved("Belgeler", "*.pdf")]);
        assert!(!remove_saved(&mut list, "Resimler"), "gone already");
    }

    fn temp_store(name: &str, settings: Option<&str>) -> ConfigStore {
        let dir = std::env::temp_dir().join(format!("gezik-filters-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        if let Some(text) = settings {
            std::fs::write(dir.join("settings.toml"), text).unwrap();
        }
        ConfigStore::new(dir)
    }

    #[test]
    fn a_failed_save_keeps_the_list_as_the_file_has_it() {
        let broken = "[keyboard
typing = \"filter\"
";
        let store = temp_store("broken", Some(broken));
        set_settings(KeyboardSettings::default(), vec![saved("A", "a")]);
        let wanted = vec![saved("A", "a"), saved("B", "b")];
        let seq = queue_write(wanted.clone());
        assert_eq!(latest(), wanted, "the next edit builds on the save on its way");
        let note = finish_write(seq, wanted.clone(), store.save_filters(&wanted));
        assert!(note.is_some_and(|n| n.contains("settings.toml")));
        assert_eq!(super::saved(), [saved("A", "a")], "the menu stays as the file is");
        assert_eq!(latest(), [saved("A", "a")]);
        assert_eq!(std::fs::read_to_string(store.dir().join("settings.toml")).unwrap(), broken);
        let _ = std::fs::remove_dir_all(store.dir());
    }

    #[test]
    fn a_good_save_takes_effect() {
        let store = temp_store("good", None);
        set_settings(KeyboardSettings::default(), Vec::new());
        let wanted = vec![saved("Resimler", "*.jpg")];
        let seq = queue_write(wanted.clone());
        assert_eq!(finish_write(seq, wanted.clone(), store.save_filters(&wanted)), None);
        assert_eq!(super::saved(), wanted);
        let text = std::fs::read_to_string(store.dir().join("settings.toml")).unwrap();
        assert!(text.contains("[[filters]]") && text.contains("name = \"Resimler\""));
        let _ = std::fs::remove_dir_all(store.dir());
    }

    #[test]
    fn quick_saves_build_on_one_another() {
        set_settings(KeyboardSettings::default(), Vec::new());
        let one = vec![saved("A", "a")];
        let first = queue_write(one.clone());
        let two = vec![saved("A", "a"), saved("B", "b")];
        let second = queue_write(two.clone());
        assert_eq!(finish_write(first, one.clone(), Ok(())), None);
        assert_eq!(super::saved(), one);
        assert_eq!(latest(), two, "the second is still on its way");
        assert_eq!(finish_write(second, two.clone(), Ok(())), None);
        assert_eq!(super::saved(), two);
        assert_eq!(latest(), two);
    }

    #[test]
    fn thirty_saved_filters_at_most() {
        let full: Vec<SavedFilter> = (0..FILTER_MAX).map(|i| saved(&format!("F{i}"), "x")).collect();
        assert!(!room_for(&full, "New"));
        assert!(room_for(&full, "f3"), "replacing one needs no room");
        assert!(room_for(&full[1..], "New"));
    }

    #[test]
    fn only_a_good_non_empty_text_can_be_saved() {
        assert!(can_save_text(Some("*.jpg"), ""));
        assert!(!can_save_text(Some("  "), ""));
        assert!(!can_save_text(None, ""));
        assert!(!can_save_text(Some("*.jpg;!"), "an empty part"));
    }
}
