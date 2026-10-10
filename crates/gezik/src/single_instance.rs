//! The running Gezik's side of the single instance (spec 5.2): another `gezik` call's paths
//! open here, in the tab already showing them or in new tabs, and the window comes forward.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::Instant;

use gezik_core::nav::Location;
use gezik_platform::instance::{CARRY_OUT_TIMEOUT, Listener};
use slint::ComponentHandle;

use crate::AppWindow;
use crate::cli::{self, Open};
use crate::navigation::{self, Navigator};

/// Answers other `gezik` calls from now on. Received paths are only opened as folders in
/// tabs, never run.
pub fn serve(listener: Listener, window: slint::Weak<AppWindow>) {
    listener.serve(move |request| {
        let (new_tab, trash) = (request.new_tab, request.trash);
        let window = window.clone();
        carry_out(
            Instant::now() + CARRY_OUT_TIMEOUT,
            // Looking at the paths may be slow (a network folder): not on the UI thread.
            move || cli::group(&request.targets, crate::start::path_kind),
            move |(opens, missing), turn| {
                window
                    .upgrade_in_event_loop(move |window| {
                        if turn.begin() {
                            navigation::with_current(|nav| apply(nav, opens, missing, new_tab, trash));
                            bring_to_front(&window);
                            turn.done();
                        }
                    })
                    .is_ok()
            },
        )
    });
}

/// Carries a request out by `deadline` or not at all: a caller told "not done" opens its
/// own window, so the same paths must not open here later too. `look` prepares it on a
/// thread of its own; `queue` hands it on with a [`Turn`], whose work goes ahead only if
/// it begins before the deadline.
fn carry_out<T: Send + 'static>(
    deadline: Instant,
    look: impl FnOnce() -> T + Send + 'static,
    queue: impl FnOnce(T, Turn) -> bool,
) -> bool {
    let left = || deadline.saturating_duration_since(Instant::now());
    let (sent, looked) = mpsc::channel();
    let looking = thread::Builder::new().name("gezik-instance-look".into()).spawn(move || {
        let _ = sent.send(look());
    });
    // shortcut: a look stuck on a dead server keeps its thread until the system gives up;
    // only this user's calls get here, at most 16 at once.
    let Ok(found) = looking.map_err(drop).and_then(|_| looked.recv_timeout(left()).map_err(drop)) else {
        return false;
    };
    let (done, carried_out) = mpsc::channel();
    let gate = Arc::new(AtomicBool::new(false));
    if !queue(found, Turn { gate: gate.clone(), done }) {
        return false;
    }
    if carried_out.recv_timeout(left()).is_ok() {
        return true;
    }
    if !gate.swap(true, SeqCst) {
        // Not begun: it never will.
        return false;
    }
    // Begun just in time: it ends soon (the UI thread is running it).
    carried_out.recv().is_ok()
}

/// The right to carry one request out, unless the caller gave up first.
struct Turn {
    gate: Arc<AtomicBool>,
    done: Sender<()>,
}

impl Turn {
    /// Whether the work may go ahead; false once the deadline has passed.
    fn begin(&self) -> bool {
        !self.gate.swap(true, SeqCst)
    }

    fn done(self) {
        let _ = self.done.send(());
    }
}

/// Opens what another call asked for (spec 5.2, decision 2): each folder in the tab already
/// showing it (unless `new_tab`), else in a new tab, with its names selected; a bare call
/// opens the start folder the same way; `trash` opens the trash after the paths. Missing paths
/// are said in the status bar.
pub fn apply(nav: &Navigator, opens: Vec<Open>, missing: Vec<PathBuf>, new_tab: bool, trash: bool) {
    let existing = |location: &Location| {
        if new_tab { None } else { cli::tab_for(&nav.tab_locations(), nav.active_index(), location) }
    };
    if opens.is_empty() && missing.is_empty() && !trash {
        let start = nav.start();
        match existing(&start) {
            Some(index) => nav.activate_tab(index),
            None => nav.open_tab(start, true),
        }
        return;
    }
    for Open { dir, select } in opens {
        let location = Location::Path(dir.clone());
        match existing(&location) {
            Some(index) => {
                nav.activate_tab(index);
                if !select.is_empty() {
                    nav.go_selecting(dir, select);
                }
            }
            None if select.is_empty() => nav.open_tab(location, true),
            None => nav.open_tab_selecting(dir, select),
        }
    }
    if !missing.is_empty() {
        let names: Vec<String> = missing.iter().map(|path| path.display().to_string()).collect();
        crate::view::with_current(|view| view.note(format!("{}: not found", names.join(", "))));
    }
    if trash {
        match existing(&Location::Trash) {
            Some(index) => nav.activate_tab(index),
            None => nav.open_tab(Location::Trash, true),
        }
    }
}

/// Opens `request` in this Gezik as if another `gezik` had sent it: FileManager1's calls and
/// macOS's open event. Paths are looked at on a thread, opened on the UI thread.
#[cfg_attr(windows, allow(dead_code))]
pub fn open_here(window: slint::Weak<AppWindow>, request: gezik_platform::instance::Request) {
    std::thread::spawn(move || {
        let (opens, missing) = cli::group(&request.targets, crate::start::path_kind);
        let _ = window.upgrade_in_event_loop(move |window| {
            navigation::with_current(|nav| apply(nav, opens, missing, request.new_tab, request.trash));
            bring_to_front(&window);
        });
    });
}

thread_local! {
    static WINDOW: std::cell::RefCell<Option<slint::Weak<AppWindow>>> = const { std::cell::RefCell::new(None) };
    /// Linux: another file manager holds FileManager1 (decision 18; the panel says so).
    static FM1_TAKEN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Linux: FileManager1 is wanted (started and not stopped since); a start under way
    /// that finds it unwanted lets the name go at once.
    static FM1_WANTED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(all(unix, not(target_os = "macos")))]
thread_local! {
    /// The FileManager1 name while Gezik holds it.
    static FILE_MANAGER1: std::cell::RefCell<Option<gezik_platform::file_manager1::Owner>> =
        const { std::cell::RefCell::new(None) };
}

/// The window FileManager1's calls open into; set once by main.
pub fn set_window(window: slint::Weak<AppWindow>) {
    WINDOW.with(|w| *w.borrow_mut() = Some(window));
}

/// Answers "Show in folder" from now on, if no other file manager does (Linux, UI thread).
/// Taking the name waits on the bus, so it is done on a thread.
pub fn start_file_manager1() {
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        use gezik_platform::file_manager1::{Show, serve};
        let Some(window) = WINDOW.with(|w| w.borrow().clone()) else { return };
        if FM1_WANTED.replace(true) {
            return;
        }
        std::thread::spawn(move || {
            let opener = window.clone();
            let result = serve(move |call| {
                let select = call.show != Show::Folders;
                let targets = call.paths.into_iter().map(|path| cli::Target { path, select }).collect();
                let request = gezik_platform::instance::Request { targets, trash: call.trash, ..Default::default() };
                open_here(opener.clone(), request);
            });
            let _ = slint::invoke_from_event_loop(move || match result {
                Ok(owner) if FM1_WANTED.get() => {
                    FM1_TAKEN.set(false);
                    FILE_MANAGER1.with(|f| *f.borrow_mut() = Some(owner));
                }
                Ok(_) => {}
                Err(why) => {
                    eprintln!("gezik: FileManager1: {why}");
                    FM1_TAKEN.set(why.kind() == std::io::ErrorKind::AlreadyExists);
                    FM1_WANTED.set(false);
                }
            });
        });
    }
}

/// Restore: the connection closes and the name goes.
pub fn stop_file_manager1() {
    FM1_WANTED.set(false);
    FM1_TAKEN.set(false);
    #[cfg(all(unix, not(target_os = "macos")))]
    FILE_MANAGER1.with(|f| f.borrow_mut().take());
}

pub fn file_manager1_taken() -> bool {
    FM1_TAKEN.get()
}

/// Restores and raises the window (spec 5.2): on Windows with the leave the caller gave; on
/// macOS and X11 through winit. Wayland: see `Cli::request`'s shortcut.
fn bring_to_front(window: &AppWindow) {
    #[cfg(windows)]
    gezik_platform::instance::bring_to_front(&window.window().window_handle());
    #[cfg(not(windows))]
    {
        use slint::winit_030::WinitWindowAccessor;
        window.window().with_winit_window(|native| {
            native.set_minimized(false);
            native.focus_window();
        });
        // macOS: a window coming back from the Dock takes a moment; focusing it before that is
        // ignored and the app stays behind the one in front. Again once it is back.
        #[cfg(target_os = "macos")]
        {
            let weak = window.as_weak();
            slint::Timer::single_shot(std::time::Duration::from_millis(400), move || {
                if let Some(window) = weak.upgrade() {
                    window.window().with_winit_window(|native| native.focus_window());
                }
                gezik_platform::app::activate();
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;
    use std::time::Duration;

    use super::*;

    fn soon() -> Instant {
        Instant::now() + Duration::from_millis(300)
    }

    #[test]
    fn work_done_in_time_is_said_done() {
        let ran = Arc::new(AtomicUsize::new(0));
        let r = ran.clone();
        let answer = carry_out(
            soon(),
            || 7,
            move |n, turn| {
                thread::spawn(move || {
                    if turn.begin() {
                        r.fetch_add(n, SeqCst);
                        turn.done();
                    }
                });
                true
            },
        );
        assert!(answer);
        assert_eq!(ran.load(SeqCst), 7);
    }

    #[test]
    fn a_slow_look_queues_nothing() {
        let queued = Arc::new(AtomicBool::new(false));
        let q = queued.clone();
        let answer = carry_out(
            soon(),
            || thread::sleep(Duration::from_millis(800)),
            move |(), _| {
                q.store(true, SeqCst);
                true
            },
        );
        assert!(!answer);
        thread::sleep(Duration::from_millis(700));
        assert!(!queued.load(SeqCst), "nothing is queued after the caller was told no");
    }

    #[test]
    fn work_that_begins_late_never_runs() {
        let (began, begun) = mpsc::channel();
        let answer = carry_out(
            soon(),
            || (),
            move |(), turn| {
                thread::spawn(move || {
                    thread::sleep(Duration::from_millis(600));
                    let _ = began.send(turn.begin());
                });
                true
            },
        );
        assert!(!answer);
        assert_eq!(begun.recv().ok(), Some(false), "the late work is refused its turn");
    }

    #[test]
    fn work_dropped_unrun_is_not_done() {
        assert!(!carry_out(
            soon(),
            || (),
            |(), turn| {
                drop(turn);
                true
            }
        ));
        assert!(!carry_out(soon(), || (), |(), _| false));
    }
}
