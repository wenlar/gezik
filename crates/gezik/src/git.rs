//! Git status (spec 16 §8) with the system's `git`: the repository a folder is in (a `.git` look
//! on the loading thread, nothing more outside repositories), the program, one cancellable
//! `git status` for the folder shown, and a watch on the repository's `.git` folder.
//!
//! A repository's own config must never run code through git (one downloaded with its `.git`).
//! Every run gets its config through `GIT_CONFIG_COUNT`/`GIT_CONFIG_KEY_n`/`GIT_CONFIG_VALUE_n`
//! (git ≥ 2.31; a key is never split at an `=` there, as `-c key=value` is):
//! `core.fsmonitor=false`, the repository's own filter drivers emptied (a clean filter runs when
//! a file's stat changed), and no transport at all (`GIT_ALLOW_PROTOCOL` names none, which no
//! config can widen; `GIT_NO_LAZY_FETCH` too). Submodules are looked at only by their commit
//! (no git runs inside them). Status runs no hooks, and with `--no-optional-locks` writes
//! nothing (no `post-index-change`). Anything unexpected (an old git, a config git cannot read,
//! a driver name it cannot pass safely) means no status, never a status with filters on.

use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use gezik_config::lock;
use gezik_config::settings::GitSettings;
use gezik_core::git::GitStatus;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

/// The oldest git used: `GIT_CONFIG_COUNT` came in 2.31 (`config --show-scope` in 2.26).
pub const MIN_VERSION: (u32, u32) = (2, 31);
/// A git not found is looked for again after this long (on the next run that wants one).
const MISSING_AGAIN: Duration = Duration::from_secs(300);
/// The most of a `.git` file or `commondir` read (a `gitdir:` line).
const SMALL_FILE: u64 = 4096;

static SETTINGS: LazyLock<Mutex<GitSettings>> = LazyLock::new(Mutex::default);

/// The git found for `[git] program` (the setting it was found for, when): looked for on the
/// first run's thread, again after a settings change, and after `MISSING_AGAIN` when none was.
struct Found {
    setting: String,
    when: Instant,
    /// The program, and whether its version will do.
    git: Option<(PathBuf, bool)>,
}

static PROGRAM: Mutex<Option<Found>> = Mutex::new(None);

/// settings.toml changed; whether `[git]` did.
pub fn set_settings(settings: GitSettings) -> bool {
    let mut now = lock(&SETTINGS);
    let changed = *now != settings;
    *now = settings;
    changed
}

pub fn settings() -> GitSettings {
    lock(&SETTINGS).clone()
}

/// A repository a folder is in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    /// Its work tree's top folder (where `.git` is).
    pub root: PathBuf,
    /// HEAD's and the index's folder: `.git`, or the one a `.git` file names (a worktree, a
    /// submodule).
    pub git_dir: PathBuf,
    /// The refs' folder's parent: a linked worktree's common folder, else `git_dir`.
    pub common: PathBuf,
    /// The folder shown as git sees it (links and junctions resolved): git runs there, and
    /// its place under `root` is this one's.
    pub real: PathBuf,
}

/// The repository `folder` is in, if `[git] status` is on: `.git` (a folder with a `HEAD`, or a
/// file naming one) looked for in it and the folders above, up to the home folder (when under
/// it) or the drive's root, on the folder's real path (git resolves links and junctions too).
/// Not on a network drive unless `[git] network`, nor inside a `.git` folder. Runs on the
/// loading thread; outside a repository it costs one look a level.
pub fn find_repo(folder: &Path) -> Option<Repo> {
    let settings = settings();
    if !settings.status || (!settings.network && gezik_platform::fs::is_network(folder).unwrap_or(true)) {
        return None;
    }
    let real = plain(std::fs::canonicalize(folder).ok()?);
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(|home| std::fs::canonicalize(&home).map(plain).unwrap_or_else(|_| PathBuf::from(home)));
    let kind = |p: &Path| std::fs::symlink_metadata(p).ok().map(|m| m.is_dir());
    find_in(&real, home.as_deref(), &kind, &read_small)
}

/// `canonicalize`'s `\\?\C:\…` as `C:\…` (and `\\?\UNC\…` as `\\…`): the spelling git and the
/// rest of Gezik use.
fn plain(path: PathBuf) -> PathBuf {
    let Some(text) = path.to_str() else { return path };
    match text.strip_prefix(r"\\?\") {
        Some(rest) if rest.starts_with("UNC\\") => PathBuf::from(format!(r"\\{}", &rest[4..])),
        Some(rest) => PathBuf::from(rest),
        None => path,
    }
}

/// A small file's text: only a plain file (a FIFO would block the loading thread), at most
/// `SMALL_FILE` bytes.
fn read_small(path: &Path) -> Option<String> {
    if !std::fs::metadata(path).ok()?.is_file() {
        return None;
    }
    let mut text = String::new();
    std::fs::File::open(path).ok()?.take(SMALL_FILE).read_to_string(&mut text).ok()?;
    Some(text)
}

/// Whether `name` is `.git` as the file system compares names (any case on Windows and macOS).
fn is_dot_git(name: &OsStr) -> bool {
    match name.to_str() {
        Some(name) if cfg!(any(windows, target_os = "macos")) => name.eq_ignore_ascii_case(".git"),
        Some(name) => name == ".git",
        None => false,
    }
}

/// `find_repo`'s look in `folder` (already real): `kind` says whether a path is a folder
/// (`Some(true)`), something else, or not there; `read` a small file's text.
fn find_in(
    folder: &Path,
    home: Option<&Path>,
    kind: &dyn Fn(&Path) -> Option<bool>,
    read: &dyn Fn(&Path) -> Option<String>,
) -> Option<Repo> {
    if folder.components().any(|c| is_dot_git(c.as_os_str())) {
        return None;
    }
    for dir in folder.ancestors() {
        let dot = dir.join(".git");
        let git_dir = match kind(&dot) {
            Some(true) => Some(dot),
            // `gitdir: <path>`, from the folder it is in.
            Some(false) => read(&dot)
                .and_then(|text| {
                    text.lines().next().and_then(|l| l.strip_prefix("gitdir:")).map(str::trim).map(PathBuf::from)
                })
                .map(|to| dir.join(to)),
            None => None,
        };
        // An empty or broken `.git` (no HEAD file) is no repository.
        if let Some(git_dir) = git_dir.filter(|g| kind(&g.join("HEAD")) == Some(false)) {
            let common = read(&git_dir.join("commondir"))
                .map(|text| git_dir.join(text.trim()))
                .unwrap_or_else(|| git_dir.clone());
            return Some(Repo { root: dir.to_path_buf(), git_dir, common, real: folder.to_path_buf() });
        }
        if home.is_some_and(|home| gezik_core::ops::paths::same_path(dir, home)) {
            break;
        }
    }
    None
}

/// The git to run (`pick_program`), if its version will do: `Missing` without one, `Failed`
/// for one older than `MIN_VERSION` (or one that does not say). Blocking: a run's thread.
pub fn program() -> Result<PathBuf, Outcome> {
    let setting = settings().program;
    let cached = lock(&PROGRAM)
        .as_ref()
        .filter(|f| f.setting == setting && (f.git.is_some() || f.when.elapsed() < MISSING_AGAIN))
        .map(|f| f.git.clone());
    let git = match cached {
        Some(git) => git,
        None => {
            let xcode = || {
                std::process::Command::new("/usr/bin/xcode-select")
                    .arg("-p")
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .is_ok_and(|s| s.success())
            };
            let macos = cfg!(target_os = "macos");
            let found = pick_program(&setting, std::env::var_os("PATH"), &|p| p.is_file(), &xcode, macos);
            let git = found.map(|program| {
                let ok = version(&program).is_some_and(|v| v >= MIN_VERSION);
                (program, ok)
            });
            *lock(&PROGRAM) = Some(Found { setting, when: Instant::now(), git: git.clone() });
            git
        }
    };
    match git {
        None => Err(Outcome::Missing),
        Some((program, true)) => Ok(program),
        Some(_) => Err(Outcome::Failed(format!("Git {}.{} or newer is needed", MIN_VERSION.0, MIN_VERSION.1))),
    }
}

/// `git version`'s major and minor number (`git version 2.47.0.windows.2`).
fn version(program: &Path) -> Option<(u32, u32)> {
    let out = command(program, &[OsString::from("version")], &std::env::temp_dir(), &[]).output().ok()?;
    parse_version(&String::from_utf8_lossy(&out.stdout))
}

fn parse_version(text: &str) -> Option<(u32, u32)> {
    let mut numbers = text.trim().strip_prefix("git version ")?.split('.');
    Some((numbers.next()?.parse().ok()?, numbers.next()?.trim_end_matches(|c: char| !c.is_ascii_digit()).parse().ok()?))
}

/// `program`'s choice. Only absolute paths count: git is never started from the folder shown
/// (Windows would try a `git.exe` there). A setting that is a bare name is looked for on PATH.
fn pick_program(
    setting: &str,
    path: Option<OsString>,
    exists: &dyn Fn(&Path) -> bool,
    xcode: &dyn Fn() -> bool,
    macos: bool,
) -> Option<PathBuf> {
    let on_path = |name: &str, stub_ok: &mut Option<bool>| {
        path.iter().flat_map(std::env::split_paths).filter(|dir| dir.is_absolute()).map(|dir| dir.join(name)).find(
            |git| exists(git) && (!macos || git != Path::new("/usr/bin/git") || *stub_ok.get_or_insert_with(xcode)),
        )
    };
    let mut stub_ok = None;
    match setting {
        "" => {}
        s if Path::new(s).is_absolute() => return Some(PathBuf::from(s)).filter(|p| exists(p)),
        // A bare name: on PATH; a relative path (`bin/git`) would be the folder shown's.
        s if Path::new(s).components().count() == 1 => return on_path(s, &mut stub_ok),
        _ => return None,
    }
    let name = if cfg!(windows) { "git.exe" } else { "git" };
    let brew = ["/opt/homebrew/bin/git", "/usr/local/bin/git"].into_iter().map(PathBuf::from);
    on_path(name, &mut stub_ok).or_else(|| brew.filter(|_| macos).find(|git| exists(git)))
}

/// How a status ended.
#[derive(Debug, Clone)]
pub enum Outcome {
    Done(GitStatus),
    /// No git (said once).
    Missing,
    /// Git said no (its first error line: not a repository Git trusts, a broken one), or Gezik
    /// would not run it (too old, a config it could not make safe).
    Failed(String),
    /// Past `[git] timeout`: stopped.
    TimedOut,
    /// Stopped (another folder, a newer run, the pane closed).
    Stopped,
}

/// The config of every run, before the filter drivers.
const SAFE: [(&str, &str); 3] = [("core.fsmonitor", "false"), ("core.quotepath", "off"), ("protocol.allow", "never")];

/// Every git Gezik runs, with `config` as command-line config (`GIT_CONFIG_COUNT`, which never
/// splits a key at `=`): no prompt, no transport whatever the repository's config says
/// (`GIT_ALLOW_PROTOCOL` lists none), nothing fetched on demand, English messages, nothing of a
/// repository Gezik did not ask for.
fn command(program: &Path, args: &[OsString], dir: &Path, config: &[(String, String)]) -> std::process::Command {
    let mut command = std::process::Command::new(program);
    command
        .args(args)
        .current_dir(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ALLOW_PROTOCOL", "none")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("LC_ALL", "C")
        .env("GIT_CONFIG_COUNT", config.len().to_string());
    for (i, (key, value)) in config.iter().enumerate() {
        command.env(format!("GIT_CONFIG_KEY_{i}"), key).env(format!("GIT_CONFIG_VALUE_{i}"), value);
    }
    for name in ["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_OBJECT_DIRECTORY", "GIT_COMMON_DIR"] {
        command.env_remove(name);
    }
    command
}

fn safe_config() -> Vec<(String, String)> {
    SAFE.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect()
}

/// One status of the folder `dir` (spec 16 §8.2): first the repository's own filter drivers
/// (`git config --show-scope`, local or worktree), emptied for the status; ended when `stop` is
/// set or after `timeout`.
pub fn status(dir: &Path, ignored: bool, timeout: Duration, stop: &AtomicBool) -> Outcome {
    let program = match program() {
        Ok(program) => program,
        Err(outcome) => return outcome,
    };
    let deadline = Instant::now() + timeout;
    let mut config = safe_config();
    let listing = ["config", "-z", "--show-scope", "--name-only", "--get-regexp", r"^filter\..*\.(clean|process)$"]
        .map(OsString::from);
    let drivers = match run(&program, &listing, dir, &config, deadline, stop) {
        Ok((code, out, errors)) => drivers_from(code.code(), &out, &errors),
        Err(outcome) => return outcome,
    };
    let drivers = match drivers {
        Ok(drivers) => drivers,
        Err(why) => return Outcome::Failed(why),
    };
    for driver in drivers {
        for key in ["clean", "process"] {
            config.push((format!("filter.{driver}.{key}"), String::new()));
        }
    }
    let args = [
        "--no-optional-locks",
        "status",
        "--porcelain=v2",
        "-z",
        "--branch",
        "--untracked-files=normal",
        "--ignore-submodules=dirty",
        if ignored { "--ignored=matching" } else { "--ignored=no" },
        "--",
        ".",
    ]
    .map(OsString::from);
    match run(&program, &args, dir, &config, deadline, stop) {
        Ok((code, out, _)) if code.success() => Outcome::Done(gezik_core::git::parse_status(&out)),
        Ok((_, _, errors)) => Outcome::Failed(first_line(&errors)),
        Err(outcome) => outcome,
    }
}

/// The filter drivers from the `config` run's exit `code` and output. Exit 1: none set.
/// Anything else (129: an option git does not know, 128: a config it cannot read) leaves the
/// filters unknown: no status.
fn drivers_from(code: Option<i32>, out: &[u8], errors: &str) -> Result<Vec<String>, String> {
    match code {
        Some(0 | 1) => local_drivers(out),
        _ => Err(format!("git too old or config unreadable: {}", first_line(errors))),
    }
}

/// Git's first error line, without `fatal: `.
fn first_line(errors: &str) -> String {
    let line = errors.lines().map(|l| l.trim_start_matches("fatal: ")).find(|l| !l.trim().is_empty());
    line.unwrap_or("git status failed").to_owned()
}

/// The filter drivers the repository's own config (`local`, `worktree`) sets, from
/// `config -z --show-scope --name-only` (`scope\0name\0…`). A name with `=` or a line break is
/// refused (no status): nothing that strange is passed on, however it is quoted.
fn local_drivers(out: &[u8]) -> Result<Vec<String>, String> {
    let mut words = out.split(|b| *b == 0).map(String::from_utf8_lossy);
    let mut drivers = Vec::new();
    while let (Some(scope), Some(name)) = (words.next(), words.next()) {
        let driver = name.strip_prefix("filter.").and_then(|n| n.rsplit_once('.')).map(|(d, _)| d.to_owned());
        if !matches!(&*scope, "local" | "worktree") {
            continue;
        }
        let Some(driver) = driver else { continue };
        if driver.contains(['=', '\n', '\r']) {
            return Err(format!(
                "the repository's filter \"{}\" has a name Gezik does not pass to git",
                driver.escape_debug()
            ));
        }
        if !drivers.contains(&driver) {
            drivers.push(driver);
        }
    }
    Ok(drivers)
}

/// Runs `program` with `args` and `config` in `dir`, its output and error output read
/// meanwhile; `Err` when it could not start, was stopped or ran past `deadline`.
fn run(
    program: &Path,
    args: &[OsString],
    dir: &Path,
    config: &[(String, String)],
    deadline: Instant,
    stop: &AtomicBool,
) -> Result<(std::process::ExitStatus, Vec<u8>, String), Outcome> {
    run_command(command(program, args, dir, config), deadline, stop)
}

/// `run` with the command made.
fn run_command(
    command: std::process::Command,
    deadline: Instant,
    stop: &AtomicBool,
) -> Result<(std::process::ExitStatus, Vec<u8>, String), Outcome> {
    let (mut child, mut stdout, mut stderr) =
        gezik_platform::ChildProcess::spawn_piped(command).map_err(|e| Outcome::Failed(e.to_string()))?;
    let out = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout.read_to_end(&mut bytes);
        bytes
    });
    let errors = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stderr.by_ref().take(64 * 1024).read_to_end(&mut bytes);
        // The rest, unread, would hold git up.
        let _ = std::io::copy(&mut stderr, &mut std::io::sink());
        String::from_utf8_lossy(&bytes).into_owned()
    });
    let ended = child.wait_or_stop(|| stop.load(Ordering::Relaxed) || Instant::now() >= deadline);
    let (out, errors) = (out.join().unwrap_or_default(), errors.join().unwrap_or_default());
    match ended {
        Ok(Some(code)) => Ok((code, out, errors)),
        _ if stop.load(Ordering::Relaxed) => Err(Outcome::Stopped),
        Ok(None) => Err(Outcome::TimedOut),
        Err(err) => Err(Outcome::Failed(err.to_string())),
    }
}

/// Watches `repo`'s HEAD, index and refs (its `.git` folder, `refs/` with what is under it);
/// `on_change` is told on the watcher's thread. Lock files, objects and logs do not count.
pub fn watch(repo: &Repo, on_change: impl Fn() + Send + 'static) -> Option<RecommendedWatcher> {
    let roots = [repo.git_dir.clone(), repo.common.clone()];
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
        if event.map_or(true, |event| concerns(&event, &roots)) {
            on_change();
        }
    })
    .ok()?;
    watcher.watch(&repo.git_dir, RecursiveMode::NonRecursive).ok()?;
    // Without it a branch moved by another worktree or a push (refs/remotes) goes unseen.
    let _ = watcher.watch(&repo.common.join("refs"), RecursiveMode::Recursive);
    Some(watcher)
}

/// Whether `event` may change the status: not a read, not only lock files, objects or logs
/// (judged inside the git folders `roots`, never by the folders above them).
fn concerns(event: &Event, roots: &[PathBuf]) -> bool {
    if matches!(event.kind, EventKind::Access(_)) {
        return false;
    }
    let counts = |path: &PathBuf| {
        let inner = roots.iter().find_map(|root| path.strip_prefix(root).ok()).unwrap_or(path);
        let lock = path.extension() == Some(OsStr::new("lock"));
        let inside = inner.components().any(|c| matches!(c.as_os_str().to_str(), Some("objects" | "logs")));
        !lock && !inside
    };
    event.paths.is_empty() || event.paths.iter().any(counts)
}

/// The folder `dir` from the repository's root `root`, with `/` ("" for the root).
pub fn prefix(root: &Path, dir: &Path) -> String {
    let rest = dir.strip_prefix(root).unwrap_or(Path::new(""));
    rest.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/")
}

/// What a git call lets the repository's own config do (spec 16 §9.4). Every call keeps the
/// ground above: no prompt, git by its full path (2.31 or newer), the environment's repository
/// redirections removed, fsmonitor off, settings passed in the environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trust {
    /// Gezik's own reads and the local commands (stage, unstage, switch, restore): as a status,
    /// the repository's own filter drivers emptied and no transport.
    Local,
    /// fetch, pull: as `Local`, with transport by [`PROTOCOLS`] only.
    Network,
    /// commit: its hooks and filters run as git runs them (spec 16 §9.2); no transport.
    Hooks,
    /// push: its hooks run, transport by [`PROTOCOLS`] only.
    HooksNetwork,
}

/// The protocols push, pull and fetch may use: never `ext::` (a command line) or another one.
pub const PROTOCOLS: &str = "http:https:ssh:git:file";

/// The repository settings that name a program git runs (spec 16 §9.4): filter drivers, merge
/// and diff drivers, ssh and proxy commands, credential helpers, a remote's upload-pack and
/// receive-pack (a `file` remote runs them here). Keys as git writes them (section and key in
/// lower case).
const PROGRAM_KEYS: &str = r"^(filter\..*\.(clean|process|smudge)|merge\..*\.driver|core\.(sshcommand|gitproxy|askpass)|credential\.(.*\.)?helper|remote\..*\.(uploadpack|receivepack|proxy)|diff\..*\.(command|textconv))$";

/// What a program setting of the repository's own is replaced by when the user's own config has
/// none: nothing, or what git does without it.
fn blank(key: &str) -> &'static str {
    if key.starts_with("merge.") {
        // Git's own three-way merge of the file, as an external driver.
        "git merge-file --marker-size=%L %A %O %B"
    } else if key.ends_with(".uploadpack") {
        "git-upload-pack"
    } else if key.ends_with(".receivepack") {
        "git-receive-pack"
    } else {
        ""
    }
}

/// The settings that replace the program settings the repository's own config sets (`local`,
/// `worktree` scope, files it includes too), from `config -z --show-scope --get-regexp`
/// (`scope\0key\nvalue\0…`): each such key gets its blank and then the user's own values for it
/// again (system, global, command line), so the last one is theirs. Credential helpers are a
/// list: it is reset and the user's helpers come back in their order. A key with `=` or a line
/// break is refused.
fn overrides(out: &[u8]) -> Result<Vec<(String, String)>, String> {
    let mut words = out.split(|b| *b == 0).map(String::from_utf8_lossy);
    let mut entries: Vec<(bool, String, String)> = Vec::new();
    while let (Some(scope), Some(pair)) = (words.next(), words.next()) {
        let (key, value) = pair.split_once('\n').unwrap_or((&pair, ""));
        entries.push((matches!(&*scope, "local" | "worktree"), key.to_owned(), value.to_owned()));
    }
    let credential = |key: &str| key.starts_with("credential.");
    let mut keys: Vec<&str> = Vec::new();
    for (own, key, _) in &entries {
        if *own && !keys.contains(&key.as_str()) {
            if key.contains(['=', '\n', '\r']) {
                return Err(format!(
                    "the repository's setting \"{}\" has a name Gezik does not pass to git",
                    key.escape_debug()
                ));
            }
            keys.push(key);
        }
    }
    let mut out = Vec::new();
    for key in keys.iter().filter(|k| !credential(k)) {
        out.push(((*key).to_owned(), blank(key).to_owned()));
        out.extend(entries.iter().filter(|(own, k, _)| !own && k == key).map(|(_, k, v)| (k.clone(), v.clone())));
    }
    if keys.iter().any(|k| credential(k)) {
        for key in keys.iter().filter(|k| credential(k)) {
            out.push(((*key).to_owned(), String::new()));
        }
        out.extend(entries.iter().filter(|(own, k, _)| !own && credential(k)).map(|(_, k, v)| (k.clone(), v.clone())));
    }
    Ok(out)
}

/// An empty folder of Gezik's own for `core.hooksPath`: no hook runs (a repository's own
/// `core.hooksPath` is overridden too). Made once a run.
fn no_hooks() -> Result<PathBuf, Outcome> {
    static DIR: Mutex<Option<PathBuf>> = Mutex::new(None);
    let mut dir = lock(&DIR);
    if let Some(dir) = dir.as_ref().filter(|d| d.is_dir()) {
        return Ok(dir.clone());
    }
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
    // shortcut: an empty folder left in the temp folder after Gezik quits; a cleanup is not
    // worth its code.
    let made = std::env::temp_dir().join(format!("gezik-no-hooks-{}-{stamp}", std::process::id()));
    std::fs::create_dir(&made).map_err(|e| Outcome::Failed(e.to_string()))?;
    *dir = Some(made.clone());
    Ok(made)
}

/// Whether ssh must be told not to ask (`GIT_SSH_COMMAND=ssh -o BatchMode=yes`): neither the
/// environment (`GIT_SSH`, `GIT_SSH_COMMAND`) nor the user's own config names an ssh command.
pub fn batch_ssh(env_ssh: bool, user_ssh_command: bool) -> bool {
    !env_ssh && !user_ssh_command
}

/// The git command `args` in `dir` as `trust` allows (spec 16 §9.4). Always: the program
/// settings of the repository's own config replaced by the user's or nothing (a `git config`
/// look first; a config that cannot be read refuses the command). `Local` and `Network`: no
/// hooks at all. `Network`, `HooksNetwork`: transport by [`PROTOCOLS`] only, ssh never asks.
/// Blocking.
pub fn prepared(
    args: &[OsString],
    dir: &Path,
    trust: Trust,
    stop: &AtomicBool,
) -> Result<std::process::Command, Outcome> {
    let program = program()?;
    let network = matches!(trust, Trust::Network | Trust::HooksNetwork);
    let mut config = safe_config();
    if network {
        config.retain(|(key, _)| key != "protocol.allow");
    }
    let deadline = Instant::now() + Duration::from_secs(settings().timeout.into());
    let listing = ["config", "-z", "--show-scope", "--get-regexp", PROGRAM_KEYS].map(OsString::from);
    let (code, out, errors) = run(&program, &listing, dir, &config, deadline, stop)?;
    if !matches!(code.code(), Some(0 | 1)) {
        return Err(Outcome::Failed(format!("git too old or config unreadable: {}", first_line(&errors))));
    }
    let user_ssh = out.split(|b| *b == 0).collect::<Vec<_>>().chunks(2).any(|pair| {
        pair.len() == 2 && !matches!(pair[0], b"local" | b"worktree") && pair[1].starts_with(b"core.sshcommand\n")
    });
    let (set, packs) = overrides(&out)
        .map_err(Outcome::Failed)?
        .into_iter()
        .partition::<Vec<_>, _>(|(key, _)| !(key.ends_with(".uploadpack") || key.ends_with(".receivepack")));
    config.extend(set);
    if matches!(trust, Trust::Local | Trust::Network) {
        config.push(("core.hooksPath".to_owned(), no_hooks()?.to_string_lossy().into_owned()));
    }
    let mut args = args.to_vec();
    // A remote's upload-pack and receive-pack cannot be set twice (git keeps the first, the
    // repository's): the command's own option replaces them (the user's value, else git's).
    let pack = |end: &str| packs.iter().rev().find(|(key, _)| key.ends_with(end)).map(|(_, value)| value.clone());
    let option = match args.first().and_then(|a| a.to_str()) {
        Some("fetch" | "pull") => pack(".uploadpack").map(|v| format!("--upload-pack={v}")),
        Some("push") => pack(".receivepack").map(|v| format!("--receive-pack={v}")),
        _ => None,
    };
    if let Some(option) = option {
        args.insert(1, option.into());
    }
    let mut command = command(&program, &args, dir, &config);
    command.env("GIT_LITERAL_PATHSPECS", "1");
    if network {
        command.env_remove("GIT_NO_LAZY_FETCH").env("GIT_ALLOW_PROTOCOL", PROTOCOLS);
        let env_ssh = ["GIT_SSH", "GIT_SSH_COMMAND"].iter().any(|v| std::env::var_os(v).is_some_and(|v| !v.is_empty()));
        if batch_ssh(env_ssh, user_ssh) {
            command.env("GIT_SSH_COMMAND", "ssh -o BatchMode=yes");
        }
    }
    Ok(command)
}

/// Runs `args` in `dir` as `trust` allows and gives its output, or git's first error line
/// (Gezik's own reads: the branches, the staged files, a file at HEAD; a discard's restore).
/// Blocking.
pub fn read(args: &[&str], dir: &Path, trust: Trust) -> Result<Vec<u8>, String> {
    let stop = AtomicBool::new(false);
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    let failed = |outcome: Outcome| match outcome {
        Outcome::Failed(why) => why,
        Outcome::Missing => gezik_core::git::missing_text(),
        _ => gezik_i18n::t!("Git took too long ([git] timeout)").into_owned(),
    };
    let command = prepared(&args, dir, trust, &stop).map_err(failed)?;
    let deadline = Instant::now() + Duration::from_secs(settings().timeout.into());
    let (code, out, errors) = run_command(command, deadline, &stop).map_err(failed)?;
    if code.success() { Ok(out) } else { Err(first_line(&errors)) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn tree(entries: &[(&str, &str)]) -> HashMap<PathBuf, String> {
        entries.iter().map(|(p, t)| (PathBuf::from(p), (*t).to_owned())).collect()
    }

    fn find(folder: &str, home: Option<&str>, files: &HashMap<PathBuf, String>) -> Option<Repo> {
        let kind = |p: &Path| files.get(p).map(|text| text == "<dir>");
        let read = |p: &Path| files.get(p).filter(|t| *t != "<dir>").cloned();
        find_in(Path::new(folder), home.map(Path::new), &kind, &read)
    }

    #[test]
    fn a_repository_is_found_above_and_a_git_file_followed() {
        let files = tree(&[
            ("/home/ali/src/app/.git", "<dir>"),
            ("/home/ali/src/app/.git/HEAD", "ref: refs/heads/main"),
            ("/home/ali/src/app/vendor/lib/.git", "gitdir: ../../.git/modules/lib\n"),
            ("/home/ali/src/app/vendor/lib/../../.git/modules/lib/HEAD", "x"),
            ("/home/ali/src/wt/.git", "gitdir: /home/ali/src/app/.git/worktrees/wt"),
            ("/home/ali/src/app/.git/worktrees/wt/HEAD", "x"),
            ("/home/ali/src/app/.git/worktrees/wt/commondir", "../..\n"),
        ]);
        let repo = find("/home/ali/src/app/docs/a", Some("/home/ali"), &files).unwrap();
        assert_eq!(repo.root, Path::new("/home/ali/src/app"));
        assert_eq!(repo.real, Path::new("/home/ali/src/app/docs/a"));
        assert_eq!(repo.git_dir, repo.common);
        let sub = find("/home/ali/src/app/vendor/lib/x", Some("/home/ali"), &files).unwrap();
        assert_eq!(sub.root, Path::new("/home/ali/src/app/vendor/lib"), "a submodule is its own repository");
        assert_eq!(sub.git_dir, Path::new("/home/ali/src/app/vendor/lib/../../.git/modules/lib"));
        let wt = find("/home/ali/src/wt", Some("/home/ali"), &files).unwrap();
        assert_eq!(wt.common, Path::new("/home/ali/src/app/.git/worktrees/wt/../.."));
        assert!(find("/home/ali/src/app/.git/refs", Some("/home/ali"), &files).is_none(), "inside .git");
        if cfg!(any(windows, target_os = "macos")) {
            assert!(find("/home/ali/src/app/.GIT/refs", Some("/home/ali"), &files).is_none(), "any case");
        }
    }

    #[test]
    fn an_empty_or_broken_git_folder_is_no_repository() {
        let files = tree(&[("/w/a/.git", "<dir>"), ("/w/.git", "<dir>"), ("/w/.git/HEAD", "x")]);
        assert_eq!(find("/w/a/b", None, &files).unwrap().root, Path::new("/w"), "the empty one is passed over");
        let file = tree(&[("/v/.git", "gitdir: /nowhere")]);
        assert!(find("/v", None, &file).is_none());
    }

    #[test]
    fn the_look_stops_at_the_home_folder() {
        let files =
            tree(&[("/home/.git", "<dir>"), ("/home/.git/HEAD", "x"), ("/srv/.git", "<dir>"), ("/srv/.git/HEAD", "x")]);
        assert!(find("/home/ali/notes", Some("/home/ali"), &files).is_none(), "not above home");
        assert!(find("/srv/www/site", Some("/home/ali"), &files).is_some(), "elsewhere up to the root");
        let home = tree(&[("/home/ali/.git", "<dir>"), ("/home/ali/.git/HEAD", "x")]);
        assert!(find("/home/ali/notes", Some("/home/ali"), &home).is_some(), "home itself counts");
    }

    #[test]
    fn network_drives_are_not_looked_at() {
        if cfg!(windows) {
            assert!(find_repo(Path::new(r"\\nowhere-gezik-test\share\x")).is_none(), "a share is not looked at");
        }
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gezik-git-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        plain(std::fs::canonicalize(&dir).unwrap())
    }

    #[test]
    fn small_files_only_are_read() {
        let dir = temp("small");
        std::fs::write(dir.join("big"), vec![b'x'; 100_000]).unwrap();
        assert_eq!(read_small(&dir.join("big")).map(|t| t.len()), Some(SMALL_FILE as usize));
        assert_eq!(read_small(&dir), None, "a folder");
        #[cfg(unix)]
        {
            // A FIFO would block an open() until a writer comes.
            let fifo = dir.join("fifo");
            assert!(std::process::Command::new("mkfifo").arg(&fifo).status().unwrap().success());
            assert_eq!(read_small(&fifo), None);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_folder_through_a_link_or_junction_is_found_as_git_sees_it() {
        let dir = temp("link");
        std::fs::create_dir_all(dir.join("repo/.git")).unwrap();
        std::fs::write(dir.join("repo/.git/HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::create_dir_all(dir.join("repo/sub")).unwrap();
        let link = dir.join("link");
        #[cfg(windows)]
        let made = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(&link)
            .arg(dir.join("repo").join("sub"))
            .output()
            .is_ok_and(|o| o.status.success());
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(dir.join("repo/sub"), &link).is_ok();
        assert!(made, "the link");
        let repo = find_repo(&link).expect("found through the link");
        assert_eq!(repo.root, dir.join("repo"));
        assert_eq!(prefix(&repo.root, &repo.real), "sub");
        #[cfg(windows)]
        let _ = std::fs::remove_dir(&link);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn macos_skips_the_stub_without_command_line_tools() {
        let exists = |p: &Path| ["/usr/bin/git", "/opt/homebrew/bin/git"].iter().any(|g| p == Path::new(g));
        let path = Some(OsString::from("/usr/bin"));
        let no = pick_program("", path.clone(), &exists, &|| false, true);
        assert_eq!(no, Some(PathBuf::from("/opt/homebrew/bin/git")));
        let yes = pick_program("", path.clone(), &exists, &|| true, true);
        if cfg!(not(windows)) {
            assert_eq!(yes, Some(PathBuf::from("/usr/bin/git")));
        }
        let never = |_: &Path| false;
        assert_eq!(pick_program("", path, &never, &|| panic!("not asked"), true), None);
        let relative = Some(OsString::from("."));
        assert_eq!(pick_program("", relative, &|_| true, &|| true, false), None, "never the folder shown");
    }

    #[test]
    fn a_set_program_is_a_full_path_or_a_name_on_path() {
        let root = if cfg!(windows) { r"C:\tools" } else { "/tools" };
        let full = Path::new(root).join("git");
        let exists = |p: &Path| p == full;
        let path = Some(OsString::from(root));
        assert_eq!(pick_program(full.to_str().unwrap(), None, &exists, &|| false, false), Some(full.clone()));
        assert_eq!(pick_program(&format!("{root}/nope"), None, &exists, &|| false, false), None, "not there");
        assert_eq!(pick_program("git", path.clone(), &exists, &|| false, false), Some(full.clone()), "on PATH");
        // Relative to the folder shown: never.
        assert_eq!(pick_program("bin/git", path.clone(), &|_| true, &|| false, false), None);
        assert_eq!(pick_program("./git", path, &|_| true, &|| false, false), None);
    }

    #[test]
    fn versions_are_read_and_old_ones_refused() {
        assert_eq!(parse_version("git version 2.47.0.windows.2\n"), Some((2, 47)));
        assert_eq!(parse_version("git version 2.30.9"), Some((2, 30)));
        assert_eq!(parse_version("git version 2.39.5 (Apple Git-154)"), Some((2, 39)));
        assert_eq!(parse_version("hub version 2"), None);
        assert!(parse_version("git version 2.30.1").unwrap() < MIN_VERSION);
    }

    #[test]
    fn the_repository_s_program_settings_give_way_to_the_user_s() {
        let out = b"global\0filter.lfs.smudge\ngit-lfs smudge\0local\0filter.lfs.smudge\nevil\0\
            global\0credential.helper\nmanager\0local\0credential.helper\n!evil\0\
            worktree\0credential.https://x.helper\n!evil2\0local\0remote.origin.uploadpack\nevil\0\
            local\0merge.m.driver\nevil\0local\0core.sshcommand\nevil\0";
        let set = overrides(out).unwrap();
        let pairs: Vec<(&str, &str)> = set.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        assert_eq!(
            pairs,
            [
                ("filter.lfs.smudge", ""),
                ("filter.lfs.smudge", "git-lfs smudge"),
                ("remote.origin.uploadpack", "git-upload-pack"),
                ("merge.m.driver", "git merge-file --marker-size=%L %A %O %B"),
                ("core.sshcommand", ""),
                ("credential.helper", ""),
                ("credential.https://x.helper", ""),
                ("credential.helper", "manager"),
            ]
        );
        assert!(overrides(b"global\0core.sshcommand\nmine\0").unwrap().is_empty(), "the user's own stay as they are");
        assert!(overrides(b"local\0filter.a=b.smudge\nx\0").is_err());
        assert!(batch_ssh(false, false) && !batch_ssh(true, false) && !batch_ssh(false, true));
    }

    #[test]
    fn only_the_repository_s_own_filters_are_emptied() {
        let out = b"system\0filter.lfs.clean\0global\0filter.lfs.process\0local\0filter.evil.clean\0\
            worktree\0filter.a.b.process\0local\0filter.evil.process\0";
        assert_eq!(local_drivers(out).unwrap(), ["evil", "a.b"]);
        assert!(local_drivers(b"").unwrap().is_empty());
        assert!(local_drivers(b"local\0filter.x=y.clean\0").is_err(), "an = in the name: refused");
        assert!(local_drivers(b"local\0filter.x\ny.clean\0").is_err());
        assert!(local_drivers(b"global\0filter.x=y.clean\0").unwrap().is_empty(), "the user's own: left alone");
    }

    #[test]
    fn the_filter_look_fails_closed() {
        assert!(drivers_from(Some(1), b"", "").unwrap().is_empty(), "none set");
        assert_eq!(drivers_from(Some(0), b"local\0filter.e.clean\0", "").unwrap(), ["e"]);
        let old = drivers_from(Some(129), b"", "error: unknown option `show-scope'\n");
        assert!(old.is_err_and(|why| why.contains("show-scope")), "an old git: no status");
        assert!(drivers_from(Some(128), b"", "fatal: bad config line 1").is_err());
        assert!(drivers_from(None, b"", "").is_err(), "ended by a signal");
    }

    #[test]
    fn git_s_own_events_count_wherever_the_repository_is() {
        use notify::event::{CreateKind, ModifyKind};
        let root = PathBuf::from("/home/logs/objects/repo/.git");
        let event = |path: PathBuf| Event::new(EventKind::Modify(ModifyKind::Any)).add_path(path);
        let roots = [root.clone(), root.clone()];
        assert!(concerns(&event(root.join("index")), &roots), "logs/objects above the repository");
        assert!(concerns(&event(root.join("refs/heads/main")), &roots));
        assert!(!concerns(&event(root.join("objects/ab/cdef")), &roots));
        assert!(!concerns(&event(root.join("logs/HEAD")), &roots));
        assert!(!concerns(&event(root.join("index.lock")), &roots));
        let read = Event::new(EventKind::Access(notify::event::AccessKind::Any)).add_path(root.join("index"));
        assert!(!concerns(&read, &roots));
        assert!(concerns(&Event::new(EventKind::Create(CreateKind::Any)), &roots), "no path: may be anything");
    }

    #[test]
    fn prefixes_use_slashes() {
        let root = Path::new("/r");
        assert_eq!(prefix(root, Path::new("/r")), "");
        assert_eq!(prefix(root, &Path::new("/r").join("a").join("b c")), "a/b c");
    }

    #[test]
    fn verbatim_paths_are_written_plainly() {
        assert_eq!(plain(PathBuf::from(r"\\?\C:\Users\a")), PathBuf::from(r"C:\Users\a"));
        assert_eq!(plain(PathBuf::from(r"\\?\UNC\nas\s\x")), PathBuf::from(r"\\nas\s\x"));
        assert_eq!(plain(PathBuf::from("/home/a")), PathBuf::from("/home/a"));
    }

    /// A program that runs about five seconds and prints nothing.
    fn slow() -> (PathBuf, Vec<OsString>) {
        if cfg!(windows) {
            ("ping.exe".into(), ["-n", "6", "127.0.0.1"].map(OsString::from).to_vec())
        } else {
            ("/bin/sleep".into(), vec!["5".into()])
        }
    }

    #[test]
    fn a_slow_git_is_stopped_at_its_time_or_when_asked() {
        let (program, args) = slow();
        let dir = std::env::temp_dir();
        let started = Instant::now();
        let stop = AtomicBool::new(false);
        let outcome = run(&program, &args, &dir, &[], Instant::now() + Duration::from_millis(300), &stop);
        assert!(matches!(outcome, Err(Outcome::TimedOut)), "{outcome:?}");
        assert!(started.elapsed() < Duration::from_secs(3), "{:?}", started.elapsed());
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let asker = {
            let stop = stop.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(200));
                stop.store(true, Ordering::Relaxed);
            })
        };
        let started = Instant::now();
        let outcome = run(&program, &args, &dir, &[], Instant::now() + Duration::from_secs(60), &stop);
        asker.join().unwrap();
        assert!(matches!(outcome, Err(Outcome::Stopped)), "{outcome:?}");
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    /// A new repository with one commit, made with the real git (`None` without one).
    struct Scratch {
        git: PathBuf,
        dir: PathBuf,
    }

    impl Scratch {
        fn new(name: &str) -> Option<Scratch> {
            set_settings(GitSettings::default());
            let git = program().ok()?;
            let scratch = Scratch { git, dir: temp(name) };
            scratch.git(&["init", "-q", "-b", "main"]);
            std::fs::write(scratch.dir.join("a.txt"), "a").unwrap();
            scratch.git(&["add", "."]);
            scratch.git(&["commit", "-q", "-m", "one"]);
            Some(scratch)
        }

        fn git(&self, args: &[&str]) {
            let ok = std::process::Command::new(&self.git)
                .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "core.autocrlf=false"])
                .args(args)
                .current_dir(&self.dir)
                .output()
                .unwrap()
                .status
                .success();
            assert!(ok, "git {args:?}");
        }

        /// A shell command writing `marker` (git runs it through its own `sh`).
        fn writes(&self, marker: &Path) -> String {
            format!("echo x > \"{}\"", marker.display()).replace('\\', "/")
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    /// With the real git (none: nothing to check): a repository's marks, its hostile config
    /// never run.
    #[test]
    fn real_git_marks_a_repository_and_runs_none_of_its_config() {
        let Some(s) = Scratch::new("marks") else { return };
        let dir = s.dir.clone();
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub/b.txt"), "b").unwrap();
        std::fs::write(dir.join(".gitignore"), "*.log\n").unwrap();
        s.git(&["add", "."]);
        s.git(&["commit", "-q", "-m", "two"]);
        std::fs::write(dir.join("a.txt"), "changed").unwrap();
        std::fs::write(dir.join("sub/new.txt"), "n").unwrap();
        std::fs::write(dir.join("x.log"), "l").unwrap();
        // Hostile: each would write a marker file.
        let marker = dir.join("ran");
        s.git(&["config", "core.fsmonitor", &s.writes(&marker)]);
        s.git(&["config", "filter.evil.clean", &s.writes(&marker)]);
        std::fs::write(dir.join(".git/info/attributes"), "* filter=evil\n").unwrap();
        // A stat change makes git look at the file's content (through a clean filter).
        std::fs::write(dir.join("sub/b.txt"), "b").unwrap();

        let stop = AtomicBool::new(false);
        let Outcome::Done(whole) = status(&dir, true, Duration::from_secs(20), &stop) else { panic!("no status") };
        assert!(!marker.exists(), "the repository's fsmonitor or clean filter ran");
        assert_eq!(whole.branch.as_deref(), Some("main"));
        let marks = gezik_core::git::folder_marks(&whole, "");
        use gezik_core::git::GitMark;
        assert_eq!(marks.of("a.txt"), Some(GitMark::Modified));
        assert_eq!(marks.of("sub"), Some(GitMark::Untracked));
        assert_eq!(marks.of("x.log"), Some(GitMark::Ignored));
        let repo = find_repo(&dir.join("sub")).unwrap();
        assert_eq!(repo.root, dir);
        let Outcome::Done(sub) = status(&repo.real, false, Duration::from_secs(20), &stop) else { panic!("no status") };
        let marks = gezik_core::git::folder_marks(&sub, &prefix(&repo.root, &repo.real));
        assert_eq!((marks.of("new.txt"), marks.of("b.txt"), marks.changed), (Some(GitMark::Untracked), None, 1));
    }

    /// `-c filter.x=y.clean=` sets `filter.x.y=clean=`… no: git splits `-c` at the first `=`,
    /// so a driver named `x=y` stayed on. Gezik refuses such a name, and its config passing
    /// never splits a key.
    #[test]
    fn a_filter_named_with_an_equals_sign_never_runs() {
        let Some(s) = Scratch::new("equals") else { return };
        let marker = s.dir.join("ran");
        s.git(&["config", "filter.x=y.clean", &s.writes(&marker)]);
        std::fs::write(s.dir.join(".git/info/attributes"), "* filter=x=y\n").unwrap();
        std::fs::write(s.dir.join("a.txt"), "a").unwrap();
        let stop = AtomicBool::new(false);
        let outcome = status(&s.dir, true, Duration::from_secs(20), &stop);
        assert!(!marker.exists(), "the clean filter ran");
        assert!(matches!(&outcome, Outcome::Failed(why) if why.contains("x=y")), "{outcome:?}");
        // The key reaches git whole.
        let config = [("filter.x=y.clean".to_owned(), String::new())];
        let args = ["config", "--get", "filter.x=y.clean"].map(OsString::from);
        let (code, out, _) =
            run(&s.git, &args, &s.dir, &config, Instant::now() + Duration::from_secs(20), &stop).unwrap();
        assert!(code.success());
        assert_eq!(String::from_utf8_lossy(&out).trim(), "", "emptied, not left as the repository set it");
    }

    /// A repository's `protocol.ext.allow = always` beats `protocol.allow = never`; nothing
    /// beats `GIT_ALLOW_PROTOCOL`.
    #[test]
    fn a_repository_cannot_allow_a_transport() {
        let Some(s) = Scratch::new("ext") else { return };
        let marker = s.dir.join("ran");
        s.git(&["config", "protocol.ext.allow", "always"]);
        s.git(&["remote", "add", "origin", &format!("ext::sh -c {}", s.writes(&marker).replace(' ', "% "))]);
        let stop = AtomicBool::new(false);
        let args = ["fetch", "-q", "origin"].map(OsString::from);
        let ran = run(&s.git, &args, &s.dir, &safe_config(), Instant::now() + Duration::from_secs(20), &stop);
        assert!(ran.is_ok_and(|(code, _, _)| !code.success()), "the fetch is refused");
        assert!(!marker.exists(), "the ext:: command ran");
    }

    #[test]
    fn a_config_git_cannot_read_gives_no_status() {
        let Some(s) = Scratch::new("broken") else { return };
        std::fs::write(s.dir.join(".git/config"), "[core\nbroken = = =\n").unwrap();
        let stop = AtomicBool::new(false);
        let outcome = status(&s.dir, true, Duration::from_secs(20), &stop);
        assert!(matches!(&outcome, Outcome::Failed(why) if why.contains("config")), "{outcome:?}");
    }
}
