//! macOS: files dropped on winit's view reach Gezik's handler (with their position and the
//! keys held) through Gezik's own `NSDraggingDestination` methods, put in place of winit's;
//! a drag leaving the window becomes an `NSDraggingSession` with the files' URLs.

use std::cell::RefCell;
use std::ffi::c_char;
use std::path::PathBuf;
use std::rc::Rc;

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, Imp, NSObject, NSObjectProtocol, ProtocolObject, Sel};
use objc2::{AllocAnyThread, ClassType, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSDragOperation, NSDraggingContext, NSDraggingInfo, NSDraggingItem, NSDraggingSession,
    NSDraggingSource, NSEvent, NSEventModifierFlags, NSPasteboardTypeFileURL, NSPasteboardWriting, NSView, NSWorkspace,
};
use objc2_foundation::{NSArray, NSPoint, NSRect, NSSize, NSString, NSURL};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

use super::{Allowed, DragEnd, DropHandler, Effect, Keys, Offer, OnEnd};

mod promise;

/// What the replaced methods need: Gezik's handler and the offer over the view now.
struct Target {
    handler: Rc<dyn DropHandler>,
    offer: Option<Offer>,
}

thread_local! {
    static TARGET: RefCell<Option<Target>> = const { RefCell::new(None) };
    /// Called when Gezik's drag session ends.
    static ON_END: RefCell<Option<OnEnd>> = const { RefCell::new(None) };
}

/// The view's drop target, until dropped.
pub struct Registration {
    view: Retained<NSView>,
}

impl Drop for Registration {
    fn drop(&mut self) {
        // At exit another thread-local may drop this after TARGET is gone: nothing to clear then.
        let _ = TARGET.try_with(|t| t.borrow_mut().take());
    }
}

/// The NSDragOperation for an effect. A move is told as "generic": Gezik moves the files
/// itself, and a source told "move" would delete the originals.
fn operation_for(effect: Option<Effect>) -> NSDragOperation {
    match effect {
        Some(Effect::Copy) => NSDragOperation::Copy,
        Some(Effect::Move) => NSDragOperation::Generic,
        Some(Effect::Link) => NSDragOperation::Link,
        None => NSDragOperation::None,
    }
}

fn allowed_by(mask: NSDragOperation) -> Allowed {
    Allowed {
        copy: mask.contains(NSDragOperation::Copy),
        move_: mask.contains(NSDragOperation::Move) || mask.contains(NSDragOperation::Generic),
        link: mask.contains(NSDragOperation::Link),
    }
}

fn keys_now() -> Keys {
    let flags = NSEvent::modifierFlags_class();
    gezik_core::drag::keys_of(
        gezik_core::drag::DragOs::Mac,
        flags.contains(NSEventModifierFlags::Shift),
        flags.contains(NSEventModifierFlags::Control),
        flags.contains(NSEventModifierFlags::Option),
        flags.contains(NSEventModifierFlags::Command),
    )
}

/// The file paths a drag carries.
fn paths_of(info: &ProtocolObject<dyn NSDraggingInfo>) -> Vec<PathBuf> {
    let pasteboard = info.draggingPasteboard();
    let classes = NSArray::from_slice(&[NSURL::class()]);
    let Some(objects) = (unsafe { pasteboard.readObjectsForClasses_options(&classes, None) }) else {
        return Vec::new();
    };
    objects
        .iter()
        .filter_map(|object| object.downcast::<NSURL>().ok())
        .filter(|url| url.isFileURL())
        .filter_map(|url| url.path())
        .map(|path| PathBuf::from(path.to_string()))
        .collect()
}

/// Where the drag is, in the view's physical pixels from its top left.
fn point_in(view: &NSView, info: &ProtocolObject<dyn NSDraggingInfo>) -> (f64, f64) {
    let point = view.convertPoint_fromView(info.draggingLocation(), None);
    let height = view.bounds().size.height;
    let y = if view.isFlipped() { point.y } else { height - point.y };
    let scale = view.window().map_or(1.0, |w| w.backingScaleFactor());
    (point.x * scale, y * scale)
}

fn over(view: &NSView, info: &ProtocolObject<dyn NSDraggingInfo>, entered: bool) -> NSDragOperation {
    let (x, y) = point_in(view, info);
    let keys = keys_now();
    let allowed = allowed_by(info.draggingSourceOperationMask());
    TARGET.with(|t| {
        let mut target = t.borrow_mut();
        let Some(target) = target.as_mut() else { return NSDragOperation::None };
        if entered || target.offer.is_none() {
            let paths = paths_of(info);
            // No file paths: files another program promises.
            let virtual_count = if paths.is_empty() { promise::count(info) } else { 0 };
            target.offer = Some(Offer { paths, allowed, right: false, virtual_count, virtual_files: None });
        }
        let Some(offer) = target.offer.as_mut() else { return NSDragOperation::None };
        offer.allowed = allowed;
        if offer.paths.is_empty() && offer.virtual_count == 0 {
            return NSDragOperation::None;
        }
        operation_for(target.handler.over(offer, x, y, keys).effect)
    })
}

extern "C-unwind" fn dragging_entered(
    this: &NSView,
    _: Sel,
    info: &ProtocolObject<dyn NSDraggingInfo>,
) -> NSDragOperation {
    over(this, info, true)
}

extern "C-unwind" fn dragging_updated(
    this: &NSView,
    _: Sel,
    info: &ProtocolObject<dyn NSDraggingInfo>,
) -> NSDragOperation {
    over(this, info, false)
}

extern "C-unwind" fn dragging_exited(_: &NSView, _: Sel, _: Option<&ProtocolObject<dyn NSDraggingInfo>>) {
    let handler = TARGET.with(|t| {
        let mut target = t.borrow_mut();
        let target = target.as_mut()?;
        target.offer.take()?;
        Some(target.handler.clone())
    });
    if let Some(handler) = handler {
        handler.leave();
    }
}

extern "C-unwind" fn prepare(_: &NSView, _: Sel, _: &ProtocolObject<dyn NSDraggingInfo>) -> bool {
    true
}

extern "C-unwind" fn perform(this: &NSView, _: Sel, info: &ProtocolObject<dyn NSDraggingInfo>) -> bool {
    let (x, y) = point_in(this, info);
    let keys = keys_now();
    let taken = TARGET.with(|t| {
        let mut target = t.borrow_mut();
        let target = target.as_mut()?;
        let offer = target.offer.take()?;
        Some((target.handler.clone(), offer))
    });
    match taken {
        Some((handler, mut offer)) => {
            if offer.virtual_count > 0 {
                // Asked for now, on the main thread; the job waits for the files.
                let Some(files) = promise::receive(info) else { return false };
                offer.virtual_files = Some(files);
            }
            handler.dropped(&offer, x, y, keys).is_some()
        }
        None => false,
    }
}

/// Puts `imp` in place of `selector` on `class` (or adds it).
unsafe fn replace(class: &AnyClass, selector: Sel, imp: Imp, types: &[u8]) {
    unsafe {
        objc2::ffi::class_replaceMethod(
            class as *const AnyClass as *mut AnyClass,
            selector,
            imp,
            types.as_ptr().cast::<c_char>(),
        );
    }
}

/// Type encodings: an NSUInteger, a BOOL, void; the receiver, the selector, one object.
const RETURNS_OPERATION: &[u8] = b"Q@:@\0";
#[cfg(target_arch = "aarch64")]
const RETURNS_BOOL: &[u8] = b"B@:@\0";
#[cfg(not(target_arch = "aarch64"))]
const RETURNS_BOOL: &[u8] = b"c@:@\0";
const RETURNS_VOID: &[u8] = b"v@:@\0";

pub fn register(window: &impl HasWindowHandle, handler: Rc<dyn DropHandler>) -> Option<Registration> {
    let handle = window.window_handle().ok()?;
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else { return None };
    // Safety: winit's view lives as long as its window.
    let view: Retained<NSView> = unsafe { Retained::retain(appkit.ns_view.as_ptr().cast::<NSView>()) }?;
    let class = (*view).class();
    unsafe {
        type Entered = extern "C-unwind" fn(&NSView, Sel, &ProtocolObject<dyn NSDraggingInfo>) -> NSDragOperation;
        type Exited = extern "C-unwind" fn(&NSView, Sel, Option<&ProtocolObject<dyn NSDraggingInfo>>);
        type Decide = extern "C-unwind" fn(&NSView, Sel, &ProtocolObject<dyn NSDraggingInfo>) -> bool;
        let imp = |f: *const ()| std::mem::transmute::<*const (), Imp>(f);
        replace(class, sel!(draggingEntered:), imp(dragging_entered as Entered as *const ()), RETURNS_OPERATION);
        replace(class, sel!(draggingUpdated:), imp(dragging_updated as Entered as *const ()), RETURNS_OPERATION);
        replace(class, sel!(draggingExited:), imp(dragging_exited as Exited as *const ()), RETURNS_VOID);
        replace(class, sel!(prepareForDragOperation:), imp(prepare as Decide as *const ()), RETURNS_BOOL);
        replace(class, sel!(performDragOperation:), imp(perform as Decide as *const ()), RETURNS_BOOL);
    }
    // winit registers the window, so drags go to its delegate (with no position or keys);
    // a registered view under the pointer comes first, so the methods above are asked.
    // Files, and files promised by other programs (Mail, Photos).
    view.registerForDraggedTypes(&promise::types().arrayByAddingObject(unsafe { NSPasteboardTypeFileURL }));
    TARGET.with(|t| *t.borrow_mut() = Some(Target { handler, offer: None }));
    Some(Registration { view })
}

define_class!(
    // Safety: NSObject has no subclassing requirements, and this class has no Drop.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "GezikDragSource"]
    struct DragSource;

    unsafe impl NSObjectProtocol for DragSource {}

    unsafe impl NSDraggingSource for DragSource {
        #[unsafe(method(draggingSession:sourceOperationMaskForDraggingContext:))]
        fn source_operation_mask(&self, _session: &NSDraggingSession, _context: NSDraggingContext) -> NSDragOperation {
            NSDragOperation::Copy | NSDragOperation::Move | NSDragOperation::Generic
        }

        #[unsafe(method(draggingSession:endedAtPoint:operation:))]
        fn ended(&self, _session: &NSDraggingSession, _point: NSPoint, operation: NSDragOperation) {
            let end = if operation == NSDragOperation::None { DragEnd::Cancelled } else { DragEnd::Dropped };
            if let Some(on_end) = ON_END.with(|e| e.borrow_mut().take()) {
                on_end(end);
            }
        }
    }
);

impl DragSource {
    fn new(mtm: MainThreadMarker) -> Retained<DragSource> {
        let this = Self::alloc(mtm).set_ivars(());
        unsafe { msg_send![super(this), init] }
    }
}

impl Registration {
    /// Starts a drag session for `paths` from winit's view, with the event that moved the
    /// pointer out of the window. AppKit runs it; `on_end` is called when it ends.
    pub fn drag_out(&self, paths: &[PathBuf], on_end: OnEnd) -> Result<(), String> {
        let mtm = MainThreadMarker::new().ok_or("not on the main thread")?;
        let event = NSApplication::sharedApplication(mtm).currentEvent().ok_or("no current event")?;
        let start = self.view.convertPoint_fromView(event.locationInWindow(), None);
        // One icon (the first item's) for all: reading each file's icon would make a large
        // selection slow to pick up.
        let first = paths.first().and_then(|path| path.to_str()).ok_or("no path can be dragged")?;
        let icon = NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(first));
        let items: Vec<Retained<NSDraggingItem>> = paths
            .iter()
            .filter_map(|path| {
                // Built from the text: `fileURLWithPath` would ask the disk whether it is a folder.
                NSURL::URLWithString(&NSString::from_str(&crate::linux::uri::file_uri(path)))
            })
            .enumerate()
            .map(|(i, url)| {
                let writer: &ProtocolObject<dyn NSPasteboardWriting> = ProtocolObject::from_ref(&*url);
                let item = NSDraggingItem::initWithPasteboardWriter(NSDraggingItem::alloc(), writer);
                let offset = 4.0 * i as f64;
                let frame = NSRect::new(
                    NSPoint::new(start.x - 16.0 + offset, start.y - 16.0 - offset),
                    NSSize::new(32.0, 32.0),
                );
                unsafe { item.setDraggingFrame_contents(frame, Some(&icon)) };
                item
            })
            .collect();
        if items.is_empty() {
            return Err("no path can be dragged".into());
        }
        let source = DragSource::new(mtm);
        ON_END.with(|e| *e.borrow_mut() = Some(on_end));
        let items = NSArray::from_retained_slice(&items);
        let _session =
            self.view.beginDraggingSessionWithItems_event_source(&items, &event, ProtocolObject::from_ref(&*source));
        Ok(())
    }
}
