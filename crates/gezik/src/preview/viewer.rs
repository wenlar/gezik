//! The preview's viewer (spec 13 §3.1): what it shows beyond the first look, for the shown
//! file only. One owner (the preview), dropped when the file changes or nothing shows the
//! preview: what it holds goes with it.

use gezik_config::shortcuts::Key;
use slint::{ModelRc, VecModel};

use super::hex::HexView;
use super::text::{Row, TextView};
use crate::{PreviewInfo, PreviewPart, PreviewRow};

/// The text view, and the hex view over it while shown (the text keeps its place under it).
#[derive(Default)]
pub struct Viewer {
    pub text: Option<TextView>,
    pub hex: Option<HexView>,
}

/// Where the viewer shows: whole characters across, whole rows down.
pub type Room = (usize, usize);

impl Viewer {
    /// Puts the rows and the bar into `info`.
    pub fn fill(&self, info: &mut PreviewInfo, (cols, page): Room) {
        let (bar, rows, text, note, gutter, (pos, size)) = if let Some(hex) = &self.hex {
            (2, hex.rows(page + 1, cols), hex.status(), "", hex.gutter(), hex.scroll_bar(page, cols))
        } else if let Some(view) = &self.text {
            (
                1,
                view.rows(page + 1, cols),
                view.status(cols, page),
                view.note(),
                view.gutter(),
                view.scroll_bar(cols, page),
            )
        } else {
            // A file shows its picture or icon; Hex is a click away (never by itself).
            let file = matches!(info.kind, 1 | 3) && !info.loading;
            (if file { 3 } else { 0 }, Vec::new(), String::new(), "", 0, (0.0, 0.0))
        };
        info.bar = bar;
        info.rows = model(rows);
        info.bar_text = text.into();
        info.bar_note = note.into();
        info.gutter = i32::try_from(gutter).unwrap_or(0);
        info.wrap = self.hex.is_none() && self.text.as_ref().is_some_and(|t| t.wrap);
        info.scroll_pos = pos;
        info.scroll_size = size;
    }

    pub fn shown(&self) -> bool {
        self.hex.is_some() || self.text.is_some()
    }

    pub fn scroll(&mut self, delta: i64, (cols, page): Room) {
        if let Some(hex) = &mut self.hex {
            hex.scroll(delta, cols, page);
        } else if let Some(text) = &mut self.text {
            text.scroll(delta, cols, page);
        }
    }

    pub fn jump(&mut self, fraction: f32, (cols, page): Room) {
        if let Some(hex) = &mut self.hex {
            hex.jump(fraction, cols, page);
        } else if let Some(text) = &mut self.text {
            text.jump(fraction, cols, page);
        }
    }

    /// Page Up/Down, Home/End and (`arrows`: in the pane) Up/Down; whether it was one.
    pub fn key(&mut self, key: Key, arrows: bool, room: Room) -> bool {
        let (cols, page) = room;
        let step = i64::try_from(page.saturating_sub(1).max(1)).unwrap_or(1);
        match key {
            Key::PageDown => self.scroll(step, room),
            Key::PageUp => self.scroll(-step, room),
            Key::Down if arrows => self.scroll(1, room),
            Key::Up if arrows => self.scroll(-1, room),
            Key::Home => {
                if let Some(hex) = &mut self.hex {
                    hex.home();
                } else if let Some(text) = &mut self.text {
                    text.home();
                }
            }
            Key::End => {
                if let Some(hex) = &mut self.hex {
                    hex.end(cols, page);
                } else if let Some(text) = &mut self.text {
                    text.end(cols, page);
                }
            }
            _ => return false,
        }
        true
    }
}

fn model(rows: Vec<Row>) -> ModelRc<PreviewRow> {
    if rows.is_empty() {
        // Equal to the default: an unused preview stays as it was.
        return ModelRc::default();
    }
    let rows: Vec<PreviewRow> = rows
        .into_iter()
        .map(|row| {
            let parts: Vec<PreviewPart> = row
                .parts
                .into_iter()
                .map(|(col, text, class)| PreviewPart {
                    col: i32::try_from(col).unwrap_or(i32::MAX),
                    text: text.into(),
                    class: class as i32,
                })
                .collect();
            PreviewRow { number: row.number.into(), parts: ModelRc::new(VecModel::from(parts)) }
        })
        .collect();
    ModelRc::new(VecModel::from(rows))
}
