//! The preview of the selection: the pane on the right (Alt+P) and the quick look window
//! (Space) show the same thing. Loading runs on a short-lived thread per request; a newer
//! request makes older results be dropped.

use std::cell::{Cell, RefCell};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use gezik_core::format_size;
use gezik_platform::{IconTarget, Rgba, format_datetime};
use slint::{ComponentHandle, Image, Rgba8Pixel, SharedPixelBuffer};

use crate::view::View;
use crate::{AppWindow, PreviewInfo};

/// How much of a text file is shown.
pub const TEXT_BYTES: usize = 64 * 1024;
/// A NUL byte in this much of the start makes a file binary, not text.
pub const BINARY_PROBE: usize = 8 * 1024;
/// A folder's entries are counted up to this many.
pub const MAX_COUNTED: usize = 10_000;

/// What to preview, captured on the UI thread.
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    Nothing,
    Entry {
        path: PathBuf,
        is_dir: bool,
        name: String,
        type_name: String,
        /// Files only.
        size: Option<u64>,
        modified: Option<SystemTime>,
        created: Option<SystemTime>,
        /// `Kind` index, for the fallback icon.
        kind: i32,
    },
    Several {
        count: usize,
        /// Of the selected files; `None` if only folders are selected.
        size: Option<u64>,
    },
}

/// What the loading thread found; `Send`.
pub enum Body {
    None,
    /// The picture, and its own size when it is the file itself (not a thumbnail or icon).
    Picture(SharedPixelBuffer<Rgba8Pixel>, Option<(u32, u32)>),
    Text(String),
    Folder {
        count: usize,
        more: bool,
    },
}

fn buffer(rgba: Rgba) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
    let expected = rgba.width as usize * rgba.height as usize * 4;
    (rgba.width > 0 && rgba.pixels.len() == expected)
        .then(|| SharedPixelBuffer::clone_from_slice(&rgba.pixels, rgba.width, rgba.height))
}

/// Loads the preview of `target`, pictures fitting `px`. Runs on a worker thread; never
/// panics on bad files.
pub fn load(target: &Target, px: u32) -> Body {
    let Target::Entry { path, is_dir, .. } = target else { return Body::None };
    if *is_dir {
        let (count, more) = count_entries(path);
        return Body::Folder { count, more };
    }
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or_default().to_lowercase();
    if gezik_platform::can_decode(&ext)
        && let Ok(decoded) = gezik_platform::decode_image(path, px)
        && let Some(picture) = buffer(decoded.image)
    {
        return Body::Picture(picture, Some((decoded.width, decoded.height)));
    }
    if let Some(text) = read_text(path) {
        return Body::Text(text);
    }
    let picture =
        gezik_platform::thumbnail(path, px).or_else(|| gezik_platform::icon(&IconTarget::Path(path.clone()), px));
    match picture.and_then(buffer) {
        Some(picture) => Body::Picture(picture, None),
        None => Body::None,
    }
}

/// The start of a text file (at most [`TEXT_BYTES`]); `None` for binary files (a NUL in
/// the first [`BINARY_PROBE`] bytes) and unreadable ones.
pub fn read_text(path: &Path) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::with_capacity(TEXT_BYTES);
    file.take(TEXT_BYTES as u64).read_to_end(&mut bytes).ok()?;
    if bytes[..bytes.len().min(BINARY_PROBE)].contains(&0) {
        return None;
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// How many entries a folder has, up to [`MAX_COUNTED`]; `true` if there are more.
pub fn count_entries(path: &Path) -> (usize, bool) {
    let Ok(entries) = std::fs::read_dir(path) else { return (0, false) };
    let count = entries.take(MAX_COUNTED + 1).count();
    (count.min(MAX_COUNTED), count > MAX_COUNTED)
}

/// `10000` as `10,000`.
fn with_commas(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// What the pane shows for `target`; `body` is `None` while it loads.
pub fn describe(target: &Target, body: Option<Body>) -> PreviewInfo {
    match target {
        Target::Nothing => PreviewInfo::default(),
        Target::Several { count, size } => PreviewInfo {
            kind: 5,
            title: format!("{count} items selected").into(),
            details: size.map(|s| format!("Total size: {}", format_size(s))).unwrap_or_default().into(),
            item_kind: 1,
            ..PreviewInfo::default()
        },
        Target::Entry { name, type_name, size, modified, created, kind, is_dir, .. } => {
            let mut lines = vec![type_name.clone()];
            if let Some(size) = size {
                lines.push(format_size(*size));
            }
            if let Some(time) = modified {
                lines.push(format!("Modified {}", format_datetime(*time)));
            }
            if let Some(time) = created {
                lines.push(format!("Created {}", format_datetime(*time)));
            }
            let mut info = PreviewInfo {
                kind: if *is_dir { 4 } else { 3 },
                title: name.as_str().into(),
                item_kind: *kind,
                loading: body.is_none(),
                ..PreviewInfo::default()
            };
            match body {
                None | Some(Body::None) => {}
                Some(Body::Picture(picture, size)) => {
                    if let Some((w, h)) = size {
                        lines.push(format!("{w} × {h} pixels"));
                        info.kind = 1;
                    }
                    info.picture = Image::from_rgba8(picture);
                    info.has_picture = true;
                }
                Some(Body::Text(text)) => {
                    info.kind = 2;
                    info.text = text.into();
                }
                Some(Body::Folder { count, more }) => lines.push(match (count, more) {
                    (_, true) => format!("{}+ items", with_commas(MAX_COUNTED)),
                    (1, false) => "1 item".to_owned(),
                    (n, false) => format!("{n} items"),
                }),
            }
            info.details = lines.join("\n").into();
            info
        }
    }
}

thread_local! {
    /// The preview of this (UI) thread, so loading threads and timers can reach it.
    static CURRENT: RefCell<Option<Preview>> = const { RefCell::new(None) };
}

/// Runs `f` with this UI thread's preview, if there is one.
pub fn with_current<R>(f: impl FnOnce(&Preview) -> R) -> Option<R> {
    let preview = CURRENT.with(|c| c.borrow().clone())?;
    Some(f(&preview))
}

/// How long the selection must stay before its preview loads (arrow keys held down).
const DELAY: Duration = Duration::from_millis(100);

struct Inner {
    window: slint::Weak<AppWindow>,
    view: View,
    pane_open: Cell<bool>,
    /// Bumped per request: a load whose number is no longer current is dropped.
    generation: Arc<AtomicU64>,
    timer: slint::Timer,
    /// What is shown now (the quick look window opens with it).
    info: RefCell<PreviewInfo>,
    quick_look: RefCell<Option<crate::quick_look::QuickLook>>,
}

#[derive(Clone)]
pub struct Preview(Rc<Inner>);

impl Preview {
    pub fn new(window: &AppWindow, view: View) -> Preview {
        let preview = Preview(Rc::new(Inner {
            window: window.as_weak(),
            view: view.clone(),
            pane_open: Cell::new(false),
            generation: Arc::default(),
            timer: slint::Timer::default(),
            info: RefCell::new(PreviewInfo::default()),
            quick_look: RefCell::new(None),
        }));
        CURRENT.with(|c| *c.borrow_mut() = Some(preview.clone()));
        view.on_selection_changed(|| {
            with_current(Preview::schedule);
        });
        preview
    }

    pub fn is_pane_open(&self) -> bool {
        self.0.pane_open.get()
    }

    pub fn set_pane_open(&self, open: bool) {
        self.0.pane_open.set(open);
        if let Some(window) = self.0.window.upgrade() {
            window.set_preview_open(open);
        }
        if open {
            self.refresh();
        } else if !self.active() {
            self.release();
        }
    }

    pub fn toggle_pane(&self) {
        self.set_pane_open(!self.is_pane_open());
    }

    /// Whether anything shows the preview.
    fn active(&self) -> bool {
        self.0.pane_open.get() || self.quick_look_open()
    }

    /// The selection changed: loads its preview once it stays for a moment.
    pub fn schedule(&self) {
        if !self.active() {
            return;
        }
        self.0.timer.start(slint::TimerMode::SingleShot, DELAY, || {
            with_current(Preview::refresh);
        });
    }

    /// Shows the selection's facts now and loads its picture or text in the background.
    pub fn refresh(&self) {
        if !self.active() {
            return;
        }
        let target = self.0.view.preview_target();
        let generation = self.0.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.publish(describe(&target, None));
        if !matches!(target, Target::Entry { .. }) {
            return;
        }
        let (current, weak, px) = (self.0.generation.clone(), self.0.window.clone(), self.picture_px());
        std::thread::spawn(move || {
            gezik_platform::init_thread();
            let body = load(&target, px);
            let _ = weak.upgrade_in_event_loop(move |_| {
                if current.load(Ordering::SeqCst) == generation {
                    with_current(|p| p.publish(describe(&target, Some(body))));
                }
            });
        });
    }

    /// The picture size to load, in physical pixels: what the pane can show.
    fn picture_px(&self) -> u32 {
        let Some(window) = self.0.window.upgrade() else { return 256 };
        let quick = self.0.quick_look.borrow().as_ref().map_or(0.0, |q| q.width());
        let logical = if self.0.pane_open.get() { window.get_preview_width().max(quick) } else { quick };
        (logical * window.window().scale_factor()).round().clamp(64.0, 1024.0) as u32
    }

    fn publish(&self, info: PreviewInfo) {
        if let Some(window) = self.0.window.upgrade() {
            window.set_preview(info.clone());
        }
        if let Some(quick_look) = self.0.quick_look.borrow().as_ref() {
            quick_look.set_info(info.clone());
        }
        *self.0.info.borrow_mut() = info;
    }

    pub fn quick_look_open(&self) -> bool {
        self.0.quick_look.borrow().is_some()
    }

    /// Space on the list: opens quick look, or closes it.
    pub fn toggle_quick_look(&self) {
        if self.quick_look_open() {
            return self.close_quick_look();
        }
        let Some(window) = self.0.window.upgrade() else { return };
        let info = self.0.info.borrow().clone();
        let opened = crate::quick_look::QuickLook::open(
            &window,
            info,
            window.get_mono_font(),
            |text, ctrl, alt, shift, meta| {
                with_current(|p| p.quick_look_key(text, ctrl, alt, shift, meta)).unwrap_or(false)
            },
            Preview::close_later,
        );
        match opened {
            Ok(quick_look) => {
                *self.0.quick_look.borrow_mut() = Some(quick_look);
                self.refresh();
            }
            Err(err) => window.set_status(format!("Cannot open quick look: {err}").into()),
        }
    }

    pub fn close_quick_look(&self) {
        // Taken out first: no borrow is held while the window is hidden.
        let quick_look = self.0.quick_look.borrow_mut().take();
        if let Some(quick_look) = quick_look {
            quick_look.close();
        }
        if !self.active() {
            self.release();
        }
    }

    /// Closes quick look once the current event is done: its window may be the one
    /// handling it.
    fn close_later() {
        slint::Timer::single_shot(Duration::ZERO, || {
            with_current(Preview::close_quick_look);
        });
    }

    /// A key in the quick look window: Space or Esc closes it, arrows move in the list.
    fn quick_look_key(&self, text: &str, ctrl: bool, alt: bool, shift: bool, meta: bool) -> bool {
        use gezik_config::shortcuts::{Key, Platform};
        use gezik_core::layout::Move;
        let Some(chord) = crate::keys::chord_from_slint(text, ctrl, alt, shift, meta, Platform::current()) else {
            return false;
        };
        if chord.ctrl || chord.alt || chord.meta || chord.shift {
            return false;
        }
        let mv = match chord.key {
            Key::Space | Key::Escape => {
                Preview::close_later();
                return true;
            }
            Key::Up => Move::Up,
            Key::Down => Move::Down,
            Key::Left => Move::Left,
            Key::Right => Move::Right,
            _ => return false,
        };
        self.0.view.key_move(mv, false, false, 1)
    }

    /// The theme changed: quick look follows (the main window is done by theme_bridge).
    pub fn retheme(&self, theme: &gezik_config::theme::ResolvedTheme) {
        if let Some(quick_look) = self.0.quick_look.borrow().as_ref() {
            quick_look.retheme(theme);
        }
    }

    /// Nothing shows the preview: lets go of its picture and drops loads in flight.
    fn release(&self) {
        self.0.timer.stop();
        self.0.generation.fetch_add(1, Ordering::SeqCst);
        self.publish(PreviewInfo::default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gezik-preview-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn entry(path: PathBuf, is_dir: bool) -> Target {
        Target::Entry {
            name: path.file_name().unwrap().to_string_lossy().into_owned(),
            path,
            is_dir,
            type_name: "Thing".into(),
            size: (!is_dir).then_some(2048),
            modified: None,
            created: None,
            kind: 1,
        }
    }

    #[test]
    fn text_files_show_their_start() {
        let dir = temp("text");
        let path = dir.join("a.txt");
        std::fs::write(&path, "merhaba dünya").unwrap();
        assert_eq!(read_text(&path).as_deref(), Some("merhaba dünya"));
        let big = dir.join("big.log");
        std::fs::write(&big, "x".repeat(TEXT_BYTES * 2)).unwrap();
        assert_eq!(read_text(&big).unwrap().len(), TEXT_BYTES);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn binary_files_are_not_text() {
        let dir = temp("binary");
        let path = dir.join("a.bin");
        std::fs::write(&path, b"MZ\x90\x00\x03\x00").unwrap();
        assert_eq!(read_text(&path), None);
        assert_eq!(read_text(&dir.join("missing")), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn folders_are_counted_up_to_the_limit() {
        let dir = temp("count");
        for i in 0..3 {
            std::fs::write(dir.join(format!("{i}")), "").unwrap();
        }
        assert_eq!(count_entries(&dir), (3, false));
        assert_eq!(count_entries(&dir.join("missing")), (0, false));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn loads_pictures_with_their_size() {
        let dir = temp("picture");
        let path = dir.join("p.png");
        image::RgbaImage::from_pixel(300, 200, image::Rgba([1, 2, 3, 255])).save(&path).unwrap();
        match load(&entry(path, false), 100) {
            Body::Picture(picture, Some((300, 200))) => assert_eq!((picture.width(), picture.height()), (100, 67)),
            _ => panic!("expected the picture"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn describes_entries_and_selections() {
        let several = describe(&Target::Several { count: 3, size: Some(1536) }, None);
        assert_eq!(
            (several.kind, several.title.as_str(), several.details.as_str()),
            (5, "3 items selected", "Total size: 1.5 KB")
        );
        let file = entry(PathBuf::from("/x/notes.txt"), false);
        let loading = describe(&file, None);
        assert!(loading.loading && loading.kind == 3);
        assert_eq!(loading.details.as_str(), "Thing\n2.0 KB");
        let text = describe(&file, Some(Body::Text("hi".into())));
        assert_eq!((text.kind, text.text.as_str(), text.loading), (2, "hi", false));
        let picture = describe(&file, Some(Body::Picture(SharedPixelBuffer::new(2, 2), Some((300, 200)))));
        assert_eq!(picture.kind, 1);
        assert!(picture.has_picture && picture.details.ends_with("300 × 200 pixels"));
        let folder =
            describe(&entry(PathBuf::from("/x/sub"), true), Some(Body::Folder { count: MAX_COUNTED, more: true }));
        assert_eq!(folder.kind, 4);
        assert!(folder.details.ends_with("10,000+ items"), "{}", folder.details);
        assert_eq!(describe(&Target::Nothing, None).kind, 0);
    }
}
