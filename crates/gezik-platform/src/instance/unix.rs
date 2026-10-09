//! The single-instance channel on macOS and Linux: a Unix socket in a folder only this user
//! can enter (spec 5.2), its taking ordered by a lock file, the peer's user checked both ways.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::thread;
use std::time::{Duration, Instant};

use super::{CallSlot, Claim, SEND_TIMEOUT, dir_is_private, socket_paths};

pub(super) struct Listener {
    socket: UnixListener,
    /// Held while this Gezik lives: the next one sees the channel taken.
    lock: File,
}

fn me() -> u32 {
    // SAFETY: geteuid has no arguments and cannot fail.
    unsafe { libc::geteuid() }
}

/// The socket and lock paths for `key`, in this user's private folder.
pub(super) fn paths(key: &str) -> Option<(PathBuf, PathBuf)> {
    socket_paths(&private_dir()?, key)
}

/// `$TMPDIR` on macOS, `$XDG_RUNTIME_DIR` on Linux, if private; else `/tmp/gezik-<uid>`,
/// made 0700 here and checked the same way (spec 5.2). `None`: no single instance now.
/// The folder itself is checked (no link followed): once it is ours and 0700 nobody else
/// can change what is in it, and sticky `/tmp` keeps others from renaming it.
fn private_dir() -> Option<PathBuf> {
    let me = me();
    let name = if cfg!(target_os = "macos") { "TMPDIR" } else { "XDG_RUNTIME_DIR" };
    if let Some(dir) = std::env::var_os(name).filter(|d| !d.is_empty()).map(PathBuf::from)
        && is_private(&dir, me)
    {
        return Some(dir);
    }
    let dir = PathBuf::from(format!("/tmp/gezik-{me}"));
    let _ = fs::DirBuilder::new().mode(0o700).create(&dir);
    is_private(&dir, me).then_some(dir)
}

fn is_private(dir: &Path, me: u32) -> bool {
    fs::symlink_metadata(dir)
        .is_ok_and(|meta| dir_is_private(meta.is_dir(), meta.file_type().is_symlink(), meta.uid(), meta.mode(), me))
}

#[cfg(target_os = "macos")]
fn peer_uid(stream: &UnixStream) -> Option<u32> {
    let (mut uid, mut gid) = (0, 0);
    // SAFETY: the descriptor is open; uid and gid are written by the call.
    (unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) } == 0).then_some(uid)
}

#[cfg(not(target_os = "macos"))]
fn peer_uid(stream: &UnixStream) -> Option<u32> {
    let mut cred = libc::ucred { pid: 0, uid: 0, gid: 0 };
    let mut len = size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: the descriptor is open; `cred` is as large as `len` says.
    let ok = unsafe {
        libc::getsockopt(stream.as_raw_fd(), libc::SOL_SOCKET, libc::SO_PEERCRED, (&raw mut cred).cast(), &mut len)
    } == 0;
    ok.then_some(cred.uid)
}

/// The running Gezik's socket, if one listens; refused if its owner is someone else.
pub(super) fn connect(key: &str, timeout: Duration) -> io::Result<Option<UnixStream>> {
    let Some((socket, _)) = paths(key) else { return Ok(None) };
    let stream = match UnixStream::connect(&socket) {
        Ok(stream) => stream,
        // None there, or a crashed Gezik's file: nobody listens.
        Err(err) if matches!(err.kind(), io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused) => {
            return Ok(None);
        }
        Err(err) => return Err(err),
    };
    if peer_uid(&stream) != Some(me()) {
        return Err(io::Error::from(io::ErrorKind::PermissionDenied));
    }
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    Ok(Some(stream))
}

pub(super) fn claim(key: &str) -> Claim {
    let Some((socket, lock)) = paths(key) else { return Claim::Off };
    let Ok(lock) = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&lock)
    else {
        return Claim::Off;
    };
    match lock.try_lock() {
        Ok(()) => {}
        Err(fs::TryLockError::WouldBlock) => return Claim::Taken,
        Err(fs::TryLockError::Error(_)) => return Claim::Off,
    }
    // The lock is ours: a socket file still there is a crashed Gezik's.
    let _ = fs::remove_file(&socket);
    let Ok(listener) = UnixListener::bind(&socket) else { return Claim::Off };
    let _ = fs::set_permissions(&socket, fs::Permissions::from_mode(0o600));
    Claim::Listening(super::Listener(Listener { socket: listener, lock }))
}

/// A call whose reads all end by one deadline: a caller trickling bytes cannot hold its thread.
pub(super) struct Call {
    stream: UnixStream,
    until: Instant,
}

impl Read for Call {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let left = self.until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(io::ErrorKind::TimedOut.into());
        }
        self.stream.set_read_timeout(Some(left))?;
        self.stream.read(buf)
    }
}

impl Write for Call {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stream.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

impl Listener {
    pub(super) fn serve(self, answer: impl Fn(&mut Call) + Send + Sync + 'static) {
        let answer = Arc::new(answer);
        let me = me();
        let calls = Arc::new(AtomicUsize::new(0));
        let _ = thread::Builder::new().name("gezik-instance".into()).spawn(move || {
            let Listener { socket, lock: _lock } = self;
            for stream in socket.incoming() {
                let Ok(stream) = stream else {
                    // Out of descriptors, say: no spinning while it lasts.
                    thread::sleep(Duration::from_millis(50));
                    continue;
                };
                // Someone else's process, or too many calls at once: closed unanswered.
                if peer_uid(&stream) != Some(me) {
                    continue;
                }
                let Some(slot) = CallSlot::take(&calls) else { continue };
                let _ = stream.set_write_timeout(Some(SEND_TIMEOUT));
                let mut call = Call { stream, until: Instant::now() + SEND_TIMEOUT };
                let answer = answer.clone();
                let _ = thread::Builder::new().name("gezik-instance-call".into()).spawn(move || {
                    answer(&mut call);
                    drop(slot);
                });
            }
        });
    }
}
