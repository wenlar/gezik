//! Drag and drop with other programs. Inside its own window Gezik drags files itself; this
//! takes files dropped on the window from outside, and hands a drag that leaves the window
//! to the system.

use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

pub use gezik_core::drag::{Allowed, Effect, Keys};

/// An item another program offers with no file behind it: an e-mail attachment, a picture in
/// a browser, a file in a zip view, a macOS file promise. Its name comes from that program and
/// is untrusted (`gezik_core::drop_names`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualEntry {
    /// `Ekler\a.txt`: a folder and a file in it (Windows); `Ekler/a.txt` elsewhere.
    pub name: String,
    pub is_dir: bool,
    /// What the program says it holds; only for progress (what is written is counted).
    pub size: Option<u64>,
    /// The program could not give it (a promise that failed): why.
    pub failed: Option<String>,
}

/// Where dropped virtual items are read from, by the job writing them (any thread).
pub trait VirtualSource: Send + Sync {
    /// The items, a folder before what is in it. May wait for them (macOS: until the
    /// promised files arrive); `stop` says the job was cancelled.
    fn entries(&self, stop: &dyn Fn() -> bool) -> io::Result<Vec<VirtualEntry>>;
    /// Writes file item `index` (of `entries`) to `to`, a new file; `progress` gets the bytes
    /// written so far and stops it by returning false. Returns the bytes written.
    fn write(&self, index: usize, to: &Path, progress: &mut dyn FnMut(u64) -> bool) -> io::Result<u64>;
}

/// The items of a virtual drop, held for the job that writes them. Equal only to itself (the
/// same source).
#[derive(Clone)]
pub struct VirtualFiles(pub Arc<dyn VirtualSource>);

impl std::fmt::Debug for VirtualFiles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("VirtualFiles")
    }
}

impl PartialEq for VirtualFiles {
    fn eq(&self, other: &VirtualFiles) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for VirtualFiles {}

/// What is being dragged over Gezik's window from outside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    pub paths: Vec<PathBuf>,
    pub allowed: Allowed,
    /// The right button is held: a menu follows the drop.
    pub right: bool,
    /// No file paths, but this many items with no file behind them (always copied).
    pub virtual_count: usize,
    /// Those items, set at the drop (the platform holds them for the job from then on).
    pub virtual_files: Option<VirtualFiles>,
}

/// What a drop at the pointer would do.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Answer {
    pub effect: Option<Effect>,
    /// The folder it would go into, as named in the system's drag image ("Move to Documents").
    pub folder: Option<String>,
}

/// Gezik's side of the window's drop target. Called on the UI thread; `x`, `y` are physical
/// pixels in the window's client area.
pub trait DropHandler {
    /// The offer moved over the window (or its keys changed): what a drop here would do.
    fn over(&self, offer: &Offer, x: f64, y: f64, keys: Keys) -> Answer;
    fn leave(&self);
    /// Dropped: what was done. A move is done by Gezik itself, so the source must not delete.
    fn dropped(&self, offer: &Offer, x: f64, y: f64, keys: Keys) -> Option<Effect>;
}

/// How a drag that left Gezik's window ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragEnd {
    Dropped,
    Cancelled,
    /// The pointer came back into Gezik's window, button still down: the drag goes on there.
    Returned,
}

/// A drag outside the window that Gezik drives itself (X11); elsewhere the system drives it
/// and these do nothing.
pub trait OutsideDrag {
    /// The pointer moved (physical client pixels; outside the window).
    fn moved(&mut self, x: f64, y: f64, keys: Keys);
    fn released(&mut self);
    fn cancel(&mut self);
    /// Whether Gezik drives it from the window's pointer events (X11), not the system.
    fn driven_by_gezik(&self) -> bool {
        false
    }
}

/// Called once when a drag handed to the system ends.
pub type OnEnd = Box<dyn FnOnce(DragEnd)>;

/// A drag handed to the system.
pub enum Handoff {
    /// Over already (Windows: the system's drag loop blocks until the drop).
    Ended(DragEnd),
    /// Under way; the `on_end` given to [`Attached::drag_out`] is called when it ends.
    Running(Box<dyn OutsideDrag>),
}

/// The window's drop target, registered while this lives.
pub struct Attached {
    #[cfg(windows)]
    inner: windows::Registration,
    #[cfg(all(unix, not(target_os = "macos")))]
    inner: linux::Attachment,
    #[cfg(target_os = "macos")]
    inner: macos::Registration,
}

/// A drag the system runs by itself (macOS): nothing for Gezik to pass on.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
struct SystemDrag;

impl OutsideDrag for SystemDrag {
    fn moved(&mut self, _x: f64, _y: f64, _keys: Keys) {}
    fn released(&mut self) {}
    fn cancel(&mut self) {}
}

/// Makes `window` take files dropped from other programs. `wake` is called from another
/// thread when events wait for [`Attached::poll`] (on the UI thread); unused on Windows.
pub fn attach(
    window: &(impl HasWindowHandle + HasDisplayHandle),
    handler: Rc<dyn DropHandler>,
    wake: Arc<dyn Fn() + Send + Sync>,
) -> Option<Attached> {
    #[cfg(windows)]
    {
        let _ = wake;
        let inner = windows::register(window, handler)?;
        Some(Attached { inner })
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let inner = linux::Attachment::new(crate::linux::start(window, wake)?, handler);
        Some(Attached { inner })
    }
    #[cfg(target_os = "macos")]
    {
        let _ = wake;
        let inner = macos::register(window, handler)?;
        Some(Attached { inner })
    }
}

impl Attached {
    /// Hands the drag of `paths` (all in one folder) to the system: the pointer left the
    /// window with the button (`right`: the right one) still down.
    pub fn drag_out(&self, paths: &[PathBuf], right: bool, on_end: OnEnd) -> Result<Handoff, String> {
        #[cfg(windows)]
        {
            let _ = on_end;
            self.inner.drag_out(paths, right).map(Handoff::Ended)
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            let _ = right;
            self.inner.drag_out(paths, on_end).map(Handoff::Running)
        }
        #[cfg(target_os = "macos")]
        {
            let _ = right;
            self.inner.drag_out(paths, on_end).map(|()| Handoff::Running(Box::new(SystemDrag)))
        }
    }

    /// Tells the source what a drop would do now, when that changed with no move (Linux;
    /// Windows and macOS ask again by themselves).
    pub fn answer(&self, answer: &Answer) {
        #[cfg(all(unix, not(target_os = "macos")))]
        self.inner.answer(answer);
        #[cfg(not(all(unix, not(target_os = "macos"))))]
        let _ = answer;
    }

    /// Gezik's scale factor, for window systems that speak logical pixels (Wayland).
    pub fn set_scale(&self, scale: f32) {
        #[cfg(all(unix, not(target_os = "macos")))]
        self.inner.set_scale(f64::from(scale));
        #[cfg(not(all(unix, not(target_os = "macos"))))]
        let _ = scale;
    }

    /// Hands waiting events to the handler (Linux; nothing elsewhere).
    pub fn poll(&self) {
        #[cfg(all(unix, not(target_os = "macos")))]
        self.inner.poll();
    }
}

#[cfg(windows)]
mod windows;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(all(unix, not(target_os = "macos")))]
mod linux {
    use std::cell::RefCell;
    use std::path::PathBuf;
    use std::rc::Rc;
    use std::sync::Arc;

    use super::{DropHandler, OnEnd, OutsideDrag};
    use crate::linux::{Backend, UiEvent};

    /// The window's backend and Gezik's side of it, on the UI thread.
    pub struct Attachment {
        backend: Arc<dyn Backend>,
        handler: Rc<dyn DropHandler>,
        /// Called when Gezik's drag outside the window ends.
        on_end: RefCell<Option<OnEnd>>,
    }

    impl Attachment {
        pub fn new(backend: Arc<dyn Backend>, handler: Rc<dyn DropHandler>) -> Attachment {
            Attachment { backend, handler, on_end: RefCell::new(None) }
        }

        pub fn drag_out(&self, paths: &[PathBuf], on_end: OnEnd) -> Result<Box<dyn OutsideDrag>, String> {
            let drag = self.backend.drag_out(paths)?;
            *self.on_end.borrow_mut() = Some(on_end);
            Ok(drag)
        }

        pub fn set_scale(&self, scale: f64) {
            self.backend.set_scale(scale);
        }

        pub fn answer(&self, answer: &super::Answer) {
            self.backend.answer(answer);
        }

        pub fn poll(&self) {
            for event in self.backend.take_events() {
                match event {
                    UiEvent::Over { offer: Some(offer), x, y, keys } => {
                        let answer = self.handler.over(&offer, x, y, keys);
                        self.backend.answer(&answer);
                    }
                    UiEvent::Over { offer: None, .. } => self.backend.answer(&Default::default()),
                    UiEvent::Leave => self.handler.leave(),
                    UiEvent::Dropped { offer: Some(offer), x, y, keys } => {
                        let done = self.handler.dropped(&offer, x, y, keys);
                        self.backend.finish(done);
                    }
                    UiEvent::Dropped { offer: None, .. } => self.backend.finish(None),
                    UiEvent::SourceEnded(end) => {
                        let on_end = self.on_end.borrow_mut().take();
                        if let Some(on_end) = on_end {
                            on_end(end);
                        }
                    }
                }
            }
        }
    }
}
