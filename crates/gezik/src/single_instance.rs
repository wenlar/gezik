//! The running Gezik's side of the single instance (spec 5.2): another `gezik` call's paths
//! open here, in the tab already showing them or in new tabs, and the window comes forward.

use std::path::PathBuf;
use std::sync::mpsc;

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
        // On this call's own thread: looking at the paths may be slow (a network folder).
        let (opens, missing) = cli::group(&request.targets, crate::start::path_kind);
        let new_tab = request.new_tab;
        let (done, carried_out) = mpsc::channel();
        let queued = window.upgrade_in_event_loop(move |window| {
            navigation::with_current(|nav| apply(nav, opens, missing, new_tab));
            bring_to_front(&window);
            let _ = done.send(());
        });
        // A window that does not get to it is as good as gone: the caller opens its own.
        queued.is_ok() && carried_out.recv_timeout(CARRY_OUT_TIMEOUT).is_ok()
    });
}

/// Opens what another call asked for (spec 5.2, decision 2): each folder in the tab already
/// showing it (unless `new_tab`), else in a new tab, with its names selected; a bare call
/// opens the start folder the same way. Missing paths are said in the status bar.
pub fn apply(nav: &Navigator, opens: Vec<Open>, missing: Vec<PathBuf>, new_tab: bool) {
    let existing = |location: &Location| {
        if new_tab { None } else { cli::tab_for(&nav.tab_locations(), nav.active_index(), location) }
    };
    if opens.is_empty() && missing.is_empty() {
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
    }
}
