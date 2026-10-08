//! The tools Gezik uses from outside (7-Zip, ffmpeg, the pdfium library): where one is (the
//! path in settings, Gezik's download, then PATH; pdfium only in Gezik's download), and where a
//! download goes (`<data>/tools/<name>-<version>/`).
//! Downloading itself is `tasks::DownloadTask`.

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use gezik_platform::ChildProcess;

use gezik_core::batch::tools::{Platform, Tool, ToolBuild, build_for};

/// Where a tool's program is: the path in settings (a full one), Gezik's download, PATH,
/// then where an installer puts it. Only a recent enough one counts (`recent_enough`); it
/// starts the program, so this runs off the UI thread. For pdfium it is the downloaded
/// library's path or nothing (it has no setting; PATH and the system are not looked in).
pub fn find(tool: Tool, data_dir: &Path, configured: Option<&Path>) -> Option<PathBuf> {
    let build = Platform::current().and_then(|platform| build_for(tool, platform));
    let installed = build.and_then(|build| program_in(build, &install_dir(build, data_dir)));
    let configured = configured.filter(|path| path.is_absolute());
    let ok = |path: &Path| recent_enough(tool, path);
    find_in(tool, configured, installed, std::env::var_os("PATH").as_deref(), &ok)
        .or_else(|| installed_elsewhere(tool).filter(|path| ok(path)))
}

/// The oldest 7-Zip Gezik hands an archive to: 25.00 fixed how links in an archive are
/// unpacked (CVE-2025-11001/11002). p7zip (16.02, unmaintained) never counts.
const SEVEN_ZIP_MIN: (u32, u32) = (25, 0);

/// Whether the program at `path` is a version Gezik trusts (for ffmpeg: any ffmpeg). Asked once per program and
/// change time. pdfium is a library, which cannot be run and asked: its version is the one
/// MANIFEST pins (chromium/7881 or newer, what the bindings need).
fn recent_enough(tool: Tool, path: &Path) -> bool {
    static SEEN: Answers<bool> = Answers::new();
    match tool {
        Tool::SevenZip => SEEN
            .get(path, |timeout| match banner(path, timeout) {
                Some(text) => Asked::Answer(seven_zip_version(&text).is_some_and(|v| v >= SEVEN_ZIP_MIN)),
                None => Asked::NoAnswer,
            })
            .unwrap_or(false),
        // Any ffmpeg does audio and video; what needs a newer one (HEIC) asks for the version.
        // (It keeps its own answers.)
        Tool::Ffmpeg => crate::convert::ffmpeg::is_ffmpeg(path),
        Tool::Pdfium => true,
    }
}

/// What the program prints when started without arguments (its name and version first);
/// `None` if it does not start or takes longer than `timeout`.
fn banner(path: &Path, timeout: Duration) -> Option<String> {
    let mut child = ChildProcess::spawn(path, std::iter::empty::<&str>(), None).ok()?;
    let lines = child.stdout_lines();
    let started = Instant::now();
    child.wait_or_stop(|| started.elapsed() > timeout).ok()??;
    Some(lines.take(10).collect::<Vec<_>>().join("\n"))
}

/// What asking a program (for its version) gave.
pub(crate) enum Asked<T> {
    /// It ran and said this (or something else, which is an answer too): kept until the
    /// program changes.
    Answer(T),
    /// It did not start, or did not end in time: asked again next time. The first start of
    /// a program just downloaded can wait long for a virus scanner (100 MB of ffmpeg); kept,
    /// "no answer" would offer the download again and again while a good one is there.
    NoAnswer,
}

/// How long a program may take to answer the first time it is asked: that start may wait for
/// a virus scan of the whole program.
pub(crate) const FIRST_ASK: Duration = Duration::from_secs(20);

/// How long it may take when it is asked again after it did not answer in time.
pub(crate) const ASK_AGAIN: Duration = Duration::from_secs(5);

/// Programs asked, by path: the change time asked for, and the answer (`None`: none came).
type Seen<T> = HashMap<PathBuf, (Option<SystemTime>, Option<T>)>;

/// The answers programs gave, once per program and change time.
pub(crate) struct Answers<T>(Mutex<Option<Seen<T>>>);

impl<T: Clone> Answers<T> {
    pub(crate) const fn new() -> Answers<T> {
        Answers(Mutex::new(None))
    }

    /// The answer of the program at `path`: kept from before, or asked now with `ask`, which
    /// is given how long it may wait ([`FIRST_ASK`], [`ASK_AGAIN`] after no answer). `None`
    /// when no answer came (not kept). The ask blocks without looking at any stop.
    pub(crate) fn get(&self, path: &Path, ask: impl FnOnce(Duration) -> Asked<T>) -> Option<T> {
        let modified = std::fs::metadata(path).and_then(|meta| meta.modified()).ok();
        let timeout = {
            let mut seen = self.0.lock().unwrap_or_else(|e| e.into_inner());
            match seen.get_or_insert_with(HashMap::new).get(path) {
                Some((when, Some(answer))) if *when == modified => return Some(answer.clone()),
                Some((when, None)) if *when == modified => ASK_AGAIN,
                _ => FIRST_ASK,
            }
        };
        let answer = match ask(timeout) {
            Asked::Answer(answer) => Some(answer),
            Asked::NoAnswer => None,
        };
        let mut seen = self.0.lock().unwrap_or_else(|e| e.into_inner());
        seen.get_or_insert_with(HashMap::new).insert(path.to_path_buf(), (modified, answer.clone()));
        answer
    }
}

/// 7-Zip's version from its banner (`7-Zip 26.03 (x64) : Copyright…`, `7-Zip (z) 24.08`);
/// `None` for p7zip (`p7zip Version 16.02`) or no 7-Zip at all.
fn seven_zip_version(banner: &str) -> Option<(u32, u32)> {
    if banner.contains("p7zip") {
        return None;
    }
    let line = banner.lines().map(str::trim).find(|line| line.starts_with("7-Zip"))?;
    line.split_whitespace().find_map(|word| {
        let (major, minor) = word.split_once('.')?;
        Some((major.parse().ok()?, minor.parse().ok()?))
    })
}

/// `<data>/tools/<name>-<version>/`.
pub fn install_dir(build: &ToolBuild, data_dir: &Path) -> PathBuf {
    data_dir.join("tools").join(format!("{}-{}", folder_name(build.tool), build.version))
}

/// The download's SHA-256 is not the one expected.
#[derive(Debug)]
pub struct Damaged;

impl fmt::Display for Damaged {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "download damaged — try again")
    }
}

impl std::error::Error for Damaged {}

pub(crate) fn damaged() -> io::Error {
    io::Error::other(Damaged)
}

/// Whether `err` says the download was damaged (trying again may help).
pub fn is_damaged(err: &io::Error) -> bool {
    err.get_ref().is_some_and(|inner| inner.is::<Damaged>())
}

/// Whether a failure's text, as the engine's report keeps it, says the download was damaged.
pub fn says_damaged(message: &str) -> bool {
    message == Damaged.to_string()
}

/// The first part of a tool's folder name (`7zip` in `7zip-25.01`).
pub(crate) fn folder_name(tool: Tool) -> &'static str {
    match tool {
        Tool::SevenZip => "7zip",
        Tool::Ffmpeg => "ffmpeg",
        Tool::Pdfium => "pdfium",
    }
}

/// How the tool is called in the UI.
pub(crate) fn display_name(tool: Tool) -> &'static str {
    match tool {
        Tool::SevenZip => "7-Zip",
        Tool::Ffmpeg => "ffmpeg",
        Tool::Pdfium => "pdfium",
    }
}

/// The tool's own program in a download unpacked at `dir` (the first of `programs`).
fn program_in(build: &ToolBuild, dir: &Path) -> Option<PathBuf> {
    let program = build.programs.first()?;
    Some(program.split('/').fold(dir.to_path_buf(), |path, part| path.join(part))).filter(|p| executable(p))
}

/// The search without the machine's own places: `configured`, `installed`, then `path_var`,
/// each only if `ok` says so.
fn find_in(
    tool: Tool,
    configured: Option<&Path>,
    installed: Option<PathBuf>,
    path_var: Option<&OsStr>,
    ok: &dyn Fn(&Path) -> bool,
) -> Option<PathBuf> {
    if let Some(path) = configured.filter(|path| executable(path) && ok(path)) {
        return Some(path.to_path_buf());
    }
    installed.filter(|path| ok(path)).or_else(|| on_path(path_names(tool), path_var?).filter(|path| ok(path)))
}

/// The tool's program names on PATH (none for pdfium: only Gezik's download is used).
fn path_names(tool: Tool) -> &'static [&'static str] {
    match tool {
        Tool::SevenZip => &["7z", "7zz", "7za"],
        Tool::Ffmpeg => &["ffmpeg"],
        Tool::Pdfium => &[],
    }
}

/// Where an installer puts the tool (Windows' 7-Zip in Program Files; ffmpeg and pdfium have
/// none).
fn installed_elsewhere(tool: Tool) -> Option<PathBuf> {
    match tool {
        Tool::SevenZip if cfg!(windows) => {
            let dir = PathBuf::from(std::env::var_os("ProgramFiles")?);
            Some(dir.join("7-Zip").join("7z.exe")).filter(|p| executable(p))
        }
        Tool::SevenZip | Tool::Ffmpeg | Tool::Pdfium => None,
    }
}

/// The first of `names` (`.exe` added on Windows) in a folder of `path_var`.
pub(crate) fn on_path(names: &[&str], path_var: &OsStr) -> Option<PathBuf> {
    std::env::split_paths(path_var)
        .filter(|dir| dir.is_absolute())
        .find_map(|dir| names.iter().map(|name| dir.join(exe(name))).find(|path| executable(path)))
}

fn exe(name: &str) -> OsString {
    let mut name = OsString::from(name);
    name.push(std::env::consts::EXE_SUFFIX);
    name
}

/// A file that may be run (on Unix: with an execute bit).
pub(crate) fn executable(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else { return false };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.is_file() && meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    meta.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("gezik-tools-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// An empty program file at `path`, executable.
    fn program(path: &Path) -> PathBuf {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        path.to_path_buf()
    }

    fn build() -> &'static ToolBuild {
        Box::leak(Box::new(ToolBuild {
            tool: Tool::SevenZip,
            platform: Platform::current().unwrap(),
            version: "25.01",
            url: "https://example.invalid/7zip.zip",
            size: 1,
            sha256: "",
            programs: &["bin/7z.exe", "bin/7z.dll"],
            kind: "zip",
        }))
    }

    #[test]
    fn install_dir_names_tool_and_version() {
        let dir = install_dir(build(), Path::new("data"));
        assert_eq!(dir, Path::new("data").join("tools").join("7zip-25.01"));
    }

    #[test]
    fn find_prefers_settings_then_download_then_path() {
        let d = dir("find");
        let configured = program(&d.join("mine").join("7z-custom.exe"));
        let build = build();
        let data = d.join("data");
        let downloaded = program(&install_dir(build, &data).join("bin").join("7z.exe"));
        let on_path_dir = d.join("path");
        let path_program = program(&on_path_dir.join(exe("7zz")));
        let path_var = std::env::join_paths([d.join("nothing-here"), on_path_dir.clone()]).unwrap();
        let installed = || program_in(build, &install_dir(build, &data));
        let find = |configured: Option<&Path>, installed| {
            find_in(Tool::SevenZip, configured, installed, Some(path_var.as_os_str()), &|_| true)
        };

        assert_eq!(find(Some(&configured), installed()), Some(configured.clone()));
        // A path in settings that is not there is passed over.
        assert_eq!(find(Some(&d.join("gone.exe")), installed()), Some(downloaded.clone()));
        assert_eq!(find(None, installed()), Some(downloaded.clone()));
        std::fs::remove_file(&downloaded).unwrap();
        assert_eq!(installed(), None);
        assert_eq!(find(None, installed()), Some(path_program.clone()));
        std::fs::remove_file(&path_program).unwrap();
        assert_eq!(find(None, installed()), None);
        // A folder of that name is not a program; a relative PATH entry is not looked in.
        std::fs::create_dir(on_path_dir.join(exe("7z"))).unwrap();
        assert_eq!(on_path(&["7z"], path_var.as_os_str()), None);
        assert_eq!(on_path(&["7z"], OsStr::new(".")), None);
        // One that is too old is passed over.
        let old = |path: &Path| path != configured;
        let found = find_in(Tool::SevenZip, Some(&configured), None, Some(path_var.as_os_str()), &old);
        assert_eq!(found, None);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// pdfium is found only in Gezik's download (`<data>/tools/pdfium-<build>/`), never on PATH.
    #[test]
    fn pdfium_is_only_the_downloaded_library() {
        let d = dir("pdfium");
        let data = d.join("data");
        let build = build_for(Tool::Pdfium, Platform::current().unwrap()).unwrap();
        let installed = install_dir(build, &data);
        assert_eq!(installed, data.join("tools").join(format!("pdfium-{}", build.version)));
        let library = program(&installed.join(build.programs[0]));
        assert_eq!(find(Tool::Pdfium, &data, None), Some(library.clone()));
        std::fs::remove_file(&library).unwrap();
        assert_eq!(find(Tool::Pdfium, &data, None), None);
        // A library of that name on PATH is not used.
        let on_path_dir = d.join("path");
        program(&on_path_dir.join(build.programs[0]));
        program(&on_path_dir.join("pdfium.dll"));
        let path_var = std::env::join_paths([on_path_dir]).unwrap();
        let ok = |path: &Path| recent_enough(Tool::Pdfium, path);
        assert_eq!(find_in(Tool::Pdfium, None, None, Some(path_var.as_os_str()), &ok), None);
        assert_eq!(installed_elsewhere(Tool::Pdfium), None);
        assert_eq!((folder_name(Tool::Pdfium), display_name(Tool::Pdfium)), ("pdfium", "pdfium"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn seven_zip_versions_are_read_from_the_banner() {
        let read = seven_zip_version;
        assert_eq!(read("\n7-Zip 26.03 (x64) : Copyright (c) 1999-2026 Igor Pavlov : 2026-03-01\n"), Some((26, 3)));
        assert_eq!(read("7-Zip (z) 24.08 (x64) : Copyright (c) 1999-2024 Igor Pavlov"), Some((24, 8)));
        assert_eq!(
            read("7-Zip [64] 16.02 : Copyright (c) 1999-2016 Igor Pavlov : 2016-05-21\np7zip Version 16.02"),
            None
        );
        assert_eq!(read("usage: something else 1.2"), None);
        assert!(read("7-Zip 25.00 (x64)").is_some_and(|v| v >= SEVEN_ZIP_MIN));
        assert!(read("7-Zip 24.09 (x64)").is_some_and(|v| v < SEVEN_ZIP_MIN));
    }

    /// The installed 7-Zip's version is read (when there is one).
    #[cfg(windows)]
    #[test]
    fn installed_seven_zip_banner_is_read() {
        let Some(path) = installed_elsewhere(Tool::SevenZip) else { return };
        let banner = banner(&path, FIRST_ASK).expect("7-Zip printed its banner");
        assert!(seven_zip_version(&banner).is_some(), "{banner}");
    }

    #[test]
    fn only_answers_are_kept_and_a_first_ask_may_take_longer() {
        let d = dir("answers");
        let path = program(&d.join("tool.exe"));
        let answers: Answers<bool> = Answers::new();
        let asked = &std::cell::RefCell::new(Vec::new());
        let ask = |reply: Option<bool>| {
            move |timeout: Duration| {
                asked.borrow_mut().push(timeout);
                reply.map_or(Asked::NoAnswer, Asked::Answer)
            }
        };
        // No answer (it timed out, or did not start): not kept, asked again (more briefly).
        assert_eq!(answers.get(&path, ask(None)), None);
        assert_eq!(answers.get(&path, ask(None)), None);
        assert_eq!(answers.get(&path, ask(Some(true))), Some(true));
        assert_eq!(*asked.borrow(), [FIRST_ASK, ASK_AGAIN, ASK_AGAIN]);
        // An answer is kept: not asked again.
        assert_eq!(answers.get(&path, ask(Some(false))), Some(true));
        assert_eq!(asked.borrow().len(), 3);
        // "Not this tool" is an answer too.
        let other = program(&d.join("other.exe"));
        assert_eq!(answers.get(&other, ask(Some(false))), Some(false));
        assert_eq!(answers.get(&other, ask(Some(true))), Some(false));
        assert_eq!(asked.borrow().len(), 4);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn damaged_is_recognized() {
        assert!(is_damaged(&damaged()));
        assert_eq!(damaged().to_string(), "download damaged — try again");
        assert!(!is_damaged(&io::Error::other("download damaged — try again")));
        assert!(says_damaged(&gezik_platform::fs::describe(&damaged())));
        assert!(!says_damaged("curl: (22) The requested URL returned error: 404"));
    }
}
