use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriveKind {
    Fixed,
    Removable,
    Network,
    Optical,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drive {
    /// Root of the drive or mount point (`C:\`, `/Volumes/Data`, `/media/usb`).
    pub path: PathBuf,
    /// Display label (`Local Disk (C:)`), in the OS language where available.
    pub label: String,
    pub kind: DriveKind,
}

/// Mount points worth showing, from `/proc/self/mounts` text (octal escapes decoded).
#[cfg_attr(windows, allow(dead_code))]
pub(crate) fn parse_mounts(text: &str) -> Vec<PathBuf> {
    text.lines()
        .filter_map(|line| line.split_whitespace().nth(1))
        .map(unescape_mount)
        .filter(|mount| {
            mount == "/"
                || mount.starts_with("/media/")
                || mount.starts_with("/run/media/")
                || mount.starts_with("/mnt/")
        })
        .map(PathBuf::from)
        .collect()
}

#[cfg_attr(windows, allow(dead_code))]
fn unescape_mount(field: &str) -> String {
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let octal = (bytes[i] == b'\\' && i + 3 < bytes.len())
            .then(|| &bytes[i + 1..i + 4])
            .filter(|d| d.iter().all(|b| (b'0'..=b'7').contains(b)))
            .map(|d| u32::from(d[0] - b'0') * 64 + u32::from(d[1] - b'0') * 8 + u32::from(d[2] - b'0'))
            .and_then(|v| u8::try_from(v).ok());
        if let Some(value) = octal {
            out.push(value);
            i += 4;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(windows)]
pub fn drives() -> Vec<Drive> {
    use windows::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives};
    use windows::core::HSTRING;
    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_REMOTE: u32 = 4;
    const DRIVE_CDROM: u32 = 5;

    let mask = unsafe { GetLogicalDrives() };
    (0..26u8)
        .filter(|i| mask & (1 << i) != 0)
        .map(|i| {
            let root = format!("{}:\\", (b'A' + i) as char);
            let kind = match unsafe { GetDriveTypeW(&HSTRING::from(root.as_str())) } {
                DRIVE_REMOVABLE => DriveKind::Removable,
                DRIVE_REMOTE => DriveKind::Network,
                DRIVE_CDROM => DriveKind::Optical,
                _ => DriveKind::Fixed,
            };
            let letter = (b'A' + i) as char;
            let label = match kind {
                // The Shell name of a disconnected network drive (or an empty optical one) can
                // block for tens of seconds; the volume label alone does not ask the Shell.
                DriveKind::Network | DriveKind::Optical => labeled(&kind, letter, volume_label(&root).as_deref()),
                DriveKind::Fixed | DriveKind::Removable => crate::known::display_name(std::path::Path::new(&root))
                    .unwrap_or_else(|| root.trim_end_matches('\\').to_owned()),
            };
            Drive { path: PathBuf::from(root), label, kind }
        })
        .collect()
}

/// The volume label of the drive at `root` (`Z:\`), if it can be read.
#[cfg(windows)]
fn volume_label(root: &str) -> Option<String> {
    use windows::Win32::Storage::FileSystem::GetVolumeInformationW;
    use windows::core::HSTRING;
    let mut name = [0u16; 261];
    unsafe { GetVolumeInformationW(&HSTRING::from(root), Some(&mut name), None, None, None, None) }.ok()?;
    let end = name.iter().position(|&c| c == 0).unwrap_or(name.len());
    Some(String::from_utf16_lossy(&name[..end]))
}

/// `Label (Z:)`, or `Network (Z:)` / `CD Drive (E:)` when there is no usable volume label.
#[cfg_attr(not(windows), allow(dead_code))]
fn labeled(kind: &DriveKind, letter: char, volume_label: Option<&str>) -> String {
    let name = match volume_label.map(str::trim).filter(|l| !l.is_empty()) {
        Some(label) => label,
        None => match kind {
            DriveKind::Network => "Network",
            DriveKind::Optical => "CD Drive",
            DriveKind::Removable => "Removable Disk",
            DriveKind::Fixed => "Local Disk",
        },
    };
    format!("{name} ({letter}:)")
}

#[cfg(windows)]
pub fn drive_signature() -> u64 {
    u64::from(unsafe { windows::Win32::Storage::FileSystem::GetLogicalDrives() })
}

#[cfg(target_os = "macos")]
pub fn drives() -> Vec<Drive> {
    let Ok(entries) = std::fs::read_dir("/Volumes") else { return Vec::new() };
    let mut drives: Vec<Drive> = entries
        .filter_map(Result::ok)
        .map(|e| Drive {
            label: e.file_name().to_string_lossy().into_owned(),
            // The startup disk is a link to `/` there: its files are under `/`, not under
            // `/Volumes/Macintosh HD`, and the drive rule must find them on it.
            path: std::fs::canonicalize(e.path()).unwrap_or_else(|_| e.path()),
            kind: DriveKind::Fixed,
        })
        .collect();
    drives.sort_by(|a, b| a.label.cmp(&b.label));
    drives
}

#[cfg(all(unix, not(target_os = "macos")))]
pub fn drives() -> Vec<Drive> {
    let text = std::fs::read_to_string("/proc/self/mounts").unwrap_or_default();
    let mut mounts = parse_mounts(&text);
    if !mounts.iter().any(|m| m == std::path::Path::new("/")) {
        mounts.insert(0, PathBuf::from("/"));
    }
    mounts
        .into_iter()
        .map(|path| {
            let removable = path.starts_with("/media") || path.starts_with("/run/media");
            let label = if path == std::path::Path::new("/") {
                "File System".to_owned()
            } else {
                path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
            };
            Drive { path, label, kind: if removable { DriveKind::Removable } else { DriveKind::Fixed } }
        })
        .collect()
}

#[cfg(not(windows))]
pub fn drive_signature() -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    if cfg!(target_os = "macos") {
        if let Ok(entries) = std::fs::read_dir("/Volumes") {
            let mut names: Vec<_> = entries.filter_map(Result::ok).map(|e| e.file_name()).collect();
            names.sort();
            names.hash(&mut hasher);
        }
    } else {
        parse_mounts(&std::fs::read_to_string("/proc/self/mounts").unwrap_or_default()).hash(&mut hasher);
    }
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_machine_has_at_least_one_drive() {
        let drives = drives();
        assert!(!drives.is_empty());
        assert!(drives.iter().all(|d| !d.label.is_empty()));
        assert!(drives.iter().filter(|d| d.kind == DriveKind::Fixed).all(|d| d.path.exists()));
    }

    /// The startup disk's entry in /Volumes links to `/`; it is the drive of `/`.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_startup_disk_is_the_root() {
        assert!(drives().iter().any(|d| d.path == std::path::Path::new("/")), "{:?}", drives());
    }

    #[test]
    fn signature_is_stable_without_changes() {
        assert_eq!(drive_signature(), drive_signature());
    }

    #[test]
    fn network_and_optical_labels_fall_back_to_the_kind() {
        assert_eq!(labeled(&DriveKind::Network, 'Z', Some("Share")), "Share (Z:)");
        assert_eq!(labeled(&DriveKind::Network, 'Z', None), "Network (Z:)");
        assert_eq!(labeled(&DriveKind::Network, 'Z', Some("")), "Network (Z:)");
        assert_eq!(labeled(&DriveKind::Network, 'Z', Some("  ")), "Network (Z:)");
        assert_eq!(labeled(&DriveKind::Optical, 'E', None), "CD Drive (E:)");
        assert_eq!(labeled(&DriveKind::Optical, 'E', Some("WIN11_DVD")), "WIN11_DVD (E:)");
    }

    #[test]
    fn oversized_octal_escapes_are_kept_literally() {
        let text = "/dev/x /mnt/a\\777b ext4 rw 0 0
";
        assert_eq!(parse_mounts(text), [PathBuf::from(r"/mnt/a\777b")]);
    }

    #[test]
    fn parses_linux_mounts() {
        let text = "/dev/sda1 / ext4 rw 0 0\nproc /proc proc rw 0 0\n/dev/sdb1 /media/alice/My\\040Stick vfat rw 0 0\n/dev/sdc1 /mnt/data ext4 rw 0 0\ntmpfs /run/user/1000 tmpfs rw 0 0\n";
        let mounts = parse_mounts(text);
        assert_eq!(mounts, [PathBuf::from("/"), PathBuf::from("/media/alice/My Stick"), PathBuf::from("/mnt/data")]);
    }
}
