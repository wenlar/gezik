//! The Mark of the Web (Windows): a file taken out of a downloaded archive and handed to
//! another program (opened with its app, dragged out, copied out) carries the archive's mark,
//! as Explorer does for files extracted from a downloaded zip, so SmartScreen and Office's
//! Protected View still apply. An archive without a mark gives none; the preview inside Gezik
//! marks nothing. Nothing happens off Windows.

use std::path::Path;

/// Gives every file at or under `path` the mark the archive `archive` (on disk) has, if any.
pub fn mark_like(archive: Option<&Path>, path: &Path) {
    #[cfg(windows)]
    if let Some(mark) = archive.and_then(|archive| std::fs::read(stream(archive)).ok()) {
        mark_all(path, &mark);
    }
    #[cfg(not(windows))]
    let _ = (archive, path);
}

#[cfg(windows)]
fn stream(path: &Path) -> std::path::PathBuf {
    let mut text = path.as_os_str().to_owned();
    text.push(":Zone.Identifier");
    text.into()
}

#[cfg(windows)]
fn mark_all(path: &Path, mark: &[u8]) {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => {
            for entry in std::fs::read_dir(path).into_iter().flatten().flatten() {
                mark_all(&entry.path(), mark);
            }
        }
        Ok(meta) if meta.is_file() => {
            // Writing a stream sets the file's change time: the entry's own date is put back.
            if std::fs::write(stream(path), mark).is_ok()
                && let Ok(modified) = meta.modified()
                && let Ok(file) = std::fs::OpenOptions::new().write(true).open(path)
            {
                let _ = file.set_modified(modified);
            }
        }
        _ => {}
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn files_taken_out_carry_the_archives_mark() {
        let d = std::env::temp_dir().join(format!("gezik-vfs-motw-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("out/sub")).unwrap();
        let (marked, plain) = (d.join("a.zip"), d.join("b.zip"));
        std::fs::write(&marked, b"zip").unwrap();
        std::fs::write(&plain, b"zip").unwrap();
        let mark = b"[ZoneTransfer]\r\nZoneId=3\r\n";
        std::fs::write(stream(&marked), mark).unwrap();
        std::fs::write(d.join("out/sub/x.txt"), b"x").unwrap();
        std::fs::write(d.join("out/y.txt"), b"y").unwrap();
        mark_like(Some(&plain), &d.join("out"));
        assert!(std::fs::read(stream(&d.join("out/y.txt"))).is_err(), "no mark to give");
        mark_like(Some(&marked), &d.join("out"));
        assert_eq!(std::fs::read(stream(&d.join("out/sub/x.txt"))).unwrap(), mark);
        assert_eq!(std::fs::read(stream(&d.join("out/y.txt"))).unwrap(), mark);
        let _ = std::fs::remove_dir_all(&d);
    }
}
