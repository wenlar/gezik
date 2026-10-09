//! The Recycle Bin: `$Recycle.Bin\<SID>` on every local fixed and removable drive.

use super::Bin;
use crate::DriveKind;

pub(super) fn bins() -> Vec<Bin> {
    let Some(sid) = crate::instance::user_sid() else { return Vec::new() };
    crate::drives()
        .into_iter()
        .filter(|d| matches!(d.kind, DriveKind::Fixed | DriveKind::Removable))
        .map(|d| Bin::Windows(d.path.join("$Recycle.Bin").join(&sid)))
        .collect()
}

// Not in the `windows` crate nor in shell32.lib; shell32.dll exports it (shlobj_core.h).
#[link(name = "shell32", kind = "raw-dylib")]
unsafe extern "system" {
    fn SHUpdateRecycleBinIcon();
}

pub(super) fn changed() {
    // Redraws the Recycle Bin's icon (empty or full) on the desktop and in Explorer.
    // SAFETY: no arguments, no result.
    unsafe { SHUpdateRecycleBinIcon() };
}
