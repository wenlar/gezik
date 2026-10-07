//! "Copy path as ▸" and `copy-path` (spec 4): paths as text on the system clipboard. Gezik's
//! own file clipboard (the cut items, paste) is left as it is.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use gezik_core::path_text::{PathFormat, format_paths, unc_path};
use gezik_platform::clipboard::{self, ClipboardError};

use crate::view::View;

/// The status line after `count` paths were copied.
pub fn copied_text(count: usize) -> String {
    if count == 1 {
        "Copied the path".to_owned()
    } else {
        format!("Copied {} paths", crate::preview::with_commas(count))
    }
}

/// The shares of mapped drives, by drive letter. Asking Windows for a share can stall for a
/// while on a disconnected drive, so the UI thread only reads what is here; a lookup runs on
/// a thread of its own and its answer shows from the next menu on.
#[derive(Default)]
struct Shares {
    known: HashMap<char, Option<String>>,
    pending: HashSet<char>,
}

impl Shares {
    /// What is known for `letter` (`None`: not looked up yet, or not mapped), and whether a
    /// lookup should start now, which is when none is running for it.
    fn read(&mut self, letter: char) -> (Option<String>, bool) {
        let letter = letter.to_ascii_uppercase();
        let start = self.pending.insert(letter);
        (self.known.get(&letter).cloned().flatten(), start)
    }

    fn store(&mut self, letter: char, share: Option<String>) {
        let letter = letter.to_ascii_uppercase();
        self.pending.remove(&letter);
        self.known.insert(letter, share);
    }
}

static SHARES: LazyLock<Mutex<Shares>> = LazyLock::new(Mutex::default);

fn shares() -> std::sync::MutexGuard<'static, Shares> {
    SHARES.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The share of mapped drive `letter` as last found, for the UI thread; a refresh starts in
/// the background. Off Windows there is none.
fn cached_share(letter: char) -> Option<String> {
    if !cfg!(windows) {
        return None;
    }
    let (share, start) = shares().read(letter);
    if start {
        let spawned = std::thread::Builder::new().name("gezik-drive-share".to_owned()).spawn(move || {
            let share = gezik_platform::fs::mapped_remote(letter);
            shares().store(letter, share);
        });
        if spawned.is_err() {
            shares().store(letter, None);
        }
    }
    share
}

/// Whether "UNC path" is offered: on Windows, when the first item is on a mapped drive. Only
/// once the drive's share is known: the first menu for a drive starts the lookup and does
/// not offer it yet.
pub fn unc_offered(first: Option<&Path>) -> bool {
    cfg!(windows) && first.is_some_and(|path| unc_path(&path.to_string_lossy(), &cached_share).is_some())
}

/// Copies `paths` written as `kind`; the status bar says how it went.
pub fn copy(view: &View, paths: &[PathBuf], kind: PathFormat) {
    if paths.is_empty() {
        return;
    }
    let texts: Vec<String> = paths.iter().map(|path| path.to_string_lossy().into_owned()).collect();
    let text = format_paths(&texts, kind, cfg!(windows), &cached_share);
    view.note(match clipboard::write_text(&text) {
        Ok(()) => copied_text(paths.len()),
        Err(ClipboardError::Unsupported) => "No system clipboard here".to_owned(),
        Err(ClipboardError::Failed(why)) => format!("Cannot use the clipboard: {why}"),
    });
}

/// `copy-path`: the selected items' full paths, else the folder shown's; in This PC the
/// selected drives (none selected: nothing).
pub fn copy_selection(view: &View) {
    let mut paths = view.selected_paths();
    if paths.is_empty() {
        paths.extend(view.folder());
    }
    copy(view, &paths, PathFormat::Full);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_status_line_counts_the_paths() {
        assert_eq!(copied_text(1), "Copied the path");
        assert_eq!(copied_text(3), "Copied 3 paths");
        assert_eq!(copied_text(1234), "Copied 1,234 paths");
    }

    #[test]
    fn unc_is_offered_only_for_a_mapped_drive() {
        assert!(!unc_offered(None));
        assert!(!unc_offered(Some(Path::new("/home/a"))));
        #[cfg(windows)]
        assert!(!unc_offered(Some(Path::new(r"C:\Windows"))), "a local drive");
    }

    #[test]
    fn a_share_is_read_from_what_a_lookup_stored() {
        let mut shares = Shares::default();
        // First ask: nothing known, a lookup starts; asking again while it runs starts none.
        assert_eq!(shares.read('z'), (None, true));
        assert_eq!(shares.read('Z'), (None, false));
        shares.store('z', Some(r"\sunucu\pay".to_owned()));
        // Known now, whatever the letter's case; the old answer shows while a refresh runs.
        assert_eq!(shares.read('Z'), (Some(r"\sunucu\pay".to_owned()), true));
        shares.store('Z', None);
        assert_eq!(shares.read('Z'), (None, true));
        // Another drive is its own.
        assert_eq!(shares.read('Y'), (None, true));
    }
}
