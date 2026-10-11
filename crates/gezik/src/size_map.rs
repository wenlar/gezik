//! The size map (spec 11 §10): a layer over the window with a folder's squarified treemap.
//! Nothing is kept until it opens; the scan runs on a low-priority thread and is cancelled
//! when the layer closes, which drops the tree and the model (the window's `Slot` empties).

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use gezik_config::shortcuts::{Chord, Key, Platform};
use gezik_core::nav::Location;
use gezik_core::treemap::{self, NONE, Rect, SizeTree, Tile, What};
use gezik_core::{format_size, kind::Kind};
use gezik_search::size::FolderTotal;
use slint::{Color, ComponentHandle, ModelRc, VecModel};

use crate::panes::PaneId;
use crate::view::View;
use crate::{AppWindow, MapRect, Theme};

/// The size map's menu (spec 11 §13.3: 2300–2309).
pub const MAP_OPEN: u32 = 2300;
pub const MAP_SHOW: u32 = 2301;
pub const MAP_TRASH: u32 = 2302;
pub const MAP_COPY_PATH: u32 = 2303;

thread_local! {
    pub(crate) static CURRENT: crate::windows::Slot<SizeMap> = const { crate::windows::Slot::new() };
}

/// Runs `f` with this window's size map, if it is open.
pub fn with_current(f: impl FnOnce(&SizeMap)) {
    if let Some(map) = CURRENT.with(crate::windows::Slot::current) {
        f(&map);
    }
}

pub fn is_open() -> bool {
    CURRENT.with(crate::windows::Slot::current).is_some()
}

/// `size-map`: the selected folder, else the folder shown (This PC: the selected drive).
pub fn open_for(view: &View) {
    let selected = view.selected_items();
    let folder = match selected.as_slice() {
        [(path, true)] => Some(path.clone()),
        _ => view.folder(),
    };
    let Some(folder) = folder else { return view.note("Size Map works on a folder".to_owned()) };
    open(view, folder);
}

/// The menus' Size Map on `folder`.
pub fn open_at(folder: PathBuf) {
    open(&crate::panes::active_view(), folder);
}

fn open(view: &View, folder: PathBuf) {
    let Some(window) = crate::windows::with_window(crate::windows::current(), |w| w.as_weak()) else { return };
    if is_open() {
        return;
    }
    let map = SizeMap(Rc::new(Inner {
        window,
        pane: view.pane_id(),
        root: folder,
        tree: RefCell::new(Arc::new(Mutex::new(SizeTree::new("")))),
        cancel: RefCell::new(Arc::default()),
        scanning: Cell::new(false),
        focus: Cell::new(0),
        area: Cell::new((0.0, 0.0)),
        tiles: RefCell::default(),
        current: Cell::new(None),
        zoomed: Cell::new(false),
    }));
    CURRENT.with(|c| c.set(crate::windows::current(), map.clone()));
    if let Some(window) = map.0.window.upgrade() {
        window.set_sm_open(true);
    }
    map.scan();
}

struct Inner {
    window: slint::Weak<AppWindow>,
    pane: PaneId,
    root: PathBuf,
    /// The scan's tree and flag (a new pair for each scan).
    tree: RefCell<Arc<Mutex<SizeTree>>>,
    cancel: RefCell<Arc<AtomicBool>>,
    scanning: Cell<bool>,
    /// The folder shown (a node), and the area's size.
    focus: Cell<u32>,
    area: Cell<(f32, f32)>,
    tiles: RefCell<Vec<Tile>>,
    /// The rectangle the keyboard is on.
    current: Cell<Option<usize>>,
    /// The last click zoomed in: a double-click's second half means the folder zoomed into.
    zoomed: Cell<bool>,
}

#[derive(Clone)]
pub struct SizeMap(Rc<Inner>);

/// What a rectangle stands for, by path.
enum Target {
    Folder(PathBuf),
    File(PathBuf),
}

impl SizeMap {
    /// Starts a scan of the root afresh (opening, F5).
    fn scan(&self) {
        self.0.cancel.borrow().store(true, Ordering::Relaxed);
        let name = self.0.root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let tree = Arc::new(Mutex::new(SizeTree::new(&name)));
        let cancel = Arc::new(AtomicBool::new(false));
        *self.0.tree.borrow_mut() = tree.clone();
        *self.0.cancel.borrow_mut() = cancel.clone();
        self.0.scanning.set(true);
        self.0.focus.set(0);
        self.0.current.set(None);
        let (root, window_id) = (self.0.root.clone(), crate::windows::current());
        let tick_tree = tree.clone();
        let spawned = std::thread::Builder::new().name("gezik-size-map".into()).spawn(move || {
            gezik_platform::priority::lower_this_thread();
            let threads = if gezik_platform::fs::is_network(&root).unwrap_or(false) { 1 } else { 2 };
            let tick = Box::new(move || {
                let tree = tick_tree.clone();
                crate::windows::post(window_id, move || with_current(|map| map.arrived(&tree, false)));
            });
            let ended = gezik_search::size::scan_tree(&root, threads, &cancel, tree.clone(), tick);
            if ended {
                crate::windows::post(window_id, move || with_current(|map| map.arrived(&tree, true)));
            }
            gezik_platform::priority::give_back_memory();
        });
        if spawned.is_err() {
            self.0.scanning.set(false);
        }
        self.redraw();
    }

    /// The scan of `tree` has more (`done`: all of it).
    fn arrived(&self, tree: &Arc<Mutex<SizeTree>>, done: bool) {
        if !Arc::ptr_eq(tree, &self.0.tree.borrow()) {
            return;
        }
        if done {
            self.0.scanning.set(false);
            self.keep_totals();
        }
        self.redraw();
    }

    /// Every folder's total goes into the pane's folder sizes (spec §10.1), the deepest first
    /// so that the cache keeps the ones nearest the root.
    fn keep_totals(&self) {
        let totals: Vec<(PathBuf, FolderTotal, Option<std::time::SystemTime>)> = {
            let tree = self.0.tree.borrow().clone();
            let tree = tree.lock().unwrap_or_else(PoisonError::into_inner);
            (0..tree.len() as u32)
                .rev()
                .filter(|&id| id == 0 || tree.node(id).parent != NONE)
                .map(|id| {
                    let node = tree.node(id);
                    let total = FolderTotal {
                        bytes: node.bytes,
                        counts: Some((node.files, node.folders)),
                        partial: node.partial,
                    };
                    (self.path_of(&tree, id), total, node.stamp)
                })
                .collect()
        };
        crate::panes::with_id(self.0.pane, |p| p.folder_sizes.put_all(totals));
    }

    fn path_of(&self, tree: &SizeTree, id: u32) -> PathBuf {
        let mut path = self.0.root.clone();
        path.extend(tree.path(id));
        path
    }

    /// Lays the folder shown out again and shows it.
    fn redraw(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        let (w, h) = self.0.area.get();
        let tree = self.0.tree.borrow().clone();
        let mut tree = tree.lock().unwrap_or_else(PoisonError::into_inner);
        if self.0.scanning.get() {
            tree.sum();
        }
        let tiles = treemap::layout(&tree, self.0.focus.get(), Rect { x: 0.0, y: 0.0, w, h });
        let theme = window.global::<Theme>();
        let (accent, background) = (theme.get_accent(), theme.get_background());
        let rects: Vec<MapRect> = tiles
            .iter()
            .map(|tile| {
                let color = color_of(&tree, tile, accent, background);
                let label = if tile.labelled() {
                    format!("{}  {}", name_of(&tree, tile.what), format_size(tile.bytes))
                } else {
                    String::new()
                };
                MapRect {
                    x: tile.rect.x,
                    y: tile.rect.y,
                    w: tile.rect.w,
                    h: tile.rect.h,
                    color,
                    ink: ink_on(color),
                    label: label.into(),
                }
            })
            .collect();
        window.set_sm_rects(ModelRc::new(VecModel::from(rects)));
        window.set_sm_path(self.path_of(&tree, self.0.focus.get()).display().to_string().into());
        *self.0.tiles.borrow_mut() = tiles;
        drop(tree);
        self.show_current();
    }

    /// The rectangle the keyboard is on, and the status line: it, else the scan's state.
    fn show_current(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        let current = self.0.current.get().filter(|&i| i < self.0.tiles.borrow().len());
        window.set_sm_current(current.map_or(-1, |i| i as i32));
        let status = match current {
            Some(i) => self.describe(i),
            None if self.0.scanning.get() => "Scanning…".to_owned(),
            None => self.describe_focus(),
        };
        window.set_sm_status(status.into());
    }

    fn describe_focus(&self) -> String {
        let tree = self.0.tree.borrow().clone();
        let tree = tree.lock().unwrap_or_else(PoisonError::into_inner);
        let node = tree.node(self.0.focus.get());
        let more = if node.partial { " (some folders could not be read)" } else { "" };
        format!("{} · {} files · {} folders{more}", format_size(node.bytes), node.files, node.folders)
    }

    /// The status line for rectangle `i`: `C:\Work\video\raw · 38.2 GB · 1,204 files · 12% of C:\Work`.
    fn describe(&self, i: usize) -> String {
        let Some(tile) = self.0.tiles.borrow().get(i).copied() else { return String::new() };
        let tree = self.0.tree.borrow().clone();
        let tree = tree.lock().unwrap_or_else(PoisonError::into_inner);
        let focus = self.0.focus.get();
        let whole = tree.node(focus).bytes.max(1);
        let (name, files) = match tile.what {
            What::Folder(id) => {
                (self.path_of(&tree, id).display().to_string(), format!(" · {} files", tree.node(id).files))
            }
            What::File(f) => (self.0.root.join(&tree.files[f].0).display().to_string(), String::new()),
            What::Files(id) => {
                let node = tree.node(id);
                (format!("{} · its files", self.path_of(&tree, id).display()), format!(" · {} files", node.own_files))
            }
            What::Small(_) => (name_of(&tree, tile.what), String::new()),
        };
        let share = tile.bytes as f64 * 100.0 / whole as f64;
        let of = self.path_of(&tree, focus).display().to_string();
        format!("{name} · {}{files} · {share:.0}% of {of}", format_size(tile.bytes))
    }

    fn target(&self, i: usize) -> Option<Target> {
        let tile = self.0.tiles.borrow().get(i).copied()?;
        let tree = self.0.tree.borrow().clone();
        let tree = tree.lock().unwrap_or_else(PoisonError::into_inner);
        match tile.what {
            What::Folder(id) | What::Files(id) => Some(Target::Folder(self.path_of(&tree, id))),
            What::File(f) => Some(Target::File(self.0.root.join(&tree.files[f].0))),
            What::Small(_) => None,
        }
    }

    fn tile_at(&self, x: f32, y: f32) -> Option<usize> {
        treemap::hit(&self.0.tiles.borrow(), x, y)
    }

    pub fn resized(&self, w: f32, h: f32) {
        if self.0.area.replace((w, h)) != (w, h) {
            self.redraw();
        }
    }

    pub fn hover(&self, x: f32, y: f32) {
        let at = self.tile_at(x, y);
        if let (Some(i), Some(window)) = (at, self.0.window.upgrade()) {
            window.set_sm_status(self.describe(i).into());
        }
    }

    /// A click: a folder is zoomed into (no reading: the tree has it), anything else is picked.
    pub fn clicked(&self, x: f32, y: f32) {
        let Some(i) = self.tile_at(x, y) else { return };
        let what = self.0.tiles.borrow().get(i).map(|t| t.what);
        self.0.zoomed.set(matches!(what, Some(What::Folder(_))));
        match what {
            Some(What::Folder(id)) => self.zoom(id),
            _ => {
                self.0.current.set(Some(i));
                self.show_current();
            }
        }
    }

    fn zoom(&self, id: u32) {
        self.0.focus.set(id);
        self.0.current.set(None);
        self.redraw();
    }

    /// Up, Backspace: one level out (not above the folder the map opened on).
    pub fn up(&self) {
        let parent = {
            let tree = self.0.tree.borrow().clone();
            let tree = tree.lock().unwrap_or_else(PoisonError::into_inner);
            tree.node(self.0.focus.get()).parent
        };
        if parent != NONE {
            self.zoom(parent);
        }
    }

    /// Enter, double-click: the pane goes to the folder (a file: its folder, with it selected)
    /// and the map closes.
    pub fn activated_at(&self, x: f32, y: f32) {
        if self.0.zoomed.replace(false) {
            let path = {
                let tree = self.0.tree.borrow().clone();
                let tree = tree.lock().unwrap_or_else(PoisonError::into_inner);
                self.path_of(&tree, self.0.focus.get())
            };
            self.close();
            return crate::panes::active_nav().go(Location::Path(path));
        }
        if let Some(i) = self.tile_at(x, y) {
            self.enter(i);
        }
    }

    fn enter(&self, i: usize) {
        let Some(target) = self.target(i) else { return };
        self.close();
        match target {
            Target::Folder(path) => crate::panes::active_nav().go(Location::Path(path)),
            Target::File(path) => crate::operations::with_current(|ops| ops.show_path_in_folder(&path, false)),
        }
    }

    /// The right-click menu's items for the rectangle at `x`, `y` (it becomes the current one).
    pub fn menu_items(&self, x: f32, y: f32) -> Vec<(u32, String, bool)> {
        let Some(i) = self.tile_at(x, y) else { return Vec::new() };
        self.0.current.set(Some(i));
        self.show_current();
        if self.target(i).is_none() {
            return Vec::new();
        }
        [(MAP_OPEN, "Open"), (MAP_SHOW, "Show in Pane"), (MAP_TRASH, "Move to Trash"), (MAP_COPY_PATH, "Copy Path")]
            .into_iter()
            .map(|(id, title)| (id, title.to_owned(), true))
            .collect()
    }

    pub fn menu_chosen(&self, id: u32) {
        let Some(i) = self.0.current.get() else { return };
        let Some(target) = self.target(i) else { return };
        let (path, is_dir) = match &target {
            Target::Folder(path) => (path.clone(), true),
            Target::File(path) => (path.clone(), false),
        };
        match id {
            MAP_OPEN if is_dir => self.enter(i),
            MAP_OPEN => crate::panes::active_nav().open_item(path, false),
            MAP_SHOW => {
                self.close();
                crate::operations::with_current(|ops| ops.show_path_in_folder(&path, false));
            }
            MAP_TRASH => {
                crate::operations::with_current(|ops| ops.trash_paths(vec![path], false));
                self.drop_tile(i);
            }
            MAP_COPY_PATH => {
                crate::copy_path::copy(&crate::panes::active_view(), &[path], gezik_core::path_text::PathFormat::Full);
            }
            _ => {}
        }
    }

    /// Rectangle `i` went to the trash: the map leaves it out without scanning again.
    fn drop_tile(&self, i: usize) {
        let Some(what) = self.0.tiles.borrow().get(i).map(|t| t.what) else { return };
        {
            let tree = self.0.tree.borrow().clone();
            let mut tree = tree.lock().unwrap_or_else(PoisonError::into_inner);
            match what {
                What::Folder(id) => tree.remove(id),
                What::File(f) => tree.remove_file(f),
                What::Files(_) | What::Small(_) => return,
            }
            tree.sum();
        }
        self.0.current.set(None);
        self.redraw();
    }

    /// The layer's keys (spec §10.3); every key is its own while it is open.
    pub fn chord(&self, chord: &Chord) -> bool {
        let plain = !chord.ctrl && !chord.alt && !chord.meta && !chord.shift;
        let len = self.0.tiles.borrow().len();
        let step = |dx: f32, dy: f32| {
            let next = match self.0.current.get() {
                Some(i) => treemap::neighbor(&self.0.tiles.borrow(), i, dx, dy),
                None => (len > 0).then_some(0),
            };
            if next.is_some() {
                self.0.current.set(next);
                self.show_current();
            }
        };
        match chord.key {
            Key::Escape if self.0.scanning.get() => {
                self.0.cancel.borrow().store(true, Ordering::Relaxed);
                self.0.scanning.set(false);
                self.redraw();
                if let Some(window) = self.0.window.upgrade() {
                    window.set_sm_status("Stopped".into());
                }
            }
            Key::Escape => self.close(),
            Key::Backspace => self.up(),
            Key::Up if chord.alt => self.up(),
            Key::Enter => {
                if let Some(i) = self.0.current.get() {
                    self.enter(i);
                }
            }
            Key::F(5) => self.scan(),
            Key::Char('r') if crate::keys::is_primary(chord, Platform::current()) => self.scan(),
            Key::Left if plain => step(-1.0, 0.0),
            Key::Right if plain => step(1.0, 0.0),
            Key::Up if plain => step(0.0, -1.0),
            Key::Down if plain => step(0.0, 1.0),
            _ => {}
        }
        true
    }

    pub fn close(&self) {
        self.0.cancel.borrow().store(true, Ordering::Relaxed);
        if let Some(window) = self.0.window.upgrade() {
            window.set_sm_open(false);
            window.set_sm_rects(ModelRc::default());
            if !window.get_dialog_open() && !window.get_conflicts_open() {
                window.invoke_focus_list();
            }
        }
        CURRENT.with(|c| c.forget(crate::windows::current()));
    }
}

/// A rectangle's name: the folder's or file's, "Files", "N small items".
fn name_of(tree: &SizeTree, what: What) -> String {
    match what {
        What::Folder(id) => tree.name(id).to_owned(),
        What::File(f) => tree.files[f].0.clone(),
        What::Files(_) => "Files".to_owned(),
        What::Small(n) => format!("{n} small items"),
    }
}

/// Folders: tones of the accent by depth; files: six fixed colors by type (spec §10.2).
fn color_of(tree: &SizeTree, tile: &Tile, accent: Color, background: Color) -> Color {
    let rgb = |v: u32| Color::from_rgb_u8((v >> 16) as u8, (v >> 8) as u8, v as u8);
    match tile.what {
        What::Folder(_) => accent.mix(&background, if tile.depth == 0 { 0.45 } else { 0.7 }),
        What::Small(_) => rgb(0xb4b4b4),
        What::Files(_) => rgb(0x9a9a9a),
        What::File(f) => rgb(file_color(&tree.files[f].0)),
    }
}

/// A file's color by its name's type: pictures, video, sound, documents, archives, other.
fn file_color(name: &str) -> u32 {
    match Kind::of(name, false) {
        Kind::Image => 0x5aa469,
        Kind::Video => 0xc0504d,
        Kind::Audio => 0x8064a2,
        Kind::Document | Kind::Spreadsheet | Kind::Presentation | Kind::Pdf | Kind::Text => 0x4f81bd,
        Kind::Archive | Kind::DiskImage => 0xd39a2c,
        _ => 0x9a9a9a,
    }
}

/// Black or white, whichever reads on `color`.
fn ink_on(color: Color) -> Color {
    let light = 0.299 * f32::from(color.red()) + 0.587 * f32::from(color.green()) + 0.114 * f32::from(color.blue());
    if light > 140.0 { Color::from_rgb_u8(0, 0, 0) } else { Color::from_rgb_u8(255, 255, 255) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_take_their_type_colors() {
        assert_eq!(file_color("a.JPG"), 0x5aa469);
        assert_eq!(file_color("a.mkv"), 0xc0504d);
        assert_eq!(file_color("a.flac"), 0x8064a2);
        assert_eq!(file_color("a.pdf"), 0x4f81bd);
        assert_eq!(file_color("a.7z"), 0xd39a2c);
        assert_eq!(file_color("a.bin"), 0x9a9a9a);
        assert_eq!(ink_on(Color::from_rgb_u8(250, 250, 250)), Color::from_rgb_u8(0, 0, 0));
        assert_eq!(ink_on(Color::from_rgb_u8(20, 20, 60)), Color::from_rgb_u8(255, 255, 255));
    }
}
