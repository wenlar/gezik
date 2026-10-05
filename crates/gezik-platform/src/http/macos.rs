//! macOS: `NSURLSession`, with the system's proxy settings and certificate trust.
//!
//! A download task writes the body to a temporary file of its own and calls the completion
//! handler with it; the file is deleted as soon as the handler returns, so the handler moves it
//! to `dest` itself. Meanwhile this thread waits on a channel, looking every 100 ms at
//! `countOfBytesReceived` (the progress, and the size limit) and at `stop` (then `cancel`).
//!
//! Only https is asked for: the address is checked before, and a session delegate
//! (`RedirectGuard`) sees each redirect before it is followed, refusing a non-https one (unless
//! `allow_http`) and a sixth one. App Transport Security refuses plain http by default too,
//! and the address the answer finally came from is checked once more.
//!
//! The timeout is NSURLSession's request timeout (the longest pause in what arrives, 60 s);
//! there is no separate connect timeout in its API.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use block2::{DynBlock, RcBlock};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{AllocAnyThread, DefinedClass, define_class, msg_send};
use objc2_foundation::{
    NSError, NSHTTPURLResponse, NSObject, NSObjectProtocol, NSString, NSURL, NSURLRequest, NSURLResponse, NSURLSession,
    NSURLSessionConfiguration, NSURLSessionDelegate, NSURLSessionTask, NSURLSessionTaskDelegate,
};

use super::{MAX_REDIRECTS, Throttle, interrupted, too_big};

/// Why `RedirectGuard` refused a redirect (`REFUSED_*`), or `FOLLOWED`.
const FOLLOWED: u8 = 0;
const REFUSED_INSECURE: u8 = 1;
const REFUSED_TOO_MANY: u8 = 2;

/// What one download's `RedirectGuard` knows.
struct Guard {
    allow_http: bool,
    hops: AtomicU32,
    /// Shared with the completion handler, which says why the download ended at a redirect.
    refused: Arc<AtomicU8>,
}

define_class!(
    // Safety: NSObject has no subclassing requirements, and this class has no Drop.
    #[unsafe(super(NSObject))]
    #[name = "GezikRedirectGuard"]
    #[ivars = Guard]
    struct RedirectGuard;

    unsafe impl NSObjectProtocol for RedirectGuard {}

    unsafe impl NSURLSessionDelegate for RedirectGuard {}

    unsafe impl NSURLSessionTaskDelegate for RedirectGuard {
        /// Called (on the session's queue) before each redirect: `handler(new request)`
        /// follows it, `handler(nil)` stops there (the task then ends with the redirect
        /// response, which the completion handler refuses).
        #[unsafe(method(URLSession:task:willPerformHTTPRedirection:newRequest:completionHandler:))]
        fn will_redirect(
            &self,
            _session: &NSURLSession,
            _task: &NSURLSessionTask,
            _response: &NSHTTPURLResponse,
            request: &NSURLRequest,
            handler: &DynBlock<dyn Fn(*mut NSURLRequest)>,
        ) {
            let guard = self.ivars();
            let hops = guard.hops.fetch_add(1, Ordering::SeqCst) + 1;
            let scheme = request.URL().and_then(|url| url.scheme()).map(|s| s.to_string().to_ascii_lowercase());
            let secure = match scheme.as_deref() {
                Some("https") => true,
                Some("http") => guard.allow_http,
                _ => false,
            };
            let verdict = if !secure {
                REFUSED_INSECURE
            } else if hops > MAX_REDIRECTS {
                REFUSED_TOO_MANY
            } else {
                FOLLOWED
            };
            if verdict == FOLLOWED {
                handler.call((request as *const NSURLRequest as *mut NSURLRequest,));
            } else {
                guard.refused.store(verdict, Ordering::SeqCst);
                handler.call((std::ptr::null_mut(),));
            }
        }
    }
);

impl RedirectGuard {
    fn new(allow_http: bool, refused: Arc<AtomicU8>) -> Retained<RedirectGuard> {
        let this = Self::alloc().set_ivars(Guard { allow_http, hops: AtomicU32::new(0), refused });
        unsafe { msg_send![super(this), init] }
    }
}

/// How often the task is looked at.
const POLL: Duration = Duration::from_millis(100);

/// The longest pause in what arrives, in seconds.
const TIMEOUT: f64 = 60.0;

pub(super) fn download(
    url: &str,
    dest: &Path,
    max_bytes: u64,
    allow_http: bool,
    progress: &mut dyn FnMut(u64),
    stop: &dyn Fn() -> bool,
) -> io::Result<()> {
    let Some(address) = NSURL::URLWithString(&NSString::from_str(url)) else {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "the address is not valid"));
    };
    // Ephemeral: no cookies or cache written to disk for this.
    let config = NSURLSessionConfiguration::ephemeralSessionConfiguration();
    config.setTimeoutIntervalForRequest(TIMEOUT);
    let refused = Arc::new(AtomicU8::new(FOLLOWED));
    let guard = RedirectGuard::new(allow_http, refused.clone());
    // Safety: no queue given: the session makes its own serial one, where the delegate and the
    // handler run. The session keeps the delegate until it is invalidated below.
    let session = unsafe {
        NSURLSession::sessionWithConfiguration_delegate_delegateQueue(
            &config,
            Some(ProtocolObject::from_ref(&*guard)),
            None,
        )
    };
    let request = NSURLRequest::requestWithURL(&address);

    // The handler runs once, on the session's own queue: it sends what came of it.
    let (sender, receiver) = mpsc::channel::<io::Result<()>>();
    let target = dest.to_path_buf();
    let handler = RcBlock::new(move |location: *mut NSURL, response: *mut NSURLResponse, error: *mut NSError| {
        // Safety: the pointers are null or valid for the call.
        let result = match refused.load(Ordering::SeqCst) {
            REFUSED_INSECURE => Err(io::Error::other("the download was redirected to an insecure address")),
            REFUSED_TOO_MANY => Err(io::Error::other("the download was redirected too many times")),
            _ => unsafe {
                finish(location.as_ref(), response.as_ref(), error.as_ref(), &target, max_bytes, allow_http)
            },
        };
        let _ = sender.send(result);
    });
    // Safety: the handler only holds a channel sender, a path and an atomic, all fine on any
    // thread.
    let task = unsafe { session.downloadTaskWithRequest_completionHandler(&request, &handler) };
    task.resume();

    let mut throttle = Throttle::new(progress);
    let result = loop {
        match receiver.recv_timeout(POLL) {
            Ok(result) => break result,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break Err(io::Error::other("the download ended without an answer")),
        }
        let received = u64::try_from(task.countOfBytesReceived()).unwrap_or(0);
        let stopped = stop();
        if stopped || received > max_bytes {
            task.cancel();
            // The handler still runs (with a "cancelled" error): once it did, nothing writes
            // `dest` any more.
            let _ = receiver.recv();
            break Err(if stopped { interrupted() } else { too_big() });
        }
        throttle.update(received);
    };
    // A session holds on to itself until invalidated.
    session.finishTasksAndInvalidate();
    result
}

pub(super) fn tool_missing_hint() -> Option<String> {
    None
}

/// What the completion handler got, checked; the temporary file at `location` is moved to
/// `dest`.
fn finish(
    location: Option<&NSURL>,
    response: Option<&NSURLResponse>,
    error: Option<&NSError>,
    dest: &Path,
    max_bytes: u64,
    allow_http: bool,
) -> io::Result<()> {
    if let Some(error) = error {
        return Err(io::Error::other(error.localizedDescription().to_string()));
    }
    let Some(response) = response else { return Err(io::Error::other("the server gave no answer")) };
    // Where the redirects led.
    let scheme = response.URL().and_then(|url| url.scheme()).map(|scheme| scheme.to_string().to_ascii_lowercase());
    if !allow_http && scheme.as_deref() != Some("https") {
        return Err(io::Error::other("the download was redirected to an insecure address"));
    }
    if let Some(http) = response.downcast_ref::<NSHTTPURLResponse>() {
        let status = http.statusCode();
        if status != 200 {
            return Err(io::Error::other(format!("the server answered {status}")));
        }
    }
    let Some(from) = location.and_then(|url| url.path()).map(|path| PathBuf::from(path.to_string())) else {
        return Err(io::Error::other("the download left no file"));
    };
    if std::fs::metadata(&from)?.len() > max_bytes {
        return Err(too_big());
    }
    // The temporary folder may be on another volume than `dest`: then it is copied.
    if std::fs::rename(&from, dest).is_err() {
        std::fs::copy(&from, dest)?;
    }
    Ok(())
}
