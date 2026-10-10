//! The windows of the process (spec 10 §9.3): a register of them in focus order, the window
//! whose event is being handled, and the parts each window has of its own ([`Slot`]).
//!
//! A module's window parts sit in a [`Slot`]; its `with_current` reaches the [`current`]
//! window's. A background result or a timer keeps the id of the window it was started in and
//! runs in that window ([`with_window`], [`bind`]): a closed window's is dropped.
//!
//! Everything here is read as `panes::with_picked` reads: the value is cloned out of the
//! borrow before `f` runs (an event of one window may open or close another).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use gezik_config::settings::State;
use gezik_config::shortcuts::{Key, Platform};
use gezik_config::store::{ConfigFiles, ConfigStore, Loaded};
use gezik_core::nav::Session;
use gezik_core::view_memory::ViewMemory;
use slint::ComponentHandle;
use slint::winit_030::winit::keyboard::ModifiersState;
use slint::winit_030::{EventResult, WinitWindowAccessor, winit};

use crate::frame_limit::{self, FrameLimit};
#[cfg(target_os = "macos")]
use crate::menu_bar;
use crate::operations::Operations;
use crate::start::StartPlan;
use crate::{
    AppWindow, MenuPlace, archives, batch_rename, context_menu, convert, dialog, drag, dual, filter, folder_sizes,
    info, integration, keys, navigation, palette, panes, path_box, places, popup, preview, resident, saved_searches,
    search, select_tools, sidebar, stack, tab_sets, tab_tools, theme_bridge, view, view_options, window_state,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowId(u32);

thread_local! {
    /// The open windows, the most recently focused first.
    static WINDOWS: RefCell<Vec<(WindowId, AppWindow)>> = const { RefCell::new(Vec::new()) };
    /// The window whose event is being handled (the router sets it on each one), or that
    /// [`enter`] runs code for.
    static ACTING: Cell<Option<WindowId>> = const { Cell::new(None) };
    static NEXT: Cell<u32> = const { Cell::new(0) };
    /// The settings last resolved, for a window opened later (deviation 16).
    static LOADED: RefCell<Option<Rc<Loaded>>> = const { RefCell::new(None) };
}

/// The parts of a window that are kept by window: each slot forgets a closed window's
/// (deviation 2; a test counts the slots against this list).
#[cfg_attr(not(test), allow(dead_code, reason = "closing a window comes with Task 3"))]
const FORGETS: &[fn(WindowId)] = &[
    |id| crate::archives::CURRENT.with(|s| s.forget(id)),
    |id| crate::batch_rename::CURRENT.with(|s| s.forget(id)),
    |id| crate::convert::CURRENT.with(|s| s.forget(id)),
    |id| crate::drag::CURRENT.with(|s| s.forget(id)),
    |id| crate::dual::DUAL.with(|s| s.forget(id)),
    |id| crate::info::CURRENT.with(|s| s.forget(id)),
    |id| crate::integration::CURRENT.with(|s| s.forget(id)),
    |id| crate::operations::PARTS.with(|s| s.forget(id)),
    |id| crate::palette::CURRENT.with(|s| s.forget(id)),
    |id| crate::panes::PANES.with(|s| s.forget(id)),
    |id| crate::preview::CURRENT.with(|s| s.forget(id)),
    |id| crate::saved_searches::CURRENT.with(|s| s.forget(id)),
    |id| crate::select_tools::CURRENT.with(|s| s.forget(id)),
    |id| crate::sidebar::CURRENT.with(|s| s.forget(id)),
    |id| crate::stack::CURRENT.with(|s| s.forget(id)),
    |id| crate::tab_sets::CURRENT.with(|s| s.forget(id)),
    |id| crate::tab_tools::CURRENT.with(|s| s.forget(id)),
    |id| crate::window_state::PENDING.with(|s| s.forget(id)),
];

/// A part each window has of its own: a value per open window.
pub struct Slot<T>(RefCell<Vec<(WindowId, T)>>);

impl<T: Clone> Slot<T> {
    pub const fn new() -> Slot<T> {
        Slot(RefCell::new(Vec::new()))
    }

    /// Window `id`'s value is `value` now.
    pub fn set(&self, id: WindowId, value: T) {
        let old = {
            let mut list = self.0.borrow_mut();
            match list.iter_mut().find(|(of, _)| *of == id) {
                Some((_, slot)) => Some(std::mem::replace(slot, value)),
                None => {
                    list.push((id, value));
                    None
                }
            }
        };
        // Dropped outside the borrow: its drop may reach the slot.
        drop(old);
    }

    pub fn get(&self, id: WindowId) -> Option<T> {
        self.0.borrow().iter().find(|(of, _)| *of == id).map(|(_, v)| v.clone())
    }

    /// The [`current`] window's value.
    pub fn current(&self) -> Option<T> {
        self.get(current())
    }

    /// The current window's value, made with `make` if it has none yet.
    pub fn current_or(&self, make: impl FnOnce() -> T) -> T {
        let id = current();
        if let Some(value) = self.get(id) {
            return value;
        }
        let value = make();
        self.set(id, value.clone());
        value
    }

    /// The first of `f`'s answers over the windows' values, looked for under the borrow: `f`
    /// must not reach the slot (it only looks).
    pub fn find_map<R>(&self, mut f: impl FnMut(WindowId, &T) -> Option<R>) -> Option<R> {
        self.0.borrow().iter().find_map(|(id, value)| f(*id, value))
    }

    /// Every window's value, with the window.
    pub fn all(&self) -> Vec<(WindowId, T)> {
        self.0.borrow().clone()
    }

    pub fn forget(&self, id: WindowId) {
        let gone: Vec<_> = {
            let mut list = self.0.borrow_mut();
            let (gone, kept) = std::mem::take(&mut *list).into_iter().partition(|(of, _)| *of == id);
            *list = kept;
            gone
        };
        drop(gone);
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.0.borrow().len()
    }
}

/// Puts `window` in the register, in front; its id.
pub fn install(window: AppWindow) -> WindowId {
    let id = WindowId(NEXT.with(|n| n.replace(n.get() + 1)));
    WINDOWS.with(|w| w.borrow_mut().insert(0, (id, window)));
    id
}

/// Takes window `id` out of the register and every slot: nothing of it is kept here.
#[cfg_attr(not(test), allow(dead_code, reason = "closing a window comes with Task 3"))]
pub fn forget(id: WindowId) {
    let gone = WINDOWS.with(|w| {
        let mut list = w.borrow_mut();
        let at = list.iter().position(|(of, _)| *of == id)?;
        Some(list.remove(at))
    });
    if ACTING.with(Cell::get) == Some(id) {
        ACTING.with(|a| a.set(None));
    }
    for forget in FORGETS {
        forget(id);
    }
    drop(gone);
}

/// Calls [`release_all`] when dropped: main's every return lets the windows go in order.
pub struct Release;

impl Drop for Release {
    fn drop(&mut self) {
        release_all();
    }
}

/// Lets go of every window at the end of main, as main's own handle went before 10g (their
/// drop is not left to the thread's end; the slots stay as they did).
pub fn release_all() {
    let gone = WINDOWS.with(|w| std::mem::take(&mut *w.borrow_mut()));
    ACTING.with(|a| a.set(None));
    drop(gone);
}

/// Window `id` got the keyboard focus: it goes in front.
pub fn focused(id: WindowId) {
    WINDOWS.with(|w| {
        let mut list = w.borrow_mut();
        if let Some(at) = list.iter().position(|(of, _)| *of == id) {
            let window = list.remove(at);
            list.insert(0, window);
        }
    });
}

/// The open windows, the most recently focused first.
pub fn focus_order() -> Vec<WindowId> {
    WINDOWS.with(|w| w.borrow().iter().map(|(id, _)| *id).collect())
}

/// The window an event is handled for (or [`enter`] runs code for), else the most recently
/// focused one. Before any window (tests), the first window's id.
pub fn current() -> WindowId {
    ACTING.with(Cell::get).or_else(|| focus_order().first().copied()).unwrap_or(WindowId(0))
}

/// The router: the window whose event comes now.
fn acting(id: WindowId) {
    ACTING.with(|a| a.set(Some(id)));
}

/// Runs `f` as window `id`'s: [`current`] is `id` inside.
pub fn enter<R>(id: WindowId, f: impl FnOnce() -> R) -> R {
    let before = ACTING.with(|a| a.replace(Some(id)));
    let result = f();
    ACTING.with(|a| a.set(before));
    result
}

fn window_of(id: WindowId) -> Option<AppWindow> {
    WINDOWS.with(|w| w.borrow().iter().find(|(of, _)| *of == id).map(|(_, window)| window.clone_strong()))
}

/// Runs `f` in window `id` ([`enter`]), if it is still open.
pub fn with_window<R>(id: WindowId, f: impl FnOnce(&AppWindow) -> R) -> Option<R> {
    let window = window_of(id)?;
    Some(enter(id, || f(&window)))
}

/// Runs `f` in the most recently focused window (the single instance, the tray, the global
/// shortcut, the system's open requests: spec 9.3, decision 32).
pub fn with_focused<R>(f: impl FnOnce(&AppWindow) -> R) -> Option<R> {
    with_window(*focus_order().first()?, f)
}

/// Runs `f` in each open window in turn.
pub fn for_each(mut f: impl FnMut(&AppWindow)) {
    let list: Vec<_> = WINDOWS.with(|w| w.borrow().iter().map(|(id, w)| (*id, w.clone_strong())).collect());
    for (id, window) in list {
        enter(id, || f(&window));
    }
}

/// `f`, to run later (a timer) in the window that is current now; it is dropped if that
/// window has closed by then. Call it on the UI thread: a background thread takes the id
/// with [`current`] before it starts, and runs its result with [`with_window`].
pub fn bind(f: impl FnOnce()) -> impl FnOnce() {
    let id = current();
    move || {
        with_window(id, |_| f());
    }
}

/// Hands `f` to the UI thread, to run in window `id` (taken with [`current`] on the UI
/// thread before the work left it); dropped if that window has closed by then.
pub fn post(id: WindowId, f: impl FnOnce() + Send + 'static) {
    let _ = slint::invoke_from_event_loop(move || {
        with_window(id, |_| f());
    });
}

/// [`bind`] for a timer's callback that runs again and again.
pub fn bind_mut(mut f: impl FnMut()) -> impl FnMut() {
    let id = current();
    move || {
        with_window(id, |_| f());
    }
}

/// The settings (and theme) last resolved; main resolves them before the first window opens.
pub fn loaded() -> Option<Rc<Loaded>> {
    LOADED.with(|l| l.borrow().clone())
}

pub fn set_loaded(loaded: Loaded) {
    LOADED.with(|l| *l.borrow_mut() = Some(Rc::new(loaded)));
}

/// What the windows share, made once by main (the process's parts, spec 9.6).
pub struct Process {
    pub config: Option<ConfigStore>,
    /// state.toml as read at start.
    pub state: Rc<State>,
    pub memory: Rc<RefCell<ViewMemory>>,
    pub media: crate::media::Media,
    pub ops: Operations,
    /// The config files last read (a system light/dark switch resolves them again).
    pub files: Arc<Mutex<ConfigFiles>>,
    /// A Gezik that could not hand over (spec 5.3, deviation 17): it writes no state.
    pub secondary: bool,
    /// A window's close button: the tray, the running jobs' question, saving.
    pub close_requested: Rc<dyn Fn() -> slint::CloseRequestResponse>,
    /// Saves what the window shows (Windows: signing out ends Gezik without a close request).
    #[cfg_attr(not(windows), allow(dead_code))]
    pub save: Rc<dyn Fn()>,
}

/// What a window opens with.
pub struct Opening {
    /// Its left pane's tabs and the start folder.
    pub plan: StartPlan,
    /// Its right pane's tabs.
    pub right: Session,
    /// The right pane is open, which pane is active, sync browsing is on.
    pub dual: bool,
    pub active_pane: usize,
    pub sync: bool,
    /// How far it is moved off the saved position (spec 5.3).
    pub offset: i32,
}

/// Builds window `id`'s parts and callbacks (all but showing it): its panes, sidebar, preview,
/// layers, menus and drags, and the router of its events.
pub fn open(id: WindowId, opening: Opening, process: &Process) {
    let Some(window) = window_of(id) else { return };
    enter(id, || build(id, &window, opening, process));
}

#[allow(clippy::too_many_lines, reason = "main's window set-up, moved here as it was")]
fn build(id: WindowId, window: &AppWindow, opening: Opening, process: &Process) {
    let window = window.clone_strong();
    let Some(loaded) = loaded() else { return };
    let settings = &loaded.settings;
    let state = process.state.clone();
    let config = process.config.clone();
    let secondary = process.secondary;
    window.on_system_scheme_changed({
        let files = process.files.clone();
        move || {
            crate::apply_config(&files.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
        }
    });
    window_state::restore(&window, &state, opening.offset);
    resident::when_shown({
        let weak = window.as_weak();
        move || crate::keep_on_screen(id, weak, 0)
    });
    // The pane's parts carry its id; it is installed once they all exist.
    let pane_id = panes::next_id();
    window.set_panes(panes::model());
    // One media and one folder view memory for the process; each view holds a client of the
    // media and the same memory (spec 10 §3.3).
    let view =
        view::View::new(pane_id, window.as_weak(), process.media.client(), process.memory.clone(), config.clone());
    view.set_defaults(settings.view);
    // No folder shows yet: its place comes with the first one.
    view.set_rules(view::compile_rules(&settings.view_rules), Default::default);
    view.set_options(view_options::current());
    view.set_columns(state.columns.clone().unwrap_or_else(gezik_core::view::default_columns));
    view.set_result_columns(state.result_columns.clone().unwrap_or_else(gezik_core::view::default_result_columns));
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
    window.set_preview_width(state.preview_width.unwrap_or(280) as f32);
    window.set_column_width(state.column_width.unwrap_or(crate::COLUMN_WIDTH) as f32);
    let folder_sizes = folder_sizes::FolderSizes::new(pane_id, window.as_weak(), view.clone());
    folder_sizes.set_settings(settings.folder_sizes, settings.search.everything);
    let preview = preview::Preview::new(&window);
    let StartPlan { session, select, start, .. } = opening.plan;
    let nav = navigation::Navigator::new(pane_id, window.as_weak(), view.clone(), session, select, start);
    // The open tabs go to state.toml as they change (spec 5.1); its own thread writes them, so
    // a crash or a kill leaves the last tabs too.
    if !secondary && let Some(store) = config.clone() {
        nav.keep_session(state.session.clone(), settings.session.restore, move |session| {
            let session = session.clone();
            store.update_state(move |state| state.session = session);
        });
    }
    let path_box =
        path_box::PathBox::new(pane_id, window.as_weak(), nav.clone(), config.clone(), state.history.clone());
    // Captures no navigator (it is not `Send`): the result finds the panes on the UI thread.
    places::load_in_background(window.as_weak(), move |part| {
        with_window(id, |_| {
            for p in panes::all() {
                p.nav.set_places(part.clone());
            }
        });
    });

    let dialogs = dialog::Dialogs::new(&window);
    let filter = filter::Filter::new(pane_id, window.as_weak(), view.clone(), dialogs.clone(), config.clone());
    let searches = search::Searches::new(pane_id, window.as_weak(), nav.clone(), view.clone(), dialogs.clone());
    searches.set_settings(settings.search.clone());
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
    // Opening the preview reads the active pane's selection.
    preview.set_pane_open(state.preview_open);
    let restore = settings.session.restore && !secondary;
    let state_store = config.clone().filter(|_| !secondary);
    let right_session = if restore { opening.right } else { Default::default() };
    let stores = (state_store, config.clone());
    dual::install(&window, stores, settings.session.restore, right_session, state.pane_split);
    // The first tab loads once the pane is installed: its listing comes back through it.
    nav.install();
    let sidebar = sidebar::Sidebar::new(&window, config.clone(), dialogs.clone());
    sidebar.install();
    sidebar.set_pinned(settings.pinned.clone());
    sidebar.set_show_cloud(settings.sidebar_cloud);
    sidebar.set_tree_follow(settings.sidebar_tree_follow);
    let _tab_sets = tab_sets::TabSets::new(dialogs.clone(), config.clone());
    let _saved_searches = saved_searches::SavedSearches::new(dialogs.clone(), config.clone());
    let _tab_tools = tab_tools::TabTools::new(&window);
    let _integration = integration::Integration::new(&window);
    let _select_tools =
        select_tools::SelectTools::new(dialogs.clone(), config.clone(), state.selection.last_pattern.clone());
    let ops = process.ops.clone();
    ops.attach(&window, dialogs.clone());
    let _palette = palette::Palette::new(&window, config.clone(), state.palette_recent.clone(), {
        let (weak, preview, ops) = (window.as_weak(), preview.clone(), ops.clone());
        move |action| {
            if let Some(window) = weak.upgrade() {
                let p = panes::active();
                crate::perform(action, &window, &p.nav, &p.view, &preview, &ops);
            }
        }
    });
    let _batch_rename = batch_rename::BatchRename::new(&window, ops.clone());
    let _stack = stack::Stack::new(&window, ops.clone());
    #[cfg(target_os = "macos")]
    menu_bar::install(&window, ops.clone());
    // Tools Gezik downloads (7-Zip) go to `<config dir>/tools/`, next to the pending deletes.
    let _archives =
        archives::Archives::new(&window, ops.clone(), dialogs.clone(), config.clone(), state.archive.clone());
    let _info = info::Info::new(&window, ops.clone(), dialogs.clone());
    let _convert = convert::Convert::new(&window, ops.clone(), dialogs.clone(), config.clone(), state.convert.clone());
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
    #[cfg(windows)]
    if !secondary {
        resident::when_shown({
            let (weak, save) = (window.as_weak(), process.save.clone());
            move || crate::save_on_session_end(weak, save, 0)
        });
    }
    window.window().on_close_requested({
        let close_requested = process.close_requested.clone();
        move || with_window(id, |_| close_requested()).unwrap_or(slint::CloseRequestResponse::HideWindow)
    });
    // The sidebar is the window's: it leads the active pane (spec 10 §4.10).
    window.on_sidebar_clicked({
        let sidebar = sidebar.clone();
        move |section, index| {
            if section == sidebar::SECTION_SEARCHES {
                return crate::run_saved_search(index, false);
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
                return crate::run_saved_search(index, true);
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
        move || {
            with_window(id, |_| drags.attach_when_ready(0));
        }
    });
    // A pane's callbacks name it by its row: a click or an action there makes it the active
    // one first (`pick`); what it only tells goes to it as it is.
    window.on_row_menu({
        let menus = menus.clone();
        move |pane, i, x, y| {
            let Some(p) = crate::pick(pane) else { return };
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
            let Some(p) = crate::pick(pane) else { return };
            p.view.clear_selection();
            menus.background(x, y)
        }
    });
    window.on_item_pressed({
        let ops = ops.clone();
        move |pane, i, ctrl, shift| {
            ops.end_unfocused_rename();
            let Some(p) = crate::pick(pane) else { return };
            if let Ok(index) = usize::try_from(i) {
                p.view.press(index, ctrl, shift);
            }
        }
    });
    window.on_marquee(move |pane, x, y, width, height, additive| {
        if let Some(p) = crate::pick(pane) {
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
            let Some(p) = crate::pick(pane) else { return };
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
            slint::Timer::single_shot(std::time::Duration::ZERO, bind(move || menus.keyboard(i, x, y)));
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
        if let Some(p) = crate::pick(pane) {
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
            if crate::pick(pane).is_some() {
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
        if let Some(p) = crate::pick(pane) {
            p.view.toggle_group(usize::try_from(first).unwrap_or(usize::MAX));
        }
    });
    window.on_list_group_menu({
        let menus = menus.clone();
        move |pane, x, y| {
            if crate::pick(pane).is_some() {
                menus.group_header(x, y);
            }
        }
    });
    window.on_filter_menu({
        let menus = menus.clone();
        move |pane, left, bottom, right, top| {
            if crate::pick(pane).is_some() {
                menus.filter_menu(popup::Anchor::below(left, top, right, bottom));
            }
        }
    });
    window.on_search_menu({
        let menus = menus.clone();
        move |pane, which, left, bottom, right, top| {
            if crate::pick(pane).is_some() {
                let which = search::SearchMenu::from_index(which);
                menus.search_menu(which, popup::Anchor::below(left, top, right, bottom));
            }
        }
    });
    window.on_view_menu({
        let menus = menus.clone();
        move |pane, left, bottom, right, top| {
            if crate::pick(pane).is_some() {
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
            crate::pick(pane);
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
        if let Some(p) = crate::pick(pane) {
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
    window.on_tab_menu({
        let menus = menus.clone();
        move |pane, i, x, y| {
            if let (Some(_), Ok(i)) = (crate::pick(pane), usize::try_from(i)) {
                menus.tab(i, x, y);
            }
        }
    });

    // Double-click; with single-click-open the click already opened it.
    // Miller columns (spec 10 §7): a press in a column beside the focused one takes it there
    // first, so the press, drag and menu that follow are the focused column's (as a pane is
    // made active first, 10b); a middle-click, a scroll, the columns on screen.
    window.on_column_pressed({
        let (ops, drags) = (ops.clone(), drags.clone());
        move |pane, column, row, scroll, x, y, right| {
            ops.end_unfocused_rename();
            if let Some(index) = crate::pick(pane).and_then(|p| p.view.column_clicked(column, row, scroll)) {
                drags.down(index, x, y, right, true);
            }
        }
    });
    window.on_column_menu(move |pane, row, x, y| {
        let Some(p) = crate::pick(pane) else { return };
        match p.view.single_selected().filter(|_| row) {
            Some(index) => menus.row(i32::try_from(index).unwrap_or(-1), x, y),
            None => {
                p.view.clear_selection();
                menus.background(x, y);
            }
        }
    });
    window.on_column_middle(|pane, column, row| {
        let Some(p) = crate::pick(pane) else { return };
        if let (Ok(column), Ok(row)) = (usize::try_from(column), usize::try_from(row))
            && let Some((path, true)) = p.view.column_entry(column, row)
        {
            p.nav.open_tab(gezik_core::nav::Location::Path(path), false);
        }
    });
    window.on_column_scrolled(|pane, column, scroll| {
        panes::with_row(pane, |p| p.view.column_scrolled(column, scroll));
    });
    window.on_columns_shown(|pane, first, end| {
        panes::with_row(pane, |p| p.view.columns_on_screen(first, end));
    });
    window.on_open_row(|pane, i| {
        if let Some(p) = crate::pick(pane)
            && let Ok(index) = usize::try_from(i)
            && !view_options::current().single_click_open
        {
            crate::open_entry(&p.nav, &p.view, index);
        }
    });
    window.on_go_back(|pane| {
        if let Some(p) = crate::pick(pane) {
            p.nav.back();
        }
    });
    window.on_go_forward(|pane| {
        if let Some(p) = crate::pick(pane) {
            p.nav.forward();
        }
    });
    window.on_go_up(|pane| {
        if let Some(p) = crate::pick(pane) {
            p.nav.up();
        }
    });
    window.on_refresh(|pane| {
        if let Some(p) = crate::pick(pane) {
            p.nav.reload();
        }
    });
    window.on_navigate(|pane, text| {
        if let Some(p) = crate::pick(pane) {
            p.nav.navigate_text(text.into());
        }
    });
    window.on_crumb_clicked(|pane, i| {
        if let Some(p) = crate::pick(pane) {
            p.nav.crumb_clicked(i);
        }
    });
    // Slint passes indexes as `i32`: a negative one does nothing.
    window.on_tab_activate(|pane, i| {
        if let (Some(p), Ok(i)) = (crate::pick(pane), usize::try_from(i)) {
            p.nav.activate_tab(i);
        }
    });
    // Closed once the click is fully handled.
    window.on_tab_close(|pane, i| {
        if let (Some(p), Ok(i)) = (crate::pick(pane), usize::try_from(i)) {
            crate::close_tab_later(&p.nav, i);
        }
    });
    window.on_tab_new(|pane| {
        if let Some(p) = crate::pick(pane) {
            p.nav.open_tab(p.nav.start(), true);
        }
    });
    window.on_tab_move(|pane, from, to| {
        if let (Some(p), Ok(from), Ok(to)) = (crate::pick(pane), usize::try_from(from), usize::try_from(to)) {
            p.nav.move_tab(from, to);
        }
    });
    window.on_row_middle_clicked(|pane, i| {
        let Some(p) = crate::pick(pane) else { return };
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
            let used = crate::handle_key(
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

    hook(&window, id);

    // The second pane of last time, with its own tabs (its listing comes in the background).
    if restore && opening.dual {
        dual::open(Some(opening.active_pane));
        if opening.sync {
            dual::toggle_sync();
        }
    }
}

/// What the router keeps of its window between events.
struct Router {
    minimized: Cell<bool>,
    /// The pointer's last window position (logical pixels).
    pointer: Cell<(f32, f32)>,
    modifiers: Cell<ModifiersState>,
    /// The window's own frame limit (deviation 14).
    frames: RefCell<FrameLimit>,
}

/// Puts the router on window `id`'s events; it holds the window weakly.
fn hook(window: &AppWindow, id: WindowId) {
    let weak = window.as_weak();
    let router = Router {
        minimized: Cell::new(false),
        pointer: Cell::new((0.0, 0.0)),
        modifiers: Cell::new(ModifiersState::empty()),
        frames: RefCell::new(FrameLimit::new(0)),
    };
    window.window().on_winit_window_event(move |slint_window, event| route(id, &weak, &router, slint_window, event));
}

/// Window `id`'s winit event, before Slint handles it: from here to the next event, `id` is
/// the [`current`] window (Slint's callbacks for the event run after this returns).
fn route(
    id: WindowId,
    weak: &slint::Weak<AppWindow>,
    router: &Router,
    slint_window: &slint::Window,
    event: &winit::event::WindowEvent,
) -> EventResult {
    acting(id);
    if let winit::event::WindowEvent::Focused(focused) = event {
        // macOS: winit's value can be wrong (winit #4371); Slint reads the window too.
        let focused = if cfg!(target_os = "macos") {
            slint_window.with_winit_window(|native| native.has_focus()).unwrap_or(*focused)
        } else {
            *focused
        };
        if focused {
            self::focused(id);
        }
    }
    if let winit::event::WindowEvent::ModifiersChanged(m) = event {
        router.modifiers.set(m.state());
    }
    // The keypad's keys and Ctrl+Shift+digits, which Slint's text cannot tell apart
    // (keys.rs `Physical`), and AltGr on a key it types nothing with (keys.rs
    // `altgr_blank`): noted before Slint hands the key to `key-event`.
    if let winit::event::WindowEvent::KeyboardInput { event, .. } = event {
        keys::note_key(event);
        let m = router.modifiers.get();
        let dead = event.state == winit::event::ElementState::Pressed
            && matches!(event.logical_key, winit::keyboard::Key::Dead(_));
        if let Some(digit) =
            keys::dead_digit(keys::physical_of_event(event), dead, m.control_key() || m.super_key(), m.alt_key())
        {
            let text = slint::SharedString::from(digit.to_string());
            slint_window.dispatch_event(slint::platform::WindowEvent::KeyPressed { text: text.clone() });
            slint_window.dispatch_event(slint::platform::WindowEvent::KeyReleased { text });
            return EventResult::PreventDefault;
        }
    }
    if let winit::event::WindowEvent::Focused(false) = event {
        keys::forget_altgr();
    }
    if let winit::event::WindowEvent::Focused(true) = event {
        crate::operations::with_current(crate::operations::Operations::clipboard_check);
    }
    if let winit::event::WindowEvent::DroppedFile(path) = event {
        drag::with_current(|drags| drags.dropped_file(path.clone()));
    }
    // Windows drops a minimized window's picture, but the size on restore is the old
    // one, so Slint redraws only what changed and the rest of the window stays empty.
    if let winit::event::WindowEvent::Resized(size) = event {
        let zero = size.width == 0 || size.height == 0;
        if !zero && router.minimized.get() {
            let weak = weak.clone();
            slint::Timer::single_shot(std::time::Duration::ZERO, move || {
                if let Some(window) = weak.upgrade() {
                    theme_bridge::repaint_all(&window);
                }
            });
        }
        router.minimized.set(zero);
    }
    // Caps the frame rate at `max-fps`: Slint draws as often as the display refreshes.
    if let winit::event::WindowEvent::RedrawRequested = event {
        frame_limit::wait_for_frame(&router.frames);
        return EventResult::Propagate;
    }
    // A drag that lost its pointer grab to a tab switch follows the window's events.
    if let winit::event::WindowEvent::CursorMoved { position, .. } = event {
        let scale = weak.upgrade().map_or(1.0, |w| w.window().scale_factor());
        let at = position.to_logical::<f32>(f64::from(scale));
        drag::with_current(|drags| drags.window_pointer_moved(at.x, at.y));
        router.pointer.set((at.x, at.y));
    }
    if let winit::event::WindowEvent::MouseInput {
        state: winit::event::ElementState::Released,
        button: button @ (winit::event::MouseButton::Left | winit::event::MouseButton::Right),
        ..
    } = event
    {
        let (x, y) = router.pointer.get();
        // A left release goes on to Slint (its grab points at an entry that is gone);
        // a right one would open a second menu after the drop's (see drag.rs).
        let right = *button == winit::event::MouseButton::Right;
        if drag::with_current(|drags| drags.window_released(x, y, right)) == Some(true) {
            return EventResult::PreventDefault;
        }
    }
    if let winit::event::WindowEvent::MouseInput { state: winit::event::ElementState::Pressed, button, .. } = event {
        match button {
            winit::event::MouseButton::Back => panes::active_nav().back(),
            winit::event::MouseButton::Forward => panes::active_nav().forward(),
            _ => {}
        }
    }
    EventResult::Propagate
}

#[cfg(test)]
pub(crate) mod tests {
    use std::rc::Rc;

    use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
    use slint::platform::{Platform, WindowAdapter};

    use super::*;
    use crate::Theme;

    /// Slint without a screen (deviation 18): each window gets a window of its own in memory.
    struct Headless;

    impl Platform for Headless {
        fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
            Ok(MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer))
        }
    }

    /// Sets the headless platform up on this test's thread (Slint's context is the thread's).
    pub(crate) fn platform() {
        let _ = slint::platform::set_platform(Box::new(Headless));
    }

    /// A window put in the register, as main puts the first one.
    fn window() -> (WindowId, slint::Weak<AppWindow>) {
        platform();
        let window = AppWindow::new().expect("a headless window");
        let weak = window.as_weak();
        (install(window), weak)
    }

    #[test]
    fn the_most_recently_focused_window_is_in_front() {
        let (a, _) = window();
        let (b, _) = window();
        assert_eq!(focus_order(), [b, a], "a new window comes in front");
        focused(a);
        assert_eq!(focus_order(), [a, b]);
        assert_eq!(current(), a, "no event: the most recently focused one");
        assert_eq!(with_focused(|_| current()), Some(a));
    }

    #[test]
    fn the_window_an_event_is_for_is_the_current_one() {
        let (a, _) = window();
        let (b, _) = window();
        focused(a);
        assert_eq!(enter(b, current), b);
        assert_eq!(current(), a, "back to the front one after it");
        // The router makes it current until the next event.
        acting(b);
        assert_eq!(current(), b);
        assert_eq!(enter(a, || enter(b, current)), b, "nested: the inner one");
        assert_eq!(current(), b);
        // A timer or a background result made in `b` runs in `b`, wherever it is called.
        let ran = Rc::new(Cell::new(None));
        let later = {
            let ran = ran.clone();
            bind(move || ran.set(Some(current())))
        };
        enter(a, later);
        assert_eq!(ran.get(), Some(b));
        let mut seen = Vec::new();
        for_each(|_| seen.push(current()));
        assert_eq!(seen, [a, b]);
    }

    #[test]
    fn a_forgotten_window_keeps_nothing() {
        let (a, weak) = window();
        let (b, _) = window();
        let pane = enter(a, crate::panes::next_id);
        enter(b, crate::panes::next_id);
        crate::window_state::PENDING.with(|p| p.set(a, (1, 2, 3)));
        assert_eq!(crate::panes::PANES.with(Slot::len), 2);
        acting(a);
        forget(a);
        assert!(with_window(a, |_| ()).is_none(), "its results and timers are dropped");
        assert_eq!(current(), b, "an event of a closed window acts on nothing of it");
        assert_eq!(focus_order(), [b]);
        assert_eq!(crate::panes::PANES.with(Slot::len), 1, "its panes are gone");
        assert!(crate::panes::data(pane).is_none());
        assert_eq!(crate::window_state::PENDING.with(Slot::len), 0);
        assert!(weak.upgrade().is_none(), "no strong handle is left: its frame buffer goes");
    }

    #[test]
    fn a_slot_keeps_a_value_per_window() {
        let slot: Slot<u32> = Slot::new();
        let (a, b) = (WindowId(7), WindowId(8));
        slot.set(a, 1);
        slot.set(b, 2);
        slot.set(a, 3);
        assert_eq!((slot.get(a), slot.get(b), slot.len()), (Some(3), Some(2), 2));
        assert_eq!(enter(b, || slot.current()), Some(2));
        assert_eq!(enter(WindowId(9), || slot.current_or(|| 5)), 5);
        slot.forget(a);
        assert_eq!((slot.get(a), slot.len()), (None, 2));
    }

    /// Deviation 2: every slot is forgotten when its window closes, none is left out of
    /// `FORGETS` (a slot is a `static …: Slot<…>` or `static …: crate::windows::Slot<…>`).
    #[test]
    fn every_slot_is_forgotten() {
        fn count(dir: &std::path::Path) -> usize {
            let mut slots = 0;
            for entry in std::fs::read_dir(dir).expect("the source folder") {
                let path = entry.expect("an entry").path();
                if path.is_dir() {
                    slots += count(&path);
                } else if path.extension().is_some_and(|e| e == "rs") && !path.ends_with("windows.rs") {
                    let text = std::fs::read_to_string(&path).expect("a source file");
                    slots += text
                        .lines()
                        .filter(|line| {
                            let line = line.trim_start();
                            (line.starts_with("static ") || line.starts_with("pub(crate) static "))
                                && (line.contains(": Slot<") || line.contains(": crate::windows::Slot<"))
                        })
                        .count();
                }
            }
            slots
        }
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        assert_eq!(count(&src), FORGETS.len());
    }

    /// Spec 9.3 (decision 46): a settings change shows alike in every window.
    #[test]
    fn every_window_shows_the_settings_alike() {
        let (a, first) = window();
        let (b, second) = window();
        let mut files = gezik_config::store::ConfigFiles::default();
        files.warnings.push(gezik_config::Warning::new("settings.toml", "bad value"));
        let mut loaded = gezik_config::store::resolve(&files, true);
        loaded.settings.sidebar = gezik_config::settings::SidebarPosition::Right;
        let theme = loaded.theme.clone().expect("the built-in theme");
        for_each(|window| crate::apply_to_window(window, &loaded));
        let shown = |weak: &slint::Weak<AppWindow>| {
            let window = weak.upgrade().expect("open");
            let background = window.global::<Theme>().get_background();
            (background, window.get_notice(), window.get_sidebar_position(), window.get_mono_font())
        };
        let (one, two) = (shown(&first), shown(&second));
        assert_eq!(one, two);
        let c = theme.colors.background;
        assert_eq!(one.0, slint::Color::from_argb_u8(c.a, c.r, c.g, c.b));
        assert_eq!(one.1.as_str(), "settings.toml: bad value");
        assert_eq!((one.2, one.3.as_str()), (1, crate::mono_font()));
        let _ = (a, b);
    }
}
