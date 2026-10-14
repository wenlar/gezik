//! Phones and media players (MTP, spec 16 §10): finding them without loading anything, and a
//! session on one. Windows talks to them through WPD (`windows.rs`); macOS and Linux through
//! libmtp, loaded when a phone is opened (`libmtp.rs`), and find them in the IORegistry
//! (`watch_mac.rs`) or `/sys` (`watch_linux.rs`). `gezik-vfs` turns a session into a place.

use std::io;
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};
use std::time::SystemTime;

#[cfg(any(target_os = "linux", target_os = "macos", test))]
mod libmtp;
#[cfg(any(target_os = "linux", test))]
mod watch_linux;
#[cfg(target_os = "macos")]
mod watch_mac;
#[cfg(windows)]
mod windows;

/// A phone plugged in: its lasting id (Windows: the WPD interface path) and the name shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub id: String,
    pub name: String,
}

/// A storage, folder or file on a device. `id` is the device's own (names may repeat).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Object {
    pub id: String,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
}

/// An open device. Every call may fail with the device gone (unplugged); none waits for ever.
/// `more` is told each piece's bytes and returns false to stop (the call then fails with
/// `Interrupted` and leaves nothing half written on the device).
pub trait Session: Send + Sync {
    /// Its storages ("Internal shared storage", "SD card") as folders. Empty while the phone is
    /// locked or not in "File transfer".
    fn storages(&self) -> io::Result<Vec<Object>>;
    /// What folder (or storage) `parent` holds.
    fn list(&self, parent: &str) -> io::Result<Vec<Object>>;
    /// Writes file `id` to `out`.
    fn read_to(&self, id: &str, out: &mut dyn io::Write, more: &mut dyn FnMut(u64) -> bool) -> io::Result<()>;
    /// A new file `name` of `size` bytes from `from` in folder `parent`.
    fn put(
        &self,
        parent: &str,
        name: &str,
        size: u64,
        from: &mut dyn io::Read,
        more: &mut dyn FnMut(u64) -> bool,
    ) -> io::Result<Object>;
    /// A new folder `name` in `parent`.
    fn mkdir(&self, parent: &str, name: &str) -> io::Result<Object>;
    /// Deletes `id`, a folder with what it holds.
    fn delete(&self, id: &str) -> io::Result<()>;
    fn rename(&self, id: &str, name: &str) -> io::Result<()>;
    /// Whether `id` can be renamed (some devices refuse it for files).
    fn can_rename(&self, _id: &str) -> bool {
        true
    }
    /// Whether anything on it can be changed (an iPhone's photos cannot).
    fn writable(&self) -> bool {
        true
    }
    /// Whether it is still plugged in.
    fn connected(&self) -> bool {
        true
    }
}

/// The phones plugged in now, without loading WPD or libmtp: the device interfaces' list and
/// names (Windows: cfgmgr32; macOS: the IORegistry; Linux: `/sys`). A device with a drive
/// letter is left out (it shows as a drive); two of one name are told apart as `Pixel 8 (2)`.
pub fn devices() -> Vec<Device> {
    named(found())
}

/// `(id, name)` of each phone plugged in.
fn found() -> Vec<(String, String)> {
    // A drive's interface is left out before anything of it is read (not even its name).
    #[cfg(windows)]
    return windows::interfaces()
        .into_iter()
        .filter(|id| !has_drive_letter(id))
        .map(|id| {
            let name = windows::name_of(&id);
            (id, name)
        })
        .collect();
    #[cfg(target_os = "macos")]
    return watch_mac::interfaces();
    #[cfg(target_os = "linux")]
    return watch_linux::scan(std::path::Path::new("/sys")).into_iter().map(|f| (f.id, f.name)).collect();
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    Vec::new()
}

/// Opens the device `id` (loads WPD on Windows, libmtp elsewhere). Runs off the UI thread.
pub fn open(id: &str) -> io::Result<Box<dyn Session>> {
    #[cfg(windows)]
    return windows::open(id);
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    return libmtp::open(id);
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        let _ = id;
        Err(io::ErrorKind::Unsupported.into())
    }
}

/// Whether the device `id` is still plugged in (no WPD or libmtp call).
pub fn present(id: &str) -> bool {
    #[cfg(windows)]
    return windows::interfaces().iter().any(|found| found.eq_ignore_ascii_case(id));
    #[cfg(not(windows))]
    found().iter().any(|(found, _)| found == id)
}

/// Calls `on_change` (on a thread of its own on Linux, on the main thread on macOS) whenever a
/// USB device comes or goes, for as long as Gezik runs: IOKit's notification on the main run
/// loop, or one thread sleeping on the kernel's uevent socket. False where there is none.
#[cfg(not(windows))]
pub fn watch(on_change: impl Fn() + Send + 'static) -> bool {
    #[cfg(target_os = "macos")]
    return watch_mac::watch(on_change);
    #[cfg(target_os = "linux")]
    return watch_linux::watch(on_change);
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = on_change;
        false
    }
}

/// Linux: gvfs's folder for phone `id` when gvfs holds it (`/run/user/<uid>/gvfs/mtp:host=…`);
/// libmtp cannot open the phone then, and the folder works as any other. `None` elsewhere.
pub fn gvfs_mount(id: &str) -> Option<PathBuf> {
    #[cfg(target_os = "linux")]
    return watch_linux::gvfs_mount(id);
    #[cfg(not(target_os = "linux"))]
    {
        let _ = id;
        None
    }
}

/// Where libmtp may be besides the system's: the path in settings (`[devices] libmtp`) and
/// Gezik's download (macOS). Looked at when a phone is opened and libmtp is not loaded yet.
pub fn set_library(configured: Option<PathBuf>, downloaded: Option<PathBuf>) {
    *LIBRARY.lock().unwrap_or_else(PoisonError::into_inner) = (configured, downloaded);
}

static LIBRARY: Mutex<(Option<PathBuf>, Option<PathBuf>)> = Mutex::new((None, None));

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn library_paths() -> (Option<PathBuf>, Option<PathBuf>) {
    LIBRARY.lock().unwrap_or_else(PoisonError::into_inner).clone()
}

/// libmtp is not there (macOS: neither Homebrew's nor the download; Linux: no system package).
#[derive(Debug)]
pub struct NoLibmtp;

impl std::fmt::Display for NoLibmtp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if cfg!(target_os = "macos") {
            f.write_str(&gezik_i18n::t!("Phones need libmtp"))
        } else {
            f.write_str(&gezik_i18n::t!("Install libmtp (e.g. sudo apt install libmtp9)"))
        }
    }
}

impl std::error::Error for NoLibmtp {}

/// Whether opening a phone failed for want of libmtp.
pub fn needs_libmtp(err: &io::Error) -> bool {
    err.get_ref().is_some_and(|inner| inner.is::<NoLibmtp>())
}

/// A phone's id on macOS and Linux: its USB bus and address, as libmtp knows them.
#[cfg(any(target_os = "linux", target_os = "macos", test))]
fn usb_id(bus: u32, address: u8) -> String {
    format!("usb:{bus}:{address}")
}

/// The bus and address in an id of [`usb_id`].
#[cfg(any(target_os = "linux", target_os = "macos", test))]
fn parse_usb_id(id: &str) -> Option<(u32, u8)> {
    let (bus, address) = id.strip_prefix("usb:")?.split_once(':')?;
    Some((bus.parse().ok()?, address.parse().ok()?))
}

/// Whether a USB interface is a phone's MTP: still image class 6/1/1 (today's Android), or a
/// vendor class one named "MTP" (older Android). Apple's are not (an iPhone shows only on
/// Windows, spec 16 §17).
#[cfg(any(target_os = "linux", target_os = "macos", test))]
fn is_mtp_interface(class: u8, subclass: u8, protocol: u8, name: &str, vendor: u16) -> bool {
    const APPLE: u16 = 0x05ac;
    vendor != APPLE && ((class, subclass, protocol) == (6, 1, 1) || (class == 255 && name.trim() == "MTP"))
}

/// Whether the WPD interface `id` stands for a disk with a drive letter (a USB stick or card
/// reader that Windows also lists as a portable device): those come through the WPD bus
/// enumerator, phones through USB.
pub fn has_drive_letter(id: &str) -> bool {
    id.to_ascii_lowercase().contains("wpdbusenum")
}

/// `(id, name)` pairs as devices, a repeated name numbered (`Pixel 8 (2)`).
fn named(found: Vec<(String, String)>) -> Vec<Device> {
    let mut devices: Vec<Device> = Vec::with_capacity(found.len());
    for (id, name) in found {
        let name = if name.trim().is_empty() { gezik_i18n::t!("Phone").into_owned() } else { name.trim().to_owned() };
        let taken = |n: &str| devices.iter().any(|d| d.name.eq_ignore_ascii_case(n));
        let name = if taken(&name) { gezik_core::ops::names::next_free(&name, true, taken) } else { name };
        devices.push(Device { id, name });
    }
    devices
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drives_are_left_out_and_repeated_names_numbered() {
        let stick =
            r"\\?\swd#wpdbusenum#_??_usbstor#disk&ven_vendorc&prod_productcode#{6ac27878-a6fa-4155-ba85-f98f491d4f33}";
        let phone = r"\\?\usb#vid_18d1&pid_4ee1#serial#{6ac27878-a6fa-4155-ba85-f98f491d4f33}";
        assert!(has_drive_letter(stick) && !has_drive_letter(phone));
        let found =
            named(vec![("a".into(), "Pixel 8".into()), ("b".into(), "pixel 8".into()), ("c".into(), " ".into())]);
        let names: Vec<&str> = found.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["Pixel 8", "pixel 8 (2)", "Phone"]);
    }

    #[test]
    fn usb_ids_and_mtp_interfaces() {
        assert_eq!(parse_usb_id(&usb_id(20, 7)), Some((20, 7)));
        assert_eq!(parse_usb_id("usb:1:300"), None);
        assert_eq!(parse_usb_id("usb#vid_18d1"), None);
        assert!(is_mtp_interface(6, 1, 1, "", 0x18d1));
        assert!(is_mtp_interface(255, 255, 0, "MTP", 0x04e8));
        assert!(!is_mtp_interface(255, 255, 0, "ADB Interface", 0x18d1));
        assert!(!is_mtp_interface(6, 1, 1, "PTP", 0x05ac), "an iPhone");
        assert!(!is_mtp_interface(8, 6, 80, "", 0x0781), "a USB stick");
        let missing = io::Error::new(io::ErrorKind::NotFound, NoLibmtp);
        assert!(needs_libmtp(&missing) && !needs_libmtp(&io::ErrorKind::NotFound.into()));
    }

    #[test]
    fn finding_phones_never_fails_and_is_quick() {
        // This PC may have none, or a USB stick listed as one (left out): either way a list.
        // What `[devices] phones` (on by default) costs once after start: this one read.
        let started = std::time::Instant::now();
        let devices = devices();
        let took = started.elapsed();
        println!("phones found in {took:?}");
        assert!(devices.iter().all(|d| !has_drive_letter(&d.id) && !d.name.is_empty()));
        assert!(took < std::time::Duration::from_millis(500), "{took:?}");
    }
}
