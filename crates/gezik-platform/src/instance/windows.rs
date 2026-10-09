//! The single-instance channel on Windows: a named pipe whose name holds the user's SID and
//! the session (spec 5.2), open to this user only, never to other machines; the caller checks
//! that the pipe's server is this user's and gives it leave to come to the front.

use std::fs::{File, OpenOptions};
use std::io;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

use ::windows::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, HANDLE, HLOCAL,
    HWND, LocalFree,
};
use ::windows::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use ::windows::Win32::Security::{
    GetTokenInformation, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenSessionId, TokenUser,
};
use ::windows::Win32::Storage::FileSystem::{
    FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX, SECURITY_IDENTIFICATION,
};
use ::windows::Win32::System::IO::CancelSynchronousIo;
use ::windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, GetNamedPipeServerProcessId, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
    PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT, WaitNamedPipeW,
};
use ::windows::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};
use ::windows::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, IsIconic, SW_RESTORE, SetForegroundWindow, ShowWindow,
};
use ::windows::core::{HSTRING, PWSTR};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

use super::{Claim, SEND_TIMEOUT};

pub(super) struct Listener {
    /// The instance waiting for the next call.
    pipe: File,
    name: HSTRING,
    sid: String,
}

/// This process's user (its SID as text) and session.
fn identity() -> Option<(String, u32)> {
    let mut token = HANDLE::default();
    // SAFETY: the pseudo handle of this process; `token` is written by the call.
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }.ok()?;
    let found = user_and_session(token);
    // SAFETY: the token was opened above.
    unsafe {
        let _ = CloseHandle(token);
    }
    found
}

/// The user's SID (as text) and the session of an open token.
fn user_and_session(token: HANDLE) -> Option<(String, u32)> {
    let mut size = 0u32;
    // SAFETY: a size query: no buffer.
    unsafe {
        let _ = GetTokenInformation(token, TokenUser, None, 0, &mut size);
    }
    // u64s: TOKEN_USER wants pointer alignment.
    let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
    // SAFETY: the buffer holds `size` bytes.
    unsafe { GetTokenInformation(token, TokenUser, Some(buffer.as_mut_ptr().cast()), size, &mut size) }.ok()?;
    // SAFETY: filled by the call as a TOKEN_USER, aligned.
    let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    let mut text = PWSTR::null();
    // SAFETY: the SID lives in `buffer`; the text is freed below.
    unsafe { ConvertSidToStringSidW(user.User.Sid, &mut text) }.ok()?;
    // SAFETY: a NUL-ended string from the call.
    let sid = unsafe { text.to_string() }.ok();
    // SAFETY: allocated by ConvertSidToStringSidW.
    unsafe {
        let _ = LocalFree(Some(HLOCAL(text.0.cast())));
    }
    let (mut session, mut len) = (0u32, 0u32);
    // SAFETY: a u32 is what TokenSessionId writes.
    unsafe { GetTokenInformation(token, TokenSessionId, Some((&raw mut session).cast()), 4, &mut len) }.ok()?;
    Some((sid?, session))
}

fn pipe_name(sid: &str, session: u32, key: &str) -> String {
    format!(r"\\.\pipe\gezik-{sid}-{session}-{key}")
}

/// A pipe instance only `sid` may open, never from another machine; `first` fails with
/// ERROR_ACCESS_DENIED when another process has the name.
fn create(name: &HSTRING, sid: &str, first: bool) -> io::Result<File> {
    let sddl = HSTRING::from(format!("D:P(A;;GA;;;{sid})"));
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    // SAFETY: `descriptor` is written by the call and freed below.
    unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(&sddl, SDDL_REVISION_1, &mut descriptor, None) }
        .map_err(io::Error::other)?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: false.into(),
    };
    let mut open = PIPE_ACCESS_DUPLEX;
    if first {
        open |= FILE_FLAG_FIRST_PIPE_INSTANCE;
    }
    // SAFETY: the name and attributes live through the call.
    let handle = unsafe {
        CreateNamedPipeW(
            name,
            open,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            PIPE_UNLIMITED_INSTANCES,
            4096,
            4096,
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

pub(super) fn claim(key: &str) -> Claim {
    let Some((sid, session)) = identity() else { return Claim::Off };
    let name = HSTRING::from(pipe_name(&sid, session, key));
    match create(&name, &sid, true) {
        Ok(pipe) => Claim::Listening(super::Listener(Listener { pipe, name, sid })),
        Err(err) if err.raw_os_error() == Some(ERROR_ACCESS_DENIED.0 as i32) => Claim::Taken,
        Err(_) => Claim::Off,
    }
}

/// The running Gezik's pipe, if one is there; refused if its server is not this user's.
/// It may come to the front from now on (only a process in front may give that leave).
pub(super) fn connect(key: &str, timeout: Duration) -> io::Result<Option<File>> {
    let Some((sid, session)) = identity() else { return Ok(None) };
    let name = pipe_name(&sid, session, key);
    // SECURITY_IDENTIFICATION: the server learns who calls but cannot act as the caller.
    let open = || OpenOptions::new().read(true).write(true).security_qos_flags(SECURITY_IDENTIFICATION.0).open(&name);
    let pipe = match open() {
        Ok(pipe) => pipe,
        Err(err) if err.raw_os_error() == Some(ERROR_FILE_NOT_FOUND.0 as i32) => return Ok(None),
        Err(err) if err.raw_os_error() == Some(ERROR_PIPE_BUSY.0 as i32) => {
            // SAFETY: a plain name; no pointers kept.
            let _ = unsafe { WaitNamedPipeW(&HSTRING::from(name.as_str()), timeout.as_millis() as u32) };
            open()?
        }
        Err(err) => return Err(err),
    };
    let mut server = 0u32;
    // SAFETY: the handle is open.
    unsafe { GetNamedPipeServerProcessId(HANDLE(pipe.as_raw_handle()), &mut server) }.map_err(io::Error::other)?;
    if !is_users(server, &sid) {
        return Err(io::Error::from(io::ErrorKind::PermissionDenied));
    }
    // SAFETY: no pointers; it may fail (not in front), then the window only flashes.
    unsafe {
        let _ = AllowSetForegroundWindow(server);
    }
    Ok(Some(pipe))
}

/// Whether process `pid` runs as the user `sid` (else another user took the name first).
fn is_users(pid: u32, sid: &str) -> bool {
    // SAFETY: query rights only; the handles are closed below.
    let Ok(process) = (unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }) else { return false };
    let mut token = HANDLE::default();
    // SAFETY: `process` is open; `token` is written by the call.
    let theirs =
        unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) }.ok().and_then(|()| user_and_session(token));
    // SAFETY: opened above (a null token is not closed).
    unsafe {
        if !token.is_invalid() {
            let _ = CloseHandle(token);
        }
        let _ = CloseHandle(process);
    }
    theirs.is_some_and(|(theirs, _)| theirs == sid)
}

impl Listener {
    pub(super) fn serve(self, answer: impl Fn(&mut File) + Send + Sync + 'static) {
        let answer = Arc::new(answer);
        let _ = thread::Builder::new().name("gezik-instance".into()).spawn(move || {
            let Listener { mut pipe, name, sid } = self;
            loop {
                // SAFETY: the handle is open; blocks until a call comes (no CPU meanwhile).
                let connected = unsafe { ConnectNamedPipe(HANDLE(pipe.as_raw_handle()), None) };
                if let Err(err) = connected
                    && err.code() != ERROR_PIPE_CONNECTED.to_hresult()
                {
                    // A caller gone before it was seen: wait on a fresh instance.
                    match create(&name, &sid, false) {
                        Ok(fresh) => pipe = fresh,
                        Err(_) => return,
                    }
                    continue;
                }
                // The next instance first, so the name never goes away while this call is answered.
                let Ok(next) = create(&name, &sid, false) else { return };
                let mut call = std::mem::replace(&mut pipe, next);
                let answer = answer.clone();
                let (done, finished) = mpsc::channel::<()>();
                let Ok(worker) = thread::Builder::new().name("gezik-instance-call".into()).spawn(move || {
                    answer(&mut call);
                    // Waits until the caller has read the answer (FlushFileBuffers).
                    let _ = call.sync_all();
                    drop(done);
                }) else {
                    continue;
                };
                // A pipe File has no timeouts: this cancels the call's blocked read, write or
                // flush, so a silent or hung caller cannot hold its thread.
                let _ = thread::Builder::new().name("gezik-instance-watch".into()).spawn(move || {
                    while finished.recv_timeout(SEND_TIMEOUT) == Err(mpsc::RecvTimeoutError::Timeout) {
                        // SAFETY: `worker` keeps the thread's handle open; with no I/O pending
                        // the call does nothing.
                        unsafe {
                            let _ = CancelSynchronousIo(HANDLE(worker.as_raw_handle()));
                        }
                    }
                });
            }
        });
    }
}

/// Brings Gezik's window to the front, restored if minimized: the caller gave the leave
/// (spec 5.2). Not winit's `focus_window`: it presses a fake Alt key.
pub fn bring_to_front(window: &impl HasWindowHandle) {
    let Ok(handle) = window.window_handle() else { return };
    let RawWindowHandle::Win32(raw) = handle.as_raw() else { return };
    let hwnd = HWND(raw.hwnd.get() as *mut _);
    // SAFETY: Gezik's own live window.
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let _ = SetForegroundWindow(hwnd);
    }
}
