//! Which modifier keys are down, for catching up after a modal loop (a native menu) that
//! took the key releases the window would otherwise have seen.

use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, VIRTUAL_KEY, VK_LCONTROL, VK_LSHIFT, VK_LWIN, VK_MENU, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN,
};

/// The modifier keys held down, as the UI thread's message queue has seen them so far.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ModifierKeys {
    pub left_shift: bool,
    pub right_shift: bool,
    pub left_control: bool,
    pub right_control: bool,
    /// Either Alt.
    pub alt: bool,
    /// Right Alt, which is AltGr on many layouts.
    pub right_alt: bool,
    pub left_meta: bool,
    pub right_meta: bool,
}

/// The modifier keys down now. Uses the thread's key state rather than the physical one:
/// a release still waiting in the queue reaches the window as usual.
pub fn modifier_keys_down() -> ModifierKeys {
    let down = |key: VIRTUAL_KEY| {
        // SAFETY: GetKeyState only reads the calling thread's key state; the high bit is "down".
        unsafe { GetKeyState(i32::from(key.0)) < 0 }
    };
    ModifierKeys {
        left_shift: down(VK_LSHIFT),
        right_shift: down(VK_RSHIFT),
        left_control: down(VK_LCONTROL),
        right_control: down(VK_RCONTROL),
        alt: down(VK_MENU),
        right_alt: down(VK_RMENU),
        left_meta: down(VK_LWIN),
        right_meta: down(VK_RWIN),
    }
}
