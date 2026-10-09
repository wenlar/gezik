//! Ejecting a drive and disconnecting a network one (spec 9 §7.4). Gezik lets go of the drive
//! before it calls this (the app's `eject.rs`). Blocking: background threads only.

use std::path::Path;

use crate::process::Ran;
use crate::{Drive, DriveKind};

pub const IN_USE: &str = "The drive is in use. Close the files on it and try again.";
pub const CANNOT: &str = "This drive cannot be ejected";
pub const NEEDS_TOOLS: &str = "Ejecting needs gio or udisksctl";
const MAX_MESSAGE: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EjectWay {
    Eject,
    Disconnect,
}

impl EjectWay {
    pub fn title(self) -> &'static str {
        match self {
            EjectWay::Eject => "Eject",
            EjectWay::Disconnect => "Disconnect",
        }
    }
}

/// What Gezik offers for a drive: never the system disk; a network drive is disconnected;
/// removable media and optical drives are ejected; a fixed one on Windows only when it is
/// `external` (USB, FireWire, card: the Shell has no `eject` for an internal disk); macOS
/// ejects every volume; Linux removable media only (`/mnt` and `/` stay).
pub fn way_for(kind: &DriveKind, system: bool, external: bool, windows: bool, mac: bool) -> Option<EjectWay> {
    if system {
        return None;
    }
    match kind {
        DriveKind::Network => Some(EjectWay::Disconnect),
        DriveKind::Removable | DriveKind::Optical => Some(EjectWay::Eject),
        DriveKind::Fixed if mac || (windows && external) => Some(EjectWay::Eject),
        DriveKind::Fixed => None,
    }
}

/// `way_for` for this system's `drive` (Windows asks a fixed drive's bus: one small query).
pub fn offer(drive: &Drive) -> Option<EjectWay> {
    #[cfg(windows)]
    let external = drive.kind == DriveKind::Fixed && win::external(&drive.path);
    #[cfg(not(windows))]
    let external = false;
    way_for(&drive.kind, is_system(drive), external, cfg!(windows), cfg!(target_os = "macos"))
}

/// Whether a disk on `bus` (`STORAGE_BUS_TYPE`) can be unplugged: USB, 1394, SD, MMC, or
/// one the system calls removable media.
#[cfg_attr(not(windows), allow(dead_code))]
fn external_bus(bus: i32, removable_media: bool) -> bool {
    removable_media || matches!(bus, 4 | 7 | 12 | 13)
}

/// Whether an ejected Windows drive has gone: its letter left, or (a card reader keeps its
/// letter) its volume answers ERROR_NOT_READY (no media).
#[cfg_attr(not(windows), allow(dead_code))]
fn ejected(letter_present: bool, volume_error: Option<u32>) -> bool {
    const ERROR_NOT_READY: u32 = 21;
    !letter_present || volume_error == Some(ERROR_NOT_READY)
}

/// Whether `drive` is the one the system runs from (`%SystemDrive%\`, `/`).
pub fn is_system(drive: &Drive) -> bool {
    #[cfg(windows)]
    {
        let system = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_owned());
        gezik_core::ops::paths::same_path(&drive.path, Path::new(&format!("{system}\\")))
    }
    #[cfg(not(windows))]
    {
        drive.path == Path::new("/")
    }
}

/// Ejects or disconnects `drive`; `owner` owns any window the system shows. Blocking.
pub fn eject(drive: &Drive, owner: isize) -> Result<(), String> {
    #[cfg(windows)]
    {
        win::eject(drive, owner)
    }
    #[cfg(target_os = "macos")]
    {
        let _ = owner;
        mac::eject(drive)
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = owner;
        let mounts = std::fs::read_to_string("/proc/self/mounts").unwrap_or_default();
        linux_eject(drive, &mounts, &mut crate::process::run_with_input)
    }
}

pub fn windows_error_text(code: u32) -> String {
    match code {
        // ERROR_OPEN_FILES, ERROR_DEVICE_IN_USE
        2401 | 2404 => IN_USE.to_owned(),
        2250 => "This network drive is not connected".to_owned(),
        _ => std::io::Error::from_raw_os_error(code as i32).to_string(),
    }
}

/// EBUSY (POSIX domain) or fBsyErr (Carbon): something has a file open there.
pub fn mac_error_text(code: isize, description: &str) -> String {
    if code == 16 || code == -47 || description.to_lowercase().contains("in use") {
        IN_USE.to_owned()
    } else {
        description.chars().take(MAX_MESSAGE).collect()
    }
}

pub fn linux_error_text(stderr: &str) -> String {
    if stderr.to_lowercase().contains("busy") {
        return IN_USE.to_owned();
    }
    let line: String =
        stderr.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").chars().take(MAX_MESSAGE).collect();
    if line.is_empty() { "Could not eject the drive".to_owned() } else { line }
}

/// The block device mounted at `mount_point` (`/proc/self/mounts` text).
pub fn device_of(mounts: &str, mount_point: &Path) -> Option<String> {
    mounts.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        let device = fields.next()?;
        let point = crate::drives::unescape_mount(fields.next()?);
        (Path::new(&point) == mount_point && device.starts_with("/dev/")).then(|| device.to_owned())
    })
}

/// The whole disk of a partition (`/dev/sdb1` → `/dev/sdb`, `/dev/nvme0n1p2` → `/dev/nvme0n1`).
pub fn whole_disk(device: &str) -> Option<String> {
    let name = device.strip_prefix("/dev/").filter(|n| !n.is_empty())?;
    let numbered = name.starts_with("nvme") || name.starts_with("mmcblk");
    let trimmed = name.trim_end_matches(|c: char| c.is_ascii_digit());
    let disk = if numbered {
        // Their partitions end in `p<n>`; the disk's own name ends in a digit.
        match trimmed.strip_suffix('p') {
            Some(disk) if trimmed.len() < name.len() => disk,
            _ => name,
        }
    } else {
        trimmed
    };
    (!disk.is_empty()).then(|| format!("/dev/{disk}"))
}

/// Linux: `gio mount -e` (removable) or `-u` (a gvfs network mount); without gio,
/// `udisksctl unmount` and then `power-off` of the whole disk (a nicety it may refuse).
pub fn linux_eject(
    drive: &Drive,
    mounts: &str,
    run: &mut dyn FnMut(&[&str], &str) -> std::io::Result<Ran>,
) -> Result<(), String> {
    let path = drive.path.to_string_lossy().into_owned();
    let network = drive.kind == DriveKind::Network;
    let gio = ["gio", "mount", if network { "-u" } else { "-e" }, path.as_str()];
    match run(&gio, "") {
        Ok(ran) if ran.ok => return Ok(()),
        Ok(ran) => return Err(linux_error_text(&ran.stderr)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(err.to_string()),
    }
    if network {
        return Err(crate::network::NEEDS_GIO.to_owned());
    }
    let device = device_of(mounts, &drive.path).ok_or_else(|| CANNOT.to_owned())?;
    let unmount = match run(&["udisksctl", "unmount", "-b", device.as_str()], "") {
        Ok(ran) => ran,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Err(NEEDS_TOOLS.to_owned()),
        Err(err) => return Err(err.to_string()),
    };
    if !unmount.ok {
        return Err(linux_error_text(&unmount.stderr));
    }
    // Unmounted is enough to pull it out; powering off is a nicety the system may refuse.
    if let Some(disk) = whole_disk(&device) {
        let _ = run(&["udisksctl", "power-off", "-b", disk.as_str()], "");
    }
    Ok(())
}

#[cfg(windows)]
mod win {
    use super::{CANNOT, IN_USE, ejected, external_bus, windows_error_text};
    use crate::{Drive, DriveKind};
    use std::ffi::c_void;
    use std::path::Path;
    use windows::Win32::Foundation::{CloseHandle, HWND, NO_ERROR, WIN32_ERROR};
    use windows::Win32::NetworkManagement::WNet::{CONNECT_UPDATE_PROFILE, WNetCancelConnection2W};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, GetVolumeInformationW, OPEN_EXISTING,
    };
    use windows::Win32::System::IO::DeviceIoControl;
    use windows::Win32::System::Ioctl::{
        IOCTL_STORAGE_QUERY_PROPERTY, PropertyStandardQuery, STORAGE_DEVICE_DESCRIPTOR, STORAGE_PROPERTY_QUERY,
        StorageDeviceProperty,
    };
    use windows::core::HSTRING;

    /// Whether the disk under `root` (`E:\`) sits on a bus it can be unplugged from.
    pub fn external(root: &Path) -> bool {
        let text = root.to_string_lossy();
        let Some(letter) = text.chars().next().filter(char::is_ascii_alphabetic) else { return false };
        // No access rights needed: the query only reads properties.
        let opened = unsafe {
            CreateFileW(
                &HSTRING::from(format!(r"\\.\{letter}:")),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                None,
            )
        };
        let Ok(handle) = opened else { return false };
        let query = STORAGE_PROPERTY_QUERY {
            PropertyId: StorageDeviceProperty,
            QueryType: PropertyStandardQuery,
            AdditionalParameters: [0],
        };
        // The descriptor is followed by its strings; room for them keeps the call from failing.
        let mut answer = [0u64; 128];
        let mut returned = 0u32;
        let asked = unsafe {
            DeviceIoControl(
                handle,
                IOCTL_STORAGE_QUERY_PROPERTY,
                Some(&query as *const STORAGE_PROPERTY_QUERY as *const c_void),
                size_of::<STORAGE_PROPERTY_QUERY>() as u32,
                Some(answer.as_mut_ptr().cast()),
                size_of_val(&answer) as u32,
                Some(&mut returned),
                None,
            )
        };
        let _ = unsafe { CloseHandle(handle) };
        if asked.is_err() || (returned as usize) < size_of::<STORAGE_DEVICE_DESCRIPTOR>() {
            return false;
        }
        // SAFETY: the buffer is 8-byte aligned and holds a whole descriptor (checked above).
        let descriptor = unsafe { &*answer.as_ptr().cast::<STORAGE_DEVICE_DESCRIPTOR>() };
        external_bus(descriptor.BusType.0, descriptor.RemovableMedia)
    }

    pub fn eject(drive: &Drive, owner: isize) -> Result<(), String> {
        let text = drive.path.to_string_lossy();
        let letter = text.chars().next().filter(char::is_ascii_alphabetic).ok_or_else(|| CANNOT.to_owned())?;
        if drive.kind == DriveKind::Network {
            // The mapping goes from the profile too: it does not come back at sign-in.
            let name = HSTRING::from(format!("{letter}:"));
            let code = unsafe { WNetCancelConnection2W(&name, CONNECT_UPDATE_PROFILE, false) };
            return if code == NO_ERROR { Ok(()) } else { Err(windows_error_text(code.0)) };
        }
        crate::init_thread();
        crate::shell_menu::invoke_verb(HWND(owner as *mut _), &drive.path, c"eject").map_err(|_| CANNOT.to_owned())?;
        if drive.kind == DriveKind::Optical {
            return Ok(());
        }
        // The Shell answers before the device has gone: its letter or its media going says so.
        let bit = 1u32 << (letter.to_ascii_uppercase() as u8 - b'A');
        let root = HSTRING::from(format!("{letter}:\\"));
        for _ in 0..30 {
            let present = crate::drive_signature() as u32 & bit != 0;
            let error = present
                .then(|| unsafe { GetVolumeInformationW(&root, None, None, None, None, None) }.err())
                .flatten()
                .and_then(|err| WIN32_ERROR::from_error(&err))
                .map(|code| code.0);
            if ejected(present, error) {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        Err(IN_USE.to_owned())
    }
}

/// The canonical names of the Shell's verbs in `root`'s menu (the probe: nothing is run).
#[cfg(windows)]
pub fn shell_verbs(root: &Path) -> Vec<String> {
    crate::init_thread();
    crate::shell_menu::verb_names(root).unwrap_or_default()
}

#[cfg(target_os = "macos")]
mod mac {
    use super::mac_error_text;
    use crate::Drive;
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::{NSString, NSURL};

    pub fn eject(drive: &Drive) -> Result<(), String> {
        let url = NSURL::fileURLWithPath(&NSString::from_str(&drive.path.to_string_lossy()));
        NSWorkspace::sharedWorkspace()
            .unmountAndEjectDeviceAtURL_error(&url)
            .map_err(|err| mac_error_text(err.code(), &err.localizedDescription().to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::path::PathBuf;

    fn drive(path: &str, kind: DriveKind) -> Drive {
        Drive { path: PathBuf::from(path), label: path.to_owned(), kind }
    }

    #[test]
    fn what_each_drive_offers() {
        use DriveKind::*;
        // (kind, system disk, external, windows, mac)
        assert_eq!(way_for(&Removable, false, false, true, false), Some(EjectWay::Eject));
        assert_eq!(way_for(&Fixed, false, true, true, false), Some(EjectWay::Eject), "Windows: a USB disk");
        assert_eq!(way_for(&Fixed, false, false, true, false), None, "Windows: an internal disk has no eject");
        assert_eq!(way_for(&Fixed, true, true, true, false), None, "never the system disk");
        assert_eq!(way_for(&Network, false, false, true, false), Some(EjectWay::Disconnect));
        assert_eq!(way_for(&Optical, false, false, true, false), Some(EjectWay::Eject));
        assert_eq!(way_for(&Fixed, false, false, false, true), Some(EjectWay::Eject), "macOS: every volume but /");
        assert_eq!(way_for(&Fixed, true, false, false, true), None);
        assert_eq!(way_for(&Removable, false, false, false, false), Some(EjectWay::Eject));
        assert_eq!(way_for(&Fixed, false, false, false, false), None, "Linux: /mnt stays");
        assert_eq!(way_for(&Network, false, false, false, false), Some(EjectWay::Disconnect), "gvfs");
        assert_eq!((EjectWay::Eject.title(), EjectWay::Disconnect.title()), ("Eject", "Disconnect"));
    }

    #[test]
    fn external_buses_and_gone_drives() {
        // USB, 1394, SD, MMC; SATA (11), NVMe (17), SAS (10) stay inside.
        for bus in [7, 4, 12, 13] {
            assert!(external_bus(bus, false), "{bus}");
        }
        for bus in [11, 17, 10, 0] {
            assert!(!external_bus(bus, false), "{bus}");
        }
        assert!(external_bus(11, true), "removable media");
        assert!(ejected(false, None), "the letter went");
        assert!(ejected(true, Some(21)), "a card reader keeps its letter, the media went");
        assert!(!ejected(true, None), "still there");
        assert!(!ejected(true, Some(5)), "another error is not gone");
    }

    #[test]
    fn the_system_disk_is_known() {
        let root = if cfg!(windows) { r"C:\" } else { "/" };
        if cfg!(windows) && std::env::var("SystemDrive").is_ok_and(|d| !d.eq_ignore_ascii_case("C:")) {
            return;
        }
        assert!(is_system(&drive(root, DriveKind::Fixed)));
        assert!(!is_system(&drive(if cfg!(windows) { r"Q:\" } else { "/mnt/q" }, DriveKind::Fixed)));
    }

    #[test]
    fn devices_and_their_disks() {
        let mounts = "/dev/sda2 / ext4 rw 0 0\n/dev/sdb1 /media/u/My\\040Stick vfat rw 0 0\nbad\n";
        assert_eq!(device_of(mounts, Path::new("/media/u/My Stick")).as_deref(), Some("/dev/sdb1"));
        assert_eq!(device_of(mounts, Path::new("/media/u/Other")), None);
        assert_eq!(
            device_of("gvfsd-fuse /run/user/1000/gvfs fuse rw 0 0\n", Path::new("/run/user/1000/gvfs")),
            None,
            "not a device"
        );
        for (device, disk) in [
            ("/dev/sdb1", Some("/dev/sdb")),
            ("/dev/sdb", Some("/dev/sdb")),
            ("/dev/nvme0n1p2", Some("/dev/nvme0n1")),
            ("/dev/nvme0n1", Some("/dev/nvme0n1")),
            ("/dev/mmcblk0p1", Some("/dev/mmcblk0")),
            ("/dev/", None),
            ("sdb1", None),
        ] {
            assert_eq!(whole_disk(device).as_deref(), disk, "{device}");
        }
    }

    fn fake<'a>(
        log: &'a RefCell<Vec<String>>,
        answer: impl Fn(&[&str]) -> std::io::Result<Ran> + 'a,
    ) -> impl FnMut(&[&str], &str) -> std::io::Result<Ran> + 'a {
        move |args, _input| {
            log.borrow_mut().push(args.join(" "));
            answer(args)
        }
    }

    #[test]
    fn linux_tries_gio_then_udisksctl() {
        let mounts = "/dev/sdb1 /media/u/My\\040Stick vfat rw 0 0\n";
        let stick = drive("/media/u/My Stick", DriveKind::Removable);
        let log = RefCell::new(Vec::new());
        let ok = Ran { ok: true, ..Ran::default() };
        let mut no_gio =
            fake(&log, |args| if args[0] == "gio" { Err(std::io::ErrorKind::NotFound.into()) } else { Ok(ok.clone()) });
        assert_eq!(linux_eject(&stick, mounts, &mut no_gio), Ok(()));
        assert_eq!(
            *log.borrow(),
            ["gio mount -e /media/u/My Stick", "udisksctl unmount -b /dev/sdb1", "udisksctl power-off -b /dev/sdb"]
        );
        log.borrow_mut().clear();
        let mut gio = fake(&log, |_| Ok(Ran { ok: true, ..Ran::default() }));
        assert_eq!(linux_eject(&stick, mounts, &mut gio), Ok(()));
        assert_eq!(*log.borrow(), ["gio mount -e /media/u/My Stick"], "gio alone is enough");
        let mut nothing = fake(&log, |_| Err(std::io::ErrorKind::NotFound.into()));
        assert_eq!(linux_eject(&stick, mounts, &mut nothing), Err(NEEDS_TOOLS.to_owned()));
        let mut busy = fake(&log, |args| {
            if args[0] == "gio" {
                Err(std::io::ErrorKind::NotFound.into())
            } else {
                Ok(Ran { ok: false, stdout: String::new(), stderr: "Error unmounting: target is busy\n".into() })
            }
        });
        assert_eq!(linux_eject(&stick, mounts, &mut busy), Err(IN_USE.to_owned()), "udisksctl's busy too");
    }

    #[test]
    fn a_network_mount_is_unmounted_with_gio() {
        let share = drive("/run/user/1000/gvfs/smb-share:server=nas,share=foto", DriveKind::Network);
        let log = RefCell::new(Vec::new());
        let mut gio = fake(&log, |_| Ok(Ran { ok: true, ..Ran::default() }));
        assert_eq!(linux_eject(&share, "", &mut gio), Ok(()));
        assert_eq!(*log.borrow(), ["gio mount -u /run/user/1000/gvfs/smb-share:server=nas,share=foto"]);
        let mut none = fake(&log, |_| Err(std::io::ErrorKind::NotFound.into()));
        assert_eq!(linux_eject(&share, "", &mut none), Err(crate::network::NEEDS_GIO.to_owned()));
    }

    #[test]
    fn a_busy_drive_says_so() {
        let stick = drive("/media/u/USB", DriveKind::Removable);
        let log = RefCell::new(Vec::new());
        let mut busy = fake(&log, |_| {
            Ok(Ran { ok: false, stdout: String::new(), stderr: "gio: /media/u/USB: target is busy\n".into() })
        });
        assert_eq!(linux_eject(&stick, "", &mut busy), Err(IN_USE.to_owned()));
        let mut other =
            fake(&log, |_| Ok(Ran { ok: false, stdout: String::new(), stderr: "\n  gio: Not allowed\nmore\n".into() }));
        assert_eq!(linux_eject(&stick, "", &mut other), Err("gio: Not allowed".to_owned()));
    }

    #[test]
    fn windows_and_mac_error_texts() {
        assert_eq!(windows_error_text(2401), IN_USE, "open files");
        assert_eq!(windows_error_text(2404), IN_USE, "device in use");
        assert_eq!(windows_error_text(2250), "This network drive is not connected");
        assert!(!windows_error_text(5).is_empty());
        assert_eq!(mac_error_text(16, "x"), IN_USE, "EBUSY");
        assert_eq!(mac_error_text(-47, "x"), IN_USE, "fBsyErr");
        assert_eq!(mac_error_text(1, "The volume is in use by Mail"), IN_USE);
        assert_eq!(mac_error_text(1, "Nope"), "Nope");
        assert_eq!(linux_error_text(""), "Could not eject the drive");
        assert_eq!(linux_error_text(&"x".repeat(500)).len(), MAX_MESSAGE, "cut short");
    }
}
