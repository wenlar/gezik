//! Platform-specific code for Gezik: drives, known folders, native menus, icons, thumbnails and dates. Everything else in the app is platform-independent.

pub mod clipboard;
mod datetime;
pub mod dnd;
mod drives;
pub mod everything;
pub mod fs;
pub mod http;
mod icons;
pub mod instance;
mod known;
pub mod link;
mod linux;
mod locale;
mod picture;
pub mod priority;
pub mod process;
pub mod taskbar;
pub mod terminal;
mod text;

use std::path::PathBuf;

pub use datetime::{civil_from_days, format_date, format_datetime, local_date_parts};
pub use drives::{Drive, DriveKind, drive_signature, drives};
pub use icons::{IconTarget, Rgba, icon, init_thread, type_name};
pub use known::{KnownFolder, known_folders};
pub use locale::language;
pub use picture::{Decoded, MAX_DECODE_BYTES, MAX_DECODE_SIDE, can_decode, decode_image, thumbnail};
pub use process::{ChildProcess, Lines, process_alive};
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

/// Explorer menu commands Gezik does itself, with its own engine (not Explorer's dialogs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellVerb {
    Cut,
    Copy,
    Paste,
    Delete,
    Rename,
    /// Explorer's "Create shortcut".
    Link,
}

impl ShellVerb {
    /// The verb for a canonical command name (`GetCommandString`), if Gezik does it.
    pub fn from_name(name: &[u8]) -> Option<ShellVerb> {
        match name.to_ascii_lowercase().as_slice() {
            b"cut" => Some(ShellVerb::Cut),
            b"copy" => Some(ShellVerb::Copy),
            b"paste" => Some(ShellVerb::Paste),
            b"delete" => Some(ShellVerb::Delete),
            b"rename" => Some(ShellVerb::Rename),
            b"link" => Some(ShellVerb::Link),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuOutcome {
    /// One of Gezik's own items, by id.
    Gezik(u32),
    /// An Explorer command Gezik does itself; nothing ran yet.
    Verb(ShellVerb),
    /// A system command ran; the folder may have changed.
    SystemCommandRan,
    Dismissed,
}

#[cfg(target_os = "macos")]
pub mod app;
#[cfg(target_os = "macos")]
pub mod key_place;
#[cfg(windows)]
mod keyboard;
#[cfg(windows)]
mod pointer;
#[cfg(windows)]
mod removal;
#[cfg(windows)]
mod shell_menu;
#[cfg(windows)]
pub use keyboard::{ModifierKeys, modifier_keys_down};
#[cfg(windows)]
pub use pointer::catch_up_pointer;
#[cfg(windows)]
pub use removal::{RemovalWatch, watch_removal};
#[cfg(windows)]
pub use shell_menu::{FIRST_SHELL_ID, ShellSubmenu, show_shell_menu};

/// Elsewhere a drive is not asked about before it goes: nothing to watch.
#[cfg(not(windows))]
mod removal_elsewhere {
    pub struct RemovalWatch;

    pub fn watch_removal(
        _window: &impl raw_window_handle::HasWindowHandle,
        _folder: &std::path::Path,
        _on_asked: impl Fn() + 'static,
    ) -> Option<RemovalWatch> {
        None
    }
}
#[cfg(not(windows))]
pub use removal_elsewhere::{RemovalWatch, watch_removal};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explorer_verbs_gezik_does() {
        assert_eq!(ShellVerb::from_name(b"link"), Some(ShellVerb::Link));
        assert_eq!(ShellVerb::from_name(b"delete"), Some(ShellVerb::Delete));
        assert_eq!(ShellVerb::from_name(b"Paste"), Some(ShellVerb::Paste));
        assert_eq!(ShellVerb::from_name(b"properties"), None);
    }
}
