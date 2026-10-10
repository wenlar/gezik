//! The actions of the keyboard package (6a, 6b) and of 7a and 7b, run by their shortcuts (main.rs
//! `handle_key`) and on macOS by the menu bar when their chord cannot be played there (the
//! keypad's) or there is none.

use gezik_config::shortcuts::Action;
use gezik_core::group::GroupBy;
use gezik_core::nav::Location;

use crate::navigation::Navigator;
use crate::view::View;

/// `[[commands]]` entry `index`, run by its key or the macOS menu bar on the selection (the
/// focused item when nothing is selected).
pub fn run_command(index: usize, view: &View) {
    if view.shows_drives() {
        return view.note("Commands run on files and folders".to_owned());
    }
    if view.shows_trash() {
        return view.note(crate::trash_view::not_here());
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
        Action::Filter => {
            crate::panes::with_active(|p| p.filter.open());
        }
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
        Action::Search => {
            crate::panes::with_active(|p| p.search.open());
        }
        Action::FlatView => {
            crate::panes::with_active(|p| p.search.flat_view());
        }
        Action::ToggleTabLock => nav.toggle_tab_lock(nav.active_index()),
        Action::TabPicker => crate::tab_tools::with_current(crate::tab_tools::TabTools::open),
        Action::ClearHistory => {
            crate::panes::with_active(|p| p.path_box.forget(true));
        }
        Action::OpenTerminal => crate::terminal::open_for_view(view, false),
        Action::OpenTerminalAdmin => crate::terminal::open_for_view(view, true),
        Action::CopyPath => crate::copy_path::copy_selection(view),
        Action::SaveTabSet => crate::tab_sets::with_current(crate::tab_sets::TabSets::ask_save),
        Action::ShowInFolder => crate::operations::with_current(|ops| ops.show_in_folder(false)),
        Action::CopyWithFolders => crate::operations::with_current(|ops| ops.copy_with_folders(false)),
        Action::CutWithFolders => crate::operations::with_current(|ops| ops.copy_with_folders(true)),
        Action::CalculateFolderSizes => {
            crate::panes::with_active(|p| p.folder_sizes.calculate());
        }
        Action::SaveSearch => {
            crate::panes::with_active(|p| p.search.save_current());
        }
        Action::MakeAlias => crate::operations::with_current(crate::operations::Operations::make_alias_of_selection),
        Action::GetInfo => crate::info::get_info(view),
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
        Action::NewWindow => open_new_window(nav, view),
        Action::ShowTrash => nav.go(gezik_core::nav::Location::Trash),
        Action::PutBack => crate::trash_view::put_back(view),
        Action::KeepOffline => crate::cloud::keep_selection(view),
        Action::FreeUpSpace => crate::cloud::free_up_selection(view),
        Action::EmptyTrash => crate::trash_view::empty(),
        Action::SystemIntegration => crate::integration::with_current(crate::integration::Integration::open),
        Action::ConnectToServer => crate::connect::open(),
        Action::Eject => crate::eject::eject_selection(view, nav),
        Action::GroupNone => view.set_group(GroupBy::None),
        Action::GroupType => view.set_group(GroupBy::Type),
        Action::GroupDate => view.set_group(GroupBy::Date),
        Action::GroupSize => view.set_group(GroupBy::Size),
        Action::CollapseGroups => view.collapse_all(true),
        Action::ExpandGroups => view.collapse_all(false),
        Action::RevealInTree => crate::sidebar::with_current(crate::sidebar::Sidebar::reveal_current),
        Action::ToggleDualPane => crate::dual::toggle(),
        Action::FocusOtherPane => return crate::dual::focus_other(),
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

/// `new-window` (spec 5.3): this Gezik again as a process of its own, at the folder shown
/// (the start folder for This PC, a search or a flat view). One process has one window.
fn open_new_window(nav: &Navigator, view: &View) {
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(err) => return view.note(format!("Cannot open a new window: {}", gezik_platform::fs::describe(&err))),
    };
    let mut command = std::process::Command::new(exe);
    command.arg("--new-window").stdin(std::process::Stdio::null());
    if let gezik_core::nav::Location::Path(dir) = nav.active_location() {
        command.arg("--").arg(dir);
    }
    match command.spawn() {
        // Waited for on a thread of its own, so its exit leaves no zombie behind.
        #[cfg(unix)]
        Ok(mut child) => {
            let _ = std::thread::Builder::new().name("gezik-window".into()).spawn(move || child.wait());
        }
        #[cfg(not(unix))]
        Ok(_) => {}
        Err(err) => view.note(format!("Cannot open a new window: {}", gezik_platform::fs::describe(&err))),
    }
}
