//! File operations for the administrator helper (spec 9 §10.4) that never follow a link: on
//! Unix every path is opened from the root part by part with `O_NOFOLLOW`; on Windows every
//! handle is opened on the link itself and checked to be where the path says
//! (`GetFinalPathNameByHandleW`). Each change goes through what was opened. A system folder
//! itself is recognised by its identity, not its spelling. A file with several names (hard
//! links) may lose one name but is never changed itself (its permissions, owner, flags or
//! content through a replace). Nothing here needs rights: the tests run it in the temp folder.

use std::io;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use super::secure_unix as imp;
#[cfg(windows)]
use super::secure_windows as imp;

pub const LINK_ON_THE_WAY: &str =
    "A folder on the way is a link or leads elsewhere; Gezik does not follow it as administrator";
pub const GUARDED: &str = "A system folder itself; Gezik does not change it as administrator";
pub const OTHER_DRIVE: &str = "It holds another mounted drive; not gone into";
pub const A_LINK: &str = "A link; its permissions and owner are not changed";
pub const LINK_COPY: &str = "It holds a link; Gezik does not copy links as administrator";
pub const SPECIAL: &str = "A device, pipe or socket; left alone";
pub const FOLDER_THERE: &str = "A folder is there; Gezik does not replace folders";
pub const TOO_DEEP: &str = "Folders nested too deep";
pub const STICKY_FILE: &str = "The sticky bit is for folders";
pub const CHANGED: &str = "It changed while Gezik was at it";
pub const NOT_A_FOLDER: &str = "Not a folder";
pub const SEVERAL_NAMES: &str = "It has several names; Gezik does not change it as administrator";
pub(crate) const NOT_PLAIN: &str = "Not a plain full path";
pub(crate) const BAD_NAME: &str = "Not a plain name";
/// Folders inside folders, at most (a deeper tree is refused, never half walked blindly).
pub(crate) const MAX_DEPTH: usize = 256;

pub(crate) fn refused(why: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, why)
}

/// A single name in a folder: no separator, NUL, `.` or `..` (Windows also no `:`, which names
/// a stream).
pub(crate) fn plain_name(name: &str) -> io::Result<&str> {
    let odd = |c: char| c == '/' || c == '\0' || (cfg!(windows) && (c == '\\' || c == ':'));
    if name.is_empty() || name == "." || name == ".." || name.contains(odd) {
        return Err(refused(BAD_NAME));
    }
    Ok(name)
}

/// The identities of the folders the helper never changes themselves
/// (`gezik_core::elevated::{unix,windows}_protected`): their spelling may differ (case, a short
/// name, a firmlink), their identity does not.
pub struct Guard(Vec<imp::Id>);

impl Guard {
    pub fn new(protected: &[PathBuf]) -> Guard {
        Guard(protected.iter().filter_map(|path| imp::id_of(path)).collect())
    }

    pub fn none() -> Guard {
        Guard(Vec::new())
    }

    pub(crate) fn check(&self, id: imp::Id) -> io::Result<()> {
        if self.0.contains(&id) { Err(refused(GUARDED)) } else { Ok(()) }
    }
}

/// Deletes `path` for good: a folder with everything in it; a link itself, never what it leads
/// to. A file with several names loses this one.
pub fn delete(path: &Path, guard: &Guard) -> io::Result<()> {
    imp::delete(path, guard)
}

/// Removes the empty folder `path`.
pub fn rmdir(path: &Path, guard: &Guard) -> io::Result<()> {
    imp::rmdir(path, guard)
}

/// Makes the folder `path` (its folder must be there; a name that is taken fails).
pub fn mkdir(path: &Path) -> io::Result<()> {
    imp::mkdir(path)
}

/// Gives `path` the name `name` in its folder; a taken name fails.
pub fn rename(path: &Path, name: &str, guard: &Guard) -> io::Result<()> {
    imp::rename(path, plain_name(name)?, guard)
}

/// Moves `from` to `to` (a rename, or a copy and a delete across drives); `replace`: a file at
/// `to` is replaced, a folder or a file with several names never.
pub fn move_to(from: &Path, to: &Path, replace: bool, guard: &Guard) -> io::Result<()> {
    imp::move_to(from, to, replace, guard)
}

/// Copies `from` to `to`: a folder is merged into one that is there; `replace`: files that are
/// there are replaced (through a temporary name), else each fails. If `to` was not there and the
/// copy fails, what it made is deleted.
pub fn copy(from: &Path, to: &Path, replace: bool) -> io::Result<()> {
    imp::copy(from, to, replace)
}

#[cfg(unix)]
pub fn chmod(path: &Path, mode: u32, guard: &Guard) -> io::Result<()> {
    imp::chmod(path, mode, guard)
}

#[cfg(unix)]
pub fn chown(path: &Path, uid: u32, gid: u32, guard: &Guard) -> io::Result<()> {
    imp::chown(path, uid, gid, guard)
}

#[cfg(target_os = "macos")]
pub fn chflags(path: &Path, set: u32, clear: u32, guard: &Guard) -> io::Result<()> {
    imp::chflags(path, set, clear, guard)
}

/// 32 lowercase hex digits from std's hasher keys, which come from the system's random source:
/// the result pipe's name and temporary names.
pub fn random_name() -> String {
    use std::hash::{BuildHasher, Hasher};
    let half = || {
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        hasher.write_u128(nanos);
        hasher.write_u32(std::process::id());
        hasher.finish()
    };
    format!("{:016x}{:016x}", half(), half())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A fresh folder under the temp folder, with its real spelling (macOS `/var` is a link to
    /// `/private/var`; a Windows TEMP may hold short names): what Gezik hands the helper.
    fn base(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gezik-secure-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let real = fs::canonicalize(&dir).unwrap();
        match real.to_str().and_then(|t| t.strip_prefix(r"\\?\")) {
            Some(plain) => PathBuf::from(plain),
            None => real,
        }
    }

    fn write(path: &Path, text: &str) {
        fs::write(path, text).unwrap();
    }

    /// A link at `at` to the folder `target`: a symlink, or on Windows a junction (no rights needed).
    fn link_dir(target: &Path, at: &Path) {
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, at).unwrap();
        #[cfg(windows)]
        crate::link::create(gezik_core::templates::LinkKind::Junction, target, at, true).unwrap();
    }

    #[test]
    fn delete_takes_a_whole_tree() {
        let dir = base("delete");
        fs::create_dir_all(dir.join("t/a/b")).unwrap();
        write(&dir.join("t/a/b/x.txt"), "x");
        write(&dir.join("t/y.txt"), "y");
        delete(&dir.join("t"), &Guard::none()).unwrap();
        assert!(!dir.join("t").exists());
        assert_eq!(delete(&dir.join("t"), &Guard::none()).unwrap_err().kind(), io::ErrorKind::NotFound);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_link_inside_is_removed_not_followed() {
        let dir = base("link-inside");
        fs::create_dir_all(dir.join("outside")).unwrap();
        write(&dir.join("outside/keep.txt"), "keep");
        fs::create_dir_all(dir.join("t")).unwrap();
        link_dir(&dir.join("outside"), &dir.join("t/link"));
        delete(&dir.join("t"), &Guard::none()).unwrap();
        assert!(!dir.join("t").exists());
        assert_eq!(fs::read_to_string(dir.join("outside/keep.txt")).unwrap(), "keep");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_link_on_the_way_is_refused() {
        let dir = base("link-on-the-way");
        fs::create_dir_all(dir.join("outside")).unwrap();
        write(&dir.join("outside/keep.txt"), "keep");
        link_dir(&dir.join("outside"), &dir.join("link"));
        let through = dir.join("link").join("keep.txt");
        for result in [
            delete(&through, &Guard::none()),
            rename(&through, "gone.txt", &Guard::none()),
            mkdir(&dir.join("link").join("new")),
            copy(&dir.join("outside/keep.txt"), &dir.join("link").join("copy.txt"), false),
            move_to(&dir.join("outside/keep.txt"), &dir.join("link").join("moved.txt"), false, &Guard::none()),
            move_to(&through, &dir.join("moved.txt"), false, &Guard::none()),
        ] {
            let err = result.unwrap_err();
            assert_eq!(err.to_string(), LINK_ON_THE_WAY, "{err:?}");
        }
        assert_eq!(fs::read_dir(dir.join("outside")).unwrap().count(), 1, "nothing changed behind the link");
        delete(&dir.join("link"), &Guard::none()).unwrap();
        assert!(dir.join("outside/keep.txt").exists(), "the link itself goes, not what it leads to");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_guarded_folder_is_refused_by_identity() {
        let dir = base("guard");
        fs::create_dir_all(dir.join("System").join("inside")).unwrap();
        let guard = Guard::new(&[dir.join("System")]);
        for result in [
            delete(&dir.join("System"), &guard),
            rmdir(&dir.join("System"), &guard),
            rename(&dir.join("System"), "Other", &guard),
            move_to(&dir.join("System"), &dir.join("Moved"), false, &guard),
        ] {
            assert_eq!(result.unwrap_err().to_string(), GUARDED);
        }
        if cfg!(any(windows, target_os = "macos")) {
            // Another spelling of the same folder on a case-blind disk.
            assert_eq!(delete(&dir.join("SYSTEM"), &guard).unwrap_err().to_string(), GUARDED);
        }
        // A guarded folder deep inside what is deleted stops the delete there.
        let guard_inner = Guard::new(&[dir.join("System").join("inside")]);
        assert_eq!(delete(&dir.join("System"), &guard_inner).unwrap_err().to_string(), GUARDED);
        assert!(dir.join("System").join("inside").is_dir());
        delete(&dir.join("System").join("inside"), &guard).unwrap();
        assert!(dir.join("System").is_dir(), "what is inside is free");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn names_that_are_taken_are_never_overwritten_unasked() {
        let dir = base("taken");
        write(&dir.join("a.txt"), "a");
        write(&dir.join("b.txt"), "b");
        let none = Guard::none();
        assert_eq!(rename(&dir.join("a.txt"), "b.txt", &none).unwrap_err().kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(
            move_to(&dir.join("a.txt"), &dir.join("b.txt"), false, &none).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(
            copy(&dir.join("a.txt"), &dir.join("b.txt"), false).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(fs::read_to_string(dir.join("b.txt")).unwrap(), "b");
        rename(&dir.join("a.txt"), "c.txt", &none).unwrap();
        assert_eq!(fs::read_to_string(dir.join("c.txt")).unwrap(), "a");
        move_to(&dir.join("c.txt"), &dir.join("b.txt"), true, &none).unwrap();
        assert_eq!(fs::read_to_string(dir.join("b.txt")).unwrap(), "a");
        assert!(!dir.join("c.txt").exists());
        fs::create_dir(dir.join("folder")).unwrap();
        assert_eq!(
            move_to(&dir.join("b.txt"), &dir.join("folder"), true, &none).unwrap_err().to_string(),
            FOLDER_THERE
        );
        assert_eq!(copy(&dir.join("b.txt"), &dir.join("folder"), true).unwrap_err().to_string(), FOLDER_THERE);
        // Into another folder: the rename goes by the target's full name.
        move_to(&dir.join("b.txt"), &dir.join("folder").join("b.txt"), false, &none).unwrap();
        assert_eq!(fs::read_to_string(dir.join("folder/b.txt")).unwrap(), "a");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_name_is_a_name() {
        let dir = base("names");
        fs::create_dir(dir.join("sub")).unwrap();
        write(&dir.join("a.txt"), "a");
        for name in ["", ".", "..", "../x", "sub/x", "sub\\x", "a\0b"] {
            assert!(rename(&dir.join("a.txt"), name, &Guard::none()).is_err(), "{name:?}");
        }
        assert!(dir.join("a.txt").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn several_names_are_dropped_but_never_changed() {
        let dir = base("hard-links");
        write(&dir.join("h"), "shared");
        fs::hard_link(dir.join("h"), dir.join("h2")).unwrap();
        write(&dir.join("x"), "x");
        let none = Guard::none();
        // Replacing changes what the other name holds in the user's eyes: refused.
        assert_eq!(copy(&dir.join("x"), &dir.join("h"), true).unwrap_err().to_string(), SEVERAL_NAMES);
        assert_eq!(move_to(&dir.join("x"), &dir.join("h"), true, &none).unwrap_err().to_string(), SEVERAL_NAMES);
        #[cfg(unix)]
        {
            assert_eq!(chmod(&dir.join("h"), 0o600, &none).unwrap_err().to_string(), SEVERAL_NAMES);
            assert_eq!(chown(&dir.join("h"), 0, 0, &none).unwrap_err().to_string(), SEVERAL_NAMES);
        }
        assert_eq!(fs::read_to_string(dir.join("h2")).unwrap(), "shared");
        // Dropping one name leaves the other as it was.
        rename(&dir.join("h"), "r", &none).unwrap();
        delete(&dir.join("r"), &none).unwrap();
        assert_eq!(fs::read_to_string(dir.join("h2")).unwrap(), "shared");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn copy_makes_merges_and_replaces() {
        let dir = base("copy");
        fs::create_dir_all(dir.join("src/sub")).unwrap();
        write(&dir.join("src/a.txt"), "new a");
        write(&dir.join("src/sub/b.txt"), "new b");
        copy(&dir.join("src"), &dir.join("dst"), false).unwrap();
        assert_eq!(fs::read_to_string(dir.join("dst/sub/b.txt")).unwrap(), "new b");
        write(&dir.join("dst/a.txt"), "old a");
        write(&dir.join("dst/only.txt"), "mine");
        assert_eq!(copy(&dir.join("src"), &dir.join("dst"), false).unwrap_err().kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read_to_string(dir.join("dst/a.txt")).unwrap(), "old a", "a file there fails, unasked");
        copy(&dir.join("src"), &dir.join("dst"), true).unwrap();
        assert_eq!(fs::read_to_string(dir.join("dst/a.txt")).unwrap(), "new a");
        assert_eq!(fs::read_to_string(dir.join("dst/only.txt")).unwrap(), "mine", "merged, not replaced");
        let leftovers = fs::read_dir(dir.join("dst"))
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with(".gezik-"))
            .count();
        assert_eq!(leftovers, 0, "no temporary name stays");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_copy_takes_back_what_it_made() {
        let dir = base("copy-rollback");
        fs::create_dir_all(dir.join("src/sub")).unwrap();
        fs::create_dir_all(dir.join("elsewhere")).unwrap();
        write(&dir.join("src/a.txt"), "a");
        // Windows refuses a link in what is copied; Unix copies a link but not a FIFO.
        #[cfg(windows)]
        link_dir(&dir.join("elsewhere"), &dir.join("src/sub/link"));
        #[cfg(unix)]
        {
            let fifo = std::ffi::CString::new(dir.join("src/sub/fifo").to_str().unwrap()).unwrap();
            // SAFETY: a valid C string.
            assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        }
        let err = copy(&dir.join("src"), &dir.join("dst"), false).unwrap_err();
        assert!([LINK_COPY, SPECIAL].contains(&err.to_string().as_str()), "{err}");
        assert!(!dir.join("dst").exists(), "the half-made copy is gone");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn folders_come_and_go_only_empty() {
        let dir = base("dirs");
        mkdir(&dir.join("n")).unwrap();
        assert_eq!(mkdir(&dir.join("n")).unwrap_err().kind(), io::ErrorKind::AlreadyExists);
        write(&dir.join("n/x"), "x");
        assert!(rmdir(&dir.join("n"), &Guard::none()).is_err(), "not empty");
        assert_eq!(rmdir(&dir.join("n").join("x"), &Guard::none()).unwrap_err().to_string(), NOT_A_FOLDER);
        fs::remove_file(dir.join("n/x")).unwrap();
        rmdir(&dir.join("n"), &Guard::none()).unwrap();
        assert!(!dir.join("n").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_relative_or_odd_path_is_refused() {
        assert!(delete(Path::new("relative/x"), &Guard::none()).is_err());
        assert!(mkdir(Path::new("/")).is_err());
        let dir = base("odd");
        fs::create_dir(dir.join("a")).unwrap();
        assert!(mkdir(&dir.join("a/../b")).is_err());
        assert!(!dir.join("b").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn random_names_differ() {
        let (a, b) = (random_name(), random_name());
        assert_ne!(a, b);
        assert!(a.len() == 32 && a.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f')), "{a}");
    }

    #[cfg(windows)]
    #[test]
    fn a_read_only_file_is_deleted() {
        let dir = base("read-only");
        write(&dir.join("r.txt"), "r");
        let mut perms = fs::metadata(dir.join("r.txt")).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(dir.join("r.txt"), perms).unwrap();
        delete(&dir.join("r.txt"), &Guard::none()).unwrap();
        assert!(!dir.join("r.txt").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn short_names_and_network_paths_are_refused() {
        let dir = base("short");
        fs::create_dir(dir.join("a long folder name")).unwrap();
        write(&dir.join("a long folder name/x.txt"), "x");
        // Not every volume makes 8.3 names; where one is there it must not be followed.
        let long = windows::core::HSTRING::from(dir.join("a long folder name").as_os_str());
        let mut buffer = [0u16; 1024];
        // SAFETY: the buffer holds its length.
        let len = unsafe { windows::Win32::Storage::FileSystem::GetShortPathNameW(&long, Some(&mut buffer)) } as usize;
        let short = String::from_utf16_lossy(&buffer[..len.min(buffer.len())]);
        if !short.is_empty() && !short.eq_ignore_ascii_case(&long.to_string_lossy()) {
            let err = delete(&Path::new(&short).join("x.txt"), &Guard::none()).unwrap_err();
            assert_eq!(err.to_string(), LINK_ON_THE_WAY, "{short}");
            assert!(dir.join("a long folder name/x.txt").exists());
        }
        assert!(delete(Path::new(r"\\localhost\c$\nothing-here"), &Guard::none()).is_err());
        assert!(mkdir(Path::new(r"\\?\C:\gezik-not-made")).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn copies_keep_links_and_drop_setuid() {
        use std::os::unix::fs::PermissionsExt;
        let dir = base("unix-copy");
        fs::create_dir_all(dir.join("App.app/Versions/A")).unwrap();
        write(&dir.join("App.app/Versions/A/tool"), "#!/bin/sh\n");
        std::os::unix::fs::symlink("A", dir.join("App.app/Versions/Current")).unwrap();
        fs::set_permissions(dir.join("App.app/Versions/A/tool"), fs::Permissions::from_mode(0o4755)).unwrap();
        copy(&dir.join("App.app"), &dir.join("Copy.app"), false).unwrap();
        assert_eq!(
            fs::read_link(dir.join("Copy.app/Versions/Current")).unwrap(),
            Path::new("A"),
            "a link stays a link"
        );
        let mode = fs::metadata(dir.join("Copy.app/Versions/A/tool")).unwrap().permissions().mode() & 0o7777;
        assert_eq!(mode, 0o755, "setuid does not travel");
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn permissions_and_owners_never_go_through_a_link() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let dir = base("unix-attrs");
        write(&dir.join("f"), "f");
        fs::create_dir(dir.join("d")).unwrap();
        std::os::unix::fs::symlink(dir.join("f"), dir.join("l")).unwrap();
        let none = Guard::none();
        assert_eq!(chmod(&dir.join("l"), 0o600, &none).unwrap_err().to_string(), A_LINK);
        assert_eq!(chown(&dir.join("l"), 0, 0, &none).unwrap_err().to_string(), A_LINK);
        assert_eq!(chmod(&dir.join("f"), 0o1644, &none).unwrap_err().to_string(), STICKY_FILE);
        chmod(&dir.join("f"), 0o640, &none).unwrap();
        chmod(&dir.join("d"), 0o1775, &none).unwrap();
        assert_eq!(fs::metadata(dir.join("f")).unwrap().permissions().mode() & 0o7777, 0o640);
        assert_eq!(fs::metadata(dir.join("d")).unwrap().permissions().mode() & 0o7777, 0o1775);
        let meta = fs::metadata(dir.join("f")).unwrap();
        chown(&dir.join("f"), meta.uid(), meta.gid(), &none).unwrap();
        #[cfg(target_os = "macos")]
        {
            use std::os::macos::fs::MetadataExt as _;
            chflags(&dir.join("f"), gezik_core::attrs::HIDDEN, 0, &none).unwrap();
            assert_ne!(fs::symlink_metadata(dir.join("f")).unwrap().st_flags() & gezik_core::attrs::HIDDEN, 0);
            chflags(&dir.join("f"), 0, gezik_core::attrs::HIDDEN, &none).unwrap();
        }
        let _ = fs::remove_dir_all(&dir);
    }
}
