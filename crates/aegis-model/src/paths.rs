//! Lazy best-first enumeration of the top-$k$ easiest attack paths in an
//! [`AttackTree`] from a bottom-up [`Evaluation`] (rule A4).
//!
//! An [`AttackPath`] is a minimal leaf-step realization (minimal cut set) of
//! the tree's root goal: at every reached [`GateKind::Or`] gate it chooses one
//! child branch, at every reached [`GateKind::And`] or [`GateKind::SeqAnd`]
//! gate it includes all child branches, and no proper subset of its steps
//! suffices to satisfy the root goal. Its [`AttackPath::steps`] slice lists
//! the distinct leaf steps ([`Node::Step`]) visited in left-to-right
//! pre-order, its [`AttackPath::feasibility`] is the minimum value across
//! those steps ($\min_{s \in \text{steps}} v(s)$), and its
//! [`AttackPath::bottleneck`] is the first step in `steps` that attains
//! `feasibility`.
//!
//! Because an AND/OR tree of depth $d$ can have $2^{\Omega(d)}$ distinct
//! attack paths, [`AttackPaths`] enumerates paths lazily using best-first
//! search over partial realizations guided by the already-computed
//! [`Evaluation`] node values. Under [`MaxMin`](crate::MaxMin), each node's
//! evaluated value is the exact feasibility of the easiest completion of its
//! subgraph, so the first path (`k = 1`) on a tree is found in
//! $O(V(V + E))$ time by expanding only the deciding spine without
//! materializing alternative branches.

use std::cmp::Ordering;
use std::collections::{BTreeSet, BinaryHeap};

use crate::eval::{Derivation, Evaluation};
use crate::id::NodeIdx;
use crate::tree::{AttackTree, GateKind, Node};

/// One complete, minimal attack path (minimal cut set of leaf steps realizing
/// the root goal) produced by [`Evaluation::attack_paths`] or
/// [`Evaluation::top_k_paths`].
///
/// Holds only [`NodeIdx`] indices and the path's bottleneck value `V`, never
/// user-supplied labels or names (rule R6).
///
/// ```
/// use aegis_model::{evaluate, AttackTree, GateKind, MaxMin, Node, NodeId};
///
/// # fn main() -> Result<(), aegis_model::ModelError> {
/// let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::And, "root"))?;
/// let a = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("a"))?;
/// let b = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("b"))?;
/// let eval = evaluate(&tree, &MaxMin::<u8>::new(), |i| Some(if i == a { 4 } else { 2 }))?;
/// let paths = eval.top_k_paths(&tree, 5).ok_or(aegis_model::ModelError::UnknownNode(a))?;
/// assert_eq!(paths.len(), 1);
/// assert_eq!(paths[0].steps(), &[a, b]);
/// assert_eq!(*paths[0].feasibility(), 2);
/// assert_eq!(paths[0].bottleneck(), b);
/// assert_eq!(paths[0].len(), 2);
/// assert!(!paths[0].is_empty());
/// assert!(paths[0].contains(a));
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttackPath<V> {
    steps: Box<[NodeIdx]>,
    bottleneck: NodeIdx,
    feasibility: V,
}

impl<V> AttackPath<V> {
    /// Returns the distinct leaf step indices that form this attack path, in
    /// left-to-right pre-order traversal. Always non-empty.
    ///
    /// Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::{evaluate, AttackTree, GateKind, MaxMin, Node, NodeId};
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::SeqAnd, "root"))?;
    /// let s1 = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("first"))?;
    /// let s2 = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("second"))?;
    /// let eval = evaluate(&tree, &MaxMin::<u8>::new(), |_| Some(3))?;
    /// let path = eval.attack_paths(&tree).and_then(|mut it| it.next()).ok_or(aegis_model::ModelError::UnknownNode(s1))?;
    /// assert_eq!(path.steps(), &[s1, s2]);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn steps(&self) -> &[NodeIdx] {
        &self.steps
    }

    /// Returns the bottleneck leaf step of this path: the first step in
    /// [`steps`](Self::steps) whose value equals
    /// [`feasibility`](Self::feasibility).
    ///
    /// Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::{evaluate, AttackTree, GateKind, MaxMin, Node, NodeId};
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::And, "root"))?;
    /// let easy = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("easy"))?;
    /// let hard = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("hard"))?;
    /// let eval = evaluate(&tree, &MaxMin::<u8>::new(), |i| Some(if i == easy { 5 } else { 1 }))?;
    /// let path = eval.attack_paths(&tree).and_then(|mut it| it.next()).ok_or(aegis_model::ModelError::UnknownNode(easy))?;
    /// assert_eq!(path.bottleneck(), hard);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn bottleneck(&self) -> NodeIdx {
        self.bottleneck
    }

    /// Returns the overall feasibility of this attack path (the minimum value
    /// across its [`steps`](Self::steps)).
    ///
    /// Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::{evaluate, AttackTree, MaxMin, Node, NodeId};
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let tree = AttackTree::new(NodeId::new_v7(), Node::step("only"))?;
    /// let eval = evaluate(&tree, &MaxMin::<u8>::new(), |_| Some(4))?;
    /// let path = eval.attack_paths(&tree).and_then(|mut it| it.next()).ok_or(aegis_model::ModelError::UnknownNode(tree.root()))?;
    /// assert_eq!(*path.feasibility(), 4);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn feasibility(&self) -> &V {
        &self.feasibility
    }

    /// Returns the number of leaf steps in this attack path (always `>= 1`).
    ///
    /// Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::{evaluate, AttackTree, MaxMin, Node, NodeId};
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let tree = AttackTree::new(NodeId::new_v7(), Node::step("only"))?;
    /// let eval = evaluate(&tree, &MaxMin::<u8>::new(), |_| Some(4))?;
    /// let path = eval.attack_paths(&tree).and_then(|mut it| it.next()).ok_or(aegis_model::ModelError::UnknownNode(tree.root()))?;
    /// assert_eq!(path.len(), 1);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// Returns `true` if the path has no steps. Every valid attack path has
    /// at least one leaf step, so this is always `false`; provided to pair
    /// with [`len`](Self::len).
    ///
    /// Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::{evaluate, AttackTree, MaxMin, Node, NodeId};
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let tree = AttackTree::new(NodeId::new_v7(), Node::step("only"))?;
    /// let eval = evaluate(&tree, &MaxMin::<u8>::new(), |_| Some(4))?;
    /// let path = eval.attack_paths(&tree).and_then(|mut it| it.next()).ok_or(aegis_model::ModelError::UnknownNode(tree.root()))?;
    /// assert!(!path.is_empty());
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Returns `true` if `step` is one of the leaf steps in this path.
    ///
    /// Time O(S) where S is [`len`](Self::len); space O(1).
    ///
    /// ```
    /// use aegis_model::{evaluate, AttackTree, GateKind, MaxMin, Node, NodeId};
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root"))?;
    /// let a = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("a"))?;
    /// let b = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("b"))?;
    /// let eval = evaluate(&tree, &MaxMin::<u8>::new(), |i| Some(if i == a { 5 } else { 1 }))?;
    /// let path = eval.attack_paths(&tree).and_then(|mut it| it.next()).ok_or(aegis_model::ModelError::UnknownNode(a))?;
    /// assert!(path.contains(a));
    /// assert!(!path.contains(b));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn contains(&self, step: NodeIdx) -> bool {
        self.steps.contains(&step)
    }
}

/// One leaf step's coverage across the top-$k$ easiest attack paths of a
/// tree, produced by [`Evaluation::choke_points`](crate::Evaluation::choke_points).
///
/// A step that appears on many of the easiest paths is a **choke point**:
/// a control attached there (via
/// [`Op::SetStepMitigation`](crate::Op::SetStepMitigation)) removes more of
/// the attacker's best options than the same control anywhere else. Holds
/// only [`NodeIdx`] indices and path values, never user-supplied labels or
/// names (rule R6).
///
/// ```
/// use aegis_model::{evaluate, AttackTree, GateKind, MaxMin, Node, NodeId};
///
/// # fn main() -> Result<(), aegis_model::ModelError> {
/// // root = AND(x, OR(y, z)): `x` lies on both of the two easiest paths.
/// let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::And, "root"))?;
/// let x = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("x"))?;
/// let or = tree.add_child(tree.root(), NodeId::new_v7(), Node::gate(GateKind::Or, "or"))?;
/// let y = tree.add_child(or, NodeId::new_v7(), Node::step("y"))?;
/// let z = tree.add_child(or, NodeId::new_v7(), Node::step("z"))?;
/// let eval = evaluate(&tree, &MaxMin::<u8>::new(), |i| {
///     Some(if i == y { 2 } else if i == z { 3 } else { 4 })
/// })?;
/// let choke = eval.choke_points(&tree, 2).ok_or(aegis_model::ModelError::UnknownNode(x))?;
/// assert_eq!(choke[0].step(), x);
/// assert_eq!(choke[0].path_count(), 2);
/// assert_eq!(choke[0].bottleneck_count(), 0);
/// assert_eq!(*choke[0].easiest_path_feasibility(), 3);
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChokePoint<V> {
    pub(crate) step: NodeIdx,
    pub(crate) path_count: usize,
    pub(crate) bottleneck_count: usize,
    pub(crate) easiest_path_feasibility: V,
}

impl<V> ChokePoint<V> {
    /// Returns the covered leaf step. Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::{evaluate, AttackTree, MaxMin, Node, NodeId};
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let tree = AttackTree::new(NodeId::new_v7(), Node::step("only"))?;
    /// let eval = evaluate(&tree, &MaxMin::<u8>::new(), |_| Some(4))?;
    /// let choke = eval.choke_points(&tree, 1).ok_or(aegis_model::ModelError::UnknownNode(tree.root()))?;
    /// assert_eq!(choke[0].step(), tree.root());
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn step(&self) -> NodeIdx {
        self.step
    }

    /// Returns how many of the top-$k$ paths contain
    /// [`step`](Self::step) (always `>= 1`). Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::{evaluate, AttackTree, MaxMin, Node, NodeId};
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let tree = AttackTree::new(NodeId::new_v7(), Node::step("only"))?;
    /// let eval = evaluate(&tree, &MaxMin::<u8>::new(), |_| Some(4))?;
    /// let choke = eval.choke_points(&tree, 1).ok_or(aegis_model::ModelError::UnknownNode(tree.root()))?;
    /// assert_eq!(choke[0].path_count(), 1);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn path_count(&self) -> usize {
        self.path_count
    }

    /// Returns how many of the top-$k$ paths have
    /// [`step`](Self::step) as their bottleneck (the step whose value equals
    /// the path's feasibility). Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::{evaluate, AttackTree, MaxMin, Node, NodeId};
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let tree = AttackTree::new(NodeId::new_v7(), Node::step("only"))?;
    /// let eval = evaluate(&tree, &MaxMin::<u8>::new(), |_| Some(4))?;
    /// let choke = eval.choke_points(&tree, 1).ok_or(aegis_model::ModelError::UnknownNode(tree.root()))?;
    /// assert_eq!(choke[0].bottleneck_count(), 1);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn bottleneck_count(&self) -> usize {
        self.bottleneck_count
    }

    /// Returns the feasibility of the easiest (first-enumerated) top-$k$ path
    /// that contains [`step`](Self::step), the most the attacker keeps while
    /// `step` stays at its current value. Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::{evaluate, AttackTree, MaxMin, Node, NodeId};
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let tree = AttackTree::new(NodeId::new_v7(), Node::step("only"))?;
    /// let eval = evaluate(&tree, &MaxMin::<u8>::new(), |_| Some(4))?;
    /// let choke = eval.choke_points(&tree, 1).ok_or(aegis_model::ModelError::UnknownNode(tree.root()))?;
    /// assert_eq!(*choke[0].easiest_path_feasibility(), 4);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn easiest_path_feasibility(&self) -> &V {
        &self.easiest_path_feasibility
    }
}

/// Partial or complete attack path state in the best-first search heap.
#[derive(Clone, Debug)]
struct SearchState<V> {
    /// Upper bound on any completion of this state; equals the exact path
    /// feasibility when `next_or` is `None`.
    bound: V,
    /// Ranks (`0..child_count`) chosen at each expanded `Or` gate so far,
    /// where rank `0` is the most feasible child (earliest on ties).
    choices: Vec<u32>,
    /// Leaf steps collected so far in left-to-right pre-order.
    steps: Vec<NodeIdx>,
    /// The next unexpanded `Or` gate in pre-order, or `None` if this state is
    /// a complete attack path.
    next_or: Option<NodeIdx>,
    /// Remaining unexpanded nodes to visit after `next_or`, stored in reverse
    /// pre-order so `pop` visits the next node in O(1).
    worklist: Vec<NodeIdx>,
    /// Nodes already expanded in this realization (length == `tree.len()`),
    /// ensuring shared DAG nodes are expanded at most once per path.
    expanded: Vec<bool>,
}

impl<V: Ord> PartialEq for SearchState<V> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl<V: Ord> Eq for SearchState<V> {}

impl<V: Ord> PartialOrd for SearchState<V> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<V: Ord> Ord for SearchState<V> {
    fn cmp(&self, other: &Self) -> Ordering {
        // 1. Higher bound (easiest for the attacker) comes first.
        // 2. Lexicographically smaller `choices` vector comes first, so
        //    rank-0 (`why_path`) choices are expanded depth-first before
        //    rank-1 alternatives of the same bound.
        self.bound
            .cmp(&other.bound)
            .then_with(|| other.choices.cmp(&self.choices))
    }
}

impl<V: Ord + Clone> SearchState<V> {
    /// Expands deterministic nodes (`Step`, `And`, `SeqAnd`) from `worklist`
    /// until the first unexpanded `Or` gate or until `worklist` is empty, and
    /// recomputes `bound`. Returns `None` if a node lookup fails (cannot
    /// happen on a validated tree/evaluation pair).
    fn normalize(&mut self, tree: &AttackTree, eval: &Evaluation<V>) -> Option<()> {
        while let Some(idx) = self.worklist.pop() {
            let slot = self.expanded.get_mut(idx.index())?;
            if *slot {
                continue;
            }
            *slot = true;
            match tree.get(idx)? {
                Node::Step { .. } => {
                    self.steps.push(idx);
                }
                Node::Gate {
                    kind: GateKind::And | GateKind::SeqAnd,
                    children,
                    ..
                } => {
                    for &child in children.iter().rev() {
                        if !self.expanded.get(child.index()).copied().unwrap_or(true) {
                            self.worklist.push(child);
                        }
                    }
                }
                Node::Gate {
                    kind: GateKind::Or, ..
                } => {
                    self.next_or = Some(idx);
                    break;
                }
            }
        }
        self.bound = self.compute_bound(eval)?;
        Some(())
    }

    /// Computes the exact feasibility bound across `steps`, `next_or`, and
    /// unexpanded nodes in `worklist`.
    fn compute_bound(&self, eval: &Evaluation<V>) -> Option<V> {
        let mut min_val: Option<&V> = None;
        for &s in &self.steps {
            let v = eval.get(s)?;
            min_val = Some(match min_val {
                None => v,
                Some(cur) if v < cur => v,
                Some(cur) => cur,
            });
        }
        if let Some(or_idx) = self.next_or {
            let v = eval.get(or_idx)?;
            min_val = Some(match min_val {
                None => v,
                Some(cur) if v < cur => v,
                Some(cur) => cur,
            });
        }
        for &w in &self.worklist {
            if !self.expanded.get(w.index()).copied().unwrap_or(true) {
                let v = eval.get(w)?;
                min_val = Some(match min_val {
                    None => v,
                    Some(cur) if v < cur => v,
                    Some(cur) => cur,
                });
            }
        }
        min_val.cloned()
    }
}

/// Lazy iterator yielding [`AttackPath`]s in descending order of feasibility
/// (easiest attack paths first), created by [`Evaluation::attack_paths`].
///
/// See the [module documentation](self) and [`Evaluation::attack_paths`] for
/// complexity guarantees.
///
/// ```
/// use aegis_model::{evaluate, AttackTree, GateKind, MaxMin, Node, NodeId};
///
/// # fn main() -> Result<(), aegis_model::ModelError> {
/// let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root"))?;
/// let s1 = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("s1"))?;
/// let s2 = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("s2"))?;
/// let eval = evaluate(&tree, &MaxMin::<u8>::new(), |i| Some(if i == s1 { 2 } else { 5 }))?;
/// let mut iter = eval.attack_paths(&tree).ok_or(aegis_model::ModelError::UnknownNode(s1))?;
/// let first = iter.next().ok_or(aegis_model::ModelError::UnknownNode(s2))?;
/// assert_eq!(first.steps(), &[s2]);
/// assert_eq!(*first.feasibility(), 5);
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct AttackPaths<'a, V: Ord> {
    tree: &'a AttackTree,
    eval: &'a Evaluation<V>,
    topo: Vec<NodeIdx>,
    has_shared: bool,
    heap: BinaryHeap<SearchState<V>>,
    yielded: BTreeSet<Box<[NodeIdx]>>,
}

impl<'a, V: Ord + Clone> AttackPaths<'a, V> {
    pub(crate) fn new(tree: &'a AttackTree, eval: &'a Evaluation<V>) -> Option<Self> {
        if !matches_tree(tree, eval) {
            return None;
        }
        let topo = tree.topological_order().ok()?;
        let has_shared = tree
            .iter()
            .any(|(idx, _, _)| tree.parents(idx).nth(1).is_some());
        let mut init = SearchState {
            bound: eval.root_value().clone(),
            choices: Vec::new(),
            steps: Vec::new(),
            next_or: None,
            worklist: vec![tree.root()],
            expanded: vec![false; tree.len()],
        };
        init.normalize(tree, eval)?;
        let mut heap = BinaryHeap::new();
        heap.push(init);
        Some(Self {
            tree,
            eval,
            topo,
            has_shared,
            heap,
            yielded: BTreeSet::new(),
        })
    }
}

impl<V: Ord + Clone> Iterator for AttackPaths<'_, V> {
    type Item = AttackPath<V>;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(mut state) = self.heap.pop() {
            if let Some(or_idx) = state.next_or.take() {
                let children = self.tree.children(or_idx);
                // Order children by descending evaluated value, breaking ties
                // by stored child position so rank 0 matches `MaxMin`.
                let mut ranked: Vec<(usize, NodeIdx, &V)> = children
                    .iter()
                    .enumerate()
                    .filter_map(|(pos, &c)| Some((pos, c, self.eval.get(c)?)))
                    .collect();
                ranked.sort_by(|a, b| b.2.cmp(a.2).then_with(|| a.0.cmp(&b.0)));

                for (rank, (_pos, child_idx, _val)) in ranked.into_iter().enumerate() {
                    let mut next_state = state.clone();
                    let rank_u32 = u32::try_from(rank).unwrap_or(u32::MAX);
                    next_state.choices.push(rank_u32);
                    next_state.worklist.push(child_idx);
                    if next_state.normalize(self.tree, self.eval).is_some() {
                        self.heap.push(next_state);
                    }
                }
            } else {
                if self.has_shared
                    && state.steps.len() > 1
                    && !is_minimal_cut_set(self.tree, &self.topo, &state.steps)
                {
                    continue;
                }
                let mut canonical_key = state.steps.clone();
                canonical_key.sort_unstable();
                if !self.yielded.insert(canonical_key.into_boxed_slice()) {
                    continue;
                }
                let steps: Box<[NodeIdx]> = state.steps.into_boxed_slice();
                let mut bottleneck = *steps.first()?;
                let mut min_val = self.eval.get(bottleneck)?;
                for &s in steps.iter().skip(1) {
                    let v = self.eval.get(s)?;
                    if v < min_val {
                        min_val = v;
                        bottleneck = s;
                    }
                }
                return Some(AttackPath {
                    steps,
                    bottleneck,
                    feasibility: min_val.clone(),
                });
            }
        }
        None
    }
}

/// Checks whether `steps` is a minimal cut set of `tree`: removing any single
/// step from `steps` must leave `tree.root()` unsatisfied. Time
/// O(|steps| * (V + E)), space O(V).
fn is_minimal_cut_set(tree: &AttackTree, topo: &[NodeIdx], steps: &[NodeIdx]) -> bool {
    let mut active = vec![false; tree.len()];
    for &s in steps {
        if let Some(slot) = active.get_mut(s.index()) {
            *slot = true;
        }
    }
    let mut sat = vec![false; tree.len()];
    for &s in steps {
        if let Some(slot) = active.get_mut(s.index()) {
            *slot = false;
        }
        if satisfies_root(tree, topo, &active, &mut sat) {
            return false;
        }
        if let Some(slot) = active.get_mut(s.index()) {
            *slot = true;
        }
    }
    true
}

/// Returns `true` if the leaf steps marked `true` in `active` satisfy
/// `tree.root()`. Time O(V + E), space O(1) using the caller's `sat` buffer.
fn satisfies_root(tree: &AttackTree, topo: &[NodeIdx], active: &[bool], sat: &mut [bool]) -> bool {
    for &idx in topo.iter().rev() {
        let ok = match tree.get(idx) {
            Some(Node::Step { .. }) => active.get(idx.index()).copied().unwrap_or(false),
            Some(Node::Gate {
                kind: GateKind::Or,
                children,
                ..
            }) => children
                .iter()
                .any(|&c| sat.get(c.index()).copied().unwrap_or(false)),
            Some(Node::Gate {
                kind: GateKind::And | GateKind::SeqAnd,
                children,
                ..
            }) => {
                !children.is_empty()
                    && children
                        .iter()
                        .all(|&c| sat.get(c.index()).copied().unwrap_or(false))
            }
            None => false,
        };
        if let Some(slot) = sat.get_mut(idx.index()) {
            *slot = ok;
        }
    }
    sat.get(tree.root().index()).copied().unwrap_or(false)
}

/// Checks that `tree` structurally matches `eval`: identical node count, the
/// root has a value, every step has `Derivation::Leaf`, and every gate has
/// non-empty children and a matching gate rule in `eval`. Other nodes may have
/// no value (an excluded node, APL-02); such nodes are never on a path, because
/// OR skips children without a value and every child of a valued AND/SAND gate
/// has a value. Time O(V).
fn matches_tree<V>(tree: &AttackTree, eval: &Evaluation<V>) -> bool {
    if tree.len() != eval.len() || eval.get(tree.root()).is_none() {
        return false;
    }
    for (idx, _, node) in tree.iter() {
        let Some(derivation) = eval.derivation(idx) else {
            return false;
        };
        match (node, derivation) {
            (Node::Step { .. }, Derivation::Leaf) => {}
            (
                Node::Gate { kind, children, .. },
                Derivation::Selected { rule, .. } | Derivation::Combined { rule },
            ) if *kind == rule && !children.is_empty() => {}
            _ => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::{MaxMin, evaluate};
    use crate::testkit::node_id;

    #[test]
    fn exponential_or_tree_yields_top_k_without_materializing_all() {
        // Depth-25 binary OR chain: 2^25 = 33,554,432 paths if materialized,
        // yet built with 51 nodes by sharing each level's sub-gate across two
        // parent OR branches, or a 31-node tree with 16 leaves. Even better:
        // build a 20-stage OR-of-AND-of-OR diamond DAG with 2^20 = 1,048,576
        // paths, and verify top_k_paths(..., 3) finishes instantaneously.
        let mut tree = AttackTree::new(node_id(1), Node::gate(GateKind::And, "root")).expect("ok");
        let root = tree.root();
        let mut next_id = 2_u64;
        for stage in 0..10_u8 {
            let or_gate = tree
                .add_child(root, node_id(next_id), Node::gate(GateKind::Or, "choice"))
                .expect("or");
            next_id += 1;
            let s_hi = tree
                .add_child(or_gate, node_id(next_id), Node::step("hi"))
                .expect("s_hi");
            next_id += 1;
            let s_lo = tree
                .add_child(or_gate, node_id(next_id), Node::step("lo"))
                .expect("s_lo");
            next_id += 1;
            let _ = (stage, s_hi, s_lo);
        }
        // 10 independent binary OR gates under an AND root => 2^10 = 1024 paths.
        let eval = evaluate(&tree, &MaxMin::<u8>::new(), |idx| {
            // Odd step positions are high (10), even are lower (1..=10).
            Some(if idx.index() % 3 == 2 { 20 } else { 5 })
        })
        .expect("eval");

        let top3 = eval.top_k_paths(&tree, 3).expect("valid tree");
        assert_eq!(top3.len(), 3);
        assert_eq!(*top3[0].feasibility(), 20);
        assert_eq!(top3[0].len(), 10);
        assert_eq!(*top3[1].feasibility(), 5);
        assert_eq!(*top3[2].feasibility(), 5);
        assert_ne!(top3[1].steps(), top3[2].steps());
    }

    #[test]
    fn shared_dag_filters_supersets_and_deduplicates() {
        // root = OR(AND(s1, s2), s1) where s1=3, s2=5:
        // Both branches have feasibility 3, but {s1, s2} is a strict superset
        // of {s1}, so only the minimal cut set [s1] is yielded.
        let mut tree = AttackTree::new(node_id(1), Node::gate(GateKind::Or, "root")).expect("ok");
        let root = tree.root();
        let and_gate = tree
            .add_child(root, node_id(2), Node::gate(GateKind::And, "and"))
            .expect("and");
        let s1 = tree
            .add_child(and_gate, node_id(3), Node::step("s1"))
            .expect("s1");
        let s2 = tree
            .add_child(and_gate, node_id(4), Node::step("s2"))
            .expect("s2");
        tree.link(root, s1).expect("link s1 to root");

        let eval = evaluate(&tree, &MaxMin::<u8>::new(), |i| {
            Some(if i == s1 {
                3
            } else if i == s2 {
                5
            } else {
                0
            })
        })
        .expect("eval");
        let paths: Vec<_> = eval.attack_paths(&tree).expect("valid").collect();
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].steps(), &[s1]);
        assert_eq!(*paths[0].feasibility(), 3);
        assert_eq!(paths[0].bottleneck(), s1);
    }

    #[test]
    fn mismatched_tree_returns_none() {
        let mut t1 = AttackTree::new(node_id(1), Node::gate(GateKind::Or, "root")).expect("ok");
        t1.add_child(t1.root(), node_id(2), Node::step("s1"))
            .expect("s1");
        let eval = evaluate(&t1, &MaxMin::<u8>::new(), |_| Some(1)).expect("eval");

        let t2 = AttackTree::new(node_id(1), Node::step("only")).expect("ok");
        assert!(eval.attack_paths(&t2).is_none());
        assert!(eval.top_k_paths(&t2, 1).is_none());

        let mut t3 = AttackTree::new(node_id(1), Node::gate(GateKind::And, "root")).expect("ok");
        t3.add_child(t3.root(), node_id(2), Node::step("s1"))
            .expect("s1");
        assert!(eval.attack_paths(&t3).is_none());
        assert!(eval.choke_points(&t2, 3).is_none());
        assert!(eval.choke_points(&t3, 3).is_none());
        assert!(eval.top_k_hitting_set(&t2, 3).is_none());
        assert!(eval.top_k_hitting_set(&t3, 3).is_none());
    }

    #[test]
    fn choke_points_and_hitting_set_rank_shared_steps_and_cover_paths() {
        // root = OR(AND(s_shared, s_a), AND(s_shared, s_b), s_solo)
        // Values: s_shared = 3, s_a = 4, s_b = 2, s_solo = 1
        // Minimal paths:
        //   1. [s_shared, s_a] -> feasibility 3, bottleneck s_shared
        //   2. [s_shared, s_b] -> feasibility 2, bottleneck s_b
        //   3. [s_solo]        -> feasibility 1, bottleneck s_solo
        let mut tree = AttackTree::new(node_id(1), Node::gate(GateKind::Or, "root")).expect("ok");
        let root = tree.root();
        let g1 = tree
            .add_child(root, node_id(2), Node::gate(GateKind::And, "g1"))
            .expect("g1");
        let g2 = tree
            .add_child(root, node_id(3), Node::gate(GateKind::And, "g2"))
            .expect("g2");
        let s_shared = tree
            .add_child(g1, node_id(4), Node::step("shared"))
            .expect("shared");
        let s_a = tree.add_child(g1, node_id(5), Node::step("a")).expect("a");
        tree.link(g2, s_shared).expect("link shared");
        let s_b = tree.add_child(g2, node_id(6), Node::step("b")).expect("b");
        let s_solo = tree
            .add_child(root, node_id(7), Node::step("solo"))
            .expect("solo");

        let eval = evaluate(&tree, &MaxMin::<u8>::new(), |i| {
            Some(if i == s_shared {
                3
            } else if i == s_a {
                4
            } else if i == s_b {
                2
            } else {
                u8::from(i == s_solo)
            })
        })
        .expect("eval");

        assert_eq!(eval.choke_points(&tree, 0), Some(vec![]));
        assert_eq!(eval.top_k_hitting_set(&tree, 0), Some(vec![]));

        let cp = eval.choke_points(&tree, 3).expect("choke points");
        assert_eq!(cp.len(), 4);
        // #1: s_shared (in 2 paths, bottleneck in 1, easiest path feasibility = 3)
        assert_eq!(cp[0].step(), s_shared);
        assert_eq!(cp[0].path_count(), 2);
        assert_eq!(cp[0].bottleneck_count(), 1);
        assert_eq!(*cp[0].easiest_path_feasibility(), 3);
        // #2 and #3: s_b (1 path, 1 bottleneck, feas 2) and s_solo (1 path, 1 bottleneck, feas 1)
        // Ordered by bottleneck_count desc (tied at 1), then step asc (s_b < s_solo).
        assert_eq!(cp[1].step(), s_b);
        assert_eq!(cp[1].path_count(), 1);
        assert_eq!(cp[1].bottleneck_count(), 1);
        assert_eq!(*cp[1].easiest_path_feasibility(), 2);
        assert_eq!(cp[2].step(), s_solo);
        assert_eq!(cp[2].path_count(), 1);
        assert_eq!(cp[2].bottleneck_count(), 1);
        assert_eq!(*cp[2].easiest_path_feasibility(), 1);
        // #4: s_a (1 path, 0 bottlenecks, feas 3)
        assert_eq!(cp[3].step(), s_a);
        assert_eq!(cp[3].path_count(), 1);
        assert_eq!(cp[3].bottleneck_count(), 0);
        assert_eq!(*cp[3].easiest_path_feasibility(), 3);

        // Hitting set for top 2 paths needs only [s_shared]; for top 3 needs [s_shared, s_solo].
        assert_eq!(eval.top_k_hitting_set(&tree, 2), Some(vec![s_shared]));
        assert_eq!(
            eval.top_k_hitting_set(&tree, 3),
            Some(vec![s_shared, s_solo])
        );
    }
}
