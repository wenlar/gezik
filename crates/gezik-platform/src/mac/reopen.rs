//! macOS: a Dock click shows Gezik's hidden window (9b9 deviation 8):
//! `applicationShouldHandleReopen:hasVisibleWindows:` added to winit's app delegate class,
//! once, as open_urls.rs adds `application:openURLs:`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Bool, Imp, Sel};
use objc2::sel;
use objc2_app_kit::NSApplication;

type OnReopen = Box<dyn Fn()>;

thread_local! {
    static ON_REOPEN: RefCell<Option<Rc<OnReopen>>> = const { RefCell::new(None) };
    /// Whether the method went in, once the delegate was there to take it.
    static ADDED: Cell<Option<bool>> = const { Cell::new(None) };
}

extern "C-unwind" fn reopen(_this: &AnyObject, _sel: Sel, _app: &AnyObject, _visible: Bool) -> Bool {
    // Cloned out first: `install` called inside the call must not meet a held borrow.
    if let Some(f) = ON_REOPEN.with(|f| f.borrow().clone()) {
        f();
    }
    Bool::YES
}

/// Calls `on_reopen` (on the main thread) when the Dock icon is clicked; the latest call's
/// `on_reopen` wins. Whether the method went in: false when winit's delegate answers it
/// already (then a Dock click does only what winit does; the menu bar icon and the shortcut
/// still show the window), or off the main thread or before winit set its delegate.
pub fn install(on_reopen: OnReopen) -> bool {
    let Some(mtm) = MainThreadMarker::new() else { return false };
    ON_REOPEN.with(|f| *f.borrow_mut() = Some(Rc::new(on_reopen)));
    if let Some(added) = ADDED.get() {
        return added;
    }
    let app = NSApplication::sharedApplication(mtm);
    let Some(delegate) = app.delegate() else { return false };
    // SAFETY: the delegate is alive (retained here); every Objective-C object has a class.
    let class: &AnyClass = unsafe { &*Retained::as_ptr(&delegate).cast::<AnyObject>() }.class();
    type Reopen = extern "C-unwind" fn(&AnyObject, Sel, &AnyObject, Bool) -> Bool;
    // BOOL is `bool` ('B') on Apple silicon and `signed char` ('c') on Intel.
    let types = if cfg!(target_arch = "aarch64") { c"B@:@B" } else { c"c@:@c" };
    // SAFETY: the function matches `applicationShouldHandleReopen:hasVisibleWindows:` (BOOL;
    // self, _cmd, NSApplication, BOOL) and its type encoding.
    let added = unsafe {
        let imp = std::mem::transmute::<*const (), Imp>(reopen as Reopen as *const ());
        objc2::ffi::class_addMethod(
            class as *const AnyClass as *mut AnyClass,
            sel!(applicationShouldHandleReopen:hasVisibleWindows:),
            imp,
            types.as_ptr(),
        )
    };
    ADDED.set(Some(added.as_bool()));
    if added.as_bool() {
        // AppKit caches which optional methods a delegate answers when it is set: set it again.
        app.setDelegate(Some(&delegate));
    }
    added.as_bool()
}
