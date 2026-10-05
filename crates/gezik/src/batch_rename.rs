//! The batch rename layer: rules on the left, the preview on the right, Rename runs one
//! `RenameTask`. The rules and their results live here; Slint only shows them.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gezik_batch::rename::{Item, Status, check, compile, new_names};
use gezik_config::settings::BatchRenameState;
use gezik_core::batch::case::{CaseMode, Lang};
use gezik_core::batch::rules::{ExtensionRule, NumberAt, Rule, RuleEntry};
use gezik_core::ops::names::NameRules;
use slint::{ComponentHandle, ModelRc, VecModel};

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
    /// Names in the folder that are not being renamed.
    pub others: HashSet<String>,
    pub lang: Lang,
    // Results, by display position.
    pub new: Vec<String>,
    pub statuses: Vec<Status>,
    pub rule_errors: Vec<Option<String>>,
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
        let others: HashSet<String> =
            self.others.iter().map(|n| if IGNORE_CASE { n.to_lowercase() } else { n.clone() }).collect();
        let existing =
            |_: usize, name: &str| others.contains(&if IGNORE_CASE { name.to_lowercase() } else { name.to_owned() });
        self.statuses = check(&items, &new, &existing, NameRules::current(), IGNORE_CASE);
        self.rule_errors = compiled.errors().to_vec();
        self.new = new;
    }

    pub fn changed(&self) -> usize {
        self.statuses.iter().filter(|s| **s == Status::Changed).count()
    }

    pub fn blocked(&self) -> usize {
        self.statuses.iter().filter(|s| s.blocks()).count()
    }

    pub fn can_rename(&self) -> bool {
        self.changed() > 0 && self.blocked() == 0 && self.rule_errors.iter().all(Option::is_none)
    }

    /// "1 duplicate name · 22 will change".
    pub fn footer(&self) -> String {
        let count = |want: fn(&Status) -> bool| self.statuses.iter().filter(|s| want(s)).count();
        let mut parts = Vec::new();
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
    selection: RefCell<gezik_core::selection::Selection>,
    /// Display positions on screen ("Only changed" hides some).
    shown: RefCell<Vec<usize>>,
    only_changed: std::cell::Cell<bool>,
    open: std::cell::Cell<bool>,
    timer: slint::Timer,
    /// The rule (and rule count) whose texts the fields show.
    shown_rule: std::cell::Cell<Option<(usize, usize)>>,
    /// Load the texts on the next `show_rules` (a preset or a mode replaced the rule).
    reload_texts: std::cell::Cell<bool>,
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
            selection: RefCell::new(gezik_core::selection::Selection::new(0)),
            shown: RefCell::default(),
            only_changed: Default::default(),
            open: Default::default(),
            timer: slint::Timer::default(),
            shown_rule: Default::default(),
            reload_texts: Default::default(),
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
        // Add rule ▾ and Presets ▾ open Slint menus through context_menu.rs (Task 8 for presets).
    }

    pub fn is_open(&self) -> bool {
        self.0.open.get()
    }

    /// Opens the layer for `items` (paths and whether each is a folder), in list order.
    /// `others`: the folder's other names; `last`: the rules used last time.
    pub fn open(&self, items: Vec<(PathBuf, bool)>, others: Vec<String>, last: BatchRenameState) {
        let Some(window) = self.0.window.upgrade() else { return };
        let selected: HashSet<String> =
            items.iter().filter_map(|(p, _)| p.file_name().map(|n| n.to_string_lossy().into_owned())).collect();
        let lang = Lang::from_code(&gezik_platform::language());
        let list: Vec<Item> = items
            .iter()
            .map(|(path, is_dir)| {
                let meta = std::fs::symlink_metadata(path).ok();
                Item {
                    name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                    is_dir: *is_dir,
                    parent: path
                        .parent()
                        .and_then(|p| p.file_name())
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    folder: 0,
                    size: meta.as_ref().map_or(0, |m| m.len()),
                    modified: meta.and_then(|m| m.modified().ok()).and_then(gezik_platform::local_date_parts),
                    taken: None,
                }
            })
            .collect();
        {
            let mut r = self.0.rules.borrow_mut();
            *r = Rules {
                order: (0..list.len()).collect(),
                paths: items.into_iter().map(|(p, _)| p).collect(),
                items: list,
                others: others.into_iter().filter(|n| !selected.contains(n)).collect(),
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
        window.set_rb_scroll(0.0);
        self.0.open.set(true);
        self.recompute();
        window.set_rb_open(true);
    }

    pub fn close(&self) {
        self.0.open.set(false);
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

    fn recompute(&self) {
        self.0.rules.borrow_mut().recompute();
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
        if selection.len() != shown.len() {
            *selection = gezik_core::selection::Selection::new(shown.len());
        }
        let rows: Vec<RenamePreviewRow> =
            shown.iter().enumerate().filter_map(|(row, &p)| r.preview_row(p, selection.is_selected(row))).collect();
        self.0.rows.set_vec(rows);
        *self.0.shown.borrow_mut() = shown;
        window.set_rb_footer(r.footer().into());
        window.set_rb_can_rename(r.can_rename());
        window.set_rb_only_changed(only_changed);
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
        self.0.ops.submit(Box::new(gezik_ops::RenameTask::many(pairs)), None, crate::operations::After::Select);
    }

    /// A key while the layer has the keyboard; returns whether it was used.
    fn key(&self, text: &str, control: bool, alt: bool, shift: bool, meta: bool) -> bool {
        use gezik_config::shortcuts::{Key, Platform};
        let platform = Platform::current();
        let Some(chord) = crate::keys::chord_from_slint(text, control, alt, shift, meta, platform) else {
            return false;
        };
        let primary = crate::keys::is_primary(&chord, platform);
        match chord.key {
            Key::Escape if !primary && !chord.shift => self.close(),
            Key::Enter if primary => self.rename(),
            _ => return false,
        }
        true
    }
}

thread_local! {
    static CURRENT: RefCell<Option<BatchRename>> = const { RefCell::new(None) };
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
            others: others.iter().map(|s| s.to_string()).collect(),
            ..Rules::default()
        }
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
