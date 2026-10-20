//! Property tests for attack tree evaluation on random DAGs.
//!
//! The hand-built fixtures in `tests/eval.rs` and `tests/explain.rs` check
//! known shapes. This file checks the same engine on thousands of random
//! attack trees with shared subtrees, all three gate kinds, and random leaf
//! values. Every case shows that [`evaluate`]:
//!
//! - agrees with a naive recursive evaluator written here, on every node's
//!   value and explain-why [`Derivation`] (`random_dag_matches_naive`,
//!   `random_dag_trace_matches_naive`);
//! - evaluates each node exactly once (rule A2): the leaf closure runs once
//!   per step and never for a gate, each gate folds its children once, and
//!   each gate clones exactly one value (`*_evaluates_each_node_once`);
//! - is deterministic (principle P4): the same tree, a clone, and the same
//!   plan rebuilt with new UUIDs give equal evaluations, byte for byte in
//!   their `Debug` text, and the same leaf-call sequence
//!   (`*random_dag_deterministic`);
//! - reports the first error in reverse topological order
//!   (`random_dag_first_error`).
//!
//! # Plan format
//!
//! Proptest generates and shrinks a [`Plan`], not a tree: a list of [`Op`]s
//! and a list of leaf values. [`build`] turns a plan into an `AttackTree`:
//!
//! - start with an OR root (arena index 0);
//! - `Add { parent, kind }`: the parent is node `parent % len` (arena
//!   index); if that node is a step, its owner gate is used instead, so the
//!   op is never wasted. `kind` 0, 1, 2 add an OR, AND, or SAND gate, 3 adds
//!   a step. The new node gets the next arena index and a fresh `UUIDv7`;
//! - `Link { parent, child }`: calls `tree.link(parent % len, child % len)`.
//!   `NotAGate`, `DuplicateEdge`, and `WouldCreateCycle` mean "skip" (the
//!   tree must be unchanged); any other error fails the case;
//! - with `fix_empty`, every gate that still has no children gets one step
//!   child at the end, so evaluation succeeds.
//!
//! Step `i` (arena index) has the raw leaf value `leaves[i % leaves.len()]`.
//! So the `Debug` text of a failing plan is enough to rebuild the tree by
//! hand. Shrinking works on `ops`, so a failure shrinks to a small plan.
//!
//! # Why the DAGs stay acyclic, and why they are interesting
//!
//! `Add` only appends a fresh node under an existing gate, which cannot
//! close a cycle. `Link` may connect any two nodes in either index direction
//! (so shapes are not limited to "low index to high index"), and
//! `AttackTree::link` rejects every edge that would close a cycle. Ops are
//! interleaved, so links happen while the tree grows. `Link` has a high
//! weight, so most small plans share at least one node; `generator_coverage`
//! checks this, plus all gate kinds, deep trees, and trees with no link, on
//! 256 deterministic draws.
//!
//! # Domains
//!
//! - `MaxMin<u8>` with leaf `v % 4`: few levels, many ties, which tests the
//!   "earlier child wins a tie" rule of the trace.
//! - `MinTime` with the raw value, drawn from small values, values near
//!   `u64::MAX`, and any `u64`: tests saturation of the SAND sum.
//! - `Poly` (test-only): OR, AND, and SAND are three different
//!   non-commutative polynomial steps, so a fold in the wrong order or with
//!   the wrong gate rule changes the value.
//!
//! # Naive evaluator budget
//!
//! The naive evaluator is plain recursion through `tree.children(n)` with no
//! memo, called once per node. It is exponential on DAGs on purpose, so it
//! stops after [`NAIVE_BUDGET`] recursive calls per tree and domain; such a
//! case is rejected with `prop_assume!`. `generator_coverage` asserts that at
//! most 5% of small plans go over the budget. Recursion depth is at most the
//! node count (under 100 for small plans), so the stack is safe.
//!
//! # Replaying a failure
//!
//! On a failure proptest prints the shrunk plan and stores its seed in
//! `proptest-regressions/eval_properties.txt`; commit that file, and every
//! later run replays the seed first. For a longer soak run, set
//! `PROPTEST_CASES`, for example
//! `PROPTEST_CASES=10000 cargo test --release --test eval_properties`.
//!
//! Cost: test code only. Building a plan is O(ops x (V + E)) because every
//! `link` runs a cycle check; the naive evaluator is bounded by its budget;
//! every other check is O(V + E) per case.

use std::cell::Cell;
use std::fmt;

use aegis_model::{
    AttackTree, AttributeDomain, Derivation, Evaluation, GateKind, MaxMin, MinTime, ModelError,
    Node, NodeId, NodeIdx, evaluate,
};
use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::{FileFailurePersistence, TestRunner};

/// Maximum recursive calls of the naive evaluator per tree and domain.
const NAIVE_BUDGET: usize = 1_000_000;

// ---------------------------------------------------------------------------
// Plans and the plan builder
// ---------------------------------------------------------------------------

/// One step of a build plan. Positions are taken modulo the current node
/// count; see the module docs.
#[derive(Clone, Debug)]
enum Op {
    /// Add a node under node `parent % len` (or its owner, if it is a step).
    /// `kind` 0 = OR, 1 = AND, 2 = SAND, 3 = step.
    Add { parent: usize, kind: u8 },
    /// Link node `child % len` under node `parent % len`.
    Link { parent: usize, child: usize },
}

/// What proptest generates: the ops, and the raw leaf values by arena index
/// (`leaves[i % leaves.len()]`, never empty).
#[derive(Clone, Debug)]
struct Plan {
    ops: Vec<Op>,
    leaves: Vec<u64>,
}

/// A tree built from a plan.
struct Built {
    tree: AttackTree,
    /// Every node in arena order (`nodes[i].index() == i`).
    nodes: Vec<NodeIdx>,
    /// Number of `link` calls that succeeded.
    links: usize,
}

fn fail(e: impl fmt::Display) -> TestCaseError {
    TestCaseError::fail(e.to_string())
}

/// One op. `pos` bounds the raw positions so a shrunk plan stays readable.
fn op(pos: usize, add_weight: u32, link_weight: u32) -> impl Strategy<Value = Op> {
    prop_oneof![
        add_weight => (0..pos, 0u8..4).prop_map(|(parent, kind)| Op::Add { parent, kind }),
        link_weight => (0..pos, 0..pos).prop_map(|(parent, child)| Op::Link { parent, child }),
    ]
}

/// Keeps the first `max_add` `Add` ops and the first `max_link` `Link` ops,
/// in their original order. Time O(ops).
fn cap(ops: Vec<Op>, max_add: usize, max_link: usize) -> Vec<Op> {
    let (mut adds, mut links) = (0, 0);
    ops.into_iter()
        .filter(|op| match op {
            Op::Add { .. } => {
                adds += 1;
                adds <= max_add
            }
            Op::Link { .. } => {
                links += 1;
                links <= max_link
            }
        })
        .collect()
}

/// A raw leaf value: small, near `u64::MAX` (saturation), or any.
fn leaf_value() -> impl Strategy<Value = u64> {
    prop_oneof![0u64..8, (u64::MAX - 7)..=u64::MAX, any::<u64>()]
}

/// Small plans: 0..=48 `Add` and 0..=24 `Link` ops, interleaved.
fn small_plan() -> impl Strategy<Value = Plan> {
    (
        prop::collection::vec(op(4096, 3, 2), 0..=72).prop_map(|ops| cap(ops, 48, 24)),
        prop::collection::vec(leaf_value(), 1..=16),
    )
        .prop_map(|(ops, leaves)| Plan { ops, leaves })
}

/// Large plans: up to 3,000 `Add` and 3,000 `Link` ops, interleaved.
fn large_plan() -> impl Strategy<Value = Plan> {
    (
        prop::collection::vec(op(1 << 20, 1, 1), 0..=6000).prop_map(|ops| cap(ops, 3000, 3000)),
        prop::collection::vec(leaf_value(), 1..=64),
    )
        .prop_map(|(ops, leaves)| Plan { ops, leaves })
}

fn make_node(kind: u8) -> Node {
    match kind {
        0 => Node::gate(GateKind::Or, "or"),
        1 => Node::gate(GateKind::And, "and"),
        2 => Node::gate(GateKind::SeqAnd, "sand"),
        _ => Node::step("step"),
    }
}

fn is_step(tree: &AttackTree, idx: NodeIdx) -> bool {
    tree.get(idx).is_some_and(|n| n.gate_kind().is_none())
}

fn pick(nodes: &[NodeIdx], pos: usize) -> Result<NodeIdx, TestCaseError> {
    nodes
        .get(pos % nodes.len().max(1))
        .copied()
        .ok_or_else(|| fail("no nodes"))
}

/// Builds the tree described by `plan` (see the module docs). Asserts that
/// `link` fails only with the expected errors and that a skipped link leaves
/// the tree unchanged. Time O(ops x (V + E)).
fn build(plan: &Plan, fix_empty: bool) -> Result<Built, TestCaseError> {
    let mut tree =
        AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root")).map_err(fail)?;
    let mut nodes = vec![tree.root()];
    let mut links = 0;
    for op in &plan.ops {
        match *op {
            Op::Add { parent, kind } => {
                let mut p = pick(&nodes, parent)?;
                if is_step(&tree, p) {
                    p = tree.owner(p).ok_or_else(|| fail("step without owner"))?;
                }
                let idx = tree
                    .add_child(p, NodeId::new_v7(), make_node(kind))
                    .map_err(fail)?;
                prop_assert_eq!(idx.index(), nodes.len());
                nodes.push(idx);
            }
            Op::Link { parent, child } => {
                let p = pick(&nodes, parent)?;
                let c = pick(&nodes, child)?;
                let len = tree.len();
                let before = tree.children(p).to_vec();
                match tree.link(p, c) {
                    Ok(()) => {
                        links += 1;
                        prop_assert_eq!(tree.len(), len);
                        prop_assert_eq!(tree.children(p).len(), before.len() + 1);
                        prop_assert_eq!(tree.children(p).last(), Some(&c));
                    }
                    Err(
                        e @ (ModelError::NotAGate(_)
                        | ModelError::DuplicateEdge { .. }
                        | ModelError::WouldCreateCycle { .. }),
                    ) => {
                        // Skip: the tree must be unchanged.
                        if let ModelError::NotAGate(x) = e {
                            prop_assert_eq!(x, p);
                            prop_assert!(is_step(&tree, p));
                        } else {
                            prop_assert!(!is_step(&tree, p), "{e}");
                        }
                        prop_assert_eq!(tree.len(), len);
                        prop_assert_eq!(tree.children(p), &before[..]);
                    }
                    Err(e) => return Err(fail(format!("unexpected link error: {e}"))),
                }
            }
        }
    }
    if fix_empty {
        let empty: Vec<NodeIdx> = nodes
            .iter()
            .copied()
            .filter(|&n| tree.get(n).is_some_and(|node| node.gate_kind().is_some()))
            .filter(|&n| tree.children(n).is_empty())
            .collect();
        for gate in empty {
            let idx = tree
                .add_child(gate, NodeId::new_v7(), Node::step("fill"))
                .map_err(fail)?;
            prop_assert_eq!(idx.index(), nodes.len());
            nodes.push(idx);
        }
    }
    prop_assert_eq!(nodes.len(), tree.len());
    Ok(Built { tree, nodes, links })
}

/// The raw leaf value of arena index `idx`.
fn raw(plan: &Plan, idx: NodeIdx) -> u64 {
    plan.leaves
        .get(idx.index() % plan.leaves.len().max(1))
        .copied()
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Domains and the naive evaluator
// ---------------------------------------------------------------------------

/// Test-only domain whose three gate rules differ and do not commute.
#[derive(Clone, Copy, Debug)]
struct Poly;

impl AttributeDomain for Poly {
    type Value = u64;
    fn or(&self, acc: u64, next: &u64) -> u64 {
        acc.wrapping_mul(31).wrapping_add(next ^ 1)
    }
    fn and(&self, acc: u64, next: &u64) -> u64 {
        acc.wrapping_mul(37).wrapping_add(next ^ 2)
    }
    fn seq_and(&self, acc: u64, next: &u64) -> u64 {
        acc.wrapping_mul(41).wrapping_add(next ^ 3)
    }
}

/// A domain under test plus its naive, independently written semantics.
trait Oracle: AttributeDomain<Value: PartialEq + fmt::Debug> {
    /// Maps a raw plan value to this domain's leaf value.
    fn leaf(&self, raw: u64) -> Self::Value;

    /// The value and derivation of a `kind` gate whose children have the
    /// given values, in stored order. `None` for no children.
    fn naive_gate(
        &self,
        kind: GateKind,
        inputs: &[(NodeIdx, Self::Value)],
    ) -> Option<(Self::Value, Derivation)>;
}

/// Selection: the first child, replaced by every strictly `better` later
/// child. Time O(k).
fn select<V: Clone>(
    kind: GateKind,
    inputs: &[(NodeIdx, V)],
    better: impl Fn(&V, &V) -> bool,
) -> Option<(V, Derivation)> {
    let (first, rest) = inputs.split_first()?;
    let mut best = first;
    for x in rest {
        if better(&x.1, &best.1) {
            best = x;
        }
    }
    Some((
        best.1.clone(),
        Derivation::Selected {
            rule: kind,
            child: best.0,
        },
    ))
}

impl Oracle for MaxMin<u8> {
    fn leaf(&self, raw: u64) -> u8 {
        u8::try_from(raw % 4).unwrap_or(0)
    }

    fn naive_gate(&self, kind: GateKind, inputs: &[(NodeIdx, u8)]) -> Option<(u8, Derivation)> {
        match kind {
            GateKind::Or => select(kind, inputs, |n, best| n > best),
            GateKind::And | GateKind::SeqAnd => select(kind, inputs, |n, best| n < best),
        }
    }
}

impl Oracle for MinTime {
    fn leaf(&self, raw: u64) -> u64 {
        raw
    }

    fn naive_gate(&self, kind: GateKind, inputs: &[(NodeIdx, u64)]) -> Option<(u64, Derivation)> {
        match kind {
            GateKind::Or => select(kind, inputs, |n, best| n < best),
            GateKind::And => select(kind, inputs, |n, best| n > best),
            // No children: `None`; one child: that child is selected.
            GateKind::SeqAnd if inputs.len() <= 1 => select(kind, inputs, |_, _| false),
            GateKind::SeqAnd => {
                let wide: u128 = inputs.iter().map(|(_, v)| u128::from(*v)).sum();
                Some((
                    u64::try_from(wide).unwrap_or(u64::MAX),
                    Derivation::Combined { rule: kind },
                ))
            }
        }
    }
}

impl Oracle for Poly {
    fn leaf(&self, raw: u64) -> u64 {
        raw
    }

    fn naive_gate(&self, kind: GateKind, inputs: &[(NodeIdx, u64)]) -> Option<(u64, Derivation)> {
        let ((_, first), rest) = inputs.split_first()?;
        if rest.is_empty() {
            return select(kind, inputs, |_, _| false);
        }
        let (mul, xor) = match kind {
            GateKind::Or => (31, 1),
            GateKind::And => (37, 2),
            GateKind::SeqAnd => (41, 3),
        };
        let value = rest.iter().fold(*first, |acc, (_, v)| {
            acc.wrapping_mul(mul).wrapping_add(v ^ xor)
        });
        Some((value, Derivation::Combined { rule: kind }))
    }
}

/// Why the naive evaluator stopped.
enum Stop {
    OverBudget,
    Broken(TestCaseError),
}

/// Plain recursion, no memo: the value and derivation of `idx`. Each call
/// uses one unit of `budget`. Time exponential in the worst case, bounded by
/// the budget.
fn naive_node<D: Oracle>(
    tree: &AttackTree,
    domain: &D,
    plan: &Plan,
    idx: NodeIdx,
    budget: &mut usize,
) -> Result<(D::Value, Derivation), Stop> {
    *budget = budget.checked_sub(1).ok_or(Stop::OverBudget)?;
    if is_step(tree, idx) {
        return Ok((domain.leaf(raw(plan, idx)), Derivation::Leaf));
    }
    let kind = tree
        .get(idx)
        .and_then(Node::gate_kind)
        .ok_or_else(|| Stop::Broken(fail(format!("unknown node {idx:?}"))))?;
    let mut inputs = Vec::with_capacity(tree.children(idx).len());
    for &c in tree.children(idx) {
        let (v, _) = naive_node(tree, domain, plan, c, budget)?;
        inputs.push((c, v));
    }
    domain
        .naive_gate(kind, &inputs)
        .ok_or_else(|| Stop::Broken(fail(format!("empty gate {idx:?}"))))
}

/// Naive value and derivation per node, in arena order.
type NaiveAll<V> = Vec<(V, Derivation)>;

/// The naive value and derivation of every node, in arena order, or `None`
/// if the budget ran out.
fn naive_all<D: Oracle>(
    built: &Built,
    domain: &D,
    plan: &Plan,
) -> Result<Option<NaiveAll<D::Value>>, TestCaseError> {
    let mut budget = NAIVE_BUDGET;
    let mut out = Vec::with_capacity(built.nodes.len());
    for &n in &built.nodes {
        match naive_node(&built.tree, domain, plan, n, &mut budget) {
            Ok(x) => out.push(x),
            Err(Stop::OverBudget) => return Ok(None),
            Err(Stop::Broken(e)) => return Err(e),
        }
    }
    Ok(Some(out))
}

/// Compares `evaluate` with the naive evaluator on every node: values, and
/// derivations if `traces`. Returns `false` (nothing checked) if the naive
/// evaluator went over budget.
fn agrees<D: Oracle>(
    built: &Built,
    domain: &D,
    plan: &Plan,
    traces: bool,
) -> Result<bool, TestCaseError> {
    let Some(want) = naive_all(built, domain, plan)? else {
        return Ok(false);
    };
    let eval = evaluate(&built.tree, domain, |i| Some(domain.leaf(raw(plan, i)))).map_err(fail)?;
    prop_assert_eq!(eval.len(), built.tree.len());
    prop_assert_eq!(eval.get(built.tree.root()), Some(eval.root_value()));
    for (&n, (value, how)) in built.nodes.iter().zip(&want) {
        prop_assert_eq!(eval.get(n), Some(value), "value of {:?}", n);
        if traces {
            prop_assert_eq!(eval.derivation(n), Some(*how), "derivation of {:?}", n);
        }
    }
    Ok(true)
}

// ---------------------------------------------------------------------------
// Counting
// ---------------------------------------------------------------------------

/// Counts the plain `or`, `and`, and `seq_and` calls, then delegates. Does
/// not override the `*_kept` methods, whose defaults call the plain method
/// exactly once.
struct Counting<D> {
    inner: D,
    or: Cell<usize>,
    and: Cell<usize>,
    seq: Cell<usize>,
}

impl<D> Counting<D> {
    fn new(inner: D) -> Self {
        Counting {
            inner,
            or: Cell::new(0),
            and: Cell::new(0),
            seq: Cell::new(0),
        }
    }

    /// `[or, and, seq_and]` call counts.
    fn counts(&self) -> [usize; 3] {
        [self.or.get(), self.and.get(), self.seq.get()]
    }
}

fn bump(c: &Cell<usize>) {
    c.set(c.get() + 1);
}

impl<D: AttributeDomain> AttributeDomain for Counting<D> {
    type Value = D::Value;
    fn or(&self, acc: D::Value, next: &D::Value) -> D::Value {
        bump(&self.or);
        self.inner.or(acc, next)
    }
    fn and(&self, acc: D::Value, next: &D::Value) -> D::Value {
        bump(&self.and);
        self.inner.and(acc, next)
    }
    fn seq_and(&self, acc: D::Value, next: &D::Value) -> D::Value {
        bump(&self.seq);
        self.inner.seq_and(acc, next)
    }
}

thread_local! {
    /// Number of `Counted::clone` calls on this thread.
    static CLONES: Cell<usize> = const { Cell::new(0) };
}

/// A value whose `Clone` is counted in `CLONES`.
#[derive(Debug)]
struct Counted(u64);

impl Clone for Counted {
    fn clone(&self) -> Self {
        CLONES.with(bump);
        Counted(self.0)
    }
}

/// `Poly` over `Counted`; never clones (it reads `next.0`).
struct CountedPoly;

impl AttributeDomain for CountedPoly {
    type Value = Counted;
    fn or(&self, acc: Counted, next: &Counted) -> Counted {
        Counted(Poly.or(acc.0, &next.0))
    }
    fn and(&self, acc: Counted, next: &Counted) -> Counted {
        Counted(Poly.and(acc.0, &next.0))
    }
    fn seq_and(&self, acc: Counted, next: &Counted) -> Counted {
        Counted(Poly.seq_and(acc.0, &next.0))
    }
}

fn kind_slot(kind: GateKind) -> usize {
    match kind {
        GateKind::Or => 0,
        GateKind::And => 1,
        GateKind::SeqAnd => 2,
    }
}

/// AC3: leaf closure once per step and never for a gate; `k - 1` domain
/// calls per gate of `k` children, per kind; one value clone per gate.
fn check_each_node_once(built: &Built, plan: &Plan) -> Result<(), TestCaseError> {
    let tree = &built.tree;
    let mut want = [0usize; 3];
    let mut gates = 0usize;
    for (_, _, node) in tree.iter() {
        if let Some(kind) = node.gate_kind() {
            gates += 1;
            if let Some(slot) = want.get_mut(kind_slot(kind)) {
                *slot += node.children().len().saturating_sub(1);
            }
        }
    }

    let counting = Counting::new(Poly);
    let mut calls = vec![0u32; tree.len()];
    let poly = evaluate(tree, &counting, |i| {
        if let Some(c) = calls.get_mut(i.index()) {
            *c += 1;
        }
        Some(raw(plan, i))
    })
    .map_err(fail)?;
    for (idx, _, node) in tree.iter() {
        let expected = u32::from(node.gate_kind().is_none());
        prop_assert_eq!(calls.get(idx.index()).copied(), Some(expected), "{:?}", idx);
    }
    prop_assert_eq!(counting.counts(), want);

    CLONES.with(|c| c.set(0));
    let counted = Counting::new(CountedPoly);
    let eval = evaluate(tree, &counted, |i| Some(Counted(raw(plan, i)))).map_err(fail)?;
    prop_assert_eq!(CLONES.with(Cell::get), gates);
    prop_assert_eq!(counted.counts(), want);
    for &n in &built.nodes {
        prop_assert_eq!(eval.get(n).map(|c| c.0), poly.get(n).copied());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Determinism
// ---------------------------------------------------------------------------

/// Evaluates `tree` and records the leaf-call sequence.
fn run<D: Oracle>(
    tree: &AttackTree,
    domain: &D,
    plan: &Plan,
) -> Result<(Evaluation<D::Value>, Vec<NodeIdx>), TestCaseError> {
    let mut seq = Vec::new();
    let eval = evaluate(tree, domain, |i| {
        seq.push(i);
        Some(domain.leaf(raw(plan, i)))
    })
    .map_err(fail)?;
    Ok((eval, seq))
}

/// AC4: twice, a clone, and a rebuild with new UUIDs give the same result.
fn check_deterministic<D: Oracle>(
    built: &Built,
    rebuilt: &Built,
    domain: &D,
    plan: &Plan,
) -> Result<(), TestCaseError> {
    let clone = built.tree.clone();
    let (first, first_seq) = run(&built.tree, domain, plan)?;
    let first_text = format!("{first:?}");
    for tree in [&built.tree, &clone, &rebuilt.tree] {
        let (eval, seq) = run(tree, domain, plan)?;
        prop_assert!(eval == first, "evaluations differ");
        prop_assert!(format!("{eval:?}") == first_text, "Debug text differs");
        prop_assert!(seq == first_seq, "leaf-call sequences differ");
    }
    Ok(())
}

fn check_all_deterministic(plan: &Plan) -> Result<(), TestCaseError> {
    let built = build(plan, true)?;
    let rebuilt = build(plan, true)?;
    // Same shape, new UUIDs.
    prop_assert_eq!(built.tree.len(), rebuilt.tree.len());
    prop_assert_ne!(
        built.tree.id_of(built.tree.root()),
        rebuilt.tree.id_of(rebuilt.tree.root())
    );
    for &n in &built.nodes {
        prop_assert_eq!(built.tree.children(n), rebuilt.tree.children(n));
    }
    check_deterministic(&built, &rebuilt, &MaxMin::<u8>::new(), plan)?;
    check_deterministic(&built, &rebuilt, &MinTime, plan)?;
    check_deterministic(&built, &rebuilt, &Poly, plan)
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// The first offending node in reverse topological order, or `None`.
fn first_error(
    tree: &AttackTree,
    missing: impl Fn(NodeIdx) -> bool,
) -> Result<Option<ModelError>, TestCaseError> {
    let order = tree.topological_order().map_err(fail)?;
    for &n in order.iter().rev() {
        let id = tree.id_of(n).ok_or_else(|| fail("no id"))?;
        if is_step(tree, n) {
            if missing(n) {
                return Ok(Some(ModelError::MissingLeafValue(id)));
            }
        } else if tree.children(n).is_empty() {
            return Ok(Some(ModelError::EmptyGate(id)));
        }
    }
    Ok(None)
}

/// AC5 on one tree.
fn check_first_error(built: &Built, plan: &Plan, missing: &[bool]) -> Result<(), TestCaseError> {
    let tree = &built.tree;
    let is_missing = |i: NodeIdx| {
        missing
            .get(i.index() % missing.len().max(1))
            .copied()
            .unwrap_or(false)
    };
    let any_bad = built.nodes.iter().any(|&n| {
        if is_step(tree, n) {
            is_missing(n)
        } else {
            tree.children(n).is_empty()
        }
    });
    let want = first_error(tree, is_missing)?;
    prop_assert_eq!(want.is_some(), any_bad);

    let leaf = |i: NodeIdx| (!is_missing(i)).then(|| raw(plan, i));
    let got = evaluate(tree, &MinTime, leaf);
    let again = evaluate(tree, &MinTime, leaf);
    match want {
        None => prop_assert!(got.is_ok(), "{:?}", got.err()),
        Some(e) => prop_assert_eq!(got.as_ref().err(), Some(&e)),
    }
    prop_assert!(got == again, "a second run gave a different result");
    Ok(())
}

// ---------------------------------------------------------------------------
// Properties
// ---------------------------------------------------------------------------

/// `cases` cases, unless the `PROPTEST_CASES` environment variable is set
/// (`with_cases` alone would ignore it). Failures are persisted to
/// `proptest-regressions/eval_properties.txt` relative to the crate root
/// (the working directory of `cargo test`); the default persistence looks
/// for a `lib.rs` above the test file and finds none under `tests/`.
fn config(cases: u32) -> ProptestConfig {
    let mut config = ProptestConfig::default();
    if std::env::var_os("PROPTEST_CASES").is_none() {
        config.cases = cases;
    }
    config.failure_persistence = Some(Box::new(FileFailurePersistence::Direct(
        "proptest-regressions/eval_properties.txt",
    )));
    config
}

proptest! {
    #![proptest_config(config(256))]

    /// AC1: values agree with the naive evaluator for all three domains.
    #[test]
    fn random_dag_matches_naive(plan in small_plan()) {
        let built = build(&plan, true)?;
        prop_assume!(agrees(&built, &MaxMin::<u8>::new(), &plan, false)?);
        prop_assert!(agrees(&built, &MinTime, &plan, false)?);
        prop_assert!(agrees(&built, &Poly, &plan, false)?);
    }

    /// AC2: derivations agree with the naive evaluator (ties pick the
    /// earlier child; `MinTime` SAND and multi-child `Poly` are combined).
    #[test]
    fn random_dag_trace_matches_naive(plan in small_plan()) {
        let built = build(&plan, true)?;
        prop_assume!(agrees(&built, &MaxMin::<u8>::new(), &plan, true)?);
        prop_assert!(agrees(&built, &MinTime, &plan, true)?);
        prop_assert!(agrees(&built, &Poly, &plan, true)?);
    }

    /// AC3 on small DAGs.
    #[test]
    fn random_dag_evaluates_each_node_once(plan in small_plan()) {
        let built = build(&plan, true)?;
        check_each_node_once(&built, &plan)?;
    }

    /// AC4 on small DAGs.
    #[test]
    fn random_dag_deterministic(plan in small_plan()) {
        check_all_deterministic(&plan)?;
    }

    /// AC5: the first error in reverse topological order, on trees with
    /// empty gates (`fix_empty = false`) and, for the `Ok` and
    /// missing-value paths, on the same plan with `fix_empty = true`.
    #[test]
    fn random_dag_first_error(
        plan in small_plan(),
        missing in prop::collection::vec(prop::bool::weighted(0.15), 1..=16),
    ) {
        check_first_error(&build(&plan, false)?, &plan, &missing)?;
        check_first_error(&build(&plan, true)?, &plan, &missing)?;
    }
}

proptest! {
    #![proptest_config(config(32))]

    /// AC3 on large DAGs.
    #[test]
    fn large_random_dag_evaluates_each_node_once(plan in large_plan()) {
        let built = build(&plan, true)?;
        check_each_node_once(&built, &plan)?;
    }

    /// AC4 on large DAGs.
    #[test]
    fn large_random_dag_deterministic(plan in large_plan()) {
        check_all_deterministic(&plan)?;
    }
}

// ---------------------------------------------------------------------------
// Generator coverage
// ---------------------------------------------------------------------------

/// Length of the longest path from the root, in edges. Time O(V + E).
fn depth(tree: &AttackTree) -> Result<usize, TestCaseError> {
    let order = tree.topological_order().map_err(fail)?;
    let mut depth = vec![0usize; tree.len()];
    let mut max = 0;
    for &n in &order {
        let d = depth.get(n.index()).copied().unwrap_or(0);
        max = max.max(d);
        for &c in tree.children(n) {
            if let Some(slot) = depth.get_mut(c.index()) {
                *slot = (*slot).max(d + 1);
            }
        }
    }
    Ok(max)
}

/// AC6: the small-plan generator produces the shapes the properties need.
#[test]
fn generator_coverage() -> Result<(), TestCaseError> {
    const DRAWS: usize = 256;
    let mut runner = TestRunner::deterministic();
    let strategy = small_plan();
    let (mut shared2, mut shared3, mut deep, mut no_link, mut over) = (0, 0, 0, 0, 0);
    let mut kinds = [0usize; 3];
    for _ in 0..DRAWS {
        let plan = strategy
            .new_tree(&mut runner)
            .map_err(|r| fail(format!("{r:?}")))?
            .current();
        let built = build(&plan, true)?;
        let tree = &built.tree;
        let max_parents = built
            .nodes
            .iter()
            .map(|&n| tree.parents(n).count())
            .max()
            .unwrap_or(0);
        shared2 += usize::from(max_parents >= 2);
        shared3 += usize::from(max_parents >= 3);
        let mut seen = [false; 3];
        for (idx, _, node) in tree.iter() {
            if let (true, Some(kind)) = (idx != tree.root(), node.gate_kind()) {
                if let Some(s) = seen.get_mut(kind_slot(kind)) {
                    *s = true;
                }
            }
        }
        for (k, s) in kinds.iter_mut().zip(seen) {
            *k += usize::from(s);
        }
        deep += usize::from(depth(tree)? >= 8);
        no_link += usize::from(built.links == 0);
        over += usize::from(naive_all(&built, &MinTime, &plan)?.is_none());
    }
    let stats = format!(
        "shared2={shared2} shared3={shared3} kinds(or,and,sand)={kinds:?} \
         deep={deep} no_link={no_link} over_budget={over} of {DRAWS}"
    );
    prop_assert!(shared2 * 2 >= DRAWS, "{}", stats);
    prop_assert!(shared3 >= 1, "{}", stats);
    for k in kinds {
        prop_assert!(k * 2 >= DRAWS, "{}", stats);
    }
    prop_assert!(deep >= 1, "{}", stats);
    prop_assert!(no_link >= 1, "{}", stats);
    prop_assert!(over * 20 <= DRAWS, "{}", stats);
    println!("generator coverage: {stats}");
    Ok(())
}
