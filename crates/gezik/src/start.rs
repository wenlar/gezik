//! Where the app opens: the command-line path, the `start-folder` setting and the tabs of
//! last time (spec 5.1). Pure: the file system is reached only through the closures passed
//! in, so every rule is tested here.

use std::path::{Component, Path, PathBuf};

use gezik_config::Warning;
use gezik_core::nav::{Location, Session, SessionTab};

use crate::cli::{Target, group, tab_for};

/// What a path on disk is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathKind {
    Dir,
    File,
    Missing,
}

/// The resolved start: the tabs that open (the one in front among them) and where new tabs
/// open.
#[derive(Debug, Clone, PartialEq)]
pub struct StartPlan {
    /// The tabs to open: the saved session, or one tab.
    pub session: Session,
    /// Names to select in the tab in front (files given on the command line).
    pub select: Vec<String>,
    /// The `start-folder` setting, resolved: where new tabs open. Never the command line.
    pub start: Location,
    pub warnings: Vec<Warning>,
}

impl StartPlan {
    /// Where the tab in front opens (the tests read it; the navigator takes the whole session).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn first(&self) -> &Location {
        self.session.active_location().unwrap_or(&self.start)
    }
}

/// Resolves the `start-folder` setting (`"drives"` or a path with tokens), falling back to
/// `home` with a warning when it is not a folder.
pub fn resolve_start_folder(
    setting: &str,
    home: &Path,
    expand: impl Fn(&str) -> Option<PathBuf>,
    kind: impl Fn(&Path) -> PathKind,
) -> (Location, Option<Warning>) {
    let text = setting.trim();
    if text.eq_ignore_ascii_case("drives") {
        return (Location::Drives, None);
    }
    match expand(text) {
        Some(path) if kind(&path) == PathKind::Dir => (Location::Path(path), None),
        _ => (
            Location::Path(home.to_path_buf()),
            Some(Warning::new("settings.toml", format!("start-folder: \"{text}\" is not a folder; using home"))),
        ),
    }
}

/// The saved tabs (`saved`, when `[session] restore` is on and there are any) come back as
/// they were; each command-line target then goes to the tab already showing its folder
/// (unless `new_tab`) or to a new tab at the end, and the last one is in front (spec 5.1,
/// decision 2). Without saved tabs the targets replace the start tab. A file opens its folder
/// with it selected; only the tab in front gets its selection. Missing paths warn. The saved
/// tabs are not looked at on disk. The setting is always resolved, for new tabs.
#[allow(clippy::too_many_arguments, reason = "the plan's inputs, each a plain value the tests set")]
pub fn plan_start(
    setting: &str,
    targets: &[Target],
    new_tab: bool,
    trash: bool,
    home: &Path,
    expand: impl Fn(&str) -> Option<PathBuf>,
    kind: impl Fn(&Path) -> PathKind,
    saved: Option<&Session>,
) -> StartPlan {
    let (start, warning) = resolve_start_folder(setting, home, expand, &kind);
    let restored = saved.filter(|session| !session.is_empty()).cloned();
    let from_session = restored.is_some();
    let mut plan = StartPlan {
        session: restored.unwrap_or_else(|| Session::single(start.clone())),
        select: Vec::new(),
        start,
        warnings: warning.into_iter().collect(),
    };
    let (opens, missing) = group(targets, &kind);
    let opening = if from_session { "opening the last tabs" } else { "opening start-folder" };
    for path in missing {
        plan.warnings.push(Warning::new("command line", format!("{}: not found; {opening}", path.display())));
    }
    if opens.is_empty() && !trash {
        return plan;
    }
    plan.select = opens.last().map(|o| o.select.clone()).unwrap_or_default();
    // shortcut: only the tab in front gets its names selected; others with files lose
    // theirs (several folders with files at start is rare).
    if !from_session {
        plan.session.tabs.clear();
    }
    for open in opens {
        let location = Location::Path(open.dir);
        let shown: Vec<Location> = plan.session.tabs.iter().map(|tab| tab.location.clone()).collect();
        match tab_for(&shown, plan.session.active, &location).filter(|_| !new_tab) {
            Some(index) => plan.session.active = index,
            None => {
                plan.session.tabs.push(SessionTab { location, locked: false });
                plan.session.active = plan.session.tabs.len() - 1;
            }
        }
    }
    if trash {
        let shown: Vec<Location> = plan.session.tabs.iter().map(|tab| tab.location.clone()).collect();
        match tab_for(&shown, plan.session.active, &Location::Trash).filter(|_| !new_tab) {
            Some(index) => plan.session.active = index,
            None => {
                plan.session.tabs.push(SessionTab { location: Location::Trash, locked: false });
                plan.session.active = plan.session.tabs.len() - 1;
            }
        }
    }
    plan
}

/// [`PathKind`] from the real file system.
pub fn path_kind(path: &Path) -> PathKind {
    // A bare `\\server` has no metadata; Windows lists its shares there (9b6).
    if cfg!(windows) && gezik_core::path_text::server_only(&path.to_string_lossy()).is_some() {
        return PathKind::Dir;
    }
    match std::fs::metadata(path) {
        Ok(meta) if meta.is_dir() => PathKind::Dir,
        Ok(_) => PathKind::File,
        Err(_) => PathKind::Missing,
    }
}

/// `path` made absolute with `.` and `..` folded away by name: on Unix
/// `std::path::absolute` keeps `..`, so `gezik ..` would open `/home/u/proj/..`.
pub fn absolute(path: PathBuf) -> PathBuf {
    // A bare `\\server` is absolute already; Windows would garble it.
    if cfg!(windows) && gezik_core::path_text::server_only(&path.to_string_lossy()).is_some() {
        return path;
    }
    let Ok(full) = std::path::absolute(&path) else { return path };
    let mut out = PathBuf::new();
    for component in full.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home() -> PathBuf {
        PathBuf::from("/home/u")
    }

    /// `{home}` expands to `/home/u`; `..` is refused like `KnownDirs::expand_checked`.
    fn expand(text: &str) -> Option<PathBuf> {
        if text.contains("..") {
            return None;
        }
        Some(PathBuf::from(text.replace("{home}", "/home/u")))
    }

    fn kind(path: &Path) -> PathKind {
        match path.to_str() {
            Some("/home/u" | "/home/u/Docs" | "/work" | "/work/sub") => PathKind::Dir,
            Some("/work/notes.txt") => PathKind::File,
            _ => PathKind::Missing,
        }
    }

    fn plan(setting: &str, cli: Option<&str>) -> StartPlan {
        let targets: Vec<Target> = cli.map(|p| Target { path: PathBuf::from(p), select: false }).into_iter().collect();
        plan_start(setting, &targets, false, false, &home(), expand, kind, None)
    }

    fn targets(paths: &[&str]) -> Vec<Target> {
        paths.iter().map(|p| Target { path: PathBuf::from(p), select: false }).collect()
    }

    fn p(path: &str) -> Location {
        Location::Path(PathBuf::from(path))
    }

    #[test]
    fn setting_without_command_line() {
        let docs = plan("{home}/Docs", None);
        assert_eq!(
            (docs.first().clone(), docs.start, docs.select),
            (p("/home/u/Docs"), p("/home/u/Docs"), Vec::<String>::new())
        );
        assert!(docs.warnings.is_empty());

        let drives = plan("  Drives ", None);
        assert_eq!((drives.first().clone(), drives.start), (Location::Drives, Location::Drives));
        assert!(drives.warnings.is_empty());
    }

    #[test]
    fn invalid_setting_warns_and_uses_home() {
        for setting in ["{home}/../x", "/nowhere", "/work/notes.txt"] {
            let plan = plan(setting, None);
            assert_eq!((plan.first().clone(), plan.start), (p("/home/u"), p("/home/u")), "{setting}");
            assert_eq!(plan.warnings.len(), 1, "{setting}");
            assert!(plan.warnings[0].message.contains("is not a folder"), "{setting}");
        }
    }

    #[test]
    fn command_line_folder_wins_but_new_tabs_use_the_setting() {
        let plan = plan("drives", Some("/work/sub"));
        assert_eq!(
            (plan.first().clone(), plan.start, plan.select),
            (p("/work/sub"), Location::Drives, Vec::<String>::new())
        );
        assert!(plan.warnings.is_empty());
    }

    #[test]
    fn command_line_file_opens_its_folder_selected() {
        let plan = plan("{home}", Some("/work/notes.txt"));
        assert_eq!(plan.first().clone(), p("/work"));
        assert_eq!(plan.select, ["notes.txt"]);
        assert_eq!(plan.start, p("/home/u"));
    }

    #[test]
    fn missing_command_line_path_warns_and_opens_the_setting() {
        let plan = plan("drives", Some("/gone"));
        assert_eq!(
            (plan.first().clone(), plan.start, plan.select),
            (Location::Drives, Location::Drives, Vec::<String>::new())
        );
        assert_eq!(plan.warnings.len(), 1);
        assert_eq!(plan.warnings[0].file, "command line");
        assert!(plan.warnings[0].message.ends_with("not found; opening start-folder"));
    }

    #[test]
    fn invalid_setting_is_reported_even_with_a_command_line_path() {
        let plan = plan("/nowhere", Some("/work"));
        assert_eq!((plan.first().clone(), plan.start), (p("/work"), p("/home/u")));
        assert_eq!(plan.warnings.len(), 1);
        assert_eq!(plan.warnings[0].file, "settings.toml");
    }

    fn saved() -> Session {
        Session {
            tabs: vec![
                SessionTab { location: p("/gone/far"), locked: true },
                SessionTab { location: Location::Drives, locked: false },
            ],
            active: 1,
        }
    }

    #[test]
    fn the_saved_tabs_come_back_and_new_tabs_still_open_in_the_setting() {
        let plan = plan_start("{home}/Docs", &[], false, false, &home(), expand, kind, Some(&saved()));
        assert_eq!(plan.session, saved());
        assert_eq!(
            (plan.first().clone(), plan.start.clone(), plan.select.clone()),
            (Location::Drives, p("/home/u/Docs"), Vec::new())
        );
        assert!(plan.warnings.is_empty());
    }

    #[test]
    fn a_command_line_path_joins_the_saved_tabs_at_the_end() {
        let folder =
            plan_start("{home}", &targets(&["/work/sub"]), false, false, &home(), expand, kind, Some(&saved()));
        assert_eq!(folder.session.tabs.len(), 3);
        assert_eq!((folder.session.active, folder.first().clone()), (2, p("/work/sub")));
        assert!(folder.session.tabs[0].locked, "the saved locks stay");
        let file =
            plan_start("{home}", &targets(&["/work/notes.txt"]), false, false, &home(), expand, kind, Some(&saved()));
        assert_eq!((file.first().clone(), file.select.clone()), (p("/work"), vec!["notes.txt".to_owned()]));
        let gone = plan_start("{home}", &targets(&["/nowhere"]), false, false, &home(), expand, kind, Some(&saved()));
        assert_eq!(gone.session, saved(), "the session still comes");
        assert!(gone.warnings[0].message.ends_with("not found; opening the last tabs"), "{:?}", gone.warnings);
    }

    #[test]
    fn a_saved_session_is_never_looked_at_on_disk() {
        let asked = std::cell::RefCell::new(Vec::new());
        let counting = |path: &Path| {
            asked.borrow_mut().push(path.to_path_buf());
            kind(path)
        };
        let plan = plan_start("{home}", &[], false, false, &home(), expand, counting, Some(&saved()));
        assert_eq!(plan.session, saved());
        assert_eq!(*asked.borrow(), [PathBuf::from("/home/u")], "only start-folder is looked at");
    }

    #[test]
    fn an_empty_session_is_todays_start() {
        let plan =
            plan_start("drives", &targets(&["/work"]), false, false, &home(), expand, kind, Some(&Session::default()));
        assert_eq!(plan.session, Session::single(p("/work")));
        let plain = plan_start("drives", &[], false, false, &home(), expand, kind, None);
        assert_eq!(plain.session, Session::single(Location::Drives));
    }

    #[test]
    fn several_paths_open_a_tab_each_the_last_in_front() {
        let plan = plan_start(
            "drives",
            &targets(&["/work", "/work/sub", "/home/u"]),
            false,
            false,
            &home(),
            expand,
            kind,
            None,
        );
        let shown: Vec<_> = plan.session.tabs.iter().map(|t| t.location.clone()).collect();
        assert_eq!(shown, [p("/work"), p("/work/sub"), p("/home/u")]);
        assert_eq!(plan.session.active, 2);
        assert_eq!(plan.start, Location::Drives, "new tabs still open in the setting");
    }

    #[test]
    fn a_saved_tab_showing_the_folder_is_reused_at_start() {
        let saved = Session {
            tabs: vec![
                SessionTab { location: p("/work"), locked: false },
                SessionTab { location: p("/home/u"), locked: false },
            ],
            active: 1,
        };
        let plan = plan_start("{home}", &targets(&["/work"]), false, false, &home(), expand, kind, Some(&saved));
        assert_eq!(plan.session.tabs.len(), 2, "no second /work tab");
        assert_eq!(plan.session.active, 0);
        let file =
            plan_start("{home}", &targets(&["/work/notes.txt"]), false, false, &home(), expand, kind, Some(&saved));
        assert_eq!((file.session.tabs.len(), file.session.active), (2, 0));
        assert_eq!(file.select, ["notes.txt"]);
    }

    #[test]
    fn new_tab_always_adds() {
        let saved = Session { tabs: vec![SessionTab { location: p("/work"), locked: false }], active: 0 };
        let plan = plan_start("{home}", &targets(&["/work"]), true, false, &home(), expand, kind, Some(&saved));
        assert_eq!((plan.session.tabs.len(), plan.session.active), (2, 1));
    }

    #[test]
    fn the_trash_opens_in_its_own_tab_or_the_one_showing_it() {
        let alone = plan_start("drives", &[], false, true, &home(), expand, kind, None);
        assert_eq!(alone.session, Session::single(Location::Trash), "replaces the start tab");
        let trash = SessionTab { location: Location::Trash, locked: false };
        let saved = Session { tabs: vec![SessionTab { location: p("/work"), locked: false }, trash], active: 0 };
        let plan = plan_start("{home}", &[], false, true, &home(), expand, kind, Some(&saved));
        assert_eq!((plan.session.tabs.len(), plan.session.active), (2, 1), "the tab already showing it");
        let plan = plan_start("{home}", &[], true, true, &home(), expand, kind, Some(&saved));
        assert_eq!((plan.session.tabs.len(), plan.session.active), (3, 2), "--new-tab");
    }

    #[test]
    fn only_missing_paths_leave_the_start_as_it_was() {
        let plan = plan_start("drives", &targets(&["/gone", "/nowhere"]), false, false, &home(), expand, kind, None);
        assert_eq!(plan.session, Session::single(Location::Drives));
        assert_eq!(plan.warnings.len(), 2);
        assert!(
            plan.warnings
                .iter()
                .all(|w| w.file == "command line" && w.message.ends_with("not found; opening start-folder"))
        );
    }

    #[test]
    fn a_command_line_path_folds_its_dots() {
        let cwd = std::env::current_dir().unwrap();
        assert_eq!(absolute(PathBuf::from("..")), cwd.parent().unwrap());
        assert_eq!(absolute(PathBuf::from("./a/../b")), cwd.join("b"));
        let root = absolute(PathBuf::from("/"));
        assert_eq!(absolute(root.join("..")), root, "no higher than the root");
    }

    /// `gezik --shell \\server` (Explorer's folder verb): a bare server stays one through
    /// `make_absolute` and opens as a folder, asked of no network.
    #[cfg(windows)]
    #[test]
    fn a_bare_server_opens_as_a_folder() {
        for text in [r"\\no-such-host-9b6", r"\\no-such-host-9b6\"] {
            let path = absolute(PathBuf::from(text));
            assert_eq!(gezik_core::path_text::server_only(&path.to_string_lossy()), Some("no-such-host-9b6"), "{text}");
            assert_eq!(path_kind(&path), PathKind::Dir, "{text}");
        }
    }
}
