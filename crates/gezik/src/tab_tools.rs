//! The tab picker (Ctrl+Shift+A): a box over the window lists the open tabs; typing filters
//! them by title or path (the filter's pattern language), Enter or a click switches to one.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gezik_config::shortcuts::{Chord, Key};
use gezik_core::pattern::Pattern;
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::navigation::Navigator;
use crate::{AppWindow, TabPickRow};

thread_local! {
    /// The tab picker of this (UI) thread, for the actions and the keys.
    static CURRENT: RefCell<Option<TabTools>> = const { RefCell::new(None) };
}

/// Runs `f` with this UI thread's tab picker, if there is one yet.
pub fn with_current(f: impl FnOnce(&TabTools)) {
    if let Some(tools) = CURRENT.with(|c| c.borrow().clone()) {
        f(&tools);
    }
}

/// The tabs whose title or path the query lets through (all for an empty or unfinished one):
/// an including part may match either, a leaving-out part must match neither (`!downloads`
/// hides the tab whose path has it, whatever its title). The pattern language takes names,
/// which have no `/`: a path is typed with either slash, and both sides are matched with `\`
/// for it.
pub fn picker_rows(tabs: &[(String, String)], query: &str) -> Vec<usize> {
    let all = || (0..tabs.len()).collect();
    let Ok(pattern) = Pattern::compile(&query.replace('/', "\\")) else { return all() };
    if pattern.is_empty() {
        return all();
    }
    tabs.iter()
        .enumerate()
        .filter(|(_, (title, path))| pattern.matches_any(&[title, &path.replace('/', "\\")]))
        .map(|(i, _)| i)
        .collect()
}

/// Whether a question, the conflict list or another layer is open over the window.
fn over_another_layer(window: &AppWindow) -> bool {
    window.get_dialog_open()
        || window.get_conflicts_open()
        || window.get_rb_open()
        || window.get_cp_open()
        || window.get_cv_open()
}

/// The row Up/Down moves to, within `len` rows.
pub fn step(current: usize, len: usize, down: bool) -> usize {
    if len == 0 {
        0
    } else if down {
        (current + 1).min(len - 1)
    } else {
        current.saturating_sub(1).min(len - 1)
    }
}

struct Inner {
    window: slint::Weak<AppWindow>,
    nav: Navigator,
    /// The tabs as the picker opened (title, path); they cannot change while it is open.
    tabs: RefCell<Vec<(String, String)>>,
    /// The tabs shown, as indexes into `tabs`.
    rows: RefCell<Vec<usize>>,
    /// The shown row Enter picks.
    current: Cell<usize>,
    model: Rc<VecModel<TabPickRow>>,
}

#[derive(Clone)]
pub struct TabTools(Rc<Inner>);

impl TabTools {
    pub fn new(window: &AppWindow, nav: Navigator) -> TabTools {
        let model = Rc::new(VecModel::default());
        window.set_tp_rows(ModelRc::from(model.clone()));
        let tools = TabTools(Rc::new(Inner {
            window: window.as_weak(),
            nav,
            tabs: RefCell::default(),
            rows: RefCell::default(),
            current: Cell::new(0),
            model,
        }));
        window.on_tp_edited(|query| with_current(|t| t.edited(&query)));
        window.on_tp_chosen(|row| {
            with_current(|t| {
                if let Ok(row) = usize::try_from(row) {
                    t.0.current.set(row);
                    t.choose();
                }
            });
        });
        CURRENT.with(|c| *c.borrow_mut() = Some(tools.clone()));
        tools
    }

    /// Opens the picker on every tab, the active one current, the field with the keyboard.
    /// Not over a question or another layer (the macOS menu bar can still ask for it): the
    /// keys would go to the layer under it.
    pub fn open(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        if over_another_layer(&window) {
            return;
        }
        *self.0.tabs.borrow_mut() = self.0.nav.tab_list();
        let all = picker_rows(&self.0.tabs.borrow(), "");
        *self.0.rows.borrow_mut() = all;
        self.0.current.set(self.0.nav.active_index());
        window.set_tp_query("".into());
        self.show(&window);
        window.set_tp_open(true);
        window.invoke_focus_list();
    }

    /// A key while the picker is open: Esc closes, Enter switches, Up/Down move; else the field's.
    pub fn chord(&self, chord: &Chord) -> bool {
        if chord.ctrl || chord.alt || chord.meta || chord.shift {
            return false;
        }
        match chord.key {
            Key::Escape => self.close(),
            Key::Enter => self.choose(),
            Key::Up | Key::Down => {
                let len = self.0.rows.borrow().len();
                self.0.current.set(step(self.0.current.get(), len, chord.key == Key::Down));
                if let Some(window) = self.0.window.upgrade() {
                    window.set_tp_current(self.current_row());
                }
            }
            _ => return false,
        }
        true
    }

    /// The query changed: the rows it lets through; the current tab stays current if shown.
    fn edited(&self, query: &str) {
        let Some(window) = self.0.window.upgrade() else { return };
        let was = self.0.rows.borrow().get(self.0.current.get()).copied();
        let rows = picker_rows(&self.0.tabs.borrow(), query);
        let current = was.and_then(|tab| rows.iter().position(|&r| r == tab)).unwrap_or(0);
        *self.0.rows.borrow_mut() = rows;
        self.0.current.set(current);
        self.show(&window);
    }

    /// Switches to the current row's tab and closes; with no rows shown, the picker stays
    /// open for the query to be changed.
    fn choose(&self) {
        let Some(tab) = self.0.rows.borrow().get(self.0.current.get()).copied() else { return };
        self.close();
        self.0.nav.activate_tab(tab);
    }

    fn close(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        window.set_tp_open(false);
        self.0.model.set_vec(Vec::new());
        if !window.get_dialog_open() && !window.get_conflicts_open() {
            window.invoke_focus_list();
        }
    }

    fn current_row(&self) -> i32 {
        i32::try_from(self.0.current.get()).unwrap_or(0)
    }

    /// Puts the shown rows and the current one on screen.
    fn show(&self, window: &AppWindow) {
        let active = self.0.nav.active_index();
        let tabs = self.0.tabs.borrow();
        let rows: Vec<TabPickRow> = self
            .0
            .rows
            .borrow()
            .iter()
            .map(|&i| {
                let (title, path) = &tabs[i];
                TabPickRow { title: title.into(), path: path.into(), active: i == active }
            })
            .collect();
        self.0.model.set_vec(rows);
        window.set_tp_current(self.current_row());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tabs() -> Vec<(String, String)> {
        [("Documents", "C:\\Users\\a\\Documents"), ("İndirilenler", "C:\\Users\\a\\Downloads"), ("This PC", "")]
            .map(|(t, p)| (t.to_owned(), p.to_owned()))
            .into()
    }

    #[test]
    fn the_picker_matches_titles_and_paths() {
        assert_eq!(picker_rows(&tabs(), ""), [0, 1, 2]);
        assert_eq!(picker_rows(&tabs(), "doc"), [0]);
        assert_eq!(picker_rows(&tabs(), "downloads"), [1], "by path");
        assert_eq!(picker_rows(&tabs(), "indir"), [1], "Turkish İ");
        assert_eq!(picker_rows(&tabs(), "zzz"), Vec::<usize>::new());
        assert_eq!(picker_rows(&tabs(), "!"), [0, 1, 2], "an unfinished pattern shows them all");
    }

    #[test]
    fn an_exclusion_holds_for_both_title_and_path() {
        assert_eq!(picker_rows(&tabs(), "!downloads"), [0, 2], "by path, though the title has no 'downloads'");
        assert_eq!(picker_rows(&tabs(), "doc;!Users"), Vec::<usize>::new(), "Documents is under Users");
        assert_eq!(picker_rows(&tabs(), "!this"), [0, 1], "by title");
        assert_eq!(picker_rows(&tabs(), "a/d;!indir"), [0], "includes still match title or path");
    }

    #[test]
    fn the_picker_takes_a_path_typed_with_slashes() {
        let tabs: Vec<(String, String)> = [("one", "/tmp/tb/one"), ("two", "/tmp/tb/two"), ("Users", r"C:\Users")]
            .map(|(t, p)| (t.to_owned(), p.to_owned()))
            .into();
        assert_eq!(picker_rows(&tabs, "/tmp/tb/one"), [0]);
        assert_eq!(picker_rows(&tabs, "tb/t"), [1]);
        assert_eq!(picker_rows(&tabs, "c:/users"), [2], "either slash for Windows paths");
    }

    #[test]
    fn up_and_down_stay_within_the_rows() {
        assert_eq!(step(0, 3, true), 1);
        assert_eq!(step(2, 3, true), 2);
        assert_eq!(step(0, 3, false), 0);
        assert_eq!(step(0, 0, true), 0);
    }
}
