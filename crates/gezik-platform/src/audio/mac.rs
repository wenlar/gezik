//! AudioToolbox's AudioQueue (plain C functions): three 50 ms buffers; the queue hands each
//! back on its own thread when played, and `write` waits for one. The clock is the queue's
//! own sample time.

use std::cell::Cell;
use std::ffi::c_void;
use std::io;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::Duration;

use super::{CHANNELS, RATE};

type Queue = *mut c_void;
type OsStatus = i32;

#[repr(C)]
struct QueueBuffer {
    capacity: u32,
    data: *mut c_void,
    size: u32,
    user: *mut c_void,
    description_capacity: u32,
    descriptions: *mut c_void,
    description_count: u32,
}

type Buffer = *mut QueueBuffer;

#[repr(C)]
struct StreamDescription {
    sample_rate: f64,
    format_id: u32,
    format_flags: u32,
    bytes_per_packet: u32,
    frames_per_packet: u32,
    bytes_per_frame: u32,
    channels_per_frame: u32,
    bits_per_channel: u32,
    reserved: u32,
}

#[repr(C)]
#[derive(Default)]
struct SmpteTime {
    subframes: i16,
    subframe_divisor: i16,
    counter: u32,
    kind: u32,
    flags: u32,
    hours: i16,
    minutes: i16,
    seconds: i16,
    frames: i16,
}

#[repr(C)]
#[derive(Default)]
struct TimeStamp {
    sample_time: f64,
    host_time: u64,
    rate_scalar: f64,
    word_clock_time: u64,
    smpte: SmpteTime,
    flags: u32,
    reserved: u32,
}

type Played = extern "C" fn(*mut c_void, Queue, Buffer);

#[link(name = "AudioToolbox", kind = "framework")]
unsafe extern "C" {
    fn AudioQueueNewOutput(
        format: *const StreamDescription,
        callback: Played,
        user: *mut c_void,
        run_loop: *const c_void,
        run_loop_mode: *const c_void,
        flags: u32,
        queue: *mut Queue,
    ) -> OsStatus;
    fn AudioQueueAllocateBuffer(queue: Queue, size: u32, buffer: *mut Buffer) -> OsStatus;
    fn AudioQueueEnqueueBuffer(queue: Queue, buffer: Buffer, descriptions: u32, description: *const c_void)
    -> OsStatus;
    fn AudioQueueStart(queue: Queue, start: *const TimeStamp) -> OsStatus;
    fn AudioQueuePause(queue: Queue) -> OsStatus;
    fn AudioQueueStop(queue: Queue, immediate: u8) -> OsStatus;
    fn AudioQueueDispose(queue: Queue, immediate: u8) -> OsStatus;
    fn AudioQueueGetCurrentTime(queue: Queue, timeline: *mut c_void, time: *mut TimeStamp, jumped: *mut u8)
    -> OsStatus;
}

/// `'lpcm'`, signed integer, packed.
const LINEAR_PCM: u32 = 0x6C70_636D;
const SIGNED_PACKED: u32 = 4 | 8;
const BUFFERS: usize = 3;
const BUFFER_FRAMES: u32 = RATE / 20;

/// The buffers the queue handed back.
#[derive(Default)]
struct Free {
    buffers: Mutex<Vec<usize>>,
    came: Condvar,
}

extern "C" fn played(user: *mut c_void, _queue: Queue, buffer: Buffer) {
    // SAFETY: `user` is the `Free` the `Out` keeps alive until the queue is disposed.
    let free = unsafe { &*user.cast::<Free>() };
    free.buffers.lock().unwrap_or_else(PoisonError::into_inner).push(buffer as usize);
    free.came.notify_one();
}

pub struct Out {
    queue: Queue,
    free: Arc<Free>,
    started: bool,
    paused: bool,
    /// The last clock read (the queue says nothing while stopped).
    heard: Cell<u64>,
}

fn check(status: OsStatus) -> io::Result<()> {
    if status == 0 {
        Ok(())
    } else {
        Err(io::Error::other(gezik_i18n::t!("No sound: the audio queue failed ({})", status)))
    }
}

impl Out {
    pub fn open() -> io::Result<Out> {
        let bytes_per_frame = CHANNELS * 2;
        let format = StreamDescription {
            sample_rate: f64::from(RATE),
            format_id: LINEAR_PCM,
            format_flags: SIGNED_PACKED,
            bytes_per_packet: bytes_per_frame,
            frames_per_packet: 1,
            bytes_per_frame,
            channels_per_frame: CHANNELS,
            bits_per_channel: 16,
            reserved: 0,
        };
        let free = Arc::new(Free::default());
        let mut queue: Queue = std::ptr::null_mut();
        let user = Arc::as_ptr(&free).cast_mut().cast::<c_void>();
        // SAFETY: the format lives through the call; `user` outlives the queue (kept in `Out`).
        check(unsafe {
            AudioQueueNewOutput(&format, played, user, std::ptr::null(), std::ptr::null(), 0, &mut queue)
        })?;
        let out = Out { queue, free, started: false, paused: false, heard: Cell::new(0) };
        for _ in 0..BUFFERS {
            let mut buffer: Buffer = std::ptr::null_mut();
            // SAFETY: the queue is open; the buffer belongs to it.
            check(unsafe { AudioQueueAllocateBuffer(out.queue, BUFFER_FRAMES * bytes_per_frame, &mut buffer) })?;
            out.free.buffers.lock().unwrap_or_else(PoisonError::into_inner).push(buffer as usize);
        }
        Ok(out)
    }

    pub fn write(&mut self, samples: &[i16]) -> io::Result<()> {
        let per_buffer = (BUFFER_FRAMES * CHANNELS) as usize;
        for chunk in samples.chunks(per_buffer) {
            let buffer = {
                let mut buffers = self.free.buffers.lock().unwrap_or_else(PoisonError::into_inner);
                loop {
                    if let Some(buffer) = buffers.pop() {
                        break buffer as Buffer;
                    }
                    let (next, waited) = self
                        .free
                        .came
                        .wait_timeout(buffers, Duration::from_secs(2))
                        .unwrap_or_else(PoisonError::into_inner);
                    buffers = next;
                    if waited.timed_out() && buffers.is_empty() {
                        return Err(io::Error::other(gezik_i18n::t!("No sound: the audio queue stopped").into_owned()));
                    }
                }
            };
            // SAFETY: a free buffer is Gezik's until enqueued; `chunk` fits its capacity.
            unsafe {
                std::ptr::copy_nonoverlapping(chunk.as_ptr(), (*buffer).data.cast::<i16>(), chunk.len());
                (*buffer).size = u32::try_from(chunk.len() * 2).unwrap_or(0);
                check(AudioQueueEnqueueBuffer(self.queue, buffer, 0, std::ptr::null()))?;
            }
            if !self.started && !self.paused {
                // SAFETY: the queue is open.
                check(unsafe { AudioQueueStart(self.queue, std::ptr::null()) })?;
                self.started = true;
            }
        }
        Ok(())
    }

    pub fn played_frames(&self) -> u64 {
        let mut time = TimeStamp::default();
        // SAFETY: the queue is open; `time` lives through the call.
        let status =
            unsafe { AudioQueueGetCurrentTime(self.queue, std::ptr::null_mut(), &mut time, std::ptr::null_mut()) };
        if status == 0 && time.sample_time.is_finite() && time.sample_time > 0.0 {
            self.heard.set(self.heard.get().max(time.sample_time as u64));
        }
        self.heard.get()
    }

    pub fn pause(&mut self) {
        self.paused = true;
        // SAFETY: the queue is open.
        unsafe { AudioQueuePause(self.queue) };
    }

    pub fn resume(&mut self) {
        self.paused = false;
        if self.started {
            // SAFETY: the queue is open.
            unsafe { AudioQueueStart(self.queue, std::ptr::null()) };
        }
    }
}

impl Drop for Out {
    fn drop(&mut self) {
        // SAFETY: stopped at once, then disposed with its buffers; no callback comes after.
        unsafe {
            AudioQueueStop(self.queue, 1);
            AudioQueueDispose(self.queue, 1);
        }
    }
}
