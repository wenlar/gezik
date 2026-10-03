//! Keyboard: Slint key events → shortcut chords, list navigation keys, type-ahead.

use std::cell::RefCell;
use std::time::{Duration, Instant};

use gezik_config::shortcuts::{Action, Chord, Key, Shortcuts};
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

/// Converts a Slint key event's text and modifiers into a chord, if the key is one we know.
pub fn chord_from_event(text: &str, ctrl: bool, alt: bool, shift: bool, meta: bool) -> Option<Chord> {
    let mut chars = text.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else { return None };
    let named = [
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
    let key = if let Some((_, key)) = named.iter().find(|(k, _)| char::from(*k) == c) {
        *key
    } else if c.is_ascii_alphanumeric() || c == '[' || c == ']' {
        Key::Char(c.to_ascii_lowercase())
    } else {
        return None;
    };
    // Backtab is Shift+Tab even if the platform did not report shift.
    let shift = shift || c == char::from(SlintKey::Backtab);
    Some(Chord { ctrl, alt, shift, meta, key })
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

    /// Adds `c` (typed at `now`) and returns the index of the first name starting with the
    /// typed text (case-insensitive), searching all names.
    pub fn type_char(&mut self, c: char, now: Instant, names: impl Iterator<Item = String>) -> Option<usize> {
        if self.last.is_none_or(|last| now.saturating_duration_since(last) > Self::WINDOW) {
            self.typed.clear();
        }
        self.last = Some(now);
        self.typed.extend(c.to_lowercase());
        let typed = self.typed.as_str();
        names.enumerate().find(|(_, name)| name.to_lowercase().starts_with(typed)).map(|(i, _)| i)
    }
}

impl Default for TypeAhead {
    fn default() -> Self {
        TypeAhead::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn backtab_is_shift_tab() {
        let c = chord_from_event(&text(SlintKey::Backtab), true, false, false, false).unwrap();
        assert_eq!(c, Chord { ctrl: true, alt: false, shift: true, meta: false, key: Key::Tab });
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

    fn names<'a>(list: &'a [&'a str]) -> impl Iterator<Item = String> + 'a {
        list.iter().map(|s| s.to_string())
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
