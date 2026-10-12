//! The preview's player (spec 13 §6): nothing runs until Play. Then two ffmpeg processes, one
//! for the picture (PAM frames, each saying its own size) and one for the sound (48 kHz 16-bit
//! stereo), each read on its own thread. The sound device's count of played frames is the
//! clock; without sound (none in the file, no device, or it ended) the wall clock goes on from
//! there. A frame is shown at its time, waited for when early, dropped when more than a frame
//! late. At most one frame is on its way to the UI thread. Dropping the session ends both
//! processes, and its threads close the device.

#![deny(clippy::indexing_slicing)]

use std::ffi::OsString;
use std::io::{self, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use gezik_platform::ChildProcess;
use gezik_platform::audio::{AudioOut, CHANNELS, RATE};
use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

/// The largest picture asked of ffmpeg (spec 13 §17 decision 15: the software renderer draws
/// every frame).
pub const MAX_BOX: (u32, u32) = (1280, 720);
/// The most frames a second.
pub const MAX_FPS: f64 = 30.0;
/// A pause this long ends the processes; Play starts them again where it was.
pub const PAUSE_LIMIT: Duration = Duration::from_secs(60);
/// `J` and `L`.
pub const SKIP: f64 = 10.0;
/// Sound is read and played in 20 ms pieces.
const PIECE_FRAMES: usize = (RATE / 50) as usize;
/// A PAM header is at most this long.
const MAX_HEADER: usize = 256;

/// The box frames are made to fit: the shown area in physical pixels, at most 1280 × 720.
pub fn frame_box((w, h): (f32, f32)) -> (u32, u32) {
    let side = |v: f32, max: u32| (v.round() as u32).clamp(16, max);
    (side(w, MAX_BOX.0), side(h, MAX_BOX.1))
}

/// The frame rate asked of ffmpeg: the source's, at most 30; 25 when not known.
pub fn frame_rate(source: f64) -> f64 {
    if source.is_finite() && source > 0.0 { source.min(MAX_FPS) } else { 25.0 }
}

/// `-ss` for a start past the beginning (before `-i`: ffmpeg seeks to the key frame before it
/// and drops what comes before the time).
fn start_at(args: &mut Vec<OsString>, at: f64) {
    if at > 0.0 {
        args.extend(["-ss".into(), format!("{at:.3}").into()]);
    }
}

/// ffmpeg's arguments for the picture: the first video stream from `at` seconds, `fps` frames
/// a second, fitting `box_` (its aspect kept), as PAM frames.
pub fn video_args(file: &Path, at: f64, (w, h): (u32, u32), fps: f64) -> Vec<OsString> {
    let mut args: Vec<OsString> = ["-nostdin", "-hide_banner", "-loglevel", "error", "-hwaccel", "auto"]
        .into_iter()
        .map(OsString::from)
        .collect();
    start_at(&mut args, at);
    args.extend(["-i".into(), file.as_os_str().to_os_string()]);
    let filter = format!("fps={fps:.3},scale={w}:{h}:force_original_aspect_ratio=decrease");
    for arg in ["-map", "0:v:0", "-an", "-sn", "-vf", &filter, "-pix_fmt", "rgba", "-f", "image2pipe"] {
        args.push(arg.into());
    }
    args.extend(["-c:v".into(), "pam".into(), "pipe:1".into()]);
    args
}

/// ffmpeg's arguments for the sound: the first audio stream from `at` seconds, as 48 kHz
/// 16-bit stereo.
pub fn audio_args(file: &Path, at: f64) -> Vec<OsString> {
    let mut args: Vec<OsString> =
        ["-nostdin", "-hide_banner", "-loglevel", "error"].into_iter().map(OsString::from).collect();
    start_at(&mut args, at);
    args.extend(["-i".into(), file.as_os_str().to_os_string()]);
    let rate = RATE.to_string();
    let channels = CHANNELS.to_string();
    for arg in ["-map", "0:a:0", "-vn", "-sn", "-ac", &channels, "-ar", &rate, "-f", "s16le", "pipe:1"] {
        args.push(arg.into());
    }
    args
}

/// `62.4` → `1:02`; an hour or more `1:02:03`.
pub fn time_text(seconds: f64) -> String {
    let s = if seconds.is_finite() { seconds.max(0.0) as u64 } else { 0 };
    let (h, m, s) = (s / 3600, s / 60 % 60, s % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

/// The next PAM frame's size; `None` at the end of the stream. A header that is not RGBA
/// with 8-bit samples, is longer than 256 bytes, or a frame larger than `max` is an error.
pub fn read_pam_header(input: &mut impl BufRead, max: (u32, u32)) -> io::Result<Option<(u32, u32)>> {
    let bad = |why: &str| io::Error::new(io::ErrorKind::InvalidData, format!("a bad frame header ({why})"));
    let mut header = Vec::with_capacity(96);
    loop {
        let mut byte = [0u8];
        if input.read(&mut byte)? == 0 {
            return if header.is_empty() { Ok(None) } else { Err(bad("cut short")) };
        }
        header.extend_from_slice(&byte);
        if header.ends_with(b"ENDHDR\n") {
            break;
        }
        if header.len() > MAX_HEADER {
            return Err(bad("too long"));
        }
    }
    let text = std::str::from_utf8(&header).map_err(|_| bad("not text"))?;
    let mut lines = text.lines();
    if lines.next() != Some("P7") {
        return Err(bad("not PAM"));
    }
    let (mut w, mut h, mut depth, mut maxval) = (0u32, 0u32, 0u32, 0u32);
    for line in lines {
        let mut words = line.split_whitespace();
        let (Some(key), value) = (words.next(), words.next()) else { continue };
        let number = || value.and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
        match key {
            "WIDTH" => w = number(),
            "HEIGHT" => h = number(),
            "DEPTH" => depth = number(),
            "MAXVAL" => maxval = number(),
            _ => {}
        }
    }
    if depth != 4 || maxval != 255 {
        return Err(bad("not 8-bit RGBA"));
    }
    if w == 0 || h == 0 || w > max.0 || h > max.1 {
        return Err(bad("a size not asked for"));
    }
    Ok(Some((w, h)))
}

/// The volume's gain in 1/65536: `volume`² (0–100), so its steps sound even; 0 when muted.
pub fn gain(volume: u8, muted: bool) -> u32 {
    let v = u32::from(volume.min(100));
    if muted { 0 } else { v * v * 65_536 / 10_000 }
}

/// Scales `samples` by `gain` (1/65536, at most 1): never past the 16-bit range.
pub fn scale(samples: &mut [i16], gain: u32) {
    if gain >= 65_536 {
        return;
    }
    let gain = i64::from(gain);
    for sample in samples {
        *sample = ((i64::from(*sample) * gain) >> 16) as i16;
    }
}

/// When to show a frame due `at` seconds, the clock at `now`, frames `frame` seconds apart.
#[derive(Debug, PartialEq)]
pub enum Pace {
    Wait(Duration),
    Show,
    /// Over a frame late: dropped (never the first, which is the picture after a seek).
    Drop,
}

pub fn pace(now: f64, at: f64, frame: f64, first: bool) -> Pace {
    if now + 0.002 < at {
        Pace::Wait(Duration::from_secs_f64(at - now))
    } else if !first && now > at + frame {
        Pace::Drop
    } else {
        Pace::Show
    }
}

/// The player's clock, seconds from the session's start: the sound device's played frames
/// while there is sound, else the wall clock (stopped while paused).
#[derive(Debug, Clone, PartialEq)]
pub struct Clock {
    base: f64,
    /// The wall clock runs from here.
    since: Option<Instant>,
    /// Frames heard: the sound leads.
    heard: Option<u64>,
}

impl Clock {
    pub fn new(sound: bool, at: Instant) -> Clock {
        Clock { base: 0.0, since: (!sound).then_some(at), heard: sound.then_some(0) }
    }

    pub fn now(&self, at: Instant) -> f64 {
        match (self.heard, self.since) {
            (Some(frames), _) => self.base + frames as f64 / f64::from(RATE),
            (None, Some(since)) => self.base + at.saturating_duration_since(since).as_secs_f64(),
            (None, None) => self.base,
        }
    }

    pub fn heard(&mut self, frames: u64) {
        if self.heard.is_some() {
            self.heard = Some(frames);
        }
    }

    /// The sound ended (or failed): the wall clock goes on from where it was.
    pub fn sound_gone(&mut self, at: Instant, paused: bool) {
        if self.heard.is_some() {
            self.base = self.now(at);
            self.heard = None;
            self.since = (!paused).then_some(at);
        }
    }

    pub fn pause(&mut self, at: Instant) {
        if self.heard.is_none() {
            self.base = self.now(at);
            self.since = None;
        }
    }

    pub fn resume(&mut self, at: Instant) {
        if self.heard.is_none() && self.since.is_none() {
            self.since = Some(at);
        }
    }
}

/// What a session's threads share with the UI thread.
struct Shared {
    paused: bool,
    stop: bool,
    clock: Clock,
}

pub struct Control {
    shared: Mutex<Shared>,
    changed: Condvar,
}

impl Control {
    fn new(sound: bool, paused: bool) -> Control {
        let mut clock = Clock::new(sound, Instant::now());
        // Without sound the wall clock starts with the first frame (`first_frame`), not while
        // ffmpeg starts: else the first frames would come late and be dropped.
        clock.pause(Instant::now());
        Control { shared: Mutex::new(Shared { paused, stop: false, clock }), changed: Condvar::new() }
    }

    fn lock(&self) -> MutexGuard<'_, Shared> {
        self.shared.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn now(&self) -> f64 {
        self.lock().clock.now(Instant::now())
    }

    fn set_paused(&self, paused: bool) {
        let mut shared = self.lock();
        if shared.paused != paused {
            shared.paused = paused;
            if paused { shared.clock.pause(Instant::now()) } else { shared.clock.resume(Instant::now()) }
        }
        self.changed.notify_all();
    }

    /// The first frame is here: a wall clock starts now (unless paused).
    fn first_frame(&self) {
        let mut shared = self.lock();
        if !shared.paused {
            shared.clock.resume(Instant::now());
        }
    }

    fn stop(&self) {
        self.lock().stop = true;
        self.changed.notify_all();
    }

    fn heard(&self, frames: u64) {
        self.lock().clock.heard(frames);
        self.changed.notify_all();
    }

    fn sound_gone(&self) {
        let mut shared = self.lock();
        let paused = shared.paused;
        shared.clock.sound_gone(Instant::now(), paused);
        self.changed.notify_all();
    }

    /// Waits until a frame due `at` is to be shown (`Some(true)`) or dropped; `None` once
    /// stopped.
    fn wait_for(&self, at: f64, frame: f64, first: bool) -> Option<bool> {
        let mut shared = self.lock();
        loop {
            if shared.stop {
                return None;
            }
            if shared.paused {
                shared = self.changed.wait(shared).unwrap_or_else(PoisonError::into_inner);
                continue;
            }
            match pace(shared.clock.now(Instant::now()), at, frame, first) {
                Pace::Wait(time) => {
                    // The sound clock moves every 20 ms: look again by then.
                    let time = time.min(Duration::from_millis(20));
                    shared = self.changed.wait_timeout(shared, time).unwrap_or_else(PoisonError::into_inner).0;
                }
                Pace::Show => return Some(true),
                Pace::Drop => return Some(false),
            }
        }
    }

    /// `None` once stopped, else whether paused.
    fn paused(&self) -> Option<bool> {
        let shared = self.lock();
        (!shared.stop).then_some(shared.paused)
    }

    /// Waits while paused and not stopped.
    fn wait_paused(&self) {
        let mut shared = self.lock();
        while shared.paused && !shared.stop {
            shared = self.changed.wait(shared).unwrap_or_else(PoisonError::into_inner);
        }
    }

    fn stopped(&self) -> bool {
        self.lock().stop
    }
}

/// Where the sound goes: the device, or (none there) nowhere at the wall clock's pace.
pub trait Sink {
    fn write(&mut self, samples: &[i16]) -> io::Result<()>;
    fn played_frames(&self) -> u64;
    fn pause(&mut self);
    fn resume(&mut self);
}

impl Sink for AudioOut {
    fn write(&mut self, samples: &[i16]) -> io::Result<()> {
        AudioOut::write(self, samples)
    }
    fn played_frames(&self) -> u64 {
        AudioOut::played_frames(self)
    }
    fn pause(&mut self) {
        AudioOut::pause(self);
    }
    fn resume(&mut self) {
        AudioOut::resume(self);
    }
}

/// No sound device: the sound is read at the pace it would be heard, so the picture and the
/// end come in time.
pub struct Silent {
    before: f64,
    since: Option<Instant>,
    written: u64,
}

impl Silent {
    pub fn new() -> Silent {
        Silent { before: 0.0, since: Some(Instant::now()), written: 0 }
    }

    fn elapsed(&self) -> f64 {
        self.before + self.since.map_or(0.0, |since| since.elapsed().as_secs_f64())
    }
}

impl Sink for Silent {
    fn write(&mut self, samples: &[i16]) -> io::Result<()> {
        self.written += (samples.len() / CHANNELS as usize) as u64;
        while self.written as f64 / f64::from(RATE) > self.elapsed() + 0.1 {
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    }
    fn played_frames(&self) -> u64 {
        self.written.min((self.elapsed() * f64::from(RATE)) as u64)
    }
    fn pause(&mut self) {
        self.before = self.elapsed();
        self.since = None;
    }
    fn resume(&mut self) {
        self.since = Some(Instant::now());
    }
}

/// What a session's threads tell the UI thread.
pub enum Event {
    /// A frame to show now.
    Frame(SharedPixelBuffer<Rgba8Pixel>),
    /// The device could not be opened: it plays without sound.
    NoSound(String),
    /// The picture's frames ended (how many came).
    VideoEnd(u64),
    /// The sound ended (how many frames came).
    AudioEnd(u64),
}

/// Reads PAM frames from `input` and shows each at its time (`show`); how many came.
fn video_loop(
    input: impl Read,
    control: &Control,
    max: (u32, u32),
    fps: f64,
    show: &dyn Fn(SharedPixelBuffer<Rgba8Pixel>),
) -> u64 {
    let mut input = BufReader::with_capacity(64 * 1024, input);
    let frame = 1.0 / fps;
    let mut count = 0u64;
    while let Ok(Some((w, h))) = read_pam_header(&mut input, max) {
        let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(w, h);
        if input.read_exact(buffer.make_mut_bytes()).is_err() {
            break;
        }
        if count == 0 {
            control.first_frame();
        }
        match control.wait_for(count as f64 * frame, frame, count == 0) {
            None => break,
            Some(true) => show(buffer),
            Some(false) => {}
        }
        count += 1;
    }
    count
}

/// Fills `buf` from `input` as far as it goes; how much (0: the end).
fn read_full(input: &mut impl Read, buf: &mut [u8]) -> usize {
    let mut filled = 0;
    while let Some(rest) = buf.get_mut(filled..).filter(|rest| !rest.is_empty()) {
        match input.read(rest) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
    filled
}

/// Plays the sound from `input` into `out` at the volume's `gain`; how many frames came.
fn audio_loop(mut input: impl Read, out: &mut dyn Sink, control: &Control, gain: &AtomicU32) -> u64 {
    let mut bytes = vec![0u8; PIECE_FRAMES * 4];
    let mut samples = vec![0i16; PIECE_FRAMES * 2];
    let mut frames = 0u64;
    let mut device_paused = false;
    loop {
        match control.paused() {
            None => return frames,
            Some(true) => {
                if !device_paused {
                    out.pause();
                    device_paused = true;
                }
                control.wait_paused();
                continue;
            }
            Some(false) if device_paused => {
                out.resume();
                device_paused = false;
            }
            Some(false) => {}
        }
        // Whole frames only.
        let n = read_full(&mut input, &mut bytes) / 4 * 4;
        if n == 0 {
            break;
        }
        let count = n / 2;
        for (sample, pair) in samples.iter_mut().zip(bytes.as_chunks::<2>().0).take(count) {
            *sample = i16::from_le_bytes(*pair);
        }
        let piece = samples.get_mut(..count).unwrap_or_default();
        scale(piece, gain.load(Ordering::Relaxed));
        if out.write(piece).is_err() {
            break;
        }
        frames += (count / 2) as u64;
        control.heard(out.played_frames());
    }
    // What the device still holds plays out (at most a second).
    let until = Instant::now() + Duration::from_secs(1);
    while out.played_frames() < frames && Instant::now() < until && !control.stopped() {
        std::thread::sleep(Duration::from_millis(10));
        control.heard(out.played_frames());
    }
    frames
}

/// Session numbers: an event of an ended session never matches a newer one.
static NEXT: AtomicU64 = AtomicU64::new(1);

/// What a session plays.
pub struct Start {
    pub ffmpeg: PathBuf,
    pub file: PathBuf,
    /// Seconds into the file.
    pub at: f64,
    /// The frame box and rate, when it has a picture.
    pub video: Option<((u32, u32), f64)>,
    pub audio: bool,
}

/// The processes and threads playing from one place; dropping it ends them.
pub struct Session {
    pub id: u64,
    /// Seconds into the file it started at.
    pub at: f64,
    control: Arc<Control>,
    processes: Vec<ChildProcess>,
    /// A frame is on its way to the UI thread: the next waits (is dropped).
    pub in_flight: Arc<AtomicBool>,
}

impl Session {
    /// Starts `start`'s processes and threads; `deliver` gets the events (on those threads)
    /// with the session's number. `open` opens the sound device on the sound thread.
    pub fn start(
        start: Start,
        gain: Arc<AtomicU32>,
        open: fn() -> io::Result<Box<dyn Sink>>,
        deliver: impl Fn(u64, Event) + Send + Sync + 'static,
    ) -> io::Result<Session> {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let control = Arc::new(Control::new(start.audio, false));
        let in_flight = Arc::new(AtomicBool::new(false));
        let deliver = Arc::new(deliver);
        let mut processes = Vec::new();
        if let Some((max, fps)) = start.video {
            let (process, _, stdout) =
                ChildProcess::spawn_binary(&start.ffmpeg, video_args(&start.file, start.at, max, fps))?;
            processes.push(process);
            let (control, in_flight, deliver) = (control.clone(), in_flight.clone(), deliver.clone());
            std::thread::Builder::new().name("gezik-video".into()).spawn(move || {
                let show = |frame| {
                    if !in_flight.swap(true, Ordering::SeqCst) {
                        deliver(id, Event::Frame(frame));
                    }
                };
                let count = video_loop(stdout, &control, max, fps, &show);
                deliver(id, Event::VideoEnd(count));
            })?;
        }
        if start.audio {
            let (process, _, stdout) = ChildProcess::spawn_binary(&start.ffmpeg, audio_args(&start.file, start.at))?;
            processes.push(process);
            let (control, deliver) = (control.clone(), deliver.clone());
            std::thread::Builder::new().name("gezik-sound".into()).spawn(move || {
                let mut out = open().unwrap_or_else(|why| {
                    deliver(id, Event::NoSound(why.to_string()));
                    Box::new(Silent::new())
                });
                let count = audio_loop(stdout, out.as_mut(), &control, &gain);
                drop(out);
                control.sound_gone();
                deliver(id, Event::AudioEnd(count));
            })?;
        }
        Ok(Session { id, at: start.at, control, processes, in_flight })
    }

    /// Seconds into the file.
    pub fn position(&self) -> f64 {
        self.at + self.control.now()
    }

    pub fn set_paused(&self, paused: bool) {
        self.control.set_paused(paused);
    }

    /// The last line ffmpeg wrote to its error output (once its processes ended).
    pub fn error(&mut self) -> String {
        let mut last = String::new();
        for process in &mut self.processes {
            let _ = process.wait_or_stop(|| false);
            if let Some(line) = process.stderr_text().lines().rev().find(|l| !l.trim().is_empty()) {
                last = line.trim().to_owned();
            }
        }
        last
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // The threads see it and close the device; the processes end with their handles.
        self.control.stop();
    }
}

/// The real sound device.
pub fn open_device() -> io::Result<Box<dyn Sink>> {
    AudioOut::open().map(|out| Box::new(out) as Box<dyn Sink>)
}

/// Where the player is.
#[derive(Debug, Clone, PartialEq)]
pub enum Play {
    /// Nothing runs (before Play, at the end, after a long pause or a seek while paused).
    Stopped,
    /// ffmpeg is looked for and the file probed.
    Finding,
    Playing,
    Paused,
    /// ffmpeg is not there; what to install first for downloading to work.
    NoFfmpeg(Option<String>),
    Failed,
}

/// The player of the shown video or sound file.
pub struct Player {
    pub path: PathBuf,
    pub state: Play,
    /// Seconds into the file.
    pub position: f64,
    pub duration: Option<f64>,
    /// Where the seek bar is dragged (the time shows; it seeks on release).
    pub seeking: Option<f64>,
    /// Why there is no sound.
    pub no_sound: Option<String>,
    pub session: Option<Session>,
    /// ffmpeg and what the probe said: video, sound, frame rate.
    pub found: Option<(PathBuf, bool, bool, f64)>,
    /// The last frame shown (it stays when paused or ended).
    pub frame: Option<Image>,
    /// What of the session ended: the picture's and the sound's frame counts.
    pub ends: (Option<u64>, Option<u64>),
    /// It played to the end: Play starts from the beginning.
    pub at_end: bool,
}

impl Player {
    pub fn new(path: PathBuf) -> Player {
        Player {
            path,
            state: Play::Stopped,
            position: 0.0,
            duration: None,
            seeking: None,
            no_sound: None,
            session: None,
            found: None,
            frame: None,
            ends: (None, None),
            at_end: false,
        }
    }

    pub fn playing(&self) -> bool {
        matches!(self.state, Play::Playing | Play::Finding)
    }

    /// Where it would go: `to` seconds within the file.
    pub fn clamp(&self, to: f64) -> f64 {
        let to = if to.is_finite() { to.max(0.0) } else { 0.0 };
        self.duration.map_or(to, |d| to.min(d))
    }

    /// The bar's time: `1:02 / 3:25` (`--:--` while the length is not known).
    pub fn time(&self) -> String {
        let at = time_text(self.seeking.unwrap_or(self.position));
        format!("{at} / {}", self.duration.map_or_else(|| "--:--".to_owned(), time_text))
    }

    /// The seek bar 0–1; -1 hides it (the length is not known).
    pub fn seek_bar(&self) -> f32 {
        match self.duration {
            Some(d) if d > 0.0 => (self.seeking.unwrap_or(self.position) / d).clamp(0.0, 1.0) as f32,
            _ => -1.0,
        }
    }

    /// The bar's note and link.
    pub fn note(&self) -> (String, &'static str) {
        match &self.state {
            Play::NoFfmpeg(_) => ("Playing needs ffmpeg".into(), "Download"),
            Play::Failed => ("Cannot play this file".into(), ""),
            Play::Finding => ("Opening…".into(), ""),
            _ => (self.no_sound.clone().unwrap_or_default(), ""),
        }
    }

    /// A part of the session ended; whether all of it has (the picture's and the sound's
    /// threads both said so, or the one there is).
    pub fn ended(&mut self, event: &Event) -> bool {
        match event {
            Event::VideoEnd(n) => self.ends.0 = Some(*n),
            Event::AudioEnd(n) => self.ends.1 = Some(*n),
            _ => return false,
        }
        let (video, audio) = self.found.as_ref().map_or((true, true), |f| (f.1, f.2));
        (!video || self.ends.0.is_some()) && (!audio || self.ends.1.is_some())
    }

    /// Nothing at all came out: the file could not be played.
    pub fn nothing_came(&self) -> bool {
        self.ends.0.unwrap_or(0) == 0 && self.ends.1.unwrap_or(0) == 0
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn strings(args: &[OsString]) -> Vec<String> {
        args.iter().map(|a| a.to_string_lossy().into_owned()).collect()
    }

    #[test]
    fn the_command_lines_ask_for_a_bounded_picture_and_48_khz_sound() {
        assert_eq!(frame_box((4000.0, 3000.0)), (1280, 720));
        assert_eq!(frame_box((300.4, 2.0)), (300, 16));
        assert_eq!(frame_rate(59.94), 30.0);
        assert_eq!(frame_rate(23.976), 23.976);
        assert_eq!(frame_rate(0.0), 25.0);
        assert_eq!(frame_rate(f64::NAN), 25.0);
        let video = strings(&video_args(Path::new("/m/a b.mkv"), 0.0, (640, 360), 30.0));
        assert!(!video.contains(&"-ss".to_owned()), "from the start: no seek");
        let at = |args: &[String], what: &str| args.iter().position(|a| a == what).unwrap();
        assert_eq!(video[at(&video, "-i") + 1], "/m/a b.mkv");
        assert_eq!(video[at(&video, "-vf") + 1], "fps=30.000,scale=640:360:force_original_aspect_ratio=decrease");
        assert_eq!(video[at(&video, "-map") + 1], "0:v:0");
        assert!(at(&video, "-hwaccel") < at(&video, "-i"));
        assert_eq!(video.last().unwrap(), "pipe:1");
        let audio = strings(&audio_args(Path::new("/m/a.mp3"), 62.5));
        assert_eq!(audio[at(&audio, "-ss") + 1], "62.500");
        assert!(at(&audio, "-ss") < at(&audio, "-i"), "an input seek");
        assert_eq!(audio[at(&audio, "-ar") + 1], "48000");
        assert_eq!(audio[at(&audio, "-ac") + 1], "2");
        assert_eq!(audio[at(&audio, "-f") + 1], "s16le");
        assert_eq!(audio[at(&audio, "-map") + 1], "0:a:0");
    }

    #[test]
    fn times_read_as_minutes_and_hours() {
        assert_eq!(time_text(0.0), "0:00");
        assert_eq!(time_text(62.9), "1:02");
        assert_eq!(time_text(3723.0), "1:02:03");
        assert_eq!(time_text(-5.0), "0:00");
        assert_eq!(time_text(f64::INFINITY), "0:00");
    }

    fn pam(w: u32, h: u32) -> Vec<u8> {
        let mut frame =
            format!("P7\nWIDTH {w}\nHEIGHT {h}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n").into_bytes();
        frame.resize(frame.len() + (w * h * 4) as usize, 7);
        frame
    }

    #[test]
    fn pam_headers_are_checked() {
        let frame = pam(4, 2);
        let mut input = &frame[..];
        assert_eq!(read_pam_header(&mut input, (8, 8)).unwrap(), Some((4, 2)));
        assert_eq!(input.len(), 32);
        assert_eq!(read_pam_header(&mut &b""[..], (8, 8)).unwrap(), None);
        assert!(read_pam_header(&mut &pam(9, 2)[..], (8, 8)).is_err(), "larger than asked");
        assert!(read_pam_header(&mut &b"P7\nWIDTH 2\nHEIGHT 2\nDEPTH 3\nMAXVAL 255\nENDHDR\n"[..], (8, 8)).is_err());
        assert!(read_pam_header(&mut &b"P6\nENDHDR\n"[..], (8, 8)).is_err());
        assert!(read_pam_header(&mut &b"P7\nWIDTH 2\n"[..], (8, 8)).is_err(), "cut short");
        assert!(read_pam_header(&mut &[b'x'; 1000][..], (8, 8)).is_err(), "no end in 256 bytes");
        assert!(read_pam_header(&mut &b"P7\nWIDTH 0\nHEIGHT 2\nDEPTH 4\nMAXVAL 255\nENDHDR\n"[..], (8, 8)).is_err());
        // Random bytes never panic.
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        for _ in 0..5_000 {
            let bytes: Vec<u8> = (0..64)
                .map(|_| {
                    seed ^= seed << 13;
                    seed ^= seed >> 7;
                    seed ^= seed << 17;
                    [b'P', b'7', b'\n', b' ', b'4', b'E', b'N', b'D', b'H', b'R', 0xff][(seed % 11) as usize]
                })
                .collect();
            let _ = read_pam_header(&mut &bytes[..], (1280, 720));
        }
    }

    #[test]
    fn the_volume_scales_without_overflow() {
        assert_eq!(gain(100, false), 65_536);
        assert_eq!(gain(50, false), 16_384);
        assert_eq!(gain(80, true), 0);
        assert_eq!(gain(200, false), 65_536, "at most 100");
        let mut samples = [i16::MIN, i16::MAX, -1, 1000];
        scale(&mut samples, gain(100, false));
        assert_eq!(samples, [i16::MIN, i16::MAX, -1, 1000]);
        scale(&mut samples, gain(50, false));
        assert_eq!(samples, [-8192, 8191, -1, 250]);
        scale(&mut samples, 0);
        assert_eq!(samples, [0; 4]);
    }

    #[test]
    fn early_frames_wait_late_ones_are_dropped() {
        assert_eq!(pace(0.0, 0.5, 0.04, false), Pace::Wait(Duration::from_secs_f64(0.5)));
        assert_eq!(pace(0.499, 0.5, 0.04, false), Pace::Show);
        assert_eq!(pace(0.53, 0.5, 0.04, false), Pace::Show);
        assert_eq!(pace(0.55, 0.5, 0.04, false), Pace::Drop);
        assert_eq!(pace(9.0, 0.0, 0.04, true), Pace::Show, "the first frame always shows");
    }

    #[test]
    fn the_clock_follows_the_sound_then_the_wall() {
        let t = Instant::now();
        let s = |n: u64| t + Duration::from_millis(n);
        let mut clock = Clock::new(true, t);
        assert_eq!(clock.now(s(500)), 0.0, "the sound leads: nothing heard yet");
        clock.heard(u64::from(RATE));
        assert_eq!(clock.now(s(9000)), 1.0);
        clock.pause(s(1000));
        assert_eq!(clock.now(s(5000)), 1.0, "the device stopped: so did the count");
        clock.sound_gone(s(1000), false);
        assert_eq!(clock.now(s(1500)), 1.5, "the wall clock goes on from the sound's time");
        clock.heard(1);
        assert_eq!(clock.now(s(1500)), 1.5, "a late count is not taken any more");
        clock.pause(s(2000));
        assert_eq!(clock.now(s(9000)), 2.0);
        clock.resume(s(3000));
        assert_eq!(clock.now(s(3250)), 2.25);
        let mut wall = Clock::new(false, t);
        assert_eq!(wall.now(s(100)), 0.1);
        wall.sound_gone(s(100), false);
        assert_eq!(wall.now(s(200)), 0.2, "no sound to lose");
        let mut paused = Clock::new(true, t);
        paused.sound_gone(s(0), true);
        assert_eq!(paused.now(s(800)), 0.0, "sound gone while paused: still paused");
    }

    #[test]
    fn frames_come_at_their_time_and_count_to_the_end() {
        let control = Control::new(false, false);
        let input: Vec<u8> = (0..5).flat_map(|_| pam(4, 2)).collect();
        let shown = Mutex::new(Vec::new());
        let started = Instant::now();
        let count = video_loop(&input[..], &control, (8, 8), 50.0, &|frame| {
            shown.lock().unwrap().push((frame.width(), started.elapsed()));
        });
        assert_eq!(count, 5);
        let shown = shown.into_inner().unwrap();
        assert!(!shown.is_empty() && shown.iter().all(|(w, _)| *w == 4));
        assert!(started.elapsed() >= Duration::from_millis(78), "five frames at 50 a second: 80 ms");
        // A frame larger than asked ends it.
        let control = Control::new(false, false);
        assert_eq!(video_loop(&pam(9, 9)[..], &control, (8, 8), 50.0, &|_| {}), 0);
        // Stopped: nothing more is shown.
        let control = Control::new(false, true);
        control.stop();
        assert_eq!(video_loop(&input[..], &control, (8, 8), 50.0, &|_| panic!("stopped")), 0);
    }

    /// A device that takes everything at once and says it played it.
    #[derive(Default)]
    struct Fake {
        samples: Vec<i16>,
        pauses: u32,
    }

    impl Sink for Fake {
        fn write(&mut self, samples: &[i16]) -> io::Result<()> {
            self.samples.extend_from_slice(samples);
            Ok(())
        }
        fn played_frames(&self) -> u64 {
            (self.samples.len() / 2) as u64
        }
        fn pause(&mut self) {
            self.pauses += 1;
        }
        fn resume(&mut self) {}
    }

    #[test]
    fn sound_is_scaled_into_the_device_and_moves_the_clock() {
        let control = Control::new(true, false);
        let bytes: Vec<u8> = (0..4_801u32).flat_map(|i| ((i % 100) as i16 * 100).to_le_bytes()).collect();
        let mut out = Fake::default();
        let frames = audio_loop(&bytes[..], &mut out, &control, &AtomicU32::new(gain(50, false)));
        assert_eq!(frames, 2_400, "whole frames: the odd last sample is dropped");
        assert_eq!(out.samples.len(), 4_800);
        assert_eq!(out.samples[1], 25, "100 at a quarter");
        assert_eq!(control.now(), 0.05, "2,400 frames heard");
        // Stopped while paused: the device was paused, nothing written.
        let control = Control::new(true, true);
        let stopper = std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(Duration::from_millis(50));
                control.stop();
            });
            let mut out = Fake::default();
            let frames = audio_loop(&bytes[..], &mut out, &control, &AtomicU32::new(65_536));
            (frames, out.pauses, out.samples.len())
        });
        assert_eq!(stopper, (0, 1, 0));
    }

    #[test]
    fn silence_keeps_the_pace_it_would_be_heard_at() {
        let mut silent = Silent::new();
        let started = Instant::now();
        silent.write(&vec![0; (RATE / 5 * 2) as usize]).unwrap();
        assert!(started.elapsed() >= Duration::from_millis(90), "0.2 s written, 0.1 s ahead at most");
        assert!(silent.played_frames() <= u64::from(RATE / 5));
        silent.pause();
        let held = silent.played_frames();
        std::thread::sleep(Duration::from_millis(30));
        assert_eq!(silent.played_frames(), held);
    }

    /// ffmpeg on PATH: a one-second test video plays through both processes to its end.
    #[test]
    #[ignore = "needs ffmpeg on PATH"]
    fn a_real_video_plays_to_its_end() {
        let dir = std::env::temp_dir().join(format!("gezik-play-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("t.mp4");
        let made = std::process::Command::new("ffmpeg")
            .args(["-y", "-loglevel", "error", "-f", "lavfi", "-i", "testsrc=size=320x240:rate=25:duration=1"])
            .args(["-f", "lavfi", "-i", "sine=duration=1", "-shortest"])
            .arg(&file)
            .status()
            .unwrap();
        assert!(made.success());
        let (tx, rx) = std::sync::mpsc::channel();
        let tx = Mutex::new(tx);
        let start = Start {
            ffmpeg: "ffmpeg".into(),
            file: file.clone(),
            at: 0.0,
            video: Some(((160, 160), 25.0)),
            audio: true,
        };
        let session = Session::start(start, Arc::new(AtomicU32::new(65_536)), || Ok(Box::new(Silent::new())), {
            move |_, event| {
                let event = match event {
                    Event::Frame(frame) => format!("frame {}x{}", frame.width(), frame.height()),
                    Event::VideoEnd(n) => format!("video {n}"),
                    Event::AudioEnd(n) => format!("audio {n}"),
                    Event::NoSound(why) => why,
                };
                let _ = tx.lock().unwrap().send(event);
            }
        })
        .unwrap();
        let mut seen = Vec::new();
        while !(seen.iter().any(|e: &String| e.starts_with("video")) && seen.iter().any(|e| e.starts_with("audio"))) {
            let event = rx.recv_timeout(Duration::from_secs(10)).expect("both ends within 10 s");
            if event.starts_with("frame") {
                session.in_flight.store(false, Ordering::SeqCst);
            }
            seen.push(event);
        }
        assert!(seen.contains(&"frame 160x120".to_owned()), "{seen:?}");
        assert!(seen.contains(&"video 25".to_owned()), "{seen:?}");
        let audio: u64 = seen.iter().find_map(|e| e.strip_prefix("audio ")).unwrap().parse().unwrap();
        assert!((47_000..=49_500).contains(&audio), "{audio}");
        assert!(session.position() >= 0.9, "{}", session.position());
        drop(session);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_session_ends_when_both_parts_did_and_says_when_nothing_came() {
        let mut player = Player::new("a.mp4".into());
        player.found = Some(("ffmpeg".into(), true, true, 25.0));
        assert!(!player.ended(&Event::VideoEnd(0)));
        assert!(player.ended(&Event::AudioEnd(0)));
        assert!(player.nothing_came());
        player.ends = (None, None);
        player.found = Some(("ffmpeg".into(), false, true, 0.0));
        assert!(player.ended(&Event::AudioEnd(48_000)));
        assert!(!player.nothing_came());
        player.duration = Some(205.0);
        player.position = 62.0;
        assert_eq!(player.time(), "1:02 / 3:25");
        assert!((player.seek_bar() - 62.0 / 205.0).abs() < 1e-6);
        player.seeking = Some(300.0);
        assert_eq!(player.clamp(300.0), 205.0);
        player.duration = None;
        assert_eq!(player.seek_bar(), -1.0);
        assert_eq!(player.note(), (String::new(), ""));
        player.state = Play::NoFfmpeg(None);
        assert_eq!(player.note(), ("Playing needs ffmpeg".to_owned(), "Download"));
    }
}
