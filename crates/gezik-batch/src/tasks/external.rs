//! Extracting with 7-Zip, for the formats Gezik does not read itself: `7z x` as a child
//! process without a window, its percentage as progress, a wrong password asked again,
//! and on cancel the process ended with everything it started.

use std::cell::Cell;
use std::ffi::OsString;
use std::io;
use std::path::Path;

use gezik_ops::{Answer, Question, RunCx};
use gezik_platform::ChildProcess;

use super::cancelled;

/// Unpacks `archive` into `dest` (an empty folder) with the 7-Zip at `seven_zip`.
pub(super) fn extract(seven_zip: &Path, archive: &Path, dest: &Path, cx: &RunCx<'_>) -> io::Result<()> {
    let size = std::fs::metadata(archive).map_or(0, |meta| meta.len());
    // 7-Zip says how far it is in percent: the archive's size stands for the work.
    cx.found(1, size);
    // An empty password first: 7-Zip then never waits for one on its input, and an
    // encrypted archive says "wrong password".
    let mut password = String::new();
    let mut asked = false;
    // Bytes counted so far (a try after a wrong password counts only what goes further).
    let counted = Cell::new(0u64);
    loop {
        let mut child = ChildProcess::spawn(seven_zip, arguments(archive, dest, &password), None)?;
        let lines = child.stdout_lines();
        let status = child.wait_or_stop(|| {
            for line in lines.ready() {
                if let Some(percent) = percent(&line) {
                    let now = size / 100 * percent + size % 100 * percent / 100;
                    if now > counted.get() {
                        cx.add_bytes(now - counted.get());
                        counted.set(now);
                    }
                }
            }
            cx.stopped()
        })?;
        let Some(status) = status else { return Err(cancelled()) };
        let errors = child.stderr_text();
        match status.code() {
            Some(0) => {
                cx.one_done(size.saturating_sub(counted.get()));
                return Ok(());
            }
            // A warning: some files could not be read; the rest is there.
            Some(1) => {
                cx.fail(archive, &io::Error::other(message(&errors)));
                cx.one_done(size.saturating_sub(counted.get()));
                return Ok(());
            }
            Some(2) if wrong_password(&errors) => {
                match cx.ask(Question::Password { archive: archive.to_path_buf(), retry: asked }) {
                    Answer::Text(text) => password = text,
                    _ if cx.stopped() => return Err(cancelled()),
                    _ => return Err(io::Error::new(io::ErrorKind::PermissionDenied, "no password")),
                }
                asked = true;
                // What the wrong password left (empty or broken files) goes before the next try.
                std::fs::remove_dir_all(dest)?;
                std::fs::create_dir(dest)?;
            }
            _ => return Err(io::Error::other(message(&errors))),
        }
    }
}

/// `x -y -bsp1 -bso0 -o<dest> -p<password> -- <archive>`: no shell, each one argument.
fn arguments(archive: &Path, dest: &Path, password: &str) -> Vec<OsString> {
    let mut out = OsString::from("-o");
    out.push(dest.as_os_str());
    let mut args: Vec<OsString> = ["x", "-y", "-bsp1", "-bso0"].into_iter().map(OsString::from).collect();
    args.push(out);
    args.push(OsString::from(format!("-p{password}")));
    args.push("--".into());
    args.push(archive.as_os_str().to_owned());
    args
}

/// The percentage at the start of a progress piece (`" 45% 3 - a.txt"`).
fn percent(line: &str) -> Option<u64> {
    let (number, _) = line.trim_start().split_once('%')?;
    number.parse::<u64>().ok().filter(|&p| p <= 100)
}

fn wrong_password(errors: &str) -> bool {
    errors.contains("Wrong password")
        || errors.contains("Can not open encrypted archive")
        || errors.contains("Cannot open encrypted archive")
}

/// What 7-Zip said went wrong, in a line.
fn message(errors: &str) -> String {
    let lines: Vec<&str> = errors.lines().map(str::trim).filter(|line| !line.is_empty()).collect();
    if lines.is_empty() { "7-Zip could not extract it".to_owned() } else { format!("7-Zip: {}", lines.join(" ")) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_and_errors_are_read() {
        assert_eq!(percent("  0%"), Some(0));
        assert_eq!(percent(" 45% 3 - a.txt"), Some(45));
        assert_eq!(percent("100%"), Some(100));
        assert_eq!(percent("  0M Scan"), None);
        assert!(wrong_password("ERROR: Wrong password : a.txt"));
        assert!(wrong_password("ERROR: x.7z\nCannot open encrypted archive. Wrong password?"));
        assert!(!wrong_password("ERROR: Data Error : a.txt"));
        assert_eq!(message("\nERROR: x\n\nHeaders Error\n"), "7-Zip: ERROR: x Headers Error");
        let args = arguments(Path::new("a b.wim"), Path::new("out dir"), "p w");
        assert_eq!(args, ["x", "-y", "-bsp1", "-bso0", "-oout dir", "-pp w", "--", "a b.wim"]);
    }
}
