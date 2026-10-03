//! Window size and position, kept in the machine-local `state.toml`.

use gezik_config::settings::{State, WindowState};
use slint::winit_030::WinitWindowAccessor;
use slint::{ComponentHandle, LogicalSize, PhysicalPosition};

use crate::AppWindow;

/// Applies the saved size and position. Call before the window is shown.
pub fn restore(window: &AppWindow, state: &State) {
    let Some(saved) = state.window else { return };
    window.window().set_size(LogicalSize::new(saved.width as f32, saved.height as f32));
    if let (Some(x), Some(y)) = (saved.x, saved.y) {
        window.window().set_position(PhysicalPosition::new(x, y));
    }
}

/// Moves the window onto the primary monitor if its restored position is on a monitor
/// that is no longer connected. Needs the native window, so call it from the event loop.
/// Returns false if the native window does not exist yet (try again later).
pub fn ensure_visible(window: &AppWindow) -> bool {
    let Some(target) = window.window().with_winit_window(|native| {
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

/// The current size (logical pixels) and position (physical pixels).
pub fn capture(window: &AppWindow) -> State {
    let native = window.window();
    let size = native.size().to_logical(native.scale_factor());
    let position = native.position();
    State {
        window: Some(WindowState {
            width: size.width.round() as u32,
            height: size.height.round() as u32,
            x: Some(position.x),
            y: Some(position.y),
        }),
        sidebar_width: None,
    }
}
