//! Inspector core types, Fluent keys, explain-why causes, and `primary_cause`
//! resolution (WBX-06, WBX-11).

#[path = "common/fragments.rs"]
mod fragments;

use std::sync::Arc;

use aegis_canvas::Tone;
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
    AssetId, AssumptionId, AttackTree, ClaimId, ComponentId, ConditionId, ControlId, ControlStatus,
    DamageId, DataFlowId, ElementId, ElementKind, ElementRef, GateKind, GoalId, InterfaceId,
    ItemRef, Methodology, Node, NodeId, Op, PartyId, PartyKind, Provenance, RiskTreatment,
    SharedStepId, StepRef, StepTarget, ThreatId, TreeId, TrustBoundaryId,
};

const NEW_KEYS: &[&str] = &[
    "cmd-go-to-cause",
    "cmd-go-to-cause-tip",
    "inspector-details",
    "inspector-explain-why",
    "inspector-used-by",
    "inspector-used-in",
    "fragment",
    "inspector-fragment-reference",
    "inspector-rev-fragment-used-in",
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
        for key in ["fragment", "inspector-rev-fragment-used-in"] {
            let value = catalog.tr(key);
            assert_ne!(value, key, "{language:?} fallback key");
            assert_ne!(value, format!("[{key}]"), "{language:?} missing key");
            if language == Language::Pseudo {
                assert!(value.starts_with('⟦'), "pseudo must be visibly localized");
            } else {
                assert_eq!(
                    value,
                    if language == Language::En {
                        "Fragment"
                    } else {
                        "フラグメント"
                    }
                );
            }
        }
        assert_eq!(
            catalog.tr("inspector-fragment-reference"),
            match language {
                Language::En => "Fragment reference",
                Language::Ja => "フラグメント参照",
                Language::Pseudo => "⟦Frâgmént référéncé␣⟧",
            }
        );
        assert_eq!(
            reverse_relation_label(RelationKey::FRAGMENT_USED_IN, &catalog),
            catalog.tr("inspector-rev-fragment-used-in"),
        );
        assert_ne!(
            reverse_relation_label(RelationKey::FRAGMENT_USED_IN, &catalog),
            catalog.tr("inspector-rev-conditions"),
        );
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
                role: Default::default(),
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
                role: Default::default(),
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
                role: Default::default(),
                id: tree_id,
                tree: AttackTree::new(root1, Node::gate(GateKind::Or, "Tamper ECU")).expect("t1"),
                step_feasibility: vec![],
                provenance: prov(),
            },
            Op::AddAttackTree {
                role: Default::default(),
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
                role: Default::default(),
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

fn fragment_rows(
    view: &aegis_client::inspector::InspectorView,
) -> Vec<&aegis_client::inspector::InspectorRelationRow> {
    view.relations
        .iter()
        .chain(&view.reverse_relations)
        .filter(|r| r.key == RelationKey::FRAGMENT_USED_IN)
        .collect()
}

fn assert_fragment_user(ws: &Workspace, catalog: &Catalog, tree: TreeId, node: NodeId, a: TreeId) {
    let view = inspector_view(ws, Selection::TreeNode(tree, node), catalog).expect("user view");
    let rows = fragment_rows(&view);
    assert_eq!(
        rows.len(),
        1,
        "exactly one fragment row across both directions"
    );
    assert!(
        !view
            .relations
            .iter()
            .any(|r| r.key == RelationKey::FRAGMENT_USED_IN)
    );
    assert_eq!(rows[0].label, catalog.tr("inspector-rev-fragment-used-in"));
    assert!(!rows[0].editable);
    assert_eq!(
        rows[0].chips.iter().map(|c| c.target).collect::<Vec<_>>(),
        vec![Selection::Tree(a)]
    );
    let a_view = inspector_view(ws, Selection::Tree(a), catalog).expect("fragment");
    let note = view.shared_note.expect("fragment note");
    assert!(note.contains(&catalog.tr("fragment")));
    assert!(note.contains(&a_view.visual_id));
    assert!(note.contains("Fragment A"));
}

#[test]
#[allow(clippy::too_many_lines)]
fn loaded_fragments_have_one_read_only_relation_and_localized_notes() {
    assert_eq!(
        RELATIONS
            .iter()
            .filter(|r| r.key.as_str() == "fragment-used-in")
            .count(),
        1
    );
    assert!(!RELATIONS.iter().any(|r| r.key.as_str() == "step-fragment"));
    let registry = RELATIONS
        .iter()
        .find(|r| r.key == RelationKey::FRAGMENT_USED_IN)
        .expect("row");
    assert_eq!(registry.from, aegis_client::relations::ElementKind::Tree);
    assert_eq!(registry.to, &[aegis_client::relations::ElementKind::Step]);
    assert!(registry.many);
    assert!(registry.link.is_none());
    assert!(registry.unlink.is_none());

    for language in [Language::En, Language::Ja, Language::Pseudo] {
        let catalog = Catalog::load(language);
        let (mut ws, i) = fragments::loaded();
        let mut expected = [(i.first, i.user_a), (i.second, i.user_b), (i.b, i.user_c)];
        expected.sort();
        let expected: Vec<_> = expected
            .into_iter()
            .map(|(t, n)| Selection::TreeNode(t, n))
            .collect();
        let check_a = |ws: &Workspace, targets: &[Selection]| {
            let view = inspector_view(ws, Selection::Tree(i.a), &catalog).expect("A");
            let rows = fragment_rows(&view);
            assert_eq!(rows.len(), 1);
            assert!(
                !view
                    .reverse_relations
                    .iter()
                    .any(|r| r.key == RelationKey::FRAGMENT_USED_IN)
            );
            assert_eq!(rows[0].label, catalog.tr("inspector-used-in"));
            assert!(!rows[0].editable);
            assert_eq!(
                rows[0].chips.iter().map(|c| c.target).collect::<Vec<_>>(),
                targets
            );
        };
        check_a(&ws, &expected);
        for (tree, node) in [(i.first, i.user_a), (i.second, i.user_b), (i.b, i.user_c)] {
            assert_fragment_user(&ws, &catalog, tree, node, i.a);
        }
        for tree in [i.unused, i.first] {
            let view = inspector_view(&ws, Selection::Tree(tree), &catalog).expect("tree");
            assert!(fragment_rows(&view).iter().all(|r| r.chips.is_empty()));
        }
        let ordinary = inspector_view(&ws, Selection::TreeNode(i.first, i.ordinary), &catalog)
            .expect("ordinary");
        assert!(fragment_rows(&ordinary).is_empty());
        assert!(ordinary.shared_note.is_none());
        let shared =
            inspector_view(&ws, Selection::TreeNode(i.first, i.shared), &catalog).expect("shared");
        assert!(fragment_rows(&shared).is_empty());
        let note = shared.shared_note.as_ref().expect("shared note");
        assert_eq!(
            note,
            match language {
                Language::En => "Shared step L00, used in 1 tree",
                Language::Ja => "共通ステップ L00、1 件のツリーで使用",
                Language::Pseudo => "⟦Shâréd stép L00, ûséd în 1 tréé␣⟧",
            }
        );
        assert_eq!(
            shared
                .reverse_relations
                .iter()
                .find(|r| r.key == RelationKey::SHARED_STEP_USED_IN)
                .expect("shared reverse")
                .label,
            catalog.tr("inspector-rev-shared-step-used-in")
        );

        let ops = Edit::UnuseFragment {
            tree: i.first,
            node: i.user_a,
        }
        .to_ops(ws.project(), "inspector-editor", 900)
        .expect("unuse ops");
        assert_eq!(ops.len(), 1, "Unuse is detach, never Unshare");
        ws.apply(ops, Origin::Edit).expect("detach");
        let detached: Vec<_> = expected
            .iter()
            .copied()
            .filter(|s| *s != Selection::TreeNode(i.first, i.user_a))
            .collect();
        check_a(&ws, &detached);
        let check_detached = |ws: &Workspace| {
            let view = inspector_view(ws, Selection::TreeNode(i.first, i.user_a), &catalog)
                .expect("detached");
            assert!(fragment_rows(&view).is_empty());
            assert!(view.shared_note.is_none());
        };
        check_detached(&ws);
        ws.undo().expect("undo detach");
        check_a(&ws, &expected);
        assert_fragment_user(&ws, &catalog, i.first, i.user_a, i.a);
        ws.redo().expect("redo detach");
        check_a(&ws, &detached);
        check_detached(&ws);
        assert_eq!(
            shared,
            inspector_view(&ws, Selection::TreeNode(i.first, i.shared), &catalog)
                .expect("unchanged shared")
        );
    }
}

fn named_node(ws: &Workspace, tree_id: TreeId, label: &str) -> NodeId {
    let project = ws.project();
    let tree = project
        .attack_tree(project.attack_tree_idx(&tree_id).expect("tree index"))
        .expect("tree");
    tree.iter()
        .find_map(|(_, id, node)| (node.label() == label).then_some(id))
        .expect("named node")
}

fn root_node(ws: &Workspace, tree_id: TreeId) -> NodeId {
    let project = ws.project();
    let tree = project
        .attack_tree(project.attack_tree_idx(&tree_id).expect("tree index"))
        .expect("tree");
    tree.id_of(tree.root()).expect("root ID")
}

fn assert_reference_ratings(
    ws: &Workspace,
    tree: TreeId,
    node: NodeId,
    before: (&str, Option<Tone>),
    after: (&str, Option<Tone>),
) {
    let catalog = Catalog::load(Language::En);
    let view = inspector_view(ws, Selection::TreeNode(tree, node), &catalog).expect("reference");
    assert_eq!(view.kind, "Fragment reference");
    let fields: Vec<_> = view
        .fields
        .iter()
        .map(|field| (field.label.as_str(), field.value.as_str(), field.tone))
        .collect();
    assert_eq!(
        fields,
        [
            ("Before controls", before.0, before.1),
            ("After controls", after.0, after.1),
        ]
    );
}

fn explained_causes(ws: &Workspace, selection: Selection) -> Vec<(String, Selection, String)> {
    explain_selection(ws, selection, &Catalog::load(Language::En))
        .expect("explanation")
        .causes
        .into_iter()
        .map(|cause| (cause.role, cause.target, cause.label))
        .collect()
}

fn set_control_status(ws: &mut Workspace, control: ControlId, status: ControlStatus) {
    ws.apply(
        vec![Op::SetControlStatus {
            control,
            status,
            provenance: prov(),
        }],
        Origin::Edit,
    )
    .expect("control status");
}

#[test]
#[allow(clippy::too_many_lines)]
fn fragment_reference_ratings_and_causes_use_the_actual_mode_and_target_trees() {
    let (mut ws, i) = fragments::loaded();
    let catalog = Catalog::load(Language::En);
    let rated = named_node(&ws, i.a, "Rated fragment leaf");
    let (a_root, b_root, second_root) = (
        root_node(&ws, i.a),
        root_node(&ws, i.b),
        root_node(&ws, i.second),
    );
    let high = ws
        .project()
        .methodology()
        .feasibility_level("high")
        .expect("high");
    let very_low = ws
        .project()
        .methodology()
        .feasibility_level("very-low")
        .expect("very low");
    let nested_shared = SharedStepId::new_v7();
    ws.apply(
        vec![
            Op::SetStepFeasibility {
                target: StepTarget::Step(i.a, rated),
                level: None,
                provenance: prov(),
            },
            Op::AddSharedStep {
                id: nested_shared,
                name: "Nested shared leaf".into(),
                provenance: prov(),
            },
            Op::SetStepFeasibility {
                target: StepTarget::Shared(nested_shared),
                level: Some(high),
                provenance: prov(),
            },
            Op::UseSharedStep {
                tree: i.a,
                node: rated,
                step: nested_shared,
                provenance: prov(),
            },
            Op::SetStepMitigation {
                tree: i.second,
                node: i.user_b,
                control: i.control,
                feasibility: Some(very_low),
                provenance: prov(),
            },
            Op::SetStepFeasibility {
                target: StepTarget::Step(i.first, i.ordinary),
                level: Some(very_low),
                provenance: prov(),
            },
            Op::SetStepFeasibility {
                target: StepTarget::Shared(i.shared_id),
                level: Some(very_low),
                provenance: prov(),
            },
        ],
        Origin::Edit,
    )
    .expect("independent nested ratings");

    let project = ws.project();
    let first_idx = project.attack_tree_idx(&i.first).expect("first");
    let user_idx = project
        .attack_tree(first_idx)
        .expect("first tree")
        .idx_of(&i.user_a)
        .expect("reference index");
    assert_eq!(
        project.effective_step_feasibility(first_idx, user_idx),
        None,
        "references intentionally have no own-rating value"
    );
    assert_eq!(project.residual_step_feasibility(first_idx, user_idx), None);
    let a_idx = project.attack_tree_idx(&i.a).expect("fragment A");
    let rated_idx = project
        .attack_tree(a_idx)
        .expect("fragment A tree")
        .idx_of(&rated)
        .expect("nested shared index");
    let shared_idx = project
        .step_shared(a_idx, rated_idx)
        .expect("nested shared");
    assert_eq!(
        project.shared_step(shared_idx).expect("shared step").name(),
        "Nested shared leaf"
    );
    assert_reference_ratings(
        &ws,
        i.first,
        i.user_a,
        ("High", Some(Tone::High)),
        ("Low", Some(Tone::Low)),
    );
    let view = inspector_view(&ws, Selection::TreeNode(i.first, i.user_a), &catalog)
        .expect("planned reference");
    assert!(
        view.shared_note
            .expect("fragment note")
            .contains("Fragment A")
    );
    assert_eq!(view.fields.len(), 2, "reference ratings stay read-only");

    let direct = Selection::TreeNode(i.first, i.user_a);
    let nested = Selection::TreeNode(i.a, rated);
    assert_eq!(
        explained_causes(&ws, Selection::Tree(i.first)),
        [
            (
                "Fragment reference".to_owned(),
                direct,
                "First directly uses A".to_owned(),
            ),
            (
                "Capped by".to_owned(),
                Selection::Control(i.control),
                "Loaded mitigation".to_owned(),
            ),
            (
                "Deciding step".to_owned(),
                Selection::TreeNode(i.a, a_root),
                "Fragment A".to_owned(),
            ),
            (
                "Deciding step".to_owned(),
                nested,
                "Rated fragment leaf".to_owned(),
            ),
        ]
    );
    assert_eq!(primary_cause(&ws, Selection::Tree(i.first)), Some(direct));
    let selected_causes = explained_causes(&ws, direct);
    assert_eq!(selected_causes[0].1, Selection::Control(i.control));
    assert!(selected_causes.contains(&(
        "Deciding step".to_owned(),
        nested,
        "Rated fragment leaf".to_owned(),
    )));

    let nested_targets = [
        Selection::TreeNode(i.second, i.transitive),
        Selection::TreeNode(i.b, b_root),
        Selection::TreeNode(i.b, i.user_c),
        Selection::TreeNode(i.a, a_root),
        nested,
    ];
    let nested_labels = [
        "Uses B",
        "Fragment B",
        "B directly uses A",
        "Fragment A",
        "Rated fragment leaf",
    ];
    let second_causes = explained_causes(&ws, Selection::Tree(i.second));
    assert_eq!(
        second_causes
            .iter()
            .map(|cause| cause.1)
            .collect::<Vec<_>>(),
        nested_targets
    );
    assert_eq!(
        second_causes
            .iter()
            .map(|cause| cause.2.as_str())
            .collect::<Vec<_>>(),
        nested_labels
    );
    let gate_causes = explained_causes(&ws, Selection::TreeNode(i.second, second_root));
    assert_eq!(gate_causes[0].0, "Deciding child");
    assert_eq!(
        gate_causes.iter().map(|cause| cause.1).collect::<Vec<_>>(),
        nested_targets,
        "a planned residual-only ceiling must select the residual path"
    );

    let asset = AssetId::new_v7();
    let threat = ThreatId::new_v7();
    let category = ws
        .project()
        .methodology()
        .threat_categories()
        .next()
        .expect("threat category");
    let properties = ws.project().methodology().all_properties();
    ws.apply(
        vec![
            Op::AddAsset {
                id: asset,
                name: "Threat asset".into(),
                properties,
                provenance: prov(),
            },
            Op::AddThreatScenario {
                id: threat,
                name: "Nested threat".into(),
                asset,
                category,
                catalog_refs: Vec::new(),
                damages: Vec::new(),
                provenance: prov(),
            },
            Op::SetThreatAttackTree {
                threat,
                tree: Some(i.second),
                provenance: prov(),
            },
        ],
        Origin::Edit,
    )
    .expect("threat fixture");
    assert_eq!(
        explained_causes(&ws, Selection::Threat(threat))
            .iter()
            .map(|cause| cause.1)
            .collect::<Vec<_>>(),
        nested_targets
    );

    set_control_status(&mut ws, i.control, ControlStatus::Implemented);
    assert_reference_ratings(
        &ws,
        i.first,
        i.user_a,
        ("Low", Some(Tone::Low)),
        ("Low", Some(Tone::Low)),
    );
    assert!(
        explained_causes(&ws, Selection::Tree(i.first))
            .iter()
            .any(|cause| cause.1 == Selection::Control(i.control)),
        "implemented ceilings decide both modes even when Before equals After"
    );

    set_control_status(&mut ws, i.control, ControlStatus::Off);
    assert_reference_ratings(
        &ws,
        i.first,
        i.user_a,
        ("High", Some(Tone::High)),
        ("High", Some(Tone::High)),
    );
    assert!(
        !explained_causes(&ws, Selection::Tree(i.first))
            .iter()
            .any(|cause| cause.1 == Selection::Control(i.control)),
        "an off control cannot remain a deciding cause"
    );
}

#[test]
fn unrated_and_excluded_fragments_never_invent_reference_numbers() {
    let (mut unrated, i) = fragments::loaded();
    let rated = named_node(&unrated, i.a, "Rated fragment leaf");
    unrated
        .apply(
            vec![Op::SetStepFeasibility {
                target: StepTarget::Step(i.a, rated),
                level: None,
                provenance: prov(),
            }],
            Origin::Edit,
        )
        .expect("unrate fragment");
    set_control_status(&mut unrated, i.control, ControlStatus::Off);
    assert_reference_ratings(
        &unrated,
        i.first,
        i.user_a,
        ("not rated", None),
        ("not rated", None),
    );

    let (mut excluded, i) = fragments::loaded();
    set_control_status(&mut excluded, i.control, ControlStatus::Off);
    let condition = ConditionId::new_v7();
    let local_condition = ConditionId::new_v7();
    excluded
        .apply(
            vec![
                Op::AddCondition {
                    id: condition,
                    name: "Fragment unavailable".into(),
                    description: "".into(),
                    present: false,
                    provenance: prov(),
                },
                Op::NeedCondition {
                    element: ElementRef::Tree(i.a),
                    condition,
                    provenance: prov(),
                },
                Op::AddCondition {
                    id: local_condition,
                    name: "Reference unavailable".into(),
                    description: "".into(),
                    present: false,
                    provenance: prov(),
                },
                Op::NeedStepCondition {
                    tree: i.first,
                    node: i.user_a,
                    condition: local_condition,
                    provenance: prov(),
                },
            ],
            Origin::Edit,
        )
        .expect("exclude fragment");
    assert_reference_ratings(
        &excluded,
        i.first,
        i.user_a,
        ("not rated", None),
        ("not rated", None),
    );
    let causes = explained_causes(&excluded, Selection::TreeNode(i.first, i.user_a));
    assert!(causes.contains(&(
        "Excluded by".to_owned(),
        Selection::Condition(condition),
        "Fragment unavailable".to_owned(),
    )));
    assert!(causes.contains(&(
        "Excluded by".to_owned(),
        Selection::Condition(local_condition),
        "Reference unavailable".to_owned(),
    )));
    let catalog = Catalog::load(Language::En);
    let fragment = inspector_view(&excluded, Selection::Tree(i.a), &catalog).expect("fragment");
    assert!(fragment.excluded_reason.is_some());
    assert_eq!(
        fragment.exclusion_causes[0].target,
        Selection::Condition(condition)
    );
}
