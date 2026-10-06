//! Where Gezik's own menus go (widgets/popup-menu.slint): inside the window, flipped up or
//! left when there is no room below or to the right, and no taller than the window (the
//! list scrolls then). Native menus (Windows and macOS) place themselves.

/// The gap kept between a menu and the window's edges.
pub const MARGIN: f32 = 4.0;

/// Where a menu opens: its top-left corner at `x`, `y` when there is room; flipped, its right
/// edge at `flip_x` or its bottom at `flip_y`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Anchor {
    pub x: f32,
    pub y: f32,
    pub flip_x: f32,
    pub flip_y: f32,
}

impl Anchor {
    /// At the pointer: flipped, the menu ends there.
    pub fn point(x: f32, y: f32) -> Anchor {
        Anchor { x, y, flip_x: x, flip_y: y }
    }

    /// Under a button (its box): flipped, the menu goes above it, or ends at its right edge.
    pub fn below(left: f32, top: f32, right: f32, bottom: f32) -> Anchor {
        Anchor { x: left, y: bottom, flip_x: right, flip_y: top }
    }
}

/// A menu's box in the window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Where a menu of `size` (width, height) goes for `anchor` in a window of `area` (width,
/// height): as asked if it fits, else flipped if that fits, else against the far edge. A
/// menu bigger than the window is cut to it (and scrolls).
pub fn place_menu(anchor: Anchor, size: (f32, f32), area: (f32, f32)) -> Placed {
    let (x, width) = place_axis(anchor.x, anchor.flip_x, size.0, area.0);
    let (y, height) = place_axis(anchor.y, anchor.flip_y, size.1, area.1);
    Placed { x, y, width, height }
}

/// One axis of `place_menu`: the start and the length.
fn place_axis(start: f32, flip_end: f32, length: f32, area: f32) -> (f32, f32) {
    let room = (area - 2.0 * MARGIN).max(0.0);
    let length = length.max(0.0).min(room);
    let far = area - MARGIN;
    if start + length <= far {
        (start.max(MARGIN), length)
    } else if flip_end - length >= MARGIN {
        (flip_end - length, length)
    } else {
        // Room on neither side: as far from the edge it ran past as it can be.
        (far - length, length)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: (f32, f32) = (900.0, 600.0);

    #[test]
    fn a_menu_that_fits_opens_where_asked() {
        let placed = place_menu(Anchor::point(100.0, 120.0), (200.0, 300.0), AREA);
        assert_eq!(placed, Placed { x: 100.0, y: 120.0, width: 200.0, height: 300.0 });
        // Right against the edges, still inside.
        let placed = place_menu(Anchor::point(696.0, 296.0), (200.0, 300.0), AREA);
        assert_eq!((placed.x, placed.y), (696.0, 296.0));
    }

    #[test]
    fn near_the_bottom_it_opens_upward() {
        // A right-click at y 540 with a menu of 352: it ends at the pointer.
        let placed = place_menu(Anchor::point(300.0, 540.0), (280.0, 352.0), AREA);
        assert_eq!((placed.y, placed.height), (540.0 - 352.0, 352.0));
        assert_eq!(placed.x, 300.0);
    }

    #[test]
    fn near_the_right_edge_it_opens_to_the_left() {
        let placed = place_menu(Anchor::point(820.0, 100.0), (280.0, 200.0), AREA);
        assert_eq!(placed.x, 820.0 - 280.0);
        // Bottom right: both.
        let placed = place_menu(Anchor::point(820.0, 540.0), (280.0, 352.0), AREA);
        assert_eq!((placed.x, placed.y), (540.0, 188.0));
    }

    #[test]
    fn under_a_button_it_flips_above_it_or_to_its_right_edge() {
        // A button at 778-864 x 36-66 (Presets, top right): the menu ends at its right edge.
        let button = Anchor::below(778.0, 36.0, 864.0, 66.0);
        let placed = place_menu(button, (300.0, 150.0), AREA);
        assert_eq!((placed.x, placed.y), (864.0 - 300.0, 66.0));
        // A button near the bottom: the menu goes above it.
        let button = Anchor::below(200.0, 500.0, 400.0, 530.0);
        let placed = place_menu(button, (200.0, 200.0), AREA);
        assert_eq!((placed.x, placed.y), (200.0, 300.0));
    }

    #[test]
    fn without_room_on_either_side_it_stays_inside() {
        // 400 high at y 300 in 600: neither below (300 + 400) nor above (300 - 400).
        let placed = place_menu(Anchor::point(100.0, 300.0), (200.0, 400.0), AREA);
        assert_eq!((placed.y, placed.height), (600.0 - MARGIN - 400.0, 400.0));
        assert!(placed.y >= MARGIN && placed.y + placed.height <= AREA.1 - MARGIN);
    }

    #[test]
    fn a_list_taller_than_the_window_is_cut_to_it() {
        // 40 encodings of 32 px under a button at the bottom.
        let button = Anchor::below(202.0, 206.0, 402.0, 236.0);
        let placed = place_menu(button, (280.0, 40.0 * 32.0 + 12.0), AREA);
        assert_eq!((placed.y, placed.height), (MARGIN, 600.0 - 2.0 * MARGIN));
        // And wider than it.
        let placed = place_menu(Anchor::point(50.0, 50.0), (1200.0, 100.0), AREA);
        assert_eq!((placed.x, placed.width), (MARGIN, 900.0 - 2.0 * MARGIN));
    }

    #[test]
    fn an_anchor_outside_the_window_is_pulled_in() {
        let placed = place_menu(Anchor::point(-20.0, -5.0), (100.0, 100.0), AREA);
        assert_eq!((placed.x, placed.y), (MARGIN, MARGIN));
        // A window smaller than the margins: nothing negative.
        let placed = place_menu(Anchor::point(0.0, 0.0), (100.0, 100.0), (6.0, 6.0));
        assert!(placed.width >= 0.0 && placed.height >= 0.0);
    }
}
