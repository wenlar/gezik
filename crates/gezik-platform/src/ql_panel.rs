//! The system Quick Look panel (spec 9 §4.5): what a key pressed in it does. The panel itself
//! is macOS's (`mac::ql_panel`).

use gezik_core::layout::Move;

/// What a key the panel did not use does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelKey {
    /// Gezik moves its focus; the panel then shows the new item.
    Move(Move),
    /// Space: the panel closes (Finder's toggle).
    Close,
    /// Left to the panel.
    Pass,
}

/// `key_code`: macOS's virtual key code. `modified`: ⌘, ⌃, ⌥ or ⇧ held. `items`: the items the
/// panel shows; with several, the arrows page among them in the panel.
pub fn key_action(key_code: u16, modified: bool, items: usize) -> PanelKey {
    let arrow = match key_code {
        123 => Move::Left,
        124 => Move::Right,
        125 => Move::Down,
        126 => Move::Up,
        49 if !modified => return PanelKey::Close,
        _ => return PanelKey::Pass,
    };
    if modified || items != 1 { PanelKey::Pass } else { PanelKey::Move(arrow) }
}

#[cfg(target_os = "macos")]
pub use crate::mac::ql_panel::{close, is_open, show, update};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_keys_move_gezik_only_with_one_item() {
        assert_eq!(key_action(126, false, 1), PanelKey::Move(Move::Up));
        assert_eq!(key_action(125, false, 1), PanelKey::Move(Move::Down));
        assert_eq!(key_action(123, false, 1), PanelKey::Move(Move::Left));
        assert_eq!(key_action(124, false, 1), PanelKey::Move(Move::Right));
        assert_eq!(key_action(124, false, 3), PanelKey::Pass, "several items: the panel pages");
        assert_eq!(key_action(126, true, 1), PanelKey::Pass, "with a modifier: the panel's");
        assert_eq!(key_action(49, false, 1), PanelKey::Close, "Space");
        assert_eq!(key_action(49, false, 4), PanelKey::Close);
        assert_eq!(key_action(53, false, 1), PanelKey::Pass, "Esc is the panel's own");
        assert_eq!(key_action(0, false, 1), PanelKey::Pass, "A");
    }
}
