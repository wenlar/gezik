//! The Slint model of the file view: one `ItemRow` per line, built only for lines on
//! screen. In the list a line holds one entry.

use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::rc::Rc;

use gezik_core::format_size;
use gezik_core::kind::{fallback_type_name, has_own_icon};
use gezik_core::selection::{PendingPress, Selection};
use gezik_core::view::{IconMode, ViewMode};
use slint::{Model, ModelNotify, ModelRc, ModelTracker, VecModel};

use super::listing::Listing;
use crate::media::{Media, MediaKey};
use crate::{FileRow, ItemRow};

/// What the view shows; read whenever Slint builds a line.
#[derive(Default)]
pub struct ViewData {
    pub listing: Listing,
    pub selection: Selection,
    /// The selection when a rubber-band drag started (Ctrl) or nothing; `None` when no drag.
    pub marquee_base: Option<Selection>,
    /// A press on a selected entry, waiting for its release.
    pub pending: PendingPress,
    pub media: Media,
    pub icons: IconMode,
    pub mode: ViewMode,
    /// Pictures and videos show a thumbnail in the grid.
    pub thumbnails: bool,
    /// The icon size to ask for, in physical pixels.
    pub icon_px: u32,
    /// Names in this folder on the clipboard as cut: they look faded.
    pub cut: std::collections::HashSet<String>,
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

    pub fn per_row(&self) -> usize {
        self.per_row.get()
    }

    /// Sets the entries per line (the grid's columns); the caller resets the model.
    pub fn set_per_row(&self, per_row: usize) {
        self.per_row.set(per_row.max(1));
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
    let entry = match listing {
        Listing::Files(_, entries) => entries.get(i),
        Listing::Drives(_) => None,
    };
    let date = |time: Option<std::time::SystemTime>| time.map(gezik_platform::format_datetime).unwrap_or_default();
    let icon = picture_for(data, i);
    FileRow {
        name: listing.name_at(i).unwrap_or_default().into(),
        is_dir,
        kind: listing.kind(i).index(),
        size: match listing {
            Listing::Files(..) if !is_dir => format_size(listing.file_size(i)).into(),
            _ => "".into(),
        },
        modified: date(entry.and_then(|e| e.modified)).into(),
        created: date(entry.and_then(|e| e.created)).into(),
        type_name: type_name_for(data, i).into(),
        has_icon: icon.is_some(),
        icon: icon.unwrap_or_default(),
        selected: data.selection.is_selected(i),
        focused: data.selection.focus() == Some(i),
        cut: matches!(listing, Listing::Files(..)) && listing.name_at(i).is_some_and(|name| data.cut.contains(name)),
    }
}

/// The Type column: the system's name once known, until then `PNG File`.
pub fn type_name_for(data: &ViewData, i: usize) -> String {
    match &data.listing {
        Listing::Files(_, entries) => match entries.get(i) {
            Some(e) => data
                .media
                .type_name(&e.extension().to_lowercase(), e.is_dir, Some(i))
                .unwrap_or_else(|| fallback_type_name(&e.name, e.is_dir)),
            None => String::new(),
        },
        Listing::Drives(_) => "Drive".to_owned(),
    }
}

/// In the grid with thumbnails on, a file's thumbnail once loaded (its icon until then).
fn picture_for(data: &ViewData, i: usize) -> Option<slint::Image> {
    if data.mode == ViewMode::Grid
        && data.thumbnails
        && let Listing::Files(dir, entries) = &data.listing
        && let Some(e) = entries.get(i)
        && !e.is_dir
        && (cfg!(windows) || gezik_platform::can_decode(e.extension()))
    {
        let key = MediaKey::Thumbnail { path: dir.join(&e.name), modified: e.modified, px: data.icon_px };
        if let Some(picture) = data.media.picture(key, i) {
            return Some(picture);
        }
    }
    icon_for(data, i)
}

/// The system icon of entry `i`, if loaded (it is requested otherwise). `None` with Gezik's
/// own icons.
fn icon_for(data: &ViewData, i: usize) -> Option<slint::Image> {
    if data.icons == IconMode::Gezik {
        return None;
    }
    let px = data.icon_px;
    let key = match &data.listing {
        Listing::Drives(drives) => MediaKey::PathIcon { path: drives.get(i)?.path.clone(), px },
        Listing::Files(dir, entries) => {
            let e = entries.get(i)?;
            if e.is_dir {
                MediaKey::FolderIcon { path: dir.join(&e.name), px }
            } else if has_own_icon(&e.name) {
                MediaKey::PathIcon { path: dir.join(&e.name), px }
            } else {
                MediaKey::ExtIcon { ext: e.extension().to_lowercase(), px }
            }
        }
    };
    data.media.picture(key, i)
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
        let mut data = ViewData {
            listing: super::super::listing::files("/x", &["sub/", "a.txt"]),
            media: Media::idle(),
            ..Default::default()
        };
        data.selection = Selection::from_indices(2, [1], Some(1));
        let (dir, file) = (file_row(&data, 0), file_row(&data, 1));
        assert!(dir.is_dir && dir.size.is_empty() && !dir.selected);
        assert_eq!(file.size.as_str(), "10 B");
        assert!(file.selected && file.focused);
        assert_eq!(file.kind, gezik_core::kind::Kind::Text.index());
    }

    #[test]
    fn gezik_icons_ask_for_nothing() {
        let data = ViewData {
            listing: super::super::listing::files("/x", &["a.txt"]),
            media: Media::idle(),
            icons: IconMode::Gezik,
            ..Default::default()
        };
        let row = file_row(&data, 0);
        assert!(!row.has_icon);
        assert_eq!(
            row.type_name.as_str(),
            gezik_core::kind::fallback_type_name("a.txt", false),
            "the system name is still asked for"
        );
    }
}
