//! Typed fragment edits and structure-preserving loaded-project undo.
#[path = "common/fragments.rs"]
mod fragments;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use aegis_client::commands::{Edit, EditError};
use aegis_client::fragments::{CopyMode, copy_subtree};
use aegis_client::i18n::{Catalog, Language};
use aegis_client::relations::{RelationKey, candidates, related};
use aegis_client::view::Selection;
use aegis_client::workspace::{MAX_SUBMIT_OPS, Origin, Workspace, WorkspaceError};
use aegis_model::{
    AssetId, AssumptionId, AttackFilter, AttackTree, ComponentId, ConditionId, ControlId,
    ControlStatus, ElementRef, FeasibilityLevel, GateKind, InterfaceId, ItemRef,
    MAX_ASSUMPTION_EXCLUDES, MAX_ATTACK_TREE_NODES, MAX_CONDITIONS_PER_ELEMENT, MAX_FRAGMENT_DEPTH,
    MAX_FRAGMENT_REFERENCES, MAX_STEP_MITIGATIONS, Methodology, ModelError, Node, NodeId, NodeIdx,
    Op, OpLogReader, OpLogWriter, Project, Provenance, SharedStepId, StepRef, StepTarget, ThreatId,
    TreeId, TreeIdx, TreeRole,
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

    fn assumption_active(&self, aid: AssumptionId, a: &aegis_model::Assumption) -> bool {
        a.holds()
            && !self
                .project
                .condition_link_ids(ElementRef::Assumption(aid))
                .iter()
                .any(|&c_idx| self.project.condition(c_idx).is_some_and(|c| !c.present()))
    }

    fn tree_excluded(&self, tree: TreeId) -> bool {
        let cond_off = self
            .project
            .condition_link_ids(ElementRef::Tree(tree))
            .iter()
            .any(|&c_idx| self.project.condition(c_idx).is_some_and(|c| !c.present()));
        let assumption_off = self.project.attack_tree_idx(&tree).is_some_and(|t_idx| {
            self.project.assumptions().any(|(_, aid, a)| {
                self.assumption_active(aid, a) && a.excludes_trees().contains(&t_idx)
            })
        });
        cond_off || assumption_off
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
        let assumption_off = self.project.assumptions().any(|(_, aid, a)| {
            self.assumption_active(aid, a)
                && a.excludes_steps()
                    .iter()
                    .any(|sr| sr.tree == tree_id && sr.node == n_id)
        });
        if step_cond_off || factor_off || via_off || assumption_off {
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

#[test]
#[allow(clippy::too_many_lines)]
fn cross_gate_linked_sibling_ahead_of_owner_preserves_order_and_folds() {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let mut ws = Workspace::empty(Arc::clone(&m));
    let p = Provenance::human("dag-order", 6006);
    let low = m.feasibility_level("low").expect("low");
    let med = m.feasibility_level("medium").expect("medium");
    let high = m.feasibility_level("high").expect("high");

    let frag_k_id = TreeId::new_v7();
    let fk_root = NodeId::new_v7();

    // 7-node DAG: root -> [Q, O, R], Q -> [K, Y], O owns Z then links K ([Z, K]), R links Z.
    // reachable_topo visits [root, Q, Y, O, K, R, Z], putting K ahead of O in the arena.
    let dag_id = TreeId::new_v7();
    let (n_root, n_q, n_o, n_r, n_k, n_y, n_z) = (
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
    );
    let mut dag = AttackTree::new(n_root, Node::gate(GateKind::Or, "root")).unwrap();
    let r_idx = dag.root();
    let q_idx = dag
        .add_child(r_idx, n_q, Node::gate(GateKind::And, "Q"))
        .unwrap();
    let o_idx = dag
        .add_child(r_idx, n_o, Node::gate(GateKind::SeqAnd, "O"))
        .unwrap();
    let r_gate_idx = dag
        .add_child(r_idx, n_r, Node::gate(GateKind::And, "R"))
        .unwrap();
    let k_idx = dag.add_child(q_idx, n_k, Node::step("K")).unwrap();
    dag.add_child(q_idx, n_y, Node::step("Y")).unwrap();
    let z_idx = dag.add_child(o_idx, n_z, Node::step("Z")).unwrap();
    dag.link(o_idx, k_idx).unwrap();
    dag.link(r_gate_idx, z_idx).unwrap();

    let host_ins_id = TreeId::new_v7();
    let hi_root = NodeId::new_v7();
    let hi_slot = NodeId::new_v7();
    let mut host_ins = AttackTree::new(hi_root, Node::gate(GateKind::Or, "Host")).unwrap();
    host_ins
        .add_child(host_ins.root(), hi_slot, Node::step("Slot"))
        .unwrap();

    ws.apply(
        vec![
            Op::AddAttackTree {
                role: TreeRole::Fragment,
                id: frag_k_id,
                tree: AttackTree::new(fk_root, Node::step("FragK")).unwrap(),
                step_feasibility: vec![(fk_root, high)],
                provenance: p.clone(),
            },
            Op::AddAttackTree {
                role: TreeRole::Threat,
                id: dag_id,
                tree: dag,
                step_feasibility: vec![(n_y, low), (n_z, med)],
                provenance: p.clone(),
            },
            Op::AddAttackTree {
                role: TreeRole::Threat,
                id: host_ins_id,
                tree: host_ins,
                step_feasibility: vec![],
                provenance: p.clone(),
            },
            Op::UseFragment {
                tree: dag_id,
                node: n_k,
                fragment: frag_k_id,
                provenance: p.clone(),
            },
        ],
        Origin::Edit,
    )
    .expect("setup 7-node dag");

    let assert_orders = |project: &Project, tree_id: TreeId, k_label: &str, expect_frag: bool| {
        let t_idx = project.attack_tree_idx(&tree_id).unwrap();
        let t = project.attack_tree(t_idx).unwrap();
        let child_labels = |idx| -> Vec<&str> {
            t.children(idx)
                .iter()
                .map(|&c| t.get(c).unwrap().label())
                .collect()
        };
        let parent_labels = |idx| -> Vec<&str> {
            t.parents(idx)
                .map(|p_idx| t.get(p_idx).unwrap().label())
                .collect()
        };
        for (idx, _, node) in t.iter() {
            match node.label() {
                "root" | "Slot" => assert_eq!(child_labels(idx), vec!["Q", "O", "R"]),
                "Q" => assert_eq!(child_labels(idx), vec![k_label, "Y"]),
                "O" => assert_eq!(child_labels(idx), vec!["Z", k_label]),
                "R" => assert_eq!(child_labels(idx), vec!["Z"]),
                "Z" => assert_eq!(parent_labels(idx), vec!["O", "R"]),
                lbl if lbl == k_label => {
                    assert_eq!(parent_labels(idx), vec!["Q", "O"]);
                    assert_eq!(
                        project.step_fragment_id(t_idx, idx),
                        expect_frag.then_some(frag_k_id)
                    );
                }
                _ => {}
            }
        }
    };

    for (mode, k_label, expect_frag) in [
        (CopyMode::PreserveReferences, "K", true),
        (CopyMode::Materialize, "FragK", false),
    ] {
        let plan = copy_subtree(ws.project(), dag_id, n_root, mode).expect("copy_subtree");
        let dst = TreeId::new_v7();
        apply_checked_undo_redo(
            &mut ws,
            plan.new_tree_ops(dst, TreeRole::Threat, &p).expect("ops"),
        );
        assert_fold_matches(ws.project(), dag_id, dst);
        assert_orders(ws.project(), dst, k_label, expect_frag);

        let start_idx = ws
            .project()
            .attack_tree(ws.project().attack_tree_idx(&host_ins_id).unwrap())
            .unwrap()
            .len();
        let mut ins_ops = vec![Op::SetTreeNodeKind {
            tree: host_ins_id,
            id: hi_slot,
            gate: plan.root_kind(),
            provenance: p.clone(),
        }];
        ins_ops.extend(
            plan.insert_ops(host_ins_id, hi_slot, start_idx, &p)
                .expect("insert_ops"),
        );
        apply_checked_undo_redo(&mut ws, ins_ops);
        assert_fold_matches(ws.project(), dag_id, host_ins_id);
        assert_orders(ws.project(), host_ins_id, k_label, expect_frag);
        ws.undo().expect("reset host_ins_id slot");
    }

    // Also verify out-of-order owned siblings where an earlier-ranked owned child is linked by a
    // descendant of a later-ranked owned sibling (`O -> [C1, C2]`, `C2 -> X`, `X -> C1`).
    let sib_dag_id = TreeId::new_v7();
    let (s_o, s_c1, s_c2, s_x) = (
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
    );
    let mut sib_dag = AttackTree::new(s_o, Node::gate(GateKind::SeqAnd, "O_seq")).unwrap();
    let sc1 = sib_dag
        .add_child(sib_dag.root(), s_c1, Node::step("C1"))
        .unwrap();
    let sc2 = sib_dag
        .add_child(sib_dag.root(), s_c2, Node::gate(GateKind::And, "C2"))
        .unwrap();
    let sx = sib_dag
        .add_child(sc2, s_x, Node::gate(GateKind::Or, "X"))
        .unwrap();
    sib_dag.link(sx, sc1).unwrap();
    ws.apply(
        vec![Op::AddAttackTree {
            role: TreeRole::Threat,
            id: sib_dag_id,
            tree: sib_dag,
            step_feasibility: vec![(s_c1, low)],
            provenance: p.clone(),
        }],
        Origin::Edit,
    )
    .expect("setup sib_dag");
    for mode in [CopyMode::PreserveReferences, CopyMode::Materialize] {
        let plan = copy_subtree(ws.project(), sib_dag_id, s_o, mode).expect("sib copy");
        let dst = TreeId::new_v7();
        apply_checked_undo_redo(
            &mut ws,
            plan.new_tree_ops(dst, TreeRole::Threat, &p)
                .expect("sib ops"),
        );
        assert_fold_matches(ws.project(), sib_dag_id, dst);
        let t_idx = ws.project().attack_tree_idx(&dst).unwrap();
        let t = ws.project().attack_tree(t_idx).unwrap();
        for (idx, _, node) in t.iter() {
            if node.label() == "O_seq" {
                let kids: Vec<_> = t
                    .children(idx)
                    .iter()
                    .map(|&c| t.get(c).unwrap().label())
                    .collect();
                assert_eq!(kids, vec!["C1", "C2"]);
            } else if node.label() == "C1" {
                let ps: Vec<_> = t
                    .parents(idx)
                    .map(|p_idx| t.get(p_idx).unwrap().label())
                    .collect();
                assert_eq!(ps, vec!["O_seq", "X"]);
            }
        }
    }
}

fn eval_outcome_pair(ws: &Workspace, tree: TreeId) -> (FoldOutcome, FoldOutcome) {
    let fold_res = |res: Result<&aegis_model::Evaluation<FeasibilityLevel>, ModelError>| match res {
        Ok(eval) => FoldOutcome::Value(*eval.root_value()),
        Err(ModelError::TreeNotApplicable(_)) => FoldOutcome::Excluded,
        Err(ModelError::EmptyGate(_)) => FoldOutcome::EmptyGate,
        Err(ModelError::MissingLeafValue(_)) => FoldOutcome::Unrated,
        Err(other) => panic!("unexpected cached eval error on {tree}: {other}"),
    };
    let fresh = ws.project().eval_cache();
    let inh = ws.cache().evaluate_tree(ws.project(), tree);
    let res = ws.cache().evaluate_residual_tree(ws.project(), tree);
    assert_eq!(inh, fresh.evaluate_tree(ws.project(), tree));
    assert_eq!(res, fresh.evaluate_residual_tree(ws.project(), tree));
    (fold_res(inh), fold_res(res))
}

fn parent_positions(project: &Project, tree_id: TreeId, node_id: NodeId) -> Vec<(NodeId, usize)> {
    let t_idx = project.attack_tree_idx(&tree_id).expect("tree");
    let tree = project.attack_tree(t_idx).expect("tree");
    let n_idx = tree.idx_of(&node_id).expect("node");
    tree.parents(n_idx)
        .map(|p_idx| {
            let p_id = tree.id_of(p_idx).expect("parent id");
            let pos = tree
                .children(p_idx)
                .iter()
                .position(|&c| c == n_idx)
                .expect("child pos");
            (p_id, pos)
        })
        .collect()
}

fn verify_make_shared_fragment(
    ws: &mut Workspace,
    host_tree: TreeId,
    selected_node: NodeId,
    new_frag_id: TreeId,
) {
    assert!(
        ws.can_undo(),
        "tests must start with a pre-existing undo entry"
    );
    let before_project = ws.project().clone();
    let before_snapshot = saved(&before_project);
    let before_ws_files = before_project
        .to_workspace_files()
        .expect("ws files before");
    let before_undo_count = ws.edits().count();
    let before_prior_label = ws.undo_label().expect("prior undo label").clone();
    let before_cache_eval = eval_outcome_pair(ws, host_tree);
    let before_oracle = {
        let mut oracle = IndependentFold::new(&before_project);
        (
            oracle.fold_tree(host_tree, false),
            oracle.fold_tree(host_tree, true),
        )
    };
    assert_eq!(before_cache_eval, before_oracle);

    let host_idx = before_project.attack_tree_idx(&host_tree).expect("host");
    let host_t = before_project.attack_tree(host_idx).expect("host tree");
    let host_root_id = host_t.id_of(host_t.root()).expect("root id");
    let host_role = before_project.attack_tree_role(host_idx).expect("role");
    let host_vis = before_project.attack_tree_visual_id(host_idx).expect("vis");
    let host_desc = before_project
        .description(ElementRef::Tree(host_tree))
        .map(str::to_owned);
    let host_conds = before_project
        .condition_link_ids(ElementRef::Tree(host_tree))
        .to_vec();
    let host_threats: Vec<_> = before_project
        .threat_scenarios()
        .filter(|(_, _, t)| t.attack_tree() == Some(host_idx))
        .map(|(_, id, _)| id)
        .collect();
    let sel_idx = host_t.idx_of(&selected_node).expect("selected node");
    let sel_label = host_t.get(sel_idx).expect("sel").label().to_owned();
    let sel_parents = parent_positions(&before_project, host_tree, selected_node);
    let before_node_ids: HashSet<NodeId> = host_t.iter().map(|(_, id, _)| id).collect();

    let edit = Edit::MakeSharedFragment {
        tree: host_tree,
        node: selected_node,
        id: new_frag_id,
    };
    assert_eq!(edit.created(), Some(Selection::Tree(new_frag_id)));
    let ops = edit
        .to_ops(ws.project(), "extractor-author", 1_700_000_000_999)
        .expect("to_ops");
    assert!(!ops.is_empty());
    ws.apply(ops, Origin::Edit)
        .unwrap_or_else(|e| panic!("apply MakeSharedFragment ({sel_label}): {e}"));

    let after_project = ws.project().clone();
    let after_snapshot = saved(&after_project);
    let after_ws_files = after_project.to_workspace_files().expect("ws files after");
    assert_ne!(after_snapshot, before_snapshot);
    assert_eq!(ws.edits().count(), before_undo_count + 1);

    let lbl = ws.undo_label().expect("undo label").clone();
    assert!(!lbl.is_removal());
    assert!(lbl.imperative().contains("fragment"));
    assert!(lbl.past().contains("shared"));
    assert_eq!(
        lbl.selection(&after_project),
        Some(Selection::Tree(new_frag_id))
    );

    // Verify new fragment role, TT ID, disjoint fresh node IDs, and no host tree metadata leaked.
    let frag_idx = after_project
        .attack_tree_idx(&new_frag_id)
        .expect("new fragment");
    assert_eq!(
        after_project.attack_tree_role(frag_idx),
        Some(TreeRole::Fragment)
    );
    let frag_vis = after_project
        .attack_tree_visual_id(frag_idx)
        .expect("frag vis");
    assert!(frag_vis.starts_with("TT"), "expected TT id, got {frag_vis}");
    assert_eq!(
        after_project.description(ElementRef::Tree(new_frag_id)),
        None
    );
    assert!(
        after_project
            .condition_link_ids(ElementRef::Tree(new_frag_id))
            .is_empty()
    );
    let frag_tree = after_project.attack_tree(frag_idx).expect("frag tree");
    let frag_node_ids: Vec<NodeId> = frag_tree.iter().map(|(_, id, _)| id).collect();
    let frag_node_set: HashSet<NodeId> = frag_node_ids.iter().copied().collect();
    assert!(before_node_ids.is_disjoint(&frag_node_set));
    assert_eq!(
        frag_tree.get(frag_tree.root()).expect("frag root").label(),
        sel_label
    );

    // Verify host tree identity, metadata, threat links, and selected reference node.
    let after_host_idx = after_project.attack_tree_idx(&host_tree).expect("host");
    assert_eq!(
        after_project.attack_tree_role(after_host_idx),
        Some(host_role)
    );
    assert_eq!(
        after_project.attack_tree_visual_id(after_host_idx),
        Some(host_vis)
    );
    assert_eq!(
        after_project
            .description(ElementRef::Tree(host_tree))
            .map(str::to_owned),
        host_desc
    );
    assert_eq!(
        after_project.condition_link_ids(ElementRef::Tree(host_tree)),
        host_conds.as_slice()
    );
    let after_threats: Vec<_> = after_project
        .threat_scenarios()
        .filter(|(_, _, t)| t.attack_tree() == Some(after_host_idx))
        .map(|(_, id, _)| id)
        .collect();
    assert_eq!(after_threats, host_threats);

    let after_host_tree = after_project
        .attack_tree(after_host_idx)
        .expect("host tree");
    assert_eq!(
        after_host_tree.id_of(after_host_tree.root()),
        Some(host_root_id)
    );
    let after_sel_idx = after_host_tree
        .idx_of(&selected_node)
        .expect("selected node kept");
    let after_sel_node = after_host_tree.get(after_sel_idx).expect("sel");
    assert_eq!(after_sel_node.gate_kind(), None);
    assert_eq!(after_sel_node.label(), sel_label);
    assert_eq!(
        parent_positions(&after_project, host_tree, selected_node),
        sel_parents
    );
    assert_eq!(
        after_project.step_fragment_id(after_host_idx, after_sel_idx),
        Some(new_frag_id)
    );
    assert!(
        after_project
            .step_condition_link_ids(host_tree, selected_node)
            .is_empty(),
        "conditions must move into fragment, not duplicate on host reference"
    );
    assert!(
        after_project
            .step_mitigations(after_host_idx, after_sel_idx)
            .is_empty(),
        "mitigations must move into fragment, not duplicate on host reference"
    );
    assert!(
        !after_project.review_marked(host_tree, selected_node),
        "review mark must move into fragment, not duplicate on host reference"
    );

    // Independent oracle & cached eval prove unchanged host values in both modes.
    let after_cache_eval = eval_outcome_pair(ws, host_tree);
    let after_oracle = {
        let mut oracle = IndependentFold::new(&after_project);
        (
            oracle.fold_tree(host_tree, false),
            oracle.fold_tree(host_tree, true),
        )
    };
    assert_eq!(after_cache_eval, before_oracle);
    assert_eq!(after_oracle, before_oracle);
    let frag_cache_eval = eval_outcome_pair(ws, new_frag_id);
    let frag_oracle = {
        let mut oracle = IndependentFold::new(&after_project);
        (
            oracle.fold_tree(new_frag_id, false),
            oracle.fold_tree(new_frag_id, true),
        )
    };
    assert_eq!(frag_cache_eval, frag_oracle);

    // One undo restores complete Project, snapshot/workspace bytes, prior undo entry, and cache.
    ws.undo().expect("one undo");
    assert!(ws.project() == &before_project);
    assert_eq!(saved(ws.project()), before_snapshot);
    assert_eq!(
        ws.project().to_workspace_files().expect("ws files undo"),
        before_ws_files
    );
    assert!(ws.project().attack_tree_idx(&new_frag_id).is_none());
    assert_eq!(ws.edits().count(), before_undo_count);
    assert_eq!(ws.undo_label(), Some(&before_prior_label));
    assert_eq!(eval_outcome_pair(ws, host_tree), before_cache_eval);
    assert_eq!(lbl.selection(ws.project()), None);

    // One redo uses identical copied IDs, same TT ID/role, and restores post-extraction bytes.
    ws.redo().expect("one redo");
    assert!(ws.project() == &after_project);
    assert_eq!(saved(ws.project()), after_snapshot);
    assert_eq!(
        ws.project().to_workspace_files().expect("ws files redo"),
        after_ws_files
    );
    let redo_frag_idx = ws
        .project()
        .attack_tree_idx(&new_frag_id)
        .expect("redo frag");
    assert_eq!(
        ws.project().attack_tree_role(redo_frag_idx),
        Some(TreeRole::Fragment)
    );
    assert_eq!(
        ws.project().attack_tree_visual_id(redo_frag_idx),
        Some(frag_vis)
    );
    let redo_node_ids: Vec<NodeId> = ws
        .project()
        .attack_tree(redo_frag_idx)
        .expect("redo frag tree")
        .iter()
        .map(|(_, id, _)| id)
        .collect();
    assert_eq!(redo_node_ids, frag_node_ids);
    assert_eq!(eval_outcome_pair(ws, host_tree), after_cache_eval);
    assert_eq!(eval_outcome_pair(ws, new_frag_id), frag_cache_eval);
}

#[test]
fn make_shared_fragment_fluent_labels_resolve_in_en_ja_pseudo() {
    for lang in [Language::En, Language::Ja, Language::Pseudo] {
        let cat = Catalog::load(lang);
        let label = cat.tr("make-shared-fragment");
        assert!(!label.is_empty());
        assert_ne!(label, "make-shared-fragment");
        assert_ne!(label, "[make-shared-fragment]");
        match lang {
            Language::En => assert_eq!(label, "Make shared fragment"),
            Language::Ja => assert_eq!(label, "共有フラグメントを作成"),
            Language::Pseudo => {
                assert!(label.starts_with('⟦') && label.ends_with('⟧'), "{label}");
            }
        }
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn make_shared_fragment_workflows_preserve_values_structure_and_loaded_undo() {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let mut setup_ws = Workspace::empty(Arc::clone(&m));
    let p = Provenance::human("fixture-author", 4242);
    let low = m.feasibility_level("low").expect("low");
    let med = m.feasibility_level("medium").expect("medium");
    let high = m.feasibility_level("high").expect("high");
    let factor = m.feasibility_factors().next().expect("factor");
    let flevel = m.factor_levels(factor).next().expect("factor level");
    let atk_type = m.attack_types().next().expect("attack type");

    let comp_id = ComponentId::new_v7();
    let comp_off_id = ComponentId::new_v7();
    let asset_id = AssetId::new_v7();
    let threat_id = ThreatId::new_v7();
    let cond_on = ConditionId::new_v7();
    let cond_off = ConditionId::new_v7();
    let cond_tree = ConditionId::new_v7();
    let ctrl_impl = ControlId::new_v7();
    let ctrl_plan = ControlId::new_v7();
    let assump_step = AssumptionId::new_v7();
    let shared_step_id = SharedStepId::new_v7();

    // Pre-existing nested fragment used inside selected subtrees.
    let nested_frag_id = TreeId::new_v7();
    let nf_root = NodeId::new_v7();
    let nested_frag_tree = AttackTree::new(nf_root, Node::step("Nested frag leaf")).unwrap();

    // Consumer threat tree using a Fragment tree (`host_frag`) so we test root extraction of a Fragment.
    let host_frag_id = TreeId::new_v7();
    let hf_root = NodeId::new_v7();
    let hf_a = NodeId::new_v7();
    let hf_b = NodeId::new_v7();
    let mut host_frag = AttackTree::new(hf_root, Node::gate(GateKind::And, "HostFrag")).unwrap();
    host_frag
        .add_child(host_frag.root(), hf_a, Node::step("HF_A"))
        .unwrap();
    host_frag
        .add_child(host_frag.root(), hf_b, Node::step("HF_B"))
        .unwrap();

    let consumer_id = TreeId::new_v7();
    let con_root = NodeId::new_v7();
    let con_ref = NodeId::new_v7();
    let mut consumer = AttackTree::new(con_root, Node::gate(GateKind::Or, "Consumer")).unwrap();
    consumer
        .add_child(consumer.root(), con_ref, Node::step("Uses HostFrag"))
        .unwrap();

    // Main complex Threat tree (inherent = High, residual = Low; never poisoned by Unrated) covering:
    // - rich rated step (factors, via, attack_type, conditions, mitigations, review mark)
    // - AND / OR / SEQ gates
    // - multiply-parented node (`multi_gate`) in middle positions of `root` and `sib_parent`
    // - internal diamond inside `multi_gate` (`dm_a -> dm_shared`, `dm_b -> dm_shared`)
    // - descendant shared outside selection (both owner-outside `out_owned` AND owner-inside `in_owned`
    //   which is a Gate owning `in_owned_kid` and linking `keep_target` in stays!)
    // - nested SharedStep and nested Fragment ref with overlays
    let host_id = TreeId::new_v7();
    let (
        n_root,
        n_step_rich,
        n_sib_left,
        n_multi_gate,
        n_sib_right,
        n_sib_parent,
        n_sp_left,
        n_sp_right,
    ) = (
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
    );
    let (
        n_mg_seq,
        n_mg_or,
        n_dm_a,
        n_dm_b,
        n_dm_shared,
        n_nested_sh,
        n_nested_fr,
        n_out_gate,
        n_out_owned,
        n_in_owned_gate,
        n_in_owned_kid,
        n_keep_target,
    ) = (
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
    );

    let mut host = AttackTree::new(n_root, Node::gate(GateKind::Or, "Host Root")).unwrap();
    let r = host.root();
    host.add_child(r, n_step_rich, Node::step("Rich step"))
        .unwrap();
    host.add_child(r, n_sib_left, Node::step("SibL")).unwrap();
    let mg = host
        .add_child(r, n_multi_gate, Node::gate(GateKind::And, "MultiGate"))
        .unwrap();
    host.add_child(r, n_sib_right, Node::step("SibR")).unwrap();

    let sp = host
        .add_child(r, n_sib_parent, Node::gate(GateKind::Or, "SibParent"))
        .unwrap();
    host.add_child(sp, n_sp_left, Node::step("SpL")).unwrap();
    host.link(sp, mg).unwrap();
    host.add_child(sp, n_sp_right, Node::step("SpR")).unwrap();

    let og = host
        .add_child(r, n_out_gate, Node::gate(GateKind::Or, "OutGate"))
        .unwrap();
    let oo = host
        .add_child(og, n_out_owned, Node::step("OutOwnedShared"))
        .unwrap();
    let kt = host
        .add_child(og, n_keep_target, Node::step("KeepTarget"))
        .unwrap();

    let mg_seq = host
        .add_child(mg, n_mg_seq, Node::gate(GateKind::SeqAnd, "SeqInside"))
        .unwrap();
    let mg_or = host
        .add_child(mg, n_mg_or, Node::gate(GateKind::Or, "OrInside"))
        .unwrap();
    let dm_a = host
        .add_child(mg_seq, n_dm_a, Node::gate(GateKind::And, "DmA"))
        .unwrap();
    let dm_b = host
        .add_child(mg_seq, n_dm_b, Node::gate(GateKind::Or, "DmB"))
        .unwrap();
    let dm_s = host
        .add_child(dm_a, n_dm_shared, Node::step("DiamondShared"))
        .unwrap();
    host.link(dm_b, dm_s).unwrap();
    host.add_child(mg_or, n_nested_sh, Node::step("NestedShared"))
        .unwrap();
    host.add_child(mg_or, n_nested_fr, Node::step("NestedFrag"))
        .unwrap();
    host.link(mg_or, oo).unwrap();

    let io_gate = host
        .add_child(
            mg,
            n_in_owned_gate,
            Node::gate(GateKind::And, "InOwnedGate"),
        )
        .unwrap();
    host.add_child(io_gate, n_in_owned_kid, Node::step("InOwnedKid"))
        .unwrap();
    host.link(io_gate, kt).unwrap();
    host.link(og, io_gate).unwrap();

    // Dedicated trees for unrated and excluded inputs (off condition, via off component, assumption, attack filter).
    let unrated_tree_id = TreeId::new_v7();
    let (ur_root, ur_step) = (NodeId::new_v7(), NodeId::new_v7());
    let mut unrated_tree =
        AttackTree::new(ur_root, Node::gate(GateKind::Or, "UnratedRoot")).unwrap();
    unrated_tree
        .add_child(unrated_tree.root(), ur_step, Node::step("UnratedStep"))
        .unwrap();

    let excl_tree_id = TreeId::new_v7();
    let (ex_root, ex_cond_step, ex_via_step, ex_factor_step) = (
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
    );
    let mut excl_tree = AttackTree::new(ex_root, Node::gate(GateKind::Or, "ExclRoot")).unwrap();
    let exr = excl_tree.root();
    excl_tree
        .add_child(exr, ex_cond_step, Node::step("ExCond"))
        .unwrap();
    excl_tree
        .add_child(exr, ex_via_step, Node::step("ExVia"))
        .unwrap();
    excl_tree
        .add_child(exr, ex_factor_step, Node::step("ExFactor"))
        .unwrap();

    let assump_tree_id = TreeId::new_v7();
    let (at_root, ex_assump_step) = (NodeId::new_v7(), NodeId::new_v7());
    let mut assump_tree = AttackTree::new(at_root, Node::gate(GateKind::Or, "AssumpRoot")).unwrap();
    assump_tree
        .add_child(assump_tree.root(), ex_assump_step, Node::step("ExAssump"))
        .unwrap();

    // Single-step root Threat tree (already Step at root).
    let step_root_tree_id = TreeId::new_v7();
    let sr_node = NodeId::new_v7();
    let step_root_tree = AttackTree::new(sr_node, Node::step("Single root step")).unwrap();

    // >32-node gate selection tree (40 nodes).
    let big_tree_id = TreeId::new_v7();
    let (bt_root, bt_gate) = (NodeId::new_v7(), NodeId::new_v7());
    let mut big_tree = AttackTree::new(bt_root, Node::gate(GateKind::Or, "BigRoot")).unwrap();
    let bg = big_tree
        .add_child(
            big_tree.root(),
            bt_gate,
            Node::gate(GateKind::SeqAnd, "BigGate"),
        )
        .unwrap();
    let mut big_leaves = Vec::new();
    for idx in 0..38 {
        let lid = NodeId::new_v7();
        big_tree
            .add_child(bg, lid, Node::step(format!("BigStep {idx}")))
            .unwrap();
        big_leaves.push(lid);
    }

    let mut setup_ops = vec![
        Op::AddComponent {
            id: comp_id,
            name: "ECU".into(),
            parent: None,
            provenance: p.clone(),
        },
        Op::AddComponent {
            id: comp_off_id,
            name: "DisabledECU".into(),
            parent: None,
            provenance: p.clone(),
        },
        Op::AddAsset {
            id: asset_id,
            name: "Asset".into(),
            properties: m.all_properties(),
            provenance: p.clone(),
        },
        Op::AddThreatScenario {
            id: threat_id,
            name: "HostThreat".into(),
            asset: asset_id,
            category: m.threat_categories().next().unwrap(),
            catalog_refs: vec![],
            damages: vec![],
            provenance: p.clone(),
        },
        Op::AddCondition {
            id: cond_on,
            name: "CondOn".into(),
            description: "".into(),
            present: true,
            provenance: p.clone(),
        },
        Op::AddCondition {
            id: cond_off,
            name: "CondOff".into(),
            description: "".into(),
            present: false,
            provenance: p.clone(),
        },
        Op::AddCondition {
            id: cond_tree,
            name: "TreeCondOn".into(),
            description: "".into(),
            present: true,
            provenance: p.clone(),
        },
        Op::NeedCondition {
            element: ElementRef::Component(comp_off_id),
            condition: cond_off,
            provenance: p.clone(),
        },
        Op::AddSecurityControl {
            id: ctrl_impl,
            name: "ImplCtrl".into(),
            provenance: p.clone(),
        },
        Op::SetControlStatus {
            control: ctrl_impl,
            status: ControlStatus::Planned,
            provenance: p.clone(),
        },
        Op::SetControlRule {
            control: ctrl_impl,
            attack_type: atk_type,
            ceiling: Some(low),
            provenance: p.clone(),
        },
        Op::ApplyControl {
            control: ctrl_impl,
            target: ItemRef::Component(comp_id),
            provenance: p.clone(),
        },
        Op::AddSecurityControl {
            id: ctrl_plan,
            name: "PlanCtrl".into(),
            provenance: p.clone(),
        },
        Op::SetControlStatus {
            control: ctrl_plan,
            status: ControlStatus::Planned,
            provenance: p.clone(),
        },
        Op::AddAssumption {
            id: assump_step,
            name: "HoldAssump".into(),
            holds: true,
            provenance: p.clone(),
        },
        Op::SetAttackFilter {
            filter: AttackFilter::default().without(factor, flevel),
            provenance: p.clone(),
        },
        Op::AddSharedStep {
            id: shared_step_id,
            name: "SharedLeaf".into(),
            provenance: p.clone(),
        },
        Op::SetStepFeasibility {
            target: StepTarget::Shared(shared_step_id),
            level: Some(med),
            provenance: p.clone(),
        },
        Op::AddAttackTree {
            role: TreeRole::Fragment,
            id: nested_frag_id,
            tree: nested_frag_tree,
            step_feasibility: vec![(nf_root, high)],
            provenance: p.clone(),
        },
        Op::AddAttackTree {
            role: TreeRole::Fragment,
            id: host_frag_id,
            tree: host_frag,
            step_feasibility: vec![(hf_a, low), (hf_b, med)],
            provenance: p.clone(),
        },
        Op::AddAttackTree {
            role: TreeRole::Threat,
            id: consumer_id,
            tree: consumer,
            step_feasibility: vec![],
            provenance: p.clone(),
        },
        Op::UseFragment {
            tree: consumer_id,
            node: con_ref,
            fragment: host_frag_id,
            provenance: p.clone(),
        },
        Op::AddAttackTree {
            role: TreeRole::Threat,
            id: host_id,
            tree: host,
            step_feasibility: vec![
                (n_step_rich, high),
                (n_sib_left, low),
                (n_sib_right, low),
                (n_sp_left, low),
                (n_sp_right, low),
                (n_out_owned, med),
                (n_keep_target, low),
                (n_dm_shared, high),
                (n_in_owned_kid, med),
            ],
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
        Op::SetDescriptionTranslation {
            target: ElementRef::Tree(host_id),
            lang: "ja".into(),
            text: "ホスト説明".into(),
            provenance: p.clone(),
        },
        Op::NeedCondition {
            element: ElementRef::Tree(host_id),
            condition: cond_tree,
            provenance: p.clone(),
        },
        Op::SetStepFactor {
            target: StepTarget::Step(host_id, n_step_rich),
            factor,
            level: Some(
                m.factor_levels(factor)
                    .last()
                    .expect("non-excluded factor level"),
            ),
            provenance: p.clone(),
        },
        Op::SetStepVia {
            target: StepTarget::Step(host_id, n_step_rich),
            via: Some(ItemRef::Component(comp_id)),
            provenance: p.clone(),
        },
        Op::SetStepAttackType {
            target: StepTarget::Step(host_id, n_step_rich),
            attack_type: Some(atk_type),
            provenance: p.clone(),
        },
        Op::NeedStepCondition {
            tree: host_id,
            node: n_step_rich,
            condition: cond_on,
            provenance: p.clone(),
        },
        Op::SetStepMitigation {
            tree: host_id,
            node: n_step_rich,
            control: ctrl_plan,
            feasibility: Some(low),
            provenance: p.clone(),
        },
        Op::SetReviewMark {
            tree: host_id,
            node: n_step_rich,
            marked: true,
            provenance: p.clone(),
        },
        Op::UseSharedStep {
            tree: host_id,
            node: n_nested_sh,
            step: shared_step_id,
            provenance: p.clone(),
        },
        Op::SetReviewMark {
            tree: host_id,
            node: n_nested_sh,
            marked: true,
            provenance: p.clone(),
        },
        Op::UseFragment {
            tree: host_id,
            node: n_nested_fr,
            fragment: nested_frag_id,
            provenance: p.clone(),
        },
        Op::SetStepMitigation {
            tree: host_id,
            node: n_nested_fr,
            control: ctrl_plan,
            feasibility: Some(low),
            provenance: p.clone(),
        },
        Op::NeedStepCondition {
            tree: host_id,
            node: n_in_owned_kid,
            condition: cond_on,
            provenance: p.clone(),
        },
        Op::SetStepMitigation {
            tree: host_id,
            node: n_in_owned_kid,
            control: ctrl_plan,
            feasibility: Some(low),
            provenance: p.clone(),
        },
        Op::SetReviewMark {
            tree: host_id,
            node: n_in_owned_kid,
            marked: true,
            provenance: p.clone(),
        },
        Op::AddAttackTree {
            role: TreeRole::Threat,
            id: unrated_tree_id,
            tree: unrated_tree,
            step_feasibility: vec![],
            provenance: p.clone(),
        },
        Op::AddAttackTree {
            role: TreeRole::Threat,
            id: excl_tree_id,
            tree: excl_tree,
            step_feasibility: vec![
                (ex_cond_step, high),
                (ex_via_step, high),
                (ex_factor_step, high),
            ],
            provenance: p.clone(),
        },
        Op::NeedStepCondition {
            tree: excl_tree_id,
            node: ex_cond_step,
            condition: cond_off,
            provenance: p.clone(),
        },
        Op::SetStepVia {
            target: StepTarget::Step(excl_tree_id, ex_via_step),
            via: Some(ItemRef::Component(comp_off_id)),
            provenance: p.clone(),
        },
        Op::SetStepFactor {
            target: StepTarget::Step(excl_tree_id, ex_factor_step),
            factor,
            level: Some(flevel),
            provenance: p.clone(),
        },
        Op::AddAttackTree {
            role: TreeRole::Threat,
            id: assump_tree_id,
            tree: assump_tree,
            step_feasibility: vec![(ex_assump_step, med)],
            provenance: p.clone(),
        },
        Op::SetAssumptionExcludes {
            id: assump_step,
            trees: vec![],
            steps: vec![StepRef {
                tree: assump_tree_id,
                node: ex_assump_step,
            }],
            provenance: p.clone(),
        },
        Op::AddAttackTree {
            role: TreeRole::Threat,
            id: step_root_tree_id,
            tree: step_root_tree,
            step_feasibility: vec![(sr_node, med)],
            provenance: p.clone(),
        },
    ];
    for batch in add_tree_skeleton_ops(TreeRole::Threat, big_tree_id, &big_tree, &p).chunks(400) {
        setup_ws.apply(batch.to_vec(), Origin::Edit).expect("big");
    }
    for (i, lid) in big_leaves.iter().enumerate() {
        setup_ops.push(Op::SetStepFeasibility {
            target: StepTarget::Step(big_tree_id, *lid),
            level: Some(if i % 2 == 0 { low } else { high }),
            provenance: p.clone(),
        });
    }
    setup_ws.apply(setup_ops, Origin::Edit).expect("setup ops");

    // Round-trip through snapshot bytes AND workspace files to get a loaded non-default provenance fixture.
    let snap_bytes = saved(setup_ws.project());
    let loaded_from_snap = Project::load(snap_bytes.as_slice(), Arc::clone(&m)).expect("snap load");
    let ws_files = loaded_from_snap.to_workspace_files().expect("to_ws");
    let loaded_proj = Project::from_workspace_files(
        ws_files
            .iter()
            .map(|f| (f.path.as_str(), f.bytes.as_slice())),
        Arc::clone(&m),
    )
    .expect("from_ws");
    assert_eq!(loaded_proj, *setup_ws.project());

    // Create workspace with a pre-existing undo entry before every scenario.
    let make_ws = || {
        let mut ws = Workspace::new(loaded_proj.clone(), 0);
        let prior = Edit::RenameTreeNode {
            tree: host_id,
            node: n_sib_left,
            label: "SibL renamed".into(),
        };
        let pops = prior
            .to_ops(ws.project(), "prior-editor", 100)
            .expect("prior");
        ws.apply(pops, Origin::Edit).expect("prior apply");
        ws
    };

    // 1. Rich step selection (ratings/factors/via/attack_type/conditions/mitigations/review mark move, not duplicate).
    // Verify inherent (High) != residual (Med) on host tree and both modes stay unchanged.
    {
        let mut ws = make_ws();
        let (inh, res) = eval_outcome_pair(&ws, host_id);
        assert_eq!(inh, FoldOutcome::Value(high));
        assert_eq!(res, FoldOutcome::Value(med));
        let frag_id = TreeId::new_v7();
        verify_make_shared_fragment(&mut ws, host_id, n_step_rich, frag_id);
        let f_idx = ws.project().attack_tree_idx(&frag_id).unwrap();
        let ft = ws.project().attack_tree(f_idx).unwrap();
        let fr = ft.root();
        let fr_id = ft.id_of(fr).unwrap();
        assert_eq!(ws.project().step_feasibility(f_idx, fr), Some(high));
        assert_eq!(
            ws.project().step_factor(f_idx, fr, factor),
            m.factor_levels(factor).last()
        );
        assert_eq!(
            ws.project().step_via(f_idx, fr),
            Some(ItemRef::Component(comp_id))
        );
        assert_eq!(ws.project().step_attack_type(f_idx, fr), Some(atk_type));
        assert_eq!(
            ws.project().step_condition_link_ids(frag_id, fr_id).len(),
            1
        );
        assert_eq!(ws.project().step_mitigations(f_idx, fr).len(), 1);
        assert!(ws.project().review_marked(frag_id, fr_id));
        // Undoing MakeSharedFragment then undoing the pre-existing entry works cleanly.
        ws.undo().expect("undo extraction");
        ws.undo().expect("undo pre-existing");
        assert!(!ws.can_undo());
    }

    // 2. Unrated step & excluded steps (condition-off, via-off, assumption-excluded, filter-excluded) + root.
    for (tree_id, node_id) in [
        (unrated_tree_id, ur_step),
        (unrated_tree_id, ur_root),
        (excl_tree_id, ex_cond_step),
        (excl_tree_id, ex_via_step),
        (assump_tree_id, ex_assump_step),
        (excl_tree_id, ex_factor_step),
        (excl_tree_id, ex_root),
    ] {
        let mut ws = make_ws();
        let frag_id = TreeId::new_v7();
        verify_make_shared_fragment(&mut ws, tree_id, node_id, frag_id);
    }
    // Extracting a gate over a removed assumption-excluded descendant cleans up `excludes_steps` and undoes cleanly.
    {
        let mut ws = make_ws();
        let before = ws.project().clone();
        let before_bytes = saved(&before);
        let frag_id = TreeId::new_v7();
        let ops = Edit::MakeSharedFragment {
            tree: assump_tree_id,
            node: at_root,
            id: frag_id,
        }
        .to_ops(ws.project(), "extractor", 200)
        .expect("ops");
        ws.apply(ops, Origin::Edit).expect("apply");
        let a_idx = ws.project().assumption_idx(&assump_step).unwrap();
        assert!(
            ws.project()
                .assumption(a_idx)
                .unwrap()
                .excludes_steps()
                .is_empty()
        );
        ws.undo().expect("undo");
        assert!(ws.project() == &before);
        assert_eq!(saved(ws.project()), before_bytes);
    }

    // 3. Multiply-parented gate (`n_multi_gate`) with SEQ + OR gates, internal diamond,
    //    nested SharedStep & Fragment, outside-shared owner-outside (`n_out_owned`),
    //    and outside-shared owner-inside gate (`n_in_owned_gate` with `n_in_owned_kid` and link to `n_keep_target`).
    {
        let mut ws = make_ws();
        let frag_id = TreeId::new_v7();
        verify_make_shared_fragment(&mut ws, host_id, n_multi_gate, frag_id);

        // Host still has `n_out_owned`, `n_in_owned_gate` (re-parented to `n_out_gate`), `n_in_owned_kid`, and `n_keep_target`.
        let h_idx = ws.project().attack_tree_idx(&host_id).unwrap();
        let h_tree = ws.project().attack_tree(h_idx).unwrap();
        assert!(h_tree.idx_of(&n_dm_shared).is_none());
        assert!(h_tree.idx_of(&n_nested_sh).is_none());
        assert!(h_tree.idx_of(&n_nested_fr).is_none());
        let io_idx = h_tree
            .idx_of(&n_in_owned_gate)
            .expect("outside-shared gate kept");
        let og_idx = h_tree.idx_of(&n_out_gate).unwrap();
        assert_eq!(h_tree.owner(io_idx), Some(og_idx));
        let io_kids: Vec<_> = h_tree
            .children(io_idx)
            .iter()
            .map(|&c| h_tree.id_of(c).unwrap())
            .collect();
        assert_eq!(io_kids, vec![n_in_owned_kid, n_keep_target]);
        let ik_idx = h_tree
            .idx_of(&n_in_owned_kid)
            .expect("surviving child kept");
        assert_eq!(
            ws.project()
                .step_condition_link_ids(host_id, n_in_owned_kid)
                .len(),
            1
        );
        assert_eq!(ws.project().step_mitigations(h_idx, ik_idx).len(), 1);
        assert!(ws.project().review_marked(host_id, n_in_owned_kid));

        // Fragment preserves SEQ order, diamond parent order, nested SharedStep/Fragment + overlays.
        let f_idx = ws.project().attack_tree_idx(&frag_id).unwrap();
        let f_tree = ws.project().attack_tree(f_idx).unwrap();
        for (idx, _, node) in f_tree.iter() {
            if node.label() == "SeqInside" {
                assert_eq!(node.gate_kind(), Some(GateKind::SeqAnd));
                let kids: Vec<_> = f_tree
                    .children(idx)
                    .iter()
                    .map(|&c| f_tree.get(c).unwrap().label())
                    .collect();
                assert_eq!(kids, vec!["DmA", "DmB"]);
            } else if node.label() == "DiamondShared" {
                let ps: Vec<_> = f_tree
                    .parents(idx)
                    .map(|pi| f_tree.get(pi).unwrap().label())
                    .collect();
                assert_eq!(ps, vec!["DmA", "DmB"]);
            } else if node.label() == "NestedShared" {
                assert_eq!(
                    ws.project().step_shared_id(f_idx, idx),
                    Some(shared_step_id)
                );
            } else if node.label() == "NestedFrag" {
                assert_eq!(
                    ws.project().step_fragment_id(f_idx, idx),
                    Some(nested_frag_id)
                );
                assert_eq!(ws.project().step_mitigations(f_idx, idx).len(), 1);
            }
        }
    }

    // 3b. Also select internal OR (`n_mg_or`, with outside-shared `n_out_owned`) and SEQ (`n_mg_seq`) gates directly.
    for gate_node in [n_mg_or, n_mg_seq] {
        let mut ws = make_ws();
        let frag_id = TreeId::new_v7();
        verify_make_shared_fragment(&mut ws, host_id, gate_node, frag_id);
    }

    // 4. Root selections: Threat root gate, Fragment root gate (with consumer), and 1-step root.
    for (tree_id, root_node) in [
        (host_id, n_root),
        (host_frag_id, hf_root),
        (step_root_tree_id, sr_node),
    ] {
        let mut ws = make_ws();
        let frag_id = TreeId::new_v7();
        verify_make_shared_fragment(&mut ws, tree_id, root_node, frag_id);
        if tree_id == host_frag_id {
            assert_fold_matches(ws.project(), consumer_id, host_frag_id);
        }
    }

    // 5. >32-node gate selection (and >32-node root selection).
    for sel_node in [bt_gate, bt_root] {
        let mut ws = make_ws();
        let frag_id = TreeId::new_v7();
        verify_make_shared_fragment(&mut ws, big_tree_id, sel_node, frag_id);
        let f_idx = ws.project().attack_tree_idx(&frag_id).unwrap();
        let f_tree = ws.project().attack_tree(f_idx).unwrap();
        assert!(f_tree.len() > 32);
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn make_shared_fragment_guards_and_prospective_limits_fail_atomically() {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let mut ws = Workspace::empty(Arc::clone(&m));
    let p = Provenance::human("guard-author", 7777);
    let low = m.feasibility_level("low").expect("low");
    let high = m.feasibility_level("high").expect("high");

    // Depth-8 chain: F8 <- F7 <- ... <- F1 <- Depth8Host.
    let mut chain = Vec::new();
    let f_leaf = TreeId::new_v7();
    let f_leaf_n = NodeId::new_v7();
    let mut setup = vec![Op::AddAttackTree {
        role: TreeRole::Fragment,
        id: f_leaf,
        tree: AttackTree::new(f_leaf_n, Node::step("ChainLeaf")).unwrap(),
        step_feasibility: vec![(f_leaf_n, high)],
        provenance: p.clone(),
    }];
    chain.push(f_leaf);
    for d in 2..=MAX_FRAGMENT_DEPTH {
        let tid = TreeId::new_v7();
        let rn = NodeId::new_v7();
        let mut ft = AttackTree::new(rn, Node::gate(GateKind::Or, format!("F{d}"))).unwrap();
        let child = NodeId::new_v7();
        ft.add_child(ft.root(), child, Node::step(format!("F{d}_child")))
            .unwrap();
        setup.push(Op::AddAttackTree {
            role: TreeRole::Fragment,
            id: tid,
            tree: ft,
            step_feasibility: vec![],
            provenance: p.clone(),
        });
        setup.push(Op::UseFragment {
            tree: tid,
            node: child,
            fragment: *chain.last().unwrap(),
            provenance: p.clone(),
        });
        chain.push(tid);
    }
    let f1_id = *chain.last().unwrap();
    let f1_root = {
        // Depth-8 Threat host wrapping F1 inside gate G.
        let h_id = TreeId::new_v7();
        let (h_root, h_gate, h_ref) = (NodeId::new_v7(), NodeId::new_v7(), NodeId::new_v7());
        let mut ht = AttackTree::new(h_root, Node::gate(GateKind::Or, "D8Host")).unwrap();
        let g = ht
            .add_child(ht.root(), h_gate, Node::gate(GateKind::And, "D8Gate"))
            .unwrap();
        ht.add_child(g, h_ref, Node::step("D8Ref")).unwrap();
        setup.push(Op::AddAttackTree {
            role: TreeRole::Threat,
            id: h_id,
            tree: ht,
            step_feasibility: vec![],
            provenance: p.clone(),
        });
        setup.push(Op::UseFragment {
            tree: h_id,
            node: h_ref,
            fragment: f1_id,
            provenance: p.clone(),
        });

        // SharedStep and Fragment leaves for AlreadyShared guard.
        let sh_id = SharedStepId::new_v7();
        setup.push(Op::AddSharedStep {
            id: sh_id,
            name: "Shared".into(),
            provenance: p.clone(),
        });
        let sh_node = NodeId::new_v7();
        let fr_node = NodeId::new_v7();
        let g_tree_id = TreeId::new_v7();
        let g_root = NodeId::new_v7();
        let mut gt = AttackTree::new(g_root, Node::gate(GateKind::Or, "GuardTree")).unwrap();
        gt.add_child(gt.root(), sh_node, Node::step("UsesShared"))
            .unwrap();
        gt.add_child(gt.root(), fr_node, Node::step("UsesFrag"))
            .unwrap();
        setup.push(Op::AddAttackTree {
            role: TreeRole::Threat,
            id: g_tree_id,
            tree: gt,
            step_feasibility: vec![],
            provenance: p.clone(),
        });
        setup.push(Op::UseSharedStep {
            tree: g_tree_id,
            node: sh_node,
            step: sh_id,
            provenance: p.clone(),
        });
        setup.push(Op::UseFragment {
            tree: g_tree_id,
            node: fr_node,
            fragment: f_leaf,
            provenance: p.clone(),
        });

        // Host at MAX_FRAGMENT_REFERENCES (256) outside `cap_gate`, plus contrast with 1 ref inside.
        let cap_tree_id = TreeId::new_v7();
        let (cap_root, cap_gate, cap_leaf, contrast_gate, contrast_ref) = (
            NodeId::new_v7(),
            NodeId::new_v7(),
            NodeId::new_v7(),
            NodeId::new_v7(),
            NodeId::new_v7(),
        );
        let mut cap_tree = AttackTree::new(cap_root, Node::gate(GateKind::Or, "CapTree")).unwrap();
        let cr = cap_tree.root();
        let cg = cap_tree
            .add_child(cr, cap_gate, Node::gate(GateKind::Or, "CapGate"))
            .unwrap();
        cap_tree
            .add_child(cg, cap_leaf, Node::step("Plain"))
            .unwrap();
        let ctr_g = cap_tree
            .add_child(cr, contrast_gate, Node::gate(GateKind::Or, "ContrastGate"))
            .unwrap();
        cap_tree
            .add_child(ctr_g, contrast_ref, Node::step("InnerRef"))
            .unwrap();
        let mut cap_refs = vec![contrast_ref];
        for i in 0..(MAX_FRAGMENT_REFERENCES - 1) {
            let k = NodeId::new_v7();
            cap_tree
                .add_child(cr, k, Node::step(format!("Ref{i}")))
                .unwrap();
            cap_refs.push(k);
        }

        // Oversized >520-node gate so `ops.len() > MAX_SUBMIT_OPS (1024)`.
        let huge_tree_id = TreeId::new_v7();
        let (huge_root, huge_gate) = (NodeId::new_v7(), NodeId::new_v7());
        let mut huge_tree = AttackTree::new(huge_root, Node::gate(GateKind::Or, "Huge")).unwrap();
        let hg = huge_tree
            .add_child(
                huge_tree.root(),
                huge_gate,
                Node::gate(GateKind::Or, "HugeGate"),
            )
            .unwrap();
        for i in 0..550 {
            let s = NodeId::new_v7();
            huge_tree
                .add_child(hg, s, Node::step(format!("H{i}")))
                .unwrap();
        }

        ws.apply(setup, Origin::Edit).expect("setup");
        for batch in add_tree_skeleton_ops(TreeRole::Threat, cap_tree_id, &cap_tree, &p).chunks(400)
        {
            ws.apply(batch.to_vec(), Origin::Edit).expect("cap tree");
        }
        for batch in
            add_tree_skeleton_ops(TreeRole::Threat, huge_tree_id, &huge_tree, &p).chunks(400)
        {
            ws.apply(batch.to_vec(), Origin::Edit).expect("huge tree");
        }
        let use_cap: Vec<Op> = cap_refs
            .into_iter()
            .map(|n| Op::UseFragment {
                tree: cap_tree_id,
                node: n,
                fragment: f_leaf,
                provenance: p.clone(),
            })
            .collect();
        ws.apply(use_cap, Origin::Edit).expect("256 refs");

        (
            h_id,
            h_gate,
            g_tree_id,
            sh_node,
            fr_node,
            cap_tree_id,
            cap_gate,
            contrast_gate,
            huge_tree_id,
            huge_gate,
        )
    };
    let (
        d8_host,
        d8_gate,
        guard_tree,
        sh_node,
        fr_node,
        cap_tree_id,
        cap_gate,
        contrast_gate,
        huge_tree_id,
        huge_gate,
    ) = f1_root;

    // Pre-existing undo entry & pending batch kept intact across all failed attempts.
    let prior_edit = Edit::RenameTreeNode {
        tree: d8_host,
        node: d8_gate,
        label: "D8Gate renamed".into(),
    };
    let prior_ops = prior_edit.to_ops(ws.project(), "prior", 50).expect("prior");
    ws.apply(prior_ops, Origin::Edit).expect("apply prior");

    let assert_unchanged =
        |ws: &Workspace,
         before_proj: &Project,
         before_bytes: &[u8],
         before_files: &[aegis_model::WorkspaceFile],
         before_pending: usize,
         before_undo_len: usize,
         before_label: &aegis_client::undo_label::Label| {
            assert!(ws.project() == before_proj);
            assert_eq!(saved(ws.project()), before_bytes);
            assert_eq!(
                ws.project().to_workspace_files().expect("ws files"),
                before_files
            );
            assert_eq!(ws.pending_batches(), before_pending);
            assert_eq!(ws.edits().count(), before_undo_len);
            assert_eq!(ws.undo_label(), Some(before_label));
        };

    let before_proj = ws.project().clone();
    let before_bytes = saved(&before_proj);
    let before_files = before_proj.to_workspace_files().expect("ws files");
    let before_pending = ws.pending_batches();
    let before_undo_len = ws.edits().count();
    let before_label = ws.undo_label().expect("prior label").clone();
    let before_d8_eval = eval_outcome_pair(&ws, d8_host);

    // 1. Guard: already shared step / already fragment reference -> EditError::AlreadyShared.
    for already_node in [sh_node, fr_node] {
        let err = Edit::MakeSharedFragment {
            tree: guard_tree,
            node: already_node,
            id: TreeId::new_v7(),
        }
        .to_ops(ws.project(), "ed", 1)
        .expect_err("already shared");
        assert_eq!(err, EditError::AlreadyShared);
        assert!(err.to_string().contains("Unshare"));
    }

    // 2. Guard: invalid selection (missing tree, missing node, cross-tree node).
    for (t, n) in [
        (TreeId::new_v7(), sh_node),
        (guard_tree, NodeId::new_v7()),
        (guard_tree, d8_gate),
    ] {
        let err = Edit::MakeSharedFragment {
            tree: t,
            node: n,
            id: TreeId::new_v7(),
        }
        .to_ops(ws.project(), "ed", 1)
        .expect_err("missing");
        assert_eq!(err, EditError::Missing);
    }

    // 3. Prospective depth-9 failure on host gate AND inside depth-7 fragment `f1_id` fails atomically.
    let f1_root_id = {
        let f1_t = ws
            .project()
            .attack_tree(ws.project().attack_tree_idx(&f1_id).unwrap())
            .unwrap();
        f1_t.id_of(f1_t.root()).unwrap()
    };
    for (target_tree, target_node) in [(d8_host, d8_gate), (f1_id, f1_root_id)] {
        let orphan = TreeId::new_v7();
        let ops = Edit::MakeSharedFragment {
            tree: target_tree,
            node: target_node,
            id: orphan,
        }
        .to_ops(ws.project(), "ed", 2)
        .expect("ops");
        let err = ws.apply(ops, Origin::Edit).expect_err("depth 9 fails");
        assert_eq!(
            err,
            WorkspaceError::Model(ModelError::FragmentDepth {
                max: MAX_FRAGMENT_DEPTH
            })
        );
        assert!(ws.project().attack_tree_idx(&orphan).is_none());
        assert_unchanged(
            &ws,
            &before_proj,
            &before_bytes,
            &before_files,
            before_pending,
            before_undo_len,
            &before_label,
        );
        assert_eq!(eval_outcome_pair(&ws, d8_host), before_d8_eval);
    }

    // 4. Duplicate target ID fails atomically.
    let dup_ops = Edit::MakeSharedFragment {
        tree: guard_tree,
        node: sh_node,
        id: f_leaf,
    };
    assert_eq!(
        dup_ops.to_ops(ws.project(), "ed", 2),
        Err(EditError::AlreadyShared)
    );
    let dup_gate_ops = Edit::MakeSharedFragment {
        tree: d8_host,
        node: d8_gate,
        id: d8_host,
    }
    .to_ops(ws.project(), "ed", 2)
    .expect("ops");
    assert_eq!(
        ws.apply(dup_gate_ops, Origin::Edit).expect_err("dup"),
        WorkspaceError::Model(ModelError::DuplicateAttackTree(d8_host))
    );
    assert_unchanged(
        &ws,
        &before_proj,
        &before_bytes,
        &before_files,
        before_pending,
        before_undo_len,
        &before_label,
    );

    // 5. MAX_SUBMIT_OPS cap overflow fails atomically.
    let huge_orphan = TreeId::new_v7();
    let huge_ops = Edit::MakeSharedFragment {
        tree: huge_tree_id,
        node: huge_gate,
        id: huge_orphan,
    }
    .to_ops(ws.project(), "ed", 2)
    .expect("ops");
    assert!(huge_ops.len() > MAX_SUBMIT_OPS);
    assert!(matches!(
        ws.apply(huge_ops, Origin::Edit),
        Err(WorkspaceError::TooLarge { .. })
    ));
    assert!(ws.project().attack_tree_idx(&huge_orphan).is_none());
    assert_unchanged(
        &ws,
        &before_proj,
        &before_bytes,
        &before_files,
        before_pending,
        before_undo_len,
        &before_label,
    );

    // 6. MAX_FRAGMENT_REFERENCES (256) outside selection fails atomically; contrast (1 inside) succeeds!
    let cap_orphan = TreeId::new_v7();
    let cap_ops = Edit::MakeSharedFragment {
        tree: cap_tree_id,
        node: cap_gate,
        id: cap_orphan,
    }
    .to_ops(ws.project(), "ed", 2)
    .expect("ops");
    assert_eq!(
        ws.apply(cap_ops, Origin::Edit).expect_err("257 refs"),
        WorkspaceError::Model(ModelError::TooManyFragmentReferences {
            tree: cap_tree_id,
            max: MAX_FRAGMENT_REFERENCES,
        })
    );
    assert!(ws.project().attack_tree_idx(&cap_orphan).is_none());
    assert_unchanged(
        &ws,
        &before_proj,
        &before_bytes,
        &before_files,
        before_pending,
        before_undo_len,
        &before_label,
    );

    let contrast_frag = TreeId::new_v7();
    let contrast_ops = Edit::MakeSharedFragment {
        tree: cap_tree_id,
        node: contrast_gate,
        id: contrast_frag,
    }
    .to_ops(ws.project(), "ed", 3)
    .expect("contrast ops");
    ws.apply(contrast_ops, Origin::Edit)
        .expect("contrast replacing 1 ref with 1 ref stays at 256");

    // 7. In-use protection: newly extracted fragment cannot be removed until UnuseFragment.
    let rm_ops = Edit::RemoveTree {
        tree: contrast_frag,
    }
    .to_ops(ws.project(), "ed", 4)
    .expect("rm ops");
    assert!(ws.apply(rm_ops, Origin::Edit).is_err());

    // 8. Also confirm existing MakeShared (shared step) retains its exact behavior.
    let plain_tree = TreeId::new_v7();
    let (pr, ps) = (NodeId::new_v7(), NodeId::new_v7());
    let mut pt = AttackTree::new(pr, Node::gate(GateKind::Or, "Plain")).unwrap();
    pt.add_child(pt.root(), ps, Node::step("PlainStep"))
        .unwrap();
    ws.apply(
        vec![Op::AddAttackTree {
            role: TreeRole::Threat,
            id: plain_tree,
            tree: pt,
            step_feasibility: vec![(ps, low)],
            provenance: p,
        }],
        Origin::Edit,
    )
    .expect("add plain");
    let new_shared = SharedStepId::new_v7();
    let ms_ops = Edit::MakeShared {
        tree: plain_tree,
        node: ps,
        id: new_shared,
    }
    .to_ops(ws.project(), "ed", 5)
    .expect("MakeShared");
    ws.apply(ms_ops, Origin::Edit).expect("apply MakeShared");
    let p_idx = ws.project().attack_tree_idx(&plain_tree).unwrap();
    let ps_idx = ws
        .project()
        .attack_tree(p_idx)
        .unwrap()
        .idx_of(&ps)
        .unwrap();
    assert_eq!(ws.project().step_shared_id(p_idx, ps_idx), Some(new_shared));
}

#[test]
fn unshare_fragment_fluent_labels_resolve_in_en_ja_pseudo() {
    for lang in [Language::En, Language::Ja, Language::Pseudo] {
        let cat = Catalog::load(lang);
        let label = cat.tr("unshare-fragment");
        assert!(!label.is_empty());
        assert_ne!(label, "unshare-fragment");
        assert_ne!(label, "[unshare-fragment]");
        match lang {
            Language::En => assert_eq!(label, "Unshare fragment"),
            Language::Ja => assert_eq!(label, "フラグメントの共有を解除"),
            Language::Pseudo => {
                assert!(label.starts_with('⟦') && label.ends_with('⟧'), "{label}");
            }
        }
    }
}

fn verify_unshare_fragment(ws: &mut Workspace, host_tree: TreeId, selected_node: NodeId) {
    assert!(
        ws.can_undo(),
        "tests must start with a pre-existing undo entry"
    );
    let before_project = ws.project().clone();
    let before_snapshot = saved(&before_project);
    let before_ws_files = before_project
        .to_workspace_files()
        .expect("ws files before");
    let before_undo_count = ws.edits().count();
    let before_prior_label = ws.undo_label().expect("prior undo label").clone();
    let before_cache_eval = eval_outcome_pair(ws, host_tree);
    let before_oracle = {
        let mut oracle = IndependentFold::new(&before_project);
        (
            oracle.fold_tree(host_tree, false),
            oracle.fold_tree(host_tree, true),
        )
    };
    assert_eq!(before_cache_eval, before_oracle);

    let host_idx = before_project.attack_tree_idx(&host_tree).expect("host");
    let host_t = before_project.attack_tree(host_idx).expect("host tree");
    let host_root_id = host_t.id_of(host_t.root()).expect("root id");
    let host_role = before_project.attack_tree_role(host_idx).expect("role");
    let host_vis = before_project.attack_tree_visual_id(host_idx).expect("vis");
    let host_desc = before_project
        .description(ElementRef::Tree(host_tree))
        .map(str::to_owned);
    let host_trans: Vec<(String, String)> = before_project
        .description_translations(ElementRef::Tree(host_tree))
        .map(|(l, t)| (l.to_owned(), t.to_owned()))
        .collect();
    let host_conds = before_project
        .condition_link_ids(ElementRef::Tree(host_tree))
        .to_vec();
    let host_threats: Vec<_> = before_project
        .threat_scenarios()
        .filter(|(_, _, t)| t.attack_tree() == Some(host_idx))
        .map(|(_, id, _)| id)
        .collect();

    let sel_idx = host_t.idx_of(&selected_node).expect("selected node");
    let sel_label = host_t.get(sel_idx).expect("sel").label().to_owned();
    let sel_parents = parent_positions(&before_project, host_tree, selected_node);
    let frag_id = before_project
        .step_fragment_id(host_idx, sel_idx)
        .expect("selected node must reference a fragment");
    let frag_idx = before_project.attack_tree_idx(&frag_id).expect("frag");
    let frag_tree = before_project.attack_tree(frag_idx).expect("frag tree");
    let frag_root_label = frag_tree
        .get(frag_tree.root())
        .expect("frag root")
        .label()
        .to_owned();
    let mut resolved_idx = frag_idx;
    let frag_root_kind = loop {
        let rt = before_project.attack_tree(resolved_idx).expect("resolved");
        if let Some(next_frag) = before_project.step_fragment(resolved_idx, rt.root()) {
            resolved_idx = next_frag;
            continue;
        }
        break rt.get(rt.root()).expect("resolved root").gate_kind();
    };

    let before_reusable_trees: Vec<(TreeId, AttackTree)> = before_project
        .attack_trees()
        .filter(|(ti, _, _)| {
            before_project.attack_tree_role(*ti) == Some(TreeRole::Fragment) && *ti != host_idx
        })
        .map(|(_, tid, t)| (tid, t.clone()))
        .collect();
    let before_shared_ids: Vec<SharedStepId> = before_project
        .shared_steps()
        .map(|(_, sid, _)| sid)
        .collect();

    let ops = Edit::Unshare {
        tree: host_tree,
        node: selected_node,
    }
    .to_ops(ws.project(), "unshare-author", 1_700_000_001_111)
    .expect("Unshare to_ops");
    assert!(!ops.is_empty());
    let multi_op = ops.len() > 1;
    ws.apply(ops, Origin::Edit)
        .unwrap_or_else(|e| panic!("apply Unshare ({sel_label}): {e}"));

    let after_project = ws.project().clone();
    let after_snapshot = saved(&after_project);
    let after_ws_files = after_project.to_workspace_files().expect("ws files after");
    assert_ne!(after_snapshot, before_snapshot);
    assert_eq!(ws.edits().count(), before_undo_count + 1);

    if multi_op {
        let lbl = ws.undo_label().expect("undo label");
        assert!(!lbl.is_removal());
        assert!(lbl.imperative().contains("unshare fragment"));
        assert!(lbl.imperative().contains(&frag_root_label));
        assert!(lbl.past().contains("unshared"));
        assert_eq!(
            lbl.selection(&after_project),
            Some(Selection::TreeNode(host_tree, selected_node))
        );
    }

    // Source fragments and shared steps remain completely untouched (even if last user).
    for (tid, old_tree) in &before_reusable_trees {
        let cur_idx = after_project
            .attack_tree_idx(tid)
            .expect("source frag kept");
        assert_eq!(
            after_project.attack_tree_role(cur_idx),
            Some(TreeRole::Fragment)
        );
        assert_eq!(after_project.attack_tree(cur_idx), Some(old_tree));
        assert_eq!(
            after_project.condition_link_ids(ElementRef::Tree(*tid)),
            before_project.condition_link_ids(ElementRef::Tree(*tid))
        );
    }
    let after_shared_ids: Vec<SharedStepId> = after_project
        .shared_steps()
        .map(|(_, sid, _)| sid)
        .collect();
    assert_eq!(after_shared_ids, before_shared_ids);

    // Host identity, metadata, threat links, root, and incoming edges preserved.
    let after_host_idx = after_project.attack_tree_idx(&host_tree).expect("host");
    assert_eq!(
        after_project.attack_tree_role(after_host_idx),
        Some(host_role)
    );
    assert_eq!(
        after_project.attack_tree_visual_id(after_host_idx),
        Some(host_vis)
    );
    assert_eq!(
        after_project
            .description(ElementRef::Tree(host_tree))
            .map(str::to_owned),
        host_desc
    );
    let after_trans: Vec<(String, String)> = after_project
        .description_translations(ElementRef::Tree(host_tree))
        .map(|(l, t)| (l.to_owned(), t.to_owned()))
        .collect();
    assert_eq!(after_trans, host_trans);
    assert_eq!(
        after_project.condition_link_ids(ElementRef::Tree(host_tree)),
        host_conds.as_slice()
    );
    let after_threats: Vec<_> = after_project
        .threat_scenarios()
        .filter(|(_, _, t)| t.attack_tree() == Some(after_host_idx))
        .map(|(_, id, _)| id)
        .collect();
    assert_eq!(after_threats, host_threats);

    let after_host_tree = after_project
        .attack_tree(after_host_idx)
        .expect("host tree");
    assert_eq!(
        after_host_tree.id_of(after_host_tree.root()),
        Some(host_root_id)
    );
    let after_sel_idx = after_host_tree
        .idx_of(&selected_node)
        .expect("selected kept");
    let after_sel_node = after_host_tree.get(after_sel_idx).expect("sel node");
    assert_eq!(after_sel_node.label(), sel_label);
    assert_eq!(after_sel_node.gate_kind(), frag_root_kind);
    assert_eq!(
        parent_positions(&after_project, host_tree, selected_node),
        sel_parents
    );

    // No node inside the materialized subtree retains a fragment or shared slot.
    let mut stack = vec![after_sel_idx];
    let mut seen_sub = HashSet::new();
    while let Some(ni) = stack.pop() {
        if !seen_sub.insert(ni) {
            continue;
        }
        assert!(
            after_project.step_fragment(after_host_idx, ni).is_none(),
            "copied node must not retain fragment slot"
        );
        assert!(
            after_project.step_shared(after_host_idx, ni).is_none(),
            "copied node must not retain shared step slot"
        );
        for &ch in after_host_tree.children(ni) {
            stack.push(ch);
        }
    }
    let copied_node_ids: Vec<NodeId> = after_host_tree.iter().map(|(_, id, _)| id).collect();

    // Independent oracle & cached eval prove identical host values in both modes.
    let after_cache_eval = eval_outcome_pair(ws, host_tree);
    let after_oracle = {
        let mut oracle = IndependentFold::new(&after_project);
        (
            oracle.fold_tree(host_tree, false),
            oracle.fold_tree(host_tree, true),
        )
    };
    assert_eq!(after_cache_eval, before_oracle);
    assert_eq!(after_oracle, before_oracle);

    // Exactly one undo restores complete Project, snapshot/workspace bytes, prior undo entry, and cache.
    ws.undo().expect("one undo");
    assert!(ws.project() == &before_project);
    assert_eq!(saved(ws.project()), before_snapshot);
    assert_eq!(
        ws.project().to_workspace_files().expect("ws files undo"),
        before_ws_files
    );
    assert_eq!(ws.edits().count(), before_undo_count);
    assert_eq!(ws.undo_label(), Some(&before_prior_label));
    assert_eq!(eval_outcome_pair(ws, host_tree), before_cache_eval);

    // One redo restores identical copied IDs and post-Unshare bytes.
    ws.redo().expect("one redo");
    assert!(ws.project() == &after_project);
    assert_eq!(saved(ws.project()), after_snapshot);
    assert_eq!(
        ws.project().to_workspace_files().expect("ws files redo"),
        after_ws_files
    );
    let redo_host_idx = ws.project().attack_tree_idx(&host_tree).expect("redo host");
    let redo_node_ids: Vec<NodeId> = ws
        .project()
        .attack_tree(redo_host_idx)
        .expect("redo host tree")
        .iter()
        .map(|(_, id, _)| id)
        .collect();
    assert_eq!(redo_node_ids, copied_node_ids);
    assert_eq!(eval_outcome_pair(ws, host_tree), after_cache_eval);
}

#[test]
#[allow(clippy::too_many_lines)]
fn unshare_fragment_workflows_preserve_values_overlays_and_live_refs() {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let mut setup_ws = Workspace::empty(Arc::clone(&m));
    let p = Provenance::human("unshare-fixture", 1420);
    let low = m.feasibility_level("low").expect("low");
    let med = m.feasibility_level("medium").expect("medium");
    let high = m.feasibility_level("high").expect("high");
    let factor = m.feasibility_factors().next().expect("factor");
    let flevel_excl = m
        .factor_levels(factor)
        .next()
        .expect("excluded factor level");
    let flevel_ok = m
        .factor_levels(factor)
        .last()
        .expect("allowed factor level");
    let atk_type = m.attack_types().next().expect("attack type");
    let iface_kind = m.interface_kinds().next().expect("iface kind");

    let comp_id = ComponentId::new_v7();
    let iface_id = InterfaceId::new_v7();
    let comp_off_id = ComponentId::new_v7();
    let asset_id = AssetId::new_v7();
    let threat_main_id = ThreatId::new_v7();
    let threat_root_id = ThreatId::new_v7();

    let cond_host_tree = ConditionId::new_v7();
    let cond_host_local = ConditionId::new_v7();
    let cond_outer_tree = ConditionId::new_v7();
    let cond_bnd_a = ConditionId::new_v7();
    let cond_bnd_b = ConditionId::new_v7();
    let cond_frag_tree = ConditionId::new_v7();
    let cond_leaf_on = ConditionId::new_v7();
    let cond_off = ConditionId::new_v7();
    let cond_assump = ConditionId::new_v7();

    let ctrl_same = ControlId::new_v7();
    let ctrl_host_plan = ControlId::new_v7();
    let ctrl_inner_plan = ControlId::new_v7();
    let ctrl_off = ControlId::new_v7();
    let ctrl_via = ControlId::new_v7();

    let assump_host = AssumptionId::new_v7();
    let assump_frag_tree = AssumptionId::new_v7();
    let shared_rich_id = SharedStepId::new_v7();
    let shared_plain_id = SharedStepId::new_v7();

    // Inner diamond fragment `frag_diam`:
    // DiamRoot(OR) -> [Left(AND), Right(SEQ)]
    // Left owns `SharedLeaf`, Right links `SharedLeaf` then owns `SeqTail` (uses `shared_rich_id`).
    let frag_diam_id = TreeId::new_v7();
    let (fd_root, fd_left, fd_right, fd_shared_leaf, fd_seq_tail) = (
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
    );
    let mut frag_diam = AttackTree::new(fd_root, Node::gate(GateKind::Or, "DiamRoot")).unwrap();
    let fdr = frag_diam.root();
    let fdl = frag_diam
        .add_child(fdr, fd_left, Node::gate(GateKind::And, "LeftBranch"))
        .unwrap();
    let fdrg = frag_diam
        .add_child(fdr, fd_right, Node::gate(GateKind::SeqAnd, "RightSeq"))
        .unwrap();
    let fd_sl = frag_diam
        .add_child(fdl, fd_shared_leaf, Node::step("SharedLeaf"))
        .unwrap();
    frag_diam.link(fdrg, fd_sl).unwrap();
    frag_diam
        .add_child(fdrg, fd_seq_tail, Node::step("SeqTail"))
        .unwrap();

    // Outer fragment `frag_outer`:
    // OuterSeq(SEQ) -> [BoundaryA (uses frag_diam), BoundaryB (uses frag_diam)]
    let frag_outer_id = TreeId::new_v7();
    let (fo_root, fo_bnd_a, fo_bnd_b) = (NodeId::new_v7(), NodeId::new_v7(), NodeId::new_v7());
    let mut frag_outer =
        AttackTree::new(fo_root, Node::gate(GateKind::SeqAnd, "OuterSeq")).unwrap();
    frag_outer
        .add_child(frag_outer.root(), fo_bnd_a, Node::step("BoundaryA"))
        .unwrap();
    frag_outer
        .add_child(frag_outer.root(), fo_bnd_b, Node::step("BoundaryB"))
        .unwrap();

    // Host Threat tree `host_id` with multi-parent ref `h_ref` + sibling consumer `sib_consumer_id`.
    let host_id = TreeId::new_v7();
    let (h_root, h_sib_l, h_ref, h_sib_r, h_extra, h_es0, h_es1) = (
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
    );
    let mut host = AttackTree::new(h_root, Node::gate(GateKind::Or, "HostRoot")).unwrap();
    let hr = host.root();
    host.add_child(hr, h_sib_l, Node::step("SibLeft")).unwrap();
    let h_ref_idx = host.add_child(hr, h_ref, Node::step("HostRef")).unwrap();
    host.add_child(hr, h_sib_r, Node::step("SibRight")).unwrap();
    let he_idx = host
        .add_child(hr, h_extra, Node::gate(GateKind::And, "ExtraParent"))
        .unwrap();
    host.add_child(he_idx, h_es0, Node::step("ExtraSib0"))
        .unwrap();
    host.link(he_idx, h_ref_idx).unwrap();
    host.add_child(he_idx, h_es1, Node::step("ExtraSib1"))
        .unwrap();

    let sib_consumer_id = TreeId::new_v7();
    let (sc_sib_l, sc_sib_r, sc_es0, sc_es1) = (
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
    );
    // Keep same child order as `host` so evaluation is directly comparable.
    let (sc_root2, sc_ref2, sc_extra2) = (NodeId::new_v7(), NodeId::new_v7(), NodeId::new_v7());
    let mut sib_con2 = AttackTree::new(sc_root2, Node::gate(GateKind::Or, "SibCon2")).unwrap();
    let sr2 = sib_con2.root();
    sib_con2
        .add_child(sr2, sc_sib_l, Node::step("SibLeft"))
        .unwrap();
    let sr2_ref = sib_con2
        .add_child(sr2, sc_ref2, Node::step("ConRef"))
        .unwrap();
    sib_con2
        .add_child(sr2, sc_sib_r, Node::step("SibRight"))
        .unwrap();
    let sr2_ex = sib_con2
        .add_child(sr2, sc_extra2, Node::gate(GateKind::And, "ConExtra"))
        .unwrap();
    sib_con2
        .add_child(sr2_ex, sc_es0, Node::step("ExtraSib0"))
        .unwrap();
    sib_con2.link(sr2_ex, sr2_ref).unwrap();
    sib_con2
        .add_child(sr2_ex, sc_es1, Node::step("ExtraSib1"))
        .unwrap();

    // Host root reference trees (Threat root ref + Fragment root ref with consumer).
    let root_threat_id = TreeId::new_v7();
    let rt_node = NodeId::new_v7();
    let root_threat = AttackTree::new(rt_node, Node::step("RootHostRef")).unwrap();

    let frag_host_id = TreeId::new_v7();
    let fh_node = NodeId::new_v7();
    let frag_host = AttackTree::new(fh_node, Node::step("FragRootRef")).unwrap();

    let frag_con_id = TreeId::new_v7();
    let (fc_root, fc_ref) = (NodeId::new_v7(), NodeId::new_v7());
    let mut frag_con = AttackTree::new(fc_root, Node::gate(GateKind::Or, "FragCon")).unwrap();
    frag_con
        .add_child(frag_con.root(), fc_ref, Node::step("UsesFragHost"))
        .unwrap();

    // Single-step root target fragments: plain rated step AND SharedStep with factors/via/type.
    let frag_single_step_id = TreeId::new_v7();
    let fss_node = NodeId::new_v7();
    let frag_single_step = AttackTree::new(fss_node, Node::step("SingleStepTarget")).unwrap();

    let frag_single_shared_id = TreeId::new_v7();
    let fssh_node = NodeId::new_v7();
    let frag_single_shared = AttackTree::new(fssh_node, Node::step("SingleSharedTarget")).unwrap();

    let single_host_id = TreeId::new_v7();
    let (sh_root, sh_plain) = (NodeId::new_v7(), NodeId::new_v7());
    let mut single_host = AttackTree::new(sh_root, Node::gate(GateKind::Or, "SingleHost")).unwrap();
    single_host
        .add_child(single_host.root(), sh_plain, Node::step("PlainHostSlot"))
        .unwrap();

    let single_shared_host_id = TreeId::new_v7();
    let (ssh_root, sh_shared) = (NodeId::new_v7(), NodeId::new_v7());
    let mut single_shared_host =
        AttackTree::new(ssh_root, Node::gate(GateKind::Or, "SingleSharedHost")).unwrap();
    single_shared_host
        .add_child(
            single_shared_host.root(),
            sh_shared,
            Node::step("SharedHostSlot"),
        )
        .unwrap();

    // Unrated single-step fragment & isolated host, excluded multi-leaf fragment, and unrated multi-leaf fragment.
    let frag_unrated_id = TreeId::new_v7();
    let fur_node = NodeId::new_v7();
    let frag_unrated = AttackTree::new(fur_node, Node::step("UnratedSingle")).unwrap();

    let unrated_single_host_id = TreeId::new_v7();
    let (ush_root, sh_unrated) = (NodeId::new_v7(), NodeId::new_v7());
    let mut unrated_single_host =
        AttackTree::new(ush_root, Node::gate(GateKind::Or, "UnratedSingleHost")).unwrap();
    unrated_single_host
        .add_child(
            unrated_single_host.root(),
            sh_unrated,
            Node::step("UnratedHostSlot"),
        )
        .unwrap();

    let frag_excl_id = TreeId::new_v7();
    let (fe_root, fe_cond, fe_via, fe_factor, fe_assump, fe_ok) = (
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
    );
    let mut frag_excl = AttackTree::new(fe_root, Node::gate(GateKind::Or, "ExclRoot")).unwrap();
    let fer = frag_excl.root();
    for (nid, lbl) in [
        (fe_cond, "ExCond"),
        (fe_via, "ExVia"),
        (fe_factor, "ExFactor"),
        (fe_assump, "ExAssump"),
        (fe_ok, "ExOk"),
    ] {
        frag_excl.add_child(fer, nid, Node::step(lbl)).unwrap();
    }
    let excl_host_id = TreeId::new_v7();
    let (eh_root, eh_ref) = (NodeId::new_v7(), NodeId::new_v7());
    let mut excl_host = AttackTree::new(eh_root, Node::gate(GateKind::Or, "ExclHost")).unwrap();
    excl_host
        .add_child(excl_host.root(), eh_ref, Node::step("ExclSlot"))
        .unwrap();

    let frag_unrated_multi_id = TreeId::new_v7();
    let (fum_root, fum_unrated, fum_rated) = (NodeId::new_v7(), NodeId::new_v7(), NodeId::new_v7());
    let mut frag_unrated_multi =
        AttackTree::new(fum_root, Node::gate(GateKind::Or, "UnratedMultiRoot")).unwrap();
    frag_unrated_multi
        .add_child(
            frag_unrated_multi.root(),
            fum_unrated,
            Node::step("ExUnrated"),
        )
        .unwrap();
    frag_unrated_multi
        .add_child(
            frag_unrated_multi.root(),
            fum_rated,
            Node::step("ExRatedSib"),
        )
        .unwrap();
    let unrated_multi_host_id = TreeId::new_v7();
    let (umh_root, umh_ref) = (NodeId::new_v7(), NodeId::new_v7());
    let mut unrated_multi_host =
        AttackTree::new(umh_root, Node::gate(GateKind::Or, "UnratedMultiHost")).unwrap();
    unrated_multi_host
        .add_child(
            unrated_multi_host.root(),
            umh_ref,
            Node::step("UnratedMultiSlot"),
        )
        .unwrap();

    // >32-node target fragment (40 nodes) and depth-8 chain fragment.
    let big_frag_id = TreeId::new_v7();
    let (bf_root, bf_gate) = (NodeId::new_v7(), NodeId::new_v7());
    let mut big_frag = AttackTree::new(bf_root, Node::gate(GateKind::Or, "BigFrag")).unwrap();
    let bfg = big_frag
        .add_child(
            big_frag.root(),
            bf_gate,
            Node::gate(GateKind::SeqAnd, "BigSeq"),
        )
        .unwrap();
    let mut bf_leaves = Vec::new();
    for idx in 0..38 {
        let lid = NodeId::new_v7();
        big_frag
            .add_child(bfg, lid, Node::step(format!("BFLeaf {idx}")))
            .unwrap();
        bf_leaves.push(lid);
    }
    let big_host_id = TreeId::new_v7();
    let (bh_root, bh_ref) = (NodeId::new_v7(), NodeId::new_v7());
    let mut big_host = AttackTree::new(bh_root, Node::gate(GateKind::Or, "BigHost")).unwrap();
    big_host
        .add_child(big_host.root(), bh_ref, Node::step("BigRef"))
        .unwrap();

    // Standalone shared step + plain detach comparison tree.
    let plain_shared_host_id = TreeId::new_v7();
    let (psh_root, psh_shared, psh_detach) = (NodeId::new_v7(), NodeId::new_v7(), NodeId::new_v7());
    let mut plain_shared_host =
        AttackTree::new(psh_root, Node::gate(GateKind::Or, "SharedHost")).unwrap();
    plain_shared_host
        .add_child(
            plain_shared_host.root(),
            psh_shared,
            Node::step("UsesShared"),
        )
        .unwrap();
    plain_shared_host
        .add_child(
            plain_shared_host.root(),
            psh_detach,
            Node::step("UsesDetach"),
        )
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
            component: comp_id,
            kind: iface_kind,
            name: "CAN".into(),
            provenance: p.clone(),
        },
        Op::AddComponent {
            id: comp_off_id,
            name: "OffECU".into(),
            parent: None,
            provenance: p.clone(),
        },
        Op::AddAsset {
            id: asset_id,
            name: "Asset".into(),
            properties: m.all_properties(),
            provenance: p.clone(),
        },
        Op::AddThreatScenario {
            id: threat_main_id,
            name: "MainThreat".into(),
            asset: asset_id,
            category: m.threat_categories().next().unwrap(),
            catalog_refs: vec![],
            damages: vec![],
            provenance: p.clone(),
        },
        Op::AddThreatScenario {
            id: threat_root_id,
            name: "RootThreat".into(),
            asset: asset_id,
            category: m.threat_categories().next().unwrap(),
            catalog_refs: vec![],
            damages: vec![],
            provenance: p.clone(),
        },
        Op::AddCondition {
            id: cond_host_tree,
            name: "HostTreeCond".into(),
            description: "".into(),
            present: true,
            provenance: p.clone(),
        },
        Op::AddCondition {
            id: cond_host_local,
            name: "HostLocalCond".into(),
            description: "".into(),
            present: true,
            provenance: p.clone(),
        },
        Op::AddCondition {
            id: cond_outer_tree,
            name: "OuterTreeCond".into(),
            description: "".into(),
            present: true,
            provenance: p.clone(),
        },
        Op::AddCondition {
            id: cond_bnd_a,
            name: "BndACond".into(),
            description: "".into(),
            present: true,
            provenance: p.clone(),
        },
        Op::AddCondition {
            id: cond_bnd_b,
            name: "BndBCond".into(),
            description: "".into(),
            present: true,
            provenance: p.clone(),
        },
        Op::AddCondition {
            id: cond_frag_tree,
            name: "FragTreeCond".into(),
            description: "".into(),
            present: true,
            provenance: p.clone(),
        },
        Op::AddCondition {
            id: cond_leaf_on,
            name: "LeafCond".into(),
            description: "".into(),
            present: true,
            provenance: p.clone(),
        },
        Op::AddCondition {
            id: cond_off,
            name: "CondOff".into(),
            description: "".into(),
            present: false,
            provenance: p.clone(),
        },
        Op::AddCondition {
            id: cond_assump,
            name: "AssumpCond".into(),
            description: "".into(),
            present: true,
            provenance: p.clone(),
        },
        Op::NeedCondition {
            element: ElementRef::Component(comp_off_id),
            condition: cond_off,
            provenance: p.clone(),
        },
        Op::AddSecurityControl {
            id: ctrl_same,
            name: "CtrlSame".into(),
            provenance: p.clone(),
        },
        Op::SetControlStatus {
            control: ctrl_same,
            status: ControlStatus::Implemented,
            provenance: p.clone(),
        },
        Op::AddSecurityControl {
            id: ctrl_host_plan,
            name: "CtrlHostPlan".into(),
            provenance: p.clone(),
        },
        Op::SetControlStatus {
            control: ctrl_host_plan,
            status: ControlStatus::Planned,
            provenance: p.clone(),
        },
        Op::AddSecurityControl {
            id: ctrl_inner_plan,
            name: "CtrlInnerPlan".into(),
            provenance: p.clone(),
        },
        Op::SetControlStatus {
            control: ctrl_inner_plan,
            status: ControlStatus::Planned,
            provenance: p.clone(),
        },
        Op::AddSecurityControl {
            id: ctrl_off,
            name: "CtrlOff".into(),
            provenance: p.clone(),
        },
        Op::SetControlStatus {
            control: ctrl_off,
            status: ControlStatus::Off,
            provenance: p.clone(),
        },
        Op::AddSecurityControl {
            id: ctrl_via,
            name: "CtrlVia".into(),
            provenance: p.clone(),
        },
        Op::SetControlStatus {
            control: ctrl_via,
            status: ControlStatus::Planned,
            provenance: p.clone(),
        },
        Op::SetControlRule {
            control: ctrl_via,
            attack_type: atk_type,
            ceiling: Some(low),
            provenance: p.clone(),
        },
        Op::ApplyControl {
            control: ctrl_via,
            target: ItemRef::Component(comp_id),
            provenance: p.clone(),
        },
        Op::AddAssumption {
            id: assump_host,
            name: "AssumpHost".into(),
            holds: false,
            provenance: p.clone(),
        },
        Op::NeedCondition {
            element: ElementRef::Assumption(assump_host),
            condition: cond_assump,
            provenance: p.clone(),
        },
        Op::AddAssumption {
            id: assump_frag_tree,
            name: "AssumpFragTree".into(),
            holds: true,
            provenance: p.clone(),
        },
        Op::SetAttackFilter {
            filter: AttackFilter::default().without(factor, flevel_excl),
            provenance: p.clone(),
        },
        Op::AddSharedStep {
            id: shared_rich_id,
            name: "SharedRich".into(),
            provenance: p.clone(),
        },
        Op::SetStepFactor {
            target: StepTarget::Shared(shared_rich_id),
            factor,
            level: Some(flevel_ok),
            provenance: p.clone(),
        },
        Op::SetStepFeasibility {
            target: StepTarget::Shared(shared_rich_id),
            level: Some(high),
            provenance: p.clone(),
        },
        Op::SetStepVia {
            target: StepTarget::Shared(shared_rich_id),
            via: Some(ItemRef::Interface(iface_id)),
            provenance: p.clone(),
        },
        Op::SetStepAttackType {
            target: StepTarget::Shared(shared_rich_id),
            attack_type: Some(atk_type),
            provenance: p.clone(),
        },
        Op::AddSharedStep {
            id: shared_plain_id,
            name: "SharedPlain".into(),
            provenance: p.clone(),
        },
        Op::SetStepFeasibility {
            target: StepTarget::Shared(shared_plain_id),
            level: Some(med),
            provenance: p.clone(),
        },
        Op::AddAttackTree {
            role: TreeRole::Fragment,
            id: frag_diam_id,
            tree: frag_diam,
            step_feasibility: vec![(fd_shared_leaf, high)],
            provenance: p.clone(),
        },
        Op::UseSharedStep {
            tree: frag_diam_id,
            node: fd_seq_tail,
            step: shared_rich_id,
            provenance: p.clone(),
        },
        Op::NeedCondition {
            element: ElementRef::Tree(frag_diam_id),
            condition: cond_frag_tree,
            provenance: p.clone(),
        },
        Op::SetStepMitigation {
            tree: frag_diam_id,
            node: fd_shared_leaf,
            control: ctrl_same,
            feasibility: Some(high),
            provenance: p.clone(),
        },
        Op::SetStepMitigation {
            tree: frag_diam_id,
            node: fd_shared_leaf,
            control: ctrl_inner_plan,
            feasibility: Some(med),
            provenance: p.clone(),
        },
        Op::NeedStepCondition {
            tree: frag_diam_id,
            node: fd_seq_tail,
            condition: cond_leaf_on,
            provenance: p.clone(),
        },
        Op::AddAttackTree {
            role: TreeRole::Fragment,
            id: frag_outer_id,
            tree: frag_outer,
            step_feasibility: vec![],
            provenance: p.clone(),
        },
        Op::NeedCondition {
            element: ElementRef::Tree(frag_outer_id),
            condition: cond_outer_tree,
            provenance: p.clone(),
        },
        Op::UseFragment {
            tree: frag_outer_id,
            node: fo_bnd_a,
            fragment: frag_diam_id,
            provenance: p.clone(),
        },
        Op::UseFragment {
            tree: frag_outer_id,
            node: fo_bnd_b,
            fragment: frag_diam_id,
            provenance: p.clone(),
        },
        Op::SetStepMitigation {
            tree: frag_outer_id,
            node: fo_bnd_a,
            control: ctrl_same,
            feasibility: Some(low),
            provenance: p.clone(),
        },
        Op::SetStepMitigation {
            tree: frag_outer_id,
            node: fo_bnd_a,
            control: ctrl_off,
            feasibility: Some(low),
            provenance: p.clone(),
        },
        Op::NeedStepCondition {
            tree: frag_outer_id,
            node: fo_bnd_a,
            condition: cond_bnd_a,
            provenance: p.clone(),
        },
        Op::SetReviewMark {
            tree: frag_outer_id,
            node: fo_bnd_a,
            marked: true,
            provenance: p.clone(),
        },
        Op::SetStepMitigation {
            tree: frag_outer_id,
            node: fo_bnd_b,
            control: ctrl_same,
            feasibility: Some(med),
            provenance: p.clone(),
        },
        Op::NeedStepCondition {
            tree: frag_outer_id,
            node: fo_bnd_b,
            condition: cond_bnd_b,
            provenance: p.clone(),
        },
        Op::AddAttackTree {
            role: TreeRole::Threat,
            id: host_id,
            tree: host,
            step_feasibility: vec![(h_sib_l, low), (h_sib_r, low), (h_es0, high), (h_es1, high)],
            provenance: p.clone(),
        },
        Op::SetThreatAttackTree {
            threat: threat_main_id,
            tree: Some(host_id),
            provenance: p.clone(),
        },
        Op::SetDescription {
            target: ElementRef::Tree(host_id),
            text: "Host desc".into(),
            provenance: p.clone(),
        },
        Op::SetDescriptionTranslation {
            target: ElementRef::Tree(host_id),
            lang: "ja".into(),
            text: "ホスト".into(),
            provenance: p.clone(),
        },
        Op::NeedCondition {
            element: ElementRef::Tree(host_id),
            condition: cond_host_tree,
            provenance: p.clone(),
        },
        Op::UseFragment {
            tree: host_id,
            node: h_ref,
            fragment: frag_outer_id,
            provenance: p.clone(),
        },
        Op::NeedStepCondition {
            tree: host_id,
            node: h_ref,
            condition: cond_host_local,
            provenance: p.clone(),
        },
        Op::SetStepMitigation {
            tree: host_id,
            node: h_ref,
            control: ctrl_same,
            feasibility: Some(med),
            provenance: p.clone(),
        },
        Op::SetStepMitigation {
            tree: host_id,
            node: h_ref,
            control: ctrl_host_plan,
            feasibility: Some(low),
            provenance: p.clone(),
        },
        Op::AddAttackTree {
            role: TreeRole::Threat,
            id: sib_consumer_id,
            tree: sib_con2,
            step_feasibility: vec![
                (sc_sib_l, low),
                (sc_sib_r, low),
                (sc_es0, high),
                (sc_es1, high),
            ],
            provenance: p.clone(),
        },
        Op::UseFragment {
            tree: sib_consumer_id,
            node: sc_ref2,
            fragment: frag_outer_id,
            provenance: p.clone(),
        },
        Op::NeedStepCondition {
            tree: sib_consumer_id,
            node: sc_ref2,
            condition: cond_host_local,
            provenance: p.clone(),
        },
        Op::SetStepMitigation {
            tree: sib_consumer_id,
            node: sc_ref2,
            control: ctrl_same,
            feasibility: Some(med),
            provenance: p.clone(),
        },
        Op::SetStepMitigation {
            tree: sib_consumer_id,
            node: sc_ref2,
            control: ctrl_host_plan,
            feasibility: Some(low),
            provenance: p.clone(),
        },
        Op::SetAssumptionExcludes {
            id: assump_host,
            trees: vec![],
            steps: vec![
                StepRef {
                    tree: host_id,
                    node: h_ref,
                },
                StepRef {
                    tree: sib_consumer_id,
                    node: sc_ref2,
                },
            ],
            provenance: p.clone(),
        },
        Op::AddAttackTree {
            role: TreeRole::Threat,
            id: root_threat_id,
            tree: root_threat,
            step_feasibility: vec![],
            provenance: p.clone(),
        },
        Op::SetThreatAttackTree {
            threat: threat_root_id,
            tree: Some(root_threat_id),
            provenance: p.clone(),
        },
        Op::SetDescription {
            target: ElementRef::Tree(root_threat_id),
            text: "Root threat desc".into(),
            provenance: p.clone(),
        },
        Op::NeedCondition {
            element: ElementRef::Tree(root_threat_id),
            condition: cond_host_tree,
            provenance: p.clone(),
        },
        Op::UseFragment {
            tree: root_threat_id,
            node: rt_node,
            fragment: frag_outer_id,
            provenance: p.clone(),
        },
        Op::SetReviewMark {
            tree: root_threat_id,
            node: rt_node,
            marked: true,
            provenance: p.clone(),
        },
        Op::AddAttackTree {
            role: TreeRole::Fragment,
            id: frag_host_id,
            tree: frag_host,
            step_feasibility: vec![],
            provenance: p.clone(),
        },
        Op::UseFragment {
            tree: frag_host_id,
            node: fh_node,
            fragment: frag_diam_id,
            provenance: p.clone(),
        },
        Op::AddAttackTree {
            role: TreeRole::Threat,
            id: frag_con_id,
            tree: frag_con,
            step_feasibility: vec![],
            provenance: p.clone(),
        },
        Op::UseFragment {
            tree: frag_con_id,
            node: fc_ref,
            fragment: frag_host_id,
            provenance: p.clone(),
        },
        Op::AddAttackTree {
            role: TreeRole::Fragment,
            id: frag_single_step_id,
            tree: frag_single_step,
            step_feasibility: vec![(fss_node, high)],
            provenance: p.clone(),
        },
        Op::SetStepFactor {
            target: StepTarget::Step(frag_single_step_id, fss_node),
            factor,
            level: Some(flevel_ok),
            provenance: p.clone(),
        },
        Op::SetStepVia {
            target: StepTarget::Step(frag_single_step_id, fss_node),
            via: Some(ItemRef::Interface(iface_id)),
            provenance: p.clone(),
        },
        Op::SetStepAttackType {
            target: StepTarget::Step(frag_single_step_id, fss_node),
            attack_type: Some(atk_type),
            provenance: p.clone(),
        },
        Op::NeedCondition {
            element: ElementRef::Tree(frag_single_step_id),
            condition: cond_frag_tree,
            provenance: p.clone(),
        },
        Op::SetStepMitigation {
            tree: frag_single_step_id,
            node: fss_node,
            control: ctrl_same,
            feasibility: Some(high),
            provenance: p.clone(),
        },
        Op::AddAttackTree {
            role: TreeRole::Fragment,
            id: frag_single_shared_id,
            tree: frag_single_shared,
            step_feasibility: vec![],
            provenance: p.clone(),
        },
        Op::UseSharedStep {
            tree: frag_single_shared_id,
            node: fssh_node,
            step: shared_rich_id,
            provenance: p.clone(),
        },
        Op::AddAttackTree {
            role: TreeRole::Fragment,
            id: frag_unrated_id,
            tree: frag_unrated,
            step_feasibility: vec![],
            provenance: p.clone(),
        },
        Op::AddAttackTree {
            role: TreeRole::Threat,
            id: single_host_id,
            tree: single_host,
            step_feasibility: vec![],
            provenance: p.clone(),
        },
        Op::UseFragment {
            tree: single_host_id,
            node: sh_plain,
            fragment: frag_single_step_id,
            provenance: p.clone(),
        },
        Op::NeedStepCondition {
            tree: single_host_id,
            node: sh_plain,
            condition: cond_host_local,
            provenance: p.clone(),
        },
        Op::SetStepMitigation {
            tree: single_host_id,
            node: sh_plain,
            control: ctrl_same,
            feasibility: Some(low),
            provenance: p.clone(),
        },
        Op::SetReviewMark {
            tree: single_host_id,
            node: sh_plain,
            marked: true,
            provenance: p.clone(),
        },
        Op::AddAttackTree {
            role: TreeRole::Threat,
            id: single_shared_host_id,
            tree: single_shared_host,
            step_feasibility: vec![],
            provenance: p.clone(),
        },
        Op::UseFragment {
            tree: single_shared_host_id,
            node: sh_shared,
            fragment: frag_single_shared_id,
            provenance: p.clone(),
        },
        Op::AddAttackTree {
            role: TreeRole::Threat,
            id: unrated_single_host_id,
            tree: unrated_single_host,
            step_feasibility: vec![],
            provenance: p.clone(),
        },
        Op::UseFragment {
            tree: unrated_single_host_id,
            node: sh_unrated,
            fragment: frag_unrated_id,
            provenance: p.clone(),
        },
        Op::AddAttackTree {
            role: TreeRole::Fragment,
            id: frag_excl_id,
            tree: frag_excl,
            step_feasibility: vec![
                (fe_cond, high),
                (fe_via, high),
                (fe_factor, high),
                (fe_assump, med),
                (fe_ok, low),
            ],
            provenance: p.clone(),
        },
        Op::NeedStepCondition {
            tree: frag_excl_id,
            node: fe_cond,
            condition: cond_off,
            provenance: p.clone(),
        },
        Op::SetStepVia {
            target: StepTarget::Step(frag_excl_id, fe_via),
            via: Some(ItemRef::Component(comp_off_id)),
            provenance: p.clone(),
        },
        Op::SetStepFactor {
            target: StepTarget::Step(frag_excl_id, fe_factor),
            factor,
            level: Some(flevel_excl),
            provenance: p.clone(),
        },
        Op::SetAssumptionExcludes {
            id: assump_frag_tree,
            trees: vec![],
            steps: vec![StepRef {
                tree: frag_excl_id,
                node: fe_assump,
            }],
            provenance: p.clone(),
        },
        Op::AddAttackTree {
            role: TreeRole::Threat,
            id: excl_host_id,
            tree: excl_host,
            step_feasibility: vec![],
            provenance: p.clone(),
        },
        Op::UseFragment {
            tree: excl_host_id,
            node: eh_ref,
            fragment: frag_excl_id,
            provenance: p.clone(),
        },
        Op::AddAttackTree {
            role: TreeRole::Fragment,
            id: frag_unrated_multi_id,
            tree: frag_unrated_multi,
            step_feasibility: vec![(fum_rated, low)],
            provenance: p.clone(),
        },
        Op::AddAttackTree {
            role: TreeRole::Threat,
            id: unrated_multi_host_id,
            tree: unrated_multi_host,
            step_feasibility: vec![],
            provenance: p.clone(),
        },
        Op::UseFragment {
            tree: unrated_multi_host_id,
            node: umh_ref,
            fragment: frag_unrated_multi_id,
            provenance: p.clone(),
        },
        Op::AddAttackTree {
            role: TreeRole::Threat,
            id: big_host_id,
            tree: big_host,
            step_feasibility: vec![],
            provenance: p.clone(),
        },
        Op::AddAttackTree {
            role: TreeRole::Threat,
            id: plain_shared_host_id,
            tree: plain_shared_host,
            step_feasibility: vec![],
            provenance: p.clone(),
        },
        Op::UseSharedStep {
            tree: plain_shared_host_id,
            node: psh_shared,
            step: shared_plain_id,
            provenance: p.clone(),
        },
        Op::UseFragment {
            tree: plain_shared_host_id,
            node: psh_detach,
            fragment: frag_single_step_id,
            provenance: p.clone(),
        },
    ];
    // Add big_frag (40 nodes) + depth-8 chain fragment.
    setup.extend(add_tree_skeleton_ops(
        TreeRole::Fragment,
        big_frag_id,
        &big_frag,
        &p,
    ));
    for (idx, lid) in bf_leaves.iter().enumerate() {
        setup.push(Op::SetStepFeasibility {
            target: StepTarget::Step(big_frag_id, *lid),
            level: Some(if idx % 2 == 0 { low } else { high }),
            provenance: p.clone(),
        });
    }
    setup.push(Op::UseFragment {
        tree: big_host_id,
        node: bh_ref,
        fragment: big_frag_id,
        provenance: p.clone(),
    });
    let mut chain = vec![frag_single_step_id];
    for d in 2..=MAX_FRAGMENT_DEPTH {
        let tid = TreeId::new_v7();
        let rn = NodeId::new_v7();
        let mut ft = AttackTree::new(rn, Node::gate(GateKind::Or, format!("D{d}"))).unwrap();
        let ch = NodeId::new_v7();
        ft.add_child(ft.root(), ch, Node::step(format!("D{d}Child")))
            .unwrap();
        setup.push(Op::AddAttackTree {
            role: TreeRole::Fragment,
            id: tid,
            tree: ft,
            step_feasibility: vec![],
            provenance: p.clone(),
        });
        setup.push(Op::UseFragment {
            tree: tid,
            node: ch,
            fragment: *chain.last().unwrap(),
            provenance: p.clone(),
        });
        chain.push(tid);
    }
    let d8_host_id = TreeId::new_v7();
    let d8_node = NodeId::new_v7();
    setup.push(Op::AddAttackTree {
        role: TreeRole::Threat,
        id: d8_host_id,
        tree: AttackTree::new(d8_node, Node::step("D8HostRef")).unwrap(),
        step_feasibility: vec![],
        provenance: p.clone(),
    });
    setup.push(Op::UseFragment {
        tree: d8_host_id,
        node: d8_node,
        fragment: *chain.last().unwrap(),
        provenance: p.clone(),
    });

    for batch in setup.chunks(400) {
        setup_ws.apply(batch.to_vec(), Origin::Edit).expect("setup");
    }
    let snap = saved(setup_ws.project());
    let loaded_snap = Project::load(snap.as_slice(), Arc::clone(&m)).expect("snap load");
    let ws_files = loaded_snap.to_workspace_files().expect("to_ws");
    let loaded_proj = Project::from_workspace_files(
        ws_files
            .iter()
            .map(|f| (f.path.as_str(), f.bytes.as_slice())),
        Arc::clone(&m),
    )
    .expect("from_ws");

    let make_ws = || {
        let mut ws = Workspace::new(loaded_proj.clone(), 0);
        let prior_ops = Edit::RenameTreeNode {
            tree: host_id,
            node: h_sib_l,
            label: "SibLeft renamed".into(),
        }
        .to_ops(ws.project(), "prior-editor", 10)
        .expect("prior");
        ws.apply(prior_ops, Origin::Edit).expect("prior apply");
        ws
    };

    // 1. Nested diamond + differing boundary overlays + multi-parent + equal-control merging
    //    + live control/condition/assumption transitions + post-Unshare source mutation isolation.
    {
        let mut ws = make_ws();
        verify_unshare_fragment(&mut ws, host_id, h_ref);

        let p_after = ws.project();
        let h_idx = p_after.attack_tree_idx(&host_id).unwrap();
        let h_tree = p_after.attack_tree(h_idx).unwrap();
        let ref_idx = h_tree.idx_of(&h_ref).unwrap();
        assert_eq!(
            parent_positions(p_after, host_id, h_ref),
            vec![(h_root, 1), (h_extra, 1)]
        );
        let bnds: Vec<_> = h_tree
            .children(ref_idx)
            .iter()
            .map(|&c| (c, h_tree.get(c).unwrap().label()))
            .collect();
        assert_eq!(bnds.len(), 2);
        assert_eq!(bnds[0].1, "DiamRoot");
        assert_eq!(bnds[1].1, "DiamRoot");

        let ctrl_same_idx = p_after.security_control_idx(&ctrl_same).unwrap();
        let check_boundary = |bnd_idx: NodeIdx,
                              expect_same_ceil: FeasibilityLevel,
                              expect_mark: bool,
                              extra_cond: ConditionId| {
            let mut leaves = Vec::new();
            let mut stack = vec![bnd_idx];
            let mut seen = HashSet::new();
            while let Some(ni) = stack.pop() {
                if !seen.insert(ni) {
                    continue;
                }
                let node = h_tree.get(ni).unwrap();
                if node.gate_kind().is_none() {
                    leaves.push((ni, node.label()));
                } else {
                    for &c in h_tree.children(ni) {
                        stack.push(c);
                    }
                }
            }
            assert_eq!(leaves.len(), 2);
            for (li, lbl) in leaves {
                let lid = h_tree.id_of(li).unwrap();
                assert_eq!(p_after.review_marked(host_id, lid), expect_mark);
                let cond_ids: HashSet<ConditionId> = p_after
                    .step_condition_link_ids(host_id, lid)
                    .iter()
                    .filter_map(|&ci| p_after.condition_id(ci))
                    .collect();
                assert!(cond_ids.contains(&cond_host_local));
                assert!(cond_ids.contains(&cond_outer_tree));
                assert!(cond_ids.contains(&cond_frag_tree));
                assert!(cond_ids.contains(&extra_cond));
                let same_mit = p_after
                    .step_mitigations(h_idx, li)
                    .iter()
                    .find(|mt| mt.control() == ctrl_same_idx)
                    .expect("ctrl_same");
                assert_eq!(same_mit.feasibility(), expect_same_ceil);
                if lbl == "SeqTail" {
                    assert!(cond_ids.contains(&cond_leaf_on));
                    assert_eq!(
                        p_after.step_factors(h_idx, li).collect::<Vec<_>>(),
                        vec![(factor, flevel_ok)]
                    );
                    assert_eq!(
                        p_after.step_via(h_idx, li),
                        Some(ItemRef::Interface(iface_id))
                    );
                    assert_eq!(p_after.step_attack_type(h_idx, li), Some(atk_type));
                }
            }
        };
        check_boundary(bnds[0].0, low, true, cond_bnd_a);
        check_boundary(bnds[1].0, med, false, cond_bnd_b);

        // Active-control & exclusion transitions after Unshare match BOTH IndependentFold and sibling consumer.
        let transitions = vec![
            Edit::SetControlStatus {
                control: ctrl_host_plan,
                status: ControlStatus::Implemented,
            },
            Edit::SetControlStatus {
                control: ctrl_same,
                status: ControlStatus::Off,
            },
            Edit::SetConditionPresent {
                condition: cond_bnd_a,
                present: false,
            },
            Edit::SetAssumptionHolds {
                assumption: assump_host,
                holds: true,
            },
            Edit::SetAssumptionHolds {
                assumption: assump_host,
                holds: false,
            },
            Edit::SetConditionPresent {
                condition: cond_bnd_a,
                present: true,
            },
            Edit::SetConditionPresent {
                condition: cond_outer_tree,
                present: false,
            },
            Edit::SetConditionPresent {
                condition: cond_outer_tree,
                present: true,
            },
            Edit::SetControlStatus {
                control: ctrl_same,
                status: ControlStatus::Implemented,
            },
            Edit::SetControlStatus {
                control: ctrl_host_plan,
                status: ControlStatus::Planned,
            },
        ];
        for (step_no, ed) in transitions.into_iter().enumerate() {
            let tops = ed
                .to_ops(ws.project(), "trans", 500 + step_no as u64)
                .unwrap();
            ws.apply(tops, Origin::Edit).unwrap();
            assert_fold_matches(ws.project(), host_id, sib_consumer_id);
        }

        // Source rating and structure edits AFTER Unshare do NOT alter copied host structure or values.
        let before_host_eval = eval_outcome_pair(&ws, host_id);
        let src_edits = vec![
            Op::SetStepFeasibility {
                target: StepTarget::Shared(shared_rich_id),
                level: Some(low),
                provenance: p.clone(),
            },
            Op::SetStepVia {
                target: StepTarget::Shared(shared_rich_id),
                via: None,
                provenance: p.clone(),
            },
            Op::SetStepMitigation {
                tree: frag_diam_id,
                node: fd_shared_leaf,
                control: ctrl_same,
                feasibility: Some(low),
                provenance: p.clone(),
            },
            Op::AddTreeNode {
                tree: frag_diam_id,
                parent: fd_left,
                id: NodeId::new_v7(),
                node: Node::step("ExtraSourceLeaf"),
                index: 5,
                position: 1,
                provenance: p.clone(),
            },
            Op::SetTreeNodeGate {
                tree: frag_outer_id,
                id: fo_root,
                gate: GateKind::And,
                provenance: p.clone(),
            },
        ];
        ws.apply(src_edits, Origin::Edit).expect("source mutations");
        assert_eq!(eval_outcome_pair(&ws, host_id), before_host_eval);
        let mut oracle = IndependentFold::new(ws.project());
        assert_eq!(
            (
                oracle.fold_tree(host_id, false),
                oracle.fold_tree(host_id, true)
            ),
            before_host_eval
        );
    }

    // 2. Host ROOT fragment references (Threat root + Fragment root with consumer).
    for (tree_id, node_id) in [(root_threat_id, rt_node), (frag_host_id, fh_node)] {
        let mut ws = make_ws();
        verify_unshare_fragment(&mut ws, tree_id, node_id);
        if tree_id == frag_host_id {
            assert_fold_matches(ws.project(), frag_con_id, frag_host_id);
        }
    }

    // 3. Target single-Step roots stay host Steps with raw fields & merged overlays; unrated isolated and copied unrated.
    {
        let mut ws = make_ws();
        verify_unshare_fragment(&mut ws, single_host_id, sh_plain);
        verify_unshare_fragment(&mut ws, single_shared_host_id, sh_shared);
        verify_unshare_fragment(&mut ws, unrated_single_host_id, sh_unrated);

        assert_eq!(
            eval_outcome_pair(&ws, single_host_id),
            (FoldOutcome::Value(low), FoldOutcome::Value(low))
        );
        assert_eq!(
            eval_outcome_pair(&ws, single_shared_host_id),
            (FoldOutcome::Value(high), FoldOutcome::Value(low))
        );
        assert_eq!(
            eval_outcome_pair(&ws, unrated_single_host_id),
            (FoldOutcome::Unrated, FoldOutcome::Unrated)
        );

        let proj = ws.project();
        let ctrl_same_idx = proj.security_control_idx(&ctrl_same).unwrap();
        for (tree_id, nid, expect_feas, expect_overlay) in [
            (single_host_id, sh_plain, Some(high), true),
            (single_shared_host_id, sh_shared, Some(high), false),
            (unrated_single_host_id, sh_unrated, None, false),
        ] {
            let si = proj.attack_tree_idx(&tree_id).unwrap();
            let st = proj.attack_tree(si).unwrap();
            let ni = st.idx_of(&nid).unwrap();
            assert_eq!(st.get(ni).unwrap().gate_kind(), None);
            assert_eq!(proj.step_fragment(si, ni), None);
            assert_eq!(proj.step_shared(si, ni), None);
            assert_eq!(proj.step_feasibility(si, ni), expect_feas);
            if expect_feas.is_some() {
                assert_eq!(
                    proj.step_factors(si, ni).collect::<Vec<_>>(),
                    vec![(factor, flevel_ok)]
                );
                assert_eq!(proj.step_via(si, ni), Some(ItemRef::Interface(iface_id)));
                assert_eq!(proj.step_attack_type(si, ni), Some(atk_type));
            } else {
                assert_eq!(proj.step_factors(si, ni).next(), None);
                assert_eq!(proj.step_via(si, ni), None);
                assert_eq!(proj.step_attack_type(si, ni), None);
            }
            assert_eq!(proj.review_marked(tree_id, nid), expect_overlay);
            let conds: HashSet<ConditionId> = proj
                .step_condition_link_ids(tree_id, nid)
                .iter()
                .filter_map(|&ci| proj.condition_id(ci))
                .collect();
            let mits = proj.step_mitigations(si, ni);
            if expect_overlay {
                assert_eq!(conds, HashSet::from([cond_host_local, cond_frag_tree]));
                assert_eq!(mits.len(), 1);
                assert_eq!(mits[0].control(), ctrl_same_idx);
                assert_eq!(mits[0].feasibility(), low);
            } else {
                assert!(conds.is_empty());
                assert!(mits.is_empty());
            }
        }

        // Verify distributed conditions and merged ceiling drive single_host evaluation.
        for (cond_id, present, expected) in [
            (
                cond_frag_tree,
                false,
                (FoldOutcome::Excluded, FoldOutcome::Excluded),
            ),
            (
                cond_frag_tree,
                true,
                (FoldOutcome::Value(low), FoldOutcome::Value(low)),
            ),
            (
                cond_host_local,
                false,
                (FoldOutcome::Excluded, FoldOutcome::Excluded),
            ),
            (
                cond_host_local,
                true,
                (FoldOutcome::Value(low), FoldOutcome::Value(low)),
            ),
        ] {
            let tops = Edit::SetConditionPresent {
                condition: cond_id,
                present,
            }
            .to_ops(ws.project(), "single-cond", 200)
            .unwrap();
            ws.apply(tops, Origin::Edit).unwrap();
            assert_eq!(eval_outcome_pair(&ws, single_host_id), expected);
            let mut oracle = IndependentFold::new(ws.project());
            assert_eq!(
                (
                    oracle.fold_tree(single_host_id, false),
                    oracle.fold_tree(single_host_id, true)
                ),
                expected
            );
        }
    }

    // 4. Excluded multi-leaf fragment proves all four exclusion mechanisms survive; unrated multi-leaf stays isolated.
    {
        let mut ws = make_ws();
        verify_unshare_fragment(&mut ws, excl_host_id, eh_ref);
        assert!(ws.project().attack_tree_idx(&frag_excl_id).is_some());
        assert_eq!(
            eval_outcome_pair(&ws, excl_host_id),
            (FoldOutcome::Value(low), FoldOutcome::Value(low))
        );

        let proj = ws.project();
        let ei = proj.attack_tree_idx(&excl_host_id).unwrap();
        let et = proj.attack_tree(ei).unwrap();
        let find_leaf = |lbl: &str| {
            et.iter()
                .find_map(|(ni, nid, n)| (n.label() == lbl).then_some((ni, nid)))
                .unwrap()
        };
        let (ex_cond_idx, ex_cond_id) = find_leaf("ExCond");
        let (ex_via_idx, _) = find_leaf("ExVia");
        let (ex_factor_idx, _) = find_leaf("ExFactor");
        let (_, ex_assump_id) = find_leaf("ExAssump");
        let (ex_ok_idx, _) = find_leaf("ExOk");
        assert_eq!(proj.step_feasibility(ei, ex_cond_idx), Some(high));
        assert_eq!(
            proj.step_condition_link_ids(excl_host_id, ex_cond_id)
                .iter()
                .filter_map(|&ci| proj.condition_id(ci))
                .collect::<Vec<_>>(),
            vec![cond_off]
        );
        assert_eq!(
            proj.step_via(ei, ex_via_idx),
            Some(ItemRef::Component(comp_off_id))
        );
        assert_eq!(
            proj.step_factors(ei, ex_factor_idx).collect::<Vec<_>>(),
            vec![(factor, flevel_excl)]
        );
        assert_eq!(proj.step_feasibility(ei, ex_ok_idx), Some(low));

        let aft_idx = proj.assumption_idx(&assump_frag_tree).unwrap();
        let aft = proj.assumption(aft_idx).unwrap();
        assert!(aft.excludes_steps().contains(&StepRef {
            tree: frag_excl_id,
            node: fe_assump,
        }));
        assert!(aft.excludes_steps().contains(&StepRef {
            tree: excl_host_id,
            node: ex_assump_id,
        }));

        // Lifting assump_frag_tree raises excl_host_id from low to med (ExAssump) while cond/via/factor (high) stay excluded.
        let lift_ops = Edit::SetAssumptionHolds {
            assumption: assump_frag_tree,
            holds: false,
        }
        .to_ops(ws.project(), "lift-assump", 201)
        .unwrap();
        ws.apply(lift_ops, Origin::Edit).unwrap();
        assert_eq!(
            eval_outcome_pair(&ws, excl_host_id),
            (FoldOutcome::Value(med), FoldOutcome::Value(med))
        );
        let mut oracle = IndependentFold::new(ws.project());
        assert_eq!(
            (
                oracle.fold_tree(excl_host_id, false),
                oracle.fold_tree(excl_host_id, true)
            ),
            (FoldOutcome::Value(med), FoldOutcome::Value(med))
        );

        // Unrated multi-leaf fragment (last user!) copies unrated state without inventing values.
        verify_unshare_fragment(&mut ws, unrated_multi_host_id, umh_ref);
        assert!(
            ws.project()
                .attack_tree_idx(&frag_unrated_multi_id)
                .is_some()
        );
        let ui = ws
            .project()
            .attack_tree_idx(&unrated_multi_host_id)
            .unwrap();
        let ut = ws.project().attack_tree(ui).unwrap();
        let unrated_idx = ut
            .iter()
            .find_map(|(ni, _, n)| (n.label() == "ExUnrated").then_some(ni))
            .unwrap();
        let rated_idx = ut
            .iter()
            .find_map(|(ni, _, n)| (n.label() == "ExRatedSib").then_some(ni))
            .unwrap();
        assert_eq!(ws.project().step_feasibility(ui, unrated_idx), None);
        assert_eq!(ws.project().step_factors(ui, unrated_idx).next(), None);
        assert_eq!(ws.project().step_via(ui, unrated_idx), None);
        assert_eq!(ws.project().step_attack_type(ui, unrated_idx), None);
        assert_eq!(ws.project().step_feasibility(ui, rated_idx), Some(low));
        assert_eq!(
            eval_outcome_pair(&ws, unrated_multi_host_id),
            (FoldOutcome::Unrated, FoldOutcome::Unrated)
        );
    }

    // 5. >32-node target fragment AND exact depth-8 Unshare.
    {
        let mut ws = make_ws();
        verify_unshare_fragment(&mut ws, big_host_id, bh_ref);
        let bi = ws.project().attack_tree_idx(&big_host_id).unwrap();
        assert!(ws.project().attack_tree(bi).unwrap().len() > 32);
        verify_unshare_fragment(&mut ws, d8_host_id, d8_node);
    }

    // 6. Existing SharedStep Unshare and plain 1-op Edit::UnuseFragment remain intact.
    {
        let mut ws = make_ws();
        let sh_ops = Edit::Unshare {
            tree: plain_shared_host_id,
            node: psh_shared,
        }
        .to_ops(ws.project(), "sh-unshare", 99)
        .expect("shared unshare");
        ws.apply(sh_ops, Origin::Edit)
            .expect("apply shared unshare");
        let pi = ws.project().attack_tree_idx(&plain_shared_host_id).unwrap();
        let pt = ws.project().attack_tree(pi).unwrap();
        let si = pt.idx_of(&psh_shared).unwrap();
        assert_eq!(ws.project().step_shared(pi, si), None);
        assert_eq!(ws.project().step_feasibility(pi, si), Some(med));
        assert!(ws.project().shared_step_idx(&shared_plain_id).is_some());

        let unuse_ops = Edit::UnuseFragment {
            tree: plain_shared_host_id,
            node: psh_detach,
        }
        .to_ops(ws.project(), "unuse", 100)
        .expect("unuse");
        assert_eq!(unuse_ops.len(), 1);
        ws.apply(unuse_ops, Origin::Edit).expect("apply unuse");
        let pt = ws.project().attack_tree(pi).unwrap();
        let di = pt.idx_of(&psh_detach).unwrap();
        assert_eq!(ws.project().step_fragment(pi, di), None);
        assert_eq!(ws.project().step_feasibility(pi, di), None);
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn unshare_fragment_caps_and_guards_fail_atomically() {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let mut ws = Workspace::empty(Arc::clone(&m));
    let p = Provenance::human("unshare-caps", 1421);
    let low = m.feasibility_level("low").expect("low");

    let conds: Vec<ConditionId> = (0..17).map(|_| ConditionId::new_v7()).collect();
    let ctrls: Vec<ControlId> = (0..17).map(|_| ControlId::new_v7()).collect();
    let mut setup = Vec::new();
    for (i, &cid) in conds.iter().enumerate() {
        setup.push(Op::AddCondition {
            id: cid,
            name: format!("C{i}").into(),
            description: "".into(),
            present: true,
            provenance: p.clone(),
        });
    }
    for (i, &kid) in ctrls.iter().enumerate() {
        setup.push(Op::AddSecurityControl {
            id: kid,
            name: format!("Ctrl{i}").into(),
            provenance: p.clone(),
        });
    }

    // 1-step fragment (`frag_1`), 2-node gate fragment (`frag_2`), 2-leaf gate fragment (`frag_2l`).
    let frag_1 = TreeId::new_v7();
    let f1_r = NodeId::new_v7();
    let frag_2 = TreeId::new_v7();
    let (f2_r, f2_l) = (NodeId::new_v7(), NodeId::new_v7());
    let mut f2_t = AttackTree::new(f2_r, Node::gate(GateKind::Or, "F2")).unwrap();
    f2_t.add_child(f2_t.root(), f2_l, Node::step("F2Leaf"))
        .unwrap();
    let frag_2l = TreeId::new_v7();
    let (f2l_r, f2l_a, f2l_b) = (NodeId::new_v7(), NodeId::new_v7(), NodeId::new_v7());
    let mut f2l_t = AttackTree::new(f2l_r, Node::gate(GateKind::Or, "F2L")).unwrap();
    f2l_t
        .add_child(f2l_t.root(), f2l_a, Node::step("A"))
        .unwrap();
    f2l_t
        .add_child(f2l_t.root(), f2l_b, Node::step("B"))
        .unwrap();

    // Fragment with 10 conditions and 10 mitigations for metadata merge cap tests.
    let frag_meta = TreeId::new_v7();
    let fm_r = NodeId::new_v7();
    setup.push(Op::AddAttackTree {
        role: TreeRole::Fragment,
        id: frag_1,
        tree: AttackTree::new(f1_r, Node::step("F1")).unwrap(),
        step_feasibility: vec![(f1_r, low)],
        provenance: p.clone(),
    });
    // Exponential depth-5 chain exceeding MAX_ATTACK_TREE_NODES (6^5 = 7776 > 4096).
    let mut exp_chain = vec![frag_1];
    for lvl in 0..5 {
        let tid = TreeId::new_v7();
        let rn = NodeId::new_v7();
        let mut ft = AttackTree::new(rn, Node::gate(GateKind::Or, format!("Exp{lvl}"))).unwrap();
        let mut refs = Vec::new();
        for k in 0..6 {
            let ch = NodeId::new_v7();
            ft.add_child(ft.root(), ch, Node::step(format!("E{lvl}_{k}")))
                .unwrap();
            refs.push(ch);
        }
        setup.push(Op::AddAttackTree {
            role: TreeRole::Fragment,
            id: tid,
            tree: ft,
            step_feasibility: vec![],
            provenance: p.clone(),
        });
        for ch in refs {
            setup.push(Op::UseFragment {
                tree: tid,
                node: ch,
                fragment: *exp_chain.last().unwrap(),
                provenance: p.clone(),
            });
        }
        exp_chain.push(tid);
    }

    // Large 350-node annotated fragment so Unshare emits > MAX_SUBMIT_OPS (1024).
    let frag_huge = TreeId::new_v7();
    let fh_r = NodeId::new_v7();
    let mut fh_t = AttackTree::new(fh_r, Node::gate(GateKind::Or, "HugeFrag")).unwrap();
    let mut fh_leaves = Vec::new();
    for i in 0..350 {
        let ch = NodeId::new_v7();
        fh_t.add_child(fh_t.root(), ch, Node::step(format!("HF{i}")))
            .unwrap();
        fh_leaves.push(ch);
    }

    // Host tree (`host`) with test slots for metadata caps, assumption caps, exponential ref, and huge ref.
    let host = TreeId::new_v7();
    let (h_r, h_plain, h_c_over, h_c_ok, h_m_over, h_m_ok, h_exp, h_huge, h_as_step, h_as_tree) = (
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
    );
    let mut h_t = AttackTree::new(h_r, Node::gate(GateKind::Or, "HostCaps")).unwrap();
    for (nid, lbl) in [
        (h_plain, "Plain"),
        (h_c_over, "COver"),
        (h_c_ok, "COk"),
        (h_m_over, "MOver"),
        (h_m_ok, "MOk"),
        (h_exp, "Exp"),
        (h_huge, "Huge"),
        (h_as_step, "AsStep"),
        (h_as_tree, "AsTree"),
    ] {
        h_t.add_child(h_t.root(), nid, Node::step(lbl)).unwrap();
    }
    let mut filler = Vec::new();
    for i in 0..255 {
        let fid = NodeId::new_v7();
        h_t.add_child(h_t.root(), fid, Node::step(format!("Fill{i}")))
            .unwrap();
        filler.push(StepRef {
            tree: host,
            node: fid,
        });
    }

    let frag_tree_excl = TreeId::new_v7();
    let fte_r = NodeId::new_v7();
    setup.extend([
        Op::AddAttackTree {
            role: TreeRole::Fragment,
            id: frag_2,
            tree: f2_t,
            step_feasibility: vec![(f2_l, low)],
            provenance: p.clone(),
        },
        Op::AddAttackTree {
            role: TreeRole::Fragment,
            id: frag_2l,
            tree: f2l_t,
            step_feasibility: vec![(f2l_a, low), (f2l_b, low)],
            provenance: p.clone(),
        },
        Op::AddAttackTree {
            role: TreeRole::Fragment,
            id: frag_meta,
            tree: AttackTree::new(fm_r, Node::step("FM")).unwrap(),
            step_feasibility: vec![(fm_r, low)],
            provenance: p.clone(),
        },
        Op::AddAttackTree {
            role: TreeRole::Fragment,
            id: frag_tree_excl,
            tree: AttackTree::new(fte_r, Node::step("FTE")).unwrap(),
            step_feasibility: vec![(fte_r, low)],
            provenance: p.clone(),
        },
    ]);
    for batch in add_tree_skeleton_ops(TreeRole::Threat, host, &h_t, &p).chunks(400) {
        ws.apply(batch.to_vec(), Origin::Edit).expect("host");
    }
    for &c in &conds[0..10] {
        setup.push(Op::NeedStepCondition {
            tree: frag_meta,
            node: fm_r,
            condition: c,
            provenance: p.clone(),
        });
    }
    for &ctrl in &ctrls[0..10] {
        setup.push(Op::SetStepMitigation {
            tree: frag_meta,
            node: fm_r,
            control: ctrl,
            feasibility: Some(low),
            provenance: p.clone(),
        });
    }
    for (nid, fid) in [
        (h_c_over, frag_meta),
        (h_c_ok, frag_meta),
        (h_m_over, frag_meta),
        (h_m_ok, frag_meta),
        (h_exp, *exp_chain.last().unwrap()),
        (h_huge, frag_huge),
        (h_as_step, frag_2l),
        (h_as_tree, frag_tree_excl),
    ] {
        if nid != h_huge {
            setup.push(Op::UseFragment {
                tree: host,
                node: nid,
                fragment: fid,
                provenance: p.clone(),
            });
        }
    }
    for &c in &conds[10..17] {
        setup.push(Op::NeedStepCondition {
            tree: host,
            node: h_c_over,
            condition: c,
            provenance: p.clone(),
        });
    }
    for &c in &conds[6..16] {
        setup.push(Op::NeedStepCondition {
            tree: host,
            node: h_c_ok,
            condition: c,
            provenance: p.clone(),
        });
    }
    for &ctrl in &ctrls[10..17] {
        setup.push(Op::SetStepMitigation {
            tree: host,
            node: h_m_over,
            control: ctrl,
            feasibility: Some(low),
            provenance: p.clone(),
        });
    }
    for &ctrl in &ctrls[6..16] {
        setup.push(Op::SetStepMitigation {
            tree: host,
            node: h_m_ok,
            control: ctrl,
            feasibility: Some(low),
            provenance: p.clone(),
        });
    }

    let a_step = AssumptionId::new_v7();
    let a_tree = AssumptionId::new_v7();
    let mut step_list = filler.clone();
    step_list.push(StepRef {
        tree: host,
        node: h_as_step,
    });
    setup.extend([
        Op::AddAssumption {
            id: a_step,
            name: "CapStep".into(),
            holds: true,
            provenance: p.clone(),
        },
        Op::SetAssumptionExcludes {
            id: a_step,
            trees: vec![],
            steps: step_list,
            provenance: p.clone(),
        },
        Op::AddAssumption {
            id: a_tree,
            name: "CapTree".into(),
            holds: true,
            provenance: p.clone(),
        },
        Op::SetAssumptionExcludes {
            id: a_tree,
            trees: vec![frag_tree_excl],
            steps: filler,
            provenance: p.clone(),
        },
    ]);

    for batch in setup.chunks(400) {
        ws.apply(batch.to_vec(), Origin::Edit).expect("setup");
    }
    for batch in add_tree_skeleton_ops(TreeRole::Fragment, frag_huge, &fh_t, &p).chunks(400) {
        ws.apply(batch.to_vec(), Origin::Edit).expect("frag_huge");
    }
    let mut huge_meta = Vec::new();
    for lid in fh_leaves {
        huge_meta.push(Op::SetStepFeasibility {
            target: StepTarget::Step(frag_huge, lid),
            level: Some(low),
            provenance: p.clone(),
        });
        huge_meta.push(Op::NeedStepCondition {
            tree: frag_huge,
            node: lid,
            condition: conds[0],
            provenance: p.clone(),
        });
        huge_meta.push(Op::SetReviewMark {
            tree: frag_huge,
            node: lid,
            marked: true,
            provenance: p.clone(),
        });
    }
    for batch in huge_meta.chunks(400) {
        ws.apply(batch.to_vec(), Origin::Edit).expect("huge_meta");
    }
    ws.apply(
        vec![Op::UseFragment {
            tree: host,
            node: h_huge,
            fragment: frag_huge,
            provenance: p.clone(),
        }],
        Origin::Edit,
    )
    .expect("use huge");

    // Host tree at exact MAX_ATTACK_TREE_NODES (4096).
    let max_host = TreeId::new_v7();
    let (mh_r, mh_ref2, mh_ref1) = (NodeId::new_v7(), NodeId::new_v7(), NodeId::new_v7());
    let mut mh_t = AttackTree::new(mh_r, Node::gate(GateKind::Or, "MaxHost")).unwrap();
    mh_t.add_child(mh_t.root(), mh_ref2, Node::step("MaxRef2"))
        .unwrap();
    mh_t.add_child(mh_t.root(), mh_ref1, Node::step("MaxRef1"))
        .unwrap();
    for i in 0..(MAX_ATTACK_TREE_NODES - 3) {
        let ch = NodeId::new_v7();
        mh_t.add_child(mh_t.root(), ch, Node::step(format!("M{i}")))
            .unwrap();
    }
    for batch in add_tree_skeleton_ops(TreeRole::Threat, max_host, &mh_t, &p).chunks(400) {
        ws.apply(batch.to_vec(), Origin::Edit).expect("max_host");
    }
    ws.apply(
        vec![
            Op::UseFragment {
                tree: max_host,
                node: mh_ref2,
                fragment: frag_2,
                provenance: p.clone(),
            },
            Op::UseFragment {
                tree: max_host,
                node: mh_ref1,
                fragment: frag_1,
                provenance: p.clone(),
            },
        ],
        Origin::Edit,
    )
    .expect("use max_host");

    // Pre-existing undo entry & clean pending count before testing failures.
    let mut ws = Workspace::new(ws.project().clone(), 0);
    let prior_ops = Edit::RenameTreeNode {
        tree: host,
        node: h_plain,
        label: "Plain renamed".into(),
    }
    .to_ops(ws.project(), "prior", 1)
    .unwrap();
    ws.apply(prior_ops, Origin::Edit).expect("prior");

    let before_proj = ws.project().clone();
    let before_bytes = saved(&before_proj);
    let before_files = before_proj.to_workspace_files().expect("ws files");
    let before_pending = ws.pending_batches();
    let before_undo_len = ws.edits().count();
    let before_label = ws.undo_label().expect("prior label").clone();

    let assert_intact = |ws: &Workspace| {
        assert!(ws.project() == &before_proj);
        assert_eq!(saved(ws.project()), before_bytes);
        assert_eq!(
            ws.project().to_workspace_files().expect("ws files"),
            before_files
        );
        assert_eq!(ws.pending_batches(), before_pending);
        assert_eq!(ws.edits().count(), before_undo_len);
        assert_eq!(ws.undo_label(), Some(&before_label));
    };

    // 1. Invalid selections -> EditError::Missing.
    for (t, n) in [
        (TreeId::new_v7(), h_plain),
        (host, NodeId::new_v7()),
        (host, h_r),
        (host, h_plain),
    ] {
        assert_eq!(
            Edit::Unshare { tree: t, node: n }.to_ops(ws.project(), "ed", 2),
            Err(EditError::Missing)
        );
    }

    // 2. Cap overflows fail in to_ops before mutation.
    for ((t, n), expect_res, expect_max) in [
        ((max_host, mh_ref2), "nodes", MAX_ATTACK_TREE_NODES),
        ((host, h_exp), "nodes", MAX_ATTACK_TREE_NODES),
        (
            (host, h_c_over),
            "step_conditions",
            MAX_CONDITIONS_PER_ELEMENT,
        ),
        ((host, h_m_over), "step_mitigations", MAX_STEP_MITIGATIONS),
        (
            (host, h_as_step),
            "assumption_excludes",
            MAX_ASSUMPTION_EXCLUDES,
        ),
        (
            (host, h_as_tree),
            "assumption_excludes",
            MAX_ASSUMPTION_EXCLUDES,
        ),
    ] {
        assert_eq!(
            Edit::Unshare { tree: t, node: n }.to_ops(ws.project(), "ed", 3),
            Err(EditError::FragmentCopyLimit {
                resource: expect_res,
                max: expect_max
            }),
            "expected {expect_res} cap failure"
        );
        assert_intact(&ws);
    }

    // 3. MAX_SUBMIT_OPS overflow fails atomically in Workspace::apply.
    let huge_ops = Edit::Unshare {
        tree: host,
        node: h_huge,
    }
    .to_ops(ws.project(), "ed", 4)
    .expect("huge ops");
    assert!(huge_ops.len() > MAX_SUBMIT_OPS);
    assert!(matches!(
        ws.apply(huge_ops, Origin::Edit),
        Err(WorkspaceError::TooLarge { .. })
    ));
    assert_intact(&ws);

    // 4. Contrasts succeed: 16 overlapping conditions, 16 overlapping mitigations, and 1-step on 4096-node host!
    for (t, n) in [(host, h_c_ok), (host, h_m_ok), (max_host, mh_ref1)] {
        let ops = Edit::Unshare { tree: t, node: n }
            .to_ops(ws.project(), "ed", 5)
            .expect("contrast ops");
        ws.apply(ops, Origin::Edit).expect("contrast apply");
    }
    assert_eq!(
        ws.project()
            .attack_tree(ws.project().attack_tree_idx(&max_host).unwrap())
            .unwrap()
            .len(),
        MAX_ATTACK_TREE_NODES
    );
}

fn accept_and_verify_state(
    ws: &mut Workspace,
    log_bytes: &mut Vec<u8>,
    seq: &mut u64,
    m: &Arc<Methodology>,
    expected_op_count: Option<usize>,
) {
    let sub = ws.next_submission().expect("pending submission");
    if let Some(n) = expected_op_count {
        assert_eq!(sub.ops.len(), n, "must submit entire batch atomically");
    }
    assert!(
        ws.next_submission().is_none(),
        "only one in-flight submission"
    );
    let new_seq = *seq + sub.ops.len() as u64;
    ws.on_accepted(new_seq).expect("on_accepted");
    assert_eq!(ws.pending_batches(), 0);

    let mut writer = OpLogWriter::resume(&mut *log_bytes, Arc::clone(m), *seq);
    for op in &sub.ops {
        writer.append(op).expect("append oplog");
    }
    *seq = writer.last_seq();
    writer.into_inner().expect("flush oplog");
    assert_eq!(*seq, new_seq);

    let snap = saved(ws.project());
    let snap_loaded = Project::load(snap.as_slice(), Arc::clone(m)).expect("snap reload");
    assert!(snap_loaded == *ws.project());

    let ws_files = ws.project().to_workspace_files().expect("to_ws");
    let ws_loaded = Project::from_workspace_files(
        ws_files
            .iter()
            .map(|f| (f.path.as_str(), f.bytes.as_slice())),
        Arc::clone(m),
    )
    .expect("from_ws");
    assert!(ws_loaded == *ws.project());

    let mut replayed = Project::new(Arc::clone(m));
    let reader = OpLogReader::open(log_bytes.as_slice(), Arc::clone(m)).expect("open oplog");
    for rec in reader {
        replayed
            .apply(rec.expect("valid record").op)
            .expect("replay op");
    }
    assert!(replayed == *ws.project());
    assert_eq!(saved(&replayed), snap);
}

#[test]
#[allow(clippy::too_many_lines)]
fn make_shared_then_unshare_end_to_end_roundtrips_snapshot_oplog_and_workspace_files() {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let p = Provenance::human("e2e-author", 1422);
    let low = m.feasibility_level("low").expect("low");
    let med = m.feasibility_level("medium").expect("medium");
    let high = m.feasibility_level("high").expect("high");
    let factor = m.feasibility_factors().next().expect("factor");
    let flevel = m.factor_levels(factor).last().expect("flevel");
    let atk_type = m.attack_types().next().expect("atk_type");

    let comp_id = ComponentId::new_v7();
    let cond_id = ConditionId::new_v7();
    let ctrl_id = ControlId::new_v7();
    let assump_id = AssumptionId::new_v7();
    let shared_inner_id = SharedStepId::new_v7();

    let host_id = TreeId::new_v7();
    let (h_root, h_gate, h_dm_a, h_dm_b, h_dm_leaf, h_shared_use, h_step) = (
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
    );
    let mut host = AttackTree::new(h_root, Node::gate(GateKind::Or, "E2ERoot")).unwrap();
    let hr = host.root();
    let hg = host
        .add_child(hr, h_gate, Node::gate(GateKind::SeqAnd, "SubGate"))
        .unwrap();
    let da = host
        .add_child(hg, h_dm_a, Node::gate(GateKind::And, "DmA"))
        .unwrap();
    let db = host
        .add_child(hg, h_dm_b, Node::gate(GateKind::Or, "DmB"))
        .unwrap();
    let dl = host.add_child(da, h_dm_leaf, Node::step("DmLeaf")).unwrap();
    host.link(db, dl).unwrap();
    host.add_child(hg, h_shared_use, Node::step("NestedShared"))
        .unwrap();
    host.add_child(hr, h_step, Node::step("PlainRichStep"))
        .unwrap();

    let setup_ops = vec![
        Op::AddComponent {
            id: comp_id,
            name: "ECU".into(),
            parent: None,
            provenance: p.clone(),
        },
        Op::AddCondition {
            id: cond_id,
            name: "Cond".into(),
            description: "".into(),
            present: true,
            provenance: p.clone(),
        },
        Op::AddSecurityControl {
            id: ctrl_id,
            name: "Ctrl".into(),
            provenance: p.clone(),
        },
        Op::SetControlStatus {
            control: ctrl_id,
            status: ControlStatus::Implemented,
            provenance: p.clone(),
        },
        Op::AddAssumption {
            id: assump_id,
            name: "Assump".into(),
            holds: false,
            provenance: p.clone(),
        },
        Op::AddSharedStep {
            id: shared_inner_id,
            name: "InnerShared".into(),
            provenance: p.clone(),
        },
        Op::SetStepFeasibility {
            target: StepTarget::Shared(shared_inner_id),
            level: Some(med),
            provenance: p.clone(),
        },
        Op::SetStepFactor {
            target: StepTarget::Shared(shared_inner_id),
            factor,
            level: Some(flevel),
            provenance: p.clone(),
        },
        Op::SetStepVia {
            target: StepTarget::Shared(shared_inner_id),
            via: Some(ItemRef::Component(comp_id)),
            provenance: p.clone(),
        },
        Op::SetStepAttackType {
            target: StepTarget::Shared(shared_inner_id),
            attack_type: Some(atk_type),
            provenance: p.clone(),
        },
        Op::AddAttackTree {
            role: TreeRole::Threat,
            id: host_id,
            tree: host,
            step_feasibility: vec![(h_dm_leaf, high), (h_step, high)],
            provenance: p.clone(),
        },
        Op::UseSharedStep {
            tree: host_id,
            node: h_shared_use,
            step: shared_inner_id,
            provenance: p.clone(),
        },
        Op::NeedStepCondition {
            tree: host_id,
            node: h_dm_leaf,
            condition: cond_id,
            provenance: p.clone(),
        },
        Op::SetStepMitigation {
            tree: host_id,
            node: h_dm_leaf,
            control: ctrl_id,
            feasibility: Some(low),
            provenance: p.clone(),
        },
        Op::SetReviewMark {
            tree: host_id,
            node: h_dm_leaf,
            marked: true,
            provenance: p.clone(),
        },
        Op::SetAssumptionExcludes {
            id: assump_id,
            trees: vec![],
            steps: vec![StepRef {
                tree: host_id,
                node: h_dm_leaf,
            }],
            provenance: p.clone(),
        },
        Op::SetStepFactor {
            target: StepTarget::Step(host_id, h_step),
            factor,
            level: Some(flevel),
            provenance: p.clone(),
        },
        Op::SetStepVia {
            target: StepTarget::Step(host_id, h_step),
            via: Some(ItemRef::Component(comp_id)),
            provenance: p.clone(),
        },
        Op::SetStepAttackType {
            target: StepTarget::Step(host_id, h_step),
            attack_type: Some(atk_type),
            provenance: p.clone(),
        },
    ];

    let mut log_bytes = OpLogWriter::create(Vec::new(), Arc::clone(&m))
        .expect("create log")
        .into_inner()
        .expect("log bytes");
    let mut seq = 0u64;
    let mut init_ws = Workspace::empty(Arc::clone(&m));
    init_ws.apply(setup_ops, Origin::Edit).expect("setup");
    accept_and_verify_state(&mut init_ws, &mut log_bytes, &mut seq, &m, None);

    // Reload initial Workspace from snapshot/workspace files (with prior undo entry + op log).
    let init_snap = saved(init_ws.project());
    let ws_files = Project::load(init_snap.as_slice(), Arc::clone(&m))
        .expect("snap")
        .to_workspace_files()
        .expect("ws_files");
    let loaded_proj = Project::from_workspace_files(
        ws_files
            .iter()
            .map(|f| (f.path.as_str(), f.bytes.as_slice())),
        Arc::clone(&m),
    )
    .expect("loaded");
    let mut ws = Workspace::new(loaded_proj, seq);

    let prior_ops = Edit::RenameTreeNode {
        tree: host_id,
        node: h_step,
        label: "PlainRichStep 2".into(),
    }
    .to_ops(ws.project(), "prior", 50)
    .expect("prior");
    let prior_len = prior_ops.len();
    ws.apply(prior_ops, Origin::Edit).expect("apply prior");
    accept_and_verify_state(&mut ws, &mut log_bytes, &mut seq, &m, Some(prior_len));

    let run_cycle = |ws: &mut Workspace,
                     log_bytes: &mut Vec<u8>,
                     seq: &mut u64,
                     make_edit: Edit,
                     unshare_edit: Edit| {
        let s0 = ws.project().clone();
        let s0_bytes = saved(&s0);
        let s0_eval = eval_outcome_pair(ws, host_id);

        let make_ops = make_edit
            .to_ops(ws.project(), "e2e", 100)
            .expect("make ops");
        let make_len = make_ops.len();
        ws.apply(make_ops, Origin::Edit).expect("apply make");
        accept_and_verify_state(ws, log_bytes, seq, &m, Some(make_len));
        let s1 = ws.project().clone();
        let s1_bytes = saved(&s1);
        assert_eq!(eval_outcome_pair(ws, host_id), s0_eval);

        let un_ops = unshare_edit
            .to_ops(ws.project(), "e2e", 101)
            .expect("unshare ops");
        let un_len = un_ops.len();
        ws.apply(un_ops, Origin::Edit).expect("apply unshare");
        accept_and_verify_state(ws, log_bytes, seq, &m, Some(un_len));
        let s2 = ws.project().clone();
        let s2_bytes = saved(&s2);
        let s2_ids: Vec<NodeId> = s2
            .attack_tree(s2.attack_tree_idx(&host_id).unwrap())
            .unwrap()
            .iter()
            .map(|(_, id, _)| id)
            .collect();
        assert_eq!(eval_outcome_pair(ws, host_id), s0_eval);

        // undo -> undo -> redo -> redo with snapshot/oplog/workspace reload after EVERY state.
        ws.undo().expect("undo 1 (unshare)");
        accept_and_verify_state(ws, log_bytes, seq, &m, None);
        assert!(ws.project() == &s1);
        assert_eq!(saved(ws.project()), s1_bytes);

        ws.undo().expect("undo 2 (make)");
        accept_and_verify_state(ws, log_bytes, seq, &m, None);
        assert!(ws.project() == &s0);
        assert_eq!(saved(ws.project()), s0_bytes);

        ws.redo().expect("redo 1 (make)");
        accept_and_verify_state(ws, log_bytes, seq, &m, None);
        assert!(ws.project() == &s1);
        assert_eq!(saved(ws.project()), s1_bytes);

        ws.redo().expect("redo 2 (unshare)");
        accept_and_verify_state(ws, log_bytes, seq, &m, None);
        assert!(ws.project() == &s2);
        assert_eq!(saved(ws.project()), s2_bytes);
        let redo_ids: Vec<NodeId> = ws
            .project()
            .attack_tree(ws.project().attack_tree_idx(&host_id).unwrap())
            .unwrap()
            .iter()
            .map(|(_, id, _)| id)
            .collect();
        assert_eq!(redo_ids, s2_ids);
        assert_eq!(eval_outcome_pair(ws, host_id), s0_eval);
    };

    // Workflow 1: MakeSharedFragment -> Unshare -> undo -> undo -> redo -> redo.
    let frag_id = TreeId::new_v7();
    run_cycle(
        &mut ws,
        &mut log_bytes,
        &mut seq,
        Edit::MakeSharedFragment {
            tree: host_id,
            node: h_gate,
            id: frag_id,
        },
        Edit::Unshare {
            tree: host_id,
            node: h_gate,
        },
    );

    // Workflow 2: MakeShared -> Unshare -> undo -> undo -> redo -> redo.
    let shared_id = SharedStepId::new_v7();
    run_cycle(
        &mut ws,
        &mut log_bytes,
        &mut seq,
        Edit::MakeShared {
            tree: host_id,
            node: h_step,
            id: shared_id,
        },
        Edit::Unshare {
            tree: host_id,
            node: h_step,
        },
    );
}
