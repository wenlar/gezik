//! macOS: `~/.Trash` and `/Volumes/*/.Trashes/<uid>`. Linux (freedesktop.org): the home trash,
//! and on every mount `$topdir/.Trash/<uid>` (a shared, sticky `.Trash`) and `$topdir/.Trash-<uid>`.

use std::path::{Path, PathBuf};

use super::Bin;

fn me() -> u32 {
    // SAFETY: getuid cannot fail.
    unsafe { libc::getuid() }
}

#[cfg(target_os = "macos")]
pub(super) fn bins() -> Vec<Bin> {
    let mut bins: Vec<Bin> = dirs::home_dir()
        .map(|home| Bin::Mac { dir: home.join(".Trash"), volume: PathBuf::from("/") })
        .into_iter()
        .collect();
    for drive in crate::drives().into_iter().filter(|d| d.path != Path::new("/")) {
        bins.push(Bin::Mac { dir: drive.path.join(".Trashes").join(me().to_string()), volume: drive.path });
    }
    bins
}

#[cfg(not(target_os = "macos"))]
pub(super) fn bins() -> Vec<Bin> {
    use super::{own_trash_ok, shared_trash_ok};
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let checked = |path: &Path, ok: &dyn Fn(&std::fs::Metadata) -> bool| {
        std::fs::symlink_metadata(path).is_ok_and(|meta| ok(&meta))
    };
    let own = |meta: &std::fs::Metadata| own_trash_ok(meta.is_dir(), meta.file_type().is_symlink(), meta.uid(), me());
    let shared = |meta: &std::fs::Metadata| {
        shared_trash_ok(meta.is_dir(), meta.file_type().is_symlink(), meta.permissions().mode())
    };
    let mut bins: Vec<Bin> =
        crate::fs::home_trash().map(|dir| Bin::Freedesktop { dir, topdir: None }).into_iter().collect();
    for drive in crate::drives() {
        let top: PathBuf = drive.path;
        let mine = top.join(".Trash").join(me().to_string());
        if checked(&top.join(".Trash"), &shared) && checked(&mine, &own) {
            bins.push(Bin::Freedesktop { dir: mine, topdir: Some(top.clone()) });
        }
        let private = top.join(format!(".Trash-{}", me()));
        if checked(&private, &own) {
            bins.push(Bin::Freedesktop { dir: private, topdir: Some(top) });
        }
    }
    bins
}

pub(super) fn changed() {}
