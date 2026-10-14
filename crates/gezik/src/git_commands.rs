//! Git commands (spec 16 §9): stage, unstage, commit, push, pull, fetch, switch branch, discard
//! changes and compare with HEAD, in the repository the folder shown is in. Each runs only when
//! chosen, with the system's git through `git.rs`'s safe path, in the pane's command panel
//! (16c): push, pull, fetch and commit open it and can be stopped; stage, unstage and switch
//! open it only if they fail. One command at a time in a repository (the others wait); its
//! status runs wait too, and every pane in it looks again once it ended.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

use slint::ComponentHandle;

use gezik_config::shortcuts::Action;
use gezik_core::git::GitMark;
use gezik_core::ops::paths::same_path;
use gezik_i18n::{t, tn, tx};
use gezik_ops::{Outcome, PlanItem, Resources, RunCx, ScanSink, Stage as When, Task, TaskKind, Work};

use crate::command_line::Process;
use crate::git::{Outcome as Ran, Trust};
use crate::panes::{Pane, PaneId};
use crate::view::View;

/// A path list longer than this (bytes) goes to git's input instead (spec 16 §9.1).
const INLINE_PATHS: usize = 32 * 1024;
/// The most branches Switch Branch lists.
pub const MAX_BRANCHES: usize = 50;
/// The most deleted names the status bar's menu lists.
const MAX_DELETED: usize = 20;

/// A git command with what it acts on (paths from the repository's root, with `/`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cmd {
    Stage(Vec<String>),
    Unstage(Vec<String>),
    /// Unstage before the first commit (no HEAD to restore from): `git rm --cached`.
    UnstageNew(Vec<String>),
    Commit {
        all: bool,
        message: String,
    },
    Push,
    Pull,
    Fetch,
    Switch(String),
}

impl Cmd {
    /// git's arguments, and what goes to its input.
    pub fn args(&self) -> (Vec<OsString>, Option<Vec<u8>>) {
        let words = |words: &[&str]| words.iter().map(OsString::from).collect::<Vec<_>>();
        match self {
            Cmd::Stage(paths) => with_paths(&["add"], paths),
            Cmd::Unstage(paths) => with_paths(&["restore", "--staged"], paths),
            Cmd::UnstageNew(paths) => with_paths(&["rm", "--cached", "-r", "-q"], paths),
            // Git's own cleanup of the message (no `--cleanup`): as `git commit -F`.
            Cmd::Commit { all: true, message } => (words(&["commit", "-a", "-F", "-"]), Some(message.clone().into())),
            Cmd::Commit { all: false, message } => (words(&["commit", "-F", "-"]), Some(message.clone().into())),
            Cmd::Push => (words(&["push"]), None),
            Cmd::Pull => (words(&["pull"]), None),
            Cmd::Fetch => (words(&["fetch"]), None),
            // A branch named `-x` is no option.
            Cmd::Switch(branch) => (words(&["switch", "--end-of-options", branch]), None),
        }
    }

    /// What the repository's config may do while it runs: commit and push run its hooks (spec
    /// 16 §9.2); only push, pull and fetch reach the network.
    pub fn trust(&self) -> Trust {
        match self {
            Cmd::Commit { .. } => Trust::Hooks,
            Cmd::Push => Trust::HooksNetwork,
            Cmd::Pull | Cmd::Fetch => Trust::Network,
            Cmd::Stage(_) | Cmd::Unstage(_) | Cmd::UnstageNew(_) | Cmd::Switch(_) => Trust::Local,
        }
    }

    /// The short ones open the panel only if they fail.
    fn quiet(&self) -> bool {
        matches!(self, Cmd::Stage(_) | Cmd::Unstage(_) | Cmd::UnstageNew(_) | Cmd::Switch(_))
    }

    /// What Open in Terminal puts on the clipboard: only for the commands that reach the
    /// network (the ones a password stops), a fixed line nothing of a repository goes into.
    fn copy(&self) -> Option<String> {
        match self {
            Cmd::Push => Some("git push".to_owned()),
            Cmd::Pull => Some("git pull".to_owned()),
            Cmd::Fetch => Some("git fetch".to_owned()),
            _ => None,
        }
    }

    /// The panel's title.
    fn title(&self) -> String {
        let (args, input) = self.args();
        let mut text = String::from("git");
        for arg in &args {
            text.push(' ');
            text.push_str(&arg.to_string_lossy());
            if text.len() > 200 {
                text.push('…');
                break;
            }
        }
        if input.is_some() && matches!(self, Cmd::Stage(_) | Cmd::Unstage(_) | Cmd::UnstageNew(_)) {
            text.push_str(" < …");
        }
        text
    }
}

/// `head`, then the paths after `--`, or through git's input past [`INLINE_PATHS`].
fn with_paths(head: &[&str], paths: &[String]) -> (Vec<OsString>, Option<Vec<u8>>) {
    let mut args: Vec<OsString> = head.iter().map(OsString::from).collect();
    let total: usize = paths.iter().map(|p| p.len() + 1).sum();
    if total > INLINE_PATHS {
        args.extend(["--pathspec-from-file=-", "--pathspec-file-nul"].map(OsString::from));
        let mut input = Vec::with_capacity(total);
        for path in paths {
            input.extend_from_slice(path.as_bytes());
            input.push(0);
        }
        return (args, Some(input));
    }
    args.push("--".into());
    args.extend(paths.iter().map(OsString::from));
    (args, None)
}

/// The lock file git's output says is in its way (`Unable to create '…/index.lock': File
/// exists`): a git that was ended may have left it.
pub fn lock_file(output: &str) -> Option<&str> {
    output.lines().find_map(|line| {
        let start = line.find("Unable to create '")? + "Unable to create '".len();
        let path = &line[start..];
        let path = &path[..path.find('\'')?];
        path.ends_with(".lock").then_some(path)
    })
}

/// Whether git's output says it needed a password or key it could not ask for (spec 16 §9.4).
pub fn auth_failed(output: &str) -> bool {
    const SIGNS: [&str; 6] = [
        "authentication failed",
        "permission denied (publickey",
        "could not read username",
        "could not read password",
        "terminal prompts disabled",
        "host key verification failed",
    ];
    let lower = output.to_lowercase();
    SIGNS.iter().any(|sign| lower.contains(sign))
}

/// The local branches from `git for-each-ref --format=%(HEAD)%(refname:lstrip=2)` (`*` before the
/// one checked out), at most [`MAX_BRANCHES`]; and which one is checked out.
pub fn parse_branches(out: &[u8]) -> (Vec<String>, Option<usize>) {
    let mut names = Vec::new();
    let mut current = None;
    for line in String::from_utf8_lossy(out).lines().take(MAX_BRANCHES) {
        let (mark, name) = line.split_at(line.chars().next().map_or(0, char::len_utf8));
        if name.is_empty() {
            continue;
        }
        if mark == "*" {
            current = Some(names.len());
        }
        names.push(name.to_owned());
    }
    (names, current)
}

/// What Discard Changes takes for `selected` (paths from the root): tracked files changed in
/// the work tree (`true`: git writes them anew) and untracked ones (`false`: only to the trash).
/// A folder stands for its changed files; staged changes and ignored files stay.
pub fn discard_targets(entries: &[(String, GitMark)], selected: &[String]) -> Vec<(String, bool)> {
    let mut out: Vec<(String, bool)> = Vec::new();
    let mut add = |path: &str, tracked: bool| {
        if !out.iter().any(|(p, _)| p == path) {
            out.push((path.to_owned(), tracked));
        }
    };
    let inside = |path: &str, dir: &str| dir.is_empty() || path.strip_prefix(dir).is_some_and(|r| r.starts_with('/'));
    for chosen in selected {
        for (path, mark) in entries {
            let tracked = match mark {
                GitMark::Modified | GitMark::StagedModified => true,
                GitMark::Untracked => false,
                _ => continue,
            };
            if path == chosen || inside(path, chosen) {
                add(path, tracked);
            } else if !tracked && inside(chosen, path) {
                // Inside an untracked folder: the item itself.
                add(chosen, false);
            }
        }
    }
    out
}

/// Which of the row menu's Git items apply to rows with `marks` (`(mark, is a folder)`):
/// stage, unstage, discard, compare.
pub fn applies(marks: &[(Option<GitMark>, bool)]) -> [bool; 4] {
    let any = |f: &dyn Fn(GitMark, bool) -> bool| marks.iter().any(|(m, dir)| m.is_some_and(|m| f(m, *dir)));
    let stage = any(&|m, dir| {
        dir || matches!(m, GitMark::Untracked | GitMark::Modified | GitMark::StagedModified | GitMark::Conflict)
    });
    let unstage = any(&|m, dir| {
        dir || matches!(m, GitMark::Staged | GitMark::Added | GitMark::Renamed | GitMark::StagedModified)
    });
    let discard = any(&|m, _| matches!(m, GitMark::Untracked | GitMark::Modified | GitMark::StagedModified));
    let compare = matches!(marks, [(Some(GitMark::Modified | GitMark::Staged | GitMark::StagedModified), false)]);
    [stage, unstage, discard, compare]
}

// Menu ids (spec 16 §11.3): Git 3250–3299 (3250, 3251 are 16d's), branches 3300–3349.
pub const GIT_STAGE: u32 = 3252;
pub const GIT_UNSTAGE: u32 = 3253;
pub const GIT_COMMIT: u32 = 3254;
pub const GIT_PUSH: u32 = 3255;
pub const GIT_PULL: u32 = 3256;
pub const GIT_FETCH: u32 = 3257;
pub const GIT_SWITCH: u32 = 3258;
pub const GIT_DISCARD: u32 = 3259;
pub const GIT_COMPARE: u32 = 3260;
pub const BRANCH_FIRST: u32 = 3300;

/// The action a Git menu item runs.
pub fn action_for(id: u32) -> Option<Action> {
    Some(match id {
        GIT_STAGE => Action::GitStage,
        GIT_UNSTAGE => Action::GitUnstage,
        GIT_COMMIT => Action::GitCommit,
        GIT_PUSH => Action::GitPush,
        GIT_PULL => Action::GitPull,
        GIT_FETCH => Action::GitFetch,
        GIT_SWITCH => Action::GitSwitchBranch,
        GIT_DISCARD => Action::GitDiscard,
        GIT_COMPARE => Action::GitCompareHead,
        _ => return None,
    })
}

/// The repository's own items (the status bar's menu, under the row's).
fn repo_items() -> Vec<(u32, String, bool)> {
    [
        (GIT_COMMIT, tx!("Commit…")),
        (GIT_PUSH, tx!("Push")),
        (GIT_PULL, tx!("Pull")),
        (GIT_FETCH, tx!("Fetch")),
        (GIT_SWITCH, tx!("Switch Branch…")),
    ]
    .into_iter()
    .map(|(id, title)| (id, gezik_i18n::t("", title).into_owned(), true))
    .collect()
}

/// A row menu's `Git ▸` for rows with `marks`.
pub fn row_items(marks: &[(Option<GitMark>, bool)]) -> Vec<(u32, String, bool)> {
    let [stage, unstage, discard, compare] = applies(marks);
    let mut items: Vec<(u32, String, bool)> = [
        (GIT_STAGE, tx!("Stage"), stage),
        (GIT_UNSTAGE, tx!("Unstage"), unstage),
        (GIT_DISCARD, tx!("Discard Changes…"), discard),
        (GIT_COMPARE, tx!("Compare with HEAD"), compare),
    ]
    .into_iter()
    .map(|(id, title, on)| (id, gezik_i18n::t("", title).into_owned(), on))
    .collect();
    items.extend(repo_items());
    items
}

/// The status bar's Git menu: the repository's commands, then the deleted files' names (they
/// have no row; spec 16 §8.3).
pub fn bar_items(deleted: &[String]) -> Vec<(u32, String, bool)> {
    let mut items = repo_items();
    if !deleted.is_empty() {
        // TRANSLATORS: a heading in the status bar's Git menu over the deleted files' names
        items.push((0, tn!("{n} deleted" | "{n} deleted", deleted.len()), false));
        items.extend(deleted.iter().take(MAX_DELETED).map(|name| (0, name.clone(), false)));
        if deleted.len() > MAX_DELETED {
            items.push((0, tn!("and {n} more" | "and {n} more", deleted.len() - MAX_DELETED), false));
        }
    }
    items
}

/// A command waiting or running.
struct Job {
    id: u64,
    pane: PaneId,
    root: PathBuf,
    cmd: Cmd,
    /// Told whether it succeeded (a commit's draft goes).
    after: Option<Box<dyn FnOnce(bool)>>,
}

thread_local! {
    static QUEUE: RefCell<VecDeque<Job>> = const { RefCell::new(VecDeque::new()) };
    /// Started (being prepared or running): their panes and repositories.
    static RUNNING: RefCell<Vec<Job>> = const { RefCell::new(Vec::new()) };
    static NEXT: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    /// The commit message typed, by repository, for this run of Gezik (spec 16 §9.2).
    static DRAFTS: RefCell<HashMap<PathBuf, String>> = RefCell::default();
}

/// The repositories a git command or a discard holds: one at a time in each (spec 16 §9.4).
/// Shared with the engine's thread, where a discard runs.
static HELD: std::sync::Mutex<Vec<PathBuf>> = std::sync::Mutex::new(Vec::new());

/// Whether a git command or a discard runs in the repository at `root` (its status waits).
pub fn busy(root: &Path) -> bool {
    gezik_config::lock(&HELD).iter().any(|r| same_path(r, root))
}

/// Holds the repository at `root` if nothing does; whether it did.
fn try_hold(root: &Path) -> bool {
    let mut held = gezik_config::lock(&HELD);
    let free = !held.iter().any(|r| same_path(r, root));
    if free {
        held.push(root.to_path_buf());
    }
    free
}

fn release(root: &Path) {
    let mut held = gezik_config::lock(&HELD);
    if let Some(at) = held.iter().position(|r| same_path(r, root)) {
        held.remove(at);
    }
}

/// Runs `cmd` in `root` from `pane`: at once, or after the command running in the repository
/// (or in the pane's panel). A command typed in the panel's address bar (`>`) is asked about.
fn submit(pane: &Pane, root: PathBuf, cmd: Cmd, after: Option<Box<dyn FnOnce(bool)>>) {
    let id = NEXT.with(|n| {
        n.set(n.get() + 1);
        n.get()
    });
    let job = Job { id, pane: pane.id, root, cmd, after };
    let ours = RUNNING.with(|r| r.borrow().iter().any(|j| j.pane == pane.id));
    if pane.command.is_running() && !ours {
        let command = pane.command.clone();
        let job = RefCell::new(Some(job));
        return crate::operations::with_current(|ops| {
            ops.dialogs().ask_escape(
                t!("Git"),
                t!("A command is still running. Stop it?"),
                &[&t!("Cancel"), &t!("Stop and Run")],
                0,
                move |choice| {
                    if choice == Some(1)
                        && let Some(job) = job.take()
                    {
                        command.stop();
                        QUEUE.with(|q| q.borrow_mut().push_back(job));
                        pump();
                    }
                },
            );
        });
    }
    QUEUE.with(|q| q.borrow_mut().push_back(job));
    pump();
}

/// Starts every waiting command whose repository and pane are free.
fn pump() {
    loop {
        let next = QUEUE.with(|q| {
            let mut queue = q.borrow_mut();
            let running: Vec<(PaneId, PathBuf)> =
                RUNNING.with(|r| r.borrow().iter().map(|j| (j.pane, j.root.clone())).collect());
            let free = |job: &Job| may_start(&running, job.pane, &job.root) && !busy(&job.root);
            let at = queue.iter().position(free)?;
            queue.remove(at)
        });
        let Some(job) = next else { return };
        if crate::panes::with_id(job.pane, |_| ()).is_none() {
            continue;
        }
        prepare(job);
    }
}

/// Whether a command of `pane` in `root` may start beside the `running` ones (pane, root): one
/// at a time in a repository (spec 16 §9.4) and in a panel.
fn may_start(running: &[(PaneId, PathBuf)], pane: PaneId, root: &Path) -> bool {
    running.iter().all(|(p, r)| *p != pane && !same_path(r, root))
}

/// `cmd` as it runs in `root`: Unstage before the first commit is `git rm --cached` (there is
/// no HEAD to restore from). Blocking.
fn resolved(cmd: Cmd, root: &Path) -> Cmd {
    match cmd {
        Cmd::Unstage(paths)
            if crate::git::read(&["rev-parse", "--verify", "-q", "HEAD"], root, Trust::Local).is_err() =>
        {
            Cmd::UnstageNew(paths)
        }
        cmd => cmd,
    }
}

/// Makes the command on a thread (16d's safe path looks at the repository's config first),
/// then runs it in the pane's panel.
fn prepare(job: Job) {
    let (id, root, cmd) = (job.id, job.root.clone(), job.cmd.clone());
    try_hold(&root);
    RUNNING.with(|r| r.borrow_mut().push(job));
    let window = crate::windows::current();
    let spawned = std::thread::Builder::new().name("gezik-git-command".into()).spawn(move || {
        let cmd = resolved(cmd, &root);
        let (args, input) = cmd.args();
        let made = crate::git::prepared(&args, &root, cmd.trust(), &AtomicBool::new(false));
        crate::windows::post(window, move || launch(id, made, input, cmd));
    });
    if spawned.is_err() {
        finished(id, false);
    }
}

fn launch(id: u64, made: Result<std::process::Command, Ran>, input: Option<Vec<u8>>, cmd: Cmd) {
    let Some((pane, root)) = RUNNING.with(|r| r.borrow().iter().find(|j| j.id == id).map(|j| (j.pane, j.root.clone())))
    else {
        return;
    };
    let command = match made {
        Ok(command) => command,
        Err(outcome) => {
            let note = problem(outcome);
            crate::panes::with_id(pane, |p| p.view.note(note));
            return finished(id, false);
        }
    };
    let again: Rc<dyn Fn()> = {
        let (root, cmd) = (root.clone(), cmd.clone());
        Rc::new(move || {
            crate::panes::with_id(pane, |p| submit(p, root.clone(), cmd.clone(), None));
        })
    };
    let process = Process {
        title: cmd.title(),
        dir: root,
        command,
        input,
        quiet: cmd.quiet(),
        copy: cmd.copy(),
        again: Some(again),
        on_end: Some(Box::new(move |ok, output| {
            if !ok && let Some(file) = lock_file(output) {
                // TRANSLATORS: {} is a lock file of git's (…/.git/index.lock)
                let note = t!("Git left {} behind: delete it only if no git is running", file);
                crate::panes::with_id(pane, |p| p.view.note(note));
            }
            finished(id, ok);
            !ok && auth_failed(output)
        })),
    };
    let ran = crate::panes::with_id(pane, |p| p.command.run_process(process));
    if ran.is_none() {
        finished(id, false);
    }
}

/// What the status bar says when git could not run.
fn problem(outcome: Ran) -> String {
    match outcome {
        Ran::Missing => gezik_core::git::missing_text(),
        // TRANSLATORS: {} is git's own message (not translated).
        Ran::Failed(why) => t!("Git: {}", why),
        _ => t!("Git took too long ([git] timeout)").into_owned(),
    }
}

/// Command `id` ended: the next one may start, and every pane in its repository looks again.
fn finished(id: u64, ok: bool) {
    let Some(job) = RUNNING.with(|r| {
        let mut running = r.borrow_mut();
        let at = running.iter().position(|j| j.id == id)?;
        Some(running.remove(at))
    }) else {
        return;
    };
    release(&job.root);
    if let Some(after) = job.after {
        after(ok);
    }
    refresh(&job.root);
    pump();
}

/// Every pane showing a folder of the repository at `root` looks again.
fn refresh(root: &Path) {
    for pane in crate::panes::everywhere() {
        crate::panes::with_id(pane.id, |p| {
            if p.view.git_repo().is_some_and(|repo| same_path(&repo.root, root)) {
                p.view.git_soon();
            }
        });
    }
}

/// The selected rows of `view` (the focused one when none is), if it shows a repository's
/// folder: the repository's root and the rows.
fn targets(view: &View) -> Option<(PathBuf, Vec<(PathBuf, bool)>)> {
    let Some(repo) = view.git_repo() else {
        view.note(t!("Not in a Git repository").into_owned());
        return None;
    };
    let mut rows = view.selected_items();
    if rows.is_empty() {
        rows.extend(view.focus().and_then(|i| view.entry_path(i)));
    }
    // The rows are in the folder shown: by its real path (links followed), as the root is.
    let rows = rows
        .into_iter()
        .filter_map(|(path, dir)| path.file_name().map(|name| (repo.real.join(name), dir)))
        .filter(|(path, _)| path.starts_with(&repo.root) && path != &repo.root)
        .collect();
    Some((repo.root, rows))
}

/// The marks of `rows` as the folder's last status gave them.
pub fn marks_of(view: &View, rows: &[(PathBuf, bool)]) -> Vec<(Option<GitMark>, bool)> {
    let shown = view.git_shown();
    rows.iter()
        .map(|(path, dir)| {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            (shown.as_ref().and_then(|g| g.marks.of(&name)), *dir)
        })
        .collect()
}

/// Runs a Git action in the active pane.
pub fn run(action: Action, view: &View) {
    let Some(pane) = crate::panes::with_id(view.pane_id(), Pane::clone) else { return };
    let Some((root, rows)) = targets(view) else { return };
    let rel = |rows: &[(PathBuf, bool)]| rows.iter().map(|(p, _)| crate::git::prefix(&root, p)).collect::<Vec<_>>();
    match action {
        Action::GitStage | Action::GitUnstage if rows.is_empty() => view.note(t!("Select files first").into_owned()),
        Action::GitStage => submit(&pane, root.clone(), Cmd::Stage(rel(&rows)), None),
        Action::GitUnstage => submit(&pane, root.clone(), Cmd::Unstage(rel(&rows)), None),
        Action::GitCommit => open_commit(&pane, root),
        Action::GitPush => submit(&pane, root, Cmd::Push, None),
        Action::GitPull => submit(&pane, root, Cmd::Pull, None),
        Action::GitFetch => submit(&pane, root, Cmd::Fetch, None),
        Action::GitSwitchBranch => branches(pane.id, root),
        Action::GitDiscard => discard(view, root, &rows),
        Action::GitCompareHead => compare_head(view, root, &rows),
        _ => {}
    }
}

/// Asks for the branch to switch to: the local branches come from git, then a menu.
fn branches(pane: PaneId, root: PathBuf) {
    let window = crate::windows::current();
    let _ = std::thread::Builder::new().name("gezik-git-branches".into()).spawn(move || {
        let count = format!("--count={MAX_BRANCHES}");
        let args =
            ["for-each-ref", &count, "--sort=-committerdate", "--format=%(HEAD)%(refname:lstrip=2)", "refs/heads/"];
        let out = crate::git::read(&args, &root, Trust::Local);
        crate::windows::post(window, move || match out {
            Ok(out) => {
                let (names, current) = parse_branches(&out);
                if names.is_empty() {
                    return crate::panes::active_view().note(t!("No branches yet").into_owned());
                }
                let at = BRANCH_MENU.with(std::cell::Cell::get);
                crate::context_menu::with_current(|m| m.branch_menu(pane, root, names, current, at));
            }
            // TRANSLATORS: {} is git's own message (not translated).
            Err(why) => crate::panes::active_view().note(t!("Git: {}", why)),
        });
    });
}

thread_local! {
    /// Where the branch menu opens: the status bar's Git part when it was clicked last.
    static BRANCH_MENU: std::cell::Cell<Option<crate::popup::Anchor>> = const { std::cell::Cell::new(None) };
}

/// The status bar's Git part was clicked (its box): its menu.
pub fn bar_menu(anchor: crate::popup::Anchor) {
    let view = crate::panes::active_view();
    let Some(_) = view.git_repo() else { return };
    BRANCH_MENU.with(|b| b.set(Some(anchor)));
    let deleted = view.git_shown().map(|g| g.marks.deleted.clone()).unwrap_or_default();
    crate::context_menu::with_current(|m| m.git_menu(bar_items(&deleted), anchor));
}

/// A branch was chosen in the branch menu.
pub fn switch_to(pane: PaneId, root: PathBuf, branch: String) {
    crate::panes::with_id(pane, |p| submit(p, root, Cmd::Switch(branch), None));
}

/// Commit…: the dialog with the staged files (asked of git first) and the draft.
fn open_commit(pane: &Pane, root: PathBuf) {
    let window = crate::windows::current();
    let id = pane.id;
    let _ = std::thread::Builder::new().name("gezik-git-staged".into()).spawn(move || {
        let names = |args: &[&str]| {
            crate::git::read(args, &root, Trust::Local).map(|out| {
                out.split(|b| *b == 0)
                    .filter(|f| !f.is_empty())
                    .map(|f| String::from_utf8_lossy(f).into_owned())
                    .collect::<Vec<String>>()
            })
        };
        // The staged files, and what "Stage all changes first" (`-a`) adds: tracked files
        // changed in the work tree.
        let staged = names(&["diff", "--cached", "--name-only", "--no-renames", "-z"]);
        let changed = names(&["diff", "--name-only", "--no-renames", "--no-ext-diff", "-z"]);
        let out = staged.and_then(|staged| changed.map(|changed| (staged, changed)));
        crate::windows::post(window, move || match out {
            Ok((staged, changed)) => {
                let mut all = staged.clone();
                all.extend(changed.into_iter().filter(|f| !staged.contains(f)));
                show_commit(id, root, staged, all);
            }
            Err(why) => crate::panes::with_id(id, |p| p.view.note(t!("Git: {}", why))).unwrap_or_default(),
        });
    });
}

thread_local! {
    /// The dialog's repository and pane while it is open.
    static COMMIT: RefCell<Option<(PaneId, PathBuf)>> = const { RefCell::new(None) };
}

fn show_commit(pane: PaneId, root: PathBuf, files: Vec<String>, all: Vec<String>) {
    let draft = DRAFTS.with(|d| d.borrow().get(&root).cloned().unwrap_or_default());
    let title = root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    crate::windows::with_window(crate::windows::current(), |w| {
        w.set_gc_files(files.join("\n").into());
        w.set_gc_all_files(all.join("\n").into());
        // TRANSLATORS: the commit dialog's title; {} is the repository's folder name
        w.set_gc_title(t!("Commit to {}", title).into());
        w.set_gc_message(draft.into());
        w.set_gc_all(false);
        w.set_gc_open(true);
    });
    COMMIT.with(|c| *c.borrow_mut() = Some((pane, root)));
}

/// The commit dialog's buttons, for every window (once).
pub fn connect(window: &crate::AppWindow) {
    let weak = window.as_weak();
    window.on_gc_chosen(move |commit| {
        let Some(w) = weak.upgrade() else { return };
        let (message, all) = (w.get_gc_message().to_string(), w.get_gc_all());
        let shown = if all { w.get_gc_all_files() } else { w.get_gc_files() };
        if commit && (message.is_empty() || shown.is_empty()) {
            return;
        }
        w.set_gc_open(false);
        w.invoke_focus_list();
        let Some((pane, root)) = COMMIT.with(|c| c.borrow_mut().take()) else { return };
        DRAFTS.with(|d| d.borrow_mut().insert(root.clone(), message.clone()));
        if !commit {
            return;
        }
        let after: Box<dyn FnOnce(bool)> = {
            let root = root.clone();
            Box::new(move |ok| {
                if ok {
                    DRAFTS.with(|d| d.borrow_mut().remove(&root));
                }
            })
        };
        crate::panes::with_id(pane, |p| submit(p, root, Cmd::Commit { all, message }, Some(after)));
    });
}

/// Discard Changes…: asks, then a job that trashes each file first and has git write the
/// tracked ones anew (spec 16 §9.3); one Ctrl+Z brings the files back.
fn discard(view: &View, root: PathBuf, rows: &[(PathBuf, bool)]) {
    let Some(shown) = view.git_shown() else {
        return view.note(t!("Git status is not known yet").into_owned());
    };
    let chosen: Vec<String> = rows.iter().map(|(p, _)| crate::git::prefix(&root, p)).collect();
    let targets = discard_targets(&shown.status.entries, &chosen);
    if targets.is_empty() {
        return view.note(t!("No changes to discard (staged changes stay: unstage them first)").into_owned());
    }
    // A changed folder that git tracks is a submodule: never discarded from here.
    let items: Vec<(PathBuf, bool)> = targets
        .iter()
        .map(|(p, tracked)| (from_git(&root, p), *tracked))
        .filter(|(path, tracked)| !(*tracked && path.is_dir()))
        .collect();
    if items.is_empty() {
        return view.note(t!("No changes to discard (staged changes stay: unstage them first)").into_owned());
    }
    let count = items.len();
    let message = tn!(
        "Discard changes to {n} file? It goes to the trash first, so you can undo."
            | "Discard changes to {n} files? They go to the trash first, so you can undo.",
        count
    );
    let label = tn!("Discard changes to {n} file" | "Discard changes to {n} files", count);
    crate::operations::with_current(|ops| {
        if !gezik_platform::fs::drive_facts(&root).is_ok_and(|facts| facts.trash) {
            return view.note(
                t!("This drive has no trash: changes are not discarded (they could not be undone)").into_owned(),
            );
        }
        ops.dialogs().ask(t!("Discard Changes"), message, &[&t!("Discard"), &t!("Cancel")], move |choice| {
            if choice == Some(0) {
                let task = DiscardTask::new(root, items, Some(crate::windows::current()));
                crate::operations::with_current(|ops| {
                    ops.submit_chain(vec![Box::new(task)], Some(label), None, crate::operations::After::Nothing);
                });
            }
        });
    });
}

/// Discard Changes as one job (spec 16 §9.3): each file goes to the trash, then a tracked one
/// is written anew by `git restore --worktree` (from the index: staged changes stay). Undo sees
/// a file replaced in place and brings the trashed one back over git's. It holds the
/// repository (no git command runs meanwhile), and looks at each file's status again first: one
/// that changed since the list was shown is left alone. An untracked folder gives only its
/// untracked files git does not ignore (no ignored file, no repository inside it).
struct DiscardTask {
    root: PathBuf,
    /// (path, tracked).
    items: Vec<(PathBuf, bool)>,
    held: AtomicBool,
    /// Where the panes look again once it ended.
    window: Option<crate::windows::WindowId>,
}

impl DiscardTask {
    fn new(root: PathBuf, items: Vec<(PathBuf, bool)>, window: Option<crate::windows::WindowId>) -> DiscardTask {
        DiscardTask { root, items, held: AtomicBool::new(false), window }
    }

    /// Waits until the repository is free and holds it (once for the job).
    fn hold(&self, cx: &RunCx<'_>) -> io::Result<()> {
        while !self.held.load(Ordering::Relaxed) {
            if try_hold(&self.root) {
                self.held.store(true, Ordering::Relaxed);
            } else if cx.stopped() {
                return Err(io::Error::new(io::ErrorKind::Interrupted, t!("cancelled")));
            } else {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
        Ok(())
    }

    /// Whether `rel` still has the change it was chosen for (`tracked`: changed in the work tree;
    /// else untracked).
    fn still_changed(&self, rel: &str, tracked: bool) -> io::Result<bool> {
        let args = [
            "--no-optional-locks",
            "status",
            "--porcelain=v2",
            "-z",
            "--untracked-files=normal",
            "--ignore-submodules=dirty",
            "--",
            rel,
        ];
        let out = crate::git::read(&args, &self.root, Trust::Local).map_err(io::Error::other)?;
        let status = gezik_core::git::parse_status(&out);
        let mark = status.entries.iter().find(|(path, _)| path == rel).map(|(_, mark)| *mark);
        Ok(match mark {
            Some(GitMark::Modified | GitMark::StagedModified) => tracked,
            Some(GitMark::Untracked) => !tracked,
            _ => false,
        })
    }

    /// An untracked folder's files to trash: untracked and not ignored, none in a repository
    /// inside it (git lists such a one as `dir/` and does not go in).
    fn untracked_files(&self, rel: &str) -> io::Result<Vec<String>> {
        let args = ["ls-files", "-o", "--exclude-standard", "-z", "--", rel];
        let out = crate::git::read(&args, &self.root, Trust::Local).map_err(io::Error::other)?;
        Ok(out
            .split(|b| *b == 0)
            .filter(|f| !f.is_empty() && !f.ends_with(b"/"))
            .map(|f| String::from_utf8_lossy(f).into_owned())
            .collect())
    }
}

/// A path git gives (from the root, with `/`) on this system.
fn from_git(root: &Path, rel: &str) -> PathBuf {
    rel.split('/').fold(root.to_path_buf(), |path, part| path.join(part))
}

/// Moves `path` to the trash, then (`tracked`) has git write it as the index has it. A restore
/// that fails puts the file back; if that fails too, the error says where it is in the trash.
fn discard_one(root: &Path, path: &Path, tracked: bool) -> io::Result<Outcome> {
    let trashed = gezik_ops::trash_path(path)?;
    let gone = Outcome::Trashed { original: path.to_path_buf(), trashed: trashed.clone() };
    if !tracked {
        return Ok(gone);
    }
    let rel = crate::git::prefix(root, path);
    match crate::git::read(&["restore", "--worktree", "--", &rel], root, Trust::Local) {
        Ok(_) => {
            let facts = gezik_ops::facts_after(path, false);
            Ok(Outcome::Several(vec![gone, Outcome::Created { path: path.to_path_buf(), facts, from: None }]))
        }
        Err(why) => match gezik_platform::fs::restore(&trashed, path) {
            Ok(()) => Err(io::Error::other(why)),
            Err(err) => Err(io::Error::other(t!(
                "{0}; the file is still in the trash at {1} ({2})",
                why,
                trashed.display(),
                err
            ))),
        },
    }
}

impl Task for DiscardTask {
    fn kind(&self) -> TaskKind {
        TaskKind::Command
    }

    fn title(&self) -> String {
        tn!("Discarding changes to {n} file" | "Discarding changes to {n} files", self.items.len())
    }

    fn count(&self) -> usize {
        self.items.len()
    }

    fn resources(&self) -> Resources {
        Resources { paths: vec![self.root.clone()], work: Work::Disk }
    }

    fn workers(&self) -> Option<usize> {
        // One git at a time (the index lock).
        Some(1)
    }

    fn plan(&self, sink: &mut dyn ScanSink) {
        for (root, (path, _)) in self.items.iter().enumerate() {
            match std::fs::symlink_metadata(path) {
                Ok(meta) => {
                    let item = PlanItem::new(When::Before, gezik_ops::walk::facts_of(&meta)).source(path).top(root);
                    if !sink.item(item) {
                        return;
                    }
                }
                Err(err) => sink.failed(path, err),
            }
        }
    }

    fn run(&self, item: &PlanItem, cx: &RunCx<'_>) -> io::Result<Outcome> {
        let Some(path) = &item.source else { return Ok(Outcome::Nothing) };
        let tracked = self.items.get(item.root).is_some_and(|(_, tracked)| *tracked);
        // Never a discard that cannot be undone.
        if !cx.has_trash(path) {
            return Err(gezik_ops::no_trash());
        }
        self.hold(cx)?;
        let rel = crate::git::prefix(&self.root, path);
        let dir = path.is_dir();
        if (tracked && dir) || !self.still_changed(&rel, tracked)? {
            cx.skip(path, &io::Error::other(t!("changed since: left as it is")));
            return Ok(Outcome::Nothing);
        }
        if !dir {
            return discard_one(&self.root, path, tracked);
        }
        let mut done = Vec::new();
        for file in self.untracked_files(&rel)? {
            if cx.stopped() {
                break;
            }
            let path = from_git(&self.root, &file);
            match discard_one(&self.root, &path, false) {
                Ok(outcome) => done.push(outcome),
                Err(err) => cx.fail(&path, &err),
            }
        }
        Ok(Outcome::Several(done))
    }

    fn done(&self, _cancelled: bool) {
        if self.held.swap(false, Ordering::Relaxed) {
            release(&self.root);
        }
        if let Some(window) = self.window {
            let root = self.root.clone();
            crate::windows::post(window, move || {
                refresh(&root);
                pump();
            });
        }
    }
}

/// Compare with HEAD: the file as HEAD has it (`git cat-file --filters`, with git's line ends)
/// in a temporary folder, beside the file, in step 11's compare (built in or the program set).
fn compare_head(view: &View, root: PathBuf, rows: &[(PathBuf, bool)]) {
    let [(path, false)] = rows else {
        return view.note(t!("Select one file to compare with HEAD").into_owned());
    };
    let path = path.clone();
    let rel = crate::git::prefix(&root, &path);
    let window = crate::windows::current();
    let _ = std::thread::Builder::new().name("gezik-git-head".into()).spawn(move || {
        let object = format!("HEAD:{rel}");
        let written = crate::git::read(&["cat-file", "--filters", &object], &root, Trust::Local).and_then(|bytes| {
            // shortcut: one folder a run of Gezik, emptied at each compare; a compare program
            // still showing the older file keeps its copy until then.
            let dir = std::env::temp_dir().join(format!("gezik-head-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            let name = path.file_name().map(|n| format!("HEAD - {}", n.to_string_lossy())).unwrap_or_default();
            let file = dir.join(name);
            std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&file, bytes)).map_err(|e| e.to_string())?;
            Ok(file)
        });
        crate::windows::post(window, move || match written {
            Ok(head) => crate::diff::with_current(|d| d.open(head, path)),
            Err(why) => crate::panes::active_view().note(t!("Git: {}", why)),
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(args: &[OsString]) -> Vec<String> {
        args.iter().map(|a| a.to_string_lossy().into_owned()).collect()
    }

    #[test]
    fn each_command_has_its_command_line() {
        let paths = vec!["a b.txt".to_owned(), "-n".to_owned(), "ğ/*.rs".to_owned()];
        let (args, input) = Cmd::Stage(paths.clone()).args();
        assert_eq!(strings(&args), ["add", "--", "a b.txt", "-n", "ğ/*.rs"]);
        assert_eq!(input, None);
        let (args, _) = Cmd::Unstage(paths).args();
        assert_eq!(strings(&args), ["restore", "--staged", "--", "a b.txt", "-n", "ğ/*.rs"]);
        assert_eq!(strings(&Cmd::Push.args().0), ["push"]);
        assert_eq!(strings(&Cmd::Pull.args().0), ["pull"]);
        assert_eq!(strings(&Cmd::Fetch.args().0), ["fetch"]);
        assert_eq!(strings(&Cmd::Switch("-dev".into()).args().0), ["switch", "--end-of-options", "-dev"]);
        assert_eq!(strings(&Cmd::UnstageNew(vec!["a".into()]).args().0), ["rm", "--cached", "-r", "-q", "--", "a"]);
        assert_eq!(Cmd::Push.copy().as_deref(), Some("git push"));
        assert_eq!(Cmd::Stage(vec!["a; rm -rf ~".into()]).copy(), None, "Open in Terminal: network commands only");
        assert_eq!(Cmd::Commit { all: false, message: "x".into() }.copy(), None);
        assert_eq!(Cmd::Push.trust(), Trust::HooksNetwork);
        assert_eq!(Cmd::Stage(Vec::new()).trust(), Trust::Local);
        assert!(Cmd::Switch("x".into()).quiet() && !Cmd::Fetch.quiet());
        assert_eq!(Cmd::Push.title(), "git push");
    }

    #[test]
    fn a_long_path_list_goes_to_the_input() {
        let paths: Vec<String> = (0..2_000).map(|i| format!("folder/a long file name {i:05}.txt")).collect();
        let (args, input) = Cmd::Stage(paths.clone()).args();
        assert_eq!(strings(&args), ["add", "--pathspec-from-file=-", "--pathspec-file-nul"]);
        let input = input.expect("the list");
        let read: Vec<&[u8]> = input.split(|b| *b == 0).filter(|p| !p.is_empty()).collect();
        assert_eq!(read.len(), 2_000);
        assert_eq!(read[0], paths[0].as_bytes());
        // Just under the limit: still on the command line.
        let short: Vec<String> = (0..100).map(|i| format!("f{i}")).collect();
        assert_eq!(Cmd::Unstage(short).args().1, None);
    }

    #[test]
    fn the_commit_message_goes_to_the_input_as_written() {
        let message = "Düzeltme: ğüş\n\n# not a comment to strip here\nikinci satır\n".to_owned();
        let (args, input) = Cmd::Commit { all: false, message: message.clone() }.args();
        assert_eq!(strings(&args), ["commit", "-F", "-"], "git's own cleanup, no --cleanup");
        assert_eq!(input.as_deref(), Some(message.as_bytes()));
        let (args, _) = Cmd::Commit { all: true, message }.args();
        assert_eq!(strings(&args), ["commit", "-a", "-F", "-"]);
        assert_eq!(Cmd::Commit { all: true, message: String::new() }.trust(), Trust::Hooks, "hooks run");
    }

    #[test]
    fn auth_failures_are_seen() {
        assert!(auth_failed("remote: Invalid username or password.\nfatal: Authentication failed for 'https://x'"));
        assert!(auth_failed("git@github.com: Permission denied (publickey).\nfatal: Could not read from remote"));
        assert!(auth_failed("fatal: could not read Username for 'https://github.com': terminal prompts disabled"));
        assert!(!auth_failed("error: failed to push some refs to 'origin'\nhint: Updates were rejected"));
    }

    #[test]
    fn branches_are_listed_with_the_current_one_and_at_most_fifty() {
        let (names, current) = parse_branches(" main\n*feature/ğ\n dev\n".as_bytes());
        assert_eq!(names, ["main", "feature/ğ", "dev"]);
        assert_eq!(current, Some(1));
        let many: String = (0..80).map(|i| format!(" b{i}\n")).collect();
        let (names, current) = parse_branches(many.as_bytes());
        assert_eq!((names.len(), current), (MAX_BRANCHES, None));
        assert_eq!(parse_branches(b"").0.len(), 0);
    }

    #[test]
    fn discard_takes_changed_and_untracked_files_only() {
        let entries = vec![
            ("src/a.rs".to_owned(), GitMark::Modified),
            ("src/b.rs".to_owned(), GitMark::Staged),
            ("src/c.rs".to_owned(), GitMark::StagedModified),
            ("src/new".to_owned(), GitMark::Untracked),
            ("src/build".to_owned(), GitMark::Ignored),
            ("srcx/d.rs".to_owned(), GitMark::Modified),
        ];
        let folder = discard_targets(&entries, &["src".to_owned()]);
        assert_eq!(
            folder,
            [("src/a.rs".to_owned(), true), ("src/c.rs".to_owned(), true), ("src/new".to_owned(), false)],
            "staged-only and ignored stay; srcx is another folder"
        );
        assert!(discard_targets(&entries, &["src/b.rs".to_owned()]).is_empty(), "staged: unstage first");
        assert_eq!(discard_targets(&entries, &["src/new/x.txt".to_owned()]), [("src/new/x.txt".to_owned(), false)]);
        assert_eq!(discard_targets(&entries, &["src/a.rs".to_owned(), "src/a.rs".to_owned()]).len(), 1);
    }

    #[test]
    fn one_command_at_a_time_in_a_repository_and_a_panel() {
        let (left, right) = (crate::panes::next_id(), crate::panes::next_id());
        let running = vec![(left, PathBuf::from("/r/one"))];
        assert!(!may_start(&running, right, Path::new("/r/one")), "the same repository waits");
        assert!(!may_start(&running, left, Path::new("/r/two")), "the same panel waits");
        assert!(may_start(&running, right, Path::new("/r/two")));
        assert!(may_start(&[], left, Path::new("/r/one")));
    }

    #[test]
    fn the_menu_offers_what_applies() {
        let modified = [(Some(GitMark::Modified), false)];
        assert_eq!(applies(&modified), [true, false, true, true]);
        assert_eq!(applies(&[(Some(GitMark::Staged), false)]), [false, true, false, true]);
        assert_eq!(applies(&[(None, false)]), [false; 4], "unchanged");
        assert_eq!(applies(&[(Some(GitMark::Untracked), false), (None, false)]), [true, false, true, false]);
        let items = bar_items(&(0..25).map(|i| format!("gone{i}.txt")).collect::<Vec<_>>());
        assert_eq!(items.iter().filter(|(id, _, on)| *id == 0 && !on).count(), 1 + MAX_DELETED + 1);
        assert!(items.iter().any(|(id, _, _)| *id == GIT_SWITCH));
        assert_eq!(action_for(GIT_COMPARE), Some(Action::GitCompareHead));
    }

    /// Runs `cmd` in `dir` as Gezik makes it, without the panel: whether it succeeded, its
    /// error output.
    fn run_cmd(cmd: &Cmd, dir: &Path) -> (bool, String) {
        use std::io::Write;
        use std::process::Stdio;
        let (args, input) = cmd.args();
        let mut command = crate::git::prepared(&args, dir, cmd.trust(), &AtomicBool::new(false)).unwrap();
        command.stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() });
        let mut child = command.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        if let (Some(mut pipe), Some(input)) = (child.stdin.take(), input) {
            pipe.write_all(&input).unwrap();
        }
        let out = child.wait_with_output().unwrap();
        (out.status.success(), String::from_utf8_lossy(&out.stderr).into_owned())
    }

    /// With the real git (none: nothing to check), all local: stage, commit, push to a bare
    /// repository and pull from it, no `ext::` transport, no repository filter on stage, and
    /// a discard that one undo takes back.
    #[test]
    fn real_git_commits_pushes_pulls_and_discards() {
        crate::git::set_settings(gezik_config::settings::GitSettings::default());
        let Ok(git) = crate::git::program() else { return };
        let base = std::env::temp_dir().join(format!("gezik-git16e-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        gezik_platform::trash::purge_at_thread_end(&base);
        let sh = |dir: &Path, args: &[&str]| {
            let out = std::process::Command::new(&git).args(args).current_dir(dir).output().unwrap();
            assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
            String::from_utf8_lossy(&out.stdout).into_owned()
        };
        let ok = |cmd: Cmd, dir: &Path| {
            let (ok, errors) = run_cmd(&cmd, dir);
            assert!(ok, "{cmd:?}: {errors}");
        };
        let (bare, a, b) = (base.join("remote.git"), base.join("a"), base.join("b"));
        std::fs::create_dir_all(&a).unwrap();
        sh(&base, &["init", "-q", "--bare", "-b", "main", "remote.git"]);
        sh(&a, &["init", "-q", "-b", "main"]);
        for (key, value) in [("user.name", "t"), ("user.email", "t@t"), ("core.autocrlf", "false")] {
            sh(&a, &["config", key, value]);
        }
        std::fs::write(a.join("x.txt"), "one").unwrap();
        ok(Cmd::Stage(vec!["x.txt".into()]), &a);
        ok(Cmd::Commit { all: false, message: "Bir ğ\n\nikinci satır\n".into() }, &a);
        assert_eq!(sh(&a, &["log", "-1", "--format=%B"]).trim_end(), "Bir ğ\n\nikinci satır");
        sh(&a, &["remote", "add", "origin", &bare.to_string_lossy()]);
        sh(&a, &["push", "-q", "-u", "origin", "main"]);
        sh(&base, &["clone", "-q", &bare.to_string_lossy(), "b"]);
        std::fs::write(a.join("x.txt"), "two").unwrap();
        ok(Cmd::Commit { all: true, message: "two".into() }, &a);
        ok(Cmd::Push, &a);
        ok(Cmd::Fetch, &b);
        ok(Cmd::Pull, &b);
        assert_eq!(std::fs::read_to_string(b.join("x.txt")).unwrap(), "two", "pulled");
        ok(Cmd::Switch("main".into()), &b);

        // A transport that runs a command is never used.
        let marker = base.join("ran");
        let touch = format!("ext::sh -c touch% {}", marker.display()).replace('\\', "/");
        sh(&a, &["remote", "add", "evil", &touch]);
        let fetch_evil =
            crate::git::prepared(&["fetch".into(), "evil".into()], &a, Trust::Network, &AtomicBool::new(false));
        let out = fetch_evil.unwrap().output().unwrap();
        assert!(!out.status.success() && !marker.exists(), "ext:: ran");
        // Nor a filter of the repository's own config on stage.
        let write_marker = format!("echo x > \"{}\"", marker.display()).replace('\\', "/");
        sh(&a, &["config", "filter.evil.clean", &write_marker]);
        std::fs::write(a.join(".git/info/attributes"), "* filter=evil\n").unwrap();
        std::fs::write(a.join("y.txt"), "y").unwrap();
        ok(Cmd::Stage(vec!["y.txt".into()]), &a);
        assert!(!marker.exists(), "the repository's clean filter ran");
        std::fs::remove_file(a.join(".git/info/attributes")).unwrap();
        ok(Cmd::Unstage(vec!["y.txt".into()]), &a);

        // Discard: the changed file goes to the trash and back to HEAD's, the new one to the
        // trash; one undo brings both back.
        std::fs::write(a.join("x.txt"), "dirty").unwrap();
        let engine = gezik_ops::Engine::new(gezik_ops::Settings::default(), || {});
        let task = DiscardTask::new(a.clone(), vec![(a.join("x.txt"), true), (a.join("y.txt"), false)], None);
        engine.submit(Box::new(task));
        assert!(engine.wait_idle(std::time::Duration::from_secs(120)));
        assert_eq!(std::fs::read_to_string(a.join("x.txt")).unwrap(), "two", "as HEAD has it");
        assert!(!a.join("y.txt").exists(), "untracked: trashed");
        assert!(engine.undo().is_some());
        assert!(engine.wait_idle(std::time::Duration::from_secs(120)));
        assert_eq!(std::fs::read_to_string(a.join("x.txt")).unwrap(), "dirty", "undone");
        assert_eq!(std::fs::read_to_string(a.join("y.txt")).unwrap(), "y");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A test folder of the real git's (none: `None`), with `sh` for plain git commands there
    /// (no Gezik safety: the control runs) and a marker command writing `marker`.
    struct Lab {
        git: PathBuf,
        base: PathBuf,
    }

    impl Lab {
        fn new(name: &str) -> Option<Lab> {
            crate::git::set_settings(gezik_config::settings::GitSettings::default());
            let git = crate::git::program().ok()?;
            let base = std::env::temp_dir().join(format!("gezik-16e-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&base);
            std::fs::create_dir_all(&base).unwrap();
            gezik_platform::trash::purge_at_thread_end(&base);
            Some(Lab { git, base })
        }

        fn sh(&self, dir: &Path, args: &[&str]) -> std::process::Output {
            let mut command = std::process::Command::new(&self.git);
            command.args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "core.autocrlf=false"]);
            command.args(args).current_dir(dir).output().unwrap()
        }

        fn ok(&self, dir: &Path, args: &[&str]) -> String {
            let out = self.sh(dir, args);
            assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
            String::from_utf8_lossy(&out.stdout).into_owned()
        }

        /// A shell command (git runs config commands with `sh`) that writes `marker`.
        fn writes(marker: &Path) -> String {
            format!("echo x > \"{}\"", marker.display()).replace('\\', "/")
        }

        /// A repository `name` with one commit of `f.txt`.
        fn repo(&self, name: &str) -> PathBuf {
            let dir = self.base.join(name);
            std::fs::create_dir_all(&dir).unwrap();
            self.ok(&dir, &["init", "-q", "-b", "main"]);
            for (key, value) in [("user.name", "t"), ("user.email", "t@t"), ("core.autocrlf", "false")] {
                self.ok(&dir, &["config", key, value]);
            }
            std::fs::write(dir.join("f.txt"), "1\n2\n3\n").unwrap();
            self.ok(&dir, &["add", "f.txt"]);
            self.ok(&dir, &["commit", "-q", "-m", "one"]);
            dir
        }

        /// A folder of hooks whose `name` hook writes `marker`.
        fn hooks(&self, name: &str, marker: &Path) -> PathBuf {
            let dir = self.base.join("hooks");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join(name), format!("#!/bin/sh\n{}\n", Lab::writes(marker))).unwrap();
            #[cfg(unix)]
            std::fs::set_permissions(dir.join(name), std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
            dir
        }
    }

    /// Switch and Compare with HEAD in a hostile repository: its smudge filter and its
    /// post-checkout hook (through its own `core.hooksPath`) run for plain git, never for Gezik.
    #[test]
    fn switch_and_head_run_no_smudge_filter_or_hook_of_the_repository() {
        let Some(lab) = Lab::new("switch") else { return };
        let r = lab.repo("r");
        lab.ok(&r, &["branch", "other"]);
        lab.ok(&r, &["switch", "-q", "other"]);
        std::fs::write(r.join("f.txt"), "other\n").unwrap();
        lab.ok(&r, &["commit", "-q", "-am", "other"]);
        lab.ok(&r, &["switch", "-q", "main"]);
        let (smudged, hooked) = (lab.base.join("smudged"), lab.base.join("hooked"));
        lab.ok(&r, &["config", "filter.evil.smudge", &Lab::writes(&smudged)]);
        std::fs::write(r.join(".git/info/attributes"), "* filter=evil\n").unwrap();
        let hooks = lab.hooks("post-checkout", &hooked);
        lab.ok(&r, &["config", "core.hooksPath", &hooks.to_string_lossy()]);
        // The control: plain git runs both.
        lab.sh(&r, &["switch", "-q", "other"]);
        assert!(smudged.exists() && hooked.exists(), "the hostile repository is not hostile");
        lab.sh(&r, &["switch", "-q", "main"]);
        let _ = (std::fs::remove_file(&smudged), std::fs::remove_file(&hooked));
        let (ok, errors) = run_cmd(&Cmd::Switch("other".into()), &r);
        assert!(ok, "{errors}");
        assert!(!smudged.exists(), "the repository's smudge filter ran on switch");
        assert!(!hooked.exists(), "the repository's post-checkout hook ran on switch");
        assert_eq!(lab.ok(&r, &["branch", "--show-current"]).trim(), "other");
        let head = crate::git::read(&["cat-file", "--filters", "HEAD:f.txt"], &r, Trust::Local).unwrap();
        assert_eq!(head, b"other\n");
        assert!(!smudged.exists(), "the smudge filter ran for Compare with HEAD");
    }

    /// Fetch from a `file` remote whose upload-pack the repository sets to a command, and from an
    /// ssh remote with the repository's own `core.sshCommand`: plain git runs them, Gezik
    /// fetches with git's own upload-pack and ssh.
    #[test]
    fn fetch_runs_no_upload_pack_or_ssh_command_of_the_repository() {
        let Some(lab) = Lab::new("fetch") else { return };
        let a = lab.repo("a");
        lab.ok(&lab.base, &["clone", "-q", &a.to_string_lossy(), "b"]);
        let b = lab.base.join("b");
        let marker = lab.base.join("ran");
        lab.ok(&b, &["config", "remote.origin.uploadpack", &Lab::writes(&marker)]);
        lab.sh(&b, &["fetch", "-q", "origin"]);
        assert!(marker.exists(), "control: plain git runs the upload-pack setting");
        std::fs::remove_file(&marker).unwrap();
        let (ok, errors) = run_cmd(&Cmd::Fetch, &b);
        assert!(ok, "git's own upload-pack: {errors}");
        assert!(!marker.exists(), "the repository's upload-pack ran");

        lab.ok(&b, &["remote", "add", "evilssh", "ssh://127.0.0.1:1/x.git"]);
        lab.ok(&b, &["config", "core.sshCommand", &Lab::writes(&marker)]);
        lab.sh(&b, &["fetch", "-q", "evilssh"]);
        assert!(marker.exists(), "control: plain git runs core.sshCommand");
        std::fs::remove_file(&marker).unwrap();
        let fetch =
            crate::git::prepared(&["fetch".into(), "evilssh".into()], &b, Trust::Network, &AtomicBool::new(false));
        let out = fetch.unwrap().stdin(std::process::Stdio::null()).output().unwrap();
        assert!(!out.status.success() && !marker.exists(), "the repository's ssh command ran");
    }

    /// Pull merging with the repository's own merge driver and post-merge hook: plain git runs
    /// both; Gezik merges with git's own three-way merge and runs no hook.
    #[test]
    fn pull_runs_no_merge_driver_or_hook_of_the_repository() {
        let Some(lab) = Lab::new("pull") else { return };
        let a = lab.repo("a");
        let (driver, hooked) = (lab.base.join("driver"), lab.base.join("hooked"));
        let hooks = lab.hooks("post-merge", &hooked);
        let mut clones = Vec::new();
        for name in ["b1", "b2"] {
            lab.ok(&lab.base, &["clone", "-q", &a.to_string_lossy(), name]);
            let b = lab.base.join(name);
            for (key, value) in [("user.name", "t"), ("user.email", "t@t"), ("pull.rebase", "false")] {
                lab.ok(&b, &["config", key, value]);
            }
            lab.ok(&b, &["config", "merge.evil.driver", &Lab::writes(&driver)]);
            lab.ok(&b, &["config", "core.hooksPath", &hooks.to_string_lossy()]);
            std::fs::write(b.join(".git/info/attributes"), "* merge=evil\n").unwrap();
            std::fs::write(b.join("f.txt"), "1\n2\nthree\n").unwrap();
            lab.ok(&b, &["commit", "-q", "-am", "mine"]);
            clones.push(b);
        }
        std::fs::write(a.join("f.txt"), "one\n2\n3\n").unwrap();
        lab.ok(&a, &["commit", "-q", "-am", "theirs"]);
        lab.sh(&clones[0], &["pull", "-q", "--no-edit"]);
        assert!(driver.exists() && hooked.exists(), "control: plain git runs the merge driver and the hook");
        let _ = (std::fs::remove_file(&driver), std::fs::remove_file(&hooked));
        let (ok, errors) = run_cmd(&Cmd::Pull, &clones[1]);
        assert!(ok, "{errors}");
        assert!(!driver.exists(), "the repository's merge driver ran on pull");
        assert!(!hooked.exists(), "a hook ran on pull");
        let merged = std::fs::read_to_string(clones[1].join("f.txt")).unwrap().replace("\r\n", "\n");
        assert_eq!(merged, "one\n2\nthree\n", "git's own merge");
    }

    /// A branch named like an option is switched to, listed by its whole name; Unstage works
    /// before the first commit.
    #[test]
    fn odd_branches_and_a_repository_without_commits() {
        let Some(lab) = Lab::new("odd") else { return };
        let r = lab.repo("r");
        lab.ok(&r, &["update-ref", "refs/heads/-evil", "HEAD"]);
        let out =
            crate::git::read(&["for-each-ref", "--format=%(HEAD)%(refname:lstrip=2)", "refs/heads/"], &r, Trust::Local)
                .unwrap();
        assert!(parse_branches(&out).0.contains(&"-evil".to_owned()));
        let (ok, errors) = run_cmd(&Cmd::Switch("-evil".into()), &r);
        assert!(ok, "{errors}");
        assert_eq!(lab.ok(&r, &["branch", "--show-current"]).trim(), "-evil");

        let fresh = lab.base.join("fresh");
        std::fs::create_dir_all(&fresh).unwrap();
        lab.ok(&fresh, &["init", "-q"]);
        std::fs::write(fresh.join("n.txt"), "n").unwrap();
        lab.ok(&fresh, &["add", "n.txt"]);
        let plain = Cmd::Unstage(vec!["n.txt".into()]);
        assert!(!run_cmd(&plain, &fresh).0, "control: restore --staged needs a HEAD");
        let (ok, errors) = run_cmd(&resolved(plain, &fresh), &fresh);
        assert!(ok, "{errors}");
        assert_eq!(lab.ok(&fresh, &["status", "--porcelain"]).trim(), "?? n.txt");
    }

    fn discard(root: &Path, items: Vec<(PathBuf, bool)>) -> gezik_ops::Engine {
        let engine = gezik_ops::Engine::new(gezik_ops::Settings::default(), || {});
        engine.submit(Box::new(DiscardTask::new(root.to_path_buf(), items, None)));
        engine
    }

    /// Discard of an untracked folder takes only its untracked, not ignored files (no ignored
    /// file, no repository inside it); a file whose state changed since is left alone; it waits
    /// while a git command holds the repository.
    #[test]
    fn discard_keeps_ignored_files_nested_repositories_and_changed_states() {
        let Some(lab) = Lab::new("discard") else { return };
        let r = lab.repo("r");
        std::fs::write(r.join(".gitignore"), "*.log\n").unwrap();
        lab.ok(&r, &["add", ".gitignore"]);
        lab.ok(&r, &["commit", "-q", "-m", "ignore"]);
        let new = r.join("new");
        std::fs::create_dir_all(new.join("deep")).unwrap();
        std::fs::write(new.join("a.txt"), "a").unwrap();
        std::fs::write(new.join("deep/b.txt"), "b").unwrap();
        std::fs::write(new.join("keep.log"), "log").unwrap();
        std::fs::create_dir_all(new.join("inner")).unwrap();
        lab.ok(&new.join("inner"), &["init", "-q"]);
        std::fs::write(new.join("inner/precious.txt"), "p").unwrap();
        let engine = discard(&r, vec![(new.clone(), false)]);
        assert!(engine.wait_idle(std::time::Duration::from_secs(120)));
        assert!(!new.join("a.txt").exists() && !new.join("deep/b.txt").exists(), "untracked files: trashed");
        assert!(new.join("keep.log").exists(), "an ignored file went to the trash");
        assert!(new.join("inner/precious.txt").exists() && new.join("inner/.git").exists(), "a nested repository");

        // Chosen as untracked, staged since: left alone.
        std::fs::write(r.join("s.txt"), "s").unwrap();
        lab.ok(&r, &["add", "s.txt"]);
        let engine = discard(&r, vec![(r.join("s.txt"), false)]);
        assert!(engine.wait_idle(std::time::Duration::from_secs(120)));
        assert!(r.join("s.txt").exists(), "a file staged since was discarded");

        // A command holds the repository: the discard waits for it.
        std::fs::write(r.join("f.txt"), "dirty\n").unwrap();
        assert!(try_hold(&r));
        let engine = discard(&r, vec![(r.join("f.txt"), true)]);
        assert!(!engine.wait_idle(std::time::Duration::from_millis(600)), "it ran beside a git command");
        assert_eq!(std::fs::read_to_string(r.join("f.txt")).unwrap(), "dirty\n");
        release(&r);
        assert!(engine.wait_idle(std::time::Duration::from_secs(120)));
        assert_eq!(std::fs::read_to_string(r.join("f.txt")).unwrap(), "1\n2\n3\n");
        assert!(!busy(&r), "the discard let the repository go");
    }

    #[test]
    fn a_left_lock_file_is_named() {
        let out = "fatal: Unable to create 'C:/w/r/.git/index.lock': File exists.\n\nAnother git process seems";
        assert_eq!(lock_file(out), Some("C:/w/r/.git/index.lock"));
        assert_eq!(lock_file("error: failed to push some refs"), None);
    }
}
