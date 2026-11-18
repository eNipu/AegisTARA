//! The project explorer model: counts, marks, keyboard moves, the filter,
//! reveal and the drop rule (182b).

use std::sync::Arc;

use aegis_client::commands::Edit;
use aegis_client::explorer::{
    ExplorerIndex, ExplorerKey, ExplorerRow, ExplorerState, Folder, Mark, Owner, drop_edit,
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

fn saved(ws: &Workspace) -> Vec<u8> {
    let mut out = Vec::new();
    ws.project().save(&mut out).expect("save");
    out
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
    // here and in ADAS. Problems: five property warnings on A1, T1 without
    // treatment, T1 with a goal but not Reduce, the empty root gate of the
    // tree (error), D1 without impact = 9.
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
            (F::Problems, Some(9)),
        ],
    );
    // ADAS. Problems: five property warnings on A2, T2 without treatment,
    // T2 without feasibility (error), D1 without impact = 8.
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
            (F::Problems, Some(8)),
        ],
    );
    // Not in a system: Loose and A3. Problems: A3 has no threat, Loose is
    // in no system = 2. Shared kinds never sit here.
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
            (F::Problems, Some(2)),
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

#[test]
fn marks_climb_to_the_root_and_the_worst_wins() {
    use Folder as F;
    let fx = fixture();
    let rows = all_rows(&index(&fx.ws));
    let mark = |k| row(&rows, k).mark;
    let adas = Owner::System(fx.adas);
    // T2 has no feasibility: an error on ADAS > Threat scenarios.
    assert_eq!(mark(ExplorerKey::Folder(adas, F::Threats)), Mark::Error);
    assert_eq!(mark(ExplorerKey::Folder(adas, F::Problems)), Mark::Error);
    assert_eq!(mark(ExplorerKey::System(fx.adas)), Mark::Error);
    assert_eq!(
        mark(ExplorerKey::Folder(Owner::Project, F::Systems)),
        Mark::Error
    );
    assert_eq!(mark(ExplorerKey::Root), Mark::Error);
    // Loose is in no system: a warning on Not in a system > Components.
    let none = Owner::Unassigned;
    assert_eq!(
        mark(ExplorerKey::Folder(none, F::Components)),
        Mark::Warning
    );
    assert_eq!(
        mark(ExplorerKey::Folder(none, F::ItemDefinition)),
        Mark::Warning
    );
    assert_eq!(mark(ExplorerKey::Unassigned), Mark::Warning);
    // A shared damage scenario marks the project folder and the linked ones.
    assert_eq!(
        mark(ExplorerKey::Folder(Owner::Project, F::Damage)),
        Mark::Warning
    );
    assert_eq!(mark(ExplorerKey::Folder(adas, F::Damage)), Mark::Warning);
    assert_eq!(
        mark(ExplorerKey::Folder(Owner::Project, F::SharedAnalysis)),
        Mark::Warning
    );
    // No finding.
    assert_eq!(
        mark(ExplorerKey::Folder(Owner::System(fx.brake), F::Controls)),
        Mark::None
    );
    assert_eq!(
        mark(ExplorerKey::Folder(Owner::Project, F::Controls)),
        Mark::None
    );
    assert_eq!(
        mark(ExplorerKey::Folder(Owner::Project, F::Parties)),
        Mark::None
    );
    assert_eq!(mark(ExplorerKey::Folder(adas, F::Interfaces)), Mark::None);
}

#[test]
fn keyboard_walks_to_a_party_and_back() {
    use Folder as F;
    let fx = fixture();
    let ix = index(&fx.ws);
    let mut state = ExplorerState::default();
    let def = ExplorerKey::Folder(Owner::Project, F::ProjectDefinition);
    let parties = ExplorerKey::Folder(Owner::Project, F::Parties);
    let party = ExplorerKey::Element(Owner::Project, F::Parties, Selection::Party(fx.party));
    let overview = ExplorerKey::Folder(Owner::Project, F::Overview);
    type Step = fn(&mut ExplorerState, &[ExplorerRow]);
    let script: [(Step, ExplorerKey, bool); 14] = [
        (ExplorerState::down, overview, false),
        (ExplorerState::down, def, false),
        (ExplorerState::right, def, true),
        (ExplorerState::right, parties, false),
        (ExplorerState::right, parties, true),
        (ExplorerState::right, party, false),
        (ExplorerState::right, party, false),
        (ExplorerState::left, parties, true),
        (ExplorerState::left, parties, false),
        (ExplorerState::left, def, true),
        (ExplorerState::left, def, false),
        (ExplorerState::up, overview, false),
        (ExplorerState::up, ExplorerKey::Root, true),
        (ExplorerState::up, ExplorerKey::Root, true),
    ];
    for (n, (step, selected, expanded)) in script.into_iter().enumerate() {
        let rows = ix.rows(&state);
        step(&mut state, &rows);
        assert_eq!(state.selected, Some(selected), "step {n}");
        assert_eq!(state.expanded.contains(&selected), expanded, "step {n}");
    }
    // Parties was collapsed at step 8, the definition at step 10.
    assert!(!state.expanded.contains(&parties));
    assert!(!state.expanded.contains(&def));
    // With no selection, down selects the first row.
    state.selected = None;
    state.down(&ix.rows(&state));
    assert_eq!(state.selected, Some(ExplorerKey::Root));
    // The default state shows the root, the project folders, the systems.
    let keys: Vec<ExplorerKey> = ix
        .rows(&ExplorerState::default())
        .iter()
        .map(|r| r.key)
        .collect();
    assert_eq!(keys.len(), 12);
    assert_eq!(keys.last(), Some(&ExplorerKey::Unassigned));
    state.toggle(ExplorerKey::Root);
    assert_eq!(ix.rows(&state).len(), 1);
    state.select(party);
    assert_eq!(state.selected, Some(party));
}

#[test]
fn filter_shows_matches_and_their_ancestors() {
    use Folder as F;
    let fx = fixture();
    let ix = index(&fx.ws);
    let state = ExplorerState {
        filter: "T1".into(),
        ..ExplorerState::default()
    };
    let rows = ix.rows(&state);
    let brake = Owner::System(fx.brake);
    let t1 = Selection::Threat(fx.t1);
    let keys: Vec<ExplorerKey> = rows.iter().map(|r| r.key).collect();
    assert_eq!(
        keys,
        [
            ExplorerKey::Root,
            ExplorerKey::Folder(Owner::Project, F::Systems),
            ExplorerKey::System(fx.brake),
            ExplorerKey::Folder(brake, F::Threats),
            ExplorerKey::Element(brake, F::Threats, t1),
            ExplorerKey::Folder(brake, F::Risks),
            ExplorerKey::Element(brake, F::Risks, t1),
        ]
    );
    assert!(rows.iter().filter(|r| r.has_children).all(|r| r.expanded));
    // Matching is case-insensitive and also finds visual IDs.
    let lower = ExplorerState {
        filter: "a00".into(),
        ..ExplorerState::default()
    };
    let a1 = ExplorerKey::Element(brake, F::Assets, Selection::Asset(fx.a1));
    assert!(ix.rows(&lower).iter().any(|r| r.key == a1));
    let nothing = ExplorerState {
        filter: "zzz".into(),
        ..ExplorerState::default()
    };
    assert_eq!(ix.rows(&nothing).len(), 1);
}

#[test]
fn reveal_selects_the_threat_under_its_system() {
    use Folder as F;
    let fx = fixture();
    let ix = index(&fx.ws);
    let mut state = ExplorerState::default();
    state.reveal(&ix, Selection::Threat(fx.t2));
    let adas = Owner::System(fx.adas);
    let key = ExplorerKey::Element(adas, F::Threats, Selection::Threat(fx.t2));
    assert_eq!(state.selected, Some(key));
    for k in [
        ExplorerKey::Folder(adas, F::Threats),
        ExplorerKey::System(fx.adas),
        ExplorerKey::Folder(Owner::Project, F::Systems),
        ExplorerKey::Root,
    ] {
        assert!(state.expanded.contains(&k), "{k:?}");
    }
    assert!(ix.rows(&state).iter().any(|r| r.key == key));
}

#[test]
fn dropping_a_component_on_a_system_moves_it_and_undo_restores() {
    use Folder as F;
    let mut fx = fixture();
    let (brake, adas) = (Owner::System(fx.brake), Owner::System(fx.adas));
    let camera = ExplorerKey::Element(adas, F::Components, Selection::Component(fx.camera));
    let p = fx.ws.project();
    let edit = drop_edit(p, &camera, &ExplorerKey::System(fx.brake));
    assert_eq!(
        edit,
        Some(Edit::MoveToSystem {
            components: vec![fx.camera],
            system: Some(fx.brake),
        })
    );
    // Onto a folder of a system and onto Not in a system.
    let onto_folder = drop_edit(p, &camera, &ExplorerKey::Folder(brake, F::Threats));
    assert_eq!(onto_folder, edit);
    assert_eq!(
        drop_edit(p, &camera, &ExplorerKey::Unassigned),
        Some(Edit::MoveToSystem {
            components: vec![fx.camera],
            system: None,
        })
    );
    // Every other pair.
    let pad = ExplorerKey::Element(brake, F::Components, Selection::Component(fx.pad));
    let a1 = ExplorerKey::Element(brake, F::Assets, Selection::Asset(fx.a1));
    let loose = ExplorerKey::Element(
        Owner::Unassigned,
        F::Components,
        Selection::Component(fx.loose),
    );
    assert_eq!(drop_edit(p, &pad, &ExplorerKey::System(fx.adas)), None);
    assert_eq!(drop_edit(p, &a1, &ExplorerKey::System(fx.adas)), None);
    assert_eq!(drop_edit(p, &camera, &ExplorerKey::System(fx.adas)), None);
    assert_eq!(
        drop_edit(p, &camera, &ExplorerKey::Folder(adas, F::Assets)),
        None
    );
    assert_eq!(
        drop_edit(p, &camera, &ExplorerKey::Folder(Owner::Project, F::Damage)),
        None
    );
    assert_eq!(
        drop_edit(
            p,
            &loose,
            &ExplorerKey::Folder(Owner::Unassigned, F::Assets)
        ),
        None
    );
    let a3 = ExplorerKey::Element(Owner::Unassigned, F::Assets, Selection::Asset(fx.a3));
    assert_eq!(drop_edit(p, &a3, &ExplorerKey::System(fx.brake)), None);
    let caliper = ExplorerKey::Element(brake, F::Components, Selection::Component(fx.caliper));
    assert!(drop_edit(p, &caliper, &ExplorerKey::Folder(adas, F::Problems)).is_some());

    let before = saved(&fx.ws);
    let ops = edit
        .expect("edit")
        .to_ops(fx.ws.project(), "mira", 2)
        .expect("ops");
    fx.ws.apply(ops, Origin::Edit).expect("applies");
    let p = fx.ws.project();
    let moved = p.component_idx(&fx.camera).and_then(|i| p.component(i));
    assert_eq!(moved.and_then(|c| c.system()), Some(fx.brake));
    let rows = all_rows(&index(&fx.ws));
    assert_eq!(count(&rows, brake, F::Components), Some(3));
    assert_eq!(count(&rows, adas, F::Components), Some(0));
    assert_eq!(count(&rows, brake, F::Assets), Some(2));
    assert_eq!(count(&rows, adas, F::Assets), Some(0));
    fx.ws.undo().expect("undo");
    assert_eq!(
        saved(&fx.ws),
        before,
        "undo did not restore the saved bytes"
    );
}
