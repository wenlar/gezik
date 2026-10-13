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
use gezik_i18n::{t, tn, tx};
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
    let Some(folder) = folder else { return view.note(t!("Size Map works on a folder").into_owned()) };
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
        stopped: Cell::new(false),
        trashing: RefCell::default(),
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
    /// Esc stopped the scan: its totals stay as they were then.
    stopped: Cell<bool>,
    /// Paths sent to the trash: they leave the map once a job has taken them away.
    trashing: RefCell<Vec<PathBuf>>,
}

#[derive(Clone)]
pub struct SizeMap(Rc<Inner>);

/// What a rectangle stands for, by path.
enum Target {
    Folder(PathBuf),
    File(PathBuf),
    /// A folder's own files as one block: only its folder can be opened or named.
    FilesOf(PathBuf),
}

/// The menu a rectangle gets: a folder or file everything, a files block only what does not
/// act on the folder as a whole (its Move to Trash would trash the folder).
pub fn menu_ids(what: What) -> &'static [u32] {
    match what {
        What::Folder(_) | What::File(_) => &[MAP_OPEN, MAP_SHOW, MAP_TRASH, MAP_COPY_PATH],
        What::Files(_) => &[MAP_OPEN, MAP_COPY_PATH],
        What::Small(_) => &[],
    }
}

fn menu_title(id: u32) -> &'static str {
    match id {
        MAP_OPEN => tx!("Open"),
        MAP_SHOW => tx!("Show in Pane"),
        MAP_TRASH => tx!("Move to Trash"),
        _ => tx!("Copy Path"),
    }
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
        self.0.stopped.set(false);
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
        // A newer scan's, or one stopped by Esc (its last ticks are not shown).
        if !Arc::ptr_eq(tree, &self.0.tree.borrow()) || !self.0.scanning.get() {
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
                .filter(|&id| tree.is_live(id))
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
            None if self.0.scanning.get() => t!("Scanning…").into_owned(),
            // TRANSLATORS: {} is what the map shows ("38.2 GB · 1204 files · 12 folders").
            None if self.0.stopped.get() => t!("Stopped · {}", self.describe_focus()),
            None => self.describe_focus(),
        };
        window.set_sm_status(status.into());
    }

    fn describe_focus(&self) -> String {
        let tree = self.0.tree.borrow().clone();
        let tree = tree.lock().unwrap_or_else(PoisonError::into_inner);
        let node = tree.node(self.0.focus.get());
        let text = [
            format_size(node.bytes),
            tn!("{n} file" | "{n} files", node.files),
            tn!("{n} folder" | "{n} folders", node.folders),
        ]
        .join(" · ");
        if node.partial { t!("{} (some folders could not be read)", text) } else { text }
    }

    /// The status line for rectangle `i`: `C:\Work\video\raw · 38.2 GB · 1,204 files · 12% of C:\Work`.
    fn describe(&self, i: usize) -> String {
        let Some(tile) = self.0.tiles.borrow().get(i).copied() else { return String::new() };
        let tree = self.0.tree.borrow().clone();
        let tree = tree.lock().unwrap_or_else(PoisonError::into_inner);
        let focus = self.0.focus.get();
        let whole = tree.node(focus).bytes.max(1);
        let share = tile.bytes as f64 * 100.0 / whole as f64;
        let (share, size) = (format!("{share:.0}"), format_size(tile.bytes));
        let of = self.path_of(&tree, focus).display().to_string();
        // TRANSLATORS: {0} is a folder or file, {1} its size, {n} its files, {2} its share in percent of folder {3}.
        match tile.what {
            What::Folder(id) => {
                let path = self.path_of(&tree, id).display().to_string();
                tn!(
                    "{0} · {1} · {n} file · {2}% of {3}" | "{0} · {1} · {n} files · {2}% of {3}",
                    tree.node(id).files,
                    path,
                    size,
                    share,
                    of
                )
            }
            What::File(f) => {
                let path = self.0.root.join(&tree.files[f].0).display().to_string();
                t!("{0} · {1} · {2}% of {3}", path, size, share, of)
            }
            What::Files(id) => {
                let path = self.path_of(&tree, id).display().to_string();
                // TRANSLATORS: a folder's own files (not its subfolders') as one block of the size map.
                tn!(
                    "{0} · its files · {1} · {n} file · {2}% of {3}"
                        | "{0} · its files · {1} · {n} files · {2}% of {3}",
                    tree.node(id).own_files,
                    path,
                    size,
                    share,
                    of
                )
            }
            What::Small(_) => t!("{0} · {1} · {2}% of {3}", name_of(&tree, tile.what), size, share, of),
        }
    }

    fn target(&self, i: usize) -> Option<Target> {
        let tile = self.0.tiles.borrow().get(i).copied()?;
        let tree = self.0.tree.borrow().clone();
        let tree = tree.lock().unwrap_or_else(PoisonError::into_inner);
        match tile.what {
            What::Folder(id) => Some(Target::Folder(self.path_of(&tree, id))),
            What::Files(id) => Some(Target::FilesOf(self.path_of(&tree, id))),
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
        self.0.zoomed.set(false);
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
        let at = self.tile_at(x, y);
        if self.0.zoomed.replace(false) && at.is_some() {
            let path = {
                let tree = self.0.tree.borrow().clone();
                let tree = tree.lock().unwrap_or_else(PoisonError::into_inner);
                self.path_of(&tree, self.0.focus.get())
            };
            self.close();
            return crate::panes::active_nav().go(Location::Path(path));
        }
        if let Some(i) = at {
            self.enter(i);
        }
    }

    fn enter(&self, i: usize) {
        let Some(target) = self.target(i) else { return };
        self.close();
        match target {
            Target::Folder(path) | Target::FilesOf(path) => crate::panes::active_nav().go(Location::Path(path)),
            Target::File(path) => crate::operations::with_current(|ops| ops.show_path_in_folder(&path, false)),
        }
    }

    /// The right-click menu's items for the rectangle at `x`, `y` (it becomes the current one).
    pub fn menu_items(&self, x: f32, y: f32) -> Vec<(u32, String, bool)> {
        let Some(i) = self.tile_at(x, y) else { return Vec::new() };
        self.0.current.set(Some(i));
        self.show_current();
        let Some(what) = self.0.tiles.borrow().get(i).map(|t| t.what) else { return Vec::new() };
        menu_ids(what).iter().map(|&id| (id, t("", menu_title(id)).into_owned(), true)).collect()
    }

    pub fn menu_chosen(&self, id: u32) {
        let Some(i) = self.0.current.get() else { return };
        let Some(what) = self.0.tiles.borrow().get(i).map(|t| t.what) else { return };
        if !menu_ids(what).contains(&id) {
            return;
        }
        let Some(target) = self.target(i) else { return };
        let (path, is_dir) = match &target {
            Target::Folder(path) | Target::FilesOf(path) => (path.clone(), true),
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
                // It leaves the map only once a job has taken it away (`job_finished`): Cancel
                // in the question, or a failed job, keeps it.
                self.0.trashing.borrow_mut().push(path.clone());
                crate::operations::with_current(|ops| ops.trash_paths(vec![path], false));
            }
            MAP_COPY_PATH => {
                crate::copy_path::copy(&crate::panes::active_view(), &[path], gezik_core::path_text::PathFormat::Full);
            }
            _ => {}
        }
    }

    /// A job ended: what was sent to the trash and is gone now leaves the map (no new scan),
    /// and the pane's folder sizes forget it and the folders above it (after `keep_totals`).
    pub fn job_finished(&self) {
        let gone: Vec<PathBuf> = {
            let mut trashing = self.0.trashing.borrow_mut();
            let (gone, kept) =
                std::mem::take(&mut *trashing).into_iter().partition(|p: &PathBuf| p.symlink_metadata().is_err());
            *trashing = kept;
            gone
        };
        if gone.is_empty() {
            return;
        }
        {
            let tree = self.0.tree.borrow().clone();
            let mut tree = tree.lock().unwrap_or_else(PoisonError::into_inner);
            for path in &gone {
                let Ok(under) = path.strip_prefix(&self.0.root) else { continue };
                let names: Vec<&str> = under.iter().filter_map(|n| n.to_str()).collect();
                let file = match names.as_slice() {
                    [name] => tree.files.iter().position(|(f, _)| f == name),
                    _ => None,
                };
                match (file, tree.find(&names)) {
                    (Some(f), _) => tree.remove_file(f),
                    (None, Some(id)) if id != 0 => tree.remove(id),
                    _ => {}
                }
            }
            tree.sum();
        }
        crate::panes::with_id(self.0.pane, |p| p.folder_sizes.forget(&gone));
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
                {
                    // The totals as far as it got, added up once; later ticks are dropped.
                    let tree = self.0.tree.borrow().clone();
                    tree.lock().unwrap_or_else(PoisonError::into_inner).sum();
                }
                self.0.scanning.set(false);
                self.0.stopped.set(true);
                self.0.current.set(None);
                self.redraw();
            }
            Key::Escape => self.close(),
            Key::Backspace => self.up(),
            Key::Up if chord.alt => self.up(),
            Key::Enter => {
                self.0.zoomed.set(false);
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
        What::Files(_) => t!("Files").into_owned(),
        What::Small(n) => tn!("{n} small item" | "{n} small items", n),
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
    fn a_files_block_cannot_be_trashed_or_shown() {
        assert_eq!(menu_ids(What::Files(3)), [MAP_OPEN, MAP_COPY_PATH], "its trash would take the folder");
        assert!(menu_ids(What::Folder(3)).contains(&MAP_TRASH) && menu_ids(What::File(0)).contains(&MAP_TRASH));
        assert!(menu_ids(What::Small(9)).is_empty());
    }

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
