//! StatusNotifierItem (spec 9 §9.1, decision 24): Gezik's tray icon on KDE, and on GNOME with
//! the AppIndicator extension. No menu (com.canonical.dbusmenu is large): Activate shows or
//! hides the window. A theme icon, no pixmap (deviation 7). Any app on the session bus may
//! call it: it only toggles the window and answers its properties.

#![cfg_attr(not(all(unix, not(target_os = "macos"))), allow(dead_code))]

use crate::linux::dbus::{Message, MsgKind, Value, answer};
use crate::tray::{TIP, TrayEvent};

pub const PATH: &str = "/StatusNotifierItem";
pub const IFACE: &str = "org.kde.StatusNotifierItem";
pub const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const WATCHER_PATH: &str = "/StatusNotifierWatcher";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
/// The watcher (the panel) starting again: Gezik registers again.
pub const WATCHER_MATCH: &str =
    "type='signal',sender='org.freedesktop.DBus',member='NameOwnerChanged',arg0='org.kde.StatusNotifierWatcher'";
const ICON: &str = "system-file-manager";
const INTROSPECTION: &str = r#"<node><interface name="org.kde.StatusNotifierItem"><property name="Category" type="s" access="read"/><property name="Id" type="s" access="read"/><property name="Title" type="s" access="read"/><property name="Status" type="s" access="read"/><property name="WindowId" type="i" access="read"/><property name="IconName" type="s" access="read"/><property name="IconThemePath" type="s" access="read"/><property name="ToolTip" type="(sa(iiay)ss)" access="read"/><property name="ItemIsMenu" type="b" access="read"/><method name="Activate"><arg type="i" direction="in"/><arg type="i" direction="in"/></method><method name="SecondaryActivate"><arg type="i" direction="in"/><arg type="i" direction="in"/></method><method name="ContextMenu"><arg type="i" direction="in"/><arg type="i" direction="in"/></method><method name="Scroll"><arg type="i" direction="in"/><arg type="s" direction="in"/></method></interface><interface name="org.freedesktop.DBus.Properties"><method name="Get"><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="v" direction="out"/></method><method name="GetAll"><arg type="s" direction="in"/><arg type="a{sv}" direction="out"/></method></interface><interface name="org.freedesktop.DBus.Introspectable"><method name="Introspect"><arg type="s" direction="out"/></method></interface><interface name="org.freedesktop.DBus.Peer"><method name="Ping"/></interface></node>"#;

/// The well-known name hosts look for (the spec's `org.kde.StatusNotifierItem-<pid>-<n>`).
pub fn item_name(pid: u32) -> String {
    format!("org.kde.StatusNotifierItem-{pid}-1")
}

pub fn properties() -> Vec<(&'static str, Value)> {
    vec![
        ("Category", Value::Str("ApplicationStatus".into())),
        ("Id", Value::Str("gezik".into())),
        ("Title", Value::Str(TIP.into())),
        ("Status", Value::Str("Active".into())),
        ("WindowId", Value::I32(0)),
        ("IconName", Value::Str(ICON.into())),
        ("IconThemePath", Value::Str(String::new())),
        (
            "ToolTip",
            Value::Struct(vec![
                Value::Str(ICON.into()),
                Value::Array("(iiay)".into(), Vec::new()),
                Value::Str(TIP.into()),
                Value::Str(String::new()),
            ]),
        ),
        ("ItemIsMenu", Value::Bool(false)),
    ]
}

fn error(m: &Message, name: &str, text: &str) -> (Option<TrayEvent>, Option<Message>) {
    (None, answer(m, Some((name, text)), Vec::new()))
}

/// What a message asks for, and the reply to send (none for signals and replies).
pub fn dispatch(m: &Message) -> (Option<TrayEvent>, Option<Message>) {
    if m.kind != MsgKind::Call {
        return (None, None);
    }
    if m.path.as_deref() != Some(PATH) {
        return error(m, "org.freedesktop.DBus.Error.UnknownObject", "no such object");
    }
    match (m.interface.as_deref(), m.member.as_deref()) {
        (Some(IFACE), Some("Activate" | "SecondaryActivate")) => (Some(TrayEvent::Toggle), answer(m, None, Vec::new())),
        (Some(IFACE), Some("ContextMenu" | "Scroll")) => (None, answer(m, None, Vec::new())),
        (Some(PROPERTIES), Some("GetAll")) => {
            let ours = matches!(m.body.first(), Some(Value::Str(i)) if i == IFACE);
            let entries = if ours {
                properties()
                    .into_iter()
                    .map(|(k, v)| Value::Entry(Box::new(Value::Str(k.into())), Box::new(Value::Variant(Box::new(v)))))
                    .collect()
            } else {
                Vec::new()
            };
            (None, answer(m, None, vec![Value::Array("{sv}".into(), entries)]))
        }
        (Some(PROPERTIES), Some("Get")) => match m.body.as_slice() {
            [Value::Str(i), Value::Str(name)] if i == IFACE => {
                match properties().into_iter().find(|(k, _)| k == name) {
                    Some((_, value)) => (None, answer(m, None, vec![Value::Variant(Box::new(value))])),
                    None => error(m, "org.freedesktop.DBus.Error.UnknownProperty", "no such property"),
                }
            }
            _ => error(m, "org.freedesktop.DBus.Error.InvalidArgs", "expected (s, s)"),
        },
        (Some("org.freedesktop.DBus.Introspectable"), Some("Introspect")) => {
            (None, answer(m, None, vec![Value::Str(INTROSPECTION.into())]))
        }
        (Some("org.freedesktop.DBus.Peer"), Some("Ping")) => (None, answer(m, None, Vec::new())),
        _ => error(m, "org.freedesktop.DBus.Error.UnknownMethod", "Gezik does not answer this"),
    }
}

pub fn register_call(name: &str) -> Message {
    Message {
        path: Some(WATCHER_PATH.into()),
        destination: Some(WATCHER.into()),
        interface: Some(WATCHER.into()),
        member: Some("RegisterStatusNotifierItem".into()),
        body: vec![Value::Str(name.into())],
        ..Message::default()
    }
}

/// NameOwnerChanged from the bus itself: the watcher has a new owner. Another app may send a
/// signal straight to Gezik (no match rule stops it), so the sender is checked.
pub fn watcher_came_back(m: &Message) -> bool {
    m.kind == MsgKind::Signal
        && m.sender.as_deref() == Some("org.freedesktop.DBus")
        && m.member.as_deref() == Some("NameOwnerChanged")
        && matches!(m.body.as_slice(), [Value::Str(name), _, Value::Str(new)] if name == WATCHER && !new.is_empty())
}

/// The bus's error when nobody owns the watcher's name: no tray on this desktop.
pub fn no_watcher(error: &str) -> bool {
    ["ServiceUnknown", "NameHasNoOwner", "UnknownMethod"].iter().any(|name| error.contains(name))
}

/// What the reply to registering says: Ok, no tray, or another error.
pub fn registered(reply: &Message) -> Result<(), crate::tray::TrayError> {
    use crate::tray::TrayError;
    match (reply.kind, reply.error_name.as_deref()) {
        (MsgKind::Error, Some(name)) if no_watcher(name) => Err(TrayError::NoTray),
        (MsgKind::Error, name) => Err(TrayError::Failed(name.unwrap_or_default().to_owned())),
        _ => Ok(()),
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
pub use serving::{Item, start};

#[cfg(all(unix, not(target_os = "macos")))]
mod serving {
    use std::os::unix::net::UnixStream;
    use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::linux::dbus::{Bus, NO_REPLY_EXPECTED, tell};
    use crate::tray::{OnEvent, OnReady, TrayError};

    /// The icon while held: dropped, the connection closes and the bus lets the name go.
    /// Drop never waits for the thread, so it may run inside `on_event` too.
    pub struct Item {
        stream: Arc<Mutex<Option<UnixStream>>>,
        stop: Arc<AtomicBool>,
    }

    /// Connects and takes the name on a thread of its own (a hung bus waits up to its 2 s
    /// timeouts there, never on the UI thread), registers, then answers calls (no CPU while
    /// idle). The registration's reply comes in the serving loop: the host may ask for the
    /// properties before the watcher answers.
    pub fn start(on_event: OnEvent, on_ready: OnReady) -> Item {
        let item = Item { stream: Arc::default(), stop: Arc::default() };
        let (slot, stop) = (item.stream.clone(), item.stop.clone());
        // shortcut: if no thread can be started, on_ready is never called (as tray/windows.rs).
        let _ = std::thread::Builder::new().name("gezik-sni".into()).spawn(move || match connect() {
            // Dropped meanwhile: nobody to tell.
            Err(why) => tell(&stop, on_ready, Err(why)),
            Ok(mut bus) => {
                if let (Ok(clone), Ok(mut held)) = (bus.stream.try_clone(), slot.lock()) {
                    *held = Some(clone);
                }
                // Dropped meanwhile: the connection closes here and the name goes.
                if stop.load(SeqCst) {
                    return;
                }
                serve(&mut bus, &on_event, on_ready, &stop);
            }
        });
        item
    }

    fn connect() -> Result<Bus, TrayError> {
        let failed = |err: std::io::Error| TrayError::Failed(err.to_string());
        let mut bus = Bus::session().map_err(failed)?;
        if !matches!(bus.request_name(&item_name(std::process::id())).map_err(failed)?, 1 | 4) {
            return Err(TrayError::Failed("another program holds the icon's name".into()));
        }
        bus.add_match(WATCHER_MATCH).map_err(failed)?;
        bus.stream.set_read_timeout(None).map_err(failed)?;
        Ok(bus)
    }

    fn serve(bus: &mut Bus, on_event: &OnEvent, on_ready: OnReady, stop: &AtomicBool) {
        let name = item_name(std::process::id());
        // The bus answers a silent watcher with NoReply after its own timeout.
        let Ok(serial) = bus.send(register_call(&name)) else {
            return tell(stop, on_ready, Err(TrayError::Failed("the session bus closed".into())));
        };
        let mut on_ready = Some(on_ready);
        while let Ok(message) = bus.read() {
            if message.reply_serial == Some(serial)
                && matches!(message.kind, MsgKind::Return | MsgKind::Error)
                && let Some(ready) = on_ready.take()
            {
                let result = registered(&message);
                let failed = result.is_err();
                tell(stop, ready, result);
                if failed {
                    return;
                }
                continue;
            }
            if watcher_came_back(&message) {
                let again = Message { flags: NO_REPLY_EXPECTED, ..register_call(&name) };
                if bus.send(again).is_err() {
                    break;
                }
                continue;
            }
            let (event, reply) = dispatch(&message);
            if let Some(reply) = reply
                && bus.send(reply).is_err()
            {
                break;
            }
            if let Some(event) = event {
                on_event(event);
            }
        }
        if let Some(ready) = on_ready {
            // The shutdown of a dropped Item ends the read too: then nobody is told.
            tell(stop, ready, Err(TrayError::Failed("the session bus closed".into())));
        }
    }

    impl Drop for Item {
        fn drop(&mut self) {
            // The thread keeps the stream before it looks at `stop` (as tray/windows.rs).
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

    fn call(interface: &str, member: &str, body: Vec<Value>) -> Message {
        Message {
            kind: MsgKind::Call,
            serial: 7,
            sender: Some(":1.9".into()),
            path: Some(PATH.into()),
            interface: Some(interface.into()),
            member: Some(member.into()),
            body,
            ..Message::default()
        }
    }

    #[test]
    fn activate_toggles_and_properties_answer() {
        let (event, reply) = dispatch(&call(IFACE, "Activate", vec![Value::I32(5), Value::I32(6)]));
        assert_eq!(event, Some(TrayEvent::Toggle));
        assert_eq!(reply.unwrap().reply_serial, Some(7));
        assert_eq!(
            dispatch(&call(IFACE, "SecondaryActivate", vec![Value::I32(0), Value::I32(0)])).0,
            Some(TrayEvent::Toggle)
        );
        let (none, reply) = dispatch(&call(IFACE, "ContextMenu", vec![Value::I32(0), Value::I32(0)]));
        assert!(none.is_none() && reply.unwrap().error_name.is_none(), "no menu: answered, nothing done");
        let all = dispatch(&call(PROPERTIES, "GetAll", vec![Value::Str(IFACE.into())])).1.unwrap();
        let Value::Array(sig, entries) = &all.body[0] else { panic!("{all:?}") };
        assert_eq!(sig, "{sv}");
        assert_eq!(entries.len(), properties().len());
        assert!(
            encode(&Message { serial: 1, ..all.clone() }).is_ok(),
            "the reply encodes (variants of a struct and an array)"
        );
        let icon = dispatch(&call(PROPERTIES, "Get", vec![Value::Str(IFACE.into()), Value::Str("IconName".into())]))
            .1
            .unwrap();
        assert_eq!(icon.body, [Value::Variant(Box::new(Value::Str("system-file-manager".into())))]);
        let other = dispatch(&call(PROPERTIES, "GetAll", vec![Value::Str("org.example".into())])).1.unwrap();
        assert_eq!(other.body, [Value::Array("{sv}".into(), vec![])], "another interface: no properties");
        let quiet = Message { flags: crate::linux::dbus::NO_REPLY_EXPECTED, ..call(IFACE, "Activate", vec![]) };
        assert_eq!(dispatch(&quiet), (Some(TrayEvent::Toggle), None), "no reply when none is expected");
    }

    #[test]
    fn other_calls_get_errors() {
        let error = |m: Message| dispatch(&m).1.and_then(|r| r.error_name);
        assert_eq!(error(call(IFACE, "Explode", vec![])).as_deref(), Some("org.freedesktop.DBus.Error.UnknownMethod"));
        let elsewhere = Message { path: Some("/".into()), ..call(IFACE, "Activate", vec![]) };
        assert_eq!(error(elsewhere).as_deref(), Some("org.freedesktop.DBus.Error.UnknownObject"));
        let unknown = call(PROPERTIES, "Get", vec![Value::Str(IFACE.into()), Value::Str("Menu".into())]);
        assert_eq!(error(unknown).as_deref(), Some("org.freedesktop.DBus.Error.UnknownProperty"), "no menu object");
        let bad = call(PROPERTIES, "Get", vec![Value::U32(1)]);
        assert_eq!(error(bad).as_deref(), Some("org.freedesktop.DBus.Error.InvalidArgs"));
        let signal = Message { kind: MsgKind::Signal, ..call(IFACE, "Activate", vec![]) };
        assert_eq!(dispatch(&signal), (None, None));
        let intro = dispatch(&call("org.freedesktop.DBus.Introspectable", "Introspect", vec![])).1.unwrap();
        assert!(matches!(&intro.body[0], Value::Str(xml) if xml.contains("org.kde.StatusNotifierItem")));
    }

    #[test]
    fn registering_and_the_watcher_coming_back() {
        assert_eq!(item_name(42), "org.kde.StatusNotifierItem-42-1");
        let register = register_call(&item_name(42));
        assert_eq!(register.destination.as_deref(), Some(WATCHER));
        assert_eq!(register.member.as_deref(), Some("RegisterStatusNotifierItem"));
        assert!(encode(&Message { serial: 1, ..register }).is_ok());
        let owner = |new: &str| Message {
            kind: MsgKind::Signal,
            sender: Some("org.freedesktop.DBus".into()),
            interface: Some("org.freedesktop.DBus".into()),
            member: Some("NameOwnerChanged".into()),
            body: vec![Value::Str(WATCHER.into()), Value::Str(String::new()), Value::Str(new.into())],
            ..Message::default()
        };
        assert!(watcher_came_back(&owner(":1.5")));
        assert!(!watcher_came_back(&owner("")), "it went away: nothing to register with");
        let forged = Message { sender: Some(":1.66".into()), ..owner(":1.5") };
        assert!(!watcher_came_back(&forged), "sent straight to Gezik by another app");
        assert!(no_watcher("org.freedesktop.DBus.Error.ServiceUnknown"));
        assert!(!no_watcher("org.freedesktop.DBus.Error.AccessDenied"));
        use crate::tray::TrayError;
        let reply = |kind, name: Option<&str>| Message { kind, error_name: name.map(Into::into), ..Message::default() };
        assert_eq!(registered(&reply(MsgKind::Return, None)), Ok(()));
        let none = reply(MsgKind::Error, Some("org.freedesktop.DBus.Error.ServiceUnknown"));
        assert_eq!(registered(&none), Err(TrayError::NoTray));
        let denied = reply(MsgKind::Error, Some("org.freedesktop.DBus.Error.AccessDenied"));
        assert_eq!(registered(&denied), Err(TrayError::Failed("org.freedesktop.DBus.Error.AccessDenied".into())));
    }
}
