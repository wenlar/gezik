//! Platform-specific code for Gezik. Everything else in the app is platform-independent.

mod drives;
mod known;

use std::path::PathBuf;

pub use drives::{Drive, DriveKind, drive_signature, drives};
pub use known::{KnownFolder, known_folders};

/// What was right-clicked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuTarget {
    /// A file or folder.
    Item(PathBuf),
    /// Empty space in a folder's listing.
    Background(PathBuf),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuOutcome {
    /// One of Gezik's own items, by id.
    Gezik(u32),
    /// A system command ran; the folder may have changed.
    SystemCommandRan,
    Dismissed,
}

#[cfg(windows)]
mod shell_menu;
#[cfg(windows)]
pub use shell_menu::show_shell_menu;
