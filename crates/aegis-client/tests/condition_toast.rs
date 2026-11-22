//! Integration tests for the condition-switch undo toast
//! (`aegis_client::exclusion::ConditionSnapshot`, 135b).

use std::sync::Arc;

use aegis_client::exclusion::ConditionSnapshot;
use aegis_client::pages;
use aegis_client::view::Selection;
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{
    AssetId, AttackTree, ConditionId, DamageId, GateKind, Methodology, Node, NodeId, Op,
    Provenance, ThreatId, TreeId,
};

fn prov() -> Provenance {
    Provenance::human("tester", 1_792_956_600_000)
}

fn add_threat_with_single_cond_step(
    ws: &mut Workspace,
    asset: AssetId,
    dmg: DamageId,
    cond: ConditionId,
    name: &str,
) -> ThreatId {
    let m = ws.project().methodology();
    let spoofing = m.threat_category("spoofing").expect("spoofing");
    let high = m.feasibility_level("high").expect("high");

    let tree_id = TreeId::new_v7();
    let root = NodeId::new_v7();
    let step = NodeId::new_v7();
    let mut tree = AttackTree::new(root, Node::gate(GateKind::Or, name)).expect("root");
    tree.add_child(tree.root(), step, Node::step("Debug access"))
        .expect("step");

    let tid = ThreatId::new_v7();
    ws.apply(
        vec![
            Op::AddAttackTree {
                id: tree_id,
                tree,
                step_feasibility: vec![(step, high)],
                provenance: prov(),
            },
            Op::NeedStepCondition {
                tree: tree_id,
                node: step,
                condition: cond,
                provenance: prov(),
            },
            Op::AddThreatScenario {
                id: tid,
                name: name.into(),
                asset,
                category: spoofing,
                catalog_refs: Vec::new(),
                damages: vec![dmg],
                provenance: prov(),
            },
            Op::SetThreatAttackTree {
                threat: tid,
                tree: Some(tree_id),
                provenance: prov(),
            },
        ],
        Origin::Edit,
    )
    .expect("add threat with single cond step");
    tid
}

#[test]
fn condition_switch_toast_reports_step_counts_and_moved_risks() {
    let m = Arc::new(Methodology::iso21434_r155().expect("methodology"));
    let integrity = m.property("integrity").expect("integrity");
    let authenticity = m.property("authenticity").expect("authenticity");
    let safety = m
        .impact_category("safety")
        .or_else(|| m.impact_category("safety-health"))
        .expect("safety");
    let severe = m.level(safety, "severe").expect("severe");
    let spoofing = m.threat_category("spoofing").expect("spoofing");
    let high = m.feasibility_level("high").expect("high");

    let mut ws = Workspace::empty(Arc::clone(&m));

    let cf_debug = ConditionId::new_v7();
    let cf_unused = ConditionId::new_v7();
    let asset = AssetId::new_v7();
    let dmg = DamageId::new_v7();

    // T00: two-step OR tree; one step needs CF00, the other stays applicable at the same
    // feasibility, so switching CF00 off excludes 1 step here without moving T00's risk.
    let tree0_id = TreeId::new_v7();
    let root0 = NodeId::new_v7();
    let step0_always = NodeId::new_v7();
    let step0_cond = NodeId::new_v7();
    let mut tree0 = AttackTree::new(root0, Node::gate(GateKind::Or, "Tamper")).expect("tree0");
    tree0
        .add_child(tree0.root(), step0_always, Node::step("Bus injection"))
        .expect("step0_always");
    tree0
        .add_child(tree0.root(), step0_cond, Node::step("JTAG flash"))
        .expect("step0_cond");
    let t0 = ThreatId::new_v7();

    ws.apply(
        vec![
            Op::AddCondition {
                id: cf_debug,
                name: "Debug port".into(),
                description: "".into(),
                present: true,
                provenance: prov(),
            },
            Op::AddCondition {
                id: cf_unused,
                name: "Spare option".into(),
                description: "".into(),
                present: true,
                provenance: prov(),
            },
            Op::AddAsset {
                id: asset,
                name: "ECU firmware".into(),
                properties: [integrity, authenticity].into_iter().collect(),
                provenance: prov(),
            },
            Op::AddDamageScenario {
                id: dmg,
                name: "Braking loss".into(),
                impacts: vec![(safety, severe)],
                provenance: prov(),
            },
            Op::AddAttackTree {
                id: tree0_id,
                tree: tree0,
                step_feasibility: vec![(step0_always, high), (step0_cond, high)],
                provenance: prov(),
            },
            Op::NeedStepCondition {
                tree: tree0_id,
                node: step0_cond,
                condition: cf_debug,
                provenance: prov(),
            },
            Op::AddThreatScenario {
                id: t0,
                name: "Tamper".into(),
                asset,
                category: spoofing,
                catalog_refs: Vec::new(),
                damages: vec![dmg],
                provenance: prov(),
            },
            Op::SetThreatAttackTree {
                threat: t0,
                tree: Some(tree0_id),
                provenance: prov(),
            },
        ],
        Origin::Edit,
    )
    .expect("setup base workspace");

    // T01: single-step tree needing CF00.
    let t1 = add_threat_with_single_cond_step(&mut ws, asset, dmg, cf_debug, "Spoof");

    let expected_risk_level = ws
        .cache()
        .evaluate_residual_risk(ws.project(), t1)
        .expect("t1 risk before")
        .max_risk()
        .expect("t1 max risk");
    let risk_label = ws
        .project()
        .methodology()
        .risk_level_label(expected_risk_level)
        .expect("risk level label")
        .to_owned();

    let cf_vis = pages::selection_visual_id(ws.project(), Selection::Condition(cf_debug));
    let t1_vis = pages::selection_visual_id(ws.project(), Selection::Threat(t1));

    // Non-condition ops return None from capture.
    assert!(
        ConditionSnapshot::capture(
            &ws,
            &[Op::SetThreatFeasibility {
                threat: t0,
                level: Some(high),
                provenance: prov(),
            }]
        )
        .is_none()
    );

    // 1. Switch CF00 off: 2 steps excluded, 0 included, 1 risk moved.
    let ops_off = vec![Op::SetConditionPresent {
        id: cf_debug,
        present: false,
        provenance: prov(),
    }];
    let snap_off = ConditionSnapshot::capture(&ws, &ops_off).expect("capture off");
    ws.apply(ops_off, Origin::Edit).expect("apply off");
    assert_eq!(
        snap_off.toast(&ws),
        format!(
            "Condition '{cf_vis} Debug port' switched off: 2 steps excluded, 0 included, 1 risk moved ({t1_vis} 'Spoof' {risk_label} \u{2192} not applicable)"
        )
    );

    // 2. Switch CF00 back on: 0 steps excluded, 2 included, 1 risk moved.
    let ops_on = vec![Op::SetConditionPresent {
        id: cf_debug,
        present: true,
        provenance: prov(),
    }];
    let snap_on = ConditionSnapshot::capture(&ws, &ops_on).expect("capture on");
    ws.apply(ops_on, Origin::Edit).expect("apply on");
    assert_eq!(
        snap_on.toast(&ws),
        format!(
            "Condition '{cf_vis} Debug port' switched on: 0 steps excluded, 2 included, 1 risk moved ({t1_vis} 'Spoof' not applicable \u{2192} {risk_label})"
        )
    );

    // 3. Switch an unused condition off: 0 steps excluded, 0 included, no risk moved.
    let unused_vis = pages::selection_visual_id(ws.project(), Selection::Condition(cf_unused));
    let ops_noop = vec![Op::SetConditionPresent {
        id: cf_unused,
        present: false,
        provenance: prov(),
    }];
    let snap_noop = ConditionSnapshot::capture(&ws, &ops_noop).expect("capture noop");
    ws.apply(ops_noop, Origin::Edit).expect("apply noop");
    assert_eq!(
        snap_noop.toast(&ws),
        format!(
            "Condition '{unused_vis} Spare option' switched off: 0 steps excluded, 0 included, no risk moved"
        )
    );

    // 4. Five moved risks: names the first three in visual-ID order and ends with `, and 2 more)`.
    let t2 = add_threat_with_single_cond_step(&mut ws, asset, dmg, cf_debug, "Replay");
    let t3 = add_threat_with_single_cond_step(&mut ws, asset, dmg, cf_debug, "Flood");
    let _t4 = add_threat_with_single_cond_step(&mut ws, asset, dmg, cf_debug, "Dump");
    let _t5 = add_threat_with_single_cond_step(&mut ws, asset, dmg, cf_debug, "Erase");

    let t2_vis = pages::selection_visual_id(ws.project(), Selection::Threat(t2));
    let t3_vis = pages::selection_visual_id(ws.project(), Selection::Threat(t3));

    let ops_five = vec![Op::SetConditionPresent {
        id: cf_debug,
        present: false,
        provenance: prov(),
    }];
    let snap_five = ConditionSnapshot::capture(&ws, &ops_five).expect("capture five");
    ws.apply(ops_five, Origin::Edit).expect("apply five");
    assert_eq!(
        snap_five.toast(&ws),
        format!(
            "Condition '{cf_vis} Debug port' switched off: 6 steps excluded, 0 included, 5 risks moved ({t1_vis} 'Spoof' {risk_label} \u{2192} not applicable, {t2_vis} 'Replay' {risk_label} \u{2192} not applicable, {t3_vis} 'Flood' {risk_label} \u{2192} not applicable, and 2 more)"
        )
    );
}
