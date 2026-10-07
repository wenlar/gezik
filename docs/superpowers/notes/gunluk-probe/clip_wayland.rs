//! Q3 on Wayland: a focused toplevel (the selection is told only to the window with the
//! keyboard), then image/png and text over wl_data_offer.receive + a pipe, as Gezik reads.
#[cfg(not(all(unix, not(target_os = "macos"))))]
fn main() {}

#[cfg(all(unix, not(target_os = "macos")))]
fn main() {
    imp::main()
}

#[cfg(all(unix, not(target_os = "macos")))]
mod imp {
    use std::io::Read;
    use std::os::fd::{AsFd, FromRawFd, OwnedFd};
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    use wayland_client::protocol::wl_buffer::WlBuffer;
    use wayland_client::protocol::wl_compositor::WlCompositor;
    use wayland_client::protocol::wl_data_device::{self, WlDataDevice};
    use wayland_client::protocol::wl_data_device_manager::WlDataDeviceManager;
    use wayland_client::protocol::wl_data_offer::{self, WlDataOffer};
    use wayland_client::protocol::wl_keyboard::{self, WlKeyboard};
    use wayland_client::protocol::wl_registry::{self, WlRegistry};
    use wayland_client::protocol::wl_seat::{self, WlSeat};
    use wayland_client::protocol::wl_shm::{Format, WlShm};
    use wayland_client::protocol::wl_shm_pool::WlShmPool;
    use wayland_client::protocol::wl_surface::WlSurface;
    use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, WEnum, delegate_noop, event_created_child};
    use wayland_protocols::xdg::shell::client::xdg_surface::{self, XdgSurface};
    use wayland_protocols::xdg::shell::client::xdg_toplevel::XdgToplevel;
    use wayland_protocols::xdg::shell::client::xdg_wm_base::{self, XdgWmBase};

    #[derive(Default)]
    struct State {
        compositor: Option<WlCompositor>,
        shm: Option<WlShm>,
        wm: Option<XdgWmBase>,
        seat: Option<WlSeat>,
        manager: Option<WlDataDeviceManager>,
        selection: Option<WlDataOffer>,
        focused: bool,
        configured: bool,
    }

    type Types = Mutex<Vec<String>>;

    impl Dispatch<WlRegistry, ()> for State {
        fn event(s: &mut Self, r: &WlRegistry, e: wl_registry::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
            let wl_registry::Event::Global { name, interface, version } = e else { return };
            match interface.as_str() {
                "wl_compositor" => s.compositor = Some(r.bind(name, version.min(4), qh, ())),
                "wl_shm" => s.shm = Some(r.bind(name, 1, qh, ())),
                "xdg_wm_base" => s.wm = Some(r.bind(name, 1, qh, ())),
                "wl_seat" if s.seat.is_none() => s.seat = Some(r.bind(name, version.min(5), qh, ())),
                "wl_data_device_manager" => s.manager = Some(r.bind(name, version.min(3), qh, ())),
                _ => {}
            }
        }
    }
    impl Dispatch<WlSeat, ()> for State {
        fn event(_: &mut Self, seat: &WlSeat, e: wl_seat::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
            if let wl_seat::Event::Capabilities { capabilities: WEnum::Value(c) } = e
                && c.contains(wl_seat::Capability::Keyboard)
            {
                seat.get_keyboard(qh, ());
            }
        }
    }
    impl Dispatch<WlKeyboard, ()> for State {
        fn event(s: &mut Self, _: &WlKeyboard, e: wl_keyboard::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
            if let wl_keyboard::Event::Enter { .. } = e {
                s.focused = true;
            }
        }
    }
    impl Dispatch<XdgWmBase, ()> for State {
        fn event(_: &mut Self, wm: &XdgWmBase, e: xdg_wm_base::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
            if let xdg_wm_base::Event::Ping { serial } = e {
                wm.pong(serial);
            }
        }
    }
    impl Dispatch<XdgSurface, ()> for State {
        fn event(s: &mut Self, x: &XdgSurface, e: xdg_surface::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
            if let xdg_surface::Event::Configure { serial } = e {
                x.ack_configure(serial);
                s.configured = true;
            }
        }
    }
    impl Dispatch<WlDataDevice, ()> for State {
        fn event(s: &mut Self, _: &WlDataDevice, e: wl_data_device::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
            if let wl_data_device::Event::Selection { id } = e {
                s.selection = id;
            }
        }
        event_created_child!(State, WlDataDevice, [
            wl_data_device::EVT_DATA_OFFER_OPCODE => (WlDataOffer, Types::default()),
        ]);
    }
    impl Dispatch<WlDataOffer, Types> for State {
        fn event(_: &mut Self, _: &WlDataOffer, e: wl_data_offer::Event, t: &Types, _: &Connection, _: &QueueHandle<Self>) {
            if let wl_data_offer::Event::Offer { mime_type } = e {
                t.lock().unwrap().push(mime_type);
            }
        }
    }
    delegate_noop!(State: ignore WlCompositor);
    delegate_noop!(State: ignore WlShm);
    delegate_noop!(State: ignore WlShmPool);
    delegate_noop!(State: ignore WlBuffer);
    delegate_noop!(State: ignore WlSurface);
    delegate_noop!(State: ignore XdgToplevel);
    delegate_noop!(State: ignore WlDataDeviceManager);

    fn memfd(len: usize) -> OwnedFd {
        unsafe {
            let fd = libc::memfd_create(c"probe7".as_ptr(), libc::MFD_CLOEXEC);
            libc::ftruncate(fd, len as i64);
            OwnedFd::from_raw_fd(fd)
        }
    }

    fn receive(conn: &Connection, offer: &WlDataOffer, mime: &str, timeout: Duration) -> Option<Vec<u8>> {
        let mut fds = [0; 2];
        if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
            return None;
        }
        let (reader, writer) = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
        offer.receive(mime.to_owned(), writer.as_fd());
        conn.flush().ok()?;
        drop(writer);
        // Same loop as gezik-platform's read_with_timeout (poll + read until EOF).
        let deadline = Instant::now() + timeout;
        let mut file = std::fs::File::from(reader);
        let mut data = Vec::new();
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return None;
            }
            let mut p = libc::pollfd { fd: std::os::fd::AsRawFd::as_raw_fd(&file), events: libc::POLLIN, revents: 0 };
            if unsafe { libc::poll(&mut p, 1, left.as_millis() as i32) } <= 0 {
                continue;
            }
            match file.read(&mut buf) {
                Ok(0) => return Some(data),
                Ok(n) => data.extend_from_slice(&buf[..n]),
                Err(_) => return None,
            }
        }
    }

    pub fn main() {
        let conn = Connection::connect_to_env().expect("WAYLAND_DISPLAY");
        let mut queue = conn.new_event_queue();
        let qh = queue.handle();
        conn.display().get_registry(&qh, ());
        let mut s = State::default();
        queue.roundtrip(&mut s).unwrap();
        let surface = s.compositor.as_ref().unwrap().create_surface(&qh, ());
        let xdg = s.wm.as_ref().unwrap().get_xdg_surface(&surface, &qh, ());
        xdg.get_toplevel(&qh, ()).set_title("probe7".into());
        surface.commit();
        let _device = s.manager.as_ref().unwrap().get_data_device(s.seat.as_ref().unwrap(), &qh, ());
        while !s.configured {
            queue.blocking_dispatch(&mut s).unwrap();
        }
        let (w, h) = (64, 64);
        let fd = memfd(w * h * 4);
        let pool = s.shm.as_ref().unwrap().create_pool(fd.as_fd(), (w * h * 4) as i32, &qh, ());
        let buffer = pool.create_buffer(0, w as i32, h as i32, (w * 4) as i32, Format::Argb8888, &qh, ());
        surface.attach(Some(&buffer), 0, 0);
        surface.commit();
        let t = Instant::now();
        while !(s.focused && s.selection.is_some()) && t.elapsed() < Duration::from_secs(5) {
            conn.flush().unwrap();
            let _ = queue.roundtrip(&mut s);
            std::thread::sleep(Duration::from_millis(20));
        }
        println!("focused {} selection {}", s.focused, s.selection.is_some());
        let Some(offer) = s.selection.clone() else { return };
        let types = offer.data::<Types>().unwrap().lock().unwrap().clone();
        println!("offered: {types:?}");
        for mime in ["image/png", "text/plain;charset=utf-8", "UTF8_STRING", "text/plain"] {
            if !types.iter().any(|t| t == mime) {
                continue;
            }
            let t = Instant::now();
            let bytes = receive(&conn, &offer, mime, Duration::from_secs(1));
            let took = t.elapsed();
            match bytes {
                Some(b) if mime == "image/png" => {
                    let dims = image::ImageReader::new(std::io::Cursor::new(&b)).with_guessed_format().unwrap().into_dimensions();
                    println!("{mime}: {:.2} MB in {took:?}, {dims:?}", b.len() as f64 / 1e6);
                }
                Some(b) => println!("{mime}: {:?} in {took:?}", String::from_utf8_lossy(&b)),
                None => println!("{mime}: gave up after {took:?}"),
            }
        }
    }
}
