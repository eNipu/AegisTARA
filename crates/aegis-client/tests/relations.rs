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
    AssetId, AttackTree, ClaimId, ComponentId, ControlId, DamageId, DataFlowId, GateKind, GoalId,
    ItemRef, Methodology, Node, NodeId, Op, Project, Provenance, ThreatId, TreeId,
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
    let (flow, asset, d1, d2, threat) = (
        DataFlowId::new_v7(),
        AssetId::new_v7(),
        DamageId::new_v7(),
        DamageId::new_v7(),
        ThreatId::new_v7(),
    );
    let (tree_id, root, step) = (TreeId::new_v7(), NodeId::new_v7(), NodeId::new_v7());
    let (control1, control2, goal, claim) = (
        ControlId::new_v7(),
        ControlId::new_v7(),
        GoalId::new_v7(),
        ClaimId::new_v7(),
    );

    let mut tree = AttackTree::new(root, Node::gate(GateKind::Or, "root")).expect("tree");
    tree.add_child(tree.root(), step, Node::step("step"))
        .expect("child");

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
        Op::AddDataFlow {
            id: flow,
            name: "flow".into(),
            from: c1,
            to: c2,
            interface: None,
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
            tree: tree_id,
            node: step,
            via: Some(ItemRef::Component(c1)),
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
        Op::SetThreatGoal {
            threat,
            goal: Some(goal),
            provenance: provenance.clone(),
        },
        Op::AddCybersecurityClaim {
            id: claim,
            name: "claim".into(),
            provenance: provenance.clone(),
        },
        Op::SetThreatClaim {
            threat,
            claim: Some(claim),
            provenance,
        },
    ];

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
        ElementKind::Control => project.security_controls().map(|(_, id, _)| Selection::Control(id)).collect(),
        ElementKind::DataFlow => project.data_flows().map(|(_, id, _)| Selection::DataFlow(id)).collect(),
        ElementKind::Step => project
            .attack_trees()
            .flat_map(|(_, id, tree)| tree.iter().map(move |(_, node, _)| Selection::TreeNode(id, node)))
            .collect(),
        _ => Vec::new(),
    }
}

/// Enumerates every link from the model arenas directly, independent of
/// `RELATIONS`. A model link with no registry row makes this set disagree with
/// `registry_links`.
fn model_links(project: &Project) -> BTreeSet<(Sel, Sel)> {
    let mut out = BTreeSet::new();

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
        if let Some(goal) = threat.goal().and_then(|g| project.cybersecurity_goal_id(g)) {
            out.insert((source, Sel(Selection::Goal(goal))));
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

    for (_, id, _) in project.security_controls() {
        if let Some(idx) = project.security_control_idx(&id) {
            for item in project.control_targets(idx) {
                if let Some(target) = item_selection(item) {
                    out.insert((Sel(Selection::Control(id)), Sel(target)));
                }
            }
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
    }

    for (tree_idx, tree_id, tree) in project.attack_trees() {
        for (node_idx, node_id, _) in tree.iter() {
            let source = Sel(Selection::TreeNode(tree_id, node_id));
            if let Some(via) = project
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
            .filter(|row| row.from == from && row.to == to)
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
        (RelationKey::CONTROL_TARGETS, "control-targets"),
        (RelationKey::STEP_VIA, "step-via"),
        (RelationKey::STEP_MITIGATIONS, "step-mitigations"),
        (RelationKey::DATAFLOW_FROM, "dataflow-from"),
        (RelationKey::DATAFLOW_TO, "dataflow-to"),
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
