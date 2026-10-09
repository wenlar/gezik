//! Deleting for good. With a pending list, each chosen item is first renamed to a hidden name
//! (one quick rename), so it leaves the folder at once while its contents are deleted.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use gezik_core::ops::names::next_free_os;
use gezik_platform::fs;

use super::what;
use crate::engine::lock;
use crate::pending::{PendingDeletes, Restore, can_hold, hidden_name};
use crate::task::{Outcome, PlanItem, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work};
use crate::walk::{Step, facts_of, walk};

pub struct DeleteTask {
    roots: Vec<PathBuf>,
    pending: Option<Arc<PendingDeletes>>,
    /// The roots are hidden folders of an earlier delete (from `pending-deletes`).
    recovering: bool,
    /// For each root this delete hid: (hidden, original, whether it was hidden to begin with).
    hidden: Mutex<Vec<(PathBuf, PathBuf, bool)>>,
    /// For a delete from the trash: each root's record in its bin (`$I`, `.trashinfo`), deleted
    /// once the root is gone (index by index with `roots`; empty otherwise).
    infos: Vec<Option<PathBuf>>,
    /// For a delete from the trash: the roots that are not items in a bin. Any refuses the
    /// whole delete (nothing is deleted, no record removed).
    refused: Vec<PathBuf>,
}

impl DeleteTask {
    pub fn new(paths: Vec<PathBuf>, pending: Option<Arc<PendingDeletes>>) -> DeleteTask {
        DeleteTask {
            roots: paths,
            pending,
            recovering: false,
            hidden: Mutex::default(),
            infos: Vec::new(),
            refused: Vec::new(),
        }
    }

    /// Finishes deleting folders an earlier delete hid.
    pub(crate) fn recover(paths: Vec<PathBuf>, pending: Arc<PendingDeletes>) -> DeleteTask {
        DeleteTask {
            roots: paths,
            pending: Some(pending),
            recovering: true,
            hidden: Mutex::default(),
            infos: Vec::new(),
            refused: Vec::new(),
        }
    }

    /// Deletes items in the trash for good, each with its record (`items`: the entry in the
    /// bin, its record if the bin keeps one). No undo: nothing goes anywhere it could come back from.
    pub fn from_trash(items: Vec<(PathBuf, Option<PathBuf>)>, pending: Option<Arc<PendingDeletes>>) -> DeleteTask {
        let refused = items
            .iter()
            .filter(|(root, info)| !in_a_bin(root, info.as_deref()))
            .map(|(root, _)| root.clone())
            .collect();
        let (roots, infos) = items.into_iter().unzip();
        DeleteTask { roots, infos, refused, ..DeleteTask::new(Vec::new(), pending) }
    }

    /// Puts back what a cancelled or failed delete hid; the originals that could not go back
    /// now (the next start puts them back).
    fn put_back_hidden(&self) -> Vec<PathBuf> {
        let Some(pending) = &self.pending else { return Vec::new() };
        if self.recovering {
            let gone: Vec<&Path> = self
                .roots
                .iter()
                .map(PathBuf::as_path)
                .filter(|root| std::fs::symlink_metadata(root).is_err())
                .collect();
            pending.remove_all(&gone);
            return Vec::new();
        }
        let (mut gone, mut stuck) = (Vec::new(), Vec::new());
        for (hidden, original, was_hidden) in lock(&self.hidden).drain(..) {
            // Cancelled, or something inside could not be deleted: what is left goes back
            // under its own name, so nothing stays hidden and nothing is deleted later unasked.
            // If it cannot go back now, the next start puts it back.
            let back = (0..RESTORE_TRIES).any(|attempt| {
                if attempt > 0 {
                    std::thread::sleep(RESTORE_WAIT);
                }
                std::fs::symlink_metadata(&hidden).is_err() || restore_hidden(&hidden, &original, was_hidden)
            });
            if back {
                gone.push(hidden);
            } else {
                stuck.push(Restore { hidden, original, was_hidden });
            }
        }
        pending.remove_all(&gone.iter().map(PathBuf::as_path).collect::<Vec<_>>());
        pending.add_restores(&stuck);
        stuck.into_iter().map(|r| r.original).collect()
    }

    /// Renames each root to a hidden name next to it, all noted in one write; where each is
    /// now (the original where that fails).
    fn hide(&self, roots: &[&Path]) -> Vec<PathBuf> {
        let originals = || roots.iter().map(|root| root.to_path_buf()).collect();
        let Some(pending) = self.pending.as_ref().filter(|_| !self.recovering) else { return originals() };
        // A root in a folder a line cannot hold is deleted where it is.
        let hidden: Vec<Option<PathBuf>> = roots
            .iter()
            .map(|root| root.parent().map(|parent| parent.join(hidden_name())).filter(|hidden| can_hold(hidden)))
            .collect();
        // Noted first: a crash between the note and the rename leaves a path that does not
        // exist, which recovery drops; the other order could leave a folder hidden forever.
        let noted: Vec<&Path> = hidden.iter().flatten().map(PathBuf::as_path).collect();
        if pending.add_all(&noted).is_err() {
            return originals();
        }
        let mut unused = Vec::new();
        let paths = roots
            .iter()
            .zip(&hidden)
            .map(|(root, hidden)| {
                let Some(hidden) = hidden else { return root.to_path_buf() };
                let was_hidden = fs::is_hidden_attr(root);
                if fs::move_entry(root, hidden).is_err() {
                    unused.push(hidden.as_path());
                    return root.to_path_buf();
                }
                let _ = fs::set_hidden(hidden);
                lock(&self.hidden).push((hidden.clone(), root.to_path_buf(), was_hidden));
                hidden.clone()
            })
            .collect();
        pending.remove_all(&unused);
        paths
    }
}

/// Whether every part of `path` is a plain name under its root (no `.` or `..`).
fn plain(path: &Path) -> bool {
    use std::path::Component;
    path.is_absolute()
        && path.components().all(|c| matches!(c, Component::Prefix(_) | Component::RootDir | Component::Normal(_)))
}

fn name_of(path: &Path) -> Option<&str> {
    path.file_name()?.to_str()
}

/// Whether `root` sits directly in a bin's payload folder and `info` (if any) is its record in
/// that bin: `X:\$Recycle.Bin\<SID>\$R…` with `$I…`.
#[cfg(windows)]
fn in_a_bin(root: &Path, info: Option<&Path>) -> bool {
    // `get`: a name may start with a letter of more than one byte.
    let after = |n: &str, prefix: &str| {
        n.get(..2).filter(|p| p.eq_ignore_ascii_case(prefix)).map(|_| n[2..].to_owned()).filter(|r| !r.is_empty())
    };
    let Some(rest) = name_of(root).and_then(|n| after(n, "$R")) else { return false };
    let Some(sid) = root.parent().filter(|sid| name_of(sid).is_some_and(|n| n.starts_with("S-"))) else { return false };
    let bin_on_a_volume = sid.parent().is_some_and(|bin| {
        name_of(bin).is_some_and(|n| n.eq_ignore_ascii_case("$Recycle.Bin"))
            && bin.parent().is_some_and(|v| v.parent().is_none())
    });
    let record = info.is_none_or(|info| {
        info.parent() == Some(sid) && name_of(info).and_then(|n| after(n, "$I")) == Some(rest.clone())
    });
    plain(root) && bin_on_a_volume && record
}

/// Whether `root` sits directly in a bin's payload folder and `info` (if any) is its record in
/// that bin: `…/Trash/files/x` (the home trash), `<top>/.Trash-<uid>/files/x` or
/// `<top>/.Trash/<uid>/files/x`, with `…/info/x.trashinfo`.
#[cfg(all(unix, not(target_os = "macos")))]
fn in_a_bin(root: &Path, info: Option<&Path>) -> bool {
    let Some(name) = name_of(root) else { return false };
    let Some(trash) = root.parent().filter(|files| name_of(files) == Some("files")).and_then(Path::parent) else {
        return false;
    };
    let is_trash = name_of(trash).is_some_and(|n| {
        n == "Trash"
            || n.strip_prefix(".Trash-").is_some_and(|uid| !uid.is_empty() && uid.bytes().all(|b| b.is_ascii_digit()))
            || (!n.is_empty()
                && n.bytes().all(|b| b.is_ascii_digit())
                && trash.parent().and_then(name_of) == Some(".Trash"))
    });
    let record = info.is_none_or(|info| info == trash.join("info").join(format!("{name}.trashinfo")));
    plain(root) && is_trash && record
}

/// Whether `root` sits directly in a bin: `~/.Trash/x` or `<volume>/.Trashes/<uid>/x`. The bins
/// keep no record per item here.
#[cfg(target_os = "macos")]
fn in_a_bin(root: &Path, info: Option<&Path>) -> bool {
    let Some(bin) = root.parent().filter(|_| name_of(root).is_some()) else { return false };
    let is_bin = name_of(bin) == Some(".Trash")
        || (name_of(bin).is_some_and(|uid| !uid.is_empty() && uid.bytes().all(|b| b.is_ascii_digit()))
            && bin.parent().and_then(name_of) == Some(".Trashes"));
    plain(root) && is_bin && info.is_none()
}

/// How often putting a hidden folder back is tried before it is left for the next start
/// (something that opened a file in it, like a virus scanner, may let go quickly).
const RESTORE_TRIES: u32 = 5;
const RESTORE_WAIT: std::time::Duration = std::time::Duration::from_millis(100);

/// Puts what is left of a hidden folder back: under its own name, or the next free one if
/// that was taken meanwhile; the hidden attribute goes unless the folder had it before.
/// Returns whether it is back.
pub(crate) fn restore_hidden(hidden: &Path, original: &Path, was_hidden: bool) -> bool {
    let mut back = hidden.to_path_buf();
    match fs::move_entry(hidden, original) {
        Ok(()) => back = original.to_path_buf(),
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
            if let (Some(parent), Some(name)) = (original.parent(), original.file_name()) {
                let is_dir = std::fs::symlink_metadata(hidden).is_ok_and(|m| m.is_dir());
                let free =
                    next_free_os(name, is_dir, |candidate| std::fs::symlink_metadata(parent.join(candidate)).is_ok());
                let target = parent.join(free);
                if fs::move_entry(hidden, &target).is_ok() {
                    back = target;
                }
            }
        }
        Err(_) => {}
    }
    if !was_hidden {
        let _ = fs::clear_hidden(&back);
    }
    back != hidden
}

const FILE: u8 = 0;
const DIR: u8 = 1;

impl Task for DeleteTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Delete
    }

    fn title(&self) -> String {
        if self.recovering {
            "Finishing an earlier delete".to_owned()
        } else {
            format!("Deleting {}", what(&self.roots))
        }
    }

    fn count(&self) -> usize {
        self.roots.len()
    }

    fn resources(&self) -> Resources {
        Resources { paths: self.roots.clone(), work: Work::Disk }
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        if !self.refused.is_empty() {
            for root in &self.refused {
                sink.failed(root, io::Error::new(io::ErrorKind::InvalidInput, "Not an item in the trash"));
            }
            return;
        }
        let roots: Vec<(usize, &Path)> = self
            .roots
            .iter()
            .enumerate()
            .filter(|(_, original)| !super::refuse_root(sink, original, "delete"))
            .map(|(root, original)| (root, original.as_path()))
            .collect();
        let paths = self.hide(&roots.iter().map(|&(_, original)| original).collect::<Vec<_>>());
        for ((root, original), path) in roots.into_iter().zip(paths) {
            let meta = match std::fs::symlink_metadata(&path) {
                Ok(meta) => meta,
                Err(err) => {
                    sink.failed(original, err);
                    continue;
                }
            };
            let facts = facts_of(&meta);
            if !facts.is_dir {
                if !sink.item(PlanItem::new(Stage::Parallel, facts).source(&path).top(root).tag(FILE)) {
                    return;
                }
                continue;
            }
            // Folders go last, the deepest first (After items run in reverse).
            if !sink.item(PlanItem::new(Stage::After, facts).source(&path).top(root).tag(DIR)) {
                return;
            }
            let walked = walk(&path, &mut |step| match step {
                Step::Entry { path, facts, .. } => {
                    let (stage, tag) = if facts.is_dir { (Stage::After, DIR) } else { (Stage::Parallel, FILE) };
                    sink.item(PlanItem::new(stage, facts).source(path).under(root).tag(tag))
                }
                Step::Failed { path, error } => {
                    sink.failed(path, error);
                    true
                }
            });
            if !walked {
                return;
            }
        }
    }

    fn run(&self, item: &PlanItem, _cx: &RunCx<'_>) -> io::Result<Outcome> {
        let Some(path) = &item.source else { return Ok(Outcome::Nothing) };
        fs::delete(path)?;
        Ok(Outcome::Deleted { path: path.clone() })
    }

    fn done(&self, _cancelled: bool) {
        let stuck = self.put_back_hidden();
        if !self.refused.is_empty() {
            return;
        }
        // A root that is gone takes its record along; one still there (cancelled, something
        // inside could not go) or waiting to be put back at the next start keeps it, so the
        // trash still lists it.
        for (root, info) in self.roots.iter().zip(&self.infos) {
            if let Some(info) = info
                && std::fs::symlink_metadata(root).is_err_and(|err| err.kind() == io::ErrorKind::NotFound)
                && !stuck.contains(root)
            {
                let _ = std::fs::remove_file(info);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{Engine, Settings};
    use crate::testing::{CollectSink, defaults, finish, test_dir, write};

    fn engine_with_pending(dir: &Path) -> Engine {
        Engine::new(Settings { pending_deletes: Some(dir.join("pending-deletes")), ..Settings::default() }, || {})
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> =
            std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names
    }

    #[test]
    fn delete_hides_at_once_and_removes_everything() {
        let dir = test_dir("delete");
        let work = dir.join("work");
        write(&work.join("victim/a/b.txt"), "b");
        write(&work.join("victim/c.txt"), "c");
        write(&work.join("file.txt"), "f");
        write(&work.join("keep.txt"), "k");
        let engine = engine_with_pending(&dir);
        let task = DeleteTask::new(vec![work.join("victim"), work.join("file.txt")], engine.pending_deletes());
        let job = engine.submit(Box::new(task));
        let (report, _) = finish(&engine, job, defaults);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(names(&work), ["keep.txt"], "nothing hidden is left behind");
        assert!(!dir.join("pending-deletes").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn without_a_pending_list_it_deletes_in_place() {
        let dir = test_dir("delete-in-place");
        write(&dir.join("victim/a.txt"), "a");
        let engine = crate::testing::engine();
        let job = engine.submit(Box::new(DeleteTask::new(vec![dir.join("victim")], None)));
        let (report, _) = finish(&engine, job, defaults);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert!(names(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_cancelled_delete_puts_the_folder_back() {
        let dir = test_dir("delete-cancel");
        write(&dir.join("victim/a.txt"), "a");
        let pending = Arc::new(PendingDeletes::new(dir.join("pending-deletes")));
        let task = DeleteTask::new(vec![dir.join("victim")], Some(pending.clone()));
        let mut sink = CollectSink::default();
        task.plan(&mut sink);
        assert!(!dir.join("victim").exists(), "hidden at once");
        assert_eq!(pending.load().len(), 1);
        task.done(true);
        assert_eq!(std::fs::read_to_string(dir.join("victim/a.txt")).unwrap(), "a");
        assert!(pending.load().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_cancelled_delete_leaves_the_folder_visible() {
        let dir = test_dir("delete-cancel-visible");
        write(&dir.join("victim/a.txt"), "a");
        let pending = Arc::new(PendingDeletes::new(dir.join("pending-deletes")));
        let task = DeleteTask::new(vec![dir.join("victim")], Some(pending));
        task.plan(&mut CollectSink::default());
        assert!(!dir.join("victim").exists());
        task.done(true);
        assert!(dir.join("victim/a.txt").exists());
        assert!(!fs::is_hidden_attr(&dir.join("victim")), "not hidden after coming back");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_taken_original_name_gets_the_next_free_one() {
        let dir = test_dir("delete-cancel-taken");
        write(&dir.join("victim/a.txt"), "a");
        let pending = Arc::new(PendingDeletes::new(dir.join("pending-deletes")));
        let task = DeleteTask::new(vec![dir.join("victim")], Some(pending.clone()));
        task.plan(&mut CollectSink::default());
        write(&dir.join("victim/other.txt"), "o");
        task.done(true);
        assert_eq!(std::fs::read_to_string(dir.join("victim (2)/a.txt")).unwrap(), "a");
        assert!(dir.join("victim/other.txt").exists());
        assert!(!fs::is_hidden_attr(&dir.join("victim (2)")));
        assert!(pending.load().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Something opened a file in the hidden folder (a virus scanner): it cannot be renamed
    /// back now, so it is noted and put back at the next start, never deleted.
    #[cfg(windows)]
    #[test]
    fn a_folder_that_cannot_go_back_now_goes_back_at_the_next_start() {
        let dir = test_dir("delete-cancel-held");
        write(&dir.join("victim/a.txt"), "a");
        let pending = Arc::new(PendingDeletes::new(dir.join("pending-deletes")));
        let task = DeleteTask::new(vec![dir.join("victim")], Some(pending.clone()));
        task.plan(&mut CollectSink::default());
        let hidden = pending.load().pop().unwrap();
        let held = std::fs::File::open(hidden.join("a.txt")).unwrap();
        task.done(true);
        assert!(pending.load().is_empty(), "not deleted later");
        assert_eq!(pending.restores().len(), 1, "noted to go back");
        drop(held);

        let engine = engine_with_pending(&dir);
        assert_eq!(engine.recover_deletes(), None, "nothing to delete");
        assert_eq!(std::fs::read_to_string(dir.join("victim/a.txt")).unwrap(), "a");
        assert!(!fs::is_hidden_attr(&dir.join("victim")));
        assert!(!dir.join("pending-deletes").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recovery_finishes_an_interrupted_delete() {
        let dir = test_dir("delete-recover");
        let hidden = dir.join(format!("{}left", crate::pending::HIDDEN_PREFIX));
        write(&hidden.join("x/y.txt"), "y");
        std::fs::write(dir.join("pending-deletes"), format!("{}\n", hidden.display())).unwrap();
        let engine = engine_with_pending(&dir);
        let job = engine.recover_deletes().expect("one folder to finish");
        let (report, _) = finish(&engine, job, defaults);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert!(!hidden.exists());
        assert!(!dir.join("pending-deletes").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recovery_deletes_a_large_copy_left_unfinished() {
        let dir = test_dir("delete-recover-copy");
        let partial = dir.join(format!("{}left", crate::pending::COPYING_PREFIX));
        std::fs::write(&partial, "part").unwrap();
        write(&dir.join("keep.bin"), "k");
        std::fs::write(dir.join("pending-deletes"), format!("{}\n", partial.display())).unwrap();
        let engine = engine_with_pending(&dir);
        let job = engine.recover_deletes().expect("one leftover to delete");
        let (report, _) = finish(&engine, job, defaults);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert!(!partial.exists());
        assert!(dir.join("keep.bin").exists());
        assert!(!dir.join("pending-deletes").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn finished_pid() -> u32 {
        let mut child = if cfg!(windows) {
            std::process::Command::new("cmd").args(["/c", "exit"]).spawn().unwrap()
        } else {
            std::process::Command::new("true").spawn().unwrap()
        };
        child.wait().unwrap();
        child.id()
    }

    #[test]
    fn recovery_deletes_only_what_a_dead_copy_left_under_its_prefix() {
        let dir = test_dir("delete-recover-copies");
        let (dead, other) = (crate::pending::copy_prefix(), crate::pending::copy_prefix());
        write(&dir.join(format!("{dead}0")), "part");
        write(&dir.join(format!("{dead}1")), "part");
        write(&dir.join(format!("{dead}2/inside.txt")), "a folder with the name is not ours");
        write(&dir.join(format!("{other}0")), "another copy's");
        write(&dir.join("keep.bin"), "k");
        let pending = PendingDeletes::new(dir.join("pending-deletes"));
        pending.add_copies(finished_pid(), &dir, &dead);
        let engine = engine_with_pending(&dir);
        assert_eq!(engine.recover_deletes(), None, "cleaned at once, no job");
        let mut left = names(&dir);
        left.retain(|name| name != "pending-deletes");
        let mut expected = vec![format!("{dead}2"), format!("{other}0"), "keep.bin".to_owned()];
        expected.sort();
        assert_eq!(left, expected);
        assert!(!dir.join("pending-deletes").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recovery_leaves_the_files_of_a_copy_that_still_runs() {
        let dir = test_dir("delete-recover-running");
        let prefix = crate::pending::copy_prefix();
        write(&dir.join(format!("{prefix}0")), "being copied");
        let mut running = if cfg!(windows) {
            std::process::Command::new("cmd").args(["/c", "ping -n 30 127.0.0.1 >nul"]).spawn().unwrap()
        } else {
            std::process::Command::new("sleep").arg("30").spawn().unwrap()
        };
        let pending = PendingDeletes::new(dir.join("pending-deletes"));
        pending.add_copies(running.id(), &dir, &prefix);
        let engine = engine_with_pending(&dir);
        engine.recover_deletes();
        let _ = running.kill();
        let _ = running.wait();
        assert!(dir.join(format!("{prefix}0")).exists());
        assert_eq!(pending.copies().len(), 1, "kept for a later start");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Another Gezik window deletes it right now: a start must not delete it alongside.
    #[test]
    fn recovery_leaves_a_delete_that_still_runs() {
        let dir = test_dir("delete-recover-running-delete");
        let hidden = dir.join(crate::pending::hidden_name());
        write(&hidden.join("x.txt"), "x");
        let mut running = if cfg!(windows) {
            std::process::Command::new("cmd").args(["/c", "ping -n 30 127.0.0.1 >nul"]).spawn().unwrap()
        } else {
            std::process::Command::new("sleep").arg("30").spawn().unwrap()
        };
        std::fs::write(dir.join("pending-deletes"), format!("deleting\t{}\t{}\n", running.id(), hidden.display()))
            .unwrap();
        let engine = engine_with_pending(&dir);
        assert_eq!(engine.recover_deletes(), None);
        let _ = running.kill();
        let _ = running.wait();
        assert!(hidden.join("x.txt").exists());
        assert_eq!(
            PendingDeletes::new(dir.join("pending-deletes")).load(),
            std::slice::from_ref(&hidden),
            "still noted"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Another Gezik window renames through these right now: only a dead one's go back.
    #[test]
    fn recovery_puts_back_only_what_a_dead_rename_left() {
        let dir = test_dir("delete-recover-rename");
        let rn = crate::pending::RENAMING_PREFIX;
        let (live, dead) =
            (dir.join(format!("{rn}{}-0", std::process::id())), dir.join(format!("{rn}{}-0", finished_pid())));
        write(&live, "live");
        write(&dead, "dead");
        let pending = PendingDeletes::new(dir.join("pending-deletes"));
        pending.add_restores(&[
            Restore { hidden: live.clone(), original: dir.join("a.txt"), was_hidden: true },
            Restore { hidden: dead.clone(), original: dir.join("b.txt"), was_hidden: true },
        ]);
        let engine = engine_with_pending(&dir);
        assert_eq!(engine.recover_deletes(), None);
        assert_eq!(std::fs::read_to_string(&live).unwrap(), "live", "left to the running rename");
        assert_eq!(std::fs::read_to_string(dir.join("b.txt")).unwrap(), "dead");
        assert_eq!(pending.restores().len(), 1, "the running rename's note stays");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recovery_ignores_foreign_paths() {
        let dir = test_dir("delete-recover-foreign");
        write(&dir.join("Documents/important.txt"), "keep");
        let text = format!("{}\n{}\n", dir.join("Documents").display(), dir.join("gone").display());
        std::fs::write(dir.join("pending-deletes"), text).unwrap();
        let engine = engine_with_pending(&dir);
        assert_eq!(engine.recover_deletes(), None);
        assert_eq!(std::fs::read_to_string(dir.join("Documents/important.txt")).unwrap(), "keep");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Moves `path` (the test's own) to the real trash: its entry there and the bin's record.
    fn trashed(path: &Path) -> (PathBuf, Option<PathBuf>) {
        let entry = fs::trash(path).unwrap().expect("the temp folder has a trash");
        let name = entry.file_name().unwrap().to_str().unwrap().to_owned();
        let record = if cfg!(windows) {
            Some(entry.with_file_name(format!("$I{}", &name[2..])))
        } else if cfg!(target_os = "macos") {
            None
        } else {
            Some(entry.parent().unwrap().parent().unwrap().join("info").join(format!("{name}.trashinfo")))
        };
        (entry, record)
    }

    fn exists(path: &Option<PathBuf>) -> bool {
        path.as_ref().is_some_and(|p| p.exists())
    }

    fn delete_for_good(engine: &Engine, items: Vec<(PathBuf, Option<PathBuf>)>) -> crate::engine::Report {
        let task = DeleteTask::from_trash(items, engine.pending_deletes());
        finish(engine, engine.submit(Box::new(task)), defaults).0
    }

    #[test]
    fn deleting_from_the_trash_takes_the_record_along() {
        let dir = test_dir("delete-trash");
        write(&dir.join("x.txt"), "x");
        let (entry, record) = trashed(&dir.join("x.txt"));
        assert!(cfg!(target_os = "macos") || exists(&record), "the bin keeps a record");
        let report = delete_for_good(&crate::testing::engine(), vec![(entry.clone(), record.clone())]);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert!(!entry.exists() && !exists(&record), "no orphan record");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_deleted_folder_takes_its_record_along() {
        let dir = test_dir("delete-trash-folder");
        write(&dir.join("d/a/b.txt"), "b");
        let (entry, record) = trashed(&dir.join("d"));
        let report = delete_for_good(&engine_with_pending(&dir), vec![(entry.clone(), record.clone())]);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert!(!entry.exists() && !exists(&record));
        // Other tests share the bin: what this delete hid is in its own note.
        assert!(!dir.join("pending-deletes").exists(), "nothing hidden is left behind");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_root_still_there_keeps_its_record() {
        // Cancelled before it ran, or something inside could not go: the item stays in the
        // trash, and so does what makes the trash list it.
        let dir = test_dir("delete-trash-kept");
        write(&dir.join("kept.txt"), "x");
        write(&dir.join("gone.txt"), "x");
        let kept = trashed(&dir.join("kept.txt"));
        DeleteTask::from_trash(vec![kept.clone()], None).done(true);
        assert!(kept.0.exists() && (cfg!(target_os = "macos") || exists(&kept.1)));
        let gone = trashed(&dir.join("gone.txt"));
        std::fs::remove_file(&gone.0).unwrap();
        DeleteTask::from_trash(vec![gone.clone()], None).done(false);
        assert!(!exists(&gone.1), "its entry is gone: the record goes");
        assert!(delete_for_good(&crate::testing::engine(), vec![kept]).failures.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn what_is_not_in_a_bin_is_refused_and_left_alone() {
        let dir = test_dir("delete-trash-refused");
        let (file, record) = (dir.join("$R5.txt"), dir.join("$I5.txt"));
        write(&file, "x");
        write(&record, "not a record");
        write(&dir.join("in-bin.txt"), "x");
        let in_bin = trashed(&dir.join("in-bin.txt"));
        let engine = crate::testing::engine();
        let report = delete_for_good(&engine, vec![in_bin.clone(), (file.clone(), Some(record.clone()))]);
        assert_eq!(report.failures.len(), 1, "{:?}", report.failures);
        assert!(file.exists() && record.exists(), "a normal file is never deleted for good from here");
        assert!(in_bin.0.exists(), "one stranger refuses the whole delete");
        // An entry already gone does not take a file outside the bin along as its "record".
        DeleteTask::from_trash(vec![(dir.join("gone"), Some(record.clone()))], None).done(false);
        assert!(record.exists());
        assert!(delete_for_good(&engine, vec![in_bin]).failures.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn only_items_in_a_volume_bin_with_their_own_record_count() {
        let sid = r"C:\$Recycle.Bin\S-1-5-21-7";
        let ok = |root: &str, info: Option<&str>| in_a_bin(Path::new(root), info.map(Path::new));
        assert!(ok(&format!(r"{sid}\$RAB12.txt"), Some(&format!(r"{sid}\$IAB12.txt"))));
        assert!(ok(&format!(r"{sid}\$rab12.txt"), Some(&format!(r"{sid}\$iab12.txt"))));
        assert!(ok(&format!(r"{sid}\$RAB12"), None));
        assert!(!ok(&format!(r"{sid}\$RAB12.txt"), Some(&format!(r"{sid}\$IXX.txt"))), "another item's record");
        assert!(!ok(&format!(r"{sid}\$RAB12.txt"), Some(r"C:\work\$IAB12.txt")), "a record elsewhere");
        assert!(!ok(&format!(r"{sid}\$RAB12.txt"), Some(&format!(r"{sid}\desktop.ini"))));
        assert!(!ok(r"C:\work\$R1.txt", None), "not in a bin");
        assert!(!ok(r"C:\work\$Recycle.Bin\S-1\$R1.txt", None), "a bin not at a volume's top");
        assert!(!ok(&format!(r"{sid}\$RAB12\inner.txt"), None), "inside an item, not an item");
        assert!(!ok(&format!(r"{sid}\desktop.ini"), None));
        assert!(!ok(&format!(r"{sid}\$R"), None));
        assert!(!ok(&format!(r"{sid}\$ş"), None), "a name of wide letters does not panic");
        assert!(!ok(&format!(r"{sid}\..\..\Windows"), None));
        assert!(!ok(r"$Recycle.Bin\S-1\$R1", None), "relative");
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn only_items_in_a_bin_with_their_own_record_count() {
        let ok = |root: &str, info: Option<&str>| in_a_bin(Path::new(root), info.map(Path::new));
        let home = "/home/u/.local/share/Trash";
        assert!(ok(&format!("{home}/files/a"), Some(&format!("{home}/info/a.trashinfo"))));
        assert!(ok("/mnt/.Trash-1000/files/a", Some("/mnt/.Trash-1000/info/a.trashinfo")));
        assert!(ok("/mnt/.Trash/1000/files/a", None));
        assert!(!ok(&format!("{home}/files/a"), Some(&format!("{home}/info/b.trashinfo"))));
        assert!(!ok(&format!("{home}/files/a"), Some("/home/u/a.trashinfo")));
        assert!(!ok("/home/u/files/a", None));
        assert!(!ok("/mnt/.Trash-x/files/a", None));
        assert!(!ok(&format!("{home}/files/a/b"), None), "inside an item");
        assert!(!ok(&format!("{home}/files/../files/a"), None));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn only_items_in_a_bin_count() {
        let ok = |root: &str, info: Option<&str>| in_a_bin(Path::new(root), info.map(Path::new));
        assert!(ok("/Users/u/.Trash/a", None));
        assert!(ok("/Volumes/X/.Trashes/501/a", None));
        assert!(!ok("/Users/u/.Trash/a", Some("/Users/u/.Trash/.DS_Store")), "no record to take along here");
        assert!(!ok("/Users/u/a", None));
        assert!(!ok("/Users/u/.Trash/a/b", None), "inside an item");
    }

    /// Hidden for the delete, then held so it cannot go back now: it goes back at the next
    /// start, so its record must stay.
    #[cfg(windows)]
    #[test]
    fn a_folder_waiting_to_go_back_keeps_its_record() {
        let dir = test_dir("delete-trash-held");
        write(&dir.join("held/a.txt"), "a");
        let (entry, record) = trashed(&dir.join("held"));
        let pending = Arc::new(PendingDeletes::new(dir.join("pending-deletes")));
        let task = DeleteTask::from_trash(vec![(entry.clone(), record.clone())], Some(pending.clone()));
        task.plan(&mut CollectSink::default());
        let hidden = pending.load().pop().unwrap();
        let held = std::fs::File::open(hidden.join("a.txt")).unwrap();
        task.done(true);
        drop(held);
        assert!(!entry.exists() && exists(&record), "the record waits for its entry");
        assert_eq!(pending.restores().len(), 1);
        let engine = engine_with_pending(&dir);
        assert_eq!(engine.recover_deletes(), None);
        assert!(entry.exists(), "back at the next start");
        assert!(delete_for_good(&engine, vec![(entry, record)]).failures.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
