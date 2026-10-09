//! Administrator operations (spec 9 §10): the list the one-shot helper does, how it travels on
//! its command line, what the helper accepts, its replies, and the undo of each. Pure: Gezik,
//! the helper and the tests share every rule, so Gezik refuses before the prompt what the helper
//! would refuse after it.

mod quote;

use std::path::{Path, PathBuf};

use crate::attrs::{Attrs, HIDDEN, LOCKED, Wanted, flags_first};
use crate::ops::paths::is_within;

pub use quote::{applescript, osascript_source, sh_word, windows_arg, windows_command_line};

/// The helper's flag (spec §5.1: internal), then `VERSION`, the channel, the operations.
pub const ARG: &str = "--elevated";
pub const VERSION: &str = "1";
/// macOS and Linux: replies on standard output.
pub const STDOUT: &str = "-";
/// Windows: the result pipe is this and 32 lowercase hex digits.
pub const PIPE_PREFIX: &str = r"\\.\pipe\gezik-elev-";
/// macOS and Linux: all the arguments together, and each one (spec §10.3).
pub const UNIX_TOTAL: usize = 128 * 1024;
pub const MAX_ARG: usize = 32 * 1024;
/// Windows: a command line in UTF-16 units, its terminating NUL included.
pub const WINDOWS_TOTAL: usize = 32_767;
/// A reply's message, in characters.
pub const MAX_MESSAGE: usize = 200;
/// One name in a path, in bytes (Windows: UTF-16 units).
pub const MAX_NAME: usize = 255;

pub const TOO_MANY: &str = "Too many items for one administrator operation; select fewer.";
pub const NOT_ELEVATED: &str = "The helper is not running as administrator";
pub const STOPPED: &str = "The administrator helper stopped before it said how this went";
pub const SKIPPED: &str = "The administrator helper did not do it";
pub const NOT_UNICODE: &str = "A name that is not Unicode";
pub const NOT_FULL: &str = "Not a full path";
pub const NOT_PLAIN: &str = "Not a plain path (., .. or an empty part)";
pub const DEVICE: &str = "A device path, a network path or a stream name";
pub const RESERVED: &str = "A name Windows keeps for devices";
pub const CONTROL: &str = "A control character in a name";
pub const BAD_CHAR: &str = "A character Windows does not allow in names";
pub const TOO_LONG: &str = "A path or name that is too long";
pub const GUARDED: &str = "A drive, a system folder or a home folder itself; Gezik does not change it as administrator";
pub const BAD_NAME: &str = "Not a single name";
pub const MODE: &str = "Permissions Gezik does not set as administrator (setuid, setgid)";
pub const UNIX_ONLY: &str = "Permissions and owners are changed in Windows' Properties";
pub const MAC_ONLY: &str = "Hidden and Locked are macOS flags";
pub const FLAGS: &str = "Only Hidden and Locked are changed";
pub const INTO_ITSELF: &str = "A folder into itself";
pub const NOTHING: &str = "Nothing to do";
pub const WRONG_VERSION: &str = "A list for another version of Gezik";
pub const MALFORMED: &str = "A list Gezik did not write";
pub const DONE: &str = "done";

/// One thing the helper does (spec §10.3; `Rmdir` is the undo of `Mkdir`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    /// A file, link or folder; a folder that is there is merged into. `replace`: files that are
    /// there are replaced (else each fails); a folder is never replaced.
    Copy {
        from: PathBuf,
        to: PathBuf,
        replace: bool,
    },
    /// Renames, or copies then deletes across drives. `replace`: a file there is replaced.
    Move {
        from: PathBuf,
        to: PathBuf,
        replace: bool,
    },
    /// For good, a folder with everything in it; a link is removed, never followed.
    Delete(PathBuf),
    /// In its folder; a taken name fails.
    Rename {
        path: PathBuf,
        name: String,
    },
    Mkdir(PathBuf),
    /// An empty folder only.
    Rmdir(PathBuf),
    /// Unix: permissions, never setuid or setgid.
    Chmod {
        path: PathBuf,
        mode: u32,
    },
    Chown {
        path: PathBuf,
        uid: u32,
        gid: u32,
    },
    /// macOS: `HIDDEN` and `LOCKED` set and cleared.
    Chflags {
        path: PathBuf,
        set: u32,
        clear: u32,
    },
}

impl Op {
    /// The item it acts on: what a failure names.
    pub fn path(&self) -> &Path {
        match self {
            Op::Copy { from, .. } | Op::Move { from, .. } => from,
            Op::Delete(path) | Op::Mkdir(path) | Op::Rmdir(path) => path,
            Op::Rename { path, .. } | Op::Chmod { path, .. } | Op::Chown { path, .. } | Op::Chflags { path, .. } => {
                path
            }
        }
    }

    /// Where a copy or move lands.
    pub fn target(&self) -> Option<&Path> {
        match self {
            Op::Copy { to, .. } | Op::Move { to, .. } => Some(to),
            _ => None,
        }
    }

    fn paths(&self) -> impl Iterator<Item = &Path> {
        std::iter::once(self.path()).chain(self.target())
    }

    /// The same with each path through `f` (Gezik resolves the folders before the prompt).
    pub fn map_paths(self, f: impl Fn(PathBuf) -> PathBuf) -> Op {
        match self {
            Op::Copy { from, to, replace } => Op::Copy { from: f(from), to: f(to), replace },
            Op::Move { from, to, replace } => Op::Move { from: f(from), to: f(to), replace },
            Op::Delete(path) => Op::Delete(f(path)),
            Op::Rename { path, name } => Op::Rename { path: f(path), name },
            Op::Mkdir(path) => Op::Mkdir(f(path)),
            Op::Rmdir(path) => Op::Rmdir(f(path)),
            Op::Chmod { path, mode } => Op::Chmod { path: f(path), mode },
            Op::Chown { path, uid, gid } => Op::Chown { path: f(path), uid, gid },
            Op::Chflags { path, set, clear } => Op::Chflags { path: f(path), set, clear },
        }
    }

    /// Its words on the command line; `None` if a path is not Unicode.
    fn words(&self) -> Option<Vec<String>> {
        let s = |path: &Path| path.to_str().map(str::to_owned);
        Some(match self {
            Op::Copy { from, to, replace } => {
                vec![if *replace { "copy-replace" } else { "copy" }.to_owned(), s(from)?, s(to)?]
            }
            Op::Move { from, to, replace } => {
                vec![if *replace { "move-replace" } else { "move" }.to_owned(), s(from)?, s(to)?]
            }
            Op::Delete(path) => vec!["delete".to_owned(), s(path)?],
            Op::Rename { path, name } => vec!["rename".to_owned(), s(path)?, name.clone()],
            Op::Mkdir(path) => vec!["mkdir".to_owned(), s(path)?],
            Op::Rmdir(path) => vec!["rmdir".to_owned(), s(path)?],
            Op::Chmod { path, mode } => vec!["chmod".to_owned(), s(path)?, format!("{mode:o}")],
            Op::Chown { path, uid, gid } => vec!["chown".to_owned(), s(path)?, uid.to_string(), gid.to_string()],
            Op::Chflags { path, set, clear } => {
                vec!["chflags".to_owned(), s(path)?, flag_words(*set), flag_words(*clear)]
            }
        })
    }
}

/// `hidden,uchg`, or `-` for none.
fn flag_words(bits: u32) -> String {
    let names: Vec<&str> =
        [(HIDDEN, "hidden"), (LOCKED, "uchg")].into_iter().filter(|(bit, _)| bits & bit != 0).map(|(_, n)| n).collect();
    if names.is_empty() { "-".to_owned() } else { names.join(",") }
}

fn parse_flags(word: &str) -> Option<u32> {
    if word == "-" {
        return Some(0);
    }
    word.split(',').try_fold(0, |bits, name| match name {
        "hidden" => Some(bits | HIDDEN),
        "uchg" => Some(bits | LOCKED),
        _ => None,
    })
}

fn octal(word: &str) -> Result<u32, &'static str> {
    if word.is_empty() || word.len() > 5 || !word.bytes().all(|b| (b'0'..=b'7').contains(&b)) {
        return Err(MALFORMED);
    }
    u32::from_str_radix(word, 8).map_err(|_| MALFORMED)
}

fn number(word: &str) -> Result<u32, &'static str> {
    if word.is_empty() || word.len() > 10 || !word.bytes().all(|b| b.is_ascii_digit()) {
        return Err(MALFORMED);
    }
    word.parse().map_err(|_| MALFORMED)
}

/// The helper's arguments (after the program): `ARG`, `VERSION`, `channel`, the operations.
pub fn encode(channel: &str, ops: &[Op]) -> Option<Vec<String>> {
    let mut args = vec![ARG.to_owned(), VERSION.to_owned(), channel.to_owned()];
    for op in ops {
        args.extend(op.words()?);
    }
    Some(args)
}

/// The channel of a helper's arguments, before anything else is read.
pub fn channel_of(args: &[String]) -> Option<&str> {
    match args {
        [arg, version, channel, ..] if arg == ARG && version == VERSION => Some(channel),
        _ => None,
    }
}

/// The channel and the operations; anything Gezik would not have written is an error.
pub fn decode(args: &[String]) -> Result<(String, Vec<Op>), &'static str> {
    let [arg, version, channel, rest @ ..] = args else { return Err(MALFORMED) };
    if arg != ARG {
        return Err(MALFORMED);
    }
    if version != VERSION {
        return Err(WRONG_VERSION);
    }
    let mut words = rest.iter().map(String::as_str);
    let mut ops = Vec::new();
    while let Some(word) = words.next() {
        let mut next = || words.next().ok_or(MALFORMED);
        let op = match word {
            "copy" | "copy-replace" => {
                Op::Copy { from: next()?.into(), to: next()?.into(), replace: word == "copy-replace" }
            }
            "move" | "move-replace" => {
                Op::Move { from: next()?.into(), to: next()?.into(), replace: word == "move-replace" }
            }
            "delete" => Op::Delete(next()?.into()),
            "rename" => Op::Rename { path: next()?.into(), name: next()?.to_owned() },
            "mkdir" => Op::Mkdir(next()?.into()),
            "rmdir" => Op::Rmdir(next()?.into()),
            "chmod" => Op::Chmod { path: next()?.into(), mode: octal(next()?)? },
            "chown" => Op::Chown { path: next()?.into(), uid: number(next()?)?, gid: number(next()?)? },
            "chflags" => Op::Chflags {
                path: next()?.into(),
                set: parse_flags(next()?).ok_or(MALFORMED)?,
                clear: parse_flags(next()?).ok_or(MALFORMED)?,
            },
            _ => return Err(MALFORMED),
        };
        ops.push(op);
    }
    if ops.is_empty() {
        return Err(NOTHING);
    }
    Ok((channel.clone(), ops))
}

/// The system a list is checked for, and the folders never changed themselves.
pub struct System<'a> {
    pub windows: bool,
    pub macos: bool,
    pub protected: &'a [PathBuf],
}

impl System<'_> {
    /// Windows and macOS (by default) do not tell `A` from `a` in names.
    fn fold(&self, text: &str) -> String {
        if self.windows || self.macos { text.to_lowercase() } else { text.to_owned() }
    }
}

/// Every rule of spec §10.4 for each operation; the first one broken, with its place. Nothing of
/// a list runs unless all of it passes (the helper checks it again before anything).
pub fn check(ops: &[Op], sys: &System<'_>) -> Result<(), (usize, &'static str)> {
    if ops.is_empty() {
        return Err((0, NOTHING));
    }
    for (index, op) in ops.iter().enumerate() {
        check_op(op, sys).map_err(|why| (index, why))?;
    }
    Ok(())
}

fn check_op(op: &Op, sys: &System<'_>) -> Result<(), &'static str> {
    for path in op.paths() {
        check_path(path, sys)?;
    }
    // Copying a system folder somewhere else only reads it; anything else changes the path itself.
    let read = match op {
        Op::Copy { from, .. } => Some(from.as_path()),
        _ => None,
    };
    if op.paths().any(|path| Some(path) != read && guarded(path, sys)) {
        return Err(GUARDED);
    }
    match op {
        Op::Copy { from, to, .. } | Op::Move { from, to, .. } if is_within(to, from) => Err(INTO_ITSELF),
        Op::Rename { name, .. } => check_name(name, sys.windows),
        Op::Chmod { .. } | Op::Chown { .. } if sys.windows => Err(UNIX_ONLY),
        Op::Chmod { mode, .. } if *mode > 0o1777 => Err(MODE),
        Op::Chflags { .. } if !sys.macos => Err(MAC_ONLY),
        Op::Chflags { set, clear, .. } if (set | clear) & !(HIDDEN | LOCKED) != 0 || set & clear != 0 => Err(FLAGS),
        _ => Ok(()),
    }
}

fn check_path(path: &Path, sys: &System<'_>) -> Result<(), &'static str> {
    let text = path.to_str().ok_or(NOT_UNICODE)?;
    if text.len() > MAX_ARG {
        return Err(TOO_LONG);
    }
    if text.chars().any(char::is_control) {
        return Err(CONTROL);
    }
    if sys.windows { windows_path(text) } else { unix_path(text) }
}

fn unix_path(text: &str) -> Result<(), &'static str> {
    let rest = text.strip_prefix('/').ok_or(NOT_FULL)?;
    if rest.is_empty() {
        return Ok(());
    }
    for part in rest.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            return Err(NOT_PLAIN);
        }
        if part.len() > MAX_NAME {
            return Err(TOO_LONG);
        }
    }
    Ok(())
}

fn windows_path(text: &str) -> Result<(), &'static str> {
    // `\\?\` and `\\.\` reach devices and skip Windows' own checks; `\\server` is the network.
    if text.starts_with(r"\\") {
        return Err(DEVICE);
    }
    let bytes = text.as_bytes();
    if !(bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\') {
        return Err(NOT_FULL);
    }
    let rest = &text[3..];
    if rest.is_empty() {
        return Ok(());
    }
    rest.split('\\').try_for_each(|part| windows_name(part, NOT_PLAIN))
}

/// One Windows name: no stream (`a:b`), no device name, nothing Windows would change or refuse.
fn windows_name(part: &str, empty: &'static str) -> Result<(), &'static str> {
    if part.is_empty() || part == "." || part == ".." {
        return Err(empty);
    }
    if part.contains(':') {
        return Err(DEVICE);
    }
    if part.contains(['<', '>', '"', '/', '|', '?', '*']) || part.ends_with(['.', ' ']) {
        return Err(BAD_CHAR);
    }
    if part.encode_utf16().count() > MAX_NAME {
        return Err(TOO_LONG);
    }
    let stem = part.split('.').next().unwrap_or_default().trim_end().to_ascii_uppercase();
    let numbered = |prefix: &str| {
        stem.strip_prefix(prefix)
            .is_some_and(|n| matches!(n, "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"))
    };
    if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$")
        || numbered("COM")
        || numbered("LPT")
    {
        return Err(RESERVED);
    }
    Ok(())
}

fn check_name(name: &str, windows: bool) -> Result<(), &'static str> {
    if name.chars().any(char::is_control) {
        return Err(CONTROL);
    }
    if name.contains('/') || (windows && name.contains('\\')) {
        return Err(BAD_NAME);
    }
    if windows {
        return windows_name(name, BAD_NAME);
    }
    if name.is_empty() || name == "." || name == ".." {
        return Err(BAD_NAME);
    }
    if name.len() > MAX_NAME { Err(TOO_LONG) } else { Ok(()) }
}

/// A drive or file system root, a listed system folder, or a home folder itself.
fn guarded(path: &Path, sys: &System<'_>) -> bool {
    let Some(text) = path.to_str() else { return true };
    let me = sys.fold(text);
    let root = if sys.windows { me.len() <= 3 } else { me == "/" };
    let listed = sys.protected.iter().any(|p| p.to_str().is_some_and(|p| sys.fold(p) == me));
    let home = match path.parent().and_then(Path::to_str).map(|parent| sys.fold(parent)).as_deref() {
        Some(parent) if sys.windows => parent.len() == 8 && parent.ends_with(r":\users"),
        Some(parent) => parent == "/home" || parent == "/users",
        None => false,
    };
    root || listed || home
}

/// The folders the helper never changes themselves, on macOS (`macos`) or Linux.
pub fn unix_protected(macos: bool) -> Vec<PathBuf> {
    const COMMON: [&str; 18] = [
        "/bin", "/boot", "/dev", "/etc", "/home", "/lib", "/lib32", "/lib64", "/libx32", "/opt", "/proc", "/root",
        "/run", "/sbin", "/sys", "/tmp", "/usr", "/var",
    ];
    const MAC: [&str; 11] = [
        "/Applications",
        "/Library",
        "/System",
        "/Users",
        "/Volumes",
        "/private",
        "/private/etc",
        "/private/tmp",
        "/private/var",
        "/var/root",
        "/cores",
    ];
    COMMON.iter().chain(if macos { &MAC[..] } else { &[] }).map(PathBuf::from).collect()
}

/// The same on Windows, from the Windows folder (`GetSystemWindowsDirectoryW`).
pub fn windows_protected(windows_dir: &str) -> Vec<PathBuf> {
    let drive = windows_dir.get(..3).unwrap_or(r"C:\");
    let mut list = vec![PathBuf::from(windows_dir)];
    list.extend(
        ["Program Files", "Program Files (x86)", "ProgramData", "Users"]
            .map(|name| PathBuf::from(format!("{drive}{name}"))),
    );
    list
}

/// Whether `ops` fit on one command line with `exe` (spec §10.2: one prompt, never split).
pub fn fits(exe: &str, ops: &[Op], windows: bool) -> bool {
    let channel = if windows { format!("{PIPE_PREFIX}{}", "0".repeat(32)) } else { STDOUT.to_owned() };
    let Some(args) = encode(&channel, ops) else { return false };
    if windows {
        format!("{} {}", windows_arg(exe), windows_command_line(&args)).encode_utf16().count() < WINDOWS_TOTAL
    } else {
        args.iter().all(|arg| arg.len() <= MAX_ARG)
            && exe.len() + args.iter().map(|arg| arg.len() + 1).sum::<usize>() <= UNIX_TOTAL
    }
}

/// Whether the helper may write to `channel`: standard output, or (Windows) a pipe of Gezik's.
pub fn valid_channel(channel: &str, windows: bool) -> bool {
    if !windows {
        return channel == STDOUT;
    }
    channel
        .strip_prefix(PIPE_PREFIX)
        .is_some_and(|hex| hex.len() == 32 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
}

/// A line the helper writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    Ok(usize),
    Err {
        index: usize,
        code: i32,
        message: String,
    },
    /// The whole list, before anything was done.
    Refused(String),
    Done,
}

pub fn ok_line(index: usize) -> String {
    format!("ok {index}")
}

pub fn err_line(index: usize, code: i32, message: &str) -> String {
    format!("err {index} {code} {}", clean(message))
}

pub fn refused_line(why: &str) -> String {
    format!("refused {}", clean(why))
}

/// One line of words: control characters as spaces, at most `MAX_MESSAGE` characters.
fn clean(text: &str) -> String {
    let line: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).take(MAX_MESSAGE).collect();
    line.trim().to_owned()
}

fn index(word: &str) -> Option<usize> {
    if word.is_empty() || word.len() > 7 || !word.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    word.parse().ok()
}

/// A line from the helper; anything else is `None` (and ignored).
pub fn parse_reply(line: &str) -> Option<Reply> {
    let line = line.strip_suffix('\r').unwrap_or(line);
    if line == DONE {
        return Some(Reply::Done);
    }
    if let Some(why) = line.strip_prefix("refused ") {
        return Some(Reply::Refused(clean(why)));
    }
    if let Some(rest) = line.strip_prefix("ok ") {
        return index(rest).map(Reply::Ok);
    }
    let mut parts = line.strip_prefix("err ")?.splitn(3, ' ');
    let index = index(parts.next()?)?;
    let code = parts.next()?.parse().ok()?;
    Some(Reply::Err { index, code, message: clean(parts.next().unwrap_or_default()) })
}

/// What became of one operation, by the helper's word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    Done,
    Failed(String),
    NotRun(String),
}

/// Each operation's answer: the first reply for it counts (later ones, and replies for no
/// operation, are dropped); a refused list ran nothing; one with no reply was skipped (after
/// `done`) or is not known (the helper stopped).
pub fn answers(count: usize, replies: &[Reply]) -> Vec<Answer> {
    let refused = replies.iter().find_map(|reply| match reply {
        Reply::Refused(why) => Some(why),
        _ => None,
    });
    if let Some(why) = refused {
        return vec![Answer::NotRun(format!("Not done: {why}")); count];
    }
    let mut out: Vec<Option<Answer>> = vec![None; count];
    for reply in replies {
        let (at, answer) = match reply {
            Reply::Ok(at) => (*at, Answer::Done),
            Reply::Err { index, message, .. } => {
                (*index, Answer::Failed(if message.is_empty() { "Failed".to_owned() } else { message.clone() }))
            }
            _ => continue,
        };
        if let Some(slot) = out.get_mut(at)
            && slot.is_none()
        {
            *slot = Some(answer);
        }
    }
    let silent = if replies.contains(&Reply::Done) { SKIPPED } else { STOPPED };
    out.into_iter().map(|answer| answer.unwrap_or_else(|| Answer::NotRun(silent.to_owned()))).collect()
}

/// Why the helper did not run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchError {
    /// The user said no at the system's prompt.
    Cancelled,
    /// The system has no way to ask (no pkexec).
    Unavailable(String),
    Failed(String),
}

/// What Gezik saw (without rights) before the helper ran: what an undo may rely on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Before {
    Absent,
    Present,
    Attrs(Attrs),
    /// Gezik could not look.
    Unknown,
}

/// The operation that undoes `op` (spec §10.2 step 5), if it can be undone: what a copy or move
/// made only if nothing was there before; a delete or a replaced file never.
pub fn undo_of(op: &Op, before: Before) -> Option<Op> {
    match (op, before) {
        (Op::Copy { to, .. }, Before::Absent) => Some(Op::Delete(to.clone())),
        (Op::Move { from, to, .. }, Before::Absent) => {
            Some(Op::Move { from: to.clone(), to: from.clone(), replace: false })
        }
        (Op::Rename { path, name }, _) => {
            Some(Op::Rename { path: path.with_file_name(name), name: path.file_name()?.to_str()?.to_owned() })
        }
        (Op::Mkdir(path), _) => Some(Op::Rmdir(path.clone())),
        (Op::Rmdir(path), _) => Some(Op::Mkdir(path.clone())),
        (Op::Chmod { path, .. }, Before::Attrs(was)) if was.mode & 0o7777 <= 0o1777 => {
            Some(Op::Chmod { path: path.clone(), mode: was.mode & 0o7777 })
        }
        (Op::Chown { path, .. }, Before::Attrs(was)) => {
            Some(Op::Chown { path: path.clone(), uid: was.uid, gid: was.gid })
        }
        (Op::Chflags { path, set, clear }, Before::Attrs(was)) => {
            let both = set | clear;
            Some(Op::Chflags { path: path.clone(), set: both & was.flags, clear: both & !was.flags })
        }
        _ => None,
    }
}

/// The Info window's refused changes as operations, item by item: the flags first when Locked
/// comes off (it blocks the rest), else last (`attrs::flags_first`).
pub fn attr_ops(wanted: &[Wanted]) -> Vec<Op> {
    let mut ops = Vec::new();
    for w in wanted {
        let changed = (w.from.flags ^ w.to.flags) & (HIDDEN | LOCKED);
        let flags = (changed != 0).then(|| Op::Chflags {
            path: w.path.clone(),
            set: w.to.flags & changed,
            clear: w.from.flags & changed,
        });
        let first = flags_first(w.from, w.to);
        if first {
            ops.extend(flags.clone());
        }
        if (w.from.uid, w.from.gid) != (w.to.uid, w.to.gid) {
            ops.push(Op::Chown { path: w.path.clone(), uid: w.to.uid, gid: w.to.gid });
        }
        if w.from.mode & 0o7777 != w.to.mode & 0o7777 {
            ops.push(Op::Chmod { path: w.path.clone(), mode: w.to.mode & 0o7777 });
        }
        if !first {
            ops.extend(flags);
        }
    }
    ops
}

/// What the job, its Undo and the history say: `Copy 3 items as administrator`.
pub fn label(ops: &[Op]) -> String {
    let items = |n: usize| if n == 1 { "1 item".to_owned() } else { format!("{n} items") };
    let all = |f: fn(&Op) -> bool| !ops.is_empty() && ops.iter().all(f);
    let what = if all(|op| matches!(op, Op::Copy { .. })) {
        format!("Copy {}", items(ops.len()))
    } else if all(|op| matches!(op, Op::Move { .. })) {
        format!("Move {}", items(ops.len()))
    } else if all(|op| matches!(op, Op::Delete(_))) {
        format!("Delete {} permanently", items(ops.len()))
    } else if all(|op| matches!(op, Op::Rename { .. })) {
        if ops.len() == 1 { "Rename".to_owned() } else { format!("Rename {}", items(ops.len())) }
    } else if all(|op| matches!(op, Op::Mkdir(_))) {
        "New folder".to_owned()
    } else if all(|op| matches!(op, Op::Chmod { .. } | Op::Chown { .. } | Op::Chflags { .. })) {
        let mut paths: Vec<&Path> = ops.iter().map(Op::path).collect();
        paths.dedup();
        format!("Change {}", items(paths.len()))
    } else {
        format!("{} changes", ops.len())
    };
    format!("{what} as administrator")
}

/// The line of macOS's password prompt.
pub fn prompt(ops: &[Op]) -> String {
    format!("Gezik: {}.", label(ops))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attrs::Identity;

    fn p(text: &str) -> PathBuf {
        PathBuf::from(text)
    }

    fn sys(protected: &[PathBuf]) -> System<'_> {
        System { windows: false, macos: false, protected }
    }

    fn all_ops() -> Vec<Op> {
        vec![
            Op::Copy { from: p("/a/b c"), to: p("/d/b c"), replace: false },
            Op::Copy { from: p("/a/ğ"), to: p("/d/ğ"), replace: true },
            Op::Move { from: p("/a/x"), to: p("/d/x"), replace: false },
            Op::Move { from: p("/a/y"), to: p("/d/y"), replace: true },
            Op::Delete(p("/d/old")),
            Op::Rename { path: p("/d/a.txt"), name: "b c.txt".into() },
            Op::Mkdir(p("/d/new")),
            Op::Rmdir(p("/d/new")),
            Op::Chmod { path: p("/d/f"), mode: 0o1755 },
            Op::Chown { path: p("/d/f"), uid: 0, gid: 80 },
            Op::Chflags { path: p("/d/f"), set: HIDDEN | LOCKED, clear: 0 },
            Op::Chflags { path: p("/d/g"), set: 0, clear: HIDDEN },
        ]
    }

    #[test]
    fn lists_round_trip() {
        let ops = all_ops();
        let args = encode(STDOUT, &ops).unwrap();
        assert_eq!(&args[..3], [ARG, VERSION, STDOUT]);
        assert_eq!(args[3..6], ["copy", "/a/b c", "/d/b c"]);
        assert!(args.contains(&"1755".to_owned()) && args.contains(&"hidden,uchg".to_owned()));
        assert_eq!(channel_of(&args), Some(STDOUT));
        assert_eq!(decode(&args), Ok((STDOUT.to_owned(), ops)));
    }

    #[test]
    fn decode_refuses_what_gezik_did_not_write() {
        let s = |words: &[&str]| words.iter().map(|w| (*w).to_owned()).collect::<Vec<_>>();
        assert_eq!(decode(&s(&[ARG, "2", "-", "mkdir", "/x"])), Err(WRONG_VERSION));
        assert_eq!(decode(&s(&[ARG, VERSION, "-"])), Err(NOTHING));
        assert_eq!(channel_of(&s(&["--pdf-worker", VERSION, "-"])), None);
        for bad in [
            &[ARG, VERSION, "-", "mkdir"][..],
            &[ARG, VERSION, "-", "copy", "/a"],
            &[ARG, VERSION, "-", "rm", "/a"],
            &[ARG, VERSION, "-", "chmod", "/a", "0o755"],
            &[ARG, VERSION, "-", "chmod", "/a", "8"],
            &[ARG, VERSION, "-", "chmod", "/a", ""],
            &[ARG, VERSION, "-", "chmod", "/a", "1234567"],
            &[ARG, VERSION, "-", "chown", "/a", "-1", "0"],
            &[ARG, VERSION, "-", "chown", "/a", "1e3", "0"],
            &[ARG, VERSION, "-", "chown", "/a", "99999999999", "0"],
            &[ARG, VERSION, "-", "chflags", "/a", "schg", "-"],
            &[ARG, VERSION, "-", "chflags", "/a", "hidden,", "-"],
            &[ARG, VERSION],
            &["--other", VERSION, "-", "mkdir", "/a"],
        ] {
            assert!(decode(&s(bad)).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn paths_that_are_refused() {
        let none: [PathBuf; 0] = [];
        let unix_bad = [
            ("x/y", NOT_FULL),
            ("/a/../etc", NOT_PLAIN),
            ("/a/./b", NOT_PLAIN),
            ("/a//b", NOT_PLAIN),
            ("/a/b/", NOT_PLAIN),
            ("/a/b\nc", CONTROL),
            ("/a/\u{7f}", CONTROL),
        ];
        for (path, why) in unix_bad {
            assert_eq!(check(&[Op::Mkdir(p(path))], &sys(&none)), Err((0, why)), "{path:?}");
        }
        let win = System { windows: true, macos: false, protected: &none };
        let windows_bad = [
            (r"\\?\C:\x", DEVICE),
            (r"\\.\PhysicalDrive0", DEVICE),
            (r"\\server\share\x", DEVICE),
            (r"C:x", NOT_FULL),
            (r"x\y", NOT_FULL),
            ("C:/a", NOT_FULL),
            (r"C:\a\b:stream", DEVICE),
            (r"C:\a\..\Windows", NOT_PLAIN),
            (r"C:\a\\b", NOT_PLAIN),
            (r"C:\a\NUL", RESERVED),
            (r"C:\a\con.txt", RESERVED),
            (r"C:\a\COM1", RESERVED),
            (r"C:\a\lpt9.log", RESERVED),
            (r"C:\a\COM¹", RESERVED),
            (r"C:\a\b.", BAD_CHAR),
            (r"C:\a\b ", BAD_CHAR),
            (r"C:\a\b?", BAD_CHAR),
            (r"C:\a\b/c", BAD_CHAR),
            ("C:\\a\\b\u{1}", CONTROL),
        ];
        for (path, why) in windows_bad {
            assert_eq!(check(&[Op::Mkdir(p(path))], &win), Err((0, why)), "{path:?}");
        }
        assert_eq!(check(&[Op::Mkdir(p(r"C:\a\console"))], &win), Ok(()), "only the device names themselves");
        assert_eq!(check(&[Op::Mkdir(p(r"C:\a\COM10"))], &win), Ok(()));
        assert_eq!(check(&[Op::Mkdir(p(&format!("/a/{}", "x".repeat(256))))], &sys(&none)), Err((0, TOO_LONG)));
    }

    #[test]
    fn system_folders_and_homes_are_refused_themselves() {
        let list = unix_protected(false);
        let s = sys(&list);
        for path in ["/", "/etc", "/usr", "/home/teo", "/tmp", "/lib64"] {
            assert_eq!(check(&[Op::Delete(p(path))], &s), Err((0, GUARDED)), "{path}");
        }
        for path in ["/etc/hosts", "/usr/local/x", "/home/teo/notes", "/ETC"] {
            assert_eq!(check(&[Op::Delete(p(path))], &s), Ok(()), "{path}: what is inside is free");
        }
        assert_eq!(check(&[Op::Copy { from: p("/etc"), to: p("/home/teo/etc"), replace: false }], &s), Ok(()));
        assert_eq!(check(&[Op::Copy { from: p("/home/teo/x"), to: p("/usr"), replace: true }], &s), Err((0, GUARDED)));
        assert_eq!(check(&[Op::Chmod { path: p("/etc"), mode: 0o755 }], &s), Err((0, GUARDED)));
        let mac = unix_protected(true);
        let m = System { windows: false, macos: true, protected: &mac };
        for path in ["/System", "/system", "/Users/teo", "/users/TEO", "/private/etc", "/Applications"] {
            assert_eq!(check(&[Op::Delete(p(path))], &m), Err((0, GUARDED)), "{path}");
        }
        assert_eq!(check(&[Op::Delete(p("/Applications/Old.app"))], &m), Ok(()));
        let win_list = windows_protected(r"C:\Windows");
        let w = System { windows: true, macos: false, protected: &win_list };
        for path in [
            r"C:\",
            r"D:\",
            r"C:\WINDOWS",
            r"c:\program files",
            r"C:\Program Files (x86)",
            r"C:\ProgramData",
            r"C:\Users",
            r"C:\Users\teo",
            r"E:\Users\x",
        ] {
            assert_eq!(check(&[Op::Delete(p(path))], &w), Err((0, GUARDED)), "{path}");
        }
        assert_eq!(check(&[Op::Delete(p(r"C:\Program Files\Old"))], &w), Ok(()));
        assert_eq!(check(&[Op::Delete(p(r"C:\Windows\Temp\x.log"))], &w), Ok(()));
    }

    #[test]
    fn one_bad_item_names_its_place() {
        let list = unix_protected(false);
        let ops = [Op::Mkdir(p("/opt/a")), Op::Mkdir(p("/opt/b")), Op::Delete(p("relative"))];
        assert_eq!(check(&ops, &sys(&list)), Err((2, NOT_FULL)));
        assert_eq!(check(&[], &sys(&list)), Err((0, NOTHING)));
    }

    #[test]
    fn what_each_operation_may_be() {
        let list = unix_protected(false);
        let s = sys(&list);
        let one = |op: Op| check(&[op], &s).map_err(|(_, why)| why);
        assert_eq!(one(Op::Rename { path: p("/d/a"), name: "x/y".into() }), Err(BAD_NAME));
        assert_eq!(one(Op::Rename { path: p("/d/a"), name: "..".into() }), Err(BAD_NAME));
        assert_eq!(one(Op::Rename { path: p("/d/a"), name: String::new() }), Err(BAD_NAME));
        assert_eq!(one(Op::Rename { path: p("/d/a"), name: "a\u{0}b".into() }), Err(CONTROL));
        assert_eq!(one(Op::Rename { path: p("/d/a"), name: "b c.txt".into() }), Ok(()));
        assert_eq!(one(Op::Chmod { path: p("/d/f"), mode: 0o4755 }), Err(MODE), "setuid");
        assert_eq!(one(Op::Chmod { path: p("/d/f"), mode: 0o2755 }), Err(MODE), "setgid");
        assert_eq!(
            one(Op::Chmod { path: p("/d/f"), mode: 0o1777 }),
            Ok(()),
            "sticky: the helper checks it is a folder"
        );
        assert_eq!(one(Op::Chflags { path: p("/d/f"), set: HIDDEN, clear: 0 }), Err(MAC_ONLY));
        assert_eq!(one(Op::Copy { from: p("/d/a"), to: p("/d/a/b"), replace: false }), Err(INTO_ITSELF));
        assert_eq!(one(Op::Move { from: p("/d/a"), to: p("/d/a"), replace: false }), Err(INTO_ITSELF));
        let mac_list = unix_protected(true);
        let m = System { windows: false, macos: true, protected: &mac_list };
        let flags = |set, clear| check(&[Op::Chflags { path: p("/d/f"), set, clear }], &m).map_err(|(_, why)| why);
        assert_eq!(flags(HIDDEN, 0), Ok(()));
        assert_eq!(flags(0x2_0000, 0), Err(FLAGS), "schg and anything else");
        assert_eq!(flags(HIDDEN, HIDDEN), Err(FLAGS));
        let none: [PathBuf; 0] = [];
        let w = System { windows: true, macos: false, protected: &none };
        assert_eq!(check(&[Op::Chmod { path: p(r"C:\a"), mode: 0o755 }], &w), Err((0, UNIX_ONLY)));
        assert_eq!(check(&[Op::Chown { path: p(r"C:\a"), uid: 0, gid: 0 }], &w), Err((0, UNIX_ONLY)));
        assert_eq!(check(&[Op::Rename { path: p(r"C:\a\b"), name: r"c\d".into() }], &w), Err((0, BAD_NAME)));
        assert_eq!(check(&[Op::Rename { path: p(r"C:\a\b"), name: "nul".into() }], &w), Err((0, RESERVED)));
    }

    #[test]
    fn lists_that_do_not_fit_are_refused() {
        let many: Vec<Op> =
            (0..2000).map(|i| Op::Delete(p(&format!(r"C:\Program Files\App\file number {i}.dat")))).collect();
        assert!(!fits(r"C:\Tools\gezik.exe", &many, true), "over 32,767 UTF-16 units");
        assert!(fits(r"C:\Tools\gezik.exe", &many[..100], true));
        let unix_many: Vec<Op> = (0..4000).map(|i| Op::Delete(p(&format!("/opt/app/file number {i}.dat")))).collect();
        assert!(!fits("/usr/local/bin/gezik", &unix_many, false), "over 128 KB");
        assert!(fits("/usr/local/bin/gezik", &unix_many[..100], false));
        let long = Op::Delete(p(&format!("/opt/{}", "x/".repeat(17_000))));
        assert!(!fits("/g", &[long], false), "one argument over 32 KB");
        assert!(valid_channel(&format!("{PIPE_PREFIX}{}", "0a".repeat(16)), true));
        let upper = format!("{PIPE_PREFIX}{}", "A".repeat(32));
        for bad in [r"\\.\pipe\gezik-elev-XYZ", r"\\.\pipe\other", "-", upper.as_str()] {
            assert!(!valid_channel(bad, true), "{bad}");
        }
        assert!(valid_channel("-", false) && !valid_channel(r"\\.\pipe\x", false));
    }

    #[test]
    fn replies_parse_and_garbage_is_dropped() {
        assert_eq!(parse_reply(&ok_line(3)), Some(Reply::Ok(3)));
        assert_eq!(
            parse_reply(&err_line(1, 5, "Access denied\nfake line")),
            Some(Reply::Err { index: 1, code: 5, message: "Access denied fake line".into() })
        );
        assert_eq!(
            parse_reply(&refused_line("item 2: Not a full path")),
            Some(Reply::Refused("item 2: Not a full path".into()))
        );
        assert_eq!(parse_reply(DONE), Some(Reply::Done));
        assert_eq!(parse_reply("done\r"), Some(Reply::Done));
        assert_eq!(err_line(0, 1, &"é".repeat(500)).chars().count(), "err 0 1 ".len() + MAX_MESSAGE);
        for garbage in
            ["", "ok", "ok -1", "ok 1x", "ok 99999999", "err 1", "err x 5 m", "err 1 y m", "OK 1", "rm -rf /"]
        {
            assert_eq!(parse_reply(garbage), None, "{garbage:?}");
        }
    }

    #[test]
    fn answers_take_the_first_reply_and_name_the_silent() {
        let replies = [
            Reply::Ok(0),
            Reply::Err { index: 0, code: 5, message: "late".into() },
            Reply::Err { index: 1, code: 5, message: "Access denied".into() },
            Reply::Ok(9),
        ];
        assert_eq!(
            answers(3, &replies),
            [Answer::Done, Answer::Failed("Access denied".into()), Answer::NotRun(STOPPED.into())]
        );
        let mut finished = replies.to_vec();
        finished.push(Reply::Done);
        assert_eq!(answers(3, &finished)[2], Answer::NotRun(SKIPPED.into()));
        let refused = [Reply::Ok(0), Reply::Refused("item 1: Not a full path".into())];
        assert_eq!(answers(2, &refused), vec![Answer::NotRun("Not done: item 1: Not a full path".into()); 2]);
    }

    #[test]
    fn each_operation_knows_its_undo() {
        let attrs = Attrs { mode: 0o644, uid: 501, gid: 20, flags: HIDDEN };
        let undo = |op: Op, before| undo_of(&op, before);
        let copy = Op::Copy { from: p("/a/x"), to: p("/d/x"), replace: false };
        assert_eq!(undo(copy, Before::Absent), Some(Op::Delete(p("/d/x"))));
        let over = Op::Copy { from: p("/a/x"), to: p("/d/x"), replace: true };
        assert_eq!(undo(over, Before::Present), None, "the old file is gone");
        let moved = Op::Move { from: p("/a/x"), to: p("/d/x"), replace: false };
        assert_eq!(
            undo(moved.clone(), Before::Absent),
            Some(Op::Move { from: p("/d/x"), to: p("/a/x"), replace: false })
        );
        assert_eq!(undo(moved, Before::Unknown), None);
        assert_eq!(
            undo(Op::Rename { path: p("/d/a.txt"), name: "b.txt".into() }, Before::Present),
            Some(Op::Rename { path: p("/d/b.txt"), name: "a.txt".into() })
        );
        assert_eq!(undo(Op::Mkdir(p("/d/n")), Before::Absent), Some(Op::Rmdir(p("/d/n"))));
        assert_eq!(undo(Op::Rmdir(p("/d/n")), Before::Present), Some(Op::Mkdir(p("/d/n"))));
        assert_eq!(undo(Op::Delete(p("/d/x")), Before::Present), None);
        assert_eq!(
            undo(Op::Chmod { path: p("/f"), mode: 0o600 }, Before::Attrs(attrs)),
            Some(Op::Chmod { path: p("/f"), mode: 0o644 })
        );
        let setuid = Attrs { mode: 0o4755, ..attrs };
        assert_eq!(undo(Op::Chmod { path: p("/f"), mode: 0o755 }, Before::Attrs(setuid)), None, "never sets setuid");
        assert_eq!(
            undo(Op::Chown { path: p("/f"), uid: 0, gid: 0 }, Before::Attrs(attrs)),
            Some(Op::Chown { path: p("/f"), uid: 501, gid: 20 })
        );
        assert_eq!(
            undo(Op::Chflags { path: p("/f"), set: LOCKED, clear: HIDDEN }, Before::Attrs(attrs)),
            Some(Op::Chflags { path: p("/f"), set: HIDDEN, clear: LOCKED })
        );
        assert_eq!(undo(Op::Chmod { path: p("/f"), mode: 0o600 }, Before::Unknown), None);
    }

    #[test]
    fn attribute_changes_become_operations_in_a_safe_order() {
        let w = |from: Attrs, to: Attrs| Wanted { path: p("/f"), id: Identity::default(), from, to };
        let a = Attrs { mode: 0o644, uid: 501, gid: 20, flags: LOCKED };
        let unlock = w(a, Attrs { mode: 0o600, uid: 0, flags: 0, ..a });
        assert_eq!(
            attr_ops(&[unlock]),
            [
                Op::Chflags { path: p("/f"), set: 0, clear: LOCKED },
                Op::Chown { path: p("/f"), uid: 0, gid: 20 },
                Op::Chmod { path: p("/f"), mode: 0o600 },
            ],
            "Locked comes off first: it blocks every other change"
        );
        let lock = w(Attrs { flags: 0, ..a }, Attrs { flags: LOCKED, mode: 0o600, ..a });
        assert_eq!(
            attr_ops(&[lock]),
            [Op::Chmod { path: p("/f"), mode: 0o600 }, Op::Chflags { path: p("/f"), set: LOCKED, clear: 0 }],
            "and goes on last"
        );
        assert!(attr_ops(&[w(a, a)]).is_empty());
    }

    #[test]
    fn labels_say_what_and_how_many() {
        let copy = |n: usize| {
            (0..n)
                .map(|i| Op::Copy { from: p(&format!("/a/{i}")), to: p(&format!("/d/{i}")), replace: false })
                .collect::<Vec<_>>()
        };
        assert_eq!(label(&copy(1)), "Copy 1 item as administrator");
        assert_eq!(label(&copy(3)), "Copy 3 items as administrator");
        assert_eq!(label(&[Op::Delete(p("/a")), Op::Delete(p("/b"))]), "Delete 2 items permanently as administrator");
        assert_eq!(label(&[Op::Rename { path: p("/a"), name: "b".into() }]), "Rename as administrator");
        assert_eq!(label(&[Op::Mkdir(p("/a"))]), "New folder as administrator");
        assert_eq!(
            label(&[Op::Chown { path: p("/a"), uid: 0, gid: 0 }, Op::Chmod { path: p("/a"), mode: 0o600 }]),
            "Change 1 item as administrator"
        );
        assert_eq!(label(&[Op::Mkdir(p("/a")), Op::Delete(p("/b"))]), "2 changes as administrator");
        assert_eq!(prompt(&copy(2)), "Gezik: Copy 2 items as administrator.");
    }

    #[test]
    fn the_protected_lists() {
        assert_eq!(
            windows_protected(r"D:\WINNT"),
            [r"D:\WINNT", r"D:\Program Files", r"D:\Program Files (x86)", r"D:\ProgramData", r"D:\Users"]
                .map(PathBuf::from)
        );
        assert!(unix_protected(false).contains(&p("/etc")) && !unix_protected(false).contains(&p("/System")));
        assert!(unix_protected(true).contains(&p("/private/etc")) && unix_protected(true).contains(&p("/System")));
    }
}
