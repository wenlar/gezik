//! The file operations the engine runs.

mod copy;
mod delete;
mod move_;
mod new;
mod restore;
mod trash;

pub use copy::CopyTask;
pub use delete::DeleteTask;
pub use move_::MoveTask;
pub use new::NewTask;
pub use restore::RestoreTask;
pub use trash::TrashTask;

use std::path::{Path, PathBuf};

use gezik_platform::fs;

/// `rapor.pdf`, or `3 items`.
pub(crate) fn what(paths: &[PathBuf]) -> String {
    match paths {
        [one] => name(one),
        many => format!("{} items", many.len()),
    }
}

/// The last part of `path` (the whole path for a drive root).
pub(crate) fn name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string())
}

/// Whether `a` and `b` (or their nearest existing ancestors) are on the same drive.
pub(crate) fn same_drive(a: &Path, b: &Path) -> bool {
    let id = |path: &Path| fs::nearest_existing(path).and_then(|p| fs::drive_facts(&p).ok()).map(|facts| facts.id);
    matches!((id(a), id(b)), (Some(x), Some(y)) if x == y)
}
