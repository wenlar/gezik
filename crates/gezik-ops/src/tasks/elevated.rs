//! A list of administrator operations (spec 9 §10): one item, so one prompt for the whole list;
//! the helper does it, then only the disk is believed: each operation the helper calls done is
//! looked at (without rights) before it counts or gets an undo. Undo is another such list,
//! behind a new prompt.

use std::io;
use std::path::Path;

use gezik_core::elevated::{self, Answer, Before, LaunchError, Op, System};
use gezik_core::ops::conflict::Facts;

use crate::task::{Outcome, PlanItem, Resources, RunCx, ScanSink, Stage, Task, TaskKind, Work};

/// Runs the helper on a list and hands over its reply lines (the app: the system's prompt,
/// `gezik_platform::elevate::run`; tests: a fake). Never called unless the user asked.
pub trait Elevator: Send + Sync {
    fn run(&self, ops: &[Op], prompt: &str, reply: &mut dyn FnMut(&str)) -> Result<(), LaunchError>;
}

pub const NOT_AVAILABLE: &str = "Administrator operations are not available here";
pub const CANCELLED: &str = "Not done: the administrator prompt was cancelled";
pub const NOT_SO: &str = "The administrator helper said it was done, but it is not";
/// The app's note for `Report::unchecked`.
pub const UNCHECKED: &str = "Done as administrator; Gezik could not check it";
/// Whether names that differ only in case are the same name here (Windows and macOS by default).
const CASE_BLIND: bool = cfg!(any(windows, target_os = "macos"));
/// Reply lines read at most (the helper writes one per operation and one more).
const MAX_REPLIES: usize = 100_000;

pub struct ElevatedTask {
    ops: Vec<Op>,
}

impl ElevatedTask {
    pub fn new(ops: Vec<Op>) -> ElevatedTask {
        ElevatedTask { ops }
    }
}

/// What Gezik sees at a path without rights.
enum Look {
    /// There; whether it is a folder.
    There(bool),
    Gone,
    /// No way to look (no rights there).
    Unknown,
}

fn look(path: &Path) -> Look {
    match std::fs::symlink_metadata(path) {
        Ok(meta) => Look::There(meta.is_dir()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Look::Gone,
        Err(_) => listed(path),
    }
}

/// The item looked up in its folder's list, when the item itself cannot be looked at.
// shortcut: reads the whole folder per item; fine for a list that fits one command line.
fn listed(path: &Path) -> Look {
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else { return Look::Unknown };
    let Ok(mut entries) = std::fs::read_dir(parent) else { return Look::Unknown };
    let fold = |text: &std::ffi::OsStr| {
        let text = text.to_string_lossy();
        if CASE_BLIND { text.to_lowercase() } else { text.into_owned() }
    };
    let wanted = fold(name);
    let Some(entry) = entries.find(|entry| entry.as_ref().is_ok_and(|entry| fold(&entry.file_name()) == wanted)) else {
        return Look::Gone;
    };
    match entry.and_then(|entry| entry.file_type()) {
        Ok(kind) => Look::There(kind.is_dir()),
        Err(_) => Look::Unknown,
    }
}

/// What an undo may rely on, seen before the helper runs.
fn before(op: &Op) -> Before {
    match op {
        Op::Copy { to, .. } | Op::Move { to, .. } => match look(to) {
            Look::Gone => Before::Absent,
            Look::There(_) => Before::Present,
            Look::Unknown => Before::Unknown,
        },
        Op::Chmod { path, .. } | Op::Chown { path, .. } | Op::Chflags { path, .. } => {
            gezik_platform::attrs::read(path).map_or(Before::Unknown, |entry| Before::Attrs(entry.attrs))
        }
        _ => Before::Present,
    }
}

/// A rename that changes only the case: on a case-blind disk (`case_blind`) the old name still
/// "exists". Elsewhere the old name must be gone like any other.
fn same_name(a: &Path, b: &Path, case_blind: bool) -> bool {
    case_blind && a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
}

/// Whether the disk shows `op` done: `Some(true)`, `Some(false)`, or `None`: Gezik cannot look.
fn shows_done(op: &Op) -> Option<bool> {
    let there = |path: &Path| match look(path) {
        Look::There(_) => Some(true),
        Look::Gone => Some(false),
        Look::Unknown => None,
    };
    let gone = |path: &Path| there(path).map(|is| !is);
    let attrs = |path: &Path| gezik_platform::attrs::read(path).ok().map(|entry| entry.attrs);
    match op {
        Op::Copy { to, .. } => there(to),
        Op::Move { from, to, .. } => Some(there(to)? && (same_name(from, to, CASE_BLIND) || gone(from)?)),
        Op::Delete(path) | Op::Rmdir(path) => gone(path),
        Op::Rename { path, name } => {
            let new = path.with_file_name(name);
            Some(there(&new)? && (same_name(path, &new, CASE_BLIND) || gone(path)?))
        }
        Op::Mkdir(path) => match look(path) {
            Look::There(is_dir) => Some(is_dir),
            Look::Gone => Some(false),
            Look::Unknown => None,
        },
        Op::Chmod { path, mode } => attrs(path).map(|a| a.mode & 0o7777 == *mode),
        Op::Chown { path, uid, gid } => attrs(path).map(|a| (a.uid, a.gid) == (*uid, *gid)),
        Op::Chflags { path, set, clear } => attrs(path).map(|a| a.flags & (set | clear) == *set),
    }
}

/// The undo of `op`, only if the helper would take it: it passes the same rules as any list
/// (a rename back may bring a name the rules refuse). Its paths are the operation's own, which
/// passed the protected folders already, so only the rules of form are needed here.
fn undo_for(op: &Op, before: Before) -> Option<Op> {
    let sys = System { windows: cfg!(windows), macos: cfg!(target_os = "macos"), protected: &[] };
    elevated::undo_of(op, before).filter(|undo| elevated::check(std::slice::from_ref(undo), &sys).is_ok())
}

impl Task for ElevatedTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Elevated
    }

    fn title(&self) -> String {
        elevated::label(&self.ops)
    }

    fn count(&self) -> usize {
        self.ops.len()
    }

    fn resources(&self) -> Resources {
        let paths = self.ops.iter().flat_map(|op| std::iter::once(op.path()).chain(op.target())).map(Path::to_path_buf);
        Resources { paths: paths.collect(), work: Work::External }
    }

    fn workers(&self) -> Option<usize> {
        Some(1)
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        // One item: one prompt for the whole list (spec §10.2: never split).
        if let Some(first) = self.ops.first() {
            sink.item(PlanItem::new(Stage::Parallel, Facts::default()).source(first.path()).top(0));
        }
    }

    fn run(&self, _item: &PlanItem, cx: &RunCx<'_>) -> io::Result<Outcome> {
        let seen: Vec<Before> = self.ops.iter().map(before).collect();
        let count = self.ops.len();
        let answers = match cx.elevator() {
            None => vec![Answer::NotRun(NOT_AVAILABLE.to_owned()); count],
            Some(elevator) => {
                let mut replies = Vec::new();
                let ran = elevator.run(&self.ops, &elevated::prompt(&self.ops), &mut |line| {
                    if replies.len() < MAX_REPLIES
                        && let Some(reply) = elevated::parse_reply(line)
                    {
                        replies.push(reply);
                    }
                });
                match ran {
                    Ok(()) => elevated::answers(count, &replies),
                    Err(LaunchError::Cancelled) => vec![Answer::NotRun(CANCELLED.to_owned()); count],
                    Err(LaunchError::Unavailable(why) | LaunchError::Failed(why)) => vec![Answer::NotRun(why); count],
                }
            }
        };
        let mut undo = Vec::new();
        for ((op, before), answer) in self.ops.iter().zip(seen).zip(answers) {
            cx.touched(std::iter::once(op.path()).chain(op.target()));
            let failed = |message: &str| cx.fail(op.path(), &io::Error::other(message.to_owned()));
            match answer {
                Answer::Done => match shows_done(op) {
                    Some(true) => undo.extend(undo_for(op, before).map(|undo| Outcome::AsAdmin { undo })),
                    Some(false) => failed(NOT_SO),
                    // Gezik cannot look there without rights: the helper's word stands, noted as
                    // unchecked, and no undo is built on it.
                    None => cx.unchecked(op.path()),
                },
                Answer::Failed(message) | Answer::NotRun(message) => failed(&message),
            }
        }
        Ok(Outcome::Several(undo))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    use gezik_core::elevated::{DONE, err_line, ok_line};

    use super::*;
    use crate::testing::{defaults, engine, finish, test_dir, write};

    /// The helper, played: it does the list with plain std calls (`act`), then says `lines`,
    /// then ends as `launch` says.
    struct Fake {
        act: bool,
        lines: Vec<&'static str>,
        launch: Result<(), LaunchError>,
        seen: Mutex<Vec<Vec<Op>>>,
    }

    impl Fake {
        fn new(act: bool, lines: &[&'static str], launch: Result<(), LaunchError>) -> Arc<Fake> {
            Arc::new(Fake { act, lines: lines.to_vec(), launch, seen: Mutex::default() })
        }

        fn seen(&self) -> Vec<Vec<Op>> {
            self.seen.lock().unwrap().clone()
        }
    }

    fn apply(op: &Op) -> io::Result<()> {
        match op {
            Op::Copy { from, to, .. } => std::fs::copy(from, to).map(drop),
            Op::Move { from, to, .. } => std::fs::rename(from, to),
            Op::Delete(path) if path.is_dir() => std::fs::remove_dir_all(path),
            Op::Delete(path) => std::fs::remove_file(path),
            Op::Rename { path, name } => std::fs::rename(path, path.with_file_name(name)),
            Op::Mkdir(path) => std::fs::create_dir(path),
            Op::Rmdir(path) => std::fs::remove_dir(path),
            _ => Err(io::ErrorKind::Unsupported.into()),
        }
    }

    impl Elevator for Fake {
        fn run(&self, ops: &[Op], _prompt: &str, reply: &mut dyn FnMut(&str)) -> Result<(), LaunchError> {
            self.seen.lock().unwrap().push(ops.to_vec());
            if self.act {
                for (i, op) in ops.iter().enumerate() {
                    match apply(op) {
                        Ok(()) => reply(&ok_line(i)),
                        Err(err) => reply(&err_line(i, err.raw_os_error().unwrap_or(0), &err.to_string())),
                    }
                }
                reply(DONE);
            }
            for line in &self.lines {
                reply(line);
            }
            self.launch.clone()
        }
    }

    fn run(fake: Option<Arc<Fake>>, ops: &[Op]) -> (crate::Engine, crate::Report) {
        let engine = engine();
        if let Some(fake) = fake {
            engine.set_elevator(fake);
        }
        let job = engine.submit_chain(vec![Box::new(ElevatedTask::new(ops.to_vec()))], Some(elevated::label(ops)));
        let report = finish(&engine, job, defaults).0;
        (engine, report)
    }

    fn messages(report: &crate::Report) -> Vec<(PathBuf, String)> {
        report.failures.iter().map(|f| (f.path.clone(), f.message.clone())).collect()
    }

    #[test]
    fn a_done_list_is_checked_on_disk_and_undone_with_a_new_prompt() {
        let dir = test_dir("elevated-done");
        write(&dir.join("a.txt"), "a");
        let ops =
            vec![Op::Copy { from: dir.join("a.txt"), to: dir.join("b.txt"), replace: false }, Op::Mkdir(dir.join("d"))];
        let fake = Fake::new(true, &[], Ok(()));
        let (engine, report) = run(Some(fake.clone()), &ops);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert!(report.as_admin.is_empty(), "nothing refused");
        assert!(dir.join("b.txt").exists() && dir.join("d").is_dir());
        assert_eq!(engine.undo_label().as_deref(), Some("2 changes as administrator"));
        assert!(engine.undo_needs_admin());
        let undo = engine.undo().unwrap();
        let undone = finish(&engine, undo, defaults).0;
        assert!(undone.failures.is_empty(), "{:?}", undone.failures);
        assert_eq!(
            fake.seen()[1],
            [Op::Rmdir(dir.join("d")), Op::Delete(dir.join("b.txt"))],
            "reverse order, one list"
        );
        assert!(!dir.join("b.txt").exists() && !dir.join("d").exists());
        assert!(engine.redo_needs_admin());
        assert!(!engine.undo_needs_admin(), "nothing left to undo");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_reply_the_disk_does_not_bear_out_is_a_failure() {
        let dir = test_dir("elevated-lie");
        let ops = [Op::Mkdir(dir.join("d"))];
        let (engine, report) = run(Some(Fake::new(false, &["ok 0", "done"], Ok(()))), &ops);
        assert_eq!(messages(&report), [(dir.join("d"), NOT_SO.to_owned())]);
        assert!(!report.failures[0].denied && report.as_admin.is_empty(), "never offered again as administrator");
        assert_eq!(engine.undo_label(), None, "nothing to undo");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_helpers_errors_and_silence_are_failures() {
        let dir = test_dir("elevated-errors");
        let ops = [Op::Mkdir(dir.join("d1")), Op::Mkdir(dir.join("d2"))];
        let lines = ["err 0 5 Access denied", "ok 7", "garbage", "err 0 2 late"];
        let (_, report) = run(Some(Fake::new(false, &lines, Ok(()))), &ops);
        assert_eq!(
            messages(&report),
            [(dir.join("d1"), "Access denied".to_owned()), (dir.join("d2"), elevated::STOPPED.to_owned())]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_cancelled_prompt_does_nothing_and_says_so() {
        let dir = test_dir("elevated-cancel");
        let ops = [Op::Mkdir(dir.join("d1")), Op::Mkdir(dir.join("d2"))];
        let (engine, report) = run(Some(Fake::new(false, &[], Err(LaunchError::Cancelled))), &ops);
        assert_eq!(messages(&report), [(dir.join("d1"), CANCELLED.to_owned()), (dir.join("d2"), CANCELLED.to_owned())]);
        assert!(!report.cancelled, "a failure to retry, not a cancelled job");
        assert_eq!(engine.undo_label(), None);
        let (_, missing) =
            run(Some(Fake::new(false, &[], Err(LaunchError::Unavailable("No pkexec".into())))), &ops[..1]);
        assert_eq!(messages(&missing), [(dir.join("d1"), "No pkexec".to_owned())]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_refused_list_is_not_done() {
        let dir = test_dir("elevated-refused");
        let ops = [Op::Mkdir(dir.join("d"))];
        let (_, report) = run(Some(Fake::new(false, &["refused item 1: Not a full path"], Ok(()))), &ops);
        assert_eq!(messages(&report), [(dir.join("d"), "Not done: item 1: Not a full path".to_owned())]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn without_an_elevator_nothing_runs() {
        let dir = test_dir("elevated-none");
        let ops = [Op::Mkdir(dir.join("d"))];
        let (_, report) = run(None, &ops);
        assert_eq!(messages(&report), [(dir.join("d"), NOT_AVAILABLE.to_owned())]);
        assert!(!dir.join("d").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_copy_over_a_file_that_was_there_has_no_undo() {
        let dir = test_dir("elevated-replace");
        write(&dir.join("a.txt"), "new");
        write(&dir.join("b.txt"), "old");
        let ops = [Op::Copy { from: dir.join("a.txt"), to: dir.join("b.txt"), replace: true }];
        let (engine, report) = run(Some(Fake::new(true, &[], Ok(()))), &ops);
        assert!(report.failures.is_empty());
        assert_eq!(std::fs::read_to_string(dir.join("b.txt")).unwrap(), "new");
        assert_eq!(engine.undo_label(), None, "the old b.txt is gone for good: no undo pretends otherwise");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn what_gezik_cannot_look_at_is_noted_not_believed() {
        let dir = test_dir("elevated-unchecked");
        // A name no system takes: the item cannot be looked at; its folder can, or cannot.
        let hidden = dir.join("a\0b").join("d");
        let listed = dir.join("x\0");
        let ops = [Op::Mkdir(hidden.clone()), Op::Mkdir(listed.clone())];
        let (engine, report) = run(Some(Fake::new(false, &["ok 0", "ok 1", "done"], Ok(()))), &ops);
        assert_eq!(report.unchecked, [hidden], "done by the helper's word, not checked");
        assert_eq!(messages(&report), [(listed, NOT_SO.to_owned())], "its folder's list shows it is not there");
        assert_eq!(engine.undo_label(), None, "no undo built on what was not seen");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn case_only_renames_follow_the_systems_rule() {
        let (a, b) = (Path::new("/d/ReadMe.md"), Path::new("/d/README.md"));
        assert!(same_name(a, b, true));
        assert!(!same_name(a, b, false), "Linux: two names, the old one must be gone");
        assert!(!same_name(a, Path::new("/d/other"), true));
    }

    #[test]
    fn an_undo_the_helper_would_refuse_is_not_offered() {
        let dir = std::env::temp_dir();
        let plain = Op::Rename { path: dir.join("a"), name: "b".into() };
        assert_eq!(undo_for(&plain, Before::Present), Some(Op::Rename { path: dir.join("b"), name: "a".into() }));
        // Renamed away from a name the rules refuse: renaming back would be refused.
        let odd = Op::Rename { path: dir.join("a\u{1}"), name: "b".into() };
        assert_eq!(undo_for(&odd, Before::Present), None);
    }
}
