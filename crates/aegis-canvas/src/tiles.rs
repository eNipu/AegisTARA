//! Cutting a big attack tree into printable panels.
//!
//! A report page is small and a tree can be wide (deep nesting) or tall (many
//! siblings). Shrinking a big tree onto one page makes the text unreadable,
//! so [`plan`] decides:
//!
//! 1. If the whole tree fits at a readable zoom ([`READ_ZOOM`], the zoom from
//!    which the screen draws text), one panel shows all of it.
//! 2. Otherwise the first panel is an **overview** that fits the whole tree
//!    on one page (boxes and rating colors, no text, like the screen at low
//!    zoom), followed by **detail panels** at a readable zoom that together
//!    cover the tree, in reading order (top to bottom, left to right).
//!    Neighbouring panels overlap by one box, so no box is cut on every
//!    panel that shows it. Panels with nothing in them are skipped.
//! 3. At most [`TileLimits::max_panels`] detail panels are planned; the rest
//!    are counted in [`TilePlan::omitted`], and the caller says so and points
//!    to the outline, which always lists every node.
//!
//! The plan is plain data (viewports), so the PDF, an HTML export and a PNG
//! exporter share it. Cost: O(n log n) for n nodes (one pass over the boxes),
//! plus O(k log n) to render k panels; the number of grid cells is never
//! iterated, so a million-node tree costs no more than its nodes (A9).

use std::collections::BTreeSet;

use crate::geometry::{Point, Rect, Size};
use crate::scene::Scene;
use crate::viewport::Viewport;

/// Zoom from which the renderer draws text (see `render`).
pub const READ_ZOOM: f32 = 0.5;

/// Limits of a plan.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TileLimits {
    /// The most detail panels planned.
    pub max_panels: usize,
    /// Zoom of the detail panels. At least [`READ_ZOOM`].
    pub detail_zoom: f32,
    /// Free space around a panel's content, in screen units.
    pub margin: f32,
}

impl Default for TileLimits {
    fn default() -> Self {
        TileLimits {
            max_panels: 12,
            detail_zoom: 0.6,
            margin: 6.0,
        }
    }
}

/// One detail panel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tile {
    /// Row in the grid, from 0 at the top.
    pub row: u32,
    /// Column in the grid, from 0 at the left.
    pub col: u32,
    /// What it shows.
    pub view: Viewport,
}

/// How to draw a tree on pages.
#[derive(Clone, Debug, PartialEq)]
pub struct TilePlan {
    /// The whole tree on one panel. At a readable zoom when
    /// [`detail`](Self::detail) is empty.
    pub overview: Viewport,
    /// Whether the overview is too small to read and detail panels follow
    /// (or would follow, if the plan was not cut off).
    pub needs_detail: bool,
    /// Detail panels in reading order.
    pub detail: Vec<Tile>,
    /// Grid size of the detail panels.
    pub rows: u32,
    /// Grid size of the detail panels.
    pub cols: u32,
    /// Detail panels left out because of [`TileLimits::max_panels`].
    pub omitted: usize,
}

/// Plans the panels for `scene` on panels of `area` screen units.
///
/// An empty scene gives an empty overview and no panels.
#[must_use]
pub fn plan(scene: &Scene, area: Size, limits: &TileLimits) -> TilePlan {
    let world = scene.bounds();
    let m = limits.margin;
    let avail = Size::new((area.w - 2.0 * m).max(1.0), (area.h - 2.0 * m).max(1.0));
    let fit = (avail.w / world.size.w.max(1.0))
        .min(avail.h / world.size.h.max(1.0))
        .clamp(crate::viewport::MIN_ZOOM, 1.0);

    // The overview panel is only as large as its content.
    let overview_size = Size::new(
        (world.size.w * fit + 2.0 * m).min(area.w),
        (world.size.h * fit + 2.0 * m).min(area.h),
    );
    let mut overview = Viewport::new(overview_size);
    overview.fit(world, m);

    let mut plan = TilePlan {
        overview,
        needs_detail: false,
        detail: Vec::new(),
        rows: 0,
        cols: 0,
        omitted: 0,
    };
    if scene.is_empty() || fit >= READ_ZOOM {
        return plan;
    }
    plan.needs_detail = true;

    let zoom = limits.detail_zoom.max(READ_ZOOM);
    let tile_w = avail.w / zoom;
    let tile_h = avail.h / zoom;
    let cfg = scene.config();
    // Overlap of one box, so a box is whole on at least one panel. A page too
    // small for that still advances by half a panel.
    let step_x = (tile_w - cfg.node.w).max(tile_w / 2.0);
    let step_y = (tile_h - cfg.node.h).max(tile_h / 2.0);
    plan.cols = cells(world.size.w, tile_w, step_x);
    plan.rows = cells(world.size.h, tile_h, step_y);

    // The cells that hold a box: the cell of each box's centre. Every box
    // centre lies in exactly one cell's own (non-overlap) region.
    let mut used: BTreeSet<(u32, u32)> = BTreeSet::new();
    for v in 0..scene.len() {
        let Some(rect) = u32::try_from(v)
            .ok()
            .and_then(|v| scene.node_idx(v))
            .and_then(|idx| scene.node_rect(idx))
        else {
            continue;
        };
        let c = rect.center();
        let col = cell_of(c.x - world.origin.x, step_x, plan.cols);
        let row = cell_of(c.y - world.origin.y, step_y, plan.rows);
        used.insert((row, col));
    }

    plan.omitted = used.len().saturating_sub(limits.max_panels);
    for (row, col) in used.into_iter().take(limits.max_panels) {
        // A panel is never larger than the part of the tree it can show, so a
        // short, wide tree gives short panels that share a page.
        let mut view = Viewport::new(Size::new(
            area.w.min(world.size.w * zoom + 2.0 * m),
            area.h.min(world.size.h * zoom + 2.0 * m),
        ));
        view.zoom = zoom;
        view.origin = Point::new(
            world.origin.x + col as f32 * step_x - m / zoom,
            world.origin.y + row as f32 * step_y - m / zoom,
        );
        plan.detail.push(Tile { row, col, view });
    }
    plan
}

/// Number of cells of size `tile` advancing by `step` to cover `extent`.
fn cells(extent: f32, tile: f32, step: f32) -> u32 {
    if extent <= tile {
        1
    } else {
        // Truncation is the point; the value is small and non-negative.
        1 + ((extent - tile) / step).ceil() as u32
    }
}

/// The cell whose own region (width `step`) holds `offset`.
fn cell_of(offset: f32, step: f32, count: u32) -> u32 {
    ((offset / step).floor().max(0.0) as u32).min(count.saturating_sub(1))
}

/// The world rectangle a tile shows.
#[must_use]
pub fn world_of(tile: &Tile) -> Rect {
    tile.view.world_rect()
}

#[cfg(test)]
mod tests {
    use aegis_model::{AttackTree, GateKind, Node, NodeId};

    use super::*;
    use crate::scene::{LayoutConfig, NodeStyle};

    fn scene(children: usize, depth: usize) -> Scene {
        let mut tree =
            AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "Goal")).expect("tree");
        let mut parents = vec![tree.root()];
        for level in 0..depth {
            let mut next = Vec::new();
            for &p in &parents {
                for i in 0..children {
                    let kind = if level + 1 == depth {
                        Node::step(format!("Step {level}.{i}"))
                    } else {
                        Node::gate(GateKind::And, format!("Gate {level}.{i}"))
                    };
                    next.push(tree.add_child(p, NodeId::new_v7(), kind).expect("add"));
                }
            }
            parents = next;
        }
        Scene::from_tree(&tree, LayoutConfig::default(), |_| NodeStyle::default())
    }

    const PAGE: Size = Size { w: 500.0, h: 640.0 };

    #[test]
    fn a_small_tree_is_one_readable_panel() {
        let s = scene(2, 2);
        let p = plan(&s, PAGE, &TileLimits::default());
        assert!(!p.needs_detail);
        assert!(p.detail.is_empty());
        assert!(p.overview.zoom >= READ_ZOOM);
        assert_eq!(p.omitted, 0);
    }

    #[test]
    fn a_tall_tree_gets_an_overview_and_detail_panels_that_cover_every_box() {
        // 60 siblings: about 4500 world pixels tall.
        let s = scene(60, 1);
        let limits = TileLimits {
            max_panels: 1000,
            ..TileLimits::default()
        };
        let p = plan(&s, PAGE, &limits);
        assert!(p.needs_detail);
        assert!(p.overview.zoom < READ_ZOOM);
        assert!(p.detail.len() > 1);
        assert_eq!(p.omitted, 0);
        assert!(p.detail.iter().all(|t| t.view.zoom >= READ_ZOOM));
        // Every box is whole in at least one panel.
        for v in 0..s.len() as u32 {
            let rect = s.node_rect(s.node_idx(v).expect("idx")).expect("rect");
            assert!(
                p.detail.iter().any(|t| {
                    let w = world_of(t);
                    rect.origin.x >= w.origin.x - 0.01
                        && rect.origin.y >= w.origin.y - 0.01
                        && rect.right() <= w.right() + 0.01
                        && rect.bottom() <= w.bottom() + 0.01
                }),
                "box {v} is cut on every panel"
            );
        }
    }

    #[test]
    fn a_deep_tree_is_cut_along_its_width() {
        // A chain of 12 gates is about 3200 world pixels wide and short.
        let s = scene(1, 12);
        let p = plan(&s, PAGE, &TileLimits::default());
        assert!(p.needs_detail);
        assert!(p.cols > 1);
        assert_eq!(p.rows, 1);
        // Reading order: left to right.
        let cols: Vec<u32> = p.detail.iter().map(|t| t.col).collect();
        let mut sorted = cols.clone();
        sorted.sort_unstable();
        assert_eq!(cols, sorted);
    }

    #[test]
    fn the_panel_count_is_capped_and_the_rest_is_counted() {
        let s = scene(60, 1);
        let p = plan(
            &s,
            PAGE,
            &TileLimits {
                max_panels: 3,
                ..TileLimits::default()
            },
        );
        assert_eq!(p.detail.len(), 3);
        assert!(p.omitted > 0);
    }

    #[test]
    fn panels_that_hold_nothing_are_skipped() {
        // A wide, tall tree: two chains far apart leave empty cells between.
        let s = scene(30, 2);
        let p = plan(
            &s,
            PAGE,
            &TileLimits {
                max_panels: 10_000,
                ..TileLimits::default()
            },
        );
        let grid = (p.rows * p.cols) as usize;
        assert!(p.detail.len() <= grid);
        for t in &p.detail {
            assert!(
                !s.nodes_in(&world_of(t)).is_empty(),
                "tile {},{} is empty",
                t.row,
                t.col
            );
        }
    }

    #[test]
    fn a_short_wide_tree_gets_short_panels() {
        let s = scene(1, 12);
        let p = plan(&s, PAGE, &TileLimits::default());
        assert!(p.detail.iter().all(|t| t.view.size.h < 200.0), "{p:?}");
        assert!(p.detail.iter().all(|t| t.view.size.w <= PAGE.w));
    }

    #[test]
    fn the_plan_is_deterministic() {
        let s = scene(40, 2);
        let a = plan(&s, PAGE, &TileLimits::default());
        let b = plan(&s, PAGE, &TileLimits::default());
        assert_eq!(a, b);
    }

    #[test]
    fn a_degenerate_page_does_not_loop_or_panic() {
        let s = scene(10, 2);
        let tiny = Size { w: 5.0, h: 5.0 };
        let p = plan(&s, tiny, &TileLimits::default());
        assert!(p.detail.len() <= 12);
        assert!(p.overview.zoom > 0.0);
    }

    /// Stress: `AEGIS_STRESS_NODES` boxes (default 30 000; a local release
    /// run uses 1 000 000). The plan costs the boxes, not the grid.
    #[test]
    fn a_big_tree_costs_its_boxes_not_its_grid() {
        let nodes: usize = std::env::var("AEGIS_STRESS_NODES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(30_000);
        let s = scene(nodes, 1);
        let started = std::time::Instant::now();
        let p = plan(&s, PAGE, &TileLimits::default());
        let took = started.elapsed();
        eprintln!("tile plan for {} boxes: {took:?}", s.len());
        assert_eq!(p.detail.len(), 12);
        assert!(p.omitted > 0);
        // Generous: a debug build on a busy runner. Release does 1M in well
        // under a second.
        assert!(took.as_secs() < 20, "{took:?}");
    }
}
