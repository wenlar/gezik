//! The filter bar above the file list: Ctrl+F or `/` opens it (or a letter, with `[keyboard]
//! typing = "filter"`), typing narrows the list, Esc closes it; its ▾ menu keeps patterns
//! under a name in settings.toml (`[[filters]]`). What it lets through is the view's
//! (`View::set_filter`); this is the keyboard, the bar's focus and the saved filters.

use std::cell::RefCell;
use std::time::Duration;

use gezik_config::settings::{KeyboardSettings, SavedFilter, Typing};
use gezik_config::store::ConfigStore;
use slint::ComponentHandle;

use crate::AppWindow;
use crate::dialog::Dialogs;
use crate::view::View;

thread_local! {
    /// The filter of this (UI) thread, for the key handler and the actions.
    static CURRENT: RefCell<Option<Filter>> = const { RefCell::new(None) };
    /// `[keyboard]` and `[[filters]]` of the settings in effect.
    static SETTINGS: RefCell<(KeyboardSettings, Vec<SavedFilter>)> = RefCell::default();
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

/// Runs `f` with this UI thread's filter, if there is one yet.
pub fn with_current(f: impl FnOnce(&Filter)) {
    if let Some(filter) = CURRENT.with(|c| c.borrow().clone()) {
        f(&filter);
    }
}

#[derive(Clone)]
pub struct Filter {
    window: slint::Weak<AppWindow>,
    view: View,
    dialogs: Dialogs,
    store: Option<ConfigStore>,
}

impl Filter {
    /// The ▾ menu is `Menus::filter_menu`'s (main.rs connects it).
    pub fn new(window: &AppWindow, view: View, dialogs: Dialogs, store: Option<ConfigStore>) -> Filter {
        let filter = Filter { window: window.as_weak(), view, dialogs, store };
        window.on_filter_edited({
            let view = filter.view.clone();
            move |text| view.set_filter(Some(&text))
        });
        CURRENT.with(|c| *c.borrow_mut() = Some(filter.clone()));
        filter
    }

    /// Ctrl+F: opens the bar empty and gives it the keyboard; on an open bar, selects its text.
    pub fn open(&self) {
        let Some(window) = self.window.upgrade() else { return };
        if self.view.shows_drives() {
            return self.view.note("The filter works in folders".to_owned());
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
        if self.view.shows_drives() {
            return self.view.note("The filter works in folders".to_owned());
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
        if self.view.shows_drives() {
            return self.view.note("The filter works in folders".to_owned());
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
            let filters = saved();
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
        let mut filters = saved();
        put_saved(&mut filters, name, pattern);
        self.write(filters);
    }

    /// Deletes the saved filter `name`, if it is still there.
    pub fn delete_saved(&self, name: &str) {
        let mut filters = saved();
        if remove_saved(&mut filters, name) {
            self.write(filters);
        }
    }

    /// The list in effect at once; settings.toml (`[[filters]]`) after it.
    fn write(&self, filters: Vec<SavedFilter>) {
        set_saved(filters.clone());
        if let Some(store) = &self.store
            && let Err(warning) = store.save_filters(&filters)
        {
            self.view.note(warning.to_string());
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

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn only_a_good_non_empty_text_can_be_saved() {
        assert!(can_save_text(Some("*.jpg"), ""));
        assert!(!can_save_text(Some("  "), ""));
        assert!(!can_save_text(None, ""));
        assert!(!can_save_text(Some("*.jpg;!"), "an empty part"));
    }
}
