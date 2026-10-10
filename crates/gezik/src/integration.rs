//! The System Integration panel (spec 3.2) and its palette commands: the tab picker's box
//! (`widgets/picker.slint`) with a row per feature (9b3: the command line), the journal's
//! count with Show, and Undo all. The state is read off the UI thread and only shown. Every
//! button first asks, naming exactly what will be written or taken back (read again for the
//! question); then it runs on a thread of its own, which reads what is there once more
//! (system_changes.rs). Answers come back as questions over the window.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gezik_config::shortcuts::{Chord, Key, Platform, parse_hotkey};
use gezik_config::system_journal::FILE;
use gezik_core::system_change::{self as sc, Change, Kind, Value};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::system_changes::{self as changes, DefaultState, PathState, Snapshot};
use crate::{AppWindow, PickRow};

/// A palette command; each panel button is one (spec 3.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    MakeDefault,
    RestoreDefault,
    AddToPath,
    RemoveFromPath,
    UndoAll,
    TrayOn,
    TrayOff,
    HotkeyOn,
    HotkeyOff,
    LoginOn,
    LoginOff,
}

impl Command {
    pub const ALL: [Command; 11] = [
        Command::MakeDefault,
        Command::RestoreDefault,
        Command::AddToPath,
        Command::RemoveFromPath,
        Command::UndoAll,
        Command::TrayOn,
        Command::TrayOff,
        Command::HotkeyOn,
        Command::HotkeyOff,
        Command::LoginOn,
        Command::LoginOff,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Command::MakeDefault => "Make Gezik the default file manager",
            Command::RestoreDefault => "Restore the system file manager",
            Command::AddToPath => "Add gezik to PATH",
            Command::RemoveFromPath => "Remove gezik from PATH",
            Command::UndoAll => "Undo all system changes",
            Command::TrayOn => "Turn on tray icon",
            Command::TrayOff => "Turn off tray icon",
            Command::HotkeyOn => "Set a global shortcut",
            Command::HotkeyOff => "Turn off the global shortcut",
            Command::LoginOn => "Start Gezik at login",
            Command::LoginOff => "Do not start Gezik at login",
        }
    }

    /// Settings of Gezik's own (settings.toml): no system question, no journal (spec 3.1).
    pub fn is_setting(self) -> bool {
        matches!(self, Command::TrayOn | Command::TrayOff | Command::HotkeyOn | Command::HotkeyOff)
    }
}

/// A line of the box: the feature, its state or path, the state word, the button's word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub title: String,
    pub detail: String,
    pub tag: &'static str,
    pub button: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowAction {
    Run(Command),
    /// Repair and Update: undo Gezik's PATH changes, then make them again.
    Repair,
    /// Repair and Update of the default file manager: Restore, then Make default again.
    RepairDefault,
    /// Update and Repair of start at login: its entry undone, then made again.
    RepairLogin,
    Show,
    Nothing,
}

fn row(title: impl Into<String>, detail: impl Into<String>, tag: &'static str, button: &'static str) -> Row {
    Row { title: title.into(), detail: detail.into(), tag, button }
}

/// The system's own file manager, as the rows and questions name it.
fn system_manager() -> &'static str {
    if cfg!(windows) {
        "Explorer"
    } else if cfg!(target_os = "macos") {
        "Finder"
    } else {
        "the system's file manager"
    }
}

/// Where the tray icon is, as the rows say it.
fn tray_place() -> &'static str {
    if cfg!(windows) {
        "the notification area"
    } else if cfg!(target_os = "macos") {
        "the menu bar"
    } else {
        "the tray"
    }
}

/// The rows for what was read (pure).
pub fn rows(snapshot: &Snapshot) -> Vec<(Row, RowAction)> {
    const DEFAULT: &str = "Default file manager";
    const PATH: &str = "Command line (PATH)";
    let on_detail = if cfg!(windows) {
        "Folders, Win+E and the Recycle Bin open in Gezik"
    } else if cfg!(target_os = "macos") {
        "Folders and Reveal in Finder open in Gezik"
    } else {
        "Folders and Show in folder open in Gezik"
    };
    let taken = if snapshot.dbus_taken { " (another file manager answers \"Show in folder\")" } else { "" };
    let default = match (&snapshot.changes, &snapshot.default) {
        (Err(_), _) => (row(DEFAULT, format!("Fix or delete {FILE} first"), "?", ""), RowAction::Nothing),
        (_, DefaultState::Off) => (
            row(DEFAULT, format!("Folders open in {}", system_manager()), "Off", "Make default"),
            RowAction::Run(Command::MakeDefault),
        ),
        (Ok(changes), DefaultState::On)
            if cfg!(target_os = "macos")
                && !changes.iter().any(|c| c.done && c.kind == gezik_core::system_change::Kind::MacDefault) =>
        {
            (
                row(DEFAULT, "Reveal in Finder opens in Gezik; macOS keeps folders for Finder", "On", "Restore"),
                RowAction::Run(Command::RestoreDefault),
            )
        }
        (_, DefaultState::On) => {
            (row(DEFAULT, format!("{on_detail}{taken}"), "On", "Restore"), RowAction::Run(Command::RestoreDefault))
        }
        (_, DefaultState::Moved { old }) => (
            row(DEFAULT, format!("Gezik's exe moved; registrations still name {old}"), "On", "Update"),
            RowAction::RepairDefault,
        ),
        (_, DefaultState::Changed) => {
            (row(DEFAULT, "Changed outside Gezik", "Changed", "Repair"), RowAction::RepairDefault)
        }
        (_, DefaultState::Taken { why }) => (row(DEFAULT, why.as_str(), "Off", ""), RowAction::Nothing),
    };
    let path = match (&snapshot.changes, &snapshot.path) {
        (Err(_), _) => (row(PATH, format!("Fix or delete {FILE} first"), "?", ""), RowAction::Nothing),
        (_, PathState::Off) => {
            (row(PATH, "gezik is not a command yet", "Off", "Add"), RowAction::Run(Command::AddToPath))
        }
        (_, PathState::On { command }) => (
            row(PATH, format!("gezik in a new terminal opens Gezik ({command})"), "On", "Remove"),
            RowAction::Run(Command::RemoveFromPath),
        ),
        (_, PathState::Moved { old }) => {
            (row(PATH, format!("Gezik's exe moved; gezik still starts {old}"), "On", "Update"), RowAction::Repair)
        }
        (_, PathState::Changed) => (row(PATH, "Changed outside Gezik", "Changed", "Repair"), RowAction::Repair),
        (_, PathState::Taken { place }) => {
            (row(PATH, format!("{place} was not made by Gezik; left alone"), "Off", ""), RowAction::Nothing)
        }
    };
    const TRAY: &str = "Tray icon";
    const SHORTCUT: &str = "Global shortcut";
    const LOGIN: &str = "Start at login";
    let resident = &snapshot.resident;
    let first = "Kept by the first Gezik window";
    let tray = if !resident.primary {
        (row(TRAY, first, "", ""), RowAction::Nothing)
    } else if resident.tray_missing {
        (
            row(TRAY, "No tray on this desktop (it needs a StatusNotifier host)", "Off", "Turn on"),
            RowAction::Run(Command::TrayOn),
        )
    } else if resident.tray {
        (
            row(TRAY, format!("Closing the window keeps Gezik in {}", tray_place()), "On", "Turn off"),
            RowAction::Run(Command::TrayOff),
        )
    } else {
        (row(TRAY, "Closing the window quits Gezik", "Off", "Turn on"), RowAction::Run(Command::TrayOn))
    };
    let shortcut = match (&resident.hotkey, &resident.hotkey_problem) {
        _ if !resident.primary => (row(SHORTCUT, first, "", ""), RowAction::Nothing),
        (Some(_), Some(why)) => (row(SHORTCUT, why.as_str(), "Off", "Change"), RowAction::Run(Command::HotkeyOn)),
        (Some(name), None) => (
            row(SHORTCUT, format!("{name} shows or hides Gezik from any app"), "On", "Turn off"),
            RowAction::Run(Command::HotkeyOff),
        ),
        (None, _) => {
            (row(SHORTCUT, "No key shows Gezik from other apps", "Off", "Turn on"), RowAction::Run(Command::HotkeyOn))
        }
    };
    let starts = if resident.tray {
        "Gezik starts in the tray when you sign in"
    } else {
        "Gezik opens its window when you sign in"
    };
    let login = match (&snapshot.changes, &snapshot.login) {
        (Err(_), _) => (row(LOGIN, format!("Fix or delete {FILE} first"), "?", ""), RowAction::Nothing),
        (_, changes::LoginState::Off) => {
            (row(LOGIN, "Gezik does not start when you sign in", "Off", "Turn on"), RowAction::Run(Command::LoginOn))
        }
        (_, changes::LoginState::On) => (row(LOGIN, starts, "On", "Turn off"), RowAction::Run(Command::LoginOff)),
        (_, changes::LoginState::Moved { old }) => (
            row(LOGIN, format!("Gezik's exe moved; sign-in still starts {old}"), "On", "Update"),
            RowAction::RepairLogin,
        ),
        (_, changes::LoginState::Changed) => {
            (row(LOGIN, "Changed outside Gezik", "Changed", "Repair"), RowAction::RepairLogin)
        }
        (_, changes::LoginState::Taken { why }) => (row(LOGIN, why.as_str(), "Off", ""), RowAction::Nothing),
    };
    let log = match &snapshot.changes {
        Ok(list) if list.is_empty() => {
            (row("Changes made: 0", "Nothing written to the system", "", ""), RowAction::Nothing)
        }
        Ok(list) => (
            row(format!("Changes made: {} ({FILE})", list.len()), snapshot.journal.display().to_string(), "", "Show"),
            RowAction::Show,
        ),
        Err(why) => (row("Changes made: ?", why.as_str(), "", ""), RowAction::Nothing),
    };
    let undo = (
        row(
            "Undo all system changes",
            "As gezik --unregister: newest first; what changed since is left",
            "",
            "Undo all",
        ),
        RowAction::Run(Command::UndoAll),
    );
    vec![default, path, tray, shortcut, login, log, undo]
}

/// The question's title and its yes button.
fn words(action: RowAction) -> (String, &'static str) {
    match action {
        RowAction::Run(Command::AddToPath) => ("Add gezik to PATH?".into(), "Add"),
        RowAction::Run(Command::RemoveFromPath) => ("Remove gezik from PATH?".into(), "Remove"),
        RowAction::Run(Command::UndoAll) => ("Undo all system changes?".into(), "Undo All"),
        RowAction::Run(Command::MakeDefault) => ("Make Gezik the default file manager?".into(), "Make Default"),
        RowAction::Run(Command::RestoreDefault) => (format!("Give folders back to {}?", system_manager()), "Restore"),
        RowAction::RepairDefault => ("Repair the default file manager?".into(), "Repair"),
        RowAction::Run(Command::LoginOn) => ("Start Gezik at login?".into(), "Turn On"),
        RowAction::Run(Command::LoginOff) => ("Stop starting Gezik at login?".into(), "Turn Off"),
        RowAction::RepairLogin => ("Repair start at login?".into(), "Repair"),
        _ => ("Repair gezik on PATH?".into(), "Repair"),
    }
}

/// What adding writes, a line per place (one of `path_targets`).
fn will_write(target: &Change) -> String {
    let what = sc::what(target);
    match (target.kind, &target.after) {
        (Kind::Folder, _) => format!("the folder {what}, if it is not there"),
        (Kind::RegistryKey, _) => format!("the registry key {what}, if it is not there"),
        (Kind::File, _) if target.feature != changes::FEATURE_PATH => format!("the file {what}"),
        (Kind::File, _) => {
            format!("the file {what}, a script that runs: {}", changes::SHIM.lines().last().unwrap_or_default())
        }
        (Kind::RegistryValue, Value::Reg { data, .. }) => format!("{what} = \"{data}\""),
        (Kind::PathEntry, _) => {
            format!("{} at the end of {what}; every other entry stays as it is", target.entry)
        }
        (Kind::Symlink, Value::Link(to)) => format!("the link {what}, to {to}"),
        (Kind::MacDefault, Value::Text(id)) => format!("the app for folders (public.folder): {id}"),
        (Kind::MacPref, Value::Text(id)) => format!("{what} (Reveal in Finder) = {id}"),
        (Kind::Mimeapps, Value::Text(id)) => format!("{} = {id} in {}", target.name, target.place),
        _ => what,
    }
}

fn bullets(lines: impl Iterator<Item = String>) -> String {
    lines.map(|l| format!("• {l}")).collect::<Vec<_>>().join("\n")
}

/// The question a button asks (pure): `Ok`, the text naming exactly what is written or
/// taken back; `Err`, why there is nothing to do (said, nothing written). `add`: what
/// adding writes (`None`: the folder or the exe's path is not known); `sweep`: what Undo
/// all takes back when the journal is empty.
#[cfg(test)]
pub fn confirmation(
    action: RowAction,
    snapshot: &Snapshot,
    add: Option<&[Change]>,
    sweep: &[Change],
) -> Result<String, String> {
    confirmation_with(action, snapshot, add, sweep, None, None, None)
}

/// As [`confirmation`], with what Make default writes (`None`: not known) and the warning
/// that Gezik's exe is somewhere it may leave (decision 14), said first, and what turning on
/// start at login writes (`login_add`).
pub fn confirmation_with(
    action: RowAction,
    snapshot: &Snapshot,
    add: Option<&[Change]>,
    sweep: &[Change],
    default_add: Option<&[Change]>,
    risky: Option<&str>,
    login_add: Option<&[Change]>,
) -> Result<String, String> {
    let list = match &snapshot.changes {
        Err(why) => {
            return Err(format!("{why}\n\nNothing was changed. Fix or delete {}.", snapshot.journal.display()));
        }
        Ok(list) => list,
    };
    let newest_first = |path_only: bool| {
        bullets(list.iter().rev().filter(move |c| !path_only || c.feature == changes::FEATURE_PATH).map(sc::describe))
    };
    let adding = || match add {
        Some(targets) => Ok(format!(
            "Gezik writes these, each noted in {FILE} before it is made; what is there already is kept:\n\n{}",
            bullets(targets.iter().map(will_write))
        )),
        None => Err("Gezik cannot tell its own path or the user's folder; nothing was changed.".to_owned()),
    };
    let taking_back = || {
        let lines = newest_first(true);
        if lines.is_empty() {
            return Err("Gezik has not added gezik to PATH; there is nothing to remove.".to_owned());
        }
        Ok(format!(
            "Gezik puts these back as they were, newest first, each only if it is still what Gezik wrote:\n\n{lines}"
        ))
    };
    let feature_lines = |feature: &str| bullets(list.iter().rev().filter(|c| c.feature == feature).map(sc::describe));
    let default_lines = || feature_lines(changes::FEATURE_DEFAULT);
    let login_lines = || feature_lines(changes::FEATURE_LOGIN);
    match action {
        RowAction::Run(Command::MakeDefault) => match &snapshot.default {
            DefaultState::On => Err("Gezik is already the default file manager.".into()),
            DefaultState::Taken { why } => Err(why.clone()),
            _ => {
                let Some(targets) = default_add else {
                    return Err("Gezik cannot tell its own path or the user's folders; nothing was changed.".into());
                };
                let mut text = risky.map(|r| format!("{r}\n\n")).unwrap_or_default();
                text.push_str(&format!(
                    "Gezik writes these, each noted in {FILE} before it is made; if anything is in the way, nothing is written:\n\n{}",
                    bullets(targets.iter().map(will_write))
                ));
                if cfg!(windows) {
                    text.push_str(&format!(
                        "\n\nA file that undoes this without Gezik is kept next to {FILE}: {}.",
                        changes::RESTORE_REG
                    ));
                }
                Ok(text)
            }
        },
        RowAction::Run(Command::RestoreDefault) => {
            let lines = default_lines();
            if lines.is_empty() {
                return Err("Gezik is not the default file manager; there is nothing to restore.".into());
            }
            Ok(format!(
                "Gezik puts these back as they were, newest first, each only if it is still what Gezik wrote:\n\n{lines}"
            ))
        }
        RowAction::RepairDefault => Ok(format!(
            "Gezik takes back its registrations and makes them again for this Gezik:\n\n{}",
            default_lines()
        )),
        RowAction::Run(Command::AddToPath) => match &snapshot.path {
            PathState::On { command } => Err(format!("gezik is already a command ({command}).")),
            PathState::Taken { place } => Err(format!("{place} is there and was not made by Gezik; it is left alone.")),
            _ => adding(),
        },
        RowAction::Run(Command::RemoveFromPath) => taking_back(),
        RowAction::Repair => Ok(format!("{}\n\nThen, for this Gezik: {}", taking_back()?, adding()?)),
        RowAction::Run(Command::UndoAll) if !list.is_empty() => Ok(format!(
            "Gezik puts every change in {FILE} back as it was, newest first; anything changed since by something else is left as it is:\n\n{}",
            newest_first(false)
        )),
        RowAction::Run(Command::UndoAll) if !sweep.is_empty() => Ok(format!(
            "{FILE} lists no changes. Gezik takes back these, found by Gezik's own names and shapes:\n\n{}",
            bullets(sweep.iter().map(sc::what))
        )),
        RowAction::Run(Command::UndoAll) => Err("Gezik has made no system changes; there is nothing to undo.".into()),
        RowAction::Run(Command::LoginOn) => match &snapshot.login {
            changes::LoginState::On => Err("Gezik already starts at login.".into()),
            changes::LoginState::Taken { why } => Err(why.clone()),
            _ => {
                let Some(targets) = login_add else {
                    return Err("Gezik cannot tell its own path or the user's folders; nothing was changed.".into());
                };
                Ok(format!(
                    "Gezik writes these, each noted in {FILE} before it is made; what is there already is kept:\n\n{}\n\nAt sign-in Gezik starts with --background: without a window while the tray icon is on, else with its window.",
                    bullets(targets.iter().map(will_write))
                ))
            }
        },
        RowAction::Run(Command::LoginOff) => {
            let lines = login_lines();
            if lines.is_empty() {
                return Err("Gezik does not start at login; there is nothing to turn off.".into());
            }
            Ok(format!(
                "Gezik puts these back as they were, newest first, each only if it is still what Gezik wrote:\n\n{lines}"
            ))
        }
        RowAction::RepairLogin => {
            let Some(targets) = login_add else {
                return Err("Gezik cannot tell its own path or the user's folders; nothing was changed.".into());
            };
            Ok(format!(
                "Gezik takes back its start at login entry, newest first, each only if it is still what Gezik wrote:\n\n{}\n\nThen, for this Gezik, it writes these, each noted in {FILE} before it is made:\n\n{}",
                login_lines(),
                bullets(targets.iter().map(will_write))
            ))
        }
        // Settings never ask (`run`, `choose`); spelled out so the match stays exhaustive.
        RowAction::Run(Command::TrayOn | Command::TrayOff | Command::HotkeyOn | Command::HotkeyOff) => {
            Err(String::new())
        }
        RowAction::Show | RowAction::Nothing => Err(String::new()),
    }
}

thread_local! {
    static CURRENT: RefCell<Option<Integration>> = const { RefCell::new(None) };
}

pub fn with_current(f: impl FnOnce(&Integration)) {
    if let Some(integration) = CURRENT.with(|c| c.borrow().clone()) {
        f(&integration);
    }
}

pub fn is_open() -> bool {
    CURRENT.with(|c| c.borrow().as_ref().is_some_and(|i| i.0.open.get()))
}

struct Inner {
    window: slint::Weak<AppWindow>,
    open: Cell<bool>,
    actions: RefCell<Vec<RowAction>>,
    current: Cell<usize>,
    snapshot: RefCell<Option<Snapshot>>,
    model: Rc<VecModel<PickRow>>,
}

#[derive(Clone)]
pub struct Integration(Rc<Inner>);

impl Integration {
    pub fn new(window: &AppWindow) -> Integration {
        let integration = Integration(Rc::new(Inner {
            window: window.as_weak(),
            open: Cell::new(false),
            actions: RefCell::default(),
            current: Cell::new(0),
            snapshot: RefCell::default(),
            model: Rc::new(VecModel::default()),
        }));
        CURRENT.with(|c| *c.borrow_mut() = Some(integration.clone()));
        integration
    }

    /// `system-integration`: the state is read on a thread, then the box opens.
    pub fn open(&self) {
        std::thread::spawn(|| {
            let snapshot = changes::read_snapshot();
            let _ = slint::invoke_from_event_loop(move || with_current(|i| i.show(snapshot)));
        });
    }

    fn show(&self, snapshot: Snapshot) {
        let Some(window) = self.0.window.upgrade() else { return };
        // Something else opened meanwhile: its keys would go to the box.
        if window.get_tp_open() || crate::tab_tools::over_another_layer(&window) {
            return;
        }
        let mut snapshot = snapshot;
        snapshot.dbus_taken = crate::single_instance::file_manager1_taken();
        snapshot.resident = crate::resident::status();
        let (lines, actions): (Vec<Row>, Vec<RowAction>) = rows(&snapshot).into_iter().unzip();
        self.0.model.set_vec(
            lines
                .into_iter()
                .map(|r| PickRow {
                    title: r.title.into(),
                    detail: r.detail.into(),
                    tag: r.tag.into(),
                    keys: r.button.into(),
                    bold: false,
                })
                .collect::<Vec<_>>(),
        );
        *self.0.actions.borrow_mut() = actions;
        *self.0.snapshot.borrow_mut() = Some(snapshot);
        self.0.current.set(0);
        self.0.open.set(true);
        window.set_tp_rows(ModelRc::from(self.0.model.clone()));
        window.set_tp_label("System integration".into());
        window.set_tp_empty("".into());
        window.set_tp_query("".into());
        window.set_tp_current(0);
        window.set_tp_open(true);
        window.invoke_focus_list();
    }

    /// Esc closes, Enter runs the current row's button, Up/Down move; typing does nothing.
    pub fn chord(&self, chord: &Chord) -> bool {
        if chord.ctrl || chord.alt || chord.meta || chord.shift {
            return false;
        }
        match chord.key {
            Key::Escape => self.close(),
            Key::Enter => self.choose(),
            Key::Up | Key::Down => {
                let len = self.0.actions.borrow().len();
                self.0.current.set(crate::tab_tools::step(self.0.current.get(), len, chord.key == Key::Down));
                if let Some(window) = self.0.window.upgrade() {
                    window.set_tp_current(i32::try_from(self.0.current.get()).unwrap_or(0));
                }
            }
            _ => return false,
        }
        true
    }

    pub fn chosen(&self, row: usize) {
        self.0.current.set(row);
        self.choose();
    }

    fn choose(&self) {
        let Some(action) = self.0.actions.borrow().get(self.0.current.get()).copied() else { return };
        if action == RowAction::Nothing {
            return;
        }
        let snapshot = self.0.snapshot.borrow_mut().take();
        self.close();
        match action {
            RowAction::Show => show_log(snapshot),
            RowAction::Run(command) if command.is_setting() => setting(command),
            _ => confirm(action),
        }
    }

    fn close(&self) {
        let Some(window) = self.0.window.upgrade() else { return };
        self.0.open.set(false);
        window.set_tp_open(false);
        self.0.model.set_vec(Vec::new());
        self.0.actions.borrow_mut().clear();
        if !window.get_dialog_open() && !window.get_conflicts_open() {
            window.invoke_focus_list();
        }
    }
}

/// A palette command or a panel button: system changes ask first; settings just happen.
pub fn run(command: Command) {
    if command.is_setting() { setting(command) } else { confirm(RowAction::Run(command)) }
}

fn setting(command: Command) {
    // Deviation 11: only the first Gezik keeps them; a second window writes nothing.
    if !crate::resident::status().primary {
        return crate::view::with_current(|v| v.note("Kept by the first Gezik window".into()));
    }
    match command {
        Command::TrayOn => crate::resident::turn_on_tray(),
        Command::TrayOff => crate::resident::turn_off_tray(),
        Command::HotkeyOn => ask_hotkey(None),
        Command::HotkeyOff => crate::resident::turn_off_hotkey(),
        _ => {}
    }
}

fn hotkey_platform(os: changes::Os) -> Platform {
    if os == changes::Os::Mac { Platform::Mac } else { Platform::Other }
}

/// The line under the shortcut field as it is typed (pure).
pub fn hotkey_note(text: &str, os: changes::Os) -> (String, bool) {
    match parse_hotkey(text, hotkey_platform(os)) {
        Ok(None) => (format!("Type a shortcut, for example {}", crate::resident::example(os)), true),
        Ok(Some(chord)) => (format!("{} will show or hide Gezik", crate::resident::label(&chord, os)), false),
        Err(why) => (format!("\"{}\" {why}", text.trim()), true),
    }
}

/// Set a global shortcut (user decision: no default; the field starts empty, the example is
/// only in the text). `previous`: the text and the reason when asking again.
pub fn ask_hotkey(previous: Option<(String, String)>) {
    let os = changes::Os::HERE;
    // The rules of parse_hotkey (a77a1c2), in words.
    let rule = match os {
        changes::Os::Mac => "A letter, a digit or F1-F12 with ⌘ (cmd) or ⌃ (ctrl), and ⌥ (alt) or ⇧ (shift).",
        changes::Os::Windows => {
            "A letter, a digit or F1-F12 with Win, or with Ctrl+Shift or Alt+Shift; Ctrl+Alt is not used (it types AltGr characters)."
        }
        changes::Os::Linux => {
            "A letter, a digit or F1-F12 with Super, or with Ctrl+Shift or Alt+Shift; Ctrl+Alt is not used (it types AltGr characters)."
        }
    };
    let mut message = format!(
        "Type the keys that show or hide Gezik from any app, for example {}. {rule}",
        crate::resident::example(os),
    );
    let initial = match previous {
        Some((text, why)) => {
            message = format!("{why}\n\n{message}");
            text
        }
        None => String::new(),
    };
    crate::operations::with_current(|ops| {
        ops.dialogs().ask_text_noted(
            "Global shortcut",
            message,
            initial,
            &["Turn On", "Cancel"],
            move |text| hotkey_note(text, os),
            move |answer| {
                let Some(text) = answer else { return };
                match parse_hotkey(&text, hotkey_platform(os)) {
                    Ok(Some(chord)) => crate::resident::set_hotkey(chord),
                    Ok(None) => {}
                    Err(why) => {
                        let reason = format!("\"{}\" {why}", text.trim());
                        ask_hotkey(Some((text, reason)));
                    }
                }
            },
        )
    });
}

/// Reads what is there now on a thread, asks with exactly what will be written, then does it.
fn confirm(action: RowAction) {
    std::thread::spawn(move || {
        let snapshot = changes::read_snapshot();
        let access = changes::SystemAccess::new();
        let exe = gezik_platform::system::exe().unwrap_or_default();
        let add = exe.to_str().and_then(|e| changes::path_targets(&access.places, e, cfg!(windows)));
        let sweep = if action == RowAction::Run(Command::UndoAll) {
            changes::sweep_changes(&access, &access.places, &exe, changes::Os::HERE)
        } else {
            Vec::new()
        };
        let (default_add, risky) = if action == RowAction::Run(Command::MakeDefault) {
            let targets =
                exe.to_str().and_then(|e| changes::default_targets(&access.places, e, changes::Os::HERE, None));
            (targets, changes::risky_note())
        } else {
            (None, None)
        };
        let login_add = if matches!(action, RowAction::Run(Command::LoginOn) | RowAction::RepairLogin) {
            exe.to_str().and_then(|e| changes::login_targets(&access.places, e, changes::Os::HERE))
        } else {
            None
        };
        let asked = confirmation_with(
            action,
            &snapshot,
            add.as_deref(),
            &sweep,
            default_add.as_deref(),
            risky.as_deref(),
            login_add.as_deref(),
        );
        let _ = slint::invoke_from_event_loop(move || {
            let (title, button) = words(action);
            match asked {
                Err(why) => tell(&title, why),
                Ok(message) => ask(&title, message, &[button, "Cancel"], move |choice| {
                    if choice == Some(0) {
                        carry_out(action);
                    }
                }),
            }
        });
    });
}

fn carry_out(action: RowAction) {
    match action {
        RowAction::Run(Command::AddToPath) => {
            std::thread::spawn(|| {
                let result = changes::add_now();
                let _ = slint::invoke_from_event_loop(move || added(result));
            });
        }
        RowAction::Run(Command::RemoveFromPath) => in_background("Remove gezik from PATH", changes::remove_now),
        RowAction::Run(Command::UndoAll) => {
            in_background("Undo all system changes", || Ok(changes::undo_all_now().lines))
        }
        RowAction::Repair => in_background("Repair gezik on PATH", changes::repair_now),
        RowAction::Run(Command::MakeDefault) => {
            std::thread::spawn(|| {
                let result = changes::make_default_now();
                let _ = slint::invoke_from_event_loop(move || match result {
                    Err(why) => tell("Could not make Gezik the default file manager", why),
                    Ok(Some(note)) => tell("Reveal in Finder opens in Gezik now", note),
                    Ok(None) => {
                        crate::single_instance::start_file_manager1();
                        crate::view::with_current(|v| v.note("Gezik is the default file manager now".into()));
                    }
                });
            });
        }
        RowAction::Run(Command::RestoreDefault) => {
            crate::single_instance::stop_file_manager1();
            in_background("Restore the system file manager", changes::restore_default_now);
        }
        RowAction::RepairDefault => in_background("Repair the default file manager", changes::repair_default_now),
        RowAction::Run(Command::LoginOn) => {
            std::thread::spawn(|| {
                let result = changes::login_on_now();
                let _ = slint::invoke_from_event_loop(move || match result {
                    Err(why) => tell("Could not make Gezik start at login", why),
                    Ok(()) => crate::view::with_current(|v| v.note("Gezik starts at login now".into())),
                });
            });
        }
        RowAction::Run(Command::LoginOff) => {
            in_background("Do not start Gezik at login", || changes::undo_feature_now(changes::FEATURE_LOGIN))
        }
        RowAction::RepairLogin => in_background("Repair start at login", changes::repair_login_now),
        RowAction::Run(command @ (Command::TrayOn | Command::TrayOff | Command::HotkeyOn | Command::HotkeyOff)) => {
            setting(command)
        }
        RowAction::Show | RowAction::Nothing => {}
    }
}

/// The start-up check's question (decision 13): Repair does what the panel's Update does.
pub fn offer_repair(text: String) {
    ask("Gezik moved", text, &["Repair", "Later"], |choice| {
        if choice == Some(0) {
            in_background("Repair system registrations", || {
                // Read again: the question's state is only shown, never acted on.
                let snapshot = changes::read_snapshot();
                let mut lines = Vec::new();
                if matches!(snapshot.default, DefaultState::Moved { .. }) {
                    lines.extend(changes::repair_default_now()?);
                }
                if matches!(snapshot.path, PathState::Moved { .. }) {
                    lines.extend(changes::repair_now()?);
                }
                if matches!(snapshot.login, changes::LoginState::Moved { .. }) {
                    lines.extend(changes::repair_login_now()?);
                }
                Ok(lines)
            });
        }
    });
}

fn added(result: Result<(), String>) {
    match result {
        Err(why) => tell("Could not add gezik to PATH", why),
        Ok(()) => match changes::path_hint() {
            Some(line) => ask(
                "gezik is a command now",
                format!(
                    "If gezik is not found in a new terminal, add this line to your shell profile (~/.zshrc, ~/.bashrc):\n\n{line}"
                ),
                &["Copy", "Close"],
                move |choice| {
                    if choice == Some(0) {
                        let _ = gezik_platform::clipboard::write_text(line);
                    }
                },
            ),
            None => {
                crate::view::with_current(|v| v.note("gezik is a command now: open a new terminal to use it".into()))
            }
        },
    }
}

/// Runs `work` on a thread; its lines (or its error) come back as one question.
fn in_background(title: &'static str, work: impl FnOnce() -> Result<Vec<String>, String> + Send + 'static) {
    std::thread::spawn(move || {
        let result = work();
        let _ = slint::invoke_from_event_loop(move || match result {
            Ok(lines) if lines.is_empty() => tell(title, "Nothing to do.".into()),
            Ok(lines) => tell(title, lines.join("\n")),
            Err(why) => tell(title, why),
        });
    });
}

fn show_log(snapshot: Option<Snapshot>) {
    let lines: Vec<String> =
        snapshot.and_then(|s| s.changes.ok()).unwrap_or_default().iter().map(sc::describe).collect();
    tell(FILE, lines.join("\n"));
}

fn tell(title: &str, message: String) {
    ask(title, message, &["Close"], |_| {});
}

fn ask(title: &str, message: impl Into<String>, buttons: &[&str], answer: impl FnOnce(Option<usize>) + 'static) {
    let message = message.into();
    crate::operations::with_current(|ops| ops.dialogs().ask(title, message, buttons, answer));
}

#[cfg(test)]
mod tests {
    use super::*;
    use gezik_platform::system::Places;
    use std::path::PathBuf;

    fn snap(path: PathState, changes: Result<usize, &str>) -> Snapshot {
        let change = gezik_core::system_change::Change {
            feature: "path".into(),
            kind: gezik_core::system_change::Kind::Folder,
            place: "/x".into(),
            name: String::new(),
            entry: String::new(),
            before: gezik_core::system_change::Value::Absent,
            after: gezik_core::system_change::Value::Present,
            done: true,
        };
        Snapshot {
            path,
            default: changes::DefaultState::Off,
            login: changes::LoginState::Off,
            changes: changes.map(|n| vec![change; n]).map_err(str::to_owned),
            journal: PathBuf::from("/c/system-changes.toml"),
            dbus_taken: false,
            resident: crate::resident::Status { primary: true, ..Default::default() },
        }
    }

    fn shape(snapshot: &Snapshot) -> Vec<(String, &'static str, &'static str, RowAction)> {
        rows(snapshot).into_iter().map(|(r, a)| (r.title, r.tag, r.button, a)).collect()
    }

    #[test]
    fn rows_follow_the_state() {
        let off = shape(&snap(PathState::Off, Ok(0)));
        assert_eq!(off[1], ("Command line (PATH)".into(), "Off", "Add", RowAction::Run(Command::AddToPath)));
        assert_eq!(off[5], ("Changes made: 0".into(), "", "", RowAction::Nothing));
        assert_eq!(off[6], ("Undo all system changes".into(), "", "Undo all", RowAction::Run(Command::UndoAll)));
        let on = shape(&snap(PathState::On { command: "/h/.local/bin/gezik".into() }, Ok(3)));
        assert_eq!((on[1].1, on[1].2), ("On", "Remove"));
        assert_eq!(on[1].3, RowAction::Run(Command::RemoveFromPath));
        assert_eq!(on[5], ("Changes made: 3 (system-changes.toml)".into(), "", "Show", RowAction::Show));
        assert_eq!(shape(&snap(PathState::Moved { old: "/old".into() }, Ok(3)))[1].2, "Update");
        assert_eq!(shape(&snap(PathState::Changed, Ok(3)))[1].3, RowAction::Repair);
        assert_eq!(shape(&snap(PathState::Taken { place: "/p".into() }, Ok(0)))[1].3, RowAction::Nothing);
        let bad = shape(&snap(PathState::Off, Err("system-changes.toml cannot be read: version 9")));
        assert_eq!(bad[0].3, RowAction::Nothing, "nothing is written next to a journal that cannot be read");
        assert_eq!(bad[1].3, RowAction::Nothing);
        assert_eq!(bad[4].3, RowAction::Nothing, "start at login");
        assert_eq!(bad[5].3, RowAction::Nothing);
    }

    #[test]
    fn every_command_has_a_title() {
        let titles: Vec<&str> = Command::ALL.iter().map(|c| c.title()).collect();
        assert_eq!(
            titles,
            [
                "Make Gezik the default file manager",
                "Restore the system file manager",
                "Add gezik to PATH",
                "Remove gezik from PATH",
                "Undo all system changes",
                "Turn on tray icon",
                "Turn off tray icon",
                "Set a global shortcut",
                "Turn off the global shortcut",
                "Start Gezik at login",
                "Do not start Gezik at login"
            ]
        );
        assert!(
            Command::ALL.iter().filter(|c| c.is_setting()).count() == 4,
            "tray and shortcut are settings: no system question"
        );
    }

    fn with_default(default: DefaultState) -> Snapshot {
        Snapshot { default, ..snap(PathState::Off, Ok(0)) }
    }

    #[test]
    fn the_default_row_says_what_it_does() {
        let first = |state: DefaultState| {
            let (row, action) = rows(&with_default(state)).swap_remove(0);
            (row.title, row.tag, row.button, action)
        };
        assert_eq!(
            first(DefaultState::Off),
            ("Default file manager".into(), "Off", "Make default", RowAction::Run(Command::MakeDefault))
        );
        assert_eq!(first(DefaultState::On).3, RowAction::Run(Command::RestoreDefault));
        assert_eq!(first(DefaultState::On).2, "Restore");
        assert_eq!(first(DefaultState::Moved { old: "x".into() }).3, RowAction::RepairDefault);
        assert_eq!(first(DefaultState::Moved { old: "x".into() }).2, "Update");
        assert_eq!(first(DefaultState::Changed).2, "Repair");
        assert_eq!(first(DefaultState::Taken { why: "w".into() }).3, RowAction::Nothing);
        let taken = Snapshot { dbus_taken: true, ..with_default(DefaultState::On) };
        assert!(rows(&taken)[0].0.detail.ends_with("(another file manager answers \"Show in folder\")"));
    }

    #[test]
    fn make_default_asks_with_the_risky_place_first() {
        let make = RowAction::Run(Command::MakeDefault);
        let places = Places { home: Some(PathBuf::from("/h")), ..Places::default() };
        let add = changes::default_targets(&places, r"C:\G\gezik.exe", changes::Os::Windows, None).unwrap();
        let off = with_default(DefaultState::Off);
        let text =
            confirmation_with(make, &off, None, &[], Some(&add), Some("Gezik is in Downloads. …"), None).unwrap();
        assert!(text.starts_with("Gezik is in Downloads. …\n\nGezik writes these"), "{text}");
        assert!(text.contains(r#"= ""C:\G\gezik.exe" --shell "%1""#), "{text}");
        assert!(!confirmation_with(make, &off, None, &[], Some(&add), None, None).unwrap().starts_with("Gezik is in"));
        assert!(confirmation_with(make, &off, None, &[], None, None, None).is_err(), "targets not known");
        let on = with_default(DefaultState::On);
        assert!(confirmation_with(make, &on, None, &[], Some(&add), None, None).unwrap_err().contains("already"));
        let taken = with_default(DefaultState::Taken { why: "another file manager answers Win+E".into() });
        assert_eq!(
            confirmation_with(make, &taken, None, &[], Some(&add), None, None).unwrap_err(),
            "another file manager answers Win+E"
        );
        let restore = RowAction::Run(Command::RestoreDefault);
        assert!(confirmation(restore, &off, None, &[]).unwrap_err().contains("nothing to restore"));
        let mut made = add[0].clone();
        made.done = true;
        let listed = Snapshot { changes: Ok(vec![made]), ..with_default(DefaultState::On) };
        assert!(
            confirmation(restore, &listed, None, &[]).unwrap().contains("Classes"),
            "the default changes are listed"
        );
        assert!(confirmation(RowAction::RepairDefault, &listed, None, &[]).unwrap().contains("Classes"));
    }

    #[test]
    fn the_question_names_what_is_written() {
        let places = Places { local_app_data: Some(PathBuf::from(r"C:\L")), ..Places::default() };
        let add = changes::path_targets(&places, r"C:\G\gezik.exe", true).unwrap();
        let add_to_path = RowAction::Run(Command::AddToPath);
        let text = confirmation(add_to_path, &snap(PathState::Off, Ok(0)), Some(&add), &[]).unwrap();
        for part in [
            r#"• the folder C:\L\Gezik\bin, if it is not there"#,
            r#"• the file C:\L\Gezik\bin\gezik.cmd, a script that runs: start "" gezik.exe %*"#,
            r#"App Paths\gezik.exe\(Default) = "C:\G\gezik.exe""#,
            r"• C:\L\Gezik\bin at the end of HKCU\Environment\Path; every other entry stays as it is",
        ] {
            assert!(text.contains(part), "{part} in {text}");
        }
        let on = snap(PathState::On { command: "x".into() }, Ok(1));
        assert!(confirmation(add_to_path, &on, Some(&add), &[]).unwrap_err().contains("already"));
        let taken = snap(PathState::Taken { place: "/p".into() }, Ok(0));
        assert!(confirmation(add_to_path, &taken, Some(&add), &[]).unwrap_err().contains("not made by Gezik"));
        assert!(confirmation(add_to_path, &snap(PathState::Off, Ok(0)), None, &[]).is_err());

        let remove = RowAction::Run(Command::RemoveFromPath);
        assert!(confirmation(remove, &snap(PathState::Off, Ok(0)), Some(&add), &[]).is_err());
        let text = confirmation(remove, &on, Some(&add), &[]).unwrap();
        assert!(text.contains("• path: /x: before (none), after (there)"), "{text}");
        let repair = confirmation(RowAction::Repair, &snap(PathState::Changed, Ok(1)), Some(&add), &[]).unwrap();
        assert!(repair.contains("• path: /x") && repair.contains("gezik.cmd"), "{repair}");

        let undo = RowAction::Run(Command::UndoAll);
        assert!(confirmation(undo, &snap(PathState::Off, Ok(0)), Some(&add), &[]).is_err(), "nothing to undo");
        assert!(confirmation(undo, &snap(PathState::Off, Ok(0)), Some(&add), &add).unwrap().contains("gezik.cmd"));
        let bad = snap(PathState::Off, Err("version 9"));
        for action in [add_to_path, remove, undo, RowAction::Repair] {
            assert!(confirmation(action, &bad, Some(&add), &add).unwrap_err().starts_with("version 9"));
        }
    }

    fn with_resident(resident: crate::resident::Status) -> Snapshot {
        Snapshot { resident, ..snap(PathState::Off, Ok(0)) }
    }

    fn row_of(snapshot: &Snapshot, title: &str) -> (String, &'static str, &'static str, RowAction) {
        rows(snapshot)
            .into_iter()
            .find(|(r, _)| r.title == title)
            .map(|(r, a)| (r.detail, r.tag, r.button, a))
            .unwrap_or_else(|| panic!("no row {title}"))
    }

    #[test]
    fn the_tray_and_shortcut_rows_say_what_they_do() {
        use crate::resident::Status;
        let primary = Status { primary: true, ..Status::default() };
        let tray = |s: Status| row_of(&with_resident(s), "Tray icon");
        assert_eq!(
            tray(primary.clone()),
            ("Closing the window quits Gezik".into(), "Off", "Turn on", RowAction::Run(Command::TrayOn))
        );
        let on = tray(Status { tray: true, ..primary.clone() });
        assert_eq!((on.1, on.2, on.3), ("On", "Turn off", RowAction::Run(Command::TrayOff)));
        assert!(on.0.starts_with("Closing the window keeps Gezik in "), "{}", on.0);
        let missing = tray(Status { tray: true, tray_missing: true, ..primary.clone() });
        assert!(missing.0.starts_with("No tray on this desktop"), "{}", missing.0);
        assert_eq!((missing.1, missing.3), ("Off", RowAction::Run(Command::TrayOn)), "it may be tried again");
        let second = tray(Status::default());
        assert_eq!((second.0.as_str(), second.3), ("Kept by the first Gezik window", RowAction::Nothing));

        let shortcut = |s: Status| row_of(&with_resident(s), "Global shortcut");
        assert_eq!(
            shortcut(primary.clone()),
            ("No key shows Gezik from other apps".into(), "Off", "Turn on", RowAction::Run(Command::HotkeyOn))
        );
        let set = shortcut(Status { hotkey: Some("Shift+Win+E".into()), ..primary.clone() });
        assert_eq!(
            set,
            (
                "Shift+Win+E shows or hides Gezik from any app".into(),
                "On",
                "Turn off",
                RowAction::Run(Command::HotkeyOff)
            )
        );
        let taken = shortcut(Status {
            hotkey: Some("Shift+Win+E".into()),
            hotkey_problem: Some("Shift+Win+E is used by another app.".into()),
            ..primary.clone()
        });
        assert_eq!(
            taken,
            ("Shift+Win+E is used by another app.".into(), "Off", "Change", RowAction::Run(Command::HotkeyOn))
        );
        assert_eq!(shortcut(Status::default()).3, RowAction::Nothing);
    }

    #[test]
    fn the_login_row_and_its_question() {
        let login = |state: changes::LoginState| {
            row_of(&Snapshot { login: state, ..snap(PathState::Off, Ok(0)) }, "Start at login")
        };
        assert_eq!(login(changes::LoginState::Off).3, RowAction::Run(Command::LoginOn));
        assert_eq!(login(changes::LoginState::On).3, RowAction::Run(Command::LoginOff));
        assert_eq!(login(changes::LoginState::Moved { old: "x".into() }).3, RowAction::RepairLogin);
        assert_eq!(login(changes::LoginState::Moved { old: "x".into() }).2, "Update");
        assert_eq!(login(changes::LoginState::Changed).2, "Repair");
        assert_eq!(login(changes::LoginState::Taken { why: "w".into() }).3, RowAction::Nothing);
        let places = Places::default();
        let add = changes::login_targets(&places, r"C:\G\gezik.exe", changes::Os::Windows).unwrap();
        let on = RowAction::Run(Command::LoginOn);
        let off = snap(PathState::Off, Ok(0));
        let text = confirmation_with(on, &off, None, &[], None, None, Some(&add)).unwrap();
        assert!(
            text.contains(
                r#"• HKCU\Software\Microsoft\Windows\CurrentVersion\Run\Gezik = ""C:\G\gezik.exe" --background""#
            ),
            "{text}"
        );
        assert!(text.contains("At sign-in Gezik starts with --background"), "{text}");
        assert!(confirmation_with(on, &off, None, &[], None, None, None).is_err(), "targets not known");
        let already = Snapshot { login: changes::LoginState::On, ..off.clone() };
        assert!(confirmation_with(on, &already, None, &[], None, None, Some(&add)).unwrap_err().contains("already"));
        let off_action = RowAction::Run(Command::LoginOff);
        assert!(confirmation(off_action, &off, None, &[]).unwrap_err().contains("nothing to turn off"));
        let mut made = add.iter().find(|c| c.kind == Kind::RegistryValue).unwrap().clone();
        made.done = true;
        let listed = Snapshot { changes: Ok(vec![made]), login: changes::LoginState::On, ..off };
        assert!(confirmation(off_action, &listed, None, &[]).unwrap().contains(r"Run\Gezik"));
        let other = changes::login_targets(&places, r"D:\New\gezik.exe", changes::Os::Windows).unwrap();
        let repair = confirmation_with(RowAction::RepairLogin, &listed, None, &[], None, None, Some(&other)).unwrap();
        assert!(repair.contains(r"Run\Gezik"), "what is taken back: {repair}");
        assert!(repair.contains(r#"""D:\New\gezik.exe" --background"#), "what is written: {repair}");
        assert!(confirmation_with(RowAction::RepairLogin, &listed, None, &[], None, None, None).is_err());
    }

    #[test]
    fn the_hotkey_note_says_what_it_reads() {
        assert_eq!(hotkey_note("", changes::Os::Windows), ("Type a shortcut, for example win+shift+e".into(), true));
        assert_eq!(
            hotkey_note("win+shift+e", changes::Os::Windows),
            ("Shift+Win+E will show or hide Gezik".into(), false)
        );
        assert_eq!(
            hotkey_note("ctrl+alt+e", changes::Os::Windows),
            ("\"ctrl+alt+e\" uses Ctrl+Alt, which types AltGr characters".into(), true)
        );
        assert!(hotkey_note("alt+e", changes::Os::Mac).1, "macOS needs ⌘ or ⌃");
        assert_eq!(hotkey_note("super+shift+e", changes::Os::Linux).0, "Shift+Super+E will show or hide Gezik");
    }
}
