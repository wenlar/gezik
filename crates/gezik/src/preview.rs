//! The preview of the selection: the pane on the right (Alt+P) and the quick look window
//! (Space) show the same thing. Loading runs on one worker thread that only ever takes the
//! latest request: a newer request replaces one still waiting, makes the one in progress
//! stop at its next step, and makes older results be dropped.

use std::cell::{Cell, RefCell};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use gezik_platform::{IconTarget, Rgba};
use slint::{ComponentHandle, Image, Rgba8Pixel, SharedPixelBuffer};

use crate::{AppWindow, PreviewInfo};

/// How much of a text file is shown.
pub const TEXT_BYTES: usize = 64 * 1024;
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
        /// A file's; a folder's once worked out.
        size: Option<u64>,
        /// A folder's total was only partly read.
        partial: bool,
        /// A folder's files and folders, once its size is worked out (spec 6.2).
        counts: Option<(u64, u64)>,
        modified: Option<SystemTime>,
        created: Option<SystemTime>,
        /// `Kind` index, for the fallback icon.
        kind: i32,
    },
    Several {
        count: usize,
        /// Of the selected files and sized folders; `None` if nothing selected has a size.
        size: Option<u64>,
        /// A selected folder's size is still on its way, or partial.
        more: bool,
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

/// [`load_while`] that never gives up (tests).
#[cfg(test)]
pub fn load(target: &Target, px: u32) -> Body {
    load_while(target, px, &|| true)
}

/// Loads the preview of `target`, pictures fitting `px`; gives up (with [`Body::None`])
/// before each slow step once `wanted` says the result is no longer needed. Runs on a
/// worker thread; never panics on bad files.
pub fn load_while(target: &Target, px: u32, wanted: &dyn Fn() -> bool) -> Body {
    let Target::Entry { path, is_dir, counts, .. } = target else { return Body::None };
    if !wanted() {
        return Body::None;
    }
    if *is_dir {
        if counts.is_some() {
            return Body::None;
        }
        let (count, more) = count_entries(path);
        return Body::Folder { count, more };
    }
    // Reading a file only in iCloud would download it: its icon only.
    let local = !gezik_platform::only_in_cloud(path);
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or_default().to_lowercase();
    if local
        && gezik_platform::can_decode(&ext)
        && let Ok(decoded) = gezik_platform::decode_image(path, px)
        && let Some(picture) = buffer(decoded.image)
    {
        return Body::Picture(picture, Some((decoded.width, decoded.height)));
    }
    if !wanted() {
        return Body::None;
    }
    if local && let Some(text) = read_text(path) {
        return Body::Text(text);
    }
    if !wanted() {
        return Body::None;
    }
    let mut picture = gezik_platform::thumbnail_while(path, px, wanted);
    if picture.is_none() && wanted() {
        // A cloud-only file's own icon would be read from its data: its type's icon instead.
        let target = if local { IconTarget::Path(path.clone()) } else { IconTarget::Extension(ext) };
        picture = gezik_platform::icon(&target, px);
    }
    match picture.and_then(buffer) {
        Some(picture) => Body::Picture(picture, None),
        None => Body::None,
    }
}

/// A one-place mailbox: [`Slot::put`] replaces what is waiting, [`Slot::take`] waits for
/// something. The preview worker only ever sees the latest request this way.
pub struct Slot<T> {
    value: Mutex<Option<T>>,
    ready: Condvar,
}

impl<T> Default for Slot<T> {
    fn default() -> Self {
        Slot { value: Mutex::new(None), ready: Condvar::new() }
    }
}

impl<T> Slot<T> {
    pub fn put(&self, value: T) {
        *self.value.lock().unwrap_or_else(PoisonError::into_inner) = Some(value);
        self.ready.notify_one();
    }

    /// Waits until a value is there and takes it.
    pub fn take(&self) -> T {
        let mut value = self.value.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            if let Some(taken) = value.take() {
                return taken;
            }
            value = self.ready.wait(value).unwrap_or_else(PoisonError::into_inner);
        }
    }
}

/// What the worker loads: `target` with pictures fitting `px`, as request `generation`.
struct Request {
    target: Target,
    px: u32,
    generation: u64,
}

/// Starts the worker thread: it takes the latest request, skips it if a newer one came
/// meanwhile, loads it and hands the result to the UI thread.
fn start_worker(window: slint::Weak<AppWindow>, current: Arc<AtomicU64>) -> Arc<Slot<Request>> {
    let slot = Arc::new(Slot::default());
    let requests = slot.clone();
    let spawned = std::thread::Builder::new().name("gezik-preview".into()).spawn(move || {
        gezik_platform::init_thread();
        loop {
            let Request { target, px, generation } = requests.take();
            let wanted = || current.load(Ordering::SeqCst) == generation;
            let body = load_while(&target, px, &wanted);
            if !wanted() {
                continue;
            }
            let current = current.clone();
            let _ = window.upgrade_in_event_loop(move |_| {
                if current.load(Ordering::SeqCst) == generation {
                    with_current(|p| p.publish(describe(&target, Some(body))));
                }
            });
        }
    });
    if let Err(err) = spawned {
        eprintln!("gezik: cannot start the preview thread: {err}");
    }
    slot
}

/// The start of a text file (at most [`TEXT_BYTES`]); `None` for binary files (a NUL in
/// the first [`gezik_core::text::PROBE`] bytes) and unreadable ones.
pub fn read_text(path: &Path) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::with_capacity(TEXT_BYTES);
    file.take(TEXT_BYTES as u64).read_to_end(&mut bytes).ok()?;
    decode_text(&bytes)
}

/// Text from the start of a file (`gezik_core::text`: byte order marks, UTF-16 without one,
/// UTF-8, the user's ANSI code page); `None` for binary data.
fn decode_text(bytes: &[u8]) -> Option<String> {
    gezik_core::text::decode(bytes, gezik_platform::decode_ansi)
}

/// How many entries a folder has, up to [`MAX_COUNTED`]; `true` if there are more.
pub fn count_entries(path: &Path) -> (usize, bool) {
    let Ok(entries) = std::fs::read_dir(path) else { return (0, false) };
    let count = entries.take(MAX_COUNTED + 1).count();
    (count.min(MAX_COUNTED), count > MAX_COUNTED)
}

/// `10000` as `10,000`.
pub fn with_commas(n: usize) -> String {
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
        Target::Several { count, size, more } => PreviewInfo {
            kind: 5,
            title: format!("{count} items selected").into(),
            details: size
                .map(|s| format!("Total size: {}{}", crate::view_options::size_text(s), if *more { "+" } else { "" }))
                .unwrap_or_default()
                .into(),
            item_kind: 1,
            ..PreviewInfo::default()
        },
        Target::Entry { name, type_name, size, partial, counts, modified, created, kind, is_dir, .. } => {
            let mut lines = vec![type_name.clone()];
            if let Some(size) = size {
                let prefix = if *partial { "≥ " } else { "" };
                lines.push(format!("{prefix}{}", crate::view_options::size_text(*size)));
            }
            if let Some((files, folders)) = counts {
                let n = |v: u64, one: &str| match v {
                    1 => format!("1 {one}"),
                    v => format!("{} {one}s", with_commas(usize::try_from(v).unwrap_or(usize::MAX))),
                };
                lines.push(format!("{}, {}", n(*files, "file"), n(*folders, "folder")));
            }
            if *partial {
                lines.push("Some folders could not be read".to_owned());
            }
            if let Some(time) = modified {
                lines.push(format!("Modified {}", crate::view_options::date_text(*time)));
            }
            if let Some(time) = created {
                lines.push(format!("Created {}", crate::view_options::date_text(*time)));
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
                    (n, false) => crate::stack::count_text(n),
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

thread_local! {
    /// Space opens the system's Quick Look panel (macOS, `[system] quick-look = "system"`).
    static SYSTEM_PANEL: Cell<bool> = const { Cell::new(false) };
}

/// settings.toml changed: `[system] quick-look`. Only macOS has the system's panel.
pub fn set_quick_look(mode: gezik_config::settings::QuickLookMode) {
    let system = cfg!(target_os = "macos") && mode == gezik_config::settings::QuickLookMode::System;
    SYSTEM_PANEL.with(|s| s.set(system));
}

/// The panel's items: the selection (at the focused one's place), else the focused item.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn panel_items(selected: Vec<PathBuf>, focused: Option<PathBuf>) -> (Vec<PathBuf>, usize) {
    if selected.is_empty() {
        return (focused.into_iter().collect(), 0);
    }
    let index = focused.and_then(|f| selected.iter().position(|p| *p == f)).unwrap_or(0);
    (selected, index)
}

struct Inner {
    window: slint::Weak<AppWindow>,
    pane_open: Cell<bool>,
    /// Bumped per request: a load whose number is no longer current is dropped.
    generation: Arc<AtomicU64>,
    /// Requests for the worker thread, started with the first one.
    worker: RefCell<Option<Arc<Slot<Request>>>>,
    timer: slint::Timer,
    /// What is shown now (the quick look window opens with it).
    info: RefCell<PreviewInfo>,
    quick_look: RefCell<Option<crate::quick_look::QuickLook>>,
}

#[derive(Clone)]
pub struct Preview(Rc<Inner>);

impl Preview {
    pub fn new(window: &AppWindow) -> Preview {
        let preview = Preview(Rc::new(Inner {
            window: window.as_weak(),
            pane_open: Cell::new(false),
            generation: Arc::default(),
            worker: RefCell::new(None),
            timer: slint::Timer::default(),
            info: RefCell::new(PreviewInfo::default()),
            quick_look: RefCell::new(None),
        }));
        CURRENT.with(|c| *c.borrow_mut() = Some(preview.clone()));
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

    /// Whether anything shows the preview: the pane, quick look, or the last Miller column of
    /// the active pane (a file is selected there, spec 10 §7.1).
    fn active(&self) -> bool {
        self.0.pane_open.get() || self.quick_look_open() || self.column()
    }

    fn column(&self) -> bool {
        crate::panes::with_active(|p| p.view.column_file()).unwrap_or(false)
    }

    /// The selection changed: loads its preview once it stays for a moment.
    pub fn schedule(&self) {
        #[cfg(target_os = "macos")]
        if SYSTEM_PANEL.with(Cell::get) && gezik_platform::ql_panel::is_open() {
            let (items, index) = self.panel_view();
            gezik_platform::ql_panel::update(&items, index);
        }
        if !self.active() {
            // The preview column closed (a folder selected, the list chosen): its picture goes.
            if *self.0.info.borrow() != PreviewInfo::default() {
                self.release();
            }
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
        let target = crate::panes::active_view().preview_target();
        let generation = self.0.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.publish(describe(&target, None));
        if !matches!(target, Target::Entry { .. }) {
            return;
        }
        let px = self.picture_px();
        let worker = self
            .0
            .worker
            .borrow_mut()
            .get_or_insert_with(|| start_worker(self.0.window.clone(), self.0.generation.clone()))
            .clone();
        worker.put(Request { target, px, generation });
    }

    /// The picture size to load, in physical pixels: what the pane can show.
    fn picture_px(&self) -> u32 {
        let Some(window) = self.0.window.upgrade() else { return 256 };
        let quick = self.0.quick_look.borrow().as_ref().map_or(0.0, |q| q.width());
        let logical = if self.0.pane_open.get() {
            window.get_preview_width().max(quick)
        } else if self.column() {
            window.get_column_width().max(quick)
        } else {
            quick
        };
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

    /// The selection and focus as the system panel shows them.
    #[cfg(target_os = "macos")]
    fn panel_view(&self) -> (Vec<PathBuf>, usize) {
        let view = &crate::panes::active_view();
        let selected = view.selected_items().into_iter().map(|(path, _)| path).collect();
        panel_items(selected, view.focus().and_then(|i| view.entry_path(i)).map(|(path, _)| path))
    }

    /// Space with the system panel (macOS): closes it, or opens it on the selection. If it has
    /// not come on screen shortly after, Gezik's own window is shown instead.
    #[cfg(target_os = "macos")]
    fn toggle_system_panel(&self) -> bool {
        if gezik_platform::ql_panel::is_open() {
            gezik_platform::ql_panel::close();
            return true;
        }
        let (items, index) = self.panel_view();
        if items.is_empty() {
            return true;
        }
        let Some(window) = self.0.window.upgrade() else { return true };
        let on_move = Box::new(|to| crate::panes::active_view().key_move(to, false, false, 1));
        if gezik_platform::ql_panel::show(&window.window().window_handle(), &items, index, on_move) {
            return true;
        }
        // With several items the panel can come on screen a moment after the call: Gezik's own
        // window is shown only if it still is not there then.
        slint::Timer::single_shot(Duration::from_millis(500), || {
            if gezik_platform::ql_panel::is_open() {
                return;
            }
            with_current(|p| {
                if let Some(window) = p.0.window.upgrade() {
                    window.set_status("The system Quick Look panel did not open; Gezik's own is shown".into());
                }
                p.open_own_quick_look();
            });
        });
        true
    }

    /// Space on the list: opens quick look, or closes it.
    pub fn toggle_quick_look(&self) {
        #[cfg(target_os = "macos")]
        if SYSTEM_PANEL.with(Cell::get) && !self.quick_look_open() && self.toggle_system_panel() {
            return;
        }
        if self.quick_look_open() {
            return self.close_quick_look();
        }
        self.open_own_quick_look();
    }

    /// Gezik's own quick look window on the selection.
    fn open_own_quick_look(&self) {
        // Nothing selected: nothing to look at.
        if crate::panes::active_view().preview_target() == Target::Nothing {
            return;
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
        crate::panes::active_view().key_move(mv, false, false, 1)
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

    #[test]
    fn panel_items_are_the_selection_or_the_focus() {
        let p = |s: &str| PathBuf::from(s);
        assert_eq!(panel_items(vec![p("/a"), p("/b"), p("/c")], Some(p("/b"))), (vec![p("/a"), p("/b"), p("/c")], 1));
        assert_eq!(panel_items(vec![p("/a"), p("/b")], Some(p("/z"))), (vec![p("/a"), p("/b")], 0), "focus outside");
        assert_eq!(panel_items(Vec::new(), Some(p("/f"))), (vec![p("/f")], 0));
        assert_eq!(panel_items(Vec::new(), None), (Vec::new(), 0));
    }

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
            partial: false,
            counts: None,
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
    fn utf8_with_a_byte_order_mark_drops_the_mark() {
        assert_eq!(decode_text(b"\xEF\xBB\xBFSat\xC4\xB1r").as_deref(), Some("Satır"));
        assert_eq!(decode_text(b"").as_deref(), Some(""));
    }

    #[test]
    fn utf8_cut_mid_character_keeps_the_whole_characters() {
        assert_eq!(decode_text(b"Sat\xC4").as_deref(), Some("Sat"));
    }

    #[test]
    fn utf16_little_endian_with_a_mark_is_text() {
        let mut bytes = vec![0xFF, 0xFE];
        bytes.extend("Satır — x".encode_utf16().flat_map(u16::to_le_bytes));
        assert_eq!(decode_text(&bytes).as_deref(), Some("Satır — x"));
        // An odd byte cut by the read limit is dropped.
        bytes.push(b'y');
        assert_eq!(decode_text(&bytes).as_deref(), Some("Satır — x"));
    }

    #[test]
    fn utf16_big_endian_with_a_mark_is_text() {
        let mut bytes = vec![0xFE, 0xFF];
        bytes.extend("Satır 😀".encode_utf16().flat_map(u16::to_be_bytes));
        assert_eq!(decode_text(&bytes).as_deref(), Some("Satır 😀"));
        // Half a surrogate pair cut by the read limit is dropped.
        bytes.truncate(bytes.len() - 2);
        assert_eq!(decode_text(&bytes).as_deref(), Some("Satır "));
    }

    #[cfg(windows)]
    #[test]
    fn invalid_utf8_is_read_in_the_ansi_code_page() {
        // "Satır — x" in Windows-1254; 0x97 is the em dash in the Windows code pages, so
        // the dash holds on non-Turkish systems too.
        let bytes = b"Sat\xFDr \x97 x";
        let text = decode_text(bytes).unwrap();
        assert_eq!(Some(text.clone()), gezik_platform::decode_ansi(bytes));
        assert!(text.starts_with("Sat") && text.ends_with("r \u{2014} x"), "{text}");
        assert!(!text.contains('\u{FFFD}'), "{text}");
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
    fn the_slot_keeps_only_the_latest_value() {
        let slot = Slot::default();
        slot.put(1);
        slot.put(2);
        assert_eq!(slot.take(), 2);
        let slot = Arc::new(slot);
        let taker = std::thread::spawn({
            let slot = slot.clone();
            move || slot.take()
        });
        std::thread::sleep(Duration::from_millis(20));
        slot.put(3);
        assert_eq!(taker.join().unwrap(), 3, "take waits for a value");
    }

    #[test]
    fn stale_loads_stop_early() {
        let dir = temp("stale");
        let path = dir.join("p.png");
        image::RgbaImage::from_pixel(30, 20, image::Rgba([1, 2, 3, 255])).save(&path).unwrap();
        assert!(matches!(load_while(&entry(path.clone(), false), 100, &|| false), Body::None));
        assert!(matches!(load_while(&entry(dir.clone(), true), 100, &|| false), Body::None));
        assert!(matches!(load_while(&entry(path, false), 100, &|| true), Body::Picture(..)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn describes_entries_and_selections() {
        let several = describe(&Target::Several { count: 3, size: Some(1536), more: false }, None);
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

    #[test]
    fn a_sized_folder_shows_its_size_and_counts() {
        let target = Target::Entry {
            path: "/w/d".into(),
            is_dir: true,
            name: "d".into(),
            type_name: "Folder".into(),
            size: Some(2048),
            partial: true,
            counts: Some((1234, 5)),
            modified: None,
            created: None,
            kind: 4,
        };
        let info = describe(&target, Some(Body::None));
        assert_eq!(
            info.details.as_str(),
            "Folder
≥ 2.0 KB
1,234 files, 5 folders
Some folders could not be read"
        );
        assert!(matches!(load(&target, 64), Body::None), "no count of its own once the size is known");
        let mut one = target.clone();
        if let Target::Entry { counts, partial, .. } = &mut one {
            (*counts, *partial) = (Some((1, 1)), false);
        }
        assert_eq!(
            describe(&one, Some(Body::None)).details.as_str(),
            "Folder
2.0 KB
1 file, 1 folder"
        );
        let several = describe(&Target::Several { count: 2, size: Some(1536), more: true }, None);
        assert_eq!(several.details.as_str(), "Total size: 1.5 KB+");
    }

    #[test]
    fn utf16_without_a_mark_shows_as_text() {
        let bytes: Vec<u8> = "Satır bir\r\nSatır iki".encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert_eq!(decode_text(&bytes).as_deref(), Some("Satır bir\r\nSatır iki"));
    }
}
