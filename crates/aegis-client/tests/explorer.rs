//! The project explorer model: the tree, its folders and their counts
//! (182b).

use std::sync::Arc;

use aegis_client::commands::Edit;
use aegis_client::explorer::{
    ExplorerIndex, ExplorerKey, ExplorerRow, ExplorerState, Folder, Owner,
};
use aegis_client::i18n::{Catalog, Language};
use aegis_client::view::Selection;
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{
    AssetId, ClaimId, ComponentId, ControlId, DamageId, DataFlowId, ElementId, GoalId, InterfaceId,
    ItemRef, Methodology, Op, PartyId, PartyKind, Provenance, ThreatId, TreeId, TrustBoundaryId,
};

fn workspace() -> Workspace {
    Workspace::empty(Arc::new(Methodology::iso21434_r155().expect("builtin")))
}

fn run(ws: &mut Workspace, edit: &Edit) {
    let ops = edit.to_ops(ws.project(), "mira", 1).expect("builds");
    ws.apply(ops, Origin::Edit).expect("applies");
}

fn add_system(ws: &mut Workspace, name: &str) -> ElementId {
    let id = ElementId::new_v7();
    run(
        ws,
        &Edit::AddSystem {
            id,
            name: name.into(),
        },
    );
    id
}

fn add_component(
    ws: &mut Workspace,
    name: &str,
    parent: Option<ComponentId>,
    system: Option<ElementId>,
) -> ComponentId {
    let id = ComponentId::new_v7();
    run(
        ws,
        &Edit::AddComponent {
            id,
            name: name.into(),
            parent,
        },
    );
    if system.is_some() {
        run(
            ws,
            &Edit::MoveToSystem {
                components: vec![id],
                system,
            },
        );
    }
    id
}

fn add_asset(ws: &mut Workspace, name: &str, component: ComponentId) -> AssetId {
    let id = AssetId::new_v7();
    run(
        ws,
        &Edit::AddAssetBound {
            id,
            name: name.into(),
            properties: (0..6).collect(),
            component,
        },
    );
    id
}

fn add_threat(ws: &mut Workspace, name: &str, asset: AssetId, damage: DamageId) -> ThreatId {
    let id = ThreatId::new_v7();
    run(
        ws,
        &Edit::AddThreat {
            id,
            name: name.into(),
            asset,
            category: 0,
        },
    );
    run(
        ws,
        &Edit::LinkDamage {
            threat: id,
            damage,
            linked: true,
        },
    );
    id
}

#[allow(dead_code)] // used again by the 182b2 tests
struct Fx {
    ws: Workspace,
    brake: ElementId,
    adas: ElementId,
    caliper: ComponentId,
    pad: ComponentId,
    camera: ComponentId,
    loose: ComponentId,
    a1: AssetId,
    a3: AssetId,
    t1: ThreatId,
    t2: ThreatId,
    party: PartyId,
}

/// Brake (Caliper with child Pad), ADAS (Camera), Loose in no system; an
/// interface on Caliper; flow Camera -> Caliper; a boundary over Camera;
/// assets A1, A2, A3 on Caliper, Camera, Loose; threats T1 (A1) and T2 (A2)
/// both linked to D1; a tree for T1; a control applied to Caliper; a goal
/// listing that control and linked to T1; one unlinked claim; one party.
fn fixture() -> Fx {
    let mut ws = workspace();
    let brake = add_system(&mut ws, "Brake");
    let adas = add_system(&mut ws, "ADAS");
    let caliper = add_component(&mut ws, "Caliper", None, Some(brake));
    let pad = add_component(&mut ws, "Pad", Some(caliper), None);
    let camera = add_component(&mut ws, "Camera", None, Some(adas));
    let loose = add_component(&mut ws, "Loose", None, None);
    let port = InterfaceId::new_v7();
    run(
        &mut ws,
        &Edit::AddInterface {
            id: port,
            name: "Caliper port".into(),
            component: caliper,
            kind: 0,
        },
    );
    run(
        &mut ws,
        &Edit::AddDataFlow {
            id: DataFlowId::new_v7(),
            name: "Video".into(),
            from: camera,
            to: caliper,
            interface: None,
        },
    );
    let boundary = TrustBoundaryId::new_v7();
    run(
        &mut ws,
        &Edit::AddTrustBoundary {
            id: boundary,
            name: "Camera zone".into(),
        },
    );
    run(
        &mut ws,
        &Edit::SetTrustBoundaryMember {
            boundary,
            component: camera,
            member: true,
        },
    );
    let a1 = add_asset(&mut ws, "A1", caliper);
    let a2 = add_asset(&mut ws, "A2", camera);
    let a3 = add_asset(&mut ws, "A3", loose);
    let d1 = DamageId::new_v7();
    run(
        &mut ws,
        &Edit::AddDamage {
            id: d1,
            name: "D1".into(),
        },
    );
    let t1 = add_threat(&mut ws, "T1", a1, d1);
    let t2 = add_threat(&mut ws, "T2", a2, d1);
    run(
        &mut ws,
        &Edit::NewTree {
            id: TreeId::new_v7(),
            root: "Spoof caliper".into(),
            threat: Some(t1),
        },
    );
    let (control, goal) = (ControlId::new_v7(), GoalId::new_v7());
    run(
        &mut ws,
        &Edit::AddControl {
            id: control,
            name: "Signed frames".into(),
        },
    );
    run(
        &mut ws,
        &Edit::ApplyControl {
            control,
            target: ItemRef::Component(caliper),
        },
    );
    run(
        &mut ws,
        &Edit::AddGoal {
            id: goal,
            name: "Protect caliper".into(),
        },
    );
    run(
        &mut ws,
        &Edit::LinkGoalControl {
            goal,
            control,
            linked: true,
        },
    );
    run(
        &mut ws,
        &Edit::SetThreatGoal {
            threat: t1,
            goal: Some(goal),
        },
    );
    run(
        &mut ws,
        &Edit::AddClaim {
            id: ClaimId::new_v7(),
            name: "Accepted residual".into(),
        },
    );
    let party = PartyId::new_v7();
    ws.apply(
        vec![Op::AddParty {
            id: party,
            name: "Bosch".into(),
            kind: PartyKind::Supplier,
            provenance: Provenance::human("mira", 1),
        }],
        Origin::Edit,
    )
    .expect("applies");
    Fx {
        ws,
        brake,
        adas,
        caliper,
        pad,
        camera,
        loose,
        a1,
        a3,
        t1,
        t2,
        party,
    }
}

fn index(ws: &Workspace) -> ExplorerIndex {
    let catalog = Catalog::load(Language::En);
    let names = ws.project().methodology().element_names().clone();
    ExplorerIndex::build(ws, &names, &catalog, "Demo")
}

/// Every node expanded, so every row is listed.
fn all_rows(ix: &ExplorerIndex) -> Vec<ExplorerRow> {
    let mut state = ExplorerState::default();
    loop {
        let rows = ix.rows(&state);
        let before = state.expanded.len();
        state.expanded.extend(rows.iter().map(|r| r.key));
        if state.expanded.len() == before {
            return rows;
        }
    }
}

fn row(rows: &[ExplorerRow], key: ExplorerKey) -> &ExplorerRow {
    rows.iter()
        .find(|r| r.key == key)
        .unwrap_or_else(|| panic!("no row {key:?}"))
}

fn count(rows: &[ExplorerRow], owner: Owner, f: Folder) -> Option<usize> {
    row(rows, ExplorerKey::Folder(owner, f)).count
}

/// Asserts each `(folder, count)` of `owner`, naming the failing one.
fn assert_counts(rows: &[ExplorerRow], owner: Owner, expected: &[(Folder, Option<usize>)]) {
    for (f, n) in expected {
        assert_eq!(count(rows, owner, *f), *n, "{owner:?} {f:?}");
    }
}

#[test]
fn every_folder_has_the_hand_counted_number() {
    use Folder as F;
    let fx = fixture();
    let rows = all_rows(&index(&fx.ws));
    assert_counts(
        &rows,
        Owner::Project,
        &[
            (F::Overview, None),
            (F::ProjectDefinition, None),
            (F::Parties, Some(1)),
            (F::SharedAnalysis, None),
            (F::Damage, Some(1)),
            (F::Controls, Some(1)),
            (F::Goals, Some(1)),
            (F::Claims, Some(1)),
            (F::Systems, Some(2)),
        ],
    );
    // Brake: Caliper and its child Pad; the flow Camera -> Caliper counts
    // here and in ADAS.
    assert_counts(
        &rows,
        Owner::System(fx.brake),
        &[
            (F::ItemDefinition, Some(4)),
            (F::Components, Some(2)),
            (F::Interfaces, Some(1)),
            (F::DataFlows, Some(1)),
            (F::TrustBoundaries, Some(0)),
            (F::Assets, Some(1)),
            (F::Damage, Some(1)),
            (F::Threats, Some(1)),
            (F::Trees, Some(1)),
            (F::Controls, Some(1)),
            (F::Risks, Some(1)),
            (F::GoalsAndClaims, Some(1)),
        ],
    );
    // ADAS.
    assert_counts(
        &rows,
        Owner::System(fx.adas),
        &[
            (F::ItemDefinition, Some(3)),
            (F::Components, Some(1)),
            (F::Interfaces, Some(0)),
            (F::DataFlows, Some(1)),
            (F::TrustBoundaries, Some(1)),
            (F::Assets, Some(1)),
            (F::Damage, Some(1)),
            (F::Threats, Some(1)),
            (F::Trees, Some(0)),
            (F::Controls, Some(0)),
            (F::Risks, Some(1)),
            (F::GoalsAndClaims, Some(0)),
        ],
    );
    // Not in a system: Loose and A3. Shared kinds never sit here.
    assert_counts(
        &rows,
        Owner::Unassigned,
        &[
            (F::ItemDefinition, Some(1)),
            (F::Components, Some(1)),
            (F::Interfaces, Some(0)),
            (F::DataFlows, Some(0)),
            (F::TrustBoundaries, Some(0)),
            (F::Assets, Some(1)),
            (F::Damage, Some(0)),
            (F::Threats, Some(0)),
            (F::Trees, Some(0)),
            (F::Controls, Some(0)),
            (F::Risks, Some(0)),
            (F::GoalsAndClaims, Some(0)),
        ],
    );
    let brake = row(&rows, ExplorerKey::System(fx.brake));
    assert_eq!(
        (brake.label.as_str(), brake.system_slot),
        ("Brake", Some(1))
    );
    assert_eq!(
        row(&rows, ExplorerKey::System(fx.adas)).system_slot,
        Some(2)
    );
    assert!(
        row(
            &rows,
            ExplorerKey::Folder(Owner::System(fx.brake), F::Damage)
        )
        .linked
    );
    assert!(
        !row(
            &rows,
            ExplorerKey::Folder(Owner::System(fx.brake), F::Assets)
        )
        .linked
    );
    let t1 = ExplorerKey::Element(Owner::System(fx.brake), F::Risks, Selection::Threat(fx.t1));
    assert_eq!(
        (row(&rows, t1).label.as_str(), row(&rows, t1).icon),
        ("T00 T1", "file")
    );
}

#[test]
fn a_system_without_elements_keeps_every_folder_at_zero() {
    let mut fx = fixture();
    let gateway = add_system(&mut fx.ws, "Gateway");
    let rows = all_rows(&index(&fx.ws));
    let folders: Vec<Folder> = rows
        .iter()
        .filter_map(|r| match r.key {
            ExplorerKey::Folder(Owner::System(s), f) if s == gateway => Some(f),
            _ => None,
        })
        .collect();
    use Folder as F;
    assert_eq!(
        folders,
        [
            F::ItemDefinition,
            F::Components,
            F::Interfaces,
            F::DataFlows,
            F::TrustBoundaries,
            F::Assets,
            F::Damage,
            F::Threats,
            F::Trees,
            F::Controls,
            F::Risks,
            F::GoalsAndClaims,
            F::Problems,
        ]
    );
    for f in folders {
        assert_eq!(count(&rows, Owner::System(gateway), f), Some(0), "{f:?}");
    }
    assert_eq!(count(&rows, Owner::Project, F::Systems), Some(3));
    assert_eq!(
        row(&rows, ExplorerKey::System(gateway)).system_slot,
        Some(3)
    );
}

#[test]
fn not_in_a_system_goes_when_empty_and_comes_back_on_undo() {
    let mut fx = fixture();
    let has_unassigned = |ws: &Workspace| {
        all_rows(&index(ws))
            .iter()
            .any(|r| r.key == ExplorerKey::Unassigned)
    };
    assert!(has_unassigned(&fx.ws));
    // Moving Loose moves A3 with it: nothing is left in no system.
    run(
        &mut fx.ws,
        &Edit::MoveToSystem {
            components: vec![fx.loose],
            system: Some(fx.brake),
        },
    );
    assert!(!has_unassigned(&fx.ws));
    let rows = all_rows(&index(&fx.ws));
    assert_eq!(
        count(&rows, Owner::System(fx.brake), Folder::Components),
        Some(3)
    );
    assert_eq!(
        count(&rows, Owner::System(fx.brake), Folder::Assets),
        Some(2)
    );
    fx.ws.undo().expect("undo");
    assert!(has_unassigned(&fx.ws));
}
