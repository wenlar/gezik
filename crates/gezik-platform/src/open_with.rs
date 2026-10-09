//! "Open With ▸" (spec 9 §4.3): which apps open the selected items, and opening them with one.
//! The list is merged here (tested on every system); macOS asks LaunchServices (`mac::open_with`).

use std::path::{Path, PathBuf};

/// Most apps listed (`OPEN_WITH_FIRST` 1700-1739); the rest are reached through Other….
pub const MAX_APPS: usize = 40;
/// Up to this many items the list is the apps that open all of them; beyond, the focused one's.
pub const MAX_ITEMS: usize = 50;

/// An app offered in Open With ▸.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppChoice {
    pub path: PathBuf,
    /// What Finder calls it ("Preview").
    pub name: String,
    /// The default app of every item.
    pub default: bool,
}

/// The items whose apps are asked: the selection up to `MAX_ITEMS`, else (or when nothing is
/// selected) the focused one.
pub fn asked_items(selected: &[PathBuf], focused: Option<&Path>) -> Vec<PathBuf> {
    if !selected.is_empty() && selected.len() <= MAX_ITEMS {
        selected.to_vec()
    } else {
        focused.map(Path::to_path_buf).into_iter().collect()
    }
}

/// The menu's apps from each item's (default, apps): those every item has; the first item's
/// default first and marked when it is every item's default; the rest by name; at most `MAX_APPS`.
pub fn merge(lists: &[(Option<PathBuf>, Vec<PathBuf>)], name: impl Fn(&Path) -> String) -> Vec<AppChoice> {
    let Some(((first_default, first), rest)) = lists.split_first() else { return Vec::new() };
    let shared_default =
        first_default.clone().filter(|app| rest.iter().all(|(default, _)| default.as_ref() == Some(app)));
    let mut apps: Vec<AppChoice> = Vec::new();
    for app in first {
        if apps.iter().any(|a| &a.path == app) || !rest.iter().all(|(_, apps)| apps.contains(app)) {
            continue;
        }
        let default = shared_default.as_ref() == Some(app);
        apps.push(AppChoice { path: app.clone(), name: name(app), default });
    }
    apps.sort_by_cached_key(|a| (!a.default, a.name.to_lowercase(), a.path.clone()));
    apps.truncate(MAX_APPS);
    apps
}

/// The apps for `items` (from `asked_items`), merged. Any thread; empty off macOS (9b10 has Linux's).
pub fn apps(items: &[PathBuf]) -> Vec<AppChoice> {
    #[cfg(target_os = "macos")]
    {
        let lists: Vec<_> = items.iter().map(|item| crate::mac::open_with::apps_of(item)).collect();
        merge(&lists, crate::mac::open_with::app_name)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = items;
        Vec::new()
    }
}

/// An app's name for messages: Finder's on macOS, else the file name without `.app`.
pub fn app_name_of(app: &Path) -> String {
    #[cfg(target_os = "macos")]
    return crate::mac::open_with::app_name(app);
    #[cfg(not(target_os = "macos"))]
    app.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Opens `paths` with `app` in one request. `on_error` is called later, on another thread, if
/// the app could not open them.
pub fn open(paths: &[PathBuf], app: &Path, on_error: impl FnOnce(String) + Send + 'static) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return crate::mac::open_with::open(paths, app, Box::new(on_error));
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (paths, app, on_error);
        Err("Open With is on macOS only".to_owned())
    }
}

/// Asks for an app in `/Applications` (Other…). Main thread; blocks until the panel closes.
pub fn choose_app() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    return crate::mac::open_with::choose_app();
    #[cfg(not(target_os = "macos"))]
    None
}

/// Opens `file` (only it) with `app` from now on (the Info window). `on_error` is called later,
/// on another thread, if the system refused.
pub fn set_default_for_file(
    app: &Path,
    file: &Path,
    on_error: impl FnOnce(String) + Send + 'static,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return crate::mac::info::set_for_file(app, file, Box::new(on_error));
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, file, on_error);
        Err("This is on macOS only".to_owned())
    }
}

/// Opens every file of `file`'s type with `app` ("Change All…").
pub fn set_default_for_type(app: &Path, file: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return crate::mac::info::set_for_type(app, file);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, file);
        Err("This is on macOS only".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    fn name(path: &Path) -> String {
        path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
    }

    #[test]
    fn items_asked_are_the_selection_or_the_focused_one() {
        let few = vec![p("/a.pdf"), p("/b.pdf")];
        assert_eq!(asked_items(&few, Some(Path::new("/b.pdf"))), few);
        let many: Vec<PathBuf> = (0..=MAX_ITEMS).map(|i| p(&format!("/{i}.txt"))).collect();
        assert_eq!(asked_items(&many, Some(Path::new("/7.txt"))), [p("/7.txt")], "beyond 50: the focused one's");
        assert_eq!(asked_items(&many[..MAX_ITEMS], None).len(), MAX_ITEMS, "50 are all asked");
        assert_eq!(asked_items(&[], Some(Path::new("/f.txt"))), [p("/f.txt")], "nothing selected: the focused one");
        assert!(asked_items(&[], None).is_empty());
    }

    #[test]
    fn open_with_lists_only_apps_for_every_item() {
        let (preview, acrobat, safari) = (p("/A/Preview.app"), p("/A/acrobat.app"), p("/A/Safari.app"));
        let one = (Some(preview.clone()), vec![preview.clone(), safari.clone(), acrobat.clone()]);
        let names: Vec<(String, bool)> =
            merge(std::slice::from_ref(&one), name).into_iter().map(|a| (a.name, a.default)).collect();
        assert_eq!(
            names,
            [("Preview".into(), true), ("acrobat".into(), false), ("Safari".into(), false)],
            "the default first, then by name whatever the case"
        );
        // Two items: only the apps both open; the same default stays marked.
        let two = (Some(preview.clone()), vec![preview.clone(), acrobat.clone()]);
        let both = merge(&[one.clone(), two], name);
        assert_eq!(both.iter().map(|a| a.path.clone()).collect::<Vec<_>>(), [preview.clone(), acrobat.clone()]);
        assert!(both[0].default);
        // Defaults differ: nothing is marked, the order is by name.
        let other = (Some(acrobat.clone()), vec![acrobat.clone(), preview.clone()]);
        let mixed = merge(&[one.clone(), other], name);
        assert!(mixed.iter().all(|a| !a.default));
        assert_eq!(mixed[0].path, acrobat);
        // An item no app opens: nothing; a repeated app once; at most MAX_APPS.
        assert!(merge(&[one.clone(), (None, Vec::new())], name).is_empty());
        let twice = (None, vec![safari.clone(), safari.clone()]);
        assert_eq!(merge(&[twice], name).len(), 1);
        let lots: Vec<PathBuf> = (0..60).map(|i| p(&format!("/A/App{i:02}.app"))).collect();
        assert_eq!(merge(&[(None, lots)], name).len(), MAX_APPS);
        assert!(merge(&[], name).is_empty());
    }
}
