//! Window size and position, kept in the machine-local `state.toml`.

use gezik_config::settings::{State, WindowState};
use slint::winit_030::WinitWindowAccessor;
use slint::{ComponentHandle, LogicalSize, PhysicalPosition};

use crate::AppWindow;

thread_local! {
    /// The saved position and the second window's offset, until the native window exists.
    static PENDING: std::cell::Cell<Option<(i32, i32, i32)>> = const { std::cell::Cell::new(None) };
}

/// Applies the saved size, position and sidebar width. Call before the window is shown.
/// `offset` moves a second window off the first (spec 5.3).
pub fn restore(window: &AppWindow, state: &State, offset: i32) {
    if let Some(width) = state.sidebar_width {
        window.set_sidebar_width(width as f32);
    }
    let Some(saved) = state.window else { return };
    window.window().set_size(LogicalSize::new(saved.width as f32, saved.height as f32));
    if let (Some(x), Some(y)) = (saved.x, saved.y) {
        window.window().set_position(PhysicalPosition::new(x + offset, y + offset));
        // On macOS a position set before the native window exists is not used: it is set
        // again once there is one (`ensure_visible`), with the offset in points.
        PENDING.with(|p| p.set(Some((x, y, offset))));
    }
    // After the normal rect, so un-maximizing goes back to it.
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
            && let Some((x, y, offset)) = PENDING.with(|p| p.take())
        {
            let offset = (f64::from(offset) * native.scale_factor()).round() as i32;
            native.set_outer_position(slint::winit_030::winit::dpi::PhysicalPosition::new(x + offset, y + offset));
        }
        let position = native.outer_position().ok()?;
        // A point in the title bar must be on some monitor, so the window can be dragged.
        let (x, y) = (position.x + 100, position.y + 20);
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

/// Updates `state` from the window: the size (logical pixels) and position (physical
/// pixels) only while the window is in its normal state (a minimized or maximized one has
/// no meaningful normal rect), whether it is maximized unless minimized, the sidebar width
/// always.
pub fn capture_into(window: &AppWindow, state: &mut State) {
    let native = window.window();
    if native.is_maximized() {
        // shortcut: a window maximized before any normal rect was saved is not remembered
        // maximized (no rect to restore to); fine unless users report it.
        if let Some(saved) = &mut state.window {
            saved.maximized = true;
        }
    } else if !native.is_minimized() {
        let size = native.size().to_logical(native.scale_factor());
        let position = native.position();
        state.window = Some(WindowState {
            width: size.width.round() as u32,
            height: size.height.round() as u32,
            x: Some(position.x),
            y: Some(position.y),
            maximized: false,
        });
    }
    state.sidebar_width = Some(window.get_sidebar_width().round().clamp(120.0, 480.0) as u32);
}
