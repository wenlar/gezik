//! The command line (spec 5.1): parsed here, pure, before any window or settings; the
//! targets become tabs by [`group`] and [`tab_for`], at start (start.rs) and in the running
//! Gezik (single_instance.rs) alike.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use gezik_core::nav::Location;
use gezik_core::ops::paths::same_path;
pub use gezik_platform::instance::Target;
use gezik_platform::instance::{MAX_PATHS, Request};

use crate::start::PathKind;

/// `gezik --help`. Each later part adds its options here (spec 5.1).
pub const HELP: &str = "\
Usage: gezik [OPTIONS] [PATH...]

  PATH              folder: open it; file: open its folder with it selected; several: a tab each
  --new-tab         always a new tab (default: reuse a tab already showing that folder)
  --new-window      a separate Gezik window (a new process), not the running one
  --select PATH     open PATH's folder with PATH selected (may repeat)
  --version, --help
";

/// The parsed command line.
#[derive(Debug, Default, PartialEq)]
pub struct Cli {
    pub targets: Vec<Target>,
    pub new_tab: bool,
    // Used by the single instance, Task 4.
    #[cfg_attr(not(test), allow(dead_code))]
    pub new_window: bool,
    pub help: bool,
    pub version: bool,
    /// Unknown options and the like: on the console, and in the status bar of a Gezik that
    /// opens; never a reason not to open.
    pub warnings: Vec<String>,
}

/// Parses the arguments after the program's name. `--` ends the options. With `windows` a
/// trailing `"` is dropped: `gezik "D:\Work\"` arrives as `D:\Work"` (`\"` is an escaped
/// quote to `CommandLineToArgvW`), and no Windows name has a `"`.
pub fn parse(args: impl IntoIterator<Item = OsString>, windows: bool) -> Cli {
    let mut cli = Cli::default();
    let mut args = args.into_iter();
    let mut options = true;
    while let Some(arg) = args.next() {
        let option = arg.to_str().filter(|text| options && text.len() > 1 && text.starts_with('-'));
        let Some(option) = option else {
            push(&mut cli, arg, false, windows);
            continue;
        };
        match option {
            "--" => options = false,
            "--new-tab" => cli.new_tab = true,
            "--new-window" => cli.new_window = true,
            "--help" | "-h" => cli.help = true,
            "--version" | "-V" => cli.version = true,
            "--select" => match args.next() {
                Some(path) => push(&mut cli, path, true, windows),
                None => cli.warnings.push("--select needs a path".to_owned()),
            },
            other => match other.strip_prefix("--select=") {
                Some(path) => {
                    let path = OsString::from(path);
                    push(&mut cli, path, true, windows);
                }
                None => cli.warnings.push(format!("unknown option {other} (ignored)")),
            },
        }
    }
    if cli.targets.len() > MAX_PATHS {
        cli.targets.truncate(MAX_PATHS);
        cli.warnings.push(format!("only the first {MAX_PATHS} paths are opened"));
    }
    cli
}

fn push(cli: &mut Cli, mut arg: OsString, select: bool, windows: bool) {
    if windows && let Some(cut) = arg.to_str().and_then(|text| text.strip_suffix('"')) {
        arg = OsString::from(cut);
    }
    if !arg.is_empty() {
        cli.targets.push(Target { path: PathBuf::from(arg), select });
    }
}

impl Cli {
    /// Makes the paths absolute against this process's working folder (the running Gezik
    /// does not know it), with `.` and `..` folded away.
    pub fn make_absolute(&mut self) {
        for target in &mut self.targets {
            target.path = crate::start::absolute(std::mem::take(&mut target.path));
        }
    }

    /// What is handed to a running Gezik.
    // Used by the single instance, Task 4.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn request(&self) -> Request {
        // shortcut: the token is carried but not used yet (winit 0.30 cannot activate an
        // existing window with a token from elsewhere); use it once winit can.
        let activation_token =
            if cfg!(all(unix, not(target_os = "macos"))) { std::env::var("XDG_ACTIVATION_TOKEN").ok() } else { None };
        Request { new_tab: self.new_tab, targets: self.targets.clone(), activation_token }
    }
}

/// A tab to show: a folder, and names to select in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Open {
    pub dir: PathBuf,
    pub select: Vec<String>,
}

/// The targets as tabs, in order: a folder opens as is; a file, or anything given with
/// `--select`, opens its folder with it selected; targets in one folder share a tab (the
/// first one's place). Missing paths come back apart.
pub fn group(targets: &[Target], kind: impl Fn(&Path) -> PathKind) -> (Vec<Open>, Vec<PathBuf>) {
    let mut opens: Vec<Open> = Vec::new();
    let mut missing = Vec::new();
    for target in targets {
        let (dir, name) = match kind(&target.path) {
            PathKind::Missing => {
                missing.push(target.path.clone());
                continue;
            }
            PathKind::Dir if !target.select => (target.path.clone(), None),
            _ => match (target.path.parent(), target.path.file_name()) {
                (Some(parent), Some(name)) => (parent.to_path_buf(), Some(name.to_string_lossy().into_owned())),
                // A root given with --select: the root itself.
                _ => (target.path.clone(), None),
            },
        };
        match opens.iter_mut().find(|open| same_path(&open.dir, &dir)) {
            Some(open) => {
                if let Some(name) = name
                    && !open.select.contains(&name)
                {
                    open.select.push(name);
                }
            }
            None => opens.push(Open { dir, select: name.into_iter().collect() }),
        }
    }
    (opens, missing)
}

/// The tab already showing `location` (spec decision 2): the one in front if it does, else
/// the first that does.
pub fn tab_for(tabs: &[Location], active: usize, location: &Location) -> Option<usize> {
    let shows = |tab: &Location| match (tab, location) {
        (Location::Path(a), Location::Path(b)) => same_path(a, b),
        (a, b) => a == b,
    };
    if tabs.get(active).is_some_and(shows) {
        return Some(active);
    }
    tabs.iter().position(shows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    fn t(path: &str, select: bool) -> Target {
        Target { path: PathBuf::from(path), select }
    }

    #[test]
    fn paths_and_flags() {
        let cli = parse(args(&["--new-tab", "/a", "--select", "/b/c.txt", "/d", "--select=/e"]), false);
        assert!(cli.new_tab && !cli.new_window && !cli.help && !cli.version);
        assert_eq!(cli.targets, [t("/a", false), t("/b/c.txt", true), t("/d", false), t("/e", true)]);
        assert!(cli.warnings.is_empty());
        let cli = parse(args(&["--new-window", "-h", "-V"]), false);
        assert!(cli.new_window && cli.help && cli.version && cli.targets.is_empty());
        assert!(parse(args(&["--help"]), false).help);
        assert!(parse(args(&["--version"]), false).version);
    }

    #[test]
    fn an_unknown_flag_warns_and_gezik_still_opens() {
        let cli = parse(args(&["--frobnicate", "/a"]), false);
        assert_eq!(cli.targets, [t("/a", false)]);
        assert_eq!(cli.warnings, ["unknown option --frobnicate (ignored)"]);
        let cli = parse(args(&["--select"]), false);
        assert_eq!(cli.warnings, ["--select needs a path"]);
    }

    #[test]
    fn double_dash_ends_the_options() {
        let cli = parse(args(&["--", "--new-tab", "-x"]), false);
        assert!(!cli.new_tab);
        assert_eq!(cli.targets, [t("--new-tab", false), t("-x", false)]);
        assert_eq!(parse(args(&["-"]), false).targets, [t("-", false)], "a lone dash is a name");
    }

    #[test]
    fn a_trailing_quote_from_cmd_is_dropped() {
        // `gezik "D:\Work\"` arrives as `D:\Work"`: `\"` is an escaped quote to CommandLineToArgvW.
        assert_eq!(parse(args(&[r#"D:\Work""#]), true).targets, [t(r"D:\Work", false)]);
        assert_eq!(parse(args(&[r#"/a""#]), false).targets, [t(r#"/a""#, false)], "a Unix name may end in a quote");
        assert!(parse(args(&["", r#"""#]), true).targets.is_empty(), "empty arguments are nothing");
    }

    #[test]
    fn at_most_a_thousand_paths() {
        let many: Vec<OsString> = (0..1005).map(|i| OsString::from(format!("/p{i}"))).collect();
        let cli = parse(many, false);
        assert_eq!(cli.targets.len(), gezik_platform::instance::MAX_PATHS);
        assert_eq!(cli.warnings, ["only the first 1000 paths are opened"]);
    }

    #[test]
    fn relative_paths_become_absolute_before_sending() {
        let cwd = std::env::current_dir().unwrap();
        let mut cli = parse(args(&[".", "sub/../x", "--select", "y.txt"]), cfg!(windows));
        cli.make_absolute();
        assert_eq!(
            cli.targets,
            [
                Target { path: cwd.clone(), select: false },
                Target { path: cwd.join("x"), select: false },
                Target { path: cwd.join("y.txt"), select: true },
            ]
        );
        let request = cli.request();
        assert_eq!(request.targets, cli.targets);
        assert!(!request.new_tab);
    }

    /// `/w` and `/w/sub` are folders, `/w/a.txt` and `/w/b.txt` files, the rest missing.
    fn kind(path: &Path) -> PathKind {
        match path.to_str() {
            Some("/w" | "/w/sub" | "/") => PathKind::Dir,
            Some("/w/a.txt" | "/w/b.txt") => PathKind::File,
            _ => PathKind::Missing,
        }
    }

    fn open(dir: &str, select: &[&str]) -> Open {
        Open { dir: PathBuf::from(dir), select: select.iter().map(|s| (*s).to_owned()).collect() }
    }

    #[test]
    fn a_folder_opens_and_a_file_opens_its_folder_selected() {
        let (opens, missing) = group(&[t("/w/sub", false), t("/w/a.txt", false)], kind);
        assert_eq!(opens, [open("/w/sub", &[]), open("/w", &["a.txt"])]);
        assert!(missing.is_empty());
    }

    #[test]
    fn select_shows_a_folder_in_its_parent() {
        let (opens, _) = group(&[t("/w/sub", true)], kind);
        assert_eq!(opens, [open("/w", &["sub"])]);
        let (opens, _) = group(&[t("/", true)], kind);
        assert_eq!(opens, [open("/", &[])], "a root has no parent: itself");
    }

    #[test]
    fn targets_in_one_folder_share_a_tab() {
        let (opens, _) =
            group(&[t("/w/a.txt", false), t("/w/sub", false), t("/w/b.txt", true), t("/w/a.txt", true)], kind);
        assert_eq!(opens, [open("/w", &["a.txt", "b.txt"]), open("/w/sub", &[])]);
        let (opens, _) = group(&[t("/w", false), t("/w/a.txt", false)], kind);
        assert_eq!(opens, [open("/w", &["a.txt"])], "the folder and a file in it: one tab");
    }

    #[test]
    fn missing_paths_are_set_apart() {
        let (opens, missing) = group(&[t("/gone", false), t("/w", false)], kind);
        assert_eq!(opens, [open("/w", &[])]);
        assert_eq!(missing, [PathBuf::from("/gone")]);
    }

    fn p(path: &str) -> Location {
        Location::Path(PathBuf::from(path))
    }

    #[test]
    fn the_tab_in_front_wins_then_the_first() {
        let tabs = [p("/a"), p("/w"), p("/b"), p("/w")];
        assert_eq!(tab_for(&tabs, 3, &p("/w")), Some(3), "in front already");
        assert_eq!(tab_for(&tabs, 0, &p("/w")), Some(1), "else the first");
        assert_eq!(tab_for(&tabs, 0, &p("/x")), None);
        assert_eq!(tab_for(&[], 0, &p("/x")), None);
        assert_eq!(tab_for(&[p("/a"), Location::Drives], 0, &Location::Drives), Some(1), "This PC too");
    }

    #[cfg(windows)]
    #[test]
    fn paths_compare_as_the_system_does() {
        let tabs = [p(r"C:\Work")];
        assert_eq!(tab_for(&tabs, 0, &p(r"c:\work\")), Some(0));
    }
}
