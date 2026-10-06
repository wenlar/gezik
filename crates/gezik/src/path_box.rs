//! The address bar while a path is typed (spec 6.1): suggestions of the sub-folders the text
//! points into, read on a thread of their own once typing pauses for 80 ms (1 s at most), and
//! the keys that move among them. The UI thread never touches the disk here.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use gezik_config::settings::HistorySettings;
use gezik_config::shortcuts::{Chord, Key};
use gezik_config::store::ConfigStore;
use gezik_core::complete::{rank, shows_history, sort_names, split_typed};
use gezik_core::history::{FolderHistory, Visit};
use gezik_core::nav::{Location, expand_typed};
use gezik_core::ops::paths::same_path;
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::navigation::{Navigator, resolve_typed};
use crate::{AppWindow, PathRow};

/// How long typing pauses before the folder is read.
const DEBOUNCE: Duration = Duration::from_millis(80);
/// How long the list waits for a read (a network share, a sleeping disk) before it gives up on
/// it, counted from the pause in typing that asked for it (each pause waits anew).
const READ_LIMIT: Duration = Duration::from_secs(1);
/// Folders read at once at most: hung shares do not pile up threads.
const MAX_READS: usize = 4;

/// A line of the suggestion list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// A greyed group title.
    Heading(&'static str),
    /// A folder: its name, the folder it is in (or ""), and where it leads.
    Folder { title: String, detail: String, path: PathBuf },
}

/// The user's home folder, `~`: USERPROFILE on Windows, HOME elsewhere.
pub fn home() -> PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// `text` with `~` and the environment's variables put in (spec 6.1).
pub fn expand(text: &str) -> String {
    expand_typed(text, &home(), |name| std::env::var(name).ok(), cfg!(windows))
}

/// The folder whose names complete `text` (expanded) and the start of a name in it. A text
/// without a folder part completes in `base`, the folder on screen; `None` for a relative
/// text when there is none (This PC).
pub fn completion_target(text: &str, base: Option<&Path>, windows: bool) -> Option<(PathBuf, String)> {
    let (folder, typed) = split_typed(text, windows);
    let folder = if folder.is_empty() {
        base?.to_path_buf()
    } else if base.is_none()
        && !Path::new(&folder).has_root()
        && !matches!(Path::new(&folder).components().next(), Some(std::path::Component::Prefix(_)))
    {
        // `X:name` and `\path` lead somewhere in This PC too.
        return None;
    } else {
        resolve_typed(&folder, base)
    };
    Some((folder, typed))
}

/// The suggestions of `names` (the sub-folders of `folder`) for `typed`.
pub fn folder_rows(folder: &Path, names: &[String], typed: &str) -> Vec<Row> {
    rank(names, typed)
        .into_iter()
        .map(|i| Row::Folder { title: names[i].clone(), detail: String::new(), path: folder.join(&names[i]) })
        .collect()
}

/// Rows of the empty address's lists.
pub const RECENT: usize = 5;
pub const FREQUENT: usize = 7;
/// History rows under a folder's suggestions.
pub const HISTORY_MATCHES: usize = 5;

/// A visited folder's row: its name (the whole path for a root) and the folder it is in.
fn visit_row(visit: &Visit) -> Row {
    let title = visit
        .path
        .file_name()
        .map_or_else(|| visit.path.display().to_string(), |name| name.to_string_lossy().into_owned());
    let detail = visit.path.parent().map(|parent| parent.display().to_string()).unwrap_or_default();
    Row::Folder { title, detail, path: visit.path.clone() }
}

/// The list of an empty address (or `~`, a root): "Recent" (the last 5) and "Frequent" (the
/// best 7 of the others) (spec 6.2).
pub fn history_rows(history: &FolderHistory, now: u64) -> Vec<Row> {
    let recent = history.recent(RECENT);
    let frequent = history.frequent(FREQUENT, now, &recent);
    let mut rows = Vec::new();
    if !recent.is_empty() {
        rows.push(Row::Heading("Recent"));
        rows.extend(recent.iter().map(|visit| visit_row(visit)));
    }
    if !frequent.is_empty() {
        rows.push(Row::Heading("Frequent"));
        rows.extend(frequent.iter().map(|visit| visit_row(visit)));
    }
    rows
}

/// `rows` (a folder's suggestions) with the history's folders whose path holds `typed` under
/// them, after a "History" heading (at most 5, none already listed).
pub fn with_history(mut rows: Vec<Row>, history: &FolderHistory, typed: &str, now: u64) -> Vec<Row> {
    let listed: Vec<PathBuf> = rows
        .iter()
        .filter_map(|row| match row {
            Row::Folder { path, .. } => Some(path.clone()),
            Row::Heading(_) => None,
        })
        .collect();
    let found = history.matching(typed, HISTORY_MATCHES, now, |path| listed.iter().any(|l| same_path(l, path)));
    if !found.is_empty() {
        rows.push(Row::Heading("History"));
        rows.extend(found.iter().map(|visit| visit_row(visit)));
    }
    rows
}

/// `\\server\share` (and `//server`) paths are network ones, whatever the drive says.
fn is_network_text(path: &Path) -> bool {
    let text = path.to_string_lossy();
    text.starts_with(r"\\") || text.starts_with("//")
}

/// Whether `path` is gone from a local disk (spec 6.2): the system says it is not there and
/// its drive is no network one (an unanswering share keeps its folders). Touches the disk:
/// only on a worker thread.
pub fn gone_locally(path: &Path) -> bool {
    if is_network_text(path) {
        return false;
    }
    match std::fs::metadata(path) {
        // The drive is asked about through what is still there of the path.
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => gezik_platform::fs::nearest_existing(path)
            .and_then(|existing| gezik_platform::fs::drive_facts(&existing).ok())
            .is_some_and(|facts| facts.kind != gezik_platform::fs::DiskKind::Network),
        _ => false,
    }
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

thread_local! {
    /// settings.toml's `[history] remember`.
    static REMEMBER: Cell<bool> = const { Cell::new(true) };
}

fn remember() -> bool {
    REMEMBER.with(Cell::get)
}

/// settings.toml changed: with `remember = false` nothing is recorded and what was is
/// forgotten (spec 6.2).
pub fn set_settings(history: HistorySettings) {
    REMEMBER.with(|r| r.set(history.remember));
    if !history.remember {
        with_current(|p| p.forget(false));
    }
}

/// What a key does in the address bar (spec 6.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathKey {
    /// Shows the list for the text now.
    Open,
    /// Moves the chosen row down (`true`) or up.
    Move(bool),
    /// Writes the chosen row (or the first) into the field.
    Accept,
    /// Goes to the chosen row.
    Go,
    /// Closes the list (typing goes on).
    Close,
    /// The field's own.
    Field,
}

/// What `key` (with `shift`, no other modifier) does with the list `open` or not and row
/// `current` chosen: ↓ opens and moves down, ↑ moves up, Tab writes the chosen or first row
/// (or opens), → writes only a chosen row, Enter goes only to a chosen row, Esc closes; with
/// Shift every key is the field's.
pub fn path_key(key: Key, shift: bool, open: bool, current: Option<usize>) -> PathKey {
    match key {
        // Shift+arrows select text, Shift+Tab moves the focus.
        _ if shift => PathKey::Field,
        Key::Down if open => PathKey::Move(true),
        Key::Down => PathKey::Open,
        Key::Up if open => PathKey::Move(false),
        Key::Tab if open => PathKey::Accept,
        Key::Tab => PathKey::Open,
        Key::Right if open && current.is_some() => PathKey::Accept,
        Key::Enter if open && current.is_some() => PathKey::Go,
        Key::Escape if open => PathKey::Close,
        _ => PathKey::Field,
    }
}

/// The row Up/Down moves to, past headings; up from the first folder is back to the text.
pub fn step(rows: &[Row], current: Option<usize>, down: bool) -> Option<usize> {
    let folder = |i: &usize| matches!(rows[*i], Row::Folder { .. });
    match (current, down) {
        (None, true) => (0..rows.len()).find(folder),
        (None, false) => None,
        (Some(at), true) => (at + 1..rows.len()).find(folder).or(Some(at)),
        (Some(at), false) => (0..at).rev().find(folder),
    }
}

/// The first folder row.
pub fn first_folder(rows: &[Row]) -> Option<usize> {
    step(rows, None, true)
}

/// What a taken suggestion writes into the field: its path with a separator at the end, so
/// that the next suggestions are its sub-folders.
pub fn accept_text(path: &Path) -> String {
    let text = path.display().to_string();
    if text.ends_with(std::path::is_separator) { text } else { format!("{text}{}", std::path::MAIN_SEPARATOR) }
}

/// Whether `folder`'s names are in memory, being read, or to be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Read {
    Cached,
    /// Being read already: a slow share is not read twice.
    Wait,
    Start,
    /// `MAX_READS` folders are being read (hung shares): no suggestions.
    Busy,
}

pub fn read_needed(folder: &Path, cached: Option<&Path>, reading: &[PathBuf]) -> Read {
    if cached.is_some_and(|cached| same_path(cached, folder)) {
        Read::Cached
    } else if reading.iter().any(|path| same_path(path, folder)) {
        Read::Wait
    } else if reading.len() >= MAX_READS {
        Read::Busy
    } else {
        Read::Start
    }
}

/// Whether a read made in typing `of` (a generation) is kept now, in `generation`, while the
/// address is `editing`: names are never kept from one typing to the next.
pub fn keeps(of: u64, generation: u64, editing: bool) -> bool {
    editing && of == generation
}

/// The rows shown and the one chosen with ↑ ↓ (none: the text is what counts).
#[derive(Debug, Default)]
pub struct List {
    rows: Vec<Row>,
    current: Option<usize>,
}

impl List {
    fn show(&mut self, rows: Vec<Row>) {
        self.rows = rows;
        self.current = None;
    }

    /// The text changed: the chosen row may not be where it leads any more.
    fn edited(&mut self) {
        self.current = None;
    }

    /// What `key` does while the address is typed in.
    fn key(&self, key: Key, shift: bool) -> PathKey {
        path_key(key, shift, !self.rows.is_empty(), self.current)
    }
}

/// The names of the folders in `dir` (links to folders too, not Gezik's temporary names),
/// sorted as suggestions list them. Touches the disk: only on a worker thread.
pub fn list_subfolders(dir: &Path) -> std::io::Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(dir)?.flatten() {
        let is_dir = match entry.file_type() {
            Ok(kind) if kind.is_symlink() => entry.path().is_dir(),
            Ok(kind) => kind.is_dir(),
            Err(_) => false,
        };
        if is_dir
            && let Some(name) = entry.file_name().to_str()
            && !gezik_ops::pending::is_internal_name(name)
        {
            names.push(name.to_owned());
        }
    }
    sort_names(&mut names);
    Ok(names)
}

struct Inner {
    window: slint::Weak<AppWindow>,
    nav: Navigator,
    /// The text as last typed; `None`: not changed since typing began (the path on screen).
    text: RefCell<Option<String>>,
    list: RefCell<List>,
    /// Bumped when typing begins or ends: reads of an earlier typing are dropped.
    generation: Cell<u64>,
    debounce: slint::Timer,
    limit: slint::Timer,
    /// The last folder read and its sub-folders, for this typing.
    cache: RefCell<Option<(PathBuf, Rc<Vec<String>>)>>,
    /// Folders being read now.
    reading: RefCell<Vec<PathBuf>>,
    /// The folder, and the start of a name in it, the list waits for.
    wanted: RefCell<Option<(PathBuf, String)>>,
    history: RefCell<FolderHistory>,
    store: Option<ConfigStore>,
    /// The folders looked at for being gone in this typing.
    checked: RefCell<Vec<PathBuf>>,
}

#[derive(Clone)]
pub struct PathBox(Rc<Inner>);

thread_local! {
    static CURRENT: RefCell<Option<PathBox>> = const { RefCell::new(None) };
}

/// Runs `f` with this UI thread's path box, if set up.
pub fn with_current(f: impl FnOnce(&PathBox)) {
    if let Some(path_box) = CURRENT.with(|c| c.borrow().clone()) {
        f(&path_box);
    }
}

fn slint_row(row: &Row) -> PathRow {
    match row {
        Row::Heading(title) => PathRow { title: (*title).into(), detail: "".into(), path: "".into(), heading: true },
        Row::Folder { title, detail, path } => PathRow {
            title: title.as_str().into(),
            detail: detail.as_str().into(),
            path: path.display().to_string().into(),
            heading: false,
        },
    }
}

impl PathBox {
    pub fn new(window: &AppWindow, nav: Navigator, store: Option<ConfigStore>, saved: Vec<Visit>) -> PathBox {
        let history = if remember() {
            FolderHistory::from_visits(saved, now())
        } else {
            if !saved.is_empty()
                && let Some(store) = &store
            {
                store.update_state(|state| state.history.clear());
            }
            FolderHistory::default()
        };
        let this = PathBox(Rc::new(Inner {
            window: window.as_weak(),
            nav: nav.clone(),
            text: RefCell::default(),
            list: RefCell::default(),
            generation: Cell::new(0),
            debounce: slint::Timer::default(),
            limit: slint::Timer::default(),
            cache: RefCell::default(),
            reading: RefCell::default(),
            wanted: RefCell::default(),
            history: RefCell::new(history),
            store,
            checked: RefCell::default(),
        }));
        window.on_path_edited(|text| with_current(|p| p.edited(text.into())));
        // By path, not by row: whatever happened to the list meanwhile, the click goes there.
        window.on_path_chosen(|path| with_current(|p| p.go_to(PathBuf::from(path.as_str()))));
        window.on_path_editing_changed(|| with_current(PathBox::reset));
        nav.on_visited(|path| with_current(|p| p.visited(path)));
        CURRENT.with(|c| *c.borrow_mut() = Some(this.clone()));
        this
    }

    fn edited(&self, text: String) {
        *self.0.text.borrow_mut() = Some(text);
        self.0.list.borrow_mut().edited();
        self.set_current(None);
        self.0.debounce.start(slint::TimerMode::SingleShot, DEBOUNCE, || with_current(PathBox::update));
    }

    /// Typing began or ended: the list, the text and what was read are forgotten (reads
    /// still under way too, when they end).
    pub fn reset(&self) {
        self.0.generation.set(self.0.generation.get() + 1);
        self.0.text.take();
        self.0.cache.take();
        self.0.checked.borrow_mut().clear();
        self.close();
    }

    /// Closes the list (Esc) until the next key: a pause or a read still under way does not
    /// bring it back.
    fn close(&self) {
        self.0.wanted.take();
        self.0.debounce.stop();
        self.0.limit.stop();
        self.show(Vec::new());
    }

    fn typed(&self) -> String {
        self.0
            .text
            .borrow()
            .clone()
            .or_else(|| self.0.window.upgrade().map(|w| w.get_current_path().to_string()))
            .unwrap_or_default()
    }

    fn is_open(&self) -> bool {
        !self.0.list.borrow().rows.is_empty() && self.0.window.upgrade().is_some_and(|w| w.get_path_editing())
    }

    /// The list for the text now: the history (Task 8) or the folder's names, read first if
    /// they are not in memory.
    fn update(&self) {
        let typed = self.typed();
        if shows_history(&typed, cfg!(windows)) {
            return self.show(self.history_list());
        }
        let base = match self.0.nav.active_location() {
            Location::Path(path) => Some(path),
            Location::Drives => None,
        };
        let Some((folder, prefix)) = completion_target(&expand(&typed), base.as_deref(), cfg!(windows)) else {
            return self.show(self.suggestions(Vec::new()));
        };
        let cached = self.0.cache.borrow().clone();
        let decision = read_needed(&folder, cached.as_ref().map(|(f, _)| f.as_path()), &self.0.reading.borrow());
        match decision {
            Read::Cached => {
                let names = cached.map(|(_, names)| names).unwrap_or_default();
                self.show(self.suggestions(folder_rows(&folder, &names, &prefix)));
            }
            // Another folder's rows go while this one is read (the history's for this text
            // stay).
            Read::Busy => self.show(self.suggestions(Vec::new())),
            Read::Wait => {
                self.show(self.suggestions(Vec::new()));
                self.wait_for(folder, prefix);
            }
            Read::Start => {
                self.show(self.suggestions(Vec::new()));
                self.wait_for(folder.clone(), prefix);
                self.read(folder);
            }
        }
    }

    /// The folder's suggestions as shown, the history's matches under them.
    fn suggestions(&self, rows: Vec<Row>) -> Vec<Row> {
        let typed = expand(&self.typed());
        let typed = if cfg!(windows) { typed.replace('/', "\\") } else { typed };
        with_history(rows, &self.0.history.borrow(), &typed, now())
    }

    /// The list for an empty address: Recent and Frequent.
    fn history_list(&self) -> Vec<Row> {
        let rows = history_rows(&self.0.history.borrow(), now());
        self.check_gone(&rows);
        rows
    }

    /// A folder was gone to: counted, and state.toml written by its writer thread.
    fn visited(&self, path: &Path) {
        if !remember() {
            return;
        }
        self.0.history.borrow_mut().visit(path, now());
        self.save();
    }

    fn save(&self) {
        if let Some(store) = &self.0.store {
            let visits = self.0.history.borrow().visits().to_vec();
            store.update_state(move |state| state.history = visits);
        }
    }

    /// Forgets every folder visited (`clear-history`; `remember = false`); `say`: the status
    /// bar says so.
    pub fn forget(&self, say: bool) {
        let had = !self.0.history.borrow().is_empty();
        self.0.history.borrow_mut().clear();
        if had {
            self.save();
        }
        if say && let Some(window) = self.0.window.upgrade() {
            window.set_status("Folder history cleared".into());
        }
        if self.is_open() {
            self.update();
        }
    }

    /// Looks on a thread whether the folders of `rows` not looked at yet in this typing are
    /// gone from a local disk.
    fn check_gone(&self, rows: &[Row]) {
        let paths: Vec<PathBuf> = {
            let checked = self.0.checked.borrow();
            rows.iter()
                .filter_map(|row| match row {
                    Row::Folder { path, .. } if !checked.iter().any(|c| same_path(c, path)) => Some(path.clone()),
                    _ => None,
                })
                .collect()
        };
        if paths.is_empty() {
            return;
        }
        self.0.checked.borrow_mut().extend(paths.iter().cloned());
        let _ = std::thread::Builder::new().name("gezik-history-check".into()).spawn(move || {
            let gone: Vec<PathBuf> = paths.into_iter().filter(|path| gone_locally(path)).collect();
            if !gone.is_empty() {
                let _ = slint::invoke_from_event_loop(move || with_current(|p| p.gone(gone)));
            }
        });
    }

    fn gone(&self, gone: Vec<PathBuf>) {
        if !self.0.history.borrow_mut().remove(&gone) {
            return;
        }
        self.save();
        if self.is_open() {
            self.update();
        }
    }

    fn wait_for(&self, folder: PathBuf, prefix: String) {
        *self.0.wanted.borrow_mut() = Some((folder, prefix));
        self.0.limit.start(slint::TimerMode::SingleShot, READ_LIMIT, || with_current(PathBox::too_slow));
    }

    /// Reads `folder` on a thread of its own.
    fn read(&self, folder: PathBuf) {
        self.0.reading.borrow_mut().push(folder.clone());
        let generation = self.0.generation.get();
        let spawned = std::thread::Builder::new().name("gezik-complete".into()).spawn({
            let folder = folder.clone();
            move || {
                let names = list_subfolders(&folder);
                let _ = slint::invoke_from_event_loop(move || with_current(|p| p.listed(generation, folder, names)));
            }
        });
        if spawned.is_err() {
            self.0.reading.borrow_mut().retain(|path| !same_path(path, &folder));
        }
    }

    /// A read is done: kept for this typing, shown if the list still waits for it.
    fn listed(&self, generation: u64, folder: PathBuf, names: std::io::Result<Vec<String>>) {
        self.0.reading.borrow_mut().retain(|path| !same_path(path, &folder));
        let editing = self.0.window.upgrade().is_some_and(|w| w.get_path_editing());
        if !keeps(generation, self.0.generation.get(), editing) {
            return;
        }
        // An unreadable folder suggests nothing.
        let names = Rc::new(names.unwrap_or_default());
        *self.0.cache.borrow_mut() = Some((folder.clone(), names.clone()));
        let wanted = self.0.wanted.borrow().clone();
        if let Some((want, prefix)) = wanted
            && same_path(&want, &folder)
        {
            self.0.wanted.take();
            self.0.limit.stop();
            self.show(self.suggestions(folder_rows(&folder, &names, &prefix)));
        }
    }

    /// The read took longer than `READ_LIMIT`: no folder suggestions (spec 6.1); a later
    /// result waits for the next key.
    fn too_slow(&self) {
        if self.0.wanted.take().is_some() {
            self.show(self.suggestions(Vec::new()));
        }
    }

    fn show(&self, rows: Vec<Row>) {
        if let Some(window) = self.0.window.upgrade() {
            window.set_path_rows(ModelRc::new(VecModel::from(rows.iter().map(slint_row).collect::<Vec<_>>())));
            window.set_path_current(-1);
        }
        self.0.list.borrow_mut().show(rows);
    }

    fn set_current(&self, current: Option<usize>) {
        self.0.list.borrow_mut().current = current;
        if let Some(window) = self.0.window.upgrade() {
            window.set_path_current(current.and_then(|i| i32::try_from(i).ok()).unwrap_or(-1));
        }
    }

    fn path_of(&self, index: usize) -> Option<PathBuf> {
        match self.0.list.borrow().rows.get(index) {
            Some(Row::Folder { path, .. }) => Some(path.clone()),
            _ => None,
        }
    }

    /// A key while the address bar is typed in (no Ctrl, Alt or ⌘); returns whether it was used.
    pub fn chord(&self, chord: &Chord) -> bool {
        let key = || self.0.list.borrow().key(chord.key, chord.shift);
        let mut what = key();
        // Typed since the list was made (the pause not over): the list for the text first.
        if !matches!(what, PathKey::Field | PathKey::Close) && self.0.debounce.running() {
            self.0.debounce.stop();
            self.update();
            what = key();
        }
        let current = self.0.list.borrow().current;
        match what {
            PathKey::Field => return false,
            PathKey::Open => {
                self.0.debounce.stop();
                self.update();
            }
            PathKey::Move(down) => {
                let next = step(&self.0.list.borrow().rows, current, down);
                self.set_current(next);
            }
            PathKey::Accept => {
                let chosen = current.or_else(|| first_folder(&self.0.list.borrow().rows));
                if let Some(path) = chosen.and_then(|i| self.path_of(i)) {
                    self.accept(&path);
                }
            }
            PathKey::Go => {
                if let Some(path) = current.and_then(|i| self.path_of(i)) {
                    self.go_to(path);
                }
            }
            PathKey::Close => self.close(),
        }
        true
    }

    /// Writes `path` into the field (typing goes on, now inside it).
    fn accept(&self, path: &Path) {
        let Some(window) = self.0.window.upgrade() else { return };
        let text = accept_text(path);
        window.invoke_set_path_text(text.as_str().into(), i32::try_from(text.len()).unwrap_or(i32::MAX));
        self.edited(text);
    }

    fn go_to(&self, path: PathBuf) {
        if let Some(window) = self.0.window.upgrade() {
            window.set_path_editing(false);
        }
        self.reset();
        self.0.nav.go(Location::Path(path));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn folder(name: &str) -> Row {
        Row::Folder { title: name.to_owned(), detail: String::new(), path: PathBuf::from(name) }
    }

    #[test]
    fn keys_in_the_address_bar() {
        use PathKey::*;
        assert_eq!(path_key(Key::Down, false, false, None), Open);
        assert_eq!(path_key(Key::Down, false, true, None), Move(true));
        assert_eq!(path_key(Key::Up, false, true, Some(0)), Move(false));
        assert_eq!(path_key(Key::Up, false, false, None), Field);
        assert_eq!(path_key(Key::Tab, false, false, None), Open);
        assert_eq!(path_key(Key::Tab, false, true, None), Accept, "Tab takes the first when none is chosen");
        assert_eq!(path_key(Key::Tab, true, true, Some(1)), Field, "Shift+Tab is the field's");
        assert_eq!(path_key(Key::Right, false, true, None), Field, "→ moves the cursor until a row is chosen");
        assert_eq!(path_key(Key::Right, false, true, Some(2)), Accept);
        assert_eq!(path_key(Key::Enter, false, true, None), Field, "Enter goes where the text says");
        assert_eq!(path_key(Key::Enter, false, true, Some(0)), Go);
        assert_eq!(path_key(Key::Escape, false, true, None), Close);
        assert_eq!(path_key(Key::Escape, false, false, None), Field, "the second Esc ends typing");
        assert_eq!(path_key(Key::Char('a'), false, true, Some(0)), Field);
        assert_eq!(path_key(Key::Down, true, true, Some(0)), Field, "with Shift every key is the field's");
        assert_eq!(path_key(Key::Right, true, true, Some(0)), Field);
        assert_eq!(path_key(Key::Enter, true, true, Some(0)), Field);
        assert_eq!(path_key(Key::Escape, true, true, None), Field);
    }

    #[test]
    fn up_and_down_skip_headings_and_up_from_the_top_is_the_text() {
        let rows = [Row::Heading("Recent"), folder("a"), folder("b"), Row::Heading("Frequent"), folder("c")];
        assert_eq!(step(&rows, None, true), Some(1));
        assert_eq!(step(&rows, Some(2), true), Some(4));
        assert_eq!(step(&rows, Some(4), true), Some(4), "stays at the end");
        assert_eq!(step(&rows, Some(4), false), Some(2));
        assert_eq!(step(&rows, Some(1), false), None, "back to the text");
        assert_eq!(step(&[], None, true), None);
        assert_eq!(first_folder(&rows), Some(1));
    }

    #[test]
    fn a_taken_suggestion_ends_in_a_separator() {
        let path = std::path::absolute("/a").unwrap().join("b");
        assert_eq!(accept_text(&path), format!("{}{}", path.display(), std::path::MAIN_SEPARATOR));
        let root = std::path::absolute("/").unwrap();
        assert_eq!(accept_text(&root), root.display().to_string(), "a root has its separator");
    }

    #[test]
    fn a_folder_is_read_once_at_a_time() {
        let (a, b) = (Path::new("/a"), Path::new("/b"));
        assert_eq!(read_needed(a, Some(a), &[]), Read::Cached);
        assert_eq!(read_needed(a, Some(b), &[a.to_path_buf()]), Read::Wait, "a slow share is not read twice");
        assert_eq!(read_needed(a, Some(b), &[b.to_path_buf()]), Read::Start);
        assert_eq!(read_needed(a, None, &[]), Read::Start);
    }

    #[test]
    fn typing_forgets_the_chosen_row() {
        let mut list = List::default();
        list.show(vec![folder("a"), folder("b")]);
        list.current = step(&list.rows, None, true);
        assert_eq!(list.key(Key::Enter, false), PathKey::Go);
        list.edited();
        assert_eq!(list.key(Key::Enter, false), PathKey::Field, "Enter goes to the typed path");
        assert_eq!(list.key(Key::Right, false), PathKey::Field);
    }

    #[test]
    fn only_a_read_of_this_typing_is_kept() {
        assert!(keeps(3, 3, true));
        assert!(!keeps(2, 3, true), "a read from an earlier typing is dropped");
        assert!(!keeps(3, 3, false), "nothing is kept once typing ended");
    }

    #[test]
    fn at_most_four_folders_are_read_at_once() {
        let a = Path::new("/a");
        let reading: Vec<PathBuf> = ["/1", "/2", "/3", "/4"].map(PathBuf::from).to_vec();
        assert_eq!(read_needed(a, None, &reading), Read::Busy);
        assert_eq!(read_needed(a, None, &reading[..3]), Read::Start);
        assert_eq!(read_needed(Path::new("/4"), None, &reading), Read::Wait);
    }

    #[cfg(windows)]
    #[test]
    fn a_drive_or_rooted_text_completes_in_this_pc() {
        let (folder, typed) = completion_target(r"\Windows\Sys", None, true).unwrap();
        assert_eq!((folder, typed.as_str()), (std::path::absolute(r"\Windows").unwrap(), "Sys"));
        let (folder, typed) = completion_target("C:Win", None, true).unwrap();
        assert_eq!((folder, typed.as_str()), (std::path::absolute("C:").unwrap(), "Win"));
    }
    #[test]
    fn the_folder_to_list_is_where_the_text_points() {
        let windows = cfg!(windows);
        let base = std::path::absolute("/work").unwrap();
        let sep = std::path::MAIN_SEPARATOR;
        let (folder, typed) = completion_target(&format!("src{sep}ma"), Some(&base), windows).unwrap();
        assert_eq!((folder, typed.as_str()), (base.join("src"), "ma"));
        let (folder, typed) = completion_target("ma", Some(&base), windows).unwrap();
        assert_eq!((folder, typed.as_str()), (base.clone(), "ma"));
        assert!(completion_target(&format!("src{sep}ma"), None, windows).is_none(), "nothing to start from in This PC");
        let root = std::path::absolute("/").unwrap();
        let (folder, typed) = completion_target(&format!("{}tmp{sep}x", root.display()), None, windows).unwrap();
        assert_eq!((folder, typed.as_str()), (root.join("tmp"), "x"));
    }

    #[test]
    fn suggestions_lead_into_the_folder() {
        let folder = Path::new("/home/ali");
        let rows = folder_rows(folder, &["Alpha".to_owned(), "beta".to_owned(), "alpine".to_owned()], "AL");
        assert_eq!(
            rows,
            [
                Row::Folder { title: "Alpha".into(), detail: String::new(), path: folder.join("Alpha") },
                Row::Folder { title: "alpine".into(), detail: String::new(), path: folder.join("alpine") },
            ]
        );
    }

    #[test]
    fn only_folders_are_listed_sorted() {
        let dir = std::env::temp_dir().join(format!("gezik-path-box-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for name in ["b", "A", ".gezik-copying-0123456789abcdef-0"] {
            std::fs::create_dir_all(dir.join(name)).unwrap();
        }
        std::fs::write(dir.join("c.txt"), "x").unwrap();
        assert_eq!(list_subfolders(&dir).unwrap(), ["A", "b"]);
        assert!(list_subfolders(&dir.join("missing")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    const NOW: u64 = 1_800_000_000;

    fn titles(rows: &[Row]) -> Vec<String> {
        rows.iter()
            .map(|row| match row {
                Row::Heading(title) => format!("# {title}"),
                Row::Folder { title, .. } => title.clone(),
            })
            .collect()
    }

    #[test]
    fn an_empty_address_lists_recent_then_frequent() {
        let visits: Vec<Visit> = (0..6u64)
            .map(|i| Visit { path: PathBuf::from(format!("/r{i}")), count: 1, last: NOW - 100 + i })
            .chain([Visit { path: PathBuf::from("/often"), count: 40, last: NOW - 30 * 86_400 }])
            .collect();
        let history = FolderHistory::from_visits(visits, NOW);
        assert_eq!(
            titles(&history_rows(&history, NOW)),
            ["# Recent", "r5", "r4", "r3", "r2", "r1", "# Frequent", "often", "r0"]
        );
        let Row::Folder { detail, .. } = &history_rows(&history, NOW)[1] else { panic!("a folder row") };
        assert_eq!(detail, &Path::new("/").display().to_string());
        assert!(history_rows(&FolderHistory::default(), NOW).is_empty());
    }

    #[test]
    fn typed_text_adds_matching_history_under_the_folders() {
        let history = FolderHistory::from_visits(
            vec![
                Visit { path: PathBuf::from("/srv/Projeler"), count: 3, last: NOW },
                Visit { path: PathBuf::from("/home/ali/proje-eski"), count: 1, last: NOW },
                Visit { path: PathBuf::from("/home/ali/Belgeler"), count: 9, last: NOW },
            ],
            NOW,
        );
        let folder = Path::new("/home/ali");
        let rows = folder_rows(folder, &["proje-eski".to_owned(), "Projeler".to_owned()], "proje");
        let rows = with_history(rows, &history, "proje", NOW);
        assert_eq!(titles(&rows), ["proje-eski", "Projeler", "# History", "Projeler"]);
        let Row::Folder { detail, .. } = &rows[3] else { panic!("a folder row") };
        assert_eq!(detail, &Path::new("/srv").display().to_string());
        assert!(with_history(Vec::new(), &history, "zzz", NOW).is_empty());
    }

    #[test]
    fn only_a_folder_gone_from_a_local_disk_is_dropped() {
        let dir = std::env::temp_dir().join(format!("gezik-gone-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("here")).unwrap();
        assert!(!gone_locally(&dir.join("here")));
        assert!(!gone_locally(Path::new(r"\\gezik-no-such-server\share\x")));
        assert!(!gone_locally(Path::new("//gezik-no-such-server/share/x")));
        // A file system read as a network one (a container's overlay, tmpfs, btrfs on Linux)
        // keeps its folders.
        let local =
            gezik_platform::fs::drive_facts(&dir).is_ok_and(|f| f.kind != gezik_platform::fs::DiskKind::Network);
        assert_eq!(gone_locally(&dir.join("gone")), local);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Spec 1: suggestions within 100 ms of a pause: 80 ms of it waiting, the rest for reading
    /// and ranking. Run: `cargo test --release -p gezik path_box -- --ignored`.
    #[test]
    #[ignore = "timing; run in release with --ignored"]
    fn ten_thousand_folders_are_listed_and_ranked_within_budget() {
        let dir = std::env::temp_dir().join(format!("gezik-complete-bench-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for i in 0..10_000 {
            std::fs::create_dir_all(dir.join(format!("Klasör {i:05}"))).unwrap();
        }
        let started = Instant::now();
        let names = list_subfolders(&dir).unwrap();
        let rows = folder_rows(&dir, &names, "klasör 0999");
        let took = started.elapsed();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!((names.len(), rows.len()), (10_000, 10));
        assert!(took < Duration::from_millis(20), "{took:?}");
    }
}
