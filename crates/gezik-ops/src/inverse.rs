//! Undo, built from what a job did: what it made goes to the trash, what it moved goes back,
//! what it trashed comes back. Nothing here knows the task kinds.

use std::path::PathBuf;
use std::sync::Arc;

use gezik_core::ops::conflict::Facts;
use gezik_core::ops::paths::cover;

use crate::task::{Outcome, Task};
use crate::tasks::{MoveTask, RenameTask, RestoreTask, TrashTask};

/// Files are checked before undo touches them; folders are not (they change as files land).
fn expect(facts: &Facts) -> Option<Facts> {
    (!facts.is_dir).then_some(*facts)
}

/// `outcomes` with every `Several` opened up, in order.
fn flatten<'a>(outcomes: &'a [Outcome], into: &mut Vec<&'a Outcome>) {
    for outcome in outcomes {
        match outcome {
            Outcome::Several(inner) => flatten(inner, into),
            other => into.push(other),
        }
    }
}

/// The tasks that undo `outcomes`, in order: trash what was made, move back what was moved,
/// restore what was trashed (an item replaced by a copy comes back after the copy left).
pub(crate) fn build(outcomes: &[Outcome]) -> Vec<Arc<dyn Task>> {
    let mut made: Vec<(PathBuf, Option<Facts>)> = Vec::new();
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
            Outcome::Restored { original, facts } => made.push((original.clone(), expect(facts))),
            Outcome::Deleted { .. } | Outcome::Nothing | Outcome::Several(_) => {}
        }
    }
    let mut tasks: Vec<Arc<dyn Task>> = Vec::new();
    let made = cover(made, |(path, _)| path);
    if !made.is_empty() {
        tasks.push(Arc::new(TrashTask::checked(made)));
    }
    let moved = cover(moved, |(path, _, _)| path);
    // Renames in one folder may swap names: they go back in an order that never collides.
    let (renamed, moved): (Vec<_>, Vec<_>) =
        moved.into_iter().partition(|(now, was, _)| now.parent().is_some() && now.parent() == was.parent());
    if !renamed.is_empty() {
        tasks.push(Arc::new(RenameTask::back(renamed)));
    }
    if !moved.is_empty() {
        tasks.push(Arc::new(MoveTask::back(moved)));
    }
    if !trashed.is_empty() {
        tasks.push(Arc::new(RestoreTask::new(trashed)));
    }
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

    #[test]
    fn deletes_alone_cannot_be_undone() {
        assert!(build(&[Outcome::Deleted { path: "/x".into() }, Outcome::Nothing]).is_empty());
    }
}
