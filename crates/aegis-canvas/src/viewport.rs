//! The window onto the world: pan and zoom.

use crate::geometry::{Point, Rect, Size};

/// Smallest zoom. At this zoom a 1080 pixel window shows about a million
/// world pixels of height, which is thousands of nodes; below it nothing is
/// readable anyway.
pub const MIN_ZOOM: f32 = 0.001;
/// Largest zoom.
pub const MAX_ZOOM: f32 = 4.0;

/// What part of the world is shown, and how large.
///
/// `screen = (world - origin) * zoom`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    /// The world point at the top-left corner of the screen area.
    pub origin: Point,
    /// Screen pixels per world pixel.
    pub zoom: f32,
    /// The size of the screen area in pixels.
    pub size: Size,
}

impl Viewport {
    /// Zoom 1, showing the world from its origin.
    #[must_use]
    pub fn new(size: Size) -> Self {
        Viewport {
            origin: Point::default(),
            zoom: 1.0,
            size,
        }
    }

    /// The part of the world that is visible.
    #[must_use]
    pub fn world_rect(&self) -> Rect {
        Rect::new(
            self.origin.x,
            self.origin.y,
            self.size.w / self.zoom,
            self.size.h / self.zoom,
        )
    }

    /// World to screen.
    #[must_use]
    pub fn to_screen(&self, p: Point) -> Point {
        Point::new(
            (p.x - self.origin.x) * self.zoom,
            (p.y - self.origin.y) * self.zoom,
        )
    }

    /// Screen to world.
    #[must_use]
    pub fn to_world(&self, p: Point) -> Point {
        Point::new(
            p.x / self.zoom + self.origin.x,
            p.y / self.zoom + self.origin.y,
        )
    }

    /// Drags the world by a screen distance (dragging right moves the
    /// content right, so the origin moves left).
    pub fn pan_by_screen(&mut self, dx: f32, dy: f32) {
        self.origin.x -= dx / self.zoom;
        self.origin.y -= dy / self.zoom;
    }

    /// Multiplies the zoom by `factor`, keeping the world point under
    /// `screen_point` where it is. Clamped to [`MIN_ZOOM`], [`MAX_ZOOM`].
    pub fn zoom_at(&mut self, screen_point: Point, factor: f32) {
        let anchor = self.to_world(screen_point);
        self.zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        self.origin = Point::new(
            anchor.x - screen_point.x / self.zoom,
            anchor.y - screen_point.y / self.zoom,
        );
    }

    /// Shows all of `world` centred with `margin` screen pixels around it.
    /// Never zooms in past 1:1.
    pub fn fit(&mut self, world: Rect, margin: f32) {
        let avail_w = (self.size.w - 2.0 * margin).max(1.0);
        let avail_h = (self.size.h - 2.0 * margin).max(1.0);
        let zoom = (avail_w / world.size.w.max(1.0))
            .min(avail_h / world.size.h.max(1.0))
            .clamp(MIN_ZOOM, 1.0);
        self.zoom = zoom;
        let c = world.center();
        self.origin = Point::new(
            c.x - self.size.w / zoom / 2.0,
            c.y - self.size.h / zoom / 2.0,
        );
    }

    /// Pans the least distance that brings `world` fully into view with
    /// `margin` screen pixels to spare (keyboard navigation). If it is too
    /// big to fit, its top-left corner is shown.
    pub fn reveal(&mut self, world: Rect, margin: f32) {
        let m = margin / self.zoom;
        let view = self.world_rect();
        let pan = |lo: f32, hi: f32, v_lo: f32, v_hi: f32| {
            if hi - lo + 2.0 * m >= v_hi - v_lo || lo - m < v_lo {
                lo - m
            } else if hi + m > v_hi {
                hi + m - (v_hi - v_lo)
            } else {
                v_lo
            }
        };
        self.origin.x = pan(world.origin.x, world.right(), view.origin.x, view.right());
        self.origin.y = pan(world.origin.y, world.bottom(), view.origin.y, view.bottom());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> Viewport {
        Viewport::new(Size::new(800.0, 600.0))
    }

    #[test]
    fn world_and_screen_round_trip() {
        let mut v = view();
        v.origin = Point::new(100.0, 50.0);
        v.zoom = 2.0;
        let p = Point::new(130.0, 80.0);
        let s = v.to_screen(p);
        assert_eq!(s, Point::new(60.0, 60.0));
        assert_eq!(v.to_world(s), p);
    }

    #[test]
    fn zoom_keeps_the_point_under_the_cursor_fixed() {
        let mut v = view();
        v.origin = Point::new(-30.0, 12.0);
        let cursor = Point::new(400.0, 250.0);
        let before = v.to_world(cursor);
        for factor in [1.25, 0.5, 2.0, 0.8] {
            v.zoom_at(cursor, factor);
            let after = v.to_world(cursor);
            assert!((after.x - before.x).abs() < 1e-3 && (after.y - before.y).abs() < 1e-3);
        }
    }

    #[test]
    fn zoom_is_clamped() {
        let mut v = view();
        v.zoom_at(Point::default(), 1e6);
        assert_eq!(v.zoom, MAX_ZOOM);
        v.zoom_at(Point::default(), 1e-9);
        assert_eq!(v.zoom, MIN_ZOOM);
    }

    #[test]
    fn dragging_moves_the_content_with_the_mouse() {
        let mut v = view();
        v.zoom = 2.0;
        let p = Point::new(40.0, 40.0);
        let before = v.to_screen(p);
        v.pan_by_screen(30.0, -10.0);
        let after = v.to_screen(p);
        assert_eq!((after.x - before.x, after.y - before.y), (30.0, -10.0));
    }

    #[test]
    fn fit_shows_everything_centred_and_never_zooms_in_past_one() {
        let mut v = view();
        v.fit(Rect::new(0.0, 0.0, 1600.0, 300.0), 20.0);
        assert!((v.zoom - 760.0 / 1600.0).abs() < 1e-6);
        let seen = v.world_rect();
        assert!(seen.origin.x <= 0.0 && seen.right() >= 1600.0);
        v.fit(Rect::new(0.0, 0.0, 10.0, 10.0), 20.0);
        assert_eq!(v.zoom, 1.0);
    }

    #[test]
    fn reveal_pans_minimally() {
        let mut v = view();
        v.reveal(Rect::new(100.0, 100.0, 50.0, 50.0), 10.0);
        assert_eq!(v.origin, Point::new(0.0, 0.0), "already visible: no move");
        v.reveal(Rect::new(1000.0, 100.0, 50.0, 50.0), 10.0);
        assert_eq!(v.origin.x, 1060.0 - 800.0);
        assert_eq!(v.origin.y, 0.0);
        v.reveal(Rect::new(-100.0, 100.0, 50.0, 50.0), 10.0);
        assert_eq!(v.origin.x, -110.0);
    }
}
