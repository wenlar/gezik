//! Zip archives (one file, or 7-Zip's byte-split `x.zip.001`…): AES and ZipCrypto passwords,
//! Unix modes and DOS attributes, the NTFS or Unix time if stored, else the DOS time.

use std::io::BufReader;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ::zip::result::ZipError;
use ::zip::{CompressionMethod, DateTime, ExtraField, HasZipMetadata, ZipArchive};
use gezik_core::batch::archive::safe_join;
use gezik_core::batch::date::DateParts;

use super::io::MultiFileReader;
use super::{
    ArchiveSource, Entry, ExtractCx, IoResult, Links, Meta, Stop, Volumes, cancelled, dos_attributes, link_target,
    local_time, make_dir, report, seven_zip_needed, unix_time, unsafe_path, write_file,
};

pub(super) struct ZipSource {
    volumes: Volumes,
    /// Found once, used for every entry.
    password: Option<String>,
}

/// An entry's facts, read without its password.
struct Info {
    name: String,
    is_dir: bool,
    is_symlink: bool,
    encrypted: bool,
    /// ZipCrypto (not AES): its password check lets 1 in 256 wrong passwords through.
    weak: bool,
    size: u64,
    meta: Meta,
}

impl ZipSource {
    pub(super) fn new(volumes: Volumes) -> ZipSource {
        ZipSource { volumes, password: None }
    }

    fn archive(&self) -> IoResult<ZipArchive<BufReader<MultiFileReader>>> {
        Ok(ZipArchive::new(self.volumes.reader()?)?)
    }
}

impl ArchiveSource for ZipSource {
    fn list(&mut self, _cx: &dyn ExtractCx) -> IoResult<Option<Vec<Entry>>> {
        let mut archive = self.archive()?;
        let mut entries = Vec::with_capacity(archive.len());
        for i in 0..archive.len() {
            let file = archive.by_index_raw(i)?;
            // LZMA, zstd, xz, PPMd and older methods are not compiled in: 7-Zip reads them.
            let known = matches!(
                file.compression(),
                CompressionMethod::Stored
                    | CompressionMethod::Deflated
                    | CompressionMethod::Deflate64
                    | CompressionMethod::Bzip2
            );
            if !known && !file.is_dir() {
                return Err(seven_zip_needed());
            }
            entries.push(Entry {
                name: file.name().to_owned(),
                is_dir: file.is_dir(),
                size: Some(file.size()),
                packed: Some(file.compressed_size()),
                encrypted: file.encrypted(),
            });
        }
        Ok(Some(entries))
    }

    fn extract(&mut self, dest: &Path, cx: &dyn ExtractCx) -> IoResult<()> {
        let mut archive = self.archive()?;
        let mut links = Links::default();
        // Whether a weak password has opened an entry in full.
        let mut confirmed = false;
        'entries: for i in 0..archive.len() {
            if cx.stopped() {
                return Err(cancelled());
            }
            let info = info(&mut archive, i)?;
            let Some(path) = safe_join(dest, &info.name) else {
                cx.entry_failed(&info.name, &unsafe_path());
                continue;
            };
            if info.is_dir {
                report(&info.name, make_dir(&path).map(|()| true), cx)?;
                continue;
            }
            let mut retry = false;
            // The password the CRC failed with: failing again with it, the entry is damaged.
            let mut bad_with: Option<String> = None;
            let result = loop {
                if info.encrypted && (self.password.is_none() || retry) {
                    match cx.password(retry) {
                        Some(password) => self.password = Some(password),
                        None => {
                            self.volumes.no_password(cx);
                            break 'entries;
                        }
                    }
                }
                let password = self.password.as_deref().filter(|_| info.encrypted);
                let file = match password {
                    Some(password) => archive.by_index_decrypt(i, password.as_bytes()),
                    None => archive.by_index(i),
                };
                let mut file = match file {
                    Ok(file) => file,
                    Err(ZipError::InvalidPassword) => {
                        retry = true;
                        continue;
                    }
                    Err(e) => break Err(Stop::Read(e.into())),
                };
                if info.is_symlink {
                    break link_target(&mut file).and_then(|target| links.add(&info.name, path.clone(), target));
                }
                let result = write_file(&mut file, &path, Some(info.size), &info.meta, cx).map(|()| true);
                if info.weak && !confirmed && matches!(result, Err(Stop::Read(_))) {
                    if bad_with.is_none() || bad_with != self.password {
                        // The CRC caught a wrong password the check let through.
                        bad_with.clone_from(&self.password);
                        retry = true;
                        continue;
                    }
                    confirmed = true;
                }
                confirmed |= info.encrypted && result.is_ok();
                break result;
            };
            report(&info.name, result, cx)?;
        }
        links.create(dest, cx);
        Ok(())
    }
}

fn info(archive: &mut ZipArchive<BufReader<MultiFileReader>>, i: usize) -> IoResult<Info> {
    let file = archive.by_index_raw(i)?;
    let data = file.get_metadata();
    let mut modified = file.last_modified().and_then(dos_time);
    for field in file.extra_data_fields() {
        match field {
            // 100 ns ticks since 1601: the most exact.
            ExtraField::Ntfs(ntfs) => {
                modified = ntfs
                    .mtime()
                    .checked_sub(116_444_736_000_000_000)
                    .and_then(|t| UNIX_EPOCH.checked_add(Duration::new(t / 10_000_000, (t % 10_000_000) as u32 * 100)));
                break;
            }
            ExtraField::ExtendedTimestamp(stamp) => {
                if let Some(secs) = stamp.mod_time() {
                    modified = unix_time(i64::from(secs));
                }
            }
        }
    }
    Ok(Info {
        name: file.name().to_owned(),
        is_dir: file.is_dir(),
        is_symlink: file.is_symlink(),
        encrypted: file.encrypted(),
        weak: file.encrypted() && data.aes_mode.is_none(),
        size: file.size(),
        meta: Meta { modified, mode: file.unix_mode(), attributes: Some(dos_attributes(data.external_attributes)) },
    })
}

/// A DOS date and time (local time, 2 s steps) as a time.
fn dos_time(dos: DateTime) -> Option<SystemTime> {
    if !dos.is_valid() {
        return None;
    }
    local_time(DateParts {
        year: i32::from(dos.year()),
        month: dos.month(),
        day: dos.day(),
        hour: dos.hour(),
        minute: dos.minute(),
        second: dos.second(),
    })
}
