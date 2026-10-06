//! Running ffmpeg: finding it (with ffprobe beside it) and its version, a file's duration
//! through ffprobe, and a run with progress and cancel. Media presets go through
//! [`run_preset`]; the picture steps (a HEIC decode, a lossy WebP or AVIF encode) through
//! [`run_ffmpeg`].

use std::cell::RefCell;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gezik_core::batch::convert::{parse_ffmpeg_version, parse_progress_block};
use gezik_core::batch::tools::Tool;
use gezik_platform::ChildProcess;

use crate::tools::{Answers, Asked, on_path};

/// How many of ffmpeg's last error lines a failure keeps.
const STDERR_LINES: usize = 20;

/// How long ffprobe may take before it is given up on.
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// An ffmpeg Gezik can run: the program, ffprobe beside it (or on PATH) when there is one,
/// and its (major, minor) version; `None` for a build from git, whose version is unknown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ffmpeg {
    pub ffmpeg: PathBuf,
    pub ffprobe: Option<PathBuf>,
    pub version: Option<(u32, u32)>,
}

/// The ffmpeg to use: the path in settings (a full one), Gezik's download, then PATH; only a
/// program that answers `-version` as ffmpeg counts, whatever its version (callers that need
/// a newer one look at `version`). It starts programs, so this runs off the UI thread.
pub fn find_ffmpeg(data_dir: &Path, configured: Option<&Path>) -> Option<Ffmpeg> {
    let ffmpeg = crate::tools::find(Tool::Ffmpeg, data_dir, configured)?;
    let beside = ffmpeg.with_file_name(format!("ffprobe{}", std::env::consts::EXE_SUFFIX));
    let ffprobe = Some(beside)
        .filter(|path| path.is_file())
        .or_else(|| on_path(&["ffprobe"], std::env::var_os("PATH").as_deref()?));
    let version = version(&ffmpeg);
    Some(Ffmpeg { ffmpeg, ffprobe, version })
}

/// How long `input` plays, in microseconds, from ffprobe; `None` without ffprobe, when it
/// fails or takes longer than 10 s, or when the length is unknown (`N/A`) or zero.
pub fn duration_us(ff: &Ffmpeg, input: &Path) -> Option<u64> {
    let ffprobe = ff.ffprobe.as_deref()?;
    let args: [OsString; 9] = [
        "-v".into(),
        "error".into(),
        "-show_entries".into(),
        "format=duration".into(),
        "-of".into(),
        "default=noprint_wrappers=1:nokey=1".into(),
        "-hide_banner".into(),
        "-i".into(),
        input.as_os_str().to_os_string(),
    ];
    let mut child = ChildProcess::spawn(ffprobe, &args, None).ok()?;
    let mut lines = child.stdout_lines();
    let started = Instant::now();
    let status = child.wait_or_stop(|| started.elapsed() > PROBE_TIMEOUT).ok()??;
    if !status.success() {
        return None;
    }
    // It has ended: this waits only for the reader to pass on what it wrote.
    let first = lines.find(|line| !line.trim().is_empty())?;
    parse_seconds_us(&first).filter(|&us| us > 0)
}

/// `12.345678` (seconds) → 12 345 678 µs; `N/A` and anything else → `None`.
fn parse_seconds_us(text: &str) -> Option<u64> {
    let text = text.trim();
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    if whole.is_empty() || !whole.bytes().all(|b| b.is_ascii_digit()) || !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let micros: String = fraction.chars().chain(std::iter::repeat('0')).take(6).collect();
    whole.parse::<u64>().ok()?.checked_mul(1_000_000)?.checked_add(micros.parse().ok()?)
}

/// Runs a media preset's `args` (from `ffmpeg_args`; `-nostdin` and `-progress pipe:1
/// -nostats` are added when missing) and waits for it. With a `duration` (µs), `on_progress`
/// gets how far it is (0-1, only ever rising, 1.0 at the end); without one it is not called.
/// `stop` is looked at every 50 ms: then ffmpeg is ended with all it started and the answer
/// is `Interrupted` (the caller deletes the unfinished output). A failure carries the exit
/// status and the last 20 lines ffmpeg wrote to its error output.
pub fn run_preset(
    ff: &Ffmpeg,
    mut args: Vec<OsString>,
    duration: Option<u64>,
    on_progress: &mut dyn FnMut(f64),
    stop: &dyn Fn() -> bool,
) -> io::Result<()> {
    let mut front: Vec<OsString> = Vec::new();
    if !args.iter().any(|arg| arg == "-nostdin") {
        front.push("-nostdin".into());
    }
    if !args.iter().any(|arg| arg == "-progress") {
        front.extend(["-progress".into(), "pipe:1".into(), "-nostats".into()]);
    }
    args.splice(0..0, front);
    run(&ff.ffmpeg, args, duration.filter(|&d| d > 0), on_progress, stop)
}

/// Runs `ffmpeg` with `args` (no shell, no window, no input) for a picture step and waits for
/// it, looking every 50 ms whether to `stop` (then it is ended and the answer is
/// `Interrupted`). A failure carries the exit status and the end of ffmpeg's error output.
pub fn run_ffmpeg(ffmpeg: &Path, args: Vec<OsString>, stop: &dyn Fn() -> bool) -> io::Result<()> {
    run(ffmpeg, args, None, &mut |_| {}, stop)
}

/// The `-progress` blocks read so far, and what was reported.
struct Reader<'a> {
    block: Vec<String>,
    duration: Option<u64>,
    reported: f64,
    on_progress: &'a mut dyn FnMut(f64),
}

impl Reader<'_> {
    fn line(&mut self, line: String) {
        let ends_block = line.trim_start().starts_with("progress=");
        self.block.push(line);
        if !ends_block {
            return;
        }
        let lines: Vec<&str> = self.block.iter().map(String::as_str).collect();
        let progress = parse_progress_block(&lines);
        self.block.clear();
        if let (Some(out_us), Some(duration)) = (progress.out_us, self.duration) {
            self.report((out_us as f64 / duration as f64).clamp(0.0, 1.0));
        }
    }

    fn report(&mut self, fraction: f64) {
        if fraction > self.reported {
            self.reported = fraction;
            (self.on_progress)(fraction);
        }
    }
}

fn run(
    program: &Path,
    args: Vec<OsString>,
    duration: Option<u64>,
    on_progress: &mut dyn FnMut(f64),
    stop: &dyn Fn() -> bool,
) -> io::Result<()> {
    let mut child = ChildProcess::spawn(program, &args, None)?;
    let lines = child.stdout_lines();
    let reader = RefCell::new(Reader { block: Vec::new(), duration, reported: 0.0, on_progress });
    let read = || {
        let mut reader = reader.borrow_mut();
        for line in lines.ready() {
            reader.line(line);
        }
    };
    let status = child.wait_or_stop(|| {
        read();
        stop()
    })?;
    let Some(status) = status else {
        return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
    };
    if status.success() {
        read();
        let mut reader = reader.borrow_mut();
        if reader.duration.is_some() {
            reader.report(1.0);
        }
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

/// What `-version` said: whether the program is ffmpeg, and its version.
type Answer = Option<Option<(u32, u32)>>;

/// The (major, minor) version of the ffmpeg at `path` from `ffmpeg -version`; `None` when it
/// is no ffmpeg, does not start or answer in time, or is a build from git.
pub fn version(path: &Path) -> Option<(u32, u32)> {
    ask(path).flatten()
}

/// Whether the program at `path` answers `-version` as ffmpeg (any version, git builds too).
pub fn is_ffmpeg(path: &Path) -> bool {
    ask(path).is_some()
}

/// `-version`'s answer, asked once per program and change time. Only an answer is kept: a
/// program that did not start or answer in time (the first start of a download, while a
/// virus scanner reads it) counts as no ffmpeg this time and is asked again the next. The ask
/// blocks (up to 20 s the first time) without looking at a stop.
fn ask(path: &Path) -> Answer {
    static SEEN: Answers<Answer> = Answers::new();
    SEEN.get(path, |timeout| ask_version(path, timeout)).flatten()
}

fn ask_version(path: &Path, timeout: Duration) -> Asked<Answer> {
    let Ok(mut child) = ChildProcess::spawn(path, ["-version"], None) else { return Asked::NoAnswer };
    let mut lines = child.stdout_lines();
    let started = Instant::now();
    if !matches!(child.wait_or_stop(|| started.elapsed() > timeout), Ok(Some(_))) {
        return Asked::NoAnswer;
    }
    // It ran: what it printed (or not) is the answer.
    Asked::Answer(
        lines
            .next()
            .filter(|first| first.trim_start().starts_with("ffmpeg version "))
            .map(|first| parse_ffmpeg_version(&first)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seconds_become_microseconds() {
        assert_eq!(parse_seconds_us("12.345678"), Some(12_345_678));
        assert_eq!(parse_seconds_us("1.5\r"), Some(1_500_000));
        assert_eq!(parse_seconds_us("3"), Some(3_000_000));
        assert_eq!(parse_seconds_us("0.0000019"), Some(1));
        assert_eq!(parse_seconds_us("N/A"), None);
        assert_eq!(parse_seconds_us(""), None);
        assert_eq!(parse_seconds_us("-1.0"), None);
        assert_eq!(parse_seconds_us(".5"), None);
    }

    #[test]
    fn progress_only_rises_and_needs_a_duration() {
        let mut seen = Vec::new();
        let mut on_progress = |p| seen.push(p);
        let mut reader =
            Reader { block: Vec::new(), duration: Some(1_000_000), reported: 0.0, on_progress: &mut on_progress };
        for line in ["out_time_us=500000", "progress=continue", "out_time_us=400000", "progress=continue"] {
            reader.line(line.into());
        }
        for line in ["out_time_us=N/A", "progress=continue", "out_time_us=2000000", "progress=end"] {
            reader.line(line.into());
        }
        assert_eq!(seen, [0.5, 1.0]);
        let mut calls = 0;
        let mut count = |_| calls += 1;
        let mut reader = Reader { block: Vec::new(), duration: None, reported: 0.0, on_progress: &mut count };
        reader.line("out_time_us=5".into());
        reader.line("progress=end".into());
        assert_eq!(calls, 0);
    }

    #[test]
    fn a_program_that_does_not_start_gives_no_answer() {
        let missing = std::env::temp_dir().join(format!("gezik-no-ffmpeg-{}", std::process::id())).join("ffmpeg");
        assert!(matches!(ask_version(&missing, Duration::from_secs(5)), Asked::NoAnswer));
        assert!(!is_ffmpeg(&missing));
    }

    #[test]
    fn the_error_tail_keeps_twenty_lines() {
        let text: String = (1..=25).map(|n| format!("line {n}\n\n")).collect();
        let tail = stderr_tail(&text);
        assert_eq!(tail.lines().count(), 20);
        assert!(tail.starts_with("line 6\n") && tail.ends_with("line 25"));
    }
}
