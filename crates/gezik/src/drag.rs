//! Dragging files with the mouse. Inside the window the drag is Gezik's own: Slint reports
//! presses, moves and releases on entries and draws the dragged items; this finds the drop
//! target from the window geometry (`gezik_core::drag`) and drops through the engine, so a
//! drop is a job like any other and Ctrl+Z undoes it.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use gezik_core::drag::{
    self, Action, Allowed, CrumbArea, Effect, Hit, Keys, Layout, ListArea, PaneArea, SideRow, SidebarArea, TabArea,
};
use gezik_core::layout::Rect;
use gezik_core::nav::Location;
use gezik_core::ops::paths::is_within;
use gezik_platform::DriveKind;
use gezik_platform::dnd::{Answer, Attached, DragEnd, DropHandler, Handoff, Offer, OnEnd, OutsideDrag, VirtualFiles};
use slint::{ComponentHandle, Model, Timer, TimerMode};

use crate::context_menu::Menus;
use crate::operations::Operations;
use crate::sidebar::{SECTION_GROUP, SECTION_PINNED, SECTION_TREE_MORE, Sidebar};
use crate::{AppWindow, SidebarRow, Theme};

/// How often a drag near the list's top or bottom edge scrolls it.
const SCROLL_STEP: Duration = Duration::from_millis(30);

/// Where a drop would land and what it would do.
#[derive(Debug, Clone, PartialEq)]
struct Target {
    hit: Hit,
    /// The folder it would go into (none for pinning, or where nothing can be dropped).
    dir: Option<PathBuf>,
    /// None: the drop is refused.
    action: Option<Action>,
    /// The zip, 7z or tar file it would be added to (`Action::AddToArchive`).
    archive: Option<PathBuf>,
}

impl Target {
    /// Nothing can be dropped there.
    fn none(hit: Hit) -> Target {
        Target { hit, dir: None, action: None, archive: None }
    }
}

/// A drag under way.
#[derive(Clone)]
struct Dragging {
    sources: Vec<PathBuf>,
    /// Every source is a folder, so they can be pinned.
    all_dirs: bool,
    right: bool,
    keys: Keys,
    allowed: Allowed,
    /// The pointer, in window coordinates.
    x: f32,
    y: f32,
    target: Option<Target>,
    /// The entry whose press started it (that press waits for the release).
    pressed: Option<usize>,
    /// From the drop stack: the strip takes none of it back.
    from_stack: bool,
    /// From another program, items with no file behind them (`sources` is empty): this many.
    virtual_count: usize,
    /// Those items, once dropped.
    virtual_files: Option<VirtualFiles>,
}

enum Phase {
    Idle,
    /// A button went down on the drop stack's strip (an item, or all of them); a drag starts
    /// once the pointer moves far enough.
    StackArmed {
        items: Vec<(PathBuf, bool)>,
        x: f32,
        y: f32,
    },
    /// A button went down on an entry; a drag starts once the pointer moves far enough.
    Armed {
        index: usize,
        x: f32,
        y: f32,
        right: bool,
        can_drag: bool,
    },
    Dragging(Dragging),
    /// Files from another program are over the window (no ghost: the system draws them).
    Offer(Dragging),
    /// Gezik's drag left the window and the system has it now.
    Outside(Dragging),
    /// Ended (Esc) while the button is still down: its release is no click.
    Ended,
}

/// The system's drag image of files from another program says what a drop would do on
/// Windows and Linux (`Answer::folder`); macOS has no such text, so Gezik shows it.
const OFFER_LABEL: bool = cfg!(target_os = "macos");

/// A drag handed to a system that runs it by itself (Wayland, macOS): what its end must
/// undo, even if the drag came back over Gezik's window as an offer meanwhile.
#[derive(Clone, Copy)]
struct Handed {
    /// The entry whose press started it.
    pressed: Option<usize>,
    right: bool,
    /// Where the pointer was when it left the window.
    x: f32,
    y: f32,
}

struct Inner {
    window: slint::Weak<AppWindow>,
    sidebar: Sidebar,
    ops: Operations,
    menus: Menus,
    phase: RefCell<Phase>,
    /// Where each pane's address bar parts are (window x, width), as Slint last reported them.
    crumbs: RefCell<[Vec<(String, f32, f32)>; 2]>,
    /// The tab under the pointer (pane, tab), waiting to open.
    hover_tab: Cell<Option<(usize, usize)>>,
    tab_timer: Timer,
    scroll_timer: Timer,
    /// The window's drop target for other programs.
    attached: RefCell<Option<Attached>>,
    /// A drag outside the window that Gezik drives itself (X11).
    outside: RefCell<Option<Box<dyn OutsideDrag>>>,
    /// Handing this drag to the system failed: it stays in the window.
    handoff_failed: Cell<bool>,
    /// A tab opened under the drag: the list was rebuilt, and the entry that held the
    /// pointer grab with it, so the pointer is followed from the window's own events.
    grab_lost: Cell<bool>,
    handed: Cell<Option<Handed>>,
    /// When the last click that opened an entry (single-click-open) came up: the second click
    /// of a double-click must not open again.
    last_open: Cell<Option<std::time::Instant>>,
    /// Files winit reported dropped (an X11 source that ignores `XdndProxy`), gathered until
    /// the batch ends.
    dropped_files: RefCell<Vec<PathBuf>>,
}

thread_local! {
    static CURRENT: RefCell<Option<Drags>> = const { RefCell::new(None) };
}

/// Runs `f` with this UI thread's drags, if there are any yet.
pub fn with_current<R>(f: impl FnOnce(&Drags) -> R) -> Option<R> {
    CURRENT.with(|c| c.borrow().clone()).map(|drags| f(&drags))
}

/// Which of Copy, Move and Link the right-drag menu offers: what the source allows, in a
/// writable folder, and not refused for these sources.
fn menu_effects(allowed: Allowed, writable: bool, sources: &[PathBuf], dir: &Path) -> (bool, bool, bool) {
    let can = |effect, ok: bool| ok && writable && !drag::refuse(sources, dir, effect);
    (can(Effect::Copy, allowed.copy), can(Effect::Move, allowed.move_), can(Effect::Link, allowed.link))
}

/// How a sidebar row takes a drop: headings take nothing (those of the pinned part pin there),
/// pinned rows have their pin lines, a capped branch's "… n more" line takes nothing either, the
/// rest (tree folders too) are folders.
fn side_row(row: &SidebarRow) -> SideRow {
    match (row.header, row.section) {
        (true, SECTION_PINNED | SECTION_GROUP) => SideRow::PinHeader,
        (true, _) => SideRow::Header,
        (false, SECTION_PINNED) => SideRow::Pinned,
        (false, SECTION_TREE_MORE) => SideRow::Header,
        _ => SideRow::Item,
    }
}

/// The keys held, as this system's file manager reads them (`drag::keys_of`). Slint reports
/// macOS's Command key as `control`.
fn keys(shift: bool, ctrl: bool, alt: bool) -> Keys {
    match drag::DragOs::current() {
        drag::DragOs::Mac => drag::keys_of(drag::DragOs::Mac, shift, false, alt, ctrl),
        os => drag::keys_of(os, shift, ctrl, alt, false),
    }
}

/// Whether the window's own pointer events drive the drag in `phase`, and with which keys:
/// only once its pointer grab was lost to a tab switch (`grab_lost`), while Gezik draws it
/// in the window or drives it outside (X11; on Windows the drag is back in the window when
/// these come).
fn window_drives(grab_lost: bool, phase: &Phase) -> Option<Keys> {
    match phase {
        Phase::Dragging(d) | Phase::Outside(d) if grab_lost => Some(d.keys),
        _ => None,
    }
}

/// Whether the release that dropped a window-driven drag stops there. A left one goes on to
/// Slint (whose grab points at an entry that is gone; nothing acts on a left release without
/// its press). A right one would open the menu of the background, tab or sidebar row under
/// the pointer as well as the drop's own.
fn swallows_release(dropped: bool, right: bool) -> bool {
    dropped && right
}

/// What a context menu opening or closing does to the drag in `phase`.
#[derive(Debug, PartialEq, Eq)]
enum AtMenu {
    /// Nothing of Gezik's own is under way, or the system has it: it goes on.
    Keep,
    /// A press waiting for its release (or a release that is no click): the menu takes that
    /// release (a native menu's modal loop eats it), so the press is forgotten. Kept, it
    /// would turn the next move into a drag, and that drag's right release into Copy here /
    /// Move here instead of the next menu.
    Forget,
    /// Gezik's drag in the window: it ends, dropping nothing.
    Cancel,
}

/// The phase `at` leaves behind (None: the phase stays). A cancelled drag ends in `Ended`,
/// as with Esc: its button may still be down (a menu key pressed mid-drag, or Gezik's own
/// menu on Linux, which takes no release), and that release must be no click.
fn phase_after(at: &AtMenu) -> Option<Phase> {
    match at {
        AtMenu::Keep => None,
        AtMenu::Forget => Some(Phase::Idle),
        AtMenu::Cancel => Some(Phase::Ended),
    }
}

fn at_menu(phase: &Phase) -> AtMenu {
    match phase {
        Phase::Idle | Phase::Offer(_) | Phase::Outside(_) => AtMenu::Keep,
        Phase::Armed { .. } | Phase::StackArmed { .. } | Phase::Ended => AtMenu::Forget,
        Phase::Dragging(_) => AtMenu::Cancel,
    }
}

/// How soon after a click that opened an entry the next one is the second of a double-click.
const DOUBLE_CLICK: Duration = Duration::from_millis(500);

/// Whether a left release opens the entry with single-click-open: a plain press that did not
/// become a drag or a rubber band, and not the second click of a double-click (`since`: the
/// time since the last click that opened).
fn opens_on_release(single_click_open: bool, plain_press: bool, dragged: bool, since: Option<Duration>) -> bool {
    single_click_open && plain_press && !dragged && since.is_none_or(|since| since >= DOUBLE_CLICK)
}

/// The address bar parts' places Slint reported, kept only where the part still shows the
/// label it had then (right after a navigation, the old places would point at new parts).
fn current_spans(stored: &[(String, f32, f32)], labels: &[String]) -> Vec<(f32, f32)> {
    labels
        .iter()
        .enumerate()
        .map(|(i, label)| match stored.get(i) {
            Some((was, x, width)) if was == label => (*x, *width),
            _ => (0.0, 0.0),
        })
        .collect()
}

fn path_of(location: Location) -> Option<PathBuf> {
    match location {
        Location::Path(path) => Some(path),
        Location::Drives | Location::Search(_) | Location::Flat(_) | Location::Trash => None,
    }
}

#[derive(Clone)]
pub struct Drags(Rc<Inner>);

impl Drags {
    pub fn new(window: &AppWindow, sidebar: Sidebar, ops: Operations, menus: Menus) -> Drags {
        let drags = Drags(Rc::new(Inner {
            window: window.as_weak(),
            sidebar,
            ops,
            menus,
            phase: RefCell::new(Phase::Idle),
            crumbs: RefCell::default(),
            hover_tab: Cell::new(None),
            tab_timer: Timer::default(),
            scroll_timer: Timer::default(),
            attached: RefCell::default(),
            outside: RefCell::default(),
            handoff_failed: Cell::new(false),
            grab_lost: Cell::new(false),
            last_open: Cell::new(None),
            handed: Cell::new(None),
            dropped_files: RefCell::default(),
        }));
        CURRENT.with(|c| *c.borrow_mut() = Some(drags.clone()));
        drags
    }

    pub fn install(&self, window: &AppWindow) {
        window.on_item_down({
            let drags = self.clone();
            move |pane, i, x, y, right, can_drag| {
                // A press (a right one too) is an action in its pane.
                crate::dual::pick(pane);
                if let Ok(index) = usize::try_from(i) {
                    drags.down(index, x, y, right, can_drag);
                }
            }
        });
        window.on_item_drag({
            let drags = self.clone();
            move |x, y, shift, ctrl, alt| drags.moved(x, y, keys(shift, ctrl, alt))
        });
        window.on_item_up({
            let drags = self.clone();
            move |_, x, y, right| drags.up(x, y, right)
        });
        window.on_item_cancel({
            let drags = self.clone();
            move || drags.cancel()
        });
        window.on_drag_keys({
            let drags = self.clone();
            move |shift, ctrl, alt| drags.keys_changed(keys(shift, ctrl, alt))
        });
        window.on_crumb_span({
            let drags = self.clone();
            move |pane, i, x, width| {
                if let (Ok(place @ 0..2), Ok(i)) = (usize::try_from(pane), usize::try_from(i)) {
                    let label = crate::panes::at(pane)
                        .and_then(|p| crate::panes::data(p.id))
                        .and_then(|d| d.crumbs.row_data(i))
                        .map(|crumb| crumb.label.to_string())
                        .unwrap_or_default();
                    let crumbs = &mut drags.0.crumbs.borrow_mut()[place];
                    if crumbs.len() <= i {
                        crumbs.resize(i + 1, (String::new(), 0.0, 0.0));
                    }
                    crumbs[i] = (label, x, width);
                }
            }
        });
        window.on_tab_out({
            let drags = self.clone();
            move |pane, tab, x, y, released| drags.tab_dragged(pane, tab, x, y, released)
        });
    }

    /// Tab `tab` of the pane at place `from` dragged to window point (`x`, `y`) (`tab` < 0: the
    /// drag ended without a drop): over the other pane its strip shows where the tab would go,
    /// and the release moves it there (spec 10 §4.5). Whether it is over the other pane (then
    /// the strip it came from does not reorder).
    fn tab_dragged(&self, from: i32, tab: i32, x: f32, y: f32, released: bool) -> bool {
        let Some(window) = self.0.window.upgrade() else { return false };
        let target = match (usize::try_from(from), usize::try_from(tab)) {
            (Ok(from), Ok(tab)) => drag::tab_target(&self.layout(&window), from, x, y).map(|to| (from, tab, to)),
            _ => None,
        };
        let shown = target.filter(|_| !released).map(|(_, _, to)| to);
        window.set_drop_pane(shown.map_or(-1, |(pane, _)| index(pane)));
        window.set_tab_insert(shown.and_then(|(_, slot)| slot).map_or(-1, index));
        if let Some((from, tab, (_, at))) = target.filter(|_| released) {
            // Once the release is fully handled: both strips' tabs change.
            Timer::single_shot(Duration::ZERO, move || crate::dual::move_tab(from, tab, at));
        }
        target.is_some()
    }

    /// Whether files are being dragged (Esc cancels).
    pub fn is_active(&self) -> bool {
        matches!(*self.0.phase.borrow(), Phase::Dragging(_))
    }

    /// winit says a file was dropped on the window, with no position: it comes from a program
    /// that went past Gezik's own drop target. The files of one drop go into the folder shown,
    /// with the drive rule (no keys are known).
    pub fn dropped_file(&self, path: PathBuf) {
        let first = {
            let mut files = self.0.dropped_files.borrow_mut();
            files.push(path);
            files.len() == 1
        };
        if first {
            let drags = self.clone();
            Timer::single_shot(Duration::ZERO, move || drags.take_dropped_files());
        }
    }

    fn take_dropped_files(&self) {
        let sources = std::mem::take(&mut *self.0.dropped_files.borrow_mut());
        let Some(dir) = crate::panes::active_view().folder() else {
            if crate::panes::active_view().shows_results() {
                crate::panes::active_view().note(crate::operations::NOT_HERE.to_owned());
            }
            return;
        };
        let d = Dragging {
            sources,
            all_dirs: false,
            right: false,
            keys: Keys::default(),
            allowed: Allowed::ALL,
            x: 0.0,
            y: 0.0,
            target: None,
            pressed: None,
            from_stack: false,
            virtual_count: 0,
            virtual_files: None,
        };
        if let Some(effect) = self.effect(&d, &dir) {
            self.0.ops.transfer(d.sources, dir, effect);
        }
    }

    /// The pointer moved to window position (`x`, `y`), as the window saw it. Only followed
    /// once a tab opened under the drag: before that the pressed entry reports every move.
    /// Also while the drag is outside the window (X11: Gezik drives it there from these).
    pub fn window_pointer_moved(&self, x: f32, y: f32) {
        let Some(keys) = window_drives(self.0.grab_lost.get(), &self.0.phase.borrow()) else { return };
        self.moved(x, y, keys);
    }

    /// A button came up, as the window saw it: drops a drag whose pointer grab was lost to a
    /// tab switch (nothing else in the window would), inside the window or outside it.
    /// Returns whether Slint must not see the release too (`swallows_release`).
    pub fn window_released(&self, x: f32, y: f32, right: bool) -> bool {
        if window_drives(self.0.grab_lost.get(), &self.0.phase.borrow()).is_none() {
            return false;
        }
        let used = self.up(x, y, right);
        self.0.grab_lost.set(false);
        swallows_release(used, right)
    }

    /// Esc: drops nothing. Returns whether a drag was cancelled.
    pub fn escape(&self) -> bool {
        if let Some(mut outside) = self.0.outside.borrow_mut().take() {
            outside.cancel();
            self.0.handed.set(None);
            self.0.grab_lost.set(false);
            *self.0.phase.borrow_mut() = Phase::Ended;
            return true;
        }
        if !self.is_active() {
            return false;
        }
        self.finish(None);
        *self.0.phase.borrow_mut() = Phase::Ended;
        true
    }

    /// A context menu opened or closed: no press from before it goes on (`at_menu`), and a
    /// drag in the window ends as with Esc (`phase_after`).
    ///
    /// Unlike `cancel()`, which ends whatever Gezik's own press started because that press was
    /// taken away, this leaves `Offer` and `Outside` alone: neither belongs to a press in the
    /// window. Files from another program end when their source says so (`offer_left`,
    /// `offer_dropped`), and a drag handed to the system ends in the system's drag loop
    /// (`outside_ended`, or the release Gezik drives on X11); forgetting either here would
    /// leave the source waiting for an answer, or the system's drag without its end.
    pub fn menu_shown(&self) {
        let at = at_menu(&self.0.phase.borrow());
        let Some(next) = phase_after(&at) else { return };
        if at == AtMenu::Cancel {
            self.finish(None);
        }
        *self.0.phase.borrow_mut() = next;
    }

    /// A left or right press on entry `index` at window position (`x`, `y`).
    fn down(&self, index: usize, x: f32, y: f32, right: bool, can_drag: bool) {
        if right {
            // The menu or the drag is for this entry: select it first if it is not.
            crate::panes::active_view().prepare_menu(index);
        }
        // Drives (This PC) are not files to move.
        let can_drag =
            can_drag && !crate::panes::active_view().shows_drives() && !crate::panes::active_view().shows_trash();
        *self.0.phase.borrow_mut() = Phase::Armed { index, x, y, right, can_drag };
    }

    /// A press on the drop stack's strip (`items`: what it drags).
    pub fn stack_down(&self, items: Vec<(PathBuf, bool)>, x: f32, y: f32) {
        *self.0.phase.borrow_mut() = Phase::StackArmed { items, x, y };
    }

    fn moved(&self, x: f32, y: f32, keys: Keys) {
        enum Begin {
            Entry(usize, bool),
            Stack(Vec<(PathBuf, bool)>),
        }
        let begin = match &*self.0.phase.borrow() {
            Phase::Armed { index, x: x0, y: y0, right, can_drag } => {
                (*can_drag && drag::past_threshold(x - x0, y - y0)).then_some(Begin::Entry(*index, *right))
            }
            Phase::StackArmed { items, x: x0, y: y0 } => {
                drag::past_threshold(x - x0, y - y0).then(|| Begin::Stack(items.clone()))
            }
            _ => None,
        };
        match begin {
            Some(Begin::Entry(index, right)) => self.start(index, right, keys),
            Some(Begin::Stack(items)) => self.start_with(items, None, false, keys, None, true),
            None => {}
        }
        if matches!(*self.0.phase.borrow(), Phase::Outside(_)) {
            return self.moved_outside(x, y, keys);
        }
        let dragging = {
            let mut phase = self.0.phase.borrow_mut();
            match &mut *phase {
                Phase::Dragging(d) => {
                    (d.x, d.y, d.keys) = (x, y, keys);
                    true
                }
                _ => false,
            }
        };
        if dragging {
            self.update();
        }
    }

    /// The pointer went far enough from the press on entry `index`: drag the selection.
    fn start(&self, index: usize, right: bool, keys: Keys) {
        let items = crate::panes::active_view().selected_items();
        let row = crate::panes::active_view().file_row(index);
        self.start_with(items, row, right, keys, Some(index), false);
    }

    /// Drags `items` (path, is a folder); `row` gives the picture, else Gezik's own icon of the
    /// first item.
    fn start_with(
        &self,
        items: Vec<(PathBuf, bool)>,
        row: Option<crate::FileRow>,
        right: bool,
        keys: Keys,
        pressed: Option<usize>,
        from_stack: bool,
    ) {
        if items.is_empty() {
            *self.0.phase.borrow_mut() = Phase::Idle;
            return;
        }
        let Some(window) = self.0.window.upgrade() else { return };
        match row {
            Some(row) => {
                window.set_drag_icon(row.icon);
                window.set_drag_has_icon(row.has_icon);
                window.set_drag_kind(row.kind);
            }
            None => {
                let (path, is_dir) = &items[0];
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                window.set_drag_has_icon(false);
                window.set_drag_kind(gezik_core::kind::Kind::of(&name, *is_dir).index());
            }
        }
        window.set_drag_count(i32::try_from(items.len()).unwrap_or(i32::MAX));
        window.set_drag_active(true);
        self.0.handoff_failed.set(false);
        self.0.grab_lost.set(false);
        *self.0.phase.borrow_mut() = Phase::Dragging(Dragging {
            all_dirs: items.iter().all(|(_, is_dir)| *is_dir),
            sources: items.into_iter().map(|(path, _)| path).collect(),
            right,
            keys,
            allowed: Allowed::ALL,
            x: 0.0,
            y: 0.0,
            target: None,
            pressed,
            from_stack,
            virtual_count: 0,
            virtual_files: None,
        });
    }

    /// A button came up. Returns whether a drag used it (then it is no click and no menu).
    fn up(&self, x: f32, y: f32, right: bool) -> bool {
        let phase = std::mem::replace(&mut *self.0.phase.borrow_mut(), Phase::Idle);
        match phase {
            Phase::Idle => false,
            Phase::StackArmed { .. } => false,
            // An offer from outside is not ended by Gezik's own button events.
            offer @ Phase::Offer(_) => {
                *self.0.phase.borrow_mut() = offer;
                false
            }
            Phase::Armed { index, x: x0, y: y0, right: pressed_right, .. } => {
                if !pressed_right && !right {
                    crate::panes::active_view().release(index, false);
                    let moved = gezik_core::drag::past_threshold(x - x0, y - y0);
                    let since = self.0.last_open.get().map(|at| at.elapsed());
                    // Single-click-open: a plain click (no Ctrl, Cmd or Shift) opens (spec 7.1);
                    // after the release is fully handled.
                    let plain = crate::panes::active_view().take_plain_press(index);
                    if opens_on_release(
                        crate::view_options::current().single_click_open,
                        plain,
                        moved || crate::panes::active_view().marquee_active(),
                        since,
                    ) {
                        self.0.last_open.set(Some(std::time::Instant::now()));
                        let nav = crate::panes::active_nav().clone();
                        // By path: a listing replaced before the timer fires must not open
                        // another entry under the same index.
                        let clicked = crate::panes::active_view().entry_path(index);
                        Timer::single_shot(Duration::ZERO, move || {
                            if let Some((path, is_dir)) = clicked {
                                crate::open_path(&nav, path, is_dir);
                            }
                        });
                    }
                }
                false
            }
            Phase::Ended => true,
            // Gezik drives this drag outside the window: the release drops it there.
            Phase::Outside(d) => {
                self.0.handed.set(None);
                self.0.grab_lost.set(false);
                if let Some(mut outside) = self.0.outside.borrow_mut().take() {
                    outside.released();
                }
                if let Some(index) = d.pressed {
                    crate::panes::active_view().release(index, true);
                }
                true
            }
            Phase::Dragging(mut d) => {
                (d.x, d.y) = (x, y);
                if let Some(index) = d.pressed {
                    crate::panes::active_view().release(index, true);
                }
                *self.0.phase.borrow_mut() = Phase::Dragging(d);
                self.update();
                let phase = std::mem::replace(&mut *self.0.phase.borrow_mut(), Phase::Idle);
                if let Phase::Dragging(d) = phase {
                    self.finish(Some(d));
                }
                true
            }
        }
    }

    /// The press was taken away (a menu opened, the window lost the pointer).
    fn cancel(&self) {
        if self.is_active() {
            self.finish(None);
        }
        *self.0.phase.borrow_mut() = Phase::Idle;
    }

    fn keys_changed(&self, keys: Keys) {
        let dragging = match &mut *self.0.phase.borrow_mut() {
            Phase::Dragging(d) => {
                d.keys = keys;
                true
            }
            _ => false,
        };
        if dragging {
            self.update();
        }
    }

    /// Finds the target under the pointer and shows it.
    fn update(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        let layout = self.layout(&window);
        let (x, y, target, ghost) = {
            let phase = self.0.phase.borrow();
            let (d, ghost) = match &*phase {
                Phase::Dragging(d) => (d, true),
                Phase::Offer(d) => (d, false),
                _ => return,
            };
            let hit = drag::hit(&layout, d.x, d.y, d.all_dirs);
            (d.x, d.y, self.resolve(&window, hit, d), ghost)
        };
        if ghost && target.hit == Hit::Outside && !self.0.handoff_failed.get() && self.0.attached.borrow().is_some() {
            return self.hand_off();
        }
        self.show(&window, x, y, &target, ghost);
        self.follow_tab(target.hit);
        self.follow_edge(&layout, x, y);
        if let Phase::Dragging(d) | Phase::Offer(d) = &mut *self.0.phase.borrow_mut() {
            d.target = Some(target);
        }
    }

    /// Where everything a drop can land on is, from the window as it is now.
    fn layout(&self, window: &AppWindow) -> Layout {
        let g = window.get_drop_geometry();
        let theme = window.global::<Theme>();
        let panes = crate::panes::all().iter().enumerate().map(|(place, pane)| self.pane_area(pane, place)).collect();
        let sidebar = match window.get_sidebar_position() {
            position @ (0 | 1) => {
                let width = window.get_sidebar_width();
                let x = if position == 0 { 0.0 } else { g.window_width - width };
                let model = window.get_sidebar_rows();
                let (pad, row_height) = (theme.get_spacing(), theme.get_row_height());
                let (y, height) = crate::sidebar::span();
                let (first, count) = drag::sidebar_window(g.sidebar_scroll, pad, row_height, height, model.row_count());
                let rows = (first..first + count).filter_map(|i| model.row_data(i)).map(|row| side_row(&row)).collect();
                Some(SidebarArea {
                    rect: Rect { x, y, width, height },
                    scroll: g.sidebar_scroll,
                    pad,
                    row_height,
                    first,
                    rows,
                })
            }
            _ => None,
        };
        let stack = (g.stack_height > 0.0).then_some(Rect {
            x: 0.0,
            y: g.stack_y,
            width: g.window_width,
            height: g.stack_height,
        });
        Layout { width: g.window_width, height: g.window_height, panes, sidebar, stack }
    }

    /// Where the parts of `pane` (at `place`) are.
    fn pane_area(&self, pane: &crate::panes::Pane, place: usize) -> PaneArea {
        let mirror = crate::panes::mirror(pane.id);
        let p = mirror.geometry.borrow().clone();
        let data = crate::panes::data(pane.id).unwrap_or_default();
        let list = ListArea {
            rect: Rect { x: p.view_x, y: p.view_y + p.list_top, width: p.list_width, height: p.list_height },
            scroll: pane.view.list_scroll(),
            geometry: pane.view.layout_geometry(),
            count: pane.view.len(),
            groups: pane.view.group_spans(),
        };
        let tabs = TabArea {
            rect: Rect { x: p.tab_x, y: 0.0, width: p.tab_strip_width, height: p.tab_height },
            scroll: p.tab_scroll,
            tab_width: p.tab_width,
            count: data.tabs.row_count(),
        };
        let spans = if mirror.path_editing.get() {
            Vec::new()
        } else {
            let labels: Vec<String> = data.crumbs.iter().map(|crumb| crumb.label.to_string()).collect();
            current_spans(self.0.crumbs.borrow().get(place).map_or(&[], Vec::as_slice), &labels)
        };
        let crumbs = CrumbArea {
            rect: Rect { x: p.address_x, y: p.address_y, width: p.address_width, height: p.address_height },
            spans,
        };
        PaneArea { list, tabs, crumbs }
    }

    /// What dropping `d` at `hit` would do.
    fn resolve(&self, window: &AppWindow, hit: Hit, d: &Dragging) -> Target {
        if d.virtual_count > 0 && !virtual_allows(hit) {
            return Target::none(hit);
        }
        let pane = hit.pane().and_then(|place| crate::panes::at(index(place)));
        let (hit, dir) = match (hit, &pane) {
            // Nothing goes into the bins but by a delete.
            (Hit::Entry(p, _), Some(pane)) if pane.view.shows_trash() => (Hit::Background(p), None),
            (Hit::Entry(p, i), Some(pane)) => match pane.view.entry_path(i) {
                Some((path, true)) => (hit, Some(path)),
                // A zip, 7z or tar file (not one of those dragged): the files are added to it.
                Some((path, false)) if d.virtual_count == 0 && self.can_add_to(d, &path) => {
                    let folder = path.parent().map(Path::to_path_buf);
                    let action = folder.as_deref().is_some_and(|f| self.writable(f)).then_some(Action::AddToArchive);
                    return Target { hit, dir: folder, action, archive: Some(path) };
                }
                // A file: into the folder it is in.
                _ => (Hit::Background(p), pane.view.folder()),
            },
            (Hit::Background(_), Some(pane)) => (hit, pane.view.folder()),
            (Hit::Sidebar(row), _) => {
                let place = window.get_sidebar_rows().row_data(row);
                (hit, place.and_then(|r| self.0.sidebar.location_of(r.section, r.index)).and_then(path_of))
            }
            (Hit::Tab(_, i), Some(pane)) => (hit, pane.nav.tab_location(i).and_then(path_of)),
            (Hit::Crumb(_, i), Some(pane)) => (hit, pane.nav.crumb_location(i).and_then(path_of)),
            (Hit::PinAt(_), _) => return Target { action: Some(Action::Pin), ..Target::none(hit) },
            (Hit::Stack, _) if d.from_stack => return Target::none(hit),
            (Hit::Stack, _) => return Target { action: Some(Action::AddToStack), ..Target::none(hit) },
            // Outside, nothing there, or a pane that closed.
            _ => (hit, None),
        };
        let action = dir.as_deref().and_then(|dir| self.effect(d, dir)).map(Action::Transfer);
        Target { hit, dir, action, archive: None }
    }

    /// Whether `d` can be added to the file `path`: an archive by its name, not one of the
    /// files dragged.
    fn can_add_to(&self, d: &Dragging, path: &Path) -> bool {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        gezik_core::batch::archive::can_add_to(&name) && !d.sources.iter().any(|source| source == path)
    }

    /// The effect of dropping `d` into `dir`, or None if it must be refused.
    fn effect(&self, d: &Dragging, dir: &Path) -> Option<Effect> {
        if !self.writable(dir) {
            return None;
        }
        if d.virtual_count > 0 {
            // Nothing to move or link: what has no file behind it is copied.
            return Some(Effect::Copy);
        }
        let roots: Vec<PathBuf> =
            crate::panes::active_nav().places().drives.into_iter().map(|drive| drive.path).collect();
        let first = d.sources.first()?;
        let effect = drag::choose(d.keys, drag::same_drive(first, dir, &roots), d.allowed)?;
        (!drag::refuse(&d.sources, dir, effect)).then_some(effect)
    }

    /// Whether files can be dropped into `dir` as far as Gezik knows without touching the
    /// disk: not on an optical drive.
    fn writable(&self, dir: &Path) -> bool {
        !crate::panes::active_nav()
            .places()
            .drives
            .iter()
            .any(|drive| drive.kind == DriveKind::Optical && is_within(dir, &drive.path))
    }

    /// Shows `target`: highlighted, and (with `ghost`) described next to the dragged items.
    fn show(&self, window: &AppWindow, x: f32, y: f32, target: &Target, ghost: bool) {
        let on = target.action.is_some();
        window.set_drop_pane(target.hit.pane().map_or(-1, index));
        window.set_drop_entry(match target.hit {
            Hit::Entry(_, i) if on => index(i),
            _ => -1,
        });
        window.set_drop_sidebar_row(match target.hit {
            Hit::Sidebar(row) if on => index(row),
            _ => -1,
        });
        window.set_drop_pin_row(match target.hit {
            Hit::PinAt(row) if on => index(row),
            _ => -1,
        });
        window.set_drop_tab(match target.hit {
            Hit::Tab(_, i) if on => index(i),
            _ => -1,
        });
        window.set_drop_crumb(match target.hit {
            Hit::Crumb(_, i) if on => index(i),
            _ => -1,
        });
        window.set_drop_stack(target.hit == Hit::Stack && on);
        let label = match (target.action, &target.dir) {
            (Some(Action::Pin), _) => drag::label(Action::Pin, Path::new("")),
            (Some(Action::AddToStack), _) => drag::label(Action::AddToStack, Path::new("")),
            (Some(Action::AddToArchive), _) => {
                drag::label(Action::AddToArchive, target.archive.as_deref().unwrap_or(Path::new("")))
            }
            (Some(action), Some(dir)) => drag::label(action, dir),
            _ => String::new(),
        };
        if ghost {
            window.set_drag_x(x);
            window.set_drag_y(y);
            window.set_drag_label(label.into());
            window.set_drag_forbidden(!on && target.hit != Hit::Outside);
        } else if OFFER_LABEL {
            window.set_drag_x(x);
            window.set_drag_y(y);
            window.set_offer_label(label.into());
        }
    }

    /// Resting on a tab opens it after a moment, and the drag goes on in it.
    fn follow_tab(&self, hit: Hit) {
        let tab = match hit {
            Hit::Tab(p, i) if crate::panes::at(index(p)).is_some_and(|pane| i != pane.nav.active_index()) => {
                Some((p, i))
            }
            _ => None,
        };
        if tab == self.0.hover_tab.get() {
            return;
        }
        self.0.hover_tab.set(tab);
        match tab {
            Some(tab) => self.0.tab_timer.start(TimerMode::SingleShot, drag::TAB_HOVER, move || {
                with_current(|drags| drags.tab_rested(tab));
            }),
            None => self.0.tab_timer.stop(),
        }
    }

    fn tab_rested(&self, (p, i): (usize, usize)) {
        if self.0.hover_tab.get() != Some((p, i))
            || !matches!(*self.0.phase.borrow(), Phase::Dragging(_) | Phase::Offer(_))
        {
            return;
        }
        self.0.hover_tab.set(None);
        if let Some(pane) = crate::panes::at(index(p)) {
            pane.nav.activate_tab(i);
        }
        // Only the active pane's list (the one dragged from) was rebuilt.
        if p == crate::panes::active_index()
            && let Phase::Dragging(d) = &mut *self.0.phase.borrow_mut()
        {
            // The pressed entry is gone with the old list, and so is its pointer grab: a
            // release over anything but the list would reach no one.
            d.pressed = None;
            self.0.grab_lost.set(true);
        }
        self.update();
        self.reanswer();
    }

    /// Near the list's top or bottom edge the list scrolls while the pointer rests there.
    fn follow_edge(&self, layout: &Layout, x: f32, y: f32) {
        let near = list_under(layout, x, y).is_some_and(|(_, list)| {
            drag::edge_scroll(y - list.rect.y, list.rect.height, list.geometry.row_height() / 2.0) != 0.0
        });
        if near && !self.0.scroll_timer.running() {
            self.0.scroll_timer.start(TimerMode::Repeated, SCROLL_STEP, || {
                with_current(|drags| drags.scroll_step());
            });
        } else if !near {
            self.0.scroll_timer.stop();
        }
    }

    fn scroll_step(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        let (x, y) = match &*self.0.phase.borrow() {
            Phase::Dragging(d) | Phase::Offer(d) => (d.x, d.y),
            _ => return self.0.scroll_timer.stop(),
        };
        let layout = self.layout(&window);
        let Some((p, list)) = list_under(&layout, x, y) else { return self.0.scroll_timer.stop() };
        let row_height = list.geometry.row_height();
        let step = drag::edge_scroll(y - list.rect.y, list.rect.height, row_height / 2.0);
        let lines = if list.groups.is_empty() {
            list.geometry.row_count(list.count)
        } else {
            gezik_core::layout::Lines { spans: &list.groups, per_row: list.geometry.per_row() }.count()
        };
        let content = lines as f32 * row_height;
        let lowest = (list.rect.height - content).min(0.0);
        if let Some(pane) = crate::panes::at(index(p)) {
            pane.view.set_list_scroll((list.scroll + step).clamp(lowest, 0.0));
        }
        self.update();
        self.reanswer();
    }

    /// The target of an offer from outside changed with no move (a tab opened, the list
    /// scrolled): the source is told, so a drop is never taken without being accepted first.
    fn reanswer(&self) {
        if !matches!(*self.0.phase.borrow(), Phase::Offer(_)) {
            return;
        }
        let answer = self.offer_answer();
        if let Some(attached) = &*self.0.attached.borrow() {
            attached.answer(&answer);
        }
    }

    /// What dropping the offer from outside at its target would do.
    fn offer_answer(&self) -> Answer {
        match &*self.0.phase.borrow() {
            Phase::Offer(Dragging {
                target: Some(Target { action: Some(Action::Transfer(effect)), dir: Some(dir), .. }),
                ..
            }) => Answer { effect: Some(*effect), folder: Some(drag::folder_name(dir)) },
            // Added to an archive: the files themselves stay where they are.
            Phase::Offer(Dragging {
                target: Some(Target { action: Some(Action::AddToArchive), archive: Some(archive), .. }),
                ..
            }) => Answer { effect: Some(Effect::Copy), folder: Some(drag::folder_name(archive)) },
            Phase::Offer(Dragging { target: Some(Target { action: Some(Action::AddToStack), .. }), .. }) => {
                Answer { effect: Some(Effect::Copy), folder: Some("Drop stack".to_owned()) }
            }
            _ => Answer::default(),
        }
    }

    /// Ends the drag: drops `d` on its target (None: drops nothing) and clears the window.
    /// Returns what the drop did at once (a menu, a pin or a refusal: nothing).
    fn finish(&self, d: Option<Dragging>) -> Option<Effect> {
        self.0.grab_lost.set(false);
        self.0.tab_timer.stop();
        self.0.scroll_timer.stop();
        self.0.hover_tab.set(None);
        if let Some(window) = self.0.window.upgrade() {
            window.set_drag_active(false);
            window.set_drag_label("".into());
            window.set_drag_forbidden(false);
            self.show(&window, 0.0, 0.0, &Target::none(Hit::Nothing), false);
        }
        let d = d?;
        let target = d.target.clone()?;
        self.drop_on(d, target)
    }

    fn drop_on(&self, d: Dragging, target: Target) -> Option<Effect> {
        // The pane dropped on becomes the active one (spec 10 §4.6).
        if let Some(p) = target.hit.pane() {
            crate::dual::activate(p);
        }
        if let (Hit::PinAt(row), Some(Action::Pin)) = (target.hit, target.action) {
            self.0.sidebar.pin_at_row(&d.sources, row);
            return None;
        }
        if target.action == Some(Action::AddToStack) {
            let items = d.sources.into_iter().map(|path| (path, false)).collect();
            crate::stack::with_current(|stack| stack.add(items));
            // To a source program: nothing is moved, so it deletes nothing.
            return Some(Effect::Copy);
        }
        let Some(dir) = target.dir else {
            // Search results have no folder to drop into (spec 4.6).
            if matches!(target.hit, Hit::Background(_)) && crate::panes::active_view().shows_results() {
                crate::panes::active_view().note(crate::operations::NOT_HERE.to_owned());
            }
            return None;
        };
        if d.virtual_count > 0 {
            // Written into `dir` by a job (spec 9 §8.1); no menu: a copy is all it can be.
            let files = d.virtual_files?;
            if target.action != Some(Action::Transfer(Effect::Copy)) {
                return None;
            }
            self.0.ops.materialize(files, dir, d.virtual_count);
            return Some(Effect::Copy);
        }
        if d.right {
            let writable = self.writable(&dir);
            let (can_copy, can_move, can_link) = menu_effects(d.allowed, writable, &d.sources, &dir);
            let archive = target.archive.filter(|_| target.action == Some(Action::AddToArchive));
            self.0.menus.drop_menu(d.sources, dir, archive, can_copy, can_move, can_link, d.x, d.y);
            return None;
        }
        if let (Some(Action::AddToArchive), Some(archive)) = (target.action, target.archive) {
            crate::archives::with_current(|archives| archives.add_to(archive, d.sources, None));
            return Some(Effect::Copy);
        }
        let Some(Action::Transfer(effect)) = target.action else { return None };
        self.0.ops.transfer(d.sources, dir, effect);
        Some(effect)
    }

    /// The pointer left the window with the drag: the system takes it over (Windows: until
    /// the drop, or until the pointer comes back into the window).
    fn hand_off(&self) {
        let d = match std::mem::replace(&mut *self.0.phase.borrow_mut(), Phase::Idle) {
            Phase::Dragging(d) => d,
            other => {
                *self.0.phase.borrow_mut() = other;
                return;
            }
        };
        // The window's events may be all that follows this drag (a tab opened under it): that
        // stays so outside the window and when it comes back.
        let grab_lost = self.0.grab_lost.get();
        self.finish(None);
        self.0.grab_lost.set(grab_lost);
        let (sources, right) = (d.sources.clone(), d.right);
        *self.0.phase.borrow_mut() = Phase::Outside(d.clone());
        let on_end: OnEnd = Box::new(|end| {
            with_current(|drags| drags.outside_ended(end));
        });
        let result = match &*self.0.attached.borrow() {
            Some(attached) => attached.drag_out(&sources, right, on_end),
            None => Err("no drop target".into()),
        };
        match result {
            // Its last position is outside the window: the next move (inside) places it.
            Ok(Handoff::Ended(DragEnd::Returned)) => self.resume(d, false),
            Ok(Handoff::Ended(_)) => {
                // The press ended in the system's drag loop; its release (sent back to the
                // window) is no click.
                self.0.grab_lost.set(false);
                *self.0.phase.borrow_mut() = Phase::Ended;
                if let Some(index) = d.pressed {
                    crate::panes::active_view().release(index, true);
                }
            }
            Ok(Handoff::Running(outside)) => {
                // Only a drag Gezik drives (X11) follows the window's events out there.
                if !outside.driven_by_gezik() {
                    self.0.grab_lost.set(false);
                }
                *self.0.outside.borrow_mut() = Some(outside);
                self.0.handed.set(Some(Handed { pressed: d.pressed, right: d.right, x: d.x, y: d.y }));
            }
            Err(why) => {
                eprintln!("gezik: cannot drag out of the window: {why}");
                self.0.handoff_failed.set(true);
                self.resume(d, true);
            }
        }
    }

    /// The drag is back in the window: Gezik draws it again (`place`: at its position now).
    fn resume(&self, d: Dragging, place: bool) {
        *self.0.phase.borrow_mut() = Phase::Dragging(d);
        if let Some(window) = self.0.window.upgrade() {
            window.set_drag_active(true);
        }
        if place {
            self.update();
        }
    }

    /// A move while Gezik drives the drag outside the window (X11): back inside, Gezik's own
    /// drag goes on.
    fn moved_outside(&self, x: f32, y: f32, keys: Keys) {
        let Some(window) = self.0.window.upgrade() else { return };
        let g = window.get_drop_geometry();
        let inside = x >= 0.0 && y >= 0.0 && x < g.window_width && y < g.window_height;
        let mut outside = self.0.outside.borrow_mut();
        let Some(driver) = outside.as_mut() else { return };
        if inside {
            driver.cancel();
            *outside = None;
            drop(outside);
            let phase = std::mem::replace(&mut *self.0.phase.borrow_mut(), Phase::Idle);
            if let Phase::Outside(mut d) = phase {
                (d.x, d.y, d.keys) = (x, y, keys);
                self.0.handed.set(None);
                self.resume(d, true);
            }
        } else {
            let scale = f64::from(window.window().scale_factor());
            driver.moved(f64::from(x) * scale, f64::from(y) * scale, keys);
        }
    }

    /// The system ended a drag Gezik handed it (macOS, Wayland): the window never saw the
    /// button come up, so Slint is told, and that release is no click.
    fn outside_ended(&self, _end: DragEnd) {
        self.0.outside.borrow_mut().take();
        self.0.grab_lost.set(false);
        // Gone already when the release was seen in the window (X11): nothing left to undo.
        let Some(handed) = self.0.handed.take() else { return };
        {
            let mut phase = self.0.phase.borrow_mut();
            // Also when the drag came back as an offer and was dropped on Gezik meanwhile.
            if matches!(*phase, Phase::Outside(_) | Phase::Idle) {
                *phase = Phase::Ended;
            }
        }
        if let Some(index) = handed.pressed {
            crate::panes::active_view().release(index, true);
        }
        if let Some(window) = self.0.window.upgrade() {
            use slint::platform::{PointerEventButton, WindowEvent};
            let button = if handed.right { PointerEventButton::Right } else { PointerEventButton::Left };
            let position = slint::LogicalPosition::new(handed.x, handed.y);
            window.window().dispatch_event(WindowEvent::PointerReleased { position, button });
        }
    }

    /// Puts Gezik's drop target on the window, once the native window exists (it does only
    /// after the event loop starts, so this retries for a while).
    pub fn attach_when_ready(&self, attempt: u32) {
        let drags = self.clone();
        Timer::single_shot(Duration::from_millis(20), move || {
            let Some(window) = drags.0.window.upgrade() else { return };
            let handler: Rc<dyn DropHandler> = Rc::new(Outside(drags.clone()));
            let wake = std::sync::Arc::new(|| {
                let _ = slint::invoke_from_event_loop(|| {
                    with_current(|drags| drags.poll());
                });
            });
            match gezik_platform::dnd::attach(&window.window().window_handle(), handler, wake) {
                Some(attached) => *drags.0.attached.borrow_mut() = Some(attached),
                None if attempt < 100 => drags.attach_when_ready(attempt + 1),
                None => {}
            }
        });
    }

    /// Events from the drop target's thread (Linux).
    fn poll(&self) {
        if let Some(attached) = &*self.0.attached.borrow() {
            if let Some(window) = self.0.window.upgrade() {
                attached.set_scale(window.window().scale_factor());
            }
            attached.poll();
        }
    }

    /// Physical client pixels to Slint's logical ones.
    fn logical(&self, x: f64, y: f64) -> (f32, f32) {
        let scale = self.0.window.upgrade().map_or(1.0, |w| w.window().scale_factor());
        ((x as f32) / scale, (y as f32) / scale)
    }

    /// Files from another program moved over the window: what dropping them here would do.
    fn offer_over(&self, offer: &Offer, x: f64, y: f64, keys: Keys) -> Answer {
        let (x, y) = self.logical(x, y);
        {
            let mut phase = self.0.phase.borrow_mut();
            match &mut *phase {
                Phase::Offer(d) => (d.x, d.y, d.keys, d.allowed) = (x, y, keys, offer.allowed),
                // Outside: Gezik's own drag, dropped back on its window by the system.
                Phase::Idle | Phase::Ended | Phase::Outside(_) => {
                    *phase = Phase::Offer(Dragging {
                        sources: offer.paths.clone(),
                        // Pinning dropped folders would need the disk to tell folders apart.
                        all_dirs: false,
                        right: offer.right,
                        keys,
                        allowed: offer.allowed,
                        x,
                        y,
                        target: None,
                        pressed: None,
                        from_stack: false,
                        virtual_count: offer.virtual_count,
                        virtual_files: None,
                    })
                }
                // Gezik's own drag (or a press) is under way: not an offer from outside.
                _ => return Answer::default(),
            }
        }
        self.update();
        self.offer_answer()
    }

    fn offer_left(&self) {
        if matches!(*self.0.phase.borrow(), Phase::Offer(_)) {
            *self.0.phase.borrow_mut() = Phase::Idle;
            self.finish(None);
        }
    }

    fn offer_dropped(&self, offer: &Offer, x: f64, y: f64, keys: Keys) -> Option<Effect> {
        self.offer_over(offer, x, y, keys);
        let phase = std::mem::replace(&mut *self.0.phase.borrow_mut(), Phase::Idle);
        match phase {
            Phase::Offer(mut d) => {
                d.virtual_files = offer.virtual_files.clone();
                self.finish(Some(d))
            }
            other => {
                *self.0.phase.borrow_mut() = other;
                None
            }
        }
    }
}

/// Whether items with no file behind them may land at `hit`: into a folder only (not the drop
/// stack, not pinned; an archive's entry means its folder).
fn virtual_allows(hit: Hit) -> bool {
    matches!(hit, Hit::Entry(..) | Hit::Background(_) | Hit::Sidebar(_) | Hit::Tab(..) | Hit::Crumb(..))
}

/// The pane (its place) whose list is under (`x`, `y`), and that list.
fn list_under(layout: &Layout, x: f32, y: f32) -> Option<(usize, &ListArea)> {
    layout.panes.iter().enumerate().find(|(_, area)| area.list.rect.contains(x, y)).map(|(p, area)| (p, &area.list))
}

/// A place or index as Slint numbers it.
fn index(i: usize) -> i32 {
    i32::try_from(i).unwrap_or(-1)
}

/// The window's drop target, for files dragged in from other programs.
struct Outside(Drags);

impl DropHandler for Outside {
    fn over(&self, offer: &Offer, x: f64, y: f64, keys: Keys) -> Answer {
        self.0.offer_over(offer, x, y, keys)
    }

    fn leave(&self) {
        self.0.offer_left();
    }

    fn dropped(&self, offer: &Offer, x: f64, y: f64, keys: Keys) -> Option<Effect> {
        self.0.offer_dropped(offer, x, y, keys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_click_opens_once_and_not_after_a_drag() {
        let long_ago = Some(Duration::from_secs(5));
        assert!(opens_on_release(true, true, false, None));
        assert!(opens_on_release(true, true, false, long_ago));
        assert!(!opens_on_release(false, true, false, None), "the setting is off");
        assert!(!opens_on_release(true, false, false, None), "Ctrl or Shift");
        assert!(!opens_on_release(true, true, true, None), "a drag or a rubber band");
        assert!(!opens_on_release(true, true, false, Some(Duration::from_millis(200))), "second click");
    }

    fn dragging() -> Dragging {
        Dragging {
            sources: vec![PathBuf::from("a.txt")],
            all_dirs: false,
            right: false,
            keys: Keys { shift: true, ..Keys::default() },
            allowed: Allowed::ALL,
            x: 0.0,
            y: 0.0,
            target: None,
            pressed: None,
            from_stack: false,
            virtual_count: 0,
            virtual_files: None,
        }
    }

    #[test]
    fn virtual_items_go_only_into_folders() {
        for hit in [Hit::Entry(0, 0), Hit::Background(1), Hit::Sidebar(2), Hit::Tab(1, 1), Hit::Crumb(0, 0)] {
            assert!(virtual_allows(hit), "{hit:?}");
        }
        for hit in [Hit::Stack, Hit::PinAt(1), Hit::Outside, Hit::Nothing] {
            assert!(!virtual_allows(hit), "{hit:?}");
        }
    }

    #[test]
    fn the_right_drag_menu_offers_a_link_only_where_it_may() {
        let a = PathBuf::from(if cfg!(windows) { r"C:\w\a" } else { "/w/a" });
        let src = std::slice::from_ref(&a);
        let dir = a.parent().unwrap();
        assert_eq!(menu_effects(Allowed::ALL, true, src, dir), (true, false, true), "same folder: no move");
        let no_link = Allowed { link: false, ..Allowed::ALL };
        assert_eq!(menu_effects(no_link, true, src, dir), (true, false, false));
        assert_eq!(menu_effects(Allowed::ALL, false, src, dir), (false, false, false), "read-only");
        assert_eq!(menu_effects(Allowed::ALL, true, src, &a.join("sub")), (false, false, false), "inside itself");
    }

    #[test]
    fn after_a_tab_switch_the_window_drives_the_drag_inside_and_outside() {
        let keys = Some(Keys { shift: true, ..Keys::default() });
        assert_eq!(window_drives(true, &Phase::Dragging(dragging())), keys);
        // Handed to the system after resting on a tab (X11 drives it from the window's events).
        assert_eq!(window_drives(true, &Phase::Outside(dragging())), keys);
        // The pressed entry still reports the moves: the window's events are not used.
        assert_eq!(window_drives(false, &Phase::Dragging(dragging())), None);
        assert_eq!(window_drives(false, &Phase::Outside(dragging())), None);
        // Files from another program, or no drag: never.
        assert_eq!(window_drives(true, &Phase::Offer(dragging())), None);
        assert_eq!(window_drives(true, &Phase::Idle), None);
        assert_eq!(window_drives(true, &Phase::Ended), None);
    }

    #[test]
    fn only_a_right_release_that_dropped_stops_before_slint() {
        assert!(swallows_release(true, true));
        assert!(!swallows_release(true, false));
        assert!(!swallows_release(false, true));
        assert!(!swallows_release(false, false));
    }

    fn armed(right: bool) -> Phase {
        Phase::Armed { index: 0, x: 10.0, y: 20.0, right, can_drag: true }
    }

    #[test]
    fn a_menu_forgets_a_press_waiting_for_its_release() {
        // The press whose release the menu took: kept, the next move would start a right
        // drag and its release would show Copy here instead of the next menu.
        assert_eq!(at_menu(&armed(true)), AtMenu::Forget);
        assert_eq!(at_menu(&armed(false)), AtMenu::Forget);
        assert_eq!(at_menu(&Phase::Ended), AtMenu::Forget);
        // A drag in the window ends without a drop.
        assert_eq!(at_menu(&Phase::Dragging(dragging())), AtMenu::Cancel);
    }

    #[test]
    fn a_press_on_the_stack_is_forgotten_by_a_menu() {
        let press = Phase::StackArmed { items: vec![(PathBuf::from("a.txt"), false)], x: 0.0, y: 0.0 };
        assert_eq!(at_menu(&press), AtMenu::Forget);
        assert_eq!(window_drives(true, &press), None);
    }

    #[test]
    fn a_drag_a_menu_cancels_ends_like_esc() {
        // Its button may still be down: the release that follows is no click.
        assert!(matches!(phase_after(&AtMenu::Cancel), Some(Phase::Ended)));
        assert!(matches!(phase_after(&AtMenu::Forget), Some(Phase::Idle)));
        assert!(phase_after(&AtMenu::Keep).is_none());
    }

    #[test]
    fn a_menu_leaves_drops_and_drags_the_system_has_alone() {
        // The drop menu itself opens with the drag already ended (`up` set Idle).
        assert_eq!(at_menu(&Phase::Idle), AtMenu::Keep);
        // Files from another program, and a drag handed to the system, end by themselves.
        assert_eq!(at_menu(&Phase::Offer(dragging())), AtMenu::Keep);
        assert_eq!(at_menu(&Phase::Outside(dragging())), AtMenu::Keep);
    }

    #[test]
    fn crumb_places_are_kept_only_for_unchanged_parts() {
        let stored =
            vec![("This PC".to_owned(), 10.0, 60.0), ("C:".to_owned(), 80.0, 30.0), ("old".to_owned(), 120.0, 40.0)];
        let labels = vec!["This PC".to_owned(), "C:".to_owned(), "new".to_owned(), "deeper".to_owned()];
        assert_eq!(current_spans(&stored, &labels), vec![(10.0, 60.0), (80.0, 30.0), (0.0, 0.0), (0.0, 0.0)]);
        assert_eq!(current_spans(&stored, &labels[..1]), vec![(10.0, 60.0)], "fewer parts now");
    }
}
