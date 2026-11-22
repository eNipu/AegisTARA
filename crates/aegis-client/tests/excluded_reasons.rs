//! Integration tests for human-readable exclusion reasons in rows and inspector
//! banners (`aegis_client::exclusion::reasons_text`, 135b).

use std::sync::Arc;

use aegis_client::exclusion::reasons_text;
use aegis_client::i18n::{Catalog, Language};
use aegis_client::inspector::inspector_view;
use aegis_client::pages;
use aegis_client::table::{selection_exclusion, table_rows};
use aegis_client::view::{Selection, TableId};
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{
    AssetId, AttackTree, ComponentId, ConditionId, DamageId, ElementRef, GateKind, InterfaceId,
    ItemRef, Methodology, Node, NodeId, Op, Provenance, Reason, ThreatId, TreeId,
};

fn prov() -> Provenance {
    Provenance::human("tester", 1_792_956_600_000)
}

#[test]
fn exclusion_reasons_match_expected_phrases_and_agree_across_table_and_inspector() {
    let m = Arc::new(Methodology::iso21434_r155().expect("methodology"));
    let integrity = m.property("integrity").expect("integrity");
    let authenticity = m.property("authenticity").expect("authenticity");
    let safety = m
        .impact_category("safety")
        .or_else(|| m.impact_category("safety-health"))
        .expect("safety");
    let severe = m.level(safety, "severe").expect("severe");
    let spoofing = m.threat_category("spoofing").expect("spoofing");
    let iface_kind = m.interface_kinds().next().expect("interface kind");

    let mut ws = Workspace::empty(Arc::clone(&m));

    let cf_eth = ConditionId::new_v7();
    let comp = ComponentId::new_v7();
    let iface = InterfaceId::new_v7();
    let asset_bound = AssetId::new_v7();
    let asset_free = AssetId::new_v7();
    let dmg = DamageId::new_v7();
    let threat_via = ThreatId::new_v7();
    let threat_all = ThreatId::new_v7();

    let tree1_id = TreeId::new_v7();
    let root1 = NodeId::new_v7();
    let step_via = NodeId::new_v7();
    let step_two = NodeId::new_v7();
    let mut tree1 =
        AttackTree::new(root1, Node::gate(GateKind::Or, "Compromise ECU")).expect("tree1 root");
    tree1
        .add_child(tree1.root(), step_via, Node::step("Inject Ethernet frame"))
        .expect("step_via");
    tree1
        .add_child(
            tree1.root(),
            step_two,
            Node::step("Exploit Ethernet diagnostics"),
        )
        .expect("step_two");

    let tree2_id = TreeId::new_v7();
    let root2 = NodeId::new_v7();
    let step_only = NodeId::new_v7();
    let mut tree2 =
        AttackTree::new(root2, Node::gate(GateKind::Or, "Only path")).expect("tree2 root");
    tree2
        .add_child(tree2.root(), step_only, Node::step("Use Ethernet port"))
        .expect("step_only");

    ws.apply(
        vec![
            Op::AddCondition {
                id: cf_eth,
                name: "Ethernet".into(),
                description: "".into(),
                present: false,
                provenance: prov(),
            },
            Op::AddComponent {
                id: comp,
                name: "Gateway".into(),
                parent: None,
                provenance: prov(),
            },
            Op::NeedCondition {
                element: ElementRef::Component(comp),
                condition: cf_eth,
                provenance: prov(),
            },
            Op::AddInterface {
                id: iface,
                name: "ETH0".into(),
                component: comp,
                kind: iface_kind,
                provenance: prov(),
            },
            Op::AddAsset {
                id: asset_bound,
                name: "Brake ECU".into(),
                properties: [integrity, authenticity].into_iter().collect(),
                provenance: prov(),
            },
            Op::SetAssetComponent {
                asset: asset_bound,
                component: Some(comp),
                provenance: prov(),
            },
            Op::AddAsset {
                id: asset_free,
                name: "Telematics key".into(),
                properties: [integrity, authenticity].into_iter().collect(),
                provenance: prov(),
            },
            Op::AddDamageScenario {
                id: dmg,
                name: "Loss of control".into(),
                impacts: vec![(safety, severe)],
                provenance: prov(),
            },
            Op::AddAttackTree {
                id: tree1_id,
                tree: tree1,
                step_feasibility: Vec::new(),
                provenance: prov(),
            },
            Op::SetStepVia {
                tree: tree1_id,
                node: step_via,
                via: Some(ItemRef::Interface(iface)),
                provenance: prov(),
            },
            Op::SetStepVia {
                tree: tree1_id,
                node: step_two,
                via: Some(ItemRef::Interface(iface)),
                provenance: prov(),
            },
            Op::NeedStepCondition {
                tree: tree1_id,
                node: step_two,
                condition: cf_eth,
                provenance: prov(),
            },
            Op::AddThreatScenario {
                id: threat_via,
                name: "Spoof brake command".into(),
                asset: asset_bound,
                category: spoofing,
                catalog_refs: Vec::new(),
                damages: vec![dmg],
                provenance: prov(),
            },
            Op::SetThreatAttackTree {
                threat: threat_via,
                tree: Some(tree1_id),
                provenance: prov(),
            },
            Op::AddAttackTree {
                id: tree2_id,
                tree: tree2,
                step_feasibility: Vec::new(),
                provenance: prov(),
            },
            Op::NeedStepCondition {
                tree: tree2_id,
                node: step_only,
                condition: cf_eth,
                provenance: prov(),
            },
            Op::AddThreatScenario {
                id: threat_all,
                name: "Read key over Ethernet".into(),
                asset: asset_free,
                category: spoofing,
                catalog_refs: Vec::new(),
                damages: vec![dmg],
                provenance: prov(),
            },
            Op::SetThreatAttackTree {
                threat: threat_all,
                tree: Some(tree2_id),
                provenance: prov(),
            },
        ],
        Origin::Edit,
    )
    .expect("setup project");

    let project = ws.project();
    let cf_vis = pages::selection_visual_id(project, Selection::Condition(cf_eth));
    let comp_vis = pages::selection_visual_id(project, Selection::Component(comp));
    let iface_vis = pages::selection_visual_id(project, Selection::Interface(iface));
    assert_eq!(cf_vis, "CF00");
    assert!(!comp_vis.is_empty());
    assert!(!iface_vis.is_empty());

    // 1. Component directly needing an off condition.
    assert_eq!(
        selection_exclusion(project, Selection::Component(comp)),
        (true, Some("Needs CF00 Ethernet (off)".to_owned()))
    );

    // 2. Interface excluded because its owner component is excluded.
    assert_eq!(
        selection_exclusion(project, Selection::Interface(iface)),
        (true, Some(format!("Owner {comp_vis} Gateway is excluded")))
    );

    // 3. Step going via the excluded interface.
    let expected_via = format!("Via {iface_vis} ETH0 is excluded");
    assert_eq!(
        selection_exclusion(project, Selection::TreeNode(tree1_id, step_via)),
        (true, Some(expected_via.clone()))
    );

    // 4. Root of the second tree: all paths excluded.
    assert_eq!(
        selection_exclusion(project, Selection::TreeNode(tree2_id, root2)),
        (true, Some("Every attack path is excluded".to_owned()))
    );

    // 5. Node with two reasons joined by "; ".
    assert_eq!(
        selection_exclusion(project, Selection::TreeNode(tree1_id, step_two)),
        (
            true,
            Some(format!("Needs CF00 Ethernet (off); {expected_via}"))
        )
    );

    // 6. Fallback phrases when target IDs are unknown / unresolvable.
    assert_eq!(
        reasons_text(project, &[Reason::ConditionOff(ConditionId::new_v7())]),
        "Needs a condition (off)"
    );
    assert_eq!(
        reasons_text(
            project,
            &[Reason::OwnerExcluded(ElementRef::Component(
                ComponentId::new_v7()
            ))]
        ),
        "Owner is excluded"
    );
    assert_eq!(
        reasons_text(
            project,
            &[Reason::ViaExcluded(ItemRef::Interface(
                InterfaceId::new_v7()
            ))]
        ),
        "Via is excluded"
    );

    // 7. Table rows and inspector banner return the exact same text for the same selection.
    let catalog = Catalog::load(Language::En);
    let vocab = pages::vocabulary(project.methodology());

    let checks = [
        (TableId::Assets, Selection::Asset(asset_bound)),
        (TableId::Threats, Selection::Threat(threat_via)),
        (TableId::Risks, Selection::Threat(threat_via)),
    ];
    for (table_id, sel) in checks {
        let rows = table_rows(&ws, table_id, &vocab, &catalog);
        let row = rows
            .iter()
            .find(|r| r.target == Some(sel))
            .expect("row present");
        let view = inspector_view(&ws, sel, &catalog).expect("inspector view");
        assert!(row.excluded, "{sel:?} must be excluded in table");
        assert!(row.excluded_reason.is_some());
        assert_eq!(
            row.excluded_reason, view.excluded_reason,
            "table and inspector must show the exact same exclusion text for {sel:?}"
        );
    }
}
