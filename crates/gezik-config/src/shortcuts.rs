//! Keyboard shortcuts: `mod+t`-style text ↔ chords, defaults per platform, and the
//! `[shortcuts]` table with its conflict rules.

use crate::Warning;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    /// Lowercase ASCII letter or digit, `[` or `]`.
    Char(char),
    F(u8),
    Left,
    Right,
    Up,
    Down,
    Enter,
    Tab,
    Backspace,
    Delete,
    Home,
    End,
    PageUp,
    PageDown,
    Escape,
    Space,
}

impl Key {
    fn parse(name: &str) -> Option<Key> {
        Some(match name {
            "left" => Key::Left,
            "right" => Key::Right,
            "up" => Key::Up,
            "down" => Key::Down,
            "enter" => Key::Enter,
            "tab" => Key::Tab,
            "backspace" => Key::Backspace,
            "delete" => Key::Delete,
            "home" => Key::Home,
            "end" => Key::End,
            "pageup" => Key::PageUp,
            "pagedown" => Key::PageDown,
            "escape" => Key::Escape,
            "space" => Key::Space,
            _ => {
                if let Some(n) = name.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()) {
                    return (1..=12).contains(&n).then_some(Key::F(n));
                }
                let mut chars = name.chars();
                let (Some(c), None) = (chars.next(), chars.next()) else { return None };
                if c.is_ascii_alphanumeric() || c == '[' || c == ']' {
                    Key::Char(c.to_ascii_lowercase())
                } else {
                    return None;
                }
            }
        })
    }
}

/// A key plus modifiers, with `mod` already resolved for the platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Chord {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// Cmd on macOS.
    pub meta: bool,
    pub key: Key,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Mac,
    Other,
}

impl Platform {
    pub fn current() -> Platform {
        if cfg!(target_os = "macos") { Platform::Mac } else { Platform::Other }
    }
}

/// Parses `"mod+shift+t"`. `""` means "no shortcut" (`Ok(None)`).
pub fn parse_chord(text: &str, platform: Platform) -> Result<Option<Chord>, String> {
    let text = text.trim().to_ascii_lowercase();
    if text.is_empty() {
        return Ok(None);
    }
    let parts: Vec<&str> = text.split('+').map(str::trim).collect();
    let (key_name, modifiers) = parts.split_last().expect("split always yields one part");
    if key_name.is_empty() {
        return Err("missing key after \"+\"".to_owned());
    }
    let key = Key::parse(key_name).ok_or_else(|| format!("unknown key \"{key_name}\""))?;
    let mut chord = Chord { ctrl: false, alt: false, shift: false, meta: false, key };
    for modifier in modifiers {
        match *modifier {
            "ctrl" => chord.ctrl = true,
            "alt" => chord.alt = true,
            "shift" => chord.shift = true,
            "mod" => match platform {
                Platform::Mac => chord.meta = true,
                Platform::Other => chord.ctrl = true,
            },
            other => return Err(format!("unknown modifier \"{other}\"")),
        }
    }
    Ok(Some(chord))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    NewTab,
    CloseTab,
    NextTab,
    PrevTab,
    Back,
    Forward,
    Up,
    FocusPath,
    Refresh,
    SelectAll,
    ViewList,
    ViewGrid,
}

impl Action {
    pub const ALL: [Action; 12] = [
        Action::NewTab,
        Action::CloseTab,
        Action::NextTab,
        Action::PrevTab,
        Action::Back,
        Action::Forward,
        Action::Up,
        Action::FocusPath,
        Action::Refresh,
        Action::SelectAll,
        Action::ViewList,
        Action::ViewGrid,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Action::NewTab => "new-tab",
            Action::CloseTab => "close-tab",
            Action::NextTab => "next-tab",
            Action::PrevTab => "prev-tab",
            Action::Back => "back",
            Action::Forward => "forward",
            Action::Up => "up",
            Action::FocusPath => "focus-path",
            Action::Refresh => "refresh",
            Action::SelectAll => "select-all",
            Action::ViewList => "view-list",
            Action::ViewGrid => "view-grid",
        }
    }

    fn from_name(name: &str) -> Option<Action> {
        Action::ALL.into_iter().find(|a| a.name() == name)
    }

    fn default_text(self, platform: Platform) -> &'static str {
        match (self, platform) {
            (Action::NewTab, _) => "mod+t",
            (Action::CloseTab, _) => "mod+w",
            (Action::NextTab, _) => "ctrl+tab",
            (Action::PrevTab, _) => "ctrl+shift+tab",
            (Action::Back, Platform::Mac) => "mod+[",
            (Action::Back, Platform::Other) => "alt+left",
            (Action::Forward, Platform::Mac) => "mod+]",
            (Action::Forward, Platform::Other) => "alt+right",
            (Action::Up, Platform::Mac) => "mod+up",
            (Action::Up, Platform::Other) => "alt+up",
            (Action::FocusPath, _) => "mod+l",
            (Action::Refresh, Platform::Mac) => "mod+r",
            (Action::Refresh, Platform::Other) => "f5",
            (Action::SelectAll, _) => "mod+a",
            (Action::ViewList, _) => "mod+1",
            (Action::ViewGrid, _) => "mod+2",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shortcuts {
    bindings: Vec<(Chord, Action)>,
}

impl Default for Shortcuts {
    fn default() -> Self {
        Shortcuts::defaults(Platform::current())
    }
}

impl Shortcuts {
    pub fn defaults(platform: Platform) -> Shortcuts {
        Shortcuts::from_table(None, platform, "settings.toml", &mut Vec::new())
    }

    /// Builds the bindings from the `[shortcuts]` table (or defaults if `None`). User
    /// entries win over defaults; among user entries, the one written first wins.
    pub fn from_table(
        table: Option<&toml::Table>,
        platform: Platform,
        file: &str,
        warnings: &mut Vec<Warning>,
    ) -> Shortcuts {
        // Actions the user configured (validly), in file order; None = disabled.
        let mut user: Vec<(Action, Option<Chord>)> = Vec::new();
        for (name, value) in table.into_iter().flatten() {
            let Some(action) = Action::from_name(name) else {
                warnings.push(Warning::new(file, format!("shortcuts.{name}: unknown action")));
                continue;
            };
            let Some(text) = value.as_str() else {
                warnings.push(Warning::new(file, format!("shortcuts.{name}: expected text, got {value}")));
                continue;
            };
            match parse_chord(text, platform) {
                Ok(chord) => user.push((action, chord)),
                Err(err) => warnings.push(Warning::new(file, format!("shortcuts.{name}: {err}; using the default"))),
            }
        }

        let mut bindings: Vec<(Chord, Action)> = Vec::new();
        for (action, chord) in &user {
            let Some(chord) = chord else { continue };
            if let Some((_, owner)) = bindings.iter().find(|(c, _)| c == chord) {
                warnings.push(Warning::new(
                    file,
                    format!(
                        "shortcuts.{}: already used by {}; {} is disabled",
                        action.name(),
                        owner.name(),
                        action.name()
                    ),
                ));
                continue;
            }
            bindings.push((*chord, *action));
        }

        for action in Action::ALL {
            if user.iter().any(|(a, _)| *a == action) {
                continue;
            }
            let text = action.default_text(platform);
            let chord = parse_chord(text, platform).expect("defaults are valid").expect("defaults are set");
            if let Some((_, owner)) = bindings.iter().find(|(c, _)| *c == chord) {
                warnings.push(Warning::new(
                    file,
                    format!(
                        "shortcuts: the default \"{text}\" of {} is used by {}; {} is disabled",
                        action.name(),
                        owner.name(),
                        action.name()
                    ),
                ));
                continue;
            }
            bindings.push((chord, action));
        }
        Shortcuts { bindings }
    }

    pub fn action_for(&self, chord: &Chord) -> Option<Action> {
        self.bindings.iter().find(|(c, _)| c == chord).map(|(_, a)| *a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(text: &str) -> Chord {
        parse_chord(text, Platform::Other).unwrap().unwrap()
    }

    fn key(c: char) -> Key {
        Key::Char(c)
    }

    #[test]
    fn parses_modifiers_in_any_order_and_case() {
        let c = chord("Shift+CTRL+T");
        assert_eq!(c, Chord { ctrl: true, alt: false, shift: true, meta: false, key: key('t') });
        assert_eq!(chord("ctrl+shift+t"), c);
    }

    #[test]
    fn mod_is_cmd_on_mac_and_ctrl_elsewhere() {
        let mac = parse_chord("mod+t", Platform::Mac).unwrap().unwrap();
        assert!(mac.meta && !mac.ctrl);
        let other = parse_chord("mod+t", Platform::Other).unwrap().unwrap();
        assert!(other.ctrl && !other.meta);
    }

    #[test]
    fn parses_named_keys() {
        assert_eq!(chord("f5").key, Key::F(5));
        assert_eq!(chord("alt+left").key, Key::Left);
        assert_eq!(chord("mod+[").key, key('['));
        assert_eq!(chord("pagedown").key, Key::PageDown);
        assert_eq!(chord("ctrl+tab").key, Key::Tab);
    }

    #[test]
    fn empty_text_disables() {
        assert_eq!(parse_chord("", Platform::Other), Ok(None));
        assert_eq!(parse_chord("  ", Platform::Other), Ok(None));
    }

    #[test]
    fn rejects_invalid_text() {
        for bad in ["ctrl+", "hyper+t", "ctrl+f13", "ctrl+tt", "ctrl+é", "+"] {
            assert!(parse_chord(bad, Platform::Other).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn defaults_cover_every_action() {
        for platform in [Platform::Mac, Platform::Other] {
            let s = Shortcuts::defaults(platform);
            for action in Action::ALL {
                let text = action.default_text(platform);
                let c = parse_chord(text, platform).unwrap().unwrap();
                assert_eq!(s.action_for(&c), Some(action), "{platform:?} {}", action.name());
            }
        }
    }

    fn table(text: &str) -> toml::Table {
        text.parse::<toml::Table>().unwrap()
    }

    fn build(text: &str) -> (Shortcuts, Vec<Warning>) {
        let mut warnings = Vec::new();
        let t = table(text);
        let s = Shortcuts::from_table(
            t.get("shortcuts").and_then(|v| v.as_table()),
            Platform::Other,
            "settings.toml",
            &mut warnings,
        );
        (s, warnings)
    }

    #[test]
    fn user_binding_replaces_default() {
        let (s, warnings) = build("[shortcuts]\nnew-tab = \"ctrl+n\"\n");
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(s.action_for(&chord("ctrl+n")), Some(Action::NewTab));
        assert_eq!(s.action_for(&chord("ctrl+t")), None);
    }

    #[test]
    fn empty_string_disables_an_action() {
        let (s, warnings) = build("[shortcuts]\nrefresh = \"\"\n");
        assert!(warnings.is_empty());
        assert_eq!(s.action_for(&chord("f5")), None);
    }

    #[test]
    fn invalid_binding_keeps_default_with_warning() {
        let (s, warnings) = build("[shortcuts]\nrefresh = \"hyper+r\"\nback = 5\n");
        assert_eq!(s.action_for(&chord("f5")), Some(Action::Refresh));
        assert_eq!(s.action_for(&chord("alt+left")), Some(Action::Back));
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(warnings[0].message.starts_with("shortcuts.refresh:"));
        assert!(warnings[1].message.starts_with("shortcuts.back:"));
    }

    #[test]
    fn unknown_action_is_reported() {
        let (_, warnings) = build("[shortcuts]\nteleport = \"ctrl+j\"\n");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("teleport"));
    }

    #[test]
    fn user_binding_on_another_actions_default_disables_that_default() {
        let (s, warnings) = build("[shortcuts]\nrefresh = \"ctrl+t\"\n");
        assert_eq!(s.action_for(&chord("ctrl+t")), Some(Action::Refresh));
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].message.contains("new-tab"));
    }

    #[test]
    fn first_written_user_binding_wins() {
        let (s, warnings) = build("[shortcuts]\nup = \"ctrl+u\"\nback = \"ctrl+u\"\n");
        assert_eq!(s.action_for(&chord("ctrl+u")), Some(Action::Up));
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.starts_with("shortcuts.back:"));
        // back lost its user binding and is disabled (not reverted to its default).
        assert_eq!(s.action_for(&chord("alt+left")), None);
    }
}
