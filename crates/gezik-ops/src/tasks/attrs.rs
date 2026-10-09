//! Changing permissions, owner, group and flags (the Info window, spec 9 §4.4): exactly the
//! items the window shows, each only if it is still the file it was and still as it was shown;
//! or everything inside one folder ("Apply to enclosed items"). Links are never followed.
//! Undo writes back what was there, item by item, with the same check.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gezik_core::attrs::{Attrs, Entry, Identity, Wanted, enclosed_target};
use gezik_core::ops::conflict::Facts;

use super::{name, what};
use crate::task::{Outcome, PlanItem, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work, changed_since};
use crate::walk::{Step, walk};

/// What an item the system refused without administrator rights reports. The Info window
/// looks for it; 9b7 offers to do those items as administrator.
pub const NEEDS_ADMIN: &str = "Requires administrator";

/// An item found inside the folder of "Apply to enclosed items".
const ENCLOSED: u8 = 1;

/// Reads and writes items: the system, or a test's memory.
pub(crate) trait AttrIo: Send + Sync {
    fn read(&self, path: &Path) -> io::Result<Entry>;
    /// Writes `to` over `from` if `path` is still `id`.
    fn write(&self, path: &Path, id: Identity, from: Attrs, to: Attrs) -> io::Result<()>;
}

struct SystemIo;

impl AttrIo for SystemIo {
    fn read(&self, path: &Path) -> io::Result<Entry> {
        gezik_platform::attrs::read(path)
    }

    fn write(&self, path: &Path, id: Identity, from: Attrs, to: Attrs) -> io::Result<()> {
        gezik_platform::attrs::write(path, id, from, to)
    }
}

#[derive(Debug)]
struct NeedsAdmin;

impl fmt::Display for NeedsAdmin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(NEEDS_ADMIN)
    }
}

impl std::error::Error for NeedsAdmin {}

/// EPERM and EACCES say what they mean for the Info window.
fn refused(err: io::Error) -> io::Error {
    if err.kind() == io::ErrorKind::PermissionDenied {
        io::Error::new(io::ErrorKind::PermissionDenied, NeedsAdmin)
    } else {
        err
    }
}

pub struct SetAttributesTask {
    /// The items and what each becomes (the window's change, or an undo).
    items: Vec<Wanted>,
    /// "Apply to enclosed items": the folder, which file it was, and the attributes it spreads.
    enclosed: Option<(PathBuf, Identity, Attrs)>,
    io: Arc<dyn AttrIo>,
}

impl SetAttributesTask {
    /// Changes `items` (`gezik_core::attrs::wanted`): each only if it is still `id` and `from`.
    pub fn new(items: Vec<Wanted>) -> SetAttributesTask {
        SetAttributesTask { items, enclosed: None, io: Arc::new(SystemIo) }
    }

    /// Gives everything inside `folder` (not `folder` itself) its owner, group and permissions
    /// (`enclosed_target`), if `folder` is still `id`.
    pub fn enclosed(folder: PathBuf, id: Identity, attrs: Attrs) -> SetAttributesTask {
        SetAttributesTask { items: Vec::new(), enclosed: Some((folder, id, attrs)), io: Arc::new(SystemIo) }
    }

    #[cfg(test)]
    pub(crate) fn with_io(mut self, io: Arc<dyn AttrIo>) -> SetAttributesTask {
        self.io = io;
        self
    }

    fn paths(&self) -> Vec<PathBuf> {
        match &self.enclosed {
            Some((folder, ..)) => vec![folder.clone()],
            None => self.items.iter().map(|w| w.path.clone()).collect(),
        }
    }
}

impl Task for SetAttributesTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Attributes
    }

    fn title(&self) -> String {
        match &self.enclosed {
            Some((folder, ..)) => format!("Changing the items in {}", name(folder)),
            None => format!("Changing {}", what(&self.paths())),
        }
    }

    fn count(&self) -> usize {
        self.items.len().max(usize::from(self.enclosed.is_some()))
    }

    fn resources(&self) -> Resources {
        Resources { paths: self.paths(), work: Work::Disk }
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        for (root, wanted) in self.items.iter().enumerate() {
            if !sink.item(PlanItem::new(Stage::Parallel, Facts::default()).source(&wanted.path).top(root)) {
                return;
            }
        }
        let Some((folder, id, _)) = &self.enclosed else { return };
        match self.io.read(folder) {
            Ok(now) if now.id == *id && now.is_dir && !now.is_link => {}
            Ok(_) => return sink.failed(folder, changed_since()),
            Err(err) => return sink.failed(folder, err),
        }
        // Stops when the job is cancelled (the sink says so). The walk never enters a link.
        walk(folder, &mut |step| match step {
            Step::Entry { path, facts, .. } => sink.item(
                PlanItem::new(Stage::Parallel, Facts { is_dir: facts.is_dir, ..Facts::default() })
                    .source(path)
                    .under(0)
                    .tag(ENCLOSED),
            ),
            Step::Failed { path, error } => {
                sink.failed(path, error);
                true
            }
        });
    }

    fn run(&self, item: &PlanItem, cx: &RunCx<'_>) -> io::Result<Outcome> {
        let Some(path) = &item.source else { return Ok(Outcome::Nothing) };
        let now = self.io.read(path)?;
        let (from, to) = if item.tag == ENCLOSED {
            let Some((_, _, folder)) = &self.enclosed else { return Ok(Outcome::Nothing) };
            // A link inside is left as it is (and what it leads to is never reached).
            if now.is_link {
                return Ok(Outcome::Nothing);
            }
            (now.attrs, enclosed_target(*folder, now.attrs, now.is_dir))
        } else {
            let Some(wanted) = self.items.get(item.root) else { return Ok(Outcome::Nothing) };
            if now.id != wanted.id || now.attrs != wanted.from {
                return Err(changed_since());
            }
            (wanted.from, wanted.to)
        };
        if from == to {
            return Ok(Outcome::Nothing);
        }
        let changed = |after| Outcome::AttributesChanged { path: path.clone(), id: now.id, before: from, after };
        let Err(err) = self.io.write(path, now.id, from, to) else { return Ok(changed(to)) };
        // A write that failed halfway (the owner changed, the permissions not) stays undoable.
        match self.io.read(path) {
            Ok(after) if after.id != now.id => Err(changed_since()),
            Ok(after) if after.attrs != from => {
                cx.fail(path, &refused(err));
                Ok(changed(after.attrs))
            }
            _ => Err(refused(err)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};
    use std::sync::Mutex;

    use gezik_core::attrs::{Change, wanted};

    use super::*;
    use crate::engine::Report;
    use crate::task::TaskKind;
    use crate::testing::{defaults, engine, finish, test_dir, write};

    /// Attributes kept in memory for real paths (the walk and the engine see real files).
    #[derive(Default)]
    struct Fake {
        entries: Mutex<HashMap<PathBuf, Entry>>,
        denied: Mutex<HashSet<PathBuf>>,
        /// Paths whose write changes only the owner and group, then fails.
        halfway: Mutex<HashSet<PathBuf>>,
        /// Paths an editor saves over just before the write.
        swapped: Mutex<HashSet<PathBuf>>,
    }

    impl Fake {
        fn entry(&self, path: &Path) -> Entry {
            self.read(path).unwrap()
        }

        fn edit(&self, path: &Path, f: impl FnOnce(&mut Entry)) {
            self.entry(path);
            f(self.entries.lock().unwrap().get_mut(path).unwrap());
        }
    }

    impl AttrIo for Fake {
        fn read(&self, path: &Path) -> io::Result<Entry> {
            let meta = std::fs::symlink_metadata(path)?;
            let mut entries = self.entries.lock().unwrap();
            let ino = entries.len() as u64 + 1;
            let mode = if meta.is_dir() { 0o755 } else { 0o644 };
            Ok(*entries.entry(path.to_path_buf()).or_insert(Entry {
                id: Identity { dev: 1, ino },
                attrs: Attrs { mode, uid: 501, gid: 20, flags: 0 },
                is_dir: meta.is_dir(),
                is_link: false,
            }))
        }

        fn write(&self, path: &Path, id: Identity, _from: Attrs, to: Attrs) -> io::Result<()> {
            if self.denied.lock().unwrap().contains(path) {
                return Err(io::ErrorKind::PermissionDenied.into());
            }
            let mut entries = self.entries.lock().unwrap();
            let entry = entries.get_mut(path).ok_or(io::ErrorKind::NotFound)?;
            if self.swapped.lock().unwrap().contains(path) {
                entry.id.ino += 1000;
            }
            if entry.id != id {
                return Err(io::ErrorKind::NotFound.into());
            }
            if self.halfway.lock().unwrap().contains(path) {
                (entry.attrs.uid, entry.attrs.gid) = (to.uid, to.gid);
                return Err(io::Error::other("The system turned setuid or setgid off"));
            }
            entry.attrs = to;
            Ok(())
        }
    }

    fn files(name: &str, names: &[&str]) -> (PathBuf, Vec<PathBuf>) {
        let dir = test_dir(name);
        let paths: Vec<PathBuf> = names.iter().map(|n| dir.join(n)).collect();
        for path in &paths {
            write(path, "x");
        }
        (dir, paths)
    }

    fn shown(fake: &Fake, paths: &[PathBuf]) -> Vec<(PathBuf, Entry)> {
        paths.iter().map(|p| (p.clone(), fake.entry(p))).collect()
    }

    fn run(fake: &Arc<Fake>, task: SetAttributesTask) -> Report {
        let engine = engine();
        let job = engine.submit(Box::new(task.with_io(fake.clone())));
        finish(&engine, job, defaults).0
    }

    #[test]
    fn only_the_shown_items_change() {
        let (dir, paths) = files("attrs-shown", &["a.txt", "b.txt", "c.txt"]);
        let fake = Arc::new(Fake::default());
        let items = shown(&fake, &paths[..2]);
        let report = run(&fake, SetAttributesTask::new(wanted(&items, Change::bit(0o020, true))));
        assert!(report.failures.is_empty() && report.skipped_changed == 0, "{:?}", report.failures);
        let modes: Vec<u32> = paths.iter().map(|p| fake.entry(p).attrs.mode).collect();
        assert_eq!(modes, [0o664, 0o664, 0o644], "the third was not shown");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_item_changed_since_it_was_shown_is_left_alone() {
        let (dir, paths) = files("attrs-changed", &["a.txt", "b.txt", "c.txt"]);
        let fake = Arc::new(Fake::default());
        let items = shown(&fake, &paths);
        fake.edit(&paths[0], |e| e.attrs.mode = 0o600); // chmod in a terminal meanwhile
        fake.edit(&paths[1], |e| e.id.ino = 999); // saved over by an editor: another file
        let report = run(&fake, SetAttributesTask::new(wanted(&items, Change::bit(0o004, false))));
        assert_eq!(report.skipped_changed, 2);
        assert!(report.failures.is_empty());
        let modes: Vec<u32> = paths.iter().map(|p| fake.entry(p).attrs.mode).collect();
        assert_eq!(modes, [0o600, 0o644, 0o640]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_refused_item_fails_alone_and_asks_for_an_administrator() {
        let (dir, paths) = files("attrs-denied", &["a.txt", "b.txt"]);
        let fake = Arc::new(Fake::default());
        let items = shown(&fake, &paths);
        fake.denied.lock().unwrap().insert(paths[0].clone());
        let report = run(&fake, SetAttributesTask::new(wanted(&items, Change::owner(0))));
        assert_eq!(report.failures.len(), 1);
        assert_eq!((&report.failures[0].path, report.failures[0].message.as_str()), (&paths[0], NEEDS_ADMIN));
        assert_eq!((fake.entry(&paths[0]).attrs.uid, fake.entry(&paths[1]).attrs.uid), (501, 0));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_write_that_failed_halfway_is_reported_and_undoable() {
        let (dir, paths) = files("attrs-halfway", &["a.txt"]);
        let fake = Arc::new(Fake::default());
        let items = shown(&fake, &paths);
        fake.halfway.lock().unwrap().insert(paths[0].clone());
        let engine = engine();
        let task = SetAttributesTask::new(wanted(&items, Change { uid: Some(0), ..Change::mode(0o600) }));
        let (report, _) = finish(&engine, engine.submit(Box::new(task.with_io(fake.clone()))), defaults);
        assert_eq!(report.failures.len(), 1, "the failure is reported");
        assert_eq!(
            engine.undo_label().as_deref(),
            Some("Change attributes of 1 item"),
            "and the owner change can be undone"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_saved_over_just_before_the_write_is_left_alone() {
        let (dir, paths) = files("attrs-swapped", &["a.txt"]);
        let fake = Arc::new(Fake::default());
        let items = shown(&fake, &paths);
        fake.swapped.lock().unwrap().insert(paths[0].clone());
        let report = run(&fake, SetAttributesTask::new(wanted(&items, Change::bit(0o020, true))));
        assert_eq!((report.skipped_changed, report.failures.len()), (1, 0));
        assert_eq!(fake.entry(&paths[0]).attrs.mode, 0o644);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn undo_writes_back_what_was_there_unless_it_changed_again() {
        let (dir, paths) = files("attrs-undo", &["a.txt", "b.txt"]);
        let fake = Arc::new(Fake::default());
        let changed = wanted(&shown(&fake, &paths), Change::bit(0o020, true));
        run(&fake, SetAttributesTask::new(changed.clone()));
        // What undo is built from: one task for both, as the engine records it.
        let outcomes: Vec<Outcome> = changed
            .iter()
            .map(|w| Outcome::AttributesChanged { path: w.path.clone(), id: w.id, before: w.from, after: w.to })
            .collect();
        let undo = crate::inverse::build(&outcomes);
        assert_eq!((undo.len(), undo[0].kind(), undo[0].count()), (1, TaskKind::Attributes, 2));
        // The same items, this test's io: the second was changed again since.
        fake.edit(&paths[1], |e| e.attrs.mode = 0o600);
        let back: Vec<Wanted> = changed.iter().map(|w| Wanted { from: w.to, to: w.from, ..w.clone() }).collect();
        let report = run(&fake, SetAttributesTask::new(back));
        assert_eq!(report.skipped_changed, 1);
        assert_eq!((fake.entry(&paths[0]).attrs.mode, fake.entry(&paths[1]).attrs.mode), (0o644, 0o600));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn enclosed_items_take_the_folder_settings_but_links_and_the_folder_stay() {
        let dir = test_dir("attrs-enclosed");
        let top = dir.join("top");
        for name in ["a.txt", "sub/run.sh", "link"] {
            write(&top.join(name), "x");
        }
        let fake = Arc::new(Fake::default());
        let folder = fake.entry(&top);
        fake.edit(&top.join("sub/run.sh"), |e| e.attrs.mode = 0o755);
        fake.edit(&top.join("link"), |e| e.is_link = true);
        let spread = Attrs { mode: 0o750, gid: 30, ..folder.attrs };
        let report = run(&fake, SetAttributesTask::enclosed(top.clone(), folder.id, spread));
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(fake.entry(&top.join("a.txt")).attrs, Attrs { mode: 0o640, uid: 501, gid: 30, flags: 0 });
        assert_eq!(fake.entry(&top.join("sub/run.sh")).attrs.mode, 0o750, "runnable stays runnable");
        assert_eq!(fake.entry(&top.join("sub")).attrs.mode, 0o750);
        assert_eq!(fake.entry(&top.join("link")).attrs.mode, 0o644, "a link is left as it is");
        assert_eq!(fake.entry(&top).attrs, folder.attrs, "the folder itself is not changed");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn enclosed_does_nothing_if_the_folder_is_another_one() {
        let dir = test_dir("attrs-enclosed-other");
        write(&dir.join("top/a.txt"), "x");
        let fake = Arc::new(Fake::default());
        let folder = fake.entry(&dir.join("top"));
        fake.edit(&dir.join("top"), |e| e.id.ino = 999);
        let task = SetAttributesTask::enclosed(dir.join("top"), folder.id, Attrs { mode: 0o700, ..folder.attrs });
        let report = run(&fake, task);
        assert_eq!(report.failures.len(), 1);
        assert_eq!(fake.entry(&dir.join("top/a.txt")).attrs.mode, 0o644);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn real_permissions_change_and_undo_through_the_engine() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, paths) = files("attrs-real", &["f.txt"]);
        let file = &paths[0];
        std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o644)).unwrap();
        let entry = gezik_platform::attrs::read(file).unwrap();
        let engine = engine();
        let task = SetAttributesTask::new(wanted(&[(file.clone(), entry)], Change::bit(0o020, true)));
        let (report, _) = finish(&engine, engine.submit(Box::new(task)), defaults);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        let mode = || std::fs::symlink_metadata(file).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(), 0o664);
        assert_eq!(engine.undo_label().as_deref(), Some("Change attributes of 1 item"));
        let (undo, _) = finish(&engine, engine.undo().unwrap(), defaults);
        assert!(undo.failures.is_empty() && undo.skipped_changed == 0, "{:?}", undo.failures);
        assert_eq!(mode(), 0o644);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
