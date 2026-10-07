//! Where the pointer is, for catching the window up after a modal loop (a native menu) that
//! took the moves the window would otherwise have seen.

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::UI::WindowsAndMessaging::{
    GA_ROOT, GetAncestor, GetClientRect, GetCursorPos, PostMessageW, WM_MOUSEMOVE, WindowFromPoint,
};

/// Tells `window` where the pointer is now, if it is over its client area and no other
/// window covers it there (else Slint would show a hover that is not there), with a posted
/// move. A native menu's modal loop takes the moves made while it is open, so winit (and
/// Slint) still place the pointer where the menu was opened; the press that closed the menu
/// by clicking elsewhere in the window, still waiting in the queue, would be taken there.
/// A posted message comes before the queued input, so the press finds the pointer moved.
pub fn catch_up_pointer(window: &impl HasWindowHandle) {
    let Ok(handle) = window.window_handle() else { return };
    let RawWindowHandle::Win32(win32) = handle.as_raw() else { return };
    let hwnd = HWND(win32.hwnd.get() as *mut _);
    let mut cursor = POINT::default();
    let mut client = RECT::default();
    // SAFETY: plain Win32 calls on Gezik's own window with valid out pointers.
    unsafe {
        if GetCursorPos(&mut cursor).is_err() || GetAncestor(WindowFromPoint(cursor), GA_ROOT) != hwnd {
            return;
        }
        if !ScreenToClient(hwnd, &mut cursor).as_bool() {
            return;
        }
        if GetClientRect(hwnd, &mut client).is_err() || !inside(&client, cursor) {
            return;
        }
        let _ = PostMessageW(Some(hwnd), WM_MOUSEMOVE, WPARAM(0), LPARAM(mouse_lparam(cursor.x, cursor.y)));
    }
}

fn inside(rect: &RECT, at: POINT) -> bool {
    at.x >= rect.left && at.x < rect.right && at.y >= rect.top && at.y < rect.bottom
}

/// A mouse message's position: client x in the low word, y in the high one.
fn mouse_lparam(x: i32, y: i32) -> isize {
    (((y as u32 & 0xFFFF) << 16) | (x as u32 & 0xFFFF)) as isize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_position_packs_x_low_and_y_high() {
        assert_eq!(mouse_lparam(0, 0), 0);
        assert_eq!(mouse_lparam(0x12, 0x34), 0x0034_0012);
        assert_eq!(mouse_lparam(1919, 1079), (1079 << 16) | 1919);
    }

    #[test]
    fn only_a_pointer_over_the_client_area_counts() {
        let rect = RECT { left: 0, top: 0, right: 900, bottom: 600 };
        assert!(inside(&rect, POINT { x: 0, y: 0 }));
        assert!(inside(&rect, POINT { x: 899, y: 599 }));
        assert!(!inside(&rect, POINT { x: 900, y: 10 }));
        assert!(!inside(&rect, POINT { x: 10, y: -1 }));
    }
}
