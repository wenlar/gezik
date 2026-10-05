//! Writing archives: zip (Deflate on every core, AES-256, zip64), 7z (LZMA2, AES-256 with the
//! names too, volumes), tar plain or compressed, and single `.gz` and `.xz` files. Everything
//! goes to a path the caller names (a temporary one); what was written is removed again if it
//! fails or is cancelled.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Cursor, Read, Seek, Write};
use std::num::NonZeroU64;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gezik_core::batch::archive::Codec;
use sevenz_rust2::encoder_options::{AesEncoderOptions, Lzma2Options};
use sevenz_rust2::{ArchiveEntry, ArchiveWriter, EncoderConfiguration, EncoderMethod, NtTime, Password, SourceReader};
use zip::write::{FileOptions, SimpleFileOptions};
use zip::{AesMode, CompressionMethod, DateTime, ZipArchive, ZipWriter};

use super::cancelled;
use super::io::SplitWriter;
use super::sevenz::sz_error;

/// The formats Gezik writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutFormat {
    Zip,
    SevenZ,
    Tar,
    TarGz,
    TarXz,
    /// One file, gzip-compressed.
    Gz,
    /// One file, xz-compressed.
    Xz,
}

impl OutFormat {
    /// The name's ending without the dot (`.gz` and `.xz` go after the file's whole name).
    pub fn extension(self) -> &'static str {
        match self {
            OutFormat::Zip => "zip",
            OutFormat::SevenZ => "7z",
            OutFormat::Tar => "tar",
            OutFormat::TarGz => "tar.gz",
            OutFormat::TarXz => "tar.xz",
            OutFormat::Gz => "gz",
            OutFormat::Xz => "xz",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Store,
    Fast,
    Normal,
    Best,
}

impl Level {
    /// Deflate's, gzip's and bzip2's level, and xz's preset.
    fn deflate(self) -> u32 {
        match self {
            Level::Store => 0,
            Level::Fast => 1,
            Level::Normal => 6,
            Level::Best => 9,
        }
    }

    /// 7z's LZMA2 level.
    fn lzma(self) -> u32 {
        match self {
            Level::Store => 0,
            Level::Fast => 1,
            Level::Normal => 5,
            Level::Best => 9,
        }
    }
}

/// How to compress. No `Debug`: the password must not end up in a log.
#[derive(Clone)]
pub struct CompressOptions {
    pub format: OutFormat,
    pub level: Level,
    pub password: Option<String>,
    /// 7z: encrypt the file names too.
    pub encrypt_names: bool,
    /// 7z: split into parts of this many bytes.
    pub split: Option<u64>,
}

/// One thing to put in an archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Input {
    /// Where it is.
    pub path: PathBuf,
    /// Its name in the archive, `/` between folders.
    pub name: String,
    pub kind: InputKind,
    pub modified: Option<SystemTime>,
    /// Unix permission bits (on Windows from the read-only flag).
    pub mode: u32,
    pub read_only: bool,
    /// `mode` is a real Unix mode though Gezik runs on Windows (an entry of an archive being
    /// packed again): 7z keeps it too.
    pub keep_mode: bool,
    /// Who owned it in the archive it came from (tar keeps it).
    pub owner: Option<Owner>,
}

/// A tar entry's owner.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Owner {
    pub uid: u64,
    pub gid: u64,
    pub user: Option<String>,
    pub group: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputKind {
    File {
        size: u64,
    },
    Dir,
    /// A symbolic link: tar keeps it, zip and 7z leave it out.
    Link {
        target: PathBuf,
    },
}

impl Input {
    /// Its size if it is a file.
    pub fn size(&self) -> u64 {
        match self.kind {
            InputKind::File { size } => size,
            _ => 0,
        }
    }
}

/// What writing may use: progress, cancel, failure rows.
pub trait WriteCx {
    /// `n` more bytes of the inputs were read.
    fn add_bytes(&self, n: u64);
    /// An input is in the archive.
    fn entry_done(&self);
    /// An input was left out (it could not be read, or is a link zip and 7z do not hold); it
    /// counts as done and the rest goes on.
    fn entry_failed(&self, path: &Path, error: &io::Error);
    /// True once the job is cancelled (waits while it is paused).
    fn stopped(&self) -> bool;
}

/// Inputs at least this big are packed into a temporary file by a zip worker, not in memory.
const PART_IN_FILE: u64 = 64 * 1024 * 1024;

/// xz's block when several threads compress (raised to the dictionary size by the encoder).
const XZ_BLOCK: u64 = 8 * 1024 * 1024;

/// 7z's LZMA2 chunk when several threads compress: each is compressed on its own.
const LZMA2_CHUNK: u64 = 64 * 1024 * 1024;

/// The folder `paths` have in common: where the names in the archive start.
pub fn common_parent(paths: &[PathBuf]) -> PathBuf {
    let mut parents = paths.iter().map(|p| p.parent().unwrap_or(Path::new("")));
    let Some(first) = parents.next() else { return PathBuf::new() };
    let mut common: Vec<Component<'_>> = first.components().collect();
    for parent in parents {
        let same = common.iter().zip(parent.components()).take_while(|(a, b)| **a == *b).count();
        common.truncate(same);
    }
    common.iter().collect()
}

/// `sources` and everything in their folders, named relative to their common folder. Gezik's
/// own temporary files (`.gezik-*`) are left out; what cannot be read goes to `failed`.
pub fn inputs(sources: &[PathBuf], failed: &mut dyn FnMut(&Path, io::Error)) -> Vec<Input> {
    collect(sources, true, failed)
}

/// Like `inputs`, keeping every name (an unpacked archive's own `.gezik-*` entries too).
pub(crate) fn collect(sources: &[PathBuf], skip_ours: bool, failed: &mut dyn FnMut(&Path, io::Error)) -> Vec<Input> {
    let parent = common_parent(sources);
    let mut out = Vec::new();
    for source in sources {
        add(source, &parent, skip_ours, &mut out, failed);
    }
    out
}

fn add(path: &Path, parent: &Path, skip_ours: bool, out: &mut Vec<Input>, failed: &mut dyn FnMut(&Path, io::Error)) {
    if skip_ours && path.file_name().is_some_and(|n| n.to_string_lossy().starts_with(".gezik-")) {
        return;
    }
    let name = name_in(path, parent);
    if name.is_empty() {
        failed(path, io::Error::new(io::ErrorKind::InvalidInput, "Cannot compress a drive"));
        return;
    }
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(err) => return failed(path, err),
    };
    let kind = if meta.file_type().is_symlink() {
        match fs::read_link(path) {
            Ok(target) => InputKind::Link { target },
            Err(err) => return failed(path, err),
        }
    } else if meta.is_dir() {
        InputKind::Dir
    } else {
        InputKind::File { size: meta.len() }
    };
    let read_only = meta.permissions().readonly();
    let is_dir = kind == InputKind::Dir;
    out.push(Input {
        path: path.to_path_buf(),
        name,
        kind,
        modified: meta.modified().ok(),
        mode: mode_of(&meta),
        read_only,
        keep_mode: false,
        owner: None,
    });
    if !is_dir {
        return;
    }
    let mut children: Vec<PathBuf> = match fs::read_dir(path) {
        Ok(entries) => {
            entries.filter_map(|entry| entry.map_err(|err| failed(path, err)).ok()).map(|entry| entry.path()).collect()
        }
        Err(err) => return failed(path, err),
    };
    children.sort();
    for child in children {
        add(&child, parent, skip_ours, out, failed);
    }
}

/// `path` under `parent` as an archive name: `a/b.txt`.
fn name_in(path: &Path, parent: &Path) -> String {
    let rel = path.strip_prefix(parent).unwrap_or(path);
    let parts: Vec<String> = rel
        .components()
        .filter_map(|c| match c {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    parts.join("/")
}

#[cfg(unix)]
fn mode_of(meta: &fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o7777
}

#[cfg(not(unix))]
fn mode_of(meta: &fs::Metadata) -> u32 {
    let base = if meta.is_dir() { 0o755 } else { 0o644 };
    if meta.permissions().readonly() { base & 0o555 } else { base }
}

/// Writes `inputs` as `options` say to `out`; a 7z in parts goes to `out.001`, `out.002`…
/// Gives the files written. `workers`: how many threads compress zip entries.
pub fn write(
    inputs: &[Input],
    out: &Path,
    options: &CompressOptions,
    workers: usize,
    cx: &dyn WriteCx,
) -> io::Result<Vec<PathBuf>> {
    let result = match options.format {
        OutFormat::Zip => write_zip(inputs, out, options, workers, cx).map(|()| vec![out.to_path_buf()]),
        OutFormat::SevenZ => write_7z(inputs, out, options, cx),
        OutFormat::Gz => write_single(inputs, out, Codec::Gz, options.level, cx).map(|()| vec![out.to_path_buf()]),
        OutFormat::Xz => write_single(inputs, out, Codec::Xz, options.level, cx).map(|()| vec![out.to_path_buf()]),
        OutFormat::Tar => write_tar(inputs, out, Codec::None, options.level, cx),
        OutFormat::TarGz => write_tar(inputs, out, Codec::Gz, options.level, cx),
        OutFormat::TarXz => write_tar(inputs, out, Codec::Xz, options.level, cx),
    };
    settle(result, out, cx)
}

/// Writes a tar compressed with `codec` (bzip2 too, for adding to a `.tar.bz2`).
pub(crate) fn write_tar(
    inputs: &[Input],
    out: &Path,
    codec: Codec,
    level: Level,
    cx: &dyn WriteCx,
) -> io::Result<Vec<PathBuf>> {
    let total = inputs.iter().map(Input::size).sum();
    let result = compressed(out, codec, level, total, None, &mut |w| tar_into(w, inputs, cx));
    settle(result.map(|()| vec![out.to_path_buf()]), out, cx)
}

/// On failure removes what was written; a stop becomes the cancel error.
fn settle<T>(result: io::Result<T>, out: &Path, cx: &dyn WriteCx) -> io::Result<T> {
    let err = match result {
        Ok(value) => return Ok(value),
        Err(err) => err,
    };
    let _ = fs::remove_file(out);
    for n in 1.. {
        let part = volume(out, n);
        if fs::remove_file(&part).is_err() {
            break;
        }
    }
    // A stop comes back as whatever error the library made of it.
    Err(if cx.stopped() { cancelled() } else { err })
}

/// Volume `n` of `base`: `base.001`.
pub(crate) fn volume(base: &Path, n: usize) -> PathBuf {
    let mut name = base.as_os_str().to_owned();
    name.push(format!(".{n:03}"));
    PathBuf::from(name)
}

/// What a reader returns once the job is cancelled. Not `Interrupted`: `io::copy` would retry.
fn stop() -> io::Error {
    io::Error::other("cancelled")
}

fn link_skipped() -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, "symbolic link skipped")
}

/// Reads a file for an archive, telling `on_read` each piece's size; stops when it says false.
struct Feed<R, F> {
    inner: R,
    on_read: F,
    /// The file itself could not be read (not a write or a cancel).
    read_failed: bool,
}

impl<R, F> Feed<R, F> {
    fn new(inner: R, on_read: F) -> Self {
        Feed { inner, on_read, read_failed: false }
    }
}

impl<R: Read, F: FnMut(u64) -> bool> Read for Feed<R, F> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf).inspect_err(|e| self.read_failed |= e.kind() != io::ErrorKind::Interrupted)?;
        if !(self.on_read)(n as u64) {
            return Err(stop());
        }
        Ok(n)
    }
}

/// Exactly `left` bytes of a file whose size was taken before (tar writes it first).
struct Exact<R> {
    inner: R,
    left: u64,
}

impl<R: Read> Read for Exact<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.left == 0 {
            return Ok(0);
        }
        let max = buf.len().min(usize::try_from(self.left).unwrap_or(usize::MAX));
        let n = self.inner.read(&mut buf[..max])?;
        if n == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "the file got shorter while it was read"));
        }
        self.left -= n as u64;
        Ok(n)
    }
}

// --- zip ---

fn write_zip(
    inputs: &[Input],
    out: &Path,
    options: &CompressOptions,
    workers: usize,
    cx: &dyn WriteCx,
) -> io::Result<()> {
    let mut zw = ZipWriter::new(BufWriter::new(File::create(out)?)).set_auto_large_file();
    zip_inputs(&mut zw, inputs, options, workers, out, cx)?;
    zw.finish()?.flush()
}

/// Writes `archive`'s entries that `keep` accepts (copied as they are, encrypted ones too:
/// nothing is unpacked) and then `inputs` to `out` as a new zip.
pub fn rewrite_zip<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    keep: &dyn Fn(&str) -> bool,
    inputs: &[Input],
    out: &Path,
    password: Option<&str>,
    workers: usize,
    cx: &dyn WriteCx,
) -> io::Result<()> {
    let options = CompressOptions {
        format: OutFormat::Zip,
        level: Level::Normal,
        password: password.map(str::to_owned),
        encrypt_names: false,
        split: None,
    };
    let result = (|| {
        let mut zw = ZipWriter::new(BufWriter::new(File::create(out)?)).set_auto_large_file();
        for i in 0..archive.len() {
            if cx.stopped() {
                return Err(stop());
            }
            let file = archive.by_index_raw(i)?;
            if keep(file.name()) {
                zw.raw_copy_file(file)?;
            }
        }
        zip_inputs(&mut zw, inputs, &options, workers, out, cx)?;
        zw.finish()?.flush()
    })();
    settle(result, out, cx)
}

/// The options every file entry starts from: the method and level, AES-256 with a password.
fn zip_base(options: &CompressOptions) -> FileOptions<'_, ()> {
    let base = SimpleFileOptions::default();
    let base = match options.level {
        Level::Store => base.compression_method(CompressionMethod::Stored).compression_level(None),
        level => {
            base.compression_method(CompressionMethod::Deflated).compression_level(Some(i64::from(level.deflate())))
        }
    };
    match &options.password {
        Some(password) => base.with_aes_encryption(AesMode::Aes256, password),
        None => base,
    }
}

/// `base` with `input`'s time and permissions.
fn zip_options<'k>(base: FileOptions<'k, ()>, input: &Input) -> FileOptions<'k, ()> {
    base.last_modified_time(dos_time(input.modified)).unix_permissions(input.mode)
}

/// A time as zip's DOS date (local time); before 1980 or after 2107 it is 1980-01-01.
fn dos_time(time: Option<SystemTime>) -> DateTime {
    time.and_then(gezik_platform::local_date_parts)
        .and_then(|p| {
            let year = u16::try_from(p.year).ok()?;
            DateTime::from_date_and_time(year, p.month, p.day, p.hour, p.minute, p.second).ok()
        })
        .unwrap_or_default()
}

/// Folders first on this thread, then the files: one by one with one worker, else each
/// compressed by a worker into a zip of its own that this thread copies in.
fn zip_inputs<W: Write + Seek>(
    zw: &mut ZipWriter<W>,
    inputs: &[Input],
    options: &CompressOptions,
    workers: usize,
    scratch: &Path,
    cx: &dyn WriteCx,
) -> io::Result<()> {
    let base = zip_base(options);
    let mut files = Vec::new();
    for input in inputs {
        if cx.stopped() {
            return Err(stop());
        }
        match input.kind {
            InputKind::Dir => {
                let folder = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
                zw.add_directory(input.name.as_str(), zip_options(folder, input))?;
                cx.entry_done();
            }
            InputKind::Link { .. } => cx.entry_failed(&input.path, &link_skipped()),
            InputKind::File { .. } => files.push(input),
        }
    }
    if workers <= 1 || files.len() < 2 {
        for input in files {
            zip_one(zw, input, base, cx)?;
        }
        return Ok(());
    }
    zip_parallel(zw, &files, base, workers, scratch, cx)
}

/// Compresses one file straight into `zw`; a file that cannot be read is left out.
fn zip_one<W: Write + Seek>(
    zw: &mut ZipWriter<W>,
    input: &Input,
    base: FileOptions<'_, ()>,
    cx: &dyn WriteCx,
) -> io::Result<()> {
    let file = match File::open(&input.path) {
        Ok(file) => file,
        Err(err) => {
            cx.entry_failed(&input.path, &err);
            return Ok(());
        }
    };
    zw.start_file(input.name.as_str(), zip_options(base, input))?;
    let mut feed = Feed::new(file, |n| {
        cx.add_bytes(n);
        !cx.stopped()
    });
    match io::copy(&mut feed, zw) {
        Ok(_) => cx.entry_done(),
        Err(err) if feed.read_failed => {
            zw.abort_file()?;
            cx.entry_failed(&input.path, &err);
        }
        Err(err) => return Err(err),
    }
    Ok(())
}

/// A one-entry zip a worker made.
enum Part {
    Memory(Vec<u8>),
    File(PathBuf),
}

/// Why a worker made no part.
struct Failed {
    /// The file could not be read: it is left out and the rest goes on. Otherwise writing
    /// failed, and so does the archive.
    read: bool,
    error: io::Error,
}

/// Each file is compressed by one of `workers` threads into a zip of its own; this thread
/// copies them in, in the inputs' order (the same input gives the same archive). A worker
/// stays at most `2 * workers` files ahead of the copying, so few parts wait in memory.
fn zip_parallel<W: Write + Seek>(
    zw: &mut ZipWriter<W>,
    files: &[&Input],
    base: FileOptions<'_, ()>,
    workers: usize,
    scratch: &Path,
    cx: &dyn WriteCx,
) -> io::Result<()> {
    let next = AtomicUsize::new(0);
    let merged = AtomicUsize::new(0);
    let halt = AtomicBool::new(false);
    let read = AtomicU64::new(0);
    let window = 2 * workers;
    let (sender, receiver) = mpsc::channel::<(usize, Result<Part, Failed>)>();
    std::thread::scope(|scope| {
        for _ in 0..workers.min(files.len()) {
            let sender = sender.clone();
            let (next, merged, halt, read) = (&next, &merged, &halt, &read);
            scope.spawn(move || {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= files.len() {
                        break;
                    }
                    while i >= merged.load(Ordering::Relaxed) + window && !halt.load(Ordering::Relaxed) {
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    if halt.load(Ordering::Relaxed) {
                        break;
                    }
                    let made = mini_zip(files[i], base, &volume(scratch, 1000 + i), read, halt);
                    if let Err(mpsc::SendError((_, made))) = sender.send((i, made)) {
                        if let Ok(Part::File(path)) = made {
                            let _ = fs::remove_file(path);
                        }
                        break;
                    }
                }
            });
        }
        drop(sender);
        let mut waiting = BTreeMap::new();
        let result = merge_parts(zw, files, &receiver, &mut waiting, &read, &merged, cx);
        if result.is_err() {
            // The workers stop; a halted one's error is no failure of its file.
            halt.store(true, Ordering::Relaxed);
            let left: Vec<Result<Part, Failed>> =
                waiting.into_values().chain(receiver.iter().map(|(_, made)| made)).collect();
            for made in left {
                if let Ok(Part::File(path)) = made {
                    let _ = fs::remove_file(path);
                }
            }
        }
        result
    })
}

/// Copies the workers' parts into `zw` in order until they are all done, passing on their
/// progress; `waiting` holds those that came early.
fn merge_parts<W: Write + Seek>(
    zw: &mut ZipWriter<W>,
    files: &[&Input],
    receiver: &mpsc::Receiver<(usize, Result<Part, Failed>)>,
    waiting: &mut BTreeMap<usize, Result<Part, Failed>>,
    read: &AtomicU64,
    merged: &AtomicUsize,
    cx: &dyn WriteCx,
) -> io::Result<()> {
    let mut counted = 0;
    let mut want = 0;
    loop {
        let got = receiver.recv_timeout(Duration::from_millis(100));
        let now = read.load(Ordering::Relaxed);
        cx.add_bytes(now - counted);
        counted = now;
        let finished = match got {
            Ok((i, made)) => {
                waiting.insert(i, made);
                false
            }
            Err(mpsc::RecvTimeoutError::Timeout) => false,
            Err(mpsc::RecvTimeoutError::Disconnected) => true,
        };
        while let Some(made) = waiting.remove(&want) {
            match made {
                Ok(part) => {
                    let copied = match &part {
                        Part::Memory(bytes) => ZipArchive::new(Cursor::new(bytes.as_slice()))
                            .and_then(|mut archive| merge(zw, &mut archive)),
                        Part::File(path) => File::open(path)
                            .map_err(Into::into)
                            .and_then(|f| ZipArchive::new(BufReader::new(f)))
                            .and_then(|mut archive| merge(zw, &mut archive)),
                    };
                    if let Part::File(path) = &part {
                        let _ = fs::remove_file(path);
                    }
                    copied?;
                    cx.entry_done();
                }
                Err(Failed { read: true, error }) => cx.entry_failed(&files[want].path, &error),
                Err(Failed { read: false, error }) => return Err(error),
            }
            want += 1;
            merged.store(want, Ordering::Relaxed);
        }
        if finished {
            return Ok(());
        }
        if cx.stopped() {
            return Err(stop());
        }
    }
}

/// Copies `part`'s entries into `zw` as they are. Not `merge_archive`: zip 8.6 writes an AES
/// entry's method there as Deflate in the central directory, which 7-Zip calls a header error.
fn merge<W: Write + Seek, R: Read + Seek>(
    zw: &mut ZipWriter<W>,
    part: &mut ZipArchive<R>,
) -> zip::result::ZipResult<()> {
    for i in 0..part.len() {
        zw.raw_copy_file(part.by_index_raw(i)?)?;
    }
    Ok(())
}

/// `input` alone as a zip: in memory, or at `path` if it is large.
fn mini_zip(
    input: &Input,
    base: FileOptions<'_, ()>,
    path: &Path,
    read: &AtomicU64,
    halt: &AtomicBool,
) -> Result<Part, Failed> {
    let file = File::open(&input.path).map_err(|error| Failed { read: true, error })?;
    let mut feed = Feed::new(file, |n| {
        read.fetch_add(n, Ordering::Relaxed);
        !halt.load(Ordering::Relaxed)
    });
    let options = zip_options(base, input);
    let result = if input.size() < PART_IN_FILE {
        (|| {
            let mut zw = ZipWriter::new(Cursor::new(Vec::new())).set_auto_large_file();
            zw.start_file(input.name.as_str(), options)?;
            io::copy(&mut feed, &mut zw)?;
            Ok(Part::Memory(zw.finish()?.into_inner()))
        })()
    } else {
        (|| {
            let mut zw = ZipWriter::new(BufWriter::new(File::create(path)?)).set_auto_large_file();
            zw.start_file(input.name.as_str(), options)?;
            io::copy(&mut feed, &mut zw)?;
            zw.finish()?.flush()?;
            Ok(Part::File(path.to_path_buf()))
        })()
    };
    result.map_err(|error: io::Error| {
        let _ = fs::remove_file(path);
        Failed { read: feed.read_failed, error }
    })
}

// --- 7z ---

fn write_7z(inputs: &[Input], out: &Path, options: &CompressOptions, cx: &dyn WriteCx) -> io::Result<Vec<PathBuf>> {
    match options.split {
        Some(size) => {
            let mut w = sevenz_into(SplitWriter::new(out.to_path_buf(), size), inputs, options, cx)?;
            w.flush()?;
            Ok(w.paths())
        }
        None => {
            sevenz_into(BufWriter::new(File::create(out)?), inputs, options, cx)?.flush()?;
            Ok(vec![out.to_path_buf()])
        }
    }
}

/// Writes a 7z to `w`: folders and empty files as entries without data, the other files in
/// one solid LZMA2 block (several threads for a large one), AES-256 first with a password.
fn sevenz_into<W: Write + Seek>(w: W, inputs: &[Input], options: &CompressOptions, cx: &dyn WriteCx) -> io::Result<W> {
    let mut sz = ArchiveWriter::new(w).map_err(sz_error)?;
    let mut methods: Vec<EncoderConfiguration> = Vec::new();
    if let Some(password) = &options.password {
        let mut aes = AesEncoderOptions::new(Password::new(password));
        // 7-Zip's own key strength; the crate's default is 8.
        aes.num_cycles_power = 19;
        methods.push(aes.into());
    }
    let total: u64 = inputs.iter().map(Input::size).sum();
    methods.push(match options.level {
        Level::Store => EncoderMethod::COPY.into(),
        level => lzma2(level.lzma(), total),
    });
    sz.set_content_methods(methods);
    // On by default in the crate; it only means something with AES.
    sz.set_encrypt_header(options.password.is_some() && options.encrypt_names);
    let mut entries = Vec::new();
    let mut readers = Vec::new();
    for input in inputs {
        if cx.stopped() {
            return Err(stop());
        }
        match input.kind {
            InputKind::Dir => {
                sz.push_archive_entry::<&[u8]>(sz_entry(input, true), None).map_err(sz_error)?;
                cx.entry_done();
            }
            InputKind::Link { .. } => cx.entry_failed(&input.path, &link_skipped()),
            InputKind::File { size: 0 } => {
                sz.push_archive_entry::<&[u8]>(sz_entry(input, false), None).map_err(sz_error)?;
                cx.entry_done();
            }
            InputKind::File { .. } => {
                // Opened now to leave out what cannot be read: once in the block, an entry
                // cannot be taken back.
                if let Err(err) = File::open(&input.path) {
                    cx.entry_failed(&input.path, &err);
                    continue;
                }
                entries.push(sz_entry(input, false));
                readers.push(SourceReader::new(Lazy { input, cx, file: None, ended: false }));
            }
        }
    }
    if !entries.is_empty() {
        sz.push_archive_entries(entries, readers).map_err(sz_error)?;
    }
    sz.finish()
}

/// LZMA2 at `level`; on several threads (each compressing its own chunk) when the input is
/// large enough to give each of them work.
fn lzma2(level: u32, total: u64) -> EncoderConfiguration {
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get()) as u32;
    // A thread at level 9 holds about 700 MB.
    let threads = cores.min(if level >= 9 { 4 } else { 8 });
    if threads > 1 && total >= 2 * LZMA2_CHUNK {
        Lzma2Options::from_level_mt(level, threads, LZMA2_CHUNK).into()
    } else {
        Lzma2Options::from_level(level).into()
    }
}

fn sz_entry(input: &Input, is_dir: bool) -> ArchiveEntry {
    let mut entry = if is_dir { ArchiveEntry::new_directory(&input.name) } else { ArchiveEntry::new_file(&input.name) };
    if let Some(time) = input.modified
        && let Ok(time) = NtTime::try_from(time)
    {
        entry.last_modified_date = time;
        entry.has_last_modified_date = true;
    }
    // FILE_ATTRIBUTE_DIRECTORY or _ARCHIVE, read-only; on Unix the mode in the high half as
    // 7-Zip writes it.
    let mut attributes: u32 = if is_dir { 0x10 } else { 0x20 };
    if input.read_only {
        attributes |= 1;
    }
    if cfg!(unix) || input.keep_mode {
        let kind = if is_dir { 0o040000 } else { 0o100000 };
        attributes |= 0x8000 | ((kind | input.mode) << 16);
    }
    entry.windows_attributes = attributes;
    entry.has_windows_attributes = true;
    entry
}

/// A file opened only when the solid block reaches it (not thousands at once). One that
/// cannot be opened by then fails the archive: its entry is already in the block.
struct Lazy<'a> {
    input: &'a Input,
    cx: &'a dyn WriteCx,
    file: Option<File>,
    ended: bool,
}

impl Read for Lazy<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.ended {
            return Ok(0);
        }
        if self.file.is_none() {
            let file = File::open(&self.input.path)
                .map_err(|err| io::Error::new(err.kind(), format!("{}: {err}", self.input.path.display())))?;
            self.file = Some(file);
        }
        let n = self.file.as_mut().map_or(Ok(0), |f| f.read(buf))?;
        if n == 0 {
            self.ended = true;
            self.file = None;
            self.cx.entry_done();
            return Ok(0);
        }
        self.cx.add_bytes(n as u64);
        if self.cx.stopped() {
            return Err(stop());
        }
        Ok(n)
    }
}

// --- tar and single files ---

/// Opens `out` and hands `body` a writer that compresses with `codec`; `gz_header` names the
/// file in a single `.gz` (name, time).
fn compressed(
    out: &Path,
    codec: Codec,
    level: Level,
    total: u64,
    gz_header: Option<(&str, u32)>,
    body: &mut dyn FnMut(&mut dyn Write) -> io::Result<()>,
) -> io::Result<()> {
    let file = BufWriter::new(File::create(out)?);
    match codec {
        Codec::None => {
            let mut file = file;
            body(&mut file)?;
            file.flush()
        }
        Codec::Gz => {
            let level = flate2::Compression::new(level.deflate());
            let mut gz = match gz_header {
                Some((name, mtime)) => flate2::GzBuilder::new().filename(name).mtime(mtime).write(file, level),
                None => flate2::write::GzEncoder::new(file, level),
            };
            body(&mut gz)?;
            gz.finish()?.flush()
        }
        Codec::Bz2 => {
            let mut bz = bzip2::write::BzEncoder::new(file, bzip2::Compression::new(level.deflate().max(1)));
            body(&mut bz)?;
            bz.finish()?.flush()
        }
        Codec::Xz => {
            let mut options = lzma_rust2::XzOptions::with_preset(level.deflate());
            let cores = std::thread::available_parallelism().map_or(1, |n| n.get()) as u32;
            // A thread at preset 9 holds about 700 MB.
            let threads = cores.min(if level == Level::Best { 4 } else { 8 }).max(1);
            if threads > 1 && total >= 2 * XZ_BLOCK {
                options.set_block_size(NonZeroU64::new(XZ_BLOCK));
                let mut xz = lzma_rust2::XzWriterMt::new(file, options, threads)?;
                body(&mut xz)?;
                xz.finish()?.flush()
            } else {
                let mut xz = lzma_rust2::XzWriter::new(file, options)?;
                body(&mut xz)?;
                xz.finish()?.flush()
            }
        }
        Codec::Zst => Err(io::Error::new(io::ErrorKind::Unsupported, "Gezik cannot write .zst")),
    }
}

/// Writes a tar of `inputs` to `w`, links as links.
fn tar_into(w: &mut dyn Write, inputs: &[Input], cx: &dyn WriteCx) -> io::Result<()> {
    let mut builder = tar::Builder::new(w);
    // Keep the times and modes given (Deterministic would zero them).
    builder.mode(tar::HeaderMode::Complete);
    for input in inputs {
        if cx.stopped() {
            return Err(stop());
        }
        let mut header = tar::Header::new_gnu();
        header.set_mtime(unix_secs(input.modified));
        header.set_mode(input.mode);
        if let Some(owner) = &input.owner {
            header.set_uid(owner.uid);
            header.set_gid(owner.gid);
            // A name too long for the header is left out; the number stays.
            if let Some(user) = &owner.user {
                let _ = header.set_username(user);
            }
            if let Some(group) = &owner.group {
                let _ = header.set_groupname(group);
            }
        }
        match &input.kind {
            InputKind::Dir => {
                header.set_entry_type(tar::EntryType::Directory);
                header.set_size(0);
                builder.append_data(&mut header, format!("{}/", input.name), io::empty())?;
            }
            InputKind::Link { target } => {
                header.set_entry_type(tar::EntryType::Symlink);
                header.set_size(0);
                let target = target.to_string_lossy().replace('\\', "/");
                builder.append_link(&mut header, &input.name, target)?;
            }
            InputKind::File { size } => {
                let file = match File::open(&input.path) {
                    Ok(file) => file,
                    Err(err) => {
                        cx.entry_failed(&input.path, &err);
                        continue;
                    }
                };
                header.set_entry_type(tar::EntryType::Regular);
                header.set_size(*size);
                let feed = Feed::new(file, |n| {
                    cx.add_bytes(n);
                    !cx.stopped()
                });
                builder.append_data(&mut header, &input.name, Exact { inner: feed, left: *size })?;
            }
        }
        cx.entry_done();
    }
    builder.finish()
}

/// A time as whole seconds since 1970 (0 if earlier or unknown).
fn unix_secs(time: Option<SystemTime>) -> u64 {
    time.and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs())
}

/// One file as a `.gz` or `.xz`.
fn write_single(inputs: &[Input], out: &Path, codec: Codec, level: Level, cx: &dyn WriteCx) -> io::Result<()> {
    let one_file = || io::Error::new(io::ErrorKind::InvalidInput, "A .gz or .xz file holds one file");
    let [input] = inputs else { return Err(one_file()) };
    let InputKind::File { size } = input.kind else { return Err(one_file()) };
    let mut file = File::open(&input.path)?;
    let mtime = u32::try_from(unix_secs(input.modified)).unwrap_or(0);
    compressed(out, codec, level, size, Some((&input.name, mtime)), &mut |w| {
        let mut feed = Feed::new(&mut file, |n| {
            cx.add_bytes(n);
            !cx.stopped()
        });
        io::copy(&mut feed, w).map(|_| ())
    })?;
    cx.entry_done();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_parent_of_siblings_and_cousins() {
        let p = |s: &str| PathBuf::from(s);
        assert_eq!(common_parent(&[p("/a/b/x"), p("/a/b/y")]), p("/a/b"));
        assert_eq!(common_parent(&[p("/a/b/x"), p("/a/c/y")]), p("/a"));
        assert_eq!(common_parent(&[p("/a/b/x")]), p("/a/b"));
        assert_eq!(name_in(&p("/a/b/x/y.txt"), &p("/a/b")), "x/y.txt");
    }

    #[test]
    fn dos_time_falls_back_to_1980() {
        assert_eq!(dos_time(None), DateTime::default());
        assert_eq!(dos_time(Some(UNIX_EPOCH)), DateTime::default());
        let t = dos_time(Some(UNIX_EPOCH + Duration::from_secs(1_715_953_530)));
        assert_eq!(t.year(), 2024);
    }
}
