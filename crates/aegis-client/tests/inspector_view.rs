//! Inspector core types, Fluent keys, explain-why causes, and `primary_cause`
//! resolution (WBX-06, WBX-11).

use std::sync::Arc;

use aegis_client::commands::Edit;
use aegis_client::i18n::{Catalog, Language};
use aegis_client::inspector::{
    exclusion_causes, explain_selection, inspector_view, primary_cause, reverse_relation_label,
};
use aegis_client::relations::{RELATIONS, RelationKey};
use aegis_client::table::selection_exclusion;
use aegis_client::view::Selection;
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{
    AssetId, AssumptionId, AttackTree, ClaimId, ComponentId, ConditionId, ControlId, DamageId,
    DataFlowId, ElementId, ElementKind, ElementRef, GateKind, GoalId, InterfaceId, ItemRef,
    Methodology, Node, NodeId, Op, PartyId, PartyKind, Provenance, RiskTreatment, SharedStepId,
    StepRef, StepTarget, ThreatId, TreeId, TrustBoundaryId,
};

const NEW_KEYS: &[&str] = &[
    "cmd-go-to-cause",
    "cmd-go-to-cause-tip",
    "inspector-details",
    "inspector-explain-why",
    "inspector-used-by",
    "inspector-used-in",
    "inspector-shared-step-line-one",
    "inspector-shared-step-line",
    "inspector-rev-shared-step-used-in",
    "inspector-deciding-step",
    "inspector-deciding-damage",
    "inspector-deciding-child",
    "inspector-capped-by",
    "inspector-excluded-by",
    "inspector-rev-asset-component",
    "inspector-rev-threat-asset",
    "inspector-rev-threat-damages",
    "inspector-rev-threat-tree",
    "inspector-rev-threat-goal",
    "inspector-rev-threat-claim",
    "inspector-rev-goal-controls",
    "inspector-rev-goal-responsible",
    "inspector-rev-claim-responsible",
    "inspector-rev-control-targets",
    "inspector-rev-step-via",
    "inspector-rev-step-mitigations",
    "inspector-rev-dataflow-from",
    "inspector-rev-dataflow-to",
    "inspector-rev-dataflow-interface",
    "inspector-rev-interface-component",
    "inspector-rev-boundary-members",
    "inspector-rev-conditions",
    "inspector-rev-claim-assumptions",
    "inspector-rev-assumption-excludes",
];

fn prov() -> Provenance {
    Provenance::human("tester", 1_792_956_600_000)
}

#[test]
fn all_inspector_fluent_keys_and_reverse_labels_resolve() {
    for language in [Language::En, Language::Ja, Language::Pseudo] {
        let catalog = Catalog::load(language);
        for &key in NEW_KEYS {
            let text = catalog.tr(key);
            assert!(
                !text.starts_with('['),
                "{language:?} missing Fluent key {key}: {text}"
            );
        }
        for rel in RELATIONS {
            let label = reverse_relation_label(rel.key, &catalog);
            assert!(
                !label.starts_with('['),
                "{language:?} missing reverse label for {:?}: {label}",
                rel.key
            );
        }
        let cond_label = reverse_relation_label(RelationKey::STEP_CONDITIONS, &catalog);
        assert_eq!(cond_label, catalog.tr("inspector-rev-conditions"));
        assert_eq!(
            reverse_relation_label(RelationKey::SHARED_STEP_USED_IN, &catalog),
            catalog.tr("inspector-rev-shared-step-used-in")
        );
        assert_eq!(
            reverse_relation_label(RelationKey::SHARED_STEP_VIA, &catalog),
            catalog.tr("inspector-rev-step-via")
        );
    }
}

#[test]
fn primary_cause_resolves_threat_tree_gate_step_damage_and_condition() {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let integrity = m.property("integrity").expect("integrity");
    let authenticity = m.property("authenticity").expect("authenticity");
    let safety = m
        .impact_category("safety")
        .or_else(|| m.impact_category("safety-health"))
        .expect("safety");
    let severe = m.level(safety, "severe").expect("severe");
    let negligible = m.level(safety, "negligible").expect("negligible");
    let spoofing = m.threat_category("spoofing").expect("spoofing");
    let high = m.feasibility_level("high").expect("high");
    let low = m.feasibility_level("low").expect("low");
    let very_low = m.feasibility_level("very-low").expect("very-low");

    let mut ws = Workspace::empty(Arc::clone(&m));
    let asset = AssetId::new_v7();
    let dmg_high = DamageId::new_v7();
    let dmg_low = DamageId::new_v7();
    let threat_tree = ThreatId::new_v7();
    let threat_direct = ThreatId::new_v7();
    let tree_id = TreeId::new_v7();
    let root = NodeId::new_v7();
    let sub_gate = NodeId::new_v7();
    let step_easy = NodeId::new_v7();
    let step_hard = NodeId::new_v7();
    let excluded_step = NodeId::new_v7();
    let ctrl = ControlId::new_v7();
    let cond = ConditionId::new_v7();
    let excluded_asset = AssetId::new_v7();

    let mut tree =
        AttackTree::new(root, Node::gate(GateKind::Or, "Spoof brake frame")).expect("root");
    let r = tree.root();
    let sg = tree
        .add_child(r, sub_gate, Node::gate(GateKind::And, "Inject on bus"))
        .expect("sub_gate");
    tree.add_child(sg, step_easy, Node::step("Send CAN frame"))
        .expect("step_easy");
    tree.add_child(r, step_hard, Node::step("Extract keys"))
        .expect("step_hard");
    tree.add_child(r, excluded_step, Node::step("Probe JTAG pins"))
        .expect("excluded_step");

    ws.apply(
        vec![
            Op::AddAsset {
                id: asset,
                name: "Brake command".into(),
                properties: [integrity, authenticity].into_iter().collect(),
                provenance: prov(),
            },
            Op::AddDamageScenario {
                id: dmg_high,
                name: "Unintended braking".into(),
                impacts: vec![(safety, severe)],
                provenance: prov(),
            },
            Op::AddDamageScenario {
                id: dmg_low,
                name: "Minor delay".into(),
                impacts: vec![(safety, negligible)],
                provenance: prov(),
            },
            Op::AddAttackTree {
                id: tree_id,
                tree,
                step_feasibility: vec![(step_easy, high), (step_hard, low)],
                provenance: prov(),
            },
            Op::AddSecurityControl {
                id: ctrl,
                name: "SecOC".into(),
                provenance: prov(),
            },
            Op::SetStepMitigation {
                tree: tree_id,
                node: step_easy,
                control: ctrl,
                feasibility: Some(very_low),
                provenance: prov(),
            },
            Op::AddThreatScenario {
                id: threat_tree,
                name: "Inject brake message".into(),
                asset,
                category: spoofing,
                catalog_refs: Vec::new(),
                damages: vec![dmg_low, dmg_high],
                provenance: prov(),
            },
            Op::SetThreatAttackTree {
                threat: threat_tree,
                tree: Some(tree_id),
                provenance: prov(),
            },
            Op::AddThreatScenario {
                id: threat_direct,
                name: "Replay diagnostic session".into(),
                asset,
                category: spoofing,
                catalog_refs: Vec::new(),
                damages: vec![dmg_low, dmg_high],
                provenance: prov(),
            },
            Op::SetThreatFeasibility {
                threat: threat_direct,
                level: Some(high),
                provenance: prov(),
            },
            Op::AddCondition {
                id: cond,
                name: "Debug port fitted".into(),
                description: "".into(),
                present: false,
                provenance: prov(),
            },
            Op::AddAsset {
                id: excluded_asset,
                name: "JTAG trace buffer".into(),
                properties: [integrity].into_iter().collect(),
                provenance: prov(),
            },
        ],
        Origin::Edit,
    )
    .expect("setup project");

    for edit in [
        Edit::SetConditionLink {
            element: ElementRef::Asset(excluded_asset),
            condition: cond,
            linked: true,
        },
        Edit::SetStepConditionLink {
            tree: tree_id,
            node: excluded_step,
            condition: cond,
            linked: true,
        },
    ] {
        let ops = edit
            .to_ops(ws.project(), "tester", 1_792_956_600_000)
            .expect("to_ops");
        ws.apply(ops, Origin::Edit).expect("apply condition link");
    }

    let catalog = Catalog::load(Language::En);

    // 1. Threat with attack tree: residual evaluation caps `step_easy` to `very_low`,
    // so `step_hard` (`low`) is now the deciding step of the OR root gate.
    assert_eq!(
        primary_cause(&ws, Selection::Threat(threat_tree)),
        Some(Selection::TreeNode(tree_id, step_hard))
    );
    let threat_explain =
        explain_selection(&ws, Selection::Threat(threat_tree), &catalog).expect("explain threat");
    assert_eq!(
        threat_explain.causes.first().map(|c| c.target),
        Some(Selection::TreeNode(tree_id, step_hard))
    );
    assert!(
        threat_explain
            .causes
            .iter()
            .any(|c| c.target == Selection::Damage(dmg_high)),
        "expected deciding damage in threat causes"
    );

    // 2. Threat with direct feasibility (and no tree) jumps to its deciding damage scenario.
    assert_eq!(
        primary_cause(&ws, Selection::Threat(threat_direct)),
        Some(Selection::Damage(dmg_high))
    );

    // 3. Attack tree selection jumps to the deciding leaf step.
    assert_eq!(
        primary_cause(&ws, Selection::Tree(tree_id)),
        Some(Selection::TreeNode(tree_id, step_hard))
    );

    // 4. Gate TreeNode jumps to its decisive child; sub_gate's decisive child is step_easy.
    assert_eq!(
        primary_cause(&ws, Selection::TreeNode(tree_id, root)),
        Some(Selection::TreeNode(tree_id, step_hard))
    );
    assert_eq!(
        primary_cause(&ws, Selection::TreeNode(tree_id, sub_gate)),
        Some(Selection::TreeNode(tree_id, step_easy))
    );

    // 5. Capped step TreeNode jumps to the capping SecurityControl.
    assert_eq!(
        primary_cause(&ws, Selection::TreeNode(tree_id, step_easy)),
        Some(Selection::Control(ctrl))
    );
    let step_explain = explain_selection(&ws, Selection::TreeNode(tree_id, step_easy), &catalog)
        .expect("explain capped step");
    assert!(!step_explain.lines.is_empty());
    assert_eq!(step_explain.causes[0].role, "Capped by");
    assert_eq!(step_explain.causes[0].target, Selection::Control(ctrl));

    // 6. Asset and step excluded by an off condition jump to Selection::Condition(cond).
    assert_eq!(
        primary_cause(&ws, Selection::Asset(excluded_asset)),
        Some(Selection::Condition(cond))
    );
    assert_eq!(
        primary_cause(&ws, Selection::TreeNode(tree_id, excluded_step)),
        Some(Selection::Condition(cond))
    );
    let asset_excl = exclusion_causes(ws.project(), Selection::Asset(excluded_asset), &catalog);
    assert_eq!(asset_excl.len(), 1);
    assert_eq!(asset_excl[0].role, "Excluded by");
    assert_eq!(asset_excl[0].target, Selection::Condition(cond));
    let step_excl = exclusion_causes(
        ws.project(),
        Selection::TreeNode(tree_id, excluded_step),
        &catalog,
    );
    assert_eq!(step_excl.len(), 1);
    assert_eq!(step_excl[0].target, Selection::Condition(cond));
}

#[test]
fn inspector_view_populates_all_14_selection_kinds_relations_systems_and_exclusions() {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let integrity = m.property("integrity").expect("integrity");
    let authenticity = m.property("authenticity").expect("authenticity");
    let safety = m
        .impact_category("safety")
        .or_else(|| m.impact_category("safety-health"))
        .expect("safety");
    let severe = m.level(safety, "severe").expect("severe");
    let spoofing = m.threat_category("spoofing").expect("spoofing");
    let high = m.feasibility_level("high").expect("high");
    let low = m.feasibility_level("low").expect("low");
    let very_low = m.feasibility_level("very-low").expect("very-low");
    let iface_kind = m.interface_kinds().next().expect("interface kind");
    let attack_type = m.attack_types().next().expect("attack type");
    let annex5 = m.catalog_scheme("r155-annex5").expect("annex5");
    let cat_ref = m.parse_catalog_ref(annex5, "4.1").expect("4.1");

    let mut ws = Workspace::empty(Arc::clone(&m));
    let sys_a = ElementId::new_v7();
    let sys_b = ElementId::new_v7();
    let comp_a = ComponentId::new_v7();
    let comp_b = ComponentId::new_v7();
    let iface_a = InterfaceId::new_v7();
    let flow = DataFlowId::new_v7();
    let boundary = TrustBoundaryId::new_v7();
    let asset_a = AssetId::new_v7();
    let asset_b = AssetId::new_v7();
    let dmg = DamageId::new_v7();
    let threat_a = ThreatId::new_v7();
    let threat_b = ThreatId::new_v7();
    let tree_id = TreeId::new_v7();
    let root_gate = NodeId::new_v7();
    let step_via_iface = NodeId::new_v7();
    let step_excluded = NodeId::new_v7();
    let ctrl = ControlId::new_v7();
    let goal = GoalId::new_v7();
    let claim = ClaimId::new_v7();
    let party = PartyId::new_v7();
    let cond = ConditionId::new_v7();

    let mut tree =
        AttackTree::new(root_gate, Node::gate(GateKind::Or, "Inject CAN frame")).expect("root");
    let r = tree.root();
    tree.add_child(r, step_via_iface, Node::step("Transmit spoofed ID"))
        .expect("step_via_iface");
    tree.add_child(r, step_excluded, Node::step("Use debug header"))
        .expect("step_excluded");

    ws.apply(
        vec![
            Op::AddElement {
                id: sys_a,
                kind: ElementKind::System,
                name: "ADAS".into(),
                provenance: prov(),
            },
            Op::AddElement {
                id: sys_b,
                kind: ElementKind::System,
                name: "Braking".into(),
                provenance: prov(),
            },
            Op::AddComponent {
                id: comp_a,
                name: "Radar ECU".into(),
                parent: None,
                provenance: prov(),
            },
            Op::AddComponent {
                id: comp_b,
                name: "Brake ECU".into(),
                parent: None,
                provenance: prov(),
            },
            Op::SetComponentSystem {
                id: comp_a,
                system: Some(sys_a),
                provenance: prov(),
            },
            Op::SetComponentSystem {
                id: comp_b,
                system: Some(sys_b),
                provenance: prov(),
            },
            Op::AddInterface {
                id: iface_a,
                component: comp_a,
                name: "CAN0".into(),
                kind: iface_kind,
                provenance: prov(),
            },
            Op::AddDataFlow {
                id: flow,
                name: "Deceleration request".into(),
                from: comp_a,
                to: comp_b,
                interface: Some(iface_a),
                provenance: prov(),
            },
            Op::AddTrustBoundary {
                id: boundary,
                name: "Chassis domain".into(),
                components: vec![comp_a],
                provenance: prov(),
            },
            Op::AddAsset {
                id: asset_a,
                name: "Radar object track".into(),
                properties: [integrity, authenticity].into_iter().collect(),
                provenance: prov(),
            },
            Op::SetAssetComponent {
                asset: asset_a,
                component: Some(comp_a),
                provenance: prov(),
            },
            Op::AddAsset {
                id: asset_b,
                name: "Brake actuation command".into(),
                properties: [integrity, authenticity].into_iter().collect(),
                provenance: prov(),
            },
            Op::SetAssetComponent {
                asset: asset_b,
                component: Some(comp_b),
                provenance: prov(),
            },
            Op::AddDamageScenario {
                id: dmg,
                name: "Collision with lead vehicle".into(),
                impacts: vec![(safety, severe)],
                provenance: prov(),
            },
            Op::SetImpactRationale {
                damage: dmg,
                text: "Severe frontal crash at highway speed.".into(),
                provenance: prov(),
            },
            Op::AddAttackTree {
                id: tree_id,
                tree,
                step_feasibility: vec![(step_via_iface, high), (step_excluded, high)],
                provenance: prov(),
            },
            Op::SetStepVia {
                target: StepTarget::Step(tree_id, step_via_iface),
                via: Some(ItemRef::Interface(iface_a)),
                provenance: prov(),
            },
            Op::SetStepAttackType {
                target: StepTarget::Step(tree_id, step_via_iface),
                attack_type: Some(attack_type),
                provenance: prov(),
            },
            Op::AddSecurityControl {
                id: ctrl,
                name: "SecOC authentication".into(),
                provenance: prov(),
            },
            Op::ApplyControl {
                control: ctrl,
                target: ItemRef::Component(comp_a),
                provenance: prov(),
            },
            Op::SetControlRule {
                control: ctrl,
                attack_type,
                ceiling: Some(low),
                provenance: prov(),
            },
            Op::SetStepMitigation {
                tree: tree_id,
                node: step_via_iface,
                control: ctrl,
                feasibility: Some(very_low),
                provenance: prov(),
            },
            Op::AddParty {
                id: party,
                name: "Tier-1 Supplier".into(),
                kind: PartyKind::Supplier,
                provenance: prov(),
            },
            Op::AddCybersecurityGoal {
                id: goal,
                name: "Authenticate chassis frames".into(),
                controls: vec![ctrl],
                provenance: prov(),
            },
            Op::LinkGoalParty {
                goal,
                party,
                provenance: prov(),
            },
            Op::AddCybersecurityClaim {
                id: claim,
                name: "Out-of-band calibration handled in plant".into(),
                provenance: prov(),
            },
            Op::LinkClaimParty {
                claim,
                party,
                provenance: prov(),
            },
            Op::AddThreatScenario {
                id: threat_a,
                name: "Spoof radar track".into(),
                asset: asset_a,
                category: spoofing,
                catalog_refs: vec![cat_ref],
                damages: vec![dmg],
                provenance: prov(),
            },
            Op::SetThreatAttackTree {
                threat: threat_a,
                tree: Some(tree_id),
                provenance: prov(),
            },
            Op::SetRiskTreatment {
                threat: threat_a,
                treatment: Some(RiskTreatment::Reduce),
                provenance: prov(),
            },
            Op::SetThreatGoal {
                threat: threat_a,
                goal: Some(goal),
                provenance: prov(),
            },
            Op::SetThreatScaling {
                threat: threat_a,
                scales: Some(true),
                reason: "Fleet-wide CAN IDs.".into(),
                provenance: prov(),
            },
            Op::SetDamageLinkRationale {
                threat: threat_a,
                damage: dmg,
                text: "Fake track suppresses AEB.".into(),
                provenance: prov(),
            },
            Op::SetFeasibilityRationale {
                threat: threat_a,
                text: "Assessed via tree.".into(),
                provenance: prov(),
            },
            Op::AddThreatScenario {
                id: threat_b,
                name: "Spoof brake command".into(),
                asset: asset_b,
                category: spoofing,
                catalog_refs: Vec::new(),
                damages: vec![dmg],
                provenance: prov(),
            },
            Op::SetThreatFeasibility {
                threat: threat_b,
                level: Some(high),
                provenance: prov(),
            },
            Op::SetRiskTreatment {
                threat: threat_b,
                treatment: Some(RiskTreatment::Share),
                provenance: prov(),
            },
            Op::SetThreatClaim {
                threat: threat_b,
                claim: Some(claim),
                provenance: prov(),
            },
            Op::AddCondition {
                id: cond,
                name: "Engineering debug harness".into(),
                description: "Only present on prototype builds.".into(),
                present: false,
                provenance: prov(),
            },
            Op::SetDescription {
                target: ElementRef::Asset(asset_a),
                text: "Object list transmitted over CAN.".into(),
                provenance: prov(),
            },
        ],
        Origin::Edit,
    )
    .expect("apply fixture ops");

    for edit in [
        Edit::SetConditionLink {
            element: ElementRef::Asset(asset_b),
            condition: cond,
            linked: true,
        },
        Edit::SetStepConditionLink {
            tree: tree_id,
            node: step_excluded,
            condition: cond,
            linked: true,
        },
    ] {
        let ops = edit
            .to_ops(ws.project(), "tester", 1_792_956_600_000)
            .expect("to_ops");
        ws.apply(ops, Origin::Edit).expect("apply condition links");
    }

    let catalog = Catalog::load(Language::En);

    // Unknown selections return None.
    assert!(inspector_view(&ws, Selection::Asset(AssetId::new_v7()), &catalog).is_none());
    assert!(
        inspector_view(
            &ws,
            Selection::TreeNode(tree_id, NodeId::new_v7()),
            &catalog
        )
        .is_none()
    );

    // All 14 Selection kinds return Some(InspectorView) with non-empty visual_id, name, kind, changed.
    let all_14 = [
        Selection::Component(comp_a),
        Selection::Interface(iface_a),
        Selection::DataFlow(flow),
        Selection::TrustBoundary(boundary),
        Selection::Asset(asset_a),
        Selection::Damage(dmg),
        Selection::Threat(threat_a),
        Selection::Tree(tree_id),
        Selection::TreeNode(tree_id, root_gate),
        Selection::TreeNode(tree_id, step_via_iface),
        Selection::Control(ctrl),
        Selection::Goal(goal),
        Selection::Claim(claim),
        Selection::Party(party),
        Selection::Condition(cond),
    ];
    for sel in all_14 {
        let view = inspector_view(&ws, sel, &catalog)
            .unwrap_or_else(|| panic!("expected Some(InspectorView) for {sel:?}"));
        assert_eq!(view.selection, sel);
        assert!(!view.visual_id.is_empty(), "empty visual_id for {sel:?}");
        assert!(!view.name.is_empty(), "empty name for {sel:?}");
        assert!(!view.kind.is_empty(), "empty kind for {sel:?}");
        assert!(
            view.changed.contains("tester"),
            "missing provenance for {sel:?}: {}",
            view.changed
        );
    }

    // Check Asset forward & reverse relations, description, and systems.
    let asset_view = inspector_view(&ws, Selection::Asset(asset_a), &catalog).expect("asset_a");
    assert_eq!(asset_view.description, "Object list transmitted over CAN.");
    assert_eq!(asset_view.systems, vec!["ADAS".to_owned()]);
    assert!(
        asset_view.relations.iter().any(|r| {
            r.key == RelationKey::ASSET_COMPONENT
                && r.editable
                && r.chips
                    .iter()
                    .any(|c| c.target == Selection::Component(comp_a))
        }),
        "Asset forward relations must list bound Component"
    );
    assert!(
        asset_view.reverse_relations.iter().any(|r| {
            r.key == RelationKey::THREAT_ASSET
                && !r.editable
                && r.chips
                    .iter()
                    .any(|c| c.target == Selection::Threat(threat_a))
        }),
        "Asset reverse relations must list targeting Threat"
    );

    // Check Damage shared across ADAS and Braking systems + reverse relations + fields.
    let dmg_view = inspector_view(&ws, Selection::Damage(dmg), &catalog).expect("dmg");
    assert_eq!(
        dmg_view.systems,
        vec!["ADAS".to_owned(), "Braking".to_owned()]
    );
    assert!(
        dmg_view
            .fields
            .iter()
            .any(|f| f.value == "Severe frontal crash at highway speed.")
    );
    let rev_threats = dmg_view
        .reverse_relations
        .iter()
        .find(|r| r.key == RelationKey::THREAT_DAMAGES)
        .expect("THREAT_DAMAGES in Damage reverse_relations");
    assert_eq!(rev_threats.chips.len(), 2);

    // Check Component reverse relations include bound Asset, Interface, and STEP_VIA step (even via interface!).
    let comp_view = inspector_view(&ws, Selection::Component(comp_a), &catalog).expect("comp_a");
    assert_eq!(comp_view.systems, vec!["ADAS".to_owned()]);
    for (expected_key, expected_target) in [
        (RelationKey::ASSET_COMPONENT, Selection::Asset(asset_a)),
        (
            RelationKey::INTERFACE_COMPONENT,
            Selection::Interface(iface_a),
        ),
        (
            RelationKey::STEP_VIA,
            Selection::TreeNode(tree_id, step_via_iface),
        ),
        (RelationKey::CONTROL_TARGETS, Selection::Control(ctrl)),
        (
            RelationKey::BOUNDARY_MEMBERS,
            Selection::TrustBoundary(boundary),
        ),
        (RelationKey::DATAFLOW_FROM, Selection::DataFlow(flow)),
    ] {
        assert!(
            comp_view.reverse_relations.iter().any(|r| {
                r.key == expected_key && r.chips.iter().any(|c| c.target == expected_target)
            }),
            "Component reverse_relations missing {expected_key:?} -> {expected_target:?}"
        );
    }

    // Check Control reverse relations include GOAL_CONTROLS and STEP_MITIGATIONS capped step.
    let ctrl_view = inspector_view(&ws, Selection::Control(ctrl), &catalog).expect("ctrl");
    assert_eq!(ctrl_view.systems, vec!["ADAS".to_owned()]);
    assert!(
        ctrl_view
            .fields
            .iter()
            .any(|f| f.label == catalog.tr("rules") && f.value.contains('\u{2192}'))
    );
    assert!(ctrl_view.reverse_relations.iter().any(|r| {
        r.key == RelationKey::GOAL_CONTROLS
            && r.chips.iter().any(|c| c.target == Selection::Goal(goal))
    }));
    assert!(ctrl_view.reverse_relations.iter().any(|r| {
        r.key == RelationKey::STEP_MITIGATIONS
            && r.chips
                .iter()
                .any(|c| c.target == Selection::TreeNode(tree_id, step_via_iface))
    }));

    // Check Party reverse relations include GOAL_RESPONSIBLE and CLAIM_RESPONSIBLE.
    let party_view = inspector_view(&ws, Selection::Party(party), &catalog).expect("party");
    assert!(party_view.systems.is_empty());
    assert!(
        party_view
            .fields
            .iter()
            .any(|f| f.value == catalog.tr("party-supplier"))
    );
    assert!(party_view.reverse_relations.iter().any(|r| {
        r.key == RelationKey::GOAL_RESPONSIBLE
            && r.chips.iter().any(|c| c.target == Selection::Goal(goal))
    }));
    assert!(party_view.reverse_relations.iter().any(|r| {
        r.key == RelationKey::CLAIM_RESPONSIBLE
            && r.chips.iter().any(|c| c.target == Selection::Claim(claim))
    }));

    // Check Condition description, fields, and reverse relations (elements & steps needing it).
    let cond_view = inspector_view(&ws, Selection::Condition(cond), &catalog).expect("cond");
    assert_eq!(cond_view.description, "Only present on prototype builds.");
    assert!(
        cond_view
            .fields
            .iter()
            .any(|f| f.label == catalog.tr("condition-present") && f.value == catalog.tr("no"))
    );
    assert!(cond_view.reverse_relations.iter().any(|r| {
        r.key == RelationKey::ASSET_CONDITIONS
            && r.chips
                .iter()
                .any(|c| c.target == Selection::Asset(asset_b))
    }));
    assert!(cond_view.reverse_relations.iter().any(|r| {
        r.key == RelationKey::STEP_CONDITIONS
            && r.chips
                .iter()
                .any(|c| c.target == Selection::TreeNode(tree_id, step_excluded))
    }));

    // Check excluded TreeNode and Asset exclusion_reason and exclusion_causes.
    let (step_is_excl, step_reason) =
        selection_exclusion(ws.project(), Selection::TreeNode(tree_id, step_excluded));
    assert!(step_is_excl);
    assert!(
        step_reason
            .as_deref()
            .is_some_and(|s| s.contains("Engineering debug harness"))
    );
    let excl_step_view = inspector_view(&ws, Selection::TreeNode(tree_id, step_excluded), &catalog)
        .expect("excl_step_view");
    assert_eq!(excl_step_view.kind, "Step");
    assert_eq!(excl_step_view.excluded_reason, step_reason);
    assert_eq!(excl_step_view.exclusion_causes.len(), 1);
    assert_eq!(
        excl_step_view.exclusion_causes[0].target,
        Selection::Condition(cond)
    );
    assert_eq!(
        excl_step_view.primary_cause,
        Some(Selection::Condition(cond))
    );

    // Check Threat fields, explain, and primary_cause.
    let threat_view = inspector_view(&ws, Selection::Threat(threat_a), &catalog).expect("threat_a");
    assert_eq!(
        threat_view.primary_cause,
        Some(Selection::TreeNode(tree_id, step_via_iface))
    );
    assert!(threat_view.explain.is_some());
    for expected_label in [
        catalog.tr("risk-before-controls"),
        catalog.tr("risk-after-controls"),
        catalog.tr("category"),
        catalog.tr("feasibility"),
        catalog.tr("deciding-damage"),
        catalog.tr("treatment"),
        catalog.tr("scales"),
        catalog.tr("catalog-references"),
        catalog.tr("pair-rationale"),
        catalog.tr("feasibility-rationale"),
    ] {
        assert!(
            threat_view.fields.iter().any(|f| f.label == expected_label),
            "Threat inspector missing field {expected_label}"
        );
    }
}

#[test]
fn assumption_inspector_relations_and_excluded_tree_step_causes() {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let mut ws = Workspace::empty(m);
    let (as_id, claim, cond) = (
        AssumptionId::new_v7(),
        ClaimId::new_v7(),
        ConditionId::new_v7(),
    );
    let (tree_id, root1) = (TreeId::new_v7(), NodeId::new_v7());
    let (tree2_id, root2, step_excl) = (TreeId::new_v7(), NodeId::new_v7(), NodeId::new_v7());
    let mut tree2 = AttackTree::new(root2, Node::gate(GateKind::Or, "Bypass lock")).expect("t2");
    tree2
        .add_child(tree2.root(), step_excl, Node::step("Pick lock"))
        .expect("step_excl");

    ws.apply(
        vec![
            Op::AddCondition {
                id: cond,
                name: "Locked enclosure".into(),
                description: "".into(),
                present: true,
                provenance: prov(),
            },
            Op::AddCybersecurityClaim {
                id: claim,
                name: "Enclosure prevents physical probing".into(),
                provenance: prov(),
            },
            Op::AddAttackTree {
                id: tree_id,
                tree: AttackTree::new(root1, Node::gate(GateKind::Or, "Tamper ECU")).expect("t1"),
                step_feasibility: vec![],
                provenance: prov(),
            },
            Op::AddAttackTree {
                id: tree2_id,
                tree: tree2,
                step_feasibility: vec![],
                provenance: prov(),
            },
            Op::AddAssumption {
                id: as_id,
                name: "ECU locked in steel box".into(),
                holds: true,
                provenance: prov(),
            },
            Op::LinkClaimAssumption {
                claim,
                assumption: as_id,
                provenance: prov(),
            },
            Op::NeedCondition {
                element: ElementRef::Assumption(as_id),
                condition: cond,
                provenance: prov(),
            },
            Op::SetAssumptionExcludes {
                id: as_id,
                trees: vec![tree_id],
                steps: vec![StepRef {
                    tree: tree2_id,
                    node: step_excl,
                }],
                provenance: prov(),
            },
        ],
        Origin::Edit,
    )
    .expect("setup");

    let catalog = Catalog::load(Language::En);
    let view = inspector_view(&ws, Selection::Assumption(as_id), &catalog).expect("assumption");
    assert_eq!(view.kind, "Assumption");
    assert!(
        view.fields
            .iter()
            .any(|f| f.label == "Holds" && f.value == "yes")
    );
    assert!(view.relations.iter().any(|r| {
        r.key == RelationKey::ASSUMPTION_EXCLUDES
            && r.label == "Excluded while it holds"
            && r.editable
            && r.chips.iter().any(|c| c.target == Selection::Tree(tree_id))
            && r.chips
                .iter()
                .any(|c| c.target == Selection::TreeNode(tree2_id, step_excl))
    }));
    assert!(view.relations.iter().any(|r| {
        r.key == RelationKey::ASSUMPTION_CONDITIONS
            && r.label == "Assumption conditions"
            && r.editable
            && r.chips
                .iter()
                .any(|c| c.target == Selection::Condition(cond))
    }));
    assert!(view.reverse_relations.iter().any(|r| {
        r.key == RelationKey::CLAIM_ASSUMPTIONS
            && r.label == "Claims"
            && !r.editable
            && r.chips.iter().any(|c| c.target == Selection::Claim(claim))
    }));

    for sel in [
        Selection::Tree(tree_id),
        Selection::TreeNode(tree2_id, step_excl),
    ] {
        let target_view = inspector_view(&ws, sel, &catalog).expect("excluded target");
        assert!(target_view.reverse_relations.iter().any(|r| {
            r.key == RelationKey::ASSUMPTION_EXCLUDES
                && r.label == "Excluded by assumptions"
                && !r.editable
                && r.chips
                    .iter()
                    .any(|c| c.target == Selection::Assumption(as_id))
        }));
        assert_eq!(target_view.exclusion_causes.len(), 1);
        assert_eq!(target_view.exclusion_causes[0].role, "Excluded by");
        assert_eq!(
            target_view.exclusion_causes[0].target,
            Selection::Assumption(as_id)
        );
        assert_eq!(
            target_view.primary_cause,
            Some(Selection::Assumption(as_id))
        );
    }

    // Conditioned off: assumption itself reports "Excluded by" -> Condition.
    let ops = Edit::SetConditionPresent {
        condition: cond,
        present: false,
    }
    .to_ops(ws.project(), "tester", 1_792_956_600_001)
    .expect("ops");
    ws.apply(ops, Origin::Edit).expect("condition off");
    let off_view = inspector_view(&ws, Selection::Assumption(as_id), &catalog).expect("off");
    assert_eq!(off_view.exclusion_causes.len(), 1);
    assert_eq!(off_view.exclusion_causes[0].role, "Excluded by");
    assert_eq!(
        off_view.exclusion_causes[0].target,
        Selection::Condition(cond)
    );
    assert_eq!(off_view.primary_cause, Some(Selection::Condition(cond)));

    // Holds = false shows "no".
    let ops = Edit::SetAssumptionHolds {
        assumption: as_id,
        holds: false,
    }
    .to_ops(ws.project(), "tester", 1_792_956_600_002)
    .expect("ops");
    ws.apply(ops, Origin::Edit).expect("holds off");
    let no_view = inspector_view(&ws, Selection::Assumption(as_id), &catalog).expect("no");
    assert!(
        no_view
            .fields
            .iter()
            .any(|f| f.label == "Holds" && f.value == "no")
    );
}

#[test]
fn shared_step_selection_has_visual_id_ratings_and_forward_relations() {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let mut ws = Workspace::empty(Arc::clone(&m));
    let shared = SharedStepId::new_v7();
    let component = ComponentId::new_v7();
    let tree_id = TreeId::new_v7();
    let root = NodeId::new_v7();
    let step = NodeId::new_v7();
    let mut tree = AttackTree::new(root, Node::gate(GateKind::Or, "Root")).expect("root");
    tree.add_child(tree.root(), step, Node::step("Linked"))
        .expect("step");
    let factor = m.feasibility_factors().next().expect("factor");
    let level = m.feasibility_level("high").expect("high");
    let factor_level = m.factor_levels(factor).next().expect("factor level");
    let attack_type = m.attack_types().next().expect("attack type");
    ws.apply(
        vec![
            Op::AddComponent {
                id: component,
                name: "Gateway".into(),
                parent: None,
                provenance: prov(),
            },
            Op::AddSharedStep {
                id: shared,
                name: "Reusable".into(),
                provenance: prov(),
            },
            Op::SetStepFeasibility {
                target: StepTarget::Shared(shared),
                level: Some(level),
                provenance: prov(),
            },
            Op::SetStepFactor {
                target: StepTarget::Shared(shared),
                factor,
                level: Some(factor_level),
                provenance: prov(),
            },
            Op::SetStepVia {
                target: StepTarget::Shared(shared),
                via: Some(ItemRef::Component(component)),
                provenance: prov(),
            },
            Op::SetStepAttackType {
                target: StepTarget::Shared(shared),
                attack_type: Some(attack_type),
                provenance: prov(),
            },
            Op::AddAttackTree {
                id: tree_id,
                tree,
                step_feasibility: Vec::new(),
                provenance: prov(),
            },
            Op::UseSharedStep {
                tree: tree_id,
                node: step,
                step: shared,
                provenance: prov(),
            },
            Op::SetDescription {
                target: ElementRef::SharedStep(shared),
                text: "Shared description".into(),
                provenance: prov(),
            },
        ],
        Origin::Edit,
    )
    .expect("shared fixture");
    let catalog = Catalog::load(Language::En);
    let view = inspector_view(&ws, Selection::SharedStep(shared), &catalog).expect("shared view");
    assert_eq!(view.visual_id, "L00");
    assert_eq!(view.name, "Reusable");
    assert_eq!(view.kind, "Shared step");
    assert_eq!(view.description, "Shared description");
    assert!(view.changed.contains("tester"));
    assert!(view.fields.len() >= 4);
    assert!(view.relations.iter().any(|row| {
        row.key == RelationKey::SHARED_STEP_VIA
            && row.chips[0].target == Selection::Component(component)
    }));
    let used = view
        .relations
        .iter()
        .find(|row| row.key == RelationKey::SHARED_STEP_USED_IN)
        .expect("used-in row");
    assert_eq!(used.chips[0].target, Selection::TreeNode(tree_id, step));
    assert!(inspector_view(&ws, Selection::SharedStep(SharedStepId::new_v7()), &catalog).is_none());
}
