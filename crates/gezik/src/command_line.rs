//! Commands in the address bar (spec 16 §7): `>` and a command, run by Enter only, in the
//! folder shown with the system's shell (cmd on Windows, `$SHELL -c` elsewhere; PowerShell or a
//! program of one's own by `[command-line] shell`). No terminal: no TTY, the input empty (a
//! program that waits for it ends), escape sequences taken out. Its output, both streams in
//! the order they come, fills the pane's panel; `Stop` ends the shell with everything it
//! started (a job object on Windows, a process group on Unix).
//!
//! Nothing runs or waits while idle: the two reading threads and the panel's 33 ms timer live
//! only while a command runs.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use gezik_config::settings::{COMMANDS_MAX, CommandLineSettings};
use gezik_config::store::ConfigStore;
use gezik_i18n::t;
use gezik_platform::ChildProcess;
use slint::{Model, ModelNotify, ModelRc, ModelTracker};

use crate::CmdLine;
use crate::panes::{Pane, PaneId};

/// The most output kept (spec 16 §7.3), counting each line's text and [`LINE_COST`]; past it
/// the oldest lines go.
pub const MAX_OUTPUT: usize = 2 * 1024 * 1024;
/// What is kept once the output went past [`MAX_OUTPUT`]: lines go in batches, not one by one.
const KEPT_AFTER_DROP: usize = MAX_OUTPUT / 4 * 3;
/// What a line costs in memory besides its text (its `String`, its row).
const LINE_COST: usize = 64;
/// The most read and not yet shown: past it the readers drop the oldest (a flood never
/// grows memory, and the pipes never stall the program).
const MAX_PENDING: usize = 2 * 1024 * 1024;
/// The most lines the panel takes in one tick (the rest come in the next ones).
const LINES_PER_TICK: usize = 2_000;
/// The longest line passed on; a longer one is cut into lines of about this length.
const MAX_LINE: usize = 16 * 1024;
/// How often the panel takes the new lines while a command runs (at most 30 times a second).
const TICK: Duration = Duration::from_millis(33);
/// How long output may still come once the shell ended (a program it left running holds the
/// pipes open).
const GRACE: Duration = Duration::from_millis(500);
/// History rows the address bar lists under a command (its list shows 18 rows).
const HISTORY_ROWS: usize = 18;

/// The command in an address `typed`, if it is one: what follows a `>` (spaces before it are
/// passed over). A `>` further in is part of a path.
pub fn command_text(typed: &str) -> Option<&str> {
    typed.trim_start().strip_prefix('>')
}

/// The shell a command runs with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shell {
    /// `cmd.exe /d /s /c "<command>"`.
    Cmd,
    /// `powershell` or `pwsh` with the command in `-EncodedCommand`.
    Pwsh(String),
    /// `<shell> -c <command>`.
    Posix(OsString),
    /// A program of one's own: its arguments, then the command.
    Custom(String, Vec<String>),
}

/// The shell `[command-line]` names: "" is cmd on Windows and `$SHELL` (`env_shell`, else
/// `/bin/sh`) elsewhere.
pub fn pick_shell(settings: &CommandLineSettings, env_shell: Option<OsString>, windows: bool) -> Shell {
    match settings.shell.as_str() {
        "" | "cmd" if windows => Shell::Cmd,
        "" => Shell::Posix(env_shell.filter(|s| !s.is_empty()).unwrap_or_else(|| "/bin/sh".into())),
        "powershell" | "pwsh" => Shell::Pwsh(settings.shell.clone()),
        program => Shell::Custom(program.to_owned(), settings.shell_args.clone()),
    }
}

/// A program to start, its arguments and environment variables of its own; `raw` goes on
/// Windows' command line as it is, after them (cmd reads its own command line, not the C
/// runtime's quoting).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub program: OsString,
    pub args: Vec<OsString>,
    pub raw: Option<String>,
    pub env: Vec<(&'static str, OsString)>,
}

/// What PowerShell runs: its output in UTF-8 (nothing lost to the console's code page), its
/// progress bars off, then the command, which comes base64-encoded (UTF-8) in `GEZIK_COMMAND`
/// so that no quoting can break it. Not `-EncodedCommand`: with it Windows PowerShell writes
/// its errors as CLIXML once the error output is a pipe.
const POWERSHELL_RUN: &str = "[Console]::OutputEncoding=[Text.UTF8Encoding]::new();\
    $ProgressPreference='SilentlyContinue';\
    & ([ScriptBlock]::Create([Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($env:GEZIK_COMMAND))))";

/// How `shell` runs `command`, as typed. cmd runs in two: the first sets the console's code
/// page to UTF-8 (`chcp 65001`) and starts the second, which therefore writes UTF-8 from its
/// own commands (`echo`, `dir`) and its console programs (ping) alike: the OEM code page loses
/// letters (437 has no ğ, ş, ı), and cmd reads the code page only when it starts. The second
/// gets `/s /c "<command>"` through `GEZIK_COMMAND`, put in by the first's delayed expansion
/// (`!…!`, after it parsed its own line): it takes off the outer quotes and runs the rest
/// exactly as typed, `"`, `%`, `^` and all; PowerShell gets it in
/// base64, which no quoting can break; a POSIX shell gets it as one argument. `comspec` is
/// `%ComSpec%`, cmd's own path.
pub fn invocation(shell: &Shell, command: &str, comspec: Option<OsString>) -> Invocation {
    let plain = |program: OsString, args: Vec<OsString>| Invocation { program, args, raw: None, env: Vec::new() };
    match shell {
        Shell::Cmd => {
            let program = comspec.filter(|c| !c.is_empty()).unwrap_or_else(|| "cmd.exe".into());
            let inner = program.to_string_lossy().into_owned();
            Invocation {
                program,
                args: ["/v:on", "/d", "/s", "/c"].map(OsString::from).to_vec(),
                raw: Some(format!("\"chcp 65001>nul & \"{inner}\" /d /s /c !GEZIK_COMMAND!\"")),
                env: vec![("GEZIK_COMMAND", format!("\"{command}\"").into())],
            }
        }
        Shell::Pwsh(program) => Invocation {
            program: program.into(),
            args: ["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", POWERSHELL_RUN].map(OsString::from).to_vec(),
            raw: None,
            env: vec![("GEZIK_COMMAND", base64(command.as_bytes()).into())],
        },
        Shell::Posix(program) => plain(program.clone(), vec!["-c".into(), command.into()]),
        Shell::Custom(program, args) => {
            plain(program.into(), args.iter().map(OsString::from).chain([command.into()]).collect())
        }
    }
}

/// A `\\server\share` folder, where cmd cannot work (it would run in the Windows folder).
pub fn is_network(dir: &Path) -> bool {
    let text = dir.as_os_str().to_string_lossy();
    text.starts_with(r"\\?\UNC\") || (text.starts_with(r"\\") || text.starts_with("//")) && !text.starts_with(r"\\?\")
}

/// Standard base64 with padding (PowerShell's command).
pub(crate) fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, &b)| n | u32::from(b) << (16 - 8 * i));
        for i in 0..4 {
            out.push(if i <= chunk.len() { char::from(ABC[(n >> (18 - 6 * i) & 63) as usize]) } else { '=' });
        }
    }
    out
}

/// What a command is given of the pane: the focused item, the selected ones and the folder.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    pub file: Option<PathBuf>,
    pub files: Vec<PathBuf>,
    pub dir: PathBuf,
}

/// How long `GEZIK_FILES` may get for `shell`: cmd's command line holds 8,191 characters once
/// variables are put in; a Windows variable 32,767; Linux one environment string 128 KiB.
pub fn files_limit(shell: &Shell, windows: bool) -> usize {
    match shell {
        Shell::Cmd => 7_000,
        _ if windows => 30_000,
        _ => 100 * 1024,
    }
}

/// `path` in double quotes (cmd: no Windows name holds `"`, so `&`, `|`, `^`, `(`, `<` in a
/// name stay text where `%GEZIK_FILE%` is put in).
fn quoted(path: &Path) -> OsString {
    let mut text = OsString::from("\"");
    text.push(path);
    text.push("\"");
    text
}

/// The selected paths one per line (the list file's bytes).
fn list_bytes(files: &[PathBuf]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for path in files {
        #[cfg(unix)]
        bytes.extend_from_slice(std::os::unix::ffi::OsStrExt::as_bytes(path.as_os_str()));
        #[cfg(not(unix))]
        bytes.extend_from_slice(path.to_string_lossy().as_bytes());
        bytes.push(b'\n');
    }
    bytes
}

/// The environment a command gets (spec 16 §7.1, §7.2): `GEZIK_FILE`, `GEZIK_FILES`,
/// `GEZIK_DIR`, and what keeps programs from waiting on a terminal or drawing colors. For cmd
/// (`cmd`) every path comes in double quotes and `GEZIK_FILES` is `"a" "b"`: cmd puts
/// `%VAR%` in before it reads the line, so a bare `&` in a name would start a command. Other
/// shells get the paths as they are, `GEZIK_FILES` one per line. A list longer than `limit`
/// is left out: the bytes for a list file come back instead (`GEZIK_FILES_LIST`).
pub fn environment(sel: &Selection, cmd: bool, limit: usize) -> (Vec<(&'static str, OsString)>, Option<Vec<u8>>) {
    let shown = |path: &Path| if cmd { quoted(path) } else { path.as_os_str().to_owned() };
    let mut list = OsString::new();
    for (i, path) in sel.files.iter().enumerate() {
        if i > 0 {
            list.push(if cmd { " " } else { "\n" });
        }
        list.push(shown(path));
    }
    let mut env =
        vec![("GEZIK_FILE", sel.file.as_deref().map_or_else(OsString::new, shown)), ("GEZIK_DIR", shown(&sel.dir))];
    let long = list.len() > limit;
    if !long {
        env.push(("GEZIK_FILES", list));
    }
    for (key, value) in [
        ("TERM", "dumb"),
        ("NO_COLOR", "1"),
        ("CLICOLOR", "0"),
        ("PAGER", "cat"),
        ("GIT_PAGER", "cat"),
        ("GIT_TERMINAL_PROMPT", "0"),
    ] {
        env.push((key, value.into()));
    }
    (env, long.then(|| list_bytes(&sel.files)))
}

/// Writes the selection's list to a file only this user can read; its path.
fn write_list(bytes: &[u8]) -> std::io::Result<PathBuf> {
    use std::io::Write;
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("gezik-files-{}-{n}.txt", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(&path)?;
    file.write_all(bytes)?;
    Ok(path)
}

/// One line of output as shown: the end of a line rewritten in place (`\r`) only, UTF-8 or
/// (Windows) the OEM code page console programs write in, escape sequences taken out.
pub fn decode_line(bytes: &[u8]) -> String {
    let bytes = bytes.strip_suffix(b"\r").unwrap_or(bytes);
    let bytes = bytes.rsplit(|&b| b == b'\r').find(|part| !part.is_empty()).unwrap_or_default();
    let text = match std::str::from_utf8(bytes) {
        Ok(text) => text.to_owned(),
        Err(_) => gezik_platform::decode_oem(bytes).unwrap_or_else(|| String::from_utf8_lossy(bytes).into_owned()),
    };
    gezik_core::ansi::strip(&text)
}

/// A line of output and whether it came on the error output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub text: String,
    pub err: bool,
}

impl Line {
    /// What it costs in memory.
    fn cost(&self) -> usize {
        self.text.len() + LINE_COST
    }
}

/// Where a line over [`MAX_LINE`] bytes is cut: after the last byte below `@` in its last
/// kilobyte (space, digit, punctuation: whole characters in UTF-8 and in every OEM code page,
/// whose second bytes are `@` or above), else before a UTF-8 character's first byte.
pub fn cut_at(line: &[u8]) -> usize {
    let start = line.len().saturating_sub(1024);
    if let Some(i) = line[start..].iter().rposition(|&b| b < b'@') {
        return start + i + 1;
    }
    (1..line.len()).rev().find(|&i| line[i] & 0xC0 != 0x80).unwrap_or(line.len())
}

/// The output kept: at most [`MAX_OUTPUT`] (with each line's [`LINE_COST`]), the oldest
/// lines dropped past it.
#[derive(Debug, Default)]
pub struct Output {
    lines: VecDeque<Line>,
    bytes: usize,
    /// Lines were dropped ("Earlier output dropped").
    pub dropped: bool,
}

impl Output {
    /// Adds `lines`; the number of lines dropped from the start to make room (down to three
    /// quarters of the limit at once).
    pub fn push_all(&mut self, lines: Vec<Line>) -> usize {
        for line in lines {
            self.bytes += line.cost();
            self.lines.push_back(line);
        }
        if self.bytes <= MAX_OUTPUT {
            return 0;
        }
        let mut dropped = 0;
        while self.bytes > KEPT_AFTER_DROP && self.lines.len() > 1 {
            let line = self.lines.pop_front().expect("more than one line");
            self.bytes -= line.cost();
            dropped += 1;
        }
        self.dropped = true;
        dropped
    }

    fn len(&self) -> usize {
        self.lines.len()
    }

    #[cfg(test)]
    pub fn lines(&self) -> impl Iterator<Item = &Line> {
        self.lines.iter()
    }

    pub fn text(&self) -> String {
        self.lines.iter().map(|line| line.text.as_str()).collect::<Vec<_>>().join("\n")
    }
}

/// The panel's rows: the output itself, told to the list as lines come and go (lines dropped
/// from the start are one removal, never a rebuild). While the list does not follow its end
/// (the user scrolled up) the rows are frozen: a copy of the output as it was then, nothing
/// added or dropped under the reader; the output goes on (capped) and the lines that came are
/// counted. Following again, the rows are the output once more (one reset).
#[derive(Default)]
pub struct OutputModel {
    output: RefCell<Output>,
    notify: ModelNotify,
    frozen: RefCell<Option<Vec<Line>>>,
    /// Lines that came while frozen.
    new_lines: Cell<usize>,
}

impl OutputModel {
    /// Adds `lines`, dropping the oldest past the limit.
    fn push_all(&self, lines: Vec<Line>) {
        if lines.is_empty() {
            return;
        }
        let added = lines.len();
        let before = self.output.borrow().len();
        let dropped = self.output.borrow_mut().push_all(lines);
        if self.frozen.borrow().is_some() {
            self.new_lines.set(self.new_lines.get() + added);
            return;
        }
        if dropped >= before {
            self.notify.reset();
            return;
        }
        if dropped > 0 {
            self.notify.row_removed(0, dropped);
        }
        self.notify.row_added(before - dropped, added);
    }

    /// The list stops (`false`) or starts following its end.
    fn set_following(&self, on: bool) {
        let frozen = self.frozen.borrow().is_some();
        if on && frozen {
            self.frozen.take();
            self.new_lines.set(0);
            self.notify.reset();
        } else if !on && !frozen {
            *self.frozen.borrow_mut() = Some(self.output.borrow().lines.iter().cloned().collect());
        }
    }

    fn clear(&self) {
        *self.output.borrow_mut() = Output::default();
        self.frozen.take();
        self.new_lines.set(0);
        self.notify.reset();
    }
}

impl Model for OutputModel {
    type Data = CmdLine;

    fn row_count(&self) -> usize {
        match &*self.frozen.borrow() {
            Some(rows) => rows.len(),
            None => self.output.borrow().len(),
        }
    }

    fn row_data(&self, row: usize) -> Option<CmdLine> {
        let row_of = |line: &Line| CmdLine { text: line.text.as_str().into(), err: line.err };
        match &*self.frozen.borrow() {
            Some(rows) => rows.get(row).map(row_of),
            None => self.output.borrow().lines.get(row).map(row_of),
        }
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }
}

/// Puts `command` first in `history` (newest first, no twice, at most 100). A command typed
/// with a space first is not kept (the shells' `ignorespace`: one with a password in it).
pub fn remember(history: &mut Vec<String>, command: &str) -> bool {
    let kept = command.trim_end();
    if command.starts_with(char::is_whitespace) || kept.is_empty() {
        return false;
    }
    history.retain(|old| old != kept);
    history.insert(0, kept.to_owned());
    history.truncate(COMMANDS_MAX);
    true
}

/// The history's commands that hold `typed` (any case), newest first.
pub fn matching<'a>(history: &'a [String], typed: &str) -> Vec<&'a str> {
    let typed = typed.trim().to_lowercase();
    history.iter().filter(|c| c.to_lowercase().contains(&typed)).take(HISTORY_ROWS).map(String::as_str).collect()
}

/// How long `took`: "0.4 s", "12 s", "2 min 5 s".
fn duration_text(took: Duration) -> String {
    let secs = took.as_secs();
    if secs < 10 {
        t!("{} s", format!("{:.1}", took.as_secs_f32()))
    } else if secs < 60 {
        t!("{} s", secs)
    } else {
        // TRANSLATORS: minutes and seconds a command ran
        t!("{0} min {1} s", secs / 60, secs % 60)
    }
}

thread_local! {
    static SETTINGS: RefCell<CommandLineSettings> = RefCell::default();
    /// `[history] remember`.
    static REMEMBER: Cell<bool> = const { Cell::new(true) };
    /// The commands run, newest first: one list for every pane and window.
    static HISTORY: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static STORE: RefCell<Option<ConfigStore>> = const { RefCell::new(None) };
}

/// The history kept in state.toml and where to write it.
pub fn install(store: Option<ConfigStore>, saved: Vec<String>) {
    STORE.with(|s| *s.borrow_mut() = store);
    if REMEMBER.with(Cell::get) {
        HISTORY.with(|h| *h.borrow_mut() = saved);
    } else if !saved.is_empty() {
        save(Vec::new());
    }
}

/// settings.toml changed: the shell, and with `remember = false` the history is forgotten.
pub fn set_settings(settings: CommandLineSettings, remember: bool) {
    SETTINGS.with(|s| *s.borrow_mut() = settings);
    REMEMBER.with(|r| r.set(remember));
    if !remember {
        forget();
    }
}

/// Forgets the commands run (`clear-history`, `remember = false`).
pub fn forget() {
    let had = HISTORY.with(|h| !std::mem::take(&mut *h.borrow_mut()).is_empty());
    if had {
        save(Vec::new());
    }
}

fn save(list: Vec<String>) {
    STORE.with(|s| {
        if let Some(store) = s.borrow().as_ref() {
            store.update_state(move |state| state.command_history = list);
        }
    });
}

/// `typed` ran: it goes first in the history (unless the history is off or it is private).
fn remember_run(typed: &str) {
    if !REMEMBER.with(Cell::get) {
        return;
    }
    let list = HISTORY.with(|h| {
        let mut h = h.borrow_mut();
        remember(&mut h, typed).then(|| h.clone())
    });
    if let Some(list) = list {
        save(list);
    }
}

/// The history's commands for the address bar's list while `typed` (after `>`) is typed.
pub fn history_for(typed: &str) -> Vec<String> {
    HISTORY.with(|h| matching(&h.borrow(), typed).into_iter().map(str::to_owned).collect())
}

/// Lines read and not shown yet (at most [`MAX_PENDING`], the oldest dropped past it), how
/// many of the two streams are still open, and whether the panel stopped reading.
#[derive(Default)]
struct Pending {
    lines: VecDeque<Line>,
    bytes: usize,
    open: u8,
    /// Lines were dropped before the panel took them.
    dropped: bool,
    /// The run ended while something the shell started still holds the pipes: what it writes
    /// is read and dropped.
    orphaned: bool,
}

impl Pending {
    fn push(&mut self, line: Line) {
        if self.orphaned {
            return;
        }
        self.bytes += line.cost();
        self.lines.push_back(line);
        while self.bytes > MAX_PENDING && self.lines.len() > 1 {
            let line = self.lines.pop_front().expect("more than one line");
            self.bytes -= line.cost();
            self.dropped = true;
        }
    }

    /// The first `max` lines.
    fn take(&mut self, max: usize) -> Vec<Line> {
        let lines: Vec<Line> = self.lines.drain(..max.min(self.lines.len())).collect();
        self.bytes -= lines.iter().map(Line::cost).sum::<usize>();
        lines
    }
}

type Shared = Arc<Mutex<Pending>>;

fn lock(shared: &Shared) -> std::sync::MutexGuard<'_, Pending> {
    shared.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Reads `pipe` into `shared` line by line until it closes (a line over [`MAX_LINE`] bytes is
/// cut where [`cut_at`] says).
fn read_stream(mut pipe: impl Read, err: bool, shared: &Shared) {
    let mut buf = [0u8; 8192];
    let mut line = Vec::new();
    let send = |bytes: &[u8]| lock(shared).push(Line { text: decode_line(bytes), err });
    loop {
        match pipe.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                for &byte in &buf[..n] {
                    if byte == b'\n' {
                        send(&line);
                        line.clear();
                    } else {
                        if line.len() >= MAX_LINE {
                            let at = cut_at(&line);
                            send(&line[..at]);
                            line.drain(..at);
                        }
                        line.push(byte);
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
    if !line.is_empty() {
        send(&line);
    }
    lock(shared).open -= 1;
}

/// Whether a key press is a held Enter in the address field: it neither runs a `>` command
/// again nor runs one just chosen from the history (only a fresh press does).
pub fn held_enter(repeat: bool, enter: bool, editing: bool) -> bool {
    repeat && enter && editing
}

/// The command shown in a pane's panel.
#[derive(Default)]
struct State {
    open: bool,
    command: String,
    /// What followed `>`, as typed (a space first keeps it out of the history).
    typed: String,
    selection: Selection,
    /// Runs so far: the panel starts each at its end.
    run: i32,
    /// The shell, kept while what it started may still be stopped.
    child: Option<ChildProcess>,
    shared: Option<Shared>,
    /// The selection's list when it was too long for `GEZIK_FILES`; removed when the run ends.
    list_file: Option<PathBuf>,
    started: Option<Instant>,
    /// It ended: when, and its exit code (none: ended by a signal).
    exited: Option<(Instant, Option<i32>)>,
    running: bool,
    stopped: bool,
    /// The status written last ("Running… 3 s"), so it is written only when it changes.
    status: String,
    /// A program Gezik started for a command of its own (git, spec 16 §9.4): `command` is its
    /// title as it is, no `>` before it.
    own: bool,
    /// The panel opens only if it fails (stage, unstage, switch).
    quiet: bool,
    /// What Run Again does instead of running `typed`.
    again: Option<Rc<dyn Fn()>>,
    /// Told once it ended (stopped, or not started: not ok), with its output.
    on_end: Option<OnEnd>,
    /// Open in Terminal is offered (a git command that needed a password).
    terminal: bool,
    /// What Open in Terminal puts on the clipboard.
    copy: Option<String>,
    /// Stop asked it to stop by itself once already.
    interrupted: bool,
    /// The new-lines bar's text written last, and when: it changes at most twice a second
    /// (each write makes the pane look at its row again).
    new_text: String,
    new_at: Option<Instant>,
}

/// What a program of Gezik's own is told when it ended: whether it succeeded, and its output;
/// it answers whether the panel offers Open in Terminal.
pub type OnEnd = Box<dyn FnOnce(bool, &str) -> bool>;

/// A program Gezik runs in the panel for a command of its own (git, spec 16 §9.4).
pub struct Process {
    /// The panel's title ("git push").
    pub title: String,
    pub dir: PathBuf,
    pub command: std::process::Command,
    /// Written to its input (a commit message, a list of paths).
    pub input: Option<Vec<u8>>,
    pub quiet: bool,
    /// The command line Open in Terminal puts on the clipboard (none: never offered).
    pub copy: Option<String>,
    pub again: Option<Rc<dyn Fn()>>,
    pub on_end: Option<OnEnd>,
}

impl State {
    /// The run is over: the readers drop what still comes, the list file goes.
    fn let_go(&mut self) {
        if let Some(shared) = &self.shared {
            let mut pending = lock(shared);
            pending.orphaned = true;
            pending.lines.clear();
            pending.bytes = 0;
        }
        if let Some(file) = self.list_file.take() {
            let _ = std::fs::remove_file(file);
        }
    }
}

struct Inner {
    id: PaneId,
    state: RefCell<State>,
    model: Rc<OutputModel>,
    timer: slint::Timer,
}

/// A pane's command panel and the command it runs.
#[derive(Clone)]
pub struct CommandLine(Rc<Inner>);

impl CommandLine {
    pub fn new(id: PaneId) -> CommandLine {
        CommandLine(Rc::new(Inner {
            id,
            state: RefCell::default(),
            model: Rc::new(OutputModel::default()),
            timer: slint::Timer::default(),
        }))
    }

    /// The window's panel buttons, for every pane (once).
    pub fn connect(window: &crate::AppWindow) {
        window.on_command_follow(|pane, on| {
            if let Some(p) = crate::panes::at(pane) {
                p.command.0.model.set_following(on);
                p.command.show();
            }
        });
        window.on_command_button(|pane, which| {
            if let Some(p) = crate::pick(pane) {
                match which {
                    0 => p.command.stop(),
                    1 => p.command.run_again(),
                    2 => p.command.copy_output(),
                    4 => p.command.open_terminal(),
                    _ => p.command.close(),
                }
            }
        });
    }

    pub fn is_running(&self) -> bool {
        self.0.state.borrow().running
    }

    /// Enter in the address bar with `typed` (what follows `>`): runs it in the folder `pane`
    /// shows, if it shows one on disk.
    pub fn enter(&self, pane: &Pane, typed: &str) {
        if typed.trim().is_empty() {
            return;
        }
        let Some(dir) = pane.nav.active_location().folder().map(Path::to_path_buf) else {
            return pane.view.note(t!("Commands run in a folder on disk").into_owned());
        };
        let view = &pane.view;
        let file = view.focus().and_then(|i| view.entry_path(i)).map(|(path, _)| path);
        let files: Vec<PathBuf> = view.selected_items().into_iter().map(|(path, _)| path).collect();
        self.start_or_ask(typed.to_owned(), Selection { file, files, dir });
    }

    /// Runs the command shown again, in its folder.
    fn run_again(&self) {
        let (typed, selection, again) = {
            let state = self.0.state.borrow();
            (state.typed.clone(), state.selection.clone(), state.again.clone())
        };
        if let Some(again) = again {
            return again();
        }
        if !typed.trim().is_empty() {
            self.start_or_ask(typed, selection);
        }
    }

    /// Starts `typed` (what followed `>`); while another runs, asks first whether to stop it
    /// (Enter and Esc both keep it running, and nothing is started or kept in the history).
    fn start_or_ask(&self, typed: String, selection: Selection) {
        if !self.is_running() {
            return self.start(typed, selection);
        }
        let this = self.clone();
        crate::operations::with_current(|ops| {
            ops.dialogs().ask_escape(
                t!("Run Command"),
                t!("A command is still running. Stop it?"),
                &[&t!("Cancel"), &t!("Stop and Run")],
                0,
                move |choice| {
                    if choice == Some(1) {
                        this.stop();
                        this.start(typed, selection);
                    }
                },
            );
        });
    }

    fn start(&self, typed: String, selection: Selection) {
        let command = typed.trim().to_owned();
        let shell = SETTINGS.with(|s| pick_shell(&s.borrow(), std::env::var_os("SHELL"), cfg!(windows)));
        let call = invocation(&shell, &command, std::env::var_os("ComSpec"));
        let cmd = shell == Shell::Cmd;
        let (mut env, list) = environment(&selection, cmd, files_limit(&shell, cfg!(windows)));
        let mut notes = Vec::new();
        let mut list_file = None;
        if let Some(bytes) = list {
            match write_list(&bytes) {
                Ok(path) => {
                    env.push(("GEZIK_FILES_LIST", if cmd { quoted(&path) } else { path.clone().into_os_string() }));
                    list_file = Some(path);
                    notes.push(
                        t!("The selection is too long for GEZIK_FILES: GEZIK_FILES_LIST names a file with it")
                            .into_owned(),
                    );
                }
                Err(err) => {
                    notes.push(t!("The selection is too long for GEZIK_FILES and its list was not written: {}", err))
                }
            }
        }
        let mut process = std::process::Command::new(&call.program);
        process.args(&call.args).current_dir(&selection.dir);
        process.envs(env.iter().chain(&call.env).map(|(k, v)| (k, v)));
        #[cfg(windows)]
        if let Some(raw) = &call.raw {
            use std::os::windows::process::CommandExt;
            process.raw_arg(raw);
        }
        let started = if cmd && is_network(&selection.dir) {
            // cmd would run it in the Windows folder instead, saying so only in its output.
            let why = t!("cmd cannot run in a network folder; set [command-line] shell = \"powershell\"");
            Err(std::io::Error::other(why.into_owned()))
        } else {
            ChildProcess::spawn_piped(process)
        };
        let state = State { open: true, command, typed, selection, list_file, ..State::default() };
        self.begin(state, notes, started, &call.program.to_string_lossy());
    }

    /// Runs `process`, a program of Gezik's own, in the panel (no shell, nothing kept in the
    /// history).
    pub fn run_process(&self, process: Process) {
        let Process { title, dir, command, input, quiet, copy, again, on_end } = process;
        let program = command.get_program().to_string_lossy().into_owned();
        let started = ChildProcess::spawn_piped_input(command, input);
        let selection = Selection { dir, ..Selection::default() };
        let state = State {
            open: !quiet,
            command: title,
            selection,
            own: true,
            quiet,
            copy,
            again,
            on_end,
            ..State::default()
        };
        self.begin(state, Vec::new(), started, &program);
    }

    /// Shows `fresh` in the panel with `notes` first, and follows what `started` (`program`).
    fn begin(
        &self,
        fresh: State,
        notes: Vec<String>,
        started: std::io::Result<(ChildProcess, std::process::ChildStdout, std::process::ChildStderr)>,
        program: &str,
    ) {
        let mut failed = None;
        {
            let mut state = self.0.state.borrow_mut();
            // What the last one left running ends now if it may still be stopped.
            state.let_go();
            state.child = None;
            let run = state.run.wrapping_add(1);
            *state = State { run, ..fresh };
            let typed = state.typed.clone();
            self.0.model.clear();
            self.0.model.push_all(notes.into_iter().map(|text| Line { text, err: false }).collect());
            match started {
                Ok((child, stdout, stderr)) => {
                    let shared: Shared = Arc::new(Mutex::new(Pending { open: 2, ..Pending::default() }));
                    for (pipe, err) in [(Box::new(stdout) as Box<dyn Read + Send>, false), (Box::new(stderr), true)] {
                        let shared_for = shared.clone();
                        let name = if err { "gezik-command-err" } else { "gezik-command-out" };
                        let spawned = std::thread::Builder::new()
                            .name(name.into())
                            .spawn(move || read_stream(pipe, err, &shared_for));
                        if spawned.is_err() {
                            lock(&shared).open -= 1;
                        }
                    }
                    state.child = Some(child);
                    state.shared = Some(shared);
                    state.started = Some(Instant::now());
                    state.running = true;
                    // Only what ran is kept (not one cancelled at the question).
                    remember_run(&typed);
                }
                Err(err) => {
                    // TRANSLATORS: {0} is a shell program, {1} why it did not start
                    let text = t!("Could not start {0}: {1}", program, err);
                    self.0.model.push_all(vec![Line { text, err: true }]);
                    state.status = t!("Did not start").into_owned();
                    state.open = true;
                    state.let_go();
                    failed = state.on_end.take();
                }
            }
        }
        if let Some(on_end) = failed {
            on_end(false, "");
        }
        if self.is_running() {
            let id = self.0.id;
            self.0.timer.start(
                slint::TimerMode::Repeated,
                TICK,
                crate::windows::bind_mut(move || {
                    crate::panes::with_id(id, |p| p.command.tick());
                }),
            );
        }
        self.show();
    }

    /// Takes up to [`LINES_PER_TICK`] lines read so far, sees whether the shell ended, and
    /// once it did and everything read was shown (its output closed, or half a second went
    /// by) ends the run. What it left running may go on: it is let go.
    fn tick(&self) {
        let finished = {
            let mut state = self.0.state.borrow_mut();
            if !state.running {
                return;
            }
            let (lines, open, left, dropped) = match &state.shared {
                Some(shared) => {
                    let mut pending = lock(shared);
                    let lines = pending.take(LINES_PER_TICK);
                    (lines, pending.open, pending.lines.len(), std::mem::take(&mut pending.dropped))
                }
                None => (Vec::new(), 0, 0, false),
            };
            self.0.model.push_all(lines);
            if dropped {
                self.0.model.output.borrow_mut().dropped = true;
            }
            if state.exited.is_none()
                && let Some(Ok(Some(status))) = state.child.as_mut().map(ChildProcess::try_wait)
            {
                state.exited = Some((Instant::now(), status.code()));
            }
            match state.exited {
                Some((at, _)) if (open == 0 && left == 0) || at.elapsed() >= GRACE => {
                    // What the grace left unshown (at most MAX_PENDING) comes now.
                    let rest = state.shared.as_ref().map(|shared| lock(shared).take(usize::MAX)).unwrap_or_default();
                    self.0.model.push_all(rest);
                    state.running = false;
                    if let Some(mut child) = state.child.take() {
                        child.let_go();
                    }
                    state.let_go();
                    true
                }
                _ => false,
            }
        };
        if finished {
            self.0.timer.stop();
            self.ended();
            self.show();
            self.refresh_folder();
        } else {
            self.show();
        }
    }

    /// The folder the command ran in, read once more if the pane still shows it (its watcher
    /// does too).
    fn refresh_folder(&self) {
        let dir = self.0.state.borrow().selection.dir.clone();
        crate::panes::with_id(self.0.id, |p| {
            if p.nav.active_location().folder().is_some_and(|shown| gezik_core::ops::paths::same_path(shown, &dir)) {
                p.nav.refresh_due();
            }
        });
    }

    /// `Stop`: the shell and everything it started end now. A shell that had already ended
    /// (its output still coming) keeps its exit code.
    pub fn stop(&self) {
        {
            let mut state = self.0.state.borrow_mut();
            if !state.running {
                return;
            }
            // git first gets a second to stop by itself and take its lock files away (Unix).
            if state.own && !state.interrupted {
                state.interrupted = true;
                if state.child.as_mut().is_some_and(ChildProcess::interrupt) {
                    let id = self.0.id;
                    slint::Timer::single_shot(
                        Duration::from_secs(1),
                        crate::windows::bind(move || {
                            crate::panes::with_id(id, |p| p.command.stop());
                        }),
                    );
                    return;
                }
            }
            if let Some(child) = state.child.as_mut() {
                child.kill_tree();
            }
            state.running = false;
            if state.exited.is_none() {
                state.stopped = true;
                state.exited = Some((Instant::now(), None));
            }
            let rest = state.shared.as_ref().map(|shared| lock(shared).take(usize::MAX)).unwrap_or_default();
            self.0.model.push_all(rest);
            state.child = None;
            state.let_go();
        }
        self.0.timer.stop();
        self.ended();
        self.show();
    }

    /// The run ended: a quiet one that failed opens the panel, and a program of Gezik's own is
    /// told (it may offer Open in Terminal).
    fn ended(&self) {
        let (ok, on_end) = {
            let mut state = self.0.state.borrow_mut();
            let ok = matches!(state.exited, Some((_, Some(0)))) && !state.stopped;
            if state.quiet && !ok {
                state.open = true;
            }
            (ok, state.on_end.take())
        };
        if let Some(on_end) = on_end {
            let text = self.0.model.output.borrow().text();
            let terminal = on_end(ok, &text);
            let mut state = self.0.state.borrow_mut();
            state.terminal = terminal && state.copy.is_some();
        }
    }

    /// `Open in Terminal`: a terminal in the command's folder, the command on the clipboard
    /// (spec 16 §9.4: git wanted a password it cannot ask for here).
    fn open_terminal(&self) {
        let (dir, command) = {
            let state = self.0.state.borrow();
            (state.selection.dir.clone(), state.copy.clone().unwrap_or_default())
        };
        crate::terminal::open_in(dir, false);
        if gezik_platform::clipboard::write_text(&command).is_ok() {
            // TRANSLATORS: {} is a command put on the clipboard ("git push")
            crate::panes::with_id(self.0.id, |p| p.view.note(t!("Copied: {}", command)));
        }
    }

    /// The pane closes (or its window, or Gezik quits): a running command ends with what it
    /// started, quietly; what an ended one left running goes on.
    pub fn end(&self) {
        self.0.timer.stop();
        let on_end = {
            let mut state = self.0.state.borrow_mut();
            state.running = false;
            if let Some(child) = state.child.as_mut() {
                child.kill_tree();
            }
            state.child = None;
            state.let_go();
            state.on_end.take()
        };
        if let Some(on_end) = on_end {
            on_end(false, "");
        }
    }

    /// `Close`: stops a running command and closes the panel.
    pub fn close(&self) {
        self.stop();
        self.0.state.borrow_mut().open = false;
        self.show();
        // The panel may have had the keyboard: the list gets it.
        crate::panes::with_id(self.0.id, |_| {
            crate::windows::with_window(crate::windows::current(), |w| w.invoke_focus_list());
        });
    }

    /// Esc on the list: closes a panel whose command ended; whether it did.
    pub fn escape(&self) -> bool {
        let state = self.0.state.borrow();
        if !state.open || state.running {
            return false;
        }
        drop(state);
        self.close();
        true
    }

    /// `toggle-command-output`: shows or hides the panel (a running command goes on).
    pub fn toggle(&self) {
        {
            let mut state = self.0.state.borrow_mut();
            if state.command.is_empty() && !state.open {
                drop(state);
                return crate::panes::with_id(self.0.id, |p| {
                    p.view.note(t!("No command has run in this pane").into_owned());
                })
                .unwrap_or_default();
            }
            state.open = !state.open;
        }
        self.show();
    }

    fn copy_output(&self) {
        let text = self.0.model.output.borrow().text();
        let note = match gezik_platform::clipboard::write_text(&text) {
            Ok(()) => t!("Output copied").into_owned(),
            Err(_) => t!("Could not copy the output").into_owned(),
        };
        crate::panes::with_id(self.0.id, |p| p.view.note(note));
    }

    /// Writes the panel's header to the pane (only what changed is written).
    fn show(&self) {
        let (open, title, folder, status, state_code, run, terminal) = {
            let mut state = self.0.state.borrow_mut();
            let took = match (state.started, state.exited) {
                (Some(started), Some((at, _))) => Some(at.duration_since(started)),
                (Some(started), None) => Some(started.elapsed()),
                _ => None,
            };
            let status = match (state.running, state.exited, took) {
                (true, _, Some(took)) => {
                    // Whole seconds while it runs: written once a second.
                    t!("Running… {}", duration_text(Duration::from_secs(took.as_secs())))
                }
                (false, Some(_), Some(took)) if state.stopped => t!("Stopped · {}", duration_text(took)),
                (false, Some((_, Some(code))), Some(took)) => {
                    // TRANSLATORS: {0} is a number, {1} how long the command ran ("0.4 s")
                    t!("Exit code {0} · {1}", code, duration_text(took))
                }
                (false, Some((_, None)), Some(took)) => t!("Ended by a signal · {}", duration_text(took)),
                _ => state.status.clone(),
            };
            state.status = status.clone();
            let code = if state.running {
                0
            } else if matches!(state.exited, Some((_, Some(0)))) && !state.stopped {
                1
            } else {
                2
            };
            let folder = state.selection.dir.display().to_string();
            let title = if state.own { state.command.clone() } else { format!("> {}", state.command) };
            (state.open, title, folder, status, code, state.run, state.terminal && !state.running)
        };
        let dropped = self.0.model.output.borrow().dropped;
        let new_lines = {
            let mut state = self.0.state.borrow_mut();
            let n = self.0.model.new_lines.get();
            let due = state.new_at.is_none_or(|at| at.elapsed() >= Duration::from_millis(500));
            if n == 0 || state.new_text.is_empty() || due {
                state.new_text = match n {
                    0 => String::new(),
                    // TRANSLATORS: a bar under the command output while it is scrolled up; End goes there
                    n => gezik_i18n::tn!("{n} new line · End" | "{n} new lines · End", n),
                };
                state.new_at = Some(Instant::now());
            }
            state.new_text.clone()
        };
        let model = ModelRc::from(self.0.model.clone());
        crate::panes::edit(self.0.id, |d| {
            d.cmd_open = open;
            d.cmd_lines = model;
            d.cmd_title = title.into();
            d.cmd_folder = folder.into();
            d.cmd_status = status.into();
            d.cmd_state = state_code;
            d.cmd_dropped = dropped;
            d.cmd_run = run;
            d.cmd_terminal = terminal;
            d.cmd_new_lines = new_lines.into();
        });
    }
}

/// Gezik quits: running commands end with what they started (Unix process groups would
/// outlive it); what ended commands left running in the background goes on, as on Windows.
pub fn end_all() {
    for pane in crate::panes::everywhere() {
        pane.command.end();
    }
}

/// `run-command`: the active pane's address bar with `>` typed, its list the history.
pub fn open_bar(id: PaneId) {
    crate::panes::with_id(id, |p| p.path_box.reset());
    crate::panes::edit(id, |d| {
        crate::panes::path_editing(d, true);
        crate::panes::path_text(d, ">", 1);
    });
    // After the field took the keyboard (which starts typing anew).
    slint::Timer::single_shot(Duration::ZERO, move || {
        crate::panes::with_id(id, |p| p.path_box.typed_text(">".to_owned()));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_leading_gt_makes_a_command() {
        assert_eq!(command_text(">git log -3"), Some("git log -3"));
        assert_eq!(command_text("   > dir"), Some(" dir"));
        assert_eq!(command_text(">"), Some(""));
        assert_eq!(command_text(r"C:\a>b"), None);
        assert_eq!(command_text("/home/ali/a>b"), None);
        assert_eq!(command_text(""), None);
    }

    fn settings(shell: &str, args: &[&str]) -> CommandLineSettings {
        CommandLineSettings { shell: shell.into(), shell_args: args.iter().map(|a| (*a).into()).collect() }
    }

    #[test]
    fn the_shell_follows_the_setting_and_the_system() {
        assert_eq!(pick_shell(&settings("", &[]), None, true), Shell::Cmd);
        assert_eq!(pick_shell(&settings("cmd", &[]), None, true), Shell::Cmd);
        assert_eq!(
            pick_shell(&settings("", &[]), Some("/usr/bin/fish".into()), false),
            Shell::Posix("/usr/bin/fish".into())
        );
        assert_eq!(pick_shell(&settings("", &[]), Some("".into()), false), Shell::Posix("/bin/sh".into()));
        assert_eq!(pick_shell(&settings("pwsh", &[]), None, true), Shell::Pwsh("pwsh".into()));
        assert_eq!(pick_shell(&settings("powershell", &[]), None, false), Shell::Pwsh("powershell".into()));
        assert_eq!(
            pick_shell(&settings(r"C:\msys64\usr\bin\bash.exe", &["-lc"]), None, true),
            Shell::Custom(r"C:\msys64\usr\bin\bash.exe".into(), vec!["-lc".into()])
        );
    }

    #[test]
    fn cmd_gets_the_command_exactly_as_typed() {
        let typed = r#"echo "a b" 100% ^& ğüş & dir "C:\Program Files""#;
        let call = invocation(&Shell::Cmd, typed, Some(r"C:\Windows\system32\cmd.exe".into()));
        assert_eq!(call.program, r"C:\Windows\system32\cmd.exe");
        assert_eq!(call.args, ["/v:on", "/d", "/s", "/c"]);
        let inner = r#""chcp 65001>nul & "C:\Windows\system32\cmd.exe" /d /s /c !GEZIK_COMMAND!""#;
        assert_eq!(call.raw.as_deref(), Some(inner));
        assert_eq!(call.env, [("GEZIK_COMMAND", OsString::from(format!("\"{typed}\"")))]);
        assert_eq!(invocation(&Shell::Cmd, "dir", None).program, "cmd.exe");
    }

    #[test]
    fn posix_and_own_shells_get_one_argument() {
        let typed = r#"echo "$HOME" 'ü' \; %x% ^"#;
        let call = invocation(&Shell::Posix("/bin/zsh".into()), typed, None);
        assert_eq!(call.program, "/bin/zsh");
        assert_eq!(call.args, ["-c", typed]);
        assert_eq!(call.raw, None);
        let call = invocation(&Shell::Custom("bash".into(), vec!["-l".into(), "-c".into()]), typed, None);
        assert_eq!(call.args, ["-l", "-c", typed]);
    }

    #[test]
    fn powershell_gets_the_command_in_base64() {
        let typed = "Write-Output 'ğ \"x\" %y%'";
        let call = invocation(&Shell::Pwsh("pwsh".into()), typed, None);
        assert_eq!(call.program, "pwsh");
        assert_eq!(call.args, ["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", POWERSHELL_RUN]);
        assert!(!POWERSHELL_RUN.contains('"'), "the fixed script needs no quoting of its own");
        assert_eq!(call.env, [("GEZIK_COMMAND", OsString::from(base64(typed.as_bytes())))]);
    }

    #[test]
    fn commands_run_only_in_a_folder_on_disk() {
        use gezik_core::nav::Location;
        // `enter` runs in `folder()`: none in This PC, the trash or inside an archive.
        assert!(Location::Drives.folder().is_none());
        assert!(Location::Trash.folder().is_none());
        let inside = gezik_core::vfs::VfsPath::archive(PathBuf::from("/w/x.zip"));
        assert!(Location::Vfs(inside).folder().is_none());
        assert_eq!(Location::Path(PathBuf::from("/w")).folder(), Some(Path::new("/w")));
    }

    #[test]
    fn cmd_is_kept_out_of_network_folders() {
        assert!(is_network(Path::new(r"\\nas\share\dir")));
        assert!(is_network(Path::new("//nas/share")));
        assert!(is_network(Path::new(r"\\?\UNC\nas\share")));
        assert!(!is_network(Path::new(r"\\?\C:\dir")));
        assert!(!is_network(Path::new(r"C:\Users")));
        assert!(!is_network(Path::new("/home/ali")));
    }

    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(&[0xff, 0xfe, 0x00]), "//4A");
    }

    fn get(env: &[(&'static str, OsString)], key: &str) -> Option<OsString> {
        env.iter().find(|(k, _)| *k == key).map(|(_, v)| v.clone())
    }

    fn selection(dir: &str, files: &[&str]) -> Selection {
        let files: Vec<PathBuf> = files.iter().map(PathBuf::from).collect();
        Selection { file: files.first().cloned(), files, dir: dir.into() }
    }

    #[test]
    fn the_environment_names_the_selection() {
        let sel = selection("/w", &["/w/a b.txt", "/w/ç.rs"]);
        let (env, list) = environment(&sel, false, 1000);
        assert_eq!(get(&env, "GEZIK_FILE").unwrap(), "/w/a b.txt");
        assert_eq!(get(&env, "GEZIK_FILES").unwrap(), "/w/a b.txt\n/w/ç.rs");
        assert_eq!(get(&env, "GEZIK_DIR").unwrap(), "/w");
        assert_eq!(get(&env, "TERM").unwrap(), "dumb");
        assert_eq!(get(&env, "GIT_TERMINAL_PROMPT").unwrap(), "0");
        assert_eq!(list, None);
        let (env, _) = environment(&Selection { dir: "/w".into(), ..Selection::default() }, false, 1000);
        assert_eq!(get(&env, "GEZIK_FILE").unwrap(), "");
    }

    #[test]
    fn cmd_gets_every_path_in_quotes() {
        let sel = selection(r"C:\w&x", &[r"C:\w\a&b.txt", r"C:\w\c (1)^.txt"]);
        let (env, _) = environment(&sel, true, 1000);
        assert_eq!(get(&env, "GEZIK_FILE").unwrap(), r#""C:\w\a&b.txt""#);
        assert_eq!(get(&env, "GEZIK_FILES").unwrap(), r#""C:\w\a&b.txt" "C:\w\c (1)^.txt""#);
        assert_eq!(get(&env, "GEZIK_DIR").unwrap(), r#""C:\w&x""#);
    }

    #[test]
    fn a_long_selection_goes_to_a_list_file() {
        let files: Vec<String> = (0..5_000).map(|i| format!("/w/a long name for file {i:05}.txt")).collect();
        let names: Vec<&str> = files.iter().map(String::as_str).collect();
        let sel = selection("/w", &names);
        for shell in [Shell::Cmd, Shell::Pwsh("pwsh".into()), Shell::Posix("/bin/sh".into())] {
            for windows in [true, false] {
                let (env, list) = environment(&sel, shell == Shell::Cmd, files_limit(&shell, windows));
                assert_eq!(get(&env, "GEZIK_FILES"), None, "left out");
                let first = "/w/a long name for file 00000.txt";
                let want = if shell == Shell::Cmd { format!("\"{first}\"") } else { first.to_owned() };
                assert_eq!(get(&env, "GEZIK_FILE").unwrap(), want.as_str());
                let list = String::from_utf8(list.expect("a list")).unwrap();
                assert_eq!(list.lines().count(), 5_000);
                assert!(list.ends_with("file 04999.txt\n"));
            }
        }
        let (env, list) = environment(&selection("/w", &names[..10]), true, files_limit(&Shell::Cmd, true));
        assert!(get(&env, "GEZIK_FILES").is_some() && list.is_none(), "a short one stays");
        let path = write_list(b"/w/a\n").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"/w/a\n");
        assert!(write_list(b"x").unwrap() != path, "a fresh file each time");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn lines_are_decoded_and_cleaned() {
        assert_eq!(decode_line(b"plain\r"), "plain");
        assert_eq!(decode_line(b"10%\r50%\r100%"), "100%");
        assert_eq!(decode_line("\x1b[32mğüş\x1b[0m".as_bytes()), "ğüş");
        assert_eq!(decode_line(b""), "");
    }

    #[cfg(windows)]
    #[test]
    fn a_line_not_in_utf8_falls_back_to_the_oem_code_page() {
        // 0x81 is ü in the OEM code pages of Turkish (857) and English (437) Windows.
        let oem = gezik_platform::decode_oem(b"\x81ber").unwrap();
        assert_eq!(decode_line(b"\x81ber"), oem);
        assert_ne!(oem, String::from_utf8_lossy(b"\x81ber"));
    }

    #[test]
    fn output_keeps_two_megabytes_counting_each_lines_cost() {
        let mut out = Output::default();
        let fits = MAX_OUTPUT / (1000 + LINE_COST);
        let line = || Line { text: "x".repeat(1000), err: false };
        assert_eq!(out.push_all((0..fits).map(|_| line()).collect()), 0);
        assert!(!out.dropped);
        let dropped = out.push_all(vec![line(), Line { text: "last".into(), err: true }]);
        assert!(out.dropped && dropped > 0);
        assert!(out.bytes <= KEPT_AFTER_DROP);
        assert_eq!(out.len(), fits + 2 - dropped);
        assert_eq!(out.lines().last().map(|l| (l.text.as_str(), l.err)), Some(("last", true)));
        // Empty lines count too: a million of them is no 64 KB.
        let mut out = Output::default();
        out.push_all((0..1_000_000).map(|_| Line { text: String::new(), err: false }).collect());
        assert!(out.len() * LINE_COST <= MAX_OUTPUT);
    }

    #[test]
    fn the_model_drops_from_the_front_without_a_rebuild() {
        let model = OutputModel::default();
        model.push_all(vec![Line { text: "a".into(), err: false }, Line { text: "b".into(), err: true }]);
        assert_eq!(model.row_count(), 2);
        assert_eq!(model.row_data(1).map(|r| (r.text.to_string(), r.err)), Some(("b".into(), true)));
        let big = || Line { text: "y".repeat(100_000), err: false };
        model.push_all((0..30).map(|_| big()).collect());
        assert!(model.output.borrow().dropped);
        assert!(model.row_count() < 32);
        assert_eq!(model.row_data(0).map(|r| r.text.len()), Some(100_000), "the oldest went first");
        model.clear();
        assert_eq!(model.row_count(), 0);
    }

    #[test]
    fn readers_hold_at_most_two_megabytes_and_drop_after_the_run() {
        let mut pending = Pending::default();
        for i in 0..100_000 {
            pending.push(Line { text: format!("{i:0>100}"), err: false });
        }
        assert!(pending.bytes <= MAX_PENDING && pending.dropped);
        let first = pending.take(LINES_PER_TICK);
        assert_eq!(first.len(), LINES_PER_TICK);
        assert_eq!(pending.bytes, pending.lines.iter().map(Line::cost).sum::<usize>());
        assert_eq!(pending.lines.back().map(|l| l.text.as_str()), Some(format!("{:0>100}", 99_999).as_str()));
        pending.orphaned = true;
        let before = pending.lines.len();
        pending.push(Line { text: "late".into(), err: false });
        assert_eq!(pending.lines.len(), before, "an ended run reads and drops");
    }

    #[test]
    fn long_lines_are_cut_between_characters() {
        // A space in the last kilobyte: cut after it.
        let mut line = "ğ".repeat(9000).into_bytes();
        line.extend_from_slice(b" tail");
        let at = cut_at(&line);
        assert_eq!(&line[at - 1..at], b" ");
        // None: before a UTF-8 character, never inside one.
        let line = "ğ".repeat(9000).into_bytes();
        let at = cut_at(&line);
        assert!(std::str::from_utf8(&line[..at]).is_ok() && std::str::from_utf8(&line[at..]).is_ok());
        assert!(at > line.len() - 4);
        // Read through: every piece decodes whole.
        let mut text = "ğüş".repeat(10_000);
        text.push('\n');
        let shared: Shared = Arc::new(Mutex::new(Pending { open: 1, ..Pending::default() }));
        read_stream(text.as_bytes(), false, &shared);
        let pieces: String = lock(&shared).lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(pieces, text.trim_end());
    }

    #[test]
    fn a_held_enter_runs_nothing() {
        assert!(held_enter(true, true, true));
        assert!(!held_enter(false, true, true), "a fresh press runs");
        assert!(!held_enter(true, false, true));
        assert!(!held_enter(true, true, false), "not in the address field");
    }

    #[test]
    fn history_is_newest_first_without_twins_or_private_commands() {
        let mut h = Vec::new();
        assert!(remember(&mut h, "dir"));
        assert!(remember(&mut h, "git status  "));
        assert!(remember(&mut h, "dir"));
        assert_eq!(h, ["dir", "git status"]);
        assert!(!remember(&mut h, " mysql -p secret"), "a space first keeps it out");
        assert!(!remember(&mut h, "   "));
        for i in 0..150 {
            remember(&mut h, &format!("echo {i}"));
        }
        assert_eq!(h.len(), COMMANDS_MAX);
        assert_eq!(h[0], "echo 149");
        let want: Vec<String> = (140..150).rev().map(|i| format!("echo {i}")).collect();
        assert_eq!(matching(&h, "ECHO 14"), want);
        assert_eq!(matching(&h, "").len(), HISTORY_ROWS);
    }

    #[test]
    fn only_a_run_goes_into_the_history_and_only_while_it_is_on() {
        HISTORY.with(|h| h.borrow_mut().clear());
        remember_run("dir");
        remember_run(" secret");
        assert_eq!(history_for(""), ["dir"]);
        REMEMBER.with(|r| r.set(false));
        remember_run("echo off");
        REMEMBER.with(|r| r.set(true));
        assert_eq!(history_for(""), ["dir"]);
    }

    /// The panel follows its end as lines come (also once the oldest are dropped), stays where
    /// it is once scrolled up, and starts each run at its end (headless, as drawn).
    #[test]
    fn the_output_panel_follows_its_end() {
        use slint::ComponentHandle;
        crate::windows::tests::platform();
        let probe = crate::CommandOutputProbe::new().unwrap();
        probe.show().unwrap();
        let model = Rc::new(OutputModel::default());
        probe.set_lines(ModelRc::from(model.clone()));
        probe.on_follow_changed({
            let model = model.clone();
            move |on| model.set_following(on)
        });
        let settle = || {
            for _ in 0..4 {
                std::thread::sleep(Duration::from_millis(60));
                crate::windows::tests::frame((-1.0, -1.0));
            }
        };
        let add = |from: usize, n: usize| {
            model.push_all((from..from + n).map(|i| Line { text: format!("line {i}"), err: false }).collect());
        };
        let at_end = |what: &str| {
            let (y, end) = (probe.get_view_y(), probe.get_view_end());
            assert!(end < 0.0, "{what}: more lines than room");
            assert!((y - end).abs() < 1.0, "{what}: at {y}, the end is {end}");
        };
        settle();
        for i in 0..5 {
            add(i * 20, 20);
            settle();
            at_end("lines coming");
        }
        // The cap drops the oldest lines while more come.
        for i in 0..3 {
            model.output.borrow_mut().lines.drain(..30);
            model.notify.row_removed(0, 30);
            add(1000 + i * 40, 40);
            settle();
            at_end("after a drop");
        }
        // Scrolled up by the wheel: new lines leave it there.
        let step = probe.get_line_step();
        probe.window().dispatch_event(slint::platform::WindowEvent::PointerScrolled {
            position: slint::LogicalPosition::new(200.0, 200.0),
            delta_x: 0.0,
            delta_y: step * 10.0,
        });
        settle();
        let up = probe.get_view_y();
        assert!(up > probe.get_view_end() + step, "scrolled up: {up}");
        add(2000, 30);
        settle();
        assert!((probe.get_view_y() - up).abs() < 1.0, "stays put: {} vs {up}", probe.get_view_y());
        // Back down by the wheel: it follows again.
        probe.window().dispatch_event(slint::platform::WindowEvent::PointerScrolled {
            position: slint::LogicalPosition::new(200.0, 200.0),
            delta_x: 0.0,
            delta_y: -step * 1000.0,
        });
        settle();
        add(2100, 10);
        settle();
        at_end("scrolled back down");
        // "Earlier output dropped" shows above the list (it gets lower).
        probe.set_dropped(true);
        add(2200, 5);
        settle();
        at_end("with the note");
        // A new run starts at its end.
        model.clear();
        probe.set_run(1);
        add(3000, 50);
        settle();
        at_end("a new run");
    }

    /// While the cap drops the oldest lines (`dir /s C:\\Windows`): scrolled up, the rows stay
    /// as they are (counted lines wait); End brings the end within a frame and follows.
    #[test]
    fn the_output_panel_keeps_its_place_while_lines_are_dropped() {
        use slint::ComponentHandle;
        use slint::platform::{Key, PointerEventButton, WindowEvent};
        crate::windows::tests::platform();
        let probe = crate::CommandOutputProbe::new().unwrap();
        probe.show().unwrap();
        let model = Rc::new(OutputModel::default());
        probe.set_lines(ModelRc::from(model.clone()));
        // As the app's `command-follow`.
        probe.on_follow_changed({
            let model = model.clone();
            move |on| model.set_following(on)
        });
        let frame = || {
            let started = Instant::now();
            crate::windows::tests::frame((-1.0, -1.0));
            started.elapsed()
        };
        let next = Cell::new(0usize);
        // A tick's worth of 200-character lines.
        let tick = || {
            let from = next.get();
            next.set(from + LINES_PER_TICK);
            model.push_all(
                (from..from + LINES_PER_TICK).map(|i| Line { text: format!("{i:0>200}"), err: false }).collect(),
            );
            // The bar's text, as `show` writes it.
            let n = model.new_lines.get();
            probe.set_new_lines(if n == 0 { "".into() } else { format!("{n} new lines · End").into() });
            // A frame makes only the rows on screen (a list that made every row froze).
            let took = frame();
            assert!(took < Duration::from_millis(500), "a frame took {took:?}");
        };
        let step = probe.get_line_step();
        let first_shown = || {
            let row = (-probe.get_view_y() / step).round() as usize;
            model.row_data(row).map(|line| line.text.to_string()).unwrap_or_default()
        };
        let at_end = || (probe.get_view_y() - probe.get_view_end()).abs() < 1.0;
        let key = |key: Key| {
            let text: slint::SharedString = key.into();
            probe.window().dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
            probe.window().dispatch_event(WindowEvent::KeyReleased { text });
        };
        // A real window's scale (fractional positions).
        probe.window().dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: 1.25 });
        // The first frame loads the fonts.
        frame();
        // Following until the cap drops lines.
        for _ in 0..6 {
            tick();
            frame();
            assert!(at_end(), "follows");
        }
        assert!(model.output.borrow().dropped);
        // A click gives the output the keyboard; PageUp leaves the end.
        let at = slint::LogicalPosition::new(200.0, 200.0);
        let button = PointerEventButton::Left;
        probe.window().dispatch_event(WindowEvent::PointerPressed { position: at, button });
        probe.window().dispatch_event(WindowEvent::PointerReleased { position: at, button });
        key(Key::PageUp);
        frame();
        assert!(!at_end());
        let shown = first_shown();
        let (rows, y) = (model.row_count(), probe.get_view_y());
        assert!(!shown.is_empty());
        // Lines come and old ones are dropped: the rows on screen do not change, the new ones
        // are counted.
        for n in 1..=3 {
            tick();
            frame();
            assert_eq!((first_shown(), model.row_count(), probe.get_view_y()), (shown.clone(), rows, y));
            assert_eq!(model.new_lines.get(), n * LINES_PER_TICK);
        }
        assert!(model.output.borrow().len() < rows + 3 * LINES_PER_TICK, "the output stayed capped");
        // End: the rows are the output again, at their end within a frame, and they follow.
        key(Key::End);
        frame();
        assert_eq!(model.new_lines.get(), 0);
        assert_eq!(model.row_count(), model.output.borrow().len());
        assert!(at_end(), "End reached the end: {} vs {}", probe.get_view_y(), probe.get_view_end());
        for _ in 0..4 {
            tick();
            frame();
            assert!(at_end(), "follows after End: {} vs {}", probe.get_view_y(), probe.get_view_end());
        }
        // Reaching the frozen rows' own end follows again, as End does: by the wheel, by the
        // keys, and by the scroll bar's thumb dragged down.
        let following_bottom = probe.get_bar_bottom();
        let frozen = |how: &str| {
            key(Key::PageUp);
            frame();
            tick();
            frame();
            assert!(model.new_lines.get() > 0, "{how}: frozen");
            assert!(!at_end(), "{how}: up from the end");
            // The new-lines bar has a row of its own: the list ends above it.
            assert!(probe.get_bar_bottom() < following_bottom - step, "{how}: the bar covers the list");
        };
        let resumed = |how: &str| {
            tick();
            frame();
            frame();
            assert_eq!(model.new_lines.get(), 0, "{how}: following again");
            assert_eq!(model.row_count(), model.output.borrow().len(), "{how}");
            assert!(at_end(), "{how}: at {} vs {}", probe.get_view_y(), probe.get_view_end());
        };
        frozen("wheel");
        probe.window().dispatch_event(WindowEvent::PointerScrolled {
            position: at,
            delta_x: 0.0,
            delta_y: -step * 3.0,
        });
        frame();
        probe.window().dispatch_event(WindowEvent::PointerScrolled {
            position: at,
            delta_x: 0.0,
            delta_y: -step * 300.0,
        });
        resumed("wheel");
        frozen("PageDown");
        key(Key::PageDown);
        frame();
        key(Key::PageDown);
        resumed("PageDown");
        frozen("Down");
        for _ in 0..40 {
            key(Key::DownArrow);
        }
        resumed("Down");
        // The thumb: grabbed at the bottom of the bar (where it sits after a page up in a long
        // output) and dragged below it.
        frozen("thumb");
        let (x, bottom) = (probe.get_bar_x(), probe.get_bar_bottom());
        let grab = slint::LogicalPosition::new(x, bottom - 4.0);
        let below = slint::LogicalPosition::new(x, bottom + 40.0);
        probe.window().dispatch_event(WindowEvent::PointerPressed { position: grab, button });
        probe.window().dispatch_event(WindowEvent::PointerMoved { position: below });
        probe.window().dispatch_event(WindowEvent::PointerReleased { position: below, button });
        resumed("thumb");
    }

    #[test]
    fn durations_read_as_seconds_and_minutes() {
        assert_eq!(duration_text(Duration::from_millis(420)), "0.4 s");
        assert_eq!(duration_text(Duration::from_secs(12)), "12 s");
        assert_eq!(duration_text(Duration::from_secs(125)), "2 min 5 s");
    }

    /// Runs `command` as the panel would (no UI): its lines and exit code, or `None` if it
    /// did not end within 20 s.
    fn run(shell: &Shell, command: &str, dir: &Path, env: &[(&'static str, OsString)]) -> (Vec<Line>, Option<i32>) {
        let (child, shared) = spawn(shell, command, dir, env);
        let mut child = child;
        let deadline = Instant::now() + Duration::from_secs(20);
        let code = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status.code();
            }
            assert!(Instant::now() < deadline, "{command} did not end");
            std::thread::sleep(Duration::from_millis(20));
        };
        let until = Instant::now() + Duration::from_secs(5);
        while lock(&shared).open > 0 && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(10));
        }
        (lock(&shared).take(usize::MAX), code)
    }

    fn spawn(shell: &Shell, command: &str, dir: &Path, env: &[(&'static str, OsString)]) -> (ChildProcess, Shared) {
        let call = invocation(shell, command, std::env::var_os("ComSpec"));
        let mut process = std::process::Command::new(&call.program);
        process.args(&call.args).current_dir(dir).envs(env.iter().chain(&call.env).map(|(k, v)| (k, v)));
        #[cfg(windows)]
        if let Some(raw) = &call.raw {
            use std::os::windows::process::CommandExt;
            process.raw_arg(raw);
        }
        let (child, stdout, stderr) = ChildProcess::spawn_piped(process).unwrap();
        let shared: Shared = Arc::new(Mutex::new(Pending { open: 2, ..Pending::default() }));
        let (a, b) = (shared.clone(), shared.clone());
        std::thread::spawn(move || read_stream(stdout, false, &a));
        std::thread::spawn(move || read_stream(stderr, true, &b));
        (child, shared)
    }

    fn system_shell() -> Shell {
        pick_shell(&CommandLineSettings::default(), Some("/bin/sh".into()), cfg!(windows))
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gezik-cmd-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn texts(lines: &[Line], err: bool) -> Vec<&str> {
        lines.iter().filter(|l| l.err == err).map(|l| l.text.as_str()).collect()
    }

    #[test]
    fn a_command_runs_in_the_folder_with_the_selection_and_both_streams() {
        let dir = temp("run");
        std::fs::write(dir.join("ğ ü.txt"), "ok").unwrap();
        // cmd's echo writes in the OEM code page: the name echoed back is plain.
        let sel = Selection { file: Some(dir.join("a.txt")), files: vec![dir.join("a.txt")], dir: dir.clone() };
        let (env, _) = environment(&sel, cfg!(windows), 1000);
        let command = if cfg!(windows) {
            r#"type "ğ ü.txt" & echo. & echo %GEZIK_FILE% & echo a "b" 100%& echo err 1>&2 & exit /b 3"#
        } else {
            r#"cat 'ğ ü.txt'; echo; echo "\"$GEZIK_FILE\""; echo a "\"b\"" 100%; echo err >&2; exit 3"#
        };
        let (lines, code) = run(&system_shell(), command, &dir, &env);
        assert_eq!(code, Some(3));
        let out: Vec<String> = texts(&lines, false).iter().map(|l| l.trim_end().to_owned()).collect();
        assert_eq!(out[0], "ok");
        assert_eq!(out[1], format!("\"{}\"", dir.join("a.txt").display()));
        assert_eq!(out[2], r#"a "b" 100%"#);
        assert_eq!(texts(&lines, true).iter().map(|l| l.trim_end()).collect::<Vec<_>>(), ["err"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_program_waiting_for_input_ends_at_once() {
        let dir = temp("input");
        let command = if cfg!(windows) { "set /p x=Name: & sort" } else { "read x; cat" };
        let started = Instant::now();
        let (_, code) = run(&system_shell(), command, &dir, &[]);
        assert!(code.is_some());
        assert!(started.elapsed() < Duration::from_secs(10));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stop_ends_what_the_shell_started_too() {
        let dir = temp("stop");
        // The shell starts a program of its own that holds the output open.
        let command = if cfg!(windows) { "ping -n 60 127.0.0.1" } else { "sleep 60; echo late" };
        let (mut child, shared) = spawn(&system_shell(), command, &dir, &[]);
        std::thread::sleep(Duration::from_millis(500));
        child.kill_tree();
        // The output closes only once every process that held it ended.
        let until = Instant::now() + Duration::from_secs(10);
        while lock(&shared).open > 0 {
            assert!(Instant::now() < until, "a child of the shell still runs");
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A name with `&` in it is text, never a second command (cmd puts `%GEZIK_FILE%` in
    /// before it reads the line).
    #[cfg(windows)]
    #[test]
    fn a_selected_name_cannot_run_a_command() {
        let dir = temp("inject");
        let name = dir.join("a&echo INJECTED&rem .txt");
        let sel =
            Selection { file: Some(name.clone()), files: vec![name.clone(), dir.join("b|c.txt")], dir: dir.clone() };
        let (env, _) = environment(&sel, true, 7_000);
        let (lines, code) = run(&Shell::Cmd, "echo %GEZIK_FILE% & echo %GEZIK_FILES%", &dir, &env);
        assert_eq!(code, Some(0));
        let out: Vec<&str> = texts(&lines, false).iter().map(|l| l.trim_end()).collect();
        assert_eq!(out[0], format!("\"{}\"", name.display()));
        assert_eq!(out[1], format!("\"{}\" \"{}\"", name.display(), dir.join("b|c.txt").display()));
        assert!(!out.contains(&"INJECTED"), "{out:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Letters the OEM code page lacks (437 has no ğ, ş, ı, İ) come through cmd's own commands
    /// and a console program alike; the exit code is the command's, not `chcp`'s.
    #[cfg(windows)]
    #[test]
    fn cmd_output_keeps_every_letter() {
        let dir = temp("letters");
        std::fs::create_dir_all(dir.join("Türkçe ğüşıİöç")).unwrap();
        let typed = r#"echo copy ğüşıİ test & echo a^&b ^^ 100% "x&y" !z! & dir /b& exit /b 4"#;
        let (lines, code) = run(&Shell::Cmd, typed, &dir, &[]);
        assert_eq!(code, Some(4));
        let out: Vec<&str> = texts(&lines, false).iter().map(|l| l.trim_end()).collect();
        assert_eq!(out, ["copy ğüşıİ test", r#"a&b ^ 100% "x&y" !z!"#, "Türkçe ğüşıİöç"]);
        // where.exe, a console program, names a file with them.
        std::fs::write(dir.join("ğüşıİ.txt"), "").unwrap();
        let (lines, _) = run(&Shell::Cmd, "where /r . *.txt", &dir, &[]);
        assert!(texts(&lines, false).iter().any(|l| l.ends_with("ğüşıİ.txt")), "{lines:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn powershell_runs_the_encoded_command() {
        let dir = temp("ps");
        let (lines, code) =
            run(&Shell::Pwsh("powershell".into()), r#"Write-Output 'a "b" ğüş %x%'; exit 2"#, &dir, &[]);
        assert_eq!((texts(&lines, false), code), (vec![r#"a "b" ğüş %x%"#], Some(2)));
        // Errors come as text, not CLIXML.
        let (lines, _) = run(&Shell::Pwsh("powershell".into()), "Get-Item nope", &dir, &[]);
        let errors = texts(&lines, true);
        assert!(errors.iter().any(|l| l.contains("nope")), "{errors:?}");
        assert!(!errors.iter().any(|l| l.contains("CLIXML") || l.contains("<Objs")), "{errors:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
