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

#[allow(dead_code)] // shortcut: Tasks 4-5 (badges, keep / free up) call it; drop the allow then.
pub fn roots() -> Vec<CloudRoot> {
    STATE.with(|s| s.borrow().roots.clone())
}

#[allow(dead_code)] // shortcut: Tasks 4-5 (badges, keep / free up) call it; drop the allow then.
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
}
