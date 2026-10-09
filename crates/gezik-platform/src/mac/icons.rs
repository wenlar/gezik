//! macOS system icons: NSWorkspace's, as Finder shows them, at the size asked for.

use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use objc2::rc::autoreleasepool;
use objc2_app_kit::NSWorkspace;
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

use crate::{IconTarget, Rgba};

pub fn icon(target: &IconTarget, px: u32) -> Option<Rgba> {
    // Worker threads drain no pool of their own.
    autoreleasepool(|_| {
        let workspace = NSWorkspace::sharedWorkspace();
        // `iconForFileType:` takes an extension or a type identifier and needs no file (one
        // icon per type); `iconForContentType:` would need the UniformTypeIdentifiers crate.
        #[allow(deprecated)]
        let image = match target {
            IconTarget::Extension(ext) if ext.is_empty() => {
                workspace.iconForFileType(&NSString::from_str("public.data"))
            }
            IconTarget::Extension(ext) => workspace.iconForFileType(&NSString::from_str(ext)),
            IconTarget::Folder => workspace.iconForFileType(&NSString::from_str("public.folder")),
            IconTarget::Path(path) => workspace.iconForFile(&NSString::from_str(path.to_str()?)),
        };
        let side = f64::from(px.max(1));
        let mut rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(side, side));
        // SAFETY: `rect` is a live NSRect; no context and no hints are passed.
        let picture = unsafe { image.CGImageForProposedRect_context_hints(&mut rect, None, None) }?;
        super::image::cg_to_rgba(&picture, px)
    })
}

/// A custom folder icon (Finder's `Icon\r` file or its flag), or a volume's root (another
/// device than the parent's: `/Volumes/X`, and `/Applications`, `/Users` on the Data volume).
pub fn folder_has_own_icon(path: &Path) -> bool {
    if path.join("Icon\r").exists() {
        return true;
    }
    if let (Ok(here), Some(Ok(up))) = (std::fs::metadata(path), path.parent().map(std::fs::metadata))
        && here.dev() != up.dev()
    {
        return true;
    }
    finder_info(path).is_some_and(|info| crate::icons::finder_info_custom_icon(&info))
}

/// The item's `com.apple.FinderInfo` (32 bytes), if it has one.
fn finder_info(path: &Path) -> Option<[u8; 32]> {
    let name = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut info = [0u8; 32];
    // SAFETY: both names are NUL-terminated and `info` has room for `info.len()` bytes.
    let read = unsafe {
        libc::getxattr(
            name.as_ptr(),
            c"com.apple.FinderInfo".as_ptr(),
            info.as_mut_ptr().cast(),
            info.len(),
            0,
            libc::XATTR_NOFOLLOW,
        )
    };
    (read == 32).then_some(info)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apps_folders_and_types_have_icons() {
        for target in [
            IconTarget::Path("/Applications/Safari.app".into()),
            IconTarget::Folder,
            IconTarget::Extension("pdf".into()),
            IconTarget::Extension(String::new()),
        ] {
            let icon = icon(&target, 64).unwrap_or_else(|| panic!("no icon for {target:?}"));
            assert!(icon.width <= 64 && icon.height <= 64 && icon.width >= 32, "{target:?}: {icon:?}");
            assert_eq!(icon.pixels.len(), (icon.width * icon.height * 4) as usize);
            assert!(icon.pixels.as_chunks::<4>().0.iter().any(|p| p[3] > 0), "{target:?} is all transparent");
        }
        assert!(folder_has_own_icon(Path::new("/Applications")), "the Data volume's folder");
        assert!(!folder_has_own_icon(&std::env::temp_dir()));
    }
}
