//! Places that are no folder on disk (spec 16 §4): inside an archive, on a phone. Pure: the
//! types, their text, and how a typed path splits into a file and what is inside it.

use std::path::{Component, MAIN_SEPARATOR, Path, PathBuf};

use crate::batch::archive::looks_like_archive;

/// How many archives deep a place may be (spec 16 §5.4).
pub const MAX_DEPTH: usize = 8;

/// A place inside a root: `inner` is its path there with `/`, no leading `/`; `""` is the root.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VfsPath {
    pub root: VfsRoot,
    pub inner: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum VfsRoot {
    /// An archive: on disk, or inside another root (an archive in an archive).
    Archive(Container),
    /// A phone (MTP, 16f): its lasting id and the name shown.
    Device { id: String, name: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Container {
    Disk(PathBuf),
    Inside(Box<VfsPath>),
}

/// A change to a place (spec 16 §4.3); names are inner paths (`a/b.txt`). An archive applies a
/// list of them as one rewrite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    /// The file or folder `from` (with what it holds) as `to`.
    Put {
        from: PathBuf,
        to: String,
    },
    MakeDir(String),
    /// The entry, a folder with what it holds.
    Delete(String),
    Rename {
        from: String,
        to: String,
    },
}

/// What a place can do besides being read (spec 16 §4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Caps(u8);

impl Caps {
    pub const WRITE: Caps = Caps(1);
    pub const RENAME: Caps = Caps(2);
    pub const MKDIR: Caps = Caps(4);
    pub const DELETE: Caps = Caps(8);
    /// Everything an archive Gezik writes can do.
    pub const EDIT: Caps = Caps(1 | 2 | 4 | 8);

    pub fn has(self, other: Caps) -> bool {
        self.0 & other.0 == other.0
    }
}

impl VfsPath {
    /// The top of the archive at `file`.
    pub fn archive(file: PathBuf) -> VfsPath {
        VfsPath { root: VfsRoot::Archive(Container::Disk(file)), inner: String::new() }
    }

    /// The top of the archive that is the entry `at`.
    pub fn inside(at: VfsPath) -> VfsPath {
        VfsPath { root: VfsRoot::Archive(Container::Inside(Box::new(at))), inner: String::new() }
    }

    /// The entry `name` in this place.
    pub fn join(&self, name: &str) -> VfsPath {
        let inner = if self.inner.is_empty() { name.to_owned() } else { format!("{}/{name}", self.inner) };
        VfsPath { root: self.root.clone(), inner }
    }

    /// One up inside the root; `None` at its top.
    pub fn parent(&self) -> Option<VfsPath> {
        if self.inner.is_empty() {
            return None;
        }
        let inner = self.inner.rsplit_once('/').map_or("", |(up, _)| up).to_owned();
        Some(VfsPath { root: self.root.clone(), inner })
    }

    /// Its last part; at the top, the archive's or the phone's name.
    pub fn name(&self) -> String {
        match self.inner.rsplit('/').next().filter(|name| !name.is_empty()) {
            Some(name) => name.to_owned(),
            None => self.root.name(),
        }
    }

    /// The archive on disk it is in (the outermost one); `None` on a phone.
    pub fn disk(&self) -> Option<&Path> {
        match &self.root {
            VfsRoot::Archive(Container::Disk(file)) => Some(file),
            VfsRoot::Archive(Container::Inside(at)) => at.disk(),
            VfsRoot::Device { .. } => None,
        }
    }

    /// How many archives deep it is (1: in an archive on disk).
    pub fn depth(&self) -> usize {
        match &self.root {
            VfsRoot::Archive(Container::Inside(at)) => at.depth() + 1,
            _ => 1,
        }
    }

    /// An archive place as the session keeps it (§4.2): the file on disk, then what is inside
    /// at each level (`["old/b.tar.gz", "src"]`); `None` on a phone.
    pub fn levels(&self) -> Option<(PathBuf, Vec<String>)> {
        let (file, mut inner) = match &self.root {
            VfsRoot::Archive(Container::Disk(file)) => (file.clone(), Vec::new()),
            VfsRoot::Archive(Container::Inside(at)) => at.levels()?,
            VfsRoot::Device { .. } => return None,
        };
        inner.push(self.inner.clone());
        Some((file, inner))
    }

    /// The place `levels` gives back.
    pub fn from_levels(file: PathBuf, inner: &[String]) -> Option<VfsPath> {
        let (last, outer) = inner.split_last()?;
        let mut at = VfsPath::archive(file);
        for level in outer {
            at = VfsPath::inside(VfsPath { inner: level.clone(), ..at });
        }
        Some(VfsPath { inner: last.clone(), ..at })
    }

    /// As the address bar and Copy Path write it (§4.2): `C:\a\b.zip\docs`, `mtp://Pixel 8/DCIM`.
    pub fn text(&self) -> String {
        let root = self.root.text();
        if self.inner.is_empty() {
            return root;
        }
        let separator = if matches!(self.root, VfsRoot::Device { .. }) { '/' } else { MAIN_SEPARATOR };
        format!("{root}{separator}{}", self.inner.replace('/', &separator.to_string()))
    }
}

impl VfsRoot {
    /// What tells roots apart: its text (one archive file, one phone).
    pub fn text(&self) -> String {
        match self {
            VfsRoot::Archive(Container::Disk(file)) => file.display().to_string(),
            VfsRoot::Archive(Container::Inside(at)) => at.text(),
            VfsRoot::Device { name, .. } => format!("mtp://{name}"),
        }
    }

    pub fn name(&self) -> String {
        match self {
            VfsRoot::Archive(Container::Disk(file)) => {
                file.file_name().map_or_else(|| file.display().to_string(), |n| n.to_string_lossy().into_owned())
            }
            VfsRoot::Archive(Container::Inside(at)) => at.name(),
            VfsRoot::Device { name, .. } => name.clone(),
        }
    }
}

/// A typed `path` that is no folder, split at the first part that is a file named like an
/// archive (`is_file` asks the disk; a folder called `x.zip` stays a folder): that file, and
/// the rest as the place inside it. `None` when no part is one.
pub fn split_archive(path: &Path, is_file: impl Fn(&Path) -> bool) -> Option<VfsPath> {
    let mut file = PathBuf::new();
    let mut parts = path.components();
    while let Some(part) = parts.next() {
        file.push(part);
        if let Component::Normal(name) = part
            && looks_like_archive(&name.to_string_lossy())
            && is_file(&file)
        {
            let mut inner: Vec<String> = Vec::new();
            for part in parts {
                match part {
                    Component::Normal(name) => inner.push(name.to_string_lossy().into_owned()),
                    Component::ParentDir => {
                        inner.pop();
                    }
                    _ => {}
                }
            }
            return Some(VfsPath { root: VfsRoot::Archive(Container::Disk(file)), inner: inner.join("/") });
        }
    }
    None
}

/// A typed phone address (`mtp://Pixel 8/DCIM`, §4.2, either slash): the phone's name and the
/// place on it. `None` for any other text.
pub fn split_device(path: &Path) -> Option<(String, String)> {
    let text = path.to_string_lossy().replace('\\', "/");
    let rest = text.get(6..).filter(|_| text[..6].eq_ignore_ascii_case("mtp://"))?;
    let mut parts = rest.split('/').filter(|part| !part.is_empty() && *part != ".");
    let name = parts.next()?.to_owned();
    let mut inner: Vec<&str> = Vec::new();
    for part in parts {
        if part == ".." {
            inner.pop();
        } else {
            inner.push(part);
        }
    }
    Some((name, inner.join("/")))
}

/// The same rule inside an archive: `inner` split at its first part that is a file there
/// (`is_file`) named like an archive; (that entry, the rest).
pub fn split_inner(inner: &str, is_file: impl Fn(&str) -> bool) -> Option<(String, String)> {
    let mut end = 0;
    for part in inner.split('/') {
        end += part.len();
        if looks_like_archive(part) && is_file(&inner[..end]) {
            return Some((inner[..end].to_owned(), inner.get(end + 1..).unwrap_or_default().to_owned()));
        }
        end += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disk(path: &str, inner: &str) -> VfsPath {
        VfsPath { root: VfsRoot::Archive(Container::Disk(PathBuf::from(path))), inner: inner.into() }
    }

    #[test]
    fn a_typed_path_splits_at_the_archive_file() {
        let files = ["/w/a.zip", "/w/b.ZIP", "/w/notes.txt"];
        let is_file = |p: &Path| files.iter().any(|f| Path::new(f) == p);
        assert_eq!(split_archive(Path::new("/w/a.zip/docs/2024"), is_file), Some(disk("/w/a.zip", "docs/2024")));
        assert_eq!(split_archive(Path::new("/w/a.zip/"), is_file), Some(disk("/w/a.zip", "")), "trailing /");
        assert_eq!(split_archive(Path::new("/w/b.ZIP/x"), is_file), Some(disk("/w/b.ZIP", "x")), "any case");
        assert_eq!(split_archive(Path::new("/w/a.zip/x/../y"), is_file), Some(disk("/w/a.zip", "y")));
        // Nested: only the disk part splits here; the rest is looked at inside.
        assert_eq!(split_archive(Path::new("/w/a.zip/old/c.tar.gz/src"), is_file).unwrap().inner, "old/c.tar.gz/src");
        assert_eq!(split_archive(Path::new("/w/x.zip/a"), is_file), None, "a folder called x.zip");
        assert_eq!(split_archive(Path::new("/w/notes.txt/a"), is_file), None, "a file, but no archive");
        assert_eq!(split_archive(Path::new("/w/docs"), is_file), None);
    }

    #[test]
    fn a_phone_address_gives_its_name_and_place() {
        let split = |text: &str| split_device(Path::new(text));
        assert_eq!(
            split("mtp://Pixel 8/Internal shared storage/DCIM"),
            Some(("Pixel 8".into(), "Internal shared storage/DCIM".into()))
        );
        assert_eq!(split(r"MTP:\\Pixel 8\a\..\b\"), Some(("Pixel 8".into(), "b".into())), "either slash, any case");
        assert_eq!(split("mtp://Pixel 8"), Some(("Pixel 8".into(), String::new())));
        assert_eq!(split("mtp://"), None);
        assert_eq!(split("/w/mtp://x"), None);
        assert_eq!(split("mt"), None);
        let at =
            VfsPath { root: VfsRoot::Device { id: "usb#1".into(), name: "Pixel 8".into() }, inner: "S/DCIM".into() };
        assert_eq!(split(&at.text()), Some(("Pixel 8".into(), "S/DCIM".into())), "its own text");
    }

    #[cfg(windows)]
    #[test]
    fn windows_paths_split_too() {
        let is_file = |p: &Path| p == Path::new(r"C:\iş\yedek.zip");
        let split = split_archive(Path::new(r"C:\iş\yedek.zip\docs\2024"), is_file).unwrap();
        assert_eq!(split, disk(r"C:\iş\yedek.zip", "docs/2024"));
        assert_eq!(split.text(), r"C:\iş\yedek.zip\docs\2024");
    }

    #[test]
    fn inside_an_archive_the_first_archive_file_splits() {
        let is_file = |p: &str| ["old/c.tar.gz", "old/c.tar.gz.txt"].contains(&p);
        assert_eq!(split_inner("old/c.tar.gz/src/a", is_file), Some(("old/c.tar.gz".into(), "src/a".into())));
        assert_eq!(split_inner("old/c.tar.gz", is_file), Some(("old/c.tar.gz".into(), String::new())));
        assert_eq!(split_inner("old/x.zip/a", is_file), None, "a folder called x.zip");
        assert_eq!(split_inner("old", is_file), None);
    }

    #[test]
    fn places_go_up_join_and_name_themselves() {
        let at = disk("/w/a.zip", "docs/2024");
        assert_eq!(at.parent(), Some(disk("/w/a.zip", "docs")));
        assert_eq!(disk("/w/a.zip", "docs").parent(), Some(disk("/w/a.zip", "")));
        assert_eq!(disk("/w/a.zip", "").parent(), None);
        assert_eq!(disk("/w/a.zip", "").join("x"), disk("/w/a.zip", "x"));
        assert_eq!(at.name(), "2024");
        assert_eq!(disk("/w/a.zip", "").name(), "a.zip");
        let nested = VfsPath::inside(disk("/w/a.zip", "old/c.tar.gz")).join("src");
        assert_eq!((nested.depth(), nested.disk()), (2, Some(Path::new("/w/a.zip"))));
        assert_eq!(nested.parent().unwrap().name(), "c.tar.gz");
        let sep = MAIN_SEPARATOR;
        assert_eq!(nested.text(), format!("/w/a.zip{sep}old{sep}c.tar.gz{sep}src"));
        let (file, levels) = nested.levels().unwrap();
        assert_eq!(
            (file.as_path(), &levels[..]),
            (Path::new("/w/a.zip"), &["old/c.tar.gz".to_owned(), "src".to_owned()][..])
        );
        assert_eq!(VfsPath::from_levels(file, &levels), Some(nested.clone()));
        assert_eq!(VfsPath::from_levels("/w/a.zip".into(), &[]), None);
        let phone =
            VfsPath { root: VfsRoot::Device { id: "usb#1".into(), name: "Pixel 8".into() }, inner: "DCIM/a".into() };
        assert_eq!((phone.text(), phone.disk()), ("mtp://Pixel 8/DCIM/a".to_owned(), None));
    }
}
