//! The preview's and Info's property rows (spec 13 §7.4): a file's fields under their group
//! headings, in the registry's order.

use std::sync::atomic::{AtomicBool, Ordering};

use gezik_core::fields::{self as f, Fields, Group};
use slint::{ModelRc, VecModel};

use crate::PropRow;

/// `[preview] properties` (on unless set off): the pane reads and shows the fields.
static PROPERTIES: AtomicBool = AtomicBool::new(true);

pub fn set_properties(on: bool) {
    PROPERTIES.store(on, Ordering::Relaxed);
}

pub fn properties() -> bool {
    PROPERTIES.load(Ordering::Relaxed)
}

/// The rows of `fields`: width and height show as Dimensions; a title goes with the music or
/// document fields it came with.
pub fn rows(fields: &Fields) -> Vec<PropRow> {
    let has = |group| fields.iter().any(|(id, _)| id != f::TITLE && id.def().group == group);
    let title_group = if has(Group::Music) {
        Group::Music
    } else if has(Group::Document) {
        Group::Document
    } else {
        Group::Media
    };
    let mut shown = None;
    let mut rows = Vec::new();
    for (id, _) in fields.iter() {
        if matches!(id, f::WIDTH | f::HEIGHT) && fields.get(f::DIMENSIONS).is_some() {
            continue;
        }
        let Some(value) = fields.text(id) else { continue };
        let group = if id == f::TITLE { title_group } else { id.def().group };
        let heading = if shown == Some(group) { Default::default() } else { gezik_i18n::t("", group.label()) };
        shown = Some(group);
        let label = gezik_i18n::t("", id.def().label);
        rows.push(PropRow {
            group: heading.as_ref().into(),
            key: id.def().key.into(),
            label: label.as_ref().into(),
            value: value.into(),
        });
    }
    rows
}

pub fn model(rows: Vec<PropRow>) -> ModelRc<PropRow> {
    if rows.is_empty() { ModelRc::default() } else { ModelRc::new(VecModel::from(rows)) }
}

#[cfg(test)]
mod tests {
    use gezik_core::fields::Value;

    use super::*;

    fn shown(rows: &[PropRow]) -> Vec<(String, String, String)> {
        rows.iter().map(|r| (r.group.to_string(), r.label.to_string(), r.value.to_string())).collect()
    }

    #[test]
    fn rows_go_under_their_groups() {
        let song = Fields::new(vec![
            (f::DURATION, Value::Duration(205_000)),
            (f::TITLE, Value::Text("Gölge".into())),
            (f::ARTIST, Value::Text("Öztürk".into())),
            (f::WIDTH, Value::Int(4)),
            (f::HEIGHT, Value::Int(3)),
            (f::DIMENSIONS, Value::Int(12)),
        ]);
        let s = |a: &str, b: &str, c: &str| (a.to_owned(), b.to_owned(), c.to_owned());
        assert_eq!(
            shown(&rows(&song)),
            [
                s("Picture", "Dimensions", "4 × 3"),
                s("Media", "Duration", "3:25"),
                s("Music", "Title", "Gölge"),
                s("", "Artist", "Öztürk"),
            ]
        );
        let document = Fields::new(vec![(f::TITLE, Value::Text("Rapor".into())), (f::PAGES, Value::Int(3))]);
        assert_eq!(shown(&rows(&document)), [s("Document", "Title", "Rapor"), s("", "Pages", "3")]);
        let video = Fields::new(vec![(f::DURATION, Value::Duration(1000)), (f::TITLE, Value::Text("Film".into()))]);
        assert_eq!(shown(&rows(&video)), [s("Media", "Duration", "0:01"), s("", "Title", "Film")]);
        assert!(rows(&Fields::default()).is_empty());
    }
}
