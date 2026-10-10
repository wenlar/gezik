//! Undo, built from what a job did: what it made goes to the trash, what it moved goes back,
//! what it trashed comes back; items moved into a folder the job made come out before it goes,
//! and come back after it (spec 8.2); attributes go back to what they were. Nothing here knows
//! the task kinds; what the administrator did is undone by the administrator.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gezik_core::attrs::Wanted;
use gezik_core::ops::conflict::Facts;
use gezik_core::ops::paths::{cover, path_key};

use crate::task::{Outcome, Task};
use crate::tasks::{ElevatedTask, MoveTask, RenameTask, RestoreTask, SetAttributesTask, TrashTask};

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

/// The keys of every folder one of `paths` is inside: `path` is inside `dir` (not `dir` itself)
/// for one of them if this holds `path_key(dir)`.
fn around<'a>(paths: impl IntoIterator<Item = &'a Path>) -> HashSet<Vec<String>> {
    let mut keys = HashSet::new();
    for path in paths {
        let key = path_key(path);
        for len in 1..key.len() {
            keys.insert(key[..len].to_vec());
        }
    }
    keys
}

/// The tasks that undo `outcomes`, in order: trash what was made, move back what was moved,
/// restore what was trashed (an item replaced by a copy comes back after the copy left).
/// Items moved into a folder the job made come out before it goes (it goes only if empty);
/// their redo brings the folder back before they go in again. A folder made empty that nothing
/// the job made went into (New folder) goes only if it still holds no files. Folders made on
/// the way (copy or move with folders) go last, each only if it then holds no files: what went
/// into them is undone item by item first, each with its own check, and a file put there since
/// stays.
pub(crate) fn build(outcomes: &[Outcome]) -> Vec<Arc<dyn Task>> {
    let mut made: Vec<(PathBuf, Option<Facts>)> = Vec::new();
    // The folders among them that were made empty (filled, if at all, by the job's own items).
    let mut made_dirs: HashSet<Vec<String>> = HashSet::new();
    let mut parents: Vec<PathBuf> = Vec::new();
    let mut restored_dirs: Vec<PathBuf> = Vec::new();
    let mut moved: Vec<(PathBuf, PathBuf, Option<Facts>)> = Vec::new();
    let mut trashed: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut attrs: Vec<Wanted> = Vec::new();
    let mut admin = Vec::new();
    let mut flat = Vec::new();
    flatten(outcomes, &mut flat);
    for outcome in flat {
        match outcome {
            Outcome::Created { path, facts, from: None } => {
                if facts.is_dir {
                    made_dirs.insert(path_key(path));
                }
                made.push((path.clone(), expect(facts)));
            }
            Outcome::Created { path, facts, from: Some(from) } => {
                moved.push((path.clone(), from.clone(), expect(facts)))
            }
            Outcome::Placed { path } => made.push((path.clone(), None)),
            Outcome::Moved { from, to, facts } => moved.push((to.clone(), from.clone(), expect(facts))),
            Outcome::Trashed { original, trashed: at } => trashed.push((at.clone(), original.clone())),
            Outcome::Restored { original, facts } if facts.is_dir => restored_dirs.push(original.clone()),
            Outcome::Restored { original, facts } => made.push((original.clone(), expect(facts))),
            Outcome::MadeParent { path } => parents.push(path.clone()),
            Outcome::AttributesChanged { path, id, before, after } => {
                attrs.push(Wanted { path: path.clone(), id: *id, from: *after, to: *before })
            }
            Outcome::AsAdmin { undo } => admin.push(undo.clone()),
            Outcome::Deleted { .. } | Outcome::Nothing | Outcome::Several(_) => {}
        }
    }
    // A folder brought back together with what was inside it (the redo of undoing a copy with
    // folders) is such a folder on the way again; a folder brought back alone goes whole.
    let holding = around(made.iter().map(|(path, _)| path.as_path()).chain(restored_dirs.iter().map(PathBuf::as_path)));
    for dir in restored_dirs {
        if holding.contains(&path_key(&dir)) {
            parents.push(dir);
        } else {
            made.push((dir, None));
        }
    }
    // Items moved into a folder the job made (New folder with selection): they come out
    // before it goes to the trash, else they would go with it, and it goes only if empty.
    let moved_into = around(moved.iter().map(|(now, _, _)| now.as_path()));
    let mut only_empty: Vec<PathBuf> =
        made.iter().filter(|(dir, _)| moved_into.contains(&path_key(dir))).map(|(dir, _)| dir.clone()).collect();
    let moves_first = !only_empty.is_empty();
    // The redo of that: the folder comes back from the trash before they go into it.
    let trashed_keys: HashSet<Vec<String>> = trashed.iter().map(|(_, original)| path_key(original)).collect();
    let restore_first = moved.iter().any(|(_, was, _)| {
        let key = path_key(was);
        (1..key.len()).any(|len| trashed_keys.contains(&key[..len]))
    });
    let made_into = around(made.iter().map(|(path, _)| path.as_path()));
    let made = cover(made, |(path, _)| path);
    only_empty.extend(
        made.iter()
            .map(|(dir, _)| dir)
            .filter(|dir| {
                let key = path_key(dir);
                made_dirs.contains(&key) && !made_into.contains(&key)
            })
            .cloned(),
    );
    let mut trash: Option<Arc<dyn Task>> =
        (!made.is_empty()).then(|| Arc::new(TrashTask::checked(made).only_if_empty(only_empty)) as Arc<dyn Task>);
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
    // A job of its own: no other outcome comes with it.
    if !attrs.is_empty() {
        tasks.push(Arc::new(SetAttributesTask::new(attrs)));
    }
    // The administrator's list goes back as one list, last first, behind one new prompt.
    if !admin.is_empty() {
        admin.reverse();
        tasks.push(Arc::new(ElevatedTask::new(admin)));
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
    fn administrator_outcomes_undo_as_one_list_in_reverse() {
        use gezik_core::elevated::Op;
        let outcomes = vec![Outcome::Several(vec![
            Outcome::AsAdmin { undo: Op::Delete("/d/b".into()) },
            Outcome::AsAdmin { undo: Op::Rmdir("/d/n".into()) },
        ])];
        let tasks = build(&outcomes);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].kind(), TaskKind::Elevated);
        assert_eq!(tasks[0].count(), 2);
        assert_eq!(tasks[0].title(), "2 changes as administrator");
    }

    #[test]
    fn deletes_alone_cannot_be_undone() {
        assert!(build(&[Outcome::Deleted { path: "/x".into() }, Outcome::Nothing]).is_empty());
    }
}
