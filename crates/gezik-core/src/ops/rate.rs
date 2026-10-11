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

/// A copy job's speed limit (spec 11 §5.2): a token bucket all its workers share, holding at
/// most one second's bytes. It starts empty, so a short copy does not start with a burst.
#[derive(Debug, Default)]
pub struct Bucket {
    /// Bytes that may go now; below 0, bytes already gone that are not due yet.
    tokens: f64,
    last: Option<Instant>,
}

impl Bucket {
    /// Takes `bytes` at `now` at `rate` bytes a second; returns how long the taker waits before
    /// they are due (zero: at once). What it waits is the bucket's debt, so takers share `rate`.
    pub fn take(&mut self, rate: u64, bytes: u64, now: Instant) -> Duration {
        let rate = rate.max(1) as f64;
        let earned = self.last.map_or(0.0, |last| now.saturating_duration_since(last).as_secs_f64() * rate);
        self.last = Some(now);
        self.tokens = (self.tokens + earned).min(rate) - bytes as f64;
        if self.tokens >= 0.0 { Duration::ZERO } else { Duration::from_secs_f64(-self.tokens / rate) }
    }

    /// The time since the last take was not spent copying (a verification read): it pays what
    /// is owed but saves nothing up, so the next file does not start with a burst.
    pub fn idle(&mut self, rate: u64, now: Instant) {
        if self.tokens > 0.0 {
            self.tokens = 0.0;
        } else {
            self.take(rate, 0, now);
            self.tokens = self.tokens.min(0.0);
        }
        self.last = Some(now);
    }
}

/// `[files] max-speed`: `"20 MB/s"`, `"500 KB/s"`, `"1.5 GB/s"`, `"300 B/s"` in decimal units
/// (`/s` may be left out, any case) as bytes a second; `None` for anything else, zero included.
pub fn parse_speed(text: &str) -> Option<u64> {
    let lower = text.trim().to_ascii_lowercase();
    let lower = lower.strip_suffix("/s").unwrap_or(&lower);
    let split = lower.find(|c: char| !(c.is_ascii_digit() || c == '.'))?;
    let (number, unit) = lower.split_at(split);
    let scale = match unit.trim() {
        "b" => 1.0,
        "kb" => 1e3,
        "mb" => 1e6,
        "gb" => 1e9,
        _ => return None,
    };
    let value = number.parse::<f64>().ok()? * scale;
    (1.0..1e15).contains(&value).then_some(value as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Copies `total` bytes in `chunk`s through `bucket` at `rate`, sleeping on a fake clock;
    /// returns how long it took.
    fn fake_copy(bucket: &mut Bucket, rate: u64, total: u64, chunk: u64, t0: Instant) -> Duration {
        let mut now = t0;
        let mut left = total;
        while left > 0 {
            let n = chunk.min(left);
            left -= n;
            now += bucket.take(rate, n, now);
        }
        now - t0
    }

    #[test]
    fn a_limited_copy_takes_as_long_as_the_rate_says() {
        let took = fake_copy(&mut Bucket::default(), 10_000_000, 50_000_000, 1 << 20, Instant::now());
        let secs = took.as_secs_f64();
        assert!((secs - 5.0).abs() <= 0.25, "50 MB at 10 MB/s took {secs} s");
        // Large chunks (Linux's 8 MiB) keep the average.
        let took = fake_copy(&mut Bucket::default(), 1_000_000, 64 << 20, 8 << 20, Instant::now());
        assert!((took.as_secs_f64() / 67.108_864 - 1.0).abs() <= 0.05, "{took:?}");
    }

    #[test]
    fn workers_share_the_rate() {
        let (t0, mut bucket) = (Instant::now(), Bucket::default());
        // Two workers taking 1 MB each at the same moment: the second waits for both.
        assert_eq!(bucket.take(1_000_000, 1_000_000, t0), Duration::from_secs(1));
        assert_eq!(bucket.take(1_000_000, 1_000_000, t0), Duration::from_secs(2));
    }

    #[test]
    fn idle_time_saves_up_at_most_one_second() {
        let (t0, mut bucket) = (Instant::now(), Bucket::default());
        assert!(bucket.take(1000, 0, t0).is_zero());
        let later = t0 + Duration::from_secs(60);
        assert!(bucket.take(1000, 1000, later).is_zero(), "one second's worth is there");
        assert_eq!(bucket.take(1000, 500, later), Duration::from_millis(500), "not more");
        // The rate changes on the next take.
        assert_eq!(bucket.take(100, 0, later + Duration::from_secs(1)), Duration::from_secs(4));
    }

    #[test]
    fn a_wait_follows_a_new_rate() {
        let (t0, mut bucket) = (Instant::now(), Bucket::default());
        assert_eq!(bucket.take(1000, 10_000, t0), Duration::from_secs(10));
        let later = t0 + Duration::from_secs(1);
        assert_eq!(bucket.take(1000, 0, later), Duration::from_secs(9));
        // The limit goes up tenfold: the 9,000 bytes still owed take 0.9 s.
        assert_eq!(bucket.take(10_000, 0, later), Duration::from_millis(900));
    }

    #[test]
    fn time_spent_verifying_saves_nothing_up() {
        let (t0, mut bucket) = (Instant::now(), Bucket::default());
        assert!(bucket.take(1000, 0, t0).is_zero());
        bucket.idle(1000, t0 + Duration::from_secs(5));
        assert_eq!(bucket.take(1000, 500, t0 + Duration::from_secs(5)), Duration::from_millis(500), "no burst");
        // A debt is still paid by the time that passed.
        let mut owing = Bucket::default();
        assert_eq!(owing.take(1000, 2000, t0), Duration::from_secs(2));
        owing.idle(1000, t0 + Duration::from_secs(1));
        assert_eq!(owing.take(1000, 0, t0 + Duration::from_secs(1)), Duration::from_secs(1));
    }

    #[test]
    fn speeds_parse_in_decimal_units() {
        assert_eq!(parse_speed("20 MB/s"), Some(20_000_000));
        assert_eq!(parse_speed("500 KB/s"), Some(500_000));
        assert_eq!(parse_speed("1.5 GB/s"), Some(1_500_000_000));
        assert_eq!(parse_speed(" 300 B/s "), Some(300));
        assert_eq!(parse_speed("5mb"), Some(5_000_000));
        for bad in ["", "0 MB/s", "20", "fast", "-1 MB/s", "1.2.3 MB/s", "10 MiB/s", "MB/s", "0.1 B/s"] {
            assert_eq!(parse_speed(bad), None, "{bad}");
        }
    }

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
