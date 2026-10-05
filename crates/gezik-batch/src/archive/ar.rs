//! `ar` archives, and Debian packages (an `ar` of `debian-binary`, `control.tar.*` and
//! `data.tar.*`) laid out as dpkg-deb does: the data at the top, the control files in
//! `DEBIAN/`.

use std::io::BufReader;
use std::path::Path;

use gezik_core::batch::archive::{Codec, safe_join};

use super::io::decoder;
use super::tar::extract_tar;
use super::{
    ArchiveSource, Entry, ExtractCx, IoResult, Meta, Volumes, cancelled, damaged, report_stream, seven_zip_needed,
    unix_time, unsafe_path, write_file,
};

pub(super) struct ArSource {
    volumes: Volumes,
    deb: bool,
}

impl ArSource {
    pub(super) fn new(volumes: Volumes, deb: bool) -> ArSource {
        ArSource { volumes, deb }
    }
}

impl ArchiveSource for ArSource {
    fn list(&mut self, _cx: &dyn ExtractCx) -> IoResult<Option<Vec<Entry>>> {
        if self.deb {
            return Ok(None);
        }
        let mut archive = ::ar::Archive::new(self.volumes.reader()?);
        let mut entries = Vec::new();
        while let Some(entry) = archive.next_entry() {
            let entry = entry?;
            let header = entry.header();
            entries.push(Entry {
                name: String::from_utf8_lossy(header.identifier()).into_owned(),
                is_dir: false,
                size: Some(header.size()),
                packed: Some(header.size()),
                encrypted: false,
            });
        }
        Ok(Some(entries))
    }

    fn extract(&mut self, dest: &Path, cx: &dyn ExtractCx) -> IoResult<()> {
        let mut archive = ::ar::Archive::new(self.volumes.reader()?);
        while let Some(entry) = archive.next_entry() {
            if cx.stopped() {
                return Err(cancelled());
            }
            let mut entry = entry.map_err(damaged)?;
            let header = entry.header();
            let name = String::from_utf8_lossy(header.identifier()).into_owned();
            if self.deb {
                // `debian-binary` (the format version) and signatures (`_gpg…`) are not files.
                let Some((folder, codec)) = deb_member(&name) else { continue };
                let Some(codec) = codec else {
                    cx.entry_failed(&name, &seven_zip_needed());
                    continue;
                };
                let target = if folder.is_empty() { dest.to_path_buf() } else { dest.join(folder) };
                extract_tar(decoder(codec, BufReader::new(&mut entry)), &target, cx)?;
                continue;
            }
            let Some(path) = safe_join(dest, &name) else {
                cx.entry_failed(&name, &unsafe_path());
                continue;
            };
            let meta = Meta {
                modified: i64::try_from(header.mtime()).ok().and_then(unix_time),
                mode: Some(header.mode()),
                attributes: None,
            };
            let size = header.size();
            let result = write_file(&mut entry, &path, Some(size), &meta, cx).map(|()| true);
            report_stream(&name, result, cx)?;
        }
        Ok(())
    }
}

/// Where a deb member's tar goes (`""`: the top, `DEBIAN`), and its compression (`None`:
/// one Gezik does not read, such as lzma). `None` for a member that is not one of them.
fn deb_member(name: &str) -> Option<(&'static str, Option<Codec>)> {
    let (folder, rest) = match name.strip_prefix("data.tar") {
        Some(rest) => ("", rest),
        None => ("DEBIAN", name.strip_prefix("control.tar")?),
    };
    let codec = match rest {
        "" => Some(Codec::None),
        ".gz" => Some(Codec::Gz),
        ".xz" => Some(Codec::Xz),
        ".bz2" => Some(Codec::Bz2),
        ".zst" => Some(Codec::Zst),
        _ => None,
    };
    Some((folder, codec))
}
