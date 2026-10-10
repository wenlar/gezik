// Hide the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actions;
mod admin;
mod archives;
mod batch_rename;
mod cli;
mod cloud;
mod conflicts;
mod connect;
mod context_menu;
mod convert;
mod copy_path;
mod dialog;
mod drag;
mod dual;
mod eject;
mod elevated;
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
mod stack;
mod start;
mod system_changes;
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

/// Resolves settings + theme from `files` and shows them. No I/O, so it runs on the UI
/// thread at startup, after config files change and when the system theme flips.
/// A click on the sidebar's saved search `index` (middle click: in a new tab).
fn run_saved_search(index: i32, new_tab: bool) {
    let names = saved_searches::names();
    if let Some(name) = usize::try_from(index).ok().and_then(|i| names.get(i)) {
        saved_searches::with_current(|s| s.run(name, new_tab));
    }
}

fn apply_config(window: &AppWindow, files: &ConfigFiles) -> Loaded {
    let loaded = store::resolve(files, window.get_system_dark());
    if let Some(theme) = &loaded.theme {
        theme_bridge::apply(window, theme);
    }
    theme_bridge::set_reduce_motion(window, loaded.settings.reduce_motion);
    let new = not_printed_yet(&mut PRINTED.lock().unwrap_or_else(std::sync::PoisonError::into_inner), &loaded.warnings);
    for warning in new {
        eprintln!("gezik: {warning}");
    }
    window.set_notice(notice_text(&loaded.warnings).into());
    for p in panes::all() {
        p.nav.set_session_restore(loaded.settings.session.restore);
    }
    dual::set_restore(loaded.settings.session.restore);
    dual::set_confirm(loaded.settings.panes_confirm);
    window.set_sidebar_position(match loaded.settings.sidebar {
        SidebarPosition::Left => 0,
        SidebarPosition::Right => 1,
        SidebarPosition::Hidden => 2,
    });
    // A reload may hide or move the sidebar (or change its pins): no tip stays behind.
    window.set_sidebar_tip("".into());
    // The shortcuts first: the pins' tips name their keys.
    keys::set_shortcuts(loaded.settings.shortcuts.clone());
    // Unchanged pins cost nothing (also after the reload that follows our own save).
    sidebar::with_current(|sidebar| sidebar.set_pinned(loaded.settings.pinned.clone()));
    sidebar::with_current(sidebar::Sidebar::relabel);
    // 9b9: the tray and the shortcut follow [system] (the tray menu follows the sidebar's pins).
    resident::apply(&loaded.settings.system);
    sidebar::with_current(|s| s.set_show_cloud(loaded.settings.sidebar_cloud));
    sidebar::with_current(|s| s.set_tree_follow(loaded.settings.sidebar_tree_follow));
    // The defaults first: a rule change then switches the view once.
    let rules = view::compile_rules(&loaded.settings.view_rules);
    for p in panes::all() {
        p.view.set_defaults(loaded.settings.view);
        p.view.set_rules(rules.clone(), || p.nav.rule_place());
    }
    view_options::set_from_file(loaded.settings.view.options);
    #[cfg(target_os = "macos")]
    menu_bar::set_commands(window, &loaded.settings.commands);
    frame_limit::set_max_fps(loaded.settings.max_fps);
    operations::with_current(|ops| ops.set_files(loaded.settings.files));
    batch_rename::set_presets(loaded.settings.rename_presets.clone());
    archives::set_settings(loaded.settings.tools.clone(), loaded.settings.archives);
    preview::set_quick_look(loaded.settings.system.quick_look);
    convert::set_settings(loaded.settings.convert.clone(), loaded.settings.commands.clone());
    filter::set_settings(loaded.settings.keyboard, loaded.settings.filters.clone());
    path_box::set_settings(loaded.settings.history);
    for p in panes::all() {
        p.search.set_settings(loaded.settings.search.clone());
        p.folder_sizes.set_settings(loaded.settings.folder_sizes, loaded.settings.search.everything);
    }
    tab_sets::set_settings(loaded.settings.tab_sets.clone());
    saved_searches::set_settings(loaded.settings.searches.clone());
    #[cfg(target_os = "macos")]
    menu_bar::set_tab_sets(window, &tab_sets::names());
    terminal::set_settings(loaded.settings.terminal.command.clone());
    loaded
}

/// Closes tab `index` once the current event is fully handled (the last tab closes the
/// window). The tab is remembered by id, so a close that runs after other tab changes, or
/// a second close of the same tab, never hits another tab.
fn close_tab_later(nav: &navigation::Navigator, index: usize) {
    if let Some(id) = nav.tab_id(index) {
        let nav = nav.clone();
        slint::Timer::single_shot(std::time::Duration::ZERO, move || nav.close_tab_by_id(id));
    }
}

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
        | Action::MoveTabToOtherPane => return actions::run(action, nav, view),
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
    // The Info window: Esc wherever its focus is, other keys to its fields.
    if window.get_info_open() {
        let mut used = false;
        if let Some(chord) = &chord {
            info::with_current(|info| used = info.chord(chord));
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
    window: &AppWindow,
    files: &mut ConfigFiles,
    cli: &[cli::Target],
    new_tab: bool,
    saved: Option<&Session>,
) -> (Settings, StartPlan) {
    let loaded = apply_config(window, files);
    let plan = resolve_start(&loaded.settings, cli, new_tab, false, saved);
    if !plan.warnings.is_empty() {
        files.warnings.extend(plan.warnings.iter().cloned());
        apply_config(window, files);
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
/// so retry shortly until it does, then move it on screen if needed.
fn keep_on_screen(window: slint::Weak<AppWindow>, attempt: u32) {
    slint::Timer::single_shot(std::time::Duration::from_millis(10), move || {
        let Some(strong) = window.upgrade() else { return };
        if !window_state::ensure_visible(&strong) && attempt < 200 {
            keep_on_screen(window, attempt + 1);
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
    // second call costs only the attempt. --new-window never hands over and never listens.
    let key = instance::key(gezik_config::paths::config_dir().as_deref());
    let request = cli.request();
    // A window that could not hand over is a second window as --new-window's is (spec 5.3):
    // two windows writing the same tabs to state.toml would lose one's.
    let mut secondary = cli.new_window;
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
        match instance::send(&key, &request, instance::SEND_TIMEOUT) {
            instance::Sent::Delivered => return Ok(()),
            instance::Sent::NoInstance => {}
            // A hung Gezik (spec 5.2): what the shell asked for still opens somewhere.
            instance::Sent::Failed if shell.is_some() => {
                std::process::exit(explorer_fallback(shell.as_deref().unwrap_or_default()))
            }
            // Hung, refusing or someone else's: a window of its own, the channel left alone.
            instance::Sent::Failed => secondary = true,
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

    let config = ConfigStore::system();
    templates::set_dir(config.as_ref().map(ConfigStore::templates_dir));
    view_options::install(&window, config.clone());
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
    let initial_settings = apply_config(&window, &files).settings;
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
    resident::begin(&window, config.clone(), resident_primary, !hidden, saved_state.tray_told);
    let plan = resolve_start(
        &initial_settings,
        &cli.targets,
        cli.new_tab,
        cli.trash,
        (!secondary).then_some(&saved_state.session),
    );
    if !plan.warnings.is_empty() {
        files.warnings.extend(plan.warnings.iter().cloned());
        apply_config(&window, &files);
    }

    // Folder views; a broken views.toml starts over and says so in the status bar.
    let (memory, views_warning) = config.as_ref().map(ConfigStore::load_views).unwrap_or_default();
    files.warnings.extend(views_warning);

    // The latest config files, so a system light/dark switch can re-resolve without I/O.
    let files = Arc::new(Mutex::new(files));

    window.on_system_scheme_changed({
        let weak = window.as_weak();
        let files = files.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                apply_config(&window, &files.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
            }
        }
    });

    // Reading happens on the watcher thread; resolving and applying on the UI thread.
    let files_for_warning = files.clone();
    let _watcher = config.as_ref().and_then(|store| {
        let reader = store.clone();
        let weak = window.as_weak();
        let files = files.clone();
        watcher::watch_config(
            store,
            move || {
                let fresh = reader.read_files();
                let files = files.clone();
                let _ = weak.upgrade_in_event_loop(move |window| {
                    let mut current = files.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                    *current = fresh;
                    let (_, plan) = apply_config_and_start(&window, &mut current, &[], false, None);
                    for p in panes::all() {
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
    apply_config(&window, &files.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
    window_state::restore(&window, &saved_state, if secondary { 32 } else { 0 });
    resident::when_shown({
        let weak = window.as_weak();
        move || keep_on_screen(weak, 0)
    });
    // The pane's parts carry its id; it is installed once they all exist.
    let pane_id = panes::next_id();
    window.set_panes(panes::model());
    // One media and one folder view memory for the process; each view holds a client of the
    // media and the same memory (spec 10 §3.3).
    let media = media::Media::new();
    media.install();
    let memory = Rc::new(std::cell::RefCell::new(memory));
    let view = view::View::new(pane_id, window.as_weak(), media.client(), memory, config.clone());
    view.set_defaults(initial_settings.view);
    // No folder shows yet: its place comes with the first one.
    view.set_rules(view::compile_rules(&initial_settings.view_rules), Default::default);
    view.set_options(view_options::current());
    view.set_columns(saved_state.columns.clone().unwrap_or_else(gezik_core::view::default_columns));
    view.set_result_columns(
        saved_state.result_columns.clone().unwrap_or_else(gezik_core::view::default_result_columns),
    );
    window.set_mono_font(
        if cfg!(windows) {
            "Consolas"
        } else if cfg!(target_os = "macos") {
            "Menlo"
        } else {
            "monospace"
        }
        .into(),
    );
    window.set_preview_width(saved_state.preview_width.unwrap_or(280) as f32);
    let folder_sizes = folder_sizes::FolderSizes::new(pane_id, window.as_weak(), view.clone());
    folder_sizes.set_settings(initial_settings.folder_sizes, initial_settings.search.everything);
    let preview = preview::Preview::new(&window);
    preview.set_pane_open(saved_state.preview_open);
    let StartPlan { session, select, start, .. } = plan;
    let nav = navigation::Navigator::new(pane_id, window.as_weak(), view.clone(), session, select, start);
    // The open tabs go to state.toml as they change (spec 5.1); its own thread writes them, so
    // a crash or a kill leaves the last tabs too.
    if !secondary && let Some(store) = config.clone() {
        nav.keep_session(saved_state.session.clone(), initial_settings.session.restore, move |session| {
            let session = session.clone();
            store.update_state(move |state| state.session = session);
        });
    }
    let path_box =
        path_box::PathBox::new(pane_id, window.as_weak(), nav.clone(), config.clone(), saved_state.history.clone());
    // Captures no navigator (it is not `Send`): the result finds the panes on the UI thread.
    places::load_in_background(window.as_weak(), |part| {
        for p in panes::all() {
            p.nav.set_places(part.clone());
        }
    });
    // The templates of New ▸, read once the window is up.
    slint::Timer::single_shot(std::time::Duration::from_millis(500), templates::load_in_background);

    let dialogs = dialog::Dialogs::new(&window);
    let filter = filter::Filter::new(pane_id, window.as_weak(), view.clone(), dialogs.clone(), config.clone());
    let searches = search::Searches::new(pane_id, window.as_weak(), nav.clone(), view.clone(), dialogs.clone());
    searches.set_settings(initial_settings.search.clone());
    // Every pane's bars and address go through the same window callbacks.
    path_box::PathBox::connect(&window);
    filter::Filter::connect(&window);
    search::Searches::connect(&window);
    let left = panes::Pane {
        id: pane_id,
        nav: nav.clone(),
        view: view.clone(),
        filter,
        search: searches,
        path_box,
        folder_sizes,
    };
    dual::connect(&left);
    // Installed before the window's parts are made: they reach the active pane.
    panes::install(left);
    let restore = initial_settings.session.restore && !secondary;
    let state_store = config.clone().filter(|_| !secondary);
    let right_session = if restore { saved_state.right_session.clone() } else { Default::default() };
    let stores = (state_store, config.clone());
    dual::install(&window, stores, initial_settings.session.restore, right_session, saved_state.pane_split);
    // The first tab loads once the pane is installed: its listing comes back through it.
    nav.install();
    let sidebar = sidebar::Sidebar::new(&window, config.clone(), dialogs.clone());
    sidebar.install();
    sidebar.set_pinned(initial_settings.pinned);
    sidebar.set_show_cloud(initial_settings.sidebar_cloud);
    sidebar.set_tree_follow(initial_settings.sidebar_tree_follow);
    let _tab_sets = tab_sets::TabSets::new(&window, dialogs.clone(), config.clone());
    let _saved_searches = saved_searches::SavedSearches::new(&window, dialogs.clone(), config.clone());
    let _tab_tools = tab_tools::TabTools::new(&window);
    let _integration = integration::Integration::new(&window);
    let _select_tools =
        select_tools::SelectTools::new(dialogs.clone(), config.clone(), saved_state.selection.last_pattern.clone());
    let engine_settings = gezik_ops::Settings {
        threads: initial_settings.files.copy_threads,
        pending_deletes: config.as_ref().map(|store| store.dir().join("pending-deletes")),
    };
    let ops = operations::Operations::new(
        &window,
        sidebar.clone(),
        dialogs.clone(),
        engine_settings,
        initial_settings.files,
        saved_state.operations_collapsed,
        config.clone(),
        saved_state.batch_rename.clone().unwrap_or_default(),
    );
    connect::install(config.clone(), saved_state.servers_recent.clone());
    let _palette = palette::Palette::new(&window, config.clone(), saved_state.palette_recent.clone(), {
        let (weak, preview, ops) = (window.as_weak(), preview.clone(), ops.clone());
        move |action| {
            if let Some(window) = weak.upgrade() {
                let p = panes::active();
                perform(action, &window, &p.nav, &p.view, &preview, &ops);
            }
        }
    });
    let _batch_rename = batch_rename::BatchRename::new(&window, ops.clone());
    let _stack = stack::Stack::new(&window, ops.clone());
    #[cfg(target_os = "macos")]
    menu_bar::install(&window, ops.clone());
    // Tools Gezik downloads (7-Zip) go to `<config dir>/tools/`, next to the pending deletes.
    let _archives =
        archives::Archives::new(&window, ops.clone(), dialogs.clone(), config.clone(), saved_state.archive.clone());
    let _info = info::Info::new(&window, ops.clone(), dialogs.clone());
    let _convert =
        convert::Convert::new(&window, ops.clone(), dialogs.clone(), config.clone(), saved_state.convert.clone());
    window.on_op_pause({
        let ops = ops.clone();
        move |id| ops.pause(id)
    });
    window.on_op_resume({
        let ops = ops.clone();
        move |id| ops.resume(id)
    });
    window.on_op_cancel({
        let ops = ops.clone();
        move |id| ops.cancel(id)
    });
    window.on_op_start_now({
        let ops = ops.clone();
        move |id| ops.start_now(id)
    });
    window.on_op_details({
        let ops = ops.clone();
        move |id| ops.details(id)
    });
    window.on_op_retry({
        let ops = ops.clone();
        move |id| ops.retry(id)
    });
    window.on_op_dismiss({
        let ops = ops.clone();
        move |id| ops.dismiss(id)
    });
    window.on_ops_toggle({
        let ops = ops.clone();
        move || ops.toggle_collapsed()
    });
    window.on_ops_tab_chosen({
        let ops = ops.clone();
        move |tab| ops.choose_tab(tab)
    });
    window.on_history_show({
        let ops = ops.clone();
        move |id| ops.history_show(id)
    });
    window.on_history_details({
        let ops = ops.clone();
        move |id| ops.history_details(id)
    });
    window.on_history_toggle({
        let ops = ops.clone();
        move || ops.history_toggle()
    });
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
    let save_and_quit: Rc<dyn Fn()> = {
        let (weak, store, preview, ops) = (window.as_weak(), config.clone(), preview.clone(), ops.clone());
        Rc::new(move || {
            let view = panes::active_view();
            if let (Some(window), Some(store)) = (weak.upgrade(), &store) {
                store.update_state(|state| {
                    window_state::capture_into(&window, state);
                    state.columns = Some(view.columns());
                    state.result_columns = Some(view.result_columns());
                    state.preview_open = preview.is_pane_open();
                    state.preview_width = Some(window.get_preview_width().round().clamp(200.0, 600.0) as u32);
                    state.operations_collapsed = ops.collapsed();
                });
                dual::save();
                // Quitting: what the other parts changed lately is written too (a saved
                // filter, a pin or a default just sent to settings.toml as well).
                store.flush_state();
                store.flush_settings();
                // A view change still waiting for its timer is handed over, then written.
                view.flush_memory();
                store.flush_views();
            }
            // Its window would otherwise keep the event loop (and the process) running.
            preview.close_quick_look();
        })
    };
    // Saves and ends the event loop (run_event_loop_until_quit: deviation 13).
    let finish: Rc<dyn Fn()> = {
        let (save_and_quit, weak) = (save_and_quit.clone(), window.as_weak());
        Rc::new(move || {
            save_and_quit();
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
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
            let (weak, preview, ops) = (window.as_weak(), preview.clone(), ops.clone());
            Rc::new(move |action| {
                if let Some(window) = weak.upgrade() {
                    let p = panes::active();
                    perform(action, &window, &p.nav, &p.view, &preview, &ops);
                }
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
    #[cfg(windows)]
    if !secondary {
        resident::when_shown({
            let (weak, save) = (window.as_weak(), save_and_quit.clone());
            move || save_on_session_end(weak, save, 0)
        });
    }
    window.window().on_close_requested({
        let (ops, save_and_quit, finish) = (ops.clone(), save_and_quit.clone(), finish.clone());
        move || {
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
        }
    });
    // The sidebar is the window's: it leads the active pane (spec 10 §4.10).
    window.on_sidebar_clicked({
        let sidebar = sidebar.clone();
        move |section, index| {
            if section == sidebar::SECTION_SEARCHES {
                return run_saved_search(index, false);
            }
            if let Some(location) = sidebar.location_of(section, index) {
                panes::active_nav().go(location);
            }
        }
    });
    window.on_sidebar_middle_clicked({
        let sidebar = sidebar.clone();
        move |section, index| {
            if section == sidebar::SECTION_SEARCHES {
                return run_saved_search(index, true);
            }
            if let Some(location) = sidebar.location_of(section, index) {
                panes::active_nav().open_tab(location, false);
            }
        }
    });
    window.on_pinned_drop({
        let sidebar = sidebar.clone();
        move |from, line| {
            if let (Ok(from), Ok(line)) = (usize::try_from(from), usize::try_from(line)) {
                sidebar.drop_pinned(from, line);
            }
        }
    });
    window.on_sidebar_toggled({
        let sidebar = sidebar.clone();
        move |row| {
            if let Ok(row) = usize::try_from(row) {
                sidebar.toggle_row(row);
            }
        }
    });
    // The sidebar width stays in memory and is saved with the window state on close.

    // Gezik's own menus (where Slint has no system ones) stay inside the window.
    window.set_native_menus(context_menu::native_menus());
    window.on_place_menu(|x, y, flip_x, flip_y, width, height, area_width, area_height| {
        let anchor = popup::Anchor { x, y, flip_x, flip_y };
        let placed = popup::place_menu(anchor, (width, height), (area_width, area_height));
        MenuPlace { x: placed.x, y: placed.y, width: placed.width, height: placed.height }
    });
    window.on_menu_step(|lines, from, down| {
        use slint::Model;
        let enabled: Vec<bool> = lines.iter().map(|line| line.enabled).collect();
        popup::step_line(&enabled, from, down)
    });
    let menus = context_menu::Menus::new(&window, preview.clone(), sidebar.clone(), ops.clone());
    let drags = drag::Drags::new(&window, sidebar, ops.clone(), menus.clone());
    drags.install(&window);
    resident::when_shown({
        let drags = drags.clone();
        move || drags.attach_when_ready(0)
    });
    // A pane's callbacks name it by its row: a click or an action there makes it the active
    // one first (`pick`); what it only tells goes to it as it is.
    window.on_row_menu({
        let menus = menus.clone();
        move |pane, i, x, y| {
            let Some(p) = pick(pane) else { return };
            if let Ok(index) = usize::try_from(i) {
                p.view.prepare_menu(index);
            }
            menus.row(i, x, y)
        }
    });
    // Right-click on empty space clears the selection, as in Explorer. The Windows menu
    // opens at the cursor.
    window.on_background_menu({
        let menus = menus.clone();
        move |pane, x, y| {
            let Some(p) = pick(pane) else { return };
            p.view.clear_selection();
            menus.background(x, y)
        }
    });
    window.on_item_pressed({
        let ops = ops.clone();
        move |pane, i, ctrl, shift| {
            ops.end_unfocused_rename();
            let Some(p) = pick(pane) else { return };
            if let Ok(index) = usize::try_from(i) {
                p.view.press(index, ctrl, shift);
            }
        }
    });
    window.on_marquee(move |pane, x, y, width, height, additive| {
        if let Some(p) = pick(pane) {
            p.view.marquee(gezik_core::layout::Rect { x, y, width, height }, additive);
        }
    });
    window.on_marquee_done(|pane| {
        panes::with_row(pane, |p| p.view.marquee_done());
    });
    window.on_background_pressed({
        let ops = ops.clone();
        move |pane, ctrl| {
            ops.end_unfocused_rename();
            let Some(p) = pick(pane) else { return };
            if !ctrl {
                p.view.clear_selection();
            }
        }
    });
    // Opened once the pane's request is done with (it asks from inside Slint's change
    // handlers, where a system menu's own loop must not start).
    window.on_keyboard_menu({
        let menus = menus.clone();
        move |_pane, i, x, y| {
            let menus = menus.clone();
            slint::Timer::single_shot(std::time::Duration::ZERO, move || menus.keyboard(i, x, y));
        }
    });
    window.on_sidebar_menu({
        let menus = menus.clone();
        move |section, i, x, y| {
            // With two panes the sidebar is outside them: typing an address ends here.
            let id = panes::active().id;
            if panes::mirror(id).path_editing.get() {
                panes::edit(id, |d| panes::path_editing(d, false));
            }
            menus.sidebar_entry(section, i, x, y)
        }
    });
    window.on_header_clicked(|pane, column| {
        if let Some(p) = pick(pane) {
            p.view.header_clicked(column);
        }
    });
    window.on_preview_resized({
        let preview = preview.clone();
        move || preview.schedule()
    });
    // The columns are every pane's (spec 10 §3.3).
    window.on_columns_resized(|_pane| {
        for p in panes::all() {
            p.view.columns_resized();
        }
    });
    window.on_header_menu({
        let menus = menus.clone();
        move |pane, x, y| {
            if pick(pane).is_some() {
                menus.header(x, y);
            }
        }
    });
    window.on_list_line_of(|pane, i| {
        panes::with_row(pane, |p| p.view.place_of(usize::try_from(i).unwrap_or(usize::MAX)).0).unwrap_or(0)
    });
    window.on_list_column_of(|pane, i| {
        panes::with_row(pane, |p| p.view.place_of(usize::try_from(i).unwrap_or(usize::MAX)).1).unwrap_or(0)
    });
    window.on_list_group_toggled(|pane, first| {
        if let Some(p) = pick(pane) {
            p.view.toggle_group(usize::try_from(first).unwrap_or(usize::MAX));
        }
    });
    window.on_list_group_menu({
        let menus = menus.clone();
        move |pane, x, y| {
            if pick(pane).is_some() {
                menus.group_header(x, y);
            }
        }
    });
    window.on_filter_menu({
        let menus = menus.clone();
        move |pane, left, bottom, right, top| {
            if pick(pane).is_some() {
                menus.filter_menu(popup::Anchor::below(left, top, right, bottom));
            }
        }
    });
    window.on_search_menu({
        let menus = menus.clone();
        move |pane, which, left, bottom, right, top| {
            if pick(pane).is_some() {
                let which = search::SearchMenu::from_index(which);
                menus.search_menu(which, popup::Anchor::below(left, top, right, bottom));
            }
        }
    });
    window.on_view_menu({
        let menus = menus.clone();
        move |pane, left, bottom, right, top| {
            if pick(pane).is_some() {
                menus.view_menu(popup::Anchor::below(left, top, right, bottom));
            }
        }
    });
    window.on_grid_columns_changed(|pane, columns| {
        panes::with_row(pane, |p| p.view.grid_columns_changed(usize::try_from(columns).unwrap_or(1)));
    });
    // What the pane keeps and only tells (spec 10 §3.2): Rust's mirror of it.
    window.on_scrolled(|pane, y| panes::mirror_at(pane).scroll.set(y));
    window.on_pane_focus(|pane, focus| {
        // A field of the pane took the keyboard: an action in that pane.
        if focus.filter || focus.search || focus.rename {
            pick(pane);
        }
        *panes::mirror_at(pane).focus.borrow_mut() = focus;
    });
    window.on_pane_geometry(|pane, geometry| {
        let columns = usize::try_from(geometry.grid_columns).unwrap_or(1);
        *panes::mirror_at(pane).geometry.borrow_mut() = geometry;
        // A pane made after the first layout never sees its grid's columns change.
        panes::with_row(pane, |p| p.view.grid_columns_changed(columns));
    });
    window.on_revealed(|pane, index, target| {
        panes::with_row(pane, |p| p.view.revealed(index, target));
    });
    window.on_zoom(|pane, bigger| {
        if let Some(p) = pick(pane) {
            p.view.zoom(bigger);
        }
    });
    window.on_pane_split_moved(dual::split_moved);
    window.on_pane_split_done(dual::split_done);
    window.on_pane_split_reset(dual::split_reset);
    window.on_panes_cramped(dual::cramped);
    window.on_conflict_row_menu({
        let menus = menus.clone();
        move |row, x, y| menus.conflict(row, x, y)
    });
    window.on_rb_add_rule({
        let menus = menus.clone();
        move |left, bottom, right, top| menus.add_rule(popup::Anchor::below(left, top, right, bottom))
    });
    window.on_rb_presets({
        let menus = menus.clone();
        move |left, bottom, right, top| menus.presets(popup::Anchor::below(left, top, right, bottom))
    });
    // The Convert layer's menus: its presets, the text encodings.
    window.on_cv_presets({
        let menus = menus.clone();
        move |left, bottom, right, top| {
            let mut items = Vec::new();
            convert::with_current(|convert| items = convert.preset_menu());
            menus.convert_menu(items, popup::Anchor::below(left, top, right, bottom));
        }
    });
    window.on_cv_from({
        let menus = menus.clone();
        move |left, bottom, right, top| {
            let mut items = Vec::new();
            convert::with_current(|convert| items = convert.encoding_menu(true));
            menus.convert_menu(items, popup::Anchor::below(left, top, right, bottom));
        }
    });
    window.on_cv_to({
        let menus = menus.clone();
        move |left, bottom, right, top| {
            let mut items = Vec::new();
            convert::with_current(|convert| items = convert.encoding_menu(false));
            menus.convert_menu(items, popup::Anchor::below(left, top, right, bottom));
        }
    });
    window.on_info_app_menu({
        let menus = menus.clone();
        move |left, bottom, right, top| {
            let mut items = Vec::new();
            info::with_current(|info| items = info.app_menu());
            menus.info_menu(items, popup::Anchor::below(left, top, right, bottom));
        }
    });
    window.on_info_group_menu({
        let menus = menus.clone();
        move |left, bottom, right, top| {
            let mut items = Vec::new();
            info::with_current(|info| items = info.group_menu());
            menus.info_menu(items, popup::Anchor::below(left, top, right, bottom));
        }
    });
    // Slint passes indexes as `i32`: a negative one does nothing.
    window.on_tab_menu(move |pane, i, x, y| {
        if let (Some(_), Ok(i)) = (pick(pane), usize::try_from(i)) {
            menus.tab(i, x, y);
        }
    });

    // Double-click; with single-click-open the click already opened it.
    window.on_open_row(|pane, i| {
        if let Some(p) = pick(pane)
            && let Ok(index) = usize::try_from(i)
            && !view_options::current().single_click_open
        {
            open_entry(&p.nav, &p.view, index);
        }
    });
    window.on_go_back(|pane| {
        if let Some(p) = pick(pane) {
            p.nav.back();
        }
    });
    window.on_go_forward(|pane| {
        if let Some(p) = pick(pane) {
            p.nav.forward();
        }
    });
    window.on_go_up(|pane| {
        if let Some(p) = pick(pane) {
            p.nav.up();
        }
    });
    window.on_refresh(|pane| {
        if let Some(p) = pick(pane) {
            p.nav.reload();
        }
    });
    window.on_navigate(|pane, text| {
        if let Some(p) = pick(pane) {
            p.nav.navigate_text(text.into());
        }
    });
    window.on_crumb_clicked(|pane, i| {
        if let Some(p) = pick(pane) {
            p.nav.crumb_clicked(i);
        }
    });
    // Slint passes indexes as `i32`: a negative one does nothing.
    window.on_tab_activate(|pane, i| {
        if let (Some(p), Ok(i)) = (pick(pane), usize::try_from(i)) {
            p.nav.activate_tab(i);
        }
    });
    // Closed once the click is fully handled.
    window.on_tab_close(|pane, i| {
        if let (Some(p), Ok(i)) = (pick(pane), usize::try_from(i)) {
            close_tab_later(&p.nav, i);
        }
    });
    window.on_tab_new(|pane| {
        if let Some(p) = pick(pane) {
            p.nav.open_tab(p.nav.start(), true);
        }
    });
    window.on_tab_move(|pane, from, to| {
        if let (Some(p), Ok(from), Ok(to)) = (pick(pane), usize::try_from(from), usize::try_from(to)) {
            p.nav.move_tab(from, to);
        }
    });
    window.on_row_middle_clicked(|pane, i| {
        let Some(p) = pick(pane) else { return };
        // A folder in the trash is a bin entry: not opened (spec 7.1).
        if let Some((path, true)) =
            p.nav.entry_path(i).filter(|_| p.nav.active_location() != gezik_core::nav::Location::Trash)
        {
            p.nav.open_tab(gezik_core::nav::Location::Path(path), false);
        }
    });

    window.on_rename_accepted({
        let ops = ops.clone();
        move |_pane, text| ops.rename_accepted(text.into())
    });
    window.on_rename_cancelled({
        let ops = ops.clone();
        move |_pane| ops.rename_cancelled()
    });
    window.on_rename_tab({
        let ops = ops.clone();
        move |_pane, text, back| ops.rename_tab(text.into(), back)
    });
    window.on_rename_blurred({
        let ops = ops.clone();
        move |pane, text, generation| {
            // A rename of a pane left is ended already (`dual::activate`).
            if usize::try_from(pane).is_ok_and(|pane| pane == panes::active_index()) {
                ops.rename_blurred(text.into(), generation)
            }
        }
    });
    window.on_rename_edited({
        let ops = ops.clone();
        move |pane, text| {
            *panes::mirror_at(pane).rename_text.borrow_mut() = text.clone();
            ops.rename_edited(&text)
        }
    });

    window.on_key_event({
        let (preview, ops, weak) = (preview.clone(), ops.clone(), window.as_weak());
        let drags = drags.clone();
        let mut type_ahead = keys::TypeAhead::new();
        // The pane typed into last: type-ahead starts over in another one.
        let mut typed_in = None;
        move |event| {
            let Some(window) = weak.upgrade() else { return false };
            let pane = panes::active();
            if typed_in.replace(pane.id) != Some(pane.id) {
                type_ahead = keys::TypeAhead::new();
            }
            let m = event.modifiers;
            // Slint's `control` is ⌘ on macOS.
            let press = keys::take_pressed();
            // AltGr on a key it types nothing with is Ctrl+Alt (keys.rs `altgr_blank`).
            let (control, alt) = press.modifiers(m.control, m.alt);
            let text = keys::shortcut_text(&event.text, control);
            let chord =
                keys::chord_from_press(&text, press.physical, control, alt, m.shift, m.meta, Platform::current());
            let menu_key = keys::is_context_menu_key(&event.text, control, alt, m.shift, m.meta);
            // Esc while dragging files drops nothing.
            if chord.as_ref().is_some_and(|c| c.key == Key::Escape) && drags.escape() {
                return true;
            }
            let used = handle_key(
                &window,
                &pane.nav,
                &pane.view,
                &preview,
                &ops,
                &mut type_ahead,
                &event.text,
                chord,
                control || alt || m.meta,
                menu_key,
            );
            press.swallowed(used)
        }
    });

    // Mouse back/forward side buttons, anywhere in the window. Slint passes them on to
    // the items too, which ignore them.
    {
        use slint::winit_030::{EventResult, WinitWindowAccessor, winit};
        let weak = window.as_weak();
        let minimized = std::cell::Cell::new(false);
        // The pointer's last window position (logical pixels).
        let pointer = std::cell::Cell::new((0.0f32, 0.0f32));
        let ops = ops.clone();
        let drags = drags.clone();
        window.window().on_winit_window_event(move |_, event| {
            // The keypad's keys and Ctrl+Shift+digits, which Slint's text cannot tell apart
            // (keys.rs `Physical`), and AltGr on a key it types nothing with (keys.rs
            // `altgr_blank`): noted before Slint hands the key to `key-event`.
            if let winit::event::WindowEvent::KeyboardInput { event, .. } = event {
                keys::note_key(event);
            }
            if let winit::event::WindowEvent::Focused(false) = event {
                keys::forget_altgr();
            }
            if let winit::event::WindowEvent::Focused(true) = event {
                ops.clipboard_check();
            }
            if let winit::event::WindowEvent::DroppedFile(path) = event {
                drags.dropped_file(path.clone());
            }
            // Windows drops a minimized window's picture, but the size on restore is the old
            // one, so Slint redraws only what changed and the rest of the window stays empty.
            if let winit::event::WindowEvent::Resized(size) = event {
                let zero = size.width == 0 || size.height == 0;
                if !zero && minimized.get() {
                    let weak = weak.clone();
                    slint::Timer::single_shot(std::time::Duration::ZERO, move || {
                        if let Some(window) = weak.upgrade() {
                            theme_bridge::repaint_all(&window);
                        }
                    });
                }
                minimized.set(zero);
            }
            // Caps the frame rate at `max-fps`: Slint draws as often as the display refreshes.
            if let winit::event::WindowEvent::RedrawRequested = event {
                frame_limit::wait_for_frame();
                return EventResult::Propagate;
            }
            // A drag that lost its pointer grab to a tab switch follows the window's events.
            if let winit::event::WindowEvent::CursorMoved { position, .. } = event {
                let scale = weak.upgrade().map_or(1.0, |w| w.window().scale_factor());
                let at = position.to_logical::<f32>(f64::from(scale));
                drags.window_pointer_moved(at.x, at.y);
                pointer.set((at.x, at.y));
            }
            if let winit::event::WindowEvent::MouseInput {
                state: winit::event::ElementState::Released,
                button: button @ (winit::event::MouseButton::Left | winit::event::MouseButton::Right),
                ..
            } = event
            {
                let (x, y) = pointer.get();
                // A left release goes on to Slint (its grab points at an entry that is gone);
                // a right one would open a second menu after the drop's (see drag.rs).
                if drags.window_released(x, y, *button == winit::event::MouseButton::Right) {
                    return EventResult::PreventDefault;
                }
            }
            if let winit::event::WindowEvent::MouseInput {
                state: winit::event::ElementState::Pressed, button, ..
            } = event
            {
                match button {
                    winit::event::MouseButton::Back => panes::active_nav().back(),
                    winit::event::MouseButton::Forward => panes::active_nav().forward(),
                    _ => {}
                }
            }
            EventResult::Propagate
        });
    }

    // The second pane of last time, with its own tabs (its listing comes in the background).
    if restore && saved_state.dual {
        dual::open(Some(saved_state.active_pane));
    }
    // The window and tabs are up; calls that came before this wait in the channel.
    single_instance::set_window(window.as_weak());
    // 9b9: a Dock click shows the window hidden in the menu bar (deviation 8).
    #[cfg(target_os = "macos")]
    if !gezik_platform::reopen::install(Box::new(resident::reveal)) {
        eprintln!("gezik: the Dock icon does not show a hidden window (winit answers it)");
    }
    #[cfg(target_os = "macos")]
    {
        let weak = window.as_weak();
        gezik_platform::open_urls::install(Box::new(move |paths| {
            let targets = paths.into_iter().map(|path| cli::Target { path, select: false }).collect();
            single_instance::open_here(weak.clone(), instance::Request { targets, ..Default::default() });
        }));
    }
    if let Some(listener) = listener {
        single_instance::serve(listener, window.as_weak());
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
        let (f3_moved, f3_moved_at) = (saved_state.f3_moved, saved_state.f3_moved_at);
        // Decision 13: once, two seconds after start; with no journal, one stat and nothing more.
        // The hint then too: the first folder is shown by then, which would take a note away.
        slint::Timer::single_shot(std::time::Duration::from_secs(2), move || {
            dual::f3_hint_at_start(f3_hint, f3_moved, f3_moved_at);
            std::thread::spawn(|| {
                let (note, file_manager1) = system_changes::idle_check();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(note) = note {
                        integration::offer_repair(note);
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
        window.show()?;
    }
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
}
