//! Linux (and other X11/Wayland systems): the system clipboard and drag and drop, over one
//! backend per window system, chosen by the kind of window winit made.

#[cfg_attr(not(all(unix, not(target_os = "macos"))), allow(dead_code))]
pub(crate) mod uri;
#[cfg_attr(not(all(unix, not(target_os = "macos"))), allow(dead_code))]
pub(crate) mod xdnd;

#[cfg(all(unix, not(target_os = "macos")))]
pub(crate) use backend::*;

/// What to ask a clipboard owner for, from the formats it offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(all(unix, not(target_os = "macos"))), allow(dead_code))]
pub(crate) struct Ask {
    /// The format with the file list.
    pub files: u32,
    /// Also KDE's cut flag (a URI list says nothing about cut).
    pub kde_cut: bool,
}

/// GNOME's list (it says copy or cut itself), else a URI list and KDE's cut flag if offered;
/// None if no file list is offered. Each format asked for is a wait on the owner.
#[cfg_attr(not(all(unix, not(target_os = "macos"))), allow(dead_code))]
pub(crate) fn what_to_ask(offered: &[u32], gnome: u32, uri_list: u32, kde: u32) -> Option<Ask> {
    if offered.contains(&gnome) {
        Some(Ask { files: gnome, kde_cut: false })
    } else if offered.contains(&uri_list) {
        Some(Ask { files: uri_list, kde_cut: offered.contains(&kde) })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_offered_formats_are_asked_for() {
        let (gnome, uris, kde, text) = (1, 2, 3, 4);
        assert_eq!(what_to_ask(&[text, uris, gnome], gnome, uris, kde), Some(Ask { files: gnome, kde_cut: false }));
        assert_eq!(what_to_ask(&[uris, kde], gnome, uris, kde), Some(Ask { files: uris, kde_cut: true }));
        assert_eq!(what_to_ask(&[uris], gnome, uris, kde), Some(Ask { files: uris, kde_cut: false }));
        assert_eq!(what_to_ask(&[text], gnome, uris, kde), None);
        assert_eq!(what_to_ask(&[], gnome, uris, kde), None);
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod wayland;
#[cfg(all(unix, not(target_os = "macos")))]
mod x11;

#[cfg(all(unix, not(target_os = "macos")))]
mod backend {
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};

    use crate::clipboard::{ClipboardError, ClipboardFiles};
    use crate::dnd::{Answer, DragEnd, Effect, Keys, Offer, OutsideDrag};

    /// What a backend's thread hands the UI thread. `offer` is read on the UI thread, where
    /// waiting for another program's answer is allowed.
    pub(crate) enum UiEvent {
        Over {
            offer: Option<Offer>,
            x: f64,
            y: f64,
            keys: Keys,
        },
        Leave,
        Dropped {
            offer: Option<Offer>,
            x: f64,
            y: f64,
            keys: Keys,
        },
        /// Gezik's drag outside the window ended.
        SourceEnded(DragEnd),
    }

    /// A window system's clipboard and drag and drop. Called on the UI thread.
    pub(crate) trait Backend: Send + Sync {
        fn write_files(&self, paths: &[PathBuf], cut: bool) -> Result<(), ClipboardError>;
        fn read_files(&self) -> Result<Option<ClipboardFiles>, ClipboardError>;
        fn sequence(&self) -> u64;
        fn clear(&self) -> Result<(), ClipboardError>;
        fn take_events(&self) -> Vec<UiEvent>;
        /// What a drop at the last position would do.
        fn answer(&self, answer: &Answer);
        /// The drop was taken (`done`) or refused.
        fn finish(&self, done: Option<Effect>);
        fn drag_out(&self, paths: &[PathBuf]) -> Result<Box<dyn OutsideDrag>, String>;
        /// Gezik's scale factor, for backends that speak logical pixels (Wayland).
        fn set_scale(&self, _scale: f64) {}
    }

    static BACKEND: Mutex<Option<Arc<dyn Backend>>> = Mutex::new(None);

    /// The backend of Gezik's window, once it has one.
    pub(crate) fn backend() -> Option<Arc<dyn Backend>> {
        BACKEND.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
    }

    /// Starts the backend for `window`'s window system.
    pub(crate) fn start(
        window: &(impl HasWindowHandle + HasDisplayHandle),
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Option<Arc<dyn Backend>> {
        let handle = window.window_handle().ok()?;
        let started: Result<Arc<dyn Backend>, String> = match handle.as_raw() {
            RawWindowHandle::Xlib(h) => super::x11::X11::start(h.window as u32, wake).map(|b| Arc::new(b) as _),
            RawWindowHandle::Xcb(h) => super::x11::X11::start(h.window.get(), wake).map(|b| Arc::new(b) as _),
            RawWindowHandle::Wayland(h) => match window.display_handle().ok().map(|d| d.as_raw()) {
                // Safety: winit's display and surface live as long as its window, which
                // outlives Gezik's use of them (the app quits when the window closes).
                Some(RawDisplayHandle::Wayland(d)) => unsafe {
                    super::wayland::Wayland::start(d.display.as_ptr(), h.surface.as_ptr(), wake)
                        .map(|b| Arc::new(b) as _)
                },
                _ => Err("a Wayland window without a Wayland display".into()),
            },
            other => Err(format!("no clipboard or drag and drop for {other:?} windows")),
        };
        match started {
            Ok(backend) => {
                *BACKEND.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(backend.clone());
                Some(backend)
            }
            Err(why) => {
                eprintln!("gezik: {why}");
                None
            }
        }
    }
}
