//! The second pane (spec 10 §4): opening and closing it, which pane is active (one folder
//! watcher, the active pane's), and what state.toml keeps of them.

use std::cell::{Cell, RefCell};
use std::path::{Component, Path, PathBuf};

use gezik_config::settings_writer::SettingsChange;
use gezik_config::store::ConfigStore;
use gezik_core::drag::Effect;
use gezik_core::nav::{Location, Session, TRASH_NAME};
use gezik_core::ops::paths::{is_within, same_path};
use slint::ComponentHandle;

use crate::AppWindow;
use crate::panes::{self, Pane, PaneId};

/// Narrower than this the second pane does not open (spec 10 §4.1).
const MIN_WIDTH: f32 = 640.0;

#[derive(Default)]
struct Dual {
    window: slint::Weak<AppWindow>,
    /// Where state.toml is; none for a second window (it writes no state) or without a config
    /// folder.
    store: Option<ConfigStore>,
    /// `[session] restore`: off, state.toml keeps no tabs of either pane.
    restore: bool,
    /// The right pane's tabs while it is closed (none before it was first opened).
    right: Option<Session>,
    /// The left pane's share of the two panes' width (0.2–0.8).
    split: f32,
    /// settings.toml, for Don't Ask Again.
    config: Option<ConfigStore>,
    /// When the F3 hint showed (Unix seconds), while it may show once more.
    f3_at: Option<u64>,
}

thread_local! {
    static DUAL: RefCell<Dual> = RefCell::default();
    /// `[panes] confirm`: F5/F6 ask first.
    static CONFIRM: Cell<bool> = const { Cell::new(true) };
}

/// The F3 hint's words (spec 10 §10.2).
const F3_HINT: &str = "F3 now opens a second pane; search is Ctrl+Shift+F or Ctrl+E";

/// How long after the F3 hint opening the second pane with F3 says it once more.
const F3_AGAIN_FOR: u64 = 7 * 24 * 60 * 60;

fn window() -> Option<AppWindow> {
    DUAL.with(|d| d.borrow().window.upgrade())
}

/// Call once the left pane is installed: `store` writes state.toml (none in a second window),
/// `config` settings.toml; `right` is the right pane's tabs of last time, `split` the left
/// pane's share in thousandths.
pub fn install(
    window: &AppWindow,
    (store, config): (Option<ConfigStore>, Option<ConfigStore>),
    restore: bool,
    right: Session,
    split: Option<u16>,
) {
    let right = (!right.is_empty()).then_some(right);
    let split = split.map_or(0.5, |split| f32::from(split) / 1000.0);
    window.set_pane_split(split * 100.0);
    window.set_right_split(100.0 - split * 100.0);
    DUAL.with(|d| {
        *d.borrow_mut() = Dual { window: window.as_weak(), store, restore, right, split, config, f3_at: None }
    });
}

/// `[panes] confirm` changed.
pub fn set_confirm(confirm: bool) {
    CONFIRM.with(|c| c.set(confirm));
}

/// `[session] restore` changed: off, state.toml forgets the right pane's tabs too.
pub fn set_restore(restore: bool) {
    let changed = DUAL.with(|d| std::mem::replace(&mut d.borrow_mut().restore, restore) != restore);
    if changed {
        save();
    }
}

pub fn is_open() -> bool {
    panes::count() > 1
}

/// `toggle-dual-pane`.
pub fn toggle() {
    if is_open() { close() } else { open(None) }
}

/// Opens the right pane with the tabs it had when it closed (else at the left pane's folder);
/// it becomes the active one, unless `start` (the session of last time) says which is.
pub fn open(start: Option<usize>) {
    let Some(window) = window() else { return };
    if is_open() {
        return;
    }
    let scale = window.window().scale_factor();
    if start.is_none() && (window.window().size().width as f32 / scale) < MIN_WIDTH {
        window.set_view_two_panes(false);
        return panes::active_view().note("The window is too narrow for two panes".to_owned());
    }
    let left = panes::active();
    let session = DUAL.with(|d| d.borrow_mut().right.take());
    let session = session.unwrap_or_else(|| Session::single(left.nav.active_location()));
    let right = make(&left, session.clone());
    connect(&right);
    panes::install(right.clone());
    let (store, restore) = DUAL.with(|d| (d.borrow().store.clone(), d.borrow().restore));
    if let Some(store) = store {
        right.nav.keep_session(session, restore, move |session| {
            let session = session.clone();
            store.update_state(move |state| state.right_session = session);
        });
    }
    right.nav.install();
    window.set_dual(true);
    window.set_view_two_panes(true);
    activate(start.unwrap_or(1));
    save();
}

/// Closes the right pane: everything of it goes but its tabs (kept here and in state.toml).
/// Its locked tabs do not keep it open.
pub fn close() {
    let Some(window) = window() else { return };
    let Some(right) = panes::at(1) else { return };
    activate(0);
    let session = release(right.id);
    DUAL.with(|d| d.borrow_mut().right = session);
    window.set_dual(false);
    window.set_view_two_panes(false);
    // A field of the closed pane may have had the keyboard: it went with the pane.
    window.invoke_focus_list();
    save();
}

/// Takes pane `id` out after stopping what it runs: its search, folder sizes, watcher and
/// media requests. Its tabs.
fn release(id: PaneId) -> Option<Session> {
    let pane = panes::with_id(id, Pane::clone)?;
    pane.search.leaving();
    pane.folder_sizes.shown(&Location::Drives);
    pane.nav.unwatch();
    pane.view.clear();
    let session = pane.nav.session();
    drop(pane);
    panes::remove(id);
    Some(session)
}

/// A pane like `like` (its settings, media, folder view memory, dialogs, history) showing
/// `session`'s tabs; it loads nothing until its navigator is installed.
fn make(like: &Pane, session: Session) -> Pane {
    let id = panes::next_id();
    let view = like.view.for_pane(id);
    let nav = like.nav.for_pane(id, view.clone(), session);
    Pane {
        id,
        folder_sizes: like.folder_sizes.for_pane(id, view.clone()),
        path_box: like.path_box.for_pane(id, nav.clone()),
        filter: like.filter.for_pane(id, view.clone()),
        search: like.search.for_pane(id, nav.clone(), view.clone()),
        nav,
        view,
    }
}

/// What a pane tells the window's parts: its search follows its location; the sidebar, the
/// preview and the file operations hear only the active pane.
pub fn connect(pane: &Pane) {
    let id = pane.id;
    pane.nav.on_changed(move |location| {
        panes::with_id(id, |p| p.search.location_changed(location));
        if panes::is_active(id) {
            crate::sidebar::with_current(|s| s.follow(location));
        }
    });
    pane.view.on_selection_changed(move || {
        if panes::is_active(id) {
            crate::preview::with_current(crate::preview::Preview::schedule);
        }
    });
    pane.view.on_shown(move || {
        if panes::is_active(id) {
            crate::operations::with_current(crate::operations::Operations::shown);
        }
    });
}

/// Makes the pane at place `index` the active one: a rename or address typed in the other one
/// ends, the watcher moves over and the newly active pane's folder is read again (quietly);
/// the window's title, status line, sidebar and preview follow it.
pub fn activate(index: usize) {
    if index == panes::active_index() || index >= panes::count() {
        return;
    }
    crate::operations::with_current(crate::operations::Operations::end_rename_for_switch);
    let old = panes::active();
    if panes::mirror(old.id).path_editing.get() {
        panes::edit(old.id, |d| panes::path_editing(d, false));
    }
    old.nav.unwatch();
    panes::set_active(index);
    let pane = panes::active();
    if let Some(window) = window() {
        window.set_active_pane(i32::try_from(index).unwrap_or(0));
        window.set_address_box(panes::mirror(pane.id).geometry.borrow().clone());
    }
    pane.nav.update_chrome();
    pane.view.update_status();
    pane.nav.rewatch();
    crate::preview::with_current(crate::preview::Preview::schedule);
}

/// A click or an action in the pane on row `row` (a Slint callback's) makes it the active one.
pub fn pick(row: i32) {
    if let Ok(index) = usize::try_from(row) {
        activate(index);
    }
}

/// `focus-other-pane`: false (the key goes on) with one pane.
pub fn focus_other() -> bool {
    if !is_open() {
        return false;
    }
    activate(1 - panes::active_index());
    true
}

/// The splitter between the panes was dragged to `share` of their room.
pub fn split_moved(share: f32) {
    let split = share.clamp(0.2, 0.8);
    DUAL.with(|d| d.borrow_mut().split = split);
    if let Some(window) = window() {
        window.set_pane_split(split * 100.0);
        window.set_right_split(100.0 - split * 100.0);
    }
}

/// A double-click on the splitter: the panes share the room evenly.
pub fn split_reset() {
    split_moved(0.5);
    split_done();
}

/// The splitter was let go: state.toml keeps the share.
pub fn split_done() {
    let (store, split) = DUAL.with(|d| (d.borrow().store.clone(), d.borrow().split));
    let split = (split * 1000.0).round() as u16;
    if let Some(store) = store {
        store.update_state(move |state| state.pane_split = (split != 500).then_some(split));
    }
}

/// Two panes and the preview came down to their narrowest: the preview closes first.
pub fn cramped() {
    crate::preview::with_current(|preview| {
        if preview.is_pane_open() {
            preview.set_pane_open(false);
            panes::active_view().note("The preview pane closed to keep room for two panes".to_owned());
        }
    });
}

/// Writes whether the second pane is open and which pane is active, and the right pane's
/// tabs while it is closed; with `[session] restore` off, none of them. Also at quit: a pane
/// switch writes nothing by itself.
pub fn save() {
    let (store, restore, right) = DUAL.with(|d| {
        let d = d.borrow();
        (d.store.clone(), d.restore, d.right.clone())
    });
    let Some(store) = store else { return };
    let open = is_open();
    let (dual, active) = if restore { (open, panes::active_index()) } else { (false, 0) };
    store.update_state(move |state| {
        state.dual = dual;
        state.active_pane = active;
        if !open {
            state.right_session = right.filter(|_| restore).unwrap_or_default();
        }
    });
}

/// `copy-to-other-pane` / `move-to-other-pane` (F5 / F6, spec 10 §4.4): the active pane's
/// selection (else its focused item) into the other pane's folder, asked first unless
/// `[panes] confirm` is off. The job is a drop's: conflict list, panel, undo; no drive rule.
/// Out of the trash, F6 puts the items back into that folder.
pub fn to_other(moving: bool) {
    let source = panes::active_view();
    let Some(other) = panes::at(i32::from(panes::active_index() == 0)) else {
        return source.note("There is no other pane".to_owned());
    };
    let verb = if moving { "Move" } else { "Copy" };
    let Some(base) = other.view.folder() else {
        return source.note(if moving && other.view.shows_trash() {
            format!("Use Delete to move items to the {TRASH_NAME}")
        } else {
            format!("The other pane shows no folder to {} into", verb.to_lowercase())
        });
    };
    if source.folder().is_some_and(|folder| same_path(&folder, &base)) {
        return source.note("Both panes show the same folder".to_owned());
    }
    // The trash's rows are entries in the bins, with the names they had.
    let trash = source.shows_trash().then(|| source.selected_trash());
    let (paths, names): (Vec<PathBuf>, Vec<PathBuf>) = match &trash {
        Some(rows) => rows.iter().map(|(entry, label)| (entry.clone(), PathBuf::from(&*label.name))).unzip(),
        None => source.selected_entries().into_iter().map(|(path, _)| (path.clone(), path)).unzip(),
    };
    if paths.is_empty() {
        return;
    }
    let shown = base.clone();
    let go = move |target: PathBuf| {
        crate::operations::with_current(|ops| match trash {
            Some(rows) => {
                let rows = rows.into_iter().map(|(entry, label)| (entry, label.name.into())).collect();
                ops.restore_from_trash(crate::trash_view::into_folder(&target, rows));
            }
            None if same_path(&target, &shown) => {
                ops.transfer(paths, target, if moving { Effect::Move } else { Effect::Copy });
            }
            None => {
                let anchor = if is_within(&target, &shown) { shown } else { target.clone() };
                ops.transfer_making(paths, &target, anchor, moving);
            }
        });
    };
    if !CONFIRM.with(Cell::get) {
        return go(base);
    }
    // shortcut: the typed folder is not looked up on disk (no stat on the UI thread): a file by
    // that name fails in the job; check it off the thread if that confuses.
    let title = format!("{verb} {} to {}?", crate::operations::items_text(&names), base.display());
    let message = "Into this folder (a relative path is under it; a missing folder is made):";
    let typed_base = base.clone();
    let note = move |typed: &str| match typed_target(&typed_base, typed) {
        None => ("Type a folder".to_owned(), true),
        Some(target) if same_path(&target, &typed_base) => (String::new(), false),
        Some(target) => (format!("Into {}", target.display()), false),
    };
    let field = base.display().to_string();
    let typed_base = base;
    crate::operations::with_current(|ops| {
        ops.dialogs().ask_text_choice(
            title,
            message,
            field,
            &[verb, "Don't Ask Again", "Cancel"],
            note,
            move |answer| {
                let Some((choice, typed)) = answer else { return };
                if choice == 1 {
                    dont_ask_again();
                }
                match typed_target(&typed_base, &typed) {
                    Some(target) => go(target),
                    None => panes::active_view().note("No folder was typed".to_owned()),
                }
            },
        );
    });
}

/// The F5/F6 question's Don't Ask Again: `[panes] confirm = false`, now and in settings.toml.
fn dont_ask_again() {
    set_confirm(false);
    let Some(config) = DUAL.with(|d| d.borrow().config.clone()) else { return };
    config.write_settings(SettingsChange::PanesConfirm(false), |result| {
        if let Err(warning) = result {
            let _ = slint::invoke_from_event_loop(move || panes::active_view().note(warning.to_string()));
        }
    });
}

/// The folder typed into the F5/F6 question: a relative path is under `base`, `.` and `..`
/// are worked out. `None` when nothing is typed or the path has no root (Windows `D:x`).
pub fn typed_target(base: &Path, typed: &str) -> Option<PathBuf> {
    let typed = typed.trim();
    if typed.is_empty() {
        return None;
    }
    let mut target = PathBuf::new();
    for part in base.join(typed).components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                if target.file_name().is_some() {
                    target.pop();
                }
            }
            part => target.push(part.as_os_str()),
        }
    }
    target.is_absolute().then_some(target)
}

/// At start (Windows and Linux, `[shortcuts]` naming neither search nor toggle-dual-pane):
/// says once that F3 moved, and keeps when (state.toml `[hints]`, spec 10 §10.2).
pub fn f3_hint_at_start(applies: bool, shown: bool, shown_at: Option<u64>) {
    if shown || !applies {
        return DUAL.with(|d| d.borrow_mut().f3_at = shown_at);
    }
    let now = now_secs();
    panes::active_view().note(F3_HINT.to_owned());
    DUAL.with(|d| d.borrow_mut().f3_at = Some(now));
    let store = DUAL.with(|d| d.borrow().store.clone());
    if let Some(store) = store {
        store.update_state(move |state| (state.f3_moved, state.f3_moved_at) = (true, Some(now)));
    }
}

/// F3 opened the second pane: within a week of the hint it says it once more (no timer: the
/// date is looked at only now).
pub fn f3_pressed() {
    let Some(at) = DUAL.with(|d| d.borrow().f3_at) else { return };
    if !f3_again(at, now_secs()) {
        return;
    }
    panes::active_view().note(F3_HINT.to_owned());
    DUAL.with(|d| d.borrow_mut().f3_at = None);
    let store = DUAL.with(|d| d.borrow().store.clone());
    if let Some(store) = store {
        store.update_state(|state| state.f3_moved_at = None);
    }
}

/// Whether the F3 hint shown at `at` says it again at `now`.
fn f3_again(at: u64, now: u64) -> bool {
    now.checked_sub(at).is_some_and(|since| since < F3_AGAIN_FOR)
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::Model;

    /// A pane as main.rs makes the left one, with no window.
    fn test_pane() -> Pane {
        let id = panes::next_id();
        let window = slint::Weak::default();
        let dialogs = crate::dialog::Dialogs::detached();
        let memory = std::rc::Rc::default();
        let view = crate::view::View::new(id, window.clone(), crate::media::Media::idle(), memory, None);
        let drives = Session::single(Location::Drives);
        let nav =
            crate::navigation::Navigator::new(id, window.clone(), view.clone(), drives, Vec::new(), Location::Drives);
        Pane {
            id,
            folder_sizes: crate::folder_sizes::FolderSizes::new(id, window.clone(), view.clone()),
            path_box: crate::path_box::PathBox::new(id, window.clone(), nav.clone(), None, Vec::new()),
            filter: crate::filter::Filter::new(id, window.clone(), view.clone(), dialogs.clone(), None),
            search: crate::search::Searches::new(id, window, nav.clone(), view.clone(), dialogs),
            nav,
            view,
        }
    }

    #[test]
    fn a_closed_pane_lets_go_of_everything_but_its_tabs() {
        let left = test_pane();
        connect(&left);
        panes::install(left.clone());
        let mut tabs = Session::single(Location::Trash);
        tabs.tabs.push(gezik_core::nav::SessionTab { location: Location::Drives, locked: true });
        let right = make(&left, tabs.clone());
        connect(&right);
        panes::install(right.clone());
        assert_eq!(panes::count(), 2);
        let (view, nav, id) = (right.view.downgrade(), right.nav.downgrade(), right.id);
        drop(right);
        // Locked tabs do not keep it open; its tabs are what is left of it.
        assert_eq!(release(id), Some(tabs));
        assert!(view.upgrade().is_none(), "the right pane's view is gone");
        assert!(nav.upgrade().is_none(), "the right pane's navigator is gone");
        assert_eq!(panes::count(), 1);
        assert_eq!(panes::model().row_count(), 1, "its row is gone too");
        assert_eq!(panes::active_id(), Some(left.id));
    }

    /// The window's keys follow how many panes are open (spec 10 §10.2, `handle_key`'s lookup).
    #[test]
    fn the_keys_are_the_pane_actions_only_with_two_panes() {
        use gezik_config::shortcuts::{Action, Platform, Shortcuts, parse_chord};
        crate::keys::set_shortcuts(Shortcuts::defaults(Platform::Other));
        let key = |text: &str| crate::keys::action_for(&parse_chord(text, Platform::Other).unwrap().unwrap());
        let left = test_pane();
        panes::install(left.clone());
        assert_eq!((key("f5"), key("f6"), key("tab")), (Some(Action::Refresh), None, None));
        let right = make(&left, Session::single(Location::Drives));
        panes::install(right.clone());
        assert_eq!(key("f5"), Some(Action::CopyToOtherPane));
        assert_eq!(key("f6"), Some(Action::MoveToOtherPane));
        assert_eq!(key("tab"), Some(Action::FocusOtherPane));
        assert_eq!(key("ctrl+r"), Some(Action::Refresh));
        assert_eq!(key("ctrl+e"), Some(Action::Search));
        assert_eq!(key("f3"), Some(Action::ToggleDualPane));
        release(right.id);
        assert_eq!(key("f5"), Some(Action::Refresh), "closed: F5 refreshes again");
    }

    #[test]
    fn a_typed_folder_is_under_the_other_panes_folder() {
        let base = std::env::temp_dir().join("Yedek");
        assert_eq!(typed_target(&base, "  "), None);
        assert_eq!(typed_target(&base, &base.display().to_string()), Some(base.clone()));
        assert_eq!(typed_target(&base, "new/sub"), Some(base.join("new").join("sub")));
        assert_eq!(typed_target(&base, "./a/../b"), Some(base.join("b")));
        assert_eq!(typed_target(&base, ".."), base.parent().map(Path::to_path_buf));
        let elsewhere = std::env::temp_dir().join("elsewhere");
        assert_eq!(typed_target(&base, &elsewhere.display().to_string()), Some(elsewhere));
        if cfg!(windows) {
            assert_eq!(typed_target(&base, "D:x"), None, "no root: not a folder");
            assert_eq!(typed_target(Path::new("C:\\"), "..\\.."), Some(PathBuf::from("C:\\")));
        }
    }

    #[test]
    fn the_f3_hint_says_it_again_only_within_a_week() {
        let at = 1_800_000_000;
        assert!(f3_again(at, at));
        assert!(f3_again(at, at + F3_AGAIN_FOR - 1));
        assert!(!f3_again(at, at + F3_AGAIN_FOR));
        assert!(!f3_again(at, at - 1), "a clock set back says nothing");
    }
}
