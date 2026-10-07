//! "Open terminal" (spec 3): which terminal opens a folder on this system, and opening it so
//! that it lives on after Gezik. The choice is pure (`choose`, with the environment and PATH
//! given in a `Lookup`), so every rule is tested on every system; `open` asks the real ones.
//! Windows' and Linux's rules were tried in docs/superpowers/notes/2026-10-08-gunluk-kolayliklar-probe.md.

use std::ffi::OsString;
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
    Failed(String),
}

/// The environment `choose` decides by.
pub struct Lookup<'a> {
    pub os: Os,
    /// `[terminal] command`.
    pub command: Option<&'a [String]>,
    /// `$TERMINAL` (Linux).
    pub terminal_var: Option<&'a str>,
    /// `XDG_CURRENT_DESKTOP` (Linux), `:`-separated.
    pub desktop: Option<&'a str>,
    /// Whether a program runs: a name on PATH (`.exe` added on Windows) or a path.
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

/// `args` as one Windows command line, the way `CommandLineToArgvW` splits it back.
pub fn windows_command_line(args: &[String]) -> String {
    let mut out = String::new();
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        if !arg.is_empty() && !arg.contains([' ', '\t', '"']) {
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
        let mut args = gezik_core::batch::convert::expand_dir_command(command, dir).map_err(TerminalError::Failed)?;
        let program = args.remove(0);
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
            let go = format!("Set-Location -LiteralPath '{}'", text.replace('\'', "''"));
            Ok(launch(shell, vec!["-NoExit".into(), "-Command".into(), go.into()]))
        }
        Os::Mac => Ok(launch("/usr/bin/open", vec!["-a".into(), "Terminal".into(), dir.as_os_str().to_owned()])),
        Os::Linux => {
            if let Some(var) = lookup.terminal_var {
                let mut words = var.split_whitespace();
                if let Some(program) = words.next().filter(|program| found(program)) {
                    return Ok(launch(program, words.map(OsString::from).collect()));
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

/// Whether `name` runs: a path to a program, or a program in a folder of PATH (`.exe` added on
/// Windows, where the Store's app aliases count: Rust 1.99 sees them as files, probe 4.1).
fn on_path(name: &str) -> bool {
    let path = Path::new(name);
    if path.components().count() > 1 {
        return runnable(path);
    }
    let Some(dirs) = std::env::var_os("PATH") else { return false };
    let file = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    std::env::split_paths(&dirs).filter(|dir| dir.is_absolute()).any(|dir| runnable(&dir.join(&file)))
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
    let dirs = std::env::var_os("PATH")?;
    std::env::split_paths(&dirs)
        .map(|dir| dir.join(name))
        .find(|path| runnable(path))
        .and_then(|path| std::fs::canonicalize(path).ok())
        .and_then(|path| path.file_name().map(|n| n.to_string_lossy().into_owned()))
}

/// Opens a terminal in `dir` (`admin`: as administrator, Windows only). Looks through PATH, so
/// not on the UI thread. The terminal does not end with Gezik.
pub fn open(dir: &Path, admin: bool, command: Option<&[String]>) -> Result<(), TerminalError> {
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
    start(&choose(dir, admin, &lookup)?)
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

/// A process of its own group (Ctrl+C where Gezik was started does not reach it), waited for
/// on a small thread so that one that exits at once (gnome-terminal hands its window to its
/// server) leaves no zombie.
#[cfg(unix)]
fn start(launch: &Launch) -> Result<(), TerminalError> {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    let mut child = Command::new(&launch.program)
        .args(&launch.args)
        .current_dir(&launch.dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|err| TerminalError::Failed(crate::fs::describe(&err)))?;
    let _ = std::thread::Builder::new().name("gezik-terminal".into()).stack_size(64 * 1024).spawn(move || {
        let _ = child.wait();
    });
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
        let command = vec!["wezterm".to_owned(), "start".to_owned(), "--cwd".to_owned(), "{dir}".to_owned()];
        for os in [Os::Windows, Os::Mac, Os::Linux] {
            let mut lookup = look(os, &none);
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
