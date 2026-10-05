//! Running ffmpeg for one picture step (a HEIC decode, a lossy WebP or AVIF encode), and
//! asking a program which ffmpeg version it is. Media conversions with progress come on top
//! of this.

use std::collections::HashMap;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use gezik_core::batch::convert::parse_ffmpeg_version;
use gezik_platform::ChildProcess;

/// How many of ffmpeg's last error lines a failure keeps.
const STDERR_LINES: usize = 20;

/// Runs `ffmpeg` with `args` (no shell, no window, no input) and waits for it, looking every
/// 50 ms whether to `stop` (then it is ended and the answer is `Interrupted`). A failure
/// carries the exit status and the end of what ffmpeg wrote to its error output.
pub fn run_ffmpeg(ffmpeg: &Path, args: Vec<OsString>, stop: &dyn Fn() -> bool) -> io::Result<()> {
    let mut child = ChildProcess::spawn(ffmpeg, &args, None)?;
    // Nothing is read from its output here; the reader thread drops it.
    drop(child.stdout_lines());
    let Some(status) = child.wait_or_stop(stop)? else {
        return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
    };
    if status.success() {
        return Ok(());
    }
    let errors = child.stderr_text();
    let tail = stderr_tail(&errors);
    let code = status.code().map_or_else(|| status.to_string(), |code| format!("exit code {code}"));
    Err(io::Error::other(if tail.is_empty() {
        format!("ffmpeg failed ({code})")
    } else {
        format!("ffmpeg failed ({code}): {tail}")
    }))
}

/// The last lines of an error output, blank ones left out.
pub fn stderr_tail(text: &str) -> String {
    let lines: Vec<&str> = text.lines().map(str::trim_end).filter(|line| !line.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(STDERR_LINES)..].join("\n")
}

/// Versions asked for: the answer, by path and the change time it was for.
type Versions = HashMap<PathBuf, (Option<SystemTime>, Option<(u32, u32)>)>;

/// The (major, minor) version of the ffmpeg at `path` from `ffmpeg -version`; `None` when it
/// does not start, takes longer than 5 s or is a build from git. Asked once per program and
/// change time.
pub fn version(path: &Path) -> Option<(u32, u32)> {
    static SEEN: Mutex<Option<Versions>> = Mutex::new(None);
    let modified = std::fs::metadata(path).and_then(|meta| meta.modified()).ok();
    if let Some((when, version)) =
        SEEN.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).get(path)
        && *when == modified
    {
        return *version;
    }
    let version = ask_version(path);
    SEEN.lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashMap::new)
        .insert(path.to_path_buf(), (modified, version));
    version
}

fn ask_version(path: &Path) -> Option<(u32, u32)> {
    let mut child = ChildProcess::spawn(path, ["-version"], None).ok()?;
    let lines = child.stdout_lines();
    let started = Instant::now();
    child.wait_or_stop(|| started.elapsed() > Duration::from_secs(5)).ok()??;
    lines.take(1).find_map(|line| parse_ffmpeg_version(&line))
}
