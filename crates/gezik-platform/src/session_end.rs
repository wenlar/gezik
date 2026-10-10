//! Windows: the session ends (sign out, restart, shut down). Windows sends WM_ENDSESSION to
//! every top-level window, hidden ones too, and ends the process once it is answered; winit
//! passes it on to no one, so Gezik's window is subclassed to save first.

use std::cell::RefCell;

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::WM_ENDSESSION;

const SUBCLASS_ID: usize = 0x6765_7A65;

thread_local! {
    static ON_END: RefCell<Option<Box<dyn Fn()>>> = RefCell::new(None);
}

/// Calls `on_end` (on the window's thread) when the session is ending, before the process is
/// ended. Call once, on the UI thread; `false`: no native window yet, try again later.
pub fn on_session_end(window: &impl HasWindowHandle, on_end: impl Fn() + 'static) -> bool {
    let Ok(handle) = window.window_handle() else { return false };
    let RawWindowHandle::Win32(raw) = handle.as_raw() else { return false };
    ON_END.with(|f| *f.borrow_mut() = Some(Box::new(on_end)));
    let hwnd = HWND(raw.hwnd.get() as *mut _);
    if !unsafe { SetWindowSubclass(hwnd, Some(messages), SUBCLASS_ID, 0) }.as_bool() {
        eprintln!("gezik: SetWindowSubclass failed; signing out will not save the window state");
    }
    true
}

unsafe extern "system" fn messages(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    // wParam TRUE: the session really ends (FALSE: someone refused it).
    if msg == WM_ENDSESSION && wparam.0 != 0 {
        ON_END.with(|f| {
            if let Ok(f) = f.try_borrow()
                && let Some(f) = &*f
            {
                f();
            }
        });
    }
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}
