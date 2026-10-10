//! "Retry as administrator" (the operations panel's Problems window) and "Change as
//! administrator…" (the Info window), spec 9 §10: what is asked before the system's prompt
//! (conflicts, then the whole list in words), Gezik's own check of the list, and the job that
//! runs the one-shot helper. Nothing here runs unless the user pressed one of those buttons.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use gezik_core::attrs::{HIDDEN, LOCKED};
use gezik_core::elevated::{self, LaunchError, Op, System};
use gezik_ops::{ElevatedTask, Elevator, JobId};

use crate::operations::{After, Operations};

pub const RETRY_AS_ADMIN: &str = "Retry as administrator";
pub const UNDO_ADMIN: &str = "Undoing this needs administrator rights.";
pub const REDO_ADMIN: &str = "Redoing this needs administrator rights.";
const EXACTLY: &str = "Gezik asks the system for administrator rights to do exactly this:";

/// The system's prompt and helper, for the engine (`Engine::set_elevator`).
pub struct SystemElevator {
    /// The window the Windows prompt belongs to.
    owner: isize,
}

impl SystemElevator {
    pub fn new(window: &crate::AppWindow) -> SystemElevator {
        use slint::ComponentHandle;
        SystemElevator { owner: gezik_platform::elevate::owner_of(&window.window().window_handle()) }
    }
}

impl Elevator for SystemElevator {
    fn run(&self, ops: &[Op], prompt: &str, reply: &mut dyn FnMut(&str)) -> Result<(), LaunchError> {
        let exe =
            gezik_platform::system::exe().map_err(|err| LaunchError::Failed(gezik_platform::fs::describe(&err)))?;
        gezik_platform::elevate::run(&exe, ops, prompt, self.owner, reply)
    }
}

/// The Problems window's buttons: the administrator first when the job offers it.
pub fn detail_buttons(admin: bool, retry: bool) -> Vec<&'static str> {
    let mut buttons = Vec::new();
    if admin {
        buttons.push(RETRY_AS_ADMIN);
    }
    if retry {
        buttons.push("Retry");
    }
    buttons.push("Close");
    buttons
}

/// `path` with the folder it is in resolved as Gezik sees it now (links, `\\?\`): the helper
/// then walks it without following anything. The item itself is left as it is (a link is
/// worked on as a link); a folder that cannot be resolved leaves the path as it is.
pub fn real_path(path: &Path, canonical: impl Fn(&Path) -> std::io::Result<PathBuf>) -> PathBuf {
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else { return path.to_path_buf() };
    let Ok(real) = canonical(parent) else { return path.to_path_buf() };
    let plain = match real.to_str().and_then(|text| text.strip_prefix(r"\\?\")) {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
        _ => real,
    };
    plain.join(name)
}

/// How many copies and moves would land on something already there.
pub fn taken(ops: &[Op], exists: &dyn Fn(&Path) -> bool) -> usize {
    ops.iter().filter(|op| op.target().is_some_and(exists)).count()
}

/// Those land with `replace` (files there are replaced; a folder is merged into, never replaced).
pub fn replacing(ops: Vec<Op>, exists: &dyn Fn(&Path) -> bool) -> Vec<Op> {
    ops.into_iter()
        .map(|op| match op {
            Op::Copy { from, to, .. } if exists(&to) => Op::Copy { from, to, replace: true },
            Op::Move { from, to, .. } if exists(&to) => Op::Move { from, to, replace: true },
            other => other,
        })
        .collect()
}

/// Those are left out.
pub fn skipping(ops: Vec<Op>, exists: &dyn Fn(&Path) -> bool) -> Vec<Op> {
    ops.into_iter().filter(|op| !op.target().is_some_and(exists)).collect()
}

/// The conflict question's buttons: Enter (the first) never replaces.
pub const TAKEN_BUTTONS: &[&str] = &["Cancel", "Replace", "Skip them"];

/// The confirmation's buttons and which one goes ahead: with a delete or a replace in the list,
/// Cancel comes first, so Enter does nothing destructive.
pub fn confirm_buttons(ops: &[Op]) -> (&'static [&'static str], usize) {
    let replaces = ops.iter().any(|op| matches!(op, Op::Copy { replace: true, .. } | Op::Move { replace: true, .. }));
    let deletes = ops.iter().any(|op| matches!(op, Op::Delete(_)));
    match (deletes, replaces) {
        (true, _) => (&["Cancel", "Delete"], 1),
        (false, true) => (&["Cancel", "Continue"], 1),
        _ => (&["Continue", "Cancel"], 0),
    }
}

fn items(n: usize) -> String {
    if n == 1 { "1 item".to_owned() } else { format!("{n} items") }
}

pub fn taken_text(n: usize) -> String {
    if n == 1 {
        "1 item is already where it goes. Replace it as administrator?".to_owned()
    } else {
        format!("{n} items are already where they go. Replace them as administrator?")
    }
}

/// Spec §10.2's confirmation of a permanent delete.
pub fn delete_text(n: usize) -> String {
    format!("This will delete {} permanently as administrator. It cannot be undone.", items(n))
}

/// `text` with what could hide or reorder what is shown (control, bidi and invisible format
/// characters) written as `\u{..}`: the list on screen reads as what is sent.
pub fn escaped(text: &str) -> String {
    text.chars()
        .map(|c| {
            let hidden = c.is_control()
                || matches!(
                    c,
                    '\u{AD}'
                        | '\u{61C}'
                        | '\u{180E}'
                        | '\u{200B}'..='\u{200F}'
                        | '\u{202A}'..='\u{202E}'
                        | '\u{2060}'..='\u{2069}'
                        | '\u{FEFF}'
                        | '\u{FFF9}'..='\u{FFFB}'
                );
            if hidden { format!("\\u{{{:04x}}}", u32::from(c)) } else { c.to_string() }
        })
        .collect()
}

fn shown(path: &Path) -> String {
    escaped(&path.display().to_string())
}

pub fn refusal_text(op: &Op, why: &str) -> String {
    format!("Not done as administrator: {} ({why})", shown(op.path()))
}

/// One operation in words, with every value the helper gets.
pub fn op_text(op: &Op) -> String {
    match op {
        Op::Copy { from, to, replace } => {
            let how = if *replace { ", replacing files there" } else { "" };
            format!("Copy {} to {}{how}", shown(from), shown(to))
        }
        Op::Move { from, to, replace } => {
            let how = if *replace { ", replacing a file there" } else { "" };
            format!("Move {} to {}{how}", shown(from), shown(to))
        }
        Op::Delete(path) => format!("Delete {} permanently", shown(path)),
        Op::Rename { path, name } => format!("Rename {} to \"{}\"", shown(path), escaped(name)),
        Op::Mkdir(path) => format!("Make the folder {}", shown(path)),
        Op::Rmdir(path) => format!("Remove the empty folder {}", shown(path)),
        Op::Chmod { path, mode } => format!("Set the permissions of {} to {mode:03o}", shown(path)),
        Op::Chown { path, uid, gid } => {
            format!("Set the owner of {} to user {uid} and group {gid}", shown(path))
        }
        Op::Chflags { path, set, clear } => {
            let flag = |bit: u32, name: &str| {
                [(set, "on"), (clear, "off")]
                    .into_iter()
                    .find(|(bits, _)| *bits & bit != 0)
                    .map(|(_, s)| format!("{name} {s}"))
            };
            let what: Vec<String> = [flag(HIDDEN, "Hidden"), flag(LOCKED, "Locked")].into_iter().flatten().collect();
            format!("Change the flags of {}: {}", shown(path), what.join(", "))
        }
    }
}

/// The whole list, one line each, and the warning when others could swap Gezik's exe.
fn list_text(ops: &[Op], exposed: bool) -> String {
    let mut lines: Vec<String> = ops.iter().map(op_text).collect();
    if exposed {
        lines.push(String::new());
        lines.push(gezik_platform::elevate::EXPOSED.to_owned());
    }
    lines.join("\n")
}

/// What the user says yes to before the system's prompt: exactly the list that is sent.
pub fn confirm_text(ops: &[Op], exposed: bool) -> String {
    let deletes = ops.iter().filter(|op| matches!(op, Op::Delete(_))).count();
    let lead = if deletes > 0 { format!("{}\n\n{EXACTLY}", delete_text(deletes)) } else { EXACTLY.to_owned() };
    format!("{lead}\n{}", list_text(ops, exposed))
}

/// Ctrl+Z / Ctrl+Y's question (`redo`: Ctrl+Y) with the list it sends.
pub fn undo_text(redo: bool, ops: &[Op], exposed: bool) -> String {
    format!("{}\n\n{}", if redo { REDO_ADMIN } else { UNDO_ADMIN }, list_text(ops, exposed))
}

/// Done items Gezik could not look at (`Report::unchecked`), for the Problems window.
pub fn unchecked_text(paths: &[PathBuf]) -> Option<String> {
    if paths.is_empty() {
        return None;
    }
    let lines: Vec<String> = paths.iter().map(|path| shown(path)).collect();
    Some(format!("{}:\n{}", gezik_ops::UNCHECKED, lines.join("\n")))
}

/// Whether something other than an administrator could swap Gezik's exe before the helper runs.
pub fn exposed() -> bool {
    gezik_platform::system::exe().map_or(true, |exe| gezik_platform::elevate::others_can_change(&exe))
}

fn exists(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

type Started = Box<dyn FnOnce(JobId)>;

/// Runs `wanted` as administrator after asking what must be asked; `started` gets the job.
/// Called only by the two buttons.
pub fn start(ops: &Operations, wanted: Vec<Op>, started: impl FnOnce(JobId) + 'static) {
    let started: Started = Box::new(started);
    let wanted: Vec<Op> =
        wanted.into_iter().map(|op| op.map_paths(|path| real_path(&path, |dir| std::fs::canonicalize(dir)))).collect();
    let count = taken(&wanted, &exists);
    if count == 0 {
        return confirm(ops, wanted, started);
    }
    let again = ops.clone();
    ops.dialogs().ask_escape(RETRY_AS_ADMIN, taken_text(count), TAKEN_BUTTONS, 0, move |choice| match choice {
        Some(1) => confirm(&again, replacing(wanted, &exists), started),
        Some(2) => confirm(&again, skipping(wanted, &exists), started),
        _ => {}
    });
}

/// Gezik's own check (what the helper would refuse is refused before anything is asked), then
/// the list in words; the job gets the very list the user said yes to.
fn confirm(ops: &Operations, wanted: Vec<Op>, started: Started) {
    if wanted.is_empty() {
        return;
    }
    let protected = gezik_platform::elevate::protected();
    let system = System { windows: cfg!(windows), macos: cfg!(target_os = "macos"), protected: &protected };
    if let Err((index, why)) = elevated::check(&wanted, &system) {
        return ops.status(refusal_text(&wanted[index], why));
    }
    let exe = gezik_platform::system::exe().ok().and_then(|exe| exe.to_str().map(str::to_owned)).unwrap_or_default();
    if !elevated::fits(&exe, &wanted, cfg!(windows)) {
        return ops.status(elevated::TOO_MANY.to_owned());
    }
    let (buttons, yes) = confirm_buttons(&wanted);
    let cancel = 1 - yes;
    let again = ops.clone();
    let text = confirm_text(&wanted, exposed());
    ops.dialogs().ask_escape(elevated::label(&wanted), text, buttons, cancel, move |choice| {
        if choice == Some(yes) {
            submit(&again, wanted, started);
        }
    });
}

fn submit(ops: &Operations, wanted: Vec<Op>, started: Started) {
    let label = elevated::label(&wanted);
    // The row's Retry asks again, with the same list.
    let again: Rc<dyn Fn()> = {
        let (ops, wanted) = (ops.clone(), wanted.clone());
        Rc::new(move || confirm(&ops, wanted.clone(), Box::new(|_| {})))
    };
    let id = ops.submit_chain(vec![Box::new(ElevatedTask::new(wanted))], Some(label), Some(again), After::Nothing);
    started(id);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(text: &str) -> PathBuf {
        PathBuf::from(text)
    }

    #[test]
    fn the_problems_window_offers_the_administrator_first() {
        assert_eq!(detail_buttons(true, true), [RETRY_AS_ADMIN, "Retry", "Close"]);
        assert_eq!(detail_buttons(true, false), [RETRY_AS_ADMIN, "Close"]);
        assert_eq!(detail_buttons(false, true), ["Retry", "Close"]);
        assert_eq!(detail_buttons(false, false), ["Close"]);
    }

    #[test]
    fn folders_are_resolved_before_the_prompt_the_item_is_not() {
        let mac = |dir: &Path| -> std::io::Result<PathBuf> {
            Ok(PathBuf::from(dir.to_str().unwrap().replacen("/etc", "/private/etc", 1)))
        };
        assert_eq!(real_path(&p("/etc/hosts"), mac), p("/private/etc/hosts"));
        let windows = |dir: &Path| -> std::io::Result<PathBuf> { Ok(PathBuf::from(format!(r"\\?\{}", dir.display()))) };
        assert_eq!(real_path(&p(r"C:\Program Files\x"), windows), p(r"C:\Program Files\x"), "the \\\\?\\ goes");
        let unc = |_: &Path| -> std::io::Result<PathBuf> { Ok(p(r"\\?\UNC\nas\share")) };
        assert_eq!(real_path(&p(r"Z:\x"), unc), p(r"\\?\UNC\nas\share\x"), "left for the check to refuse");
        let missing = |_: &Path| -> std::io::Result<PathBuf> { Err(std::io::ErrorKind::NotFound.into()) };
        assert_eq!(real_path(&p("/opt/new/x"), missing), p("/opt/new/x"));
    }

    #[test]
    fn conflicts_are_asked_once_for_all() {
        let ops = vec![
            Op::Copy { from: p("/a/1"), to: p("/d/1"), replace: false },
            Op::Copy { from: p("/a/2"), to: p("/d/2"), replace: false },
            Op::Move { from: p("/a/3"), to: p("/d/3"), replace: false },
            Op::Delete(p("/d/4")),
        ];
        let exists = |path: &Path| path == Path::new("/d/2") || path == Path::new("/d/3") || path == Path::new("/d/4");
        assert_eq!(taken(&ops, &exists), 2, "a delete's own path is no conflict");
        let replaced = replacing(ops.clone(), &exists);
        assert_eq!(replaced[0], ops[0]);
        assert_eq!(replaced[1], Op::Copy { from: p("/a/2"), to: p("/d/2"), replace: true });
        assert_eq!(replaced[2], Op::Move { from: p("/a/3"), to: p("/d/3"), replace: true });
        assert_eq!(skipping(ops.clone(), &exists), [ops[0].clone(), ops[3].clone()]);
        assert_eq!(taken_text(1), "1 item is already where it goes. Replace it as administrator?");
        assert_eq!(taken_text(3), "3 items are already where they go. Replace them as administrator?");
    }

    #[test]
    fn what_is_said_before_and_instead() {
        assert_eq!(delete_text(3), "This will delete 3 items permanently as administrator. It cannot be undone.");
        assert_eq!(delete_text(1), "This will delete 1 item permanently as administrator. It cannot be undone.");
        assert_eq!(
            refusal_text(&Op::Delete(p("/etc")), "A system folder"),
            format!("Not done as administrator: {} (A system folder)", p("/etc").display())
        );
    }

    #[test]
    fn the_confirmation_names_every_item_and_what_is_done_to_it() {
        let ops = vec![
            Op::Copy { from: p("/a/1"), to: p("/d/1"), replace: true },
            Op::Move { from: p("/a/2"), to: p("/d/2"), replace: false },
            Op::Delete(p("/d/3")),
            Op::Rename { path: p("/d/4"), name: "five".into() },
            Op::Mkdir(p("/d/6")),
            Op::Rmdir(p("/d/7")),
            Op::Chmod { path: p("/d/8"), mode: 0o644 },
            Op::Chown { path: p("/d/9"), uid: 501, gid: 20 },
            Op::Chflags { path: p("/d/10"), set: LOCKED, clear: HIDDEN },
        ];
        let d = |text: &str| p(text).display().to_string();
        let lines = [
            format!("Copy {} to {}, replacing files there", d("/a/1"), d("/d/1")),
            format!("Move {} to {}", d("/a/2"), d("/d/2")),
            format!("Delete {} permanently", d("/d/3")),
            format!("Rename {} to \"five\"", d("/d/4")),
            format!("Make the folder {}", d("/d/6")),
            format!("Remove the empty folder {}", d("/d/7")),
            format!("Set the permissions of {} to 644", d("/d/8")),
            format!("Set the owner of {} to user 501 and group 20", d("/d/9")),
            format!("Change the flags of {}: Hidden off, Locked on", d("/d/10")),
        ];
        let text = confirm_text(&ops, false);
        assert_eq!(text, format!("{}\n\n{EXACTLY}\n{}", delete_text(1), lines.join("\n")));
        assert!(!text.contains(gezik_platform::elevate::EXPOSED));
        let exposed = confirm_text(&ops[..1], true);
        assert_eq!(exposed, format!("{EXACTLY}\n{}\n\n{}", lines[0], gezik_platform::elevate::EXPOSED));
    }

    #[test]
    fn undo_and_redo_texts() {
        assert_eq!(UNDO_ADMIN, "Undoing this needs administrator rights.");
        assert_eq!(REDO_ADMIN, "Redoing this needs administrator rights.");
        let ops = [Op::Rmdir(p("/d/new"))];
        assert_eq!(undo_text(false, &ops, false), format!("{UNDO_ADMIN}\n\n{}", op_text(&ops[0])));
        assert!(undo_text(true, &ops, true).starts_with(REDO_ADMIN));
        assert!(undo_text(true, &ops, true).ends_with(gezik_platform::elevate::EXPOSED));
    }

    #[test]
    fn hidden_characters_are_shown_not_obeyed() {
        let op = Op::Rename { path: p("/d/report\u{202E}fdp.exe"), name: "a\u{200B}b\nc\u{FEFF}".into() };
        let text = op_text(&op);
        assert!(text.contains(r"report\u{202e}fdp.exe"), "{text}");
        assert!(text.ends_with(r#"to "a\u{200b}b\u{000a}c\u{feff}""#), "{text}");
        assert!(!text.chars().any(|c| c.is_control() || ('\u{2000}'..='\u{206F}').contains(&c) || c == '\u{FEFF}'));
        assert_eq!(escaped("çğ ü.txt"), "çğ ü.txt", "plain names stay");
        assert!(unchecked_text(&[p("/x/\u{2066}a")]).unwrap().contains(r"\u{2066}a"));
    }

    #[test]
    fn enter_never_picks_what_destroys() {
        assert_eq!(TAKEN_BUTTONS[0], "Cancel");
        let copy = Op::Copy { from: p("/a/1"), to: p("/d/1"), replace: false };
        assert_eq!(confirm_buttons(std::slice::from_ref(&copy)), (&["Continue", "Cancel"][..], 0));
        assert_eq!(confirm_buttons(&[copy, Op::Delete(p("/d/2"))]), (&["Cancel", "Delete"][..], 1));
        let replace = [Op::Move { from: p("/a/1"), to: p("/d/1"), replace: true }];
        assert_eq!(confirm_buttons(&replace), (&["Cancel", "Continue"][..], 1));
    }

    #[test]
    fn unchecked_items_are_named() {
        assert_eq!(unchecked_text(&[]), None);
        let text = unchecked_text(&[p("/x/a"), p("/x/b")]).unwrap();
        assert_eq!(text, format!("{}:\n{}\n{}", gezik_ops::UNCHECKED, p("/x/a").display(), p("/x/b").display()));
    }
}
