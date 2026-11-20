//! Inspector core types, Fluent keys, explain-why causes, and `primary_cause`
//! resolution (WBX-06, WBX-11).

use std::sync::Arc;

use aegis_client::commands::Edit;
use aegis_client::i18n::{Catalog, Language};
use aegis_client::inspector::{
    exclusion_causes, explain_selection, primary_cause, reverse_relation_label,
};
use aegis_client::relations::{RELATIONS, RelationKey};
use aegis_client::view::Selection;
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{
    AssetId, AttackTree, ConditionId, ControlId, DamageId, ElementRef, GateKind, Methodology, Node,
    NodeId, Op, Provenance, ThreatId, TreeId,
};

const NEW_KEYS: &[&str] = &[
    "cmd-go-to-cause",
    "cmd-go-to-cause-tip",
    "inspector-details",
    "inspector-explain-why",
    "inspector-used-by",
    "inspector-used-in",
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
