//! WASAPI in shared mode: Windows converts Gezik's 48 kHz 16-bit stereo to the device's own
//! format. A full buffer is waited out in 5 ms sleeps (only while something plays). When the
//! device goes (headphones pulled out) the new default device is opened and the clock goes on.

use std::io;
use std::time::Duration;

use windows::Win32::Media::Audio::{
    AUDCLNT_E_DEVICE_INVALIDATED, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
    AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, IAudioClient, IAudioRenderClient, IMMDeviceEnumerator, MMDeviceEnumerator,
    WAVE_FORMAT_PCM, WAVEFORMATEX, eConsole, eRender,
};
use windows::Win32::System::Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx};

use super::{CHANNELS, RATE};

/// The device buffer: 100 ms, in 100 ns units.
const BUFFER: i64 = 1_000_000;

pub struct Out {
    client: IAudioClient,
    render: IAudioRenderClient,
    /// The buffer's size in frames.
    size: u32,
    /// Frames written to this device, and how many were still waiting at the last look.
    written: u64,
    waiting: u32,
    /// Frames heard on devices before this one.
    heard_before: u64,
    started: bool,
    paused: bool,
}

fn error(err: windows::core::Error) -> io::Error {
    io::Error::other(format!("No sound: {}", err.message()))
}

/// The default output device's client, set up for Gezik's format.
fn connect() -> windows::core::Result<(IAudioClient, IAudioRenderClient, u32)> {
    let block = (CHANNELS * 2) as u16;
    let format = WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_PCM as u16,
        nChannels: CHANNELS as u16,
        nSamplesPerSec: RATE,
        nAvgBytesPerSec: RATE * u32::from(block),
        nBlockAlign: block,
        wBitsPerSample: 16,
        cbSize: 0,
    };
    // SAFETY: COM calls on this thread's own objects, with a format that lives through them.
    unsafe {
        // The player's own sound thread: a second call only reports it.
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let devices: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let client: IAudioClient = devices.GetDefaultAudioEndpoint(eRender, eConsole)?.Activate(CLSCTX_ALL, None)?;
        let flags = AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;
        client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, BUFFER, 0, &format, None)?;
        let size = client.GetBufferSize()?;
        let render: IAudioRenderClient = client.GetService()?;
        Ok((client, render, size))
    }
}

impl Out {
    pub fn open() -> io::Result<Out> {
        let (client, render, size) = connect().map_err(error)?;
        Ok(Out { client, render, size, written: 0, waiting: 0, heard_before: 0, started: false, paused: false })
    }

    pub fn write(&mut self, samples: &[i16]) -> io::Result<()> {
        let per_frame = CHANNELS as usize;
        let mut rest = samples;
        while rest.len() >= per_frame {
            match self.fill(rest) {
                Ok(frames) => rest = rest.get(frames * per_frame..).unwrap_or_default(),
                Err(err) if err.code() == AUDCLNT_E_DEVICE_INVALIDATED => self.reconnect()?,
                Err(err) => return Err(error(err)),
            }
        }
        Ok(())
    }

    /// Puts as much of `samples` as fits into the device; how many frames (0: it was full and
    /// a moment passed).
    fn fill(&mut self, samples: &[i16]) -> windows::core::Result<usize> {
        // SAFETY: the buffer GetBuffer gives holds `frames` frames until ReleaseBuffer.
        unsafe {
            self.waiting = self.client.GetCurrentPadding()?;
            let room = self.size.saturating_sub(self.waiting);
            let frames = room.min(u32::try_from(samples.len() / CHANNELS as usize).unwrap_or(u32::MAX));
            if frames == 0 {
                if !self.started && !self.paused {
                    self.client.Start()?;
                    self.started = true;
                }
                std::thread::sleep(Duration::from_millis(5));
                return Ok(0);
            }
            let buffer = self.render.GetBuffer(frames)?;
            let count = frames as usize * CHANNELS as usize;
            std::ptr::copy_nonoverlapping(samples.as_ptr(), buffer.cast::<i16>(), count);
            self.render.ReleaseBuffer(frames, 0)?;
            self.written += u64::from(frames);
            if !self.started && !self.paused {
                self.client.Start()?;
                self.started = true;
            }
            Ok(frames as usize)
        }
    }

    /// The device went: what it had not played yet is lost, the clock goes on from what was
    /// heard.
    fn reconnect(&mut self) -> io::Result<()> {
        self.heard_before += self.written.saturating_sub(u64::from(self.waiting));
        let (client, render, size) = connect().map_err(error)?;
        (self.client, self.render, self.size) = (client, render, size);
        (self.written, self.waiting, self.started) = (0, 0, false);
        Ok(())
    }

    pub fn played_frames(&self) -> u64 {
        // SAFETY: a plain COM call on this thread's client.
        let waiting = unsafe { self.client.GetCurrentPadding() }.unwrap_or(self.waiting);
        self.heard_before + self.written.saturating_sub(u64::from(waiting))
    }

    pub fn pause(&mut self) {
        self.paused = true;
        // SAFETY: a plain COM call on this thread's client.
        let _ = unsafe { self.client.Stop() };
    }

    pub fn resume(&mut self) {
        self.paused = false;
        if self.started {
            // SAFETY: as above.
            let _ = unsafe { self.client.Start() };
        }
    }
}

impl Drop for Out {
    fn drop(&mut self) {
        // SAFETY: as above; the COM objects are released after.
        let _ = unsafe { self.client.Stop() };
    }
}
