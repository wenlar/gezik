//! Miller columns (spec 10 §7): the tab's column path, the listings of its columns and how they
//! reach the pane. The focused column is the view itself (its folder, rows, selection, keys,
//! filter and file view, laid over the column's place); every other column is read here in the
//! background, tied to the pane and the column generation, and let go of by the memory rule
//! (the columns on screen and the focused one's first 8 ancestors keep their listings).

use std::cell::RefCell;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gezik_core::Entry;
use gezik_core::columns::ColumnPath;
use gezik_core::nav::{Location, ViewState};
use gezik_core::selection::Selection;
use gezik_core::sort::{SortSpec, sort_entries_grouped};
use gezik_core::view::ViewMode;
use slint::{ModelRc, VecModel};

use super::model::{ItemsModel, ViewData};
use super::{Listing, View};
use crate::ColumnItem;

/// At most this many other tabs keep their column paths.
const MAX_PARKED: usize = 64;

/// How long the columns must stay before the ones missing are read (↓ held over folders opens
/// and closes a column per row: only the last one is read).
const READ_DELAY: std::time::Duration = std::time::Duration::from_millis(100);

/// The status line where only the list can show (spec 10 §7.3).
pub const LIST_ONLY: &str = "Columns show folders only: the list is shown here";

/// A column's listing; the focused column's is the view's.
struct List {
    folder: PathBuf,
    /// `None` until it is read.
    shown: Option<(Rc<RefCell<ViewData>>, Rc<ItemsModel>)>,
    /// A read of it is under way.
    reading: bool,
    /// Its scroll offset when it was focused or pressed last.
    scroll: f32,
    /// The name selected in it, as its rows show it.
    selected: Option<String>,
}

/// A column let go of: its icon requests go too, so they do not wait ahead of the live ones.
impl Drop for List {
    fn drop(&mut self) {
        if let Some(data) = self.shown.as_ref().and_then(|(data, _)| data.try_borrow().ok()) {
            data.media.new_generation();
        }
    }
}

impl List {
    fn new(folder: PathBuf) -> List {
        List { folder, shown: None, reading: false, scroll: 0.0, selected: None }
    }

    fn entries(&self) -> Option<Rc<Vec<Entry>>> {
        match &self.shown.as_ref()?.0.borrow().listing {
            Listing::Files(_, entries) => Some(entries.clone()),
            _ => None,
        }
    }
}

/// A pane's columns. Pure but for the listings' models: what the tests reach.
#[derive(Default)]
pub(super) struct Columns {
    /// The active tab's column path, kept while the tab wants columns; while no folder shows
    /// it says so with an empty root.
    path: Option<ColumnPath>,
    tab: Option<u64>,
    /// Other tabs' paths, by tab id.
    parked: Vec<(u64, ColumnPath)>,
    /// What the load under way lands on (Up from the first column).
    next: Option<ColumnPath>,
    /// Bumped whenever the columns start over: a read for an earlier generation is dropped.
    generation: u64,
    /// The columns on screen (from the first to one past the last that fits).
    visible: Range<usize>,
    /// The focused column takes its first item once its listing shows (→ into a column with
    /// nothing selected in it); not when that listing is empty.
    select_first: bool,
    /// The focused column shows an empty stand-in until its listing is read: its selection says
    /// nothing yet.
    unread: bool,
    /// Folders to read once the columns stay (`READ_DELAY`).
    pending: Vec<PathBuf>,
    timer: slint::Timer,
    lists: Vec<List>,
    model: Option<Rc<VecModel<ColumnItem>>>,
}

impl Columns {
    /// A load lands on `folder` (`None`: no folder) in tab `tab`: another tab's path comes
    /// back; a root change (address bar, sidebar, Back, Forward, Up) starts the path over at
    /// the folder, whatever the history did with it, unless the load is the one a path waits
    /// for (`next`).
    fn land(&mut self, folder: Option<&Path>, tab: Option<u64>, root_change: bool) {
        if tab != self.tab {
            if let (Some(old), Some(path)) = (self.tab, self.path.take()) {
                self.parked.retain(|(id, _)| *id != old);
                self.parked.push((old, path));
                if self.parked.len() > MAX_PARKED {
                    self.parked.remove(0);
                }
            }
            self.path = tab.and_then(|id| {
                let at = self.parked.iter().position(|(of, _)| *of == id)?;
                Some(self.parked.remove(at).1)
            });
            self.tab = tab;
            self.restart();
        }
        let next = self.next.take();
        let (Some(folder), Some(path)) = (folder, self.path.as_mut()) else { return };
        match next {
            Some(next) if next.location() == folder => *path = next,
            _ if root_change => *path = ColumnPath::new(folder.to_path_buf()),
            _ => return,
        }
        self.restart();
    }

    /// The view shows `folder` (`None`: results, a trash, This PC) and its view says columns
    /// (`wanted`) or not: whether the columns show. A tab in columns stays in them for every
    /// folder it shows (spec 10 §7.1); elsewhere the list shows (spec 10 §7.3).
    fn show(&mut self, folder: Option<&Path>, wanted: bool) -> bool {
        let Some(folder) = folder else {
            if wanted && self.path.is_none() {
                self.path = Some(ColumnPath::new(PathBuf::new()));
            }
            return false;
        };
        match &self.path {
            None if !wanted => return false,
            Some(path) if path.location() == folder => return true,
            _ => {}
        }
        self.path = Some(ColumnPath::new(folder.to_path_buf()));
        self.restart();
        true
    }

    /// The list or grid was chosen: the tab leaves the columns.
    fn off(&mut self) {
        self.path = None;
        self.next = None;
        self.restart();
    }

    /// The columns start over: reads under way are dropped (and done again where needed).
    fn restart(&mut self) {
        self.generation += 1;
        self.select_first = false;
        self.unread = false;
        for list in &mut self.lists {
            list.reading = false;
        }
    }

    /// The listings the path keeps (its columns but the focused one, by the memory rule),
    /// the others let go of; the folders to read.
    fn keep(&mut self) -> Vec<PathBuf> {
        let Some(path) = &self.path else {
            self.lists.clear();
            return Vec::new();
        };
        let kept: Vec<&Path> = path
            .columns()
            .iter()
            .enumerate()
            .filter(|&(i, _)| i != path.focus() && path.keeps(i, self.visible.clone()))
            .map(|(_, column)| column.folder.as_path())
            .collect();
        self.lists.retain(|list| kept.contains(&list.folder.as_path()));
        let mut read = Vec::new();
        for folder in kept {
            let list = match self.lists.iter().position(|list| list.folder == folder) {
                Some(at) => &mut self.lists[at],
                None => {
                    self.lists.push(List::new(folder.to_path_buf()));
                    self.lists.last_mut().expect("just pushed")
                }
            };
            if list.shown.is_none() && !list.reading {
                list.reading = true;
                read.push(folder.to_path_buf());
            }
        }
        read
    }

    /// The folders waiting to be read that are still wanted, taken out, and their generation.
    fn due(&mut self) -> (Vec<PathBuf>, u64) {
        let mut due = std::mem::take(&mut self.pending);
        due.dedup();
        due.retain(|folder| self.waits_for(self.generation, folder));
        (due, self.generation)
    }

    /// Whether a read of `folder` for `generation` is still wanted.
    fn waits_for(&self, generation: u64, folder: &Path) -> bool {
        generation == self.generation && self.lists.iter().any(|list| list.reading && list.folder == folder)
    }

    /// The listing of column `folder`, taken out (its column takes the focus) with its offset.
    fn take(&mut self, folder: &Path) -> Option<(Rc<Vec<Entry>>, f32)> {
        let at = self.lists.iter().position(|list| list.folder == folder)?;
        let list = self.lists.remove(at);
        Some((list.entries()?, list.scroll))
    }

    /// `list` in its column, if the path still has that column (not the focused one).
    fn put(&mut self, list: List) {
        let Some(path) = &self.path else { return };
        let column = path.columns().iter().position(|c| c.folder == list.folder);
        if column.is_none_or(|i| i == path.focus()) {
            return;
        }
        match self.lists.iter_mut().find(|l| l.folder == list.folder) {
            Some(old) => *old = list,
            None => self.lists.push(list),
        }
    }
}

/// `name` selected and focused at `scroll`, as a column move shows its column.
fn selecting(name: Option<String>, scroll: f32) -> ViewState {
    ViewState { selected: name.iter().cloned().collect(), focus: name, scroll, filter: None }
}

impl View {
    /// The columns show: the view's mode is Columns only while a folder shows (`show`).
    pub fn columns_on(&self) -> bool {
        self.0.current.get().mode == ViewMode::Columns
    }

    /// The column path, while the view shows its focused column (not while a column's or a
    /// tab's load is under way).
    fn column_path(&self) -> Option<ColumnPath> {
        if !self.columns_on() {
            return None;
        }
        let folder = self.shown_folder()?;
        self.0.miller.borrow().path.clone().filter(|path| path.location() == folder)
    }

    /// A load lands on `location` in tab `tab` (navigation.rs, before it is shown);
    /// `root_change`: a move in the history, which starts the columns over at it.
    pub fn columns_land(&self, location: &Location, tab: Option<u64>, root_change: bool) {
        let folder = match location {
            Location::Path(path) => Some(path.as_path()),
            _ => None,
        };
        self.0.miller.borrow_mut().land(folder, tab, root_change);
    }

    /// Whether the view about to show `folder` shows it as columns (`wanted`: its view says
    /// so); [`Columns::show`].
    pub(super) fn columns_for(&self, folder: Option<&Path>, wanted: bool) -> bool {
        self.0.miller.borrow_mut().show(folder, wanted)
    }

    /// `view` from the defaults or a rule, in the columns while the tab is in them: only the
    /// list or grid chosen leaves them.
    pub(super) fn keep_columns(&self, mut view: gezik_core::view::ViewSettings) -> gezik_core::view::ViewSettings {
        if self.columns_on() {
            view.mode = ViewMode::Columns;
        }
        view
    }

    /// The list or grid was chosen.
    pub(super) fn columns_off(&self) {
        self.0.miller.borrow_mut().off();
    }

    /// The columns on screen changed (`first` to one past the last that fits).
    pub fn columns_on_screen(&self, first: i32, end: i32) {
        let first = usize::try_from(first).unwrap_or(0);
        let end = usize::try_from(end).unwrap_or(0).max(first);
        if self.0.miller.borrow().visible == (first..end) {
            return;
        }
        self.0.miller.borrow_mut().visible = first..end;
        self.columns_sync();
    }

    /// The focused column's one selected item is a file: its preview is the last column
    /// (unless the window's preview pane shows it).
    pub fn column_file(&self) -> bool {
        if !self.columns_on() {
            return false;
        }
        let data = self.0.data.borrow();
        data.selection.count() == 1 && data.selection.iter().next().is_some_and(|i| !data.listing.is_dir(i))
    }

    /// → (`right`) or ←, in the columns; false (not used) elsewhere.
    pub fn column_key(&self, right: bool) -> bool {
        if !self.columns_on() {
            return false;
        }
        if right {
            self.column_move(ColumnPath::enter, None, None, true);
        } else {
            self.column_leave();
        }
        true
    }

    /// ←, or Up from a column but the first: back to the column on the left, its folder still
    /// selected; a step up for sync browsing (spec 10 §4.7).
    pub fn column_leave(&self) -> bool {
        let Some(path) = self.column_path() else { return false };
        let name = path.focus().checked_sub(1).and_then(|left| path.columns()[left].selected.clone());
        self.column_move(ColumnPath::leave, name, None, false)
    }

    /// Up from the first column: the root's parent becomes the root once the history move
    /// to it lands, the old root selected in it (returned, to be selected).
    pub fn column_up_root(&self) -> Option<String> {
        let mut path = self.column_path()?;
        let name = path.root().file_name()?.to_string_lossy().into_owned();
        if path.focus() != 0 || !path.up() {
            return None;
        }
        self.0.miller.borrow_mut().next = Some(path);
        Some(name)
    }

    /// Opening `folder` (Enter, a double-click): into its column when it is the one the
    /// focused column's selection opened, as →.
    pub fn column_enter(&self, folder: &Path) -> bool {
        let Some(path) = self.column_path() else { return false };
        if path.columns().get(path.focus() + 1).is_none_or(|next| next.folder != folder) {
            return false;
        }
        self.column_move(ColumnPath::enter, None, None, true)
    }

    /// A press on `row` of `column`, not the focused one, scrolled to `scroll`: that column
    /// takes the focus with the row selected (a folder opens its column), or with nothing
    /// selected for its empty space (`row` < 0). The row's place in the view, once it shows.
    pub fn column_clicked(&self, column: i32, row: i32, scroll: f32) -> Option<usize> {
        let column = usize::try_from(column).ok()?;
        let path = self.column_path()?;
        if column == path.focus() || column >= path.columns().len() {
            return None;
        }
        let Ok(row) = usize::try_from(row) else {
            self.column_move(move |path| path.select(column, None, false), None, Some(scroll), false);
            return None;
        };
        let (name, is_dir) =
            self.column_entry_at(column, row, |entries| (entries[row].name.clone(), entries[row].is_dir))?;
        let select = name.clone();
        self.column_move(move |path| path.select(column, Some(select), is_dir), Some(name), Some(scroll), false);
        self.single_selected()
    }

    /// What `f` makes of the listing of `column` (not the focused one), if it has `row`.
    fn column_entry_at<R>(&self, column: usize, row: usize, f: impl FnOnce(&[Entry]) -> R) -> Option<R> {
        let folder = self.column_path()?.columns().get(column)?.folder.clone();
        let columns = self.0.miller.borrow();
        let entries = columns.lists.iter().find(|list| list.folder == folder)?.entries()?;
        (row < entries.len()).then(|| f(&entries))
    }

    /// Entry `row` of `column` (not the focused one) and whether it is a folder: a drop
    /// target, a middle-click.
    pub fn column_entry(&self, column: usize, row: usize) -> Option<(PathBuf, bool)> {
        let folder = self.column_folder(column)?;
        self.column_entry_at(column, row, |entries| (folder.join(&entries[row].name), entries[row].is_dir))
    }

    /// The folder `column` shows.
    pub fn column_folder(&self, column: usize) -> Option<PathBuf> {
        Some(self.column_path()?.columns().get(column)?.folder.clone())
    }

    /// Each column's scroll offset and the rows it shows (none: the focused one, those not on
    /// screen or not read), for drops.
    pub fn column_rows(&self) -> Vec<(f32, usize)> {
        let Some(path) = self.column_path() else { return Vec::new() };
        let columns = self.0.miller.borrow();
        let rows = |i: usize, folder: &Path| {
            let list = columns.lists.iter().find(|list| list.folder == folder)?;
            let count = list.entries().filter(|_| i != path.focus() && columns.visible.contains(&i))?.len();
            Some((list.scroll, count))
        };
        path.columns().iter().enumerate().map(|(i, c)| rows(i, &c.folder).unwrap_or_default()).collect()
    }

    /// `column` was scrolled to `scroll` (while it shows its rows).
    pub fn column_scrolled(&self, column: i32, scroll: f32) {
        let Ok(column) = usize::try_from(column) else { return };
        let Some(folder) = self.column_folder(column) else { return };
        let mut columns = self.0.miller.borrow_mut();
        if !columns.visible.contains(&column) {
            return;
        }
        if let Some(list) = columns.lists.iter_mut().find(|list| list.folder == folder && list.shown.is_some()) {
            list.scroll = scroll;
        }
    }

    /// Moves the focus as `change` does to the path: the column left keeps the view's rows;
    /// the one focused shows its own (none until read) with `name` selected at `scroll` (else
    /// its own offset), or its first item (`first`); the tab's location follows without a step
    /// in its history (spec 10 §7.1). A relative step for sync browsing (navigation.rs).
    fn column_move(
        &self,
        change: impl FnOnce(&mut ColumnPath) -> bool,
        name: Option<String>,
        scroll: Option<f32>,
        first: bool,
    ) -> bool {
        let Some(mut path) = self.column_path() else { return false };
        let left = path.location().to_path_buf();
        if !change(&mut path) {
            return false;
        }
        // Nothing shown yet (its read under way): the column left is read like any other.
        let entries = self.0.data.borrow().full.clone();
        let list = (!entries.is_empty()).then(|| {
            let mut list = self.make_list(&left, entries);
            list.scroll = self.list_scroll();
            list
        });
        let taken = {
            let mut columns = self.0.miller.borrow_mut();
            columns.path = Some(path.clone());
            if let Some(list) = list {
                columns.put(list);
            }
            columns.select_first = first;
            let taken = columns.take(path.location());
            columns.unread = taken.is_none();
            taken
        };
        let (entries, own) = taken.unwrap_or_default();
        let state = selecting(name, scroll.unwrap_or(own));
        let listing = Listing::Files(path.location().to_path_buf(), entries);
        crate::panes::with_id(self.0.id, |p| p.nav.column_move(&path, listing, state));
        true
    }

    /// The focused column's selection changed (or a listing showed): one selected folder opens
    /// its column to the right, anything else closes them; → into a column with nothing
    /// selected selects its first item once it has rows.
    pub(super) fn columns_follow(&self) {
        let Some(path) = self.column_path() else { return };
        let (one, is_dir, count, len) = {
            let data = self.0.data.borrow();
            let one = (data.selection.count() == 1).then(|| data.selection.iter().next()).flatten();
            let name = one.and_then(|i| data.listing.name_at(i)).map(str::to_owned);
            (name, one.is_some_and(|i| data.listing.is_dir(i)), data.selection.count(), data.listing.len())
        };
        let (first, waiting) = {
            let mut columns = self.0.miller.borrow_mut();
            // The empty stand-in of a column not read yet: its listing decides once it shows.
            let waiting = columns.unread && len == 0;
            let first = columns.select_first && !waiting && count == 0 && len > 0;
            if !waiting {
                columns.unread = false;
                columns.select_first = false;
            }
            (first, waiting)
        };
        if first {
            // Comes back here with the first item selected.
            return self.jump_to(0);
        }
        if waiting {
            return self.columns_sync();
        }
        let focus = path.focus();
        let open = !is_dir || path.columns().len() > focus + 1;
        if (path.columns()[focus].selected != one || !open)
            && let Some(path) = self.0.miller.borrow_mut().path.as_mut()
        {
            path.select(focus, one, is_dir);
        }
        self.columns_sync();
    }

    /// The listings and the pane follow the path: what the memory rule keeps stays, the rest
    /// is let go of, missing listings are read; each column's selection; the focused column
    /// and whether its preview shows. Without columns, all of it goes.
    pub(super) fn columns_sync(&self) {
        let on = self.columns_on();
        let (read, items, focus, model, fresh) = {
            let mut columns = self.0.miller.borrow_mut();
            if !on {
                columns.lists.clear();
                let had = columns.model.take().is_some();
                drop(columns);
                if had {
                    crate::panes::edit(self.0.id, |d| d.columns = ModelRc::default());
                }
                return;
            }
            let read = columns.keep();
            let Some(path) = columns.path.clone() else { return };
            let visible = columns.visible.clone();
            let items: Vec<ColumnItem> = path
                .columns()
                .iter()
                .enumerate()
                .map(|(i, column)| {
                    let list = columns.lists.iter_mut().find(|list| list.folder == column.folder);
                    let Some(list) = list.filter(|_| i != path.focus()) else { return ColumnItem::default() };
                    if let Some((data, model)) = &list.shown
                        && list.selected != column.selected
                    {
                        list.selected = column.selected.clone();
                        select_in(data, model, column.selected.as_deref());
                    }
                    let rows = match &list.shown {
                        Some((_, model)) if visible.contains(&i) => ModelRc::from(model.clone()),
                        _ => ModelRc::default(),
                    };
                    ColumnItem { rows, scroll: list.scroll }
                })
                .collect();
            let fresh = columns.model.is_none();
            let model = columns.model.get_or_insert_with(Rc::default).clone();
            (read, items, path.focus(), model, fresh)
        };
        if fresh {
            crate::panes::edit(self.0.id, |d| d.columns = ModelRc::from(model.clone()));
        }
        crate::navigation::sync_model(&model, items.into_iter());
        let file = self.column_file();
        crate::panes::edit(self.0.id, |d| {
            d.column_focus = i32::try_from(focus).unwrap_or(0);
            d.column_file = file;
        });
        self.read_soon(read);
    }

    /// `folders` are read once the columns stay a moment: one read per column that stayed,
    /// not one per row held over.
    fn read_soon(&self, folders: Vec<PathBuf>) {
        if folders.is_empty() {
            return;
        }
        let mut columns = self.0.miller.borrow_mut();
        columns.pending.extend(folders);
        let weak = Rc::downgrade(&self.0);
        columns.timer.start(slint::TimerMode::SingleShot, READ_DELAY, move || {
            if let Some(inner) = weak.upgrade() {
                let view = View(inner);
                let (due, generation) = view.0.miller.borrow_mut().due();
                for folder in due {
                    view.read_column(folder, generation);
                }
            }
        });
    }

    /// The `[view]` options or defaults changed: the columns beside the focused one are read
    /// again with them.
    pub(super) fn columns_relist(&self) {
        let had = !self.0.miller.borrow().lists.is_empty();
        if had {
            self.0.miller.borrow_mut().lists.clear();
            self.columns_sync();
        }
    }

    /// Reads column `folder` in the background, as the folder on screen is read.
    fn read_column(&self, folder: PathBuf, generation: u64) {
        let (id, window) = (self.0.id, self.0.window.clone());
        let spawned = std::thread::Builder::new().name("gezik-column".into()).spawn(move || {
            let entries = crate::navigation::read_folder(&folder);
            let _ = window.upgrade_in_event_loop(move |_| {
                crate::panes::with_id(id, |p| p.view.column_read(generation, folder, entries));
            });
        });
        if spawned.is_err() {
            eprintln!("gezik: cannot start a column's read");
        }
    }

    /// Column `folder` was read for `generation` (`None`: it cannot be listed, shown empty).
    pub fn column_read(&self, generation: u64, folder: PathBuf, entries: Option<Vec<Entry>>) {
        if !self.0.miller.borrow().waits_for(generation, &folder) {
            return;
        }
        let entries = self.column_entries(&folder, entries.unwrap_or_default());
        let mut list = self.make_list(&folder, entries);
        {
            let mut columns = self.0.miller.borrow_mut();
            list.scroll = columns.lists.iter().find(|l| l.folder == folder).map_or(0.0, |l| l.scroll);
            columns.put(list);
        }
        self.columns_sync();
    }

    /// Gezik's own job changed `dirs`: the columns showing one of them are read again.
    pub fn columns_touched(&self, dirs: &[PathBuf]) {
        let read = {
            let mut columns = self.0.miller.borrow_mut();
            let mut read = Vec::new();
            for list in columns.lists.iter_mut().filter(|l| l.shown.is_some() && !l.reading) {
                if dirs.iter().any(|dir| gezik_core::ops::paths::same_path(dir, &list.folder)) {
                    list.reading = true;
                    read.push(list.folder.clone());
                }
            }
            read
        };
        self.read_soon(read);
    }

    /// `entries` read for a column as the view would show them: without what `[view]` hides,
    /// in the folder's own sort (views.toml) or `[view]`'s.
    fn column_entries(&self, folder: &Path, entries: Vec<Entry>) -> Rc<Vec<Entry>> {
        let options = self.0.options.get();
        let read = Listing::Files(folder.to_path_buf(), Rc::new(entries));
        let Listing::Files(_, entries) = read.without_hidden(options.show_hidden, options.show_system) else {
            return Rc::default();
        };
        let own = self.0.memory.borrow_mut().get(&folder.display().to_string());
        let sort = own.unwrap_or(self.0.defaults.get().view).sort;
        // A folder is read sorted by name, folders first.
        if sort == SortSpec::default() && options.folders_first {
            return entries;
        }
        let mut entries = Rc::unwrap_or_clone(entries);
        sort_entries_grouped(&mut entries, sort, options.folders_first, |e| self.type_name_of(e), None);
        Rc::new(entries)
    }

    /// A column's rows over `entries`, drawn as the view draws its own, with its own icon
    /// requests.
    fn make_list(&self, folder: &Path, entries: Rc<Vec<Entry>>) -> List {
        let media = self.0.media.client();
        let (icons, icon_px, options) = {
            let data = self.0.data.borrow();
            (data.icons, data.icon_px, data.options)
        };
        let data = Rc::new(RefCell::new(ViewData {
            listing: Listing::Files(folder.to_path_buf(), entries),
            media: media.clone(),
            icons,
            icon_px,
            options,
            ..ViewData::default()
        }));
        let model = Rc::new(ItemsModel::new(data.clone()));
        let weak = Rc::downgrade(&model);
        media.on_ready(move |entries, _| {
            if let Some(model) = weak.upgrade() {
                let rows: Vec<Range<usize>> = entries.iter().map(|&i| i..i + 1).collect();
                model.entries_changed(&rows);
            }
        });
        List { folder: folder.to_path_buf(), shown: Some((data, model)), reading: false, scroll: 0.0, selected: None }
    }
}

/// Only `name` selected in a column's rows (none: nothing).
fn select_in(data: &RefCell<ViewData>, model: &ItemsModel, name: Option<&str>) {
    let changes = {
        let mut data = data.borrow_mut();
        let index = name.and_then(|name| data.listing.index_of(name));
        let len = data.listing.len();
        data.selection.replace(Selection::from_indices(len, index, index))
    };
    model.entries_changed(&changes);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(text: &str) -> PathBuf {
        PathBuf::from(text)
    }

    fn with_path(path: ColumnPath) -> Columns {
        Columns { path: Some(path), ..Columns::default() }
    }

    fn folders(columns: &Columns) -> Vec<PathBuf> {
        columns.path.as_ref().map(|path| path.columns().iter().map(|c| c.folder.clone()).collect()).unwrap_or_default()
    }

    /// /a, b selected in it, c in b; focus on /a/b.
    fn deep() -> ColumnPath {
        let mut path = ColumnPath::new(p("/a"));
        path.select(0, Some("b".into()), true);
        path.select(1, Some("c".into()), true);
        path
    }

    #[test]
    fn a_folder_shows_as_columns_only_when_wanted_and_stays_so() {
        let mut columns = Columns::default();
        assert!(!columns.show(Some(Path::new("/a")), false), "a list folder");
        assert!(columns.path.is_none());
        assert!(columns.show(Some(Path::new("/a")), true));
        // In columns every folder shows as columns, whatever its own view says.
        assert!(columns.show(Some(Path::new("/z")), false));
        assert_eq!(folders(&columns), [p("/z")]);
        // Results show the list, the tab still wants columns; the next folder has them.
        assert!(!columns.show(None, false));
        assert!(columns.show(Some(Path::new("/y")), false));
        columns.off();
        assert!(!columns.show(Some(Path::new("/y")), false), "the list was chosen");
        // Results whose view says columns: the next folder has them.
        assert!(!columns.show(None, true));
        assert!(columns.show(Some(Path::new("/x")), false));
    }

    #[test]
    fn the_focused_folder_shown_again_keeps_the_path() {
        let mut columns = with_path(deep());
        let generation = columns.generation;
        assert!(columns.show(Some(Path::new("/a/b")), false), "a reload, a column move");
        assert_eq!(folders(&columns).len(), 3);
        assert_eq!(columns.generation, generation, "nothing starts over");
    }

    #[test]
    fn every_root_change_starts_over_even_at_the_same_folder() {
        let mut columns = with_path(deep());
        // The sidebar's entry for the folder in focus: History::navigate does nothing there,
        // but the path starts over with it as the root.
        columns.land(Some(Path::new("/a/b")), None, true);
        assert_eq!(folders(&columns), [p("/a/b")]);
        assert_eq!(columns.path.as_ref().map(ColumnPath::focus), Some(0));
        // A reload or a tab shown again: kept.
        let mut columns = with_path(deep());
        columns.land(Some(Path::new("/a/b")), None, false);
        assert_eq!(folders(&columns).len(), 3);
        // A place that is no folder: kept for the next folder.
        columns.land(None, None, true);
        assert_eq!(folders(&columns).len(), 3);
        // No columns: nothing made.
        let mut list = Columns::default();
        list.land(Some(Path::new("/a")), None, true);
        assert!(list.path.is_none());
    }

    #[test]
    fn up_from_the_first_column_lands_on_the_path_it_waits_for() {
        let mut columns = with_path(deep());
        let mut next = deep();
        assert!(next.leave() && next.up());
        columns.next = Some(next.clone());
        columns.land(Some(Path::new("/")), None, true);
        assert_eq!(columns.path, Some(next));
        assert!(columns.next.is_none());
        // Overtaken (the sidebar went elsewhere): the root change wins, the wait is over.
        let mut columns = with_path(deep());
        columns.next = Some(ColumnPath::new(p("/")));
        columns.land(Some(Path::new("/q")), None, true);
        assert_eq!(folders(&columns), [p("/q")]);
        assert!(columns.next.is_none());
    }

    #[test]
    fn each_tab_keeps_its_path() {
        let mut columns = Columns { tab: Some(1), ..with_path(deep()) };
        columns.land(Some(Path::new("/t")), Some(2), false);
        assert!(columns.path.is_none(), "tab 2 shows the list");
        columns.land(Some(Path::new("/a/b")), Some(1), false);
        assert_eq!(folders(&columns).len(), 3, "tab 1's columns come back");
        assert!(columns.parked.is_empty());
    }

    #[test]
    fn the_memory_rule_reads_and_lets_go() {
        let mut path = ColumnPath::new(p("/0"));
        for column in 0..12 {
            path.select(column, Some("n".into()), true);
            path.enter();
        }
        // 13 columns, the focus on the last; 3 fit on screen at its end.
        let mut columns = Columns { visible: 10..13, ..with_path(path.clone()) };
        let read = columns.keep();
        let all: Vec<PathBuf> = path.columns().iter().map(|c| c.folder.clone()).collect();
        assert_eq!(read, all[4..12].to_vec(), "the 8 ancestors; the focus is the view's");
        // Scrolled back to the start: the first columns are read again, the ancestors kept.
        columns.visible = 0..3;
        let read = columns.keep();
        assert_eq!(read, all[..3].to_vec());
        assert_eq!(columns.lists.len(), 11);
        // ← twice: the focus's ancestors move; a column off screen and past them goes.
        path.leave();
        path.leave();
        columns.path = Some(path);
        columns.visible = 9..12;
        columns.keep();
        let kept: Vec<&PathBuf> = columns.lists.iter().map(|l| &l.folder).collect();
        assert!(!kept.contains(&&all[0]) && kept.contains(&&all[2]) && kept.contains(&&all[11]));
        assert!(!kept.contains(&&all[10]), "the new focus: the view's");
    }

    #[test]
    fn a_late_read_is_dropped() {
        let mut columns = with_path(deep());
        columns.visible = 0..3;
        let read = columns.keep();
        assert_eq!(read, [p("/a"), p("/a/b/c")]);
        let generation = columns.generation;
        assert!(columns.waits_for(generation, Path::new("/a")));
        assert!(!columns.waits_for(generation, Path::new("/elsewhere")));
        // The columns start over: the read under way no longer counts, and is asked again.
        columns.land(Some(Path::new("/a/b")), None, true);
        assert!(!columns.waits_for(generation, Path::new("/a")));
        columns.path = Some(deep());
        assert_eq!(columns.keep(), [p("/a"), p("/a/b/c")]);
        assert!(columns.waits_for(columns.generation, Path::new("/a")));
        // A column that closed meanwhile: its read is not wanted.
        let mut path = deep();
        path.select(0, Some("x.txt".into()), false);
        columns.path = Some(path);
        columns.keep();
        assert!(!columns.waits_for(columns.generation, Path::new("/a/b/c")));
    }

    #[test]
    fn only_the_columns_still_wanted_are_read_when_they_stay() {
        let mut columns = with_path(deep());
        columns.visible = 0..3;
        columns.pending = columns.keep();
        assert!(columns.keep().is_empty(), "asked once");
        // ↓ to a file meanwhile: /a/b/c closed before its read started.
        let mut path = deep();
        path.select(1, Some("f.txt".into()), false);
        columns.path = Some(path);
        columns.keep();
        let (due, generation) = columns.due();
        assert_eq!((due, generation), (vec![p("/a")], columns.generation));
        assert!(columns.pending.is_empty());
        // The columns started over: nothing of before is read.
        columns.pending.push(p("/a"));
        columns.restart();
        assert!(columns.due().0.is_empty());
    }

    #[test]
    fn a_listing_goes_only_to_a_column_of_the_path() {
        let mut columns = with_path(deep());
        columns.put(List::new(p("/a")));
        columns.put(List::new(p("/a/b")));
        columns.put(List::new(p("/nowhere")));
        let kept: Vec<&PathBuf> = columns.lists.iter().map(|l| &l.folder).collect();
        assert_eq!(kept, [&p("/a")], "the focused column is the view's");
        assert!(columns.take(Path::new("/a")).is_none(), "not read: nothing to show");
        assert!(columns.lists.is_empty());
    }

    #[test]
    fn a_tab_in_columns_keeps_them_for_every_folder_and_the_list_elsewhere() {
        use gezik_core::group::GroupBy;
        use gezik_core::view_rules::Place;
        let id = crate::panes::next_id();
        let view = View::new(id, slint::Weak::default(), crate::media::Media::idle(), Default::default(), None);
        let state = ViewState::default();
        let show = |listing: Listing| view.show(listing, &state, None, Place::default());
        show(super::super::listing::files("/a", &["b/", "c.txt"]));
        assert!(!view.columns_on());
        assert!(!view.show_columns(&Location::Trash), "the trash: the list");
        assert!(!view.columns_on());
        assert!(view.show_columns(&Location::Path(p("/a"))));
        assert!(view.columns_on());
        view.set_group(GroupBy::Type);
        assert_eq!((view.view_settings().group, view.group_by()), (GroupBy::Type, GroupBy::None), "kept, not applied");
        // A folder with no view of its own, then results: columns, then the list.
        show(super::super::listing::files("/z", &["y/"]));
        assert!(view.columns_on());
        show(super::super::listing::results(&[("", "a.txt")]));
        assert!(!view.columns_on());
        show(super::super::listing::files("/x", &[]));
        assert!(view.columns_on(), "back in a folder: columns again");
        // /x has no view of its own (the list): a sort changed there keeps it the list in
        // views.toml, the columns only show it so.
        view.set_sort(SortSpec { key: gezik_core::sort::SortKey::Size, dir: gezik_core::sort::SortDir::Desc });
        assert_eq!(view.0.memory.borrow_mut().get("/x").map(|v| v.mode), Some(ViewMode::List));
        assert!(view.columns_on());
        view.set_mode(ViewMode::List);
        show(super::super::listing::files("/w", &[]));
        assert!(!view.columns_on(), "the list was chosen");
        assert_eq!(view.view_settings().group, GroupBy::None, "/w has the defaults");
    }

    #[test]
    fn a_column_beside_the_focus_gives_drops_its_rows_and_folder() {
        use gezik_core::view_rules::Place;
        let view = View::new(
            crate::panes::next_id(),
            slint::Weak::default(),
            crate::media::Media::idle(),
            Default::default(),
            None,
        );
        view.show(super::super::listing::files("/a", &["b/", "c.txt"]), &ViewState::default(), None, Place::default());
        assert!(view.show_columns(&Location::Path(p("/a"))));
        view.press(0, false, false);
        assert_eq!(view.column_folder(1), Some(p("/a/b")), "b opened its column");
        assert_eq!(view.column_entry(1, 0), None, "not read yet");
        let Listing::Files(_, entries) = super::super::listing::files("/a/b", &["d/", "e.txt"]) else { unreachable!() };
        let list = view.make_list(Path::new("/a/b"), entries);
        view.0.miller.borrow_mut().put(list);
        assert_eq!(view.column_rows(), [(0.0, 0), (0.0, 0)], "off screen: no rows");
        view.0.miller.borrow_mut().visible = 0..3;
        view.column_scrolled(1, -20.0);
        assert_eq!(view.column_rows(), [(0.0, 0), (-20.0, 2)], "the focused column is the list");
        assert_eq!(view.column_entry(1, 0), Some((p("/a/b").join("d"), true)));
        assert_eq!(view.column_entry(1, 1), Some((p("/a/b").join("e.txt"), false)));
        assert_eq!(view.column_entry(1, 2), None);
        assert_eq!(view.column_clicked(0, 0, 0.0), None, "the focused column is the view's");
    }

    #[test]
    fn a_move_selects_what_it_names() {
        let state = selecting(Some("b".into()), -52.0);
        assert_eq!((state.selected, state.focus.as_deref(), state.scroll), (vec!["b".to_owned()], Some("b"), -52.0));
        assert!(selecting(None, 0.0).selected.is_empty());
    }
}
