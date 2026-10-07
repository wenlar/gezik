//! "Open terminal" in the app (spec 3.1): where it opens, and the status bar when it does not.
//! Choosing and starting it (gezik_platform::terminal) happens on a thread: it looks through PATH.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use gezik_platform::terminal::TerminalError;

use crate::view::View;

pub const ONLY_ON_WINDOWS: &str = "Only on Windows";
pub const NOT_FOUND: &str = "No terminal found. Set [terminal] command in settings.toml.";

thread_local! {
    /// `[terminal] command` of the settings in effect.
    static COMMAND: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
}

/// settings.toml changed (every resolve).
pub fn set_settings(command: Option<Vec<String>>) {
    COMMAND.with(|c| *c.borrow_mut() = command);
}

/// Where the terminal opens for the file list: the focused row's folder (its own if it is
/// one; a drive in This PC is one), else the folder shown; in This PC with no drive focused,
/// nowhere.
pub fn folder_for(focused: Option<(PathBuf, bool)>, shown: Option<PathBuf>) -> Option<PathBuf> {
    match focused {
        Some((path, true)) => Some(path),
        Some((path, false)) => path.parent().map(Path::to_path_buf).or(shown),
        None => shown,
    }
}

/// What the status bar says when the terminal did not open; nothing for a No to UAC.
pub fn error_text(err: &TerminalError) -> Option<String> {
    match err {
        TerminalError::NotFound => Some(NOT_FOUND.to_owned()),
        TerminalError::OnlyOnWindows => Some(ONLY_ON_WINDOWS.to_owned()),
        TerminalError::Cancelled => None,
        TerminalError::UnsafeFolder(_) => Some(err.to_string()),
        TerminalError::Failed(why) => Some(format!("Cannot open the terminal: {why}")),
    }
}

fn note(text: String) {
    crate::view::with_current(|view| view.note(text));
}

/// Opens a terminal in `dir` (`admin`: as administrator, Windows only), on a thread.
pub fn open_in(dir: PathBuf, admin: bool) {
    if admin && !cfg!(windows) {
        return note(ONLY_ON_WINDOWS.to_owned());
    }
    let command = COMMAND.with(|c| c.borrow().clone());
    let started = std::thread::Builder::new().name("gezik-open-terminal".into()).spawn(move || {
        let result = gezik_platform::terminal::open(&dir, admin, command.as_deref());
        if let Some(text) = result.err().as_ref().and_then(error_text) {
            let _ = slint::invoke_from_event_loop(move || note(text));
        }
    });
    if let Err(err) = started {
        note(format!("Cannot open the terminal: {err}"));
    }
}

/// `open-terminal` and `open-terminal-admin` on the file list.
pub fn open_for_view(view: &View, admin: bool) {
    let focused = view.focus().and_then(|i| view.entry_path(i));
    if let Some(dir) = folder_for(focused, view.folder()) {
        open_in(dir, admin);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_terminal_opens_in_the_focused_folder_or_the_one_shown() {
        let shown = Some(PathBuf::from("/a"));
        assert_eq!(folder_for(Some((PathBuf::from("/a/sub"), true)), shown.clone()), Some(PathBuf::from("/a/sub")));
        assert_eq!(folder_for(Some((PathBuf::from("/a/f.txt"), false)), shown.clone()), Some(PathBuf::from("/a")));
        assert_eq!(folder_for(None, shown.clone()), shown);
        assert_eq!(folder_for(None, None), None, "This PC with no drive focused");
    }

    #[test]
    fn what_the_status_bar_says() {
        assert_eq!(error_text(&TerminalError::NotFound).as_deref(), Some(NOT_FOUND));
        assert_eq!(error_text(&TerminalError::OnlyOnWindows).as_deref(), Some("Only on Windows"));
        assert_eq!(error_text(&TerminalError::Cancelled), None, "a No to UAC is no error");
        assert_eq!(error_text(&TerminalError::Failed("x".into())).as_deref(), Some("Cannot open the terminal: x"));
        assert_eq!(
            error_text(&TerminalError::UnsafeFolder("cmd".into())).as_deref(),
            Some("This folder's name can't be passed safely to cmd")
        );
    }
}
