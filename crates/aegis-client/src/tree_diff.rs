//! From "this tree should become that tree" to node ops (job 065).
//!
//! The outline editor parses a text into the tree it describes. This module
//! compares that tree with the one in the project and returns the smallest
//! sensible list of node ops: nodes that are the same (same ID, same owner)
//! stay as they are, with their ratings, factors and controls, and only the
//! rest is added, linked, unlinked or removed. The list is valid when applied
//! in order, in one batch, so it is one undo step.
//!
//! Rules the ops follow (see `aegis_model::Op`):
//!
//! 1. a removed step is cleared first (a node with a rating cannot go);
//! 2. extra edges that change are unlinked, then nodes are removed, children
//!    before parents;
//! 3. new nodes are added in the new tree's order, each at the end of the
//!    arena and at its place among its owner's children;
//! 4. extra edges that are new are linked;
//! 5. a step that was removed and added again (because it moved to another
//!    owner) gets its rating, factors and controls back.
//!
//! A node is also replaced when keeping it would put its owner's kept
//! children in another order, since the ops cannot move a child. Time
//! O(V + E) per pass and at most O(V) passes; typical edits take one.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use aegis_model::{
    AttackTree, GateKind, Node, NodeId, Op, Project, Provenance, StepRef, StepTarget, TreeId,
};

use crate::commands::EditError;

type Info = (Arc<str>, Option<GateKind>);

/// One tree seen by node ID.
#[derive(Clone)]
struct Shape {
    order: Vec<NodeId>,
    info: HashMap<NodeId, Info>,
    owner: HashMap<NodeId, Option<NodeId>>,
    children: HashMap<NodeId, Vec<NodeId>>,
    root: Option<NodeId>,
}

impl Shape {
    fn of(tree: &AttackTree) -> Self {
        let mut s = Shape {
            order: Vec::with_capacity(tree.len()),
            info: HashMap::with_capacity(tree.len()),
            owner: HashMap::with_capacity(tree.len()),
            children: HashMap::new(),
            root: tree.id_of(tree.root()),
        };
        for (idx, id, node) in tree.iter() {
            s.order.push(id);
            s.info
                .insert(id, (Arc::from(node.label()), node.gate_kind()));
            s.owner
                .insert(id, tree.owner(idx).and_then(|o| tree.id_of(o)));
            if node.gate_kind().is_some() {
                s.children.insert(
                    id,
                    tree.children(idx)
                        .iter()
                        .filter_map(|c| tree.id_of(*c))
                        .collect(),
                );
            }
        }
        s
    }

    fn node(&self, id: NodeId) -> Option<Node> {
        let (label, gate) = self.info.get(&id)?;
        Some(match gate {
            Some(kind) => Node::gate(*kind, Arc::clone(label)),
            None => Node::step(Arc::clone(label)),
        })
    }

    /// Reorders `self.order` so every owner precedes its owned children while
    /// preserving relative order among siblings. Time O(V).
    fn rebuild_order(&mut self) {
        let Some(root) = self.root else {
            return;
        };
        let mut by_owner: HashMap<NodeId, Vec<NodeId>> = HashMap::with_capacity(self.order.len());
        for &id in &self.order {
            if let Some(Some(owner)) = self.owner.get(&id) {
                by_owner.entry(*owner).or_default().push(id);
            }
        }
        let mut out = Vec::with_capacity(self.order.len());
        let mut stack = vec![root];
        while let Some(cur) = stack.pop() {
            out.push(cur);
            if let Some(kids) = by_owner.get(&cur) {
                for &k in kids.iter().rev() {
                    stack.push(k);
                }
            }
        }
        self.order = out;
    }
}

/// Returns a boolean mask marking elements of a maximum-weight strictly
/// increasing subsequence of `ranks` (where every rank is `< rank_bound`),
/// using a Fenwick tree (Binary Indexed Tree) in O(k log k) time and O(k)
/// space. Minimizes the total op cost of evicted subtrees and rated steps.
fn mwis_mask(ranks: &[usize], weights: &[usize], rank_bound: usize) -> Vec<bool> {
    let n = ranks.len();
    let mut keep = vec![false; n];
    if n == 0 {
        return keep;
    }
    let m = rank_bound.max(ranks.iter().copied().max().map_or(0, |r| r + 1));
    // Fenwick tree over 1..=m storing (best_total_weight, index_in_ranks).
    let mut bit: Vec<(usize, usize)> = vec![(0, usize::MAX); m + 1];
    let mut prev: Vec<usize> = vec![usize::MAX; n];
    let mut overall_best: (usize, usize) = (0, usize::MAX);

    for (i, (&r, &w)) in ranks.iter().zip(weights.iter()).enumerate() {
        let mut best_prefix = (0usize, usize::MAX);
        let mut x = r;
        while x > 0 {
            if bit[x].0 > best_prefix.0 {
                best_prefix = bit[x];
            }
            x &= x - 1;
        }
        prev[i] = best_prefix.1;
        let dp_w = best_prefix.0.saturating_add(w.max(1));
        if dp_w > overall_best.0 {
            overall_best = (dp_w, i);
        }
        let mut u = r + 1;
        while u <= m {
            if dp_w > bit[u].0 {
                bit[u] = (dp_w, i);
            }
            u += u & (!u + 1);
        }
    }

    let mut cur = overall_best.1;
    while cur != usize::MAX {
        keep[cur] = true;
        cur = prev[cur];
    }
    keep
}

/// Ops that clear the rating, factors, controls, through item and attack type
/// of one step.
pub(crate) fn clear_step_ops(
    project: &Project,
    tree: TreeId,
    node: NodeId,
    p: &Provenance,
) -> Vec<Op> {
    let m = project.methodology();
    let Some(t_idx) = project.attack_tree_idx(&tree) else {
        return Vec::new();
    };
    let Some(t) = project.attack_tree(t_idx) else {
        return Vec::new();
    };
    let Some(idx) = t.idx_of(&node) else {
        return Vec::new();
    };
    let shared = project.step_shared_id(t_idx, idx);
    let fragment = project.step_fragment_id(t_idx, idx);
    let own = shared.is_none() && fragment.is_none();
    let mut ops = Vec::new();
    if project.review_marked(tree, node) {
        ops.push(Op::SetReviewMark {
            tree,
            node,
            marked: false,
            provenance: p.clone(),
        });
    }
    if fragment.is_some() {
        ops.push(Op::UnuseFragment {
            tree,
            node,
            provenance: p.clone(),
        });
    }
    if shared.is_some() {
        ops.push(Op::UnuseSharedStep {
            tree,
            node,
            provenance: p.clone(),
        });
    }
    if own && project.step_feasibility(t_idx, idx).is_some() {
        ops.push(Op::SetStepFeasibility {
            target: StepTarget::Step(tree, node),
            level: None,
            provenance: p.clone(),
        });
    }
    for factor in m.feasibility_factors().filter(|_| own) {
        if project.step_factor(t_idx, idx, factor).is_some() {
            ops.push(Op::SetStepFactor {
                target: StepTarget::Step(tree, node),
                factor,
                level: None,
                provenance: p.clone(),
            });
        }
    }
    for mitigation in project.step_mitigations(t_idx, idx) {
        if let Some(control) = project.security_control_id(mitigation.control()) {
            ops.push(Op::SetStepMitigation {
                tree,
                node,
                control,
                feasibility: None,
                provenance: p.clone(),
            });
        }
    }
    if own && project.step_via(t_idx, idx).is_some() {
        ops.push(Op::SetStepVia {
            target: StepTarget::Step(tree, node),
            via: None,
            provenance: p.clone(),
        });
    }
    if own && project.step_attack_type(t_idx, idx).is_some() {
        ops.push(Op::SetStepAttackType {
            target: StepTarget::Step(tree, node),
            attack_type: None,
            provenance: p.clone(),
        });
    }
    ops
}

/// Ops that give a step back the rating, factors and controls it had.
fn restore_step_ops(project: &Project, tree: TreeId, node: NodeId, p: &Provenance) -> Vec<Op> {
    transfer_step_ops(project, tree, node, node, p)
}

/// Ops that copy the rating, factors, controls and review mark of `from_node`
/// onto `to_node`.
pub(crate) fn transfer_step_ops(
    project: &Project,
    tree: TreeId,
    from_node: NodeId,
    to_node: NodeId,
    p: &Provenance,
) -> Vec<Op> {
    let m = project.methodology();
    let Some(t_idx) = project.attack_tree_idx(&tree) else {
        return Vec::new();
    };
    let Some(t) = project.attack_tree(t_idx) else {
        return Vec::new();
    };
    let Some(idx) = t.idx_of(&from_node) else {
        return Vec::new();
    };
    let shared = project.step_shared_id(t_idx, idx);
    let fragment = project.step_fragment_id(t_idx, idx);
    let own = shared.is_none() && fragment.is_none();
    let mut ops = Vec::new();
    if let Some(fragment) = fragment {
        ops.push(Op::UseFragment {
            tree,
            node: to_node,
            fragment,
            provenance: p.clone(),
        });
    }
    if let Some(shared) = shared {
        ops.push(Op::UseSharedStep {
            tree,
            node: to_node,
            step: shared,
            provenance: p.clone(),
        });
    }
    if own && let Some(level) = project.step_feasibility(t_idx, idx) {
        ops.push(Op::SetStepFeasibility {
            target: StepTarget::Step(tree, to_node),
            level: Some(level),
            provenance: p.clone(),
        });
    }
    for factor in m.feasibility_factors().filter(|_| own) {
        if let Some(level) = project.step_factor(t_idx, idx, factor) {
            ops.push(Op::SetStepFactor {
                target: StepTarget::Step(tree, to_node),
                factor,
                level: Some(level),
                provenance: p.clone(),
            });
        }
    }
    for mitigation in project.step_mitigations(t_idx, idx) {
        if let Some(control) = project.security_control_id(mitigation.control()) {
            ops.push(Op::SetStepMitigation {
                tree,
                node: to_node,
                control,
                feasibility: Some(mitigation.feasibility()),
                provenance: p.clone(),
            });
        }
    }
    if own && let Some(via) = project.step_via(t_idx, idx) {
        ops.push(Op::SetStepVia {
            target: StepTarget::Step(tree, to_node),
            via: Some(via),
            provenance: p.clone(),
        });
    }
    if own && let Some(attack_type) = project.step_attack_type(t_idx, idx) {
        ops.push(Op::SetStepAttackType {
            target: StepTarget::Step(tree, to_node),
            attack_type: Some(attack_type),
            provenance: p.clone(),
        });
    }
    // Removal cleanup handles source conditions separately; restore them at
    // the destination just like mitigations, even when the leaf is a reference.
    for condition in project.step_condition_link_ids(tree, from_node) {
        if let Some(condition) = project.condition_id(*condition) {
            ops.push(Op::NeedStepCondition {
                tree,
                node: to_node,
                condition,
                provenance: p.clone(),
            });
        }
    }
    if project.review_marked(tree, from_node) {
        ops.push(Op::SetReviewMark {
            tree,
            node: to_node,
            marked: true,
            provenance: p.clone(),
        });
    }
    ops
}

/// The node ops that turn the tree `tree` of `project` into `new`. Both must
/// have the same root ID (the outline parser makes sure).
///
/// # Errors
///
/// [`EditError::Missing`] if the tree is gone; [`EditError::RootKind`] if the
/// root would change between a gate and a step.
pub(crate) fn replace_tree_ops(
    project: &Project,
    tree: TreeId,
    new: &AttackTree,
    p: &Provenance,
) -> Result<Vec<Op>, EditError> {
    let t_idx = project.attack_tree_idx(&tree).ok_or(EditError::Missing)?;
    let old_tree = project.attack_tree(t_idx).ok_or(EditError::Missing)?;
    let (old, new) = (Shape::of(old_tree), Shape::of(new));
    replace_shape_ops(project, tree, &old, &new, p)
}

#[allow(clippy::too_many_lines)]
fn replace_shape_ops(
    project: &Project,
    tree: TreeId,
    old: &Shape,
    new: &Shape,
    p: &Provenance,
) -> Result<Vec<Op>, EditError> {
    let root = old.root.ok_or(EditError::Missing)?;
    if new.root != Some(root) {
        return Err(EditError::Missing);
    }
    let (old_root_kind, new_root_kind) = (old.info[&root].1, new.info[&root].1);
    if old_root_kind.is_some() != new_root_kind.is_some() {
        return Err(EditError::RootKind);
    }

    // The place of each child in the new tree, and its edges.
    let mut rank: HashMap<(NodeId, NodeId), usize> = HashMap::new();
    let mut new_edges: HashSet<(NodeId, NodeId)> = HashSet::new();
    for (g, kids) in &new.children {
        for (i, c) in kids.iter().enumerate() {
            rank.insert((*g, *c), i);
            new_edges.insert((*g, *c));
        }
    }

    let old_edges: HashSet<(NodeId, NodeId)> = old
        .children
        .iter()
        .flat_map(|(g, kids)| kids.iter().map(|c| (*g, *c)))
        .collect();

    // Precompute the op-cost weight of each node's owned subtree in `old`
    // (reverse topological order) so child reordering via Fenwick-tree MWIS
    // keeps heavy subtrees and rated steps in place and moves light ones.
    let mut node_weight: HashMap<NodeId, usize> = HashMap::with_capacity(old.order.len());
    for &id in old.order.iter().rev() {
        let self_cost = if old.info[&id].1.is_none() {
            2 + 2 * clear_step_ops(project, tree, id, p).len()
        } else {
            2
        };
        node_weight.insert(id, self_cost);
    }
    for &id in old.order.iter().rev() {
        let w = node_weight[&id];
        if let Some(Some(owner)) = old.owner.get(&id)
            && let Some(ow) = node_weight.get_mut(owner)
        {
            *ow = ow.saturating_add(w);
        }
    }

    // Which nodes stay as they are, and which extra edges are kept. Forcing
    // replacements until the kept children of every gate are in the new
    // order, using Maximum-Weight Increasing Subsequence (MWIS) in O(k log k)
    // so moving one child among k siblings evicts the minimum-cost subset.
    let mut forced_nodes: HashSet<NodeId> = HashSet::new();
    let mut forced_edges: HashSet<(NodeId, NodeId)> = HashSet::new();
    let mut stays: HashSet<NodeId>;
    loop {
        stays = HashSet::new();
        for &id in &new.order {
            let ok = if id == root {
                true
            } else {
                old.info.get(&id) == new.info.get(&id)
                    && old.owner.get(&id) == new.owner.get(&id)
                    && new.owner[&id].is_some_and(|o| stays.contains(&o))
                    && !forced_nodes.contains(&id)
            };
            if ok {
                stays.insert(id);
            }
        }
        let mut force_nodes = Vec::new();
        let mut force_edges = Vec::new();
        {
            let kept = |g: NodeId, c: NodeId| {
                stays.contains(&c) && new_edges.contains(&(g, c)) && !forced_edges.contains(&(g, c))
            };
            for &g in &new.order {
                let (true, Some(kids)) = (stays.contains(&g), old.children.get(&g)) else {
                    continue;
                };
                let kept_kids: Vec<NodeId> = kids.iter().copied().filter(|c| kept(g, *c)).collect();
                if kept_kids
                    .windows(2)
                    .any(|w| rank[&(g, w[0])] >= rank[&(g, w[1])])
                {
                    let ranks: Vec<usize> = kept_kids.iter().map(|c| rank[&(g, *c)]).collect();
                    let weights: Vec<usize> = kept_kids
                        .iter()
                        .map(|c| {
                            if old.owner[c] == Some(g) {
                                node_weight.get(c).copied().unwrap_or(2)
                            } else {
                                2
                            }
                        })
                        .collect();
                    let bound = new.children.get(&g).map_or(kept_kids.len(), Vec::len);
                    let keep = mwis_mask(&ranks, &weights, bound);
                    for (idx, c) in kept_kids.into_iter().enumerate() {
                        if !keep[idx] {
                            if old.owner[&c] == Some(g) {
                                force_nodes.push(c);
                            } else {
                                force_edges.push((g, c));
                            }
                        }
                    }
                }
            }
        }
        let changed = !force_nodes.is_empty() || !force_edges.is_empty();
        forced_nodes.extend(force_nodes);
        forced_edges.extend(force_edges);
        if !changed {
            break;
        }
    }
    let kept_edge = |g: NodeId, c: NodeId| {
        stays.contains(&c)
            && old_edges.contains(&(g, c))
            && new_edges.contains(&(g, c))
            && !forced_edges.contains(&(g, c))
    };

    let removed: Vec<NodeId> = old
        .order
        .iter()
        .copied()
        .filter(|id| !stays.contains(id))
        .collect();
    let removed_set: HashSet<NodeId> = removed.iter().copied().collect();
    let added: Vec<NodeId> = new
        .order
        .iter()
        .copied()
        .filter(|id| !stays.contains(id))
        .collect();

    let mut ops = Vec::new();
    // The root keeps its place; its text and gate follow the first line.
    if old.info[&root].0 != new.info[&root].0 {
        ops.push(Op::RenameTreeNode {
            tree,
            id: root,
            label: Arc::clone(&new.info[&root].0),
            provenance: p.clone(),
        });
    }
    if let (Some(a), Some(b)) = (old_root_kind, new_root_kind)
        && a != b
    {
        ops.push(Op::SetTreeNodeGate {
            tree,
            id: root,
            gate: b,
            provenance: p.clone(),
        });
    }
    // 1. Clear what hangs on the steps that go.
    for id in &removed {
        if old.info[id].1.is_none() {
            ops.extend(clear_step_ops(project, tree, *id, p));
        }
    }
    // 2. Unlink the extra edges that go: those of removed nodes and those
    // that are not kept.
    for (&g, kids) in &old.children {
        for &c in kids {
            let linked = old.owner[&c] != Some(g);
            if linked && (removed_set.contains(&g) || removed_set.contains(&c) || !kept_edge(g, c))
            {
                ops.push(Op::UnlinkTreeNode {
                    tree,
                    parent: g,
                    child: c,
                    provenance: p.clone(),
                });
            }
        }
    }
    // 3. Remove nodes, children before parents (reverse arena order).
    for id in removed.iter().rev() {
        ops.push(Op::RemoveTreeNode {
            tree,
            id: *id,
            provenance: p.clone(),
        });
    }

    // The children each gate has now, in order, as the ops are applied.
    let mut cur: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
    for (&g, kids) in &old.children {
        if stays.contains(&g) {
            cur.insert(
                g,
                kids.iter().copied().filter(|c| kept_edge(g, *c)).collect(),
            );
        }
    }
    let place = |cur: &HashMap<NodeId, Vec<NodeId>>, g: NodeId, c: NodeId| {
        cur.get(&g).map_or(0, |list| {
            list.iter()
                .filter(|x| rank[&(g, **x)] < rank[&(g, c)])
                .count()
        })
    };
    // 4. Add the new nodes in the new tree's order, at the end of the arena.
    let mut len = old.order.len() - removed.len();
    for &a in &added {
        let Some(owner) = new.owner[&a] else { continue };
        let position = place(&cur, owner, a);
        let Some(node) = new.node(a) else { continue };
        ops.push(Op::AddTreeNode {
            tree,
            parent: owner,
            id: a,
            node,
            index: u32::try_from(len).unwrap_or(u32::MAX),
            position: u32::try_from(position).unwrap_or(u32::MAX),
            provenance: p.clone(),
        });
        cur.entry(owner).or_default().insert(position, a);
        if new.children.contains_key(&a) {
            cur.entry(a).or_default();
        }
        len += 1;
    }
    // 5. Link the extra edges that are new.
    let mut extra: HashMap<NodeId, usize> = HashMap::new();
    for (&g, kids) in &old.children {
        for &c in kids {
            if old.owner[&c] != Some(g) && kept_edge(g, c) {
                *extra.entry(c).or_default() += 1;
            }
        }
    }
    for &g in &new.order {
        let Some(kids) = new.children.get(&g) else {
            continue;
        };
        for &c in kids {
            if new.owner[&c] == Some(g) || kept_edge(g, c) {
                continue;
            }
            let position = place(&cur, g, c);
            let slot = extra.entry(c).or_default();
            ops.push(Op::LinkTreeNode {
                tree,
                parent: g,
                child: c,
                position: u32::try_from(position).unwrap_or(u32::MAX),
                slot: u32::try_from(*slot).unwrap_or(u32::MAX),
                provenance: p.clone(),
            });
            *slot += 1;
            cur.entry(g).or_default().insert(position, c);
        }
    }
    // 6. A step that moved gets its rating, factors and controls back.
    for &a in &added {
        if old.info.get(&a).is_some_and(|(_, gate)| gate.is_none())
            && new.info.get(&a).is_some_and(|(_, gate)| gate.is_none())
        {
            ops.extend(restore_step_ops(project, tree, a, p));
        }
    }
    // 7. Moved/renamed steps keep their assumption exclusions.
    let is_step = |s: &Shape, n| s.info.get(&n).is_some_and(|(_, g)| g.is_none());
    sync_excludes(project, tree, new, p, &mut ops, None, |a| {
        a.excludes_steps().iter().any(|s| {
            s.tree == tree
                && added.contains(&s.node)
                && is_step(old, s.node)
                && is_step(new, s.node)
        })
    });
    Ok(ops)
}

fn sync_excludes(
    project: &Project,
    tree: TreeId,
    new: &Shape,
    p: &Provenance,
    ops: &mut Vec<Op>,
    remap: Option<(StepRef, StepRef)>,
    keep_if: impl Fn(&aegis_model::Assumption) -> bool,
) {
    let keep = |s: StepRef| match remap {
        Some((from, to)) if s == from => Some(to),
        _ if s.tree != tree || new.info.get(&s.node).is_some_and(|(_, g)| g.is_none()) => Some(s),
        _ => None,
    };
    for (_, id, a) in project.assumptions().filter(|(_, _, a)| keep_if(a)) {
        let trees = a
            .excludes_trees()
            .iter()
            .filter_map(|&t| project.attack_tree_id(t))
            .collect();
        let steps = a
            .excludes_steps()
            .iter()
            .copied()
            .filter_map(keep)
            .collect();
        ops.push(Op::SetAssumptionExcludes {
            id,
            trees,
            steps,
            provenance: p.clone(),
        });
    }
}

/// Converts a leaf step `step_node` into a gate of kind `gate_kind` (preserving
/// its `NodeId`, label, owner position and extra parent links) and adds a new
/// child `child_id` under it. If `transfer_ratings` is true and the new child
/// is a step, any feasibility rating, factors, controls and review mark from
/// `step_node` are transferred onto `child_id`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn branch_step_ops(
    project: &Project,
    tree: TreeId,
    step_node: NodeId,
    child_id: NodeId,
    gate_kind: GateKind,
    child_kind: Option<GateKind>,
    child_label: Arc<str>,
    transfer_ratings: bool,
    p: &Provenance,
) -> Result<Vec<Op>, EditError> {
    let t_idx = project.attack_tree_idx(&tree).ok_or(EditError::Missing)?;
    let old_tree = project.attack_tree(t_idx).ok_or(EditError::Missing)?;
    if old_tree.len() >= aegis_model::MAX_ATTACK_TREE_NODES {
        return Err(EditError::Outline(
            crate::outline::OutlineError::TooManyNodes {
                max: aegis_model::MAX_ATTACK_TREE_NODES,
            },
        ));
    }
    let old = Shape::of(old_tree);
    let Some((_, None)) = old.info.get(&step_node) else {
        return Err(EditError::Missing);
    };
    if old.root == Some(step_node) {
        return Err(EditError::RootKind);
    }
    let mut new = old.clone();
    if let Some(info) = new.info.get_mut(&step_node) {
        info.1 = Some(gate_kind);
    }
    new.children.insert(step_node, vec![child_id]);
    new.order.push(child_id);
    new.info.insert(child_id, (child_label, child_kind));
    new.owner.insert(child_id, Some(step_node));
    if child_kind.is_some() {
        new.children.insert(child_id, Vec::new());
    }
    new.rebuild_order();
    let mut ops = replace_shape_ops(project, tree, &old, &new, p)?;
    if transfer_ratings && child_kind.is_none() {
        ops.extend(transfer_step_ops(project, tree, step_node, child_id, p));
        let (old_sr, new_sr) = (
            StepRef {
                tree,
                node: step_node,
            },
            StepRef {
                tree,
                node: child_id,
            },
        );
        sync_excludes(
            project,
            tree,
            &new,
            p,
            &mut ops,
            Some((old_sr, new_sr)),
            |a| a.excludes_steps().contains(&old_sr),
        );
    }
    Ok(ops)
}

/// Inserts an intermediate gate `new_gate_id` at `target_node` in `tree`:
/// - For a non-root node, `new_gate_id` takes `target_node`'s slot under its
///   owner gate, and `target_node` (with its entire subtree) becomes the child
///   of `new_gate_id`.
/// - For the root gate, `new_gate_id` becomes the sole child of the root gate
///   and wraps all of the root gate's existing children under `new_gate_id`.
pub(crate) fn insert_parent_gate_ops(
    project: &Project,
    tree: TreeId,
    target_node: NodeId,
    new_gate_id: NodeId,
    gate_kind: GateKind,
    label: Arc<str>,
    p: &Provenance,
) -> Result<Vec<Op>, EditError> {
    let t_idx = project.attack_tree_idx(&tree).ok_or(EditError::Missing)?;
    let old_tree = project.attack_tree(t_idx).ok_or(EditError::Missing)?;
    if old_tree.len() >= aegis_model::MAX_ATTACK_TREE_NODES {
        return Err(EditError::Outline(
            crate::outline::OutlineError::TooManyNodes {
                max: aegis_model::MAX_ATTACK_TREE_NODES,
            },
        ));
    }
    let old = Shape::of(old_tree);
    if !old.info.contains_key(&target_node) {
        return Err(EditError::Missing);
    }
    let mut new = old.clone();
    if old.root == Some(target_node) {
        if old.info[&target_node].1.is_none() {
            return Err(EditError::RootKind);
        }
        let old_kids = new.children.get(&target_node).cloned().unwrap_or_default();
        new.info.insert(new_gate_id, (label, Some(gate_kind)));
        new.owner.insert(new_gate_id, Some(target_node));
        new.children.insert(target_node, vec![new_gate_id]);
        new.children.insert(new_gate_id, old_kids.clone());
        for c in old_kids {
            if new.owner.get(&c) == Some(&Some(target_node)) {
                new.owner.insert(c, Some(new_gate_id));
            }
        }
        new.order.insert(1.min(new.order.len()), new_gate_id);
    } else {
        let owner_id = old
            .owner
            .get(&target_node)
            .copied()
            .flatten()
            .ok_or(EditError::Missing)?;
        new.info.insert(new_gate_id, (label, Some(gate_kind)));
        new.owner.insert(new_gate_id, Some(owner_id));
        new.children.insert(new_gate_id, vec![target_node]);
        new.owner.insert(target_node, Some(new_gate_id));
        if let Some(kids) = new.children.get_mut(&owner_id)
            && let Some(pos) = kids.iter().position(|c| *c == target_node)
        {
            kids[pos] = new_gate_id;
        }
        if let Some(pos) = new.order.iter().position(|id| *id == target_node) {
            new.order.insert(pos, new_gate_id);
        } else {
            new.order.push(new_gate_id);
        }
    }
    new.rebuild_order();
    replace_shape_ops(project, tree, &old, &new, p)
}

/// Reorders `node` among its owner gate's children by `delta` (`-1` for up,
/// `+1` for down). Uses LIS diffing so only the moved node is touched.
pub(crate) fn move_node_ops(
    project: &Project,
    tree: TreeId,
    node: NodeId,
    delta: isize,
    p: &Provenance,
) -> Result<Vec<Op>, EditError> {
    let t_idx = project.attack_tree_idx(&tree).ok_or(EditError::Missing)?;
    let old_tree = project.attack_tree(t_idx).ok_or(EditError::Missing)?;
    let old = Shape::of(old_tree);
    let owner_id = old
        .owner
        .get(&node)
        .copied()
        .flatten()
        .ok_or(EditError::Missing)?;
    let mut new = old.clone();
    let kids = new.children.get_mut(&owner_id).ok_or(EditError::Missing)?;
    let pos = kids
        .iter()
        .position(|c| *c == node)
        .ok_or(EditError::Missing)?;
    let target = pos.checked_add_signed(delta).ok_or(EditError::Missing)?;
    if target >= kids.len() || target == pos {
        return Ok(Vec::new());
    }
    let item = kids.remove(pos);
    kids.insert(target, item);
    if let (Some(a), Some(b)) = (
        new.order.iter().position(|id| *id == node),
        new.order.iter().position(|id| *id == kids[pos]),
    ) {
        new.order.swap(a, b);
    }
    new.rebuild_order();
    replace_shape_ops(project, tree, &old, &new, p)
}

/// Removes `node` and any descendants reachable only through `node`. If a
/// descendant was shared with another surviving gate outside `node`'s branch,
/// ownership transfers to that surviving gate so the shared node is preserved.
pub(crate) fn remove_node_ops(
    project: &Project,
    tree: TreeId,
    node: NodeId,
    p: &Provenance,
) -> Result<Vec<Op>, EditError> {
    let t_idx = project.attack_tree_idx(&tree).ok_or(EditError::Missing)?;
    let old_tree = project.attack_tree(t_idx).ok_or(EditError::Missing)?;
    let old = Shape::of(old_tree);
    let root = old.root.ok_or(EditError::Missing)?;
    if node == root || !old.info.contains_key(&node) {
        return Err(EditError::Missing);
    }

    // Find all nodes still reachable from `root` when `node` is removed, and
    // record a cycle-free spanning parent from `root` for re-parenting shared
    // descendants whose original owner was inside the removed branch.
    let mut reachable: HashSet<NodeId> = HashSet::with_capacity(old.order.len());
    let mut first_parent: HashMap<NodeId, NodeId> = HashMap::new();
    let mut stack = vec![root];
    reachable.insert(root);
    while let Some(cur) = stack.pop() {
        if let Some(kids) = old.children.get(&cur) {
            for &c in kids.iter().rev() {
                if c != node && reachable.insert(c) {
                    first_parent.insert(c, cur);
                    stack.push(c);
                }
            }
        }
    }

    let mut new = old.clone();
    new.order.retain(|id| reachable.contains(id));
    new.info.retain(|id, _| reachable.contains(id));
    new.owner.retain(|id, _| reachable.contains(id));
    new.children.retain(|id, _| reachable.contains(id));
    for kids in new.children.values_mut() {
        kids.retain(|c| reachable.contains(c));
    }

    // Re-parent any surviving shared descendant whose previous owner was removed.
    for &id in &new.order {
        if id == root {
            continue;
        }
        let owner_alive = new
            .owner
            .get(&id)
            .copied()
            .flatten()
            .is_some_and(|o| reachable.contains(&o));
        if !owner_alive {
            new.owner.insert(id, first_parent.get(&id).copied());
        }
    }
    new.rebuild_order();
    replace_shape_ops(project, tree, &old, &new, p)
}
