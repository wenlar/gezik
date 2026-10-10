//! Gezik while its window is closed or hidden (spec 9 §9.1–9.3, 9b9): the tray icon, the
//! global shortcut and `--background`. All off by default; while off, nothing of this runs
//! (no thread, no connection, no timer: only a few fields here). Only the first Gezik (the
//! single instance's owner) keeps them (deviation 11). Platform parts are gezik-platform's
//! `tray` and `hotkey`; their answers come from other threads and are brought here with
//! `slint::invoke_from_event_loop`.

use std::cell::RefCell;
use std::rc::Rc;

use gezik_config::settings::SystemSettings;
use gezik_config::settings_writer::{SettingsChange, SystemValue};
use gezik_config::shortcuts::{Action, Chord, Key, Platform, hotkey_text};
use gezik_config::store::ConfigStore;
use gezik_platform::hotkey::{self, Combo, ComboKey, HotkeyError};
use gezik_platform::tray::{self, TrayError, TrayEvent};
use slint::ComponentHandle;
use slint::winit_030::WinitWindowAccessor;

use crate::AppWindow;
use crate::system_changes::Os;

/// What a press of the global shortcut or a tray click does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press {
    Show,
    Hide,
    Minimize,
}

/// Spec 9.2 (deviation 9): a window in front goes (hidden with the tray, else minimized);
/// any other is shown and raised.
pub fn on_hotkey(visible: bool, minimized: bool, focused: bool, tray: bool) -> Press {
    match (visible && !minimized && focused, tray) {
        (true, true) => Press::Hide,
        (true, false) => Press::Minimize,
        (false, _) => Press::Show,
    }
}

/// A tray click (clicking the icon takes the focus anyway): by visibility.
pub fn on_click(visible: bool, minimized: bool) -> Press {
    if visible && !minimized { Press::Hide } else { Press::Show }
}

/// `--background` starts hidden only where the tray can bring the window back (spec 9.3).
pub fn start_hidden(background: bool, primary: bool, tray: bool) -> bool {
    background && primary && tray
}

/// How long a hidden start waits for its tray icon before it shows the window.
const TRAY_WAIT: std::time::Duration = std::time::Duration::from_secs(4);

/// A hidden start whose icon is not up by `TRAY_WAIT` (no answer at all: the tray's thread
/// never ran) shows the window: nothing else could bring it back.
fn tray_late(shown: bool, ready: bool) -> bool {
    !shown && !ready
}

/// The tray and the shortcut this Gezik should hold.
pub fn wanted(system: &SystemSettings, primary: bool) -> (bool, Option<Chord>) {
    if primary { (system.tray, system.hotkey) } else { (false, None) }
}

pub fn combo(chord: &Chord) -> Option<Combo> {
    let key = match chord.key {
        Key::Char(c) if c.is_ascii_lowercase() => ComboKey::Letter(c),
        Key::Char(c) if c.is_ascii_digit() => ComboKey::Digit(c),
        Key::F(n) if (1..=12).contains(&n) => ComboKey::F(n),
        _ => return None,
    };
    Some(Combo { ctrl: chord.ctrl, alt: chord.alt, shift: chord.shift, logo: chord.meta, key })
}

/// A shortcut as the rows and notes name it.
pub fn label(chord: &Chord, os: Os) -> String {
    match os {
        Os::Mac => crate::keys::chord_label(chord, Platform::Mac),
        Os::Windows => crate::keys::chord_label(chord, Platform::Other),
        Os::Linux => crate::keys::chord_label(chord, Platform::Other).replace("Win", "Super"),
    }
}

/// How the panel writes the shortcut to settings.toml (Task 1's `hotkey_text`).
pub fn setting_text(chord: &Chord, os: Os) -> String {
    hotkey_text(chord, logo_word(os))
}

fn logo_word(os: Os) -> &'static str {
    match os {
        Os::Windows => "win",
        Os::Mac => "cmd",
        Os::Linux => "super",
    }
}

/// The example in the question's text (spec 9.2's suggestions; never filled in: user decision).
pub fn example(os: Os) -> &'static str {
    match os {
        Os::Windows => "win+shift+e",
        Os::Mac => "cmd+alt+e",
        Os::Linux => "super+shift+e",
    }
}

/// The first close with the tray icon on (deviation 10): title, message, the quit button.
pub fn kept_text(os: Os) -> (&'static str, &'static str, &'static str) {
    match os {
        Os::Windows => (
            "Gezik keeps running in the notification area",
            "Closing the window keeps Gezik running; click its icon to show it again, or choose Quit Gezik in the icon's menu. The tray icon is turned off in System Integration.",
            "Quit Gezik",
        ),
        Os::Mac => (
            "Gezik keeps running in the menu bar",
            "Closing the window keeps Gezik running; show it again from its menu bar icon or the Dock, or choose Quit Gezik there. The icon is turned off in System Integration.",
            "Quit Gezik",
        ),
        Os::Linux => (
            "Gezik keeps running in the tray",
            "Closing the window keeps Gezik running; click its icon to show it again. To quit, choose Quit Gezik here, or turn the tray icon off in System Integration and close the window.",
            "Quit Gezik",
        ),
    }
}

/// The rows of System Integration (Task 8).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Status {
    pub primary: bool,
    /// `[system] tray` is on here.
    pub tray: bool,
    /// Linux: the last try found no StatusNotifier host.
    pub tray_missing: bool,
    /// The shortcut's label, if one is set.
    pub hotkey: Option<String>,
    /// Why the set shortcut is not working (taken, unsupported).
    pub hotkey_problem: Option<String>,
}

/// What main gives once its closures exist.
pub struct Hooks {
    /// Quits as closing the window did before 9b9 (the running-jobs question included).
    pub quit: Rc<dyn Fn()>,
    /// Runs an action as its key would (the tray menu's pins).
    pub run: Rc<dyn Fn(Action)>,
    /// The sidebar's pinned folders' labels.
    pub pins: Rc<dyn Fn() -> Vec<String>>,
}

#[derive(Default)]
struct State {
    window: Option<slint::Weak<AppWindow>>,
    store: Option<ConfigStore>,
    primary: bool,
    started: bool,
    /// The window has been shown at least once (a `--background` start has not, yet).
    shown: bool,
    when_shown: Vec<Box<dyn FnOnce()>>,
    told: bool,
    asking: bool,
    hooks: Option<Rc<Hooks>>,
    /// `[system]` as last resolved (or as the panel just set it); `wanted` filters it.
    system: SystemSettings,
    tray: Option<tray::Tray>,
    tray_ready: bool,
    tray_missing: bool,
    /// The last try failed: not tried again on every reload, only when asked again.
    tray_failed: bool,
    /// The panel turned it on just now: a failure turns the setting off again and says so.
    tray_asked: bool,
    hotkey: Option<(Chord, hotkey::Hotkey)>,
    hotkey_problem: Option<(Chord, String)>,
    hotkey_asked: Option<Chord>,
    /// Numbers each tray and shortcut started: a ready answer may come after its handle was
    /// dropped (and another started), and is then ignored.
    tray_number: u64,
    hotkey_number: u64,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::default();
}

fn with<T>(f: impl FnOnce(&mut State) -> T) -> T {
    STATE.with(|s| f(&mut s.borrow_mut()))
}

fn window() -> Option<AppWindow> {
    with(|s| s.window.clone()).and_then(|w| w.upgrade())
}

fn note(text: String) {
    crate::view::with_current(|v| v.note(text));
}

fn tell(title: &str, message: String, buttons: &[&str], answer: impl FnOnce(Option<usize>) + 'static) {
    crate::operations::with_current(|ops| ops.dialogs().ask(title, message, buttons, answer));
}

/// Early in main: the window, the config, whether this is the first Gezik and shown.
pub fn begin(window: &AppWindow, store: Option<ConfigStore>, primary: bool, shown: bool, told: bool) {
    with(|s| {
        s.window = Some(window.as_weak());
        s.store = store;
        s.primary = primary;
        s.shown = shown;
        s.told = told;
    });
}

pub fn set_hooks(hooks: Hooks) {
    with(|s| s.hooks = Some(Rc::new(hooks)));
}

/// End of main, the window and its parts up: what the settings asked for starts now.
pub fn start() {
    with(|s| s.started = true);
    sync();
    if !with(|s| s.shown) {
        slint::Timer::single_shot(TRAY_WAIT, || {
            if with(|s| tray_late(s.shown, s.tray_ready)) {
                note("The tray icon did not come up; the window is shown instead".to_owned());
                reveal();
            }
        });
    }
}

/// Every settings resolve (start, a reload, the panel's own write). The first comes before
/// `begin`: only kept until `start`.
pub fn apply(system: &SystemSettings) {
    with(|s| s.system = *system);
    sync();
}

/// Starts or stops what is wanted and not had, or had and not wanted.
fn sync() {
    let (started, (want_tray, want_hotkey), have_tray, have_hotkey, tray_failed, hotkey_failed) = with(|s| {
        if !wanted(&s.system, s.primary).0 {
            s.tray_failed = false;
        }
        (
            s.started,
            wanted(&s.system, s.primary),
            s.tray.is_some(),
            s.hotkey.as_ref().map(|(c, _)| *c),
            s.tray_failed,
            s.hotkey_problem.as_ref().map(|(c, _)| *c),
        )
    });
    if !started {
        return;
    }
    if want_tray && !have_tray && !tray_failed {
        start_tray();
    } else if !want_tray && have_tray {
        let tray = with(|s| {
            s.tray_ready = false;
            s.tray.take()
        });
        drop(tray);
        // Review focus 1: a hidden window never stays hidden without its icon.
        reveal_if_hidden();
    }
    // A shortcut that failed is tried again only once it is changed or asked for again.
    if want_hotkey != have_hotkey && want_hotkey != hotkey_failed {
        // The old one goes first (macOS keeps one handler per thread).
        let old = with(|s| s.hotkey.take());
        drop(old);
        with(|s| {
            if s.hotkey_problem.as_ref().is_some_and(|(c, _)| Some(*c) != want_hotkey) {
                s.hotkey_problem = None;
            }
        });
        if let Some(chord) = want_hotkey {
            start_hotkey(chord);
        }
    }
}

fn start_tray() {
    let pins = with(|s| s.hooks.clone()).map(|h| (h.pins)()).unwrap_or_default();
    let number = with(|s| {
        s.tray_number += 1;
        s.tray_number
    });
    let handle = tray::start(
        pins,
        Box::new(|event| {
            let _ = slint::invoke_from_event_loop(move || tray_event(event));
        }),
        Box::new(move |result| {
            let _ = slint::invoke_from_event_loop(move || tray_ready(number, result));
        }),
    );
    with(|s| s.tray = Some(handle));
}

fn tray_ready(number: u64, result: Result<(), TrayError>) {
    if with(|s| s.tray.is_none() || s.tray_number != number) {
        return;
    }
    let asked = with(|s| std::mem::take(&mut s.tray_asked));
    match result {
        Ok(()) => with(|s| {
            s.tray_ready = true;
            s.tray_missing = false;
        }),
        Err(why) => {
            let tray = with(|s| {
                s.tray_ready = false;
                s.tray_missing = why == TrayError::NoTray;
                s.tray_failed = true;
                if asked {
                    s.system.tray = false;
                }
                s.tray.take()
            });
            drop(tray);
            let text = match why {
                TrayError::NoTray => "No tray on this desktop (it needs a StatusNotifier host, such as KDE's panel or GNOME's AppIndicator extension).".to_owned(),
                TrayError::Failed(why) => format!("The tray icon could not be shown: {why}"),
            };
            if asked {
                write(SystemValue::Tray(false));
                tell("Tray icon", text, &["Close"], |_| {});
            } else {
                note(text);
            }
            reveal_if_hidden();
        }
    }
}

fn tray_event(event: TrayEvent) {
    let Some(window) = window() else { return };
    match event {
        TrayEvent::Toggle => match on_click(window.window().is_visible(), window.window().is_minimized()) {
            Press::Hide => hide(&window),
            _ => reveal(),
        },
        TrayEvent::Show => reveal(),
        TrayEvent::Pin(n) => {
            reveal();
            if let (Some(hooks), Some(action)) = (with(|s| s.hooks.clone()), Action::pin(n + 1)) {
                (hooks.run)(action);
            }
        }
        TrayEvent::Quit => {
            if let Some(hooks) = with(|s| s.hooks.clone()) {
                (hooks.quit)();
            }
        }
    }
}

fn start_hotkey(chord: Chord) {
    let Some(keys) = combo(&chord) else { return };
    let number = with(|s| {
        s.hotkey_number += 1;
        s.hotkey_number
    });
    let handle = hotkey::register(
        keys,
        Box::new(|| {
            let _ = slint::invoke_from_event_loop(pressed);
        }),
        Box::new(move |result| {
            let _ = slint::invoke_from_event_loop(move || hotkey_ready(number, chord, result));
        }),
    );
    with(|s| s.hotkey = Some((chord, handle)));
}

fn hotkey_ready(number: u64, chord: Chord, result: Result<(), HotkeyError>) {
    // An answer for a shortcut turned off, changed or started again meanwhile is stale.
    if with(|s| s.hotkey.as_ref().map(|(c, _)| *c) != Some(chord) || s.hotkey_number != number) {
        return;
    }
    let asked = with(|s| {
        let asked = s.hotkey_asked == Some(chord);
        if asked {
            s.hotkey_asked = None;
        }
        asked
    });
    let name = label(&chord, Os::HERE);
    let why = match result {
        Ok(()) => {
            with(|s| s.hotkey_problem = None);
            if asked {
                note(format!("{name} shows or hides Gezik now"));
            }
            return;
        }
        Err(HotkeyError::Taken) => format!("{name} is used by another app."),
        Err(HotkeyError::Unsupported) => "This desktop does not support global shortcuts.".to_owned(),
        Err(HotkeyError::Failed(why)) => format!("{name} could not be registered: {why}"),
    };
    let held = with(|s| {
        s.hotkey_problem = Some((chord, why.clone()));
        s.hotkey.take()
    });
    drop(held);
    if asked {
        // Deviation 5: from the panel, the setting goes back off and the user is told.
        with(|s| s.system.hotkey = None);
        write(SystemValue::Hotkey(String::new()));
        refused(why);
    } else {
        note(format!("Global shortcut: {why} Change it in System Integration."));
    }
}

/// The panel's shortcut could not be registered (deviation 5): Choose Another asks again,
/// the field empty and the reason above it.
fn refused(why: String) {
    tell("Global shortcut", why.clone(), &["Choose Another", "Close"], move |choice| {
        if choice == Some(0) {
            crate::integration::ask_hotkey(Some((String::new(), why)));
        }
    });
}

fn pressed() {
    let Some(window) = window() else { return };
    let focused = window.window().with_winit_window(|native| native.has_focus()).unwrap_or(false);
    let tray = with(|s| s.tray.is_some() && s.tray_ready);
    match on_hotkey(window.window().is_visible(), window.window().is_minimized(), focused, tray) {
        Press::Hide => hide(&window),
        Press::Minimize => window.window().set_minimized(true),
        Press::Show => reveal(),
    }
}

/// Runs `f` once the window has been shown (now, unless a `--background` start hid it):
/// work that needs the native window waits for it (deviation 12).
pub fn when_shown(f: impl FnOnce() + 'static) {
    let shown = with(|s| s.shown);
    if shown {
        f();
    } else {
        with(|s| s.when_shown.push(Box::new(f)));
    }
}

/// Shows the window (hidden or minimized) and brings it to the front.
pub fn reveal() {
    let Some(window) = window() else { return };
    let _ = window.show();
    let waiting = with(|s| {
        s.shown = true;
        std::mem::take(&mut s.when_shown)
    });
    for f in waiting {
        f();
    }
    crate::single_instance::raise(&window);
}

fn reveal_if_hidden() {
    if window().is_some_and(|w| !w.window().is_visible()) {
        reveal();
    }
}

fn hide(window: &AppWindow) {
    // Slint destroys the native window on such a hide; the drop target keeps its surface.
    if gezik_platform::dnd::hide_destroys(&window.window().window_handle()) {
        window.window().set_minimized(true);
    } else {
        let _ = window.hide();
    }
}

/// The window's close button and the last tab's close: with the tray icon up, Gezik keeps
/// running hidden (the first time, after saying so); `false`: quit as before.
pub fn close_requested() -> bool {
    let (keeps, told, asking) = with(|s| (s.primary && s.tray.is_some() && s.tray_ready, s.told, s.asking));
    if !keeps {
        return false;
    }
    if asking {
        return true;
    }
    let Some(window) = window() else { return false };
    if told {
        hide(&window);
        return true;
    }
    with(|s| s.asking = true);
    let (title, message, quit) = kept_text(Os::HERE);
    tell(title, message.to_owned(), &["OK", quit], |choice| {
        with(|s| s.asking = false);
        match choice {
            Some(0) => {
                with(|s| s.told = true);
                if let Some(store) = with(|s| s.store.clone()) {
                    store.update_state(|state| state.tray_told = true);
                }
                if let Some(window) = crate::resident::window() {
                    hide(&window);
                }
            }
            Some(1) => {
                if let Some(hooks) = with(|s| s.hooks.clone()) {
                    (hooks.quit)();
                }
            }
            _ => {}
        }
    });
    true
}

/// The pins changed (a settings resolve): the tray menu follows.
pub fn refresh_pins() {
    let pins = with(|s| s.hooks.clone()).map(|h| (h.pins)());
    if let Some(pins) = pins {
        with(|s| {
            if let Some(tray) = &s.tray {
                tray.set_pins(pins);
            }
        });
    }
}

pub fn status() -> Status {
    with(|s| Status {
        primary: s.primary,
        tray: s.system.tray,
        tray_missing: s.tray_missing,
        hotkey: s.system.hotkey.map(|c| label(&c, Os::HERE)),
        hotkey_problem: s
            .hotkey_problem
            .as_ref()
            .filter(|(c, _)| s.system.hotkey == Some(*c))
            .map(|(_, why)| why.clone()),
    })
}

/// Writes a `[system]` value (the panel's rows); no config folder: it lasts until Gezik quits.
fn write(value: SystemValue) {
    match with(|s| s.store.clone()) {
        Some(store) => store.write_settings(SettingsChange::System(value), |result| {
            if let Err(warning) = result {
                let _ = slint::invoke_from_event_loop(move || note(warning.to_string()));
            }
        }),
        None => note("No config folder: this lasts until Gezik quits".to_owned()),
    }
}

// shortcut: a settings reload read before the panel's write lands may undo the change for a
// moment (the next reload, after the write, puts it back); fix if a screen test shows it.
pub fn turn_on_tray() {
    // On but missing or failed (Linux without a StatusNotifier host): Turn on tries again.
    if with(|s| s.system.tray && !s.tray_missing && !s.tray_failed) {
        return note("The tray icon is on already".to_owned());
    }
    with(|s| {
        s.system.tray = true;
        s.tray_asked = true;
        s.tray_failed = false;
        s.tray_missing = false;
    });
    write(SystemValue::Tray(true));
    sync();
}

pub fn turn_off_tray() {
    with(|s| s.system.tray = false);
    write(SystemValue::Tray(false));
    sync();
}

/// The panel's question gave `chord` (already checked by `parse_hotkey`).
pub fn set_hotkey(chord: Chord) {
    with(|s| {
        s.system.hotkey = Some(chord);
        s.hotkey_asked = Some(chord);
        s.hotkey_problem = None;
    });
    write(SystemValue::Hotkey(setting_text(&chord, Os::HERE)));
    sync();
}

pub fn turn_off_hotkey() {
    with(|s| {
        s.system.hotkey = None;
        s.hotkey_problem = None;
    });
    write(SystemValue::Hotkey(String::new()));
    sync();
}

/// After the event loop: the icon and the shortcut are let go explicitly (a notification icon
/// not deleted stays on Windows until the pointer passes over it).
pub fn shutdown() {
    let (tray, hotkey) = with(|s| (s.tray.take(), s.hotkey.take()));
    drop(tray);
    drop(hotkey);
}

#[cfg(test)]
mod tests {
    use super::*;
    use gezik_config::shortcuts::{Key, Platform, parse_hotkey};

    #[test]
    fn turning_on_a_missing_tray_tries_again() {
        with(|s| {
            s.system.tray = true;
            s.tray_missing = true;
            s.tray_failed = true;
        });
        turn_on_tray();
        let (failed, missing, asked) = with(|s| (s.tray_failed, s.tray_missing, s.tray_asked));
        assert!(!failed && !missing && asked, "sync may start it again");
        with(|s| *s = State::default());
    }

    fn chord(text: &str) -> Chord {
        parse_hotkey(text, Platform::Other).unwrap().unwrap()
    }

    #[test]
    fn the_hotkey_hides_only_a_window_in_front() {
        assert_eq!(on_hotkey(true, false, true, true), Press::Hide, "in front, tray on: hidden");
        assert_eq!(on_hotkey(true, false, true, false), Press::Minimize, "in front, no tray: minimized");
        assert_eq!(on_hotkey(true, false, false, true), Press::Show, "behind another window: raised");
        assert_eq!(on_hotkey(true, true, false, false), Press::Show, "minimized: restored");
        assert_eq!(on_hotkey(false, false, false, true), Press::Show, "hidden: shown");
    }

    #[test]
    fn a_click_toggles_by_visibility() {
        assert_eq!(on_click(true, false), Press::Hide);
        assert_eq!(on_click(true, true), Press::Show);
        assert_eq!(on_click(false, false), Press::Show);
    }

    #[test]
    fn a_hidden_start_without_its_icon_shows_the_window() {
        assert!(tray_late(false, false), "still hidden, no icon: shown");
        assert!(!tray_late(false, true), "the icon is up: stays hidden");
        assert!(!tray_late(true, false), "shown already: nothing to do");
    }

    #[test]
    fn start_hidden_only_with_a_tray() {
        assert!(start_hidden(true, true, true));
        assert!(!start_hidden(true, true, false), "no tray: nothing would bring the window back");
        assert!(!start_hidden(false, true, true), "a plain start shows the window");
        assert!(!start_hidden(true, false, true), "a second window keeps no tray");
    }

    #[test]
    fn nothing_runs_when_off_and_a_second_window_keeps_nothing() {
        let off = SystemSettings::default();
        assert_eq!(wanted(&off, true), (false, None), "the defaults: no tray, no shortcut");
        let on = SystemSettings { tray: true, hotkey: Some(chord("win+shift+e")), ..SystemSettings::default() };
        assert_eq!(wanted(&on, true), (true, Some(chord("win+shift+e"))));
        assert_eq!(wanted(&on, false), (false, None), "only the first Gezik keeps them");
    }

    #[test]
    fn chords_become_combos() {
        let c = combo(&chord("win+shift+e")).unwrap();
        assert_eq!(c, Combo { ctrl: false, alt: false, shift: true, logo: true, key: ComboKey::Letter('e') });
        assert_eq!(combo(&chord("ctrl+shift+7")).unwrap().key, ComboKey::Digit('7'));
        assert_eq!(combo(&chord("alt+shift+f4")).unwrap().key, ComboKey::F(4));
        let space = Chord { ctrl: true, alt: false, shift: false, meta: false, key: Key::Space };
        assert_eq!(combo(&space), None, "parse_hotkey never makes one; refused all the same");
    }

    #[test]
    fn labels_and_setting_texts_follow_the_system() {
        let c = chord("win+shift+e");
        assert_eq!(label(&c, Os::Windows), "Shift+Win+E");
        assert_eq!(label(&c, Os::Linux), "Shift+Super+E");
        assert_eq!(label(&c, Os::Mac), "⇧⌘E");
        assert_eq!(setting_text(&c, Os::Windows), "win+shift+e");
        assert_eq!(setting_text(&c, Os::Mac), "cmd+shift+e");
        assert_eq!(setting_text(&c, Os::Linux), "super+shift+e");
        for os in [Os::Windows, Os::Mac, Os::Linux] {
            let example = example(os);
            let platform = if os == Os::Mac { Platform::Mac } else { Platform::Other };
            assert!(parse_hotkey(example, platform).unwrap().is_some(), "{example} is a valid example");
        }
    }

    #[test]
    fn the_first_close_says_where_gezik_went() {
        let (title, message, quit) = kept_text(Os::Windows);
        assert_eq!(title, "Gezik keeps running in the notification area");
        assert!(message.contains("Quit Gezik"), "{message}");
        assert_eq!(quit, "Quit Gezik");
        assert_eq!(kept_text(Os::Mac).0, "Gezik keeps running in the menu bar");
        assert!(kept_text(Os::Linux).1.contains("turn the tray icon off"), "Linux has no tray menu");
    }
}
