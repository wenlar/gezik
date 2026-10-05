//! macOS: `NSURLSession`, with the system's proxy settings and certificate trust.
//!
//! A download task writes the body to a temporary file of its own and calls the completion
//! handler with it; the file is deleted as soon as the handler returns, so the handler moves it
//! to `dest` itself. Meanwhile this thread waits on a channel, looking every 100 ms at
//! `countOfBytesReceived` (the progress, and the size limit) and at `stop` (then `cancel`).
//!
//! Only https is asked for (the address is checked before; App Transport Security also refuses
//! plain http by default), and the address the answer finally came from is checked again
//! after redirects. The redirect count is NSURLSession's own (no delegate is set up to count
//! them): the hash check of what was downloaded is what guards the content.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use block2::RcBlock;
use objc2_foundation::{
    NSError, NSHTTPURLResponse, NSString, NSURL, NSURLRequest, NSURLResponse, NSURLSession, NSURLSessionConfiguration,
};

use super::{Throttle, interrupted, too_big};

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
    let session = NSURLSession::sessionWithConfiguration(&config);
    let request = NSURLRequest::requestWithURL(&address);

    // The handler runs once, on the session's own queue: it sends what came of it.
    let (sender, receiver) = mpsc::channel::<io::Result<()>>();
    let target = dest.to_path_buf();
    let handler = RcBlock::new(move |location: *mut NSURL, response: *mut NSURLResponse, error: *mut NSError| {
        // Safety: the pointers are null or valid for the call.
        let result =
            unsafe { finish(location.as_ref(), response.as_ref(), error.as_ref(), &target, max_bytes, allow_http) };
        let _ = sender.send(result);
    });
    // Safety: the handler only holds a channel sender and a path, both fine on any thread.
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
        return Err(io::Error::other("the download was redirected away from https"));
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
