//! macOS: Finder's names for folders, aliases and packages (spec 9 §4.2).

use std::path::{Path, PathBuf};

use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::AnyObject;
use objc2_foundation::{
    NSFileManager, NSNumber, NSString, NSURL, NSURLBookmarkResolutionOptions, NSURLIsAliasFileKey, NSURLIsPackageKey,
    NSURLResourceKey,
};

use crate::finder::AliasTarget;

/// What Finder calls `path` ("Belgeler" for Documents in Turkish).
pub fn display_name(path: &Path) -> Option<String> {
    let text = path.to_str()?;
    let name =
        autoreleasepool(|_| NSFileManager::defaultManager().displayNameAtPath(&NSString::from_str(text)).to_string());
    (!name.is_empty()).then_some(name)
}

fn url_of(path: &Path) -> Option<Retained<NSURL>> {
    Some(NSURL::fileURLWithPath(&NSString::from_str(path.to_str()?)))
}

/// A yes/no resource value of `url`; `false` when it cannot be read.
fn flag(url: &NSURL, key: &NSURLResourceKey) -> bool {
    let mut value: Option<Retained<AnyObject>> = None;
    // SAFETY: the keys asked for here have NSNumber values.
    let read = unsafe { url.getResourceValue_forKey_error(&mut value, key) };
    read.is_ok() && value.and_then(|v| v.downcast::<NSNumber>().ok()).is_some_and(|n| n.boolValue())
}

pub fn is_package(path: &Path) -> bool {
    // SAFETY: an extern static of Foundation.
    autoreleasepool(|_| url_of(path).is_some_and(|url| flag(&url, unsafe { NSURLIsPackageKey })))
}

pub fn resolve_alias(path: &Path) -> AliasTarget {
    autoreleasepool(|_| {
        let Some(url) = url_of(path) else { return AliasTarget::NotAlias };
        // Finder aliases and symbolic links both say yes.
        // SAFETY: an extern static of Foundation.
        if !flag(&url, unsafe { NSURLIsAliasFileKey }) {
            return AliasTarget::NotAlias;
        }
        let options = NSURLBookmarkResolutionOptions::WithoutUI | NSURLBookmarkResolutionOptions::WithoutMounting;
        let Ok(target) = NSURL::URLByResolvingAliasFileAtURL_options_error(&url, options) else {
            return AliasTarget::Missing;
        };
        let Some(target) = target.path().map(|p| PathBuf::from(p.to_string())) else { return AliasTarget::Missing };
        match std::fs::metadata(&target) {
            Ok(meta) => AliasTarget::Target { path: target, is_dir: meta.is_dir() },
            Err(_) => AliasTarget::Missing,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_have_finder_names_and_links_resolve() {
        assert!(display_name(Path::new("/Applications")).is_some_and(|n| !n.is_empty()));
        assert!(is_package(Path::new("/Applications/Safari.app")));
        assert!(!is_package(Path::new("/Applications")));
        let dir = std::env::temp_dir().join(format!("gezik-finder-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), "a").unwrap();
        std::os::unix::fs::symlink(dir.join("a.txt"), dir.join("link")).unwrap();
        std::os::unix::fs::symlink(dir.join("gone"), dir.join("broken")).unwrap();
        assert_eq!(resolve_alias(&dir.join("a.txt")), AliasTarget::NotAlias);
        assert!(matches!(resolve_alias(&dir.join("link")), AliasTarget::Target { is_dir: false, .. }));
        assert_eq!(resolve_alias(&dir.join("broken")), AliasTarget::Missing);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
