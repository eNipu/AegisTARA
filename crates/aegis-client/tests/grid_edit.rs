//! Integration tests for `Edit::SetAssetProperties` and the `aegis_client::grid`
//! core: `GridError`, `GridSelection`, `add_row_edit`, and `delete_rows_edit` (189b1).

use std::sync::Arc;

use aegis_client::commands::{Edit, EditError};
use aegis_client::explorer::Owner;
use aegis_client::grid::{GridError, GridSelection, add_row_edit, delete_rows_edit};
use aegis_client::undo_label::Label;
use aegis_client::view::{Selection, TableId};
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{AssetId, ComponentId, ElementId, Methodology, PartyId, Project, PropertySet};

const ACTOR: &str = "mira";
const NOW_MS: u64 = 1_792_956_600_000;

fn empty_workspace() -> Workspace {
    let m = Arc::new(Methodology::iso21434_r155().expect("iso21434 methodology"));
    Workspace::new(Project::new(m), 0)
}

fn save_bytes(ws: &Workspace) -> Vec<u8> {
    let mut out = Vec::new();
    ws.project().save(&mut out).expect("project saves");
    out
}

fn apply_edit(ws: &mut Workspace, edit: &Edit) {
    let ops = edit
        .to_ops(ws.project(), ACTOR, NOW_MS)
        .expect("edit builds ops");
    ws.apply(ops, Origin::Edit).expect("ops apply");
}

fn add_system(ws: &mut Workspace, name: &str) -> ElementId {
    let id = ElementId::new_v7();
    apply_edit(
        ws,
        &Edit::AddSystem {
            id,
            name: name.into(),
        },
    );
    id
}

fn add_component(ws: &mut Workspace, name: &str, system: Option<ElementId>) -> ComponentId {
    let id = ComponentId::new_v7();
    apply_edit(
        ws,
        &Edit::AddComponent {
            id,
            name: name.into(),
            parent: None,
        },
    );
    if system.is_some() {
        apply_edit(
            ws,
            &Edit::MoveToSystem {
                components: vec![id],
                system,
            },
        );
    }
    id
}

fn add_asset(ws: &mut Workspace, name: &str, props: Vec<usize>) -> AssetId {
    let id = AssetId::new_v7();
    apply_edit(
        ws,
        &Edit::AddAsset {
            id,
            name: name.into(),
            properties: props,
        },
    );
    id
}

fn prop_set(project: &Project, indices: &[usize]) -> PropertySet {
    let m = project.methodology();
    indices.iter().fold(PropertySet::default(), |acc, &i| {
        acc.with(m.properties().nth(i).expect("valid property index"))
    })
}

fn asset_props(project: &Project, id: AssetId) -> PropertySet {
    let idx = project.asset_idx(&id).expect("asset exists");
    project.asset(idx).expect("asset slot").properties()
}

#[test]
fn set_asset_properties_batch_replaces_and_undoes_in_one_step() {
    let mut ws = empty_workspace();
    let a1 = add_asset(&mut ws, "CAN Bus", vec![0]);
    let a2 = add_asset(&mut ws, "Key Store", vec![1, 2]);
    let before = save_bytes(&ws);

    let batch = Edit::Batch(vec![
        Edit::SetAssetProperties {
            asset: a1,
            properties: vec![1, 2],
        },
        Edit::SetAssetProperties {
            asset: a2,
            properties: vec![0, 3],
        },
    ]);
    assert_eq!(
        batch.bulk_label(),
        Some(Label::bulk("change properties of", "assets", 2))
    );

    apply_edit(&mut ws, &batch);
    assert_eq!(
        asset_props(ws.project(), a1),
        prop_set(ws.project(), &[1, 2])
    );
    assert_eq!(
        asset_props(ws.project(), a2),
        prop_set(ws.project(), &[0, 3])
    );
    assert_ne!(save_bytes(&ws), before);

    ws.undo().expect("single undo succeeds");
    assert_eq!(save_bytes(&ws), before);
}

#[test]
fn set_asset_properties_rejects_empty_and_out_of_range() {
    let mut ws = empty_workspace();
    let a1 = add_asset(&mut ws, "Firmware", vec![0]);
    let prop_count = ws.project().methodology().properties().count();

    let empty = Edit::SetAssetProperties {
        asset: a1,
        properties: vec![],
    };
    assert_eq!(
        empty.to_ops(ws.project(), ACTOR, NOW_MS),
        Err(EditError::NoProperties)
    );

    let oob = Edit::SetAssetProperties {
        asset: a1,
        properties: vec![0, prop_count],
    };
    assert_eq!(
        oob.to_ops(ws.project(), ACTOR, NOW_MS),
        Err(EditError::UnknownChoice)
    );
}

#[test]
fn set_asset_properties_unchanged_filters_noops_and_keeps_errors() {
    let mut ws = empty_workspace();
    let a1 = add_asset(&mut ws, "ECU A", vec![0, 2]);
    let a2 = add_asset(&mut ws, "ECU B", vec![1]);
    let prop_count = ws.project().methodology().properties().count();

    // Order and duplicates produce the same PropertySet -> unchanged.
    let same_a1 = Edit::SetAssetProperties {
        asset: a1,
        properties: vec![2, 0, 2],
    };
    let diff_a2 = Edit::SetAssetProperties {
        asset: a2,
        properties: vec![0, 1],
    };
    let filtered = Edit::Batch(vec![same_a1.clone(), diff_a2.clone()]).changing(ws.project());
    assert_eq!(filtered, diff_a2);

    // Pure no-op batch empties out.
    let noop = Edit::Batch(vec![same_a1]).changing(ws.project());
    assert_eq!(noop, Edit::Batch(vec![]));

    // Out-of-range, empty, and missing-asset edits are NOT filtered as unchanged
    // so `to_ops` / `apply` can surface the error.
    let bad_oob = Edit::SetAssetProperties {
        asset: a1,
        properties: vec![prop_count],
    };
    assert_eq!(
        Edit::Batch(vec![bad_oob.clone()]).changing(ws.project()),
        bad_oob
    );

    let bad_empty = Edit::SetAssetProperties {
        asset: a1,
        properties: vec![],
    };
    assert_eq!(
        Edit::Batch(vec![bad_empty.clone()]).changing(ws.project()),
        bad_empty
    );

    let missing = Edit::SetAssetProperties {
        asset: AssetId::new_v7(),
        properties: vec![0],
    };
    assert_eq!(
        Edit::Batch(vec![missing.clone()]).changing(ws.project()),
        missing
    );
}

#[test]
fn grid_selection_single_extend_ranges_and_contains() {
    let mut sel = GridSelection::single(2, 3);
    assert_eq!(sel.anchor_row, 2);
    assert_eq!(sel.anchor_col, 3);
    assert_eq!(sel.active_row, 2);
    assert_eq!(sel.active_col, 3);
    assert_eq!(sel.row_range(), 2..=2);
    assert_eq!(sel.col_range(), 3..=3);
    assert_eq!(sel.row_count(), 1);
    assert!(sel.contains(2, 3));
    assert!(!sel.contains(2, 4));
    assert!(!sel.contains(1, 3));

    // Extend upward and rightward (active_row < anchor_row, active_col > anchor_col).
    sel.extend_to(0, 5);
    assert_eq!(sel.anchor_row, 2);
    assert_eq!(sel.anchor_col, 3);
    assert_eq!(sel.active_row, 0);
    assert_eq!(sel.active_col, 5);
    assert_eq!(sel.row_range(), 0..=2);
    assert_eq!(sel.col_range(), 3..=5);
    assert_eq!(sel.row_count(), 3);
    assert!(sel.contains(0, 3));
    assert!(sel.contains(1, 4));
    assert!(sel.contains(2, 5));
    assert!(!sel.contains(3, 4));
    assert!(!sel.contains(1, 2));
}

#[test]
fn grid_selection_move_by_and_clamp() {
    let mut sel = GridSelection::single(1, 1);

    // Move without extending updates both anchor and active, saturating at bounds.
    sel.move_by(-10, 10, 4, 3, false);
    assert_eq!(sel, GridSelection::single(0, 2));

    // Extreme signed deltas saturate safely without overflow.
    sel.move_by(isize::MAX, isize::MIN, 4, 3, false);
    assert_eq!(sel, GridSelection::single(3, 0));

    // Move with extend keeps anchor fixed and moves active.
    sel.move_by(-2, 2, 4, 3, true);
    assert_eq!(
        sel,
        GridSelection {
            anchor_row: 3,
            anchor_col: 0,
            active_row: 1,
            active_col: 2,
        }
    );
    assert_eq!(sel.row_range(), 1..=3);
    assert_eq!(sel.col_range(), 0..=2);
    assert_eq!(sel.row_count(), 3);

    // Zero dimensions: move_by is a strict no-op.
    let snapshot = sel;
    sel.move_by(1, 1, 0, 3, false);
    assert_eq!(sel, snapshot);
    sel.move_by(1, 1, 4, 0, true);
    assert_eq!(sel, snapshot);

    // Clamp pulls both corners into bounds.
    let mut wide = GridSelection {
        anchor_row: 5,
        anchor_col: 8,
        active_row: 2,
        active_col: 6,
    };
    wide.clamp(4, 3);
    assert_eq!(
        wide,
        GridSelection {
            anchor_row: 3,
            anchor_col: 2,
            active_row: 2,
            active_col: 2,
        }
    );

    // Clamp with zero dimensions collapses that axis to 0.
    wide.clamp(0, 0);
    assert_eq!(wide, GridSelection::single(0, 0));
}

#[test]
fn add_row_edit_guards_return_exact_errors() {
    let mut ws = empty_workspace();

    // Non-empty valid name on Problems -> only NotEditable guard triggers.
    assert_eq!(
        add_row_edit(ws.project(), TableId::Problems, Owner::Project, "Valid"),
        Err(GridError::NotEditable)
    );

    // Editable table with blank/whitespace name -> only EmptyName guard triggers.
    assert_eq!(
        add_row_edit(ws.project(), TableId::Assets, Owner::Project, "   \t "),
        Err(GridError::EmptyName { row: 1 })
    );
    assert_eq!(
        add_row_edit(ws.project(), TableId::Damage, Owner::Project, ""),
        Err(GridError::EmptyName { row: 1 })
    );

    // Valid name on Threats/Risks in a project with no assets -> NoAssetForThreat.
    assert_eq!(
        add_row_edit(ws.project(), TableId::Threats, Owner::Project, "Spoofing"),
        Err(GridError::NoAssetForThreat)
    );
    assert_eq!(
        add_row_edit(ws.project(), TableId::Risks, Owner::Project, "Tampering"),
        Err(GridError::NoAssetForThreat)
    );

    // System with no assets returns NoAssetForThreat even when another system has one.
    let sys_a = add_system(&mut ws, "Brakes");
    let sys_b = add_system(&mut ws, "Infotainment");
    let comp_a = add_component(&mut ws, "Brake ECU", Some(sys_a));
    let id = AssetId::new_v7();
    apply_edit(
        &mut ws,
        &Edit::AddAssetBound {
            id,
            name: "Brake Bus".into(),
            properties: vec![0],
            component: comp_a,
        },
    );
    assert_eq!(
        add_row_edit(ws.project(), TableId::Threats, Owner::System(sys_b), "DoS"),
        Err(GridError::NoAssetForThreat)
    );
}

#[test]
fn add_and_delete_row_edits_cover_all_editable_tables_with_exact_undo() {
    let mut ws = empty_workspace();
    let sys_with_comp = add_system(&mut ws, "Powertrain");
    let sys_empty = add_system(&mut ws, "EmptySys");
    let comp = add_component(&mut ws, "Engine ECU", Some(sys_with_comp));

    // 1. Assets under Owner::Project -> AddAsset (unbound).
    let before = save_bytes(&ws);
    let (edit, sel_proj_asset) = add_row_edit(
        ws.project(),
        TableId::Assets,
        Owner::Project,
        "  Project Asset  ",
    )
    .expect("project asset edit");
    assert!(matches!(edit, Edit::AddAsset { .. }));
    let Selection::Asset(proj_asset_id) = sel_proj_asset else {
        panic!("expected Selection::Asset");
    };
    apply_edit(&mut ws, &edit);
    let idx = ws.project().asset_idx(&proj_asset_id).expect("asset added");
    let asset = ws.project().asset(idx).expect("asset slot");
    assert_eq!(asset.name(), "Project Asset");
    assert_eq!(asset.component(), None);
    assert_eq!(asset.properties(), prop_set(ws.project(), &[0]));
    ws.undo().expect("undo project asset");
    assert_eq!(save_bytes(&ws), before);

    // 2. Assets under Owner::System without components -> falls back to AddAsset.
    let before = save_bytes(&ws);
    let (edit, sel_empty_sys_asset) = add_row_edit(
        ws.project(),
        TableId::Assets,
        Owner::System(sys_empty),
        "Fallback Asset",
    )
    .expect("fallback asset edit");
    assert!(matches!(edit, Edit::AddAsset { .. }));
    assert!(matches!(sel_empty_sys_asset, Selection::Asset(_)));
    apply_edit(&mut ws, &edit);
    ws.undo().expect("undo fallback asset");
    assert_eq!(save_bytes(&ws), before);

    // 3. Assets under Owner::System with component -> AddAssetBound to first component.
    let before = save_bytes(&ws);
    let (edit, sel_sys_asset) = add_row_edit(
        ws.project(),
        TableId::Assets,
        Owner::System(sys_with_comp),
        "Bound Asset",
    )
    .expect("system asset edit");
    assert!(matches!(
        edit,
        Edit::AddAssetBound { component, .. } if component == comp
    ));
    let Selection::Asset(sys_asset_id) = sel_sys_asset else {
        panic!("expected Selection::Asset");
    };
    apply_edit(&mut ws, &edit);
    let idx = ws.project().asset_idx(&sys_asset_id).expect("bound asset");
    assert_eq!(
        ws.project().asset(idx).expect("slot").component(),
        Some(ws.project().component_idx(&comp).expect("comp idx"))
    );
    ws.undo().expect("undo bound asset");
    assert_eq!(save_bytes(&ws), before);

    // Re-add the bound asset so Threats/Risks have an asset in sys_with_comp & project.
    apply_edit(&mut ws, &edit);

    // 4-9. Damage, Threats (Project + System), Risks, Controls, Goals, Claims.
    let cases = [
        (TableId::Damage, Owner::Project, "Loss of Braking"),
        (TableId::Threats, Owner::Project, "Replay Message"),
        (TableId::Threats, Owner::System(sys_with_comp), "Spoof CAN"),
        (TableId::Risks, Owner::Project, "Tamper Calibration"),
        (TableId::Controls, Owner::Project, "SecOC MAC"),
        (TableId::Goals, Owner::Project, "Ensure Integrity"),
        (TableId::Claims, Owner::Project, "Gateway Isolated"),
    ];

    let mut created_for_delete = Vec::new();
    for (table, owner, name) in cases {
        let before = save_bytes(&ws);
        let (edit, sel) = add_row_edit(ws.project(), table, owner, name).expect("add row edit");
        match table {
            TableId::Damage => assert!(matches!(sel, Selection::Damage(_))),
            TableId::Threats | TableId::Risks => {
                assert!(matches!(sel, Selection::Threat(_)));
                assert!(matches!(
                    edit,
                    Edit::AddThreat { asset, .. } if asset == sys_asset_id
                ));
            }
            TableId::Controls => assert!(matches!(sel, Selection::Control(_))),
            TableId::Goals => assert!(matches!(sel, Selection::Goal(_))),
            TableId::Claims => assert!(matches!(sel, Selection::Claim(_))),
            TableId::Assets | TableId::Problems => unreachable!(),
        }
        apply_edit(&mut ws, &edit);
        assert_ne!(save_bytes(&ws), before);
        ws.undo().expect("single undo restores add");
        assert_eq!(save_bytes(&ws), before);

        // Re-apply so we can verify delete_rows_edit against real elements.
        apply_edit(&mut ws, &edit);
        created_for_delete.push(sel);
    }

    // Also create a standalone asset with no threats targeting it to test Asset removal.
    let (standalone_edit, sel_standalone_asset) =
        add_row_edit(ws.project(), TableId::Assets, Owner::Project, "Temp Asset")
            .expect("standalone asset");
    apply_edit(&mut ws, &standalone_edit);

    // delete_rows_edit edge cases: empty, non-removable, single (with dedup).
    assert_eq!(delete_rows_edit(&[]), None);
    assert_eq!(
        delete_rows_edit(&[
            Selection::Component(comp),
            Selection::Party(PartyId::new_v7())
        ]),
        None
    );

    let single = delete_rows_edit(&[
        sel_standalone_asset,
        Selection::Component(comp),
        sel_standalone_asset,
    ]);
    assert!(matches!(single, Some(Edit::RemoveAsset { .. })));

    // Delete standalone asset in one step and undo cleanly.
    let before_single = save_bytes(&ws);
    apply_edit(&mut ws, &single.expect("single delete edit"));
    assert_ne!(save_bytes(&ws), before_single);
    ws.undo().expect("undo single delete");
    assert_eq!(save_bytes(&ws), before_single);

    // Multi-row batch delete across Damage, Threats, Risks, Controls, Goals, Claims, and Asset
    // (with duplicates and a non-removable selection interleaved).
    let mut batch_targets = Vec::new();
    batch_targets.push(Selection::Component(comp));
    batch_targets.extend_from_slice(&created_for_delete);
    batch_targets.push(created_for_delete[0]); // duplicate
    batch_targets.push(sel_standalone_asset);

    let batch_delete = delete_rows_edit(&batch_targets).expect("batch delete edit");
    let Edit::Batch(ref items) = batch_delete else {
        panic!("expected Edit::Batch for multiple deletions");
    };
    assert_eq!(items.len(), created_for_delete.len() + 1);

    let before_batch = save_bytes(&ws);
    apply_edit(&mut ws, &batch_delete);
    assert_ne!(save_bytes(&ws), before_batch);

    // Verify all deleted rows are gone from project.
    assert!(
        ws.project()
            .asset_idx(&match sel_standalone_asset {
                Selection::Asset(id) => id,
                _ => unreachable!(),
            })
            .is_none()
    );
    assert_eq!(ws.project().damage_scenarios().count(), 0);
    assert_eq!(ws.project().threat_scenarios().count(), 0);
    assert_eq!(ws.project().security_controls().count(), 0);
    assert_eq!(ws.project().cybersecurity_goals().count(), 0);
    assert_eq!(ws.project().cybersecurity_claims().count(), 0);

    // Single undo restores the entire batch byte-for-byte.
    ws.undo().expect("undo batch delete");
    assert_eq!(save_bytes(&ws), before_batch);
}

#[test]
fn add_row_edit_picks_compatible_category_and_respects_unassigned_owner() {
    let mut ws = empty_workspace();
    let sys = add_system(&mut ws, "Chassis");
    let comp = add_component(&mut ws, "ABS ECU", Some(sys));

    // System-bound asset with all properties supports category 0.
    let bound_id = AssetId::new_v7();
    apply_edit(
        &mut ws,
        &Edit::AddAssetBound {
            id: bound_id,
            name: "ABS Sensor".into(),
            properties: (0..6).collect(),
            component: comp,
        },
    );
    let (edit, _) = add_row_edit(
        ws.project(),
        TableId::Threats,
        Owner::System(sys),
        "Spoof Wheel Speed",
    )
    .expect("threat on full-property asset");
    assert!(matches!(
        edit,
        Edit::AddThreat {
            asset,
            category: 0,
            ..
        } if asset == bound_id
    ));
    apply_edit(&mut ws, &edit);

    // Owner::Unassigned has no unassigned asset yet -> NoAssetForThreat.
    assert_eq!(
        add_row_edit(ws.project(), TableId::Threats, Owner::Unassigned, "Leak"),
        Err(GridError::NoAssetForThreat)
    );

    // Add an unassigned asset via add_row_edit (gets properties: vec![0]).
    let (asset_edit, sel) = add_row_edit(
        ws.project(),
        TableId::Assets,
        Owner::Unassigned,
        "Loose Asset",
    )
    .expect("unassigned asset");
    let Selection::Asset(unassigned_id) = sel else {
        panic!("expected Selection::Asset");
    };
    apply_edit(&mut ws, &asset_edit);

    // Owner::Unassigned threat targets the unassigned asset with a compatible category.
    let (threat_edit, _) = add_row_edit(
        ws.project(),
        TableId::Threats,
        Owner::Unassigned,
        "Disclose Loose Asset",
    )
    .expect("threat on unassigned asset");
    assert!(matches!(
        threat_edit,
        Edit::AddThreat { asset, .. } if asset == unassigned_id
    ));
    apply_edit(&mut ws, &threat_edit);
}
