// Hide the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod context_menu;
mod keys;
mod navigation;
mod places;
mod sidebar;
mod start;
mod theme_bridge;
mod watcher;
mod window_state;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use gezik_config::Warning;
use gezik_config::settings::{Settings, SidebarPosition};
use gezik_config::shortcuts::{Action, Chord, Key, Platform};
use gezik_config::store::{self, ConfigFiles, ConfigStore, Loaded};
use gezik_config::theme;
use slint::Model;
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
    keys::set_shortcuts(loaded.settings.shortcuts.clone());
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
fn handle_key(
    window: &AppWindow,
    nav: &navigation::Navigator,
    type_ahead: &mut keys::TypeAhead,
    text: &str,
    chord: Option<Chord>,
    has_modifier: bool,
) -> bool {
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
        match action {
            Action::NewTab => nav.open_tab(nav.start(), true),
            Action::CloseTab => close_tab_later(nav, nav.with_tabs(|tabs| tabs.active_index())),
            Action::NextTab => nav.next_tab(),
            Action::PrevTab => nav.prev_tab(),
            Action::Back => nav.back(),
            Action::Forward => nav.forward(),
            Action::Up => nav.up(),
            Action::FocusPath => window.invoke_edit_path(),
            Action::Refresh => nav.reload(),
        }
        // The typed text no longer fits once the location or tab changed.
        if editing && action != Action::FocusPath {
            window.invoke_focus_list();
        }
        return true;
    }
    if editing || has_modifier || !window.get_list_focused() {
        return false;
    }

    // List navigation (fixed keys).
    let count = i32::try_from(window.get_rows().row_count()).unwrap_or(i32::MAX);
    let page = window.get_list_page_rows().max(1);
    let selected = window.get_selected();
    let target = match chord.map(|c| c.key) {
        Some(Key::Down) => Some(selected.saturating_add(1)),
        Some(Key::Up) => Some(selected.saturating_sub(1)),
        Some(Key::PageDown) => Some(selected.saturating_add(page)),
        Some(Key::PageUp) => Some(selected.saturating_sub(page)),
        Some(Key::Home) => Some(0),
        Some(Key::End) => Some(count - 1),
        Some(Key::Enter) => {
            if selected >= 0 {
                nav.open_row(selected);
            }
            return true;
        }
        _ => {
            let Some(c) = keys::typed_char(text) else { return false };
            let rows = window.get_rows();
            let found = type_ahead.type_char(c, std::time::Instant::now(), rows.iter().map(|row| row.name.to_string()));
            // A typed character that matches nothing is still used up.
            let Some(i) = found.and_then(|i| i32::try_from(i).ok()) else { return true };
            Some(i)
        }
    };
    match target {
        Some(i) if count > 0 => {
            let i = i.clamp(0, count - 1);
            window.set_selected(i);
            window.invoke_ensure_visible(i);
            true
        }
        _ => false,
    }
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
    if let Some(store) = &config {
        window_state::restore(&window, &store.load_state());
    }
    keep_on_screen(window.as_weak(), 0);
    window.window().on_close_requested({
        let weak = window.as_weak();
        let store = config.clone();
        move || {
            // A minimized or maximized window has no meaningful normal rect: keep the old state.
            if let (Some(window), Some(store)) = (weak.upgrade(), &store)
                && !window.window().is_minimized()
                && !window.window().is_maximized()
                && let Err(err) = store.save_state(&window_state::capture(&window))
            {
                eprintln!("gezik: cannot save window state: {err}");
            }
            slint::CloseRequestResponse::HideWindow
        }
    });

    let nav = navigation::Navigator::new(&window, plan.first, plan.select, plan.start);
    nav.install();
    // Captures no navigator (it is not `Send`): the result finds it on the UI thread.
    places::load_in_background(window.as_weak(), |part| navigation::with_current(|nav| nav.set_places(part)));

    let sidebar = sidebar::Sidebar::new(&window, nav.clone(), config.clone());
    sidebar.install();
    sidebar.set_pinned(initial_settings.pinned);
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

    let menus = context_menu::Menus::new(&window, nav.clone(), sidebar);
    window.on_row_menu({
        let menus = menus.clone();
        move |i, x, y| menus.row(i, x, y)
    });
    // The Windows menu opens at the cursor; there is no Slint menu for empty space.
    window.on_background_menu({
        let menus = menus.clone();
        move |_, _| menus.background()
    });
    window.on_sidebar_menu({
        let menus = menus.clone();
        move |section, i, x, y| menus.sidebar_entry(section, i, x, y)
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

    window.on_key_event({
        let (nav, weak) = (nav.clone(), window.as_weak());
        let mut type_ahead = keys::TypeAhead::new();
        move |event| {
            let Some(window) = weak.upgrade() else { return false };
            let m = event.modifiers;
            let chord = keys::chord_from_slint(&event.text, m.control, m.alt, m.shift, m.meta, Platform::current());
            handle_key(&window, &nav, &mut type_ahead, &event.text, chord, m.control || m.alt || m.meta)
        }
    });

    // Mouse back/forward side buttons, anywhere in the window. Slint passes them on to
    // the items too, which ignore them.
    {
        use slint::winit_030::{EventResult, WinitWindowAccessor, winit};
        window.window().on_winit_window_event(move |_, event| {
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
