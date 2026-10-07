//! Where the app opens: the command-line path, the `start-folder` setting and the tabs of
//! last time (spec 5.1). Pure: the file system is reached only through the closures passed
//! in, so every rule is tested here.

use std::path::{Path, PathBuf};

use gezik_config::Warning;
use gezik_core::nav::{Location, Session, SessionTab};

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
    /// Entry to select in the tab in front (a file given on the command line).
    pub select: Option<String>,
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
/// they were, and a command-line path opens after them, in front; without them, the
/// command-line path wins as before: a folder opens as is, a file opens its folder with the
/// file selected, a missing path warns. The saved tabs are not looked at on disk (a gone
/// folder falls back when it is shown). The setting is always resolved, for new tabs.
pub fn plan_start(
    setting: &str,
    cli: Option<PathBuf>,
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
        select: None,
        start,
        warnings: warning.into_iter().collect(),
    };
    let Some(path) = cli else { return plan };
    let (location, select) = match kind(&path) {
        PathKind::Dir => (Location::Path(path), None),
        PathKind::File => match (path.parent().filter(|p| !p.as_os_str().is_empty()), path.file_name()) {
            (Some(parent), Some(name)) => {
                (Location::Path(parent.to_path_buf()), Some(name.to_string_lossy().into_owned()))
            }
            _ => return plan,
        },
        PathKind::Missing => {
            let opening = if from_session { "opening the last tabs" } else { "opening start-folder" };
            plan.warnings.push(Warning::new("command line", format!("{}: not found; {opening}", path.display())));
            return plan;
        }
    };
    if from_session {
        plan.session.tabs.push(SessionTab { location, locked: false });
        plan.session.active = plan.session.tabs.len() - 1;
    } else {
        plan.session = Session::single(location);
    }
    plan.select = select;
    plan
}

/// [`PathKind`] from the real file system.
pub fn path_kind(path: &Path) -> PathKind {
    match std::fs::metadata(path) {
        Ok(meta) if meta.is_dir() => PathKind::Dir,
        Ok(_) => PathKind::File,
        Err(_) => PathKind::Missing,
    }
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
        plan_start(setting, cli.map(PathBuf::from), &home(), expand, kind, None)
    }

    fn p(path: &str) -> Location {
        Location::Path(PathBuf::from(path))
    }

    #[test]
    fn setting_without_command_line() {
        let docs = plan("{home}/Docs", None);
        assert_eq!((docs.first().clone(), docs.start, docs.select), (p("/home/u/Docs"), p("/home/u/Docs"), None));
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
        assert_eq!((plan.first().clone(), plan.start, plan.select), (p("/work/sub"), Location::Drives, None));
        assert!(plan.warnings.is_empty());
    }

    #[test]
    fn command_line_file_opens_its_folder_selected() {
        let plan = plan("{home}", Some("/work/notes.txt"));
        assert_eq!(plan.first().clone(), p("/work"));
        assert_eq!(plan.select.as_deref(), Some("notes.txt"));
        assert_eq!(plan.start, p("/home/u"));
    }

    #[test]
    fn missing_command_line_path_warns_and_opens_the_setting() {
        let plan = plan("drives", Some("/gone"));
        assert_eq!((plan.first().clone(), plan.start, plan.select), (Location::Drives, Location::Drives, None));
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
        let plan = plan_start("{home}/Docs", None, &home(), expand, kind, Some(&saved()));
        assert_eq!(plan.session, saved());
        assert_eq!(
            (plan.first().clone(), plan.start.clone(), plan.select.clone()),
            (Location::Drives, p("/home/u/Docs"), None)
        );
        assert!(plan.warnings.is_empty());
    }

    #[test]
    fn a_command_line_path_joins_the_saved_tabs_at_the_end() {
        let folder = plan_start("{home}", Some(PathBuf::from("/work/sub")), &home(), expand, kind, Some(&saved()));
        assert_eq!(folder.session.tabs.len(), 3);
        assert_eq!((folder.session.active, folder.first().clone()), (2, p("/work/sub")));
        assert!(folder.session.tabs[0].locked, "the saved locks stay");
        let file = plan_start("{home}", Some(PathBuf::from("/work/notes.txt")), &home(), expand, kind, Some(&saved()));
        assert_eq!((file.first().clone(), file.select.as_deref()), (p("/work"), Some("notes.txt")));
        let gone = plan_start("{home}", Some(PathBuf::from("/nowhere")), &home(), expand, kind, Some(&saved()));
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
        let plan = plan_start("{home}", None, &home(), expand, counting, Some(&saved()));
        assert_eq!(plan.session, saved());
        assert_eq!(*asked.borrow(), [PathBuf::from("/home/u")], "only start-folder is looked at");
    }

    #[test]
    fn an_empty_session_is_todays_start() {
        let plan = plan_start("drives", Some(PathBuf::from("/work")), &home(), expand, kind, Some(&Session::default()));
        assert_eq!(plan.session, Session::single(p("/work")));
        let plain = plan_start("drives", None, &home(), expand, kind, None);
        assert_eq!(plain.session, Session::single(Location::Drives));
    }
}
