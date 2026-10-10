//! macOS: ⌘Q, the Dock's Quit and signing out end the app by `terminate:`, which never asks
//! the window to close; `applicationShouldTerminate:` is added to winit's app delegate class
//! (as reopen.rs adds its method) so Gezik saves first. The app then quits as asked.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
use objc2::sel;
use objc2_app_kit::NSApplication;

type OnQuit = Box<dyn Fn()>;

thread_local! {
    static ON_QUIT: RefCell<Option<Rc<OnQuit>>> = const { RefCell::new(None) };
    static ADDED: Cell<Option<bool>> = const { Cell::new(None) };
}

/// NSTerminateNow.
const TERMINATE_NOW: usize = 1;

extern "C-unwind" fn should_terminate(_this: &AnyObject, _sel: Sel, _app: &AnyObject) -> usize {
    if let Some(f) = ON_QUIT.with(|f| f.borrow().clone()) {
        f();
    }
    TERMINATE_NOW
}

/// Calls `on_quit` (on the main thread) just before the app is terminated; it must finish its
/// work before it returns. False when the method could not go in (winit answers it already,
/// off the main thread, or before winit set its delegate).
pub fn install(on_quit: OnQuit) -> bool {
    let Some(mtm) = MainThreadMarker::new() else { return false };
    ON_QUIT.with(|f| *f.borrow_mut() = Some(Rc::new(on_quit)));
    if let Some(added) = ADDED.get() {
        return added;
    }
    let app = NSApplication::sharedApplication(mtm);
    let Some(delegate) = app.delegate() else { return false };
    // SAFETY: the delegate is alive (retained here); every Objective-C object has a class.
    let class: &AnyClass = unsafe { &*Retained::as_ptr(&delegate).cast::<AnyObject>() }.class();
    type ShouldTerminate = extern "C-unwind" fn(&AnyObject, Sel, &AnyObject) -> usize;
    // SAFETY: the function matches `applicationShouldTerminate:` (NSApplicationTerminateReply,
    // an NSUInteger: 'Q'; self, _cmd, NSApplication) and its type encoding.
    let added = unsafe {
        let imp = std::mem::transmute::<*const (), Imp>(should_terminate as ShouldTerminate as *const ());
        objc2::ffi::class_addMethod(
            class as *const AnyClass as *mut AnyClass,
            sel!(applicationShouldTerminate:),
            imp,
            c"Q@:@".as_ptr(),
        )
    };
    ADDED.set(Some(added.as_bool()));
    if added.as_bool() {
        // AppKit caches which optional methods a delegate answers when it is set: set it again.
        app.setDelegate(Some(&delegate));
    }
    added.as_bool()
}
