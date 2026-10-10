//! org.freedesktop.portal.GlobalShortcuts (spec 9 §9.2, Wayland): a session, one shortcut
//! bound with the user's chord as `preferred_trigger` (the desktop may ask the user and may
//! bind another key: deviation 15), then its Activated signals. No portal: `Unsupported`.

#![cfg_attr(not(all(unix, not(target_os = "macos"))), allow(dead_code))]

use crate::linux::dbus::{Message, MsgKind, Value, answer};

pub const DESKTOP: &str = "org.freedesktop.portal.Desktop";
pub const PATH: &str = "/org/freedesktop/portal/desktop";
pub const IFACE: &str = "org.freedesktop.portal.GlobalShortcuts";
pub const SHORTCUT_ID: &str = "show-gezik";
/// Only the portal's signals (the bus resolves the name to its owner): another app cannot
/// answer Gezik's requests or press its shortcut by broadcasting.
pub const RESPONSE_MATCH: &str = "type='signal',sender='org.freedesktop.portal.Desktop',interface='org.freedesktop.portal.Request',member='Response'";
pub const ACTIVATED_MATCH: &str = "type='signal',sender='org.freedesktop.portal.Desktop',interface='org.freedesktop.portal.GlobalShortcuts',member='Activated'";

fn entry(key: &str, value: Value) -> Value {
    Value::Entry(Box::new(Value::Str(key.into())), Box::new(Value::Variant(Box::new(value))))
}

fn portal_call(member: &str, body: Vec<Value>) -> Message {
    Message {
        destination: Some(DESKTOP.into()),
        path: Some(PATH.into()),
        interface: Some(IFACE.into()),
        member: Some(member.into()),
        body,
        ..Message::default()
    }
}

pub fn create_session(token: &str) -> Message {
    portal_call(
        "CreateSession",
        vec![Value::Array(
            "{sv}".into(),
            vec![
                entry("handle_token", Value::Str(token.into())),
                entry("session_handle_token", Value::Str(token.into())),
            ],
        )],
    )
}

pub fn bind(session: &str, trigger: &str, token: &str) -> Message {
    let shortcut = Value::Struct(vec![
        Value::Str(SHORTCUT_ID.into()),
        Value::Array(
            "{sv}".into(),
            vec![
                entry("description", Value::Str("Show or hide Gezik".into())),
                entry("preferred_trigger", Value::Str(trigger.into())),
            ],
        ),
    ]);
    portal_call(
        "BindShortcuts",
        vec![
            Value::Path(session.into()),
            Value::Array("(sa{sv})".into(), vec![shortcut]),
            Value::Str(String::new()),
            Value::Array("{sv}".into(), vec![entry("handle_token", Value::Str(token.into()))]),
        ],
    )
}

/// The request object a call's reply names (its Response comes later as a signal).
pub fn request_of(reply: &Message) -> Option<&str> {
    match reply.body.first() {
        Some(Value::Path(path)) => Some(path),
        _ => None,
    }
}

/// A Response signal of `request`: the code (0 done, 1 cancelled by the user, 2 other) and
/// the results.
pub fn response<'a>(m: &'a Message, request: &str) -> Option<(u32, &'a [Value])> {
    if m.kind != MsgKind::Signal || m.path.as_deref() != Some(request) || m.member.as_deref() != Some("Response") {
        return None;
    }
    match m.body.as_slice() {
        [Value::U32(code), Value::Array(_, results)] => Some((*code, results)),
        _ => None,
    }
}

fn result<'a>(results: &'a [Value], key: &str) -> Option<&'a Value> {
    results.iter().find_map(|item| match item {
        Value::Entry(k, v) if matches!(&**k, Value::Str(name) if name == key) => match &**v {
            Value::Variant(inner) => Some(&**inner),
            _ => None,
        },
        _ => None,
    })
}

pub fn session_handle(results: &[Value]) -> Option<String> {
    match result(results, "session_handle")? {
        Value::Str(handle) | Value::Path(handle) => Some(handle.clone()),
        _ => None,
    }
}

/// Whether BindShortcuts' results hold a shortcut.
pub fn bound(results: &[Value]) -> bool {
    matches!(result(results, "shortcuts"), Some(Value::Array(_, list)) if !list.is_empty())
}

/// Gezik's shortcut was pressed in `session`.
pub fn activated(m: &Message, session: &str) -> bool {
    m.kind == MsgKind::Signal
        && m.interface.as_deref() == Some(IFACE)
        && m.member.as_deref() == Some("Activated")
        && matches!(m.body.as_slice(), [Value::Path(s), Value::Str(id), ..] if s == session && id == SHORTCUT_ID)
}

/// The bus's error when there is no GlobalShortcuts portal.
pub fn unsupported(error: &str) -> bool {
    ["ServiceUnknown", "UnknownMethod", "UnknownInterface", "UnknownObject"].iter().any(|name| error.contains(name))
}

/// Gezik serves no object on this connection: a call to it gets an error, nothing else.
pub fn refuse(m: &Message) -> Option<Message> {
    if m.kind != MsgKind::Call {
        return None;
    }
    answer(m, Some(("org.freedesktop.DBus.Error.UnknownObject", "no such object")), Vec::new())
}

#[cfg(all(unix, not(target_os = "macos")))]
pub use serving::{Shortcut, bind_shortcut};

#[cfg(all(unix, not(target_os = "macos")))]
mod serving {
    use std::os::unix::net::UnixStream;
    use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::hotkey::{HotkeyError, OnPress, OnReady};
    use crate::linux::dbus::Bus;

    /// The bound shortcut while held: dropped, the connection closes and the portal ends the
    /// session. Drop never waits for the thread, so it may run inside `on_press` too.
    pub struct Shortcut {
        stream: Arc<Mutex<Option<UnixStream>>>,
        stop: Arc<AtomicBool>,
    }

    pub fn bind_shortcut(trigger: String, on_press: OnPress, on_ready: OnReady) -> Shortcut {
        let shortcut = Shortcut { stream: Arc::default(), stop: Arc::default() };
        let (slot, stop) = (shortcut.stream.clone(), shortcut.stop.clone());
        // shortcut: if no thread can be started, on_ready is never called (as hotkey/windows.rs).
        let _ = std::thread::Builder::new().name("gezik-portal".into()).spawn(move || {
            match set_up(&trigger, &slot, &stop) {
                Err(why) => on_ready(Err(why)),
                Ok((mut bus, session)) => {
                    on_ready(Ok(()));
                    while let Ok(message) = bus.read() {
                        if activated(&message, &session) {
                            on_press();
                        } else if let Some(reply) = refuse(&message)
                            && bus.send(reply).is_err()
                        {
                            break;
                        }
                    }
                }
            }
        });
        shortcut
    }

    fn failed(err: std::io::Error) -> HotkeyError {
        let text = err.to_string();
        if unsupported(&text) { HotkeyError::Unsupported } else { HotkeyError::Failed(text) }
    }

    fn set_up(
        trigger: &str,
        slot: &Mutex<Option<UnixStream>>,
        stop: &AtomicBool,
    ) -> Result<(Bus, String), HotkeyError> {
        let mut bus = Bus::session().map_err(failed)?;
        if let (Ok(clone), Ok(mut held)) = (bus.stream.try_clone(), slot.lock()) {
            *held = Some(clone);
        }
        if stop.load(SeqCst) {
            return Err(HotkeyError::Failed("turned off meanwhile".into()));
        }
        bus.add_match(RESPONSE_MATCH).map_err(failed)?;
        bus.add_match(ACTIVATED_MATCH).map_err(failed)?;
        let token = format!("gezik{}", std::process::id());
        // The desktop may ask the user now: no time limit (dropping the Shortcut ends the wait).
        bus.stream.set_read_timeout(None).map_err(failed)?;
        let (code, results) = request(&mut bus, create_session(&token))?;
        let session = session_handle(&results)
            .filter(|_| code == 0)
            .ok_or(HotkeyError::Failed("the desktop did not open a shortcut session".into()))?;
        let (code, results) = request(&mut bus, bind(&session, trigger, &format!("{token}b")))?;
        if code != 0 || !bound(&results) {
            return Err(HotkeyError::Failed("the desktop did not bind the shortcut".into()));
        }
        Ok((bus, session))
    }

    /// A portal call and its Response. A Response that comes before the call's reply is
    /// kept (the reply names the request it belongs to).
    fn request(bus: &mut Bus, call: Message) -> Result<(u32, Vec<Value>), HotkeyError> {
        let serial = bus.send(call).map_err(failed)?;
        let mut early: Vec<Message> = Vec::new();
        let request = loop {
            let message = bus.read().map_err(failed)?;
            if message.reply_serial == Some(serial) {
                match message.kind {
                    MsgKind::Error => {
                        return Err(failed(std::io::Error::other(message.error_name.unwrap_or_default())));
                    }
                    _ => match request_of(&message) {
                        Some(path) => break path.to_owned(),
                        None => return Err(HotkeyError::Failed("the portal's reply had no request".into())),
                    },
                }
            }
            if message.member.as_deref() == Some("Response") {
                // Bounded: only the portal's signals match, but a few are enough.
                if early.len() == 8 {
                    early.remove(0);
                }
                early.push(message);
            } else if let Some(reply) = refuse(&message) {
                bus.send(reply).map_err(failed)?;
            }
        };
        if let Some((code, results)) = early.iter().find_map(|m| response(m, &request)) {
            return Ok((code, results.to_vec()));
        }
        loop {
            let message = bus.read().map_err(failed)?;
            if let Some((code, results)) = response(&message, &request) {
                return Ok((code, results.to_vec()));
            }
            if let Some(reply) = refuse(&message) {
                bus.send(reply).map_err(failed)?;
            }
        }
    }

    impl Drop for Shortcut {
        fn drop(&mut self) {
            self.stop.store(true, SeqCst);
            if let Ok(mut held) = self.stream.lock()
                && let Some(stream) = held.take()
            {
                let _ = stream.shutdown(std::net::Shutdown::Both);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linux::dbus::{Message, MsgKind, Value, encode};

    fn entry(key: &str, value: Value) -> Value {
        Value::Entry(Box::new(Value::Str(key.into())), Box::new(Value::Variant(Box::new(value))))
    }

    fn signal(path: &str, member: &str, body: Vec<Value>) -> Message {
        Message {
            kind: MsgKind::Signal,
            path: Some(path.into()),
            member: Some(member.into()),
            body,
            ..Message::default()
        }
    }

    #[test]
    fn the_calls_encode_with_their_signatures() {
        let create = create_session("gezik42");
        assert_eq!(create.signature(), "a{sv}");
        assert_eq!(create.interface.as_deref(), Some(IFACE));
        assert!(encode(&Message { serial: 1, ..create }).is_ok());
        let bind = bind("/org/freedesktop/portal/desktop/session/1_9/gezik42", "SHIFT+LOGO+e", "gezik42b");
        assert_eq!(bind.signature(), "oa(sa{sv})sa{sv}");
        assert!(encode(&Message { serial: 2, ..bind.clone() }).is_ok());
        let Value::Array(_, shortcuts) = &bind.body[1] else { panic!() };
        let Value::Struct(parts) = &shortcuts[0] else { panic!() };
        assert_eq!(parts[0], Value::Str(SHORTCUT_ID.into()));
        assert!(format!("{:?}", parts[1]).contains("SHIFT+LOGO+e"), "the user's chord as preferred_trigger");
    }

    #[test]
    fn responses_and_activations_are_read() {
        let reply = Message { body: vec![Value::Path("/r/1".into())], ..Message::default() };
        assert_eq!(request_of(&reply), Some("/r/1"));
        let session = "/org/freedesktop/portal/desktop/session/1_9/gezik42";
        let done = signal(
            "/r/1",
            "Response",
            vec![Value::U32(0), Value::Array("{sv}".into(), vec![entry("session_handle", Value::Str(session.into()))])],
        );
        let (code, results) = response(&done, "/r/1").unwrap();
        assert_eq!(code, 0);
        assert_eq!(session_handle(results).as_deref(), Some(session));
        assert!(response(&done, "/r/2").is_none(), "another request's answer");
        let bound_now = signal(
            "/r/2",
            "Response",
            vec![
                Value::U32(0),
                Value::Array(
                    "{sv}".into(),
                    vec![entry(
                        "shortcuts",
                        Value::Array(
                            "(sa{sv})".into(),
                            vec![Value::Struct(vec![
                                Value::Str(SHORTCUT_ID.into()),
                                Value::Array("{sv}".into(), vec![]),
                            ])],
                        ),
                    )],
                ),
            ],
        );
        assert!(bound(response(&bound_now, "/r/2").unwrap().1));
        let cancelled = signal("/r/2", "Response", vec![Value::U32(1), Value::Array("{sv}".into(), vec![])]);
        assert_eq!(response(&cancelled, "/r/2").map(|(c, r)| (c, bound(r))), Some((1, false)));
        let pressed = Message {
            interface: Some(IFACE.into()),
            ..signal(
                PATH,
                "Activated",
                vec![
                    Value::Path(session.into()),
                    Value::Str(SHORTCUT_ID.into()),
                    Value::U64(9),
                    Value::Array("{sv}".into(), vec![]),
                ],
            )
        };
        assert!(activated(&pressed, session));
        assert!(!activated(&pressed, "/other/session"));
        let other_id = Message { body: vec![Value::Path(session.into()), Value::Str("x".into())], ..pressed.clone() };
        assert!(!activated(&other_id, session));
        assert!(unsupported("org.freedesktop.DBus.Error.ServiceUnknown"));
        assert!(unsupported("org.freedesktop.DBus.Error.UnknownMethod"));
        assert!(!unsupported("org.freedesktop.portal.Error.NotAllowed"));
    }

    #[test]
    fn calls_to_gezik_here_are_refused() {
        let call =
            Message { kind: MsgKind::Call, serial: 3, sender: Some(":1.7".into()), ..signal("/", "Explode", vec![]) };
        let reply = refuse(&call).unwrap();
        assert_eq!(reply.error_name.as_deref(), Some("org.freedesktop.DBus.Error.UnknownObject"));
        assert_eq!((reply.reply_serial, reply.destination.as_deref()), (Some(3), Some(":1.7")));
        assert!(refuse(&signal("/", "Activated", vec![])).is_none());
    }
}
