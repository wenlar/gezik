//! The actions of the keyboard package (6a, 6b) and of 7a and 7b, run by their shortcuts (main.rs
//! `handle_key`) and on macOS by the menu bar when their chord cannot be played there (the
//! keypad's) or there is none.

use gezik_config::shortcuts::Action;
use gezik_core::nav::Location;

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

/// Runs `action` if it is one of 6a's, 6b's, 7a's, 7b's and 8a's; returns whether it ran (a pin
/// number with no pin did not).
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
        Action::ToggleHidden => crate::view_options::toggle_hidden(),
        Action::AddToStack => crate::stack::with_current(crate::stack::Stack::add_selection),
        Action::ToggleStack => crate::stack::with_current(crate::stack::Stack::toggle),
        Action::ShowHistory => crate::operations::with_current(crate::operations::Operations::show_history),
        Action::ReopenTab => nav.reopen_tab(),
        Action::Search => crate::search::with_current(crate::search::Searches::open),
        Action::FlatView => crate::search::with_current(crate::search::Searches::flat_view),
        Action::ToggleTabLock => nav.toggle_tab_lock(nav.active_index()),
        Action::TabPicker => crate::tab_tools::with_current(crate::tab_tools::TabTools::open),
        Action::ClearHistory => crate::path_box::with_current(|p| p.forget(true)),
        Action::OpenTerminal => crate::terminal::open_for_view(view, false),
        Action::OpenTerminalAdmin => crate::terminal::open_for_view(view, true),
        Action::CopyPath => crate::copy_path::copy_selection(view),
        Action::SaveTabSet => crate::tab_sets::with_current(crate::tab_sets::TabSets::ask_save),
        Action::ShowInFolder => crate::operations::with_current(|ops| ops.show_in_folder(false)),
        Action::CopyWithFolders => crate::operations::with_current(|ops| ops.copy_with_folders(false)),
        Action::CutWithFolders => crate::operations::with_current(|ops| ops.copy_with_folders(true)),
        Action::CalculateFolderSizes => crate::folder_sizes::with_current(crate::folder_sizes::FolderSizes::calculate),
        Action::SaveSearch => crate::search::with_current(crate::search::Searches::save_current),
        Action::MakeAlias => crate::operations::with_current(crate::operations::Operations::make_alias_of_selection),
        Action::ShowPackageContents => {
            // The selected (or focused) folder, even a package; a file: nothing.
            let item = view.single_selected().or_else(|| view.focus()).and_then(|i| view.entry_path(i));
            let Some((folder, true)) = item else { return false };
            nav.go(Location::Path(folder));
        }
        Action::NewFolderWithSelection => {
            crate::operations::with_current(crate::operations::Operations::new_folder_with_selection)
        }
        Action::Pin1
        | Action::Pin2
        | Action::Pin3
        | Action::Pin4
        | Action::Pin5
        | Action::Pin6
        | Action::Pin7
        | Action::Pin8
        | Action::Pin9 => {
            // A number with no pin shown: nothing (spec 6.4), and the key stays unused, so it
            // reaches the text box and the focus stays where it is.
            let mut location = None;
            if let Some(n) = action.pin_number() {
                crate::sidebar::with_current(|sidebar| location = sidebar.pin_location(n - 1));
            }
            let Some(location) = location else { return false };
            nav.go(location);
        }
        // Not theirs: `handle_key` and the menu bar run these themselves. Listed one by one so
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
        | Action::Share
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
        | Action::CommandPalette
        | Action::QuickOpen => return false,
    }
    true
}
