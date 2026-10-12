//! The zoom surface (spec 13 §5.2, §5.3): a page or a picture scaled into the preview, moved
//! by dragging, zoomed in steps around the pointer. Pure: sizes in, places out. The content is
//! measured in its own units (PDF points, picture pixels); the view in physical pixels.

/// The zoom steps, 25% to 800%.
pub const STEPS: [f32; 11] = [0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 2.0, 3.0, 4.0, 6.0, 8.0];

/// The largest region asked for at once (the PDF view protocol's picture side).
const MAX_REGION: f32 = 4096.0;

/// How the content is scaled.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Fit {
    /// The whole page or picture in the view (the start).
    Page,
    /// Its width fills the view.
    Width,
    /// A zoom step: 1.0 is 100%.
    Zoom(f32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ZoomView {
    /// The content's size in its units (positive).
    content: (f32, f32),
    /// Physical pixels per content unit at 100%.
    unit: f32,
    pub fit: Fit,
    /// The view's size in physical pixels.
    view: (f32, f32),
    /// The view's top-left in the scaled content (negative while it is smaller: centered).
    offset: (f32, f32),
}

impl ZoomView {
    pub fn new(content: (f32, f32), unit: f32, view: (f32, f32)) -> ZoomView {
        let mut zoom = ZoomView { content: (1.0, 1.0), unit: unit.max(0.01), fit: Fit::Page, view, offset: (0.0, 0.0) };
        zoom.set_content(content);
        zoom.set_view(view);
        zoom
    }

    /// Physical pixels per content unit now.
    pub fn scale(&self) -> f32 {
        let (cw, ch) = self.content;
        let (vw, vh) = self.view;
        match self.fit {
            Fit::Page => (vw / cw).min(vh / ch),
            Fit::Width => vw / cw,
            Fit::Zoom(z) => z * self.unit,
        }
        .max(1e-4)
    }

    /// The zoom as a percentage of 100% (the bar's `125%`).
    pub fn percent(&self) -> u32 {
        (self.scale() / self.unit * 100.0).round() as u32
    }

    fn scaled(&self) -> (f32, f32) {
        let s = self.scale();
        (self.content.0 * s, self.content.1 * s)
    }

    /// Keeps the view on the content: centered on an axis where the content is smaller.
    fn clamp(&mut self) {
        let (sw, sh) = self.scaled();
        let axis = |offset: f32, scaled: f32, view: f32| {
            if scaled <= view { -((view - scaled) / 2.0).floor() } else { offset.clamp(0.0, scaled - view) }
        };
        self.offset = (axis(self.offset.0, sw, self.view.0), axis(self.offset.1, sh, self.view.1));
    }

    pub fn set_view(&mut self, view: (f32, f32)) {
        self.view = (view.0.max(1.0), view.1.max(1.0));
        self.clamp();
    }

    /// Another page (its own size): the zoom stays, the view goes to its top (`top`) or
    /// bottom.
    pub fn set_content(&mut self, content: (f32, f32)) {
        let ok = |v: f32| if v.is_finite() && v > 0.0 { v } else { 1.0 };
        self.content = (ok(content.0), ok(content.1));
        self.clamp();
    }

    pub fn go_top(&mut self) {
        self.offset.1 = 0.0;
        self.clamp();
    }

    pub fn go_bottom(&mut self) {
        self.offset.1 = f32::MAX;
        self.clamp();
    }

    /// One step in (`steps` > 0) or out, keeping the content under `anchor` (a point in the
    /// view) where it is; whether the zoom changed.
    pub fn zoom(&mut self, steps: i32, anchor: (f32, f32)) -> bool {
        let old = self.scale();
        let now = old / self.unit;
        let next = if steps > 0 {
            STEPS.iter().copied().find(|&s| s > now * 1.001)
        } else {
            STEPS.iter().rev().copied().find(|&s| s < now * 0.999)
        };
        let Some(next) = next else { return false };
        self.set_fit(Fit::Zoom(next), anchor, old);
        true
    }

    /// Fits or zooms to `fit` around the view's middle.
    pub fn fit_to(&mut self, fit: Fit) {
        self.fit_at(fit, self.middle());
    }

    /// Fits or zooms to `fit`, keeping the content under `anchor` (a point in the view).
    pub fn fit_at(&mut self, fit: Fit, anchor: (f32, f32)) {
        let old = self.scale();
        self.set_fit(fit, anchor, old);
    }

    /// The view's middle (where keys and buttons zoom around).
    pub fn middle(&self) -> (f32, f32) {
        (self.view.0 / 2.0, self.view.1 / 2.0)
    }

    pub fn content(&self) -> (f32, f32) {
        self.content
    }

    /// Whether the view can move `dy` (a wheel: positive moves the content down) without
    /// passing an edge.
    pub fn can_scroll(&self, dy: f32) -> bool {
        if dy > 0.0 { !self.at_top() } else { !self.at_bottom() }
    }

    /// The whole view as a region `(0, 0, w, h)` (at most 4096 a side): what a page of
    /// unknown size is fitted into.
    pub fn view_region(&self) -> (u32, u32, u32, u32) {
        let side = |v: f32| v.ceil().clamp(1.0, MAX_REGION) as u32;
        (0, 0, side(self.view.0), side(self.view.1))
    }

    fn set_fit(&mut self, fit: Fit, (ax, ay): (f32, f32), old: f32) {
        self.fit = fit;
        let k = self.scale() / old;
        self.offset = ((self.offset.0 + ax) * k - ax, (self.offset.1 + ay) * k - ay);
        self.clamp();
    }

    /// Moves the content by `dx`, `dy` physical pixels (a drag); whether it moved.
    pub fn pan(&mut self, dx: f32, dy: f32) -> bool {
        let before = self.offset;
        self.offset = (self.offset.0 - dx, self.offset.1 - dy);
        self.clamp();
        self.offset != before
    }

    /// The view shows the content's bottom (the wheel turns the page there).
    pub fn at_bottom(&self) -> bool {
        self.offset.1 >= self.scaled().1 - self.view.1 - 0.5
    }

    pub fn at_top(&self) -> bool {
        self.offset.1 <= 0.5
    }

    /// The part of the scaled content that shows, in whole physical pixels: `(x, y, w, h)`,
    /// never larger than the view (nor 4096) and never empty.
    pub fn visible(&self) -> (u32, u32, u32, u32) {
        let (sw, sh) = self.scaled();
        let axis = |offset: f32, scaled: f32, view: f32| {
            let start = offset.max(0.0).floor();
            let end = (offset + view).min(scaled).ceil();
            let len = (end - start).clamp(1.0, view.ceil().min(MAX_REGION));
            (start as u32, len as u32)
        };
        let (x, w) = axis(self.offset.0, sw, self.view.0);
        let (y, h) = axis(self.offset.1, sh, self.view.1);
        (x, y, w, h)
    }

    /// Where a picture of `region` of the content drawn at `scale` shows in the view now:
    /// `(x, y, w, h)` in physical pixels (stretched while a sharper one is on its way).
    pub fn place(&self, scale: f32, (x, y, w, h): (u32, u32, u32, u32)) -> (f32, f32, f32, f32) {
        let k = self.scale() / scale.max(1e-4);
        (x as f32 * k - self.offset.0, y as f32 * k - self.offset.1, w as f32 * k, h as f32 * k)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Letter page (612 × 792 points) at 100% = 4/3 px a point, in an 800 × 600 view.
    fn letter() -> ZoomView {
        ZoomView::new((612.0, 792.0), 4.0 / 3.0, (800.0, 600.0))
    }

    #[test]
    fn fitting_shows_the_whole_page_centered() {
        let z = letter();
        assert!((z.scale() - 600.0 / 792.0).abs() < 1e-5);
        let (x, y, w, h) = z.visible();
        assert_eq!((x, y, h), (0, 0, 600));
        assert_eq!(w, (612.0f32 * 600.0 / 792.0).ceil() as u32);
        let (px, py, ..) = z.place(z.scale(), z.visible());
        assert!(px > 100.0 && py == 0.0, "centered across: {px}");
        assert!(z.at_top() && z.at_bottom());
    }

    #[test]
    fn steps_go_through_the_list_and_stop_at_its_ends() {
        let mut z = letter();
        // Fit is 57%: one step in is 75%, out from there 50%, then 25%, then nothing.
        assert!(z.zoom(1, (0.0, 0.0)));
        assert_eq!(z.percent(), 75);
        assert!(z.zoom(-1, (0.0, 0.0)) && z.zoom(-1, (0.0, 0.0)));
        assert_eq!(z.percent(), 25);
        assert!(!z.zoom(-1, (0.0, 0.0)));
        for _ in 0..20 {
            z.zoom(1, (0.0, 0.0));
        }
        assert_eq!(z.percent(), 800);
        assert!(!z.zoom(1, (0.0, 0.0)));
    }

    #[test]
    fn the_point_under_the_pointer_stays() {
        let mut z = letter();
        z.fit_to(Fit::Zoom(2.0));
        z.pan(-300.0, -400.0);
        let anchor = (250.0, 170.0);
        let point = |z: &ZoomView| ((z.offset.0 + anchor.0) / z.scale(), (z.offset.1 + anchor.1) / z.scale());
        let before = point(&z);
        z.zoom(1, anchor);
        let after = point(&z);
        assert!((before.0 - after.0).abs() < 0.01 && (before.1 - after.1).abs() < 0.01, "{before:?} {after:?}");
    }

    #[test]
    fn panning_stops_at_the_edges_and_the_region_never_passes_the_view() {
        let mut z = letter();
        z.fit_to(Fit::Zoom(8.0));
        assert!(z.pan(-1e9, -1e9));
        assert!(z.at_bottom() && !z.at_top());
        assert!(!z.pan(-10.0, -10.0), "already at the end");
        let (x, y, w, h) = z.visible();
        let s = z.scale();
        assert_eq!((w, h), (800, 600));
        assert!((x + w) as f32 <= (612.0 * s).ceil() && (y + h) as f32 <= (792.0 * s).ceil());
        z.go_top();
        assert!(z.at_top());
        // A huge view still asks for at most 4096 a side.
        z.set_view((10_000.0, 10_000.0));
        let (_, _, w, h) = z.visible();
        assert!(w <= 4096 && h <= 4096);
    }

    #[test]
    fn fit_width_and_a_new_page_keep_the_zoom() {
        let mut z = letter();
        z.fit_to(Fit::Width);
        assert!((z.scale() - 800.0 / 612.0).abs() < 1e-5);
        z.set_content((792.0, 612.0));
        assert!((z.scale() - 800.0 / 792.0).abs() < 1e-5, "a landscape page fills the width too");
        z.fit_to(Fit::Zoom(1.0));
        z.set_content((100.0, 100.0));
        assert_eq!(z.percent(), 100);
        z.set_content((f32::NAN, 0.0));
        assert!(z.scale().is_finite());
    }

    #[test]
    fn a_stretched_picture_is_placed_by_its_scale() {
        let mut z = letter();
        let old = z.scale();
        let region = z.visible();
        z.fit_to(Fit::Zoom(2.0));
        let (x, y, w, h) = z.place(old, region);
        let k = z.scale() / old;
        assert!((w - region.2 as f32 * k).abs() < 0.01 && (h - region.3 as f32 * k).abs() < 0.01);
        assert!(x <= 0.0 && y <= 0.0, "grown around the middle: ({x}, {y})");
    }
}
