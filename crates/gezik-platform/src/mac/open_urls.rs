//! macOS: folders LaunchServices hands to Gezik (spec 6.2, decision 16): `application:openURLs:`
//! added to winit's app delegate class, once, before the event loop runs.

use std::cell::RefCell;
use std::path::PathBuf;

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
use objc2::sel;
use objc2_app_kit::NSApplication;
use objc2_foundation::{NSArray, NSURL};

type OnOpen = Box<dyn Fn(Vec<PathBuf>)>;

thread_local! {
    static ON_OPEN: RefCell<Option<OnOpen>> = const { RefCell::new(None) };
}

extern "C-unwind" fn open_urls(_this: &AnyObject, _sel: Sel, _app: &AnyObject, urls: &NSArray<NSURL>) {
    let paths: Vec<PathBuf> = (0..urls.count())
        .filter_map(|i| urls.objectAtIndex(i).path())
        .map(|path| PathBuf::from(path.to_string()))
        .collect();
    ON_OPEN.with(|f| {
        if let Some(f) = f.borrow().as_ref() {
            f(paths);
        }
    });
}

/// Calls `on_open` (on the main thread) with the folders macOS asks Gezik to open. The latest
/// call's `on_open` wins. If winit's delegate already answers the selector, it keeps it.
pub fn install(on_open: OnOpen) {
    let first = ON_OPEN.with(|f| f.borrow_mut().replace(on_open).is_none());
    if !first {
        return;
    }
    let Some(mtm) = MainThreadMarker::new() else { return };
    let Some(delegate) = NSApplication::sharedApplication(mtm).delegate() else { return };
    // SAFETY: the delegate is alive (retained here); every Objective-C object has a class.
    let class: &AnyClass = unsafe { &*Retained::as_ptr(&delegate).cast::<AnyObject>() }.class();
    type OpenUrls = extern "C-unwind" fn(&AnyObject, Sel, &AnyObject, &NSArray<NSURL>);
    // SAFETY: the function matches `application:openURLs:` (void; self, _cmd, NSApplication,
    // NSArray) and its type encoding.
    unsafe {
        let imp = std::mem::transmute::<*const (), Imp>(open_urls as OpenUrls as *const ());
        objc2::ffi::class_addMethod(
            class as *const AnyClass as *mut AnyClass,
            sel!(application:openURLs:),
            imp,
            c"v@:@@".as_ptr(),
        );
    }
}
