//! The temporary area (spec 16 §4.5): what is taken out of an archive to preview it, open
//! it with its app, drag it out or open an archive in it goes to
//! `<temp>/gezik-vfs-<pid>/<n>/`, one folder per opened root. Made on first use, when the
//! folders of processes no longer running are removed (not at start); a root's folder goes
//! when it is let go of, the whole area when Gezik quits.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

const PREFIX: &str = "gezik-vfs-";

static AREA: OnceLock<PathBuf> = OnceLock::new();

/// This process's area: made, and dead ones cleared, the first time.
pub fn area() -> &'static Path {
    AREA.get_or_init(|| {
        let temp = std::env::temp_dir();
        clear_dead(&temp, gezik_platform::process::process_alive);
        temp.join(format!("{PREFIX}{}", std::process::id()))
    })
}

/// A new folder for one opened root (made when something is written into it).
pub fn new_folder() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    area().join(NEXT.fetch_add(1, Ordering::Relaxed).to_string())
}

/// Removes the area, if one was made (Gezik quits).
pub fn remove_area() {
    if let Some(area) = AREA.get() {
        let _ = std::fs::remove_dir_all(area);
    }
}

/// Removes the areas in `temp` of processes `alive` says are gone.
pub fn clear_dead(temp: &Path, alive: impl Fn(u32) -> bool) {
    let Ok(listing) = std::fs::read_dir(temp) else { return };
    for entry in listing.flatten() {
        let name = entry.file_name();
        let pid = name.to_str().and_then(|n| n.strip_prefix(PREFIX)).and_then(|n| n.parse::<u32>().ok());
        if let Some(pid) = pid
            && pid != std::process::id()
            && !alive(pid)
        {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_dead_processes_areas_are_cleared() {
        let temp = std::env::temp_dir().join(format!("gezik-vfs-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp);
        for name in ["gezik-vfs-1", "gezik-vfs-2", "gezik-vfs-x", "other-3"] {
            std::fs::create_dir_all(temp.join(name).join("0")).unwrap();
        }
        clear_dead(&temp, |pid| pid == 2);
        let mut left: Vec<String> =
            std::fs::read_dir(&temp).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        left.sort();
        assert_eq!(left, ["gezik-vfs-2", "gezik-vfs-x", "other-3"]);
        let _ = std::fs::remove_dir_all(&temp);
    }
}
