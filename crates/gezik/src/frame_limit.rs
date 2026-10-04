//! Caps how many frames are drawn per second (`max-fps`). Slint draws as many frames as
//! the display refreshes (360 on a 360 Hz screen) while it scrolls or animates, and the
//! software renderer redraws the visible text every frame.

use std::cell::RefCell;
use std::time::{Duration, Instant};

thread_local! {
    /// The limit of this (UI) thread's windows.
    static LIMIT: RefCell<FrameLimit> = RefCell::new(FrameLimit::new(0));
}

/// Applies `max-fps` (after every settings load).
pub fn set_max_fps(max_fps: u32) {
    LIMIT.with(|l| l.borrow_mut().set_max_fps(max_fps));
}

/// Called for every redraw request, before Slint draws: if the last frame was drawn too
/// recently, waits until the next one may be drawn (at most one frame interval). Frames
/// are delayed, never dropped: a dropped request may be the system's repaint after the
/// window is restored or uncovered, which Slint does not repeat.
pub fn wait_for_frame() {
    let wait = LIMIT.with(|l| l.borrow().wait(Instant::now()));
    if !wait.is_zero() {
        std::thread::sleep(wait);
    }
    LIMIT.with(|l| l.borrow_mut().allow(Instant::now()));
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
