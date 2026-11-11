//! UI-10.6: every `Op` kind, applied through the workspace as one user edit,
//! is undone to exactly the previous project (links, ratings, descriptions,
//! tree nodes), and redone to exactly the project after it.
//!
//! Three walks cover all kinds. The first replays the ops of the model's own
//! "every op" walk-through over the second methodology (`second_every_op`,
//! 37 kinds: vocabulary, threats, trees, controls, goals); the second walk
//! below covers the next 28 (item definition, tree nodes, descriptions,
//! review marks); the third covers the generic element ops. A new `Op`
//! variant fails this test until a walk covers it.

#[path = "../../aegis-model/tests/common/mod.rs"]
mod common;

use std::collections::BTreeSet;

use std::sync::Arc;

use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{
    AssetId, AttackTree, AttrValue, ComponentId, ControlId, DataFlowId, ElementId, ElementKind,
    ElementRef, GateKind, InterfaceId, Methodology, Node, NodeId, Op, Project, Provenance, TreeId,
    TrustBoundaryId,
};

/// The variant names declared in `op.rs`: lines like `    AddAsset {`.
fn declared_variants() -> BTreeSet<String> {
    let src = include_str!("../../aegis-model/src/op.rs");
    let body = src.split("pub enum Op {").nth(1).expect("the Op enum");
    body.lines()
        .take_while(|l| *l != "}")
        .filter(|l| l.starts_with("    ") && !l.starts_with("     "))
        .filter_map(|l| l.trim().strip_suffix(" {"))
        .filter(|n| n.chars().all(char::is_alphanumeric))
        .map(str::to_owned)
        .collect()
}

/// Applies `op` as one user edit, then checks undo restores the project
/// exactly and redo repeats the edit exactly.
fn apply_undo_redo(ws: &mut Workspace, seen: &mut BTreeSet<String>, op: Op) {
    let kind = common::variant(&op);
    let before = ws.project().clone();
    ws.apply(vec![op], Origin::Edit)
        .unwrap_or_else(|e| panic!("{kind} failed: {e}"));
    let after = ws.project().clone();
    ws.undo().expect("undo");
    assert!(ws.project() == &before, "undo of {kind} did not restore");
    ws.redo().expect("redo");
    assert!(ws.project() == &after, "redo of {kind} did not repeat it");
    seen.insert(kind);
}

fn who() -> Provenance {
    Provenance::human("alice", 1)
}

/// The 28 kinds the model's walk-through does not reach, built against a
/// valid starting state and torn down again.
fn walk_item_definition_trees_and_descriptions(seen: &mut BTreeSet<String>) {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let can = m.interface_kind("can").expect("can");
    let eth = m.interface_kind("ethernet").expect("ethernet");
    let mut ws = Workspace::empty(Arc::clone(&m));
    let (asset, c1, c2, i1, df, tb) = (
        AssetId::new_v7(),
        ComponentId::new_v7(),
        ComponentId::new_v7(),
        InterfaceId::new_v7(),
        DataFlowId::new_v7(),
        TrustBoundaryId::new_v7(),
    );
    let (tree, root, s1, g2, s2) = (
        TreeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
    );
    let name = |n: &str| -> Arc<str> { n.into() };
    let text = |t: &str| -> Arc<str> { t.into() };
    let mut go = |op: Op| apply_undo_redo(&mut ws, seen, op);

    go(Op::AddAsset {
        id: asset,
        name: name("ECU"),
        properties: m.all_properties(),
        provenance: who(),
    });
    // Item definition: components, interfaces, flows, trust boundaries.
    go(Op::AddComponent {
        id: c1,
        name: name("Gateway"),
        parent: None,
        provenance: who(),
    });
    go(Op::AddComponent {
        id: c2,
        name: name("Brake"),
        parent: Some(c1),
        provenance: who(),
    });
    go(Op::RenameComponent {
        id: c2,
        name: name("Brake ECU"),
        provenance: who(),
    });
    go(Op::SetComponentParent {
        id: c2,
        parent: None,
        provenance: who(),
    });
    go(Op::SetAssetComponent {
        asset,
        component: Some(c1),
        provenance: who(),
    });
    go(Op::AddInterface {
        id: i1,
        name: name("CAN"),
        component: c1,
        kind: can,
        provenance: who(),
    });
    go(Op::SetInterfaceKind {
        id: i1,
        kind: eth,
        provenance: who(),
    });
    go(Op::RenameInterface {
        id: i1,
        name: name("Ethernet"),
        provenance: who(),
    });
    go(Op::AddDataFlow {
        id: df,
        name: name("Frame"),
        from: c1,
        to: c2,
        interface: Some(i1),
        provenance: who(),
    });
    go(Op::RenameDataFlow {
        id: df,
        name: name("Torque frame"),
        provenance: who(),
    });
    go(Op::SetDataFlowInterface {
        id: df,
        interface: None,
        provenance: who(),
    });
    go(Op::AddTrustBoundary {
        id: tb,
        name: name("Safety"),
        components: vec![c1],
        provenance: who(),
    });
    go(Op::RenameTrustBoundary {
        id: tb,
        name: name("Safety domain"),
        provenance: who(),
    });
    go(Op::AddBoundaryComponent {
        boundary: tb,
        component: c2,
        provenance: who(),
    });
    go(Op::RemoveBoundaryComponent {
        boundary: tb,
        component: c2,
        provenance: who(),
    });
    // Descriptions: set, translate, clear (a described element cannot go).
    go(Op::SetDescription {
        target: ElementRef::Component(c1),
        text: text("Central gateway"),
        provenance: who(),
    });
    go(Op::SetDescriptionTranslation {
        target: ElementRef::Component(c1),
        lang: text("ja"),
        text: text("中央ゲートウェイ"),
        provenance: who(),
    });
    go(Op::SetProjectLanguages {
        langs: Arc::from([text("ja")]),
        provenance: who(),
    });
    go(Op::SetDescriptionTranslation {
        target: ElementRef::Component(c1),
        lang: text("ja"),
        text: text(""),
        provenance: who(),
    });
    go(Op::SetDescription {
        target: ElementRef::Component(c1),
        text: text(""),
        provenance: who(),
    });
    let ctrl = ControlId::new_v7();
    let at = m.attack_types().next().expect("attack type");
    let low = m.feasibility_level("low").expect("low");
    go(Op::AddSecurityControl {
        id: ctrl,
        name: name("JTAG disabled"),
        provenance: who(),
    });
    go(Op::SetControlRule {
        control: ctrl,
        attack_type: at,
        ceiling: Some(low),
        provenance: who(),
    });
    go(Op::SetControlRule {
        control: ctrl,
        attack_type: at,
        ceiling: None,
        provenance: who(),
    });
    go(Op::RemoveSecurityControl { id: ctrl });

    // Attack tree nodes.
    let t = AttackTree::new(root, Node::gate(GateKind::Or, "Goal")).expect("tree");
    go(Op::AddAttackTree {
        id: tree,
        tree: t,
        step_feasibility: vec![],
        provenance: who(),
    });
    go(Op::AddTreeNode {
        tree,
        parent: root,
        id: s1,
        node: Node::step("Step"),
        index: 1,
        position: 0,
        provenance: who(),
    });
    go(Op::AddTreeNode {
        tree,
        parent: root,
        id: g2,
        node: Node::gate(GateKind::And, "Gate"),
        index: 2,
        position: 1,
        provenance: who(),
    });
    go(Op::AddTreeNode {
        tree,
        parent: root,
        id: s2,
        node: Node::step("Other"),
        index: 3,
        position: 2,
        provenance: who(),
    });
    go(Op::RenameTreeNode {
        tree,
        id: s1,
        label: name("Renamed step"),
        provenance: who(),
    });
    go(Op::SetTreeNodeGate {
        tree,
        id: g2,
        gate: GateKind::Or,
        provenance: who(),
    });
    go(Op::SetReviewMark {
        tree,
        node: s1,
        marked: true,
        provenance: who(),
    });
    go(Op::SetReviewMark {
        tree,
        node: s1,
        marked: false,
        provenance: who(),
    });
    go(Op::LinkTreeNode {
        tree,
        parent: g2,
        child: s1,
        position: 0,
        slot: 0,
        provenance: who(),
    });
    go(Op::UnlinkTreeNode {
        tree,
        parent: g2,
        child: s1,
        provenance: who(),
    });
    go(Op::RemoveTreeNode {
        tree,
        id: s2,
        provenance: who(),
    });

    // Tear down what the item definition built.
    go(Op::RemoveDataFlow { id: df });
    go(Op::RemoveInterface { id: i1 });
    go(Op::RemoveTrustBoundary { id: tb });
    go(Op::SetAssetComponent {
        asset,
        component: None,
        provenance: who(),
    });
    go(Op::RemoveComponent { id: c2 });
    go(Op::RemoveComponent { id: c1 });
}

/// The generic element ops, over an empty workspace.
fn walk_elements(seen: &mut BTreeSet<String>) {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let mut ws = Workspace::empty(m);
    let id = ElementId::new_v7();
    apply_undo_redo(
        &mut ws,
        seen,
        Op::AddElement {
            id,
            kind: ElementKind::MisuseCase,
            name: "Spoofed key fob".into(),
            provenance: who(),
        },
    );
    apply_undo_redo(
        &mut ws,
        seen,
        Op::RenameElement {
            id,
            name: "Relayed key fob".into(),
            provenance: who(),
        },
    );
    for value in [
        Some(AttrValue::Level("cal-2".into())),
        Some(AttrValue::Text("Note".into())),
        None,
    ] {
        apply_undo_redo(
            &mut ws,
            seen,
            Op::SetElementAttribute {
                id,
                key: "cal".into(),
                value,
                provenance: who(),
            },
        );
    }
    apply_undo_redo(
        &mut ws,
        seen,
        Op::SetElementAttribute {
            id,
            key: "cal".into(),
            value: Some(AttrValue::Choice("a".into())),
            provenance: who(),
        },
    );
    apply_undo_redo(
        &mut ws,
        seen,
        Op::SetElementAttribute {
            id,
            key: "cal".into(),
            value: None,
            provenance: who(),
        },
    );
    apply_undo_redo(&mut ws, seen, Op::RemoveElement { id });
}

#[test]
fn every_op_kind_is_undone_and_redone_exactly() {
    let m = common::second();
    let mut ws = Workspace::empty(Arc::clone(&m));
    let mut seen = BTreeSet::new();
    for op in common::record_every_op(m) {
        apply_undo_redo(&mut ws, &mut seen, op);
    }
    walk_item_definition_trees_and_descriptions(&mut seen);
    walk_elements(&mut seen);
    assert_eq!(seen, declared_variants(), "an Op variant is not exercised");
}

/// A batch is one undo step: undoing it restores every element it touched.
#[test]
fn a_whole_session_of_edits_undoes_back_to_empty() {
    let m = common::second();
    let ops = common::record_every_op(Arc::clone(&m));
    let mut ws = Workspace::empty(Arc::clone(&m));
    ws.apply(ops, Origin::Edit).expect("the batch applies");
    ws.undo().expect("one undo step for the whole batch");
    assert!(!ws.can_undo());
    assert!(ws.project() == &Project::new(m));
}
