//! Undo, built from what a job did: what it made goes to the trash, what it moved goes back,
//! what it trashed comes back. Nothing here knows the task kinds.

use std::path::PathBuf;

use gezik_core::ops::conflict::Facts;
use gezik_core::ops::paths::cover;

use crate::task::{Outcome, Task};
use crate::tasks::{MoveTask, RestoreTask, TrashTask};

/// Files are checked before undo touches them; folders are not (they change as files land).
fn expect(facts: &Facts) -> Option<Facts> {
    (!facts.is_dir).then_some(*facts)
}

/// The tasks that undo `outcomes`, in order: trash what was made, move back what was moved,
/// restore what was trashed (an item replaced by a copy comes back after the copy left).
pub(crate) fn build(outcomes: &[Outcome]) -> Vec<Box<dyn Task>> {
    let mut made: Vec<(PathBuf, Option<Facts>)> = Vec::new();
    let mut moved: Vec<(PathBuf, PathBuf, Option<Facts>)> = Vec::new();
    let mut trashed: Vec<(PathBuf, PathBuf)> = Vec::new();
    for outcome in outcomes {
        match outcome {
            Outcome::Created { path, facts, from: None } => made.push((path.clone(), expect(facts))),
            Outcome::Created { path, facts, from: Some(from) } => {
                moved.push((path.clone(), from.clone(), expect(facts)))
            }
            Outcome::Moved { from, to, facts } => moved.push((to.clone(), from.clone(), expect(facts))),
            Outcome::Trashed { original, trashed: at } => trashed.push((at.clone(), original.clone())),
            Outcome::Restored { original, facts } => made.push((original.clone(), expect(facts))),
            Outcome::Deleted { .. } | Outcome::Nothing => {}
        }
    }
    let mut tasks: Vec<Box<dyn Task>> = Vec::new();
    let made = cover(made, |(path, _)| path);
    if !made.is_empty() {
        tasks.push(Box::new(TrashTask::checked(made)));
    }
    let moved = cover(moved, |(path, _, _)| path);
    if !moved.is_empty() {
        tasks.push(Box::new(MoveTask::back(moved)));
    }
    if !trashed.is_empty() {
        tasks.push(Box::new(RestoreTask::new(trashed)));
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
    fn deletes_alone_cannot_be_undone() {
        assert!(build(&[Outcome::Deleted { path: "/x".into() }, Outcome::Nothing]).is_empty());
    }
}
