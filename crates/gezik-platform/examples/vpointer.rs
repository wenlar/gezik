//! A virtual pointer for trying drag and drop under a wlroots compositor with no real mouse
//! (scripts/linux/test.sh): `vpointer WIDTH HEIGHT move X Y | down | up | sleep MS ...`,
//! positions in the output's pixels.

#[cfg(all(unix, not(target_os = "macos")))]
fn main() {
    if let Err(err) = linux::run(std::env::args().skip(1).collect()) {
        eprintln!("vpointer: {err}");
        std::process::exit(1);
    }
}

#[cfg(not(all(unix, not(target_os = "macos"))))]
fn main() {
    eprintln!("vpointer: Wayland only");
}

#[cfg(all(unix, not(target_os = "macos")))]
mod linux {
    use std::time::{Duration, Instant};

    use wayland_client::protocol::wl_pointer::ButtonState;
    use wayland_client::protocol::wl_registry::{self, WlRegistry};
    use wayland_client::protocol::wl_seat::WlSeat;
    use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, delegate_noop};
    use wayland_protocols_wlr::virtual_pointer::v1::client::zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1;
    use wayland_protocols_wlr::virtual_pointer::v1::client::zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1;

    /// Linux's code for the left button.
    const BTN_LEFT: u32 = 0x110;

    #[derive(Default)]
    struct State {
        manager: Option<ZwlrVirtualPointerManagerV1>,
        seat: Option<WlSeat>,
    }

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
            if interface == ZwlrVirtualPointerManagerV1::interface().name {
                state.manager = Some(registry.bind(name, version.min(2), qh, ()));
            } else if interface == WlSeat::interface().name && state.seat.is_none() {
                state.seat = Some(registry.bind(name, version.min(7), qh, ()));
            }
        }
    }

    delegate_noop!(State: ignore ZwlrVirtualPointerManagerV1);
    delegate_noop!(State: ignore ZwlrVirtualPointerV1);
    delegate_noop!(State: ignore WlSeat);

    pub fn run(args: Vec<String>) -> Result<(), String> {
        let number = |text: Option<&String>| -> Result<u32, String> {
            text.ok_or("missing number")?.parse().map_err(|e| format!("{e}"))
        };
        let (width, height) = (number(args.first())?, number(args.get(1))?);
        let conn = Connection::connect_to_env().map_err(|e| e.to_string())?;
        let mut queue = conn.new_event_queue();
        let qh = queue.handle();
        conn.display().get_registry(&qh, ());
        let mut state = State::default();
        queue.roundtrip(&mut state).map_err(|e| e.to_string())?;
        let manager = state.manager.clone().ok_or("the compositor has no virtual pointer")?;
        let pointer = manager.create_virtual_pointer(state.seat.as_ref(), &qh, ());
        let start = Instant::now();
        let time = || start.elapsed().as_millis() as u32;
        let mut words = args.iter().skip(2);
        while let Some(word) = words.next() {
            match word.as_str() {
                "move" => {
                    let (x, y) = (number(words.next())?, number(words.next())?);
                    pointer.motion_absolute(time(), x, y, width, height);
                }
                "down" => pointer.button(time(), BTN_LEFT, ButtonState::Pressed),
                "up" => pointer.button(time(), BTN_LEFT, ButtonState::Released),
                "sleep" => {
                    conn.flush().map_err(|e| e.to_string())?;
                    std::thread::sleep(Duration::from_millis(u64::from(number(words.next())?)));
                    continue;
                }
                other => return Err(format!("unknown command {other}")),
            }
            pointer.frame();
            queue.roundtrip(&mut state).map_err(|e| e.to_string())?;
        }
        pointer.destroy();
        queue.roundtrip(&mut state).map_err(|e| e.to_string())?;
        Ok(())
    }
}
