//! Phones and media players (MTP, spec 16 §10): finding them without loading anything, and a
//! session on one. Windows talks to them through WPD (`windows.rs`); macOS and Linux come with
//! 16g. `gezik-vfs` turns a session into a place.

use std::io;
use std::time::SystemTime;

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

/// The phones plugged in now, without loading WPD: the device interfaces' list and names
/// (Windows: cfgmgr32). A device with a drive letter is left out (it shows as a drive); two of
/// one name are told apart as `Pixel 8 (2)`. Empty where phones are not supported yet.
pub fn devices() -> Vec<Device> {
    // A drive's interface is left out before anything of it is read (not even its name).
    #[cfg(windows)]
    let found = windows::interfaces()
        .into_iter()
        .filter(|id| !has_drive_letter(id))
        .map(|id| {
            let name = windows::name_of(&id);
            (id, name)
        })
        .collect();
    #[cfg(not(windows))]
    let found = Vec::new();
    named(found)
}

/// Opens the device `id` (loads WPD on Windows). Runs off the UI thread.
pub fn open(id: &str) -> io::Result<Box<dyn Session>> {
    #[cfg(windows)]
    return windows::open(id);
    #[cfg(not(windows))]
    {
        let _ = id;
        Err(io::ErrorKind::Unsupported.into())
    }
}

/// Whether the device `id` is still plugged in (no WPD call).
pub fn present(id: &str) -> bool {
    #[cfg(windows)]
    return windows::interfaces().iter().any(|found| found.eq_ignore_ascii_case(id));
    #[cfg(not(windows))]
    {
        let _ = id;
        false
    }
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
