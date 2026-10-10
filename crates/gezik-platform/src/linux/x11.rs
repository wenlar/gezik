//! X11: the clipboard (selection ownership, ICCCM transfers with INCR, XFixes change
//! counting) and XDND, on Gezik's own connection and a hidden window of its own. Drops on
//! winit's window reach that window through `XdndProxy` (client messages sent to a window only
//! reach the connection that created it).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use x11rb::connection::{Connection, RequestConnection as _};
use x11rb::protocol::Event;
use x11rb::protocol::xfixes::{ConnectionExt as _, SelectionEventMask};
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ClientMessageEvent, ConnectionExt as _, CreateWindowAux, EventMask, KeyButMask, PropMode, Property,
    SelectionNotifyEvent, SelectionRequestEvent, Window, WindowClass,
};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;
use x11rb::{CURRENT_TIME, NONE};

use super::{UiEvent, uri, xdnd};
use crate::clipboard::{ClipboardError, ClipboardFiles, ClipboardImage, PasteKind};
use crate::dnd::{Allowed, Answer, DragEnd, Effect, Keys, Offer, OutsideDrag};

x11rb::atom_manager! {
    Atoms: AtomsCookie {
        CLIPBOARD,
        TARGETS,
        INCR,
        UTF8_STRING,
        TEXT,
        TEXT_PLAIN: b"text/plain;charset=utf-8",
        TEXT_PLAIN_ANY: b"text/plain",
        IMAGE_PNG: b"image/png",
        URI_LIST: b"text/uri-list",
        GNOME_FILES: b"x-special/gnome-copied-files",
        KDE_CUT: b"application/x-kde-cutselection",
        GEZIK_TRANSFER,
        XdndAware,
        XdndProxy,
        XdndEnter,
        XdndPosition,
        XdndStatus,
        XdndLeave,
        XdndDrop,
        XdndFinished,
        XdndSelection,
        XdndTypeList,
        XdndActionCopy,
        XdndActionMove,
        XdndActionLink,
    }
}

/// How long a transfer from another program may take before Gezik gives up on it.
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(1);
/// A picture arrives in pieces (INCR) and may take longer.
const IMAGE_TIMEOUT: Duration = Duration::from_secs(5);
/// How long a drop may wait for the target's `XdndFinished`.
const FINISH_TIMEOUT: Duration = Duration::from_secs(5);

/// Files Gezik offers on a selection.
#[derive(Clone)]
struct Owned {
    paths: Vec<PathBuf>,
    cut: bool,
    /// Text instead of files (copied paths): offered as text only.
    text: Option<String>,
}

/// A drag from another program over Gezik's window.
struct Incoming {
    source: Window,
    /// It offers `text/uri-list`.
    has_uris: bool,
    /// Its files, once read (on the UI thread, the first time they are needed).
    paths: Option<Vec<PathBuf>>,
    /// The pointer in the window (physical pixels), the keys, and the time of the last position.
    x: f64,
    y: f64,
    keys: Keys,
    time: u32,
}

/// Gezik's drag over another program's window.
struct Outgoing {
    /// The window with `XdndAware` under the pointer, the window messages go to (its proxy,
    /// or itself), and the version both speak.
    target: Option<(Window, Window, u32)>,
    accepted: bool,
    /// A drop was sent; waiting for `XdndFinished`.
    dropping: bool,
}

#[derive(Default)]
struct State {
    clipboard: Option<Owned>,
    dragged: Option<Owned>,
    incoming: Option<Incoming>,
    outgoing: Option<Outgoing>,
    events: Vec<UiEvent>,
}

struct Shared {
    conn: RustConnection,
    atoms: Atoms,
    root: Window,
    /// Gezik's hidden window: selection owner, transfer requestor, XDND proxy.
    window: Window,
    /// winit's window (the drop target users see).
    target: Window,
    state: Mutex<State>,
    /// A transfer under way on the UI thread, waiting for the event thread's events.
    waiting: Mutex<Option<Sender<Event>>>,
    sequence: AtomicU64,
    /// The clipboard owner's offered names, with the `sequence` they were asked at.
    offers: Mutex<Option<(u64, Vec<&'static str>)>>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl Shared {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn send(&self, to: Window, kind: Atom, data: [u32; 5]) {
        let event = ClientMessageEvent::new(32, to, kind, data);
        let _ = self.conn.send_event(false, to, EventMask::NO_EVENT, event);
        let _ = self.conn.flush();
    }

    fn queue(&self, event: UiEvent) {
        let mut state = self.state();
        // A newer position replaces one the UI has not taken yet.
        if matches!(event, UiEvent::Over { .. })
            && let Some(last) = state.events.last()
            && matches!(last, UiEvent::Over { .. })
        {
            state.events.pop();
        }
        state.events.push(event);
        drop(state);
        (self.wake)();
    }

    fn keys(&self) -> Keys {
        let mask = self.conn.query_pointer(self.root).ok().and_then(|c| c.reply().ok()).map(|r| r.mask);
        let held = |bit: KeyButMask| mask.is_some_and(|mask| u16::from(mask) & u16::from(bit) != 0);
        gezik_core::drag::keys_of(
            gezik_core::drag::DragOs::Linux,
            held(KeyButMask::SHIFT),
            held(KeyButMask::CONTROL),
            held(KeyButMask::MOD1),
            false,
        )
    }

    /// Root (`x`, `y`) in winit's window, in physical pixels.
    fn in_window(&self, x: i16, y: i16) -> (f64, f64) {
        match self.conn.translate_coordinates(self.root, self.target, x, y).ok().and_then(|c| c.reply().ok()) {
            Some(r) => (f64::from(r.dst_x), f64::from(r.dst_y)),
            None => (f64::from(x), f64::from(y)),
        }
    }

    fn action_atom(&self, effect: Option<Effect>) -> Atom {
        match effect {
            Some(Effect::Move) => self.atoms.XdndActionMove,
            Some(Effect::Copy) => self.atoms.XdndActionCopy,
            Some(Effect::Link) => self.atoms.XdndActionLink,
            None => NONE,
        }
    }

    /// The formats Gezik offers for `owned`, `TARGETS` first.
    fn targets(&self, owned: &Owned) -> Vec<Atom> {
        let a = &self.atoms;
        if owned.text.is_some() {
            vec![a.TARGETS, a.UTF8_STRING, a.TEXT_PLAIN, a.TEXT, AtomEnum::STRING.into()]
        } else {
            vec![a.TARGETS, a.URI_LIST, a.GNOME_FILES, a.KDE_CUT, a.UTF8_STRING, a.TEXT_PLAIN]
        }
    }

    /// The bytes of `owned` in format `target`, if Gezik offers it.
    fn render(&self, owned: &Owned, target: Atom) -> Option<Vec<u8>> {
        let a = &self.atoms;
        if let Some(text) = &owned.text {
            return if target == a.UTF8_STRING || target == a.TEXT_PLAIN || target == a.TEXT {
                Some(text.as_bytes().to_vec())
            } else if target == Atom::from(AtomEnum::STRING) {
                Some(super::latin1(text))
            } else {
                None
            };
        }
        if target == a.URI_LIST {
            Some(uri::uri_list(&owned.paths).into_bytes())
        } else if target == a.GNOME_FILES {
            Some(uri::gnome_copied_files(&owned.paths, owned.cut).into_bytes())
        } else if target == a.KDE_CUT {
            Some(uri::kde_cut(owned.cut).to_vec())
        } else if target == a.UTF8_STRING || target == a.TEXT_PLAIN {
            let lines: Vec<String> = owned.paths.iter().map(|p| p.to_string_lossy().into_owned()).collect();
            Some(lines.join("\n").into_bytes())
        } else {
            None
        }
    }

    /// Another program asks for what Gezik put on a selection.
    fn serve(&self, request: &SelectionRequestEvent) {
        let a = &self.atoms;
        let owned = {
            let state = self.state();
            if request.selection == a.CLIPBOARD {
                state.clipboard.clone()
            } else if request.selection == a.XdndSelection {
                state.dragged.clone()
            } else {
                None
            }
        };
        // Obsolete clients name no property: the target is the property then (ICCCM).
        let property = if request.property == NONE { request.target } else { request.property };
        let mut answered = NONE;
        if let Some(owned) = owned {
            if request.target == a.TARGETS {
                let targets = self.targets(&owned);
                if self
                    .conn
                    .change_property32(PropMode::REPLACE, request.requestor, property, AtomEnum::ATOM, &targets)
                    .is_ok()
                {
                    answered = property;
                }
            } else if let Some(bytes) = self.render(&owned, request.target) {
                // A file list never needs INCR; one too big for a single request is refused.
                let fits = bytes.len() + 64 < self.conn.maximum_request_bytes();
                // TEXT is answered in the encoding it was given in (ICCCM).
                let kind = if request.target == a.TEXT { a.UTF8_STRING } else { request.target };
                if fits
                    && self.conn.change_property8(PropMode::REPLACE, request.requestor, property, kind, &bytes).is_ok()
                {
                    answered = property;
                }
            }
        }
        let notify = SelectionNotifyEvent {
            response_type: x11rb::protocol::xproto::SELECTION_NOTIFY_EVENT,
            sequence: 0,
            time: request.time,
            requestor: request.requestor,
            selection: request.selection,
            target: request.target,
            property: answered,
        };
        let _ = self.conn.send_event(false, request.requestor, EventMask::NO_EVENT, notify);
        let _ = self.conn.flush();
    }

    /// Asks the owner of `selection` for `target` and waits (on the UI thread) for the bytes.
    fn transfer(&self, selection: Atom, target: Atom, time: u32) -> Option<Vec<u8>> {
        self.transfer_within(selection, target, time, TRANSFER_TIMEOUT)
    }

    /// `transfer` with its own time limit (a picture).
    fn transfer_within(&self, selection: Atom, target: Atom, time: u32, timeout: Duration) -> Option<Vec<u8>> {
        let (tx, rx) = mpsc::channel();
        *self.waiting.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(tx);
        let result = self.transfer_with(&rx, selection, target, time, timeout);
        *self.waiting.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        result
    }

    fn transfer_with(
        &self,
        rx: &Receiver<Event>,
        selection: Atom,
        target: Atom,
        time: u32,
        timeout: Duration,
    ) -> Option<Vec<u8>> {
        let property = self.atoms.GEZIK_TRANSFER;
        self.conn.delete_property(self.window, property).ok()?;
        self.conn.convert_selection(self.window, selection, target, property, time).ok()?;
        self.conn.flush().ok()?;
        let deadline = Instant::now() + timeout;
        let wait = |rx: &Receiver<Event>| rx.recv_timeout(deadline.saturating_duration_since(Instant::now())).ok();
        loop {
            match wait(rx)? {
                Event::SelectionNotify(e) if e.selection == selection => {
                    if e.property == NONE {
                        return None;
                    }
                    break;
                }
                _ => continue,
            }
        }
        let reply =
            self.conn.get_property(true, self.window, property, AtomEnum::ANY, 0, u32::MAX / 4).ok()?.reply().ok()?;
        if reply.type_ != self.atoms.INCR {
            return Some(reply.value);
        }
        // INCR: the owner writes the data in pieces, each after Gezik deletes the last.
        let mut data = Vec::new();
        loop {
            match wait(rx)? {
                Event::PropertyNotify(e)
                    if e.window == self.window && e.atom == property && e.state == Property::NEW_VALUE =>
                {
                    let piece = self
                        .conn
                        .get_property(true, self.window, property, AtomEnum::ANY, 0, u32::MAX / 4)
                        .ok()?
                        .reply()
                        .ok()?;
                    if piece.value.is_empty() {
                        return Some(data);
                    }
                    if data.len() + piece.value.len() > super::MAX_TRANSFER_BYTES {
                        return None;
                    }
                    data.extend_from_slice(&piece.value);
                }
                _ => continue,
            }
        }
    }

    fn owns(&self, selection: Atom) -> bool {
        self.conn
            .get_selection_owner(selection)
            .ok()
            .and_then(|c| c.reply().ok())
            .is_some_and(|r| r.owner == self.window)
    }

    /// An XDND message to Gezik's window: a drag over it, or the answer to Gezik's own drag.
    fn client_message(&self, event: &ClientMessageEvent) {
        let a = &self.atoms;
        let data = event.data.as_data32();
        if event.type_ == a.XdndEnter {
            let enter = xdnd::parse_enter(data);
            let types = if enter.more_types { self.type_list(enter.source) } else { enter.types };
            self.state().incoming = Some(Incoming {
                source: enter.source,
                has_uris: types.contains(&a.URI_LIST),
                paths: None,
                x: 0.0,
                y: 0.0,
                keys: Keys::default(),
                time: CURRENT_TIME,
            });
        } else if event.type_ == a.XdndPosition {
            let position = xdnd::parse_position(data);
            let (x, y) = self.in_window(position.x, position.y);
            let keys = self.keys();
            let has_uris = {
                let mut state = self.state();
                let Some(incoming) = state.incoming.as_mut() else { return };
                (incoming.x, incoming.y, incoming.keys, incoming.time) = (x, y, keys, position.time);
                incoming.has_uris
            };
            if has_uris {
                self.queue(UiEvent::Over { offer: None, x, y, keys });
            } else {
                self.send(position.source, a.XdndStatus, xdnd::status(self.target, false, NONE));
            }
        } else if event.type_ == a.XdndLeave {
            if self.state().incoming.take().is_some() {
                self.queue(UiEvent::Leave);
            }
        } else if event.type_ == a.XdndDrop {
            // The data is asked for with the drop's own time (XDND: `data.l[2]`).
            let has_uris = self.state().incoming.as_mut().map(|i| {
                i.time = data[2];
                i.has_uris
            });
            match has_uris {
                Some(true) => self.queue(UiEvent::Dropped { offer: None, x: 0.0, y: 0.0, keys: Keys::default() }),
                Some(false) => {
                    let source = self.state().incoming.take().map(|i| i.source);
                    if let Some(source) = source {
                        self.send(source, a.XdndFinished, xdnd::finished(self.target, false, NONE));
                    }
                }
                None => {}
            }
        } else if event.type_ == a.XdndStatus {
            let status = xdnd::parse_status(data);
            if let Some(outgoing) = self.state().outgoing.as_mut() {
                outgoing.accepted = status.accept;
            }
        } else if event.type_ == a.XdndFinished {
            let finished = self.state().outgoing.as_mut().is_some_and(|o| std::mem::take(&mut o.dropping));
            if finished {
                self.queue(UiEvent::SourceEnded(DragEnd::Dropped));
            }
        }
    }

    fn type_list(&self, source: Window) -> Vec<Atom> {
        self.conn
            .get_property(false, source, self.atoms.XdndTypeList, AtomEnum::ATOM, 0, 1024)
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|r| r.value32().map(Iterator::collect))
            .unwrap_or_default()
    }

    fn run(self: Arc<Self>) {
        loop {
            let Ok(event) = self.conn.wait_for_event() else { return };
            match event {
                Event::SelectionRequest(e) => self.serve(&e),
                Event::SelectionClear(e) => {
                    let mut state = self.state();
                    if e.selection == self.atoms.CLIPBOARD {
                        state.clipboard = None;
                    } else if e.selection == self.atoms.XdndSelection {
                        state.dragged = None;
                    }
                }
                Event::XfixesSelectionNotify(_) => {
                    self.sequence.fetch_add(1, Ordering::SeqCst);
                }
                Event::ClientMessage(e) => self.client_message(&e),
                e @ (Event::SelectionNotify(_) | Event::PropertyNotify(_)) => {
                    if let Some(tx) = &*self.waiting.lock().unwrap_or_else(std::sync::PoisonError::into_inner) {
                        let _ = tx.send(e);
                    }
                }
                _ => {}
            }
        }
    }
}

/// The X11 side of the clipboard and drag and drop.
pub(crate) struct X11(Arc<Shared>);

impl X11 {
    /// Connects to the X server (`DISPLAY`) and takes drops on winit's window `target`.
    pub fn start(target: Window, wake: Arc<dyn Fn() + Send + Sync>) -> Result<X11, String> {
        let (conn, screen) = RustConnection::connect(None).map_err(|e| e.to_string())?;
        let root = conn.setup().roots.get(screen).ok_or("no X screen")?.root;
        let atoms = Atoms::new(&conn).map_err(|e| e.to_string())?.reply().map_err(|e| e.to_string())?;
        let window = conn.generate_id().map_err(|e| e.to_string())?;
        conn.create_window(
            x11rb::COPY_DEPTH_FROM_PARENT,
            window,
            root,
            -10,
            -10,
            1,
            1,
            0,
            WindowClass::INPUT_ONLY,
            x11rb::COPY_FROM_PARENT,
            &CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE),
        )
        .map_err(|e| e.to_string())?;
        // XDND: winit's window says it takes drops (winit sets XdndAware itself) and that its
        // messages go to Gezik's window; that window points to itself, as the proxy must.
        let version = [xdnd::VERSION];
        conn.change_property32(PropMode::REPLACE, target, atoms.XdndAware, AtomEnum::ATOM, &version)
            .map_err(|e| e.to_string())?;
        conn.change_property32(PropMode::REPLACE, window, atoms.XdndAware, AtomEnum::ATOM, &version)
            .map_err(|e| e.to_string())?;
        conn.change_property32(PropMode::REPLACE, target, atoms.XdndProxy, AtomEnum::WINDOW, &[window])
            .map_err(|e| e.to_string())?;
        conn.change_property32(PropMode::REPLACE, window, atoms.XdndProxy, AtomEnum::WINDOW, &[window])
            .map_err(|e| e.to_string())?;
        // Counting clipboard changes needs XFixes (any X server of the last 20 years has it).
        if conn.xfixes_query_version(5, 0).ok().and_then(|c| c.reply().ok()).is_some() {
            let mask = SelectionEventMask::SET_SELECTION_OWNER
                | SelectionEventMask::SELECTION_WINDOW_DESTROY
                | SelectionEventMask::SELECTION_CLIENT_CLOSE;
            let _ = conn.xfixes_select_selection_input(window, atoms.CLIPBOARD, mask);
        }
        conn.flush().map_err(|e| e.to_string())?;
        let shared = Arc::new(Shared {
            conn,
            atoms,
            root,
            window,
            target,
            state: Mutex::default(),
            waiting: Mutex::new(None),
            sequence: AtomicU64::new(1),
            offers: Mutex::new(None),
            wake,
        });
        let runner = shared.clone();
        std::thread::Builder::new().name("gezik-x11".into()).spawn(move || runner.run()).map_err(|e| e.to_string())?;
        Ok(X11(shared))
    }
}

impl X11 {
    /// Puts `owned` on the clipboard: Gezik's hidden window becomes its owner.
    fn own_clipboard(&self, owned: Owned) -> Result<(), ClipboardError> {
        let s = &self.0;
        s.state().clipboard = Some(owned);
        let set =
            s.conn.set_selection_owner(s.window, s.atoms.CLIPBOARD, CURRENT_TIME).is_ok() && s.conn.flush().is_ok();
        if set && s.owns(s.atoms.CLIPBOARD) {
            Ok(())
        } else {
            s.state().clipboard = None;
            Err(ClipboardError::Failed("cannot take the X11 clipboard".into()))
        }
    }
}

impl X11 {
    /// The clipboard owner's targets by name, those Gezik knows (the rest do not matter here).
    /// Asked once per clipboard change (the XFixes `sequence`), an unanswered ask included: a
    /// hung owner costs one wait, not one per call.
    fn offered_names(&self) -> Vec<&'static str> {
        let s = &self.0;
        let at = s.sequence.load(Ordering::SeqCst);
        let cached = super::cached_offers(&s.offers.lock().unwrap_or_else(std::sync::PoisonError::into_inner), at);
        if let Some(names) = cached {
            return names;
        }
        let names = self.ask_offered_names();
        *s.offers.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some((at, names.clone()));
        names
    }

    fn ask_offered_names(&self) -> Vec<&'static str> {
        let s = &self.0;
        let a = &s.atoms;
        let Some(targets) = s.transfer(a.CLIPBOARD, a.TARGETS, CURRENT_TIME) else { return Vec::new() };
        let offered: Vec<Atom> = targets.as_chunks::<4>().0.iter().map(|c| u32::from_ne_bytes(*c)).collect();
        let known: [(Atom, &'static str); 9] = [
            (a.KDE_CUT, "application/x-kde-cutselection"),
            (a.URI_LIST, "text/uri-list"),
            (a.GNOME_FILES, "x-special/gnome-copied-files"),
            (a.IMAGE_PNG, "image/png"),
            (a.UTF8_STRING, "UTF8_STRING"),
            (a.TEXT_PLAIN, "text/plain;charset=utf-8"),
            (a.TEXT_PLAIN_ANY, "text/plain"),
            (a.TEXT, "TEXT"),
            (AtomEnum::STRING.into(), "STRING"),
        ];
        known.iter().filter(|(atom, _)| offered.contains(atom)).map(|(_, name)| *name).collect()
    }

    /// The atom of text type `name` (one of `TEXT_TYPES`).
    fn text_atom(&self, name: &str) -> Atom {
        let a = &self.0.atoms;
        match name {
            "UTF8_STRING" => a.UTF8_STRING,
            "text/plain;charset=utf-8" => a.TEXT_PLAIN,
            "text/plain" => a.TEXT_PLAIN_ANY,
            "TEXT" => a.TEXT,
            _ => AtomEnum::STRING.into(),
        }
    }
}

impl super::Backend for X11 {
    fn write_files(&self, paths: &[PathBuf], cut: bool) -> Result<(), ClipboardError> {
        self.own_clipboard(Owned { paths: paths.to_vec(), cut, text: None })
    }

    fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
        // `serve` hands it over in one request (no INCR): one too big would be refused there,
        // after "Copied" was shown.
        if text.len() + 64 >= self.0.conn.maximum_request_bytes() {
            return Err(ClipboardError::Failed("the text is too long for the X11 clipboard".into()));
        }
        self.own_clipboard(Owned { paths: Vec::new(), cut: false, text: Some(text.to_owned()) })
    }

    fn read_files(&self) -> Result<Option<ClipboardFiles>, ClipboardError> {
        let s = &self.0;
        if s.owns(s.atoms.CLIPBOARD) {
            return Ok(s
                .state()
                .clipboard
                .clone()
                .filter(|o| o.text.is_none())
                .map(|o| ClipboardFiles { paths: o.paths, cut: o.cut }));
        }
        // The owner's formats first, from the cached ask (once per clipboard change): a wait
        // (at most a second) for each format asked for, so only what it offers is asked for.
        let offered = self.offered_names();
        let a = &s.atoms;
        let atoms: Vec<Atom> = offered
            .iter()
            .filter_map(|name| match *name {
                "x-special/gnome-copied-files" => Some(a.GNOME_FILES),
                "text/uri-list" => Some(a.URI_LIST),
                "application/x-kde-cutselection" => Some(a.KDE_CUT),
                _ => None,
            })
            .collect();
        let Some(ask) = super::what_to_ask(&atoms, a.GNOME_FILES, a.URI_LIST, a.KDE_CUT) else { return Ok(None) };
        let Some(text) = s.transfer(a.CLIPBOARD, ask.files, CURRENT_TIME) else { return Ok(None) };
        let text = String::from_utf8_lossy(&text);
        if ask.files == a.GNOME_FILES {
            return Ok(uri::parse_gnome_copied_files(&text).map(|(cut, paths)| ClipboardFiles { paths, cut }));
        }
        let paths = uri::parse_uri_list(&text);
        if paths.is_empty() {
            return Ok(None);
        }
        let cut = ask.kde_cut && s.transfer(a.CLIPBOARD, a.KDE_CUT, CURRENT_TIME).is_some_and(|v| v == b"1");
        Ok(Some(ClipboardFiles { paths, cut }))
    }

    fn paste_kind(&self) -> Option<PasteKind> {
        let s = &self.0;
        if s.owns(s.atoms.CLIPBOARD) {
            // Gezik's own: copied paths are text, cut or copied files paste as files.
            return s.state().clipboard.as_ref().and_then(|o| o.text.as_ref()).map(|_| PasteKind::Text);
        }
        super::paste_kind_of(&self.offered_names())
    }

    fn read_image(&self) -> Result<Option<ClipboardImage>, ClipboardError> {
        let s = &self.0;
        if s.owns(s.atoms.CLIPBOARD) || !self.offered_names().contains(&"image/png") {
            return Ok(None);
        }
        let png = s.transfer_within(s.atoms.CLIPBOARD, s.atoms.IMAGE_PNG, CURRENT_TIME, IMAGE_TIMEOUT);
        Ok(png.map(ClipboardImage::Png))
    }

    fn read_text(&self) -> Result<Option<String>, ClipboardError> {
        let s = &self.0;
        if s.owns(s.atoms.CLIPBOARD) {
            return Ok(s
                .state()
                .clipboard
                .as_ref()
                .and_then(|o| o.text.clone())
                .and_then(crate::clipboard::clean_text));
        }
        let offered = self.offered_names();
        let Some(name) = super::text_type(&offered) else { return Ok(None) };
        let Some(bytes) = s.transfer(s.atoms.CLIPBOARD, self.text_atom(name), CURRENT_TIME) else { return Ok(None) };
        let text =
            if name == "STRING" { super::from_latin1(&bytes) } else { String::from_utf8_lossy(&bytes).into_owned() };
        Ok(crate::clipboard::clean_text(text))
    }

    fn sequence(&self) -> u64 {
        self.0.sequence.load(Ordering::SeqCst)
    }

    fn clear(&self) -> Result<(), ClipboardError> {
        let s = &self.0;
        if s.owns(s.atoms.CLIPBOARD) {
            let _ = s.conn.set_selection_owner(NONE, s.atoms.CLIPBOARD, CURRENT_TIME);
            let _ = s.conn.flush();
        }
        s.state().clipboard = None;
        Ok(())
    }

    fn take_events(&self) -> Vec<UiEvent> {
        let s = &self.0;
        let events = std::mem::take(&mut s.state().events);
        events
            .into_iter()
            .filter_map(|event| match event {
                UiEvent::Over { x, y, keys, .. } => {
                    self.offer().map(|offer| UiEvent::Over { offer: Some(offer), x, y, keys })
                }
                UiEvent::Dropped { .. } => {
                    let (x, y, keys) = s.state().incoming.as_ref().map(|i| (i.x, i.y, i.keys))?;
                    self.offer().map(|offer| UiEvent::Dropped { offer: Some(offer), x, y, keys })
                }
                other => Some(other),
            })
            .collect()
    }

    fn answer(&self, answer: &Answer) {
        let s = &self.0;
        let source = s.state().incoming.as_ref().map(|i| i.source);
        if let Some(source) = source {
            let action = s.action_atom(answer.effect);
            s.send(source, s.atoms.XdndStatus, xdnd::status(s.target, answer.effect.is_some(), action));
        }
    }

    fn finish(&self, done: Option<Effect>) {
        let s = &self.0;
        let Some(incoming) = s.state().incoming.take() else { return };
        // A move was done by Gezik itself: the source is told "copy", so it deletes nothing.
        let action = done.map(|_| s.atoms.XdndActionCopy).unwrap_or(NONE);
        s.send(incoming.source, s.atoms.XdndFinished, xdnd::finished(s.target, done.is_some(), action));
    }

    fn drag_out(&self, paths: &[PathBuf]) -> Result<Box<dyn OutsideDrag>, String> {
        let s = &self.0;
        {
            let mut state = s.state();
            state.dragged = Some(Owned { paths: paths.to_vec(), cut: false, text: None });
            state.outgoing = Some(Outgoing { target: None, accepted: false, dropping: false });
        }
        s.conn.set_selection_owner(s.window, s.atoms.XdndSelection, CURRENT_TIME).map_err(|e| e.to_string())?;
        s.conn.flush().map_err(|e| e.to_string())?;
        Ok(Box::new(X11Drag { shared: s.clone(), origin: None, aware: HashMap::new() }))
    }
}

impl X11 {
    /// The files over the window, read from the source the first time.
    fn offer(&self) -> Option<Offer> {
        let s = &self.0;
        let (known, time) = {
            let state = s.state();
            let incoming = state.incoming.as_ref()?;
            (incoming.paths.clone(), incoming.time)
        };
        let paths = match known {
            Some(paths) => paths,
            None => {
                let text = s.transfer(s.atoms.XdndSelection, s.atoms.URI_LIST, time).unwrap_or_default();
                let paths = uri::parse_uri_list(&String::from_utf8_lossy(&text));
                if let Some(incoming) = s.state().incoming.as_mut() {
                    incoming.paths = Some(paths.clone());
                }
                paths
            }
        };
        Some(Offer { paths, allowed: Allowed::ALL, right: false, virtual_count: 0, virtual_files: None })
    }
}

/// Gezik's drag over other windows: Gezik sends the XDND messages itself, following Slint's
/// pointer moves (winit keeps the pointer while the button is down).
struct X11Drag {
    shared: Arc<Shared>,
    /// Where winit's window is on the root window (it does not move during a drag).
    origin: Option<(i16, i16)>,
    /// What each top-level window under the pointer takes drops on, found once per drag: a
    /// pointer move then costs one request, not a walk down the window tree.
    aware: HashMap<Window, Option<(Window, Window, u32)>>,
}

type Aware = Option<(Window, Window, u32)>;

impl X11Drag {
    /// The window under root (`x`, `y`) that takes XDND drops, where to send to, and the version.
    fn aware_at(&mut self, x: i16, y: i16) -> Aware {
        let s = self.shared.clone();
        let top = s.conn.translate_coordinates(s.root, s.root, x, y).ok()?.reply().ok()?.child;
        if top == NONE {
            return None;
        }
        if let Some(known) = self.aware.get(&top) {
            return *known;
        }
        let found = Self::aware_below(&s, top, x, y);
        self.aware.insert(top, found);
        found
    }

    /// `aware_at` within top-level window `top` (itself first).
    fn aware_below(s: &Shared, top: Window, x: i16, y: i16) -> Aware {
        let mut window = top;
        loop {
            if let Some(found) = Self::aware(s, window) {
                return found;
            }
            let child = s.conn.translate_coordinates(s.root, window, x, y).ok()?.reply().ok()?.child;
            if child == NONE {
                return None;
            }
            window = child;
        }
    }

    /// Whether `window` takes XDND drops: None if it says nothing, Some(None) if its version is
    /// too old, else where messages go and the version.
    fn aware(s: &Shared, window: Window) -> Option<Aware> {
        let version = s
            .conn
            .get_property(false, window, s.atoms.XdndAware, AtomEnum::ATOM, 0, 1)
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|r| r.value32().and_then(|mut v| v.next()))?;
        if version < 3 {
            return Some(None);
        }
        let proxy = s
            .conn
            .get_property(false, window, s.atoms.XdndProxy, AtomEnum::WINDOW, 0, 1)
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|r| r.value32().and_then(|mut v| v.next()))
            .filter(|&proxy| proxy != NONE);
        Some(Some((window, proxy.unwrap_or(window), version.min(xdnd::VERSION))))
    }
}

impl OutsideDrag for X11Drag {
    fn driven_by_gezik(&self) -> bool {
        true
    }

    fn moved(&mut self, x: f64, y: f64, keys: Keys) {
        let s = self.shared.clone();
        if self.origin.is_none() {
            let reply = s.conn.translate_coordinates(s.target, s.root, 0, 0).ok().and_then(|c| c.reply().ok());
            self.origin = reply.map(|r| (r.dst_x, r.dst_y));
        }
        let Some((ox, oy)) = self.origin else { return };
        let (rx, ry) = ((f64::from(ox) + x) as i16, (f64::from(oy) + y) as i16);
        let found = self.aware_at(rx, ry).filter(|(window, ..)| *window != s.target);
        let old = s.state().outgoing.as_ref().and_then(|o| o.target);
        if old.map(|t| t.0) != found.map(|t| t.0) {
            if let Some((_, to, _)) = old {
                s.send(to, s.atoms.XdndLeave, xdnd::leave(s.window));
            }
            if let Some((_, to, version)) = found {
                s.send(to, s.atoms.XdndEnter, xdnd::enter(s.window, version, &[s.atoms.URI_LIST, s.atoms.UTF8_STRING]));
            }
            if let Some(outgoing) = s.state().outgoing.as_mut() {
                (outgoing.target, outgoing.accepted) = (found, false);
            }
        }
        if let Some((_, to, _)) = found {
            // The target picks copy or move itself; Shift asks for a move, as everywhere.
            let action = if keys.shift && !keys.copy { s.atoms.XdndActionMove } else { s.atoms.XdndActionCopy };
            s.send(to, s.atoms.XdndPosition, xdnd::position(s.window, rx, ry, CURRENT_TIME, action));
        }
    }

    fn released(&mut self) {
        let s = self.shared.clone();
        let (target, accepted) = match s.state().outgoing.as_ref() {
            Some(o) => (o.target, o.accepted),
            None => return,
        };
        match target {
            Some((_, to, _)) if accepted => {
                if let Some(outgoing) = s.state().outgoing.as_mut() {
                    outgoing.dropping = true;
                }
                s.send(to, s.atoms.XdndDrop, xdnd::drop(s.window, CURRENT_TIME));
                // A target that never answers does not keep the drag open.
                std::thread::spawn(move || {
                    std::thread::sleep(FINISH_TIMEOUT);
                    let unanswered = s.state().outgoing.as_mut().is_some_and(|o| std::mem::take(&mut o.dropping));
                    if unanswered {
                        s.queue(UiEvent::SourceEnded(DragEnd::Dropped));
                    }
                });
            }
            other => {
                if let Some((_, to, _)) = other {
                    s.send(to, s.atoms.XdndLeave, xdnd::leave(s.window));
                }
                s.state().outgoing = None;
                s.queue(UiEvent::SourceEnded(DragEnd::Cancelled));
            }
        }
    }

    fn cancel(&mut self) {
        let s = &self.shared;
        let outgoing = s.state().outgoing.take();
        if let Some((_, to, _)) = outgoing.and_then(|o| o.target) {
            s.send(to, s.atoms.XdndLeave, xdnd::leave(s.window));
        }
    }
}
