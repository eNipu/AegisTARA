//! Independent input fold for fragment-aware sensitivity and cache fixtures.
//! No Project evaluation, cached roots, reverse indexes or ceiling helpers.

#![allow(dead_code)]
#![allow(clippy::pedantic)]

use std::collections::{HashMap, HashSet};

use aegis_model::{
    Applicability, ControlStatus, ElementRef, FeasibilityLevel, GateKind, ItemRef, Lifted, NodeId,
    NodeIdx, Project, TreeId, TreeIdx,
};

pub type Value = Lifted<FeasibilityLevel>;
pub type TippingTuple = (
    NodeId,
    FeasibilityLevel,
    FeasibilityLevel,
    FeasibilityLevel,
    FeasibilityLevel,
);

pub struct Oracle<'a> {
    project: &'a Project,
    app: Applicability,
    roots: HashMap<(TreeId, bool), Result<Value, String>>,
    active: HashSet<(TreeId, bool)>,
}

impl<'a> Oracle<'a> {
    pub fn new(project: &'a Project) -> Self {
        Self {
            project,
            app: project.applicability(),
            roots: HashMap::new(),
            active: HashSet::new(),
        }
    }

    pub fn root(&mut self, id: TreeId, residual: bool) -> Result<Value, String> {
        if let Some(root) = self.roots.get(&(id, residual)) {
            return root.clone();
        }
        if !self.active.insert((id, residual)) {
            return Err(format!("reference cycle at {id}"));
        }
        let root = self.host(id, residual, None);
        self.active.remove(&(id, residual));
        self.roots.insert((id, residual), root.clone());
        root
    }

    /// Perturb only this host's resolved leaf, never its source fragment.
    pub fn host(
        &mut self,
        id: TreeId,
        residual: bool,
        override_leaf: Option<(NodeId, FeasibilityLevel)>,
    ) -> Result<Value, String> {
        let idx = self
            .project
            .attack_tree_idx(&id)
            .ok_or_else(|| format!("unknown tree {id}"))?;
        if self.app.is_excluded(ElementRef::Tree(id)) {
            return Ok(Lifted::Excluded);
        }
        let tree = self.project.attack_tree(idx).expect("live tree");
        self.node(id, idx, tree.root(), residual, override_leaf)
    }

    fn node(
        &mut self,
        id: TreeId,
        tree_idx: TreeIdx,
        idx: NodeIdx,
        residual: bool,
        override_leaf: Option<(NodeId, FeasibilityLevel)>,
    ) -> Result<Value, String> {
        let tree = self.project.attack_tree(tree_idx).expect("live tree");
        let node = tree.get(idx).expect("live node");
        let node_id = tree.id_of(idx).expect("live node id");
        let Some(gate) = node.gate_kind() else {
            let baseline = self.leaf(id, tree_idx, idx, residual)?;
            return Ok(match (baseline, override_leaf) {
                (Lifted::Value(_), Some((target, level))) if target == node_id => {
                    Lifted::Value(level)
                }
                _ => baseline,
            });
        };
        if node.children().is_empty() {
            return Err(format!("empty gate {node_id}"));
        }
        // Visit every child; exclusions must not hide missing inputs.
        let children: Vec<_> = node
            .children()
            .iter()
            .map(|&child| self.node(id, tree_idx, child, residual, override_leaf))
            .collect();
        let children = children.into_iter().collect::<Result<Vec<_>, _>>()?;
        let values: Vec<_> = children
            .iter()
            .filter_map(|v| match v {
                Lifted::Value(v) => Some(*v),
                Lifted::Excluded => None,
            })
            .collect();
        if gate != GateKind::Or && values.len() != children.len() {
            return Ok(Lifted::Excluded);
        }
        Ok(match gate {
            GateKind::Or => values.into_iter().max(),
            GateKind::And | GateKind::SeqAnd => values.into_iter().min(),
        }
        .map_or(Lifted::Excluded, Lifted::Value))
    }

    pub fn leaf(
        &mut self,
        id: TreeId,
        tree_idx: TreeIdx,
        idx: NodeIdx,
        residual: bool,
    ) -> Result<Value, String> {
        let p = self.project;
        let node = p
            .attack_tree(tree_idx)
            .expect("tree")
            .id_of(idx)
            .expect("node");
        if self.app.is_node_excluded(id, node) {
            return Ok(Lifted::Excluded);
        }
        let value = if let Some(fragment) = p.step_fragment_id(tree_idx, idx) {
            self.root(fragment, residual)?
        } else {
            // Ordinary own/shared factor resolution only, never a reference.
            Lifted::Value(
                p.effective_step_feasibility(tree_idx, idx)
                    .ok_or_else(|| format!("missing leaf {node}"))?,
            )
        };
        let Lifted::Value(mut value) = value else {
            return Ok(value);
        };
        let counts = |status| {
            matches!(status, ControlStatus::Implemented)
                || (residual && matches!(status, ControlStatus::Planned))
        };
        for mitigation in p.step_mitigations(tree_idx, idx) {
            let control = p.security_control(mitigation.control()).expect("control");
            if counts(control.status()) {
                value = value.min(mitigation.feasibility());
            }
        }
        if let (Some(via), Some(attack)) =
            (p.step_via(tree_idx, idx), p.step_attack_type(tree_idx, idx))
        {
            let owner = match via {
                ItemRef::Interface(id) => p
                    .interface_idx(&id)
                    .and_then(|idx| p.interface(idx))
                    .and_then(|interface| p.component_id(interface.component()))
                    .map(ItemRef::Component),
                ItemRef::Component(_) => None,
            };
            for (idx, _, control) in p.security_controls() {
                if counts(control.status())
                    && p.control_targets(idx)
                        .any(|target| target == via || Some(target) == owner)
                    && let Some(ceiling) = control.rule_for(attack)
                {
                    value = value.min(ceiling);
                }
            }
        }
        Ok(Lifted::Value(value))
    }

    /// Complete adjacent-level enumeration independent of reported tips.
    pub fn tips(&mut self, id: TreeId, residual: bool) -> Result<Vec<TippingTuple>, String> {
        let Lifted::Value(from_root) = self.root(id, residual)? else {
            return Ok(vec![]);
        };
        let p = self.project;
        let idx = p.attack_tree_idx(&id).expect("tree");
        let tree = p.attack_tree(idx).expect("tree");
        let levels: Vec<_> = p
            .methodology()
            .feasibility_levels()
            .map(|(v, _, _)| v)
            .collect();
        let mut tips = Vec::new();
        for (step, node, entry) in tree.iter() {
            if entry.gate_kind().is_some() {
                continue;
            }
            let Lifted::Value(from_step) = self.leaf(id, idx, step, residual)? else {
                continue;
            };
            for next in [
                from_step.index().checked_sub(1),
                from_step.index().checked_add(1),
            ]
            .into_iter()
            .flatten()
            {
                let Some(&to_step) = levels.get(next) else {
                    continue;
                };
                if let Lifted::Value(to_root) = self.host(id, residual, Some((node, to_step)))?
                    && to_root != from_root
                {
                    tips.push((node, from_step, to_step, from_root, to_root));
                }
            }
        }
        tips.sort();
        Ok(tips)
    }
}
