//! Linux: the tray icon is a StatusNotifierItem (linux/sni.rs). It has no menu (decision 24),
//! so the pins are not used.

use crate::tray::{OnEvent, OnReady};

pub struct Tray(#[allow(dead_code, reason = "held for its Drop, which closes the connection")] crate::linux::sni::Item);

pub fn start(_pins: Vec<String>, on_event: OnEvent, on_ready: OnReady) -> Tray {
    Tray(crate::linux::sni::start(on_event, on_ready))
}

impl Tray {
    pub fn set_pins(&self, _pins: Vec<String>) {}
}
