//! Git status (spec 16 §8) with the system's `git`: the repository a folder is in (a `.git` look
//! on the loading thread, nothing more outside repositories), the program, one cancellable
//! `git status` for the folder shown, and a watch on the repository's `.git` folder.
//!
//! A repository's own config must never run code through git (one downloaded with its `.git`):
//! status runs with `core.fsmonitor=false`, the repository's own filter drivers emptied (a
//! clean filter runs when a file's stat changed), no transport (`protocol.allow=never`, no lazy
//! fetch) and submodules looked at only by their commit (no git run inside them). Status runs
//! no hooks, and with `--no-optional-locks` writes nothing (no `post-index-change`).

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

static SETTINGS: LazyLock<Mutex<GitSettings>> = LazyLock::new(Mutex::default);
/// The git found for `[git] program` (the setting it was found for): looked for once, on the
/// first run's thread.
static PROGRAM: Mutex<Option<(String, Option<PathBuf>)>> = Mutex::new(None);

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
}

/// The repository `folder` is in, if `[git] status` is on: `.git` (a folder or a file) looked
/// for in it and the folders above, up to the home folder (when under it) or the drive's root.
/// Not on a network drive unless `[git] network`, nor inside a `.git` folder. Runs on the
/// loading thread; outside a repository it costs one look a level.
pub fn find_repo(folder: &Path) -> Option<Repo> {
    let settings = settings();
    if !settings.status || (!settings.network && gezik_platform::fs::is_network(folder).unwrap_or(true)) {
        return None;
    }
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    find_in(folder, home.as_deref(), &|p| std::fs::symlink_metadata(p).ok().map(|m| m.is_dir()), &|p| {
        std::fs::read_to_string(p).ok()
    })
}

/// `find_repo`'s look: `kind` says whether a path is a folder (`Some(true)`), something else, or
/// not there; `read` a small file's text.
fn find_in(
    folder: &Path,
    home: Option<&Path>,
    kind: &dyn Fn(&Path) -> Option<bool>,
    read: &dyn Fn(&Path) -> Option<String>,
) -> Option<Repo> {
    if folder.components().any(|c| c.as_os_str() == ".git") {
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
        if let Some(git_dir) = git_dir {
            let common = read(&git_dir.join("commondir"))
                .map(|text| git_dir.join(text.trim()))
                .unwrap_or_else(|| git_dir.clone());
            return Some(Repo { root: dir.to_path_buf(), git_dir, common });
        }
        if home.is_some_and(|home| gezik_core::ops::paths::same_path(dir, home)) {
            break;
        }
    }
    None
}

/// The git to run: `[git] program`, else `git` on PATH; on macOS not `/usr/bin/git` unless
/// `xcode-select -p` succeeds (without Command Line Tools that stub opens an install window),
/// then Homebrew's. Looked for once per setting (blocking: a run's thread).
pub fn program() -> Option<PathBuf> {
    let setting = settings().program;
    if let Some((was, found)) = lock(&PROGRAM).as_ref()
        && *was == setting
    {
        return found.clone();
    }
    let xcode = || {
        std::process::Command::new("/usr/bin/xcode-select")
            .arg("-p")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    };
    let found = pick_program(&setting, std::env::var_os("PATH"), &|p| p.is_file(), &xcode, cfg!(target_os = "macos"));
    *lock(&PROGRAM) = Some((setting, found.clone()));
    found
}

/// `program`'s choice. Only absolute PATH folders count: git is never started from the folder
/// shown (Windows would try a `git.exe` there).
fn pick_program(
    setting: &str,
    path: Option<OsString>,
    exists: &dyn Fn(&Path) -> bool,
    xcode: &dyn Fn() -> bool,
    macos: bool,
) -> Option<PathBuf> {
    if !setting.is_empty() {
        return Some(PathBuf::from(setting)).filter(|p| exists(p));
    }
    let name = if cfg!(windows) { "git.exe" } else { "git" };
    let mut stub_ok = None;
    let on_path =
        path.iter().flat_map(std::env::split_paths).filter(|dir| dir.is_absolute()).map(|dir| dir.join(name)).find(
            |git| exists(git) && (!macos || git != Path::new("/usr/bin/git") || *stub_ok.get_or_insert_with(xcode)),
        );
    let brew = ["/opt/homebrew/bin/git", "/usr/local/bin/git"].into_iter().map(PathBuf::from);
    on_path.or_else(|| brew.filter(|_| macos).find(|git| exists(git)))
}

/// How a status ended.
#[derive(Debug)]
pub enum Outcome {
    Done(GitStatus),
    /// No git (said once).
    Missing,
    /// Git said no (its first error line: not a repository Git trusts, a broken one).
    Failed(String),
    /// Past `[git] timeout`: stopped.
    TimedOut,
    /// Stopped (another folder, a newer run, the pane closed).
    Stopped,
}

/// Every git Gezik runs: no prompt, nothing fetched on demand, English messages, nothing of a
/// repository Gezik did not ask for.
fn command(program: &Path, args: &[OsString], dir: &Path) -> std::process::Command {
    let mut command = std::process::Command::new(program);
    command
        .args(args)
        .current_dir(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("LC_ALL", "C");
    for name in ["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_OBJECT_DIRECTORY", "GIT_COMMON_DIR"] {
        command.env_remove(name);
    }
    command
}

/// The `-c` options of every run.
const SAFE: [&str; 6] = ["-c", "core.fsmonitor=false", "-c", "core.quotepath=off", "-c", "protocol.allow=never"];

/// One status of the folder `dir` (spec 16 §8.2): first the repository's own filter drivers
/// (`git config --show-scope`, local or worktree), emptied for the status; ended when `stop` is
/// set or after `timeout`.
pub fn status(dir: &Path, ignored: bool, timeout: Duration, stop: &AtomicBool) -> Outcome {
    let Some(program) = program() else { return Outcome::Missing };
    let deadline = Instant::now() + timeout;
    let mut args: Vec<OsString> = SAFE.iter().map(OsString::from).collect();
    let listing = ["config", "-z", "--show-scope", "--name-only", "--get-regexp", r"^filter\..*\.(clean|process)$"];
    let config: Vec<OsString> = args.iter().cloned().chain(listing.iter().map(OsString::from)).collect();
    // Exit 1: none set.
    let drivers = match run(&program, &config, dir, deadline, stop) {
        Ok((_, out, _)) => local_drivers(&out),
        Err(outcome) => return outcome,
    };
    for driver in drivers {
        for key in ["clean", "process"] {
            args.extend(["-c".into(), format!("filter.{driver}.{key}=").into()]);
        }
    }
    args.splice(0..0, [OsString::from("--no-optional-locks")]);
    args.extend(
        [
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
        .map(OsString::from),
    );
    match run(&program, &args, dir, deadline, stop) {
        Ok((code, out, _)) if code.success() => Outcome::Done(gezik_core::git::parse_status(&out)),
        Ok((_, _, errors)) => {
            let line = errors.lines().map(|l| l.trim_start_matches("fatal: ")).find(|l| !l.trim().is_empty());
            Outcome::Failed(line.unwrap_or("git status failed").to_owned())
        }
        Err(outcome) => outcome,
    }
}

/// The filter drivers the repository's own config (`local`, `worktree`) sets, from
/// `config -z --show-scope --name-only` (`scope\0name\0…`).
fn local_drivers(out: &[u8]) -> Vec<String> {
    let mut words = out.split(|b| *b == 0).map(String::from_utf8_lossy);
    let mut drivers = Vec::new();
    while let (Some(scope), Some(name)) = (words.next(), words.next()) {
        let driver = name.strip_prefix("filter.").and_then(|n| n.rsplit_once('.')).map(|(d, _)| d.to_owned());
        if matches!(&*scope, "local" | "worktree")
            && let Some(driver) = driver
            && !drivers.contains(&driver)
        {
            drivers.push(driver);
        }
    }
    drivers
}

/// Runs `program` with `args` in `dir`, its output and error output read meanwhile; `Err` when
/// it could not start (`Missing`), was stopped or ran past `deadline`.
fn run(
    program: &Path,
    args: &[OsString],
    dir: &Path,
    deadline: Instant,
    stop: &AtomicBool,
) -> Result<(std::process::ExitStatus, Vec<u8>, String), Outcome> {
    let (mut child, mut stdout, mut stderr) = gezik_platform::ChildProcess::spawn_piped(command(program, args, dir))
        .map_err(|e| Outcome::Failed(e.to_string()))?;
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
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
        if event.map_or(true, |event| concerns(&event)) {
            on_change();
        }
    })
    .ok()?;
    watcher.watch(&repo.git_dir, RecursiveMode::NonRecursive).ok()?;
    // Without it a branch moved by another worktree or a push (refs/remotes) goes unseen.
    let _ = watcher.watch(&repo.common.join("refs"), RecursiveMode::Recursive);
    Some(watcher)
}

/// Whether `event` may change the status: not a read, not only lock files, objects or logs.
fn concerns(event: &Event) -> bool {
    if matches!(event.kind, EventKind::Access(_)) {
        return false;
    }
    let counts = |path: &PathBuf| {
        let lock = path.extension() == Some(OsStr::new("lock"));
        let inside = path.components().any(|c| matches!(c.as_os_str().to_str(), Some("objects" | "logs")));
        !lock && !inside
    };
    event.paths.is_empty() || event.paths.iter().any(counts)
}

/// The folder `dir` from the repository's root `root`, with `/` ("" for the root).
pub fn prefix(root: &Path, dir: &Path) -> String {
    let rest = dir.strip_prefix(root).unwrap_or(Path::new(""));
    rest.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/")
}

/// The note when no git was found: once a run of Gezik.
pub fn missing_note() -> Option<String> {
    static SAID: AtomicBool = AtomicBool::new(false);
    (!SAID.swap(true, Ordering::Relaxed)).then(gezik_core::git::missing_text)
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
            ("/home/ali/src/app/vendor/lib/.git", "gitdir: ../../.git/modules/lib\n"),
            ("/home/ali/src/wt/.git", "gitdir: /home/ali/src/app/.git/worktrees/wt"),
            ("/home/ali/src/app/.git/worktrees/wt/commondir", "../..\n"),
        ]);
        let repo = find("/home/ali/src/app/docs/a", Some("/home/ali"), &files).unwrap();
        assert_eq!(repo.root, Path::new("/home/ali/src/app"));
        assert_eq!(repo.git_dir, repo.common);
        let sub = find("/home/ali/src/app/vendor/lib/x", Some("/home/ali"), &files).unwrap();
        assert_eq!(sub.root, Path::new("/home/ali/src/app/vendor/lib"), "a submodule is its own repository");
        assert_eq!(sub.git_dir, Path::new("/home/ali/src/app/vendor/lib/../../.git/modules/lib"));
        let wt = find("/home/ali/src/wt", Some("/home/ali"), &files).unwrap();
        assert_eq!(wt.common, Path::new("/home/ali/src/app/.git/worktrees/wt/../.."));
        assert!(find("/home/ali/src/app/.git/refs", Some("/home/ali"), &files).is_none(), "inside .git");
    }

    #[test]
    fn the_look_stops_at_the_home_folder() {
        let files = tree(&[("/home/.git", "<dir>"), ("/srv/.git", "<dir>")]);
        assert!(find("/home/ali/notes", Some("/home/ali"), &files).is_none(), "not above home");
        assert!(find("/srv/www/site", Some("/home/ali"), &files).is_some(), "elsewhere up to the root");
        let home = tree(&[("/home/ali/.git", "<dir>")]);
        assert!(find("/home/ali/notes", Some("/home/ali"), &home).is_some(), "home itself counts");
    }

    #[test]
    fn network_drives_are_not_looked_at() {
        if cfg!(windows) {
            assert!(find_repo(Path::new(r"\\nowhere-gezik-test\share\x")).is_none(), "a share is not looked at");
        }
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
        assert_eq!(pick_program("/x/git", None, &|p| p == Path::new("/x/git"), &|| false, true), Some("/x/git".into()));
        assert_eq!(pick_program("/x/git", None, &never, &|| false, false), None, "a setting that is not there");
        let relative = Some(OsString::from("."));
        assert_eq!(pick_program("", relative, &|_| true, &|| true, false), None, "never the folder shown");
    }

    #[test]
    fn only_the_repository_s_own_filters_are_emptied() {
        let out = b"system\0filter.lfs.clean\0global\0filter.lfs.process\0local\0filter.evil.clean\0\
            worktree\0filter.a.b.process\0local\0filter.evil.process\0";
        assert_eq!(local_drivers(out), ["evil", "a.b"]);
        assert!(local_drivers(b"").is_empty());
    }

    #[test]
    fn prefixes_use_slashes() {
        let root = Path::new("/r");
        assert_eq!(prefix(root, Path::new("/r")), "");
        assert_eq!(prefix(root, &Path::new("/r").join("a").join("b c")), "a/b c");
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
        let outcome = run(&program, &args, &dir, Instant::now() + Duration::from_millis(300), &stop);
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
        let outcome = run(&program, &args, &dir, Instant::now() + Duration::from_secs(60), &stop);
        asker.join().unwrap();
        assert!(matches!(outcome, Err(Outcome::Stopped)), "{outcome:?}");
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    /// With the real git (none: nothing to check): a repository's marks, its hostile config
    /// never run.
    #[test]
    fn real_git_marks_a_repository_and_runs_none_of_its_config() {
        set_settings(GitSettings::default());
        let Some(git) = program() else { return };
        let dir = std::env::temp_dir().join(format!("gezik-git-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        let git_in = |args: &[&str]| {
            let ok = std::process::Command::new(&git)
                .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "core.autocrlf=false"])
                .args(args)
                .current_dir(&dir)
                .output()
                .unwrap()
                .status
                .success();
            assert!(ok, "git {args:?}");
        };
        git_in(&["init", "-q", "-b", "main"]);
        std::fs::write(dir.join("a.txt"), "a").unwrap();
        std::fs::write(dir.join("sub/b.txt"), "b").unwrap();
        std::fs::write(dir.join(".gitignore"), "*.log\n").unwrap();
        git_in(&["add", "."]);
        git_in(&["commit", "-q", "-m", "one"]);
        std::fs::write(dir.join("a.txt"), "changed").unwrap();
        std::fs::write(dir.join("sub/new.txt"), "n").unwrap();
        std::fs::write(dir.join("x.log"), "l").unwrap();
        // Hostile: each would write a marker file.
        let marker = dir.join("ran");
        let write_marker = format!("echo x > \"{}\"", marker.display()).replace('\\', "/");
        git_in(&["config", "core.fsmonitor", &write_marker]);
        git_in(&["config", "filter.evil.clean", &write_marker]);
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
        let Outcome::Done(sub) = status(&dir.join("sub"), false, Duration::from_secs(20), &stop) else {
            panic!("no status")
        };
        let marks = gezik_core::git::folder_marks(&sub, &prefix(&repo.root, &dir.join("sub")));
        assert_eq!((marks.of("new.txt"), marks.of("b.txt"), marks.changed), (Some(GitMark::Untracked), None, 1));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
