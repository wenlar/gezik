//! Finder's ways (spec 9 §4.2): the names Finder shows for some folders, aliases and
//! packages. Off macOS there are none of these: the functions say so and touch nothing.

use std::path::{Path, PathBuf};

/// The `st_flags` bit of a file whose data is only in the cloud (iCloud, File Provider):
/// reading it downloads it.
pub const SF_DATALESS: u32 = 0x4000_0000;

/// What opening a file that may be an alias leads to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AliasTarget {
    /// Not an alias (nor a symbolic link): opened as it is.
    NotAlias,
    /// It leads here.
    Target { path: PathBuf, is_dir: bool },
    /// An alias whose original is gone.
    Missing,
}

/// Whether Finder may show another name for the folder `path`: a folder right under the home
/// folder or `/` (Documents: "Belgeler"), or one whose name ends in `.localized`.
pub fn shows_finder_name(path: &Path, home: Option<&Path>) -> bool {
    let Some(parent) = path.parent() else { return false };
    parent == Path::new("/") || Some(parent) == home || path.extension().is_some_and(|e| e == "localized")
}

/// The name Finder shows for the folder `path` where it is not the folder's own
/// ([`shows_finder_name`]); `None` elsewhere and off macOS. Folders only: for a file Finder's
/// name may hide the extension. Cached on the calling thread.
pub fn finder_name(path: &Path) -> Option<String> {
    #[cfg(target_os = "macos")]
    return cached_finder_name(path);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        None
    }
}

#[cfg(target_os = "macos")]
fn cached_finder_name(path: &Path) -> Option<String> {
    use std::cell::RefCell;
    use std::collections::HashMap;
    /// Few folders ask (those under home and `/`): forgotten wholesale beyond this many.
    const MAX_NAMES: usize = 256;
    thread_local! {
        static NAMES: RefCell<HashMap<PathBuf, Option<String>>> = RefCell::default();
    }
    static HOME: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    if !shows_finder_name(path, HOME.get_or_init(dirs::home_dir).as_deref()) {
        return None;
    }
    if let Some(known) = NAMES.with(|names| names.borrow().get(path).cloned()) {
        return known;
    }
    let own = path.file_name().map(|n| n.to_string_lossy().into_owned());
    let name = crate::mac::finder::display_name(path).filter(|name| Some(name) != own.as_ref());
    NAMES.with(|names| {
        let mut names = names.borrow_mut();
        if names.len() >= MAX_NAMES {
            names.clear();
        }
        names.insert(path.to_path_buf(), name.clone());
    });
    name
}

/// What the alias (or symbolic link) at `path` leads to. Reads the disk (never mounts a volume
/// or shows a window); off macOS always [`AliasTarget::NotAlias`].
pub fn resolve_alias(path: &Path) -> AliasTarget {
    #[cfg(target_os = "macos")]
    return crate::mac::finder::resolve_alias(path);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        AliasTarget::NotAlias
    }
}

/// Whether macOS says the folder `path` is a package (`NSURLIsPackageKey`); off macOS never.
pub fn is_package(path: &Path) -> bool {
    #[cfg(target_os = "macos")]
    return crate::mac::finder::is_package(path);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        false
    }
}

/// Makes a Finder alias at `at` to `target` (a bookmark file, as Finder's Make Alias). Never
/// replaces a file already at `at`. macOS only.
pub fn make_alias(target: &Path, at: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    return crate::mac::finder::make_alias(target, at);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (target, at);
        Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "Aliases are made on macOS only"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finder_names_are_asked_for_only_under_home_and_root() {
        let home = Path::new("/Users/u");
        assert!(shows_finder_name(Path::new("/Users/u/Documents"), Some(home)));
        assert!(shows_finder_name(Path::new("/Applications"), Some(home)));
        assert!(shows_finder_name(Path::new("/x/y/Reports.localized"), Some(home)));
        assert!(!shows_finder_name(Path::new("/Users/u/Documents/Work"), Some(home)));
        assert!(!shows_finder_name(Path::new("/"), Some(home)), "the root has no parent");
        assert!(!shows_finder_name(Path::new("/x/y/Reports"), None));
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn nothing_changes_off_macos() {
        let path = Path::new("/Users/u/Documents");
        assert_eq!(finder_name(path), None);
        assert_eq!(resolve_alias(path), AliasTarget::NotAlias);
        assert!(!is_package(Path::new("/Applications/Safari.app")));
        let dir = std::env::temp_dir();
        let at = dir.join(format!("gezik-no-alias-{}", std::process::id()));
        assert_eq!(make_alias(&dir, &at).unwrap_err().kind(), std::io::ErrorKind::Unsupported);
        assert!(!at.exists());
    }
}
