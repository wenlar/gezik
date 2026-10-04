//! Platform-specific code for Gezik: drives, known folders, native menus, icons, thumbnails and dates. Everything else in the app is platform-independent.

mod datetime;
mod drives;
pub mod fs;
mod icons;
mod known;
mod picture;
mod text;

use std::path::PathBuf;

pub use datetime::format_datetime;
pub use drives::{Drive, DriveKind, drive_signature, drives};
pub use icons::{IconTarget, Rgba, icon, init_thread, type_name};
pub use known::{KnownFolder, known_folders};
pub use picture::{Decoded, MAX_DECODE_BYTES, MAX_DECODE_SIDE, can_decode, decode_image, thumbnail};
pub use text::decode_ansi;

/// What was right-clicked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuTarget {
    /// A file or folder.
    Item(PathBuf),
    /// Several files or folders in the same folder (a multiple selection).
    Items(Vec<PathBuf>),
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
mod keyboard;
#[cfg(windows)]
mod shell_menu;
#[cfg(windows)]
pub use keyboard::{ModifierKeys, modifier_keys_down};
#[cfg(windows)]
pub use shell_menu::show_shell_menu;
