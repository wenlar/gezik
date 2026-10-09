//! Links for macOS and Linux (spec 8.2).

use std::io;
use std::path::Path;

/// Puts a link to `target` at `link` in one step (a temporary link renamed over it): no
/// moment without one, and a file there is replaced only by the caller's choice.
pub fn replace_symlink(link: &Path, target: &Path) -> io::Result<()> {
    let name = link.file_name().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no file name"))?;
    let tmp = link.with_file_name(format!(".{}.{}.tmp", name.to_string_lossy(), std::process::id()));
    let _ = std::fs::remove_file(&tmp);
    std::os::unix::fs::symlink(target, &tmp)?;
    std::fs::rename(&tmp, link).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_is_replaced_in_one_step() {
        let dir = std::env::temp_dir().join(format!("gezik-system-link-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let link = dir.join("gezik");
        replace_symlink(&link, Path::new("/old/gezik")).unwrap();
        replace_symlink(&link, Path::new("/new/gezik")).unwrap();
        assert_eq!(std::fs::read_link(&link).unwrap(), Path::new("/new/gezik"));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1, "no temporary left");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
