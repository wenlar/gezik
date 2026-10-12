//! The shortcuts on the settings window's Keyboard page (spec 14 §5): one row per action, per
//! `[[commands]]` entry with a name, and on macOS the menu bar's keys that cannot change. A key
//! is set by pressing it; one another action or command has is asked about first. Only the
//! changed action is written to `[shortcuts]`, a command's key to its own entry.

use gezik_config::settings::Settings;
use gezik_config::settings_writer::SettingsChange;
use gezik_config::shortcuts::{
    Action, Chord, Key, KeyOwner, MENU_BAR_KEYS, Platform, chord_text, parse_chord, refused,
};

use crate::SettingItem;
use crate::keys::chord_label;

/// Whose keys a row shows.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Owner {
    Action(Action),
    /// A `[[commands]]` entry, by its index among the valid ones.
    Command(usize, String),
    /// The macOS menu bar's (`MENU_BAR_KEYS`).
    Fixed,
}

#[derive(Debug, Clone)]
pub(super) struct KeyRow {
    pub owner: Owner,
    pub title: String,
    /// `shortcuts.new-tab`: Copy Key's text.
    pub path: String,
    pub keys: Vec<Chord>,
    /// Not written in settings.toml.
    pub is_default: bool,
    /// The default keys as shown.
    pub default: String,
    /// What the search looks in: the title, the name, the keys as shown and as written.
    haystack: String,
}

/// What a key pressed while a row waits for one does.
#[derive(Debug, PartialEq)]
pub(super) enum Pressed {
    /// Keep waiting (a modifier alone, a key the table cannot name), with a word for the status line.
    Wait(String),
    /// Stop waiting and write nothing (Esc, the key the row has already).
    Done(String),
    Write(Vec<SettingsChange>),
    /// The key is another's: the status line's question, and what Replace writes.
    Ask(String, Vec<SettingsChange>),
}

/// The rows: the actions by title, the commands, the menu bar's keys (macOS).
pub(super) fn rows(settings: &Settings, platform: Platform) -> Vec<KeyRow> {
    let shortcuts = &settings.shortcuts;
    let mut actions: Vec<Action> = Action::ALL.to_vec();
    actions.sort_by_key(|a| a.title());
    let mut rows: Vec<KeyRow> = actions
        .into_iter()
        .map(|action| {
            let defaults: Vec<Chord> =
                action.default_texts(platform).iter().filter_map(|t| parse_chord(t, platform).ok().flatten()).collect();
            let row = KeyRow {
                owner: Owner::Action(action),
                title: action.title().to_owned(),
                path: format!("shortcuts.{}", action.name()),
                keys: shortcuts.chords_for(action),
                is_default: !shortcuts.is_written(action),
                default: labels(&defaults, platform),
                haystack: String::new(),
            };
            with_haystack(row, action.name(), platform)
        })
        .collect();
    for (index, command) in settings.commands.iter().enumerate() {
        let row = KeyRow {
            owner: Owner::Command(index, command.name.clone()),
            title: format!("Command: {}", command.name),
            path: "commands.shortcut".to_owned(),
            keys: shortcuts.command_chord(index).into_iter().collect(),
            is_default: command.shortcut.is_none(),
            default: labels(&[], platform),
            haystack: String::new(),
        };
        rows.push(with_haystack(row, "", platform));
    }
    if platform == Platform::Mac {
        for (title, text) in MENU_BAR_KEYS {
            let keys: Vec<Chord> = parse_chord(text, platform).ok().flatten().into_iter().collect();
            let row = KeyRow {
                owner: Owner::Fixed,
                title: title.to_owned(),
                path: String::new(),
                default: labels(&keys, platform),
                keys,
                is_default: true,
                haystack: String::new(),
            };
            rows.push(with_haystack(row, "", platform));
        }
    }
    rows
}

fn with_haystack(mut row: KeyRow, name: &str, platform: Platform) -> KeyRow {
    // "cmd+shift" on a Mac, "ctrl+shift" elsewhere, finds the keys written with `mod`.
    let primary = if platform == Platform::Mac { "cmd+" } else { "ctrl+" };
    let written: Vec<String> =
        row.keys.iter().filter_map(|c| chord_text(c, platform)).map(|t| t.replace("mod+", primary)).collect();
    row.haystack = format!("{} {name} {} {}", row.title, labels(&row.keys, platform), written.join(" ")).to_lowercase();
    row
}

/// Whether the search `query` finds `row` (every word in it).
pub(super) fn matches(row: &KeyRow, query: &str) -> bool {
    query.to_lowercase().split_whitespace().all(|word| row.haystack.contains(word))
}

/// The keys as a menu shows them, or "None".
fn labels(keys: &[Chord], platform: Platform) -> String {
    if keys.is_empty() {
        return "None".to_owned();
    }
    keys.iter().map(|c| chord_label(c, platform)).collect::<Vec<_>>().join(", ")
}

pub(super) fn item(row: &KeyRow, platform: Platform) -> SettingItem {
    let fixed = row.owner == Owner::Fixed;
    SettingItem {
        kind: if fixed { 6 } else { 5 },
        title: row.title.as_str().into(),
        key: row.path.as_str().into(),
        description: if fixed { "macOS keeps this key".into() } else { Default::default() },
        value: labels(&row.keys, platform).into(),
        is_default: row.is_default,
        default: row.default.as_str().into(),
        options: Default::default(),
        place: 0,
    }
}

/// `chord` pressed while `row` waits for a key (`None`: one the table has no name for, or a
/// modifier alone; `modifier`: it was a modifier). `text` is settings.toml's.
pub(super) fn pressed(
    row: &KeyRow,
    chord: Option<Chord>,
    modifier: bool,
    settings: &Settings,
    text: &str,
    platform: Platform,
) -> Pressed {
    let to = match &row.owner {
        Owner::Action(action) => KeyOwner::Action(*action),
        Owner::Command(index, _) => KeyOwner::Command(*index),
        Owner::Fixed => return Pressed::Done(String::new()),
    };
    let Some(chord) = chord else {
        return Pressed::Wait(if modifier {
            String::new()
        } else {
            "Gezik's shortcuts cannot use that key".to_owned()
        });
    };
    let alone = !(chord.ctrl || chord.alt || chord.shift || chord.meta);
    if alone && chord.key == Key::Escape {
        return Pressed::Done(String::new());
    }
    if alone && chord.key == Key::Backspace {
        return Pressed::Write(change(&row.owner, &[], platform).into_iter().collect());
    }
    if let Some(why) = refused(&chord, matches!(to, KeyOwner::Command(_)), platform) {
        return Pressed::Wait(why);
    }
    if row.keys == [chord] {
        return Pressed::Done(format!("{} is already {}'s key", chord_label(&chord, platform), row.title));
    }
    claim(to, &[chord], settings, text, platform, change(&row.owner, &[chord], platform))
}

/// The owner `to` takes `chords` (its change is `write`): written at once, or a Replace?
/// question when another action or command has one of them.
fn claim(
    to: KeyOwner,
    chords: &[Chord],
    settings: &Settings,
    text: &str,
    platform: Platform,
    write: Option<SettingsChange>,
) -> Pressed {
    // Each owner once, keeping only what it keeps of every chord.
    let mut lost: Vec<(KeyOwner, Vec<Chord>)> = Vec::new();
    for chord in chords {
        for (owner, keeps) in settings.shortcuts.taken_by(text, to, *chord, platform) {
            match lost.iter_mut().find(|(o, _)| *o == owner) {
                Some((_, kept)) => kept.retain(|c| keeps.contains(c)),
                None => lost.push((owner, keeps)),
            }
        }
    }
    let mut changes: Vec<SettingsChange> = Vec::new();
    let mut owners: Vec<String> = Vec::new();
    for (owner, keeps) in &lost {
        let owner = match owner {
            KeyOwner::Action(action) => Owner::Action(*action),
            KeyOwner::Command(index) => match settings.commands.get(*index) {
                Some(command) => Owner::Command(*index, command.name.clone()),
                None => continue,
            },
        };
        owners.push(match &owner {
            Owner::Command(_, name) => format!("Command: {name}"),
            Owner::Action(action) => action.title().to_owned(),
            Owner::Fixed => String::new(),
        });
        changes.extend(change(&owner, keeps, platform));
    }
    changes.extend(write);
    if owners.is_empty() {
        return Pressed::Write(changes);
    }
    let keys: Vec<String> = chords.iter().map(|c| chord_label(c, platform)).collect();
    let used = if keys.len() == 1 { "is" } else { "are" };
    let question = format!("{} {used} used by {} — Replace?", keys.join(", "), owners.join(" and "));
    Pressed::Ask(question, changes)
}

/// What Reset to Default does for `row` (another's default key is asked about, as Set Key
/// does), or why it does nothing.
pub(super) fn reset(
    row: &KeyRow,
    settings: &Settings,
    text: &str,
    platform: Platform,
) -> Result<Pressed, &'static str> {
    match &row.owner {
        Owner::Fixed => Err("macOS keeps this key"),
        _ if row.is_default => Err("It has its default key"),
        Owner::Action(action) => {
            let defaults: Vec<Chord> =
                action.default_texts(platform).iter().filter_map(|t| parse_chord(t, platform).ok().flatten()).collect();
            let write = SettingsChange::Value {
                table: Some("shortcuts".to_owned()),
                key: action.name().to_owned(),
                value: Some(literal(action.default_texts(platform).iter().map(|t| (*t).to_owned()).collect())),
            };
            Ok(claim(KeyOwner::Action(*action), &defaults, settings, text, platform, Some(write)))
        }
        Owner::Command(..) => Ok(Pressed::Write(change(&row.owner, &[], platform).into_iter().collect())),
    }
}

/// Gives `owner` `keys` (none: no key); the menu bar's keys never change.
fn change(owner: &Owner, keys: &[Chord], platform: Platform) -> Option<SettingsChange> {
    let texts: Vec<String> = keys.iter().filter_map(|c| chord_text(c, platform)).collect();
    Some(match owner {
        Owner::Command(_, name) => {
            SettingsChange::CommandShortcut { name: name.clone(), shortcut: texts.first().cloned().unwrap_or_default() }
        }
        Owner::Action(action) => SettingsChange::Value {
            table: Some("shortcuts".to_owned()),
            key: action.name().to_owned(),
            value: Some(literal(texts)),
        },
        Owner::Fixed => return None,
    })
}

/// Keys as TOML, the way the template writes them: `""` (none), `"mod+t"`, `["f5", "ctrl+r"]`.
/// Key texts have no quote or backslash.
fn literal(texts: Vec<String>) -> String {
    match texts.as_slice() {
        [] => "\"\"".to_owned(),
        [one] => format!("\"{one}\""),
        many => format!("[{}]", many.iter().map(|t| format!("\"{t}\"")).collect::<Vec<_>>().join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gezik_config::shortcuts::Shortcuts;

    const OTHER: Platform = Platform::Other;

    /// Read with Windows' and Linux's keys on every host: `parse` reads `mod` as Cmd on a Mac.
    fn settings(text: &str) -> Settings {
        let mut settings = Settings::parse("settings.toml", text, &mut Vec::new());
        let table: toml::Table = text.parse().unwrap();
        let shortcuts = table.get("shortcuts").and_then(toml::Value::as_table);
        settings.shortcuts = Shortcuts::from_table(shortcuts, OTHER, "settings.toml", &mut Vec::new());
        for (index, command) in settings.commands.iter().enumerate() {
            if let Some(text) = &command.shortcut {
                settings.shortcuts.bind_command(index, chord(text)).unwrap();
            }
        }
        settings
    }

    fn chord(text: &str) -> Chord {
        parse_chord(text, OTHER).unwrap().unwrap()
    }

    fn row(rows: &[KeyRow], title: &str) -> KeyRow {
        rows.iter().find(|r| r.title == title).unwrap().clone()
    }

    fn set(action: &str, value: &str) -> SettingsChange {
        SettingsChange::Value { table: Some("shortcuts".into()), key: action.into(), value: Some(value.into()) }
    }

    #[test]
    fn rows_show_every_action_and_command() {
        let text = "[shortcuts]\ncopy-path = \"ctrl+k\"\n\n[[commands]]\nname = \"Shrink\"\nrun = [\"magick\"]\nshortcut = \"ctrl+alt+s\"\n";
        let s = settings(text);
        let rows = rows(&s, OTHER);
        assert_eq!(rows.len(), Action::ALL.len() + 1, "no menu bar rows off macOS");
        let copy = row(&rows, "Copy Path");
        assert_eq!(
            (copy.is_default, copy.keys.as_slice(), copy.default.as_str()),
            (false, &[chord("ctrl+k")][..], "Ctrl+Shift+C")
        );
        let new_tab = item(&row(&rows, "New Tab"), OTHER);
        assert_eq!((new_tab.kind, new_tab.value.as_str(), new_tab.is_default), (5, "Ctrl+T", true));
        let command = row(&rows, "Command: Shrink");
        assert_eq!((command.keys.as_slice(), command.is_default), (&[chord("ctrl+alt+s")][..], false));
        assert!(matches(&row(&rows, "Refresh"), "ctrl+r"), "by a key");
        assert!(matches(&copy, "path ctrl+k"));
        assert!(!matches(&copy, "ctrl+shift"));
        let ctrl_shift: Vec<&str> =
            rows.iter().filter(|r| matches(r, "ctrl+shift")).map(|r| r.title.as_str()).collect();
        assert!(ctrl_shift.contains(&"New Folder") && !ctrl_shift.contains(&"Copy Path"), "{ctrl_shift:?}");
    }

    #[test]
    fn macos_shows_its_symbols_and_the_menu_bar_keys() {
        let mac = Settings { shortcuts: Shortcuts::defaults(Platform::Mac), ..Settings::default() };
        let rows = rows(&mac, Platform::Mac);
        let quit = rows.iter().find(|r| r.title == "Quit Gezik").unwrap();
        let shown = item(quit, Platform::Mac);
        assert_eq!((shown.kind, shown.value.as_str()), (6, "⌘Q"));
        assert_eq!(pressed(quit, Some(chord("f2")), false, &mac, "", Platform::Mac), Pressed::Done(String::new()));
        assert!(reset(quit, &mac, "", Platform::Mac).is_err());
        let copy = rows.iter().find(|r| r.title == "Copy Path").unwrap();
        assert_eq!(item(copy, Platform::Mac).value.as_str(), "⌥⌘C");
        assert!(matches(copy, "cmd+alt"));
    }

    #[test]
    fn a_pressed_key_is_waited_for_written_or_asked_about() {
        let s = settings("");
        let rows = rows(&s, OTHER);
        let copy = row(&rows, "Copy Path");
        let press = |c: Option<Chord>, modifier| pressed(&copy, c, modifier, &s, "", OTHER);
        assert_eq!(press(None, true), Pressed::Wait(String::new()), "a modifier alone: waits");
        assert!(matches!(press(None, false), Pressed::Wait(w) if !w.is_empty()), "AltGr's character: refused");
        assert_eq!(press(Some(chord("escape")), false), Pressed::Done(String::new()));
        assert_eq!(press(Some(chord("backspace")), false), Pressed::Write(vec![set("copy-path", "\"\"")]));
        assert!(matches!(press(Some(chord("k")), false), Pressed::Wait(w) if w.contains("types")));
        assert!(matches!(press(Some(chord("ctrl+up")), false), Pressed::Wait(w) if w.contains("file list")));
        assert!(matches!(press(Some(chord("ctrl+shift+c")), false), Pressed::Done(_)), "its key already");
        assert_eq!(press(Some(chord("ctrl+alt+k")), false), Pressed::Write(vec![set("copy-path", "\"mod+alt+k\"")]));
        assert_eq!(
            press(Some(chord("ctrl+w")), false),
            Pressed::Ask(
                "Ctrl+W is used by Close Tab — Replace?".to_owned(),
                vec![set("close-tab", "\"\""), set("copy-path", "\"mod+w\"")]
            )
        );
        assert_eq!(
            pressed(&row(&rows, "New Tab"), Some(chord("f5")), false, &s, "", OTHER),
            Pressed::Ask(
                "F5 is used by Refresh and Copy to the Other Pane — Replace?".to_owned(),
                vec![set("refresh", "\"mod+r\""), set("copy-to-other-pane", "\"\""), set("new-tab", "\"f5\"")]
            )
        );
    }

    #[test]
    fn a_commands_key_goes_to_its_entry() {
        let text = "[[commands]]\nname = \"Shrink\"\nrun = [\"magick\"]\nshortcut = \"ctrl+alt+s\"\n";
        let s = settings(text);
        let rows = rows(&s, OTHER);
        let command = row(&rows, "Command: Shrink");
        let shortcut =
            |shortcut: &str| SettingsChange::CommandShortcut { name: "Shrink".into(), shortcut: shortcut.into() };
        assert_eq!(pressed(&command, Some(chord("f9")), false, &s, text, OTHER), Pressed::Write(vec![shortcut("f9")]));
        assert!(matches!(pressed(&command, Some(chord("delete")), false, &s, text, OTHER), Pressed::Wait(_)));
        assert_eq!(
            pressed(&command, Some(chord("ctrl+t")), false, &s, text, OTHER),
            Pressed::Ask(
                "Ctrl+T is used by New Tab — Replace?".to_owned(),
                vec![set("new-tab", "\"\""), shortcut("mod+t")]
            )
        );
        assert_eq!(
            pressed(&row(&rows, "New Tab"), Some(chord("ctrl+alt+s")), false, &s, text, OTHER),
            Pressed::Ask(
                "Ctrl+Alt+S is used by Command: Shrink — Replace?".to_owned(),
                vec![shortcut(""), set("new-tab", "\"mod+alt+s\"")]
            )
        );
        assert_eq!(reset(&command, &s, text, OTHER), Ok(Pressed::Write(vec![shortcut("")])));
    }

    #[test]
    fn reset_writes_the_defaults_in_place() {
        let s = settings("[shortcuts]\nrefresh = \"f9\"\n");
        let rows = rows(&s, OTHER);
        let text = "[shortcuts]\nrefresh = \"f9\"\n";
        assert_eq!(
            reset(&row(&rows, "Refresh"), &s, text, OTHER),
            Ok(Pressed::Write(vec![set("refresh", "[\"f5\", \"ctrl+r\"]")]))
        );
        assert!(reset(&row(&rows, "New Tab"), &s, text, OTHER).is_err(), "not written: nothing to do");
    }

    #[test]
    fn reset_asks_when_another_has_the_default_key() {
        let text = "[shortcuts]\nnew-tab = \"mod+w\"\nclose-tab = \"\"\n";
        let s = settings(text);
        let rows = rows(&s, OTHER);
        assert_eq!(
            reset(&row(&rows, "Close Tab"), &s, text, OTHER),
            Ok(Pressed::Ask(
                "Ctrl+W is used by New Tab — Replace?".to_owned(),
                vec![set("new-tab", "\"\""), set("close-tab", "\"mod+w\"")]
            ))
        );
    }
}
