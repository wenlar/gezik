//! The preview of the selection: the pane on the right (Alt+P) and the quick look window
//! (Space) show the same thing. Loading runs on one worker thread that only ever takes the
//! latest request: a newer request replaces one still waiting, makes the one in progress
//! stop at its next step, and makes older results be dropped.

use std::cell::{Cell, RefCell};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime};

use gezik_core::batch::pdf_view::{Command, Reply};
use gezik_platform::{IconTarget, Rgba};
use slint::{ComponentHandle, Image, Rgba8Pixel, SharedPixelBuffer};

use crate::{AppWindow, PreviewActions, PreviewInfo, PropRow};

pub mod fields;
mod hex;
mod media;
mod pdf;
mod scroller;
mod text;
mod viewer;
mod zoom;

use media::{Play, Player};
use pdf::{PdfView, State};
use viewer::{PictureView, Place, Room, Viewer};
use zoom::{Fit, ZoomView};

/// How long a drag or a zoom must rest before the PDF's new region is drawn (spec 13 §5.2).
const SETTLE: Duration = Duration::from_millis(150);
/// A preview narrower than this (logical px) drops the bar's zoom figure and Hex.
const NARROW: f32 = 300.0;
/// The wheel turns at most one PDF page in this time (a trackpad sends many events).
const TURN_GAP: Duration = Duration::from_millis(300);

/// The PDF `target` is, if it is one.
fn pdf_path(target: &Target) -> Option<PathBuf> {
    match target {
        Target::Entry { path, is_dir: false, .. }
            if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) =>
        {
            Some(path.clone())
        }
        _ => None,
    }
}

/// A `Kind` index of a video or sound file (what the player plays).
pub fn media_kind(kind: i32) -> bool {
    use gezik_core::kind::Kind;
    kind == Kind::Video.index() || kind == Kind::Audio.index()
}

/// The video or sound file `target` is, if it is one.
fn media_path(target: &Target) -> Option<PathBuf> {
    match target {
        Target::Entry { path, is_dir: false, kind, .. } if media_kind(*kind) => Some(path.clone()),
        _ => None,
    }
}

/// How often the bar's time moves while playing.
const TICK: Duration = Duration::from_millis(250);

/// Where downloaded tools are (as `archives.rs` says).
fn data_dir() -> PathBuf {
    crate::windows::config().map_or_else(|| std::env::temp_dir().join("gezik").join("tools"), |store| store.tools_dir())
}

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
    let Target::Entry { path, is_dir, counts, kind, .. } = target else { return Body::None };
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
    // A picture Gezik does not decode (over its limits) still says its own size: the zoom bar
    // tells why it does not zoom (spec 13 §5.3).
    let mut own_size = None;
    if local && gezik_platform::can_decode(&ext) {
        match gezik_platform::decode_image(path, px) {
            Ok(decoded) => {
                if let Some(picture) = buffer(decoded.image) {
                    return Body::Picture(picture, Some((decoded.width, decoded.height)));
                }
            }
            Err(_) => own_size = gezik_platform::picture_size(path),
        }
    }
    if !wanted() {
        return Body::None;
    }
    // A PDF is never shown as text (an uncompressed one is all ASCII): its pages are.
    if local
        && ext != "pdf"
        && let Some(text) = read_text(path).filter(|text| !text.starts_with("%PDF-"))
    {
        return Body::Text(text);
    }
    if !wanted() {
        return Body::None;
    }
    // A sound file's own cover is its poster (spec 13 §6.1).
    if local
        && *kind == gezik_core::kind::Kind::Audio.index()
        && let Some(cover) = gezik_batch::fields::cover(path)
        && let Some(picture) = gezik_platform::decode_image_bytes(&cover, px).ok().and_then(|d| buffer(d.image))
    {
        return Body::Picture(picture, None);
    }
    let mut picture = gezik_platform::thumbnail_while(path, px, wanted);
    if picture.is_none() && wanted() {
        // A cloud-only file's own icon would be read from its data: its type's icon instead.
        let target = if local { IconTarget::Path(path.clone()) } else { IconTarget::Extension(ext) };
        picture = gezik_platform::icon(&target, px);
    }
    match picture.and_then(buffer) {
        Some(picture) => Body::Picture(picture, own_size),
        None => Body::None,
    }
}

/// The same file or folder (its size and dates may have changed).
fn same_item(a: &Target, b: &Target) -> bool {
    match (a, b) {
        (Target::Entry { path: p, is_dir: d, .. }, Target::Entry { path: q, is_dir: e, .. }) => p == q && d == e,
        _ => a == b,
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
fn start_worker(current: Arc<AtomicU64>) -> Arc<Slot<Request>> {
    let slot = Arc::new(Slot::default());
    let requests = slot.clone();
    let window_id = crate::windows::current();
    let spawned = std::thread::Builder::new().name("gezik-preview".into()).spawn(move || {
        gezik_platform::init_thread();
        loop {
            let Request { target, px, generation } = requests.take();
            let wanted = || current.load(Ordering::SeqCst) == generation;
            let body = load_while(&target, px, &wanted);
            if !wanted() {
                continue;
            }
            let fields_of = fields_wanted(&target, &body);
            let (posted, current_) = (target.clone(), current.clone());
            crate::windows::post(window_id, move || {
                if current_.load(Ordering::SeqCst) == generation {
                    with_current(|p| p.loaded(posted, body));
                }
            });
            // Then the file's fields, apart (the picture does not wait for them; spec 13 §7.4).
            let Some((path, want)) = fields_of else { continue };
            let found = gezik_batch::fields::read(&path, want, &|| !wanted());
            if !wanted() || found.is_empty() {
                continue;
            }
            let current = current.clone();
            crate::windows::post(window_id, move || {
                if current.load(Ordering::SeqCst) == generation {
                    with_current(|p| p.fields_loaded(&target, &found));
                }
            });
        }
    });
    if let Err(err) = spawned {
        eprintln!("gezik: cannot start the preview thread: {err}");
    }
    slot
}

/// The file whose fields the pane shows, and which: none for folders or with `[preview]
/// properties = false`; a picture Gezik decoded says its size already.
fn fields_wanted(target: &Target, body: &Body) -> Option<(PathBuf, gezik_core::fields::FieldSet)> {
    use gezik_core::fields::{self as f, FieldSet};
    let Target::Entry { path, is_dir: false, .. } = target else { return None };
    let sized = matches!(body, Body::Picture(_, Some(_)));
    let want =
        if sized { FieldSet::ALL.without(FieldSet::of(&[f::WIDTH, f::HEIGHT, f::DIMENSIONS])) } else { FieldSet::ALL };
    fields::properties().then(|| (path.clone(), want))
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
    gezik_core::group_digits(n as u64)
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
                // A PDF offers its pages (spec 13 §5).
                page: i32::from(pdf_path(target).is_some()),
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
                // The viewer shows it (`Preview::loaded`).
                Some(Body::Text(_)) => info.kind = 2,
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
    pub(crate) static CURRENT: crate::windows::Slot<Preview> = const { crate::windows::Slot::new() };
}

/// Runs `f` with this UI thread's preview, if there is one.
pub fn with_current<R>(f: impl FnOnce(&Preview) -> R) -> Option<R> {
    let preview = CURRENT.with(crate::windows::Slot::current)?;
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
    /// The shown file's viewer, and the target it is for (spec 13 §3.1).
    viewer: RefCell<Viewer>,
    viewer_target: RefCell<Target>,
    /// The viewer's room in this window and in quick look.
    room: Cell<Room>,
    room_quick: Cell<Room>,
    /// Text and code wrap (`state.toml [preview] wrap-text`, `wrap-code`).
    wrap: Cell<(bool, bool)>,
    /// Where the pane's viewer was when quick look opened: it goes back there (quick look
    /// scrolls on its own).
    pane_place: RefCell<Option<(Target, Place)>>,
    /// The zoom surface's size (logical) in this window and in quick look.
    surface: Cell<(f32, f32)>,
    surface_quick: Cell<(f32, f32)>,
    /// The shown picture's own size, once loaded (what zooming it starts from).
    picture_size: Cell<Option<(u32, u32)>>,
    /// The PDF view worker, started with the first PDF page or zoom; it stays 10 s after the
    /// preview leaves PDFs (`park_timer`).
    pdf_worker: RefCell<Option<pdf::Worker>>,
    park_timer: slint::Timer,
    /// Draws the PDF's region once a drag or zoom rests.
    settle_timer: slint::Timer,
    /// When the wheel last turned a page.
    turned: Cell<Option<Instant>>,
    /// A picture is being decoded at its own size, and the one to decode after it.
    decoding: Cell<bool>,
    decode_next: RefCell<Option<PathBuf>>,
    /// The player's volume 0-100 and mute (`state.toml [preview] volume`, `muted`), and the
    /// gain its sound threads read.
    volume: Cell<(u8, bool)>,
    gain: Arc<AtomicU32>,
    /// Moves the bar's time while playing; ends a long pause's processes.
    tick_timer: slint::Timer,
    pause_timer: slint::Timer,
    /// The shown file's property rows and length (ms; the player's bar before it plays).
    properties: RefCell<(slint::ModelRc<PropRow>, Option<u64>)>,
}

#[derive(Clone)]
pub struct Preview(Rc<Inner>);

impl Preview {
    pub fn new(window: &AppWindow, wrap: (bool, bool), volume: (u8, bool)) -> Preview {
        let preview = Preview(Rc::new(Inner {
            window: window.as_weak(),
            pane_open: Cell::new(false),
            generation: Arc::default(),
            worker: RefCell::new(None),
            timer: slint::Timer::default(),
            info: RefCell::new(PreviewInfo::default()),
            quick_look: RefCell::new(None),
            viewer: RefCell::default(),
            viewer_target: RefCell::new(Target::Nothing),
            room: Cell::new((80, 40)),
            room_quick: Cell::new((80, 40)),
            wrap: Cell::new(wrap),
            pane_place: RefCell::new(None),
            surface: Cell::new((400.0, 400.0)),
            surface_quick: Cell::new((400.0, 400.0)),
            picture_size: Cell::new(None),
            pdf_worker: RefCell::new(None),
            park_timer: slint::Timer::default(),
            settle_timer: slint::Timer::default(),
            turned: Cell::new(None),
            decoding: Cell::new(false),
            decode_next: RefCell::new(None),
            volume: Cell::new(volume),
            gain: Arc::new(AtomicU32::new(media::gain(volume.0, volume.1))),
            tick_timer: slint::Timer::default(),
            pause_timer: slint::Timer::default(),
            properties: RefCell::default(),
        }));
        CURRENT.with(|c| c.set(crate::windows::current(), preview.clone()));
        // Its own preview, not the current window's: a viewer made while a frame is drawn says
        // its room then, when no window may be current.
        let weak = Rc::downgrade(&preview.0);
        let here = move |f: &dyn Fn(&Preview) -> bool| weak.upgrade().is_some_and(|inner| f(&Preview(inner)));
        let actions = window.global::<PreviewActions>();
        actions.on_resized({
            let here = here.clone();
            move |cols, rows| {
                _ = here(&|p| {
                    p.resized(false, cols, rows);
                    true
                })
            }
        });
        actions.on_scrolled({
            let here = here.clone();
            move |rows| {
                _ = here(&|p| {
                    p.scroll_viewer(rows);
                    true
                })
            }
        });
        actions.on_jumped({
            let here = here.clone();
            move |fraction| {
                _ = here(&|p| {
                    p.jump_viewer(fraction);
                    true
                })
            }
        });
        actions.on_toggle_hex({
            let here = here.clone();
            move || _ = here(&Preview::toggle_hex)
        });
        actions.on_toggle_wrap({
            let here = here.clone();
            move || {
                _ = here(&|p| {
                    p.toggle_wrap();
                    true
                })
            }
        });
        let run = here.clone();
        connect_surface(
            &actions,
            false,
            Rc::new(move |f| {
                _ = run(&|p| {
                    f(p);
                    true
                })
            }),
        );
        actions.on_key(move |event| {
            let m = event.modifiers;
            here(&|p| p.viewer_key(&event.text, (m.control, m.alt, m.shift, m.meta), true))
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
            if self.0.info.borrow().kind != 0 {
                self.release();
            }
            return;
        }
        self.0.timer.start(
            slint::TimerMode::SingleShot,
            DELAY,
            crate::windows::bind_mut(|| {
                with_current(Preview::refresh);
            }),
        );
    }

    /// Shows the selection's facts now and loads its picture or text in the background.
    pub fn refresh(&self) {
        if !self.active() {
            return;
        }
        let target = crate::panes::active_view().preview_target();
        // Another file: what its viewer holds goes (spec 13 §3.1). The same one keeps its place
        // (the pane was resized, the list refreshed, a log grew).
        if !same_item(&self.0.viewer_target.borrow(), &target) {
            self.drop_viewer();
        }
        *self.0.viewer_target.borrow_mut() = target.clone();
        let generation = self.0.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.publish(describe(&target, None));
        if !matches!(target, Target::Entry { .. }) {
            return;
        }
        let px = self.picture_px();
        let worker = self.0.worker.borrow_mut().get_or_insert_with(|| start_worker(self.0.generation.clone())).clone();
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
        self.publish_to(info, true);
    }

    /// Shows `info`, in this window too unless `main` is false (quick look scrolled).
    fn publish_to(&self, mut info: PreviewInfo, main: bool) {
        let (volume, muted) = self.0.volume.get();
        (info.volume, info.muted) = (f32::from(volume) / 100.0, muted);
        let (properties, length) = self.0.properties.borrow().clone();
        info.properties = properties;
        // Before playing, the bar says the length the fields found.
        let length = |info: &mut PreviewInfo| {
            if let (6, Some(ms)) = (info.bar, length)
                && info.bar_text == "--:--"
            {
                info.bar_text = gezik_core::fields::duration_text(ms).into();
            }
        };
        // Kept without the viewer's part: the next fill starts from the file's own preview
        // (a zoomed picture's large copy is not held here once it is back to fit).
        let mut shown = info.clone();
        let quick = self.quick_look_open();
        let narrow = |cell: &Cell<(f32, f32)>| cell.get().0 < NARROW;
        // A narrow bar drops some buttons and the volume bar (the right-click menu has them).
        let tools = |info: &mut PreviewInfo, cell| {
            info.tools = viewer::bar_tools(info, narrow(cell));
            if narrow(cell) {
                info.volume = -1.0;
            }
        };
        self.0.viewer.borrow().fill(&mut shown, self.room(), true);
        length(&mut shown);
        tools(&mut shown, if quick { &self.0.surface_quick } else { &self.0.surface });
        // A picture over Gezik's limits says so before any zoom.
        if shown.bar == 5
            && shown.bar_note.is_empty()
            && let Some(why) = self.0.picture_size.get().and_then(viewer::too_large)
        {
            shown.bar_note = why.into();
        }
        if main && let Some(window) = self.0.window.upgrade() {
            if quick {
                // Behind quick look the pane keeps the file's own look: the zoom placed for
                // quick look's size would show stretched there.
                let mut pane = info.clone();
                self.0.viewer.borrow().fill(&mut pane, self.room(), false);
                length(&mut pane);
                tools(&mut pane, &self.0.surface);
                window.set_preview(pane);
            } else {
                window.set_preview(shown.clone());
            }
        }
        if let Some(quick_look) = self.0.quick_look.borrow().as_ref() {
            quick_look.set_info(shown);
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
        slint::Timer::single_shot(
            Duration::from_millis(500),
            crate::windows::bind(|| {
                if gezik_platform::ql_panel::is_open() {
                    return;
                }
                with_current(|p| {
                    if let Some(window) = p.0.window.upgrade() {
                        window.set_status("The system Quick Look panel did not open; Gezik's own is shown".into());
                    }
                    p.open_own_quick_look();
                });
            }),
        );
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
        // Its window's events are no events of a Gezik window: they act in this one.
        let id = crate::windows::current();
        // One quick look in the process (deviation 15): another window's closes.
        for (other, preview) in CURRENT.with(crate::windows::Slot::all) {
            if other != id {
                crate::windows::enter(other, || preview.close_quick_look());
            }
        }
        let opened = crate::quick_look::QuickLook::open(
            &window,
            info,
            window.get_mono_font(),
            move |text, ctrl, alt, shift, meta| {
                crate::windows::with_window(id, |_| with_current(|p| p.quick_look_key(text, ctrl, alt, shift, meta)))
                    .flatten()
                    .unwrap_or(false)
            },
            move || {
                crate::windows::with_window(id, |_| Preview::close_later());
            },
        );
        match opened {
            Ok(quick_look) => {
                let place = self.0.viewer.borrow().place();
                *self.0.pane_place.borrow_mut() = Some((self.0.viewer_target.borrow().clone(), place));
                // Its viewer's events act in this window's preview.
                let actions = quick_look.viewer_actions();
                let here = move |f: &dyn Fn(&Preview)| {
                    crate::windows::with_window(id, |_| with_current(f));
                };
                actions.on_resized(move |cols, rows| here(&|p| p.resized(true, cols, rows)));
                actions.on_scrolled(move |rows| here(&|p| p.scroll_viewer(rows)));
                actions.on_jumped(move |fraction| here(&|p| p.jump_viewer(fraction)));
                actions.on_toggle_hex(move || here(&|p| _ = p.toggle_hex()));
                actions.on_toggle_wrap(move || here(&Preview::toggle_wrap));
                connect_surface(&actions, true, Rc::new(here));
                actions.on_key(move |event| {
                    let m = event.modifiers;
                    crate::windows::with_window(id, |_| {
                        with_current(|p| p.quick_look_key(&event.text, m.control, m.alt, m.shift, m.meta))
                    })
                    .flatten()
                    .unwrap_or(false)
                });
                // Its viewer may have said its room before anyone listened.
                actions.set_ask(actions.get_ask() + 1);
                *self.0.quick_look.borrow_mut() = Some(quick_look);
                self.refresh();
                self.sync_view();
                self.sync_frame();
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
        if let Some((target, place)) = self.0.pane_place.take()
            && same_item(&target, &self.0.viewer_target.borrow())
        {
            self.0.viewer.borrow_mut().set_place(place);
        }
        if !self.active() {
            self.release();
        } else if self.0.viewer.borrow().shown() {
            // Back to this window's room.
            self.sync_view();
            self.sync_frame();
            self.redraw();
        }
    }

    /// Closes quick look once the current event is done: its window may be the one
    /// handling it.
    fn close_later() {
        slint::Timer::single_shot(
            Duration::ZERO,
            crate::windows::bind(|| {
                with_current(Preview::close_quick_look);
            }),
        );
    }

    /// A key in the quick look window: Space or Esc closes it, arrows move in the list.
    fn quick_look_key(&self, text: &str, ctrl: bool, alt: bool, shift: bool, meta: bool) -> bool {
        use gezik_config::shortcuts::{Key, Platform};
        use gezik_core::layout::Move;
        if self.viewer_key(text, (ctrl, alt, shift, meta), false) {
            return true;
        }
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
        self.drop_viewer();
        *self.0.viewer_target.borrow_mut() = Target::Nothing;
        self.publish(PreviewInfo::default());
    }

    /// The worker's result: text gets its viewer, from the start the worker read.
    fn loaded(&self, target: Target, mut body: Body) {
        if let (Body::Text(head), Target::Entry { path, size, .. }) = (&mut body, &target)
            && self.0.viewer.borrow().text.is_none()
            && same_item(&self.0.viewer_target.borrow(), &target)
        {
            let size = size.unwrap_or(head.len() as u64);
            let view = text::TextView::from_head(path, size, std::mem::take(head), self.0.wrap.get());
            self.0.viewer.borrow_mut().text = Some(view);
        }
        if let Body::Picture(_, Some(size)) = &body
            && same_item(&self.0.viewer_target.borrow(), &target)
        {
            self.0.picture_size.set(Some(*size));
        }
        self.publish(describe(&target, Some(body)));
    }

    /// The worker read the shown file's fields: its property rows show under its facts.
    fn fields_loaded(&self, target: &Target, found: &gezik_core::fields::Fields) {
        if !same_item(&self.0.viewer_target.borrow(), target) {
            return;
        }
        let length = match found.get(gezik_core::fields::DURATION) {
            Some(gezik_core::fields::Value::Duration(ms)) => Some(*ms),
            _ => None,
        };
        *self.0.properties.borrow_mut() = (fields::model(fields::rows(found)), length);
        let info = self.0.info.borrow().clone();
        self.publish(info);
    }

    /// The viewer's room where it is seen: quick look's while it is open.
    fn room(&self) -> Room {
        if self.quick_look_open() { self.0.room_quick.get() } else { self.0.room.get() }
    }

    /// Shows the viewer's rows again (it scrolled, its room changed).
    fn redraw(&self) {
        let info = self.0.info.borrow().clone();
        self.publish_to(info, !self.quick_look_open());
    }

    /// The viewer has `cols` × `rows` in this window or in quick look.
    fn resized(&self, quick: bool, cols: i32, rows: i32) {
        let room = (usize::try_from(cols).unwrap_or(1).max(1), usize::try_from(rows).unwrap_or(1).max(1));
        let cell = if quick { &self.0.room_quick } else { &self.0.room };
        if cell.replace(room) != room && quick == self.quick_look_open() && self.0.viewer.borrow().shown() {
            self.redraw();
        }
    }

    fn scroll_viewer(&self, rows: i32) {
        let room = self.room();
        self.0.viewer.borrow_mut().scroll(i64::from(rows), room);
        self.redraw();
    }

    fn jump_viewer(&self, fraction: f32) {
        let room = self.room();
        self.0.viewer.borrow_mut().jump(fraction, room);
        self.redraw();
    }

    /// Hex, or back to the text or picture (spec 13 §4.4); whether the shown item is a file.
    pub fn toggle_hex(&self) -> bool {
        let Target::Entry { path, is_dir: false, size, .. } = self.0.viewer_target.borrow().clone() else {
            return false;
        };
        let shown = self.0.viewer.borrow().hex.is_some();
        if shown {
            self.0.viewer.borrow_mut().hex = None;
        } else if gezik_platform::only_in_cloud(&path) {
            // Reading it would download it.
            if let Some(window) = self.0.window.upgrade() {
                window.set_status("This file is only in the cloud: it is not read".into());
            }
            return true;
        } else {
            let size = size.or_else(|| std::fs::metadata(&path).ok().map(|m| m.len())).unwrap_or(0);
            let at = self.0.viewer.borrow().text.as_ref().map_or(0, text::TextView::file_offset);
            self.0.viewer.borrow_mut().hex = Some(hex::HexView::new(&path, size).starting_at(at));
        }
        self.redraw();
        true
    }

    /// `preview-hex`: the selected file as hex (the pane opens if nothing shows the preview).
    pub fn show_hex(&self) {
        if !self.active() {
            self.set_pane_open(true);
        }
        self.toggle_hex();
    }

    /// Wrap on or off for the shown text; remembered for text or code (state.toml).
    pub fn toggle_wrap(&self) {
        let (mut wrap_text, mut wrap_code) = self.0.wrap.get();
        {
            let mut viewer = self.0.viewer.borrow_mut();
            let Some(view) = viewer.text.as_mut() else { return };
            view.set_wrap(!view.wrap);
            if view.is_code() {
                wrap_code = view.wrap;
            } else {
                wrap_text = view.wrap;
            }
        }
        self.0.wrap.set((wrap_text, wrap_code));
        if let Some(store) = crate::windows::config() {
            store.update_state(|state| state.preview_wrap = Some((wrap_text, wrap_code)));
        }
        self.redraw();
    }

    /// For the preview's menu: hex is shown, the item is a file, the text's wrap.
    pub fn viewer_state(&self) -> (bool, bool, Option<bool>) {
        let viewer = self.0.viewer.borrow();
        let file = matches!(*self.0.viewer_target.borrow(), Target::Entry { is_dir: false, .. });
        let wrap = viewer.text.as_ref().filter(|_| viewer.hex.is_none()).map(|t| t.wrap);
        (viewer.hex.is_some(), file, wrap)
    }

    /// For the preview's menu: the zoom surface shown (4 a PDF, 5 a picture, 6 a player, 0
    /// none) and the bar's link ("Download", "Enter Password…"; "" none).
    pub fn surface_state(&self) -> (i32, &'static str) {
        if media_path(&self.0.viewer_target.borrow()).is_some() && self.0.viewer.borrow().hex.is_none() {
            let link = self.0.viewer.borrow().media.as_ref().map_or("", |player| player.note().1);
            return (6, link);
        }
        if !self.surface_target() {
            return (0, "");
        }
        let link = self.0.viewer.borrow().pdf.as_ref().map_or("", |pdf| pdf.note().1);
        (if pdf_path(&self.0.viewer_target.borrow()).is_some() { 4 } else { 5 }, link)
    }

    /// The viewer's own keys (spec 13 §3.2): Page Up/Down, Home/End, H, W; in the pane also
    /// the arrows, and Esc gives the keyboard back to the list.
    fn viewer_key(&self, text: &str, (ctrl, alt, shift, meta): (bool, bool, bool, bool), pane: bool) -> bool {
        use gezik_config::shortcuts::{Key, Platform};
        let Some(chord) = crate::keys::chord_from_slint(text, ctrl, alt, shift, meta, Platform::current()) else {
            return false;
        };
        // The zoom keys by what they type (`+` is a shifted key on many layouts).
        if !(chord.ctrl || chord.alt || chord.meta) && self.surface_target() {
            match text {
                "+" | "=" => return self.zoom_step(1, None),
                "-" | "_" => return self.zoom_step(-1, None),
                "0" => return self.fit_page(),
                "1" => return self.zoom_actual(None),
                _ => {}
            }
        }
        if chord.ctrl || chord.alt || chord.meta || chord.shift {
            return false;
        }
        let pdf = self.0.viewer.borrow().hex.is_none() && pdf_path(&self.0.viewer_target.borrow()).is_some();
        let media = self.0.viewer.borrow().hex.is_none() && media_path(&self.0.viewer_target.borrow()).is_some();
        match chord.key {
            Key::Char('k') if media => self.play_pause(),
            Key::Char('j') if media => self.skip(-media::SKIP),
            Key::Char('l') if media => self.skip(media::SKIP),
            Key::Char('m') if media => {
                self.toggle_mute();
                true
            }
            Key::PageDown if pdf => self.pdf_step(1, true),
            Key::PageUp if pdf => self.pdf_step(-1, true),
            Key::Home if pdf => self.pdf_step(-PAGE_ENDS, true),
            Key::End if pdf => self.pdf_step(PAGE_ENDS, true),
            Key::Char('h') => self.toggle_hex(),
            Key::Char('w') if self.viewer_state().2.is_some() => {
                self.toggle_wrap();
                true
            }
            Key::Escape if pane => {
                if let Some(window) = self.0.window.upgrade() {
                    window.invoke_focus_list();
                }
                true
            }
            key => {
                let room = self.room();
                let used = self.0.viewer.borrow_mut().key(key, pane, room);
                if used {
                    self.redraw();
                }
                used
            }
        }
    }
}

/// A step past either end of any document (Home, End).
const PAGE_ENDS: i64 = 1 << 40;

/// Runs its argument with this window's preview.
type WithPreview = Rc<dyn Fn(&dyn Fn(&Preview))>;

/// The zoom surface's events (spec 13 §5), from this window's pane or from quick look.
fn connect_surface(actions: &PreviewActions, quick: bool, run: WithPreview) {
    actions.on_surface(move |what, x, y, dx, dy| run(&|p| p.surface_event(quick, what, (x, y), (dx, dy))));
}

/// The PDF pages and the zoom surface (spec 13 §5).
impl Preview {
    /// One of the zoom surface's events (`PreviewActions.surface` says what each is).
    fn surface_event(&self, quick: bool, what: i32, (x, y): (f32, f32), (dx, dy): (f32, f32)) {
        let steps = if dx < 0.0 { -1 } else { 1 };
        match what {
            0 => self.surface_resized(quick, x, y),
            1 => _ = self.pdf_step(i64::from(steps), true),
            2 => self.ask_page(),
            3 => _ = self.zoom_step(steps, (x >= 0.0).then_some((x, y))),
            4 => self.fit(),
            5 => _ = self.zoom_actual((x >= 0.0).then_some((x, y))),
            6 => self.pan(dx, dy),
            7 | 8 => self.wheel((dx, dy), what == 8, (x, y)),
            9 => self.bar_link(),
            10 => _ = self.play_pause(),
            11 => self.seek_fraction(x, true),
            12 => self.seek_fraction(x, false),
            13 => self.toggle_mute(),
            14 => self.set_volume(x, dx > 0.0),
            _ => {}
        }
    }

    /// The preview's menu: a PDF's page and the zoom, the player (ids 2603–2614,
    /// `context_menu.rs`).
    pub fn surface_menu(&self, id: u32) {
        match id {
            crate::context_menu::PREVIEW_PREVIOUS_PAGE => _ = self.pdf_step(-1, true),
            crate::context_menu::PREVIEW_NEXT_PAGE => _ = self.pdf_step(1, true),
            crate::context_menu::PREVIEW_GO_TO_PAGE => self.ask_page(),
            crate::context_menu::PREVIEW_ZOOM_IN => _ = self.zoom_step(1, None),
            crate::context_menu::PREVIEW_ZOOM_OUT => _ = self.zoom_step(-1, None),
            crate::context_menu::PREVIEW_FIT => self.fit(),
            crate::context_menu::PREVIEW_ACTUAL_SIZE => _ = self.zoom_actual(None),
            crate::context_menu::PREVIEW_LINK => self.bar_link(),
            crate::context_menu::PREVIEW_PLAY => _ = self.play_pause(),
            crate::context_menu::PREVIEW_BACK => _ = self.skip(-media::SKIP),
            crate::context_menu::PREVIEW_FORWARD => _ = self.skip(media::SKIP),
            crate::context_menu::PREVIEW_MUTE => self.toggle_mute(),
            _ => {}
        }
    }

    /// "Go to Page…": asks for a page of the shown PDF (the window's question; no text field
    /// of the preview's own takes keys from the list).
    fn ask_page(&self) {
        if !self.pdf_ready() {
            return;
        }
        let (page, pages) = self.0.viewer.borrow().pdf.as_ref().map_or((1, 0), |pdf| (pdf.page, pdf.pages));
        let message = if pages > 0 { format!("Page (1 to {pages}):") } else { "Page:".to_owned() };
        crate::operations::with_current(|ops| {
            ops.dialogs().ask_text("Go to Page", message, page.to_string(), &["Go", "Cancel"], |text| {
                if let Some(text) = text {
                    with_current(|p| p.pdf_go_to(&text));
                }
            });
        });
    }

    /// Lets go of the shown file's viewer; a PDF's worker waits 10 s for the next PDF.
    fn drop_viewer(&self) {
        *self.0.properties.borrow_mut() = Default::default();
        let old = std::mem::take(&mut *self.0.viewer.borrow_mut());
        self.0.picture_size.set(None);
        self.0.settle_timer.stop();
        if old.pdf.is_some() {
            self.park_pdf();
        }
        if old.media.is_some() {
            // Its processes end with it (its session); its frames go too.
            self.0.tick_timer.stop();
            self.0.pause_timer.stop();
            drop(old);
            self.sync_frame();
        }
    }

    fn park_pdf(&self) {
        {
            let mut worker = self.0.pdf_worker.borrow_mut();
            let Some(worker) = worker.as_mut() else { return };
            if worker.parked.is_none() {
                // Its document's memory goes now; the process when the time is up.
                worker.send(Command::Close);
                worker.parked = Some(Instant::now());
            }
        }
        // A little past the time: a timer that fires a moment early still finds it over.
        self.0.park_timer.start(
            slint::TimerMode::SingleShot,
            pdf::PARK + Duration::from_millis(50),
            crate::windows::bind_mut(|| {
                with_current(Preview::park_check);
            }),
        );
    }

    fn park_check(&self) {
        let parked = self.0.pdf_worker.borrow().as_ref().and_then(|w| w.parked);
        if self.0.viewer.borrow().pdf.is_none() && pdf::park_over(parked, Instant::now()) {
            *self.0.pdf_worker.borrow_mut() = None;
        }
    }

    /// Physical pixels per logical one where the preview is seen.
    fn scale_factor(&self) -> f32 {
        if let Some(quick_look) = self.0.quick_look.borrow().as_ref() {
            return quick_look.scale_factor();
        }
        self.0.window.upgrade().map_or(1.0, |w| w.window().scale_factor())
    }

    /// The zoom surface's size in physical pixels where it is seen.
    fn surface_px(&self) -> (f32, f32) {
        let (w, h) = if self.quick_look_open() { self.0.surface_quick.get() } else { self.0.surface.get() };
        let sf = self.scale_factor();
        (w * sf, h * sf)
    }

    fn surface_resized(&self, quick: bool, w: f32, h: f32) {
        let cell = if quick { &self.0.surface_quick } else { &self.0.surface };
        let old = cell.replace((w, h));
        if old == (w, h) {
            return;
        }
        if quick == self.quick_look_open() && self.0.viewer.borrow().shown() {
            self.sync_view();
        } else if (old.0 < NARROW) != (w < NARROW) {
            // The bar narrows or widens.
            self.redraw();
        }
    }

    /// The PDF or picture follows the surface's size where it is seen now.
    // shortcut: a PDF's 100% keeps the scale it was made at; a quick look on a screen of
    // another scale shows it larger or smaller (remake the view's unit if that matters).
    fn sync_view(&self) {
        let (view, sf) = (self.surface_px(), self.scale_factor());
        {
            let mut v = self.0.viewer.borrow_mut();
            let v = &mut *v;
            if let Some(pdf) = v.pdf.as_mut() {
                pdf.zoom.set_view(view);
                pdf.scale_factor = sf;
            } else if let Some(picture) = v.picture.as_mut() {
                picture.zoom.set_view(view);
                picture.scale_factor = sf;
            } else {
                return;
            }
        }
        self.redraw();
        self.settle();
    }

    /// Draws the PDF's region once things rest.
    fn settle(&self) {
        if self.0.viewer.borrow().pdf.is_some() {
            self.0.settle_timer.start(
                slint::TimerMode::SingleShot,
                SETTLE,
                crate::windows::bind_mut(|| {
                    with_current(Preview::pdf_render);
                }),
            );
        }
    }

    /// The shown item is a PDF or a picture, with nothing else (text, hex) over it.
    fn surface_target(&self) -> bool {
        let v = self.0.viewer.borrow();
        v.hex.is_none()
            && v.text.is_none()
            && (pdf_path(&self.0.viewer_target.borrow()).is_some() || self.0.picture_size.get().is_some())
    }

    /// The shown PDF's viewer, made on its first use; false when the shown item is no PDF.
    fn pdf_ready(&self) -> bool {
        if self.0.viewer.borrow().pdf.is_some() {
            return true;
        }
        let Some(path) = pdf_path(&self.0.viewer_target.borrow()) else { return false };
        if self.0.viewer.borrow().hex.is_some() {
            return false;
        }
        if gezik_platform::only_in_cloud(&path) {
            // Reading it would download it.
            if let Some(window) = self.0.window.upgrade() {
                window.set_status("This file is only in the cloud: it is not read".into());
            }
            return false;
        }
        let view = PdfView::new(path.clone(), self.scale_factor(), self.surface_px());
        self.0.viewer.borrow_mut().pdf = Some(view);
        self.find_pdfium(path);
        true
    }

    /// Looks for pdfium off the UI thread (a file system look; the hint runs programs).
    fn find_pdfium(&self, path: PathBuf) {
        let data = data_dir();
        let window_id = crate::windows::current();
        std::thread::spawn(move || {
            let library = gezik_batch::tools::find(gezik_core::batch::tools::Tool::Pdfium, &data, None);
            let hint = if library.is_none() { gezik_platform::http::tool_missing_hint() } else { None };
            crate::windows::post(window_id, move || {
                with_current(|p| p.pdfium_found(&path, library, hint));
            });
        });
    }

    fn pdfium_found(&self, path: &Path, library: Option<PathBuf>, hint: Option<String>) {
        let open = {
            let mut v = self.0.viewer.borrow_mut();
            let Some(pdf) = v.pdf.as_mut().filter(|pdf| pdf.path == path) else { return };
            match library {
                Some(library) => {
                    pdf.state = State::Opening;
                    Some(Command::Open { library, pdf: path.to_path_buf() })
                }
                None => {
                    pdf.set_stuck(State::NoPdfium(hint));
                    None
                }
            }
        };
        if let Some(open) = open {
            self.pdf_send(open);
        }
        self.redraw();
    }

    /// Sends `command` for the shown PDF, starting the worker if there is none.
    fn pdf_send(&self, command: Command) {
        self.0.park_timer.stop();
        let id = {
            let mut worker = self.0.pdf_worker.borrow_mut();
            if worker.is_none() {
                *worker = self.start_pdf_worker();
            }
            worker.as_mut().and_then(|w| {
                w.parked = None;
                w.send(command)
            })
        };
        if id.is_none() {
            *self.0.pdf_worker.borrow_mut() = None;
        }
        if let Some(pdf) = self.0.viewer.borrow_mut().pdf.as_mut() {
            match id {
                Some(id) => pdf.wanted = id,
                None => pdf.set_stuck(State::Failed),
            }
        }
    }

    fn start_pdf_worker(&self) -> Option<pdf::Worker> {
        let window_id = crate::windows::current();
        let deliver = move |id, event| {
            crate::windows::post(window_id, move || {
                with_current(|p| p.pdf_event(id, event));
            });
        };
        let started = std::env::current_exe()
            .and_then(|exe| pdf::Worker::start(&exe, &[gezik_batch::pdf::view::VIEW_ARG], pdf::LIMIT, deliver));
        started.map_err(|err| eprintln!("gezik: cannot start the PDF view worker: {err}")).ok()
    }

    /// What the worker said: for the newest command of the shown PDF, or that it stopped.
    fn pdf_event(&self, id: u64, event: pdf::Event) {
        let mut render = false;
        if let pdf::Event::Died(why) = &event {
            eprintln!("gezik: the PDF view worker stopped: {why}");
            // Only the one that stopped (a new one may have started since): the PDF it had
            // open failed; another shown PDF opens again with a new worker.
            let dead = self.0.pdf_worker.borrow().as_ref().filter(|w| w.ended()).map(|w| w.opened.clone());
            let Some(opened) = dead else { return };
            *self.0.pdf_worker.borrow_mut() = None;
            let again = self.0.viewer.borrow_mut().pdf.as_mut().and_then(|pdf| {
                let again = pdf.worker_died(opened.as_deref());
                again.then(|| pdf.path.clone())
            });
            if let Some(path) = again {
                self.find_pdfium(path);
            }
            return self.redraw();
        }
        {
            let mut v = self.0.viewer.borrow_mut();
            let Some(pdf) = v.pdf.as_mut().filter(|pdf| pdf.wanted == id) else {
                drop(v);
                return self.redraw();
            };
            match event {
                pdf::Event::Reply(Reply::Pages(pages)) => {
                    pdf.pages = pages;
                    pdf.page = pdf.page.min(pages);
                    pdf.state = State::Ready;
                    render = true;
                }
                pdf::Event::Reply(Reply::NeedsPassword) => pdf.state = State::Password { wrong: false },
                pdf::Event::Reply(Reply::WrongPassword) => pdf.state = State::Password { wrong: true },
                pdf::Event::Reply(Reply::Failed(why)) => {
                    eprintln!("gezik: the PDF could not be shown: {why}");
                    if crate::pdf::says_pdfium_failed(&why) {
                        // A broken library: ended, so a new download can replace its file.
                        pdf.set_stuck(State::BrokenPdfium);
                        *self.0.pdf_worker.borrow_mut() = None;
                    } else if pdf.state == State::Ready {
                        // One page (a damaged one, a scale out of range): the rest still shows.
                        pdf.page_failed = true;
                    } else {
                        pdf.set_stuck(State::Failed);
                    }
                }
                pdf::Event::Picture { page_size, picture } => {
                    let first = pdf.sized != pdf.page;
                    pdf.take_picture(page_size, picture);
                    // The page's size is known now: its region at the zoom asked.
                    render = first && pdf.zoom.fit != Fit::Page;
                }
                pdf::Event::Reply(Reply::Image { .. }) | pdf::Event::Died(_) => {}
            }
        }
        if render {
            self.pdf_render();
        }
        self.redraw();
    }

    /// Asks for the shown page's visible region (or the whole page, while its size is not
    /// known).
    fn pdf_render(&self) {
        let command = {
            let mut v = self.0.viewer.borrow_mut();
            let Some(pdf) = v.pdf.as_mut().filter(|pdf| pdf.state == State::Ready) else { return };
            let command = pdf.next_render();
            if let Command::Render { scale, x, y, w, h, .. } = command {
                pdf.asked = (scale, (x, y, w, h));
            }
            command
        };
        self.pdf_send(command);
    }

    /// `step` pages on (the view at the page's top, or its bottom going back with the wheel);
    /// whether the shown item is a PDF.
    fn pdf_step(&self, step: i64, top: bool) -> bool {
        if !self.pdf_ready() {
            return false;
        }
        let moved = self.0.viewer.borrow_mut().pdf.as_mut().is_some_and(|pdf| pdf.step(step, top));
        if moved {
            self.0.settle_timer.stop();
            self.pdf_render();
            self.redraw();
        }
        true
    }

    /// The page typed in the bar's field.
    fn pdf_go_to(&self, text: &str) {
        let Ok(page) = text.trim().parse::<u32>() else { return self.redraw() };
        let now = self.0.viewer.borrow().pdf.as_ref().map_or(1, |pdf| pdf.page);
        if !self.pdf_step(i64::from(page) - i64::from(now), true) || page == now {
            self.redraw();
        }
    }

    /// The shown picture's zoom view, made on first use (its own size decoded on a thread
    /// when not too large); false when the shown item is no loaded picture.
    fn picture_ready(&self) -> bool {
        if self.0.viewer.borrow().picture.is_some() {
            return true;
        }
        let Some(size) = self.0.picture_size.get() else { return false };
        let Target::Entry { path, .. } = self.0.viewer_target.borrow().clone() else { return false };
        let view = PictureView::new(size, self.scale_factor(), self.surface_px());
        let decode = view.too_large.is_none();
        self.0.viewer.borrow_mut().picture = Some(view);
        if decode {
            self.decode_full(path);
        }
        true
    }

    /// Decodes `path` at its own size on a thread, one at a time: while one runs, only the
    /// newest picture waits (older ones are skipped).
    fn decode_full(&self, path: PathBuf) {
        if self.0.decoding.replace(true) {
            *self.0.decode_next.borrow_mut() = Some(path);
            return;
        }
        let window_id = crate::windows::current();
        std::thread::spawn(move || {
            // Straight into the picture's buffer: no third copy.
            let full =
                gezik_platform::decode_image_into(&path, SharedPixelBuffer::<Rgba8Pixel>::new, |b| b.make_mut_bytes());
            crate::windows::post(window_id, move || {
                with_current(|p| p.picture_decoded(&path, full.ok()));
            });
        });
    }

    fn picture_decoded(&self, path: &Path, full: Option<SharedPixelBuffer<Rgba8Pixel>>) {
        self.0.decoding.set(false);
        let shown = |path: &Path| matches!(&*self.0.viewer_target.borrow(), Target::Entry { path: p, .. } if p == path);
        let shown_now = shown(path);
        if let (Some(picture), Some(full)) = (self.0.viewer.borrow_mut().picture.as_mut().filter(|_| shown_now), full) {
            picture.full = Some(Image::from_rgba8(full));
        }
        let next = self.0.decode_next.take();
        let waiting = self.0.viewer.borrow().picture.as_ref().is_some_and(|p| p.full.is_none());
        if let Some(next) = next.filter(|next| waiting && shown(next)) {
            self.decode_full(next);
        }
        self.redraw();
    }

    /// Runs `f` on the shown PDF's or picture's zoom (making its viewer first), then shows it;
    /// whether there is one.
    fn with_zoom(&self, f: impl FnOnce(&mut ZoomView)) -> bool {
        if !(self.surface_target() && (self.pdf_ready() || self.picture_ready())) {
            return false;
        }
        {
            let mut v = self.0.viewer.borrow_mut();
            let v = &mut *v;
            let zoom = match (v.pdf.as_mut(), v.picture.as_mut()) {
                (Some(pdf), _) => &mut pdf.zoom,
                (_, Some(picture)) => &mut picture.zoom,
                _ => return true,
            };
            f(zoom);
        }
        self.redraw();
        self.settle();
        true
    }

    fn zoom_step(&self, steps: i32, at: Option<(f32, f32)>) -> bool {
        let sf = self.scale_factor();
        self.with_zoom(|zoom| {
            let anchor = at.map_or_else(|| zoom.middle(), |(x, y)| (x * sf, y * sf));
            zoom.zoom(steps, anchor);
        })
    }

    /// 100% around `at` (a double click) or the middle.
    fn zoom_actual(&self, at: Option<(f32, f32)>) -> bool {
        let sf = self.scale_factor();
        self.with_zoom(|zoom| {
            let anchor = at.map_or_else(|| zoom.middle(), |(x, y)| (x * sf, y * sf));
            zoom.fit_at(Fit::Zoom(1.0), anchor);
        })
    }

    /// `0`: the whole page; a picture back to its fitted preview (its large copy goes).
    fn fit_page(&self) -> bool {
        if self.0.viewer.borrow_mut().picture.take().is_some() {
            self.redraw();
            return true;
        }
        // Asked first: the borrow must be gone before `with_zoom` borrows it mutably.
        let pdf = self.0.viewer.borrow().pdf.is_some();
        pdf && self.with_zoom(|zoom| zoom.fit_to(Fit::Page))
    }

    /// The bar's Fit: a PDF goes between the whole page and its width; a picture back to fit.
    fn fit(&self) {
        let pdf_fit = self.0.viewer.borrow().pdf.as_ref().map(|pdf| pdf.zoom.fit);
        match pdf_fit {
            Some(Fit::Page) => _ = self.with_zoom(|zoom| zoom.fit_to(Fit::Width)),
            Some(_) => _ = self.with_zoom(|zoom| zoom.fit_to(Fit::Page)),
            None => _ = self.fit_page(),
        }
    }

    /// A drag moved the surface by `dx`, `dy` logical pixels.
    fn pan(&self, dx: f32, dy: f32) {
        let sf = self.scale_factor();
        let moved = {
            let mut v = self.0.viewer.borrow_mut();
            let v = &mut *v;
            match (v.pdf.as_mut(), v.picture.as_mut()) {
                (Some(pdf), _) => pdf.zoom.pan(dx * sf, dy * sf),
                (_, Some(picture)) => picture.zoom.pan(dx * sf, dy * sf),
                _ => false,
            }
        };
        if moved {
            self.redraw();
            self.settle();
        }
    }

    /// The wheel over the surface: with Ctrl (⌘) it zooms around the pointer; on a PDF it
    /// scrolls the page and at its end turns it; on a zoomed picture it scrolls.
    fn wheel(&self, (dx, dy): (f32, f32), zoom: bool, at: (f32, f32)) {
        if zoom {
            if dy != 0.0 {
                self.zoom_step(if dy > 0.0 { 1 } else { -1 }, Some(at));
            }
            return;
        }
        let pdf_scrolls = self.0.viewer.borrow().pdf.as_ref().map(|pdf| pdf.zoom.can_scroll(dy));
        let is_pdf = pdf_path(&self.0.viewer_target.borrow()).is_some();
        if pdf_scrolls == Some(true) || (!is_pdf && self.0.viewer.borrow().picture.is_some()) {
            return self.pan(dx, dy);
        }
        // A PDF's first look (no viewer yet) is not turned by the wheel: nothing starts by itself.
        if !is_pdf || dy == 0.0 || pdf_scrolls.is_none() || self.0.turned.get().is_some_and(|t| t.elapsed() < TURN_GAP)
        {
            return;
        }
        self.0.turned.set(Some(Instant::now()));
        if dy < 0.0 {
            self.pdf_step(1, true);
        } else {
            self.pdf_step(-1, false);
        }
    }

    /// pdfium was downloaded: the shown PDF `path` looks for it again.
    fn look_again(&self, path: &Path) {
        if let Some(pdf) = self.0.viewer.borrow_mut().pdf.as_mut().filter(|pdf| pdf.path == path) {
            pdf.state = State::Finding;
        } else {
            return;
        }
        self.find_pdfium(path.to_path_buf());
        self.redraw();
    }

    /// The bar's link: download pdfium or ffmpeg, or the PDF's password.
    fn bar_link(&self) {
        let no_ffmpeg = self.0.viewer.borrow().media.as_ref().and_then(|player| match &player.state {
            Play::NoFfmpeg(hint) => Some((player.path.clone(), hint.clone())),
            _ => None,
        });
        if let Some((path, hint)) = no_ffmpeg {
            // Downloaded: it plays at once.
            let again = Rc::new(move || {
                with_current(|p| {
                    if let Some(player) = p.0.viewer.borrow_mut().media.as_mut().filter(|pl| pl.path == path) {
                        player.state = Play::Stopped;
                    }
                    p.play_pause();
                });
            });
            let need = crate::archives::Need::Playback;
            crate::archives::with_current(|archives| archives.offer_ffmpeg(need, hint, again, None));
            return;
        }
        let (path, state) = {
            let v = self.0.viewer.borrow();
            let Some(pdf) = v.pdf.as_ref() else { return };
            (pdf.path.clone(), pdf.state.clone())
        };
        match state {
            State::BrokenPdfium => {
                let again = Rc::new(move || with_current(|p| p.look_again(&path)).unwrap_or_default());
                crate::archives::with_current(|archives| archives.offer_pdfium(true, None, again, None));
            }
            State::NoPdfium(hint) => {
                let again = Rc::new(move || with_current(|p| p.look_again(&path)).unwrap_or_default());
                crate::archives::with_current(|archives| archives.offer_pdf_pages(hint, again));
            }
            State::Password { wrong } => {
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let message = if wrong {
                    "Wrong password. Try again:".to_owned()
                } else {
                    format!("{name} is encrypted. Password:")
                };
                crate::operations::with_current(|ops| {
                    ops.dialogs().ask_secret("Password", message, &["Open", "Cancel"], move |text| {
                        let Some(text) = text else { return };
                        with_current(|p| {
                            if let Some(pdf) = p.0.viewer.borrow_mut().pdf.as_mut().filter(|pdf| pdf.path == path) {
                                pdf.state = State::Opening;
                            } else {
                                return;
                            }
                            // Only in the command on its way to the worker; never kept.
                            p.pdf_send(Command::Password(text));
                            p.redraw();
                        });
                    });
                });
            }
            _ => {}
        }
    }
}

/// The player (spec 13 §6).
impl Preview {
    /// The shown video's or sound's player, made on its first use; false when the shown item is
    /// none (or shows as hex).
    fn media_ready(&self) -> bool {
        if self.0.viewer.borrow().hex.is_some() {
            return false;
        }
        if self.0.viewer.borrow().media.is_some() {
            return true;
        }
        let Some(path) = media_path(&self.0.viewer_target.borrow()) else { return false };
        if gezik_platform::only_in_cloud(&path) {
            // Playing it would download it.
            if let Some(window) = self.0.window.upgrade() {
                window.set_status("This file is only in the cloud: it is not read".into());
            }
            return false;
        }
        self.0.viewer.borrow_mut().media = Some(Player::new(path));
        true
    }

    /// Play or pause (`K`, the bar, a click, `preview-play`); whether the shown item plays.
    fn play_pause(&self) -> bool {
        if !self.media_ready() {
            return false;
        }
        let (state, session) = {
            let v = self.0.viewer.borrow();
            let Some(player) = v.media.as_ref() else { return false };
            (player.state.clone(), player.session.is_some())
        };
        match state {
            Play::Playing => self.set_paused(true),
            Play::Paused if session => self.set_paused(false),
            Play::Finding => {}
            Play::NoFfmpeg(_) => self.bar_link(),
            Play::Stopped | Play::Paused | Play::Failed => self.media_start(),
        }
        self.redraw();
        true
    }

    /// `preview-play`: the selected file plays or pauses (the pane opens if nothing shows the
    /// preview).
    pub fn play_action(&self) {
        if !self.active() {
            self.set_pane_open(true);
        }
        self.play_pause();
    }

    fn set_paused(&self, paused: bool) {
        {
            let mut v = self.0.viewer.borrow_mut();
            let Some(player) = v.media.as_mut() else { return };
            let Some(session) = player.session.as_ref() else {
                // A seek waits to start it again: pausing just stays there.
                if paused && player.state == Play::Playing {
                    player.state = Play::Paused;
                    self.0.pause_timer.stop();
                    self.0.tick_timer.stop();
                }
                return;
            };
            session.set_paused(paused);
            player.position = player.clamp(session.position());
            player.state = if paused { Play::Paused } else { Play::Playing };
        }
        if !paused {
            self.0.pause_timer.stop();
            return self.start_tick();
        }
        self.0.tick_timer.stop();
        // A long pause lets the processes go; the place and the last frame stay.
        self.0.pause_timer.start(
            slint::TimerMode::SingleShot,
            media::PAUSE_LIMIT,
            crate::windows::bind_mut(|| {
                with_current(|p| {
                    if let Some(player) = p.0.viewer.borrow_mut().media.as_mut().filter(|pl| pl.state == Play::Paused) {
                        player.session = None;
                    }
                });
            }),
        );
    }

    fn start_tick(&self) {
        self.0.tick_timer.start(
            slint::TimerMode::Repeated,
            TICK,
            crate::windows::bind_mut(|| {
                with_current(|p| {
                    let moved = p.0.viewer.borrow_mut().media.as_mut().is_some_and(|player| {
                        let at =
                            player.session.as_ref().filter(|_| player.state == Play::Playing).map(|s| s.position());
                        at.map(|at| player.position = player.clamp(at)).is_some()
                    });
                    if moved {
                        p.redraw();
                    }
                });
            }),
        );
    }

    /// Plays from the player's place: looks for ffmpeg and probes the file first.
    fn media_start(&self) {
        let path = {
            let mut v = self.0.viewer.borrow_mut();
            let Some(player) = v.media.as_mut() else { return };
            if player.at_end {
                (player.position, player.at_end) = (0.0, false);
            }
            if player.found.is_some() {
                None
            } else {
                player.state = Play::Finding;
                Some(player.path.clone())
            }
        };
        let Some(path) = path else { return self.media_session() };
        let (data, configured) = (data_dir(), crate::convert::configured_ffmpeg());
        let window_id = crate::windows::current();
        std::thread::spawn(move || {
            use gezik_batch::convert::ffmpeg;
            let found = ffmpeg::find_ffmpeg(&data, configured.as_deref());
            let hint = if found.is_none() { gezik_platform::http::tool_missing_hint() } else { None };
            let probe = found.as_ref().and_then(|ff| ffmpeg::probe(ff, &path));
            let program = found.map(|ff| ff.ffmpeg);
            crate::windows::post(window_id, move || {
                with_current(|p| p.media_found(&path, program, probe, hint));
            });
        });
    }

    fn media_found(
        &self,
        path: &Path,
        ffmpeg: Option<PathBuf>,
        probe: Option<gezik_batch::convert::ffmpeg::Probe>,
        hint: Option<String>,
    ) {
        let start = {
            let mut v = self.0.viewer.borrow_mut();
            let Some(player) = v.media.as_mut().filter(|pl| pl.path == path && pl.state == Play::Finding) else {
                return;
            };
            match ffmpeg {
                None => {
                    player.state = Play::NoFfmpeg(hint);
                    false
                }
                Some(ffmpeg) => {
                    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    let video_kind = gezik_core::kind::Kind::of(&name, false) == gezik_core::kind::Kind::Video;
                    let (video, audio, fps) = probe.map_or((video_kind, true, 0.0), |p| (p.video, p.audio, p.fps));
                    player.duration = probe.and_then(|p| p.duration_us).map(|us| us as f64 / 1e6);
                    player.position = player.clamp(player.position);
                    player.found = Some((ffmpeg, video, audio, fps));
                    if !(video || audio) {
                        player.state = Play::Failed;
                    }
                    video || audio
                }
            }
        };
        if start {
            self.media_session();
        }
        self.redraw();
    }

    /// Starts the processes from the player's place.
    fn media_session(&self) {
        let box_ = media::frame_box(self.surface_px());
        let window_id = crate::windows::current();
        let deliver = move |id, event| {
            crate::windows::post(window_id, move || {
                with_current(|p| p.media_event(id, event));
            });
        };
        let started = {
            let mut v = self.0.viewer.borrow_mut();
            let Some(player) = v.media.as_mut() else { return };
            let Some((ffmpeg, video, audio, fps)) = player.found.clone() else { return };
            player.session = None;
            player.ends = (None, None);
            let start = media::Start {
                ffmpeg,
                file: player.path.clone(),
                at: player.position,
                video: video.then(|| (box_, media::frame_rate(fps))),
                audio,
            };
            match media::Session::start(start, self.0.gain.clone(), media::open_device, deliver) {
                Ok(session) => {
                    player.session = Some(session);
                    player.state = Play::Playing;
                    true
                }
                Err(err) => {
                    eprintln!("gezik: cannot start ffmpeg to play {}: {err}", player.path.display());
                    player.state = Play::Failed;
                    false
                }
            }
        };
        if started {
            self.0.pause_timer.stop();
            self.start_tick();
        }
    }

    /// What a session's threads said: a frame, no sound, or that a part ended.
    fn media_event(&self, id: u64, event: media::Event) {
        let frame = {
            let mut v = self.0.viewer.borrow_mut();
            let Some(player) = v.media.as_mut().filter(|pl| pl.session.as_ref().is_some_and(|s| s.id == id)) else {
                return;
            };
            match event {
                media::Event::Frame(buffer) => {
                    let image = Image::from_rgba8(buffer);
                    player.frame = Some(image.clone());
                    if let Some(session) = &player.session {
                        session.in_flight.store(false, Ordering::SeqCst);
                    }
                    Some(image)
                }
                media::Event::NoSound(why) => {
                    eprintln!("gezik: {why}");
                    player.no_sound = Some(why);
                    None
                }
                ended => {
                    if player.ended(&ended) {
                        if player.nothing_came() {
                            let why = player.session.as_mut().map(media::Session::error).unwrap_or_default();
                            eprintln!("gezik: cannot play {}: {why}", player.path.display());
                            player.state = Play::Failed;
                        } else {
                            let at = player.session.as_ref().map_or(player.position, |s| s.position());
                            player.position = player.duration.unwrap_or(at);
                            (player.state, player.at_end) = (Play::Stopped, true);
                        }
                        player.session = None;
                        self.0.tick_timer.stop();
                    }
                    None
                }
            }
        };
        match frame {
            Some(image) => self.show_frame(image),
            None => self.redraw(),
        }
    }

    /// A new frame, where the preview is seen (quick look while it is open).
    fn show_frame(&self, image: Image) {
        if let Some(quick_look) = self.0.quick_look.borrow().as_ref() {
            quick_look.viewer_actions().set_frame(image);
        } else if let Some(window) = self.0.window.upgrade() {
            window.global::<PreviewActions>().set_frame(image);
        }
    }

    /// The player's last frame in both places (none once it is gone): quick look opened or
    /// closed, or the player went.
    fn sync_frame(&self) {
        let frame = self.0.viewer.borrow().media.as_ref().and_then(|player| player.frame.clone()).unwrap_or_default();
        if let Some(quick_look) = self.0.quick_look.borrow().as_ref() {
            quick_look.viewer_actions().set_frame(frame.clone());
        }
        if let Some(window) = self.0.window.upgrade() {
            window.global::<PreviewActions>().set_frame(frame);
        }
    }

    /// Goes to `to` seconds: playing, it starts again there once seeks rest for 150 ms (a held
    /// `L` ends and starts no processes per press); paused, its processes go and Play starts them
    /// there; at the end it stops.
    fn seek(&self, to: f64) {
        let restart = {
            let mut v = self.0.viewer.borrow_mut();
            let Some(player) = v.media.as_mut() else { return };
            let playing = player.state == Play::Playing;
            if player.go_to(to) {
                self.0.tick_timer.stop();
                self.0.pause_timer.stop();
                false
            } else {
                if player.state == Play::Paused {
                    self.0.pause_timer.stop();
                }
                // Its processes go now; the new ones once the seeks rest.
                player.session = None;
                playing
            }
        };
        if restart {
            // The pause timer is idle while playing.
            self.0.pause_timer.start(
                slint::TimerMode::SingleShot,
                SETTLE,
                crate::windows::bind_mut(|| {
                    with_current(|p| {
                        let waiting =
                            p.0.viewer
                                .borrow()
                                .media
                                .as_ref()
                                .is_some_and(|player| player.state == Play::Playing && player.session.is_none());
                        if waiting {
                            p.media_session();
                        }
                    });
                }),
            );
        }
        self.redraw();
    }

    /// `J`, `L`: `by` seconds from where it plays; whether the shown item plays.
    fn skip(&self, by: f64) -> bool {
        if !self.media_ready() {
            return false;
        }
        let at = self.0.viewer.borrow().media.as_ref().map_or(0.0, |player| {
            let playing = player.session.as_ref().filter(|_| player.state == Play::Playing);
            playing.map_or(player.position, |s| s.position())
        });
        self.seek(at + by);
        true
    }

    /// The seek bar at `x` (0–1 in px): dragged, the time shows; released, it goes there.
    fn seek_fraction(&self, x: f32, done: bool) {
        let to = {
            let mut v = self.0.viewer.borrow_mut();
            let Some(player) = v.media.as_mut() else { return };
            let Some(duration) = player.duration else { return };
            let to = f64::from(x.clamp(0.0, 1.0)) * duration;
            if !done {
                player.seeking = Some(to);
            }
            to
        };
        if done { self.seek(to) } else { self.redraw() }
    }

    fn toggle_mute(&self) {
        let (volume, muted) = self.0.volume.get();
        self.set_sound(volume, !muted, true);
    }

    /// The volume bar at `x` (0–1 in px); kept when `done`.
    fn set_volume(&self, x: f32, done: bool) {
        self.set_sound((x.clamp(0.0, 1.0) * 100.0).round() as u8, false, done);
    }

    fn set_sound(&self, volume: u8, muted: bool, keep: bool) {
        self.0.volume.set((volume, muted));
        self.0.gain.store(media::gain(volume, muted), Ordering::Relaxed);
        if keep && let Some(store) = crate::windows::config() {
            store.update_state(|state| state.preview_volume = Some((volume, muted)));
        }
        self.redraw();
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
        assert_eq!((text.kind, text.loading), (2, false));
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

    #[test]
    fn the_viewer_fills_the_bar_and_rows_and_leaves_an_unused_preview_alone() {
        use slint::Model;
        let mut info = PreviewInfo::default();
        Viewer::default().fill(&mut info, (80, 40), true);
        let d = PreviewInfo::default();
        assert!(
            info.rows == d.rows && info.bar == 0 && info.scroll_size == d.scroll_size,
            "nothing new while nothing is shown"
        );
        let file = entry(PathBuf::from("/x/a.rs"), false);
        let mut info = describe(&file, Some(Body::Picture(SharedPixelBuffer::new(2, 2), None)));
        Viewer::default().fill(&mut info, (80, 40), true);
        assert_eq!(info.bar, 3, "a file offers Hex");
        let text = text::TextView::from_head(Path::new("/x/a.rs"), 9, "fn x\ny\n".into(), (true, false));
        let viewer = Viewer { text: Some(text), ..Viewer::default() };
        let mut info = describe(&file, Some(Body::Text(String::new())));
        viewer.fill(&mut info, (80, 40), true);
        assert_eq!((info.bar, info.gutter, info.bar_text.as_str(), info.wrap), (1, 1, "2 lines", false));
        assert_eq!(info.rows.row_count(), 2);
        let first = info.rows.row_data(0).unwrap();
        assert_eq!((first.number.as_str(), first.parts.row_data(0).unwrap().class), ("1", 1), "a keyword");
    }

    #[test]
    fn the_viewer_goes_back_to_a_place_and_knows_its_widest_row() {
        use slint::Model;
        let text = text::TextView::from_head(Path::new("/x/a.txt"), 12, "a\nbbbb\nc\nd\n".into(), (false, false));
        let mut viewer = Viewer { text: Some(text), ..Viewer::default() };
        let place = viewer.place();
        viewer.scroll(2, (80, 2));
        viewer.set_place(place);
        let mut info = PreviewInfo::default();
        viewer.fill(&mut info, (80, 2), true);
        assert_eq!(info.rows.row_data(0).unwrap().number.as_str(), "1", "back at the top");
        assert_eq!(info.max_cols, 4, "bbbb");
    }

    #[test]
    fn the_preview_has_no_text_field_of_its_own() {
        // The window's key router gives a field its editing keys (Ctrl+A, Ctrl+C, arrows…) only
        // for the fields it knows (path box, search, filter, dialogs): a text field in the
        // preview would lose Ctrl+A to "select all files". The page is asked by a question.
        let pane = include_str!("../../ui/widgets/preview-pane.slint");
        for field in ["TextField", "TextInput", "LineEdit", "TextEdit"] {
            assert!(!pane.contains(field), "{field} in the preview pane");
        }
    }

    #[test]
    fn a_picture_over_the_decode_limit_says_its_size_and_a_pdf_is_never_text() {
        let dir = temp("limits");
        let wide = dir.join("wide.png");
        image::GrayImage::new(gezik_platform::MAX_DECODE_SIDE + 8, 4).save(&wide).unwrap();
        if let Body::Picture(_, size) = load(&entry(wide, false), 64) {
            assert_eq!(size, Some((gezik_platform::MAX_DECODE_SIDE + 8, 4)), "its own size, from its header");
        }
        let pdf = dir.join("plain.pdf");
        std::fs::write(
            &pdf,
            "%PDF-1.4
1 0 obj << /Type /Catalog >> endobj
",
        )
        .unwrap();
        assert!(!matches!(load(&entry(pdf.clone(), false), 64), Body::Text(_)));
        let renamed = dir.join("plain.txt");
        std::fs::rename(&pdf, &renamed).unwrap();
        assert!(!matches!(load(&entry(renamed, false), 64), Body::Text(_)), "%PDF- however it is named");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_pdf_offers_its_pages_and_a_picture_its_zoom_and_fit_lets_the_large_copy_go() {
        let pdf = entry(PathBuf::from("/x/a.PDF"), false);
        let mut info = describe(&pdf, Some(Body::Picture(SharedPixelBuffer::new(2, 2), None)));
        Viewer::default().fill(&mut info, (80, 40), true);
        assert_eq!((info.bar, info.page, info.zoom_text.as_str(), info.zoomed), (4, 1, "Fit", false));
        {
            use slint::Model;
            let texts =
                |narrow| viewer::bar_tools(&info, narrow).iter().map(|t| t.text.to_string()).collect::<Vec<_>>();
            assert_eq!(texts(false), ["Hex", "‹", "1", "›", "−", "+", "Fit"], "Fit once");
            assert_eq!(texts(true), ["‹", "1", "›", "−", "+", "Fit"], "narrow: no Hex, no figure");
            let mut zoomed = info.clone();
            zoomed.zoom_text = "125%".into();
            assert_eq!(viewer::bar_tools(&zoomed, true).row_count(), 6);
            assert_eq!(viewer::bar_tools(&zoomed, false).row_data(5).unwrap().text.as_str(), "125%");
            let mut locked = info.clone();
            locked.bar_link = "Enter Password…".into();
            let locked =
                |narrow| viewer::bar_tools(&locked, narrow).iter().map(|t| t.text.to_string()).collect::<Vec<_>>();
            assert_eq!(locked(true), ["Enter Password…"], "locked: the note gets the room");
            assert_eq!(locked(false), ["Enter Password…", "Hex"]);
        }
        // A 4000 × 3000 photo shown from its 1024 px preview.
        let photo = entry(PathBuf::from("/x/p.jpg"), false);
        let base = describe(&photo, Some(Body::Picture(SharedPixelBuffer::new(1024, 768), Some((4000, 3000)))));
        let mut info = base.clone();
        Viewer::default().fill(&mut info, (80, 40), true);
        assert_eq!((info.bar, info.page), (5, 0));
        let mut view = PictureView::new((4000, 3000), 1.0, (400.0, 300.0));
        assert_eq!(view.too_large, None);
        view.full = Some(Image::from_rgba8(SharedPixelBuffer::new(4000, 3000)));
        view.zoom.fit_to(Fit::Zoom(1.0));
        let viewer = Viewer { picture: Some(view), ..Viewer::default() };
        let mut info = base.clone();
        viewer.fill(&mut info, (80, 40), true);
        assert!(info.zoomed && info.zoom_text == "100%" && info.picture.size().width == 4000);
        assert_eq!((info.pic_w, info.pic_h), (4000.0, 3000.0), "one picture pixel a physical pixel");
        // Back to fit: filled from the file's own preview again, the large copy is not held.
        let mut info = base;
        Viewer::default().fill(&mut info, (80, 40), true);
        assert!(!info.zoomed && info.picture.size().width == 1024);
        // Past 48 MP only the preview is zoomed.
        assert_eq!(viewer::too_large((8000, 6100)).as_deref(), Some("Too large to zoom (49 MP)"));
        assert_eq!(viewer::too_large((8000, 6000)), None);
        assert_eq!(
            viewer::too_large((8200, 6200)).as_deref(),
            Some("Too large to zoom (8200 × 6200, over 8192 px a side)"),
            "Gezik does not decode it at all"
        );
    }
}
