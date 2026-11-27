//! Typed fragment edits and structure-preserving loaded-project undo.
#[path = "common/fragments.rs"]
mod fragments;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use aegis_client::commands::{Edit, EditError};
use aegis_client::fragments::{CopyMode, copy_subtree};
use aegis_client::relations::{RelationKey, candidates, related};
use aegis_client::view::Selection;
use aegis_client::workspace::{Origin, Workspace, WorkspaceError};
use aegis_model::{
    AssetId, AttackFilter, AttackTree, ComponentId, ConditionId, ControlId, ControlStatus,
    ElementRef, FeasibilityLevel, GateKind, InterfaceId, ItemRef, MAX_ATTACK_TREE_NODES,
    MAX_CONDITIONS_PER_ELEMENT, MAX_FRAGMENT_DEPTH, MAX_FRAGMENT_REFERENCES, MAX_STEP_MITIGATIONS,
    Methodology, ModelError, Node, NodeId, NodeIdx, Op, Project, Provenance, SharedStepId,
    StepTarget, ThreatId, TreeId, TreeIdx, TreeRole,
};
use fragments::{loaded, saved};

fn apply(ws: &mut Workspace, edit: &Edit) {
    let ops = edit.to_ops(ws.project(), "new-editor", 900).expect("ops");
    assert!(!ops.is_empty(), "{edit:?}");
    ws.apply(ops, Origin::Edit).expect("apply");
}

fn exact(ws: &mut Workspace, edit: &Edit) {
    let before = ws.project().clone();
    let before_bytes = saved(&before);
    apply(ws, edit);
    let after = ws.project().clone();
    let after_bytes = saved(&after);
    assert_ne!(after_bytes, before_bytes);
    ws.undo().expect("one undo");
    assert!(ws.project() == &before, "{edit:?}: project undo");
    assert_eq!(saved(ws.project()), before_bytes, "{edit:?}: bytes undo");
    ws.redo().expect("one redo");
    assert!(ws.project() == &after, "{edit:?}: project redo");
    assert_eq!(saved(ws.project()), after_bytes, "{edit:?}: bytes redo");
}

#[test]
fn typed_fragment_edits_have_exact_loaded_undo_and_provenance() {
    let (mut ws, i) = loaded();
    let new = TreeId::new_v7();
    let edit = Edit::NewFragment {
        id: new,
        root: "New fragment".into(),
    };
    assert_eq!(edit.created(), Some(Selection::Tree(new)));
    exact(&mut ws, &edit);
    let idx = ws.project().attack_tree_idx(&new).expect("new");
    assert_eq!(ws.project().attack_tree_role(idx), Some(TreeRole::Fragment));
    let t = ws.project().attack_tree(idx).expect("tree");
    assert_eq!(
        t.get(t.root()).expect("root").gate_kind(),
        Some(GateKind::Or)
    );
    for edit in [
        Edit::UseFragment {
            tree: i.first,
            node: i.ordinary,
            fragment: i.a,
        },
        Edit::UnuseFragment {
            tree: i.first,
            node: i.ordinary,
        },
        Edit::RenameFragment {
            tree: i.a,
            name: "Renamed A".into(),
        },
    ] {
        exact(&mut ws, &edit);
    }
    let no_change = Edit::RenameFragment {
        tree: i.a,
        name: " Renamed A ".into(),
    };
    assert!(
        no_change
            .to_ops(ws.project(), "new-editor", 901)
            .expect("unchanged")
            .is_empty()
    );
    assert!(
        matches!(Edit::Batch(vec![no_change]).changing(ws.project()), Edit::Batch(v) if v.is_empty())
    );
    assert_eq!(
        Edit::RenameFragment {
            tree: i.first,
            name: "Wrong role".into()
        }
        .to_ops(ws.project(), "new-editor", 901),
        Err(EditError::Missing),
    );
}

#[test]
fn loaded_reference_structure_edits_restore_bytes_and_links() {
    for case in 0..7 {
        let (mut ws, i) = loaded();
        let child = NodeId::new_v7();
        let edit = match case {
            0 => Edit::RemoveTreeNode {
                tree: i.first,
                node: i.user_a,
            },
            1 => Edit::RemoveTree { tree: i.first },
            2 => Edit::ReplaceTree {
                tree: i.first,
                outline: "OR First\n  Ordinary\n".into(),
            },
            3 => Edit::BranchStepToGate {
                tree: i.first,
                node: i.user_a,
                id: child,
                gate: GateKind::And,
                child_label: "Reference child".into(),
            },
            4 => Edit::MoveTreeNode {
                tree: i.first,
                node: i.user_a,
                delta: -1,
            },
            5 => Edit::RemoveTreeNode {
                tree: i.first,
                node: i.ordinary,
            },
            _ => Edit::InsertParentGate {
                tree: i.first,
                node: i.user_a,
                id: child,
                gate: GateKind::And,
                label: "Parent".into(),
            },
        };
        exact(&mut ws, &edit);
        if case >= 3 {
            let node = if case == 3 { child } else { i.user_a };
            let t = ws.project().attack_tree_idx(&i.first).expect("tree");
            let n = ws
                .project()
                .attack_tree(t)
                .expect("tree")
                .idx_of(&node)
                .expect("node");
            assert_eq!(ws.project().step_fragment_id(t, n), Some(i.a));
            assert_eq!(ws.project().step_condition_link_ids(i.first, node).len(), 1);
            assert_eq!(ws.project().step_mitigations(t, n).len(), 1);
            assert!(ws.project().review_marked(i.first, node));
        }
    }
}

#[test]
fn protected_deletion_and_invalid_attachment_are_atomic() {
    let (mut ws, i) = loaded();
    for edit in [
        Edit::RemoveTree { tree: i.a },
        Edit::UseFragment {
            tree: i.first,
            node: i.shared,
            fragment: i.a,
        },
        Edit::UseFragment {
            tree: i.first,
            node: i.ordinary,
            fragment: i.first,
        },
    ] {
        let before = ws.project().clone();
        let bytes = saved(&before);
        let ops = edit
            .to_ops(ws.project(), "new-editor", 900)
            .expect("typed ops");
        assert!(ws.apply(ops, Origin::Edit).is_err(), "{edit:?}");
        assert!(ws.project() == &before);
        assert_eq!(saved(ws.project()), bytes);
        assert!(!ws.can_undo());
    }
    let level = ws
        .project()
        .methodology()
        .feasibility_level("low")
        .expect("low");
    ws.apply(
        vec![aegis_model::Op::SetStepFeasibility {
            target: aegis_model::StepTarget::Step(i.first, i.ordinary),
            level: Some(level),
            provenance: aegis_model::Provenance::human("rater", 71),
        }],
        Origin::Edit,
    )
    .expect("rate");
    let before = saved(ws.project());
    let ops = Edit::UseFragment {
        tree: i.first,
        node: i.ordinary,
        fragment: i.a,
    }
    .to_ops(ws.project(), "new-editor", 900)
    .expect("ops");
    assert_eq!(ops.len(), 1, "attachment never clears ratings");
    assert!(ws.apply(ops, Origin::Edit).is_err());
    assert_eq!(saved(ws.project()), before);
}

#[test]
fn direct_users_and_threat_candidates_are_independently_named() {
    let (ws, i) = loaded();
    let mut expected = [(i.first, i.user_a), (i.second, i.user_b), (i.b, i.user_c)];
    expected.sort();
    assert_eq!(
        related(
            ws.project(),
            RelationKey::FRAGMENT_USED_IN,
            Selection::Tree(i.a)
        ),
        expected
            .into_iter()
            .map(|(t, n)| Selection::TreeNode(t, n))
            .collect::<Vec<_>>(),
    );
    assert_eq!(
        candidates(ws.project(), RelationKey::THREAT_TREE),
        vec![Selection::Tree(i.first), Selection::Tree(i.second)]
    );
    for tree in [i.unused, i.first, TreeId::new_v7()] {
        assert!(
            related(
                ws.project(),
                RelationKey::FRAGMENT_USED_IN,
                Selection::Tree(tree)
            )
            .is_empty()
        );
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum FoldOutcome {
    Value(FeasibilityLevel),
    Excluded,
    Unrated,
    EmptyGate,
}

/// Independent input-only folding over raw `Project` attack-tree DAGs,
/// shared-step fields, conditions, applicability filter, and control ceilings.
/// Does not invoke `Project::evaluate_tree`, `Project::evaluate_residual_tree`,
/// or `aegis_client::fragments`.
struct IndependentFold<'a> {
    project: &'a Project,
    memo: HashMap<(TreeId, bool), FoldOutcome>,
    active: HashSet<TreeId>,
}

impl<'a> IndependentFold<'a> {
    fn new(project: &'a Project) -> Self {
        Self {
            project,
            memo: HashMap::new(),
            active: HashSet::new(),
        }
    }

    fn tree_excluded(&self, tree: TreeId) -> bool {
        self.project
            .condition_link_ids(ElementRef::Tree(tree))
            .iter()
            .any(|&c_idx| self.project.condition(c_idx).is_some_and(|c| !c.present()))
    }

    fn item_excluded(&self, item: ItemRef) -> bool {
        let mut elem = match item {
            ItemRef::Component(id) => Some(ElementRef::Component(id)),
            ItemRef::Interface(id) => Some(ElementRef::Interface(id)),
        };
        while let Some(e) = elem {
            if self
                .project
                .condition_link_ids(e)
                .iter()
                .any(|&c_idx| self.project.condition(c_idx).is_some_and(|c| !c.present()))
            {
                return true;
            }
            elem = match e {
                ElementRef::Interface(id) => self
                    .project
                    .interface_idx(&id)
                    .and_then(|idx| self.project.interface(idx))
                    .and_then(|i| self.project.component_id(i.component()))
                    .map(ElementRef::Component),
                ElementRef::Component(id) => self
                    .project
                    .component_idx(&id)
                    .and_then(|idx| self.project.component(idx))
                    .and_then(|c| c.parent())
                    .and_then(|p| self.project.component_id(p))
                    .map(ElementRef::Component),
                _ => None,
            };
        }
        false
    }

    fn fold_tree(&mut self, tree_id: TreeId, residual: bool) -> FoldOutcome {
        if let Some(cached) = self.memo.get(&(tree_id, residual)) {
            return cached.clone();
        }
        if !self.active.insert(tree_id) {
            return FoldOutcome::Unrated;
        }
        let outcome = if self.tree_excluded(tree_id) {
            FoldOutcome::Excluded
        } else if let Some(t_idx) = self.project.attack_tree_idx(&tree_id)
            && let Some(tree) = self.project.attack_tree(t_idx)
        {
            let mut node_memo = HashMap::new();
            self.fold_node(tree_id, t_idx, tree.root(), residual, &mut node_memo)
        } else {
            FoldOutcome::Unrated
        };
        self.active.remove(&tree_id);
        self.memo.insert((tree_id, residual), outcome.clone());
        outcome
    }

    fn fold_node(
        &mut self,
        tree_id: TreeId,
        t_idx: TreeIdx,
        n_idx: NodeIdx,
        residual: bool,
        node_memo: &mut HashMap<NodeIdx, FoldOutcome>,
    ) -> FoldOutcome {
        if let Some(cached) = node_memo.get(&n_idx) {
            return cached.clone();
        }
        let tree = self.project.attack_tree(t_idx).expect("live tree");
        let node = tree.get(n_idx).expect("live node");
        let out = match node.gate_kind() {
            Some(kind) => {
                let children = node.children();
                if children.is_empty() {
                    FoldOutcome::EmptyGate
                } else {
                    let mut vals = Vec::new();
                    let mut early = None;
                    for &c in children {
                        match self.fold_node(tree_id, t_idx, c, residual, node_memo) {
                            FoldOutcome::EmptyGate => {
                                early = Some(FoldOutcome::EmptyGate);
                                break;
                            }
                            FoldOutcome::Unrated => {
                                early = Some(FoldOutcome::Unrated);
                                break;
                            }
                            FoldOutcome::Excluded => {
                                if kind != GateKind::Or && early.is_none() {
                                    early = Some(FoldOutcome::Excluded);
                                }
                            }
                            FoldOutcome::Value(v) => vals.push(v),
                        }
                    }
                    if let Some(stop) = early {
                        stop
                    } else if vals.is_empty() {
                        FoldOutcome::Excluded
                    } else {
                        let v = match kind {
                            GateKind::Or => vals.into_iter().max().expect("non-empty"),
                            GateKind::And | GateKind::SeqAnd => {
                                vals.into_iter().min().expect("non-empty")
                            }
                        };
                        FoldOutcome::Value(v)
                    }
                }
            }
            None => self.fold_step(tree_id, t_idx, n_idx, residual),
        };
        node_memo.insert(n_idx, out.clone());
        out
    }

    fn fold_step(
        &mut self,
        tree_id: TreeId,
        t_idx: TreeIdx,
        n_idx: NodeIdx,
        residual: bool,
    ) -> FoldOutcome {
        let tree = self.project.attack_tree(t_idx).expect("live tree");
        let n_id = tree.id_of(n_idx).expect("node id");
        let step_cond_off = self
            .project
            .step_condition_link_ids(tree_id, n_id)
            .iter()
            .any(|&c_idx| self.project.condition(c_idx).is_some_and(|c| !c.present()));
        let filter = self.project.attack_filter();
        let factor_off = self
            .project
            .step_factors(t_idx, n_idx)
            .any(|(f, l)| filter.is_excluded(f, l));
        let via_off = self
            .project
            .step_via(t_idx, n_idx)
            .is_some_and(|item| self.item_excluded(item));
        if step_cond_off || factor_off || via_off {
            return FoldOutcome::Excluded;
        }

        let base = if let Some(frag_id) = self.project.step_fragment_id(t_idx, n_idx) {
            match self.fold_tree(frag_id, residual) {
                FoldOutcome::Value(v) => v,
                FoldOutcome::Excluded => return FoldOutcome::Excluded,
                FoldOutcome::Unrated | FoldOutcome::EmptyGate => return FoldOutcome::Unrated,
            }
        } else if let Some(shared_id) = self.project.step_shared_id(t_idx, n_idx) {
            let s_idx = self.project.shared_step_idx(&shared_id).expect("shared");
            let shared = self.project.shared_step(s_idx).expect("shared");
            let m = self.project.methodology();
            let resolved = m
                .resolve_attack_potential(shared.factor_slice())
                .map(|ap| ap.feasibility())
                .or(shared.feasibility());
            let Some(v) = resolved else {
                return FoldOutcome::Unrated;
            };
            v
        } else {
            let m = self.project.methodology();
            let mut factor_vec = vec![None; m.feasibility_factors().len()];
            for (f, l) in self.project.step_factors(t_idx, n_idx) {
                factor_vec[f.index()] = Some(l);
            }
            let resolved = m
                .resolve_attack_potential(&factor_vec)
                .map(|ap| ap.feasibility())
                .or_else(|| self.project.step_feasibility(t_idx, n_idx));
            let Some(v) = resolved else {
                return FoldOutcome::Unrated;
            };
            v
        };

        let counts = |status: ControlStatus| {
            matches!(status, ControlStatus::Implemented)
                || (residual && matches!(status, ControlStatus::Planned))
        };
        let mut value = base;
        for mit in self.project.step_mitigations(t_idx, n_idx) {
            let ctrl = self.project.security_control(mit.control()).expect("ctrl");
            if counts(ctrl.status()) {
                value = value.min(mit.feasibility());
            }
        }
        if let (Some(via), Some(attack)) = (
            self.project.step_via(t_idx, n_idx),
            self.project.step_attack_type(t_idx, n_idx),
        ) {
            let owner = match via {
                ItemRef::Interface(id) => self
                    .project
                    .interface_idx(&id)
                    .and_then(|idx| self.project.interface(idx))
                    .and_then(|i| self.project.component_id(i.component()))
                    .map(ItemRef::Component),
                ItemRef::Component(_) => None,
            };
            for (c_idx, _, ctrl) in self.project.security_controls() {
                if counts(ctrl.status())
                    && self
                        .project
                        .control_targets(c_idx)
                        .any(|t| t == via || Some(t) == owner)
                    && let Some(ceiling) = ctrl.rule_for(attack)
                {
                    value = value.min(ceiling);
                }
            }
        }
        FoldOutcome::Value(value)
    }
}

fn project_fold(project: &Project, tree: TreeId, residual: bool) -> FoldOutcome {
    let res = if residual {
        project.evaluate_residual_tree(tree)
    } else {
        project.evaluate_tree(tree)
    };
    match res {
        Ok(eval) => FoldOutcome::Value(*eval.root_value()),
        Err(ModelError::TreeNotApplicable(_)) => FoldOutcome::Excluded,
        Err(ModelError::EmptyGate(_)) => FoldOutcome::EmptyGate,
        Err(ModelError::MissingLeafValue(_)) => FoldOutcome::Unrated,
        Err(other) => panic!("unexpected eval error on {tree}: {other}"),
    }
}

fn assert_fold_matches(project: &Project, left: TreeId, right: TreeId) {
    let mut oracle = IndependentFold::new(project);
    for residual in [false, true] {
        let a = oracle.fold_tree(left, residual);
        let b = oracle.fold_tree(right, residual);
        assert_eq!(a, b, "independent fold mismatch (residual={residual})");
        assert_eq!(
            project_fold(project, left, residual),
            a,
            "model vs oracle mismatch on left (residual={residual})"
        );
        assert_eq!(
            project_fold(project, right, residual),
            b,
            "model vs oracle mismatch on right (residual={residual})"
        );
    }
}

fn apply_checked_undo_redo(ws: &mut Workspace, ops: Vec<Op>) {
    let before = ws.project().clone();
    let before_bytes = saved(&before);
    ws.apply(ops, Origin::Edit).expect("batch apply");
    let after = ws.project().clone();
    let after_bytes = saved(&after);
    assert_ne!(after_bytes, before_bytes);
    ws.undo().expect("undo");
    assert!(ws.project() == &before);
    assert_eq!(saved(ws.project()), before_bytes);
    ws.redo().expect("redo");
    assert!(ws.project() == &after);
    assert_eq!(saved(ws.project()), after_bytes);
}

fn add_tree_skeleton_ops(role: TreeRole, id: TreeId, tree: &AttackTree, p: &Provenance) -> Vec<Op> {
    let r = tree.root();
    let r_id = tree.id_of(r).expect("root id");
    let r_node = tree.get(r).expect("root node");
    let root_only = AttackTree::new(
        r_id,
        match r_node.gate_kind() {
            Some(k) => Node::gate(k, r_node.label()),
            None => Node::step(r_node.label()),
        },
    )
    .expect("root");
    let mut ops = vec![Op::AddAttackTree {
        role,
        id,
        tree: root_only,
        step_feasibility: Vec::new(),
        provenance: p.clone(),
    }];
    let mut owned_counts: HashMap<NodeIdx, u32> = HashMap::new();
    for (idx, nid, node) in tree.iter() {
        if idx == r {
            continue;
        }
        let owner = tree.owner(idx).expect("owner");
        let owner_id = tree.id_of(owner).expect("owner id");
        let pos = owned_counts.entry(owner).or_insert(0);
        ops.push(Op::AddTreeNode {
            tree: id,
            parent: owner_id,
            id: nid,
            node: match node.gate_kind() {
                Some(k) => Node::gate(k, node.label()),
                None => Node::step(node.label()),
            },
            index: u32::try_from(idx.index()).expect("u32"),
            position: *pos,
            provenance: p.clone(),
        });
        *pos += 1;
    }
    ops
}

fn clear_step_for_insert_ops(
    project: &Project,
    tree: TreeId,
    node: NodeId,
    p: &Provenance,
) -> Vec<Op> {
    let t_idx = project.attack_tree_idx(&tree).expect("tree");
    let t = project.attack_tree(t_idx).expect("tree");
    let n_idx = t.idx_of(&node).expect("node");
    let mut ops = Vec::new();
    if project.review_marked(tree, node) {
        ops.push(Op::SetReviewMark {
            tree,
            node,
            marked: false,
            provenance: p.clone(),
        });
    }
    if project.step_fragment_id(t_idx, n_idx).is_some() {
        ops.push(Op::UnuseFragment {
            tree,
            node,
            provenance: p.clone(),
        });
    }
    if project.step_shared_id(t_idx, n_idx).is_some() {
        ops.push(Op::UnuseSharedStep {
            tree,
            node,
            provenance: p.clone(),
        });
    } else {
        if project.step_feasibility(t_idx, n_idx).is_some() {
            ops.push(Op::SetStepFeasibility {
                target: StepTarget::Step(tree, node),
                level: None,
                provenance: p.clone(),
            });
        }
        for (factor, _) in project.step_factors(t_idx, n_idx) {
            ops.push(Op::SetStepFactor {
                target: StepTarget::Step(tree, node),
                factor,
                level: None,
                provenance: p.clone(),
            });
        }
        if project.step_via(t_idx, n_idx).is_some() {
            ops.push(Op::SetStepVia {
                target: StepTarget::Step(tree, node),
                via: None,
                provenance: p.clone(),
            });
        }
        if project.step_attack_type(t_idx, n_idx).is_some() {
            ops.push(Op::SetStepAttackType {
                target: StepTarget::Step(tree, node),
                attack_type: None,
                provenance: p.clone(),
            });
        }
    }
    for m in project.step_mitigations(t_idx, n_idx) {
        if let Some(ctrl) = project.security_control_id(m.control()) {
            ops.push(Op::SetStepMitigation {
                tree,
                node,
                control: ctrl,
                feasibility: None,
                provenance: p.clone(),
            });
        }
    }
    for &c_idx in project.step_condition_link_ids(tree, node) {
        if let Some(cond) = project.condition_id(c_idx) {
            ops.push(Op::UnneedStepCondition {
                tree,
                node,
                condition: cond,
                provenance: p.clone(),
            });
        }
    }
    ops
}

#[test]
#[allow(clippy::too_many_lines)]
fn copy_subtree_over_32_nodes_preserve_and_materialize_fold_and_roundtrip() {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let mut ws = Workspace::empty(Arc::clone(&m));
    let p = Provenance::human("copy-author", 1001);

    let very_low = m.feasibility_levels().next().expect("very_low").0;
    let low = m.feasibility_level("low").expect("low");
    let med = m.feasibility_level("medium").expect("medium");
    let high = m.feasibility_level("high").expect("high");
    assert!(very_low < low && low < med && med < high);
    let atk_type = m.attack_types().next().expect("attack_type");
    let iface_kind = m.interface_kinds().next().expect("iface_kind");

    let comp_id = ComponentId::new_v7();
    let iface_id = InterfaceId::new_v7();
    let asset_id = AssetId::new_v7();
    let threat_id = ThreatId::new_v7();
    let shared_id = SharedStepId::new_v7();

    let c_tree_inner = ConditionId::new_v7();
    let c_occ1 = ConditionId::new_v7();
    let c_occ2 = ConditionId::new_v7();
    let c_host_tree = ConditionId::new_v7();

    let ctrl_impl = ControlId::new_v7();
    let ctrl_plan = ControlId::new_v7();
    let ctrl_off = ControlId::new_v7();
    let ctrl_rule = ControlId::new_v7();

    let mut setup = vec![
        Op::AddComponent {
            id: comp_id,
            name: "Gateway".into(),
            parent: None,
            provenance: p.clone(),
        },
        Op::AddInterface {
            id: iface_id,
            name: "CAN0".into(),
            component: comp_id,
            kind: iface_kind,
            provenance: p.clone(),
        },
        Op::AddAsset {
            id: asset_id,
            name: "Bus".into(),
            properties: m.all_properties(),
            provenance: p.clone(),
        },
        Op::AddThreatScenario {
            id: threat_id,
            name: "Spoof".into(),
            asset: asset_id,
            category: m.threat_categories().next().expect("cat"),
            catalog_refs: vec![],
            damages: vec![],
            provenance: p.clone(),
        },
        Op::AddSharedStep {
            id: shared_id,
            name: "Shared CAN inject".into(),
            provenance: p.clone(),
        },
        Op::SetDescription {
            target: ElementRef::SharedStep(shared_id),
            text: "Source shared description".into(),
            provenance: p.clone(),
        },
        Op::SetStepVia {
            target: StepTarget::Shared(shared_id),
            via: Some(ItemRef::Interface(iface_id)),
            provenance: p.clone(),
        },
        Op::SetStepAttackType {
            target: StepTarget::Shared(shared_id),
            attack_type: Some(atk_type),
            provenance: p.clone(),
        },
        Op::AddCondition {
            id: c_tree_inner,
            name: "Inner tree cond".into(),
            description: "".into(),
            present: true,
            provenance: p.clone(),
        },
        Op::AddCondition {
            id: c_occ1,
            name: "Occ 1 cond".into(),
            description: "".into(),
            present: true,
            provenance: p.clone(),
        },
        Op::AddCondition {
            id: c_occ2,
            name: "Occ 2 cond".into(),
            description: "".into(),
            present: true,
            provenance: p.clone(),
        },
        Op::AddCondition {
            id: c_host_tree,
            name: "Host tree cond".into(),
            description: "".into(),
            present: true,
            provenance: p.clone(),
        },
        Op::AddSecurityControl {
            id: ctrl_impl,
            name: "Implemented ctrl".into(),
            provenance: p.clone(),
        },
        Op::SetControlStatus {
            control: ctrl_impl,
            status: ControlStatus::Implemented,
            provenance: p.clone(),
        },
        Op::AddSecurityControl {
            id: ctrl_plan,
            name: "Planned ctrl".into(),
            provenance: p.clone(),
        },
        Op::SetControlStatus {
            control: ctrl_plan,
            status: ControlStatus::Planned,
            provenance: p.clone(),
        },
        Op::AddSecurityControl {
            id: ctrl_off,
            name: "Off ctrl".into(),
            provenance: p.clone(),
        },
        Op::SetControlStatus {
            control: ctrl_off,
            status: ControlStatus::Off,
            provenance: p.clone(),
        },
        Op::AddSecurityControl {
            id: ctrl_rule,
            name: "Rule ctrl".into(),
            provenance: p.clone(),
        },
        Op::SetControlStatus {
            control: ctrl_rule,
            status: ControlStatus::Planned,
            provenance: p.clone(),
        },
        Op::ApplyControl {
            control: ctrl_rule,
            target: ItemRef::Component(comp_id),
            provenance: p.clone(),
        },
        Op::SetControlRule {
            control: ctrl_rule,
            attack_type: atk_type,
            ceiling: Some(low),
            provenance: p.clone(),
        },
    ];
    let mut tracked_factor = None;
    let mut tracked_level = None;
    for factor in m.feasibility_factors() {
        let lvl = m.factor_levels(factor).next().expect("factor level");
        if tracked_factor.is_none() {
            tracked_factor = Some(factor);
            tracked_level = Some(lvl);
        }
        setup.push(Op::SetStepFactor {
            target: StepTarget::Shared(shared_id),
            factor,
            level: Some(lvl),
            provenance: p.clone(),
        });
    }
    let (tracked_factor, tracked_level) = (tracked_factor.expect("f"), tracked_level.expect("l"));
    ws.apply(setup, Origin::Edit).expect("setup");

    // Build inner fragment with SEQ gate and internal diamond sharing a SharedStep leaf.
    let f_inner_id = TreeId::new_v7();
    let fi_root = NodeId::new_v7();
    let fi_a = NodeId::new_v7();
    let fi_b = NodeId::new_v7();
    let fi_shared_leaf = NodeId::new_v7();
    let fi_direct_leaf = NodeId::new_v7();
    let mut f_inner = AttackTree::new(fi_root, Node::gate(GateKind::SeqAnd, "Inner SEQ")).unwrap();
    f_inner
        .add_child(f_inner.root(), fi_a, Node::gate(GateKind::Or, "Branch A"))
        .unwrap();
    f_inner
        .add_child(f_inner.root(), fi_b, Node::gate(GateKind::Or, "Branch B"))
        .unwrap();
    let fi_a_idx = f_inner.idx_of(&fi_a).unwrap();
    let fi_b_idx = f_inner.idx_of(&fi_b).unwrap();
    let shared_leaf_idx = f_inner
        .add_child(fi_a_idx, fi_shared_leaf, Node::step("Inner shared leaf"))
        .unwrap();
    f_inner
        .add_child(fi_b_idx, fi_direct_leaf, Node::step("Inner direct leaf"))
        .unwrap();
    f_inner.link(fi_b_idx, shared_leaf_idx).unwrap();

    // Build mid fragment referencing F_inner twice with different overlays.
    let f_mid_id = TreeId::new_v7();
    let fm_root = NodeId::new_v7();
    let fm_occ1 = NodeId::new_v7();
    let fm_occ2 = NodeId::new_v7();
    let mut f_mid = AttackTree::new(fm_root, Node::gate(GateKind::Or, "Mid OR")).unwrap();
    f_mid
        .add_child(f_mid.root(), fm_occ1, Node::step("Occ 1"))
        .unwrap();
    f_mid
        .add_child(f_mid.root(), fm_occ2, Node::step("Occ 2"))
        .unwrap();

    // Build host threat tree with >32 own nodes, AND/OR/SEQ, internal diamond, and repeated refs.
    let host_id = TreeId::new_v7();
    let h_root = NodeId::new_v7();
    let h_and = NodeId::new_v7();
    let h_seq = NodeId::new_v7();
    let h_mid1 = NodeId::new_v7();
    let h_mid2 = NodeId::new_v7();
    let h_direct_inner = NodeId::new_v7();
    let h_diamond_a = NodeId::new_v7();
    let h_diamond_b = NodeId::new_v7();
    let h_diamond_shared = NodeId::new_v7();
    let mut host = AttackTree::new(h_root, Node::gate(GateKind::Or, "Host OR")).unwrap();
    host.add_child(host.root(), h_and, Node::gate(GateKind::And, "Host AND"))
        .unwrap();
    host.add_child(host.root(), h_seq, Node::gate(GateKind::SeqAnd, "Host SEQ"))
        .unwrap();
    let h_and_idx = host.idx_of(&h_and).unwrap();
    let h_seq_idx = host.idx_of(&h_seq).unwrap();
    host.add_child(h_and_idx, h_mid1, Node::step("Host ref Mid 1"))
        .unwrap();
    host.add_child(h_seq_idx, h_mid2, Node::step("Host ref Mid 2"))
        .unwrap();
    host.add_child(h_and_idx, h_direct_inner, Node::step("Host ref Inner"))
        .unwrap();
    host.add_child(
        h_and_idx,
        h_diamond_a,
        Node::gate(GateKind::Or, "Diamond A"),
    )
    .unwrap();
    host.add_child(
        h_seq_idx,
        h_diamond_b,
        Node::gate(GateKind::Or, "Diamond B"),
    )
    .unwrap();
    let da_idx = host.idx_of(&h_diamond_a).unwrap();
    let db_idx = host.idx_of(&h_diamond_b).unwrap();
    let ds_idx = host
        .add_child(da_idx, h_diamond_shared, Node::step("Host diamond leaf"))
        .unwrap();
    host.link(db_idx, ds_idx).unwrap();
    let mut extra_host_steps = Vec::new();
    for k in 0..28 {
        let s = NodeId::new_v7();
        host.add_child(host.root(), s, Node::step(format!("Ordinary step {k}")))
            .unwrap();
        extra_host_steps.push(s);
    }
    assert!(host.len() > 32);

    let mut build_ops = vec![
        Op::AddAttackTree {
            role: TreeRole::Fragment,
            id: f_inner_id,
            tree: f_inner,
            step_feasibility: vec![(fi_direct_leaf, high)],
            provenance: p.clone(),
        },
        Op::AddAttackTree {
            role: TreeRole::Fragment,
            id: f_mid_id,
            tree: f_mid,
            step_feasibility: vec![],
            provenance: p.clone(),
        },
    ];
    build_ops.extend(add_tree_skeleton_ops(TreeRole::Threat, host_id, &host, &p));
    build_ops.extend([
        Op::LinkTreeNode {
            tree: host_id,
            parent: h_diamond_b,
            child: h_diamond_shared,
            position: 0,
            slot: 0,
            provenance: p.clone(),
        },
        Op::SetThreatAttackTree {
            threat: threat_id,
            tree: Some(host_id),
            provenance: p.clone(),
        },
        Op::SetDescription {
            target: ElementRef::Tree(host_id),
            text: "Host description".into(),
            provenance: p.clone(),
        },
        Op::SetDescription {
            target: ElementRef::Tree(f_inner_id),
            text: "Inner fragment description".into(),
            provenance: p.clone(),
        },
        Op::NeedCondition {
            element: ElementRef::Tree(f_inner_id),
            condition: c_tree_inner,
            provenance: p.clone(),
        },
        Op::NeedCondition {
            element: ElementRef::Tree(host_id),
            condition: c_host_tree,
            provenance: p.clone(),
        },
        Op::UseSharedStep {
            tree: f_inner_id,
            node: fi_shared_leaf,
            step: shared_id,
            provenance: p.clone(),
        },
        Op::SetStepMitigation {
            tree: f_inner_id,
            node: fi_direct_leaf,
            control: ctrl_impl,
            feasibility: Some(high),
            provenance: p.clone(),
        },
        Op::UseFragment {
            tree: f_mid_id,
            node: fm_occ1,
            fragment: f_inner_id,
            provenance: p.clone(),
        },
        Op::UseFragment {
            tree: f_mid_id,
            node: fm_occ2,
            fragment: f_inner_id,
            provenance: p.clone(),
        },
        Op::NeedStepCondition {
            tree: f_mid_id,
            node: fm_occ1,
            condition: c_occ1,
            provenance: p.clone(),
        },
        Op::SetStepMitigation {
            tree: f_mid_id,
            node: fm_occ1,
            control: ctrl_plan,
            feasibility: Some(low),
            provenance: p.clone(),
        },
        Op::SetReviewMark {
            tree: f_mid_id,
            node: fm_occ1,
            marked: true,
            provenance: p.clone(),
        },
        Op::NeedStepCondition {
            tree: f_mid_id,
            node: fm_occ2,
            condition: c_occ2,
            provenance: p.clone(),
        },
        Op::SetStepMitigation {
            tree: f_mid_id,
            node: fm_occ2,
            control: ctrl_impl,
            feasibility: Some(med),
            provenance: p.clone(),
        },
        Op::UseFragment {
            tree: host_id,
            node: h_mid1,
            fragment: f_mid_id,
            provenance: p.clone(),
        },
        Op::UseFragment {
            tree: host_id,
            node: h_mid2,
            fragment: f_mid_id,
            provenance: p.clone(),
        },
        Op::UseFragment {
            tree: host_id,
            node: h_direct_inner,
            fragment: f_inner_id,
            provenance: p.clone(),
        },
        Op::SetStepMitigation {
            tree: host_id,
            node: h_mid1,
            control: ctrl_impl,
            feasibility: Some(high),
            provenance: p.clone(),
        },
        Op::SetStepMitigation {
            tree: host_id,
            node: h_mid2,
            control: ctrl_off,
            feasibility: Some(very_low),
            provenance: p.clone(),
        },
        Op::SetStepFeasibility {
            target: StepTarget::Step(host_id, h_diamond_shared),
            level: Some(med),
            provenance: p.clone(),
        },
    ]);
    for s in extra_host_steps {
        build_ops.push(Op::SetStepFeasibility {
            target: StepTarget::Step(host_id, s),
            level: Some(low),
            provenance: p.clone(),
        });
    }
    ws.apply(build_ops, Origin::Edit).expect("build fixture");

    // 1. PreserveReferences into a new >32-node Threat tree in one Workspace batch.
    let plan_preserve =
        copy_subtree(ws.project(), host_id, h_root, CopyMode::PreserveReferences).expect("plan");
    assert!(plan_preserve.tree().len() > 32);
    let preserve_id = TreeId::new_v7();
    let preserve_ops = plan_preserve
        .new_tree_ops(preserve_id, TreeRole::Threat, &p)
        .expect("new_tree_ops");
    apply_checked_undo_redo(&mut ws, preserve_ops);
    assert_fold_matches(ws.project(), host_id, preserve_id);
    assert_eq!(
        ws.project().description(ElementRef::Tree(preserve_id)),
        None
    );
    assert!(
        ws.project()
            .condition_link_ids(ElementRef::Tree(preserve_id))
            .is_empty()
    );
    let p_idx = ws.project().attack_tree_idx(&preserve_id).unwrap();
    let p_tree = ws.project().attack_tree(p_idx).unwrap();
    let p_frag_count = p_tree
        .iter()
        .filter(|(idx, _, _)| ws.project().step_fragment_id(p_idx, *idx).is_some())
        .count();
    assert_eq!(p_frag_count, 3);

    // 2. Materialize into a new >32-node Threat tree in one Workspace batch.
    let plan_mat =
        copy_subtree(ws.project(), host_id, h_root, CopyMode::Materialize).expect("plan mat");
    assert!(plan_mat.tree().len() > 32);
    let mat_id = TreeId::new_v7();
    let mat_ops = plan_mat
        .new_tree_ops(mat_id, TreeRole::Threat, &p)
        .expect("new_tree_ops mat");
    apply_checked_undo_redo(&mut ws, mat_ops);
    assert_fold_matches(ws.project(), host_id, mat_id);

    // Zero fragment or shared slots remain in materialized tree, source descriptions preserved.
    let m_idx = ws.project().attack_tree_idx(&mat_id).unwrap();
    let m_tree = ws.project().attack_tree(m_idx).unwrap();
    let mut occ1_leaves = 0usize;
    let mut occ2_leaves = 0usize;
    let mut shared_diamonds = 0usize;
    let c_tree_inner_idx = ws.project().condition_idx(&c_tree_inner).unwrap();
    let c_occ1_idx = ws.project().condition_idx(&c_occ1).unwrap();
    let c_occ2_idx = ws.project().condition_idx(&c_occ2).unwrap();
    let c_host_idx = ws.project().condition_idx(&c_host_tree).unwrap();
    let ctrl_impl_idx = ws.project().security_control_idx(&ctrl_impl).unwrap();
    for (idx, nid, node) in m_tree.iter() {
        assert_eq!(ws.project().step_fragment_id(m_idx, idx), None);
        assert_eq!(ws.project().step_shared_id(m_idx, idx), None);
        if node.label() == "Inner shared leaf" && m_tree.parents(idx).count() == 2 {
            shared_diamonds += 1;
        }
        if node.gate_kind().is_none() {
            let conds = ws.project().step_condition_link_ids(mat_id, nid);
            assert!(!conds.contains(&c_host_idx));
            if node.label() == "Inner direct leaf" {
                assert!(conds.contains(&c_tree_inner_idx));
                if conds.contains(&c_occ1_idx) {
                    occ1_leaves += 1;
                    assert!(ws.project().review_marked(mat_id, nid));
                }
                if conds.contains(&c_occ2_idx) {
                    occ2_leaves += 1;
                    assert!(!ws.project().review_marked(mat_id, nid));
                    // Merged ctrl_impl ceiling from high & med -> min(high, med) == med.
                    let mit = ws
                        .project()
                        .step_mitigations(m_idx, idx)
                        .iter()
                        .find(|m| m.control() == ctrl_impl_idx)
                        .expect("ctrl_impl merged");
                    assert_eq!(mit.feasibility(), med);
                }
            }
        }
    }
    assert_eq!(occ1_leaves, 2, "two distinct occurrences of Mid -> Occ 1");
    assert_eq!(occ2_leaves, 2, "two distinct occurrences of Mid -> Occ 2");
    assert_eq!(
        shared_diamonds, 5,
        "all 5 occurrences keep internal diamond"
    );
    for (idx, _, node) in m_tree.iter() {
        if node.label() == "Inner SEQ" {
            let labels: Vec<_> = m_tree
                .children(idx)
                .iter()
                .map(|&c| m_tree.get(c).unwrap().label())
                .collect();
            assert_eq!(labels, vec!["Branch A", "Branch B"]);
        }
        if node.label() == "Host SEQ" {
            let labels: Vec<_> = m_tree
                .children(idx)
                .iter()
                .map(|&c| m_tree.get(c).unwrap().label())
                .collect();
            assert_eq!(labels, vec!["Mid OR", "Diamond B"]);
        }
    }
    assert_eq!(
        ws.project().description(ElementRef::Tree(host_id)),
        Some("Host description")
    );
    assert_eq!(
        ws.project().description(ElementRef::Tree(f_inner_id)),
        Some("Inner fragment description")
    );
    assert_eq!(
        ws.project().description(ElementRef::SharedStep(shared_id)),
        Some("Source shared description")
    );
    assert_eq!(ws.project().description(ElementRef::Tree(mat_id)), None);

    // 3. Dynamic condition / control-status / attack-filter changes apply identically after Materialize!
    for op in [
        Op::SetConditionPresent {
            id: c_occ1,
            present: false,
            provenance: p.clone(),
        },
        Op::SetControlStatus {
            control: ctrl_plan,
            status: ControlStatus::Implemented,
            provenance: p.clone(),
        },
        Op::SetControlStatus {
            control: ctrl_off,
            status: ControlStatus::Planned,
            provenance: p.clone(),
        },
        Op::SetControlStatus {
            control: ctrl_rule,
            status: ControlStatus::Implemented,
            provenance: p.clone(),
        },
        Op::SetAttackFilter {
            filter: AttackFilter::default().without(tracked_factor, tracked_level),
            provenance: p.clone(),
        },
        Op::SetConditionPresent {
            id: c_tree_inner,
            present: false,
            provenance: p.clone(),
        },
    ] {
        ws.apply(vec![op], Origin::Edit).expect("mutation");
        assert_fold_matches(ws.project(), host_id, mat_id);
        assert_fold_matches(ws.project(), host_id, preserve_id);
    }

    // Host tree-level condition stays on host only (toggling it off excludes host, not copies).
    ws.apply(
        vec![Op::SetConditionPresent {
            id: c_tree_inner,
            present: true,
            provenance: p.clone(),
        }],
        Origin::Edit,
    )
    .expect("restore c_tree_inner");
    ws.apply(
        vec![Op::SetConditionPresent {
            id: c_host_tree,
            present: false,
            provenance: p.clone(),
        }],
        Origin::Edit,
    )
    .expect("host tree cond off");
    assert_eq!(
        project_fold(ws.project(), host_id, false),
        FoldOutcome::Excluded
    );
    assert_ne!(
        project_fold(ws.project(), mat_id, false),
        FoldOutcome::Excluded
    );
    assert_ne!(
        project_fold(ws.project(), preserve_id, false),
        FoldOutcome::Excluded
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn insert_ops_gate_step_roots_and_interior_diamond_owner_promotion() {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let mut ws = Workspace::empty(Arc::clone(&m));
    let p = Provenance::human("insert-author", 2002);
    let low = m.feasibility_level("low").expect("low");
    let med = m.feasibility_level("medium").expect("medium");
    let high = m.feasibility_level("high").expect("high");

    let shared_id = SharedStepId::new_v7();
    let cond_local = ConditionId::new_v7();
    let cond_frag = ConditionId::new_v7();
    let ctrl = ControlId::new_v7();

    // 1-step fragment backed by a SharedStep.
    let f_step_id = TreeId::new_v7();
    let fs_root = NodeId::new_v7();
    let f_step = AttackTree::new(fs_root, Node::step("Frag step root")).unwrap();

    // Multi-node diamond fragment with gate root.
    let f_gate_id = TreeId::new_v7();
    let fg_root = NodeId::new_v7();
    let fg_a = NodeId::new_v7();
    let fg_b = NodeId::new_v7();
    let fg_d = NodeId::new_v7();
    let mut f_gate = AttackTree::new(fg_root, Node::gate(GateKind::And, "Frag gate")).unwrap();
    f_gate
        .add_child(f_gate.root(), fg_a, Node::gate(GateKind::Or, "FG A"))
        .unwrap();
    f_gate
        .add_child(f_gate.root(), fg_b, Node::gate(GateKind::Or, "FG B"))
        .unwrap();
    let fg_a_idx = f_gate.idx_of(&fg_a).unwrap();
    let fg_b_idx = f_gate.idx_of(&fg_b).unwrap();
    let fg_d_idx = f_gate
        .add_child(fg_a_idx, fg_d, Node::step("FG shared"))
        .unwrap();
    f_gate.link(fg_b_idx, fg_d_idx).unwrap();

    // Host tree where u_gate is shared by two gates and u_step is a 1-step ref.
    let host_id = TreeId::new_v7();
    let h_root = NodeId::new_v7();
    let h_left = NodeId::new_v7();
    let h_right = NodeId::new_v7();
    let u_gate = NodeId::new_v7();
    let u_step = NodeId::new_v7();
    let h_ext_leaf = NodeId::new_v7();
    let mut host = AttackTree::new(h_root, Node::gate(GateKind::Or, "Host")).unwrap();
    host.add_child(host.root(), h_left, Node::gate(GateKind::Or, "Left"))
        .unwrap();
    host.add_child(host.root(), h_right, Node::gate(GateKind::And, "Right"))
        .unwrap();
    let hl = host.idx_of(&h_left).unwrap();
    let hr = host.idx_of(&h_right).unwrap();
    let ug = host
        .add_child(hl, u_gate, Node::step("Keep gate label"))
        .unwrap();
    host.link(hr, ug).unwrap();
    host.add_child(hr, u_step, Node::step("Keep step label"))
        .unwrap();
    let ext = host
        .add_child(host.root(), h_ext_leaf, Node::step("External owned"))
        .unwrap();
    host.link(hl, ext).unwrap();

    ws.apply(
        vec![
            Op::AddSharedStep {
                id: shared_id,
                name: "Shared leaf".into(),
                provenance: p.clone(),
            },
            Op::SetStepFeasibility {
                target: StepTarget::Shared(shared_id),
                level: Some(high),
                provenance: p.clone(),
            },
            Op::AddCondition {
                id: cond_local,
                name: "Local cond".into(),
                description: "".into(),
                present: true,
                provenance: p.clone(),
            },
            Op::AddCondition {
                id: cond_frag,
                name: "Frag cond".into(),
                description: "".into(),
                present: true,
                provenance: p.clone(),
            },
            Op::AddSecurityControl {
                id: ctrl,
                name: "Control".into(),
                provenance: p.clone(),
            },
            Op::AddAttackTree {
                role: TreeRole::Fragment,
                id: f_step_id,
                tree: f_step,
                step_feasibility: vec![],
                provenance: p.clone(),
            },
            Op::AddAttackTree {
                role: TreeRole::Fragment,
                id: f_gate_id,
                tree: f_gate,
                step_feasibility: vec![(fg_d, med)],
                provenance: p.clone(),
            },
            Op::AddAttackTree {
                role: TreeRole::Threat,
                id: host_id,
                tree: host,
                step_feasibility: vec![(h_ext_leaf, low)],
                provenance: p.clone(),
            },
            Op::UseSharedStep {
                tree: f_step_id,
                node: fs_root,
                step: shared_id,
                provenance: p.clone(),
            },
            Op::SetReviewMark {
                tree: f_step_id,
                node: fs_root,
                marked: true,
                provenance: p.clone(),
            },
            Op::NeedCondition {
                element: ElementRef::Tree(f_gate_id),
                condition: cond_frag,
                provenance: p.clone(),
            },
            Op::UseFragment {
                tree: host_id,
                node: u_gate,
                fragment: f_gate_id,
                provenance: p.clone(),
            },
            Op::UseFragment {
                tree: host_id,
                node: u_step,
                fragment: f_step_id,
                provenance: p.clone(),
            },
            Op::NeedStepCondition {
                tree: host_id,
                node: u_gate,
                condition: cond_local,
                provenance: p.clone(),
            },
            Op::SetStepMitigation {
                tree: host_id,
                node: u_step,
                control: ctrl,
                feasibility: Some(med),
                provenance: p.clone(),
            },
        ],
        Origin::Edit,
    )
    .expect("setup");

    let mut baseline_oracle = IndependentFold::new(ws.project());
    let before_init = baseline_oracle.fold_tree(host_id, false);
    let before_res = baseline_oracle.fold_tree(host_id, true);

    // 1. Insert Gate-root copy into cleared u_gate (which keeps its label, NodeId, and 2 parents!).
    let gate_plan =
        copy_subtree(ws.project(), host_id, u_gate, CopyMode::Materialize).expect("gate plan");
    assert_eq!(gate_plan.root_kind(), Some(GateKind::And));
    let start_idx = ws
        .project()
        .attack_tree(ws.project().attack_tree_idx(&host_id).unwrap())
        .unwrap()
        .len();
    let mut gate_batch = clear_step_for_insert_ops(ws.project(), host_id, u_gate, &p);
    gate_batch.push(Op::SetTreeNodeKind {
        tree: host_id,
        id: u_gate,
        gate: gate_plan.root_kind(),
        provenance: p.clone(),
    });
    gate_batch.extend(
        gate_plan
            .insert_ops(host_id, u_gate, start_idx, &p)
            .expect("insert_ops gate"),
    );
    apply_checked_undo_redo(&mut ws, gate_batch);

    let h_idx = ws.project().attack_tree_idx(&host_id).unwrap();
    let h_tree = ws.project().attack_tree(h_idx).unwrap();
    let ug_idx = h_tree.idx_of(&u_gate).unwrap();
    assert_eq!(h_tree.get(ug_idx).unwrap().label(), "Keep gate label");
    assert_eq!(h_tree.parents(ug_idx).count(), 2);
    assert_eq!(ws.project().step_fragment_id(h_idx, ug_idx), None);

    // 2. Insert Step-root copy into cleared u_step (retains label & ID, gets SharedStep rating + review mark + mitigation).
    let step_plan =
        copy_subtree(ws.project(), host_id, u_step, CopyMode::Materialize).expect("step plan");
    assert_eq!(step_plan.root_kind(), None);
    let start_idx = ws
        .project()
        .attack_tree(ws.project().attack_tree_idx(&host_id).unwrap())
        .unwrap()
        .len();
    let mut step_batch = clear_step_for_insert_ops(ws.project(), host_id, u_step, &p);
    step_batch.extend(
        step_plan
            .insert_ops(host_id, u_step, start_idx, &p)
            .expect("insert_ops step"),
    );
    apply_checked_undo_redo(&mut ws, step_batch);

    let h_idx = ws.project().attack_tree_idx(&host_id).unwrap();
    let h_tree = ws.project().attack_tree(h_idx).unwrap();
    let us_idx = h_tree.idx_of(&u_step).unwrap();
    assert_eq!(h_tree.get(us_idx).unwrap().label(), "Keep step label");
    assert_eq!(ws.project().step_fragment_id(h_idx, us_idx), None);
    assert_eq!(ws.project().step_shared_id(h_idx, us_idx), None);
    assert_eq!(ws.project().step_feasibility(h_idx, us_idx), Some(high));
    assert!(ws.project().review_marked(host_id, u_step));
    assert_eq!(ws.project().step_mitigations(h_idx, us_idx).len(), 1);

    let mut after_oracle = IndependentFold::new(ws.project());
    assert_eq!(after_oracle.fold_tree(host_id, false), before_init);
    assert_eq!(after_oracle.fold_tree(host_id, true), before_res);
    assert_eq!(project_fold(ws.project(), host_id, false), before_init);
    assert_eq!(project_fold(ws.project(), host_id, true), before_res);

    // 3. Extracting interior gate h_left promotes h_ext_leaf (originally owned by host root) to be owned inside copy!
    let sub_plan = copy_subtree(ws.project(), host_id, h_left, CopyMode::PreserveReferences)
        .expect("interior copy");
    let sub_id = TreeId::new_v7();
    let sub_ops = sub_plan
        .new_tree_ops(sub_id, TreeRole::Fragment, &p)
        .expect("new_tree_ops");
    apply_checked_undo_redo(&mut ws, sub_ops);
    let s_idx = ws.project().attack_tree_idx(&sub_id).unwrap();
    let s_tree = ws.project().attack_tree(s_idx).unwrap();
    assert!(s_tree.iter().any(|(_, _, n)| n.label() == "External owned"));
}

#[test]
fn empty_unrated_and_excluded_roots_preserve_structure_without_fallback() {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let mut ws = Workspace::empty(Arc::clone(&m));
    let p = Provenance::human("edge-author", 3003);
    let med = m.feasibility_level("medium").expect("medium");
    let cond_off = ConditionId::new_v7();

    let f_empty_id = TreeId::new_v7();
    let f_unrated_id = TreeId::new_v7();
    let f_excl_id = TreeId::new_v7();
    let host_or_id = TreeId::new_v7();
    let host_and_id = TreeId::new_v7();
    let host_empty_id = TreeId::new_v7();
    let host_unrated_id = TreeId::new_v7();

    let (u_excl_or, rated_sib, u_excl_and, u_empty_ref, u_unrated_ref) = (
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
    );

    let mut host_or = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "OR")).unwrap();
    host_or
        .add_child(host_or.root(), u_excl_or, Node::step("Excl ref"))
        .unwrap();
    host_or
        .add_child(host_or.root(), rated_sib, Node::step("Rated"))
        .unwrap();

    let mut host_and = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::And, "AND")).unwrap();
    host_and
        .add_child(host_and.root(), u_excl_and, Node::step("Excl ref"))
        .unwrap();

    let host_empty = AttackTree::new(u_empty_ref, Node::step("Empty ref")).unwrap();
    let host_unrated = AttackTree::new(u_unrated_ref, Node::step("Unrated ref")).unwrap();

    ws.apply(
        vec![
            Op::AddCondition {
                id: cond_off,
                name: "Off".into(),
                description: "".into(),
                present: false,
                provenance: p.clone(),
            },
            Op::AddAttackTree {
                role: TreeRole::Fragment,
                id: f_empty_id,
                tree: AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "Empty")).unwrap(),
                step_feasibility: vec![],
                provenance: p.clone(),
            },
            Op::AddAttackTree {
                role: TreeRole::Fragment,
                id: f_unrated_id,
                tree: AttackTree::new(NodeId::new_v7(), Node::step("Unrated leaf")).unwrap(),
                step_feasibility: vec![],
                provenance: p.clone(),
            },
            Op::AddAttackTree {
                role: TreeRole::Fragment,
                id: f_excl_id,
                tree: AttackTree::new(NodeId::new_v7(), Node::step("Excluded leaf")).unwrap(),
                step_feasibility: vec![],
                provenance: p.clone(),
            },
            Op::NeedCondition {
                element: ElementRef::Tree(f_excl_id),
                condition: cond_off,
                provenance: p.clone(),
            },
            Op::AddAttackTree {
                role: TreeRole::Threat,
                id: host_or_id,
                tree: host_or,
                step_feasibility: vec![(rated_sib, med)],
                provenance: p.clone(),
            },
            Op::AddAttackTree {
                role: TreeRole::Threat,
                id: host_and_id,
                tree: host_and,
                step_feasibility: vec![],
                provenance: p.clone(),
            },
            Op::AddAttackTree {
                role: TreeRole::Threat,
                id: host_empty_id,
                tree: host_empty,
                step_feasibility: vec![],
                provenance: p.clone(),
            },
            Op::AddAttackTree {
                role: TreeRole::Threat,
                id: host_unrated_id,
                tree: host_unrated,
                step_feasibility: vec![],
                provenance: p.clone(),
            },
            Op::UseFragment {
                tree: host_or_id,
                node: u_excl_or,
                fragment: f_excl_id,
                provenance: p.clone(),
            },
            Op::UseFragment {
                tree: host_and_id,
                node: u_excl_and,
                fragment: f_excl_id,
                provenance: p.clone(),
            },
            Op::UseFragment {
                tree: host_empty_id,
                node: u_empty_ref,
                fragment: f_empty_id,
                provenance: p.clone(),
            },
            Op::UseFragment {
                tree: host_unrated_id,
                node: u_unrated_ref,
                fragment: f_unrated_id,
                provenance: p.clone(),
            },
        ],
        Origin::Edit,
    )
    .expect("setup");

    for src in [host_or_id, host_and_id, host_empty_id, host_unrated_id] {
        let s_idx = ws.project().attack_tree_idx(&src).unwrap();
        let s_root = ws
            .project()
            .attack_tree(s_idx)
            .and_then(|t| t.id_of(t.root()))
            .unwrap();
        let plan = copy_subtree(ws.project(), src, s_root, CopyMode::Materialize).expect("plan");
        let dst = TreeId::new_v7();
        let ops = plan.new_tree_ops(dst, TreeRole::Threat, &p).expect("ops");
        apply_checked_undo_redo(&mut ws, ops);
        if src == host_empty_id {
            // Reference step over an empty-gate fragment folds as Unrated before Materialize,
            // and Materialize preserves the empty gate itself without inventing a child/rating.
            let mut oracle = IndependentFold::new(ws.project());
            for residual in [false, true] {
                assert_eq!(oracle.fold_tree(src, residual), FoldOutcome::Unrated);
                assert_eq!(
                    project_fold(ws.project(), src, residual),
                    FoldOutcome::Unrated
                );
                assert_eq!(oracle.fold_tree(dst, residual), FoldOutcome::EmptyGate);
                assert_eq!(
                    project_fold(ws.project(), dst, residual),
                    FoldOutcome::EmptyGate
                );
            }
            let d_idx = ws.project().attack_tree_idx(&dst).unwrap();
            let d_tree = ws.project().attack_tree(d_idx).unwrap();
            let root_node = d_tree.get(d_tree.root()).unwrap();
            assert_eq!(root_node.gate_kind(), Some(GateKind::Or));
            assert!(root_node.children().is_empty());
        } else {
            assert_fold_matches(ws.project(), src, dst);
        }
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn copy_cap_overflow_preflights_cleanly_and_leaves_project_unchanged() {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let mut ws = Workspace::empty(Arc::clone(&m));
    let p = Provenance::human("cap-author", 4004);
    let low = m.feasibility_level("low").expect("low");
    let med = m.feasibility_level("medium").expect("medium");

    // 1. Exponential node expansion (> MAX_ATTACK_TREE_NODES = 4096) within depth 6 <= MAX_FRAGMENT_DEPTH.
    let mut frag_ids = Vec::new();
    let leaf_frag = TreeId::new_v7();
    let mut ops = vec![Op::AddAttackTree {
        role: TreeRole::Fragment,
        id: leaf_frag,
        tree: AttackTree::new(NodeId::new_v7(), Node::step("Base")).unwrap(),
        step_feasibility: vec![],
        provenance: p.clone(),
    }];
    frag_ids.push(leaf_frag);

    for level in 1..=6 {
        let tid = TreeId::new_v7();
        let fanout = if level == 6 { 5 } else { 4 };
        let mut t = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "Fan")).unwrap();
        let mut children = Vec::new();
        for k in 0..fanout {
            let cid = NodeId::new_v7();
            t.add_child(t.root(), cid, Node::step(format!("C{k}")))
                .unwrap();
            children.push(cid);
        }
        ops.push(Op::AddAttackTree {
            role: TreeRole::Fragment,
            id: tid,
            tree: t,
            step_feasibility: vec![],
            provenance: p.clone(),
        });
        let target = *frag_ids.last().unwrap();
        for cid in children {
            ops.push(Op::UseFragment {
                tree: tid,
                node: cid,
                fragment: target,
                provenance: p.clone(),
            });
        }
        frag_ids.push(tid);
    }
    let top_frag = *frag_ids.last().unwrap();

    // 2. Merged condition & mitigation bounds (16 ok with overlap, 17 overflows).
    let mut conds = Vec::new();
    for i in 0..17 {
        let cid = ConditionId::new_v7();
        ops.push(Op::AddCondition {
            id: cid,
            name: format!("Cond {i}").into(),
            description: "".into(),
            present: true,
            provenance: p.clone(),
        });
        conds.push(cid);
    }
    let mut ctrls = Vec::new();
    for i in 0..17 {
        let cid = ControlId::new_v7();
        ops.push(Op::AddSecurityControl {
            id: cid,
            name: format!("Ctrl {i}").into(),
            provenance: p.clone(),
        });
        ctrls.push(cid);
    }

    let f_meta_id = TreeId::new_v7();
    let fm_leaf = NodeId::new_v7();
    ops.push(Op::AddAttackTree {
        role: TreeRole::Fragment,
        id: f_meta_id,
        tree: AttackTree::new(fm_leaf, Node::step("Meta leaf")).unwrap(),
        step_feasibility: vec![(fm_leaf, med)],
        provenance: p.clone(),
    });
    for &c in &conds[..10] {
        ops.push(Op::NeedCondition {
            element: ElementRef::Tree(f_meta_id),
            condition: c,
            provenance: p.clone(),
        });
    }
    for &ctrl in &ctrls[..10] {
        ops.push(Op::SetStepMitigation {
            tree: f_meta_id,
            node: fm_leaf,
            control: ctrl,
            feasibility: Some(med),
            provenance: p.clone(),
        });
    }

    let host_id = TreeId::new_v7();
    let h_root = NodeId::new_v7();
    let h_cond_over = NodeId::new_v7();
    let h_cond_ok = NodeId::new_v7();
    let h_mit_over = NodeId::new_v7();
    let h_mit_ok = NodeId::new_v7();
    let mut host = AttackTree::new(h_root, Node::gate(GateKind::Or, "Host")).unwrap();
    for id in [h_cond_over, h_cond_ok, h_mit_over, h_mit_ok] {
        host.add_child(host.root(), id, Node::step("Ref")).unwrap();
    }
    ops.push(Op::AddAttackTree {
        role: TreeRole::Threat,
        id: host_id,
        tree: host,
        step_feasibility: vec![],
        provenance: p.clone(),
    });
    for id in [h_cond_over, h_cond_ok, h_mit_over, h_mit_ok] {
        ops.push(Op::UseFragment {
            tree: host_id,
            node: id,
            fragment: f_meta_id,
            provenance: p.clone(),
        });
    }
    for &c in &conds[10..17] {
        ops.push(Op::NeedStepCondition {
            tree: host_id,
            node: h_cond_over,
            condition: c,
            provenance: p.clone(),
        });
    }
    // 10 on f_meta_id + 10 on h_cond_ok with 4 overlapping = 16 distinct -> allowed!
    for &c in &conds[6..16] {
        ops.push(Op::NeedStepCondition {
            tree: host_id,
            node: h_cond_ok,
            condition: c,
            provenance: p.clone(),
        });
    }
    for &ctrl in &ctrls[10..17] {
        ops.push(Op::SetStepMitigation {
            tree: host_id,
            node: h_mit_over,
            control: ctrl,
            feasibility: Some(low),
            provenance: p.clone(),
        });
    }
    // 10 on f_meta_id + 10 on h_mit_ok with 4 overlapping = 16 distinct -> allowed!
    for &ctrl in &ctrls[6..16] {
        ops.push(Op::SetStepMitigation {
            tree: host_id,
            node: h_mit_ok,
            control: ctrl,
            feasibility: Some(low),
            provenance: p.clone(),
        });
    }

    ws.apply(ops, Origin::Edit).expect("setup caps");
    let ws = Workspace::new(ws.project().clone(), 0);
    let before = ws.project().clone();
    let before_bytes = saved(&before);

    let top_root = {
        let idx = ws.project().attack_tree_idx(&top_frag).unwrap();
        let t = ws.project().attack_tree(idx).unwrap();
        t.id_of(t.root()).unwrap()
    };
    assert_eq!(
        copy_subtree(ws.project(), top_frag, top_root, CopyMode::Materialize),
        Err(EditError::FragmentCopyLimit {
            resource: "nodes",
            max: MAX_ATTACK_TREE_NODES,
        })
    );
    assert_eq!(
        copy_subtree(ws.project(), host_id, h_cond_over, CopyMode::Materialize),
        Err(EditError::FragmentCopyLimit {
            resource: "step_conditions",
            max: MAX_CONDITIONS_PER_ELEMENT,
        })
    );
    assert_eq!(
        copy_subtree(ws.project(), host_id, h_mit_over, CopyMode::Materialize),
        Err(EditError::FragmentCopyLimit {
            resource: "step_mitigations",
            max: MAX_STEP_MITIGATIONS,
        })
    );
    assert_eq!(
        copy_subtree(
            ws.project(),
            TreeId::new_v7(),
            h_cond_ok,
            CopyMode::Materialize
        ),
        Err(EditError::Missing)
    );
    assert_eq!(
        copy_subtree(
            ws.project(),
            host_id,
            NodeId::new_v7(),
            CopyMode::Materialize
        ),
        Err(EditError::Missing)
    );

    // Overlapping 16-distinct cases succeed!
    assert!(copy_subtree(ws.project(), host_id, h_cond_ok, CopyMode::Materialize).is_ok());
    assert!(copy_subtree(ws.project(), host_id, h_mit_ok, CopyMode::Materialize).is_ok());

    // insert_ops also refuses exceeding MAX_ATTACK_TREE_NODES.
    let small_plan =
        copy_subtree(ws.project(), host_id, h_root, CopyMode::PreserveReferences).unwrap();
    assert_eq!(
        small_plan.insert_ops(host_id, h_root, MAX_ATTACK_TREE_NODES, &p),
        Err(EditError::FragmentCopyLimit {
            resource: "nodes",
            max: MAX_ATTACK_TREE_NODES,
        })
    );

    // Project and undo stack remain completely untouched after all overflow refusals.
    assert!(ws.project() == &before);
    assert_eq!(saved(ws.project()), before_bytes);
    assert!(!ws.can_undo());
}

#[test]
#[allow(clippy::too_many_lines)]
fn seq_diamond_disjoint_ids_exact_depth_eight_and_ref_cap_rollback() {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let mut ws = Workspace::empty(Arc::clone(&m));
    let p = Provenance::human("edge-seq", 5005);
    let low = m.feasibility_level("low").expect("low");
    let high = m.feasibility_level("high").expect("high");
    let factor = m.feasibility_factors().next().expect("factor");
    let flevel = m.factor_levels(factor).next().expect("factor level");
    let atk_type = m.attack_types().next().expect("attack_type");
    let iface_kind = m.interface_kinds().next().expect("iface_kind");

    let comp_id = ComponentId::new_v7();
    let iface_id = InterfaceId::new_v7();
    let ctrl_id = ControlId::new_v7();
    let frag_step_id = TreeId::new_v7();
    let frag_root = NodeId::new_v7();
    let src_id = TreeId::new_v7();
    let src_root = NodeId::new_v7();
    let g_b = NodeId::new_v7();
    let g_seq = NodeId::new_v7();
    let g_c = NodeId::new_v7();
    let diamond = NodeId::new_v7();
    let owned1 = NodeId::new_v7();
    let owned2 = NodeId::new_v7();
    let c_leaf = NodeId::new_v7();

    let mut src_tree = AttackTree::new(src_root, Node::gate(GateKind::Or, "Top OR")).unwrap();
    let src_root_idx = src_tree.root();
    src_tree
        .add_child(src_root_idx, g_b, Node::gate(GateKind::And, "OwnerB"))
        .unwrap();
    src_tree
        .add_child(src_root_idx, g_seq, Node::gate(GateKind::SeqAnd, "SeqS"))
        .unwrap();
    src_tree
        .add_child(src_root_idx, g_c, Node::gate(GateKind::And, "ParentC"))
        .unwrap();
    let gb_idx = src_tree.idx_of(&g_b).unwrap();
    let gs_idx = src_tree.idx_of(&g_seq).unwrap();
    let gc_idx = src_tree.idx_of(&g_c).unwrap();
    src_tree
        .add_child(gb_idx, diamond, Node::step("Diamond"))
        .unwrap();
    src_tree
        .add_child(gs_idx, owned1, Node::step("Owned1"))
        .unwrap();
    src_tree
        .add_child(gs_idx, owned2, Node::step("Owned2"))
        .unwrap();
    src_tree
        .add_child(gc_idx, c_leaf, Node::step("CLeaf"))
        .unwrap();
    let d_idx = src_tree.idx_of(&diamond).unwrap();
    src_tree.link_at(gs_idx, d_idx, 1, 0).unwrap();
    src_tree.link_at(gc_idx, d_idx, 1, 1).unwrap();

    // Target step host for PreserveReferences insert_ops check.
    let target_host_id = TreeId::new_v7();
    let th_root = NodeId::new_v7();
    let th_slot = NodeId::new_v7();
    let mut th_tree = AttackTree::new(th_root, Node::gate(GateKind::Or, "DestRoot")).unwrap();
    let th_root_idx = th_tree.root();
    th_tree
        .add_child(th_root_idx, th_slot, Node::step("DestSlot"))
        .unwrap();

    let mut setup = vec![
        Op::AddComponent {
            id: comp_id,
            name: "ECU".into(),
            parent: None,
            provenance: p.clone(),
        },
        Op::AddInterface {
            id: iface_id,
            name: "ETH0".into(),
            component: comp_id,
            kind: iface_kind,
            provenance: p.clone(),
        },
        Op::AddSecurityControl {
            id: ctrl_id,
            name: "Filter".into(),
            provenance: p.clone(),
        },
        Op::SetControlStatus {
            control: ctrl_id,
            status: ControlStatus::Implemented,
            provenance: p.clone(),
        },
        Op::AddAttackTree {
            role: TreeRole::Fragment,
            id: frag_step_id,
            tree: AttackTree::new(frag_root, Node::step("Raw frag step")).unwrap(),
            step_feasibility: vec![(frag_root, high)],
            provenance: p.clone(),
        },
        Op::AddAttackTree {
            role: TreeRole::Threat,
            id: src_id,
            tree: src_tree,
            step_feasibility: vec![(owned2, low), (c_leaf, low)],
            provenance: p.clone(),
        },
        Op::AddAttackTree {
            role: TreeRole::Threat,
            id: target_host_id,
            tree: th_tree,
            step_feasibility: vec![],
            provenance: p.clone(),
        },
        Op::UseFragment {
            tree: src_id,
            node: diamond,
            fragment: frag_step_id,
            provenance: p.clone(),
        },
        Op::SetStepMitigation {
            tree: src_id,
            node: diamond,
            control: ctrl_id,
            feasibility: Some(low),
            provenance: p.clone(),
        },
        Op::SetStepFactor {
            target: StepTarget::Step(src_id, owned1),
            factor,
            level: Some(flevel),
            provenance: p.clone(),
        },
        Op::SetStepVia {
            target: StepTarget::Step(src_id, owned1),
            via: Some(ItemRef::Interface(iface_id)),
            provenance: p.clone(),
        },
        Op::SetStepAttackType {
            target: StepTarget::Step(src_id, owned1),
            attack_type: Some(atk_type),
            provenance: p.clone(),
        },
    ];

    // Exact depth-8 fragment chain (F8 <- F7 <- ... <- F1 <- depth8_host) vs depth-9 host!
    let mut chain = Vec::new();
    let f_leaf = TreeId::new_v7();
    let f_leaf_n = NodeId::new_v7();
    setup.push(Op::AddAttackTree {
        role: TreeRole::Fragment,
        id: f_leaf,
        tree: AttackTree::new(f_leaf_n, Node::step("ChainLeaf")).unwrap(),
        step_feasibility: vec![(f_leaf_n, high)],
        provenance: p.clone(),
    });
    chain.push(f_leaf);
    for d in 2..=MAX_FRAGMENT_DEPTH {
        let tid = TreeId::new_v7();
        let rn = NodeId::new_v7();
        setup.push(Op::AddAttackTree {
            role: TreeRole::Fragment,
            id: tid,
            tree: AttackTree::new(rn, Node::step(format!("F{d}"))).unwrap(),
            step_feasibility: vec![],
            provenance: p.clone(),
        });
        setup.push(Op::UseFragment {
            tree: tid,
            node: rn,
            fragment: *chain.last().unwrap(),
            provenance: p.clone(),
        });
        chain.push(tid);
    }
    let depth8_host = TreeId::new_v7();
    let d8_n = NodeId::new_v7();
    setup.push(Op::AddAttackTree {
        role: TreeRole::Threat,
        id: depth8_host,
        tree: AttackTree::new(d8_n, Node::step("Depth8Host")).unwrap(),
        step_feasibility: vec![],
        provenance: p.clone(),
    });
    setup.push(Op::UseFragment {
        tree: depth8_host,
        node: d8_n,
        fragment: *chain.last().unwrap(),
        provenance: p.clone(),
    });

    // 150 fragment refs in one threat tree (< 256) so duplicating it exceeds 256 in Workspace::apply.
    let big_ref_tree_id = TreeId::new_v7();
    let big_root = NodeId::new_v7();
    let mut big_tree = AttackTree::new(big_root, Node::gate(GateKind::Or, "BigRefs")).unwrap();
    let mut big_kids = Vec::new();
    for i in 0..150 {
        let kid = NodeId::new_v7();
        big_tree
            .add_child(big_tree.root(), kid, Node::step(format!("R{i}")))
            .unwrap();
        big_kids.push(kid);
    }
    for batch in add_tree_skeleton_ops(TreeRole::Threat, big_ref_tree_id, &big_tree, &p).chunks(400)
    {
        ws.apply(batch.to_vec(), Origin::Edit).expect("big tree");
    }
    for kid in big_kids {
        setup.push(Op::UseFragment {
            tree: big_ref_tree_id,
            node: kid,
            fragment: frag_step_id,
            provenance: p.clone(),
        });
    }
    ws.apply(setup, Origin::Edit).expect("setup");

    // A. Disjoint fresh IDs across source and repeated plans.
    let src_ids: HashSet<NodeId> = ws
        .project()
        .attack_tree(ws.project().attack_tree_idx(&src_id).unwrap())
        .unwrap()
        .iter()
        .map(|(_, id, _)| id)
        .collect();
    let p1 = copy_subtree(ws.project(), src_id, src_root, CopyMode::PreserveReferences).unwrap();
    let p2 = copy_subtree(ws.project(), src_id, src_root, CopyMode::PreserveReferences).unwrap();
    let pm = copy_subtree(ws.project(), src_id, src_root, CopyMode::Materialize).unwrap();
    let ids1: HashSet<NodeId> = p1.tree().iter().map(|(_, id, _)| id).collect();
    let ids2: HashSet<NodeId> = p2.tree().iter().map(|(_, id, _)| id).collect();
    let idsm: HashSet<NodeId> = pm.tree().iter().map(|(_, id, _)| id).collect();
    assert!(src_ids.is_disjoint(&ids1));
    assert!(src_ids.is_disjoint(&ids2));
    assert!(src_ids.is_disjoint(&idsm));
    assert!(ids1.is_disjoint(&ids2));
    assert!(ids1.is_disjoint(&idsm));
    assert!(ids2.is_disjoint(&idsm));

    // B. SEQ child order & diamond parent order + own factors/via/attack_type + raw step_feasibility.
    let mat_id = TreeId::new_v7();
    apply_checked_undo_redo(
        &mut ws,
        pm.new_tree_ops(mat_id, TreeRole::Threat, &p).unwrap(),
    );
    assert_fold_matches(ws.project(), src_id, mat_id);

    let m_idx = ws.project().attack_tree_idx(&mat_id).unwrap();
    let m_tree = ws.project().attack_tree(m_idx).unwrap();
    let ctrl_idx = ws.project().security_control_idx(&ctrl_id).unwrap();
    for (idx, _, node) in m_tree.iter() {
        if node.label() == "SeqS" {
            let child_labels: Vec<_> = m_tree
                .children(idx)
                .iter()
                .map(|&c| m_tree.get(c).unwrap().label())
                .collect();
            assert_eq!(child_labels, vec!["Owned1", "Raw frag step", "Owned2"]);
        }
        if node.label() == "Raw frag step" {
            let parent_labels: Vec<_> = m_tree
                .parents(idx)
                .map(|p| m_tree.get(p).unwrap().label())
                .collect();
            assert_eq!(parent_labels, vec!["OwnerB", "SeqS", "ParentC"]);
            assert_eq!(ws.project().step_feasibility(m_idx, idx), Some(high));
            assert_eq!(ws.project().step_mitigations(m_idx, idx).len(), 1);
            assert_eq!(
                ws.project().step_mitigations(m_idx, idx)[0].control(),
                ctrl_idx
            );
        }
    }

    // PreserveReferences insert_ops keeps SEQ order, parents order, and own factors/via/attack_type.
    let th_len = ws
        .project()
        .attack_tree(ws.project().attack_tree_idx(&target_host_id).unwrap())
        .unwrap()
        .len();
    let mut ins = clear_step_for_insert_ops(ws.project(), target_host_id, th_slot, &p);
    ins.push(Op::SetTreeNodeKind {
        tree: target_host_id,
        id: th_slot,
        gate: p1.root_kind(),
        provenance: p.clone(),
    });
    ins.extend(p1.insert_ops(target_host_id, th_slot, th_len, &p).unwrap());
    apply_checked_undo_redo(&mut ws, ins);
    assert_fold_matches(ws.project(), src_id, target_host_id);
    let th_idx = ws.project().attack_tree_idx(&target_host_id).unwrap();
    let th_t = ws.project().attack_tree(th_idx).unwrap();
    for (idx, _, node) in th_t.iter() {
        if node.label() == "SeqS" {
            let child_labels: Vec<_> = th_t
                .children(idx)
                .iter()
                .map(|&c| th_t.get(c).unwrap().label())
                .collect();
            assert_eq!(child_labels, vec!["Owned1", "Diamond", "Owned2"]);
        }
        if node.label() == "Diamond" {
            let parent_labels: Vec<_> = th_t
                .parents(idx)
                .map(|p| th_t.get(p).unwrap().label())
                .collect();
            assert_eq!(parent_labels, vec!["OwnerB", "SeqS", "ParentC"]);
        }
        if node.label() == "Owned1" {
            let factors: Vec<_> = ws.project().step_factors(th_idx, idx).collect();
            assert_eq!(factors, vec![(factor, flevel)]);
            assert_eq!(
                ws.project().step_via(th_idx, idx),
                Some(ItemRef::Interface(iface_id))
            );
            assert_eq!(ws.project().step_attack_type(th_idx, idx), Some(atk_type));
        }
    }

    // C. Exact MAX_FRAGMENT_DEPTH == 8 succeeds cleanly in Materialize and folds identically.
    let d8_plan = copy_subtree(ws.project(), depth8_host, d8_n, CopyMode::Materialize)
        .expect("exact depth 8 allowed");
    let d8_copy_id = TreeId::new_v7();
    apply_checked_undo_redo(
        &mut ws,
        d8_plan
            .new_tree_ops(d8_copy_id, TreeRole::Threat, &p)
            .unwrap(),
    );
    assert_fold_matches(ws.project(), depth8_host, d8_copy_id);

    // D. Duplicating a 150-ref subtree into a host already holding 150 refs fails atomically at 256.
    let dst_ref_host = TreeId::new_v7();
    let dr_root = NodeId::new_v7();
    let dr_slot = NodeId::new_v7();
    let mut dr_tree = AttackTree::new(dr_root, Node::gate(GateKind::Or, "RefHost")).unwrap();
    let dr_r = dr_tree.root();
    dr_tree
        .add_child(dr_r, dr_slot, Node::step("Slot"))
        .unwrap();
    let mut dr_kids = Vec::new();
    for i in 0..150 {
        let kid = NodeId::new_v7();
        dr_tree
            .add_child(dr_r, kid, Node::step(format!("H{i}")))
            .unwrap();
        dr_kids.push(kid);
    }
    for batch in add_tree_skeleton_ops(TreeRole::Threat, dst_ref_host, &dr_tree, &p).chunks(400) {
        ws.apply(batch.to_vec(), Origin::Edit).expect("ref host");
    }
    let use_ops: Vec<Op> = dr_kids
        .into_iter()
        .map(|kid| Op::UseFragment {
            tree: dst_ref_host,
            node: kid,
            fragment: frag_step_id,
            provenance: p.clone(),
        })
        .collect();
    ws.apply(use_ops, Origin::Edit).expect("host refs");

    let mut ws = Workspace::new(ws.project().clone(), 0);
    let before = ws.project().clone();
    let before_bytes = saved(&before);
    let big_plan = copy_subtree(
        ws.project(),
        big_ref_tree_id,
        big_root,
        CopyMode::PreserveReferences,
    )
    .expect("within subtree preflight");
    let dr_len = ws
        .project()
        .attack_tree(ws.project().attack_tree_idx(&dst_ref_host).unwrap())
        .unwrap()
        .len();
    let mut big_ops = vec![Op::SetTreeNodeKind {
        tree: dst_ref_host,
        id: dr_slot,
        gate: big_plan.root_kind(),
        provenance: p.clone(),
    }];
    big_ops.extend(
        big_plan
            .insert_ops(dst_ref_host, dr_slot, dr_len, &p)
            .unwrap(),
    );
    let err = ws.apply(big_ops, Origin::Edit).expect_err("over 256 refs");
    assert_eq!(
        err,
        WorkspaceError::Model(ModelError::TooManyFragmentReferences {
            tree: dst_ref_host,
            max: MAX_FRAGMENT_REFERENCES,
        })
    );
    assert!(ws.project() == &before);
    assert_eq!(saved(ws.project()), before_bytes);
    assert!(!ws.can_undo());
}
