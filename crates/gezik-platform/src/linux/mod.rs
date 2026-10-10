//! Linux (and other X11/Wayland systems): the system clipboard and drag and drop, over one
//! backend per window system, chosen by the kind of window winit made.

#[cfg(any(all(unix, not(target_os = "macos")), test))]
pub mod dbus;
#[cfg(any(all(unix, not(target_os = "macos")), test))]
pub mod file_manager1;
#[cfg_attr(not(all(unix, not(target_os = "macos"))), allow(dead_code))]
pub(crate) mod icon_theme;
#[cfg_attr(not(all(unix, not(target_os = "macos"))), allow(dead_code))]
pub(crate) mod mime;
// shortcut: nothing calls it on Linux until Open With (9b10 Task 5); then cfg_attr like the rest.
#[allow(dead_code)]
pub(crate) mod desktop_entry;
#[cfg_attr(not(all(unix, not(target_os = "macos"))), allow(dead_code))]
pub(crate) mod thumbs;
#[cfg_attr(not(all(unix, not(target_os = "macos"))), allow(dead_code))]
pub(crate) mod uri;
#[cfg_attr(not(all(unix, not(target_os = "macos"))), allow(dead_code))]
pub(crate) mod xdnd;

#[cfg(all(unix, not(target_os = "macos")))]
pub(crate) use backend::*;

/// The most a clipboard transfer is read for: a picture (or anything) beyond it is dropped.
#[cfg_attr(not(all(unix, not(target_os = "macos"))), allow(dead_code))]
pub(crate) const MAX_TRANSFER_BYTES: usize = 256 * 1024 * 1024;

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

/// `text` in Latin-1, for the X11 `STRING` target: other characters become `?`.
#[cfg_attr(not(all(unix, not(target_os = "macos"))), allow(dead_code))]
pub(crate) fn latin1(text: &str) -> Vec<u8> {
    text.chars().map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?')).collect()
}

/// The text types Gezik reads, best first.
pub(crate) const TEXT_TYPES: [&str; 5] = ["UTF8_STRING", "text/plain;charset=utf-8", "text/plain", "TEXT", "STRING"];

/// What paste would write as a file, from the types (X11 targets, Wayland MIME types) the
/// clipboard's owner offers: None while it offers files (they paste as files), else a PNG
/// picture before text (a browser's picture often comes with text).
#[cfg_attr(not(all(unix, not(target_os = "macos"))), allow(dead_code))]
pub(crate) fn paste_kind_of(offered: &[&str]) -> Option<gezik_core::templates::PasteKind> {
    use gezik_core::templates::PasteKind;
    let has = |name: &str| offered.contains(&name);
    if has("x-special/gnome-copied-files") || has("text/uri-list") {
        None
    } else if has("image/png") {
        Some(PasteKind::Image)
    } else {
        text_type(offered).map(|_| PasteKind::Text)
    }
}

/// The best text type among `offered`.
#[cfg_attr(not(all(unix, not(target_os = "macos"))), allow(dead_code))]
pub(crate) fn text_type<'a>(offered: &[&'a str]) -> Option<&'a str> {
    TEXT_TYPES.iter().find_map(|wanted| offered.iter().find(|o| **o == *wanted).copied())
}

/// The offers asked for at `at` (the clipboard's change counter), if that is the last ask.
#[cfg_attr(not(all(unix, not(target_os = "macos"))), allow(dead_code))]
pub(crate) fn cached_offers(cache: &Option<(u64, Vec<&'static str>)>, at: u64) -> Option<Vec<&'static str>> {
    cache.as_ref().filter(|(asked_at, _)| *asked_at == at).map(|(_, names)| names.clone())
}

/// `STRING` (Latin-1) bytes as text.
#[cfg_attr(not(all(unix, not(target_os = "macos"))), allow(dead_code))]
pub(crate) fn from_latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| char::from(b)).collect()
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

    #[test]
    fn latin1_keeps_what_it_can() {
        assert_eq!(latin1("ça ş"), [0xE7, b'a', b' ', b'?']);
    }

    #[test]
    fn files_win_then_an_image_then_text() {
        use gezik_core::templates::PasteKind;
        // Gezik's own files also offer text; a browser's picture often comes with text too.
        assert_eq!(paste_kind_of(&["TARGETS", "text/uri-list", "UTF8_STRING"]), None);
        assert_eq!(paste_kind_of(&["x-special/gnome-copied-files", "image/png"]), None);
        assert_eq!(paste_kind_of(&["text/html", "image/png", "UTF8_STRING"]), Some(PasteKind::Image));
        assert_eq!(paste_kind_of(&["TEXT", "STRING"]), Some(PasteKind::Text));
        assert_eq!(paste_kind_of(&["text/html"]), None, "nothing paste can write");
        assert_eq!(text_type(&["STRING", "text/plain", "UTF8_STRING"]), Some("UTF8_STRING"));
        assert_eq!(text_type(&["STRING", "text/plain"]), Some("text/plain"));
        assert_eq!(text_type(&["image/png"]), None);
        assert_eq!(from_latin1(&[0xE7, b'a']), "ça");
    }

    #[test]
    fn offers_are_reused_until_the_clipboard_changes() {
        let cache = Some((3, vec!["image/png"]));
        assert_eq!(cached_offers(&cache, 3), Some(vec!["image/png"]));
        assert_eq!(cached_offers(&cache, 4), None, "the clipboard changed");
        assert_eq!(cached_offers(&Some((3, Vec::new())), 3), Some(Vec::new()), "no answer is kept too");
        assert_eq!(cached_offers(&None, 3), None);
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

    use crate::clipboard::{ClipboardError, ClipboardFiles, ClipboardImage, PasteKind};
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
        fn write_text(&self, text: &str) -> Result<(), ClipboardError>;
        fn read_files(&self) -> Result<Option<ClipboardFiles>, ClipboardError>;
        /// What paste would write as a file (formats only).
        fn paste_kind(&self) -> Option<PasteKind>;
        fn read_image(&self) -> Result<Option<ClipboardImage>, ClipboardError>;
        fn read_text(&self) -> Result<Option<String>, ClipboardError>;
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
