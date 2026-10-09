//! Known folders and drives, loaded off the UI thread, plus display titles for locations.

use std::path::Path;

use gezik_core::nav::Location;
use gezik_platform::{Drive, KnownFolder};

use crate::AppWindow;

#[derive(Debug, Clone, Default)]
pub struct Places {
    pub known: Vec<KnownFolder>,
    pub drives: Vec<Drive>,
}

/// One half of [`Places`]: known folders and drives arrive separately, so a slow drive
/// (an offline network share) never holds back the folders.
#[derive(Debug, Clone)]
pub enum PlacesPart {
    Known(Vec<KnownFolder>),
    Drives(Vec<Drive>),
}

impl Places {
    /// Replaces the half that `part` carries; the other half stays as it was.
    pub fn apply(&mut self, part: PlacesPart) {
        match part {
            PlacesPart::Known(known) => self.known = known,
            PlacesPart::Drives(drives) => self.drives = drives,
        }
    }

    /// Tab title for a location: the OS name of known folders and drives, else the folder name.
    pub fn title_for(&self, location: &Location) -> String {
        let path = match location {
            Location::Path(path) => path,
            Location::Drives => return gezik_core::nav::DRIVES_NAME.to_owned(),
            Location::Search(spec) => return spec.title(),
            Location::Flat(folder) => {
                return format!("{} (all files)", self.title_for(&Location::Path(folder.clone())));
            }
        };
        if let Some(folder) = self.known.iter().find(|f| f.path == *path) {
            return folder.name.clone();
        }
        if let Some(drive) = self.drives.iter().find(|d| d.path == *path) {
            return drive.label.clone();
        }
        gezik_platform::finder::finder_name(path).unwrap_or_else(|| file_name_or_path(path))
    }
}

fn file_name_or_path(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string())
}

/// Reads known folders and drives on two background threads, then calls `on_ready` on the
/// UI thread once for each, as soon as it is ready.
pub fn load_in_background(window: slint::Weak<AppWindow>, on_ready: impl Fn(PlacesPart) + Clone + Send + 'static) {
    spawn_part(window.clone(), on_ready.clone(), || PlacesPart::Known(gezik_platform::known_folders()));
    spawn_part(window, on_ready, || PlacesPart::Drives(gezik_platform::drives()));
}

fn spawn_part(
    window: slint::Weak<AppWindow>,
    on_ready: impl Fn(PlacesPart) + Send + 'static,
    read: impl FnOnce() -> PlacesPart + Send + 'static,
) {
    std::thread::spawn(move || {
        let part = read();
        let _ = window.upgrade_in_event_loop(move |_| on_ready(part));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn titles_prefer_os_names() {
        let places = Places {
            known: vec![KnownFolder { path: PathBuf::from("/u/a/Docs"), name: "Belgeler".into() }],
            drives: vec![Drive {
                path: PathBuf::from("/mnt/x"),
                label: "Data".into(),
                kind: gezik_platform::DriveKind::Fixed,
            }],
        };
        assert_eq!(places.title_for(&Location::Path("/u/a/Docs".into())), "Belgeler");
        assert_eq!(places.title_for(&Location::Path("/mnt/x".into())), "Data");
        assert_eq!(places.title_for(&Location::Path("/u/a/Projects".into())), "Projects");
        assert_eq!(places.title_for(&Location::Drives), gezik_core::nav::DRIVES_NAME);
        assert_eq!(places.title_for(&Location::Flat("/u/a/Docs".into())), "Belgeler (all files)");
    }

    #[test]
    fn parts_replace_only_their_half() {
        let folder = KnownFolder { path: PathBuf::from("/u/a"), name: "Home".into() };
        let drive =
            Drive { path: PathBuf::from("/"), label: "File System".into(), kind: gezik_platform::DriveKind::Fixed };
        let mut places = Places::default();
        places.apply(PlacesPart::Known(vec![folder.clone()]));
        assert_eq!(places.known, std::slice::from_ref(&folder));
        assert!(places.drives.is_empty());
        places.apply(PlacesPart::Drives(vec![drive.clone()]));
        assert_eq!(
            (places.known.as_slice(), places.drives.as_slice()),
            (std::slice::from_ref(&folder), std::slice::from_ref(&drive))
        );
        // A later drive reload (polling) keeps the folders.
        places.apply(PlacesPart::Drives(Vec::new()));
        assert_eq!(places.known, [folder]);
        assert!(places.drives.is_empty());
    }
}
