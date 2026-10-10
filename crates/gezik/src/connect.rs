//! Connect to Server (spec 9 §7.4): an address over the window, the last ten addresses, and
//! the system's own login prompt (Linux: Gezik's password field, to gio's stdin only). The
//! connection runs on a background thread, one at a time; nothing runs until the user asks.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;

use gezik_config::settings::SERVERS_MAX;
use gezik_config::store::ConfigStore;
use gezik_core::nav::Location;
use gezik_platform::network::{self, Connect, ConnectError, Login, ServerAddress};

const TITLE: &str = "Connect to Server";

thread_local! {
    static RECENT: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static STORE: RefCell<Option<ConfigStore>> = const { RefCell::new(None) };
    static BUSY: Cell<bool> = const { Cell::new(false) };
}

/// The addresses of last time (`state.toml` `[servers] recent`) and where to save new ones.
pub fn install(store: Option<ConfigStore>, recent: Vec<String>) {
    RECENT.with(|r| *r.borrow_mut() = recent);
    STORE.with(|s| *s.borrow_mut() = store);
}

pub fn recent() -> Vec<String> {
    RECENT.with(|r| r.borrow().clone())
}

/// `text` to the front of `list`, once (case ignored), ten at most.
pub(crate) fn remember(list: &mut Vec<String>, text: String) {
    list.retain(|old| old.to_lowercase() != text.to_lowercase());
    list.insert(0, text);
    list.truncate(SERVERS_MAX);
}

/// What the recent list keeps of `address`: no user name either, so no login of any kind
/// reaches `state.toml` (spec 9 §10.5).
pub(crate) fn kept(address: &ServerAddress, windows: bool) -> String {
    ServerAddress { user: String::new(), ..address.clone() }.shown(windows)
}

/// The line under the field: where the address leads, or what is wrong with it.
pub(crate) fn note_for(text: &str, windows: bool) -> (String, bool) {
    if text.trim().is_empty() {
        return (network::EMPTY.to_owned(), false);
    }
    match network::parse_address(text, windows) {
        Ok(address) => (format!("Opens {}", address.shown(windows)), false),
        Err(why) => (why.to_owned(), true),
    }
}

/// Windows also offers to map the share to `letter` (Z down to D); Cancel is last (Esc).
pub(crate) fn buttons(windows: bool, letter: Option<char>) -> Vec<String> {
    match (windows, letter) {
        (true, Some(letter)) => vec!["Connect".to_owned(), format!("Map to {letter}:"), "Cancel".to_owned()],
        _ => vec!["Connect".to_owned(), "Cancel".to_owned()],
    }
}

/// The action and the menus' Connect to Server…: the field starts with the last address.
pub fn open() {
    let windows = cfg!(windows);
    // `drive_signature` is `GetLogicalDrives` on Windows.
    let letter = if windows { network::free_letter(gezik_platform::drive_signature() as u32) } else { None };
    let buttons = buttons(windows, letter);
    let initial = recent().into_iter().next().unwrap_or_default();
    crate::operations::with_current(|ops| {
        let names: Vec<&str> = buttons.iter().map(String::as_str).collect();
        ops.dialogs().ask_text_choice(
            TITLE,
            "",
            initial,
            &names,
            move |text| note_for(text, windows),
            move |answer| {
                if let Some((choice, text)) = answer {
                    start(&text, if choice == 1 { letter } else { None });
                }
            },
        );
    });
}

/// A recent address from This PC's menu, without the field.
pub fn open_recent(index: usize) {
    if let Some(text) = recent().get(index) {
        start(text, None);
    }
}

fn start(text: &str, letter: Option<char>) {
    match network::parse_address(text, cfg!(windows)) {
        Ok(address) => {
            let letter = letter_for(&address, letter);
            run(address, letter, None)
        }
        Err(why) => status(why.to_owned()),
    }
}

/// A bare `\\server` is not mapped to a letter: it is connected and its shares listed.
fn letter_for(address: &ServerAddress, letter: Option<char>) -> Option<char> {
    letter.filter(|_| !address.share.is_empty())
}

/// Hands the worker's result to `post` when dropped, and a failure if there is none (the
/// worker panicked or never ran), so `finish` always runs and `BUSY` never sticks.
struct Reply<F: FnOnce(Result<PathBuf, ConnectError>)> {
    post: Option<F>,
    result: Option<Result<PathBuf, ConnectError>>,
}

impl<F: FnOnce(Result<PathBuf, ConnectError>)> Reply<F> {
    /// Takes `self` whole: a closure that only set the field would capture the field alone.
    fn send(mut self, result: Result<PathBuf, ConnectError>) {
        self.result = Some(result);
    }
}

impl<F: FnOnce(Result<PathBuf, ConnectError>)> Drop for Reply<F> {
    fn drop(&mut self) {
        if let Some(post) = self.post.take() {
            post(self.result.take().unwrap_or_else(|| Err(ConnectError::Failed("Connection failed".to_owned()))));
        }
    }
}

fn run(address: ServerAddress, letter: Option<char>, login: Option<Login>) {
    if BUSY.with(|b| b.replace(true)) {
        // `login` goes here: its password is wiped.
        return status("Still connecting; try again when it is done".to_owned());
    }
    let shown = address.shown(cfg!(windows));
    status(format!("Connecting to {shown}…"));
    let mut owner = 0;
    crate::panes::with_active(|p| owner = p.nav.owner());
    let target = address.clone();
    // shortcut: if the event loop is gone, `finish` cannot run; the app is closing then.
    let reply = Reply {
        post: Some(move |result| {
            let _ = slint::invoke_from_event_loop(move || finish(address, shown, result));
        }),
        result: None,
    };
    // A thread that cannot start drops `reply` (and the login) at once: `finish` still runs.
    let _ = std::thread::Builder::new().spawn(move || {
        let how = Connect { letter, owner, login };
        let result = network::connect(&target, &how);
        // The password goes now, not when the answer is shown.
        drop(how);
        reply.send(result);
    });
}

fn finish(address: ServerAddress, shown: String, result: Result<PathBuf, ConnectError>) {
    BUSY.with(|b| b.set(false));
    match result {
        Ok(folder) => {
            save(kept(&address, cfg!(windows)));
            // A mapped letter, a macOS volume or a gvfs share: the drives changed.
            crate::sidebar::with_current(|sidebar| sidebar.check_drives(true));
            crate::panes::with_active(|p| p.nav.go(Location::Path(folder)));
        }
        Err(ConnectError::NeedsLogin) => ask_login(address),
        Err(ConnectError::Cancelled) => status("Not connected".to_owned()),
        Err(ConnectError::Failed(why)) => status(format!("Cannot connect to {shown}: {why}")),
    }
}

/// Only an address that worked is kept (spec 9 §7.4).
fn save(text: String) {
    let list = RECENT.with(|r| {
        let mut r = r.borrow_mut();
        remember(&mut r, text);
        r.clone()
    });
    STORE.with(|s| {
        if let Some(store) = s.borrow().as_ref() {
            store.update_state(move |state| state.servers_recent = list);
        }
    });
}

/// Linux: gio wants a login: the user name, then the password (never on a command line,
/// never kept, spec 9 §10.5).
fn ask_login(address: ServerAddress) {
    let user = if address.user.is_empty() { std::env::var("USER").unwrap_or_default() } else { address.user.clone() };
    crate::operations::with_current(|ops| {
        let dialogs = ops.dialogs().clone();
        let message = format!("User name for {}", kept(&address, false));
        ops.dialogs().ask_text(TITLE, message, user, &["Next", "Cancel"], move |user| {
            let Some(user) = user else { return };
            dialogs.ask_secret(TITLE, format!("Password for {user}"), &["Connect", "Cancel"], move |password| {
                if let Some(password) = password {
                    run(address, None, Some(Login { user, domain: String::new(), password }));
                }
            });
        });
    });
}

fn status(text: String) {
    crate::panes::with_active(|p| p.nav.status(text));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remember_keeps_ten_once() {
        let mut list: Vec<String> = (0..10).map(|i| format!(r"\\nas\s{i}")).collect();
        remember(&mut list, r"\\NAS\S5".to_owned());
        assert_eq!(list[0], r"\\NAS\S5", "to the front, as typed last");
        assert_eq!(list.iter().filter(|s| s.eq_ignore_ascii_case(r"\\nas\s5")).count(), 1, "once");
        remember(&mut list, "smb://new/x".to_owned());
        assert_eq!((list.len(), list[0].as_str()), (10, "smb://new/x"));
        assert!(!list.contains(&r"\\nas\s9".to_owned()), "the oldest went");
    }

    #[test]
    fn connect_notes_and_buttons() {
        assert_eq!(note_for("", true), (network::EMPTY.to_owned(), false));
        assert_eq!(note_for("smb://nas/foto", true), (r"Opens \\nas\foto".to_owned(), false));
        assert_eq!(note_for(r"\\nas\foto", false), ("Opens smb://nas/foto".to_owned(), false));
        assert_eq!(note_for(r"\\nas", false), (network::NEED_SHARE.to_owned(), true));
        assert!(note_for("smb://a:b@nas/x", true).1);
        assert_eq!(buttons(true, Some('Z')), ["Connect", "Map to Z:", "Cancel"]);
        assert_eq!(buttons(true, None), ["Connect", "Cancel"]);
        assert_eq!(buttons(false, Some('Z')), ["Connect", "Cancel"]);
    }

    #[test]
    fn a_bare_server_is_not_mapped() {
        let bare = network::parse_address(r"\\nas", true).unwrap();
        let share = network::parse_address(r"\\nas\foto", true).unwrap();
        assert_eq!(letter_for(&bare, Some('Z')), None);
        assert_eq!(letter_for(&share, Some('Z')), Some('Z'));
        assert_eq!(letter_for(&share, None), None);
    }

    #[test]
    fn a_worker_that_panics_still_answers() {
        let (tx, rx) = std::sync::mpsc::channel();
        let reply = Reply { post: Some(move |r| tx.send(r).unwrap()), result: None };
        let worker = std::thread::spawn(move || {
            if reply.post.is_some() {
                panic!("the connection broke");
            }
            reply.send(Ok(PathBuf::new()));
        });
        assert!(worker.join().is_err());
        assert_eq!(rx.recv().unwrap(), Err(ConnectError::Failed("Connection failed".to_owned())));
        let (tx, rx) = std::sync::mpsc::channel();
        // The way `run` hands it over: moved into a thread, sent from there.
        let reply = Reply { post: Some(move |r| tx.send(r).unwrap()), result: None };
        std::thread::spawn(move || reply.send(Ok(PathBuf::from("Z:\\")))).join().unwrap();
        assert_eq!(rx.recv().unwrap(), Ok(PathBuf::from("Z:\\")));
    }

    #[test]
    fn the_recent_list_keeps_no_login() {
        for windows in [false, true] {
            let address = network::parse_address("smb://ali@nas/foto/2024", windows).unwrap();
            let text = kept(&address, windows);
            assert_eq!(text, if windows { r"\\nas\foto\2024" } else { "smb://nas/foto/2024" });
            assert!(!text.contains('@') && !text.contains("ali"), "{text}");
            // A password never parses, so it never reaches the list.
            assert!(network::parse_address("smb://ali:gizli@nas/foto", windows).is_err());
            assert!(network::parse_address("smb://ali%3Agizli@nas/foto", windows).is_err());
        }
    }
}
