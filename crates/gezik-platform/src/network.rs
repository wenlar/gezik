//! Network shares (spec 9 §7.4): a server's address as the user types it, a Windows server's
//! disk shares, and connecting to a share with the system's own login prompt. No discovery:
//! every server name comes from the user. Blocking calls are for background threads only.

use std::path::{Path, PathBuf};

pub use crate::process::Ran;

pub const EMPTY: &str = r"Type a server address, like \\server\share or smb://server/share";
pub const NEED_SHARE: &str = "Add the share: smb://server/share";
pub const PASSWORD: &str = "Leave the password out; you are asked for it";
pub const NOT_ADDRESS: &str = "Not a server address";
pub const NEEDS_GIO: &str = "Connecting to servers needs gvfs (the gio command).";
pub const WRONG_LOGIN: &str = "The user name or password was not accepted";
pub const NOT_FOUND: &str = "The server or share was not found";
pub const LINE_BREAK: &str = "The user name and password cannot hold a line break";

/// Most shares a server's answer may list (it is the server's text, spec 9 §10.6).
const MAX_SHARES: usize = 4096;
/// Most of a helper's message shown.
const MAX_MESSAGE: usize = 200;

/// `\\server\share\rest…` / `smb://user@server/share/rest…`, parts decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerAddress {
    pub server: String,
    /// Empty: the server alone (Windows lists its shares).
    pub share: String,
    pub rest: Vec<String>,
    /// From `smb://user@…` (macOS, Linux); Windows asks for it in its own window.
    pub user: String,
}

impl ServerAddress {
    /// `\\server\share` (`\\server` alone).
    pub fn share_unc(&self) -> String {
        if self.share.is_empty() { format!(r"\\{}", self.server) } else { format!(r"\\{}\{}", self.server, self.share) }
    }

    /// The whole address as a Windows path.
    pub fn unc(&self) -> String {
        let mut text = self.share_unc();
        for part in &self.rest {
            text.push('\\');
            text.push_str(part);
        }
        text
    }

    /// The whole address as an `smb://` URL, each part percent-encoded.
    pub fn smb_url(&self) -> String {
        // The user's `:` and `@` encoded: written back they never read as a password or host.
        let user = if self.user.is_empty() { String::new() } else { format!("{}@", encode(&self.user, b"")) };
        // The server keeps `:` (a port); it has no `@` (refused).
        let mut text = format!("smb://{user}{}", encode(&self.server, b":"));
        for part in std::iter::once(&self.share).filter(|s| !s.is_empty()).chain(&self.rest) {
            text.push('/');
            text.push_str(&encode(part, b":@"));
        }
        text
    }

    /// How Gezik writes it back (the recent list, the status bar): UNC on Windows.
    pub fn shown(&self, windows: bool) -> String {
        if windows { self.unc() } else { self.smb_url() }
    }

    /// The part under the share, to go to once connected.
    pub fn rest_path(&self) -> PathBuf {
        self.rest.iter().collect()
    }
}

/// Reads what the user typed: `\\server\share…`, `//server/share…`, `server/share…` or
/// `smb://[user@]server/share…`. A server alone only on Windows (its shares are listed).
pub fn parse_address(text: &str, windows: bool) -> Result<ServerAddress, &'static str> {
    let text = text.trim();
    if text.is_empty() {
        return Err(EMPTY);
    }
    if text.chars().any(char::is_control) {
        return Err(NOT_ADDRESS);
    }
    let url = text.get(..6).is_some_and(|scheme| scheme.eq_ignore_ascii_case("smb://"));
    let (user, parts) = if url {
        let rest = &text[6..];
        let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
        let (user, server) = authority.rsplit_once('@').unwrap_or(("", authority));
        // Decoded first: `ali%3Agizli` is a password too.
        let user = decode(user)?;
        if user.contains(':') {
            return Err(PASSWORD);
        }
        let mut parts = vec![decode(server)?];
        for part in path.split('/').filter(|p| !p.is_empty()) {
            parts.push(decode(part)?);
        }
        (user, parts)
    } else {
        let body = text.trim_start_matches(['\\', '/']);
        // Two leading separators or none: `\\nas\foto`, `//nas/foto`, `nas/foto`.
        if !matches!(text.len() - body.len(), 0 | 2) {
            return Err(NOT_ADDRESS);
        }
        (String::new(), body.split(['\\', '/']).filter(|p| !p.is_empty()).map(str::to_owned).collect())
    };
    let mut parts = parts.into_iter();
    let server = parts.next().unwrap_or_default();
    let share = parts.next().unwrap_or_default();
    let rest: Vec<String> = parts.collect();
    // A part that is `.`/`..`, hides a separator (`%2F`) or a control character.
    let bad = |p: &str| p == "." || p == ".." || p.contains(['\\', '/']) || p.chars().any(char::is_control);
    let bad_server = server.is_empty()
        || server == "?"
        || server.chars().any(char::is_whitespace)
        // `\\evil@nas`, `smb://nas%40x`: a login hidden in the server.
        || server.contains('@')
        // `C:\x` is a path, not a server.
        || (server.contains(':') && !url)
        || bad(&server);
    if bad_server || bad(&share) || rest.iter().any(|p| bad(p)) || user.chars().any(char::is_control) {
        return Err(NOT_ADDRESS);
    }
    if share.is_empty() && !windows {
        return Err(NEED_SHARE);
    }
    Ok(ServerAddress { server, share, rest, user })
}

/// `%XX` escapes decoded; a broken escape or bytes that are not UTF-8 are no address.
fn decode(part: &str) -> Result<String, &'static str> {
    let bytes = part.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = bytes.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok());
            let value = hex.and_then(|h| u8::from_str_radix(h, 16).ok()).ok_or(NOT_ADDRESS)?;
            out.push(value);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| NOT_ADDRESS)
}

/// Everything but the URL's plain characters (and `keep`) as `%XX` (spaces, non-ASCII letters).
fn encode(part: &str, keep: &[u8]) -> String {
    let mut out = String::with_capacity(part.len());
    for &b in part.as_bytes() {
        if b.is_ascii_alphanumeric() || b"-._~!$&'()*+,;=".contains(&b) || keep.contains(&b) {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// A Linux login for gio; its password is wiped when it goes (spec 9 §10.5).
pub struct Login {
    pub user: String,
    pub domain: String,
    pub password: String,
}

impl Drop for Login {
    fn drop(&mut self) {
        wipe(&mut self.password);
    }
}

/// Overwrites `text` with zero bytes (still valid UTF-8) in a way the compiler keeps.
pub fn wipe(text: &mut str) {
    // SAFETY: zero bytes are valid UTF-8; the length does not change.
    for byte in unsafe { text.as_bytes_mut() } {
        unsafe { std::ptr::write_volatile(byte, 0) };
    }
}

/// How to connect: Windows maps `letter` (lasting, reconnected at sign-in) and owns its login
/// window by `owner`; Linux sends `login` to gio.
pub struct Connect {
    pub letter: Option<char>,
    pub owner: isize,
    pub login: Option<Login>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectError {
    /// Linux: gio asked for a user and password; ask them and try again.
    NeedsLogin,
    Cancelled,
    Failed(String),
}

/// A server's shares worth listing: disk shares, not the hidden (`C$`, `ADMIN$`) or special
/// ones, not printers or IPC; sorted, at most [`MAX_SHARES`].
pub fn visible_shares(found: Vec<(String, u32)>) -> Vec<String> {
    const TYPE_MASK: u32 = 0xFF;
    const SPECIAL: u32 = 0x8000_0000;
    let mut names: Vec<String> = found
        .into_iter()
        .filter(|(name, kind)| kind & TYPE_MASK == 0 && kind & SPECIAL == 0 && !name.is_empty() && !name.ends_with('$'))
        .map(|(name, _)| name)
        .take(MAX_SHARES)
        .collect();
    names.sort_by_key(|name| name.to_lowercase());
    names
}

/// The first drive letter not in `mask` (`GetLogicalDrives`), from Z down to D (Explorer's
/// choice for a new mapping).
pub fn free_letter(mask: u32) -> Option<char> {
    (b'D'..=b'Z').rev().find(|l| mask & (1 << (l - b'A')) == 0).map(char::from)
}

/// Windows errors a login can fix: access denied, wrong password, not signed in, guest
/// access refused, logon failure.
pub fn needs_login(code: u32) -> bool {
    matches!(code, 5 | 86 | 1244 | 1272 | 1326)
}

pub fn windows_connect_error(code: u32) -> ConnectError {
    match code {
        1223 => ConnectError::Cancelled,
        86 | 1326 => ConnectError::Failed(WRONG_LOGIN.into()),
        53 | 67 => ConnectError::Failed(NOT_FOUND.into()),
        _ => ConnectError::Failed(std::io::Error::from_raw_os_error(code as i32).to_string()),
    }
}

/// NetFS's status: ECANCELED or userCanceledErr; EAUTH; an errno; else the number.
pub fn mac_connect_error(code: i32) -> ConnectError {
    match code {
        89 | -128 => ConnectError::Cancelled,
        80 => ConnectError::Failed(WRONG_LOGIN.into()),
        1.. => ConnectError::Failed(std::io::Error::from_raw_os_error(code).to_string()),
        _ => ConnectError::Failed(format!("Could not connect (error {code})")),
    }
}

/// What gio reads, in the order it asks for an SMB share: user, domain (empty: its own
/// default), password. **doğrulanacak** (Linux list).
pub fn gio_answers(login: &Login) -> String {
    format!("{}\n{}\n{}\n", login.user, login.domain, login.password)
}

/// Whether gio stopped at its login questions (it prints them; `LC_ALL=C`).
pub fn gio_wants_login(stdout: &str, stderr: &str) -> bool {
    let text = format!("{stdout}\n{stderr}").to_lowercase();
    text.contains("password") || text.contains("user [")
}

/// Server and share of a gvfs mount's folder name (`smb-share:server=nas,share=foto,…`).
pub fn gvfs_smb_fields(name: &str) -> Option<(String, String)> {
    let spec = name.strip_prefix("smb-share:")?;
    let (mut server, mut share) = (None, None);
    for pair in spec.split(',') {
        let (key, value) = pair.split_once('=')?;
        match key {
            "server" => server = Some(decode(value).ok()?),
            "share" => share = Some(decode(value).ok()?),
            _ => {}
        }
    }
    Some((server?, share?))
}

/// The first non-empty line of a helper's message, cut short.
fn first_line(text: &str) -> String {
    text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").chars().take(MAX_MESSAGE).collect()
}

/// Linux: `gio mount` the share (a login only on its stdin), then find its folder under
/// `gvfs` (`names` lists that folder). `run` is `process::run_with_input` (tests: a fake).
pub fn linux_connect(
    address: &ServerAddress,
    login: Option<&Login>,
    run: &mut dyn FnMut(&[&str], &str) -> std::io::Result<Ran>,
    gvfs: &Path,
    names: &dyn Fn() -> Vec<String>,
) -> Result<PathBuf, ConnectError> {
    // gio reads one answer per line: a line break would answer its next question.
    if login.is_some_and(|l| [&l.user, &l.domain, &l.password].iter().any(|f| f.contains(['\n', '\r', '\0']))) {
        return Err(ConnectError::Failed(LINE_BREAK.into()));
    }
    let url = address.smb_url();
    let mut answers = login.map(gio_answers).unwrap_or_default();
    let result = run(&["gio", "mount", url.as_str()], &answers);
    wipe(&mut answers);
    let ran = match result {
        Ok(ran) => ran,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Err(ConnectError::Failed(NEEDS_GIO.into())),
        Err(err) => return Err(ConnectError::Failed(err.to_string())),
    };
    if !ran.ok && !ran.stderr.contains("already mounted") {
        if login.is_none() && gio_wants_login(&ran.stdout, &ran.stderr) {
            return Err(ConnectError::NeedsLogin);
        }
        return Err(ConnectError::Failed(first_line(&ran.stderr)));
    }
    let same = |a: &str, b: &str| a.to_lowercase() == b.to_lowercase();
    names()
        .into_iter()
        .find(|name| gvfs_smb_fields(name).is_some_and(|(s, p)| same(&s, &address.server) && same(&p, &address.share)))
        .map(|name| gvfs.join(name).join(address.rest_path()))
        .ok_or_else(|| ConnectError::Failed("Connected, but the share's folder was not found".into()))
}

/// The window that owns the system's login prompt (Windows HWND; 0 elsewhere).
pub fn owner_of(window: &impl raw_window_handle::HasWindowHandle) -> isize {
    #[cfg(windows)]
    if let Ok(handle) = window.window_handle()
        && let raw_window_handle::RawWindowHandle::Win32(raw) = handle.as_raw()
    {
        return raw.hwnd.get();
    }
    let _ = window;
    0
}

/// A Windows server's disk shares (`\\server`, spec 9 §7.4). Blocking (the network).
#[cfg(windows)]
pub fn shares(server: &str) -> std::io::Result<Vec<String>> {
    use windows::Win32::NetworkManagement::NetManagement::{MAX_PREFERRED_LENGTH, NetApiBufferFree};
    use windows::Win32::Storage::FileSystem::{NetShareEnum, SHARE_INFO_1};
    use windows::core::HSTRING;
    let name = HSTRING::from(format!(r"\\{server}"));
    let mut buffer: *mut u8 = std::ptr::null_mut();
    let (mut read, mut total) = (0u32, 0u32);
    // SAFETY: level 1 fills SHARE_INFO_1s; the buffer is the API's, freed below on every path.
    let code = unsafe { NetShareEnum(&name, 1, &mut buffer, MAX_PREFERRED_LENGTH, &mut read, &mut total, None) };
    let found: Vec<(String, u32)> = if code == 0 && !buffer.is_null() {
        let items = unsafe { std::slice::from_raw_parts(buffer as *const SHARE_INFO_1, read as usize) };
        // A name with a lone surrogate is left out.
        items.iter().filter_map(|i| Some((unsafe { i.shi1_netname.to_string() }.ok()?, i.shi1_type.0))).collect()
    } else {
        Vec::new()
    };
    if !buffer.is_null() {
        unsafe { NetApiBufferFree(Some(buffer.cast())) };
    }
    if code != 0 {
        return Err(std::io::Error::from_raw_os_error(code as i32));
    }
    Ok(visible_shares(found))
}

#[cfg(not(windows))]
pub fn shares(_server: &str) -> std::io::Result<Vec<String>> {
    Err(std::io::ErrorKind::Unsupported.into())
}

/// Connects to `address` and says which folder to go to. Blocking: background threads only.
pub fn connect(address: &ServerAddress, how: &Connect) -> Result<PathBuf, ConnectError> {
    #[cfg(windows)]
    return win::connect(address, how);
    #[cfg(target_os = "macos")]
    {
        // Its login window is NetAuth's: no letter, owner or login here.
        let _ = how;
        mac::connect(address)
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let runtime = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", unsafe { libc::getuid() })));
        let gvfs = runtime.join("gvfs");
        let names = || {
            std::fs::read_dir(&gvfs)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        };
        linux_connect(address, how.login.as_ref(), &mut crate::process::run_with_input, &gvfs, &names)
    }
}

#[cfg(windows)]
mod win {
    use super::{Connect, ConnectError, ServerAddress, needs_login, windows_connect_error};
    use std::path::PathBuf;
    use windows::Win32::Foundation::{HWND, NO_ERROR};
    use windows::Win32::NetworkManagement::WNet::{
        CONNECT_INTERACTIVE, CONNECT_PROMPT, CONNECT_UPDATE_PROFILE, NETRESOURCEW, RESOURCETYPE_ANY, RESOURCETYPE_DISK,
        WNetAddConnection3W,
    };
    use windows::core::{HSTRING, PCWSTR, PWSTR};

    /// Straight to the UNC path first (one sign-in is often enough); a login problem, or a
    /// drive letter asked for, goes through Windows' own prompt (spec 9 §7.4).
    pub fn connect(address: &ServerAddress, how: &Connect) -> Result<PathBuf, ConnectError> {
        if how.letter.is_some() && address.share.is_empty() {
            return Err(ConnectError::Failed(r"Type a share to map, like \\server\share".into()));
        }
        if how.letter.is_none() {
            match reachable(address) {
                Ok(()) => return Ok(PathBuf::from(address.unc())),
                Err(code) if !needs_login(code) => return Err(windows_connect_error(code)),
                Err(_) => {}
            }
        }
        // A server alone: sign in to its IPC$, then its shares can be listed.
        let (remote, kind) = if address.share.is_empty() {
            (HSTRING::from(format!(r"\\{}\IPC$", address.server)), RESOURCETYPE_ANY)
        } else {
            (HSTRING::from(address.share_unc()), RESOURCETYPE_DISK)
        };
        let local = how.letter.map(|l| HSTRING::from(format!("{l}:")));
        let resource = NETRESOURCEW {
            dwType: kind,
            lpLocalName: local.as_ref().map_or(PWSTR::null(), |l| PWSTR(l.as_ptr() as *mut _)),
            lpRemoteName: PWSTR(remote.as_ptr() as *mut _),
            ..Default::default()
        };
        let mut flags = CONNECT_INTERACTIVE | CONNECT_PROMPT;
        if how.letter.is_some() {
            flags |= CONNECT_UPDATE_PROFILE;
        }
        // SAFETY: the strings live through the call; the prompt is Windows' window.
        let code = unsafe {
            WNetAddConnection3W(Some(HWND(how.owner as *mut _)), &resource, PCWSTR::null(), PCWSTR::null(), flags)
        };
        if code != NO_ERROR {
            return Err(windows_connect_error(code.0));
        }
        Ok(match how.letter {
            Some(letter) => PathBuf::from(format!("{letter}:\\")).join(address.rest_path()),
            None => PathBuf::from(address.unc()),
        })
    }

    /// Whether the share (or the server's list) opens as things are; the Win32 error if not.
    fn reachable(address: &ServerAddress) -> Result<(), u32> {
        let code = |e: std::io::Error| e.raw_os_error().unwrap_or(0) as u32;
        if address.share.is_empty() {
            super::shares(&address.server).map(|_| ()).map_err(code)
        } else {
            std::fs::read_dir(address.share_unc()).map(|_| ()).map_err(code)
        }
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use super::{ConnectError, ServerAddress, mac_connect_error};
    use objc2::rc::Retained;
    use objc2_foundation::{NSArray, NSMutableDictionary, NSString, NSURL};
    use std::ffi::c_void;
    use std::path::PathBuf;

    #[link(name = "NetFS", kind = "framework")]
    unsafe extern "C" {
        /// NetFS.h; the mount points array follows the Copy rule (ours to release).
        fn NetFSMountURLSync(
            url: *const c_void,
            mountpath: *const c_void,
            user: *const c_void,
            passwd: *const c_void,
            open_options: *const c_void,
            mount_options: *const c_void,
            mountpoints: *mut *const c_void,
        ) -> i32;
    }

    /// Mounts the share where macOS puts it (`/Volumes/…`), its login window allowed
    /// (`kNAUIOptionKey` = `kNAUIOptionAllowUI`; **doğrulanacak**). Finder does not open.
    pub fn connect(address: &ServerAddress) -> Result<PathBuf, ConnectError> {
        let Some(url) = NSURL::URLWithString(&NSString::from_str(&address.smb_url())) else {
            return Err(ConnectError::Failed(super::NOT_ADDRESS.into()));
        };
        let options = NSMutableDictionary::<NSString, NSString>::new();
        options.insert(&*NSString::from_str("UIOption"), &*NSString::from_str("AllowUI"));
        let mut points: *const c_void = std::ptr::null();
        // SAFETY: NSURL and NSMutableDictionary are toll-free bridged with CFURL and
        // CFMutableDictionary; `points` is filled with a +1 CFArray or left null.
        let status = unsafe {
            NetFSMountURLSync(
                Retained::as_ptr(&url).cast(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                Retained::as_ptr(&options).cast(),
                std::ptr::null(),
                &mut points,
            )
        };
        let points: Option<Retained<NSArray<NSString>>> =
            unsafe { Retained::from_raw(points as *mut NSArray<NSString>) };
        const EEXIST: i32 = 17;
        if status == EEXIST {
            // Mounted already: where macOS names it after the share (**doğrulanacak**).
            let there = PathBuf::from("/Volumes").join(&address.share);
            return if there.is_dir() { Ok(there.join(address.rest_path())) } else { Err(mac_connect_error(status)) };
        }
        if status != 0 {
            return Err(mac_connect_error(status));
        }
        points
            .and_then(|p| p.firstObject())
            .map(|first| PathBuf::from(first.to_string()).join(address.rest_path()))
            .ok_or_else(|| ConnectError::Failed("The server gave no folder".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn w(text: &str) -> Result<ServerAddress, &'static str> {
        parse_address(text, true)
    }

    #[test]
    fn addresses_parse() {
        assert_eq!(w(r"\\nas\foto").unwrap().unc(), r"\\nas\foto");
        assert_eq!(w("smb://nas/foto/2024").unwrap().unc(), r"\\nas\foto\2024");
        assert_eq!(w("SMB://nas/foto").unwrap().share, "foto", "the scheme in any case");
        assert_eq!(w("nas/foto").unwrap().share, "foto");
        assert_eq!(w("//nas/foto/").unwrap().unc(), r"\\nas\foto", "a trailing separator");
        assert_eq!(w(r"  \\nas\Fotoğraflar  ").unwrap().share, "Fotoğraflar");
        assert_eq!(w("smb://nas/My%20Share").unwrap().share, "My Share");
        let a = w(r"\\nas\foto\2024\yaz").unwrap();
        assert_eq!((a.share_unc(), a.rest_path()), (r"\\nas\foto".to_owned(), PathBuf::from("2024").join("yaz")));
        assert_eq!(w(r"\\nas").unwrap().unc(), r"\\nas", "a server alone: its shares (Windows)");
        assert_eq!(parse_address(r"\\nas", false), Err(NEED_SHARE), "macOS and Linux need the share");
        let user = parse_address("smb://W;ali@nas/foto", false).unwrap();
        assert_eq!((user.user.as_str(), user.smb_url().as_str()), ("W;ali", "smb://W;ali@nas/foto"));
        assert_eq!(w("smb://ali@nas/foto").unwrap().shown(true), r"\\nas\foto", "Windows asks the user itself");
    }

    #[test]
    fn a_password_in_the_address_is_refused() {
        assert_eq!(w("smb://ali:gizli@nas/foto"), Err(PASSWORD));
        assert_eq!(parse_address("smb://ali:@nas/foto", false), Err(PASSWORD));
        assert_eq!(w("smb://ali%3Agizli@nas/foto"), Err(PASSWORD), "an encoded colon too");
        assert_eq!(w(r"\\evil@nas\share"), Err(NOT_ADDRESS), "no user hidden in a server");
        assert_eq!(w("smb://nas%40evil/share"), Err(NOT_ADDRESS));
        assert_eq!(w("smb://a%3Ab%40c/share"), Err(NOT_ADDRESS));
    }

    #[test]
    fn user_and_server_cannot_turn_into_a_login() {
        // A user name with `@` (ali@corp) is kept, but written back encoded.
        let a = parse_address("smb://ali%40corp@nas/foto", false).unwrap();
        assert_eq!((a.user.as_str(), a.smb_url().as_str()), ("ali@corp", "smb://ali%40corp@nas/foto"));
        assert_eq!(parse_address(&a.smb_url(), false).unwrap(), a);
        let port = parse_address("smb://nas:445/foto", false).unwrap();
        assert_eq!(port.smb_url(), "smb://nas:445/foto", "a port stays a port");
        let path = parse_address("smb://nas/foto/a:b@c", false).unwrap();
        assert_eq!(path.smb_url(), "smb://nas/foto/a:b@c");
    }

    #[test]
    fn bad_addresses_are_refused() {
        assert_eq!(w(""), Err(EMPTY));
        assert_eq!(w("   "), Err(EMPTY));
        for bad in [
            r"\\",
            r"\nas\foto",
            r"\\\nas\foto",
            r"\\nas\..\x",
            r"\\nas\foto\.\x",
            "smb://nas/a%zz",
            "smb://nas/a%2Fb",
            "smb://nas/a%00b",
            "smb:///foto",
            "na s/foto",
            "a\u{7}b/c",
            r"\\?\C:\x",
            r"\\.\pipe\x",
            r"C:\x",
            "smb://nas/%C3",
        ] {
            assert!(w(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn smb_urls_round_trip() {
        let a = parse_address("smb://nas/Fotoğraf Arşivi/a b", false).unwrap();
        assert_eq!(a.smb_url(), "smb://nas/Foto%C4%9Fraf%20Ar%C5%9Fivi/a%20b");
        assert_eq!(parse_address(&a.smb_url(), false).unwrap(), a);
        assert_eq!(a.shown(false), a.smb_url());
        assert_eq!(a.shown(true), r"\\nas\Fotoğraf Arşivi\a b");
    }

    #[test]
    fn only_disk_shares_are_listed() {
        let found = vec![
            ("foto".to_owned(), 0),
            ("ADMIN$".to_owned(), 0x8000_0000),
            ("C$".to_owned(), 0x8000_0000),
            ("IPC$".to_owned(), 3 | 0x8000_0000),
            ("printer".to_owned(), 1),
            ("Belgeler".to_owned(), 0),
            ("gizli$".to_owned(), 0),
            (String::new(), 0),
            ("Arşiv".to_owned(), 0x4000_0000),
        ];
        assert_eq!(visible_shares(found), ["Arşiv", "Belgeler", "foto"]);
        let many = (0..5000).map(|i| (format!("s{i:04}"), 0)).collect();
        assert_eq!(visible_shares(many).len(), 4096, "a server's answer is bounded");
    }

    #[test]
    fn the_free_letter_is_counted_down_from_z() {
        assert_eq!(free_letter(0), Some('Z'));
        assert_eq!(free_letter(1 << 25), Some('Y'), "Z taken");
        assert_eq!(free_letter(0x03FF_FFF0), Some('D'), "E to Z taken");
        assert_eq!(free_letter(0x03FF_FFF8), None, "D to Z taken");
    }

    #[test]
    fn windows_answers() {
        assert!([5, 86, 1244, 1272, 1326].into_iter().all(needs_login));
        assert!(!needs_login(53), "no such server: a login does not help");
        assert_eq!(windows_connect_error(1223), ConnectError::Cancelled);
        assert_eq!(windows_connect_error(1326), ConnectError::Failed(WRONG_LOGIN.into()));
        assert_eq!(windows_connect_error(53), ConnectError::Failed(NOT_FOUND.into()));
        assert_eq!(windows_connect_error(67), ConnectError::Failed(NOT_FOUND.into()));
        assert!(matches!(windows_connect_error(85), ConnectError::Failed(_)));
        assert_eq!(mac_connect_error(89), ConnectError::Cancelled);
        assert_eq!(mac_connect_error(-128), ConnectError::Cancelled);
        assert_eq!(mac_connect_error(80), ConnectError::Failed(WRONG_LOGIN.into()));
        assert!(matches!(mac_connect_error(-5000), ConnectError::Failed(t) if t.contains("-5000")));
    }

    #[test]
    fn gio_answers_in_order() {
        let login = Login { user: "teo".into(), domain: String::new(), password: "pw".into() };
        assert_eq!(gio_answers(&login), "teo\n\npw\n");
        assert!(gio_wants_login("Authentication Required\nUser [teo]: ", ""));
        assert!(gio_wants_login("", "Password: "));
        assert!(!gio_wants_login("", "gio: smb://nas/: No such file or directory"));
    }

    #[test]
    fn gvfs_names_give_server_and_share() {
        let f = |n: &str| gvfs_smb_fields(n);
        assert_eq!(f("smb-share:server=nas,share=foto"), Some(("nas".into(), "foto".into())));
        assert_eq!(
            f("smb-share:domain=W,server=nas,share=My%20Share,user=teo"),
            Some(("nas".into(), "My Share".into()))
        );
        assert_eq!(f("smb-share:server=nas"), None);
        assert_eq!(f("sftp:host=x"), None);
        assert_eq!(f("smb-share:server=nas,share=a%zz"), None);
        assert_eq!(f("smb-share:"), None);
    }

    fn fake(log: &RefCell<Vec<(String, String)>>, ran: Ran) -> impl FnMut(&[&str], &str) -> std::io::Result<Ran> + '_ {
        move |args, input| {
            log.borrow_mut().push((args.join(" "), input.to_owned()));
            Ok(ran.clone())
        }
    }

    #[test]
    fn gio_asks_for_a_login_and_gets_it_on_stdin_only() {
        let address = parse_address("smb://nas/foto/2024", false).unwrap();
        let gvfs = Path::new("/run/user/1000/gvfs");
        let names = || vec!["smb-share:server=NAS,share=Foto".to_owned(), "sftp:host=x".to_owned()];
        let log = RefCell::new(Vec::new());
        let asks = Ran { ok: false, stdout: "Authentication Required\nUser [teo]: ".into(), stderr: String::new() };
        assert_eq!(linux_connect(&address, None, &mut fake(&log, asks), gvfs, &names), Err(ConnectError::NeedsLogin));
        let login = Login { user: "teo".into(), domain: String::new(), password: "s3cret".into() };
        let done = Ran { ok: true, ..Ran::default() };
        assert_eq!(
            linux_connect(&address, Some(&login), &mut fake(&log, done), gvfs, &names),
            Ok(gvfs.join("smb-share:server=NAS,share=Foto").join("2024")),
            "the share's folder, server and share in any case"
        );
        let log = log.into_inner();
        assert_eq!(log[0], ("gio mount smb://nas/foto/2024".to_owned(), String::new()));
        assert_eq!(log[1].1, "teo\n\ns3cret\n");
        assert!(log.iter().all(|(args, _)| !args.contains("s3cret")), "never on the command line");
    }

    #[test]
    fn a_login_with_a_line_break_is_refused() {
        let address = parse_address("smb://nas/foto", false).unwrap();
        let log = RefCell::new(Vec::new());
        let none = || Vec::new();
        for (user, domain, password) in
            [("teo\nx", "", "pw"), ("teo", "W\r", "pw"), ("teo", "", "p\nw"), ("teo", "", "p\0w")]
        {
            let login = Login { user: user.into(), domain: domain.into(), password: password.into() };
            let done = Ran { ok: true, ..Ran::default() };
            let got = linux_connect(&address, Some(&login), &mut fake(&log, done), Path::new("/g"), &none);
            assert_eq!(got, Err(ConnectError::Failed(LINE_BREAK.into())), "{user:?} {domain:?} {password:?}");
        }
        assert!(log.borrow().is_empty(), "gio never ran");
    }

    #[test]
    fn linux_connect_reports_gio() {
        let address = parse_address("smb://nas/foto", false).unwrap();
        let gvfs = Path::new("/g");
        let none = || Vec::new();
        let log = RefCell::new(Vec::new());
        let mut missing = |_: &[&str], _: &str| -> std::io::Result<Ran> { Err(std::io::ErrorKind::NotFound.into()) };
        assert_eq!(
            linux_connect(&address, None, &mut missing, gvfs, &none),
            Err(ConnectError::Failed(NEEDS_GIO.into()))
        );
        let refused = Ran {
            ok: false,
            stdout: String::new(),
            stderr: "gio: smb://nas/foto/: Failed to mount Windows share: No route to host\n".into(),
        };
        assert_eq!(
            linux_connect(&address, None, &mut fake(&log, refused), gvfs, &none),
            Err(ConnectError::Failed("gio: smb://nas/foto/: Failed to mount Windows share: No route to host".into()))
        );
        let already = Ran {
            ok: false,
            stdout: String::new(),
            stderr: "gio: smb://nas/foto/: Location is already mounted\n".into(),
        };
        let there = || vec!["smb-share:server=nas,share=foto".to_owned()];
        assert_eq!(
            linux_connect(&address, None, &mut fake(&log, already), gvfs, &there),
            Ok(gvfs.join("smb-share:server=nas,share=foto"))
        );
        let lost = Ran { ok: true, ..Ran::default() };
        assert!(matches!(
            linux_connect(&address, None, &mut fake(&log, lost), gvfs, &none),
            Err(ConnectError::Failed(_))
        ));
    }
}
