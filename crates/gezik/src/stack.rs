//! The drop stack (spec 9.3): paths gathered from anywhere in a strip above the status bar,
//! then copied or moved together into the folder shown. Kept for the session only, at most
//! `STACK_MAX`; whether each still exists is checked off the UI thread.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;

use gezik_core::drag::Effect;
use gezik_core::kind::Kind;
use gezik_core::ops::paths::{path_key, same_path};
use gezik_ops::{JobId, Report};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::operations::Operations;
use crate::{AppWindow, StackRow};

/// The most paths the stack keeps.
pub const STACK_MAX: usize = 1000;
/// The most items the strip draws (all of them are copied or moved).
pub const STACK_SHOWN: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackItem {
    pub path: PathBuf,
    pub is_dir: bool,
    /// Not there when last checked: drawn faded, left out.
    pub gone: bool,
}

/// The stack's paths, in the order added.
#[derive(Debug, Default)]
pub struct StackItems {
    items: Vec<StackItem>,
    /// `path_key` of every item, for adding without comparing with each one.
    keys: HashSet<Vec<String>>,
}

impl StackItems {
    /// Adds the paths not there yet, while there is room: (added, left out for room).
    pub fn add(&mut self, paths: impl IntoIterator<Item = (PathBuf, bool)>) -> (usize, usize) {
        let (mut added, mut full) = (0, 0);
        for (path, is_dir) in paths {
            let key = path_key(&path);
            if self.keys.contains(&key) {
                continue;
            }
            if self.items.len() >= STACK_MAX {
                full += 1;
                continue;
            }
            self.keys.insert(key);
            self.items.push(StackItem { path, is_dir, gone: false });
            added += 1;
        }
        (added, full)
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.items.len() {
            let item = self.items.remove(index);
            self.keys.remove(&path_key(&item.path));
        }
    }

    pub fn clear(&mut self) {
        self.items = Vec::new();
        self.keys = HashSet::new();
    }

    pub fn items(&self) -> &[StackItem] {
        &self.items
    }

    /// The paths still there, for Copy here / Move here.
    pub fn usable(&self) -> Vec<PathBuf> {
        self.items.iter().filter(|item| !item.gone).map(|item| item.path.clone()).collect()
    }

    /// What a check found: per path, whether it is a folder, or `None` if it is gone. The
    /// results come in the order of the items asked about, so they are walked side by side;
    /// only when the stack changed meanwhile are they looked up by key.
    pub fn checked(&mut self, found: &[(PathBuf, Option<bool>)]) {
        let mut by_key: Option<HashMap<Vec<String>, Option<bool>>> = None;
        for (i, item) in self.items.iter_mut().enumerate() {
            let state = match found.get(i) {
                Some((path, state)) if *path == item.path => Some(*state),
                _ => by_key
                    .get_or_insert_with(|| found.iter().map(|(path, state)| (path_key(path), *state)).collect())
                    .get(&path_key(&item.path))
                    .copied(),
            };
            if let Some(state) = state {
                item.gone = state.is_none();
                if let Some(is_dir) = state {
                    item.is_dir = is_dir;
                }
            }
        }
    }

    /// A Move here ended and `found` is what the disk says of the paths it was to move: the
    /// ones no longer there left the stack; the rest (skipped, cancelled, failed, or already
    /// in the folder) stay.
    pub fn left(&mut self, found: &[(PathBuf, Option<bool>)]) {
        let gone: HashSet<Vec<String>> =
            found.iter().filter(|(_, state)| state.is_none()).map(|(path, _)| path_key(path)).collect();
        let keys = &mut self.keys;
        self.items.retain(|item| {
            let key = path_key(&item.path);
            let went = gone.contains(&key);
            if went {
                keys.remove(&key);
            }
            !went
        });
    }
}

/// "12 items".
pub fn count_text(n: usize) -> String {
    if n == 1 { "1 item".to_owned() } else { format!("{n} items") }
}

/// "+900 more" past what the strip draws; empty otherwise.
pub fn more_text(n: usize) -> String {
    if n > STACK_SHOWN { format!("+{} more", n - STACK_SHOWN) } else { String::new() }
}

/// Per path: whether it is a folder, `None` if it is not there. Reads the disk.
fn check(paths: Vec<PathBuf>) -> Vec<(PathBuf, Option<bool>)> {
    paths
        .into_iter()
        .map(|path| {
            let state = std::fs::metadata(&path)
                .map(|meta| meta.is_dir())
                .ok()
                .or_else(|| std::fs::symlink_metadata(&path).ok().map(|_| false));
            (path, state)
        })
        .collect()
}

thread_local! {
    static CURRENT: RefCell<Option<Stack>> = const { RefCell::new(None) };
}

/// Runs `f` with this UI thread's stack, if set up.
pub fn with_current(f: impl FnOnce(&Stack)) {
    if let Some(stack) = CURRENT.with(|c| c.borrow().clone()) {
        f(&stack);
    }
}

struct Inner {
    window: slint::Weak<AppWindow>,
    ops: Operations,
    items: RefCell<StackItems>,
    open: Cell<bool>,
    rows: Rc<VecModel<StackRow>>,
    /// The Move here jobs under way: each job and what it moves.
    moving: RefCell<Vec<(JobId, Vec<PathBuf>)>>,
    /// Counts the checks started; the result of an older one is stale.
    generation: Cell<u64>,
}

#[derive(Clone)]
pub struct Stack(Rc<Inner>);

impl Stack {
    pub fn new(window: &AppWindow, ops: Operations) -> Stack {
        let rows = Rc::new(VecModel::default());
        window.set_stack_rows(ModelRc::from(rows.clone()));
        let stack = Stack(Rc::new(Inner {
            window: window.as_weak(),
            ops,
            items: RefCell::default(),
            open: Cell::new(false),
            rows,
            moving: RefCell::default(),
            generation: Cell::new(0),
        }));
        window.on_stack_down(|i, x, y| with_current(|stack| stack.down(i, x, y)));
        window.on_stack_hovered(|i| with_current(|stack| stack.hovered(i)));
        window.on_stack_remove(|i| {
            with_current(|stack| {
                if let Ok(i) = usize::try_from(i) {
                    stack.0.items.borrow_mut().remove(i);
                    stack.sync();
                }
            })
        });
        window.on_stack_copy(|| with_current(|stack| stack.send(Effect::Copy)));
        window.on_stack_move(|| with_current(|stack| stack.send(Effect::Move)));
        window.on_stack_clear(|| {
            with_current(|stack| {
                stack.0.items.borrow_mut().clear();
                stack.sync();
            })
        });
        CURRENT.with(|c| *c.borrow_mut() = Some(stack.clone()));
        stack
    }

    pub fn is_open(&self) -> bool {
        self.0.open.get()
    }

    /// Adds `items` (path, is a folder as far as known); the strip opens. Says so when the
    /// stack is full.
    pub fn add(&self, items: Vec<(PathBuf, bool)>) {
        let (added, full) = self.0.items.borrow_mut().add(items);
        if full > 0 {
            crate::panes::active_view().note(format!("The drop stack holds at most {STACK_MAX} items"));
        }
        if added > 0 {
            self.0.open.set(true);
            self.check();
        } else if full == 0 {
            // All of it was on the stack already: show where.
            self.0.open.set(true);
            crate::panes::active_view().note("Already on the drop stack".to_owned());
        }
        self.sync();
    }

    /// `add-to-stack`: the selected items.
    pub fn add_selection(&self) {
        if crate::panes::active_view().shows_drives() {
            return crate::panes::active_view().note("Open a folder to add items to the drop stack".to_owned());
        }
        let items = crate::panes::active_view().selected_items();
        if items.is_empty() {
            return crate::panes::active_view().note("Select the items to add to the drop stack".to_owned());
        }
        self.add(items);
    }

    /// `toggle-stack`, View ▸ Drop stack.
    pub fn toggle(&self) {
        self.0.open.set(!self.0.open.get());
        if self.0.open.get() {
            self.check();
        }
        self.sync();
    }

    /// A job ended: a Move here takes what it moved off the stack; what is there may have
    /// changed, so it is checked again.
    pub fn job_finished(&self, id: JobId, _report: &Report) {
        let moved = {
            let mut moving = self.0.moving.borrow_mut();
            moving.iter().position(|(job, _)| *job == id).map(|at| moving.remove(at).1)
        };
        match moved {
            // What left is decided by the disk: a skipped, cancelled or failed item is still
            // there and stays.
            Some(paths) => {
                let _ = std::thread::Builder::new().name("gezik-stack-check".into()).spawn(move || {
                    let found = check(paths);
                    let _ = slint::invoke_from_event_loop(move || {
                        with_current(|stack| {
                            stack.0.items.borrow_mut().left(&found);
                            stack.sync();
                            stack.check();
                        })
                    });
                });
            }
            None => {
                if !self.0.items.borrow().items().is_empty() {
                    self.check();
                }
                self.sync();
            }
        }
    }

    /// Copy here / Move here: the paths still there, to the folder shown, as one job.
    fn send(&self, effect: Effect) {
        let Some(dir) = crate::panes::active_view().folder() else {
            return crate::panes::active_view().note("Open a folder to copy or move the drop stack into".to_owned());
        };
        let usable = self.0.items.borrow().usable();
        if usable.is_empty() {
            return crate::panes::active_view().note("The drop stack has nothing to copy or move".to_owned());
        }
        // Items already in this folder have nothing to do here.
        let paths: Vec<PathBuf> =
            usable.into_iter().filter(|path| !path.parent().is_some_and(|parent| same_path(parent, &dir))).collect();
        if paths.is_empty() {
            return crate::panes::active_view()
                .note("Everything on the drop stack is already in this folder".to_owned());
        }
        let job = self.0.ops.transfer_job(paths.clone(), dir, effect);
        if effect == Effect::Move {
            self.0.moving.borrow_mut().push((job, paths));
        }
    }

    /// A press on item `index` (-1: all of them): Gezik's drag starts once it moves.
    fn down(&self, index: i32, x: f32, y: f32) {
        let items: Vec<(PathBuf, bool)> = {
            let stack = self.0.items.borrow();
            let pick = |item: &StackItem| (!item.gone).then(|| (item.path.clone(), item.is_dir));
            match usize::try_from(index) {
                Ok(i) => stack.items().get(i).and_then(pick).into_iter().collect(),
                Err(_) => stack.items().iter().filter_map(pick).collect(),
            }
        };
        if !items.is_empty() {
            crate::drag::with_current(|drags| drags.stack_down(items, x, y));
        }
    }

    /// The pointer rests on item `index`: its full path in the status bar.
    fn hovered(&self, index: i32) {
        let path = usize::try_from(index)
            .ok()
            .and_then(|i| self.0.items.borrow().items().get(i).map(|item| item.path.clone()));
        match path {
            Some(path) => crate::panes::active_view().note(path.display().to_string()),
            None => crate::panes::active_view().clear_note(),
        }
    }

    /// Checks on a thread of its own which paths are still there.
    fn check(&self) {
        let generation = self.0.generation.get() + 1;
        self.0.generation.set(generation);
        let paths: Vec<PathBuf> = self.0.items.borrow().items().iter().map(|item| item.path.clone()).collect();
        let _ = std::thread::Builder::new().name("gezik-stack-check".into()).spawn(move || {
            let found = check(paths);
            let _ = slint::invoke_from_event_loop(move || {
                with_current(|stack| {
                    if stack.0.generation.get() == generation {
                        stack.0.items.borrow_mut().checked(&found);
                        stack.sync();
                    }
                })
            });
        });
    }

    /// The strip as the stack is now.
    fn sync(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        let stack = self.0.items.borrow();
        let rows = stack.items().iter().take(STACK_SHOWN).map(|item| {
            let name = item
                .path
                .file_name()
                .map_or_else(|| item.path.display().to_string(), |n| n.to_string_lossy().into_owned());
            StackRow { kind: Kind::of(&name, item.is_dir).index(), name: name.into(), gone: item.gone }
        });
        crate::navigation::sync_model(&self.0.rows, rows);
        window.set_stack_count(count_text(stack.items().len()).into());
        window.set_stack_more(more_text(stack.items().len()).into());
        window.set_stack_open(self.0.open.get());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(name: &str) -> PathBuf {
        PathBuf::from(if cfg!(windows) { format!(r"C:\s\{name}") } else { format!("/s/{name}") })
    }

    #[test]
    fn the_stack_keeps_each_path_once_and_at_most_a_thousand() {
        let mut s = StackItems::default();
        assert_eq!(s.add([(p("a"), false), (p("b"), true), (p("a"), false)]), (2, 0));
        if cfg!(windows) {
            assert_eq!(s.add([(p("A"), false)]), (0, 0), "the case is ignored as Windows does");
        }
        let many = (0..1100).map(|i| (p(&format!("f{i}")), false));
        assert_eq!(s.add(many), (STACK_MAX - 2, 1100 - (STACK_MAX - 2)));
        assert_eq!(s.items().len(), STACK_MAX);
        s.remove(0);
        assert_eq!(s.items()[0].path, p("b"));
        s.remove(5000);
        s.clear();
        assert!(s.items().is_empty());
    }

    #[test]
    fn gone_items_are_left_out_and_unmoved_items_stay() {
        let mut s = StackItems::default();
        s.add([(p("a"), false), (p("b"), false), (p("c"), false)]);
        s.checked(&[(p("b"), None), (p("a"), Some(false)), (p("c"), Some(true))]);
        assert!(s.items()[1].gone);
        assert!(s.items()[2].is_dir, "the check tells folders apart");
        assert_eq!(s.usable(), [p("a"), p("c")]);
        // The disk decides after a Move here: a was moved away; c was skipped (or cancelled,
        // or already in the folder) and is still there.
        s.left(&[(p("a"), None), (p("c"), Some(true))]);
        let left: Vec<PathBuf> = s.items().iter().map(|i| i.path.clone()).collect();
        assert_eq!(left, [p("b"), p("c")], "c is still there: it stays; a went");
        assert_eq!(s.add([(p("a"), false)]), (1, 0), "a left the key set too");
        s.checked(&[(p("b"), Some(false))]);
        assert!(!s.items()[0].gone, "back again");
    }

    #[test]
    fn a_check_that_comes_in_another_order_is_matched_by_path() {
        let mut s = StackItems::default();
        s.add([(p("a"), false), (p("b"), false)]);
        s.remove(0);
        s.checked(&[(p("a"), None), (p("b"), None)]);
        assert_eq!(s.items().len(), 1);
        assert!(s.items()[0].gone, "b is found though the stack changed under the check");
        s.left(&[]);
        assert_eq!(s.items().len(), 1, "nothing checked: nothing leaves");
    }

    #[test]
    fn the_strip_says_how_many_and_how_many_more() {
        assert_eq!(count_text(1), "1 item");
        assert_eq!(count_text(12), "12 items");
        assert_eq!(more_text(STACK_SHOWN), "");
        assert_eq!(more_text(STACK_SHOWN + 900), "+900 more");
    }
}
