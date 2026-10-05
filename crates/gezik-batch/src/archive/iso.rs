//! ISO 9660 images with Joliet and Rock Ridge names (a Rock Ridge name, else a Joliet name,
//! else the plain ISO name). Images whose files are only in UDF go to 7-Zip.

use std::collections::HashSet;
use std::io::{ErrorKind, Read};
use std::path::Path;

use gezik_core::batch::archive::safe_join;
use gezik_core::batch::date::DateParts;
use hadris_iso::file::EntryType;
use hadris_iso::read::{DirEntry, FileChunkIterator, IsoImage, RootDir};

use super::io::MultiFileReader;
use super::{
    ArchiveSource, Entry, ExtractCx, IoError, IoResult, Links, Meta, Stop, Volumes, cancelled, make_dir, report,
    seconds, unix_time, unsafe_path, write_file,
};

pub(super) struct IsoSource {
    volumes: Volumes,
}

impl IsoSource {
    pub(super) fn new(volumes: Volumes) -> IsoSource {
        IsoSource { volumes }
    }

    fn image(&self) -> IoResult<IsoImage<MultiFileReader>> {
        IsoImage::open(MultiFileReader::open(&self.volumes.paths)?).map_err(iso_error)
    }
}

/// One entry with its path in the image.
struct Item {
    name: String,
    entry: DirEntry,
}

impl ArchiveSource for IsoSource {
    fn list(&mut self, _cx: &dyn ExtractCx) -> IoResult<Option<Vec<Entry>>> {
        let image = self.image()?;
        let entries = walk(&image)?
            .into_iter()
            .map(|item| Entry {
                is_dir: item.entry.is_directory(),
                size: (!item.entry.is_directory()).then(|| item.entry.total_size()),
                packed: None,
                encrypted: false,
                name: item.name,
            })
            .collect();
        Ok(Some(entries))
    }

    fn extract(&mut self, dest: &Path, cx: &dyn ExtractCx) -> IoResult<()> {
        let image = self.image()?;
        let mut links = Links::default();
        for Item { name, entry } in walk(&image)? {
            if cx.stopped() {
                return Err(cancelled());
            }
            let Some(path) = safe_join(dest, &name) else {
                cx.entry_failed(&name, &unsafe_path());
                continue;
            };
            let rrip = entry.rrip.as_ref();
            let result = if entry.is_directory() {
                make_dir(&path).map(|()| true)
            } else if let Some(target) = rrip.and_then(|r| r.symlink_target.clone()) {
                links.add(&name, path, target)
            } else {
                let meta = Meta {
                    modified: modified(&entry),
                    mode: rrip.and_then(|r| r.posix_attributes.as_ref()).map(|p| p.file_mode.read()),
                    // The ISO "existence" flag hides a file.
                    attributes: Some(if entry.header().flags & 1 != 0 { 2 } else { 0 }),
                };
                match image.read_file_chunked::<{ super::BUF }>(&entry) {
                    Ok(iter) => {
                        let mut chunks = Chunks { iter, rest: Vec::new(), at: 0 };
                        write_file(&mut chunks, &path, Some(entry.total_size()), &meta, cx).map(|()| true)
                    }
                    Err(e) => Err(Stop::Read(iso_error(e))),
                }
            };
            report(&name, result, cx)?;
        }
        links.create(dest, cx);
        Ok(())
    }
}

/// The root to read: the Rock Ridge tree (full names and modes), else Joliet, else the
/// plain one.
fn root(image: &IsoImage<MultiFileReader>) -> IoResult<(RootDir, bool)> {
    let roots: Vec<RootDir> = image.root_dirs().iter().copied().collect();
    let rrip = |ty: EntryType| match ty {
        EntryType::Level1 { supports_rrip, .. }
        | EntryType::Level2 { supports_rrip, .. }
        | EntryType::Level3 { supports_rrip, .. } => supports_rrip,
        EntryType::Joliet { .. } => false,
    };
    let joliet = |ty: EntryType| matches!(ty, EntryType::Joliet { .. });
    let chosen = (image.supports_rrip().then(|| roots.iter().find(|r| rrip(r.entry_type()))).flatten())
        .or_else(|| roots.iter().find(|r| joliet(r.entry_type())))
        .copied()
        .or_else(|| image.root_dirs().try_best_choice())
        .ok_or_else(|| IoError::new(ErrorKind::InvalidData, "the image has no folders"))?;
    Ok((chosen, joliet(chosen.entry_type())))
}

/// Every entry under the root, folders before what they hold. A folder met twice (a loop in
/// a crafted image) is read once.
fn walk(image: &IsoImage<MultiFileReader>) -> IoResult<Vec<Item>> {
    let (root, joliet) = root(image)?;
    let mut items = Vec::new();
    let mut seen = HashSet::new();
    let mut stack = vec![(root.dir_ref(), String::new())];
    while let Some((dir, prefix)) = stack.pop() {
        if !seen.insert(dir.extent.0) {
            continue;
        }
        for entry in image.open_dir(dir).entries() {
            let entry = entry.map_err(iso_error)?;
            if entry.is_special() {
                continue;
            }
            let name = format!("{prefix}{}", name_of(&entry, joliet));
            if entry.is_directory() {
                stack.push((entry.as_dir_ref(image).map_err(iso_error)?, format!("{name}/")));
            }
            items.push(Item { name, entry });
        }
    }
    Ok(items)
}

/// An entry's name: Rock Ridge's, else Joliet's (UTF-16), else the ISO name without its
/// version (`;1`) and the dot of a name without an extension.
fn name_of(entry: &DirEntry, joliet: bool) -> String {
    if let Some(name) = entry.rrip.as_ref().and_then(|r| r.alternate_name.clone()) {
        return name;
    }
    let mut name = if joliet { entry.record.joliet_name() } else { String::from_utf8_lossy(entry.name()).into_owned() };
    if let Some((base, version)) = name.rsplit_once(';')
        && !version.is_empty()
        && version.bytes().all(|b| b.is_ascii_digit())
    {
        name.truncate(base.len());
    }
    if !joliet && !entry.is_directory() && name.ends_with('.') {
        name.pop();
    }
    name
}

/// Rock Ridge's modification time, else the recording time (bytes 18..25 of the record:
/// years since 1900, month, day, hour, minute, second, offset in quarter hours).
fn modified(entry: &DirEntry) -> Option<std::time::SystemTime> {
    let rr = entry.rrip.as_ref().and_then(|r| r.timestamps.as_ref()).and_then(|t| t.modify);
    let (parts, offset) = match rr {
        Some(t) => (
            DateParts {
                year: i32::from(t.year),
                month: t.month,
                day: t.day,
                hour: t.hour,
                minute: t.minute,
                second: t.second,
            },
            t.gmt_offset,
        ),
        None => {
            let raw = entry.header().to_bytes();
            (
                DateParts {
                    year: 1900 + i32::from(raw[18]),
                    month: raw[19],
                    day: raw[20],
                    hour: raw[21],
                    minute: raw[22],
                    second: raw[23],
                },
                raw[24] as i8,
            )
        }
    };
    unix_time(seconds(parts)? - i64::from(offset) * 900)
}

/// A file's data as a reader.
struct Chunks<'a> {
    iter: FileChunkIterator<'a, MultiFileReader, { super::BUF }>,
    rest: Vec<u8>,
    at: usize,
}

impl Read for Chunks<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.at == self.rest.len() {
            match self.iter.next_chunk().map_err(iso_error)? {
                Some(chunk) => (self.rest, self.at) = (chunk, 0),
                None => return Ok(0),
            }
        }
        let n = buf.len().min(self.rest.len() - self.at);
        buf[..n].copy_from_slice(&self.rest[self.at..self.at + n]);
        self.at += n;
        Ok(n)
    }
}

/// hadris's errors as I/O errors.
fn iso_error(e: impl std::fmt::Debug) -> IoError {
    IoError::new(ErrorKind::InvalidData, format!("the image is damaged: {e:?}"))
}
