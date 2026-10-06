//! Navigation model: locations, per-tab history and the set of tabs. Pure data — no I/O,
//! no UI — so every rule is unit-tested here and the app only wires it to Slint.

use std::path::{Component, Path, PathBuf};

/// The most back entries a tab keeps; older ones are dropped.
pub const MAX_BACK: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Location {
    Path(PathBuf),
    /// The virtual "This PC" list of drives.
    Drives,
}

impl Location {
    /// A folder's parent; a filesystem root's parent is `Drives`; `Drives` has none.
    pub fn parent(&self) -> Option<Location> {
        match self {
            Location::Drives => None,
            Location::Path(path) => Some(match path.parent() {
                Some(parent) => Location::Path(parent.to_path_buf()),
                None => Location::Drives,
            }),
        }
    }
}

/// What the user was looking at: restored when coming back via back/forward or a tab switch.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ViewState {
    /// Selected entries by name (not index: the folder may have changed meanwhile). Empty
    /// when more than 1000 were selected: only the focus is kept then.
    pub selected: Vec<String>,
    /// The entry with the keyboard focus, by name.
    pub focus: Option<String>,
    /// List scroll offset (Slint `content-y`, zero or negative).
    pub scroll: f32,
    /// The filter bar's text (`None`: closed). Kept by a tab switch and a reload; the
    /// navigator shows a move to another place without it.
    pub filter: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HistoryEntry {
    pub location: Location,
    pub view: ViewState,
}

/// One tab's back/forward history.
#[derive(Debug, Clone, PartialEq)]
pub struct History {
    back: Vec<HistoryEntry>,
    current: HistoryEntry,
    forward: Vec<HistoryEntry>,
}

impl History {
    pub fn new(location: Location) -> Self {
        Self { back: Vec::new(), current: HistoryEntry { location, view: ViewState::default() }, forward: Vec::new() }
    }
    pub fn location(&self) -> &Location {
        &self.current.location
    }
    pub fn view(&self) -> &ViewState {
        &self.current.view
    }
    pub fn set_view(&mut self, view: ViewState) {
        self.current.view = view;
    }
    pub fn can_go_back(&self) -> bool {
        !self.back.is_empty()
    }
    pub fn can_go_forward(&self) -> bool {
        !self.forward.is_empty()
    }
    /// Where `back()` would go, without going there (to load it first).
    pub fn back_target(&self) -> Option<&HistoryEntry> {
        self.back.last()
    }
    pub fn forward_target(&self) -> Option<&HistoryEntry> {
        self.forward.last()
    }
    /// Goes to `location`. Returns `false` (and changes nothing) if already there.
    pub fn navigate(&mut self, location: Location) -> bool {
        if location == self.current.location {
            return false;
        }
        let previous = std::mem::replace(&mut self.current, HistoryEntry { location, view: ViewState::default() });
        self.back.push(previous);
        if self.back.len() > MAX_BACK {
            self.back.remove(0);
        }
        self.forward.clear();
        true
    }
    pub fn back(&mut self) -> bool {
        let Some(target) = self.back.pop() else { return false };
        let previous = std::mem::replace(&mut self.current, target);
        self.forward.push(previous);
        true
    }
    pub fn forward(&mut self) -> bool {
        let Some(target) = self.forward.pop() else { return false };
        let previous = std::mem::replace(&mut self.current, target);
        self.back.push(previous);
        true
    }
    /// Applies `step`. Returns `false` if it changed nothing.
    pub fn step(&mut self, step: &Step) -> bool {
        match step {
            Step::Navigate(location) => self.navigate(location.clone()),
            Step::Back => self.back(),
            Step::Forward => self.forward(),
        }
    }
    /// Where `steps` lead from here, without going there (to load it first); `None` if a
    /// back or forward step has nowhere to go. A navigate to where the steps already are
    /// is fine: it just changes nothing.
    pub fn target_after(&self, steps: &[Step]) -> Option<Location> {
        let mut history = self.clone();
        for step in steps {
            if !history.step(step) && !matches!(step, Step::Navigate(_)) {
                return None;
            }
        }
        Some(history.current.location)
    }
    /// Applies `steps` in order: all the moves that were queued while their target loaded.
    pub fn apply_steps(&mut self, steps: &[Step]) {
        for step in steps {
            self.step(step);
        }
    }
}

/// One move in a tab's history. Moves made while an earlier one is still loading are
/// queued after it and applied together once the final target has loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Navigate(Location),
    Back,
    Forward,
}

/// How many closed tabs `Tabs::reopen` can bring back; the oldest are forgotten.
pub const MAX_CLOSED: usize = 20;

/// A closed tab: where it was and its whole history.
#[derive(Debug, Clone, PartialEq)]
pub struct ClosedTab {
    pub index: usize,
    pub history: History,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Closed {
    Remaining,
    /// The last tab was closed: the window should close.
    LastTab,
    /// The tab is locked: nothing closed.
    Locked,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tabs {
    tabs: Vec<History>,
    /// A stable id per tab, parallel to `tabs`: identifies a tab across moves and closes.
    ids: Vec<u64>,
    next_id: u64,
    active: usize,
    /// The ids of the locked tabs.
    locked: Vec<u64>,
    /// The closed tabs, the newest last.
    closed: Vec<ClosedTab>,
}

impl Tabs {
    pub fn new(location: Location) -> Self {
        Self {
            tabs: vec![History::new(location)],
            ids: vec![0],
            next_id: 1,
            active: 0,
            locked: Vec::new(),
            closed: Vec::new(),
        }
    }
    fn new_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
    /// The stable id of tab `index`.
    pub fn id(&self, index: usize) -> Option<u64> {
        self.ids.get(index).copied()
    }
    /// Where the tab with `id` is now; `None` once it is closed.
    pub fn index_of(&self, id: u64) -> Option<usize> {
        self.ids.iter().position(|&i| i == id)
    }
    pub fn len(&self) -> usize {
        self.tabs.len()
    }
    pub fn is_empty(&self) -> bool {
        false
    }
    pub fn active_index(&self) -> usize {
        self.active
    }
    pub fn active(&self) -> &History {
        &self.tabs[self.active]
    }
    pub fn active_mut(&mut self) -> &mut History {
        &mut self.tabs[self.active]
    }
    pub fn get(&self, index: usize) -> Option<&History> {
        self.tabs.get(index)
    }
    pub fn iter(&self) -> impl Iterator<Item = &History> {
        self.tabs.iter()
    }
    /// Opens a tab right after the active one. Returns its index.
    pub fn open(&mut self, location: Location, activate: bool) -> usize {
        let index = self.active + 1;
        self.tabs.insert(index, History::new(location));
        let id = self.new_id();
        self.ids.insert(index, id);
        if activate {
            self.active = index;
        }
        index
    }
    /// Closes tab `index` (out of range: no-op; locked: `Locked`, nothing closes). Closing the
    /// active tab activates its right neighbour, or the left one if it was last. The closed
    /// tab goes on the list `reopen` takes from.
    pub fn close(&mut self, index: usize) -> Closed {
        if index >= self.tabs.len() {
            return Closed::Remaining;
        }
        if self.is_locked(index) {
            return Closed::Locked;
        }
        if self.tabs.len() == 1 {
            return Closed::LastTab;
        }
        self.take_out(index);
        if index < self.active || (index == self.active && self.active == self.tabs.len()) {
            self.active -= 1;
        }
        Closed::Remaining
    }

    /// Closes every tab but `index` and the locked ones; `index` becomes active. Returns how
    /// many locked tabs stayed.
    pub fn close_others(&mut self, index: usize) -> usize {
        if index >= self.tabs.len() {
            return 0;
        }
        let keep = self.ids[index];
        let mut locked = 0;
        // From the right, so that each tab's index is still the one it had.
        for i in (0..self.tabs.len()).rev() {
            if i == index {
                continue;
            }
            if self.is_locked(i) {
                locked += 1;
            } else {
                self.take_out(i);
            }
        }
        self.active = self.index_of(keep).unwrap_or(0);
        locked
    }

    /// Takes tab `index` out onto the closed list (the oldest is forgotten past `MAX_CLOSED`).
    fn take_out(&mut self, index: usize) {
        let history = self.tabs.remove(index);
        self.ids.remove(index);
        self.closed.push(ClosedTab { index, history });
        if self.closed.len() > MAX_CLOSED {
            self.closed.remove(0);
        }
    }

    /// Opens the last closed tab again, with its history, where it was (at the end if there
    /// are fewer tabs now), as the active tab. Returns its index; `None` if none is left.
    pub fn reopen(&mut self) -> Option<usize> {
        let ClosedTab { index, history } = self.closed.pop()?;
        let index = index.min(self.tabs.len());
        self.tabs.insert(index, history);
        let id = self.new_id();
        self.ids.insert(index, id);
        self.active = index;
        Some(index)
    }

    /// How many closed tabs `reopen` can bring back.
    pub fn closed_count(&self) -> usize {
        self.closed.len()
    }

    pub fn is_locked(&self, index: usize) -> bool {
        self.id(index).is_some_and(|id| self.locked.contains(&id))
    }

    /// Locks or unlocks tab `index` (out of range: no-op). A locked tab does not close.
    pub fn set_locked(&mut self, index: usize, locked: bool) {
        let Some(id) = self.id(index) else { return };
        self.locked.retain(|&l| l != id);
        if locked {
            self.locked.push(id);
        }
    }
    /// Copies tab `index` (with its history) right after it. Returns the copy's index.
    pub fn duplicate(&mut self, index: usize) -> usize {
        let Some(copy) = self.tabs.get(index).cloned() else { return self.active };
        self.tabs.insert(index + 1, copy);
        let id = self.new_id();
        self.ids.insert(index + 1, id);
        if self.active > index {
            self.active += 1;
        }
        index + 1
    }
    /// Returns `true` if the active tab changed.
    pub fn activate(&mut self, index: usize) -> bool {
        if index >= self.tabs.len() || index == self.active {
            return false;
        }
        self.active = index;
        true
    }
    pub fn next(&mut self) {
        self.active = (self.active + 1) % self.tabs.len();
    }
    pub fn prev(&mut self) {
        self.active = (self.active + self.tabs.len() - 1) % self.tabs.len();
    }
    /// Moves tab `from` to position `to` (clamped); the active tab stays active.
    pub fn move_tab(&mut self, from: usize, to: usize) {
        if from >= self.tabs.len() {
            return;
        }
        let to = to.min(self.tabs.len() - 1);
        if from == to {
            return;
        }
        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);
        let id = self.ids.remove(from);
        self.ids.insert(to, id);
        self.active = if self.active == from {
            to
        } else if from < self.active && to >= self.active {
            self.active - 1
        } else if from > self.active && to <= self.active {
            self.active + 1
        } else {
            self.active
        };
    }
}

/// One clickable part of the address bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Crumb {
    pub label: String,
    pub location: Location,
}

/// The name of the list of drives (`Location::Drives`): Explorer's "This PC"; on macOS,
/// Finder's "Computer" (Go ▸ Computer lists the volumes too).
pub const DRIVES_NAME: &str = if cfg!(target_os = "macos") { "Computer" } else { "This PC" };

/// The address bar parts for `location`: always the drives (`DRIVES_NAME`) first, then the path from its
/// root. With more than `max_parts` path parts, the leading ones collapse into one "…"
/// part that goes to the first hidden folder's parent... (see tests).
pub fn crumbs(location: &Location, max_parts: usize) -> Vec<Crumb> {
    let mut out = vec![Crumb { label: DRIVES_NAME.to_owned(), location: Location::Drives }];
    let Location::Path(path) = location else { return out };

    let mut parts: Vec<Crumb> = Vec::new();
    let mut acc = PathBuf::new();
    for component in path.components() {
        acc.push(component.as_os_str());
        match component {
            Component::Prefix(prefix) => parts.push(Crumb {
                label: prefix.as_os_str().to_string_lossy().into_owned(),
                location: Location::Path(acc.clone()),
            }),
            Component::RootDir => match parts.last_mut() {
                // `C:` + `\` is one part: the drive root.
                Some(drive) => drive.location = Location::Path(acc.clone()),
                None => parts.push(Crumb { label: "/".to_owned(), location: Location::Path(acc.clone()) }),
            },
            Component::Normal(name) => {
                parts.push(Crumb { label: name.to_string_lossy().into_owned(), location: Location::Path(acc.clone()) })
            }
            Component::CurDir | Component::ParentDir => {}
        }
    }

    let max_parts = max_parts.max(1);
    if parts.len() > max_parts {
        let hidden = parts.len() - max_parts;
        let ellipsis_target = parts[hidden - 1].location.clone();
        parts.drain(..hidden);
        out.push(Crumb { label: "…".to_owned(), location: ellipsis_target });
    }
    out.extend(parts);
    out
}

/// `location` if it exists, otherwise its nearest existing ancestor, otherwise `Drives`.
pub fn nearest_existing(location: &Location, exists: impl Fn(&Path) -> bool) -> Location {
    let mut current = location.clone();
    loop {
        match &current {
            Location::Drives => return Location::Drives,
            Location::Path(path) if exists(path) => return current,
            Location::Path(_) => current = current.parent().unwrap_or(Location::Drives),
        }
    }
}

/// The text typed into the address bar with `~` and environment variables put in (spec 6.1):
/// `~` alone or followed by a separator is `home`; on Windows `%NAME%`, elsewhere `$NAME` and
/// `${NAME}`. A variable `var` does not know stays as typed, and so does `~name`.
pub fn expand_typed(text: &str, home: &Path, var: impl Fn(&str) -> Option<String>, windows: bool) -> String {
    let separators: &[char] = if windows { &['/', '\\'] } else { &['/'] };
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    if let Some(after) = text.strip_prefix('~')
        && (after.is_empty() || after.starts_with(separators))
    {
        out.push_str(&home.to_string_lossy());
        rest = after;
    }
    if windows {
        while let Some(start) = rest.find('%') {
            out.push_str(&rest[..start]);
            let after = &rest[start + 1..];
            let found = after.find('%').filter(|end| *end > 0).and_then(|end| Some((end, var(&after[..end])?)));
            match found {
                Some((end, value)) => {
                    out.push_str(&value);
                    rest = &after[end + 1..];
                }
                None => match after.find('%').filter(|end| *end > 0) {
                    // An unknown name stays as typed, closing % and all, so that % is not
                    // taken for the next variable's opening one.
                    Some(end) => {
                        out.push('%');
                        out.push_str(&after[..=end]);
                        rest = &after[end + 1..];
                    }
                    None => {
                        out.push('%');
                        rest = after;
                    }
                },
            }
        }
    } else {
        while let Some(start) = rest.find('$') {
            out.push_str(&rest[..start]);
            let after = &rest[start + 1..];
            let (name, used) = match after.strip_prefix('{') {
                Some(braced) => match braced.find('}') {
                    Some(end) => (&braced[..end], end + 2),
                    None => ("", 0),
                },
                None => {
                    let len = after.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).unwrap_or(after.len());
                    (&after[..len], len)
                }
            };
            let valid = !name.is_empty()
                && !name.starts_with(|c: char| c.is_ascii_digit())
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
            match valid.then(|| var(name)).flatten() {
                Some(value) => {
                    out.push_str(&value);
                    rest = &after[used..];
                }
                None => {
                    out.push('$');
                    rest = after;
                }
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(path: &str) -> Location {
        Location::Path(PathBuf::from(path))
    }

    fn view(name: &str, scroll: f32) -> ViewState {
        ViewState { selected: vec![name.to_owned()], focus: Some(name.to_owned()), scroll, filter: None }
    }

    // ---- Tab ids ----

    fn ids(tabs: &Tabs) -> Vec<u64> {
        (0..tabs.len()).filter_map(|i| tabs.id(i)).collect()
    }

    #[test]
    fn tab_ids_are_unique_and_follow_their_tab() {
        let mut tabs = Tabs::new(p("/a"));
        tabs.open(p("/b"), false);
        tabs.open(p("/c"), false);
        let before = ids(&tabs);
        let mut sorted = before.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 3);

        let moved = tabs.id(0).unwrap();
        tabs.move_tab(0, 2);
        assert_eq!(tabs.index_of(moved), Some(2));
        assert_eq!(tabs.get(2).unwrap().location(), &p("/a"));

        let original = tabs.id(1).unwrap();
        let copy = tabs.duplicate(1);
        let copy_id = tabs.id(copy).unwrap();
        assert!(!before.contains(&copy_id), "a duplicate gets a new id");
        assert_eq!(tabs.index_of(original), Some(1));

        let closed = tabs.id(0).unwrap();
        tabs.close(0);
        assert_eq!(tabs.index_of(closed), None);
        assert_eq!(tabs.index_of(original), Some(0));
        assert_eq!(tabs.id(tabs.len()), None);
    }

    #[test]
    fn closing_the_same_id_twice_closes_one_tab() {
        let mut tabs = Tabs::new(p("/a"));
        tabs.open(p("/b"), false);
        tabs.open(p("/c"), false);
        let id = tabs.id(1).unwrap();
        for _ in 0..2 {
            if let Some(index) = tabs.index_of(id) {
                tabs.close(index);
            }
        }
        assert_eq!(tabs.len(), 2);
        assert_eq!(tabs.index_of(id), None);
    }

    #[test]
    fn close_others_keeps_the_kept_tabs_id() {
        let mut tabs = Tabs::new(p("/a"));
        tabs.open(p("/b"), false);
        let id = tabs.id(1).unwrap();
        tabs.close_others(1);
        assert_eq!(ids(&tabs), [id]);
    }

    // ---- Location ----

    #[test]
    fn parent_of_folder_root_and_drives() {
        assert_eq!(p("/a/b").parent(), Some(p("/a")));
        assert_eq!(p("/").parent(), Some(Location::Drives));
        assert_eq!(Location::Drives.parent(), None);
    }

    #[cfg(windows)]
    #[test]
    fn parent_of_windows_drive_root_is_drives() {
        assert_eq!(p(r"C:\").parent(), Some(Location::Drives));
        assert_eq!(p(r"C:\Users").parent(), Some(p(r"C:\")));
    }

    // ---- History ----

    #[test]
    fn navigate_pushes_back_and_clears_forward() {
        let mut h = History::new(p("/a"));
        assert!(h.navigate(p("/b")));
        assert!(h.navigate(p("/c")));
        assert!(h.back());
        assert!(h.can_go_forward());
        assert!(h.navigate(p("/d")));
        assert!(!h.can_go_forward());
        assert_eq!(h.location(), &p("/d"));
        assert_eq!(h.back_target().unwrap().location, p("/b"));
    }

    #[test]
    fn navigating_to_the_current_location_changes_nothing() {
        let mut h = History::new(p("/a"));
        h.set_view(view("x", -40.0));
        assert!(!h.navigate(p("/a")));
        assert!(!h.can_go_back());
        assert_eq!(h.view(), &view("x", -40.0));
    }

    #[test]
    fn back_and_forward_restore_each_locations_view() {
        let mut h = History::new(p("/a"));
        h.set_view(view("in-a", -10.0));
        h.navigate(p("/b"));
        assert_eq!(h.view(), &ViewState::default());
        h.set_view(view("in-b", -20.0));

        assert!(h.back());
        assert_eq!(h.location(), &p("/a"));
        assert_eq!(h.view(), &view("in-a", -10.0));

        assert!(h.forward());
        assert_eq!(h.location(), &p("/b"));
        assert_eq!(h.view(), &view("in-b", -20.0));
    }

    #[test]
    fn back_and_forward_at_the_ends_do_nothing() {
        let mut h = History::new(p("/a"));
        assert!(!h.back());
        assert!(!h.forward());
        assert!(h.back_target().is_none() && h.forward_target().is_none());
        assert_eq!(h.location(), &p("/a"));
    }

    #[test]
    fn back_stack_is_capped() {
        let mut h = History::new(p("/0"));
        for i in 1..=(MAX_BACK + 5) {
            h.navigate(p(&format!("/{i}")));
        }
        let mut steps = 0;
        while h.back() {
            steps += 1;
        }
        assert_eq!(steps, MAX_BACK);
        assert_eq!(h.location(), &p("/5"));
    }

    // ---- Steps (moves queued while their target loads) ----

    fn deep_history() -> History {
        let mut h = History::new(p("/a"));
        for path in ["/a/b", "/a/b/c", "/a/b/c/d"] {
            h.navigate(p(path));
        }
        h
    }

    #[test]
    fn three_queued_backs_go_three_steps_back() {
        let mut h = deep_history();
        let steps = [Step::Back, Step::Back, Step::Back];
        assert_eq!(h.target_after(&steps[..1]), Some(p("/a/b/c")));
        assert_eq!(h.target_after(&steps), Some(p("/a")));
        assert_eq!(h.location(), &p("/a/b/c/d"), "computing a target changes nothing");
        h.apply_steps(&steps);
        assert_eq!(h.location(), &p("/a"));
        assert!(!h.can_go_back());
        assert_eq!(h.forward_target().unwrap().location, p("/a/b"));
    }

    #[test]
    fn a_step_past_the_end_has_no_target() {
        let h = deep_history();
        assert_eq!(h.target_after(&[Step::Back, Step::Back, Step::Back, Step::Back]), None);
        assert_eq!(h.target_after(&[Step::Forward]), None);
        assert_eq!(h.target_after(&[Step::Back, Step::Forward]), Some(p("/a/b/c/d")));
    }

    #[test]
    fn up_after_a_queued_navigate_goes_to_the_pending_targets_parent() {
        let mut h = History::new(p("/a"));
        let mut steps = vec![Step::Navigate(p("/x/y/z"))];
        let pending = h.target_after(&steps).unwrap();
        assert_eq!(pending, p("/x/y/z"));
        steps.push(Step::Navigate(pending.parent().unwrap()));
        assert_eq!(h.target_after(&steps), Some(p("/x/y")));
        h.apply_steps(&steps);
        assert_eq!(h.location(), &p("/x/y"));
        h.back();
        assert_eq!(h.location(), &p("/x/y/z"));
        h.back();
        assert_eq!(h.location(), &p("/a"));
    }

    #[test]
    fn steps_whose_load_failed_change_nothing() {
        // On failure the navigator never calls `apply_steps`: only `target_after` ran.
        let h = deep_history();
        let before = h.clone();
        let _ = h.target_after(&[Step::Back, Step::Back]);
        let _ = h.target_after(&[Step::Navigate(p("/elsewhere"))]);
        assert_eq!(h, before);
    }

    // ---- Tabs ----

    fn tabs_at(paths: &[&str]) -> Tabs {
        let mut tabs = Tabs::new(p(paths[0]));
        for (i, path) in paths.iter().enumerate().skip(1) {
            tabs.activate(i - 1);
            tabs.open(p(path), true);
        }
        tabs
    }

    fn locations(tabs: &Tabs) -> Vec<Location> {
        tabs.iter().map(|h| h.location().clone()).collect()
    }

    #[test]
    fn open_inserts_right_after_the_active_tab() {
        let mut tabs = tabs_at(&["/a", "/b"]);
        tabs.activate(0);
        let index = tabs.open(p("/new"), false);
        assert_eq!(index, 1);
        assert_eq!(locations(&tabs), [p("/a"), p("/new"), p("/b")]);
        assert_eq!(tabs.active_index(), 0);
        tabs.open(p("/x"), true);
        assert_eq!(tabs.active_index(), 1);
        assert_eq!(tabs.active().location(), &p("/x"));
    }

    #[test]
    fn close_active_activates_right_neighbour_then_left() {
        let mut tabs = tabs_at(&["/a", "/b", "/c"]);
        tabs.activate(1);
        assert_eq!(tabs.close(1), Closed::Remaining);
        assert_eq!(tabs.active().location(), &p("/c"));
        assert_eq!(tabs.close(1), Closed::Remaining);
        assert_eq!(tabs.active().location(), &p("/a"));
    }

    #[test]
    fn close_inactive_keeps_the_active_tab() {
        let mut tabs = tabs_at(&["/a", "/b", "/c"]);
        tabs.activate(2);
        tabs.close(0);
        assert_eq!(tabs.active().location(), &p("/c"));
        assert_eq!(tabs.active_index(), 1);
    }

    #[test]
    fn close_last_tab_reports_it_and_out_of_range_is_ignored() {
        let mut tabs = Tabs::new(p("/a"));
        assert_eq!(tabs.close(5), Closed::Remaining);
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs.close(0), Closed::LastTab);
        assert_eq!(tabs.len(), 1, "the model keeps one tab; the window closes");
    }

    #[test]
    fn close_others_keeps_only_that_tab() {
        let mut tabs = tabs_at(&["/a", "/b", "/c"]);
        tabs.close_others(1);
        assert_eq!(locations(&tabs), [p("/b")]);
        assert_eq!(tabs.active_index(), 0);
        tabs.close_others(0);
        assert_eq!(locations(&tabs), [p("/b")]);
        tabs.close_others(9);
        assert_eq!(locations(&tabs), [p("/b")]);
    }

    #[test]
    fn duplicate_copies_history_next_to_the_tab() {
        let mut tabs = Tabs::new(p("/a"));
        tabs.active_mut().navigate(p("/b"));
        let index = tabs.duplicate(0);
        assert_eq!(index, 1);
        assert_eq!(tabs.get(1).unwrap().location(), &p("/b"));
        assert!(tabs.get(1).unwrap().can_go_back());
        assert_eq!(tabs.active_index(), 0);
    }

    #[test]
    fn activate_next_prev_wrap_around() {
        let mut tabs = tabs_at(&["/a", "/b", "/c"]);
        assert!(tabs.activate(0));
        assert!(!tabs.activate(0));
        assert!(!tabs.activate(7));
        tabs.prev();
        assert_eq!(tabs.active_index(), 2);
        tabs.next();
        assert_eq!(tabs.active_index(), 0);
    }

    #[test]
    fn move_tab_keeps_the_active_tab_active() {
        let mut tabs = tabs_at(&["/a", "/b", "/c", "/d"]);
        tabs.activate(1); // /b
        tabs.move_tab(1, 3);
        assert_eq!(locations(&tabs), [p("/a"), p("/c"), p("/d"), p("/b")]);
        assert_eq!(tabs.active().location(), &p("/b"));
        tabs.move_tab(0, 2); // /a moves past the active /b? no: /b is at 3
        assert_eq!(locations(&tabs), [p("/c"), p("/d"), p("/a"), p("/b")]);
        assert_eq!(tabs.active().location(), &p("/b"));
        tabs.move_tab(3, 0);
        assert_eq!(locations(&tabs), [p("/b"), p("/c"), p("/d"), p("/a")]);
        assert_eq!(tabs.active().location(), &p("/b"));
    }

    #[test]
    fn move_tab_shifts_active_index_when_others_cross_it() {
        let mut tabs = tabs_at(&["/a", "/b", "/c"]);
        tabs.activate(1); // /b
        tabs.move_tab(0, 2); // /a jumps over /b
        assert_eq!(tabs.active().location(), &p("/b"));
        assert_eq!(tabs.active_index(), 0);
        tabs.move_tab(2, 0); // /a back in front
        assert_eq!(tabs.active().location(), &p("/b"));
        assert_eq!(tabs.active_index(), 1);
    }

    #[test]
    fn move_tab_to_same_place_or_out_of_range() {
        let mut tabs = tabs_at(&["/a", "/b"]);
        tabs.move_tab(1, 1);
        tabs.move_tab(5, 0);
        assert_eq!(locations(&tabs), [p("/a"), p("/b")]);
        tabs.move_tab(0, 99); // clamped to the end
        assert_eq!(locations(&tabs), [p("/b"), p("/a")]);
    }

    // ---- crumbs ----

    fn labels(crumbs: &[Crumb]) -> Vec<&str> {
        crumbs.iter().map(|c| c.label.as_str()).collect()
    }

    #[test]
    fn crumbs_for_drives_and_unix_paths() {
        assert_eq!(labels(&crumbs(&Location::Drives, 4)), [DRIVES_NAME]);
        let c = crumbs(&p("/home/a/docs"), 4);
        assert_eq!(labels(&c), [DRIVES_NAME, "/", "home", "a", "docs"]);
        assert_eq!(c[0].location, Location::Drives);
        assert_eq!(c[1].location, p("/"));
        assert_eq!(c[3].location, p("/home/a"));
    }

    #[test]
    fn long_paths_collapse_leading_parts() {
        let c = crumbs(&p("/a/b/c/d/e/f"), 4);
        assert_eq!(labels(&c), [DRIVES_NAME, "…", "c", "d", "e", "f"]);
        assert_eq!(c[1].location, p("/a/b"));
    }

    #[cfg(windows)]
    #[test]
    fn crumbs_for_windows_paths() {
        let c = crumbs(&p(r"C:\Users\teoma"), 4);
        assert_eq!(labels(&c), ["This PC", "C:", "Users", "teoma"]);
        assert_eq!(c[1].location, p(r"C:\"));
        assert_eq!(c[2].location, p(r"C:\Users"));
    }

    // ---- nearest_existing ----

    #[test]
    fn nearest_existing_ancestor_is_returned() {
        let exists = |path: &Path| path == Path::new("/a") || path == Path::new("/");
        assert_eq!(nearest_existing(&p("/a/b/c"), exists), p("/a"));
        assert_eq!(nearest_existing(&p("/a"), exists), p("/a"));
    }

    #[test]
    fn nearest_existing_ancestor_falls_back_to_drives() {
        assert_eq!(nearest_existing(&p("/gone/x"), |_| false), Location::Drives);
        assert_eq!(nearest_existing(&Location::Drives, |_| false), Location::Drives);
    }

    // ---- Closed tabs, locks, filter ----

    #[test]
    fn a_closed_tab_comes_back_where_it_was_with_its_history() {
        let mut tabs = tabs_at(&["/a", "/b", "/c"]);
        tabs.activate(1);
        tabs.active_mut().navigate(p("/b/deeper"));
        tabs.active_mut().set_view(ViewState { filter: Some("*.jpg".into()), ..view("x", -30.0) });
        assert_eq!(tabs.close(1), Closed::Remaining);
        assert_eq!(locations(&tabs), [p("/a"), p("/c")]);
        assert_eq!(tabs.closed_count(), 1);
        assert_eq!(tabs.reopen(), Some(1));
        assert_eq!(locations(&tabs), [p("/a"), p("/b/deeper"), p("/c")]);
        assert_eq!(tabs.active_index(), 1, "the reopened tab is active");
        assert_eq!(tabs.active().view().filter.as_deref(), Some("*.jpg"));
        assert!(tabs.active_mut().back());
        assert_eq!(tabs.active().location(), &p("/b"));
        assert_eq!(tabs.reopen(), None, "nothing more to reopen");
    }

    #[test]
    fn the_closed_list_keeps_the_last_twenty() {
        let mut tabs = Tabs::new(p("/0"));
        for i in 1..=25 {
            tabs.open(p(&format!("/{i}")), true);
        }
        for _ in 0..25 {
            assert_eq!(tabs.close(1), Closed::Remaining);
        }
        assert_eq!(tabs.closed_count(), MAX_CLOSED);
        assert_eq!(tabs.reopen(), Some(1));
        assert_eq!(tabs.active().location(), &p("/25"), "the newest first");
        for _ in 1..MAX_CLOSED {
            assert!(tabs.reopen().is_some());
        }
        assert_eq!(tabs.reopen(), None, "/1-/5 were forgotten");
        assert_eq!(tabs.len(), 21);
    }

    #[test]
    fn a_locked_tab_stays_open() {
        let mut tabs = tabs_at(&["/a", "/b", "/c", "/d"]);
        tabs.set_locked(1, true);
        assert!(tabs.is_locked(1) && !tabs.is_locked(0) && !tabs.is_locked(99));
        assert_eq!(tabs.close(1), Closed::Locked);
        assert_eq!((tabs.len(), tabs.closed_count()), (4, 0));
        assert_eq!(tabs.close_others(2), 1, "one locked tab stayed");
        assert_eq!(locations(&tabs), [p("/b"), p("/c")]);
        assert_eq!(tabs.active().location(), &p("/c"), "the kept tab is active");
        // The last tab, locked, does not close the window either.
        tabs.close(1);
        assert_eq!(tabs.close(0), Closed::Locked);
        tabs.set_locked(0, false);
        assert_eq!(tabs.close(0), Closed::LastTab);
    }

    #[test]
    fn tabs_closed_together_come_back_in_their_places() {
        let mut tabs = tabs_at(&["/a", "/b", "/c", "/d", "/e"]);
        tabs.set_locked(3, true);
        assert_eq!(tabs.close_others(1), 1);
        assert_eq!(locations(&tabs), [p("/b"), p("/d")]);
        assert_eq!(tabs.closed_count(), 3, "each closed tab is on the list");
        while tabs.reopen().is_some() {}
        assert_eq!(locations(&tabs), [p("/a"), p("/b"), p("/c"), p("/d"), p("/e")]);
    }

    #[test]
    fn the_lock_follows_its_tab_and_a_copy_is_unlocked() {
        let mut tabs = tabs_at(&["/a", "/b", "/c"]);
        tabs.set_locked(0, true);
        tabs.move_tab(0, 2);
        assert!(tabs.is_locked(2) && !tabs.is_locked(0));
        let copy = tabs.duplicate(2);
        assert!(!tabs.is_locked(copy));
        tabs.set_locked(2, false);
        assert!(!tabs.is_locked(2));
    }

    #[test]
    fn a_reopened_tab_whose_place_is_gone_goes_last() {
        let mut tabs = tabs_at(&["/a", "/b"]);
        tabs.closed.push(ClosedTab { index: 9, history: History::new(p("/z")) });
        assert_eq!(tabs.reopen(), Some(2));
        assert_eq!(locations(&tabs), [p("/a"), p("/b"), p("/z")]);
        let new_id = tabs.id(2).unwrap();
        assert!(tabs.id(0) != Some(new_id) && tabs.id(1) != Some(new_id), "a new id");
    }

    #[test]
    fn each_place_keeps_its_filter_in_the_history() {
        let mut h = History::new(p("/a"));
        h.set_view(ViewState { filter: Some("jpg".into()), ..ViewState::default() });
        h.navigate(p("/b"));
        assert_eq!(h.view().filter, None, "a new place starts without one");
        h.back();
        assert_eq!(h.view().filter.as_deref(), Some("jpg"), "kept; the navigator decides whether to show it");
    }

    #[test]
    fn a_tilde_at_the_start_is_home() {
        let home = Path::new("/home/ali");
        let none = |_: &str| None;
        assert_eq!(expand_typed("~", home, none, false), "/home/ali");
        assert_eq!(expand_typed("~/Projeler", home, none, false), "/home/ali/Projeler");
        assert_eq!(expand_typed("~veli/x", home, none, false), "~veli/x", "another user's ~ stays");
        assert_eq!(expand_typed("/srv/~/x", home, none, false), "/srv/~/x", "only at the start");
        assert_eq!(expand_typed(r"~\Belgeler", Path::new(r"C:\Users\ali"), none, true), r"C:\Users\ali\Belgeler");
        assert_eq!(expand_typed(r"~\x", home, none, false), r"~\x", "a backslash is no separator on Unix");
    }

    #[test]
    fn windows_variables_are_between_percent_signs() {
        let var = |name: &str| match name {
            "USERPROFILE" => Some(r"C:\Users\ali".to_owned()),
            "A" => Some("1".to_owned()),
            _ => None,
        };
        let home = Path::new(r"C:\Users\ali");
        assert_eq!(expand_typed(r"%USERPROFILE%\Desktop", home, var, true), r"C:\Users\ali\Desktop");
        assert_eq!(expand_typed("%A%%A%", home, var, true), "11");
        assert_eq!(expand_typed(r"%NOPE%\x", home, var, true), r"%NOPE%\x");
        assert_eq!(expand_typed("100%", home, var, true), "100%");
        assert_eq!(expand_typed("%%", home, var, true), "%%");
        assert_eq!(expand_typed("%NOPE%PATH%", home, var, true), "%NOPE%PATH%");
        assert_eq!(expand_typed("%NOPE%A%", home, var, true), "%NOPE%A%", "its closing % is not an opening one");
        assert_eq!(expand_typed("%NOPE%%A%", home, var, true), "%NOPE%1");
        assert_eq!(expand_typed("$A", home, var, true), "$A", "no $ on Windows");
    }

    #[test]
    fn unix_variables_start_with_a_dollar() {
        let var = |name: &str| match name {
            "HOME" => Some("/home/ali".to_owned()),
            "X_1" => Some("x".to_owned()),
            _ => None,
        };
        let home = Path::new("/home/ali");
        assert_eq!(expand_typed("$HOME/Belgeler", home, var, false), "/home/ali/Belgeler");
        assert_eq!(expand_typed("${HOME}x", home, var, false), "/home/alix");
        assert_eq!(expand_typed("/a/$X_1-b", home, var, false), "/a/x-b");
        assert_eq!(expand_typed("$NOPE/x ${NOPE}", home, var, false), "$NOPE/x ${NOPE}");
        assert_eq!(expand_typed("a$ $1 ${HOME", home, var, false), "a$ $1 ${HOME");
        assert_eq!(expand_typed("%HOME%", home, var, false), "%HOME%", "no % on Unix");
    }
}
