//! The line protocol between Gezik and its PDF worker process (`gezik --pdf-worker`).
//!
//! The parent writes one [`Request`] to the worker's stdin; the worker answers with one
//! [`Reply`] per line on stdout. Everything is plain text with tab-separated fields, so no
//! name or password can add a line, and paths survive exactly (even non-Unicode ones).

use super::pdf::{PageImage, Split};
use std::fmt;
use std::path::{Path, PathBuf};

pub const MAGIC: &str = "gezik-pdf";
pub const VERSION: u32 = 1;

/// What the worker is asked to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerJob {
    Merge,
    Split(Split),
    Extract(String),
    Render { dpi: u32, image: PageImage },
    Count,
}

/// One job for the worker. `Debug` never shows the passwords.
#[derive(Clone, PartialEq, Eq)]
pub struct Request {
    /// The pdfium library file.
    pub library: PathBuf,
    /// Where outputs go (an empty folder); unused by `Count`.
    pub dir: PathBuf,
    pub job: WorkerJob,
    pub inputs: Vec<PathBuf>,
    /// Passwords by input index.
    pub passwords: Vec<(usize, String)>,
}

impl fmt::Debug for Request {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Request")
            .field("library", &self.library)
            .field("dir", &self.dir)
            .field("job", &self.job)
            .field("inputs", &self.inputs)
            .field("passwords", &format_args!("{} given", self.passwords.len()))
            .finish()
    }
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c if c.is_control() => out.push_str(&format!("\\x{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn unescape(text: &str) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next()? {
            '\\' => out.push('\\'),
            't' => out.push('\t'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            'x' => {
                let hi = chars.next()?.to_digit(16)?;
                let lo = chars.next()?.to_digit(16)?;
                out.push(char::from_u32(hi * 16 + lo)?);
            }
            _ => return None,
        }
    }
    Some(out)
}

/// A path as one tab-free, newline-free field: `s:` + escaped text when it is valid Unicode,
/// otherwise `w:` (UTF-16 units, 4 hex digits each; Windows) or `b:` (bytes, 2 hex digits
/// each; Unix).
pub fn encode_path(path: &Path) -> String {
    if let Some(text) = path.to_str() {
        return format!("s:{}", escape(text));
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let mut out = String::from("w:");
        for unit in path.as_os_str().encode_wide() {
            out.push_str(&format!("{unit:04x}"));
        }
        out
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let mut out = String::from("b:");
        for byte in path.as_os_str().as_bytes() {
            out.push_str(&format!("{byte:02x}"));
        }
        out
    }
    #[cfg(not(any(windows, unix)))]
    {
        format!("s:{}", escape(&path.to_string_lossy()))
    }
}

/// The inverse of [`encode_path`]; `None` for a malformed field (or one from another OS).
pub fn decode_path(text: &str) -> Option<PathBuf> {
    if let Some(rest) = text.strip_prefix("s:") {
        return Some(PathBuf::from(unescape(rest)?));
    }
    #[cfg(windows)]
    if let Some(rest) = text.strip_prefix("w:") {
        use std::os::windows::ffi::OsStringExt;
        if !rest.is_ascii() || rest.len() % 4 != 0 {
            return None;
        }
        let units = (0..rest.len() / 4)
            .map(|i| u16::from_str_radix(&rest[i * 4..i * 4 + 4], 16).ok())
            .collect::<Option<Vec<u16>>>()?;
        return Some(PathBuf::from(std::ffi::OsString::from_wide(&units)));
    }
    #[cfg(unix)]
    if let Some(rest) = text.strip_prefix("b:") {
        use std::os::unix::ffi::OsStringExt;
        if !rest.is_ascii() || rest.len() % 2 != 0 {
            return None;
        }
        let bytes = (0..rest.len() / 2)
            .map(|i| u8::from_str_radix(&rest[i * 2..i * 2 + 2], 16).ok())
            .collect::<Option<Vec<u8>>>()?;
        return Some(PathBuf::from(std::ffi::OsString::from_vec(bytes)));
    }
    None
}

impl Request {
    pub fn to_text(&self) -> String {
        let mut out = format!("{MAGIC}\t{VERSION}\n");
        out.push_str(&format!("library\t{}\n", encode_path(&self.library)));
        out.push_str(&format!("dir\t{}\n", encode_path(&self.dir)));
        match &self.job {
            WorkerJob::Merge => out.push_str("job\tmerge\n"),
            WorkerJob::Split(Split::EachPage) => out.push_str("job\tsplit\teach\n"),
            WorkerJob::Split(Split::Every(n)) => out.push_str(&format!("job\tsplit\tevery\t{n}\n")),
            WorkerJob::Split(Split::Ranges(text)) => {
                out.push_str(&format!("job\tsplit\tranges\t{}\n", escape(text)));
            }
            WorkerJob::Extract(text) => out.push_str(&format!("job\textract\t{}\n", escape(text))),
            WorkerJob::Render { dpi, image } => {
                let image = match image {
                    PageImage::Png => "png",
                    PageImage::Jpeg => "jpeg",
                };
                out.push_str(&format!("job\trender\t{dpi}\t{image}\n"));
            }
            WorkerJob::Count => out.push_str("job\tcount\n"),
        }
        for input in &self.inputs {
            out.push_str(&format!("in\t{}\n", encode_path(input)));
        }
        for (index, password) in &self.passwords {
            out.push_str(&format!("password\t{index}\t{}\n", escape(password)));
        }
        out
    }

    pub fn parse(text: &str) -> Result<Request, String> {
        let mut lines = text.lines();
        if lines.next() != Some(&format!("{MAGIC}\t{VERSION}")) {
            return Err(format!("not a {MAGIC} {VERSION} request"));
        }
        let mut library = None;
        let mut dir = None;
        let mut job = None;
        let mut inputs = Vec::new();
        let mut passwords = Vec::new();
        for line in lines {
            let (key, rest) = line.split_once('\t').unwrap_or((line, ""));
            match key {
                "library" => library = Some(decode_path(rest).ok_or("bad library path")?),
                "dir" => dir = Some(decode_path(rest).ok_or("bad dir path")?),
                "job" => job = Some(parse_job(rest)?),
                "in" => inputs.push(decode_path(rest).ok_or("bad input path")?),
                "password" => {
                    let (index, password) = rest.split_once('\t').ok_or("bad password line")?;
                    let index = index.parse::<usize>().map_err(|_| "bad password line")?;
                    passwords.push((index, unescape(password).ok_or("bad password line")?));
                }
                _ => return Err(format!("unknown line \"{key}\"")),
            }
        }
        let library = library.ok_or("missing library")?;
        let job = job.ok_or("missing job")?;
        if inputs.is_empty() {
            return Err("missing in".to_string());
        }
        match &job {
            WorkerJob::Merge if inputs.len() < 2 => return Err("merge needs two inputs".to_string()),
            WorkerJob::Merge => {}
            other if inputs.len() > 1 => return Err(format!("one input only for {}", job_name(other))),
            _ => {}
        }
        if passwords.iter().any(|(index, _)| *index >= inputs.len()) {
            return Err("password for a missing input".to_string());
        }
        // `dir` is unused by `count`, so a count request may leave it out.
        let dir = match dir {
            Some(dir) => dir,
            None if job == WorkerJob::Count => PathBuf::new(),
            None => return Err("missing dir".to_string()),
        };
        Ok(Request { library, dir, job, inputs, passwords })
    }

    /// The password given for input `input`, if any.
    pub fn password(&self, input: usize) -> Option<&str> {
        self.passwords.iter().find(|(index, _)| *index == input).map(|(_, p)| p.as_str())
    }
}

fn job_name(job: &WorkerJob) -> &'static str {
    match job {
        WorkerJob::Merge => "merge",
        WorkerJob::Split(_) => "split",
        WorkerJob::Extract(_) => "extract",
        WorkerJob::Render { .. } => "render",
        WorkerJob::Count => "count",
    }
}

fn parse_job(text: &str) -> Result<WorkerJob, String> {
    let bad = || "bad job line".to_string();
    let parts: Vec<&str> = text.splitn(4, '\t').collect();
    match parts.as_slice() {
        ["merge"] => Ok(WorkerJob::Merge),
        ["count"] => Ok(WorkerJob::Count),
        ["split", "each"] => Ok(WorkerJob::Split(Split::EachPage)),
        ["split", "every", n] => Ok(WorkerJob::Split(Split::Every(n.parse().map_err(|_| bad())?))),
        ["split", "ranges", ranges] => Ok(WorkerJob::Split(Split::Ranges(unescape(ranges).ok_or_else(bad)?))),
        ["extract", ranges] => Ok(WorkerJob::Extract(unescape(ranges).ok_or_else(bad)?)),
        ["render", dpi, image] => {
            let image = match *image {
                "png" => PageImage::Png,
                "jpeg" => PageImage::Jpeg,
                _ => return Err(bad()),
            };
            Ok(WorkerJob::Render { dpi: dpi.parse().map_err(|_| bad())?, image })
        }
        _ => Err(bad()),
    }
}

/// Why the worker could not finish.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    Damaged,
    Library,
    Ranges,
    Io,
    Other,
}

impl Failure {
    fn word(self) -> &'static str {
        match self {
            Failure::Damaged => "damaged",
            Failure::Library => "library",
            Failure::Ranges => "ranges",
            Failure::Io => "io",
            Failure::Other => "other",
        }
    }

    fn from_word(word: &str) -> Option<Failure> {
        Some(match word {
            "damaged" => Failure::Damaged,
            "library" => Failure::Library,
            "ranges" => Failure::Ranges,
            "io" => Failure::Io,
            "other" => Failure::Other,
            _ => return None,
        })
    }
}

/// One line from the worker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    /// `Count`'s answer.
    Pages {
        input: usize,
        pages: u32,
    },
    /// Work found: this many page steps more.
    Steps(u64),
    /// One page step done.
    Step,
    /// This many page steps done at once (a document's pages copied in one go): one line, so
    /// a burst of them cannot crowd out other replies.
    Stepped(u64),
    /// 1-based page rendered at a lower dpi.
    Lowered {
        page: u32,
        dpi: u32,
    },
    NeedsPassword(usize),
    WrongPassword(usize),
    Failed {
        input: Option<usize>,
        why: Failure,
        message: String,
    },
    Done,
}

impl Reply {
    /// The reply as one line, without a newline.
    pub fn to_line(&self) -> String {
        match self {
            Reply::Pages { input, pages } => format!("pages\t{input}\t{pages}"),
            Reply::Steps(n) => format!("steps\t{n}"),
            Reply::Step => "step".to_string(),
            Reply::Stepped(n) => format!("stepped	{n}"),
            Reply::Lowered { page, dpi } => format!("lowered\t{page}\t{dpi}"),
            Reply::NeedsPassword(i) => format!("needs-password\t{i}"),
            Reply::WrongPassword(i) => format!("wrong-password\t{i}"),
            Reply::Failed { input, why, message } => {
                let input = input.map_or("-".to_string(), |i| i.to_string());
                format!("failed\t{input}\t{}\t{}", why.word(), escape(message))
            }
            Reply::Done => "done".to_string(),
        }
    }

    /// An unknown or malformed line is `None` (the reader ignores it).
    pub fn parse(line: &str) -> Option<Reply> {
        let parts: Vec<&str> = line.splitn(4, '\t').collect();
        match parts.as_slice() {
            ["pages", i, n] => Some(Reply::Pages { input: i.parse().ok()?, pages: n.parse().ok()? }),
            ["steps", n] => Some(Reply::Steps(n.parse().ok()?)),
            ["step"] => Some(Reply::Step),
            ["stepped", n] => Some(Reply::Stepped(n.parse().ok()?)),
            ["lowered", page, dpi] => Some(Reply::Lowered { page: page.parse().ok()?, dpi: dpi.parse().ok()? }),
            ["needs-password", i] => Some(Reply::NeedsPassword(i.parse().ok()?)),
            ["wrong-password", i] => Some(Reply::WrongPassword(i.parse().ok()?)),
            ["failed", input, why, message] => {
                let input = if *input == "-" { None } else { Some(input.parse().ok()?) };
                Some(Reply::Failed { input, why: Failure::from_word(why)?, message: unescape(message)? })
            }
            ["done"] => Some(Reply::Done),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::batch::pdf::{PageImage, Split};

    fn request(job: WorkerJob, inputs: &[&str]) -> Request {
        Request {
            library: PathBuf::from(r"C:\Users\ş\AppData\Roaming\gezik\tools\pdfium-8086\pdfium.dll"),
            dir: PathBuf::from("/tmp/.gezik-x-1-0/x/.0"),
            job,
            inputs: inputs.iter().map(PathBuf::from).collect(),
            passwords: Vec::new(),
        }
    }

    #[test]
    fn every_job_goes_there_and_back() {
        let jobs = [
            (WorkerJob::Merge, vec!["a.pdf", "b.pdf"]),
            (WorkerJob::Split(Split::EachPage), vec!["a.pdf"]),
            (WorkerJob::Split(Split::Every(10)), vec!["a.pdf"]),
            (WorkerJob::Split(Split::Ranges("1-3, 5,\t8-".into())), vec!["a.pdf"]),
            (WorkerJob::Extract("2-".into()), vec!["a.pdf"]),
            (WorkerJob::Render { dpi: 150, image: PageImage::Jpeg }, vec!["a.pdf"]),
            (WorkerJob::Render { dpi: 300, image: PageImage::Png }, vec!["a.pdf"]),
            (WorkerJob::Count, vec!["a.pdf"]),
        ];
        for (job, inputs) in jobs {
            let req = request(job, &inputs);
            assert_eq!(Request::parse(&req.to_text()).unwrap(), req);
        }
    }

    #[test]
    fn odd_names_and_passwords_pass_unchanged() {
        let mut req = request(WorkerJob::Merge, &["rapor ş \"a\"; $ & b.pdf", "line\nbreak\ttab\\back.pdf"]);
        req.passwords = vec![(1, "p\tw\nö\\".to_owned())];
        let back = Request::parse(&req.to_text()).unwrap();
        assert_eq!(back, req);
        assert_eq!(back.password(1), Some("p\tw\nö\\"));
        assert_eq!(back.password(0), None);
        // Every field stays on its own line: a name or password cannot add a line.
        assert_eq!(req.to_text().lines().count(), 7);
    }

    #[test]
    fn debug_hides_passwords() {
        let mut req = request(WorkerJob::Count, &["a.pdf"]);
        req.passwords = vec![(0, "sekret".to_owned())];
        let shown = format!("{req:?}");
        assert!(!shown.contains("sekret") && shown.contains("1 given"), "{shown}");
    }

    #[cfg(windows)]
    #[test]
    fn a_windows_name_that_is_not_unicode_passes() {
        use std::os::windows::ffi::OsStringExt;
        let lone = PathBuf::from(std::ffi::OsString::from_wide(&[0x61, 0xD800, 0x2E, 0x70, 0x64, 0x66]));
        let text = encode_path(&lone);
        assert!(text.starts_with("w:0061d800"), "{text}");
        assert_eq!(decode_path(&text), Some(lone));
    }

    #[cfg(unix)]
    #[test]
    fn a_unix_name_that_is_not_utf8_passes() {
        use std::os::unix::ffi::OsStringExt;
        let raw = PathBuf::from(std::ffi::OsString::from_vec(vec![b'a', 0xFF, b'.', b'p', b'd', b'f']));
        let text = encode_path(&raw);
        assert_eq!(text, "b:61ff2e706466");
        assert_eq!(decode_path(&text), Some(raw));
    }

    #[test]
    fn bad_requests_say_why() {
        assert_eq!(Request::parse("hello").unwrap_err(), "not a gezik-pdf 1 request");
        assert_eq!(Request::parse("gezik-pdf\t2\n").unwrap_err(), "not a gezik-pdf 1 request");
        let ok = request(WorkerJob::Count, &["a.pdf"]).to_text();
        assert_eq!(Request::parse(&ok.replace("library\t", "librar\t")).unwrap_err(), "unknown line \"librar\"");
        let two = request(WorkerJob::Count, &["a.pdf", "b.pdf"]).to_text();
        assert_eq!(Request::parse(&two).unwrap_err(), "one input only for count");
        let one = request(WorkerJob::Merge, &["a.pdf"]).to_text();
        assert_eq!(Request::parse(&one).unwrap_err(), "merge needs two inputs");
        assert_eq!(Request::parse(&format!("{ok}password\t3\tx\n")).unwrap_err(), "password for a missing input");
    }

    #[test]
    fn replies_go_there_and_back() {
        let replies = [
            Reply::Pages { input: 0, pages: 14 },
            Reply::Steps(20_000),
            Reply::Step,
            Reply::Stepped(7),
            Reply::Lowered { page: 3, dpi: 41 },
            Reply::NeedsPassword(1),
            Reply::WrongPassword(0),
            Reply::Failed { input: Some(1), why: Failure::Damaged, message: "bad xref\ttable\nhere".into() },
            Reply::Failed { input: None, why: Failure::Library, message: "LoadLibraryError".into() },
            Reply::Done,
        ];
        for reply in replies {
            let line = reply.to_line();
            assert!(!line.contains('\n'), "{line}");
            assert_eq!(Reply::parse(&line), Some(reply));
        }
        assert_eq!(Reply::parse("progress=end"), None);
        assert_eq!(Reply::parse("steps\tmany"), None);
    }
}
