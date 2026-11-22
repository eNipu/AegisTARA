//! A laid-out attack tree, ready to draw and to hit test.
//!
//! [`Scene::from_tree`] reads an [`AttackTree`], lays it out (linear time),
//! and builds two spatial indexes. After that:
//!
//! - a **rating change** is [`Scene::restyle`]: O(1), no layout, because the
//!   node boxes have a fixed size and ratings only change their text and
//!   color;
//! - a **structure change** rebuilds the scene in O(n log n), dominated by
//!   the index build.
//!
//! # Shared subtrees
//!
//! The model allows a node to have several parents (a DAG). Each node has one
//! *owner*, the parent it was added under. The layout draws the **owner
//! tree**. Edges from other parents are drawn too, dashed, and may cross.
//!
//! # Memory
//!
//! Struct of arrays (D4): about 60 bytes per node for the scene plus the
//! R-tree entries (about 40 bytes per node and per edge).

use std::sync::Arc;

use aegis_model::{AttackTree, GateKind, NodeIdx};

use crate::geometry::{Point, Rect, Size};
use crate::index::SpatialIndex;
use crate::layout::{Forest, layout};
use crate::style::Tone;

/// Sizes and gaps, in world pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutConfig {
    /// Every node box has this size.
    pub node: Size,
    /// Horizontal space between a node and its children.
    pub gap_x: f32,
    /// Vertical space between neighbouring nodes.
    pub gap_y: f32,
}

impl Default for LayoutConfig {
    /// The sizes of the design mockup: 210 x 60 boxes.
    fn default() -> Self {
        LayoutConfig {
            node: Size::new(210.0, 60.0),
            gap_x: 56.0,
            gap_y: 16.0,
        }
    }
}

/// What kind of node a box is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    /// A leaf attack step.
    Step,
    /// A gate.
    Gate(GateKind),
}

/// How a node is annotated on the canvas. The caller computes these from
/// the evaluation (the canvas knows nothing about ratings).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NodeStyle {
    /// Bold text at the bottom left, for example `13 · High`.
    pub score: Option<String>,
    /// The rating color of the bar on the left.
    pub tone: Tone,
    /// The node is on the highlighted (deciding) path.
    pub critical: bool,
    /// The node is a ghost proposal from a suggestion branch (FND-05).
    pub ghost: bool,
    /// What a step goes through, `ADAS ECU / JTAG`, shown as a chip (CFG-01).
    pub via: Option<String>,
    /// What caps a step's feasibility, `Secure boot on ADAS ECU`, shown as a
    /// shield badge and spoken to screen readers (CFG-04).
    pub capped: Option<String>,
    /// Human-readable exclusion reasons when the node is excluded by
    /// applicability (`None` when included).
    pub excluded: Option<String>,
}

/// A connection between two nodes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Edge {
    /// Parent node.
    pub from: u32,
    /// Child node.
    pub to: u32,
    /// The parent is not the child's owner: a shared-subtree link.
    pub link: bool,
}

/// Which way to move on the keyboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// To the owner (the tree grows rightwards, so the parent is to the left).
    Left,
    /// To the child nearest in height.
    Right,
    /// To the node above in the same column.
    Up,
    /// To the node below in the same column.
    Down,
}

/// A laid-out tree.
#[derive(Debug)]
pub struct Scene {
    config: LayoutConfig,
    ids: Vec<NodeIdx>,
    labels: Vec<Arc<str>>,
    kinds: Vec<NodeKind>,
    styles: Vec<NodeStyle>,
    pos: Vec<Point>,
    owner: Vec<Option<u32>>,
    forest: Forest,
    edges: Vec<Edge>,
    /// `edge_start[v]..edge_start[v + 1]` are the edges leaving `v`.
    edge_start: Vec<u32>,
    edge_critical: Vec<bool>,
    /// How many shared (dashed) edges end at each node.
    shared_in: Vec<u32>,
    root: u32,
    bounds: Rect,
    node_index: SpatialIndex,
    edge_index: SpatialIndex,
}

impl Scene {
    /// Lays out `tree`. `style` is asked once per node.
    ///
    /// Time O(n log n) (the index build; the layout itself is O(n)),
    /// space O(n).
    #[must_use]
    pub fn from_tree(
        tree: &AttackTree,
        config: LayoutConfig,
        style: impl Fn(NodeIdx) -> NodeStyle,
    ) -> Scene {
        let n = tree.len();
        let mut ids = Vec::with_capacity(n);
        let mut labels = Vec::with_capacity(n);
        let mut kinds = Vec::with_capacity(n);
        let mut styles = Vec::with_capacity(n);
        let mut owner = Vec::with_capacity(n);
        for (idx, _, node) in tree.iter() {
            ids.push(idx);
            labels.push(Arc::from(node.label()));
            kinds.push(node.gate_kind().map_or(NodeKind::Step, NodeKind::Gate));
            styles.push(style(idx));
            owner.push(tree.owner(idx).map(|o| o.index() as u32));
        }

        // Owned children in display order; every other parent link is a
        // dashed edge.
        let mut owned: Vec<Vec<u32>> = vec![Vec::new(); n];
        let mut edges = Vec::new();
        let mut edge_start = Vec::with_capacity(n + 1);
        for p in 0..n {
            edge_start.push(edges.len() as u32);
            for &c in tree.children(ids[p]) {
                let c = c.index() as u32;
                let is_owner = owner[c as usize] == Some(p as u32);
                if is_owner {
                    owned[p].push(c);
                }
                edges.push(Edge {
                    from: p as u32,
                    to: c,
                    link: !is_owner,
                });
            }
        }
        edge_start.push(edges.len() as u32);
        let mut shared_in = vec![0u32; n];
        for e in edges.iter().filter(|e| e.link) {
            shared_in[e.to as usize] += 1;
        }

        let root = tree.root().index() as u32;
        let forest = Forest::from_children(&owned);
        let placed = layout(&forest, root, 1.0);
        let min_breadth = placed.breadth.iter().copied().fold(f64::INFINITY, f64::min);
        let min_breadth = if min_breadth.is_finite() {
            min_breadth
        } else {
            0.0
        };
        let pos: Vec<Point> = (0..n)
            .map(|v| {
                Point::new(
                    placed.depth[v] as f32 * (config.node.w + config.gap_x),
                    ((placed.breadth[v] - min_breadth) as f32) * (config.node.h + config.gap_y),
                )
            })
            .collect();

        let mut scene = Scene {
            config,
            ids,
            labels,
            kinds,
            styles,
            pos,
            owner,
            forest,
            edges,
            edge_start,
            edge_critical: vec![false; 0],
            shared_in,
            root,
            bounds: Rect::default(),
            node_index: SpatialIndex::default(),
            edge_index: SpatialIndex::default(),
        };
        scene.edge_critical = vec![false; scene.edges.len()];
        scene.build_indexes();
        scene
    }

    fn build_indexes(&mut self) {
        let rects: Vec<Rect> = (0..self.len()).map(|v| self.rect_of(v as u32)).collect();
        self.bounds = rects
            .iter()
            .copied()
            .reduce(|a, b| a.union(&b))
            .unwrap_or_default();
        self.node_index =
            SpatialIndex::bulk_load(rects.iter().enumerate().map(|(i, r)| (i as u32, *r)));
        let edge_rects: Vec<(u32, Rect)> = self
            .edges
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let (a, b) = self.edge_ends(*e);
                let x = a.x.min(b.x);
                let y = a.y.min(b.y);
                (
                    i as u32,
                    Rect::new(x, y, (a.x - b.x).abs(), (a.y - b.y).abs()),
                )
            })
            .collect();
        self.edge_index = SpatialIndex::bulk_load(edge_rects);
    }

    fn rect_of(&self, v: u32) -> Rect {
        let p = self.pos[v as usize];
        Rect::new(p.x, p.y, self.config.node.w, self.config.node.h)
    }

    /// Where an edge starts (parent, right middle) and ends (child, left middle).
    #[must_use]
    pub fn edge_ends(&self, e: Edge) -> (Point, Point) {
        let a = self.rect_of(e.from);
        let b = self.rect_of(e.to);
        (
            Point::new(a.right(), a.origin.y + a.size.h / 2.0),
            Point::new(b.origin.x, b.origin.y + b.size.h / 2.0),
        )
    }

    /// Number of nodes. Time O(1).
    #[must_use]
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    /// Whether the scene has no nodes. Time O(1).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// The layout sizes.
    #[must_use]
    pub fn config(&self) -> &LayoutConfig {
        &self.config
    }

    /// The box that holds every node, in world coordinates.
    #[must_use]
    pub fn bounds(&self) -> Rect {
        self.bounds
    }

    /// The root node.
    #[must_use]
    pub fn root(&self) -> Option<NodeIdx> {
        self.ids.get(self.root as usize).copied()
    }

    /// A node's box in world coordinates.
    #[must_use]
    pub fn node_rect(&self, idx: NodeIdx) -> Option<Rect> {
        (idx.index() < self.len()).then(|| self.rect_of(idx.index() as u32))
    }

    /// A node's label.
    #[must_use]
    pub fn label(&self, v: u32) -> &str {
        self.labels.get(v as usize).map_or("", |l| &**l)
    }

    /// A node's kind.
    #[must_use]
    pub fn kind(&self, v: u32) -> NodeKind {
        self.kinds
            .get(v as usize)
            .copied()
            .unwrap_or(NodeKind::Step)
    }

    /// A node's annotation.
    #[must_use]
    pub fn style(&self, v: u32) -> Option<&NodeStyle> {
        self.styles.get(v as usize)
    }

    /// The edges, grouped by parent in node order.
    #[must_use]
    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    /// The edges leaving a node (to its children, owned and shared), in
    /// display order. Time O(1).
    #[must_use]
    pub fn edges_from(&self, v: u32) -> &[Edge] {
        let (Some(a), Some(b)) = (
            self.edge_start.get(v as usize),
            self.edge_start.get(v as usize + 1),
        ) else {
            return &[];
        };
        &self.edges[*a as usize..*b as usize]
    }

    /// The children a node owns, in display order. Time O(1).
    #[must_use]
    pub fn owned_children(&self, v: u32) -> &[u32] {
        if (v as usize) < self.len() {
            self.forest.children(v)
        } else {
            &[]
        }
    }

    /// How many other nodes reach this one through a shared (dashed) edge.
    /// Time O(1).
    #[must_use]
    pub fn shared_in(&self, v: u32) -> u32 {
        self.shared_in.get(v as usize).copied().unwrap_or(0)
    }

    /// The node that owns `v` as a position, if it is not the root.
    /// Time O(1).
    #[must_use]
    pub fn owner_pos(&self, v: u32) -> Option<u32> {
        self.owner.get(v as usize).copied().flatten()
    }

    /// Whether the edge is on the highlighted path.
    #[must_use]
    pub fn is_edge_critical(&self, edge: u32) -> bool {
        self.edge_critical
            .get(edge as usize)
            .copied()
            .unwrap_or(false)
    }

    /// The typed ID of a node by its position. Time O(1).
    #[must_use]
    pub fn node_idx(&self, v: u32) -> Option<NodeIdx> {
        self.ids.get(v as usize).copied()
    }

    /// Replaces a node's annotation. Time O(1); the layout and the indexes
    /// do not change, because the box size is fixed.
    pub fn restyle(&mut self, idx: NodeIdx, style: NodeStyle) {
        if let Some(slot) = self.styles.get_mut(idx.index()) {
            *slot = style;
        }
    }

    /// Highlights a path from the root down: each node's `critical` flag and
    /// the edges between consecutive nodes. An empty slice clears it.
    /// Time O(path length x degree).
    pub fn set_critical_path(&mut self, path: &[NodeIdx]) {
        for s in &mut self.styles {
            s.critical = false;
        }
        self.edge_critical.iter_mut().for_each(|c| *c = false);
        for pair in path.windows(2) {
            let (a, b) = (pair[0].index(), pair[1].index());
            if a >= self.len() || b >= self.len() {
                continue;
            }
            let range = self.edge_start[a] as usize..self.edge_start[a + 1] as usize;
            if let Some(i) = range.into_iter().find(|i| self.edges[*i].to as usize == b) {
                self.edge_critical[i] = true;
            }
        }
        for node in path {
            if let Some(s) = self.styles.get_mut(node.index()) {
                s.critical = true;
            }
        }
    }

    /// The node under a world point, if any. Nodes never overlap, so there
    /// is at most one. Time O(log n).
    #[must_use]
    pub fn hit_test(&self, world: Point) -> Option<NodeIdx> {
        self.node_index
            .at_point(world)
            .first()
            .and_then(|v| self.node_idx(*v))
    }

    /// Node positions whose boxes touch `area`, ascending. Time O(log n + k log k).
    #[must_use]
    pub fn nodes_in(&self, area: &Rect) -> Vec<u32> {
        self.node_index.query(area)
    }

    /// Edge positions whose bounding boxes touch `area`, ascending.
    /// Time O(log n + k log k).
    #[must_use]
    pub fn edges_in(&self, area: &Rect) -> Vec<u32> {
        self.edge_index.query(area)
    }

    /// The owner of a node, if it is not the root.
    #[must_use]
    pub fn owner_of(&self, idx: NodeIdx) -> Option<NodeIdx> {
        self.owner
            .get(idx.index())
            .copied()
            .flatten()
            .and_then(|o| self.node_idx(o))
    }

    /// Moves the selection one step. Time O(degree) for left and right,
    /// O(log n + column height) for up and down.
    #[must_use]
    pub fn navigate(&self, from: NodeIdx, direction: Direction) -> Option<NodeIdx> {
        let v = from.index();
        if v >= self.len() {
            return None;
        }
        let here = self.rect_of(v as u32);
        match direction {
            Direction::Left => self.owner_of(from),
            Direction::Right => {
                let cy = here.center().y;
                self.forest
                    .children(v as u32)
                    .iter()
                    .min_by(|a, b| {
                        let da = (self.rect_of(**a).center().y - cy).abs();
                        let db = (self.rect_of(**b).center().y - cy).abs();
                        da.total_cmp(&db).then(a.cmp(b))
                    })
                    .and_then(|c| self.node_idx(*c))
            }
            Direction::Up | Direction::Down => {
                let up = direction == Direction::Up;
                // The whole column of this depth: same x, any y.
                let column = Rect::new(
                    here.origin.x,
                    self.bounds.origin.y,
                    here.size.w,
                    self.bounds.size.h,
                );
                self.node_index
                    .query(&column)
                    .into_iter()
                    .filter(|c| *c as usize != v)
                    .filter_map(|c| {
                        let dy = self.rect_of(c).origin.y - here.origin.y;
                        ((up && dy < 0.0) || (!up && dy > 0.0)).then_some((dy.abs(), c))
                    })
                    .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
                    .and_then(|(_, c)| self.node_idx(c))
            }
        }
    }
}
