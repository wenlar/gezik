//! The file operations the engine runs.

mod copy;
mod delete;
mod move_;
mod new;
mod rename;
mod restore;
mod trash;

pub use copy::CopyTask;
pub use delete::DeleteTask;
pub(crate) use delete::restore_hidden;
pub use move_::MoveTask;
pub use new::NewTask;
pub use rename::RenameTask;
pub use restore::RestoreTask;
pub use trash::{TrashTask, trash_path};

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

/// A drive or volume root (`C:\`, `/`): nothing here may copy, move, trash or delete one.
pub(crate) fn is_root(path: &Path) -> bool {
    path.parent().is_none() || path.file_name().is_none()
}

/// Reports `path` as refused when it is a root (`verb`: "delete", "copy"...); true if refused.
pub(crate) fn refuse_root(sink: &mut dyn crate::task::ScanSink, path: &Path, verb: &str) -> bool {
    if !is_root(path) {
        return false;
    }
    sink.failed(path, std::io::Error::new(std::io::ErrorKind::InvalidInput, format!("Cannot {verb} a drive")));
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::Task;
    use crate::testing::CollectSink;

    fn roots() -> Vec<PathBuf> {
        let mut roots = vec![PathBuf::from("/")];
        roots.extend(fs::drive_root(&std::env::temp_dir()));
        roots
    }

    /// Plans `task` into a sink only (nothing runs): exactly one failure, nothing planned.
    fn refused(task: &dyn Task, root: &Path) {
        let mut sink = CollectSink::default();
        task.plan(&mut sink);
        assert!(sink.items.is_empty(), "{root:?} must not be planned");
        assert_eq!(sink.failed, [root.to_path_buf()]);
    }

    #[test]
    fn a_drive_root_is_never_planned() {
        let dir = std::env::temp_dir().join("gezik-root-refused");
        for root in roots() {
            assert!(is_root(&root));
            refused(&DeleteTask::new(vec![root.clone()], None), &root);
            refused(&TrashTask::new(vec![root.clone()]), &root);
            refused(&CopyTask::into(vec![root.clone()], &dir), &root);
            refused(&CopyTask::duplicate(vec![root.clone()]), &root);
            refused(&MoveTask::into(vec![root.clone()], &dir), &root);
            refused(&RenameTask::one(root.clone(), "x"), &root);
        }
        assert!(!is_root(&std::env::temp_dir().join("x")));
    }
}
