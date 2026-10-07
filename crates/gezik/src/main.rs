// Hide the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actions;
mod archives;
mod batch_rename;
mod conflicts;
mod context_menu;
mod convert;
mod copy_path;
mod dialog;
mod drag;
mod filter;
mod folder_watch;
mod frame_limit;
mod keys;
mod media;
#[cfg(target_os = "macos")]
mod menu_bar;
mod navigation;
mod operations;
mod path_box;
mod pdf;
mod places;
mod popup;
mod preview;
mod quick_look;
mod select_tools;
mod sidebar;
mod start;
mod tab_sets;
mod tab_tools;
mod terminal;
mod theme_bridge;
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
use start::StartPlan;

slint::include_modules!();

/// Resolves settings + theme from `files` and shows them. No I/O, so it runs on the UI
/// thread at startup, after config files change and when the system theme flips.
/// Opens entry `index` of the list (a double-click, or a click with single-click-open): an
/// archive is extracted next to itself if `[archives] double-click` says so.
fn open_entry(nav: &navigation::Navigator, view: &view::View, index: usize) {
    let archive = view.entry_path(index).filter(|(path, is_dir)| {
        !is_dir
            && path.file_name().is_some_and(|n| gezik_core::batch::archive::looks_like_archive(&n.to_string_lossy()))
    });
    match archive {
        Some((path, _)) if archives::extracts_on_double_click() => {
            archives::with_current(|archives| archives.extract_here(vec![path]));
        }
        _ => nav.open_row(i32::try_from(index).unwrap_or(-1)),
    }
}

fn apply_config(window: &AppWindow, files: &ConfigFiles) -> Loaded {
    let loaded = store::resolve(files, window.get_system_dark());
    if let Some(theme) = &loaded.theme {
        theme_bridge::apply(window, theme);
    }
    for warning in &loaded.warnings {
        eprintln!("gezik: {warning}");
    }
    window.set_notice(notice_text(&loaded.warnings).into());
    navigation::with_current(|nav| nav.set_session_restore(loaded.settings.session.restore));
    window.set_sidebar_position(match loaded.settings.sidebar {
        SidebarPosition::Left => 0,
        SidebarPosition::Right => 1,
        SidebarPosition::Hidden => 2,
    });
    // Unchanged pins cost nothing (also after the reload that follows our own save).
    sidebar::with_current(|sidebar| sidebar.set_pinned(loaded.settings.pinned.clone()));
    view::with_current(|view| view.set_defaults(loaded.settings.view));
    view_options::set_from_file(loaded.settings.view.options);
    keys::set_shortcuts(loaded.settings.shortcuts.clone());
    #[cfg(target_os = "macos")]
    menu_bar::set_commands(window, &loaded.settings.commands);
    frame_limit::set_max_fps(loaded.settings.max_fps);
    operations::with_current(|ops| ops.set_files(loaded.settings.files));
    batch_rename::set_presets(loaded.settings.rename_presets.clone());
    archives::set_settings(loaded.settings.tools.clone(), loaded.settings.archives);
    convert::set_settings(loaded.settings.convert.clone(), loaded.settings.commands.clone());
    filter::set_settings(loaded.settings.keyboard, loaded.settings.filters.clone());
    path_box::set_settings(loaded.settings.history);
    tab_sets::set_settings(loaded.settings.tab_sets.clone());
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
    // The tab picker: Esc, Enter, Up and Down are its own, other keys go to its field.
    if window.get_tp_open() {
        let mut used = false;
        if let Some(chord) = &chord {
            tab_tools::with_current(|t| used = t.chord(chord));
        }
        return used;
    }
    // The name field being edited has the keyboard (Enter, Esc, Tab are its own).
    if ops.end_unfocused_rename() {
        return false;
    }
    let editing = window.get_path_editing();

    if editing && let Some(chord) = &chord {
        // The suggestion list's keys first: ↓ ↑ Tab → Enter Esc (spec 6.1).
        if !has_modifier {
            let mut used = false;
            path_box::with_current(|p| used = p.chord(chord));
            if used {
                return true;
            }
        }
        if chord.key == Key::Escape && !has_modifier {
            window.invoke_focus_list();
            window.set_path_editing(false);
            return true;
        }
        if !has_modifier || keys::is_text_edit(chord, Platform::current()) {
            return false;
        }
    }

    // The filter bar's field: Esc closes the filter, Down or Enter give the list the keyboard
    // (the bar stays); other plain keys and the text editing shortcuts are the field's.
    let filtering = window.get_filter_focused();
    if filtering && let Some(chord) = &chord {
        if !has_modifier && !chord.shift {
            match chord.key {
                Key::Escape => {
                    filter::with_current(filter::Filter::close);
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

    if let Some(action) = chord.as_ref().and_then(keys::action_for) {
        // Space opens quick look only on the focused list and outside type-ahead; elsewhere
        // it is an ordinary key.
        let ordinary_key = action == Action::QuickLook
            && (!window.get_list_focused() || type_ahead.is_active(std::time::Instant::now()));
        if !ordinary_key {
            if keys::waits_for_text_fields(action)
                && (editing || filtering || (keys::needs_list(action) && !window.get_list_focused()))
            {
                return false;
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
                    path_box::with_current(path_box::PathBox::reset);
                    window.invoke_edit_path()
                }
                Action::Refresh => nav.reload(),
                Action::SelectAll => view.select_all(),
                Action::ViewList => view.set_mode(gezik_core::view::ViewMode::List),
                Action::ViewGrid => view.set_mode(gezik_core::view::ViewMode::Grid),
                Action::TogglePreview => preview.toggle_pane(),
                Action::QuickLook => preview.toggle_quick_look(),
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
                | Action::Pin9 => {
                    if action == Action::Filter && editing {
                        window.set_path_editing(false);
                    }
                    if !actions::run(action, nav, view) {
                        return false;
                    }
                }
            }
            // The typed text no longer fits once the location or tab changed.
            if (editing || filtering) && !matches!(action, Action::FocusPath | Action::Filter) {
                window.invoke_focus_list();
            }
            return true;
        }
    }
    // A `[[commands]]` key: on the selection, only while the file list has the keyboard.
    if let Some(index) = chord.as_ref().and_then(keys::command_for) {
        if editing || filtering || !window.get_list_focused() {
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
        window.invoke_open_keyboard_menu();
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
                let page = usize::try_from(window.get_list_page_rows()).unwrap_or(1).max(1);
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
                // The first Esc closes the filter, the next clears the selection.
                Key::Escape if !primary && !chord.shift => {
                    if view.filter_text().is_some() {
                        filter::with_current(filter::Filter::close);
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
        filter::with_current(filter::Filter::open);
        return true;
    }
    // A space is no letter: it stays quick look's (or type-ahead's, as before).
    if filter::typing() == Typing::Filter && c != ' ' && !view.shows_drives() {
        filter::with_current(|f| f.typed(c));
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
    cli: Option<PathBuf>,
    saved: Option<&Session>,
) -> (Settings, StartPlan) {
    let loaded = apply_config(window, files);
    let plan = resolve_start(&loaded.settings, cli, saved);
    if !plan.warnings.is_empty() {
        files.warnings.extend(plan.warnings.iter().cloned());
        apply_config(window, files);
    }
    (loaded.settings, plan)
}

/// [`start::plan_start`] against the real file system.
fn resolve_start(settings: &Settings, cli: Option<PathBuf>, saved: Option<&Session>) -> StartPlan {
    let saved = saved.filter(|_| settings.session.restore);
    // Absolute, so the address bar parts and "up" work for `gezik .` too.
    let cli = cli.map(|path| std::path::absolute(&path).unwrap_or(path));
    let dirs = gezik_config::paths::KnownDirs::system();
    start::plan_start(
        &settings.start_folder,
        cli,
        &path_box::home(),
        |text| dirs.expand_checked(text),
        start::path_kind,
        saved,
    )
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

fn main() -> Result<(), slint::PlatformError> {
    // The PDF worker (`gezik --pdf-worker`) is this exe run by Gezik itself: it does one PDF
    // request with pdfium and exits, before any window, Slint or settings. Its pipes come from
    // the handles `ChildProcess` gives it, so this works in the windowless release build too.
    if std::env::args_os().nth(1).is_some_and(|arg| arg == gezik_batch::pdf::client::WORKER_ARG) {
        std::process::exit(gezik_batch::pdf::worker::main());
    }
    // Gezik has its own tabs: no window tabs of macOS (nor their items in the View menu).
    #[cfg(target_os = "macos")]
    gezik_platform::app::no_window_tabs();
    let window = AppWindow::new()?;

    let config = ConfigStore::system();
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
    }
    // Something sensible is on screen even if the selected theme cannot be read.
    theme_bridge::apply(&window, &theme::builtin_dark());
    let saved_state = config.as_ref().map(ConfigStore::load_state).unwrap_or_default();
    let (initial_settings, plan) = apply_config_and_start(
        &window,
        &mut files,
        std::env::args_os().nth(1).map(PathBuf::from),
        Some(&saved_state.session),
    );

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
        watcher::watch_config(store, move || {
            let fresh = reader.read_files();
            let files = files.clone();
            let _ = weak.upgrade_in_event_loop(move |window| {
                let mut current = files.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                *current = fresh;
                let (_, plan) = apply_config_and_start(&window, &mut current, None, None);
                navigation::with_current(|nav| nav.set_start(plan.start));
            });
        })
        .map_err(|err| {
            eprintln!("gezik: cannot watch {}: {err}", store.dir().display());
            let warning = Warning::new("config", format!("cannot watch the config folder for changes: {err}"));
            let mut current = files_for_warning.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            current.warnings.push(warning);
        })
        .ok()
    });
    apply_config(&window, &files.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
    window_state::restore(&window, &saved_state);
    keep_on_screen(window.as_weak(), 0);
    let view = view::View::new(&window, memory, config.clone());
    view.set_defaults(initial_settings.view);
    view.set_options(view_options::current());
    view.set_columns(saved_state.columns.clone().unwrap_or_else(gezik_core::view::default_columns));
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
    let preview = preview::Preview::new(&window, view.clone());
    preview.set_pane_open(saved_state.preview_open);
    let StartPlan { session, select, start, .. } = plan;
    let nav = navigation::Navigator::new(&window, view.clone(), session, select, start);
    nav.install();
    // The open tabs go to state.toml as they change (spec 5.1); its own thread writes them, so
    // a crash or a kill leaves the last tabs too.
    if let Some(store) = config.clone() {
        nav.keep_session(saved_state.session.clone(), initial_settings.session.restore, move |session| {
            let session = session.clone();
            store.update_state(move |state| state.session = session);
        });
    }
    let _path_box = path_box::PathBox::new(&window, nav.clone(), config.clone(), saved_state.history.clone());
    // Captures no navigator (it is not `Send`): the result finds it on the UI thread.
    places::load_in_background(window.as_weak(), |part| navigation::with_current(|nav| nav.set_places(part)));

    let sidebar = sidebar::Sidebar::new(&window, nav.clone(), config.clone());
    sidebar.install();
    sidebar.set_pinned(initial_settings.pinned);
    let dialogs = dialog::Dialogs::new(&window);
    let _tab_sets = tab_sets::TabSets::new(&window, nav.clone(), view.clone(), dialogs.clone(), config.clone());
    let _filter = filter::Filter::new(&window, view.clone(), dialogs.clone(), config.clone());
    let _tab_tools = tab_tools::TabTools::new(&window, nav.clone());
    let _select_tools = select_tools::SelectTools::new(
        view.clone(),
        dialogs.clone(),
        config.clone(),
        saved_state.selection.last_pattern.clone(),
    );
    let engine_settings = gezik_ops::Settings {
        threads: initial_settings.files.copy_threads,
        pending_deletes: config.as_ref().map(|store| store.dir().join("pending-deletes")),
    };
    let ops = operations::Operations::new(
        &window,
        nav.clone(),
        view.clone(),
        sidebar.clone(),
        dialogs.clone(),
        engine_settings,
        initial_settings.files,
        saved_state.operations_collapsed,
        config.clone(),
        saved_state.batch_rename.clone().unwrap_or_default(),
    );
    let _batch_rename = batch_rename::BatchRename::new(&window, ops.clone());
    #[cfg(target_os = "macos")]
    menu_bar::install(&window, view.clone(), nav.clone(), ops.clone());
    // Tools Gezik downloads (7-Zip) go to `<config dir>/tools/`, next to the pending deletes.
    let _archives =
        archives::Archives::new(&window, ops.clone(), dialogs.clone(), config.clone(), saved_state.archive.clone());
    let _convert = convert::Convert::new(&window, ops.clone(), dialogs, config.clone(), saved_state.convert.clone());
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
    // Deletes cut short last time finish in the background once the window is up.
    slint::Timer::single_shot(std::time::Duration::from_millis(500), {
        let ops = ops.clone();
        move || ops.recover()
    });
    let save_and_quit: Rc<dyn Fn()> = {
        let (weak, store, view, preview, ops) =
            (window.as_weak(), config.clone(), view.clone(), preview.clone(), ops.clone());
        Rc::new(move || {
            if let (Some(window), Some(store)) = (weak.upgrade(), &store) {
                store.update_state(|state| {
                    window_state::capture_into(&window, state);
                    state.columns = Some(view.columns());
                    state.preview_open = preview.is_pane_open();
                    state.preview_width = Some(window.get_preview_width().round().clamp(200.0, 600.0) as u32);
                    state.operations_collapsed = ops.collapsed();
                });
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
    window.window().on_close_requested({
        let (ops, save_and_quit, weak) = (ops.clone(), save_and_quit.clone(), window.as_weak());
        move || {
            let quit = {
                let (save_and_quit, weak) = (save_and_quit.clone(), weak.clone());
                move || {
                    save_and_quit();
                    if let Some(window) = weak.upgrade() {
                        let _ = window.hide();
                    }
                    let _ = slint::quit_event_loop();
                }
            };
            if ops.confirm_close(quit) {
                return slint::CloseRequestResponse::KeepWindowShown;
            }
            save_and_quit();
            slint::CloseRequestResponse::HideWindow
        }
    });
    window.on_sidebar_clicked({
        let (nav, sidebar) = (nav.clone(), sidebar.clone());
        move |section, index| {
            if let Some(location) = sidebar.location_of(section, index) {
                nav.go(location);
            }
        }
    });
    window.on_sidebar_middle_clicked({
        let (nav, sidebar) = (nav.clone(), sidebar.clone());
        move |section, index| {
            if let Some(location) = sidebar.location_of(section, index) {
                nav.open_tab(location, false);
            }
        }
    });
    window.on_pinned_move({
        let sidebar = sidebar.clone();
        move |from, to| {
            if let (Ok(from), Ok(to)) = (usize::try_from(from), usize::try_from(to)) {
                sidebar.move_pinned(from, to);
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
    let menus =
        context_menu::Menus::new(&window, nav.clone(), view.clone(), preview.clone(), sidebar.clone(), ops.clone());
    let drags = drag::Drags::new(&window, nav.clone(), view.clone(), sidebar, ops.clone(), menus.clone());
    drags.install(&window);
    drags.attach_when_ready(0);
    window.on_row_menu({
        let (menus, view) = (menus.clone(), view.clone());
        move |i, x, y| {
            if let Ok(index) = usize::try_from(i) {
                view.prepare_menu(index);
            }
            menus.row(i, x, y)
        }
    });
    // Right-click on empty space clears the selection, as in Explorer. The Windows menu
    // opens at the cursor.
    window.on_background_menu({
        let (menus, view) = (menus.clone(), view.clone());
        move |x, y| {
            view.clear_selection();
            menus.background(x, y)
        }
    });
    window.on_item_pressed({
        let view = view.clone();
        let ops = ops.clone();
        move |i, ctrl, shift| {
            ops.end_unfocused_rename();
            if let Ok(index) = usize::try_from(i) {
                view.press(index, ctrl, shift);
            }
        }
    });
    window.on_marquee({
        let view = view.clone();
        move |x, y, width, height, additive| view.marquee(gezik_core::layout::Rect { x, y, width, height }, additive)
    });
    window.on_marquee_done({
        let view = view.clone();
        move || view.marquee_done()
    });
    window.on_background_pressed({
        let view = view.clone();
        let ops = ops.clone();
        move |ctrl| {
            ops.end_unfocused_rename();
            if !ctrl {
                view.clear_selection();
            }
        }
    });
    window.on_keyboard_menu({
        let menus = menus.clone();
        move |i, x, y| menus.keyboard(i, x, y)
    });
    window.on_sidebar_menu({
        let menus = menus.clone();
        move |section, i, x, y| menus.sidebar_entry(section, i, x, y)
    });
    window.on_header_clicked({
        let view = view.clone();
        move |column| view.header_clicked(column)
    });
    window.on_preview_resized({
        let preview = preview.clone();
        move || preview.schedule()
    });
    window.on_columns_resized({
        let view = view.clone();
        move || view.columns_resized()
    });
    window.on_header_menu({
        let menus = menus.clone();
        move |x, y| menus.header(x, y)
    });
    window.on_filter_menu({
        let menus = menus.clone();
        move |left, bottom, right, top| menus.filter_menu(popup::Anchor::below(left, top, right, bottom))
    });
    window.on_view_menu({
        let menus = menus.clone();
        move |left, bottom, right, top| menus.view_menu(popup::Anchor::below(left, top, right, bottom))
    });
    window.on_grid_columns_changed({
        let view = view.clone();
        move |columns| view.grid_columns_changed(usize::try_from(columns).unwrap_or(1))
    });
    window.on_zoom({
        let view = view.clone();
        move |bigger| view.zoom(bigger)
    });
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
    // Slint passes indexes as `i32`: a negative one does nothing.
    window.on_tab_menu(move |i, x, y| {
        if let Ok(i) = usize::try_from(i) {
            menus.tab(i, x, y);
        }
    });

    // Double-click; with single-click-open the click already opened it.
    window.on_open_row({
        let (nav, view) = (nav.clone(), view.clone());
        move |i| {
            if let Ok(index) = usize::try_from(i)
                && !view_options::current().single_click_open
            {
                open_entry(&nav, &view, index);
            }
        }
    });
    window.on_go_back({
        let nav = nav.clone();
        move || nav.back()
    });
    window.on_go_forward({
        let nav = nav.clone();
        move || nav.forward()
    });
    window.on_go_up({
        let nav = nav.clone();
        move || nav.up()
    });
    window.on_refresh({
        let nav = nav.clone();
        move || nav.reload()
    });
    window.on_navigate({
        let nav = nav.clone();
        move |text| nav.navigate_text(text.into())
    });
    window.on_crumb_clicked({
        let nav = nav.clone();
        move |i| nav.crumb_clicked(i)
    });
    // Slint passes indexes as `i32`: a negative one does nothing.
    window.on_tab_activate({
        let nav = nav.clone();
        move |i| {
            if let Ok(i) = usize::try_from(i) {
                nav.activate_tab(i);
            }
        }
    });
    // Closed once the click is fully handled.
    window.on_tab_close({
        let nav = nav.clone();
        move |i| {
            if let Ok(i) = usize::try_from(i) {
                close_tab_later(&nav, i);
            }
        }
    });
    window.on_tab_new({
        let nav = nav.clone();
        move || nav.open_tab(nav.start(), true)
    });
    window.on_tab_move({
        let nav = nav.clone();
        move |from, to| {
            if let (Ok(from), Ok(to)) = (usize::try_from(from), usize::try_from(to)) {
                nav.move_tab(from, to);
            }
        }
    });
    window.on_row_middle_clicked({
        let nav = nav.clone();
        move |i| {
            if let Some((path, true)) = nav.entry_path(i) {
                nav.open_tab(gezik_core::nav::Location::Path(path), false);
            }
        }
    });

    window.on_rename_accepted({
        let ops = ops.clone();
        move |text| ops.rename_accepted(text.into())
    });
    window.on_rename_cancelled({
        let ops = ops.clone();
        move || ops.rename_cancelled()
    });
    window.on_rename_tab({
        let ops = ops.clone();
        move |text, back| ops.rename_tab(text.into(), back)
    });
    window.on_rename_blurred({
        let ops = ops.clone();
        move |text, generation| ops.rename_blurred(text.into(), generation)
    });
    window.on_rename_edited({
        let ops = ops.clone();
        move |text| ops.rename_edited(&text)
    });

    window.on_key_event({
        let (nav, view, preview, ops, weak) =
            (nav.clone(), view.clone(), preview.clone(), ops.clone(), window.as_weak());
        let drags = drags.clone();
        let mut type_ahead = keys::TypeAhead::new();
        move |event| {
            let Some(window) = weak.upgrade() else { return false };
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
                &nav,
                &view,
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
                    winit::event::MouseButton::Back => nav.back(),
                    winit::event::MouseButton::Forward => nav.forward(),
                    _ => {}
                }
            }
            EventResult::Propagate
        });
    }

    window.run()
}
