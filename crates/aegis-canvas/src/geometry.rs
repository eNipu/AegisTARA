//! Points, sizes, and rectangles in `f32`.
//!
//! The canvas has two coordinate spaces. **World** space is where the layout
//! puts nodes: x grows to the right, y downward, units are logical pixels at
//! zoom 1. **Screen** space is what a viewport shows: world space moved and
//! scaled. Display lists are in screen space, so a toolkit that cannot
//! transform (Slint, see `spikes/RESULTS.md`) draws them as they are.

/// A point.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    /// Horizontal coordinate.
    pub x: f32,
    /// Vertical coordinate.
    pub y: f32,
}

impl Point {
    /// A point.
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Point { x, y }
    }
}

/// A size.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    /// Width.
    pub w: f32,
    /// Height.
    pub h: f32,
}

impl Size {
    /// A size.
    #[must_use]
    pub const fn new(w: f32, h: f32) -> Self {
        Size { w, h }
    }
}

/// An axis-aligned rectangle: top-left corner and size.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    /// Top-left corner.
    pub origin: Point,
    /// Extent.
    pub size: Size,
}

impl Rect {
    /// A rectangle from its corner and size.
    #[must_use]
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Rect {
            origin: Point::new(x, y),
            size: Size::new(w, h),
        }
    }

    /// Right edge.
    #[must_use]
    pub fn right(&self) -> f32 {
        self.origin.x + self.size.w
    }

    /// Bottom edge.
    #[must_use]
    pub fn bottom(&self) -> f32 {
        self.origin.y + self.size.h
    }

    /// Whether the point lies inside (edges count).
    #[must_use]
    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.origin.x && p.x <= self.right() && p.y >= self.origin.y && p.y <= self.bottom()
    }

    /// Whether the two rectangles overlap or touch.
    #[must_use]
    pub fn intersects(&self, other: &Rect) -> bool {
        self.origin.x <= other.right()
            && other.origin.x <= self.right()
            && self.origin.y <= other.bottom()
            && other.origin.y <= self.bottom()
    }

    /// The smallest rectangle holding both.
    #[must_use]
    pub fn union(&self, other: &Rect) -> Rect {
        let x = self.origin.x.min(other.origin.x);
        let y = self.origin.y.min(other.origin.y);
        Rect::new(
            x,
            y,
            self.right().max(other.right()) - x,
            self.bottom().max(other.bottom()) - y,
        )
    }

    /// Grown by `d` on every side.
    #[must_use]
    pub fn inflate(&self, d: f32) -> Rect {
        Rect::new(
            self.origin.x - d,
            self.origin.y - d,
            self.size.w + 2.0 * d,
            self.size.h + 2.0 * d,
        )
    }

    /// The center.
    #[must_use]
    pub fn center(&self) -> Point {
        Point::new(
            self.origin.x + self.size.w / 2.0,
            self.origin.y + self.size.h / 2.0,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contains_and_intersects_count_touching_edges() {
        let r = Rect::new(0.0, 0.0, 10.0, 10.0);
        assert!(r.contains(Point::new(10.0, 10.0)));
        assert!(!r.contains(Point::new(10.1, 5.0)));
        assert!(r.intersects(&Rect::new(10.0, 0.0, 5.0, 5.0)));
        assert!(!r.intersects(&Rect::new(10.1, 0.0, 5.0, 5.0)));
    }

    #[test]
    fn union_and_inflate() {
        let a = Rect::new(0.0, 0.0, 2.0, 2.0);
        let b = Rect::new(5.0, 5.0, 1.0, 1.0);
        assert_eq!(a.union(&b), Rect::new(0.0, 0.0, 6.0, 6.0));
        assert_eq!(a.inflate(1.0), Rect::new(-1.0, -1.0, 4.0, 4.0));
        assert_eq!(a.center(), Point::new(1.0, 1.0));
    }
}
