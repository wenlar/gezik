//! Miller columns (spec 10 §7): a tab's column path in the columns view. Column 0 is the root,
//! the folder the tab went to; each column's selected folder opens the next column to its right.
//! The focused column's folder is the tab's location. Pure: the app reads each column's folder
//! and tells this what was selected; nothing here touches the disk.

use std::ops::Range;
use std::path::{Path, PathBuf};

use crate::nav::{History, Location};

/// How many of the focused column's ancestors keep their listings off screen (spec 10 §7.2).
pub const KEPT_ANCESTORS: usize = 8;

/// One column: a folder and the name selected in it (the row with the focus).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    pub folder: PathBuf,
    pub selected: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnPath {
    /// Never empty; `columns[i + 1].folder` is `columns[i].folder` joined with its selection.
    columns: Vec<Column>,
    focus: usize,
}

impl ColumnPath {
    /// A root change (address bar, sidebar, Back/Forward): the path starts over at `root`, whose
    /// ancestors are not shown (spec 10 §7.1).
    pub fn new(root: PathBuf) -> ColumnPath {
        ColumnPath { columns: vec![Column { folder: root, selected: None }], focus: 0 }
    }

    pub fn root(&self) -> &Path {
        &self.columns[0].folder
    }

    pub fn columns(&self) -> &[Column] {
        &self.columns
    }

    pub fn focus(&self) -> usize {
        self.focus
    }

    /// The tab's location: the focused column's folder.
    pub fn location(&self) -> &Path {
        &self.columns[self.focus].folder
    }

    /// A click on `name` in `column` (or ↑↓ there, `None` when nothing is selected): that column
    /// takes the focus, the columns right of it close, and a folder opens its own to its right.
    /// False for a column that is not shown.
    pub fn select(&mut self, column: usize, name: Option<String>, is_folder: bool) -> bool {
        if column >= self.columns.len() {
            return false;
        }
        self.columns.truncate(column + 1);
        self.focus = column;
        if let (Some(name), true) = (&name, is_folder) {
            let folder = self.columns[column].folder.join(name);
            self.columns.push(Column { folder, selected: None });
        }
        self.columns[column].selected = name;
        true
    }

    /// →: into the selected folder's column; its selection stays (the app selects the first
    /// item when there is none). False when the selection is no folder.
    pub fn enter(&mut self) -> bool {
        if self.focus + 1 >= self.columns.len() {
            return false;
        }
        self.focus += 1;
        true
    }

    /// ←: back to the column on the left, its folder still selected; the column left keeps
    /// showing but loses its selection, so the ones right of it close. A step up for sync
    /// browsing (spec 10 §4.7). False in the root column.
    pub fn leave(&mut self) -> bool {
        if self.focus == 0 {
            return false;
        }
        self.columns.truncate(self.focus + 1);
        self.columns[self.focus].selected = None;
        self.focus -= 1;
        true
    }

    /// Up: the root's parent becomes the root, with the old root selected in it, and takes the
    /// focus; the columns shown stay. False at a filesystem root (the app goes up the usual way).
    pub fn up(&mut self) -> bool {
        let root = self.root();
        let (Some(parent), Some(name)) = (root.parent(), root.file_name()) else { return false };
        let column = Column { folder: parent.to_path_buf(), selected: Some(name.to_string_lossy().into_owned()) };
        self.columns.insert(0, column);
        self.focus = 0;
        true
    }

    /// Whether `column`'s listing stays in memory while `visible` columns are on screen: those,
    /// and the focused column with its first `KEPT_ANCESTORS` ancestors (spec 10 §7.2).
    pub fn keeps(&self, column: usize, visible: Range<usize>) -> bool {
        visible.contains(&column) || (column <= self.focus && self.focus - column <= KEPT_ANCESTORS)
    }
}

/// Whether `location` can show as columns: only a folder has a chain of folders; search
/// results, a flat view, the trash and This PC show the list instead (spec 10 §7.3).
pub fn shows(location: &Location) -> bool {
    matches!(location, Location::Path(_))
}

/// A move between columns in the tab's history (spec 10 §7.1): the current entry now says the
/// focused folder, and nothing is added, so Back leaves the columns at once.
pub fn record(history: &mut History, path: &ColumnPath) {
    let location = Location::Path(path.location().to_path_buf());
    if *history.location() != location {
        history.replace(location);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(text: &str) -> PathBuf {
        PathBuf::from(text)
    }

    fn folders(path: &ColumnPath) -> Vec<PathBuf> {
        path.columns().iter().map(|c| c.folder.clone()).collect()
    }

    /// /a, with b selected in it and c in b (both folders); the focus on /a/b.
    fn deep() -> ColumnPath {
        let mut path = ColumnPath::new(p("/a"));
        assert!(path.select(0, Some("b".into()), true));
        assert!(path.select(1, Some("c".into()), true));
        path
    }

    #[test]
    fn a_new_path_is_its_root_alone() {
        let path = ColumnPath::new(p("/a"));
        assert_eq!((folders(&path), path.focus(), path.location()), (vec![p("/a")], 0, Path::new("/a")));
        assert_eq!(path.root(), Path::new("/a"));
    }

    #[test]
    fn a_click_on_a_folder_opens_its_column_and_a_file_none() {
        let mut path = deep();
        assert_eq!(folders(&path), [p("/a"), p("/a/b"), p("/a/b/c")]);
        assert_eq!((path.focus(), path.location()), (1, Path::new("/a/b")));
        // A file in the root column: the columns right of it close.
        assert!(path.select(0, Some("x.txt".into()), false));
        assert_eq!((folders(&path), path.focus()), (vec![p("/a")], 0));
        assert_eq!(path.columns()[0].selected.as_deref(), Some("x.txt"));
        assert!(path.select(0, None, false));
        assert_eq!(path.columns()[0].selected, None);
        assert!(!path.select(3, Some("z".into()), true), "no such column");
    }

    #[test]
    fn right_goes_into_the_selected_folder() {
        let mut path = deep();
        assert!(path.enter());
        assert_eq!((path.focus(), path.location()), (2, Path::new("/a/b/c")));
        assert!(!path.enter(), "nothing selected in /a/b/c");
        assert!(path.select(2, Some("f.txt".into()), false));
        assert!(!path.enter(), "a file is no column");
    }

    #[test]
    fn left_goes_back_and_closes_the_columns_right() {
        let mut path = deep();
        assert!(path.enter());
        assert!(path.select(2, Some("d".into()), true));
        assert_eq!(folders(&path).len(), 4);
        assert!(path.leave());
        assert_eq!((path.focus(), path.location()), (1, Path::new("/a/b")));
        assert_eq!(folders(&path), [p("/a"), p("/a/b"), p("/a/b/c")], "the column left stays, d's closes");
        assert_eq!(path.columns()[1].selected.as_deref(), Some("c"));
        assert_eq!(path.columns()[2].selected, None);
        assert!(path.leave());
        assert!(!path.leave(), "the root column");
        assert_eq!(path.location(), Path::new("/a"));
    }

    #[test]
    fn up_makes_the_roots_parent_the_root() {
        let mut path = deep();
        assert!(path.up());
        assert_eq!(folders(&path), [p("/"), p("/a"), p("/a/b"), p("/a/b/c")]);
        assert_eq!((path.focus(), path.location()), (0, Path::new("/")));
        assert_eq!(path.columns()[0].selected.as_deref(), Some("a"));
        assert!(!path.up(), "/ has no parent");
        assert!(path.enter());
        assert_eq!(path.location(), Path::new("/a"));
    }

    #[test]
    fn a_root_change_starts_over() {
        let mut path = deep();
        assert!(path.enter());
        path = ColumnPath::new(p("/z"));
        assert_eq!((folders(&path), path.focus()), (vec![p("/z")], 0));
    }

    #[test]
    fn moves_between_columns_add_no_history_entry() {
        let mut history = History::new(Location::Path(p("/before")));
        assert!(history.navigate(Location::Path(p("/a"))));
        let mut path = ColumnPath::new(p("/a"));
        for step in 0..3 {
            path.select(step, Some("n".into()), true);
            assert!(path.enter());
            record(&mut history, &path);
        }
        assert!(path.leave());
        record(&mut history, &path);
        assert_eq!(history.location(), &Location::Path(p("/a/n/n")));
        assert!(!history.can_go_forward());
        assert!(history.back());
        assert_eq!(history.location(), &Location::Path(p("/before")), "Back leaves the columns");
        assert!(!history.can_go_back());
    }

    #[test]
    fn the_shown_columns_and_eight_ancestors_keep_their_listings() {
        let mut path = ColumnPath::new(p("/0"));
        for column in 0..30 {
            path.select(column, Some("n".into()), true);
            assert!(path.enter());
        }
        assert_eq!((path.focus(), path.columns().len()), (30, 31));
        let visible = 27..31;
        let kept: Vec<usize> = (0..31).filter(|&c| path.keeps(c, visible.clone())).collect();
        assert_eq!(kept, (22..31).collect::<Vec<_>>(), "the focus and its 8 ancestors, the 4 shown among them");
        // Scrolled back to the left: what is shown again is kept again.
        let kept: Vec<usize> = (0..31).filter(|&c| path.keeps(c, 0..4)).collect();
        assert_eq!(kept, [0, 1, 2, 3, 22, 23, 24, 25, 26, 27, 28, 29, 30]);
        // A column right of the focus is kept only while shown.
        assert!(path.leave());
        assert!(path.keeps(30, 26..31) && !path.keeps(30, 0..4));
    }

    #[test]
    fn only_a_folder_shows_as_columns() {
        use crate::search::{Scope, SearchSpec};
        assert!(shows(&Location::Path(p("/a"))));
        for location in [
            Location::Drives,
            Location::Trash,
            Location::Flat(p("/a")),
            Location::Search(Box::new(SearchSpec::new(Scope::AllDrives))),
        ] {
            assert!(!shows(&location), "{location:?}");
        }
    }
}
