//! The keyboard package's actions (6a), run by their shortcuts (main.rs `handle_key`) and on
//! macOS by the menu bar when their chord cannot be played there (the keypad's) or there is none.

use gezik_config::shortcuts::Action;

use crate::navigation::Navigator;
use crate::view::View;

/// Runs `action` if it is one of 6a's; returns whether it ran.
pub fn run(action: Action, nav: &Navigator, view: &View) -> bool {
    let _ = view;
    if let Some(n) = action.tab_number() {
        // A tab that is not there: nothing (spec 5).
        nav.activate_tab(n - 1);
        return true;
    }
    match action {
        Action::TabLast => nav.activate_tab(nav.tab_count().saturating_sub(1)),
        // Task 6-9 fill these in.
        Action::Filter
        | Action::InvertSelection
        | Action::SelectPattern
        | Action::DeselectPattern
        | Action::SelectSameType
        | Action::RestoreSelection
        | Action::ReopenTab
        | Action::TabPicker
        | Action::ToggleTabLock => return false,
        _ => return false,
    }
    true
}
