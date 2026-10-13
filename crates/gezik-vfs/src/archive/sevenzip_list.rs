//! Archives only 7-Zip reads (dmg, wim, vhd, msi, …; spec 16 §5.2): listed with
//! `7z l -slt -ba` and chosen entries extracted with `7z x -i@<list>`. The password goes to
//! its input, never on its command line; a wrong one is asked again.

use std::ffi::OsString;
use std::io;
use std::path::Path;

use gezik_batch::archive::{Entry, ExtractCx, local_time};
use gezik_core::batch::date::DateParts;
use gezik_platform::ChildProcess;

/// Every entry of `archive`, asking for a password while 7-Zip says it is wrong.
pub fn list(
    program: &Path,
    archive: &Path,
    cx: &dyn ExtractCx,
    password: &mut Option<String>,
) -> io::Result<Vec<Entry>> {
    let mut args: Vec<OsString> = ["l", "-slt", "-ba", "-sccUTF-8", "--"].into_iter().map(OsString::from).collect();
    args.push(archive.as_os_str().to_owned());
    let mut retry = false;
    loop {
        let input = format!("{}\n", password.as_deref().unwrap_or_default()).into_bytes();
        let mut child = ChildProcess::spawn_with_input(program, &args, None, Some(input))?;
        let mut entries = Parser::default();
        // Read as it comes: a large listing must not wait in the pipe.
        for line in child.stdout_lines() {
            if cx.stopped() {
                child.kill_tree();
                return Err(io::ErrorKind::Interrupted.into());
            }
            entries.line(&line);
        }
        let status = child.wait_or_stop(|| cx.stopped())?.ok_or(io::ErrorKind::Interrupted)?;
        let errors = child.stderr_text();
        match status.code() {
            Some(0 | 1) => return Ok(entries.done()),
            Some(2) if wrong_password(&errors) => {
                *password = Some(cx.password(retry).ok_or_else(no_password)?);
                retry = true;
            }
            _ => return Err(io::Error::other(message(&errors))),
        }
    }
}

/// Extracts `names` (and what is under them) from `archive` into `dest`.
pub fn extract_some(
    program: &Path,
    archive: &Path,
    names: &[String],
    dest: &Path,
    cx: &dyn ExtractCx,
    password: &mut Option<String>,
) -> io::Result<()> {
    let list = dest.join(".gezik-names.txt");
    std::fs::write(&list, names.join("\n"))?;
    let mut out = OsString::from("-o");
    out.push(dest.as_os_str());
    let mut include = OsString::from("-i@");
    include.push(list.as_os_str());
    let mut args: Vec<OsString> =
        ["x", "-y", "-spd", "-bso0", "-bsp0", "-scsUTF-8"].into_iter().map(OsString::from).collect();
    args.extend([out, include, "--".into(), archive.as_os_str().to_owned()]);
    let mut retry = false;
    let result = loop {
        let input = format!("{}\n", password.as_deref().unwrap_or_default()).into_bytes();
        let mut child = ChildProcess::spawn_with_input(program, &args, None, Some(input))?;
        let Some(status) = child.wait_or_stop(|| cx.stopped())? else { break Err(io::ErrorKind::Interrupted.into()) };
        let errors = child.stderr_text();
        match status.code() {
            Some(0) => break Ok(()),
            Some(2) if wrong_password(&errors) => match cx.password(retry) {
                Some(text) => {
                    *password = Some(text);
                    retry = true;
                }
                None => break Err(no_password()),
            },
            _ => break Err(io::Error::other(message(&errors))),
        }
    };
    let _ = std::fs::remove_file(&list);
    result
}

fn no_password() -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, gezik_i18n::t!("no password"))
}

fn wrong_password(errors: &str) -> bool {
    errors.contains("Wrong password") || errors.contains("open encrypted archive")
}

fn message(errors: &str) -> String {
    let lines: Vec<&str> = errors.lines().map(str::trim).filter(|line| !line.is_empty()).collect();
    if lines.is_empty() {
        gezik_i18n::t!("7-Zip could not read it").into_owned()
    } else {
        format!("7-Zip: {}", lines.join(" "))
    }
}

/// `-slt` output: a block of `Key = value` lines per entry, each starting with `Path`.
#[derive(Default)]
struct Parser {
    entries: Vec<Entry>,
    /// Packed sizes are given per solid block, not per entry, when the line is empty.
    current: Option<Entry>,
}

impl Parser {
    fn line(&mut self, line: &str) {
        let Some((key, value)) = line.split_once(" = ").or_else(|| line.strip_suffix(" =").map(|key| (key, ""))) else {
            return;
        };
        if key == "Path" {
            self.entries.extend(self.current.take());
            let name = if cfg!(windows) { value.replace('\\', "/") } else { value.to_owned() };
            self.current =
                Some(Entry { name, is_dir: false, size: None, packed: None, encrypted: false, modified: None });
            return;
        }
        let Some(entry) = self.current.as_mut() else { return };
        match key {
            "Folder" => entry.is_dir = value == "+",
            // `D` first in the attributes: a folder (formats without a Folder line).
            "Attributes" => entry.is_dir |= value.starts_with('D'),
            "Size" => entry.size = value.parse().ok(),
            "Packed Size" => entry.packed = value.parse().ok(),
            "Encrypted" => entry.encrypted = value == "+",
            "Modified" => entry.modified = date(value),
            _ => {}
        }
    }

    fn done(mut self) -> Vec<Entry> {
        self.entries.extend(self.current.take());
        self.entries
    }
}

/// `2024-05-17 13:45:30` (a fraction may follow), local time.
fn date(text: &str) -> Option<std::time::SystemTime> {
    let (day, time) = text.split_once(' ')?;
    let mut d = day.split('-').map(|p| p.parse::<u32>().ok());
    let mut t = time.split(['.', ':']).map(|p| p.parse::<u32>().ok());
    let parts = DateParts {
        year: i32::try_from(d.next()??).ok()?,
        month: u8::try_from(d.next()??).ok()?,
        day: u8::try_from(d.next()??).ok()?,
        hour: u8::try_from(t.next()??).ok()?,
        minute: u8::try_from(t.next()??).ok()?,
        second: u8::try_from(t.next()??).ok()?,
    };
    local_time(parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slt_blocks_become_entries() {
        let output = "Path = docs\nFolder = +\nSize = 0\nPacked Size = 0\nModified = 2024-05-17 13:45:30\n\
            Attributes = D\nPath = docs/a.txt\nFolder = -\nSize = 12\nPacked Size = 7\n\
            Modified = 2024-05-17 13:45:30.1234567\nEncrypted = +\nPath = noattr.bin\nSize = 3\n\
            Packed Size =\nAttributes = D_ drwxr-xr-x\nPath = plain\nAttributes = A";
        let mut parser = Parser::default();
        for line in output.lines() {
            parser.line(line);
        }
        let entries = parser.done();
        type Row<'a> = (&'a str, bool, Option<u64>, Option<u64>, bool);
        let summary: Vec<Row<'_>> =
            entries.iter().map(|e| (e.name.as_str(), e.is_dir, e.size, e.packed, e.encrypted)).collect();
        assert_eq!(
            summary,
            [
                ("docs", true, Some(0), Some(0), false),
                ("docs/a.txt", false, Some(12), Some(7), true),
                ("noattr.bin", true, Some(3), None, false),
                ("plain", false, None, None, false),
            ]
        );
        let parts = DateParts { year: 2024, month: 5, day: 17, hour: 13, minute: 45, second: 30 };
        assert_eq!(entries[1].modified, local_time(parts));
        assert_eq!(date("not a date"), None);
        assert!(wrong_password("ERROR: x.7z\nCan not open encrypted archive. Wrong password?"));
        assert_eq!(message(""), "7-Zip could not read it");
    }
}
