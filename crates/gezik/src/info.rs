//! The Info window (spec 9 §4.4), macOS and Linux: Get Info on the selection. It reads the items
//! once, on a thread, and changes exactly those: each change is a `SetAttributesTask` job (one
//! Ctrl+Z); an item that changed since is left alone and shown again as it is. Windows shows the
//! system's Properties instead (`Info::show_for`).

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use gezik_config::shortcuts::{Chord, Key};
use gezik_core::attrs::{self, Attrs, Change, Entry, HIDDEN, LOCKED, PERM_BITS, Wanted};
use gezik_core::kind::{fallback_type_name, own_type_name};
use gezik_ops::{JobId, NEEDS_ADMIN, Report, SetAttributesTask};
use gezik_platform::open_with::AppChoice;
use slint::ComponentHandle;

use crate::context_menu::{INFO_GROUP_FIRST, INFO_GROUP_MAX, OPEN_WITH_FIRST, OPEN_WITH_MAX, OPEN_WITH_OTHER};
use crate::dialog::Dialogs;
use crate::operations::{After, Operations};
use crate::stack::count_text;
use crate::{AppWindow, InfoView};

/// Most items one window shows (each is read when it opens).
pub const MAX_ITEMS: usize = 1000;

thread_local! {
    static CURRENT: RefCell<Option<Info>> = const { RefCell::new(None) };
}

/// Runs `f` with this UI thread's Info window, if set up.
pub fn with_current(f: impl FnOnce(&Info)) {
    if let Some(info) = CURRENT.with(|c| c.borrow().clone()) {
        f(&info);
    }
}

/// `get-info`: the selection, else the focused item, else the folder shown.
pub fn get_info(view: &crate::view::View) {
    let mut paths: Vec<PathBuf> = view.selected_items().into_iter().map(|(path, _)| path).collect();
    if paths.is_empty() {
        paths.extend(view.focus().and_then(|i| view.entry_path(i)).map(|(path, _)| path).or_else(|| view.folder()));
    }
    if !paths.is_empty() {
        with_current(|info| info.show_for(paths));
    }
}

/// The names the window turns ids into, read once per opening (spec: released on close).
#[derive(Debug, Default)]
struct Names {
    users: Vec<(String, u32)>,
    groups: Vec<(String, u32)>,
    /// The groups the user is in: changing to one needs no administrator.
    mine: Vec<u32>,
}

/// What a read on the loader thread found.
struct Loaded {
    items: Vec<(PathBuf, Entry)>,
    unreadable: usize,
    acl: bool,
    /// One item: created, modified, last opened.
    times: [Option<SystemTime>; 3],
    /// Read when the window opens, not on a reload.
    names: Option<Names>,
    /// macOS, one file: the apps that open it.
    apps: Vec<AppChoice>,
}

fn load(paths: &[PathBuf], names: bool) -> Loaded {
    let items: Vec<(PathBuf, Entry)> =
        paths.iter().filter_map(|path| gezik_platform::attrs::read(path).ok().map(|e| (path.clone(), e))).collect();
    let acl = items.iter().any(|(path, _)| gezik_platform::attrs::has_acl(path));
    let times = match items.as_slice() {
        [(path, _)] => std::fs::symlink_metadata(path)
            .map(|m| [m.created().ok(), m.modified().ok(), m.accessed().ok()])
            .unwrap_or_default(),
        _ => [None; 3],
    };
    let names = names.then(|| Names {
        users: gezik_platform::attrs::users(),
        groups: gezik_platform::attrs::groups(),
        mine: gezik_platform::attrs::my_groups(),
    });
    // macOS: the apps for one file (or package); the row is not there for folders and links.
    let apps = match items.as_slice() {
        [(path, e)] if !e.is_link && (!e.is_dir || gezik_core::kind::is_package_name(&path.to_string_lossy())) => {
            gezik_platform::open_with::apps(std::slice::from_ref(path))
        }
        _ => Vec::new(),
    };
    Loaded { unreadable: paths.len() - items.len(), items, acl, times, names, apps }
}

struct State {
    /// What the window was opened on: every reload reads these again.
    paths: Vec<PathBuf>,
    /// The items it shows and changes, as last read (or as the last change will leave them).
    items: Vec<(PathBuf, Entry)>,
    names: Names,
    acl: bool,
    times: [Option<SystemTime>; 3],
    apps: Vec<AppChoice>,
    size: String,
    note: (String, bool),
    /// This window's jobs and what each was to change.
    jobs: Vec<(JobId, Vec<Wanted>)>,
    /// The last job's items the system refused without administrator rights: 9b7's
    /// "Change as administrator…" does these (absolute values, spec §10.3).
    denied: Vec<Wanted>,
}

struct Inner {
    window: slint::Weak<AppWindow>,
    ops: Operations,
    dialogs: Dialogs,
    state: RefCell<Option<State>>,
    /// Bumped by each read: an older read that comes late is dropped.
    generation: Cell<u64>,
    /// Bumped by each opening and closing: the size worked out on a thread stops.
    opened: Arc<AtomicU64>,
}

#[derive(Clone)]
pub struct Info(Rc<Inner>);

impl Info {
    pub fn new(window: &AppWindow, ops: Operations, dialogs: Dialogs) -> Info {
        let info = Info(Rc::new(Inner {
            window: window.as_weak(),
            ops,
            dialogs,
            state: RefCell::default(),
            generation: Cell::new(0),
            opened: Arc::default(),
        }));
        info.install(window);
        CURRENT.with(|c| *c.borrow_mut() = Some(info.clone()));
        info
    }

    fn install(&self, window: &AppWindow) {
        let t = self.clone();
        window.on_info_toggle_bit(move |i| t.toggle_bit(i));
        let t = self.clone();
        window.on_info_toggle_flag(move |i| t.toggle_flag(if i == 0 { HIDDEN } else { LOCKED }));
        let t = self.clone();
        window.on_info_owner_accepted(move || t.accept_id(true));
        let t = self.clone();
        window.on_info_group_accepted(move || t.accept_id(false));
        let t = self.clone();
        window.on_info_octal_accepted(move || t.octal_accepted());
        let t = self.clone();
        window.on_info_enclosed(move || t.ask_enclosed());
        let t = self.clone();
        window.on_info_change_all(move || t.ask_change_all());
        let t = self.clone();
        window.on_info_close(move || t.close());
    }

    /// Get Info on `paths`: Windows' Properties, else this window.
    pub fn show_for(&self, paths: Vec<PathBuf>) {
        if !cfg!(windows) {
            return self.open(paths);
        }
        let Some(window) = self.0.window.upgrade() else { return };
        if let Err(why) = gezik_platform::show_properties(&window.window().window_handle(), &paths) {
            window.set_status(format!("Cannot show Properties: {why}").into());
        }
    }

    pub fn open(&self, paths: Vec<PathBuf>) {
        let Some(window) = self.0.window.upgrade() else { return };
        if paths.len() > MAX_ITEMS {
            return window.set_status(format!("Get Info shows at most {MAX_ITEMS} items; select fewer").into());
        }
        self.read(paths, true);
    }

    /// Reads `paths` on a thread (`names`: the window is opening).
    /// An opening also adds up the files under the items, after the read on the same thread
    /// (one thread, not two: exe size); closing or opening again stops it.
    fn read(&self, paths: Vec<PathBuf>, names: bool) {
        let generation = self.0.generation.get() + 1;
        self.0.generation.set(generation);
        let opened = if names { self.0.opened.fetch_add(1, Ordering::SeqCst) + 1 } else { 0 };
        let stop = self.0.opened.clone();
        std::thread::spawn(move || {
            let loaded = load(&paths, names);
            let sized = (names && !loaded.items.is_empty()).then(|| paths.clone());
            let _ = slint::invoke_from_event_loop(move || {
                with_current(|info| info.loaded(generation, paths, loaded));
            });
            if let Some(paths) = sized {
                let (files, bytes) = crate::archives::total_size(&paths, || stop.load(Ordering::SeqCst) != opened);
                let _ = slint::invoke_from_event_loop(move || {
                    with_current(|info| info.sized(opened, files, bytes));
                });
            }
        });
    }

    fn loaded(&self, generation: u64, paths: Vec<PathBuf>, loaded: Loaded) {
        if generation != self.0.generation.get() {
            return;
        }
        let Some(window) = self.0.window.upgrade() else { return };
        let Some(names) = loaded.names else {
            // A reload after a job: the same window, the items as they are now.
            if let Some(state) = self.0.state.borrow_mut().as_mut() {
                state.items = loaded.items;
                state.acl = loaded.acl;
                state.times = loaded.times;
                state.apps = loaded.apps;
            }
            return self.show(true);
        };
        if loaded.items.is_empty() {
            return window.set_status("Get Info: the items could not be read".into());
        }
        *self.0.state.borrow_mut() = Some(State {
            paths,
            items: loaded.items,
            names,
            acl: loaded.acl,
            times: loaded.times,
            apps: loaded.apps,
            size: "Size: calculating…".to_owned(),
            note: unreadable_note(loaded.unreadable),
            jobs: Vec::new(),
            denied: Vec::new(),
        });
        self.show(true);
        window.set_info_open(true);
    }

    fn sized(&self, opened: u64, files: u64, bytes: u64) {
        if self.0.opened.load(Ordering::SeqCst) != opened {
            return;
        }
        if let Some(state) = self.0.state.borrow_mut().as_mut() {
            state.size = crate::archives::size_text(files, bytes);
        }
        self.show(false);
    }

    /// Puts the state in the window; `fields`: the owner, group and octal fields too (not when
    /// they hold a mistake being shown).
    fn show(&self, fields: bool) {
        let Some(window) = self.0.window.upgrade() else { return };
        let state = self.0.state.borrow();
        let Some(state) = state.as_ref() else { return };
        let all: Vec<Attrs> = state.items.iter().map(|(_, e)| e.attrs).collect();
        let (perms_enabled, ids_enabled) = attrs::editable(&state.items);
        let special = attrs::special_text(&all);
        let perms = perm_digits(attrs::perm_states(&all));
        window.set_info_view(InfoView {
            title: title(&state.items).into(),
            general: general_text(&state.items, state.times, &state.size).into(),
            perms,
            perms_enabled,
            ids_enabled,
            special: special_text(&special, state.acl).into(),
            show_flags: cfg!(target_os = "macos"),
            hidden: attrs::flag_state(&all, HIDDEN).index(),
            locked: attrs::flag_state(&all, LOCKED).index(),
            app: default_app(&state.apps).into(),
            enclosed: matches!(state.items.as_slice(), [(_, e)] if e.is_dir && !e.is_link),
            note: state.note.0.as_str().into(),
            error: state.note.1,
        });
        if fields {
            window.set_info_owner(attrs::shared_name(all.iter().map(|a| a.uid), &state.names.users).into());
            window.set_info_group(attrs::shared_name(all.iter().map(|a| a.gid), &state.names.groups).into());
            window.set_info_octal(attrs::octal_text(&all).into());
        }
    }

    fn set_note(&self, text: String, error: bool) {
        if let Some(state) = self.0.state.borrow_mut().as_mut() {
            state.note = (text, error);
        }
        self.show(false);
    }

    /// Runs `change` on the window's items as one job; they show the new values at once.
    fn apply(&self, change: Change) {
        let wanted = {
            let mut state = self.0.state.borrow_mut();
            let Some(state) = state.as_mut() else { return };
            let wanted = attrs::wanted(&state.items, change);
            // `wanted` keeps the items' order: one pass sets each to what it becomes.
            let mut next = wanted.iter().peekable();
            for (path, entry) in &mut state.items {
                if let Some(w) = next.next_if(|w| &w.path == path) {
                    entry.attrs = w.to;
                }
            }
            state.note = (String::new(), false);
            wanted
        };
        if wanted.is_empty() {
            return;
        }
        let id = self.0.ops.submit(Box::new(SetAttributesTask::new(wanted.clone())), None, After::Nothing);
        if let Some(state) = self.0.state.borrow_mut().as_mut() {
            state.jobs.push((id, wanted));
        }
        self.show(true);
    }

    fn attrs(&self) -> Vec<Attrs> {
        self.0.state.borrow().as_ref().map(|s| s.items.iter().map(|(_, e)| e.attrs).collect()).unwrap_or_default()
    }

    fn toggle_bit(&self, i: i32) {
        let Some(&bit) = usize::try_from(i).ok().and_then(|i| PERM_BITS.get(i)) else { return };
        let on = attrs::tri(self.attrs().iter().map(|a| a.mode & bit != 0)).clicked();
        self.apply(Change::bit(bit, on));
    }

    fn toggle_flag(&self, flag: u32) {
        let on = attrs::flag_state(&self.attrs(), flag).clicked();
        self.apply(Change::flag(flag, on));
    }

    /// Enter in the owner (`owner`) or group field: a name or a number.
    fn accept_id(&self, owner: bool) {
        let Some(window) = self.0.window.upgrade() else { return };
        let text = if owner { window.get_info_owner() } else { window.get_info_group() };
        let parsed = {
            let state = self.0.state.borrow();
            let Some(state) = state.as_ref() else { return };
            if owner {
                attrs::parse_id(&text, &state.names.users, "user").map(Change::owner)
            } else {
                attrs::parse_id(&text, &state.names.groups, "group").map(Change::group)
            }
        };
        match parsed {
            Ok(change) => self.apply(change),
            Err(why) => self.set_note(why, true),
        }
    }

    fn octal_accepted(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        match attrs::parse_octal(&window.get_info_octal()) {
            Ok(perms) => self.apply(Change::mode(perms)),
            Err(why) => self.set_note(why, true),
        }
    }

    /// Group ▾: the groups the user is in.
    pub fn group_menu(&self) -> Vec<(u32, String, bool)> {
        let state = self.0.state.borrow();
        state.as_ref().map(|s| group_items(&s.names.mine, &s.names.groups)).unwrap_or_default()
    }

    /// Open with ▾ (macOS, one file).
    pub fn app_menu(&self) -> Vec<(u32, String, bool)> {
        self.0.state.borrow().as_ref().map(|s| app_items(&s.apps)).unwrap_or_default()
    }

    /// The one file the "Open with" row is for.
    fn app_file(&self) -> Option<PathBuf> {
        match self.0.state.borrow().as_ref()?.items.as_slice() {
            [(path, _)] => Some(path.clone()),
            _ => None,
        }
    }

    /// This file opens with `app` from now on; the row shows it at once.
    fn set_app(&self, app: PathBuf) {
        let Some(file) = self.app_file() else { return };
        let failed = |why: String| {
            let _ = slint::invoke_from_event_loop(move || {
                with_current(|info| info.set_note(format!("Cannot change the app: {why}"), true));
            });
        };
        match gezik_platform::open_with::set_default_for_file(&app, &file, failed) {
            Ok(()) => {
                if let Some(state) = self.0.state.borrow_mut().as_mut() {
                    for choice in &mut state.apps {
                        choice.default = choice.path == app;
                    }
                }
                self.show(false);
            }
            Err(why) => self.set_note(format!("Cannot change the app: {why}"), true),
        }
    }

    /// "Change All…": asks, naming the type and the app; only "Change All" changes the default
    /// for every file of this type (for this user, everywhere).
    fn ask_change_all(&self) {
        let app = self.0.state.borrow().as_ref().and_then(|s| s.apps.iter().find(|a| a.default).cloned());
        let (Some(file), Some(app)) = (self.app_file(), app) else {
            return self.set_note("Choose an app first".to_owned(), true);
        };
        // The type is read once: the one asked about is the one changed.
        let Some(uti) = gezik_platform::open_with::type_of(&file).filter(|t| !t.is_empty() && !t.starts_with("dyn."))
        else {
            return self.set_note("This file's type is not known to the system".to_owned(), true);
        };
        let name = crate::operations::items_text(std::slice::from_ref(&file));
        let kind = own_type_name(&name, false).unwrap_or_else(|| fallback_type_name(&name, false));
        let kind = format!("{kind} ({uti})");
        let info = self.clone();
        self.0.dialogs.ask(
            "Change All?",
            format!(
                "Open every {kind} file, like \"{name}\", with {}? This changes it for all your files of \
                 this type, in every app.",
                app.name
            ),
            &["Change All", "Cancel"],
            move |choice| {
                if choice == Some(0) {
                    let (note, error) = match gezik_platform::open_with::set_default_for_type(&app.path, &uti) {
                        Ok(()) => (format!("{kind} files open with {} now", app.name), false),
                        Err(why) => (format!("Cannot change the app: {why}"), true),
                    };
                    info.set_note(note, error);
                }
            },
        );
    }

    pub fn menu_chosen(&self, id: u32) {
        if id == OPEN_WITH_OTHER {
            // After the menu is gone: the panel is modal.
            let info = self.clone();
            return slint::Timer::single_shot(std::time::Duration::ZERO, move || {
                if let Some(app) = gezik_platform::open_with::choose_app() {
                    info.set_app(app);
                }
            });
        }
        if (OPEN_WITH_FIRST..OPEN_WITH_FIRST + OPEN_WITH_MAX).contains(&id) {
            let app = self.0.state.borrow().as_ref().and_then(|s| s.apps.get((id - OPEN_WITH_FIRST) as usize).cloned());
            if let Some(app) = app {
                self.set_app(app.path);
            }
            return;
        }
        if !(INFO_GROUP_FIRST..INFO_GROUP_FIRST + INFO_GROUP_MAX).contains(&id) {
            return;
        }
        let gid =
            self.0.state.borrow().as_ref().and_then(|s| s.names.mine.get((id - INFO_GROUP_FIRST) as usize).copied());
        if let Some(gid) = gid {
            self.apply(Change::group(gid));
        }
    }

    /// "Apply to enclosed items…": asks, then spreads the folder's owner, group and permissions.
    fn ask_enclosed(&self) {
        let folder = match self.0.state.borrow().as_ref().map(|s| s.items.as_slice()) {
            Some([(path, e)]) if e.is_dir && !e.is_link => (path.clone(), e.id, e.attrs),
            _ => return,
        };
        let name = crate::operations::items_text(std::slice::from_ref(&folder.0));
        let info = self.clone();
        self.0.dialogs.ask(
            "Apply to enclosed items?",
            format!(
                "Everything inside \"{name}\" gets this folder's owner, group and permissions. Files keep \
                 execute only where they had it; links are left as they are. Undo puts each item back."
            ),
            &["Apply", "Cancel"],
            move |choice| {
                if choice == Some(0) {
                    let (path, id, attrs) = folder;
                    let job =
                        info.0.ops.submit(Box::new(SetAttributesTask::enclosed(path, id, attrs)), None, After::Nothing);
                    if let Some(state) = info.0.state.borrow_mut().as_mut() {
                        state.jobs.push((job, Vec::new()));
                    }
                }
            },
        );
    }

    /// One of this window's jobs ended: what it says, and the items read again.
    pub fn job_finished(&self, id: JobId, report: &Report) {
        let paths = {
            let mut state = self.0.state.borrow_mut();
            let Some(state) = state.as_mut() else { return };
            let Some(at) = state.jobs.iter().position(|(job, _)| *job == id) else { return };
            let (_, wanted) = state.jobs.remove(at);
            state.denied = denied(&wanted, report);
            state.note = job_note(report, state.denied.len());
            state.paths.clone()
        };
        self.read(paths, false);
    }

    /// Esc closes the window wherever its focus is; returns whether the key was used.
    pub fn chord(&self, chord: &Chord) -> bool {
        let plain = !chord.shift && !chord.ctrl && !chord.alt && !chord.meta;
        if chord.key == Key::Escape && plain {
            self.close();
            return true;
        }
        false
    }

    /// Closes the window: its state, names and reads go (its jobs run on).
    pub fn close(&self) {
        self.0.generation.set(self.0.generation.get() + 1);
        self.0.opened.fetch_add(1, Ordering::SeqCst);
        self.0.state.borrow_mut().take();
        if let Some(window) = self.0.window.upgrade() {
            window.set_info_open(false);
            if !window.get_dialog_open() && !window.get_conflicts_open() {
                window.invoke_focus_list();
            }
        }
    }
}

/// "rapor.pdf Info", or "3 items".
fn title(items: &[(PathBuf, Entry)]) -> String {
    match items {
        [(path, _)] => format!("{} Info", crate::operations::items_text(std::slice::from_ref(path))),
        many => format!("{} items", many.len()),
    }
}

/// The General part: kind, size, where, and one item's dates, a line each.
fn general_text(items: &[(PathBuf, Entry)], times: [Option<SystemTime>; 3], size: &str) -> String {
    let mut lines = Vec::new();
    match items {
        [(_, entry)] if entry.is_link => lines.push("Kind: Symbolic link".to_owned()),
        [(path, entry)] => {
            let name = crate::operations::items_text(std::slice::from_ref(path));
            let kind = own_type_name(&name, entry.is_dir).unwrap_or_else(|| fallback_type_name(&name, entry.is_dir));
            lines.push(format!("Kind: {kind}"));
        }
        many => lines.push(format!("Kind: {} items", many.len())),
    }
    lines.push(size.to_owned());
    let mut folders = items.iter().filter_map(|(path, _)| path.parent());
    match folders.next() {
        Some(folder) if folders.all(|other| other == folder) => lines.push(format!("Where: {}", folder.display())),
        Some(_) => lines.push("Where: several folders".to_owned()),
        None => {}
    }
    for (label, time) in ["Created", "Modified", "Last opened"].into_iter().zip(times) {
        if let Some(time) = time {
            lines.push(format!("{label}: {}", crate::view_options::date_text(time)));
        }
    }
    lines.join("\n")
}

/// The line beside Octal: the special bits and the access control entries, a line each.
fn special_text(special: &str, acl: bool) -> String {
    let mut lines = Vec::new();
    if !special.is_empty() {
        lines.push(format!("Special: {special} (not changed here)"));
    }
    if acl {
        lines.push("This item has access control entries; they are not shown or changed here".to_owned());
    }
    lines.join("\n")
}

/// The nine boxes as one int for info.slint: box `i` is base-3 digit `i`.
fn perm_digits(states: [attrs::Tri; 9]) -> i32 {
    states.iter().rev().fold(0, |digits, state| digits * 3 + state.index())
}

fn unreadable_note(count: usize) -> (String, bool) {
    if count == 0 {
        (String::new(), false)
    } else {
        (format!("{} could not be read and are not shown", count_text(count)), true)
    }
}

/// Group ▾'s items: the user's groups by name, at most `INFO_GROUP_MAX`.
fn group_items(mine: &[u32], groups: &[(String, u32)]) -> Vec<(u32, String, bool)> {
    mine.iter()
        .take(INFO_GROUP_MAX as usize)
        .enumerate()
        .map(|(i, gid)| (INFO_GROUP_FIRST + i as u32, attrs::name_of(*gid, groups), true))
        .collect()
}

/// Open with ▾: the apps, the default marked, then Other… (as Open With ▸).
fn app_items(apps: &[AppChoice]) -> Vec<(u32, String, bool)> {
    crate::context_menu::open_with_sub(Some(apps), 0).items
}

/// The row's button: the default app, "Not set" without one, "" (no row) without apps.
fn default_app(apps: &[AppChoice]) -> String {
    match apps.iter().find(|a| a.default) {
        Some(app) => app.name.clone(),
        None if apps.is_empty() => String::new(),
        None => "Not set".to_owned(),
    }
}

/// The items of a job the system refused without administrator rights.
fn denied(wanted: &[Wanted], report: &Report) -> Vec<Wanted> {
    wanted
        .iter()
        .filter(|w| report.failures.iter().any(|f| f.path == w.path && f.message == NEEDS_ADMIN))
        .cloned()
        .collect()
}

/// What the window says after a job: nothing when all went well.
fn job_note(report: &Report, denied: usize) -> (String, bool) {
    let mut parts = Vec::new();
    if denied > 0 {
        parts.push(format!("Requires administrator: {} not changed", count_text(denied)));
    }
    let others = report.failures.len().saturating_sub(denied);
    if others > 0 {
        parts.push(format!("{} could not be changed (see the operations panel)", count_text(others)));
    }
    if report.skipped_changed > 0 {
        let what = if report.skipped_changed == 1 { "it is" } else { "they are" };
        parts.push(format!("{} changed since; shown as {what} now", count_text(report.skipped_changed)));
    }
    if report.cancelled {
        parts.push("Cancelled".to_owned());
    }
    let error = !parts.is_empty();
    (parts.join(" · "), error)
}

#[cfg(test)]
mod tests {
    use gezik_core::attrs::Identity;
    use gezik_ops::{Failure, TaskKind};

    use super::*;

    fn report(failures: &[(&str, &str)], changed: usize) -> Report {
        Report {
            kind: TaskKind::Attributes,
            cancelled: false,
            failures: failures.iter().map(|(p, m)| Failure { path: p.into(), message: (*m).to_owned() }).collect(),
            skipped: Vec::new(),
            skipped_changed: changed,
            no_trash: Vec::new(),
            results: Vec::new(),
            changed_dirs: Vec::new(),
            moved: Vec::new(),
        }
    }

    fn asked(path: &str) -> Wanted {
        Wanted {
            path: path.into(),
            id: Identity::default(),
            from: Attrs::default(),
            to: Attrs { uid: 7, ..Attrs::default() },
        }
    }

    #[test]
    fn notes_after_a_job() {
        let wanted = [asked("/d/a"), asked("/d/b"), asked("/d/c")];
        let r = report(&[("/d/a", NEEDS_ADMIN), ("/d/b", "The disk is write-protected")], 1);
        let refused = denied(&wanted, &r);
        assert_eq!(refused, [wanted[0].clone()], "only the administrator's, kept for 9b7");
        assert_eq!(
            job_note(&r, refused.len()),
            (
                "Requires administrator: 1 item not changed · 1 item could not be changed (see the operations panel) · 1 item changed since; shown as it is now"
                    .to_owned(),
                true
            )
        );
        assert_eq!(job_note(&report(&[], 0), 0), (String::new(), false));
        assert!(denied(&wanted, &report(&[("/d/z", NEEDS_ADMIN)], 0)).is_empty(), "not this window's item");
    }

    #[test]
    fn general_lines_and_titles() {
        let e = |is_dir| Entry { is_dir, ..Entry::default() };
        let one = [(PathBuf::from("/d/rapor.pdf"), e(false))];
        let text = general_text(&one, [None; 3], "Size: 1 KB in 1 file");
        assert!(text.starts_with("Kind: "), "{text}");
        assert_eq!(text.lines().skip(1).collect::<Vec<_>>(), ["Size: 1 KB in 1 file", "Where: /d"]);
        let two = [(PathBuf::from("/d/a"), e(false)), (PathBuf::from("/e/b"), e(true))];
        assert_eq!(general_text(&two, [None; 3], "Size: …"), "Kind: 2 items\nSize: …\nWhere: several folders");
        assert_eq!(title(&one), "rapor.pdf Info");
        assert_eq!(title(&two), "2 items");
        assert_eq!(unreadable_note(2), ("2 items could not be read and are not shown".to_owned(), true));
        assert_eq!(unreadable_note(0), (String::new(), false));
        let mut states = [attrs::Tri::Off; 9];
        states[0] = attrs::Tri::On;
        states[8] = attrs::Tri::Mixed;
        assert_eq!(perm_digits(states), 1 + 2 * 3i32.pow(8), "box 0 the lowest digit");
    }

    #[test]
    fn the_app_menu_lists_the_apps_then_other() {
        let app = |name: &str, default| AppChoice { path: format!("/A/{name}.app").into(), name: name.into(), default };
        let apps = [app("Preview", true), app("Safari", false)];
        assert_eq!(
            app_items(&apps),
            [
                (OPEN_WITH_FIRST, "Preview (default)".to_owned(), true),
                (OPEN_WITH_FIRST + 1, "Safari".to_owned(), true),
                (OPEN_WITH_OTHER, "Other…".to_owned(), true)
            ]
        );
        assert_eq!(default_app(&apps), "Preview");
        assert_eq!(default_app(&[app("Safari", false)]), "Not set");
        assert_eq!(default_app(&[]), "");
    }

    #[test]
    fn the_group_menu_lists_my_groups_by_name() {
        let groups = vec![("staff".to_owned(), 20), ("admin".to_owned(), 80)];
        let items = group_items(&[20, 80, 12], &groups);
        assert_eq!(
            items,
            [
                (INFO_GROUP_FIRST, "staff".to_owned(), true),
                (INFO_GROUP_FIRST + 1, "admin".to_owned(), true),
                (INFO_GROUP_FIRST + 2, "12".to_owned(), true)
            ]
        );
        let many: Vec<u32> = (0..40).collect();
        assert_eq!(group_items(&many, &groups).len(), INFO_GROUP_MAX as usize);
    }
}
