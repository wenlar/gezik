//! Cabinets (one `.cab`; sets spanning several files are not read). The crate decodes a
//! compressed folder from its start for every file it opens, so a folder of many files
//! costs the square of its size: those cabinets go to 7-Zip.

use std::collections::HashSet;
use std::io::ErrorKind;
use std::path::Path;

use ::cab::{Cabinet, CompressionType};
use gezik_core::batch::archive::safe_join;
use gezik_core::batch::date::DateParts;

use super::io::MultiFileReader;
use super::{
    ArchiveSource, Entry, ExtractCx, IoError, IoResult, Meta, Stop, Volumes, cancelled, local_time, report,
    seven_zip_needed, unsafe_path, write_file,
};

/// The most files a compressed folder may hold before 7-Zip does it faster.
const MAX_FILES: usize = 200;

pub(super) struct CabSource {
    cabinet: Cabinet<MultiFileReader>,
}

/// A file's facts, read before any is opened (the listing borrows the cabinet).
struct Info {
    name: String,
    size: u64,
    meta: Meta,
}

impl CabSource {
    pub(super) fn open(volumes: Volumes) -> IoResult<CabSource> {
        let cabinet = Cabinet::new(MultiFileReader::open(&volumes.paths)?)?;
        for folder in cabinet.folder_entries() {
            let compressed = folder.compression_type() != CompressionType::None;
            if matches!(folder.compression_type(), CompressionType::Quantum(..)) {
                return Err(seven_zip_needed());
            }
            if compressed && folder.file_entries().len() > MAX_FILES {
                return Err(IoError::new(ErrorKind::Unsupported, "large cabinet — 7-Zip is faster"));
            }
        }
        Ok(CabSource { cabinet })
    }

    /// Every file, folder by folder in the order of their data.
    fn infos(&self) -> Vec<Info> {
        let mut infos = Vec::new();
        for folder in self.cabinet.folder_entries() {
            for file in folder.file_entries() {
                let attributes = u32::from(file.is_read_only())
                    | u32::from(file.is_hidden()) << 1
                    | u32::from(file.is_system()) << 2;
                infos.push(Info {
                    name: file.name().to_owned(),
                    size: u64::from(file.uncompressed_size()),
                    meta: Meta {
                        modified: file.datetime().and_then(|t| {
                            local_time(DateParts {
                                year: t.year(),
                                month: u8::from(t.month()),
                                day: t.day(),
                                hour: t.hour(),
                                minute: t.minute(),
                                second: t.second(),
                            })
                        }),
                        // Read-only on Unix too.
                        mode: file.is_read_only().then_some(0o444),
                        attributes: Some(attributes),
                    },
                });
            }
        }
        infos
    }
}

impl ArchiveSource for CabSource {
    fn list(&mut self, _cx: &dyn ExtractCx) -> IoResult<Option<Vec<Entry>>> {
        let entries = self
            .infos()
            .into_iter()
            .map(|info| Entry {
                name: info.name.replace('\\', "/"),
                is_dir: false,
                size: Some(info.size),
                packed: None,
                encrypted: false,
            })
            .collect();
        Ok(Some(entries))
    }

    fn extract(&mut self, dest: &Path, cx: &dyn ExtractCx) -> IoResult<()> {
        let mut seen = HashSet::new();
        for info in self.infos() {
            if cx.stopped() {
                return Err(cancelled());
            }
            let name = info.name.replace('\\', "/");
            let Some(path) = safe_join(dest, &info.name) else {
                cx.entry_failed(&name, &unsafe_path());
                continue;
            };
            // The crate finds a file by its name, so only the first of a name can be read.
            if !seen.insert(info.name.clone()) {
                cx.entry_failed(&name, &IoError::new(ErrorKind::Unsupported, "a second file of this name; skipped"));
                continue;
            }
            let result = match self.cabinet.read_file(&info.name) {
                Ok(mut r) => write_file(&mut r, &path, Some(info.size), &info.meta, cx).map(|()| true),
                Err(e) => Err(Stop::Read(e)),
            };
            report(&name, result, cx)?;
        }
        Ok(())
    }
}
