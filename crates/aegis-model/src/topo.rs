//! Graph algorithms over the node arena: topological ordering, cycle
//! extraction, and the reachability search that keeps `link` acyclic.
//!
//! The ordering algorithm is Kahn's (A. B. Kahn, "Topological sorting of large
//! networks", Communications of the ACM 5(11), 1962). It is iterative, so a
//! very deep graph cannot overflow the stack. When it cannot order every node,
//! the nodes left over contain at least one cycle; [`extract_cycle`] finds one
//! of them in O(V + E). See `docs/adr/0002-dag-links-and-cycle-detection.md`.

use std::collections::VecDeque;

use crate::id::NodeIdx;
use crate::tree::Node;

/// Returns all nodes in an order where every parent comes before each of its
/// children, or `Err` with the indices of one cycle (see [`extract_cycle`]).
///
/// Deterministic: the queue is seeded with in-degree-0 nodes in ascending
/// index, and children are visited in their stored order.
///
/// Time O(V + E), extra space O(V) plus the output.
pub(crate) fn topological_order(nodes: &[Node]) -> Result<Vec<NodeIdx>, Vec<NodeIdx>> {
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
        Err(extract_cycle(nodes, &in_degree))
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

/// Finds a path from `from` down to `to` along child edges, or `None` if `to`
/// is not reachable from `from`. A node reaches itself: `from == to` gives
/// `[from]`.
///
/// Iterative depth-first search (no recursion): the stack holds the current
/// path, each entry with the position of its next child, so when `to` is
/// found the stack is the path. Children are visited in stored order, so the
/// result is deterministic. Each node is entered at most once (`visited`).
///
/// Time O(d + e) for the d descendants of `from` and their e child edges,
/// plus one zeroed O(V) allocation for `visited`; at most O(V + E). Space
/// O(V).
pub(crate) fn find_path(nodes: &[Node], from: NodeIdx, to: NodeIdx) -> Option<Vec<NodeIdx>> {
    if from == to {
        return Some(vec![from]);
    }
    let mut visited = vec![false; nodes.len()];
    if let Some(v) = visited.get_mut(from.index()) {
        *v = true;
    }
    let mut stack: Vec<(NodeIdx, usize)> = vec![(from, 0)];
    while let Some(top) = stack.last_mut() {
        let (n, k) = *top;
        let children = nodes.get(n.index()).map_or(&[][..], Node::children);
        let Some(&c) = children.get(k) else {
            stack.pop();
            continue;
        };
        top.1 += 1;
        if c == to {
            let mut path: Vec<NodeIdx> = stack.iter().map(|(n, _)| *n).collect();
            path.push(c);
            return Some(path);
        }
        if let Some(v @ false) = visited.get_mut(c.index()) {
            *v = true;
            stack.push((c, 0));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use proptest::prelude::*;

    use super::*;

    fn idx(i: usize) -> NodeIdx {
        NodeIdx::try_from_usize(i).expect("small index")
    }

    /// Builds an arena of `n` gates with the given edges, bypassing
    /// `AttackTree` so that cycles can be represented.
    fn arena(n: usize, edges: &[(usize, usize)]) -> Vec<Node> {
        let label: Arc<str> = Arc::from("g");
        let mut nodes: Vec<Node> = (0..n)
            .map(|_| Node::Gate {
                kind: crate::GateKind::And,
                label: Arc::clone(&label),
                children: Vec::new(),
            })
            .collect();
        for &(p, c) in edges {
            if let Node::Gate { children, .. } = &mut nodes[p] {
                children.push(idx(c));
            }
        }
        nodes
    }

    fn is_edge(nodes: &[Node], p: NodeIdx, c: NodeIdx) -> bool {
        nodes[p.index()].children().contains(&c)
    }

    /// Reference cycle check: iterative three-color DFS, written
    /// independently of Kahn's algorithm. Time O(V + E).
    fn has_cycle_reference(nodes: &[Node]) -> bool {
        // 0 = white, 1 = on stack, 2 = done.
        let mut color = vec![0u8; nodes.len()];
        for start in 0..nodes.len() {
            if color[start] != 0 {
                continue;
            }
            let mut stack = vec![(start, 0usize)];
            color[start] = 1;
            while let Some(top) = stack.last_mut() {
                let (n, k) = *top;
                if let Some(c) = nodes[n].children().get(k) {
                    top.1 += 1;
                    match color[c.index()] {
                        1 => return true,
                        0 => {
                            color[c.index()] = 1;
                            stack.push((c.index(), 0));
                        }
                        _ => {}
                    }
                } else {
                    color[n] = 2;
                    stack.pop();
                }
            }
        }
        false
    }

    #[test]
    fn cycle_self_link() {
        let nodes = arena(3, &[(0, 1), (1, 1), (1, 2)]);
        assert_eq!(topological_order(&nodes), Err(vec![idx(1)]));
    }

    #[test]
    fn cycle_path_exact() {
        // root -> g1 -> g2 -> g3 -> g1, with nodes 4..6 hanging below.
        let nodes = arena(7, &[(0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (2, 6), (3, 1)]);
        assert_eq!(topological_order(&nodes), Err(vec![idx(1), idx(2), idx(3)]));
    }

    #[test]
    fn cycle_path_rotated_to_smallest() {
        // a(1) -> b(2) -> c(3) -> a, entered at b from root.
        let nodes = arena(4, &[(0, 2), (2, 3), (3, 1), (1, 2)]);
        assert_eq!(topological_order(&nodes), Err(vec![idx(1), idx(2), idx(3)]));
    }

    #[test]
    fn topo_deterministic_on_cycle() {
        let nodes = arena(4, &[(0, 1), (1, 2), (2, 1), (2, 3), (3, 2)]);
        let first = topological_order(&nodes);
        assert!(first.is_err());
        assert_eq!(topological_order(&nodes), first);
        assert_eq!(topological_order(&nodes.clone()), first);
    }

    #[test]
    fn deep_chain_cycle() {
        const N: usize = 100_000;
        let mut edges: Vec<(usize, usize)> = (1..N).map(|i| (i - 1, i)).collect();
        edges.push((N - 1, 0));
        let nodes = arena(N, &edges);
        let want: Vec<NodeIdx> = (0..N).map(idx).collect();
        assert_eq!(topological_order(&nodes), Err(want));
    }

    #[test]
    fn find_path_cases() {
        // 0 -> 1 -> 2, 0 -> 3 -> 2 (diamond), 4 isolated.
        let nodes = arena(5, &[(0, 1), (1, 2), (0, 3), (3, 2)]);
        assert_eq!(find_path(&nodes, idx(1), idx(1)), Some(vec![idx(1)]));
        assert_eq!(
            find_path(&nodes, idx(0), idx(2)),
            Some(vec![idx(0), idx(1), idx(2)])
        );
        assert_eq!(
            find_path(&nodes, idx(3), idx(2)),
            Some(vec![idx(3), idx(2)])
        );
        assert_eq!(find_path(&nodes, idx(2), idx(0)), None);
        assert_eq!(find_path(&nodes, idx(4), idx(0)), None);
        assert_eq!(find_path(&nodes, idx(0), idx(4)), None);
    }

    #[test]
    fn find_path_deep_chain() {
        const N: usize = 100_000;
        let edges: Vec<(usize, usize)> = (1..N).map(|i| (i - 1, i)).collect();
        let nodes = arena(N, &edges);
        let path = find_path(&nodes, idx(0), idx(N - 1)).expect("reachable");
        assert_eq!(path.len(), N);
        assert_eq!(find_path(&nodes, idx(N - 1), idx(0)), None);
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        /// AC8: on arbitrary graphs (cycles allowed, built without
        /// `AttackTree`), `topological_order` is `Err` exactly when the
        /// reference finds a cycle, and both results are well formed.
        #[test]
        fn topo_matches_reference(
            n in 1usize..=500,
            raw in prop::collection::vec((any::<usize>(), any::<usize>()), 0..=500),
            forward_only in any::<bool>(),
        ) {
            // `forward_only` keeps every edge low -> high index, so the graph
            // is acyclic; this covers both outcomes well.
            let mut edges: Vec<(usize, usize)> = Vec::new();
            for (a, b) in raw {
                let (a, b) = (a % n, b % n);
                let e = if forward_only { (a.min(b), a.max(b)) } else { (a, b) };
                if forward_only && a == b {
                    continue;
                }
                if !edges.contains(&e) {
                    edges.push(e);
                }
            }
            let nodes = arena(n, &edges);
            let expect_cycle = has_cycle_reference(&nodes);
            if forward_only {
                prop_assert!(!expect_cycle);
            }
            match topological_order(&nodes) {
                Ok(order) => {
                    prop_assert!(!expect_cycle);
                    prop_assert_eq!(order.len(), n);
                    let mut pos = vec![usize::MAX; n];
                    for (k, v) in order.iter().enumerate() {
                        prop_assert_eq!(pos[v.index()], usize::MAX);
                        pos[v.index()] = k;
                    }
                    for &(p, c) in &edges {
                        prop_assert!(pos[p] < pos[c]);
                    }
                }
                Err(path) => {
                    prop_assert!(expect_cycle);
                    prop_assert!(!path.is_empty());
                    let mut seen = vec![false; n];
                    for v in &path {
                        prop_assert!(!seen[v.index()]);
                        seen[v.index()] = true;
                    }
                    for w in path.windows(2) {
                        prop_assert!(is_edge(&nodes, w[0], w[1]));
                    }
                    prop_assert!(is_edge(&nodes, path[path.len() - 1], path[0]));
                    prop_assert_eq!(path.iter().min(), Some(&path[0]));
                }
            }
            prop_assert_eq!(topological_order(&nodes), topological_order(&nodes.clone()));
        }
    }
}
