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

/// Who has `chord` outside the shortcut table, if anyone: the file list's own keys (the
/// primary modifier on arrows, PgUp/PgDn, Home/End and Space; Shift+F10, the menu key) or, on
/// macOS, the menu bar's (Quit, Hide, Hide Others, Minimize), which never reach the window.
pub fn fixed_owner(chord: &Chord, platform: Platform) -> Option<&'static str> {
    let (primary, other) = match platform {
        Platform::Mac => (chord.meta, chord.ctrl),
        Platform::Other => (chord.ctrl, chord.meta),
    };
    let plain = !other && !chord.alt;
    let moves = matches!(
        chord.key,
        Key::Up | Key::Down | Key::Left | Key::Right | Key::PageUp | Key::PageDown | Key::Home | Key::End
    );
    let menu_key = chord.shift && !primary && chord.key == Key::F(10);
    if plain && (moves || menu_key || (primary && !chord.shift && chord.key == Key::Space)) {
        return Some("the file list");
    }
    let menu_bar = platform == Platform::Mac
        && primary
        && !other
        && !chord.shift
        && matches!((chord.alt, chord.key), (false, Key::Char('q' | 'h' | 'm')) | (true, Key::Char('h')));
    menu_bar.then_some("the macOS menu bar")
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

/// `[system] hotkey` (spec 9 §9.2, §13.1): the chord syntax plus `win`, `super`, `cmd` and
/// `meta` for the logo key (`meta`), `option` for Alt; `mod` is Cmd on macOS, Ctrl elsewhere.
/// `""` is off (`Ok(None)`); there is no default. A system-wide shortcut takes its key from
/// every app, so only a letter, a digit or F1-F12, with the logo key or two of Ctrl, Alt and
/// Shift (one alone is every app's Ctrl+C, Alt+F4, Alt+<menu letter>); never Ctrl+Alt (AltGr
/// types with it); not the system's own Win+L/D/E/R or Win+<digit>. On macOS ⌘ or ⌃ with ⌥ or
/// ⇧ (⌘ alone is every app's Quit, Copy, Close; macOS 15 refuses ⌥ or ⇧ alone).
/// An error is a phrase that follows the quoted text: `"ctrl+alt+e" uses Ctrl+Alt, …`.
pub fn parse_hotkey(text: &str, platform: Platform) -> Result<Option<Chord>, String> {
    let text = text.trim().to_ascii_lowercase();
    if text.is_empty() {
        return Ok(None);
    }
    let not_a_key = |name: &str| format!("ends in \"{name}\": a global shortcut's key is a letter, a digit or F1-F12");
    // "num+" ends in the separator, so it would read as a missing key.
    if text.ends_with("num+") {
        return Err(not_a_key("num+"));
    }
    let (modifiers, name) = text.rsplit_once('+').unwrap_or(("", text.as_str()));
    let name = name.trim();
    if name.is_empty() {
        return Err("has no key after \"+\"".to_owned());
    }
    let key = match Key::parse(name) {
        Some(Key::Char(c)) if c.is_ascii_alphanumeric() => Key::Char(c),
        Some(Key::F(n)) => Key::F(n),
        _ => return Err(not_a_key(name)),
    };
    let mut chord = Chord { ctrl: false, alt: false, shift: false, meta: false, key };
    for modifier in modifiers.split('+').map(str::trim).filter(|_| !modifiers.is_empty()) {
        match modifier {
            "ctrl" => chord.ctrl = true,
            "alt" | "option" => chord.alt = true,
            "shift" => chord.shift = true,
            "win" | "super" | "cmd" | "meta" => chord.meta = true,
            "mod" => match platform {
                Platform::Mac => chord.meta = true,
                Platform::Other => chord.ctrl = true,
            },
            other => return Err(format!("has an unknown modifier \"{other}\"")),
        }
    }
    if platform == Platform::Mac {
        if !(chord.meta || chord.ctrl) || !(chord.alt || chord.shift) {
            return Err("needs ⌘ (cmd) or ⌃ (ctrl) with ⌥ (alt) or ⇧ (shift) on macOS".to_owned());
        }
        let screenshot = chord.meta && chord.shift && !chord.alt && matches!(chord.key, Key::Char('3' | '4' | '5'));
        let dock = chord.meta && chord.alt && !chord.ctrl && !chord.shift && chord.key == Key::Char('d');
        if screenshot || dock {
            return Err("belongs to the system (⌘⇧3, 4 and 5 take screenshots; ⌘⌥D hides the Dock)".to_owned());
        }
        return Ok(Some(chord));
    }
    if chord.ctrl && chord.alt {
        return Err("uses Ctrl+Alt, which types AltGr characters".to_owned());
    }
    let alone = !chord.ctrl && !chord.alt && !chord.shift;
    let system_own = match chord.key {
        Key::Char(c) => c.is_ascii_digit() || (alone && matches!(c, 'l' | 'd' | 'e' | 'r')),
        _ => false,
    };
    if chord.meta && system_own {
        return Err("belongs to the system (Win+L locks; Win+D, E, R and Win+<digit> are Explorer's)".to_owned());
    }
    // Ctrl+Alt is refused above, so two of the three is Ctrl+Shift or Alt+Shift.
    if !chord.meta && !(chord.shift && (chord.ctrl || chord.alt)) {
        return Err("needs two of Ctrl, Alt and Shift (Ctrl+Shift or Alt+Shift) or the Windows/Super key".to_owned());
    }
    Ok(Some(chord))
}

/// How Gezik writes a global shortcut to settings.toml: the logo key by this system's name
/// (`win`, `cmd`, `super`), `ctrl`, `alt`, `shift`, the key (as the spec writes them).
pub fn hotkey_text(chord: &Chord, logo: &str) -> String {
    let held = [(chord.meta, logo), (chord.ctrl, "ctrl"), (chord.alt, "alt"), (chord.shift, "shift")];
    let mut parts: Vec<String> = held.iter().filter(|(on, _)| *on).map(|(_, name)| (*name).to_owned()).collect();
    parts.push(match chord.key {
        Key::F(n) => format!("f{n}"),
        Key::Char(c) => c.to_string(),
        // parse_hotkey makes no other key.
        _ => String::new(),
    });
    parts.join("+")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    NewTab,
    NewWindow,
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
    /// Miller columns (spec 10 §7); the list in search results, a flat view, the trash, This PC.
    ViewColumns,
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
    /// Opens a terminal in the focused folder, else the one shown (7a).
    OpenTerminal,
    /// The same as administrator (Windows only).
    OpenTerminalAdmin,
    /// Copies the selected items' full paths, else the folder's.
    CopyPath,
    /// Saves the open tabs as a tab set.
    SaveTabSet,
    /// Goes to the first pinned folder the sidebar shows (7b); `Pin2` … `Pin9` to the next ones.
    Pin1,
    Pin2,
    Pin3,
    Pin4,
    Pin5,
    Pin6,
    Pin7,
    Pin8,
    Pin9,
    /// Moves the selected items into a new folder next to them (7c).
    NewFolderWithSelection,
    /// Puts the selected items on the drop stack.
    AddToStack,
    /// Shows or hides the drop stack.
    ToggleStack,
    /// Opens the operations panel on its History.
    ShowHistory,
    /// Opens the search bar on the folder shown (8a).
    Search,
    /// Every file under the folder in one list; again: back to the folder.
    FlatView,
    /// A search result's folder, with it selected.
    ShowInFolder,
    /// Copies the selected results keeping their folders under the search's scope.
    CopyWithFolders,
    /// The same as a move.
    CutWithFolders,
    /// The command palette: actions, commands and view options (8b).
    CommandPalette,
    /// Quick open: places first, then the rest.
    QuickOpen,
    /// Works out the sizes of the selected folders, or of every folder shown.
    CalculateFolderSizes,
    /// Saves the search bar's search under a name (`[[searches]]`).
    SaveSearch,
    /// macOS: a Finder alias of each selected item, next to it (9a1).
    MakeAlias,
    /// Goes into the selected folder even when it is a package (an app's contents).
    ShowPackageContents,
    /// macOS: Finder's Share… for the selection (AirDrop, Mail, Messages …) (9a2).
    Share,
    /// The Recycle Bin / Trash: every bin of this user in one list (9b2).
    ShowTrash,
    /// In the trash: the selected items back where they were.
    PutBack,
    /// Deletes everything in the trash for good (asks first).
    EmptyTrash,
    /// The System Integration panel (9b3): gezik on the command line, the changes made, Undo all.
    SystemIntegration,
    /// Cloud folders: the chosen items always kept on this device (macOS: Download Now) (9b5).
    KeepOffline,
    /// Cloud folders: the chosen synced items back to online-only (macOS: Remove Download).
    FreeUpSpace,
    /// The selection's Info window (macOS, Linux); Windows: the system's Properties (9a3).
    GetInfo,
    /// Connect to Server…: a share by its address (9b6).
    ConnectToServer,
    /// Ejects the selected drive or the shown folder's; a network drive: Disconnect (9b6).
    Eject,
    /// The list in one piece, no group headers (10d).
    GroupNone,
    /// Groups by the Type column's text.
    GroupType,
    /// Groups by date modified (created when sorted by it): Today, Yesterday, …
    GroupDate,
    /// Groups by size: Empty, Tiny … Gigantic, and Folders.
    GroupSize,
    /// Closes every group of a grouped view (10d).
    CollapseGroups,
    /// Opens every group of a grouped view.
    ExpandGroups,
    /// Opens the sidebar tree down to the folder shown and gives it the keyboard (10c).
    RevealInTree,
    /// Opens or closes the second pane (10b).
    ToggleDualPane,
    /// Makes the other pane the active one (Tab, only with two panes and the list focused).
    FocusOtherPane,
    /// Copies the selection into the other pane's folder (F5 with two panes).
    CopyToOtherPane,
    /// Moves the selection into the other pane's folder (F6 with two panes).
    MoveToOtherPane,
    /// Moves the active tab into the other pane, opening it first (no key).
    MoveTabToOtherPane,
    /// Turns sync browsing on or off: steps into a sub-folder or up are made in both panes.
    SyncBrowsing,
    /// Swaps the two panes' tabs; the active pane moves with its tab.
    SwapPanes,
    /// The other pane goes to the active pane's folder (no key).
    OtherPaneSameFolder,
}

impl Action {
    pub const ALL: [Action; 101] = [
        Action::NewTab,
        Action::NewWindow,
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
        Action::ViewColumns,
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
        Action::OpenTerminal,
        Action::OpenTerminalAdmin,
        Action::CopyPath,
        Action::SaveTabSet,
        Action::Pin1,
        Action::Pin2,
        Action::Pin3,
        Action::Pin4,
        Action::Pin5,
        Action::Pin6,
        Action::Pin7,
        Action::Pin8,
        Action::Pin9,
        Action::NewFolderWithSelection,
        Action::AddToStack,
        Action::ToggleStack,
        Action::ShowHistory,
        Action::Search,
        Action::FlatView,
        Action::ShowInFolder,
        Action::CopyWithFolders,
        Action::CutWithFolders,
        Action::CommandPalette,
        Action::QuickOpen,
        Action::CalculateFolderSizes,
        Action::SaveSearch,
        Action::MakeAlias,
        Action::ShowPackageContents,
        Action::Share,
        Action::ShowTrash,
        Action::PutBack,
        Action::EmptyTrash,
        Action::SystemIntegration,
        Action::KeepOffline,
        Action::FreeUpSpace,
        Action::GetInfo,
        Action::ConnectToServer,
        Action::Eject,
        Action::GroupNone,
        Action::GroupType,
        Action::GroupDate,
        Action::GroupSize,
        Action::CollapseGroups,
        Action::ExpandGroups,
        Action::RevealInTree,
        Action::ToggleDualPane,
        Action::FocusOtherPane,
        Action::CopyToOtherPane,
        Action::MoveToOtherPane,
        Action::MoveTabToOtherPane,
        Action::SyncBrowsing,
        Action::SwapPanes,
        Action::OtherPaneSameFolder,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Action::NewTab => "new-tab",
            Action::NewWindow => "new-window",
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
            Action::ViewColumns => "view-columns",
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
            Action::OpenTerminal => "open-terminal",
            Action::OpenTerminalAdmin => "open-terminal-admin",
            Action::CopyPath => "copy-path",
            Action::SaveTabSet => "save-tab-set",
            Action::Pin1 => "pin-1",
            Action::Pin2 => "pin-2",
            Action::Pin3 => "pin-3",
            Action::Pin4 => "pin-4",
            Action::Pin5 => "pin-5",
            Action::Pin6 => "pin-6",
            Action::Pin7 => "pin-7",
            Action::Pin8 => "pin-8",
            Action::Pin9 => "pin-9",
            Action::NewFolderWithSelection => "new-folder-with-selection",
            Action::AddToStack => "add-to-stack",
            Action::ToggleStack => "toggle-stack",
            Action::ShowHistory => "show-history",
            Action::Search => "search",
            Action::FlatView => "flat-view",
            Action::ShowInFolder => "show-in-folder",
            Action::CopyWithFolders => "copy-with-folders",
            Action::CutWithFolders => "cut-with-folders",
            Action::CommandPalette => "command-palette",
            Action::QuickOpen => "quick-open",
            Action::CalculateFolderSizes => "calculate-folder-sizes",
            Action::SaveSearch => "save-search",
            Action::MakeAlias => "make-alias",
            Action::ShowPackageContents => "show-package-contents",
            Action::Share => "share",
            Action::ShowTrash => "show-trash",
            Action::PutBack => "put-back",
            Action::EmptyTrash => "empty-trash",
            Action::SystemIntegration => "system-integration",
            Action::KeepOffline => "always-keep-offline",
            Action::FreeUpSpace => "free-up-space",
            Action::GetInfo => "get-info",
            Action::ConnectToServer => "connect-to-server",
            Action::Eject => "eject",
            Action::GroupNone => "group-none",
            Action::GroupType => "group-type",
            Action::GroupDate => "group-date",
            Action::GroupSize => "group-size",
            Action::CollapseGroups => "collapse-groups",
            Action::ExpandGroups => "expand-groups",
            Action::RevealInTree => "reveal-in-tree",
            Action::ToggleDualPane => "toggle-dual-pane",
            Action::FocusOtherPane => "focus-other-pane",
            Action::CopyToOtherPane => "copy-to-other-pane",
            Action::MoveToOtherPane => "move-to-other-pane",
            Action::MoveTabToOtherPane => "move-tab-to-other-pane",
            Action::SyncBrowsing => "sync-browsing",
            Action::SwapPanes => "swap-panes",
            Action::OtherPaneSameFolder => "other-pane-same-folder",
        }
    }

    /// Its title: the macOS menu bar's item and the command palette's line.
    pub fn title(self) -> &'static str {
        match self {
            Action::NewTab => "New Tab",
            Action::NewWindow => "New Window",
            Action::CloseTab => "Close Tab",
            Action::NextTab => "Show Next Tab",
            Action::PrevTab => "Show Previous Tab",
            Action::Back => "Back",
            Action::Forward => "Forward",
            Action::Up => "Enclosing Folder",
            Action::FocusPath => "Go to Folder…",
            Action::Refresh => "Refresh",
            Action::SelectAll => "Select All",
            Action::ViewList => "View as List",
            Action::ViewGrid => "View as Grid",
            Action::ViewColumns => "View as Columns",
            Action::TogglePreview => "Show Preview",
            Action::QuickLook => "Quick Look",
            Action::Copy => "Copy",
            Action::Cut => "Cut",
            Action::Paste => "Paste",
            Action::PasteMove => "Move Item Here",
            Action::Trash => "Move to Trash",
            Action::DeletePermanently => "Delete Immediately…",
            Action::Rename => "Rename",
            Action::NewFolder => "New Folder",
            Action::Duplicate => "Duplicate",
            Action::Undo => "Undo",
            Action::Redo => "Redo",
            Action::BatchRename => "Rename Items…",
            Action::ToggleHidden => "Show Hidden Items",
            Action::Filter => "Filter…",
            Action::InvertSelection => "Invert Selection",
            Action::SelectPattern => "Select by Pattern…",
            Action::DeselectPattern => "Deselect by Pattern…",
            Action::SelectSameType => "Select Same Type",
            Action::RestoreSelection => "Restore Selection",
            Action::Tab1 => "Tab 1",
            Action::Tab2 => "Tab 2",
            Action::Tab3 => "Tab 3",
            Action::Tab4 => "Tab 4",
            Action::Tab5 => "Tab 5",
            Action::Tab6 => "Tab 6",
            Action::Tab7 => "Tab 7",
            Action::Tab8 => "Tab 8",
            Action::TabLast => "Last Tab",
            Action::ReopenTab => "Reopen Closed Tab",
            Action::TabPicker => "Show All Tabs…",
            Action::ToggleTabLock => "Lock or Unlock Tab",
            Action::ClearHistory => "Clear Folder History",
            Action::OpenTerminal => "Open Terminal",
            Action::OpenTerminalAdmin => "Open Terminal as Administrator",
            Action::CopyPath => "Copy Path",
            Action::SaveTabSet => "Save Tabs As…",
            Action::Pin1 => "Pinned 1",
            Action::Pin2 => "Pinned 2",
            Action::Pin3 => "Pinned 3",
            Action::Pin4 => "Pinned 4",
            Action::Pin5 => "Pinned 5",
            Action::Pin6 => "Pinned 6",
            Action::Pin7 => "Pinned 7",
            Action::Pin8 => "Pinned 8",
            Action::Pin9 => "Pinned 9",
            Action::NewFolderWithSelection => "New Folder with Selection",
            Action::AddToStack => "Add to Drop Stack",
            Action::ToggleStack => "Drop Stack",
            Action::ShowHistory => "Operation History",
            Action::Search => "Find…",
            Action::FlatView => "Flat View",
            Action::ShowInFolder => "Show in Folder",
            Action::CopyWithFolders => "Copy with Folders",
            Action::CutWithFolders => "Cut with Folders",
            Action::CommandPalette => "Command Palette…",
            Action::QuickOpen => "Quick Open…",
            Action::CalculateFolderSizes => "Calculate Folder Sizes",
            Action::SaveSearch => "Save Search…",
            Action::MakeAlias => "Make Alias",
            Action::ShowPackageContents => "Show Package Contents",
            Action::Share => "Share…",
            Action::ShowTrash => "Show Trash",
            Action::PutBack => "Put Back",
            Action::EmptyTrash => "Empty Trash…",
            Action::SystemIntegration => "System Integration…",
            Action::KeepOffline => {
                if cfg!(target_os = "macos") {
                    "Download Now"
                } else {
                    "Always Keep on This Device"
                }
            }
            Action::FreeUpSpace => {
                if cfg!(target_os = "macos") {
                    "Remove Download"
                } else {
                    "Free Up Space"
                }
            }
            Action::GetInfo => "Get Info",
            Action::ConnectToServer => "Connect to Server…",
            Action::Eject => "Eject",
            Action::GroupNone => "Group by None",
            Action::GroupType => "Group by Type",
            Action::GroupDate => "Group by Date",
            Action::GroupSize => "Group by Size",
            Action::CollapseGroups => "Collapse All Groups",
            Action::ExpandGroups => "Expand All Groups",
            Action::RevealInTree => "Show in Sidebar Tree",
            Action::ToggleDualPane => "Two Panes",
            Action::FocusOtherPane => "Switch to the Other Pane",
            Action::CopyToOtherPane => "Copy to the Other Pane",
            Action::MoveToOtherPane => "Move to the Other Pane",
            Action::MoveTabToOtherPane => "Move Tab to the Other Pane",
            Action::SyncBrowsing => "Sync Browsing",
            Action::SwapPanes => "Swap Panes",
            Action::OtherPaneSameFolder => "Same Folder in the Other Pane",
        }
    }

    /// The pinned folder a `pin-N` action goes to (1-based): `Pin1` → 1 … `Pin9` → 9.
    pub fn pin_number(self) -> Option<usize> {
        Some(match self {
            Action::Pin1 => 1,
            Action::Pin2 => 2,
            Action::Pin3 => 3,
            Action::Pin4 => 4,
            Action::Pin5 => 5,
            Action::Pin6 => 6,
            Action::Pin7 => 7,
            Action::Pin8 => 8,
            Action::Pin9 => 9,
            _ => return None,
        })
    }

    /// `pin-number` (1-9).
    pub fn pin(number: usize) -> Option<Action> {
        Action::ALL.into_iter().find(|action| action.pin_number() == Some(number))
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

    /// Only with two panes: its key may be another action's too, which has it with one pane
    /// (spec 10 §4.4).
    pub fn needs_dual_pane(self) -> bool {
        matches!(self, Action::FocusOtherPane | Action::CopyToOtherPane | Action::MoveToOtherPane)
    }

    /// The action named `name` in settings.toml (`new-tab`).
    pub fn from_name(name: &str) -> Option<Action> {
        Action::ALL.into_iter().find(|a| a.name() == name)
    }

    /// The default chords on `platform` (none, one or several).
    fn default_texts(self, platform: Platform) -> &'static [&'static str] {
        match (self, platform) {
            (Action::NewTab, _) => &["mod+t"],
            (Action::NewWindow, _) => &["mod+n"],
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
            // Ctrl+R as in Explorer: F5 copies while two panes are open (spec 10 §4.4).
            (Action::Refresh, Platform::Other) => &["f5", "ctrl+r"],
            (Action::SelectAll, _) => &["mod+a"],
            (Action::ViewList, _) => &["mod+shift+1"],
            (Action::ViewGrid, _) => &["mod+shift+2"],
            // ⌃⌘3: ⇧⌘3 is the macOS screenshot, ⌘3 is tab 3 (spec 10 §7.1).
            (Action::ViewColumns, Platform::Mac) => &["mod+ctrl+3"],
            (Action::ViewColumns, Platform::Other) => &["ctrl+shift+3"],
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
            // Shift+F4 is Dolphin's; Ctrl+Alt+T second: AltGr types with it on some layouts
            // (₺ on Turkish Q) and GNOME takes it system-wide (spec 10.3).
            (Action::OpenTerminal, Platform::Mac) => &["mod+alt+t"],
            (Action::OpenTerminal, Platform::Other) => &["shift+f4", "ctrl+alt+t"],
            (Action::OpenTerminalAdmin, _) => &[],
            (Action::CopyPath, Platform::Mac) => &["mod+alt+c"],
            (Action::CopyPath, Platform::Other) => &["ctrl+shift+c"],
            (Action::SaveTabSet, _) => &[],
            // Alt+digit, not Ctrl+Alt+digit: Windows takes the left Ctrl+Alt for AltGr, which
            // types with the digit keys on many layouts (Turkish Q: > £ # $ ½ { [ ]; spec 10.3).
            (Action::Pin1, Platform::Mac) => &["mod+alt+1"],
            (Action::Pin1, Platform::Other) => &["alt+1"],
            (Action::Pin2, Platform::Mac) => &["mod+alt+2"],
            (Action::Pin2, Platform::Other) => &["alt+2"],
            (Action::Pin3, Platform::Mac) => &["mod+alt+3"],
            (Action::Pin3, Platform::Other) => &["alt+3"],
            (Action::Pin4, Platform::Mac) => &["mod+alt+4"],
            (Action::Pin4, Platform::Other) => &["alt+4"],
            (Action::Pin5, Platform::Mac) => &["mod+alt+5"],
            (Action::Pin5, Platform::Other) => &["alt+5"],
            (Action::Pin6, Platform::Mac) => &["mod+alt+6"],
            (Action::Pin6, Platform::Other) => &["alt+6"],
            (Action::Pin7, Platform::Mac) => &["mod+alt+7"],
            (Action::Pin7, Platform::Other) => &["alt+7"],
            (Action::Pin8, Platform::Mac) => &["mod+alt+8"],
            (Action::Pin8, Platform::Other) => &["alt+8"],
            (Action::Pin9, Platform::Mac) => &["mod+alt+9"],
            (Action::Pin9, Platform::Other) => &["alt+9"],
            // Finder's ⌃⌘N; Ctrl+Alt+N may type with AltGr on a few layouts (Polish ń): the
            // template's comment says so (spec 10.3).
            (Action::NewFolderWithSelection, Platform::Mac) => &["mod+ctrl+n"],
            (Action::NewFolderWithSelection, Platform::Other) => &["ctrl+alt+n"],
            (Action::AddToStack, _) => &["mod+shift+s"],
            (Action::ToggleStack, _) => &[],
            (Action::ShowHistory, _) => &[],
            // F3 is Mission Control's on macOS (spec 9.3); elsewhere the second pane's since 10b,
            // Ctrl+E is Explorer's search key.
            (Action::Search, Platform::Mac) => &["mod+shift+f"],
            (Action::Search, Platform::Other) => &["mod+shift+f", "ctrl+e"],
            (Action::FlatView, _) => &["mod+b"],
            (Action::ShowInFolder, _) => &["mod+shift+e"],
            (Action::CopyWithFolders | Action::CutWithFolders, _) => &[],
            (Action::CommandPalette, _) => &["mod+shift+p"],
            (Action::QuickOpen, _) => &["mod+p"],
            (Action::CalculateFolderSizes | Action::SaveSearch, _) => &[],
            (Action::MakeAlias, Platform::Mac) => &["mod+ctrl+a"],
            (Action::MakeAlias, Platform::Other) | (Action::ShowPackageContents, _) | (Action::Share, _) => &[],
            (Action::EmptyTrash, Platform::Mac) => &["mod+shift+backspace"],
            (
                Action::ShowTrash
                | Action::PutBack
                | Action::SystemIntegration
                | Action::KeepOffline
                | Action::FreeUpSpace,
                _,
            )
            | (Action::EmptyTrash, Platform::Other) => &[],
            (Action::GetInfo, Platform::Mac) => &["mod+i"],
            (Action::GetInfo, Platform::Other) => &["alt+enter"],
            (Action::ConnectToServer, _) => &["mod+k"],
            (Action::Eject, Platform::Mac) => &["mod+e"],
            (Action::Eject, Platform::Other) => &[],
            (
                Action::GroupNone
                | Action::GroupType
                | Action::GroupDate
                | Action::GroupSize
                | Action::CollapseGroups
                | Action::ExpandGroups
                | Action::RevealInTree
                | Action::MoveTabToOtherPane
                | Action::SyncBrowsing
                | Action::OtherPaneSameFolder,
                _,
            ) => &[],
            (Action::SwapPanes, Platform::Mac) => &["mod+ctrl+u"],
            (Action::SwapPanes, Platform::Other) => &["ctrl+u"],
            (Action::ToggleDualPane, Platform::Mac) => &["mod+ctrl+p"],
            (Action::ToggleDualPane, Platform::Other) => &["f3"],
            (Action::FocusOtherPane, _) => &["tab"],
            // fn+F5 / fn+F6 on a Mac laptop's keyboard.
            (Action::CopyToOtherPane, _) => &["f5"],
            (Action::MoveToOtherPane, _) => &["f6"],
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

/// Where a key is pressed: with two panes open, a key shared by a pane action is that one's.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeyContext {
    pub dual: bool,
}

/// The action that has `chord` already, as far as `action` is concerned: one of the same kind
/// (a pane action and another may share a key, spec 10 §4.4).
fn holder(bindings: &[(Chord, Action)], chord: &Chord, action: Action) -> Option<Action> {
    bindings.iter().find(|(c, a)| c == chord && a.needs_dual_pane() == action.needs_dual_pane()).map(|(_, a)| *a)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shortcuts {
    bindings: Vec<(Chord, Action)>,
    /// The actions `[shortcuts]` names (validly), whatever keys it gave them.
    written: Vec<Action>,
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
                match holder(&bindings, chord, *action) {
                    // The same key written twice for one action is just that key.
                    Some(owner) if owner == *action => {}
                    Some(owner) => taken.push(owner),
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
                // A key written for another action is not a pane action's to share: the user
                // meant it for that one (unless it is that action's own default, `refresh = "f5"`).
                let written = || {
                    let mut owners = bindings.iter().filter(|(c, _)| *c == chord).map(|(_, a)| *a);
                    owners.find(|owner| {
                        user.iter().any(|(a, _)| a == owner)
                            && !owner
                                .default_texts(platform)
                                .iter()
                                .any(|t| parse_chord(t, platform) == Ok(Some(chord)))
                    })
                };
                match holder(&bindings, &chord, action).or_else(|| action.needs_dual_pane().then(written).flatten()) {
                    Some(owner) => taken.push((text, owner)),
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
        let written = user.iter().map(|(action, _)| *action).collect();
        Shortcuts { bindings, written, commands: Vec::new() }
    }

    /// The action `chord` runs with one pane.
    pub fn action_for(&self, chord: &Chord) -> Option<Action> {
        self.action_in(chord, KeyContext::default())
    }

    /// The action `chord` runs in `context`: with two panes a pane action first, else the
    /// other one; with one pane never a pane action.
    pub fn action_in(&self, chord: &Chord, context: KeyContext) -> Option<Action> {
        let of =
            |dual: bool| self.bindings.iter().find(|(c, a)| c == chord && a.needs_dual_pane() == dual).map(|(_, a)| *a);
        if context.dual { of(true).or_else(|| of(false)) } else { of(false) }
    }

    /// Whether `[shortcuts]` names `action` (its default keys are not in use then).
    pub fn is_written(&self, action: Action) -> bool {
        self.written.contains(&action)
    }

    /// The chord bound to `action`, if any.
    pub fn chord_for(&self, action: Action) -> Option<Chord> {
        self.bindings.iter().find(|(_, a)| *a == action).map(|(c, _)| *c)
    }

    /// Binds `chord` to command `index` (its place among the valid `[[commands]]`), unless an
    /// action (default or written) or an earlier command has it: then nothing changes and
    /// the owner is returned.
    pub fn bind_command(&mut self, index: usize, chord: Chord) -> Result<(), KeyOwner> {
        // Any action's key, a pane action's too (F6 copies with two panes).
        if let Some((_, action)) = self.bindings.iter().find(|(c, _)| *c == chord) {
            return Err(KeyOwner::Action(*action));
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
    fn pins_have_alt_and_cmd_option_digits() {
        let other = Shortcuts::defaults(Platform::Other);
        let mac = Shortcuts::defaults(Platform::Mac);
        let mac_chord = |t: &str| parse_chord(t, Platform::Mac).unwrap().unwrap();
        for n in 1..=9 {
            let action = Action::pin(n).unwrap();
            assert_eq!(action.name(), format!("pin-{n}"));
            assert_eq!(action.pin_number(), Some(n));
            assert_eq!(Action::from_name(&format!("pin-{n}")), Some(action));
            let alt = chord(&format!("alt+{n}"));
            assert_eq!(other.chord_for(action), Some(alt));
            assert_eq!(fixed_owner(&alt, Platform::Other), None);
            let cmd_option = mac_chord(&format!("mod+alt+{n}"));
            assert_eq!(mac.action_for(&cmd_option), Some(action));
            assert_eq!(fixed_owner(&cmd_option, Platform::Mac), None);
        }
        assert_eq!((Action::pin(0), Action::pin(10)), (None, None));
        assert_eq!(Action::ALL.len(), 101);
        assert_eq!(other.action_for(&chord("ctrl+1")), Some(Action::Tab1), "Ctrl+1 is still tab 1");
        assert_eq!(other.action_for(&chord("ctrl+alt+1")), None, "AltGr+1 types");
    }

    #[test]
    fn the_daily_7c_actions_have_their_keys() {
        let other = Shortcuts::defaults(Platform::Other);
        let mac = Shortcuts::defaults(Platform::Mac);
        let mac_chord = |t: &str| parse_chord(t, Platform::Mac).unwrap().unwrap();
        assert_eq!(other.action_for(&chord("ctrl+alt+n")), Some(Action::NewFolderWithSelection));
        assert_eq!(mac.action_for(&mac_chord("mod+ctrl+n")), Some(Action::NewFolderWithSelection), "Finder's ⌃⌘N");
        assert_eq!(other.action_for(&chord("ctrl+shift+s")), Some(Action::AddToStack));
        assert_eq!(mac.action_for(&mac_chord("mod+shift+s")), Some(Action::AddToStack));
        for action in [Action::ToggleStack, Action::ShowHistory] {
            assert_eq!((other.chord_for(action), mac.chord_for(action)), (None, None), "{}", action.name());
        }
        for (text, platform) in [("ctrl+alt+n", Platform::Other), ("ctrl+shift+s", Platform::Other)] {
            assert_eq!(fixed_owner(&parse_chord(text, platform).unwrap().unwrap(), platform), None, "{text}");
        }
        for (text, name) in [("mod+ctrl+n", "new-folder-with-selection"), ("mod+shift+s", "add-to-stack")] {
            assert_eq!(fixed_owner(&mac_chord(text), Platform::Mac), None, "{name}");
        }
        for name in ["new-folder-with-selection", "add-to-stack", "toggle-stack", "show-history"] {
            assert!(Action::from_name(name).is_some(), "{name}");
        }
    }

    #[test]
    fn parses_modifiers_in_any_order_and_case() {
        let c = chord("Shift+CTRL+T");
        assert_eq!(c, Chord { ctrl: true, alt: false, shift: true, meta: false, key: key('t') });
        assert_eq!(chord("ctrl+shift+t"), c);
    }

    #[test]
    fn the_file_lists_own_keys_and_the_macos_menus_are_taken() {
        let other = |text| fixed_owner(&chord(text), Platform::Other);
        let mac = |text| fixed_owner(&parse_chord(text, Platform::Mac).unwrap().unwrap(), Platform::Mac);
        for text in ["ctrl+down", "ctrl+shift+end", "ctrl+pageup", "ctrl+home", "ctrl+space", "shift+f10"] {
            assert_eq!(other(text), Some("the file list"), "{text}");
        }
        for text in ["ctrl+alt+down", "alt+home", "ctrl+shift+space", "f10", "ctrl+f10", "ctrl+q", "ctrl+h"] {
            assert_eq!(other(text), None, "{text}");
        }
        for text in ["mod+down", "mod+shift+left", "mod+space", "shift+f10"] {
            assert_eq!(mac(text), Some("the file list"), "{text}");
        }
        for text in ["mod+q", "mod+h", "mod+alt+h", "mod+m"] {
            assert_eq!(mac(text), Some("the macOS menu bar"), "{text}");
        }
        for text in ["ctrl+down", "mod+shift+q", "mod+alt+q", "ctrl+q"] {
            assert_eq!(mac(text), None, "{text}");
        }
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
            let mut warnings = Vec::new();
            let s = Shortcuts::from_table(None, platform, "settings.toml", &mut warnings);
            assert!(warnings.is_empty(), "{platform:?}: {warnings:?}");
            for action in Action::ALL {
                let context = KeyContext { dual: action.needs_dual_pane() };
                for text in action.default_texts(platform) {
                    let c = parse_chord(text, platform).unwrap().unwrap();
                    assert_eq!(s.action_in(&c, context), Some(action), "{platform:?} {} {text}", action.name());
                }
            }
        }
        // F5 in each context.
        let other = Shortcuts::defaults(Platform::Other);
        assert_eq!(other.action_in(&chord("f5"), KeyContext { dual: false }), Some(Action::Refresh));
        assert_eq!(other.action_in(&chord("f5"), KeyContext { dual: true }), Some(Action::CopyToOtherPane));
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
        assert_eq!(s.action_in(&chord("f5"), KeyContext { dual: true }), Some(Action::CopyToOtherPane));
        assert_eq!(s.action_for(&chord("ctrl+h")), Some(Action::ToggleHidden));
        assert_eq!(s.action_for(&chord("ctrl+d")), Some(Action::Duplicate));
        assert_eq!(s.action_for(&chord("ctrl+3")), Some(Action::Tab3), "the other tab keys stay");
        assert_eq!(s.chord_for(Action::Tab1), None);
        assert_eq!(s.chord_for(Action::Tab2), None);
        let messages: Vec<&str> = warnings.iter().map(|w| w.message.as_str()).collect();
        assert_eq!(
            messages,
            [
                "shortcuts: the default \"mod+n\" of new-window is used by new-tab; new-window is disabled (give new-tab another key to use it)",
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
up = \"ctrl+g\"
forward = \"ctrl+j\"
back = [\"ctrl+g\", \"ctrl+j\"]
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
up = \"ctrl+g\"
back = [\"ctrl+g\", \"ctrl+j\"]
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
        let (s, warnings) = build("[shortcuts]\nnew-tab = \"ctrl+shift+y\"\n");
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(s.action_for(&chord("ctrl+shift+y")), Some(Action::NewTab));
        assert_eq!(s.action_for(&chord("ctrl+t")), None);
    }

    #[test]
    fn empty_string_disables_an_action() {
        let (s, warnings) = build("[shortcuts]\nrefresh = \"\"\n");
        assert!(warnings.is_empty());
        assert_eq!(s.action_for(&chord("f5")), None);
        assert_eq!(s.action_in(&chord("f5"), KeyContext { dual: true }), Some(Action::CopyToOtherPane));
    }

    #[test]
    fn invalid_binding_keeps_default_with_warning() {
        let (s, warnings) = build("[shortcuts]\nrefresh = \"hyper+r\"\nback = 5\n");
        assert_eq!(s.action_for(&chord("f5")), Some(Action::Refresh));
        assert_eq!(s.action_in(&chord("f5"), KeyContext { dual: true }), Some(Action::CopyToOtherPane));
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
        let (s, warnings) = build("[shortcuts]\nup = \"ctrl+g\"\nback = \"ctrl+g\"\n");
        assert_eq!(s.action_for(&chord("ctrl+g")), Some(Action::Up));
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
    fn the_daily_actions_have_their_keys() {
        let other = Shortcuts::defaults(Platform::Other);
        assert_eq!(other.action_for(&chord("shift+f4")), Some(Action::OpenTerminal));
        assert_eq!(other.action_for(&chord("ctrl+alt+t")), Some(Action::OpenTerminal));
        assert_eq!(other.chord_for(Action::OpenTerminal), Some(chord("shift+f4")), "the one the menus show");
        assert_eq!(other.action_for(&chord("ctrl+shift+c")), Some(Action::CopyPath));
        assert_eq!(other.chord_for(Action::OpenTerminalAdmin), None);
        assert_eq!(other.chord_for(Action::SaveTabSet), None);
        let mac = Shortcuts::defaults(Platform::Mac);
        let mac_chord = |t: &str| parse_chord(t, Platform::Mac).unwrap().unwrap();
        assert_eq!(mac.action_for(&mac_chord("mod+alt+t")), Some(Action::OpenTerminal));
        assert_eq!(mac.action_for(&mac_chord("mod+alt+c")), Some(Action::CopyPath));
        for name in ["open-terminal", "open-terminal-admin", "copy-path", "save-tab-set"] {
            assert!(Action::from_name(name).is_some(), "{name}");
        }
        for platform in [Platform::Mac, Platform::Other] {
            let defaults = Shortcuts::defaults(platform);
            for action in [Action::OpenTerminal, Action::CopyPath] {
                let chord = defaults.chord_for(action).unwrap();
                assert_eq!(fixed_owner(&chord, platform), None, "{}", action.name());
            }
        }
    }

    #[test]
    fn a_command_key_never_takes_an_action_key() {
        let mut s = Shortcuts::defaults(Platform::Other);
        assert_eq!(s.bind_command(0, chord("ctrl+f")), Err(KeyOwner::Action(Action::Filter)));
        assert_eq!(s.bind_command(0, chord("ctrl+alt+x")), Ok(()));
        assert_eq!(s.bind_command(1, chord("ctrl+alt+x")), Err(KeyOwner::Command(0)));
        assert_eq!((s.command_for(&chord("ctrl+alt+x")), s.action_for(&chord("ctrl+alt+x"))), (Some(0), None));
    }

    #[test]
    fn the_search_actions_have_their_keys() {
        let other = Shortcuts::defaults(Platform::Other);
        let mac = Shortcuts::defaults(Platform::Mac);
        let mac_chord = |t: &str| parse_chord(t, Platform::Mac).unwrap().unwrap();
        assert_eq!(other.action_for(&chord("ctrl+shift+f")), Some(Action::Search));
        assert_eq!(other.action_for(&chord("f3")), Some(Action::ToggleDualPane), "F3 moved in 10b");
        assert_eq!(other.action_for(&chord("ctrl+e")), Some(Action::Search), "Explorer's search key");
        assert_eq!(mac.action_for(&mac_chord("mod+shift+f")), Some(Action::Search));
        assert_eq!(mac.action_for(&mac_chord("f3")), None, "F3 is macOS's (Mission Control)");
        assert_eq!(other.action_for(&chord("ctrl+b")), Some(Action::FlatView));
        assert_eq!(mac.action_for(&mac_chord("mod+b")), Some(Action::FlatView));
        assert_eq!(other.action_for(&chord("ctrl+shift+e")), Some(Action::ShowInFolder));
        assert_eq!(mac.action_for(&mac_chord("mod+shift+e")), Some(Action::ShowInFolder));
        for action in [Action::CopyWithFolders, Action::CutWithFolders] {
            assert_eq!((other.chord_for(action), mac.chord_for(action)), (None, None), "{}", action.name());
        }
        for text in ["ctrl+shift+f", "f3", "ctrl+e", "ctrl+b", "ctrl+shift+e"] {
            assert_eq!(fixed_owner(&chord(text), Platform::Other), None, "{text}");
        }
        for name in ["search", "flat-view", "show-in-folder", "copy-with-folders", "cut-with-folders"] {
            assert!(Action::from_name(name).is_some(), "{name}");
        }
        assert_eq!(Action::ALL.len(), 101);
    }

    #[test]
    fn the_8b_actions_have_their_keys() {
        let other = Shortcuts::defaults(Platform::Other);
        let mac = Shortcuts::defaults(Platform::Mac);
        let mac_chord = |t: &str| parse_chord(t, Platform::Mac).unwrap().unwrap();
        assert_eq!(other.action_for(&chord("ctrl+shift+p")), Some(Action::CommandPalette));
        assert_eq!(other.action_for(&chord("ctrl+p")), Some(Action::QuickOpen));
        assert_eq!(mac.action_for(&mac_chord("mod+shift+p")), Some(Action::CommandPalette));
        assert_eq!(mac.action_for(&mac_chord("mod+p")), Some(Action::QuickOpen));
        for action in [Action::CalculateFolderSizes, Action::SaveSearch] {
            assert_eq!((other.chord_for(action), mac.chord_for(action)), (None, None), "{}", action.name());
        }
        for text in ["ctrl+shift+p", "ctrl+p"] {
            assert_eq!(fixed_owner(&chord(text), Platform::Other), None, "{text}");
        }
        assert_eq!(fixed_owner(&mac_chord("mod+p"), Platform::Mac), None);
        for name in ["command-palette", "quick-open", "calculate-folder-sizes", "save-search"] {
            assert!(Action::from_name(name).is_some(), "{name}");
        }
        assert_eq!(Action::ALL.len(), 101);
    }

    #[test]
    fn the_9a1_actions_have_their_keys() {
        let other = Shortcuts::defaults(Platform::Other);
        let mac = Shortcuts::defaults(Platform::Mac);
        let mac_chord = |t: &str| parse_chord(t, Platform::Mac).unwrap().unwrap();
        assert_eq!(mac.action_for(&mac_chord("mod+ctrl+a")), Some(Action::MakeAlias), "Finder's ⌃⌘A");
        assert_eq!(fixed_owner(&mac_chord("mod+ctrl+a"), Platform::Mac), None);
        assert_eq!(other.chord_for(Action::MakeAlias), None);
        assert_eq!(
            (other.chord_for(Action::ShowPackageContents), mac.chord_for(Action::ShowPackageContents)),
            (None, None)
        );
        for name in ["make-alias", "show-package-contents"] {
            assert!(Action::from_name(name).is_some(), "{name}");
        }
        assert_eq!(Action::MakeAlias.title(), "Make Alias");
        assert_eq!(Action::ShowPackageContents.title(), "Show Package Contents");
        assert_eq!(Action::ALL.len(), 101);
    }

    #[test]
    fn the_9a2_actions_have_their_keys() {
        for platform in [Platform::Other, Platform::Mac] {
            assert_eq!(Shortcuts::defaults(platform).chord_for(Action::Share), None, "no key (spec 13.2)");
        }
        assert_eq!(Action::from_name("share"), Some(Action::Share));
        assert_eq!(Action::Share.title(), "Share…");
        assert_eq!(Action::ALL.len(), 101);
    }

    #[test]
    fn the_9a3_action_has_its_keys() {
        let chord = |t: &str, p| parse_chord(t, p).unwrap().unwrap();
        let other = Shortcuts::defaults(Platform::Other);
        let mac = Shortcuts::defaults(Platform::Mac);
        assert_eq!(other.action_for(&chord("alt+enter", Platform::Other)), Some(Action::GetInfo));
        assert_eq!(mac.action_for(&chord("mod+i", Platform::Mac)), Some(Action::GetInfo), "Finder's ⌘I");
        assert_eq!(fixed_owner(&chord("alt+enter", Platform::Other), Platform::Other), None);
        assert_eq!(fixed_owner(&chord("mod+i", Platform::Mac), Platform::Mac), None);
        assert_eq!(Action::from_name("get-info"), Some(Action::GetInfo));
        assert_eq!(Action::GetInfo.title(), "Get Info");
        assert_eq!(Action::ALL.len(), 101);
    }

    #[test]
    fn connect_to_server_has_mod_k() {
        let chord = |t: &str, p| parse_chord(t, p).unwrap().unwrap();
        for platform in [Platform::Other, Platform::Mac] {
            let keys = Shortcuts::defaults(platform);
            assert_eq!(keys.action_for(&chord("mod+k", platform)), Some(Action::ConnectToServer));
            assert_eq!(fixed_owner(&chord("mod+k", platform), platform), None);
        }
        assert_eq!(Action::from_name("connect-to-server"), Some(Action::ConnectToServer));
        assert_eq!(Action::ConnectToServer.title(), "Connect to Server…");
        assert_eq!(Action::ALL.len(), 101);
    }

    #[test]
    fn the_group_actions_have_no_keys() {
        for (action, name, title) in [
            (Action::GroupNone, "group-none", "Group by None"),
            (Action::GroupType, "group-type", "Group by Type"),
            (Action::GroupDate, "group-date", "Group by Date"),
            (Action::GroupSize, "group-size", "Group by Size"),
            (Action::CollapseGroups, "collapse-groups", "Collapse All Groups"),
            (Action::ExpandGroups, "expand-groups", "Expand All Groups"),
        ] {
            assert_eq!(Action::from_name(name), Some(action));
            assert_eq!(action.title(), title);
            for platform in [Platform::Other, Platform::Mac] {
                assert_eq!(Shortcuts::defaults(platform).chord_for(action), None, "{name}");
            }
        }
        assert_eq!(Action::ALL.len(), 101);
    }

    #[test]
    fn the_tree_action_has_no_key() {
        assert_eq!(Action::from_name("reveal-in-tree"), Some(Action::RevealInTree));
        assert_eq!(Action::RevealInTree.title(), "Show in Sidebar Tree");
        for platform in [Platform::Other, Platform::Mac] {
            assert_eq!(Shortcuts::defaults(platform).chord_for(Action::RevealInTree), None);
        }
        assert_eq!(Action::ALL.len(), 101);
    }

    #[test]
    fn the_pane_actions_have_their_keys() {
        let chord = |t: &str, p| parse_chord(t, p).unwrap().unwrap();
        let mac = Shortcuts::defaults(Platform::Mac);
        assert_eq!(mac.action_for(&chord("mod+ctrl+p", Platform::Mac)), Some(Action::ToggleDualPane), "⌃⌘P");
        assert_eq!(fixed_owner(&chord("mod+ctrl+p", Platform::Mac), Platform::Mac), None);
        let other = Shortcuts::defaults(Platform::Other);
        assert_eq!(other.action_for(&chord("f3", Platform::Other)), Some(Action::ToggleDualPane));
        let (one, two) = (KeyContext { dual: false }, KeyContext { dual: true });
        for platform in [Platform::Other, Platform::Mac] {
            let keys = Shortcuts::defaults(platform);
            let tab = chord("tab", platform);
            assert_eq!(keys.action_in(&tab, one), None, "Tab goes on with one pane");
            assert_eq!(keys.action_in(&tab, two), Some(Action::FocusOtherPane));
            let f6 = chord("f6", platform);
            assert_eq!(keys.action_in(&f6, one), None, "F6 does nothing with one pane");
            assert_eq!(keys.action_in(&f6, two), Some(Action::MoveToOtherPane));
            assert_eq!(keys.action_in(&chord("f5", platform), two), Some(Action::CopyToOtherPane));
            for text in ["tab", "f5", "f6"] {
                assert_eq!(fixed_owner(&chord(text, platform), platform), None, "{text}");
            }
        }
        assert_eq!(Shortcuts::defaults(Platform::Mac).action_in(&chord("f5", Platform::Mac), one), None);
        for text in ["f3", "ctrl+e", "ctrl+r"] {
            assert_eq!(fixed_owner(&chord(text, Platform::Other), Platform::Other), None, "{text}");
        }
        assert_eq!(Action::from_name("toggle-dual-pane"), Some(Action::ToggleDualPane));
        assert_eq!(Action::from_name("focus-other-pane"), Some(Action::FocusOtherPane));
        assert_eq!(Action::ToggleDualPane.title(), "Two Panes");
        let swap = |text: &str, p| Shortcuts::defaults(p).action_for(&chord(text, p));
        assert_eq!(swap("ctrl+u", Platform::Other), Some(Action::SwapPanes));
        assert_eq!(swap("mod+ctrl+u", Platform::Mac), Some(Action::SwapPanes), "⌃⌘U");
        assert_eq!(fixed_owner(&chord("mod+ctrl+u", Platform::Mac), Platform::Mac), None);
        assert_eq!(Action::from_name("sync-browsing"), Some(Action::SyncBrowsing));
        assert_eq!(Action::from_name("other-pane-same-folder"), Some(Action::OtherPaneSameFolder));
        assert_eq!(Action::ALL.len(), 101);
    }

    #[test]
    fn view_columns_has_its_keys_and_they_are_free() {
        let chord = |t: &str, p| parse_chord(t, p).unwrap().unwrap();
        for (text, platform) in [("ctrl+shift+3", Platform::Other), ("mod+ctrl+3", Platform::Mac)] {
            let c = chord(text, platform);
            assert_eq!(Shortcuts::defaults(platform).action_for(&c), Some(Action::ViewColumns), "{text}");
            assert_eq!(fixed_owner(&c, platform), None, "{text}");
        }
        // ⇧⌘3 is the macOS screenshot; ⌘3 stays tab 3.
        let mac = Shortcuts::defaults(Platform::Mac);
        assert_eq!(mac.action_for(&chord("mod+shift+3", Platform::Mac)), None);
        assert_eq!(mac.action_for(&chord("mod+3", Platform::Mac)), Some(Action::Tab3));
        assert_eq!(Action::from_name("view-columns"), Some(Action::ViewColumns));
        assert_eq!(Action::ViewColumns.title(), "View as Columns");
        assert_eq!(Action::ALL.len(), 101);
    }

    #[test]
    fn a_pane_action_shares_a_key_with_another_and_only_with_one() {
        let two = KeyContext { dual: true };
        // A pane action on another action's key: no warning, each has it in its context.
        let (s, warnings) = build("[shortcuts]\nmove-to-other-pane = \"ctrl+t\"\n");
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(s.action_for(&chord("ctrl+t")), Some(Action::NewTab));
        assert_eq!(s.action_in(&chord("ctrl+t"), two), Some(Action::MoveToOtherPane));
        assert_eq!(s.action_in(&chord("ctrl+n"), two), Some(Action::NewWindow), "the others stay with two panes");
        // Two pane actions on one key: the second loses it.
        let (s, warnings) = build("[shortcuts]\ncopy-to-other-pane = \"f6\"\n");
        assert_eq!(s.action_in(&chord("f6"), two), Some(Action::CopyToOtherPane));
        assert_eq!(s.chord_for(Action::MoveToOtherPane), None);
        assert_eq!(
            warnings[0].message,
            "shortcuts: the default \"f6\" of move-to-other-pane is used by copy-to-other-pane; move-to-other-pane is disabled (give copy-to-other-pane another key to use it)"
        );
        // Search written on F3: the user's choice wins, the pane toggle says so (spec 10 §10.2).
        let (s, warnings) = build("[shortcuts]\nsearch = \"f3\"\n");
        assert_eq!(s.action_in(&chord("f3"), two), Some(Action::Search));
        assert!(s.is_written(Action::Search) && !s.is_written(Action::ToggleDualPane));
        assert!(
            warnings[0].message.starts_with("shortcuts: the default \"f3\" of toggle-dual-pane is used by search"),
            "{warnings:?}"
        );
    }

    #[test]
    fn a_key_written_for_another_action_is_not_a_pane_actions() {
        let two = KeyContext { dual: true };
        let (s, warnings) = build(
            "[shortcuts]
rename = \"f6\"
",
        );
        assert_eq!(s.action_in(&chord("f6"), two), Some(Action::Rename), "the user's F6 stays with two panes");
        assert_eq!(s.chord_for(Action::MoveToOtherPane), None);
        assert_eq!(
            warnings[0].message,
            "shortcuts: the default \"f6\" of move-to-other-pane is used by rename; move-to-other-pane is disabled (give rename another key to use it)"
        );
        // Refresh written with its own default keys (the template's line): F5 still copies.
        let (s, warnings) = build(
            "[shortcuts]
refresh = [\"f5\", \"ctrl+r\"]
",
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(s.action_in(&chord("f5"), two), Some(Action::CopyToOtherPane));
    }

    #[test]
    fn ctrl_r_refreshes_with_one_pane_and_two() {
        let other = Shortcuts::defaults(Platform::Other);
        for dual in [false, true] {
            assert_eq!(other.action_in(&chord("ctrl+r"), KeyContext { dual }), Some(Action::Refresh));
        }
        assert_eq!(other.chord_for(Action::Refresh), Some(chord("f5")), "the one the menus show");
    }

    #[test]
    fn tab_and_the_pane_keys_are_no_command_keys() {
        let mut s = Shortcuts::defaults(Platform::Other);
        assert!(!chord("tab").leaves_typing_alone());
        assert_eq!(s.bind_command(0, chord("tab")), Err(KeyOwner::Action(Action::FocusOtherPane)));
        assert_eq!(s.bind_command(0, chord("f6")), Err(KeyOwner::Action(Action::MoveToOtherPane)));
        assert_eq!(s.bind_command(0, chord("f5")), Err(KeyOwner::Action(Action::Refresh)));
    }

    #[test]
    fn eject_has_mod_e_on_macos_only() {
        let mac_chord = parse_chord("mod+e", Platform::Mac).unwrap().unwrap();
        assert_eq!(Shortcuts::defaults(Platform::Mac).action_for(&mac_chord), Some(Action::Eject));
        assert_eq!(fixed_owner(&mac_chord, Platform::Mac), None);
        assert_eq!(Shortcuts::defaults(Platform::Other).chord_for(Action::Eject), None);
        assert_eq!(Action::from_name("eject"), Some(Action::Eject));
        assert_eq!(Action::Eject.title(), "Eject");
        assert_eq!(Action::ALL.len(), 101);
    }

    #[test]
    fn trash_actions_and_their_keys() {
        assert_eq!(Action::from_name("show-trash"), Some(Action::ShowTrash));
        assert_eq!(Action::from_name("put-back"), Some(Action::PutBack));
        assert_eq!(Action::from_name("empty-trash"), Some(Action::EmptyTrash));
        let mac = Shortcuts::defaults(Platform::Mac);
        let mac_chord = |t: &str| parse_chord(t, Platform::Mac).unwrap().unwrap();
        assert_eq!(mac.action_for(&mac_chord("mod+shift+backspace")), Some(Action::EmptyTrash));
        assert_eq!(fixed_owner(&mac_chord("mod+shift+backspace"), Platform::Mac), None);
        let other = Shortcuts::defaults(Platform::Other);
        for action in [Action::ShowTrash, Action::PutBack, Action::EmptyTrash] {
            assert_eq!(other.chord_for(action), None, "{action:?}: no default key on Windows and Linux");
        }
        assert_eq!((mac.chord_for(Action::ShowTrash), mac.chord_for(Action::PutBack)), (None, None));
    }

    #[test]
    fn the_cloud_actions_have_names_and_no_keys() {
        assert_eq!(Action::from_name("always-keep-offline"), Some(Action::KeepOffline));
        assert_eq!(Action::from_name("free-up-space"), Some(Action::FreeUpSpace));
        let mac = cfg!(target_os = "macos");
        assert_eq!(Action::KeepOffline.title(), if mac { "Download Now" } else { "Always Keep on This Device" });
        assert_eq!(Action::FreeUpSpace.title(), if mac { "Remove Download" } else { "Free Up Space" });
        for platform in [Platform::Mac, Platform::Other] {
            let keys = Shortcuts::defaults(platform);
            assert_eq!((keys.chord_for(Action::KeepOffline), keys.chord_for(Action::FreeUpSpace)), (None, None));
        }
    }

    #[test]
    fn system_integration_has_no_default_key() {
        assert_eq!(Action::from_name("system-integration"), Some(Action::SystemIntegration));
        assert_eq!(Action::SystemIntegration.title(), "System Integration…");
        for platform in [Platform::Mac, Platform::Other] {
            assert_eq!(Shortcuts::defaults(platform).chord_for(Action::SystemIntegration), None);
        }
    }

    #[test]
    fn every_action_has_a_title() {
        let mut seen = std::collections::HashSet::new();
        for action in Action::ALL {
            let title = action.title();
            assert!(!title.is_empty() && title.chars().next().unwrap().is_uppercase(), "{}", action.name());
            assert!(seen.insert(title), "\"{title}\" twice");
        }
        assert_eq!(Action::Search.title(), "Find…");
        assert_eq!(Action::CommandPalette.title(), "Command Palette…");
    }

    #[test]
    fn hotkey_rules() {
        let win = |text: &str| parse_hotkey(text, Platform::Other);
        let mac = |text: &str| parse_hotkey(text, Platform::Mac);
        let chord = |ctrl, alt, shift, meta, key| Some(Chord { ctrl, alt, shift, meta, key });
        assert_eq!(win(""), Ok(None), "empty is off");
        assert_eq!(win("  "), Ok(None));
        assert_eq!(win("win+shift+e"), Ok(chord(false, false, true, true, Key::Char('e'))));
        assert_eq!(win("Super+Shift+E"), Ok(chord(false, false, true, true, Key::Char('e'))), "any case");
        assert_eq!(
            win("cmd+alt+e"),
            Ok(chord(false, true, false, true, Key::Char('e'))),
            "cmd is the logo key everywhere"
        );
        assert_eq!(win("meta + alt + k"), Ok(chord(false, true, false, true, Key::Char('k'))), "spaces around +");
        assert_eq!(win("win+g"), Ok(chord(false, false, false, true, Key::Char('g'))), "the logo key alone is enough");
        assert_eq!(win("win+f9"), Ok(chord(false, false, false, true, Key::F(9))));
        assert_eq!(win("ctrl+shift+f12"), Ok(chord(true, false, true, false, Key::F(12))));
        assert_eq!(win("alt+shift+7"), Ok(chord(false, true, true, false, Key::Char('7'))));
        assert_eq!(win("mod+shift+g"), Ok(chord(true, false, true, false, Key::Char('g'))), "mod is Ctrl off macOS");
        assert_eq!(mac("mod+alt+e"), Ok(chord(false, true, false, true, Key::Char('e'))), "mod is Cmd on macOS");
        assert_eq!(mac("option+cmd+e"), Ok(chord(false, true, false, true, Key::Char('e'))));
        assert!(mac("cmd+shift+f5").is_ok());
        assert!(mac("ctrl+shift+e").is_ok(), "⌃⇧ is enough on macOS");
        assert!(mac("ctrl+alt+e").is_ok(), "⌃⌥ is no AltGr on macOS");
        let refused = |result: Result<Option<Chord>, String>, why: &str| {
            let err = result.unwrap_err();
            assert!(err.contains(why), "{err} should say {why}");
        };
        refused(win("ctrl+alt+e"), "uses Ctrl+Alt, which types AltGr characters");
        refused(win("ctrl+alt+shift+f11"), "AltGr");
        refused(win("win+ctrl+alt+k"), "AltGr");
        // One modifier or none: every app's own keys (copy, paste, close, menu mnemonics).
        let two = "needs two of Ctrl, Alt and Shift (Ctrl+Shift or Alt+Shift) or the Windows/Super key";
        for text in ["e", "shift+e", "f12", "ctrl+c", "ctrl+v", "ctrl+x", "ctrl+z", "ctrl+y", "ctrl+a", "ctrl+s"] {
            refused(win(text), two);
        }
        for text in ["ctrl+w", "alt+f4", "alt+f", "ctrl+f5", "shift+f10"] {
            refused(win(text), two);
        }
        // The system's own logo-key shortcuts.
        let own = "belongs to the system";
        for text in ["win+l", "win+d", "win+e", "win+r", "win+1", "win+shift+3", "super+ctrl+0"] {
            refused(win(text), own);
        }
        for text in ["cmd+shift+3", "cmd+shift+4", "cmd+shift+5", "ctrl+cmd+shift+4", "cmd+alt+d"] {
            refused(mac(text), own);
        }
        assert!(mac("cmd+shift+6").is_ok() && mac("cmd+alt+shift+d").is_ok());
        refused(win("win+space"), "letter, a digit or F1-F12");
        refused(win("win+num+"), "letter, a digit or F1-F12");
        refused(win("win+["), "letter, a digit or F1-F12");
        refused(win("win+left"), "letter, a digit or F1-F12");
        refused(win("hyper+e"), "unknown modifier \"hyper\"");
        refused(win("win+"), "no key after");
        let mac_rule = "needs ⌘ (cmd) or ⌃ (ctrl) with ⌥ (alt) or ⇧ (shift) on macOS";
        for text in
            ["cmd+q", "cmd+c", "cmd+w", "cmd+h", "cmd+e", "cmd+f5", "ctrl+e", "alt+e", "alt+shift+e", "cmd+ctrl+e"]
        {
            refused(mac(text), mac_rule);
        }
    }

    #[test]
    fn hotkey_text_reads_back() {
        for (text, logo) in
            [("win+shift+e", "win"), ("ctrl+shift+f12", "win"), ("cmd+alt+e", "cmd"), ("super+shift+k", "super")]
        {
            let chord = parse_hotkey(text, Platform::Other).unwrap().unwrap();
            let written = hotkey_text(&chord, logo);
            assert_eq!(written, text, "canonical order: logo, ctrl, alt, shift, key");
            assert_eq!(parse_hotkey(&written, Platform::Other).unwrap(), Some(chord));
        }
        let chord = parse_hotkey("win+shift+e", Platform::Other).unwrap().unwrap();
        assert_eq!(hotkey_text(&chord, "win"), "win+shift+e", "the modifiers in Gezik's order");
        let chord = parse_hotkey("shift+alt+win+f3", Platform::Other).unwrap().unwrap();
        assert_eq!(hotkey_text(&chord, "super"), "super+alt+shift+f3");
        assert!(parse_hotkey("e+shift+win", Platform::Other).is_err(), "the key comes last");
    }
}
