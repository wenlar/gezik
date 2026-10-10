//! Caps how many frames are drawn per second (`max-fps`). Slint draws as many frames as
//! the display refreshes (360 on a 360 Hz screen) while it scrolls or animates, and the
//! software renderer redraws the visible text every frame.

use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};

thread_local! {
    /// `max-fps`. Each window keeps its own limit (deviation 14): one limit would make two
    /// windows' frames wait for each other.
    static MAX_FPS: Cell<u32> = const { Cell::new(0) };
}

/// Applies `max-fps` (after every settings load).
pub fn set_max_fps(max_fps: u32) {
    MAX_FPS.with(|m| m.set(max_fps));
}

/// Called for every redraw request of the window `limit` is of, before Slint draws: if its
/// last frame was drawn too recently, waits until the next one may be drawn (at most one
/// frame interval). Frames are delayed, never dropped: a dropped request may be the
/// system's repaint after the window is restored or uncovered, which Slint does not repeat.
pub fn wait_for_frame(limit: &RefCell<FrameLimit>) {
    limit.borrow_mut().set_max_fps(MAX_FPS.with(Cell::get));
    let wait = limit.borrow().wait(Instant::now());
    if !wait.is_zero() {
        std::thread::sleep(wait);
    }
    limit.borrow_mut().allow(Instant::now());
}

pub struct FrameLimit {
    /// Shortest time between two frames; `None`: no limit.
    min_interval: Option<Duration>,
    last: Option<Instant>,
}

impl FrameLimit {
    /// `max_fps` 0 means no limit.
    pub fn new(max_fps: u32) -> FrameLimit {
        let mut limit = FrameLimit { min_interval: None, last: None };
        limit.set_max_fps(max_fps);
        limit
    }

    pub fn set_max_fps(&mut self, max_fps: u32) {
        self.min_interval = (max_fps > 0).then(|| Duration::from_secs(1) / max_fps);
    }

    /// Whether a frame may be drawn at `now`; if so it counts as drawn.
    pub fn allow(&mut self, now: Instant) -> bool {
        let ready = match (self.min_interval, self.last) {
            (Some(interval), Some(last)) => now.saturating_duration_since(last) >= interval,
            _ => true,
        };
        if ready {
            self.last = Some(now);
        }
        ready
    }

    /// How long until the next frame may be drawn (zero if it may now).
    pub fn wait(&self, now: Instant) -> Duration {
        match (self.min_interval, self.last) {
            (Some(interval), Some(last)) => interval.saturating_sub(now.saturating_duration_since(last)),
            _ => Duration::ZERO,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deviation 14: each window's limit is its own, one window's frame never waits for another's.
    #[test]
    fn two_windows_do_not_wait_for_each_other() {
        let t0 = Instant::now();
        let (mut a, mut b) = (FrameLimit::new(100), FrameLimit::new(100));
        assert!(a.allow(t0));
        assert!(b.allow(t0 + Duration::from_millis(1)), "the other window's frame came just before");
        assert_eq!(b.wait(t0 + Duration::from_millis(1)), Duration::from_millis(10));
        assert!(a.wait(t0 + Duration::from_millis(10)).is_zero());
    }

    #[test]
    fn spaces_frames_by_the_interval() {
        let t0 = Instant::now();
        let mut limit = FrameLimit::new(100);
        assert!(limit.allow(t0), "the first frame always draws");
        assert!(!limit.allow(t0 + Duration::from_millis(4)));
        assert_eq!(limit.wait(t0 + Duration::from_millis(4)), Duration::from_millis(6));
        assert!(limit.allow(t0 + Duration::from_millis(10)));
        assert!(!limit.allow(t0 + Duration::from_millis(15)), "measured from the last drawn frame");
        assert_eq!(limit.wait(t0 + Duration::from_millis(30)), Duration::ZERO);
    }

    #[test]
    fn zero_means_no_limit() {
        let t0 = Instant::now();
        let mut limit = FrameLimit::new(0);
        assert!(limit.allow(t0));
        assert!(limit.allow(t0));
        assert_eq!(limit.wait(t0), Duration::ZERO);
    }

    #[test]
    fn changing_the_limit_applies_at_once() {
        let t0 = Instant::now();
        let mut limit = FrameLimit::new(10);
        assert!(limit.allow(t0));
        assert!(!limit.allow(t0 + Duration::from_millis(20)));
        limit.set_max_fps(0);
        assert!(limit.allow(t0 + Duration::from_millis(21)));
    }
}
