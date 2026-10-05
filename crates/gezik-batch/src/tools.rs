//! The tools Gezik uses from outside (7-Zip): where one is (the path in settings, Gezik's
//! download, then PATH), and where a download goes (`<data>/tools/<name>-<version>/`).
//! Downloading itself is `tasks::DownloadTask`.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use gezik_core::batch::tools::{Platform, Tool, ToolBuild, build_for};

/// Where a tool's program is: the path in settings, Gezik's download, then PATH.
pub fn find(tool: Tool, data_dir: &Path, configured: Option<&Path>) -> Option<PathBuf> {
    let build = Platform::current().and_then(|platform| build_for(tool, platform));
    let installed = build.and_then(|build| program_in(build, &install_dir(build, data_dir)));
    find_in(tool, configured, installed, std::env::var_os("PATH").as_deref()).or_else(|| installed_elsewhere(tool))
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
    }
}

/// How the tool is called in the UI.
pub(crate) fn display_name(tool: Tool) -> &'static str {
    match tool {
        Tool::SevenZip => "7-Zip",
    }
}

/// The tool's own program in a download unpacked at `dir` (the first of `programs`).
fn program_in(build: &ToolBuild, dir: &Path) -> Option<PathBuf> {
    let program = build.programs.first()?;
    Some(program.split('/').fold(dir.to_path_buf(), |path, part| path.join(part))).filter(|p| executable(p))
}

/// The search without the machine's own places: `configured`, `installed`, then `path_var`.
fn find_in(
    tool: Tool,
    configured: Option<&Path>,
    installed: Option<PathBuf>,
    path_var: Option<&OsStr>,
) -> Option<PathBuf> {
    if let Some(path) = configured.filter(|path| executable(path)) {
        return Some(path.to_path_buf());
    }
    installed.or_else(|| on_path(path_names(tool), path_var?))
}

/// The tool's program names on PATH.
fn path_names(tool: Tool) -> &'static [&'static str] {
    match tool {
        Tool::SevenZip => &["7z", "7zz", "7za"],
    }
}

/// Where an installer puts the tool (Windows' 7-Zip in Program Files).
fn installed_elsewhere(tool: Tool) -> Option<PathBuf> {
    match tool {
        Tool::SevenZip if cfg!(windows) => {
            let dir = PathBuf::from(std::env::var_os("ProgramFiles")?);
            Some(dir.join("7-Zip").join("7z.exe")).filter(|p| executable(p))
        }
        Tool::SevenZip => None,
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
fn executable(path: &Path) -> bool {
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
            find_in(Tool::SevenZip, configured, installed, Some(path_var.as_os_str()))
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
