//! "Open With ▸" (spec 9 §4.3, §8.5): which apps open the selected items, and opening them
//! with one. The list is merged here (tested on every system); macOS asks LaunchServices
//! (`mac::open_with`), Linux its MIME lists (`linux::mime`).

use std::path::{Path, PathBuf};

/// Most apps listed (`OPEN_WITH_FIRST` 1700-1739); the rest are reached through Other….
pub const MAX_APPS: usize = 40;
/// Up to this many items the list is the apps that open all of them; beyond, the focused one's.
pub const MAX_ITEMS: usize = 50;
/// Whether the system lists apps for Open With ▸ (macOS LaunchServices, Linux's MIME lists).
pub const SUPPORTED: bool = cfg!(any(target_os = "macos", all(unix, not(target_os = "macos"))));
/// Whether an app can be set for one file (macOS); Linux has only a type's default.
pub const PER_FILE: bool = cfg!(target_os = "macos");
/// Whether Other… is Gezik's own question (Linux has no app chooser of its own).
pub const ASKS_IN_GEZIK: bool = cfg!(all(unix, not(target_os = "macos")));

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

/// The apps for `items` (from `asked_items`), merged. Any thread; empty on Windows.
pub fn apps(items: &[PathBuf]) -> Vec<AppChoice> {
    #[cfg(target_os = "macos")]
    {
        let lists: Vec<_> = items.iter().map(|item| crate::mac::open_with::apps_of(item)).collect();
        merge(&lists, crate::mac::open_with::app_name)
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        use crate::linux::mime;
        let (Some(xdg), Some(db)) = (mime::xdg(), mime::db()) else { return Vec::new() };
        let assoc = mime::associations(xdg);
        let lists: Vec<_> = items.iter().map(|item| mime::apps_of(xdg, db, &assoc, item, &mime::on_path)).collect();
        merge(&lists, mime::app_name)
    }
    #[cfg(windows)]
    {
        let _ = items;
        Vec::new()
    }
}

/// Other… on Linux: every app it may offer, by name; read on each call, not kept. Blocking:
/// not on the UI thread. Empty elsewhere.
pub fn all_apps() -> Vec<AppChoice> {
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        use crate::linux::mime;
        let Some(xdg) = mime::xdg() else { return Vec::new() };
        mime::all_apps(xdg, &mime::on_path)
            .into_iter()
            .map(|(path, entry)| AppChoice { path, name: entry.name, default: false })
            .collect()
    }
    #[cfg(not(all(unix, not(target_os = "macos"))))]
    Vec::new()
}

/// The app `text` names (decision 2): its name or file name in full, else the first whose
/// name starts with it, else the first whose name or file name holds it; case aside.
pub fn find_app<'a>(apps: &'a [AppChoice], text: &str) -> Option<&'a AppChoice> {
    let want = text.trim().to_lowercase();
    if want.is_empty() {
        return None;
    }
    let stem = |a: &AppChoice| a.path.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
    let name = |a: &AppChoice| a.name.to_lowercase();
    apps.iter()
        .find(|a| name(a) == want || stem(a) == want)
        .or_else(|| apps.iter().find(|a| name(a).starts_with(&want)))
        .or_else(|| apps.iter().find(|a| name(a).contains(&want) || stem(a).contains(&want)))
}

/// A Linux app's desktop id, for mimeapps.list (`Change All…`).
pub fn desktop_id_of(app: &Path) -> Option<String> {
    crate::linux::desktop_entry::desktop_id_of(app)
}

/// Whether `text` is a MIME type that is safe to write as a mimeapps.list key.
pub fn is_mime_type(text: &str) -> bool {
    crate::linux::mime::mime_like(text)
}

/// An app's name for messages: Finder's on macOS, its entry's Name on Linux, else the file name.
pub fn app_name_of(app: &Path) -> String {
    #[cfg(target_os = "macos")]
    return crate::mac::open_with::app_name(app);
    #[cfg(all(unix, not(target_os = "macos")))]
    return crate::linux::mime::app_name(app);
    #[cfg(windows)]
    app.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Opens `paths` with `app` in one request. `on_error` is called later, on another thread, if
/// the app could not open them.
pub fn open(paths: &[PathBuf], app: &Path, on_error: impl FnOnce(String) + Send + 'static) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return crate::mac::open_with::open(paths, app, Box::new(on_error));
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let (paths, app) = (paths.to_vec(), app.to_path_buf());
        std::thread::Builder::new()
            .name("gezik-open-with".into())
            .spawn(move || {
                if let Err(why) = crate::linux::desktop_entry::launch(&app, &paths) {
                    on_error(why);
                }
            })
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    #[cfg(windows)]
    {
        let _ = (paths, app, on_error);
        Err("Open With is on macOS and Linux only".to_owned())
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

/// `file`'s type for `Change All…`: macOS its type identifier ("com.adobe.pdf"), Linux its
/// MIME type by name ("application/pdf"); None on Windows or when unknown.
pub fn type_of(file: &Path) -> Option<String> {
    #[cfg(target_os = "macos")]
    return crate::mac::services::type_of(file);
    #[cfg(all(unix, not(target_os = "macos")))]
    return crate::linux::mime::db()?.type_of_name(&file.file_name()?.to_string_lossy());
    #[cfg(windows)]
    {
        let _ = file;
        None
    }
}

/// Opens every file of type `uti` (from `type_of`) with `app` ("Change All…").
pub fn set_default_for_type(app: &Path, uti: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return crate::mac::info::set_for_type(app, uti);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, uti);
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

    #[test]
    fn other_finds_an_app_by_its_name() {
        let apps = [
            AppChoice {
                path: p("/a/org.gimp.GIMP.desktop"),
                name: "GNU Image Manipulation Program".into(),
                default: false,
            },
            AppChoice { path: p("/a/gimp-viewer.desktop"), name: "Gimp Viewer".into(), default: false },
            AppChoice { path: p("/a/eog.desktop"), name: "Image Viewer".into(), default: false },
        ];
        let found = |t: &str| find_app(&apps, t).map(|a| a.name.as_str());
        assert_eq!(found("image viewer"), Some("Image Viewer"), "the whole name first");
        assert_eq!(found("EOG"), Some("Image Viewer"), "the file name");
        assert_eq!(found("gimp"), Some("Gimp Viewer"), "the start of a name before a part of one");
        assert_eq!(found("manipulation"), Some("GNU Image Manipulation Program"));
        assert_eq!(found("org.gimp"), Some("GNU Image Manipulation Program"), "a part of the file name");
        assert_eq!(found("  "), None);
        assert_eq!(found("krita"), None);
    }

    #[test]
    fn only_macos_and_linux_list_apps() {
        assert_eq!(SUPPORTED, cfg!(any(target_os = "macos", all(unix, not(target_os = "macos")))));
        assert_eq!(PER_FILE, cfg!(target_os = "macos"));
        assert_eq!(ASKS_IN_GEZIK, cfg!(all(unix, not(target_os = "macos"))));
        assert!(is_mime_type("text/plain") && !is_mime_type("text/plain\n[x]"));
        assert_eq!(desktop_id_of(Path::new("/usr/share/applications/eog.desktop")).as_deref(), Some("eog.desktop"));
    }
}
