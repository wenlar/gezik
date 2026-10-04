//! The Slint model of the file view: one `ItemRow` per line, built only for lines on
//! screen. In the list a line holds one entry.

use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::rc::Rc;

use gezik_core::format_size;
use gezik_core::selection::Selection;
use slint::{Model, ModelNotify, ModelRc, ModelTracker, VecModel};

use super::listing::Listing;
use crate::{FileRow, ItemRow};

/// What the view shows; read whenever Slint builds a line.
#[derive(Default)]
pub struct ViewData {
    pub listing: Listing,
    pub selection: Selection,
}

pub struct ItemsModel {
    data: Rc<RefCell<ViewData>>,
    /// Entries per line: 1 in the list.
    per_row: Cell<usize>,
    pub notify: ModelNotify,
}

impl ItemsModel {
    pub fn new(data: Rc<RefCell<ViewData>>) -> ItemsModel {
        ItemsModel { data, per_row: Cell::new(1), notify: ModelNotify::default() }
    }

    #[allow(dead_code, reason = "used by later view tasks (grid, preview)")]
    pub fn per_row(&self) -> usize {
        self.per_row.get()
    }

    /// Redraws the lines holding the entries in `rows`; everything when that is many.
    /// Returns whether the whole model was reset.
    pub fn entries_changed(&self, rows: &[Range<usize>]) -> bool {
        match notify_plan(rows, self.per_row.get()) {
            Plan::Reset => {
                self.notify.reset();
                true
            }
            Plan::Lines(lines) => {
                lines.into_iter().for_each(|line| self.notify.row_changed(line));
                false
            }
        }
    }
}

impl Model for ItemsModel {
    type Data = ItemRow;

    fn row_count(&self) -> usize {
        self.data.borrow().listing.len().div_ceil(self.per_row.get())
    }

    fn row_data(&self, line: usize) -> Option<ItemRow> {
        let data = self.data.borrow();
        let first = line * self.per_row.get();
        let len = data.listing.len();
        if first >= len {
            return None;
        }
        let cells: Vec<FileRow> = (first..(first + self.per_row.get()).min(len)).map(|i| file_row(&data, i)).collect();
        Some(ItemRow { first: i32::try_from(first).unwrap_or(i32::MAX), cells: ModelRc::new(VecModel::from(cells)) })
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Entry `i` as Slint shows it.
pub fn file_row(data: &ViewData, i: usize) -> FileRow {
    let listing = &data.listing;
    let is_dir = listing.is_dir(i);
    FileRow {
        name: listing.name_at(i).unwrap_or_default().into(),
        is_dir,
        kind: listing.kind(i).index(),
        size: match listing {
            Listing::Files(..) if !is_dir => format_size(listing.file_size(i)).into(),
            _ => "".into(),
        },
        selected: data.selection.is_selected(i),
        focused: data.selection.focus() == Some(i),
    }
}

/// More changed lines than this redraw the whole view at once (Ctrl+A in a huge folder)
/// instead of one notification per line.
const MAX_LINE_UPDATES: usize = 256;

#[derive(Debug, PartialEq, Eq)]
pub enum Plan {
    Reset,
    Lines(Vec<usize>),
}

/// Which lines to redraw for changed entry `rows` with `per_row` entries per line.
pub fn notify_plan(rows: &[Range<usize>], per_row: usize) -> Plan {
    let per_row = per_row.max(1);
    let mut lines: Vec<usize> = Vec::new();
    for range in rows.iter().filter(|r| !r.is_empty()) {
        let (first, last) = (range.start / per_row, (range.end - 1) / per_row);
        if lines.len() + (last - first + 1) > MAX_LINE_UPDATES {
            return Plan::Reset;
        }
        for line in first..=last {
            if !lines.contains(&line) {
                lines.push(line);
            }
        }
    }
    Plan::Lines(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_changes_redraw_their_lines() {
        assert_eq!(notify_plan(&[2..4, 9..10], 1), Plan::Lines(vec![2, 3, 9]));
        assert_eq!(notify_plan(&[2..4, 9..10], 4), Plan::Lines(vec![0, 2]));
        assert_eq!(notify_plan(&[], 1), Plan::Lines(vec![]));
    }

    #[test]
    #[allow(clippy::single_range_in_vec_init, reason = "one changed range of entries")]
    fn large_changes_reset_the_model() {
        assert_eq!(notify_plan(&[0..100_000], 1), Plan::Reset);
        assert_eq!(notify_plan(&[0..1000], 8), Plan::Lines((0..125).collect()));
    }

    #[test]
    fn rows_show_selection_focus_and_sizes() {
        let mut data =
            ViewData { listing: super::super::listing::files("/x", &["sub/", "a.txt"]), ..Default::default() };
        data.selection = Selection::from_indices(2, [1], Some(1));
        let (dir, file) = (file_row(&data, 0), file_row(&data, 1));
        assert!(dir.is_dir && dir.size.is_empty() && !dir.selected);
        assert_eq!(file.size.as_str(), "10 B");
        assert!(file.selected && file.focused);
        assert_eq!(file.kind, gezik_core::kind::Kind::Text.index());
    }
}
