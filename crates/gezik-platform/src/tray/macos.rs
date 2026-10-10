//! macOS: an NSStatusItem in the menu bar with the SF Symbol `folder` (a template image, so it
//! takes the bar's colour; deviation 7) and its menu: Show Gezik, the first nine pins, Quit
//! Gezik. A click opens the menu (deviation 8). Main thread only; no thread of its own.

use std::cell::RefCell;
use std::rc::Rc;

use objc2::rc::Retained;
use objc2::runtime::{NSObject, NSObjectProtocol};
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{NSImage, NSMenu, NSMenuItem, NSStatusBar, NSStatusItem, NSVariableStatusItemLength};
use objc2_foundation::NSString;

use crate::tray::{MenuLine, OnEvent, OnReady, TIP, TrayError, event_for, menu};

thread_local! {
    /// The one icon's events (the app holds at most one Tray; a new one is made after the old
    /// one is dropped). An Rc, so the call below holds no borrow while it runs.
    static ON_EVENT: RefCell<Option<Rc<OnEvent>>> = const { RefCell::new(None) };
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and this class has no Drop.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "GezikTrayTarget"]
    struct MenuTarget;

    unsafe impl NSObjectProtocol for MenuTarget {}

    impl MenuTarget {
        #[unsafe(method(chosen:))]
        fn chosen(&self, item: &NSMenuItem) {
            let Some(event) = u32::try_from(item.tag()).ok().and_then(event_for) else { return };
            // Cloned out first: a Tray dropped inside the call must not meet a held borrow.
            if let Some(f) = ON_EVENT.with(|f| f.borrow().clone()) {
                f(event);
            }
        }
    }
);

impl MenuTarget {
    fn new(mtm: MainThreadMarker) -> Retained<MenuTarget> {
        // SAFETY: NSObject's plain init.
        unsafe { msg_send![MenuTarget::alloc(mtm), init] }
    }
}

pub struct Tray(Option<(Retained<NSStatusItem>, Retained<MenuTarget>)>);

pub fn start(pins: Vec<String>, on_event: OnEvent, on_ready: OnReady) -> Tray {
    let Some(mtm) = MainThreadMarker::new() else {
        on_ready(Err(TrayError::Failed("the menu bar is reached from the main thread only".to_owned())));
        return Tray(None);
    };
    ON_EVENT.with(|f| *f.borrow_mut() = Some(Rc::new(on_event)));
    let item = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
    if let Some(button) = item.button(mtm) {
        let tip = NSString::from_str(TIP);
        let image =
            NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str("folder"), Some(&tip));
        if let Some(image) = &image {
            image.setTemplate(true);
        }
        button.setImage(image.as_deref());
        button.setToolTip(Some(&tip));
    }
    let target = MenuTarget::new(mtm);
    item.setMenu(Some(&build(mtm, &pins, &target)));
    on_ready(Ok(()));
    Tray(Some((item, target)))
}

fn build(mtm: MainThreadMarker, pins: &[String], target: &MenuTarget) -> Retained<NSMenu> {
    let list = NSMenu::new(mtm);
    for line in menu(pins, false) {
        match line {
            MenuLine::Separator => list.addItem(&NSMenuItem::separatorItem(mtm)),
            MenuLine::Item(id, label) => {
                // SAFETY: a plain item whose action is our target's `chosen:`.
                let item = unsafe {
                    NSMenuItem::initWithTitle_action_keyEquivalent(
                        NSMenuItem::alloc(mtm),
                        &NSString::from_str(&label),
                        Some(sel!(chosen:)),
                        &NSString::from_str(""),
                    )
                };
                // SAFETY: the target lives as long as the Tray that holds it and the menu.
                unsafe { item.setTarget(Some(target)) };
                item.setTag(id as isize);
                list.addItem(&item);
            }
        }
    }
    list
}

impl Tray {
    pub fn set_pins(&self, pins: Vec<String>) {
        let (Some((item, target)), Some(mtm)) = (&self.0, MainThreadMarker::new()) else { return };
        item.setMenu(Some(&build(mtm, &pins, target)));
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        if let Some((item, _target)) = self.0.take() {
            NSStatusBar::systemStatusBar().removeStatusItem(&item);
            ON_EVENT.with(|f| f.borrow_mut().take());
        }
    }
}
