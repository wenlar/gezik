//! Other processes: whether one still runs (a note it left may still be in use), and running
//! a helper program (7-Zip) without a window, so that it and everything it starts can be
//! stopped at once.

use std::collections::VecDeque;
use std::ffi::OsStr;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

/// Whether a process with id `pid` runs. A reused id counts as running: what waits on it
/// only waits longer.
#[cfg(windows)]
pub fn process_alive(pid: u32) -> bool {
    use windows::Win32::Foundation::{CloseHandle, ERROR_ACCESS_DENIED, STILL_ACTIVE};
    use windows::Win32::System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    unsafe {
        match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(handle) => {
                let mut code = 0u32;
                let running = GetExitCodeProcess(handle, &mut code).is_ok() && code == STILL_ACTIVE.0 as u32;
                let _ = CloseHandle(handle);
                running
            }
            // It exists, but belongs to someone we may not look at.
            Err(err) => err.code() == ERROR_ACCESS_DENIED.to_hresult(),
        }
    }
}

#[cfg(unix)]
pub fn process_alive(pid: u32) -> bool {
    let Ok(pid) = libc::pid_t::try_from(pid) else { return false };
    // Signal 0 only checks; EPERM means it exists under another user.
    unsafe { libc::kill(pid, 0) == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM) }
}

/// What a finished helper said (gio, udisksctl).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ran {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Runs `args` (no shell) with `input` on its stdin, its messages in English (`LC_ALL=C`, so
/// what Gezik looks for in them stays the same), and waits. Blocking: background threads only.
/// `input` never goes on the command line: it may hold a password (spec 9 §10.5).
pub fn run_with_input(args: &[&str], input: &str) -> io::Result<Ran> {
    run_until(args, input, HELPER_LIMIT)
}

/// How long a helper may take (a server that never answers): it is then stopped.
const HELPER_LIMIT: Duration = Duration::from_secs(60);

/// [`run_with_input`] with its own time limit; past it the helper is stopped (`TimedOut`).
pub(crate) fn run_until(args: &[&str], input: &str, limit: Duration) -> io::Result<Ran> {
    let (program, rest) = args.split_first().ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
    let mut child = Command::new(program)
        .args(rest)
        .env("LC_ALL", "C")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    // Read on other threads, so a chatty helper never stops on a full pipe.
    fn read_all(pipe: Option<impl Read + Send + 'static>) -> Option<JoinHandle<Vec<u8>>> {
        pipe.map(|mut pipe| {
            std::thread::spawn(move || {
                let mut bytes = Vec::new();
                let _ = pipe.read_to_end(&mut bytes);
                bytes
            })
        })
    }
    let stdout = read_all(child.stdout.take());
    let stderr = read_all(child.stderr.take());
    if let Some(mut stdin) = child.stdin.take() {
        // A helper that asks nothing closes its end: that is not an error. Dropped: closed.
        let _ = stdin.write_all(input.as_bytes());
    }
    let started = std::time::Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() > limit {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::ErrorKind::TimedOut.into());
        }
        std::thread::sleep(POLL);
    };
    let text = |thread: Option<JoinHandle<Vec<u8>>>| {
        String::from_utf8_lossy(&thread.and_then(|t| t.join().ok()).unwrap_or_default()).into_owned()
    };
    Ok(Ran { ok: status.success(), stdout: text(stdout), stderr: text(stderr) })
}

/// The longest command line Gezik starts a program with, kept short of the system's limit:
/// on Windows 32,000 characters (of 32,767), but 8,000 (of 8,191) for a `.bat` or `.cmd`
/// `program`, which `cmd.exe` runs and which also rewrites `%`; elsewhere half of `ARG_MAX`
/// (the other half is the environment's) but at most 3 MiB (the kernel's own cap is
/// `min(stack / 4, 6 MiB)` however large `ARG_MAX` reads), or 64 KiB if the system does not say.
pub fn command_line_limit(program: &str) -> usize {
    #[cfg(windows)]
    {
        let extension = Path::new(program).extension().map(|e| e.to_string_lossy().to_ascii_lowercase());
        if matches!(extension.as_deref(), Some("bat" | "cmd")) { 8_000 } else { 32_000 }
    }
    #[cfg(unix)]
    {
        let _ = program;
        // SAFETY: sysconf only reads a value of the system.
        let max = unsafe { libc::sysconf(libc::_SC_ARG_MAX) };
        usize::try_from(max).ok().filter(|max| *max > 0).map_or(64 * 1024, |max| (max / 2).min(3 * 1024 * 1024))
    }
}

/// Makes a crash of this process a plain non-zero exit: on Windows no error reporting dialog,
/// no "insert a disk" or "file not found" box (`SetErrorMode`, inherited by what it starts)
/// and no "Abnormal program termination" box when C code in it calls `abort()`. For helper
/// processes whose parent reports how they ended (the PDF worker) and test binaries, which
/// must never stop on a dialog. Elsewhere nothing to do.
pub fn quiet_crashes() {
    #[cfg(windows)]
    {
        use windows::Win32::System::Diagnostics::Debug::{
            SEM_FAILCRITICALERRORS, SEM_NOGPFAULTERRORBOX, SEM_NOOPENFILEERRORBOX, SetErrorMode,
        };
        // The C runtime's (ucrt) switch for what `abort()` does besides ending the process.
        unsafe extern "C" {
            fn _set_abort_behavior(flags: u32, mask: u32) -> u32;
        }
        const WRITE_ABORT_MSG: u32 = 0x1;
        const CALL_REPORTFAULT: u32 = 0x2;
        unsafe {
            SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX | SEM_NOOPENFILEERRORBOX);
            _set_abort_behavior(0, WRITE_ABORT_MSG | CALL_REPORTFAULT);
        }
    }
}

/// How often `wait_or_stop` looks.
const POLL: Duration = Duration::from_millis(50);

/// A program Gezik runs (7-Zip), with no window, no input (or bytes given once, then its
/// input is closed), its output read on other threads,
/// and what it starts in turn kept with it: Windows puts them in one job object (closed with
/// the last handle, which ends them all), Unix in one process group.
pub struct ChildProcess {
    child: Child,
    #[cfg(windows)]
    job: Job,
    lines: Option<Lines>,
    stderr: Option<JoinHandle<String>>,
    finished: bool,
}

/// The longest piece of output passed on; the rest of a longer one is dropped.
const MAX_PIECE: usize = 16 * 1024;

/// The most output (in bytes) kept waiting to be read: past it the oldest pieces are
/// dropped. A program that writes much more than is read (a chatty user command, binary
/// output) never grows Gezik's memory, and is never held up on a full pipe either.
const MAX_WAITING: usize = 1024 * 1024;

/// How much of the end of a program's error output is kept (the failure message shows its
/// last lines).
const ERRORS_KEPT: usize = 64 * 1024;

/// The output pieces not read yet.
#[derive(Default)]
struct Waiting {
    pieces: VecDeque<String>,
    bytes: usize,
    /// The program closed its output.
    closed: bool,
}

#[derive(Default)]
struct Pieces {
    waiting: Mutex<Waiting>,
    came: Condvar,
}

impl Pieces {
    fn lock(&self) -> MutexGuard<'_, Waiting> {
        self.waiting.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Adds `piece`, dropping the oldest ones past [`MAX_WAITING`].
    fn push(&self, piece: String) {
        let mut waiting = self.lock();
        waiting.bytes += piece.len();
        waiting.pieces.push_back(piece);
        while waiting.bytes > MAX_WAITING && waiting.pieces.len() > 1 {
            let dropped = waiting.pieces.pop_front().map_or(0, |p| p.len());
            waiting.bytes -= dropped;
        }
        self.came.notify_all();
    }

    fn close(&self) {
        self.lock().closed = true;
        self.came.notify_all();
    }
}

/// The pieces of a program's output: lines, and the steps of a progress line it rewrites
/// in place (separated by `\r` or backspaces). At most 1 MB waits to be read (the oldest
/// pieces go first) and a piece is at most 16 KB long.
pub struct Lines(Arc<Pieces>);

impl Lines {
    /// What came so far, without waiting.
    pub fn ready(&self) -> impl Iterator<Item = String> + '_ {
        let mut waiting = self.0.lock();
        waiting.bytes = 0;
        std::mem::take(&mut waiting.pieces).into_iter()
    }
}

impl Iterator for Lines {
    type Item = String;

    /// Waits for the next piece; `None` once the program closed its output.
    fn next(&mut self) -> Option<String> {
        let mut waiting = self.0.lock();
        loop {
            if let Some(piece) = waiting.pieces.pop_front() {
                waiting.bytes -= piece.len();
                return Some(piece);
            }
            if waiting.closed {
                return None;
            }
            waiting = self.0.came.wait(waiting).unwrap_or_else(PoisonError::into_inner);
        }
    }
}

/// Reads `stdout` into `pieces` (split at `\n`, `\r` and backspaces) until it closes.
fn read_pieces(mut stdout: impl Read, pieces: &Pieces) {
    let mut buf = [0u8; 4096];
    let mut piece = Vec::new();
    let send = |piece: &mut Vec<u8>| {
        if !piece.is_empty() {
            pieces.push(String::from_utf8_lossy(piece).into_owned());
            piece.clear();
        }
    };
    loop {
        match stdout.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                for &byte in &buf[..n] {
                    if matches!(byte, b'\n' | b'\r' | 8) {
                        send(&mut piece);
                    } else if piece.len() < MAX_PIECE {
                        piece.push(byte);
                    }
                }
            }
            Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
    send(&mut piece);
    pieces.close();
}

/// Reads `stderr` until it closes, keeping its last [`ERRORS_KEPT`] bytes (from the start of
/// a line when it was cut).
fn read_errors(mut stderr: impl Read) -> String {
    let mut kept = Vec::new();
    let mut cut = false;
    let mut buf = [0u8; 4096];
    loop {
        match stderr.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                kept.extend_from_slice(&buf[..n]);
                if kept.len() > 2 * ERRORS_KEPT {
                    kept.drain(..kept.len() - ERRORS_KEPT);
                    cut = true;
                }
            }
            Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
    if kept.len() > ERRORS_KEPT {
        kept.drain(..kept.len() - ERRORS_KEPT);
        cut = true;
    }
    if cut && let Some(newline) = kept.iter().position(|&b| b == b'\n') {
        kept.drain(..=newline);
    }
    String::from_utf8_lossy(&kept).into_owned()
}

impl ChildProcess {
    /// Starts `program` with `args` (passed as they are, no shell) in `cwd`.
    pub fn spawn<I, S>(program: &Path, args: I, cwd: Option<&Path>) -> io::Result<ChildProcess>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        ChildProcess::spawn_with_input(program, args, cwd, None)
    }

    /// Like `spawn`, with `input` written to the program's input on another thread (a
    /// password: never on its command line, where other users' tools can see it), which is
    /// then closed.
    pub fn spawn_with_input<I, S>(
        program: &Path,
        args: I,
        cwd: Option<&Path>,
        input: Option<Vec<u8>>,
    ) -> io::Result<ChildProcess>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = Command::new(program);
        let stdin = if input.is_some() { Stdio::piped() } else { Stdio::null() };
        command.args(args).stdin(stdin).stdout(Stdio::piped()).stderr(Stdio::piped());
        if let Some(cwd) = cwd {
            command.current_dir(cwd);
        }
        #[cfg(windows)]
        let job = Job::new()?;
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let mut child = command.spawn()?;
        #[cfg(windows)]
        if let Err(err) = job.assign(&child) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(err);
        }
        if let (Some(mut stdin), Some(input)) = (child.stdin.take(), input) {
            // A program that never reads it ends the pipe: that error is no failure.
            std::thread::spawn(move || {
                let _ = stdin.write_all(&input);
            });
        }
        let pieces = Arc::new(Pieces::default());
        match child.stdout.take() {
            Some(stdout) => {
                let pieces = pieces.clone();
                std::thread::spawn(move || read_pieces(stdout, &pieces));
            }
            None => pieces.close(),
        }
        let stderr = child.stderr.take().map(|stderr| std::thread::spawn(move || read_errors(stderr)));
        Ok(ChildProcess {
            child,
            #[cfg(windows)]
            job,
            lines: Some(Lines(pieces)),
            stderr,
            finished: false,
        })
    }

    pub fn id(&self) -> u32 {
        self.child.id()
    }

    /// Its output as it comes (taken once; later calls give nothing).
    pub fn stdout_lines(&mut self) -> Lines {
        self.lines.take().unwrap_or_else(|| {
            let closed = Pieces::default();
            closed.close();
            Lines(Arc::new(closed))
        })
    }

    /// The end of what it wrote to its error output (its last 64 KB); waits until it closed
    /// it (call after it ended).
    pub fn stderr_text(&mut self) -> String {
        self.stderr.take().and_then(|thread| thread.join().ok()).unwrap_or_default()
    }

    /// Waits until it ends, looking every 50 ms whether to `stop`; if so it is ended with
    /// everything it started, and the answer is `None`.
    pub fn wait_or_stop(&mut self, stop: impl Fn() -> bool) -> io::Result<Option<ExitStatus>> {
        loop {
            if let Some(status) = self.child.try_wait()? {
                self.finished = true;
                return Ok(Some(status));
            }
            if stop() {
                self.kill_tree();
                return Ok(None);
            }
            std::thread::sleep(POLL);
        }
    }

    /// Ends it and everything it started.
    pub fn kill_tree(&mut self) {
        #[cfg(windows)]
        self.job.terminate();
        #[cfg(unix)]
        if let Ok(group) = libc::pid_t::try_from(self.child.id()) {
            unsafe { libc::killpg(group, libc::SIGKILL) };
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.finished = true;
    }

    /// The ids of the processes it is made of now (itself and what it started).
    #[cfg(all(windows, test))]
    pub fn process_ids(&self) -> Vec<u32> {
        self.job.process_ids()
    }
}

impl Drop for ChildProcess {
    /// A program still running when its owner goes is ended with what it started.
    fn drop(&mut self) {
        if !self.finished {
            self.kill_tree();
        }
    }
}

/// A Windows job object that ends its processes when its handle closes.
#[cfg(windows)]
struct Job(windows::Win32::Foundation::HANDLE);

// A kernel handle: any thread may use or close it.
#[cfg(windows)]
unsafe impl Send for Job {}
#[cfg(windows)]
unsafe impl Sync for Job {}

#[cfg(windows)]
impl Job {
    fn new() -> io::Result<Job> {
        use windows::Win32::System::JobObjects::{
            CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JobObjectExtendedLimitInformation, SetInformationJobObject,
        };
        let handle = unsafe { CreateJobObjectW(None, windows::core::PCWSTR::null()) }.map_err(win_error)?;
        let job = Job(handle);
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        unsafe {
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        }
        .map_err(win_error)?;
        Ok(job)
    }

    fn assign(&self, child: &Child) -> io::Result<()> {
        use std::os::windows::io::AsRawHandle;
        use windows::Win32::System::JobObjects::AssignProcessToJobObject;
        let process = windows::Win32::Foundation::HANDLE(child.as_raw_handle());
        unsafe { AssignProcessToJobObject(self.0, process) }.map_err(win_error)
    }

    fn terminate(&self) {
        use windows::Win32::System::JobObjects::TerminateJobObject;
        let _ = unsafe { TerminateJobObject(self.0, 1) };
    }

    #[cfg(test)]
    fn process_ids(&self) -> Vec<u32> {
        use windows::Win32::System::JobObjects::{
            JOBOBJECT_BASIC_PROCESS_ID_LIST, JobObjectBasicProcessIdList, QueryInformationJobObject,
        };
        // The list's header and room for 64 ids.
        #[repr(C)]
        struct List {
            head: JOBOBJECT_BASIC_PROCESS_ID_LIST,
            more: [usize; 63],
        }
        let mut list = List { head: JOBOBJECT_BASIC_PROCESS_ID_LIST::default(), more: [0; 63] };
        let queried = unsafe {
            QueryInformationJobObject(
                Some(self.0),
                JobObjectBasicProcessIdList,
                (&mut list as *mut List).cast(),
                size_of::<List>() as u32,
                None,
            )
        };
        if queried.is_err() {
            return Vec::new();
        }
        let count = (list.head.NumberOfProcessIdsInList as usize).min(64);
        let ids = std::iter::once(list.head.ProcessIdList[0]).chain(list.more.iter().copied());
        ids.take(count).map(|id| id as u32).collect()
    }
}

#[cfg(windows)]
impl Drop for Job {
    fn drop(&mut self) {
        let _ = unsafe { windows::Win32::Foundation::CloseHandle(self.0) };
    }
}

#[cfg(windows)]
fn win_error(err: windows::core::Error) -> io::Error {
    crate::fs::io_error(err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn this_process_runs_and_a_finished_one_does_not() {
        assert!(process_alive(std::process::id()));
        let mut child = if cfg!(windows) {
            std::process::Command::new("cmd").args(["/c", "exit"]).spawn().unwrap()
        } else {
            std::process::Command::new("true").spawn().unwrap()
        };
        let pid = child.id();
        child.wait().unwrap();
        // Waited for and closed: the id names no running process (barring quick reuse).
        assert!(!process_alive(pid));
    }

    /// Waits up to 10 s for `done`.
    fn eventually(done: impl Fn() -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done() {
            if Instant::now() > deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        true
    }

    #[cfg(windows)]
    #[test]
    fn process_tree_is_killed() {
        let args = ["/c", "ping", "-n", "30", "127.0.0.1", ">nul"];
        let mut child = ChildProcess::spawn(Path::new("cmd"), args, None).unwrap();
        let ping = |ids: &[u32]| ids.iter().any(|&id| id != child.id());
        assert!(eventually(|| ping(&child.process_ids())), "cmd started ping");
        let ids = child.process_ids();
        assert!(ids.contains(&child.id()) && ids.len() >= 2, "{ids:?}");
        let started = Instant::now();
        let stopped = child.wait_or_stop(|| true).unwrap();
        assert_eq!(stopped, None);
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(eventually(|| ids.iter().all(|&id| !process_alive(id))), "{ids:?} still run");
    }

    #[cfg(unix)]
    #[test]
    fn process_tree_is_killed() {
        let mut child = ChildProcess::spawn(Path::new("sh"), ["-c", "sleep 30 & sleep 30"], None).unwrap();
        let group = libc::pid_t::try_from(child.id()).unwrap();
        // Signal 0 to the group: true while any of it is there.
        let group_alive = || unsafe { libc::kill(-group, 0) == 0 };
        // The shell started its two sleeps (both in its group).
        std::thread::sleep(Duration::from_millis(300));
        assert!(group_alive());
        assert_eq!(child.wait_or_stop(|| true).unwrap(), None);
        assert!(eventually(|| !group_alive()), "the shell's children still run");
    }

    #[test]
    fn only_the_end_of_the_error_output_is_kept() {
        let text: String = (0..20_000).map(|n| format!("error line {n}\n")).collect();
        let kept = read_errors(text.as_bytes());
        assert!(kept.len() <= ERRORS_KEPT, "{}", kept.len());
        assert!(kept.starts_with("error line "), "cut at a line's start");
        assert!(kept.ends_with("error line 19999\n"));
        assert_eq!(read_errors(&b"short\n"[..]), "short\n");
    }

    #[test]
    fn output_nobody_reads_is_bounded() {
        let mut text: Vec<u8> = (0..40_000).flat_map(|n| format!("{n:0>99}\n").into_bytes()).collect();
        text.extend(std::iter::repeat_n(b'x', 50_000));
        text.extend_from_slice(b"\nlast\n");
        let pieces = Pieces::default();
        read_pieces(&text[..], &pieces);
        let waiting = pieces.lock();
        assert!(waiting.bytes <= MAX_WAITING && waiting.closed);
        assert_eq!(waiting.bytes, waiting.pieces.iter().map(String::len).sum::<usize>());
        let n = waiting.pieces.len();
        assert_eq!(waiting.pieces[n - 2].len(), MAX_PIECE, "a long piece is cut");
        assert_eq!(waiting.pieces[n - 1], "last");
        drop(waiting);
        let mut lines = Lines(Arc::new(pieces));
        assert_eq!(lines.ready().count(), n);
        assert_eq!(lines.next(), None);
    }

    #[test]
    fn input_is_given_once_and_closed() {
        let (program, args): (&str, &[&str]) = if cfg!(windows) {
            ("cmd", &["/c", "set /p LINE=& call echo got %LINE%"])
        } else {
            ("sh", &["-c", "read LINE; echo got $LINE"])
        };
        let mut child =
            ChildProcess::spawn_with_input(Path::new(program), args, None, Some(b"p w\n".to_vec())).unwrap();
        let lines = child.stdout_lines();
        let status = child.wait_or_stop(|| false).unwrap().unwrap();
        assert!(status.success());
        let lines: Vec<String> = lines.map(|line| line.trim().to_owned()).collect();
        assert_eq!(lines, ["got p w"]);
    }

    #[test]
    fn output_comes_in_pieces_and_the_exit_status_is_kept() {
        let (program, args): (&str, &[&str]) = if cfg!(windows) {
            ("cmd", &["/c", "echo one& echo two& exit 3"])
        } else {
            ("sh", &["-c", "echo one; echo two; exit 3"])
        };
        let mut child = ChildProcess::spawn(Path::new(program), args, None).unwrap();
        let lines = child.stdout_lines();
        let status = child.wait_or_stop(|| false).unwrap().unwrap();
        assert_eq!(status.code(), Some(3));
        let lines: Vec<String> = lines.map(|line| line.trim().to_owned()).collect();
        assert_eq!(lines, ["one", "two"]);
        assert_eq!(child.stderr_text(), "");
    }

    #[test]
    fn a_missing_helper_is_not_found() {
        let err = run_with_input(&["gezik-no-such-helper-9b6"], "").unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
        assert_eq!(run_with_input(&[], "").unwrap_err().kind(), std::io::ErrorKind::InvalidInput);
    }

    #[cfg(unix)]
    #[test]
    fn the_input_goes_to_stdin_only() {
        let ran = run_with_input(&["cat"], "teo\n\nsecret\n").unwrap();
        assert_eq!(ran, Ran { ok: true, stdout: "teo\n\nsecret\n".into(), stderr: String::new() });
        let failed = run_with_input(&["sh", "-c", "echo no >&2; exit 3"], "").unwrap();
        assert!(!failed.ok && failed.stderr == "no\n");
    }

    #[test]
    fn a_helper_that_hangs_is_stopped() {
        let args: &[&str] = if cfg!(windows) { &["ping", "-n", "30", "127.0.0.1"] } else { &["sleep", "30"] };
        let started = Instant::now();
        let err = run_until(args, "", Duration::from_millis(300)).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(5));
        let quick: &[&str] = if cfg!(windows) { &["cmd", "/c", "echo hi"] } else { &["echo", "hi"] };
        assert_eq!(run_until(quick, "", Duration::from_secs(10)).unwrap().stdout.trim(), "hi");
    }

    #[test]
    fn the_command_line_limit_leaves_room() {
        let limit = command_line_limit("zip");
        if cfg!(windows) {
            assert_eq!(limit, 32_000);
            assert_eq!(command_line_limit("run.BAT"), 8_000);
            assert_eq!(command_line_limit(r"C:\tools\run.cmd"), 8_000);
            assert_eq!(command_line_limit(r"C:\tools\run.exe"), 32_000);
        } else {
            assert!((16 * 1024..=3 * 1024 * 1024).contains(&limit), "{limit}");
        }
    }
}
