//! macOS: Quick Actions and Services installed as bundles (`~/Library/Services`,
//! `/Library/Services`), the type of a file, and running a service (spec 9 §4.3).

use std::ffi::c_void;
use std::path::{Path, PathBuf};

use objc2::msg_send;
use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2_app_kit::{NSPasteboard, NSPasteboardWriting, NSPerformService};
#[allow(deprecated, reason = "NSURLContentTypeKey gives a UTType, which needs objc2-uniform-type-identifiers")]
use objc2_foundation::NSURLTypeIdentifierKey;
use objc2_foundation::{NSArray, NSDictionary, NSString, NSURL};

use crate::services::{Service, file_types};

#[link(name = "CoreServices", kind = "framework")]
unsafe extern "C" {
    /// Deprecated but present; CFStringRef arguments (NSString is toll-free bridged).
    fn UTTypeConformsTo(in_uti: *const c_void, in_conforms_to_uti: *const c_void) -> u8;
}

pub fn conforms(item: &str, to: &str) -> bool {
    autoreleasepool(|_| {
        let (a, b) = (NSString::from_str(item), NSString::from_str(to));
        // SAFETY: two live NSStrings passed as CFStringRefs.
        unsafe { UTTypeConformsTo(Retained::as_ptr(&a).cast(), Retained::as_ptr(&b).cast()) != 0 }
    })
}

/// `path`'s type (UTI), e.g. `public.png`, `public.folder`.
#[allow(deprecated, reason = "NSURLContentTypeKey gives a UTType, which needs objc2-uniform-type-identifiers")]
pub fn type_of(path: &Path) -> Option<String> {
    autoreleasepool(|_| {
        let url = NSURL::fileURLWithPath(&NSString::from_str(path.to_str()?));
        let mut value: Option<Retained<AnyObject>> = None;
        // SAFETY: the type identifier's value is an NSString.
        unsafe { url.getResourceValue_forKey_error(&mut value, NSURLTypeIdentifierKey) }.ok()?;
        Some(value?.downcast::<NSString>().ok()?.to_string())
    })
}

fn strings(value: Option<Retained<AnyObject>>) -> Vec<String> {
    let Some(array) = value.and_then(|v| v.downcast::<NSArray>().ok()) else { return Vec::new() };
    array.iter().filter_map(|s| s.downcast::<NSString>().ok()).map(|s| s.to_string()).collect()
}

/// The file-taking services of one bundle's `Contents/Info.plist`.
fn services_of(bundle: &Path) -> Vec<Service> {
    let Some(plist) = bundle.join("Contents/Info.plist").to_str().map(NSString::from_str) else { return Vec::new() };
    let url = NSURL::fileURLWithPath(&plist);
    // SAFETY: a property list's dictionary has string keys; its values are only downcast.
    let Ok(info) = (unsafe { NSDictionary::<NSString, AnyObject>::dictionaryWithContentsOfURL_error(&url) }) else {
        return Vec::new();
    };
    let Some(entries) = info.objectForKey(&NSString::from_str("NSServices")).and_then(|v| v.downcast::<NSArray>().ok())
    else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|entry| entry.downcast::<NSDictionary>().ok())
        .filter_map(|entry| {
            let get = |key: &str| entry.objectForKey(&*Retained::into_super(NSString::from_str(key)));
            let item = get("NSMenuItem")?.downcast::<NSDictionary>().ok()?;
            let title = item
                .objectForKey(&*Retained::into_super(NSString::from_str("default")))?
                .downcast::<NSString>()
                .ok()?
                .to_string();
            let types = file_types(strings(get("NSSendFileTypes")), &strings(get("NSSendTypes")));
            (!title.is_empty() && !types.is_empty()).then_some(Service { title, file_types: types })
        })
        .collect()
}

/// Every file-taking service in the user's and the computer's Services folders.
pub fn installed() -> Vec<Service> {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    [home.join("Library/Services"), PathBuf::from("/Library/Services")]
        .iter()
        .filter_map(|dir| std::fs::read_dir(dir).ok())
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|p| p.extension().is_some_and(|e| e == "workflow" || e == "service"))
        .flat_map(|bundle| autoreleasepool(|_| services_of(&bundle)))
        .collect()
}

pub fn perform(title: &str, paths: &[PathBuf]) -> Result<(), String> {
    autoreleasepool(|_| {
        let urls: Vec<Retained<NSURL>> =
            paths.iter().filter_map(|p| p.to_str()).map(|p| NSURL::fileURLWithPath(&NSString::from_str(p))).collect();
        if urls.is_empty() {
            return Err("no item can be passed to it".to_owned());
        }
        let writers: Vec<&ProtocolObject<dyn NSPasteboardWriting>> =
            urls.iter().map(|u| ProtocolObject::from_ref(&**u)).collect();
        let board = NSPasteboard::pasteboardWithUniqueName();
        board.clearContents();
        let written = board.writeObjects(&NSArray::from_slice(&writers));
        let ran = written && NSPerformService(&NSString::from_str(title), Some(&board));
        // Not in objc2-app-kit 0.3: frees the unique pasteboard in the pasteboard server.
        let _: () = unsafe { msg_send![&*board, releaseGlobally] };
        if ran { Ok(()) } else { Err("it did not run".to_owned()) }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn types_and_conformance_come_from_the_system() {
        let file = std::env::temp_dir().join(format!("gezik-services-{}.png", std::process::id()));
        std::fs::write(&file, b"x").unwrap();
        let png = type_of(&file).unwrap();
        assert_eq!(png, "public.png");
        assert!(conforms(&png, "public.image") && conforms(&png, "public.item") && !conforms(&png, "public.folder"));
        assert_eq!(type_of(&std::env::temp_dir()).as_deref().map(|t| conforms(t, "public.folder")), Some(true));
        let _ = std::fs::remove_file(&file);
        let _ = installed(); // reads whatever is installed without failing
    }
}
