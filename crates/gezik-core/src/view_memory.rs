//! The view of each folder the user changed (mode, sort, grid size), so that folder looks
//! the same next time. At most [`MAX_FOLDERS`]; the least recently used is dropped.

use crate::view::ViewSettings;

pub const MAX_FOLDERS: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderView {
    /// The folder as displayed (`C:\Users\me\Pictures`).
    pub path: String,
    pub view: ViewSettings,
    /// When it was last used, as a counter: higher is more recent.
    pub used: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ViewMemory {
    folders: Vec<FolderView>,
    clock: u64,
}

impl ViewMemory {
    /// From a saved list: duplicates (the same folder twice) keep the most recent, and
    /// only the [`MAX_FOLDERS`] most recent are kept.
    pub fn from_folders(mut folders: Vec<FolderView>) -> ViewMemory {
        folders.sort_by_key(|f| std::cmp::Reverse(f.used));
        let mut kept: Vec<FolderView> = Vec::new();
        for folder in folders {
            if kept.len() < MAX_FOLDERS && !kept.iter().any(|k| same_folder(&k.path, &folder.path)) {
                kept.push(folder);
            }
        }
        let clock = kept.iter().map(|f| f.used).max().unwrap_or(0);
        ViewMemory { folders: kept, clock }
    }

    pub fn folders(&self) -> &[FolderView] {
        &self.folders
    }

    /// Whether `path` has a view of its own (not counted as a use).
    pub fn contains(&self, path: &str) -> bool {
        self.folders.iter().any(|f| same_folder(&f.path, path))
    }

    /// The view saved for `path`, which counts as a use.
    pub fn get(&mut self, path: &str) -> Option<ViewSettings> {
        self.clock += 1;
        let clock = self.clock;
        let folder = self.folders.iter_mut().find(|f| same_folder(&f.path, path))?;
        folder.used = clock;
        Some(folder.view)
    }

    pub fn set(&mut self, path: &str, view: ViewSettings) {
        self.clock += 1;
        if let Some(folder) = self.folders.iter_mut().find(|f| same_folder(&f.path, path)) {
            folder.view = view;
            folder.used = self.clock;
            return;
        }
        if self.folders.len() >= MAX_FOLDERS
            && let Some(oldest) = self.folders.iter().enumerate().min_by_key(|(_, f)| f.used).map(|(i, _)| i)
        {
            self.folders.swap_remove(oldest);
        }
        self.folders.push(FolderView { path: path.to_owned(), view, used: self.clock });
    }

    pub fn remove(&mut self, path: &str) -> bool {
        let before = self.folders.len();
        self.folders.retain(|f| !same_folder(&f.path, path));
        self.folders.len() != before
    }

    pub fn clear(&mut self) {
        self.folders.clear();
    }
}

/// Whether two displayed paths are the same folder: on Windows case-insensitively and with
/// either separator; a trailing separator is ignored.
pub fn same_folder(a: &str, b: &str) -> bool {
    folder_key(a) == folder_key(b)
}

fn folder_key(path: &str) -> String {
    let mut key = if cfg!(windows) { path.replace('/', "\\").to_lowercase() } else { path.to_owned() };
    while key.len() > 1 && (key.ends_with('/') || key.ends_with('\\')) && !key.ends_with(":\\") {
        key.pop();
    }
    key
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::{GridSize, ViewMode};

    fn grid() -> ViewSettings {
        ViewSettings { mode: ViewMode::Grid, ..ViewSettings::default() }
    }

    #[test]
    fn remembers_and_updates_a_folder() {
        let mut m = ViewMemory::default();
        assert_eq!(m.get("/pics"), None);
        m.set("/pics", grid());
        assert_eq!(m.get("/pics"), Some(grid()));
        let large = ViewSettings { grid_size: GridSize::Large, ..grid() };
        m.set("/pics/", large);
        assert_eq!(m.folders().len(), 1, "trailing separator is the same folder");
        assert_eq!(m.get("/pics"), Some(large));
        assert!(m.contains("/pics"));
        assert!(m.remove("/pics"));
        assert!(!m.remove("/pics"));
        assert!(!m.contains("/pics"));
    }

    #[test]
    fn drops_the_least_recently_used_beyond_the_limit() {
        let mut m = ViewMemory::default();
        for i in 0..MAX_FOLDERS {
            m.set(&format!("/f{i}"), grid());
        }
        m.get("/f0");
        m.set("/new", grid());
        assert_eq!(m.folders().len(), MAX_FOLDERS);
        assert!(m.get("/f0").is_some(), "used recently");
        assert!(m.get("/f1").is_none(), "the oldest went");
        assert!(m.get("/new").is_some());
    }

    #[test]
    fn loading_keeps_the_newest_duplicate_and_the_limit() {
        let f = |path: &str, used| FolderView { path: path.to_owned(), view: ViewSettings::default(), used };
        let mut folders: Vec<FolderView> = (0..MAX_FOLDERS as u64 + 10).map(|i| f(&format!("/f{i}"), i + 10)).collect();
        folders.push(FolderView { view: grid(), ..f("/f509", 9999) });
        let mut m = ViewMemory::from_folders(folders);
        assert_eq!(m.folders().len(), MAX_FOLDERS);
        assert_eq!(m.get("/f509"), Some(grid()));
        assert!(m.get("/f0").is_none());
        m.set("/later", grid());
        assert!(m.folders().iter().find(|x| x.path == "/later").unwrap().used > 9999);
    }

    #[cfg(windows)]
    #[test]
    fn windows_paths_ignore_case_and_separators() {
        assert!(same_folder(r"C:\Users\Me\Pictures", "c:/users/me/pictures/"));
        assert!(!same_folder(r"C:\", r"C:\Users"));
        assert_eq!(folder_key(r"C:\"), r"c:\");
    }
}
