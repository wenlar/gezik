//! Tags in the window (spec 12 §5): `[tags]` as in effect, the tag picker (the tab picker's
//! box, `widgets/picker.slint`), `tags`, `tag-1…7` and `clear-tags`, the rows' tag dots, and
//! the reads they need. Files are read only off the UI thread: the rows on screen through
//! `crate::media` while dots show, a selection when the picker opens or a tag key toggles.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, OnceLock};

use gezik_config::shortcuts::{Action, Chord, Key, Platform};
use gezik_config::store::ConfigStore;
use gezik_config::tags::{Dots, TagsSettings};
use gezik_core::meta::{Meta, MetaEdit, Tag, TagColor, check_tag_name, same_name};
use gezik_core::pattern::fold_text;
use gezik_ops::meta::MetaStore;
use slint::{Color, ComponentHandle, ModelRc, VecModel};

use crate::view::View;
use crate::{AppWindow, PickRow};

/// The engine's tag store: what every read goes through (files and Gezik's own store).
static STORE: OnceLock<Arc<MetaStore>> = OnceLock::new();

#[derive(Default)]
struct State {
    settings: TagsSettings,
    /// A tag was written once (`state.toml` `[tags] used`).
    used: bool,
    config: Option<ConfigStore>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
    pub(crate) static CURRENT: crate::windows::Slot<TagPicker> = const { crate::windows::Slot::new() };
}

/// Once, at start: the engine's store, where settings.toml and state.toml are, and whether a
/// tag was written before.
pub fn install(store: Arc<MetaStore>, config: Option<ConfigStore>, used: bool) {
    let _ = STORE.set(store);
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.config = config;
        s.used = used;
    });
}

/// `[tags]` read (every resolve): rows are drawn again when the dots come or go.
pub fn set_settings(settings: TagsSettings) {
    let before = dots_on();
    let list_changed = STATE.with(|s| {
        let mut s = s.borrow_mut();
        let changed = s.settings.list != settings.list;
        s.settings = settings;
        changed
    });
    if before != dots_on() || (list_changed && dots_on()) {
        redraw_all();
    }
    sync_window();
}

/// The list in effect.
pub fn list() -> Vec<Tag> {
    STATE.with(|s| s.borrow().settings.list.clone())
}

/// `f` with the list in effect, not copied (each row drawn).
pub fn with_list<R>(f: impl FnOnce(&[Tag]) -> R) -> R {
    STATE.with(|s| f(&s.borrow().settings.list))
}

/// Whether rows show tag dots now (`[tags] dots`; `auto`: macOS, or a tag was written).
pub fn dots_on() -> bool {
    STATE.with(|s| {
        let s = s.borrow();
        dots_shown(s.settings.dots, s.used, cfg!(target_os = "macos"))
    })
}

pub fn dots_shown(dots: Dots, used: bool, mac: bool) -> bool {
    match dots {
        Dots::On => true,
        Dots::Off => false,
        Dots::Auto => mac || used,
    }
}

/// A row's tags, on the icon worker (media.rs); a file that cannot be read has none.
pub fn read_for_rows(path: &Path) -> Box<[Tag]> {
    STORE.get().and_then(|store| store.read(path).ok()).map(|meta| meta.tags.into_boxed_slice()).unwrap_or_default()
}

/// Finder's colours, the same in light and dark themes; no colour: transparent (a ring).
pub fn color_of(color: TagColor) -> Color {
    let rgb = |r, g, b| Color::from_rgb_u8(r, g, b);
    match color {
        TagColor::None => Color::from_argb_u8(0, 0, 0, 0),
        TagColor::Gray => rgb(0x8e, 0x8e, 0x93),
        TagColor::Green => rgb(0x34, 0xc7, 0x59),
        TagColor::Purple => rgb(0xaf, 0x52, 0xde),
        TagColor::Blue => rgb(0x00, 0x7a, 0xff),
        TagColor::Yellow => rgb(0xff, 0xcc, 0x00),
        TagColor::Red => rgb(0xff, 0x3b, 0x30),
        TagColor::Orange => rgb(0xff, 0x95, 0x00),
    }
}

/// The colour a tag shows: the list's for a listed name, else the file's (on Linux, where files
/// keep none, gray).
pub fn shown_color(tag: &Tag, list: &[Tag], linux: bool) -> TagColor {
    match list.iter().find(|t| same_name(&t.name, &tag.name)) {
        Some(listed) => listed.color,
        None if linux && tag.color == TagColor::None => TagColor::Gray,
        None => tag.color,
    }
}

/// The most dots a row draws; more are a count (`+2`).
pub const MAX_DOTS: usize = 3;

/// A row's dots (at most three), how many more, and the names for a screen reader.
pub fn dots_of(tags: &[Tag], list: &[Tag]) -> (Vec<Color>, i32, String) {
    let linux = cfg!(target_os = "linux");
    let colors = tags.iter().take(MAX_DOTS).map(|t| color_of(shown_color(t, list, linux))).collect();
    let more = i32::try_from(tags.len().saturating_sub(MAX_DOTS)).unwrap_or(i32::MAX);
    let names = tags.iter().map(|t| &*t.name).collect::<Vec<_>>().join(", ");
    (colors, more, names)
}

/// Drops what is known of every row's tags and draws the rows again (a tag job ended, F5).
pub fn forget_rows() {
    crate::media::with_current(crate::media::Media::forget_tags);
    if dots_on() {
        redraw_all();
    }
}

fn redraw_all() {
    for p in crate::panes::everywhere() {
        p.view.redraw();
    }
}

/// The macOS View menu's Tag Dots mark.
fn sync_window() {
    let on = dots_on();
    crate::windows::for_each(|window| window.set_view_tag_dots(on));
}

/// View ▸ Tag dots: shows or hides them, and writes `[tags] dots`.
pub fn toggle_dots() {
    let dots = if dots_on() { Dots::Off } else { Dots::On };
    let config = STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.settings.dots = dots;
        s.config.clone()
    });
    redraw_all();
    sync_window();
    if let Some(config) = config {
        let id = crate::windows::current();
        config.write_settings(gezik_config::settings_writer::SettingsChange::TagDots(dots), move |result| {
            if let Err(warning) = result {
                crate::windows::post(id, move || crate::panes::active_view().note(warning.to_string()));
            }
        });
    }
}

/// What the tag actions act on: the selection, else the focused item; `None` (said in the
/// status bar) for drives and the trash.
fn targets(view: &View) -> Option<Vec<PathBuf>> {
    if view.shows_drives() || view.shows_trash() {
        view.note("Tags work on files and folders".to_owned());
        return None;
    }
    let mut items: Vec<PathBuf> = view.selected_items().into_iter().map(|(path, _)| path).collect();
    if items.is_empty() {
        items.extend(view.focus().and_then(|i| view.entry_path(i)).map(|(path, _)| path));
    }
    (!items.is_empty()).then_some(items)
}

/// Reads `paths`' tags on a thread, then runs `then` with them in this window.
fn read_then(paths: Vec<PathBuf>, then: impl FnOnce(Vec<PathBuf>, Vec<Meta>) + Send + 'static) {
    let Some(store) = STORE.get().cloned() else { return };
    let id = crate::windows::current();
    let spawned = std::thread::Builder::new().name("gezik-tags".into()).spawn(move || {
        let metas = paths.iter().map(|path| store.read(path).unwrap_or_default()).collect();
        crate::windows::post(id, move || then(paths, metas));
    });
    if let Err(err) = spawned {
        eprintln!("gezik: cannot start the tags thread: {err}");
    }
}

/// `edits` on `paths` as one job (one undo); the first tag written turns `dots = "auto"` on.
fn submit(paths: Vec<PathBuf>, edits: Vec<MetaEdit>) {
    if paths.is_empty() || edits.is_empty() {
        return;
    }
    let adds = edits.iter().any(|e| matches!(e, MetaEdit::AddTags(_)));
    let task = Box::new(gezik_ops::TagsTask::new(paths, edits));
    crate::operations::with_current(|ops| {
        ops.submit(task, None, crate::operations::After::Nothing);
    });
    if adds && !STATE.with(|s| s.borrow().used) {
        let before = dots_on();
        let config = STATE.with(|s| {
            let mut s = s.borrow_mut();
            s.used = true;
            s.config.clone()
        });
        if let Some(config) = config {
            config.update_state(|state| state.tags_used = true);
        }
        if before != dots_on() {
            redraw_all();
            sync_window();
        }
    }
}

/// Whether `tag-n` takes `name` off (all have it) rather than putting it on.
pub fn all_have(metas: &[Meta], name: &str) -> bool {
    !metas.is_empty() && metas.iter().all(|m| m.has_tag(name))
}

/// `tag-n` (1-based): the list's n-th tag on the selection, or off it when all have it.
pub fn toggle_nth(n: usize, view: &View) {
    let Some(tag) = list().get(n.wrapping_sub(1)).cloned() else {
        return view.note(format!("No tag {n} in your list ([tags] list)"));
    };
    let Some(paths) = targets(view) else { return };
    toggle_on(tag, paths);
}

/// `tag` on `paths`, or off them when all have it; the status bar says which.
pub fn toggle_on(tag: Tag, paths: Vec<PathBuf>) {
    read_then(paths, move |paths, metas| {
        let off = all_have(&metas, &tag.name);
        let what = crate::stack::count_text(paths.len());
        let note =
            if off { format!("{} removed from {what}", tag.name) } else { format!("{} added to {what}", tag.name) };
        let edit = if off { MetaEdit::RemoveTags(vec![tag.name.clone()]) } else { MetaEdit::AddTags(vec![tag]) };
        submit(paths, vec![edit]);
        crate::panes::active_view().note(note);
    });
}

/// `clear-tags`: every tag off the selection (rating and comment stay); nothing to read first.
pub fn clear(view: &View) {
    if let Some(paths) = targets(view) {
        clear_paths(paths);
    }
}

pub fn clear_paths(paths: Vec<PathBuf>) {
    let note = format!("Tags cleared from {}", crate::stack::count_text(paths.len()));
    submit(paths, vec![MetaEdit::ClearTags]);
    crate::panes::active_view().note(note);
}

/// `tags`: the picker for the selection.
pub fn open_picker(view: &View) {
    if let Some(paths) = targets(view) {
        with_current(|p| p.open(paths));
    }
}

/// A tag job (or its undo) ended: rows read their tags again.
pub fn job_done() {
    forget_rows();
}

// The menus (spec 12 §5.3, §12.3: 2400–2599).

/// `Tags ▸` for `paths` at place `at`: the list's tags, marked `• ` when every row (whose tags
/// are known) has it, `– ` when some do; then Tags… and Clear tags.
pub fn menu_sub(paths: &[PathBuf], at: usize) -> crate::context_menu::Submenu {
    use crate::context_menu::{TAGS_CLEAR, TAGS_PICKER, TAGS_TAG_FIRST};
    let mut known: Vec<Box<[Tag]>> = Vec::new();
    crate::media::with_current(|m| known = paths.iter().filter_map(|p| m.known_tags(p)).collect());
    // Marks only when every row is known: nothing is read for a menu.
    let all_known = known.len() == paths.len() && !paths.is_empty();
    let mut items: Vec<(u32, String, bool)> = list()
        .iter()
        .enumerate()
        .map(|(i, tag)| {
            let count = known.iter().filter(|tags| tags.iter().any(|t| same_name(&t.name, &tag.name))).count();
            let mark = match (all_known, count) {
                (false, _) | (_, 0) => "    ",
                (true, n) if n == paths.len() => "• ",
                _ => "– ",
            };
            (TAGS_TAG_FIRST + i as u32, format!("{mark}{}", tag.name), true)
        })
        .collect();
    items.push((TAGS_PICKER, "Tags…".to_owned(), true));
    items.push((TAGS_CLEAR, "Clear tags".to_owned(), true));
    crate::context_menu::Submenu { title: "Tags".to_owned(), at, items }
}

/// A `Tags ▸` item chosen for `paths`; returns whether `id` was one.
pub fn menu_chosen(id: u32, paths: Vec<PathBuf>) -> bool {
    use crate::context_menu::{TAGS_CLEAR, TAGS_PICKER, TAGS_TAG_FIRST, TAGS_TAG_MAX};
    match id {
        TAGS_PICKER => with_current(|p| p.open(paths)),
        TAGS_CLEAR => clear_paths(paths),
        id if (TAGS_TAG_FIRST..TAGS_TAG_FIRST + TAGS_TAG_MAX).contains(&id) => {
            if let Some(tag) = list().get((id - TAGS_TAG_FIRST) as usize).cloned() {
                toggle_on(tag, paths);
            }
        }
        _ => return false,
    }
    true
}

/// The listed tag `name` on the selection, or off it (the palette's Toggle Tag items).
pub fn toggle_named(name: &str, view: &View) {
    let Some(tag) = list().into_iter().find(|t| same_name(&t.name, name)) else { return };
    if let Some(paths) = targets(view) {
        toggle_on(tag, paths);
    }
}

// The picker (spec 12 §5.1).

/// A line of the picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    /// A tag, and whether the list has it.
    Tag(Tag, bool),
    /// `Create "…"`: the typed name.
    Create(String),
}

/// What the picker changes when it closes: tags put on every item, and tags taken off all.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Edits {
    pub adds: Vec<Tag>,
    pub removes: Vec<Box<str>>,
}

impl Edits {
    /// One job's edits: the removals, then the additions.
    pub fn to_edits(&self) -> Vec<MetaEdit> {
        let mut out = Vec::new();
        if !self.removes.is_empty() {
            out.push(MetaEdit::RemoveTags(self.removes.clone()));
        }
        if !self.adds.is_empty() {
            out.push(MetaEdit::AddTags(self.adds.clone()));
        }
        out
    }

    /// 2 on every item, 1 on some, 0 on none, as the picker shows `name` now; `metas` `None`
    /// while they are read (nothing is known to be on any item yet).
    pub fn check(&self, name: &str, metas: Option<&[Meta]>) -> i32 {
        if self.adds.iter().any(|t| same_name(&t.name, name)) {
            return 2;
        }
        if self.removes.iter().any(|n| same_name(n, name)) {
            return 0;
        }
        original(name, metas)
    }

    /// A click on `tag`: on every item when it is not on all, off all when it is.
    pub fn toggle(&mut self, tag: &Tag, metas: Option<&[Meta]>) {
        let name = &tag.name;
        let was = original(name, metas);
        if self.check(name, metas) == 2 {
            self.adds.retain(|t| !same_name(&t.name, name));
            if metas.is_some() && was != 0 {
                self.removes.push(name.clone());
            }
        } else {
            self.removes.retain(|n| !same_name(n, name));
            if was != 2 {
                self.adds.push(tag.clone());
            }
        }
    }
}

/// How many of `metas` have `name`, as 0, 1 (some) or 2 (all).
fn original(name: &str, metas: Option<&[Meta]>) -> i32 {
    let Some(metas) = metas else { return 0 };
    match metas.iter().filter(|m| m.has_tag(name)).count() {
        0 => 0,
        n if n == metas.len() => 2,
        _ => 1,
    }
}

/// The picker's lines for `query`: the list's tags, then those the items have that the list
/// has not, then `Create "…"` when the typed name is no tag's.
pub fn picker_lines(list: &[Tag], metas: Option<&[Meta]>, edits: &Edits, query: &str) -> Vec<Line> {
    let mut tags: Vec<Line> = list.iter().map(|t| Line::Tag(t.clone(), true)).collect();
    let found = metas.into_iter().flatten().flat_map(|m| &m.tags).chain(&edits.adds);
    for tag in found {
        if !tags.iter().any(|line| matches!(line, Line::Tag(t, _) if same_name(&t.name, &tag.name))) {
            tags.push(Line::Tag(tag.clone(), false));
        }
    }
    let typed = query.trim();
    let folded = fold_text(typed);
    let exact = tags.iter().any(|line| matches!(line, Line::Tag(t, _) if same_name(&t.name, typed)));
    let mut lines: Vec<Line> = tags
        .into_iter()
        .filter(|line| matches!(line, Line::Tag(t, _) if fold_text(&t.name).contains(&folded)))
        .collect();
    if !typed.is_empty() && !exact {
        lines.push(Line::Create(typed.to_owned()));
    }
    lines
}

pub fn with_current(f: impl FnOnce(&TagPicker)) {
    if let Some(picker) = CURRENT.with(crate::windows::Slot::current) {
        f(&picker);
    }
}

pub fn is_open() -> bool {
    CURRENT.with(crate::windows::Slot::current).is_some_and(|p| p.0.open.get())
}

struct Inner {
    window: slint::Weak<AppWindow>,
    open: Cell<bool>,
    /// Which opening the reads belong to: a late answer for an earlier one is dropped.
    opening: Cell<u64>,
    paths: RefCell<Vec<PathBuf>>,
    /// The items' tags; `None` while they are read.
    metas: RefCell<Option<Vec<Meta>>>,
    edits: RefCell<Edits>,
    query: RefCell<String>,
    lines: RefCell<Vec<Line>>,
    current: Cell<usize>,
    model: Rc<VecModel<PickRow>>,
}

#[derive(Clone)]
pub struct TagPicker(Rc<Inner>);

impl TagPicker {
    pub fn new(window: &AppWindow) -> TagPicker {
        let picker = TagPicker(Rc::new(Inner {
            window: window.as_weak(),
            open: Cell::new(false),
            opening: Cell::new(0),
            paths: RefCell::default(),
            metas: RefCell::default(),
            edits: RefCell::default(),
            query: RefCell::default(),
            lines: RefCell::default(),
            current: Cell::new(0),
            model: Rc::new(VecModel::default()),
        }));
        CURRENT.with(|c| c.set(crate::windows::current(), picker.clone()));
        picker
    }

    /// Opens on `paths`; their tags are read meanwhile.
    pub fn open(&self, paths: Vec<PathBuf>) {
        let Some(window) = self.0.window.upgrade() else { return };
        if paths.is_empty() || window.get_tp_open() || crate::tab_tools::over_another_layer(&window) {
            return;
        }
        let opening = self.0.opening.get() + 1;
        self.0.opening.set(opening);
        *self.0.paths.borrow_mut() = paths.clone();
        *self.0.metas.borrow_mut() = None;
        *self.0.edits.borrow_mut() = Edits::default();
        self.0.query.borrow_mut().clear();
        self.0.current.set(0);
        self.0.open.set(true);
        window.set_tp_rows(ModelRc::from(self.0.model.clone()));
        window.set_tp_empty("No tags match".into());
        window.set_tp_query("".into());
        self.show();
        window.set_tp_open(true);
        window.invoke_focus_list();
        read_then(paths, move |_, metas| {
            with_current(|p| {
                if p.0.open.get() && p.0.opening.get() == opening {
                    *p.0.metas.borrow_mut() = Some(metas);
                    p.show();
                }
            });
        });
    }

    pub fn edited(&self, query: &str) {
        *self.0.query.borrow_mut() = query.to_owned();
        self.0.current.set(0);
        self.show();
    }

    /// Esc closes (and applies), Enter toggles the current line, Up/Down move.
    pub fn chord(&self, chord: &Chord) -> bool {
        if chord.ctrl || chord.alt || chord.meta || chord.shift {
            return false;
        }
        match chord.key {
            Key::Escape => self.close(),
            Key::Enter => self.choose(self.0.current.get()),
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
        self.choose(line);
    }

    /// Toggles a tag on the items (in memory; the box stays open), or creates the typed one.
    fn choose(&self, line: usize) {
        let Some(line) = self.0.lines.borrow().get(line).cloned() else { return };
        self.0.current.set(self.0.lines.borrow().iter().position(|l| *l == line).unwrap_or(0));
        let tag = match line {
            Line::Tag(tag, _) => tag,
            Line::Create(name) => {
                if let Err(err) = check_tag_name(&name) {
                    return crate::panes::active_view().note(format!("\"{name}\": {err}"));
                }
                let tag = Tag::new(&name, TagColor::None);
                add_to_list(tag.clone());
                if let Some(window) = self.0.window.upgrade() {
                    window.set_tp_query("".into());
                }
                self.0.query.borrow_mut().clear();
                tag
            }
        };
        {
            let metas = self.0.metas.borrow();
            self.0.edits.borrow_mut().toggle(&tag, metas.as_deref());
        }
        self.show();
    }

    /// Closes the box; what changed becomes one job.
    fn close(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        self.0.open.set(false);
        window.set_tp_open(false);
        self.0.model.set_vec(Vec::new());
        let edits = std::mem::take(&mut *self.0.edits.borrow_mut()).to_edits();
        let paths = std::mem::take(&mut *self.0.paths.borrow_mut());
        *self.0.metas.borrow_mut() = None;
        self.0.lines.borrow_mut().clear();
        if !window.get_dialog_open() && !window.get_conflicts_open() {
            window.invoke_focus_list();
        }
        submit(paths, edits);
    }

    fn show(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        let list = list();
        let metas = self.0.metas.borrow();
        let edits = self.0.edits.borrow();
        let lines = picker_lines(&list, metas.as_deref(), &edits, &self.0.query.borrow());
        let paths = self.0.paths.borrow();
        let label = match (metas.is_some(), paths.as_slice()) {
            (false, _) => "Reading tags…".to_owned(),
            (true, many) => format!("Tags for {}", crate::operations::items_text(many)),
        };
        let rows: Vec<PickRow> = lines
            .iter()
            .map(|line| match line {
                Line::Tag(tag, listed) => {
                    let keys = key_label(&list, &tag.name);
                    let linux = cfg!(target_os = "linux");
                    PickRow {
                        title: (*tag.name).into(),
                        detail: if *listed { "" } else { "Not in your list" }.into(),
                        tag: "".into(),
                        keys: keys.into(),
                        bold: false,
                        dot: color_of(shown_color(tag, &list, linux)),
                        check: edits.check(&tag.name, metas.as_deref()),
                    }
                }
                Line::Create(name) => PickRow {
                    title: format!("Create \"{name}\"").into(),
                    detail: "".into(),
                    tag: "New tag".into(),
                    keys: "".into(),
                    bold: false,
                    dot: Color::default(),
                    check: -1,
                },
            })
            .collect();
        drop((metas, edits, paths));
        *self.0.lines.borrow_mut() = lines;
        let current = self.0.current.get().min(rows.len().saturating_sub(1));
        self.0.current.set(current);
        self.0.model.set_vec(rows);
        window.set_tp_label(label.into());
        window.set_tp_current(i32::try_from(current).unwrap_or(0));
    }
}

/// The key of the `tag-N` action for `name`'s place in `list` (the first seven), else "".
pub fn key_label(list: &[Tag], name: &str) -> String {
    list.iter()
        .position(|t| same_name(&t.name, name))
        .and_then(|i| Action::ALL.into_iter().find(|a| a.tag_number() == Some(i + 1)))
        .and_then(crate::keys::chord_for)
        .map(|chord| crate::keys::chord_label(&chord, Platform::current()))
        .unwrap_or_default()
}

/// Puts `tag` at the end of `[tags] list` (shown at once, written to settings.toml).
fn add_to_list(tag: Tag) {
    let (list, config) = STATE.with(|s| {
        let mut s = s.borrow_mut();
        if !s.settings.list.iter().any(|t| same_name(&t.name, &tag.name))
            && s.settings.list.len() < gezik_config::tags::TAGS_MAX
        {
            s.settings.list.push(tag);
        }
        (s.settings.list.clone(), s.config.clone())
    });
    if let Some(config) = config {
        let id = crate::windows::current();
        config.write_settings(gezik_config::settings_writer::SettingsChange::TagList(list), move |result| {
            if let Err(warning) = result {
                crate::windows::post(id, move || crate::panes::active_view().note(warning.to_string()));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(name: &str, color: TagColor) -> Tag {
        Tag::new(name, color)
    }

    fn with(names: &[&str]) -> Meta {
        Meta { tags: names.iter().map(|n| tag(n, TagColor::None)).collect(), ..Meta::default() }
    }

    fn list() -> Vec<Tag> {
        vec![tag("Red", TagColor::Red), tag("Orange", TagColor::Orange), tag("Green", TagColor::Green)]
    }

    #[test]
    fn the_picker_lists_the_list_then_the_items_own_then_create() {
        let metas = [with(&["Project X", "red"]), with(&["Red"])];
        let lines = picker_lines(&list(), Some(&metas), &Edits::default(), "");
        let names: Vec<String> = lines
            .iter()
            .map(|l| match l {
                Line::Tag(t, listed) => format!("{}{}", t.name, if *listed { "" } else { "*" }),
                Line::Create(n) => format!("+{n}"),
            })
            .collect();
        assert_eq!(names, ["Red", "Orange", "Green", "Project X*"], "red is Red: one line");
        let typed = picker_lines(&list(), Some(&metas), &Edits::default(), "pro");
        assert_eq!(typed, [Line::Tag(tag("Project X", TagColor::None), false), Line::Create("pro".into())]);
        let exact = picker_lines(&list(), Some(&metas), &Edits::default(), " GREEN ");
        assert_eq!(exact, [Line::Tag(tag("Green", TagColor::Green), true)], "a tag's own name: no Create");
        let turkish = picker_lines(&[tag("İş", TagColor::Blue)], None, &Edits::default(), "iş");
        assert_eq!(turkish.len(), 1, "Turkish İ folds");
    }

    #[test]
    fn checks_say_all_some_or_none_and_a_click_flips_all() {
        let metas = [with(&["Red", "Orange"]), with(&["Red"])];
        let m = Some(&metas[..]);
        let mut edits = Edits::default();
        assert_eq!((edits.check("Red", m), edits.check("orange", m), edits.check("Green", m)), (2, 1, 0));
        // ✓ → off all; – → on all; none → on all.
        edits.toggle(&tag("Red", TagColor::Red), m);
        edits.toggle(&tag("Orange", TagColor::Orange), m);
        edits.toggle(&tag("Green", TagColor::Green), m);
        assert_eq!((edits.check("Red", m), edits.check("Orange", m), edits.check("Green", m)), (0, 2, 2));
        assert_eq!(
            edits.to_edits(),
            [
                MetaEdit::RemoveTags(vec!["Red".into()]),
                MetaEdit::AddTags(vec![tag("Orange", TagColor::Orange), tag("Green", TagColor::Green)]),
            ],
            "one job: removals, then additions"
        );
        // Back where it was: nothing to do.
        edits.toggle(&tag("Red", TagColor::Red), m);
        edits.toggle(&tag("Green", TagColor::Green), m);
        assert_eq!(edits.check("Red", m), 2);
        assert_eq!(edits.check("Green", m), 0);
        assert_eq!(edits.to_edits(), [MetaEdit::AddTags(vec![tag("Orange", TagColor::Orange)])]);
    }

    #[test]
    fn while_tags_are_read_a_click_only_adds() {
        let mut edits = Edits::default();
        assert_eq!(edits.check("Red", None), 0, "no – before the reads");
        edits.toggle(&tag("Red", TagColor::Red), None);
        assert_eq!(edits.to_edits(), [MetaEdit::AddTags(vec![tag("Red", TagColor::Red)])]);
        edits.toggle(&tag("Red", TagColor::Red), None);
        assert_eq!(edits.to_edits(), [], "a second click takes the addition back, removes nothing");
    }

    #[test]
    fn a_tag_key_takes_off_only_what_all_have() {
        let metas = [with(&["Red"]), with(&["red", "Blue"])];
        assert!(all_have(&metas, "Red"));
        assert!(!all_have(&metas, "Blue"));
        assert!(!all_have(&[], "Red"));
    }

    #[test]
    fn dots_show_three_and_count_the_rest_in_the_lists_colours() {
        let tags = [
            tag("red", TagColor::Blue),
            tag("Project X", TagColor::None),
            tag("Mine", TagColor::Purple),
            tag("A", TagColor::None),
            tag("B", TagColor::None),
        ];
        let (colors, more, names) = dots_of(&tags, &list());
        assert_eq!(colors.len(), MAX_DOTS);
        assert_eq!(colors[0], color_of(TagColor::Red), "the list's colour wins");
        let unlisted =
            if cfg!(target_os = "linux") { color_of(TagColor::Gray) } else { Color::from_argb_u8(0, 0, 0, 0) };
        assert_eq!(colors[1], unlisted, "no colour: a ring (gray on Linux)");
        assert_eq!(colors[2], color_of(TagColor::Purple), "the file's own colour");
        assert_eq!(more, 2);
        assert_eq!(names, "red, Project X, Mine, A, B");
        assert_eq!(dots_of(&[], &list()), (Vec::new(), 0, String::new()));
    }

    #[test]
    fn auto_dots_wait_for_the_first_tag_off_macos() {
        assert!(!dots_shown(Dots::Auto, false, false));
        assert!(dots_shown(Dots::Auto, true, false));
        assert!(dots_shown(Dots::Auto, false, true), "macOS: Finder's users expect them");
        assert!(dots_shown(Dots::On, false, false) && !dots_shown(Dots::Off, true, true));
    }
}
