//! Linux: Wayland through the GlobalShortcuts portal, X11 by XGrabKey (deviation 15).

use crate::hotkey::{Combo, OnPress, OnReady, portal_trigger};

#[allow(dead_code, reason = "held for its Drop, which releases the shortcut")]
pub enum Hotkey {
    X11(crate::linux::hotkey_x11::Grab),
    Portal(crate::linux::portal::Shortcut),
}

pub fn register(combo: Combo, on_press: OnPress, on_ready: OnReady) -> Hotkey {
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        Hotkey::Portal(crate::linux::portal::bind_shortcut(portal_trigger(&combo), on_press, on_ready))
    } else {
        Hotkey::X11(crate::linux::hotkey_x11::grab(combo, on_press, on_ready))
    }
}
