//! When to reload a folder that changes on disk: soon after a change, but never so often that
//! a folder written to all the time (a download, a build, a log) keeps the app busy.

use std::time::{Duration, Instant};

/// Changes this close together make one reload.
pub const QUIET: Duration = Duration::from_millis(200);

/// Reloads start at least this far apart...
pub const MIN_GAP: Duration = Duration::from_secs(1);

/// ...and at least this many times the last reload's cost apart, so reloading takes at most a
/// quarter of the time however big the folder is.
pub const COST_FACTOR: u32 = 4;

#[derive(Debug, Default)]
pub struct RefreshPace {
    /// The first change not covered by a reload yet, and the latest one.
    dirty: Option<(Instant, Instant)>,
    /// When the reload in flight started.
    loading: Option<Instant>,
    /// The last reload: when it started and how long it took.
    last: Option<(Instant, Duration)>,
}

impl RefreshPace {
    pub fn new() -> RefreshPace {
        RefreshPace::default()
    }

    /// The folder changed at `now`.
    pub fn changed(&mut self, now: Instant) {
        self.dirty = Some(match self.dirty {
            Some((first, _)) => (first, now),
            None => (now, now),
        });
    }

    /// When to reload; `None` if nothing changed or a reload is in flight (its end decides).
    pub fn next(&self) -> Option<Instant> {
        let (first, latest) = self.dirty?;
        if self.loading.is_some() {
            return None;
        }
        let gap = self.last.map_or(MIN_GAP, |(_, cost)| MIN_GAP.max(cost * COST_FACTOR));
        // Wait for a quiet moment, but not past one gap after the first change.
        let wanted = (latest + QUIET).min(first + gap);
        Some(match self.last {
            Some((started, _)) => wanted.max(started + gap),
            None => wanted,
        })
    }

    /// A reload of the folder started at `now` (for any reason): it covers the changes so far.
    pub fn started(&mut self, now: Instant) {
        self.loading = Some(now);
        self.dirty = None;
    }

    /// The reload in flight ended at `now`.
    pub fn finished(&mut self, now: Instant) {
        if let Some(started) = self.loading.take() {
            self.last = Some((started, now.saturating_duration_since(started)));
        }
    }

    /// Another folder is shown: nothing is owed to the old one.
    pub fn reset(&mut self) {
        *self = RefreshPace::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn nothing_changed_nothing_to_do() {
        assert_eq!(RefreshPace::new().next(), None);
    }

    #[test]
    fn a_change_reloads_after_a_quiet_moment() {
        let t = Instant::now();
        let mut pace = RefreshPace::new();
        pace.changed(t);
        assert_eq!(pace.next(), Some(t + QUIET));
        pace.changed(t + ms(100));
        assert_eq!(pace.next(), Some(t + ms(100) + QUIET), "a burst makes one reload");
    }

    #[test]
    fn changes_that_never_stop_still_reload_once_a_gap() {
        let t = Instant::now();
        let mut pace = RefreshPace::new();
        pace.changed(t);
        for i in 1..20 {
            pace.changed(t + ms(i * 100));
        }
        assert_eq!(pace.next(), Some(t + MIN_GAP));
    }

    #[test]
    fn reloads_start_a_gap_apart_and_slow_ones_further() {
        let t = Instant::now();
        let mut pace = RefreshPace::new();
        pace.started(t);
        pace.finished(t + ms(50));
        pace.changed(t + ms(100));
        assert_eq!(pace.next(), Some(t + MIN_GAP), "a second after the last one started");

        // A big folder took 600 ms: the next reload starts 2.4 s after it.
        pace.started(t + ms(2000));
        pace.finished(t + ms(2600));
        pace.changed(t + ms(3000));
        assert_eq!(pace.next(), Some(t + ms(2000) + ms(600) * COST_FACTOR));
    }

    #[test]
    fn a_reload_for_another_reason_covers_earlier_changes() {
        let t = Instant::now();
        let mut pace = RefreshPace::new();
        pace.changed(t);
        pace.started(t + ms(50));
        assert_eq!(pace.next(), None, "in flight");
        pace.finished(t + ms(80));
        assert_eq!(pace.next(), None, "covered");
        pace.started(t + ms(100));
        pace.changed(t + ms(120));
        assert_eq!(pace.next(), None, "waits for the reload in flight");
        pace.finished(t + ms(150));
        assert_eq!(pace.next(), Some(t + ms(100) + MIN_GAP), "a change during it needs another");
    }

    #[test]
    fn another_folder_starts_afresh() {
        let t = Instant::now();
        let mut pace = RefreshPace::new();
        pace.started(t);
        pace.finished(t + ms(900));
        pace.changed(t + ms(1000));
        pace.reset();
        assert_eq!(pace.next(), None);
        pace.changed(t + ms(1100));
        assert_eq!(pace.next(), Some(t + ms(1100) + QUIET));
    }
}
