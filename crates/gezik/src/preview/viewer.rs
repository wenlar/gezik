//! The preview's viewer (spec 13 §3.1): what it shows beyond the first look, for the shown
//! file only. One owner (the preview), dropped when the file changes or nothing shows the
//! preview: what it holds goes with it.

use gezik_config::shortcuts::Key;
use gezik_core::text::display_width;
use slint::{Image, ModelRc, VecModel};

use super::hex::HexView;
use super::media::Player;
use super::pdf::PdfView;
use super::text::{Row, TextView};
use super::zoom::{Fit, ZoomView};
use crate::{PreviewInfo, PreviewPart, PreviewRow, PreviewTool};

/// The text view, and the hex view over it while shown (the text keeps its place under it);
/// a PDF's pages or a zoomed picture (spec 13 §5); a video's or sound's player (spec 13 §6).
#[derive(Default)]
pub struct Viewer {
    pub text: Option<TextView>,
    pub hex: Option<HexView>,
    pub pdf: Option<PdfView>,
    pub picture: Option<PictureView>,
    pub media: Option<Player>,
}

/// The largest picture zoomed at its own size (spec 13 §5.3: 48 MP, about 192 MB); a larger
/// one is zoomed from its preview.
pub const MAX_ZOOM_PIXELS: u64 = 48_000_000;

/// A picture zoomed past "fit": its own size, decoded whole once (when not too large).
pub struct PictureView {
    /// Its content is the picture's own size; 100% is one picture pixel a physical pixel.
    pub zoom: ZoomView,
    /// Made into an `Image` once: every redraw (a drag) shows the same one.
    pub full: Option<Image>,
    /// Why it is not decoded whole (too large), for the bar.
    pub too_large: Option<String>,
    pub scale_factor: f32,
}

impl PictureView {
    pub fn new(size: (u32, u32), scale_factor: f32, view: (f32, f32)) -> PictureView {
        PictureView {
            zoom: ZoomView::new((size.0 as f32, size.1 as f32), 1.0, view),
            full: None,
            too_large: too_large(size),
            scale_factor,
        }
    }
}

/// Why a `w` × `h` picture is zoomed from its preview only: Gezik decodes at most 8192 px a
/// side, and at most 48 MP at its own size.
pub fn too_large((w, h): (u32, u32)) -> Option<String> {
    let side = gezik_platform::MAX_DECODE_SIDE;
    let pixels = u64::from(w) * u64::from(h);
    if w > side || h > side {
        Some(format!("Too large to zoom ({w} × {h}, over {side} px a side)"))
    } else if pixels > MAX_ZOOM_PIXELS {
        Some(format!("Too large to zoom ({} MP)", pixels.div_ceil(1_000_000)))
    } else {
        None
    }
}

/// A PDF's or picture's bar buttons for `info` (bar 4 or 5): Hex, ‹ the page ›, − the zoom
/// +, Fit; with a link (locked) only the link and Hex; a player's (bar 6): Hex, Play or Pause
/// after its link. A `narrow` bar drops Hex and the zoom's figure (the right-click menu has
/// them all).
pub fn bar_tools(info: &PreviewInfo, narrow: bool) -> ModelRc<PreviewTool> {
    if info.bar < 4 {
        return ModelRc::default();
    }
    let tool = |text: String, label: String, what: i32, step: i32| PreviewTool {
        text: text.into(),
        label: label.into(),
        what,
        step,
    };
    let mut tools = Vec::new();
    // The link (Download, Enter Password…) first: it is what to do next.
    let link = !info.bar_link.is_empty();
    if link {
        tools.push(tool(info.bar_link.to_string(), info.bar_link.to_string(), 9, 0));
    }
    if !narrow {
        tools.push(tool("Hex".into(), "Hex".into(), -1, 0));
    }
    if info.bar == 6 {
        // Pause is drawn (step 1): the UI font has no ❚❚.
        let (text, label, step) = if info.playing { ("", "Pause", 1) } else { ("▶", "Play", 0) };
        tools.push(tool(text.into(), label.into(), 10, step));
        return ModelRc::new(VecModel::from(tools));
    }
    // Locked (a password, no pdfium): pages cannot turn and the zoom shows nothing new, so
    // only the link, and the note gets the room.
    if link {
        return ModelRc::new(VecModel::from(tools));
    }
    if info.bar == 4 {
        let (page, pages) = (info.page, info.pages);
        let (text, label) = if pages > 0 {
            (format!("{page} / {pages}"), format!("Page {page} of {pages}, go to page"))
        } else {
            (page.to_string(), format!("Page {page}, go to page"))
        };
        tools.push(tool("‹".into(), "Previous page".into(), 1, -1));
        tools.push(tool(text, label, 2, 0));
        tools.push(tool("›".into(), "Next page".into(), 1, 1));
    }
    tools.push(tool("−".into(), "Zoom out".into(), 3, -1));
    // The figure (a click is 100%); "Fit" only once, the button says it.
    if !narrow && info.zoom_text != "Fit" {
        tools.push(tool(info.zoom_text.to_string(), format!("{}, actual size", info.zoom_text), 5, 0));
    }
    tools.push(tool("+".into(), format!("Zoom in, {}", info.zoom_text), 3, 1));
    let fit = if info.bar == 4 { "Fit the page or its width" } else { "Fit the picture" };
    tools.push(tool("Fit".into(), fit.into(), 4, 0));
    ModelRc::new(VecModel::from(tools))
}

/// The bar's zoom: "Fit", "Fit Width" or "125%".
fn zoom_text(zoom: &ZoomView) -> String {
    match zoom.fit {
        Fit::Page => "Fit".into(),
        Fit::Width => "Fit Width".into(),
        Fit::Zoom(_) => format!("{}%", zoom.percent()),
    }
}

/// Shows `picture` (drawn at `scale` physical pixels per content unit, `region` of it) where
/// `zoom` places it, in logical pixels.
fn place(info: &mut PreviewInfo, zoom: &ZoomView, scale_factor: f32, scale: f32, region: (u32, u32, u32, u32)) {
    let (x, y, w, h) = zoom.place(scale, region);
    let sf = scale_factor.max(0.1);
    info.zoomed = true;
    (info.pic_x, info.pic_y, info.pic_w, info.pic_h) = (x / sf, y / sf, w / sf, h / sf);
}

/// Where the viewer shows: whole characters across, whole rows down.
pub type Room = (usize, usize);

/// The text view's and the hex view's places.
pub type Place = (Option<((u64, usize), bool)>, Option<u64>);

impl Viewer {
    /// Puts the rows and the bar into `info`; the zoom surface only when `surface` (not in
    /// the pane behind quick look).
    pub fn fill(&self, info: &mut PreviewInfo, (cols, page): Room, surface: bool) {
        let pdf = self.pdf.as_ref().filter(|_| surface);
        let picture = self.picture.as_ref().filter(|_| surface);
        info.seek = -1.0;
        let (bar, rows, text, note, gutter, (pos, size)) = if let Some(hex) = &self.hex {
            (2, hex.rows(page + 1, cols), hex.status(), String::new(), hex.gutter(), hex.scroll_bar(page, cols))
        } else if let Some(pdf) = pdf {
            info.page = i32::try_from(pdf.page).unwrap_or(i32::MAX);
            info.pages = i32::try_from(pdf.pages).unwrap_or(i32::MAX);
            info.zoom_text = zoom_text(&pdf.zoom).into();
            let (note, link) = pdf.note();
            info.bar_link = link.into();
            // The last picture stays until the next page's comes.
            if let Some(shown) = &pdf.shown {
                info.picture = shown.image.clone();
                info.has_picture = true;
                place(info, &pdf.zoom, pdf.scale_factor, shown.scale, shown.region);
            }
            (4, Vec::new(), String::new(), note.to_owned(), 0, (0.0, 0.0))
        } else if let Some(picture) = picture {
            info.zoom_text = zoom_text(&picture.zoom).into();
            if let Some(full) = &picture.full {
                info.picture = full.clone();
            }
            let size = info.picture.size();
            let scale = size.width as f32 / picture.zoom.content().0;
            place(info, &picture.zoom, picture.scale_factor, scale, (0, 0, size.width, size.height));
            (5, Vec::new(), String::new(), picture.too_large.clone().unwrap_or_default(), 0, (0.0, 0.0))
        } else if let Some(player) = &self.media {
            let (note, link) = player.note();
            info.bar_link = link.into();
            info.seek = player.seek_bar();
            info.playing = player.playing();
            (6, Vec::new(), player.time(), note, 0, (0.0, 0.0))
        } else if let Some(view) = &self.text {
            (
                1,
                view.rows(page + 1, cols),
                view.status(cols, page),
                view.note().to_owned(),
                view.gutter(),
                view.scroll_bar(cols, page),
            )
        } else {
            // A file shows its picture or icon; Hex is a click away (never by itself). A PDF
            // offers its pages and a picture its zoom: nothing runs until one is used.
            let file = matches!(info.kind, 1 | 3) && !info.loading;
            let bar = match (file, info.kind, info.page) {
                (false, ..) => 0,
                (_, _, 1..) => 4,
                // A video or sound file (its `Kind`): Play is a click away.
                _ if super::media_kind(info.item_kind) => 6,
                (_, 1, _) => 5,
                _ => 3,
            };
            if bar == 4 || bar == 5 {
                info.zoom_text = "Fit".into();
            }
            let time = if bar == 6 { "--:--" } else { "" };
            (bar, Vec::new(), time.to_owned(), String::new(), 0, (0.0, 0.0))
        };
        info.bar = bar;
        info.max_cols = rows
            .iter()
            .filter_map(|row| {
                row.parts.last().map(|(col, text, _)| col + text.chars().map(display_width).sum::<usize>())
            })
            .max()
            .map_or(0, |cols| i32::try_from(cols).unwrap_or(i32::MAX));
        info.rows = model(rows);
        info.bar_text = text.into();
        info.bar_note = note.into();
        info.gutter = i32::try_from(gutter).unwrap_or(0);
        info.wrap = self.hex.is_none() && self.text.as_ref().is_some_and(|t| t.wrap);
        info.scroll_pos = pos;
        info.scroll_size = size;
    }

    /// Where the views are (the pane keeps its place while quick look scrolls).
    pub fn place(&self) -> Place {
        (self.text.as_ref().map(TextView::place), self.hex.as_ref().map(HexView::place))
    }

    pub fn set_place(&mut self, (text, hex): Place) {
        if let (Some(view), Some(place)) = (&mut self.text, text) {
            view.set_place(place);
        }
        if let (Some(view), Some(place)) = (&mut self.hex, hex) {
            view.set_place(place);
        }
    }

    pub fn shown(&self) -> bool {
        self.hex.is_some()
            || self.text.is_some()
            || self.pdf.is_some()
            || self.picture.is_some()
            || self.media.is_some()
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
                    wide: super::text::is_wide(&text),
                    class: class as i32,
                    text: text.into(),
                })
                .collect();
            PreviewRow { number: row.number.into(), parts: ModelRc::new(VecModel::from(parts)) }
        })
        .collect();
    ModelRc::new(VecModel::from(rows))
}
