//! Undo audit: every removal edit on a project where everything is linked,
//! described, rated and conditioned. Each edit must succeed (it clears what
//! the model's guards refuse) and one undo must restore the saved bytes
//! exactly, visual IDs and list order included (L06, L30).

use std::sync::Arc;

use aegis_client::commands::Edit;
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{
    AssetId, AttackTree, AttrValue, ClaimId, ComponentId, ConditionId, ControlId, DamageId,
    DataFlowId, ElementId, ElementKind, ElementRef, GateKind, GoalId, InterfaceId, ItemRef,
    LinkTarget, Methodology, Node, NodeId, Op, PartyId, PartyKind, Provenance, RiskTreatment,
    StepTarget, ThreatId, TreeId, TrustBoundaryId,
};

fn who() -> Provenance {
    Provenance::human("alice", 1_700_000_000_000)
}

fn saved(ws: &Workspace) -> Vec<u8> {
    let mut out = Vec::new();
    ws.project().save(&mut out).expect("save");
    out
}

/// Builds one edit from the fixture's IDs.
type MakeEdit = fn(&Ids) -> Edit;

struct Ids {
    system: ElementId,
    c1: ComponentId,
    c2: ComponentId,
    iface: InterfaceId,
    flow: DataFlowId,
    boundary: TrustBoundaryId,
    asset: AssetId,
    damage: DamageId,
    threat: ThreatId,
    rated: ThreatId,
    tree: TreeId,
    step: NodeId,
    control: ControlId,
    goal: GoalId,
    claim: ClaimId,
    condition: ConditionId,
}

/// A project with every kind of element and every link, field and side
/// table set at least once.
#[allow(clippy::too_many_lines)] // one flat list of setup ops
fn loaded() -> (Workspace, Ids) {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let mut ws = Workspace::empty(Arc::clone(&m));
    let ids = Ids {
        system: ElementId::new_v7(),
        c1: ComponentId::new_v7(),
        c2: ComponentId::new_v7(),
        iface: InterfaceId::new_v7(),
        flow: DataFlowId::new_v7(),
        boundary: TrustBoundaryId::new_v7(),
        asset: AssetId::new_v7(),
        damage: DamageId::new_v7(),
        threat: ThreatId::new_v7(),
        rated: ThreatId::new_v7(),
        tree: TreeId::new_v7(),
        step: NodeId::new_v7(),
        control: ControlId::new_v7(),
        goal: GoalId::new_v7(),
        claim: ClaimId::new_v7(),
        condition: ConditionId::new_v7(),
    };
    let i = &ids;
    let (party, misuse, root) = (PartyId::new_v7(), ElementId::new_v7(), NodeId::new_v7());
    let category = m.impact_categories().next().expect("impact category");
    let level = m.levels(category).next().expect("level");
    let low = m.feasibility_level("low").expect("low");
    let attack_type = m.attack_types().next().expect("attack type");
    let mut tree = AttackTree::new(root, Node::gate(GateKind::Or, "root")).expect("tree");
    tree.add_child(tree.root(), i.step, Node::step("Replay the CAN frame"))
        .expect("step");
    let p = who;
    let mut ops = vec![
        Op::AddElement {
            id: i.system,
            kind: ElementKind::System,
            name: "Body".into(),
            provenance: p(),
        },
        Op::AddComponent {
            id: i.c1,
            name: "Gateway".into(),
            parent: None,
            provenance: p(),
        },
        Op::AddComponent {
            id: i.c2,
            name: "Door ECU".into(),
            parent: Some(i.c1),
            provenance: p(),
        },
        Op::SetComponentSystem {
            id: i.c1,
            system: Some(i.system),
            provenance: p(),
        },
        Op::AddInterface {
            id: i.iface,
            name: "CAN".into(),
            component: i.c1,
            kind: m.interface_kind("can").expect("can"),
            provenance: p(),
        },
        Op::AddDataFlow {
            id: i.flow,
            name: "Unlock".into(),
            from: i.c1,
            to: i.c2,
            interface: Some(i.iface),
            provenance: p(),
        },
        Op::AddTrustBoundary {
            id: i.boundary,
            name: "Vehicle".into(),
            components: vec![i.c1, i.c2],
            provenance: p(),
        },
        Op::AddAsset {
            id: i.asset,
            name: "Unlock command".into(),
            properties: m.all_properties(),
            provenance: p(),
        },
        Op::SetAssetComponent {
            asset: i.asset,
            component: Some(i.c1),
            provenance: p(),
        },
        Op::AddDamageScenario {
            id: i.damage,
            name: "Theft".into(),
            impacts: vec![(category, level)],
            provenance: p(),
        },
        Op::SetImpactRationale {
            damage: i.damage,
            text: "Car stolen".into(),
            provenance: p(),
        },
        Op::AddThreatScenario {
            id: i.threat,
            name: "Replay unlock".into(),
            asset: i.asset,
            category: m.threat_categories().next().expect("threat category"),
            catalog_refs: vec![],
            damages: vec![i.damage],
            provenance: p(),
        },
        Op::SetDamageLinkRationale {
            threat: i.threat,
            damage: i.damage,
            text: "Opens the door".into(),
            provenance: p(),
        },
        Op::AddThreatScenario {
            id: i.rated,
            name: "Spoof unlock".into(),
            asset: i.asset,
            category: m.threat_categories().next().expect("threat category"),
            catalog_refs: vec![],
            damages: vec![i.damage],
            provenance: p(),
        },
        Op::SetThreatFeasibility {
            threat: i.rated,
            level: Some(low),
            provenance: p(),
        },
        Op::SetFeasibilityRationale {
            threat: i.rated,
            text: "Needs a tool".into(),
            provenance: p(),
        },
        Op::SetThreatScaling {
            threat: i.rated,
            scales: Some(true),
            reason: "Fleet".into(),
            provenance: p(),
        },
        Op::AddAttackTree {
            id: i.tree,
            tree,
            step_feasibility: vec![],
            provenance: p(),
        },
        Op::SetStepFeasibility {
            target: StepTarget::Step(i.tree, i.step),
            level: Some(low),
            provenance: p(),
        },
        Op::SetStepVia {
            target: StepTarget::Step(i.tree, i.step),
            via: Some(ItemRef::Interface(i.iface)),
            provenance: p(),
        },
        Op::SetStepAttackType {
            target: StepTarget::Step(i.tree, i.step),
            attack_type: Some(attack_type),
            provenance: p(),
        },
        Op::SetReviewMark {
            tree: i.tree,
            node: i.step,
            marked: true,
            provenance: p(),
        },
        Op::SetThreatAttackTree {
            threat: i.threat,
            tree: Some(i.tree),
            provenance: p(),
        },
        Op::AddSecurityControl {
            id: i.control,
            name: "Rolling code".into(),
            provenance: p(),
        },
        Op::ApplyControl {
            control: i.control,
            target: ItemRef::Component(i.c1),
            provenance: p(),
        },
        Op::SetControlRule {
            control: i.control,
            attack_type,
            ceiling: Some(low),
            provenance: p(),
        },
        Op::SetControlKind {
            control: i.control,
            kind: m.control_kinds().next(),
            provenance: p(),
        },
        Op::SetStepMitigation {
            tree: i.tree,
            node: i.step,
            control: i.control,
            feasibility: Some(low),
            provenance: p(),
        },
        Op::AddCybersecurityGoal {
            id: i.goal,
            name: "No replay".into(),
            controls: vec![i.control],
            provenance: p(),
        },
        Op::AddCybersecurityClaim {
            id: i.claim,
            name: "Accepted".into(),
            provenance: p(),
        },
        Op::SetRiskTreatment {
            threat: i.threat,
            treatment: Some(RiskTreatment::Reduce),
            provenance: p(),
        },
        Op::SetThreatGoal {
            threat: i.threat,
            goal: Some(i.goal),
            provenance: p(),
        },
        Op::SetThreatClaim {
            threat: i.threat,
            claim: Some(i.claim),
            provenance: p(),
        },
        Op::AddParty {
            id: party,
            name: "Supplier".into(),
            kind: PartyKind::Supplier,
            provenance: p(),
        },
        Op::LinkGoalParty {
            goal: i.goal,
            party,
            provenance: p(),
        },
        Op::LinkClaimParty {
            claim: i.claim,
            party,
            provenance: p(),
        },
        Op::AddElement {
            id: misuse,
            kind: ElementKind::MisuseCase,
            name: "Relay".into(),
            provenance: p(),
        },
        Op::SetElementAttribute {
            id: misuse,
            key: "cal".into(),
            value: Some(AttrValue::Choice("a".into())),
            provenance: p(),
        },
        Op::LinkElement {
            id: misuse,
            target: LinkTarget::Threat(i.threat),
            provenance: p(),
        },
        Op::AddCondition {
            id: i.condition,
            name: "Keyless entry fitted".into(),
            description: "Only some trims".into(),
            present: true,
            provenance: p(),
        },
        Op::NeedStepCondition {
            tree: i.tree,
            node: i.step,
            condition: i.condition,
            provenance: p(),
        },
    ];
    let described = [
        ElementRef::Component(i.c1),
        ElementRef::Component(i.c2),
        ElementRef::Interface(i.iface),
        ElementRef::DataFlow(i.flow),
        ElementRef::TrustBoundary(i.boundary),
        ElementRef::Asset(i.asset),
        ElementRef::Damage(i.damage),
        ElementRef::Threat(i.threat),
        ElementRef::Threat(i.rated),
        ElementRef::Tree(i.tree),
        ElementRef::Control(i.control),
        ElementRef::Goal(i.goal),
        ElementRef::Claim(i.claim),
    ];
    for target in described {
        ops.push(Op::SetDescription {
            target,
            text: "Described".into(),
            provenance: p(),
        });
    }
    for element in [
        ElementRef::Asset(i.asset),
        ElementRef::Component(i.c1),
        ElementRef::Component(i.c2),
        ElementRef::Interface(i.iface),
        ElementRef::Threat(i.threat),
        ElementRef::Threat(i.rated),
        ElementRef::Tree(i.tree),
        ElementRef::Goal(i.goal),
        ElementRef::Claim(i.claim),
    ] {
        ops.push(Op::NeedCondition {
            element,
            condition: i.condition,
            provenance: p(),
        });
    }
    for op in ops {
        let label = format!("{op:?}");
        ws.apply(vec![op], Origin::Edit)
            .unwrap_or_else(|e| panic!("setup {label}: {e:?}"));
    }
    (ws, ids)
}

#[test]
fn every_removal_edit_on_a_loaded_project_undoes_byte_for_byte() {
    let removals: Vec<(&str, MakeEdit)> = vec![
        ("condition", |i| Edit::RemoveCondition {
            condition: i.condition,
        }),
        ("claim", |i| Edit::RemoveClaim { claim: i.claim }),
        ("goal", |i| Edit::RemoveGoal { goal: i.goal }),
        ("control", |i| Edit::RemoveControl { control: i.control }),
        ("tree node", |i| Edit::RemoveTreeNode {
            tree: i.tree,
            node: i.step,
        }),
        ("tree", |i| Edit::RemoveTree { tree: i.tree }),
        ("threat", |i| Edit::RemoveThreat { threat: i.threat }),
        ("rated threat", |i| Edit::RemoveThreat { threat: i.rated }),
        ("damage", |i| Edit::RemoveDamage { damage: i.damage }),
        ("data flow", |i| Edit::RemoveDataFlow { data_flow: i.flow }),
        ("trust boundary", |i| Edit::RemoveTrustBoundary {
            boundary: i.boundary,
        }),
        ("interface", |i| Edit::RemoveInterface {
            interface: i.iface,
        }),
        ("child component", |i| Edit::RemoveComponent {
            component: i.c2,
        }),
        ("system", |i| Edit::RemoveSystem { system: i.system }),
    ];
    let mut refused = Vec::new();
    for (what, make) in removals {
        let (mut ws, ids) = loaded();
        let before = saved(&ws);
        let ops = match make(&ids).to_ops(ws.project(), "alice", 1_800_000_000_000) {
            Ok(ops) => ops,
            Err(e) => {
                refused.push(format!("{what}: edit refused: {e:?}"));
                continue;
            }
        };
        if let Err(e) = ws.apply(ops, Origin::Edit) {
            refused.push(format!("{what}: model refused: {e:?}"));
            continue;
        }
        assert_ne!(saved(&ws), before, "{what}: nothing was removed");
        ws.undo().expect("one undo step");
        assert!(
            saved(&ws) == before,
            "{what}: undo did not restore the saved bytes"
        );
    }
    assert!(
        refused.is_empty(),
        "removals that fail on a loaded project:\n{}",
        refused.join("\n")
    );
}

/// Removing the asset, after its threats are gone, and the parent component
/// after its dependants are gone, are the last two removals.
#[test]
fn asset_and_parent_component_removals_undo_byte_for_byte() {
    let (mut ws, i) = loaded();
    for edit in [
        Edit::RemoveThreat { threat: i.threat },
        Edit::RemoveThreat { threat: i.rated },
    ] {
        let ops = edit.to_ops(ws.project(), "alice", 1).expect("ops");
        ws.apply(ops, Origin::Edit).expect("threat removed");
    }
    for (what, edit) in [
        ("asset", Edit::RemoveAsset { asset: i.asset }),
        (
            "parent component",
            Edit::RemoveComponent { component: i.c1 },
        ),
    ] {
        let before = saved(&ws);
        match edit
            .to_ops(ws.project(), "alice", 1)
            .map(|ops| ws.apply(ops, Origin::Edit))
        {
            Ok(Ok(_)) => {
                ws.undo().expect("undo");
                assert!(
                    saved(&ws) == before,
                    "{what}: undo did not restore the saved bytes"
                );
            }
            other => panic!("{what}: removal failed on a loaded project: {other:?}"),
        }
    }
}

/// Tree restructuring on a loaded step: turning it into a gate, or putting a
/// gate above it, succeeds and undoes byte for byte.
#[test]
fn restructuring_a_loaded_step_undoes_byte_for_byte() {
    let edits: Vec<(&str, MakeEdit)> = vec![
        ("branch step to gate", |i| Edit::BranchStepToGate {
            tree: i.tree,
            node: i.step,
            id: NodeId::new_v7(),
            gate: GateKind::And,
            child_label: "First half".into(),
        }),
        ("insert parent gate", |i| Edit::InsertParentGate {
            tree: i.tree,
            node: i.step,
            id: NodeId::new_v7(),
            gate: GateKind::And,
            label: "Both".into(),
        }),
    ];
    for (what, make) in edits {
        let (mut ws, ids) = loaded();
        let before = saved(&ws);
        let ops = make(&ids)
            .to_ops(ws.project(), "alice", 1)
            .unwrap_or_else(|e| panic!("{what}: edit refused: {e:?}"));
        ws.apply(ops, Origin::Edit)
            .unwrap_or_else(|e| panic!("{what}: model refused: {e:?}"));
        ws.undo().expect("undo");
        assert!(
            saved(&ws) == before,
            "{what}: undo did not restore the saved bytes"
        );
    }
}
