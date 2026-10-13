//! More Columns… and sorting or grouping by a property (spec 13 §8): the tab picker's box
//! (`widgets/picker.slint`) lists the metadata fields with their groups; typing filters by
//! label, key or group. For columns a pick shows or hides one and the box stays open; for a
//! sort or a grouping it applies and closes.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gezik_config::shortcuts::{Chord, Key};
use gezik_core::fields::{self, FieldId};
use gezik_core::group::GroupBy;
use gezik_core::pattern::fold_text;
use gezik_core::sort::{SortKey, SortSpec};
use gezik_core::view::ColumnKey;
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::{AppWindow, PickRow};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    Columns,
    Sort,
    Group,
}

/// The fields `query` lets through for `purpose` (a grouping lists only the fields that group):
/// by label, key or group (shown or English), case folded.
pub fn matching(purpose: Purpose, query: &str) -> Vec<FieldId> {
    let query = fold_text(query.trim());
    fields::all()
        .filter(|id| purpose != Purpose::Group || id.def().groupable)
        .filter(|id| {
            let def = id.def();
            // The shown (translated) label and group, and the English ones.
            let (label, group) = (gezik_i18n::t("", def.label), gezik_i18n::t("", def.group.label()));
            [def.label, def.key, def.group.label(), &label, &group].iter().any(|text| fold_text(text).contains(&query))
        })
        .collect()
}

thread_local! {
    pub(crate) static CURRENT: crate::windows::Slot<FieldPicker> = const { crate::windows::Slot::new() };
}

pub fn with_current(f: impl FnOnce(&FieldPicker)) {
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
    purpose: Cell<Purpose>,
    rows: RefCell<Vec<FieldId>>,
    current: Cell<usize>,
    model: Rc<VecModel<PickRow>>,
}

#[derive(Clone)]
pub struct FieldPicker(Rc<Inner>);

impl FieldPicker {
    pub fn new(window: &AppWindow) -> FieldPicker {
        let picker = FieldPicker(Rc::new(Inner {
            window: window.as_weak(),
            open: Cell::new(false),
            purpose: Cell::new(Purpose::Columns),
            rows: RefCell::default(),
            current: Cell::new(0),
            model: Rc::new(VecModel::default()),
        }));
        CURRENT.with(|c| c.set(crate::windows::current(), picker.clone()));
        picker
    }

    /// Opens the box on every field for `purpose`, the field with the keyboard.
    pub fn open(&self, purpose: Purpose) {
        let Some(window) = self.0.window.upgrade() else { return };
        if window.get_tp_open() || crate::tab_tools::over_another_layer(&window) {
            return;
        }
        if purpose != Purpose::Columns && crate::panes::active_view().shows_results() {
            return crate::panes::active_view().note(gezik_i18n::t("", crate::view::FIELDS_IN_FOLDERS).into_owned());
        }
        self.0.purpose.set(purpose);
        *self.0.rows.borrow_mut() = matching(purpose, "");
        self.0.current.set(0);
        self.0.open.set(true);
        window.set_tp_rows(ModelRc::from(self.0.model.clone()));
        window.set_tp_label(
            match purpose {
                Purpose::Columns => gezik_i18n::t!("More columns"),
                Purpose::Sort => gezik_i18n::t!("Sort by property"),
                Purpose::Group => gezik_i18n::t!("Group by property"),
            }
            .as_ref()
            .into(),
        );
        window.set_tp_empty(gezik_i18n::t!("No properties match").as_ref().into());
        window.set_tp_query("".into());
        self.show(&window);
        window.set_tp_open(true);
        window.invoke_focus_list();
    }

    /// Esc closes, Enter picks, Up/Down move; other keys are the field's.
    pub fn chord(&self, chord: &Chord) -> bool {
        if chord.ctrl || chord.alt || chord.meta || chord.shift {
            return false;
        }
        match chord.key {
            Key::Escape => self.close(),
            Key::Enter => self.choose(),
            Key::Up | Key::Down => {
                let len = self.0.rows.borrow().len();
                self.0.current.set(crate::tab_tools::step(self.0.current.get(), len, chord.key == Key::Down));
                if let Some(window) = self.0.window.upgrade() {
                    window.set_tp_current(i32::try_from(self.0.current.get()).unwrap_or(0));
                }
            }
            _ => return false,
        }
        true
    }

    pub fn edited(&self, query: &str) {
        let Some(window) = self.0.window.upgrade() else { return };
        *self.0.rows.borrow_mut() = matching(self.0.purpose.get(), query);
        self.0.current.set(0);
        self.show(&window);
    }

    pub fn chosen(&self, row: usize) {
        self.0.current.set(row);
        self.choose();
    }

    fn choose(&self) {
        let Some(id) = self.0.rows.borrow().get(self.0.current.get()).copied() else { return };
        let view = crate::panes::active_view();
        match self.0.purpose.get() {
            Purpose::Columns => {
                view.toggle_column(ColumnKey::Field(id));
                if let Some(window) = self.0.window.upgrade() {
                    self.show(&window);
                }
            }
            Purpose::Sort => {
                self.close();
                view.set_sort(SortSpec { key: SortKey::Field(id), dir: view.sort().dir });
            }
            Purpose::Group => {
                self.close();
                view.set_group(GroupBy::Field(id));
            }
        }
    }

    fn close(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        self.0.open.set(false);
        window.set_tp_open(false);
        self.0.model.set_vec(Vec::new());
        if !window.get_dialog_open() && !window.get_conflicts_open() {
            window.invoke_focus_list();
        }
    }

    /// The rows on screen: shown columns, the sort's or the grouping's field in bold.
    fn show(&self, window: &AppWindow) {
        let view = crate::panes::active_view();
        let (purpose, settings) = (self.0.purpose.get(), view.view_settings());
        let columns = view.columns_shown();
        let marked = |id: FieldId| match purpose {
            Purpose::Columns => columns.iter().any(|c| c.key == ColumnKey::Field(id) && c.visible),
            Purpose::Sort => settings.sort.key == SortKey::Field(id),
            Purpose::Group => settings.group == GroupBy::Field(id),
        };
        let rows: Vec<PickRow> = self
            .0
            .rows
            .borrow()
            .iter()
            .map(|&id| {
                let on = marked(id);
                PickRow {
                    title: gezik_i18n::t("", id.def().label).as_ref().into(),
                    detail: gezik_i18n::t("", id.def().group.label()).as_ref().into(),
                    tag: if on && purpose == Purpose::Columns { gezik_i18n::t!("Shown") } else { "".into() }
                        .as_ref()
                        .into(),
                    keys: "".into(),
                    bold: on,
                    dot: slint::Color::default(),
                    check: -1,
                    stars: -1,
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
    use gezik_core::fields::{ALBUM, ALBUM_ARTIST, ARTIST, DURATION, TAKEN};

    #[test]
    fn typing_filters_by_label_key_or_group() {
        assert_eq!(matching(Purpose::Columns, "").len(), fields::all().count());
        assert_eq!(matching(Purpose::Columns, "art"), [ARTIST, ALBUM_ARTIST]);
        assert_eq!(matching(Purpose::Columns, "ALBUM"), [ALBUM, ALBUM_ARTIST]);
        assert_eq!(matching(Purpose::Columns, "dur"), [DURATION]);
        assert!(matching(Purpose::Columns, "music").contains(&ARTIST), "by group");
        assert!(matching(Purpose::Columns, "zzz").is_empty());
        let groups = matching(Purpose::Group, "");
        assert!(groups.contains(&TAKEN) && !groups.contains(&DURATION), "only the fields that group");
    }
}
