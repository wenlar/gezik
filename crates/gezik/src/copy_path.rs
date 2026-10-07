//! "Copy path as ▸" and `copy-path` (spec 4): paths as text on the system clipboard. Gezik's
//! own file clipboard (the cut items, paste) is left as it is.

use std::path::{Path, PathBuf};

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

/// Whether "UNC path" is offered: on Windows, when the first item is on a mapped drive.
pub fn unc_offered(first: Option<&Path>) -> bool {
    cfg!(windows)
        && first.is_some_and(|path| unc_path(&path.to_string_lossy(), &gezik_platform::fs::mapped_remote).is_some())
}

/// Copies `paths` written as `kind`; the status bar says how it went.
pub fn copy(view: &View, paths: &[PathBuf], kind: PathFormat) {
    if paths.is_empty() {
        return;
    }
    let texts: Vec<String> = paths.iter().map(|path| path.to_string_lossy().into_owned()).collect();
    let text = format_paths(&texts, kind, cfg!(windows), &gezik_platform::fs::mapped_remote);
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
}
