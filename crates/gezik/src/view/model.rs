//! The Slint model of the file view: one `ItemRow` per line, built only for lines on
//! screen. In the list a line holds one entry.

use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::rc::Rc;

use gezik_core::kind::{fallback_type_name, has_own_icon, own_type_name};
use gezik_core::selection::{PendingPress, Selection};
use gezik_core::view::{IconMode, SizeFormat, ViewMode};
use gezik_core::{Entry, format_size_in};
use slint::{Model, ModelNotify, ModelRc, ModelTracker, VecModel};

use super::listing::Listing;
use crate::media::{Media, MediaKey};
use crate::{FileRow, ItemRow};

/// What the view shows; read whenever Slint builds a line.
#[derive(Default)]
pub struct ViewData {
    /// What the view shows: `full` as the filter lets it through.
    pub listing: Listing,
    /// The folder's entries without the hidden files, sorted, before the filter; empty for
    /// "This PC" and the empty listing.
    pub full: Rc<Vec<gezik_core::Entry>>,
    /// Where each entry of `listing` is in `full` (in `results` for the results) while the
    /// filter shows a part of it; `None` when `listing` is all of it (or the drives).
    pub rows: Option<Vec<usize>>,
    /// The search results without the filter (`listing` shows what it lets through); `None`
    /// for a folder.
    pub results: Option<std::sync::Arc<gezik_search::results::ResultSet>>,
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
    /// Names in this folder (keys in the results) on the clipboard as cut: they look faded.
    pub cut: std::collections::HashSet<String>,
    /// `[view]`'s options in effect (view_options.rs).
    pub options: gezik_core::view::ViewOptions,
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
    let entry = listing.entry(i);
    let entry = entry.as_deref();
    let options = data.options;
    let now = std::time::SystemTime::now();
    let date = |time: Option<std::time::SystemTime>| {
        time.map(|time| gezik_platform::format_date(time, options.date_format, now)).unwrap_or_default()
    };
    let icon = picture_for(data, i);
    let name = listing.shown_name_at(i).unwrap_or_default();
    let entries = !matches!(listing, Listing::Drives(_));
    let (folder, found) = match listing {
        Listing::Results(set) => (
            set.shown_folder(i).unwrap_or_default().into(),
            set.found(i).map(|(line, text)| format!("{line}: {text}")).unwrap_or_default().into(),
        ),
        _ => (slint::SharedString::default(), slint::SharedString::default()),
    };
    FileRow {
        name: gezik_core::shown_name(name, is_dir, options.hide_extensions && entries).into(),
        is_dir,
        kind: listing.kind(i).index(),
        size: entry.filter(|_| entries).map(|e| size_cell(e, options.size_format)).unwrap_or_default().into(),
        size_stale: entries && entry.is_some_and(|e| e.is_dir && e.flags & Entry::SIZE_STALE != 0),
        modified: date(entry.and_then(|e| e.modified)).into(),
        created: date(entry.and_then(|e| e.created)).into(),
        type_name: type_name_for(data, i).into(),
        has_icon: icon.is_some(),
        icon: icon.unwrap_or_default(),
        selected: data.selection.is_selected(i),
        focused: data.selection.focus() == Some(i),
        cut: entries && listing.key_at(i).is_some_and(|key| data.cut.contains(&*key)),
        folder,
        found,
    }
}

/// A row's Size text: a file's size; a folder's total once known (`≥` when part of it could not
/// be read), `…` while it is on its way, else nothing (spec 6.2).
pub fn size_cell(e: &Entry, format: SizeFormat) -> String {
    match e.known_size() {
        Some(bytes) if e.is_dir && e.flags & Entry::SIZE_PARTIAL != 0 => format!("≥ {}", format_size_in(bytes, format)),
        Some(bytes) => format_size_in(bytes, format),
        None if e.size_pending() => "…".to_owned(),
        None => String::new(),
    }
}

/// The Type column: Gezik's own name for a split archive's part, else the system's name once
/// known, until then `PNG File`.
pub fn type_name_for(data: &ViewData, i: usize) -> String {
    if let Listing::Drives(_) = data.listing {
        return "Drive".to_owned();
    }
    match data.listing.entry(i) {
        Some(e) => own_type_name(&e.name, e.is_dir)
            .or_else(|| data.media.type_name(&e.extension().to_lowercase(), e.is_dir, Some(i)))
            .unwrap_or_else(|| fallback_type_name(&e.name, e.is_dir)),
        None => String::new(),
    }
}

/// In the grid with thumbnails on, a file's thumbnail once loaded (its icon until then).
fn picture_for(data: &ViewData, i: usize) -> Option<slint::Image> {
    if data.mode == ViewMode::Grid
        && data.thumbnails
        && let Some(e) = data.listing.entry(i)
        && !e.is_dir
        && (cfg!(windows) || gezik_platform::can_decode(e.extension()))
        && let Some((path, _)) = data.listing.path_at(i)
    {
        let key = MediaKey::Thumbnail { path, modified: e.modified, px: data.icon_px };
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
        listing => {
            let e = listing.entry(i)?;
            if e.is_dir {
                MediaKey::FolderIcon { path: listing.path_at(i)?.0, px }
            } else if has_own_icon(&e.name) {
                MediaKey::PathIcon { path: listing.path_at(i)?.0, px }
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
    use std::path::PathBuf;
    use std::sync::Arc;

    #[test]
    fn a_folders_size_cell_says_what_is_known() {
        let folder =
            |flags: u8, size: u64| Entry { name: "d".into(), is_dir: true, flags, size, modified: None, created: None };
        assert_eq!(size_cell(&folder(0, 4096), SizeFormat::Binary), "", "not sized: blank, as before");
        assert_eq!(size_cell(&folder(Entry::SIZE_PENDING, 0), SizeFormat::Binary), "…");
        assert_eq!(size_cell(&folder(Entry::SIZED, 1536), SizeFormat::Binary), "1.5 KB");
        assert_eq!(size_cell(&folder(Entry::SIZED | Entry::SIZE_PARTIAL, 1536), SizeFormat::Binary), "≥ 1.5 KB");
        assert_eq!(
            size_cell(&folder(Entry::SIZED | Entry::SIZE_STALE, 1536), SizeFormat::Binary),
            "1.5 KB",
            "old: drawn faint"
        );
    }

    #[test]
    fn trash_rows_show_the_name_and_place_they_had() {
        let set = crate::trash_view::to_set(vec![gezik_platform::trash::TrashItem {
            trashed: PathBuf::from("/bin").join("$RAB.txt"),
            info: None,
            name: "a.txt".into(),
            original: Some(PathBuf::from("/w").join("a.txt")),
            deleted: None,
            is_dir: false,
            size: 10,
        }]);
        let data = ViewData { listing: Listing::Results(Arc::new(set)), media: Media::idle(), ..Default::default() };
        let row = file_row(&data, 0);
        assert_eq!((row.name.as_str(), row.folder.as_str()), ("a.txt", "/w"));
    }

    #[test]
    fn result_rows_show_their_folder() {
        let data = ViewData {
            listing: super::super::listing::results(&[("sub", "a.txt"), ("", "b.txt")]),
            media: Media::idle(),
            ..Default::default()
        };
        let (a, b) = (file_row(&data, 0), file_row(&data, 1));
        assert_eq!((a.name.as_str(), a.folder.as_str(), a.size.as_str()), ("a.txt", "sub", "10 B"));
        assert_eq!(b.folder.as_str(), "", "the scope itself");
        assert_eq!(a.found.as_str(), "", "no content search");
    }

    #[test]
    fn rows_follow_the_view_options() {
        let mut data = ViewData {
            listing: super::super::listing::files("/x", &["sub.d/", "a.txt", ".gitignore", "a.tar.gz"]),
            media: Media::idle(),
            ..Default::default()
        };
        data.options.hide_extensions = true;
        let names: Vec<String> = (0..4).map(|i| file_row(&data, i).name.to_string()).collect();
        assert_eq!(names, ["sub.d/", "a", ".gitignore", "a.tar"], "folders and dot names keep theirs");
        data.options.hide_extensions = false;
        assert_eq!(file_row(&data, 1).name.as_str(), "a.txt");
        assert_eq!(file_row(&data, 1).size.as_str(), "10 B");
    }

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
