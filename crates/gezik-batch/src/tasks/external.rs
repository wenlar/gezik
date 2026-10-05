//! Extracting with 7-Zip, for the formats Gezik does not read itself: `7z x` as a child
//! process without a window, its percentage as progress, the password on its input (never on
//! its command line), a wrong one asked again, and on cancel the process ended with
//! everything it started.

use std::cell::Cell;
use std::ffi::OsString;
use std::io;
use std::path::Path;

use gezik_ops::{Answer, Question, RunCx};
use gezik_platform::ChildProcess;

use super::cancelled;

/// Unpacks `archive` into `dest` (an empty folder) with the 7-Zip at `seven_zip`.
pub(super) fn extract(seven_zip: &Path, archive: &Path, dest: &Path, cx: &RunCx<'_>) -> io::Result<()> {
    // 7-Zip reads `*` and `?` in `-o` as the archive's name; Windows names cannot hold them.
    if cfg!(unix) && dest.to_string_lossy().contains(['*', '?']) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "7-Zip cannot extract into a folder named with * or ?",
        ));
    }
    let size = std::fs::metadata(archive).map_or(0, |meta| meta.len());
    // 7-Zip says how far it is in percent: the archive's size stands for the work.
    cx.found(1, size);
    // An empty password first: an encrypted archive then says "wrong password". 7-Zip reads a
    // password from its input only when it needs one.
    let mut password = String::new();
    let mut asked = false;
    // Bytes counted so far (a try after a wrong password counts only what goes further).
    let counted = Cell::new(0u64);
    loop {
        let input = format!("{password}\n").into_bytes();
        let mut child = ChildProcess::spawn_with_input(seven_zip, arguments(archive, dest), None, Some(input))?;
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
                remove_unsafe_links(archive, dest, cx)?;
                cx.one_done(size.saturating_sub(counted.get()));
                return Ok(());
            }
            // A warning: some files could not be read; the rest is there.
            Some(1) => {
                cx.fail(archive, &io::Error::other(message(&errors)));
                remove_unsafe_links(archive, dest, cx)?;
                cx.one_done(size.saturating_sub(counted.get()));
                return Ok(());
            }
            Some(2) if wrong_password(&errors) => {
                let answer = cx.ask(Question::Password { archive: archive.to_path_buf(), retry: asked });
                // What the wrong password left (empty or broken files) goes before the next try.
                std::fs::remove_dir_all(dest)?;
                std::fs::create_dir(dest)?;
                match answer {
                    Answer::Text(text) => password = text,
                    _ if cx.stopped() => return Err(cancelled()),
                    // Skipped by the user: a note, and nothing is extracted.
                    _ => {
                        cx.skip(archive, &io::Error::new(io::ErrorKind::PermissionDenied, "no password"));
                        cx.one_done(size.saturating_sub(counted.get()));
                        return Ok(());
                    }
                }
                asked = true;
            }
            _ => return Err(io::Error::other(message(&errors))),
        }
    }
}

/// Removes the symbolic links 7-Zip made in `dest` that Gezik's own reader would not have
/// made (on Unix those `check_link` refuses, on Windows all), before anything is placed: a
/// link that leads out of the folder must not be followed by a later move or by the user.
fn remove_unsafe_links(archive: &Path, dest: &Path, cx: &RunCx<'_>) -> io::Result<()> {
    let mut folders = vec![dest.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(&folder)? {
            let entry = entry?;
            let path = entry.path();
            let kind = entry.file_type()?;
            if kind.is_symlink() {
                #[cfg(unix)]
                let refused = crate::archive::check_link(dest, &path, &std::fs::read_link(&path)?).err();
                #[cfg(not(unix))]
                let refused = Some(io::Error::new(io::ErrorKind::Unsupported, "symbolic link skipped"));
                if let Some(err) = refused {
                    // A link to a folder is removed as a folder on Windows.
                    std::fs::remove_file(&path).or_else(|_| std::fs::remove_dir(&path))?;
                    cx.skip(&archive.join(path.strip_prefix(dest).unwrap_or(&path)), &err);
                }
            } else if kind.is_dir() {
                folders.push(path);
            }
        }
    }
    Ok(())
}

/// `x -y -spd -bsp1 -bso0 -o<dest> -- <archive>`: no shell, each one argument, no
/// wildcards in names (`-spd`).
fn arguments(archive: &Path, dest: &Path) -> Vec<OsString> {
    let mut out = OsString::from("-o");
    out.push(dest.as_os_str());
    let mut args: Vec<OsString> = ["x", "-y", "-spd", "-bsp1", "-bso0"].into_iter().map(OsString::from).collect();
    args.push(out);
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
        // No password among them: it goes to 7-Zip's input.
        let args = arguments(Path::new("a b.wim"), Path::new("out dir"));
        assert_eq!(args, ["x", "-y", "-spd", "-bsp1", "-bso0", "-oout dir", "--", "a b.wim"]);
    }
}
