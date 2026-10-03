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
        if bytes[i] == b'\\' && i + 3 < bytes.len() && bytes[i + 1..i + 4].iter().all(|b| (b'0'..=b'7').contains(b)) {
            let value = (bytes[i + 1] - b'0') * 64 + (bytes[i + 2] - b'0') * 8 + (bytes[i + 3] - b'0');
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
            let label = crate::known::display_name(std::path::Path::new(&root))
                .unwrap_or_else(|| root.trim_end_matches('\\').to_owned());
            Drive { path: PathBuf::from(root), label, kind }
        })
        .collect()
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
        .map(|e| Drive { label: e.file_name().to_string_lossy().into_owned(), path: e.path(), kind: DriveKind::Fixed })
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
        assert!(drives.iter().all(|d| d.path.exists() && !d.label.is_empty()));
    }

    #[test]
    fn signature_is_stable_without_changes() {
        assert_eq!(drive_signature(), drive_signature());
    }

    #[test]
    fn parses_linux_mounts() {
        let text = "/dev/sda1 / ext4 rw 0 0\nproc /proc proc rw 0 0\n/dev/sdb1 /media/alice/My\\040Stick vfat rw 0 0\n/dev/sdc1 /mnt/data ext4 rw 0 0\ntmpfs /run/user/1000 tmpfs rw 0 0\n";
        let mounts = parse_mounts(text);
        assert_eq!(mounts, [PathBuf::from("/"), PathBuf::from("/media/alice/My Stick"), PathBuf::from("/mnt/data")]);
    }
}
