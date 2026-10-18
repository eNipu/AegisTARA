//! Topological ordering and cycle extraction over the node arena.
//!
//! The algorithm is Kahn's (A. B. Kahn, "Topological sorting of large
//! networks", Communications of the ACM 5(11), 1962). It is iterative, so a
//! very deep graph cannot overflow the stack. When it cannot order every node,
//! the nodes left over contain at least one cycle; [`extract_cycle`] finds one
//! of them in O(V + E). See `docs/adr/0002-dag-links-and-cycle-detection.md`.

use std::collections::VecDeque;

use crate::error::ModelError;
use crate::id::NodeIdx;
use crate::tree::Node;

/// Returns all nodes in an order where every parent comes before each of its
/// children, or [`ModelError::Cycle`] with one cycle path.
///
/// Deterministic: the queue is seeded with in-degree-0 nodes in ascending
/// index, and children are visited in their stored order.
///
/// Time O(V + E), extra space O(V) plus the output.
pub(crate) fn topological_order(nodes: &[Node]) -> Result<Vec<NodeIdx>, ModelError> {
    // In-degree per node, counting every child edge. A node's in-degree is at
    // most the number of gates (each gate holds a child at most once), which
    // fits in a `u32` except in the theoretical case of a full 2^32-node arena
    // where every node links to the same child; `saturating_add` keeps that
    // case panic-free (the node is then reported as part of a cycle).
    let mut in_degree: Vec<u32> = vec![0; nodes.len()];
    for node in nodes {
        for c in node.children() {
            if let Some(d) = in_degree.get_mut(c.index()) {
                *d = d.saturating_add(1);
            }
        }
    }

    let mut queue: VecDeque<NodeIdx> = in_degree
        .iter()
        .enumerate()
        .filter(|(_, d)| **d == 0)
        .filter_map(|(i, _)| NodeIdx::try_from_usize(i))
        .collect();
    let mut order: Vec<NodeIdx> = Vec::with_capacity(nodes.len());

    while let Some(n) = queue.pop_front() {
        order.push(n);
        let children = nodes.get(n.index()).map_or(&[][..], Node::children);
        for &c in children {
            if let Some(d) = in_degree.get_mut(c.index()) {
                *d = d.saturating_sub(1);
                if *d == 0 {
                    queue.push_back(c);
                }
            }
        }
    }

    if order.len() == nodes.len() {
        Ok(order)
    } else {
        // A node is left over exactly when its in-degree never reached 0.
        Err(ModelError::Cycle(extract_cycle(nodes, &in_degree)))
    }
}

/// Finds one cycle among the nodes with `in_degree > 0` after Kahn's
/// algorithm stopped.
///
/// 1. For every remaining node, record its first remaining predecessor
///    (parents in ascending index, children in stored order). Each remaining
///    node has one, because its in-degree counts only edges from remaining
///    parents and is above 0.
/// 2. Walk predecessors from the smallest remaining index until a node
///    repeats; the walk from that node's first visit to the repeat is a cycle.
/// 3. Reverse it into edge direction and rotate the smallest index to the
///    front.
///
/// Returns the path in edge order: `path[i] -> path[i + 1]` and
/// `path[last] -> path[0]` are edges. Time O(V + E), space O(V).
fn extract_cycle(nodes: &[Node], in_degree: &[u32]) -> Vec<NodeIdx> {
    let remaining = |i: NodeIdx| in_degree.get(i.index()).is_some_and(|d| *d > 0);

    // Step 1: one remaining predecessor per remaining node.
    let mut pred: Vec<Option<NodeIdx>> = vec![None; nodes.len()];
    for (p, node) in nodes.iter().enumerate() {
        let Some(p) = NodeIdx::try_from_usize(p) else {
            continue;
        };
        if !remaining(p) {
            continue;
        }
        for &c in node.children() {
            if !remaining(c) {
                continue;
            }
            if let Some(slot @ None) = pred.get_mut(c.index()) {
                *slot = Some(p);
            }
        }
    }

    // Step 2: walk predecessors from the smallest remaining node, recording
    // the position of each visit in `walk`.
    let start = in_degree
        .iter()
        .position(|d| *d > 0)
        .and_then(NodeIdx::try_from_usize);
    let mut visit_pos: Vec<Option<usize>> = vec![None; nodes.len()];
    let mut walk: Vec<NodeIdx> = Vec::new();
    let mut cur = start;
    let mut cycle_start = walk.len();
    while let Some(n) = cur {
        let Some(slot) = visit_pos.get_mut(n.index()) else {
            break;
        };
        if let Some(first) = *slot {
            cycle_start = first;
            break;
        }
        *slot = Some(walk.len());
        walk.push(n);
        cur = pred.get(n.index()).copied().flatten();
    }

    // Step 3: `walk` runs against edge direction; flip and normalize.
    let mut cycle = walk.split_off(cycle_start.min(walk.len()));
    cycle.reverse();
    if let Some(min_pos) = cycle
        .iter()
        .enumerate()
        .min_by_key(|(_, n)| **n)
        .map(|(i, _)| i)
    {
        cycle.rotate_left(min_pos);
    }
    cycle
}
