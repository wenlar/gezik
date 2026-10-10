//! Items another program offers with no file behind them (spec 9 §8.1): an e-mail attachment,
//! a browser's picture, a zip view's files, a macOS file promise. Written into the drop folder,
//! each under a name made safe first; a copy as far as undo goes (Ctrl+Z trashes them).

use std::collections::{BTreeSet, HashSet};
use std::io;
use std::path::{Component, Path, PathBuf};

use gezik_core::drop_names::{safe_relative, shown};
use gezik_core::ops::conflict::Facts;
use gezik_core::ops::names::next_free;
use gezik_core::ops::paths::path_key;
use gezik_platform::dnd::VirtualFiles;

use crate::task::{Outcome, PlanItem, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work, facts_after};

/// A folder on the way that no item names (`Yeni\iç\b.txt` alone).
const MAKE_PARENT: u8 = 1;

/// The most parts (folders and the name) an item's path may have.
const MAX_PARTS: usize = 32;

pub struct MaterializeTask {
    files: VirtualFiles,
    dir: PathBuf,
    /// How many items the drop offered (for "Copy 3 items").
    count: usize,
}

impl MaterializeTask {
    pub fn new(files: VirtualFiles, dir: &Path, count: usize) -> MaterializeTask {
        MaterializeTask { files, dir: dir.to_path_buf(), count }
    }

    /// `name` as a path under the drop folder, or why it is refused.
    fn relative(name: &str) -> io::Result<PathBuf> {
        let relative =
            safe_relative(name, cfg!(windows)).map_err(|why| io::Error::new(io::ErrorKind::InvalidInput, why))?;
        if relative.components().count() > MAX_PARTS {
            let message = format!("Too many folders deep (at most {MAX_PARTS} parts)");
            return Err(io::Error::new(io::ErrorKind::InvalidInput, message));
        }
        Ok(relative)
    }

    /// Refuses `target` unless it is plain names under the drop folder with no link on the way
    /// (a folder already there that is a symbolic link or junction, or the target itself):
    /// what is written stays under the folder.
    fn check_target(&self, target: &Path) -> io::Result<()> {
        let inside = target.strip_prefix(&self.dir).is_ok_and(|rest| {
            rest.components().next().is_some() && rest.components().all(|part| matches!(part, Component::Normal(_)))
        });
        if !inside {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "It would leave the folder it was dropped in"));
        }
        let linked = target
            .ancestors()
            .take_while(|path| *path != self.dir)
            .any(|path| std::fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink()));
        if linked {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "A link is on the way; it is not followed"));
        }
        Ok(())
    }
}

impl Task for MaterializeTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Copy
    }

    fn title(&self) -> String {
        let what = if self.count == 1 { "1 item".to_owned() } else { format!("{} items", self.count) };
        format!("Copying {what} to {}", self.dir.display())
    }

    fn count(&self) -> usize {
        self.count
    }

    fn resources(&self) -> Resources {
        Resources { paths: vec![self.dir.clone()], work: Work::Disk }
    }

    /// One at a time: the source answers on one thread anyway, and keeps its order.
    fn workers(&self) -> Option<usize> {
        Some(1)
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        let entries = match self.files.0.entries(&|| sink.cancelled()) {
            Ok(entries) => entries,
            Err(_) if sink.cancelled() => return,
            Err(err) => return sink.failed(&self.dir, err),
        };
        let relatives: Vec<io::Result<PathBuf>> = entries
            .iter()
            .map(|entry| match &entry.failed {
                Some(why) => Err(io::Error::other(why.clone())),
                None => Self::relative(&entry.name),
            })
            .collect();
        // Folders on the way that no item names, made first (shallowest first).
        let named: HashSet<&Path> = relatives
            .iter()
            .zip(&entries)
            .filter_map(|(relative, entry)| relative.as_deref().ok().filter(|_| entry.is_dir))
            .collect();
        let mut parents = BTreeSet::new();
        for relative in relatives.iter().flatten() {
            for parent in relative.ancestors().skip(1).filter(|p| !p.as_os_str().is_empty()) {
                if !named.contains(parent) {
                    parents.insert(self.dir.join(parent));
                }
            }
        }
        let mut parents: Vec<PathBuf> = parents.into_iter().collect();
        parents.sort_by_key(|p| p.components().count());
        if !super::plan_parents(sink, &parents, MAKE_PARENT) {
            return;
        }
        let mut planned = HashSet::new();
        for (index, (entry, relative)) in entries.iter().zip(relatives).enumerate() {
            let mut relative = match relative {
                Ok(relative) => relative,
                Err(err) => {
                    sink.failed(&self.dir.join(shown(&entry.name)), err);
                    continue;
                }
            };
            // Two files that meet once made safe (`a<b`, `a>b`; case on Windows): the later
            // one gets a free number, as Keep both would.
            if !entry.is_dir && planned.contains(&path_key(&relative)) {
                let name = relative.file_name().unwrap_or_default().to_string_lossy().into_owned();
                let parent = relative.parent().unwrap_or(Path::new("")).to_path_buf();
                let free = next_free(&name, false, |candidate| {
                    let path = parent.join(candidate);
                    planned.contains(&path_key(&path)) || std::fs::symlink_metadata(self.dir.join(&path)).is_ok()
                });
                relative = parent.join(free);
            }
            planned.insert(path_key(&relative));
            let size = if entry.is_dir { 0 } else { entry.size.unwrap_or(0) };
            let facts = Facts { is_dir: entry.is_dir, size, modified: None };
            let stage = if entry.is_dir { Stage::Before } else { Stage::Parallel };
            let item = PlanItem::new(stage, facts).target(self.dir.join(&relative)).checked();
            // `root` is the item's place in the source (the engine reads only `is_root`).
            let item = if relative.components().count() == 1 { item.top(index) } else { item.under(index) };
            if !sink.item(item) {
                return;
            }
        }
    }

    fn run(&self, item: &PlanItem, cx: &RunCx<'_>) -> io::Result<Outcome> {
        let Some(target) = &item.target else { return Ok(Outcome::Nothing) };
        self.check_target(target)?;
        if item.tag == MAKE_PARENT {
            return super::make_parent_dir(item);
        }
        if item.facts.is_dir {
            // Nothing when merged into a folder that was already there.
            return Ok(if super::make_dir(target)? {
                Outcome::Created { path: target.clone(), facts: facts_after(target, true), from: None }
            } else {
                Outcome::Nothing
            });
        }
        // Written as a new file under a temporary name, then put in place without replacing
        // anything: a cut-short write never passes for the file, and nothing is written over
        // (names that meet once made safe, `a<b` and `a>b`, go through the conflict list).
        let temp = cx.temp_file_for(target);
        let mut counted = 0u64;
        let written = self.files.0.write(item.root, &temp, &mut |done| {
            cx.add_bytes(done.saturating_sub(counted));
            counted = counted.max(done);
            !cx.stopped()
        });
        if let Err(err) = written.and_then(|_| gezik_platform::fs::move_entry(&temp, target)) {
            let _ = gezik_platform::fs::delete(&temp);
            return Err(err);
        }
        Ok(Outcome::Created { path: target.clone(), facts: facts_after(target, false), from: None })
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    use gezik_core::ops::conflict::Decision;
    use gezik_platform::dnd::{VirtualEntry, VirtualFiles, VirtualSource};

    use super::*;
    use crate::engine::ConflictItem;
    use crate::testing::{defaults, engine, finish, read, test_dir, write};

    #[derive(Clone, Copy, PartialEq)]
    enum Mode {
        Plain,
        /// `entries` waits until the job is cancelled (a promise that never comes).
        NeverArrives,
        /// Every write goes on until the job is cancelled.
        Endless,
        /// Every write puts a few bytes down, then fails.
        Fails,
    }

    /// Items in memory, as another program would offer them.
    struct Fake {
        entries: Vec<VirtualEntry>,
        bodies: Vec<&'static str>,
        mode: Mode,
        started: Arc<AtomicBool>,
    }

    impl VirtualSource for Fake {
        fn entries(&self, stop: &dyn Fn() -> bool) -> io::Result<Vec<VirtualEntry>> {
            if self.mode == Mode::NeverArrives {
                self.started.store(true, Ordering::SeqCst);
                while !stop() {
                    std::thread::sleep(Duration::from_millis(5));
                }
                return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
            }
            Ok(self.entries.clone())
        }

        fn write(&self, index: usize, to: &Path, progress: &mut dyn FnMut(u64) -> bool) -> io::Result<u64> {
            let mut file = std::fs::File::create_new(to)?;
            let body = self.bodies.get(index).copied().unwrap_or_default();
            file.write_all(body.as_bytes())?;
            let mut done = body.len() as u64;
            if self.mode == Mode::Fails {
                return Err(io::Error::other("The attachment could not be read"));
            }
            if self.mode == Mode::Endless {
                self.started.store(true, Ordering::SeqCst);
                loop {
                    file.write_all(&[0; 1024])?;
                    done += 1024;
                    if !progress(done) {
                        return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
                    }
                }
            }
            Ok(done)
        }
    }

    fn file(name: &str) -> VirtualEntry {
        VirtualEntry { name: name.into(), is_dir: false, size: Some(3), failed: None }
    }

    fn folder(name: &str) -> VirtualEntry {
        VirtualEntry { name: name.into(), is_dir: true, size: None, failed: None }
    }

    fn task(
        dir: &Path,
        entries: Vec<VirtualEntry>,
        bodies: Vec<&'static str>,
        mode: Mode,
    ) -> (Box<dyn Task>, Arc<AtomicBool>) {
        let started = Arc::new(AtomicBool::new(false));
        let count = entries.len();
        let fake = Fake { entries, bodies, mode, started: started.clone() };
        (Box::new(MaterializeTask::new(VirtualFiles(Arc::new(fake)), dir, count)), started)
    }

    fn wait_for(flag: &AtomicBool) {
        let deadline = Instant::now() + Duration::from_secs(30);
        while !flag.load(Ordering::SeqCst) {
            assert!(Instant::now() < deadline, "the fake source never started");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn names_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> =
            std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names
    }

    #[test]
    fn dropped_items_are_written_and_undone_into_the_trash() {
        let dir = test_dir("materialize-plain");
        let entries = vec![file("rapor.pdf"), folder("Ekler"), file("Ekler/a.txt"), file("Yeni/iç/b.txt")];
        let (task, _) = task(&dir, entries, vec!["pdf", "", "aaa", "bbb"], Mode::Plain);
        let engine = engine();
        let (report, _) = finish(&engine, engine.submit(task), defaults);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(read(&dir.join("rapor.pdf")), "pdf");
        assert_eq!(read(&dir.join("Ekler").join("a.txt")), "aaa");
        assert_eq!(read(&dir.join("Yeni").join("iç").join("b.txt")), "bbb");
        assert!(report.results.contains(&dir.join("rapor.pdf")) && report.results.contains(&dir.join("Ekler")));
        assert_eq!(engine.undo_label().as_deref(), Some("Copy 4 items"));
        finish(&engine, engine.undo().unwrap(), defaults);
        assert!(names_in(&dir).is_empty(), "undo takes all it made: {:?}", names_in(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unsafe_names_never_leave_the_folder() {
        let root = test_dir("materialize-names");
        let dir = root.join("drop");
        std::fs::create_dir(&dir).unwrap();
        let entries = vec![file("../evil.txt"), file("/abs.txt"), file("a/../../evil2.txt"), file(""), file("ok.txt")];
        let (task, _) = task(&dir, entries, vec!["x", "x", "x", "x", "fine"], Mode::Plain);
        let engine = engine();
        let (report, _) = finish(&engine, engine.submit(task), defaults);
        assert_eq!(report.failures.len(), 4, "{:?}", report.failures);
        assert!(report.failures.iter().all(|f| f.path.starts_with(&dir)), "{:?}", report.failures);
        assert_eq!(names_in(&root), ["drop"]);
        assert_eq!(names_in(&dir), ["ok.txt"]);
        assert!(!Path::new("/abs.txt").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn too_deep_a_path_is_refused() {
        let dir = test_dir("materialize-deep");
        let deep = format!("{}x.txt", "d/".repeat(MAX_PARTS));
        let fine = format!("{}x.txt", "d/".repeat(MAX_PARTS - 1));
        let (task, _) = task(&dir, vec![file(&deep), file(&fine)], vec!["deep", "fine"], Mode::Plain);
        let engine = engine();
        let (report, _) = finish(&engine, engine.submit(task), defaults);
        assert_eq!(report.failures.len(), 1, "{:?}", report.failures);
        assert!(report.failures[0].message.contains("Too many folders"), "{:?}", report.failures);
        let mut at = dir.clone();
        (0..MAX_PARTS - 1).for_each(|_| at.push("d"));
        assert_eq!(read(&at.join("x.txt")), "fine");
        assert_eq!(names_in(&at), ["x.txt"], "nothing went one level deeper");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn windows_names_are_made_safe_on_disk() {
        let dir = test_dir("materialize-windows-names");
        let (task, _) =
            task(&dir, vec![file("CON"), file(r"C:\evil.txt"), file("a?.txt")], vec!["c", "x", "q"], Mode::Plain);
        let engine = engine();
        let (report, _) = finish(&engine, engine.submit(task), defaults);
        assert_eq!(report.failures.len(), 1, "{:?}", report.failures);
        assert_eq!(names_in(&dir), ["_CON", "a_.txt"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_taken_name_meets_the_conflict_list() {
        let dir = test_dir("materialize-conflict");
        write(&dir.join("a.txt"), "old");
        let engine = engine();
        let keep_both = |conflicts: &[ConflictItem]| {
            assert_eq!(conflicts.len(), 1);
            vec![Decision::KeepBoth]
        };
        let (task1, _) = task(&dir, vec![file("a.txt")], vec!["new"], Mode::Plain);
        finish(&engine, engine.submit(task1), keep_both);
        assert_eq!(read(&dir.join("a.txt")), "old", "never written over");
        assert_eq!(read(&dir.join("a (2).txt")), "new");
        let (task2, _) = task(&dir, vec![file("a.txt")], vec!["newer"], Mode::Plain);
        finish(&engine, engine.submit(task2), |c: &[ConflictItem]| vec![Decision::Skip; c.len()]);
        assert_eq!(names_in(&dir), ["a (2).txt", "a.txt"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn names_that_meet_after_cleaning_never_write_over_each_other() {
        let dir = test_dir("materialize-meet");
        let (task, _) = task(&dir, vec![file("a<b.txt"), file("a>b.txt")], vec!["one", "two"], Mode::Plain);
        let engine = engine();
        let (report, _) = finish(&engine, engine.submit(task), defaults);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(read(&dir.join("a_b.txt")), "one");
        assert_eq!(names_in(&dir), ["a_b (2).txt", "a_b.txt"], "the second met the first: kept both");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_link_on_the_way_is_not_followed() {
        let root = test_dir("materialize-link");
        let (dir, elsewhere) = (root.join("drop"), root.join("elsewhere"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::create_dir_all(&elsewhere).unwrap();
        #[cfg(windows)]
        {
            let made = std::process::Command::new("cmd")
                .args(["/c", "mklink", "/J"])
                .arg(dir.join("Ekler"))
                .arg(&elsewhere)
                .output()
                .unwrap();
            assert!(made.status.success(), "{made:?}");
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(&elsewhere, dir.join("Ekler")).unwrap();
        let entries = vec![file("Ekler/a.txt"), folder("Ekler"), file("Ekler/in/b.txt")];
        let (task, _) = task(&dir, entries, vec!["x", "", "y"], Mode::Plain);
        let engine = engine();
        let (report, _) = finish(&engine, engine.submit(task), defaults);
        assert!(!report.failures.is_empty(), "{:?}", report.failures);
        assert!(names_in(&elsewhere).is_empty(), "nothing went through the link: {:?}", names_in(&elsewhere));
        assert!(std::fs::symlink_metadata(dir.join("Ekler")).is_ok(), "the link stays: {:?}", report.failures);
        let _ = std::fs::remove_dir(dir.join("Ekler"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn cancel_mid_write_leaves_nothing() {
        let dir = test_dir("materialize-cancel-write");
        let (task, started) = task(&dir, vec![file("big.bin")], vec!["x"], Mode::Endless);
        let engine = engine();
        let job = engine.submit(task);
        wait_for(&started);
        engine.cancel(job);
        let (report, _) = finish(&engine, job, defaults);
        assert!(report.cancelled);
        assert!(names_in(&dir).is_empty(), "no target, no temporary name: {:?}", names_in(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cancel_while_waiting_for_the_files() {
        let dir = test_dir("materialize-cancel-wait");
        let (task, started) = task(&dir, vec![file("a.txt")], vec!["x"], Mode::NeverArrives);
        let engine = engine();
        let job = engine.submit(task);
        wait_for(&started);
        engine.cancel(job);
        let (report, _) = finish(&engine, job, defaults);
        assert!(report.cancelled && report.failures.is_empty(), "{report:?}");
        assert!(names_in(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_write_leaves_nothing_and_failures_are_reported() {
        let dir = test_dir("materialize-fails");
        let mut gone = file("gone.txt");
        gone.failed = Some("The promise was broken".into());
        let (task, _) = task(&dir, vec![file("a.txt"), gone], vec!["abc", ""], Mode::Fails);
        let engine = engine();
        let (report, _) = finish(&engine, engine.submit(task), defaults);
        let messages: Vec<&str> = report.failures.iter().map(|f| f.message.as_str()).collect();
        assert_eq!(report.failures.len(), 2, "{messages:?}");
        assert!(messages.iter().any(|m| m.contains("could not be read")), "{messages:?}");
        assert!(messages.iter().any(|m| m.contains("promise was broken")), "{messages:?}");
        assert!(names_in(&dir).is_empty(), "the partial file went: {:?}", names_in(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
