//! UI-10.6: every `Op` kind, applied through the workspace as one user edit,
//! is undone to exactly the previous project (links, ratings, descriptions,
//! tree nodes), and redone to exactly the project after it.
//!
//! Three walks cover all kinds. The first replays the ops of the model's own
//! "every op" walk-through over the second methodology (`second_every_op`,
//! 37 kinds: vocabulary, threats, trees, controls, goals); the second walk
//! below covers the next 29 (item definition, tree nodes, descriptions,
//! review marks); the third covers the generic element ops. A new `Op`
//! variant fails this test until a walk covers it.

#[path = "../../aegis-model/tests/common/mod.rs"]
mod common;

use std::collections::BTreeSet;

use std::sync::Arc;

use aegis_client::workspace::{Origin, Workspace};
use aegis_model::ItemRef;
use aegis_model::{
    AssetId, AssumptionId, AttackTree, AttrValue, ClaimId, ComponentId, ConditionId, ControlId,
    DamageId, DataFlowId, ElementId, ElementKind, ElementRef, GateKind, GoalId, InterfaceId,
    LinkTarget, Methodology, Node, NodeId, Op, PartyId, PartyKind, Project, Provenance,
    SharedStepId, StepRef, StepTarget, ThreatId, TreeId, TreeRole, TrustBoundaryId,
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

/// The 29 kinds the model's walk-through does not reach, built against a
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
    go(Op::SetControlKind {
        control: ctrl,
        kind: m.control_kinds().next(),
        provenance: who(),
    });
    go(Op::SetControlKind {
        control: ctrl,
        kind: None,
        provenance: who(),
    });
    go(Op::SetControlStatus {
        control: ctrl,
        status: aegis_model::ControlStatus::Implemented,
        provenance: who(),
    });
    go(Op::SetControlStatus {
        control: ctrl,
        status: aegis_model::ControlStatus::Off,
        provenance: who(),
    });
    go(Op::SetControlStatus {
        control: ctrl,
        status: aegis_model::ControlStatus::Planned,
        provenance: who(),
    });
    go(Op::RemoveSecurityControl { id: ctrl });

    // Attack tree nodes.
    let t = AttackTree::new(root, Node::gate(GateKind::Or, "Goal")).expect("tree");
    go(Op::AddAttackTree {
        role: Default::default(),
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
    let (asset, damage, threat) = (AssetId::new_v7(), DamageId::new_v7(), ThreatId::new_v7());
    let m = Arc::clone(ws.project().methodology());
    let category = m.impact_categories().next().expect("category");
    let level = m.levels(category).next().expect("level");
    for op in [
        Op::AddAsset {
            id: asset,
            name: "ECU".into(),
            properties: m.all_properties(),
            provenance: who(),
        },
        Op::AddDamageScenario {
            id: damage,
            name: "Harm".into(),
            impacts: vec![(category, level)],
            provenance: who(),
        },
        Op::AddThreatScenario {
            id: threat,
            name: "Spoof".into(),
            asset,
            category: m.threat_categories().next().expect("threat category"),
            catalog_refs: vec![],
            damages: vec![damage],
            provenance: who(),
        },
    ] {
        apply_undo_redo(&mut ws, seen, op);
    }
    apply_undo_redo(
        &mut ws,
        seen,
        Op::SetDamageLinkRationale {
            threat,
            damage,
            text: "Trusted boot".into(),
            provenance: who(),
        },
    );
    let target = LinkTarget::Threat(threat);
    apply_undo_redo(
        &mut ws,
        seen,
        Op::LinkElement {
            id,
            target,
            provenance: who(),
        },
    );
    apply_undo_redo(
        &mut ws,
        seen,
        Op::UnlinkElement {
            id,
            target,
            provenance: who(),
        },
    );
    apply_undo_redo(&mut ws, seen, Op::RemoveElement { id });
    let (system, component) = (ElementId::new_v7(), ComponentId::new_v7());
    for op in [
        Op::AddElement {
            id: system,
            kind: ElementKind::System,
            name: "Powertrain".into(),
            provenance: who(),
        },
        Op::AddComponent {
            id: component,
            name: "Gateway".into(),
            parent: None,
            provenance: who(),
        },
    ] {
        apply_undo_redo(&mut ws, seen, op);
    }
    for system in [Some(system), None] {
        apply_undo_redo(
            &mut ws,
            seen,
            Op::SetComponentSystem {
                id: component,
                system,
                provenance: who(),
            },
        );
    }
    apply_undo_redo(&mut ws, seen, Op::RemoveComponent { id: component });
    apply_undo_redo(&mut ws, seen, Op::RemoveElement { id: system });
}

/// The four party ops, over an empty workspace.
fn walk_parties(seen: &mut BTreeSet<String>) {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let mut ws = Workspace::empty(m);
    let id = PartyId::new_v7();
    for op in [
        Op::AddParty {
            id,
            name: "Bosch".into(),
            kind: PartyKind::Supplier,
            provenance: who(),
        },
        Op::RenameParty {
            id,
            name: "Bosch Mobility".into(),
            provenance: who(),
        },
        Op::SetPartyKind {
            id,
            kind: PartyKind::Customer,
            provenance: who(),
        },
    ] {
        apply_undo_redo(&mut ws, seen, op);
    }
    // A goal and a claim give the responsible-link ops a target. Each call
    // leaves its op applied, so the party, goal and claim exist below.
    let (goal, claim) = (GoalId::new_v7(), ClaimId::new_v7());
    for op in [
        Op::AddCybersecurityGoal {
            id: goal,
            name: "Protect the asset".into(),
            controls: vec![],
            provenance: who(),
        },
        Op::AddCybersecurityClaim {
            id: claim,
            name: "Retain with rationale".into(),
            provenance: who(),
        },
    ] {
        apply_undo_redo(&mut ws, seen, op);
    }
    apply_undo_redo(
        &mut ws,
        seen,
        Op::LinkGoalParty {
            goal,
            party: id,
            provenance: who(),
        },
    );
    apply_undo_redo(
        &mut ws,
        seen,
        Op::UnlinkGoalParty {
            goal,
            party: id,
            provenance: who(),
        },
    );
    apply_undo_redo(
        &mut ws,
        seen,
        Op::LinkClaimParty {
            claim,
            party: id,
            provenance: who(),
        },
    );
    apply_undo_redo(
        &mut ws,
        seen,
        Op::UnlinkClaimParty {
            claim,
            party: id,
            provenance: who(),
        },
    );
    // Tear down: no responsible links remain, so the party can be removed.
    let asset = AssetId::new_v7();
    let threat = ThreatId::new_v7();
    let m = Arc::clone(ws.project().methodology());
    let cat = m.threat_categories().next().expect("cat");
    apply_undo_redo(
        &mut ws,
        seen,
        Op::AddAsset {
            id: asset,
            name: "Gateway".into(),
            properties: m.all_properties(),
            provenance: who(),
        },
    );
    apply_undo_redo(
        &mut ws,
        seen,
        Op::AddThreatScenario {
            id: threat,
            name: "Spoof bus".into(),
            asset,
            category: cat,
            catalog_refs: vec![],
            damages: vec![],
            provenance: who(),
        },
    );
    apply_undo_redo(
        &mut ws,
        seen,
        Op::LinkThreatGoal {
            threat,
            goal,
            provenance: who(),
        },
    );
    apply_undo_redo(
        &mut ws,
        seen,
        Op::UnlinkThreatGoal {
            threat,
            goal,
            provenance: who(),
        },
    );
    apply_undo_redo(&mut ws, seen, Op::RemoveThreatScenario { id: threat });
    apply_undo_redo(&mut ws, seen, Op::RemoveAsset { id: asset });
    apply_undo_redo(&mut ws, seen, Op::RemoveCybersecurityGoal { id: goal });
    apply_undo_redo(&mut ws, seen, Op::RemoveCybersecurityClaim { id: claim });
    apply_undo_redo(&mut ws, seen, Op::RemoveParty { id });
}

fn walk_conditions(seen: &mut BTreeSet<String>) {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let mut ws = Workspace::empty(Arc::clone(&m));
    let id = ConditionId::new_v7();
    let asset = AssetId::new_v7();
    let (tree, root, step) = (TreeId::new_v7(), NodeId::new_v7(), NodeId::new_v7());
    for op in [
        Op::AddAsset {
            id: asset,
            name: "ECU".into(),
            properties: m.all_properties(),
            provenance: who(),
        },
        Op::AddAttackTree {
            role: Default::default(),
            id: tree,
            tree: AttackTree::new(root, Node::gate(GateKind::Or, "Goal")).expect("tree"),
            step_feasibility: vec![],
            provenance: who(),
        },
        Op::AddTreeNode {
            tree,
            parent: root,
            id: step,
            node: Node::step("Step"),
            index: 1,
            position: 0,
            provenance: who(),
        },
    ] {
        apply_undo_redo(&mut ws, seen, op);
    }
    for op in [
        Op::AddCondition {
            id,
            name: "Airbag deploys".into(),
            description: "Frontal crash".into(),
            present: true,
            provenance: who(),
        },
        Op::RenameCondition {
            id,
            name: "Airbag deploys in a crash".into(),
            provenance: who(),
        },
        Op::SetConditionPresent {
            id,
            present: false,
            provenance: who(),
        },
    ] {
        apply_undo_redo(&mut ws, seen, op);
    }
    let element = ElementRef::Asset(asset);
    for op in [
        Op::NeedCondition {
            element,
            condition: id,
            provenance: who(),
        },
        Op::NeedStepCondition {
            tree,
            node: step,
            condition: id,
            provenance: who(),
        },
        Op::UnneedCondition {
            element,
            condition: id,
            provenance: who(),
        },
        Op::UnneedStepCondition {
            tree,
            node: step,
            condition: id,
            provenance: who(),
        },
    ] {
        apply_undo_redo(&mut ws, seen, op);
    }
    apply_undo_redo(&mut ws, seen, Op::RemoveCondition { id });
}

fn walk_assumptions(seen: &mut BTreeSet<String>) {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let mut ws = Workspace::empty(m);
    let (id, cond, claim) = (
        AssumptionId::new_v7(),
        ConditionId::new_v7(),
        ClaimId::new_v7(),
    );
    let (tree, root, step) = (TreeId::new_v7(), NodeId::new_v7(), NodeId::new_v7());
    let target = ElementRef::Assumption(id);
    for op in [
        Op::AddCondition {
            id: cond,
            name: "Production mode".into(),
            description: "".into(),
            present: true,
            provenance: who(),
        },
        Op::AddCybersecurityClaim {
            id: claim,
            name: "Locked enclosure".into(),
            provenance: who(),
        },
        Op::AddAttackTree {
            role: Default::default(),
            id: tree,
            tree: AttackTree::new(root, Node::gate(GateKind::Or, "Goal")).expect("tree"),
            step_feasibility: vec![],
            provenance: who(),
        },
        Op::AddTreeNode {
            tree,
            parent: root,
            id: step,
            node: Node::step("Bypass lock"),
            index: 1,
            position: 0,
            provenance: who(),
        },
        Op::AddAssumption {
            id,
            name: "ECU physically secured".into(),
            holds: true,
            provenance: who(),
        },
        Op::RenameAssumption {
            id,
            name: "ECU inside locked cabin".into(),
            provenance: who(),
        },
        Op::SetAssumptionHolds {
            id,
            holds: false,
            provenance: who(),
        },
        Op::SetAssumptionExcludes {
            id,
            trees: vec![tree],
            steps: vec![StepRef { tree, node: step }],
            provenance: who(),
        },
        Op::LinkClaimAssumption {
            claim,
            assumption: id,
            provenance: who(),
        },
        Op::SetProjectLanguages {
            langs: Arc::from([Arc::from("ja")]),
            provenance: who(),
        },
        Op::SetDescription {
            target,
            text: "Mounted inside locked enclosure.".into(),
            provenance: who(),
        },
        Op::SetDescriptionTranslation {
            target,
            lang: "ja".into(),
            text: "ロックされた筐体内に設置。".into(),
            provenance: who(),
        },
        Op::NeedCondition {
            element: target,
            condition: cond,
            provenance: who(),
        },
        Op::UnneedCondition {
            element: target,
            condition: cond,
            provenance: who(),
        },
        Op::UnlinkClaimAssumption {
            claim,
            assumption: id,
            provenance: who(),
        },
        Op::SetAssumptionExcludes {
            id,
            trees: vec![],
            steps: vec![],
            provenance: who(),
        },
        Op::SetDescriptionTranslation {
            target,
            lang: "ja".into(),
            text: "".into(),
            provenance: who(),
        },
        Op::SetDescription {
            target,
            text: "".into(),
            provenance: who(),
        },
        Op::RemoveAssumption { id },
        Op::RemoveTreeNode {
            tree,
            id: step,
            provenance: who(),
        },
        Op::RemoveAttackTree { id: tree },
        Op::RemoveCybersecurityClaim { id: claim },
        Op::RemoveCondition { id: cond },
    ] {
        apply_undo_redo(&mut ws, seen, op);
    }
}

fn walk_shared_steps(seen: &mut BTreeSet<String>) {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let mut ws = Workspace::empty(Arc::clone(&m));
    let id = SharedStepId::new_v7();
    let component = ComponentId::new_v7();
    let tree = TreeId::new_v7();
    let node = NodeId::new_v7();
    let fragment = TreeId::new_v7();
    let fragment_root = NodeId::new_v7();
    let target = StepTarget::Shared(id);
    let factor = m.feasibility_factors().next().expect("factor");
    let level = m.factor_levels(factor).next().expect("factor level");
    let attack_type = m.attack_type("debug-access").expect("attack type");
    for op in [
        Op::AddComponent {
            id: component,
            name: "Gateway".into(),
            parent: None,
            provenance: who(),
        },
        Op::AddAttackTree {
            role: Default::default(),
            id: tree,
            tree: AttackTree::new(node, Node::step("Inject frame")).expect("tree"),
            step_feasibility: Vec::new(),
            provenance: who(),
        },
        Op::AddSharedStep {
            id,
            name: "Inject frame".into(),
            provenance: who(),
        },
        Op::RenameSharedStep {
            id,
            name: "Spoof frame".into(),
            provenance: who(),
        },
        Op::SetStepFeasibility {
            target,
            level: Some(m.feasibility_level("high").expect("feasibility")),
            provenance: who(),
        },
        Op::SetStepFeasibility {
            target,
            level: None,
            provenance: who(),
        },
        Op::SetStepFactor {
            target,
            factor,
            level: Some(level),
            provenance: who(),
        },
        Op::SetStepFactor {
            target,
            factor,
            level: None,
            provenance: who(),
        },
        Op::SetStepVia {
            target,
            via: Some(ItemRef::Component(component)),
            provenance: who(),
        },
        Op::SetStepVia {
            target,
            via: None,
            provenance: who(),
        },
        Op::SetStepAttackType {
            target,
            attack_type: Some(attack_type),
            provenance: who(),
        },
        Op::SetStepAttackType {
            target,
            attack_type: None,
            provenance: who(),
        },
        Op::UseSharedStep {
            tree,
            node,
            step: id,
            provenance: who(),
        },
        Op::UnuseSharedStep {
            tree,
            node,
            provenance: who(),
        },
        Op::AddAttackTree {
            role: TreeRole::Fragment,
            id: fragment,
            tree: AttackTree::new(fragment_root, Node::step("Fragment")).expect("tree"),
            step_feasibility: Vec::new(),
            provenance: who(),
        },
        Op::UseFragment {
            tree,
            node,
            fragment,
            provenance: who(),
        },
        Op::UnuseFragment {
            tree,
            node,
            provenance: who(),
        },
        Op::RemoveAttackTree { id: fragment },
        Op::RemoveSharedStep { id },
        Op::RemoveAttackTree { id: tree },
        Op::RemoveComponent { id: component },
    ] {
        apply_undo_redo(&mut ws, seen, op);
    }
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
    walk_parties(&mut seen);
    walk_conditions(&mut seen);
    walk_assumptions(&mut seen);
    walk_shared_steps(&mut seen);
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

/// Removing a goal or claim that lists a party is one undo step that brings
/// the party links back.
#[test]
fn removing_a_goal_or_claim_with_parties_undoes_the_links() {
    use aegis_client::commands::Edit;
    use aegis_model::{ClaimId, GoalId};
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let mut ws = Workspace::empty(m);
    let (party, goal, claim) = (PartyId::new_v7(), GoalId::new_v7(), ClaimId::new_v7());
    let setup = vec![
        Op::AddParty {
            id: party,
            name: "Bosch".into(),
            kind: PartyKind::Supplier,
            provenance: who(),
        },
        Op::AddCybersecurityGoal {
            id: goal,
            name: "G".into(),
            controls: vec![],
            provenance: who(),
        },
        Op::AddCybersecurityClaim {
            id: claim,
            name: "C".into(),
            provenance: who(),
        },
        Op::LinkGoalParty {
            goal,
            party,
            provenance: who(),
        },
        Op::LinkClaimParty {
            claim,
            party,
            provenance: who(),
        },
    ];
    ws.apply(setup, Origin::Edit).expect("setup");
    let before = ws.project().clone();
    for edit in [Edit::RemoveGoal { goal }, Edit::RemoveClaim { claim }] {
        let ops = edit.to_ops(ws.project(), "alice", 1).expect("ops");
        ws.apply(ops, Origin::Edit).expect("removes");
    }
    ws.undo().expect("undo claim removal");
    ws.undo().expect("undo goal removal");
    assert!(*ws.project() == before, "party links restored");
}

/// Removing a condition, an asset, a step or a tree that needs one is one
/// edit and one undo step, and undo restores every condition link.
#[test]
fn removals_clear_condition_links_in_one_undo_step() {
    use aegis_client::commands::Edit;
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    type MakeEdit = fn(AssetId, TreeId, NodeId, ConditionId) -> Edit;
    let edits: [MakeEdit; 4] = [
        |_, _, _, condition| Edit::RemoveCondition { condition },
        |asset, _, _, _| Edit::RemoveAsset { asset },
        |_, tree, node, _| Edit::RemoveTreeNode { tree, node },
        |_, tree, _, _| Edit::RemoveTree { tree },
    ];
    for (n, make) in edits.into_iter().enumerate() {
        let mut ws = Workspace::empty(Arc::clone(&m));
        let (asset, tree, root, step) = (
            AssetId::new_v7(),
            TreeId::new_v7(),
            NodeId::new_v7(),
            NodeId::new_v7(),
        );
        let condition = ConditionId::new_v7();
        let mut at = AttackTree::new(root, Node::gate(GateKind::Or, "root")).expect("tree");
        at.add_child(at.root(), step, Node::step("step"))
            .expect("step");
        let setup = vec![
            Op::AddCondition {
                id: condition,
                name: "Bluetooth fitted".into(),
                description: "".into(),
                present: true,
                provenance: who(),
            },
            Op::AddAsset {
                id: asset,
                name: "Key".into(),
                properties: m.all_properties(),
                provenance: who(),
            },
            Op::AddAttackTree {
                role: Default::default(),
                id: tree,
                tree: at,
                step_feasibility: vec![],
                provenance: who(),
            },
            Op::NeedCondition {
                element: ElementRef::Asset(asset),
                condition,
                provenance: who(),
            },
            Op::NeedCondition {
                element: ElementRef::Tree(tree),
                condition,
                provenance: who(),
            },
            Op::NeedStepCondition {
                tree,
                node: step,
                condition,
                provenance: who(),
            },
        ];
        ws.apply(setup, Origin::Edit).expect("setup");
        let before = ws.project().clone();
        let ops = make(asset, tree, step, condition)
            .to_ops(ws.project(), "alice", 1)
            .unwrap_or_else(|e| panic!("edit {n}: {e:?}"));
        ws.apply(ops, Origin::Edit)
            .unwrap_or_else(|e| panic!("edit {n}: {e:?}"));
        ws.undo().expect("one undo step");
        assert!(
            *ws.project() == before,
            "edit {n}: undo restores every link"
        );
    }
}

/// L30 round-trips for assumption removals, claim removals with assumptions,
/// tree/node removals on assumption-excluded targets, outline step moves, and
/// `BranchStepToGate` with `transfer_ratings: true`.
#[test]
fn assumption_edits_removals_and_tree_moves_undo_exactly() {
    use aegis_client::commands::Edit;
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let mut ws = Workspace::empty(Arc::clone(&m));
    let (as_id, cond, claim, party) = (
        AssumptionId::new_v7(),
        ConditionId::new_v7(),
        ClaimId::new_v7(),
        PartyId::new_v7(),
    );
    let (tree, root, gate_a, gate_b, step1, step2) = (
        TreeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
    );
    let mut at = AttackTree::new(root, Node::gate(GateKind::Or, "Root")).expect("root");
    let r = at.root();
    let ga = at
        .add_child(r, gate_a, Node::gate(GateKind::And, "Gate A"))
        .expect("ga");
    let gb = at
        .add_child(r, gate_b, Node::gate(GateKind::Or, "Gate B"))
        .expect("gb");
    at.add_child(ga, step1, Node::step("Step 1")).expect("s1");
    at.add_child(gb, step2, Node::step("Step 2")).expect("s2");

    let target_as = ElementRef::Assumption(as_id);
    let target_claim = ElementRef::Claim(claim);
    ws.apply(
        vec![
            Op::SetProjectLanguages {
                langs: Arc::from([Arc::from("ja")]),
                provenance: who(),
            },
            Op::AddCondition {
                id: cond,
                name: "Prod".into(),
                description: "".into(),
                present: true,
                provenance: who(),
            },
            Op::AddParty {
                id: party,
                name: "Supplier".into(),
                kind: PartyKind::Supplier,
                provenance: who(),
            },
            Op::AddCybersecurityClaim {
                id: claim,
                name: "Locked".into(),
                provenance: who(),
            },
            Op::LinkClaimParty {
                claim,
                party,
                provenance: who(),
            },
            Op::NeedCondition {
                element: target_claim,
                condition: cond,
                provenance: who(),
            },
            Op::SetDescription {
                target: target_claim,
                text: "Claim note".into(),
                provenance: who(),
            },
            Op::AddAttackTree {
                role: Default::default(),
                id: tree,
                tree: at,
                step_feasibility: vec![],
                provenance: who(),
            },
            Op::AddAssumption {
                id: as_id,
                name: "Locked box".into(),
                holds: true,
                provenance: who(),
            },
            Op::LinkClaimAssumption {
                claim,
                assumption: as_id,
                provenance: who(),
            },
            Op::NeedCondition {
                element: target_as,
                condition: cond,
                provenance: who(),
            },
            Op::SetDescription {
                target: target_as,
                text: "Steel box".into(),
                provenance: who(),
            },
            Op::SetDescriptionTranslation {
                target: target_as,
                lang: "ja".into(),
                text: "鋼鉄製ボックス".into(),
                provenance: who(),
            },
            Op::SetAssumptionExcludes {
                id: as_id,
                trees: vec![tree],
                steps: vec![StepRef { tree, node: step1 }, StepRef { tree, node: step2 }],
                provenance: who(),
            },
        ],
        Origin::Edit,
    )
    .expect("setup");

    let run = |ws: &mut Workspace, edit: Edit| {
        let ops = edit.to_ops(ws.project(), "alice", 2).expect("to_ops");
        ws.apply(ops, Origin::Edit).expect("apply");
    };

    // 1. RemoveAssumption clears description, translation, condition, claim link, and excludes.
    let before = ws.project().clone();
    run(&mut ws, Edit::RemoveAssumption { assumption: as_id });
    assert!(ws.project().assumption_idx(&as_id).is_none());
    ws.undo().expect("undo RemoveAssumption");
    assert!(*ws.project() == before, "RemoveAssumption undo mismatch");

    // 2. RemoveClaim unlinks assumption, party, condition, and description.
    run(&mut ws, Edit::RemoveClaim { claim });
    assert!(ws.project().cybersecurity_claim_idx(&claim).is_none());
    ws.undo().expect("undo RemoveClaim");
    assert!(*ws.project() == before, "RemoveClaim undo mismatch");

    // 3. RemoveTreeNode on an excluded step (and gate subtree) cleans up excludes cleanly.
    run(&mut ws, Edit::RemoveTreeNode { tree, node: step1 });
    ws.undo().expect("undo RemoveTreeNode");
    assert!(*ws.project() == before, "RemoveTreeNode undo mismatch");

    // 4. RemoveTree on a tree with excluded tree + multiple excluded steps.
    run(&mut ws, Edit::RemoveTree { tree });
    assert!(ws.project().attack_tree_idx(&tree).is_none());
    ws.undo().expect("undo RemoveTree");
    assert!(*ws.project() == before, "RemoveTree undo mismatch");

    // 5. ReplaceTree moving excluded step1 under Gate B preserves its exclusion and undoes.
    let moved_outline = "OR Root\n  AND Gate A\n  OR Gate B\n    - Step 1\n    - Step 2\n";
    run(
        &mut ws,
        Edit::ReplaceTree {
            tree,
            outline: moved_outline.to_owned(),
        },
    );
    let a = ws
        .project()
        .assumption(ws.project().assumption_idx(&as_id).expect("as"))
        .expect("a");
    assert!(a.excludes_steps().contains(&StepRef { tree, node: step1 }));
    assert!(a.excludes_steps().contains(&StepRef { tree, node: step2 }));
    ws.undo().expect("undo ReplaceTree move");
    assert!(*ws.project() == before, "ReplaceTree move undo mismatch");

    // 5b. ReplaceTree deleting step2 while moving step1 keeps step1 and drops step2 cleanly.
    let drop_and_move = "OR Root\n  AND Gate A\n  OR Gate B\n    - Step 1\n";
    run(
        &mut ws,
        Edit::ReplaceTree {
            tree,
            outline: drop_and_move.to_owned(),
        },
    );
    let a = ws
        .project()
        .assumption(ws.project().assumption_idx(&as_id).expect("as"))
        .expect("a");
    assert!(a.excludes_steps().contains(&StepRef { tree, node: step1 }));
    assert!(!a.excludes_steps().contains(&StepRef { tree, node: step2 }));
    ws.undo().expect("undo ReplaceTree drop+move");
    assert!(
        *ws.project() == before,
        "ReplaceTree drop+move undo mismatch"
    );

    // 6. BranchStepToGate transfers assumption exclusion from step1 onto the new child step.
    let child_id = NodeId::new_v7();
    run(
        &mut ws,
        Edit::BranchStepToGate {
            tree,
            node: step1,
            id: child_id,
            gate: GateKind::Or,
            child_label: "Sub-step".into(),
        },
    );
    let a = ws
        .project()
        .assumption(ws.project().assumption_idx(&as_id).expect("as"))
        .expect("a");
    assert!(!a.excludes_steps().contains(&StepRef { tree, node: step1 }));
    assert!(a.excludes_steps().contains(&StepRef {
        tree,
        node: child_id
    }));
    assert!(a.excludes_steps().contains(&StepRef { tree, node: step2 }));
    ws.undo().expect("undo BranchStepToGate");
    assert!(*ws.project() == before, "BranchStepToGate undo mismatch");
}

#[test]
fn set_attack_filter_op_and_edit_undo_redo() {
    use aegis_client::commands::Edit;
    use aegis_model::{AttackFilter, ScalabilityFilter};

    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let f0 = m.feasibility_factors().next().expect("factor");
    let l0 = m.factor_levels(f0).next().expect("level");
    let mut ws = Workspace::empty(Arc::clone(&m));
    let initial = ws.project().clone();

    let non_default = AttackFilter::default()
        .without(f0, l0)
        .with_scalability(ScalabilityFilter::ScaledOnly);

    // Raw Op::SetAttackFilter (change + reset).
    for filter in [non_default, AttackFilter::default()] {
        let before = ws.project().clone();
        ws.apply(
            vec![Op::SetAttackFilter {
                filter,
                provenance: who(),
            }],
            Origin::Edit,
        )
        .expect("apply op");
        let after = ws.project().clone();
        ws.undo().expect("undo op");
        assert!(*ws.project() == before);
        ws.redo().expect("redo op");
        assert!(*ws.project() == after);
    }
    assert!(*ws.project() == initial);

    // Edit::SetAttackFilter (change + reset).
    for edit in [
        Edit::SetAttackFilter {
            filter: non_default,
        },
        Edit::SetAttackFilter {
            filter: AttackFilter::default(),
        },
    ] {
        let before = ws.project().clone();
        let ops = edit.to_ops(ws.project(), "alice", 1).expect("to_ops");
        ws.apply(ops, Origin::Edit).expect("apply edit");
        let after = ws.project().clone();
        ws.undo().expect("undo edit");
        assert!(*ws.project() == before);
        ws.redo().expect("redo edit");
        assert!(*ws.project() == after);
    }
    assert!(*ws.project() == initial);
}
