//! Dragging files: what is under the pointer, what dropping there would do, and what the
//! pointer says. Pure rules; the window geometry comes from the UI, in logical pixels.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::layout::{Geometry, Rect};
use crate::ops::paths::{is_within, lexical_roots, same_path};

/// How far the pointer moves (on either axis) before a press becomes a drag.
pub const THRESHOLD: f32 = 4.0;
/// How long a dragged item rests on a tab before the tab opens.
pub const TAB_HOVER: Duration = Duration::from_millis(600);
/// Near the list's top or bottom edge (this close), a drag scrolls it.
pub const EDGE: f32 = 24.0;

pub fn past_threshold(dx: f32, dy: f32) -> bool {
    dx.abs() > THRESHOLD || dy.abs() > THRESHOLD
}

/// The file list: its visible area (window coordinates), scroll offset (content-y, zero or
/// negative), the list or grid geometry, and how many entries it has.
#[derive(Debug, Clone, PartialEq)]
pub struct ListArea {
    pub rect: Rect,
    pub scroll: f32,
    pub geometry: Geometry,
    pub count: usize,
}

/// A sidebar row: a section title, a place, or the nth pinned folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SideRow {
    Header,
    Item,
    Pinned(usize),
}

/// The sidebar: its area, scroll offset (zero or negative), the space above the first row,
/// and its rows.
#[derive(Debug, Clone, PartialEq)]
pub struct SidebarArea {
    pub rect: Rect,
    pub scroll: f32,
    pub pad: f32,
    pub row_height: f32,
    pub rows: Vec<SideRow>,
}

/// The tab strip: its area, scroll offset (content-x, zero or negative), tab width and count.
#[derive(Debug, Clone, PartialEq)]
pub struct TabArea {
    pub rect: Rect,
    pub scroll: f32,
    pub tab_width: f32,
    pub count: usize,
}

/// The address bar: its area and where each path part is (x, width).
#[derive(Debug, Clone, PartialEq)]
pub struct CrumbArea {
    pub rect: Rect,
    pub spans: Vec<(f32, f32)>,
}

/// Where everything a drop can land on is.
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub width: f32,
    pub height: f32,
    pub list: ListArea,
    /// None while the sidebar is hidden.
    pub sidebar: Option<SidebarArea>,
    pub tabs: TabArea,
    pub crumbs: CrumbArea,
}

/// What is under the pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    /// Outside the window.
    Outside,
    /// Somewhere nothing can be dropped (a title, a button).
    Nothing,
    /// Entry n of the list.
    Entry(usize),
    /// The list's empty space: the folder shown.
    Background,
    /// Sidebar row n (a place).
    Sidebar(usize),
    /// Between pinned folders: pins at this position.
    PinAt(usize),
    Tab(usize),
    Crumb(usize),
}

/// What is at window point (`x`, `y`). `pin_zones`: the edges of pinned rows pin (only
/// folders can be pinned).
pub fn hit(layout: &Layout, x: f32, y: f32, pin_zones: bool) -> Hit {
    if !(x >= 0.0 && y >= 0.0 && x < layout.width && y < layout.height) {
        return Hit::Outside;
    }
    let tabs = &layout.tabs;
    if tabs.rect.contains(x, y) {
        return match index_at(x - tabs.rect.x - tabs.scroll, tabs.tab_width) {
            Some(i) if i < tabs.count => Hit::Tab(i),
            _ => Hit::Nothing,
        };
    }
    let crumbs = &layout.crumbs;
    if crumbs.rect.contains(x, y) {
        return crumbs
            .spans
            .iter()
            .position(|&(left, width)| x >= left && x < left + width)
            .map_or(Hit::Nothing, Hit::Crumb);
    }
    if let Some(side) = &layout.sidebar
        && side.rect.contains(x, y)
    {
        return sidebar_hit(side, y, pin_zones);
    }
    if layout.list.rect.contains(x, y) {
        return list_hit(&layout.list, x, y);
    }
    Hit::Nothing
}

/// Which slot of `size` holds `offset` (None before the first or with no size).
fn index_at(offset: f32, size: f32) -> Option<usize> {
    (offset >= 0.0 && size > 0.0).then(|| (offset / size).floor() as usize)
}

fn sidebar_hit(side: &SidebarArea, y: f32, pin_zones: bool) -> Hit {
    let offset = y - side.rect.y - side.scroll - side.pad;
    let Some(row) = index_at(offset, side.row_height) else { return Hit::Nothing };
    match side.rows.get(row) {
        None | Some(SideRow::Header) => Hit::Nothing,
        Some(SideRow::Item) => Hit::Sidebar(row),
        Some(SideRow::Pinned(i)) => {
            let within = (offset - row as f32 * side.row_height) / side.row_height;
            if pin_zones && within < 0.25 {
                Hit::PinAt(*i)
            } else if pin_zones && within > 0.75 {
                Hit::PinAt(i + 1)
            } else {
                Hit::Sidebar(row)
            }
        }
    }
}

/// Grid cells are inset this much: the gaps between them are the folder's empty space.
const CELL_INSET: f32 = 4.0;

fn list_hit(list: &ListArea, x: f32, y: f32) -> Hit {
    let content_y = y - list.rect.y - list.scroll;
    let Some(line) = index_at(content_y, list.geometry.row_height()) else { return Hit::Background };
    let index = match list.geometry {
        Geometry::List { .. } => Some(line),
        Geometry::Grid { cell_width, cell_height, columns } => {
            let local_x = x - list.rect.x;
            index_at(local_x, cell_width).filter(|&column| column < columns.max(1)).and_then(|column| {
                let (in_x, in_y) = (local_x - column as f32 * cell_width, content_y - line as f32 * cell_height);
                let inside = in_x >= CELL_INSET
                    && in_x < cell_width - CELL_INSET
                    && in_y >= CELL_INSET
                    && in_y < cell_height - CELL_INSET;
                inside.then_some(line * columns.max(1) + column)
            })
        }
    };
    match index {
        Some(i) if i < list.count => Hit::Entry(i),
        _ => Hit::Background,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    Copy,
    Move,
}

/// The keys held during a drag: Shift moves, the copy key (Ctrl; Option on macOS) copies.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Keys {
    pub shift: bool,
    pub copy: bool,
}

/// What the drag's source lets the target do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Allowed {
    pub copy: bool,
    pub move_: bool,
}

impl Allowed {
    pub const BOTH: Allowed = Allowed { copy: true, move_: true };
}

/// The effect of a drop: Shift moves, the copy key copies; otherwise a move on the same
/// drive and a copy to another. Limited to what the source allows (None: nothing is).
pub fn choose(keys: Keys, same_drive: bool, allowed: Allowed) -> Option<Effect> {
    let wanted = match (keys.shift, keys.copy) {
        (true, false) => Effect::Move,
        (false, true) => Effect::Copy,
        _ if same_drive => Effect::Move,
        _ => Effect::Copy,
    };
    let allows = |effect| match effect {
        Effect::Copy => allowed.copy,
        Effect::Move => allowed.move_,
    };
    let other = match wanted {
        Effect::Copy => Effect::Move,
        Effect::Move => Effect::Copy,
    };
    [wanted, other].into_iter().find(|&effect| allows(effect))
}

/// Whether dropping `sources` into folder `target` must be refused: onto one of them, into
/// one of them, or a move into the folder they are already in.
pub fn refuse(sources: &[PathBuf], target: &Path, effect: Effect) -> bool {
    if sources.iter().any(|source| is_within(target, source)) {
        return true;
    }
    effect == Effect::Move
        && !sources.is_empty()
        && sources.iter().all(|source| source.parent().is_some_and(|parent| same_path(parent, target)))
}

/// Whether `a` and `b` are on the same drive: the longest of `roots` (drive roots and mount
/// points) each is in; failing that, their roots as written.
pub fn same_drive(a: &Path, b: &Path, roots: &[PathBuf]) -> bool {
    let drive_of =
        |path: &Path| roots.iter().filter(|root| is_within(path, root)).max_by_key(|root| root.components().count());
    match (drive_of(a), drive_of(b)) {
        (Some(x), Some(y)) => same_path(x, y),
        _ => lexical_roots([a]) == lexical_roots([b]),
    }
}

/// What a drop does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Transfer(Effect),
    Pin,
    /// Onto a zip, 7z or tar file: the files are added to it.
    AddToArchive,
}

/// The text next to the dragged items; `folder` is the archive for `AddToArchive`.
pub fn label(action: Action, folder: &Path) -> String {
    match action {
        Action::Transfer(Effect::Move) => format!("Move to {}", folder_name(folder)),
        Action::Transfer(Effect::Copy) => format!("Copy to {}", folder_name(folder)),
        Action::Pin => "Pin to sidebar".to_owned(),
        Action::AddToArchive => format!("Add to {}", folder_name(folder)),
    }
}

/// A folder's name as shown: its last part, or the whole path for a root.
pub fn folder_name(path: &Path) -> String {
    match path.file_name() {
        Some(name) => name.to_string_lossy().into_owned(),
        None => path.display().to_string(),
    }
}

/// How much a drag at `y_in_list` (from the list's visible top) scrolls the list per step:
/// up (positive) near the top or above it, down near the bottom or below it.
pub fn edge_scroll(y_in_list: f32, visible: f32, step: f32) -> f32 {
    if y_in_list < EDGE {
        step
    } else if y_in_list > visible - EDGE {
        -step
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(count: usize) -> ListArea {
        ListArea {
            rect: Rect { x: 200.0, y: 100.0, width: 600.0, height: 400.0 },
            scroll: 0.0,
            geometry: Geometry::List { row_height: 20.0 },
            count,
        }
    }

    fn layout(list: ListArea) -> Layout {
        Layout {
            width: 1000.0,
            height: 700.0,
            list,
            sidebar: Some(SidebarArea {
                rect: Rect { x: 0.0, y: 100.0, width: 195.0, height: 400.0 },
                scroll: 0.0,
                pad: 4.0,
                row_height: 20.0,
                rows: vec![SideRow::Header, SideRow::Item, SideRow::Header, SideRow::Pinned(0), SideRow::Pinned(1)],
            }),
            tabs: TabArea {
                rect: Rect { x: 0.0, y: 0.0, width: 960.0, height: 30.0 },
                scroll: 0.0,
                tab_width: 100.0,
                count: 3,
            },
            crumbs: CrumbArea {
                rect: Rect { x: 300.0, y: 40.0, width: 500.0, height: 24.0 },
                spans: vec![(300.0, 60.0), (372.0, 40.0)],
            },
        }
    }

    #[test]
    fn threshold_is_four_pixels_on_either_axis() {
        assert!(!past_threshold(4.0, -4.0));
        assert!(past_threshold(4.5, 0.0));
        assert!(past_threshold(0.0, -5.0));
    }

    #[test]
    fn list_rows_and_the_space_below_them() {
        let l = layout(list(3));
        assert_eq!(hit(&l, 300.0, 105.0, false), Hit::Entry(0));
        assert_eq!(hit(&l, 300.0, 145.0, false), Hit::Entry(2));
        assert_eq!(hit(&l, 300.0, 170.0, false), Hit::Background);
    }

    #[test]
    fn scrolled_list_hits_the_right_entry() {
        let mut a = list(100);
        a.scroll = -200.0; // ten rows up
        assert_eq!(hit(&layout(a), 300.0, 105.0, false), Hit::Entry(10));
    }

    #[test]
    fn grid_gaps_and_empty_cells_are_the_background() {
        let a = ListArea { geometry: Geometry::Grid { cell_width: 100.0, cell_height: 120.0, columns: 5 }, ..list(7) };
        let l = layout(a);
        assert_eq!(hit(&l, 250.0, 150.0, false), Hit::Entry(0));
        assert_eq!(hit(&l, 202.0, 150.0, false), Hit::Background, "inside the 4px inset");
        assert_eq!(hit(&l, 350.0, 250.0, false), Hit::Entry(6), "second line, second column");
        assert_eq!(hit(&l, 650.0, 250.0, false), Hit::Background, "no 10th cell");
        assert_eq!(hit(&l, 760.0, 150.0, false), Hit::Background, "right of the last column");
    }

    #[test]
    fn sidebar_items_headers_and_pin_zones() {
        let l = layout(list(0));
        assert_eq!(hit(&l, 50.0, 100.0 + 4.0 + 10.0, true), Hit::Nothing, "header");
        assert_eq!(hit(&l, 50.0, 100.0 + 4.0 + 30.0, true), Hit::Sidebar(1));
        assert_eq!(hit(&l, 50.0, 100.0 + 4.0 + 61.0, true), Hit::PinAt(0), "top quarter of the first pin");
        assert_eq!(hit(&l, 50.0, 100.0 + 4.0 + 70.0, true), Hit::Sidebar(3));
        assert_eq!(hit(&l, 50.0, 100.0 + 4.0 + 99.0, true), Hit::PinAt(2), "bottom quarter of the last pin");
        assert_eq!(hit(&l, 50.0, 100.0 + 4.0 + 61.0, false), Hit::Sidebar(3), "files dragged: no pin zones");
        assert_eq!(hit(&l, 50.0, 100.0 + 4.0 + 150.0, true), Hit::Nothing, "below the rows");
    }

    #[test]
    fn a_scrolled_sidebar_moves_its_rows() {
        let mut l = layout(list(0));
        l.sidebar.as_mut().unwrap().scroll = -20.0;
        assert_eq!(hit(&l, 50.0, 100.0 + 4.0 + 10.0, true), Hit::Sidebar(1));
    }

    #[test]
    fn tabs_crumbs_and_outside() {
        let mut l = layout(list(0));
        assert_eq!(hit(&l, 150.0, 10.0, false), Hit::Tab(1));
        assert_eq!(hit(&l, 350.0, 10.0, false), Hit::Nothing, "past the last tab");
        assert_eq!(hit(&l, 380.0, 50.0, false), Hit::Crumb(1));
        assert_eq!(hit(&l, 365.0, 50.0, false), Hit::Nothing, "between parts");
        assert_eq!(hit(&l, -1.0, 50.0, false), Hit::Outside);
        assert_eq!(hit(&l, 500.0, 700.0, false), Hit::Outside);
        l.tabs.scroll = -100.0;
        assert_eq!(hit(&l, 150.0, 10.0, false), Hit::Tab(2), "a scrolled strip");
    }

    #[test]
    fn a_hidden_sidebar_leaves_nothing_there() {
        let mut l = layout(list(0));
        l.sidebar = None;
        assert_eq!(hit(&l, 50.0, 130.0, true), Hit::Nothing);
    }

    #[test]
    fn effect_follows_the_drive_and_the_keys() {
        let none = Keys { shift: false, copy: false };
        assert_eq!(choose(none, true, Allowed::BOTH), Some(Effect::Move));
        assert_eq!(choose(none, false, Allowed::BOTH), Some(Effect::Copy));
        assert_eq!(choose(Keys { shift: true, copy: false }, false, Allowed::BOTH), Some(Effect::Move));
        assert_eq!(choose(Keys { shift: false, copy: true }, true, Allowed::BOTH), Some(Effect::Copy));
        assert_eq!(choose(Keys { shift: true, copy: true }, true, Allowed::BOTH), Some(Effect::Move), "both: default");
        assert_eq!(choose(none, true, Allowed { copy: true, move_: false }), Some(Effect::Copy));
        assert_eq!(
            choose(Keys { shift: false, copy: true }, true, Allowed { copy: false, move_: true }),
            Some(Effect::Move)
        );
        assert_eq!(choose(none, true, Allowed { copy: false, move_: false }), None);
    }

    #[test]
    fn refuses_onto_itself_and_into_its_own_subfolder() {
        let a = PathBuf::from(if cfg!(windows) { r"C:\w\a" } else { "/w/a" });
        let w = a.parent().unwrap().to_path_buf();
        assert!(refuse(std::slice::from_ref(&a), &a, Effect::Copy));
        assert!(refuse(std::slice::from_ref(&a), &a.join("sub"), Effect::Move));
        assert!(refuse(std::slice::from_ref(&a), &w, Effect::Move), "moving into its own folder does nothing");
        assert!(!refuse(std::slice::from_ref(&a), &w, Effect::Copy), "copying there makes 'a (2)'");
        assert!(!refuse(std::slice::from_ref(&a), &w.join("b"), Effect::Move));
        assert!(!refuse(std::slice::from_ref(&a), &w.join("ab"), Effect::Move), "a sibling sharing a prefix");
    }

    #[test]
    fn drives_are_matched_by_their_longest_root() {
        let roots = if cfg!(windows) {
            vec![PathBuf::from(r"C:\"), PathBuf::from(r"D:\")]
        } else {
            vec![PathBuf::from("/"), PathBuf::from("/media/usb")]
        };
        let (a, b, c) =
            if cfg!(windows) { (r"C:\x", r"c:\y\z", r"D:\q") } else { ("/home/x", "/tmp/y", "/media/usb/q") };
        assert!(same_drive(Path::new(a), Path::new(b), &roots));
        assert!(!same_drive(Path::new(a), Path::new(c), &roots));
    }

    #[test]
    fn unknown_drives_fall_back_to_the_written_root() {
        if cfg!(windows) {
            assert!(same_drive(Path::new(r"\\srv\share\a"), Path::new(r"\\srv\share\b"), &[]));
            assert!(!same_drive(Path::new(r"\\srv\share\a"), Path::new(r"E:\b"), &[]));
        } else {
            assert!(same_drive(Path::new("/a"), Path::new("/b"), &[]));
        }
    }

    /// macOS: the startup disk is `/`, another volume is mounted under /Volumes.
    #[cfg(unix)]
    #[test]
    fn a_volume_mounted_under_the_root_is_another_drive() {
        let roots = [PathBuf::from("/"), PathBuf::from("/Volumes/Backup")];
        assert!(!same_drive(Path::new("/tmp/a.txt"), Path::new("/Volumes/Backup/x"), &roots));
        assert!(same_drive(Path::new("/tmp/a.txt"), Path::new("/Users/me"), &roots));
        assert!(same_drive(Path::new("/Volumes/Backup/a"), Path::new("/Volumes/Backup/b"), &roots));
    }

    #[test]
    fn labels_name_the_folder() {
        let p = PathBuf::from(if cfg!(windows) { r"C:\Users\Belgeler" } else { "/home/Belgeler" });
        assert_eq!(label(Action::Transfer(Effect::Move), &p), "Move to Belgeler");
        assert_eq!(label(Action::Pin, &p), "Pin to sidebar");
        assert_eq!(label(Action::AddToArchive, &p.join("Fotolar.zip")), "Add to Fotolar.zip");
        let root = PathBuf::from(if cfg!(windows) { r"D:\" } else { "/" });
        assert_eq!(label(Action::Transfer(Effect::Copy), &root), format!("Copy to {}", root.display()));
    }

    #[test]
    fn edges_scroll_the_list() {
        assert_eq!(edge_scroll(10.0, 400.0, 10.0), 10.0);
        assert_eq!(edge_scroll(390.0, 400.0, 10.0), -10.0);
        assert_eq!(edge_scroll(200.0, 400.0, 10.0), 0.0);
        assert_eq!(edge_scroll(-30.0, 400.0, 10.0), 10.0, "above the list still scrolls up");
    }
}
