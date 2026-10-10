//! org.freedesktop.FileManager1 (spec 6.4, 10.6): "Show in folder" from browsers and other
//! apps. Only opens, selects and shows; only `file://` and `trash:` URIs; at most 1,000.

use std::path::PathBuf;

use crate::linux::dbus::{Message, MsgKind, Value, answer};

#[cfg_attr(not(all(unix, not(target_os = "macos"))), allow(dead_code))]
pub const NAME: &str = "org.freedesktop.FileManager1";
pub const PATH: &str = "/org/freedesktop/FileManager1";
pub const INTERFACE: &str = "org.freedesktop.FileManager1";
const MAX_URIS: usize = 1000;
const INTROSPECTION: &str = r#"<node><interface name="org.freedesktop.FileManager1"><method name="ShowFolders"><arg type="as" direction="in"/><arg type="s" direction="in"/></method><method name="ShowItems"><arg type="as" direction="in"/><arg type="s" direction="in"/></method><method name="ShowItemProperties"><arg type="as" direction="in"/><arg type="s" direction="in"/></method></interface><interface name="org.freedesktop.DBus.Introspectable"><method name="Introspect"><arg type="s" direction="out"/></method></interface><interface name="org.freedesktop.DBus.Peer"><method name="Ping"/></interface></node>"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Show {
    Folders,
    Items,
    /// Decision 18: as Items.
    Properties,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub show: Show,
    pub paths: Vec<PathBuf>,
    pub trash: bool,
}

/// What a message asks for, and the reply to send (none for signals and replies).
pub fn dispatch(m: &Message) -> (Option<Call>, Option<Message>) {
    if m.kind != MsgKind::Call {
        return (None, None);
    }
    if m.path.as_deref() != Some(PATH) {
        return (None, answer(m, Some(("org.freedesktop.DBus.Error.UnknownObject", "no such object")), Vec::new()));
    }
    match (m.interface.as_deref(), m.member.as_deref()) {
        (Some("org.freedesktop.DBus.Introspectable"), Some("Introspect")) => {
            (None, answer(m, None, vec![Value::Str(INTROSPECTION.into())]))
        }
        (Some("org.freedesktop.DBus.Peer"), Some("Ping")) => (None, answer(m, None, Vec::new())),
        (Some(INTERFACE), Some(member @ ("ShowFolders" | "ShowItems" | "ShowItemProperties"))) => {
            let [Value::Array(elem, uris), Value::Str(_)] = m.body.as_slice() else {
                return (
                    None,
                    answer(m, Some(("org.freedesktop.DBus.Error.InvalidArgs", "expected (as, s)")), Vec::new()),
                );
            };
            if elem != "s" {
                return (
                    None,
                    answer(m, Some(("org.freedesktop.DBus.Error.InvalidArgs", "expected (as, s)")), Vec::new()),
                );
            }
            let show = match member {
                "ShowFolders" => Show::Folders,
                "ShowItems" => Show::Items,
                _ => Show::Properties,
            };
            let mut call = Call { show, paths: Vec::new(), trash: false };
            for uri in uris.iter().take(MAX_URIS) {
                let Value::Str(uri) = uri else { continue };
                if uri.starts_with("trash:") {
                    call.trash = true;
                } else if let Some(path) = crate::linux::uri::path_from_uri(uri)
                    // A NUL (from %00) cannot be in a real path.
                    && !path.as_os_str().as_encoded_bytes().contains(&0)
                {
                    call.paths.push(path);
                }
            }
            (Some(call), answer(m, None, Vec::new()))
        }
        _ => (
            None,
            answer(m, Some(("org.freedesktop.DBus.Error.UnknownMethod", "Gezik does not answer this")), Vec::new()),
        ),
    }
}

/// While held, Gezik answers as FileManager1; dropped, the connection closes and the name goes.
#[cfg(all(unix, not(target_os = "macos")))]
pub struct Owner(std::os::unix::net::UnixStream);

#[cfg(all(unix, not(target_os = "macos")))]
impl Drop for Owner {
    fn drop(&mut self) {
        let _ = self.0.shutdown(std::net::Shutdown::Both);
    }
}

/// Takes the name (DO_NOT_QUEUE: if another file manager has it, `AlreadyExists`) and answers
/// calls on a thread of its own (no CPU while idle). A broken message closes the connection.
/// Blocks up to a few seconds on a hung bus: call it off the UI thread.
#[cfg(all(unix, not(target_os = "macos")))]
pub fn serve(on_call: impl Fn(Call) + Send + 'static) -> std::io::Result<Owner> {
    use crate::linux::dbus::Bus;
    let mut bus = Bus::session()?;
    let code = bus.request_name(NAME)?;
    bus.stream.set_read_timeout(None)?;
    // 1 primary owner, 4 already the owner.
    if !matches!(code, 1 | 4) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "another file manager answers Show in folder",
        ));
    }
    let owner = Owner(bus.stream.try_clone()?);
    let guard = Owner(bus.stream.try_clone()?);
    std::thread::Builder::new().name("gezik-filemanager1".into()).spawn(move || {
        // Shuts the socket down on any exit, a panic in on_call too: the Owner's clone would
        // otherwise keep the name with nobody answering.
        let _guard = guard;
        while let Ok(message) = bus.read() {
            let (call, reply) = dispatch(&message);
            if let Some(reply) = reply
                && bus.send(reply).is_err()
            {
                break;
            }
            if let Some(call) = call {
                on_call(call);
            }
        }
    })?;
    Ok(owner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linux::dbus::{Message, MsgKind, Value};

    fn call(member: &str, uris: &[&str]) -> Message {
        Message {
            kind: MsgKind::Call,
            serial: 5,
            sender: Some(":1.42".into()),
            path: Some(PATH.into()),
            interface: Some(INTERFACE.into()),
            member: Some(member.into()),
            body: vec![
                Value::Array("s".into(), uris.iter().map(|u| Value::Str((*u).into())).collect()),
                Value::Str(String::new()),
            ],
            ..Message::default()
        }
    }

    #[test]
    fn show_items_opens_file_uris_and_the_trash_only() {
        let (call_, reply) =
            dispatch(&call("ShowItems", &["file:///home/u/a%20b.txt", "https://x", "trash:///", "file://host/x"]));
        let call_ = call_.unwrap();
        assert_eq!(call_.show, Show::Items);
        assert_eq!(call_.paths, vec![PathBuf::from("/home/u/a b.txt")]);
        assert!(call_.trash);
        let reply = reply.unwrap();
        assert_eq!(
            (reply.kind, reply.reply_serial, reply.destination.as_deref()),
            (MsgKind::Return, Some(5), Some(":1.42"))
        );
    }

    #[test]
    fn other_calls_get_errors_and_introspection_answers() {
        let (none, reply) = dispatch(&call("Explode", &[]));
        assert!(none.is_none());
        assert_eq!(reply.unwrap().error_name.as_deref(), Some("org.freedesktop.DBus.Error.UnknownMethod"));
        let wrong = Message { body: vec![Value::Str("x".into())], ..call("ShowFolders", &[]) };
        assert_eq!(dispatch(&wrong).1.unwrap().error_name.as_deref(), Some("org.freedesktop.DBus.Error.InvalidArgs"));
        let intro = Message {
            interface: Some("org.freedesktop.DBus.Introspectable".into()),
            member: Some("Introspect".into()),
            body: vec![],
            ..call("", &[])
        };
        let Value::Str(xml) = &dispatch(&intro).1.unwrap().body[0] else { panic!() };
        assert!(xml.contains("ShowItemProperties"));
        let quiet = Message { flags: crate::linux::dbus::NO_REPLY_EXPECTED, ..call("ShowFolders", &["file:///x"]) };
        assert!(dispatch(&quiet).1.is_none(), "no reply when none is expected");
        let signal = Message { kind: MsgKind::Signal, ..call("NameAcquired", &[]) };
        assert_eq!(dispatch(&signal), (None, None));
        let many: Vec<String> = (0..2000).map(|i| format!("file:///x{i}")).collect();
        let refs: Vec<&str> = many.iter().map(String::as_str).collect();
        assert_eq!(dispatch(&call("ShowFolders", &refs)).0.unwrap().paths.len(), 1000);
    }

    #[test]
    fn only_our_object_and_string_arrays_are_served() {
        let elsewhere = Message { path: Some("/".into()), ..call("ShowFolders", &["file:///x"]) };
        let (none, reply) = dispatch(&elsewhere);
        assert!(none.is_none());
        assert_eq!(reply.unwrap().error_name.as_deref(), Some("org.freedesktop.DBus.Error.UnknownObject"));
        let paths = Message {
            body: vec![Value::Array("o".into(), vec![Value::Path("/x".into())]), Value::Str(String::new())],
            ..call("ShowFolders", &[])
        };
        let (none, reply) = dispatch(&paths);
        assert!(none.is_none());
        assert_eq!(reply.unwrap().error_name.as_deref(), Some("org.freedesktop.DBus.Error.InvalidArgs"));
    }

    #[test]
    fn a_nul_in_a_path_is_skipped() {
        let (call_, _) = dispatch(&call("ShowItems", &["file:///a%00b", "file:///ok"]));
        assert_eq!(call_.unwrap().paths, vec![PathBuf::from("/ok")]);
    }
}
