//! Where Gezik keeps its files, and machine-independent `{token}` paths.

use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};

/// The per-user config folder: `GEZIK_CONFIG_DIR` if set, otherwise e.g.
/// `%APPDATA%\gezik` on Windows or `~/.config/gezik` on Linux.
pub fn config_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("GEZIK_CONFIG_DIR").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    dirs::config_dir().map(|dir| dir.join("gezik"))
}

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
        PathBuf::from(text)
    }
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
/// readers and sync tools never see a half-written file.
pub fn write_atomic(path: &Path, contents: &str) -> io::Result<()> {
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?;
    let tmp = path.with_file_name(format!(".{}.tmp", name.to_string_lossy()));
    let mut file = std::fs::File::create(&tmp)?;
    file.write_all(contents.as_bytes())?;
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
    fn write_atomic_replaces_content_and_leaves_no_temp_file() {
        let dir = crate::test_dir("atomic");
        let path = dir.join("state.toml");
        write_atomic(&path, "first").unwrap();
        write_atomic(&path, "second").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "second");
        let names: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names, ["state.toml"]);
    }
}
