//! Computers on the local network (spec 16h): Windows' Network folder, Bonjour on macOS, Avahi
//! on Linux. Blocking, for a worker thread; nothing is kept or polled.

use std::time::Duration;

/// How long a search runs at most.
pub const DISCOVERY_TIME: Duration = Duration::from_secs(5);

// Marked for translation: shown with `gezik_i18n::t`.
pub const NO_AVAHI: &str = gezik_i18n::tx!("Avahi is not running");

/// A computer found: the name shown and the server to connect to (`\\NAME` on Windows, a host
/// name elsewhere).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub name: String,
    pub address: String,
}

/// Looks for computers sharing files, handing each to `found` as it comes (one may come twice:
/// IPv4 and IPv6). Ends within [`DISCOVERY_TIME`] on macOS and Linux; Windows' own search can take
/// longer (the caller stops listening). `Err`: why nothing can be looked for (a `tx!` text).
pub fn discover(found: &mut dyn FnMut(Found)) -> Result<(), &'static str> {
    #[cfg(windows)]
    return win::discover(found);
    #[cfg(target_os = "macos")]
    return mac::discover(found);
    #[cfg(all(unix, not(target_os = "macos")))]
    return linux::discover(found);
}

/// An item of Windows' Network folder: a computer if its parsing name is `\\NAME`.
#[cfg_attr(not(windows), allow(dead_code))]
fn computer(parsing: &str, display: &str) -> Option<Found> {
    let name = parsing.strip_prefix(r"\\").filter(|n| !n.is_empty() && !n.contains('\\'))?;
    let shown = if display.trim().is_empty() { name } else { display };
    Some(Found { name: shown.to_owned(), address: name.to_owned() })
}

/// One line of `avahi-browse -p -r`: a resolved service (`=;eth0;IPv4;Name;_smb._tcp;local;
/// host.local;192.168.1.5;445;…`) gives its name and host.
#[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))]
fn avahi_line(line: &str) -> Option<Found> {
    let fields: Vec<&str> = line.split(';').collect();
    if fields.first() != Some(&"=") || fields.len() < 7 {
        return None;
    }
    let (name, host) = (unescape(fields[3]), unescape(fields[6]));
    (!name.is_empty() && !host.is_empty()).then(|| Found { name, address: host.trim_end_matches('.').to_owned() })
}

/// Avahi's escapes: `\DDD` a byte in decimal, `\x` the character x.
#[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))]
fn unescape(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let digits = bytes.get(i + 1..i + 4).filter(|d| d.iter().all(u8::is_ascii_digit));
        match (bytes[i], digits) {
            (b'\\', Some(d)) => {
                let value = d.iter().fold(0u32, |n, b| n * 10 + u32::from(b - b'0'));
                out.push(u8::try_from(value).unwrap_or(b'?'));
                i += 4;
            }
            (b'\\', None) if i + 1 < bytes.len() => {
                out.push(bytes[i + 1]);
                i += 2;
            }
            (byte, _) => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(windows)]
mod win {
    use super::{Found, computer};
    use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree};
    use windows::Win32::UI::Shell::{
        BHID_EnumItems, FOLDERID_NetworkFolder, IEnumShellItems, IShellItem, KF_FLAG_DEFAULT, SHGetKnownFolderItem,
        SIGDN, SIGDN_DESKTOPABSOLUTEPARSING, SIGDN_NORMALDISPLAY,
    };

    /// The shell's Network folder, as Explorer lists it (its own NetBIOS and WS-Discovery search).
    /// Nothing found (network discovery off, the folder not readable) is an empty list.
    pub fn discover(found: &mut dyn FnMut(Found)) -> Result<(), &'static str> {
        // SAFETY: the shell items live through the calls; each name string is freed once.
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let Ok(folder) = SHGetKnownFolderItem::<IShellItem>(&FOLDERID_NetworkFolder, KF_FLAG_DEFAULT, None) else {
                return Ok(());
            };
            let Ok(items) = folder.BindToHandler::<_, IEnumShellItems>(None, &BHID_EnumItems) else { return Ok(()) };
            let name = |item: &IShellItem, kind: SIGDN| {
                let text = item.GetDisplayName(kind).ok()?;
                let name = text.to_string().ok();
                CoTaskMemFree(Some(text.0 as *const _));
                name
            };
            loop {
                let mut item = [None];
                let mut fetched = 0;
                if items.Next(&mut item, Some(&mut fetched)).is_err() || fetched == 0 {
                    return Ok(());
                }
                let Some(item) = item[0].take() else { continue };
                if let Some(parsing) = name(&item, SIGDN_DESKTOPABSOLUTEPARSING)
                    && let Some(computer) = computer(&parsing, &name(&item, SIGDN_NORMALDISPLAY).unwrap_or_default())
                {
                    found(computer);
                }
            }
        }
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use super::{DISCOVERY_TIME, Found};
    use std::ffi::{CStr, CString, c_char, c_void};
    use std::time::Instant;

    type ServiceRef = *mut c_void;
    type BrowseReply =
        extern "C" fn(ServiceRef, u32, u32, i32, *const c_char, *const c_char, *const c_char, *mut c_void);
    type ResolveReply =
        extern "C" fn(ServiceRef, u32, u32, i32, *const c_char, *const c_char, u16, u16, *const u8, *mut c_void);

    // dns_sd.h, in libSystem.
    unsafe extern "C" {
        fn DNSServiceBrowse(
            sd_ref: *mut ServiceRef,
            flags: u32,
            interface: u32,
            regtype: *const c_char,
            domain: *const c_char,
            reply: BrowseReply,
            context: *mut c_void,
        ) -> i32;
        fn DNSServiceResolve(
            sd_ref: *mut ServiceRef,
            flags: u32,
            interface: u32,
            name: *const c_char,
            regtype: *const c_char,
            domain: *const c_char,
            reply: ResolveReply,
            context: *mut c_void,
        ) -> i32;
        fn DNSServiceRefSockFD(sd_ref: ServiceRef) -> i32;
        fn DNSServiceProcessResult(sd_ref: ServiceRef) -> i32;
        fn DNSServiceRefDeallocate(sd_ref: ServiceRef);
    }

    const FLAG_ADD: u32 = 0x2;

    /// A service seen by the browse: its name, interface, type and domain (to resolve it).
    struct Seen {
        name: String,
        interface: u32,
        regtype: CString,
        domain: CString,
    }

    fn text(p: *const c_char) -> Option<CString> {
        (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) }.to_owned())
    }

    extern "C" fn browsed(
        _: ServiceRef,
        flags: u32,
        interface: u32,
        error: i32,
        name: *const c_char,
        regtype: *const c_char,
        domain: *const c_char,
        context: *mut c_void,
    ) {
        // SAFETY: `context` is the `Vec<Seen>` of `discover`, alive while results are processed.
        let seen = unsafe { &mut *(context as *mut Vec<Seen>) };
        if error != 0 || flags & FLAG_ADD == 0 {
            return;
        }
        if let (Some(name), Some(regtype), Some(domain)) = (text(name), text(regtype), text(domain)) {
            seen.push(Seen { name: name.to_string_lossy().into_owned(), interface, regtype, domain });
        }
    }

    extern "C" fn resolved(
        _: ServiceRef,
        _: u32,
        _: u32,
        error: i32,
        _: *const c_char,
        host: *const c_char,
        _: u16,
        _: u16,
        _: *const u8,
        context: *mut c_void,
    ) {
        // SAFETY: `context` is the `Option<String>` of `resolve`, alive while it waits.
        let out = unsafe { &mut *(context as *mut Option<String>) };
        if error == 0 && out.is_none() {
            *out = text(host).map(|h| h.to_string_lossy().trim_end_matches('.').to_owned());
        }
    }

    /// Waits for `sd_ref`'s answers until `done` or `until`, handing each to its reply.
    fn pump(sd_ref: ServiceRef, until: Instant, done: impl Fn() -> bool) {
        let fd = unsafe { DNSServiceRefSockFD(sd_ref) };
        while !done() {
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() || fd < 0 {
                return;
            }
            let mut poll = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
            let ms = i32::try_from(left.as_millis()).unwrap_or(i32::MAX).max(1);
            if unsafe { libc::poll(&mut poll, 1, ms) } <= 0 || unsafe { DNSServiceProcessResult(sd_ref) } != 0 {
                return;
            }
        }
    }

    /// The host behind a Bonjour service name ("Ali's MacBook" → `Alis-MacBook.local`).
    fn resolve(seen: &Seen, until: Instant) -> Option<String> {
        let name = CString::new(seen.name.as_str()).ok()?;
        let mut host: Option<String> = None;
        let mut sd_ref: ServiceRef = std::ptr::null_mut();
        let context = (&mut host as *mut Option<String>).cast();
        // SAFETY: the strings and `host` outlive the ref, deallocated before returning.
        let error = unsafe {
            DNSServiceResolve(
                &mut sd_ref,
                0,
                seen.interface,
                name.as_ptr(),
                seen.regtype.as_ptr(),
                seen.domain.as_ptr(),
                resolved,
                context,
            )
        };
        if error != 0 {
            return None;
        }
        // One second each at most: a silent machine does not hold up the others.
        let until = until.min(Instant::now() + std::time::Duration::from_secs(1));
        pump(sd_ref, until, || unsafe { (*(context as *const Option<String>)).is_some() });
        unsafe { DNSServiceRefDeallocate(sd_ref) };
        host
    }

    pub fn discover(found: &mut dyn FnMut(Found)) -> Result<(), &'static str> {
        let until = Instant::now() + DISCOVERY_TIME;
        let mut seen: Vec<Seen> = Vec::new();
        let mut sd_ref: ServiceRef = std::ptr::null_mut();
        let context = (&mut seen as *mut Vec<Seen>).cast();
        // SAFETY: `seen` outlives the ref, deallocated before returning.
        let error =
            unsafe { DNSServiceBrowse(&mut sd_ref, 0, 0, c"_smb._tcp".as_ptr(), c"local.".as_ptr(), browsed, context) };
        if error != 0 {
            return Ok(());
        }
        while Instant::now() < until {
            // Short waits, so each service is resolved and shown as it comes.
            let step = until.min(Instant::now() + std::time::Duration::from_millis(200));
            pump(sd_ref, step, || unsafe { !(*(context as *const Vec<Seen>)).is_empty() });
            let new: Vec<Seen> = std::mem::take(unsafe { &mut *(context as *mut Vec<Seen>) });
            for one in &new {
                if let Some(address) = resolve(one, until) {
                    found(Found { name: one.name.clone(), address });
                }
            }
        }
        unsafe { DNSServiceRefDeallocate(sd_ref) };
        Ok(())
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod linux {
    use super::{DISCOVERY_TIME, Found, NO_AVAHI, avahi_line};
    use crate::process::ChildProcess;
    use std::time::{Duration, Instant};

    /// `avahi-browse` (avahi-utils) lists the `_smb._tcp` services and resolves their hosts.
    // shortcut: the command, not Avahi's D-Bus API (no D-Bus crate here); use D-Bus if one comes in.
    pub fn discover(found: &mut dyn FnMut(Found)) -> Result<(), &'static str> {
        let until = Instant::now() + DISCOVERY_TIME;
        let mut child = ChildProcess::spawn(std::path::Path::new("avahi-browse"), ["-tpr", "_smb._tcp"], None)
            .map_err(|_| NO_AVAHI)?;
        let mut lines = child.stdout_lines();
        let mut any = false;
        let mut hand = |line: String, any: &mut bool| {
            if let Some(one) = avahi_line(&line) {
                *any = true;
                found(one);
            }
        };
        loop {
            for line in lines.ready() {
                hand(line, &mut any);
            }
            match child.try_wait() {
                Ok(Some(status)) => {
                    // Ended: what is left of its output is there to read.
                    for line in lines.by_ref() {
                        hand(line, &mut any);
                    }
                    // "Daemon not running" (and no avahi-browse) end in failure.
                    return if status.success() || any { Ok(()) } else { Err(NO_AVAHI) };
                }
                Ok(None) if Instant::now() < until => std::thread::sleep(Duration::from_millis(100)),
                _ => {
                    child.kill_tree();
                    return Ok(());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_computers_of_the_network_folder_are_kept() {
        let found = computer(r"\\NAS", "nas");
        assert_eq!(found, Some(Found { name: "nas".into(), address: "NAS".into() }));
        assert_eq!(computer(r"\\NAS", "").unwrap().name, "NAS");
        assert_eq!(computer("::{F02C1A0D-BE21-4350-88B0-7367FC96EF3C}", "Printer"), None);
        assert_eq!(computer("uuid:1234", "Media server"), None);
        assert_eq!(computer(r"\\NAS\share", "share"), None);
        assert_eq!(computer(r"\\", "x"), None);
    }

    #[test]
    fn avahi_resolved_lines_give_name_and_host() {
        let line = r#"=;eth0;IPv4;Ali\039s\032NAS;_smb._tcp;local;ali-nas.local;192.168.1.5;445;"""#;
        assert_eq!(avahi_line(line), Some(Found { name: "Ali's NAS".into(), address: "ali-nas.local".into() }));
        assert_eq!(avahi_line("+;eth0;IPv4;NAS;_smb._tcp;local"), None, "seen but not resolved");
        assert_eq!(avahi_line("=;eth0;IPv4;NAS"), None);
        assert_eq!(unescape(r"a\.b\\c\195\182"), "a.b\\cö");
    }
}
