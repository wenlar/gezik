//! cpio archives: newc (`070701`, `070702`) through the `cpio` crate, and the old portable
//! format odc (`070707`) through our own parser. Binary cpio is left to 7-Zip.

use std::io::{BufRead, Cursor, ErrorKind, Read};
use std::path::Path;

use gezik_core::batch::archive::safe_join;

use super::{
    ArchiveSource, Entry, ExtractCx, IoError, IoResult, Links, Meta, Stop, Volumes, cancelled, damaged, is_top,
    link_target, make_dir, report_stream, seven_zip_needed, unix_time, unsafe_path, write_file,
};

pub(super) struct CpioSource {
    volumes: Volumes,
    odc: bool,
}

impl CpioSource {
    pub(super) fn open(volumes: Volumes) -> IoResult<CpioSource> {
        let mut magic = [0u8; 6];
        volumes.reader()?.read_exact(&mut magic)?;
        let odc = match &magic {
            b"070701" | b"070702" => false,
            b"070707" => true,
            _ => return Err(seven_zip_needed()),
        };
        Ok(CpioSource { volumes, odc })
    }
}

/// An entry's header, in either format.
struct Header {
    name: String,
    mode: u32,
    mtime: u64,
    size: u64,
}

impl ArchiveSource for CpioSource {
    fn list(&mut self, _cx: &dyn ExtractCx) -> IoResult<Option<Vec<Entry>>> {
        Ok(None)
    }

    fn extract(&mut self, dest: &Path, cx: &dyn ExtractCx) -> IoResult<()> {
        let mut r = self.volumes.reader()?;
        let mut links = Links::default();
        loop {
            if cx.stopped() {
                return Err(cancelled());
            }
            if self.odc {
                let header = read_odc(&mut r).map_err(damaged)?;
                if header.name == TRAILER {
                    break;
                }
                let mut data = (&mut r).take(header.size);
                let result = entry(&header, &mut data, dest, &mut links, cx);
                report_stream(&header.name, result, cx)?;
                // What a skipped or failed entry left unread.
                std::io::copy(&mut data, &mut std::io::sink()).map_err(damaged)?;
            } else {
                let raw = read_newc(&mut r).map_err(damaged)?;
                let mut data = ::cpio::NewcReader::new(Cursor::new(raw).chain(r)).map_err(damaged)?;
                let e = data.entry();
                if e.is_trailer() {
                    break;
                }
                let header = Header {
                    name: e.name().to_owned(),
                    mode: e.mode(),
                    mtime: u64::from(e.mtime()),
                    size: u64::from(e.file_size()),
                };
                let result = entry(&header, &mut data, dest, &mut links, cx);
                report_stream(&header.name, result, cx)?;
                r = data.finish().map_err(damaged)?.into_inner().1;
            }
        }
        links.create(dest, cx);
        Ok(())
    }
}

const TRAILER: &str = "TRAILER!!!";

/// The longest entry name a header may announce.
const MAX_NAME: u64 = 64 << 10;

/// Reads a newc header (110 bytes: the magic, then 13 fields of 8 hex digits) for the crate to
/// parse, refusing a name size over 64 KiB first: the crate allocates whatever it says.
fn read_newc(r: &mut dyn Read) -> IoResult<[u8; 110]> {
    let mut raw = [0u8; 110];
    r.read_exact(&mut raw)?;
    let name_size = std::str::from_utf8(&raw[94..102]).ok().and_then(|s| u64::from_str_radix(s, 16).ok());
    if name_size.is_none_or(|size| size > MAX_NAME) {
        return Err(IoError::new(ErrorKind::InvalidData, "bad name size in a cpio header"));
    }
    Ok(raw)
}

/// Writes one entry; `Ok(false)` for a link made later.
fn entry(
    header: &Header,
    data: &mut dyn Read,
    dest: &Path,
    links: &mut Links,
    cx: &dyn ExtractCx,
) -> Result<bool, Stop> {
    let kind = header.mode & 0o170000;
    if kind == 0o040000 && is_top(&header.name) {
        return Ok(false);
    }
    let path = safe_join(dest, &header.name).ok_or_else(|| Stop::Skip(unsafe_path()))?;
    let meta = Meta {
        modified: i64::try_from(header.mtime).ok().and_then(unix_time),
        mode: Some(header.mode),
        attributes: None,
    };
    match kind {
        0o040000 => make_dir(&path).map(|()| true),
        0o100000 => write_file(data, dest, &path, Some(header.size), &meta, cx).map(|()| true),
        // The data is the target.
        0o120000 => link_target(data).and_then(|target| links.add(&header.name, path, target)),
        _ => Err(Stop::Skip(IoError::new(ErrorKind::Unsupported, "not a file or folder; skipped"))),
    }
}

/// Reads an odc header: 76 bytes of octal fields (magic 6, dev 6, ino 6, mode 6, uid 6,
/// gid 6, nlink 6, rdev 6, mtime 11, namesize 6, filesize 11), then the name with its NUL.
/// The data follows at once, without padding.
fn read_odc(r: &mut dyn BufRead) -> IoResult<Header> {
    let mut raw = [0u8; 76];
    r.read_exact(&mut raw)?;
    if &raw[..6] != b"070707" {
        return Err(IoError::new(ErrorKind::InvalidData, "not a cpio header"));
    }
    let mode = octal(&raw[18..24])?;
    let mtime = octal(&raw[48..59])?;
    let name_size = octal(&raw[59..65])?;
    let size = octal(&raw[65..76])?;
    if name_size > MAX_NAME {
        return Err(IoError::new(ErrorKind::InvalidData, "bad name size in a cpio header"));
    }
    let mut name = vec![0u8; name_size as usize];
    r.read_exact(&mut name)?;
    if name.pop() != Some(0) {
        return Err(IoError::new(ErrorKind::InvalidData, "cpio name without its end"));
    }
    Ok(Header { name: String::from_utf8_lossy(&name).into_owned(), mode: mode as u32, mtime, size })
}

/// An octal field (all digits, as odc writes them).
fn octal(field: &[u8]) -> IoResult<u64> {
    std::str::from_utf8(field)
        .ok()
        .filter(|s| s.bytes().all(|b| (b'0'..=b'7').contains(&b)))
        .and_then(|s| u64::from_str_radix(s, 8).ok())
        .ok_or_else(|| IoError::new(ErrorKind::InvalidData, "bad number in a cpio header"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An odc header and name for `name` with `size` bytes of data.
    fn odc(name: &str, mode: u32, size: u64) -> Vec<u8> {
        let mut h = format!(
            "070707{:06o}{:06o}{:06o}{:06o}{:06o}{:06o}{:06o}{:011o}{:06o}{:011o}",
            0,
            1,
            mode,
            0,
            0,
            1,
            0,
            1_715_953_530u64,
            name.len() + 1,
            size
        )
        .into_bytes();
        h.extend(name.as_bytes());
        h.push(0);
        h
    }

    #[test]
    fn reads_odc_headers() {
        let mut bytes = odc("dir/a.txt", 0o100644, 5);
        bytes.extend(b"hello");
        bytes.extend(odc(TRAILER, 0, 0));
        let mut r = &bytes[..];
        let header = read_odc(&mut r).unwrap();
        assert_eq!(header.name, "dir/a.txt");
        assert_eq!((header.mode, header.mtime, header.size), (0o100644, 1_715_953_530, 5));
        assert_eq!(&r[..5], b"hello");
        r = &r[5..];
        assert_eq!(read_odc(&mut r).unwrap().name, TRAILER);
        assert!(r.is_empty());
    }

    #[test]
    fn refuses_a_huge_newc_name_size() {
        let header = |name_size: &str| {
            let mut h = b"070701".to_vec();
            for _ in 0..11 {
                h.extend(b"00000000");
            }
            h.extend(name_size.as_bytes());
            h.extend(b"00000000");
            h
        };
        let ok = header("00000002");
        assert_eq!(read_newc(&mut &ok[..]).unwrap()[..], ok[..]);
        for bad in [header("FFFFFFFF"), header("00010001"), header("0000000G")] {
            assert_eq!(read_newc(&mut &bad[..]).unwrap_err().kind(), ErrorKind::InvalidData);
        }
    }

    #[test]
    fn refuses_bad_odc_headers() {
        let good = odc("a", 0o100644, 0);
        let mut bad_digit = good.clone();
        bad_digit[20] = b'9';
        let mut no_nul = good.clone();
        *no_nul.last_mut().unwrap() = b'x';
        for bytes in [bad_digit, no_nul, good[..40].to_vec()] {
            assert!(read_odc(&mut &bytes[..]).is_err());
        }
    }
}
