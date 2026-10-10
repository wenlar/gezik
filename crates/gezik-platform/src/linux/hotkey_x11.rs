//! X11: the global shortcut by XGrabKey on the root window, with the CapsLock and NumLock
//! variants (deviation 15), on Gezik's own connection and thread. A grab another client holds
//! is BadAccess: `Taken`. Key repeats within 250 ms of the last press are one press.

#![cfg_attr(not(all(unix, not(target_os = "macos"))), allow(dead_code))]

/// None, LockMask (CapsLock), Mod2Mask (NumLock), both.
pub const LOCK_VARIANTS: [u16; 4] = [0, 2, 16, 18];

/// The first keycode whose keysyms hold `keysym` (`GetKeyboardMapping`'s table).
pub fn keycode_for(keysyms: &[u32], per_keycode: u8, min_keycode: u8, keysym: u32) -> Option<u8> {
    if per_keycode == 0 {
        return None;
    }
    let at = keysyms.chunks(usize::from(per_keycode)).position(|codes| codes.contains(&keysym))?;
    u8::try_from(usize::from(min_keycode) + at).ok()
}

/// X times are milliseconds that wrap.
pub fn is_repeat(last: Option<u32>, now: u32) -> bool {
    last.is_some_and(|then| now.wrapping_sub(then) < 250)
}

#[cfg(all(unix, not(target_os = "macos")))]
pub use grabbing::{Grab, grab};

#[cfg(all(unix, not(target_os = "macos")))]
mod grabbing {
    use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
    use std::sync::{Arc, Mutex};

    use x11rb::connection::Connection;
    use x11rb::errors::ReplyError;
    use x11rb::protocol::xproto::{
        AtomEnum, ClientMessageEvent, ConnectionExt as _, CreateWindowAux, EventMask, GrabMode, ModMask, WindowClass,
    };
    use x11rb::protocol::{ErrorKind, Event};
    use x11rb::rust_connection::RustConnection;

    use super::*;
    use crate::hotkey::{Combo, HotkeyError, OnPress, OnReady, x11_keysym, x11_modifiers};

    type Held = Arc<Mutex<Option<(Arc<RustConnection>, u32)>>>;

    /// The grab while held: dropped, a message to Gezik's own small window ends the thread,
    /// which lets the key go. Drop never waits for the thread, so it may run inside `on_press`.
    pub struct Grab {
        held: Held,
        stop: Arc<AtomicBool>,
    }

    pub fn grab(combo: Combo, on_press: OnPress, on_ready: OnReady) -> Grab {
        let grab = Grab { held: Arc::default(), stop: Arc::default() };
        let (held, stop) = (grab.held.clone(), grab.stop.clone());
        // shortcut: if no thread can be started, on_ready is never called (as hotkey/windows.rs).
        let _ = std::thread::Builder::new()
            .name("gezik-x11-hotkey".into())
            .spawn(move || run(combo, &held, &stop, on_press, on_ready));
        grab
    }

    fn run(combo: Combo, held: &Held, stop: &AtomicBool, on_press: OnPress, on_ready: OnReady) {
        let failed = HotkeyError::Failed;
        let (conn, screen) = match RustConnection::connect(None) {
            Ok(found) => found,
            Err(err) => return on_ready(Err(failed(err.to_string()))),
        };
        let conn = Arc::new(conn);
        let Some(root) = conn.setup().roots.get(screen).map(|s| s.root) else {
            return on_ready(Err(failed("no X screen".into())));
        };
        // A 1x1 input-only window of ours: Drop's message to it wakes this thread.
        let made = conn.generate_id().ok().filter(|&wake| {
            conn.create_window(0, wake, root, 0, 0, 1, 1, 0, WindowClass::INPUT_ONLY, 0, &CreateWindowAux::new())
                .is_ok_and(|cookie| cookie.check().is_ok())
        });
        let Some(wake) = made else { return on_ready(Err(failed("cannot make a window".into()))) };
        if let Ok(mut slot) = held.lock() {
            *slot = Some((conn.clone(), wake));
        }
        // Dropped meanwhile: the connection closes here, and its window with it.
        if stop.load(SeqCst) {
            return;
        }
        let (min, max) = (conn.setup().min_keycode, conn.setup().max_keycode);
        let mapping = conn
            .get_keyboard_mapping(min, max.saturating_sub(min).saturating_add(1))
            .ok()
            .and_then(|cookie| cookie.reply().ok());
        let code = mapping.and_then(|m| keycode_for(&m.keysyms, m.keysyms_per_keycode, min, x11_keysym(combo.key)));
        let Some(code) = code else { return on_ready(Err(failed("this keyboard has no such key".into()))) };
        let mods = x11_modifiers(&combo);
        let (mut taken, mut other) = (false, None);
        for lock in LOCK_VARIANTS {
            let grabbed =
                conn.grab_key(false, root, ModMask::from(mods | lock), code, GrabMode::ASYNC, GrabMode::ASYNC);
            match grabbed.map_err(ReplyError::from).and_then(|cookie| cookie.check()) {
                Ok(()) => {}
                Err(ReplyError::X11Error(err)) if err.error_kind == ErrorKind::Access => taken = true,
                Err(err) => other = Some(err.to_string()),
            }
        }
        let release = || {
            for lock in LOCK_VARIANTS {
                let _ = conn.ungrab_key(code, root, ModMask::from(mods | lock));
            }
            let _ = conn.destroy_window(wake);
            let _ = conn.flush();
        };
        if taken || other.is_some() {
            release();
            return on_ready(Err(if taken { HotkeyError::Taken } else { failed(other.unwrap_or_default()) }));
        }
        on_ready(Ok(()));
        let mut last = None;
        while let Ok(event) = conn.wait_for_event() {
            match event {
                Event::KeyPress(press) if press.detail == code => {
                    if !is_repeat(last, press.time) {
                        on_press();
                    }
                    last = Some(press.time);
                }
                // Only Gezik's own window: other clients' messages to it are ignored too.
                Event::ClientMessage(message) if message.window == wake && stop.load(SeqCst) => break,
                _ => {}
            }
        }
        release();
    }

    impl Drop for Grab {
        fn drop(&mut self) {
            self.stop.store(true, SeqCst);
            if let Ok(mut slot) = self.held.lock()
                && let Some((conn, wake)) = slot.take()
            {
                // An empty event mask: the message goes to the window's owner, Gezik's thread.
                let message = ClientMessageEvent::new(32, wake, AtomEnum::NONE, [0u32; 5]);
                let _ = conn.send_event(false, wake, EventMask::NO_EVENT, message);
                let _ = conn.flush();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keycodes_are_found_in_the_mapping() {
        // Two keysyms per keycode from keycode 8: 8 = (a, A), 9 = (e, E), 10 = (F1, none).
        let keysyms = [0x61, 0x41, 0x65, 0x45, 0xFFBE, 0];
        assert_eq!(keycode_for(&keysyms, 2, 8, 0x65), Some(9));
        assert_eq!(keycode_for(&keysyms, 2, 8, 0xFFBE), Some(10));
        assert_eq!(keycode_for(&keysyms, 2, 8, 0x7A), None, "no z on this keyboard");
        assert_eq!(keycode_for(&keysyms, 0, 8, 0x61), None);
        assert_eq!(keycode_for(&[0x61; 600], 1, 8, 0x61), Some(8));
        let mut late = vec![0u32; 600];
        late[599] = 0x61;
        assert_eq!(keycode_for(&late, 1, 8, 0x61), None, "past keycode 255");
    }

    #[test]
    fn repeats_are_dropped() {
        assert!(!is_repeat(None, 1000));
        assert!(is_repeat(Some(1000), 1033), "auto-repeat");
        assert!(!is_repeat(Some(1000), 1300), "a new press");
        assert!(is_repeat(Some(u32::MAX - 10), 20), "the X server's clock wraps");
        assert_eq!(LOCK_VARIANTS, [0, 2, 16, 18], "none, CapsLock, NumLock, both");
    }
}
