//! Phones in the sidebar (spec 16 §10.2, §10.5): found by the system's device event, never by
//! polling, and read on a worker without loading WPD. A phone unplugged takes its tabs to This
//! PC. `[devices] phones = false` looks for none.

use std::cell::{Cell, RefCell};

use gezik_core::nav::Location;
use gezik_core::vfs::{VfsPath, VfsRoot};
use gezik_platform::mtp::Device;
use slint::ComponentHandle;

use crate::AppWindow;

thread_local! {
    static LIST: RefCell<Vec<Device>> = const { RefCell::new(Vec::new()) };
    static ON: Cell<bool> = const { Cell::new(true) };
    static WATCHING: Cell<bool> = const { Cell::new(false) };
    /// The latest read asked for: an older one coming back later is dropped.
    static ASKED: Cell<u64> = const { Cell::new(0) };
}

/// The phones plugged in, as last read.
pub fn list() -> Vec<Device> {
    LIST.with(|l| l.borrow().clone())
}

/// The top of phone `index` of [`list`].
pub fn location(index: usize) -> Option<Location> {
    let phone = list().into_iter().nth(index)?;
    Some(Location::Vfs(VfsPath { root: VfsRoot::Device { id: phone.id, name: phone.name }, inner: String::new() }))
}

/// A phone's address (`mtp://Pixel 8/DCIM`) as the place on the phone of that name now.
pub fn place_of(name: &str, inner: String) -> Option<Location> {
    let phone = list().into_iter().find(|p| p.name.eq_ignore_ascii_case(name))?;
    Some(Location::Vfs(VfsPath { root: VfsRoot::Device { id: phone.id, name: phone.name }, inner }))
}

/// `[devices] phones`: turned on, they are looked for (once watching started); off, none show.
pub fn set_settings(on: bool) {
    if ON.replace(on) == on {
        return;
    }
    if WATCHING.get() {
        read();
    } else if on {
        // Turned on while running: the watch starts now, no restart needed.
        crate::windows::with_focused(start);
    }
}

/// After the first frame: the device event on `window` (the system tells it when a portable
/// device comes or goes), then the first read. Nothing at all while `[devices] phones` is off.
// shortcut: only this window hears the event; if it closes while others stay, phones are found again at the next start.
pub fn start(window: &AppWindow) {
    if WATCHING.get() || !ON.get() {
        return;
    }
    let handle = window.window().window_handle();
    if gezik_platform::watch_phones(&handle, read) {
        WATCHING.set(true);
        read();
    }
}

/// Reads the phones on a worker.
fn read() {
    let on = ON.get();
    let asked = ASKED.with(|a| a.get() + 1);
    ASKED.with(|a| a.set(asked));
    std::thread::spawn(move || {
        let phones = if on { gezik_platform::mtp::devices() } else { Vec::new() };
        let _ = slint::invoke_from_event_loop(move || {
            if ASKED.with(Cell::get) == asked {
                arrived(phones);
            }
        });
    });
}

/// The phones read: a phone gone takes its tabs to This PC; every sidebar shows the list.
fn arrived(phones: Vec<Device>) {
    let before = LIST.with(|l| l.replace(phones.clone()));
    if before == phones {
        return;
    }
    for gone in gone(&before, &phones) {
        // Its session is done with, even if it comes back: the next visit opens a new one.
        let root = VfsRoot::Device { id: gone.id.clone(), name: gone.name.clone() };
        if let Some(vfs) = gezik_vfs::mounted(&root) {
            vfs.lost();
        }
        for p in crate::panes::everywhere() {
            p.nav.leave_device(&gone.id);
        }
        crate::panes::with_active(|p| p.nav.status(gezik_i18n::t!("{} was disconnected", gone.name)));
    }
    crate::windows::for_each(|_| crate::sidebar::with_current(crate::sidebar::Sidebar::phones_changed));
}

/// The phones of `before` not in `now`.
fn gone<'a>(before: &'a [Device], now: &[Device]) -> Vec<&'a Device> {
    before.iter().filter(|d| !now.iter().any(|n| n.id == d.id)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn phone(id: &str) -> Device {
        Device { id: id.into(), name: id.to_uppercase() }
    }

    #[test]
    fn a_phone_gone_is_one_no_longer_listed() {
        let before = [phone("a"), phone("b")];
        let gone = gone(&before, &[phone("b"), phone("c")]);
        assert_eq!(gone, [&phone("a")]);
    }
}
