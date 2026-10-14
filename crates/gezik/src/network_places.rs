//! The sidebar's Network section (spec 16h): computers sharing files on the local network. It is
//! closed at start; a search runs on a thread of its own only when it is opened or Refresh is
//! chosen, and ends within five seconds. Closing it drops the list; nothing is kept or polled.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;

use gezik_core::nav::Location;
use gezik_i18n::t;
use gezik_platform::network::{DISCOVERY_TIME, Found};

thread_local! {
    static OPEN: Cell<bool> = const { Cell::new(false) };
    static SEARCHING: Cell<bool> = const { Cell::new(false) };
    static FOUND: RefCell<Vec<Found>> = const { RefCell::new(Vec::new()) };
    /// Why nothing could be looked for (a `tx!` text), after a search.
    static TROUBLE: Cell<Option<&'static str>> = const { Cell::new(None) };
    /// The latest search asked for: what an older one finds later is dropped.
    static ASKED: Cell<u64> = const { Cell::new(0) };
}

/// A row's index under the heading: a note (searching, nothing found).
pub const NOTE: i32 = -1;
/// The row that opens Windows' network discovery settings.
pub const SETTINGS: i32 = -2;

pub fn is_open() -> bool {
    OPEN.get()
}

/// The rows under the heading: (label, index); index ≥ 0 is a computer of the list.
pub fn lines() -> Vec<(String, i32)> {
    if !OPEN.get() {
        return Vec::new();
    }
    FOUND.with(|f| lines_of(&f.borrow(), SEARCHING.get(), TROUBLE.get(), cfg!(windows)))
}

fn lines_of(found: &[Found], searching: bool, trouble: Option<&'static str>, windows: bool) -> Vec<(String, i32)> {
    let mut lines: Vec<(String, i32)> = found.iter().zip(0..).map(|(one, i)| (one.name.clone(), i)).collect();
    if searching {
        lines.push((t!("Searching…").into_owned(), NOTE));
    } else if let Some(why) = trouble {
        lines.push((gezik_i18n::t("", why).into_owned(), NOTE));
    } else if found.is_empty() {
        lines.push((t!("No computers found").into_owned(), NOTE));
        // Network discovery may be off: Gezik does not turn it on, it shows where.
        if windows {
            lines.push((t!("Open Settings").into_owned(), SETTINGS));
        }
    }
    lines
}

/// The heading's arrow: opens the section (and searches) or closes it (and drops the list).
pub fn toggle() {
    if OPEN.replace(!OPEN.get()) {
        ASKED.set(ASKED.get() + 1);
        SEARCHING.set(false);
        FOUND.with(|f| f.borrow_mut().clear());
        changed();
    } else {
        search();
    }
}

/// Refresh on the heading: searches again (opening the section if it is closed).
pub fn refresh() {
    OPEN.set(true);
    search();
}

fn search() {
    let asked = ASKED.get() + 1;
    ASKED.set(asked);
    SEARCHING.set(true);
    TROUBLE.set(None);
    FOUND.with(|f| f.borrow_mut().clear());
    changed();
    std::thread::spawn(move || {
        let result = gezik_platform::network::discover(&mut |one| {
            let _ = slint::invoke_from_event_loop(move || arrived(asked, one));
        });
        let _ = slint::invoke_from_event_loop(move || ended(asked, result.err()));
    });
    // Windows' own search may take longer: the list is what came within the time.
    slint::Timer::single_shot(DISCOVERY_TIME, move || ended(asked, None));
}

fn arrived(asked: u64, one: Found) {
    if ASKED.get() != asked || !SEARCHING.get() {
        return;
    }
    // One computer may answer twice (IPv4 and IPv6, two interfaces).
    let new = FOUND.with(|f| {
        let mut f = f.borrow_mut();
        let new = !f.iter().any(|o| o.name.eq_ignore_ascii_case(&one.name));
        if new {
            f.push(one);
        }
        new
    });
    if new {
        changed();
    }
}

fn ended(asked: u64, trouble: Option<&'static str>) {
    if ASKED.get() == asked && SEARCHING.replace(false) {
        TROUBLE.set(trouble);
        changed();
    }
}

fn changed() {
    crate::windows::for_each(|_| crate::sidebar::with_current(crate::sidebar::Sidebar::network_changed));
}

/// A click on row `index`: Windows lists the computer's shares (`\\NAME`); macOS and Linux open
/// Connect to Server with `smb://host/`, for the share to be added.
pub fn clicked(index: i32, new_tab: bool) {
    if index == SETTINGS {
        // shortcut: the Control Panel page Explorer's banner opens; if it ever goes, ms-settings:network.
        let _ = std::process::Command::new("control.exe")
            .args(["/name", "Microsoft.NetworkAndSharingCenter", "/page", "Advanced"])
            .spawn();
        return;
    }
    let Some(one) = usize::try_from(index).ok().and_then(|i| FOUND.with(|f| f.borrow().get(i).cloned())) else {
        return;
    };
    if cfg!(windows) {
        let place = Location::Path(PathBuf::from(format!(r"\\{}", one.address)));
        let nav = crate::panes::active_nav();
        if new_tab { nav.open_tab(place, false) } else { nav.go(place) }
    } else {
        crate::connect::open_with(format!("smb://{}/", one.address));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nas() -> Found {
        Found { name: "NAS".into(), address: "nas.local".into() }
    }

    #[test]
    fn the_rows_say_what_the_search_did() {
        assert_eq!(lines_of(&[nas()], true, None, false), [("NAS".into(), 0), ("Searching…".into(), NOTE)]);
        assert_eq!(lines_of(&[nas()], false, None, true), [("NAS".to_string(), 0)]);
        assert_eq!(
            lines_of(&[], false, None, true),
            [("No computers found".into(), NOTE), ("Open Settings".into(), SETTINGS)]
        );
        assert_eq!(lines_of(&[], false, None, false), [("No computers found".to_string(), NOTE)]);
        let avahi = lines_of(&[], false, Some(gezik_platform::network::NO_AVAHI), false);
        assert_eq!(avahi, [("Avahi is not running".to_string(), NOTE)]);
    }
}
