//! Q3 on X11: TARGETS, then image/png (INCR) and UTF8_STRING from the CLIPBOARD owner,
//! timed as Gezik's `transfer` would (one deadline for the whole transfer).
#[cfg(not(all(unix, not(target_os = "macos"))))]
fn main() {}

#[cfg(all(unix, not(target_os = "macos")))]
fn main() {
    use std::time::{Duration, Instant};

    use x11rb::connection::{Connection, RequestConnection};
    use x11rb::protocol::Event;
    use x11rb::protocol::xproto::*;
    use x11rb::{CURRENT_TIME, NONE};

    x11rb::atom_manager! {
        Atoms: AtomsCookie {
            CLIPBOARD, TARGETS, INCR, UTF8_STRING,
            TEXT_PLAIN: b"text/plain;charset=utf-8",
            IMAGE_PNG: b"image/png",
            GEZIK_TRANSFER,
        }
    }

    let (conn, screen) = x11rb::connect(None).unwrap();
    let root = conn.setup().roots[screen].root;
    let atoms = Atoms::new(&conn).unwrap().reply().unwrap();
    println!("max request: {} bytes", conn.maximum_request_bytes());
    let window = conn.generate_id().unwrap();
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
    .unwrap();
    conn.flush().unwrap();

    // Returns the bytes and the number of INCR pieces.
    let transfer = |target: Atom, timeout: Duration| -> Option<(Vec<u8>, usize)> {
        let property = atoms.GEZIK_TRANSFER;
        conn.delete_property(window, property).ok()?;
        conn.convert_selection(window, atoms.CLIPBOARD, target, property, CURRENT_TIME).ok()?;
        conn.flush().ok()?;
        let deadline = Instant::now() + timeout;
        let next = || -> Option<Event> {
            loop {
                if let Some(e) = conn.poll_for_event().ok()? {
                    return Some(e);
                }
                if Instant::now() > deadline {
                    return None;
                }
                std::thread::sleep(Duration::from_micros(200));
            }
        };
        loop {
            match next()? {
                Event::SelectionNotify(e) if e.property == NONE => return None,
                Event::SelectionNotify(_) => break,
                _ => {}
            }
        }
        let reply = conn.get_property(true, window, property, AtomEnum::ANY, 0, u32::MAX / 4).ok()?.reply().ok()?;
        if reply.type_ != atoms.INCR {
            return Some((reply.value, 0));
        }
        let mut data = Vec::new();
        let mut pieces = 0;
        loop {
            match next()? {
                Event::PropertyNotify(e) if e.atom == property && e.state == Property::NEW_VALUE => {
                    let piece =
                        conn.get_property(true, window, property, AtomEnum::ANY, 0, u32::MAX / 4).ok()?.reply().ok()?;
                    if piece.value.is_empty() {
                        return Some((data, pieces));
                    }
                    pieces += 1;
                    data.extend_from_slice(&piece.value);
                }
                _ => {}
            }
        }
    };

    let t = Instant::now();
    let (targets, _) = transfer(atoms.TARGETS, Duration::from_secs(1)).expect("TARGETS");
    let offered: Vec<Atom> = targets.chunks_exact(4).map(|c| u32::from_ne_bytes(c.try_into().unwrap())).collect();
    let names: Vec<String> = offered
        .iter()
        .filter_map(|a| conn.get_atom_name(*a).ok()?.reply().ok())
        .map(|r| String::from_utf8_lossy(&r.name).into_owned())
        .collect();
    println!("TARGETS in {:?}: {names:?}", t.elapsed());

    if offered.contains(&atoms.IMAGE_PNG) {
        for timeout in [Duration::from_secs(1), Duration::from_secs(10)] {
            let t = Instant::now();
            match transfer(atoms.IMAGE_PNG, timeout) {
                Some((png, pieces)) => {
                    let took = t.elapsed();
                    let dims = image::ImageReader::new(std::io::Cursor::new(&png))
                        .with_guessed_format()
                        .unwrap()
                        .into_dimensions();
                    println!(
                        "image/png (timeout {timeout:?}): {:.2} MB, {pieces} INCR pieces, {took:?}, {dims:?}",
                        png.len() as f64 / 1e6
                    );
                }
                None => println!("image/png (timeout {timeout:?}): gave up after {:?}", t.elapsed()),
            }
        }
    }
    for (name, atom) in [("UTF8_STRING", atoms.UTF8_STRING), ("text/plain;charset=utf-8", atoms.TEXT_PLAIN)] {
        if offered.contains(&atom) {
            let t = Instant::now();
            let text = transfer(atom, Duration::from_secs(1)).map(|(b, _)| String::from_utf8_lossy(&b).into_owned());
            println!("{name}: {text:?} in {:?}", t.elapsed());
        }
    }
}
