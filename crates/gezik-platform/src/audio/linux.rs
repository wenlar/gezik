//! PulseAudio's simple API, `libpulse-simple.so.0` loaded the first time something plays
//! (PipeWire's `pipewire-pulse` serves it too): Gezik starts without it and plays without
//! sound where there is none. The server keeps about 100 ms; the clock is what was written
//! less what it still holds.

use std::ffi::{c_char, c_int, c_void};
use std::io;
use std::sync::OnceLock;

use super::{CHANNELS, RATE};

#[repr(C)]
struct SampleSpec {
    format: c_int,
    rate: u32,
    channels: u8,
}

#[repr(C)]
struct BufferAttr {
    maxlength: u32,
    tlength: u32,
    prebuf: u32,
    minreq: u32,
    fragsize: u32,
}

type New = unsafe extern "C" fn(
    server: *const c_char,
    name: *const c_char,
    direction: c_int,
    device: *const c_char,
    stream: *const c_char,
    spec: *const SampleSpec,
    map: *const c_void,
    attr: *const BufferAttr,
    error: *mut c_int,
) -> *mut c_void;
type Write = unsafe extern "C" fn(*mut c_void, *const c_void, usize, *mut c_int) -> c_int;
type Latency = unsafe extern "C" fn(*mut c_void, *mut c_int) -> u64;
type Flush = unsafe extern "C" fn(*mut c_void, *mut c_int) -> c_int;
type Free = unsafe extern "C" fn(*mut c_void);

const S16LE: c_int = 3;
const PLAYBACK: c_int = 1;

/// The library's functions (it stays loaded once loaded).
struct Simple {
    new: New,
    write: Write,
    latency: Latency,
    flush: Flush,
    free: Free,
}

fn simple() -> Option<&'static Simple> {
    static SIMPLE: OnceLock<Option<Simple>> = OnceLock::new();
    SIMPLE
        .get_or_init(|| {
            // SAFETY: dlopen and dlsym with NUL-ended names; each symbol has the type the
            // library's header gives it.
            unsafe {
                let lib = libc::dlopen(c"libpulse-simple.so.0".as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
                if lib.is_null() {
                    return None;
                }
                let get = |name: &std::ffi::CStr| {
                    let symbol = libc::dlsym(lib, name.as_ptr());
                    (!symbol.is_null()).then_some(symbol)
                };
                Some(Simple {
                    new: std::mem::transmute::<*mut c_void, New>(get(c"pa_simple_new")?),
                    write: std::mem::transmute::<*mut c_void, Write>(get(c"pa_simple_write")?),
                    latency: std::mem::transmute::<*mut c_void, Latency>(get(c"pa_simple_get_latency")?),
                    flush: std::mem::transmute::<*mut c_void, Flush>(get(c"pa_simple_flush")?),
                    free: std::mem::transmute::<*mut c_void, Free>(get(c"pa_simple_free")?),
                })
            }
        })
        .as_ref()
}

pub struct Out {
    lib: &'static Simple,
    stream: *mut c_void,
    /// Frames written since the last flush, and heard before it.
    written: u64,
    heard_before: u64,
}

fn no_sound() -> io::Error {
    io::Error::other(gezik_i18n::t!("No sound: PulseAudio or PipeWire is not available").into_owned())
}

impl Out {
    pub fn open() -> io::Result<Out> {
        let lib = simple().ok_or_else(no_sound)?;
        let spec = SampleSpec { format: S16LE, rate: RATE, channels: CHANNELS as u8 };
        let bytes = RATE / 10 * CHANNELS * 2;
        let attr =
            BufferAttr { maxlength: u32::MAX, tlength: bytes, prebuf: u32::MAX, minreq: u32::MAX, fragsize: u32::MAX };
        let mut error = 0;
        // SAFETY: every pointer lives through the call; NULL server and device are the defaults.
        let stream = unsafe {
            (lib.new)(
                std::ptr::null(),
                c"Gezik".as_ptr(),
                PLAYBACK,
                std::ptr::null(),
                c"Preview".as_ptr(),
                &spec,
                std::ptr::null(),
                &attr,
                &mut error,
            )
        };
        if stream.is_null() {
            return Err(no_sound());
        }
        Ok(Out { lib, stream, written: 0, heard_before: 0 })
    }

    pub fn write(&mut self, samples: &[i16]) -> io::Result<()> {
        let mut error = 0;
        // SAFETY: the stream is open; `samples` lives through the call.
        let status = unsafe {
            (self.lib.write)(self.stream, samples.as_ptr().cast(), std::mem::size_of_val(samples), &mut error)
        };
        if status < 0 {
            return Err(io::Error::other(gezik_i18n::t!("No sound: PulseAudio failed ({})", error)));
        }
        self.written += (samples.len() / CHANNELS as usize) as u64;
        Ok(())
    }

    pub fn played_frames(&self) -> u64 {
        let mut error = 0;
        // SAFETY: the stream is open.
        let latency_us = unsafe { (self.lib.latency)(self.stream, &mut error) };
        let waiting = if error == 0 { latency_us.saturating_mul(u64::from(RATE)) / 1_000_000 } else { 0 };
        self.heard_before + self.written.saturating_sub(waiting)
    }

    /// The simple API cannot hold a stream: what waits is dropped, the clock keeps what was
    /// heard.
    pub fn pause(&mut self) {
        let heard = self.played_frames();
        let mut error = 0;
        // SAFETY: the stream is open.
        unsafe { (self.lib.flush)(self.stream, &mut error) };
        (self.heard_before, self.written) = (heard, 0);
    }

    pub fn resume(&mut self) {}
}

impl Drop for Out {
    fn drop(&mut self) {
        // SAFETY: the stream is open and not used after.
        unsafe { (self.lib.free)(self.stream) };
    }
}
