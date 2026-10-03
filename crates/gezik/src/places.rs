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

impl Places {
    /// Tab title for a location: the OS name of known folders and drives, else the folder name.
    pub fn title_for(&self, location: &Location) -> String {
        let Location::Path(path) = location else { return "This PC".to_owned() };
        if let Some(folder) = self.known.iter().find(|f| f.path == *path) {
            return folder.name.clone();
        }
        if let Some(drive) = self.drives.iter().find(|d| d.path == *path) {
            return drive.label.clone();
        }
        file_name_or_path(path)
    }
}

fn file_name_or_path(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string())
}

/// Reads known folders and drives on a background thread, then calls `on_ready` on the UI thread.
pub fn load_in_background(window: slint::Weak<AppWindow>, on_ready: impl FnOnce(Places) + Send + 'static) {
    std::thread::spawn(move || {
        let places = Places { known: gezik_platform::known_folders(), drives: gezik_platform::drives() };
        let _ = window.upgrade_in_event_loop(move |_| on_ready(places));
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
        assert_eq!(places.title_for(&Location::Drives), "This PC");
    }
}
