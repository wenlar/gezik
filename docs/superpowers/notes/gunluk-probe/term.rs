//! Q4: open a terminal in a folder.
//!   Windows: `term` runs the checks (each terminal writes its working folder to a file and closes).
//!   Linux: `term <dir>` launches the chosen terminal; `term --plan <dir>` prints the command.
//!   macOS: `term <dir>` runs `open -a Terminal <dir>`.
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    #[cfg(windows)]
    win::main();
    #[cfg(all(unix, not(target_os = "macos")))]
    linux::main();
    #[cfg(target_os = "macos")]
    {
        let dir = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
        println!("{:?}", mac::open_terminal(&dir, None));
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    /// `open -a Terminal <dir>`: a new Terminal window in that folder. `app` may name another
    /// (iTerm, Ghostty, WezTerm); `open` fails (exit 1) if it is not installed.
    pub fn open_terminal(dir: &Path, app: Option<&str>) -> std::io::Result<()> {
        let status = Command::new("/usr/bin/open").arg("-a").arg(app.unwrap_or("Terminal")).arg(dir).status()?;
        if status.success() { Ok(()) } else { Err(std::io::Error::other(format!("open: {status}"))) }
    }
}

/// Linux: which terminal, and how it is told the folder. Every terminal is also started with
/// the folder as its working directory, so ones without a flag (xterm, st, urxvt) work too.
#[cfg_attr(not(all(unix, not(target_os = "macos"))), allow(dead_code))]
pub mod linux_plan {
    use super::*;

    /// How a terminal takes its starting folder.
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub enum DirArg {
        /// `--flag=DIR`
        Joined(&'static str),
        /// `--flag DIR`
        Separate(&'static str),
        /// Words before the folder (`wezterm start --cwd DIR`).
        Sub(&'static [&'static str]),
        /// Only the working directory.
        Cwd,
    }

    /// Known terminals, in the order tried after `$TERMINAL` and x-terminal-emulator.
    /// Desktop defaults first (GNOME's new and old, KDE, Xfce, ...), then the rest.
    pub const KNOWN: &[(&str, DirArg)] = &[
        // GNOME 47+ (Fedora, Ubuntu 25.10): the folder counts only with --new-window or --tab.
        ("ptyxis", DirArg::Sub(&["--new-window", "--working-directory"])),
        ("kgx", DirArg::Separate("--working-directory")),    // GNOME Console
        ("gnome-terminal", DirArg::Joined("--working-directory")),
        ("konsole", DirArg::Separate("--workdir")),
        ("xfce4-terminal", DirArg::Joined("--working-directory")),
        ("mate-terminal", DirArg::Joined("--working-directory")),
        ("lxterminal", DirArg::Joined("--working-directory")),
        ("qterminal", DirArg::Separate("--workdir")),
        ("tilix", DirArg::Joined("--working-directory")),
        ("terminator", DirArg::Joined("--working-directory")),
        ("deepin-terminal", DirArg::Separate("--work-directory")),
        ("cosmic-term", DirArg::Cwd),
        ("ghostty", DirArg::Joined("--working-directory")),
        ("wezterm", DirArg::Sub(&["start", "--cwd"])),
        ("kitty", DirArg::Separate("--directory")),
        ("alacritty", DirArg::Separate("--working-directory")),
        ("foot", DirArg::Joined("--working-directory")),
        ("terminology", DirArg::Joined("--current-directory")),
        ("urxvt", DirArg::Separate("-cd")),
        ("xterm", DirArg::Cwd),
        ("st", DirArg::Cwd),
    ];

    pub fn dir_arg_for(program: &str) -> DirArg {
        let name = Path::new(program).file_name().and_then(|n| n.to_str()).unwrap_or(program);
        KNOWN.iter().find(|(n, _)| *n == name).map_or(DirArg::Cwd, |(_, a)| *a)
    }

    pub fn find_in_path(program: &str, path: &str) -> Option<PathBuf> {
        if program.contains('/') {
            return Path::new(program).is_file().then(|| PathBuf::from(program));
        }
        std::env::split_paths(path).map(|d| d.join(program)).find(|p| is_executable(p))
    }

    fn is_executable(p: &Path) -> bool {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            p.metadata().is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        }
        #[cfg(not(unix))]
        p.is_file()
    }

    /// The command for `dir`: `$TERMINAL` (a program name, possibly with words after it),
    /// then x-terminal-emulator (Debian's alternative; it resolves to a real terminal, whose
    /// flag is used), then the known list.
    pub fn plan(dir: &Path, terminal_env: Option<&str>, path: &str) -> Option<Command> {
        let mut candidates: Vec<(PathBuf, Vec<String>)> = Vec::new();
        if let Some(t) = terminal_env.map(str::trim).filter(|t| !t.is_empty()) {
            let mut words = t.split_whitespace().map(str::to_owned);
            let program = words.next()?;
            if let Some(found) = find_in_path(&program, path) {
                candidates.push((found, words.collect()));
            }
        }
        if let Some(found) = find_in_path("x-terminal-emulator", path) {
            candidates.push((found, Vec::new()));
        }
        for (name, _) in KNOWN {
            if let Some(found) = find_in_path(name, path) {
                candidates.push((found, Vec::new()));
            }
        }
        let (program, extra) = candidates.into_iter().next()?;
        // x-terminal-emulator is a symlink chain to the real one: its flag is the real one's.
        let real = std::fs::canonicalize(&program).unwrap_or_else(|_| program.clone());
        let mut command = Command::new(&program);
        command.args(&extra).current_dir(dir);
        let real_name = real.file_name().and_then(|n| n.to_str()).unwrap_or("").to_owned();
        let real_name = real_name.strip_suffix(".wrapper").unwrap_or(&real_name).to_owned();
        match dir_arg_for(&real_name) {
            DirArg::Joined(flag) => {
                let mut a = std::ffi::OsString::from(format!("{flag}="));
                a.push(dir.as_os_str());
                command.arg(a);
            }
            DirArg::Separate(flag) => {
                command.arg(flag).arg(dir);
            }
            DirArg::Sub(words) => {
                command.args(words).arg(dir);
            }
            DirArg::Cwd => {}
        }
        Some(command)
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod linux {
    use super::*;
    pub fn main() {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let (dry, dir) = match args.as_slice() {
            [flag, dir] if flag == "--plan" => (true, dir.clone()),
            [dir] => (false, dir.clone()),
            _ => (true, ".".into()),
        };
        let terminal = std::env::var("TERMINAL").ok();
        let path = std::env::var("PATH").unwrap_or_default();
        let Some(mut command) = linux_plan::plan(Path::new(&dir), terminal.as_deref(), &path) else {
            println!("no terminal found");
            return;
        };
        println!("{:?} (cwd {:?})", command, command.get_current_dir());
        if !dry {
            // Detached: Gezik does not wait; the child is reaped by a thread (or setsid).
            match command.spawn() {
                Ok(mut child) => {
                    std::thread::sleep(std::time::Duration::from_secs(3));
                    println!("still running after 3 s: {:?}", child.try_wait().map(|s| s.is_none()));
                }
                Err(e) => println!("spawn: {e}"),
            }
        }
    }
}

#[cfg(windows)]
mod win {
    use std::os::windows::process::CommandExt;
    use std::time::{Duration, Instant};

    use super::*;

    const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;

    /// Windows Terminal through its App Execution Alias, if installed and the alias is on.
    pub fn find_wt() -> Option<PathBuf> {
        let local = std::env::var_os("LOCALAPPDATA")?;
        let alias = PathBuf::from(local).join(r"Microsoft\WindowsApps\wt.exe");
        // The alias is a reparse point (IO_REPARSE_TAG_APPEXECLINK): symlink_metadata sees it.
        std::fs::symlink_metadata(&alias).is_ok().then_some(alias)
    }

    /// wt splits its command line at `;` (a new-tab separator) even inside one argument:
    /// `\;` keeps it.
    pub fn wt_dir_arg(dir: &Path) -> String {
        dir.to_string_lossy().replace(';', r"\;")
    }

    pub fn wt_command(wt: &Path, dir: &Path) -> Command {
        let mut c = Command::new(wt);
        c.arg("-d").arg(wt_dir_arg(dir));
        c
    }

    pub fn powershell_command(dir: &Path) -> Command {
        let mut c = Command::new("powershell.exe");
        c.arg("-NoExit").current_dir(dir).creation_flags(CREATE_NEW_CONSOLE);
        c
    }

    /// Elevated terminal: ShellExecuteW "runas" (ShellExecuteExW would need the
    /// Win32_System_Registry feature: SHELLEXECUTEINFOW has an HKEY field). The folder goes in
    /// the arguments, not only in lpDirectory. A cancelled UAC prompt returns 5
    /// (SE_ERR_ACCESSDENIED) and GetLastError() == ERROR_CANCELLED (1223).
    pub fn shell_execute(verb: &str, file: &str, params: &str, dir: &Path) -> Result<(), (isize, u32)> {
        use windows::Win32::Foundation::GetLastError;
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        use windows::core::HSTRING;
        let r = unsafe {
            ShellExecuteW(
                None,
                &HSTRING::from(verb),
                &HSTRING::from(file),
                &HSTRING::from(params),
                &HSTRING::from(dir.as_os_str()),
                SW_SHOWNORMAL,
            )
        };
        let code = r.0 as isize;
        if code > 32 { Ok(()) } else { Err((code, unsafe { GetLastError() }.0)) }
    }

    /// PowerShell single-quoted literal: ' doubles.
    pub fn ps_quote(s: &str) -> String {
        format!("'{}'", s.replace('\'', "''"))
    }

    fn wait_for(file: &Path) -> Option<String> {
        let t = Instant::now();
        while t.elapsed() < Duration::from_secs(15) {
            if let Ok(bytes) = std::fs::read(file)
                && !bytes.is_empty()
            {
                std::thread::sleep(Duration::from_millis(200));
                let bytes = std::fs::read(file).ok()?;
                // PowerShell's Out-File utf8 has a BOM; cmd writes the OEM code page.
                let text = String::from_utf8(bytes.clone())
                    .map(|s| s.trim_start_matches('\u{feff}').trim().to_owned())
                    .unwrap_or_else(|_| format!("(not UTF-8: {} bytes)", bytes.len()));
                return Some(text);
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        None
    }

    pub fn main() {
        let wt = find_wt();
        let alias = PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap()).join(r"Microsoft\WindowsApps\wt.exe");
        println!("wt alias: exists()={} symlink_metadata={:?} metadata={:?}", alias.exists(),
            std::fs::symlink_metadata(&alias).map(|m| m.len()).map_err(|e| e.raw_os_error()),
            std::fs::metadata(&alias).map(|m| m.len()).map_err(|e| e.raw_os_error()));
        println!("find_wt: {wt:?}");

        let base = std::env::temp_dir().join("probe7 term");
        let _ = std::fs::remove_dir_all(&base);
        let dirs = [base.join("boşluk ve ş 日本"), base.join("noktalı;virgül"), base.join("[köşeli] it's")];
        for d in &dirs {
            std::fs::create_dir_all(d).unwrap();
        }
        let out = std::env::temp_dir().join("probe7-term-out.txt");
        let out_s = out.to_string_lossy().into_owned();

        if let Some(wt) = &wt {
            for d in &dirs {
                let _ = std::fs::remove_file(&out);
                // The probe's tab runs PowerShell to write its folder, then closes.
                let mut c = wt_command(wt, d);
                c.args(["powershell", "-NoProfile", "-Command"]).arg(format!(
                    "(Get-Location).ProviderPath | Out-File -Encoding utf8 {}",
                    ps_quote(&out_s)
                ));
                let t = Instant::now();
                let status = c.status();
                println!("wt -d {:?}: {:?} in {:?} -> {:?}", wt_dir_arg(d), status.map(|s| s.code()), t.elapsed(), wait_for(&out));
            }
            // Unescaped `;` for comparison.
            let _ = std::fs::remove_file(&out);
            let mut c = Command::new(wt);
            c.arg("-d").arg(&dirs[1]).args(["powershell", "-NoProfile", "-Command"]).arg(format!(
                "(Get-Location).ProviderPath | Out-File -Encoding utf8 {}",
                ps_quote(&out_s)
            ));
            let _ = c.status();
            println!("wt -d with a raw ';' -> {:?}", wait_for(&out));
        }

        for d in &dirs {
            let _ = std::fs::remove_file(&out);
            let mut c = Command::new("powershell.exe");
            c.current_dir(d).creation_flags(CREATE_NEW_CONSOLE).args(["-NoProfile", "-Command"]).arg(format!(
                "(Get-Location).ProviderPath | Out-File -Encoding utf8 {}",
                ps_quote(&out_s)
            ));
            let child = c.spawn();
            println!("powershell (current_dir) {:?}: spawned {} -> {:?}", d.file_name().unwrap(), child.is_ok(), wait_for(&out));
        }

        // ShellExecuteExW with the parameters an elevated launch uses, verb "open" here
        // ("runas" would raise the UAC prompt; this machine prompts, ConsentPromptBehaviorAdmin=5).
        for d in &dirs {
            let _ = std::fs::remove_file(&out);
            let params = format!(
                "-NoProfile -Command Set-Location -LiteralPath {}; (Get-Location).ProviderPath | Out-File -Encoding utf8 {}",
                ps_quote(&d.to_string_lossy()),
                ps_quote(&out_s)
            );
            let r = shell_execute("open", "powershell.exe", &params, d);
            println!("ShellExecuteW open powershell {:?}: {:?} -> {:?}", d.file_name().unwrap(), r, wait_for(&out));
        }
        if let Some(wt) = &wt {
            let _ = std::fs::remove_file(&out);
            let params = format!(
                "-d \"{}\" powershell -NoProfile -Command \"(Get-Location).ProviderPath | Out-File -Encoding utf8 '{}'\"",
                wt_dir_arg(&dirs[0]),
                out_s
            );
            let r = shell_execute("open", &wt.to_string_lossy(), &params, &dirs[0]);
            println!("ShellExecuteW open wt.exe: {:?} -> {:?}", r, wait_for(&out));
        }
        let _ = std::fs::remove_file(&out);
        let _ = std::fs::remove_dir_all(&base);
    }
}
