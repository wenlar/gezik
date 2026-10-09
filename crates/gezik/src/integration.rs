//! The System Integration panel (spec 3.2) and its palette commands: the tab picker's box
//! (`widgets/picker.slint`) with a row per feature (9b3: the command line), the journal's
//! count with Show, and Undo all. The state is read off the UI thread and only shown. Every
//! button first asks, naming exactly what will be written or taken back (read again for the
//! question); then it runs on a thread of its own, which reads what is there once more
//! (system_changes.rs). Answers come back as questions over the window.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gezik_config::shortcuts::{Chord, Key};
use gezik_config::system_journal::FILE;
use gezik_core::system_change::{self as sc, Change, Kind, Value};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::system_changes::{self as changes, PathState, Snapshot};
use crate::{AppWindow, PickRow};

/// A palette command; each panel button is one (spec 3.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    AddToPath,
    RemoveFromPath,
    UndoAll,
}

impl Command {
    pub const ALL: [Command; 3] = [Command::AddToPath, Command::RemoveFromPath, Command::UndoAll];

    pub fn title(self) -> &'static str {
        match self {
            Command::AddToPath => "Add gezik to PATH",
            Command::RemoveFromPath => "Remove gezik from PATH",
            Command::UndoAll => "Undo all system changes",
        }
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
    Show,
    Nothing,
}

fn row(title: impl Into<String>, detail: impl Into<String>, tag: &'static str, button: &'static str) -> Row {
    Row { title: title.into(), detail: detail.into(), tag, button }
}

/// The rows for what was read (pure).
pub fn rows(snapshot: &Snapshot) -> Vec<(Row, RowAction)> {
    const PATH: &str = "Command line (PATH)";
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
    vec![path, log, undo]
}

/// The question's title and its yes button.
fn words(action: RowAction) -> (String, &'static str) {
    match action {
        RowAction::Run(Command::AddToPath) => ("Add gezik to PATH?".into(), "Add"),
        RowAction::Run(Command::RemoveFromPath) => ("Remove gezik from PATH?".into(), "Remove"),
        RowAction::Run(Command::UndoAll) => ("Undo all system changes?".into(), "Undo All"),
        _ => ("Repair gezik on PATH?".into(), "Repair"),
    }
}

/// What adding writes, a line per place (one of `path_targets`).
fn will_write(target: &Change) -> String {
    let what = sc::what(target);
    match (target.kind, &target.after) {
        (Kind::Folder, _) => format!("the folder {what}, if it is not there"),
        (Kind::RegistryKey, _) => format!("the registry key {what}, if it is not there"),
        (Kind::File, _) => {
            format!("the file {what}, a script that runs: {}", changes::SHIM.lines().last().unwrap_or_default())
        }
        (Kind::RegistryValue, Value::Reg { data, .. }) => format!("{what} = \"{data}\""),
        (Kind::PathEntry, _) => {
            format!("{} at the end of {what}; every other entry stays as it is", target.entry)
        }
        (Kind::Symlink, Value::Link(to)) => format!("the link {what}, to {to}"),
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
pub fn confirmation(
    action: RowAction,
    snapshot: &Snapshot,
    add: Option<&[Change]>,
    sweep: &[Change],
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
    match action {
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

/// A palette command or a panel button: asks first.
pub fn run(command: Command) {
    confirm(RowAction::Run(command));
}

/// Reads what is there now on a thread, asks with exactly what will be written, then does it.
fn confirm(action: RowAction) {
    std::thread::spawn(move || {
        let snapshot = changes::read_snapshot();
        let access = changes::SystemAccess::new();
        let exe = gezik_platform::system::exe().unwrap_or_default();
        let add = exe.to_str().and_then(|e| changes::path_targets(&access.places, e, cfg!(windows)));
        let sweep = if action == RowAction::Run(Command::UndoAll) {
            changes::sweep_changes(&access, &access.places, &exe, cfg!(windows))
        } else {
            Vec::new()
        };
        let asked = confirmation(action, &snapshot, add.as_deref(), &sweep);
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
        RowAction::Show | RowAction::Nothing => {}
    }
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
            changes: changes.map(|n| vec![change; n]).map_err(str::to_owned),
            journal: PathBuf::from("/c/system-changes.toml"),
        }
    }

    fn shape(snapshot: &Snapshot) -> Vec<(String, &'static str, &'static str, RowAction)> {
        rows(snapshot).into_iter().map(|(r, a)| (r.title, r.tag, r.button, a)).collect()
    }

    #[test]
    fn rows_follow_the_state() {
        let off = shape(&snap(PathState::Off, Ok(0)));
        assert_eq!(off[0], ("Command line (PATH)".into(), "Off", "Add", RowAction::Run(Command::AddToPath)));
        assert_eq!(off[1], ("Changes made: 0".into(), "", "", RowAction::Nothing));
        assert_eq!(off[2], ("Undo all system changes".into(), "", "Undo all", RowAction::Run(Command::UndoAll)));
        let on = shape(&snap(PathState::On { command: "/h/.local/bin/gezik".into() }, Ok(3)));
        assert_eq!((on[0].1, on[0].2), ("On", "Remove"));
        assert_eq!(on[0].3, RowAction::Run(Command::RemoveFromPath));
        assert_eq!(on[1], ("Changes made: 3 (system-changes.toml)".into(), "", "Show", RowAction::Show));
        assert_eq!(shape(&snap(PathState::Moved { old: "/old".into() }, Ok(3)))[0].2, "Update");
        assert_eq!(shape(&snap(PathState::Changed, Ok(3)))[0].3, RowAction::Repair);
        assert_eq!(shape(&snap(PathState::Taken { place: "/p".into() }, Ok(0)))[0].3, RowAction::Nothing);
        let bad = shape(&snap(PathState::Off, Err("system-changes.toml cannot be read: version 9")));
        assert_eq!(bad[0].3, RowAction::Nothing, "nothing is written next to a journal that cannot be read");
        assert_eq!(bad[1].3, RowAction::Nothing);
    }

    #[test]
    fn every_command_has_a_title() {
        let titles: Vec<&str> = Command::ALL.iter().map(|c| c.title()).collect();
        assert_eq!(titles, ["Add gezik to PATH", "Remove gezik from PATH", "Undo all system changes"]);
    }

    #[test]
    fn the_question_names_what_is_written() {
        let places = Places { home: None, local_app_data: Some(PathBuf::from(r"C:\L")) };
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
}
