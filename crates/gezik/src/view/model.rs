//! The Slint model of the file view: one `ItemRow` per line, built only for lines on
//! screen. In the list a line holds one entry.

use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::rc::Rc;

use gezik_core::fields::{FieldId, FieldSet, Fields};
use gezik_core::group::GroupKey;
use gezik_core::kind::{IconLookup, fallback_type_name, icon_lookup, own_type_name};
use gezik_core::layout::{Line, Lines, Span};
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
    /// The groups shown (spec 10 §6), in list order; empty when the view is not grouped.
    /// Per group, never per entry (spec 10 §6.3).
    pub groups: Vec<Span>,
    /// Each group's key, as `groups`.
    pub group_keys: Vec<GroupKey>,
    /// The folder's tags read whole (spec 12 §3.4): only while a sort, grouping or filter needs them.
    pub folder_tags: Option<Rc<super::FolderTags>>,
    /// The Tags column shows: rows on screen read their tags.
    pub tags_column: bool,
    /// The Rating and Comment columns show (spec 12 §8.2): rows on screen read their meta.
    pub rating_column: bool,
    pub comment_column: bool,
    /// The field columns shown (spec 13 §8.1), in their order: rows on screen read them.
    pub field_columns: Vec<FieldId>,
    /// The folder's fields read whole (spec 13 §7.5): only while a sort, grouping or filter
    /// needs them.
    pub folder_fields: Option<Rc<super::folder_fields::FolderFields>>,
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
        let plan = notify_plan(rows, self.per_row.get(), &self.data.borrow().groups);
        match plan {
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

    /// Redraws `lines` (model rows), those past the end left out; never a reset. Returns how
    /// many were redrawn.
    pub fn lines_changed(&self, lines: Range<usize>) -> usize {
        let lines = lines.start..lines.end.min(self.row_count());
        lines.clone().for_each(|line| self.notify.row_changed(line));
        lines.len()
    }

    /// The line entry `index` is on and its column; `None` in a closed group or past the end.
    /// While the view's data is borrowed (a binding read during a change) it answers as
    /// without groups, never with a panic.
    pub fn place_of(&self, index: usize) -> Option<(usize, usize)> {
        let per_row = self.per_row.get();
        match self.data.try_borrow() {
            Ok(data) if !data.groups.is_empty() => Lines { spans: &data.groups, per_row }.line_of(index),
            Ok(data) => (index < data.listing.len()).then_some((index / per_row, index % per_row)),
            Err(_) => Some((index / per_row, index % per_row)),
        }
    }
}

impl Model for ItemsModel {
    type Data = ItemRow;

    fn row_count(&self) -> usize {
        let data = self.data.borrow();
        let per_row = self.per_row.get();
        if data.groups.is_empty() {
            return data.listing.len().div_ceil(per_row);
        }
        Lines { spans: &data.groups, per_row }.count()
    }

    fn row_data(&self, line: usize) -> Option<ItemRow> {
        let data = self.data.borrow();
        let per_row = self.per_row.get();
        let len = data.listing.len();
        let cells = if data.groups.is_empty() {
            let first = line * per_row;
            first..(first + per_row).min(len)
        } else {
            match (Lines { spans: &data.groups, per_row }).line(line)? {
                Line::Header(g) => return header_row(&data, g),
                Line::Cells(cells) => cells.start..cells.end.min(len),
            }
        };
        if cells.is_empty() {
            return None;
        }
        let first = i32::try_from(cells.start).unwrap_or(i32::MAX);
        let cells: Vec<FileRow> = cells.map(|i| file_row(&data, i)).collect();
        Some(ItemRow {
            first,
            cells: ModelRc::new(VecModel::from(cells)),
            header: Default::default(),
            collapsed: false,
        })
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Group `g`'s header line: its name and how many entries it has (`Today (12)`; `GroupKey::header`).
fn header_row(data: &ViewData, g: usize) -> Option<ItemRow> {
    let (span, key) = (data.groups.get(g)?, data.group_keys.get(g)?);
    (span.start < data.listing.len()).then(|| ItemRow {
        first: i32::try_from(span.start).unwrap_or(i32::MAX),
        cells: ModelRc::default(),
        header: key.header(span.len).into(),
        collapsed: span.collapsed,
    })
}

/// Rows in closed groups are never selected (spec 10 §6.2): they leave the selection, and a
/// focus on one moves to the next row shown (the last one before it at the end; none if none
/// shows). Returns the rows to draw again.
pub fn hide_collapsed(data: &mut ViewData) -> Vec<Range<usize>> {
    let closed: Vec<Range<usize>> =
        data.groups.iter().filter(|s| s.collapsed).map(|s| s.start..s.start + s.len).collect();
    if closed.is_empty() {
        return Vec::new();
    }
    let mut changes = Vec::new();
    if data.selection.count() > 0 {
        for range in &closed {
            changes.extend(data.selection.unselect(range.clone()));
        }
    }
    if let Some(focus) = data.selection.focus().filter(|f| closed.iter().any(|r| r.contains(f))) {
        let open = || data.groups.iter().filter(|s| !s.collapsed);
        let next = open()
            .find(|s| s.start > focus)
            .map(|s| s.start)
            .or_else(|| open().rfind(|s| s.start < focus).map(|s| s.start + s.len - 1));
        data.selection = std::mem::take(&mut data.selection).focused_at(next);
        changes.push(focus..focus + 1);
        changes.extend(next.map(|n| n..n + 1));
    }
    changes
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
    // macOS: Finder's name for a folder right under the home folder or `/` (spec 9 §4.2);
    // sorting, filtering and renaming keep the real one.
    let finder = (cfg!(target_os = "macos") && is_dir && entries)
        .then(|| listing.path_at(i).and_then(|(path, _)| gezik_platform::finder::finder_name(&path)))
        .flatten();
    let RowCells {
        dots: [dot1, dot2, dot3],
        count: dots,
        more,
        tag_names,
        rating,
        rating_faint,
        comment,
        meta,
        pending,
    } = row_meta(data, i);
    let selected = data.selection.is_selected(i);
    let (tint, bold) = match entry.filter(|_| entries) {
        Some(e) => {
            use gezik_core::highlight::RowMeta;
            let known = match (&meta, pending) {
                (Some(meta), _) => RowMeta::Known(meta),
                (None, true) => RowMeta::Pending,
                (None, false) => RowMeta::Never,
            };
            crate::tags::highlight_of(e, now, known, selected)
        }
        None => Default::default(),
    };
    let (folder, found) = match listing {
        Listing::Results(set) => (
            set.shown_folder(i).unwrap_or_default().into(),
            set.found(i).map(|(line, text)| format!("{line}: {text}")).unwrap_or_default().into(),
        ),
        _ => (slint::SharedString::default(), slint::SharedString::default()),
    };
    FileRow {
        name: finder
            .as_deref()
            .unwrap_or_else(|| gezik_core::shown_name(name, is_dir, options.hide_extensions && entries))
            .into(),
        is_dir,
        kind: listing.kind(i).index(),
        size: entry.filter(|_| entries).map(|e| size_cell(e, options.size_format)).unwrap_or_default().into(),
        size_stale: entries && entry.is_some_and(|e| e.is_dir && e.flags & Entry::SIZE_STALE != 0),
        modified: date(entry.and_then(|e| e.modified)).into(),
        created: date(entry.and_then(|e| e.created)).into(),
        type_name: type_name_for(data, i).into(),
        has_icon: icon.is_some(),
        icon: icon.unwrap_or_default(),
        selected,
        focused: data.selection.focus() == Some(i),
        cut: entries
            && (listing.key_at(i).is_some_and(|key| data.cut.contains(&*key)) || waiting(data) || waiting_fields(data)),
        cloud: cloud_state_at(data, i).map_or(0, |s| s as i32),
        folder,
        found,
        dot1,
        dot2,
        dot3,
        dots,
        more,
        tag_names: tag_names.into(),
        rating: rating.into(),
        rating_faint,
        comment: comment.into(),
        tint,
        bold,
        fields: field_cells(data, i),
    }
}

/// Entry `i`'s Tags, Rating or Comment cell (`key`) as far as its meta is known (an export:
/// nothing is read for it).
pub fn known_meta_cell(data: &ViewData, i: usize, key: gezik_core::view::ColumnKey) -> String {
    use gezik_core::view::ColumnKey;
    let cell = |meta: &gezik_core::meta::Meta| match key {
        ColumnKey::Tags => meta.tags.iter().map(|t| &*t.name).collect::<Vec<_>>().join(", "),
        ColumnKey::Rating => gezik_core::meta::stars_text(meta.rating),
        _ => meta.comment.to_string(),
    };
    match (&data.listing, &data.folder_tags) {
        (Listing::Files(dir, entries), Some(f)) if f.dir == *dir => {
            entries.get(i).and_then(|e| f.of(&e.name)).map(cell)
        }
        _ => data.listing.path_at(i).and_then(|(path, _)| data.media.known_tags(&path)).map(|row| cell(&row.meta)),
    }
    .unwrap_or_default()
}

/// Whether a `tag:` filter waits for the folder's tags: the rows it lets through meanwhile are
/// drawn faint (spec 12 §6.3).
fn waiting(data: &ViewData) -> bool {
    data.folder_tags.as_ref().is_some_and(|f| f.faint.get() && !f.complete)
}

/// What a row shows of its meta: up to three dots, how many, how many more, the tags' names,
/// the stars (faint: Explorer's), the comment.
#[derive(Default)]
struct RowCells {
    dots: [slint::Color; 3],
    count: i32,
    more: i32,
    tag_names: String,
    rating: String,
    rating_faint: bool,
    comment: String,
    /// The meta, for highlight rules that ask for it (spec 12 §9.2); `pending`: it is being read.
    meta: Option<gezik_core::meta::Meta>,
    pending: bool,
}

/// Entry `i`'s tag dots while dots show (spec 12 §5.4), its tags' names while they or the Tags
/// column show (§6.1), its stars and comment while their columns show (§8.2): asked for only
/// here, so only for rows on screen; until read, none. A folder read whole answers without
/// asking. Cloud folders are read only with `dots = true`. On Windows, with the Tags or Rating
/// column, Explorer's Tags and stars of the kinds that keep them, in files not only in the
/// cloud (§8.3): after Gezik's tags in brackets, and faint stars when Gezik has none.
fn row_meta(data: &ViewData, i: usize) -> RowCells {
    let dots_on = crate::tags::dots_on();
    let columns = data.tags_column || data.rating_column || data.comment_column;
    let highlight = crate::tags::highlight_needs_meta();
    if matches!(data.listing, Listing::Drives(_)) || !(dots_on || columns || highlight) {
        return RowCells::default();
    }
    let entry = data.listing.entry(i);
    let explorer = cfg!(windows)
        && (data.tags_column || data.rating_column)
        && entry
            .as_deref()
            .is_some_and(|e| !e.is_dir && e.flags & Entry::CLOUD_ONLY == 0 && gezik_core::meta::explorer_kind(&e.name));
    let whole = match (&data.listing, &data.folder_tags) {
        (Listing::Files(dir, _), Some(f)) if f.dir == *dir => entry.as_deref().and_then(|e| f.of(&e.name)),
        _ => None,
    };
    let path = (whole.is_none() || explorer).then(|| data.listing.path_at(i)).flatten().map(|(path, _)| path);
    // Cloud folders are not read row by row (a folder read whole was asked for: it answers).
    let cloud = path.as_ref().is_some_and(|p| !crate::tags::dots_forced() && crate::cloud::root_of(p).is_some());
    let windows = path.as_deref().filter(|_| explorer && !cloud).and_then(crate::tags::explorer_of);
    let asked = whole.is_none() && !cloud && path.is_some();
    let row = match path {
        Some(path) if whole.is_none() && !cloud => data.media.tags(path, i),
        _ => None,
    };
    let Some(meta) = whole.or(row.as_deref().map(|r| &r.meta)) else {
        // Asked for and on its way (a row not read, as in a cloud folder, never comes).
        return RowCells { pending: asked, ..RowCells::default() };
    };
    let windows = windows.as_ref();
    let mut cells = RowCells { meta: highlight.then(|| meta.clone()), ..RowCells::default() };
    if !meta.tags.is_empty() {
        let (colors, more, names) = crate::tags::with_list(|list| crate::tags::dots_of(&meta.tags, list));
        if dots_on {
            cells.dots.iter_mut().zip(&colors).for_each(|(dot, color)| *dot = *color);
            (cells.count, cells.more) = (i32::try_from(colors.len()).unwrap_or(3), more);
        }
        cells.tag_names = names;
    }
    if let Some(keywords) = windows.map(|w| &w.keywords).filter(|k| data.tags_column && !k.is_empty()) {
        let gap = if cells.tag_names.is_empty() { "" } else { " " };
        cells.tag_names = format!("{}{gap}({keywords})", cells.tag_names);
    }
    if data.rating_column {
        let explorers = windows.map_or(0, |w| w.stars);
        cells.rating_faint = meta.rating == 0 && explorers > 0;
        cells.rating = gezik_core::meta::stars_text(if meta.rating > 0 { meta.rating } else { explorers });
    }
    if data.comment_column {
        cells.comment = meta.comment.to_string();
    }
    cells
}

/// Whether a property filter waits for the folder's fields: the rows it lets through meanwhile
/// are drawn faint (spec 13 §8.3).
fn waiting_fields(data: &ViewData) -> bool {
    data.folder_fields.as_ref().is_some_and(|f| f.faint.get() && !f.complete)
}

/// Entry `i`'s field cells while field columns show in the list (spec 13 §8.1): asked for only
/// here, so only for rows on screen; empty until read.
fn field_cells(data: &ViewData, i: usize) -> ModelRc<slint::SharedString> {
    if data.field_columns.is_empty() || data.mode != ViewMode::List {
        return ModelRc::default();
    }
    let values = row_fields(data, i, true);
    let cells: Vec<slint::SharedString> = data
        .field_columns
        .iter()
        .map(|id| values.as_ref().and_then(|v| v.text(*id)).unwrap_or_default().into())
        .collect();
    ModelRc::new(VecModel::from(cells))
}

/// Field `id` of entry `i` as far as known (an export: nothing is read for it).
pub fn known_field(data: &ViewData, i: usize, id: FieldId) -> String {
    row_fields(data, i, false).and_then(|v| v.text(id)).unwrap_or_default()
}

/// Entry `i`'s field values as far as known: from the folder read whole, else from the rows'
/// cache; with `ask`, the fields of the columns its readers give and not read yet are asked for
/// (a `.txt` asks nothing). Folders have none.
fn row_fields(data: &ViewData, i: usize, ask: bool) -> Option<Rc<Fields>> {
    if matches!(data.listing, Listing::Drives(_)) || data.listing.is_dir(i) {
        return None;
    }
    let want = FieldSet::of(&data.field_columns);
    if let (Listing::Files(dir, entries), Some(f)) = (&data.listing, &data.folder_fields)
        && f.dir == *dir
        && f.read.covers(want)
        && let Some(values) = entries.get(i).and_then(|e| f.of(&e.name))
    {
        return Some(values);
    }
    let (path, _) = data.listing.path_at(i)?;
    if !ask {
        return data.media.known_fields(&path);
    }
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase();
    let want = want.and(gezik_core::fields::provided(&ext));
    if want.is_empty() {
        return None;
    }
    data.media.fields(path, want, i)
}

/// The cloud state of entry `i` when its folder is under a cloud root (spec 9 §7.3): from the
/// flags the folder read gave, no call of its own. Folder listings only.
pub fn cloud_state_at(data: &ViewData, i: usize) -> Option<gezik_core::CloudState> {
    let Listing::Files(dir, entries) = &data.listing else { return None };
    if !crate::cloud::shows_state(dir) {
        return None;
    }
    entries.get(i).map(Entry::cloud_state)
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

/// In the grid with thumbnails on, a file's thumbnail once loaded (its icon until then): the
/// system's on Windows and macOS, Linux's thumbnail cache, else Gezik's own formats. Asked
/// only for the cells on screen; reads only the file's metadata unless Gezik decodes it.
fn picture_for(data: &ViewData, i: usize) -> Option<slint::Image> {
    if data.mode == ViewMode::Grid
        && data.thumbnails
        && let Some(e) = data.listing.entry(i)
        && !e.is_dir
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
            match lookup_for(&e, cfg!(target_os = "macos")) {
                IconLookup::Folder => MediaKey::FolderIcon { path: listing.path_at(i)?.0, px },
                IconLookup::Path => MediaKey::PathIcon { path: listing.path_at(i)?.0, px },
                IconLookup::Type => MediaKey::ExtIcon { ext: e.extension().to_lowercase(), px },
            }
        }
    };
    data.media.picture(key, i)
}

/// How entry `e`'s system icon is looked up: a cloud-only item never by its own path, since
/// the shell would read (download) it for its icon.
fn lookup_for(e: &Entry, mac: bool) -> IconLookup {
    match icon_lookup(&e.name, e.is_dir, mac) {
        IconLookup::Path if e.flags & Entry::CLOUD_ONLY != 0 => {
            if e.is_dir {
                IconLookup::Folder
            } else {
                IconLookup::Type
            }
        }
        lookup => lookup,
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

/// Which lines to redraw for changed entry `rows` with `per_row` entries per line, in a view
/// grouped as `groups` (empty: not grouped). A closed group's rows are on no line.
pub fn notify_plan(rows: &[Range<usize>], per_row: usize, groups: &[Span]) -> Plan {
    let per_row = per_row.max(1);
    if !groups.is_empty() {
        if rows.iter().map(|r| r.len()).sum::<usize>() > MAX_LINE_UPDATES * per_row {
            return Plan::Reset;
        }
        let lines_of = Lines { spans: groups, per_row };
        let mut lines: Vec<usize> = Vec::new();
        for (line, _) in rows.iter().flat_map(|r| r.clone()).filter_map(|i| lines_of.line_of(i)) {
            if !lines.contains(&line) {
                lines.push(line);
            }
        }
        return Plan::Lines(lines);
    }
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

    use gezik_core::group::GroupKey;
    use gezik_core::layout::Span;

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
    fn a_cloud_only_item_is_never_looked_up_by_its_path() {
        let file = |name: &str, flags: u8| Entry {
            name: name.into(),
            is_dir: false,
            flags,
            size: 1,
            modified: None,
            created: None,
        };
        assert_eq!(lookup_for(&file("setup.exe", 0), false), IconLookup::Path);
        assert_eq!(lookup_for(&file("setup.exe", Entry::CLOUD_ONLY), false), IconLookup::Type);
        assert_eq!(lookup_for(&file("a.lnk", Entry::CLOUD_ONLY | Entry::PINNED), false), IconLookup::Type);
        let package = Entry { is_dir: true, ..file("Pages.app", Entry::CLOUD_ONLY) };
        assert_eq!(lookup_for(&package, true), IconLookup::Folder);
    }

    #[test]
    fn the_state_is_drawn_only_under_a_root() {
        let base = if cfg!(windows) { r"C:\u\OneDrive" } else { "/u/OneDrive" };
        crate::cloud::set_roots(vec![gezik_platform::cloud::CloudRoot {
            path: base.into(),
            label: "OneDrive".into(),
            account: String::new(),
        }]);
        let file = |name: &str, flags: u8| Entry {
            name: name.into(),
            is_dir: false,
            flags,
            size: 1,
            modified: None,
            created: None,
        };
        let mut entries = vec![file("a", Entry::CLOUD_ONLY), file("b", 0), file("c", Entry::PINNED)];
        let inside = ViewData {
            listing: Listing::Files(PathBuf::from(base).join("Docs"), Rc::new(entries.clone())),
            media: Media::idle(),
            ..ViewData::default()
        };
        let states: Vec<i32> = (0..3).map(|i| file_row(&inside, i).cloud).collect();
        if cfg!(any(windows, target_os = "macos")) {
            assert_eq!(states, [1, 2, 3]);
        } else {
            assert_eq!(states, [0, 0, 0], "Linux: no state");
        }
        entries[1].flags = Entry::CLOUD_ONLY; // an offline file outside any cloud folder
        let outside = ViewData {
            listing: Listing::Files(PathBuf::from("/elsewhere"), Rc::new(entries)),
            media: Media::idle(),
            ..ViewData::default()
        };
        assert!((0..3).all(|i| file_row(&outside, i).cloud == 0), "no badge outside a root");
        crate::cloud::set_roots(Vec::new());
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
        assert_eq!(notify_plan(&[2..4, 9..10], 1, &[]), Plan::Lines(vec![2, 3, 9]));
        assert_eq!(notify_plan(&[2..4, 9..10], 4, &[]), Plan::Lines(vec![0, 2]));
        assert_eq!(notify_plan(&[], 1, &[]), Plan::Lines(vec![]));
    }

    #[test]
    #[allow(clippy::single_range_in_vec_init, reason = "one changed range of entries")]
    fn large_changes_reset_the_model() {
        assert_eq!(notify_plan(&[0..100_000], 1, &[]), Plan::Reset);
        assert_eq!(notify_plan(&[0..1000], 8, &[]), Plan::Lines((0..125).collect()));
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

    fn grouped(names: &[&str], spans: &[(usize, usize, bool)], keys: &[&str]) -> ViewData {
        ViewData {
            listing: super::super::listing::files("/x", names),
            media: Media::idle(),
            groups: spans.iter().map(|&(start, len, collapsed)| Span { start, len, collapsed }).collect(),
            group_keys: keys.iter().map(|k| GroupKey::Type((*k).to_owned())).collect(),
            ..ViewData::default()
        }
    }

    #[test]
    fn a_grouped_view_has_a_header_line_per_group() {
        let data =
            grouped(&["a.txt", "b.txt", "c.png", "d.png", "e.png"], &[(0, 2, false), (2, 3, true)], &["TXT", "PNG"]);
        let model = ItemsModel::new(Rc::new(RefCell::new(data)));
        assert_eq!(model.row_count(), 4, "header, a, b, closed header");
        let header = model.row_data(0).unwrap();
        assert_eq!((header.header.as_str(), header.first, header.cells.row_count()), ("TXT (2)", 0, 0));
        assert_eq!(model.row_data(1).unwrap().cells.row_data(0).unwrap().name.as_str(), "a.txt");
        let closed = model.row_data(3).unwrap();
        assert!(closed.collapsed && closed.header.as_str() == "PNG (3)" && closed.first == 2);
        assert!(model.row_data(4).is_none());
        model.set_per_row(2);
        assert_eq!(model.row_count(), 3);
        assert_eq!(model.place_of(1), Some((1, 1)));
        assert_eq!(model.place_of(3), None, "closed");
    }

    #[test]
    fn stale_groups_never_index_past_the_listing() {
        // Groups worked out for five rows, two left (a deletion before the regroup).
        let data = grouped(&["a.txt", "b.txt"], &[(0, 1, false), (1, 4, false), (5, 1, false)], &["A", "B", "C"]);
        let model = ItemsModel::new(Rc::new(RefCell::new(data)));
        let lines: Vec<Option<ItemRow>> = (0..model.row_count()).map(|n| model.row_data(n)).collect();
        assert!(lines.iter().flatten().all(|row| row.first < 2), "nothing past the end");
        assert!(lines.last().unwrap().is_none(), "a group starting past the end is not drawn");
    }

    #[test]
    fn place_of_falls_back_while_borrowed() {
        let data = Rc::new(RefCell::new(grouped(&["a", "b"], &[(0, 2, false)], &["A"])));
        let model = ItemsModel::new(data.clone());
        let _held = data.borrow_mut();
        assert_eq!(model.place_of(1), Some((1, 0)), "as without groups, never a panic");
    }

    #[test]
    fn hidden_rows_leave_the_selection_and_the_focus_moves() {
        let mut data =
            grouped(&["a", "b", "c", "d", "e"], &[(0, 2, false), (2, 2, true), (4, 1, false)], &["A", "B", "C"]);
        data.selection = Selection::from_indices(5, [0, 2, 3, 4], Some(3));
        let changed = hide_collapsed(&mut data);
        assert_eq!(data.selection.iter().collect::<Vec<_>>(), [0, 4]);
        assert_eq!(data.selection.focus(), Some(4), "the next row shown");
        assert!(changed.contains(&(3..4)) && changed.contains(&(4..5)));
        data.groups[2].collapsed = true;
        data.selection = Selection::from_indices(5, std::iter::empty(), Some(4));
        hide_collapsed(&mut data);
        assert_eq!(data.selection.focus(), Some(1), "at the end: the row before");
    }

    #[test]
    fn all_groups_closed_leave_no_focus() {
        let mut data = grouped(&["a", "b"], &[(0, 2, true)], &["A"]);
        data.selection = Selection::from_indices(2, [0, 1], Some(1));
        hide_collapsed(&mut data);
        assert_eq!((data.selection.count(), data.selection.focus()), (0, None));
    }

    #[test]
    fn a_grouped_redraw_is_by_line_past_a_big_closed_group() {
        // Lines on screen: open A (header, 0), closed B (100k rows), open C (header, 100_001).
        let names: Vec<String> = (0..100_002).map(|i| format!("{i}")).collect();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let data = grouped(&names, &[(0, 1, false), (1, 100_000, true), (100_001, 1, false)], &["A", "B", "C"]);
        let model = ItemsModel::new(Rc::new(RefCell::new(data)));
        assert_eq!(model.row_count(), 5);
        assert_eq!(model.lines_changed(0..5), 5, "each line on screen, no reset");
        assert_eq!(model.lines_changed(3..40), 2, "nothing past the end");
    }

    #[test]
    fn grouped_changes_redraw_their_lines() {
        let s = [Span { start: 0, len: 3, collapsed: false }, Span { start: 3, len: 2, collapsed: true }];
        assert_eq!(notify_plan(&[1..2, 3..5], 1, &s), Plan::Lines(vec![2]), "a closed group's rows are not drawn");
        #[allow(clippy::single_range_in_vec_init, reason = "one changed range of entries")]
        let all = [0..100_000];
        assert_eq!(notify_plan(&all, 1, &s), Plan::Reset);
    }

    #[test]
    fn tags_are_asked_only_for_the_rows_drawn_and_only_with_dots() {
        use gezik_config::tags::{Dots, TagsSettings};
        use gezik_core::meta::{Tag, TagColor};
        let file = |i: usize| Entry {
            name: format!("f{i}.txt"),
            is_dir: false,
            flags: 0,
            size: 1,
            modified: None,
            created: None,
        };
        let entries: Vec<Entry> = (0..1000).map(file).collect();
        let dir = PathBuf::from(if cfg!(windows) { r"C:\t" } else { "/t" });
        let data = ViewData {
            listing: Listing::Files(dir.clone(), Rc::new(entries)),
            media: Media::idle(),
            ..ViewData::default()
        };
        let draw = |rows: std::ops::Range<usize>| rows.for_each(|i| drop(file_row(&data, i)));
        crate::tags::set_settings(TagsSettings { dots: Dots::Off, ..TagsSettings::default() });
        draw(0..40);
        let others = data.media.pending();
        assert!(others <= 2, "a type name and an icon, no tags: {others}");
        crate::tags::set_settings(TagsSettings { dots: Dots::On, ..TagsSettings::default() });
        draw(0..40);
        assert_eq!(data.media.pending(), others + 40, "the 40 rows on screen");
        draw(0..40);
        assert_eq!(data.media.pending(), others + 40, "asked once");
        // Scrolled far: the oldest requests go (the queue keeps the newest).
        draw(40..400);
        assert!(data.media.pending() <= crate::media::MAX_QUEUED + others, "{}", data.media.pending());
        // Once read, a row draws its dots; no colour is a ring.
        let red = Tag::new("Red", TagColor::None);
        let key = crate::media::MediaKey::Tags { path: dir.join("f0.txt") };
        let meta = gezik_core::meta::Meta { tags: vec![red, Tag::new("Mine", TagColor::None)], ..Default::default() };
        let row = crate::tags::RowMeta { meta };
        data.media.finish(
            &std::sync::atomic::AtomicBool::new(true),
            key,
            crate::media::Outcome::Tags(std::sync::Arc::new(row)),
        );
        let row = file_row(&data, 0);
        assert_eq!(row.dots, 2);
        assert_eq!(row.dot1, crate::tags::color_of(TagColor::Red), "the list's colour");
        assert_eq!((row.more, row.tag_names.as_str()), (0, "Red, Mine"));
        // Files changed (a listing shown): read again, the old dots drawn until then.
        data.media.new_generation();
        draw(0..1);
        let before = data.media.pending();
        data.media.forget_tags();
        assert_eq!(file_row(&data, 0).dots, 2, "no flicker");
        assert_eq!(data.media.pending(), before + 1, "asked again");
        crate::tags::set_settings(TagsSettings { dots: Dots::Off, ..TagsSettings::default() });
        assert_eq!(file_row(&data, 0).dots, 0, "dots off: none drawn");
    }

    #[test]
    fn the_rating_and_comment_columns_show_the_stars_and_the_comment() {
        use gezik_config::tags::{Dots, TagsSettings};
        use gezik_core::meta::Meta;
        let entries: Vec<Entry> = ["a.jpg", "b.txt"]
            .iter()
            .map(|n| Entry { name: (*n).into(), is_dir: false, flags: 0, size: 1, modified: None, created: None })
            .collect();
        let dir = PathBuf::from(if cfg!(windows) { r"C:\r" } else { "/r" });
        let mut data = ViewData {
            listing: Listing::Files(dir.clone(), Rc::new(entries)),
            media: Media::idle(),
            ..ViewData::default()
        };
        crate::tags::set_settings(TagsSettings { dots: Dots::Off, ..TagsSettings::default() });
        drop(file_row(&data, 1));
        let others = data.media.pending();
        data.comment_column = true;
        drop(file_row(&data, 1));
        assert_eq!(data.media.pending(), others + 1, "the comment column asks");
        let meta = Meta { rating: 0, comment: "Ready".into(), ..Meta::default() };
        crate::tags::set_explorer_for_tests(
            &dir.join("a.jpg"),
            crate::tags::Explorer { keywords: "beach".into(), stars: 4 },
        );
        let key = crate::media::MediaKey::Tags { path: dir.join("a.jpg") };
        let row = std::sync::Arc::new(crate::tags::RowMeta { meta });
        data.media.finish(&std::sync::atomic::AtomicBool::new(true), key, crate::media::Outcome::Tags(row));
        data.rating_column = true;
        data.tags_column = true;
        let row = file_row(&data, 0);
        assert_eq!(row.comment.as_str(), "Ready");
        if cfg!(windows) {
            assert_eq!((row.rating.as_str(), row.rating_faint), ("★★★★☆", true), "Explorer's stars, faint");
            assert_eq!(row.tag_names.as_str(), "(beach)", "Explorer's Tags in brackets");
        } else {
            assert_eq!(row.rating.as_str(), "", "no Explorer off Windows");
        }
        data.rating_column = false;
        assert_eq!(file_row(&data, 0).rating.as_str(), "", "column hidden: no stars");
    }

    #[test]
    fn a_cloud_folder_read_whole_keeps_its_stars_and_reads_nothing_more() {
        use gezik_config::tags::{Dots, TagsSettings};
        use gezik_core::meta::Meta;
        let root = std::env::temp_dir().join("gezik-12d").join(format!("cloud-{}", std::process::id()));
        crate::cloud::set_roots(vec![gezik_platform::cloud::CloudRoot {
            path: root.clone(),
            label: "OneDrive".into(),
            account: String::new(),
        }]);
        let entries =
            vec![Entry { name: "a.jpg".into(), is_dir: false, flags: 0, size: 1, modified: None, created: None }];
        let mut data = ViewData {
            listing: Listing::Files(root.clone(), Rc::new(entries)),
            media: Media::idle(),
            rating_column: true,
            comment_column: true,
            ..ViewData::default()
        };
        crate::tags::set_settings(TagsSettings { dots: Dots::Auto, ..TagsSettings::default() });
        let known = Meta { rating: 3, comment: "Ready".into(), ..Meta::default() };
        let tags = std::collections::HashMap::from([(Box::from("a.jpg"), known)]);
        data.folder_tags = Some(Rc::new(super::super::FolderTags::read(root, tags)));
        drop(file_row(&data, 0));
        let pending = data.media.pending();
        let row = file_row(&data, 0);
        assert_eq!((row.rating.as_str(), row.comment.as_str()), ("★★★☆☆", "Ready"), "a picture keeps its stars");
        assert_eq!(data.media.pending(), pending, "no row read, no Explorer read in the cloud");
        crate::cloud::set_roots(Vec::new());
    }

    #[test]
    fn highlight_rules_color_rows_and_read_meta_only_when_they_ask() {
        use gezik_config::tags::{Dots, TagsSettings};
        use gezik_config::theme::ThemeColors;
        use gezik_core::highlight::{Highlights, Hue, Rule, Style};
        use gezik_core::meta::{Meta, Tag, TagColor};
        use gezik_core::meta_filter::TagTerm;
        use gezik_core::search::KindFilter;
        let entries: Vec<Entry> = ["a.jpg", "b.txt"]
            .iter()
            .map(|n| Entry { name: (*n).into(), is_dir: false, flags: 0, size: 1, modified: None, created: None })
            .collect();
        let dir = PathBuf::from(if cfg!(windows) { r"C:\h" } else { "/h" });
        let mut data = ViewData {
            listing: Listing::Files(dir.clone(), Rc::new(entries)),
            media: Media::idle(),
            ..ViewData::default()
        };
        crate::tags::set_settings(TagsSettings { dots: Dots::Off, ..TagsSettings::default() });
        let color = |c: &str| gezik_config::Color::parse(c).unwrap();
        let light = ThemeColors { background: color("#ffffff"), selection: color("#0060d0"), ..ThemeColors::default() };
        let dark = ThemeColors { background: color("#1e1e1e"), selection: color("#3a3a3a"), ..ThemeColors::default() };
        let none = slint::Color::from_argb_u8(0, 0, 0, 0);
        // No rules: nothing colored, nothing read.
        crate::tags::set_highlights(Highlights::default(), Some(&light));
        let row = file_row(&data, 0);
        drop(file_row(&data, 1));
        let others = data.media.pending();
        assert_eq!((row.tint, row.bold), (none, false));
        // A rule without meta: colored at once, still nothing read.
        let pictures = Rule {
            kind: Some(KindFilter::Pictures),
            style: Style { color: Some(Hue::Named(TagColor::Blue)), dark: None, bold: true },
            ..Rule::default()
        };
        crate::tags::set_highlights(Highlights::new(vec![pictures.clone()]), Some(&light));
        let row = file_row(&data, 0);
        assert_eq!((row.tint, row.bold), (slint::Color::from_rgb_u8(0x00, 0x60, 0xd0), true));
        assert_eq!(file_row(&data, 1).tint, none);
        assert_eq!(data.media.pending(), others, "no meta rule: no read");
        // Selected: blue on the blue selection does not read, the theme's color shows; bold stays.
        data.selection = Selection::from_indices(2, [0], Some(0));
        let row = file_row(&data, 0);
        assert_eq!((row.tint, row.bold), (none, true));
        data.selection = Selection::default();
        // The dark theme's shade.
        crate::tags::set_highlights(Highlights::new(vec![pictures.clone()]), Some(&dark));
        assert_eq!(file_row(&data, 0).tint, slint::Color::from_rgb_u8(0x40, 0x9c, 0xff));
        // A tag rule first: rows on screen are read; until then no color (not the later rule's).
        let red = Rule {
            terms: vec![TagTerm::Named("Red".into())],
            style: Style { color: Some(Hue::Rgb([1, 2, 3])), dark: None, bold: false },
            ..Rule::default()
        };
        crate::tags::set_highlights(Highlights::new(vec![red, pictures]), Some(&dark));
        let row = file_row(&data, 0);
        assert_eq!((row.tint, row.bold), (none, false), "waits for the tags");
        assert_eq!(data.media.pending(), others + 1, "the row is read");
        let meta = Meta { tags: vec![Tag::new("Red", TagColor::Red)], ..Meta::default() };
        let key = crate::media::MediaKey::Tags { path: dir.join("a.jpg") };
        let read = std::sync::Arc::new(crate::tags::RowMeta { meta });
        data.media.finish(&std::sync::atomic::AtomicBool::new(true), key, crate::media::Outcome::Tags(read));
        assert_eq!(file_row(&data, 0).tint, slint::Color::from_rgb_u8(1, 2, 3));
        crate::tags::set_highlights(Highlights::default(), Some(&light));
    }

    #[test]
    fn the_tags_column_asks_for_the_rows_drawn_and_a_folder_read_whole_answers() {
        use gezik_config::tags::{Dots, TagsSettings};
        use gezik_core::meta::{Tag, TagColor};
        let entries: Vec<Entry> = (0..100)
            .map(|i| Entry {
                name: format!("f{i}.txt"),
                is_dir: false,
                flags: 0,
                size: 1,
                modified: None,
                created: None,
            })
            .collect();
        let dir = PathBuf::from(if cfg!(windows) { r"C:\c" } else { "/c" });
        let mut data = ViewData {
            listing: Listing::Files(dir.clone(), Rc::new(entries)),
            media: Media::idle(),
            ..ViewData::default()
        };
        crate::tags::set_settings(TagsSettings { dots: Dots::Off, ..TagsSettings::default() });
        (0..20).for_each(|i| drop(file_row(&data, i)));
        let others = data.media.pending();
        assert!(others <= 2, "no column, no dots: a type name and an icon, no tags: {others}");
        data.tags_column = true;
        (0..20).for_each(|i| drop(file_row(&data, i)));
        assert_eq!(data.media.pending(), others + 20, "the rows on screen");
        // Read whole: answered from it, names without dots (they are off), nothing asked.
        let red = gezik_core::meta::Meta { tags: vec![Tag::new("Red", TagColor::Red)], ..Default::default() };
        let tags = std::collections::HashMap::from([(Box::from("f30.txt"), red)]);
        data.folder_tags = Some(Rc::new(super::super::FolderTags::read(dir, tags)));
        let pending = data.media.pending();
        let row = file_row(&data, 30);
        assert_eq!((row.dots, row.tag_names.as_str(), row.cut), (0, "Red", false));
        assert_eq!(file_row(&data, 31).tag_names.as_str(), "");
        assert_eq!(data.media.pending(), pending);
    }

    #[test]
    fn field_columns_ask_only_rows_drawn_and_only_fields_not_read() {
        use gezik_core::fields::{ALBUM, ARTIST, DURATION, Value};
        let names = ["a.mp3", "b.txt", "c.mp3"];
        let entries: Vec<Entry> = names
            .iter()
            .map(|n| Entry { name: (*n).into(), is_dir: false, flags: 0, size: 1, modified: None, created: None })
            .collect();
        let dir = PathBuf::from(if cfg!(windows) { r"C:\m" } else { "/m" });
        let mut data = ViewData {
            listing: Listing::Files(dir.clone(), Rc::new(entries)),
            media: Media::idle(),
            ..ViewData::default()
        };
        let asked = |data: &ViewData| data.media.pending();
        (0..3).for_each(|i| drop(file_row(&data, i)));
        let none = asked(&data);
        assert_eq!(file_row(&data, 0).fields.row_count(), 0, "no field column: no cells");
        data.field_columns = vec![ARTIST, DURATION];
        let row = file_row(&data, 0);
        assert_eq!(row.fields.row_count(), 2, "one cell a column, empty until read");
        assert_eq!(asked(&data), none + 1, "the row drawn asks");
        drop(file_row(&data, 1));
        assert_eq!(asked(&data), none + 1, "a .txt has no reader: nothing asked");
        // Read: shown; a wider set asks only for the field missing.
        let path = dir.join("a.mp3");
        let key = MediaKey::Fields { path: path.clone(), want: FieldSet::of(&[ARTIST, DURATION]) };
        let values = Fields::new(vec![(ARTIST, Value::Text("Tarkan".into())), (DURATION, Value::Duration(205_000))]);
        data.media.finish(&std::sync::atomic::AtomicBool::new(true), key, crate::media::Outcome::Fields(values));
        let pending = asked(&data);
        let cells: Vec<String> = file_row(&data, 0).fields.iter().map(|c| c.to_string()).collect();
        assert_eq!(cells, ["Tarkan", "3:25"]);
        assert_eq!(asked(&data), pending, "known: nothing asked");
        data.field_columns.push(ALBUM);
        drop(file_row(&data, 0));
        assert_eq!(asked(&data), pending + 1);
        assert!(data.media.is_pending(&MediaKey::Fields { path: path.clone(), want: FieldSet::of(&[ALBUM]) }));
        assert_eq!(known_field(&data, 0, ARTIST), "Tarkan");
        data.mode = ViewMode::Grid;
        assert_eq!(file_row(&data, 2).fields.row_count(), 0, "the grid has no columns");
    }
}
