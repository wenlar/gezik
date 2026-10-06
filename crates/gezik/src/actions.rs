//! The keyboard package's actions (6a's and 6b's), run by their shortcuts (main.rs `handle_key`) and on
//! macOS by the menu bar when their chord cannot be played there (the keypad's) or there is none.

use gezik_config::shortcuts::Action;

use crate::navigation::Navigator;
use crate::view::View;

/// `[[commands]]` entry `index`, run by its key or the macOS menu bar on the selection (the
/// focused item when nothing is selected).
pub fn run_command(index: usize, view: &View) {
    if view.shows_drives() {
        return view.note("Commands run on files and folders".to_owned());
    }
    let mut items = view.selected_items();
    if items.is_empty() {
        items.extend(view.focus().and_then(|i| view.entry_path(i)));
    }
    crate::convert::run_by_index(index, items);
}

/// Runs `action` if it is one of 6a's and 6b's; returns whether it ran.
pub fn run(action: Action, nav: &Navigator, view: &View) -> bool {
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
        Action::InvertSelection => view.invert_selection(),
        Action::SelectSameType => view.select_same_type(),
        Action::RestoreSelection => view.restore_remembered(),
        Action::SelectPattern => crate::select_tools::with_current(|s| s.ask(true)),
        Action::DeselectPattern => crate::select_tools::with_current(|s| s.ask(false)),
        Action::ReopenTab => nav.reopen_tab(),
        Action::ToggleTabLock => nav.toggle_tab_lock(nav.active_index()),
        Action::TabPicker => crate::tab_tools::with_current(crate::tab_tools::TabTools::open),
        Action::ClearHistory => crate::path_box::with_current(|p| p.forget(true)),
        // Not 6a's or 6b's: `handle_key` and the menu bar run these themselves. Listed one by one so
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
