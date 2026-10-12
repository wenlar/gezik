//! Where Gezik keeps its files, and machine-independent `{token}` paths.

use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};

/// The folder for this machine's files ([`Dirs::local`]): `GEZIK_CONFIG_DIR` if set, else
/// `gezik-data` next to a `gezik-portable` marker, else e.g. `%APPDATA%\gezik` on Windows or
/// `~/.config/gezik` on Linux (spec 14 §3.1).
pub fn config_dir() -> Option<PathBuf> {
    location().dirs.as_ref().map(|dirs| dirs.local.clone())
}

/// The file next to Gezik (next to `Gezik.app` on macOS) that turns on portable mode.
pub const PORTABLE_MARKER: &str = "gezik-portable";
/// The folder next to the marker that portable Gezik keeps everything in.
pub const PORTABLE_DATA: &str = "gezik-data";

/// The status bar note when the portable data folder cannot be written.
pub const READ_ONLY_NOTE: &str = "Portable data folder is read-only: changes are not saved";
/// The status bar note when macOS runs Gezik from a random read-only copy (App Translocation).
pub const TRANSLOCATED_NOTE: &str =
    "Move Gezik.app out of Downloads (or open it once from Finder after moving) to use portable mode";

/// Where Gezik's files are (spec 14 §3.1): `shared` holds settings.toml, the themes and the
/// templates; `local` this machine's files (state, views, tags, tools, backups, the system
/// changes). The same folder until a sync folder (14f) is set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dirs {
    pub shared: PathBuf,
    pub local: PathBuf,
    /// In `gezik-data` next to a `gezik-portable` marker.
    pub portable: bool,
    /// `shared` is a sync folder `location.toml` names (spec 14 §8): it may be missing, and
    /// `settings.local.toml` is read over its settings.toml.
    pub synced: bool,
}

impl Dirs {
    /// One folder for everything.
    pub fn one(dir: PathBuf) -> Dirs {
        Dirs { shared: dir.clone(), local: dir, portable: false, synced: false }
    }
}

/// Where the files go, before any folder is looked at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    /// `GEZIK_CONFIG_DIR`, or the system's config folder (none: no folder at all).
    Plain(Option<PathBuf>),
    /// A marker in this folder: everything in its `gezik-data`; the system's folder if that
    /// cannot be made.
    Portable { folder: PathBuf, system: Option<PathBuf> },
    /// Started by macOS App Translocation, where no marker can be seen: the system's folder.
    Translocated(Option<PathBuf>),
}

/// The folder a marker is looked for in: the AppImage's folder, the one holding a macOS
/// `X.app` (inside the bundle would break its signature), else the exe's folder.
pub fn marker_folder<'a>(exe: &'a Path, appimage: Option<&'a Path>) -> Option<&'a Path> {
    if let Some(image) = appimage {
        return image.parent();
    }
    let folder = exe.parent()?;
    let bundle = folder
        .parent()
        .filter(|_| folder.file_name().is_some_and(|n| n == "MacOS"))
        .filter(|contents| contents.file_name().is_some_and(|n| n == "Contents"))
        .and_then(Path::parent)
        .filter(|app| app.extension().is_some_and(|e| e == "app"));
    match bundle {
        Some(app) => app.parent(),
        None => Some(folder),
    }
}

/// The order of spec 14 §3.1 (pure): `GEZIK_CONFIG_DIR` (`env`), then a marker next to `exe`
/// (`has_marker` asks the disk), then the system's folder.
pub fn choose(
    env: Option<PathBuf>,
    exe: Option<&Path>,
    appimage: Option<&Path>,
    has_marker: impl Fn(&Path) -> bool,
    system: Option<PathBuf>,
) -> Choice {
    if env.is_some() {
        return Choice::Plain(env);
    }
    let Some(exe) = exe else { return Choice::Plain(system) };
    if let Some(folder) = marker_folder(exe, appimage).filter(|f| has_marker(&f.join(PORTABLE_MARKER))) {
        return Choice::Portable { folder: folder.to_path_buf(), system };
    }
    if exe.to_string_lossy().contains("/AppTranslocation/") {
        return Choice::Translocated(system);
    }
    Choice::Plain(system)
}

/// Where the files are and what to say about it, worked out once at start.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Location {
    pub dirs: Option<Dirs>,
    /// The portable data folder cannot be written: nothing is written to it.
    pub read_only: bool,
    /// Said in the status bar at start.
    pub note: Option<String>,
}

/// Looks at the disk for `choice`: a portable `gezik-data` that is there but cannot be
/// written is used read-only; one that cannot be made falls back to the system's folder.
/// The data folder itself is made by the store's first run (which writes the starter files).
pub fn locate(choice: Choice, writable: impl Fn(&Path) -> bool) -> Location {
    let plain =
        |dir: Option<PathBuf>, note: Option<String>| Location { dirs: dir.map(Dirs::one), read_only: false, note };
    match choice {
        Choice::Plain(dir) => plain(dir, None),
        Choice::Translocated(dir) => plain(dir, Some(TRANSLOCATED_NOTE.to_owned())),
        Choice::Portable { folder, system } => {
            let data = folder.join(PORTABLE_DATA);
            let portable = || Some(Dirs { portable: true, ..Dirs::one(data.clone()) });
            if data.is_dir() {
                let read_only = !writable(&data);
                Location { dirs: portable(), read_only, note: read_only.then(|| READ_ONLY_NOTE.to_owned()) }
            } else if writable(&folder) {
                Location { dirs: portable(), read_only: false, note: None }
            } else {
                let place = system.as_ref().map_or("the default settings".to_owned(), |s| s.display().to_string());
                let note = format!("Portable mode: {} cannot be made (read-only?); using {place}", data.display());
                plain(system, Some(note))
            }
        }
    }
}

/// Whether a file can be made in `dir` (a probe file, removed at once).
fn writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".gezik-probe-{}", std::process::id()));
    let made = std::fs::File::create(&probe).is_ok();
    let _ = std::fs::remove_file(&probe);
    made
}

/// `\\?\C:\x` → `C:\x`, `\\?\UNC\server\share` → `\\server\share` (what `canonicalize` gives on
/// Windows); anything else as it is.
pub fn plain_path(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    match text.strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
        _ => path.to_path_buf(),
    }
}

/// This process's [`Location`]: one `metadata` call for the marker when there is none, and one
/// `open` for `location.toml` (spec 14 §3.5).
pub fn location() -> &'static Location {
    static LOCATION: std::sync::OnceLock<Location> = std::sync::OnceLock::new();
    LOCATION.get_or_init(|| {
        let env = std::env::var_os("GEZIK_CONFIG_DIR").filter(|d| !d.is_empty()).map(PathBuf::from);
        // The real file (a link, junction or subst drive reaches the same folder, so the same
        // single-instance key); on Windows without the `\\?\` the user would see.
        let exe = std::env::current_exe().ok().map(|e| e.canonicalize().map(|real| plain_path(&real)).unwrap_or(e));
        let appimage = std::env::var_os("APPIMAGE").filter(|_| cfg!(target_os = "linux")).map(PathBuf::from);
        let system = dirs::config_dir().map(|dir| dir.join("gezik"));
        let choice = choose(env, exe.as_deref(), appimage.as_deref(), |m| m.is_file(), system);
        let mut location = locate(choice, writable);
        if let Some(dirs) = location.dirs.as_mut()
            && let Ok(text) = std::fs::read_to_string(dirs.local.join(crate::sync_folder::LOCATION_FILE))
            && let Some(folder) = crate::sync_folder::parse_location(&text, &KnownDirs::system())
        {
            (dirs.shared, dirs.synced) = (folder, true);
        }
        location
    })
}

/// The `{token}`s [`KnownDirs::system`] knows, for settings that check a path's token.
pub const TOKENS: [&str; 7] = ["home", "desktop", "documents", "downloads", "pictures", "music", "videos"];

/// Well-known folders, used to store paths so they work on another machine or OS.
#[derive(Debug, Clone)]
pub struct KnownDirs {
    /// (token, folder), deepest folder first so nested folders win.
    dirs: Vec<(&'static str, PathBuf)>,
}

impl KnownDirs {
    pub fn new(mut dirs: Vec<(&'static str, PathBuf)>) -> Self {
        dirs.sort_by_key(|(_, path)| std::cmp::Reverse(path.components().count()));
        Self { dirs }
    }

    /// The current user's folders on this machine.
    pub fn system() -> Self {
        let candidates = [
            ("home", dirs::home_dir()),
            ("desktop", dirs::desktop_dir()),
            ("documents", dirs::document_dir()),
            ("downloads", dirs::download_dir()),
            ("pictures", dirs::picture_dir()),
            ("music", dirs::audio_dir()),
            ("videos", dirs::video_dir()),
        ];
        Self::new(candidates.into_iter().filter_map(|(token, dir)| Some((token, dir?))).collect())
    }

    /// `C:\Users\a\Documents\Work` → `{documents}/Work`. Paths outside every known folder
    /// stay absolute, written with `/` separators.
    pub fn collapse(&self, path: &Path) -> String {
        for (token, dir) in &self.dirs {
            if let Ok(rest) = path.strip_prefix(dir) {
                let rest = slash_path(rest);
                return if rest.is_empty() { format!("{{{token}}}") } else { format!("{{{token}}}/{rest}") };
            }
        }
        slash_path(path)
    }

    /// Inverse of [`collapse`](Self::collapse). Text that does not start with a known
    /// token is taken as a literal path.
    pub fn expand(&self, text: &str) -> PathBuf {
        if let Some((token, rest)) = text.strip_prefix('{').and_then(|inner| inner.split_once('}'))
            && let Some((_, dir)) = self.dirs.iter().find(|(t, _)| *t == token)
        {
            let mut path = dir.clone();
            path.extend(rest.split('/').filter(|part| !part.is_empty()));
            return path;
        }
        PathBuf::from(native_separators(text))
    }

    /// [`expand`](Self::expand), but rejects text with a `..` segment (returns `None`).
    pub fn expand_checked(&self, text: &str) -> Option<PathBuf> {
        (!has_parent_segment(text)).then(|| self.expand(text))
    }
}

/// Stored paths use `/`, but the Windows shell APIs need `\`: a literal `D:/a/b` becomes
/// `D:\a\b` and `//srv/share/x` becomes `\\srv\share\x`. Other OSes keep the text.
pub fn native_separators(text: &str) -> String {
    if cfg!(windows) { text.replace('/', "\\") } else { text.to_owned() }
}

/// Whether a stored path tries to climb out of its folder with `..`.
pub fn has_parent_segment(text: &str) -> bool {
    text.split(['/', '\\']).any(|segment| segment == "..")
}

/// Joins path components with `/` on every OS: `C:\a\b` → `C:/a/b`.
fn slash_path(path: &Path) -> String {
    let mut out = String::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => out.push_str(&prefix.as_os_str().to_string_lossy()),
            Component::RootDir => out.push('/'),
            other => {
                if !out.is_empty() && !out.ends_with('/') {
                    out.push('/');
                }
                out.push_str(&other.as_os_str().to_string_lossy());
            }
        }
    }
    out
}

/// Writes `contents` to a temporary file next to `path`, then renames it into place, so
/// readers and sync tools never see a half-written file. A symlinked file (say, from a
/// dotfiles repo) stays a link: its target is written.
pub fn write_atomic(path: &Path, contents: &str) -> io::Result<()> {
    write_atomic_bytes(path, contents.as_bytes())
}

/// [`write_atomic`] for bytes (a UTF-16 .reg file).
pub fn write_atomic_bytes(path: &Path, contents: &[u8]) -> io::Result<()> {
    let target;
    let path = if path.symlink_metadata().is_ok_and(|meta| meta.file_type().is_symlink()) {
        target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        &target
    } else {
        path
    };
    let name = path.file_name().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?;
    // The process id keeps two Gezik windows from writing into the same temp file.
    let tmp = path.with_file_name(format!(".{}.{}.tmp", name.to_string_lossy(), std::process::id()));
    let mut file = std::fs::File::create(&tmp)?;
    file.write_all(contents)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake() -> KnownDirs {
        KnownDirs::new(vec![
            ("home", PathBuf::from("/u/alice")),
            ("documents", PathBuf::from("/u/alice/OneDrive/Documents")),
            ("downloads", PathBuf::from("/u/alice/Downloads")),
        ])
    }

    #[test]
    fn literal_paths_get_native_separators() {
        if cfg!(windows) {
            assert_eq!(native_separators("D:/Work/x"), r"D:\Work\x");
            assert_eq!(native_separators("//server/share/x"), r"\\server\share\x");
            assert_eq!(fake().expand("D:/Work/x").to_string_lossy(), r"D:\Work\x");
        } else {
            assert_eq!(native_separators("/srv/x"), "/srv/x");
        }
    }

    #[test]
    fn collapses_to_the_deepest_matching_folder() {
        let dirs = fake();
        assert_eq!(dirs.collapse(Path::new("/u/alice/OneDrive/Documents/Work/x")), "{documents}/Work/x");
        assert_eq!(dirs.collapse(Path::new("/u/alice/Music")), "{home}/Music");
        assert_eq!(dirs.collapse(Path::new("/u/alice/Downloads")), "{downloads}");
    }

    #[test]
    fn keeps_unknown_paths_absolute_with_forward_slashes() {
        assert_eq!(fake().collapse(Path::new("/srv/data")), "/srv/data");
    }

    #[test]
    fn expands_tokens() {
        let dirs = fake();
        assert_eq!(dirs.expand("{documents}/Work/x"), PathBuf::from("/u/alice/OneDrive/Documents/Work/x"));
        assert_eq!(dirs.expand("{downloads}"), PathBuf::from("/u/alice/Downloads"));
    }

    #[test]
    fn unknown_tokens_stay_literal() {
        assert_eq!(fake().expand("{nope}/x"), PathBuf::from("{nope}/x"));
    }

    #[test]
    fn parent_segments_are_detected_and_rejected() {
        assert!(has_parent_segment("{home}/../x"));
        assert!(has_parent_segment(r"C:\a\..\b"));
        assert!(has_parent_segment(".."));
        assert!(!has_parent_segment("{home}/a..b/c"));
        assert!(!has_parent_segment("/srv/data"));
        assert_eq!(fake().expand_checked("{home}/../x"), None);
        assert_eq!(fake().expand_checked("{downloads}"), Some(PathBuf::from("/u/alice/Downloads")));
    }

    #[test]
    fn round_trips() {
        let dirs = fake();
        for path in ["/u/alice/OneDrive/Documents/a/b", "/u/alice/Music", "/u/alice/Downloads", "/srv/data"] {
            assert_eq!(dirs.expand(&dirs.collapse(Path::new(path))), PathBuf::from(path), "{path}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_paths_use_forward_slashes() {
        let dirs = KnownDirs::new(vec![("documents", PathBuf::from(r"C:\Users\a\Documents"))]);
        assert_eq!(dirs.collapse(Path::new(r"C:\Users\a\Documents\Work")), "{documents}/Work");
        assert_eq!(dirs.collapse(Path::new(r"D:\Work\gezik")), "D:/Work/gezik");
        assert_eq!(dirs.expand("D:/Work/gezik"), PathBuf::from(r"D:\Work\gezik"));
    }

    #[test]
    fn system_dirs_know_home() {
        assert_ne!(KnownDirs::system().expand("{home}"), PathBuf::from("{home}"));
    }

    #[test]
    fn config_dir_ends_with_gezik() {
        // GEZIK_CONFIG_DIR is not set when tests run.
        assert!(config_dir().is_some_and(|dir| dir.ends_with("gezik")));
    }

    #[test]
    fn the_marker_is_next_to_the_exe_the_bundle_or_the_appimage() {
        let at =
            |exe: &str, image: Option<&str>| marker_folder(Path::new(exe), image.map(Path::new)).map(Path::to_path_buf);
        assert_eq!(at("/e/Gezik/gezik.exe", None), Some(PathBuf::from("/e/Gezik")));
        assert_eq!(at("/v/X/Gezik.app/Contents/MacOS/gezik", None), Some(PathBuf::from("/v/X")), "beside the bundle");
        assert_eq!(at("/v/X/Contents/MacOS/gezik", None), Some(PathBuf::from("/v/X/Contents/MacOS")), "not a bundle");
        assert_eq!(at("/v/X/Gezik.app/Other/MacOS/gezik", None), Some(PathBuf::from("/v/X/Gezik.app/Other/MacOS")));
        assert_eq!(at("/tmp/.mount_ab/usr/bin/gezik", Some("/usb/Gezik.AppImage")), Some(PathBuf::from("/usb")));
    }

    #[test]
    fn the_config_dir_wins_then_the_marker_then_the_system() {
        let system = Some(PathBuf::from("/home/a/.config/gezik"));
        let exe = Path::new("/usb/Gezik/gezik");
        let marker = |m: &Path| m == Path::new("/usb/Gezik/gezik-portable");
        let none = |_: &Path| false;
        let env = Some(PathBuf::from("/dev/conf"));
        assert_eq!(choose(env.clone(), Some(exe), None, marker, system.clone()), Choice::Plain(env));
        let portable = Choice::Portable { folder: PathBuf::from("/usb/Gezik"), system: system.clone() };
        assert_eq!(choose(None, Some(exe), None, marker, system.clone()), portable);
        assert_eq!(choose(None, Some(exe), None, none, system.clone()), Choice::Plain(system.clone()));
        assert_eq!(choose(None, None, None, marker, system.clone()), Choice::Plain(system.clone()));
        let image = Path::new("/usb/Gezik/Gezik.AppImage");
        let mounted = Path::new("/tmp/.mount_x/usr/bin/gezik");
        assert!(matches!(choose(None, Some(mounted), Some(image), marker, None), Choice::Portable { .. }));
        let translocated =
            Path::new("/private/var/folders/x/T/AppTranslocation/AB-12/d/Gezik.app/Contents/MacOS/gezik");
        assert_eq!(choose(None, Some(translocated), None, none, system.clone()), Choice::Translocated(system));
    }

    #[test]
    fn verbatim_prefixes_are_dropped() {
        assert_eq!(plain_path(Path::new(r"\\?\E:\Gezik\gezik.exe")), Path::new(r"E:\Gezik\gezik.exe"));
        assert_eq!(plain_path(Path::new(r"\\?\UNC\nas\usb\gezik.exe")), Path::new(r"\\nas\usb\gezik.exe"));
        assert_eq!(plain_path(Path::new(r"\\?\Volume{1}\x")), Path::new(r"\\?\Volume{1}\x"));
        assert_eq!(plain_path(Path::new("/usb/gezik")), Path::new("/usb/gezik"));
    }

    #[test]
    fn a_portable_data_folder_is_used_read_only_or_left_for_the_system_one() {
        let dir = crate::test_dir("portable");
        let system = dir.join("system");
        let choice = || Choice::Portable { folder: dir.clone(), system: Some(system.clone()) };
        let data = dir.join(PORTABLE_DATA);
        let portable = Some(Dirs { portable: true, ..Dirs::one(data.clone()) });
        // Not there yet: made by the first run, not here.
        let first = locate(choice(), writable);
        assert_eq!(first, Location { dirs: portable.clone(), read_only: false, note: None });
        assert!(!data.exists());
        let names: Vec<_> = std::fs::read_dir(&dir).unwrap().collect();
        assert!(names.is_empty(), "the probe file is gone");
        // A read-only folder (a CD) without gezik-data: the system's folder, said.
        let fallback = locate(choice(), |_| false);
        assert_eq!(fallback.dirs, Some(Dirs::one(system.clone())));
        assert!(fallback.note.unwrap().contains("cannot be made"));
        // gezik-data there but read-only: used, nothing written.
        std::fs::create_dir(&data).unwrap();
        let read_only = locate(choice(), |_| false);
        assert_eq!(read_only, Location { dirs: portable, read_only: true, note: Some(READ_ONLY_NOTE.into()) });
        // No marker: one folder as before, nothing said.
        let plain = locate(Choice::Plain(Some(system.clone())), |_| false);
        assert_eq!(plain, Location { dirs: Some(Dirs::one(system.clone())), read_only: false, note: None });
        let moved = locate(Choice::Translocated(Some(system)), writable);
        assert_eq!(moved.note.as_deref(), Some(TRANSLOCATED_NOTE));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_atomic_replaces_content_and_leaves_no_temp_file() {
        let dir = crate::test_dir("atomic");
        let path = dir.join("state.toml");
        write_atomic(&path, "first").unwrap();
        write_atomic(&path, "second").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "second");
        let names: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names, ["state.toml"]);
    }

    #[test]
    fn write_atomic_keeps_a_symlinked_file_a_link() {
        let dir = crate::test_dir("atomic-link");
        let (target, link) = (dir.join("real.toml"), dir.join("settings.toml"));
        std::fs::write(&target, "old").unwrap();
        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_file(&target, &link);
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(&target, &link);
        if made.is_err() {
            eprintln!("symbolic links need Developer Mode here: skipped");
            return;
        }
        write_atomic(&link, "new").unwrap();
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
    }

    #[test]
    fn the_system_folders_use_the_known_tokens() {
        assert!(KnownDirs::system().dirs.iter().all(|(token, _)| TOKENS.contains(token)));
    }
}
