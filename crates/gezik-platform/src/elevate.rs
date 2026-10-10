//! The one-shot administrator helper (spec 9 §10): starting it from Gezik behind the system's
//! prompt (Windows `runas`, macOS `osascript … with administrator privileges`, Linux `pkexec`)
//! and reading its replies; and, inside the helper, getting ready and answering. Nothing here
//! runs unless the user asked: no service, no listener, no file; the helper ends with its list,
//! the pipe and the reading thread with it.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};

use gezik_core::elevated::{self, LaunchError, Op};

pub const NEEDS_PKEXEC: &str = "Administrator operations need pkexec (polkit).";
pub const NO_AGENT: &str =
    "No polkit authentication agent is running, so nothing can ask for the administrator password.";
pub const ENDED: &str = "The administrator helper stopped early; some items may have been done. Check the folder.";
pub const NOT_PLAIN_EXE: &str = "Gezik's own path has characters the prompt cannot carry";
/// A reply line is cut to this; more than `MAX_READ` bytes in all are not read.
const MAX_LINE: usize = 4096;
const MAX_READ: u64 = 16 * 1024 * 1024;

/// The checks before anything starts: a plain, full exe path and a list that fits one command line.
pub fn ready<'a>(exe: &'a Path, ops: &[Op]) -> Result<&'a str, LaunchError> {
    let text = exe
        .to_str()
        .filter(|text| exe.is_absolute() && !text.chars().any(char::is_control))
        .ok_or_else(|| LaunchError::Failed(NOT_PLAIN_EXE.into()))?;
    if !elevated::fits(text, ops, cfg!(windows)) {
        return Err(LaunchError::Failed(elevated::TOO_MANY.into()));
    }
    Ok(text)
}

/// Starts the helper (`exe --elevated 1 <channel> <ops>`) behind the system's prompt and hands
/// each reply line to `reply` as it comes; returns once the helper has ended. `owner`: the
/// window the Windows prompt belongs to (0: none); `prompt`: macOS's line. Blocking: the
/// engine's job thread only. Never called by a test.
pub fn run(exe: &Path, ops: &[Op], prompt: &str, owner: isize, reply: &mut dyn FnMut(&str)) -> Result<(), LaunchError> {
    let text = ready(exe, ops)?;
    imp::run(exe, text, ops, prompt, owner, reply)
}

/// What the helper writes, line by line (`\r\n` or `\n`), each cut at `MAX_LINE`.
pub(crate) fn read_lines(from: impl Read, reply: &mut dyn FnMut(&str)) {
    let mut reader = BufReader::new(from.take(MAX_READ));
    let mut line = Vec::new();
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        while line.last().is_some_and(|b| *b == b'\n' || *b == b'\r') {
            line.pop();
        }
        line.truncate(MAX_LINE);
        reply(&String::from_utf8_lossy(&line));
    }
}

/// A short first line of what a tool said on stderr.
fn first_line(text: &str) -> String {
    text.lines().map(str::trim).find(|line| !line.is_empty()).unwrap_or_default().chars().take(200).collect()
}

/// `ShellExecuteExW`'s error: ERROR_CANCELLED is the user's No at the UAC prompt.
pub fn windows_launch_error(code: Option<i32>, text: String) -> LaunchError {
    if code == Some(1223) { LaunchError::Cancelled } else { LaunchError::Failed(text) }
}

/// `osascript`'s end: -128 is the user's Cancel at the password prompt.
pub fn mac_outcome(code: Option<i32>, stderr: &str) -> Result<(), LaunchError> {
    match code {
        Some(0) => Ok(()),
        _ if stderr.contains("(-128)") => Err(LaunchError::Cancelled),
        None => Err(LaunchError::Failed(ENDED.into())),
        Some(_) => Err(LaunchError::Failed(first_line(stderr))),
    }
}

/// `pkexec`'s end: 126 the user dismissed or was not allowed, 127 no way to ask (no agent).
pub fn linux_outcome(code: Option<i32>, stderr: &str) -> Result<(), LaunchError> {
    match code {
        Some(0) => Ok(()),
        Some(126) => Err(LaunchError::Cancelled),
        Some(127) if stderr.contains("authentication agent") => Err(LaunchError::Failed(NO_AGENT.into())),
        None => Err(LaunchError::Failed(ENDED.into())),
        Some(_) => Err(LaunchError::Failed(first_line(stderr))),
    }
}

/// `pkexec` from the usual places only: never a `pkexec` that `PATH` happens to find first.
pub fn pkexec_path(exists: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    ["/usr/bin/pkexec", "/bin/pkexec", "/usr/local/bin/pkexec"].into_iter().map(PathBuf::from).find(|p| exists(p))
}

/// The helper's first steps (spec §10.4): system DLLs only (Windows), a fixed working folder,
/// `umask 022` (Unix). Before anything else of the helper; an error: the helper must not go on.
pub fn harden() -> io::Result<()> {
    imp::harden()
}

/// Whether this process runs with administrator rights (an elevated token; root).
pub fn is_elevated() -> bool {
    imp::is_elevated()
}

/// The helper's reply channel: standard output (Unix), or the parent's pipe (Windows), opened so
/// that the pipe's owner can never act as the helper.
pub fn open_channel(channel: &str) -> io::Result<Box<dyn Write>> {
    imp::open_channel(channel)
}

/// The folders the helper never changes themselves (`gezik_core::elevated::check`, `secure::Guard`).
pub fn protected() -> Vec<PathBuf> {
    imp::protected()
}

/// The confirm text's extra line when `others_can_change(exe)` (security review 1, 2).
pub const EXPOSED: &str = "Gezik is in a folder other programs can change.";

/// Whether something other than an administrator could swap Gezik's exe, or put a DLL beside
/// it, between the prompt and the helper's start: Windows, the exe or its folder lets a
/// non-administrator write; Unix, the exe or a folder above it is not root's or is writable
/// beyond root and the admin group. Only a warning: the helper still starts. Unreadable: true.
pub fn others_can_change(exe: &Path) -> bool {
    imp::others_can_change(exe)
}

/// One Unix item (the exe or a folder above it) that someone other than root/admin may change.
#[cfg_attr(windows, allow(dead_code))]
fn open_to_others(uid: u32, gid: u32, mode: u32, macos: bool) -> bool {
    // macOS's admin group (80) owns /Applications with group write.
    let admin_group = gid == 0 || (macos && gid == 80);
    uid != 0 || mode & 0o002 != 0 || (mode & 0o020 != 0 && !admin_group)
}

/// The native window of `window` (Windows: its HWND), for the UAC prompt; 0 elsewhere.
pub fn owner_of(window: &impl raw_window_handle::HasWindowHandle) -> isize {
    #[cfg(windows)]
    {
        if let Ok(handle) = window.window_handle()
            && let raw_window_handle::RawWindowHandle::Win32(win) = handle.as_raw()
        {
            return win.hwnd.get();
        }
    }
    let _ = window;
    0
}

#[cfg(unix)]
mod imp {
    use std::io::{self, Read, Write};
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};

    use gezik_core::elevated::{self, LaunchError, Op};

    pub(super) fn run(
        exe: &Path,
        text: &str,
        ops: &[Op],
        prompt: &str,
        _owner: isize,
        reply: &mut dyn FnMut(&str),
    ) -> Result<(), LaunchError> {
        let args =
            elevated::encode(elevated::STDOUT, ops).ok_or_else(|| LaunchError::Failed(elevated::NOT_UNICODE.into()))?;
        #[cfg(target_os = "macos")]
        let mut command = {
            let _ = exe;
            let mut command = Command::new("/usr/bin/osascript");
            command.arg("-e").arg(elevated::osascript_source(text, &args, prompt));
            command
        };
        #[cfg(not(target_os = "macos"))]
        let mut command = {
            let _ = (text, prompt);
            let pkexec = super::pkexec_path(|p| p.exists())
                .ok_or_else(|| LaunchError::Unavailable(super::NEEDS_PKEXEC.into()))?;
            let mut command = Command::new(pkexec);
            command.arg(exe).args(&args);
            command
        };
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| LaunchError::Unavailable(crate::fs::describe(&err)))?;
        let stderr = child.stderr.take();
        let errors = std::thread::spawn(move || {
            let mut text = String::new();
            if let Some(stderr) = stderr {
                let _ = stderr.take(64 * 1024).read_to_string(&mut text);
            }
            text
        });
        if let Some(stdout) = child.stdout.take() {
            super::read_lines(stdout, reply);
        }
        let status = child.wait().map_err(|err| LaunchError::Failed(crate::fs::describe(&err)))?;
        let errors = errors.join().unwrap_or_default();
        if cfg!(target_os = "macos") {
            super::mac_outcome(status.code(), &errors)
        } else {
            super::linux_outcome(status.code(), &errors)
        }
    }

    pub(super) fn harden() -> io::Result<()> {
        // SAFETY: umask only changes this process's mask.
        unsafe { libc::umask(0o022) };
        std::env::set_current_dir("/")
    }

    pub(super) fn is_elevated() -> bool {
        // SAFETY: geteuid only reads.
        unsafe { libc::geteuid() == 0 }
    }

    pub(super) fn open_channel(channel: &str) -> io::Result<Box<dyn Write>> {
        if !elevated::valid_channel(channel, false) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok(Box::new(io::stdout()))
    }

    pub(super) fn protected() -> Vec<PathBuf> {
        elevated::unix_protected(cfg!(target_os = "macos"))
    }

    pub(super) fn others_can_change(exe: &Path) -> bool {
        use std::os::unix::fs::MetadataExt;
        let Ok(real) = std::fs::canonicalize(exe) else { return true };
        real.ancestors().any(|item| {
            std::fs::metadata(item)
                .map_or(true, |m| super::open_to_others(m.uid(), m.gid(), m.mode(), cfg!(target_os = "macos")))
        })
    }
}

#[cfg(windows)]
mod imp {
    use std::fs::File;
    use std::io::{self, Read, Write};
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::{AsRawHandle, FromRawHandle};
    use std::path::{Path, PathBuf};

    use ::windows::Win32::Foundation::{
        CloseHandle, ERROR_BROKEN_PIPE, ERROR_IO_PENDING, ERROR_NO_DATA, ERROR_PIPE_CONNECTED, ERROR_SUCCESS,
        GENERIC_WRITE, HANDLE, HLOCAL, HWND, LocalFree, WAIT_OBJECT_0,
    };
    use ::windows::Win32::Security::Authorization::{
        ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, GetNamedSecurityInfoW,
        SDDL_REVISION_1, SE_FILE_OBJECT,
    };
    use ::windows::Win32::Security::{
        ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, DACL_SECURITY_INFORMATION, GetAce, GetTokenInformation, INHERIT_ONLY_ACE,
        PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation,
    };
    use ::windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, FILE_SHARE_MODE, FILE_SHARE_READ,
        OPEN_EXISTING, PIPE_ACCESS_INBOUND, ReadFile, SECURITY_IDENTIFICATION, SECURITY_SQOS_PRESENT,
    };
    use ::windows::Win32::System::Com::{
        COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize,
    };
    use ::windows::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
    use ::windows::Win32::System::LibraryLoader::{
        LOAD_LIBRARY_SEARCH_SYSTEM32, SetDefaultDllDirectories, SetDllDirectoryW,
    };
    use ::windows::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, GetNamedPipeClientProcessId, PIPE_READMODE_BYTE,
        PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT,
    };
    use ::windows::Win32::System::SystemInformation::{GetSystemDirectoryW, GetSystemWindowsDirectoryW};
    use ::windows::Win32::System::SystemServices::ACCESS_ALLOWED_ACE_TYPE;
    use ::windows::Win32::System::Threading::{
        CreateEventW, GetCurrentProcess, GetProcessId, INFINITE, OpenProcessToken, WaitForMultipleObjects,
        WaitForSingleObject,
    };
    use ::windows::Win32::UI::Shell::{
        SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW,
    };
    use ::windows::Win32::UI::WindowsAndMessaging::SW_HIDE;
    use ::windows::core::{HSTRING, PCWSTR, PWSTR, w};

    use gezik_core::elevated::{self, LaunchError, Op};

    /// Connections turned away before Gezik stops waiting for the helper's.
    const CONNECTS: usize = 8;

    pub(super) fn run(
        exe: &Path,
        _text: &str,
        ops: &[Op],
        _prompt: &str,
        owner: isize,
        reply: &mut dyn FnMut(&str),
    ) -> Result<(), LaunchError> {
        let name = format!("{}{}", elevated::PIPE_PREFIX, crate::fs::secure::random_name());
        let args = elevated::encode(&name, ops).ok_or_else(|| LaunchError::Failed(elevated::NOT_UNICODE.into()))?;
        let pipe = create_pipe(&name).map_err(|err| LaunchError::Failed(crate::fs::describe(&err)))?;
        // Held until the helper ends: meanwhile nobody renames or replaces the exe behind the
        // prompt (deviation 15). shortcut: best effort; if something has the exe open for
        // writing, Gezik goes on without the hold (spec §10.4 counts exe swaps out anyway).
        let _held = std::fs::OpenOptions::new().read(true).share_mode(FILE_SHARE_READ.0).open(exe).ok();
        let process = launch(exe, &elevated::windows_command_line(&args), owner)?;
        let read = serve(&pipe, process, reply);
        // SAFETY: the helper's handle from ShellExecuteExW, waited on and closed once.
        unsafe {
            WaitForSingleObject(process, INFINITE);
            let _ = CloseHandle(process);
        }
        read
    }

    fn launch(exe: &Path, params: &str, owner: isize) -> Result<HANDLE, LaunchError> {
        let (file, params) = (HSTRING::from(exe.as_os_str()), HSTRING::from(params));
        // ShellExecuteExW wants COM on its thread.
        // SAFETY: undone below on this thread.
        let com = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) }.is_ok();
        let mut info = SHELLEXECUTEINFOW {
            cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
            hwnd: HWND(owner as *mut _),
            lpVerb: w!("runas"),
            lpFile: PCWSTR(file.as_ptr()),
            lpParameters: PCWSTR(params.as_ptr()),
            nShow: SW_HIDE.0,
            ..Default::default()
        };
        // SAFETY: the strings live through the call.
        let started = unsafe { ShellExecuteExW(&mut info) };
        if com {
            unsafe { CoUninitialize() };
        }
        match started {
            Ok(()) if !info.hProcess.is_invalid() => Ok(info.hProcess),
            Ok(()) => Err(LaunchError::Failed(super::ENDED.into())),
            Err(err) => {
                let err = crate::fs::io_error(err);
                Err(super::windows_launch_error(err.raw_os_error(), crate::fs::describe(&err)))
            }
        }
    }

    /// The result pipe: inbound, one instance (the name taken already fails), never from another
    /// machine, open to this user and Administrators only (a standard user's helper runs as an
    /// administrator account).
    pub(super) fn create_pipe(name: &str) -> io::Result<File> {
        let sid = crate::instance::user_sid().ok_or_else(|| io::Error::other("Gezik cannot read its user"))?;
        let sddl = HSTRING::from(format!("D:P(A;;GA;;;{sid})(A;;GA;;;BA)"));
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        // SAFETY: `descriptor` is written by the call and freed below.
        unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(&sddl, SDDL_REVISION_1, &mut descriptor, None) }
            .map_err(io::Error::other)?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: false.into(),
        };
        // SAFETY: the name and attributes live through the call.
        let handle = unsafe {
            CreateNamedPipeW(
                &HSTRING::from(name),
                PIPE_ACCESS_INBOUND | FILE_FLAG_FIRST_PIPE_INSTANCE | FILE_FLAG_OVERLAPPED,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                0,
                64 * 1024,
                0,
                Some(&attributes),
            )
        };
        let error = io::Error::last_os_error();
        // SAFETY: allocated by ConvertStringSecurityDescriptorToSecurityDescriptorW.
        unsafe {
            let _ = LocalFree(Some(HLOCAL(descriptor.0)));
        }
        if handle.is_invalid() {
            return Err(error);
        }
        // SAFETY: a fresh handle owned from here on.
        Ok(unsafe { File::from_raw_handle(handle.0) })
    }

    struct Event(HANDLE);

    impl Drop for Event {
        fn drop(&mut self) {
            // SAFETY: made by CreateEventW, closed once.
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }

    /// Waits for the helper (`process`) to connect, turning away any other process, then reads
    /// its lines until it closes the pipe.
    pub(super) fn serve(pipe: &File, process: HANDLE, reply: &mut dyn FnMut(&str)) -> Result<(), LaunchError> {
        let h = HANDLE(pipe.as_raw_handle());
        // SAFETY: a manual-reset event for the overlapped calls.
        let event =
            Event(unsafe { CreateEventW(None, true, false, None) }.map_err(|e| LaunchError::Failed(e.message()))?);
        // SAFETY: a real process handle (WaitForMultipleObjects below fails on a pseudo handle).
        let pid = unsafe { GetProcessId(process) };
        for _ in 0..CONNECTS {
            let mut overlapped = OVERLAPPED { hEvent: event.0, ..Default::default() };
            // SAFETY: `overlapped` lives until the connection is done or cancelled below.
            match unsafe { ConnectNamedPipe(h, Some(&mut overlapped)) } {
                Ok(()) => {}
                // Connected before Gezik asked; NO_DATA: and already gone, its lines still in the pipe.
                Err(err)
                    if err.code() == ERROR_PIPE_CONNECTED.to_hresult() || err.code() == ERROR_NO_DATA.to_hresult() => {}
                Err(err) if err.code() == ERROR_IO_PENDING.to_hresult() => {
                    // SAFETY: two live handles.
                    let woke = unsafe { WaitForMultipleObjects(&[event.0, process], false, INFINITE) };
                    if woke != WAIT_OBJECT_0 {
                        // The helper ended before it connected: the pending connect is cancelled
                        // and waited for, so `overlapped` outlives it.
                        let mut n = 0;
                        unsafe {
                            let _ = CancelIoEx(h, Some(&overlapped));
                            let _ = GetOverlappedResult(h, &overlapped, &mut n, true);
                        }
                        return Err(LaunchError::Failed(super::ENDED.into()));
                    }
                }
                Err(err) => return Err(LaunchError::Failed(err.message())),
            }
            let mut client = 0u32;
            // SAFETY: a connected pipe.
            if unsafe { GetNamedPipeClientProcessId(h, &mut client) }.is_ok() && client == pid {
                super::read_lines(PipeReader { pipe: h, event: &event }, reply);
                return Ok(());
            }
            // Another process of the same user got there first: never read, turned away.
            unsafe {
                let _ = DisconnectNamedPipe(h);
            }
        }
        Err(LaunchError::Failed(super::ENDED.into()))
    }

    struct PipeReader<'a> {
        pipe: HANDLE,
        event: &'a Event,
    }

    impl Read for PipeReader<'_> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let mut overlapped = OVERLAPPED { hEvent: self.event.0, ..Default::default() };
            let mut n = 0u32;
            // SAFETY: `buf` and `overlapped` live until GetOverlappedResult has waited for the read.
            let started = unsafe { ReadFile(self.pipe, Some(buf), None, Some(&mut overlapped)) };
            let done = match started {
                Err(err) if err.code() != ERROR_IO_PENDING.to_hresult() => Err(err),
                _ => unsafe { GetOverlappedResult(self.pipe, &overlapped, &mut n, true) },
            };
            match done {
                Ok(()) => Ok(n as usize),
                Err(err) if err.code() == ERROR_BROKEN_PIPE.to_hresult() => Ok(0),
                Err(err) => Err(crate::fs::io_error(err)),
            }
        }
    }

    fn system_dir(get: impl Fn(&mut [u16]) -> u32) -> Option<String> {
        let mut buffer = [0u16; 260];
        let len = get(&mut buffer) as usize;
        (len > 0 && len < buffer.len()).then(|| String::from_utf16_lossy(&buffer[..len]))
    }

    pub(super) fn harden() -> io::Result<()> {
        // SAFETY: these change only this process's DLL search and current folder.
        unsafe {
            SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32)?;
            SetDllDirectoryW(w!(""))?;
        }
        // SAFETY: the buffer holds its length.
        let dir = system_dir(|buffer| unsafe { GetSystemDirectoryW(Some(buffer)) }).ok_or(io::ErrorKind::NotFound)?;
        std::env::set_current_dir(dir)
    }

    pub(super) fn is_elevated() -> bool {
        let mut token = HANDLE::default();
        // SAFETY: this process's pseudo handle; the token is closed below.
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }.is_err() {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION::default();
        let mut len = 0u32;
        // SAFETY: `elevation` is what TokenElevation writes.
        let read = unsafe {
            GetTokenInformation(
                token,
                TokenElevation,
                Some((&raw mut elevation).cast()),
                size_of::<TOKEN_ELEVATION>() as u32,
                &mut len,
            )
        }
        .is_ok();
        unsafe {
            let _ = CloseHandle(token);
        }
        read && elevation.TokenIsElevated != 0
    }

    pub(super) fn open_channel(channel: &str) -> io::Result<Box<dyn Write>> {
        if !elevated::valid_channel(channel, true) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        // Identification only: whoever owns the pipe may learn who wrote, never act as the helper.
        // SAFETY: the name lives through the call; the handle is owned by the File.
        let handle = unsafe {
            CreateFileW(
                &HSTRING::from(channel),
                GENERIC_WRITE.0,
                FILE_SHARE_MODE(0),
                None,
                OPEN_EXISTING,
                SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                None,
            )
        }
        .map_err(crate::fs::io_error)?;
        Ok(Box::new(unsafe { File::from_raw_handle(handle.0) }))
    }

    pub(super) fn protected() -> Vec<PathBuf> {
        // SAFETY: the buffer holds its length.
        let windows = system_dir(|buffer| unsafe { GetSystemWindowsDirectoryW(Some(buffer)) })
            .unwrap_or_else(|| r"C:\Windows".to_owned());
        elevated::windows_protected(&windows)
    }

    /// Rights that let a DLL in beside the exe or the exe be replaced or renamed: write data /
    /// add file (0x2), append / add folder (0x4), delete child (0x40), DELETE, WRITE_DAC,
    /// WRITE_OWNER, GENERIC_ALL, GENERIC_WRITE.
    const WRITES: u32 = 0x2 | 0x4 | 0x40 | 0x1_0000 | 0x4_0000 | 0x8_0000 | 0x1000_0000 | 0x4000_0000;
    /// SYSTEM, Administrators, TrustedInstaller.
    const TRUSTED: [&str; 3] =
        ["S-1-5-18", "S-1-5-32-544", "S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464"];

    pub(super) fn others_can_change(exe: &Path) -> bool {
        // Not the folders above: a drive's root lets every user add folders, and a folder
        // with a running exe inside cannot be renamed.
        [Some(exe), exe.parent()].into_iter().flatten().any(|item| writable_by_others(item).unwrap_or(true))
    }

    fn writable_by_others(item: &Path) -> Option<bool> {
        let mut dacl: *mut ACL = std::ptr::null_mut();
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        // SAFETY: `dacl` points into `descriptor`, which is freed below.
        let read = unsafe {
            GetNamedSecurityInfoW(
                &HSTRING::from(item.as_os_str()),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                None,
                None,
                Some(&mut dacl),
                None,
                &mut descriptor,
            )
        };
        if read != ERROR_SUCCESS {
            return None;
        }
        // SAFETY: the DACL of `descriptor`, still alive.
        let open = unsafe { open_aces(dacl) };
        // SAFETY: allocated by GetNamedSecurityInfoW.
        unsafe {
            let _ = LocalFree(Some(HLOCAL(descriptor.0)));
        }
        Some(open)
    }

    /// Whether an allow entry gives a write right to anyone but SYSTEM, Administrators or
    /// TrustedInstaller. Deny entries are not weighed (a warning too many, never one too few);
    /// entries only for children are skipped.
    /// SAFETY: `dacl` is null or a valid ACL.
    unsafe fn open_aces(dacl: *const ACL) -> bool {
        if dacl.is_null() {
            return true; // no DACL: everyone may do anything
        }
        // SAFETY: a valid ACL (caller).
        let count = unsafe { (*dacl).AceCount };
        for index in 0..u32::from(count) {
            let mut ace = std::ptr::null_mut();
            // SAFETY: index < AceCount; `ace` points into the ACL.
            if unsafe { GetAce(dacl, index, &mut ace) }.is_err() {
                return true;
            }
            // SAFETY: every ACE starts with its header.
            let header = unsafe { &*(ace as *const ACE_HEADER) };
            if u32::from(header.AceType) != ACCESS_ALLOWED_ACE_TYPE
                || u32::from(header.AceFlags) & INHERIT_ONLY_ACE.0 != 0
            {
                continue;
            }
            // SAFETY: an ACCESS_ALLOWED_ACE (its type), its SID starting at SidStart.
            let allowed = unsafe { &*(ace as *const ACCESS_ALLOWED_ACE) };
            if allowed.Mask & WRITES == 0 {
                continue;
            }
            let sid = PSID((&raw const allowed.SidStart).cast_mut().cast());
            let mut text = PWSTR::null();
            // SAFETY: a SID inside the ACE; `text` is freed below.
            if unsafe { ConvertSidToStringSidW(sid, &mut text) }.is_err() {
                return true;
            }
            // SAFETY: a NUL-ended string from the call.
            let name = unsafe { text.to_string() }.unwrap_or_default();
            // SAFETY: allocated by ConvertSidToStringSidW.
            unsafe {
                let _ = LocalFree(Some(HLOCAL(text.0.cast())));
            }
            if !TRUSTED.contains(&name.as_str()) {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_answers_are_mapped() {
        assert_eq!(windows_launch_error(Some(1223), "x".into()), LaunchError::Cancelled);
        assert_eq!(
            windows_launch_error(Some(2), "It no longer exists".into()),
            LaunchError::Failed("It no longer exists".into())
        );
        assert_eq!(mac_outcome(Some(0), ""), Ok(()));
        assert_eq!(
            mac_outcome(Some(1), "0:116: execution error: User canceled. (-128)\n"),
            Err(LaunchError::Cancelled)
        );
        assert_eq!(mac_outcome(Some(1), "\n  boom\nmore"), Err(LaunchError::Failed("boom".into())));
        assert_eq!(linux_outcome(Some(0), ""), Ok(()));
        assert_eq!(
            linux_outcome(Some(126), "Error executing command as another user: Not authorized"),
            Err(LaunchError::Cancelled)
        );
        assert_eq!(
            linux_outcome(Some(127), "Error executing command as another user: No authentication agent found."),
            Err(LaunchError::Failed(NO_AGENT.into()))
        );
        assert_eq!(linux_outcome(Some(127), "odd"), Err(LaunchError::Failed("odd".into())));
        assert_eq!(linux_outcome(None, ""), Err(LaunchError::Failed(ENDED.into())));
        assert_eq!(mac_outcome(Some(1), &"é".repeat(400)), Err(LaunchError::Failed("é".repeat(200))));
    }

    #[test]
    fn pkexec_is_looked_for_in_fixed_places() {
        assert_eq!(pkexec_path(|p| p == Path::new("/bin/pkexec")), Some(PathBuf::from("/bin/pkexec")));
        assert_eq!(pkexec_path(|_| true), Some(PathBuf::from("/usr/bin/pkexec")));
        assert_eq!(pkexec_path(|_| false), None);
    }

    #[test]
    fn long_lines_are_cut() {
        let text = format!("ok 0\r\n{}\ndone", "x".repeat(10_000));
        let mut lines = Vec::new();
        read_lines(std::io::Cursor::new(text), &mut |line| lines.push(line.to_owned()));
        assert_eq!(lines, ["ok 0".to_owned(), "x".repeat(MAX_LINE), "done".to_owned()]);
    }

    #[test]
    fn nothing_starts_without_a_plain_exe_and_a_fitting_list() {
        let ops = [Op::Mkdir(PathBuf::from("/opt/x"))];
        assert_eq!(ready(Path::new("/a\nb/gezik"), &ops), Err(LaunchError::Failed(NOT_PLAIN_EXE.into())));
        let many: Vec<Op> = (0..5000).map(|i| Op::Delete(PathBuf::from(format!("/opt/app/file number {i}")))).collect();
        let exe = std::env::temp_dir().join("gezik");
        assert_eq!(ready(&exe, &many), Err(LaunchError::Failed(elevated::TOO_MANY.into())));
        assert_eq!(ready(&exe, &ops), Ok(exe.to_str().unwrap()));
        assert_eq!(ready(Path::new("gezik"), &ops), Err(LaunchError::Failed(NOT_PLAIN_EXE.into())), "not a full path");
    }

    #[test]
    fn only_root_and_admins_may_change_the_way_to_gezik() {
        assert!(!open_to_others(0, 0, 0o755, false));
        assert!(open_to_others(501, 20, 0o755, true), "a user's own folder");
        assert!(open_to_others(0, 0, 0o777, false), "world-writable");
        assert!(!open_to_others(0, 80, 0o775, true), "/Applications: root:admin 775");
        assert!(open_to_others(0, 80, 0o775, false), "gid 80 is no admin group on Linux");
        assert!(open_to_others(0, 100, 0o775, false), "a group's write");
    }

    #[test]
    fn a_folder_of_the_users_own_is_open_to_others() {
        let dir = std::env::temp_dir().join(format!("gezik-elevate-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("gezik.exe");
        std::fs::write(&exe, "").unwrap();
        assert!(others_can_change(&exe));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(others_can_change(&exe), "unreadable counts as open");
    }

    #[cfg(windows)]
    #[test]
    fn system32_is_closed_to_others() {
        let cmd = PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32").join("cmd.exe");
        assert!(!others_can_change(&cmd), "{cmd:?}");
    }

    #[cfg(unix)]
    #[test]
    fn the_system_shell_is_closed_to_others() {
        assert!(!others_can_change(Path::new("/bin/sh")));
    }

    #[cfg(unix)]
    #[test]
    fn the_helper_writes_only_to_standard_output() {
        assert!(open_channel(elevated::STDOUT).is_ok());
        assert!(open_channel("/tmp/x").is_err());
    }

    #[cfg(windows)]
    mod windows {
        use super::super::imp::{create_pipe, serve};
        use super::*;
        use ::windows::Win32::Foundation::{CloseHandle, HANDLE};
        use ::windows::Win32::System::Threading::{
            OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
        };
        use std::os::windows::io::AsRawHandle;

        fn name() -> String {
            format!("{}{}", elevated::PIPE_PREFIX, crate::fs::secure::random_name())
        }

        /// What `serve` reads from a "helper" thread of this process; `early`: the helper has
        /// written and closed its end before Gezik starts waiting.
        fn through_pipe(early: bool) -> Result<Vec<String>, LaunchError> {
            let name = name();
            let pipe = create_pipe(&name).unwrap();
            let mut writer = Some(std::thread::spawn({
                let name = name.clone();
                move || {
                    let mut out = open_channel(&name).unwrap();
                    out.write_all(
                        b"ok 0
err 1 5 Access denied
done
",
                    )
                    .unwrap();
                }
            }));
            if early && let Some(writer) = writer.take() {
                writer.join().unwrap();
            }
            let mut lines = Vec::new();
            // A real handle: WaitForMultipleObjects fails on the pseudo handle.
            // SAFETY: this process, by id; closed below.
            let me = unsafe {
                OpenProcess(PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION, false, std::process::id())
            }
            .unwrap();
            let served = serve(&pipe, me, &mut |line| lines.push(line.to_owned()));
            // SAFETY: opened above, closed once.
            unsafe {
                let _ = CloseHandle(me);
            }
            if let Some(writer) = writer {
                writer.join().unwrap();
            }
            served.map(|()| lines)
        }

        #[test]
        fn the_helpers_lines_come_through_the_pipe() {
            assert_eq!(through_pipe(false).unwrap(), ["ok 0", "err 1 5 Access denied", "done"]);
        }

        #[test]
        fn a_helper_that_wrote_and_left_before_gezik_waited_is_read() {
            assert_eq!(through_pipe(true).unwrap(), ["ok 0", "err 1 5 Access denied", "done"]);
        }

        #[test]
        fn a_stranger_on_the_pipe_is_turned_away() {
            let name = name();
            let pipe = create_pipe(&name).unwrap();
            // The "helper": a short-lived child that never connects.
            let child = std::process::Command::new("cmd").args(["/c", "ping -n 3 127.0.0.1 >nul"]).spawn().unwrap();
            let stranger = std::thread::spawn({
                let name = name.clone();
                move || {
                    if let Ok(mut out) = open_channel(&name) {
                        let _ = out.write_all(b"ok 0\ndone\n");
                    }
                }
            });
            let mut lines = Vec::new();
            let ended = serve(&pipe, HANDLE(child.as_raw_handle()), &mut |line| lines.push(line.to_owned()));
            stranger.join().unwrap();
            assert_eq!(ended, Err(LaunchError::Failed(ENDED.into())));
            assert!(lines.is_empty(), "nothing the stranger wrote was read: {lines:?}");
        }

        #[test]
        fn a_second_pipe_of_the_same_name_is_refused() {
            let name = name();
            let _first = create_pipe(&name).unwrap();
            assert!(create_pipe(&name).is_err(), "FILE_FLAG_FIRST_PIPE_INSTANCE");
        }

        #[test]
        fn the_channel_name_is_checked() {
            assert!(open_channel(r"\\.\pipe\other").is_err());
            assert!(open_channel("-").is_err());
        }

        #[test]
        fn command_lines_read_back_through_windows() {
            use ::windows::Win32::Foundation::{HLOCAL, LocalFree};
            use ::windows::Win32::UI::Shell::CommandLineToArgvW;
            use ::windows::core::HSTRING;
            let args: Vec<String> = [
                r"C:\Program Files\App\x.dll",
                r"C:\a b\",
                r#"say "hi""#,
                r"\\",
                "",
                "ğüşİ € 😀",
                "tab\there",
                r#"\"quoted\""#,
                "trailing space ",
            ]
            .map(str::to_owned)
            .to_vec();
            let line = HSTRING::from(format!("gezik.exe {}", elevated::windows_command_line(&args)));
            let mut count = 0;
            // SAFETY: a NUL-ended line; the array is freed below.
            let argv = unsafe { CommandLineToArgvW(&line, &mut count) };
            assert!(!argv.is_null());
            let back: Vec<String> = (1..count as usize)
                // SAFETY: `count` entries, each a NUL-ended string.
                .map(|i| unsafe { (*argv.add(i)).to_string().unwrap() })
                .collect();
            // SAFETY: allocated by CommandLineToArgvW.
            unsafe {
                let _ = LocalFree(Some(HLOCAL(argv.cast())));
            }
            assert_eq!(back, args);
        }

        #[test]
        fn the_exe_can_be_held_against_renaming() {
            use std::os::windows::fs::OpenOptionsExt;
            let exe = std::env::current_exe().unwrap();
            let held = std::fs::OpenOptions::new().read(true).share_mode(1).open(&exe);
            assert!(held.is_ok(), "deviation 15: {held:?}");
            // A held exe still starts (what runas does after the prompt), and cannot be renamed.
            let dir = std::env::temp_dir().join(format!("gezik-held-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let copy = dir.join("held.exe");
            let cmd = PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32").join("cmd.exe");
            std::fs::copy(cmd, &copy).unwrap();
            let held = std::fs::OpenOptions::new().read(true).share_mode(1).open(&copy).unwrap();
            let ran = std::process::Command::new(&copy).args(["/c", "exit 7"]).status().unwrap();
            assert_eq!(ran.code(), Some(7));
            assert!(std::fs::rename(&copy, dir.join("other.exe")).is_err());
            drop(held);
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[test]
        fn the_protected_folders_start_with_windows() {
            let list = protected();
            assert!(list[0].to_string_lossy().to_lowercase().ends_with(r"\windows"), "{list:?}");
            assert!(list.iter().any(|p| p.ends_with("Program Files")));
        }
    }
}
