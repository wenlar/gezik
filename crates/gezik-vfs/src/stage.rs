//! The temporary area (spec 16 §4.5): what is taken out of an archive to preview it, open
//! it with its app, drag it out or open an archive in it goes to
//! `<temp>/gezik-vfs-<pid>[-<n>]/<n>/`, one folder per opened root. Made on first use, when
//! the folders of processes no longer running are removed (not at start); a root's folder goes
//! when it is let go of, the whole area when Gezik quits.
//!
//! The area holds decrypted contents: it is always a folder this process made itself (an
//! existing one of that name is never used, another name is taken), private to the user
//! (mode 0700 on Unix; the temporary folder is the user's own on Windows).

use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

const PREFIX: &str = "gezik-vfs-";

static AREA: OnceLock<Option<PathBuf>> = OnceLock::new();
static NEXT: AtomicU64 = AtomicU64::new(0);

/// This process's area: made, and dead ones cleared, the first time. `None` when no private
/// folder could be made (nothing is taken out then).
pub fn area() -> io::Result<&'static Path> {
    let made = AREA.get_or_init(|| {
        let temp = std::env::temp_dir();
        clear_dead(&temp, gezik_platform::process::process_alive);
        let pid = std::process::id();
        (0..100).find_map(|n| {
            let name = if n == 0 { format!("{PREFIX}{pid}") } else { format!("{PREFIX}{pid}-{n}") };
            let path = temp.join(name);
            make_private(&path).ok().map(|()| path)
        })
    });
    made.as_deref().ok_or_else(|| io::Error::new(io::ErrorKind::PermissionDenied, "no private temporary folder"))
}

/// Makes folder `path` itself (never an existing one), readable by this user only.
pub fn make_private(path: &Path) -> io::Result<()> {
    #[cfg_attr(not(unix), allow(unused_mut))]
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(path)
}

/// A new folder in the area, made private now (a root's, a drag's, a list file's place).
pub fn new_folder() -> io::Result<PathBuf> {
    let area = area()?;
    loop {
        let path = area.join(NEXT.fetch_add(1, Ordering::Relaxed).to_string());
        match make_private(&path) {
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {}
            done => return done.map(|()| path),
        }
    }
}

/// Removes the area, if one was made (Gezik quits).
pub fn remove_area() {
    if let Some(Some(area)) = AREA.get() {
        let _ = std::fs::remove_dir_all(area);
    }
}

/// Removes the areas in `temp` of processes `alive` says are gone.
pub fn clear_dead(temp: &Path, alive: impl Fn(u32) -> bool) {
    let Ok(listing) = std::fs::read_dir(temp) else { return };
    for entry in listing.flatten() {
        let name = entry.file_name();
        let pid = name
            .to_str()
            .and_then(|n| n.strip_prefix(PREFIX))
            .and_then(|n| n.split('-').next())
            .and_then(|n| n.parse::<u32>().ok());
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
        for name in ["gezik-vfs-1", "gezik-vfs-1-2", "gezik-vfs-2", "gezik-vfs-x", "other-3"] {
            std::fs::create_dir_all(temp.join(name).join("0")).unwrap();
        }
        clear_dead(&temp, |pid| pid == 2);
        let mut left: Vec<String> =
            std::fs::read_dir(&temp).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        left.sort();
        assert_eq!(left, ["gezik-vfs-2", "gezik-vfs-x", "other-3"]);
        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn folders_are_made_new_and_private() {
        let a = new_folder().unwrap();
        let b = new_folder().unwrap();
        assert!(a != b && a.is_dir() && b.is_dir());
        assert!(make_private(&a).is_err(), "an existing folder is never taken");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for path in [area().unwrap(), a.as_path()] {
                assert_eq!(std::fs::metadata(path).unwrap().permissions().mode() & 0o777, 0o700);
            }
        }
        let _ = (std::fs::remove_dir(&a), std::fs::remove_dir(&b));
    }
}
