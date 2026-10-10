//! The sidebar's rows as Slint sees them (spec 10 §5.2): the places sidebar.rs builds (a few
//! dozen rows) with the open branches of the folder tree between them. A `slint::Model` that
//! builds a row only when asked: the `ListView` asks only for the rows on screen, so a branch
//! of 20,000 folders costs ~40 rows. The rows are a flat list laid out again only when the tree
//! or the places change, so a row costs the same however long the list is.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gezik_config::shortcuts::{Chord, Key};
use gezik_core::tree::{self, NodeId, RootKey, Tree};
use slint::{Model, ModelNotify, ModelTracker};

use crate::SidebarRow;
use crate::sidebar::{ICON_FOLDER, SECTION_TREE, SECTION_TREE_MORE, same_path};

/// A sidebar row: one of the places' rows (`Shown::base`) or a line of an open branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Line {
    Base(usize),
    Tree(tree::Line),
}

/// What the sidebar shows; shared by `Sidebar` (which changes it) and `SidebarModel` (which
/// Slint reads).
#[derive(Default)]
pub struct Shown {
    /// The sections and places, as sidebar.rs builds them (depth 0, no arrow).
    pub base: Vec<SidebarRow>,
    /// The tree root each base row stands for (a place with a folder), else `None`.
    pub keys: Vec<Option<RootKey>>,
    pub lines: Vec<Line>,
    /// The row of each base row.
    pub base_rows: Vec<usize>,
    pub tree: Tree,
    /// The folder shown, for the highlight.
    pub current: Option<PathBuf>,
    /// The tree version `lines` were laid out for; `None`: lay them out again.
    built: Option<u64>,
}

impl Shown {
    /// The places' rows (every rebuild of the sidebar): the rows are laid out again only if
    /// their places changed.
    pub fn set_base(&mut self, base: Vec<SidebarRow>, keys: Vec<Option<RootKey>>) {
        if keys != self.keys {
            self.built = None;
        }
        self.base = base;
        self.keys = keys;
    }

    /// The rows again, if the tree or the places changed since: each base row, then its
    /// place's open branch. Returns the rows before, when laid out again.
    pub fn relayout(&mut self) -> Option<Vec<Line>> {
        if self.built == Some(self.tree.version()) {
            return None;
        }
        let mut lines = Vec::with_capacity(self.base.len());
        let mut base_rows = Vec::with_capacity(self.keys.len());
        for (k, key) in self.keys.iter().enumerate() {
            base_rows.push(lines.len());
            lines.push(Line::Base(k));
            if let Some(root) = key.as_ref().and_then(|key| self.tree.root_of(key)) {
                lines.extend(self.tree.lines(root).into_iter().map(Line::Tree));
            }
        }
        self.base_rows = base_rows;
        self.built = Some(self.tree.version());
        Some(std::mem::replace(&mut self.lines, lines))
    }

    /// Row `i` as Slint draws it.
    pub fn row(&self, i: usize) -> Option<SidebarRow> {
        match *self.lines.get(i)? {
            Line::Base(k) => {
                let mut row = self.base.get(k)?.clone();
                if let Some(key) = self.keys.get(k)? {
                    row.arrow = self.tree.root_of(key).and_then(|id| self.tree.node(id)).map_or(1, |n| n.state.arrow());
                }
                Some(row)
            }
            Line::Tree(tree::Line::Node(id)) => Some(node_row(id, self.tree.node(id)?, self.current.as_deref())),
            Line::Tree(tree::Line::More(id)) => Some(more_row(id, self.tree.node(id)?)),
        }
    }

    /// Row `i` as the keys see it (no strings built).
    pub fn key_row(&self, i: usize) -> Option<KeyRow> {
        Some(match *self.lines.get(i)? {
            Line::Base(k) => {
                let row = self.base.get(k)?;
                let arrow = match self.keys.get(k)? {
                    Some(key) => {
                        self.tree.root_of(key).and_then(|id| self.tree.node(id)).map_or(1, |n| n.state.arrow())
                    }
                    None => 0,
                };
                KeyRow { selectable: !row.header, depth: 0, arrow }
            }
            Line::Tree(tree::Line::Node(id)) => {
                let node = self.tree.node(id)?;
                KeyRow { selectable: true, depth: i32::from(node.depth), arrow: node.state.arrow() }
            }
            Line::Tree(tree::Line::More(id)) => {
                KeyRow { selectable: true, depth: i32::from(self.tree.node(id)?.depth) + 1, arrow: 0 }
            }
        })
    }

    /// Row `i`'s name, and whether it can be chosen, for typing.
    pub fn label(&self, i: usize) -> Option<(bool, String)> {
        let row = self.row(i)?;
        Some((!row.header, row.label.to_string()))
    }

    /// The row of the folder shown, if the sidebar has it.
    // shortcut: builds each row to find the lit one, once per focus without a cursor; keep the lit row's index in Shown if it shows in a profile.
    pub fn active_row(&self) -> Option<usize> {
        (0..self.lines.len()).find(|i| self.row(*i).is_some_and(|row| row.active))
    }
}

fn index_of(id: NodeId) -> i32 {
    i32::try_from(id).unwrap_or(i32::MAX)
}

/// A folder of an open branch.
fn node_row(id: NodeId, node: &tree::Node, current: Option<&Path>) -> SidebarRow {
    SidebarRow {
        header: false,
        label: node.name().into(),
        section: SECTION_TREE,
        index: index_of(id),
        active: current.is_some_and(|c| same_path(c, &node.path)),
        tip: if node.state == tree::State::Loop { "Leads back to a folder above it".into() } else { "".into() },
        icon: ICON_FOLDER,
        depth: i32::from(node.depth),
        arrow: node.state.arrow(),
    }
}

/// The line closing a capped branch: a click opens the folder, where the list shows them all.
fn more_row(id: NodeId, node: &tree::Node) -> SidebarRow {
    SidebarRow {
        header: false,
        label: format!("… {} more (open the folder)", crate::preview::with_commas(node.more)).into(),
        section: SECTION_TREE_MORE,
        index: index_of(id),
        active: false,
        tip: "".into(),
        icon: 0,
        depth: i32::from(node.depth) + 1,
        arrow: 0,
    }
}

/// What Slint is told after a change.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Change {
    /// Rows that came or went: at row `.0`, `.1` removed, then `.2` added (a branch opened or
    /// closed is one such run, so the rows around it and the scroll stay).
    pub splice: Option<(usize, usize, usize)>,
    /// Rows kept but drawn again (rows of the new list): the places always, the folders
    /// `touched` says.
    pub rows: Vec<usize>,
}

/// What changed from `old` (`None`: not laid out again) to `new`. `touched` is asked only with
/// `scan` (something in the tree changed or the highlight moved), so a navigation with the
/// tree untouched costs the places' rows only.
pub fn change_plan(
    old: Option<&[Line]>,
    new: &[Line],
    base_rows: &[usize],
    touched: &dyn Fn(NodeId) -> bool,
    scan: bool,
) -> Change {
    let splice = old.filter(|old| *old != new).map(|old| {
        let head = old.iter().zip(new).take_while(|(a, b)| a == b).count();
        let room = old.len().min(new.len()) - head;
        let tail = old.iter().rev().zip(new.iter().rev()).take(room).take_while(|(a, b)| a == b).count();
        (head, old.len() - head - tail, new.len() - head - tail)
    });
    let fresh = |row: usize| splice.is_some_and(|(at, _, added)| (at..at + added).contains(&row));
    let mut rows: Vec<usize> = base_rows.iter().copied().filter(|row| !fresh(*row)).collect();
    if scan {
        rows.extend(new.iter().enumerate().filter_map(|(row, line)| match line {
            Line::Tree(tree::Line::Node(id)) if !fresh(row) && touched(*id) => Some(row),
            _ => None,
        }));
        rows.sort_unstable();
    }
    Change { splice, rows }
}

/// The base row at or after row `row` (`base_rows.len()` past the last): a branch's rows count
/// as the place after them, so a pin dropped there goes before that place (sapma 15).
pub fn base_at(base_rows: &[usize], row: usize) -> usize {
    base_rows.partition_point(|r| *r < row)
}

/// The row of base row `k`.
pub fn row_of_base(base_rows: &[usize], k: usize) -> Option<usize> {
    base_rows.get(k).copied()
}

/// The sidebar's scroll offset (content-y, zero or less) that shows row `row` whole, moving as
/// little as it can, for `height` shown.
pub fn scroll_to_show(row: usize, row_height: f32, scroll: f32, height: f32) -> f32 {
    let top = row as f32 * row_height;
    if top < -scroll {
        -top
    } else if top + row_height > -scroll + height {
        // A row taller than the sidebar shows its top.
        -(top + row_height - height).max(0.0).min(top)
    } else {
        scroll
    }
}

/// A key the sidebar tree takes while it has the keyboard (spec 10 §5.1, sapma 12).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SideKey {
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Open,
    OpenInTab,
    Leave,
    Menu,
}

/// What a key does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyDo {
    Move(usize),
    Toggle(usize),
    Go(usize),
    GoInTab(usize),
    Menu(usize),
    Leave,
    Nothing,
}

/// A row as the keys see it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyRow {
    /// Not a heading.
    pub selectable: bool,
    pub depth: i32,
    /// As `SidebarRow::arrow`.
    pub arrow: i32,
}

/// The tree key of `chord`: arrows, Home/End, PgUp/PgDn, Enter, Esc without modifiers, the
/// primary modifier (⌘ on a Mac) with Enter, and the menu key. Tab is none: it leaves.
pub fn side_key(chord: &Chord, menu_key: bool, mac: bool) -> Option<SideKey> {
    if menu_key {
        return Some(SideKey::Menu);
    }
    let primary = if mac { chord.meta } else { chord.ctrl };
    let other = if mac { chord.ctrl } else { chord.meta };
    if chord.alt || chord.shift || other {
        return None;
    }
    if primary {
        return (chord.key == Key::Enter).then_some(SideKey::OpenInTab);
    }
    Some(match chord.key {
        Key::Up => SideKey::Up,
        Key::Down => SideKey::Down,
        Key::Left => SideKey::Left,
        Key::Right => SideKey::Right,
        Key::Home => SideKey::Home,
        Key::End => SideKey::End,
        Key::PageUp => SideKey::PageUp,
        Key::PageDown => SideKey::PageDown,
        Key::Enter => SideKey::Open,
        Key::Escape => SideKey::Leave,
        _ => return None,
    })
}

/// What `key` does with the cursor on row `cursor` (none yet: the first place) among `count`
/// rows, `page` rows a screen.
pub fn key_do(
    rows: &dyn Fn(usize) -> Option<KeyRow>,
    count: usize,
    cursor: Option<usize>,
    key: SideKey,
    page: usize,
) -> KeyDo {
    let find = |from: isize, step: isize| -> Option<usize> {
        let mut i = from;
        while i >= 0 && (i as usize) < count {
            if rows(i as usize).is_some_and(|r| r.selectable) {
                return Some(i as usize);
            }
            i += step;
        }
        None
    };
    let to = |row: Option<usize>| row.map_or(KeyDo::Nothing, KeyDo::Move);
    if key == SideKey::Leave {
        return KeyDo::Leave;
    }
    let Some((at, row)) = cursor.filter(|c| *c < count).and_then(|c| Some((c, rows(c)?))) else {
        return to(find(0, 1));
    };
    let (at_i, page) = (at as isize, page.max(1) as isize);
    match key {
        SideKey::Up => to(find(at_i - 1, -1)),
        SideKey::Down => to(find(at_i + 1, 1)),
        SideKey::Home => to(find(0, 1)),
        SideKey::End => to(find(count as isize - 1, -1)),
        SideKey::PageUp => to(find((at_i - page).max(0), 1)),
        SideKey::PageDown => to(find((at_i + page).min(count as isize - 1), -1)),
        SideKey::Right => match row.arrow {
            1 => KeyDo::Toggle(at),
            2 => to(rows(at + 1).filter(|next| next.depth > row.depth).map(|_| at + 1)),
            _ => KeyDo::Nothing,
        },
        SideKey::Left => match row.arrow {
            2 | 3 => KeyDo::Toggle(at),
            _ if row.depth > 0 => to((0..at).rev().find(|i| rows(*i).is_some_and(|r| r.depth == row.depth - 1))),
            _ => KeyDo::Nothing,
        },
        SideKey::Open => KeyDo::Go(at),
        SideKey::OpenInTab => KeyDo::GoInTab(at),
        SideKey::Menu => KeyDo::Menu(at),
        SideKey::Leave => KeyDo::Leave,
    }
}

/// The row whose name starts with `typed` (lowercase), from the one after the cursor round to
/// it (with more than one letter typed, from the cursor's own row).
pub fn find_typed(
    label: &dyn Fn(usize) -> Option<(bool, String)>,
    count: usize,
    cursor: Option<usize>,
    typed: &str,
) -> Option<usize> {
    if count == 0 {
        return None;
    }
    let start = match cursor {
        Some(c) if typed.chars().count() > 1 => c,
        Some(c) => c + 1,
        None => 0,
    };
    (0..count).map(|k| (start + k) % count).find(|&i| {
        label(i).is_some_and(|(selectable, name)| selectable && crate::keys::starts_with_lowercase(&name, typed))
    })
}

/// The sidebar's rows for Slint, built when asked.
pub struct SidebarModel {
    shown: Rc<RefCell<Shown>>,
    notify: ModelNotify,
}

impl SidebarModel {
    pub fn new(shown: Rc<RefCell<Shown>>) -> SidebarModel {
        SidebarModel { shown, notify: ModelNotify::default() }
    }

    pub fn apply(&self, change: Change) {
        if let Some((at, removed, added)) = change.splice {
            if removed > 0 {
                self.notify.row_removed(at, removed);
            }
            if added > 0 {
                self.notify.row_added(at, added);
            }
        }
        change.rows.into_iter().for_each(|row| self.notify.row_changed(row));
    }
}

impl Model for SidebarModel {
    type Data = SidebarRow;

    // Never a second borrow (panic=abort): while the sidebar changes what it shows, nothing.
    fn row_count(&self) -> usize {
        self.shown.try_borrow().map_or(0, |shown| shown.lines.len())
    }

    fn row_data(&self, row: usize) -> Option<SidebarRow> {
        self.shown.try_borrow().ok()?.row(row)
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sidebar::{SECTION_DRIVES, SECTION_FOLDERS};
    use gezik_config::shortcuts::{Chord, Key};

    fn place(label: &str, section: i32) -> SidebarRow {
        SidebarRow {
            header: false,
            label: label.into(),
            section,
            index: 0,
            active: false,
            tip: "".into(),
            icon: ICON_FOLDER,
            depth: 0,
            arrow: 0,
        }
    }

    fn heading(label: &str) -> SidebarRow {
        SidebarRow { header: true, icon: 0, ..place(label, SECTION_FOLDERS) }
    }

    fn shown() -> Shown {
        let mut shown = Shown::default();
        shown.set_base(
            vec![heading("FOLDERS"), place("Home", SECTION_FOLDERS), heading("DRIVES"), place("C", SECTION_DRIVES)],
            vec![None, Some((SECTION_FOLDERS, PathBuf::from("/h"))), None, Some((SECTION_DRIVES, PathBuf::from("/c")))],
        );
        shown.relayout();
        shown
    }

    fn labels(s: &Shown) -> Vec<String> {
        (0..s.lines.len()).map(|i| s.row(i).unwrap().label.to_string()).collect()
    }

    #[test]
    fn a_closed_tree_is_the_places_with_their_arrows() {
        let mut s = shown();
        assert_eq!(s.lines, [Line::Base(0), Line::Base(1), Line::Base(2), Line::Base(3)]);
        assert_eq!(s.base_rows, [0, 1, 2, 3]);
        assert!(s.tree.is_empty(), "nothing read, nothing kept (spec 10 §5.4)");
        assert_eq!(s.row(1).unwrap().arrow, 1, "a place may have sub-folders");
        assert_eq!(s.row(0).unwrap().arrow, 0, "a heading has none");
        assert_eq!(s.relayout(), None, "nothing changed: nothing laid out again");
        let (base, keys) = (s.base.clone(), s.keys.clone());
        s.set_base(base.clone(), keys);
        assert_eq!(s.relayout(), None, "the same places (a navigation)");
        s.set_base(base, vec![None; 4]);
        assert!(s.relayout().is_some(), "other places");
        assert!(s.tree.is_empty());
    }

    #[test]
    fn an_open_branch_comes_under_its_place() {
        let mut s = shown();
        let read = s.tree.toggle_root((SECTION_FOLDERS, PathBuf::from("/h"))).unwrap();
        s.relayout();
        assert_eq!(s.row(1).unwrap().arrow, 3, "reading: the still sign");
        s.tree.loaded(&read, Some(tree::prepare(vec!["b".into(), "a".into()], None)));
        s.current = Some(PathBuf::from("/h/b"));
        s.relayout();
        assert_eq!(labels(&s), ["FOLDERS", "Home", "a", "b", "DRIVES", "C"]);
        assert_eq!(s.base_rows, [0, 1, 4, 5]);
        let b = s.row(3).unwrap();
        assert_eq!((b.section, b.depth, b.arrow, b.active, b.icon), (SECTION_TREE, 1, 1, true, ICON_FOLDER));
        assert_eq!(s.row(1).unwrap().arrow, 2);
    }

    #[test]
    fn a_capped_branch_ends_in_its_more_line() {
        let mut s = shown();
        let read = s.tree.toggle_root((SECTION_DRIVES, PathBuf::from("/c"))).unwrap();
        let many: Vec<String> = (0..tree::BRANCH_CAP + 31_204).map(|i| format!("d{i}")).collect();
        s.tree.loaded(&read, Some(tree::prepare(many, None)));
        s.relayout();
        let last = s.row(s.lines.len() - 1).unwrap();
        assert_eq!(last.label.as_str(), "… 31,204 more (open the folder)");
        assert_eq!((last.section, last.depth, last.arrow), (SECTION_TREE_MORE, 1, 0));
        assert_eq!(s.lines.len(), 4 + tree::BRANCH_CAP + 1);
    }

    #[test]
    fn pinned_rows_map_through_open_branches() {
        let mut s = shown();
        let read = s.tree.toggle_root((SECTION_FOLDERS, PathBuf::from("/h"))).unwrap();
        s.tree.loaded(&read, Some(tree::prepare(vec!["a".into(), "b".into()], None)));
        s.relayout();
        // Rows: 0 FOLDERS, 1 Home, 2 a, 3 b, 4 DRIVES, 5 C.
        assert_eq!(base_at(&s.base_rows, 1), 1);
        assert_eq!(base_at(&s.base_rows, 2), 2, "a branch's rows count as before the next place");
        assert_eq!(base_at(&s.base_rows, 3), 2);
        assert_eq!(base_at(&s.base_rows, 6), 4, "below the last row");
        assert_eq!(row_of_base(&s.base_rows, 2), Some(4));
        assert_eq!(row_of_base(&s.base_rows, 9), None);
    }

    #[test]
    fn the_model_redraws_rows_when_it_can_and_splices_when_rows_move() {
        let old = [Line::Base(0), Line::Tree(tree::Line::Node(5)), Line::Base(1)];
        let base_rows = [0, 2];
        let plan = |old: Option<&[Line]>, new: &[Line], base_rows: &[usize], touched: &dyn Fn(NodeId) -> bool| {
            change_plan(old, new, base_rows, touched, true)
        };
        assert_eq!(plan(None, &old, &base_rows, &|id| id == 5), Change { splice: None, rows: vec![0, 1, 2] });
        assert_eq!(
            plan(Some(&old), &old, &base_rows, &|_| false),
            Change { splice: None, rows: vec![0, 2] },
            "places always, folders when touched"
        );
        assert_eq!(
            change_plan(None, &old, &base_rows, &|_| panic!("not asked"), false).rows,
            [0, 2],
            "a navigation with the tree untouched looks at no folder"
        );
        let closed = [Line::Base(0), Line::Base(1)];
        assert_eq!(
            plan(Some(&old), &closed, &[0, 1], &|_| true),
            Change { splice: Some((1, 1, 0)), rows: vec![0, 1] },
            "a branch closed: its rows go, the rest stay"
        );
        let opened = [
            Line::Base(0),
            Line::Tree(tree::Line::Node(5)),
            Line::Tree(tree::Line::Node(7)),
            Line::Tree(tree::Line::Node(8)),
            Line::Base(1),
        ];
        assert_eq!(
            plan(Some(&old), &opened, &[0, 4], &|id| id == 5 || id == 7),
            Change { splice: Some((2, 0, 2)), rows: vec![0, 1, 4] },
            "a sub-branch opened: its rows come in, its folder is drawn again"
        );
        let moved = [Line::Base(0), Line::Tree(tree::Line::Node(6)), Line::Base(1)];
        assert_eq!(plan(Some(&old), &moved, &base_rows, &|_| false).splice, Some((1, 1, 1)));
    }

    #[test]
    fn a_row_scrolls_into_view_moving_as_little_as_it_can() {
        // 20 px rows, 200 px shown.
        assert_eq!(scroll_to_show(3, 20.0, 0.0, 200.0), 0.0, "already shown");
        assert_eq!(scroll_to_show(30, 20.0, 0.0, 200.0), -420.0, "below: it comes up to the bottom");
        assert_eq!(scroll_to_show(2, 20.0, -400.0, 200.0), -40.0, "above: it comes down to the top");
        assert_eq!(scroll_to_show(0, 20.0, 0.0, 10.0), 0.0, "a sidebar lower than a row");
    }

    #[test]
    fn rows_fall_back_while_borrowed() {
        let shown = Rc::new(RefCell::new(shown()));
        let model = SidebarModel::new(shown.clone());
        assert_eq!(model.row_count(), 4);
        assert_eq!(model.row_data(1).unwrap().label.as_str(), "Home");
        let _held = shown.borrow_mut();
        assert_eq!(model.row_count(), 0, "no second borrow (panic=abort)");
        assert!(model.row_data(1).is_none());
    }

    #[test]
    fn the_keys_see_the_rows_as_drawn() {
        let mut s = shown();
        let read = s.tree.toggle_root((SECTION_FOLDERS, PathBuf::from("/h"))).unwrap();
        s.tree.loaded(&read, Some(tree::prepare(vec!["a".into(), "b".into()], None)));
        s.current = Some(PathBuf::from("/h/b"));
        s.relayout();
        for i in 0..s.lines.len() {
            let (row, key) = (s.row(i).unwrap(), s.key_row(i).unwrap());
            assert_eq!((key.selectable, key.depth, key.arrow), (!row.header, row.depth, row.arrow), "row {i}");
            assert_eq!(s.label(i), Some((!row.header, row.label.to_string())));
        }
        assert_eq!(s.active_row(), Some(3), "/h/b");
        assert_eq!(s.key_row(s.lines.len()), None);
    }

    fn rows() -> Vec<KeyRow> {
        let r = |selectable, depth, arrow| KeyRow { selectable, depth, arrow };
        // 0 FOLDERS, 1 Home (open), 2 a (closed), 3 b (open), 4 b1, 5 DRIVES, 6 C (closed)
        vec![r(false, 0, 0), r(true, 0, 2), r(true, 1, 1), r(true, 1, 2), r(true, 2, 0), r(false, 0, 0), r(true, 0, 1)]
    }

    fn press(cursor: Option<usize>, key: SideKey) -> KeyDo {
        let rows = rows();
        key_do(&|i| rows.get(i).copied(), rows.len(), cursor, key, 3)
    }

    #[test]
    fn keys_walk_the_tree() {
        assert_eq!(press(None, SideKey::Down), KeyDo::Move(1), "no cursor: the first place");
        assert_eq!(press(Some(4), SideKey::Down), KeyDo::Move(6), "headings are skipped");
        assert_eq!(press(Some(1), SideKey::Up), KeyDo::Nothing, "nothing above the first place");
        assert_eq!(press(Some(6), SideKey::Down), KeyDo::Nothing, "nothing below the last");
        assert_eq!(press(Some(4), SideKey::Home), KeyDo::Move(1));
        assert_eq!(press(Some(1), SideKey::End), KeyDo::Move(6));
        assert_eq!(press(Some(1), SideKey::PageDown), KeyDo::Move(4));
        assert_eq!(press(Some(6), SideKey::PageUp), KeyDo::Move(3));
        assert_eq!(press(Some(1), SideKey::PageUp), KeyDo::Move(1), "a page up from the top stays");
        assert_eq!(press(Some(2), SideKey::Right), KeyDo::Toggle(2), "closed: it opens");
        assert_eq!(press(Some(3), SideKey::Right), KeyDo::Move(4), "open: down to its first folder");
        assert_eq!(press(Some(4), SideKey::Right), KeyDo::Nothing, "no arrow");
        assert_eq!(press(Some(3), SideKey::Left), KeyDo::Toggle(3), "open: it closes");
        assert_eq!(press(Some(4), SideKey::Left), KeyDo::Move(3), "up to its folder");
        assert_eq!(press(Some(2), SideKey::Left), KeyDo::Move(1));
        assert_eq!(press(Some(6), SideKey::Left), KeyDo::Nothing, "a closed place has no folder above");
        assert_eq!(press(Some(4), SideKey::Open), KeyDo::Go(4));
        assert_eq!(press(Some(4), SideKey::OpenInTab), KeyDo::GoInTab(4));
        assert_eq!(press(Some(4), SideKey::Menu), KeyDo::Menu(4));
        assert_eq!(press(Some(4), SideKey::Leave), KeyDo::Leave);
        assert_eq!(press(None, SideKey::Leave), KeyDo::Leave, "Esc always leaves");
        assert_eq!(press(Some(99), SideKey::Up), KeyDo::Move(1), "a cursor past the rows starts again");
        assert_eq!(key_do(&|_| None, 0, None, SideKey::Down, 3), KeyDo::Nothing, "no rows");
    }

    #[test]
    fn typing_finds_the_next_row() {
        let names = ["FOLDERS", "Home", "hidden", "Huge", "Hub", "DRIVES", "C:"];
        let label = |i: usize| names.get(i).map(|n| (i != 0 && i != 5, (*n).to_owned()));
        assert_eq!(find_typed(&label, 7, Some(1), "h"), Some(2), "one letter: the next one after the cursor");
        assert_eq!(find_typed(&label, 7, Some(4), "h"), Some(1), "round past the end");
        assert_eq!(find_typed(&label, 7, Some(3), "hu"), Some(3), "more letters: the cursor's row may stay");
        assert_eq!(find_typed(&label, 7, None, "d"), None, "a heading is not found");
        assert_eq!(find_typed(&label, 0, None, "a"), None);
    }

    #[test]
    fn a_chord_is_a_tree_key_only_without_modifiers() {
        let chord = |key, ctrl, meta, shift| Chord { ctrl, alt: false, shift, meta, key };
        assert_eq!(side_key(&chord(Key::Down, false, false, false), false, false), Some(SideKey::Down));
        assert_eq!(side_key(&chord(Key::Down, false, false, true), false, false), None, "Shift+Down is no tree key");
        assert_eq!(side_key(&chord(Key::Enter, true, false, false), false, false), Some(SideKey::OpenInTab));
        assert_eq!(side_key(&chord(Key::Enter, false, true, false), false, true), Some(SideKey::OpenInTab), "⌘Enter");
        assert_eq!(side_key(&chord(Key::Enter, true, false, false), false, true), None, "Ctrl+Enter on a Mac");
        assert_eq!(
            side_key(&chord(Key::Delete, false, false, false), false, false),
            None,
            "Delete deletes nothing here"
        );
        assert_eq!(side_key(&chord(Key::Tab, false, false, false), false, false), None, "Tab leaves the sidebar");
        assert_eq!(side_key(&chord(Key::Tab, false, false, true), false, false), None, "and Shift+Tab");
        assert_eq!(side_key(&chord(Key::F(10), false, false, true), true, false), Some(SideKey::Menu));
        assert_eq!(side_key(&chord(Key::Escape, false, false, false), false, false), Some(SideKey::Leave));
    }
}
