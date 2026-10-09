//! macOS row menu extras (spec 9 §4.3): Open With ▸, Share…, Quick Actions ▸. What they list
//! is asked when the menu opens, on a short-lived thread, never ahead of it.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gezik_platform::open_with::AppChoice;

use crate::context_menu::{OPEN, OPEN_DEFAULT, SHOW_PACKAGE};

/// What a macOS row menu lists besides Gezik's own items.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Extras {
    pub apps: Vec<AppChoice>,
    pub services: Vec<gezik_platform::services::Service>,
}

/// A list that came after its menu had opened, kept for the same items' next menu.
pub type Late = Arc<Mutex<Option<(Vec<PathBuf>, Extras)>>>;

/// How long a menu waits for its list (spec 9 §4.3).
pub const WAIT: Duration = Duration::from_millis(50);

fn compute(items: &[PathBuf]) -> Extras {
    Extras { apps: gezik_platform::open_with::apps(items), services: gezik_platform::services::for_items(items) }
}

/// The extras for `items`, or `None` ("Loading…") when they take longer than `WAIT`.
pub fn fetch(late: &Late, items: Vec<PathBuf>) -> Option<Extras> {
    fetch_with(late, items, WAIT, compute)
}

fn fetch_with(late: &Late, items: Vec<PathBuf>, wait: Duration, compute: fn(&[PathBuf]) -> Extras) -> Option<Extras> {
    if let Ok(mut kept) = late.lock()
        && kept.as_ref().is_some_and(|(kept_items, _)| *kept_items == items)
    {
        return kept.take().map(|(_, extras)| extras);
    }
    let (send, receive) = std::sync::mpsc::channel();
    let keep = late.clone();
    let spawned = std::thread::Builder::new().name("gezik-finder-menu".into()).spawn(move || {
        let extras = compute(&items);
        // The menu stopped waiting: kept for the same items' next menu (one list only).
        if let Err(unsent) = send.send(extras)
            && let Ok(mut kept) = keep.lock()
        {
            *kept = Some((items, unsent.0));
        }
    });
    spawned.ok()?;
    receive.recv_timeout(wait).ok()
}

/// Whether Open With ▸ is offered for `rows`: on macOS, up to `MAX_ITEMS` files or packages
/// (beyond, the list would be one item's, yet every row would open with the app).
pub fn offers_open_with(rows: &[(PathBuf, bool)], mac: bool) -> bool {
    mac && !rows.is_empty()
        && rows.len() <= gezik_platform::open_with::MAX_ITEMS
        && rows.iter().all(|(path, is_dir)| {
            !is_dir || path.file_name().is_some_and(|n| gezik_core::kind::is_package_name(&n.to_string_lossy()))
        })
}

/// Where Open With ▸ goes: after Open, Open with default app and Show Package Contents.
pub fn open_with_place(list: &[(u32, String)]) -> usize {
    list.iter().rposition(|(id, _)| matches!(*id, OPEN | OPEN_DEFAULT | SHOW_PACKAGE)).map_or(0, |i| i + 1)
}

/// Opens `paths` with `app`, or with one chosen in /Applications (Other…) once the menu is done.
pub fn open_with(window: &slint::Weak<crate::AppWindow>, paths: Vec<PathBuf>, app: Option<PathBuf>) {
    let window = window.clone();
    slint::Timer::single_shot(Duration::ZERO, move || {
        let Some(app) = app.or_else(gezik_platform::open_with::choose_app) else { return };
        let name = gezik_platform::open_with::app_name_of(&app);
        let report = window.clone();
        let failed = move |why: String| {
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(window) = report.upgrade() {
                    window.set_status(format!("{name} could not open them: {why}").into());
                }
            });
        };
        if let Err(why) = gezik_platform::open_with::open(&paths, &app, failed)
            && let Some(window) = window.upgrade()
        {
            window.set_status(why.into());
        }
    });
}

/// Share… for `paths` at window position `at` (a right-click), else at the pointer.
pub fn share(window: &crate::AppWindow, paths: Vec<PathBuf>, at: Option<(f32, f32)>) {
    use slint::ComponentHandle;
    let at = at.map(|(x, y)| (f64::from(x), f64::from(y)));
    if let Err(why) = gezik_platform::finder::share(&window.window().window_handle(), &paths, at) {
        window.set_status(format!("Cannot share: {why}").into());
    }
}

/// The `share` action: the selection, or the focused item. macOS only (hidden elsewhere).
pub fn share_selection(window: &crate::AppWindow, view: &crate::view::View) {
    if !cfg!(target_os = "macos") || view.shows_drives() {
        return;
    }
    let mut items: Vec<PathBuf> = view.selected_items().into_iter().map(|(path, _)| path).collect();
    if items.is_empty() {
        items.extend(view.focus().and_then(|i| view.entry_path(i)).map(|(path, _)| path));
    }
    if !items.is_empty() {
        share(window, items, None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gezik_platform::open_with::AppChoice;

    fn app(name: &str) -> AppChoice {
        AppChoice { path: format!("/A/{name}.app").into(), name: name.to_owned(), default: false }
    }

    /// Built through `Default`: Task 2 adds a field, and these stay as they are.
    fn extras(name: &str) -> Extras {
        let mut extras = Extras::default();
        extras.apps.push(app(name));
        extras
    }

    fn quick(_: &[PathBuf]) -> Extras {
        extras("Quick")
    }

    fn slow(_: &[PathBuf]) -> Extras {
        std::thread::sleep(Duration::from_millis(150));
        extras("Slow")
    }

    #[test]
    fn a_slow_list_opens_the_menu_with_loading() {
        let late = Late::default();
        let items = vec![PathBuf::from("/a.pdf")];
        assert_eq!(fetch_with(&late, items.clone(), WAIT, quick).unwrap().apps, [app("Quick")]);
        let start = std::time::Instant::now();
        assert_eq!(fetch_with(&late, items, Duration::from_millis(20), slow), None);
        assert!(start.elapsed() < Duration::from_millis(120), "the menu does not wait for the list");
    }

    #[test]
    fn late_lists_serve_only_the_same_items() {
        let late = Late::default();
        let a = vec![PathBuf::from("/a.pdf")];
        let b = vec![PathBuf::from("/b.pdf")];
        assert_eq!(fetch_with(&late, a.clone(), Duration::from_millis(10), slow), None);
        std::thread::sleep(Duration::from_millis(250));
        // Another selection: asked again, never the late list.
        assert_eq!(fetch_with(&late, b, WAIT, quick).unwrap().apps, [app("Quick")]);
        assert!(late.lock().unwrap().is_some(), "a's list is still kept");
        // The same items: the late list, at once, once.
        let start = std::time::Instant::now();
        assert_eq!(fetch_with(&late, a.clone(), WAIT, slow).unwrap().apps, [app("Slow")]);
        assert!(start.elapsed() < Duration::from_millis(50));
        assert!(late.lock().unwrap().is_none());
    }

    #[test]
    fn open_with_is_offered_for_files_and_packages_on_macos() {
        let file = (PathBuf::from("/x/a.pdf"), false);
        let app = (PathBuf::from("/Applications/Safari.app"), true);
        let folder = (PathBuf::from("/x/Docs"), true);
        assert!(offers_open_with(&[file.clone(), app.clone()], true));
        assert!(!offers_open_with(&[file.clone(), folder], true), "a plain folder opens in Gezik");
        assert!(!offers_open_with(std::slice::from_ref(&file), false), "not off macOS");
        assert!(!offers_open_with(&[], true));
        let many = vec![file.clone(); gezik_platform::open_with::MAX_ITEMS];
        assert!(offers_open_with(&many, true), "50 rows are asked together");
        assert!(!offers_open_with(&[many, vec![file]].concat(), true), "not for more rows than are asked");
    }

    #[test]
    fn open_with_goes_after_the_open_items() {
        let list = |ids: &[u32]| ids.iter().map(|id| (*id, String::new())).collect::<Vec<_>>();
        use crate::context_menu::{CUT, OPEN, OPEN_DEFAULT, OPEN_IN_NEW_TAB, PIN, SHOW_PACKAGE};
        assert_eq!(open_with_place(&list(&[OPEN, OPEN_DEFAULT, CUT])), 2);
        assert_eq!(open_with_place(&list(&[OPEN_IN_NEW_TAB, SHOW_PACKAGE, PIN])), 2, "a package");
        assert_eq!(open_with_place(&list(&[OPEN, CUT])), 1, "several rows");
        assert_eq!(open_with_place(&list(&[CUT])), 0);
    }
}
