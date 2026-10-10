//! Not built yet (9b9 Task 4).
use crate::hotkey::{Combo, HotkeyError, OnPress, OnReady};

pub struct Hotkey;

pub fn register(_combo: Combo, _on_press: OnPress, on_ready: OnReady) -> Hotkey {
    on_ready(Err(HotkeyError::Unsupported));
    Hotkey
}
