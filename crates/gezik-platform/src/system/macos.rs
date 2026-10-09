//! macOS: the default app for folders and NSFileViewer (spec 6.2; 9b4 decision 15).

use std::ffi::c_void;
use std::io;
use std::path::Path;

use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::NSObject;
use objc2_foundation::{NSString, NSURL};

#[link(name = "CoreServices", kind = "framework")]
unsafe extern "C" {
    // Deprecated in macOS 12 but working (as in mac/info.rs; 9a1 deviation 1).
    fn LSCopyDefaultRoleHandlerForContentType(content_type: *const c_void, role: u32) -> *const c_void;
    fn LSSetDefaultRoleHandlerForContentType(content_type: *const c_void, role: u32, handler: *const c_void) -> i32;
    fn LSRegisterURL(url: *const c_void, update: u8) -> i32;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFPreferencesAnyApplication: *const c_void;
    fn CFPreferencesCopyAppValue(key: *const c_void, app: *const c_void) -> *const c_void;
    fn CFPreferencesSetAppValue(key: *const c_void, value: *const c_void, app: *const c_void);
    fn CFPreferencesAppSynchronize(app: *const c_void) -> u8;
}

/// `kLSRolesAll`.
const ALL_ROLES: u32 = 0xFFFF_FFFF;

/// A +1 CF object from a Copy call, if it is a string.
fn take_string(raw: *const c_void) -> Option<String> {
    // SAFETY: a +1 CF object or null; NSObject is toll-free bridged; taken over once.
    let object = unsafe { Retained::from_raw(raw as *mut NSObject) }?;
    object.downcast::<NSString>().ok().map(|s| s.to_string())
}

pub fn default_handler(content_type: &str) -> Option<String> {
    autoreleasepool(|_| {
        let kind = NSString::from_str(content_type);
        // SAFETY: the NSString lives across the call.
        take_string(unsafe { LSCopyDefaultRoleHandlerForContentType(Retained::as_ptr(&kind).cast(), ALL_ROLES) })
    })
}

pub fn set_default_handler(content_type: &str, bundle_id: &str) -> io::Result<()> {
    autoreleasepool(|_| {
        let (kind, id) = (NSString::from_str(content_type), NSString::from_str(bundle_id));
        // SAFETY: both live across the call.
        let status = unsafe {
            LSSetDefaultRoleHandlerForContentType(
                Retained::as_ptr(&kind).cast(),
                ALL_ROLES,
                Retained::as_ptr(&id).cast(),
            )
        };
        if status == 0 { Ok(()) } else { Err(io::Error::other(format!("LaunchServices refused it ({status})"))) }
    })
}

/// The global domain's `key` (`defaults read -g key`).
pub fn global_pref(key: &str) -> Option<String> {
    autoreleasepool(|_| {
        let key = NSString::from_str(key);
        // SAFETY: the key lives across the call; the domain is a constant.
        take_string(unsafe { CFPreferencesCopyAppValue(Retained::as_ptr(&key).cast(), kCFPreferencesAnyApplication) })
    })
}

/// Sets (or with `None` removes) the global domain's `key`.
pub fn set_global_pref(key: &str, value: Option<&str>) -> io::Result<()> {
    autoreleasepool(|_| {
        let key = NSString::from_str(key);
        let value = value.map(NSString::from_str);
        let raw = value.as_ref().map_or(std::ptr::null(), |v| Retained::as_ptr(v).cast());
        // SAFETY: all live across the calls.
        let synced = unsafe {
            CFPreferencesSetAppValue(Retained::as_ptr(&key).cast(), raw, kCFPreferencesAnyApplication);
            CFPreferencesAppSynchronize(kCFPreferencesAnyApplication)
        };
        if synced != 0 { Ok(()) } else { Err(io::Error::other("the preferences could not be saved")) }
    })
}

pub fn register_app(app: &Path) {
    let Some(text) = app.to_str() else { return };
    autoreleasepool(|_| {
        let url = NSURL::fileURLWithPath(&NSString::from_str(text));
        // SAFETY: NSURL is toll-free bridged to CFURL and lives across the call.
        unsafe { LSRegisterURL(Retained::as_ptr(&url).cast(), 1) };
    });
}
