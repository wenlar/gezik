//! RAR archives (RAR4 and RAR5, `x.part1.rar`… volumes, encrypted data or headers) through
//! UnRAR. UnRAR writes each file itself; its data callback reports the progress and stops an
//! entry on cancel (the vendored unrar-ng's `extract_into`).

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use gezik_core::batch::archive::{VolumeKind, safe_join, volume_name, volume_set};
use unrar_ng::error::{Code, UnrarError};
use unrar_ng::{Archive, CursorBeforeFile, CursorBeforeHeader, OpenArchive, Process, Redirect};

use super::{
    ArchiveSource, Entry, ExtractCx, IoError, IoResult, Links, Meta, Stop, Volumes, apply_meta, cancelled, make_dir,
    remove_earlier, report, unsafe_path,
};

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

    /// One pass over the archive from entry `from` on (those before are skipped).
    fn pass(&self, from: usize, dest: &Path, cx: &dyn ExtractCx, links: &mut Links, confirmed: &mut bool) -> End {
        // Before an entry's header is read, bad data is a wrong password only if one was given.
        let given = self.password.is_some();
        let mut open = match self.archive().open_for_processing() {
            Ok(open) => open,
            Err(error) => return End::Failed { at: from, name: None, error, encrypted: given },
        };
        let mut at = 0;
        loop {
            let header = match open.read_header() {
                Ok(Some(header)) => header,
                Ok(None) => return End::Finished,
                Err(error) => return End::Failed { at, name: None, error, encrypted: given },
            };
            let entry = header.entry();
            let name = name_of(&entry.filename);
            let encrypted = entry.is_encrypted();
            let is_dir = entry.is_directory();
            let redirect = entry.redirect;
            let target = entry.redirect_target.as_deref().map(name_of);
            let meta = meta_of(entry.file_attr);
            let failed = |error, name| End::Failed { at, name, error, encrypted };
            if at < from {
                open = match header.skip() {
                    Ok(open) => open,
                    Err(error) => return End::Failed { at, name: None, error, encrypted: given },
                };
                at += 1;
                continue;
            }
            if cx.stopped() {
                return End::Cancelled;
            }
            let next = match safe_join(dest, &name) {
                None => {
                    cx.entry_failed(&name, &unsafe_path());
                    header.skip()
                }
                Some(path) if is_dir => {
                    let _ = report(&name, make_dir(&path).map(|()| true), cx);
                    header.skip()
                }
                // UnRAR resolves a hard link's or a copy's source itself, from a name in the
                // archive that can lead anywhere (outside the stage, too): never let it.
                Some(_) if matches!(redirect, Redirect::HardLink | Redirect::FileCopy | Redirect::Unknown(_)) => {
                    cx.entry_skipped(&name, &IoError::new(ErrorKind::Unsupported, "link or copy entry skipped"));
                    header.skip()
                }
                // Symbolic links and junctions go through `Links` and its rules.
                Some(path) if redirect != Redirect::None => {
                    let (target, next) = match target {
                        Some(target) => (target, header.skip()),
                        None => rar4_link_target(header, dest),
                    };
                    match next {
                        Ok(next) => {
                            let _ = report(&name, links.add(&name, path, target), cx);
                            Ok(next)
                        }
                        Err(error) => return failed(error, Some(name)),
                    }
                }
                Some(_) if encrypted && !given => return End::Password { at },
                Some(path) => {
                    let ready = match path.parent() {
                        Some(parent) => {
                            fs::create_dir_all(parent).map_err(Stop::Skip).and_then(|()| remove_earlier(&path))
                        }
                        None => Ok(()),
                    };
                    if let Err(e) = ready {
                        let _ = report(&name, Err(e), cx);
                        header.skip()
                    } else {
                        let progress = |n| {
                            cx.add_bytes(n);
                            !cx.stopped()
                        };
                        match header.extract_into(dest, &path, progress) {
                            Ok(next) => {
                                apply_meta(&path, &meta);
                                cx.entry_done();
                                *confirmed |= encrypted;
                                Ok(next)
                            }
                            Err(error) => {
                                let _ = fs::remove_file(&path);
                                if cx.stopped() {
                                    return End::Cancelled;
                                }
                                return failed(error, Some(name));
                            }
                        }
                    }
                }
            };
            open = match next {
                Ok(open) => open,
                Err(error) => return End::Failed { at, name: None, error, encrypted: given },
            };
            at += 1;
        }
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
            }
        }
        links.create(dest, cx);
        Ok(())
    }
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
}

/// A RAR4 Unix link's target: RAR4 keeps it as the entry's data, which UnRAR only writes
/// out as a link. On Unix it makes that link at a passing name no entry can have
/// (`safe_join` refuses control characters), where it is read back and removed. Windows
/// skips links.
fn rar4_link_target(header: OpenArchive<Process, CursorBeforeFile>, dest: &Path) -> (String, Next) {
    if cfg!(not(unix)) {
        return (String::new(), header.skip());
    }
    let passing = dest.join("gezik-link\u{1}");
    let _ = fs::remove_file(&passing);
    let next = header.extract_into(dest, &passing, |_| true);
    let target = fs::read_link(&passing).map(|t| t.to_string_lossy().into_owned()).unwrap_or_default();
    let _ = fs::remove_file(&passing);
    (target, next)
}

type Next = Result<OpenArchive<Process, CursorBeforeHeader>, UnrarError>;

/// What an entry's file gets from its attribute word: a Unix mode when packed on Unix, else
/// DOS attributes. UnRAR sets the time itself, to the 100 ns of RAR5.
fn meta_of(attr: u32) -> Meta {
    let unix = matches!(attr & 0o170000, 0o100000 | 0o040000 | 0o120000);
    Meta { modified: None, mode: unix.then_some(attr), attributes: (!unix).then_some(attr & 0xFF) }
}

/// An entry's name with `/` between its parts (UnRAR gives `\` on Windows).
fn name_of(filename: &Path) -> String {
    filename.to_string_lossy().replace('\\', "/")
}

fn needs_password(e: &UnrarError) -> bool {
    matches!(e.code, Code::MissingPassword | Code::BadPassword)
}
