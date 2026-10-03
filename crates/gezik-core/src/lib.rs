//! Platform-independent core: directory listing, sorting and formatting.
//! Nothing in here touches the UI, so it can be tested and reused freely.

use std::cmp::Ordering;
use std::io;
use std::path::Path;
use std::time::SystemTime;

#[derive(Debug, Clone)]
pub struct Entry {
    /// File name only. The full path is `dir.join(name)`; storing it per entry would
    /// repeat the folder path for every file and dominate memory in large folders.
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
}

/// Reads a directory and returns its entries sorted: folders first, then by name.
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
                size: if is_dir { 0 } else { meta.as_ref().map_or(0, |m| m.len()) },
                modified: meta.and_then(|m| m.modified().ok()),
            })
        })
        .collect();
    entries.sort_by(compare_entries);
    entries.shrink_to_fit();
    Ok(entries)
}

fn compare_entries(a: &Entry, b: &Entry) -> Ordering {
    b.is_dir
        .cmp(&a.is_dir)
        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        .then_with(|| a.name.cmp(&b.name))
}

/// Formats a byte count for display, e.g. `1.5 KB`.
pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
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
    fn formats_sizes() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(1023), "1023 B");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(5 * 1024 * 1024), "5.0 MB");
    }
}
