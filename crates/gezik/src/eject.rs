//! Eject and Disconnect (spec 9 §7.4): Gezik lets go of the drive first (its tabs go to This
//! PC, the watches end), then asks the system on a background thread.

use std::path::Path;
use std::time::Duration;

use gezik_core::nav::Location;
use gezik_core::ops::paths::{is_within, same_path};
use gezik_platform::Drive;
use gezik_platform::eject::{self, EjectWay};

use crate::navigation::Navigator;
use crate::view::View;

pub const NO_DRIVE: &str = "Choose a drive to eject";
/// Letting go may take this long before the system tries the drive (`removal.rs`'s grace:
/// the watcher's thread closes its handles within moments).
const GRACE: Duration = Duration::from_millis(150);

/// The drive `path` is on: the one whose root holds it, the deepest if they nest (`/` and
/// `/Volumes/USB`).
pub(crate) fn drive_for<'a>(drives: &'a [Drive], path: &Path) -> Option<&'a Drive> {
    drives.iter().filter(|d| is_within(path, &d.path)).max_by_key(|d| d.path.components().count())
}

/// What Eject offers for the drive whose root is `root` (a drive row); none for a folder.
pub(crate) fn way_at(root: &Path) -> Option<EjectWay> {
    drives().iter().find(|d| same_path(&d.path, root)).and_then(eject::offer)
}

/// The drives as last read (This PC's and the sidebar's).
fn drives() -> Vec<Drive> {
    let mut drives = Vec::new();
    crate::navigation::with_current(|nav| drives = nav.places().drives);
    drives
}

/// The action: the selected (or focused) drive on This PC, else the drive of the folder shown.
pub fn eject_selection(view: &View, nav: &Navigator) {
    let path = if view.shows_drives() {
        view.single_selected().or_else(|| view.focus()).and_then(|i| view.entry_path(i)).map(|(path, _)| path)
    } else {
        nav.active_location().folder().map(Path::to_path_buf)
    };
    match path {
        Some(path) => eject_path(&path),
        None => view.note(NO_DRIVE.to_owned()),
    }
}

/// Ejects (or disconnects) the drive `path` is on.
pub fn eject_path(path: &Path) {
    let drives = drives();
    let Some(drive) = drive_for(&drives, path).cloned() else { return status(NO_DRIVE.to_owned()) };
    let Some(way) = eject::offer(&drive) else { return status(eject::CANNOT.to_owned()) };
    let mut owner = 0;
    crate::navigation::with_current(|nav| {
        nav.leave_drive(&drive.path);
        owner = nav.owner();
    });
    let doing = if way == EjectWay::Eject { "Ejecting" } else { "Disconnecting" };
    status(format!("{doing} {}…", drive.label));
    std::thread::spawn(move || {
        std::thread::sleep(GRACE);
        let result = eject::eject(&drive, owner);
        let _ = slint::invoke_from_event_loop(move || finish(&drive, way, result));
    });
}

fn finish(drive: &Drive, way: EjectWay, result: Result<(), String>) {
    match result {
        Ok(()) => {
            status(match way {
                EjectWay::Eject => format!("{} can be removed", drive.label),
                EjectWay::Disconnect => format!("Disconnected {}", drive.label),
            });
            crate::sidebar::with_current(|sidebar| sidebar.check_drives(true));
            crate::navigation::with_current(|nav| {
                if nav.active_location() == Location::Drives {
                    nav.reload();
                }
            });
        }
        // The tabs stay on This PC; Back returns to the folder.
        Err(why) => status(why),
    }
}

fn status(text: String) {
    crate::navigation::with_current(|nav| nav.status(text));
}

#[cfg(test)]
mod tests {
    use super::*;
    use gezik_platform::DriveKind;
    use std::path::PathBuf;

    fn drive(path: &str, kind: DriveKind) -> Drive {
        Drive { path: PathBuf::from(path), label: path.to_owned(), kind }
    }

    #[test]
    fn the_drive_of_a_path_is_the_deepest_root() {
        let windows = cfg!(windows);
        let drives = if windows {
            vec![drive(r"C:\", DriveKind::Fixed), drive(r"E:\", DriveKind::Removable)]
        } else {
            vec![drive("/", DriveKind::Fixed), drive("/media/u/USB", DriveKind::Removable)]
        };
        let inside = if windows { r"e:\Photos\a.jpg" } else { "/media/u/USB/Photos" };
        let home = if windows { r"C:\Users" } else { "/home/u" };
        assert_eq!(drive_for(&drives, Path::new(inside)).map(|d| d.kind.clone()), Some(DriveKind::Removable));
        assert_eq!(drive_for(&drives, Path::new(home)).map(|d| d.kind.clone()), Some(DriveKind::Fixed));
        assert!(drive_for(&drives, Path::new(r"\\nas\foto")).is_none(), "a share is no drive");
        assert!(drive_for(&[], Path::new(home)).is_none());
    }
}
