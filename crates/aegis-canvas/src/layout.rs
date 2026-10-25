//! Tree layout in linear time: Buchheim, Jünger and Leipert,
//! "Improving Walker's Algorithm to Run in Linear Time" (2002) (rule A6).
//!
//! Walker's algorithm draws a rooted tree so that
//! 1. a parent is centred over its children,
//! 2. subtrees at the same depth never overlap,
//! 3. identical subtrees are drawn identically, and
//! 4. the drawing is a mirror image if the tree is.
//!
//! Walker's original takes O(n²) in the worst case; the version here keeps
//! "threads" between the contours of subtrees and moves whole subtrees with
//! lazy shifts, which makes it O(n) in time and space.
//!
//! This implementation is **iterative**: both walks use explicit stacks, so
//! a degenerate tree a million levels deep cannot overflow the call stack.
//!
//! The layout works on abstract unit coordinates (siblings one unit apart)
//! and returns, per node, its *depth* and its *breadth* position. [`Scene`]
//! scales them to pixels.
//!
//! [`Scene`]: crate::scene::Scene

/// A rooted ordered tree over nodes `0..n`, in compressed-row form.
///
/// `children(v)` is `kids[start[v]..start[v + 1]]`. Built by
/// [`Forest::from_parents`] in O(n).
#[derive(Debug, Clone)]
pub struct Forest {
    start: Vec<u32>,
    kids: Vec<u32>,
}

impl Forest {
    /// Builds the child lists from a parent array: `parent[v]` is the owner
    /// of `v`, `None` for the root. Children keep ascending node order, so
    /// the caller controls sibling order by numbering.
    ///
    /// Time O(n), space O(n).
    #[must_use]
    pub fn from_parents(parent: &[Option<u32>]) -> Self {
        let n = parent.len();
        let mut start = vec![0u32; n + 1];
        for p in parent.iter().flatten() {
            start[*p as usize + 1] += 1;
        }
        for i in 0..n {
            start[i + 1] += start[i];
        }
        let mut fill = start.clone();
        let mut kids = vec![0u32; start[n] as usize];
        for (v, p) in parent.iter().enumerate() {
            if let Some(p) = p {
                kids[fill[*p as usize] as usize] = v as u32;
                fill[*p as usize] += 1;
            }
        }
        Forest { start, kids }
    }

    /// Builds a forest from explicit, ordered child lists. Time O(n + edges).
    #[must_use]
    pub fn from_children(children: &[Vec<u32>]) -> Self {
        let mut start = Vec::with_capacity(children.len() + 1);
        let mut kids = Vec::new();
        start.push(0);
        for c in children {
            kids.extend_from_slice(c);
            start.push(kids.len() as u32);
        }
        Forest { start, kids }
    }

    /// Number of nodes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.start.len() - 1
    }

    /// Whether the forest has no nodes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The ordered children of `v`.
    #[must_use]
    pub fn children(&self, v: u32) -> &[u32] {
        &self.kids[self.start[v as usize] as usize..self.start[v as usize + 1] as usize]
    }
}

/// Where the layout put each node.
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    /// Distance from the root, in levels.
    pub depth: Vec<u32>,
    /// Position across the tree, in sibling spacings (unit distance), with
    /// the root at 0. May be negative.
    pub breadth: Vec<f64>,
}

const NONE: u32 = u32::MAX;

struct State<'a> {
    forest: &'a Forest,
    parent: Vec<u32>,
    /// Position among siblings, 1-based (Walker's `number`).
    number: Vec<u32>,
    prelim: Vec<f64>,
    modifier: Vec<f64>,
    change: Vec<f64>,
    shift: Vec<f64>,
    ancestor: Vec<u32>,
    thread: Vec<u32>,
}

impl State<'_> {
    fn first_child(&self, v: u32) -> u32 {
        self.forest.children(v).first().copied().unwrap_or(NONE)
    }

    fn last_child(&self, v: u32) -> u32 {
        self.forest.children(v).last().copied().unwrap_or(NONE)
    }

    fn left_sibling(&self, v: u32) -> u32 {
        let n = self.number[v as usize];
        if n <= 1 {
            return NONE;
        }
        let p = self.parent[v as usize];
        self.forest.children(p)[n as usize - 2]
    }

    fn leftmost_sibling(&self, v: u32) -> u32 {
        let p = self.parent[v as usize];
        if p == NONE {
            return v;
        }
        self.forest.children(p)[0]
    }

    fn next_left(&self, v: u32) -> u32 {
        let c = self.first_child(v);
        if c == NONE {
            self.thread[v as usize]
        } else {
            c
        }
    }

    fn next_right(&self, v: u32) -> u32 {
        let c = self.last_child(v);
        if c == NONE {
            self.thread[v as usize]
        } else {
            c
        }
    }

    /// Walker's `MoveSubtree`: moves the subtree rooted at `wr` right by
    /// `shift`, spreading the move over the trees between `wl` and `wr`.
    fn move_subtree(&mut self, wl: u32, wr: u32, shift: f64) {
        let subtrees = f64::from(self.number[wr as usize] - self.number[wl as usize]);
        self.change[wr as usize] -= shift / subtrees;
        self.shift[wr as usize] += shift;
        self.change[wl as usize] += shift / subtrees;
        self.prelim[wr as usize] += shift;
        self.modifier[wr as usize] += shift;
    }

    fn execute_shifts(&mut self, v: u32) {
        let mut shift = 0.0;
        let mut change = 0.0;
        // Right to left over the children.
        for i in (0..self.forest.children(v).len()).rev() {
            let w = self.forest.children(v)[i] as usize;
            self.prelim[w] += shift;
            self.modifier[w] += shift;
            change += self.change[w];
            shift += self.shift[w] + change;
        }
    }

    fn ancestor_of(&self, vil: u32, v: u32, default: u32) -> u32 {
        let a = self.ancestor[vil as usize];
        if self.parent[a as usize] == self.parent[v as usize] {
            a
        } else {
            default
        }
    }

    /// Walker's `Apportion`: pushes the subtree of `v` right until it clears
    /// the contour of everything to its left, using threads to walk the
    /// contours in time proportional to the shorter one.
    fn apportion(&mut self, v: u32, mut default: u32, distance: f64) -> u32 {
        let w = self.left_sibling(v);
        if w == NONE {
            return default;
        }
        let (mut vir, mut vor) = (v, v);
        let (mut vil, mut vol) = (w, self.leftmost_sibling(v));
        let mut sir = self.modifier[vir as usize];
        let mut sor = self.modifier[vor as usize];
        let mut sil = self.modifier[vil as usize];
        let mut sol = self.modifier[vol as usize];
        while self.next_right(vil) != NONE && self.next_left(vir) != NONE {
            vil = self.next_right(vil);
            vir = self.next_left(vir);
            vol = self.next_left(vol);
            vor = self.next_right(vor);
            self.ancestor[vor as usize] = v;
            let shift =
                (self.prelim[vil as usize] + sil) - (self.prelim[vir as usize] + sir) + distance;
            if shift > 0.0 {
                let a = self.ancestor_of(vil, v, default);
                self.move_subtree(a, v, shift);
                sir += shift;
                sor += shift;
            }
            sil += self.modifier[vil as usize];
            sir += self.modifier[vir as usize];
            sol += self.modifier[vol as usize];
            sor += self.modifier[vor as usize];
        }
        if self.next_right(vil) != NONE && self.next_right(vor) == NONE {
            self.thread[vor as usize] = self.next_right(vil);
            self.modifier[vor as usize] += sil - sor;
        }
        if self.next_left(vir) != NONE && self.next_left(vol) == NONE {
            self.thread[vol as usize] = self.next_left(vir);
            self.modifier[vol as usize] += sir - sol;
            default = v;
        }
        default
    }
}

/// One frame of the iterative first walk.
struct Frame {
    v: u32,
    /// Index of the next child to descend into.
    next: usize,
    /// The child descended into last, whose `apportion` is still to do.
    pending: u32,
    default_ancestor: u32,
}

/// Lays out the tree under `root` with siblings `distance` apart.
///
/// Nodes not reachable from `root` get depth 0 and breadth 0; they are not
/// part of the tree. Time O(n), space O(n), no recursion.
#[must_use]
pub fn layout(forest: &Forest, root: u32, distance: f64) -> Layout {
    let n = forest.len();
    let mut depth = vec![0u32; n];
    let mut breadth = vec![0.0f64; n];
    if n == 0 || root as usize >= n {
        return Layout { depth, breadth };
    }

    let mut st = State {
        forest,
        parent: vec![NONE; n],
        number: vec![0; n],
        prelim: vec![0.0; n],
        modifier: vec![0.0; n],
        change: vec![0.0; n],
        shift: vec![0.0; n],
        ancestor: (0..n as u32).collect(),
        thread: vec![NONE; n],
    };
    // Parents, sibling numbers, and depths in one pass from the root.
    let mut stack = vec![root];
    while let Some(v) = stack.pop() {
        for (i, &c) in forest.children(v).iter().enumerate() {
            st.parent[c as usize] = v;
            st.number[c as usize] = i as u32 + 1;
            depth[c as usize] = depth[v as usize] + 1;
            stack.push(c);
        }
    }
    st.number[root as usize] = 1;

    // First walk: post-order, computing preliminary positions.
    let first = st.first_child(root);
    let mut frames = vec![Frame {
        v: root,
        next: 0,
        pending: NONE,
        default_ancestor: first,
    }];
    while let Some(top) = frames.last_mut() {
        let v = top.v;
        if top.pending != NONE {
            let w = top.pending;
            top.pending = NONE;
            let da = top.default_ancestor;
            let da = st.apportion(w, da, distance);
            if let Some(top) = frames.last_mut() {
                top.default_ancestor = da;
            }
            continue;
        }
        let kids = forest.children(v);
        if top.next < kids.len() {
            let w = kids[top.next];
            top.next += 1;
            top.pending = w;
            let da = st.first_child(w);
            frames.push(Frame {
                v: w,
                next: 0,
                pending: NONE,
                default_ancestor: da,
            });
            continue;
        }
        // All children done: finish v.
        frames.pop();
        let left = st.left_sibling(v);
        if kids.is_empty() {
            st.prelim[v as usize] = if left == NONE {
                0.0
            } else {
                st.prelim[left as usize] + distance
            };
        } else {
            st.execute_shifts(v);
            let mid = (st.prelim[st.first_child(v) as usize]
                + st.prelim[st.last_child(v) as usize])
                / 2.0;
            if left == NONE {
                st.prelim[v as usize] = mid;
            } else {
                st.prelim[v as usize] = st.prelim[left as usize] + distance;
                st.modifier[v as usize] = st.prelim[v as usize] - mid;
            }
        }
    }

    // Second walk: final positions, with the root at 0.
    let root_shift = st.prelim[root as usize];
    let mut stack = vec![(root, 0.0f64)];
    while let Some((v, m)) = stack.pop() {
        breadth[v as usize] = st.prelim[v as usize] + m - root_shift;
        let below = m + st.modifier[v as usize];
        for &c in forest.children(v) {
            stack.push((c, below));
        }
    }
    Layout { depth, breadth }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn forest(parents: &[Option<u32>]) -> Forest {
        Forest::from_parents(parents)
    }

    #[test]
    fn a_single_node_sits_at_the_origin() {
        let l = layout(&forest(&[None]), 0, 1.0);
        assert_eq!(l.depth, vec![0]);
        assert_eq!(l.breadth, vec![0.0]);
    }

    #[test]
    fn children_are_one_unit_apart_and_the_parent_is_centred() {
        // 0 -> 1, 2, 3
        let l = layout(&forest(&[None, Some(0), Some(0), Some(0)]), 0, 1.0);
        assert_eq!(l.depth, vec![0, 1, 1, 1]);
        assert_eq!(l.breadth, vec![0.0, -1.0, 0.0, 1.0]);
    }

    #[test]
    fn a_chain_stays_on_one_line() {
        let parents: Vec<Option<u32>> = std::iter::once(None).chain((0..9).map(Some)).collect();
        let l = layout(&forest(&parents), 0, 1.0);
        assert!(l.breadth.iter().all(|b| *b == 0.0));
        assert_eq!(l.depth, (0..10).collect::<Vec<u32>>());
    }

    #[test]
    fn identical_subtrees_are_drawn_identically() {
        //        0
        //     1     2
        //    3 4   5 6     (1 and 2 have the same shape)
        let parents = [None, Some(0), Some(0), Some(1), Some(1), Some(2), Some(2)];
        let l = layout(&forest(&parents), 0, 1.0);
        let rel = |c: usize, p: usize| l.breadth[c] - l.breadth[p];
        assert_eq!(rel(3, 1), rel(5, 2));
        assert_eq!(rel(4, 1), rel(6, 2));
        assert_eq!(l.breadth[3] + l.breadth[4], 2.0 * l.breadth[1]);
    }

    #[test]
    fn a_mirrored_tree_gives_a_mirrored_drawing() {
        // A lopsided tree and its mirror image (children reversed).
        let left =
            Forest::from_children(&[vec![1, 2], vec![3, 4, 5], vec![], vec![], vec![], vec![]]);
        let right =
            Forest::from_children(&[vec![2, 1], vec![5, 4, 3], vec![], vec![], vec![], vec![]]);
        let a = layout(&left, 0, 1.0);
        let b = layout(&right, 0, 1.0);
        for v in 0..6 {
            assert!((a.breadth[v] + b.breadth[v]).abs() < 1e-9, "node {v}");
        }
    }

    #[test]
    fn subtrees_at_one_depth_never_overlap() {
        // A deep left subtree next to a shallow wide right one.
        //   0 -> 1, 2 ; 1 -> 3 ; 3 -> 4, 5 ; 2 -> 6, 7, 8
        let parents = [
            None,
            Some(0),
            Some(0),
            Some(1),
            Some(3),
            Some(3),
            Some(2),
            Some(2),
            Some(2),
        ];
        let l = layout(&forest(&parents), 0, 1.0);
        for a in 0..9 {
            for b in (a + 1)..9 {
                if l.depth[a] == l.depth[b] {
                    assert!(
                        (l.breadth[a] - l.breadth[b]).abs() >= 1.0 - 1e-9,
                        "{a} vs {b}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_million_levels_do_not_overflow_the_stack() {
        let n = 1_000_000usize;
        let mut parents: Vec<Option<u32>> = vec![None];
        parents.extend((0..n - 1).map(|i| Some(i as u32)));
        let l = layout(&forest(&parents), 0, 1.0);
        assert_eq!(l.depth[n - 1], (n - 1) as u32);
        assert_eq!(l.breadth[n - 1], 0.0);
    }

    #[test]
    fn empty_and_out_of_range_roots_are_harmless() {
        let l = layout(&Forest::from_parents(&[]), 0, 1.0);
        assert!(l.depth.is_empty());
        let l = layout(&forest(&[None]), 5, 1.0);
        assert_eq!(l.depth, vec![0]);
    }

    #[test]
    fn forest_builders_agree() {
        let parents = [None, Some(0), Some(0), Some(1)];
        let a = Forest::from_parents(&parents);
        let b = Forest::from_children(&[vec![1, 2], vec![3], vec![], vec![]]);
        for v in 0..4 {
            assert_eq!(a.children(v), b.children(v));
        }
        assert_eq!(a.len(), 4);
        assert!(!a.is_empty());
    }
}
