//! "Open terminal" (spec 3): which terminal opens a folder on this system, and opening it so
//! that it lives on after Gezik. The choice is pure (`choose`, with the environment and PATH
//! given in a `Lookup`), so every rule is tested on every system; `open` asks the real ones.
//! Windows' and Linux's rules were tried in docs/superpowers/notes/2026-10-08-gunluk-kolayliklar-probe.md.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Windows,
    Mac,
    Linux,
}

impl Os {
    pub fn current() -> Os {
        if cfg!(windows) {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::Mac
        } else {
            Os::Linux
        }
    }
}

/// What to run: a program (a name on PATH or a path), its arguments, its working folder, and
/// whether as administrator (Windows).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub program: OsString,
    pub args: Vec<OsString>,
    pub dir: PathBuf,
    pub elevated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalError {
    NotFound,
    /// "as administrator" elsewhere than Windows.
    OnlyOnWindows,
    /// The UAC question was answered No.
    Cancelled,
    /// A `[terminal] command` that runs through cmd (the word named: cmd, or a `.bat`/`.cmd`
    /// file), for a folder whose name cmd would read as more than a name (`cmd_safe`).
    UnsafeFolder(String),
    Failed(String),
}

impl std::fmt::Display for TerminalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TerminalError::NotFound => write!(f, "No terminal was found"),
            TerminalError::OnlyOnWindows => write!(f, "Opening a terminal as administrator is only on Windows"),
            TerminalError::Cancelled => write!(f, "Cancelled"),
            TerminalError::UnsafeFolder(program) => {
                write!(f, "This folder's name can't be passed safely to {program}")
            }
            TerminalError::Failed(why) => write!(f, "{why}"),
        }
    }
}

/// The word of a command that runs through cmd.exe, if any: cmd itself, or a batch file
/// (cmd runs those too). By file name, case aside.
fn through_cmd<'w>(words: impl IntoIterator<Item = &'w str>) -> Option<String> {
    words.into_iter().find_map(|word| {
        let name = file_name(word);
        let lower = name.to_ascii_lowercase();
        (lower == "cmd" || lower == "cmd.exe" || lower.ends_with(".bat") || lower.ends_with(".cmd"))
            .then(|| name.to_owned())
    })
}

/// A command word's file name, `\` or `/` separated.
fn file_name(word: &str) -> &str {
    word.rsplit(['\\', '/']).next().unwrap_or(word)
}

/// Whether cmd.exe takes `dir` for a name only. cmd's `/c` and `/k` strip quotes, so no
/// quoting makes `&|<>^()` plain, `%VAR%` and (delayed expansion) `!VAR!` are expanded, and
/// `"` ends a quote: such a folder could run a command of its name (elevated, as
/// administrator). Refused rather than escaped (BatBadBut).
fn cmd_safe(dir: &Path) -> bool {
    !dir.to_string_lossy().chars().any(|c| c.is_control() || "%&|<>^\"()!".contains(c))
}

/// The environment `choose` decides by.
pub struct Lookup<'a> {
    pub os: Os,
    /// `[terminal] command`. Its program must be `found`: a name on PATH (on Windows with or
    /// without its extension, PATHEXT tried) or a path. A name only the App Paths key knows
    /// needs its full path.
    pub command: Option<&'a [String]>,
    /// `$TERMINAL` (Linux): a program and its arguments, split at spaces, not parsed as a shell
    /// would (no quotes). A known terminal named alone gets its folder flags.
    pub terminal_var: Option<&'a str>,
    /// `XDG_CURRENT_DESKTOP` (Linux), `:`-separated.
    pub desktop: Option<&'a str>,
    /// Whether a program runs: a name on PATH (PATHEXT tried on Windows for a name without an
    /// extension) or a path.
    pub found: &'a dyn Fn(&str) -> bool,
    /// The file name of the program a link on PATH stands for (`x-terminal-emulator`).
    pub real_name: &'a dyn Fn(&str) -> Option<String>,
}

/// The terminals Gezik knows on Linux, in the order it tries them, with the flags that give
/// each its folder (the working folder alone does not hold in all: gnome-terminal's server
/// has its own). Checked with each one in probe 4.3.
pub const KNOWN: [(&str, &[&str]); 9] = [
    ("gnome-terminal", &["--working-directory={dir}"]),
    // Without --new-window, Ptyxis leaves --working-directory out.
    ("ptyxis", &["--new-window", "--working-directory", "{dir}"]),
    ("kgx", &["--working-directory", "{dir}"]),
    ("konsole", &["--workdir", "{dir}"]),
    ("xfce4-terminal", &["--working-directory={dir}"]),
    ("kitty", &["--directory", "{dir}"]),
    ("alacritty", &["--working-directory", "{dir}"]),
    ("foot", &["--working-directory={dir}"]),
    ("xterm", &[]),
];

/// The known terminals, the desktop's own first.
fn linux_order(desktop: Option<&str>) -> Vec<&'static str> {
    let desktops: Vec<String> = desktop.unwrap_or("").split(':').map(str::to_ascii_lowercase).collect();
    let has = |name: &str| desktops.iter().any(|d| d == name);
    let mut order: Vec<&'static str> = Vec::new();
    if has("kde") {
        order.push("konsole");
    }
    if has("xfce") {
        order.push("xfce4-terminal");
    }
    if has("gnome") {
        order.extend(["ptyxis", "kgx", "gnome-terminal"]);
    }
    for (name, _) in KNOWN {
        if !order.contains(&name) {
            order.push(name);
        }
    }
    order
}

/// The flags that give known terminal `name` its folder (a `.wrapper` ending is Debian's).
fn flags_of(name: &str, dir: &Path) -> Vec<OsString> {
    let name = name.strip_suffix(".wrapper").unwrap_or(name);
    let flags = KNOWN.iter().find(|(known, _)| *known == name).map_or(&[][..], |(_, flags)| *flags);
    flags.iter().map(|flag| with_dir(flag, dir)).collect()
}

fn with_dir(flag: &str, dir: &Path) -> OsString {
    match flag.split_once("{dir}") {
        Some((before, after)) => {
            let mut out = OsString::from(before);
            out.push(dir);
            out.push(after);
            out
        }
        None => flag.into(),
    }
}

/// `dir` for `wt -d`: Windows Terminal takes `;` for its command separator even inside an
/// argument, `\;` for itself (probe 4.1).
pub fn wt_dir(dir: &str) -> String {
    dir.replace(';', r"\;")
}

/// The characters PowerShell's tokenizer takes for a single quote: `'` and the typographic
/// ones (U+2018-U+201B). Checked against every BMP character in Windows PowerShell 5.1's
/// parser; PowerShell 7's `IsSingleQuote` has the same set.
const POWERSHELL_SINGLE_QUOTES: [char; 5] = ['\'', '\u{2018}', '\u{2019}', '\u{201A}', '\u{201B}'];

/// `text` as a PowerShell single-quoted string: each quote doubled (by itself), so none ends
/// the string.
pub fn powershell_literal(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('\'');
    for c in text.chars() {
        if POWERSHELL_SINGLE_QUOTES.contains(&c) {
            out.push(c);
        }
        out.push(c);
    }
    out.push('\'');
    out
}

/// `args` as one Windows command line, the way `CommandLineToArgvW` splits it back. An
/// argument with `&|<>^()` is quoted too. That does not make it safe for cmd.exe, whose `/c`
/// and `/k` take quotes away: `choose` refuses such folders for a command through cmd.
pub fn windows_command_line(args: &[String]) -> String {
    let mut out = String::new();
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        if !arg.is_empty() && !arg.contains([' ', '\t', '"', '&', '|', '<', '>', '^', '(', ')']) {
            out.push_str(arg);
            continue;
        }
        out.push('"');
        let mut backslashes = 0;
        for c in arg.chars() {
            match c {
                '\\' => backslashes += 1,
                '"' => {
                    out.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
                    out.push('"');
                    backslashes = 0;
                }
                _ => {
                    out.extend(std::iter::repeat_n('\\', backslashes));
                    out.push(c);
                    backslashes = 0;
                }
            }
        }
        out.extend(std::iter::repeat_n('\\', backslashes * 2));
        out.push('"');
    }
    out
}

/// The terminal for `dir` (spec 3.2). `admin` is only for Windows.
pub fn choose(dir: &Path, admin: bool, lookup: &Lookup<'_>) -> Result<Launch, TerminalError> {
    if admin && lookup.os != Os::Windows {
        return Err(TerminalError::OnlyOnWindows);
    }
    let launch = |program: &str, args: Vec<OsString>| Launch {
        program: program.into(),
        args,
        dir: dir.to_path_buf(),
        elevated: admin,
    };
    if let Some(command) = lookup.command {
        // Through Windows Terminal (`wt -p Ubuntu -d {dir}`) a `;` would start a command of its own.
        let wt = lookup.os == Os::Windows
            && command.iter().any(|word| ["wt", "wt.exe"].contains(&&*file_name(word).to_ascii_lowercase()));
        let escaped = wt.then(|| PathBuf::from(wt_dir(&dir.to_string_lossy())));
        let mut args = gezik_core::batch::convert::expand_dir_command(command, escaped.as_deref().unwrap_or(dir))
            .map_err(TerminalError::Failed)?;
        let program = args.remove(0);
        // Never handed to the Shell unfound: it would show its own "cannot find" dialog.
        if !(lookup.found)(&program.to_string_lossy()) {
            return Err(TerminalError::NotFound);
        }
        // `wt cmd /k …` runs through cmd as much as `cmd /k …` does: every word counts.
        if lookup.os == Os::Windows
            && !cmd_safe(dir)
            && let Some(cmd) =
                through_cmd(std::iter::once(&*program.to_string_lossy()).chain(command[1..].iter().map(String::as_str)))
        {
            return Err(TerminalError::UnsafeFolder(cmd));
        }
        return Ok(Launch { program, args, dir: dir.to_path_buf(), elevated: admin });
    }
    let found = lookup.found;
    match lookup.os {
        Os::Windows => {
            let text = dir.to_string_lossy();
            if found("wt") {
                return Ok(launch("wt.exe", vec!["-d".into(), wt_dir(&text).into()]));
            }
            let shell = if found("pwsh") {
                "pwsh.exe"
            } else if found("powershell") {
                "powershell.exe"
            } else {
                return Err(TerminalError::NotFound);
            };
            // Given as an argument: PowerShell 5.1 does not start in a folder with `[`, and an
            // elevated one does not always get its working folder (probe 4.1).
            let go = format!("Set-Location -LiteralPath {}", powershell_literal(&text));
            Ok(launch(shell, vec!["-NoExit".into(), "-Command".into(), go.into()]))
        }
        Os::Mac => Ok(launch("/usr/bin/open", vec!["-a".into(), "Terminal".into(), dir.as_os_str().to_owned()])),
        Os::Linux => {
            if let Some(var) = lookup.terminal_var {
                let words: Vec<&str> = var.split_whitespace().collect();
                if let Some((program, rest)) = words.split_first().filter(|(program, _)| found(program)) {
                    let args = if rest.is_empty() {
                        let name = Path::new(program).file_name().and_then(OsStr::to_str).unwrap_or(program);
                        flags_of(name, dir)
                    } else {
                        rest.iter().map(OsString::from).collect()
                    };
                    return Ok(launch(program, args));
                }
            }
            if found("x-terminal-emulator") {
                let flags =
                    (lookup.real_name)("x-terminal-emulator").map(|real| flags_of(&real, dir)).unwrap_or_default();
                return Ok(launch("x-terminal-emulator", flags));
            }
            linux_order(lookup.desktop)
                .into_iter()
                .find(|name| found(name))
                .map(|name| launch(name, flags_of(name, dir)))
                .ok_or(TerminalError::NotFound)
        }
    }
}

/// Whether `name` runs (see `find_program`).
fn on_path(name: &str) -> bool {
    let pathext = std::env::var("PATHEXT").ok();
    find_program(name, std::env::var_os("PATH").as_deref(), pathext.as_deref(), cfg!(windows)).is_some()
}

/// The program `name` stands for: a path as it is, else the first match in a folder of `path`
/// (absolute ones only). On Windows (`windows`) a name with an extension is looked for as it
/// is, one without with each of `pathext`'s (`.COM;.EXE;.BAT;.CMD` if unset). The Store's app
/// aliases count: Rust 1.99 sees them as files (probe 4.1).
fn find_program(name: &str, path: Option<&OsStr>, pathext: Option<&str>, windows: bool) -> Option<PathBuf> {
    if name.is_empty() {
        return None;
    }
    let program = Path::new(name);
    if program.components().count() > 1 || program.is_absolute() {
        return runnable(program).then(|| program.to_path_buf());
    }
    let files: Vec<String> = if windows && program.extension().is_none() {
        pathext
            .unwrap_or(".COM;.EXE;.BAT;.CMD")
            .split(';')
            .filter(|ext| ext.len() > 1 && ext.starts_with('.'))
            .map(|ext| format!("{name}{ext}"))
            .collect()
    } else {
        vec![name.to_owned()]
    };
    std::env::split_paths(path?)
        .filter(|dir| dir.is_absolute())
        .find_map(|dir| files.iter().map(|file| dir.join(file)).find(|candidate| runnable(candidate)))
}

fn runnable(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else { return false };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.is_file() && meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    meta.is_file()
}

/// The file name of the program `name` on PATH stands for, links followed.
fn real_name(name: &str) -> Option<String> {
    // Only asked on Linux (`x-terminal-emulator`): no PATHEXT.
    find_program(name, std::env::var_os("PATH").as_deref(), None, false)
        .and_then(|path| std::fs::canonicalize(path).ok())
        .and_then(|path| path.file_name().map(|n| n.to_string_lossy().into_owned()))
}

/// Opens a terminal in `dir` (`admin`: as administrator, Windows only). Looks through PATH, so
/// not on the UI thread. The terminal does not end with Gezik.
pub fn open(dir: &Path, admin: bool, command: Option<&[String]>) -> Result<(), TerminalError> {
    match std::fs::metadata(dir) {
        Ok(meta) if meta.is_dir() => {}
        Ok(_) => return Err(TerminalError::Failed("It is not a folder".to_owned())),
        Err(err) => return Err(TerminalError::Failed(crate::fs::describe(&err))),
    }
    let os = Os::current();
    // An elevated session does not see the drives this user mapped: their share instead.
    let unc = (admin && os == Os::Windows)
        .then(|| gezik_core::path_text::unc_path(&dir.to_string_lossy(), &crate::fs::mapped_remote))
        .flatten()
        .map(PathBuf::from);
    let dir = unc.as_deref().unwrap_or(dir);
    let terminal_var = std::env::var("TERMINAL").ok();
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").ok();
    let lookup = Lookup {
        os,
        command,
        terminal_var: terminal_var.as_deref(),
        desktop: desktop.as_deref(),
        found: &on_path,
        real_name: &real_name,
    };
    let pathext = std::env::var("PATHEXT").ok();
    let path = std::env::var_os("PATH");
    start(&resolve(choose(dir, admin, &lookup)?, path.as_deref(), pathext.as_deref(), cfg!(windows))?)
}

/// `launch` with its program by its full path, found as `choose` checked it (absolute PATH
/// folders only), or `NotFound`. Never a bare name: the Shell looks in the folder being opened
/// (`lpDirectory`) before PATH, and on Unix a relative PATH entry would be taken from it too,
/// so a `wt.exe` or `gnome-terminal` planted there would run (elevated, as administrator).
fn resolve(
    mut launch: Launch,
    path: Option<&OsStr>,
    pathext: Option<&str>,
    windows: bool,
) -> Result<Launch, TerminalError> {
    let name = launch.program.to_string_lossy();
    let full = find_program(&name, path, pathext, windows).ok_or(TerminalError::NotFound)?;
    if !full.is_absolute() {
        return Err(TerminalError::NotFound);
    }
    // `term` may turn out to be `term.cmd`: through cmd as much as a name typed so.
    if windows
        && !cmd_safe(&launch.dir)
        && let Some(cmd) = through_cmd([&*full.to_string_lossy()])
    {
        return Err(TerminalError::UnsafeFolder(cmd));
    }
    launch.program = full.into_os_string();
    Ok(launch)
}

/// Through the Shell, as Explorer starts programs: a console program gets a console of its
/// own, nothing of Gezik's is handed on. "runas" asks UAC (probe 4.1: `ShellExecuteW`,
/// `ShellExecuteExW` would need the Registry feature).
#[cfg(windows)]
fn start(launch: &Launch) -> Result<(), TerminalError> {
    use windows::Win32::Foundation::{ERROR_CANCELLED, GetLastError};
    use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx};
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    use windows::core::HSTRING;
    let args: Vec<String> = launch.args.iter().map(|arg| arg.to_string_lossy().into_owned()).collect();
    let verb = HSTRING::from(if launch.elevated { "runas" } else { "open" });
    let result = unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
        ShellExecuteW(
            None,
            &verb,
            &HSTRING::from(launch.program.as_os_str()),
            &HSTRING::from(windows_command_line(&args)),
            &HSTRING::from(launch.dir.as_os_str()),
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize > 32 {
        return Ok(());
    }
    let err = unsafe { GetLastError() };
    if err == ERROR_CANCELLED {
        return Err(TerminalError::Cancelled);
    }
    Err(TerminalError::Failed(crate::fs::describe(&std::io::Error::from_raw_os_error(err.0 as i32))))
}

/// Terminals whose waiting thread could not be started: reaped at the next start.
#[cfg(unix)]
static UNREAPED: std::sync::Mutex<Vec<std::process::Child>> = std::sync::Mutex::new(Vec::new());

/// A process of its own group (Ctrl+C where Gezik was started does not reach it), waited for
/// on a small thread so that one that exits at once (gnome-terminal hands its window to its
/// server) leaves no zombie.
#[cfg(unix)]
fn start(launch: &Launch) -> Result<(), TerminalError> {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    use std::sync::{Arc, Mutex, PoisonError};
    UNREAPED.lock().unwrap_or_else(PoisonError::into_inner).retain_mut(|child| matches!(child.try_wait(), Ok(None)));
    let child = Command::new(&launch.program)
        .args(&launch.args)
        .current_dir(&launch.dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|err| TerminalError::Failed(crate::fs::describe(&err)))?;
    // Shared with the thread, so that the child is still here if the thread cannot start.
    let waiting = Arc::new(Mutex::new(Some(child)));
    let theirs = waiting.clone();
    let spawned = std::thread::Builder::new().name("gezik-terminal".into()).stack_size(64 * 1024).spawn(move || {
        let child = theirs.lock().unwrap_or_else(PoisonError::into_inner).take();
        if let Some(mut child) = child {
            let _ = child.wait();
        }
    });
    if spawned.is_err() {
        let child = waiting.lock().unwrap_or_else(PoisonError::into_inner).take();
        if let Some(mut child) = child
            && matches!(child.try_wait(), Ok(None))
        {
            UNREAPED.lock().unwrap_or_else(PoisonError::into_inner).push(child);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_link(_: &str) -> Option<String> {
        None
    }

    fn look<'a>(os: Os, found: &'a dyn Fn(&str) -> bool) -> Lookup<'a> {
        Lookup { os, command: None, terminal_var: None, desktop: None, found, real_name: &no_link }
    }

    fn args(launch: &Launch) -> Vec<String> {
        launch.args.iter().map(|a| a.to_string_lossy().into_owned()).collect()
    }

    fn program(launch: &Launch) -> String {
        launch.program.to_string_lossy().into_owned()
    }

    #[test]
    fn windows_terminal_first_then_powershell() {
        let dir = Path::new(r"C:\Users\a\x;y");
        let all = |_: &str| true;
        let wt = choose(dir, false, &look(Os::Windows, &all)).unwrap();
        assert_eq!(
            (program(&wt), args(&wt)),
            ("wt.exe".to_owned(), vec!["-d".to_owned(), r"C:\Users\a\x\;y".to_owned()])
        );
        assert_eq!((wt.dir.as_path(), wt.elevated), (dir, false));
        let no_wt = |p: &str| p != "wt";
        assert_eq!(program(&choose(dir, false, &look(Os::Windows, &no_wt)).unwrap()), "pwsh.exe");
        let only_ps = |p: &str| p == "powershell";
        assert_eq!(program(&choose(dir, false, &look(Os::Windows, &only_ps)).unwrap()), "powershell.exe");
        let none = |_: &str| false;
        assert_eq!(choose(dir, false, &look(Os::Windows, &none)), Err(TerminalError::NotFound));
    }

    #[test]
    fn powershell_always_gets_its_folder_as_a_literal_path() {
        // Windows PowerShell 5.1 does not start in a folder with `[` (probe 4.1).
        let dir = Path::new(r"C:\[köşeli] it's");
        let only_ps = |p: &str| p == "powershell";
        for admin in [false, true] {
            let ps = choose(dir, admin, &look(Os::Windows, &only_ps)).unwrap();
            assert_eq!(ps.elevated, admin);
            assert_eq!(args(&ps), ["-NoExit", "-Command", "Set-Location -LiteralPath 'C:\\[köşeli] it''s'"]);
        }
        let all = |_: &str| true;
        assert_eq!(args(&choose(dir, true, &look(Os::Windows, &all)).unwrap()), ["-d", r"C:\[köşeli] it's"]);
        assert_eq!(choose(dir, true, &look(Os::Linux, &all)), Err(TerminalError::OnlyOnWindows));
        assert_eq!(choose(dir, true, &look(Os::Mac, &all)), Err(TerminalError::OnlyOnWindows));
    }

    #[test]
    fn every_quote_powershell_reads_is_doubled() {
        // PowerShell ends a single-quoted string at ‘ ’ ‚ ‛ too: undoubled, `x’;calc;’` would
        // run calc (elevated, with "as administrator").
        let only_ps = |p: &str| p == "powershell";
        let go = |dir: &str| args(&choose(Path::new(dir), true, &look(Os::Windows, &only_ps)).unwrap())[2].clone();
        assert_eq!(go(r"C:\x’;calc;’"), "Set-Location -LiteralPath 'C:\\x’’;calc;’’'");
        assert_eq!(go(r"C:\Ali’nin"), "Set-Location -LiteralPath 'C:\\Ali’’nin'");
        assert_eq!(powershell_literal("'‘’‚‛"), "'''‘‘’’‚‚‛‛'");
        assert_eq!(powershell_literal("ʼ′＇`\"“”$(x)"), "'ʼ′＇`\"“”$(x)'", "not quotes to PowerShell");
        // Still one argument on the command line.
        let line = windows_command_line(&["-Command".to_owned(), go(r"C:\x’;calc;’")]);
        assert_eq!(line, "-Command \"Set-Location -LiteralPath 'C:\\x’’;calc;’’'\"");
    }

    /// The folder through the real chain: the command line `start` hands on, split by Windows
    /// PowerShell and run (no window; `exit 7` stands in for an injected command).
    #[cfg(windows)]
    #[test]
    fn powershell_lands_in_folders_with_quotes() {
        use std::os::windows::process::CommandExt;
        let Some(powershell) = find_program("powershell", std::env::var_os("PATH").as_deref(), None, true) else {
            return;
        };
        let root = std::env::temp_dir().join(format!("gezik-ps-{}", std::process::id()));
        let only_ps = |p: &str| p == "powershell";
        for name in ["x’;exit 7;’", "[a] it's ‘b‚c‛ & (d)"] {
            let dir = root.join(name);
            std::fs::create_dir_all(&dir).unwrap();
            let mut words = args(&choose(&dir, false, &look(Os::Windows, &only_ps)).unwrap());
            assert_eq!(words.remove(0), "-NoExit");
            words[1].push_str("; (Get-Location).ProviderPath");
            words.splice(0..0, ["-NoProfile".to_owned(), "-NonInteractive".to_owned()]);
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            let output = std::process::Command::new(&powershell)
                .raw_arg(windows_command_line(&words))
                .creation_flags(CREATE_NO_WINDOW)
                .stdin(std::process::Stdio::null())
                .output()
                .unwrap();
            assert_eq!(output.status.code(), Some(0), "{name}");
            // The console's code page may not hold every character: compare the ASCII ends.
            let printed = String::from_utf8_lossy(&output.stdout);
            assert!(printed.trim_end().starts_with(&*root.to_string_lossy()), "{name}: {printed}");
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn windows_command_lines_split_back_into_the_arguments() {
        let line = |a: &[&str]| windows_command_line(&a.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>());
        assert_eq!(line(&["-d", r"C:\"]), r"-d C:\");
        assert_eq!(line(&["-d", r"C:\a b\"]), r#"-d "C:\a b\\""#);
        assert_eq!(line(&[r#"say "hi""#, ""]), r#""say \"hi\"" """#);
        assert_eq!(line(&[r"a\\b", "tab\there"]), "a\\\\b \"tab\there\"");
        assert_eq!(
            line(&["-NoExit", "-Command", "Set-Location -LiteralPath 'C:\\it''s'"]),
            r#"-NoExit -Command "Set-Location -LiteralPath 'C:\it''s'""#
        );
        assert_eq!(line(&["a&b", "x|y", "<in>", "^", "(c)"]), r#""a&b" "x|y" "<in>" "^" "(c)""#);
    }

    #[test]
    fn macos_opens_terminal_app() {
        let none = |_: &str| false;
        let mac = choose(Path::new("/Users/a/İş"), false, &look(Os::Mac, &none)).unwrap();
        assert_eq!(
            (program(&mac), args(&mac)),
            ("/usr/bin/open".to_owned(), vec!["-a".to_owned(), "Terminal".to_owned(), "/Users/a/İş".to_owned()])
        );
    }

    #[test]
    fn linux_tries_terminal_then_the_debian_one_then_the_known_ones() {
        let dir = Path::new("/home/a/my dir");
        let all = |_: &str| true;
        let mut lookup = look(Os::Linux, &all);
        lookup.terminal_var = Some("  foot --app-id x ");
        let t = choose(dir, false, &lookup).unwrap();
        assert_eq!((program(&t), args(&t)), ("foot".to_owned(), vec!["--app-id".to_owned(), "x".to_owned()]));
        let not_missing = |p: &str| p != "missing";
        let mut lookup = look(Os::Linux, &not_missing);
        lookup.terminal_var = Some("missing");
        let x = choose(dir, false, &lookup).unwrap();
        assert_eq!(
            (program(&x), x.args.len()),
            ("x-terminal-emulator".to_owned(), 0),
            "a $TERMINAL not found is passed over"
        );
        let kitty = |p: &str| p == "kitty" || p == "xterm";
        let k = choose(dir, false, &look(Os::Linux, &kitty)).unwrap();
        assert_eq!(
            (program(&k), args(&k)),
            ("kitty".to_owned(), vec!["--directory".to_owned(), "/home/a/my dir".to_owned()])
        );
        let xterm = |p: &str| p == "xterm";
        let x = choose(dir, false, &look(Os::Linux, &xterm)).unwrap();
        assert_eq!((program(&x), x.args.len(), x.dir.as_path()), ("xterm".to_owned(), 0, dir));
        let none = |_: &str| false;
        assert_eq!(choose(dir, false, &look(Os::Linux, &none)), Err(TerminalError::NotFound));
    }

    #[test]
    fn a_known_terminal_alone_in_terminal_gets_its_flags() {
        let dir = Path::new("/d");
        let all = |_: &str| true;
        let pick = |var: &str| {
            let mut lookup = look(Os::Linux, &all);
            lookup.terminal_var = Some(var);
            let launch = choose(dir, false, &lookup).unwrap();
            (program(&launch), args(&launch))
        };
        assert_eq!(pick("kitty"), ("kitty".to_owned(), vec!["--directory".to_owned(), "/d".to_owned()]));
        assert_eq!(pick(" /usr/bin/foot "), ("/usr/bin/foot".to_owned(), vec!["--working-directory=/d".to_owned()]));
        assert_eq!(pick("st"), ("st".to_owned(), vec![]), "an unknown one gets the working folder only");
        assert_eq!(pick("kitty --single-instance"), ("kitty".to_owned(), vec!["--single-instance".to_owned()]));
    }

    #[test]
    fn the_debian_link_gets_the_flags_of_the_terminal_it_is() {
        let dir = Path::new("/d");
        let debian = |p: &str| p == "x-terminal-emulator";
        let gnome = |_: &str| Some("gnome-terminal.wrapper".to_owned());
        let mut lookup = look(Os::Linux, &debian);
        lookup.real_name = &gnome;
        let g = choose(dir, false, &lookup).unwrap();
        assert_eq!(
            (program(&g), args(&g)),
            ("x-terminal-emulator".to_owned(), vec!["--working-directory=/d".to_owned()])
        );
        let unknown = |_: &str| Some("st".to_owned());
        lookup.real_name = &unknown;
        assert!(choose(dir, false, &lookup).unwrap().args.is_empty(), "an unknown one gets the working folder only");
    }

    #[test]
    fn the_desktops_own_terminal_comes_first() {
        let dir = Path::new("/d");
        let known = |p: &str| p != "x-terminal-emulator";
        let pick = |desktop: Option<&str>| {
            let mut lookup = look(Os::Linux, &known);
            lookup.desktop = desktop;
            let launch = choose(dir, false, &lookup).unwrap();
            (program(&launch), args(&launch))
        };
        assert_eq!(pick(Some("KDE")), ("konsole".to_owned(), vec!["--workdir".to_owned(), "/d".to_owned()]));
        assert_eq!(pick(Some("XFCE")).0, "xfce4-terminal");
        assert_eq!(
            pick(Some("ubuntu:GNOME")),
            ("ptyxis".to_owned(), vec!["--new-window".to_owned(), "--working-directory".to_owned(), "/d".to_owned()])
        );
        assert_eq!(pick(None), ("gnome-terminal".to_owned(), vec!["--working-directory=/d".to_owned()]));
        assert_eq!(linux_order(Some("GNOME")).len(), KNOWN.len(), "each once");
    }

    #[test]
    fn a_command_in_settings_wins_everywhere() {
        let none = |_: &str| false;
        let wezterm = |p: &str| p == "wezterm";
        let command = vec!["wezterm".to_owned(), "start".to_owned(), "--cwd".to_owned(), "{dir}".to_owned()];
        for os in [Os::Windows, Os::Mac, Os::Linux] {
            let mut lookup = look(os, &none);
            lookup.command = Some(&command);
            assert_eq!(choose(Path::new("/w"), false, &lookup), Err(TerminalError::NotFound), "not found: not run");
            let mut lookup = look(os, &wezterm);
            lookup.command = Some(&command);
            let launch = choose(Path::new("/w {x}"), false, &lookup).unwrap();
            assert_eq!(
                (program(&launch), args(&launch)),
                ("wezterm".to_owned(), vec!["start".to_owned(), "--cwd".to_owned(), "/w {x}".to_owned()])
            );
        }
        let bad = vec!["x".to_owned(), "{in}".to_owned()];
        let mut lookup = look(Os::Linux, &none);
        lookup.command = Some(&bad);
        assert_eq!(
            choose(Path::new("/w"), false, &lookup),
            Err(TerminalError::Failed("unknown placeholder {in}".to_owned()))
        );
    }

    #[test]
    fn a_folder_cmd_would_reinterpret_is_refused_for_a_command_through_cmd() {
        let all = |_: &str| true;
        let pick = |words: &[&str], dir: &str| {
            let command: Vec<String> = words.iter().map(|w| (*w).to_owned()).collect();
            let mut lookup = look(Os::Windows, &all);
            lookup.command = Some(&command);
            choose(Path::new(dir), true, &lookup)
        };
        let unsafe_folder = |program: &str| Err(TerminalError::UnsafeFolder(program.to_owned()));
        for dir in [r"C:\a&calc", r"C:\x%PATH%", r"C:\a^b", r#"C:\"q"#, r"C:\(x)", r"C:\a!b!", "C:\\tab\there"] {
            assert_eq!(pick(&["cmd", "/k", "cd", "/d", "{dir}"], dir), unsafe_folder("cmd"), "{dir}");
            assert_eq!(pick(&["term.cmd", "{dir}"], dir), unsafe_folder("term.cmd"), "{dir}");
            assert_eq!(pick(&[r"C:\Tools\Term.BAT", "{dir}"], dir), unsafe_folder("Term.BAT"), "{dir}");
            assert_eq!(pick(&[r"C:\Windows\System32\CMD.EXE", "/k"], dir), unsafe_folder("CMD.EXE"), "{dir}");
            assert_eq!(pick(&["wt", "cmd", "/k", "cd", "{dir}"], dir), unsafe_folder("cmd"), "{dir}");
        }
        assert_eq!(
            TerminalError::UnsafeFolder("term.cmd".to_owned()).to_string(),
            "This folder's name can't be passed safely to term.cmd"
        );
        // A plain name (spaces, quotes, brackets, Turkish letters) passes.
        let plain = r"C:\Users\a\İş [2] it's; ok";
        let ok = pick(&["cmd", "/k", "cd", "/d", "{dir}"], plain).unwrap();
        assert_eq!((program(&ok), args(&ok)[3].as_str()), ("cmd".to_owned(), plain));
        assert!(pick(&["term.cmd", "{dir}"], plain).is_ok());
        // Not through cmd: the folder is one literal argument, whatever its name.
        let weird = r"C:\a&calc %PATH%";
        let wez = pick(&["wezterm", "start", "--cwd", "{dir}"], weird).unwrap();
        assert_eq!(args(&wez), ["start", "--cwd", weird]);
        // Windows Terminal takes `;` for its own: escaped as the built-in one does.
        let wt = pick(&["wt", "-p", "Ubuntu", "-d", "{dir}"], r"C:\x; calc").unwrap();
        assert_eq!((args(&wt)[3].as_str(), wt.dir.as_path()), (r"C:\x\; calc", Path::new(r"C:\x; calc")));
        let full = pick(&[r"C:\Apps\WT.EXE", "-d", "{dir}"], r"C:\x;y").unwrap();
        assert_eq!(args(&full)[1], r"C:\x\;y");
        // Linux and macOS have no cmd: a `.cmd` there is just a name.
        let command = vec!["term.cmd".to_owned(), "{dir}".to_owned()];
        let mut lookup = look(Os::Linux, &all);
        lookup.command = Some(&command);
        assert!(choose(Path::new("/a&calc"), false, &lookup).is_ok());
        // The built-in ones take it as one literal argument.
        let wt = choose(Path::new(r"C:\a&calc"), true, &look(Os::Windows, &all)).unwrap();
        assert_eq!((program(&wt), args(&wt)), ("wt.exe".to_owned(), vec!["-d".to_owned(), r"C:\a&calc".to_owned()]));
        assert_eq!(windows_command_line(&args(&wt)), r#"-d "C:\a&calc""#);
        let only_ps = |p: &str| p == "powershell";
        let ps = choose(Path::new(r"C:\a&calc"), true, &look(Os::Windows, &only_ps)).unwrap();
        assert_eq!(args(&ps)[2], r"Set-Location -LiteralPath 'C:\a&calc'");
    }

    #[test]
    fn programs_are_found_with_their_extensions() {
        let bin = std::env::temp_dir().join(format!("gezik-bin-{}", std::process::id()));
        std::fs::create_dir_all(&bin).unwrap();
        for file in ["term.cmd", "shell.exe", "notes.txt"] {
            std::fs::write(bin.join(file), "").unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(bin.join(file), std::fs::Permissions::from_mode(0o755)).unwrap();
            }
        }
        let path = std::env::join_paths([Path::new("relative"), &bin]).unwrap();
        let find = |name: &str, pathext: Option<&str>, windows: bool| find_program(name, Some(&path), pathext, windows);
        assert_eq!(find("term", Some(".exe;.cmd"), true), Some(bin.join("term.cmd")), "PATHEXT tried");
        #[cfg(windows)]
        assert!(find("term", None, true).is_some(), "PATHEXT's default");
        assert_eq!(find("term", Some(".exe"), true), None);
        assert_eq!(find("term.cmd", Some(".exe"), true), Some(bin.join("term.cmd")), "an extension: as it is");
        assert_eq!(find("shell.exe", Some(".exe"), true), Some(bin.join("shell.exe")), "no .exe added again");
        assert_eq!(find("notes.txt.exe", Some(".exe"), true), None);
        assert_eq!(find("term", Some(".cmd"), false), None, "elsewhere the name as it is");
        assert_eq!(find("term.cmd", None, false), Some(bin.join("term.cmd")));
        let full = bin.join("shell.exe");
        assert_eq!(find(&full.to_string_lossy(), None, true), Some(full.clone()), "a path as it is");
        assert_eq!(find(&bin.join("gone.exe").to_string_lossy(), None, true), None);
        assert_eq!(find("", None, true), None);
        assert_eq!(find_program("term", None, None, true), None, "no PATH");
        let _ = std::fs::remove_dir_all(&bin);
    }

    /// Empty stand-ins for programs (executable on Unix), in a new folder under `root`.
    fn plant(root: &Path, folder: &str, files: &[&str]) -> PathBuf {
        let dir = root.join(folder);
        std::fs::create_dir_all(&dir).unwrap();
        for file in files {
            std::fs::write(dir.join(file), "").unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(dir.join(file), std::fs::Permissions::from_mode(0o755)).unwrap();
            }
        }
        dir
    }

    /// Lower case, so that the Windows rules find `wt.exe` on a case-sensitive file system too.
    const PATHEXT: Option<&str> = Some(".com;.exe;.bat;.cmd");

    /// `choose` and `resolve` as `open` runs them, with `path` for PATH.
    fn resolved(os: Os, dir: &Path, admin: bool, path: &OsStr, windows: bool) -> Result<Launch, TerminalError> {
        let found = |name: &str| find_program(name, Some(path), PATHEXT, windows).is_some();
        resolve(choose(dir, admin, &look(os, &found))?, Some(path), PATHEXT, windows)
    }

    #[test]
    fn a_terminal_planted_in_the_folder_is_never_the_one_started() {
        let root = std::env::temp_dir().join(format!("gezik-plant-{}", std::process::id()));
        let target = plant(&root, "downloads", &["wt.exe", "pwsh.exe", "powershell.exe", "gnome-terminal", "xterm"]);
        let bin = plant(&root, "bin", &["wt.exe", "pwsh.exe", "gnome-terminal"]);
        let elsewhere = std::env::join_paths([&bin]).unwrap();
        for admin in [false, true] {
            let wt = resolved(Os::Windows, &target, admin, &elsewhere, true).unwrap();
            assert_eq!((wt.program.as_os_str(), wt.dir.as_path()), (bin.join("wt.exe").as_os_str(), target.as_path()));
            assert!(!Path::new(&wt.program).starts_with(&target));
        }
        let ps = plant(&root, "ps", &["pwsh.exe"]);
        let no_wt = std::env::join_paths([&ps]).unwrap();
        let pwsh = resolved(Os::Windows, &target, true, &no_wt, true).unwrap();
        assert_eq!(pwsh.program, ps.join("pwsh.exe").into_os_string());
        let linux = resolved(Os::Linux, &target, false, &elsewhere, false).unwrap();
        assert_eq!(linux.program, bin.join("gnome-terminal").into_os_string());
        // Only in the folder, not on PATH: not found, as with no terminal at all.
        let empty = std::env::join_paths([plant(&root, "empty", &[])]).unwrap();
        assert_eq!(resolved(Os::Windows, &target, true, &empty, true), Err(TerminalError::NotFound));
        assert_eq!(resolved(Os::Linux, &target, false, &empty, false), Err(TerminalError::NotFound));
        // A bare name from anywhere (a custom command's) is resolved too, and a relative path refused.
        let bare = Launch { program: "wt.exe".into(), args: vec![], dir: target.clone(), elevated: false };
        assert_eq!(resolve(bare.clone(), Some(&empty), PATHEXT, true), Err(TerminalError::NotFound));
        assert_eq!(
            resolve(bare, Some(&elsewhere), PATHEXT, true).unwrap().program,
            bin.join("wt.exe").into_os_string()
        );
        let relative = Launch { program: "downloads/wt.exe".into(), args: vec![], dir: root.clone(), elevated: false };
        assert_eq!(resolve(relative, Some(&elsewhere), PATHEXT, true), Err(TerminalError::NotFound));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_name_found_as_a_batch_file_is_checked_as_one() {
        let root = std::env::temp_dir().join(format!("gezik-batname-{}", std::process::id()));
        let bin = plant(&root, "bin", &["term.cmd"]);
        let path = std::env::join_paths([&bin]).unwrap();
        let launch = |dir: &str| Launch { program: "term".into(), args: vec![], dir: dir.into(), elevated: false };
        assert_eq!(
            resolve(launch(r"C:\a&calc"), Some(&path), PATHEXT, true),
            Err(TerminalError::UnsafeFolder("term.cmd".to_owned()))
        );
        assert_eq!(resolve(launch(r"C:\plain"), Some(&path), PATHEXT, true).unwrap().program, bin.join("term.cmd"));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Removes a folder made in the working folder, even when the test fails.
    struct Gone(PathBuf);
    impl Drop for Gone {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn relative_path_entries_are_never_searched() {
        // `.`, an empty entry or a relative folder would be taken from the folder being opened.
        let name = format!("gezik-rel-{}", std::process::id());
        let here = std::env::current_dir().unwrap();
        let _gone = Gone(here.join(&name));
        let target = plant(&here, &name, &["wt.exe", "gnome-terminal"]);
        assert!(Path::new(&name).join("wt.exe").is_file(), "reachable through the relative entry");
        let path = std::env::join_paths([Path::new(&name), Path::new("."), Path::new("")]).unwrap();
        assert_eq!(find_program("wt", Some(&path), PATHEXT, true), None);
        assert_eq!(find_program("gnome-terminal", Some(&path), PATHEXT, false), None);
        assert_eq!(resolved(Os::Windows, &target, false, &path, true), Err(TerminalError::NotFound));
        assert_eq!(resolved(Os::Linux, &target, false, &path, false), Err(TerminalError::NotFound));
        let absolute = std::env::join_paths([&target]).unwrap();
        assert_eq!(find_program("wt", Some(&absolute), PATHEXT, true), Some(target.join("wt.exe")));
    }

    #[test]
    fn nothing_is_started_for_a_missing_folder_or_program() {
        let gone = std::env::temp_dir().join(format!("gezik-gone-{}", std::process::id()));
        assert!(matches!(open(&gone, false, None), Err(TerminalError::Failed(_))));
        let file = std::env::temp_dir().join(format!("gezik-file-{}", std::process::id()));
        std::fs::write(&file, "").unwrap();
        assert_eq!(open(&file, false, None), Err(TerminalError::Failed("It is not a folder".to_owned())));
        let _ = std::fs::remove_file(&file);
        let missing = vec!["gezik-no-such-terminal".to_owned(), "{dir}".to_owned()];
        assert_eq!(open(&std::env::temp_dir(), false, Some(&missing)), Err(TerminalError::NotFound));
        let missing_path = vec![gone.join("term.exe").to_string_lossy().into_owned()];
        assert_eq!(open(&std::env::temp_dir(), false, Some(&missing_path)), Err(TerminalError::NotFound));
    }

    #[cfg(windows)]
    #[test]
    fn a_local_drive_is_not_mapped() {
        assert_eq!(crate::fs::mapped_remote('C'), None);
        assert_eq!(crate::fs::mapped_remote('1'), None);
    }

    /// A stand-in for a terminal: `sh` writes its working folder, which `start` gave it.
    #[cfg(unix)]
    #[test]
    fn a_launch_runs_in_its_folder() {
        let dir = std::env::temp_dir().join(format!("gezik-terminal-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let launch = Launch {
            program: "sh".into(),
            args: vec!["-c".into(), "pwd > where.tmp && mv where.tmp where.txt".into()],
            dir: dir.clone(),
            elevated: false,
        };
        start(&launch).unwrap();
        let file = dir.join("where.txt");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !file.exists() {
            assert!(std::time::Instant::now() < deadline, "the program did not run");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let written = std::fs::read_to_string(&file).unwrap();
        assert_eq!(Path::new(written.trim_end()).canonicalize().unwrap(), dir.canonicalize().unwrap());
        let missing = Launch { program: "/nonexistent/gezik-terminal".into(), ..launch };
        assert!(matches!(start(&missing), Err(TerminalError::Failed(_))));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Opens a real terminal: run by hand (`cargo test -p gezik-platform terminal -- --ignored`).
    #[test]
    #[ignore = "opens a terminal window"]
    fn opens_a_terminal_in_the_temp_folder() {
        open(&std::env::temp_dir(), false, None).unwrap();
    }
}
