//! Drag and drop with other programs. Inside its own window Gezik drags files itself; this
//! takes files dropped on the window from outside, and hands a drag that leaves the window
//! to the system.

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

pub use gezik_core::drag::{Allowed, Effect, Keys};

/// What is being dragged over Gezik's window from outside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    pub paths: Vec<PathBuf>,
    pub allowed: Allowed,
    /// The right button is held: a menu follows the drop.
    pub right: bool,
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
}

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
}

/// Makes `window` take files dropped from other programs. `wake` is called from another
/// thread when events wait for [`Attached::poll`] (on the UI thread); unused on Windows.
pub fn attach(
    window: &(impl HasWindowHandle + HasDisplayHandle),
    handler: Rc<dyn DropHandler>,
    wake: Arc<dyn Fn() + Send + Sync>,
) -> Option<Attached> {
    let _ = &wake;
    #[cfg(windows)]
    {
        let inner = windows::register(window, handler)?;
        Some(Attached { inner })
    }
    #[cfg(not(windows))]
    {
        let _ = (window, handler);
        None
    }
}

impl Attached {
    /// Hands the drag of `paths` (all in one folder) to the system: the pointer left the
    /// window with the button (`right`: the right one) still down.
    pub fn drag_out(
        &self,
        paths: &[PathBuf],
        right: bool,
        on_end: Box<dyn FnOnce(DragEnd)>,
    ) -> Result<Handoff, String> {
        let _ = on_end;
        #[cfg(windows)]
        {
            self.inner.drag_out(paths, right).map(Handoff::Ended)
        }
        #[cfg(not(windows))]
        {
            let _ = (paths, right);
            Err("dragging out is not supported here".into())
        }
    }

    /// Hands waiting events to the handler (Linux; nothing elsewhere).
    pub fn poll(&self) {
        #[cfg(windows)]
        let _ = &self.inner;
    }
}

#[cfg(windows)]
mod windows;
