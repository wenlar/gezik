//! The user's templates (spec 8.1): `<config>/templates/`, read on a thread of its own after
//! start and again whenever the folder changes (the UI thread reads no folder); New ▸ lists
//! what is kept here.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use gezik_core::nav::Location;
use gezik_core::templates::{Template, template_list};

thread_local! {
    /// The templates folder (none without a config folder) and its list as last read.
    static STATE: RefCell<(Option<PathBuf>, Vec<Template>)> = const { RefCell::new((None, Vec::new())) };
}

pub fn set_dir(dir: Option<PathBuf>) {
    STATE.with(|state| state.borrow_mut().0 = dir);
}

pub fn dir() -> Option<PathBuf> {
    STATE.with(|state| state.borrow().0.clone())
}

/// The templates as last read (at most `TEMPLATE_MAX`).
#[allow(dead_code)] // Task 6 uses this.
pub fn current() -> Vec<Template> {
    STATE.with(|state| state.borrow().1.clone())
}

/// The folder's entries as New ▸ lists them; none if it cannot be read. A link to a folder is
/// a folder template; a name that is not Unicode is left out (it is copied by name).
pub fn read(dir: &Path) -> Vec<Template> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    template_list(entries.filter_map(Result::ok).filter_map(|entry| {
        let path = entry.path();
        let name = entry.file_name().into_string().ok()?;
        let is_dir = std::fs::metadata(&path).is_ok_and(|meta| meta.is_dir());
        Some((name, is_dir, gezik_platform::fs::is_hidden_attr(&path)))
    }))
}

/// Reads `dir` (on the calling thread, never the UI's) and keeps the list on the UI thread.
pub fn refresh(dir: PathBuf) {
    let list = read(&dir);
    let _ = slint::invoke_from_event_loop(move || STATE.with(|state| state.borrow_mut().1 = list));
}

/// `refresh` of the templates folder on a thread of its own.
pub fn load_in_background() {
    let Some(dir) = dir() else { return };
    let _ = std::thread::Builder::new().name("gezik-templates".into()).spawn(move || refresh(dir));
}

/// "Open templates folder": made if it is not there (off the UI thread), then opened in a new
/// tab of Gezik.
#[allow(dead_code)] // Task 6 uses this.
pub fn open_folder() {
    let Some(dir) = dir() else {
        crate::view::with_current(|view| view.note("No config folder for templates".to_owned()));
        return;
    };
    let _ = std::thread::Builder::new().name("gezik-templates".into()).spawn(move || {
        let made = std::fs::create_dir_all(&dir);
        let _ = slint::invoke_from_event_loop(move || match made {
            Ok(()) => crate::navigation::with_current(|nav| nav.open_tab(Location::Path(dir), true)),
            Err(err) => {
                let why = gezik_platform::fs::describe(&err);
                crate::view::with_current(|view| view.note(format!("Cannot make the templates folder: {why}")));
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_folder_is_read_as_new_lists_it() {
        let dir = std::env::temp_dir().join(format!("gezik-templates-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Project/src")).unwrap();
        std::fs::write(dir.join("Report.docx"), "r").unwrap();
        std::fs::write(dir.join(".DS_Store"), "x").unwrap();
        let list = read(&dir);
        let shown: Vec<(&str, &str, bool)> =
            list.iter().map(|t| (t.name.as_str(), t.label.as_str(), t.is_dir)).collect();
        assert_eq!(shown, [("Project", "Project", true), ("Report.docx", "Report", false)]);
        assert!(read(&dir.join("missing")).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
