//! macOS: the Info window's "Open with" (spec 9 §4.4): the app for this one file, and for every
//! file of its type ("Change All…").

use std::ffi::c_void;
use std::path::Path;

use block2::RcBlock;
use objc2::rc::{Retained, autoreleasepool};
use objc2_app_kit::NSWorkspace;
use objc2_foundation::{NSBundle, NSError, NSString, NSURL};

#[link(name = "CoreServices", kind = "framework")]
unsafe extern "C" {
    // Deprecated in macOS 12 but working: NSWorkspace's replacement takes a UTType, which needs
    // objc2-uniform-type-identifiers 0.3 (not in the lock; 9a1 deviation 1).
    fn LSSetDefaultRoleHandlerForContentType(content_type: *const c_void, role: u32, handler: *const c_void) -> i32;
}

/// `kLSRolesAll`.
const ALL_ROLES: u32 = 0xFFFF_FFFF;

fn url_of(path: &Path) -> Result<Retained<NSURL>, String> {
    let text = path.to_str().ok_or("The path is not valid UTF-8")?;
    Ok(NSURL::fileURLWithPath(&NSString::from_str(text)))
}

/// Opens `file` (only it) with `app` from now on (macOS 12+). `on_error` is called later, on
/// another thread, if the system refused.
pub fn set_for_file(app: &Path, file: &Path, on_error: Box<dyn FnOnce(String) + Send>) -> Result<(), String> {
    autoreleasepool(|_| {
        let (app_url, file_url) = (url_of(app)?, url_of(file)?);
        // The block may be called once, on any queue: the callback is taken out of a cell.
        let on_error = std::sync::Mutex::new(Some(on_error));
        let done = RcBlock::new(move |error: *mut NSError| {
            // SAFETY: AppKit passes a valid NSError or null.
            if let Some(error) = unsafe { error.as_ref() }
                && let Some(report) = on_error.lock().ok().and_then(|mut f| f.take())
            {
                report(error.localizedDescription().to_string());
            }
        });
        NSWorkspace::sharedWorkspace().setDefaultApplicationAtURL_toOpenFileAtURL_completionHandler(
            &app_url,
            &file_url,
            Some(&done),
        );
        Ok(())
    })
}

/// Opens every file of `file`'s type with `app` ("Change All…").
pub fn set_for_type(app: &Path, file: &Path) -> Result<(), String> {
    let kind = crate::mac::services::type_of(file).ok_or("Its type is not known")?;
    autoreleasepool(|_| {
        let app_url = url_of(app)?;
        let bundle = NSBundle::bundleWithURL(&app_url).ok_or("It is not an app")?;
        let id = bundle.bundleIdentifier().ok_or("The app has no bundle identifier")?;
        let kind = NSString::from_str(&kind);
        // SAFETY: NSString is toll-free bridged to CFString; both live through the call.
        let status = unsafe {
            LSSetDefaultRoleHandlerForContentType(
                Retained::as_ptr(&kind).cast(),
                ALL_ROLES,
                Retained::as_ptr(&id).cast(),
            )
        };
        if status == 0 { Ok(()) } else { Err(format!("LaunchServices refused it ({status})")) }
    })
}
