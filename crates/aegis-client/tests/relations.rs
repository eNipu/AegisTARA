//! The bidirectional reachability safety net for the relation registry
//! (NFR-12, architecture III.9.2). The fixture holds one instance of every
//! registry link; `model_links` reads the arenas directly, so adding a model
//! link without a registry row (or pointing a key at the wrong read function)
//! fails `every_model_link_has_a_registry_row_and_vice_versa`.

use std::cmp::Ordering;
use std::collections::{BTreeSet, HashSet};
use std::sync::Arc;

use aegis_client::i18n::{Catalog, Language};
use aegis_client::relations::{
    ElementKind, RELATIONS, RelationKey, linked, related, relation, reverse_related,
};
use aegis_client::view::Selection;
use aegis_model::{
    AssetId, AssumptionId, AttackTree, ClaimId, ComponentId, ConditionId, ControlId, DamageId,
    DataFlowId, GateKind, GoalId, InterfaceId, ItemRef, Methodology, Node, NodeId, Op, PartyId,
    PartyIdx, PartyKind, Project, Provenance, SharedStepId, StepRef, StepTarget, ThreatId, TreeId,
    TreeRole, TrustBoundaryId,
};

/// `Selection` has no `Ord`, and the orphan rule forbids adding one here. This
/// local wrapper supplies a deterministic total order so the two link sets can
/// be `BTreeSet`s, giving sorted symmetric differences in failure output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Sel(Selection);

impl Sel {
    fn key(self) -> (u8, String) {
        let rank = match self.0 {
            Selection::Component(_) => 0,
            Selection::Interface(_) => 1,
            Selection::DataFlow(_) => 2,
            Selection::TrustBoundary(_) => 3,
            Selection::Asset(_) => 4,
            Selection::Damage(_) => 5,
            Selection::Threat(_) => 6,
            Selection::Tree(_) => 7,
            Selection::TreeNode(_, _) => 8,
            Selection::Control(_) => 9,
            Selection::Goal(_) => 10,
            Selection::Claim(_) => 11,
            Selection::Party(_) => 12,
            Selection::Condition(_) => 13,
            Selection::Assumption(_) => 14,
            Selection::SharedStep(_) => 15,
        };
        let s = self.0;
        (rank, format!("{s:?}"))
    }
}

impl Ord for Sel {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key().cmp(&other.key())
    }
}

impl PartialOrd for Sel {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn item_selection(item: ItemRef) -> Option<Selection> {
    match item {
        ItemRef::Component(id) => Some(Selection::Component(id)),
        ItemRef::Interface(id) => Some(Selection::Interface(id)),
    }
}

/// One `Project` holding exactly one instance of every registry row, with two
/// targets for every `many = true` row and one for every `many = false` row.
fn project() -> Project {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let provenance = Provenance::human("demo", 1);
    let spoofing = m.threat_category("spoofing").expect("spoofing");
    let low = m.feasibility_level("low").expect("low");
    let integrity = m.property("integrity").expect("integrity");
    let authenticity = m.property("authenticity").expect("authenticity");

    let (c1, c2) = (ComponentId::new_v7(), ComponentId::new_v7());
    let (iface, boundary) = (InterfaceId::new_v7(), TrustBoundaryId::new_v7());
    let can = m.interface_kinds().next().expect("an interface kind");
    let (flow, asset, d1, d2, threat) = (
        DataFlowId::new_v7(),
        AssetId::new_v7(),
        DamageId::new_v7(),
        DamageId::new_v7(),
        ThreatId::new_v7(),
    );
    let (tree_id, root, step) = (TreeId::new_v7(), NodeId::new_v7(), NodeId::new_v7());
    let (shared_step, shared_user_a, shared_user_b) =
        (SharedStepId::new_v7(), NodeId::new_v7(), NodeId::new_v7());
    let fragment = TreeId::new_v7();
    let fragment_users = [NodeId::new_v7(), NodeId::new_v7()];
    let (control1, control2, goal, goal2, claim) = (
        ControlId::new_v7(),
        ControlId::new_v7(),
        GoalId::new_v7(),
        GoalId::new_v7(),
        ClaimId::new_v7(),
    );
    let (party1, party2) = (PartyId::new_v7(), PartyId::new_v7());

    let mut tree = AttackTree::new(root, Node::gate(GateKind::Or, "root")).expect("tree");
    tree.add_child(tree.root(), step, Node::step("step"))
        .expect("child");
    tree.add_child(tree.root(), shared_user_a, Node::step("shared a"))
        .expect("shared a");
    tree.add_child(tree.root(), shared_user_b, Node::step("shared b"))
        .expect("shared b");

    for user in fragment_users {
        tree.add_child(tree.root(), user, Node::step("fragment user"))
            .expect("user");
    }

    let ops = vec![
        Op::AddComponent {
            id: c1,
            name: "C1".into(),
            parent: None,
            provenance: provenance.clone(),
        },
        Op::AddComponent {
            id: c2,
            name: "C2".into(),
            parent: None,
            provenance: provenance.clone(),
        },
        Op::AddInterface {
            id: iface,
            name: "CAN".into(),
            component: c2,
            kind: can,
            provenance: provenance.clone(),
        },
        Op::AddTrustBoundary {
            id: boundary,
            name: "vehicle".into(),
            components: vec![c1, c2],
            provenance: provenance.clone(),
        },
        Op::AddDataFlow {
            id: flow,
            name: "flow".into(),
            from: c1,
            to: c2,
            interface: Some(iface),
            provenance: provenance.clone(),
        },
        Op::AddAsset {
            id: asset,
            name: "asset".into(),
            properties: [integrity, authenticity].into_iter().collect(),
            provenance: provenance.clone(),
        },
        Op::SetAssetComponent {
            asset,
            component: Some(c1),
            provenance: provenance.clone(),
        },
        Op::AddDamageScenario {
            id: d1,
            name: "d1".into(),
            impacts: vec![],
            provenance: provenance.clone(),
        },
        Op::AddDamageScenario {
            id: d2,
            name: "d2".into(),
            impacts: vec![],
            provenance: provenance.clone(),
        },
        Op::AddThreatScenario {
            id: threat,
            name: "threat".into(),
            asset,
            category: spoofing,
            catalog_refs: vec![],
            damages: vec![d1, d2],
            provenance: provenance.clone(),
        },
        Op::AddAttackTree {
            role: Default::default(),
            id: tree_id,
            tree,
            step_feasibility: vec![],
            provenance: provenance.clone(),
        },
        Op::SetThreatAttackTree {
            threat,
            tree: Some(tree_id),
            provenance: provenance.clone(),
        },
        Op::SetStepVia {
            target: StepTarget::Step(tree_id, step),
            via: Some(ItemRef::Interface(iface)),
            provenance: provenance.clone(),
        },
        Op::AddSharedStep {
            id: shared_step,
            name: "shared".into(),
            provenance: provenance.clone(),
        },
        Op::SetStepVia {
            target: StepTarget::Shared(shared_step),
            via: Some(ItemRef::Interface(iface)),
            provenance: provenance.clone(),
        },
        Op::UseSharedStep {
            tree: tree_id,
            node: shared_user_a,
            step: shared_step,
            provenance: provenance.clone(),
        },
        Op::UseSharedStep {
            tree: tree_id,
            node: shared_user_b,
            step: shared_step,
            provenance: provenance.clone(),
        },
        Op::AddSecurityControl {
            id: control1,
            name: "control1".into(),
            provenance: provenance.clone(),
        },
        Op::AddSecurityControl {
            id: control2,
            name: "control2".into(),
            provenance: provenance.clone(),
        },
        Op::ApplyControl {
            control: control1,
            target: ItemRef::Component(c1),
            provenance: provenance.clone(),
        },
        Op::ApplyControl {
            control: control1,
            target: ItemRef::Component(c2),
            provenance: provenance.clone(),
        },
        Op::ApplyControl {
            control: control1,
            target: ItemRef::Interface(iface),
            provenance: provenance.clone(),
        },
        Op::SetStepMitigation {
            tree: tree_id,
            node: step,
            control: control1,
            feasibility: Some(low),
            provenance: provenance.clone(),
        },
        Op::SetStepMitigation {
            tree: tree_id,
            node: step,
            control: control2,
            feasibility: Some(low),
            provenance: provenance.clone(),
        },
        Op::AddCybersecurityGoal {
            id: goal,
            name: "goal".into(),
            controls: vec![control1, control2],
            provenance: provenance.clone(),
        },
        Op::AddCybersecurityGoal {
            id: goal2,
            name: "goal2".into(),
            controls: vec![],
            provenance: provenance.clone(),
        },
        Op::LinkThreatGoal {
            threat,
            goal,
            provenance: provenance.clone(),
        },
        Op::LinkThreatGoal {
            threat,
            goal: goal2,
            provenance: provenance.clone(),
        },
        Op::AddCybersecurityClaim {
            id: claim,
            name: "claim".into(),
            provenance: provenance.clone(),
        },
        Op::AddParty {
            id: party1,
            name: "Bosch".into(),
            kind: PartyKind::Supplier,
            provenance: provenance.clone(),
        },
        Op::AddParty {
            id: party2,
            name: "OEM".into(),
            kind: PartyKind::Customer,
            provenance: provenance.clone(),
        },
        Op::LinkGoalParty {
            goal,
            party: party1,
            provenance: provenance.clone(),
        },
        Op::LinkGoalParty {
            goal,
            party: party2,
            provenance: provenance.clone(),
        },
        Op::LinkClaimParty {
            claim,
            party: party1,
            provenance: provenance.clone(),
        },
        Op::LinkClaimParty {
            claim,
            party: party2,
            provenance: provenance.clone(),
        },
        Op::SetThreatClaim {
            threat,
            claim: Some(claim),
            provenance: provenance.clone(),
        },
    ];
    // Two conditions, needed by one element of every supported kind and by
    // the step, so every condition row is `many` with one owning source.
    let conditions = [ConditionId::new_v7(), ConditionId::new_v7()];
    let mut ops = ops;
    ops.push(Op::AddAttackTree {
        role: TreeRole::Fragment,
        id: fragment,
        tree: AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "Fragment"))
            .expect("fragment"),
        step_feasibility: Vec::new(),
        provenance: provenance.clone(),
    });
    for node in fragment_users {
        ops.push(Op::UseFragment {
            tree: tree_id,
            node,
            fragment,
            provenance: provenance.clone(),
        });
    }
    for (i, id) in conditions.iter().enumerate() {
        ops.push(Op::AddCondition {
            id: *id,
            name: format!("Condition {i}").into(),
            description: "".into(),
            present: true,
            provenance: provenance.clone(),
        });
    }
    use aegis_model::ElementRef as E;
    for element in [
        E::Asset(asset),
        E::Component(c1),
        E::Interface(iface),
        E::Threat(threat),
        E::Tree(tree_id),
        E::Goal(goal),
        E::Claim(claim),
    ] {
        for condition in conditions {
            ops.push(Op::NeedCondition {
                element,
                condition,
                provenance: provenance.clone(),
            });
        }
    }
    for condition in conditions {
        ops.push(Op::NeedStepCondition {
            tree: tree_id,
            node: step,
            condition,
            provenance: provenance.clone(),
        });
    }
    let (as00, as01) = (AssumptionId::new_v7(), AssumptionId::new_v7());
    for (id, name) in [(as00, "AS00"), (as01, "AS01")] {
        ops.push(Op::AddAssumption {
            id,
            name: name.into(),
            holds: true,
            provenance: provenance.clone(),
        });
        ops.push(Op::LinkClaimAssumption {
            claim,
            assumption: id,
            provenance: provenance.clone(),
        });
    }
    for condition in conditions {
        ops.push(Op::NeedCondition {
            element: E::Assumption(as00),
            condition,
            provenance: provenance.clone(),
        });
    }
    ops.push(Op::SetAssumptionExcludes {
        id: as00,
        trees: vec![tree_id],
        steps: vec![StepRef {
            tree: tree_id,
            node: step,
        }],
        provenance,
    });

    let mut project = Project::new(m);
    for op in ops {
        project.apply(op).expect("op applies");
    }
    project
}

/// Every source element of one `from` kind in the fixture.
#[rustfmt::skip]
fn sources_of(project: &Project, kind: ElementKind) -> Vec<Selection> {
    match kind {
        ElementKind::Asset => project.assets().map(|(_, id, _)| Selection::Asset(id)).collect(),
        ElementKind::Threat => project.threat_scenarios().map(|(_, id, _)| Selection::Threat(id)).collect(),
        ElementKind::Goal => project.cybersecurity_goals().map(|(_, id, _)| Selection::Goal(id)).collect(),
        ElementKind::Claim => project.cybersecurity_claims().map(|(_, id, _)| Selection::Claim(id)).collect(),
        ElementKind::Control => project.security_controls().map(|(_, id, _)| Selection::Control(id)).collect(),
        ElementKind::DataFlow => project.data_flows().map(|(_, id, _)| Selection::DataFlow(id)).collect(),
        ElementKind::Interface => project.interfaces().map(|(_, id, _)| Selection::Interface(id)).collect(),
        ElementKind::Component => project.components().map(|(_, id, _)| Selection::Component(id)).collect(),
        ElementKind::Tree => project.attack_trees().map(|(_, id, _)| Selection::Tree(id)).collect(),
        ElementKind::TrustBoundary => project.trust_boundaries().map(|(_, id, _)| Selection::TrustBoundary(id)).collect(),
        ElementKind::Step => project
            .attack_trees()
            .flat_map(|(_, id, tree)| tree.iter().filter(|(_, _, n)| n.gate_kind().is_none()).map(move |(_, node, _)| Selection::TreeNode(id, node)))
            .collect(),
        ElementKind::Assumption => project.assumptions().map(|(_, id, _)| Selection::Assumption(id)).collect(),
        ElementKind::SharedStep => project.shared_steps().map(|(_, id, _)| Selection::SharedStep(id)).collect(),
        _ => Vec::new(),
    }
}

/// Enumerates every link from the model arenas directly, independent of
/// `RELATIONS`. A model link with no registry row makes this set disagree with
/// `registry_links`.
fn model_links(project: &Project) -> BTreeSet<(Sel, Sel)> {
    let mut out = BTreeSet::new();
    let party_ids: Vec<(PartyIdx, PartyId)> =
        project.parties().map(|(idx, id, _)| (idx, id)).collect();
    let party_id = |idx: PartyIdx| party_ids.iter().find(|(i, _)| *i == idx).map(|(_, id)| *id);

    for (_, id, asset) in project.assets() {
        if let Some(component) = asset.component().and_then(|c| project.component_id(c)) {
            out.insert((
                Sel(Selection::Asset(id)),
                Sel(Selection::Component(component)),
            ));
        }
    }

    for (_, id, threat) in project.threat_scenarios() {
        let source = Sel(Selection::Threat(id));
        if let Some(asset) = project.asset_id(threat.asset()) {
            out.insert((source, Sel(Selection::Asset(asset))));
        }
        for &damage in threat.damages() {
            if let Some(damage) = project.damage_scenario_id(damage) {
                out.insert((source, Sel(Selection::Damage(damage))));
            }
        }
        if let Some(tree) = threat.attack_tree().and_then(|t| project.attack_tree_id(t)) {
            out.insert((source, Sel(Selection::Tree(tree))));
        }
        for &g in threat.goals() {
            if let Some(goal) = project.cybersecurity_goal_id(g) {
                out.insert((source, Sel(Selection::Goal(goal))));
            }
        }
        if let Some(claim) = threat
            .claim()
            .and_then(|c| project.cybersecurity_claim_id(c))
        {
            out.insert((source, Sel(Selection::Claim(claim))));
        }
    }

    for (_, id, goal) in project.cybersecurity_goals() {
        for &control in goal.controls() {
            if let Some(control) = project.security_control_id(control) {
                out.insert((Sel(Selection::Goal(id)), Sel(Selection::Control(control))));
            }
        }
    }

    for (idx, id, _) in project.shared_steps() {
        if let Some(item) = project.shared_step_via(idx).and_then(item_selection) {
            out.insert((Sel(Selection::SharedStep(id)), Sel(item)));
        }
    }
    for (t_idx, tree_id, tree) in project.attack_trees() {
        for (n_idx, node_id, _) in tree.iter() {
            if let Some(fragment) = project.step_fragment_id(t_idx, n_idx) {
                out.insert((
                    Sel(Selection::Tree(fragment)),
                    Sel(Selection::TreeNode(tree_id, node_id)),
                ));
            }
            if let Some(shared) = project
                .step_shared_id(t_idx, n_idx)
                .map(Selection::SharedStep)
            {
                out.insert((Sel(shared), Sel(Selection::TreeNode(tree_id, node_id))));
            }
        }
    }

    for (_, id, _) in project.security_controls() {
        if let Some(idx) = project.security_control_idx(&id) {
            for item in project.control_targets(idx) {
                if let Some(target) = item_selection(item) {
                    out.insert((Sel(Selection::Control(id)), Sel(target)));
                }
            }
        }
    }

    for (_, id, goal) in project.cybersecurity_goals() {
        for &idx in goal.responsible() {
            if let Some(party) = party_id(idx) {
                out.insert((Sel(Selection::Goal(id)), Sel(Selection::Party(party))));
            }
        }
    }
    for (_, id, claim) in project.cybersecurity_claims() {
        for &idx in claim.responsible() {
            if let Some(party) = party_id(idx) {
                out.insert((Sel(Selection::Claim(id)), Sel(Selection::Party(party))));
            }
        }
        for &idx in claim.assumptions() {
            if let Some(aid) = project.assumption_id(idx) {
                out.insert((Sel(Selection::Claim(id)), Sel(Selection::Assumption(aid))));
            }
        }
    }
    for (_, id, a) in project.assumptions() {
        for &t in a.excludes_trees() {
            if let Some(tid) = project.attack_tree_id(t) {
                out.insert((Sel(Selection::Assumption(id)), Sel(Selection::Tree(tid))));
            }
        }
        for s in a.excludes_steps() {
            out.insert((
                Sel(Selection::Assumption(id)),
                Sel(Selection::TreeNode(s.tree, s.node)),
            ));
        }
    }

    for (_, id, flow) in project.data_flows() {
        let source = Sel(Selection::DataFlow(id));
        if let Some(from) = project.component_id(flow.from()) {
            out.insert((source, Sel(Selection::Component(from))));
        }
        if let Some(to) = project.component_id(flow.to()) {
            out.insert((source, Sel(Selection::Component(to))));
        }
        if let Some(i) = flow.interface().and_then(|i| project.interface_id(i)) {
            out.insert((source, Sel(Selection::Interface(i))));
        }
    }

    // Condition links, read straight from the model's accessors.
    let condition = |i| {
        project
            .condition_id(i)
            .map(|c| Sel(Selection::Condition(c)))
    };
    let holders: Vec<(Selection, aegis_model::ElementRef)> = project
        .assets()
        .map(|(_, id, _)| (Selection::Asset(id), aegis_model::ElementRef::Asset(id)))
        .chain(project.components().map(|(_, id, _)| {
            (
                Selection::Component(id),
                aegis_model::ElementRef::Component(id),
            )
        }))
        .chain(project.interfaces().map(|(_, id, _)| {
            (
                Selection::Interface(id),
                aegis_model::ElementRef::Interface(id),
            )
        }))
        .chain(
            project
                .threat_scenarios()
                .map(|(_, id, _)| (Selection::Threat(id), aegis_model::ElementRef::Threat(id))),
        )
        .chain(
            project
                .attack_trees()
                .map(|(_, id, _)| (Selection::Tree(id), aegis_model::ElementRef::Tree(id))),
        )
        .chain(
            project
                .cybersecurity_goals()
                .map(|(_, id, _)| (Selection::Goal(id), aegis_model::ElementRef::Goal(id))),
        )
        .chain(
            project
                .cybersecurity_claims()
                .map(|(_, id, _)| (Selection::Claim(id), aegis_model::ElementRef::Claim(id))),
        )
        .chain(project.assumptions().map(|(_, id, _)| {
            (
                Selection::Assumption(id),
                aegis_model::ElementRef::Assumption(id),
            )
        }))
        .collect();
    for (sel, element) in holders {
        for &i in project.condition_link_ids(element) {
            if let Some(c) = condition(i) {
                out.insert((Sel(sel), c));
            }
        }
    }
    for (_, tree, t) in project.attack_trees() {
        for (_, node, _) in t.iter() {
            for &i in project.step_condition_link_ids(tree, node) {
                if let Some(c) = condition(i) {
                    out.insert((Sel(Selection::TreeNode(tree, node)), c));
                }
            }
        }
    }

    for (_, id, iface) in project.interfaces() {
        if let Some(c) = project.component_id(iface.component()) {
            out.insert((Sel(Selection::Interface(id)), Sel(Selection::Component(c))));
        }
    }

    for (_, id, boundary) in project.trust_boundaries() {
        for &c in boundary.components() {
            if let Some(c) = project.component_id(c) {
                out.insert((
                    Sel(Selection::TrustBoundary(id)),
                    Sel(Selection::Component(c)),
                ));
            }
        }
    }

    for (tree_idx, tree_id, tree) in project.attack_trees() {
        for (node_idx, node_id, _) in tree.iter() {
            let source = Sel(Selection::TreeNode(tree_id, node_id));
            if project.step_shared(tree_idx, node_idx).is_none()
                && let Some(via) = project
                    .step_via(tree_idx, node_idx)
                    .and_then(item_selection)
            {
                out.insert((source, Sel(via)));
            }
            for mitigation in project.step_mitigations(tree_idx, node_idx) {
                if let Some(control) = project.security_control_id(mitigation.control()) {
                    out.insert((source, Sel(Selection::Control(control))));
                }
            }
        }
    }

    out
}

/// Enumerates every link through `RELATIONS`: each source of the row's `from`
/// kind paired with the targets its `read` function returns.
fn registry_links(project: &Project) -> BTreeSet<(Sel, Sel)> {
    let mut out = BTreeSet::new();
    for row in RELATIONS {
        for source in sources_of(project, row.from) {
            for target in related(project, row.key, source) {
                out.insert((Sel(source), Sel(target)));
            }
        }
    }
    out
}

#[test]
fn every_model_link_has_a_registry_row_and_vice_versa() {
    let project = project();
    let model = model_links(&project);
    let registry = registry_links(&project);
    let only_model: Vec<_> = model.difference(&registry).collect();
    let only_registry: Vec<_> = registry.difference(&model).collect();
    assert!(
        only_model.is_empty() && only_registry.is_empty(),
        "model and registry disagree.\nonly in model: {only_model:?}\nonly in registry: {only_registry:?}"
    );
    let len = model.len();
    assert!(len >= 12, "expected at least 12 distinct links, got {len}");
}

#[test]
fn every_link_is_reachable_both_ways() {
    let project = project();
    for (source, target) in model_links(&project) {
        let (source, target) = (source.0, target.0);
        let from = ElementKind::kind_of(source).expect("source kind");
        let to = ElementKind::kind_of(target).expect("target kind");
        // DataFlow->Component is shared by two rows, so disambiguate by which
        // row's read actually contains this target before requiring one match.
        let matches: Vec<_> = RELATIONS
            .iter()
            .filter(|row| row.from == from && row.to.contains(&to))
            .filter(|row| related(&project, row.key, source).contains(&target))
            .collect();
        let n = matches.len();
        assert_eq!(
            n, 1,
            "expected exactly one relation for {source:?} -> {target:?}, got {n}"
        );
        let row = matches[0];
        let key = row.key.as_str();
        assert!(
            related(&project, row.key, source).contains(&target),
            "forward read of {key} misses {target:?}"
        );
        assert!(
            reverse_related(&project, row.key, target).contains(&source),
            "reverse read of {key} misses {source:?}"
        );
        assert!(
            linked(&project, source).iter().any(|(k, _)| *k == row.key),
            "linked({source:?}) misses {key}"
        );
        assert!(
            linked(&project, target).iter().any(|(k, _)| *k == row.key),
            "linked({target:?}) misses {key}"
        );
    }
}

#[test]
fn keys_and_labels_are_valid() {
    let expected = [
        (RelationKey::ASSET_COMPONENT, "asset-component"),
        (RelationKey::THREAT_ASSET, "threat-asset"),
        (RelationKey::THREAT_DAMAGES, "threat-damages"),
        (RelationKey::THREAT_TREE, "threat-tree"),
        (RelationKey::THREAT_GOAL, "threat-goal"),
        (RelationKey::THREAT_CLAIM, "threat-claim"),
        (RelationKey::GOAL_CONTROLS, "goal-controls"),
        (RelationKey::GOAL_RESPONSIBLE, "goal-responsible"),
        (RelationKey::CLAIM_RESPONSIBLE, "claim-responsible"),
        (RelationKey::CONTROL_TARGETS, "control-targets"),
        (RelationKey::STEP_VIA, "step-via"),
        (RelationKey::SHARED_STEP_VIA, "shared-step-via"),
        (RelationKey::SHARED_STEP_USED_IN, "shared-step-used-in"),
        (RelationKey::FRAGMENT_USED_IN, "fragment-used-in"),
        (RelationKey::STEP_MITIGATIONS, "step-mitigations"),
        (RelationKey::DATAFLOW_FROM, "dataflow-from"),
        (RelationKey::DATAFLOW_TO, "dataflow-to"),
        (RelationKey::DATAFLOW_INTERFACE, "dataflow-interface"),
        (RelationKey::INTERFACE_COMPONENT, "interface-component"),
        (RelationKey::BOUNDARY_MEMBERS, "boundary-members"),
        (RelationKey::ASSET_CONDITIONS, "asset-conditions"),
        (RelationKey::COMPONENT_CONDITIONS, "component-conditions"),
        (RelationKey::INTERFACE_CONDITIONS, "interface-conditions"),
        (RelationKey::THREAT_CONDITIONS, "threat-conditions"),
        (RelationKey::TREE_CONDITIONS, "tree-conditions"),
        (RelationKey::GOAL_CONDITIONS, "goal-conditions"),
        (RelationKey::CLAIM_CONDITIONS, "claim-conditions"),
        (RelationKey::STEP_CONDITIONS, "step-conditions"),
        (RelationKey::CLAIM_ASSUMPTIONS, "claim-assumptions"),
        (RelationKey::ASSUMPTION_CONDITIONS, "assumption-conditions"),
        (RelationKey::ASSUMPTION_EXCLUDES, "assumption-excludes"),
    ];
    let keys: HashSet<RelationKey> = RELATIONS.iter().map(|row| row.key).collect();
    assert_eq!(keys.len(), RELATIONS.len(), "a relation key repeats");

    for (key, name) in expected {
        assert_eq!(key.as_str(), name);
        assert!(relation(key).is_some(), "{name} has no row");
    }

    for language in [Language::En, Language::Ja, Language::Pseudo] {
        let catalog = Catalog::load(language);
        for row in RELATIONS {
            assert!(
                expected.iter().any(|(key, _)| *key == row.key),
                "unexpected key {:?}",
                row.key.as_str()
            );
            let label = catalog.tr(row.label);
            assert!(
                !label.starts_with('['),
                "missing label {:?} in {language}: {label}",
                row.label
            );
        }
    }
}

#[test]
fn many_matches_the_fixture() {
    let project = project();
    for row in RELATIONS {
        let mut count = None;
        for source in sources_of(&project, row.from) {
            let targets = related(&project, row.key, source).len();
            if targets > 0 {
                assert!(
                    count.is_none(),
                    "row {:?} has more than one owning source",
                    row.key.as_str()
                );
                count = Some(targets);
            }
        }
        let key = row.key.as_str();
        let many = row.many;
        let count = count.unwrap_or_else(|| panic!("row {key} has no source in the fixture"));
        assert_eq!(
            many,
            count > 1,
            "row {key}: many={many} but found {count} targets"
        );
    }
}

/// The new heading, relation and party-role keys resolve in every bundled
/// locale. Hyphenated ids only (L27).
#[test]
fn new_keys_resolve_in_every_language() {
    let ids = [
        "responsible",
        "relation-goal-responsible",
        "relation-claim-responsible",
        "party-us",
        "party-customer",
        "party-supplier",
        "party-other",
        "condition",
        "condition-present",
        "assumption",
        "assumptions",
        "assumption-holds",
        "relation-claim-assumptions",
        "relation-assumption-conditions",
        "relation-assumption-excludes",
        "inspector-rev-claim-assumptions",
        "inspector-rev-assumption-excludes",
    ];
    for language in [Language::En, Language::Ja, Language::Pseudo] {
        let catalog = Catalog::load(language);
        for id in ids {
            let text = catalog.tr(id);
            assert!(
                !text.starts_with('[') && !text.is_empty(),
                "missing {id} in {language}: {text}"
            );
        }
    }
    assert_eq!(Catalog::load(Language::Ja).tr("responsible"), "責任者");
}

/// A control can target an interface and a step can go via one, so the picker
/// must offer interfaces for both rows, not only components.
#[test]
fn item_targets_offer_interfaces() {
    let project = project();
    let iface = project
        .interfaces()
        .map(|(_, id, _)| Selection::Interface(id))
        .next()
        .expect("fixture interface");
    for key in [RelationKey::CONTROL_TARGETS, RelationKey::STEP_VIA] {
        let offered = aegis_client::relations::candidates(&project, key);
        assert!(
            offered.contains(&iface),
            "{} misses interfaces",
            key.as_str()
        );
        assert!(offered.iter().any(|s| matches!(s, Selection::Component(_))));
    }
}

#[test]
fn assumption_excludes_candidates_omit_gates() {
    let project = project();
    let offered = aegis_client::relations::candidates(&project, RelationKey::ASSUMPTION_EXCLUDES);
    let (_, tid, tree) = project.attack_trees().next().expect("tree");
    let root_id = tree.id_of(tree.root()).expect("root");
    assert!(offered.contains(&Selection::Tree(tid)));
    assert!(!offered.contains(&Selection::TreeNode(tid, root_id)));
    assert!(
        offered
            .iter()
            .any(|s| matches!(s, Selection::TreeNode(_, _)))
    );
}
