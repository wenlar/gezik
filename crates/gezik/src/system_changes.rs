//! The system changes Gezik makes on an explicit command and their undoing (spec 6, 8.2, 8.4,
//! 11): what PATH registration and the default file manager write, the write-ahead apply with
//! its pre-check and take-back, the undo, `--unregister`, the sweep by fixed names when the
//! journal is lost, `restore-explorer.reg`, and the rows' states for the panel.
//! No UI here (integration.rs is the panel). Runs off the UI thread: the registry, files
//! and a broadcast that may wait for hung windows. What the panel showed is never acted on:
//! every write and every undo reads what is there now, under the journal's lock.

use std::io;
use std::path::{Path, PathBuf};

use gezik_config::system_journal::{FILE, JournalFile, Locked};
use gezik_core::system_change::{self as sc, Access, Change, Kind, Outcome, RegType, Value};
use gezik_platform::system::Places;
use gezik_platform::system::text as t;

pub const FEATURE_PATH: &str = "path";
/// Gezik as the default file manager (9b4).
pub const FEATURE_DEFAULT: &str = "default-file-manager";
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
const CLASSES: &str = r"HKCU\Software\Classes";
/// The classes whose default verb Gezik changes (spec 6.1).
const SWITCHED: [&str; 3] = ["Directory", "Drive", "Folder"];
/// Win+E's CLSID (decision 4).
const WIN_E_CLSID: &str = "{52205fd8-5dfb-447d-801a-d0b52f2e83e1}";
pub const RESTORE_REG: &str = "restore-explorer.reg";
/// The bundle's Info.plist: the one built into the exe (decision 15).
pub const INFO_PLIST: &str = include_str!("../macos/Info.plist");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Windows,
    Mac,
    Linux,
}

impl Os {
    pub const HERE: Os = if cfg!(windows) {
        Os::Windows
    } else if cfg!(target_os = "macos") {
        Os::Mac
    } else {
        Os::Linux
    };
}

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

fn step(kind: Kind, place: impl Into<String>, name: &str, after: Value) -> Change {
    Change { feature: FEATURE_DEFAULT.into(), name: name.into(), ..target(kind, place, "", "", after) }
}

fn sz(data: &str) -> Value {
    Value::Reg { ty: RegType::Sz, data: data.into() }
}

/// A file's text, if there (`default_targets`' `read`).
pub type ReadText = dyn Fn(&Path) -> Option<String>;

/// What making Gezik the default writes, in order (spec 6; decisions 10, 15, 17). `read`
/// gives a file's text for the steps that depend on what is there (Linux: make mimeapps.list
/// only if missing, edit a desktop's own list only if it holds the key); `None` lists every
/// place Gezik may write (the allow list and the sweep).
pub fn default_targets(places: &Places, exe: &str, os: Os, read: Option<&ReadText>) -> Option<Vec<Change>> {
    match os {
        Os::Windows => Some(windows_targets(exe)),
        Os::Mac => mac_targets(places, exe),
        Os::Linux => linux_targets(places, exe, read),
    }
}

fn windows_targets(exe: &str) -> Vec<Change> {
    let mut out = vec![step(Kind::RegistryKey, CLASSES, "", Value::Present)];
    for class in SWITCHED {
        let shell = format!(r"{CLASSES}\{class}\shell");
        let verb = format!(r"{shell}\{}", t::VERB);
        let command = format!(r"{verb}\command");
        out.extend([
            step(Kind::RegistryKey, format!(r"{CLASSES}\{class}"), "", Value::Present),
            step(Kind::RegistryKey, shell, "", Value::Present),
            step(Kind::RegistryKey, verb.as_str(), "", Value::Present),
            step(Kind::RegistryValue, verb, "", sz(t::VERB_TITLE)),
            step(Kind::RegistryKey, command.as_str(), "", Value::Present),
            step(Kind::RegistryValue, command, "", sz(&t::shell_command(exe, t::VERB_ARG))),
        ]);
    }
    let mut key = format!(r"{CLASSES}\CLSID");
    out.push(step(Kind::RegistryKey, key.as_str(), "", Value::Present));
    for part in [WIN_E_CLSID, "shell", "opennewwindow", "command"] {
        key = format!(r"{key}\{part}");
        out.push(step(Kind::RegistryKey, key.as_str(), "", Value::Present));
    }
    out.push(step(Kind::RegistryValue, key.as_str(), "", sz(&t::shell_command(exe, ""))));
    out.push(step(Kind::RegistryValue, key, "DelegateExecute", sz("")));
    // The switch last (decision 10): stopped before it, folders open as they did.
    for class in SWITCHED {
        out.push(step(Kind::RegistryValue, format!(r"{CLASSES}\{class}\shell"), "", sz(t::VERB)));
    }
    out
}

/// The bundle an exe runs from: `…/X.app` for `…/X.app/Contents/MacOS/gezik`.
fn bundle_of(exe: &str) -> Option<&str> {
    let app = exe.strip_suffix("/Contents/MacOS/gezik")?;
    app.ends_with(".app").then_some(app)
}

fn mac_targets(places: &Places, exe: &str) -> Option<Vec<Change>> {
    let mut out = Vec::new();
    if bundle_of(exe).is_none() {
        let apps = format!("{}/Applications", places.home.as_ref()?.to_str()?);
        let app = format!("{apps}/Gezik.app");
        out.extend([
            step(Kind::Folder, apps.as_str(), "", Value::Present),
            step(Kind::Folder, app.as_str(), "", Value::Present),
            step(Kind::Folder, format!("{app}/Contents"), "", Value::Present),
            step(Kind::Folder, format!("{app}/Contents/MacOS"), "", Value::Present),
            step(Kind::File, format!("{app}/Contents/Info.plist"), "", Value::Text(INFO_PLIST.into())),
            step(Kind::Symlink, format!("{app}/Contents/MacOS/gezik"), "", Value::Link(exe.into())),
        ]);
    }
    out.push(step(Kind::MacDefault, "public.folder", "", Value::Text(t::BUNDLE_ID.into())));
    out.push(step(Kind::MacPref, "NSFileViewer", "", Value::Text(t::BUNDLE_ID.into())));
    Some(out)
}

fn linux_targets(places: &Places, exe: &str, read: Option<&ReadText>) -> Option<Vec<Change>> {
    let data = places.data_home.as_ref()?.to_str()?;
    let config = places.config_home.as_ref()?.to_str()?;
    let list = format!("{config}/mimeapps.list");
    let ours = || Value::Text("gezik.desktop;".into());
    let mut out = vec![
        step(Kind::Folder, data, "", Value::Present),
        step(Kind::Folder, format!("{data}/applications"), "", Value::Present),
        step(Kind::File, format!("{data}/applications/gezik.desktop"), "", Value::Text(t::desktop_entry(exe))),
        step(Kind::Folder, format!("{data}/dbus-1"), "", Value::Present),
        step(Kind::Folder, format!("{data}/dbus-1/services"), "", Value::Present),
        step(
            Kind::File,
            format!("{data}/dbus-1/services/org.freedesktop.FileManager1.service"),
            "",
            Value::Text(t::dbus_service(exe)),
        ),
        step(Kind::Folder, config, "", Value::Present),
    ];
    if read.is_none_or(|read| read(Path::new(&list)).is_none()) {
        out.push(step(Kind::File, list.as_str(), "", Value::Text(t::MIMEAPPS_EMPTY.into())));
    }
    out.push(step(Kind::Mimeapps, list, "inode/directory", ours()));
    for desktop in &places.desktops {
        let own = format!("{config}/{desktop}-mimeapps.list");
        let holds = read.is_none_or(|read| {
            read(Path::new(&own)).is_some_and(|text| t::mimeapps_get(&text, "inode/directory").is_some())
        });
        if holds {
            out.push(step(Kind::Mimeapps, own, "inode/directory", ours()));
        }
    }
    Some(out)
}

/// Whether `change` is at a place Gezik writes (decision 8): a journal edited by hand or by
/// something else cannot point Gezik at anything else. Feature, kind, place, name and entry
/// must all be one of [`path_targets`]'s or [`default_targets`]'s (Windows without case).
pub fn allowed(change: &Change, places: &Places, os: Os) -> bool {
    let windows = os == Os::Windows;
    let same = |a: &str, b: &str| if windows { a.to_lowercase() == b.to_lowercase() } else { a == b };
    let path = path_targets(places, "", windows).unwrap_or_default();
    let default = default_targets(places, "", os, None).unwrap_or_default();
    path.iter().chain(&default).any(|t| {
        t.feature == change.feature
            && t.kind == change.kind
            && same(&t.place, &change.place)
            && same(&t.name, &change.name)
            && same(&t.entry, &change.entry)
    })
}

/// The checks of every write (`SystemAccess::set`): an allowed place and a value Gezik writes;
/// `access` reads what is beside it.
fn check_write(
    change: &Change,
    now: &Value,
    value: &Value,
    places: &Places,
    os: Os,
    access: &dyn Access,
) -> io::Result<()> {
    if !allowed(change, places, os) {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "not a place Gezik writes; left as is"));
    }
    // The journal's values are not trusted either: only what Gezik itself would write.
    if !value_allowed(change, now, value) {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "not a value Gezik writes; left as is"));
    }
    // An empty DelegateExecute goes only beside Gezik's own Win+E command: a program that took
    // Win+E since (the Files app) needs the same value next to its command.
    if change.kind == Kind::RegistryValue
        && *value == Value::Absent
        && change.name.eq_ignore_ascii_case("DelegateExecute")
    {
        let command = Change { name: String::new(), ..change.clone() };
        let ours = matches!(access.current(&command), Ok(Value::Reg { ty: RegType::Sz, data })
            if registry_value_allowed(&change.place, "", &data));
        if !ours {
            // AlreadyExists: the undo reports it as left as is.
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, "Win+E's command beside it is not Gezik's"));
        }
    }
    Ok(())
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
        if allowed(change, &self.places, Os::HERE) {
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
            Kind::MacDefault | Kind::MacPref => mac_current(change),
            Kind::Mimeapps => mimeapps_current(Path::new(&change.place), &change.name),
        }
    }

    fn set(&self, change: &Change, value: &Value) -> io::Result<()> {
        check_write(change, &self.current(change)?, value, &self.places, Os::HERE, self)?;
        match change.kind {
            Kind::RegistryValue | Kind::PathEntry | Kind::RegistryKey => {
                registry_set(&self.key(&change.place)?, change.kind, &change.name, value)
            }
            Kind::File | Kind::Symlink | Kind::Folder => file_set(change, value),
            Kind::MacDefault | Kind::MacPref => mac_set(change, value),
            Kind::Mimeapps => mimeapps_set_file(Path::new(&change.place), &change.name, value),
        }
    }
}

/// Whether putting `value` where `now` is, is a write Gezik makes (decision 8): Gezik's own
/// files, links to a `gezik`, registry values in Gezik's shapes (a verb name may come back,
/// a command only if it is Gezik's), PATH with only Gezik's entry added or taken out (the
/// value deleted only when Gezik's entry is all there is), bundle and desktop ids.
fn value_allowed(change: &Change, now: &Value, value: &Value) -> bool {
    let entry = change.entry.as_str();
    match (change.kind, value) {
        (Kind::PathEntry, _) if entry.is_empty() => false,
        (Kind::PathEntry, Value::Absent) => matches!(now, Value::Reg { data, .. } if data == entry),
        (_, Value::Absent) | (Kind::Folder | Kind::RegistryKey, Value::Present) => true,
        (Kind::File, Value::Text(text)) => file_text_allowed(&change.place, text),
        // Links are macOS and Linux only: absolute is a leading `/`.
        (Kind::Symlink, Value::Link(to)) => to.starts_with('/') && to.rsplit('/').next() == Some("gezik"),
        (Kind::RegistryValue, Value::Reg { ty: RegType::Sz, data }) => {
            registry_value_allowed(&change.place, &change.name, data)
        }
        (Kind::MacDefault | Kind::MacPref, Value::Text(id)) => t::bundle_id_like(id),
        (Kind::Mimeapps, Value::Text(ids)) => t::desktop_ids(ids),
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

/// A file's text by its name: `allowed` has pinned the place already.
fn file_text_allowed(place: &str, text: &str) -> bool {
    let unix_gezik = |exe: String| exe.starts_with('/') && exe.rsplit('/').next() == Some("gezik");
    match place.rsplit(['/', '\\']).next().unwrap_or("") {
        "gezik.cmd" => text == SHIM,
        "Info.plist" => text == INFO_PLIST,
        "gezik.desktop" => t::desktop_exe(text).is_some_and(unix_gezik),
        "org.freedesktop.FileManager1.service" => t::service_exe(text).is_some_and(unix_gezik),
        "mimeapps.list" => text == t::MIMEAPPS_EMPTY,
        _ => false,
    }
}

/// A REG_SZ value by its place (decision 9: a verb name may come back, a command never
/// unless it is Gezik's); the caller has checked the type.
fn registry_value_allowed(place: &str, name: &str, data: &str) -> bool {
    let place = place.to_lowercase();
    if place == APP_PATH.to_lowercase() {
        return name.is_empty() && names_gezik_exe(data);
    }
    let command = |arg: &str| t::command_exe(data, arg).is_some_and(names_gezik_exe);
    if place.ends_with(r"\opennewwindow\command") {
        return if name.is_empty() {
            command("")
        } else {
            name.eq_ignore_ascii_case("DelegateExecute") && data.is_empty()
        };
    }
    if !name.is_empty() {
        return false;
    }
    if place.ends_with(r"\shell\gezik\command") {
        command(t::VERB_ARG)
    } else if place.ends_with(r"\shell\gezik") {
        data == t::VERB_TITLE
    } else {
        place.ends_with(r"\shell") && t::verb_like(data)
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

#[cfg(target_os = "macos")]
fn mac_current(change: &Change) -> io::Result<Value> {
    use gezik_platform::system::macos;
    let now = if change.kind == Kind::MacDefault {
        macos::default_handler(&change.place)
    } else {
        macos::global_pref(&change.place)
    };
    Ok(now.map_or(Value::Absent, Value::Text))
}

#[cfg(target_os = "macos")]
fn mac_set(change: &Change, value: &Value) -> io::Result<()> {
    use gezik_platform::system::macos;
    let id = match value {
        Value::Text(id) => Some(id.as_str()),
        Value::Absent => None,
        _ => return Err(io::Error::new(io::ErrorKind::InvalidInput, "Gezik does not write this")),
    };
    if change.kind == Kind::MacDefault {
        // No way to clear a handler: Finder is what macOS has without one (decision 15).
        macos::set_default_handler(&change.place, id.unwrap_or("com.apple.finder"))
    } else {
        macos::set_global_pref(&change.place, id)
    }
}

#[cfg(not(target_os = "macos"))]
fn mac_current(_: &Change) -> io::Result<Value> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "macOS only"))
}

#[cfg(not(target_os = "macos"))]
fn mac_set(_: &Change, _: &Value) -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "macOS only"))
}

/// A key of a mimeapps.list; a link or a large file is not Gezik's to edit (decision 17).
fn mimeapps_current(path: &Path, key: &str) -> io::Result<Value> {
    Ok(match file_current(path, Kind::File)? {
        Value::Absent => Value::Absent,
        Value::Text(text) => t::mimeapps_get(&text, key).map_or(Value::Absent, Value::Text),
        _ => Value::Other,
    })
}

fn mimeapps_set_file(path: &Path, key: &str, value: &Value) -> io::Result<()> {
    let text = match file_current(path, Kind::File)? {
        Value::Text(text) => text,
        Value::Absent if *value == Value::Absent => return Ok(()),
        _ => return Err(io::Error::new(io::ErrorKind::AlreadyExists, "not a plain file Gezik can edit")),
    };
    let value = match value {
        Value::Text(v) => Some(v.as_str()),
        _ => None,
    };
    gezik_config::paths::write_atomic(path, &t::mimeapps_set(&text, key, value))
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

/// A file's text for [`default_targets`]' `read`: something there that is not a plain text
/// file counts as there, so Gezik makes nothing over it.
fn disk_text(path: &Path) -> Option<String> {
    match file_current(path, Kind::File) {
        Ok(Value::Text(text)) => Some(text),
        Ok(Value::Absent) => None,
        _ => Some(String::new()),
    }
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

/// One target read against what is there: `Ok(None)` already in place, `Ok(Some)` the change
/// to make (its `before` and `after` filled in), `Err` something not Gezik's is there or PATH
/// would grow too long. Reads only.
fn plan_step(journal: &[Change], access: &dyn Access, target: Change) -> Result<Option<Change>, String> {
    let now = access.current(&target).map_err(|err| format!("{}: {err}", sc::what(&target)))?;
    let after = match target.kind {
        Kind::Folder | Kind::RegistryKey if now != Value::Absent => return Ok(None),
        Kind::PathEntry => {
            if target.entry.is_empty() {
                return Err("no folder to add to PATH".into());
            }
            let (ty, list) = match &now {
                Value::Reg { ty, data } => (*ty, data.as_str()),
                _ => (RegType::ExpandSz, ""),
            };
            let Some(list) = sc::with_entry(list, &target.entry, expand) else { return Ok(None) };
            if list.encode_utf16().count() >= MAX_PATH_VALUE {
                return Err("PATH is too long to add Gezik's folder to".into());
            }
            Value::Reg { ty, data: list }
        }
        _ => target.after.clone(),
    };
    if now == after {
        return Ok(None);
    }
    let foreign = match target.kind {
        Kind::File | Kind::Symlink => !made_by_gezik(journal, &target, &now),
        // Only Gezik's own shapes are ever written back here by an undo (decision 9).
        Kind::RegistryValue | Kind::MacDefault | Kind::MacPref | Kind::Mimeapps => !value_allowed(&target, &now, &now),
        _ => false,
    };
    if now != Value::Absent && foreign {
        let whose = if target.place.to_lowercase().ends_with(r"\opennewwindow\command") {
            "another file manager answers Win+E: "
        } else {
            ""
        };
        return Err(format!("{whose}{} is there and was not made by Gezik; it is left alone", sc::what(&target)));
    }
    Ok(Some(Change { before: now, after, ..target }))
}

/// Puts `targets` in place (decision 8): every target is read first and a foreign one refuses
/// the whole command with nothing written; then each is written to the journal before it is
/// made (decision 6); if one fails, what this run made is undone again. `Ok(true)`: the
/// user's PATH changed (the caller tells the system).
pub fn apply_all(locked: &mut Locked, access: &dyn Access, targets: Vec<Change>, exe: &str) -> Result<bool, String> {
    for target in &targets {
        plan_step(&locked.journal().changes, access, target.clone())?;
    }
    let start = locked.journal().changes.len();
    let mut path_changed = false;
    for target in targets {
        let change = match plan_step(&locked.journal().changes, access, target) {
            Ok(Some(change)) => change,
            Ok(None) => continue,
            Err(why) => return Err(take_back(locked, access, start, why)),
        };
        let (what, path) = (sc::what(&change), change.kind == Kind::PathEntry);
        match locked.record(change, exe, |c| access.set(c, &c.after)) {
            Ok(()) => path_changed |= path,
            // Something not Gezik's took the place meanwhile: nothing was made there.
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                let why = format!("{what} is there and was not made by Gezik; it is left alone");
                return Err(take_back(locked, access, start, why));
            }
            Err(err) => return Err(take_back(locked, access, start, format!("{what}: {err}"))),
        }
    }
    Ok(path_changed)
}

/// Undoes the journal's entries from `start` on (this run's) and forgets the settled ones.
fn take_back(locked: &mut Locked, access: &dyn Access, start: usize, why: String) -> String {
    let ours: Vec<Change> = locked.journal().changes[start..].to_vec();
    let outcomes = sc::undo(&ours, access);
    let saved = locked.retain(|i, _| i < start || !outcomes[i - start].settled());
    if saved.is_ok() && outcomes.iter().all(Outcome::settled) {
        format!("{why}; what this run made was taken back")
    } else {
        format!("{why}; some of what this run made could not be taken back: see Undo all")
    }
}

/// Whether `now` is what an earlier finished change of the journal put at that place.
fn made_by_gezik(journal: &[Change], target: &Change, now: &Value) -> bool {
    journal.iter().any(|c| c.done && c.kind == target.kind && c.place == target.place && c.after == *now)
}

/// The first change in `targets` [`apply_all`] would refuse, as its reason; reads only.
fn would_refuse(journal: &[Change], access: &dyn Access, targets: Vec<Change>) -> Option<String> {
    targets.into_iter().find_map(|target| plan_step(journal, access, target).err())
}

/// What an undo did: its report lines (newest first), outcomes, and whether PATH or the
/// default file manager changed.
#[derive(Debug, Default)]
pub struct Undone {
    pub lines: Vec<String>,
    pub outcomes: Vec<Outcome>,
    pub path_changed: bool,
    pub default_changed: bool,
}

fn report(changes: &[Change], outcomes: Vec<Outcome>) -> Undone {
    let lines = changes.iter().zip(&outcomes).rev().map(|(c, o)| sc::report_line(c, o)).collect();
    let made = |o: &Outcome| matches!(o, Outcome::Undone | Outcome::EntryRemoved);
    let path_changed = changes.iter().zip(&outcomes).any(|(c, o)| c.kind == Kind::PathEntry && made(o));
    let default_changed = changes.iter().zip(&outcomes).any(|(c, o)| c.feature == FEATURE_DEFAULT && made(o));
    Undone { lines, outcomes, path_changed, default_changed }
}

/// Undoes the journal's changes `which` picks, newest first (spec 11.4), and forgets the
/// settled ones (decision 12); a failed one stays for another try.
pub fn undo_matching(locked: &mut Locked, access: &dyn Access, which: impl Fn(&Change) -> bool) -> Undone {
    let picked: Vec<usize> =
        locked.journal().changes.iter().enumerate().filter(|(_, c)| which(c)).map(|(i, _)| i).collect();
    let changes: Vec<Change> = picked.iter().map(|&i| locked.journal().changes[i].clone()).collect();
    let mut undone = report(&changes, sc::undo(&changes, access));
    // A folder or key still holding another entry stays, to go when that one is undone (decision 11).
    let holds_other = |i: usize| {
        let place = locked.journal().changes[i].place.to_lowercase();
        locked.journal().changes.iter().enumerate().any(|(j, c)| {
            !picked.contains(&j)
                && c.place.to_lowercase().strip_prefix(&place).is_some_and(|rest| rest.starts_with(['\\', '/']))
        })
    };
    let forget: Vec<usize> = picked
        .iter()
        .zip(&undone.outcomes)
        .filter(|&(&i, o)| o.settled() && !(*o == Outcome::NotEmpty && holds_other(i)))
        .map(|(&i, _)| i)
        .collect();
    if let Err(err) = locked.retain(|i, _| !forget.contains(&i)) {
        undone.lines.push(format!("{FILE} could not be written: {err}"));
        undone.outcomes.push(Outcome::Failed(err.to_string()));
    }
    undone
}

/// No journal (decision 13): only Gezik's names in Gezik's shapes, as changes to undo.
pub fn sweep_changes(access: &dyn Access, places: &Places, exe: &Path, os: Os) -> Vec<Change> {
    let windows = os == Os::Windows;
    let mut out = Vec::new();
    for target in path_targets(places, "", windows).unwrap_or_default() {
        // Not named Gezik: Gezik cannot know it made them.
        if target.place == APP_PATHS || (!windows && target.kind == Kind::Folder) {
            continue;
        }
        let Ok(now) = access.current(&target) else { continue };
        let before = match (target.kind, &now) {
            (Kind::Folder | Kind::RegistryKey, Value::Present) => Value::Absent,
            (Kind::File, Value::Text(text)) if text == SHIM => Value::Absent,
            (Kind::RegistryValue, Value::Reg { ty: RegType::Sz, data }) if names_gezik_exe(data) => Value::Absent,
            (Kind::PathEntry, Value::Reg { ty, data }) => match sc::without_entry(data, &target.entry) {
                Some(rest) => Value::Reg { ty: *ty, data: rest },
                None => continue,
            },
            (Kind::Symlink, Value::Link(to)) if gezik_link(Path::new(to), exe) => Value::Absent,
            _ => continue,
        };
        out.push(Change { before, after: now, done: true, ..target });
    }
    // Win+E's keys and empty DelegateExecute only beside Gezik's own Win+E command: other file
    // managers (the Files app) make the same keys and value next to theirs.
    let command = step(
        Kind::RegistryValue,
        format!(r"{CLASSES}\CLSID\{WIN_E_CLSID}\shell\opennewwindow\command"),
        "",
        Value::Absent,
    );
    let win_e_ours = os == Os::Windows
        && matches!(access.current(&command), Ok(Value::Reg { ty: RegType::Sz, data })
            if registry_value_allowed(&command.place, "", &data));
    let clsid = WIN_E_CLSID.to_lowercase();
    for target in default_targets(places, "", os, None).unwrap_or_default() {
        if target.place.to_lowercase().contains(&clsid) && !win_e_ours {
            continue;
        }
        let Ok(now) = access.current(&target) else { continue };
        let Some(before) = swept_before(&target, &now, exe) else { continue };
        out.push(Change { before, after: now, done: true, ..target });
    }
    out
}

/// What a default file manager place goes back to in the sweep: only Gezik's shapes.
fn swept_before(target: &Change, now: &Value, exe: &Path) -> Option<Value> {
    let place = target.place.to_lowercase();
    match (target.kind, now) {
        // Only keys named Gezik's, or under Win+E's CLSID (removed only if empty).
        (Kind::RegistryKey, Value::Present)
            if place.ends_with(r"\gezik")
                || place.ends_with(r"\gezik\command")
                || place.contains(&WIN_E_CLSID.to_lowercase()) =>
        {
            Some(Value::Absent)
        }
        // The default verb only if it is Gezik's; Gezik's command, title and DelegateExecute.
        (Kind::RegistryValue, Value::Reg { data, .. }) if place.ends_with(r"\shell") => {
            (data == t::VERB).then_some(Value::Absent)
        }
        (Kind::RegistryValue, Value::Reg { ty: RegType::Sz, data }) => {
            registry_value_allowed(&target.place, &target.name, data).then_some(Value::Absent)
        }
        (Kind::File, Value::Text(text)) => {
            (file_text_allowed(&target.place, text) && !place.ends_with("mimeapps.list")).then_some(Value::Absent)
        }
        (Kind::Symlink, Value::Link(to)) => gezik_link(Path::new(to), exe).then_some(Value::Absent),
        (Kind::Folder, Value::Present)
            if place.ends_with("gezik.app")
                || place.ends_with("gezik.app/contents")
                || place.ends_with("gezik.app/contents/macos") =>
        {
            Some(Value::Absent)
        }
        (Kind::MacDefault, Value::Text(id)) if id == t::BUNDLE_ID => Some(Value::Text("com.apple.finder".into())),
        (Kind::MacPref, Value::Text(id)) if id == t::BUNDLE_ID => Some(Value::Absent),
        (Kind::Mimeapps, Value::Text(ids)) if ids == "gezik.desktop;" => Some(Value::Absent),
        _ => None,
    }
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

/// Rewrites `restore-explorer.reg` from the journal (decision 12); none left: the file goes.
pub fn write_restore_reg(locked: &Locked, os: Os) -> io::Result<()> {
    write_reg_for(&locked.journal().changes, locked.dir(), os)
}

fn write_reg_for(changes: &[Change], dir: &Path, os: Os) -> io::Result<()> {
    if os != Os::Windows {
        return Ok(());
    }
    let path = dir.join(RESTORE_REG);
    // Only Gezik's own places (restore_reg itself only bounds them to HKCU\Software\Classes);
    // Windows' default targets do not depend on the user's folders.
    let ours: Vec<Change> = changes
        .iter()
        .filter(|c| c.feature == FEATURE_DEFAULT && allowed(c, &Places::default(), Os::Windows))
        .cloned()
        .collect();
    if ours.is_empty() {
        return match std::fs::remove_file(&path) {
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other,
        };
    }
    gezik_config::paths::write_atomic_bytes(&path, &t::utf16_file(&t::restore_reg(&ours)))
}

/// `--unregister`'s result (spec 11.4).
#[derive(Debug)]
pub struct Report {
    pub lines: Vec<String>,
    pub code: i32,
    pub path_changed: bool,
    pub default_changed: bool,
}

/// Undoes everything in the journal; with no journal, the sweep. An unreadable journal
/// changes nothing (code 2).
pub fn unregister(file: Option<&JournalFile>, access: &dyn Access, places: &Places, exe: &Path, os: Os) -> Report {
    // The lock also for the sweep: no other Gezik adds to PATH meanwhile.
    let undone = match file.map(JournalFile::lock) {
        Some(Err(err)) => {
            return Report {
                lines: vec![format!("{err}; nothing was changed")],
                code: 2,
                path_changed: false,
                default_changed: false,
            };
        }
        Some(Ok(mut locked)) if !locked.journal().changes.is_empty() => {
            let undone = undo_matching(&mut locked, access, |_| true);
            let _ = write_restore_reg(&locked, os);
            undone
        }
        Some(Ok(_locked)) => sweep(access, places, exe, os),
        None => sweep(access, places, exe, os),
    };
    Report {
        code: sc::exit_code(&undone.outcomes),
        lines: undone.lines,
        path_changed: undone.path_changed,
        default_changed: undone.default_changed,
    }
}

fn sweep(access: &dyn Access, places: &Places, exe: &Path, os: Os) -> Undone {
    let changes = sweep_changes(access, places, exe, os);
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

/// The default file manager row (spec 3.2).
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code, reason = "the panel row (Task 6 of 9b4) reads the fields")]
pub enum DefaultState {
    Off,
    On,
    /// Made by Gezik for an exe that is not this one.
    Moved {
        old: String,
    },
    /// Something else changed what Gezik wrote, or it stopped before the switch.
    Changed,
    /// Gezik made nothing and Make default would refuse (another program's Win+E …): why.
    Taken {
        why: String,
    },
}

/// The exe a default change points at: the verb command, the bundle link, the .desktop.
fn exe_of(change: &Change) -> Option<String> {
    match (&change.kind, &change.after) {
        (Kind::RegistryValue, Value::Reg { ty: RegType::Sz, data })
            if change.place.to_lowercase().ends_with(r"\gezik\command") =>
        {
            t::command_exe(data, t::VERB_ARG).map(str::to_owned)
        }
        (Kind::Symlink, Value::Link(to)) => Some(to.clone()),
        (Kind::File, Value::Text(text)) if change.place.ends_with("gezik.desktop") => t::desktop_exe(text),
        _ => None,
    }
}

/// The row from the journal's finished default changes paired with what is there now.
pub fn default_state(made: &[(Change, Value)], exe: &str, taken: Option<String>) -> DefaultState {
    let made: Vec<&(Change, Value)> = made.iter().filter(|(c, _)| c.done && c.feature == FEATURE_DEFAULT).collect();
    if made.is_empty() {
        return taken.map_or(DefaultState::Off, |why| DefaultState::Taken { why });
    }
    let switch = |c: &Change| match c.kind {
        Kind::MacDefault | Kind::Mimeapps => true,
        Kind::RegistryValue => c.place.to_lowercase().ends_with(r"\shell"),
        _ => false,
    };
    let still = |(c, now): &&(Change, Value)| match c.kind {
        Kind::Folder | Kind::RegistryKey => *now != Value::Absent,
        _ => *now == c.after,
    };
    if !made.iter().any(|(c, _)| switch(c)) || !made.iter().all(still) {
        return DefaultState::Changed;
    }
    match made.iter().find_map(|(c, _)| exe_of(c)) {
        // macOS from inside a bundle: no exe in the changes; a moved bundle moves with it.
        Some(old) if !old.eq_ignore_ascii_case(exe) => DefaultState::Moved { old },
        _ => DefaultState::On,
    }
}

/// What the panel shows; only shown, never acted on.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub path: PathState,
    #[allow(dead_code, reason = "the panel row (Task 6 of 9b4) reads it")]
    pub default: DefaultState,
    pub changes: Result<Vec<Change>, String>,
    pub journal: PathBuf,
}

pub fn snapshot(file: Option<&JournalFile>, access: &dyn Access, places: &Places, exe: &str, os: Os) -> Snapshot {
    let windows = os == Os::Windows;
    let changes = match file {
        Some(file) if file.exists() => file.lock().map(|l| l.journal().changes.clone()).map_err(|e| e.to_string()),
        _ => Ok(Vec::new()),
    };
    let list = changes.as_deref().unwrap_or_default();
    let made = |feature: &str| -> Vec<(Change, Value)> {
        list.iter()
            .filter(|c| c.feature == feature)
            .map(|c| (c.clone(), access.current(c).unwrap_or(Value::Other)))
            .collect()
    };
    let taken = path_targets(places, exe, windows).and_then(|targets| {
        targets
            .into_iter()
            .filter(|t| matches!(t.kind, Kind::File | Kind::Symlink))
            .find(|t| access.current(t).is_ok_and(|now| now != Value::Absent))
            .map(|t| t.place)
    });
    let made_default = made(FEATURE_DEFAULT);
    let refused = if made_default.is_empty() {
        default_targets(places, exe, os, Some(&disk_text)).and_then(|targets| would_refuse(list, access, targets))
    } else {
        None
    };
    Snapshot {
        path: path_state(&made(FEATURE_PATH), exe, taken, windows),
        default: default_state(&made_default, exe, refused),
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
    snapshot(journal_file().as_ref(), &access, &access.places, &exe_text().unwrap_or_default(), Os::HERE)
}

/// Add gezik to PATH (spec 8.2).
pub fn add_now() -> Result<(), String> {
    let file = journal_file().ok_or("there is no config folder to keep system-changes.toml in")?;
    let access = SystemAccess::new();
    let exe = exe_text()?;
    let targets = path_targets(&access.places, &exe, cfg!(windows)).ok_or("the user's folder is not known")?;
    let mut locked = file.lock().map_err(|e| e.to_string())?;
    let changed = apply_all(&mut locked, &access, targets, &exe)?;
    drop(locked);
    if changed {
        gezik_platform::system::environment_changed();
    }
    Ok(())
}

/// Remove gezik from PATH: Gezik's PATH changes undone.
pub fn remove_now() -> Result<Vec<String>, String> {
    undo_feature_now(FEATURE_PATH)
}

/// Undoes one feature's changes (the Restore button, the loop question).
pub fn undo_feature_now(feature: &str) -> Result<Vec<String>, String> {
    let file = journal_file().ok_or("there is no config folder")?;
    let access = SystemAccess::new();
    let mut locked = file.lock().map_err(|e| e.to_string())?;
    let undone = undo_matching(&mut locked, &access, |c| c.feature == feature);
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

/// Make default (spec 6): every target read first, the recovery file written from what is
/// about to be made, then the targets, then the system told.
#[allow(dead_code, reason = "the panel and the palette (Task 6 of 9b4) call it")]
pub fn make_default_now() -> Result<(), String> {
    let file = journal_file().ok_or("there is no config folder to keep system-changes.toml in")?;
    let access = SystemAccess::new();
    let exe = exe_text()?;
    let targets =
        default_targets(&access.places, &exe, Os::HERE, Some(&disk_text)).ok_or("the user's folders are not known")?;
    let mut locked = file.lock().map_err(|e| e.to_string())?;
    // A refusal here writes nothing, not even the recovery file (decision 8).
    let mut all = locked.journal().changes.clone();
    for target in &targets {
        all.extend(plan_step(&locked.journal().changes, &access, target.clone())?);
    }
    write_reg_for(&all, locked.dir(), Os::HERE).map_err(|e| format!("{RESTORE_REG}: {e}"))?;
    let result = apply_all(&mut locked, &access, targets, &exe);
    let _ = write_restore_reg(&locked, Os::HERE);
    drop(locked);
    after_default_change(&exe);
    result.map(drop)
}

#[allow(dead_code, reason = "make_default_now's; the panel (Task 6 of 9b4) calls it")]
fn after_default_change(exe: &str) {
    gezik_platform::system::associations_changed();
    if Os::HERE == Os::Mac {
        let app = bundle_of(exe)
            .map(PathBuf::from)
            .or_else(|| gezik_platform::system::places().home.map(|h| h.join("Applications/Gezik.app")));
        if let Some(app) = app.filter(|a| a.exists()) {
            gezik_platform::system::register_app(&app);
        }
    }
}

/// Restore the system file manager: Gezik's default changes undone, the recovery file follows.
pub fn restore_default_now() -> Result<Vec<String>, String> {
    let lines = undo_feature_now(FEATURE_DEFAULT)?;
    if let Some(file) = journal_file()
        && let Ok(locked) = file.lock()
    {
        let _ = write_restore_reg(&locked, Os::HERE);
    }
    gezik_platform::system::associations_changed();
    Ok(lines)
}

/// Repair and Update: Gezik's default changes undone, then made again for this exe.
#[allow(dead_code, reason = "the panel and the moved check (Task 6 of 9b4) call it")]
pub fn repair_default_now() -> Result<Vec<String>, String> {
    let mut lines = restore_default_now()?;
    make_default_now()?;
    lines.push("made again for this Gezik".into());
    Ok(lines)
}

pub fn undo_all_now() -> Report {
    let access = SystemAccess::new();
    let exe = gezik_platform::system::exe().unwrap_or_default();
    let report = unregister(journal_file().as_ref(), &access, &access.places, &exe, Os::HERE);
    if report.path_changed {
        gezik_platform::system::environment_changed();
    }
    if report.default_changed {
        gezik_platform::system::associations_changed();
    }
    report
}

/// Where `exe` is, if it is a place it may leave (decision 14).
pub fn risky_place(exe: &Path, downloads: Option<&Path>, temp: &Path, removable: bool) -> Option<&'static str> {
    let exe = exe.to_string_lossy().to_lowercase();
    let under = |dir: &Path| {
        let dir = dir.to_string_lossy().to_lowercase();
        exe.strip_prefix(dir.trim_end_matches(['\\', '/'])).is_some_and(|rest| rest.starts_with(['\\', '/']))
    };
    if downloads.is_some_and(under) {
        Some("Downloads")
    } else if under(temp) {
        Some("a temporary folder")
    } else if removable {
        Some("a removable drive")
    } else {
        None
    }
}

/// The first paragraph of Make default's question, if Gezik's exe is somewhere it may leave.
#[allow(dead_code, reason = "Make default's question (Task 6 of 9b4) shows it")]
pub fn risky_note() -> Option<String> {
    let exe = gezik_platform::system::exe().ok()?;
    let downloads = gezik_platform::system::places().downloads;
    let removable = gezik_platform::system::is_removable(&exe);
    let place = risky_place(&exe, downloads.as_deref(), &std::env::temp_dir(), removable)?;
    Some(if cfg!(windows) {
        format!(
            "Gezik is in {place}. If it is moved or deleted, folders will not open until you repair it here or open {RESTORE_REG}."
        )
    } else {
        format!(
            "Gezik is in {place}. If it is moved or deleted, folders will not open in Gezik until you repair it here."
        )
    })
}

/// Two seconds after start (decision 13): the Repair question's text if a registration points
/// at another exe, and whether Linux should answer as FileManager1. With no journal: one stat.
#[allow(dead_code, reason = "the start-up check (Task 6 of 9b4) calls it")]
pub fn idle_check() -> (Option<String>, bool) {
    let Some(file) = journal_file() else { return (None, false) };
    if !file.exists() {
        return (None, false);
    }
    let snapshot = read_snapshot();
    let moved =
        matches!(snapshot.path, PathState::Moved { .. }) || matches!(snapshot.default, DefaultState::Moved { .. });
    let note = moved.then(|| {
        "System registrations still point to Gezik's old place; folders will not open in Gezik until they are repaired."
            .to_owned()
    });
    (note, Os::HERE == Os::Linux && snapshot.default == DefaultState::On)
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
        Places {
            home: Some(PathBuf::from(r"C:\Users\u")),
            local_app_data: Some(PathBuf::from(LAD)),
            ..Places::default()
        }
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
        apply_all(&mut file.lock().map_err(|e| e.to_string())?, fake, targets, exe)
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
        let unix = Places { home: Some(PathBuf::from("/home/u")), ..Places::default() };
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
            assert!(allowed(target, &places, Os::Windows), "{target:?}");
        }
        let upper = Change { place: CMD.to_uppercase(), ..ok[2].clone() };
        assert!(allowed(&upper, &places, Os::Windows), "Windows paths without case");
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
            assert!(!allowed(&bad, &places, Os::Windows), "{bad:?}");
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
        let err = apply_all(&mut file.lock().unwrap(), &Taken(Fake::default()), targets, exe).unwrap_err();
        assert!(err.contains("not made by Gezik"), "{err}");
        assert!(err.contains("taken back"), "{err}");
        assert!(!file.exists(), "the folders this run made were taken back");
    }

    #[test]
    fn a_foreign_file_is_left_alone() {
        let fake = Fake::default();
        fake.put(Kind::File, CMD, "", Value::Text("@echo mine".into()));
        let file = journal("foreign");
        let err = add(&fake, &file, r"C:\T\gezik.exe").unwrap_err();
        assert!(err.contains("not made by Gezik"), "{err}");
        assert_eq!(fake.get(Kind::File, CMD, ""), Some(Value::Text("@echo mine".into())));
        assert_eq!(fake.0.borrow().len(), 1, "nothing made: refused before writing");
        assert!(!file.exists());
        let state = snapshot(Some(&file), &fake, &win_places(), r"C:\T\gezik.exe", Os::Windows);
        assert_eq!(state.path, PathState::Taken { place: CMD.into() });
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
        let report = unregister(Some(&file), &fake, &win_places(), Path::new(r"C:\T\gezik.exe"), Os::Windows);
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
        let report = unregister(Some(&file), &access, &win_places(), Path::new("x"), Os::Windows);
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
        let link = &path_targets(&Places { home: Some("/h".into()), ..Places::default() }, "", false).unwrap()[2];
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
        let places = Places { local_app_data: Some(temp.clone()), ..Places::default() };
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
        let report = unregister(Some(&file), &access, &places, Path::new("x"), Os::Windows);
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
        let report = unregister(Some(&file), &fake, &win_places(), Path::new("x"), Os::Windows);
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
        let report =
            unregister(Some(&journal("sweep")), &fake, &win_places(), Path::new(r"C:\T\gezik.exe"), Os::Windows);
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
        assert!(sweep_changes(&other, &win_places(), Path::new("x"), Os::Windows).is_empty());
    }

    #[test]
    fn path_state_follows_what_is_there() {
        let exe = r"C:\T\gezik.exe";
        let fake = Fake::default();
        let file = journal("state");
        let state = |fake: &Fake| snapshot(Some(&file), fake, &win_places(), exe, Os::Windows).path;
        assert_eq!(state(&fake), PathState::Off);
        add(&fake, &file, exe).unwrap();
        assert_eq!(state(&fake), PathState::On { command: CMD.into() });
        fake.put(Kind::PathEntry, ENVIRONMENT, "Path", reg(&format!(r"{BIN};C:\later")));
        assert_eq!(state(&fake), PathState::On { command: CMD.into() }, "other PATH parts may change");
        let moved = snapshot(Some(&file), &fake, &win_places(), r"D:\New\gezik.exe", Os::Windows).path;
        assert_eq!(moved, PathState::Moved { old: exe.into() });
        let same_other_case = snapshot(Some(&file), &fake, &win_places(), &exe.to_uppercase(), Os::Windows).path;
        assert_eq!(same_other_case, PathState::On { command: CMD.into() });
        fake.put(Kind::File, CMD, "", Value::Text("@echo edited".into()));
        assert_eq!(state(&fake), PathState::Changed);
        let taken = Fake::default();
        taken.put(Kind::File, CMD, "", Value::Text("@echo mine".into()));
        assert_eq!(state_of(&taken), PathState::Taken { place: CMD.into() });
    }

    fn state_of(fake: &Fake) -> PathState {
        snapshot(Some(&journal("taken")), fake, &win_places(), r"C:\T\gezik.exe", Os::Windows).path
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
        let places = Places { local_app_data: Some(temp.clone()), ..Places::default() };
        let access = SystemAccess { places: places.clone(), registry_root: root.clone() };
        let file = JournalFile::new(temp.join("config"));
        let exe = r"C:\Tools\Gezik ç\gezik.exe";
        let targets = path_targets(&places, exe, true).unwrap();
        assert_eq!(apply_all(&mut file.lock().unwrap(), &access, targets, exe), Ok(true));
        let (ty, added) = reg::read_value(&env, "Path").unwrap().unwrap();
        assert_eq!(ty, RegType::Sz, "the type stays");
        assert_eq!(added, format!(r"{user_path};{}\Gezik\bin", temp.display()));
        assert_eq!(std::fs::read_to_string(temp.join(r"Gezik\bin\gezik.cmd")).unwrap(), SHIM);
        let app_path = format!(r"{root}\Software\Microsoft\Windows\CurrentVersion\App Paths\gezik.exe");
        assert_eq!(reg::read_value(&app_path, "").unwrap(), Some((RegType::Sz, exe.to_owned())));
        let report = unregister(Some(&file), &access, &places, Path::new(exe), Os::Windows);
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

    const EXE: &str = r"C:\T\gezik.exe";
    const DIR_SHELL: &str = r"HKCU\Software\Classes\Directory\shell";
    const WIN_E_CMD: &str =
        r"HKCU\Software\Classes\CLSID\{52205fd8-5dfb-447d-801a-d0b52f2e83e1}\shell\opennewwindow\command";

    fn win_default_targets() -> Vec<Change> {
        default_targets(&win_places(), EXE, Os::Windows, Some(&|_: &Path| None)).unwrap()
    }

    /// The real write checks (`SystemAccess::set`'s) over the Fake.
    struct Checked<'a>(&'a Fake, Places);

    impl Access for Checked<'_> {
        fn current(&self, c: &Change) -> io::Result<Value> {
            self.0.current(c)
        }
        fn set(&self, c: &Change, value: &Value) -> io::Result<()> {
            check_write(c, &self.0.current(c)?, value, &self.1, Os::Windows, self.0)?;
            self.0.set(c, value)
        }
    }

    /// Folders and keys listed here hold something: taking them away is `DirectoryNotEmpty`.
    struct Full<'a>(&'a Fake, RefCell<Vec<String>>);

    impl Access for Full<'_> {
        fn current(&self, c: &Change) -> io::Result<Value> {
            self.0.current(c)
        }
        fn set(&self, c: &Change, value: &Value) -> io::Result<()> {
            if *value == Value::Absent && self.1.borrow().contains(&c.place.to_lowercase()) {
                return Err(io::Error::new(io::ErrorKind::DirectoryNotEmpty, "not empty"));
            }
            self.0.set(c, value)
        }
    }

    #[test]
    fn default_places_are_exact() {
        let places = win_places();
        for target in win_default_targets() {
            assert!(allowed(&target, &places, Os::Windows), "{target:?}");
        }
        for (kind, place, name) in [
            (Kind::RegistryValue, r"HKCU\Software\Classes\exefile\shell\open\command", ""),
            (Kind::RegistryValue, DIR_SHELL, "Other"),
            (Kind::RegistryValue, r"HKCU\Software\Classes\Directory\shell\gezik2\command", ""),
            (Kind::RegistryKey, r"HKCU\Software\Classes\Directory\shell\open", ""),
            (Kind::RegistryValue, r"HKCU\Software\Classes\Folder\shell\open\command", ""),
            (Kind::File, r"C:\Users\u\AppData\Local\Gezik\bin\open-folder.js", ""),
            (Kind::MacPref, "AppleLanguages", ""),
            (Kind::Mimeapps, "/etc/xdg/mimeapps.list", "inode/directory"),
        ] {
            let bad = Change { kind, place: place.into(), name: name.into(), ..win_default_targets()[0].clone() };
            assert!(!allowed(&bad, &places, Os::Windows), "{bad:?}");
        }
        let other_feature = Change { feature: FEATURE_PATH.into(), ..win_default_targets()[3].clone() };
        assert!(!allowed(&other_feature, &places, Os::Windows), "the feature is part of the place");
    }

    #[test]
    fn only_gezik_shaped_default_values_are_written() {
        use gezik_platform::system::text::shell_command;
        let at = |place: &str, name: &str| {
            win_default_targets()
                .into_iter()
                .find(|t| t.kind == Kind::RegistryValue && t.place == place && t.name == name)
                .unwrap()
        };
        let ok = |c: &Change, to: Value| value_allowed(c, &Value::Absent, &to);
        let cmd = at(&format!(r"{DIR_SHELL}\gezik\command"), "");
        assert!(ok(&cmd, reg(&shell_command(r"D:\Ç\gezik.exe", "%1"))));
        assert!(!ok(&cmd, reg(r#""C:\evil.exe" "%1""#)));
        assert!(!ok(&cmd, reg(&shell_command(r"D:\evil.exe", "%1"))), "not a gezik.exe");
        assert!(!ok(&cmd, reg(&shell_command(EXE, ""))), "Win+E's form under a folder verb");
        let expand = Value::Reg { ty: RegType::ExpandSz, data: shell_command(EXE, "%1") };
        assert!(!ok(&cmd, expand), "REG_SZ only: an EXPAND_SZ would open %…%");
        let shell = at(DIR_SHELL, "");
        assert!(ok(&shell, reg("gezik")) && ok(&shell, reg("openinxyplorer")) && ok(&shell, reg("none")));
        assert!(!ok(&shell, reg(r"C:\x.exe")));
        let delegate = at(WIN_E_CMD, "DelegateExecute");
        assert!(ok(&delegate, reg("")) && !ok(&delegate, reg("{11dbb47c-a525-400b-9e80-a54615a090c0}")));
        let win_e = at(WIN_E_CMD, "");
        assert!(ok(&win_e, reg(&shell_command(EXE, ""))) && !ok(&win_e, reg(&shell_command(EXE, "%1"))));
        let title = at(&format!(r"{DIR_SHELL}\gezik"), "");
        assert!(ok(&title, reg("Open in Gezik")) && !ok(&title, reg("Open")));
        assert!(!ok(&shell, Value::Reg { ty: RegType::ExpandSz, data: "gezik".into() }), "REG_SZ only");
    }

    #[test]
    fn the_switch_is_written_last() {
        let targets = win_default_targets();
        let first_switch =
            targets.iter().position(|t| t.place.ends_with(r"\shell") && t.kind == Kind::RegistryValue).unwrap();
        assert!(
            targets[first_switch..].iter().all(|t| t.kind == Kind::RegistryValue && t.place.ends_with(r"\shell")),
            "only the three switches after the first"
        );
        assert_eq!(targets.len() - first_switch, 3);
    }

    #[test]
    fn a_foreign_win_e_handler_refuses_before_writing() {
        let fake = Fake::default();
        fake.put(Kind::RegistryValue, WIN_E_CMD, "", reg(r#""C:\Opus\dopusrt.exe" /open"#));
        let file = journal("foreign-win-e");
        let err = apply_all(&mut file.lock().unwrap(), &fake, win_default_targets(), EXE).unwrap_err();
        assert!(err.contains("not made by Gezik"), "{err}");
        assert_eq!(fake.0.borrow().len(), 1, "nothing written but the foreign value");
        assert!(!file.exists(), "no journal entry either");
        // As the Files app registers itself: an EXPAND_SZ command and an empty DelegateExecute.
        let files = Fake::default();
        let launcher = r#""%LOCALAPPDATA%\Files\Files.App.Launcher.exe""#;
        files.put(Kind::RegistryValue, WIN_E_CMD, "", Value::Reg { ty: RegType::ExpandSz, data: launcher.into() });
        files.put(Kind::RegistryValue, WIN_E_CMD, "DelegateExecute", reg(""));
        let mut key = r"HKCU\Software\Classes\CLSID".to_owned();
        for part in ["{52205FD8-5DFB-447D-801A-D0B52F2E83E1}", "shell", "opennewwindow", "command"] {
            key = format!(r"{key}\{part}");
            files.put(Kind::RegistryKey, &key, "", Value::Present);
        }
        let before = files.0.borrow().clone();
        let err = apply_all(&mut file.lock().unwrap(), &files, win_default_targets(), EXE).unwrap_err();
        assert!(err.contains("not made by Gezik"), "{err}");
        assert_eq!(*files.0.borrow(), before);
        let state = snapshot(Some(&file), &files, &win_places(), EXE, Os::Windows).default;
        assert!(matches!(state, DefaultState::Taken { ref why } if why.contains("not made by Gezik")), "{state:?}");
        // The sweep leaves the other program's keys and DelegateExecute alone too.
        assert!(sweep_changes(&files, &win_places(), Path::new(EXE), Os::Windows).is_empty());
    }

    #[test]
    fn a_win_e_taken_since_keeps_its_delegate_execute() {
        let fake = Fake::default();
        let checked = Checked(&fake, win_places());
        let file = journal("win-e-taken-since");
        apply_all(&mut file.lock().unwrap(), &checked, win_default_targets(), EXE).unwrap();
        // The Files app takes Win+E after Make default.
        let launcher =
            Value::Reg { ty: RegType::ExpandSz, data: r#""%LOCALAPPDATA%\Files\Files.App.Launcher.exe""#.into() };
        fake.put(Kind::RegistryValue, WIN_E_CMD, "", launcher.clone());
        let undone = undo_matching(&mut file.lock().unwrap(), &checked, |c| c.feature == FEATURE_DEFAULT);
        assert_eq!(fake.get(Kind::RegistryValue, WIN_E_CMD, "DelegateExecute"), Some(reg("")), "{:?}", undone.lines);
        assert_eq!(fake.get(Kind::RegistryValue, WIN_E_CMD, ""), Some(launcher));
        assert!(undone.lines.iter().any(|l| l.starts_with("left as is") && l.contains("DelegateExecute")));
        assert!(fake.get(Kind::RegistryValue, DIR_SHELL, "").is_none(), "the rest taken back");
    }

    #[test]
    fn another_default_verb_comes_back() {
        let fake = Fake::default();
        fake.put(Kind::RegistryValue, DIR_SHELL, "", reg("openinxyplorer"));
        let file = journal("other-verb");
        apply_all(&mut file.lock().unwrap(), &fake, win_default_targets(), EXE).unwrap();
        assert_eq!(fake.get(Kind::RegistryValue, DIR_SHELL, ""), Some(reg("gezik")));
        assert_eq!(snapshot(Some(&file), &fake, &win_places(), EXE, Os::Windows).default, DefaultState::On);
        undo_matching(&mut file.lock().unwrap(), &fake, |c| c.feature == FEATURE_DEFAULT);
        assert_eq!(fake.get(Kind::RegistryValue, DIR_SHELL, ""), Some(reg("openinxyplorer")));
        assert_eq!(fake.0.borrow().len(), 1, "everything else gone: {:?}", fake.0.borrow());
    }

    #[test]
    fn a_failure_midway_takes_back_this_run() {
        /// A Fake that fails the 20th write.
        struct Failing(Fake, std::cell::Cell<usize>);
        impl Access for Failing {
            fn current(&self, c: &Change) -> io::Result<Value> {
                self.0.current(c)
            }
            fn set(&self, c: &Change, v: &Value) -> io::Result<()> {
                self.1.set(self.1.get() + 1);
                if self.1.get() == 20 {
                    return Err(io::Error::other("disk full"));
                }
                self.0.set(c, v)
            }
        }
        let failing = Failing(Fake::default(), std::cell::Cell::new(0));
        let file = journal("midway");
        let err = apply_all(&mut file.lock().unwrap(), &failing, win_default_targets(), EXE).unwrap_err();
        assert!(err.contains("disk full") && err.contains("taken back"), "{err}");
        assert!(failing.0.0.borrow().is_empty(), "as before: {:?}", failing.0.0.borrow());
        assert!(!file.exists());
    }

    #[test]
    fn a_tampered_default_entry_is_not_acted_on() {
        let fake = Fake::default();
        let file = journal("tampered-default");
        let evil = Change {
            feature: FEATURE_DEFAULT.into(),
            kind: Kind::RegistryValue,
            place: format!(r"{DIR_SHELL}\gezik\command"),
            name: String::new(),
            entry: String::new(),
            before: reg(r#""C:\evil.exe" "%1""#),
            after: reg(&gezik_platform::system::text::shell_command(EXE, "%1")),
            done: true,
        };
        fake.put(Kind::RegistryValue, &evil.place, "", evil.after.clone());
        file.lock().unwrap().record(evil.clone(), EXE, |_| Ok(())).unwrap();
        let undone = undo_matching(&mut file.lock().unwrap(), &Checked(&fake, win_places()), |_| true);
        assert_eq!(fake.get(Kind::RegistryValue, &evil.place, ""), Some(evil.after.clone()), "not written back");
        assert!(undone.lines[0].starts_with("failed: "), "{:?}", undone.lines);
    }

    #[test]
    fn restore_reg_follows_the_journal() {
        let fake = Fake::default();
        let file = journal("reg-file");
        apply_all(&mut file.lock().unwrap(), &fake, win_default_targets(), EXE).unwrap();
        // A hand-edited entry at a place that is not Gezik's never reaches the file.
        let foreign = Change {
            place: r"HKCU\Software\Classes\exefile\shell\open\command".into(),
            ..win_default_targets()[6].clone()
        };
        let foreign = Change { before: reg(r#""C:\evil.exe" "%1""#), done: true, ..foreign };
        file.lock().unwrap().record(foreign, EXE, |_| Ok(())).unwrap();
        write_restore_reg(&file.lock().unwrap(), Os::Windows).unwrap();
        let reg_path = file.dir().join(RESTORE_REG);
        let bytes = std::fs::read(&reg_path).unwrap();
        assert_eq!(bytes[..2], [0xFF, 0xFE]);
        let units: Vec<u16> = bytes[2..].chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        let text = String::from_utf16(&units).unwrap();
        assert!(text.contains("[HKEY_CURRENT_USER\\Software\\Classes\\Directory\\shell]\r\n@=-"), "{text}");
        assert!(text.contains("[-HKEY_CURRENT_USER\\Software\\Classes\\Directory\\shell\\gezik]"));
        assert!(!text.contains("exefile") && !text.contains("evil"), "{text}");
        for whole in
            ["Directory]", "Drive]", "Folder]", "Directory\\shell]", "Drive\\shell]", "Folder\\shell]", "CLSID"]
        {
            assert!(!text.contains(&format!("[-HKEY_CURRENT_USER\\Software\\Classes\\{whole}")), "{whole}: {text}");
        }
        assert_eq!(text.matches("[-").count(), 3, "only the three gezik verb keys: {text}");
        assert!(text.contains("; Win+E: the next lines take back Gezik's Win+E command"), "{text}");
        undo_matching(&mut file.lock().unwrap(), &fake, |c| c.feature == FEATURE_DEFAULT);
        write_restore_reg(&file.lock().unwrap(), Os::Windows).unwrap();
        assert!(!reg_path.exists(), "no default entries: no file");
    }

    #[test]
    fn a_key_holding_another_entry_stays_in_the_journal() {
        let fake = Fake::default();
        let file = journal("shared-key");
        let parent = Change {
            feature: "a".into(),
            kind: Kind::RegistryKey,
            place: r"HKCU\Software\Classes\X".into(),
            name: String::new(),
            entry: String::new(),
            before: Value::Absent,
            after: Value::Present,
            done: true,
        };
        let child = Change { feature: "b".into(), place: r"HKCU\Software\Classes\X\Y".into(), ..parent.clone() };
        let sibling = Change { feature: "c".into(), place: r"HKCU\Software\Classes\XY".into(), ..parent.clone() };
        for change in [&parent, &child, &sibling] {
            fake.put(Kind::RegistryKey, &change.place, "", Value::Present);
        }
        {
            let mut locked = file.lock().unwrap();
            for change in [&parent, &child, &sibling] {
                locked.record(change.clone(), EXE, |_| Ok(())).unwrap();
            }
        }
        let full = Full(&fake, RefCell::new(vec![parent.place.to_lowercase(), sibling.place.to_lowercase()]));
        undo_matching(&mut file.lock().unwrap(), &full, |c| c.feature != "b");
        let left: Vec<String> = file.lock().unwrap().journal().changes.iter().map(|c| c.feature.clone()).collect();
        assert_eq!(left, ["a", "b"], "a kept: b's key is under it; XY is not under X");
        full.1.borrow_mut().clear();
        undo_matching(&mut file.lock().unwrap(), &full, |_| true);
        assert!(!file.exists());
        assert_eq!(fake.0.borrow().len(), 1, "only XY, left not empty: {:?}", fake.0.borrow());
    }

    #[test]
    fn risky_places_are_named() {
        let temp = Path::new(r"C:\Users\u\AppData\Local\Temp");
        let downloads = Path::new(r"C:\Users\u\Downloads");
        let place = |exe: &str, removable| risky_place(Path::new(exe), Some(downloads), temp, removable);
        assert_eq!(place(r"C:\Users\u\Downloads\gezik.exe", false), Some("Downloads"));
        assert_eq!(place(r"c:\users\U\appdata\local\temp\x\gezik.exe", false), Some("a temporary folder"));
        assert_eq!(place(r"E:\gezik.exe", true), Some("a removable drive"));
        assert_eq!(place(r"C:\Tools\gezik.exe", false), None);
        assert_eq!(place(r"C:\Users\u\Downloads2\gezik.exe", false), None, "a whole folder name");
    }

    #[test]
    fn default_state_follows_what_is_there() {
        let made: Vec<(Change, Value)> =
            win_default_targets().into_iter().map(|t| (Change { done: true, ..t.clone() }, t.after)).collect();
        assert_eq!(default_state(&made, EXE, None), DefaultState::On);
        assert_eq!(default_state(&made, &EXE.to_uppercase(), None), DefaultState::On);
        assert_eq!(default_state(&made, r"D:\New\gezik.exe", None), DefaultState::Moved { old: EXE.into() });
        let mut changed = made.clone();
        let switch = changed.iter_mut().find(|(c, _)| c.kind == Kind::RegistryValue && c.place == DIR_SHELL).unwrap();
        switch.1 = reg("openinxyplorer");
        assert_eq!(default_state(&changed, EXE, None), DefaultState::Changed);
        let half: Vec<(Change, Value)> = made.iter().filter(|(c, _)| !c.place.ends_with(r"\shell")).cloned().collect();
        assert_eq!(default_state(&half, EXE, None), DefaultState::Changed, "stopped before the switch");
        assert_eq!(default_state(&[], EXE, Some("x".into())), DefaultState::Taken { why: "x".into() });
        assert_eq!(default_state(&[], EXE, None), DefaultState::Off);
    }

    #[test]
    fn linux_and_mac_targets_and_values() {
        let places = Places {
            home: Some("/home/u".into()),
            data_home: Some("/home/u/.local/share".into()),
            config_home: Some("/home/u/.config".into()),
            desktops: vec!["gnome".into()],
            ..Places::default()
        };
        let exe = "/home/u/apps/gezik";
        let all = default_targets(&places, exe, Os::Linux, None).unwrap();
        assert!(all.iter().any(|t| t.place == "/home/u/.config/gnome-mimeapps.list"), "every candidate");
        let read =
            |p: &Path| (p == Path::new("/home/u/.config/mimeapps.list")).then(|| "[Default Applications]\n".to_owned());
        let some = default_targets(&places, exe, Os::Linux, Some(&read)).unwrap();
        assert!(!some.iter().any(|t| t.place.ends_with("gnome-mimeapps.list")), "not there: not edited");
        assert!(!some.iter().any(|t| t.kind == Kind::File && t.place.ends_with("/mimeapps.list")), "there: not made");
        let entry = some.iter().find(|t| t.kind == Kind::Mimeapps).unwrap();
        assert!(value_allowed(entry, &Value::Absent, &Value::Text("gezik.desktop;".into())));
        assert!(value_allowed(entry, &Value::Absent, &Value::Text("org.gnome.Nautilus.desktop;".into())));
        assert!(!value_allowed(entry, &Value::Absent, &Value::Text("/bin/sh".into())));
        let desktop = all.iter().find(|t| t.place.ends_with("gezik.desktop")).unwrap();
        assert!(value_allowed(desktop, &Value::Absent, &desktop.after));
        assert!(!value_allowed(desktop, &Value::Absent, &Value::Text("[Desktop Entry]\nExec=/bin/sh\n".into())));
        for target in &all {
            assert!(allowed(target, &places, Os::Linux), "{target:?}");
        }

        let mac = Places { home: Some("/Users/u".into()), ..Places::default() };
        let in_bundle = default_targets(&mac, "/Applications/Gezik.app/Contents/MacOS/gezik", Os::Mac, None).unwrap();
        assert_eq!(in_bundle.iter().map(|t| t.kind).collect::<Vec<_>>(), [Kind::MacDefault, Kind::MacPref]);
        let loose = default_targets(&mac, "/Users/u/bin/gezik", Os::Mac, None).unwrap();
        let link = loose.iter().find(|t| t.kind == Kind::Symlink).unwrap();
        assert_eq!(link.place, "/Users/u/Applications/Gezik.app/Contents/MacOS/gezik");
        assert!(allowed(link, &mac, Os::Mac));
        let plist = loose.iter().find(|t| t.kind == Kind::File).unwrap();
        assert!(value_allowed(plist, &Value::Absent, &plist.after) && INFO_PLIST.contains("public.folder"));
        let pref = loose.iter().find(|t| t.kind == Kind::MacPref).unwrap();
        assert!(value_allowed(pref, &Value::Absent, &Value::Text("com.apple.finder".into())));
        assert!(!value_allowed(pref, &Value::Absent, &Value::Text("/x".into())));
    }

    /// `reg query` of a test key, read only.
    #[cfg(windows)]
    fn snapshot_tree(root: &str) -> String {
        let out = std::process::Command::new("reg").args(["query", &format!(r"HKCU\{root}"), "/s"]).output().unwrap();
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// The test key's whole tree goes when dropped, so a failing test leaves nothing either.
    #[cfg(windows)]
    struct TestRoot(String);

    #[cfg(windows)]
    impl Drop for TestRoot {
        fn drop(&mut self) {
            assert!(self.0.starts_with(r"Software\GezikTest-"));
            let _ = std::process::Command::new("reg").args(["delete", &format!(r"HKCU\{}", self.0), "/f"]).output();
        }
    }

    /// Spec 16.2: Make default and its undo on the real registry, under a test key only
    /// (`HKCU\Software\GezikTest-…\Software\Classes`, never the user's own Classes).
    #[cfg(windows)]
    #[test]
    fn the_real_registry_default_round_trips() {
        use gezik_platform::system::windows as reg;
        let root = format!(r"Software\GezikTest-{}-default", std::process::id());
        let guard = TestRoot(root.clone());
        reg::create_key(&root).unwrap();
        reg::create_key(&format!(r"{root}\Software")).unwrap();
        let dir = std::env::temp_dir().join(format!("gezik-default-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let places = Places { local_app_data: Some(dir.clone()), ..Places::default() };
        let access = SystemAccess { places: places.clone(), registry_root: root.clone() };
        let file = journal("real-default");
        let before = snapshot_tree(&root);
        let exe = r"D:\Araçlar\Gezik Dev\gezik.exe";
        let targets = default_targets(&places, exe, Os::Windows, None).unwrap();
        apply_all(&mut file.lock().unwrap(), &access, targets, exe).unwrap();
        let shell = format!(r"{root}\Software\Classes\Directory\shell");
        assert_eq!(reg::read_value(&shell, "").unwrap().map(|v| v.1).as_deref(), Some("gezik"));
        let command = reg::read_value(&format!(r"{shell}\gezik\command"), "").unwrap().unwrap();
        assert_eq!(command, (RegType::Sz, gezik_platform::system::text::shell_command(exe, "%1")));
        assert_eq!(snapshot(Some(&file), &access, &places, exe, Os::Windows).default, DefaultState::On);
        let report = unregister(Some(&file), &access, &places, Path::new(exe), Os::Windows);
        assert_eq!(report.code, 0, "{:?}", report.lines);
        assert_eq!(snapshot_tree(&root), before, "byte for byte");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0, "nothing left on disk");
        assert!(!file.exists() && !file.dir().join(RESTORE_REG).exists());
        drop(guard);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_sweep_takes_back_a_lost_default() {
        let fake = Fake::default();
        fake.put(Kind::RegistryKey, CLASSES, "", Value::Present);
        let file = journal("lost-default");
        apply_all(&mut file.lock().unwrap(), &fake, win_default_targets(), EXE).unwrap();
        std::fs::remove_file(file.path()).unwrap();
        let report = unregister(Some(&file), &fake, &win_places(), Path::new(r"D:\Moved\gezik.exe"), Os::Windows);
        assert_eq!(report.code, 0, "{:?}", report.lines);
        assert!(report.default_changed);
        let left = fake.0.borrow();
        assert!(left.values().all(|v| *v == Value::Present), "no value of Gezik's left: {left:?}");
        assert!(
            left.keys().all(|(_, place, _)| !place.contains("gezik") && !place.contains("52205fd8")),
            "only keys not named Gezik's: {left:?}"
        );
    }
}
