//! The conflict list: what already exists where a copy or move goes, one decision each,
//! asked once before the job goes on.

use std::path::{Path, PathBuf};

use gezik_core::ops::conflict::{ConflictKind, Decision, choices};

/// What the buttons and the R / S / K / N keys give, in `ConflictList.decide`'s order.
pub const DECISIONS: [Decision; 4] = [Decision::Replace, Decision::Skip, Decision::KeepBoth, Decision::IfNewer];

/// The folder all `targets` are in; names are shown relative to it.
pub fn common_base(targets: &[PathBuf]) -> PathBuf {
    let mut base: Option<PathBuf> = None;
    for target in targets {
        let parent = target.parent().unwrap_or(target);
        base = Some(match base {
            None => parent.to_path_buf(),
            Some(base) => {
                let mut common = PathBuf::new();
                for (a, b) in base.components().zip(parent.components()) {
                    if a != b {
                        break;
                    }
                    common.push(a.as_os_str());
                }
                common
            }
        });
    }
    base.unwrap_or_default()
}

/// `target` relative to `base` (`fotolar/IMG_0012.jpg`).
pub fn row_name(target: &Path, base: &Path) -> String {
    target.strip_prefix(base).unwrap_or(target).display().to_string()
}

/// Sets `decision` on the conflicts at `indices` that offer it.
pub fn apply(decisions: &mut [Decision], kinds: &[ConflictKind], indices: &[usize], decision: Decision) {
    for &i in indices {
        if let (Some(slot), Some(kind)) = (decisions.get_mut(i), kinds.get(i))
            && choices(*kind).contains(&decision)
        {
            *slot = decision;
        }
    }
}

use std::cell::RefCell;
use std::rc::Rc;

use gezik_config::shortcuts::{Key, Platform};
use gezik_core::format_size;
use gezik_core::ops::conflict::{Facts, identical, source_newer};
use gezik_core::selection::Selection;
use gezik_ops::{ConflictItem, Engine, JobId};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::navigation::sync_model;
use crate::{AppWindow, ConflictRow};

/// Rows a PgUp / PgDn moves.
const PAGE: usize = 10;

#[derive(Default)]
struct Data {
    job: Option<JobId>,
    /// As the engine sent them (folder merges first); decisions go back in this order.
    items: Vec<ConflictItem>,
    decisions: Vec<Decision>,
    /// Indices into `items` on screen ("Hide identical" leaves some out).
    shown: Vec<usize>,
    /// Over `shown`.
    selection: Selection,
    hide_identical: bool,
    apply_all: bool,
    base: PathBuf,
}

struct Inner {
    window: slint::Weak<AppWindow>,
    engine: Engine,
    rows: Rc<VecModel<ConflictRow>>,
    data: RefCell<Data>,
}

#[derive(Clone)]
pub struct Conflicts(Rc<Inner>);

impl Conflicts {
    pub fn new(window: &AppWindow, engine: Engine) -> Conflicts {
        let rows = Rc::new(VecModel::default());
        window.set_conflict_rows(ModelRc::from(rows.clone()));
        let conflicts = Conflicts(Rc::new(Inner { window: window.as_weak(), engine, rows, data: RefCell::default() }));
        let c = conflicts.clone();
        window.on_conflict_decide(move |i| {
            if let Some(decision) = usize::try_from(i).ok().and_then(|i| DECISIONS.get(i)) {
                c.decide(*decision);
            }
        });
        let c = conflicts.clone();
        window.on_conflict_set_apply_all(move |all| c.set_apply_all(all));
        let c = conflicts.clone();
        window.on_conflict_toggle_identical(move || c.toggle_identical());
        let c = conflicts.clone();
        window.on_conflict_pressed(move |row, ctrl, shift| {
            if let Ok(row) = usize::try_from(row) {
                c.pressed(row, ctrl, shift);
            }
        });
        let c = conflicts.clone();
        window.on_conflict_start(move || c.start());
        let c = conflicts.clone();
        window.on_conflict_cancel(move || c.cancel());
        let c = conflicts.clone();
        window.on_conflict_key(move |event| {
            let m = event.modifiers;
            c.key(&event.text, m.control, m.alt, m.shift, m.meta)
        });
        conflicts
    }

    /// Shows the conflicts of `job` ("Copying 312 items to D:\Yedek").
    pub fn open(&self, job: JobId, title: &str, items: Vec<ConflictItem>) {
        let Some(window) = self.0.window.upgrade() else { return };
        let real = items.iter().filter(|c| c.kind != ConflictKind::Folder).count();
        let identical_count = items.iter().filter(|c| identical(c.source_facts, c.target_facts)).count();
        let targets: Vec<PathBuf> = items.iter().map(|c| c.target.clone()).collect();
        {
            let mut data = self.0.data.borrow_mut();
            *data = Data {
                job: Some(job),
                decisions: items.iter().map(|c| c.decision).collect(),
                shown: (0..items.len()).collect(),
                selection: Selection::new(items.len()),
                base: common_base(&targets),
                items,
                ..Data::default()
            };
            if !data.shown.is_empty() {
                data.selection.set_focus(0);
            }
        }
        let what = if real == 1 { "1 conflict".to_owned() } else { format!("{real} conflicts") };
        window.set_conflict_title(format!("{what} · {title}").into());
        window.set_conflict_identical_count(i32::try_from(identical_count).unwrap_or(0));
        window.set_conflict_hide_identical(false);
        window.set_conflict_apply_all(false);
        let bin = if cfg!(windows) { "Recycle Bin" } else { "Trash" };
        window.set_conflict_footer(format!("Replaced files go to the {bin} when their drive has one.").into());
        window.set_conflict_scroll(0.0);
        self.refresh();
        window.set_conflicts_open(true);
    }

    /// Closes the list if it belongs to `job` (it ended some other way).
    pub fn close_if(&self, job: JobId) {
        if self.0.data.borrow().job == Some(job) {
            self.close();
        }
    }

    fn close(&self) {
        *self.0.data.borrow_mut() = Data::default();
        self.0.rows.clear();
        if let Some(window) = self.0.window.upgrade() {
            window.set_conflicts_open(false);
            window.invoke_focus_list();
        }
    }

    fn row(data: &Data, position: usize) -> Option<ConflictRow> {
        let i = *data.shown.get(position)?;
        let item = data.items.get(i)?;
        let date = |f: Facts| f.modified.map(gezik_platform::format_datetime).unwrap_or_default();
        let size = |f: Facts| if f.is_dir { String::new() } else { format_size(f.size) };
        Some(ConflictRow {
            name: row_name(&item.target, &data.base).into(),
            source_size: size(item.source_facts).into(),
            source_date: date(item.source_facts).into(),
            target_size: size(item.target_facts).into(),
            target_date: date(item.target_facts).into(),
            newer: match source_newer(item.source_facts, item.target_facts) {
                Some(true) => 1,
                Some(false) => 2,
                None => 0,
            },
            identical: identical(item.source_facts, item.target_facts),
            kind: match item.kind {
                ConflictKind::File => 0,
                ConflictKind::Folder => 1,
                ConflictKind::Mismatch => 2,
            },
            decision: data.decisions[i].label().into(),
            selected: data.selection.is_selected(position),
            focused: data.selection.focus() == Some(position),
        })
    }

    fn refresh(&self) {
        let data = self.0.data.borrow();
        let rows = (0..data.shown.len()).filter_map(|position| Self::row(&data, position));
        sync_model(&self.0.rows, rows);
    }

    /// The conflicts a button or key acts on: all shown, else the selected, else the focused.
    fn targets(data: &Data) -> Vec<usize> {
        if data.apply_all {
            return data.shown.clone();
        }
        let mut positions: Vec<usize> = data.selection.iter().collect();
        if positions.is_empty() {
            positions.extend(data.selection.focus());
        }
        positions.into_iter().filter_map(|p| data.shown.get(p).copied()).collect()
    }

    pub fn decide(&self, decision: Decision) {
        {
            let mut data = self.0.data.borrow_mut();
            let targets = Self::targets(&data);
            let kinds: Vec<ConflictKind> = data.items.iter().map(|c| c.kind).collect();
            apply(&mut data.decisions, &kinds, &targets, decision);
        }
        self.refresh();
    }

    /// From a row's own menu: the row, or all selected rows if it is one of them.
    pub fn decide_row(&self, row: usize, decision: Decision) {
        {
            let mut data = self.0.data.borrow_mut();
            let positions: Vec<usize> =
                if data.selection.is_selected(row) { data.selection.iter().collect() } else { vec![row] };
            let targets: Vec<usize> = positions.into_iter().filter_map(|p| data.shown.get(p).copied()).collect();
            let kinds: Vec<ConflictKind> = data.items.iter().map(|c| c.kind).collect();
            apply(&mut data.decisions, &kinds, &targets, decision);
        }
        self.refresh();
    }

    /// The decisions row `row` offers (none for a merged folder).
    pub fn choices_for(&self, row: usize) -> Vec<Decision> {
        let data = self.0.data.borrow();
        let Some(item) = data.shown.get(row).and_then(|&i| data.items.get(i)) else { return Vec::new() };
        choices(item.kind).iter().copied().filter(|d| DECISIONS.contains(d)).collect()
    }

    pub fn set_apply_all(&self, all: bool) {
        self.0.data.borrow_mut().apply_all = all;
        if let Some(window) = self.0.window.upgrade() {
            window.set_conflict_apply_all(all);
        }
    }

    pub fn toggle_identical(&self) {
        let hide = {
            let mut data = self.0.data.borrow_mut();
            data.hide_identical = !data.hide_identical;
            let hide = data.hide_identical;
            data.shown = (0..data.items.len())
                .filter(|&i| !(hide && identical(data.items[i].source_facts, data.items[i].target_facts)))
                .collect();
            data.selection = Selection::new(data.shown.len());
            if !data.shown.is_empty() {
                data.selection.set_focus(0);
            }
            hide
        };
        if let Some(window) = self.0.window.upgrade() {
            window.set_conflict_hide_identical(hide);
            window.set_conflict_scroll(0.0);
        }
        self.refresh();
    }

    pub fn pressed(&self, row: usize, ctrl: bool, shift: bool) {
        {
            let mut data = self.0.data.borrow_mut();
            match (ctrl, shift) {
                (_, true) => data.selection.extend_to(row, ctrl),
                (true, false) => data.selection.toggle(row),
                (false, false) => data.selection.select_only(row),
            };
        }
        self.refresh();
    }

    /// A key while the list has the keyboard (Slint's text and modifiers); returns whether it
    /// was used.
    pub fn key(&self, text: &str, control: bool, alt: bool, shift: bool, meta: bool) -> bool {
        let platform = Platform::current();
        let Some(chord) = crate::keys::chord_from_slint(text, control, alt, shift, meta, platform) else {
            return false;
        };
        let primary = crate::keys::is_primary(&chord, platform);
        let plain = !primary && !chord.alt && !chord.shift;
        match chord.key {
            Key::Enter => self.start(),
            Key::Escape => self.cancel(),
            Key::Char('a') if primary => {
                self.0.data.borrow_mut().selection.select_all();
                self.refresh();
            }
            Key::Char('r') if plain => self.decide(Decision::Replace),
            Key::Char('s') if plain => self.decide(Decision::Skip),
            Key::Char('k') if plain => self.decide(Decision::KeepBoth),
            Key::Char('n') if plain => self.decide(Decision::IfNewer),
            Key::Up | Key::Down | Key::PageUp | Key::PageDown | Key::Home | Key::End => {
                let target = {
                    let mut data = self.0.data.borrow_mut();
                    let len = data.shown.len();
                    if len == 0 {
                        return true;
                    }
                    let at = data.selection.focus().unwrap_or(0);
                    let target = match chord.key {
                        Key::Up => at.saturating_sub(1),
                        Key::Down => (at + 1).min(len - 1),
                        Key::PageUp => at.saturating_sub(PAGE),
                        Key::PageDown => (at + PAGE).min(len - 1),
                        Key::Home => 0,
                        _ => len - 1,
                    };
                    if chord.shift {
                        data.selection.extend_to(target, primary);
                    } else if primary {
                        data.selection.set_focus(target);
                    } else {
                        data.selection.select_only(target);
                    }
                    target
                };
                self.refresh();
                if let Some(window) = self.0.window.upgrade() {
                    window.invoke_conflict_ensure_visible(i32::try_from(target).unwrap_or(0));
                }
            }
            _ => return false,
        }
        true
    }

    /// Start: the decisions go to the job, which goes on.
    pub fn start(&self) {
        let (job, decisions) = {
            let data = self.0.data.borrow();
            (data.job, data.decisions.clone())
        };
        self.close();
        if let Some(job) = job {
            self.0.engine.decide(job, decisions);
        }
    }

    /// Cancel operation: the job stops; what it already did stays (and can be undone).
    pub fn cancel(&self) {
        let job = self.0.data.borrow().job;
        self.close();
        if let Some(job) = job {
            self.0.engine.cancel(job);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_relative_to_the_shared_folder() {
        let targets = [PathBuf::from("/d/Yedek/a.txt"), PathBuf::from("/d/Yedek").join("fotolar").join("b.jpg")];
        let base = common_base(&targets);
        assert_eq!(base, PathBuf::from("/d/Yedek"));
        assert_eq!(row_name(&targets[1], &base), PathBuf::from("fotolar").join("b.jpg").display().to_string());
        assert_eq!(common_base(&[]), PathBuf::new());
    }

    #[test]
    fn a_decision_only_lands_where_it_is_offered() {
        let kinds = [ConflictKind::File, ConflictKind::Mismatch, ConflictKind::Folder];
        let mut decisions = [Decision::Skip, Decision::Skip, Decision::Merge];
        apply(&mut decisions, &kinds, &[0, 1, 2], Decision::Replace);
        assert_eq!(decisions, [Decision::Replace, Decision::Skip, Decision::Merge]);
        apply(&mut decisions, &kinds, &[1], Decision::KeepBoth);
        assert_eq!(decisions[1], Decision::KeepBoth);
    }
}
