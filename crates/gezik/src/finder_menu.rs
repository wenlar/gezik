//! Row menu extras (spec 9 §4.3, §8.5): Open With ▸ (macOS and Linux), Share… and Quick
//! Actions ▸ (macOS). What they list is asked when the menu opens, on a short-lived thread,
//! never ahead of it.

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

/// Whether Open With ▸ is offered for `rows` where the system lists apps (`supported`): up
/// to `MAX_ITEMS` files, and on macOS (`mac`) packages (beyond `MAX_ITEMS` the list would be
/// one item's, yet every row would open with the app).
pub fn offers_open_with(rows: &[(PathBuf, bool)], supported: bool, mac: bool) -> bool {
    supported
        && !rows.is_empty()
        && rows.len() <= gezik_platform::open_with::MAX_ITEMS
        && rows.iter().all(|(path, is_dir)| {
            !is_dir
                || (mac && path.file_name().is_some_and(|n| gezik_core::kind::is_package_name(&n.to_string_lossy())))
        })
}

/// Where Open With ▸ goes: after Open, Open with default app and Show Package Contents.
pub fn open_with_place(list: &[(u32, String)]) -> usize {
    list.iter().rposition(|(id, _)| matches!(*id, OPEN | OPEN_DEFAULT | SHOW_PACKAGE)).map_or(0, |i| i + 1)
}

/// Opens `paths` with `app`, or with one chosen with Other… once the menu is done (macOS'
/// panel; Linux Gezik's own question, `ask_app`).
pub fn open_with(window: &slint::Weak<crate::AppWindow>, paths: Vec<PathBuf>, app: Option<PathBuf>) {
    let window = window.clone();
    slint::Timer::single_shot(Duration::ZERO, move || match app.or_else(gezik_platform::open_with::choose_app) {
        Some(app) => open_now(&window, &paths, &app),
        None if gezik_platform::open_with::ASKS_IN_GEZIK => ask_app(move |app| open_now(&window, &paths, &app)),
        None => {}
    });
}

fn open_now(window: &slint::Weak<crate::AppWindow>, paths: &[PathBuf], app: &std::path::Path) {
    let name = gezik_platform::open_with::app_name_of(app);
    let report = window.clone();
    let failed = move |why: String| {
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(window) = report.upgrade() {
                window.set_status(format!("{name} could not open them: {why}").into());
            }
        });
    };
    if let Err(why) = gezik_platform::open_with::open(paths, app, failed)
        && let Some(window) = window.upgrade()
    {
        window.set_status(why.into());
    }
}

/// Other…'s title on Linux.
pub const OTHER_TITLE: &str = "Open With";

type Chosen = Box<dyn FnOnce(PathBuf)>;

thread_local! {
    /// Other…'s answer, kept on the UI thread while the apps are read.
    static WAITING: std::cell::RefCell<Option<Chosen>> = const { std::cell::RefCell::new(None) };
}

/// Linux Other… (deviation 2 of 9b10): reads the installed apps on a thread, then asks for one
/// by name; `chosen` gets its desktop file. A second Other… while the first reads replaces it.
pub fn ask_app(chosen: impl FnOnce(PathBuf) + 'static) {
    WAITING.with(|w| *w.borrow_mut() = Some(Box::new(chosen)));
    let spawned = std::thread::Builder::new().name("gezik-apps".into()).spawn(|| {
        let apps = gezik_platform::open_with::all_apps();
        let _ = slint::invoke_from_event_loop(move || show_apps(apps));
    });
    if spawned.is_err() {
        WAITING.with(|w| w.borrow_mut().take());
    }
}

fn show_apps(apps: Vec<AppChoice>) {
    let Some(chosen) = WAITING.with(|w| w.borrow_mut().take()) else { return };
    let apps = std::rc::Rc::new(apps);
    let (for_note, for_answer) = (apps.clone(), apps);
    crate::operations::with_current(move |ops| {
        ops.dialogs().ask_text_noted(
            OTHER_TITLE,
            "Type the app's name.",
            "",
            &["Open", "Cancel"],
            move |text| other_note(&for_note, text),
            move |answer| {
                let app = answer
                    .and_then(|text| gezik_platform::open_with::find_app(&for_answer, &text).map(|a| a.path.clone()));
                if let Some(app) = app {
                    chosen(app);
                }
            },
        );
    });
}

/// The line under Other…'s field.
pub fn other_note(apps: &[AppChoice], text: &str) -> (String, bool) {
    if apps.is_empty() {
        return ("No apps were found".to_owned(), true);
    }
    match gezik_platform::open_with::find_app(apps, text) {
        Some(app) => (format!("Opens with {}", app.name), false),
        None if text.trim().is_empty() => ("Type part of the app's name".to_owned(), false),
        None => ("No app has that name".to_owned(), true),
    }
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
    fn open_with_is_offered_where_the_system_lists_apps() {
        let file = (PathBuf::from("/a/x.pdf"), false);
        let app = (PathBuf::from("/A/Preview.app"), true);
        let folder = (PathBuf::from("/a/b"), true);
        assert!(offers_open_with(&[file.clone(), app.clone()], true, true));
        assert!(!offers_open_with(&[file.clone(), folder.clone()], true, true), "a plain folder opens in Gezik");
        assert!(!offers_open_with(std::slice::from_ref(&file), false, false), "not where no apps are listed");
        assert!(offers_open_with(std::slice::from_ref(&file), true, false), "Linux: files");
        assert!(!offers_open_with(&[file.clone(), app], true, false), "Linux: a .app folder is a folder");
        assert!(!offers_open_with(&[], true, true));
        let many = vec![file.clone(); gezik_platform::open_with::MAX_ITEMS];
        assert!(offers_open_with(&many, true, true), "50 rows are asked together");
        assert!(!offers_open_with(&[many, vec![file]].concat(), true, true), "not for more rows than are asked");
    }

    #[test]
    fn others_note_says_which_app_opens() {
        let apps = [AppChoice { path: PathBuf::from("/a/gimp.desktop"), name: "GIMP".into(), default: false }];
        assert_eq!(other_note(&apps, "gi"), ("Opens with GIMP".to_owned(), false));
        assert_eq!(other_note(&apps, ""), ("Type part of the app's name".to_owned(), false));
        assert_eq!(other_note(&apps, "krita"), ("No app has that name".to_owned(), true));
        assert_eq!(other_note(&[], "gimp"), ("No apps were found".to_owned(), true));
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
