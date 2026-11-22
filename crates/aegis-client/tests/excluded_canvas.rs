//! Integration tests for excluded attack tree nodes on the canvas (135c).

use std::sync::Arc;

use aegis_canvas::access::{describe, sentence};
use aegis_canvas::{Item, Paint, Point, Rect, RenderOptions, Scene, Size, Viewport, render};
use aegis_client::view::tree_scene;
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{
    AssetId, AttackTree, ConditionId, DamageId, GateKind, Methodology, Node, NodeId, Op,
    Provenance, ThreatId, TreeId,
};

fn prov() -> Provenance {
    Provenance::human("tester", 1_792_956_600_000)
}

fn node_pos_by_label(scene: &Scene, label: &str) -> u32 {
    (0..scene.len())
        .map(|i| u32::try_from(i).expect("scene len fits u32"))
        .find(|&v| scene.label(v) == label)
        .unwrap_or_else(|| panic!("node with label '{label}' not found"))
}

fn screen_rect(scene: &Scene, view: &Viewport, v: u32) -> Rect {
    let world = scene
        .node_rect(scene.node_idx(v).expect("idx"))
        .expect("rect");
    let tl = view.to_screen(world.origin);
    Rect::new(
        tl.x,
        tl.y,
        world.size.w * view.zoom,
        world.size.h * view.zoom,
    )
}

fn in_rect(p: Point, r: Rect) -> bool {
    let eps = 1e-2;
    p.x >= r.origin.x - eps
        && p.x <= r.right() + eps
        && p.y >= r.origin.y - eps
        && p.y <= r.bottom() + eps
}

fn hatch_count(items: &[Item], rect: Rect) -> usize {
    items
        .iter()
        .filter(|it| match it {
            Item::Polyline { points, .. } if points.len() == 2 => {
                in_rect(points[0], rect) && in_rect(points[1], rect)
            }
            _ => false,
        })
        .count()
}

#[test]
fn excluded_steps_and_fully_excluded_trees_hatch_and_speak_reasons() {
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
    let high_label = m.feasibility_level_label(high).expect("high label");

    let mut ws = Workspace::empty(Arc::clone(&m));

    let cf_eth = ConditionId::new_v7();
    let asset = AssetId::new_v7();
    let dmg = DamageId::new_v7();

    // Tree 1: OR root with step A (needs CF00) and step B (included, rated high).
    let tree1_id = TreeId::new_v7();
    let root1 = NodeId::new_v7();
    let step_a = NodeId::new_v7();
    let step_b = NodeId::new_v7();
    let mut tree1 =
        AttackTree::new(root1, Node::gate(GateKind::Or, "Compromise ECU")).expect("tree1");
    tree1
        .add_child(tree1.root(), step_a, Node::step("Attack over Ethernet"))
        .expect("step A");
    tree1
        .add_child(tree1.root(), step_b, Node::step("Attack over CAN"))
        .expect("step B");

    // Tree 2: single step needs CF00, so root has AllPathsExcluded and
    // evaluate_tree returns TreeNotApplicable.
    let tree2_id = TreeId::new_v7();
    let root2 = NodeId::new_v7();
    let step_only = NodeId::new_v7();
    let mut tree2 =
        AttackTree::new(root2, Node::gate(GateKind::Or, "Ethernet-only attack")).expect("tree2");
    tree2
        .add_child(tree2.root(), step_only, Node::step("Spoof DoIP frames"))
        .expect("step_only");

    let tid1 = ThreatId::new_v7();
    let tid2 = ThreatId::new_v7();

    ws.apply(
        vec![
            Op::AddCondition {
                id: cf_eth,
                name: "Ethernet".into(),
                description: "".into(),
                present: false,
                provenance: prov(),
            },
            Op::AddAsset {
                id: asset,
                name: "Gateway".into(),
                properties: [integrity, authenticity].into_iter().collect(),
                provenance: prov(),
            },
            Op::AddDamageScenario {
                id: dmg,
                name: "Loss of gateway integrity".into(),
                impacts: vec![(safety, severe)],
                provenance: prov(),
            },
            Op::AddAttackTree {
                id: tree1_id,
                tree: tree1,
                step_feasibility: vec![(step_a, high), (step_b, high)],
                provenance: prov(),
            },
            Op::NeedStepCondition {
                tree: tree1_id,
                node: step_a,
                condition: cf_eth,
                provenance: prov(),
            },
            Op::AddThreatScenario {
                id: tid1,
                name: "Gateway compromise".into(),
                asset,
                category: spoofing,
                catalog_refs: Vec::new(),
                damages: vec![dmg],
                provenance: prov(),
            },
            Op::SetThreatAttackTree {
                threat: tid1,
                tree: Some(tree1_id),
                provenance: prov(),
            },
            Op::AddAttackTree {
                id: tree2_id,
                tree: tree2,
                step_feasibility: vec![(step_only, high)],
                provenance: prov(),
            },
            Op::NeedStepCondition {
                tree: tree2_id,
                node: step_only,
                condition: cf_eth,
                provenance: prov(),
            },
            Op::AddThreatScenario {
                id: tid2,
                name: "DoIP spoofing".into(),
                asset,
                category: spoofing,
                catalog_refs: Vec::new(),
                damages: vec![dmg],
                provenance: prov(),
            },
            Op::SetThreatAttackTree {
                threat: tid2,
                tree: Some(tree2_id),
                provenance: prov(),
            },
        ],
        Origin::Edit,
    )
    .expect("apply setup ops");

    // Tree 1: partial exclusion.
    let scene1 = tree_scene(&ws, tree1_id, false).expect("scene1");
    let view = Viewport::new(Size::new(1600.0, 700.0));
    let list1 = render(&scene1, &view, &RenderOptions::default());

    let pos_a = node_pos_by_label(&scene1, "Attack over Ethernet");
    let pos_b = node_pos_by_label(&scene1, "Attack over CAN");
    let rect_a = screen_rect(&scene1, &view, pos_a);
    let rect_b = screen_rect(&scene1, &view, pos_b);

    assert!(
        hatch_count(&list1.items, rect_a) >= 2,
        "excluded step A must have hatch segments"
    );
    assert!(
        list1.items.iter().any(|it| matches!(
            it,
            Item::Text {
                at,
                text,
                paint: Paint::Graphite,
                bold: true,
                ..
            } if text == "excluded" && in_rect(*at, rect_a)
        )),
        "excluded step A must render 'excluded' text"
    );
    let access_a = describe(&scene1, pos_a).expect("access A");
    let spoken_a = sentence(&access_a, false);
    assert!(
        spoken_a.contains(" Excluded: Needs CF00 Ethernet (off)."),
        "{spoken_a}"
    );
    assert!(
        !spoken_a.contains("Rating "),
        "excluded step should not repeat a rating sentence: {spoken_a}"
    );

    assert_eq!(
        hatch_count(&list1.items, rect_b),
        0,
        "included step B must not have hatch segments"
    );
    assert!(
        list1.items.iter().any(|it| matches!(
            it,
            Item::Text { at, text, .. } if text.contains(high_label) && in_rect(*at, rect_b)
        )),
        "included step B must keep its rating label '{high_label}'"
    );

    // Tree 2: fully excluded tree (evaluate_tree returns TreeNotApplicable).
    assert!(
        ws.cache().evaluate_tree(ws.project(), tree2_id).is_err(),
        "tree2 should fail evaluation with TreeNotApplicable"
    );
    let scene2 = tree_scene(&ws, tree2_id, false).expect("scene2");
    let list2 = render(&scene2, &view, &RenderOptions::default());

    let pos_root2 = node_pos_by_label(&scene2, "Ethernet-only attack");
    let rect_root2 = screen_rect(&scene2, &view, pos_root2);
    assert!(
        hatch_count(&list2.items, rect_root2) >= 2,
        "fully excluded tree root must have hatch segments"
    );
    let access_root2 = describe(&scene2, pos_root2).expect("access root2");
    let spoken_root2 = sentence(&access_root2, false);
    assert!(
        spoken_root2.contains(" Excluded: Every attack path is excluded."),
        "{spoken_root2}"
    );
}
