//! What Gezik writes to the system on an explicit command (spec 8.2, 11): the registry under
//! HKCU (Windows), links and folders (macOS, Linux), and the broadcast after PATH changes.
//! Plain writes only: the journal is gezik-config's `system_journal`, the undo rule
//! gezik-core's `system_change`, the places Gezik may write the app's `system_changes`.

use std::io;
use std::path::{Path, PathBuf};

#[cfg(target_os = "macos")]
pub mod macos;
pub mod text;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
pub mod windows;

#[cfg(unix)]
pub use unix::replace_symlink;

/// The user's folders the system changes are made in.
#[derive(Debug, Clone, Default)]
pub struct Places {
    pub home: Option<PathBuf>,
    /// Windows: `%LOCALAPPDATA%`.
    pub local_app_data: Option<PathBuf>,
    /// Linux: `$XDG_DATA_HOME`, `$XDG_CONFIG_HOME` by dirs' rules.
    pub data_home: Option<PathBuf>,
    pub config_home: Option<PathBuf>,
    /// Linux: `$XDG_CURRENT_DESKTOP`'s parts, lower case.
    pub desktops: Vec<String>,
    /// The user's Downloads folder (the risky place note, 9b4 decision 14).
    pub downloads: Option<PathBuf>,
}

pub fn places() -> Places {
    let linux = cfg!(all(unix, not(target_os = "macos")));
    Places {
        home: dirs::home_dir(),
        local_app_data: if cfg!(windows) { dirs::data_local_dir() } else { None },
        data_home: if linux { dirs::data_dir() } else { None },
        config_home: if linux { dirs::config_dir() } else { None },
        desktops: std::env::var("XDG_CURRENT_DESKTOP")
            .map(|d| d.split(':').filter(|p| !p.is_empty()).map(str::to_lowercase).collect())
            .unwrap_or_default(),
        downloads: dirs::download_dir(),
    }
}

/// Tells Explorer that verbs changed (Windows, `SHChangeNotify(SHCNE_ASSOCCHANGED)`).
pub fn associations_changed() {
    #[cfg(windows)]
    windows::associations_changed();
}

/// Whether `path` is on a removable drive (Windows); elsewhere by place (spec 6.3).
pub fn is_removable(path: &Path) -> bool {
    #[cfg(windows)]
    return windows::is_removable(path);
    #[cfg(not(windows))]
    path.to_str().is_some_and(|p| ["/Volumes/", "/media/", "/run/media/"].iter().any(|r| p.starts_with(r)))
}

/// macOS: tells LaunchServices about an app bundle (`LSRegisterURL`); elsewhere nothing.
pub fn register_app(app: &Path) {
    #[cfg(target_os = "macos")]
    macos::register_app(app);
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

/// This computer's name (`COMPUTERNAME` on Windows, the host name elsewhere), if known.
pub fn machine_name() -> Option<String> {
    #[cfg(windows)]
    {
        std::env::var("COMPUTERNAME").ok().filter(|n| !n.is_empty())
    }
    #[cfg(unix)]
    {
        let mut buf = [0u8; 256];
        // SAFETY: the buffer is writable for its whole length; the name is NUL-ended within it.
        if unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) } != 0 {
            return None;
        }
        let end = buf.iter().position(|b| *b == 0).unwrap_or(buf.len());
        // "mac.local" and "mac" are one machine.
        let name = String::from_utf8_lossy(&buf[..end]);
        name.split('.').next().filter(|n| !n.is_empty()).map(str::to_owned)
    }
}

/// A stable id of this computer, read only: Windows' `MachineGuid`, macOS' `IOPlatformUUID`,
/// Linux' machine-id. It stays when the computer is renamed or gets another network name.
pub fn machine_id() -> Option<String> {
    #[cfg(windows)]
    {
        machine_guid()
    }
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("/usr/sbin/ioreg")
            .args(["-rd1", "-c", "IOPlatformExpertDevice"])
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .ok()?;
        platform_uuid(&String::from_utf8_lossy(&out.stdout))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        ["/etc/machine-id", "/var/lib/dbus/machine-id"]
            .iter()
            .find_map(|path| std::fs::read_to_string(path).ok().map(|s| s.trim().to_owned()).filter(|s| !s.is_empty()))
    }
}

/// `HKLM\SOFTWARE\Microsoft\Cryptography` `MachineGuid` (the 64-bit view), read only. Not in
/// `windows.rs`, which names HKCU alone.
#[cfg(windows)]
fn machine_guid() -> Option<String> {
    use ::windows::Win32::Foundation::ERROR_SUCCESS;
    use ::windows::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RRF_SUBKEY_WOW6464KEY, RegGetValueW};
    use ::windows::core::w;
    let mut buf = [0u16; 128];
    let mut size = (buf.len() * 2) as u32;
    // SAFETY: `buf` holds `size` bytes; the names are NUL-ended literals.
    let code = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            w!(r"SOFTWARE\Microsoft\Cryptography"),
            w!("MachineGuid"),
            RRF_RT_REG_SZ | RRF_SUBKEY_WOW6464KEY,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if code != ERROR_SUCCESS {
        return None;
    }
    let units = &buf[..(size as usize / 2).min(buf.len())];
    let end = units.iter().position(|&c| c == 0).unwrap_or(units.len());
    String::from_utf16(&units[..end]).ok().filter(|id| !id.trim().is_empty())
}

/// `"IOPlatformUUID" = "…"` in `ioreg`'s output.
#[cfg(any(target_os = "macos", test))]
fn platform_uuid(text: &str) -> Option<String> {
    let line = text.lines().find(|line| line.contains("\"IOPlatformUUID\""))?;
    let value = line.split_once('=')?.1.trim().trim_matches('"').trim();
    (!value.is_empty()).then(|| value.to_owned())
}

/// This Gezik's exe; on macOS and Linux the real file (a link Gezik makes points to it).
pub fn exe() -> io::Result<PathBuf> {
    let exe = std::env::current_exe()?;
    if cfg!(unix) { exe.canonicalize() } else { Ok(exe) }
}

/// One folder, its parent already there; 0755 on macOS and Linux (spec 8.2).
pub fn make_folder(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new().mode(0o755).create(path)
    }
    #[cfg(not(unix))]
    std::fs::create_dir(path)
}

/// Whether `dir` is in this process's `PATH` (spec 8.2: if not, the line to add is shown).
pub fn in_path_env(dir: &Path) -> bool {
    let Some(path) = std::env::var_os("PATH") else { return false };
    let real = dir.canonicalize().ok();
    std::env::split_paths(&path).any(|d| d == dir || (real.is_some() && d.canonicalize().ok() == real))
}

/// Whether a link may be put at a place holding `found` (`None` nothing, `Some(None)` not a
/// link, `Some(Some(t))` a link to `t`): only an empty place, or Gezik's own link to `expect`.
#[cfg(any(unix, test))]
fn may_replace(found: Option<Option<&Path>>, expect: Option<&Path>) -> bool {
    match found {
        None => true,
        Some(target) => target.is_some() && target == expect,
    }
}

/// Tells running programs that the user's environment changed (Windows; a new terminal
/// then sees the new PATH). Hung windows are skipped after 2 s. Elsewhere nothing.
pub fn environment_changed() {
    #[cfg(windows)]
    windows::environment_changed();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_platform_uuid_is_read_from_ioreg() {
        let out = "+-o J314sAP  <class IOPlatformExpertDevice>\n    {\n      \"IOPlatformSerialNumber\" = \"X\"\n      \"IOPlatformUUID\" = \"1A2B-3C\"\n    }\n";
        assert_eq!(platform_uuid(out).as_deref(), Some("1A2B-3C"));
        assert_eq!(platform_uuid("nothing"), None);
    }

    #[test]
    #[cfg(windows)]
    fn this_machine_has_an_id() {
        assert!(machine_id().is_some_and(|id| !id.is_empty()));
    }

    #[test]
    fn this_machine_has_a_name() {
        let name = machine_name().expect("a name");
        assert!(!name.is_empty() && !name.contains('.'), "{name}");
    }

    #[test]
    fn only_an_empty_place_or_our_own_link_is_replaced() {
        let ours = Path::new("/old/gezik");
        assert!(may_replace(None, None));
        assert!(may_replace(None, Some(ours)));
        assert!(may_replace(Some(Some(ours)), Some(ours)));
        assert!(!may_replace(Some(Some(ours)), None), "a link we did not expect");
        assert!(!may_replace(Some(Some(Path::new("/other/gezik"))), Some(ours)), "someone else's link");
        assert!(!may_replace(Some(None), Some(ours)), "a file or folder of the user's");
        assert!(!may_replace(Some(None), None));
    }

    #[test]
    fn a_folder_in_path_is_found() {
        let exe = exe().unwrap();
        assert!(exe.is_absolute());
        let dir = std::env::temp_dir();
        // Only what the process's PATH says; the temp folder is not normally in it.
        assert_eq!(
            in_path_env(&dir),
            std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).any(|d| d == dir)
        );
    }
}
