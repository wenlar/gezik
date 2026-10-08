//! 7z archives (one file, or volumes `x.7z.001`…): encrypted headers and data, solid blocks.
//! Every entry reader is read to its end, even a skipped one: entries of a solid block share
//! one stream, and an unread rest would shift the next entry.

use std::io::{self, Read};
use std::path::Path;

use gezik_core::batch::archive::safe_join;
use sevenz_rust2::{Archive, ArchiveEntry, ArchiveReader, EncoderMethod, Error as SzError, Password};

use super::{
    ArchiveSource, BUF, Entry, ExtractCx, Header, IoResult, Links, Meta, Stop, Volumes, cancelled, dos_attributes,
    link_target, make_dir, seven_zip_needed, unsafe_path, write_file,
};

pub(super) struct SevenZSource {
    volumes: Volumes,
    /// Found once, used for the header and every entry.
    password: Option<String>,
    /// `None` until the password opens an encrypted header.
    archive: Option<Archive>,
    /// The header opened with the password, so it is the right one.
    header_password: bool,
    /// The user declined the password; already reported.
    skipped: bool,
}

impl SevenZSource {
    pub(super) fn open(volumes: Volumes) -> IoResult<SevenZSource> {
        let archive = match Archive::read(&mut volumes.reader()?, &Password::empty()) {
            Ok(archive) => Some(decodable(archive)?),
            Err(SzError::PasswordRequired) => None,
            Err(e) => return Err(sz_error(e)),
        };
        Ok(SevenZSource { volumes, password: None, archive, header_password: false, skipped: false })
    }

    /// Reads the header, asking for its password if it is encrypted. `false`: skipped.
    fn header(&mut self, cx: &dyn ExtractCx) -> IoResult<bool> {
        if self.skipped {
            return Ok(false);
        }
        let mut retry = false;
        while self.archive.is_none() {
            if self.password.is_none() || retry {
                let Some(password) = cx.password(retry) else {
                    self.volumes.no_password(cx);
                    self.skipped = true;
                    return Ok(false);
                };
                self.password = Some(password);
            }
            let password = Password::new(self.password.as_deref().unwrap_or_default());
            match Archive::read(&mut self.volumes.reader()?, &password) {
                Ok(archive) => {
                    self.archive = Some(decodable(archive)?);
                    self.header_password = true;
                }
                Err(SzError::MaybeBadPassword(_) | SzError::PasswordRequired) => retry = true,
                Err(e) => return Err(sz_error(e)),
            }
        }
        Ok(true)
    }
}

impl ArchiveSource for SevenZSource {
    fn list(&mut self, cx: &dyn ExtractCx) -> IoResult<Option<Vec<Entry>>> {
        if !self.header(cx)? {
            return Ok(Some(Vec::new()));
        }
        let Some(archive) = &self.archive else { return Ok(Some(Vec::new())) };
        let entries = archive
            .files
            .iter()
            .enumerate()
            .map(|(i, e)| Entry {
                name: e.name().to_owned(),
                is_dir: e.is_directory(),
                size: Some(e.size()),
                packed: (e.compressed_size > 0).then_some(e.compressed_size),
                encrypted: archive
                    .stream_map
                    .file_block_index
                    .get(i)
                    .copied()
                    .flatten()
                    .is_some_and(|b| encrypted(archive, b)),
            })
            .collect();
        Ok(Some(entries))
    }

    fn extract(&mut self, dest: &Path, cx: &dyn ExtractCx) -> IoResult<()> {
        if !self.header(cx)? {
            return Ok(());
        }
        let Some(archive) = self.archive.clone() else { return Ok(()) };
        let any_encrypted = (0..archive.blocks.len()).any(|b| encrypted(&archive, b));
        let mut confirmed = !any_encrypted || self.header_password;
        // Entries handled in a pass a wrong password ended; the next pass skips them.
        let mut done = 0usize;
        let mut retry = false;
        // The password a pass ended with as maybe wrong: ending so again, the entry is damaged.
        let mut bad_with: Option<String> = None;
        let mut links = Links::default();
        loop {
            if any_encrypted && (self.password.is_none() || retry) {
                let Some(password) = cx.password(retry) else {
                    self.volumes.no_password(cx);
                    break;
                };
                self.password = Some(password);
            }
            confirmed |= bad_with.is_some() && bad_with == self.password;
            let password = self.password.as_deref().map_or_else(Password::empty, Password::new);
            let mut reader = ArchiveReader::from_archive(archive.clone(), self.volumes.reader()?, password);
            let mut seen = 0usize;
            let mut stop = None;
            let result = reader.for_each_entries(|e, r| {
                seen += 1;
                if seen <= done {
                    drain(r, cx);
                    return Ok(true);
                }
                if cx.stopped() {
                    stop = Some(cancelled());
                    return Err(SzError::Other("cancelled".into()));
                }
                let result = entry(e, r, dest, &mut links, cx);
                if !matches!(result, Err(Stop::Cancelled)) {
                    drain(r, cx);
                }
                match result {
                    Ok(finished) => {
                        confirmed |= e.has_stream() && e.size() > 0;
                        if finished {
                            cx.entry_done();
                        }
                    }
                    Err(Stop::Cancelled) => {
                        stop = Some(cancelled());
                        return Err(SzError::Other("cancelled".into()));
                    }
                    // Before any entry decoded in full, bad data means a wrong password.
                    Err(Stop::Read(error)) if !confirmed => return Err(SzError::MaybeBadPassword(error)),
                    Err(Stop::Read(error) | Stop::Skip(error)) => cx.entry_failed(e.name(), &error),
                    Err(Stop::Note(error)) => cx.entry_skipped(e.name(), &error),
                }
                done = seen;
                Ok(true)
            });
            match result {
                Ok(()) => break,
                Err(_) if stop.is_some() => return Err(stop.unwrap_or_else(cancelled)),
                Err(SzError::PasswordRequired) if any_encrypted => retry = self.password.is_some(),
                Err(SzError::MaybeBadPassword(_)) if !confirmed => {
                    retry = true;
                    bad_with.clone_from(&self.password);
                }
                Err(e) => return Err(sz_error(e)),
            }
        }
        links.create(dest, cx);
        Ok(())
    }
}

/// Writes one entry; `Ok(false)` for a link made later.
fn entry(e: &ArchiveEntry, r: &mut dyn Read, dest: &Path, links: &mut Links, cx: &dyn ExtractCx) -> Result<bool, Stop> {
    let path = safe_join(dest, e.name()).ok_or_else(|| Stop::Skip(unsafe_path()))?;
    let attributes = e.has_windows_attributes.then_some(e.windows_attributes());
    // 7-Zip keeps a Unix mode in the high half, flagged by 0x8000.
    let mode = attributes.filter(|a| a & 0x8000 != 0).map(|a| a >> 16);
    let modified = e.has_last_modified_date.then(|| e.last_modified_date().into());
    cx.entry_header(&path, &Header { mode: mode.map(|m| m & 0o7777), modified, owner: None });
    if e.is_directory() {
        return make_dir(&path).map(|()| true);
    }
    if mode.is_some_and(|m| m & 0o170000 == 0o120000) {
        let target = link_target(r)?;
        return links.add(e.name(), path, target);
    }
    let meta = Meta { modified, mode, attributes: attributes.map(dos_attributes) };
    write_file(r, &path, Some(e.size()), &meta, cx).map(|()| true)
}

/// `archive` if Gezik decodes every method of it, else "7-Zip needed": the job then hands
/// the archive to 7-Zip before writing anything (sevenz's Deflate, zstd, brotli, lz4 and BCJ2
/// are not compiled in).
fn decodable(archive: Archive) -> IoResult<Archive> {
    let known = |coder: &sevenz_rust2::Coder| {
        let id = coder.encoder_method_id();
        let filter = [
            EncoderMethod::ID_BCJ_X86,
            EncoderMethod::ID_BCJ_PPC,
            EncoderMethod::ID_BCJ_IA64,
            EncoderMethod::ID_BCJ_ARM,
            EncoderMethod::ID_BCJ_ARM64,
            EncoderMethod::ID_BCJ_ARM_THUMB,
            EncoderMethod::ID_BCJ_SPARC,
            EncoderMethod::ID_BCJ_RISCV,
        ];
        let method = [
            EncoderMethod::ID_COPY,
            EncoderMethod::ID_DELTA,
            EncoderMethod::ID_LZMA,
            EncoderMethod::ID_LZMA2,
            EncoderMethod::ID_PPMD,
            EncoderMethod::ID_BZIP2,
            EncoderMethod::ID_AES256_SHA256,
        ];
        // A tiny archive may ask for a 4 GiB dictionary (allocated at once): past 1.5 GiB,
        // 7-Zip with its own limits.
        let props = coder.properties();
        let memory = match id {
            EncoderMethod::ID_LZMA | EncoderMethod::ID_PPMD => {
                props.get(1..5).map_or(0, |p| u32::from_le_bytes([p[0], p[1], p[2], p[3]]).into())
            }
            EncoderMethod::ID_LZMA2 => props
                .first()
                .map_or(0, |&bits| if bits >= 40 { u64::MAX } else { (2 | u64::from(bits & 1)) << (bits / 2 + 11) }),
            _ => 0,
        };
        // A branch filter's start offset (rare) is not read by the crate.
        memory <= 1_536 << 20 && (method.contains(&id) || (filter.contains(&id) && props.is_empty()))
    };
    if archive.blocks.iter().all(|block| block.coders.iter().all(known)) {
        Ok(archive)
    } else {
        Err(seven_zip_needed())
    }
}

/// Whether block `b` is AES-encrypted.
fn encrypted(archive: &Archive, b: usize) -> bool {
    archive.blocks.get(b).is_some_and(|block| {
        block.coders.iter().any(|coder| coder.encoder_method_id() == EncoderMethod::ID_AES256_SHA256)
    })
}

/// Reads an entry to its end (solid blocks need it), stopping early once cancelled.
fn drain(r: &mut dyn Read, cx: &dyn ExtractCx) {
    let mut buf = vec![0u8; BUF];
    while !cx.stopped() {
        match r.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
    }
}

pub(super) fn sz_error(e: SzError) -> io::Error {
    match e {
        SzError::Io(e, _) | SzError::FileOpen(e, _) => e,
        SzError::PasswordRequired => io::Error::new(io::ErrorKind::PermissionDenied, "no password"),
        SzError::MaybeBadPassword(_) => io::Error::new(io::ErrorKind::PermissionDenied, "wrong password"),
        SzError::UnsupportedCompressionMethod(method) => {
            io::Error::new(io::ErrorKind::Unsupported, format!("7-Zip needed ({method})"))
        }
        e => io::Error::new(io::ErrorKind::InvalidData, e.to_string()),
    }
}
