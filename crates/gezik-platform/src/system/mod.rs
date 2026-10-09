//! What Gezik writes to the system on an explicit command (spec 8.2, 11): the registry under
//! HKCU (Windows), links and folders (macOS, Linux), and the broadcast after PATH changes.
//! Plain writes only: the journal is gezik-config's `system_journal`, the undo rule
//! gezik-core's `system_change`, the places Gezik may write the app's `system_changes`.

use std::io;
use std::path::{Path, PathBuf};

#[cfg(unix)]
mod unix;
#[cfg(windows)]
pub mod windows;

#[cfg(unix)]
pub use unix::replace_symlink;

/// The user's folders the system changes are made in.
#[derive(Debug, Clone, Default)]
pub struct Places {
    pub home: Option<PathBuf>,
    /// Windows: `%LOCALAPPDATA%`.
    pub local_app_data: Option<PathBuf>,
}

pub fn places() -> Places {
    Places { home: dirs::home_dir(), local_app_data: if cfg!(windows) { dirs::data_local_dir() } else { None } }
}

/// This Gezik's exe; on macOS and Linux the real file (a link Gezik makes points to it).
pub fn exe() -> io::Result<PathBuf> {
    let exe = std::env::current_exe()?;
    if cfg!(unix) { exe.canonicalize() } else { Ok(exe) }
}

/// One folder, its parent already there; 0755 on macOS and Linux (spec 8.2).
pub fn make_folder(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new().mode(0o755).create(path)
    }
    #[cfg(not(unix))]
    std::fs::create_dir(path)
}

/// Whether `dir` is in this process's `PATH` (spec 8.2: if not, the line to add is shown).
pub fn in_path_env(dir: &Path) -> bool {
    let Some(path) = std::env::var_os("PATH") else { return false };
    let real = dir.canonicalize().ok();
    std::env::split_paths(&path).any(|d| d == dir || (real.is_some() && d.canonicalize().ok() == real))
}

/// Whether a link may be put at a place holding `found` (`None` nothing, `Some(None)` not a
/// link, `Some(Some(t))` a link to `t`): only an empty place, or Gezik's own link to `expect`.
#[cfg(any(unix, test))]
fn may_replace(found: Option<Option<&Path>>, expect: Option<&Path>) -> bool {
    match found {
        None => true,
        Some(target) => target.is_some() && target == expect,
    }
}

/// Tells running programs that the user's environment changed (Windows; a new terminal
/// then sees the new PATH). Hung windows are skipped after 2 s. Elsewhere nothing.
pub fn environment_changed() {
    #[cfg(windows)]
    windows::environment_changed();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_empty_place_or_our_own_link_is_replaced() {
        let ours = Path::new("/old/gezik");
        assert!(may_replace(None, None));
        assert!(may_replace(None, Some(ours)));
        assert!(may_replace(Some(Some(ours)), Some(ours)));
        assert!(!may_replace(Some(Some(ours)), None), "a link we did not expect");
        assert!(!may_replace(Some(Some(Path::new("/other/gezik"))), Some(ours)), "someone else's link");
        assert!(!may_replace(Some(None), Some(ours)), "a file or folder of the user's");
        assert!(!may_replace(Some(None), None));
    }

    #[test]
    fn a_folder_in_path_is_found() {
        let exe = exe().unwrap();
        assert!(exe.is_absolute());
        let dir = std::env::temp_dir();
        // Only what the process's PATH says; the temp folder is not normally in it.
        assert_eq!(
            in_path_env(&dir),
            std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).any(|d| d == dir)
        );
    }
}
