//! A system-wide shortcut that shows or hides Gezik (spec 9 §9.2): registered by one call,
//! released when dropped. Each system's arm is in `hotkey/`; the key and modifier tables of
//! all of them are here, pure and tested on every system. Nothing of this runs unless the
//! user chose a shortcut (there is no default).

#[cfg_attr(all(unix, not(target_os = "macos")), path = "hotkey/linux.rs")]
#[cfg_attr(target_os = "macos", path = "hotkey/macos.rs")]
#[cfg_attr(windows, path = "hotkey/windows.rs")]
mod imp;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComboKey {
    /// `a`-`z`.
    Letter(char),
    /// `0`-`9`.
    Digit(char),
    /// F1-F12.
    F(u8),
}

/// A shortcut as the systems take it; `logo` is Win, Cmd or Super.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Combo {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub logo: bool,
    pub key: ComboKey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotkeyError {
    /// Another app has it.
    Taken,
    /// This desktop has no way to make one (Wayland without the GlobalShortcuts portal).
    Unsupported,
    Failed(String),
}

/// Don't drop the handle inside the callback; hop to the UI thread first (dropping it there
/// still releases the key, but without waiting for its thread).
pub type OnPress = Box<dyn Fn() + Send + Sync>;
pub type OnReady = Box<dyn FnOnce(Result<(), HotkeyError>) + Send>;

/// The shortcut while it is held.
pub struct Hotkey(#[allow(dead_code, reason = "held for its Drop, which releases the shortcut")] imp::Hotkey);

/// Registers `combo`; `on_ready` says whether it worked (later, from any thread), `on_press`
/// is called for each press, from any thread. Never blocks.
pub fn register(combo: Combo, on_press: OnPress, on_ready: OnReady) -> Hotkey {
    Hotkey(imp::register(combo, on_press, on_ready))
}

/// `RegisterHotKey`'s modifiers: MOD_ALT 1, MOD_CONTROL 2, MOD_SHIFT 4, MOD_WIN 8, and
/// MOD_NOREPEAT (a held key is one press).
pub fn windows_modifiers(combo: &Combo) -> u32 {
    let mut out = 0x4000;
    for (held, bit) in [(combo.alt, 1), (combo.ctrl, 2), (combo.shift, 4), (combo.logo, 8)] {
        if held {
            out |= bit;
        }
    }
    out
}

/// The virtual key: `A`-`Z` and `0`-`9` are their ASCII codes, F1 is 0x70.
pub fn windows_vk(key: ComboKey) -> u32 {
    match key {
        ComboKey::Letter(c) => u32::from(c.to_ascii_uppercase()),
        ComboKey::Digit(c) => u32::from(c),
        ComboKey::F(n) => 0x70 + u32::from(n.saturating_sub(1)),
    }
}

/// Carbon's modifiers: cmdKey, shiftKey, optionKey, controlKey.
pub fn carbon_modifiers(combo: &Combo) -> u32 {
    let mut out = 0;
    for (held, bit) in [(combo.logo, 0x0100), (combo.shift, 0x0200), (combo.alt, 0x0800), (combo.ctrl, 0x1000)] {
        if held {
            out |= bit;
        }
    }
    out
}

/// kVK_ANSI_* and kVK_F*: key positions on a US keyboard.
// shortcut: by position, so on layouts that move letters (Turkish Q's ı and i, AZERTY) the
// shortcut is the key at the US letter's place; map through the current layout
// (UCKeyTranslate) if users ask.
pub fn carbon_keycode(key: ComboKey) -> Option<u32> {
    const LETTERS: [u8; 26] = [
        0x00, 0x0B, 0x08, 0x02, 0x0E, 0x03, 0x05, 0x04, 0x22, 0x26, 0x28, 0x25, 0x2E, 0x2D, 0x1F, 0x23, 0x0C, 0x0F,
        0x01, 0x11, 0x20, 0x09, 0x0D, 0x07, 0x10, 0x06,
    ];
    const DIGITS: [u8; 10] = [0x1D, 0x12, 0x13, 0x14, 0x15, 0x17, 0x16, 0x1A, 0x1C, 0x19];
    const F_KEYS: [u8; 12] = [0x7A, 0x78, 0x63, 0x76, 0x60, 0x61, 0x62, 0x64, 0x65, 0x6D, 0x67, 0x6F];
    let code = match key {
        ComboKey::Letter(c) if c.is_ascii_lowercase() => LETTERS[usize::from(c as u8 - b'a')],
        ComboKey::Digit(c) if c.is_ascii_digit() => DIGITS[usize::from(c as u8 - b'0')],
        ComboKey::F(n) if (1..=12).contains(&n) => F_KEYS[usize::from(n - 1)],
        _ => return None,
    };
    Some(u32::from(code))
}

/// X11's modifier masks: ShiftMask 1, ControlMask 4, Mod1Mask (Alt) 8, Mod4Mask (Super) 64.
pub fn x11_modifiers(combo: &Combo) -> u16 {
    let mut out = 0;
    for (held, bit) in [(combo.shift, 1), (combo.ctrl, 4), (combo.alt, 8), (combo.logo, 64)] {
        if held {
            out |= bit;
        }
    }
    out
}

/// The keysym: Latin letters and digits are their ASCII codes, F1 is 0xFFBE.
pub fn x11_keysym(key: ComboKey) -> u32 {
    match key {
        ComboKey::Letter(c) => u32::from(c.to_ascii_lowercase()),
        ComboKey::Digit(c) => u32::from(c),
        ComboKey::F(n) => 0xFFBE + u32::from(n.saturating_sub(1)),
    }
}

/// The GlobalShortcuts portal's `preferred_trigger` (the XDG shortcuts format: modifiers
/// CTRL, ALT, SHIFT, LOGO, then the keysym's name).
pub fn portal_trigger(combo: &Combo) -> String {
    let mut parts: Vec<String> = Vec::new();
    for (held, name) in [(combo.ctrl, "CTRL"), (combo.alt, "ALT"), (combo.shift, "SHIFT"), (combo.logo, "LOGO")] {
        if held {
            parts.push(name.to_owned());
        }
    }
    parts.push(match combo.key {
        ComboKey::Letter(c) => c.to_ascii_lowercase().to_string(),
        ComboKey::Digit(c) => c.to_string(),
        ComboKey::F(n) => format!("F{n}"),
    });
    parts.join("+")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn combo(ctrl: bool, alt: bool, shift: bool, logo: bool, key: ComboKey) -> Combo {
        Combo { ctrl, alt, shift, logo, key }
    }

    #[test]
    fn windows_tables() {
        assert_eq!(windows_modifiers(&combo(false, false, true, true, ComboKey::Letter('e'))), 0x4000 | 4 | 8);
        assert_eq!(windows_modifiers(&combo(true, true, false, false, ComboKey::F(1))), 0x4000 | 2 | 1);
        assert_eq!(windows_vk(ComboKey::Letter('e')), 0x45);
        assert_eq!(windows_vk(ComboKey::Digit('7')), 0x37);
        assert_eq!(windows_vk(ComboKey::F(1)), 0x70);
        assert_eq!(windows_vk(ComboKey::F(12)), 0x7B);
    }

    #[test]
    fn carbon_tables() {
        assert_eq!(carbon_modifiers(&combo(false, true, false, true, ComboKey::Letter('e'))), 0x0800 | 0x0100);
        assert_eq!(carbon_modifiers(&combo(true, false, true, false, ComboKey::Letter('e'))), 0x1000 | 0x0200);
        assert_eq!(carbon_keycode(ComboKey::Letter('e')), Some(0x0E));
        assert_eq!(carbon_keycode(ComboKey::Letter('a')), Some(0x00));
        assert_eq!(carbon_keycode(ComboKey::Digit('0')), Some(0x1D));
        assert_eq!(carbon_keycode(ComboKey::F(12)), Some(0x6F));
        assert_eq!(carbon_keycode(ComboKey::F(13)), None);
        assert_eq!(carbon_keycode(ComboKey::Letter('ç')), None);
        let mut codes: Vec<u32> = ('a'..='z').filter_map(|c| carbon_keycode(ComboKey::Letter(c))).collect();
        codes.extend(('0'..='9').filter_map(|c| carbon_keycode(ComboKey::Digit(c))));
        codes.extend((1..=12).filter_map(|n| carbon_keycode(ComboKey::F(n))));
        let count = codes.len();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!((count, codes.len()), (48, 48), "every key, each its own code");
    }

    #[test]
    fn x11_and_portal_tables() {
        let c = combo(false, false, true, true, ComboKey::Letter('e'));
        assert_eq!(x11_modifiers(&c), 1 | 64, "ShiftMask | Mod4Mask");
        assert_eq!(x11_modifiers(&combo(true, true, false, false, ComboKey::F(2))), 4 | 8, "ControlMask | Mod1Mask");
        assert_eq!(x11_keysym(ComboKey::Letter('e')), 0x65);
        assert_eq!(x11_keysym(ComboKey::Digit('3')), 0x33);
        assert_eq!(x11_keysym(ComboKey::F(1)), 0xFFBE);
        assert_eq!(x11_keysym(ComboKey::F(12)), 0xFFC9);
        assert_eq!(portal_trigger(&c), "SHIFT+LOGO+e");
        assert_eq!(portal_trigger(&combo(true, true, true, true, ComboKey::F(5))), "CTRL+ALT+SHIFT+LOGO+F5");
        assert_eq!(portal_trigger(&combo(true, false, false, false, ComboKey::Digit('1'))), "CTRL+1");
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
    use std::sync::{Arc, Mutex, mpsc};
    use std::time::{Duration, Instant};

    /// An odd chord nobody uses; held only for this test and released at its end. The only
    /// test that registers a real hotkey (no other test may use this chord: they would race).
    fn odd() -> Combo {
        Combo { ctrl: true, alt: true, shift: true, logo: false, key: ComboKey::F(11) }
    }

    fn ready(presses: Arc<AtomicUsize>) -> (Hotkey, Result<(), HotkeyError>) {
        let (tell, told) = mpsc::channel();
        let hotkey = register(
            odd(),
            Box::new(move || {
                presses.fetch_add(1, SeqCst);
            }),
            Box::new(move |result| {
                let _ = tell.send(result);
            }),
        );
        (hotkey, told.recv_timeout(Duration::from_secs(5)).expect("the hotkey thread did not answer"))
    }

    #[test]
    fn a_taken_hotkey_is_said_taken_and_a_press_is_heard() {
        let presses = Arc::new(AtomicUsize::new(0));
        let (first, result) = ready(presses.clone());
        assert_eq!(result, Ok(()), "Ctrl+Alt+Shift+F11 must be free on this machine");
        let (second, again) = ready(Arc::new(AtomicUsize::new(0)));
        assert_eq!(again, Err(HotkeyError::Taken));
        drop(second);
        // No real key press: WM_HOTKEY is put on the hotkey thread's queue.
        first.0.post_press();
        let until = Instant::now() + Duration::from_secs(2);
        while presses.load(SeqCst) == 0 && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(presses.load(SeqCst), 1);
        // Dropping joins the thread (a hang here fails the test) and releases the chord.
        drop(first);
        // Dropped from inside its own press: no deadlock, and the chord is released.
        let slot: Arc<Mutex<Option<Hotkey>>> = Arc::new(Mutex::new(None));
        let (gone, went) = mpsc::channel();
        let (tell, told) = mpsc::channel();
        let held = slot.clone();
        let third = register(
            odd(),
            Box::new(move || {
                drop(held.lock().ok().and_then(|mut hotkey| hotkey.take()));
                let _ = gone.send(());
            }),
            Box::new(move |result| {
                let _ = tell.send(result);
            }),
        );
        assert_eq!(told.recv_timeout(Duration::from_secs(5)).expect("no answer"), Ok(()), "dropped: released");
        *slot.lock().unwrap() = Some(third);
        slot.lock().unwrap().as_ref().unwrap().0.post_press();
        went.recv_timeout(Duration::from_secs(5)).expect("the self-drop hung");
        // Not joined: the thread unregisters a moment later.
        let until = Instant::now() + Duration::from_secs(2);
        loop {
            let (fourth, after) = ready(Arc::new(AtomicUsize::new(0)));
            if after == Ok(()) || Instant::now() > until {
                assert_eq!(after, Ok(()), "self-dropped: released");
                drop(fourth);
                break;
            }
        }
    }
}
