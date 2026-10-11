//! Window size and position, kept in the machine-local `state.toml`.

use gezik_config::settings::{State, WindowState};
use slint::winit_030::WinitWindowAccessor;
use slint::{ComponentHandle, LogicalSize, PhysicalPosition};

use crate::AppWindow;

thread_local! {
    /// The saved position and the second window's offset, until the native window exists.
    pub(crate) static PENDING: crate::windows::Slot<(i32, i32, i32)> = const { crate::windows::Slot::new() };
    /// Each window's size and place as last taken (or opened with): a maximized, minimized
    /// or hidden window keeps its normal rect from here.
    pub(crate) static RECT: crate::windows::Slot<WindowState> = const { crate::windows::Slot::new() };
}

/// Applies the saved size, position and sidebar width. Call before the window is shown.
/// `offset` moves a second window off the first (spec 5.3).
pub fn restore(window: &AppWindow, state: &State, offset: i32) {
    if let Some(width) = state.sidebar_width {
        window.set_sidebar_width(width as f32);
    }
    let Some(saved) = state.window else { return };
    RECT.with(|r| r.set(crate::windows::current(), saved));
    window.window().set_size(LogicalSize::new(saved.width as f32, saved.height as f32));
    if let (Some(x), Some(y)) = (saved.x, saved.y) {
        window.window().set_position(PhysicalPosition::new(x + offset, y + offset));
        // On macOS a position set before the native window exists is not used: it is set
        // again once there is one (`ensure_visible`), with the offset in points.
        PENDING.with(|p| p.set(crate::windows::current(), (x, y, offset)));
    }
    // After the normal rect, so un-maximizing goes back to it.
    if saved.maximized {
        window.window().set_maximized(true);
    }
}

/// The current window's saved position, once.
fn take_pending() -> Option<(i32, i32, i32)> {
    let id = crate::windows::current();
    let pending = PENDING.with(|p| p.get(id));
    PENDING.with(|p| p.forget(id));
    pending
}

/// The point in a window's title bar (physical pixels from its outer top left) that
/// [`ensure_visible`] wants on some monitor.
pub const TITLE_POINT: (i32, i32) = (100, 20);

/// Puts the saved size back on the first show of a `--background` start. Its native window
/// is made hidden at once, while the window's parts are not built yet: Slint gives it their
/// far too small limits and shrinks it to them, then on show grows it only to the minimum.
pub fn restore_size(window: &AppWindow, saved: WindowState) {
    window.window().set_size(LogicalSize::new(saved.width as f32, saved.height as f32));
    if saved.maximized {
        window.window().set_maximized(true);
    }
}

/// Moves the window onto the primary monitor if its restored position is on a monitor
/// that is no longer connected. Needs the native window, so call it from the event loop.
/// Returns false if the native window does not exist yet (try again later).
pub fn ensure_visible(window: &AppWindow) -> bool {
    let Some(target) = window.window().with_winit_window(|native| {
        if cfg!(target_os = "macos")
            && let Some((x, y, offset)) = take_pending()
        {
            let offset = (f64::from(offset) * native.scale_factor()).round() as i32;
            native.set_outer_position(slint::winit_030::winit::dpi::PhysicalPosition::new(x + offset, y + offset));
        }
        let position = native.outer_position().ok()?;
        // A point in the title bar must be on some monitor, so the window can be dragged.
        let (x, y) = (position.x + TITLE_POINT.0, position.y + TITLE_POINT.1);
        let on_screen = native.available_monitors().any(|monitor| {
            let (origin, size) = (monitor.position(), monitor.size());
            x >= origin.x && x < origin.x + size.width as i32 && y >= origin.y && y < origin.y + size.height as i32
        });
        if on_screen {
            return None;
        }
        let monitor = native.primary_monitor().or_else(|| native.available_monitors().next())?;
        Some(PhysicalPosition::new(monitor.position().x + 100, monitor.position().y + 100))
    }) else {
        return false;
    };
    if let Some(position) = target {
        window.window().set_position(position);
    }
    true
}

/// The current window's size (logical pixels) and position (physical pixels), taken only
/// while it is in its normal state (a minimized or maximized one has no meaningful normal
/// rect: the last one taken stays), and whether it is maximized unless minimized.
pub fn capture(window: &AppWindow) -> Option<WindowState> {
    let id = crate::windows::current();
    let saved = RECT.with(|r| r.get(id));
    let native = window.window();
    // Hidden (in the tray, or a --background start never shown): the last rect stays.
    let rect = if !native.is_visible() || native.is_minimized() {
        saved
    } else if native.is_maximized() {
        // shortcut: a window maximized before any normal rect was taken is not remembered
        // maximized (no rect to restore to); fine unless users report it.
        saved.map(|saved| WindowState { maximized: true, ..saved })
    } else {
        let size = native.size().to_logical(native.scale_factor());
        let position = native.position();
        Some(WindowState {
            width: size.width.round() as u32,
            height: size.height.round() as u32,
            x: Some(position.x),
            y: Some(position.y),
            maximized: false,
        })
    };
    if let Some(rect) = rect {
        RECT.with(|r| r.set(id, rect));
    }
    rect
}

/// Updates `state` from the window: its size and place ([`capture`]), the sidebar width always.
pub fn capture_into(window: &AppWindow, state: &mut State) {
    if let Some(rect) = capture(window) {
        state.window = Some(rect);
    }
    state.sidebar_width = Some(window.get_sidebar_width().round().clamp(120.0, 480.0) as u32);
}
