//! Cloud drives in the app (spec 9 §7.2-7.3): the roots found with the places, whether a
//! folder's rows show their cloud state, and (part 9b5's commands) keep / free up.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use gezik_platform::cloud::CloudRoot;

#[derive(Default)]
struct State {
    roots: Vec<CloudRoot>,
    /// The last folder asked about and the answer: rows ask once per line drawn.
    last: Option<(PathBuf, bool)>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// The roots the places load found (UI thread).
pub fn set_roots(roots: Vec<CloudRoot>) {
    STATE.with(|s| *s.borrow_mut() = State { roots, last: None });
}

pub fn root_of(path: &Path) -> Option<CloudRoot> {
    STATE.with(|s| gezik_platform::cloud::root_of(&s.borrow().roots, path).cloned())
}

/// Whether rows of folder `dir` show their cloud state: under a root, and not on Linux (no
/// state there, spec 17 decision 19).
pub fn shows_state(dir: &Path) -> bool {
    if cfg!(not(any(windows, target_os = "macos"))) {
        return false;
    }
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if let Some((last, answer)) = &s.last
            && last == dir
        {
            return *answer;
        }
        let answer = gezik_platform::cloud::root_of(&s.roots, dir).is_some();
        s.last = Some((dir.to_path_buf(), answer));
        answer
    })
}

pub const NOT_IN_CLOUD: &str = "Not in a cloud folder";
pub const NOTHING_CHOSEN: &str = "Choose the items first";
pub const NO_COMMANDS_HERE: &str = "Cloud folders on Linux have no download commands";

/// Why the command cannot run for `paths`; `None`: it can.
fn refusal(paths: &[PathBuf]) -> Option<&'static str> {
    if cfg!(not(any(windows, target_os = "macos"))) {
        return Some(NO_COMMANDS_HERE);
    }
    if paths.is_empty() {
        return Some(NOTHING_CHOSEN);
    }
    if paths.iter().any(|p| root_of(p).is_none()) {
        return Some(NOT_IN_CLOUD);
    }
    None
}

/// The selection, else the focused row (as Get Info takes them).
fn chosen(view: &crate::view::View) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = view.selected_items().into_iter().map(|(path, _)| path).collect();
    if paths.is_empty() {
        paths.extend(view.focus().and_then(|i| view.entry_path(i)).map(|(path, _)| path));
    }
    paths
}

/// Keeps `paths` on this device (`keep`) or frees up their space: a job (progress, cancel);
/// each item's own state is read when its turn comes, not the badge's. No confirmation:
/// freeing up is undone by opening the file (it downloads again).
pub fn run(paths: Vec<PathBuf>, keep: bool, view: &crate::view::View) {
    if let Some(why) = refusal(&paths) {
        return view.note(why.to_owned());
    }
    crate::operations::with_current(|ops| {
        ops.submit(Box::new(gezik_ops::CloudPinTask::new(paths, keep)), None, crate::operations::After::Nothing);
    });
}

pub fn keep_selection(view: &crate::view::View) {
    run(chosen(view), true, view);
}

pub fn free_up_selection(view: &crate::view::View) {
    run(chosen(view), false, view);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(path: &str) -> CloudRoot {
        CloudRoot { path: PathBuf::from(path), label: "OneDrive".into(), account: String::new() }
    }

    #[test]
    fn the_answer_follows_new_roots() {
        let base = if cfg!(windows) { r"C:\u\OneDrive" } else { "/u/OneDrive" };
        let inside = Path::new(base).join("Docs");
        set_roots(Vec::new());
        assert!(!shows_state(&inside));
        set_roots(vec![root(base)]);
        assert_eq!(shows_state(&inside), cfg!(any(windows, target_os = "macos")), "Linux: no state");
        assert!(root_of(&inside).is_some());
        set_roots(Vec::new());
        assert!(!shows_state(&inside), "the cached answer went with the old roots");
    }

    #[test]
    fn commands_need_every_item_under_a_root() {
        let base = if cfg!(windows) { r"C:\u\OneDrive" } else { "/u/OneDrive" };
        set_roots(vec![root(base)]);
        let inside = Path::new(base).join("a.txt");
        let outside = PathBuf::from(if cfg!(windows) { r"C:\elsewhere\b.txt" } else { "/elsewhere/b.txt" });
        let cases = [refusal(std::slice::from_ref(&inside)), refusal(&[inside, outside]), refusal(&[])];
        if cfg!(any(windows, target_os = "macos")) {
            assert_eq!(cases, [None, Some(NOT_IN_CLOUD), Some(NOTHING_CHOSEN)]);
        } else {
            assert_eq!(cases, [Some(NO_COMMANDS_HERE); 3], "Linux: never");
        }
        set_roots(Vec::new());
    }
}
