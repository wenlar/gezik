//! The tray icon (spec 9 §9.1): an icon in the notification area (Windows), the menu bar
//! (macOS) or a StatusNotifier host (Linux); a click or its menu shows Gezik. Started only
//! when the user turned it on; dropped, it goes and its thread ends. Each system's arm is in
//! `tray/`; the menu, its ids and labels are here, pure.

#[cfg_attr(all(unix, not(target_os = "macos")), path = "tray/linux.rs")]
#[cfg_attr(target_os = "macos", path = "tray/macos.rs")]
#[cfg_attr(windows, path = "tray/windows.rs")]
mod imp;

/// What the icon asks of Gezik.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent {
    /// A click: show the window if hidden, else hide it.
    Toggle,
    /// The menu's Show Gezik.
    Show,
    /// The menu's pinned folder `n` (0-based, of the first nine).
    Pin(usize),
    Quit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayError {
    /// Linux: no StatusNotifier host on this desktop.
    NoTray,
    Failed(String),
}

/// Don't drop the handle inside the callback; hop to the UI thread first (dropping it there
/// still ends the icon, but without waiting for its thread).
pub type OnEvent = Box<dyn Fn(TrayEvent) + Send + Sync>;
pub type OnReady = Box<dyn FnOnce(Result<(), TrayError>) + Send>;

/// The icon while it is held.
pub struct Tray(imp::Tray);

/// Puts the icon up; `on_ready` says whether it is there (later, from any thread); events
/// come to `on_event` from any thread. Never blocks.
pub fn start(pins: Vec<String>, on_event: OnEvent, on_ready: OnReady) -> Tray {
    Tray(imp::start(pins, on_event, on_ready))
}

impl Tray {
    /// The pinned folders' labels for the menu (the first nine are shown).
    pub fn set_pins(&self, pins: Vec<String>) {
        self.0.set_pins(pins);
    }
}

/// The menu's own ids (spec 13.3: native, apart from Gezik's menu ids).
pub const SHOW: u32 = 1;
pub const QUIT: u32 = 2;
pub const PIN_FIRST: u32 = 10;
pub const MAX_PINS: usize = 9;
/// The icon's tooltip.
pub const TIP: &str = "Gezik";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuLine {
    Item(u32, String),
    Separator,
}

/// Show Gezik, the first nine pins, Quit Gezik (spec 9.1).
pub fn menu(pins: &[String], windows: bool) -> Vec<MenuLine> {
    let mut lines = vec![MenuLine::Item(SHOW, "Show Gezik".into()), MenuLine::Separator];
    for (i, pin) in pins.iter().take(MAX_PINS).enumerate() {
        lines.push(MenuLine::Item(PIN_FIRST + i as u32, menu_label(pin, windows)));
    }
    if !pins.is_empty() {
        lines.push(MenuLine::Separator);
    }
    lines.push(MenuLine::Item(QUIT, "Quit Gezik".into()));
    lines
}

pub fn event_for(id: u32) -> Option<TrayEvent> {
    match id {
        SHOW => Some(TrayEvent::Show),
        QUIT => Some(TrayEvent::Quit),
        n if (PIN_FIRST..PIN_FIRST + MAX_PINS as u32).contains(&n) => Some(TrayEvent::Pin((n - PIN_FIRST) as usize)),
        _ => None,
    }
}

/// A pin's label as a menu item: control characters as spaces, at most 60 characters; on
/// Windows `&` doubled (it would underline the next letter).
pub fn menu_label(text: &str, windows: bool) -> String {
    let clean: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let mut out: String = clean.chars().take(60).collect();
    if clean.chars().count() > 60 {
        out.push('…');
    }
    if windows { out.replace('&', "&&") } else { out }
}

/// What a notify icon's callback (NOTIFYICON_VERSION_4: the event in lParam's low word) asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Click {
    Toggle,
    Menu,
}

pub fn windows_click(event: u32) -> Option<Click> {
    match event {
        // NIN_SELECT, NIN_KEYSELECT
        0x400 | 0x401 => Some(Click::Toggle),
        // WM_CONTEXTMENU
        0x7B => Some(Click::Menu),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menu_is_show_pins_quit() {
        let pins: Vec<String> = (1..=11).map(|i| format!("P{i}")).collect();
        let lines = menu(&pins, false);
        assert_eq!(lines[0], MenuLine::Item(SHOW, "Show Gezik".into()));
        assert_eq!(lines[1], MenuLine::Separator);
        assert_eq!(lines[2], MenuLine::Item(PIN_FIRST, "P1".into()));
        assert_eq!(lines[10], MenuLine::Item(PIN_FIRST + 8, "P9".into()), "the first nine pins only");
        assert_eq!(lines[11], MenuLine::Separator);
        assert_eq!(lines[12], MenuLine::Item(QUIT, "Quit Gezik".into()));
        assert_eq!(lines.len(), 13);
        assert_eq!(
            menu(&[], true),
            [MenuLine::Item(SHOW, "Show Gezik".into()), MenuLine::Separator, MenuLine::Item(QUIT, "Quit Gezik".into()),]
        );
    }

    #[test]
    fn ids_map_back_to_events() {
        assert_eq!(event_for(SHOW), Some(TrayEvent::Show));
        assert_eq!(event_for(QUIT), Some(TrayEvent::Quit));
        assert_eq!(event_for(PIN_FIRST), Some(TrayEvent::Pin(0)));
        assert_eq!(event_for(PIN_FIRST + 8), Some(TrayEvent::Pin(8)));
        for none in [0, 3, PIN_FIRST - 1, PIN_FIRST + 9, u32::MAX] {
            assert_eq!(event_for(none), None, "{none}");
        }
    }

    #[test]
    fn labels_are_escaped_and_cut() {
        assert_eq!(menu_label("Tom & Jerry", true), "Tom && Jerry", "& would underline the next letter");
        assert_eq!(menu_label("Tom & Jerry", false), "Tom & Jerry");
        assert_eq!(menu_label("a\tb\nc", false), "a b c");
        let long = "ç".repeat(70);
        assert_eq!(menu_label(&long, false), format!("{}…", "ç".repeat(60)));
    }

    #[test]
    fn windows_callbacks() {
        assert_eq!(windows_click(0x400), Some(Click::Toggle), "NIN_SELECT: a left click");
        assert_eq!(windows_click(0x401), Some(Click::Toggle), "NIN_KEYSELECT: Enter or Space on the icon");
        assert_eq!(windows_click(0x7B), Some(Click::Menu), "WM_CONTEXTMENU");
        assert_eq!(windows_click(0x200), None, "WM_MOUSEMOVE");
        assert_eq!(windows_click(0x202), None, "WM_LBUTTONUP comes too; NIN_SELECT is the click");
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// Puts a real icon in the notification area for two seconds (run by hand: `--ignored`).
    #[test]
    #[ignore]
    fn a_real_tray_icon_comes_and_goes() {
        let (tell, told) = std::sync::mpsc::channel();
        let tray = start(
            vec!["Downloads".into(), "Tom & Jerry".into()],
            Box::new(|event| eprintln!("tray: {event:?}")),
            Box::new(move |result| {
                let _ = tell.send(result);
            }),
        );
        assert_eq!(told.recv_timeout(std::time::Duration::from_secs(5)).unwrap(), Ok(()));
        std::thread::sleep(std::time::Duration::from_secs(2));
        drop(tray);

        // Dropped from inside its own event (a posted fake click): no deadlock.
        let slot: Arc<Mutex<Option<Tray>>> = Arc::new(Mutex::new(None));
        let (gone, went) = std::sync::mpsc::channel();
        let (tell, told) = std::sync::mpsc::channel();
        let held = slot.clone();
        let tray = start(
            Vec::new(),
            Box::new(move |_| {
                drop(held.lock().ok().and_then(|mut tray| tray.take()));
                let _ = gone.send(());
            }),
            Box::new(move |result| {
                let _ = tell.send(result);
            }),
        );
        assert_eq!(told.recv_timeout(std::time::Duration::from_secs(5)).unwrap(), Ok(()));
        *slot.lock().unwrap() = Some(tray);
        slot.lock().unwrap().as_ref().unwrap().0.post_click();
        went.recv_timeout(std::time::Duration::from_secs(5)).expect("the self-drop hung");
    }
}
