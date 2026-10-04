//! Where the app opens: the command-line path and the `start-folder` setting. Pure: the
//! file system is reached only through the closures passed in, so every rule is tested here.

use std::path::{Path, PathBuf};

use gezik_config::Warning;
use gezik_core::nav::Location;

/// What a path on disk is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathKind {
    Dir,
    File,
    Missing,
}

/// The resolved start: where the first tab opens and where new tabs open.
#[derive(Debug, Clone, PartialEq)]
pub struct StartPlan {
    /// The first tab's location.
    pub first: Location,
    /// Entry to select in the first tab (a file given on the command line).
    pub select: Option<String>,
    /// The `start-folder` setting, resolved: where new tabs open. Never the command line.
    pub start: Location,
    pub warnings: Vec<Warning>,
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

/// The command-line path wins: a folder opens as is; a file opens its folder with the file
/// selected; a missing path warns and opens `start-folder`. The setting is always resolved
/// (and warned about) so new tabs have a valid start.
pub fn plan_start(
    setting: &str,
    cli: Option<PathBuf>,
    home: &Path,
    expand: impl Fn(&str) -> Option<PathBuf>,
    kind: impl Fn(&Path) -> PathKind,
) -> StartPlan {
    let (start, warning) = resolve_start_folder(setting, home, expand, &kind);
    let mut plan = StartPlan { first: start.clone(), select: None, start, warnings: warning.into_iter().collect() };
    let Some(path) = cli else { return plan };
    match kind(&path) {
        PathKind::Dir => plan.first = Location::Path(path),
        PathKind::File => {
            if let (Some(parent), Some(name)) = (path.parent().filter(|p| !p.as_os_str().is_empty()), path.file_name())
            {
                plan.first = Location::Path(parent.to_path_buf());
                plan.select = Some(name.to_string_lossy().into_owned());
            }
        }
        PathKind::Missing => plan
            .warnings
            .push(Warning::new("command line", format!("{}: not found; opening start-folder", path.display()))),
    }
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
        plan_start(setting, cli.map(PathBuf::from), &home(), expand, kind)
    }

    fn p(path: &str) -> Location {
        Location::Path(PathBuf::from(path))
    }

    #[test]
    fn setting_without_command_line() {
        let docs = plan("{home}/Docs", None);
        assert_eq!((docs.first, docs.start, docs.select), (p("/home/u/Docs"), p("/home/u/Docs"), None));
        assert!(docs.warnings.is_empty());

        let drives = plan("  Drives ", None);
        assert_eq!((drives.first, drives.start), (Location::Drives, Location::Drives));
        assert!(drives.warnings.is_empty());
    }

    #[test]
    fn invalid_setting_warns_and_uses_home() {
        for setting in ["{home}/../x", "/nowhere", "/work/notes.txt"] {
            let plan = plan(setting, None);
            assert_eq!((plan.first, plan.start), (p("/home/u"), p("/home/u")), "{setting}");
            assert_eq!(plan.warnings.len(), 1, "{setting}");
            assert!(plan.warnings[0].message.contains("is not a folder"), "{setting}");
        }
    }

    #[test]
    fn command_line_folder_wins_but_new_tabs_use_the_setting() {
        let plan = plan("drives", Some("/work/sub"));
        assert_eq!((plan.first, plan.start, plan.select), (p("/work/sub"), Location::Drives, None));
        assert!(plan.warnings.is_empty());
    }

    #[test]
    fn command_line_file_opens_its_folder_selected() {
        let plan = plan("{home}", Some("/work/notes.txt"));
        assert_eq!(plan.first, p("/work"));
        assert_eq!(plan.select.as_deref(), Some("notes.txt"));
        assert_eq!(plan.start, p("/home/u"));
    }

    #[test]
    fn missing_command_line_path_warns_and_opens_the_setting() {
        let plan = plan("drives", Some("/gone"));
        assert_eq!((plan.first, plan.start, plan.select), (Location::Drives, Location::Drives, None));
        assert_eq!(plan.warnings.len(), 1);
        assert_eq!(plan.warnings[0].file, "command line");
        assert!(plan.warnings[0].message.ends_with("not found; opening start-folder"));
    }

    #[test]
    fn invalid_setting_is_reported_even_with_a_command_line_path() {
        let plan = plan("/nowhere", Some("/work"));
        assert_eq!((plan.first, plan.start), (p("/work"), p("/home/u")));
        assert_eq!(plan.warnings.len(), 1);
        assert_eq!(plan.warnings[0].file, "settings.toml");
    }
}
