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
use slint::{ModelRc, VecModel};

use crate::CmdLine;
use crate::panes::{Pane, PaneId};

/// The most output kept (spec 16 §7.3); past it the oldest lines go.
pub const MAX_OUTPUT: usize = 2 * 1024 * 1024;
/// What is kept once the output went past [`MAX_OUTPUT`]: lines go in batches, not one by one.
const KEPT_AFTER_DROP: usize = MAX_OUTPUT / 4 * 3;
/// The longest line passed on; a longer one is cut into lines of this length.
const MAX_LINE: usize = 16 * 1024;
/// How often the panel takes the new lines while a command runs (at most 30 times a second).
const TICK: Duration = Duration::from_millis(33);
/// How long output may still come once the shell ended (a program it left running holds the
/// pipes open).
const GRACE: Duration = Duration::from_millis(500);
/// History rows the address bar lists under a command.
const HISTORY_ROWS: usize = 20;

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

/// A program to start and its arguments; `raw` goes on Windows' command line as it is, after
/// them (cmd reads its own command line, not the C runtime's quoting).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub program: OsString,
    pub args: Vec<OsString>,
    pub raw: Option<String>,
}

/// What PowerShell runs before the command: its output in UTF-8, so nothing is lost to the
/// console's code page.
/// (Its progress bars would come as CLIXML on the error output: they are off.)
const POWERSHELL_UTF8: &str =
    "[Console]::OutputEncoding=[Text.UTF8Encoding]::new();$ProgressPreference='SilentlyContinue';";

/// How `shell` runs `command`, as typed. cmd gets `/s /c "<command>"`: it takes off the outer
/// quotes and runs the rest exactly as typed, `"`, `%`, `^` and all; PowerShell gets it in
/// base64 (UTF-16), which no quoting can break; a POSIX shell gets it as one argument.
/// `comspec` is `%ComSpec%`, cmd's own path.
pub fn invocation(shell: &Shell, command: &str, comspec: Option<OsString>) -> Invocation {
    let plain = |program: OsString, args: Vec<OsString>| Invocation { program, args, raw: None };
    match shell {
        Shell::Cmd => Invocation {
            program: comspec.filter(|c| !c.is_empty()).unwrap_or_else(|| "cmd.exe".into()),
            args: ["/d", "/s", "/c"].map(OsString::from).to_vec(),
            raw: Some(format!("\"{command}\"")),
        },
        Shell::Pwsh(program) => {
            let script: Vec<u8> =
                format!("{POWERSHELL_UTF8}{command}").encode_utf16().flat_map(u16::to_le_bytes).collect();
            let args = ["-NoLogo", "-NoProfile", "-NonInteractive", "-EncodedCommand"]
                .map(OsString::from)
                .into_iter()
                .chain([base64(&script).into()])
                .collect();
            plain(program.into(), args)
        }
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

/// Standard base64 with padding (PowerShell's `-EncodedCommand`).
fn base64(bytes: &[u8]) -> String {
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

/// The environment a command gets (spec 16 §7.1, §7.2): the focused item, the selected ones
/// (a path per line) and the folder, and what keeps programs from waiting on a terminal or
/// drawing colors.
pub fn environment(file: Option<&Path>, files: &[PathBuf], dir: &Path) -> Vec<(&'static str, OsString)> {
    let mut list = OsString::new();
    for (i, path) in files.iter().enumerate() {
        if i > 0 {
            list.push("\n");
        }
        list.push(path);
    }
    let mut env = vec![
        ("GEZIK_FILE", file.map_or_else(OsString::new, |f| f.as_os_str().to_owned())),
        ("GEZIK_FILES", list),
        ("GEZIK_DIR", dir.as_os_str().to_owned()),
    ];
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
    env
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

/// The output kept: at most [`MAX_OUTPUT`] bytes, the oldest lines dropped past it.
#[derive(Debug, Default)]
pub struct Output {
    lines: VecDeque<Line>,
    bytes: usize,
    /// Lines were dropped ("Earlier output dropped").
    pub dropped: bool,
}

impl Output {
    /// Adds `line`; the number of lines dropped from the start to make room (down to three
    /// quarters of the limit at once).
    pub fn push(&mut self, line: Line) -> usize {
        self.bytes += line.text.len() + 1;
        self.lines.push_back(line);
        if self.bytes <= MAX_OUTPUT {
            return 0;
        }
        let mut dropped = 0;
        while self.bytes > KEPT_AFTER_DROP && self.lines.len() > 1 {
            let line = self.lines.pop_front().expect("more than one line");
            self.bytes -= line.text.len() + 1;
            dropped += 1;
        }
        self.dropped = true;
        dropped
    }

    pub fn lines(&self) -> impl Iterator<Item = &Line> {
        self.lines.iter()
    }

    pub fn text(&self) -> String {
        self.lines.iter().map(|line| line.text.as_str()).collect::<Vec<_>>().join("\n")
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

/// The history's commands for the address bar's list while `typed` (after `>`) is typed.
pub fn history_for(typed: &str) -> Vec<String> {
    HISTORY.with(|h| matching(&h.borrow(), typed).into_iter().map(str::to_owned).collect())
}

/// Lines read and not shown yet, and how many of the two streams are still open.
struct Pending {
    lines: Vec<Line>,
    open: u8,
}

type Shared = Arc<Mutex<Pending>>;

fn lock(shared: &Shared) -> std::sync::MutexGuard<'_, Pending> {
    shared.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Reads `pipe` into `shared` line by line until it closes.
fn read_stream(mut pipe: impl Read, err: bool, shared: &Shared) {
    let mut buf = [0u8; 8192];
    let mut line = Vec::new();
    let send = |line: &mut Vec<u8>| {
        lock(shared).lines.push(Line { text: decode_line(line), err });
        line.clear();
    };
    loop {
        match pipe.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                for &byte in &buf[..n] {
                    if byte == b'\n' {
                        send(&mut line);
                    } else {
                        if line.len() >= MAX_LINE {
                            send(&mut line);
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
        send(&mut line);
    }
    lock(shared).open -= 1;
}

/// The command shown in a pane's panel.
#[derive(Default)]
struct State {
    open: bool,
    command: String,
    dir: PathBuf,
    env: Vec<(&'static str, OsString)>,
    /// The shell, kept after it ended until the next command or the panel closes (on Windows
    /// what it left running ends then, with its job object).
    child: Option<ChildProcess>,
    shared: Option<Shared>,
    started: Option<Instant>,
    /// It ended: when, and its exit code (none: ended by a signal).
    exited: Option<(Instant, Option<i32>)>,
    running: bool,
    stopped: bool,
    output: Output,
    /// The status written last ("Running… 3 s"), so it is written only when it changes.
    status: String,
}

struct Inner {
    id: PaneId,
    state: RefCell<State>,
    model: Rc<VecModel<CmdLine>>,
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
            model: Rc::new(VecModel::default()),
            timer: slint::Timer::default(),
        }))
    }

    /// The window's panel buttons, for every pane (once).
    pub fn connect(window: &crate::AppWindow) {
        window.on_command_button(|pane, which| {
            if let Some(p) = crate::pick(pane) {
                match which {
                    0 => p.command.stop(),
                    1 => p.command.run_again(),
                    2 => p.command.copy_output(),
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
        let command = typed.trim();
        if command.is_empty() {
            return;
        }
        let Some(dir) = pane.nav.active_location().folder().map(Path::to_path_buf) else {
            return pane.view.note(t!("Commands run in a folder on disk").into_owned());
        };
        if REMEMBER.with(Cell::get) {
            let list = HISTORY.with(|h| {
                let mut h = h.borrow_mut();
                remember(&mut h, typed).then(|| h.clone())
            });
            if let Some(list) = list {
                save(list);
            }
        }
        let view = &pane.view;
        let file = view.focus().and_then(|i| view.entry_path(i)).map(|(path, _)| path);
        let files: Vec<PathBuf> = view.selected_items().into_iter().map(|(path, _)| path).collect();
        let env = environment(file.as_deref(), &files, &dir);
        self.start_or_ask(command.to_owned(), dir, env);
    }

    /// Runs the command shown again, in its folder.
    fn run_again(&self) {
        let (command, dir, env) = {
            let state = self.0.state.borrow();
            (state.command.clone(), state.dir.clone(), state.env.clone())
        };
        if !command.is_empty() {
            self.start_or_ask(command, dir, env);
        }
    }

    /// Starts `command`; while another runs, asks first whether to stop it.
    fn start_or_ask(&self, command: String, dir: PathBuf, env: Vec<(&'static str, OsString)>) {
        if !self.is_running() {
            return self.start(command, dir, env);
        }
        let this = self.clone();
        crate::operations::with_current(|ops| {
            ops.dialogs().ask(
                t!("Run Command"),
                t!("A command is still running. Stop it?"),
                &[&t!("Stop and Run"), &t!("Cancel")],
                move |choice| {
                    if choice == Some(0) {
                        this.stop();
                        this.start(command, dir, env);
                    }
                },
            );
        });
    }

    fn start(&self, command: String, dir: PathBuf, env: Vec<(&'static str, OsString)>) {
        let shell = SETTINGS.with(|s| pick_shell(&s.borrow(), std::env::var_os("SHELL"), cfg!(windows)));
        let call = invocation(&shell, &command, std::env::var_os("ComSpec"));
        let mut process = std::process::Command::new(&call.program);
        process.args(&call.args).current_dir(&dir).envs(env.iter().map(|(k, v)| (k, v)));
        #[cfg(windows)]
        if let Some(raw) = &call.raw {
            use std::os::windows::process::CommandExt;
            process.raw_arg(raw);
        }
        let started = if shell == Shell::Cmd && is_network(&dir) {
            // cmd would run it in the Windows folder instead, saying so only in its output.
            let why = t!("cmd cannot run in a network folder; set [command-line] shell = \"powershell\"");
            Err(std::io::Error::other(why.into_owned()))
        } else {
            ChildProcess::spawn_piped(process)
        };
        {
            let mut state = self.0.state.borrow_mut();
            // What the last one left running ends now (Windows: its job object).
            state.child = None;
            *state = State { open: true, command: command.clone(), dir, env, ..State::default() };
            self.0.model.set_vec(Vec::new());
            match started {
                Ok((child, stdout, stderr)) => {
                    let shared: Shared = Arc::new(Mutex::new(Pending { lines: Vec::new(), open: 2 }));
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
                }
                Err(err) => {
                    let program = call.program.to_string_lossy();
                    // TRANSLATORS: {0} is a shell program, {1} why it did not start
                    let text = t!("Could not start {0}: {1}", program, err);
                    state.output.push(Line { text: text.clone(), err: true });
                    self.0.model.push(CmdLine { text: text.into(), err: true });
                    state.status = t!("Did not start").into_owned();
                }
            }
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

    /// Takes the lines read so far, sees whether the shell ended, and once it did (and its
    /// output closed, or half a second went by) ends the run.
    fn tick(&self) {
        let finished = {
            let mut state = self.0.state.borrow_mut();
            if !state.running {
                return;
            }
            let (lines, open) = match &state.shared {
                Some(shared) => {
                    let mut pending = lock(shared);
                    (std::mem::take(&mut pending.lines), pending.open)
                }
                None => (Vec::new(), 0),
            };
            self.take(&mut state, lines);
            if state.exited.is_none()
                && let Some(Ok(Some(status))) = state.child.as_mut().map(ChildProcess::try_wait)
            {
                state.exited = Some((Instant::now(), status.code()));
            }
            match state.exited {
                Some((at, _)) if open == 0 || at.elapsed() >= GRACE => {
                    state.running = false;
                    true
                }
                _ => false,
            }
        };
        if finished {
            self.0.timer.stop();
            self.show();
            self.refresh_folder();
        } else {
            self.show();
        }
    }

    /// Adds `lines` to the output and the panel's rows.
    fn take(&self, state: &mut State, lines: Vec<Line>) {
        let mut dropped = 0;
        for line in lines {
            let row = CmdLine { text: line.text.as_str().into(), err: line.err };
            dropped += state.output.push(line);
            self.0.model.push(row);
        }
        if dropped > 0 {
            // Rebuilt once rather than shifted row by row.
            let rows: Vec<CmdLine> =
                state.output.lines().map(|l| CmdLine { text: l.text.as_str().into(), err: l.err }).collect();
            self.0.model.set_vec(rows);
        }
    }

    /// The folder the command ran in, read once more if the pane still shows it (its watcher
    /// does too).
    fn refresh_folder(&self) {
        let dir = self.0.state.borrow().dir.clone();
        crate::panes::with_id(self.0.id, |p| {
            if p.nav.active_location().folder().is_some_and(|shown| gezik_core::ops::paths::same_path(shown, &dir)) {
                p.nav.refresh_due();
            }
        });
    }

    /// `Stop`: the shell and everything it started end now.
    pub fn stop(&self) {
        let lines = {
            let mut state = self.0.state.borrow_mut();
            if !state.running {
                return;
            }
            if let Some(child) = state.child.as_mut() {
                child.kill_tree();
            }
            state.running = false;
            state.stopped = true;
            state.exited.get_or_insert((Instant::now(), None));
            state.shared.as_ref().map(|shared| std::mem::take(&mut lock(shared).lines)).unwrap_or_default()
        };
        self.0.timer.stop();
        let mut state = self.0.state.borrow_mut();
        self.take(&mut state, lines);
        drop(state);
        self.show();
    }

    /// The pane closes (or its window): a running command ends, quietly.
    pub fn end(&self) {
        self.0.timer.stop();
        let mut state = self.0.state.borrow_mut();
        state.running = false;
        state.child = None;
    }

    /// `Close`: stops a running command and closes the panel.
    pub fn close(&self) {
        self.stop();
        {
            let mut state = self.0.state.borrow_mut();
            state.open = false;
            state.child = None;
        }
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
        let text = self.0.state.borrow().output.text();
        let note = match gezik_platform::clipboard::write_text(&text) {
            Ok(()) => t!("Output copied").into_owned(),
            Err(_) => t!("Could not copy the output").into_owned(),
        };
        crate::panes::with_id(self.0.id, |p| p.view.note(note));
    }

    /// Writes the panel's header to the pane (only what changed is written).
    fn show(&self) {
        let (open, title, folder, status, state_code, dropped) = {
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
            (
                state.open,
                format!("> {}", state.command),
                state.dir.display().to_string(),
                status,
                code,
                state.output.dropped,
            )
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
        });
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
        assert_eq!(call.args, ["/d", "/s", "/c"]);
        assert_eq!(call.raw.as_deref(), Some(format!("\"{typed}\"").as_str()));
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
        let call = invocation(&Shell::Pwsh("pwsh".into()), "Write-Output 'ğ \"x\" %y%'", None);
        assert_eq!(call.program, "pwsh");
        assert_eq!(&call.args[..4], ["-NoLogo", "-NoProfile", "-NonInteractive", "-EncodedCommand"]);
        let script = format!("{POWERSHELL_UTF8}Write-Output 'ğ \"x\" %y%'");
        let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert_eq!(call.args[4], OsString::from(base64(&bytes)));
        assert!(call.args[4].to_str().unwrap().bytes().all(|b| b.is_ascii_alphanumeric() || b"+/=".contains(&b)));
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

    #[test]
    fn the_environment_names_the_selection() {
        let dir = Path::new("/w");
        let files = [PathBuf::from("/w/a b.txt"), PathBuf::from("/w/ç.rs")];
        let env = environment(Some(&files[0]), &files, dir);
        let get = |key: &str| env.iter().find(|(k, _)| *k == key).map(|(_, v)| v.clone()).unwrap();
        assert_eq!(get("GEZIK_FILE"), "/w/a b.txt");
        assert_eq!(get("GEZIK_FILES"), "/w/a b.txt\n/w/ç.rs");
        assert_eq!(get("GEZIK_DIR"), "/w");
        assert_eq!(get("TERM"), "dumb");
        assert_eq!(get("GIT_TERMINAL_PROMPT"), "0");
        let env = environment(None, &[], dir);
        assert_eq!(env.iter().find(|(k, _)| *k == "GEZIK_FILE").unwrap().1, "");
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
    fn output_keeps_two_megabytes() {
        let mut out = Output::default();
        let line = "x".repeat(1023);
        let mut dropped = 0;
        for _ in 0..2048 {
            dropped += out.push(Line { text: line.clone(), err: false });
        }
        assert_eq!((dropped, out.dropped), (0, false), "exactly 2 MB fits");
        dropped += out.push(Line { text: "last".into(), err: true });
        assert!(out.dropped && dropped > 0);
        assert!(out.bytes <= KEPT_AFTER_DROP);
        assert_eq!(out.lines().count(), 2049 - dropped);
        assert_eq!(out.lines().last().map(|l| (l.text.as_str(), l.err)), Some(("last", true)));
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
        (std::mem::take(&mut lock(&shared).lines), code)
    }

    fn spawn(shell: &Shell, command: &str, dir: &Path, env: &[(&'static str, OsString)]) -> (ChildProcess, Shared) {
        let call = invocation(shell, command, std::env::var_os("ComSpec"));
        let mut process = std::process::Command::new(&call.program);
        process.args(&call.args).current_dir(dir).envs(env.iter().map(|(k, v)| (k, v)));
        #[cfg(windows)]
        if let Some(raw) = &call.raw {
            use std::os::windows::process::CommandExt;
            process.raw_arg(raw);
        }
        let (child, stdout, stderr) = ChildProcess::spawn_piped(process).unwrap();
        let shared: Shared = Arc::new(Mutex::new(Pending { lines: Vec::new(), open: 2 }));
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
        let env = environment(Some(&dir.join("a.txt")), &[dir.join("a.txt")], &dir);
        let command = if cfg!(windows) {
            r#"type "ğ ü.txt" & echo. & echo "%GEZIK_FILE%" & echo a "b" 100%& echo err 1>&2 & exit /b 3"#
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

    #[cfg(windows)]
    #[test]
    fn cmd_output_in_the_oem_code_page_reads_right() {
        let dir = temp("oem");
        let (lines, _) = run(&Shell::Cmd, "echo über", &dir, &[]);
        // Where the OEM code page has ü (437, 850, 857 and UTF-8 all do).
        if gezik_platform::decode_oem(b"\x81").as_deref() == Some("ü") {
            assert_eq!(texts(&lines, false), ["über"]);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn powershell_runs_the_encoded_command() {
        let dir = temp("ps");
        let (lines, code) =
            run(&Shell::Pwsh("powershell".into()), r#"Write-Output 'a "b" ğüş %x%'; exit 2"#, &dir, &[]);
        assert_eq!((texts(&lines, false), code), (vec![r#"a "b" ğüş %x%"#], Some(2)));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
