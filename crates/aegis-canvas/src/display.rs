//! The display list: what to draw, with no toolkit in sight.
//!
//! A [`DisplayList`] is a flat, ordered list of rectangles, curves,
//! polylines, and single lines of text, in **screen** coordinates, painted
//! back to front. Any renderer can draw it: the Slint shell, the SVG adapter
//! for PDF reports, a PNG exporter. Because it is plain data it can be
//! compared in tests ("snapshot tests on the display list") with no window.
//!
//! Text is already wrapped into lines and positioned by baseline; renderers
//! only need to draw a string at a point.

use crate::geometry::{Point, Rect};
use crate::style::{Paint, Stroke};

/// Where a text item's anchor point sits horizontally.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    /// The anchor is the left edge of the text.
    Start,
    /// The anchor is the right edge of the text.
    End,
}

/// One thing to draw.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    /// A rectangle, optionally filled and stroked, with rounded corners.
    Rect {
        /// Position and size on screen.
        rect: Rect,
        /// Fill, if any.
        fill: Option<Paint>,
        /// Outline, if any.
        stroke: Option<Stroke>,
        /// Corner radius in pixels.
        radius: f32,
        /// The node this rectangle is, for hit testing by the toolkit.
        node: Option<u32>,
    },
    /// A cubic Bézier curve.
    Curve {
        /// Start.
        from: Point,
        /// First control point.
        c1: Point,
        /// Second control point.
        c2: Point,
        /// End.
        to: Point,
        /// Line style.
        stroke: Stroke,
    },
    /// A connected series of straight segments.
    Polyline {
        /// The vertices, at least two.
        points: Vec<Point>,
        /// Line style.
        stroke: Stroke,
    },
    /// One line of text.
    Text {
        /// The baseline anchor.
        at: Point,
        /// The text, already wrapped to one line.
        text: String,
        /// Font size in pixels.
        size: f32,
        /// Bold weight.
        bold: bool,
        /// Color token.
        paint: Paint,
        /// Horizontal anchoring.
        align: Align,
    },
}

/// An ordered list of items, back to front.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DisplayList {
    /// The items, painted in order.
    pub items: Vec<Item>,
}

impl DisplayList {
    /// Number of items.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether nothing is drawn.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The node whose rectangle is drawn topmost at `p`, for toolkits that
    /// prefer to hit test the list they were given.
    #[must_use]
    pub fn node_at(&self, p: Point) -> Option<u32> {
        self.items.iter().rev().find_map(|item| match item {
            Item::Rect {
                rect,
                node: Some(n),
                ..
            } if rect.contains(p) => Some(*n),
            _ => None,
        })
    }
}
