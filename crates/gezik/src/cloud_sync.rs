//! Cloud sync (spec 17 §3, §5.2): the shared folder is `cloud/` in this computer's own folder
//! (`location.toml` names `{local}/cloud`, so all of the sync folder's machinery works), and a
//! round mirrors it with the sealed copy on the server: GET, the merge
//! (`gezik_config::cloud_sync::plan`), the files written here, PUT. Its state is
//! `cloud-sync.toml` (the code; only the user may read it on Unix) and `cloud-base` (the set
//! both sides held after the last round, as a plain bundle: one atomic write). Rounds run on a
//! thread, one at a time: after start, 5 s after a change in `cloud/`, Sync Now, and at close
//! when something waits to go. Off, nothing of it runs.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use gezik_config::cloud_sync::{self as core, Files, Keys};
use gezik_config::lock;
use gezik_config::paths::write_atomic_bytes;
use gezik_config::store::ConfigStore;
use gezik_i18n::{t, tn};
use gezik_platform::http::Response;

use crate::settings_backup::{Folders, Start};

/// In this computer's folder: the shared folder while cloud sync is on.
pub const FOLDER: &str = "cloud";
const BASE: &str = "cloud-base";
const STATE: &str = "cloud-sync.toml";
/// `[cloud-sync] server`'s default.
pub const SERVER: &str = "https://sync.wenlar.com";
/// The server's limit on a sealed bundle.
const MAX: usize = 8 << 20;
/// A round begun again after another device's write (412) at most this many times.
const TRIES: usize = 3;
const AFTER_START: Duration = Duration::from_secs(2);
const AFTER_CHANGE: Duration = Duration::from_secs(5);
const AT_CLOSE: Duration = Duration::from_secs(3);

/// How the server is asked: (method, address, headers, body) → its answer. The system's HTTP
/// client in the app, a fake server in the tests.
pub type Http<'a> = &'a dyn Fn(&str, &str, &[(&str, &str)], Option<&[u8]>) -> io::Result<Response>;

/// `cloud-sync.toml`: the server, the code (plain), the box's version this computer last saw and
/// when (Unix seconds) it last synced.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct State {
    pub server: String,
    pub code: String,
    pub version: Option<u64>,
    pub last: Option<u64>,
}

impl State {
    pub fn read(local: &Path) -> Option<State> {
        let table: toml::Table = std::fs::read_to_string(local.join(STATE)).ok()?.parse().ok()?;
        let text = |key| table.get(key).and_then(toml::Value::as_str);
        let number = |key| table.get(key).and_then(toml::Value::as_integer).and_then(|n| u64::try_from(n).ok());
        Some(State {
            server: text("server")?.to_owned(),
            code: core::parse_code(text("code")?).ok()?,
            version: number("version"),
            last: number("last"),
        })
    }

    pub fn write(&self, local: &Path) -> io::Result<()> {
        let mut table = toml::Table::new();
        table.insert("server".into(), self.server.clone().into());
        table.insert("code".into(), core::display(&self.code).into());
        for (key, value) in [("version", self.version), ("last", self.last)] {
            if let Some(n) = value.and_then(|n| i64::try_from(n).ok()) {
                table.insert(key.into(), n.into());
            }
        }
        let text = toml::to_string(&table).map_err(io::Error::other)?;
        let text =
            format!("# Cloud Sync (Settings > Sync and Backup). Anyone with this code can read the settings.\n{text}");
        write_private(&local.join(STATE), text.as_bytes())
    }

    /// The box's address on the server.
    fn url(&self, keys: &Keys) -> String {
        format!("{}/v1/{}", self.server.trim_end_matches('/'), keys.id_hex())
    }
}

/// Written so that only this user may read it (the code reads the settings).
#[cfg(unix)]
fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let tmp = path.with_file_name(format!(".{STATE}.{}.tmp", std::process::id()));
    let _ = std::fs::remove_file(&tmp);
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    std::fs::rename(&tmp, path)
}

/// Under the user's profile, which only they may read already.
#[cfg(not(unix))]
fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_atomic_bytes(path, bytes)
}

/// Why a round did not end synced (spec 17 §5.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// No network, or the server would not answer now (429 twice, 5xx): the next trigger tries again.
    Offline,
    /// 403: the box was deleted or another code made it.
    Invalid,
    TooLarge,
    /// Another device wrote between our GET and PUT three times running.
    Busy,
    /// 404 for a box this computer synced with: a year without use, or Delete from Server.
    Removed,
    /// Join's 404.
    NotFound,
    Other(String),
}

impl Failure {
    pub fn text(&self) -> String {
        match self {
            Failure::Offline => t!("Not synced — offline"),
            Failure::Invalid => t!("This code is no longer valid"),
            Failure::TooLarge => t!("Too large to sync (8 MB). Large templates are the usual cause."),
            Failure::Busy => t!("Another device is syncing — try again"),
            Failure::Removed => t!("Your cloud copy was removed after a year without use. Start Syncing again?"),
            Failure::NotFound => t!("No settings found for this code"),
            Failure::Other(why) => t!("Not synced: {}", why).into(),
        }
        .into_owned()
    }

    /// Rounds stop until Stop Syncing (or Start Syncing Again).
    fn halts(&self) -> bool {
        matches!(self, Failure::Invalid | Failure::Removed)
    }
}

fn other(err: &io::Error) -> Failure {
    Failure::Other(gezik_platform::fs::describe(err))
}

/// `Authorization`'s secret: base64url without padding (the server's form).
fn base64url(bytes: &[u8]) -> String {
    crate::command_line::base64(bytes).trim_end_matches('=').replace('+', "-").replace('/', "_")
}

/// Unix seconds now.
fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Today (`2026-10-15`): the conflict copies' names.
fn today() -> String {
    gezik_platform::local_date_parts(SystemTime::now())
        .map(|p| format!("{:04}-{:02}-{:02}", p.year, p.month, p.day))
        .unwrap_or_default()
}

fn path_of(dir: &Path, name: &str) -> PathBuf {
    name.split('/').fold(dir.to_path_buf(), |path, part| path.join(part))
}

/// The files of `dir` as a bundle holds them: hidden ones (temporary files, `.DS_Store`) and
/// links left out; more than the server takes is [`Failure::TooLarge`].
fn read_files(dir: &Path) -> Result<Files, Failure> {
    fn walk(dir: &Path, prefix: &str, files: &mut Files, total: &mut usize) -> Result<(), Failure> {
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(err) => return Err(other(&err)),
        };
        for entry in entries.filter_map(Result::ok) {
            let (Ok(name), Ok(kind)) = (entry.file_name().into_string(), entry.file_type()) else { continue };
            if name.starts_with('.') {
                continue;
            }
            let name = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
            if kind.is_dir() {
                walk(&entry.path(), &name, files, total)?;
            } else if kind.is_file() {
                let data = std::fs::read(entry.path()).map_err(|err| other(&err))?;
                *total += data.len();
                if *total > MAX {
                    return Err(Failure::TooLarge);
                }
                files.insert(name, data);
            }
        }
        Ok(())
    }
    let mut files = Files::new();
    walk(dir, "", &mut files, &mut 0)?;
    Ok(files)
}

/// The last round's set (none before the first).
fn read_base(local: &Path) -> Files {
    std::fs::read(local.join(BASE)).ok().and_then(|bytes| core::unpack(&bytes).ok()).unwrap_or_default()
}

/// Whether `cloud/` changed since the last round: a round would send something.
fn pending(local: &Path) -> bool {
    read_files(&local.join(FOLDER)).is_ok_and(|files| files != read_base(local))
}

/// Asks the server; a 429 is waited out once (its Retry-After, two minutes at most).
fn call(
    http: Http,
    method: &str,
    url: &str,
    keys: &Keys,
    condition: Option<(&str, &str)>,
    body: Option<&[u8]>,
) -> Result<Response, Failure> {
    let bearer = format!("Bearer {}", base64url(&keys.auth));
    let mut headers = vec![("Authorization", bearer.as_str())];
    headers.extend(condition);
    for retried in [false, true] {
        let answer = http(method, url, &headers, body).map_err(|err| match err.kind() {
            // An address that may not be asked (http to another computer).
            io::ErrorKind::InvalidInput => Failure::Other(err.to_string()),
            _ => Failure::Offline,
        })?;
        match answer.status {
            403 => return Err(Failure::Invalid),
            413 => return Err(Failure::TooLarge),
            429 if !retried => std::thread::sleep(Duration::from_secs(answer.retry_after.unwrap_or(60).min(120))),
            429 | 500..=599 => return Err(Failure::Offline),
            _ => return Ok(answer),
        }
    }
    Err(Failure::Offline)
}

/// The version in an answer's `ETag: "<n>"`.
fn version_of(answer: &Response) -> Result<u64, Failure> {
    let tag = answer.etag.as_deref().unwrap_or_default();
    tag.trim_start_matches("W/")
        .trim_matches('"')
        .parse()
        .map_err(|_| Failure::Other(t!("the server sent no version").into_owned()))
}

fn unexpected(status: u16) -> Failure {
    Failure::Other(t!("the server answered {}", status))
}

/// The box's files and version, `None` when there is no box.
fn get(http: Http, state: &State, keys: &Keys) -> Result<Option<(Files, u64)>, Failure> {
    let answer = call(http, "GET", &state.url(keys), keys, None, None)?;
    match answer.status {
        404 => Ok(None),
        200 => {
            let version = version_of(&answer)?;
            let plain = core::open(&answer.body, keys).map_err(Failure::Other)?;
            Ok(Some((core::unpack(&plain).map_err(Failure::Other)?, version)))
        }
        status => Err(unexpected(status)),
    }
}

/// Sends `files` over `version` (none: a new box); the new version, `None` when another device
/// wrote first (412).
fn put(http: Http, state: &State, keys: &Keys, files: &Files, version: Option<u64>) -> Result<Option<u64>, Failure> {
    let plain = core::pack(files).map_err(Failure::Other)?;
    let sealed = core::seal(&plain, keys).map_err(|err| other(&err))?;
    if sealed.len() > MAX {
        return Err(Failure::TooLarge);
    }
    let tag = version.map(|v| format!("\"{v}\""));
    let condition = match &tag {
        Some(tag) => ("If-Match", tag.as_str()),
        None => ("If-None-Match", "*"),
    };
    let answer = call(http, "PUT", &state.url(keys), keys, Some(condition), Some(&sealed))?;
    match answer.status {
        200..=299 => version_of(&answer).map(Some),
        412 => Ok(None),
        status => Err(unexpected(status)),
    }
}

/// Writes the plan's files into `folder` and removes those it drops (`discard`: to the trash),
/// unless one of them changed since `here` was read: then nothing (`false`).
fn apply(folder: &Path, plan: &core::Plan, here: &Files, discard: &dyn Fn(&Path)) -> Result<bool, Failure> {
    let unchanged = |name: &String| std::fs::read(path_of(folder, name)).ok().as_ref() == here.get(name);
    if !plan.write.keys().chain(&plan.delete).all(unchanged) {
        return Ok(false);
    }
    for (name, data) in &plan.write {
        let path = path_of(folder, name);
        path.parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| write_atomic_bytes(&path, data))
            .map_err(|err| other(&err))?;
    }
    for name in &plan.delete {
        let path = path_of(folder, name);
        discard(&path);
        // Still there: the next round would send it back to every device.
        if path.exists() {
            return Err(Failure::Other(t!("Cannot remove {}", path.display())));
        }
    }
    Ok(true)
}

/// The round's end: `merged` is the new base, `version` the box's.
fn settle(local: &Path, state: &mut State, version: u64, merged: Option<&Files>) -> Result<(), Failure> {
    if let Some(merged) = merged {
        let plain = core::pack(merged).map_err(Failure::Other)?;
        write_atomic_bytes(&local.join(BASE), &plain).map_err(|err| other(&err))?;
    }
    (state.version, state.last) = (Some(version), Some(now()));
    state.write(local).map_err(|err| other(&err))
}

/// One round (spec 17 §5.2) for the cloud sync of this computer's folder `local`: the conflict
/// copies it made.
pub fn round(local: &Path, http: Http, discard: &dyn Fn(&Path)) -> Result<Vec<String>, Failure> {
    let mut state = State::read(local).ok_or_else(|| Failure::Other(t!("Cloud Sync is off").into_owned()))?;
    let keys = Keys::derive(&state.code);
    let folder = local.join(FOLDER);
    let base = read_base(local);
    // Gone as a whole (a cleanup tool, a restore): not every file deleted on purpose.
    if !folder.is_dir() && !base.is_empty() {
        return Err(Failure::Other(t!("{} is missing", folder.display())));
    }
    for _ in 0..TRIES {
        let (remote, version) = match get(http, &state, &keys)? {
            Some((files, version)) => (files, Some(version)),
            None if state.version.is_some() => return Err(Failure::Removed),
            None => (Files::new(), None),
        };
        let here = read_files(&folder)?;
        if let Some(version) = version.filter(|v| state.version == Some(*v) && here == base) {
            settle(local, &mut state, version, None)?;
            return Ok(Vec::new());
        }
        let plan = core::plan(&base, &here, &remote, &today());
        // A file changed while the round ran: again, on the files as they are now.
        if !apply(&folder, &plan, &here, discard)? {
            continue;
        }
        let version = match version {
            Some(version) if !plan.push => version,
            version => match put(http, &state, &keys, &plan.merged, version)? {
                Some(version) => version,
                None => continue,
            },
        };
        settle(local, &mut state, version, Some(&plan.merged))?;
        return Ok(plan.conflicts);
    }
    Err(Failure::Busy)
}

/// Removes `dir` and all in it; there or not.
fn clear(dir: &Path) -> Result<(), String> {
    match std::fs::remove_dir_all(dir) {
        Err(err) if err.kind() != io::ErrorKind::NotFound => {
            Err(t!("Cannot remove {0}: {1}", dir.display(), gezik_platform::fs::describe(&err)))
        }
        _ => Ok(()),
    }
}

/// The cloud sync's files go (the shared files were copied back first).
fn forget(local: &Path) {
    let _ = std::fs::remove_file(local.join(STATE));
    let _ = std::fs::remove_file(local.join(BASE));
    let _ = clear(&local.join(FOLDER));
}

/// Start Syncing: a new code, this computer's settings, themes and templates copied into
/// `cloud/` after a backup (its `[system]` to settings.local.toml), and `location.toml` naming
/// it. The caller moves the store there and runs the first round, which makes the box. The code
/// as shown.
pub fn start(folders: &Folders, server: &str, discard: &dyn Fn(&Path)) -> Result<String, String> {
    let _turn = lock(&ROUND);
    let shown = core::new_code().map_err(|err| gezik_platform::fs::describe(&err))?;
    let code = core::parse_code(&shown).map_err(|typo| gezik_i18n::t("", typo).into_owned())?;
    let local = &folders.local;
    let cloud = local.join(FOLDER);
    clear(&cloud)?;
    let _ = std::fs::remove_file(local.join(BASE));
    let state = State { server: server.to_owned(), code, ..State::default() };
    state.write(local).map_err(|err| t!("Cannot write {0}: {1}", STATE, gezik_platform::fs::describe(&err)))?;
    if let Err(why) = crate::settings_backup::start_sync(folders, &cloud, Start::Copy, None, discard) {
        forget(local);
        return Err(why);
    }
    Ok(shown)
}

/// Join With a Code: the cloud copy of code `typed` becomes this computer's settings (nothing is
/// merged), after a backup of these; this computer keeps its `[system]`. The backup's path.
pub fn join(
    folders: &Folders,
    server: &str,
    typed: &str,
    http: Http,
    discard: &dyn Fn(&Path),
) -> Result<PathBuf, Failure> {
    let _turn = lock(&ROUND);
    let code = core::parse_code(typed).map_err(|typo| Failure::Other(gezik_i18n::t("", typo).into_owned()))?;
    let mut state = State { server: server.to_owned(), code, ..State::default() };
    let keys = Keys::derive(&state.code);
    let (files, version) = get(http, &state, &keys)?.ok_or(Failure::NotFound)?;
    let local = &folders.local;
    let cloud = local.join(FOLDER);
    clear(&cloud).map_err(Failure::Other)?;
    let taken = core::Plan { write: files.clone(), ..core::Plan::default() };
    let set_up = apply(&cloud, &taken, &Files::new(), discard)
        .and_then(|_| settle(local, &mut state, version, Some(&files)))
        .and_then(|()| {
            crate::settings_backup::start_sync(folders, &cloud, Start::Use, None, discard).map_err(Failure::Other)
        });
    if set_up.is_err() {
        forget(local);
    }
    set_up
}

/// Stop Syncing: the shared files are copied back into this computer's folder (after a backup)
/// and the cloud sync's own files go; the server is not touched. The backup's path.
pub fn stop(folders: &Folders, discard: &dyn Fn(&Path)) -> Result<PathBuf, String> {
    let _turn = lock(&ROUND);
    let backup = crate::settings_backup::stop_sync(folders, discard)?;
    forget(&folders.local);
    Ok(backup)
}

/// Delete from Server: the box goes, for every device that uses the code.
pub fn delete(local: &Path, http: Http) -> Result<(), Failure> {
    let _turn = lock(&ROUND);
    let state = State::read(local).ok_or_else(|| Failure::Other(t!("Cloud Sync is off").into_owned()))?;
    let keys = Keys::derive(&state.code);
    match call(http, "DELETE", &state.url(&keys), &keys, None, None)?.status {
        200..=299 | 404 => Ok(()),
        status => Err(unexpected(status)),
    }
}

/// Start Syncing Again after the box went: the next round makes it anew with these settings.
pub fn again(local: &Path) -> io::Result<()> {
    let _turn = lock(&ROUND);
    let mut state = State::read(local).ok_or_else(|| io::Error::other("no state"))?;
    state.version = None;
    let _ = std::fs::remove_file(local.join(BASE));
    state.write(local)?;
    lock(&RUN).halted = false;
    Ok(())
}

/// The system's HTTP client, as rounds use it.
pub fn system_http(method: &str, url: &str, headers: &[(&str, &str)], body: Option<&[u8]>) -> io::Result<Response> {
    gezik_platform::http::request(method, url, headers, body, MAX as u64 + 4096)
}

/// The app's side: the store, how the last round ended, the change timer.
#[derive(Default)]
struct Run {
    store: Option<ConfigStore>,
    running: bool,
    /// When the last round ended well (Unix seconds), or why not.
    last: Option<Result<u64, Failure>>,
    halted: bool,
    /// The change timer's end, and whether its thread is up.
    due: Option<Instant>,
    timer: bool,
}

static RUN: Mutex<Run> =
    Mutex::new(Run { store: None, running: false, last: None, halted: false, due: None, timer: false });
/// One round at a time.
static ROUND: Mutex<()> = Mutex::new(());

/// Whether the store's shared folder is the cloud sync's.
pub fn on(store: &ConfigStore) -> bool {
    store.is_synced() && store.shared_dir() == store.local_dir().join(FOLDER)
}

/// At start (the first Gezik, with a folder it may write): a round once the window is up.
pub fn begin(store: ConfigStore) {
    lock(&RUN).store = Some(store);
    slint::Timer::single_shot(AFTER_START, || spawn(false, false));
}

/// A round on a thread: `only_pending`, only when `cloud/` changed since the last; `tell`, its
/// outcome in the status bar.
fn spawn(only_pending: bool, tell: bool) {
    let Some(store) = lock(&RUN).store.clone() else { return };
    let _ = std::thread::Builder::new().name("cloud sync".into()).spawn(move || run(&store, only_pending, tell));
}

/// A round now, on this thread, after the one running.
pub fn run(store: &ConfigStore, only_pending: bool, tell: bool) {
    let _turn = lock(&ROUND);
    if !on(store) || lock(&RUN).halted {
        return;
    }
    let local = store.local_dir();
    if only_pending && !pending(local) {
        return;
    }
    // The settings changes sent so far are in the file.
    store.flush_settings();
    lock(&RUN).running = true;
    refresh();
    let result = round(local, &system_http, &|path| {
        if gezik_platform::fs::trash(path).is_err() {
            let _ = std::fs::remove_file(path);
        }
    });
    let said = match &result {
        Ok(conflicts) if !conflicts.is_empty() => {
            t!("Settings synced; a key changed on both computers: Review… in Settings > Sync and Backup").into_owned()
        }
        Ok(_) => t!("Settings synced").into_owned(),
        Err(failure) => failure.text(),
    };
    {
        let mut run = lock(&RUN);
        run.running = false;
        run.halted = result.as_ref().is_err_and(Failure::halts);
        run.last = Some(result.map(|_| now()));
    }
    refresh();
    if tell {
        let _ = slint::invoke_from_event_loop(move || {
            crate::windows::with_focused(|_| crate::panes::with_active(|p| p.view.note(said)));
        });
    }
}

/// The settings window's Cloud Sync row follows.
fn refresh() {
    let _ = slint::invoke_from_event_loop(crate::settings_window::refresh_updates);
}

/// `cloud-sync-now`.
pub fn sync_now() {
    let store = lock(&RUN).store.clone();
    let Some(store) = store.filter(on) else {
        let off = t!("Cloud Sync is off: Settings > Sync and Backup turns it on").into_owned();
        crate::windows::with_focused(|_| crate::panes::with_active(|p| p.view.note(off)));
        return;
    };
    lock(&RUN).halted = false;
    let _ = std::thread::Builder::new().name("cloud sync".into()).spawn(move || run(&store, false, true));
}

/// The watcher saw a change in the shared folder: a round 5 s after the last of a burst (one
/// thread, which ends with it). The round's own writes leave nothing pending, so they start none.
pub fn changed() {
    let spawn_timer = {
        let mut run = lock(&RUN);
        if run.store.as_ref().is_none_or(|store| !on(store)) || run.halted {
            return;
        }
        run.due = Some(Instant::now() + AFTER_CHANGE);
        !std::mem::replace(&mut run.timer, true)
    };
    if !spawn_timer {
        return;
    }
    let _ = std::thread::Builder::new().name("cloud sync timer".into()).spawn(|| {
        loop {
            let wait = {
                let mut run = lock(&RUN);
                match run.due.map(|due| due.saturating_duration_since(Instant::now())) {
                    Some(wait) if !wait.is_zero() => Some(wait),
                    _ => {
                        (run.due, run.timer) = (None, false);
                        None
                    }
                }
            };
            match wait {
                Some(wait) => std::thread::sleep(wait),
                None => break,
            }
        }
        let store = lock(&RUN).store.clone();
        if let Some(store) = store {
            run(&store, true, false);
        }
    });
}

/// Quitting: what waits to go is sent, for 3 s at most.
pub fn at_close() {
    let store = lock(&RUN).store.clone();
    let Some(store) = store.filter(|store| on(store) && !lock(&RUN).halted && pending(store.local_dir())) else {
        return;
    };
    let (sent, done) = std::sync::mpsc::channel();
    let spawned = std::thread::Builder::new().name("cloud sync".into()).spawn(move || {
        run(&store, true, false);
        let _ = sent.send(());
    });
    if spawned.is_ok() {
        let _ = done.recv_timeout(AT_CLOSE);
    }
}

/// The Cloud Sync row's status line: when it last synced, or why not.
pub fn status() -> String {
    let run = lock(&RUN);
    if run.running {
        return t!("Syncing…").into_owned();
    }
    match &run.last {
        None => t!("Not synced since Gezik started").into_owned(),
        Some(Err(failure)) => failure.text(),
        Some(Ok(at)) => match now().saturating_sub(*at) / 60 {
            0 => t!("Synced just now").into_owned(),
            n @ 1..60 => tn!("Synced {n} minute ago" | "Synced {n} minutes ago", n),
            n if n < 24 * 60 => tn!("Synced {n} hour ago" | "Synced {n} hours ago", n / 60),
            _ => {
                let when = gezik_platform::local_date_parts(UNIX_EPOCH + Duration::from_secs(*at))
                    .map(|p| format!("{:04}-{:02}-{:02} {:02}:{:02}", p.year, p.month, p.day, p.hour, p.minute))
                    .unwrap_or_default();
                t!("Synced {}", when)
            }
        },
    }
}

/// Whether the last round found the box gone (Start Syncing Again is offered).
pub fn removed() -> bool {
    matches!(lock(&RUN).last, Some(Err(Failure::Removed)))
}

/// Cloud sync turned on or off here: the next round starts afresh.
pub fn reset() {
    let mut run = lock(&RUN);
    (run.last, run.halted) = (None, false);
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use gezik_config::sync_folder::conflict_of;

    use super::*;

    #[test]
    fn base64url_has_no_padding_and_the_url_alphabet() {
        for (bytes, text) in [
            (&b""[..], ""),
            (b"f", "Zg"),
            (b"fo", "Zm8"),
            (b"foo", "Zm9v"),
            (b"foob", "Zm9vYg"),
            (b"fooba", "Zm9vYmE"),
            (b"foobar", "Zm9vYmFy"),
            (&[0xfb, 0xff], "-_8"),
        ] {
            assert_eq!(base64url(bytes), text);
        }
        assert_eq!(base64url(&[0u8; 32]).len(), 43);
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gezik-cloud-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_state_file_goes_and_comes_back() {
        let dir = scratch("state");
        assert_eq!(State::read(&dir), None);
        let code = core::parse_code(&core::new_code().unwrap()).unwrap();
        let state = State { server: SERVER.to_owned(), code, version: Some(7), last: Some(1_800_000_000) };
        state.write(&dir).unwrap();
        let text = std::fs::read_to_string(dir.join(STATE)).unwrap();
        assert!(text.contains(&core::display(&state.code)), "the code as shown: {text}");
        assert_eq!(State::read(&dir), Some(state.clone()));
        let fresh = State { version: None, last: None, ..state };
        fresh.write(&dir).unwrap();
        assert_eq!(State::read(&dir), Some(fresh));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.join(STATE)).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        std::fs::write(dir.join(STATE), "server = \"x\"\ncode = \"nope\"\n").unwrap();
        assert_eq!(State::read(&dir), None, "a code that does not read");
        let _ = std::fs::remove_dir_all(&dir);
    }

    type Stored = (Vec<u8>, u64, String);

    /// The server's API (docs/cloud-sync-server.md), in memory. `race`: the next PUT finds
    /// another device's write first.
    #[derive(Default)]
    struct Fake {
        /// By id: the sealed bundle, its version, the auth that made it.
        boxes: Mutex<std::collections::HashMap<String, Stored>>,
        race: AtomicBool,
        puts: AtomicUsize,
    }

    impl Fake {
        fn answer(
            &self,
            method: &str,
            url: &str,
            headers: &[(&str, &str)],
            body: Option<&[u8]>,
        ) -> io::Result<Response> {
            let header = |name: &str| headers.iter().find(|(n, _)| *n == name).map(|(_, v)| (*v).to_owned());
            let auth = header("Authorization").unwrap_or_default();
            let reply = |status: u16, etag: Option<u64>, body: Vec<u8>| Response {
                status,
                etag: etag.map(|v| format!("\"{v}\"")),
                retry_after: None,
                body,
            };
            let id = url.rsplit('/').next().unwrap().to_owned();
            let mut boxes = self.boxes.lock().unwrap();
            let stored = boxes.get_mut(&id);
            if stored.as_ref().is_some_and(|(_, _, owner)| *owner != auth) {
                return Ok(reply(403, None, Vec::new()));
            }
            Ok(match method {
                "GET" => match stored {
                    Some((bytes, version, _)) => reply(200, Some(*version), bytes.clone()),
                    None => reply(404, None, Vec::new()),
                },
                "PUT" => {
                    self.puts.fetch_add(1, Ordering::SeqCst);
                    let mut stored = stored;
                    if self.race.swap(false, Ordering::SeqCst)
                        && let Some((_, version, _)) = stored.as_mut()
                    {
                        *version += 1;
                    }
                    let now = stored.as_ref().map(|(_, version, _)| format!("\"{version}\""));
                    let fits = match (header("If-Match"), header("If-None-Match")) {
                        (Some(tag), _) => now.as_deref() == Some(tag.as_str()),
                        (None, Some(_)) => now.is_none(),
                        (None, None) => return Ok(reply(428, None, Vec::new())),
                    };
                    if !fits {
                        return Ok(reply(412, None, Vec::new()));
                    }
                    let version = stored.as_ref().map_or(1, |(_, version, _)| version + 1);
                    boxes.insert(id, (body.unwrap_or_default().to_vec(), version, auth));
                    reply(200, Some(version), Vec::new())
                }
                "DELETE" => {
                    boxes.remove(&id);
                    reply(204, None, Vec::new())
                }
                _ => reply(405, None, Vec::new()),
            })
        }
    }

    fn put_file(dir: &Path, name: &str, text: &str) {
        let path = path_of(dir, name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn settings(local: &Path) -> String {
        std::fs::read_to_string(local.join(FOLDER).join("settings.toml")).unwrap()
    }

    fn remove(path: &Path) {
        let _ = std::fs::remove_file(path);
    }

    /// Spec 17 §7: A starts, B joins; a change on each side reaches the other; the same key
    /// changed on both leaves a conflict copy the sync folder's Review finds; a write in between
    /// (412) starts the round again.
    #[test]
    fn two_computers_sync_through_the_server() {
        let root = scratch("two");
        let (a, b) = (root.join("a"), root.join("b"));
        put_file(&a, "settings.toml", "theme = \"nord\"\nfont-size = 13\n[system]\ntray = true\n");
        put_file(&a, "themes/frost.toml", "[colors]\n");
        put_file(&b, "settings.toml", "theme = \"light\"\n");
        let fake = Fake::default();
        let http = |method: &str, url: &str, headers: &[(&str, &str)], body: Option<&[u8]>| {
            assert!(url.starts_with("http://localhost:8787/v1/") && url.len() == 25 + 32, "{url}");
            fake.answer(method, url, headers, body)
        };
        let server = "http://localhost:8787/";

        // A starts: its settings (without [system]) make the box.
        let code = start(&Folders::one(&a), server, &remove).unwrap();
        assert!(a.join(FOLDER).join("themes/frost.toml").is_file());
        let location = std::fs::read_to_string(a.join(gezik_config::sync_folder::LOCATION_FILE)).unwrap();
        assert!(location.contains("{local}/cloud"), "{location}");
        assert!(pending(&a));
        assert_eq!(round(&a, &http, &remove), Ok(Vec::new()));
        assert!(!pending(&a));
        assert_eq!(State::read(&a).unwrap().version, Some(1));

        // B joins: a typo is told before any request; then the cloud's settings, after a backup.
        let last = if code.ends_with('X') { 'Y' } else { 'X' };
        let typo = format!("{}{last}", &code[..code.len() - 1]);
        assert!(matches!(join(&Folders::one(&b), server, &typo, &http, &remove), Err(Failure::Other(_))));
        let unknown = core::new_code().unwrap();
        assert_eq!(join(&Folders::one(&b), server, &unknown, &http, &remove), Err(Failure::NotFound));
        let backup = join(&Folders::one(&b), server, &code.to_lowercase(), &http, &remove).unwrap();
        assert!(backup.starts_with(b.join("backups")));
        assert_eq!(settings(&b), "theme = \"nord\"\nfont-size = 13\n");
        assert!(b.join(FOLDER).join("themes/frost.toml").is_file());
        assert_eq!(
            gezik_config::sync_folder::read_location(&b, &gezik_config::paths::KnownDirs::new(Vec::new())),
            Some(b.join(FOLDER))
        );
        assert!(!pending(&b));

        // A changes the theme, B the font size: after the rounds both have both.
        put_file(&a.join(FOLDER), "settings.toml", "theme = \"dracula\"\nfont-size = 13\n");
        put_file(&b.join(FOLDER), "settings.toml", "theme = \"nord\"\nfont-size = 15\n");
        put_file(&b.join(FOLDER), "templates/Report.txt", "R");
        assert_eq!(round(&a, &http, &remove), Ok(Vec::new()));
        assert_eq!(round(&b, &http, &remove), Ok(Vec::new()));
        assert_eq!(round(&a, &http, &remove), Ok(Vec::new()));
        for side in [&a, &b] {
            assert_eq!(settings(side), "theme = \"dracula\"\nfont-size = 15\n");
            assert!(side.join(FOLDER).join("templates/Report.txt").is_file());
            assert!(!pending(side));
        }
        // Nothing changed: one GET, no PUT.
        let puts = fake.puts.load(Ordering::SeqCst);
        assert_eq!(round(&b, &http, &remove), Ok(Vec::new()));
        assert_eq!(fake.puts.load(Ordering::SeqCst), puts);

        // A removed theme goes on the other side too.
        std::fs::remove_file(a.join(FOLDER).join("themes/frost.toml")).unwrap();
        round(&a, &http, &remove).unwrap();
        round(&b, &http, &remove).unwrap();
        assert!(!b.join(FOLDER).join("themes/frost.toml").exists());

        // The same key on both: B keeps its own, the cloud's is a conflict copy beside it.
        put_file(&a.join(FOLDER), "settings.toml", "theme = \"solarized\"\nfont-size = 15\n");
        put_file(&b.join(FOLDER), "settings.toml", "theme = \"gruvbox\"\nfont-size = 15\n");
        round(&a, &http, &remove).unwrap();
        let conflicts = round(&b, &http, &remove).unwrap();
        assert_eq!(conflicts.len(), 1);
        let copy = &conflicts[0];
        assert_eq!(conflict_of(copy, &[]).map(|c| c.original), Some("settings.toml".to_owned()));
        assert!(settings(&b).contains("gruvbox"));
        let theirs = std::fs::read_to_string(b.join(FOLDER).join(copy)).unwrap();
        assert!(theirs.contains("solarized"), "{theirs}");

        // Another device writes between A's GET and PUT: the round begins again and gets through.
        put_file(&a.join(FOLDER), "themes/ice.toml", "[colors]\n");
        fake.race.store(true, Ordering::SeqCst);
        let puts = fake.puts.load(Ordering::SeqCst);
        assert!(round(&a, &http, &remove).is_ok());
        assert_eq!(fake.puts.load(Ordering::SeqCst), puts + 2, "the 412, then the PUT that took");
        assert!(!pending(&a));

        // The box with another secret: 403. The box gone after A synced with it: Removed.
        let state = State::read(&a).unwrap();
        let keys = Keys { auth: [1; 32], ..Keys::derive(&state.code) };
        assert_eq!(get(&http, &state, &keys), Err(Failure::Invalid));
        delete(&a, &http).unwrap();
        assert_eq!(round(&a, &http, &remove), Err(Failure::Removed));
        again(&a).unwrap();
        assert!(round(&a, &http, &remove).is_ok(), "made anew");

        // `cloud/` gone as a whole is not sent as every file deleted.
        let aside = a.join("cloud-aside");
        std::fs::rename(a.join(FOLDER), &aside).unwrap();
        assert!(matches!(round(&a, &http, &remove), Err(Failure::Other(_))));
        std::fs::rename(&aside, a.join(FOLDER)).unwrap();
        // A file the other side removed that will not go here stops the round before the base.
        put_file(&b.join(FOLDER), "themes/stuck.toml", "[colors]\n");
        round(&b, &http, &remove).unwrap();
        round(&a, &http, &remove).unwrap();
        std::fs::remove_file(b.join(FOLDER).join("themes/stuck.toml")).unwrap();
        round(&b, &http, &remove).unwrap();
        assert!(matches!(round(&a, &http, &|_: &Path| {}), Err(Failure::Other(_))));
        round(&a, &http, &remove).unwrap();
        assert!(!a.join(FOLDER).join("themes/stuck.toml").exists());

        // Stop: the files come back to A's own folder, the cloud's go.
        stop(&Folders { shared: a.join(FOLDER), local: a.clone() }, &remove).unwrap();
        assert!(std::fs::read_to_string(a.join("settings.toml")).unwrap().contains("tray = true"));
        assert!(!a.join(FOLDER).exists() && !a.join(STATE).exists() && !a.join(BASE).exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Spec 17 §7's smoke test against the real server and the system's HTTP client:
    /// `cargo test -p gezik cloud_sync -- --ignored`.
    #[test]
    #[ignore = "needs the network and sync.wenlar.com"]
    fn the_live_server_carries_a_change_across() {
        let root = scratch("live");
        let (a, b) = (root.join("a"), root.join("b"));
        put_file(&a, "settings.toml", "theme = \"nord\"\n");
        put_file(&b, "settings.toml", "theme = \"light\"\n");
        let code = start(&Folders::one(&a), SERVER, &remove).unwrap();
        assert_eq!(round(&a, &system_http, &remove), Ok(Vec::new()));
        join(&Folders::one(&b), SERVER, &code, &system_http, &remove).unwrap();
        assert_eq!(settings(&b), "theme = \"nord\"\n");
        put_file(&b.join(FOLDER), "settings.toml", "theme = \"nord\"\nfont-size = 15\n");
        assert_eq!(round(&b, &system_http, &remove), Ok(Vec::new()));
        assert_eq!(round(&a, &system_http, &remove), Ok(Vec::new()));
        assert!(settings(&a).contains("font-size = 15"));
        delete(&a, &system_http).unwrap();
        assert_eq!(round(&b, &system_http, &remove), Err(Failure::Removed));
        let _ = std::fs::remove_dir_all(&root);
    }
}
