//! The records bins keep of each item, read without trusting them (another program, another
//! version or a damaged disk may have written them): Windows' `$I` files, freedesktop.org's
//! `.trashinfo` files and Finder's `.DS_Store` put-back records. A record that does not parse
//! means the item's place is unknown (Put Back asks), never a guess.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime};

/// What a `$I` file says of its `$R` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WindowsInfo {
    /// Bytes (a folder's whole size).
    pub size: u64,
    /// When it was deleted (FILETIME).
    pub deleted: u64,
    /// Where it was, as Windows wrote it.
    pub path: String,
}

/// The longest `$I` file read: version 2's header and a path of 32,768 UTF-16 units.
pub(crate) const MAX_WINDOWS_INFO: u64 = 28 + 2 * 32_768;

/// A `$I` file: version 1 (Vista to 8.1: a 260-unit path) or 2 (10 and later: its length, then
/// the path); the path ends at its NUL.
pub(crate) fn parse_windows_info(bytes: &[u8]) -> Option<WindowsInfo> {
    let u64_at = |at: usize| bytes.get(at..at + 8)?.try_into().ok().map(u64::from_le_bytes);
    let (version, size, deleted) = (u64_at(0)?, u64_at(8)?, u64_at(16)?);
    let raw = match version {
        1 => bytes.get(24..24 + 520)?,
        2 => {
            let units = bytes.get(24..28)?.try_into().ok().map(u32::from_le_bytes)? as usize;
            if units == 0 || units > 32_768 {
                return None;
            }
            bytes.get(28..28 + 2 * units)?
        }
        _ => return None,
    };
    let units: Vec<u16> = raw.as_chunks::<2>().0.iter().map(|&c| u16::from_le_bytes(c)).collect();
    let end = units.iter().position(|&u| u == 0)?;
    let path = String::from_utf16(&units[..end]).ok()?;
    (!path.is_empty()).then_some(WindowsInfo { size, deleted, path })
}

/// `path` if Put Back may take an item there: on the bin's own volume (`bin_volume`: `X:` or
/// `\\server\share`, compared without case; a `$I` naming another drive or a stranger's share is
/// refused), as `X:\…` or `\\server\share\…`, every part a `windows_plain` name. Never a device
/// path (`\\?\`, `\\.\`).
pub(crate) fn windows_original(path: &str, bin_volume: &str) -> Option<String> {
    let b = path.as_bytes();
    let (volume, rest) = if b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'\\' {
        (&path[..2], &path[3..])
    } else {
        let unc = path.strip_prefix(r"\\")?;
        let mut parts = unc.splitn(3, '\\');
        let (server, share, rest) = (parts.next()?, parts.next()?, parts.next()?);
        // `?` and a trailing `.` fail `windows_plain`, so `\\?\` and `\\.\` do too.
        if !windows_plain(server) || !windows_plain(share) {
            return None;
        }
        (&path[..2 + server.len() + 1 + share.len()], rest)
    };
    let same_volume = volume.eq_ignore_ascii_case(bin_volume.trim_end_matches('\\'));
    (same_volume && !rest.is_empty() && rest.split('\\').all(windows_plain)).then(|| path.to_owned())
}

/// A name Windows keeps as written and that is a file, not a device: not empty, none of
/// `<>:"/\|?*` (`:` would be a stream) or control characters, no trailing `.` or space (Windows
/// strips them, so `.. ` would climb; `.` and `..` end in a dot too), and not a device name
/// (`CON`, `NUL.txt`, `com1`…: the part before the first dot, trailing spaces aside).
fn windows_plain(part: &str) -> bool {
    let stem = part.split('.').next().unwrap_or_default().trim_end_matches(' ').to_ascii_uppercase();
    let numbered = stem
        .strip_prefix("COM")
        .or_else(|| stem.strip_prefix("LPT"))
        .is_some_and(|n| ["1", "2", "3", "4", "5", "6", "7", "8", "9", "\u{b9}", "\u{b2}", "\u{b3}"].contains(&n));
    let device = numbered || ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"].contains(&stem.as_str());
    !part.is_empty()
        && !part.ends_with(['.', ' '])
        && !part.chars().any(|c| c < ' ' || "<>:\"/\\|?*".contains(c))
        && !device
}

/// A FILETIME (100 ns ticks since 1601) as a time; `None` before 1970.
pub(crate) fn filetime_time(filetime: u64) -> Option<SystemTime> {
    const SINCE_1601: u64 = 116_444_736_000_000_000;
    let ticks = filetime.checked_sub(SINCE_1601).filter(|t| *t > 0)?;
    SystemTime::UNIX_EPOCH.checked_add(Duration::from_nanos(ticks.checked_mul(100)?))
}

/// What a `.trashinfo` file says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TrashInfo {
    /// `Path=`, decoded.
    pub path: String,
    /// `DeletionDate=` (local time): year, month, day, hour, minute, second.
    pub deleted: Option<[i64; 6]>,
}

/// The longest `.trashinfo` read.
pub(crate) const MAX_TRASHINFO: u64 = 64 * 1024;

/// A `.trashinfo` file: `Path=` and `DeletionDate=` of its `[Trash Info]` group (the first of
/// each). A path that is not UTF-8 is refused (shortcut: such an item's place is unknown and
/// Put Back asks; read the bytes if this shows up).
pub(crate) fn parse_trashinfo(text: &str) -> Option<TrashInfo> {
    let (mut in_group, mut path, mut date) = (false, None, None);
    // `lines` also drops a `\r` before each `\n`.
    for line in text.lines() {
        if line.starts_with('[') {
            in_group = line == "[Trash Info]";
        } else if in_group {
            if let Some(value) = line.strip_prefix("Path=") {
                path.get_or_insert(value);
            } else if let Some(value) = line.strip_prefix("DeletionDate=") {
                date.get_or_insert(value);
            }
        }
    }
    let bytes = crate::fs::freedesktop::decode_path(path?)?;
    Some(TrashInfo { path: String::from_utf8(bytes).ok()?, deleted: date.and_then(parse_date) })
}

/// `YYYY-MM-DDThh:mm:ss`.
fn parse_date(text: &str) -> Option<[i64; 6]> {
    let b = text.as_bytes();
    if b.len() < 19 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[13] != b':' || b[16] != b':' {
        return None;
    }
    let number = |at: std::ops::Range<usize>| {
        let digits = text.get(at)?;
        digits.bytes().all(|c| c.is_ascii_digit()).then(|| digits.parse::<i64>().ok()).flatten()
    };
    let parts = [number(0..4)?, number(5..7)?, number(8..10)?, number(11..13)?, number(14..16)?, number(17..19)?];
    valid_parts(parts).then_some(parts)
}

fn valid_parts([year, month, day, hour, minute, second]: [i64; 6]) -> bool {
    (1..=9999).contains(&year)
        && (1..=12).contains(&month)
        && (1..=31).contains(&day)
        && (0..24).contains(&hour)
        && (0..60).contains(&minute)
        && (0..61).contains(&second)
}

/// Where Put Back takes an item a `.trashinfo` says was at `path`. The home trash (`topdir`
/// `None`) keeps absolute paths; a volume's own trash keeps paths relative to its `topdir` and
/// may only send items back onto that volume, so an absolute path there is refused (a stick's
/// `.Trash-1000` must not plant a file in the home folder). Every part a plain name.
pub(crate) fn freedesktop_original(path: &str, topdir: Option<&str>) -> Option<String> {
    let plain = |part: &str| !part.is_empty() && part != "." && part != ".." && !part.contains('\0');
    match (path.strip_prefix('/'), topdir) {
        (Some(rest), None) => rest.split('/').all(plain).then(|| path.to_owned()),
        (None, Some(top)) => path.split('/').all(plain).then(|| format!("{}/{path}", top.trim_end_matches('/'))),
        _ => None,
    }
}

/// The first folder on the way from `root` (not itself) to `target` (not itself) that is a
/// symlink or a junction, or cannot be looked at; `target` itself when it is not below `root`.
/// `None`: Put Back may move the item there. Put Back calls it right before the move, with the
/// volume (or home) the bin belongs to as `root`, so a link planted on the way cannot send an
/// item elsewhere. Folders not there yet are fine (Put Back makes them).
/// shortcut: a check, then the move (a local attacker can still race it); move through opened
/// folder handles (`openat`, `O_NOFOLLOW`) if that matters.
pub(crate) fn symlinked_folder(root: &Path, target: &Path) -> Option<PathBuf> {
    let Some(below) = target
        .strip_prefix(root)
        .ok()
        .filter(|below| below.components().all(|part| matches!(part, Component::Normal(_))))
    else {
        return Some(target.to_owned());
    };
    let mut folder = root.to_path_buf();
    for part in below.parent()?.components() {
        folder.push(part);
        match std::fs::symlink_metadata(&folder) {
            Ok(meta) if !meta.file_type().is_symlink() && meta.is_dir() => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
            _ => return Some(folder),
        }
    }
    None
}

/// A local time (`parse_trashinfo`'s date) as a time.
pub(crate) fn local_time(parts: [i64; 6]) -> Option<SystemTime> {
    if !valid_parts(parts) {
        return None;
    }
    let [year, month, day, hour, minute, second] = parts;
    let secs = local_seconds(year, month, day, hour, minute, second)?;
    let since = Duration::from_secs(secs.unsigned_abs());
    if secs >= 0 { SystemTime::UNIX_EPOCH.checked_add(since) } else { SystemTime::UNIX_EPOCH.checked_sub(since) }
}

#[cfg(unix)]
fn local_seconds(year: i64, month: i64, day: i64, hour: i64, minute: i64, second: i64) -> Option<i64> {
    // SAFETY: an all-zero `tm` is a valid value (a null `tm_zone` where the field exists).
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // `valid_parts` keeps every part small enough for an `int`.
    tm.tm_year = (year - 1900) as i32;
    tm.tm_mon = (month - 1) as i32;
    tm.tm_mday = day as i32;
    tm.tm_hour = hour as i32;
    tm.tm_min = minute as i32;
    tm.tm_sec = second as i32;
    tm.tm_isdst = -1;
    // SAFETY: `tm` is a valid, initialised struct; mktime only reads and normalises it.
    let secs = unsafe { libc::mktime(&mut tm) };
    (secs != -1).then_some(secs as i64)
}

/// shortcut: off Unix the date is taken as UTC (only the tests read `.trashinfo` there).
#[cfg(not(unix))]
fn local_seconds(year: i64, month: i64, day: i64, hour: i64, minute: i64, second: i64) -> Option<i64> {
    // Days since 1970 (Howard Hinnant's days_from_civil).
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let days = era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468;
    Some(days * 86_400 + hour * 3600 + minute * 60 + second)
}

/// Finder's put-back record of one item: the folder it was in (relative to its volume's root,
/// `ptbL`) and its name there (`ptbN`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PutBack {
    pub folder: Option<String>,
    pub name: Option<String>,
}

/// The largest `.DS_Store` read.
pub(crate) const MAX_DS_STORE: u64 = 64 * 1024 * 1024;
/// A file name or text longer than this (UTF-16 units) ends the read.
const MAX_UNITS: usize = 1024;
/// Blocks a file may list.
const MAX_BLOCKS: usize = 100_000;

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    bytes.get(at..at.checked_add(4)?)?.try_into().ok().map(u32::from_be_bytes)
}

fn utf16_be(bytes: &[u8]) -> Option<String> {
    let units: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|&c| u16::from_be_bytes(c)).collect();
    String::from_utf16(&units).ok()
}

/// The put-back records of a Trash folder's `.DS_Store`, by the item's name in the Trash.
/// `None` when the file is not one or is damaged anywhere on the way.
pub(crate) fn put_back_records(file: &[u8]) -> Option<HashMap<String, PutBack>> {
    if file.get(0..8)? != b"\0\0\0\x01Bud1" {
        return None;
    }
    // Every offset counts from byte 4.
    let data = &file[4..];
    let (root_at, root_len) = (u32_at(data, 4)? as usize, u32_at(data, 8)? as usize);
    if u32_at(data, 12)? as usize != root_at {
        return None;
    }
    let root = data.get(root_at..root_at.checked_add(root_len)?)?;
    let count = u32_at(root, 0)? as usize;
    if count > MAX_BLOCKS {
        return None;
    }
    let addresses: Vec<u32> = (0..count).map(|i| u32_at(root, 8 + 4 * i)).collect::<Option<_>>()?;
    // The addresses are padded to a multiple of 256; the table of contents follows.
    let mut at = 8 + 4 * count.div_ceil(256) * 256;
    let mut master = None;
    for _ in 0..u32_at(root, at)? {
        let len = *root.get(at + 4)? as usize;
        let name = root.get(at + 5..at + 5 + len)?;
        let value = u32_at(root, at + 5 + len)?;
        at += 9 + len;
        if name == b"DSDB" {
            master = Some(value);
        }
    }
    let block = |id: u32| -> Option<&[u8]> {
        let address = *addresses.get(id as usize)?;
        let offset = (address & !0x1f) as usize;
        let len = 1usize.checked_shl(address & 0x1f)?;
        data.get(offset..offset.checked_add(len)?)
    };
    let mut records = HashMap::new();
    let mut pending = vec![u32_at(block(master?)?, 0)?];
    // A sound tree's nodes never overlap, so together they are no bigger than the file. A damaged
    // one (going in circles, or nodes sharing bytes) runs out of this, which bounds time and memory.
    let mut budget = data.len();
    while let Some(id) = pending.pop() {
        let node = block(id)?;
        budget = budget.checked_sub(node.len())?;
        let (rightmost, count) = (u32_at(node, 0)?, u32_at(node, 4)? as usize);
        let mut at = 8;
        for _ in 0..count {
            if rightmost != 0 {
                pending.push(u32_at(node, at)?);
                at += 4;
            }
            at = record(node, at, &mut records)?;
        }
        if rightmost != 0 {
            pending.push(rightmost);
        }
    }
    Some(records)
}

/// Reads the record at `at` of `node` into `records` if it is a put-back one; where the next
/// record starts.
fn record(node: &[u8], mut at: usize, records: &mut HashMap<String, PutBack>) -> Option<usize> {
    let units = u32_at(node, at)? as usize;
    if units > MAX_UNITS {
        return None;
    }
    let name = utf16_be(node.get(at + 4..at + 4 + 2 * units)?)?;
    at += 4 + 2 * units;
    let code = node.get(at..at + 4)?;
    let kind = node.get(at + 4..at + 8)?;
    at += 8;
    let len = match kind {
        b"bool" => 1,
        b"long" | b"shor" | b"type" => 4,
        b"comp" | b"dutc" => 8,
        b"blob" => 4usize.checked_add(u32_at(node, at)? as usize)?,
        b"ustr" => {
            let units = u32_at(node, at)? as usize;
            if units > MAX_UNITS {
                return None;
            }
            4 + 2 * units
        }
        _ => return None,
    };
    let value = node.get(at..at.checked_add(len)?)?;
    if kind == b"ustr" && (code == b"ptbL" || code == b"ptbN") {
        let text = utf16_be(&value[4..])?;
        let entry = records.entry(name).or_default();
        if code == b"ptbL" {
            entry.folder = Some(text);
        } else {
            entry.name = Some(text);
        }
    }
    Some(at + len)
}

/// Where Put Back takes an item Finder says was in `folder` (relative to `volume`, the volume
/// whose Trash held it) as `name`. It stays on that volume: every part a plain name, and on the
/// startup volume (`/`) never into `/Volumes`, where the other volumes are mounted.
pub(crate) fn mac_original(volume: &str, folder: &str, name: &str) -> Option<String> {
    let plain = |part: &str| !part.is_empty() && part != "." && part != ".." && !part.contains(['/', '\0']);
    let folder = folder.trim_matches('/');
    if !plain(name) || (!folder.is_empty() && !folder.split('/').all(plain)) {
        return None;
    }
    let volume = volume.trim_end_matches('/');
    let first = folder.split('/').next().filter(|part| !part.is_empty()).unwrap_or(name);
    if volume.is_empty() && first.eq_ignore_ascii_case("Volumes") {
        return None;
    }
    Some(if folder.is_empty() { format!("{volume}/{name}") } else { format!("{volume}/{folder}/{name}") })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info_v2(size: u64, filetime: u64, path: &str) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend(2u64.to_le_bytes());
        bytes.extend(size.to_le_bytes());
        bytes.extend(filetime.to_le_bytes());
        let units: Vec<u16> = path.encode_utf16().chain([0]).collect();
        bytes.extend((units.len() as u32).to_le_bytes());
        units.iter().for_each(|u| bytes.extend(u.to_le_bytes()));
        bytes
    }

    fn info_v1(size: u64, filetime: u64, path: &str) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend(1u64.to_le_bytes());
        bytes.extend(size.to_le_bytes());
        bytes.extend(filetime.to_le_bytes());
        let mut units: Vec<u16> = path.encode_utf16().collect();
        units.resize(260, 0);
        units.iter().for_each(|u| bytes.extend(u.to_le_bytes()));
        bytes
    }

    // 2026-10-10 12:00:00 UTC as FILETIME.
    const FT: u64 = 116_444_736_000_000_000 + 1_791_633_600 * 10_000_000;

    #[test]
    fn windows_info_both_versions() {
        for bytes in [info_v2(4096, FT, r"C:\Work\çğ ş.txt"), info_v1(4096, FT, r"C:\Work\çğ ş.txt")] {
            let info = parse_windows_info(&bytes).unwrap();
            assert_eq!((info.size, info.deleted, info.path.as_str()), (4096, FT, r"C:\Work\çğ ş.txt"));
        }
        assert_eq!(filetime_time(FT), SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(1_791_633_600)));
        assert_eq!(filetime_time(0), None, "before 1970: unknown");
    }

    #[test]
    fn short_or_strange_windows_info_is_refused() {
        let good = info_v2(1, FT, r"C:\a.txt");
        for cut in [0, 7, 23, 27, good.len() - 1] {
            assert!(parse_windows_info(&good[..cut]).is_none(), "cut at {cut}");
        }
        let mut version = good.clone();
        version[0] = 3;
        assert!(parse_windows_info(&version).is_none());
        let mut huge = good.clone();
        huge[24..28].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(parse_windows_info(&huge).is_none(), "a length past the file");
        let mut unterminated = info_v2(1, FT, r"C:\a.txt");
        let at = unterminated.len() - 2;
        unterminated[at..].copy_from_slice(&u16::from(b'x').to_le_bytes());
        assert!(parse_windows_info(&unterminated).is_none(), "no NUL within the length");
        assert!(parse_windows_info(&info_v2(1, FT, "")).is_none(), "an empty path");
    }

    #[test]
    fn garbage_info_never_panics() {
        let mut seed = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let samples = [info_v2(9, FT, r"C:\a\b.txt"), info_v1(9, FT, r"C:\a\b.txt")];
        for round in 0..5000 {
            let mut bytes = samples[round % 2].clone();
            let len = (next() % (bytes.len() as u64 + 8)) as usize;
            bytes.resize(len, 0);
            for _ in 0..(next() % 6) {
                if !bytes.is_empty() {
                    let at = (next() % bytes.len() as u64) as usize;
                    bytes[at] = next() as u8;
                }
            }
            let _ = parse_windows_info(&bytes);
            let _ = parse_trashinfo(&String::from_utf8_lossy(&bytes));
        }
    }

    #[test]
    fn windows_places_gezik_may_restore_to() {
        for (bin, good) in [
            ("C:", r"C:\a.txt"),
            (r"D:\", r"d:\Work\x y\z"),
            (r"\\server\share", r"\\server\share\a.txt"),
            (r"\\S\P\", r"\\s\p\a\b"),
            ("C:", r"C:\console\COM0\LPT10\conx.txt\nul_"),
        ] {
            assert_eq!(windows_original(good, bin).as_deref(), Some(good), "{good}");
        }
    }

    #[test]
    fn windows_places_gezik_must_not_restore_to_are_refused() {
        for bad in [
            "a.txt",
            r"\a.txt",
            "C:a.txt",
            r"C:\",
            r"\\?\C:\a.txt",
            r"\\.\C:\a.txt",
            r"\\server\share",
            r"\\server\\a",
            r"C:\a\..\b",
            r"C:\a\.\b",
            r"C:\a\\b",
            r"C:\a.txt:secret",
            "C:\\a/b",
            "C:\\a\0b",
            r"C:\a\.. \b",
            r"C:\a\b.",
            r"\\se/rver\share\a",
            r"\\server\sh:are\a",
        ] {
            assert_eq!(windows_original(bad, "C:"), None, "{bad}");
            assert_eq!(windows_original(bad, r"\\server\share"), None, "{bad}");
        }
    }

    #[test]
    fn windows_places_on_another_volume_are_refused() {
        // A `$I` in D:'s bin may only send its item back to D:.
        assert_eq!(windows_original(r"C:\Users\u\Startup\a.exe", "D:"), None);
        assert_eq!(windows_original(r"\\evil\share\a.txt", "C:"), None, "no NTLM to a stranger");
        assert_eq!(windows_original(r"\\server\other\a.txt", r"\\server\share"), None);
        assert_eq!(windows_original(r"\\server\share2\a", r"\\server\share"), None);
        assert_eq!(windows_original(r"C:\a.txt", r"\\server\share"), None);
        assert_eq!(windows_original(r"C:\a.txt", ""), None);
    }

    #[test]
    fn windows_devices_and_odd_characters_are_refused() {
        for bad in [
            r"C:\CON",
            r"C:\a\nul.txt",
            r"C:\a\Aux .tar.gz",
            r"C:\prn\b",
            r"C:\com1",
            r"C:\LPT9.log",
            "C:\\com\u{b9}",
            "C:\\lpt\u{b3}.x",
            r"C:\conin$",
            r"C:\a<b",
            r"C:\a>b",
            "C:\\a\"b",
            r"C:\a|b",
            r"C:\a?b",
            r"C:\a*b",
            "C:\\a\u{1}b",
            "C:\\a\tb",
        ] {
            assert_eq!(windows_original(bad, "C:"), None, "{bad:?}");
        }
    }

    #[test]
    fn trashinfo_reads_its_group_only() {
        let text = "[Other]\nPath=/wrong\n[Trash Info]\r\nPath=/home/u/My%20Docs/a.txt\r\nDeletionDate=2026-10-04T09:05:07\r\n";
        let info = parse_trashinfo(text).unwrap();
        assert_eq!(info.path, "/home/u/My Docs/a.txt");
        assert_eq!(info.deleted, Some([2026, 10, 4, 9, 5, 7]));
        let no_date = parse_trashinfo("[Trash Info]\nPath=/a\nDeletionDate=yesterday\n").unwrap();
        assert_eq!(no_date.deleted, None, "a bad date is only an unknown date");
        assert!(parse_trashinfo("[Trash Info]\nDeletionDate=2026-10-04T09:05:07\n").is_none(), "no path");
        assert!(parse_trashinfo("Path=/a\n").is_none(), "outside the group");
        assert!(parse_trashinfo("[Trash Info]\nPath=/a%zz\n").is_none(), "bad escape");
        assert!(parse_trashinfo("[Trash Info]\nPath=/a%FF\n").is_none(), "not UTF-8 (shortcut: unknown place)");
    }

    #[test]
    fn trashinfo_places() {
        assert_eq!(freedesktop_original("/home/u/a.txt", None).as_deref(), Some("/home/u/a.txt"));
        assert_eq!(freedesktop_original("docs/a.txt", Some("/media/usb")).as_deref(), Some("/media/usb/docs/a.txt"));
        assert_eq!(freedesktop_original("docs/a.txt", Some("/media/usb/")).as_deref(), Some("/media/usb/docs/a.txt"));
    }

    #[test]
    fn a_volume_trash_keeps_its_items_on_its_volume() {
        // A USB stick's `.Trash-1000` must not send an item into the home folder.
        assert_eq!(freedesktop_original("/home/u/.config/autostart/x.desktop", Some("/media/usb")), None);
        assert_eq!(
            freedesktop_original("/media/usb/a", Some("/media/usb")),
            None,
            "volume trashes store relative paths"
        );
    }

    #[test]
    fn trashinfo_paths_that_climb_are_refused() {
        assert_eq!(freedesktop_original("docs/a.txt", None), None, "the home trash keeps absolute paths");
        for bad in ["../etc/passwd", "/home/u/../../etc/x", "/a/./b", "", "/", "/a/b/", "/a\0b"] {
            assert_eq!(freedesktop_original(bad, Some("/media/usb")), None, "{bad:?}");
            assert_eq!(freedesktop_original(bad, None), None, "{bad:?}");
        }
    }

    #[test]
    fn local_times_count_from_1970() {
        let time = local_time([2026, 10, 4, 9, 5, 7]).unwrap();
        let secs = time.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();
        // Local time: within 14 hours of the same time in UTC (1_791_104_707).
        assert!(secs.abs_diff(1_791_104_707) <= 14 * 3600, "{secs}");
        assert_eq!(local_time([2026, 13, 1, 0, 0, 0]), None);
    }

    /// A `.DS_Store` as Finder lays it out: the header, the root block (block addresses, padded
    /// to 256, and the table of contents naming `DSDB`), the master block, and one node with
    /// `records` (`p`: the node's own pointer, 0 for a leaf).
    fn ds_store(records: &[(&str, &[u8; 4], &str)], p: u32) -> Vec<u8> {
        let be = |v: u32| v.to_be_bytes();
        let utf16 = |s: &str| -> Vec<u8> { s.encode_utf16().flat_map(u16::to_be_bytes).collect() };
        let mut node = Vec::new();
        node.extend(be(p));
        node.extend(be(records.len() as u32));
        for (file, code, value) in records {
            node.extend(be(file.encode_utf16().count() as u32));
            node.extend(utf16(file));
            node.extend(**code);
            node.extend(*b"ustr");
            node.extend(be(value.encode_utf16().count() as u32));
            node.extend(utf16(value));
        }
        let mut master = Vec::new();
        for v in [2, 0, records.len() as u32, 1, 0x1000] {
            master.extend(be(v));
        }
        let mut root = Vec::new();
        root.extend(be(3));
        root.extend(be(0));
        for addr in [0x1000 | 12, 0x2000 | 12, 0x3000 | 12] {
            root.extend(be(addr));
        }
        root.resize(8 + 256 * 4, 0);
        root.extend(be(1));
        root.push(4);
        root.extend(*b"DSDB");
        root.extend(be(1));
        // Offsets count from byte 4 (after the leading 00 00 00 01).
        let mut data = Vec::new();
        data.extend(*b"Bud1");
        data.extend(be(0x1000));
        data.extend(be(0x1000));
        data.extend(be(0x1000));
        data.resize(0x1000, 0);
        for block in [root, master, node] {
            let start = data.len();
            data.extend(block);
            data.resize(start + 0x1000, 0);
        }
        let mut file = vec![0, 0, 0, 1];
        file.extend(data);
        file
    }

    #[test]
    fn ds_store_put_back_records() {
        let file = ds_store(
            &[
                ("a.txt", b"Iloc", "ignored"),
                ("a.txt", b"ptbL", "Users/u/Documents/"),
                ("a.txt", b"ptbN", "a.txt"),
                ("a 10.21.03.txt", b"ptbL", "Users/u/Desktop/"),
                ("a 10.21.03.txt", b"ptbN", "a.txt"),
            ],
            0,
        );
        let records = put_back_records(&file).unwrap();
        assert_eq!(records.len(), 2, "Iloc alone makes no record");
        assert_eq!(records["a.txt"], PutBack { folder: Some("Users/u/Documents/".into()), name: Some("a.txt".into()) });
        assert_eq!(records["a 10.21.03.txt"].name.as_deref(), Some("a.txt"), "the name it had");
    }

    #[test]
    fn a_node_pointing_at_itself_ends() {
        // Node 2 says its rightmost child is node 2.
        assert_eq!(put_back_records(&ds_store(&[], 2)), None);
    }

    #[test]
    fn ds_store_garbage_never_panics() {
        let good = ds_store(&[("a", b"ptbL", "Users/u/"), ("a", b"ptbN", "a")], 0);
        assert!(put_back_records(&good).is_some());
        assert_eq!(put_back_records(b"not a ds store"), None);
        assert_eq!(put_back_records(&good[..100]), None);
        let mut seed = 0x2545_F491_4F6C_DD1Du64;
        for _ in 0..3000 {
            let mut bytes = good.clone();
            for _ in 0..8 {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                let at = (seed % bytes.len() as u64) as usize;
                bytes[at] = (seed >> 32) as u8;
            }
            let _ = put_back_records(&bytes);
        }
    }

    #[test]
    fn mac_places() {
        assert_eq!(mac_original("/", "Users/u/Documents/", "a.txt").as_deref(), Some("/Users/u/Documents/a.txt"));
        assert_eq!(mac_original("/Volumes/USB", "photos/", "b.jpg").as_deref(), Some("/Volumes/USB/photos/b.jpg"));
        assert_eq!(mac_original("/Volumes/USB", "", "b.jpg").as_deref(), Some("/Volumes/USB/b.jpg"), "the root");
        for (folder, name) in
            [("Users/../etc/", "x"), ("Users/u/", ".."), ("Users/u/", "a/b"), ("a/./b/", "x"), ("a", "")]
        {
            assert_eq!(mac_original("/", folder, name), None, "{folder} {name}");
        }
    }

    #[test]
    fn mac_places_stay_on_the_trash_volume() {
        // The startup volume's Trash must not send an item onto a mounted volume.
        for folder in ["Volumes/USB/x/", "volumes/USB/", "/Volumes/USB/", "Volumes/"] {
            assert_eq!(mac_original("/", folder, "a"), None, "{folder}");
        }
        assert_eq!(mac_original("/", "", "Volumes"), None, "the mount folder itself");
        assert_eq!(mac_original("/", "Users/u/Volumes/", "a").as_deref(), Some("/Users/u/Volumes/a"));
        assert_eq!(mac_original("/Volumes/USB", "Volumes/", "a").as_deref(), Some("/Volumes/USB/Volumes/a"));
    }

    #[test]
    fn symlinked_folders_on_the_way_are_found() {
        let dir = std::env::temp_dir().join(format!("gezik-trash-links-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("real/sub")).unwrap();
        let target = dir.join("real/sub/missing/a.txt");
        assert_eq!(symlinked_folder(&dir, &target), None, "plain and missing folders are fine");
        assert_eq!(
            symlinked_folder(&dir.join("real"), &dir.join("elsewhere/a")),
            Some(dir.join("elsewhere/a")),
            "outside root"
        );
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.join("real"), dir.join("link")).unwrap();
            assert_eq!(symlinked_folder(&dir, &dir.join("link/sub/a.txt")), Some(dir.join("link")));
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
