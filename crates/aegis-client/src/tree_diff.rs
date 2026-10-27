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

use aegis_model::{AttackTree, GateKind, Node, NodeId, Op, Project, Provenance, TreeId};

use crate::commands::EditError;

type Info = (Arc<str>, Option<GateKind>);

/// One tree seen by node ID.
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
            info: HashMap::new(),
            owner: HashMap::new(),
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
}

/// Ops that clear the rating, factors and controls of one step.
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
    let mut ops = Vec::new();
    if project.step_feasibility(t_idx, idx).is_some() {
        ops.push(Op::SetStepFeasibility {
            tree,
            node,
            level: None,
            provenance: p.clone(),
        });
    }
    for factor in m.feasibility_factors() {
        if project.step_factor(t_idx, idx, factor).is_some() {
            ops.push(Op::SetStepFactor {
                tree,
                node,
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
    ops
}

/// Ops that give a step back the rating, factors and controls it had.
fn restore_step_ops(project: &Project, tree: TreeId, node: NodeId, p: &Provenance) -> Vec<Op> {
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
    let mut ops = Vec::new();
    if let Some(level) = project.step_feasibility(t_idx, idx) {
        ops.push(Op::SetStepFeasibility {
            tree,
            node,
            level: Some(level),
            provenance: p.clone(),
        });
    }
    for factor in m.feasibility_factors() {
        if let Some(level) = project.step_factor(t_idx, idx, factor) {
            ops.push(Op::SetStepFactor {
                tree,
                node,
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
                node,
                control,
                feasibility: Some(mitigation.feasibility()),
                provenance: p.clone(),
            });
        }
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
#[allow(clippy::too_many_lines)]
pub(crate) fn replace_tree_ops(
    project: &Project,
    tree: TreeId,
    new: &AttackTree,
    p: &Provenance,
) -> Result<Vec<Op>, EditError> {
    let t_idx = project.attack_tree_idx(&tree).ok_or(EditError::Missing)?;
    let old_tree = project.attack_tree(t_idx).ok_or(EditError::Missing)?;
    let (old, new) = (Shape::of(old_tree), Shape::of(new));
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

    // Which nodes stay as they are, and which extra edges are kept. Forcing
    // replacements until the kept children of every gate are in the new
    // order, since the ops cannot move a child.
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
                    for c in kept_kids {
                        if old.owner[&c] == Some(g) {
                            force_nodes.push(c);
                        } else {
                            force_edges.push((g, c));
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
        if old.info.get(&a).is_some_and(|(_, gate)| gate.is_none()) {
            ops.extend(restore_step_ops(project, tree, a, p));
        }
    }
    Ok(ops)
}
