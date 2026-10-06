//! Keyboard: Slint key events → shortcut chords, list navigation keys, type-ahead.

use std::cell::RefCell;
use std::time::{Duration, Instant};

use gezik_config::shortcuts::{Action, Chord, Key, Platform, Shortcuts};
use slint::platform::Key as SlintKey;

thread_local! {
    /// The shortcuts in effect on this (UI) thread; replaced whenever the settings change.
    static SHORTCUTS: RefCell<Shortcuts> = RefCell::new(Shortcuts::default());
}

/// Makes `shortcuts` the ones in effect (after every settings resolve).
pub fn set_shortcuts(shortcuts: Shortcuts) {
    SHORTCUTS.with(|s| *s.borrow_mut() = shortcuts);
}

/// The action bound to `chord` in the shortcuts in effect.
pub fn action_for(chord: &Chord) -> Option<Action> {
    SHORTCUTS.with(|s| s.borrow().action_for(chord))
}

/// Slint's named keys and the chord keys they are (Backtab is Shift+Tab).
const NAMED: [(SlintKey, Key); 27] = [
    (SlintKey::LeftArrow, Key::Left),
    (SlintKey::RightArrow, Key::Right),
    (SlintKey::UpArrow, Key::Up),
    (SlintKey::DownArrow, Key::Down),
    (SlintKey::Return, Key::Enter),
    (SlintKey::Tab, Key::Tab),
    (SlintKey::Backtab, Key::Tab),
    (SlintKey::Backspace, Key::Backspace),
    (SlintKey::Delete, Key::Delete),
    (SlintKey::Home, Key::Home),
    (SlintKey::End, Key::End),
    (SlintKey::PageUp, Key::PageUp),
    (SlintKey::PageDown, Key::PageDown),
    (SlintKey::Escape, Key::Escape),
    (SlintKey::Space, Key::Space),
    (SlintKey::F1, Key::F(1)),
    (SlintKey::F2, Key::F(2)),
    (SlintKey::F3, Key::F(3)),
    (SlintKey::F4, Key::F(4)),
    (SlintKey::F5, Key::F(5)),
    (SlintKey::F6, Key::F(6)),
    (SlintKey::F7, Key::F(7)),
    (SlintKey::F8, Key::F(8)),
    (SlintKey::F9, Key::F(9)),
    (SlintKey::F10, Key::F(10)),
    (SlintKey::F11, Key::F(11)),
    (SlintKey::F12, Key::F(12)),
];

/// Converts a Slint key event's text and modifiers into a chord, if the key is one we know.
pub fn chord_from_event(text: &str, ctrl: bool, alt: bool, shift: bool, meta: bool) -> Option<Chord> {
    let mut chars = text.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else { return None };
    let key = if let Some((_, key)) = NAMED.iter().find(|(k, _)| char::from(*k) == c) {
        *key
    } else if c.is_ascii_alphanumeric() || matches!(c, '[' | ']' | '.' | '=' | '-') {
        Key::Char(c.to_ascii_lowercase())
    } else {
        return None;
    };
    // Backtab is Shift+Tab even if the platform did not report shift.
    let shift = shift || c == char::from(SlintKey::Backtab);
    Some(Chord { ctrl, alt, shift, meta, key })
}

/// Converts a Slint key event into a chord. Slint reports the macOS Command key as
/// `control` and the physical Control key as `meta`; chords (like `parse_chord`) mean the
/// physical keys, so on macOS the two are swapped back.
pub fn chord_from_slint(
    text: &str,
    control: bool,
    alt: bool,
    shift: bool,
    meta: bool,
    platform: Platform,
) -> Option<Chord> {
    match platform {
        Platform::Mac => chord_from_event(text, meta, alt, shift, control),
        Platform::Other => chord_from_event(text, control, alt, shift, meta),
    }
}

/// What winit says the key being pressed is, where Slint's text cannot tell: the keypad's
/// + - * / type what the main keys type, and Ctrl+Shift+1 arrives as Ctrl+Shift+!.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Physical {
    Numpad(char),
    Digit(char),
    Other,
}

thread_local! {
    /// The physical key of the press Slint is about to hand to `key-event`: set by the winit
    /// hook (main.rs), which runs first, and taken by the key handler.
    static PRESSED: std::cell::Cell<Physical> = const { std::cell::Cell::new(Physical::Other) };
}

pub fn note_pressed(physical: Physical) {
    PRESSED.with(|p| p.set(physical));
}

/// The press's physical key, once (a key Slint makes up itself finds `Other`).
pub fn take_pressed() -> Physical {
    PRESSED.with(|p| p.replace(Physical::Other))
}

pub fn physical_of(code: slint::winit_030::winit::keyboard::KeyCode) -> Physical {
    use slint::winit_030::winit::keyboard::KeyCode as K;
    match code {
        K::NumpadAdd => Physical::Numpad('+'),
        K::NumpadSubtract => Physical::Numpad('-'),
        K::NumpadMultiply => Physical::Numpad('*'),
        K::NumpadDivide => Physical::Numpad('/'),
        K::Digit0 => Physical::Digit('0'),
        K::Digit1 => Physical::Digit('1'),
        K::Digit2 => Physical::Digit('2'),
        K::Digit3 => Physical::Digit('3'),
        K::Digit4 => Physical::Digit('4'),
        K::Digit5 => Physical::Digit('5'),
        K::Digit6 => Physical::Digit('6'),
        K::Digit7 => Physical::Digit('7'),
        K::Digit8 => Physical::Digit('8'),
        K::Digit9 => Physical::Digit('9'),
        _ => Physical::Other,
    }
}

/// The chord of a key press (see `chord_from_slint`), with what winit said the key was: the
/// keypad's + - * / are `Key::Num`; Ctrl (⌘) + Shift on a digit key is that digit, whatever
/// Shift types there (`!`, `'`). Not with Alt: AltGr (Ctrl+Alt on Windows) types characters.
/// Only a press that types something: a modifier key (one `menu_bar` plays after the menu bar
/// took a digit's press) goes by its text.
pub fn chord_from_press(
    text: &str,
    physical: Physical,
    control: bool,
    alt: bool,
    shift: bool,
    meta: bool,
    platform: Platform,
) -> Option<Chord> {
    // Slint's `control` is ⌘ on macOS (see `chord_from_slint`).
    let (ctrl, cmd) = match platform {
        Platform::Mac => (meta, control),
        Platform::Other => (control, meta),
    };
    match physical {
        Physical::Numpad(c) if typed_char(text) == Some(c) => {
            Some(Chord { ctrl, alt, shift, meta: cmd, key: Key::Num(c) })
        }
        Physical::Digit(d) if (ctrl || cmd) && shift && !alt && typed_char(text).is_some() => {
            Some(Chord { ctrl, alt, shift, meta: cmd, key: Key::Char(d) })
        }
        _ => chord_from_slint(text, control, alt, shift, meta, platform),
    }
}

/// The chord bound to `action` now, if any.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn chord_for(action: Action) -> Option<Chord> {
    SHORTCUTS.with(|s| s.borrow().chord_for(action))
}

/// The Slint key presses that make `chord` on `platform`: the modifier keys to hold (as Slint
/// names them: ⌘ is `Control` on macOS) and the key's text, as the window would get them.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn slint_keys(chord: &Chord, platform: Platform) -> (Vec<SlintKey>, String) {
    let (command, control) = match platform {
        Platform::Mac => (chord.meta, chord.ctrl),
        Platform::Other => (chord.ctrl, chord.meta),
    };
    let mut modifiers = Vec::new();
    for (held, key) in [
        (command, SlintKey::Control),
        (control, SlintKey::Meta),
        (chord.alt, SlintKey::Alt),
        (chord.shift, SlintKey::Shift),
    ] {
        if held {
            modifiers.push(key);
        }
    }
    let text = match chord.key {
        // A letter typed with Shift comes upper case.
        Key::Char(c) if chord.shift => c.to_ascii_uppercase().to_string(),
        Key::Char(c) | Key::Num(c) => c.to_string(),
        key => NAMED.iter().find(|(_, k)| *k == key).map(|(k, _)| char::from(*k).to_string()).unwrap_or_default(),
    };
    (modifiers, text)
}

/// The text a key press with ⌘ (`command`) is matched by. On macOS a ⌘ shortcut on a bracket
/// goes by the key's place, as the system's own do: on a layout where that key types another
/// letter (`ğ` on Turkish-QWERTY-PC), ⌘[ and ⌘] still work. ⌘⇧. goes by the key that types
/// `.`, whatever Shift makes of it. Elsewhere, and without ⌘, the text stays as typed.
pub fn shortcut_text(text: &str, command: bool) -> std::borrow::Cow<'_, str> {
    // Only a key that types something: a modifier or named key pressed meanwhile (one
    // `menu_bar` plays, say) is not the key the system is handling now.
    #[cfg(target_os = "macos")]
    if command && typed_char(text).is_some() {
        if let Some(bracket) = gezik_platform::key_place::bracket_of_key_being_pressed() {
            return bracket_text(text, bracket);
        }
        // ⌘⇧. types `>` (US) or `:` (Turkish): the shortcut is named by the key's own `.`.
        if gezik_platform::key_place::unshifted_text_of_key_being_pressed().as_deref() == Some(".") {
            return std::borrow::Cow::Borrowed(".");
        }
    }
    let _ = command;
    std::borrow::Cow::Borrowed(text)
}

/// The text of a press of the bracket key `bracket`: the bracket, whatever the layout typed.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn bracket_text(text: &str, bracket: char) -> std::borrow::Cow<'_, str> {
    if text.chars().eq([bracket]) {
        std::borrow::Cow::Borrowed(text)
    } else {
        std::borrow::Cow::Owned(bracket.to_string())
    }
}

/// Whether `chord` is a text editing shortcut (select all, copy, paste, cut, undo, redo
/// with the platform's primary modifier: Cmd on macOS, Ctrl elsewhere), which belongs to
/// a focused text box.
pub fn is_text_edit(chord: &Chord, platform: Platform) -> bool {
    let primary = match platform {
        Platform::Mac => chord.meta && !chord.ctrl,
        Platform::Other => chord.ctrl && !chord.meta,
    };
    primary && !chord.alt && matches!(chord.key, Key::Char('a' | 'c' | 'v' | 'x' | 'z' | 'y'))
}

/// Whether `chord` holds the platform's primary modifier (Cmd on macOS, Ctrl elsewhere),
/// the one Explorer and Finder use for multiple selection.
pub fn is_primary(chord: &Chord, platform: Platform) -> bool {
    match platform {
        Platform::Mac => chord.meta,
        Platform::Other => chord.ctrl,
    }
}

/// The character an unmodified key press types, for type-ahead: any printable character
/// (also non-ASCII letters such as `ş`), but not control or named keys (arrows, F-keys…),
/// which Slint sends as control or private-use characters.
pub fn typed_char(text: &str) -> Option<char> {
    let mut chars = text.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else { return None };
    let private_use = ('\u{E000}'..='\u{F8FF}').contains(&c);
    (!c.is_control() && !private_use).then_some(c)
}

/// Whether a key press asks for the context menu of the selection: the Menu key, or
/// Shift+F10 (as in Explorer). Ctrl, Alt or Meta make it something else.
pub fn is_context_menu_key(text: &str, ctrl: bool, alt: bool, shift: bool, meta: bool) -> bool {
    if ctrl || alt || meta {
        return false;
    }
    let mut chars = text.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else { return false };
    c == char::from(SlintKey::Menu) || (shift && c == char::from(SlintKey::F10))
}

/// Whether the list's scroll offset `now` is `target` (just set to bring a row into view)
/// moved to a row boundary. A jump of more than about a screen makes Slint's ListView
/// place the first visible row at the top edge, dropping the part of a row that
/// `target` scrolled past, so a row brought in at the bottom ends up partly hidden.
/// Any other difference (a row or more) is the user scrolling on: leave it alone.
pub fn scroll_was_snapped(now: f32, target: f32, row_height: f32) -> bool {
    now != target && (now - target).abs() < row_height && (now / row_height).fract().abs() < 0.001
}

/// Finds the next entry whose name starts with what the user typed within the last second.
pub struct TypeAhead {
    typed: String,
    last: Option<Instant>,
}

impl TypeAhead {
    pub const WINDOW: Duration = Duration::from_secs(1);

    pub fn new() -> Self {
        TypeAhead { typed: String::new(), last: None }
    }

    /// Whether a name is being typed: a key came within the last second. Space then
    /// belongs to the name, not to quick look.
    pub fn is_active(&self, now: Instant) -> bool {
        !self.typed.is_empty() && self.last.is_some_and(|last| now.saturating_duration_since(last) <= Self::WINDOW)
    }

    /// Adds `c` (typed at `now`) and returns what `find` returns for the typed text, in
    /// lowercase: the index of the first name starting with it (see
    /// [`starts_with_lowercase`]).
    pub fn type_char(&mut self, c: char, now: Instant, find: impl FnOnce(&str) -> Option<usize>) -> Option<usize> {
        if self.last.is_none_or(|last| now.saturating_duration_since(last) > Self::WINDOW) {
            self.typed.clear();
        }
        self.last = Some(now);
        self.typed.extend(c.to_lowercase());
        find(&self.typed)
    }
}

/// Whether `name` starts with `typed` (already lowercase), ignoring case. Allocates
/// nothing, as it runs for every row of a folder on each typed character.
pub fn starts_with_lowercase(name: &str, typed: &str) -> bool {
    let mut name = name.chars().flat_map(char::to_lowercase);
    typed.chars().all(|t| name.next() == Some(t))
}

impl Default for TypeAhead {
    fn default() -> Self {
        TypeAhead::new()
    }
}

/// File operation shortcuts: never while typing in the address bar.
pub fn acts_on_files(action: Action) -> bool {
    matches!(
        action,
        Action::Copy
            | Action::Cut
            | Action::Paste
            | Action::PasteMove
            | Action::Trash
            | Action::DeletePermanently
            | Action::Rename
            | Action::NewFolder
            | Action::Duplicate
            | Action::Undo
            | Action::Redo
            | Action::BatchRename
    )
}

/// Those that act on the selection: only while the file list has the keyboard.
pub fn needs_list(action: Action) -> bool {
    matches!(
        action,
        Action::Copy
            | Action::Cut
            | Action::Trash
            | Action::DeletePermanently
            | Action::Rename
            | Action::Duplicate
            | Action::BatchRename
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_shortcuts_need_the_list_only_for_the_selection() {
        assert!(acts_on_files(Action::Paste) && !needs_list(Action::Paste));
        assert!(needs_list(Action::Trash) && needs_list(Action::Copy));
        assert!(!acts_on_files(Action::Refresh));
    }

    #[test]
    fn menu_key_and_shift_f10_open_the_context_menu() {
        let (menu, f10) = (text(SlintKey::Menu), text(SlintKey::F10));
        assert!(is_context_menu_key(&menu, false, false, false, false));
        assert!(is_context_menu_key(&menu, false, false, true, false));
        assert!(is_context_menu_key(&f10, false, false, true, false));
        assert!(!is_context_menu_key(&f10, false, false, false, false), "plain F10");
        assert!(!is_context_menu_key(&f10, true, false, true, false), "Ctrl+Shift+F10");
        assert!(!is_context_menu_key(&menu, false, true, false, false), "Alt+Menu");
        assert!(!is_context_menu_key("a", false, false, true, false));
    }

    #[test]
    fn a_far_jump_snapped_to_a_row_boundary_is_detected() {
        // End on 60 rows of 26 px in a 654 px view: the target is -906, Slint shows -884.
        assert!(scroll_was_snapped(-884.0, -906.0, 26.0));
        assert!(!scroll_was_snapped(-906.0, -906.0, 26.0), "already right");
        assert!(!scroll_was_snapped(-858.0, -906.0, 26.0), "scrolled on by the user");
        assert!(!scroll_was_snapped(-900.0, -906.0, 26.0), "not on a row boundary");
    }

    fn text(key: SlintKey) -> String {
        char::from(key).to_string()
    }

    #[test]
    fn maps_letters_and_named_keys() {
        let c = chord_from_event("T", true, false, true, false).unwrap();
        assert_eq!(c, Chord { ctrl: true, alt: false, shift: true, meta: false, key: Key::Char('t') });
        assert_eq!(chord_from_event(&text(SlintKey::LeftArrow), false, true, false, false).unwrap().key, Key::Left);
        assert_eq!(chord_from_event(&text(SlintKey::F5), false, false, false, false).unwrap().key, Key::F(5));
        assert_eq!(chord_from_event("\t", true, false, false, false).unwrap().key, Key::Tab);
        assert_eq!(chord_from_event("[", true, false, false, false).unwrap().key, Key::Char('['));
        assert!(chord_from_event("é", false, false, false, false).is_none());
        assert!(chord_from_event("", false, false, false, false).is_none());
    }

    /// What `slint_keys` plays comes back as the same chord, for every default.
    #[test]
    fn played_keys_are_the_chord() {
        for platform in [Platform::Mac, Platform::Other] {
            let defaults = Shortcuts::defaults(platform);
            for action in Action::ALL {
                let Some(chord) = defaults.chord_for(action) else { continue };
                // The keypad's: the menu bar runs those itself (actions::run).
                if matches!(chord.key, Key::Num(_)) {
                    continue;
                }
                let (modifiers, text) = slint_keys(&chord, platform);
                let held = |k: SlintKey| modifiers.contains(&k);
                let got = chord_from_slint(
                    &text,
                    held(SlintKey::Control),
                    held(SlintKey::Alt),
                    held(SlintKey::Shift),
                    held(SlintKey::Meta),
                    platform,
                );
                assert_eq!(got, Some(chord), "{platform:?} {}", action.name());
            }
        }
    }

    #[test]
    fn backtab_is_shift_tab() {
        let c = chord_from_event(&text(SlintKey::Backtab), true, false, false, false).unwrap();
        assert_eq!(c, Chord { ctrl: true, alt: false, shift: true, meta: false, key: Key::Tab });
    }

    /// What the Windows/Linux backend sends for `chord`, with what winit says the key is:
    /// Ctrl as `control`, Win as `meta`; Shift+Tab arrives as Tab with shift; Shift on a digit
    /// types what it types on a US layout; the keypad's keys type their character.
    fn other_event(chord: &Chord) -> (String, Physical, bool, bool, bool, bool) {
        let (text, physical) = match chord.key {
            Key::Num(c) => (c.to_string(), Physical::Numpad(c)),
            Key::Char(d) if d.is_ascii_digit() && chord.shift => {
                let shifted = ")!@#$%^&*(".chars().nth(d.to_digit(10).unwrap() as usize).unwrap();
                (shifted.to_string(), Physical::Digit(d))
            }
            Key::Char(d) if d.is_ascii_digit() => (d.to_string(), Physical::Digit(d)),
            Key::Char(c) => (c.to_string(), Physical::Other),
            Key::F(n) => (
                text(match n {
                    2 => SlintKey::F2,
                    5 => SlintKey::F5,
                    other => panic!("no default uses f{other}"),
                }),
                Physical::Other,
            ),
            Key::Delete => (text(SlintKey::Delete), Physical::Other),
            Key::Left => (text(SlintKey::LeftArrow), Physical::Other),
            Key::Right => (text(SlintKey::RightArrow), Physical::Other),
            Key::Up => (text(SlintKey::UpArrow), Physical::Other),
            Key::Tab => ("\t".to_owned(), Physical::Other),
            Key::Space => (" ".to_owned(), Physical::Other),
            other => panic!("no default uses {other:?}"),
        };
        (text, physical, chord.ctrl, chord.alt, chord.shift, chord.meta)
    }

    #[test]
    fn every_default_is_reachable_on_windows_and_linux() {
        use gezik_config::shortcuts::parse_chord;
        let defaults = Shortcuts::defaults(Platform::Other);
        let reach = |text: &str| {
            let chord = parse_chord(text, Platform::Other).unwrap().unwrap();
            let (t, physical, control, alt, shift, meta) = other_event(&chord);
            let got = chord_from_press(&t, physical, control, alt, shift, meta, Platform::Other).unwrap();
            defaults.action_for(&got)
        };
        for action in Action::ALL {
            let text = match action {
                Action::NewTab => "ctrl+t",
                Action::CloseTab => "ctrl+w",
                Action::NextTab => "ctrl+tab",
                Action::PrevTab => "ctrl+shift+tab",
                Action::Back => "alt+left",
                Action::Forward => "alt+right",
                Action::Up => "alt+up",
                Action::FocusPath => "ctrl+l",
                Action::Refresh => "f5",
                Action::SelectAll => "ctrl+a",
                Action::ViewList => "ctrl+shift+1",
                Action::ViewGrid => "ctrl+shift+2",
                Action::TogglePreview => "alt+p",
                Action::QuickLook => "space",
                Action::Copy => "ctrl+c",
                Action::Cut => "ctrl+x",
                Action::Paste => "ctrl+v",
                Action::Trash => "delete",
                Action::DeletePermanently => "shift+delete",
                Action::Rename => "f2",
                Action::NewFolder => "ctrl+shift+n",
                Action::Undo => "ctrl+z",
                Action::Redo => "ctrl+y",
                Action::ToggleHidden => "ctrl+h",
                Action::Filter => "ctrl+f",
                Action::InvertSelection => "ctrl+shift+i",
                Action::SelectPattern => "ctrl+=",
                Action::DeselectPattern => "ctrl+-",
                Action::SelectSameType => "alt+num+",
                Action::RestoreSelection => "num/",
                Action::Tab1 => "ctrl+1",
                Action::Tab2 => "ctrl+2",
                Action::Tab3 => "ctrl+3",
                Action::Tab4 => "ctrl+4",
                Action::Tab5 => "ctrl+5",
                Action::Tab6 => "ctrl+6",
                Action::Tab7 => "ctrl+7",
                Action::Tab8 => "ctrl+8",
                Action::TabLast => "ctrl+9",
                Action::ReopenTab => "ctrl+shift+t",
                Action::TabPicker => "ctrl+shift+a",
                Action::PasteMove | Action::Duplicate | Action::BatchRename | Action::ToggleTabLock => continue,
            };
            assert_eq!(reach(text), Some(action), "{text}");
        }
        // The second keys of the defaults.
        assert_eq!(reach("num+"), Some(Action::SelectPattern));
        assert_eq!(reach("num-"), Some(Action::DeselectPattern));
        // `.` is a key of its own; Shift+. arrives as the character it types (`>` on US, `:`
        // on Turkish Q), which is no chord off macOS: `ctrl+shift+.` cannot match there.
        let dot = parse_chord("ctrl+.", Platform::Other).unwrap().unwrap();
        assert_eq!(dot.key, Key::Char('.'));
        assert_eq!(chord_from_slint(".", true, false, false, false, Platform::Other), Some(dot));
        assert!(parse_chord("ctrl+shift+.", Platform::Other).unwrap().is_some());
        assert_eq!(chord_from_slint(">", true, false, true, false, Platform::Other), None);
        assert_eq!(chord_from_slint(":", true, false, true, false, Platform::Other), None);
        assert!(parse_chord("ctrl+>", Platform::Other).is_err());
        // Ctrl+H is free for toggle-hidden: nothing else uses it.
        let ctrl_h = chord_from_slint("h", true, false, false, false, Platform::Other).unwrap();
        assert_eq!(defaults.action_for(&ctrl_h), Some(Action::ToggleHidden));
        // Ctrl+Shift+T arrives as "T" with control and shift: not new-tab.
        let got = chord_from_slint("T", true, false, true, false, Platform::Other).unwrap();
        assert_eq!(defaults.action_for(&got), Some(Action::ReopenTab));
    }

    #[test]
    fn the_keypad_keys_are_their_own() {
        let defaults = Shortcuts::defaults(Platform::Other);
        let press =
            |text, physical, alt, shift| chord_from_press(text, physical, false, alt, shift, false, Platform::Other);
        assert_eq!(press("+", Physical::Numpad('+'), false, false).unwrap().key, Key::Num('+'));
        assert_eq!(
            defaults.action_for(&press("+", Physical::Numpad('+'), true, false).unwrap()),
            Some(Action::SelectSameType)
        );
        assert_eq!(
            defaults.action_for(&press("/", Physical::Numpad('/'), false, false).unwrap()),
            Some(Action::RestoreSelection)
        );
        // The main row's + (Shift+= on a US layout) is no keypad key and no chord.
        assert_eq!(press("+", Physical::Other, false, true), None);
        // A keypad key whose text is something else goes by its text.
        assert_eq!(press("4", Physical::Numpad('+'), false, false).unwrap().key, Key::Char('4'));
        // macOS: ⌘ is Slint's `control`.
        let cmd_plus = chord_from_press("+", Physical::Numpad('+'), true, false, false, false, Platform::Mac).unwrap();
        assert!(cmd_plus.meta && !cmd_plus.ctrl && cmd_plus.key == Key::Num('+'));
    }

    #[test]
    fn ctrl_shift_and_a_digit_is_the_digit_whatever_shift_types() {
        let defaults = Shortcuts::defaults(Platform::Other);
        // US: Shift+1 types !, Shift+2 @; Turkish Q: Shift+2 types '.
        for (text, digit, action) in
            [("!", '1', Action::ViewList), ("@", '2', Action::ViewGrid), ("'", '2', Action::ViewGrid)]
        {
            let got =
                chord_from_press(text, Physical::Digit(digit), true, false, true, false, Platform::Other).unwrap();
            assert_eq!(defaults.action_for(&got), Some(action), "{text}");
        }
        let cmd = chord_from_press("!", Physical::Digit('1'), true, false, true, false, Platform::Mac).unwrap();
        assert_eq!(Shortcuts::defaults(Platform::Mac).action_for(&cmd), Some(Action::ViewList));
        // AltGr (Ctrl+Alt on Windows) types what the layout says: Turkish Q AltGr+7 is `{`.
        assert_eq!(chord_from_press("{", Physical::Digit('7'), true, true, false, false, Platform::Other), None);
        // Plain Shift+1 types `!`: no chord.
        assert_eq!(chord_from_press("!", Physical::Digit('1'), false, false, true, false, Platform::Other), None);
    }

    #[test]
    fn winit_key_codes_are_sorted_out() {
        use slint::winit_030::winit::keyboard::KeyCode;
        assert_eq!(physical_of(KeyCode::NumpadAdd), Physical::Numpad('+'));
        assert_eq!(physical_of(KeyCode::NumpadSubtract), Physical::Numpad('-'));
        assert_eq!(physical_of(KeyCode::NumpadMultiply), Physical::Numpad('*'));
        assert_eq!(physical_of(KeyCode::NumpadDivide), Physical::Numpad('/'));
        assert_eq!(physical_of(KeyCode::Digit0), Physical::Digit('0'));
        assert_eq!(physical_of(KeyCode::Digit9), Physical::Digit('9'));
        assert_eq!(physical_of(KeyCode::Numpad1), Physical::Other);
        assert_eq!(physical_of(KeyCode::KeyA), Physical::Other);
    }

    #[test]
    fn a_noted_key_is_taken_once() {
        note_pressed(Physical::Numpad('+'));
        assert_eq!(take_pressed(), Physical::Numpad('+'));
        assert_eq!(take_pressed(), Physical::Other);
    }

    #[test]
    fn mac_command_and_control_are_swapped_back() {
        let defaults = Shortcuts::defaults(Platform::Mac);
        // Cmd+T: Slint reports Command as `control`.
        let cmd_t = chord_from_slint("t", true, false, false, false, Platform::Mac).unwrap();
        assert_eq!(defaults.action_for(&cmd_t), Some(Action::NewTab));
        // Physical Ctrl+Tab: Slint reports Control as `meta`.
        let ctrl_tab = chord_from_slint("\t", false, false, false, true, Platform::Mac).unwrap();
        assert_eq!(defaults.action_for(&ctrl_tab), Some(Action::NextTab));
        let ctrl_shift_tab = chord_from_slint("\t", false, false, true, true, Platform::Mac).unwrap();
        assert_eq!(defaults.action_for(&ctrl_shift_tab), Some(Action::PrevTab));
        let cmd_bracket = chord_from_slint("[", true, false, false, false, Platform::Mac).unwrap();
        assert_eq!(defaults.action_for(&cmd_bracket), Some(Action::Back));
        // Turkish-QWERTY-PC: the key right of P types `ğ`; ⌘ on it is still ⌘[ (by its place).
        let text = bracket_text("ğ", '[');
        let cmd_g = chord_from_slint(&text, true, false, false, false, Platform::Mac).unwrap();
        assert_eq!(defaults.action_for(&cmd_g), Some(Action::Back));
        let text = bracket_text("ü", ']');
        let cmd_u = chord_from_slint(&text, true, false, false, false, Platform::Mac).unwrap();
        assert_eq!(defaults.action_for(&cmd_u), Some(Action::Forward));
        // ⌘⇧. shows hidden files (the text is the key's own `.`, see `shortcut_text`).
        let cmd_shift_dot = chord_from_slint(".", true, false, true, false, Platform::Mac).unwrap();
        assert_eq!(defaults.action_for(&cmd_shift_dot), Some(Action::ToggleHidden));
        let cmd_r = chord_from_slint("r", true, false, false, false, Platform::Mac).unwrap();
        assert_eq!(defaults.action_for(&cmd_r), Some(Action::Refresh));
        // Physical Ctrl+T is not Cmd+T.
        let ctrl_t = chord_from_slint("t", false, false, false, true, Platform::Mac).unwrap();
        assert_eq!(defaults.action_for(&ctrl_t), None);
    }

    #[test]
    fn text_edit_uses_the_primary_modifier() {
        let other = |c, m| chord_from_slint(c, m, false, false, false, Platform::Other).unwrap();
        assert!(is_text_edit(&other("a", true), Platform::Other));
        assert!(is_text_edit(&other("V", true), Platform::Other));
        assert!(!is_text_edit(&other("w", true), Platform::Other));
        assert!(!is_text_edit(&other("a", false), Platform::Other));
        // Win+C is not copy.
        let win_c = chord_from_slint("c", false, false, false, true, Platform::Other).unwrap();
        assert!(!is_text_edit(&win_c, Platform::Other));
        // On macOS, Cmd (Slint `control`) is the primary modifier; physical Ctrl is not.
        let cmd_a = chord_from_slint("a", true, false, false, false, Platform::Mac).unwrap();
        assert!(is_text_edit(&cmd_a, Platform::Mac));
        let ctrl_a = chord_from_slint("a", false, false, false, true, Platform::Mac).unwrap();
        assert!(!is_text_edit(&ctrl_a, Platform::Mac));
    }

    #[test]
    fn primary_modifier_is_cmd_on_mac() {
        let ctrl_a = chord_from_slint("a", true, false, false, false, Platform::Other).unwrap();
        assert!(is_primary(&ctrl_a, Platform::Other));
        let cmd_a = chord_from_slint("a", true, false, false, false, Platform::Mac).unwrap();
        assert!(is_primary(&cmd_a, Platform::Mac));
        let ctrl_on_mac = chord_from_slint("a", false, false, false, true, Platform::Mac).unwrap();
        assert!(!is_primary(&ctrl_on_mac, Platform::Mac));
    }

    #[test]
    fn type_ahead_is_active_for_a_second_after_a_key() {
        let mut t = TypeAhead::new();
        let t0 = Instant::now();
        assert!(!t.is_active(t0));
        t.type_char('a', t0, |_| None);
        assert!(t.is_active(t0 + Duration::from_millis(900)));
        assert!(!t.is_active(t0 + Duration::from_millis(1100)));
    }

    #[test]
    fn typed_char_takes_printable_characters_only() {
        assert_eq!(typed_char("a"), Some('a'));
        assert_eq!(typed_char("Ş"), Some('Ş'));
        assert_eq!(typed_char("."), Some('.'));
        assert_eq!(typed_char(&text(SlintKey::DownArrow)), None);
        assert_eq!(typed_char(&text(SlintKey::F2)), None);
        assert_eq!(typed_char(&text(SlintKey::Escape)), None);
        assert_eq!(typed_char(&text(SlintKey::Return)), None);
        assert_eq!(typed_char("ab"), None);
        assert_eq!(typed_char(""), None);
    }

    /// Finds the first of `list` starting with the typed text, as the navigator does.
    fn names<'a>(list: &'a [&'a str]) -> impl FnOnce(&str) -> Option<usize> + 'a {
        move |typed| list.iter().position(|name| starts_with_lowercase(name, typed))
    }

    #[test]
    fn type_ahead_joins_letters_within_a_second() {
        let list = ["Apple", "Banana", "Bandit", "cherry"];
        let mut t = TypeAhead::new();
        let t0 = Instant::now();
        assert_eq!(t.type_char('b', t0, names(&list)), Some(1));
        assert_eq!(t.type_char('a', t0 + Duration::from_millis(300), names(&list)), Some(1));
        assert_eq!(t.type_char('n', t0 + Duration::from_millis(600), names(&list)), Some(1));
        assert_eq!(t.type_char('d', t0 + Duration::from_millis(900), names(&list)), Some(2));
        assert_eq!(t.type_char('c', t0 + Duration::from_millis(2500), names(&list)), Some(3), "a pause starts over");
        assert_eq!(t.type_char('z', t0 + Duration::from_millis(5000), names(&list)), None);
    }

    #[test]
    fn type_ahead_matches_non_ascii_names() {
        let mut t = TypeAhead::default();
        let list = ["Belgeler", "İndirilenler", "şablonlar"];
        assert_eq!(t.type_char('Ş', Instant::now(), names(&list)), Some(2));
    }
}
