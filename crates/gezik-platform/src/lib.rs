//! Platform-specific code for Gezik. Everything else in the app is platform-independent.

mod drives;
mod known;

pub use drives::{Drive, DriveKind, drive_signature, drives};
pub use known::{KnownFolder, known_folders};
