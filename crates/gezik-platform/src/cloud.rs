//! Cloud drives (spec 9 §7.2): the folders cloud apps keep in step with their servers, found
//! from what is at hand (the registry, the environment, known folders, the mount table) once
//! per places load, on a background thread. Nothing is watched or polled.

use std::path::{Component, Path, PathBuf};

/// A cloud folder the sidebar shows under CLOUD.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudRoot {
    pub path: PathBuf,
    /// The provider's name ("OneDrive", "iCloud Drive"); "OneDrive (Personal)" when two share it.
    pub label: String,
    /// The account, if the source names one ("Personal", "a@b.com"); else empty.
    pub account: String,
}

/// This user's cloud folders that exist, each once, sorted by label. Blocking: call it off
/// the UI thread.
pub fn roots() -> Vec<CloudRoot> {
    #[cfg(windows)]
    let found = win::roots();
    #[cfg(target_os = "macos")]
    let found = mac::roots();
    #[cfg(not(any(windows, target_os = "macos")))]
    let found = linux::roots();
    tidy(found.into_iter().filter(|root| root.path.is_dir()).collect())
}

/// The root `path` is in (or is), comparing whole parts; case does not count on Windows and
/// macOS.
pub fn root_of<'a>(roots: &'a [CloudRoot], path: &Path) -> Option<&'a CloudRoot> {
    roots.iter().find(|root| starts_with(path, &root.path))
}

/// Whether macOS folder `dir` is inside iCloud Drive or a File Provider folder, where an item
/// can be dataless (its data only in the cloud). A plain text test: no call of its own.
/// shortcut: only the usual places count (`~/Library/Mobile Documents`, `~/Library/CloudStorage`);
/// a File Provider folder elsewhere is not detected. Upgrade by asking `roots` once per walk.
pub fn in_mac_cloud_folder(dir: &Path) -> bool {
    let dir = dir.to_string_lossy();
    ["/Library/Mobile Documents/", "/Library/CloudStorage/"].iter().any(|part| dir.contains(part))
}

fn starts_with(path: &Path, root: &Path) -> bool {
    let mut parts = path.components();
    root.components().all(|r| parts.next().is_some_and(|p| same_part(p, r)))
}

fn same_part(a: Component<'_>, b: Component<'_>) -> bool {
    let (a, b) = (a.as_os_str().to_string_lossy(), b.as_os_str().to_string_lossy());
    if cfg!(any(windows, target_os = "macos")) {
        a.chars().flat_map(char::to_lowercase).eq(b.chars().flat_map(char::to_lowercase))
    } else {
        a == b
    }
}

/// Each path once (the first source wins), sorted by label; a label two roots share gets
/// each one's account.
fn tidy(found: Vec<CloudRoot>) -> Vec<CloudRoot> {
    let mut out: Vec<CloudRoot> = Vec::new();
    for root in found {
        let taken = out.iter().any(|r| starts_with(&root.path, &r.path) && starts_with(&r.path, &root.path));
        if !taken {
            out.push(root);
        }
    }
    let shared: Vec<String> =
        out.iter().filter(|r| out.iter().filter(|o| o.label == r.label).count() > 1).map(|r| r.label.clone()).collect();
    for root in &mut out {
        if shared.contains(&root.label) && !root.account.is_empty() {
            root.label = format!("{} ({})", root.label, root.account);
        }
    }
    out.sort_by_key(|root| root.label.to_lowercase());
    out
}

/// The account part of a SyncRootManager key name (`Provider!SID!Account`); `None` if the
/// name is not one.
#[cfg_attr(not(windows), allow(dead_code))]
fn sync_root_account(key: &str) -> Option<String> {
    let mut parts = key.splitn(3, '!');
    let (provider, _sid, account) = (parts.next()?, parts.next()?, parts.next()?);
    (!provider.is_empty()).then(|| account.to_owned())
}

/// Provider and account of a `~/Library/CloudStorage` folder name (`OneDrive-Personal`).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn cloud_storage_name(name: &str) -> Option<(String, String)> {
    if name.starts_with('.') {
        return None;
    }
    let (provider, account) = name.split_once('-').unwrap_or((name, ""));
    if provider.is_empty() {
        return None;
    }
    let provider = match provider {
        "GoogleDrive" => "Google Drive",
        other => other,
    };
    Some((provider.to_owned(), account.to_owned()))
}

/// The mount points of `fuse.rclone` file systems in `/proc/mounts` text.
#[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))]
fn rclone_mounts(mounts: &str) -> Vec<PathBuf> {
    mounts
        .lines()
        .filter(|line| line.len() <= 4096)
        .filter_map(|line| {
            let mut fields = line.split(' ');
            let (_source, point, kind) = (fields.next()?, fields.next()?, fields.next()?);
            (kind == "fuse.rclone").then(|| PathBuf::from(unescape_mount(point)))
        })
        .collect()
}

/// `/proc/mounts` writes space, tab, newline and backslash as `\ooo` (octal).
#[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))]
fn unescape_mount(field: &str) -> String {
    let mut out = String::with_capacity(field.len());
    let mut rest = field;
    while let Some(at) = rest.find('\\') {
        out.push_str(&rest[..at]);
        let code = rest.get(at + 1..at + 4).and_then(|d| u8::from_str_radix(d, 8).ok());
        match code {
            Some(byte) => {
                out.push(char::from(byte));
                rest = &rest[at + 4..];
            }
            None => {
                out.push('\\');
                rest = &rest[at + 1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// The account of a gvfs Google Drive mount (`google-drive:host=gmail.com,user=ali` →
/// `ali@gmail.com`); `None` for other mounts.
#[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))]
fn gvfs_google(name: &str) -> Option<String> {
    let fields = name.strip_prefix("google-drive:")?;
    let value = |key: &str| fields.split(',').find_map(|f| f.strip_prefix(key)).unwrap_or_default();
    let (user, host) = (value("user="), value("host="));
    Some(if user.is_empty() { String::new() } else { format!("{user}@{host}") })
}

#[cfg(windows)]
mod win {
    use std::path::PathBuf;

    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{
        HKEY, HKEY_LOCAL_MACHINE, KEY_READ, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ, RegCloseKey, RegEnumKeyExW,
        RegGetValueW, RegOpenKeyExW,
    };
    use windows::core::{HSTRING, PWSTR};

    use super::{CloudRoot, sync_root_account};

    const SYNC_ROOTS: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\SyncRootManager";

    pub(super) fn roots() -> Vec<CloudRoot> {
        let mut out = sync_roots();
        for var in ["OneDrive", "OneDriveCommercial", "OneDriveConsumer"] {
            if let Some(path) = std::env::var_os(var).filter(|v| !v.is_empty()).map(PathBuf::from) {
                let label =
                    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| var.to_owned());
                out.push(CloudRoot { path, label, account: String::new() });
            }
        }
        out
    }

    /// Read only: the registry of the cloud filter's sync roots (one key per provider and
    /// account, each user's root under `UserSyncRoots`).
    fn sync_roots() -> Vec<CloudRoot> {
        let Some(sid) = crate::instance::user_sid() else { return Vec::new() };
        let mut key = HKEY::default();
        // SAFETY: `key` is written by the call and closed below.
        if unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, &HSTRING::from(SYNC_ROOTS), None, KEY_READ, &mut key) }
            != ERROR_SUCCESS
        {
            return Vec::new();
        }
        let mut out = Vec::new();
        for index in 0..256u32 {
            let mut name = [0u16; 256];
            let mut len = name.len() as u32;
            // SAFETY: `name` holds `len` units.
            let next =
                unsafe { RegEnumKeyExW(key, index, Some(PWSTR(name.as_mut_ptr())), &mut len, None, None, None, None) };
            if next != ERROR_SUCCESS {
                break;
            }
            let Ok(name) = String::from_utf16(&name[..(len as usize).min(name.len())]) else { continue };
            let Some(account) = sync_root_account(&name) else { continue };
            let Some(path) = string(key, &format!(r"{name}\UserSyncRoots"), &sid) else { continue };
            let label = string(key, &name, "DisplayNameResource")
                .map(|text| indirect(&text))
                .filter(|text| !text.is_empty())
                .unwrap_or_else(|| {
                    PathBuf::from(&path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
                });
            out.push(CloudRoot { path: PathBuf::from(path), label, account });
        }
        // SAFETY: opened above.
        unsafe {
            let _ = RegCloseKey(key);
        }
        out
    }

    /// A text value of `key\sub`; `None` if missing, not text, or longer than 2,048 units.
    // shortcut: a longer value is skipped; read its size first if a provider ever needs it.
    fn string(key: HKEY, sub: &str, value: &str) -> Option<String> {
        let mut buf = [0u16; 2048];
        let mut size = (buf.len() * 2) as u32;
        // SAFETY: `buf` holds `size` bytes; names end with NUL (HSTRING).
        let code = unsafe {
            RegGetValueW(
                key,
                &HSTRING::from(sub),
                &HSTRING::from(value),
                RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ,
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
        String::from_utf16(&units[..end]).ok()
    }

    /// `@C:\…\x.dll,-101` read through `SHLoadIndirectString`; plain text as it is.
    fn indirect(text: &str) -> String {
        if !text.starts_with('@') {
            return text.to_owned();
        }
        let mut out = [0u16; 512];
        // SAFETY: the source ends with NUL; `out` is the buffer the call fills.
        match unsafe { windows::Win32::UI::Shell::SHLoadIndirectString(&HSTRING::from(text), &mut out, None) } {
            Ok(()) => {
                let end = out.iter().position(|&c| c == 0).unwrap_or(out.len());
                String::from_utf16_lossy(&out[..end])
            }
            Err(_) => String::new(),
        }
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use super::{CloudRoot, cloud_storage_name};

    pub(super) fn roots() -> Vec<CloudRoot> {
        let Some(home) = dirs::home_dir() else { return Vec::new() };
        let mut out = Vec::new();
        let icloud = home.join("Library/Mobile Documents/com~apple~CloudDocs");
        out.push(CloudRoot { path: icloud, label: "iCloud Drive".into(), account: String::new() });
        if let Ok(read) = std::fs::read_dir(home.join("Library/CloudStorage")) {
            for entry in read.flatten().take(256) {
                let Ok(name) = entry.file_name().into_string() else { continue };
                if let Some((label, account)) = cloud_storage_name(&name) {
                    out.push(CloudRoot { path: entry.path(), label, account });
                }
            }
        }
        out
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod linux {
    use std::path::PathBuf;

    use super::{CloudRoot, gvfs_google, rclone_mounts};

    pub(super) fn roots() -> Vec<CloudRoot> {
        let mut out = Vec::new();
        if let Some(home) = dirs::home_dir() {
            for (name, label) in [("Dropbox", "Dropbox"), ("OneDrive", "OneDrive"), ("Google Drive", "Google Drive")] {
                out.push(CloudRoot { path: home.join(name), label: label.into(), account: String::new() });
            }
        }
        // Small (a few KB); read whole, as the kernel gives it.
        if let Ok(mounts) = std::fs::read_to_string("/proc/mounts") {
            for path in rclone_mounts(&mounts) {
                let label =
                    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "rclone".into());
                out.push(CloudRoot { path, label, account: String::new() });
            }
        }
        if let Some(gvfs) = std::env::var_os("XDG_RUNTIME_DIR").map(|d| PathBuf::from(d).join("gvfs"))
            && let Ok(read) = std::fs::read_dir(gvfs)
        {
            for entry in read.flatten().take(256) {
                let Ok(name) = entry.file_name().into_string() else { continue };
                if let Some(account) = gvfs_google(&name) {
                    out.push(CloudRoot { path: entry.path(), label: "Google Drive".into(), account });
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(path: &str, label: &str) -> CloudRoot {
        CloudRoot { path: PathBuf::from(path), label: label.into(), account: String::new() }
    }

    #[test]
    fn sync_root_key_names_split() {
        assert_eq!(sync_root_account("OneDrive!S-1-5-21-1-2-3-1001!Personal"), Some("Personal".to_owned()));
        assert_eq!(sync_root_account("Dropbox!S-1-5-21-1!dbid:AAB"), Some("dbid:AAB".to_owned()));
        assert_eq!(sync_root_account("OneDrive!S-1-5-21-1!"), Some(String::new()));
        assert_eq!(sync_root_account("NoBangs"), None, "not a sync root key");
        assert_eq!(sync_root_account(""), None);
    }

    #[test]
    fn cloud_storage_names_split() {
        let split = |name: &str| cloud_storage_name(name);
        assert_eq!(split("OneDrive-Personal"), Some(("OneDrive".into(), "Personal".into())));
        assert_eq!(split("OneDrive-Kişisel"), Some(("OneDrive".into(), "Kişisel".into())));
        assert_eq!(split("GoogleDrive-a@b.com"), Some(("Google Drive".into(), "a@b.com".into())));
        assert_eq!(split("Dropbox"), Some(("Dropbox".into(), String::new())));
        assert_eq!(split("Box-Box"), Some(("Box".into(), "Box".into())));
        assert_eq!(split("Acme-x-y"), Some(("Acme".into(), "x-y".into())), "only the first dash splits");
        assert_eq!(split(".DS_Store"), None);
        assert_eq!(split(""), None);
        assert_eq!(split("-x"), None, "no provider");
    }

    #[test]
    fn mount_lines_unescape() {
        let mounts = "sysfs /sys sysfs rw 0 0\n\
                      gdrive: /home/u/My\\040Drive fuse.rclone rw,nosuid 0 0\n\
                      bad line\n\
                      remote: /mnt/r fuse.rclone rw 0 0\n";
        assert_eq!(rclone_mounts(mounts), [PathBuf::from("/home/u/My Drive"), PathBuf::from("/mnt/r")]);
        assert_eq!(unescape_mount("a\\011b\\134c\\012"), "a\tb\\c\n");
        assert_eq!(unescape_mount("trailing\\04"), "trailing\\04", "a short escape stays as it is");
    }

    #[test]
    fn gvfs_names_give_the_account() {
        assert_eq!(gvfs_google("google-drive:host=gmail.com,user=ali"), Some("ali@gmail.com".to_owned()));
        assert_eq!(gvfs_google("google-drive:user=ali,host=gmail.com"), Some("ali@gmail.com".to_owned()));
        assert_eq!(gvfs_google("google-drive:host=gmail.com"), Some(String::new()));
        assert_eq!(gvfs_google("smb-share:server=x,share=y"), None);
    }

    #[test]
    fn the_same_root_is_listed_once() {
        let first = CloudRoot { account: "Personal".into(), ..root("C:\\Users\\u\\OneDrive", "OneDrive") };
        let again = root(if cfg!(windows) { "c:\\users\\U\\onedrive" } else { "C:\\Users\\u\\OneDrive" }, "x");
        let other = root("D:\\Dropbox", "Dropbox");
        let out = tidy(vec![first.clone(), again, other.clone()]);
        assert_eq!(out, [other, first], "the first of a path wins; sorted by label");
    }

    #[test]
    fn same_labels_get_their_account() {
        let a = CloudRoot { account: "Personal".into(), ..root("/a", "OneDrive") };
        let b = CloudRoot { account: "Contoso".into(), ..root("/b", "OneDrive") };
        let c = root("/c", "Dropbox");
        let labels: Vec<String> = tidy(vec![a, b, c]).into_iter().map(|r| r.label).collect();
        assert_eq!(labels, ["Dropbox", "OneDrive (Contoso)", "OneDrive (Personal)"]);
    }

    #[test]
    fn mac_cloud_folders_are_found_by_their_place() {
        assert!(in_mac_cloud_folder(Path::new("/Users/u/Library/Mobile Documents/com~apple~CloudDocs")));
        assert!(in_mac_cloud_folder(Path::new("/Users/u/Library/CloudStorage/OneDrive-Personal/a")));
        assert!(!in_mac_cloud_folder(Path::new("/Users/u/Library/CloudStorage")), "the list of drives itself");
        assert!(!in_mac_cloud_folder(Path::new("/Users/u/Documents")));
        assert!(!in_mac_cloud_folder(Path::new("/Users/u/Library/CloudStorageX/a")));
    }

    #[test]
    fn root_of_matches_whole_parts() {
        let sep = std::path::MAIN_SEPARATOR;
        let base =
            if cfg!(windows) { "C:\\Users\\Ömer\\OneDrive".to_owned() } else { "/home/ömer/OneDrive".to_owned() };
        let roots = [root(&base, "OneDrive")];
        assert!(root_of(&roots, Path::new(&base)).is_some(), "the root itself");
        assert!(root_of(&roots, Path::new(&format!("{base}{sep}a{sep}b.txt"))).is_some());
        assert!(root_of(&roots, Path::new(&format!("{base}2{sep}a"))).is_none(), "OneDrive2 is another folder");
        let shouted = base.to_uppercase();
        assert_eq!(
            root_of(&roots, Path::new(&format!("{shouted}{sep}x"))).is_some(),
            cfg!(any(windows, target_os = "macos")),
            "case counts only on Linux"
        );
        assert!(root_of(&[], Path::new(&base)).is_none());
    }
}
