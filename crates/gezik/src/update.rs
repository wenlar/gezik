//! Automatic updates (2026-10-14 güncelleme spec): once a day at most, 10 s after start, the
//! newest published GitHub Release on the channel is looked for on a thread of its own. A newer
//! one is downloaded into `<config>/update/<version>/`, checked against the release's
//! `SHA256SUMS.txt`, unpacked and asked for its `--version`; it is swapped in when Gezik ends
//! (or at Restart Now), the old one kept as `.old` until the new one has started. Where Gezik
//! cannot replace its own files a note only says that there is a new version.

use std::cell::RefCell;
use std::fmt::Display;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use gezik_config::store::ConfigStore;
use gezik_core::update::{Asset, Channel, Package, Version, newest, parse_releases, sha256_of, version_line};
use gezik_i18n::t;
use slint::SharedString;

use crate::AppWindow;

/// The releases, newest first; drafts are never in it without a token.
const FEED: &str = "https://api.github.com/repos/wenlar/gezik/releases?per_page=20";
const FEED_MAX: u64 = 1 << 20;
const SUMS_MAX: u64 = 64 << 10;
/// A check happens at most this often by itself (spec §3: daily, give or take the start time).
const EVERY: u64 = 20 * 3600;
const AFTER_START: Duration = Duration::from_secs(10);
/// How long the new program may take to say its version.
const VERSION_WAIT: Duration = Duration::from_secs(15);
/// Set for the Gezik that Restart Now starts: the process id it waits on before it looks for a
/// running Gezik (the one ending still holds the single-instance channel).
const RESTART_ENV: &str = "GEZIK_RESTARTED_FROM";
/// In `<config>/update/`: the version the last install put in place, for its first start.
const INSTALLED: &str = "installed";
/// In `<config>/update/<version>/`: downloaded, checked and unpacked.
const READY: &str = "ready";

/// An update to swap in when Gezik ends.
#[derive(Debug, Clone)]
struct Ready {
    version: String,
    /// The unpacked program (a file, or `Gezik.app`).
    new: PathBuf,
    /// What it replaces.
    target: PathBuf,
}

/// What a check found.
enum Outcome {
    UpToDate,
    Failed(String),
    Ready(Ready, String),
    /// Newer, but this install cannot be replaced by Gezik (or the release has no file for it):
    /// its version and page.
    Out(String, String),
}

/// The status bar's update note: its text, Restart Now, and the link with its page.
#[derive(Clone, Default)]
struct Note {
    text: String,
    restart: bool,
    link: String,
    page: String,
}

#[derive(Default)]
struct State {
    store: Option<ConfigStore>,
    /// The version whose note was shown (`[update] told` in state.toml): once per version.
    told: Option<String>,
    checked: Option<u64>,
    checking: bool,
    /// The last check's outcome, for Settings.
    last: Option<String>,
    ready: Option<Ready>,
    restart: bool,
    note: Option<Note>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::default();
}

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| f(&mut s.borrow_mut()))
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn running() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).expect("the workspace version is a version")
}

/// Where the releases come from: GitHub, or in debug and `measure` builds (screen tests) the JSON
/// file `GEZIK_UPDATE_FEED` names, whose files may then be local paths.
fn local_feed() -> Option<PathBuf> {
    if cfg!(any(debug_assertions, gezik_measure)) {
        std::env::var_os("GEZIK_UPDATE_FEED").filter(|f| !f.is_empty()).map(PathBuf::from)
    } else {
        None
    }
}

/// Early in main, before a running Gezik is looked for: the Gezik Restart Now started waits
/// (10 s at most) until the one that started it has ended.
pub fn wait_for_restarter() {
    let Some(pid) = std::env::var(RESTART_ENV).ok().and_then(|pid| pid.parse::<u32>().ok()) else { return };
    // SAFETY: main's first steps, before any other thread runs.
    unsafe { std::env::remove_var(RESTART_ENV) };
    let started = Instant::now();
    while gezik_platform::process::process_alive(pid) && started.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// The first Gezik's window is up: 10 s later, on a thread, an earlier update's leftovers go
/// (and its first start is said), then the releases are looked at unless `[update] check` is off
/// or they were less than 20 hours ago.
pub fn start(store: ConfigStore, checked: Option<u64>, told: Option<String>) {
    with(|s| (s.store, s.checked, s.told) = (Some(store), checked, told));
    slint::Timer::single_shot(AFTER_START, || {
        let wanted = crate::windows::loaded().map(|l| l.settings.update).unwrap_or_default();
        let due = wanted.check && with(|s| s.checked).is_none_or(|at| now().saturating_sub(at) >= EVERY);
        run(due.then_some(wanted.channel), false);
    });
}

/// `check-updates`: looks now whatever the setting and the time, and says what it found.
pub fn check_now() {
    if with(|s| s.checking) {
        return note(t!("Already checking for updates").into_owned());
    }
    note(t!("Checking for updates…").into_owned());
    let channel = crate::windows::loaded().map(|l| l.settings.update.channel).unwrap_or_default();
    run(Some(channel), true);
}

/// On a thread: tidies the update folder, then checks on `channel` (none: only tidies).
fn run(channel: Option<Channel>, asked: bool) {
    let Some(dir) = with(|s| s.store.as_ref().map(|store| store.local_dir().join("update"))) else { return };
    if channel.is_some() {
        let at = now();
        with(|s| (s.checking, s.checked) = (true, Some(at)));
        save(move |state| state.update_checked = Some(at));
    }
    let spawned = std::thread::Builder::new().name("gezik-update".into()).spawn(move || {
        let (updated, waiting) = if asked { (None, None) } else { tidy(&dir) };
        let outcome = channel.map(|channel| check(&dir, channel));
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(version) = updated {
                let page = page_of(&version);
                show(Note {
                    text: t!("Updated to {}", version),
                    restart: false,
                    link: t!("What's new").into_owned(),
                    page,
                });
            }
            if let Some(ready) = waiting {
                with(|s| {
                    s.ready.get_or_insert(ready);
                });
            }
            if let Some(outcome) = outcome {
                found(outcome, asked);
            }
        });
    });
    if spawned.is_err() {
        with(|s| s.checking = false);
    }
}

/// A check's outcome on the UI thread: kept for Settings, and said (in the status bar once per
/// version; everything when asked).
fn found(outcome: Outcome, asked: bool) {
    with(|s| s.checking = false);
    let (last, told) = match outcome {
        Outcome::UpToDate => {
            if asked {
                note(t!("Gezik is up to date").into_owned());
            }
            (t!("Up to date").into_owned(), None)
        }
        Outcome::Failed(why) => {
            eprintln!("gezik: update check: {why}");
            if asked {
                note(t!("Could not check for updates: {}", why));
            }
            (t!("Could not check: {}", why), None)
        }
        Outcome::Ready(ready, page) => {
            let version = ready.version.clone();
            with(|s| s.ready = Some(ready));
            let text = t!("Gezik {} is ready — installs when you close it", version);
            (text.clone(), Some((version, Note { text, restart: true, link: t!("What's new").into_owned(), page })))
        }
        Outcome::Out(version, page) => {
            let text = t!("Gezik {} is out", version);
            (text.clone(), Some((version, Note { text, restart: false, link: t!("Download").into_owned(), page })))
        }
    };
    with(|s| s.last = Some(last));
    if let Some((version, note)) = told
        && (asked || with(|s| s.told.as_ref() != Some(&version)))
    {
        // Asked: the note replaces "Checking for updates…".
        if asked {
            crate::windows::with_focused(|_| crate::panes::with_active(|p| p.view.clear_note()));
        }
        show(note);
        with(|s| s.told = Some(version.clone()));
        save(move |state| state.update_told = Some(version));
    }
    crate::settings_window::refresh_updates();
}

fn save(change: impl FnOnce(&mut gezik_config::settings::State) + 'static) {
    if let Some(store) = with(|s| s.store.clone()) {
        store.update_state(change);
    }
}

fn note(text: String) {
    crate::windows::with_focused(|_| crate::panes::with_active(|p| p.view.note(text)));
}

fn page_of(version: &str) -> String {
    format!("https://github.com/wenlar/gezik/releases/tag/v{version}")
}

/// Shows `note` in every window (and in those opened later, `install`).
fn show(note: Note) {
    with(|s| s.note = Some(note));
    crate::windows::for_each(apply);
}

fn apply(window: &AppWindow) {
    let note = with(|s| s.note.clone()).unwrap_or_default();
    window.set_update_text(note.text.into());
    window.set_update_act(if note.restart {
        SharedString::from(t!("Restart now").as_ref())
    } else {
        SharedString::new()
    });
    window.set_update_link(note.link.into());
}

/// A new window: the note, and what its parts do.
pub fn install(window: &AppWindow) {
    apply(window);
    window.on_update_chosen(|what| match what {
        0 => {
            with(|s| s.restart = true);
            crate::resident::quit();
        }
        1 => {
            let page = with(|s| s.note.as_ref().map(|n| n.page.clone())).unwrap_or_default();
            // Only GitHub's pages: the address comes from the network.
            if page.starts_with("https://github.com/") {
                let _ = open::that_detached(page);
            }
        }
        _ => {
            with(|s| s.note = None);
            crate::windows::for_each(apply);
        }
    });
}

/// Settings' Updates row: the last check's outcome, and when it was.
pub fn last_check() -> (String, String) {
    let (last, checked, checking) = with(|s| (s.last.clone(), s.checked, s.checking));
    let last = match last {
        _ if checking => t!("Checking…").into_owned(),
        Some(last) => last,
        None => t!("Not checked since Gezik started").into_owned(),
    };
    let when = checked
        .and_then(|at| gezik_platform::local_date_parts(UNIX_EPOCH + Duration::from_secs(at)))
        .map(|p| format!("{:04}-{:02}-{:02} {:02}:{:02}", p.year, p.month, p.day, p.hour, p.minute));
    let about = match when {
        Some(when) => t!("Gezik {}. Last check: {}", env!("CARGO_PKG_VERSION"), when),
        None => t!("Gezik {}. Not checked yet", env!("CARGO_PKG_VERSION")),
    };
    (last, about)
}

/// The end of main, and macOS' Quit: a ready update is swapped in (once), then with Restart
/// Now the Gezik in place is started.
pub fn finish() {
    let (ready, restart, store) = with(|s| (s.ready.take(), std::mem::take(&mut s.restart), s.store.clone()));
    let Some(ready) = ready else { return };
    match swap(&ready.new, &ready.target) {
        Ok(()) => {
            if let Some(store) = &store {
                let _ = fs::write(store.local_dir().join("update").join(INSTALLED), &ready.version);
            }
        }
        // Tried again at the next end (another Gezik may hold the file).
        Err(err) => eprintln!("gezik: the update to {} was not installed: {err}", ready.version),
    }
    if restart {
        let program = if cfg!(target_os = "macos") { ready.target.join("Contents/MacOS/gezik") } else { ready.target };
        let started = std::process::Command::new(program).env(RESTART_ENV, std::process::id().to_string()).spawn();
        if let Err(err) = started {
            eprintln!("gezik: cannot start Gezik again: {err}");
        }
    }
}

/// What this Gezik is and the file (or `Gezik.app`) an update replaces; `None` when it is none
/// of the release's packages (a build run from the source, macOS outside a bundle).
fn install_target() -> Option<(Package, PathBuf)> {
    let exe = gezik_platform::system::exe().ok()?;
    if cfg!(windows) {
        let portable = gezik_config::paths::location().dirs.as_ref().is_some_and(|d| d.portable);
        return Some((if portable { Package::WindowsPortable } else { Package::Windows }, exe));
    }
    if cfg!(target_os = "macos") {
        let app = exe.ancestors().nth(3)?;
        let bundled = exe.ends_with("Contents/MacOS/gezik") && app.extension().is_some_and(|e| e == "app");
        return bundled.then(|| (Package::MacApp, app.to_path_buf()));
    }
    if cfg!(target_os = "linux") {
        // The AppImage's runtime says where the file is; its binary runs from a mount.
        return match std::env::var_os("APPIMAGE").map(PathBuf::from).filter(|p| p.is_absolute()) {
            Some(image) => Some((Package::AppImage, image)),
            None => Some((Package::LinuxTar, exe)),
        };
    }
    None
}

/// Whether Gezik may replace `target`: its folder takes a new file (trying says more than its
/// permissions), and on macOS it is not run from a disk image or a translocated copy.
fn writable(target: &Path) -> bool {
    let text = target.to_string_lossy();
    if cfg!(target_os = "macos") && (text.starts_with("/Volumes/") || text.contains("/AppTranslocation/")) {
        return false;
    }
    let Some(dir) = target.parent() else { return false };
    let probe = dir.join(format!(".gezik-update-{}", std::process::id()));
    let made = fs::OpenOptions::new().write(true).create_new(true).open(&probe).is_ok();
    made && fs::remove_file(&probe).is_ok()
}

/// Leftovers of an earlier update: the version that started for the first time (its `.old`
/// removed), and an update downloaded but not installed yet (another Gezik held the file).
/// Folders of this version and older go.
fn tidy(dir: &Path) -> (Option<String>, Option<Ready>) {
    let running = running();
    let installed = fs::read_to_string(dir.join(INSTALLED)).ok();
    if installed.is_some() {
        let _ = fs::remove_file(dir.join(INSTALLED));
    }
    if let Some((_, target)) = install_target() {
        let _ = remove_any(&beside(&target, ".old"));
    }
    let mut waiting: Option<Ready> = None;
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let Some(version) = entry.file_name().to_str().and_then(Version::parse) else { continue };
        let path = entry.path();
        if version <= running {
            let _ = fs::remove_dir_all(&path);
        } else if path.join(READY).is_file()
            && waiting.as_ref().is_none_or(|w| Version::parse(&w.version).is_some_and(|w| version > w))
            && let Some((package, target)) = install_target()
            && let Some(new) = unpacked(&path, package)
        {
            waiting = Some(Ready { version: version.as_str().to_owned(), new, target });
        }
    }
    (installed.filter(|v| v.trim() == running.as_str()).map(|v| v.trim().to_owned()), waiting)
}

/// Looks for a newer release on `channel` and gets it ready.
fn check(dir: &Path, channel: Channel) -> Outcome {
    match look(dir, channel) {
        Ok(outcome) => outcome,
        Err(why) => Outcome::Failed(why),
    }
}

fn look(dir: &Path, channel: Channel) -> Result<Outcome, String> {
    let local = local_feed();
    fs::create_dir_all(dir).map_err(text)?;
    let list = dir.join("releases.json");
    match &local {
        Some(feed) => fetch(&feed.to_string_lossy(), &list, FEED_MAX, true),
        None => fetch(FEED, &list, FEED_MAX, false),
    }
    .map_err(text)?;
    let bytes = fs::read(&list).map_err(text);
    let _ = fs::remove_file(&list);
    let releases = parse_releases(&bytes?).ok_or_else(|| t!("GitHub's answer could not be read").into_owned())?;
    let Some((version, release)) = newest(&releases, channel, &running()) else { return Ok(Outcome::UpToDate) };
    let out = Outcome::Out(version.as_str().to_owned(), release.html_url.clone());
    let Some((package, target)) = install_target().filter(|(_, target)| writable(target)) else { return Ok(out) };
    let (Some(asset), Some(sums)) = (release.asset(&package.asset_name(&version)), release.asset("SHA256SUMS.txt"))
    else {
        return Ok(out);
    };
    let folder = dir.join(version.as_str());
    let new = match unpacked(&folder, package) {
        Some(new) => new,
        None => prepare(&folder, package, &version, asset, sums, local.is_some())?,
    };
    let ready = Ready { version: version.as_str().to_owned(), new, target };
    Ok(Outcome::Ready(ready, release.html_url.clone()))
}

/// Downloads `asset` into `folder`, checks it against `sums` and unpacks it; the program in it
/// must say it is `version`. Marked ready then.
fn prepare(
    folder: &Path,
    package: Package,
    version: &Version,
    asset: &Asset,
    sums: &Asset,
    local: bool,
) -> Result<PathBuf, String> {
    let file = download(folder, asset, sums, local)?;
    let new = unpack(folder, package, &file)?;
    let program = if package == Package::MacApp { new.join("Contents/MacOS/gezik") } else { new.clone() };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).map_err(text)?;
    }
    if !says_version(&program, version) {
        return Err(t!("the downloaded Gezik does not start here or is not {}", version.as_str()));
    }
    fs::write(folder.join(READY), "").map_err(text)?;
    Ok(new)
}

/// Downloads `asset` into `folder` as `<name>.part`, named `<name>` only once its size and
/// SHA-256 (from `sums`) match: a cut or wrong one is removed and starts over the next time.
fn download(folder: &Path, asset: &Asset, sums: &Asset, local: bool) -> Result<PathBuf, String> {
    fs::create_dir_all(folder).map_err(text)?;
    let sums_file = folder.join("SHA256SUMS.txt");
    fetch(&sums.browser_download_url, &sums_file, SUMS_MAX, local).map_err(text)?;
    let wanted = fs::read(&sums_file).ok().and_then(|bytes| sha256_of(&bytes, &asset.name));
    let wanted = wanted.ok_or_else(|| t!("{} is not in SHA256SUMS.txt", asset.name))?;
    let part = folder.join(format!("{}.part", asset.name));
    fetch(&asset.browser_download_url, &part, asset.size, local).map_err(text)?;
    let got = fs::metadata(&part).map(|m| m.len()).ok().filter(|len| *len == asset.size).and_then(|_| sha256(&part));
    if got.as_deref() != Some(wanted.as_str()) {
        let _ = fs::remove_file(&part);
        return Err(t!("{} does not match its SHA-256", asset.name));
    }
    let file = folder.join(&asset.name);
    fs::rename(&part, &file).map_err(text)?;
    Ok(file)
}

/// Unpacks the checked `file` into `folder/x` with Gezik's archive reader (every name through
/// `safe_join`: nothing lands outside it) and finds the program; the AppImage is the program.
fn unpack(folder: &Path, package: Package, file: &Path) -> Result<PathBuf, String> {
    if package != Package::AppImage {
        let content = folder.join("x");
        let _ = fs::remove_dir_all(&content);
        fs::create_dir(&content).map_err(text)?;
        let unpacking = Unpacking(RefCell::new(None));
        gezik_batch::archive::open(file).and_then(|mut a| a.extract(&content, &unpacking)).map_err(text)?;
        if let Some(err) = unpacking.0.into_inner() {
            return Err(text(err));
        }
        let _ = fs::remove_file(file);
    }
    program_in(folder, package).ok_or_else(|| t!("{} is not in the download", package.program()))
}

/// The program of a `folder` marked ready, if it is still there.
fn unpacked(folder: &Path, package: Package) -> Option<PathBuf> {
    folder.join(READY).is_file().then(|| program_in(folder, package)).flatten()
}

/// The program in the unpacked package (`x/<folder>/gezik.exe`, `x/Gezik.app`) or the AppImage.
fn program_in(folder: &Path, package: Package) -> Option<PathBuf> {
    let name = package.program();
    let fits = |path: &Path| {
        fs::symlink_metadata(path).is_ok_and(|m| if package == Package::MacApp { m.is_dir() } else { m.is_file() })
    };
    if package == Package::AppImage {
        let version = folder.file_name()?.to_str()?;
        let image = folder.join(package.asset_name(&Version::parse(version)?));
        return fits(&image).then_some(image);
    }
    let content = folder.join("x");
    let direct = content.join(name);
    if fits(&direct) {
        return Some(direct);
    }
    fs::read_dir(&content)
        .ok()?
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path().join(name))
        .find(|path| fits(path))
}

/// Whether `program --version` says `version` (within 15 s): the right package, and it runs here.
fn says_version(program: &Path, version: &Version) -> bool {
    let Ok(mut child) = gezik_platform::process::ChildProcess::spawn(program, ["--version"], None) else {
        return false;
    };
    let started = Instant::now();
    let ended = child.wait_or_stop(|| started.elapsed() > VERSION_WAIT);
    if !matches!(ended, Ok(Some(status)) if status.success()) {
        return false;
    }
    let wanted = version_line(version);
    child.stdout_lines().any(|line| line.trim() == wanted)
}

/// Downloads `url` into `dest` (at most `max` bytes); with a local feed a path is copied.
fn fetch(url: &str, dest: &Path, max: u64, local: bool) -> io::Result<()> {
    if local && !url.starts_with("https://") {
        if fs::metadata(url)?.len() > max {
            return Err(io::Error::new(io::ErrorKind::FileTooLarge, "larger than expected"));
        }
        return fs::copy(url, dest).map(|_| ());
    }
    gezik_platform::http::download(url, dest, max, false, &mut |_| {}, &|| false)
}

fn sha256(path: &Path) -> Option<String> {
    use gezik_ops::hash::{Algo, hash_file};
    hash_file(path, &[Algo::Sha256], &mut |_| true).ok()?.get(Algo::Sha256).map(str::to_owned)
}

fn text(err: impl Display) -> String {
    err.to_string()
}

/// Unpacking keeps the first entry that failed: a package missing a file is not installed.
struct Unpacking(RefCell<Option<io::Error>>);

impl gezik_batch::archive::ExtractCx for Unpacking {
    fn add_bytes(&self, _: u64) {}

    fn entry_done(&self) {}

    fn stopped(&self) -> bool {
        self.0.borrow().is_some()
    }

    fn password(&self, _: bool) -> Option<String> {
        None
    }

    fn entry_failed(&self, name: &str, error: &io::Error) {
        self.0.borrow_mut().get_or_insert_with(|| io::Error::new(error.kind(), format!("{name}: {error}")));
    }
}

/// `path` with `end` added to its name (`gezik.exe` → `gezik.exe.old`).
fn beside(path: &Path, end: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(end);
    PathBuf::from(name)
}

/// Puts `new` (a file, or `Gezik.app`) at `target`: copied beside it first (`.new`), the old one
/// renamed to `.old`, the copy renamed in. A failed step leaves the old one in place. `.old`
/// stays until the new Gezik has started (Windows: this one still runs from it).
fn swap(new: &Path, target: &Path) -> io::Result<()> {
    let (staged, old) = (beside(target, ".new"), beside(target, ".old"));
    remove_any(&staged)?;
    if let Err(err) = copy_any(new, &staged) {
        let _ = remove_any(&staged);
        return Err(err);
    }
    // An earlier update's, whose Gezik never started (or still runs: then this waits).
    if let Err(err) = remove_any(&old).and_then(|()| fs::rename(target, &old)) {
        let _ = remove_any(&staged);
        return Err(err);
    }
    if let Err(err) = fs::rename(&staged, target) {
        let _ = fs::rename(&old, target);
        let _ = remove_any(&staged);
        return Err(err);
    }
    Ok(())
}

/// Copies a file, or a folder with what is in it; a link is refused (a package has none).
fn copy_any(from: &Path, to: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(from)?;
    if meta.is_dir() {
        fs::create_dir(to)?;
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            copy_any(&entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else if meta.is_file() {
        // Its permissions come along (the program's x bits).
        fs::copy(from, to).map(|_| ())
    } else {
        Err(io::Error::new(io::ErrorKind::InvalidData, format!("{} is a link", from.display())))
    }
}

/// Removes a file or a folder; nothing there is fine.
fn remove_any(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
        Ok(meta) if meta.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gezik-update-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn asset(name: &str, path: &Path) -> Asset {
        let size = fs::metadata(path).map_or(0, |m| m.len());
        Asset { name: name.to_owned(), browser_download_url: path.display().to_string(), size }
    }

    #[test]
    fn a_file_is_swapped_and_the_old_one_kept() {
        let dir = scratch("file");
        let (target, new) = (dir.join("gezik.exe"), dir.join("new.exe"));
        fs::write(&target, "old").unwrap();
        fs::write(&new, "new").unwrap();
        // A leftover of a cut install goes first.
        fs::write(dir.join("gezik.exe.new"), "half").unwrap();
        swap(&new, &target).unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "new");
        assert_eq!(fs::read_to_string(dir.join("gezik.exe.old")).unwrap(), "old");
        assert!(!dir.join("gezik.exe.new").exists() && new.exists(), "copied, not moved");
        // The next update replaces the `.old` of the one before.
        fs::write(&new, "newer").unwrap();
        swap(&new, &target).unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "newer");
        assert_eq!(fs::read_to_string(dir.join("gezik.exe.old")).unwrap(), "new");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_bundle_is_swapped_whole() {
        let dir = scratch("app");
        let (target, new) = (dir.join("Gezik.app"), dir.join("x").join("Gezik.app"));
        fs::create_dir_all(target.join("Contents/MacOS")).unwrap();
        fs::write(target.join("Contents/MacOS/gezik"), "old").unwrap();
        fs::write(target.join("Contents/stale"), "").unwrap();
        fs::create_dir_all(new.join("Contents/MacOS")).unwrap();
        fs::write(new.join("Contents/MacOS/gezik"), "new").unwrap();
        swap(&new, &target).unwrap();
        assert_eq!(fs::read_to_string(target.join("Contents/MacOS/gezik")).unwrap(), "new");
        assert!(!target.join("Contents/stale").exists(), "nothing of the old one is left in it");
        assert_eq!(fs::read_to_string(dir.join("Gezik.app.old/Contents/MacOS/gezik")).unwrap(), "old");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_step_leaves_the_old_one_in_place() {
        let dir = scratch("fail");
        let target = dir.join("gezik");
        fs::write(&target, "old").unwrap();
        // Nothing to copy: the target is as it was, no copy is left beside it.
        assert!(swap(&dir.join("missing"), &target).is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), "old");
        assert!(!dir.join("gezik.new").exists() && !dir.join("gezik.old").exists());
        // No target (the folder went): the copy goes again.
        let new = dir.join("new");
        fs::write(&new, "new").unwrap();
        assert!(swap(&new, &dir.join("gone")).is_err());
        assert!(!dir.join("gone.new").exists() && !dir.join("gone").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    /// Windows lets a running exe be renamed but not deleted: the swap goes through, and an
    /// `.old` still running holds the next one back, the target as it was.
    #[cfg(windows)]
    #[test]
    fn a_running_exe_is_swapped() {
        let dir = scratch("running");
        let target = dir.join("gezik.exe");
        let system = std::env::var_os("SystemRoot").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
        fs::copy(system.join(r"System32\PING.EXE"), &target).unwrap();
        let mut running = std::process::Command::new(&target)
            .args(["-n", "30", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let new = dir.join("new.exe");
        fs::write(&new, "new").unwrap();
        let swapped = swap(&new, &target);
        let again = swap(&new, &target);
        let _ = running.kill();
        let _ = running.wait();
        swapped.unwrap();
        assert!(again.is_err(), "the running .old cannot be removed");
        assert_eq!(fs::read_to_string(&target).unwrap(), "new");
        assert!(!dir.join("gezik.exe.new").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_a_folder_that_takes_a_file_is_writable() {
        let dir = scratch("writable");
        assert!(writable(&dir.join("gezik.exe")));
        assert!(fs::read_dir(&dir).unwrap().next().is_none(), "the probe is gone");
        assert!(!writable(&dir.join("missing").join("gezik.exe")));
        #[cfg(target_os = "macos")]
        {
            assert!(!writable(Path::new("/Volumes/Gezik/Gezik.app")));
            assert!(!writable(Path::new("/private/var/folders/x/AppTranslocation/y/d/Gezik.app")));
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_download_is_kept_only_when_its_sha256_matches() {
        let dir = scratch("download");
        let name = "gezik-9.0.0-linux-x64.AppImage";
        let package = dir.join(name);
        fs::write(&package, "the new Gezik").unwrap();
        let hex = sha256(&package).unwrap();
        let sums = dir.join("SHA256SUMS.txt");
        fs::write(&sums, format!("{hex}  {name}\n")).unwrap();
        let folder = dir.join("update").join("9.0.0");
        let file = download(&folder, &asset(name, &package), &asset("s", &sums), true);
        assert_eq!(fs::read_to_string(file.unwrap()).unwrap(), "the new Gezik");
        assert!(!folder.join(format!("{name}.part")).exists());
        assert_eq!(program_in(&folder, Package::AppImage), Some(folder.join(name)));
        // Changed on the way: removed, and nothing is ready.
        let _ = fs::remove_dir_all(&folder);
        fs::write(&package, "the new Gezjk").unwrap();
        let wrong = asset(name, &package);
        assert!(download(&folder, &wrong, &asset("s", &sums), true).is_err());
        assert!(!folder.join(name).exists() && !folder.join(format!("{name}.part")).exists());
        // Not in the sums, or another size than the release says.
        assert!(download(&folder, &asset("other", &package), &asset("s", &sums), true).is_err());
        let longer = Asset { size: wrong.size + 1, ..wrong };
        assert!(download(&folder, &longer, &asset("s", &sums), true).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_package_is_unpacked_inside_its_folder_only() {
        let dir = scratch("unpack");
        let folder = dir.join("update").join("9.0.0");
        fs::create_dir_all(&folder).unwrap();
        let exe = ("gezik-9.0.0-windows-x64/gezik.exe".to_owned(), b"exe".to_vec());
        let zip =
            gezik_batch::archive::write_zip_entries(&[exe.clone(), ("../../../evil.txt".to_owned(), b"out".to_vec())])
                .unwrap();
        let file = folder.join("gezik-9.0.0-windows-x64.zip");
        fs::write(&file, zip).unwrap();
        // The entry that would land outside fails the unpack: nothing is installed from it.
        assert!(unpack(&folder, Package::Windows, &file).is_err());
        assert!(!dir.join("evil.txt").exists() && !dir.join("update").join("evil.txt").exists());
        fs::write(&file, gezik_batch::archive::write_zip_entries(&[exe]).unwrap()).unwrap();
        let new = unpack(&folder, Package::Windows, &file).unwrap();
        assert_eq!(new, folder.join("x").join("gezik-9.0.0-windows-x64").join("gezik.exe"));
        assert!(!file.exists(), "the zip goes once unpacked");
        assert_eq!(unpacked(&folder, Package::Windows), None, "not marked ready");
        fs::write(folder.join(READY), "").unwrap();
        assert_eq!(unpacked(&folder, Package::Windows), Some(new));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_first_start_after_an_update_is_told_once_and_old_folders_go() {
        let dir = scratch("tidy");
        let running = env!("CARGO_PKG_VERSION");
        fs::write(dir.join(INSTALLED), running).unwrap();
        fs::create_dir_all(dir.join(running).join("x")).unwrap();
        fs::create_dir_all(dir.join("0.0.1")).unwrap();
        fs::create_dir_all(dir.join("not-a-version")).unwrap();
        fs::create_dir_all(dir.join("99.0.0")).unwrap();
        let (updated, waiting) = tidy(&dir);
        assert_eq!(updated.as_deref(), Some(running));
        assert!(waiting.is_none(), "99.0.0 is not ready");
        assert!(!dir.join(INSTALLED).exists() && !dir.join(running).exists() && !dir.join("0.0.1").exists());
        assert!(dir.join("not-a-version").exists() && dir.join("99.0.0").exists());
        assert_eq!(tidy(&dir).0, None, "said once");
        let _ = fs::remove_dir_all(&dir);
    }
}
