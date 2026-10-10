//! Not built yet (9b9 Task 5).
use crate::tray::{OnEvent, OnReady, TrayError};

pub struct Tray;

pub fn start(_pins: Vec<String>, _on_event: OnEvent, on_ready: OnReady) -> Tray {
    on_ready(Err(TrayError::Failed("not built yet".to_owned())));
    Tray
}

impl Tray {
    pub fn set_pins(&self, _pins: Vec<String>) {}
}
