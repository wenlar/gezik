//! RAR archives (RAR4 and RAR5, `x.part1.rar`… volumes, encrypted data or headers) through
//! UnRAR. UnRAR writes each file itself, so it runs on a thread of its own while this one
//! reports progress by the size of the growing file; it cannot be stopped inside an entry,
//! so a cancel waits for the entry to end and then removes it.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::Duration;

use gezik_core::batch::archive::{VolumeKind, safe_join, volume_name, volume_set};
use unrar_ng::error::{Code, UnrarError};
use unrar_ng::{Archive, CursorBeforeFile, CursorBeforeHeader, OpenArchive, Process};

use super::{
    ArchiveSource, Entry, ExtractCx, IoError, IoResult, Links, Meta, Stop, Volumes, apply_meta, cancelled, make_dir,
    remove_earlier, report, unsafe_path,
};

/// Entries larger than this show their progress while UnRAR writes them.
const BIG: u64 = 64 << 20;

/// How often the size of a growing file is read.
const POLL: Duration = Duration::from_millis(200);

pub(super) struct RarSource {
    volumes: Volumes,
    /// The volume to open: `x.part1.rar` for `x.part3.rar`.
    first: PathBuf,
    /// Found once, used for the listing and every entry.
    password: Option<String>,
    /// The user declined the password; already reported.
    skipped: bool,
}

impl RarSource {
    pub(super) fn open(volumes: Volumes) -> IoResult<RarSource> {
        let path = volumes.paths[0].clone();
        let first = match volume_set(&volumes.name) {
            Some(set) if matches!(set.kind, VolumeKind::RarPart { .. }) => {
                let first = path.with_file_name(&set.first);
                if !first.is_file() {
                    let message = format!("the first part ({}) is missing", set.first);
                    return Err(IoError::new(ErrorKind::NotFound, message));
                }
                first
            }
            _ => path,
        };
        Ok(RarSource { volumes, first, password: None, skipped: false })
    }

    fn archive(&self) -> Archive<'_> {
        match &self.password {
            Some(password) => Archive::with_password(&self.first, password),
            None => Archive::new(&self.first),
        }
    }

    /// Asks for the password; `false` (reported) if the user declines.
    fn ask(&mut self, cx: &dyn ExtractCx, retry: bool) -> bool {
        match cx.password(retry) {
            Some(password) => {
                self.password = Some(password);
                true
            }
            None => {
                self.volumes.no_password(cx);
                self.skipped = true;
                false
            }
        }
    }

    /// UnRAR's error as an I/O error; a volume it could not open is a missing part.
    fn error(&self, e: &UnrarError) -> IoError {
        match e.code {
            Code::EOpen => self.missing_part(),
            Code::MissingPassword | Code::BadPassword => IoError::new(ErrorKind::PermissionDenied, "wrong password"),
            Code::BadData | Code::BadArchive | Code::UnknownFormat | Code::ERead => {
                IoError::new(ErrorKind::InvalidData, format!("the archive is damaged: {e}"))
            }
            Code::LargeDict => IoError::new(ErrorKind::Unsupported, "the archive needs a larger dictionary"),
            _ => IoError::other(e.to_string()),
        }
    }

    /// The error of a set whose next part is not there, named when the set is `x.partN.rar`.
    fn missing_part(&self) -> IoError {
        let message = match volume_set(&self.volumes.name) {
            Some(set) if matches!(set.kind, VolumeKind::RarPart { .. }) => (1..)
                .map(|n| volume_name(&set.base, set.kind, n))
                .find(|name| !self.first.with_file_name(name).is_file())
                .map(|name| format!("a later part ({name}) is missing")),
            _ => None,
        };
        IoError::new(ErrorKind::NotFound, message.unwrap_or_else(|| "a later part is missing".to_owned()))
    }

    /// One pass over the archive from entry `from` on, UnRAR on a thread of its own.
    fn pass(&self, from: usize, dest: &Path, cx: &dyn ExtractCx, links: &mut Links, confirmed: &mut bool) -> End {
        let stop = AtomicBool::new(false);
        let (tx, rx) = mpsc::channel();
        let password = self.password.as_deref();
        std::thread::scope(|scope| {
            let worker = scope.spawn(|| unrar_pass(&self.first, password, dest, from, &stop, tx));
            // The entry being written: its file, size and the bytes reported of it.
            let mut current: Option<(PathBuf, u64, u64)> = None;
            loop {
                match rx.recv_timeout(POLL) {
                    Ok(Msg::Start { path, size }) => current = Some((path, size, 0)),
                    Ok(Msg::Done { name, result, encrypted }) => {
                        if let Some((_, size, counted)) = current.take()
                            && result.is_ok()
                        {
                            cx.add_bytes(size.saturating_sub(counted));
                        }
                        *confirmed |= encrypted && result.is_ok();
                        let _ = report(&name, result, cx);
                    }
                    Ok(Msg::Link { name, path, target }) => {
                        let _ = report(&name, links.add(&name, path, target), cx);
                    }
                    Err(RecvTimeoutError::Timeout) => {
                        if let Some((path, size, counted)) = &mut current
                            && *size > BIG
                        {
                            let now = fs::metadata(&*path).map_or(0, |m| m.len()).min(*size);
                            if now > *counted {
                                cx.add_bytes(now - *counted);
                                *counted = now;
                            }
                        }
                    }
                    Err(RecvTimeoutError::Disconnected) => break,
                }
                if cx.stopped() {
                    stop.store(true, Ordering::Relaxed);
                }
            }
            worker.join().unwrap_or(End::Panicked)
        })
    }
}

impl ArchiveSource for RarSource {
    fn list(&mut self, cx: &dyn ExtractCx) -> IoResult<Option<Vec<Entry>>> {
        'listing: loop {
            if self.skipped {
                return Ok(Some(Vec::new()));
            }
            let listing = match self.archive().open_for_listing() {
                Ok(listing) => listing,
                Err(e) if needs_password(&e) => {
                    self.ask(cx, self.password.is_some());
                    continue;
                }
                Err(e) => return Err(self.error(&e)),
            };
            let mut entries = Vec::new();
            for header in listing {
                match header {
                    Ok(h) => entries.push(Entry {
                        name: name_of(&h.filename),
                        is_dir: h.is_directory(),
                        size: Some(h.unpacked_size),
                        packed: None,
                        encrypted: h.is_encrypted(),
                    }),
                    // Encrypted headers: no password yet, or a wrong one.
                    Err(e) if needs_password(&e) => {
                        self.ask(cx, self.password.is_some());
                        continue 'listing;
                    }
                    Err(e) => return Err(self.error(&e)),
                }
            }
            return Ok(Some(entries));
        }
    }

    fn extract(&mut self, dest: &Path, cx: &dyn ExtractCx) -> IoResult<()> {
        let mut links = Links::default();
        // Whether the password has opened an encrypted entry in full (RAR4 has no password
        // check, so before that bad data means a wrong password).
        let mut confirmed = false;
        let mut from = 0;
        while !self.skipped {
            match self.pass(from, dest, cx, &mut links, &mut confirmed) {
                End::Finished => break,
                End::Cancelled => return Err(cancelled()),
                End::Password { at } => {
                    from = at;
                    self.ask(cx, self.password.is_some());
                }
                End::Failed { at, name, error, encrypted } => {
                    let wrong = needs_password(&error) || (error.code == Code::BadData && encrypted && !confirmed);
                    if wrong {
                        from = at;
                        self.ask(cx, self.password.is_some());
                        continue;
                    }
                    if let Some(name) = &name {
                        cx.entry_failed(name, &self.error(&error));
                    }
                    // A missing volume or an unreadable header: nothing after it can be read.
                    if error.code == Code::EOpen || name.is_none() {
                        return Err(self.error(&error));
                    }
                    // UnRAR's handle ends with the error; the next pass goes on after it.
                    from = at + 1;
                }
                End::Panicked => return Err(IoError::other("UnRAR stopped")),
            }
        }
        links.create(dest, cx);
        Ok(())
    }
}

/// What the UnRAR thread tells the reporting one.
enum Msg {
    /// A file entry begins: written to `path`, `size` bytes.
    Start { path: PathBuf, size: u64 },
    /// An entry ended (a file, a folder or a refused name).
    Done { name: String, result: Result<bool, Stop>, encrypted: bool },
    /// A symbolic link, made once every file is written.
    Link { name: String, path: PathBuf, target: String },
}

/// How a pass ended.
enum End {
    Finished,
    Cancelled,
    /// Entry `at` is encrypted and there is no password yet.
    Password {
        at: usize,
    },
    /// UnRAR failed at entry `at` (`name`: `None` when its header could not be read).
    Failed {
        at: usize,
        name: Option<String>,
        error: UnrarError,
        encrypted: bool,
    },
    Panicked,
}

/// Extracts the entries from `from` on (those before are skipped), telling `tx` how each
/// went. Runs on its own thread: UnRAR's calls block until an entry is written.
fn unrar_pass(
    first: &Path,
    password: Option<&str>,
    dest: &Path,
    from: usize,
    stop: &AtomicBool,
    tx: Sender<Msg>,
) -> End {
    let archive = match password {
        Some(password) => Archive::with_password(first, password),
        None => Archive::new(first),
    };
    let failed = |at, name, error, encrypted| End::Failed { at, name, error, encrypted };
    // Before an entry's header is read, bad data is a wrong password only if one was given.
    let given = password.is_some();
    let mut open = match archive.open_for_processing() {
        Ok(open) => open,
        Err(e) => return failed(from, None, e, given),
    };
    let mut at = 0;
    loop {
        let header = match open.read_header() {
            Ok(Some(header)) => header,
            Ok(None) => return End::Finished,
            Err(e) => return failed(at, None, e, given),
        };
        let entry = header.entry();
        let name = name_of(&entry.filename);
        let encrypted = entry.is_encrypted();
        let size = entry.unpacked_size;
        let is_dir = entry.is_directory();
        let (kind, meta) = kind_and_meta(entry.file_attr);
        let done = |result| Msg::Done { name: name.clone(), result, encrypted };
        if at < from {
            open = match header.skip() {
                Ok(open) => open,
                Err(e) => return failed(at, None, e, given),
            };
            at += 1;
            continue;
        }
        if stop.load(Ordering::Relaxed) {
            return End::Cancelled;
        }
        let next = match safe_join(dest, &name) {
            None => {
                let _ = tx.send(done(Err(Stop::Skip(unsafe_path()))));
                header.skip()
            }
            Some(path) if is_dir => {
                let _ = tx.send(done(make_dir(&path).map(|()| true)));
                header.skip()
            }
            Some(path) if kind == Kind::Link => {
                let (target, next) = link_target(header, dest, at);
                if let Err(e) = next {
                    return failed(at, Some(name), e, encrypted);
                }
                let _ = tx.send(Msg::Link { name: name.clone(), path, target });
                next
            }
            Some(_) if encrypted && password.is_none() => return End::Password { at },
            Some(path) => {
                let _ = tx.send(Msg::Start { path: path.clone(), size });
                let ready = match path.parent() {
                    Some(parent) => fs::create_dir_all(parent).and_then(|()| remove_earlier(&path)),
                    None => Ok(()),
                };
                if let Err(e) = ready {
                    let _ = tx.send(done(Err(Stop::Skip(e))));
                    header.skip()
                } else {
                    match header.extract_to(&path) {
                        Ok(next) => {
                            if stop.load(Ordering::Relaxed) {
                                let _ = fs::remove_file(&path);
                                return End::Cancelled;
                            }
                            apply_meta(&path, &meta);
                            let _ = tx.send(done(Ok(true)));
                            Ok(next)
                        }
                        Err(e) => {
                            let _ = fs::remove_file(&path);
                            return failed(at, Some(name), e, encrypted);
                        }
                    }
                }
            }
        };
        open = match next {
            Ok(open) => open,
            Err(e) => return failed(at, None, e, given),
        };
        at += 1;
    }
}

/// A link entry's target, read back from the link UnRAR makes at a passing name in the stage
/// (RAR4 keeps the target as data, RAR5 elsewhere; UnRAR gives neither in memory). Windows
/// skips links, so nothing is made there.
fn link_target(header: OpenArchive<Process, CursorBeforeFile>, dest: &Path, at: usize) -> (String, Next) {
    if cfg!(not(unix)) {
        return (String::new(), header.skip());
    }
    let passing = dest.join(format!(".gezik-link-{at}"));
    let _ = fs::remove_file(&passing);
    let next = header.extract_to(&passing);
    let target = fs::read_link(&passing).map(|t| t.to_string_lossy().into_owned()).unwrap_or_default();
    let _ = fs::remove_file(&passing);
    (target, next)
}

type Next = Result<OpenArchive<Process, CursorBeforeHeader>, UnrarError>;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    File,
    Link,
}

/// What an entry is and what its file gets, from its attribute word (a Unix mode when packed
/// on Unix, else DOS attributes). UnRAR sets the time itself, to the 100 ns of RAR5.
fn kind_and_meta(attr: u32) -> (Kind, Meta) {
    let unix = matches!(attr & 0o170000, 0o100000 | 0o040000 | 0o120000);
    let link = if unix { attr & 0o170000 == 0o120000 } else { attr & 0x400 != 0 };
    let meta = Meta { modified: None, mode: unix.then_some(attr), attributes: (!unix).then_some(attr & 0xFF) };
    (if link { Kind::Link } else { Kind::File }, meta)
}

/// An entry's name with `/` between its parts (UnRAR gives `\` on Windows).
fn name_of(filename: &Path) -> String {
    filename.to_string_lossy().replace('\\', "/")
}

fn needs_password(e: &UnrarError) -> bool {
    matches!(e.code, Code::MissingPassword | Code::BadPassword)
}
