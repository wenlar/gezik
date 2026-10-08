//! Running a job: wait for the drives, plan, settle conflicts, do the items.

use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};

use gezik_core::ops::conflict::{ConflictKind, Decision, Facts, Resolution, default_decision, kind_of, resolve};
use gezik_core::ops::names::next_free;
use gezik_core::ops::paths::{DriveSet, is_within, same_path};
use gezik_core::ops::threads::workers;
use gezik_platform::fs;

use crate::engine::{ConflictItem, Event, Job, PauseReason, Shared, lock};
use crate::task::{ChangedSince, NoTrash, Outcome, PlanItem, Restart, RunCx, ScanSink, Stage, Task, Work, is_marker};
use crate::walk::facts_of;

/// After this many failures in a row the job pauses and asks.
const MAX_FAILURES_IN_ROW: u32 = 20;

pub(crate) fn run_job(shared: &Shared, job: &Arc<Job>) {
    let mut ids = Vec::new();
    let mut kinds = Vec::new();
    for task in &job.tasks {
        for path in task.resources().paths {
            if let Some(facts) = shared.drive(&path) {
                ids.push(facts.id);
                kinds.push(facts.kind);
            }
        }
    }
    shared.publish_drives(job, DriveSet::new(ids));
    if shared.wait_turn(job) {
        for task in &job.tasks {
            if job.control.cancelled() {
                break;
            }
            run_task(shared, job, task.as_ref(), &kinds);
        }
    }
    shared.finish(job, job.control.cancelled());
}

fn run_task(shared: &Shared, job: &Job, task: &dyn Task, kinds: &[gezik_core::ops::threads::DiskKind]) {
    let control = &job.control;
    let count = task.workers().unwrap_or_else(|| match task.resources().work {
        Work::Disk => workers(shared.settings().threads, kinds),
        Work::Cpu => std::thread::available_parallelism().map_or(2, |n| n.get()),
        Work::External => 2,
    });
    let (sender, receiver) = mpsc::channel::<PlanItem>();
    let receiver = Mutex::new(receiver);
    let mut after: Vec<PlanItem> = Vec::new();
    std::thread::scope(|scope| {
        for _ in 0..count.max(1) {
            scope.spawn(|| {
                loop {
                    let next = lock(&receiver).recv();
                    let Ok(item) = next else { break };
                    execute(shared, job, task, item);
                }
            });
        }
        control.scanning.store(true, std::sync::atomic::Ordering::SeqCst);
        let mut sink = Sink {
            shared,
            job,
            task,
            sender: &sender,
            after: &mut after,
            held: Vec::new(),
            merges: Vec::new(),
            renames: Vec::new(),
            blocked: Vec::new(),
            taken: HashSet::new(),
        };
        task.plan(&mut sink);
        control.scanning.store(false, std::sync::atomic::Ordering::SeqCst);
        if !sink.held.is_empty() && !control.cancelled() {
            sink.settle();
        }
        drop(sink);
        // The workers stop once the queue is empty and closed.
        drop(sender);
    });
    for item in after.into_iter().rev() {
        if control.cancelled() {
            break;
        }
        execute(shared, job, task, item);
    }
}

/// An item waiting for the user's decision, with what is inside it (a held folder).
struct Held {
    item: PlanItem,
    conflict: ConflictItem,
    children: Vec<PlanItem>,
}

/// Takes a task's plan: runs `Before` items, queues `Parallel` ones, keeps `After` ones and
/// holds conflicts.
struct Sink<'a> {
    shared: &'a Shared,
    job: &'a Job,
    task: &'a dyn Task,
    sender: &'a mpsc::Sender<PlanItem>,
    after: &'a mut Vec<PlanItem>,
    held: Vec<Held>,
    /// Folders merged into existing ones, shown with the conflicts.
    merges: Vec<ConflictItem>,
    /// Targets given a new name so far: what goes inside them follows.
    renames: Vec<(PathBuf, PathBuf)>,
    /// Held folders (target, index in `held`): what goes inside them waits with them.
    blocked: Vec<(PathBuf, usize)>,
    /// Names this task chose that may not exist on disk yet.
    taken: HashSet<PathBuf>,
}

fn conflict(item: &PlanItem, target: &Path, kind: ConflictKind, existing: Facts, decision: Decision) -> ConflictItem {
    ConflictItem {
        source: item.source.clone().unwrap_or_default(),
        target: target.to_path_buf(),
        kind,
        source_facts: item.facts,
        target_facts: existing,
        decision,
    }
}

impl ScanSink for Sink<'_> {
    fn item(&mut self, mut item: PlanItem) -> bool {
        let control = &self.job.control;
        if control.cancelled() {
            return false;
        }
        if item.counted {
            control.add_total(1, if item.facts.is_dir { 0 } else { item.facts.size });
        }
        self.follow_renames(&mut item);
        if let Some(target) = &item.target
            && let Some(&(_, index)) = self.blocked.iter().find(|(dir, _)| is_within(target, dir))
        {
            self.held[index].children.push(item);
            return true;
        }
        if item.check_target
            && let Some(target) = item.target.clone()
            && let Ok(meta) = std::fs::symlink_metadata(&target)
        {
            let existing = facts_of(&meta);
            let kind = kind_of(item.facts, existing);
            if kind == ConflictKind::Folder && item.stage == Stage::Before && item.preset.is_none() && item.merges {
                // The folder is there already: merge, its contents meet one by one.
                self.merges.push(conflict(&item, &target, kind, existing, Decision::Merge));
                control.item_done();
                if item.is_root {
                    self.job.result(target);
                }
                return true;
            }
            // A folder that cannot merge here (a one-step rename onto a folder).
            let kind = if kind == ConflictKind::Folder { ConflictKind::Mismatch } else { kind };
            if let Some(decision) = item.preset {
                self.apply(item, kind, existing, decision);
                return true;
            }
            let decision = default_decision(kind, item.facts, existing);
            let index = self.held.len();
            if item.facts.is_dir {
                self.blocked.push((target.clone(), index));
            }
            self.held.push(Held {
                conflict: conflict(&item, &target, kind, existing, decision),
                item,
                children: Vec::new(),
            });
            return true;
        }
        self.dispatch(item);
        true
    }

    fn failed(&mut self, path: &Path, error: io::Error) {
        self.job.fail(path, &error);
    }
}

impl Sink<'_> {
    fn dispatch(&mut self, item: PlanItem) {
        match item.stage {
            Stage::Before => execute(self.shared, self.job, self.task, item),
            Stage::Parallel => {
                let _ = self.sender.send(item);
            }
            Stage::After => self.after.push(item),
        }
    }

    fn follow_renames(&self, item: &mut PlanItem) {
        let Some(target) = &item.target else { return };
        let moved = self.renames.iter().find_map(|(old, new)| {
            target
                .strip_prefix(old)
                .ok()
                .map(|rest| if rest.as_os_str().is_empty() { new.clone() } else { new.join(rest) })
        });
        if let Some(moved) = moved {
            item.target = Some(moved);
        }
    }

    /// A skipped item counts as done.
    fn skip(&self, item: &PlanItem) {
        let control = &self.job.control;
        control.item_done();
        if !item.facts.is_dir {
            control.add_bytes(item.facts.size);
        }
    }

    /// A free `name (n)` next to `target`.
    fn free_target(&mut self, target: &Path, is_dir: bool) -> PathBuf {
        let parent = target.parent().unwrap_or(Path::new(""));
        let name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let taken = &self.taken;
        let free = next_free(&name, is_dir, |candidate| {
            let path = parent.join(candidate);
            taken.contains(&path) || std::fs::symlink_metadata(&path).is_ok()
        });
        let path = parent.join(free);
        self.taken.insert(path.clone());
        path
    }

    /// Settles one conflict; returns whether the item was skipped.
    fn apply(&mut self, mut item: PlanItem, kind: ConflictKind, existing: Facts, decision: Decision) -> bool {
        let mut resolution = resolve(decision, kind, item.facts, existing);
        if resolution == Resolution::Replace && onto_itself(&item) {
            // Replacing an item with itself would trash the only one: it stays as it is.
            resolution = Resolution::Skip;
        }
        match resolution {
            Resolution::Write => self.dispatch(item),
            Resolution::Replace => {
                item.replace = true;
                self.dispatch(item);
            }
            Resolution::Skip => {
                self.skip(&item);
                return true;
            }
            Resolution::Rename => {
                let Some(target) = item.target.clone() else { return true };
                let free = self.free_target(&target, item.facts.is_dir);
                self.renames.push((target, free.clone()));
                item.target = Some(free);
                self.dispatch(item);
            }
        }
        false
    }

    /// Shows the held conflicts and applies the user's decisions.
    fn settle(&mut self) {
        let control = &self.job.control;
        let merges = std::mem::take(&mut self.merges);
        let shown = merges.len();
        let conflicts: Vec<ConflictItem> =
            merges.into_iter().chain(self.held.iter().map(|h| h.conflict.clone())).collect();
        control.deciding.store(true, std::sync::atomic::Ordering::SeqCst);
        self.shared.push([Event::Conflicts { job: self.job.id, conflicts }]);
        let decisions = control.wait_decisions();
        control.deciding.store(false, std::sync::atomic::Ordering::SeqCst);
        let Some(decisions) = decisions else { return };
        let held = std::mem::take(&mut self.held);
        self.blocked.clear();
        let mut chosen = decisions.into_iter().skip(shown);
        for Held { item, conflict, children } in held {
            let decision = chosen.next().unwrap_or(conflict.decision);
            let skipped = self.apply(item, conflict.kind, conflict.target_facts, decision);
            for mut child in children {
                if skipped {
                    self.skip(&child);
                } else {
                    self.follow_renames(&mut child);
                    self.dispatch(child);
                }
            }
        }
    }
}

/// Whether `item`'s target is its own source (a copy pasted back where it came from), however
/// the two are spelled: the same text, or the same entry on disk (`\\?\`, 8.3 names, a
/// junction or link on the way, `subst`, a mapped drive and its share).
fn onto_itself(item: &PlanItem) -> bool {
    let (Some(source), Some(target)) = (&item.source, &item.target) else { return false };
    same_path(source, target) || fs::same_entry(source, target) == Some(true)
}

/// Moves an existing target out of the way for "Replace": to the trash if its drive has one,
/// else aside under a noted name (returned), deleted only once the item is done: a failed or
/// cancelled item, or Gezik stopping meanwhile, puts it back.
fn replace_target(shared: &Shared, job: &Job, cx: &RunCx<'_>, target: &Path) -> io::Result<Option<PathBuf>> {
    if shared.has_trash(target) {
        job.outcome(match fs::trash(target)? {
            Some(trashed) => Outcome::Trashed { original: target.to_path_buf(), trashed },
            None => Outcome::Deleted { path: target.to_path_buf() },
        });
        return Ok(None);
    }
    let aside = cx.aside_name(target);
    cx.note_aside(&[(aside.clone(), target.to_path_buf())]);
    if let Err(err) = fs::move_entry(target, &aside) {
        cx.forget_aside(&[&aside]);
        return Err(err);
    }
    Ok(Some(aside))
}

/// The item replacing `target` ended: its old one, set aside, is deleted if the item `done`,
/// else put back. One that cannot be deleted is kept next to it under a free name.
fn settle_aside(job: &Job, cx: &RunCx<'_>, aside: &Path, target: &Path, done: bool) {
    if done {
        match fs::delete(aside) {
            Ok(()) => job.outcome(Outcome::Deleted { path: target.to_path_buf() }),
            Err(err) => {
                crate::tasks::restore_hidden(aside, target, true);
                let message = format!("{}; the old one was kept next to it", fs::describe(&err));
                job.fail(target, &io::Error::new(err.kind(), message));
            }
        }
    } else {
        crate::tasks::restore_hidden(aside, target, true);
    }
    // One that could not go back stays noted: the next start puts it back.
    if std::fs::symlink_metadata(aside).is_err() {
        cx.forget_aside(&[aside]);
    }
}

/// Does one item, on a worker or the planning thread.
pub(crate) fn execute(shared: &Shared, job: &Job, task: &dyn Task, item: PlanItem) {
    let control = &job.control;
    if control.stopped() {
        return;
    }
    if item.replace && onto_itself(&item) {
        // `apply` never asks for this; should anything else, the source is not touched.
        let err = io::Error::new(io::ErrorKind::InvalidInput, "Cannot replace an item with itself");
        job.fail(item.path(), &err);
        if item.counted {
            control.item_done();
        }
        return;
    }
    // A file the target drive cannot hold (FAT32 and 4 GB) would end as "disk full", which
    // freeing space does not help: it fails at once with the real reason, before a replaced
    // target is touched.
    if !item.facts.is_dir
        && let Some(target) = &item.target
        && let Some(max) = shared.drive(target).and_then(|facts| facts.max_file)
        && item.facts.size > max
    {
        let gb = (max + 1) >> 30;
        let err = io::Error::new(
            io::ErrorKind::FileTooLarge,
            format!("It is too big for this drive (files there can be at most {gb} GB)"),
        );
        job.fail(item.path(), &err);
        if item.counted {
            control.item_done();
            control.add_bytes(item.facts.size);
        }
        return;
    }
    let has_trash = |path: &Path| shared.has_trash(path);
    let cx =
        RunCx { control, trash: &has_trash, added: std::cell::Cell::new(0), temp: &job.temp, job: Some((shared, job)) };
    let mut aside = None;
    if item.replace
        && let Some(target) = &item.target
    {
        match replace_target(shared, job, &cx, target) {
            Ok(set_aside) => aside = set_aside,
            Err(err) => {
                job.fail(target, &err);
                control.item_done();
                return;
            }
        }
    }
    // A cancelled or failed item may have made or removed something (a partial copy): its
    // folders are reloaded like those of a finished one.
    let touch = || job.touch(item.source.as_deref().into_iter().chain(item.target.as_deref()), item.is_root);
    // Some(whether it was done) once the item ended; None once the job stopped.
    let ended = loop {
        match task.run(&item, &cx) {
            Ok(outcome) => {
                control.succeeded();
                if item.is_root
                    && let Some(path) = outcome.result()
                {
                    job.result(path.to_path_buf());
                }
                touch();
                job.outcome(outcome);
                break Some(true);
            }
            Err(err) if err.kind() == io::ErrorKind::Interrupted && control.cancelled() => {
                touch();
                break None;
            }
            Err(err) if is_marker::<Restart>(&err) => {
                // Paused while it ran a program, which was ended: once resumed, the item is
                // done again from the start, so what it counted is taken back now (the bar
                // shows where the job really is, not the try that was thrown away).
                control.take_back_bytes(cx.added.replace(0));
                if control.stopped() {
                    touch();
                    break None;
                }
            }
            Err(err) if fs::is_disk_full(&err) => {
                shared.pause(job, PauseReason::DiskFull, item.target.clone());
                if control.stopped() {
                    touch();
                    break None;
                }
                // Resumed: try the same item again.
            }
            Err(err) if is_marker::<ChangedSince>(&err) => {
                job.skipped_changed();
                break Some(false);
            }
            Err(err) if is_marker::<NoTrash>(&err) => {
                job.no_trash(item.path());
                break Some(false);
            }
            Err(err) => {
                touch();
                job.fail(item.path(), &err);
                if control.failed_once() >= MAX_FAILURES_IN_ROW {
                    control.succeeded();
                    shared.pause(job, PauseReason::ManyFailures, None);
                    let _ = control.stopped();
                }
                break Some(false);
            }
        }
    };
    if let (Some(aside), Some(target)) = (&aside, &item.target) {
        settle_aside(job, &cx, aside, target, ended == Some(true));
    }
    if ended.is_none() {
        return;
    }
    if !item.counted {
        return;
    }
    control.item_done();
    // Whatever the task did not count itself (a rename, a delete) is done now too.
    if !item.facts.is_dir {
        control.add_bytes(item.facts.size.saturating_sub(cx.added.get()));
    }
}
