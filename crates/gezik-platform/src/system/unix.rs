//! Links for macOS and Linux (spec 8.2).

use std::io;
use std::path::Path;

/// Puts a link to `target` at `link` in one step (a temporary link renamed over it): no
/// moment without one. Only an empty place (`expect` None) or Gezik's own link to `expect`
/// is replaced; anything else there (the user's file, folder or another link) is
/// `AlreadyExists` and left alone.
pub fn replace_symlink(link: &Path, target: &Path, expect: Option<&Path>) -> io::Result<()> {
    let name = link.file_name().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no file name"))?;
    let found = match std::fs::symlink_metadata(link) {
        Err(err) if err.kind() == io::ErrorKind::NotFound => None,
        Err(err) => return Err(err),
        Ok(meta) if meta.file_type().is_symlink() => Some(Some(std::fs::read_link(link)?)),
        Ok(_) => Some(None),
    };
    if !super::may_replace(found.as_ref().map(Option::as_deref), expect) {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, format!("{} is not Gezik's link", link.display())));
    }
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
        let (old, new) = (Path::new("/old/gezik"), Path::new("/new/gezik"));
        replace_symlink(&link, old, None).unwrap();
        assert_eq!(replace_symlink(&link, new, None).unwrap_err().kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(replace_symlink(&link, new, Some(new)).unwrap_err().kind(), io::ErrorKind::AlreadyExists);
        replace_symlink(&link, new, Some(old)).unwrap();
        assert_eq!(std::fs::read_link(&link).unwrap(), new);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1, "no temporary left");

        let file = dir.join("own");
        std::fs::write(&file, "user's").unwrap();
        assert_eq!(replace_symlink(&file, new, Some(old)).unwrap_err().kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "user's", "untouched");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
