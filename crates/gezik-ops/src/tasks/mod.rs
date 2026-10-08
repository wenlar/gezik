//! The file operations the engine runs.

mod copy;
mod delete;
mod group;
mod link;
mod move_;
mod new;
mod rename;
mod restore;
mod trash;

pub use copy::CopyTask;
pub use delete::DeleteTask;
pub(crate) use delete::restore_hidden;
pub use group::GroupTask;
pub use link::LinkTask;
pub use move_::MoveTask;
pub use new::NewTask;
pub use rename::RenameTask;
pub use restore::RestoreTask;
pub use trash::{TrashTask, trash_path};

use std::io;
use std::path::{Path, PathBuf};

use gezik_core::ops::conflict::Facts;
use gezik_platform::fs;

use crate::task::{Outcome, PlanItem, ScanSink, Stage};

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

/// Makes the folder `path`; false if a folder is there already (merged into).
pub(crate) fn make_dir(path: &Path) -> io::Result<bool> {
    match std::fs::create_dir(path) {
        Ok(()) => Ok(true),
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists && path.is_dir() => Ok(false),
        Err(err) => Err(err),
    }
}

/// Plans the folders on the way that are not there yet (copy or move with folders), tagged
/// `tag`, first: Before items run as they are planned. False once the job is cancelled.
pub(crate) fn plan_parents(sink: &mut dyn ScanSink, parents: &[PathBuf], tag: u8) -> bool {
    parents.iter().filter(|parent| std::fs::symlink_metadata(parent).is_err()).all(|parent| {
        sink.item(
            PlanItem::new(Stage::Before, Facts { is_dir: true, ..Facts::default() })
                .target(parent)
                .uncounted()
                .tag(tag),
        )
    })
}

/// Does an item of `plan_parents`.
pub(crate) fn make_parent_dir(item: &PlanItem) -> io::Result<Outcome> {
    let Some(target) = &item.target else { return Ok(Outcome::Nothing) };
    Ok(if make_dir(target)? { Outcome::MadeParent { path: target.clone() } } else { Outcome::Nothing })
}

/// Where copy or move with folders puts its items (spec 4.6).
pub(crate) struct Relative {
    /// (source, `dir` joined with its relative path) per item.
    pub pairs: Vec<(PathBuf, PathBuf)>,
    /// The folders on the way that `dir` holds (`dir` too), shallowest first, each once.
    pub parents: Vec<PathBuf>,
    /// Sources whose relative path would leave `dir` (absolute, a drive, `..`, empty).
    pub refused: Vec<PathBuf>,
}

/// `dir` joined with each relative path (system separators: the Windows trash needs them for
/// undo). An item inside another chosen item goes with it and is dropped; a relative path that
/// is not plain names only (it could leave `dir`) is refused.
pub(crate) fn relative_targets(items: Vec<(PathBuf, PathBuf)>, dir: &Path) -> Relative {
    use std::path::Component;
    let items = gezik_core::ops::paths::cover(items, |(source, _)| source);
    let mut parents = std::collections::BTreeSet::new();
    let mut pairs = Vec::new();
    let mut refused = Vec::new();
    for (source, relative) in items {
        let plain = relative.components().next().is_some()
            && relative.components().all(|part| matches!(part, Component::Normal(_)));
        if !plain {
            refused.push(source);
            continue;
        }
        let target: PathBuf = dir.join(&relative).components().collect();
        let mut folder = target.parent();
        while let Some(f) = folder.filter(|f| gezik_core::ops::paths::is_within(f, dir)) {
            parents.insert(f.to_path_buf());
            folder = f.parent();
        }
        pairs.push((source, target));
    }
    let mut parents: Vec<PathBuf> = parents.into_iter().collect();
    parents.sort_by_key(|p| p.components().count());
    Relative { pairs, parents, refused }
}

/// Reports each of `refused` (see `Relative::refused`) as failed.
pub(crate) fn refuse_outside(sink: &mut dyn crate::task::ScanSink, refused: &[PathBuf]) {
    for source in refused {
        let message = "Its path under the search's folder leaves the target folder";
        sink.failed(source, std::io::Error::new(std::io::ErrorKind::InvalidInput, message));
    }
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

    #[test]
    fn relative_targets_keep_to_the_folder_and_drop_items_inside_others() {
        let dir = std::env::temp_dir().join("gezik-relative").join("dst");
        let src = std::env::temp_dir().join("gezik-relative").join("src");
        let items = vec![
            (src.join("a/b/x.txt"), PathBuf::from("a").join("b").join("x.txt")),
            (src.join("a"), PathBuf::from("a")),
            (src.join("up.txt"), PathBuf::from("..").join("up.txt")),
            (src.join("abs.txt"), src.join("abs.txt")),
            (src.join("dot.txt"), PathBuf::from(".").join("dot.txt")),
            (src.join("empty.txt"), PathBuf::new()),
            (src.join("c.txt"), PathBuf::from("c.txt")),
        ];
        let Relative { pairs, parents, mut refused } = relative_targets(items, &dir);
        assert_eq!(pairs, [(src.join("a"), dir.join("a")), (src.join("c.txt"), dir.join("c.txt"))]);
        assert_eq!(parents, std::slice::from_ref(&dir));
        refused.sort();
        let mut expected = vec![src.join("up.txt"), src.join("abs.txt"), src.join("dot.txt"), src.join("empty.txt")];
        expected.sort();
        assert_eq!(refused, expected);
    }
}
