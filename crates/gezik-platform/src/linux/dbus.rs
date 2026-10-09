//! A small D-Bus client (spec 8.3): messages encoded and decoded by signature, the session
//! bus's address, EXTERNAL auth and Hello. As much as Gezik needs (FileManager1 now; the
//! tray and the portal in 9b9). No crate: zbus would bring ~1-1.5 MB and an async runtime
//! (decision 28). A broken message is an error; the caller closes the connection.

pub const MAX_MESSAGE: usize = 1 << 20;
/// Arrays, structs and variants together.
const MAX_DEPTH: usize = 32;
/// Items in one array: bounds what a 1 MB message can make Gezik allocate.
const MAX_ITEMS: usize = 65_536;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Byte(u8),
    Bool(bool),
    I16(i16),
    U16(u16),
    I32(i32),
    U32(u32),
    I64(i64),
    U64(u64),
    F64(f64),
    Str(String),
    Path(String),
    Sig(String),
    /// The element's signature and the items.
    Array(String, Vec<Value>),
    Struct(Vec<Value>),
    /// A dict entry: only inside an array.
    Entry(Box<Value>, Box<Value>),
    Variant(Box<Value>),
}

impl Value {
    pub fn signature(&self) -> String {
        match self {
            Value::Byte(_) => "y".into(),
            Value::Bool(_) => "b".into(),
            Value::I16(_) => "n".into(),
            Value::U16(_) => "q".into(),
            Value::I32(_) => "i".into(),
            Value::U32(_) => "u".into(),
            Value::I64(_) => "x".into(),
            Value::U64(_) => "t".into(),
            Value::F64(_) => "d".into(),
            Value::Str(_) => "s".into(),
            Value::Path(_) => "o".into(),
            Value::Sig(_) => "g".into(),
            Value::Array(elem, _) => format!("a{elem}"),
            Value::Struct(items) => format!("({})", items.iter().map(Value::signature).collect::<String>()),
            Value::Entry(k, v) => format!("{{{}{}}}", k.signature(), v.signature()),
            Value::Variant(_) => "v".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MsgKind {
    #[default]
    Call = 1,
    Return = 2,
    Error = 3,
    Signal = 4,
}

pub const NO_REPLY_EXPECTED: u8 = 1;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Message {
    pub kind: MsgKind,
    pub flags: u8,
    pub serial: u32,
    pub path: Option<String>,
    pub interface: Option<String>,
    pub member: Option<String>,
    pub error_name: Option<String>,
    pub reply_serial: Option<u32>,
    pub destination: Option<String>,
    pub sender: Option<String>,
    pub body: Vec<Value>,
}

impl Message {
    pub fn signature(&self) -> String {
        self.body.iter().map(Value::signature).collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bad(pub &'static str);

fn align_of(sig: u8) -> usize {
    match sig {
        b'n' | b'q' => 2,
        b'b' | b'i' | b'u' | b's' | b'o' | b'a' => 4,
        b'x' | b't' | b'd' | b'(' | b'{' => 8,
        _ => 1,
    }
}

/// The first complete type of `sig` and the rest.
pub fn split_type(sig: &str) -> Result<(&str, &str), Bad> {
    let end = type_end(sig.as_bytes(), 0, 0)?;
    Ok(sig.split_at(end))
}

fn type_end(b: &[u8], i: usize, depth: usize) -> Result<usize, Bad> {
    if depth > MAX_DEPTH {
        return Err(Bad("signature too deep"));
    }
    match b.get(i) {
        Some(b'y' | b'b' | b'n' | b'q' | b'i' | b'u' | b'x' | b't' | b'd' | b's' | b'o' | b'g' | b'v') => Ok(i + 1),
        Some(b'a') => type_end(b, i + 1, depth + 1),
        Some(b'(') => {
            let mut j = i + 1;
            if b.get(j) == Some(&b')') {
                return Err(Bad("empty struct"));
            }
            while b.get(j) != Some(&b')') {
                j = type_end(b, j, depth + 1)?;
            }
            Ok(j + 1)
        }
        Some(b'{') if i > 0 && b[i - 1] == b'a' => {
            let key = type_end(b, i + 1, depth + 1)?;
            if key != i + 2 || matches!(b[i + 1], b'v' | b'a' | b'(' | b'{') {
                return Err(Bad("a dict key is a basic type"));
            }
            let value = type_end(b, key, depth + 1)?;
            if b.get(value) != Some(&b'}') {
                return Err(Bad("a dict entry has two types"));
            }
            Ok(value + 1)
        }
        _ => Err(Bad("bad signature")),
    }
}

/// Every type of `sig` checked; `sig` at most 255 bytes.
fn types(mut sig: &str) -> Result<Vec<&str>, Bad> {
    if sig.len() > 255 {
        return Err(Bad("signature too long"));
    }
    let mut out = Vec::new();
    while !sig.is_empty() {
        let (one, rest) = split_type(sig)?;
        out.push(one);
        sig = rest;
    }
    Ok(out)
}

struct Writer {
    buf: Vec<u8>,
    le: bool,
}

impl Writer {
    fn pad(&mut self, align: usize) {
        while !self.buf.len().is_multiple_of(align) {
            self.buf.push(0);
        }
    }
    fn num<const N: usize>(&mut self, le: [u8; N], be: [u8; N]) {
        self.pad(N);
        self.buf.extend_from_slice(if self.le { &le } else { &be });
    }
    fn u32(&mut self, n: u32) {
        self.num(n.to_le_bytes(), n.to_be_bytes());
    }
    fn text(&mut self, text: &str) -> Result<(), Bad> {
        if text.contains('\0') {
            return Err(Bad("a NUL in a string"));
        }
        self.u32(text.len() as u32);
        self.buf.extend_from_slice(text.as_bytes());
        self.buf.push(0);
        Ok(())
    }
    fn sig(&mut self, sig: &str) -> Result<(), Bad> {
        types(sig)?;
        self.buf.push(sig.len() as u8);
        self.buf.extend_from_slice(sig.as_bytes());
        self.buf.push(0);
        Ok(())
    }
    fn value(&mut self, v: &Value, depth: usize) -> Result<(), Bad> {
        if depth > MAX_DEPTH {
            return Err(Bad("too deep"));
        }
        match v {
            Value::Byte(n) => self.buf.push(*n),
            Value::Bool(b) => self.u32(u32::from(*b)),
            Value::I16(n) => self.num(n.to_le_bytes(), n.to_be_bytes()),
            Value::U16(n) => self.num(n.to_le_bytes(), n.to_be_bytes()),
            Value::I32(n) => self.num(n.to_le_bytes(), n.to_be_bytes()),
            Value::U32(n) => self.u32(*n),
            Value::I64(n) => self.num(n.to_le_bytes(), n.to_be_bytes()),
            Value::U64(n) => self.num(n.to_le_bytes(), n.to_be_bytes()),
            Value::F64(n) => self.num(n.to_le_bytes(), n.to_be_bytes()),
            Value::Str(s) | Value::Path(s) => self.text(s)?,
            Value::Sig(s) => self.sig(s)?,
            Value::Array(elem, items) => {
                // Checked with its `a`: a dict entry is only valid as an array's element.
                if elem.is_empty() || types(&format!("a{elem}"))?.len() != 1 {
                    return Err(Bad("an array has one element type"));
                }
                self.u32(0);
                let at = self.buf.len() - 4;
                self.pad(align_of(elem.as_bytes()[0]));
                let start = self.buf.len();
                for item in items {
                    if item.signature() != *elem {
                        return Err(Bad("an item of another type"));
                    }
                    self.value(item, depth + 1)?;
                }
                let len = (self.buf.len() - start) as u32;
                let bytes = if self.le { len.to_le_bytes() } else { len.to_be_bytes() };
                self.buf[at..at + 4].copy_from_slice(&bytes);
            }
            Value::Struct(items) => {
                self.pad(8);
                for item in items {
                    self.value(item, depth + 1)?;
                }
            }
            Value::Entry(k, v) => {
                self.pad(8);
                self.value(k, depth + 1)?;
                self.value(v, depth + 1)?;
            }
            Value::Variant(inner) => {
                self.sig(&inner.signature())?;
                self.value(inner, depth + 1)?;
            }
        }
        Ok(())
    }
}

pub fn encode(m: &Message) -> Result<Vec<u8>, Bad> {
    encode_with(m, true)
}

fn encode_with(m: &Message, le: bool) -> Result<Vec<u8>, Bad> {
    let mut body = Writer { buf: Vec::new(), le };
    for v in &m.body {
        body.value(v, 0)?;
    }
    let field = |code: u8, v: Value| Value::Struct(vec![Value::Byte(code), Value::Variant(Box::new(v))]);
    let text = |s: &Option<String>| s.clone();
    let mut fields = Vec::new();
    // Gezik's order: path, destination, interface, member, error, reply, sender, signature.
    if let Some(p) = text(&m.path) {
        fields.push(field(1, Value::Path(p)));
    }
    if let Some(d) = text(&m.destination) {
        fields.push(field(6, Value::Str(d)));
    }
    if let Some(i) = text(&m.interface) {
        fields.push(field(2, Value::Str(i)));
    }
    if let Some(n) = text(&m.member) {
        fields.push(field(3, Value::Str(n)));
    }
    if let Some(e) = text(&m.error_name) {
        fields.push(field(4, Value::Str(e)));
    }
    if let Some(r) = m.reply_serial {
        fields.push(field(5, Value::U32(r)));
    }
    if let Some(s) = text(&m.sender) {
        fields.push(field(7, Value::Str(s)));
    }
    let sig = m.signature();
    if !sig.is_empty() {
        fields.push(field(8, Value::Sig(sig)));
    }
    let mut w = Writer { buf: vec![if le { b'l' } else { b'B' }, m.kind as u8, m.flags, 1], le };
    w.u32(body.buf.len() as u32);
    w.u32(m.serial);
    w.value(&Value::Array("(yv)".into(), fields), 0)?;
    w.pad(8);
    w.buf.extend_from_slice(&body.buf);
    if w.buf.len() > MAX_MESSAGE || m.serial == 0 {
        return Err(Bad("too long, or serial 0"));
    }
    Ok(w.buf)
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
    le: bool,
}

impl<'a> Reader<'a> {
    fn align(&mut self, a: usize) -> Result<(), Bad> {
        let to = self.pos.next_multiple_of(a);
        let pad = self.buf.get(self.pos..to).ok_or(Bad("truncated"))?;
        if pad.iter().any(|&b| b != 0) {
            return Err(Bad("padding is not zero"));
        }
        self.pos = to;
        Ok(())
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], Bad> {
        let out = self.buf.get(self.pos..self.pos.checked_add(n).ok_or(Bad("truncated"))?).ok_or(Bad("truncated"))?;
        self.pos += n;
        Ok(out)
    }
    fn num<const N: usize>(&mut self) -> Result<[u8; N], Bad> {
        self.align(N)?;
        let mut out: [u8; N] = self.take(N)?.try_into().map_err(|_| Bad("truncated"))?;
        if !self.le {
            out.reverse();
        }
        Ok(out)
    }
    fn u32(&mut self) -> Result<u32, Bad> {
        Ok(u32::from_le_bytes(self.num()?))
    }
    fn text(&mut self, len: usize) -> Result<String, Bad> {
        let bytes = self.take(len)?;
        if self.take(1)? != [0] {
            return Err(Bad("a string without its NUL"));
        }
        let text = std::str::from_utf8(bytes).map_err(|_| Bad("a string that is not UTF-8"))?;
        if text.contains('\0') {
            return Err(Bad("a NUL in a string"));
        }
        Ok(text.to_owned())
    }
    fn value(&mut self, sig: &str, depth: usize) -> Result<Value, Bad> {
        if depth > MAX_DEPTH {
            return Err(Bad("too deep"));
        }
        Ok(match sig.as_bytes()[0] {
            b'y' => Value::Byte(self.take(1)?[0]),
            b'b' => match self.u32()? {
                0 => Value::Bool(false),
                1 => Value::Bool(true),
                _ => return Err(Bad("a boolean is 0 or 1")),
            },
            b'n' => Value::I16(i16::from_le_bytes(self.num()?)),
            b'q' => Value::U16(u16::from_le_bytes(self.num()?)),
            b'i' => Value::I32(i32::from_le_bytes(self.num()?)),
            b'u' => Value::U32(self.u32()?),
            b'x' => Value::I64(i64::from_le_bytes(self.num()?)),
            b't' => Value::U64(u64::from_le_bytes(self.num()?)),
            b'd' => Value::F64(f64::from_le_bytes(self.num()?)),
            b's' => {
                let n = self.u32()? as usize;
                Value::Str(self.text(n)?)
            }
            b'o' => {
                let n = self.u32()? as usize;
                Value::Path(self.text(n)?)
            }
            b'g' => {
                let n = usize::from(self.take(1)?[0]);
                let s = self.text(n)?;
                types(&s)?;
                Value::Sig(s)
            }
            b'v' => {
                let n = usize::from(self.take(1)?[0]);
                let s = self.text(n)?;
                if types(&s)?.len() != 1 {
                    return Err(Bad("a variant holds one type"));
                }
                Value::Variant(Box::new(self.value(&s, depth + 1)?))
            }
            b'a' => {
                let len = self.u32()? as usize;
                if len > MAX_MESSAGE {
                    return Err(Bad("array too long"));
                }
                let elem = &sig[1..];
                self.align(align_of(elem.as_bytes()[0]))?;
                let end = self.pos.checked_add(len).ok_or(Bad("truncated"))?;
                let mut items = Vec::new();
                while self.pos < end {
                    if items.len() == MAX_ITEMS {
                        return Err(Bad("too many array items"));
                    }
                    items.push(self.value(elem, depth + 1)?);
                }
                if self.pos != end {
                    return Err(Bad("array length is wrong"));
                }
                Value::Array(elem.to_owned(), items)
            }
            b'(' => {
                self.align(8)?;
                let inner = types(&sig[1..sig.len() - 1])?;
                Value::Struct(inner.into_iter().map(|t| self.value(t, depth + 1)).collect::<Result<_, _>>()?)
            }
            b'{' => {
                self.align(8)?;
                let (k, v) = split_type(&sig[1..sig.len() - 1])?;
                Value::Entry(Box::new(self.value(k, depth + 1)?), Box::new(self.value(v, depth + 1)?))
            }
            _ => return Err(Bad("bad signature")),
        })
    }
}

/// The whole message's length from its first 16 bytes; at most [`MAX_MESSAGE`].
pub fn total_len(head: &[u8; 16]) -> Result<usize, Bad> {
    let le = match head[0] {
        b'l' => true,
        b'B' => false,
        _ => return Err(Bad("not a D-Bus message")),
    };
    if head[3] != 1 {
        return Err(Bad("protocol version"));
    }
    let n = |at: usize| {
        let b: [u8; 4] = [head[at], head[at + 1], head[at + 2], head[at + 3]];
        (if le { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) }) as usize
    };
    let total = 16usize.saturating_add(n(12).next_multiple_of(8)).saturating_add(n(4));
    if total > MAX_MESSAGE { Err(Bad("longer than 1 MB")) } else { Ok(total) }
}

pub fn decode(bytes: &[u8]) -> Result<Message, Bad> {
    let head: &[u8; 16] = bytes.get(..16).and_then(|h| h.try_into().ok()).ok_or(Bad("truncated"))?;
    if total_len(head)? != bytes.len() {
        return Err(Bad("length does not match"));
    }
    let le = bytes[0] == b'l';
    let kind = match bytes[1] {
        1 => MsgKind::Call,
        2 => MsgKind::Return,
        3 => MsgKind::Error,
        4 => MsgKind::Signal,
        _ => return Err(Bad("unknown message type")),
    };
    let mut r = Reader { buf: bytes, pos: 4, le };
    let body_len = r.u32()? as usize;
    let mut m = Message { kind, flags: bytes[2], serial: r.u32()?, ..Message::default() };
    let Value::Array(_, fields) = r.value("a(yv)", 0)? else { return Err(Bad("header")) };
    let mut sig = String::new();
    for field in fields {
        let Value::Struct(parts) = field else { return Err(Bad("header")) };
        let [Value::Byte(code), Value::Variant(v)] = parts.as_slice() else { return Err(Bad("header")) };
        match (code, v.as_ref()) {
            (1, Value::Path(p)) => m.path = Some(p.clone()),
            (2, Value::Str(s)) => m.interface = Some(s.clone()),
            (3, Value::Str(s)) => m.member = Some(s.clone()),
            (4, Value::Str(s)) => m.error_name = Some(s.clone()),
            (5, Value::U32(n)) => m.reply_serial = Some(*n),
            (6, Value::Str(s)) => m.destination = Some(s.clone()),
            (7, Value::Str(s)) => m.sender = Some(s.clone()),
            (8, Value::Sig(s)) => sig = s.clone(),
            (1..=8, _) => return Err(Bad("a header field of the wrong type")),
            _ => {} // unknown fields (9 = unix fds among them) are skipped
        }
    }
    r.align(8)?;
    if r.pos + body_len != bytes.len() {
        return Err(Bad("body length"));
    }
    let mut body = Reader { buf: &bytes[r.pos..], pos: 0, le };
    for t in types(&sig)? {
        m.body.push(body.value(t, 0)?);
    }
    if body.pos != body.buf.len() || m.serial == 0 {
        return Err(Bad("trailing bytes, or serial 0"));
    }
    let needs = match kind {
        MsgKind::Call => m.path.is_some() && m.member.is_some(),
        MsgKind::Return => m.reply_serial.is_some(),
        MsgKind::Error => m.reply_serial.is_some() && m.error_name.is_some(),
        MsgKind::Signal => m.path.is_some() && m.interface.is_some() && m.member.is_some(),
    };
    if needs { Ok(m) } else { Err(Bad("a required header field is missing")) }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Addr {
    Path(std::path::PathBuf),
    Abstract(Vec<u8>),
}

fn unescape(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut bytes = text.bytes();
    while let Some(b) = bytes.next() {
        if b == b'%' {
            let hex = [bytes.next()?, bytes.next()?];
            out.push(u8::from_str_radix(std::str::from_utf8(&hex).ok()?, 16).ok()?);
        } else {
            out.push(b);
        }
    }
    Some(out)
}

/// The first `unix:path=` or `unix:abstract=` address of `DBUS_SESSION_BUS_ADDRESS`.
pub fn parse_address(text: &str) -> Option<Addr> {
    for entry in text.split(';') {
        let Some(rest) = entry.strip_prefix("unix:") else { continue };
        for pair in rest.split(',') {
            if let Some(p) = pair.strip_prefix("path=") {
                return Some(Addr::Path(String::from_utf8(unescape(p)?).ok()?.into()));
            }
            if let Some(a) = pair.strip_prefix("abstract=") {
                return Some(Addr::Abstract(unescape(a)?));
            }
        }
    }
    None
}

#[cfg(all(unix, not(target_os = "macos")))]
pub use conn::Bus;

#[cfg(all(unix, not(target_os = "macos")))]
mod conn {
    use super::*;
    use std::io::{self, Read, Write};
    use std::os::unix::net::UnixStream;

    pub struct Bus {
        pub(crate) stream: UnixStream,
        serial: u32,
    }

    fn bad(why: Bad) -> io::Error {
        io::Error::new(io::ErrorKind::InvalidData, why.0)
    }

    impl Bus {
        /// The session bus: `DBUS_SESSION_BUS_ADDRESS`, else `$XDG_RUNTIME_DIR/bus`; EXTERNAL
        /// auth with this uid; Hello. Reads and writes time out after 2 s until the caller
        /// clears that.
        pub fn session() -> io::Result<Bus> {
            let addr = std::env::var("DBUS_SESSION_BUS_ADDRESS")
                .ok()
                .and_then(|a| parse_address(&a))
                .or_else(|| {
                    std::env::var_os("XDG_RUNTIME_DIR").map(|d| Addr::Path(std::path::Path::new(&d).join("bus")))
                })
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no session bus"))?;
            let mut stream = match addr {
                Addr::Path(p) => UnixStream::connect(p)?,
                #[cfg(any(target_os = "linux", target_os = "android"))]
                Addr::Abstract(name) => {
                    use std::os::linux::net::SocketAddrExt;
                    UnixStream::connect_addr(&std::os::unix::net::SocketAddr::from_abstract_name(name)?)?
                }
                #[cfg(not(any(target_os = "linux", target_os = "android")))]
                Addr::Abstract(_) => return Err(io::Error::new(io::ErrorKind::Unsupported, "abstract sockets")),
            };
            // A hung bus must not hold the caller; serve() clears this once it owns its name.
            stream.set_read_timeout(Some(std::time::Duration::from_secs(2)))?;
            stream.set_write_timeout(Some(std::time::Duration::from_secs(2)))?;
            // SAFETY: getuid cannot fail.
            let uid = unsafe { libc::getuid() }.to_string();
            let hex: String = uid.bytes().map(|b| format!("{b:02x}")).collect();
            stream.write_all(format!("\0AUTH EXTERNAL {hex}\r\n").as_bytes())?;
            let mut line = Vec::new();
            let mut byte = [0u8];
            while !line.ends_with(b"\r\n") {
                stream.read_exact(&mut byte)?;
                line.push(byte[0]);
                if line.len() > 512 {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "auth line too long"));
                }
            }
            if !line.starts_with(b"OK ") {
                return Err(io::Error::new(io::ErrorKind::PermissionDenied, "the bus refused EXTERNAL auth"));
            }
            stream.write_all(b"BEGIN\r\n")?;
            let mut bus = Bus { stream, serial: 0 };
            bus.call(Message {
                path: Some("/org/freedesktop/DBus".into()),
                destination: Some("org.freedesktop.DBus".into()),
                interface: Some("org.freedesktop.DBus".into()),
                member: Some("Hello".into()),
                ..Message::default()
            })?;
            Ok(bus)
        }

        pub fn send(&mut self, mut m: Message) -> io::Result<u32> {
            self.serial = self.serial.wrapping_add(1).max(1);
            m.serial = self.serial;
            self.stream.write_all(&encode(&m).map_err(bad)?)?;
            Ok(m.serial)
        }

        pub fn read(&mut self) -> io::Result<Message> {
            let mut head = [0u8; 16];
            self.stream.read_exact(&mut head)?;
            let total = total_len(&head).map_err(bad)?;
            let mut bytes = head.to_vec();
            bytes.resize(total, 0);
            self.stream.read_exact(&mut bytes[16..])?;
            decode(&bytes).map_err(bad)
        }

        /// A call and its reply; other messages meanwhile (NameAcquired) are dropped.
        pub fn call(&mut self, m: Message) -> io::Result<Message> {
            let serial = self.send(m)?;
            loop {
                let reply = self.read()?;
                if reply.reply_serial == Some(serial) {
                    return match reply.kind {
                        MsgKind::Error => Err(io::Error::other(reply.error_name.unwrap_or_default())),
                        _ => Ok(reply),
                    };
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hello() -> Message {
        Message {
            kind: MsgKind::Call,
            serial: 1,
            path: Some("/org/freedesktop/DBus".into()),
            destination: Some("org.freedesktop.DBus".into()),
            interface: Some("org.freedesktop.DBus".into()),
            member: Some("Hello".into()),
            ..Message::default()
        }
    }

    #[test]
    fn hello_is_the_bytes_the_bus_expects() {
        // Fields in Gezik's order (1, 6, 2, 3); worked out by hand from the specification.
        let want = "6c01000100000000010000006e00000001016f00150000002f6f72672f667265656465736b746f702f4442757300000006017300140000006f72672e667265656465736b746f702e444275730000000002017300140000006f72672e667265656465736b746f702e4442757300000000030173000500000048656c6c6f000000";
        let bytes = encode(&hello()).unwrap();
        let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, want);
        assert_eq!(total_len(bytes[..16].try_into().unwrap()), Ok(128));
        assert_eq!(decode(&bytes), Ok(hello()));
    }

    #[test]
    fn values_round_trip_with_alignment() {
        let body = vec![
            Value::Byte(7),
            Value::U64(u64::MAX),
            Value::Array("s".into(), vec![Value::Str("file:///home/u/ç ğ".into()), Value::Str(String::new())]),
            Value::Str("startup-id".into()),
            Value::Struct(vec![Value::Bool(true), Value::I16(-2), Value::F64(1.5), Value::Path("/a".into())]),
            Value::Array(
                "{sv}".into(),
                vec![Value::Entry(Box::new(Value::Str("k".into())), Box::new(Value::Variant(Box::new(Value::U32(3)))))],
            ),
            Value::Array("t".into(), vec![]),
            Value::Sig("a{sv}".into()),
        ];
        let message = Message {
            kind: MsgKind::Call,
            serial: 9,
            path: Some("/x".into()),
            member: Some("M".into()),
            body,
            ..Message::default()
        };
        let back = decode(&encode(&message).unwrap()).unwrap();
        assert_eq!(back.signature(), "ytass(bndo)a{sv}atg");
        assert_eq!(back, message);
    }

    #[test]
    fn signatures_are_checked() {
        assert_eq!(split_type("a{sv}s"), Ok(("a{sv}", "s")));
        assert_eq!(split_type("(ii)a(yv)"), Ok(("(ii)", "a(yv)")));
        let too_deep = format!("{}i", "a".repeat(33));
        for bad in ["a", "(", "()", "(i", "{sv}", "a{vs}", "a{s}", "a{sss}", "h", "z", too_deep.as_str()] {
            assert!(split_type(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn broken_messages_are_refused() {
        let good = encode(&hello()).unwrap();
        assert!(decode(&good[..good.len() - 1]).is_err(), "truncated");
        let mut big = good.clone();
        big[4..8].copy_from_slice(&(MAX_MESSAGE as u32).to_le_bytes());
        assert!(total_len(big[..16].try_into().unwrap()).is_err(), "longer than 1 MB");
        let mut endian = good.clone();
        endian[0] = b'X';
        assert!(decode(&endian).is_err());
        let mut no_member = hello();
        no_member.member = None;
        assert!(decode(&encode(&no_member).unwrap()).is_err(), "a call needs a member");
        let mut bad_bool = encode(&Message { body: vec![Value::Bool(true)], ..hello() }).unwrap();
        let at = bad_bool.len() - 4;
        bad_bool[at] = 2;
        assert!(decode(&bad_bool).is_err(), "a boolean is 0 or 1");
        let mut bad_utf8 = encode(&Message { body: vec![Value::Str("ab".into())], ..hello() }).unwrap();
        let at = bad_utf8.len() - 3;
        bad_utf8[at] = 0xFF;
        assert!(decode(&bad_utf8).is_err());
        assert!(encode(&Message { body: vec![Value::Str("a\0b".into())], ..hello() }).is_err());
    }

    #[test]
    fn garbage_never_panics() {
        // Mutations of a rich message and random bytes: decode may refuse, never panic.
        let rich = Message {
            body: vec![
                Value::Array(
                    "{sv}".into(),
                    vec![Value::Entry(
                        Box::new(Value::Str("k".into())),
                        Box::new(Value::Variant(Box::new(Value::Array(
                            "(yo)".into(),
                            vec![Value::Struct(vec![Value::Byte(1), Value::Path("/p".into())])],
                        )))),
                    )],
                ),
                Value::Sig("a(ia{sv})".into()),
                Value::U64(5),
            ],
            ..hello()
        };
        let good = encode(&rich).unwrap();
        assert_eq!(decode(&good), Ok(rich));
        let mut seed = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for _ in 0..20_000 {
            let mut bytes = good.clone();
            for _ in 0..1 + next() % 4 {
                let at = (next() as usize) % bytes.len();
                bytes[at] = next() as u8;
            }
            let _ = decode(&bytes);
            let _ = decode(&bytes[..(next() as usize) % bytes.len()]);
            let random: Vec<u8> = (0..next() % 64).map(|_| next() as u8).collect();
            let _ = decode(&random);
            let sig: String = (0..next() % 12).map(|_| char::from(b"ayv(){}sia"[(next() % 10) as usize])).collect();
            let _ = split_type(&sig);
        }
        let deep = format!("{}y{}", "(".repeat(40), ")".repeat(40));
        assert!(split_type(&deep).is_err());
        assert!(encode(&Message { body: vec![Value::Array("{sv}x".into(), vec![])], ..hello() }).is_err());
        assert!(encode(&Message { body: vec![Value::Array(String::new(), vec![])], ..hello() }).is_err());
    }

    #[test]
    fn arrays_have_at_most_65536_items() {
        let bytes = |n: usize| Message { body: vec![Value::Array("y".into(), vec![Value::Byte(0); n])], ..hello() };
        assert!(decode(&encode(&bytes(MAX_ITEMS)).unwrap()).is_ok());
        assert_eq!(decode(&encode(&bytes(MAX_ITEMS + 1)).unwrap()), Err(Bad("too many array items")));
    }

    #[test]
    fn big_endian_messages_are_read_too() {
        let message = Message { body: vec![Value::U32(0x0102_0304), Value::Str("ç".into())], ..hello() };
        let be = encode_with(&message, false).unwrap();
        assert_eq!(be[0], b'B');
        assert_eq!(decode(&be), Ok(message));
    }

    #[test]
    fn bus_addresses() {
        assert_eq!(parse_address("unix:path=/run/user/1000/bus"), Some(Addr::Path("/run/user/1000/bus".into())));
        assert_eq!(
            parse_address("tcp:host=x;unix:abstract=/tmp/dbus-x%2cy,guid=1"),
            Some(Addr::Abstract(b"/tmp/dbus-x,y".to_vec()))
        );
        assert_eq!(parse_address("tcp:host=x,port=1"), None);
        assert_eq!(parse_address("unix:path=%zz"), None);
    }
}
