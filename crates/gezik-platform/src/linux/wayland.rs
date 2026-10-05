//! Wayland: the clipboard and drag and drop over the core `wl_data_device`, on winit's own
//! connection (a separate event queue on its `wl_display`). Unlike `data-control`, the data
//! device works on every compositor; it needs the serial of an input event on Gezik's window,
//! which Gezik's own `wl_pointer` and `wl_keyboard` objects receive alongside winit's.

use std::io::{Read, Write};
use std::os::fd::{AsFd, OwnedFd};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use wayland_backend::client::{Backend as WlBackend, ObjectId};
use wayland_client::protocol::wl_data_device::{self, WlDataDevice};
use wayland_client::protocol::wl_data_device_manager::{DndAction, WlDataDeviceManager};
use wayland_client::protocol::wl_data_offer::{self, WlDataOffer};
use wayland_client::protocol::wl_data_source::{self, WlDataSource};
use wayland_client::protocol::wl_keyboard::{self, WlKeyboard};
use wayland_client::protocol::wl_pointer::{self, WlPointer};
use wayland_client::protocol::wl_registry::{self, WlRegistry};
use wayland_client::protocol::wl_seat::{self, Capability, WlSeat};
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum, event_created_child};

use super::{UiEvent, uri};
use crate::clipboard::{ClipboardError, ClipboardFiles};
use crate::dnd::{Allowed, Answer, DragEnd, Effect, Keys, Offer, OutsideDrag};

const URI_LIST: &str = "text/uri-list";
const GNOME_FILES: &str = "x-special/gnome-copied-files";
const KDE_CUT: &str = "application/x-kde-cutselection";
const TEXT: &str = "text/plain;charset=utf-8";

/// How long reading from another program may take.
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(1);

/// Which of the offered types Gezik reads files from, best first.
pub(crate) fn preferred_mime(types: &[String]) -> Option<&'static str> {
    [GNOME_FILES, URI_LIST].into_iter().find(|wanted| types.iter().any(|t| t == wanted))
}

/// Files Gezik offers, and the bytes of each type.
#[derive(Clone)]
struct Owned {
    paths: Vec<PathBuf>,
    cut: bool,
}

impl Owned {
    fn render(&self, mime: &str) -> Option<Vec<u8>> {
        match mime {
            URI_LIST => Some(uri::uri_list(&self.paths).into_bytes()),
            GNOME_FILES => Some(uri::gnome_copied_files(&self.paths, self.cut).into_bytes()),
            KDE_CUT => Some(uri::kde_cut(self.cut).to_vec()),
            TEXT => {
                let lines: Vec<String> = self.paths.iter().map(|p| p.to_string_lossy().into_owned()).collect();
                Some(lines.join("\n").into_bytes())
            }
            _ => None,
        }
    }
}

/// What an offer (the clipboard's or a drag's) says about itself.
struct OfferInfo {
    types: Vec<String>,
    source_actions: DndAction,
}

impl Default for OfferInfo {
    fn default() -> OfferInfo {
        OfferInfo { types: Vec::new(), source_actions: DndAction::empty() }
    }
}

type OfferData = Mutex<OfferInfo>;

/// A drag from another program over Gezik's window.
struct Incoming {
    offer: WlDataOffer,
    serial: u32,
    /// Surface coordinates (logical pixels).
    x: f64,
    y: f64,
    paths: Option<Vec<PathBuf>>,
}

#[derive(Default)]
struct Inner {
    manager: Option<WlDataDeviceManager>,
    seat: Option<WlSeat>,
    device: Option<WlDataDevice>,
    pointer: Option<WlPointer>,
    keyboard: Option<WlKeyboard>,
    /// The latest input serial on Gezik's window (a press, a key, the focus).
    serial: Option<u32>,
    /// The serial of the latest button press, for starting a drag.
    press_serial: Option<u32>,
    selection: Option<WlDataOffer>,
    sequence: u64,
    clipboard: Option<(WlDataSource, Owned)>,
    dragged: Option<WlDataSource>,
    incoming: Option<Incoming>,
    events: Vec<UiEvent>,
    /// Gezik's scale factor: Wayland speaks logical pixels, Gezik's handler physical ones.
    scale: f64,
}

struct Shared {
    conn: Connection,
    qh: QueueHandle<State>,
    surface: WlSurface,
    inner: Mutex<Inner>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl Shared {
    fn inner(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn queue(&self, event: UiEvent) {
        let mut inner = self.inner();
        if matches!(event, UiEvent::Over { .. }) && matches!(inner.events.last(), Some(UiEvent::Over { .. })) {
            inner.events.pop();
        }
        inner.events.push(event);
        drop(inner);
        (self.wake)();
    }

    /// Reads type `mime` of `offer` (blocking the UI thread at most a second).
    fn receive(&self, offer: &WlDataOffer, mime: &str) -> Option<Vec<u8>> {
        let (reader, writer) = pipe()?;
        offer.receive(mime.to_owned(), writer.as_fd());
        self.conn.flush().ok()?;
        drop(writer);
        read_with_timeout(reader, TRANSFER_TIMEOUT)
    }

    fn create_source(&self, owned: Owned, types: &[&str]) -> Option<WlDataSource> {
        let manager = self.inner().manager.clone()?;
        let source = manager.create_data_source(&self.qh, Mutex::new(owned));
        for mime in types {
            source.offer((*mime).to_owned());
        }
        Some(source)
    }

    fn offer(&self) -> Option<Offer> {
        let (offer, known) = {
            let inner = self.inner();
            let incoming = inner.incoming.as_ref()?;
            (incoming.offer.clone(), incoming.paths.clone())
        };
        let info = offer.data::<OfferData>()?;
        let allowed = {
            let info = info.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            let actions = info.source_actions;
            Allowed { copy: actions.contains(DndAction::Copy), move_: actions.contains(DndAction::Move) }
        };
        let paths = match known {
            Some(paths) => paths,
            None => {
                let text = self.receive(&offer, URI_LIST).unwrap_or_default();
                let paths = uri::parse_uri_list(&String::from_utf8_lossy(&text));
                if let Some(incoming) = self.inner().incoming.as_mut() {
                    incoming.paths = Some(paths.clone());
                }
                paths
            }
        };
        Some(Offer { paths, allowed, right: false })
    }
}

/// A pipe whose ends are closed on exec.
fn pipe() -> Option<(OwnedFd, OwnedFd)> {
    use std::os::fd::FromRawFd;
    let mut fds = [0; 2];
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
        return None;
    }
    Some(unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) })
}

/// All of `fd` until its writer closes it, or None after `timeout`.
pub(crate) fn read_with_timeout(fd: OwnedFd, timeout: Duration) -> Option<Vec<u8>> {
    use std::os::fd::AsRawFd;
    let deadline = Instant::now() + timeout;
    let mut file = std::fs::File::from(fd);
    let mut data = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return None;
        }
        let mut poll = libc::pollfd { fd: file.as_raw_fd(), events: libc::POLLIN, revents: 0 };
        let ready = unsafe { libc::poll(&mut poll, 1, left.as_millis().min(i32::MAX as u128) as i32) };
        if ready < 0 {
            return None;
        }
        if ready == 0 {
            continue;
        }
        match file.read(&mut buffer) {
            Ok(0) => return Some(data),
            Ok(n) => data.extend_from_slice(&buffer[..n]),
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return None,
        }
    }
}

/// The dispatch thread's state: the shared part, and nothing else.
struct State(Arc<Shared>);

impl Dispatch<WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let wl_registry::Event::Global { name, interface, version } = event else { return };
        let mut inner = state.0.inner();
        if interface == WlSeat::interface().name && inner.seat.is_none() {
            inner.seat = Some(registry.bind::<WlSeat, _, _>(name, version.min(5), qh, ()));
        } else if interface == WlDataDeviceManager::interface().name && inner.manager.is_none() {
            inner.manager = Some(registry.bind::<WlDataDeviceManager, _, _>(name, version.min(3), qh, ()));
        }
        if inner.device.is_none()
            && let (Some(manager), Some(seat)) = (&inner.manager, &inner.seat)
        {
            inner.device = Some(manager.get_data_device(seat, qh, ()));
        }
    }
}

impl Dispatch<WlSeat, ()> for State {
    fn event(state: &mut Self, seat: &WlSeat, event: wl_seat::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        let wl_seat::Event::Capabilities { capabilities: WEnum::Value(caps) } = event else { return };
        let mut inner = state.0.inner();
        // Devices come and go (a keyboard plugged in or out): an object for a device that left
        // gets no more events, so it is let go and a new one made when the device is back.
        match (caps.contains(Capability::Pointer), inner.pointer.take()) {
            (true, Some(pointer)) => inner.pointer = Some(pointer),
            (true, None) => inner.pointer = Some(seat.get_pointer(qh, ())),
            (false, Some(pointer)) if pointer.version() >= 3 => pointer.release(),
            (false, _) => {}
        }
        match (caps.contains(Capability::Keyboard), inner.keyboard.take()) {
            (true, Some(keyboard)) => inner.keyboard = Some(keyboard),
            (true, None) => inner.keyboard = Some(seat.get_keyboard(qh, ())),
            (false, Some(keyboard)) if keyboard.version() >= 3 => keyboard.release(),
            (false, _) => {}
        }
    }
}

impl Dispatch<WlPointer, ()> for State {
    fn event(state: &mut Self, _: &WlPointer, event: wl_pointer::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        let mut inner = state.0.inner();
        match event {
            wl_pointer::Event::Enter { serial, .. } => inner.serial = Some(serial),
            wl_pointer::Event::Button { serial, state: WEnum::Value(wl_pointer::ButtonState::Pressed), .. } => {
                inner.serial = Some(serial);
                inner.press_serial = Some(serial);
            }
            _ => {}
        }
    }
}

impl Dispatch<WlKeyboard, ()> for State {
    fn event(
        state: &mut Self,
        _: &WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let mut inner = state.0.inner();
        match event {
            wl_keyboard::Event::Enter { serial, .. } | wl_keyboard::Event::Key { serial, .. } => {
                inner.serial = Some(serial);
            }
            // Its keymap file descriptor is closed as the event drops.
            _ => {}
        }
    }
}

impl Dispatch<WlDataDeviceManager, ()> for State {
    fn event(
        _: &mut Self,
        _: &WlDataDeviceManager,
        _: <WlDataDeviceManager as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlDataDevice, ()> for State {
    fn event(
        state: &mut Self,
        _: &WlDataDevice,
        event: wl_data_device::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let shared = state.0.clone();
        match event {
            wl_data_device::Event::Selection { id } => {
                let mut inner = shared.inner();
                if let Some(old) = std::mem::replace(&mut inner.selection, id) {
                    old.destroy();
                }
                inner.sequence += 1;
            }
            wl_data_device::Event::Enter { serial, surface, x, y, id } => {
                let Some(offer) = id else { return };
                if surface.id() != shared.surface.id() {
                    offer.destroy();
                    return;
                }
                let scale = {
                    let mut inner = shared.inner();
                    inner.incoming = Some(Incoming { offer, serial, x, y, paths: None });
                    inner.scale
                };
                shared.queue(UiEvent::Over { offer: None, x: x * scale, y: y * scale, keys: Keys::default() });
            }
            wl_data_device::Event::Motion { x, y, .. } => {
                let scale = {
                    let mut inner = shared.inner();
                    let Some(incoming) = inner.incoming.as_mut() else { return };
                    (incoming.x, incoming.y) = (x, y);
                    inner.scale
                };
                shared.queue(UiEvent::Over { offer: None, x: x * scale, y: y * scale, keys: Keys::default() });
            }
            wl_data_device::Event::Leave => {
                let incoming = shared.inner().incoming.take();
                if let Some(incoming) = incoming {
                    incoming.offer.destroy();
                    shared.queue(UiEvent::Leave);
                }
            }
            wl_data_device::Event::Drop if shared.inner().incoming.is_some() => {
                shared.queue(UiEvent::Dropped { offer: None, x: 0.0, y: 0.0, keys: Keys::default() });
            }
            _ => {}
        }
    }

    event_created_child!(State, WlDataDevice, [
        wl_data_device::EVT_DATA_OFFER_OPCODE => (WlDataOffer, OfferData::default()),
    ]);
}

impl Dispatch<WlDataOffer, OfferData> for State {
    fn event(
        _: &mut Self,
        _: &WlDataOffer,
        event: wl_data_offer::Event,
        data: &OfferData,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let mut info = data.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        match event {
            wl_data_offer::Event::Offer { mime_type } => info.types.push(mime_type),
            wl_data_offer::Event::SourceActions { source_actions: WEnum::Value(actions) } => {
                info.source_actions = actions
            }
            _ => {}
        }
    }
}

impl Dispatch<WlDataSource, Mutex<Owned>> for State {
    fn event(
        state: &mut Self,
        source: &WlDataSource,
        event: wl_data_source::Event,
        owned: &Mutex<Owned>,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let shared = state.0.clone();
        match event {
            wl_data_source::Event::Send { mime_type, fd } => {
                let bytes = owned.lock().unwrap_or_else(std::sync::PoisonError::into_inner).render(&mime_type);
                if let Some(bytes) = bytes {
                    let _ = std::fs::File::from(fd).write_all(&bytes);
                }
            }
            wl_data_source::Event::Cancelled => {
                let dragged = {
                    let mut inner = shared.inner();
                    if inner.clipboard.as_ref().is_some_and(|(s, _)| s == source) {
                        inner.clipboard = None;
                    }
                    let dragged = inner.dragged.as_ref() == Some(source);
                    if dragged {
                        inner.dragged = None;
                    }
                    dragged
                };
                source.destroy();
                if dragged {
                    shared.queue(UiEvent::SourceEnded(DragEnd::Cancelled));
                }
            }
            wl_data_source::Event::DndFinished => {
                let dragged = {
                    let mut inner = shared.inner();
                    let dragged = inner.dragged.as_ref() == Some(source);
                    if dragged {
                        inner.dragged = None;
                    }
                    dragged
                };
                source.destroy();
                if dragged {
                    shared.queue(UiEvent::SourceEnded(DragEnd::Dropped));
                }
            }
            _ => {}
        }
    }
}

/// The Wayland side of the clipboard and drag and drop.
pub(crate) struct Wayland(Arc<Shared>);

impl Wayland {
    /// Joins winit's connection (`display`) and takes drops on its `surface`.
    ///
    /// # Safety
    /// Both pointers come from winit's window handle and live as long as the window.
    pub unsafe fn start(
        display: *mut std::ffi::c_void,
        surface: *mut std::ffi::c_void,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Wayland, String> {
        let backend = unsafe { WlBackend::from_foreign_display(display.cast()) };
        let conn = Connection::from_backend(backend);
        let surface_id =
            unsafe { ObjectId::from_ptr(WlSurface::interface(), surface.cast()) }.map_err(|e| e.to_string())?;
        let surface = WlSurface::from_id(&conn, surface_id).map_err(|e| e.to_string())?;
        let mut queue: EventQueue<State> = conn.new_event_queue();
        let qh = queue.handle();
        let shared = Arc::new(Shared {
            conn: conn.clone(),
            qh: qh.clone(),
            surface,
            inner: Mutex::new(Inner { scale: 1.0, ..Default::default() }),
            wake,
        });
        conn.display().get_registry(&qh, ());
        let mut state = State(shared.clone());
        // The globals, the seat's capabilities and the data device, before Gezik uses them.
        queue.roundtrip(&mut state).map_err(|e| e.to_string())?;
        queue.roundtrip(&mut state).map_err(|e| e.to_string())?;
        std::thread::Builder::new()
            .name("gezik-wayland".into())
            .spawn(move || while queue.blocking_dispatch(&mut state).is_ok() {})
            .map_err(|e| e.to_string())?;
        Ok(Wayland(shared))
    }
}

impl super::Backend for Wayland {
    fn write_files(&self, paths: &[PathBuf], cut: bool) -> Result<(), ClipboardError> {
        let s = &self.0;
        let (device, serial) = {
            let inner = s.inner();
            (inner.device.clone(), inner.serial)
        };
        let (Some(device), Some(serial)) = (device, serial) else {
            return Err(ClipboardError::Failed("the window needs the keyboard to use the clipboard".into()));
        };
        let owned = Owned { paths: paths.to_vec(), cut };
        let source = s
            .create_source(owned.clone(), &[GNOME_FILES, URI_LIST, KDE_CUT, TEXT])
            .ok_or(ClipboardError::Failed("no Wayland data device".into()))?;
        device.set_selection(Some(&source), serial);
        let _ = s.conn.flush();
        if let Some((old, _)) = s.inner().clipboard.replace((source, owned)) {
            old.destroy();
        }
        Ok(())
    }

    fn read_files(&self) -> Result<Option<ClipboardFiles>, ClipboardError> {
        let s = &self.0;
        let offer = s.inner().selection.clone();
        let Some(offer) = offer else {
            // The compositor tells the selection only to the window with the keyboard; Gezik's
            // own (if any) is what it offers then.
            return Ok(s
                .inner()
                .clipboard
                .as_ref()
                .map(|(_, o)| ClipboardFiles { paths: o.paths.clone(), cut: o.cut }));
        };
        let types = offer
            .data::<OfferData>()
            .map(|d| d.lock().unwrap_or_else(std::sync::PoisonError::into_inner).types.clone())
            .unwrap_or_default();
        let Some(mime) = preferred_mime(&types) else { return Ok(None) };
        let Some(bytes) = s.receive(&offer, mime) else { return Ok(None) };
        let text = String::from_utf8_lossy(&bytes);
        if mime == GNOME_FILES {
            return Ok(uri::parse_gnome_copied_files(&text).map(|(cut, paths)| ClipboardFiles { paths, cut }));
        }
        let paths = uri::parse_uri_list(&text);
        if paths.is_empty() {
            return Ok(None);
        }
        let cut = types.iter().any(|t| t == KDE_CUT) && s.receive(&offer, KDE_CUT).is_some_and(|v| v == b"1");
        Ok(Some(ClipboardFiles { paths, cut }))
    }

    fn sequence(&self) -> u64 {
        self.0.inner().sequence
    }

    fn clear(&self) -> Result<(), ClipboardError> {
        let s = &self.0;
        let (device, serial, owned) = {
            let mut inner = s.inner();
            (inner.device.clone(), inner.serial, inner.clipboard.take())
        };
        if let (Some(device), Some(serial), Some((source, _))) = (device, serial, owned) {
            device.set_selection(None, serial);
            source.destroy();
            let _ = s.conn.flush();
        }
        Ok(())
    }

    fn take_events(&self) -> Vec<UiEvent> {
        let s = &self.0;
        let events = std::mem::take(&mut s.inner().events);
        events
            .into_iter()
            .filter_map(|event| match event {
                UiEvent::Over { x, y, keys, .. } => {
                    s.offer().map(|offer| UiEvent::Over { offer: Some(offer), x, y, keys })
                }
                UiEvent::Dropped { .. } => {
                    let (x, y, scale) = {
                        let inner = s.inner();
                        let incoming = inner.incoming.as_ref()?;
                        (incoming.x, incoming.y, inner.scale)
                    };
                    s.offer().map(|offer| UiEvent::Dropped {
                        offer: Some(offer),
                        x: x * scale,
                        y: y * scale,
                        keys: Keys::default(),
                    })
                }
                other => Some(other),
            })
            .collect()
    }

    fn answer(&self, answer: &Answer) {
        let s = &self.0;
        let inner = s.inner();
        let Some(incoming) = inner.incoming.as_ref() else { return };
        // Only "copy" is ever chosen with the source: Gezik moves the files itself, so the
        // source must not delete them after the drop.
        if answer.effect.is_some() {
            incoming.offer.set_actions(DndAction::Copy, DndAction::Copy);
            incoming.offer.accept(incoming.serial, Some(URI_LIST.to_owned()));
        } else {
            incoming.offer.set_actions(DndAction::None, DndAction::None);
            incoming.offer.accept(incoming.serial, None);
        }
        let _ = s.conn.flush();
    }

    fn finish(&self, done: Option<Effect>) {
        let s = &self.0;
        let Some(incoming) = s.inner().incoming.take() else { return };
        if done.is_some() {
            incoming.offer.finish();
        }
        incoming.offer.destroy();
        let _ = s.conn.flush();
    }

    fn drag_out(&self, paths: &[PathBuf]) -> Result<Box<dyn OutsideDrag>, String> {
        let s = &self.0;
        let (device, serial) = {
            let inner = s.inner();
            (inner.device.clone(), inner.press_serial)
        };
        let device = device.ok_or("no Wayland data device")?;
        let serial = serial.ok_or("no button press to drag from")?;
        let source = s
            .create_source(Owned { paths: paths.to_vec(), cut: false }, &[URI_LIST, TEXT])
            .ok_or("no Wayland data device")?;
        source.set_actions(DndAction::Copy | DndAction::Move);
        device.start_drag(Some(&source), &s.surface, None, serial);
        s.conn.flush().map_err(|e| e.to_string())?;
        s.inner().dragged = Some(source);
        Ok(Box::new(WaylandDrag))
    }

    fn set_scale(&self, scale: f64) {
        self.0.inner().scale = scale;
    }
}

/// The compositor runs Gezik's drag outside the window: nothing to do here.
struct WaylandDrag;

impl OutsideDrag for WaylandDrag {
    fn moved(&mut self, _x: f64, _y: f64, _keys: Keys) {}
    fn released(&mut self) {}
    fn cancel(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gnome_files_are_read_before_a_plain_uri_list() {
        let types = |list: &[&str]| list.iter().map(|t| (*t).to_owned()).collect::<Vec<_>>();
        assert_eq!(preferred_mime(&types(&[URI_LIST, GNOME_FILES])), Some(GNOME_FILES));
        assert_eq!(preferred_mime(&types(&["text/plain", URI_LIST])), Some(URI_LIST));
        assert_eq!(preferred_mime(&types(&["image/png"])), None);
    }

    #[test]
    fn reading_a_pipe_gives_up_after_its_timeout() {
        let (reader, writer) = pipe().unwrap();
        let started = Instant::now();
        assert_eq!(read_with_timeout(reader, Duration::from_millis(100)), None);
        assert!(started.elapsed() < Duration::from_secs(1));
        drop(writer);
        let (reader, writer) = pipe().unwrap();
        std::fs::File::from(writer).write_all(b"file:///x\r\n").unwrap();
        assert_eq!(read_with_timeout(reader, Duration::from_secs(1)), Some(b"file:///x\r\n".to_vec()));
    }
}
