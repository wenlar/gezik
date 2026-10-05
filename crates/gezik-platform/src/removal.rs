//! Windows: says when the drive a folder is on is about to be removed ("Safely Remove
//! Hardware", ejecting a USB stick), so that Gezik lets go of it. An open handle on the drive
//! (watching the folder is one) makes Windows refuse with "the device is in use"; Windows asks
//! only the windows that registered such a handle, and only they can let go in time.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    DBT_DEVICEQUERYREMOVE, DBT_DEVTYP_HANDLE, DEV_BROADCAST_HANDLE, DEV_BROADCAST_HDR, DEVICE_NOTIFY_WINDOW_HANDLE,
    HDEVNOTIFY, RegisterDeviceNotificationW, UnregisterDeviceNotification, WM_DEVICECHANGE,
};

use crate::fs::verbatim;

const SUBCLASS_ID: usize = 0x6765_7A72;

type Callback = Rc<dyn Fn()>;

thread_local! {
    /// The callbacks of the live watches, by the handle Windows names in its message.
    static WATCHES: RefCell<HashMap<isize, Callback>> = RefCell::new(HashMap::new());
    /// The windows whose messages are seen already.
    static WINDOWS: RefCell<Vec<isize>> = const { RefCell::new(Vec::new()) };
}

/// Told when the drive `folder` is on is about to be removed, while this lives (it keeps a
/// handle on the folder: the callback must drop it).
pub struct RemovalWatch {
    handle: HANDLE,
    registration: HDEVNOTIFY,
}

impl Drop for RemovalWatch {
    fn drop(&mut self) {
        WATCHES.with(|w| w.borrow_mut().remove(&(self.handle.0 as isize)));
        unsafe {
            let _ = UnregisterDeviceNotification(self.registration);
            let _ = CloseHandle(self.handle);
        }
    }
}

/// Calls `on_asked` (on this, the window's thread) when the drive of `folder` is about to be
/// removed: it must let go of everything on the drive before it returns, this watch included
/// (once dropped, nothing more is told: whether the drive went is for the caller to see).
/// `None` if it cannot be watched. Call on the UI thread.
pub fn watch_removal(
    window: &impl HasWindowHandle,
    folder: &Path,
    on_asked: impl Fn() + 'static,
) -> Option<RemovalWatch> {
    let RawWindowHandle::Win32(raw) = window.window_handle().ok()?.as_raw() else { return None };
    let hwnd = HWND(raw.hwnd.get() as *mut _);
    see_messages(hwnd);
    unsafe {
        let handle = CreateFileW(
            &verbatim(folder),
            FILE_READ_ATTRIBUTES.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            None,
        )
        .ok()?;
        let filter = DEV_BROADCAST_HANDLE {
            dbch_size: size_of::<DEV_BROADCAST_HANDLE>() as u32,
            dbch_devicetype: DBT_DEVTYP_HANDLE.0,
            dbch_handle: handle,
            ..Default::default()
        };
        let Ok(registration) =
            RegisterDeviceNotificationW(HANDLE(hwnd.0), (&raw const filter).cast(), DEVICE_NOTIFY_WINDOW_HANDLE)
        else {
            let _ = CloseHandle(handle);
            return None;
        };
        WATCHES.with(|w| w.borrow_mut().insert(handle.0 as isize, Rc::new(on_asked)));
        Some(RemovalWatch { handle, registration })
    }
}

/// Lets `device_messages` see the window's WM_DEVICECHANGE, once per window.
fn see_messages(hwnd: HWND) {
    let new = WINDOWS.with(|w| {
        let mut windows = w.borrow_mut();
        let key = hwnd.0 as isize;
        let new = !windows.contains(&key);
        if new {
            windows.push(key);
        }
        new
    });
    if new && !unsafe { SetWindowSubclass(hwnd, Some(device_messages), SUBCLASS_ID, 0) }.as_bool() {
        eprintln!("gezik: SetWindowSubclass failed; removing a drive may say it is in use");
    }
}

unsafe extern "system" fn device_messages(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    if msg == WM_DEVICECHANGE
        && wparam.0 as u32 == DBT_DEVICEQUERYREMOVE
        && lparam.0 != 0
        && unsafe { &*(lparam.0 as *const DEV_BROADCAST_HDR) }.dbch_devicetype == DBT_DEVTYP_HANDLE
    {
        let handle = unsafe { &*(lparam.0 as *const DEV_BROADCAST_HANDLE) }.dbch_handle;
        // Taken out first: the callback drops the watch, which removes it.
        let callback = WATCHES.with(|w| w.borrow().get(&(handle.0 as isize)).cloned());
        if let Some(callback) = callback {
            callback();
        }
        // TRUE: Gezik does not object.
        return LRESULT(1);
    }
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}
