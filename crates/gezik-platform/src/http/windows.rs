//! Windows: WinHTTP, synchronous, with the system's (or the PAC script's) proxy and the system's
//! certificate store. Redirects are WinHTTP's own: never from https to http, at most 5.
//!
//! `WinHttpCrackUrl` gets no `ICU_ESCAPE`/`ICU_DECODE`: both need caller buffers for the pieces,
//! and the addresses are the manifest's, already escaped ASCII (`WinHttpOpenRequest` escapes
//! what is left by default).

use std::ffi::c_void;
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use windows::Win32::Networking::WinHttp::{
    ICU_REJECT_USERPWD, URL_COMPONENTS, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE,
    WINHTTP_INTERNET_SCHEME_HTTPS, WINHTTP_OPEN_REQUEST_FLAGS, WINHTTP_OPTION_MAX_HTTP_AUTOMATIC_REDIRECTS,
    WINHTTP_OPTION_REDIRECT_POLICY, WINHTTP_OPTION_REDIRECT_POLICY_DISALLOW_HTTPS_TO_HTTP, WINHTTP_OPTION_URL,
    WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE, WinHttpCloseHandle, WinHttpConnect, WinHttpCrackUrl,
    WinHttpOpen, WinHttpOpenRequest, WinHttpQueryHeaders, WinHttpQueryOption, WinHttpReadData, WinHttpReceiveResponse,
    WinHttpSendRequest, WinHttpSetOption, WinHttpSetTimeouts,
};
use windows::core::{PCWSTR, w};

use super::{MAX_REDIRECTS, Throttle, interrupted, too_big};

/// Name resolution, connecting and sending may take 30 s each, a pause in what arrives 60 s.
const CONNECT_MS: i32 = 30_000;
const RECEIVE_MS: i32 = 60_000;

/// How often the worker's progress and `stop` are looked at.
const POLL: Duration = Duration::from_millis(100);

/// How much is asked for per read.
const CHUNK: usize = 64 * 1024;

pub(super) fn download(
    url: &str,
    dest: &Path,
    max_bytes: u64,
    allow_http: bool,
    progress: &mut dyn FnMut(u64),
    stop: &dyn Fn() -> bool,
) -> io::Result<()> {
    let wide: Vec<u16> = url.encode_utf16().collect();
    let parts = Parts::crack(&wide)?;
    if !allow_http && !parts.secure {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "only https addresses are downloaded"));
    }
    if stop() {
        return Err(interrupted());
    }
    // The handles close in reverse order when they go out of scope (also on a stop).
    let session = Handle::new(unsafe {
        WinHttpOpen(w!("Gezik"), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, PCWSTR::null(), PCWSTR::null(), 0)
    })?;
    unsafe { WinHttpSetTimeouts(session.0, CONNECT_MS, CONNECT_MS, CONNECT_MS, RECEIVE_MS) }.map_err(failure)?;
    let connection = Handle::new(unsafe { WinHttpConnect(session.0, PCWSTR(parts.host.as_ptr()), parts.port, 0) })?;
    let flags = if parts.secure { WINHTTP_FLAG_SECURE } else { WINHTTP_OPEN_REQUEST_FLAGS(0) };
    let mut request = Handle::new(unsafe {
        WinHttpOpenRequest(
            connection.0,
            w!("GET"),
            PCWSTR(parts.object.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            flags,
        )
    })?;
    // Always set (it is the default, but nothing should change that): an https address never
    // ends up on http.
    request.set_u32(WINHTTP_OPTION_REDIRECT_POLICY, WINHTTP_OPTION_REDIRECT_POLICY_DISALLOW_HTTPS_TO_HTTP)?;
    request.set_u32(WINHTTP_OPTION_MAX_HTTP_AUTOMATIC_REDIRECTS, MAX_REDIRECTS)?;
    // The request runs on a worker thread; this one tells the progress and watches `stop`.
    // A stop closes the request handle, which aborts a synchronous call in progress on it
    // (a stalled server would otherwise hold the worker for up to the 60 s timeout).
    let raw = request.0 as usize;
    let received = AtomicU64::new(0);
    let mut throttle = Throttle::new(progress);
    // Set before the handle is closed: the worker looks at it before each WinHTTP call, so it
    // does not start a new call with the closed handle's value, which a handle opened
    // meanwhile (another download) may have been given. A call already under way when the
    // handle closes is safe: WinHTTP aborts it and keeps its object until it returns. What is
    // left is the instant between the worker's look and the start of its call; a stray call
    // there would at worst disturb another download in the same moment, whose SHA-256 check
    // would then fail.
    let stopping = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let (sender, receiver) = mpsc::channel();
        let (received, stopping) = (&received, &stopping);
        scope.spawn(move || {
            let call = Call { request: raw as *mut c_void, stopping };
            let _ = sender.send(transfer(&call, dest, max_bytes, allow_http, received));
        });
        loop {
            match receiver.recv_timeout(POLL) {
                Ok(result) => return result,
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(io::Error::other("the download stopped unexpectedly"));
                }
            }
            if stop() {
                stopping.store(true, Ordering::SeqCst);
                request.close();
                // The worker ends with an error now; whatever it was, this was a stop. The
                // scope waits for it, so the file is closed before it is removed.
                let _ = receiver.recv();
                return Err(interrupted());
            }
            throttle.update(received.load(Ordering::Relaxed));
        }
    })
}

/// The request handle as the worker uses it: another thread may close it to abort the
/// download, after it set `stopping`.
struct Call<'a> {
    request: *mut c_void,
    stopping: &'a AtomicBool,
}

impl Call<'_> {
    /// The handle for the next WinHTTP call; an error once the download is being stopped (the
    /// handle is closed then, or about to be).
    fn handle(&self) -> io::Result<*mut c_void> {
        if self.stopping.load(Ordering::SeqCst) { Err(interrupted()) } else { Ok(self.request) }
    }
}

/// Sends the request (a handle another thread may close to abort it), checks the answer and
/// reads the body into `dest`, counting it in `received`.
fn transfer(call: &Call<'_>, dest: &Path, max_bytes: u64, allow_http: bool, received: &AtomicU64) -> io::Result<()> {
    unsafe { WinHttpSendRequest(call.handle()?, None, None, 0, 0, 0) }.map_err(failure)?;
    unsafe { WinHttpReceiveResponse(call.handle()?, std::ptr::null_mut()) }.map_err(failure)?;
    // Where the redirects led, checked once more.
    if !allow_http && !final_url(call)?.to_ascii_lowercase().starts_with("https://") {
        return Err(io::Error::other("the download was redirected to an insecure address"));
    }
    let status = status(call.handle()?)?;
    if status != 200 {
        return Err(io::Error::other(format!("the server answered {status}")));
    }
    let mut file = std::fs::File::create(dest)?;
    let mut buf = vec![0u8; CHUNK];
    let mut total = 0u64;
    loop {
        let mut read = 0u32;
        unsafe { WinHttpReadData(call.handle()?, buf.as_mut_ptr().cast(), CHUNK as u32, &mut read) }
            .map_err(failure)?;
        if read == 0 {
            break;
        }
        total += u64::from(read);
        if total > max_bytes {
            return Err(too_big());
        }
        file.write_all(&buf[..read as usize])?;
        received.store(total, Ordering::Relaxed);
    }
    file.sync_all()
}

pub(super) fn tool_missing_hint() -> Option<String> {
    None
}

/// The pieces of an address WinHTTP is given, each NUL-terminated.
struct Parts {
    secure: bool,
    host: Vec<u16>,
    port: u16,
    /// The path with the query.
    object: Vec<u16>,
}

impl Parts {
    /// WinHTTP's own reading of `url` (user names and passwords in it are refused).
    fn crack(url: &[u16]) -> io::Result<Parts> {
        let mut parts = URL_COMPONENTS {
            dwStructSize: size_of::<URL_COMPONENTS>() as u32,
            // A length and no buffer: the piece is pointed at inside `url`.
            dwSchemeLength: u32::MAX,
            dwHostNameLength: u32::MAX,
            dwUrlPathLength: u32::MAX,
            dwExtraInfoLength: u32::MAX,
            ..Default::default()
        };
        unsafe { WinHttpCrackUrl(url, ICU_REJECT_USERPWD.0, &mut parts) }.map_err(failure)?;
        let piece = |at: windows::core::PWSTR, len: u32| -> Vec<u16> {
            let mut text = if at.is_null() || len == 0 {
                Vec::new()
            } else {
                unsafe { std::slice::from_raw_parts(at.0, len as usize) }.to_vec()
            };
            text.push(0);
            text
        };
        let host = piece(parts.lpszHostName, parts.dwHostNameLength);
        let mut object = piece(parts.lpszUrlPath, parts.dwUrlPathLength);
        object.pop();
        object.extend(piece(parts.lpszExtraInfo, parts.dwExtraInfoLength));
        if host.len() <= 1 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "the address has no server"));
        }
        if object.len() <= 1 {
            object = vec![u16::from(b'/'), 0];
        }
        Ok(Parts { secure: parts.nScheme == WINHTTP_INTERNET_SCHEME_HTTPS, host, port: parts.nPort, object })
    }
}

/// A WinHTTP handle, closed when dropped.
struct Handle(*mut c_void);

impl Handle {
    /// `raw` from a call that gives null on failure.
    fn new(raw: *mut c_void) -> io::Result<Handle> {
        if raw.is_null() {
            let code = io::Error::last_os_error().raw_os_error().unwrap_or(0);
            Err(described(code as u32))
        } else {
            Ok(Handle(raw))
        }
    }

    fn set_u32(&self, option: u32, value: u32) -> io::Result<()> {
        unsafe { WinHttpSetOption(Some(self.0), option, Some(&value.to_ne_bytes())) }.map_err(failure)
    }

    /// Closes it now (once; the drop then does nothing).
    fn close(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
            self.0 = std::ptr::null_mut();
        }
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        self.close();
    }
}

/// The response's status code.
fn status(request: *mut c_void) -> io::Result<u32> {
    let mut status = 0u32;
    let mut len = size_of::<u32>() as u32;
    unsafe {
        WinHttpQueryHeaders(
            request,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some((&raw mut status).cast()),
            &mut len,
            std::ptr::null_mut(),
        )
    }
    .map_err(failure)?;
    Ok(status)
}

/// The address the response came from, after redirects.
fn final_url(call: &Call<'_>) -> io::Result<String> {
    let mut len = 0u32;
    // The first call only says how many bytes it needs.
    let _ = unsafe { WinHttpQueryOption(call.handle()?, WINHTTP_OPTION_URL, None, &mut len) };
    let mut buf = vec![0u16; (len as usize).div_ceil(2) + 1];
    let mut size = (buf.len() * 2) as u32;
    unsafe { WinHttpQueryOption(call.handle()?, WINHTTP_OPTION_URL, Some(buf.as_mut_ptr().cast()), &mut size) }
        .map_err(failure)?;
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Ok(String::from_utf16_lossy(&buf[..end]))
}

/// A WinHTTP call's error in words.
fn failure(err: windows::core::Error) -> io::Error {
    // A Win32 code comes as HRESULT 0x8007xxxx.
    described((err.code().0 as u32) & 0xFFFF)
}

/// WinHTTP's codes (12001-12192) have no text in the system's message table: the common ones
/// are put in words here.
fn described(code: u32) -> io::Error {
    let text = match code {
        12002 => "the server did not answer in time",
        12005 | 12006 => "the address is not valid",
        12007 => "the server's name could not be found",
        12029 => "could not connect to the server",
        12030 => "the connection to the server was lost",
        12152 => "the server's answer was not understood",
        12156 => "the server redirected the download to a place it may not go",
        12037 => "the server's certificate has expired",
        12038 => "the server's certificate is for another name",
        12045 => "the server's certificate is not trusted",
        12157 | 12175 => "a secure connection to the server could not be made",
        12180 | 12166 | 12167 | 12178 => "the proxy settings could not be read",
        _ => return io::Error::other(format!("the download failed (WinHTTP error {code})")),
    };
    io::Error::other(text)
}
