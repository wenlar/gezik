//! The filter bar above the file list: Ctrl+F opens it, typing narrows the list, Esc closes
//! it. What it lets through is the view's (`View::set_filter`); this is the keyboard and the
//! bar's focus.

use std::cell::RefCell;
use std::time::Duration;

use slint::ComponentHandle;

use crate::AppWindow;
use crate::view::View;

thread_local! {
    /// The filter of this (UI) thread, for the key handler and the actions.
    static CURRENT: RefCell<Option<Filter>> = const { RefCell::new(None) };
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
}

impl Filter {
    pub fn new(window: &AppWindow, view: View) -> Filter {
        let filter = Filter { window: window.as_weak(), view };
        window.on_filter_edited({
            let view = filter.view.clone();
            move |text| view.set_filter(Some(&text))
        });
        // The saved filters' menu comes with Task 7.
        window.on_filter_menu(|_, _, _, _| {});
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
    #[allow(dead_code, reason = "Task 7's typing mode and \"/\" use it")]
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
