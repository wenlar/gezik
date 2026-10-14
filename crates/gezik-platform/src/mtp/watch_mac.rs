//! macOS (spec 16 §10.3): phones found in the IORegistry without loading libmtp (their USB
//! interfaces and the device above each), and IOKit's notification of interfaces coming and
//! going on the main run loop (no thread, no polling).

use std::ffi::{CStr, c_char, c_void};

type IoObject = u32;
type KernReturn = i32;
type CfRef = *const c_void;
type Changed = unsafe extern "C" fn(refcon: *mut c_void, iterator: IoObject);

/// `kIOMainPortDefault`.
const MAIN_PORT: u32 = 0;
const ITERATE_RECURSIVELY: u32 = 1;
const ITERATE_PARENTS: u32 = 2;
const NUMBER_SINT64: isize = 4;
const UTF8: u32 = 0x0800_0100;

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOServiceMatching(name: *const c_char) -> *mut c_void;
    fn IOServiceGetMatchingServices(port: u32, matching: *mut c_void, existing: *mut IoObject) -> KernReturn;
    fn IOIteratorNext(iterator: IoObject) -> IoObject;
    fn IOObjectRelease(object: IoObject) -> KernReturn;
    fn IORegistryEntrySearchCFProperty(
        entry: IoObject,
        plane: *const c_char,
        key: CfRef,
        allocator: CfRef,
        options: u32,
    ) -> CfRef;
    fn IONotificationPortCreate(port: u32) -> *mut c_void;
    fn IONotificationPortGetRunLoopSource(port: *mut c_void) -> *mut c_void;
    fn IOServiceAddMatchingNotification(
        port: *mut c_void,
        kind: *const c_char,
        matching: *mut c_void,
        callback: Changed,
        refcon: *mut c_void,
        notification: *mut IoObject,
    ) -> KernReturn;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFRunLoopCommonModes: CfRef;
    fn CFStringCreateWithCString(allocator: CfRef, text: *const c_char, encoding: u32) -> CfRef;
    fn CFStringGetCString(text: CfRef, buffer: *mut c_char, size: isize, encoding: u32) -> u8;
    fn CFGetTypeID(object: CfRef) -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFNumberGetTypeID() -> usize;
    fn CFNumberGetValue(number: CfRef, kind: isize, value: *mut c_void) -> u8;
    fn CFRelease(object: CfRef);
    fn CFRunLoopGetMain() -> *mut c_void;
    fn CFRunLoopAddSource(run_loop: *mut c_void, source: *mut c_void, mode: CfRef);
}

/// A Core Foundation object released when dropped.
struct Owned(CfRef);

impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: an object this code created or copied (one reference).
            unsafe { CFRelease(self.0) };
        }
    }
}

/// Property `key` of `entry` or, if it has none, of the nearest entry above it.
fn property(entry: IoObject, key: &CStr) -> Option<Owned> {
    // SAFETY: a valid entry and a string made here; the result is a copy this code owns.
    unsafe {
        let key = Owned(CFStringCreateWithCString(std::ptr::null(), key.as_ptr(), UTF8));
        let plane = c"IOService";
        let found = IORegistryEntrySearchCFProperty(
            entry,
            plane.as_ptr(),
            key.0,
            std::ptr::null(),
            ITERATE_RECURSIVELY | ITERATE_PARENTS,
        );
        (!found.is_null()).then_some(Owned(found))
    }
}

fn number(entry: IoObject, key: &CStr) -> Option<i64> {
    let value = property(entry, key)?;
    let mut number = 0i64;
    // SAFETY: the type is checked before the value is read into an i64.
    let read = unsafe {
        CFGetTypeID(value.0) == CFNumberGetTypeID()
            && CFNumberGetValue(value.0, NUMBER_SINT64, (&raw mut number).cast()) != 0
    };
    read.then_some(number)
}

fn text(entry: IoObject, key: &CStr) -> Option<String> {
    let value = property(entry, key)?;
    let mut buffer = [0 as c_char; 256];
    // SAFETY: the type is checked; the buffer's length is given and it ends with a NUL.
    let read = unsafe {
        CFGetTypeID(value.0) == CFStringGetTypeID()
            && CFStringGetCString(value.0, buffer.as_mut_ptr(), buffer.len() as isize, UTF8) != 0
    };
    // SAFETY: CFStringGetCString wrote a NUL-ended string.
    read.then(|| unsafe { CStr::from_ptr(buffer.as_ptr()) }.to_string_lossy().trim().to_owned())
}

/// `(id, name)` of each USB device with an MTP interface. libusb (and so libmtp) numbers a
/// device's bus by the top byte of its `locationID` and uses its `USB Address`.
pub(super) fn interfaces() -> Vec<(String, String)> {
    let mut iterator = 0;
    // SAFETY: the matching dictionary is consumed by the call; the iterator is released below.
    let listed = unsafe {
        IOServiceGetMatchingServices(MAIN_PORT, IOServiceMatching(c"IOUSBHostInterface".as_ptr()), &mut iterator)
    };
    if listed != 0 {
        return Vec::new();
    }
    let mut found: Vec<(String, String)> = Vec::new();
    loop {
        // SAFETY: the iterator lives until released after the loop.
        let interface = unsafe { IOIteratorNext(iterator) };
        if interface == 0 {
            break;
        }
        let byte = |key: &CStr| number(interface, key).and_then(|n| u8::try_from(n).ok());
        let mtp = match (byte(c"bInterfaceClass"), byte(c"bInterfaceSubClass"), byte(c"bInterfaceProtocol")) {
            (Some(class), Some(subclass), Some(protocol)) => {
                let label = text(interface, c"USB Interface Name").unwrap_or_default();
                let vendor = number(interface, c"idVendor").and_then(|n| u16::try_from(n).ok()).unwrap_or(0);
                super::is_mtp_interface(class, subclass, protocol, &label, vendor)
            }
            _ => false,
        };
        let location = number(interface, c"locationID");
        let address = number(interface, c"USB Address").and_then(|n| u8::try_from(n).ok());
        if mtp && let (Some(location), Some(address)) = (location, address) {
            let id = super::usb_id(((location as u64) >> 24) as u32 & 0xff, address);
            if !found.iter().any(|(known, _)| *known == id) {
                let name = text(interface, c"USB Product Name").or_else(|| text(interface, c"kUSBProductString"));
                found.push((id, name.unwrap_or_default()));
            }
        }
        // SAFETY: an object the iterator handed out.
        unsafe { IOObjectRelease(interface) };
    }
    // SAFETY: the iterator made above.
    unsafe { IOObjectRelease(iterator) };
    found.sort();
    found
}

/// Empties a notification's iterator (which also arms it for the next one).
fn drain(iterator: IoObject) {
    // SAFETY: an iterator IOKit handed out; each object is released.
    unsafe {
        loop {
            let object = IOIteratorNext(iterator);
            if object == 0 {
                break;
            }
            IOObjectRelease(object);
        }
    }
}

unsafe extern "C" fn changed(refcon: *mut c_void, iterator: IoObject) {
    drain(iterator);
    // SAFETY: `refcon` is the callback `watch` leaked, never freed.
    let on_change = unsafe { &*(refcon as *const Box<dyn Fn() + Send>) };
    on_change();
}

/// IOKit tells the main run loop of each USB interface matched or gone; `on_change` is called
/// there. Registered once, for Gezik's life.
pub(super) fn watch(on_change: impl Fn() + Send + 'static) -> bool {
    let on_change: Box<dyn Fn() + Send> = Box::new(on_change);
    // SAFETY: the port and its source live for the process; each matching dictionary is
    // consumed by its call; the callback's data is leaked so it outlives every call.
    unsafe {
        let port = IONotificationPortCreate(MAIN_PORT);
        if port.is_null() {
            return false;
        }
        CFRunLoopAddSource(CFRunLoopGetMain(), IONotificationPortGetRunLoopSource(port), kCFRunLoopCommonModes);
        let refcon = Box::into_raw(Box::new(on_change)).cast::<c_void>();
        for kind in [c"IOServiceFirstMatch", c"IOServiceTerminate"] {
            let mut iterator = 0;
            let matching = IOServiceMatching(c"IOUSBHostInterface".as_ptr());
            if IOServiceAddMatchingNotification(port, kind.as_ptr(), matching, changed, refcon, &mut iterator) != 0 {
                return false;
            }
            // What is there already: not a change, but the iterator must be emptied to arm it.
            drain(iterator);
        }
    }
    true
}
