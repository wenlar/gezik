//! Tars, plain or compressed (gz, xz, bz2, zst), read as one stream with our own writing
//! instead of `unpack_in`, for failure rows and progress. Long GNU and PAX names come with
//! the entry's path.

use std::io::Read;
use std::path::Path;

use ::tar::{Archive, EntryType};
use gezik_core::batch::archive::{Codec, safe_join};

use super::io::decoder;
use super::{
    ArchiveSource, Entry, ExtractCx, IoError, IoResult, Links, Meta, Stop, Volumes, cancelled, copy_of, damaged,
    is_top, link_target_of, make_dir, report_stream, unix_time, unsafe_path, write_file,
};

pub(super) struct TarSource {
    volumes: Volumes,
    codec: Codec,
}

impl TarSource {
    pub(super) fn new(volumes: Volumes, codec: Codec) -> TarSource {
        TarSource { volumes, codec }
    }
}

impl ArchiveSource for TarSource {
    fn list(&mut self, _cx: &dyn ExtractCx) -> IoResult<Option<Vec<Entry>>> {
        Ok(None)
    }

    fn extract(&mut self, dest: &Path, cx: &dyn ExtractCx) -> IoResult<()> {
        extract_tar(decoder(self.codec, self.volumes.reader()?), dest, cx)
    }
}

/// Writes the entries of tar stream `r` under `dest` (deb's inner tars come here too).
pub(super) fn extract_tar(r: impl Read, dest: &Path, cx: &dyn ExtractCx) -> IoResult<()> {
    let mut archive = Archive::new(r);
    let mut links = Links::default();
    for entry in archive.entries().map_err(damaged)? {
        if cx.stopped() {
            return Err(cancelled());
        }
        // The next header cannot be read: the stream is bad from here on.
        let mut entry = entry.map_err(damaged)?;
        let name = String::from_utf8_lossy(&entry.path_bytes()).into_owned();
        let kind = entry.header().entry_type();
        if kind == EntryType::XGlobalHeader || (kind == EntryType::Directory && is_top(&name)) {
            continue;
        }
        // A skipped entry's data is read past by the next `entries()` step.
        let Some(path) = safe_join(dest, &name) else {
            cx.entry_failed(&name, &unsafe_path());
            continue;
        };
        let header = entry.header();
        let meta = Meta {
            modified: header.mtime().ok().and_then(|secs| unix_time(i64::try_from(secs).ok()?)),
            mode: header.mode().ok(),
            attributes: None,
        };
        let link = entry.link_name_bytes().map(|target| String::from_utf8_lossy(&target).into_owned());
        let size = entry.size();
        let result = match kind {
            EntryType::Directory => make_dir(&path).map(|()| true),
            EntryType::Regular | EntryType::Continuous => {
                write_file(&mut entry, &path, Some(size), &meta, cx).map(|()| true)
            }
            // Holes are filled in as zeros, so the size is not the stored one.
            EntryType::GNUSparse => write_file(&mut entry, &path, None, &meta, cx).map(|()| true),
            EntryType::Symlink => link_target_of(link).and_then(|target| links.add(&name, path, target)),
            EntryType::Link => link_target_of(link).and_then(|target| copy_of(dest, &target, &path, &meta, cx)),
            _ => Err(Stop::Skip(IoError::new(std::io::ErrorKind::Unsupported, "not a file or folder; skipped"))),
        };
        report_stream(&name, result, cx)?;
    }
    links.create(dest, cx);
    Ok(())
}
