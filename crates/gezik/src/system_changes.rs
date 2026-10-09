//! The system changes Gezik makes on an explicit command and their undoing (spec 8.2, 8.4,
//! 11): what PATH registration writes, the write-ahead apply, the undo, `--unregister`, the
//! sweep by fixed names when the journal is lost, and the PATH row's state for the panel.
//! No UI here (integration.rs is the panel). Runs off the UI thread: the registry, files
//! and a broadcast that may wait for hung windows. What the panel showed is never acted on:
//! every write and every undo reads what is there now, under the journal's lock.

use std::io;
use std::path::{Path, PathBuf};

use gezik_config::system_journal::{FILE, JournalFile, Locked};
use gezik_core::system_change::{self as sc, Access, Change, Kind, Outcome, RegType, Value};
use gezik_platform::system::Places;

pub const FEATURE_PATH: &str = "path";
/// `gezik.cmd` (decision 3): no path in it, so no code page can break it; `start` finds
/// gezik.exe through App Paths, which holds the exe's path as Unicode. `""` is start's
/// window title; `%*` the arguments as the shell gave them. `start` looks for `gezik.exe`
/// as typing `gezik` in cmd does (the current folder first): no new planting risk.
pub const SHIM: &str = "@echo off\r\nrem Made by Gezik for the gezik command; gezik --unregister removes it.\r\nstart \"\" gezik.exe %*\r\n";
/// What to add to a shell profile on macOS and Linux (spec 8.2: Gezik does not edit them).
pub const PATH_LINE: &str = r#"export PATH="$HOME/.local/bin:$PATH""#;
const APP_PATHS: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\App Paths";
const APP_PATH: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\App Paths\gezik.exe";
const ENVIRONMENT: &str = r"HKCU\Environment";
/// The environment block's limit for one variable, in UTF-16 units.
const MAX_PATH_VALUE: usize = 32_767;
/// Gezik's files are a few lines; a larger file at their place is not Gezik's.
const MAX_TEXT: u64 = 64 * 1024;

fn target(kind: Kind, place: impl Into<String>, name: &str, entry: &str, after: Value) -> Change {
    Change {
        feature: FEATURE_PATH.into(),
        kind,
        place: place.into(),
        name: name.into(),
        entry: entry.into(),
        before: Value::Absent,
        after,
        done: false,
    }
}

/// What `gezik` on the command line needs, in the order it is made (spec 8.2): on Windows
/// the shim, App Paths and the user's PATH; on macOS and Linux a `~/.local/bin/gezik` link.
/// Every folder and key Gezik may make is a step of its own (decision 7). A path-entry's
/// `after` is worked out from the list there when it is made. `None`: the folder is unknown.
pub fn path_targets(places: &Places, exe: &str, windows: bool) -> Option<Vec<Change>> {
    if windows {
        let local = places.local_app_data.as_ref()?.to_str()?;
        let gezik = format!(r"{local}\Gezik");
        let bin = format!(r"{gezik}\bin");
        return Some(vec![
            target(Kind::Folder, gezik.as_str(), "", "", Value::Present),
            target(Kind::Folder, bin.as_str(), "", "", Value::Present),
            target(Kind::File, format!(r"{bin}\gezik.cmd"), "", "", Value::Text(SHIM.into())),
            target(Kind::RegistryKey, APP_PATHS, "", "", Value::Present),
            target(Kind::RegistryKey, APP_PATH, "", "", Value::Present),
            target(Kind::RegistryValue, APP_PATH, "", "", Value::Reg { ty: RegType::Sz, data: exe.into() }),
            target(Kind::PathEntry, ENVIRONMENT, "Path", &bin, Value::Absent),
        ]);
    }
    let home = places.home.as_ref()?.to_str()?;
    let local = format!("{home}/.local");
    let bin = format!("{local}/bin");
    Some(vec![
        target(Kind::Folder, local.as_str(), "", "", Value::Present),
        target(Kind::Folder, bin.as_str(), "", "", Value::Present),
        target(Kind::Symlink, format!("{bin}/gezik"), "", "", Value::Link(exe.into())),
    ])
}

/// Whether `change` is at a place Gezik writes (decision 8): a journal edited by hand or by
/// something else cannot point Gezik at anything else. Kind, place, name and entry must all
/// be one of [`path_targets`]'s (Windows without case).
pub fn allowed(change: &Change, places: &Places, windows: bool) -> bool {
    let Some(targets) = path_targets(places, "", windows) else { return false };
    let same = |a: &str, b: &str| if windows { a.to_lowercase() == b.to_lowercase() } else { a == b };
    targets.iter().any(|t| {
        t.kind == change.kind
            && same(&t.place, &change.place)
            && same(&t.name, &change.name)
            && same(&t.entry, &change.entry)
    })
}

/// The system as Gezik reads and writes it, only at [`allowed`] places.
pub struct SystemAccess {
    pub places: Places,
    /// Tests: the HKCU keys under this key instead (`Software\GezikTest-…`); "" is HKCU.
    pub registry_root: String,
}

impl SystemAccess {
    pub fn new() -> SystemAccess {
        SystemAccess { places: gezik_platform::system::places(), registry_root: String::new() }
    }

    fn check(&self, change: &Change) -> io::Result<()> {
        if allowed(change, &self.places, cfg!(windows)) {
            Ok(())
        } else {
            Err(io::Error::new(io::ErrorKind::PermissionDenied, "not a place Gezik writes; left as is"))
        }
    }

    /// `HKCU\X` → the key `X` under HKCU (or under the test root); nothing else is a key.
    #[cfg_attr(not(windows), allow(dead_code))]
    fn key(&self, place: &str) -> io::Result<String> {
        let rest = place
            .get(..5)
            .filter(|root| root.eq_ignore_ascii_case(r"HKCU\"))
            .and_then(|_| place.get(5..))
            .ok_or_else(|| io::Error::new(io::ErrorKind::PermissionDenied, "only HKCU"))?;
        Ok(if self.registry_root.is_empty() { rest.to_owned() } else { format!(r"{}\{rest}", self.registry_root) })
    }
}

impl Access for SystemAccess {
    fn current(&self, change: &Change) -> io::Result<Value> {
        self.check(change)?;
        match change.kind {
            Kind::RegistryValue | Kind::PathEntry | Kind::RegistryKey => {
                registry_current(&self.key(&change.place)?, change.kind, &change.name)
            }
            Kind::File | Kind::Symlink | Kind::Folder => file_current(Path::new(&change.place), change.kind),
            // Not yet a place Gezik writes: `check` already refused it.
            Kind::MacDefault | Kind::MacPref | Kind::Mimeapps => Err(io::ErrorKind::Unsupported.into()),
        }
    }

    fn set(&self, change: &Change, value: &Value) -> io::Result<()> {
        self.check(change)?;
        // The journal's values are not trusted either: only what Gezik itself would write.
        if !value_allowed(change, &self.current(change)?, value) {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "not a value Gezik writes; left as is"));
        }
        match change.kind {
            Kind::RegistryValue | Kind::PathEntry | Kind::RegistryKey => {
                registry_set(&self.key(&change.place)?, change.kind, &change.name, value)
            }
            Kind::File | Kind::Symlink | Kind::Folder => file_set(change, value),
            Kind::MacDefault | Kind::MacPref | Kind::Mimeapps => Err(io::ErrorKind::Unsupported.into()),
        }
    }
}

/// Whether putting `value` where `now` is, is a write Gezik makes (decision 8): the shim,
/// a link to a `gezik`, an App Paths value naming `gezik.exe`, PATH with only Gezik's entry
/// added or taken out (the value deleted only when Gezik's entry is all there is).
fn value_allowed(change: &Change, now: &Value, value: &Value) -> bool {
    let entry = change.entry.as_str();
    match (change.kind, value) {
        (Kind::PathEntry, _) if entry.is_empty() => false,
        (Kind::PathEntry, Value::Absent) => matches!(now, Value::Reg { data, .. } if data == entry),
        (_, Value::Absent) | (Kind::Folder | Kind::RegistryKey, Value::Present) => true,
        (Kind::File, Value::Text(text)) => text == SHIM,
        // Links are macOS and Linux only: absolute is a leading `/`.
        (Kind::Symlink, Value::Link(to)) => to.starts_with('/') && to.rsplit('/').next() == Some("gezik"),
        (Kind::RegistryValue, Value::Reg { ty: RegType::Sz, data }) => names_gezik_exe(data),
        (Kind::PathEntry, Value::Reg { ty, data }) => match now {
            Value::Absent => data == entry,
            Value::Reg { ty: now_ty, data: list } => {
                ty == now_ty
                    && (sc::with_entry(list, entry, expand).as_deref() == Some(data)
                        || sc::without_entry(list, entry).as_deref() == Some(data)
                        || sc::with_entry(data, entry, str::to_owned).as_deref() == Some(list))
            }
            _ => false,
        },
        _ => false,
    }
}

#[cfg(windows)]
fn registry_current(key: &str, kind: Kind, name: &str) -> io::Result<Value> {
    use gezik_platform::system::windows as reg;
    if kind == Kind::RegistryKey {
        return Ok(if reg::key_exists(key)? { Value::Present } else { Value::Absent });
    }
    Ok(match reg::read_value(key, name)? {
        None => Value::Absent,
        Some((ty, data)) => Value::Reg { ty, data },
    })
}

#[cfg(windows)]
fn registry_set(key: &str, kind: Kind, name: &str, value: &Value) -> io::Result<()> {
    use gezik_platform::system::windows as reg;
    match (kind, value) {
        (Kind::RegistryKey, Value::Present) => reg::create_key(key),
        (Kind::RegistryKey, Value::Absent) => reg::delete_empty_key(key),
        (Kind::RegistryValue | Kind::PathEntry, Value::Reg { ty, data }) => reg::write_value(key, name, *ty, data),
        (Kind::RegistryValue | Kind::PathEntry, Value::Absent) => reg::delete_value(key, name),
        _ => Err(io::Error::new(io::ErrorKind::InvalidInput, "Gezik does not write this")),
    }
}

#[cfg(not(windows))]
fn registry_current(_: &str, _: Kind, _: &str) -> io::Result<Value> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "no registry here"))
}

#[cfg(not(windows))]
fn registry_set(_: &str, _: Kind, _: &str, _: &Value) -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "no registry here"))
}

/// What is at `path`, never through a link (decision 9).
fn file_current(path: &Path, kind: Kind) -> io::Result<Value> {
    let meta = match std::fs::symlink_metadata(path) {
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Value::Absent),
        other => other?,
    };
    Ok(match kind {
        Kind::Symlink if meta.file_type().is_symlink() => {
            Value::Link(std::fs::read_link(path)?.to_string_lossy().into_owned())
        }
        Kind::Folder if meta.is_dir() => Value::Present,
        Kind::File if meta.is_file() && meta.len() <= MAX_TEXT => {
            std::fs::read_to_string(path).map_or(Value::Other, Value::Text)
        }
        _ => Value::Other,
    })
}

fn file_set(change: &Change, value: &Value) -> io::Result<()> {
    let path = Path::new(&change.place);
    match (change.kind, value) {
        (Kind::Folder, Value::Present) => gezik_platform::system::make_folder(path),
        (Kind::Folder, Value::Absent) => std::fs::remove_dir(path),
        (Kind::File, Value::Text(text)) => gezik_config::paths::write_atomic(path, text),
        (Kind::File | Kind::Symlink, Value::Absent) => std::fs::remove_file(path),
        #[cfg(unix)]
        (Kind::Symlink, Value::Link(target)) => {
            // Only over what the journal says is there: nothing, or Gezik's earlier link.
            let expect = if *value == change.after { &change.before } else { &change.after };
            let expect = match expect {
                Value::Absent => None,
                Value::Link(old) => Some(Path::new(old.as_str())),
                _ => return Err(io::Error::new(io::ErrorKind::AlreadyExists, "not Gezik's link")),
            };
            gezik_platform::system::replace_symlink(path, Path::new(target), expect)
        }
        _ => Err(io::Error::new(io::ErrorKind::InvalidInput, "Gezik does not write this")),
    }
}

/// `%NAME%` opened from this process's environment; unknown names stay as written.
fn expand(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('%') else {
            rest = &rest[start..];
            break;
        };
        let name = &after[..end];
        match std::env::var(name) {
            Ok(value) if !name.is_empty() => out.push_str(&value),
            _ => {
                out.push('%');
                out.push_str(name);
                out.push('%');
            }
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Puts `targets` in place, each written to the journal before it is made (decision 6).
/// What is there already is skipped; a file or link Gezik did not make stops it (decision
/// 9). Stops at the first error; what was made stays in the journal. `Ok(true)`: the
/// user's PATH changed (the caller tells the system).
pub fn apply(locked: &mut Locked, access: &dyn Access, targets: Vec<Change>, exe: &str) -> Result<bool, String> {
    let mut path_changed = false;
    for target in targets {
        let failed = |err: io::Error| format!("{}: {err}", sc::what(&target));
        let now = access.current(&target).map_err(failed)?;
        let after = match target.kind {
            Kind::Folder | Kind::RegistryKey if now != Value::Absent => continue,
            Kind::PathEntry => {
                if target.entry.is_empty() {
                    return Err("no folder to add to PATH".into());
                }
                let (ty, list) = match &now {
                    Value::Reg { ty, data } => (*ty, data.as_str()),
                    _ => (RegType::ExpandSz, ""),
                };
                let Some(list) = sc::with_entry(list, &target.entry, expand) else { continue };
                if list.encode_utf16().count() >= MAX_PATH_VALUE {
                    return Err("PATH is too long to add Gezik's folder to".into());
                }
                Value::Reg { ty, data: list }
            }
            _ => target.after.clone(),
        };
        if now == after {
            continue;
        }
        let foreign = match target.kind {
            Kind::File | Kind::Symlink => !made_by_gezik(locked, &target, &now),
            // Only a gezik.exe is ever written back here by an undo.
            Kind::RegistryValue => !value_allowed(&target, &now, &now),
            _ => false,
        };
        if now != Value::Absent && foreign {
            return Err(format!("{} is there and was not made by Gezik; it is left alone", target.place));
        }
        let kind = target.kind;
        let change = Change { before: now, after, ..target };
        let what = sc::what(&change);
        if let Err(err) = locked.record(change, exe, |c| access.set(c, &c.after)) {
            if err.kind() == io::ErrorKind::AlreadyExists {
                // Something not Gezik's took the place meanwhile: nothing was made, so no entry.
                let last = locked.journal().changes.len().saturating_sub(1);
                let _ = locked.retain(|i, c| i != last || c.done);
                return Err(format!("{what} is there and was not made by Gezik; it is left alone"));
            }
            return Err(format!("{what}: {err}"));
        }
        path_changed |= kind == Kind::PathEntry;
    }
    Ok(path_changed)
}

/// Whether `now` is what an earlier finished change of the journal put at that place.
fn made_by_gezik(locked: &Locked, target: &Change, now: &Value) -> bool {
    locked
        .journal()
        .changes
        .iter()
        .any(|c| c.done && c.kind == target.kind && c.place == target.place && c.after == *now)
}

/// What an undo did: its report lines (newest first), outcomes, and whether PATH changed.
#[derive(Debug, Default)]
pub struct Undone {
    pub lines: Vec<String>,
    pub outcomes: Vec<Outcome>,
    pub path_changed: bool,
}

fn report(changes: &[Change], outcomes: Vec<Outcome>) -> Undone {
    let lines = changes.iter().zip(&outcomes).rev().map(|(c, o)| sc::report_line(c, o)).collect();
    let path_changed = changes
        .iter()
        .zip(&outcomes)
        .any(|(c, o)| c.kind == Kind::PathEntry && matches!(o, Outcome::Undone | Outcome::EntryRemoved));
    Undone { lines, outcomes, path_changed }
}

/// Undoes the journal's changes `which` picks, newest first (spec 11.4), and forgets the
/// settled ones (decision 12); a failed one stays for another try.
pub fn undo_matching(locked: &mut Locked, access: &dyn Access, which: impl Fn(&Change) -> bool) -> Undone {
    let picked: Vec<usize> =
        locked.journal().changes.iter().enumerate().filter(|(_, c)| which(c)).map(|(i, _)| i).collect();
    let changes: Vec<Change> = picked.iter().map(|&i| locked.journal().changes[i].clone()).collect();
    let mut undone = report(&changes, sc::undo(&changes, access));
    let forget: Vec<usize> =
        picked.iter().zip(&undone.outcomes).filter(|(_, o)| o.settled()).map(|(&i, _)| i).collect();
    if let Err(err) = locked.retain(|i, _| !forget.contains(&i)) {
        undone.lines.push(format!("{FILE} could not be written: {err}"));
        undone.outcomes.push(Outcome::Failed(err.to_string()));
    }
    undone
}

/// No journal (decision 13): only Gezik's names in Gezik's shapes, as changes to undo.
pub fn sweep_changes(access: &dyn Access, places: &Places, exe: &Path, windows: bool) -> Vec<Change> {
    let Some(targets) = path_targets(places, "", windows) else { return Vec::new() };
    let mut out = Vec::new();
    for target in targets {
        // Not named Gezik: Gezik cannot know it made them.
        if target.place == APP_PATHS || (!windows && target.kind == Kind::Folder) {
            continue;
        }
        let Ok(now) = access.current(&target) else { continue };
        let before = match (target.kind, &now) {
            (Kind::Folder | Kind::RegistryKey, Value::Present) => Value::Absent,
            (Kind::File, Value::Text(text)) if text == SHIM => Value::Absent,
            (Kind::RegistryValue, Value::Reg { data, .. }) if names_gezik_exe(data) => Value::Absent,
            (Kind::PathEntry, Value::Reg { ty, data }) => match sc::without_entry(data, &target.entry) {
                Some(rest) => Value::Reg { ty: *ty, data: rest },
                None => continue,
            },
            (Kind::Symlink, Value::Link(to)) if gezik_link(Path::new(to), exe) => Value::Absent,
            _ => continue,
        };
        out.push(Change { before, after: now, done: true, ..target });
    }
    out
}

/// A Windows path's last part is `gezik.exe` (split by hand: the tests run on every system).
fn names_gezik_exe(data: &str) -> bool {
    data.trim_matches('"').rsplit(['\\', '/']).next().is_some_and(|name| name.eq_ignore_ascii_case("gezik.exe"))
}

/// A link to this Gezik, or to a Gezik exe of this name that is gone (the exe moved).
/// Gezik writes absolute targets only.
fn gezik_link(to: &Path, exe: &Path) -> bool {
    to == exe || (to.is_absolute() && !to.exists() && exe.file_name().is_some() && to.file_name() == exe.file_name())
}

/// `--unregister`'s result (spec 11.4).
#[derive(Debug)]
pub struct Report {
    pub lines: Vec<String>,
    pub code: i32,
    pub path_changed: bool,
}

/// Undoes everything in the journal; with no journal, the sweep. An unreadable journal
/// changes nothing (code 2).
pub fn unregister(
    file: Option<&JournalFile>,
    access: &dyn Access,
    places: &Places,
    exe: &Path,
    windows: bool,
) -> Report {
    // The lock also for the sweep: no other Gezik adds to PATH meanwhile.
    let undone = match file.map(JournalFile::lock) {
        Some(Err(err)) => {
            return Report { lines: vec![format!("{err}; nothing was changed")], code: 2, path_changed: false };
        }
        Some(Ok(mut locked)) if !locked.journal().changes.is_empty() => undo_matching(&mut locked, access, |_| true),
        Some(Ok(_locked)) => sweep(access, places, exe, windows),
        None => sweep(access, places, exe, windows),
    };
    Report { code: sc::exit_code(&undone.outcomes), lines: undone.lines, path_changed: undone.path_changed }
}

fn sweep(access: &dyn Access, places: &Places, exe: &Path, windows: bool) -> Undone {
    let changes = sweep_changes(access, places, exe, windows);
    let mut undone = report(&changes, sc::undo(&changes, access));
    undone.lines.insert(0, format!("No {FILE}: taking back what has Gezik's names"));
    if changes.is_empty() {
        undone.lines.push("Nothing of Gezik's was found.".into());
    }
    undone
}

/// The command line row (spec 3.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathState {
    Off,
    /// `command`: the shim or the link.
    On {
        command: String,
    },
    /// Made by Gezik for an exe that is not this one (spec 6.3).
    Moved {
        old: String,
    },
    /// Something else changed what Gezik wrote.
    Changed,
    /// Gezik made nothing, and the place is taken (decision 9).
    Taken {
        place: String,
    },
}

/// The row from the journal's finished PATH changes paired with what is there now.
pub fn path_state(made: &[(Change, Value)], exe: &str, taken: Option<String>, windows: bool) -> PathState {
    let made: Vec<&(Change, Value)> = made.iter().filter(|(c, _)| c.done && c.feature == FEATURE_PATH).collect();
    if made.is_empty() {
        return taken.map_or(PathState::Off, |place| PathState::Taken { place });
    }
    // Stopped half way (a file that is not Gezik's, say): Repair or Remove.
    if !made.iter().any(|(c, _)| matches!(c.kind, Kind::File | Kind::Symlink)) {
        return PathState::Changed;
    }
    let still = |(c, now): &&(Change, Value)| match (c.kind, now) {
        // Other parts of PATH may change; Gezik's entry must be there.
        (Kind::PathEntry, Value::Reg { data, .. }) => sc::contains_entry(data, &c.entry),
        (Kind::Folder | Kind::RegistryKey, _) => *now != Value::Absent,
        _ => *now == c.after,
    };
    if !made.iter().all(still) {
        return PathState::Changed;
    }
    let same = |a: &str, b: &str| if windows { a.to_lowercase() == b.to_lowercase() } else { a == b };
    let points_to = made.iter().find_map(|(c, _)| match (c.kind, &c.after) {
        (Kind::RegistryValue, Value::Reg { data, .. }) | (Kind::Symlink, Value::Link(data)) => Some(data.clone()),
        _ => None,
    });
    if let Some(old) = points_to
        && !same(&old, exe)
    {
        return PathState::Moved { old };
    }
    let command = made.iter().find(|(c, _)| matches!(c.kind, Kind::File | Kind::Symlink)).map(|(c, _)| c.place.clone());
    PathState::On { command: command.unwrap_or_default() }
}

/// What the panel shows; only shown, never acted on.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub path: PathState,
    pub changes: Result<Vec<Change>, String>,
    pub journal: PathBuf,
}

pub fn snapshot(
    file: Option<&JournalFile>,
    access: &dyn Access,
    places: &Places,
    exe: &str,
    windows: bool,
) -> Snapshot {
    let changes = match file {
        Some(file) if file.exists() => file.lock().map(|l| l.journal().changes.clone()).map_err(|e| e.to_string()),
        _ => Ok(Vec::new()),
    };
    let made: Vec<(Change, Value)> = changes
        .as_ref()
        .map(|list| {
            list.iter()
                .filter(|c| c.feature == FEATURE_PATH)
                .map(|c| (c.clone(), access.current(c).unwrap_or(Value::Other)))
                .collect()
        })
        .unwrap_or_default();
    let taken = path_targets(places, exe, windows).and_then(|targets| {
        targets
            .into_iter()
            .filter(|t| matches!(t.kind, Kind::File | Kind::Symlink))
            .find(|t| access.current(t).is_ok_and(|now| now != Value::Absent))
            .map(|t| t.place)
    });
    Snapshot {
        path: path_state(&made, exe, taken, windows),
        changes,
        journal: file.map(JournalFile::path).unwrap_or_default(),
    }
}

fn journal_file() -> Option<JournalFile> {
    gezik_config::paths::config_dir().map(JournalFile::new)
}

fn exe_text() -> Result<String, String> {
    let exe = gezik_platform::system::exe().map_err(|e| format!("Gezik's own path: {e}"))?;
    exe.to_str()
        .map(str::to_owned)
        .ok_or_else(|| "Gezik's own path is not Unicode text; it cannot be written down".into())
}

pub fn read_snapshot() -> Snapshot {
    let access = SystemAccess::new();
    snapshot(journal_file().as_ref(), &access, &access.places, &exe_text().unwrap_or_default(), cfg!(windows))
}

/// Add gezik to PATH (spec 8.2).
pub fn add_now() -> Result<(), String> {
    let file = journal_file().ok_or("there is no config folder to keep system-changes.toml in")?;
    let access = SystemAccess::new();
    let exe = exe_text()?;
    let targets = path_targets(&access.places, &exe, cfg!(windows)).ok_or("the user's folder is not known")?;
    let mut locked = file.lock().map_err(|e| e.to_string())?;
    let changed = apply(&mut locked, &access, targets, &exe)?;
    drop(locked);
    if changed {
        gezik_platform::system::environment_changed();
    }
    Ok(())
}

/// Remove gezik from PATH: Gezik's PATH changes undone.
pub fn remove_now() -> Result<Vec<String>, String> {
    let file = journal_file().ok_or("there is no config folder")?;
    let access = SystemAccess::new();
    let mut locked = file.lock().map_err(|e| e.to_string())?;
    let undone = undo_matching(&mut locked, &access, |c| c.feature == FEATURE_PATH);
    drop(locked);
    if undone.path_changed {
        gezik_platform::system::environment_changed();
    }
    Ok(undone.lines)
}

/// Repair and Update: Gezik's PATH changes undone, then made again for this exe.
pub fn repair_now() -> Result<Vec<String>, String> {
    let mut lines = remove_now()?;
    add_now()?;
    lines.push("added again for this Gezik".into());
    Ok(lines)
}

pub fn undo_all_now() -> Report {
    let access = SystemAccess::new();
    let exe = gezik_platform::system::exe().unwrap_or_default();
    let report = unregister(journal_file().as_ref(), &access, &access.places, &exe, cfg!(windows));
    if report.path_changed {
        gezik_platform::system::environment_changed();
    }
    report
}

/// macOS and Linux (decision 17): the line to add when `~/.local/bin` may not be in the
/// shell's PATH; macOS always (an app opened from the Finder does not see the shell's).
pub fn path_hint() -> Option<&'static str> {
    if cfg!(windows) {
        return None;
    }
    let bin = gezik_platform::system::places().home?.join(".local").join("bin");
    (cfg!(target_os = "macos") || !gezik_platform::system::in_path_env(&bin)).then_some(PATH_LINE)
}

/// `gezik --unregister` (spec 11.4): each line on the console; the exit code.
pub fn unregister_cli() -> i32 {
    let report = undo_all_now();
    for line in &report.lines {
        println!("{line}");
    }
    report.code
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    const LAD: &str = r"C:\Users\u\AppData\Local";
    const BIN: &str = r"C:\Users\u\AppData\Local\Gezik\bin";
    const CMD: &str = r"C:\Users\u\AppData\Local\Gezik\bin\gezik.cmd";

    fn win_places() -> Places {
        Places { home: Some(PathBuf::from(r"C:\Users\u")), local_app_data: Some(PathBuf::from(LAD)) }
    }

    /// The system as a map, keyed like Windows (case does not matter).
    #[derive(Default)]
    struct Fake(RefCell<BTreeMap<(String, String, String), Value>>);

    fn key(c: &Change) -> (String, String, String) {
        (c.kind.name().to_owned(), c.place.to_lowercase(), c.name.to_lowercase())
    }

    impl Fake {
        fn put(&self, kind: Kind, place: &str, name: &str, value: Value) {
            self.0.borrow_mut().insert((kind.name().to_owned(), place.to_lowercase(), name.to_lowercase()), value);
        }
        fn get(&self, kind: Kind, place: &str, name: &str) -> Option<Value> {
            self.0.borrow().get(&(kind.name().to_owned(), place.to_lowercase(), name.to_lowercase())).cloned()
        }
    }

    impl Access for Fake {
        fn current(&self, c: &Change) -> io::Result<Value> {
            Ok(self.0.borrow().get(&key(c)).cloned().unwrap_or(Value::Absent))
        }
        fn set(&self, c: &Change, value: &Value) -> io::Result<()> {
            match value {
                Value::Absent => self.0.borrow_mut().remove(&key(c)),
                other => self.0.borrow_mut().insert(key(c), other.clone()),
            };
            Ok(())
        }
    }

    fn journal(name: &str) -> JournalFile {
        let dir = std::env::temp_dir().join(format!("gezik-system-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        JournalFile::new(dir)
    }

    fn reg(data: &str) -> Value {
        Value::Reg { ty: RegType::Sz, data: data.into() }
    }

    fn add(fake: &Fake, file: &JournalFile, exe: &str) -> Result<bool, String> {
        let targets = path_targets(&win_places(), exe, true).unwrap();
        apply(&mut file.lock().map_err(|e| e.to_string())?, fake, targets, exe)
    }

    #[test]
    fn path_targets_make_every_level() {
        let windows = path_targets(&win_places(), r"C:\T\gezik.exe", true).unwrap();
        let shape: Vec<(Kind, &str)> = windows.iter().map(|t| (t.kind, t.place.as_str())).collect();
        assert_eq!(
            shape,
            [
                (Kind::Folder, r"C:\Users\u\AppData\Local\Gezik"),
                (Kind::Folder, BIN),
                (Kind::File, CMD),
                (Kind::RegistryKey, APP_PATHS),
                (Kind::RegistryKey, APP_PATH),
                (Kind::RegistryValue, APP_PATH),
                (Kind::PathEntry, ENVIRONMENT),
            ]
        );
        assert_eq!(windows[6].entry, BIN, "the folder spelled out, never %LOCALAPPDATA%");
        assert_eq!(windows[5].after, reg(r"C:\T\gezik.exe"));
        let unix = Places { home: Some(PathBuf::from("/home/u")), local_app_data: None };
        let unix = path_targets(&unix, "/opt/gezik/gezik", false).unwrap();
        let shape: Vec<(Kind, &str)> = unix.iter().map(|t| (t.kind, t.place.as_str())).collect();
        assert_eq!(
            shape,
            [
                (Kind::Folder, "/home/u/.local"),
                (Kind::Folder, "/home/u/.local/bin"),
                (Kind::Symlink, "/home/u/.local/bin/gezik")
            ]
        );
        assert!(path_targets(&Places::default(), "x", true).is_none());
    }

    #[test]
    fn the_shim_names_no_path_and_forwards_every_argument() {
        assert!(SHIM.contains("start \"\" gezik.exe %*\r\n"));
        assert!(!SHIM.contains(':') && SHIM.is_ascii(), "no path, no code page trouble");
        assert!(SHIM.lines().all(|line| !line.contains("%~")), "arguments go as typed");
    }

    #[test]
    fn places_outside_the_list_are_refused() {
        let places = win_places();
        let mut ok = path_targets(&places, "", true).unwrap();
        for target in &ok {
            assert!(allowed(target, &places, true), "{target:?}");
        }
        let upper = Change { place: CMD.to_uppercase(), ..ok[2].clone() };
        assert!(allowed(&upper, &places, true), "Windows paths without case");
        let file = ok.remove(2);
        for bad in [
            Change { place: r"C:\Windows\gezik.cmd".into(), ..file.clone() },
            Change {
                place: r"HKLM\Software\Microsoft\Windows\CurrentVersion\App Paths\gezik.exe".into(),
                ..ok[3].clone()
            },
            Change { place: r"HKCU\Software\Other".into(), ..ok[3].clone() },
            Change { entry: r"C:\Windows\System32".into(), ..ok[5].clone() },
            Change { name: "Other".into(), ..ok[5].clone() },
            Change { kind: Kind::Folder, ..file },
        ] {
            assert!(!allowed(&bad, &places, true), "{bad:?}");
        }
    }

    #[test]
    fn apply_records_before_and_skips_what_is_there() {
        let fake = Fake::default();
        fake.put(Kind::Folder, r"C:\Users\u\AppData\Local\Gezik", "", Value::Present);
        fake.put(Kind::RegistryKey, APP_PATHS, "", Value::Present);
        fake.put(Kind::PathEntry, ENVIRONMENT, "Path", reg(r"C:\a;"));
        let file = journal("apply");
        assert_eq!(add(&fake, &file, r"C:\T\gezik.exe"), Ok(true));
        let locked = file.lock().unwrap();
        let made: Vec<(Kind, &Value)> = locked.journal().changes.iter().map(|c| (c.kind, &c.before)).collect();
        assert_eq!(
            made,
            [
                (Kind::Folder, &Value::Absent),
                (Kind::File, &Value::Absent),
                (Kind::RegistryKey, &Value::Absent),
                (Kind::RegistryValue, &Value::Absent),
                (Kind::PathEntry, &reg(r"C:\a;")),
            ],
            "what was there already is no entry"
        );
        assert!(locked.journal().changes.iter().all(|c| c.done));
        assert_eq!(fake.get(Kind::PathEntry, ENVIRONMENT, "Path"), Some(reg(&format!(r"C:\a;{BIN}"))));
        drop(locked);
        assert_eq!(add(&fake, &file, r"C:\T\gezik.exe"), Ok(false), "a second time: nothing to do");
        assert_eq!(file.lock().unwrap().journal().changes.len(), 5);
    }

    #[test]
    fn a_path_too_long_is_not_written() {
        let fake = Fake::default();
        fake.put(Kind::PathEntry, ENVIRONMENT, "Path", reg(&"x".repeat(32_760)));
        let err = add(&fake, &journal("long"), r"C:\T\gezik.exe").unwrap_err();
        assert!(err.contains("too long"), "{err}");
        assert_eq!(fake.get(Kind::PathEntry, ENVIRONMENT, "Path"), Some(reg(&"x".repeat(32_760))));
    }

    #[test]
    fn a_place_taken_meanwhile_leaves_no_entry() {
        /// `replace_symlink`'s answer when something not Gezik's appeared there.
        struct Taken(Fake);
        impl Access for Taken {
            fn current(&self, c: &Change) -> io::Result<Value> {
                self.0.current(c)
            }
            fn set(&self, c: &Change, value: &Value) -> io::Result<()> {
                if c.kind == Kind::File {
                    return Err(io::Error::new(io::ErrorKind::AlreadyExists, "not Gezik's"));
                }
                self.0.set(c, value)
            }
        }
        let file = journal("taken-meanwhile");
        let exe = r"C:\T\gezik.exe";
        let targets = path_targets(&win_places(), exe, true).unwrap();
        let err = apply(&mut file.lock().unwrap(), &Taken(Fake::default()), targets, exe).unwrap_err();
        assert!(err.contains("not made by Gezik"), "{err}");
        let locked = file.lock().unwrap();
        assert!(locked.journal().changes.iter().all(|c| c.done && c.kind == Kind::Folder), "only the folders made");
    }

    #[test]
    fn a_foreign_file_is_left_alone() {
        let fake = Fake::default();
        fake.put(Kind::File, CMD, "", Value::Text("@echo mine".into()));
        let file = journal("foreign");
        let err = add(&fake, &file, r"C:\T\gezik.exe").unwrap_err();
        assert!(err.contains("not made by Gezik"), "{err}");
        assert_eq!(fake.get(Kind::File, CMD, ""), Some(Value::Text("@echo mine".into())));
        assert!(fake.get(Kind::PathEntry, ENVIRONMENT, "Path").is_none(), "nothing after it was made");
        let state = snapshot(Some(&file), &fake, &win_places(), r"C:\T\gezik.exe", true);
        assert_eq!(state.path, PathState::Changed, "the folders it made are Gezik's; Remove takes them");
    }

    #[test]
    fn remove_takes_back_byte_for_byte() {
        let fake = Fake::default();
        fake.put(
            Kind::PathEntry,
            ENVIRONMENT,
            "Path",
            Value::Reg { ty: RegType::ExpandSz, data: r"%X%;;C:\y\;".into() },
        );
        let start = fake.0.borrow().clone();
        let file = journal("remove");
        add(&fake, &file, r"C:\T\gezik.exe").unwrap();
        let undone = undo_matching(&mut file.lock().unwrap(), &fake, |c| c.feature == FEATURE_PATH);
        assert!(undone.path_changed);
        assert_eq!(*fake.0.borrow(), start);
        assert!(!file.exists(), "the journal goes with its last entry");
        assert_eq!(undone.lines.first().map(String::as_str), Some(r"undone: HKCU\Environment\Path"), "newest first");
    }

    #[test]
    fn unregister_reports_and_exits_1() {
        let fake = Fake::default();
        let file = journal("unregister");
        add(&fake, &file, r"C:\T\gezik.exe").unwrap();
        fake.put(Kind::RegistryValue, APP_PATH, "", reg(r"D:\Other\gezik.exe"));
        let report = unregister(Some(&file), &fake, &win_places(), Path::new(r"C:\T\gezik.exe"), true);
        assert_eq!(report.code, 1);
        assert!(
            report.lines.iter().any(|l| l.starts_with("left as is") && l.contains("gezik.exe")),
            "{:?}",
            report.lines
        );
        assert_eq!(fake.get(Kind::RegistryValue, APP_PATH, ""), Some(reg(r"D:\Other\gezik.exe")));
        assert!(fake.get(Kind::File, CMD, "").is_none() && fake.get(Kind::PathEntry, ENVIRONMENT, "Path").is_none());
        assert!(!file.exists(), "a value left as is is not Gezik's any more");
    }

    #[test]
    fn a_tampered_entry_is_not_acted_on() {
        let file = journal("tampered");
        let access = SystemAccess { places: win_places(), registry_root: String::new() };
        let bad = Change {
            feature: FEATURE_PATH.into(),
            kind: Kind::File,
            place: r"C:\Windows\win.ini".into(),
            name: String::new(),
            entry: String::new(),
            before: Value::Absent,
            after: Value::Other,
            done: true,
        };
        file.lock().unwrap().record(bad, "x", |_| Ok(())).unwrap();
        let report = unregister(Some(&file), &access, &win_places(), Path::new("x"), true);
        assert_eq!(report.code, 2);
        assert!(report.lines[0].contains("not a place Gezik writes"), "{:?}", report.lines);
        assert!(file.exists(), "kept: a failure stays");
    }

    #[test]
    fn only_values_gezik_writes_are_written() {
        let all = path_targets(&win_places(), r"C:\T\gezik.exe", true).unwrap();
        let (file, value, path) = (&all[2], &all[5], &all[6]);
        let ok = |c: &Change, now: Value, to: Value| value_allowed(c, &now, &to);
        assert!(ok(file, Value::Absent, Value::Text(SHIM.into())) && ok(file, Value::Text(SHIM.into()), Value::Absent));
        assert!(!ok(file, Value::Text(SHIM.into()), Value::Text("@echo planted".into())));
        assert!(
            ok(value, Value::Absent, reg(r"C:\T\gezik.exe")) && ok(value, reg(r#""D:\Old\Gezik.exe""#), Value::Absent)
        );
        assert!(!ok(value, Value::Absent, reg(r"C:\evil.exe")));
        let expand_sz = Value::Reg { ty: RegType::ExpandSz, data: r"C:\T\gezik.exe".into() };
        assert!(!ok(value, Value::Absent, expand_sz), "Sz only");
        let user = format!(r"C:\a;{BIN};C:\b");
        assert!(ok(path, reg(r"C:\a;"), reg(&format!(r"C:\a;{BIN}"))), "add");
        assert!(ok(path, reg(&format!(r"C:\a;{BIN}")), reg(r"C:\a;")), "back byte for byte");
        assert!(ok(path, reg(&user), reg(r"C:\a;C:\b")), "Gezik's entry out");
        assert!(ok(path, Value::Absent, reg(BIN)) && ok(path, reg(BIN), Value::Absent));
        assert!(!ok(path, reg(&user), Value::Absent), "the user's PATH is never deleted");
        assert!(!ok(path, reg(&user), reg(r"C:\evil")), "nor written as the journal says");
        assert!(
            !ok(path, reg(r"C:\a"), Value::Reg { ty: RegType::ExpandSz, data: format!(r"C:\a;{BIN}") }),
            "type kept"
        );
        let link = &path_targets(&Places { home: Some("/h".into()), local_app_data: None }, "", false).unwrap()[2];
        assert!(ok(link, Value::Absent, Value::Link("/opt/gezik/gezik".into())));
        assert!(
            !ok(link, Value::Absent, Value::Link("/tmp/evil".into()))
                && !ok(link, Value::Absent, Value::Link("gezik".into()))
        );
    }

    /// A journal whose values were edited: nothing but Gezik's own values is written.
    #[cfg(windows)]
    #[test]
    fn a_tampered_value_is_not_written() {
        use gezik_platform::system::windows as reg;
        let root = format!(r"Software\GezikTest-{}-tampered", std::process::id());
        let app = format!(r"{root}\Software\Microsoft\Windows\CurrentVersion\App Paths\gezik.exe");
        let env = format!(r"{root}\Environment");
        // Every level under the test key only (never HKCU\Software itself).
        let mut level = root.clone();
        let mut made = vec![root.clone()];
        for part in app[root.len() + 1..].split('\\') {
            level = format!(r"{level}\{part}");
            made.push(level.clone());
        }
        made.push(env.clone());
        for key in &made {
            reg::create_key(key).unwrap();
        }
        let user_path = r"C:\mine;C:\also";
        reg::write_value(&env, "Path", RegType::Sz, user_path).unwrap();
        reg::write_value(&app, "", RegType::Sz, r"C:\T\gezik.exe").unwrap();
        let temp = std::env::temp_dir().join(format!("gezik-system-values-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(temp.join(r"Gezik\bin")).unwrap();
        let cmd = temp.join(r"Gezik\bin\gezik.cmd");
        std::fs::write(&cmd, SHIM).unwrap();
        let places = Places { home: None, local_app_data: Some(temp.clone()) };
        let access = SystemAccess { places: places.clone(), registry_root: root.clone() };
        let file = JournalFile::new(temp.join("config"));
        let targets = path_targets(&places, r"C:\T\gezik.exe", true).unwrap();
        let tampered = [
            Change { before: Value::Absent, after: reg(user_path), done: true, ..targets[6].clone() },
            Change { before: reg(r"C:\evil"), after: reg(user_path), done: true, ..targets[6].clone() },
            Change {
                before: Value::Text("@echo planted".into()),
                after: Value::Text(SHIM.into()),
                done: true,
                ..targets[2].clone()
            },
            Change { before: reg(r"C:\evil.exe"), after: reg(r"C:\T\gezik.exe"), done: true, ..targets[5].clone() },
        ];
        for change in tampered {
            file.lock().unwrap().record(change, "x", |_| Ok(())).unwrap();
        }
        let report = unregister(Some(&file), &access, &places, Path::new("x"), true);
        assert_eq!(report.code, 2, "{:?}", report.lines);
        assert!(report.lines.iter().all(|l| l.contains("not a value Gezik writes")), "{:?}", report.lines);
        assert_eq!(reg::read_value(&env, "Path").unwrap(), Some((RegType::Sz, user_path.to_owned())));
        assert_eq!(reg::read_value(&app, "").unwrap(), Some((RegType::Sz, r"C:\T\gezik.exe".to_owned())));
        assert_eq!(std::fs::read_to_string(&cmd).unwrap(), SHIM);
        reg::delete_value(&env, "Path").unwrap();
        reg::delete_value(&app, "").unwrap();
        for key in made.iter().rev() {
            reg::delete_empty_key(key).unwrap();
        }
        std::fs::remove_dir_all(&temp).unwrap();
    }

    #[test]
    fn an_unreadable_journal_changes_nothing() {
        let file = journal("unreadable");
        std::fs::create_dir_all(file.path().parent().unwrap()).unwrap();
        std::fs::write(file.path(), "version = 9\n").unwrap();
        let fake = Fake::default();
        fake.put(Kind::File, CMD, "", Value::Text(SHIM.into()));
        let report = unregister(Some(&file), &fake, &win_places(), Path::new("x"), true);
        assert_eq!(report.code, 2);
        assert_eq!(fake.get(Kind::File, CMD, ""), Some(Value::Text(SHIM.into())), "no sweep either");
        assert_eq!(std::fs::read_to_string(file.path()).unwrap(), "version = 9\n");
        assert!(add(&fake, &file, r"C:\T\gezik.exe").unwrap_err().contains("cannot be read"));
    }

    #[test]
    fn sweep_takes_only_gezik_shapes() {
        let fake = Fake::default();
        fake.put(Kind::PathEntry, ENVIRONMENT, "Path", reg(&format!(r"C:\x;{BIN}")));
        fake.put(Kind::RegistryKey, APP_PATHS, "", Value::Present);
        fake.put(Kind::RegistryKey, APP_PATH, "", Value::Present);
        fake.put(Kind::RegistryValue, APP_PATH, "", reg(r#""D:\Old\Gezik.exe""#));
        fake.put(Kind::File, CMD, "", Value::Text(SHIM.into()));
        fake.put(Kind::Folder, BIN, "", Value::Present);
        let report = unregister(Some(&journal("sweep")), &fake, &win_places(), Path::new(r"C:\T\gezik.exe"), true);
        assert!(report.lines[0].starts_with("No system-changes.toml"), "{:?}", report.lines);
        assert_eq!(report.code, 0);
        assert_eq!(fake.get(Kind::PathEntry, ENVIRONMENT, "Path"), Some(reg(r"C:\x")));
        assert_eq!(fake.get(Kind::RegistryKey, APP_PATHS, ""), Some(Value::Present), "not Gezik's name: kept");
        for (kind, place) in
            [(Kind::RegistryKey, APP_PATH), (Kind::RegistryValue, APP_PATH), (Kind::File, CMD), (Kind::Folder, BIN)]
        {
            assert!(fake.get(kind, place, "").is_none(), "{place}");
        }
        // Not Gezik's shapes: left.
        let other = Fake::default();
        other.put(Kind::RegistryValue, APP_PATH, "", reg(r"C:\Other\tool.exe"));
        other.put(Kind::File, CMD, "", Value::Text("@echo mine".into()));
        assert!(sweep_changes(&other, &win_places(), Path::new("x"), true).is_empty());
    }

    #[test]
    fn path_state_follows_what_is_there() {
        let exe = r"C:\T\gezik.exe";
        let fake = Fake::default();
        let file = journal("state");
        let state = |fake: &Fake| snapshot(Some(&file), fake, &win_places(), exe, true).path;
        assert_eq!(state(&fake), PathState::Off);
        add(&fake, &file, exe).unwrap();
        assert_eq!(state(&fake), PathState::On { command: CMD.into() });
        fake.put(Kind::PathEntry, ENVIRONMENT, "Path", reg(&format!(r"{BIN};C:\later")));
        assert_eq!(state(&fake), PathState::On { command: CMD.into() }, "other PATH parts may change");
        let moved = snapshot(Some(&file), &fake, &win_places(), r"D:\New\gezik.exe", true).path;
        assert_eq!(moved, PathState::Moved { old: exe.into() });
        let same_other_case = snapshot(Some(&file), &fake, &win_places(), &exe.to_uppercase(), true).path;
        assert_eq!(same_other_case, PathState::On { command: CMD.into() });
        fake.put(Kind::File, CMD, "", Value::Text("@echo edited".into()));
        assert_eq!(state(&fake), PathState::Changed);
        let taken = Fake::default();
        taken.put(Kind::File, CMD, "", Value::Text("@echo mine".into()));
        assert_eq!(state_of(&taken), PathState::Taken { place: CMD.into() });
    }

    fn state_of(fake: &Fake) -> PathState {
        snapshot(Some(&journal("taken")), fake, &win_places(), r"C:\T\gezik.exe", true).path
    }

    #[test]
    fn expand_opens_known_names_only() {
        // SAFETY: tests of this crate do not read GEZIK_EXPAND_TEST elsewhere.
        unsafe { std::env::set_var("GEZIK_EXPAND_TEST", r"C:\E") };
        assert_eq!(expand(r"%GEZIK_EXPAND_TEST%\bin;%NOPE_X%\y;50%"), r"C:\E\bin;%NOPE_X%\y;50%");
    }

    /// Spec 16.2: the real registry, under a test key, and real files in a temp folder;
    /// add, then --unregister, and everything is as it was, byte for byte.
    #[cfg(windows)]
    #[test]
    fn the_real_registry_comes_back_byte_for_byte() {
        use gezik_platform::system::windows as reg;
        let root = format!(r"Software\GezikTest-{}-path", std::process::id());
        let levels = [
            root.clone(),
            format!(r"{root}\Software"),
            format!(r"{root}\Software\Microsoft"),
            format!(r"{root}\Software\Microsoft\Windows"),
            format!(r"{root}\Software\Microsoft\Windows\CurrentVersion"),
            format!(r"{root}\Environment"),
        ];
        for level in &levels {
            reg::create_key(level).unwrap();
        }
        let env = format!(r"{root}\Environment");
        let user_path: String = (0..700).map(|i| format!(r"%USERPROFILE%\Tool {i};")).collect::<String>() + ";C:\\ç\\";
        reg::write_value(&env, "Path", RegType::Sz, &user_path).unwrap();
        let temp = std::env::temp_dir().join(format!("gezik-system-real-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(&temp).unwrap();
        let places = Places { home: None, local_app_data: Some(temp.clone()) };
        let access = SystemAccess { places: places.clone(), registry_root: root.clone() };
        let file = JournalFile::new(temp.join("config"));
        let exe = r"C:\Tools\Gezik ç\gezik.exe";
        let targets = path_targets(&places, exe, true).unwrap();
        assert_eq!(apply(&mut file.lock().unwrap(), &access, targets, exe), Ok(true));
        let (ty, added) = reg::read_value(&env, "Path").unwrap().unwrap();
        assert_eq!(ty, RegType::Sz, "the type stays");
        assert_eq!(added, format!(r"{user_path};{}\Gezik\bin", temp.display()));
        assert_eq!(std::fs::read_to_string(temp.join(r"Gezik\bin\gezik.cmd")).unwrap(), SHIM);
        let app_path = format!(r"{root}\Software\Microsoft\Windows\CurrentVersion\App Paths\gezik.exe");
        assert_eq!(reg::read_value(&app_path, "").unwrap(), Some((RegType::Sz, exe.to_owned())));
        let report = unregister(Some(&file), &access, &places, Path::new(exe), true);
        assert_eq!(report.code, 0, "{:?}", report.lines);
        assert_eq!(reg::read_value(&env, "Path").unwrap(), Some((RegType::Sz, user_path)));
        assert!(!reg::key_exists(&format!(r"{root}\Software\Microsoft\Windows\CurrentVersion\App Paths")).unwrap());
        assert!(!temp.join("Gezik").exists());
        assert!(!file.exists());
        reg::delete_value(&env, "Path").unwrap();
        for level in levels.iter().rev() {
            reg::delete_empty_key(level).unwrap();
        }
        std::fs::remove_dir_all(&temp).unwrap();
    }
}
