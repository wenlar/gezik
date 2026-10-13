//! The command palette (Ctrl+Shift+P, `>`) and quick open (Ctrl+P), spec 7: the tab picker's box
//! (`widgets/picker.slint`) lists actions, `[[commands]]`, view options and places; the order is
//! `gezik_core::palette`'s. The items are built when it opens and dropped when it closes.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gezik_config::shortcuts::{Action, Chord, Key, Platform};
use gezik_config::store::ConfigStore;
use gezik_core::nav::Location;
use gezik_core::palette::{Folded, Item, Kind, Mode, fold_items, mode_of, rank, remember};
use gezik_core::view::{DateFormat, SizeFormat};
use gezik_i18n::{t, tx};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::context_menu as ids;
use crate::{AppWindow, PickRow};

thread_local! {
    pub(crate) static CURRENT: crate::windows::Slot<Palette> = const { crate::windows::Slot::new() };
}

pub fn with_current(f: impl FnOnce(&Palette)) {
    if let Some(palette) = CURRENT.with(crate::windows::Slot::current) {
        f(&palette);
    }
}

pub fn is_open() -> bool {
    CURRENT.with(crate::windows::Slot::current).is_some_and(|p| p.0.open.get())
}

/// A line of the box: an item, or quick open's last line "Search for …".
#[derive(Debug, Clone, PartialEq)]
pub enum Line {
    Item(usize),
    SearchFor(String),
}

/// The lines for the field's `text`: the items it lets through, then (quick open, something
/// typed) the search for it (spec 7.2's last row).
pub fn lines(items: &[Item], folded: &[Folded], text: &str, recent: &[String]) -> Vec<Line> {
    let (mode, words) = mode_of(text);
    let mut out: Vec<Line> = rank(items, folded, words, mode, recent).into_iter().map(Line::Item).collect();
    if mode == Mode::QuickOpen && !words.trim().is_empty() {
        out.push(Line::SearchFor(words.trim().to_owned()));
    }
    out
}

/// What an item does when chosen.
#[derive(Debug, Clone)]
pub enum Target {
    Action(Action),
    /// A `[[commands]]` entry, by name (the list may change while the box is open).
    Command(String),
    /// A View menu id (`context_menu::view_option_for`).
    ViewOption(u32),
    Folder(PathBuf),
    /// A tab by its place (the tabs cannot change while the box is open).
    Tab(usize),
    TabSet(String),
    Filter(String),
    SavedSearch(String),
    /// A saved folder sync, by name: opened and compared, never applied (spec 11 §8.4).
    SyncProfile(String),
    /// A System Integration command (spec 3.2).
    System(crate::integration::Command),
    /// A listed tag on the selection, or off it (spec 12 §5.3), by name.
    ToggleTag(String),
}

struct Inner {
    window: slint::Weak<AppWindow>,
    store: Option<ConfigStore>,
    run_action: Box<dyn Fn(Action)>,
    open: Cell<bool>,
    items: RefCell<Vec<Item>>,
    targets: RefCell<Vec<Target>>,
    folded: RefCell<Vec<Folded>>,
    lines: RefCell<Vec<Line>>,
    current: Cell<usize>,
    recent: Rc<RefCell<Vec<String>>>,
    model: Rc<VecModel<PickRow>>,
}

#[derive(Clone)]
pub struct Palette(Rc<Inner>);

impl Palette {
    /// Whether `other` (another window's) keeps the same last values as this one.
    #[cfg(test)]
    pub fn shares_with(&self, other: &Palette) -> bool {
        Rc::ptr_eq(&self.0.recent, &other.0.recent)
    }

    pub fn new(
        window: &AppWindow,
        store: Option<ConfigStore>,
        recent: Vec<String>,
        run_action: impl Fn(Action) + 'static,
    ) -> Palette {
        // The process's one recent list: a later window shares an open window's (no window writes over another's).
        let recent = CURRENT
            .with(|c| c.find_map(|_, other| Some(other.0.recent.clone())))
            .unwrap_or_else(|| Rc::new(RefCell::new(recent)));
        let palette = Palette(Rc::new(Inner {
            window: window.as_weak(),
            store,
            run_action: Box::new(run_action),
            open: Cell::new(false),
            items: RefCell::default(),
            targets: RefCell::default(),
            folded: RefCell::default(),
            lines: RefCell::default(),
            current: Cell::new(0),
            recent,
            model: Rc::new(VecModel::default()),
        }));
        CURRENT.with(|c| c.set(crate::windows::current(), palette.clone()));
        palette
    }

    /// Opens the box: the command palette on `>` (`commands`), else quick open (spec 7.1).
    pub fn open(&self, commands: bool) {
        let Some(window) = self.0.window.upgrade() else { return };
        if window.get_tp_open() || crate::tab_tools::over_another_layer(&window) {
            return;
        }
        let mut items = Vec::new();
        let mut targets = Vec::new();
        self.add_items(&mut items, &mut targets);
        *self.0.folded.borrow_mut() = fold_items(&items);
        *self.0.items.borrow_mut() = items;
        *self.0.targets.borrow_mut() = targets;
        self.0.open.set(true);
        window.set_tp_rows(ModelRc::from(self.0.model.clone()));
        window.set_tp_label(if commands { t!("Command palette") } else { t!("Quick open") }.as_ref().into());
        window.set_tp_empty(if commands { t!("No commands match") } else { t!("Nothing matches") }.as_ref().into());
        let text = if commands { ">" } else { "" };
        window.set_tp_query(text.into());
        self.edited(text);
        window.set_tp_open(true);
        window.invoke_focus_list();
    }

    /// Every item and what it does (spec 7.2).
    pub fn add_items(&self, items: &mut Vec<Item>, targets: &mut Vec<Target>) {
        let mut add = |kind: Kind, id: String, title: String, detail: String, target: Target| {
            items.push(Item { kind, id, title, detail, also: String::new() });
            targets.push(target);
            items.len() - 1
        };
        // A translated title is also found by its English one (spec 15 §13.2).
        let mut english = Vec::new();
        for action in Action::ALL {
            // `tag-1…7` are the `Toggle Tag: <name>` items below, which name the tag.
            let skip = matches!(action, Action::CommandPalette | Action::QuickOpen)
                || action.tag_number().is_some()
                || (action == Action::OpenTerminalAdmin && !cfg!(windows))
                || (matches!(action, Action::MakeAlias | Action::ShowPackageContents | Action::Share)
                    && !cfg!(target_os = "macos"))
                || (matches!(action, Action::KeepOffline | Action::FreeUpSpace)
                    && !cfg!(any(windows, target_os = "macos")));
            if !skip {
                let title = gezik_i18n::t("", action.title()).into_owned();
                let at = add(Kind::Action, action.name().to_owned(), title, String::new(), Target::Action(action));
                english.push((at, action.title()));
            }
        }
        for name in crate::convert::command_names() {
            add(Kind::Command, name.clone(), name.clone(), String::new(), Target::Command(name));
        }
        // Actions like `tag-1…7` (remembered as `action:toggle-tag:Red`), named by the tag.
        for tag in crate::tags::list() {
            let (id, title) = (format!("toggle-tag:{}", tag.name), t!("Toggle Tag: {}", tag.name));
            add(Kind::Action, id, title, String::new(), Target::ToggleTag(tag.name.into()));
        }
        let resident = crate::resident::status();
        for command in crate::integration::Command::ALL.into_iter().filter(|c| c.applies(&resident)) {
            let title = gezik_i18n::t("", command.title()).into_owned();
            let at = add(Kind::Command, command.title().to_owned(), title, String::new(), Target::System(command));
            english.push((at, command.title()));
        }
        let options = crate::view_options::current();
        let on = |on: bool| if on { t!("On") } else { t!("Off") }.into_owned();
        let mut toggles = vec![
            (ids::HIDE_EXTENSIONS, tx!("Hide extensions"), options.hide_extensions),
            (ids::FOLDERS_FIRST, tx!("Folders first"), options.folders_first),
            (ids::SINGLE_CLICK_OPEN, tx!("Single-click to open"), options.single_click_open),
        ];
        if cfg!(windows) {
            toggles.push((ids::SHOW_SYSTEM, tx!("Show system items"), options.show_system));
        }
        for (id, title, value) in toggles {
            let shown = gezik_i18n::t("", title).into_owned();
            let at = add(Kind::ViewOption, id.to_string(), shown, on(value), Target::ViewOption(id));
            english.push((at, title));
        }
        for (i, format) in DateFormat::ALL.iter().enumerate() {
            let id = ids::DATE_FORMAT_FIRST + i as u32;
            let title = t!("Date format: {}", gezik_i18n::t("", format.label()));
            add(Kind::ViewOption, id.to_string(), title, on(*format == options.date_format), Target::ViewOption(id));
        }
        for (i, format) in SizeFormat::ALL.iter().enumerate() {
            let id = ids::SIZE_FORMAT_FIRST + i as u32;
            let title = t!("Size format: {}", gezik_i18n::t("", format.label()));
            add(Kind::ViewOption, id.to_string(), title, on(*format == options.size_format), Target::ViewOption(id));
        }
        let mut pinned = Vec::new();
        crate::sidebar::with_current(|s| pinned = s.pinned_places());
        for (label, path) in pinned {
            let text = path.display().to_string();
            add(Kind::Pinned, text.clone(), label, text, Target::Folder(path));
        }
        let mut recent = Vec::new();
        crate::panes::with_active(|p| recent = p.path_box.recent(50));
        for path in recent {
            let text = path.display().to_string();
            let name = path.file_name().map_or_else(|| text.clone(), |n| n.to_string_lossy().into_owned());
            add(Kind::Recent, text.clone(), name, text, Target::Folder(path));
        }
        for (i, (title, path)) in crate::panes::active_nav().tab_list().into_iter().enumerate() {
            add(Kind::Tab, i.to_string(), title, path, Target::Tab(i));
        }
        for name in crate::tab_sets::names() {
            add(Kind::TabSet, name.clone(), name.clone(), String::new(), Target::TabSet(name));
        }
        for filter in crate::filter::saved() {
            add(
                Kind::SavedFilter,
                filter.name.clone(),
                filter.name.clone(),
                filter.pattern,
                Target::Filter(filter.name),
            );
        }
        for profile in crate::sync::profiles() {
            let title = t!("Sync: {}", profile.name);
            let detail = format!("{} ⇄ {}", profile.left, profile.right);
            add(Kind::SyncProfile, profile.name.clone(), title, detail, Target::SyncProfile(profile.name));
        }
        for saved in crate::saved_searches::saved() {
            add(
                Kind::SavedSearch,
                saved.name.clone(),
                saved.name.clone(),
                saved.folder,
                Target::SavedSearch(saved.name),
            );
        }
        for (at, source) in english {
            if items[at].title != source {
                source.clone_into(&mut items[at].also);
            }
        }
    }

    /// The field changed: the lines it lets through, the first one current.
    pub fn edited(&self, text: &str) {
        let lines = lines(&self.0.items.borrow(), &self.0.folded.borrow(), text, &self.0.recent.borrow());
        *self.0.lines.borrow_mut() = lines;
        self.0.current.set(0);
        self.show();
    }

    /// Esc closes; Enter runs the current line (Alt+Enter: a place in a new tab); Up/Down move.
    pub fn chord(&self, chord: &Chord) -> bool {
        let alt_only = chord.alt && !chord.ctrl && !chord.meta && !chord.shift;
        if (chord.ctrl || chord.meta || chord.shift || chord.alt) && !(alt_only && chord.key == Key::Enter) {
            return false;
        }
        match chord.key {
            Key::Escape => self.close(),
            Key::Enter => self.run(self.0.current.get(), alt_only),
            Key::Up | Key::Down => {
                let len = self.0.lines.borrow().len();
                self.0.current.set(crate::tab_tools::step(self.0.current.get(), len, chord.key == Key::Down));
                if let Some(window) = self.0.window.upgrade() {
                    window.set_tp_current(i32::try_from(self.0.current.get()).unwrap_or(0));
                }
            }
            _ => return false,
        }
        true
    }

    /// A click on line `line`.
    pub fn chosen(&self, line: usize) {
        self.run(line, false);
    }

    fn close(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        self.0.open.set(false);
        window.set_tp_open(false);
        self.0.model.set_vec(Vec::new());
        self.0.items.borrow_mut().clear();
        self.0.targets.borrow_mut().clear();
        self.0.folded.borrow_mut().clear();
        if !window.get_dialog_open() && !window.get_conflicts_open() {
            window.invoke_focus_list();
        }
    }

    /// Runs line `line` once the box is gone and the list has the keyboard back (sapma 5);
    /// with no lines the box stays open for the text to change.
    fn run(&self, line: usize, new_tab: bool) {
        let Some(line) = self.0.lines.borrow().get(line).cloned() else { return };
        let target = match &line {
            Line::Item(i) => {
                let key = self.0.items.borrow()[*i].key();
                self.remember(key);
                Some(self.0.targets.borrow()[*i].clone())
            }
            Line::SearchFor(_) => None,
        };
        self.close();
        let this = self.clone();
        slint::Timer::single_shot(
            Duration::ZERO,
            crate::windows::bind(move || match (target, line) {
                (Some(target), _) => this.run_target(target, new_tab),
                (None, Line::SearchFor(text)) => {
                    crate::panes::with_active(|p| p.search.search_for(&text));
                }
                (None, Line::Item(_)) => {}
            }),
        );
    }

    fn run_target(&self, target: Target, new_tab: bool) {
        let nav = &crate::panes::active_nav();
        match target {
            Target::Action(action) => (self.0.run_action)(action),
            Target::Command(name) => {
                if let Some(index) = crate::convert::command_names().iter().position(|n| *n == name) {
                    crate::actions::run_command(index, &crate::panes::active_view());
                }
            }
            Target::ViewOption(id) => {
                if let Some(option) = ids::view_option_for(id, crate::view_options::current()) {
                    crate::view_options::change(option);
                }
            }
            Target::Folder(path) if new_tab => nav.open_tab(Location::Path(path), true),
            Target::Folder(path) => nav.go(Location::Path(path)),
            Target::Tab(index) => nav.activate_tab(index),
            Target::TabSet(name) => crate::tab_sets::with_current(|sets| sets.open(&name, false)),
            Target::Filter(name) => {
                crate::panes::with_active(|p| p.filter.apply_saved(&name));
            }
            Target::SavedSearch(name) => crate::saved_searches::with_current(|s| s.run(&name, new_tab)),
            Target::SyncProfile(name) => crate::compare::open_profile(&name),
            Target::System(command) => crate::integration::run(command),
            Target::ToggleTag(name) => crate::tags::toggle_named(&name, &crate::panes::active_view()),
        }
    }

    fn remember(&self, key: String) {
        let recent = {
            let mut recent = self.0.recent.borrow_mut();
            remember(&mut recent, key);
            recent.clone()
        };
        if let Some(store) = &self.0.store {
            store.update_state(move |state| state.palette_recent = recent);
        }
    }

    fn show(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        let items = self.0.items.borrow();
        let targets = self.0.targets.borrow();
        let rows: Vec<PickRow> = self
            .0
            .lines
            .borrow()
            .iter()
            .map(|line| match line {
                Line::Item(i) => {
                    let item = &items[*i];
                    let keys = match &targets[*i] {
                        Target::Action(action) => crate::keys::chord_for(*action)
                            .map(|chord| crate::keys::chord_label(&chord, Platform::current()))
                            .unwrap_or_default(),
                        // The first seven tags' keys are `tag-1…7`'s.
                        Target::ToggleTag(name) => crate::tags::key_label(&crate::tags::list(), name),
                        _ => String::new(),
                    };
                    PickRow {
                        title: item.title.as_str().into(),
                        detail: item.detail.as_str().into(),
                        tag: gezik_i18n::t("", item.kind.tag()).as_ref().into(),
                        keys: keys.into(),
                        bold: false,
                        dot: slint::Color::default(),
                        check: -1,
                        stars: -1,
                    }
                }
                Line::SearchFor(text) => {
                    let place = match crate::panes::active_nav().active_location() {
                        Location::Drives => t!("This PC").into_owned(),
                        location => location
                            .folder()
                            .and_then(|f| f.file_name())
                            .map_or_else(|| t!("This PC").into_owned(), |n| n.to_string_lossy().into_owned()),
                    };
                    PickRow {
                        title: t!("Search for \"{}\" in {}", text, place).into(),
                        detail: "".into(),
                        tag: t!("Search").as_ref().into(),
                        keys: "".into(),
                        bold: false,
                        dot: slint::Color::default(),
                        check: -1,
                        stars: -1,
                    }
                }
            })
            .collect();
        self.0.model.set_vec(rows);
        window.set_tp_current(i32::try_from(self.0.current.get()).unwrap_or(0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gezik_core::palette::{Item, Kind, fold_items};

    fn item(kind: Kind, title: &str) -> Item {
        Item { kind, id: title.into(), title: title.into(), detail: String::new(), also: String::new() }
    }

    #[test]
    fn quick_open_ends_with_a_search_for_the_text() {
        let items = vec![item(Kind::Recent, "Work"), item(Kind::Action, "Copy")];
        let folded = fold_items(&items);
        assert_eq!(lines(&items, &folded, "wo", &[]), [Line::Item(0), Line::SearchFor("wo".into())]);
        assert_eq!(lines(&items, &folded, "", &[]), [Line::Item(0), Line::Item(1)], "no text: no search line");
        assert_eq!(lines(&items, &folded, ">co", &[]), [Line::Item(1)], "the command palette never searches");
        assert_eq!(
            lines(&items, &folded, "zz", &[]),
            [Line::SearchFor("zz".into())],
            "nothing matches: the search still"
        );
    }

    #[test]
    fn the_menu_bar_titles_are_the_actions_titles() {
        let slint = include_str!("../ui/app.slint");
        let mut checked = 0;
        for line in slint.lines().filter(|l| l.contains("MenuItem { title: @tr(\"")) {
            let Some(name) = line.split("menu-command(\"").nth(1).and_then(|r| r.split('"').next()) else { continue };
            let Some(action) = Action::from_name(name) else { continue };
            // The View menu says "as List", "as Grid" and Group By's "None" … under its own titles
            // (sapma 6; 10d sapma 15).
            if matches!(
                action,
                Action::ViewList
                    | Action::ViewGrid
                    | Action::ViewColumns
                    | Action::GroupNone
                    | Action::GroupType
                    | Action::GroupDate
                    | Action::GroupSize
            ) {
                continue;
            }
            let title = line.split("title: @tr(\"").nth(1).and_then(|r| r.split('"').next()).unwrap_or_default();
            assert_eq!(title, action.title(), "{name}");
            checked += 1;
        }
        assert!(checked >= 50, "only {checked} menu items read");
        for name in ["copy-to", "move-to", "split-file", "join-files"] {
            assert!(slint.contains(&format!("menu-command(\"{name}\")")), "{name} in the Tools menu");
        }
    }
}
