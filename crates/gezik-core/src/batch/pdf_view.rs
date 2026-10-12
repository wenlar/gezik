//! The protocol between the preview and its long-lived PDF view worker (`gezik --pdf-view`,
//! spec 13 §5.1). Gezik writes one [`Command`] per line to the worker's input; the worker
//! greets with [`HELLO`] and answers each command but `close` and `quit` with one [`Reply`]: a
//! header line, and after an `image` header the picture's raw RGBA bytes.
//!
//! What the worker says is untrusted (pdfium reads crafted files): a header is at most
//! [`MAX_HEADER`] bytes, a picture at most [`MAX_SIDE`]² and never larger than asked
//! ([`fits`]); anything else is a protocol failure and the worker is ended. Nothing here
//! indexes, and no reply allocates more than one picture.
#![deny(clippy::indexing_slicing)]

use std::fmt;
use std::io::{self, Read, Write};
use std::path::PathBuf;

use super::pdf_worker::{decode_path, encode_path, escape, unescape};

/// The worker's first line: the protocol and its version.
pub const HELLO: &str = "gezik-pdf-view 1";
/// The longest reply header, with its line break.
pub const MAX_HEADER: usize = 1024;
/// The longest command line the worker reads (two paths).
pub const MAX_COMMAND: usize = 64 * 1024;
/// The largest picture side: a picture is at most 4096 × 4096 × 4 bytes (64 MB).
pub const MAX_SIDE: u32 = 4096;
/// The largest scale (pixels per point): 800% on a 3× screen at 96 dpi is 32; a tiny page
/// fitted to the view's width can ask more. The picture stays within `w` × `h` anyway.
pub const MAX_SCALE: f32 = 1000.0;

/// What Gezik asks. `Debug` never shows a password.
#[derive(Clone, PartialEq)]
pub enum Command {
    /// Open `pdf` with the pdfium library `library` (bound once per worker).
    Open {
        library: PathBuf,
        pdf: PathBuf,
    },
    /// Open the last `open`'s file again with this password.
    Password(String),
    /// Page `page` (from 1) at `scale` pixels per point, only the `w` × `h` pixels at `x`, `y`
    /// of the scaled page; `scale` 0 is the whole page fitted into `w` × `h`.
    Render {
        page: u32,
        scale: f32,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
    },
    /// Let go of the document (the worker waits for the next one).
    Close,
    Quit,
}

impl fmt::Debug for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Command::Password(_) => f.write_str("Password(..)"),
            other => f.write_str(&other.to_line()),
        }
    }
}

impl Command {
    /// The command as a line, without its line break.
    pub fn to_line(&self) -> String {
        match self {
            Command::Open { library, pdf } => format!("open\t{}\t{}", encode_path(library), encode_path(pdf)),
            Command::Password(text) => format!("password\t{}", escape(text)),
            Command::Render { page, scale, x, y, w, h } => format!("render\t{page}\t{scale}\t{x}\t{y}\t{w}\t{h}"),
            Command::Close => "close".into(),
            Command::Quit => "quit".into(),
        }
    }

    /// The command of `line` (without its line break).
    pub fn parse(line: &str) -> Result<Command, String> {
        let mut fields = line.split('\t');
        let word = fields.next().unwrap_or_default();
        let rest: Vec<&str> = fields.collect();
        let command = match (word, rest.as_slice()) {
            ("open", [library, pdf]) => Command::Open {
                library: decode_path(library).ok_or("a bad library path")?,
                pdf: decode_path(pdf).ok_or("a bad PDF path")?,
            },
            ("password", [text]) => Command::Password(unescape(text).ok_or("a bad password field")?),
            ("render", [page, scale, x, y, w, h]) => {
                let number = |text: &str| text.parse::<u32>().map_err(|_| format!("{text:?} is not a number"));
                let scale: f32 = scale.parse().map_err(|_| format!("{scale:?} is not a scale"))?;
                let (page, x, y, w, h) = (number(page)?, number(x)?, number(y)?, number(w)?, number(h)?);
                if page == 0 || !(0.0..=MAX_SCALE).contains(&scale) || !side(w) || !side(h) {
                    return Err("render out of range".into());
                }
                Command::Render { page, scale, x, y, w, h }
            }
            ("close", []) => Command::Close,
            ("quit", []) => Command::Quit,
            _ => return Err("an unknown command".into()),
        };
        Ok(command)
    }
}

fn side(n: u32) -> bool {
    (1..=MAX_SIDE).contains(&n)
}

/// What the worker answers.
#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    /// The document opened; it has this many pages (at least one).
    Pages(u32),
    NeedsPassword,
    WrongPassword,
    /// Page `page`'s picture, `width` × `height` RGBA; the page is `page_w` × `page_h` points.
    Image {
        page: u32,
        width: u32,
        height: u32,
        page_w: f32,
        page_h: f32,
        rgba: Vec<u8>,
    },
    /// It could not (a damaged file, no library, a page out of range).
    Failed(String),
}

/// Writes `reply`: its header line, then an image's bytes.
pub fn write_reply(out: &mut dyn Write, reply: &Reply) -> io::Result<()> {
    match reply {
        Reply::Pages(n) => writeln!(out, "pages\t{n}")?,
        Reply::NeedsPassword => writeln!(out, "needs-password")?,
        Reply::WrongPassword => writeln!(out, "wrong-password")?,
        Reply::Image { page, width, height, page_w, page_h, rgba } => {
            writeln!(out, "image\t{page}\t{width}\t{height}\t{page_w}\t{page_h}")?;
            out.write_all(rgba)?;
        }
        Reply::Failed(message) => writeln!(out, "failed\t{}", short_field(message))?,
    }
    out.flush()
}

/// The longest failure message sent, in characters.
const MAX_MESSAGE: usize = 200;

/// `message` as an escaped field that keeps a header within [`MAX_HEADER`]: its first
/// [`MAX_MESSAGE`] characters, escaped one by one so a cut never splits a character or an
/// escape.
fn short_field(message: &str) -> String {
    let mut field = String::new();
    for c in message.chars().take(MAX_MESSAGE) {
        let piece = escape(c.encode_utf8(&mut [0; 4]));
        if field.len() + piece.len() > MAX_HEADER - 16 {
            break;
        }
        field.push_str(&piece);
    }
    field
}

fn bad(why: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, why.into())
}

/// One header line (without its break), at most [`MAX_HEADER`] bytes; the end of the output
/// before a line break is an error.
pub fn read_header(input: &mut dyn Read) -> io::Result<String> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        input.read_exact(&mut byte)?;
        let [b] = byte;
        if b == b'\n' {
            break;
        }
        if line.len() + 1 >= MAX_HEADER {
            return Err(bad("a reply header too long"));
        }
        line.push(b);
    }
    String::from_utf8(line).map_err(|_| bad("a reply header that is not UTF-8"))
}

/// The worker's greeting: [`HELLO`] or an error.
pub fn read_hello(input: &mut dyn Read) -> io::Result<()> {
    let line = read_header(input)?;
    if line == HELLO { Ok(()) } else { Err(bad("not a gezik-pdf-view 1 worker")) }
}

/// The next reply; every size checked before anything is allocated.
pub fn read_reply(input: &mut dyn Read) -> io::Result<Reply> {
    let line = read_header(input)?;
    let mut fields = line.split('\t');
    let word = fields.next().unwrap_or_default();
    let rest: Vec<&str> = fields.collect();
    let number = |text: &str| text.parse::<u32>().map_err(|_| bad("a bad number"));
    let points = |text: &str| match text.parse::<f32>() {
        Ok(v) if v.is_finite() && v > 0.0 && v <= 1e6 => Ok(v),
        _ => Err(bad("a bad page size")),
    };
    match (word, rest.as_slice()) {
        ("pages", [n]) => match number(n)? {
            0 => Err(bad("a document without pages")),
            n => Ok(Reply::Pages(n)),
        },
        ("needs-password", []) => Ok(Reply::NeedsPassword),
        ("wrong-password", []) => Ok(Reply::WrongPassword),
        ("failed", [text]) => Ok(Reply::Failed(unescape(text).ok_or_else(|| bad("a bad message"))?)),
        ("image", [page, w, h, pw, ph]) => {
            let (page, width, height) = (number(page)?, number(w)?, number(h)?);
            let (page_w, page_h) = (points(pw)?, points(ph)?);
            if page == 0 || !side(width) || !side(height) {
                return Err(bad("a picture out of range"));
            }
            let mut rgba = vec![0u8; width as usize * height as usize * 4];
            input.read_exact(&mut rgba)?;
            Ok(Reply::Image { page, width, height, page_w, page_h, rgba })
        }
        _ => Err(bad("an unknown reply")),
    }
}

/// Whether `reply` can answer `asked`: a picture of the page asked, no larger than asked.
pub fn fits(reply: &Reply, asked: &Command) -> bool {
    match (reply, asked) {
        (Reply::Image { page, width, height, .. }, Command::Render { page: p, w, h, .. }) => {
            page == p && width <= w && height <= h
        }
        (Reply::Image { .. }, _) => false,
        _ => true,
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn round(reply: &Reply) -> Reply {
        let mut bytes = Vec::new();
        write_reply(&mut bytes, reply).unwrap();
        read_reply(&mut &bytes[..]).unwrap()
    }

    #[test]
    fn commands_go_there_and_back() {
        let commands = [
            Command::Open { library: "C:\\p\\pdfium.dll".into(), pdf: "/a b\tc\nd/ğ.pdf".into() },
            Command::Password("se\tcr\net\\ x".into()),
            Command::Render { page: 3, scale: 2.5, x: 10, y: 20, w: 800, h: 600 },
            Command::Render { page: 1, scale: 0.0, x: 0, y: 0, w: 1, h: MAX_SIDE },
            Command::Close,
            Command::Quit,
        ];
        for command in commands {
            let line = command.to_line();
            assert!(!line.contains('\n') && !line.contains('\r'), "{line:?}");
            assert_eq!(Command::parse(&line).unwrap(), command);
        }
        assert_eq!(format!("{:?}", Command::Password("secret".into())), "Password(..)");
    }

    #[test]
    fn bad_commands_say_so() {
        for line in [
            "",
            "open\ts:a",
            "render\t0\t1\t0\t0\t10\t10",
            "render\t1\t-1\t0\t0\t10\t10",
            "render\t1\tNaN\t0\t0\t10\t10",
            "render\t1\t1\t0\t0\t0\t10",
            "render\t1\t1\t0\t0\t4097\t10",
            "render\t1\t2000\t0\t0\t10\t10",
            "password\tbad\\q",
            "quit\tnow",
        ] {
            assert!(Command::parse(line).is_err(), "{line:?}");
        }
    }

    #[test]
    fn replies_go_there_and_back() {
        let image = Reply::Image { page: 2, width: 3, height: 2, page_w: 612.0, page_h: 791.5, rgba: vec![7; 24] };
        for reply in [
            Reply::Pages(12),
            Reply::NeedsPassword,
            Reply::WrongPassword,
            Reply::Failed("pdfium\tcannot\nread it".into()),
            image,
        ] {
            assert_eq!(round(&reply), reply);
        }
        let long = round(&Reply::Failed("x".repeat(10_000)));
        assert!(matches!(long, Reply::Failed(text) if text.len() == MAX_MESSAGE), "a long message is cut");
    }

    #[test]
    fn oversized_and_short_replies_are_errors() {
        let err = |bytes: &[u8]| read_reply(&mut &bytes[..]).is_err();
        assert!(err(format!("failed\t{}\n", "x".repeat(MAX_HEADER)).as_bytes()), "a header over 1 KB");
        assert!(err(b"image\t1\t4097\t1\t10\t10\n"), "wider than 4096");
        assert!(err(b"image\t1\t1\t4097\t10\t10\n"), "taller than 4096");
        assert!(err(b"image\t1\t0\t1\t10\t10\n"));
        assert!(err(b"image\t1\t2\t2\t10\t10\n0123456789"), "a body shorter than 2 x 2 x 4");
        assert!(err(b"image\t1\t1\t1\tinf\t10\n0123"));
        assert!(err(b"pages\t0\n"));
        assert!(err(b"pages\t12"), "no line break");
        assert!(err(b"pages\t12\t3\n"));
        assert!(err(b"\xFF\xFE\n"));
        assert!(read_hello(&mut &b"gezik-pdf-view 2\n"[..]).is_err());
        assert!(read_hello(&mut &b"gezik-pdf-view 1\n"[..]).is_ok());
    }

    #[test]
    fn a_picture_larger_than_asked_does_not_fit() {
        let asked = Command::Render { page: 2, scale: 1.0, x: 0, y: 0, w: 100, h: 50 };
        let image = |page, width, height| Reply::Image { page, width, height, page_w: 1.0, page_h: 1.0, rgba: vec![] };
        assert!(fits(&image(2, 100, 50), &asked));
        assert!(fits(&image(2, 10, 5), &asked));
        assert!(!fits(&image(2, 101, 50), &asked));
        assert!(!fits(&image(2, 100, 51), &asked));
        assert!(!fits(&image(3, 10, 5), &asked), "another page");
        assert!(!fits(&image(2, 10, 5), &Command::Close));
        assert!(fits(&Reply::Pages(3), &asked));
    }

    #[test]
    fn any_failure_message_goes_through_cut_whole() {
        let mut seed = 0x1234_5678_9ABC_DEF1u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let alphabet = ['a', 'ğ', '😀', '\\', '\t', '\n', '\u{1}', '\u{7f}', '中', 'x'];
        for _ in 0..5_000 {
            let len = (next() % 400) as usize;
            let message: String = (0..len).map(|_| alphabet[(next() % alphabet.len() as u64) as usize]).collect();
            let mut bytes = Vec::new();
            write_reply(&mut bytes, &Reply::Failed(message.clone())).unwrap();
            assert!(bytes.len() <= MAX_HEADER, "{}", bytes.len());
            let Reply::Failed(back) = read_reply(&mut &bytes[..]).unwrap() else { panic!() };
            assert!(message.starts_with(&back), "a prefix of what was said");
            assert!(back.chars().count() <= MAX_MESSAGE);
        }
    }

    #[test]
    fn random_replies_never_panic() {
        let mut seed = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let words: [&[u8]; 9] =
            [b"image\t", b"pages\t", b"failed\t", b"needs-password", b"\t", b"\n", b"4096", b"1", b"\\x"];
        for _ in 0..20_000 {
            let mut bytes = Vec::new();
            for _ in 0..next() % 12 {
                match next() % 3 {
                    0 => bytes.extend_from_slice(words[(next() % words.len() as u64) as usize]),
                    1 => bytes.extend((0..next() % 8).map(|_| (next() % 256) as u8)),
                    _ => bytes.extend_from_slice((next() % 5000).to_string().as_bytes()),
                }
            }
            let mut input = &bytes[..];
            while let Ok(reply) = read_reply(&mut input) {
                if let Reply::Image { width, height, rgba, .. } = reply {
                    assert_eq!(rgba.len(), width as usize * height as usize * 4);
                }
            }
            let text = String::from_utf8_lossy(&bytes);
            let _ = Command::parse(&text);
        }
    }
}
