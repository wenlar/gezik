// Hide the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actions;
mod admin;
mod archives;
mod batch_rename;
mod checksums;
mod cli;
mod cloud;
mod conflicts;
mod connect;
mod context_menu;
mod convert;
mod copy_path;
mod copy_to;
mod dialog;
mod diff;
mod drag;
mod dual;
mod eject;
mod elevated;
mod export;
mod filter;
mod finder_menu;
mod folder_sizes;
mod folder_watch;
mod frame_limit;
mod info;
mod integration;
mod keys;
mod media;
#[cfg(target_os = "macos")]
mod menu_bar;
mod navigation;
mod op_history;
mod operations;
mod palette;
mod panes;
mod path_box;
mod pdf;
mod places;
mod popup;
mod preview;
mod quick_look;
mod resident;
mod saved_searches;
mod search;
mod select_tools;
mod sidebar;
mod sidebar_model;
mod single_instance;
mod size_map;
mod stack;
mod start;
mod system_changes;
mod tab_drag;
mod tab_sets;
mod tab_tools;
mod templates;
mod terminal;
mod theme_bridge;
mod trash_view;
mod view;
mod view_options;
mod watcher;
mod window_state;
mod windows;

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use gezik_config::Warning;
use gezik_config::settings::{Settings, SidebarPosition, Typing};
use gezik_config::shortcuts::{Action, Chord, Key, Platform};
use gezik_config::store::{self, ConfigFiles, ConfigStore, Loaded};
use gezik_config::theme;
use gezik_core::layout::Move;
use gezik_core::nav::Session;
use gezik_platform::instance;
use start::StartPlan;

slint::include_modules!();

/// Opens entry `index` of the list (a double-click, or a click with single-click-open): an
/// archive is extracted next to itself if `[archives] double-click` says so.
fn open_entry(nav: &navigation::Navigator, view: &view::View, index: usize) {
    if let Some((path, is_dir)) = view.entry_path(index) {
        open_path(nav, path, is_dir);
    }
}

/// Opens the entry at `path` as [`open_entry`] does, for a caller that fixed the entry earlier.
fn open_path(nav: &navigation::Navigator, path: PathBuf, is_dir: bool) {
    if nav.active_location() == gezik_core::nav::Location::Trash {
        return nav.status(trash_view::open_note());
    }
    let archive = !is_dir
        && path.file_name().is_some_and(|n| gezik_core::batch::archive::looks_like_archive(&n.to_string_lossy()));
    if archive && archives::extracts_on_double_click() {
        archives::with_current(|archives| archives.extract_here(vec![path]));
    } else {
        nav.open_item(path, is_dir);
    }
}

/// A click on the sidebar's saved search `index` (middle click: in a new tab).
fn run_saved_search(index: i32, new_tab: bool) {
    let names = saved_searches::names();
    if let Some(name) = usize::try_from(index).ok().and_then(|i| names.get(i)) {
        saved_searches::with_current(|s| s.run(name, new_tab));
    }
}

/// Resolves settings + theme from `files` and shows them in every window. No I/O, so it runs
/// on the UI thread at startup, after config files change and when the system theme flips.
fn apply_config(files: &ConfigFiles) -> Loaded {
    let dark = windows::with_focused(AppWindow::get_system_dark).unwrap_or_default();
    let loaded = store::resolve(files, dark);
    // First: a pane told `[session] restore` below writes the tabs as it now says.
    windows::set_loaded(loaded.clone());
    let new = not_printed_yet(&mut PRINTED.lock().unwrap_or_else(std::sync::PoisonError::into_inner), &loaded.warnings);
    for warning in new {
        eprintln!("gezik: {warning}");
    }
    for p in panes::everywhere() {
        p.nav.set_session_restore(loaded.settings.session.restore);
    }
    dual::set_confirm(loaded.settings.panes_confirm);
    // The shortcuts first: the pins' tips name their keys.
    keys::set_shortcuts(loaded.settings.shortcuts.clone());
    // 9b9: the tray and the shortcut follow [system] (the tray menu follows the sidebar's pins).
    resident::apply(&loaded.settings.system);
    // The defaults first: a rule change then switches the view once.
    let rules = view::compile_rules(&loaded.settings.view_rules);
    for p in panes::everywhere() {
        p.view.set_defaults(loaded.settings.view);
        p.view.set_rules(rules.clone(), || p.nav.rule_place());
    }
    view_options::set_from_file(loaded.settings.view.options);
    frame_limit::set_max_fps(loaded.settings.max_fps);
    operations::with_current(|ops| ops.set_files(loaded.settings.files));
    batch_rename::set_presets(loaded.settings.rename_presets.clone());
    archives::set_settings(loaded.settings.tools.clone(), loaded.settings.archives);
    diff::set_settings(&loaded.settings.tools);
    preview::set_quick_look(loaded.settings.system.quick_look);
    convert::set_settings(loaded.settings.convert.clone(), loaded.settings.commands.clone());
    filter::set_settings(loaded.settings.keyboard, loaded.settings.filters.clone());
    path_box::set_settings(loaded.settings.history);
    for p in panes::everywhere() {
        p.search.set_settings(loaded.settings.search.clone());
        p.folder_sizes.set_settings(loaded.settings.folder_sizes, loaded.settings.search.everything);
    }
    tab_sets::set_settings(loaded.settings.tab_sets.clone());
    saved_searches::set_settings(loaded.settings.searches.clone());
    terminal::set_settings(loaded.settings.terminal.command.clone());
    // Every window shows them alike (spec 9.3, decision 46).
    windows::for_each(|window| {
        dual::set_restore(loaded.settings.session.restore);
        apply_to_window(window, &loaded);
    });
    loaded
}

/// The monospace font of this system (the preview's text, the quick look window).
fn mono_font() -> &'static str {
    if cfg!(windows) {
        "Consolas"
    } else if cfg!(target_os = "macos") {
        "Menlo"
    } else {
        "monospace"
    }
}

/// What a window shows of the settings: the theme, the notice, the sidebar and the menus.
fn apply_to_window(window: &AppWindow, loaded: &Loaded) {
    if let Some(theme) = &loaded.theme {
        theme_bridge::apply(window, theme);
    }
    theme_bridge::set_reduce_motion(window, loaded.settings.reduce_motion);
    window.set_notice(notice_text(&loaded.warnings).into());
    window.set_sidebar_position(match loaded.settings.sidebar {
        SidebarPosition::Left => 0,
        SidebarPosition::Right => 1,
        SidebarPosition::Hidden => 2,
    });
    // A reload may hide or move the sidebar (or change its pins): no tip stays behind.
    window.set_sidebar_tip("".into());
    // Unchanged pins cost nothing (also after the reload that follows our own save).
    sidebar::with_current(|sidebar| sidebar.set_pinned(loaded.settings.pinned.clone()));
    sidebar::with_current(sidebar::Sidebar::relabel);
    sidebar::with_current(|s| s.set_show_cloud(loaded.settings.sidebar_cloud));
    sidebar::with_current(|s| s.set_tree_follow(loaded.settings.sidebar_tree_follow));
    view_options::sync(window, view_options::current());
    window.set_mono_font(mono_font().into());
    // Gezik's own menus (where Slint has no system ones) stay inside the window.
    window.set_native_menus(context_menu::native_menus());
    #[cfg(target_os = "macos")]
    menu_bar::set_commands(window, &loaded.settings.commands);
    #[cfg(target_os = "macos")]
    menu_bar::set_tab_sets(window, &tab_sets::names());
}

/// Closes tab `index` once the current event is fully handled (the last tab closes the
/// window). The tab is remembered by id, so a close that runs after other tab changes, or
/// a second close of the same tab, never hits another tab.
fn close_tab_later(nav: &navigation::Navigator, index: usize) {
    if let Some(id) = nav.tab_id(index) {
        let nav = nav.clone();
        slint::Timer::single_shot(std::time::Duration::ZERO, windows::bind(move || nav.close_tab_by_id(id)));
    }
}

/// A Miller column's width by default (spec 10 §7.1; state.toml `[panes] column-width`).
const COLUMN_WIDTH: u32 = 220;

/// The pane on row `row` (a Slint callback's), made the active one: a click or an action in a
/// pane picks it (spec 10 §4.3).
fn pick(row: i32) -> Option<panes::Pane> {
    dual::pick(row);
    panes::at(row)
}

/// Runs `action` as its key would (the key handler and the palette, sapma 5); false when it did
/// nothing (a pin number with no pin), so the key goes on to the text box.
fn perform(
    action: Action,
    window: &AppWindow,
    nav: &navigation::Navigator,
    view: &view::View,
    preview: &preview::Preview,
    ops: &operations::Operations,
) -> bool {
    // The trash (spec 7.1): Delete is for good there, and what is not on its list does not run.
    if trash_view::instead(action, view) {
        return true;
    }
    match action {
        Action::NewTab => nav.open_tab(nav.start(), true),
        Action::CloseTab => close_tab_later(nav, nav.active_index()),
        Action::NextTab => nav.next_tab(),
        Action::PrevTab => nav.prev_tab(),
        Action::Back => nav.back(),
        Action::Forward => nav.forward(),
        Action::Up => nav.up(),
        Action::FocusPath => {
            panes::with_active(|p| p.path_box.reset());
            panes::edit(view.pane_id(), |d| panes::path_editing(d, true))
        }
        Action::Refresh => nav.reload(),
        Action::SelectAll => view.select_all(),
        Action::ViewList => view.set_mode(gezik_core::view::ViewMode::List),
        Action::ViewGrid => view.set_mode(gezik_core::view::ViewMode::Grid),
        Action::ViewColumns => {
            if !view.show_columns(&nav.active_location()) {
                nav.note(view::LIST_ONLY.to_owned());
            }
        }
        Action::TogglePreview => preview.toggle_pane(),
        Action::QuickLook => preview.toggle_quick_look(),
        Action::Share => finder_menu::share_selection(window, view),
        Action::Rename => ops.rename_start(),
        Action::NewFolder => ops.new_folder(None),
        Action::Copy => ops.copy(false),
        Action::Cut => ops.copy(true),
        Action::Paste => ops.paste(None, false),
        Action::PasteMove => ops.paste(None, true),
        Action::Trash => ops.trash(false),
        Action::DeletePermanently => ops.trash(true),
        Action::Duplicate => ops.duplicate(),
        Action::BatchRename => ops.batch_rename(),
        Action::Undo => ops.undo(),
        Action::Redo => ops.redo(),
        Action::CommandPalette => palette::with_current(|p| p.open(true)),
        Action::QuickOpen => palette::with_current(|p| p.open(false)),
        Action::Filter
        | Action::InvertSelection
        | Action::SelectPattern
        | Action::DeselectPattern
        | Action::SelectSameType
        | Action::RestoreSelection
        | Action::Tab1
        | Action::Tab2
        | Action::Tab3
        | Action::Tab4
        | Action::Tab5
        | Action::Tab6
        | Action::Tab7
        | Action::Tab8
        | Action::TabLast
        | Action::ReopenTab
        | Action::TabPicker
        | Action::ToggleTabLock
        | Action::ClearHistory
        | Action::OpenTerminal
        | Action::OpenTerminalAdmin
        | Action::CopyPath
        | Action::SaveTabSet
        | Action::ToggleHidden
        | Action::Pin1
        | Action::Pin2
        | Action::Pin3
        | Action::Pin4
        | Action::Pin5
        | Action::Pin6
        | Action::Pin7
        | Action::Pin8
        | Action::Pin9
        | Action::NewFolderWithSelection
        | Action::AddToStack
        | Action::ToggleStack
        | Action::ShowHistory
        | Action::Search
        | Action::FlatView
        | Action::ShowInFolder
        | Action::CopyWithFolders
        | Action::CutWithFolders
        | Action::CalculateFolderSizes
        | Action::SaveSearch
        | Action::ShowTrash
        | Action::PutBack
        | Action::EmptyTrash
        | Action::SystemIntegration
        | Action::KeepOffline
        | Action::FreeUpSpace
        | Action::NewWindow
        | Action::MakeAlias
        | Action::ShowPackageContents
        | Action::GetInfo
        | Action::ConnectToServer
        | Action::Eject
        | Action::GroupNone
        | Action::GroupType
        | Action::GroupDate
        | Action::GroupSize
        | Action::CollapseGroups
        | Action::ExpandGroups
        | Action::RevealInTree
        | Action::ToggleDualPane
        | Action::FocusOtherPane
        | Action::CopyToOtherPane
        | Action::MoveToOtherPane
        | Action::MoveTabToOtherPane
        | Action::SyncBrowsing
        | Action::SwapPanes
        | Action::OtherPaneSameFolder
        | Action::MoveTabToNewWindow
        | Action::Compare
        | Action::ExportList
        | Action::SizeMap
        | Action::Checksums
        | Action::VerifyChecksums
        | Action::CopyTo
        | Action::MoveTo
        | Action::SplitFile
        | Action::JoinFiles => return actions::run(action, nav, view),
    }
    true
}

/// Handles a key press before the focused item sees it; returns whether it was used.
/// Shortcuts work everywhere; with the address bar in typing mode, unmodified keys and
/// the text editing shortcuts stay with the text box. The list keys (arrows, PgUp/PgDn,
/// Home/End, Enter, type-ahead) are fixed and only act while the list has the focus.
/// `chord` is the key as a shortcut (if it can be one); `has_modifier` is Ctrl, Alt or
/// Meta (not Shift).
#[allow(clippy::too_many_arguments, reason = "the key event, split up, and what it acts on")]
fn handle_key(
    window: &AppWindow,
    nav: &navigation::Navigator,
    view: &view::View,
    preview: &preview::Preview,
    ops: &operations::Operations,
    type_ahead: &mut keys::TypeAhead,
    text: &str,
    chord: Option<Chord>,
    has_modifier: bool,
    menu_key: bool,
) -> bool {
    // A question or the conflict list over the window has the keyboard.
    if window.get_dialog_open() || window.get_conflicts_open() {
        return false;
    }
    // The batch rename layer: Esc and Ctrl+Enter wherever its focus is, other keys to it.
    if window.get_rb_open() {
        let mut used = false;
        if let Some(chord) = &chord {
            batch_rename::with_current(|layer| used = layer.chord(chord));
        }
        return used;
    }
    // The Compress layer: Esc and Ctrl+Enter wherever its focus is, other keys to its fields.
    if window.get_cp_open() {
        let mut used = false;
        if let Some(chord) = &chord {
            archives::with_current(|layer| used = layer.chord(chord));
        }
        return used;
    }
    // The Convert layer: the same.
    if window.get_cv_open() {
        let mut used = false;
        if let Some(chord) = &chord {
            convert::with_current(|layer| used = layer.chord(chord));
        }
        return used;
    }
    // The Checksums layer (the tool table): Esc, the row keys, and Ctrl+C / Ctrl+A unless its
    // field has the keyboard.
    if window.get_tool_open() {
        let mut used = false;
        if let Some(chord) = &chord {
            checksums::with_current(|layer| used = layer.chord(chord, window.get_tool_field_focused()));
        }
        return used;
    }
    // The size map: every key is its own.
    if window.get_sm_open() {
        if let Some(chord) = &chord {
            size_map::with_current(|map| {
                map.chord(chord);
            });
        }
        return true;
    }
    // The Info window: Esc wherever its focus is, other keys to its fields.
    if window.get_info_open() {
        let mut used = false;
        if let Some(chord) = &chord {
            info::with_current(|info| used = info.chord(chord));
        }
        return used;
    }
    // The file compare: its keys wherever its focus is.
    if window.get_df_open() {
        let mut used = false;
        if let Some(chord) = &chord {
            diff::with_current(|d| used = d.chord(chord));
        }
        return used;
    }
    // The tab picker or the palette: Esc, Enter, Up and Down are theirs, other keys go to the field.
    if window.get_tp_open() {
        let mut used = false;
        if let Some(chord) = &chord {
            if integration::is_open() {
                integration::with_current(|i| used = i.chord(chord));
            } else if palette::is_open() {
                palette::with_current(|p| used = p.chord(chord));
            } else {
                tab_tools::with_current(|t| used = t.chord(chord));
            }
        }
        return used;
    }
    // The name field scrolled off screen gave the list the keyboard: Enter and Esc are still
    // the rename's (file-view.slint `rename-focused`).
    if let Some(chord) = &chord
        && !has_modifier
        && !chord.shift
        && matches!(chord.key, Key::Enter | Key::Escape)
        && window.get_list_focused()
        && ops.off_screen_rename_key(chord.key == Key::Enter)
    {
        return true;
    }
    // The name field being edited has the keyboard (Enter, Esc, Tab are its own).
    if ops.end_unfocused_rename() {
        return false;
    }
    let mirror = panes::mirror(view.pane_id());
    let editing = mirror.path_editing.get();

    if editing && let Some(chord) = &chord {
        // The suggestion list's keys first: ↓ ↑ Tab → Enter Esc (spec 6.1).
        if !has_modifier {
            let mut used = false;
            panes::with_active(|p| used = p.path_box.chord(chord));
            if used {
                return true;
            }
        }
        if chord.key == Key::Escape && !has_modifier {
            window.invoke_focus_list();
            panes::with_active(|p| p.path_box.end_editing());
            return true;
        }
        if !has_modifier || keys::is_text_edit(chord, Platform::current()) {
            return false;
        }
    }

    // The search bar's fields (spec 4.2): Esc stops or closes, Enter searches, Alt+Enter in a
    // new tab, Down gives the list the keyboard; other plain keys are the field's.
    let in_search = mirror.focus.borrow().search;
    if in_search && let Some(chord) = &chord {
        let plain = !has_modifier && !chord.shift;
        let alt_only = chord.alt && !chord.ctrl && !chord.meta && !chord.shift;
        match chord.key {
            Key::Escape if plain => {
                panes::with_active(|p| p.search.escape());
                return true;
            }
            Key::Enter if plain => {
                panes::with_active(|p| p.search.go(false));
                return true;
            }
            Key::Enter if alt_only => {
                panes::with_active(|p| p.search.go(true));
                return true;
            }
            Key::Down if plain => {
                window.invoke_focus_list();
                return true;
            }
            _ => {}
        }
        if !has_modifier || keys::is_text_edit(chord, Platform::current()) {
            return false;
        }
    }

    // The filter bar's field: Esc closes the filter, Down or Enter give the list the keyboard
    // (the bar stays); other plain keys and the text editing shortcuts are the field's.
    let filtering = mirror.focus.borrow().filter;
    if filtering && let Some(chord) = &chord {
        // Shift+Enter: the filter's pattern searched in the subfolders (spec 4.1).
        if chord.shift && !has_modifier && chord.key == Key::Enter {
            panes::with_active(|p| p.search.filter_to_search());
            return true;
        }
        if !has_modifier && !chord.shift {
            match chord.key {
                Key::Escape => {
                    panes::with_active(|p| p.filter.close());
                    return true;
                }
                Key::Down | Key::Enter => {
                    window.invoke_focus_list();
                    return true;
                }
                _ => {}
            }
        }
        if !has_modifier || keys::is_text_edit(chord, Platform::current()) {
            return false;
        }
    }

    // The sidebar tree has the keyboard (spec 10 §5.1): its keys first; shortcuts go on below,
    // the list's own (Delete, F2 …) do not, the list not having the keyboard.
    if window.get_sidebar_focused() && !window.get_list_focused() && !editing && !filtering && !in_search {
        // The menu key has no chord: it is the tree's too.
        let mut used = false;
        sidebar::with_current(|s| used = s.key(chord.as_ref(), text, has_modifier, menu_key));
        if used {
            return true;
        }
    }
    if let Some(action) = chord.as_ref().and_then(keys::action_for) {
        // Space opens quick look only on the focused list and outside type-ahead; elsewhere
        // it is an ordinary key.
        let ordinary_key = action == Action::QuickLook
            && (!window.get_list_focused() || type_ahead.is_active(std::time::Instant::now()));
        if !ordinary_key {
            if keys::waits_for_text_fields(action)
                && (editing || filtering || in_search || (keys::needs_list(action) && !window.get_list_focused()))
            {
                return false;
            }
            if action == Action::Filter && editing {
                panes::with_active(|p| p.path_box.end_editing());
            }
            if !perform(action, window, nav, view, preview, ops) {
                return false;
            }
            if action == Action::ToggleDualPane && chord.as_ref().is_some_and(|c| c.key == Key::F(3)) && dual::is_open()
            {
                dual::f3_pressed();
            }
            // The typed text no longer fits once the location or tab changed.
            if (editing || filtering || in_search)
                && !matches!(action, Action::FocusPath | Action::Filter | Action::Search)
            {
                window.invoke_focus_list();
            }
            return true;
        }
    }
    // A `[[commands]]` key: on the selection, only while the file list has the keyboard.
    if let Some(index) = chord.as_ref().and_then(keys::command_for) {
        if editing || filtering || in_search || !window.get_list_focused() {
            return false;
        }
        actions::run_command(index, view);
        return true;
    }
    if editing || !window.get_list_focused() {
        return false;
    }

    // Shift+F10 or the Menu key: the selection's menu (the background's if none).
    if menu_key {
        panes::edit(view.pane_id(), panes::keyboard_menu);
        return true;
    }

    // List keys (fixed): arrows, PgUp/PgDn, Home/End move; Shift extends, the primary
    // modifier (Ctrl, Cmd on macOS) moves only the focus. Enter opens, Ctrl+Space flips
    // the focused entry, Esc clears the selection.
    if let Some(chord) = &chord {
        let platform = Platform::current();
        let primary = keys::is_primary(chord, platform);
        let other = chord.alt || if platform == Platform::Mac { chord.ctrl } else { chord.meta };
        if !other {
            // On macOS Enter renames, so opening is Cmd+Down (as in Finder).
            if platform == Platform::Mac && primary && !chord.shift && chord.key == Key::Down {
                nav.open_selected();
                return true;
            }
            // Miller columns (spec 10 §7.1): → into the selected folder, ← back to the left.
            if !primary
                && !chord.shift
                && matches!(chord.key, Key::Left | Key::Right)
                && view.column_key(chord.key == Key::Right)
            {
                return true;
            }
            let mv = match chord.key {
                Key::Up => Some(Move::Up),
                Key::Down => Some(Move::Down),
                Key::Left => Some(Move::Left),
                Key::Right => Some(Move::Right),
                Key::PageUp => Some(Move::PageUp),
                Key::PageDown => Some(Move::PageDown),
                Key::Home => Some(Move::Home),
                Key::End => Some(Move::End),
                _ => None,
            };
            if let Some(mv) = mv {
                let page = usize::try_from(mirror.geometry.borrow().page_rows).unwrap_or(1).max(1);
                return view.key_move(mv, chord.shift, primary, page);
            }
            match chord.key {
                Key::Enter if !primary && !chord.shift => {
                    nav.open_selected();
                    return true;
                }
                Key::Space if primary && !chord.shift => {
                    view.toggle_focus();
                    return true;
                }
                // The first Esc stops a running search, then closes the filter, then clears the selection.
                Key::Escape if !primary && !chord.shift => {
                    if search::running() {
                        panes::with_active(|p| p.search.stop());
                    } else if view.filter_text().is_some() {
                        panes::with_active(|p| p.filter.close());
                    } else {
                        view.clear_selection();
                    }
                    return true;
                }
                _ => {}
            }
        }
    }
    if has_modifier {
        return false;
    }

    let Some(c) = keys::typed_char(text) else { return false };
    // `/` is in no name: it opens the filter whatever the typing mode (and on a layout where
    // it needs Shift, as no shortcut could).
    if c == '/' {
        panes::with_active(|p| p.filter.open());
        return true;
    }
    // A space is no letter: it stays quick look's (or type-ahead's, as before).
    if filter::typing() == Typing::Filter && c != ' ' && !view.shows_drives() {
        panes::with_active(|p| p.filter.typed(c));
        return true;
    }
    // Type-ahead. A typed character that matches nothing is still used up.
    if let Some(i) = type_ahead.type_char(c, std::time::Instant::now(), |typed| view.find_prefix(typed)) {
        view.jump_to(i);
    }
    true
}

/// Like [`apply_config`], and also resolves where the app opens. Start warnings (bad
/// `start-folder`, missing command-line path) are added to `files` so the notice shows
/// them (also after later re-resolves, until the files are read again).
fn apply_config_and_start(
    files: &mut ConfigFiles,
    cli: &[cli::Target],
    new_tab: bool,
    saved: Option<&Session>,
) -> (Settings, StartPlan) {
    let loaded = apply_config(files);
    let plan = resolve_start(&loaded.settings, cli, new_tab, false, saved);
    if !plan.warnings.is_empty() {
        files.warnings.extend(plan.warnings.iter().cloned());
        apply_config(files);
    }
    (loaded.settings, plan)
}

/// [`start::plan_start`] against the real file system.
fn resolve_start(
    settings: &Settings,
    cli: &[cli::Target],
    new_tab: bool,
    trash: bool,
    saved: Option<&Session>,
) -> StartPlan {
    let saved = saved.filter(|_| settings.session.restore);
    let dirs = gezik_config::paths::KnownDirs::system();
    start::plan_start(
        &settings.start_folder,
        cli,
        new_tab,
        trash,
        &path_box::home(),
        |text| dirs.expand_checked(text),
        start::path_kind,
        saved,
    )
}

/// What a `gezik` does once it tried to hand its request over (spec 5.2, spec 10 §9.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Next {
    /// The running Gezik did it (with `--new-window`: in a new window of its own).
    Exit,
    /// No Gezik runs: this one is the first (with `--new-window` too).
    First,
    /// Hung, refusing (an older Gezik refuses `--new-window`'s flag) or someone else's: a
    /// process of its own, the channel left alone.
    OwnProcess,
    /// A hung Gezik and a place the shell asked for: it still opens somewhere.
    Explorer,
}

fn after_send(sent: instance::Sent, shell: bool) -> Next {
    match sent {
        instance::Sent::Delivered => Next::Exit,
        instance::Sent::NoInstance => Next::First,
        instance::Sent::Failed if shell => Next::Explorer,
        instance::Sent::Failed => Next::OwnProcess,
    }
}

/// `--new-window` with `[system] single-instance = false`: no Gezik answers it, yet it is a
/// second process as before 10g (no saved windows, no state written, not the tray's).
fn own_process(new_window: bool, single_instance: bool) -> bool {
    new_window && !single_instance
}

/// Hands `target` to Explorer (decisions 2, 3), unless fallbacks come so fast that Gezik and
/// Explorer are sending places back and forth: then it stops and offers once to give folders
/// back to Explorer. The exit code.
#[cfg(windows)]
fn explorer_fallback(target: &str) -> i32 {
    use gezik_platform::shell_fallback as fb;
    let file = std::env::temp_dir().join(fb::GUARD_FILE);
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (record, guard) = fb::guard(&std::fs::read_to_string(&file).unwrap_or_default(), now);
    let _ = std::fs::write(&file, record);
    let question = "Only Explorer can show these places, and it keeps sending them back to Gezik.\n\n\
                    Restore Explorer as the default file manager?";
    match guard {
        fb::Guard::Go => match fb::open_in_explorer(target) {
            Ok(()) => 0,
            Err(err) => {
                eprintln!("gezik: --shell {target:?}: {err}");
                1
            }
        },
        fb::Guard::Ask if fb::ask_restore(question) => i32::from(system_changes::restore_default_now().is_err()),
        fb::Guard::Ask | fb::Guard::Stop => 1,
    }
}

#[cfg(not(windows))]
fn explorer_fallback(_: &str) -> i32 {
    1
}

/// The warnings last printed: each config resolve prints only new ones (the list carries the
/// same ones from resolve to resolve).
static PRINTED: Mutex<Vec<Warning>> = Mutex::new(Vec::new());

/// Of `now`, those not in `printed`; `printed` becomes `now`.
fn not_printed_yet(printed: &mut Vec<Warning>, now: &[Warning]) -> Vec<Warning> {
    let new = now.iter().filter(|w| !printed.contains(w)).cloned().collect();
    *printed = now.to_vec();
    new
}

/// The first warning, plus how many more there are.
fn notice_text(warnings: &[Warning]) -> String {
    match warnings {
        [] => String::new(),
        [only] => only.to_string(),
        [first, rest @ ..] => format!("{first} (+{} more)", rest.len()),
    }
}

/// The native window only exists once the event loop runs (not yet on the first tick),
/// so retry shortly until it does, then move window `id` on screen if needed.
fn keep_on_screen(id: windows::WindowId, window: slint::Weak<AppWindow>, attempt: u32) {
    slint::Timer::single_shot(std::time::Duration::from_millis(10), move || {
        let Some(strong) = window.upgrade() else { return };
        let shown = windows::with_window(id, |_| window_state::ensure_visible(&strong)).unwrap_or(true);
        if !shown && attempt < 200 {
            keep_on_screen(id, window, attempt + 1);
        }
    });
}

/// Windows signs out or shuts down without a close request: what the window shows is saved
/// first (on the native window, once it exists).
#[cfg(windows)]
fn save_on_session_end(window: slint::Weak<AppWindow>, save: Rc<dyn Fn()>, attempt: u32) {
    slint::Timer::single_shot(std::time::Duration::from_millis(10), move || {
        let Some(strong) = window.upgrade() else { return };
        let handle = strong.window().window_handle();
        let save_now = save.clone();
        if !gezik_platform::on_session_end(&handle, move || save_now()) && attempt < 200 {
            save_on_session_end(window, save, attempt + 1);
        }
    });
}

fn main() -> Result<(), slint::PlatformError> {
    // The administrator helper (`gezik --elevated …`, spec 9 §10): started by the system's
    // prompt for one list; before anything else, the PDF worker, Slint, settings or the single
    // instance (its first step hardens the DLL search).
    if std::env::args_os().nth(1).is_some_and(|arg| arg == gezik_core::elevated::ARG) {
        std::process::exit(elevated::main());
    }
    // The PDF worker (`gezik --pdf-worker`) is this exe run by Gezik itself: it does one PDF
    // request with pdfium and exits, before any window, Slint or settings. Its pipes come from
    // the handles `ChildProcess` gives it, so this works in the windowless release build too.
    if std::env::args_os().nth(1).is_some_and(|arg| arg == gezik_batch::pdf::client::WORKER_ARG) {
        std::process::exit(gezik_batch::pdf::worker::main());
    }
    // The command line (spec 5.1), before Slint and the settings: --help and --version only print.
    let mut cli = cli::parse(std::env::args_os().skip(1), cfg!(windows));
    if cli.help || cli.version {
        // The Windows release build has no console of its own: print to the caller's.
        instance::attach_console();
        if cli.version {
            println!("gezik {}", env!("CARGO_PKG_VERSION"));
        } else {
            print!("{}", cli::HELP);
        }
        return Ok(());
    }
    // Undo the system changes and exit (spec 11.4): never handed to a running Gezik, no window.
    if cli.unregister {
        instance::attach_console();
        std::process::exit(system_changes::unregister_cli());
    }
    for warning in &cli.warnings {
        eprintln!("gezik: {warning}");
    }
    if cfg!(windows) && cli.dbus {
        eprintln!("gezik: --dbus is for Linux (ignored)");
    }
    if !cfg!(windows) && cli.shell.is_some() {
        eprintln!("gezik: --shell is for Windows (ignored)");
    }
    // Windows' folder verb and Win+E (spec 6.1): a place only Explorer shows never opens a window.
    let shell = cli.shell.take().filter(|_| cfg!(windows));
    if let Some(target) = &shell {
        match cli::shell_target(target, start::path_kind) {
            cli::Shell::Explorer => std::process::exit(explorer_fallback(target)),
            cli::Shell::Open(t) => cli.targets = vec![t],
            cli::Shell::Trash => cli.trash = true,
            cli::Shell::StartFolder => {}
        }
    }
    cli.make_absolute();
    // A running Gezik takes the paths (spec 5.2): tried before any window or settings, so a
    // second call costs only the attempt. --new-window asks it for a new window (spec 10 §9.3).
    let key = instance::key(gezik_config::paths::config_dir().as_deref());
    let request = cli.request();
    // A window that could not hand over is a process of its own (spec 5.3): two processes
    // writing the same tabs to state.toml would lose one's.
    let mut secondary = false;
    let background = cli.background;
    // Started by the bus for FileManager1 (decision 18): the channel decides, nothing is sent.
    let dbus = cli.dbus && cfg!(all(unix, not(target_os = "macos")));
    // Decision 16: LaunchServices starts the bundle bare and hands the folders over by event
    // after start; sending a bare request first would bring a running Gezik forward for nothing.
    let bundle_launch = cfg!(target_os = "macos")
        && cli.targets.is_empty()
        && !cli.new_window
        && gezik_platform::system::exe()
            .ok()
            .is_some_and(|e| e.to_str().is_some_and(|s| s.ends_with(".app/Contents/MacOS/gezik")));
    if !secondary && !dbus && !bundle_launch && !background {
        match after_send(instance::send(&key, &request, instance::SEND_TIMEOUT), shell.is_some()) {
            Next::Exit => return Ok(()),
            Next::First => {}
            Next::Explorer => std::process::exit(explorer_fallback(shell.as_deref().unwrap_or_default())),
            Next::OwnProcess => secondary = true,
        }
    }
    // Gezik has its own tabs: no window tabs of macOS (nor their items in the View menu).
    #[cfg(target_os = "macos")]
    gezik_platform::app::no_window_tabs();
    let window = match AppWindow::new() {
        Ok(window) => window,
        Err(err) => {
            // Spec 6.3: a folder asked for by the shell still opens somewhere.
            if let Some(target) = &shell {
                explorer_fallback(target);
            }
            return Err(err);
        }
    };
    // The first window: in the register from now on (the settings are shown in every window).
    let first = windows::install(window.clone_strong());
    // Every way out of main lets the windows go here, before the thread's own teardown.
    let _release = windows::Release;

    let config = ConfigStore::system();
    templates::set_dir(config.as_ref().map(ConfigStore::templates_dir));
    view_options::install(config.clone());
    let init_error = config.as_ref().and_then(|store| store.ensure_initialized().err().map(|e| (store, e)));
    let mut files = config.as_ref().map(ConfigStore::read_files).unwrap_or_default();
    if let Some((store, err)) = init_error {
        let why = gezik_platform::fs::describe(&err);
        files
            .warnings
            .push(Warning::new(store.dir().display().to_string(), format!("cannot create config folder: {why}")));
    }
    // Release builds hide stderr, so config problems also go to the status bar.
    if config.is_none() {
        eprintln!("gezik: no config folder available; using defaults");
        files.warnings.push(Warning::new("config", "no config folder available; using default settings"));
        PRINTED.lock().unwrap_or_else(std::sync::PoisonError::into_inner).extend(files.warnings.last().cloned());
    }
    let from_cli = cli.warnings.iter().map(|w| Warning::new("command line", w.clone()));
    // Printed already, before the window opened.
    PRINTED.lock().unwrap_or_else(std::sync::PoisonError::into_inner).extend(from_cli.clone());
    files.warnings.extend(from_cli);
    // Something sensible is on screen even if the selected theme cannot be read.
    theme_bridge::apply(&window, &theme::builtin_dark());
    let saved_state = config.as_ref().map(ConfigStore::load_state).unwrap_or_default();
    // As apply_config_and_start, split so the channel is claimed before the session is chosen.
    let initial_settings = apply_config(&files).settings;
    // With single instance off, --new-window found no Gezik to ask: still a process of its own.
    secondary |= own_process(cli.new_window, initial_settings.system.single_instance);
    let mut taken = false;
    let listener = if !secondary && initial_settings.system.single_instance {
        match instance::claim(&key) {
            instance::Claim::Listening(listener) => Some(listener),
            // The running Gezik holds FileManager1 already (decision 18).
            instance::Claim::Taken if dbus => return Ok(()),
            // Deviation 12: a Gezik is running already; start at login has nothing to add.
            instance::Claim::Taken if background => return Ok(()),
            // The folders come by event, later: handed on below.
            instance::Claim::Taken if bundle_launch => {
                taken = true;
                None
            }
            // Another Gezik started at the same moment and took it: it gets the paths.
            instance::Claim::Taken => {
                if instance::send(&key, &request, instance::SEND_TIMEOUT) == instance::Sent::Delivered {
                    return Ok(());
                }
                secondary = true;
                None
            }
            instance::Claim::Off => None,
        }
    } else {
        None
    };
    #[cfg(target_os = "macos")]
    if taken {
        // Decision 16: no window yet; LaunchServices' folders (none within two seconds: the
        // running Gezik just comes forward) go to the running Gezik, and this one ends.
        let got: std::rc::Rc<std::cell::RefCell<Vec<std::path::PathBuf>>> = std::rc::Rc::default();
        let keep = got.clone();
        gezik_platform::open_urls::install(Box::new(move |paths| {
            *keep.borrow_mut() = paths;
            let _ = slint::quit_event_loop();
        }));
        let wait = slint::Timer::default();
        wait.start(slint::TimerMode::SingleShot, std::time::Duration::from_secs(2), || {
            let _ = slint::quit_event_loop();
        });
        slint::run_event_loop_until_quit()?;
        drop(wait);
        let targets: Vec<cli::Target> =
            got.take().into_iter().map(|path| cli::Target { path, select: false }).collect();
        let handed = instance::Request { targets: targets.clone(), ..Default::default() };
        if instance::send(&key, &handed, instance::SEND_TIMEOUT) == instance::Sent::Delivered {
            return Ok(());
        }
        // It went away (or hangs) meanwhile: a window of its own, as Claim::Taken's other arm.
        cli.targets = targets;
        secondary = true;
    }
    let _ = taken;
    // A second window keeps the first one's tabs: it neither restores nor records them, and
    // writes no state nor folder views.
    if secondary && let Some(store) = &config {
        store.keep_state_unwritten();
    }
    // With single instance off no Gezik holds the channel: a lock of their own keeps the tray
    // and the shortcut to one Gezik, and a --background start beside a running one does nothing.
    // shortcut: taken only if they are on at start; a second window turning them on later from
    // the panel gets its own (rare: single instance off), take it in turn_on_tray if that matters.
    let mut resident_primary = !secondary;
    let system = &initial_settings.system;
    let wants_lock = !system.single_instance && (background || system.tray || system.hotkey.is_some());
    let _resident_lock = if !secondary && wants_lock {
        match instance::claim(&format!("{key}-resident")) {
            instance::Claim::Listening(lock) => Some(lock),
            instance::Claim::Taken if background => return Ok(()),
            instance::Claim::Taken => {
                resident_primary = false;
                None
            }
            instance::Claim::Off => None,
        }
    } else {
        None
    };
    // Spec 9.3: hidden only where the tray can bring the window back.
    let hidden = resident::start_hidden(background, resident_primary, initial_settings.system.tray);
    resident::begin(config.clone(), resident_primary, !hidden, saved_state.tray_told);
    let plan = resolve_start(
        &initial_settings,
        &cli.targets,
        cli.new_tab,
        cli.trash,
        (!secondary).then_some(&saved_state.session),
    );
    if !plan.warnings.is_empty() {
        files.warnings.extend(plan.warnings.iter().cloned());
        apply_config(&files);
    }

    // Folder views; a broken views.toml starts over and says so in the status bar.
    let (memory, views_warning) = config.as_ref().map(ConfigStore::load_views).unwrap_or_default();
    files.warnings.extend(views_warning);

    // The latest config files, so a system light/dark switch can re-resolve without I/O.
    let files = Arc::new(Mutex::new(files));

    // Reading happens on the watcher thread; resolving and applying on the UI thread.
    let files_for_warning = files.clone();
    let _watcher = config.as_ref().and_then(|store| {
        let reader = store.clone();
        let files = files.clone();
        watcher::watch_config(
            store,
            move || {
                let fresh = reader.read_files();
                let files = files.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    let mut current = files.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                    *current = fresh;
                    let (_, plan) = apply_config_and_start(&mut current, &[], false, None);
                    for p in panes::everywhere() {
                        p.nav.set_start(plan.start.clone());
                    }
                });
            },
            {
                let dir = store.templates_dir();
                move || templates::refresh(dir.clone())
            },
        )
        .map_err(|err| {
            eprintln!("gezik: cannot watch {}: {err}", store.dir().display());
            let warning = Warning::new("config", format!("cannot watch the config folder for changes: {err}"));
            let mut current = files_for_warning.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            current.warnings.push(warning);
        })
        .ok()
    });
    apply_config(&files.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
    // One media and one folder view memory for the process; each view holds a client of the
    // media and the same memory (spec 10 §3.3).
    let media = media::Media::new();
    media.install();
    let memory = Rc::new(std::cell::RefCell::new(memory));
    // The templates of New ▸, read once the window is up.
    slint::Timer::single_shot(std::time::Duration::from_millis(500), templates::load_in_background);
    let engine_settings = gezik_ops::Settings {
        threads: initial_settings.files.copy_threads,
        pending_deletes: config.as_ref().map(|store| store.dir().join("pending-deletes")),
        verify: initial_settings.files.verify,
        max_speed: initial_settings.files.max_speed,
    };
    let ops = operations::Operations::new(
        &window,
        engine_settings,
        initial_settings.files,
        saved_state.operations_collapsed,
        config.clone(),
        saved_state.batch_rename.clone().unwrap_or_default(),
    );
    connect::install(config.clone(), saved_state.servers_recent.clone());
    // Deletes cut short last time finish in the background once the window is up.
    slint::Timer::single_shot(std::time::Duration::from_millis(500), {
        let ops = ops.clone();
        move || ops.recover()
    });
    // Whether symbolic links can be made (Windows: Developer Mode), tried once in the
    // background: Create link ▸ offers them from then on (spec 9.2).
    slint::Timer::single_shot(std::time::Duration::from_millis(500), || {
        let _ =
            std::thread::Builder::new().name("gezik-symlink-probe".into()).spawn(gezik_platform::link::probe_symlinks);
    });
    // Saves what the most recently focused window shows.
    let save_and_quit: Rc<dyn Fn()> = {
        let (store, ops) = (config.clone(), ops.clone());
        Rc::new(move || {
            if let Some(store) = &store {
                windows::with_focused(|window| {
                    let view = panes::active_view();
                    let preview_open = preview::with_current(preview::Preview::is_pane_open).unwrap_or(false);
                    store.update_state(|state| {
                        window_state::capture_into(window, state);
                        state.columns = Some(view.columns());
                        state.result_columns = Some(view.result_columns());
                        state.preview_open = preview_open;
                        state.preview_width = Some(window.get_preview_width().round().clamp(200.0, 600.0) as u32);
                        let column = window.get_column_width().round().clamp(120.0, 600.0) as u32;
                        state.column_width = (column != COLUMN_WIDTH).then_some(column);
                        state.operations_collapsed = ops.collapsed();
                    });
                    // Every window's tabs and panes, now (deviation 5).
                    windows::save_sessions();
                    // Quitting: what the other parts changed lately is written too (a saved
                    // filter, a pin or a default just sent to settings.toml as well).
                    store.flush_state();
                    store.flush_settings();
                    // A view change still waiting for its timer is handed over, then written.
                    view.flush_memory();
                    store.flush_views();
                });
            }
            // Its window would otherwise keep the event loop (and the process) running.
            windows::for_each(|_| {
                preview::with_current(preview::Preview::close_quick_look);
            });
        })
    };
    // Saves and ends the event loop (run_event_loop_until_quit: deviation 13).
    let finish: Rc<dyn Fn()> = {
        let save_and_quit = save_and_quit.clone();
        Rc::new(move || {
            save_and_quit();
            windows::for_each(|window| {
                let _ = window.hide();
            });
            let _ = slint::quit_event_loop();
        })
    };
    // Quit Gezik (the tray menu, the first close's question): running jobs ask first, over the window.
    let quit_app: Rc<dyn Fn()> = {
        let (ops, finish) = (ops.clone(), finish.clone());
        Rc::new(move || {
            let then = finish.clone();
            if ops.confirm_close(move || then()) {
                resident::reveal();
            } else {
                finish();
            }
        })
    };
    resident::set_hooks(resident::Hooks {
        quit: quit_app.clone(),
        run: {
            let ops = ops.clone();
            Rc::new(move |action| {
                windows::with_focused(|window| {
                    let (p, preview) = (panes::active(), preview::with_current(preview::Preview::clone));
                    if let Some(preview) = preview {
                        perform(action, window, &p.nav, &p.view, &preview, &ops);
                    }
                });
            })
        },
        pins: Rc::new(|| {
            let mut pins = Vec::new();
            sidebar::with_current(|s| pins = s.pinned_places().into_iter().map(|(label, _)| label).collect());
            pins
        }),
    });
    // ⌘Q, the Dock's Quit and signing out terminate the app without a close request.
    #[cfg(target_os = "macos")]
    gezik_platform::terminate::install(Box::new({
        let save_and_quit = save_and_quit.clone();
        move || save_and_quit()
    }));
    // A window's close button.
    let close_requested: Rc<dyn Fn() -> slint::CloseRequestResponse> = {
        let (ops, save_and_quit, finish) = (ops.clone(), save_and_quit.clone(), finish.clone());
        Rc::new(move || {
            // With the tray icon up the window only hides (spec 9.1); what it shows is saved.
            if resident::close_requested() {
                save_and_quit();
                return slint::CloseRequestResponse::KeepWindowShown;
            }
            let then = finish.clone();
            if ops.confirm_close(move || then()) {
                return slint::CloseRequestResponse::KeepWindowShown;
            }
            finish();
            slint::CloseRequestResponse::HideWindow
        })
    };
    let process = Rc::new(windows::Process {
        config: config.clone(),
        state: Rc::new(saved_state),
        memory,
        media,
        ops,
        files,
        secondary,
        close_requested,
        save: save_and_quit,
    });
    windows::set_process(process.clone());
    // The other windows of last time (deviation 5), behind the first.
    if !secondary && initial_settings.session.restore {
        windows::open_saved(first, &process.state, &plan.start);
    }
    let opening = windows::Opening {
        plan,
        right: process.state.right_session.clone(),
        dual: process.state.dual,
        active_pane: process.state.active_pane,
        sync: process.state.sync,
        offset: if secondary { 32 } else { 0 },
        state: process.state.clone(),
    };
    windows::open(first, opening);
    // 9b9: a Dock click shows the window hidden in the menu bar (deviation 8).
    #[cfg(target_os = "macos")]
    if !gezik_platform::reopen::install(Box::new(resident::reveal)) {
        eprintln!("gezik: the Dock icon does not show a hidden window (winit answers it)");
    }
    #[cfg(target_os = "macos")]
    gezik_platform::open_urls::install(Box::new(move |paths| {
        let targets = paths.into_iter().map(|path| cli::Target { path, select: false }).collect();
        single_instance::open_here(instance::Request { targets, ..Default::default() });
    }));
    // The window and tabs are up; calls that came before this wait in the channel.
    if let Some(listener) = listener {
        single_instance::serve(listener);
        if dbus {
            single_instance::start_file_manager1();
        }
    }
    if !secondary {
        // The F3 hint (spec 10 §10.2), off macOS and unless [shortcuts] names either action.
        let shortcuts = &initial_settings.shortcuts;
        let f3_hint = Platform::current() == Platform::Other
            && !shortcuts.is_written(Action::Search)
            && !shortcuts.is_written(Action::ToggleDualPane);
        let (f3_moved, f3_moved_at) = (process.state.f3_moved, process.state.f3_moved_at);
        // Decision 13: once, two seconds after start; with no journal, one stat and nothing more.
        // The hint then too: the first folder is shown by then, which would take a note away.
        slint::Timer::single_shot(std::time::Duration::from_secs(2), move || {
            windows::with_window(first, |_| dual::f3_hint_at_start(f3_hint, f3_moved, f3_moved_at));
            std::thread::spawn(|| {
                let (note, file_manager1) = system_changes::idle_check();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(note) = note {
                        windows::with_focused(|_| integration::offer_repair(note));
                    }
                    if file_manager1 {
                        single_instance::start_file_manager1();
                    }
                });
            });
        });
    }
    resident::start();
    // Deviation 13: hiding the last window must not end the event loop.
    if !hidden {
        windows::show_all()?;
    }
    // The register holds the window from here on.
    drop(window);
    let ran = slint::run_event_loop_until_quit();
    resident::shutdown();
    ran
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_warning_is_printed_once_however_often_the_config_is_applied() {
        let cli = Warning::new("command line", "unknown option --x (ignored)");
        let theme = Warning::new("nord.toml", "bad colour");
        // Printed by itself before the window opens.
        let mut printed = vec![cli.clone()];
        let now = [cli.clone(), theme.clone()];
        assert_eq!(not_printed_yet(&mut printed, &now), std::slice::from_ref(&theme));
        assert!(not_printed_yet(&mut printed, &now).is_empty(), "a re-resolve prints nothing again");
        assert!(not_printed_yet(&mut printed, &[]).is_empty());
        assert_eq!(not_printed_yet(&mut printed, &now), [cli, theme], "back after it was fixed: said again");
    }

    #[test]
    fn what_a_gezik_does_after_handing_over() {
        use instance::Sent::{Delivered, Failed, NoInstance};
        // --new-window or not, the answer decides alike: the flag only changes what the
        // running Gezik does with a delivered request.
        assert_eq!(after_send(Delivered, false), Next::Exit);
        assert_eq!(after_send(NoInstance, false), Next::First);
        assert_eq!(after_send(Failed, false), Next::OwnProcess);
        assert_eq!(after_send(Failed, true), Next::Explorer, "--shell");
        assert_eq!(after_send(Delivered, true), Next::Exit);
        assert_eq!(after_send(NoInstance, true), Next::First);
        assert!(own_process(true, false), "single instance off: a process of its own");
        assert!(!own_process(true, true), "none running: the first one");
        assert!(!own_process(false, false));
        for new_window in [false, true] {
            let request = cli::Cli { new_window, ..cli::Cli::default() }.request();
            assert_eq!(request.new_window, new_window);
        }
    }

    /// spec 10 §9.3: a running Gezik that does not answer (a fake listener holding the call)
    /// leaves the caller to open a process of its own after two seconds, with --new-window too.
    #[test]
    fn a_hung_gezik_leaves_the_caller_its_own_process() {
        let key = format!("t{}hung-main", std::process::id());
        let instance::Claim::Listening(listener) = instance::claim(&key) else { panic!("no channel") };
        listener.serve(|_| {
            std::thread::sleep(std::time::Duration::from_secs(4));
            true
        });
        let calls: Vec<_> = [false, true]
            .into_iter()
            .map(|new_window| {
                let key = key.clone();
                std::thread::spawn(move || {
                    let request = instance::Request { new_window, ..Default::default() };
                    let started = std::time::Instant::now();
                    let sent = instance::send(&key, &request, instance::SEND_TIMEOUT);
                    (sent, started.elapsed())
                })
            })
            .collect();
        for call in calls {
            let (sent, took) = call.join().unwrap();
            assert_eq!(sent, instance::Sent::Failed);
            assert!(took >= instance::SEND_TIMEOUT, "gave up after {took:?}");
            assert_eq!(after_send(sent, false), Next::OwnProcess);
        }
    }
}
