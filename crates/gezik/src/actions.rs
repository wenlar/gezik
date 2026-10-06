//! The keyboard package's actions (6a), run by their shortcuts (main.rs `handle_key`) and on
//! macOS by the menu bar when their chord cannot be played there (the keypad's) or there is none.

use gezik_config::shortcuts::Action;

use crate::navigation::Navigator;
use crate::view::View;

/// Runs `action` if it is one of 6a's; returns whether it ran.
pub fn run(action: Action, nav: &Navigator, view: &View) -> bool {
    let _ = view;
    match action {
        Action::Tab1
        | Action::Tab2
        | Action::Tab3
        | Action::Tab4
        | Action::Tab5
        | Action::Tab6
        | Action::Tab7
        | Action::Tab8 => {
            // A tab that is not there: nothing (spec 5).
            if let Some(n) = action.tab_number() {
                nav.activate_tab(n - 1);
            }
        }
        Action::TabLast => nav.activate_tab(nav.tab_count().saturating_sub(1)),
        Action::Filter => crate::filter::with_current(crate::filter::Filter::open),
        // Task 7-9 fill these in.
        Action::InvertSelection
        | Action::SelectPattern
        | Action::DeselectPattern
        | Action::SelectSameType
        | Action::RestoreSelection
        | Action::ReopenTab
        | Action::TabPicker
        | Action::ToggleTabLock => return false,
        // Not 6a's: `handle_key` and the menu bar run these themselves. Listed one by one so
        // that a new action is a compile error here until it is placed.
        Action::NewTab
        | Action::CloseTab
        | Action::NextTab
        | Action::PrevTab
        | Action::Back
        | Action::Forward
        | Action::Up
        | Action::FocusPath
        | Action::Refresh
        | Action::SelectAll
        | Action::ViewList
        | Action::ViewGrid
        | Action::TogglePreview
        | Action::QuickLook
        | Action::Copy
        | Action::Cut
        | Action::Paste
        | Action::PasteMove
        | Action::Trash
        | Action::DeletePermanently
        | Action::Rename
        | Action::NewFolder
        | Action::Duplicate
        | Action::Undo
        | Action::Redo
        | Action::BatchRename
        | Action::ToggleHidden => return false,
    }
    true
}
