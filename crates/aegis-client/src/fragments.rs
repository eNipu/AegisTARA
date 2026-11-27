//! Bounded attack-tree subtree copying and fragment reference materialization.

use std::collections::{BTreeMap, HashMap};

use aegis_model::{
    AssumptionId, AttackTree, AttackType, ConditionId, ConditionIdx, ControlId, ElementRef,
    FactorLevel, FeasibilityFactor, FeasibilityLevel, GateKind, ItemRef, MAX_ATTACK_TREE_NODES,
    MAX_CONDITIONS_PER_ELEMENT, MAX_FRAGMENT_DEPTH, MAX_FRAGMENT_REFERENCES, MAX_STEP_MITIGATIONS,
    Node, NodeId, NodeIdx, Op, Project, Provenance, SharedStepId, StepRef, StepTarget, TreeId,
    TreeIdx, TreeRole,
};

use crate::commands::EditError;

const MAX_DAG_EDGES: usize = MAX_ATTACK_TREE_NODES * (MAX_ATTACK_TREE_NODES - 1) / 2;

/// Whether subtree copying keeps reusable links or expands them in place.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyMode {
    /// Keep shared-step and fragment-reference links on copied steps.
    PreserveReferences,
    /// Expand fragment occurrences recursively and copy shared-step fields as own fields.
    Materialize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct NodeMeta {
    feasibility: Option<FeasibilityLevel>,
    factors: Vec<(FeasibilityFactor, FactorLevel)>,
    via: Option<ItemRef>,
    attack_type: Option<AttackType>,
    conditions: Vec<ConditionId>,
    mitigations: Vec<(ControlId, FeasibilityLevel)>,
    assumptions: Vec<AssumptionId>,
    review_marked: bool,
    shared_step: Option<SharedStepId>,
    fragment: Option<TreeId>,
}

/// Fresh-ID copy of an attack-tree subtree and its stable per-node metadata.
#[doc(hidden)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CopyPlan {
    tree: AttackTree,
    meta: Vec<NodeMeta>,
}

impl CopyPlan {
    /// The copied attack tree DAG with fresh node IDs.
    #[must_use]
    pub fn tree(&self) -> &AttackTree {
        &self.tree
    }

    /// Gate kind of the copied root, or `None` when the root is a step.
    #[must_use]
    pub fn root_kind(&self) -> Option<GateKind> {
        self.tree.get(self.tree.root()).and_then(Node::gate_kind)
    }

    /// Emits ops to create tree `id` with `role` via a single-root skeleton
    /// `AddAttackTree`, ordered `AddTreeNode`/`LinkTreeNode`, and metadata ops.
    pub fn new_tree_ops(
        &self,
        id: TreeId,
        role: TreeRole,
        p: &Provenance,
    ) -> Result<Vec<Op>, EditError> {
        let root_idx = self.tree.root();
        let root_id = self.tree.id_of(root_idx).ok_or(EditError::Missing)?;
        let root_node = self.tree.get(root_idx).ok_or(EditError::Missing)?;
        let tree = AttackTree::new(
            root_id,
            blank_node(root_node.gate_kind(), root_node.label()),
        )
        .map_err(|_| EditError::EmptyName)?;
        let mut ops = vec![Op::AddAttackTree {
            role,
            id,
            tree,
            step_feasibility: Vec::new(),
            provenance: p.clone(),
        }];
        ops.extend(self.insert_ops(id, root_id, 1, p)?);
        Ok(ops)
    }

    /// Emits insertion and metadata ops into an existing cleared destination
    /// `root` of the planned kind inside `tree`, keeping `root`'s label/ID.
    pub fn insert_ops(
        &self,
        tree: TreeId,
        root: NodeId,
        start_index: usize,
        p: &Provenance,
    ) -> Result<Vec<Op>, EditError> {
        if start_index == 0 {
            return Err(EditError::Missing);
        }
        let u32_node = |v| u32::try_from(v).map_err(|_| limit("nodes", MAX_ATTACK_TREE_NODES));
        let u32_edge = |v| u32::try_from(v).map_err(|_| limit("edges", MAX_DAG_EDGES));
        add_capped(
            start_index,
            self.tree.len().saturating_sub(1),
            "nodes",
            MAX_ATTACK_TREE_NODES,
        )?;
        let dest = |idx: NodeIdx| {
            (idx == self.tree.root())
                .then_some(root)
                .or_else(|| self.tree.id_of(idx))
                .ok_or(EditError::Missing)
        };
        let mut cur: Vec<Vec<(usize, ())>> = vec![Vec::new(); self.tree.len()];
        let mut ops = Vec::new();

        for (idx, id, node) in self.tree.iter() {
            if idx == self.tree.root() {
                continue;
            }
            let owner = self.tree.owner(idx).ok_or(EditError::Missing)?;
            let (pos, _) =
                insert_ranked(&mut cur[owner.index()], self.tree.children(owner), idx, ())?;
            ops.push(Op::AddTreeNode {
                tree,
                parent: dest(owner)?,
                id,
                node: blank_node(node.gate_kind(), node.label()),
                index: u32_node(start_index + idx.index() - 1)?,
                position: u32_node(pos)?,
                provenance: p.clone(),
            });
        }
        for (idx, _, _) in self.tree.iter() {
            let child = dest(idx)?;
            for (slot, lp) in self
                .tree
                .parents(idx)
                .skip(usize::from(self.tree.owner(idx).is_some()))
                .enumerate()
            {
                let (pos, _) =
                    insert_ranked(&mut cur[lp.index()], self.tree.children(lp), idx, ())?;
                ops.push(Op::LinkTreeNode {
                    tree,
                    parent: dest(lp)?,
                    child,
                    position: u32_edge(pos)?,
                    slot: u32_edge(slot)?,
                    provenance: p.clone(),
                });
            }
        }
        for (idx, _, node) in self.tree.iter() {
            if node.gate_kind().is_none() {
                let meta = self.meta.get(idx.index()).ok_or(EditError::Missing)?;
                emit_step_meta_ops(tree, dest(idx)?, meta, p, &mut ops);
            }
        }
        Ok(ops)
    }

    /// Maps each assumption excluding any materialized source fragment or leaf
    /// to the destination leaf node IDs (substituting `root` when the root is
    /// a single step).
    #[doc(hidden)]
    #[must_use]
    pub fn materialized_assumptions(&self, root: NodeId) -> BTreeMap<AssumptionId, Vec<NodeId>> {
        let mut out: BTreeMap<AssumptionId, Vec<NodeId>> = BTreeMap::new();
        for (idx, id, node) in self.tree.iter() {
            if node.gate_kind().is_some() {
                continue;
            }
            let dest = if idx == self.tree.root() { root } else { id };
            if let Some(meta) = self.meta.get(idx.index()) {
                for &aid in &meta.assumptions {
                    out.entry(aid).or_default().push(dest);
                }
            }
        }
        out
    }
}

/// Copies the reachable DAG subtree rooted at `(tree, node)` in `project`
/// using fresh node IDs and bounded preflight checks.
#[doc(hidden)]
pub fn copy_subtree(
    project: &Project,
    tree: TreeId,
    node: NodeId,
    mode: CopyMode,
) -> Result<CopyPlan, EditError> {
    let t_idx = project.attack_tree_idx(&tree).ok_or(EditError::Missing)?;
    let src_tree = project.attack_tree(t_idx).ok_or(EditError::Missing)?;
    let start_idx = src_tree.idx_of(&node).ok_or(EditError::Missing)?;
    match mode {
        CopyMode::PreserveReferences => preflight_preserve(project, t_idx, src_tree, start_idx)?,
        CopyMode::Materialize => preflight_materialize(project, t_idx, start_idx)?,
    }
    build_plan_direct(project, t_idx, start_idx, mode)
}

fn limit(resource: &'static str, max: usize) -> EditError {
    EditError::FragmentCopyLimit { resource, max }
}

fn insert_ranked<T>(
    cur: &mut Vec<(usize, T)>,
    order: &[NodeIdx],
    target: NodeIdx,
    val: T,
) -> Result<(usize, Option<usize>), EditError> {
    let rank = order
        .iter()
        .position(|&c| c == target)
        .ok_or(EditError::Missing)?;
    let pos = cur.partition_point(|(r, _)| *r < rank);
    let after = (pos < cur.len()).then_some(pos);
    cur.insert(pos, (rank, val));
    Ok((pos, after))
}

fn blank_node(kind: Option<GateKind>, label: &str) -> Node {
    match kind {
        Some(k) => Node::gate(k, label),
        None => Node::step(label),
    }
}

fn tree_pair(project: &Project, t_idx: TreeIdx) -> Result<(TreeId, &AttackTree), EditError> {
    Ok((
        project.attack_tree_id(t_idx).ok_or(EditError::Missing)?,
        project.attack_tree(t_idx).ok_or(EditError::Missing)?,
    ))
}

fn add_capped(acc: usize, add: usize, res: &'static str, max: usize) -> Result<usize, EditError> {
    acc.checked_add(add)
        .filter(|&t| t <= max)
        .ok_or_else(|| limit(res, max))
}

fn reachable_topo(
    src_tree: &AttackTree,
    start_idx: NodeIdx,
) -> (Vec<NodeIdx>, Vec<Option<usize>>, usize) {
    let mut seen = vec![false; src_tree.len()];
    let mut indeg = vec![0usize; src_tree.len()];
    let mut edges = 0usize;
    let mut stack = vec![start_idx];
    seen[start_idx.index()] = true;
    while let Some(u) = stack.pop() {
        for &c in src_tree.children(u) {
            edges = edges.saturating_add(1);
            indeg[c.index()] += 1;
            if !seen[c.index()] {
                seen[c.index()] = true;
                stack.push(c);
            }
        }
    }
    let (mut topo, mut pos, mut ready) = (Vec::new(), vec![None; src_tree.len()], vec![start_idx]);
    while let Some(u) = ready.pop() {
        pos[u.index()] = Some(topo.len());
        topo.push(u);
        for &c in src_tree.children(u).iter().rev() {
            indeg[c.index()] -= 1;
            if indeg[c.index()] == 0 {
                ready.push(c);
            }
        }
    }
    (topo, pos, edges)
}

fn preflight_preserve(
    project: &Project,
    t_idx: TreeIdx,
    src_tree: &AttackTree,
    start_idx: NodeIdx,
) -> Result<(), EditError> {
    let (nodes, _, edges) = reachable_topo(src_tree, start_idx);
    add_capped(nodes.len(), 0, "nodes", MAX_ATTACK_TREE_NODES)?;
    add_capped(edges, 0, "edges", MAX_DAG_EDGES)?;
    let refs = nodes
        .iter()
        .filter(|&&n| project.step_fragment(t_idx, n).is_some())
        .count();
    add_capped(refs, 0, "fragment_references", MAX_FRAGMENT_REFERENCES)?;
    Ok(())
}

fn preflight_materialize(
    project: &Project,
    root_t: TreeIdx,
    root_start: NodeIdx,
) -> Result<(), EditError> {
    let mut active = Vec::new();
    let mut memo: HashMap<TreeIdx, (usize, usize, usize)> = HashMap::new();
    let mut stack = vec![(root_t, root_start, false)];

    while let Some((t_idx, start, expanded)) = stack.pop() {
        if !expanded {
            if t_idx != root_t && memo.contains_key(&t_idx) {
                continue;
            }
            active.push(t_idx);
            let src_tree = project.attack_tree(t_idx).ok_or(EditError::Missing)?;
            let (nodes, _, edges) = reachable_topo(src_tree, start);
            add_capped(nodes.len(), 0, "nodes", MAX_ATTACK_TREE_NODES)?;
            add_capped(edges, 0, "edges", MAX_DAG_EDGES)?;
            stack.push((t_idx, start, true));
            for &n in &nodes {
                if let Some(frag) = project.step_fragment(t_idx, n) {
                    if active.contains(&frag) || active.len() > MAX_FRAGMENT_DEPTH {
                        return Err(limit("fragment_depth", MAX_FRAGMENT_DEPTH));
                    }
                    if !memo.contains_key(&frag) {
                        let ft = project.attack_tree(frag).ok_or(EditError::Missing)?;
                        stack.push((frag, ft.root(), false));
                    }
                }
            }
        } else {
            active.pop();
            if t_idx != root_t && memo.contains_key(&t_idx) {
                continue;
            }
            let src_tree = project.attack_tree(t_idx).ok_or(EditError::Missing)?;
            let (nodes, _, mut edges) = reachable_topo(src_tree, start);
            let (mut height, mut total_nodes) = (0usize, 0usize);
            for &n in &nodes {
                let (add, e_add) = match project.step_fragment(t_idx, n) {
                    Some(frag) => {
                        let &(ch, cn, ce) = memo.get(&frag).ok_or(EditError::Missing)?;
                        height = height.max(ch + 1);
                        if height > MAX_FRAGMENT_DEPTH {
                            return Err(limit("fragment_depth", MAX_FRAGMENT_DEPTH));
                        }
                        (cn, ce)
                    }
                    None => (1, 0),
                };
                total_nodes = add_capped(total_nodes, add, "nodes", MAX_ATTACK_TREE_NODES)?;
                edges = add_capped(edges, e_add, "edges", MAX_DAG_EDGES)?;
            }
            if t_idx != root_t {
                memo.insert(t_idx, (height, total_nodes, edges));
            }
        }
    }
    Ok(())
}

fn build_plan_direct(
    project: &Project,
    t_idx: TreeIdx,
    start_idx: NodeIdx,
    mode: CopyMode,
) -> Result<CopyPlan, EditError> {
    let (et, en, e_node, e_meta, eov) =
        resolve_node(project, t_idx, start_idx, mode, &NodeMeta::default())?;
    let root_id = NodeId::new_v7();
    let mut tree = AttackTree::new(root_id, blank_node(e_node.gate_kind(), e_node.label()))
        .map_err(|_| EditError::EmptyName)?;
    let mut meta = vec![e_meta];
    let mut work = Vec::new();
    if e_node.gate_kind().is_some() {
        work.push((et, en, eov, root_id));
    }

    while let Some((item_t, item_start, item_ov, entry_id)) = work.pop() {
        let src_tree = project.attack_tree(item_t).ok_or(EditError::Missing)?;
        let (nodes, pos_in_sub, _) = reachable_topo(src_tree, item_start);
        let (mut sub_ids, mut cur) = (vec![entry_id; nodes.len()], vec![Vec::new(); nodes.len()]);
        let mut links = Vec::new();

        for (i, &v) in nodes.iter().enumerate().skip(1) {
            let parents: Vec<_> = src_tree
                .parents(v)
                .filter_map(|p| pos_in_sub[p.index()])
                .collect();
            let owner_pos = *parents.first().ok_or(EditError::Missing)?;
            let owner_idx = tree.idx_of(&sub_ids[owner_pos]).ok_or(EditError::Missing)?;
            let is_frag =
                mode == CopyMode::Materialize && project.step_fragment(item_t, v).is_some();
            let (nt, nn, nn_node, n_meta, ov) = resolve_node(project, item_t, v, mode, &item_ov)?;

            let id = NodeId::new_v7();
            let (pos, after) = insert_ranked(
                &mut cur[owner_pos],
                src_tree.children(nodes[owner_pos]),
                v,
                id,
            )?;
            let node = blank_node(nn_node.gate_kind(), nn_node.label());
            if let Some(after) = after {
                let at = tree
                    .idx_of(&cur[owner_pos][after + 1].1)
                    .ok_or(EditError::Missing)?;
                tree.insert_child(owner_idx, at, pos, id, node)
                    .map_err(|_| limit("nodes", MAX_ATTACK_TREE_NODES))?;
                meta.insert(at.index(), n_meta);
            } else {
                tree.add_child(owner_idx, id, node)
                    .map_err(|_| limit("nodes", MAX_ATTACK_TREE_NODES))?;
                meta.push(n_meta);
            }
            sub_ids[i] = id;
            if parents.len() > 1 {
                links.push((id, v, parents));
            }
            if is_frag && nn_node.gate_kind().is_some() && !nn_node.children().is_empty() {
                work.push((nt, nn, ov, id));
            }
        }
        for (id, v, parents) in links {
            let child_idx = tree.idx_of(&id).ok_or(EditError::Missing)?;
            for (slot, &lp_pos) in parents[1..].iter().enumerate() {
                let lp_idx = tree.idx_of(&sub_ids[lp_pos]).ok_or(EditError::Missing)?;
                let (l_pos, _) =
                    insert_ranked(&mut cur[lp_pos], src_tree.children(nodes[lp_pos]), v, id)?;
                tree.link_at(lp_idx, child_idx, l_pos, slot)
                    .map_err(|_| limit("edges", MAX_DAG_EDGES))?;
            }
        }
    }
    Ok(CopyPlan { tree, meta })
}

fn resolve_node<'a>(
    project: &'a Project,
    mut t_idx: TreeIdx,
    mut n_idx: NodeIdx,
    mode: CopyMode,
    base: &NodeMeta,
) -> Result<(TreeIdx, NodeIdx, &'a Node, NodeMeta, NodeMeta), EditError> {
    let mut meta = if mode == CopyMode::Materialize {
        base.clone()
    } else {
        NodeMeta::default()
    };
    if mode == CopyMode::Materialize {
        let mut steps = 0usize;
        while let Some(frag_idx) = project.step_fragment(t_idx, n_idx) {
            steps += 1;
            if steps > MAX_FRAGMENT_DEPTH {
                return Err(limit("fragment_depth", MAX_FRAGMENT_DEPTH));
            }
            let (tree_id, src) = tree_pair(project, t_idx)?;
            let n_id = src.id_of(n_idx).ok_or(EditError::Missing)?;
            let frag_id = project.attack_tree_id(frag_idx).ok_or(EditError::Missing)?;
            apply_step_overlay(project, t_idx, tree_id, n_idx, n_id, &mut meta);
            push_conds(
                project,
                project.condition_link_ids(ElementRef::Tree(frag_id)),
                &mut meta.conditions,
            );
            push_assumptions(
                project,
                tree_id,
                n_id,
                Some(frag_idx),
                &mut meta.assumptions,
            );
            t_idx = frag_idx;
            n_idx = project
                .attack_tree(frag_idx)
                .ok_or(EditError::Missing)?
                .root();
        }
        meta.conditions.sort_unstable();
        meta.conditions.dedup();
        meta.assumptions.sort_unstable();
        meta.assumptions.dedup();
    }

    let (tree_id, tree) = tree_pair(project, t_idx)?;
    let n_id = tree.id_of(n_idx).ok_or(EditError::Missing)?;
    let node = tree.get(n_idx).ok_or(EditError::Missing)?;
    if node.gate_kind().is_some() {
        return Ok((t_idx, n_idx, node, NodeMeta::default(), meta));
    }

    if mode == CopyMode::PreserveReferences {
        meta.fragment = project.step_fragment_id(t_idx, n_idx);
        meta.shared_step = project.step_shared_id(t_idx, n_idx);
    } else if project.step_shared(t_idx, n_idx).is_some()
        && project.step_shared_id(t_idx, n_idx).is_none()
    {
        return Err(EditError::Missing);
    }
    if mode == CopyMode::Materialize || (meta.fragment.is_none() && meta.shared_step.is_none()) {
        meta.feasibility = project.step_feasibility(t_idx, n_idx);
        meta.factors = project.step_factors(t_idx, n_idx).collect();
        meta.via = project.step_via(t_idx, n_idx);
        meta.attack_type = project.step_attack_type(t_idx, n_idx);
    }
    apply_step_overlay(project, t_idx, tree_id, n_idx, n_id, &mut meta);
    if mode == CopyMode::Materialize {
        push_assumptions(project, tree_id, n_id, None, &mut meta.assumptions);
        meta.assumptions.sort_unstable();
        meta.assumptions.dedup();
    }
    meta.conditions.sort_unstable();
    meta.conditions.dedup();
    if meta.conditions.len() > MAX_CONDITIONS_PER_ELEMENT {
        return Err(limit("step_conditions", MAX_CONDITIONS_PER_ELEMENT));
    }
    if meta.mitigations.len() > MAX_STEP_MITIGATIONS {
        return Err(limit("step_mitigations", MAX_STEP_MITIGATIONS));
    }
    Ok((t_idx, n_idx, node, meta, NodeMeta::default()))
}

fn push_conds(project: &Project, links: &[ConditionIdx], out: &mut Vec<ConditionId>) {
    out.extend(links.iter().filter_map(|&i| project.condition_id(i)));
}

fn push_assumptions(
    project: &Project,
    tree: TreeId,
    node: NodeId,
    frag_idx: Option<TreeIdx>,
    out: &mut Vec<AssumptionId>,
) {
    let sr = StepRef { tree, node };
    for (_, aid, a) in project.assumptions() {
        if a.excludes_steps().contains(&sr)
            || frag_idx.is_some_and(|fi| a.excludes_trees().contains(&fi))
        {
            out.push(aid);
        }
    }
}

fn apply_step_overlay(
    project: &Project,
    t_idx: TreeIdx,
    tree_id: TreeId,
    n_idx: NodeIdx,
    n_id: NodeId,
    ov: &mut NodeMeta,
) {
    push_conds(
        project,
        project.step_condition_link_ids(tree_id, n_id),
        &mut ov.conditions,
    );
    for m in project.step_mitigations(t_idx, n_idx) {
        if let Some(ctrl) = project.security_control_id(m.control()) {
            match ov.mitigations.binary_search_by_key(&ctrl, |x| x.0) {
                Ok(i) => ov.mitigations[i].1 = ov.mitigations[i].1.min(m.feasibility()),
                Err(i) => ov.mitigations.insert(i, (ctrl, m.feasibility())),
            }
        }
    }
    ov.review_marked |= project.review_marked(tree_id, n_id);
}

fn emit_step_meta_ops(
    tree: TreeId,
    node: NodeId,
    meta: &NodeMeta,
    p: &Provenance,
    ops: &mut Vec<Op>,
) {
    let target = StepTarget::Step(tree, node);
    let prov = || p.clone();
    if let Some(fragment) = meta.fragment {
        ops.push(Op::UseFragment {
            tree,
            node,
            fragment,
            provenance: prov(),
        });
    } else if let Some(step) = meta.shared_step {
        ops.push(Op::UseSharedStep {
            tree,
            node,
            step,
            provenance: prov(),
        });
    } else {
        if let Some(level) = meta.feasibility {
            ops.push(Op::SetStepFeasibility {
                target,
                level: Some(level),
                provenance: prov(),
            });
        }
        for &(factor, level) in &meta.factors {
            ops.push(Op::SetStepFactor {
                target,
                factor,
                level: Some(level),
                provenance: prov(),
            });
        }
        if let Some(via) = meta.via {
            ops.push(Op::SetStepVia {
                target,
                via: Some(via),
                provenance: prov(),
            });
        }
        if let Some(attack_type) = meta.attack_type {
            ops.push(Op::SetStepAttackType {
                target,
                attack_type: Some(attack_type),
                provenance: prov(),
            });
        }
    }
    for &condition in &meta.conditions {
        ops.push(Op::NeedStepCondition {
            tree,
            node,
            condition,
            provenance: prov(),
        });
    }
    for &(control, feasibility) in &meta.mitigations {
        ops.push(Op::SetStepMitigation {
            tree,
            node,
            control,
            feasibility: Some(feasibility),
            provenance: prov(),
        });
    }
    if meta.review_marked {
        ops.push(Op::SetReviewMark {
            tree,
            node,
            marked: true,
            provenance: prov(),
        });
    }
}
