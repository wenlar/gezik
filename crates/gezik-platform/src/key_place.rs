//! macOS: which key a key event came from, by its place on the keyboard. ⌘ shortcuts on
//! punctuation go by the key's place, as the system's own do: ⌘[ is the key right of P on
//! every layout, also where that key types something else (`ğ` on Turkish-QWERTY-PC, whose
//! `[` is ⌥8).

use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSEventModifierFlags, NSEventType};

/// The bracket the key of the key press being handled has on a US keyboard, if it is one of
/// the two bracket keys.
pub fn bracket_of_key_being_pressed() -> Option<char> {
    let mtm = MainThreadMarker::new()?;
    let event = NSApplication::sharedApplication(mtm).currentEvent()?;
    if event.r#type() != NSEventType::KeyDown {
        return None;
    }
    bracket_of_key_code(event.keyCode())
}

/// What the key of the key press being handled types with no modifier at all, on the
/// layout in use (`.` for the key whose Shift gives `>` or `:`).
pub fn unshifted_text_of_key_being_pressed() -> Option<String> {
    let mtm = MainThreadMarker::new()?;
    let event = NSApplication::sharedApplication(mtm).currentEvent()?;
    if event.r#type() != NSEventType::KeyDown {
        return None;
    }
    Some(event.charactersByApplyingModifiers(NSEventModifierFlags::empty())?.to_string())
}

/// `kVK_ANSI_LeftBracket` and `kVK_ANSI_RightBracket`: key codes name places, not characters.
pub fn bracket_of_key_code(code: u16) -> Option<char> {
    match code {
        0x21 => Some('['),
        0x1E => Some(']'),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_keys_right_of_p_are_the_brackets() {
        assert_eq!(bracket_of_key_code(0x21), Some('['));
        assert_eq!(bracket_of_key_code(0x1E), Some(']'));
        // P (0x23) and the digit 8 (0x1C), which is [ with ⌥ on Turkish-QWERTY-PC.
        assert_eq!(bracket_of_key_code(0x23), None);
        assert_eq!(bracket_of_key_code(0x1C), None);
    }

    /// Off the main thread (as here) there is no event being handled.
    #[test]
    fn no_key_is_being_pressed_in_a_test() {
        assert_eq!(bracket_of_key_being_pressed(), None);
        assert_eq!(unshifted_text_of_key_being_pressed(), None);
    }
}
