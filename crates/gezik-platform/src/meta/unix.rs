//! Linux: freedesktop's and Baloo's attributes (spec 12 §4.3). macOS: Finder's own tags,
//! FinderInfo label colour, star rating and comment (§4.1). Links are read and written
//! themselves, never what they point to.

use std::ffi::CString;
use std::fs::File;
use std::io;
use std::os::unix::ffi::OsStrExt;

use std::path::Path;

use gezik_core::meta::Meta;

use super::{Own, Volume};

fn c_path(path: &Path) -> io::Result<CString> {
    CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "a NUL in the path"))
}

fn c_name(name: &str) -> CString {
    CString::new(name).unwrap_or_default()
}

#[cfg(target_os = "linux")]
const NO_ATTR: i32 = libc::ENODATA;
#[cfg(target_os = "macos")]
const NO_ATTR: i32 = libc::ENOATTR;

/// The attribute `name` of `path` itself; `None` if it has none.
fn get(path: &CString, name: &str) -> io::Result<Option<Vec<u8>>> {
    let name = c_name(name);
    let mut buffer = vec![0u8; 256];
    loop {
        // SAFETY: valid C strings and a buffer of the length given.
        #[cfg(target_os = "linux")]
        let n = unsafe { libc::lgetxattr(path.as_ptr(), name.as_ptr(), buffer.as_mut_ptr().cast(), buffer.len()) };
        #[cfg(target_os = "macos")]
        let n = unsafe {
            libc::getxattr(
                path.as_ptr(),
                name.as_ptr(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                0,
                libc::XATTR_NOFOLLOW,
            )
        };
        if n >= 0 {
            buffer.truncate(n as usize);
            return Ok(Some(buffer));
        }
        let err = io::Error::last_os_error();
        match err.raw_os_error() {
            // A file system without attributes (vfat, exFAT, NFS) has none to read.
            Some(code) if code == NO_ATTR || code == libc::ENOTSUP || code == libc::EOPNOTSUPP => return Ok(None),
            // Bigger than the buffer: ask its size and try once more (it may grow meanwhile).
            Some(libc::ERANGE) if buffer.len() < 1 << 20 => buffer.resize(buffer.len() * 16, 0),
            _ => return Err(err),
        }
    }
}

/// Sets (or with `None` removes) the attribute `name` of `path` itself.
fn set(path: &CString, name: &str, value: Option<&[u8]>) -> io::Result<()> {
    let name = c_name(name);
    // SAFETY: valid C strings and a value of the length given.
    let done = match value {
        #[cfg(target_os = "linux")]
        Some(value) => unsafe { libc::lsetxattr(path.as_ptr(), name.as_ptr(), value.as_ptr().cast(), value.len(), 0) },
        #[cfg(target_os = "macos")]
        Some(value) => unsafe {
            libc::setxattr(path.as_ptr(), name.as_ptr(), value.as_ptr().cast(), value.len(), 0, libc::XATTR_NOFOLLOW)
        },
        #[cfg(target_os = "linux")]
        None => unsafe { libc::lremovexattr(path.as_ptr(), name.as_ptr()) },
        #[cfg(target_os = "macos")]
        None => unsafe { libc::removexattr(path.as_ptr(), name.as_ptr(), libc::XATTR_NOFOLLOW) },
    };
    if done == 0 {
        return Ok(());
    }
    let err = io::Error::last_os_error();
    if value.is_none() && err.raw_os_error() == Some(NO_ATTR) { Ok(()) } else { Err(err) }
}

/// A file that is not there has no record to read or write.
fn existing(path: &Path) -> io::Result<CString> {
    std::fs::symlink_metadata(path)?;
    c_path(path)
}

#[cfg(target_os = "linux")]
mod names {
    pub const TAGS: &str = "user.xdg.tags";
    pub const RATING: &str = "user.baloo.rating";
    pub const COMMENT: &str = "user.xdg.comment";
}

#[cfg(target_os = "linux")]
pub fn read(path: &Path) -> io::Result<Own> {
    use gezik_core::meta::{Tag, TagColor, decode_baloo_rating, decode_xdg_tags};
    let path = existing(path)?;
    let text = |name| get(&path, name).map(|value| value.map(|v| String::from_utf8_lossy(&v).into_owned()));
    let mut meta = Meta::default();
    if let Some(tags) = text(names::TAGS)? {
        // Colours come from the user's list (spec 12 §4.3).
        meta.tags = decode_xdg_tags(&tags).iter().map(|name| Tag::new(name, TagColor::None)).collect();
    }
    meta.rating = text(names::RATING)?.map_or(0, |rating| decode_baloo_rating(&rating));
    if let Some(comment) = text(names::COMMENT)? {
        meta.apply(&gezik_core::meta::MetaEdit::Comment(comment.into()));
    }
    Ok(Own { meta, ..Own::none() })
}

#[cfg(target_os = "linux")]
pub fn write(path: &Path, meta: &Meta) -> io::Result<()> {
    use gezik_core::meta::{decode_baloo_rating, encode_baloo_rating, encode_xdg_tags};
    let path = existing(path)?;
    let tags = (!meta.tags.is_empty()).then(|| encode_xdg_tags(&meta.tags).into_bytes());
    let comment = (!meta.comment.is_empty()).then(|| meta.comment.as_bytes().to_vec());
    let mut changes = vec![(names::TAGS, tags), (names::COMMENT, comment)];
    // Baloo keeps half stars: a 7 Dolphin wrote stays 7 unless the stars change.
    let now = get(&path, names::RATING)?.map_or(0, |v| decode_baloo_rating(&String::from_utf8_lossy(&v)));
    if now != meta.rating {
        changes.push((names::RATING, (meta.rating > 0).then(|| encode_baloo_rating(meta.rating).into_bytes())));
    }
    super::set_all(changes, |name| get(&path, name), |name, value| set(&path, name, value))
}

#[cfg(target_os = "macos")]
mod names {
    pub const TAGS: &str = "com.apple.metadata:_kMDItemUserTags";
    pub const FINDER_INFO: &str = "com.apple.FinderInfo";
    pub const RATING: &str = "com.apple.metadata:kMDItemStarRating";
    pub const COMMENT: &str = "com.apple.metadata:kMDItemFinderComment";
}

#[cfg(target_os = "macos")]
pub fn read(path: &Path) -> io::Result<Own> {
    use gezik_core::meta::bplist::{Value, decode};
    use gezik_core::meta::{Tag, TagColor, finder_info_color};
    let path = existing(path)?;
    let mut own = Own::none();
    match get(&path, names::TAGS)?.map(|data| decode(&data)) {
        Some(Some(Value::Strings(strings))) => {
            for text in strings {
                // `Name` or `Name\nN`, N Finder's colour number.
                let (name, color) = match text.rsplit_once('\n') {
                    Some((name, n)) => (name, n.parse().map(TagColor::from_number).unwrap_or_default()),
                    None => (text.as_str(), TagColor::None),
                };
                if !name.is_empty() && !own.meta.has_tag(name) {
                    own.meta.tags.push(Tag::new(name, color));
                }
            }
        }
        Some(_) => own.writable = false,
        // No tags list: a FinderInfo label alone is a tag of its colour's name (as in Finder).
        None => {
            let color = get(&path, names::FINDER_INFO)?.map_or(TagColor::None, |info| finder_info_color(&info));
            if color != TagColor::None {
                own.meta.tags.push(Tag::new(color.default_name(), color));
            }
        }
    }
    match get(&path, names::RATING)?.map(|data| decode(&data)) {
        Some(Some(Value::Int(stars))) => own.meta.rating = stars.clamp(0, 5) as u8,
        Some(_) => own.writable = false,
        None => {}
    }
    match get(&path, names::COMMENT)?.map(|data| decode(&data)) {
        Some(Some(Value::Str(comment))) => own.meta.apply(&gezik_core::meta::MetaEdit::Comment(comment.into())),
        Some(_) => own.writable = false,
        None => {}
    }
    Ok(own)
}

#[cfg(target_os = "macos")]
pub fn write(path: &Path, meta: &Meta) -> io::Result<()> {
    use gezik_core::meta::bplist::{encode_int, encode_string, encode_strings};
    use gezik_core::meta::{TagColor, label_color, with_finder_info_color};
    if !read(path)?.writable {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "A tag record Gezik does not know; left alone"));
    }
    let path = existing(path)?;
    let strings: Vec<String> = meta
        .tags
        .iter()
        .map(|tag| match tag.color {
            TagColor::None => tag.name.to_string(),
            color => format!("{}\n{}", tag.name, color.number()),
        })
        .collect();
    let tags = (!strings.is_empty()).then(|| encode_strings(&strings));
    // Finder keeps the last coloured tag's colour in the FinderInfo too; its other bytes stay.
    let info = get(&path, names::FINDER_INFO)?;
    let new_info = with_finder_info_color(info.as_deref(), label_color(&meta.tags));
    let comment = (!meta.comment.is_empty()).then(|| encode_string(&meta.comment));
    let mut changes = vec![(names::TAGS, tags), (names::FINDER_INFO, new_info), (names::COMMENT, comment)];
    // Left as it is unless the stars change.
    let now = match get(&path, names::RATING)?.map(|data| gezik_core::meta::bplist::decode(&data)) {
        Some(Some(gezik_core::meta::bplist::Value::Int(stars))) => stars,
        _ => 0,
    };
    if now != i64::from(meta.rating) {
        changes.push((names::RATING, (meta.rating > 0).then(|| encode_int(i64::from(meta.rating.min(5))))));
    }
    super::set_all(changes, |name| get(&path, name), |name, value| set(&path, name, value))
}

/// File systems whose files hold no extended attributes Gezik can use (statfs's f_type).
#[cfg(target_os = "linux")]
const NO_ATTRS: [u32; 9] = [
    0x4d44,      // MSDOS (FAT)
    0x2011_bab0, // exFAT
    0x9660,      // ISO 9660
    0x1513_3125, // UDF
    0x6969,      // NFS
    0x517b,      // SMB
    0xff53_4d42, // CIFS
    0xfe53_4d42, // SMB2
    0x7371_7368, // squashfs
];

/// The volume whose root (mount point) is `root`.
pub fn volume(root: &Path) -> io::Result<Volume> {
    let c_root = c_path(root)?;
    let mut vfs: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: a valid C string and a statvfs to fill.
    if unsafe { libc::statvfs(c_root.as_ptr(), &mut vfs) } != 0 {
        return Err(io::Error::last_os_error());
    }
    #[allow(clippy::unnecessary_cast)]
    let read_only = vfs.f_flag as u64 & libc::ST_RDONLY as u64 != 0;
    let mut fs: libc::statfs = unsafe { std::mem::zeroed() };
    // SAFETY: as above.
    if unsafe { libc::statfs(c_root.as_ptr(), &mut fs) } != 0 {
        return Err(io::Error::last_os_error());
    }
    #[cfg(target_os = "linux")]
    #[allow(clippy::unnecessary_cast, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let holds = !read_only && !NO_ATTRS.contains(&(fs.f_type as u32));
    // macOS writes attributes FAT and exFAT cannot hold into `._` files itself.
    #[cfg(target_os = "macos")]
    let holds = !read_only;
    Ok(Volume { key: volume_key(root, &fs), holds })
}

/// The file system's UUID (`/dev/disk/by-uuid`), else its mount source.
#[cfg(target_os = "linux")]
fn volume_key(root: &Path, _fs: &libc::statfs) -> String {
    use std::os::unix::fs::MetadataExt;
    let dev = std::fs::metadata(root).map(|meta| meta.dev()).unwrap_or(0);
    if let Ok(entries) = std::fs::read_dir("/dev/disk/by-uuid") {
        for entry in entries.flatten() {
            if std::fs::metadata(entry.path()).is_ok_and(|meta| meta.rdev() == dev) {
                return entry.file_name().to_string_lossy().into_owned();
            }
        }
    }
    let source = std::fs::read_to_string("/proc/self/mountinfo").ok().and_then(|text| mount_source(&text, root));
    match source {
        Some(source) if source.contains('/') => source,
        Some(source) => format!("{source}@{}", root.display()),
        None => root.display().to_string(),
    }
}

/// The source of the last mount at `root` in a mountinfo text.
#[cfg(target_os = "linux")]
fn mount_source(mountinfo: &str, root: &Path) -> Option<String> {
    let unescape = |text: &str| text.replace("\\040", " ").replace("\\011", "\t").replace("\\134", "\\");
    mountinfo
        .lines()
        .filter_map(|line| {
            let (left, right) = line.split_once(" - ")?;
            let point = left.split(' ').nth(4)?;
            let source = right.split(' ').nth(1)?;
            (Path::new(&unescape(point)) == root).then(|| unescape(source))
        })
        .next_back()
}

/// The volume's UUID, else what it is mounted from (`//user@server/share`).
#[cfg(target_os = "macos")]
fn volume_key(root: &Path, fs: &libc::statfs) -> String {
    #[repr(C)]
    struct AttrList {
        bitmap_count: u16,
        reserved: u16,
        common: u32,
        volume: u32,
        dir: u32,
        file: u32,
        fork: u32,
    }
    #[repr(C, packed(4))]
    struct Answer {
        length: u32,
        uuid: [u8; 16],
    }
    unsafe extern "C" {
        fn getattrlist(
            path: *const libc::c_char,
            list: *mut AttrList,
            buffer: *mut libc::c_void,
            size: libc::size_t,
            options: u32,
        ) -> libc::c_int;
    }
    const ATTR_VOL_INFO: u32 = 0x8000_0000;
    const ATTR_VOL_UUID: u32 = 0x0004_0000;
    if let Ok(c_root) = c_path(root) {
        let mut list = AttrList {
            bitmap_count: 5,
            reserved: 0,
            common: 0,
            volume: ATTR_VOL_INFO | ATTR_VOL_UUID,
            dir: 0,
            file: 0,
            fork: 0,
        };
        let mut answer = Answer { length: 0, uuid: [0; 16] };
        // SAFETY: a valid C string, the list and a buffer of the size given.
        let done = unsafe {
            getattrlist(c_root.as_ptr(), &mut list, (&mut answer as *mut Answer).cast(), size_of::<Answer>(), 0)
        };
        let uuid = answer.uuid;
        if done == 0 && uuid.iter().any(|&b| b != 0) {
            return uuid.iter().map(|b| format!("{b:02x}")).collect();
        }
    }
    // SAFETY: f_mntfromname is a NUL-terminated string the system filled.
    let from = unsafe { std::ffi::CStr::from_ptr(fs.f_mntfromname.as_ptr()) };
    from.to_string_lossy().into_owned()
}

/// After a copy: the source's extended attributes onto the copy. Linux: every `user.*` one
/// (copy_file_range and FICLONE take none), at most 64 KB in all; macOS: all of them
/// (`fcopyfile`). A target that refuses them leaves the copy as it is.
pub(crate) fn copy_xattrs(from: &File, to: &File) {
    use std::os::fd::AsRawFd;
    #[cfg(target_os = "linux")]
    {
        const MAX_TOTAL: usize = 64 * 1024;
        let (from, to) = (from.as_raw_fd(), to.as_raw_fd());
        let mut names = vec![0u8; 4096];
        // SAFETY: an open descriptor and a buffer of the length given.
        let n = unsafe { libc::flistxattr(from, names.as_mut_ptr().cast(), names.len()) };
        if n <= 0 {
            return;
        }
        names.truncate(n as usize);
        let mut total = 0usize;
        let mut value = vec![0u8; MAX_TOTAL];
        for name in names.split(|&b| b == 0).filter(|name| name.starts_with(b"user.")) {
            let Ok(name) = CString::new(name) else { continue };
            // SAFETY: as above.
            let len = unsafe { libc::fgetxattr(from, name.as_ptr(), value.as_mut_ptr().cast(), value.len() - total) };
            if len < 0 {
                continue;
            }
            total += len as usize;
            // SAFETY: as above.
            let done = unsafe { libc::fsetxattr(to, name.as_ptr(), value.as_ptr().cast(), len as usize, 0) };
            if done != 0 || total >= MAX_TOTAL {
                return;
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        unsafe extern "C" {
            fn fcopyfile(from: libc::c_int, to: libc::c_int, state: *mut libc::c_void, flags: u32) -> libc::c_int;
        }
        const COPYFILE_XATTR: u32 = 1 << 2;
        // SAFETY: two open descriptors, no state. Extended attributes only: the data is copied.
        let _ = unsafe { fcopyfile(from.as_raw_fd(), to.as_raw_fd(), std::ptr::null_mut(), COPYFILE_XATTR) };
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn a_mount_source_is_read_from_mountinfo() {
        let text = "36 35 98:0 / /mnt/my\\040stick rw,noatime - vfat /dev/sdb1 rw\n\
                    37 35 0:5 / /tmp rw - tmpfs tmpfs rw\n";
        assert_eq!(mount_source(text, Path::new("/mnt/my stick")).as_deref(), Some("/dev/sdb1"));
        assert_eq!(mount_source(text, Path::new("/tmp")).as_deref(), Some("tmpfs"));
        assert_eq!(mount_source(text, Path::new("/x")), None);
    }
}
