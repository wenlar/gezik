//! Keyboard shortcuts: `mod+t`-style text ↔ chords, defaults per platform, and the
//! `[shortcuts]` table with its conflict rules.

use crate::Warning;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    /// Lowercase ASCII letter or digit, `[`, `]`, `.`, `=` or `-`.
    Char(char),
    /// A key of the numeric keypad: '+', '-', '*' or '/'.
    Num(char),
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
            "num-" => Key::Num('-'),
            "num*" => Key::Num('*'),
            "num/" => Key::Num('/'),
            _ => {
                if let Some(n) = name.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()) {
                    return (1..=12).contains(&n).then_some(Key::F(n));
                }
                let mut chars = name.chars();
                let (Some(c), None) = (chars.next(), chars.next()) else { return None };
                if c.is_ascii_alphanumeric() || matches!(c, '[' | ']' | '.' | '=' | '-') {
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

impl Chord {
    /// Whether the chord can be a `[[commands]]` key: with Ctrl, Alt or Cmd, or an F key. A
    /// bare key (or Shift and a key) types: it would take letters from type-ahead and the
    /// filter.
    pub fn leaves_typing_alone(&self) -> bool {
        self.ctrl || self.alt || self.meta || matches!(self.key, Key::F(_))
    }
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
    // "num+" ends in the separator: it is taken off before the rest is split.
    let (modifiers, key) = match text.strip_suffix("num+") {
        Some("") => ("", Key::Num('+')),
        Some(rest) if rest.trim_end().ends_with('+') => {
            let rest = rest.trim_end();
            (&rest[..rest.len() - 1], Key::Num('+'))
        }
        _ => {
            let (modifiers, name) = text.rsplit_once('+').unwrap_or(("", text.as_str()));
            let name = name.trim();
            if name.is_empty() {
                return Err("missing key after \"+\"".to_owned());
            }
            (modifiers, Key::parse(name).ok_or_else(|| format!("unknown key \"{name}\""))?)
        }
    };
    let mut chord = Chord { ctrl: false, alt: false, shift: false, meta: false, key };
    if !modifiers.is_empty() {
        for modifier in modifiers.split('+').map(str::trim) {
            match modifier {
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
    TogglePreview,
    QuickLook,
    Copy,
    Cut,
    Paste,
    /// Paste what was copied as a move (macOS ⌘⌥V, as in Finder).
    PasteMove,
    Trash,
    DeletePermanently,
    Rename,
    NewFolder,
    Duplicate,
    Undo,
    Redo,
    /// Opens the batch rename layer even for one item.
    BatchRename,
    /// Shows or hides the files whose names start with a dot (macOS ⌘⇧., as in Finder).
    ToggleHidden,
    /// Opens the filter field of the place (6a).
    Filter,
    InvertSelection,
    SelectPattern,
    DeselectPattern,
    /// Selects the files of the type (extension) of the focused one.
    SelectSameType,
    /// Brings back the selection from before the last file operation.
    RestoreSelection,
    Tab1,
    Tab2,
    Tab3,
    Tab4,
    Tab5,
    Tab6,
    Tab7,
    Tab8,
    TabLast,
    ReopenTab,
    TabPicker,
    ToggleTabLock,
    /// Forgets the folders the address bar remembers (6b).
    ClearHistory,
}

impl Action {
    pub const ALL: [Action; 46] = [
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
        Action::TogglePreview,
        Action::QuickLook,
        Action::Copy,
        Action::Cut,
        Action::Paste,
        Action::PasteMove,
        Action::Trash,
        Action::DeletePermanently,
        Action::Rename,
        Action::NewFolder,
        Action::Duplicate,
        Action::Undo,
        Action::Redo,
        Action::BatchRename,
        Action::ToggleHidden,
        Action::Filter,
        Action::InvertSelection,
        Action::SelectPattern,
        Action::DeselectPattern,
        Action::SelectSameType,
        Action::RestoreSelection,
        Action::Tab1,
        Action::Tab2,
        Action::Tab3,
        Action::Tab4,
        Action::Tab5,
        Action::Tab6,
        Action::Tab7,
        Action::Tab8,
        Action::TabLast,
        Action::ReopenTab,
        Action::TabPicker,
        Action::ToggleTabLock,
        Action::ClearHistory,
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
            Action::TogglePreview => "toggle-preview",
            Action::QuickLook => "quick-look",
            Action::Copy => "copy",
            Action::Cut => "cut",
            Action::Paste => "paste",
            Action::PasteMove => "paste-move",
            Action::Trash => "trash",
            Action::DeletePermanently => "delete-permanently",
            Action::Rename => "rename",
            Action::NewFolder => "new-folder",
            Action::Duplicate => "duplicate",
            Action::Undo => "undo",
            Action::Redo => "redo",
            Action::BatchRename => "batch-rename",
            Action::ToggleHidden => "toggle-hidden",
            Action::Filter => "filter",
            Action::InvertSelection => "invert-selection",
            Action::SelectPattern => "select-pattern",
            Action::DeselectPattern => "deselect-pattern",
            Action::SelectSameType => "select-same-type",
            Action::RestoreSelection => "restore-selection",
            Action::Tab1 => "tab-1",
            Action::Tab2 => "tab-2",
            Action::Tab3 => "tab-3",
            Action::Tab4 => "tab-4",
            Action::Tab5 => "tab-5",
            Action::Tab6 => "tab-6",
            Action::Tab7 => "tab-7",
            Action::Tab8 => "tab-8",
            Action::TabLast => "tab-last",
            Action::ReopenTab => "reopen-tab",
            Action::TabPicker => "tab-picker",
            Action::ToggleTabLock => "toggle-tab-lock",
            Action::ClearHistory => "clear-history",
        }
    }

    /// The tab a `tab-N` action shows (1-based): `Tab1` → 1 … `Tab8` → 8.
    pub fn tab_number(self) -> Option<usize> {
        Some(match self {
            Action::Tab1 => 1,
            Action::Tab2 => 2,
            Action::Tab3 => 3,
            Action::Tab4 => 4,
            Action::Tab5 => 5,
            Action::Tab6 => 6,
            Action::Tab7 => 7,
            Action::Tab8 => 8,
            _ => return None,
        })
    }

    /// The action named `name` in settings.toml (`new-tab`).
    pub fn from_name(name: &str) -> Option<Action> {
        Action::ALL.into_iter().find(|a| a.name() == name)
    }

    /// The default chords on `platform` (none, one or several).
    fn default_texts(self, platform: Platform) -> &'static [&'static str] {
        match (self, platform) {
            (Action::NewTab, _) => &["mod+t"],
            (Action::CloseTab, _) => &["mod+w"],
            (Action::NextTab, _) => &["ctrl+tab"],
            (Action::PrevTab, _) => &["ctrl+shift+tab"],
            (Action::Back, Platform::Mac) => &["mod+["],
            (Action::Back, Platform::Other) => &["alt+left"],
            (Action::Forward, Platform::Mac) => &["mod+]"],
            (Action::Forward, Platform::Other) => &["alt+right"],
            (Action::Up, Platform::Mac) => &["mod+up"],
            (Action::Up, Platform::Other) => &["alt+up"],
            (Action::FocusPath, _) => &["mod+l"],
            (Action::Refresh, Platform::Mac) => &["mod+r"],
            (Action::Refresh, Platform::Other) => &["f5"],
            (Action::SelectAll, _) => &["mod+a"],
            (Action::ViewList, _) => &["mod+shift+1"],
            (Action::ViewGrid, _) => &["mod+shift+2"],
            (Action::TogglePreview, _) => &["alt+p"],
            (Action::QuickLook, _) => &["space"],
            (Action::Copy, _) => &["mod+c"],
            (Action::Cut, _) => &["mod+x"],
            (Action::Paste, _) => &["mod+v"],
            (Action::PasteMove, Platform::Mac) => &["mod+alt+v"],
            (Action::PasteMove, Platform::Other) => &[],
            (Action::Trash, Platform::Mac) => &["mod+backspace"],
            (Action::Trash, Platform::Other) => &["delete"],
            (Action::DeletePermanently, Platform::Mac) => &["mod+alt+backspace"],
            (Action::DeletePermanently, Platform::Other) => &["shift+delete"],
            (Action::Rename, Platform::Mac) => &["enter"],
            (Action::Rename, Platform::Other) => &["f2"],
            (Action::NewFolder, _) => &["mod+shift+n"],
            // Ctrl+D deletes in Explorer: Windows and Linux users get no surprise copies.
            (Action::Duplicate, Platform::Mac) => &["mod+d"],
            (Action::Duplicate, Platform::Other) => &[],
            (Action::Undo, _) => &["mod+z"],
            (Action::Redo, Platform::Mac) => &["mod+shift+z"],
            (Action::Redo, Platform::Other) => &["mod+y"],
            (Action::BatchRename, _) => &[],
            // Ctrl+H elsewhere, as in Linux file managers: Ctrl+Shift+. would never match
            // there (Shift+. types `>` or `:`, and only macOS maps keys by their place).
            (Action::ToggleHidden, Platform::Mac) => &["mod+shift+."],
            (Action::ToggleHidden, Platform::Other) => &["ctrl+h"],
            (Action::Filter, _) => &["mod+f"],
            (Action::InvertSelection, _) => &["mod+shift+i"],
            // The playable chord first: the macOS menu bar plays `chord_for`, the first binding.
            (Action::SelectPattern, _) => &["mod+=", "num+"],
            (Action::DeselectPattern, _) => &["mod+-", "num-"],
            (Action::SelectSameType, _) => &["alt+num+"],
            (Action::RestoreSelection, _) => &["num/"],
            (Action::Tab1, _) => &["mod+1"],
            (Action::Tab2, _) => &["mod+2"],
            (Action::Tab3, _) => &["mod+3"],
            (Action::Tab4, _) => &["mod+4"],
            (Action::Tab5, _) => &["mod+5"],
            (Action::Tab6, _) => &["mod+6"],
            (Action::Tab7, _) => &["mod+7"],
            (Action::Tab8, _) => &["mod+8"],
            (Action::TabLast, _) => &["mod+9"],
            (Action::ReopenTab, _) => &["mod+shift+t"],
            (Action::TabPicker, _) => &["mod+shift+a"],
            (Action::ToggleTabLock, _) => &[],
            (Action::ClearHistory, _) => &[],
        }
    }
}

/// What a key is bound to already.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyOwner {
    Action(Action),
    /// A `[[commands]]` entry, by its index among the valid ones.
    Command(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shortcuts {
    bindings: Vec<(Chord, Action)>,
    /// The keys of `[[commands]]` entries, by their index among the valid ones.
    commands: Vec<(Chord, usize)>,
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
        // Actions the user configured (validly), in file order; no chords = disabled.
        let mut user: Vec<(Action, Vec<Chord>)> = Vec::new();
        for (name, value) in table.into_iter().flatten() {
            let Some(action) = Action::from_name(name) else {
                warnings.push(Warning::new(file, format!("shortcuts.{name}: unknown action")));
                continue;
            };
            // "text" or ["text", …].
            let texts: Option<Vec<&str>> = match value {
                toml::Value::String(text) => Some(vec![text.as_str()]),
                toml::Value::Array(items) => items.iter().map(toml::Value::as_str).collect(),
                _ => None,
            };
            let Some(texts) = texts else {
                warnings.push(Warning::new(
                    file,
                    format!("shortcuts.{name}: expected text or a list of texts, got {value}"),
                ));
                continue;
            };
            let parsed: Result<Vec<Option<Chord>>, String> =
                texts.iter().map(|text| parse_chord(text, platform)).collect();
            match parsed {
                Ok(chords) => user.push((action, chords.into_iter().flatten().collect())),
                Err(err) => warnings.push(Warning::new(file, format!("shortcuts.{name}: {err}; using the default"))),
            }
        }

        // A key taken by an earlier binding costs just that key while the action keeps another
        // one; with none left, the action is disabled.
        let mut bindings: Vec<(Chord, Action)> = Vec::new();
        for (action, chords) in &user {
            let mut taken: Vec<Action> = Vec::new();
            for chord in chords {
                match bindings.iter().find(|(c, _)| c == chord) {
                    // The same key written twice for one action is just that key.
                    Some((_, owner)) if owner == action => {}
                    Some((_, owner)) => taken.push(*owner),
                    None => bindings.push((*chord, *action)),
                }
            }
            let kept = bindings.iter().any(|(_, a)| a == action);
            for owner in taken {
                let cost =
                    if kept { "that key is left out".to_owned() } else { format!("{} is disabled", action.name()) };
                warnings.push(Warning::new(
                    file,
                    format!("shortcuts.{}: already used by {}; {cost}", action.name(), owner.name()),
                ));
            }
        }

        for action in Action::ALL {
            if user.iter().any(|(a, _)| *a == action) {
                continue;
            }
            let mut taken: Vec<(&str, Action)> = Vec::new();
            for text in action.default_texts(platform) {
                let chord = parse_chord(text, platform).expect("defaults are valid").expect("defaults are set");
                match bindings.iter().find(|(c, _)| *c == chord) {
                    Some((_, owner)) => taken.push((text, *owner)),
                    None => bindings.push((chord, action)),
                }
            }
            let kept = bindings.iter().any(|(_, a)| *a == action);
            for (text, owner) in taken {
                let cost = if kept {
                    "that key is left out".to_owned()
                } else {
                    format!("{} is disabled (give {} another key to use it)", action.name(), owner.name())
                };
                warnings.push(Warning::new(
                    file,
                    format!(
                        "shortcuts: the default \"{text}\" of {} is used by {}; {cost}",
                        action.name(),
                        owner.name()
                    ),
                ));
            }
        }
        Shortcuts { bindings, commands: Vec::new() }
    }

    pub fn action_for(&self, chord: &Chord) -> Option<Action> {
        self.bindings.iter().find(|(c, _)| c == chord).map(|(_, a)| *a)
    }

    /// The chord bound to `action`, if any.
    pub fn chord_for(&self, action: Action) -> Option<Chord> {
        self.bindings.iter().find(|(_, a)| *a == action).map(|(c, _)| *c)
    }

    /// Binds `chord` to command `index` (its place among the valid `[[commands]]`), unless an
    /// action (default or written) or an earlier command has it: then nothing changes and
    /// the owner is returned.
    pub fn bind_command(&mut self, index: usize, chord: Chord) -> Result<(), KeyOwner> {
        if let Some(action) = self.action_for(&chord) {
            return Err(KeyOwner::Action(action));
        }
        if let Some(other) = self.command_for(&chord) {
            return Err(KeyOwner::Command(other));
        }
        self.commands.push((chord, index));
        Ok(())
    }

    /// The command `chord` runs, by its index among the valid `[[commands]]`.
    pub fn command_for(&self, chord: &Chord) -> Option<usize> {
        self.commands.iter().find(|(c, _)| c == chord).map(|(_, i)| *i)
    }

    /// The key of command `index`, if it has one.
    pub fn command_chord(&self, index: usize) -> Option<Chord> {
        self.commands.iter().find(|(_, i)| *i == index).map(|(c, _)| *c)
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
    fn file_operation_defaults() {
        let other = Shortcuts::defaults(Platform::Other);
        assert_eq!(other.action_for(&chord("delete")), Some(Action::Trash));
        assert_eq!(other.action_for(&chord("shift+delete")), Some(Action::DeletePermanently));
        assert_eq!(other.action_for(&chord("f2")), Some(Action::Rename));
        assert_eq!(other.action_for(&chord("ctrl+shift+n")), Some(Action::NewFolder));
        assert_eq!(other.action_for(&chord("ctrl+y")), Some(Action::Redo));
        assert_eq!(other.action_for(&chord("ctrl+d")), None, "Ctrl+D deletes in Explorer: no duplicate");
        let mac = Shortcuts::defaults(Platform::Mac);
        let mac_chord = |t: &str| parse_chord(t, Platform::Mac).unwrap().unwrap();
        assert_eq!(mac.action_for(&mac_chord("mod+d")), Some(Action::Duplicate));
        assert_eq!(mac.action_for(&mac_chord("mod+backspace")), Some(Action::Trash));
        assert_eq!(mac.action_for(&mac_chord("enter")), Some(Action::Rename));
        assert_eq!(mac.action_for(&mac_chord("mod+shift+z")), Some(Action::Redo));
    }

    #[test]
    fn an_action_without_a_default_can_be_bound() {
        let (s, warnings) = build("[shortcuts]\nduplicate = \"ctrl+d\"\n");
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(s.action_for(&chord("ctrl+d")), Some(Action::Duplicate));
    }

    #[test]
    fn defaults_cover_every_action() {
        for platform in [Platform::Mac, Platform::Other] {
            let s = Shortcuts::defaults(platform);
            for action in Action::ALL {
                for text in action.default_texts(platform) {
                    let c = parse_chord(text, platform).unwrap().unwrap();
                    assert_eq!(s.action_for(&c), Some(action), "{platform:?} {} {text}", action.name());
                }
            }
        }
    }

    #[test]
    fn parses_keypad_keys_and_symbols() {
        assert_eq!(chord("num+").key, Key::Num('+'));
        assert_eq!(chord("alt+num+"), Chord { ctrl: false, alt: true, shift: false, meta: false, key: Key::Num('+') });
        assert_eq!(chord("NUM-").key, Key::Num('-'));
        assert_eq!(chord("num*").key, Key::Num('*'));
        assert_eq!(chord("num/").key, Key::Num('/'));
        assert_eq!(chord("ctrl+=").key, key('='));
        assert_eq!(chord("ctrl+-").key, key('-'));
        for bad in ["num", "num+x", "ctrl+num", "num%", "ctrlnum+", "ctrl++", "ctrl++t"] {
            assert!(parse_chord(bad, Platform::Other).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn the_view_keys_moved_for_the_tab_numbers() {
        let other = Shortcuts::defaults(Platform::Other);
        assert_eq!(other.action_for(&chord("ctrl+1")), Some(Action::Tab1));
        assert_eq!(other.action_for(&chord("ctrl+8")), Some(Action::Tab8));
        assert_eq!(other.action_for(&chord("ctrl+9")), Some(Action::TabLast));
        assert_eq!(other.action_for(&chord("ctrl+shift+1")), Some(Action::ViewList));
        assert_eq!(other.action_for(&chord("ctrl+shift+2")), Some(Action::ViewGrid));
        assert_eq!(other.action_for(&chord("ctrl+shift+t")), Some(Action::ReopenTab));
        assert_eq!(other.action_for(&chord("ctrl+shift+a")), Some(Action::TabPicker));
        assert_eq!(other.action_for(&chord("ctrl+shift+i")), Some(Action::InvertSelection));
        assert_eq!(other.action_for(&chord("ctrl+f")), Some(Action::Filter));
        assert_eq!(other.action_for(&chord("num/")), Some(Action::RestoreSelection));
        assert_eq!(other.chord_for(Action::ToggleTabLock), None);
        let mac = Shortcuts::defaults(Platform::Mac);
        let mac_chord = |t: &str| parse_chord(t, Platform::Mac).unwrap().unwrap();
        assert_eq!(mac.action_for(&mac_chord("mod+3")), Some(Action::Tab3));
        assert_eq!(mac.action_for(&mac_chord("mod+shift+1")), Some(Action::ViewList));
    }

    #[test]
    fn an_old_hand_binding_of_ctrl_1_still_works() {
        let (s, warnings) = build(
            "[shortcuts]
view-list = \"mod+1\"
",
        );
        assert_eq!(s.action_for(&chord("ctrl+1")), Some(Action::ViewList));
        assert_eq!(s.action_for(&chord("ctrl+shift+1")), None, "the user's binding replaces the default");
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(
            warnings[0].message.contains("tab-1")
                && warnings[0].message.ends_with("tab-1 is disabled (give view-list another key to use it)"),
            "{warnings:?}"
        );
    }

    /// A `[shortcuts]` table written before actions could have several keys: single texts,
    /// some shadowing today's defaults, still read as before.
    #[test]
    fn an_old_settings_file_still_reads() {
        let (s, warnings) = build(
            "[shortcuts]
new-tab = \"ctrl+n\"
view-list = \"mod+1\"
view-grid = \"mod+2\"
             refresh = \"\"
toggle-hidden = \"ctrl+h\"
duplicate = \"ctrl+d\"
",
        );
        assert_eq!(s.action_for(&chord("ctrl+n")), Some(Action::NewTab));
        assert_eq!(s.action_for(&chord("ctrl+t")), None);
        assert_eq!(s.action_for(&chord("ctrl+1")), Some(Action::ViewList));
        assert_eq!(s.action_for(&chord("ctrl+2")), Some(Action::ViewGrid));
        assert_eq!(s.action_for(&chord("f5")), None);
        assert_eq!(s.action_for(&chord("ctrl+h")), Some(Action::ToggleHidden));
        assert_eq!(s.action_for(&chord("ctrl+d")), Some(Action::Duplicate));
        assert_eq!(s.action_for(&chord("ctrl+3")), Some(Action::Tab3), "the other tab keys stay");
        assert_eq!(s.chord_for(Action::Tab1), None);
        assert_eq!(s.chord_for(Action::Tab2), None);
        let messages: Vec<&str> = warnings.iter().map(|w| w.message.as_str()).collect();
        assert_eq!(
            messages,
            [
                "shortcuts: the default \"mod+1\" of tab-1 is used by view-list; tab-1 is disabled (give view-list another key to use it)",
                "shortcuts: the default \"mod+2\" of tab-2 is used by view-grid; tab-2 is disabled (give view-grid another key to use it)",
            ]
        );
    }

    #[test]
    fn an_action_can_have_several_keys() {
        let other = Shortcuts::defaults(Platform::Other);
        assert_eq!(other.action_for(&chord("num+")), Some(Action::SelectPattern));
        assert_eq!(other.action_for(&chord("ctrl+=")), Some(Action::SelectPattern));
        assert_eq!(other.action_for(&chord("alt+num+")), Some(Action::SelectSameType));
        assert_eq!(other.chord_for(Action::SelectPattern), Some(chord("ctrl+=")), "the playable one first");
        let (s, warnings) = build(
            "[shortcuts]
select-pattern = [\"num+\", \"ctrl+shift+0\"]
deselect-pattern = \"\"
",
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(s.action_for(&chord("ctrl+shift+0")), Some(Action::SelectPattern));
        assert_eq!(s.action_for(&chord("ctrl+=")), None, "the list replaces both defaults");
        assert_eq!(s.action_for(&chord("num-")), None);
        let (_, warnings) = build(
            "[shortcuts]
filter = [\"ctrl+f\", 3]
",
        );
        assert!(warnings[0].message.starts_with("shortcuts.filter: expected text or a list of texts"), "{warnings:?}");
        let (s, warnings) = build(
            "[shortcuts]
refresh = \"num+\"
",
        );
        assert_eq!(s.action_for(&chord("num+")), Some(Action::Refresh));
        assert_eq!(s.action_for(&chord("ctrl+=")), Some(Action::SelectPattern), "its other key stays");
        assert!(warnings[0].message.ends_with("that key is left out"), "{warnings:?}");
    }

    #[test]
    fn num_plus_parses_with_spaces_around_the_separator() {
        assert_eq!(
            chord("ctrl + num+"),
            Chord { ctrl: true, alt: false, shift: false, meta: false, key: Key::Num('+') }
        );
        assert_eq!(
            chord(" Ctrl + Alt +NUM+ "),
            Chord { ctrl: true, alt: true, shift: false, meta: false, key: Key::Num('+') }
        );
        assert_eq!(
            chord("alt + num-"),
            Chord { ctrl: false, alt: true, shift: false, meta: false, key: Key::Num('-') }
        );
        assert!(parse_chord("ctrl + + num+", Platform::Other).is_err());
    }

    #[test]
    fn an_action_left_with_no_key_is_disabled() {
        // Both default keys of select-pattern are taken: it has none left.
        let (s, warnings) = build(
            "[shortcuts]
refresh = \"num+\"
up = \"ctrl+=\"
",
        );
        assert_eq!(s.chord_for(Action::SelectPattern), None);
        let messages: Vec<&str> = warnings.iter().map(|w| w.message.as_str()).collect();
        assert_eq!(
            messages,
            [
                "shortcuts: the default \"mod+=\" of select-pattern is used by up; select-pattern is disabled (give up another key to use it)",
                "shortcuts: the default \"num+\" of select-pattern is used by refresh; select-pattern is disabled (give refresh another key to use it)",
            ]
        );
        // One of two taken: only that key goes.
        let (_, warnings) = build(
            "[shortcuts]
refresh = \"num+\"
",
        );
        assert_eq!(
            warnings[0].message,
            "shortcuts: the default \"num+\" of select-pattern is used by refresh; that key is left out"
        );
        // The user's own list, all of it taken by keys written before it.
        let (s, warnings) = build(
            "[shortcuts]
up = \"ctrl+u\"
forward = \"ctrl+j\"
back = [\"ctrl+u\", \"ctrl+j\"]
",
        );
        assert_eq!(s.chord_for(Action::Back), None);
        let messages: Vec<&str> = warnings.iter().map(|w| w.message.as_str()).collect();
        assert_eq!(
            messages,
            [
                "shortcuts.back: already used by up; back is disabled",
                "shortcuts.back: already used by forward; back is disabled"
            ]
        );
        let (_, warnings) = build(
            "[shortcuts]
up = \"ctrl+u\"
back = [\"ctrl+u\", \"ctrl+j\"]
",
        );
        assert_eq!(warnings[0].message, "shortcuts.back: already used by up; that key is left out");
    }

    #[test]
    fn every_action_has_a_name_that_reads_back() {
        for action in Action::ALL {
            assert_eq!(Action::from_name(action.name()), Some(action), "{}", action.name());
        }
        assert_eq!(Action::Tab3.tab_number(), Some(3));
        assert_eq!(Action::Tab8.tab_number(), Some(8));
        assert_eq!(Action::TabLast.tab_number(), None);
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

    #[test]
    fn clear_history_has_a_name_and_no_key() {
        assert_eq!(Action::from_name("clear-history"), Some(Action::ClearHistory));
        assert_eq!(Shortcuts::defaults(Platform::Other).chord_for(Action::ClearHistory), None);
        let (s, warnings) = build(
            "[shortcuts]
clear-history = \"ctrl+shift+h\"
",
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(s.action_for(&chord("ctrl+shift+h")), Some(Action::ClearHistory));
    }

    #[test]
    fn a_command_key_never_takes_an_action_key() {
        let mut s = Shortcuts::defaults(Platform::Other);
        assert_eq!(s.bind_command(0, chord("ctrl+f")), Err(KeyOwner::Action(Action::Filter)));
        assert_eq!(s.bind_command(0, chord("ctrl+alt+x")), Ok(()));
        assert_eq!(s.bind_command(1, chord("ctrl+alt+x")), Err(KeyOwner::Command(0)));
        assert_eq!((s.command_for(&chord("ctrl+alt+x")), s.action_for(&chord("ctrl+alt+x"))), (Some(0), None));
    }
}
