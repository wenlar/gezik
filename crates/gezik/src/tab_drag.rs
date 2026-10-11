//! Where a tab dragged out of its window's strip would go (spec 10 §9.5): torn off into a
//! window of its own, onto another Gezik window, or nowhere.

use gezik_core::layout::Rect;

use crate::window_state::TITLE_POINT;
use crate::windows::WindowId;

/// Tearing tabs off works where the strip keeps the pointer outside the window (§9.2 source
/// answer 4: Windows, macOS); on Linux a tab dragged out stays where it was.
pub const TEARS: bool = cfg!(any(target_os = "windows", target_os = "macos"));

/// Where in a window a dragged tab would go: its pane and strip slot (`Some((pane, None))`:
/// over the pane but not its strip, last; `None`: over no pane).
pub type Spot = Option<(usize, Option<usize>)>;

/// What a tab dragged to a point would do on release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tear {
    /// Over its own window: what a drag in the window does.
    Inside,
    /// Over another Gezik window: there (over no pane: its active pane's end).
    Window(WindowId, Spot),
    /// Over no Gezik window: a new window there.
    Empty,
    /// Outside, and tabs are not torn off here (or the point on screen is not known).
    Stay,
}

/// The tab of window `own` dragged to `point` (physical screen pixels; `None`: unknown), `inside`
/// its own window or not. `frames`: the windows on screen, the most recently focused first,
/// with their outer frames; the first one under the point is taken (deviation 12). `strip_of`:
/// where in window `id` the point is (its pane and strip slot).
pub fn target(
    point: Option<(f32, f32)>,
    inside: bool,
    own: WindowId,
    frames: &[(WindowId, Rect)],
    tears: bool,
    strip_of: impl FnOnce(WindowId) -> Spot,
) -> Tear {
    if inside {
        return Tear::Inside;
    }
    let Some((x, y)) = point.filter(|_| tears) else { return Tear::Stay };
    match frames.iter().find(|(_, frame)| frame.contains(x, y)) {
        // Its own title bar or frame.
        Some((id, _)) if *id == own => Tear::Inside,
        Some((id, _)) => Tear::Window(*id, strip_of(*id)),
        None => Tear::Empty,
    }
}

/// Where a torn-off window's outer top left goes (physical pixels): `wanted`, moved just enough
/// that its title bar point [`TITLE_POINT`] stays on `monitor` (x, y, width, height), the one
/// under the pointer; off every monitor, `ensure_visible` would send it to the primary one.
pub fn placed(wanted: (i32, i32), monitor: Option<(i32, i32, i32, i32)>) -> (i32, i32) {
    let Some((x, y, width, height)) = monitor else { return wanted };
    let (dx, dy) = TITLE_POINT;
    (wanted.0.clamp(x - dx, x + width - 1 - dx), wanted.1.clamp(y - dy, y + height - 1 - dy))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_torn_off_window_keeps_its_title_bar_on_the_pointers_monitor() {
        // The rightmost monitor, right of the primary one.
        let monitor = Some((2560, 0, 1920, 1080));
        assert_eq!(placed((3000, 400), monitor), (3000, 400));
        // Grabbed near the right edge: the point 100 px in would be past it.
        assert_eq!(placed((4400, 400), monitor), (4379, 400));
        // And near the top or the left edge.
        assert_eq!(placed((3000, -30), monitor), (3000, -20));
        assert_eq!(placed((2400, 400), monitor), (2460, 400));
        assert_eq!(placed((5000, 2000), monitor), (4379, 1059));
        // No monitor known: where it was wanted.
        assert_eq!(placed((4400, -30), None), (4400, -30));
    }

    fn rect(x: f32, y: f32) -> Rect {
        Rect { x, y, width: 100.0, height: 100.0 }
    }

    #[test]
    fn a_tab_dragged_out_goes_where_the_pointer_is() {
        let ids = crate::windows::tests::ids(3);
        let (own, other, third) = (ids[0], ids[1], ids[2]);
        // `third` overlaps `other` and was focused more recently.
        let frames = [(third, rect(150.0, 0.0)), (own, rect(0.0, 0.0)), (other, rect(200.0, 0.0))];
        let strip = |at| move |_| at;
        let go = |point, inside, tears, at: Spot| target(Some(point), inside, own, &frames, tears, strip(at));
        assert_eq!(go((50.0, 50.0), true, true, None), Tear::Inside);
        // Its own title bar (in the frame, outside the window).
        assert_eq!(go((50.0, 5.0), false, true, None), Tear::Inside);
        assert_eq!(go((280.0, 10.0), false, true, Some((0, Some(2)))), Tear::Window(other, Some((0, Some(2)))));
        assert_eq!(go((280.0, 50.0), false, true, Some((1, None))), Tear::Window(other, Some((1, None))));
        // Where two windows overlap: the one focused last.
        assert_eq!(go((220.0, 50.0), false, true, None), Tear::Window(third, None));
        assert_eq!(go((500.0, 500.0), false, true, None), Tear::Empty);
        // Linux: every point outside stays, over another window too.
        assert_eq!(go((280.0, 10.0), false, false, Some((0, Some(2)))), Tear::Stay);
        assert_eq!(go((500.0, 500.0), false, false, None), Tear::Stay);
        assert_eq!(go((50.0, 50.0), true, false, None), Tear::Inside);
        // The point on screen not known.
        assert_eq!(target(None, false, own, &frames, true, strip(None)), Tear::Stay);
    }
}
