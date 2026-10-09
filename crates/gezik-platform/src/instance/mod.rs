//! Single instance (spec 5.2): a second `gezik` hands its paths to the running one over a
//! channel only this user can reach (Windows: a named pipe; macOS and Linux: a Unix socket in
//! a private folder). This file holds the message, the channel's key and the checks, pure and
//! tested on every system.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

#[cfg(unix)]
mod unix;
#[cfg(unix)]
use unix as imp;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as imp;
#[cfg(windows)]
pub use windows::bring_to_front;

/// The message's version: a Gezik that speaks another one is not answered (its caller opens
/// a window of its own).
pub const VERSION: u8 = 1;
/// The largest message body, and the most paths in one (spec 5.2).
pub const MAX_MESSAGE: usize = 1 << 20;
pub const MAX_PATHS: usize = 1000;
/// The longest path: bytes on Unix, UTF-16 units on Windows (the systems' own limits).
pub const MAX_PATH_UNITS: usize = 32 * 1024;
/// A reply: the version, 0 for done, the receiver's process id.
pub const REPLY_LEN: usize = 6;
const MAX_TOKEN: usize = 256;
const NEW_TAB: u8 = 1;

/// A path from the command line: a folder to open, or (`select`, or any file) an item to show
/// selected in its folder. Never opened with an app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub path: PathBuf,
    pub select: bool,
}

/// What a second `gezik` asks the running one for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Request {
    pub new_tab: bool,
    /// Absolute: the caller makes them so against its own working folder (spec 5.1).
    pub targets: Vec<Target>,
    /// Wayland's `XDG_ACTIVATION_TOKEN` of the caller, if it had one.
    pub activation_token: Option<String>,
}

/// Why a message was not taken.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BadMessage {
    Version(u8),
    TooLong(usize),
    Truncated,
    TooManyPaths(usize),
    BadFlags(u8),
    BadKind(u8),
    BadPath,
    BadToken,
    Trailing,
}

/// The message for `request`: the header (version, body length) and the body. The caller
/// keeps to `MAX_PATHS` (cli.rs drops the rest).
pub fn encode(request: &Request) -> Vec<u8> {
    debug_assert!(request.targets.len() <= MAX_PATHS);
    let mut body = vec![if request.new_tab { NEW_TAB } else { 0 }];
    body.extend_from_slice(&(request.targets.len().min(MAX_PATHS) as u16).to_le_bytes());
    for target in request.targets.iter().take(MAX_PATHS) {
        body.push(u8::from(target.select));
        let bytes = path_bytes(&target.path);
        body.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        body.extend_from_slice(&bytes);
    }
    let token = request.activation_token.as_deref().unwrap_or("");
    body.extend_from_slice(&(token.len().min(usize::from(u16::MAX)) as u16).to_le_bytes());
    body.extend_from_slice(&token.as_bytes()[..token.len().min(usize::from(u16::MAX))]);
    let mut out = Vec::with_capacity(5 + body.len());
    out.push(VERSION);
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

/// Reads one message's body: the header first, so a length past `MAX_MESSAGE` is refused
/// before anything is allocated.
pub fn read_message(from: &mut impl Read) -> Result<Vec<u8>, BadMessage> {
    let mut head = [0u8; 5];
    from.read_exact(&mut head).map_err(|_| BadMessage::Truncated)?;
    if head[0] != VERSION {
        return Err(BadMessage::Version(head[0]));
    }
    let len = u32::from_le_bytes([head[1], head[2], head[3], head[4]]) as usize;
    if len > MAX_MESSAGE {
        return Err(BadMessage::TooLong(len));
    }
    let mut body = vec![0; len];
    from.read_exact(&mut body).map_err(|_| BadMessage::Truncated)?;
    Ok(body)
}

/// The request in `body`, every part checked (spec 10.6): what fails is refused whole.
pub fn decode(body: &[u8]) -> Result<Request, BadMessage> {
    let mut reader = Reader(body);
    let flags = reader.u8()?;
    if flags & !NEW_TAB != 0 {
        return Err(BadMessage::BadFlags(flags));
    }
    let count = usize::from(reader.u16()?);
    if count > MAX_PATHS {
        return Err(BadMessage::TooManyPaths(count));
    }
    let mut targets = Vec::with_capacity(count);
    for _ in 0..count {
        let select = match reader.u8()? {
            0 => false,
            1 => true,
            kind => return Err(BadMessage::BadKind(kind)),
        };
        let len = reader.u32()? as usize;
        let path = path_from_bytes(reader.take(len)?).ok_or(BadMessage::BadPath)?;
        targets.push(Target { path, select });
    }
    let len = usize::from(reader.u16()?);
    let token = reader.take(len)?;
    if len > MAX_TOKEN || !token.iter().all(u8::is_ascii_graphic) {
        return Err(BadMessage::BadToken);
    }
    if !reader.0.is_empty() {
        return Err(BadMessage::Trailing);
    }
    let activation_token = (!token.is_empty()).then(|| String::from_utf8_lossy(token).into_owned());
    Ok(Request { new_tab: flags & NEW_TAB != 0, targets, activation_token })
}

pub fn reply(pid: u32) -> [u8; REPLY_LEN] {
    let pid = pid.to_le_bytes();
    [VERSION, 0, pid[0], pid[1], pid[2], pid[3]]
}

/// The receiver's process id if `bytes` says done.
pub fn reply_ok(bytes: &[u8; REPLY_LEN]) -> Option<u32> {
    (bytes[0] == VERSION && bytes[1] == 0).then(|| u32::from_le_bytes([bytes[2], bytes[3], bytes[4], bytes[5]]))
}

/// The channel's key: FNV-1a of the config folder's path (spec 5.2), so two portable copies
/// with their own config folders never meet.
pub fn key(config_dir: Option<&Path>) -> String {
    let bytes = config_dir.map(|dir| dir.as_os_str().as_encoded_bytes()).unwrap_or_default();
    let hash = bytes
        .iter()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3));
    format!("{hash:016x}")
}

/// Whether a folder may hold the socket: a real folder (no link), this user's, and nobody
/// else's to read, write or enter.
#[cfg_attr(windows, allow(dead_code))]
pub(crate) fn dir_is_private(is_dir: bool, is_link: bool, owner: u32, mode: u32, me: u32) -> bool {
    is_dir && !is_link && owner == me && mode & 0o077 == 0
}

/// `sun_path` holds 104 bytes on macOS and 108 on Linux, the NUL included.
#[cfg_attr(windows, allow(dead_code))]
const MAX_SOCKET_PATH: usize = 100;

/// The socket and its lock file in `dir`; `None` if the socket's path is too long.
#[cfg_attr(windows, allow(dead_code))]
pub(crate) fn socket_paths(dir: &Path, key: &str) -> Option<(PathBuf, PathBuf)> {
    let socket = dir.join(format!("gezik-{key}.sock"));
    (socket.as_os_str().len() <= MAX_SOCKET_PATH).then(|| (socket, dir.join(format!("gezik-{key}.lock"))))
}

/// How long a second `gezik` waits for the running one before it opens a window of its own.
pub const SEND_TIMEOUT: Duration = Duration::from_secs(2);
/// How long the running Gezik's call thread waits for its window to carry a request out.
pub const CARRY_OUT_TIMEOUT: Duration = Duration::from_millis(1500);

/// How handing a request over went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sent {
    /// The running Gezik did it.
    Delivered,
    /// No Gezik listens for this key: this one may become the first.
    NoInstance,
    /// One is there but did not answer in time, refused, or is not this user's: open a window
    /// of its own and leave the channel alone.
    Failed,
}

/// Whether this Gezik became the one others hand their paths to.
pub enum Claim {
    Listening(Listener),
    /// Another Gezik has the channel (it started at the same moment).
    Taken,
    /// No channel on this system now (no private folder, an error): single instance off.
    Off,
}

/// The channel, taken; [`Listener::serve`] starts answering.
pub struct Listener(imp::Listener);

/// Hands `request` to the Gezik listening for `key` and waits for its answer, at most
/// `timeout`: the talking is on a thread of its own, left behind if the other side hangs.
pub fn send(key: &str, request: &Request, timeout: Duration) -> Sent {
    let (key, message) = (key.to_owned(), encode(request));
    let (done, result) = mpsc::channel();
    let talking = thread::Builder::new().name("gezik-instance-send".into()).spawn(move || {
        let _ = done.send(talk(&key, &message, timeout));
    });
    if talking.is_err() {
        return Sent::Failed;
    }
    result.recv_timeout(timeout).unwrap_or(Sent::Failed)
}

fn talk(key: &str, message: &[u8], timeout: Duration) -> Sent {
    let mut stream = match imp::connect(key, timeout) {
        Ok(Some(stream)) => stream,
        Ok(None) => return Sent::NoInstance,
        Err(_) => return Sent::Failed,
    };
    if stream.write_all(message).is_err() {
        return Sent::Failed;
    }
    let mut answer = [0; REPLY_LEN];
    match stream.read_exact(&mut answer) {
        Ok(()) if reply_ok(&answer).is_some() => Sent::Delivered,
        _ => Sent::Failed,
    }
}

/// Takes the channel for `key`, if no other Gezik has it.
pub fn claim(key: &str) -> Claim {
    imp::claim(key)
}

impl Listener {
    /// Starts the thread that waits for other `gezik` calls (no CPU while none come). Each
    /// call is read on a thread of its own and checked; `on_request` runs there and says
    /// whether the request was carried out: only then is the caller told so, else it opens
    /// its own window. Nothing that comes in is ever run.
    pub fn serve(self, on_request: impl Fn(Request) -> bool + Send + Sync + 'static) {
        let on_request: Arc<dyn Fn(Request) -> bool + Send + Sync> = Arc::new(on_request);
        self.0.serve(move |stream| answer(stream, &*on_request));
    }
}

/// One call: read, check, carry out, answer. A bad message is closed unanswered.
fn answer(stream: &mut (impl Read + Write), on_request: &(dyn Fn(Request) -> bool + Send + Sync)) {
    let Ok(body) = read_message(stream) else { return };
    let Ok(request) = decode(&body) else { return };
    if on_request(request) {
        let _ = stream.write_all(&reply(std::process::id()));
        let _ = stream.flush();
    }
}

/// Lets `--help` and `--version` reach the console Gezik was started from: the Windows
/// release build has none of its own (spec 5.1). Elsewhere nothing to do.
pub fn attach_console() {
    #[cfg(windows)]
    // SAFETY: no pointers; failing (no parent console) only leaves the output unseen.
    unsafe {
        let _ =
            ::windows::Win32::System::Console::AttachConsole(::windows::Win32::System::Console::ATTACH_PARENT_PROCESS);
    }
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], BadMessage> {
        if n > self.0.len() {
            return Err(BadMessage::Truncated);
        }
        let (head, rest) = self.0.split_at(n);
        self.0 = rest;
        Ok(head)
    }

    fn u8(&mut self) -> Result<u8, BadMessage> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, BadMessage> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Result<u32, BadMessage> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
}

#[cfg(unix)]
fn path_bytes(path: &Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    path.as_os_str().as_bytes().to_vec()
}

#[cfg(windows)]
fn path_bytes(path: &Path) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str().encode_wide().flat_map(u16::to_le_bytes).collect()
}

/// A received path, if Gezik may act on it: not empty, no NUL, within the systems' limit,
/// absolute.
#[cfg(unix)]
fn path_from_bytes(bytes: &[u8]) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStrExt;
    if bytes.is_empty() || bytes.len() > MAX_PATH_UNITS || bytes.contains(&0) {
        return None;
    }
    let path = Path::new(std::ffi::OsStr::from_bytes(bytes));
    path.is_absolute().then(|| path.to_path_buf())
}

/// The same on Windows, and none in the device namespaces (`\\.\`, `\\?\`, `\??\`): a drive's
/// raw device or a pipe is no folder to show.
#[cfg(windows)]
fn path_from_bytes(bytes: &[u8]) -> Option<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    if bytes.is_empty() || !bytes.len().is_multiple_of(2) || bytes.len() / 2 > MAX_PATH_UNITS {
        return None;
    }
    let wide: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|pair| u16::from_le_bytes(*pair)).collect();
    if wide.contains(&0) {
        return None;
    }
    let path = PathBuf::from(std::ffi::OsString::from_wide(&wide));
    let start: String = path.to_string_lossy().chars().take(4).map(|c| if c == '/' { '\\' } else { c }).collect();
    if [r"\\.\", r"\\?\", r"\??\"].contains(&start.as_str()) {
        return None;
    }
    path.is_absolute().then_some(path)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    fn test_key(name: &str) -> String {
        format!("t{}{name}", std::process::id())
    }

    fn listening(key: &str) -> Listener {
        match claim(key) {
            Claim::Listening(listener) => listener,
            Claim::Taken => panic!("{key}: taken"),
            Claim::Off => panic!("{key}: no channel on this system"),
        }
    }

    #[test]
    fn a_request_reaches_the_listener_and_is_answered() {
        let key = test_key("reach");
        let (got, received) = mpsc::channel();
        listening(&key).serve(move |request| got.send(request).is_ok());
        assert!(matches!(send(&key, &request(), SEND_TIMEOUT), Sent::Delivered));
        assert_eq!(received.recv_timeout(Duration::from_secs(1)), Ok(request()));
    }

    #[test]
    fn many_calls_one_after_another() {
        let key = test_key("many");
        listening(&key).serve(|_| true);
        for i in 0..20 {
            assert!(matches!(send(&key, &Request::default(), SEND_TIMEOUT), Sent::Delivered), "call {i}");
        }
    }

    #[test]
    fn nothing_listening_is_no_instance() {
        assert!(matches!(send(&test_key("none"), &request(), SEND_TIMEOUT), Sent::NoInstance));
    }

    #[test]
    fn a_second_claim_finds_the_channel_taken() {
        let key = test_key("taken");
        let _first = listening(&key);
        assert!(matches!(claim(&key), Claim::Taken));
    }

    #[test]
    fn a_refused_request_gets_no_answer() {
        let key = test_key("refused");
        listening(&key).serve(|_| false);
        let started = Instant::now();
        assert!(matches!(send(&key, &request(), SEND_TIMEOUT), Sent::Failed));
        assert!(started.elapsed() < Duration::from_secs(1), "closed at once, not waited out");
    }

    #[test]
    fn a_hung_listener_is_given_up_in_time() {
        let key = test_key("hung");
        listening(&key).serve(|_| {
            std::thread::sleep(Duration::from_secs(3));
            true
        });
        let started = Instant::now();
        assert!(matches!(send(&key, &request(), Duration::from_millis(300)), Sent::Failed));
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn a_huge_message_is_closed_unanswered() {
        use std::io::{Read, Write};
        let key = test_key("huge");
        let (got, received) = mpsc::channel();
        listening(&key).serve(move |request| got.send(request).is_ok());
        let mut stream = imp::connect(&key, SEND_TIMEOUT).unwrap().expect("listening");
        let mut head = vec![VERSION];
        head.extend_from_slice(&(2 * MAX_MESSAGE as u32).to_le_bytes());
        stream.write_all(&head).unwrap();
        let mut answer = Vec::new();
        let _ = stream.read_to_end(&mut answer);
        assert!(answer.is_empty());
        assert!(received.recv_timeout(Duration::from_millis(200)).is_err(), "never handed on");
    }

    #[test]
    fn a_silent_caller_is_cut_off() {
        use std::io::Read;
        let key = test_key("silent");
        let (got, received) = mpsc::channel();
        listening(&key).serve(move |request| got.send(request).is_ok());
        let mut stream = imp::connect(&key, Duration::from_secs(10)).unwrap().expect("listening");
        let started = Instant::now();
        let mut answer = Vec::new();
        let _ = stream.read_to_end(&mut answer);
        assert!(answer.is_empty());
        assert!(started.elapsed() < SEND_TIMEOUT * 2, "closed after the listener's own timeout");
        assert!(received.try_recv().is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_crashed_gezik_socket_is_replaced() {
        let key = test_key("stale");
        let (socket, _) = unix::paths(&key).expect("a private folder");
        drop(std::os::unix::net::UnixListener::bind(&socket).unwrap()); // its file stays, nobody listens
        assert!(matches!(send(&key, &request(), SEND_TIMEOUT), Sent::NoInstance));
        listening(&key).serve(|_| true);
        assert!(matches!(send(&key, &request(), SEND_TIMEOUT), Sent::Delivered));
    }

    fn abs(name: &str) -> PathBuf {
        if cfg!(windows) { PathBuf::from(format!(r"C:\{name}")) } else { PathBuf::from(format!("/{name}")) }
    }

    fn request() -> Request {
        Request {
            new_tab: true,
            targets: vec![
                Target { path: abs("work dir"), select: false },
                Target { path: abs("çalışma/notlar.txt"), select: true },
            ],
            activation_token: Some("gezik-123_abc".to_owned()),
        }
    }

    /// The body of `encode`, header taken off.
    fn body_of(request: &Request) -> Vec<u8> {
        let mut bytes = encode(request);
        read_message(&mut Cursor::new(std::mem::take(&mut bytes))).unwrap()
    }

    /// A body with one path of `bytes` (kind 0), no token.
    fn one_path(bytes: &[u8]) -> Vec<u8> {
        let mut body = vec![0, 1, 0, 0];
        body.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        body.extend_from_slice(bytes);
        body.extend_from_slice(&[0, 0]);
        body
    }

    #[cfg(unix)]
    fn raw(path: &Path) -> Vec<u8> {
        use std::os::unix::ffi::OsStrExt;
        path.as_os_str().as_bytes().to_vec()
    }

    #[cfg(windows)]
    fn raw(path: &Path) -> Vec<u8> {
        use std::os::windows::ffi::OsStrExt;
        path.as_os_str().encode_wide().flat_map(u16::to_le_bytes).collect()
    }

    #[test]
    fn a_request_round_trips() {
        assert_eq!(decode(&body_of(&request())), Ok(request()));
        let bare = Request::default();
        assert_eq!(decode(&body_of(&bare)), Ok(bare), "a bare `gezik` (Win+E later) is a request too");
    }

    #[test]
    fn the_header_says_the_version_and_the_length() {
        let bytes = encode(&request());
        assert_eq!(bytes[0], VERSION);
        let len = u32::from_le_bytes(bytes[1..5].try_into().unwrap()) as usize;
        assert_eq!(len, bytes.len() - 5);
    }

    #[test]
    fn a_huge_length_is_refused_before_reading() {
        let mut head = Cursor::new(vec![VERSION, 0xff, 0xff, 0xff, 0x7f]);
        assert_eq!(read_message(&mut head), Err(BadMessage::TooLong(0x7fff_ffff)));
        let mut just_over = vec![VERSION];
        just_over.extend_from_slice(&((MAX_MESSAGE + 1) as u32).to_le_bytes());
        assert_eq!(read_message(&mut Cursor::new(just_over)), Err(BadMessage::TooLong(MAX_MESSAGE + 1)));
    }

    #[test]
    fn short_and_long_messages_are_refused() {
        assert_eq!(read_message(&mut Cursor::new(vec![VERSION, 9, 0])), Err(BadMessage::Truncated));
        assert_eq!(read_message(&mut Cursor::new(vec![VERSION, 9, 0, 0, 0, 1])), Err(BadMessage::Truncated));
        let mut body = body_of(&request());
        body.push(0);
        assert_eq!(decode(&body), Err(BadMessage::Trailing));
        let body = body_of(&request());
        assert_eq!(decode(&body[..body.len() - 1]), Err(BadMessage::Truncated));
    }

    #[test]
    fn another_version_is_refused() {
        let mut bytes = encode(&request());
        bytes[0] = VERSION + 1;
        assert_eq!(read_message(&mut Cursor::new(bytes)), Err(BadMessage::Version(VERSION + 1)));
    }

    #[test]
    fn at_most_a_thousand_paths() {
        let many = Request { targets: vec![Target { path: abs("a"), select: false }; MAX_PATHS], ..Request::default() };
        assert_eq!(decode(&body_of(&many)).map(|r| r.targets.len()), Ok(MAX_PATHS));
        let mut body = body_of(&many);
        body[1..3].copy_from_slice(&((MAX_PATHS + 1) as u16).to_le_bytes());
        assert_eq!(decode(&body), Err(BadMessage::TooManyPaths(MAX_PATHS + 1)));
    }

    #[test]
    fn unknown_flags_and_kinds_are_refused() {
        let mut body = body_of(&request());
        body[0] = 0b10;
        assert_eq!(decode(&body), Err(BadMessage::BadFlags(0b10)));
        let mut body = one_path(&raw(&abs("a")));
        body[3] = 7;
        assert_eq!(decode(&body), Err(BadMessage::BadKind(7)));
    }

    #[test]
    fn paths_gezik_must_not_act_on_are_refused() {
        assert_eq!(decode(&one_path(&raw(&abs("ok")))).map(|r| r.targets[0].path.clone()), Ok(abs("ok")));
        assert_eq!(decode(&one_path(b"")), Err(BadMessage::BadPath), "empty");
        assert_eq!(decode(&one_path(&raw(Path::new("relative/dir")))), Err(BadMessage::BadPath), "relative");
        let mut nul = raw(&abs("a"));
        nul.extend_from_slice(&raw(Path::new("\0b")));
        assert_eq!(decode(&one_path(&nul)), Err(BadMessage::BadPath), "NUL");
        let long = raw(&abs(&"x".repeat(MAX_PATH_UNITS + 1)));
        assert_eq!(decode(&one_path(&long)), Err(BadMessage::BadPath), "too long");
    }

    #[cfg(windows)]
    #[test]
    fn device_paths_are_refused() {
        for path in [r"\\.\PhysicalDrive0", r"\\?\C:\x", r"\??\C:\x", r"//./pipe/x", r"\\.\pipe\gezik"] {
            assert_eq!(decode(&one_path(&raw(Path::new(path)))), Err(BadMessage::BadPath), "{path}");
        }
        assert_eq!(decode(&one_path(&[0x41, 0x00, 0x3a])), Err(BadMessage::BadPath), "odd byte count");
        let unc = Path::new(r"\\server\share\dir");
        assert_eq!(decode(&one_path(&raw(unc))).map(|r| r.targets[0].path.clone()), Ok(unc.to_path_buf()));
    }

    #[cfg(windows)]
    #[test]
    fn a_name_windows_allows_but_utf8_does_not_round_trips() {
        use std::os::windows::ffi::OsStringExt;
        let mut wide: Vec<u16> = r"C:\a".encode_utf16().collect();
        wide.push(0xd800);
        let path = PathBuf::from(std::ffi::OsString::from_wide(&wide));
        let request = Request { targets: vec![Target { path, select: false }], ..Request::default() };
        assert_eq!(decode(&body_of(&request)), Ok(request));
    }

    #[cfg(unix)]
    #[test]
    fn a_name_unix_allows_but_utf8_does_not_round_trips() {
        use std::os::unix::ffi::OsStrExt;
        let path = Path::new(std::ffi::OsStr::from_bytes(b"/tmp/\xff\xfe")).to_path_buf();
        let request = Request { targets: vec![Target { path, select: true }], ..Request::default() };
        assert_eq!(decode(&body_of(&request)), Ok(request));
    }

    #[test]
    fn the_activation_token_is_plain_text_only() {
        for bad in ["tök", "a b", "a\nb", &"x".repeat(257)] {
            let request = Request { activation_token: Some(bad.to_owned()), ..Request::default() };
            assert_eq!(decode(&body_of(&request)), Err(BadMessage::BadToken), "{bad:?}");
        }
    }

    #[test]
    fn garbage_never_panics() {
        // Random bodies, and a valid one with each byte changed in turn.
        let mut seed: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut next = || {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            (seed >> 33) as u8
        };
        for _ in 0..20_000 {
            let len = usize::from(next() % 64);
            let body: Vec<u8> = (0..len).map(|_| next()).collect();
            let _ = decode(&body);
            let mut framed = vec![VERSION];
            framed.extend_from_slice(&body);
            let _ = read_message(&mut Cursor::new(framed));
        }
        let valid = body_of(&request());
        for i in 0..valid.len() {
            for value in [0, 1, 0x7f, 0xff] {
                let mut body = valid.clone();
                body[i] = value;
                let _ = decode(&body);
            }
        }
    }

    #[test]
    fn a_reply_says_done_and_who() {
        assert_eq!(reply_ok(&reply(4242)), Some(4242));
        let mut refused = reply(1);
        refused[1] = 1;
        assert_eq!(reply_ok(&refused), None);
        let mut other = reply(1);
        other[0] = VERSION + 1;
        assert_eq!(reply_ok(&other), None);
    }

    #[test]
    fn the_key_follows_the_config_folder() {
        let a = key(Some(Path::new("/home/u/.config/gezik")));
        assert_eq!(a.len(), 16);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(a, key(Some(Path::new("/home/u/.config/gezik"))), "stable");
        assert_ne!(a, key(Some(Path::new("/media/usb/gezik/config"))), "a portable copy has its own");
        assert_eq!(key(None).len(), 16);
    }

    #[test]
    fn only_a_private_folder_holds_the_socket() {
        assert!(dir_is_private(true, false, 1000, 0o700, 1000));
        assert!(dir_is_private(true, false, 1000, 0o40700, 1000), "the file type bits do not matter");
        assert!(!dir_is_private(true, false, 1000, 0o755, 1000), "others can look in");
        assert!(!dir_is_private(true, false, 1000, 0o1777, 1000), "/tmp itself");
        assert!(!dir_is_private(true, false, 0, 0o700, 1000), "someone else's");
        assert!(!dir_is_private(true, true, 1000, 0o700, 1000), "a link");
        assert!(!dir_is_private(false, false, 1000, 0o700, 1000), "a file");
    }

    #[test]
    fn a_socket_path_must_fit_the_system() {
        let (socket, lock) = socket_paths(Path::new("/run/user/1000"), "0123456789abcdef").unwrap();
        assert_eq!(socket, Path::new("/run/user/1000/gezik-0123456789abcdef.sock"));
        assert_eq!(lock, Path::new("/run/user/1000/gezik-0123456789abcdef.lock"));
        assert!(socket_paths(Path::new(&format!("/{}", "d".repeat(90))), "0123456789abcdef").is_none());
    }
}
