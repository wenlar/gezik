//! The batch rename layer: rules on the left, the preview on the right, Rename runs one
//! `RenameTask`. The rules and their results live here; Slint only shows them.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use gezik_batch::rename::{Item, Status, check, compile, new_names};
use gezik_config::settings::{BatchRenameState, RenamePreset};
use gezik_config::shortcuts::{Chord, Key, Platform};
use gezik_core::Entry;
use gezik_core::batch::case::{CaseMode, Lang};
use gezik_core::batch::date::DateParts;
use gezik_core::batch::rules::{ExtensionRule, NumberAt, Rule, RuleEntry};
use gezik_core::ops::names::NameRules;
use gezik_core::selection::Selection;
use slint::{ComponentHandle, Model, ModelRc, VecModel};

use crate::{AppWindow, RenamePreviewRow, RuleOptions, RuleRow};

/// Whether this system's file names ignore case (Windows, macOS).
const IGNORE_CASE: bool = cfg!(any(windows, target_os = "macos"));

/// Everything the layer knows; no Slint in here, so it is tested on its own.
#[derive(Default)]
pub struct Rules {
    pub items: Vec<Item>,
    pub paths: Vec<PathBuf>,
    /// Display order: indices into `items`.
    pub order: Vec<usize>,
    pub rules: Vec<RuleEntry>,
    pub selected_rule: Option<usize>,
    pub include_extension: bool,
    /// Names typed by hand, by item.
    pub manual: HashMap<usize, String>,
    /// Names in each item's folder that are not being renamed, by `Item::folder`.
    pub others: Vec<HashSet<String>>,
    /// The folders of search results are being read for `others`: no renaming yet.
    pub reading: bool,
    pub lang: Lang,
    // Results, by display position.
    pub new: Vec<String>,
    pub statuses: Vec<Status>,
    pub rule_errors: Vec<Option<String>>,
    /// Some rule uses `{taken}` (set by `recompute`).
    pub needs_taken: bool,
    /// The photo dates are still being read: no renaming yet.
    pub waiting: bool,
}

impl Rules {
    pub fn recompute(&mut self) {
        let items: Vec<Item> = self.order.iter().map(|&i| self.items[i].clone()).collect();
        let compiled = compile(&self.rules);
        let mut new = new_names(&items, &compiled, self.include_extension, self.lang);
        for (position, &i) in self.order.iter().enumerate() {
            if let Some(name) = self.manual.get(&i) {
                new[position] = name.clone();
            }
        }
        let others: Vec<HashSet<String>> = self
            .others
            .iter()
            .map(|names| names.iter().map(|n| if IGNORE_CASE { n.to_lowercase() } else { n.clone() }).collect())
            .collect();
        let existing = |folder: usize, name: &str| {
            others
                .get(folder)
                .is_some_and(|names| names.contains(&if IGNORE_CASE { name.to_lowercase() } else { name.to_owned() }))
        };
        self.statuses = check(&items, &new, &existing, NameRules::current(), IGNORE_CASE);
        self.rule_errors = compiled.errors().to_vec();
        self.needs_taken = compiled.needs_taken();
        self.new = new;
    }

    /// Display position `from` moves to `to`.
    pub fn reorder(&mut self, from: usize, to: usize) {
        if from >= self.order.len() || to >= self.order.len() || from == to {
            return;
        }
        let item = self.order.remove(from);
        self.order.insert(to, item);
    }

    /// The name at display position `position`, typed by hand; the rules' name clears it.
    pub fn set_manual(&mut self, position: usize, name: &str) {
        let Some(&i) = self.order.get(position) else { return };
        self.manual.remove(&i);
        // What the rules give without the manual name.
        self.recompute();
        if self.new.get(position).is_none_or(|ruled| ruled != name) {
            self.manual.insert(i, name.to_owned());
        }
    }

    pub fn reset_manual(&mut self, positions: &[usize]) {
        for &p in positions {
            if let Some(i) = self.order.get(p) {
                self.manual.remove(i);
            }
        }
    }

    /// Back to the list's order.
    pub fn reset_order(&mut self) {
        self.order = (0..self.items.len()).collect();
    }

    /// The sort buttons: by name (0), date modified (1) or size (2); a dragged order is gone.
    pub fn sort_order(&mut self, by: i32) {
        self.reset_order();
        let items = &self.items;
        match by {
            0 => self.order.sort_by(|&a, &b| gezik_core::sort::natural_cmp(&items[a].name, &items[b].name)),
            1 => self
                .order
                .sort_by_key(|&i| items[i].modified.map(|d| (d.year, d.month, d.day, d.hour, d.minute, d.second))),
            2 => self.order.sort_by_key(|&i| items[i].size),
            _ => {}
        }
    }

    pub fn changed(&self) -> usize {
        self.statuses.iter().filter(|s| **s == Status::Changed).count()
    }

    pub fn blocked(&self) -> usize {
        self.statuses.iter().filter(|s| s.blocks()).count()
    }

    pub fn can_rename(&self) -> bool {
        !self.waiting
            && !self.reading
            && self.changed() > 0
            && self.blocked() == 0
            && self.rule_errors.iter().all(Option::is_none)
    }

    /// "1 duplicate name · 22 will change".
    pub fn footer(&self) -> String {
        let count = |want: fn(&Status) -> bool| self.statuses.iter().filter(|s| want(s)).count();
        let mut parts = Vec::new();
        if self.reading {
            parts.push("reading the folders…".to_owned());
        }
        if self.waiting {
            parts.push("reading photo dates…".to_owned());
        }
        let plural = |n: usize, one: &str, many: &str| if n == 1 { format!("1 {one}") } else { format!("{n} {many}") };
        let duplicates = count(|s| *s == Status::Duplicate);
        if duplicates > 0 {
            parts.push(plural(duplicates, "duplicate name", "duplicate names"));
        }
        let exists = count(|s| *s == Status::Exists);
        if exists > 0 {
            parts.push(plural(exists, "name already in the folder", "names already in the folder"));
        }
        let invalid = count(|s| matches!(s, Status::Invalid(_)));
        if invalid > 0 {
            parts.push(plural(invalid, "invalid name", "invalid names"));
        }
        if self.rule_errors.iter().any(Option::is_some) {
            parts.push("a rule has an error".to_owned());
        }
        parts.push(format!("{} will change", self.changed()));
        if !parts.is_empty() && self.blocked() > 0 {
            parts.push("fix the marked rows".to_owned());
        }
        parts.join(" · ")
    }

    /// The pairs to rename: only those that change.
    pub fn pairs(&self) -> Vec<(PathBuf, PathBuf)> {
        self.order
            .iter()
            .enumerate()
            .filter(|(position, _)| self.statuses.get(*position) == Some(&Status::Changed))
            .map(|(position, &i)| (self.paths[i].clone(), self.paths[i].with_file_name(&self.new[position])))
            .collect()
    }

    pub fn add_rule(&mut self, kind: &str) {
        if let Some(rule) = Rule::default_of(kind) {
            self.rules.push(RuleEntry::new(rule));
            self.selected_rule = Some(self.rules.len() - 1);
        }
    }

    pub fn move_rule(&mut self, by: i32) {
        let Some(at) = self.selected_rule else { return };
        let to = at as i64 + i64::from(by);
        if to < 0 || to as usize >= self.rules.len() {
            return;
        }
        self.rules.swap(at, to as usize);
        self.selected_rule = Some(to as usize);
    }

    pub fn remove_rule(&mut self) {
        let Some(at) = self.selected_rule else { return };
        if at < self.rules.len() {
            self.rules.remove(at);
        }
        self.selected_rule = if self.rules.is_empty() { None } else { Some(at.min(self.rules.len() - 1)) };
    }

    /// A text option of the selected rule changed (`key` as in `RuleOptions`).
    pub fn set_text(&mut self, key: &str, value: &str) {
        let Some(rule) = self.selected_rule.and_then(|i| self.rules.get_mut(i)) else { return };
        let number = |text: &str, old: i64| text.trim().parse::<i64>().unwrap_or(old);
        match (&mut rule.rule, key) {
            (Rule::Replace(r), "find") => r.find = value.to_owned(),
            (Rule::Replace(r), "with") => r.with = value.to_owned(),
            (Rule::Number(n), "start") => n.start = number(value, n.start),
            (Rule::Number(n), "step") => n.step = number(value, n.step),
            (Rule::Number(n), "digits") => n.digits = number(value, i64::from(n.digits)).clamp(1, 9) as u8,
            (Rule::Number(n), "separator") => n.separator = value.to_owned(),
            (Rule::AddText { prefix, .. }, "prefix") => *prefix = value.to_owned(),
            (Rule::AddText { suffix, .. }, "suffix") => *suffix = value.to_owned(),
            (Rule::Template(text), "text") => *text = value.to_owned(),
            (Rule::Extension(ExtensionRule::Set(ext)), "text") => *ext = value.to_owned(),
            _ => {}
        }
    }

    pub fn set_flag(&mut self, key: &str, value: bool) {
        let Some(rule) = self.selected_rule.and_then(|i| self.rules.get_mut(i)) else { return };
        match (&mut rule.rule, key) {
            (Rule::Replace(r), "regex") => r.regex = value,
            (Rule::Replace(r), "case-sensitive") => r.case_sensitive = value,
            (Rule::Replace(r), "all") => r.all = value,
            (Rule::Number(n), "at-start") => n.at = if value { NumberAt::Start } else { NumberAt::End },
            (Rule::Number(n), "per-folder") => n.per_folder = value,
            (Rule::Clean(c), "trim") => c.trim = value,
            (Rule::Clean(c), "collapse") => c.collapse_spaces = value,
            (Rule::Clean(c), "separators") => c.separators_to_spaces = value,
            (Rule::Clean(c), "underscores") => c.spaces_to_underscores = value,
            _ => {}
        }
    }

    pub fn set_mode(&mut self, mode: i32) {
        let Some(rule) = self.selected_rule.and_then(|i| self.rules.get_mut(i)) else { return };
        match &mut rule.rule {
            Rule::Case(case) => {
                if let Some(m) = usize::try_from(mode).ok().and_then(|m| CaseMode::ALL.get(m)) {
                    *case = *m;
                }
            }
            Rule::Extension(ext) => {
                *ext = match mode {
                    1 => ExtensionRule::Lower,
                    2 => ExtensionRule::Upper,
                    _ => ExtensionRule::Set(match ext {
                        ExtensionRule::Set(text) => text.clone(),
                        _ => String::new(),
                    }),
                }
            }
            _ => {}
        }
    }

    pub fn options(&self) -> RuleOptions {
        let mut o = RuleOptions::default();
        let Some(entry) = self.selected_rule.and_then(|i| self.rules.get(i)) else { return o };
        o.kind = entry.rule.kind_name().into();
        match &entry.rule {
            Rule::Replace(r) => {
                o.regex = r.regex;
                o.case_sensitive = r.case_sensitive;
                o.all = r.all;
            }
            Rule::Number(n) => {
                o.at_start = n.at == NumberAt::Start;
                o.per_folder = n.per_folder;
            }
            Rule::Case(mode) => o.mode = CaseMode::ALL.iter().position(|m| m == mode).unwrap_or(0) as i32,
            Rule::Extension(ExtensionRule::Set(_)) => o.mode = 0,
            Rule::Extension(ExtensionRule::Lower) => o.mode = 1,
            Rule::Extension(ExtensionRule::Upper) => o.mode = 2,
            Rule::Clean(c) => {
                o.trim = c.trim;
                o.collapse = c.collapse_spaces;
                o.separators = c.separators_to_spaces;
                o.underscores = c.spaces_to_underscores;
            }
            Rule::AddText { .. } | Rule::Template(_) => {}
        }
        o
    }

    /// The selected rule's texts, by `opt-*` key (find, with, start, step, digits, separator,
    /// prefix, suffix, text); keys the rule does not have are empty.
    pub fn option_texts(&self) -> Vec<(&'static str, String)> {
        let mut texts: Vec<(&'static str, String)> =
            ["find", "with", "start", "step", "digits", "separator", "prefix", "suffix", "text"]
                .into_iter()
                .map(|k| (k, String::new()))
                .collect();
        let mut set = |key: &str, value: String| {
            if let Some(slot) = texts.iter_mut().find(|(k, _)| *k == key) {
                slot.1 = value;
            }
        };
        match self.selected_rule.and_then(|i| self.rules.get(i)).map(|e| &e.rule) {
            Some(Rule::Replace(r)) => {
                set("find", r.find.clone());
                set("with", r.with.clone());
            }
            Some(Rule::Number(n)) => {
                set("start", n.start.to_string());
                set("step", n.step.to_string());
                set("digits", n.digits.to_string());
                set("separator", n.separator.clone());
            }
            Some(Rule::AddText { prefix, suffix }) => {
                set("prefix", prefix.clone());
                set("suffix", suffix.clone());
            }
            Some(Rule::Template(text)) | Some(Rule::Extension(ExtensionRule::Set(text))) => set("text", text.clone()),
            _ => {}
        }
        texts
    }

    pub fn rule_rows(&self) -> Vec<RuleRow> {
        self.rules
            .iter()
            .enumerate()
            .map(|(i, entry)| RuleRow {
                summary: entry.rule.summary().into(),
                enabled: entry.enabled,
                error: self.rule_errors.get(i).cloned().flatten().unwrap_or_default().into(),
                selected: self.selected_rule == Some(i),
            })
            .collect()
    }

    pub fn preview_row(&self, position: usize, selected: bool) -> Option<RenamePreviewRow> {
        let i = *self.order.get(position)?;
        let status = self.statuses.get(position)?;
        let (code, note) = match status {
            Status::Unchanged => (0, String::new()),
            Status::Changed => (1, String::new()),
            Status::Duplicate => (2, "Another item gets this name".to_owned()),
            Status::Exists => (3, "Something in the folder already has this name".to_owned()),
            Status::Invalid(err) => (4, err.to_string()),
        };
        Some(RenamePreviewRow {
            old_name: self.items[i].name.clone().into(),
            new_name: self.new.get(position)?.clone().into(),
            status: code,
            note: note.into(),
            manual: self.manual.contains_key(&i),
            selected,
        })
    }
}

struct Inner {
    window: slint::Weak<AppWindow>,
    ops: crate::operations::Operations,
    rules: RefCell<Rules>,
    rows: Rc<VecModel<RenamePreviewRow>>,
    /// Over display positions (shown rows when "Only changed").
    selection: RefCell<Selection>,
    /// Display positions on screen ("Only changed" hides some).
    shown: RefCell<Vec<usize>>,
    only_changed: Cell<bool>,
    open: Cell<bool>,
    timer: slint::Timer,
    /// The rule (and rule count) whose texts the fields show.
    shown_rule: Cell<Option<(usize, usize)>>,
    /// Load the texts on the next `show_rules` (a preset or a mode replaced the rule).
    reload_texts: Cell<bool>,
    /// The photo dates are being read (or were) for this opening.
    reading: Cell<bool>,
    taken_ready: Cell<bool>,
    /// Counts openings and closings: dates read for an earlier opening are dropped, and the
    /// reader stops once it changes.
    opening: Arc<AtomicU64>,
    /// The display position whose name the name field shows.
    name_shown: Cell<Option<usize>>,
}

#[derive(Clone)]
pub struct BatchRename(Rc<Inner>);

impl BatchRename {
    pub fn new(window: &AppWindow, ops: crate::operations::Operations) -> BatchRename {
        let rows = Rc::new(VecModel::default());
        window.set_rb_rows(ModelRc::from(rows.clone()));
        let this = BatchRename(Rc::new(Inner {
            window: window.as_weak(),
            ops,
            rules: RefCell::default(),
            rows,
            selection: RefCell::new(Selection::new(0)),
            shown: RefCell::default(),
            only_changed: Default::default(),
            open: Default::default(),
            timer: slint::Timer::default(),
            shown_rule: Default::default(),
            reload_texts: Default::default(),
            reading: Default::default(),
            taken_ready: Default::default(),
            opening: Default::default(),
            name_shown: Default::default(),
        }));
        this.install(window);
        CURRENT.with(|c| *c.borrow_mut() = Some(this.clone()));
        this
    }

    fn install(&self, window: &AppWindow) {
        let t = self.clone();
        window.on_rb_select_rule(move |i| t.edit(|r| r.selected_rule = usize::try_from(i).ok(), false));
        let t = self.clone();
        window.on_rb_toggle_rule(move |i| {
            t.edit(
                |r| {
                    if let Some(rule) = usize::try_from(i).ok().and_then(|i| r.rules.get_mut(i)) {
                        rule.enabled = !rule.enabled;
                    }
                },
                true,
            )
        });
        let t = self.clone();
        window.on_rb_move_rule(move |by| t.edit(|r| r.move_rule(by), true));
        let t = self.clone();
        window.on_rb_remove_rule(move || t.edit(Rules::remove_rule, true));
        let t = self.clone();
        window.on_rb_option_text(move |key, value| t.edit(|r| r.set_text(&key, &value), true));
        let t = self.clone();
        window.on_rb_option_flag(move |key, value| t.edit(|r| r.set_flag(&key, value), true));
        let t = self.clone();
        window.on_rb_option_mode(move |mode| {
            t.0.reload_texts.set(true);
            t.edit(|r| r.set_mode(mode), true)
        });
        let t = self.clone();
        window.on_rb_toggle_extension(move || t.edit(|r| r.include_extension = !r.include_extension, true));
        let t = self.clone();
        window.on_rb_toggle_only_changed(move || {
            t.0.only_changed.set(!t.0.only_changed.get());
            t.refresh();
            t.show_name();
        });
        let t = self.clone();
        window.on_rb_row_pressed(move |row, ctrl, shift| {
            if let Ok(row) = usize::try_from(row) {
                t.pressed(row, ctrl, shift);
            }
        });
        let t = self.clone();
        window.on_rb_rename(move || t.rename());
        let t = self.clone();
        window.on_rb_cancel(move || t.close());
        let t = self.clone();
        window.on_rb_key(move |event| {
            let m = event.modifiers;
            t.key(&event.text, m.control, m.alt, m.shift, m.meta)
        });
        let t = self.clone();
        window.on_rb_reorder(move |from, to| {
            if let (Ok(from), Ok(to)) = (usize::try_from(from), usize::try_from(to)) {
                t.reorder(from, to);
            }
        });
        window
            .on_rb_drop_row(|y, row_height, count| drop_row(y, row_height, usize::try_from(count).unwrap_or(0)) as i32);
        let t = self.clone();
        window.on_rb_sort(move |by| t.sort(by));
        let t = self.clone();
        window.on_rb_name_edited(move |text| {
            let Some(position) = t.focused_position() else { return };
            t.0.rules.borrow_mut().set_manual(position, &text);
            // The field keeps what is typed: only the preview follows.
            let t2 = t.clone();
            t.0.timer.start(slint::TimerMode::SingleShot, Duration::from_millis(50), move || t2.update());
        });
        let t = self.clone();
        window.on_rb_name_reset(move || {
            let positions = t.selected_positions();
            t.0.rules.borrow_mut().reset_manual(&positions);
            t.update();
            t.show_name();
        });
        // Add rule ▾ and Presets ▾ open Slint menus through context_menu.rs (main.rs wires them).
    }

    pub fn is_open(&self) -> bool {
        self.0.open.get()
    }

    /// Opens the layer for `items` (paths and entries), in list order. `others`: the folder's
    /// other names; `None` for search results, whose folders are read in the background (spec
    /// 4.6: Rename waits for them). `last`: the rules used last time.
    pub fn open(&self, items: Vec<(PathBuf, Entry)>, others: Option<Vec<String>>, last: BatchRenameState) {
        let Some(window) = self.0.window.upgrade() else { return };
        let selected: HashSet<String> = items.iter().map(|(_, e)| e.name.clone()).collect();
        let lang = Lang::from_code(&gezik_platform::language());
        // Items are counted and checked per folder (search results come from many).
        let mut folders: Vec<PathBuf> = Vec::new();
        let mut folder_of = |path: &std::path::Path| {
            let parent = path.parent().map(std::path::Path::to_path_buf).unwrap_or_default();
            match folders.iter().position(|f| *f == parent) {
                Some(i) => i,
                None => {
                    folders.push(parent);
                    folders.len() - 1
                }
            }
        };
        // From the listing: the UI thread does not ask the file system.
        let list: Vec<Item> = items
            .iter()
            .map(|(path, entry)| Item {
                name: entry.name.clone(),
                is_dir: entry.is_dir,
                parent: path
                    .parent()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                folder: folder_of(path),
                size: if entry.is_dir { 0 } else { entry.size },
                modified: entry.modified.and_then(gezik_platform::local_date_parts),
                taken: None,
            })
            .collect();
        {
            let mut r = self.0.rules.borrow_mut();
            *r = Rules {
                order: (0..list.len()).collect(),
                paths: items.into_iter().map(|(p, _)| p).collect(),
                items: list,
                others: match &others {
                    Some(names) => vec![names.iter().filter(|n| !selected.contains(*n)).cloned().collect()],
                    None => vec![HashSet::new(); folders.len()],
                },
                reading: others.is_none(),
                include_extension: last.include_extension,
                selected_rule: if last.rules.is_empty() { None } else { Some(0) },
                rules: last.rules,
                lang,
                ..Rules::default()
            };
            if r.rules.is_empty() {
                r.add_rule("replace");
            }
        }
        let count = self.0.rules.borrow().items.len();
        window.set_rb_title(format!("Rename {count} items").into());
        window.set_rb_only_changed(false);
        self.0.only_changed.set(false);
        self.0.shown_rule.set(None);
        self.0.reload_texts.set(true);
        self.0.reading.set(false);
        self.0.taken_ready.set(false);
        self.0.opening.fetch_add(1, Ordering::Relaxed);
        *self.0.selection.borrow_mut() = Selection::new(count);
        self.0.shown.borrow_mut().clear();
        window.set_rb_scroll(0.0);
        self.0.open.set(true);
        if others.is_none() {
            self.read_folders(folders);
        }
        self.recompute();
        window.set_rb_open(true);
    }

    pub fn close(&self) {
        self.0.open.set(false);
        self.0.opening.fetch_add(1, Ordering::Relaxed);
        self.0.timer.stop();
        self.0.rows.clear();
        if let Some(window) = self.0.window.upgrade() {
            window.set_rb_open(false);
            if !window.get_dialog_open() && !window.get_conflicts_open() {
                window.invoke_focus_list();
            }
        }
    }

    /// What to keep for next time (state.toml).
    pub fn state(&self) -> BatchRenameState {
        let r = self.0.rules.borrow();
        BatchRenameState { include_extension: r.include_extension, rules: r.rules.clone() }
    }

    /// Changes the rules; `later`: the preview follows 50 ms after the last change (typing).
    fn edit(&self, f: impl FnOnce(&mut Rules), later: bool) {
        f(&mut self.0.rules.borrow_mut());
        self.show_rules();
        if later {
            let t = self.clone();
            self.0.timer.start(slint::TimerMode::SingleShot, Duration::from_millis(50), move || t.recompute());
        }
    }

    fn show_rules(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        let r = self.0.rules.borrow();
        window.set_rb_rules(ModelRc::new(VecModel::from(r.rule_rows())));
        window.set_rb_selected_rule(r.selected_rule.map_or(-1, |i| i as i32));
        window.set_rb_options(r.options());
        window.set_rb_include_extension(r.include_extension);
        // The text fields are loaded only when another rule shows: rewriting the one being
        // typed in would move its caret.
        let showing = r.selected_rule.map(|i| (i, r.rules.len()));
        if self.0.shown_rule.replace(showing) != showing || self.0.reload_texts.replace(false) {
            for (key, value) in r.option_texts() {
                let value = value.into();
                match key {
                    "find" => window.set_rb_opt_find(value),
                    "with" => window.set_rb_opt_with(value),
                    "start" => window.set_rb_opt_start(value),
                    "step" => window.set_rb_opt_step(value),
                    "digits" => window.set_rb_opt_digits(value),
                    "separator" => window.set_rb_opt_separator(value),
                    "prefix" => window.set_rb_opt_prefix(value),
                    "suffix" => window.set_rb_opt_suffix(value),
                    _ => window.set_rb_opt_text(value),
                }
            }
        }
    }

    /// New names for everything, the name field included unless it is being typed in.
    fn recompute(&self) {
        self.update();
        self.follow_name();
    }

    /// New names, rules and preview; the name field is left as typed.
    fn update(&self) {
        let needs_taken = {
            let mut r = self.0.rules.borrow_mut();
            r.recompute();
            r.waiting = r.needs_taken && !self.0.taken_ready.get();
            r.needs_taken
        };
        if needs_taken && self.0.open.get() {
            self.read_taken();
        }
        self.show_rules();
        self.refresh();
    }

    fn refresh(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        let r = self.0.rules.borrow();
        let only_changed = self.0.only_changed.get();
        let shown: Vec<usize> = (0..r.order.len())
            .filter(|&p| !only_changed || r.statuses.get(p).is_some_and(|s| *s != Status::Unchanged))
            .collect();
        let mut selection = self.0.selection.borrow_mut();
        // Other rows show ("Only changed"): row numbers mean other items now.
        if *self.0.shown.borrow() != shown {
            *selection = Selection::new(shown.len());
        }
        let rows: Vec<RenamePreviewRow> =
            shown.iter().enumerate().filter_map(|(row, &p)| r.preview_row(p, selection.is_selected(row))).collect();
        update_rows(&self.0.rows, rows);
        *self.0.shown.borrow_mut() = shown;
        window.set_rb_footer(r.footer().into());
        window.set_rb_can_rename(r.can_rename());
        window.set_rb_only_changed(only_changed);
    }

    /// Reads the names in each of `folders` on another thread (search results, spec 4.6).
    fn read_folders(&self, folders: Vec<PathBuf>) {
        let counter = self.0.opening.clone();
        let opening = counter.load(Ordering::Relaxed);
        let weak = self.0.window.clone();
        let spawned = std::thread::Builder::new().name("gezik-rename-folders".into()).spawn(move || {
            let names: Vec<HashSet<String>> = folders
                .iter()
                .map(|folder| {
                    std::fs::read_dir(folder)
                        .map(|entries| {
                            entries
                                .filter_map(Result::ok)
                                .map(|e| e.file_name().to_string_lossy().into_owned())
                                .collect()
                        })
                        .unwrap_or_default()
                })
                .collect();
            if counter.load(Ordering::Relaxed) != opening {
                return;
            }
            let _ = weak.upgrade_in_event_loop(move |_| {
                with_current(|layer| {
                    if layer.0.opening.load(Ordering::Relaxed) != opening || !layer.is_open() {
                        return;
                    }
                    {
                        let mut r = layer.0.rules.borrow_mut();
                        let selected: Vec<(usize, String)> =
                            r.items.iter().map(|item| (item.folder, item.name.clone())).collect();
                        let mut names = names;
                        for (folder, name) in selected {
                            if let Some(set) = names.get_mut(folder) {
                                set.remove(&name);
                            }
                        }
                        r.others = names;
                        r.reading = false;
                    }
                    layer.recompute();
                });
            });
        });
        if spawned.is_err() {
            // The names on disk are not known: Rename goes ahead, the job says what clashes.
            self.0.rules.borrow_mut().reading = false;
        }
    }

    /// Reads the photo dates on another thread, once per opening; `{taken}` falls back to
    /// the modified date (and Rename waits) until they are in.
    fn read_taken(&self) {
        if self.0.reading.replace(true) {
            return;
        }
        let paths: Vec<(usize, PathBuf)> = {
            let r = self.0.rules.borrow();
            r.paths
                .iter()
                .enumerate()
                .filter(|&(i, p)| {
                    !r.items[i].is_dir
                        && p.file_name().is_some_and(|n| gezik_batch::exif::may_have_exif(&n.to_string_lossy()))
                })
                .map(|(i, p)| (i, p.clone()))
                .collect()
        };
        let counter = self.0.opening.clone();
        let opening = counter.load(Ordering::Relaxed);
        let weak = self.0.window.clone();
        std::thread::spawn(move || {
            let mut dates: Vec<(usize, DateParts)> = Vec::new();
            for (i, path) in paths {
                // Closed or opened again: these dates are not wanted any more.
                if counter.load(Ordering::Relaxed) != opening {
                    return;
                }
                dates.extend(gezik_batch::exif::taken(&path).map(|d| (i, d)));
            }
            let _ = weak.upgrade_in_event_loop(move |_| {
                with_current(|layer| {
                    if layer.0.opening.load(Ordering::Relaxed) != opening || !layer.is_open() {
                        return;
                    }
                    {
                        let mut r = layer.0.rules.borrow_mut();
                        for (i, date) in dates {
                            if let Some(item) = r.items.get_mut(i) {
                                item.taken = Some(date);
                            }
                        }
                    }
                    layer.0.taken_ready.set(true);
                    layer.recompute();
                });
            });
        });
    }

    fn pressed(&self, row: usize, ctrl: bool, shift: bool) {
        {
            let mut selection = self.0.selection.borrow_mut();
            match (ctrl, shift) {
                (_, true) => selection.extend_to(row, ctrl),
                (true, false) => selection.toggle(row),
                (false, false) => selection.select_only(row),
            };
        }
        self.refresh();
        self.show_name();
    }

    /// The row dragged by its handle from `from` to `to` (shown rows); it stays selected.
    fn reorder(&self, from: usize, to: usize) {
        let positions = {
            let shown = self.0.shown.borrow();
            shown.get(from).copied().zip(shown.get(to).copied())
        };
        let Some((from, to_position)) = positions else { return };
        self.0.rules.borrow_mut().reorder(from, to_position);
        self.0.selection.borrow_mut().select_only(to);
        self.recompute();
    }

    /// The sort buttons; the selected and focused items stay so (on their new rows).
    fn sort(&self, by: i32) {
        let (selected, focus) = {
            let r = self.0.rules.borrow();
            let shown = self.0.shown.borrow();
            let selection = self.0.selection.borrow();
            let item = |row: usize| shown.get(row).and_then(|&p| r.order.get(p)).copied();
            (selection.iter().filter_map(item).collect::<Vec<_>>(), selection.focus().and_then(item))
        };
        self.0.rules.borrow_mut().sort_order(by);
        self.update();
        let (rows, focus) = {
            let r = self.0.rules.borrow();
            rows_of(&selected, focus, &r.order, &self.0.shown.borrow())
        };
        {
            let mut selection = self.0.selection.borrow_mut();
            *selection = Selection::new(self.0.shown.borrow().len());
            for row in rows {
                selection.toggle(row);
            }
            if let Some(row) = focus {
                selection.set_focus(row);
            }
        }
        self.refresh();
        self.show_name();
    }

    /// Display positions of the selected rows (the focused one if none).
    fn selected_positions(&self) -> Vec<usize> {
        let shown = self.0.shown.borrow();
        let selection = self.0.selection.borrow();
        let mut rows: Vec<usize> = selection.iter().collect();
        if rows.is_empty() {
            rows.extend(selection.focus());
        }
        rows.into_iter().filter_map(|row| shown.get(row).copied()).collect()
    }

    fn focused_position(&self) -> Option<usize> {
        let row = self.0.selection.borrow().focus()?;
        self.0.shown.borrow().get(row).copied()
    }

    /// The name field follows the names, but not while it is typed in for the row it shows.
    fn follow_name(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        if window.get_rb_name_focused() && self.focused_position() == self.0.name_shown.get() {
            return;
        }
        self.show_name();
    }

    /// The name field shows the focused row's new name.
    fn show_name(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        let r = self.0.rules.borrow();
        let position = self.focused_position();
        self.0.name_shown.set(position);
        match position.and_then(|p| r.new.get(p)) {
            Some(name) => {
                window.set_rb_name_text(name.clone().into());
                window.set_rb_name_enabled(true);
            }
            None => {
                window.set_rb_name_text("".into());
                window.set_rb_name_enabled(false);
            }
        }
    }

    fn rename(&self) {
        let pairs = {
            let mut r = self.0.rules.borrow_mut();
            r.recompute();
            if !r.can_rename() {
                return;
            }
            r.pairs()
        };
        let state = self.state();
        self.close();
        crate::operations::with_current(|ops| ops.save_batch_rename(state));
        self.0.ops.remember_for(&pairs.iter().map(|(from, _)| from.clone()).collect::<Vec<_>>());
        self.0.ops.submit(Box::new(gezik_ops::RenameTask::many(pairs)), None, crate::operations::After::Select);
    }

    /// A key that reached the layer's own focus scope (the preview, not a text field);
    /// returns whether it was used.
    fn key(&self, text: &str, control: bool, alt: bool, shift: bool, meta: bool) -> bool {
        let platform = Platform::current();
        crate::keys::chord_from_slint(text, control, alt, shift, meta, platform).is_some_and(|c| self.act(&c, true))
    }

    /// A key while the layer is open, wherever its focus is (main.rs sends every key here
    /// first, so Esc and Ctrl+Enter work in the option fields too); returns whether it was used.
    pub fn chord(&self, chord: &Chord) -> bool {
        self.act(chord, false)
    }

    fn act(&self, chord: &Chord, in_list: bool) -> bool {
        let Some(action) = key_action(chord, Platform::current(), in_list) else { return false };
        match action {
            KeyAction::Close => self.close(),
            KeyAction::Rename => self.rename(),
            KeyAction::FocusName => {
                if let Some(window) = self.0.window.upgrade() {
                    window.invoke_rb_focus_name();
                }
            }
            KeyAction::SelectAll => {
                self.0.selection.borrow_mut().select_all();
                self.refresh();
                self.show_name();
            }
            KeyAction::Move(key) => {
                self.move_focus(key, chord.shift, crate::keys::is_primary(chord, Platform::current()))
            }
        }
        true
    }

    /// Arrows, PgUp/PgDn, Home/End in the preview: Shift extends, the primary key only moves
    /// the focus.
    fn move_focus(&self, key: Key, shift: bool, primary: bool) {
        let target = {
            let len = self.0.shown.borrow().len();
            if len == 0 {
                return;
            }
            let mut selection = self.0.selection.borrow_mut();
            let at = selection.focus().unwrap_or(0);
            let target = match key {
                Key::Up => at.saturating_sub(1),
                Key::Down => (at + 1).min(len - 1),
                Key::PageUp => at.saturating_sub(PAGE),
                Key::PageDown => (at + PAGE).min(len - 1),
                Key::Home => 0,
                _ => len - 1,
            };
            if shift {
                selection.extend_to(target, primary);
            } else if primary {
                selection.set_focus(target);
            } else {
                selection.select_only(target);
            }
            target
        };
        self.refresh();
        self.show_name();
        if let Some(window) = self.0.window.upgrade() {
            window.invoke_rb_ensure_visible(i32::try_from(target).unwrap_or(0));
        }
    }

    pub fn add_rule(&self, kind: &str) {
        self.edit(|r| r.add_rule(kind), true);
    }

    /// The saved set called `name`, if it is still there.
    pub fn apply_preset(&self, name: &str) {
        let Some(preset) = PRESETS.with(|p| p.borrow().iter().find(|p| p.name == name).cloned()) else { return };
        self.0.reload_texts.set(true);
        self.edit(
            |r| {
                r.rules = preset.rules;
                r.include_extension = preset.include_extension;
                r.selected_rule = if r.rules.is_empty() { None } else { Some(0) };
            },
            true,
        );
    }

    /// Asks for a name over the layer, then saves the current rules under it.
    pub fn ask_preset_name(&self) {
        let t = self.clone();
        self.0.ops.ask_text("Save rules as", "Name for this set of rules:", move |name| t.save_preset(name));
    }

    fn save_preset(&self, name: String) {
        let name = name.trim().to_owned();
        if name.is_empty() {
            return;
        }
        let state = self.state();
        let mut presets = PRESETS.with(|p| p.borrow().clone());
        let preset =
            RenamePreset { name: name.clone(), include_extension: state.include_extension, rules: state.rules };
        match presets.iter_mut().find(|p| p.name == name) {
            Some(old) => *old = preset,
            None => presets.push(preset),
        }
        self.write_presets(presets);
    }

    pub fn delete_preset(&self, name: &str) {
        let mut presets = PRESETS.with(|p| p.borrow().clone());
        if let Some(index) = presets.iter().position(|p| p.name == name) {
            presets.remove(index);
            self.write_presets(presets);
        }
    }

    fn write_presets(&self, presets: Vec<RenamePreset>) {
        set_presets(presets.clone());
        self.0.ops.save_rename_presets(&presets);
    }
}

/// The shown rows of `items` (and of the `focus` item) under `order`; hidden ones are left out.
pub fn rows_of(items: &[usize], focus: Option<usize>, order: &[usize], shown: &[usize]) -> (Vec<usize>, Option<usize>) {
    let row = |item: usize| order.iter().position(|&i| i == item).and_then(|p| shown.iter().position(|&s| s == p));
    (items.iter().filter_map(|&i| row(i)).collect(), focus.and_then(row))
}

/// How many rows PgUp/PgDn move in the preview.
const PAGE: usize = 10;

/// Puts `rows` into `model`, changing only the rows that differ while the count stays: the
/// rows' items (and a handle held for a drag) are not rebuilt under the pointer.
pub fn update_rows<T: Clone + PartialEq + 'static>(model: &VecModel<T>, rows: Vec<T>) {
    if model.row_count() != rows.len() {
        model.set_vec(rows);
        return;
    }
    for (i, row) in rows.into_iter().enumerate() {
        if model.row_data(i).as_ref() != Some(&row) {
            model.set_row_data(i, row);
        }
    }
}

/// The preview row under `y` (in the list's content), for a drag: clamped to the rows.
pub fn drop_row(y: f32, row_height: f32, count: usize) -> usize {
    if count == 0 || row_height <= 0.0 || y <= 0.0 {
        return 0;
    }
    ((y / row_height) as usize).min(count - 1)
}

/// What a key does in the layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    Close,
    Rename,
    FocusName,
    SelectAll,
    /// Moves the preview's selection: an arrow, PgUp/PgDn, Home or End.
    Move(Key),
}

/// Esc closes, Ctrl+Enter (Cmd on macOS) renames and F2 goes to the name field wherever the
/// focus is; `in_list` (the preview has the keyboard, not a text field): the arrows,
/// PgUp/PgDn, Home/End and Ctrl+A act on the preview. Other keys are for the fields.
pub fn key_action(chord: &Chord, platform: Platform, in_list: bool) -> Option<KeyAction> {
    let primary = crate::keys::is_primary(chord, platform);
    match chord.key {
        Key::Escape if !primary && !chord.shift => Some(KeyAction::Close),
        Key::Enter if primary => Some(KeyAction::Rename),
        Key::F(2) if !primary && !chord.alt && !chord.shift => Some(KeyAction::FocusName),
        Key::Char('a') if primary && in_list => Some(KeyAction::SelectAll),
        Key::Up | Key::Down | Key::PageUp | Key::PageDown | Key::Home | Key::End if in_list && !chord.alt => {
            Some(KeyAction::Move(chord.key))
        }
        _ => None,
    }
}

thread_local! {
    static CURRENT: RefCell<Option<BatchRename>> = const { RefCell::new(None) };
    static PRESETS: RefCell<Vec<RenamePreset>> = const { RefCell::new(Vec::new()) };
}

/// settings.toml changed: the saved sets.
pub fn set_presets(presets: Vec<RenamePreset>) {
    PRESETS.with(|p| *p.borrow_mut() = presets);
}

pub fn preset_names() -> Vec<String> {
    PRESETS.with(|p| p.borrow().iter().map(|p| p.name.clone()).collect())
}

/// Runs `f` with this UI thread's batch rename layer, if set up.
pub fn with_current(f: impl FnOnce(&BatchRename)) {
    if let Some(layer) = CURRENT.with(|c| c.borrow().clone()) {
        f(&layer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(names: &[&str], others: &[&str]) -> Rules {
        let items: Vec<Item> = names.iter().map(|n| Item { name: n.to_string(), ..Item::default() }).collect();
        let paths = names.iter().map(|n| PathBuf::from("/d").join(n)).collect();
        Rules {
            order: (0..items.len()).collect(),
            items,
            paths,
            others: vec![others.iter().map(|s| s.to_string()).collect()],
            ..Rules::default()
        }
    }

    #[test]
    fn names_taken_count_in_each_items_own_folder() {
        let mut m = model(&["a.txt", "c.txt"], &[]);
        m.items[1].folder = 1;
        m.others = vec![Default::default(), ["b.txt".to_owned()].into()];
        m.manual.insert(0, "b.txt".to_owned());
        m.manual.insert(1, "b.txt".to_owned());
        m.recompute();
        assert_eq!(m.statuses[0], Status::Changed, "b.txt is taken in the other folder only");
        assert_eq!(m.statuses[1], Status::Exists);
        m.reading = true;
        assert!(!m.can_rename(), "the folders are still being read");
        assert!(m.footer().starts_with("reading the folders"), "{}", m.footer());
    }

    #[test]
    fn rules_are_added_edited_moved_and_removed() {
        let mut m = model(&["IMG_1.jpg", "IMG_2.jpg"], &[]);
        m.add_rule("replace");
        m.set_text("find", "IMG_");
        m.set_text("with", "Tatil ");
        m.add_rule("case");
        m.set_mode(1);
        m.recompute();
        assert_eq!(m.new, ["TATIL 1.jpg", "TATIL 2.jpg"]);
        m.move_rule(-1);
        assert_eq!(m.selected_rule, Some(0));
        m.remove_rule();
        m.recompute();
        assert_eq!(m.new, ["Tatil 1.jpg", "Tatil 2.jpg"]);
        assert_eq!(m.pairs().len(), 2);
        assert!(m.can_rename());
    }

    #[test]
    fn warnings_block_and_say_why() {
        let mut m = model(&["a.txt", "b.txt"], &["x.txt"]);
        m.add_rule("template");
        m.set_text("text", "x");
        m.recompute();
        assert!(!m.can_rename());
        assert!(m.footer().starts_with("2 duplicate names"), "{}", m.footer());
        m.manual.insert(1, "y.txt".into());
        m.recompute();
        assert_eq!(m.statuses, [Status::Exists, Status::Changed]);
        assert!(m.footer().contains("1 name already in the folder"), "{}", m.footer());
    }

    #[test]
    fn a_bad_rule_blocks() {
        let mut m = model(&["a.txt"], &[]);
        m.add_rule("replace");
        m.set_flag("regex", true);
        m.set_text("find", "(");
        m.recompute();
        assert!(!m.can_rename());
        assert!(!m.rule_rows()[0].error.is_empty());
    }

    #[test]
    fn esc_closes_and_primary_enter_renames() {
        let chord = |key: Key, ctrl: bool, shift: bool, meta: bool| Chord { ctrl, alt: false, shift, meta, key };
        let other = Platform::Other;
        assert_eq!(key_action(&chord(Key::Escape, false, false, false), other, false), Some(KeyAction::Close));
        assert_eq!(key_action(&chord(Key::Escape, false, true, false), other, false), None);
        assert_eq!(key_action(&chord(Key::Escape, true, false, false), other, false), None);
        assert_eq!(key_action(&chord(Key::Enter, true, false, false), other, false), Some(KeyAction::Rename));
        let plain_enter = key_action(&chord(Key::Enter, false, false, false), other, true);
        assert_eq!(plain_enter, None, "plain Enter is the field's");
        assert_eq!(key_action(&chord(Key::Char('a'), false, false, false), other, true), None);
        assert_eq!(key_action(&chord(Key::Enter, false, false, true), Platform::Mac, false), Some(KeyAction::Rename));
        assert_eq!(key_action(&chord(Key::Enter, true, false, false), Platform::Mac, false), None);
    }

    #[test]
    fn list_keys_act_only_on_the_preview() {
        let chord = |key: Key, ctrl: bool, shift: bool| Chord { ctrl, alt: false, shift, meta: false, key };
        let other = Platform::Other;
        assert_eq!(key_action(&chord(Key::F(2), false, false), other, false), Some(KeyAction::FocusName));
        assert_eq!(key_action(&chord(Key::F(2), false, false), other, true), Some(KeyAction::FocusName));
        for key in [Key::Up, Key::Down, Key::PageUp, Key::PageDown, Key::Home, Key::End] {
            assert_eq!(key_action(&chord(key, false, false), other, true), Some(KeyAction::Move(key)));
            assert_eq!(key_action(&chord(key, false, true), other, true), Some(KeyAction::Move(key)));
            assert_eq!(key_action(&chord(key, false, false), other, false), None, "{key:?} in a text field");
        }
        assert_eq!(key_action(&chord(Key::Char('a'), true, false), other, true), Some(KeyAction::SelectAll));
        assert_eq!(key_action(&chord(Key::Char('a'), true, false), other, false), None, "Ctrl+A selects the text");
    }

    #[test]
    fn numbers_follow_a_dragged_order() {
        let mut m = model(&["a", "b", "c"], &[]);
        m.add_rule("number");
        m.set_text("digits", "1");
        m.set_text("separator", "");
        m.recompute();
        assert_eq!(m.new, ["a1", "b2", "c3"]);
        m.reorder(2, 0);
        m.recompute();
        assert_eq!(m.new, ["c1", "a2", "b3"]);
        assert_eq!(m.pairs()[0], (PathBuf::from("/d/c"), PathBuf::from("/d/c1")));
    }

    #[test]
    fn a_manual_name_wins_until_reset() {
        let mut m = model(&["a.txt", "b.txt"], &[]);
        m.add_rule("case");
        m.set_mode(1);
        m.recompute();
        m.set_manual(1, "Kapak.txt");
        m.recompute();
        assert_eq!(m.new, ["A.txt", "Kapak.txt"]);
        assert!(m.preview_row(1, false).unwrap().manual);
        m.set_manual(1, "B.txt");
        assert!(m.manual.is_empty(), "typing what the rules give is no manual name");
        m.set_manual(1, "x.txt");
        m.reset_manual(&[1]);
        m.recompute();
        assert_eq!(m.new[1], "B.txt");
    }

    #[test]
    fn waits_for_photo_dates() {
        let mut m = model(&["a.jpg"], &[]);
        m.add_rule("template");
        m.set_text("text", "{taken}");
        m.waiting = true;
        m.recompute();
        assert!(!m.can_rename());
        assert!(m.footer().starts_with("reading photo dates"), "{}", m.footer());
    }

    #[test]
    fn sort_buttons_replace_a_dragged_order() {
        let mut m = model(&["b10", "b9", "a"], &[]);
        m.items[0].size = 5;
        m.items[1].size = 1;
        m.items[2].size = 3;
        m.reorder(0, 2);
        m.sort_order(0);
        assert_eq!(m.order, [2, 1, 0], "natural order: b9 before b10");
        m.sort_order(2);
        assert_eq!(m.order, [1, 2, 0]);
        m.reset_order();
        assert_eq!(m.order, [0, 1, 2]);
    }

    #[test]
    fn sorting_keeps_the_selected_items() {
        // Items 2 and 0 selected, 0 focused; the new order puts them on positions 0 and 2.
        let (rows, focus) = rows_of(&[0, 2], Some(0), &[2, 1, 0], &[0, 1, 2]);
        assert_eq!((rows, focus), (vec![2, 0], Some(2)));
        // "Only changed" hides position 1 (item 1): row numbers skip it.
        let (rows, focus) = rows_of(&[1, 0], Some(1), &[2, 1, 0], &[0, 2]);
        assert_eq!((rows, focus), (vec![1], None));
    }

    #[test]
    fn rows_are_updated_in_place() {
        let model = VecModel::from(vec![1, 2, 3]);
        update_rows(&model, vec![1, 5, 3]);
        assert_eq!(model.iter().collect::<Vec<_>>(), [1, 5, 3]);
        update_rows(&model, vec![7]);
        assert_eq!(model.iter().collect::<Vec<_>>(), [7]);
    }

    #[test]
    fn a_drop_row_is_clamped() {
        assert_eq!(drop_row(-5.0, 24.0, 3), 0);
        assert_eq!(drop_row(0.0, 24.0, 3), 0);
        assert_eq!(drop_row(47.9, 24.0, 3), 1);
        assert_eq!(drop_row(48.0, 24.0, 3), 2);
        assert_eq!(drop_row(500.0, 24.0, 3), 2);
        assert_eq!(drop_row(10.0, 24.0, 0), 0);
        assert_eq!(drop_row(10.0, 0.0, 3), 0);
    }

    #[test]
    fn options_reflect_the_selected_rule() {
        let mut m = model(&["a"], &[]);
        m.add_rule("number");
        m.set_text("digits", "5");
        m.set_flag("at-start", true);
        let digits = |m: &Rules| m.option_texts().into_iter().find(|(k, _)| *k == "digits").unwrap().1;
        let o = m.options();
        assert_eq!((o.kind.as_str(), digits(&m).as_str(), o.at_start), ("number", "5", true));
        m.set_text("digits", "x");
        assert_eq!(digits(&m), "5", "a bad number keeps the old one");
    }
}
