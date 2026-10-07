//! Copy speed over the last few seconds and the time left.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Bytes per second, averaged over a sliding window of samples of the running total.
pub struct Rate {
    window: Duration,
    samples: VecDeque<(Instant, u64)>,
}

impl Rate {
    pub fn new(window: Duration) -> Rate {
        Rate { window, samples: VecDeque::new() }
    }

    /// `done` bytes in total at `now`.
    pub fn record(&mut self, now: Instant, done: u64) {
        self.samples.push_back((now, done));
        // Keep one sample older than the window, so the span covers all of it.
        while self.samples.len() > 2 && now.saturating_duration_since(self.samples[1].0) > self.window {
            self.samples.pop_front();
        }
    }

    /// `None` until the samples span half a second.
    pub fn per_second(&self) -> Option<f64> {
        let (first, last) = (self.samples.front()?, self.samples.back()?);
        let span = last.0.saturating_duration_since(first.0).as_secs_f64();
        (span >= 0.5).then(|| last.1.saturating_sub(first.1) as f64 / span)
    }

    /// The time `left` more bytes take at the current speed.
    pub fn remaining(&self, left: u64) -> Option<Duration> {
        let speed = self.per_second().filter(|s| *s > 0.0)?;
        Some(Duration::from_secs_f64((left as f64 / speed).min(360_000.0)))
    }
}

/// `~0:42`, `~12:05`, `~1:05:10`.
pub fn format_eta(left: Duration) -> String {
    let secs = left.as_secs();
    let (h, m, s) = (secs / 3600, secs / 60 % 60, secs % 60);
    if h > 0 { format!("~{h}:{m:02}:{s:02}") } else { format!("~{m}:{s:02}") }
}

/// `84 MB/s`.
pub fn format_rate(per_second: f64, format: crate::view::SizeFormat) -> String {
    format!("{}/s", crate::format_size_in(per_second.max(0.0) as u64, format))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_over_the_window() {
        let t0 = Instant::now();
        let mut rate = Rate::new(Duration::from_secs(5));
        rate.record(t0, 0);
        assert_eq!(rate.per_second(), None, "one sample");
        rate.record(t0 + Duration::from_secs(1), 1000);
        rate.record(t0 + Duration::from_secs(2), 2000);
        assert_eq!(rate.per_second(), Some(1000.0));
        assert_eq!(rate.remaining(5000), Some(Duration::from_secs(5)));
        // Old samples fall out: the speed follows the last few seconds.
        for i in 3..20 {
            rate.record(t0 + Duration::from_secs(i), 2000 + (i - 2) * 100);
        }
        let speed = rate.per_second().unwrap();
        assert!((speed - 100.0).abs() < 1.0, "{speed}");
    }

    #[test]
    fn nothing_moving_has_no_eta() {
        let t0 = Instant::now();
        let mut rate = Rate::new(Duration::from_secs(5));
        rate.record(t0, 10);
        rate.record(t0 + Duration::from_secs(2), 10);
        assert_eq!(rate.remaining(100), None);
    }

    #[test]
    fn formats() {
        assert_eq!(format_eta(Duration::from_secs(42)), "~0:42");
        assert_eq!(format_eta(Duration::from_secs(725)), "~12:05");
        assert_eq!(format_eta(Duration::from_secs(3910)), "~1:05:10");
        assert!(format_rate(84.0 * 1024.0 * 1024.0, crate::view::SizeFormat::Binary).ends_with("/s"));
    }

    #[test]
    fn rates_follow_the_size_format() {
        use crate::view::SizeFormat;
        assert_eq!(format_rate(1_500_000.0, SizeFormat::Decimal), "1.5 MB/s");
        assert_eq!(format_rate(1_500_000.0, SizeFormat::Binary), "1.4 MB/s");
        assert_eq!(format_rate(-3.0, SizeFormat::Binary), "0 B/s");
    }
}
