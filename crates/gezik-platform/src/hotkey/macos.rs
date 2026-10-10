//! macOS: Carbon's RegisterEventHotKey on the application event target (no Accessibility
//! permission; spec 9.2); the handler runs on the main thread. Linked against Carbon.framework
//! (deviation 19; AppKit loads its HIToolbox anyway). No thread of its own.

use std::cell::RefCell;
use std::ffi::c_void;
use std::ptr::null_mut;
use std::rc::Rc;

use objc2::MainThreadMarker;

use crate::hotkey::{Combo, HotkeyError, OnPress, OnReady, carbon_keycode, carbon_modifiers};

type OsStatus = i32;
type Handler = extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> OsStatus;

#[repr(C)]
struct EventTypeSpec {
    event_class: u32,
    event_kind: u32,
}

#[repr(C)]
struct EventHotKeyId {
    signature: u32,
    id: u32,
}

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn GetApplicationEventTarget() -> *mut c_void;
    fn InstallEventHandler(
        target: *mut c_void,
        handler: Handler,
        count: u32,
        types: *const EventTypeSpec,
        user_data: *mut c_void,
        out: *mut *mut c_void,
    ) -> OsStatus;
    fn RemoveEventHandler(handler: *mut c_void) -> OsStatus;
    fn RegisterEventHotKey(
        code: u32,
        modifiers: u32,
        id: EventHotKeyId,
        target: *mut c_void,
        options: u32,
        out: *mut *mut c_void,
    ) -> OsStatus;
    fn UnregisterEventHotKey(hotkey: *mut c_void) -> OsStatus;
}

/// kEventClassKeyboard, kEventHotKeyPressed.
const KEYBOARD: u32 = u32::from_be_bytes(*b"keyb");
const HOT_KEY_PRESSED: u32 = 5;
const SIGNATURE: u32 = u32::from_be_bytes(*b"Gzik");
/// eventHotKeyExistsErr.
const EXISTS: OsStatus = -9878;

thread_local! {
    /// The one shortcut's press (the app drops the old Hotkey before it registers a new one).
    /// An Rc, so the call below holds no borrow while it runs.
    static ON_PRESS: RefCell<Option<Rc<OnPress>>> = const { RefCell::new(None) };
}

extern "C" fn pressed(_call: *mut c_void, _event: *mut c_void, _data: *mut c_void) -> OsStatus {
    // Cloned out first: a Hotkey dropped inside the call must not meet a held borrow.
    if let Some(f) = ON_PRESS.with(|f| f.borrow().clone()) {
        f();
    }
    0
}

pub struct Hotkey {
    hotkey: *mut c_void,
    handler: *mut c_void,
}

pub fn register(combo: Combo, on_press: OnPress, on_ready: OnReady) -> Hotkey {
    let none = Hotkey { hotkey: null_mut(), handler: null_mut() };
    if MainThreadMarker::new().is_none() {
        on_ready(Err(HotkeyError::Failed("a shortcut is registered from the main thread only".to_owned())));
        return none;
    }
    let Some(code) = carbon_keycode(combo.key) else {
        on_ready(Err(HotkeyError::Failed("this key has no place on a Mac keyboard".to_owned())));
        return none;
    };
    let spec = EventTypeSpec { event_class: KEYBOARD, event_kind: HOT_KEY_PRESSED };
    let (mut handler, mut hotkey) = (null_mut(), null_mut());
    // SAFETY: Carbon's documented calls on the main thread; both refs are released in Drop.
    let status =
        unsafe { InstallEventHandler(GetApplicationEventTarget(), pressed, 1, &spec, null_mut(), &mut handler) };
    if status != 0 {
        on_ready(Err(HotkeyError::Failed(format!("Carbon error {status}"))));
        return none;
    }
    let id = EventHotKeyId { signature: SIGNATURE, id: 1 };
    // SAFETY: as above.
    let status =
        unsafe { RegisterEventHotKey(code, carbon_modifiers(&combo), id, GetApplicationEventTarget(), 0, &mut hotkey) };
    if status != 0 {
        // SAFETY: the handler installed above.
        unsafe {
            RemoveEventHandler(handler);
        }
        on_ready(Err(if status == EXISTS {
            HotkeyError::Taken
        } else {
            HotkeyError::Failed(format!("macOS refused this shortcut (error {status})"))
        }));
        return none;
    }
    ON_PRESS.with(|f| *f.borrow_mut() = Some(Rc::new(on_press)));
    on_ready(Ok(()));
    Hotkey { hotkey, handler }
}

impl Drop for Hotkey {
    fn drop(&mut self) {
        if self.hotkey.is_null() {
            return;
        }
        // SAFETY: the refs `register` got, each released once.
        unsafe {
            UnregisterEventHotKey(self.hotkey);
            RemoveEventHandler(self.handler);
        }
        ON_PRESS.with(|f| f.borrow_mut().take());
    }
}
