//! macOS: the system Quick Look panel (QLPreviewPanel) for Gezik's selection (spec 9 §4.5). The
//! panel asks the responder chain who controls it: winit's view gets the three methods for that
//! at the first Space, and Gezik's data source and delegate are set while it controls it.

use std::cell::{Cell, RefCell};
use std::ffi::c_char;
use std::path::PathBuf;

use gezik_core::layout::Move;
use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::{AnyClass, AnyObject, Imp, NSObject, NSObjectProtocol, ProtocolObject, Sel};
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType, NSView, NSWindowDelegate};
use objc2_foundation::{NSInteger, NSString, NSURL};
use objc2_quick_look_ui::{QLPreviewItem, QLPreviewPanel, QLPreviewPanelDataSource, QLPreviewPanelDelegate};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

use crate::ql_panel::{PanelKey, key_action};

/// Moves Gezik's focus; whether it moved.
type OnMove = Box<dyn Fn(Move) -> bool>;

thread_local! {
    static ITEMS: RefCell<Vec<Retained<NSURL>>> = const { RefCell::new(Vec::new()) };
    static ON_MOVE: RefCell<Option<OnMove>> = const { RefCell::new(None) };
    static SOURCE: RefCell<Option<Retained<PanelSource>>> = const { RefCell::new(None) };
    static INSTALLED: Cell<bool> = const { Cell::new(false) };
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and this class has no Drop.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "GezikPreviewPanelSource"]
    struct PanelSource;

    unsafe impl NSObjectProtocol for PanelSource {}
    unsafe impl NSWindowDelegate for PanelSource {}

    unsafe impl QLPreviewPanelDataSource for PanelSource {
        #[unsafe(method(numberOfPreviewItemsInPreviewPanel:))]
        fn count(&self, _panel: Option<&QLPreviewPanel>) -> NSInteger {
            ITEMS.with(|items| items.borrow().len() as NSInteger)
        }

        #[unsafe(method_id(previewPanel:previewItemAtIndex:))]
        fn item(
            &self,
            _panel: Option<&QLPreviewPanel>,
            index: NSInteger,
        ) -> Option<Retained<ProtocolObject<dyn QLPreviewItem>>> {
            let index = usize::try_from(index).ok();
            let url = ITEMS.with(|items| index.and_then(|i| items.borrow().get(i).cloned()));
            url.map(ProtocolObject::from_retained)
        }
    }

    unsafe impl QLPreviewPanelDelegate for PanelSource {
        #[unsafe(method(previewPanel:handleEvent:))]
        fn handle_event(&self, panel: Option<&QLPreviewPanel>, event: Option<&NSEvent>) -> bool {
            handle_event(panel, event)
        }
    }
);

/// A key the panel did not use: Gezik's arrows (one item) and Space (close).
fn handle_event(panel: Option<&QLPreviewPanel>, event: Option<&NSEvent>) -> bool {
    let Some(event) = event.filter(|e| e.r#type() == NSEventType::KeyDown) else { return false };
    let held = NSEventModifierFlags::Command
        | NSEventModifierFlags::Control
        | NSEventModifierFlags::Option
        | NSEventModifierFlags::Shift;
    let modified = event.modifierFlags().intersects(held);
    let count = ITEMS.with(|items| items.borrow().len());
    match key_action(event.keyCode(), modified, count) {
        PanelKey::Move(to) => ON_MOVE.with(|f| f.borrow().as_ref().is_some_and(|f| f(to))),
        PanelKey::Close => {
            if let Some(panel) = panel {
                panel.orderOut(None);
            }
            true
        }
        PanelKey::Pass => false,
    }
}

fn source(mtm: MainThreadMarker) -> Retained<PanelSource> {
    SOURCE.with(|s| {
        s.borrow_mut()
            .get_or_insert_with(|| {
                let this = PanelSource::alloc(mtm).set_ivars(());
                // SAFETY: NSObject's init.
                unsafe { msg_send![super(this), init] }
            })
            .clone()
    })
}

extern "C-unwind" fn accepts(_: &NSView, _: Sel, _: Option<&QLPreviewPanel>) -> bool {
    true
}

extern "C-unwind" fn begin(_: &NSView, _: Sel, panel: Option<&QLPreviewPanel>) {
    let (Some(panel), Some(mtm)) = (panel, MainThreadMarker::new()) else { return };
    let source = source(mtm);
    let delegate: &AnyObject = &source;
    // SAFETY: the source implements both protocols and lives in SOURCE for the thread's life.
    unsafe {
        panel.setDataSource(Some(ProtocolObject::from_ref(&*source)));
        panel.setDelegate(Some(delegate));
        panel.reloadData();
    }
}

extern "C-unwind" fn end(_: &NSView, _: Sel, panel: Option<&QLPreviewPanel>) {
    if let Some(panel) = panel {
        // SAFETY: clearing the panel's weak references to Gezik's source.
        unsafe {
            panel.setDataSource(None);
            panel.setDelegate(None);
        }
    }
}

/// Type encodings: a BOOL, void; the receiver, the selector, one object.
#[cfg(target_arch = "aarch64")]
const RETURNS_BOOL: &[u8] = b"B@:@\0";
#[cfg(not(target_arch = "aarch64"))]
const RETURNS_BOOL: &[u8] = b"c@:@\0";
const RETURNS_VOID: &[u8] = b"v@:@\0";

/// Adds the three panel-control methods to winit's view class, once.
fn install(view: &NSView) {
    if INSTALLED.with(Cell::get) {
        return;
    }
    let class: &AnyClass = view.class();
    type Accepts = extern "C-unwind" fn(&NSView, Sel, Option<&QLPreviewPanel>) -> bool;
    type Control = extern "C-unwind" fn(&NSView, Sel, Option<&QLPreviewPanel>);
    // SAFETY: the functions match the selectors' signatures and type encodings.
    unsafe {
        let imp = |f: *const ()| std::mem::transmute::<*const (), Imp>(f);
        let add = |selector: Sel, imp: Imp, types: &[u8]| {
            objc2::ffi::class_addMethod(
                class as *const AnyClass as *mut AnyClass,
                selector,
                imp,
                types.as_ptr().cast::<c_char>(),
            );
        };
        add(sel!(acceptsPreviewPanelControl:), imp(accepts as Accepts as *const ()), RETURNS_BOOL);
        add(sel!(beginPreviewPanelControl:), imp(begin as Control as *const ()), RETURNS_VOID);
        add(sel!(endPreviewPanelControl:), imp(end as Control as *const ()), RETURNS_VOID);
    }
    INSTALLED.with(|i| i.set(true));
}

fn set_items(items: &[PathBuf]) {
    let urls =
        items.iter().filter_map(|p| p.to_str()).map(|p| NSURL::fileURLWithPath(&NSString::from_str(p))).collect();
    ITEMS.with(|i| *i.borrow_mut() = urls);
}

/// Opens the panel on `items` at `index`; `on_move` moves Gezik's focus (one item). Whether the
/// panel is on screen afterwards.
pub fn show(window: &impl HasWindowHandle, items: &[PathBuf], index: usize, on_move: OnMove) -> bool {
    let Some(mtm) = MainThreadMarker::new() else { return false };
    let Ok(handle) = window.window_handle() else { return false };
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else { return false };
    autoreleasepool(|_| {
        // SAFETY: winit's view lives as long as its window.
        let Some(view) = (unsafe { Retained::retain(appkit.ns_view.as_ptr().cast::<NSView>()) }) else {
            return false;
        };
        install(&view);
        set_items(items);
        ON_MOVE.with(|f| *f.borrow_mut() = Some(on_move));
        // SAFETY: on the main thread.
        let Some(panel) = (unsafe { QLPreviewPanel::sharedPreviewPanel(mtm) }) else { return false };
        // SAFETY: the panel's own calls, on the main thread.
        unsafe {
            if panel.isVisible() {
                panel.reloadData();
            } else {
                panel.makeKeyAndOrderFront(None);
            }
            panel.setCurrentPreviewItemIndex(index as NSInteger);
        }
        panel.isVisible()
    })
}

/// The selection changed while the panel is open: it shows `items` at `index`.
pub fn update(items: &[PathBuf], index: usize) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    if !is_open() {
        return;
    }
    autoreleasepool(|_| {
        set_items(items);
        // SAFETY: on the main thread; the panel exists (is_open).
        unsafe {
            if let Some(panel) = QLPreviewPanel::sharedPreviewPanel(mtm) {
                panel.reloadData();
                panel.setCurrentPreviewItemIndex(index as NSInteger);
            }
        }
    });
}

/// Closes the panel if it is on screen.
pub fn close() {
    let Some(mtm) = MainThreadMarker::new() else { return };
    if is_open() {
        autoreleasepool(|_| {
            // SAFETY: on the main thread; the panel exists.
            if let Some(panel) = unsafe { QLPreviewPanel::sharedPreviewPanel(mtm) } {
                panel.orderOut(None);
            }
        });
    }
}

/// Whether the panel is on screen (it can be closed by its own button or Esc).
pub fn is_open() -> bool {
    let Some(mtm) = MainThreadMarker::new() else { return false };
    // SAFETY: on the main thread; `sharedPreviewPanelExists` does not create the panel.
    autoreleasepool(|_| unsafe {
        QLPreviewPanel::sharedPreviewPanelExists(mtm)
            && QLPreviewPanel::sharedPreviewPanel(mtm).is_some_and(|p| p.isVisible())
    })
}
