//! Places inside archives in the app (spec 16 §4.4, §4.5): which open ones the tabs hold (the
//! last tab that leaves lets one go), asking for a password from a worker thread, and opening
//! a file inside an archive with its app (taken out to the temporary area first).

use std::cell::{Cell, RefCell};
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use gezik_core::nav::Location;
use gezik_core::vfs::{VfsPath, VfsRoot};
use gezik_i18n::t;
use gezik_vfs::{Cx, Vfs};

thread_local! {
    /// The open places some tab shows (in any pane or window).
    static HELD: RefCell<Vec<Arc<dyn Vfs>>> = const { RefCell::new(Vec::new()) };
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
                Ok(_) => view.clear_note(),
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
