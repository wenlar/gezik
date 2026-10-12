//! Sound out of the default output device for the preview's player (spec 13 §6.3): 48 kHz,
//! two channels, 16-bit samples. Opened only while something plays; dropping it closes the
//! device. Windows: WASAPI; macOS: AudioQueue; Linux: PulseAudio's simple API, loaded while
//! running (PipeWire serves it too), so Gezik starts without it.

use std::io;

#[cfg(all(unix, not(target_os = "macos")))]
mod linux;
#[cfg(target_os = "macos")]
mod mac;
#[cfg(windows)]
mod windows;

#[cfg(all(unix, not(target_os = "macos")))]
use self::linux as imp;
#[cfg(target_os = "macos")]
use self::mac as imp;
#[cfg(windows)]
use self::windows as imp;

/// Frames (one sample per channel) a second.
pub const RATE: u32 = 48_000;
pub const CHANNELS: u32 = 2;

/// The default output device, open.
pub struct AudioOut(imp::Out);

impl AudioOut {
    /// Opens the default output device; an error says why there is no sound.
    pub fn open() -> io::Result<AudioOut> {
        imp::Out::open().map(AudioOut)
    }

    /// Plays `samples` (left and right in turn); waits while the device is full.
    pub fn write(&mut self, samples: &[i16]) -> io::Result<()> {
        self.0.write(samples)
    }

    /// How many frames were heard so far: the player's clock.
    pub fn played_frames(&self) -> u64 {
        self.0.played_frames()
    }

    pub fn pause(&mut self) {
        self.0.pause();
    }

    pub fn resume(&mut self) {
        self.0.resume();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real device: CI machines have none.
    #[test]
    #[ignore = "needs a sound device"]
    fn a_tenth_of_a_second_of_silence_plays_and_the_device_closes() {
        let mut out = AudioOut::open().expect("a default output device");
        let silence = vec![0i16; (RATE / 10 * CHANNELS) as usize];
        out.write(&silence).unwrap();
        out.write(&silence).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
        let played = out.played_frames();
        assert!(played > 0 && played <= u64::from(RATE / 5), "{played}");
        out.pause();
        out.resume();
    }
}
