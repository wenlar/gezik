//! Selecting by pattern (Ctrl+= / Ctrl+-, or the keypad's + and -): a box asks for the
//! pattern, with a live count of the items it matches; the last pattern is kept in state.toml
//! (`[selection] last-pattern`).

use std::cell::RefCell;
use std::rc::Rc;

use gezik_config::store::ConfigStore;
use gezik_core::pattern::Pattern;

use crate::dialog::Dialogs;
use crate::preview::with_commas;

const MESSAGE: &str = "Names that match (* and ? are wildcards, ; separates patterns, ! leaves out):";

thread_local! {
    /// The selection tools of this (UI) thread, for the actions.
    static CURRENT: RefCell<Option<SelectTools>> = const { RefCell::new(None) };
}

/// Runs `f` with this UI thread's selection tools, if there are any yet.
pub fn with_current(f: impl FnOnce(&SelectTools)) {
    if let Some(tools) = CURRENT.with(|c| c.borrow().clone()) {
        f(&tools);
    }
}

/// The line under the pattern field: how many shown items match, or what is wrong.
pub fn match_note(text: &str, count: impl Fn(&Pattern) -> usize) -> (String, bool) {
    match Pattern::compile(text) {
        Ok(pattern) => {
            let line = match count(&pattern) {
                0 => "No items match".to_owned(),
                1 => "1 item matches".to_owned(),
                n => format!("{} items match", with_commas(n)),
            };
            (line, false)
        }
        Err(error) => (error, true),
    }
}

struct Inner {
    dialogs: Dialogs,
    store: Option<ConfigStore>,
    /// The last pattern used (the box starts with it).
    last: RefCell<String>,
}

#[derive(Clone)]
pub struct SelectTools(Rc<Inner>);

impl SelectTools {
    /// `last`: the pattern state.toml remembers.
    pub fn new(dialogs: Dialogs, store: Option<ConfigStore>, last: Option<String>) -> SelectTools {
        let tools = SelectTools(Rc::new(Inner { dialogs, store, last: RefCell::new(last.unwrap_or_default()) }));
        CURRENT.with(|c| *c.borrow_mut() = Some(tools.clone()));
        tools
    }

    /// Asks for a pattern, then selects (`select`) or unselects the shown items it matches.
    pub fn ask(&self, select: bool) {
        let (title, buttons): (&str, &[&str]) = if select {
            ("Select by pattern", &["Select", "Cancel"])
        } else {
            ("Deselect by pattern", &["Deselect", "Cancel"])
        };
        let view = crate::panes::active_view().clone();
        let note = move |text: &str| match_note(text, |p| view.count_matching(p));
        let tools = self.clone();
        let last = self.0.last.borrow().clone();
        self.0.dialogs.ask_text_noted(title, MESSAGE, last, buttons, note, move |text| {
            let Some(text) = text else { return };
            match Pattern::compile(&text) {
                Ok(pattern) => {
                    crate::panes::active_view().select_matching(&pattern, select);
                    *tools.0.last.borrow_mut() = text.clone();
                    if let Some(store) = &tools.0.store {
                        store.update_state(|s| s.selection.last_pattern = Some(text));
                    }
                }
                Err(error) => crate::panes::active_view().note(error),
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_note_says_how_many_match_or_what_is_wrong() {
        let names = ["a.jpg", "b.JPG", "c.png"];
        let count = |p: &Pattern| names.iter().filter(|n| p.matches(n)).count();
        assert_eq!(match_note("*.jpg", count), ("2 items match".to_owned(), false));
        assert_eq!(match_note("*.png", count), ("1 item matches".to_owned(), false));
        assert_eq!(match_note("*.gif", count), ("No items match".to_owned(), false));
        assert_eq!(match_note("", count), ("3 items match".to_owned(), false), "empty: everything shown");
        assert_eq!(match_note("!", count), ("Type a name after \"!\"".to_owned(), true));
        let many = |_: &Pattern| 12_345;
        assert_eq!(match_note("x", many).0, "12,345 items match");
    }
}
