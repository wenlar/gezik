// Hide the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod context_menu;
mod dialog;
mod frame_limit;
mod keys;
mod media;
mod navigation;
mod operations;
mod places;
mod preview;
mod quick_look;
mod sidebar;
mod start;
mod theme_bridge;
mod view;
mod watcher;
mod window_state;

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use gezik_config::Warning;
use gezik_config::settings::{Settings, SidebarPosition};
use gezik_config::shortcuts::{Action, Chord, Key, Platform};
use gezik_config::store::{self, ConfigFiles, ConfigStore, Loaded};
use gezik_config::theme;
use gezik_core::layout::Move;
use start::StartPlan;

slint::include_modules!();

/// Resolves settings + theme from `files` and shows them. No I/O, so it runs on the UI
/// thread at startup, after config files change and when the system theme flips.
fn apply_config(window: &AppWindow, files: &ConfigFiles) -> Loaded {
    let loaded = store::resolve(files, window.get_system_dark());
    if let Some(theme) = &loaded.theme {
        theme_bridge::apply(window, theme);
    }
    for warning in &loaded.warnings {
        eprintln!("gezik: {warning}");
    }
    window.set_notice(notice_text(&loaded.warnings).into());
    window.set_sidebar_position(match loaded.settings.sidebar {
        SidebarPosition::Left => 0,
        SidebarPosition::Right => 1,
        SidebarPosition::Hidden => 2,
    });
    // Unchanged pins cost nothing (also after the reload that follows our own save).
    sidebar::with_current(|sidebar| sidebar.set_pinned(loaded.settings.pinned.clone()));
    view::with_current(|view| view.set_defaults(loaded.settings.view));
    keys::set_shortcuts(loaded.settings.shortcuts.clone());
    frame_limit::set_max_fps(loaded.settings.max_fps);
    operations::with_current(|ops| ops.set_files(loaded.settings.files));
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
    if window.get_dialog_open() {
        return false;
    }
    // The name field being edited has the keyboard (Enter, Esc, Tab are its own).
    if ops.end_unfocused_rename() {
        return false;
    }
    let editing = window.get_path_editing();

    if editing && let Some(chord) = &chord {
        if chord.key == Key::Escape && !has_modifier {
            window.invoke_focus_list();
            window.set_path_editing(false);
            return true;
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
            if keys::acts_on_files(action) && (editing || (keys::needs_list(action) && !window.get_list_focused())) {
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
                Action::FocusPath => window.invoke_edit_path(),
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
                Action::Undo => ops.undo(),
                Action::Redo => ops.redo(),
            }
            // The typed text no longer fits once the location or tab changed.
            if editing && action != Action::FocusPath {
                window.invoke_focus_list();
            }
            return true;
        }
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
                Key::Escape if !primary && !chord.shift => {
                    view.clear_selection();
                    return true;
                }
                _ => {}
            }
        }
    }
    if has_modifier {
        return false;
    }

    // Type-ahead. A typed character that matches nothing is still used up.
    let Some(c) = keys::typed_char(text) else { return false };
    if let Some(i) = type_ahead.type_char(c, std::time::Instant::now(), |typed| view.find_prefix(typed)) {
        view.jump_to(i);
    }
    true
}

/// Like [`apply_config`], and also resolves where the app opens. Start warnings (bad
/// `start-folder`, missing command-line path) are added to `files` so the notice shows
/// them (also after later re-resolves, until the files are read again).
fn apply_config_and_start(window: &AppWindow, files: &mut ConfigFiles, cli: Option<PathBuf>) -> (Settings, StartPlan) {
    let loaded = apply_config(window, files);
    let plan = resolve_start(&loaded.settings, cli);
    if !plan.warnings.is_empty() {
        files.warnings.extend(plan.warnings.iter().cloned());
        apply_config(window, files);
    }
    (loaded.settings, plan)
}

/// [`start::plan_start`] against the real file system.
fn resolve_start(settings: &Settings, cli: Option<PathBuf>) -> StartPlan {
    // Absolute, so the address bar parts and "up" work for `gezik .` too.
    let cli = cli.map(|path| std::path::absolute(&path).unwrap_or(path));
    let dirs = gezik_config::paths::KnownDirs::system();
    start::plan_start(&settings.start_folder, cli, &dirs_home(), |text| dirs.expand_checked(text), start::path_kind)
}

fn dirs_home() -> PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("/"))
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
    let window = AppWindow::new()?;

    let config = ConfigStore::system();
    let init_error = config.as_ref().and_then(|store| store.ensure_initialized().err().map(|e| (store, e)));
    let mut files = config.as_ref().map(ConfigStore::read_files).unwrap_or_default();
    if let Some((store, err)) = init_error {
        files
            .warnings
            .push(Warning::new(store.dir().display().to_string(), format!("cannot create config folder: {err}")));
    }
    // Release builds hide stderr, so config problems also go to the status bar.
    if config.is_none() {
        eprintln!("gezik: no config folder available; using defaults");
        files.warnings.push(Warning::new("config", "no config folder available; using default settings"));
    }
    // Something sensible is on screen even if the selected theme cannot be read.
    theme_bridge::apply(&window, &theme::builtin_dark());
    let (initial_settings, plan) =
        apply_config_and_start(&window, &mut files, std::env::args_os().nth(1).map(PathBuf::from));

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
                let (_, plan) = apply_config_and_start(&window, &mut current, None);
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
    let saved_state = config.as_ref().map(ConfigStore::load_state).unwrap_or_default();
    window_state::restore(&window, &saved_state);
    keep_on_screen(window.as_weak(), 0);
    let view = view::View::new(&window, memory, config.clone());
    view.set_defaults(initial_settings.view);
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
    let nav = navigation::Navigator::new(&window, view.clone(), plan.first, plan.select, plan.start);
    nav.install();
    // Captures no navigator (it is not `Send`): the result finds it on the UI thread.
    places::load_in_background(window.as_weak(), |part| navigation::with_current(|nav| nav.set_places(part)));

    let sidebar = sidebar::Sidebar::new(&window, nav.clone(), config.clone());
    sidebar.install();
    sidebar.set_pinned(initial_settings.pinned);
    let dialogs = dialog::Dialogs::new(&window);
    let engine_settings = gezik_ops::Settings {
        threads: initial_settings.files.copy_threads,
        pending_deletes: config.as_ref().map(|store| store.dir().join("pending-deletes")),
    };
    let ops = operations::Operations::new(
        &window,
        nav.clone(),
        view.clone(),
        sidebar.clone(),
        dialogs,
        engine_settings,
        initial_settings.files,
        saved_state.operations_collapsed,
    );
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
                let mut state = store.load_state();
                window_state::capture_into(&window, &mut state);
                state.columns = Some(view.columns());
                state.preview_open = preview.is_pane_open();
                state.preview_width = Some(window.get_preview_width().round().clamp(200.0, 600.0) as u32);
                state.operations_collapsed = ops.collapsed();
                if let Err(err) = store.save_state(&state) {
                    eprintln!("gezik: cannot save window state: {err}");
                }
                view.flush_memory();
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

    let menus = context_menu::Menus::new(&window, nav.clone(), view.clone(), preview.clone(), sidebar, ops.clone());
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
    window.on_view_menu({
        let menus = menus.clone();
        move |x, y| menus.view_menu(x, y)
    });
    window.on_grid_columns_changed({
        let view = view.clone();
        move |columns| view.grid_columns_changed(usize::try_from(columns).unwrap_or(1))
    });
    window.on_zoom({
        let view = view.clone();
        move |bigger| view.zoom(bigger)
    });
    // Slint passes indexes as `i32`: a negative one does nothing.
    window.on_tab_menu(move |i, x, y| {
        if let Ok(i) = usize::try_from(i) {
            menus.tab(i, x, y);
        }
    });

    window.on_open_row({
        let nav = nav.clone();
        move |i| nav.open_row(i)
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
        let mut type_ahead = keys::TypeAhead::new();
        move |event| {
            let Some(window) = weak.upgrade() else { return false };
            let m = event.modifiers;
            let chord = keys::chord_from_slint(&event.text, m.control, m.alt, m.shift, m.meta, Platform::current());
            let menu_key = keys::is_context_menu_key(&event.text, m.control, m.alt, m.shift, m.meta);
            handle_key(
                &window,
                &nav,
                &view,
                &preview,
                &ops,
                &mut type_ahead,
                &event.text,
                chord,
                m.control || m.alt || m.meta,
                menu_key,
            )
        }
    });

    // Mouse back/forward side buttons, anywhere in the window. Slint passes them on to
    // the items too, which ignore them.
    {
        use slint::winit_030::{EventResult, WinitWindowAccessor, winit};
        let weak = window.as_weak();
        let minimized = std::cell::Cell::new(false);
        let ops = ops.clone();
        window.window().on_winit_window_event(move |_, event| {
            if let winit::event::WindowEvent::Focused(true) = event {
                ops.clipboard_check();
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
