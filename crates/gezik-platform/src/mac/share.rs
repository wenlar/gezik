//! macOS: Finder's Share… through NSSharingServicePicker (spec 9 §4.3).

use std::path::PathBuf;

use objc2::MainThreadMarker;
use objc2::rc::{Retained, autoreleasepool};
use objc2_app_kit::{NSSharingServicePicker, NSView};
use objc2_foundation::{NSArray, NSPoint, NSRect, NSRectEdge, NSSize, NSString, NSURL};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

use crate::finder::{anchor, view_point};

thread_local! {
    /// The picker last shown, kept alive while its popover may still be up.
    static PICKER: std::cell::RefCell<Option<Retained<NSSharingServicePicker>>> = const { std::cell::RefCell::new(None) };
}

pub fn share(window: &impl HasWindowHandle, paths: &[PathBuf], at: Option<(f64, f64)>) -> Result<(), String> {
    let mtm = MainThreadMarker::new().ok_or("not on the main thread")?;
    let handle = window.window_handle().map_err(|e| e.to_string())?;
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else { return Err("not an AppKit window".into()) };
    autoreleasepool(|_| {
        // SAFETY: winit's view lives as long as its window, which outlives this call.
        let view: Retained<NSView> =
            unsafe { Retained::retain(appkit.ns_view.as_ptr().cast::<NSView>()) }.ok_or("no view")?;
        let urls: Vec<Retained<NSURL>> =
            paths.iter().filter_map(|p| p.to_str()).map(|p| NSURL::fileURLWithPath(&NSString::from_str(p))).collect();
        if urls.is_empty() {
            return Err("Nothing can be shared".to_owned());
        }
        let size = view.bounds().size;
        let flipped = view.isFlipped();
        let point = match at {
            Some(at) => view_point(at, size.height, flipped),
            None => {
                // The pointer in top-left terms, the middle when it is outside, back in the view's.
                let pointer =
                    view.window().map(|w| view.convertPoint_fromView(w.mouseLocationOutsideOfEventStream(), None));
                let pointer = pointer.map(|p| view_point((p.x, p.y), size.height, flipped));
                view_point(anchor(pointer, (size.width, size.height)), size.height, flipped)
            }
        };
        // SAFETY: an array of NSURLs is an array of objects, and the picker takes
        // NSPasteboardWriting objects, which NSURLs are.
        let items: Retained<NSArray> = unsafe { Retained::cast_unchecked(NSArray::from_retained_slice(&urls)) };
        let picker = unsafe { NSSharingServicePicker::initWithItems(mtm.alloc(), &items) };
        let rect = NSRect::new(NSPoint::new(point.0, point.1), NSSize::new(1.0, 1.0));
        picker.showRelativeToRect_ofView_preferredEdge(rect, &view, NSRectEdge::MinY);
        PICKER.with(|kept| *kept.borrow_mut() = Some(picker));
        Ok(())
    })
}
