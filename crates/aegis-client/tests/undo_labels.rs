//! UI-10.2, UI-10.9, UI-10.10: the sentences naming what Undo and Redo will
//! do, and removals of item-definition elements undone exactly (UI-10.6).

use std::sync::Arc;

use aegis_client::commands::Edit;
use aegis_client::view::Selection;
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{
    AssetId, ClaimId, ComponentId, DamageId, DataFlowId, GoalId, InterfaceId, Methodology, Op,
    PartyId, PartyKind, Provenance, ThreatId, TrustBoundaryId,
};

fn apply(ws: &mut Workspace, edit: &Edit) {
    let ops = edit
        .to_ops(ws.project(), "mira", 1_792_956_600_000)
        .expect("builds");
    ws.apply(ops, Origin::Edit).expect("applies");
}

fn workspace() -> Workspace {
    Workspace::empty(Arc::new(Methodology::iso21434_r155().expect("builtin")))
}

#[test]
fn undo_and_redo_name_the_action_and_the_element() {
    let mut ws = workspace();
    let asset = AssetId::new_v7();
    assert!(ws.undo_label().is_none() && ws.redo_label().is_none());
    apply(
        &mut ws,
        &Edit::AddAsset {
            id: asset,
            name: "Brake ECU".into(),
            properties: vec![0],
        },
    );
    assert_eq!(
        ws.undo_label().expect("label").imperative(),
        "add asset 'Brake ECU'"
    );
    apply(&mut ws, &Edit::RemoveAsset { asset });
    let removal = ws.undo_label().expect("label");
    assert!(removal.is_removal());
    assert_eq!(removal.imperative(), "remove asset 'Brake ECU'");
    assert_eq!(removal.past(), "Asset 'Brake ECU' removed");

    ws.undo().expect("undo");
    assert_eq!(
        ws.redo_label().expect("label").imperative(),
        "remove asset 'Brake ECU'",
        "redo names the same action"
    );
    assert_eq!(
        ws.undo_label().expect("label").imperative(),
        "add asset 'Brake ECU'"
    );
    ws.redo().expect("redo");
    assert!(ws.redo_label().is_none());
}

#[test]
fn a_removal_says_what_else_it_changed() {
    let mut ws = workspace();
    let (asset, damage) = (AssetId::new_v7(), DamageId::new_v7());
    apply(
        &mut ws,
        &Edit::AddAsset {
            id: asset,
            name: "ECU".into(),
            properties: (0..6).collect(),
        },
    );
    apply(
        &mut ws,
        &Edit::AddDamage {
            id: damage,
            name: "Crash".into(),
        },
    );
    for n in 0..3 {
        let threat = ThreatId::new_v7();
        apply(
            &mut ws,
            &Edit::AddThreat {
                id: threat,
                name: format!("Threat {n}"),
                asset,
                category: 0,
            },
        );
        apply(
            &mut ws,
            &Edit::LinkDamage {
                threat,
                damage,
                linked: true,
            },
        );
    }
    apply(&mut ws, &Edit::RemoveDamage { damage });
    assert_eq!(
        ws.undo_label().expect("label").past(),
        "Damage scenario 'Crash' removed and unlinked from 3 threat scenarios"
    );
}

#[test]
fn a_long_name_is_shortened() {
    let mut ws = workspace();
    apply(
        &mut ws,
        &Edit::AddAsset {
            id: AssetId::new_v7(),
            name: "x".repeat(100),
            properties: vec![0],
        },
    );
    let text = ws.undo_label().expect("label").imperative();
    assert!(text.chars().count() < 80, "{text}");
    assert!(text.contains('\u{2026}'));
}

/// Item-definition removals: each is one step, and undoing it brings back the
/// element with every link it cut (interface, flow, boundary membership).
#[test]
fn item_definition_removals_undo_exactly() {
    let mut ws = workspace();
    let (c1, c2) = (ComponentId::new_v7(), ComponentId::new_v7());
    let (iface, flow, boundary) = (
        InterfaceId::new_v7(),
        DataFlowId::new_v7(),
        TrustBoundaryId::new_v7(),
    );
    for edit in [
        Edit::AddComponent {
            id: c1,
            name: "Gateway".into(),
            parent: None,
        },
        Edit::AddComponent {
            id: c2,
            name: "Brake".into(),
            parent: Some(c1),
        },
        Edit::AddInterface {
            id: iface,
            name: "CAN".into(),
            component: c1,
            kind: 0,
        },
        Edit::AddDataFlow {
            id: flow,
            name: "Frame".into(),
            from: c1,
            to: c2,
            interface: Some(iface),
        },
        Edit::AddTrustBoundary {
            id: boundary,
            name: "Safety".into(),
        },
        Edit::SetTrustBoundaryMember {
            boundary,
            component: c2,
            member: true,
        },
    ] {
        apply(&mut ws, &edit);
    }
    let full = ws.project().clone();
    for edit in [
        Edit::RemoveDataFlow { data_flow: flow },
        Edit::RemoveInterface { interface: iface },
        Edit::RemoveTrustBoundary { boundary },
        Edit::RemoveComponent { component: c2 },
        Edit::RemoveComponent { component: c1 },
    ] {
        apply(&mut ws, &edit);
        assert!(ws.undo_label().expect("label").is_removal(), "{edit:?}");
    }
    for _ in 0..5 {
        ws.undo().expect("undo");
    }
    assert!(ws.project() == &full, "undo did not restore every link");
}

/// The four party-link ops name the goal or claim they change and resolve to
/// its selection; none falls through to the "change project" catch-all.
#[test]
fn party_link_ops_name_the_goal_or_claim() {
    let mut ws = workspace();
    let (goal, claim, party) = (GoalId::new_v7(), ClaimId::new_v7(), PartyId::new_v7());
    let prov = || Provenance::human("mira", 1_792_956_600_000);
    ws.apply(
        vec![
            Op::AddCybersecurityGoal {
                id: goal,
                name: "Sign firmware".into(),
                controls: vec![],
                provenance: prov(),
            },
            Op::AddCybersecurityClaim {
                id: claim,
                name: "Supplier signs".into(),
                provenance: prov(),
            },
            Op::AddParty {
                id: party,
                name: "Bosch".into(),
                kind: PartyKind::Supplier,
                provenance: prov(),
            },
        ],
        Origin::Edit,
    )
    .expect("applies");

    let cases = [
        (
            Edit::LinkGoalParty {
                goal,
                party,
                linked: true,
            },
            Selection::Goal(goal),
            "change responsible parties of cybersecurity goal 'Sign firmware'",
        ),
        (
            Edit::LinkGoalParty {
                goal,
                party,
                linked: false,
            },
            Selection::Goal(goal),
            "unlink responsible party from cybersecurity goal 'Sign firmware'",
        ),
        (
            Edit::LinkClaimParty {
                claim,
                party,
                linked: true,
            },
            Selection::Claim(claim),
            "change responsible parties of cybersecurity claim 'Supplier signs'",
        ),
        (
            Edit::LinkClaimParty {
                claim,
                party,
                linked: false,
            },
            Selection::Claim(claim),
            "unlink responsible party from cybersecurity claim 'Supplier signs'",
        ),
    ];
    for (edit, selection, expected) in &cases {
        apply(&mut ws, edit);
        let label = ws.undo_label().expect("label");
        assert_eq!(label.imperative(), *expected);
        assert!(label.imperative().contains("responsible"));
        assert_eq!(label.selection(ws.project()), Some(*selection));
    }
    for _ in 0..cases.len() {
        ws.undo().expect("undo");
    }
}
