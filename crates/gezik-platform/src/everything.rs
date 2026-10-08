//! Everything's IPC (spec 3.6), Windows only: its hidden window is found by class, a query goes
//! to it with `WM_COPYDATA`, and the answer comes back to a message-only window of the asking
//! thread. No DLL, no crate; the message formats are gezik-search's (plain data, tested).
//! Elsewhere every call says Everything is not running.

#[cfg(not(windows))]
use std::sync::atomic::AtomicBool;
#[cfg(not(windows))]
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EverythingError {
    NotRunning,
    /// Running, its database still loading.
    NotReady,
    NoAnswer,
    /// The search was stopped while it waited.
    Cancelled,
    Failed(String),
}

/// Everything 1.4's window class, then 1.5 alpha's.
pub const WINDOW_CLASSES: [&str; 2] = ["EVERYTHING_TASKBAR_NOTIFICATION", "EVERYTHING_TASKBAR_NOTIFICATION_(1.5a)"];

#[cfg(windows)]
pub use windows_ipc::{query, ready};

#[cfg(not(windows))]
pub fn ready(_timeout: Duration) -> Result<(), EverythingError> {
    Err(EverythingError::NotRunning)
}

#[cfg(not(windows))]
pub fn query(
    _build: &dyn Fn(u32, u32) -> Vec<u8>,
    _cancel: &AtomicBool,
    _timeout: Duration,
) -> Result<Vec<u8>, EverythingError> {
    Err(EverythingError::NotRunning)
}

#[cfg(windows)]
mod windows_ipc {
    use std::cell::{Cell, RefCell};
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::DataExchange::COPYDATASTRUCT;
    use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
    use windows::Win32::UI::WindowsAndMessaging::{
        ChangeWindowMessageFilterEx, CreateWindowExW, DestroyWindow, DispatchMessageW, FindWindowW, HWND_MESSAGE, MSG,
        MSGFLT_ALLOW, MsgWaitForMultipleObjects, PM_REMOVE, PeekMessageW, QS_ALLINPUT, SMTO_ABORTIFHUNG, SMTO_BLOCK,
        SendMessageTimeoutW, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WM_COPYDATA, WM_USER,
    };
    use windows::core::{HSTRING, PCWSTR, w};

    use super::{EverythingError, WINDOW_CLASSES};

    /// `EVERYTHING_IPC_IS_DB_LOADED`, sent with `WM_USER`.
    const IS_DB_LOADED: usize = 401;
    /// `EVERYTHING_IPC_COPYDATA_QUERY2W`.
    const COPYDATA_QUERY2W: usize = 18;
    /// The `dwData` Everything's answer carries (ours to choose).
    const REPLY_ID: usize = 0x6765_7A6B;
    const SUBCLASS_ID: usize = 0x6576_7279;

    thread_local! {
        /// The first answer to this thread's query; later ones are dropped.
        static REPLY: RefCell<Option<Vec<u8>>> = const { RefCell::new(None) };
        /// The window this thread's query went to: only its answer is taken (`wParam`).
        static ASKED: Cell<usize> = const { Cell::new(0) };
    }

    fn millis(time: Duration) -> u32 {
        u32::try_from(time.as_millis()).unwrap_or(u32::MAX)
    }

    /// Everything's window, by the first of `classes` found.
    pub(super) fn find(classes: &[&str]) -> Option<HWND> {
        classes.iter().find_map(|class| {
            // SAFETY: a class name and no window name.
            let hwnd = unsafe { FindWindowW(&HSTRING::from(*class), PCWSTR::null()) }.ok()?;
            (!hwnd.is_invalid()).then_some(hwnd)
        })
    }

    /// Whether Everything runs with its database loaded.
    pub fn ready(timeout: Duration) -> Result<(), EverythingError> {
        let everything = find(&WINDOW_CLASSES).ok_or(EverythingError::NotRunning)?;
        let mut loaded = 0usize;
        // SAFETY: a plain message with numbers; Everything answers with a number.
        let sent = unsafe {
            SendMessageTimeoutW(
                everything,
                WM_USER,
                WPARAM(IS_DB_LOADED),
                LPARAM(0),
                SMTO_ABORTIFHUNG | SMTO_BLOCK,
                millis(timeout),
                Some(&mut loaded),
            )
        };
        if sent.0 == 0 {
            return Err(EverythingError::NoAnswer);
        }
        if loaded == 0 { Err(EverythingError::NotReady) } else { Ok(()) }
    }

    /// Forgets the window asked when the query ends (a late answer is then dropped).
    struct Asked;

    impl Drop for Asked {
        fn drop(&mut self) {
            ASKED.set(0);
        }
    }

    /// A message-only window of this thread that keeps Everything's answer.
    struct ReplyWindow(HWND);

    impl ReplyWindow {
        fn new() -> Result<ReplyWindow, EverythingError> {
            // SAFETY: a system class ("STATIC"), the message-only parent.
            let hwnd = unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    w!("STATIC"),
                    PCWSTR::null(),
                    WINDOW_STYLE(0),
                    0,
                    0,
                    0,
                    0,
                    Some(HWND_MESSAGE),
                    None,
                    None,
                    None,
                )
            }
            .map_err(|err| EverythingError::Failed(err.to_string()))?;
            let window = ReplyWindow(hwnd);
            // SAFETY: our window; an Everything running as administrator may answer it (UIPI).
            unsafe {
                let _ = ChangeWindowMessageFilterEx(hwnd, WM_COPYDATA, MSGFLT_ALLOW, None);
                if !SetWindowSubclass(hwnd, Some(reply_messages), SUBCLASS_ID, 0).as_bool() {
                    return Err(EverythingError::Failed("cannot receive Everything's answer".to_owned()));
                }
            }
            Ok(window)
        }
    }

    impl Drop for ReplyWindow {
        fn drop(&mut self) {
            // SAFETY: made by `new` on this thread.
            unsafe {
                let _ = DestroyWindow(self.0);
            }
        }
    }

    unsafe extern "system" fn reply_messages(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        _data: usize,
    ) -> LRESULT {
        if msg == WM_COPYDATA && lparam.0 != 0 {
            // SAFETY: WM_COPYDATA's lParam is a COPYDATASTRUCT valid during the call.
            let data = unsafe { &*(lparam.0 as *const COPYDATASTRUCT) };
            if data.dwData == REPLY_ID && !data.lpData.is_null() && wparam.0 == ASKED.get() && ASKED.get() != 0 {
                REPLY.with(|reply| {
                    let mut reply = reply.borrow_mut();
                    if reply.is_none() {
                        // SAFETY: `cbData` bytes at `lpData`, valid during the call.
                        let bytes =
                            unsafe { std::slice::from_raw_parts(data.lpData as *const u8, data.cbData as usize) };
                        *reply = Some(bytes.to_vec());
                    }
                });
                return LRESULT(1);
            }
        }
        // SAFETY: the default for every other message.
        unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
    }

    /// Sends the query `build` makes (from our window and our answer's id) and waits up to
    /// `timeout` for the answer, handling this thread's messages meanwhile; `cancel` is looked
    /// at every 50 ms. Only an answer from the window asked (its `wParam`) counts, and only the
    /// first.
    pub fn query(
        build: &dyn Fn(u32, u32) -> Vec<u8>,
        cancel: &AtomicBool,
        timeout: Duration,
    ) -> Result<Vec<u8>, EverythingError> {
        if cancel.load(Ordering::Relaxed) {
            return Err(EverythingError::Cancelled);
        }
        let everything = find(&WINDOW_CLASSES).ok_or(EverythingError::NotRunning)?;
        let reply = ReplyWindow::new()?;
        // Window handles fit in 32 bits (Everything's QUERY2 takes a DWORD).
        let data = build(reply.0.0 as usize as u32, REPLY_ID as u32);
        let copy = COPYDATASTRUCT {
            dwData: COPYDATA_QUERY2W,
            cbData: u32::try_from(data.len()).map_err(|_| EverythingError::Failed("query too long".to_owned()))?,
            lpData: data.as_ptr() as *mut c_void,
        };
        REPLY.with(|reply| reply.borrow_mut().take());
        ASKED.set(everything.0 as usize);
        let _asked = Asked;
        let started = Instant::now();
        // SAFETY: `copy` and `data` live through the call (WM_COPYDATA copies them over).
        let sent = unsafe {
            SendMessageTimeoutW(
                everything,
                WM_COPYDATA,
                WPARAM(reply.0.0 as usize),
                LPARAM(&copy as *const COPYDATASTRUCT as isize),
                SMTO_ABORTIFHUNG,
                millis(timeout),
                None,
            )
        };
        if sent.0 == 0 {
            return Err(EverythingError::NoAnswer);
        }
        loop {
            let mut msg = MSG::default();
            // SAFETY: this thread's queue; sent messages (the answer) are handled inside.
            while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
                unsafe {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
            if let Some(bytes) = REPLY.with(|reply| reply.borrow_mut().take()) {
                return Ok(bytes);
            }
            if cancel.load(Ordering::Relaxed) {
                return Err(EverythingError::Cancelled);
            }
            let left = timeout.saturating_sub(started.elapsed());
            if left.is_zero() {
                return Err(EverythingError::NoAnswer);
            }
            // SAFETY: waits for this thread's messages only.
            unsafe {
                let _ = MsgWaitForMultipleObjects(None, false, millis(left).min(50), QS_ALLINPUT);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    #[test]
    fn an_unknown_window_class_is_not_running() {
        assert_eq!(super::windows_ipc::find(&["GEZIK_NO_SUCH_WINDOW_CLASS"]), None);
    }

    #[cfg(windows)]
    #[test]
    fn a_stopped_search_asks_nothing() {
        let stopped = std::sync::atomic::AtomicBool::new(true);
        let asked = std::cell::Cell::new(false);
        let build = |_, _| {
            asked.set(true);
            Vec::new()
        };
        let answer = super::query(&build, &stopped, std::time::Duration::from_secs(1));
        assert_eq!(answer, Err(super::EverythingError::Cancelled));
        assert!(!asked.get());
    }

    #[cfg(not(windows))]
    #[test]
    fn elsewhere_it_never_runs() {
        assert_eq!(super::ready(std::time::Duration::ZERO), Err(super::EverythingError::NotRunning));
    }
}
