//! Reading archives into a staging folder: zip, 7z, rar, the tar family, single compressed
//! files (`.gz .xz .bz2 .zst`), cab, iso, cpio and ar/deb, with passwords, volumes, cancel
//! and size limits. Every entry name goes through `safe_join`; nothing is written outside
//! the staging folder. Writing zip, 7z and tar archives is in `write`.

mod ar;
mod cab;
mod cpio;
pub mod io;
mod iso;
#[cfg(feature = "rar")]
mod rar;
mod sevenz;
mod single;
mod tar;
pub mod write;
mod zip;

use std::fs::{self, File};
use std::io::{BufReader, ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gezik_core::batch::archive::{Format, VolumeKind, detect, safe_join, volume_name, volume_set};
use gezik_core::batch::date::DateParts;

use self::io::MultiFileReader;

type IoResult<T> = std::io::Result<T>;
type IoError = std::io::Error;

/// One entry of an archive, as listed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
    pub size: Option<u64>,
    pub packed: Option<u64>,
    pub encrypted: bool,
}

/// What extraction may use: progress, cancel, the password question.
pub trait ExtractCx {
    /// Bytes written so far for the current entry grew by `n`.
    fn add_bytes(&self, n: u64);
    /// An entry finished.
    fn entry_done(&self);
    /// True once the job is cancelled (stop at the next chance).
    fn stopped(&self) -> bool;
    /// The archive needs a password (`retry`: the last one was wrong); `None`: skip it.
    fn password(&self, retry: bool) -> Option<String>;
    /// An entry was skipped or failed; the rest goes on.
    fn entry_failed(&self, name: &str, error: &IoError);
    /// An entry (or the whole archive) was left out on purpose: no password was given, or it
    /// is a link Gezik does not make. Where nothing may be lost, it is a failure.
    fn entry_skipped(&self, name: &str, why: &IoError) {
        self.entry_failed(name, why);
    }
    /// What the header of the entry written at `path` says besides its data (an archive
    /// packed again keeps it; Windows itself has no place for a Unix mode or owner).
    fn entry_header(&self, _path: &Path, _header: &Header) {}
}

/// An entry's Unix facts as its archive keeps them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Header {
    /// Permission bits (`0o7777` at most).
    pub mode: Option<u32>,
    pub modified: Option<SystemTime>,
    /// tar only.
    pub owner: Option<write::Owner>,
}

pub trait ArchiveSource {
    /// Entries with known sizes (None for streams: tar.*, .gz…). May ask for a password
    /// (header-encrypted 7z, rar -hp).
    fn list(&mut self, cx: &dyn ExtractCx) -> IoResult<Option<Vec<Entry>>>;
    /// Writes every entry under `dest` (a staging folder), through `safe_join`.
    fn extract(&mut self, dest: &Path, cx: &dyn ExtractCx) -> IoResult<()>;
}

/// How much of the start of a file `detect` looks at.
const HEAD: u64 = 0x9010;

/// The copy buffer.
const BUF: usize = 64 * 1024;

/// The archive at `path` (any volume of a set opens the first), by its first bytes and name.
pub fn open(path: &Path) -> IoResult<Box<dyn ArchiveSource + Send>> {
    let volumes = Volumes::of(path)?;
    let mut head = Vec::with_capacity(HEAD as usize);
    MultiFileReader::open(&volumes.paths)?.take(HEAD).read_to_end(&mut head)?;
    let format =
        detect(&head, &volumes.name).ok_or_else(|| IoError::new(ErrorKind::InvalidData, "not a known archive"))?;
    Ok(match format {
        Format::Zip => Box::new(zip::ZipSource::new(volumes)),
        Format::SevenZ => Box::new(sevenz::SevenZSource::open(volumes)?),
        Format::Tar(codec) => Box::new(tar::TarSource::new(volumes, codec)),
        Format::Single(codec) => Box::new(single::SingleSource::new(volumes, codec)),
        #[cfg(feature = "rar")]
        Format::Rar => Box::new(rar::RarSource::open(volumes)?),
        #[cfg(not(feature = "rar"))]
        Format::Rar => return Err(seven_zip_needed()),
        Format::Cab => Box::new(cab::CabSource::open(volumes)?),
        Format::Iso => Box::new(iso::IsoSource::new(volumes)),
        Format::Cpio => Box::new(cpio::CpioSource::open(volumes)?),
        Format::Ar => Box::new(ar::ArSource::new(volumes, false)),
        Format::Deb => Box::new(ar::ArSource::new(volumes, true)),
        Format::Udf | Format::Other(_) => return Err(seven_zip_needed()),
    })
}

/// Whether Gezik itself can open it (false: 7-Zip is needed).
pub fn supported(format: &Format) -> bool {
    // RAR needs the `rar` feature (UnRAR's C++).
    !matches!(format, Format::Udf | Format::Other(_)) && (cfg!(feature = "rar") || *format != Format::Rar)
}

/// The error of an archive only 7-Zip reads.
fn seven_zip_needed() -> IoError {
    IoError::new(ErrorKind::Unsupported, "7-Zip needed")
}

/// The files an archive is made of, and the name it goes by (`a.7z` for `a.7z.002`).
struct Volumes {
    paths: Vec<PathBuf>,
    name: String,
}

impl Volumes {
    fn of(path: &Path) -> IoResult<Volumes> {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if let Some(set) = volume_set(&name)
            && let VolumeKind::Numbered { .. } = set.kind
        {
            if !path.with_file_name(&set.first).is_file() {
                let message = format!("the first part ({}) is missing", set.first);
                return Err(IoError::new(ErrorKind::NotFound, message));
            }
            let paths: Vec<PathBuf> = (1..)
                .map(|n| path.with_file_name(volume_name(&set.base, set.kind, n)))
                .take_while(|p| p.is_file())
                .collect();
            // A later part beside a missing one: the set has a hole.
            if last_volume(path, &set.base) > paths.len() as u64 {
                let missing = volume_name(&set.base, set.kind, paths.len() as u32 + 1);
                return Err(IoError::new(ErrorKind::NotFound, format!("part {missing} is missing")));
            }
            return Ok(Volumes { paths, name: set.base });
        }
        Ok(Volumes { paths: vec![path.to_path_buf()], name })
    }

    /// The volumes as one buffered stream.
    fn reader(&self) -> IoResult<BufReader<MultiFileReader>> {
        Ok(BufReader::with_capacity(BUF, MultiFileReader::open(&self.paths)?))
    }

    /// Reports that the archive was skipped for want of a password.
    fn no_password(&self, cx: &dyn ExtractCx) {
        cx.entry_skipped(&self.name, &IoError::new(ErrorKind::PermissionDenied, "no password"));
    }
}

/// The highest number among the numbered volumes `base.NNN` beside `path` (0 if none).
fn last_volume(path: &Path, base: &str) -> u64 {
    let folder = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let Ok(listing) = fs::read_dir(folder) else { return 0 };
    let prefix = format!("{base}.");
    listing
        .filter_map(|entry| {
            let name = entry.ok()?.file_name().into_string().ok()?;
            let digits = name.strip_prefix(&prefix)?;
            (digits.len() >= 3 && digits.bytes().all(|b| b.is_ascii_digit())).then(|| digits.parse().ok()).flatten()
        })
        .max()
        .unwrap_or(0)
}

/// What an entry's file gets besides its bytes.
#[derive(Debug, Default)]
struct Meta {
    modified: Option<SystemTime>,
    /// Unix permission bits (applied on Unix only, masked with 0o777).
    #[cfg_attr(not(unix), allow(dead_code))]
    mode: Option<u32>,
    /// DOS attributes: read-only (1) and hidden (2) are applied on Windows.
    #[cfg_attr(not(windows), allow(dead_code))]
    attributes: Option<u32>,
}

/// Why an entry stopped.
#[derive(Debug)]
enum Stop {
    /// The job was cancelled.
    Cancelled,
    /// The archive's data is bad here (CRC, cut short, larger than it says, wrong password).
    Read(IoError),
    /// Only this entry: its name is unsafe, or writing it failed.
    Skip(IoError),
    /// Only this entry, left out on purpose (a link on Windows).
    Note(IoError),
}

fn cancelled() -> IoError {
    IoError::new(ErrorKind::Interrupted, "cancelled")
}

fn unsafe_path() -> IoError {
    IoError::new(ErrorKind::InvalidData, "unsafe path; skipped")
}

/// The error a stream format ends with once its data is bad.
fn damaged(error: IoError) -> IoError {
    if error.kind() == ErrorKind::Interrupted {
        return error;
    }
    IoError::new(ErrorKind::InvalidData, format!("the archive is damaged: {error}"))
}

/// Writes `r` into a new file at `path` (its folders made first) with progress, cancel and
/// the size limit, then gives it `meta`. A file left unfinished is removed.
fn write_file(
    r: &mut dyn Read,
    path: &Path,
    declared: Option<u64>,
    meta: &Meta,
    cx: &dyn ExtractCx,
) -> Result<(), Stop> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(Stop::Skip)?;
    }
    remove_earlier(path).map_err(Stop::Skip)?;
    let mut file = File::create(path).map_err(Stop::Skip)?;
    let result = copy(r, &mut file, declared, cx);
    if result.is_ok()
        && let Some(time) = meta.modified
    {
        let _ = file.set_modified(time);
    }
    drop(file);
    if result.is_err() {
        let _ = fs::remove_file(path);
        return result;
    }
    apply_meta(path, meta);
    Ok(())
}

/// Removes a file an earlier entry of the same name wrote (the last one wins), read-only or
/// hidden as it may be. A folder stays (the file then cannot be made).
fn remove_earlier(path: &Path) -> IoResult<()> {
    let Ok(metadata) = path.symlink_metadata() else { return Ok(()) };
    if metadata.is_dir() {
        return Ok(());
    }
    #[cfg(windows)]
    {
        let _ = gezik_platform::fs::clear_hidden(path);
        let mut permissions = metadata.permissions();
        if permissions.readonly() {
            #[allow(clippy::permissions_set_readonly_false)]
            permissions.set_readonly(false);
            fs::set_permissions(path, permissions)?;
        }
    }
    fs::remove_file(path)
}

/// Copies in 64 KB pieces, stopping when cancelled or past `declared * 1.1 + 1 MiB`; less
/// than `declared` means the data was cut short.
fn copy(r: &mut dyn Read, w: &mut dyn Write, declared: Option<u64>, cx: &dyn ExtractCx) -> Result<(), Stop> {
    let limit = declared.map(|size| size.saturating_add(size / 10).saturating_add(1 << 20));
    let mut buf = vec![0u8; BUF];
    let mut written = 0u64;
    loop {
        if cx.stopped() {
            return Err(Stop::Cancelled);
        }
        let n = match r.read(&mut buf) {
            Ok(0) if declared.is_some_and(|size| written < size) => {
                return Err(Stop::Read(IoError::new(ErrorKind::UnexpectedEof, "cut short")));
            }
            Ok(0) => return Ok(()),
            Ok(n) => n,
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) => return Err(Stop::Read(e)),
        };
        written += n as u64;
        if limit.is_some_and(|limit| written > limit) {
            return Err(Stop::Read(IoError::new(ErrorKind::InvalidData, "larger than the archive says; stopped")));
        }
        w.write_all(&buf[..n]).map_err(Stop::Skip)?;
        cx.add_bytes(n as u64);
    }
}

fn apply_meta(path: &Path, meta: &Meta) {
    #[cfg(unix)]
    if let Some(mode) = meta.mode {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(mode & 0o777));
    }
    #[cfg(windows)]
    if let Some(attributes) = meta.attributes {
        if attributes & 2 != 0 {
            let _ = gezik_platform::fs::set_hidden(path);
        }
        if attributes & 1 != 0
            && let Ok(metadata) = fs::metadata(path)
        {
            let mut permissions = metadata.permissions();
            permissions.set_readonly(true);
            let _ = fs::set_permissions(path, permissions);
        }
    }
    #[cfg(not(any(unix, windows)))]
    let _ = (path, meta);
}

/// The DOS attributes of zip's and 7z's attribute word (DOS in the low byte, a Unix mode in
/// the high half): a Unix mode without the owner's write bit is read-only too.
fn dos_attributes(raw: u32) -> u32 {
    let mode = raw >> 16;
    let read_only = mode != 0 && mode & 0o200 == 0;
    (raw & 0xFF) | u32::from(read_only)
}

/// Whether an entry names the archive's own top folder (`.`, `./`): nothing to make.
fn is_top(name: &str) -> bool {
    Path::new(&name.replace('\\', "/")).components().all(|c| c == std::path::Component::CurDir)
}

/// Makes a folder entry.
fn make_dir(path: &Path) -> Result<(), Stop> {
    fs::create_dir_all(path).map_err(Stop::Skip)
}

/// Reports how one entry went; only a cancel stops the archive.
fn report(name: &str, result: Result<bool, Stop>, cx: &dyn ExtractCx) -> IoResult<()> {
    match result {
        Ok(true) => cx.entry_done(),
        // A link, made at the end.
        Ok(false) => {}
        Err(Stop::Cancelled) => return Err(cancelled()),
        Err(Stop::Read(e) | Stop::Skip(e)) => cx.entry_failed(name, &e),
        Err(Stop::Note(e)) => cx.entry_skipped(name, &e),
    }
    Ok(())
}

/// Like `report`, for a stream: once the data is bad nothing after it can be read.
fn report_stream(name: &str, result: Result<bool, Stop>, cx: &dyn ExtractCx) -> IoResult<()> {
    if let Err(Stop::Read(e)) = &result {
        cx.entry_failed(name, e);
        return Err(damaged(IoError::new(e.kind(), e.to_string())));
    }
    report(name, result, cx)
}

/// Symbolic links met while extracting. A link is never made before every file is written,
/// so no entry can be written through one (`safe_join` only looks at names). Windows skips
/// them; Unix makes those whose target stays in the staging folder.
#[derive(Default)]
struct Links {
    #[cfg_attr(not(unix), allow(dead_code))]
    pending: Vec<(String, PathBuf, String)>,
}

impl Links {
    /// Keeps link `name` at `path` pointing to `target`; `Ok(false)` (done later) on Unix.
    fn add(&mut self, name: &str, path: PathBuf, target: String) -> Result<bool, Stop> {
        if cfg!(windows) {
            return Err(Stop::Note(IoError::new(ErrorKind::Unsupported, "symbolic link skipped")));
        }
        self.pending.push((name.to_owned(), path, target));
        Ok(false)
    }

    /// Makes the links kept, once every file is in place.
    fn create(self, dest: &Path, cx: &dyn ExtractCx) {
        #[cfg(unix)]
        for (name, path, target) in self.pending {
            match make_link(dest, &path, &target) {
                Ok(()) => cx.entry_done(),
                Err(e) if e.kind() == ErrorKind::PermissionDenied => cx.entry_skipped(&name, &e),
                Err(e) => cx.entry_failed(&name, &e),
            }
        }
        #[cfg(not(unix))]
        let _ = (self, dest, cx);
    }
}

/// A link's target from its header (tar).
fn link_target_of(target: Option<String>) -> Result<String, Stop> {
    target
        .filter(|t| !t.is_empty())
        .ok_or_else(|| Stop::Skip(IoError::new(ErrorKind::InvalidData, "link without a target; skipped")))
}

/// Reads a link's target (its entry's data).
fn link_target(r: &mut dyn Read) -> Result<String, Stop> {
    let mut target = Vec::new();
    r.take(4096).read_to_end(&mut target).map_err(Stop::Read)?;
    String::from_utf8(target).map_err(|_| Stop::Skip(IoError::new(ErrorKind::InvalidData, "link target is not text")))
}

/// Makes link `path` → `target` only when that cannot lead outside `dest` (`check_link`).
#[cfg(unix)]
fn make_link(dest: &Path, path: &Path, target: &str) -> IoResult<()> {
    check_link(dest, path, Path::new(target))?;
    std::os::unix::fs::symlink(target, path)
}

/// Whether link `path` → `target` cannot lead outside `dest`: every folder on the way to it
/// is a real folder (made by a regular entry; none is made here), and the target is relative
/// with `..` only at its start, not climbing above `dest`. A target then starts from a real
/// folder inside `dest` and only goes down, through links that themselves obey this rule.
#[cfg(unix)]
pub(crate) fn check_link(dest: &Path, path: &Path, target: &Path) -> IoResult<()> {
    use std::path::Component;
    let skipped = || IoError::new(ErrorKind::PermissionDenied, "symbolic link skipped");
    let folder = path.parent().and_then(|p| p.strip_prefix(dest).ok()).ok_or_else(skipped)?;
    let mut at = dest.to_path_buf();
    for part in folder.components() {
        at.push(part);
        // `symlink_metadata`: a link to a folder is not a real folder.
        if !at.symlink_metadata().is_ok_and(|m| m.is_dir()) {
            return Err(skipped());
        }
    }
    let mut ups = 0;
    let mut down = false;
    for part in target.components() {
        match part {
            Component::ParentDir if !down => ups += 1,
            Component::Normal(_) => down = true,
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return Err(skipped()),
        }
    }
    if target.as_os_str().is_empty() || ups > folder.components().count() {
        return Err(skipped());
    }
    Ok(())
}

/// A tar hard link: a copy of the file `target` names, written earlier.
fn copy_of(dest: &Path, target: &str, path: &Path, meta: &Meta, cx: &dyn ExtractCx) -> Result<bool, Stop> {
    let missing = || Stop::Skip(IoError::new(ErrorKind::NotFound, "the file it links to is missing"));
    let source = safe_join(dest, target).ok_or_else(missing)?;
    if !source.symlink_metadata().is_ok_and(|m| m.is_file()) {
        return Err(missing());
    }
    let mut file = File::open(&source).map_err(Stop::Skip)?;
    let len = file.metadata().map_err(Stop::Skip)?.len();
    write_file(&mut file, path, Some(len), meta, cx).map(|()| true)
}

/// Seconds since 1970 as a time (before 1970 too); `None` past what the system can hold
/// (a crafted header's date is dropped, not a panic).
fn unix_time(secs: i64) -> Option<SystemTime> {
    if secs >= 0 {
        UNIX_EPOCH.checked_add(Duration::from_secs(secs as u64))
    } else {
        UNIX_EPOCH.checked_sub(Duration::from_secs(secs.unsigned_abs()))
    }
}

/// A local date and time (as zip's DOS dates are) as a time.
fn local_time(parts: DateParts) -> Option<SystemTime> {
    let naive = seconds(parts)?;
    // The local offset at that moment; a second round settles a daylight-saving change.
    let mut utc = naive;
    for _ in 0..2 {
        let local = seconds(gezik_platform::local_date_parts(unix_time(utc)?)?)?;
        utc = naive - (local - utc);
    }
    unix_time(utc)
}

/// `parts` as seconds since 1970, read as UTC.
fn seconds(parts: DateParts) -> Option<i64> {
    let DateParts { year, month, day, hour, minute, second } = parts;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    // Howard Hinnant's `days_from_civil`.
    let y = i64::from(year) - i64::from(month <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (i64::from(month) + 9) % 12;
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + i64::from(hour) * 3600 + i64::from(minute) * 60 + i64::from(second))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seconds_count_from_1970() {
        let at = |year, month, day, hour, minute, second| seconds(DateParts { year, month, day, hour, minute, second });
        assert_eq!(at(1970, 1, 1, 0, 0, 0), Some(0));
        assert_eq!(at(2000, 2, 29, 0, 0, 0), Some(951_782_400));
        assert_eq!(at(2023, 11, 14, 22, 13, 20), Some(1_700_000_000));
        assert_eq!(at(1969, 12, 31, 23, 59, 0), Some(-60));
        assert_eq!(at(2024, 13, 1, 0, 0, 0), None);
    }

    #[test]
    fn local_time_reads_back_as_the_same_local_parts() {
        let parts = DateParts { year: 2024, month: 5, day: 17, hour: 13, minute: 45, second: 30 };
        let time = local_time(parts).unwrap();
        assert_eq!(gezik_platform::local_date_parts(time), Some(parts));
    }
}
