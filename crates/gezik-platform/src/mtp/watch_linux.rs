//! Linux (spec 16 §10.4): phones found in `/sys` without loading libmtp, their comings and goings
//! heard on the kernel's uevent socket by one sleeping thread, and gvfs's folder for a phone it
//! holds. The parsing is plain text, tested on every system.

use std::fs;
use std::path::Path;
#[cfg(target_os = "linux")]
use std::path::PathBuf;

/// A phone in `/sys`: its id ([`super::usb_id`]), name and serial number.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Found {
    pub id: String,
    pub name: String,
    pub serial: String,
}

/// The phones under `sys` (`/sys`): USB devices with an MTP interface, by id.
pub(super) fn scan(sys: &Path) -> Vec<Found> {
    let devices = sys.join("bus/usb/devices");
    let Ok(listing) = fs::read_dir(&devices) else { return Vec::new() };
    let names = listing.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    found_in(names, |entry, file| fs::read_to_string(devices.join(entry).join(file)).ok())
}

/// The phones among `/sys/bus/usb/devices`' entries `names`, `read` giving an entry's file.
fn found_in(names: Vec<String>, read: impl Fn(&str, &str) -> Option<String>) -> Vec<Found> {
    let read = |entry: &str, file: &str| read(entry, file).map(|s| s.trim().to_owned());
    let mut found: Vec<Found> = Vec::new();
    for interface in &names {
        // Interfaces are `1-2:1.0`, their device `1-2`.
        let Some((device, _)) = interface.split_once(':') else { continue };
        let byte = |file: &str| read(interface, file).and_then(|s| u8::from_str_radix(&s, 16).ok());
        let (Some(class), Some(subclass), Some(protocol)) =
            (byte("bInterfaceClass"), byte("bInterfaceSubClass"), byte("bInterfaceProtocol"))
        else {
            continue;
        };
        let vendor = read(device, "idVendor").and_then(|s| u16::from_str_radix(&s, 16).ok()).unwrap_or(0);
        let label = read(interface, "interface").unwrap_or_default();
        if !super::is_mtp_interface(class, subclass, protocol, &label, vendor) {
            continue;
        }
        let bus = read(device, "busnum").and_then(|s| s.parse().ok());
        let address = read(device, "devnum").and_then(|s| s.parse().ok());
        let (Some(bus), Some(address)) = (bus, address) else { continue };
        let id = super::usb_id(bus, address);
        if found.iter().any(|f| f.id == id) {
            continue;
        }
        let name = read(device, "product").or_else(|| read(device, "manufacturer")).unwrap_or_default();
        found.push(Found { id, name, serial: read(device, "serial").unwrap_or_default() });
    }
    // The folder's order is the kernel's: sorted, the same phone keeps its number in `named`.
    found.sort_by(|a, b| a.id.cmp(&b.id));
    found
}

/// Whether a kernel uevent (`add@/devices/…` then `KEY=value`, NUL-separated) may change the
/// phones: an interface of class 6 or 255 added, or a USB device removed.
pub(super) fn is_phone_change(message: &[u8]) -> bool {
    let (mut action, mut subsystem, mut devtype, mut interface) = (None, None, None, None);
    for field in message.split(|b| *b == 0) {
        let Some((key, value)) = std::str::from_utf8(field).ok().and_then(|f| f.split_once('=')) else { continue };
        match key {
            "ACTION" => action = Some(value),
            "SUBSYSTEM" => subsystem = Some(value),
            "DEVTYPE" => devtype = Some(value),
            "INTERFACE" => interface = Some(value),
            _ => {}
        }
    }
    match (action, subsystem, devtype) {
        (Some("add"), Some("usb"), Some("usb_interface")) => {
            interface.is_some_and(|i| i.starts_with("6/") || i.starts_with("255/"))
        }
        (Some("remove"), Some("usb"), Some("usb_device")) => true,
        _ => false,
    }
}

/// Whether gvfs's mount folder `name` (`mtp:host=SAMSUNG_SAMSUNG_Android_RF8M12345`, or before
/// gvfs 1.30 `mtp:host=%5Busb%3A002%2C005%5D`) is the phone at `bus`/`address` with `serial`.
pub(super) fn is_gvfs_of(name: &str, bus: u32, address: u8, serial: &str) -> bool {
    let Some(host) = name.strip_prefix("mtp:host=") else { return false };
    let host = percent_decoded(host);
    if host == format!("[usb:{bus:03},{address:03}]") {
        return true;
    }
    // udev's ID_SERIAL: vendor_model_serial, spaces and odd characters as `_`.
    let serial: String =
        serial.chars().map(|c| if c.is_ascii_alphanumeric() || "#+-.:=@_".contains(c) { c } else { '_' }).collect();
    !serial.is_empty() && (host == serial || host.ends_with(&format!("_{serial}")))
}

/// `%5B` and the like as the bytes they stand for.
fn percent_decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok());
        match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
            Some(byte) if bytes[i] == b'%' => {
                out.push(byte);
                i += 3;
            }
            _ => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// gvfs's folder for phone `id`, if it holds the phone.
#[cfg(target_os = "linux")]
pub(super) fn gvfs_mount(id: &str) -> Option<PathBuf> {
    let (bus, address) = super::parse_usb_id(id)?;
    let phone = scan(Path::new("/sys")).into_iter().find(|f| f.id == id)?;
    // SAFETY: getuid cannot fail.
    let gvfs = PathBuf::from(format!("/run/user/{}/gvfs", unsafe { libc::getuid() }));
    let listing = fs::read_dir(&gvfs).ok()?;
    listing
        .flatten()
        .find(|e| is_gvfs_of(&e.file_name().to_string_lossy(), bus, address, &phone.serial))
        .map(|e| gvfs.join(e.file_name()))
}

/// One thread with a small stack sleeps on the uevent socket and calls `on_change` for each
/// event that may change the phones (and when events were lost).
#[cfg(target_os = "linux")]
pub(super) fn watch(on_change: impl Fn() + Send + 'static) -> bool {
    // SAFETY: a socket and its address of the sizes the kernel expects; closed on failure.
    let fd = unsafe {
        let fd = libc::socket(libc::AF_NETLINK, libc::SOCK_DGRAM | libc::SOCK_CLOEXEC, libc::NETLINK_KOBJECT_UEVENT);
        if fd < 0 {
            return false;
        }
        let mut address: libc::sockaddr_nl = std::mem::zeroed();
        address.nl_family = libc::AF_NETLINK as libc::sa_family_t;
        // The kernel's own group (udev's re-sent events are group 2).
        address.nl_groups = 1;
        let size = std::mem::size_of::<libc::sockaddr_nl>() as libc::socklen_t;
        if libc::bind(fd, (&raw const address).cast(), size) != 0 {
            libc::close(fd);
            return false;
        }
        fd
    };
    let thread = std::thread::Builder::new().name("gezik-phones".into()).stack_size(64 * 1024).spawn(move || {
        let mut buffer = [0u8; 8192];
        loop {
            // SAFETY: the buffer outlives the call and is as long as said.
            let read = unsafe { libc::recv(fd, buffer.as_mut_ptr().cast(), buffer.len(), 0) };
            match usize::try_from(read) {
                Ok(read) if is_phone_change(&buffer[..read]) => on_change(),
                Ok(_) => {}
                Err(_) => match std::io::Error::last_os_error().raw_os_error() {
                    Some(libc::EINTR) => {}
                    // Events were lost: read the list again.
                    Some(libc::ENOBUFS) => on_change(),
                    _ => break,
                },
            }
        }
    });
    thread.is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `/sys` with a phone (6/1/1), an older phone (255 named MTP), an iPhone and a stick.
    #[test]
    fn phones_are_found_in_sys() {
        let mut files = std::collections::HashMap::new();
        let mut file = |entry: &str, name: &str, text: &str| {
            files.insert((entry.to_owned(), name.to_owned()), format!("{text}
"));
        };
        let mut device = |name: &str, vendor: &str, bus: &str, address: &str, product: &str, serial: &str| {
            for (key, text) in [("idVendor", vendor), ("busnum", bus), ("devnum", address), ("serial", serial)] {
                file(name, key, text);
            }
            if !product.is_empty() {
                file(name, "product", product);
            }
        };
        device("1-2", "18d1", "1", "7", "Pixel 8", "3A1B");
        device("3-1", "04e8", "3", "12", "", "R58M");
        device("1-4", "05ac", "1", "9", "iPhone", "00008");
        device("2-1", "0781", "2", "3", "Cruzer", "");
        let mut interface = |name: &str, class: &str, label: &str| {
            file(name, "bInterfaceClass", class);
            file(name, "bInterfaceSubClass", if class == "06" { "01" } else { "ff" });
            file(name, "bInterfaceProtocol", if class == "06" { "01" } else { "00" });
            if !label.is_empty() {
                file(name, "interface", label);
            }
        };
        interface("1-2:1.0", "06", "MTP");
        interface("1-2:1.1", "ff", "ADB Interface");
        interface("3-1:1.0", "ff", "MTP");
        interface("1-4:1.0", "06", "");
        interface("2-1:1.0", "08", "");
        file("3-1", "manufacturer", "SAMSUNG");
        let names = ["usb1", "1-2", "1-2:1.0", "1-2:1.1", "3-1:1.0", "3-1", "1-4", "1-4:1.0", "2-1", "2-1:1.0"];
        let found = found_in(names.map(String::from).to_vec(), |entry, name| {
            files.get(&(entry.to_owned(), name.to_owned())).cloned()
        });
        let pixel = Found { id: "usb:1:7".into(), name: "Pixel 8".into(), serial: "3A1B".into() };
        let samsung = Found { id: "usb:3:12".into(), name: "SAMSUNG".into(), serial: "R58M".into() };
        assert_eq!(found, [pixel, samsung]);
        assert!(scan(Path::new("/nothing-here")).is_empty());
    }

    #[test]
    fn uevents_that_may_change_the_phones() {
        let event = |fields: &[&str]| fields.join("\0").into_bytes();
        let add = |devtype: &str, interface: &str| {
            event(&[
                "add@/devices/pci0000:00/0000:00:14.0/usb1/1-2/1-2:1.0",
                "ACTION=add",
                "DEVPATH=/devices/pci0000:00/0000:00:14.0/usb1/1-2/1-2:1.0",
                "SUBSYSTEM=usb",
                &format!("DEVTYPE={devtype}"),
                &format!("INTERFACE={interface}"),
                "SEQNUM=4242",
            ])
        };
        assert!(is_phone_change(&add("usb_interface", "6/1/1")));
        assert!(is_phone_change(&add("usb_interface", "255/255/0")));
        assert!(!is_phone_change(&add("usb_interface", "8/6/80")), "a USB stick");
        assert!(!is_phone_change(&add("usb_device", "6/1/1")), "the device before its interfaces");
        let removed = event(&["remove@/devices/x/usb1/1-2", "ACTION=remove", "SUBSYSTEM=usb", "DEVTYPE=usb_device"]);
        assert!(is_phone_change(&removed));
        let block = event(&["remove@/devices/x/block/sdb", "ACTION=remove", "SUBSYSTEM=block", "DEVTYPE=disk"]);
        assert!(!is_phone_change(&block));
        assert!(!is_phone_change(b"libudev\0\xfe\xed"));
        assert!(!is_phone_change(b""));
    }

    #[test]
    fn gvfs_mount_names() {
        assert!(is_gvfs_of("mtp:host=SAMSUNG_SAMSUNG_Android_R58M12ABC", 3, 12, "R58M12ABC"));
        assert!(is_gvfs_of("mtp:host=Google_Pixel_8_3A1B", 1, 7, "3A1B"));
        assert!(is_gvfs_of("mtp:host=%5Busb%3A001%2C007%5D", 1, 7, ""));
        assert!(is_gvfs_of("mtp:host=Acme_Phone_SN_1_2", 1, 7, "SN 1/2"));
        assert!(!is_gvfs_of("mtp:host=%5Busb%3A001%2C008%5D", 1, 7, ""));
        assert!(!is_gvfs_of("mtp:host=Google_Pixel_8_3A1B", 1, 7, "A1B"), "only a whole serial");
        assert!(!is_gvfs_of("mtp:host=Google_Pixel_8_3A1B", 1, 7, ""));
        assert!(!is_gvfs_of("gphoto2:host=Canon_3A1B", 1, 7, "3A1B"));
        assert_eq!(percent_decoded("a%2Cb%zz%4"), "a,b%zz%4");
    }
}
