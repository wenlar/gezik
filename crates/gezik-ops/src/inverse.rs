//! Undo, built from what a job did: what it made goes to the trash, what it moved goes back,
//! what it trashed comes back; items moved into a folder the job made come out before it goes,
//! and come back after it (spec 8.2). Nothing here knows the task kinds.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gezik_core::ops::conflict::Facts;
use gezik_core::ops::paths::{cover, is_within, same_path};

use crate::task::{Outcome, Task};
use crate::tasks::{MoveTask, RenameTask, RestoreTask, TrashTask};

/// Files are checked before undo touches them; folders are not (they change as files land).
fn expect(facts: &Facts) -> Option<Facts> {
    (!facts.is_dir).then_some(*facts)
}

/// `outcomes` with every `Several` opened up, in order.
pub(crate) fn flatten<'a>(outcomes: &'a [Outcome], into: &mut Vec<&'a Outcome>) {
    for outcome in outcomes {
        match outcome {
            Outcome::Several(inner) => flatten(inner, into),
            other => into.push(other),
        }
    }
}

/// Whether `path` is inside `dir` (not `dir` itself).
fn inside(path: &Path, dir: &Path) -> bool {
    is_within(path, dir) && !same_path(path, dir)
}

/// The tasks that undo `outcomes`, in order: trash what was made, move back what was moved,
/// restore what was trashed (an item replaced by a copy comes back after the copy left).
/// Items moved into a folder the job made come out before it goes (it goes only if empty);
/// their redo brings the folder back before they go in again. Folders made on the way (copy or
/// move with folders) go last, each only if it then holds no files: what went into them is
/// undone item by item first, each with its own check, and a file put there since stays.
pub(crate) fn build(outcomes: &[Outcome]) -> Vec<Arc<dyn Task>> {
    let mut made: Vec<(PathBuf, Option<Facts>)> = Vec::new();
    let mut parents: Vec<PathBuf> = Vec::new();
    let mut restored_dirs: Vec<PathBuf> = Vec::new();
    let mut moved: Vec<(PathBuf, PathBuf, Option<Facts>)> = Vec::new();
    let mut trashed: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut flat = Vec::new();
    flatten(outcomes, &mut flat);
    for outcome in flat {
        match outcome {
            Outcome::Created { path, facts, from: None } => made.push((path.clone(), expect(facts))),
            Outcome::Created { path, facts, from: Some(from) } => {
                moved.push((path.clone(), from.clone(), expect(facts)))
            }
            Outcome::Moved { from, to, facts } => moved.push((to.clone(), from.clone(), expect(facts))),
            Outcome::Trashed { original, trashed: at } => trashed.push((at.clone(), original.clone())),
            Outcome::Restored { original, facts } if facts.is_dir => restored_dirs.push(original.clone()),
            Outcome::Restored { original, facts } => made.push((original.clone(), expect(facts))),
            Outcome::MadeParent { path } => parents.push(path.clone()),
            Outcome::Deleted { .. } | Outcome::Nothing | Outcome::Several(_) => {}
        }
    }
    // A folder brought back together with what was inside it (the redo of undoing a copy with
    // folders) is such a folder on the way again; a folder brought back alone goes whole.
    for dir in &restored_dirs {
        let holds =
            made.iter().any(|(path, _)| inside(path, dir)) || restored_dirs.iter().any(|path| inside(path, dir));
        if holds {
            parents.push(dir.clone());
        } else {
            made.push((dir.clone(), None));
        }
    }
    // Items moved into a folder the job made (New folder with selection): they come out
    // before it goes to the trash, else they would go with it, and it goes only if empty.
    let holders: Vec<PathBuf> = made
        .iter()
        .filter(|(dir, _)| moved.iter().any(|(now, _, _)| inside(now, dir)))
        .map(|(dir, _)| dir.clone())
        .collect();
    let moves_first = !holders.is_empty();
    // The redo of that: the folder comes back from the trash before they go into it.
    let restore_first = moved.iter().any(|(_, was, _)| trashed.iter().any(|(_, original)| inside(was, original)));
    let made = cover(made, |(path, _)| path);
    let mut trash: Option<Arc<dyn Task>> =
        (!made.is_empty()).then(|| Arc::new(TrashTask::checked(made).only_if_empty(holders)) as Arc<dyn Task>);
    let mut restore: Option<Arc<dyn Task>> =
        (!trashed.is_empty()).then(|| Arc::new(RestoreTask::new(trashed)) as Arc<dyn Task>);
    let moved = cover(moved, |(path, _, _)| path);
    // Renames in one folder may swap names: they go back in an order that never collides.
    let (renamed, moved): (Vec<_>, Vec<_>) =
        moved.into_iter().partition(|(now, was, _)| now.parent().is_some() && now.parent() == was.parent());
    let mut tasks: Vec<Arc<dyn Task>> = Vec::new();
    if restore_first {
        tasks.extend(restore.take());
    }
    if !moves_first {
        tasks.extend(trash.take());
    }
    if !renamed.is_empty() {
        tasks.push(Arc::new(RenameTask::back(renamed)));
    }
    if !moved.is_empty() {
        tasks.push(Arc::new(MoveTask::back(moved)));
    }
    tasks.extend(trash);
    let parents = cover(parents, |path| path);
    if !parents.is_empty() {
        let items = parents.iter().map(|path| (path.clone(), None)).collect();
        tasks.push(Arc::new(TrashTask::checked(items).only_if_empty(parents)));
    }
    tasks.extend(restore);
    tasks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::TaskKind;

    fn file() -> Facts {
        Facts { is_dir: false, size: 1, modified: None }
    }

    #[test]
    fn made_moved_and_trashed_each_get_their_inverse_in_order() {
        let outcomes = vec![
            Outcome::Created { path: "/d/new".into(), facts: Facts { is_dir: true, ..Facts::default() }, from: None },
            Outcome::Created { path: "/d/new/inside.txt".into(), facts: file(), from: None },
            Outcome::Moved { from: "/a/x".into(), to: "/b/x".into(), facts: file() },
            Outcome::Trashed { original: "/d/old.txt".into(), trashed: "/bin/1".into() },
            Outcome::Deleted { path: "/gone".into() },
        ];
        let tasks = build(&outcomes);
        let kinds: Vec<TaskKind> = tasks.iter().map(|t| t.kind()).collect();
        assert_eq!(kinds, [TaskKind::Trash, TaskKind::Move, TaskKind::Restore]);
        assert_eq!(tasks[0].count(), 1, "the new folder covers what is inside it");
    }

    #[test]
    fn renames_in_one_folder_undo_as_renames() {
        let outcomes = vec![
            Outcome::Moved { from: "/d/a".into(), to: "/d/b".into(), facts: file() },
            Outcome::Moved { from: "/d/b".into(), to: "/d/a".into(), facts: file() },
        ];
        let tasks = build(&outcomes);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].kind(), TaskKind::Rename);
        assert_eq!(tasks[0].count(), 2);
    }

    #[test]
    fn several_outcomes_are_flattened() {
        let outcomes = vec![Outcome::Several(vec![
            Outcome::Trashed { original: "/d/a.zip".into(), trashed: "/bin/1".into() },
            Outcome::Created { path: "/d/a.zip".into(), facts: file(), from: None },
        ])];
        let kinds: Vec<TaskKind> = build(&outcomes).iter().map(|t| t.kind()).collect();
        assert_eq!(kinds, [TaskKind::Trash, TaskKind::Restore]);
    }

    fn folder() -> Facts {
        Facts { is_dir: true, ..Facts::default() }
    }

    #[test]
    fn items_moved_into_a_new_folder_come_out_before_it_goes() {
        let outcomes = vec![
            Outcome::Created { path: "/d/New folder".into(), facts: folder(), from: None },
            Outcome::Moved { from: "/d/a.txt".into(), to: "/d/New folder/a.txt".into(), facts: file() },
            Outcome::Moved { from: "/d/b".into(), to: "/d/New folder/b".into(), facts: folder() },
        ];
        let kinds: Vec<TaskKind> = build(&outcomes).iter().map(|t| t.kind()).collect();
        assert_eq!(kinds, [TaskKind::Move, TaskKind::Trash], "back out first, then the empty folder");
    }

    #[test]
    fn their_redo_brings_the_folder_back_before_they_go_in() {
        let outcomes = vec![
            Outcome::Moved { from: "/d/New folder/a.txt".into(), to: "/d/a.txt".into(), facts: file() },
            Outcome::Trashed { original: "/d/New folder".into(), trashed: "/bin/1".into() },
        ];
        let kinds: Vec<TaskKind> = build(&outcomes).iter().map(|t| t.kind()).collect();
        assert_eq!(kinds, [TaskKind::Restore, TaskKind::Move]);
    }

    #[test]
    fn a_move_next_to_a_new_item_keeps_the_old_order() {
        // The made item is the moved item's place itself, not a folder around it.
        let outcomes = vec![
            Outcome::Created { path: "/d/x".into(), facts: file(), from: None },
            Outcome::Moved { from: "/a/x".into(), to: "/d/x".into(), facts: file() },
            Outcome::Trashed { original: "/a/x".into(), trashed: "/bin/2".into() },
        ];
        let kinds: Vec<TaskKind> = build(&outcomes).iter().map(|t| t.kind()).collect();
        assert_eq!(kinds, [TaskKind::Trash, TaskKind::Move, TaskKind::Restore]);
    }

    #[test]
    fn folders_made_on_the_way_go_last_and_do_not_cover_what_went_in() {
        let outcomes = vec![
            Outcome::MadeParent { path: "/d/a".into() },
            Outcome::MadeParent { path: "/d/a/b".into() },
            Outcome::Created { path: "/d/a/b/x.txt".into(), facts: file(), from: None },
            Outcome::Moved { from: "/s/y.txt".into(), to: "/d/a/y.txt".into(), facts: file() },
        ];
        let tasks = build(&outcomes);
        let kinds: Vec<TaskKind> = tasks.iter().map(|t| t.kind()).collect();
        assert_eq!(kinds, [TaskKind::Trash, TaskKind::Move, TaskKind::Trash]);
        assert_eq!(tasks[0].count(), 1, "the copied file on its own, with its check");
        assert_eq!(tasks[2].count(), 1, "the outermost folder made on the way");
    }

    #[test]
    fn a_folder_restored_with_its_contents_is_a_folder_on_the_way_again() {
        let outcomes = vec![
            Outcome::Restored { original: "/d/a".into(), facts: folder() },
            Outcome::Restored { original: "/d/a/x.txt".into(), facts: file() },
            Outcome::Restored { original: "/e/alone".into(), facts: folder() },
        ];
        let tasks = build(&outcomes);
        let kinds: Vec<TaskKind> = tasks.iter().map(|t| t.kind()).collect();
        assert_eq!(kinds, [TaskKind::Trash, TaskKind::Trash]);
        assert_eq!(tasks[0].count(), 2, "the file with its check and the folder that came back alone");
        assert_eq!(tasks[1].count(), 1, "the holder, only if it holds no files");
    }

    #[test]
    fn deletes_alone_cannot_be_undone() {
        assert!(build(&[Outcome::Deleted { path: "/x".into() }, Outcome::Nothing]).is_empty());
    }
}
