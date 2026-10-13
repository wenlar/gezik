//! Places inside archives in the app (spec 16 §4.4, §4.5): which open ones the tabs hold (the
//! last tab that leaves lets one go), asking for a password from a worker thread, and opening
//! a file inside an archive with its app (taken out to the temporary area first), watched
//! so a change can be put back (spec 16 §6.4, "Update in Archive").

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime};

use gezik_core::nav::Location;
use gezik_core::vfs::{VfsPath, VfsRoot};
use gezik_i18n::t;
use gezik_vfs::{Cx, Vfs};

thread_local! {
    /// The open places some tab shows (in any pane or window).
    static HELD: RefCell<Vec<Arc<dyn Vfs>>> = const { RefCell::new(Vec::new()) };
    /// Files inside archives opened with their apps, while their archive is shown.
    static OPENED: RefCell<Vec<Opened>> = const { RefCell::new(Vec::new()) };
}

/// A file's size and change time: what tells that it was saved.
type Facts = (u64, Option<SystemTime>);

fn facts(file: &Path) -> Option<Facts> {
    std::fs::metadata(file).ok().map(|meta| (meta.len(), meta.modified().ok()))
}

/// A file inside an archive opened with its app (spec 16 §6.4).
struct Opened {
    at: VfsPath,
    /// Where it was taken out to (the app has it open there).
    file: PathBuf,
    /// Keeps its archive open, and so the temporary folder the file is in.
    _vfs: Arc<dyn Vfs>,
    /// Its facts when last looked at (opened, put back, or a change ignored).
    seen: Facts,
    /// Its facts when the archive last had it (opened, or put back).
    stored: Facts,
    /// Its folder, watched: a save asks whether to put it back.
    watch: crate::folder_watch::FolderWatch,
    asking: bool,
}

/// A tab now shows `vfs`: kept while one does.
pub fn hold(vfs: Arc<dyn Vfs>) {
    HELD.with(|held| {
        let mut held = held.borrow_mut();
        held.retain(|kept| !gezik_vfs::same_root(kept.root(), vfs.root()) || Arc::ptr_eq(kept, &vfs));
        if !held.iter().any(|kept| Arc::ptr_eq(kept, &vfs)) {
            held.push(vfs);
        }
    });
}

/// Lets go of the places no tab shows any more (after tabs moved or closed).
pub fn sweep() {
    if HELD.with(|held| held.borrow().is_empty()) {
        return;
    }
    let shown: Vec<VfsRoot> = crate::panes::everywhere()
        .iter()
        .flat_map(|pane| pane.nav.tab_locations())
        .filter_map(|location| match location {
            Location::Vfs(at) => Some(at.root),
            Location::Search(spec) => match spec.scope {
                gezik_core::search::Scope::Vfs(at) => Some(at.root),
                _ => None,
            },
            _ => None,
        })
        .collect();
    let gone: Vec<Arc<dyn Vfs>> = HELD.with(|held| {
        let mut held = held.borrow_mut();
        let (keep, gone) = std::mem::take(&mut *held)
            .into_iter()
            .partition(|vfs| shown.iter().any(|root| gezik_vfs::same_root(root, vfs.root())));
        *held = keep;
        gone
    });
    // Its temporary folder is removed as it goes: off the UI thread.
    if !gone.is_empty() {
        std::thread::spawn(move || drop(gone));
    }
    // Opened files of archives no longer shown: a change not put back is asked about.
    let left: Vec<Opened> = OPENED.with(|opened| {
        let (keep, left) = std::mem::take(&mut *opened.borrow_mut())
            .into_iter()
            .partition(|o| o.asking || shown.iter().any(|root| gezik_vfs::same_root(root, &o.at.root)));
        *opened.borrow_mut() = keep;
        left
    });
    for opened in left {
        if facts(&opened.file).is_some_and(|now| now != opened.stored) {
            ask_on_leaving(opened);
        } else {
            std::thread::spawn(move || drop(opened));
        }
    }
}

/// Watches `file` (taken out as `at` and opened with its app) for saves.
fn watch_opened(at: VfsPath, file: PathBuf, vfs: Arc<dyn Vfs>) {
    let Some(now) = facts(&file) else { return };
    if OPENED.with(|opened| opened.borrow().iter().any(|o| o.file == file)) {
        return;
    }
    let window = crate::windows::current();
    let changed = file.clone();
    let watch = crate::folder_watch::FolderWatch::new(move || {
        let changed = changed.clone();
        crate::windows::post(window, move || saved(&changed));
    });
    watch.watch(file.parent());
    OPENED.with(|opened| {
        opened.borrow_mut().push(Opened { at, file, _vfs: vfs, seen: now, stored: now, watch, asking: false });
    });
}

/// Editors write a save in steps: asked once it is quiet for this long.
const SETTLE: Duration = Duration::from_millis(400);

/// The folder of an opened file changed: if the file was saved, asks (once quiet) whether to
/// put it back.
fn saved(file: &Path) {
    let due = OPENED.with(|opened| {
        let mut opened = opened.borrow_mut();
        let o = opened.iter_mut().find(|o| o.file == file)?;
        o.watch.take_change();
        let now = facts(&o.file)?;
        if now == o.seen || o.asking {
            return None;
        }
        o.asking = true;
        Some(o.file.clone())
    });
    if let Some(file) = due {
        slint::Timer::single_shot(SETTLE, move || ask_update(file));
    }
}

/// "report.docx changed": Update in Archive puts it back, Ignore leaves the archive as it is
/// (asked again at the next save).
fn ask_update(file: PathBuf) {
    let Some(at) = OPENED.with(|opened| opened.borrow().iter().find(|o| o.file == file).map(|o| o.at.clone())) else {
        return;
    };
    crate::operations::with_current(|ops| {
        let submit = ops.clone();
        // TRANSLATORS: {} is a file opened from inside an archive and saved by its app
        let title = t!("{} changed", at.name());
        // TRANSLATORS: {} is an archive's name
        let detail = t!("It was opened from {}.", at.root.name());
        ops.dialogs().ask(title, detail, &[&t!("Update in Archive"), &t!("Ignore")], move |choice| {
            let update = choice == Some(0);
            OPENED.with(|opened| {
                if let Some(o) = opened.borrow_mut().iter_mut().find(|o| o.file == file) {
                    o.asking = false;
                    if let Some(now) = facts(&o.file) {
                        o.seen = now;
                        if update {
                            o.stored = now;
                        }
                    }
                }
            });
            if update {
                let task = gezik_vfs::tasks::VfsEditTask::update(&at, file);
                submit.submit(Box::new(task), None, crate::operations::After::Nothing);
            }
        });
    });
}

/// The archive of a changed file is no longer shown (its temporary folder goes with it): put
/// it back, drop it, or keep a copy in Downloads (also when the question is closed).
fn ask_on_leaving(opened: Opened) {
    crate::operations::with_current(|ops| {
        let submit = ops.clone();
        // TRANSLATORS: {0} is a file opened from inside an archive, {1} the archive
        let title = t!("{0} changed but is not in {1}", opened.at.name(), opened.at.root.name());
        let (update, discard, keep) = (t!("Update in Archive"), t!("Discard"), t!("Keep a Copy in Downloads"));
        ops.dialogs().ask_escape(title, "", &[&update, &discard, &keep], 2, move |choice| {
            match choice {
                Some(0) => {
                    // Its folder goes with the archive: what is put back is a copy of its own.
                    let copy = gezik_vfs::stage::new_folder()
                        .map(|folder| folder.join(opened.file.file_name().unwrap_or_default()))
                        .and_then(|copy| std::fs::copy(&opened.file, &copy).map(|_| copy));
                    match copy {
                        Ok(copy) => {
                            let task = gezik_vfs::tasks::VfsEditTask::update(&opened.at, copy);
                            submit.submit(Box::new(task), None, crate::operations::After::Nothing);
                        }
                        Err(_) => keep_copy(&opened),
                    }
                }
                Some(1) => {}
                _ => keep_copy(&opened),
            }
            std::thread::spawn(move || drop(opened));
        });
    });
}

/// Copies an opened file's changes to Downloads under a free name, saying where.
fn keep_copy(opened: &Opened) {
    let Some(folder) = gezik_platform::downloads().filter(|dir| dir.is_dir()) else { return };
    let name = opened.at.name();
    let free = gezik_core::ops::names::next_free(&name, false, |candidate| folder.join(candidate).exists());
    let to = folder.join(free);
    if std::fs::copy(&opened.file, &to).is_ok() {
        // TRANSLATORS: {} is where a file was copied to
        crate::panes::active_view().note(t!("Kept {}", to.display()));
    }
}

/// Gezik quits: changes to opened files not put back are kept in Downloads (the temporary
/// area goes next).
pub fn rescue() {
    let opened = OPENED.with(|opened| std::mem::take(&mut *opened.borrow_mut()));
    for o in &opened {
        if facts(&o.file).is_some_and(|now| now != o.stored) {
            keep_copy(o);
        }
    }
}

/// A file inside an archive opened with its app: taken out to the temporary area (on a
/// thread), then opened. A folder or an archive in it is gone into instead (by the caller).
pub fn open_file(at: VfsPath) {
    let Some(vfs) = gezik_vfs::mounted(&at.root) else { return };
    let window = crate::windows::current();
    crate::panes::active_view().note(t!("Extracting {}…", at.name()));
    std::thread::spawn(move || {
        let cx = UiCx::new(&at, Box::new(|| false));
        let result = gezik_vfs::stage_here(vfs.as_ref(), &at.inner, &cx);
        // Handed to another program: it carries the archive's Mark of the Web.
        if let Ok(path) = &result {
            gezik_vfs::motw::mark_like(at.disk(), path);
        }
        crate::windows::post(window, move || {
            let view = crate::panes::active_view();
            match result.and_then(|path| open::that_detached(&path).map(|()| path)) {
                Ok(path) => {
                    view.clear_note();
                    // Watched for saves where Gezik can put it back.
                    let open = gezik_vfs::mounted(&at.root);
                    if let Some(vfs) = open.filter(|vfs| vfs.caps().has(gezik_core::vfs::Caps::WRITE)) {
                        watch_opened(at, path, vfs);
                    }
                }
                Err(err) => view.note(t!("Cannot open {}: {}", at.name(), gezik_platform::fs::describe(&err))),
            }
        });
    });
}

/// What reading an archive off the UI thread needs: the password asked in a window (the
/// thread waits for the answer), stopping once `stopped` says so, and how many entries a
/// stream's listing has passed (for the status bar).
pub struct UiCx<'a> {
    /// The archive's name, for the password question.
    name: String,
    stopped: Box<dyn Fn() -> bool + 'a>,
    /// The user cancelled the password question.
    declined: Cell<bool>,
    /// Listed so far, and when the status bar last said so.
    seen: Cell<(u64, Option<Instant>)>,
    /// Says how many entries were listed so far (none: nothing said).
    progress: Option<Box<dyn Fn(u64) + 'a>>,
    /// The first entry that failed, and why.
    failed: RefCell<Option<String>>,
}

impl<'a> UiCx<'a> {
    pub fn new(at: &VfsPath, stopped: Box<dyn Fn() -> bool + 'a>) -> UiCx<'a> {
        UiCx {
            name: at.root.name(),
            stopped,
            declined: Cell::new(false),
            seen: Cell::new((0, None)),
            progress: None,
            failed: RefCell::default(),
        }
    }

    pub fn with_progress(mut self, progress: Box<dyn Fn(u64) + 'a>) -> UiCx<'a> {
        self.progress = Some(progress);
        self
    }

    /// The password question was cancelled: the navigation is dropped.
    pub fn declined(&self) -> bool {
        self.declined.get()
    }
}

/// How often the status bar says how far a listing got.
const PROGRESS_EVERY: Duration = Duration::from_millis(300);

impl Cx for UiCx<'_> {
    fn add_bytes(&self, _: u64) {}

    fn entry_done(&self) {}

    fn stopped(&self) -> bool {
        (self.stopped)()
    }

    fn password(&self, retry: bool) -> Option<String> {
        let (tx, rx) = mpsc::channel();
        let (title, message) = crate::archives::password_text(Path::new(&self.name), retry);
        // The window in front asks (the thread reading the archive has none of its own).
        let _ = slint::invoke_from_event_loop(move || {
            crate::operations::with_current(|ops| {
                ops.dialogs().ask_secret(title, message, &[&t!("OK"), &t!("Cancel")], move |answer| {
                    let _ = tx.send(answer);
                });
            });
        });
        let answer = loop {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(answer) => break answer,
                Err(mpsc::RecvTimeoutError::Timeout) if !self.stopped() => {}
                Err(_) => break None,
            }
        };
        if answer.is_none() {
            self.declined.set(true);
        }
        answer
    }

    fn entry_failed(&self, name: &str, error: &std::io::Error) {
        self.failed.borrow_mut().get_or_insert_with(|| format!("{name}: {}", gezik_platform::fs::describe(error)));
    }

    fn wants(&self, _: &gezik_batch::archive::Entry) -> bool {
        let (count, said) = self.seen.get();
        let now = Instant::now();
        let due = said.is_none_or(|at| now.duration_since(at) >= PROGRESS_EVERY);
        if due && let Some(progress) = &self.progress {
            progress(count + 1);
        }
        self.seen.set((count + 1, if due { Some(now) } else { said }));
        true
    }
}
