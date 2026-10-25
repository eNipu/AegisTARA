//! A spatial index over rectangles (rule A5).
//!
//! Hit testing and viewport culling must not scan every node on every mouse
//! move or frame. An R-tree (`rstar`, bulk-loaded with the OMT algorithm)
//! answers "what is at this point" and "what is inside this rectangle" in
//! O(log n + k) for k results.
//!
//! The tree is derived data (D10): it is rebuilt from the layout and never
//! edited in place.

use rstar::{AABB, RTree, RTreeObject};

use crate::geometry::{Point, Rect};

#[derive(Clone, Copy, Debug)]
struct Entry {
    id: u32,
    min: [f32; 2],
    max: [f32; 2],
}

impl RTreeObject for Entry {
    type Envelope = AABB<[f32; 2]>;

    fn envelope(&self) -> Self::Envelope {
        AABB::from_corners(self.min, self.max)
    }
}

fn envelope(r: &Rect) -> AABB<[f32; 2]> {
    AABB::from_corners([r.origin.x, r.origin.y], [r.right(), r.bottom()])
}

/// Rectangles keyed by a `u32` ID.
#[derive(Debug, Default)]
pub struct SpatialIndex {
    tree: RTree<Entry>,
}

impl SpatialIndex {
    /// Builds the index. Time O(n log n), space O(n).
    #[must_use]
    pub fn bulk_load(rects: impl IntoIterator<Item = (u32, Rect)>) -> Self {
        let entries: Vec<Entry> = rects
            .into_iter()
            .map(|(id, r)| Entry {
                id,
                min: [r.origin.x, r.origin.y],
                max: [r.right(), r.bottom()],
            })
            .collect();
        SpatialIndex {
            tree: RTree::bulk_load(entries),
        }
    }

    /// IDs of every rectangle that overlaps or touches `area`, sorted
    /// ascending so that drawing order is deterministic (P4).
    /// Time O(log n + k log k), k = results.
    #[must_use]
    pub fn query(&self, area: &Rect) -> Vec<u32> {
        let mut ids: Vec<u32> = self
            .tree
            .locate_in_envelope_intersecting(&envelope(area))
            .map(|e| e.id)
            .collect();
        ids.sort_unstable();
        ids
    }

    /// IDs of every rectangle that contains `p`, sorted ascending.
    /// Time O(log n + k).
    #[must_use]
    pub fn at_point(&self, p: Point) -> Vec<u32> {
        self.query(&Rect::new(p.x, p.y, 0.0, 0.0))
    }

    /// Number of rectangles.
    #[must_use]
    pub fn len(&self) -> usize {
        self.tree.size()
    }

    /// Whether the index is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tree.size() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(n: u32) -> SpatialIndex {
        SpatialIndex::bulk_load((0..n * n).map(|i| {
            let (x, y) = ((i % n) as f32, (i / n) as f32);
            (i, Rect::new(x * 10.0, y * 10.0, 8.0, 8.0))
        }))
    }

    #[test]
    fn point_and_area_queries_match_a_linear_scan() {
        let index = grid(20);
        assert_eq!(index.len(), 400);
        assert_eq!(index.at_point(Point::new(5.0, 5.0)), vec![0]);
        assert_eq!(
            index.at_point(Point::new(9.0, 5.0)),
            Vec::<u32>::new(),
            "in the gap"
        );
        let area = Rect::new(18.0, 18.0, 25.0, 25.0);
        let expect: Vec<u32> = (0..400u32)
            .filter(|i| {
                let r = Rect::new((i % 20) as f32 * 10.0, (i / 20) as f32 * 10.0, 8.0, 8.0);
                r.intersects(&area)
            })
            .collect();
        assert_eq!(index.query(&area), expect);
    }

    #[test]
    fn results_are_sorted() {
        let ids = grid(10).query(&Rect::new(0.0, 0.0, 1000.0, 1000.0));
        assert_eq!(ids, (0..100).collect::<Vec<u32>>());
    }

    #[test]
    fn an_empty_index_answers_nothing() {
        let index = SpatialIndex::bulk_load([]);
        assert!(index.is_empty());
        assert!(index.query(&Rect::new(-1e9, -1e9, 2e9, 2e9)).is_empty());
    }
}
