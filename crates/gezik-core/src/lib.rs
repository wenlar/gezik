//! Platform-independent core: directory listing, sorting and formatting.
//! Nothing in here touches the UI, so it can be tested and reused freely.

pub mod attrs;
pub mod batch;
pub mod complete;
pub mod drag;
pub mod elevated;
pub mod group;
pub mod history;
pub mod kind;
pub mod layout;
pub mod nav;
pub mod ops;
pub mod palette;
pub mod path_text;
pub mod pattern;
pub mod refresh;
pub mod search;
pub mod selection;
pub mod sort;
pub mod system_change;
pub mod templates;
pub mod text;
pub mod view;
pub mod view_memory;

use std::io;
use std::path::Path;
use std::time::SystemTime;

#[derive(Debug, Clone)]
pub struct Entry {
    /// File name only. The full path is `dir.join(name)`; storing it per entry would
    /// repeat the folder path for every file and dominate memory in large folders.
    pub name: String,
    pub is_dir: bool,
    /// `Entry::HIDDEN` and `Entry::SYSTEM` (Windows' attributes), the size bits and the cloud
    /// bits. It sits in the padding after `is_dir`, so an entry is no larger for it (spec 7.1).
    pub flags: u8,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub created: Option<SystemTime>,
}

impl Entry {
    /// The hidden attribute (Windows).
    pub const HIDDEN: u8 = 1;
    /// The system attribute (Windows).
    pub const SYSTEM: u8 = 2;

    /// Folder sizes (8b, spec 6), in the bits `HIDDEN` and `SYSTEM` leave free: `size` holds the
    /// folder's total (`SIZED`); part of it could not be read (`SIZE_PARTIAL`, shown `≥`); it is
    /// on its way (`SIZE_PENDING`, shown `…`, last when sorting by size); it is older than five
    /// minutes and on its way again (`SIZE_STALE`, drawn faint). Only the view sets them.
    pub const SIZED: u8 = 4;
    pub const SIZE_PARTIAL: u8 = 8;
    pub const SIZE_PENDING: u8 = 16;
    pub const SIZE_STALE: u8 = 32;
    pub const SIZE_FLAGS: u8 = Entry::SIZED | Entry::SIZE_PARTIAL | Entry::SIZE_PENDING | Entry::SIZE_STALE;

    /// Cloud state (spec 9 §7.3), from the folder read itself (no call of its own), in the two
    /// bits left free: the data is only in the cloud (Windows: recall on access or open,
    /// offline; macOS: dataless), or it is kept on this device for good (Windows: pinned).
    pub const CLOUD_ONLY: u8 = 64;
    pub const PINNED: u8 = 128;

    /// What a row under a cloud root shows.
    pub fn cloud_state(&self) -> CloudState {
        cloud_state(self.flags)
    }

    /// The size the Size column and the sums use: a file's; a folder's worked-out total, else none.
    pub fn known_size(&self) -> Option<u64> {
        (!self.is_dir || self.flags & Entry::SIZED != 0).then_some(self.size)
    }

    /// A folder whose size is on its way and not known yet: `…`, last when sorting by size.
    pub fn size_pending(&self) -> bool {
        self.is_dir && self.flags & (Entry::SIZED | Entry::SIZE_PENDING) == Entry::SIZE_PENDING
    }

    /// Whether the list shows it: an item both hidden and system ("protected operating system
    /// files": `desktop.ini`, `$RECYCLE.BIN`) only with `show_system`; a name starting with a
    /// dot or a hidden one only with `show_hidden`; anything else always (spec 7.1).
    pub fn is_shown(&self, show_hidden: bool, show_system: bool) -> bool {
        is_shown_name(&self.name, self.flags, show_hidden, show_system)
    }

    /// The extension without the dot (`"txt"`), or `""` for folders and names without one.
    /// A leading dot alone (`.gitignore`) is not an extension, as with `Path::extension`.
    pub fn extension(&self) -> &str {
        if self.is_dir {
            return "";
        }
        match self.name.rfind('.') {
            Some(i) if i > 0 => &self.name[i + 1..],
            _ => "",
        }
    }
}

/// [`Entry::is_shown`] for a name and its flags, without an `Entry` (the search's scanner
/// decides on what a folder read gave).
pub fn is_shown_name(name: &str, flags: u8, show_hidden: bool, show_system: bool) -> bool {
    let has = |bit: u8| flags & bit != 0;
    if has(Entry::HIDDEN) && has(Entry::SYSTEM) {
        return show_system;
    }
    show_hidden || !(name.starts_with('.') || has(Entry::HIDDEN))
}

/// The name the list shows: without its extension when `hide_extension` asks for it (only for
/// files, and never a leading dot alone or a trailing one: `.gitignore`, `x.` stay). Drawing
/// only: everything else goes by the real name (spec 7.1).
pub fn shown_name(name: &str, is_dir: bool, hide_extension: bool) -> &str {
    if !hide_extension || is_dir {
        return name;
    }
    match name.rfind('.') {
        Some(i) if i > 0 && i + 1 < name.len() => &name[..i],
        _ => name,
    }
}

/// What a row under a cloud root shows (spec 9 §7.3); the number is file-icon.slint's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudState {
    OnlyInCloud = 1,
    Local = 2,
    AlwaysLocal = 3,
}

/// Pinned while the data is still on its way counts as in the cloud: it is not here yet.
pub fn cloud_state(flags: u8) -> CloudState {
    if flags & Entry::CLOUD_ONLY != 0 {
        CloudState::OnlyInCloud
    } else if flags & Entry::PINNED != 0 {
        CloudState::AlwaysLocal
    } else {
        CloudState::Local
    }
}

/// The macOS `st_flags` bit of a file whose data is only in the cloud (iCloud, File
/// Provider): reading it downloads it.
pub const SF_DATALESS: u32 = 0x4000_0000;

/// `Entry` flags from Windows file attributes as the folder read gives them: hidden, system
/// and the cloud bits.
pub fn windows_flags(attributes: u32) -> u8 {
    const HIDDEN: u32 = 0x2;
    const SYSTEM: u32 = 0x4;
    const OFFLINE: u32 = 0x1000;
    const RECALL_ON_OPEN: u32 = 0x4_0000;
    const PINNED: u32 = 0x8_0000;
    const RECALL_ON_DATA_ACCESS: u32 = 0x40_0000;
    let mut flags = 0;
    if attributes & HIDDEN != 0 {
        flags |= Entry::HIDDEN;
    }
    if attributes & SYSTEM != 0 {
        flags |= Entry::SYSTEM;
    }
    if attributes & (OFFLINE | RECALL_ON_OPEN | RECALL_ON_DATA_ACCESS) != 0 {
        flags |= Entry::CLOUD_ONLY;
    }
    if attributes & PINNED != 0 {
        flags |= Entry::PINNED;
    }
    flags
}

/// `Entry` flags from macOS `st_flags`: only the cloud bit (hidden goes by the dot name).
pub fn mac_flags(st_flags: u32) -> u8 {
    if st_flags & SF_DATALESS != 0 { Entry::CLOUD_ONLY } else { 0 }
}

/// `Entry` flags from what the directory read already gave (no call of its own: Windows fills
/// `file_attributes` from the listing, macOS `st_flags` from the `lstat` the listing does).
#[cfg(windows)]
pub fn attribute_flags(meta: &std::fs::Metadata) -> u8 {
    use std::os::windows::fs::MetadataExt;
    windows_flags(meta.file_attributes())
}

#[cfg(target_os = "macos")]
pub fn attribute_flags(meta: &std::fs::Metadata) -> u8 {
    use std::os::macos::fs::MetadataExt;
    mac_flags(meta.st_flags())
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn attribute_flags(_meta: &std::fs::Metadata) -> u8 {
    0
}

/// Reads a directory and returns its entries sorted: folders first, then by natural name order
/// (see `sort`); the view sorts again for other choices.
/// Entries that cannot be read (e.g. permission denied) are skipped instead of
/// failing the whole listing.
pub fn list_dir(path: &Path) -> io::Result<Vec<Entry>> {
    let mut entries: Vec<Entry> = std::fs::read_dir(path)?
        .filter_map(Result::ok)
        .filter_map(|e| {
            let file_type = e.file_type().ok()?;
            let is_dir = file_type.is_dir() || (file_type.is_symlink() && e.path().is_dir());
            let meta = e.metadata().ok();
            Some(Entry {
                name: e.file_name().to_string_lossy().into_owned(),
                is_dir,
                flags: meta.as_ref().map_or(0, attribute_flags),
                size: if is_dir { 0 } else { meta.as_ref().map_or(0, |m| m.len()) },
                modified: meta.as_ref().and_then(|m| m.modified().ok()),
                created: meta.as_ref().and_then(|m| m.created().ok()),
            })
        })
        .collect();
    sort::sort_entries(&mut entries, sort::SortSpec::default(), true, |_| String::new());
    entries.shrink_to_fit();
    Ok(entries)
}

/// Formats a byte count for display, e.g. `1.5 KB` (binary steps; see `format_size_in`).
pub fn format_size(bytes: u64) -> String {
    format_size_in(bytes, view::SizeFormat::Binary)
}

/// A byte count as `format` writes it: `1.5 KB` (steps of 1024) or `1.5 kB` (steps of 1000).
pub fn format_size_in(bytes: u64, format: view::SizeFormat) -> String {
    let (step, units): (f64, [&str; 5]) = match format {
        view::SizeFormat::Binary => (1024.0, ["B", "KB", "MB", "GB", "TB"]),
        view::SizeFormat::Decimal => (1000.0, ["B", "kB", "MB", "GB", "TB"]),
    };
    if (bytes as f64) < step {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= step && unit < units.len() - 1 {
        value /= step;
        unit += 1;
    }
    format!("{value:.1} {}", units[unit])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gezik-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn flagged(name: &str, flags: u8) -> Entry {
        Entry { name: name.to_owned(), is_dir: false, flags, size: 0, modified: None, created: None }
    }

    fn sized_dir(flags: u8, size: u64) -> Entry {
        Entry { name: "d".into(), is_dir: true, flags, size, modified: None, created: None }
    }

    #[test]
    fn a_folder_has_a_size_only_once_worked_out() {
        let file = Entry { name: "f".into(), is_dir: false, flags: 0, size: 7, modified: None, created: None };
        assert_eq!(file.known_size(), Some(7));
        assert_eq!(sized_dir(0, 4096).known_size(), None, "a folder's own record size is no size");
        assert_eq!(sized_dir(Entry::SIZED, 500).known_size(), Some(500));
        assert_eq!(sized_dir(Entry::SIZED | Entry::SIZE_PARTIAL, 500).known_size(), Some(500));
        assert!(sized_dir(Entry::SIZE_PENDING, 0).size_pending());
        assert!(
            !sized_dir(Entry::SIZED | Entry::SIZE_STALE, 9).size_pending(),
            "an old size is shown while it is redone"
        );
        assert!(!sized_dir(0, 0).size_pending());
        assert!(
            sized_dir(Entry::HIDDEN | Entry::SIZE_PENDING, 0).is_shown(true, false),
            "the size bits leave hidden alone"
        );
        assert_eq!(Entry::SIZE_FLAGS & (Entry::HIDDEN | Entry::SYSTEM), 0);
    }

    #[test]
    fn cloud_bits_come_from_the_attributes() {
        assert_eq!(windows_flags(0x40_0000), Entry::CLOUD_ONLY, "recall on data access (OneDrive online-only)");
        assert_eq!(windows_flags(0x4_0000), Entry::CLOUD_ONLY, "recall on open");
        assert_eq!(windows_flags(0x1000), Entry::CLOUD_ONLY, "offline");
        assert_eq!(windows_flags(0x8_0000), Entry::PINNED, "always keep on this device");
        assert_eq!(
            windows_flags(0x10_0000 | 0x20 | 0x400),
            0,
            "unpinned, archive, a placeholder's reparse bit: still here"
        );
        assert_eq!(windows_flags(0x2 | 0x4 | 0x8_0000), Entry::HIDDEN | Entry::SYSTEM | Entry::PINNED);
        assert_eq!(mac_flags(SF_DATALESS), Entry::CLOUD_ONLY);
        assert_eq!(mac_flags(0x8000 | 0x20), 0, "hidden and other flags are not cloud bits");
    }

    #[test]
    fn a_pinned_item_still_on_its_way_shows_as_in_the_cloud() {
        assert_eq!(cloud_state(0), CloudState::Local);
        assert_eq!(cloud_state(Entry::CLOUD_ONLY), CloudState::OnlyInCloud);
        assert_eq!(cloud_state(Entry::PINNED), CloudState::AlwaysLocal);
        assert_eq!(cloud_state(Entry::PINNED | Entry::CLOUD_ONLY), CloudState::OnlyInCloud);
        assert_eq!(cloud_state(Entry::HIDDEN | Entry::SIZED), CloudState::Local);
        assert_eq!(CloudState::AlwaysLocal as i32, 3, "the number file-icon.slint draws");
    }

    #[test]
    fn the_cloud_bits_meet_no_other_flag() {
        let others = Entry::HIDDEN | Entry::SYSTEM | Entry::SIZE_FLAGS;
        assert_eq!((Entry::CLOUD_ONLY | Entry::PINNED) & others, 0);
        assert_ne!(Entry::CLOUD_ONLY, Entry::PINNED);
    }

    #[test]
    fn the_flags_fit_in_the_padding_after_is_dir() {
        // `Entry` before the flags: the byte must take no room of its own (spec 7.1).
        #[allow(dead_code)]
        struct Before {
            name: String,
            is_dir: bool,
            size: u64,
            modified: Option<SystemTime>,
            created: Option<SystemTime>,
        }
        assert_eq!(std::mem::size_of::<Entry>(), std::mem::size_of::<Before>());
    }

    #[test]
    fn hidden_and_system_items_follow_the_two_settings() {
        let plain = flagged("a.txt", 0);
        let dot = flagged(".git", 0);
        let hidden = flagged("notes.txt", Entry::HIDDEN);
        let system_only = flagged("pagefile.sys", Entry::SYSTEM);
        let protected = flagged("desktop.ini", Entry::HIDDEN | Entry::SYSTEM);
        for show_hidden in [false, true] {
            for show_system in [false, true] {
                assert!(plain.is_shown(show_hidden, show_system));
                assert!(system_only.is_shown(show_hidden, show_system), "the system attribute alone hides nothing");
                assert_eq!(dot.is_shown(show_hidden, show_system), show_hidden, "a dot name");
                assert_eq!(hidden.is_shown(show_hidden, show_system), show_hidden, "the hidden attribute");
                assert_eq!(protected.is_shown(show_hidden, show_system), show_system, "protected: show-system only");
            }
        }
    }

    #[test]
    fn hiding_the_extension_keeps_folders_and_dot_names() {
        assert_eq!(shown_name("rapor.pdf", false, true), "rapor");
        assert_eq!(shown_name("a.tar.gz", false, true), "a.tar");
        assert_eq!(shown_name(".gitignore", false, true), ".gitignore");
        assert_eq!(shown_name("x.", false, true), "x.");
        assert_eq!(shown_name("noext", false, true), "noext");
        assert_eq!(shown_name("dir.d", true, true), "dir.d");
        assert_eq!(shown_name("rapor.pdf", false, false), "rapor.pdf");
    }

    #[cfg(windows)]
    #[test]
    #[allow(clippy::permissions_set_readonly_false)] // Windows only: it clears the attribute
    fn windows_attributes_reach_the_entry() {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
        const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;
        let dir = temp_dir("attributes");
        let make = |name: &str, attributes: u32| {
            fs::OpenOptions::new().write(true).create_new(true).attributes(attributes).open(dir.join(name)).unwrap();
        };
        make("plain.txt", 0);
        make("hidden.txt", FILE_ATTRIBUTE_HIDDEN);
        make("desktop.ini", FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM);
        let entries = list_dir(&dir).unwrap();
        let flags = |name: &str| entries.iter().find(|e| e.name == name).unwrap().flags;
        assert_eq!(flags("plain.txt"), 0);
        assert_eq!(flags("hidden.txt"), Entry::HIDDEN);
        assert_eq!(flags("desktop.ini"), Entry::HIDDEN | Entry::SYSTEM);
        // Read-only and system files cannot always be deleted plainly: clear the attributes.
        for name in ["hidden.txt", "desktop.ini"] {
            let mut permissions = fs::metadata(dir.join(name)).unwrap().permissions();
            permissions.set_readonly(false);
            let _ = fs::set_permissions(dir.join(name), permissions);
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn lists_folders_first_then_case_insensitive_names() {
        let dir = temp_dir("sort");
        fs::write(dir.join("b.txt"), "hello").unwrap();
        fs::write(dir.join("A.txt"), "").unwrap();
        fs::create_dir(dir.join("zeta")).unwrap();
        fs::create_dir(dir.join("Alpha")).unwrap();

        let names: Vec<_> = list_dir(&dir).unwrap().into_iter().map(|e| e.name).collect();
        assert_eq!(names, ["Alpha", "zeta", "A.txt", "b.txt"]);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn reports_file_sizes_and_zero_for_folders() {
        let dir = temp_dir("size");
        fs::write(dir.join("five.txt"), "12345").unwrap();
        fs::create_dir(dir.join("sub")).unwrap();

        let entries = list_dir(&dir).unwrap();
        assert!(entries[0].is_dir && entries[0].size == 0);
        assert!(!entries[1].is_dir && entries[1].size == 5);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_directory_is_an_error() {
        assert!(list_dir(Path::new("/definitely/not/here/gezik")).is_err());
    }

    #[test]
    fn sizes_in_both_formats() {
        use crate::view::SizeFormat::{Binary, Decimal};
        assert_eq!(format_size_in(1536, Binary), "1.5 KB");
        assert_eq!(format_size_in(1023, Binary), "1023 B");
        assert_eq!(format_size_in(1024, Binary), "1.0 KB");
        assert_eq!(format_size_in(1500, Decimal), "1.5 kB");
        assert_eq!(format_size_in(999, Decimal), "999 B");
        assert_eq!(format_size_in(1000, Decimal), "1.0 kB");
        assert_eq!(format_size_in(1_000_000, Decimal), "1.0 MB");
        assert_eq!(format_size_in(5 * 1024 * 1024, Decimal), "5.2 MB");
        assert_eq!(format_size_in(u64::MAX, Binary), "16777216.0 TB");
        assert_eq!(format_size_in(u64::MAX, Decimal), "18446744.1 TB");
        assert_eq!(format_size(1536), format_size_in(1536, Binary), "format_size stays binary");
    }

    #[test]
    fn formats_sizes() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(1023), "1023 B");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(5 * 1024 * 1024), "5.0 MB");
    }

    #[test]
    fn extension_skips_folders_and_leading_dots() {
        let e = |name: &str, is_dir| Entry {
            name: name.to_owned(),
            is_dir,
            flags: 0,
            size: 0,
            modified: None,
            created: None,
        };
        assert_eq!(e("a.tar.gz", false).extension(), "gz");
        assert_eq!(e(".gitignore", false).extension(), "");
        assert_eq!(e("noext", false).extension(), "");
        assert_eq!(e("dir.d", true).extension(), "");
    }

    #[test]
    fn a_name_and_its_flags_are_shown_as_an_entry_is() {
        for (name, flags) in
            [("a.txt", 0), (".git", 0), ("n.txt", Entry::HIDDEN), ("d.ini", Entry::HIDDEN | Entry::SYSTEM)]
        {
            for (hidden, system) in [(false, false), (true, false), (false, true), (true, true)] {
                assert_eq!(is_shown_name(name, flags, hidden, system), flagged(name, flags).is_shown(hidden, system));
            }
        }
    }
}
