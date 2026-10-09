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
  --unregister      undo every system change Gezik made (see system-changes.toml), then exit;
                    exit code 0: all undone, 1: some left as they were, 2: an error
                    (in cmd: start /wait gezik --unregister)
  --version, --help
";

/// The parsed command line.
#[derive(Debug, Default, PartialEq)]
pub struct Cli {
    pub targets: Vec<Target>,
    pub new_tab: bool,
    // Used by the single instance, Task 4.
    pub new_window: bool,
    pub help: bool,
    pub version: bool,
    /// `--unregister`: undo the system changes and exit (spec 11.4); never handed to a running Gezik.
    pub unregister: bool,
    /// `--shell TARGET` (Windows' folder verb and Win+E, spec 6.1): mapped by [`shell_target`].
    pub shell: Option<String>,
    /// `--dbus`: started by the session bus for FileManager1 (spec 6.4).
    pub dbus: bool,
    /// Open the Recycle Bin / Trash (from `--shell`; spec 6.1).
    pub trash: bool,
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
            "--unregister" => cli.unregister = true,
            "--shell" => cli.shell = Some(args.next().and_then(|a| a.into_string().ok()).unwrap_or_default()),
            "--dbus" => cli.dbus = true,
            // macOS before 10.9 passed the process serial number to apps opened by Finder.
            other if other.starts_with("-psn_") => {}
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
    pub fn request(&self) -> Request {
        // shortcut: the token is carried but not used yet (winit 0.30 cannot activate an
        // existing window with a token from elsewhere); use it once winit can.
        let activation_token =
            if cfg!(all(unix, not(target_os = "macos"))) { std::env::var("XDG_ACTIVATION_TOKEN").ok() } else { None };
        let targets = self
            .targets
            .iter()
            .filter_map(|t| Some(Target { path: hand_over_path(&t.path, cfg!(windows))?, select: t.select }))
            .collect();
        Request { new_tab: self.new_tab, targets, activation_token, trash: self.trash }
    }
}

/// `path` as the running Gezik takes it. On Windows it refuses a whole request holding a
/// device path (`\\?\`, `\\.\`, `\??\`), so `\\?\C:\x` goes as `C:\x`, `\\?\UNC\s\x` as
/// `\\s\x`, and other device paths are left out.
fn hand_over_path(path: &Path, windows: bool) -> Option<PathBuf> {
    if !windows {
        return Some(path.to_path_buf());
    }
    let start: String = path.to_string_lossy().chars().take(4).map(|c| if c == '/' { '\\' } else { c }).collect();
    if ![r"\\.\", r"\\?\", r"\??\"].contains(&start.as_str()) {
        return Some(path.to_path_buf());
    }
    let rest = path.to_str()?.get(4..)?;
    if start == r"\\.\" {
        return None;
    }
    if rest.get(..4).is_some_and(|unc| unc.eq_ignore_ascii_case(r"UNC\")) {
        return Some(PathBuf::from(format!(r"\\{}", &rest[4..])));
    }
    let drive = rest.as_bytes();
    (drive.len() >= 3 && drive[0].is_ascii_alphabetic() && drive[1] == b':' && drive[2] == b'\\')
        .then(|| PathBuf::from(rest))
}

/// Where a `--shell` target goes (spec 6.1's table).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shell {
    Open(Target),
    Trash,
    StartFolder,
    /// A place only Explorer shows (spec 6.3).
    Explorer,
}

const RECYCLE_BIN: &str = "::{645ff040-5081-101b-9f08-00aa002f954e}";
/// This PC, Home, Quick access: Gezik's start folder (spec 6.1).
const START_PLACES: [&str; 3] = [
    "::{20d04fe0-3aea-1069-a2d8-08002b30309d}",
    "::{f874310e-b6b7-47dc-bc84-b9e6b38f5903}",
    "::{679f85cb-0220-4080-b29b-5540cc05aab6}",
];

/// Maps what the shell passed (`%1`, or "" for Win+E). CLSIDs without case; `shell:` names
/// other than the Recycle Bin are left to Explorer (the shell resolves known folders to paths
/// before it gets here). A file opens its folder only if Gezik can open it as an archive.
/// Any app may start `gezik --shell X`: X is only looked at here, never run.
pub fn shell_target(text: &str, kind: impl Fn(&Path) -> PathKind) -> Shell {
    // `"C:\"` arrives as `C:"` (see `parse`).
    let text = text.trim().trim_end_matches('"');
    if text.is_empty() {
        return Shell::StartFolder;
    }
    let lower = text.to_lowercase();
    let name = lower.strip_prefix("shell:").unwrap_or(&lower);
    if name == RECYCLE_BIN || name.starts_with(&format!("{RECYCLE_BIN}\\")) || name == "recyclebinfolder" {
        return Shell::Trash;
    }
    if START_PLACES.contains(&name) {
        return Shell::StartFolder;
    }
    let path = match text.as_bytes() {
        [letter, b':'] if letter.is_ascii_alphabetic() => format!("{text}\\"),
        _ => text.to_owned(),
    };
    // shortcut: a bare `\\server` goes to Explorer until 9b6 lists its shares.
    if !plain_path(&path) {
        return Shell::Explorer;
    }
    let path = PathBuf::from(path);
    match kind(&path) {
        PathKind::Dir => Shell::Open(Target { path, select: false }),
        PathKind::File
            if path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| gezik_core::kind::Kind::of(n, false) == gezik_core::kind::Kind::Archive) =>
        {
            Shell::Open(Target { path, select: true })
        }
        _ => Shell::Explorer,
    }
}

/// `X:\…`, or `\\server\share…`; no device path, no bare server, nothing relative.
fn plain_path(text: &str) -> bool {
    match text.as_bytes() {
        [letter, b':', b'\\', ..] => letter.is_ascii_alphabetic(),
        [b'\\', b'\\', rest @ ..] => {
            let mut parts = std::str::from_utf8(rest).unwrap_or("").split('\\');
            let server = parts.next().unwrap_or("");
            let share = parts.next().unwrap_or("");
            !server.is_empty() && server != "?" && server != "." && !share.is_empty()
        }
        _ => false,
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
    fn shell_targets_map_as_the_table_says() {
        let kind = |p: &Path| match p.to_str().unwrap_or("") {
            r"C:\" | r"D:\Work" | r"\\srv\share\x" => PathKind::Dir,
            r"D:\a.zip" | r"D:\Docs.library-ms" | r"D:\a.txt" => PathKind::File,
            _ => PathKind::Missing,
        };
        let open = |p: &str, select: bool| Shell::Open(Target { path: PathBuf::from(p), select });
        let cases = [
            ("", Shell::StartFolder),
            (r#"C:""#, open(r"C:\", false)),
            ("C:", open(r"C:\", false)),
            (r"D:\Work", open(r"D:\Work", false)),
            (r"\\srv\share\x", open(r"\\srv\share\x", false)),
            ("::{645FF040-5081-101B-9F08-00AA002F954E}", Shell::Trash),
            ("::{645ff040-5081-101b-9f08-00aa002f954e}", Shell::Trash),
            (r"::{645FF040-5081-101B-9F08-00AA002F954E}\x", Shell::Trash),
            ("shell:RecycleBinFolder", Shell::Trash),
            ("shell:::{20D04FE0-3AEA-1069-A2D8-08002B30309D}", Shell::StartFolder),
            ("::{20D04FE0-3AEA-1069-A2D8-08002B30309D}", Shell::StartFolder),
            ("::{F874310E-B6B7-47DC-BC84-B9E6B38F5903}", Shell::StartFolder),
            ("::{679F85CB-0220-4080-B29B-5540CC05AAB6}", Shell::StartFolder),
            (r"::{20D04FE0-3AEA-1069-A2D8-08002B30309D}\::{F02C1A0D-BE21-4350-88B0-7367FC96EF3C}", Shell::Explorer),
            ("::{26EE0668-A00A-44D7-9371-BEB064C98683}", Shell::Explorer),
            (r"\\srv", Shell::Explorer),
            (r"\\srv\", Shell::Explorer),
            (r"\\?\C:\x", Shell::Explorer),
            (r"D:\a.zip", open(r"D:\a.zip", true)),
            (r"D:\Docs.library-ms", Shell::Explorer),
            (r"D:\a.txt", Shell::Explorer),
            (r"D:\gone", Shell::Explorer),
            ("shell:Downloads", Shell::Explorer),
            ("relative", Shell::Explorer),
        ];
        for (text, want) in cases {
            assert_eq!(shell_target(text, kind), want, "{text}");
        }
    }

    #[test]
    fn hidden_flags_parse() {
        let cli = parse(args(&["--shell", r"D:\x"]), true);
        assert_eq!(cli.shell.as_deref(), Some(r"D:\x"));
        assert!(cli.targets.is_empty() && cli.warnings.is_empty());
        assert_eq!(parse(args(&["--shell"]), true).shell.as_deref(), Some(""));
        assert!(parse(args(&["--dbus"]), false).dbus);
        assert!(parse(args(&["-psn_0_12345"]), false).warnings.is_empty(), "macOS launch argument");
        assert!(!HELP.contains("--shell") && !HELP.contains("--dbus"));
        assert!(Cli { trash: true, ..Cli::default() }.request().trash, "the trash is handed over");
    }

    #[test]
    fn unregister_is_a_flag_of_its_own() {
        let cli = parse(args(&["--unregister"]), true);
        assert!(cli.unregister && cli.targets.is_empty() && cli.warnings.is_empty());
        assert!(HELP.contains("--unregister"));
    }

    #[test]
    fn device_paths_are_handed_over_plain_or_left_out() {
        let over = |p: &str| hand_over_path(Path::new(p), true).map(|p| p.to_string_lossy().into_owned());
        assert_eq!(over(r"\\?\C:\x\y").as_deref(), Some(r"C:\x\y"));
        assert_eq!(over(r"\??\D:\").as_deref(), Some(r"D:\"));
        assert_eq!(over(r"\\?\UNC\server\share\x").as_deref(), Some(r"\\server\share\x"));
        assert_eq!(over(r"C:\plain").as_deref(), Some(r"C:\plain"));
        assert_eq!(over(r"\\server\share").as_deref(), Some(r"\\server\share"));
        for gone in [r"\\.\PhysicalDrive0", r"//./pipe/x", r"\\?\Volume{0}\", r"\\?\"] {
            assert_eq!(over(gone), None, "{gone}");
        }
        assert_eq!(hand_over_path(Path::new(r"\\?\C:\x"), false), Some(PathBuf::from(r"\\?\C:\x")));
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
