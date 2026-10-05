//! Renaming in place, one item or many: in an order that never lands on a name another item
//! still holds (`gezik_core::ops::renames`), cycles through a temporary name each.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use gezik_core::ops::conflict::Facts;
use gezik_core::ops::paths::path_key;
use gezik_core::ops::renames::{Step, order};
use gezik_platform::fs;

use super::name;
use crate::engine::lock;
use crate::pending::{PendingDeletes, Restore, renaming_name};
use crate::task::{
    Outcome, PlanItem, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work, changed_since, facts_after, unchanged,
};
use crate::walk::facts_of;

const DIRECT: u8 = 0;
const TO_TEMP: u8 = 1;
const FROM_TEMP: u8 = 2;

pub struct RenameTask {
    /// (where it is, its new path) per item.
    pairs: Vec<(PathBuf, PathBuf)>,
    /// How each item must still look (undo); `None`: anything.
    expect: Vec<Option<Facts>>,
    /// The temporary name of each pair that needs one (set while planning).
    temps: Vec<PathBuf>,
    /// The order the pairs run in.
    steps: Vec<Step>,
    /// Where the temporary names were noted, once the first item went to one (`None` inside:
    /// nowhere to note).
    noted: OnceLock<Option<Arc<PendingDeletes>>>,
    /// The pairs that went to their temporary name.
    in_temp: Mutex<Vec<usize>>,
}

impl RenameTask {
    /// Renames `path` to `name` in its folder.
    pub fn one(path: PathBuf, name: &str) -> RenameTask {
        let target = path.with_file_name(name);
        RenameTask::many(vec![(path, target)])
    }

    /// Renames each (path, new path); new paths must be distinct.
    pub fn many(pairs: Vec<(PathBuf, PathBuf)>) -> RenameTask {
        let expect = vec![None; pairs.len()];
        RenameTask::new(pairs, expect)
    }

    /// Undo: each (where it is now, its old path, how it must look).
    pub(crate) fn back(items: Vec<(PathBuf, PathBuf, Option<Facts>)>) -> RenameTask {
        let (pairs, expect) = items.into_iter().map(|(now, was, facts)| ((now, was), facts)).unzip();
        RenameTask::new(pairs, expect)
    }

    fn new(pairs: Vec<(PathBuf, PathBuf)>, expect: Vec<Option<Facts>>) -> RenameTask {
        let temps = pairs.iter().map(|(source, _)| renaming_name(source)).collect();
        let steps = order(&pairs);
        RenameTask { pairs, expect, temps, steps, noted: OnceLock::new(), in_temp: Mutex::default() }
    }

    fn index(item: &PlanItem) -> usize {
        item.root
    }

    /// The pairs the order sends through a temporary name.
    fn through_temp(&self) -> impl Iterator<Item = usize> + '_ {
        self.steps.iter().filter_map(|step| if let Step::ToTemp(i) = step { Some(*i) } else { None })
    }

    /// Notes every temporary name the job will use, in one write before the first is taken:
    /// if Gezik stops meanwhile, the next start puts the items back.
    fn note_temps(&self, cx: &RunCx<'_>) {
        self.noted.get_or_init(|| {
            let pending = cx.pending()?;
            let restores: Vec<Restore> = self
                .through_temp()
                .map(|i| Restore { hidden: self.temps[i].clone(), original: self.pairs[i].0.clone(), was_hidden: true })
                .collect();
            pending.add_restores(&restores);
            Some(pending.clone())
        });
    }

    fn went_to_temp(&self, i: usize) -> bool {
        lock(&self.in_temp).contains(&i)
    }
}

impl Task for RenameTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Rename
    }

    fn title(&self) -> String {
        match self.pairs.as_slice() {
            [(from, to)] => format!("Renaming {} to {}", name(from), name(to)),
            pairs => {
                let dir = pairs
                    .first()
                    .and_then(|(from, _)| from.parent())
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                format!("Renaming {} items in {dir}", pairs.len())
            }
        }
    }

    fn count(&self) -> usize {
        self.pairs.len()
    }

    fn resources(&self) -> Resources {
        let paths = self.pairs.iter().filter_map(|(source, _)| source.parent().map(Path::to_path_buf)).collect();
        Resources { paths, work: Work::Disk }
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        // Names this job frees before it needs them: no conflict for those.
        let freed: std::collections::HashSet<Vec<String>> =
            self.pairs.iter().map(|(source, _)| path_key(source)).collect();
        // Before: run one by one, in this order, on the planning thread.
        for &step in &self.steps {
            let (i, tag) = match step {
                Step::Direct(i) => (i, DIRECT),
                Step::ToTemp(i) => (i, TO_TEMP),
                Step::FromTemp(i) => (i, FROM_TEMP),
            };
            let (source, target) = &self.pairs[i];
            if super::refuse_root(sink, source, "rename") {
                continue;
            }
            // The second step finds the item under its temporary name.
            let at = if tag == FROM_TEMP { &self.temps[i] } else { source };
            let facts = match std::fs::symlink_metadata(at) {
                Ok(meta) => facts_of(&meta),
                Err(_) if tag == FROM_TEMP && self.went_to_temp(i) => {
                    sink.failed(source, moved_back());
                    continue;
                }
                // The temporary name is gone already only if an earlier step failed: skip.
                Err(_) if tag == FROM_TEMP => continue,
                Err(err) => {
                    sink.failed(source, err);
                    continue;
                }
            };
            let mut item = PlanItem::new(Stage::Before, facts).source(source).target(target).top(i).tag(tag);
            if tag == TO_TEMP {
                item = item.uncounted();
            }
            // A target held by something outside this job is a real conflict.
            if tag != TO_TEMP && !freed.contains(&path_key(target)) {
                // The engine takes a Before folder onto an existing folder as a merge (and would
                // do nothing): a rename never merges, it says why instead.
                if (facts.is_dir || tag == FROM_TEMP)
                    && std::fs::symlink_metadata(target).is_ok_and(|meta| meta.is_dir())
                {
                    sink.failed(
                        source,
                        io::Error::new(io::ErrorKind::AlreadyExists, "A folder with this name already exists"),
                    );
                    continue;
                }
                item = item.checked();
            }
            if !sink.item(item) {
                return;
            }
        }
    }

    fn run(&self, item: &PlanItem, cx: &RunCx<'_>) -> io::Result<Outcome> {
        let i = Self::index(item);
        let (Some(source), Some(target)) = (&item.source, &item.target) else { return Ok(Outcome::Nothing) };
        let temp = &self.temps[i];
        match item.tag {
            TO_TEMP => {
                if !unchanged(source, self.expect.get(i).copied().flatten()) {
                    return Err(changed_since());
                }
                self.note_temps(cx);
                fs::move_entry(source, temp)?;
                lock(&self.in_temp).push(i);
                // Nothing to undo yet: the item counts once it reaches its name.
                Ok(Outcome::Nothing)
            }
            FROM_TEMP => {
                if std::fs::symlink_metadata(temp).is_err() {
                    return Err(moved_back());
                }
                match fs::move_entry(temp, target) {
                    Ok(()) => Ok(Outcome::Moved {
                        from: source.clone(),
                        to: target.clone(),
                        facts: facts_after(target, item.facts.is_dir),
                    }),
                    Err(err) => {
                        // Back under its own name (or a free one next to it); else the note puts
                        // it back later.
                        super::restore_hidden(temp, source, true);
                        Err(err)
                    }
                }
            }
            _ => {
                if !unchanged(source, self.expect.get(i).copied().flatten()) {
                    return Err(changed_since());
                }
                fs::move_entry(source, target)?;
                Ok(Outcome::Moved {
                    from: source.clone(),
                    to: target.clone(),
                    facts: facts_after(target, item.facts.is_dir),
                })
            }
        }
    }

    /// A job cancelled between an item's two steps leaves it under its temporary name: it goes
    /// back under its own name (or a free one next to it). The notes of the temporary names
    /// that are gone go in one write; one that stays is put back at the next start.
    fn done(&self, _cancelled: bool) {
        for &i in lock(&self.in_temp).iter() {
            let (temp, (source, _)) = (&self.temps[i], &self.pairs[i]);
            if std::fs::symlink_metadata(temp).is_ok() {
                super::restore_hidden(temp, source, true);
            }
        }
        if let Some(Some(pending)) = self.noted.get() {
            let gone: Vec<&Path> = self
                .through_temp()
                .map(|i| self.temps[i].as_path())
                .filter(|temp| std::fs::symlink_metadata(temp).is_err())
                .collect();
            pending.remove_restores(&gone);
        }
    }
}

/// The item left its temporary name before its second step: another Gezik window starting
/// meanwhile put it back.
fn moved_back() -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, "It was moved back by another Gezik window")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ConflictItem;
    use crate::testing::{defaults, engine, finish, read, test_dir, write};
    use gezik_core::ops::conflict::Decision;

    fn no_conflicts(_: &[ConflictItem]) -> Vec<Decision> {
        panic!("no conflicts expected")
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> =
            std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names
    }

    #[test]
    fn many_names_change_and_one_undo_brings_them_back() {
        let dir = test_dir("rename-many");
        for n in 1..=3 {
            write(&dir.join(format!("IMG_{n}.jpg")), &n.to_string());
        }
        let pairs = (1..=3).map(|n| (dir.join(format!("IMG_{n}.jpg")), dir.join(format!("Tatil {n}.jpg")))).collect();
        let engine = engine();
        let (report, _) = finish(&engine, engine.submit(Box::new(RenameTask::many(pairs))), no_conflicts);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(names(&dir), ["Tatil 1.jpg", "Tatil 2.jpg", "Tatil 3.jpg"]);
        assert_eq!(engine.undo_label().as_deref(), Some("Rename 3 items"));
        let (report, _) = finish(&engine, engine.undo().unwrap(), no_conflicts);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(names(&dir), ["IMG_1.jpg", "IMG_2.jpg", "IMG_3.jpg"]);
        assert_eq!(read(&dir.join("IMG_2.jpg")), "2");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn undo_of_a_swap_swaps_back() {
        let dir = test_dir("rename-swap");
        write(&dir.join("a.txt"), "a");
        write(&dir.join("b.txt"), "b");
        let engine = engine();
        let swap = vec![(dir.join("a.txt"), dir.join("b.txt")), (dir.join("b.txt"), dir.join("a.txt"))];
        let (report, _) = finish(&engine, engine.submit(Box::new(RenameTask::many(swap))), no_conflicts);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!((read(&dir.join("a.txt")), read(&dir.join("b.txt"))), ("b".into(), "a".into()));
        let (report, _) = finish(&engine, engine.undo().unwrap(), no_conflicts);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!((read(&dir.join("a.txt")), read(&dir.join("b.txt"))), ("a".into(), "b".into()));
        assert_eq!(names(&dir), ["a.txt", "b.txt"], "no temporary name is left");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn one_rename_also_only_its_case() {
        let dir = test_dir("rename-one");
        write(&dir.join("a.txt"), "a");
        let engine = engine();
        finish(&engine, engine.submit(Box::new(RenameTask::one(dir.join("a.txt"), "b.txt"))), no_conflicts);
        let (report, _) =
            finish(&engine, engine.submit(Box::new(RenameTask::one(dir.join("b.txt"), "B.txt"))), no_conflicts);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(names(&dir), ["B.txt"]);
        assert_eq!(engine.undo_label().as_deref(), Some("Rename"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_name_held_outside_the_job_asks() {
        let dir = test_dir("rename-taken");
        write(&dir.join("a.txt"), "a");
        write(&dir.join("b.txt"), "b");
        let engine = engine();
        let (_, events) =
            finish(&engine, engine.submit(Box::new(RenameTask::one(dir.join("a.txt"), "b.txt"))), defaults);
        assert!(events.iter().any(|e| matches!(e, crate::Event::Conflicts { .. })));
        assert_eq!((read(&dir.join("a.txt")), read(&dir.join("b.txt"))), ("a".into(), "b".into()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_folder_never_merges_into_a_folder_of_its_new_name() {
        let dir = test_dir("rename-folder-taken");
        write(&dir.join("a/x.txt"), "x");
        write(&dir.join("b/y.txt"), "y");
        let engine = engine();
        let (report, _) = finish(&engine, engine.submit(Box::new(RenameTask::one(dir.join("a"), "b"))), no_conflicts);
        assert_eq!(report.failures.len(), 1);
        assert_eq!(read(&dir.join("a/x.txt")), "x");
        assert!(!dir.join("b/x.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_job_cancelled_between_the_steps_puts_the_names_back() {
        let dir = test_dir("rename-cancel");
        write(&dir.join("a.txt"), "a");
        write(&dir.join("b.txt"), "b");
        let temp = crate::task::TempCopies::new(None);
        let control = crate::control::Control::default();
        let no_bin = |_: &Path| false;
        let cx = RunCx { control: &control, trash: &no_bin, added: std::cell::Cell::new(0), temp: &temp, job: None };
        let task =
            RenameTask::many(vec![(dir.join("a.txt"), dir.join("b.txt")), (dir.join("b.txt"), dir.join("a.txt"))]);
        let facts = facts_after(&dir.join("a.txt"), false);
        let to_temp =
            PlanItem::new(Stage::Before, facts).source(dir.join("a.txt")).target(dir.join("b.txt")).top(0).tag(TO_TEMP);
        task.run(&to_temp, &cx).unwrap();
        assert_eq!(names(&dir).len(), 2);
        assert!(!dir.join("a.txt").exists(), "under its temporary name");
        // Cancelled here: the second steps never run.
        task.done(true);
        assert_eq!(names(&dir), ["a.txt", "b.txt"], "no temporary name is left");
        assert_eq!((read(&dir.join("a.txt")), read(&dir.join("b.txt"))), ("a".into(), "b".into()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_note_is_there_while_an_item_sits_under_its_temporary_name() {
        let dir = test_dir("rename-note");
        write(&dir.join("a.txt"), "a");
        write(&dir.join("b.txt"), "b");
        let pending = Arc::new(PendingDeletes::new(dir.join("pending-deletes")));
        let temp = crate::task::TempCopies::new(Some(pending.clone()));
        let control = crate::control::Control::default();
        let no_bin = |_: &Path| false;
        let cx = RunCx { control: &control, trash: &no_bin, added: std::cell::Cell::new(0), temp: &temp, job: None };
        let task =
            RenameTask::many(vec![(dir.join("a.txt"), dir.join("b.txt")), (dir.join("b.txt"), dir.join("a.txt"))]);
        let i = task.through_temp().next().unwrap();
        let (source, target) = task.pairs[i].clone();
        let to_temp = PlanItem::new(Stage::Before, facts_after(&source, false))
            .source(&source)
            .target(&target)
            .top(i)
            .tag(TO_TEMP);
        task.run(&to_temp, &cx).unwrap();
        assert!(!source.exists(), "under its temporary name");
        assert_eq!(pending.restores().len(), 1, "the temporary name is noted");
        // The other item still holds the target: the second step fails and the item goes back.
        let from_temp = to_temp.clone().tag(FROM_TEMP);
        assert!(task.run(&from_temp, &cx).is_err());
        assert_eq!(read(&source), source.file_stem().unwrap().to_str().unwrap());
        task.done(false);
        assert!(pending.restores().is_empty(), "back under its name: the note is gone");
        assert_eq!(names(&dir), ["a.txt", "b.txt"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_item_put_back_by_another_window_fails_its_second_step() {
        let dir = test_dir("rename-moved-back");
        write(&dir.join("a.txt"), "a");
        write(&dir.join("b.txt"), "b");
        let temp = crate::task::TempCopies::new(None);
        let control = crate::control::Control::default();
        let no_bin = |_: &Path| false;
        let cx = RunCx { control: &control, trash: &no_bin, added: std::cell::Cell::new(0), temp: &temp, job: None };
        let task =
            RenameTask::many(vec![(dir.join("a.txt"), dir.join("b.txt")), (dir.join("b.txt"), dir.join("a.txt"))]);
        let i = task.through_temp().next().unwrap();
        let (source, target) = task.pairs[i].clone();
        let to_temp = PlanItem::new(Stage::Before, facts_after(&source, false))
            .source(&source)
            .target(&target)
            .top(i)
            .tag(TO_TEMP);
        task.run(&to_temp, &cx).unwrap();
        std::fs::rename(&task.temps[i], &source).unwrap();
        let err = task.run(&to_temp.clone().tag(FROM_TEMP), &cx).unwrap_err();
        assert!(err.to_string().contains("another Gezik window"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_cycle_and_case_only_renames_leave_no_notes() {
        let dir = test_dir("rename-cycle-notes");
        let work = dir.join("work");
        for name in ["a.txt", "b.txt", "c.txt", "d.txt", "e.txt"] {
            write(&work.join(name), name);
        }
        let pairs = vec![
            (work.join("a.txt"), work.join("b.txt")),
            (work.join("b.txt"), work.join("c.txt")),
            (work.join("c.txt"), work.join("a.txt")),
            (work.join("d.txt"), work.join("D.txt")),
            (work.join("e.txt"), work.join("E.TXT")),
        ];
        // A step to a temporary name is not an item of its own in the progress.
        let mut sink = crate::testing::CollectSink::default();
        RenameTask::many(pairs.clone()).plan(&mut sink);
        let to_temp = sink.items.iter().filter(|item| item.tag == TO_TEMP).count();
        assert!(to_temp >= 1);
        assert!(sink.items.iter().all(|item| item.counted == (item.tag != TO_TEMP)));
        let pending_file = dir.join("pending-deletes");
        let engine = crate::Engine::new(
            crate::Settings { pending_deletes: Some(pending_file.clone()), ..crate::Settings::default() },
            || {},
        );
        let (report, _) = finish(&engine, engine.submit(Box::new(RenameTask::many(pairs))), no_conflicts);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(names(&work), ["D.txt", "E.TXT", "a.txt", "b.txt", "c.txt"]);
        assert_eq!(read(&work.join("b.txt")), "a.txt");
        assert_eq!(read(&work.join("a.txt")), "c.txt");
        assert!(PendingDeletes::new(pending_file.clone()).restores().is_empty());
        assert!(!pending_file.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
