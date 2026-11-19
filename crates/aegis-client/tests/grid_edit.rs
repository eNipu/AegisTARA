//! Integration tests for `Edit::SetAssetProperties` and the `aegis_client::grid`
//! core: `GridError`, `GridSelection`, `add_row_edit`, and `delete_rows_edit` (189b1).

use std::sync::Arc;

use aegis_client::commands::{Edit, EditError, TREATMENTS};
use aegis_client::explorer::Owner;
use aegis_client::grid::{
    CellEditorKind, CellInput, GridError, GridSelection, add_row_edit, cell_editor,
    delete_rows_edit, edit_cell, edit_cells,
};
use aegis_client::i18n::{Catalog, Language};
use aegis_client::pages::vocabulary;
use aegis_client::relations::RelationKey;
use aegis_client::undo_label::Label;
use aegis_client::view::{ColumnSpec, Selection, TableId};
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{
    AssetId, ComponentId, DamageId, ElementId, Methodology, PartyId, Project, PropertySet,
    RiskTreatment, ThreatId,
};

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

// ---- 189b2: cell editor descriptors, inline cell edit, and multi-cell edit ----

fn col_by_key(table: TableId, key: &str) -> &'static ColumnSpec {
    table
        .columns()
        .iter()
        .find(|c| c.key == key)
        .unwrap_or_else(|| panic!("column '{key}' not found on {table:?}"))
}

fn flags_for(count: usize, on: &[usize]) -> Vec<bool> {
    let mut flags = vec![false; count];
    for &ix in on {
        flags[ix] = true;
    }
    flags
}

fn add_damage(ws: &mut Workspace, name: &str) -> Selection {
    let (edit, sel) =
        add_row_edit(ws.project(), TableId::Damage, Owner::Project, name).expect("add damage");
    apply_edit(ws, &edit);
    sel
}

fn add_threat(ws: &mut Workspace, name: &str) -> Selection {
    let (edit, sel) =
        add_row_edit(ws.project(), TableId::Threats, Owner::Project, name).expect("add threat");
    apply_edit(ws, &edit);
    sel
}

fn add_control(ws: &mut Workspace, name: &str) -> Selection {
    let (edit, sel) =
        add_row_edit(ws.project(), TableId::Controls, Owner::Project, name).expect("add control");
    apply_edit(ws, &edit);
    sel
}

fn add_goal(ws: &mut Workspace, name: &str) -> Selection {
    let (edit, sel) =
        add_row_edit(ws.project(), TableId::Goals, Owner::Project, name).expect("add goal");
    apply_edit(ws, &edit);
    sel
}

fn add_claim(ws: &mut Workspace, name: &str) -> Selection {
    let (edit, sel) =
        add_row_edit(ws.project(), TableId::Claims, Owner::Project, name).expect("add claim");
    apply_edit(ws, &edit);
    sel
}

#[test]
fn cell_editor_and_inline_edit_cover_all_seven_editable_tables() {
    let mut ws = empty_workspace();
    let catalog = Catalog::load(Language::En);
    let vocab = vocabulary(ws.project().methodology());
    let prop_count = vocab.properties.len();
    let impact_count = vocab.impact_categories.len();

    let asset_id = add_asset(&mut ws, "CAN Bus", vec![0]);
    let sel_asset = Selection::Asset(asset_id);
    let sel_damage = add_damage(&mut ws, "Loss of Braking");
    let sel_control = add_control(&mut ws, "SecOC");
    let sel_goal = add_goal(&mut ws, "Protect Braking");
    let sel_claim = add_claim(&mut ws, "Gateway Isolated");

    let Selection::Damage(damage_id) = sel_damage else {
        unreachable!()
    };
    let Selection::Control(control_id) = sel_control else {
        unreachable!()
    };
    let Selection::Goal(goal_id) = sel_goal else {
        unreachable!()
    };
    let Selection::Claim(claim_id) = sel_claim else {
        unreachable!()
    };

    // 1. Assets: name, component (LinkPicker), properties (MultiChoice).
    let asset_name_col = col_by_key(TableId::Assets, "name");
    assert_eq!(
        cell_editor(
            ws.project(),
            TableId::Assets,
            Some(sel_asset),
            asset_name_col,
            &vocab,
            &catalog
        ),
        CellEditorKind::Text {
            current: "CAN Bus".into()
        }
    );
    let before = save_bytes(&ws);
    let edit = edit_cell(
        ws.project(),
        TableId::Assets,
        sel_asset,
        asset_name_col,
        &CellInput::Text("  CAN-FD Bus  ".into()),
    )
    .expect("rename asset");
    assert!(
        matches!(edit, Edit::RenameAsset { asset, ref name } if asset == asset_id && name == "CAN-FD Bus")
    );
    apply_edit(&mut ws, &edit);
    assert_eq!(
        ws.project()
            .asset(ws.project().asset_idx(&asset_id).expect("idx"))
            .expect("slot")
            .name(),
        "CAN-FD Bus"
    );
    ws.undo().expect("undo rename asset");
    assert_eq!(save_bytes(&ws), before);

    assert_eq!(
        cell_editor(
            ws.project(),
            TableId::Assets,
            Some(sel_asset),
            col_by_key(TableId::Assets, "component"),
            &vocab,
            &catalog
        ),
        CellEditorKind::LinkPicker {
            relation: RelationKey::ASSET_COMPONENT
        }
    );

    let prop_col = col_by_key(TableId::Assets, "properties");
    let expected_flags: Vec<(String, bool)> = vocab
        .properties
        .iter()
        .enumerate()
        .map(|(i, label)| (label.clone(), i == 0))
        .collect();
    assert_eq!(
        cell_editor(
            ws.project(),
            TableId::Assets,
            Some(sel_asset),
            prop_col,
            &vocab,
            &catalog
        ),
        CellEditorKind::MultiChoice {
            options: expected_flags
        }
    );
    let before = save_bytes(&ws);
    let edit = edit_cell(
        ws.project(),
        TableId::Assets,
        sel_asset,
        prop_col,
        &CellInput::MultiChoice(flags_for(prop_count, &[1, 2])),
    )
    .expect("set asset properties");
    assert!(matches!(edit, Edit::SetAssetProperties { .. }));
    apply_edit(&mut ws, &edit);
    assert_eq!(
        asset_props(ws.project(), asset_id),
        prop_set(ws.project(), &[1, 2])
    );
    ws.undo().expect("undo set asset properties");
    assert_eq!(save_bytes(&ws), before);

    // 2. Damage: name, impact (single Impact + full Impacts).
    let damage_name_col = col_by_key(TableId::Damage, "name");
    assert_eq!(
        cell_editor(
            ws.project(),
            TableId::Damage,
            Some(sel_damage),
            damage_name_col,
            &vocab,
            &catalog
        ),
        CellEditorKind::Text {
            current: "Loss of Braking".into()
        }
    );
    let before = save_bytes(&ws);
    let edit = edit_cell(
        ws.project(),
        TableId::Damage,
        sel_damage,
        damage_name_col,
        &CellInput::Text("Delayed Braking".into()),
    )
    .expect("rename damage");
    assert!(matches!(edit, Edit::RenameDamage { damage, .. } if damage == damage_id));
    apply_edit(&mut ws, &edit);
    assert_eq!(
        ws.project()
            .damage_scenario(ws.project().damage_scenario_idx(&damage_id).expect("idx"))
            .expect("slot")
            .name(),
        "Delayed Braking"
    );
    ws.undo().expect("undo rename damage");
    assert_eq!(save_bytes(&ws), before);

    let impact_col = col_by_key(TableId::Damage, "impact");
    let expected_cats: Vec<(String, Vec<String>, Option<usize>)> = vocab
        .impact_categories
        .iter()
        .map(|c| (c.label.clone(), c.levels.clone(), None))
        .collect();
    assert_eq!(
        cell_editor(
            ws.project(),
            TableId::Damage,
            Some(sel_damage),
            impact_col,
            &vocab,
            &catalog
        ),
        CellEditorKind::ImpactRatings {
            categories: expected_cats
        }
    );

    // Single category impact edit + verify cell_editor reflects selected level + exact undo.
    let before = save_bytes(&ws);
    let edit = edit_cell(
        ws.project(),
        TableId::Damage,
        sel_damage,
        impact_col,
        &CellInput::Impact {
            category: 0,
            level: Some(2),
        },
    )
    .expect("single impact edit");
    assert!(matches!(
        edit,
        Edit::SetImpact {
            damage,
            category: 0,
            level: Some(2)
        } if damage == damage_id
    ));
    apply_edit(&mut ws, &edit);
    let CellEditorKind::ImpactRatings { categories } = cell_editor(
        ws.project(),
        TableId::Damage,
        Some(sel_damage),
        impact_col,
        &vocab,
        &catalog,
    ) else {
        panic!("expected ImpactRatings");
    };
    assert_eq!(categories[0].2, Some(2));
    assert_eq!(categories[1].2, None);
    ws.undo().expect("undo single impact");
    assert_eq!(save_bytes(&ws), before);

    // Atomic Impacts vector edit + exact undo.
    let mut levels = vec![None; impact_count];
    levels[0] = Some(3);
    levels[1] = Some(1);
    let before = save_bytes(&ws);
    let edit = edit_cell(
        ws.project(),
        TableId::Damage,
        sel_damage,
        impact_col,
        &CellInput::Impacts(levels.clone()),
    )
    .expect("impacts batch edit");
    apply_edit(&mut ws, &edit);
    let CellEditorKind::ImpactRatings { categories } = cell_editor(
        ws.project(),
        TableId::Damage,
        Some(sel_damage),
        impact_col,
        &vocab,
        &catalog,
    ) else {
        panic!("expected ImpactRatings");
    };
    assert_eq!(categories[0].2, Some(3));
    assert_eq!(categories[1].2, Some(1));
    ws.undo().expect("undo impacts batch");
    assert_eq!(save_bytes(&ws), before);

    // 3. Threats: name, scales (preserving existing scaling_reason).
    let sel_threat = add_threat(&mut ws, "Spoof Brake Msg");
    let Selection::Threat(threat_id) = sel_threat else {
        unreachable!()
    };
    let threat_name_col = col_by_key(TableId::Threats, "name");
    assert_eq!(
        cell_editor(
            ws.project(),
            TableId::Threats,
            Some(sel_threat),
            threat_name_col,
            &vocab,
            &catalog
        ),
        CellEditorKind::Text {
            current: "Spoof Brake Msg".into()
        }
    );
    let before = save_bytes(&ws);
    let edit = edit_cell(
        ws.project(),
        TableId::Threats,
        sel_threat,
        threat_name_col,
        &CellInput::Text("Replay Brake Msg".into()),
    )
    .expect("rename threat");
    assert!(matches!(edit, Edit::RenameThreat { threat, .. } if threat == threat_id));
    apply_edit(&mut ws, &edit);
    ws.undo().expect("undo rename threat");
    assert_eq!(save_bytes(&ws), before);

    // Seed an existing scaling_reason before editing scales via grid.
    apply_edit(
        &mut ws,
        &Edit::SetThreatScaling {
            threat: threat_id,
            scales: Some(false),
            reason: "Requires physical OBD access".into(),
        },
    );
    let scales_col = col_by_key(TableId::Threats, "scales");
    assert_eq!(
        cell_editor(
            ws.project(),
            TableId::Threats,
            Some(sel_threat),
            scales_col,
            &vocab,
            &catalog
        ),
        CellEditorKind::SingleChoice {
            options: vec![catalog.tr("yes"), catalog.tr("no")],
            selected: Some(1),
            allow_clear: true,
        }
    );
    let before = save_bytes(&ws);
    let edit = edit_cell(
        ws.project(),
        TableId::Threats,
        sel_threat,
        scales_col,
        &CellInput::SingleChoice(Some(0)),
    )
    .expect("set scales true");
    assert_eq!(
        edit,
        Edit::SetThreatScaling {
            threat: threat_id,
            scales: Some(true),
            reason: "Requires physical OBD access".into(),
        }
    );
    apply_edit(&mut ws, &edit);
    let t = ws
        .project()
        .threat_scenario(ws.project().threat_scenario_idx(&threat_id).expect("idx"))
        .expect("slot");
    assert_eq!(t.scales(), Some(true));
    assert_eq!(t.scaling_reason(), "Requires physical OBD access");
    assert_eq!(
        cell_editor(
            ws.project(),
            TableId::Threats,
            Some(sel_threat),
            scales_col,
            &vocab,
            &catalog
        ),
        CellEditorKind::SingleChoice {
            options: vec![catalog.tr("yes"), catalog.tr("no")],
            selected: Some(0),
            allow_clear: true,
        }
    );
    // Clear scales to None, still preserving scaling_reason.
    let clear_edit = edit_cell(
        ws.project(),
        TableId::Threats,
        sel_threat,
        scales_col,
        &CellInput::SingleChoice(None),
    )
    .expect("clear scales");
    apply_edit(&mut ws, &clear_edit);
    let t = ws
        .project()
        .threat_scenario(ws.project().threat_scenario_idx(&threat_id).expect("idx"))
        .expect("slot");
    assert_eq!(t.scales(), None);
    assert_eq!(t.scaling_reason(), "Requires physical OBD access");
    ws.undo().expect("undo clear scales");
    ws.undo().expect("undo set scales");
    assert_eq!(save_bytes(&ws), before);

    // 4. Risks: name, treatment (SingleChoice with allow_clear).
    let risk_name_col = col_by_key(TableId::Risks, "name");
    assert_eq!(
        cell_editor(
            ws.project(),
            TableId::Risks,
            Some(sel_threat),
            risk_name_col,
            &vocab,
            &catalog
        ),
        CellEditorKind::Text {
            current: "Spoof Brake Msg".into()
        }
    );
    let before = save_bytes(&ws);
    let edit = edit_cell(
        ws.project(),
        TableId::Risks,
        sel_threat,
        risk_name_col,
        &CellInput::Text("Risk Renamed".into()),
    )
    .expect("rename risk row");
    assert!(matches!(edit, Edit::RenameThreat { threat, .. } if threat == threat_id));
    apply_edit(&mut ws, &edit);
    ws.undo().expect("undo rename risk");
    assert_eq!(save_bytes(&ws), before);

    let treatment_col = col_by_key(TableId::Risks, "treatment");
    assert_eq!(
        cell_editor(
            ws.project(),
            TableId::Risks,
            Some(sel_threat),
            treatment_col,
            &vocab,
            &catalog
        ),
        CellEditorKind::SingleChoice {
            options: vocab.treatments.clone(),
            selected: None,
            allow_clear: true,
        }
    );
    let before = save_bytes(&ws);
    let edit = edit_cell(
        ws.project(),
        TableId::Risks,
        sel_threat,
        treatment_col,
        &CellInput::SingleChoice(Some(1)),
    )
    .expect("set treatment");
    assert_eq!(
        edit,
        Edit::SetTreatment {
            threat: threat_id,
            treatment: Some(1)
        }
    );
    apply_edit(&mut ws, &edit);
    let t = ws
        .project()
        .threat_scenario(ws.project().threat_scenario_idx(&threat_id).expect("idx"))
        .expect("slot");
    assert_eq!(t.treatment(), Some(RiskTreatment::Reduce));
    assert_eq!(
        cell_editor(
            ws.project(),
            TableId::Risks,
            Some(sel_threat),
            treatment_col,
            &vocab,
            &catalog
        ),
        CellEditorKind::SingleChoice {
            options: vocab.treatments.clone(),
            selected: Some(1),
            allow_clear: true,
        }
    );
    let clear = edit_cell(
        ws.project(),
        TableId::Risks,
        sel_threat,
        treatment_col,
        &CellInput::SingleChoice(None),
    )
    .expect("clear treatment");
    apply_edit(&mut ws, &clear);
    let t = ws
        .project()
        .threat_scenario(ws.project().threat_scenario_idx(&threat_id).expect("idx"))
        .expect("slot");
    assert_eq!(t.treatment(), None);
    ws.undo().expect("undo clear treatment");
    ws.undo().expect("undo set treatment");
    assert_eq!(save_bytes(&ws), before);

    // 5. Controls: name.
    let control_name_col = col_by_key(TableId::Controls, "name");
    assert_eq!(
        cell_editor(
            ws.project(),
            TableId::Controls,
            Some(sel_control),
            control_name_col,
            &vocab,
            &catalog
        ),
        CellEditorKind::Text {
            current: "SecOC".into()
        }
    );
    let before = save_bytes(&ws);
    let edit = edit_cell(
        ws.project(),
        TableId::Controls,
        sel_control,
        control_name_col,
        &CellInput::Text("SecOC v2".into()),
    )
    .expect("rename control");
    assert!(matches!(edit, Edit::RenameControl { control, .. } if control == control_id));
    apply_edit(&mut ws, &edit);
    assert_eq!(
        ws.project()
            .security_control(ws.project().security_control_idx(&control_id).expect("idx"))
            .expect("slot")
            .name(),
        "SecOC v2"
    );
    ws.undo().expect("undo rename control");
    assert_eq!(save_bytes(&ws), before);

    // 6. Goals: name, responsible (LinkPicker).
    let goal_name_col = col_by_key(TableId::Goals, "name");
    assert_eq!(
        cell_editor(
            ws.project(),
            TableId::Goals,
            Some(sel_goal),
            goal_name_col,
            &vocab,
            &catalog
        ),
        CellEditorKind::Text {
            current: "Protect Braking".into()
        }
    );
    let before = save_bytes(&ws);
    let edit = edit_cell(
        ws.project(),
        TableId::Goals,
        sel_goal,
        goal_name_col,
        &CellInput::Text("Ensure Braking Integrity".into()),
    )
    .expect("rename goal");
    assert!(matches!(edit, Edit::RenameGoal { goal, .. } if goal == goal_id));
    apply_edit(&mut ws, &edit);
    assert_eq!(
        ws.project()
            .cybersecurity_goal(ws.project().cybersecurity_goal_idx(&goal_id).expect("idx"))
            .expect("slot")
            .name(),
        "Ensure Braking Integrity"
    );
    ws.undo().expect("undo rename goal");
    assert_eq!(save_bytes(&ws), before);

    assert_eq!(
        cell_editor(
            ws.project(),
            TableId::Goals,
            Some(sel_goal),
            col_by_key(TableId::Goals, "responsible"),
            &vocab,
            &catalog
        ),
        CellEditorKind::LinkPicker {
            relation: RelationKey::GOAL_RESPONSIBLE
        }
    );

    // 7. Claims: name, responsible (LinkPicker).
    let claim_name_col = col_by_key(TableId::Claims, "name");
    assert_eq!(
        cell_editor(
            ws.project(),
            TableId::Claims,
            Some(sel_claim),
            claim_name_col,
            &vocab,
            &catalog
        ),
        CellEditorKind::Text {
            current: "Gateway Isolated".into()
        }
    );
    let before = save_bytes(&ws);
    let edit = edit_cell(
        ws.project(),
        TableId::Claims,
        sel_claim,
        claim_name_col,
        &CellInput::Text("Gateway Physically Isolated".into()),
    )
    .expect("rename claim");
    assert!(matches!(edit, Edit::RenameClaim { claim, .. } if claim == claim_id));
    apply_edit(&mut ws, &edit);
    assert_eq!(
        ws.project()
            .cybersecurity_claim(
                ws.project()
                    .cybersecurity_claim_idx(&claim_id)
                    .expect("idx")
            )
            .expect("slot")
            .name(),
        "Gateway Physically Isolated"
    );
    ws.undo().expect("undo rename claim");
    assert_eq!(save_bytes(&ws), before);

    assert_eq!(
        cell_editor(
            ws.project(),
            TableId::Claims,
            Some(sel_claim),
            col_by_key(TableId::Claims, "responsible"),
            &vocab,
            &catalog
        ),
        CellEditorKind::LinkPicker {
            relation: RelationKey::CLAIM_RESPONSIBLE
        }
    );
}

#[test]
fn cell_editor_and_edit_cell_guards_return_exact_errors() {
    let mut ws = empty_workspace();
    let catalog = Catalog::load(Language::En);
    let vocab = vocabulary(ws.project().methodology());
    let prop_count = vocab.properties.len();
    let impact_count = vocab.impact_categories.len();

    let asset_id = add_asset(&mut ws, "ECU Flash", vec![0]);
    let sel_asset = Selection::Asset(asset_id);
    let sel_damage = add_damage(&mut ws, "Bricked ECU");
    let sel_threat = add_threat(&mut ws, "Reflash Attack");

    // Read-only columns: Id, Computed, read-only Links (THREAT_ASSET), editable Links,
    // and cross-table Rating column.
    let id_col = col_by_key(TableId::Assets, "id");
    let computed_col = col_by_key(TableId::Assets, "threats");
    let threat_asset_col = col_by_key(TableId::Threats, "asset");
    let asset_comp_col = col_by_key(TableId::Assets, "component");
    let treatment_col = col_by_key(TableId::Risks, "treatment");

    for (table, target, col) in [
        (TableId::Assets, sel_asset, id_col),
        (TableId::Assets, sel_asset, computed_col),
        (TableId::Threats, sel_threat, threat_asset_col),
        (
            TableId::Risks,
            sel_threat,
            col_by_key(TableId::Risks, "asset"),
        ),
        (TableId::Threats, sel_threat, treatment_col),
    ] {
        assert_eq!(
            cell_editor(ws.project(), table, Some(target), col, &vocab, &catalog),
            CellEditorKind::ReadOnly
        );
        assert_eq!(
            edit_cell(
                ws.project(),
                table,
                target,
                col,
                &CellInput::Text("Valid".into())
            ),
            Err(GridError::ReadOnlyColumn)
        );
    }
    // Editable Links column opens LinkPicker but edit_cell refuses it with ReadOnlyColumn.
    assert_eq!(
        edit_cell(
            ws.project(),
            TableId::Assets,
            sel_asset,
            asset_comp_col,
            &CellInput::Text("Valid".into())
        ),
        Err(GridError::ReadOnlyColumn)
    );

    // Problems table, None target, mismatched target kind, and stale/nonexistent IDs.
    let asset_name_col = col_by_key(TableId::Assets, "name");
    assert_eq!(
        cell_editor(
            ws.project(),
            TableId::Problems,
            Some(sel_asset),
            col_by_key(TableId::Problems, "problem"),
            &vocab,
            &catalog
        ),
        CellEditorKind::ReadOnly
    );
    assert_eq!(
        edit_cell(
            ws.project(),
            TableId::Problems,
            sel_asset,
            asset_name_col,
            &CellInput::Text("Valid".into())
        ),
        Err(GridError::NotEditable)
    );
    assert_eq!(
        cell_editor(
            ws.project(),
            TableId::Assets,
            None,
            asset_name_col,
            &vocab,
            &catalog
        ),
        CellEditorKind::ReadOnly
    );
    // Mismatched target kind with valid text -> only target_matches_table refuses.
    assert_eq!(
        cell_editor(
            ws.project(),
            TableId::Assets,
            Some(sel_damage),
            asset_name_col,
            &vocab,
            &catalog
        ),
        CellEditorKind::ReadOnly
    );
    assert_eq!(
        edit_cell(
            ws.project(),
            TableId::Assets,
            sel_damage,
            asset_name_col,
            &CellInput::Text("Valid".into())
        ),
        Err(GridError::NotEditable)
    );

    // Stale/nonexistent IDs of matching kind -> only target_exists refuses.
    let stale_asset = Selection::Asset(AssetId::new_v7());
    let stale_damage = Selection::Damage(DamageId::new_v7());
    let stale_threat = Selection::Threat(ThreatId::new_v7());
    for (table, stale, col) in [
        (TableId::Assets, stale_asset, asset_name_col),
        (
            TableId::Damage,
            stale_damage,
            col_by_key(TableId::Damage, "name"),
        ),
        (
            TableId::Threats,
            stale_threat,
            col_by_key(TableId::Threats, "name"),
        ),
    ] {
        assert_eq!(
            cell_editor(ws.project(), table, Some(stale), col, &vocab, &catalog),
            CellEditorKind::ReadOnly
        );
        assert_eq!(
            edit_cell(
                ws.project(),
                table,
                stale,
                col,
                &CellInput::Text("Valid".into())
            ),
            Err(GridError::NotEditable)
        );
    }

    // Blank name -> EmptyName { row: 1 }.
    assert_eq!(
        edit_cell(
            ws.project(),
            TableId::Assets,
            sel_asset,
            asset_name_col,
            &CellInput::Text("   \t ".into())
        ),
        Err(GridError::EmptyName { row: 1 })
    );

    // Asset properties: all-false and wrong-length MultiChoice -> UnknownRating.
    let prop_col = col_by_key(TableId::Assets, "properties");
    assert!(matches!(
        edit_cell(
            ws.project(),
            TableId::Assets,
            sel_asset,
            prop_col,
            &CellInput::MultiChoice(vec![false; prop_count])
        ),
        Err(GridError::UnknownRating { row: 1, ref col, .. }) if col == "properties"
    ));
    assert!(matches!(
        edit_cell(
            ws.project(),
            TableId::Assets,
            sel_asset,
            prop_col,
            &CellInput::MultiChoice(vec![true])
        ),
        Err(GridError::UnknownRating { row: 1, ref col, .. }) if col == "properties"
    ));
    assert!(matches!(
        edit_cell(
            ws.project(),
            TableId::Assets,
            sel_asset,
            prop_col,
            &CellInput::MultiChoice(vec![true; prop_count + 1])
        ),
        Err(GridError::UnknownRating { row: 1, ref col, .. }) if col == "properties"
    ));

    // Damage impact: wrong-length Impacts, out-of-bounds level, out-of-bounds category.
    let impact_col = col_by_key(TableId::Damage, "impact");
    assert!(matches!(
        edit_cell(
            ws.project(),
            TableId::Damage,
            sel_damage,
            impact_col,
            &CellInput::Impacts(vec![Some(0)])
        ),
        Err(GridError::UnknownRating { row: 1, ref col, .. }) if col == "impact"
    ));
    let mut oob_levels = vec![None; impact_count];
    oob_levels[0] = Some(99);
    assert!(matches!(
        edit_cell(
            ws.project(),
            TableId::Damage,
            sel_damage,
            impact_col,
            &CellInput::Impacts(oob_levels)
        ),
        Err(GridError::UnknownRating { row: 1, ref col, .. }) if col == "impact"
    ));
    assert!(matches!(
        edit_cell(
            ws.project(),
            TableId::Damage,
            sel_damage,
            impact_col,
            &CellInput::Impact {
                category: impact_count,
                level: Some(0)
            }
        ),
        Err(GridError::UnknownRating { row: 1, ref col, .. }) if col == "impact"
    ));
    assert!(matches!(
        edit_cell(
            ws.project(),
            TableId::Damage,
            sel_damage,
            impact_col,
            &CellInput::Impact {
                category: 0,
                level: Some(99)
            }
        ),
        Err(GridError::UnknownRating { row: 1, ref col, .. }) if col == "impact"
    ));

    // Threats scales & Risks treatment out-of-range indices.
    assert!(matches!(
        edit_cell(
            ws.project(),
            TableId::Threats,
            sel_threat,
            col_by_key(TableId::Threats, "scales"),
            &CellInput::SingleChoice(Some(2))
        ),
        Err(GridError::UnknownRating { row: 1, ref col, .. }) if col == "scales"
    ));
    assert!(matches!(
        edit_cell(
            ws.project(),
            TableId::Risks,
            sel_threat,
            treatment_col,
            &CellInput::SingleChoice(Some(TREATMENTS.len()))
        ),
        Err(GridError::UnknownRating { row: 1, ref col, .. }) if col == "treatment"
    ));

    // Wrong CellInput variant on editable columns -> UnknownRating with empty value.
    assert_eq!(
        edit_cell(
            ws.project(),
            TableId::Assets,
            sel_asset,
            asset_name_col,
            &CellInput::SingleChoice(Some(0))
        ),
        Err(GridError::UnknownRating {
            row: 1,
            col: "name".into(),
            value: String::new()
        })
    );
    assert_eq!(
        edit_cell(
            ws.project(),
            TableId::Assets,
            sel_asset,
            prop_col,
            &CellInput::Text("C".into())
        ),
        Err(GridError::UnknownRating {
            row: 1,
            col: "properties".into(),
            value: String::new()
        })
    );

    // edit_cells guard cases: empty targets, stale target at index 1, blank name, wrong length.
    assert_eq!(
        edit_cells(
            ws.project(),
            TableId::Assets,
            &[],
            asset_name_col,
            &CellInput::Text("Valid".into())
        ),
        Err(GridError::NotEditable)
    );
    assert_eq!(
        edit_cells(
            ws.project(),
            TableId::Assets,
            &[sel_asset, stale_asset],
            asset_name_col,
            &CellInput::Text("Valid".into())
        ),
        Err(GridError::NotEditable)
    );
    let asset2 = Selection::Asset(add_asset(&mut ws, "ECU RAM", vec![0]));
    assert_eq!(
        edit_cells(
            ws.project(),
            TableId::Assets,
            &[sel_asset, asset2],
            asset_name_col,
            &CellInput::Text("   ".into())
        ),
        Err(GridError::EmptyName { row: 1 })
    );
    assert!(matches!(
        edit_cells(
            ws.project(),
            TableId::Assets,
            &[sel_asset, asset2],
            prop_col,
            &CellInput::MultiChoice(vec![true])
        ),
        Err(GridError::UnknownRating { row: 1, ref col, .. }) if col == "properties"
    ));
}

#[test]
fn multi_cell_edits_across_three_rows_apply_and_undo_in_one_step() {
    let mut ws = empty_workspace();
    let prop_count = ws.project().methodology().properties().count();
    let impact_count = ws.project().methodology().impact_category_count();

    // 1. Assets: 3 rows starting with {C} (index 0) -> replace with {I, A} (indices 1, 2) in one batch.
    let a1 = add_asset(&mut ws, "Asset 1", vec![0]);
    let a2 = add_asset(&mut ws, "Asset 2", vec![0]);
    let a3 = add_asset(&mut ws, "Asset 3", vec![0]);
    let asset_targets = [
        Selection::Asset(a1),
        Selection::Asset(a2),
        Selection::Asset(a3),
    ];
    let prop_col = col_by_key(TableId::Assets, "properties");

    let before = save_bytes(&ws);
    let batch = edit_cells(
        ws.project(),
        TableId::Assets,
        &asset_targets,
        prop_col,
        &CellInput::MultiChoice(flags_for(prop_count, &[1, 2])),
    )
    .expect("multi-cell properties edit");
    assert!(matches!(batch, Edit::Batch(ref items) if items.len() == 3));
    apply_edit(&mut ws, &batch);
    let want_ia = prop_set(ws.project(), &[1, 2]);
    for id in [a1, a2, a3] {
        assert_eq!(asset_props(ws.project(), id), want_ia);
    }
    // Re-applying identical flags prunes all 3 as no-ops -> empty batch.
    let noop = edit_cells(
        ws.project(),
        TableId::Assets,
        &asset_targets,
        prop_col,
        &CellInput::MultiChoice(flags_for(prop_count, &[1, 2])),
    )
    .expect("noop properties edit");
    assert_eq!(noop, Edit::Batch(vec![]));

    ws.undo().expect("single undo restores all 3 assets");
    assert_eq!(save_bytes(&ws), before);

    // 2. Damage: 3 rows rated via CellInput::Impacts in one batch.
    let d1 = add_damage(&mut ws, "Damage 1");
    let d2 = add_damage(&mut ws, "Damage 2");
    let d3 = add_damage(&mut ws, "Damage 3");
    let damage_targets = [d1, d2, d3];
    let impact_col = col_by_key(TableId::Damage, "impact");
    let mut levels = vec![None; impact_count];
    levels[0] = Some(2);
    levels[1] = Some(1);

    let before = save_bytes(&ws);
    let batch = edit_cells(
        ws.project(),
        TableId::Damage,
        &damage_targets,
        impact_col,
        &CellInput::Impacts(levels.clone()),
    )
    .expect("multi-cell impacts edit");
    // 3 rows * 2 changed categories = 6 flat SetImpact edits.
    assert!(matches!(batch, Edit::Batch(ref items) if items.len() == 6));
    apply_edit(&mut ws, &batch);
    let m = ws.project().methodology();
    let cat0 = m.impact_categories().next().expect("cat0");
    let cat1 = m.impact_categories().nth(1).expect("cat1");
    let lvl0 = m.levels(cat0).nth(2).expect("lvl2");
    let lvl1 = m.levels(cat1).nth(1).expect("lvl1");
    for sel in damage_targets {
        let Selection::Damage(id) = sel else {
            unreachable!()
        };
        let d = ws
            .project()
            .damage_scenario(ws.project().damage_scenario_idx(&id).expect("idx"))
            .expect("slot");
        assert_eq!(d.impact(cat0), Some(lvl0));
        assert_eq!(d.impact(cat1), Some(lvl1));
    }
    // Re-applying same Impacts across all 3 rows prunes to empty batch.
    let noop = edit_cells(
        ws.project(),
        TableId::Damage,
        &damage_targets,
        impact_col,
        &CellInput::Impacts(levels),
    )
    .expect("noop impacts");
    assert_eq!(noop, Edit::Batch(vec![]));

    ws.undo().expect("single undo restores all 3 damages");
    assert_eq!(save_bytes(&ws), before);

    // 3. Threats: 3 rows with distinct scaling_reason values preserved on bulk scales edit.
    let t1 = add_threat(&mut ws, "Threat 1");
    let t2 = add_threat(&mut ws, "Threat 2");
    let t3 = add_threat(&mut ws, "Threat 3");
    let threat_targets = [t1, t2, t3];
    let reasons = [
        "Fleet-wide OTA vector",
        "Shared signing key",
        "Local CAN access only",
    ];
    for (sel, reason) in threat_targets.iter().zip(reasons) {
        let Selection::Threat(threat) = *sel else {
            unreachable!()
        };
        apply_edit(
            &mut ws,
            &Edit::SetThreatScaling {
                threat,
                scales: Some(false),
                reason: reason.into(),
            },
        );
    }

    let scales_col = col_by_key(TableId::Threats, "scales");
    let before = save_bytes(&ws);
    let batch = edit_cells(
        ws.project(),
        TableId::Threats,
        &threat_targets,
        scales_col,
        &CellInput::SingleChoice(Some(0)),
    )
    .expect("multi-cell scales edit");
    assert!(matches!(batch, Edit::Batch(ref items) if items.len() == 3));
    apply_edit(&mut ws, &batch);
    for (sel, expected_reason) in threat_targets.iter().zip(reasons) {
        let Selection::Threat(id) = *sel else {
            unreachable!()
        };
        let t = ws
            .project()
            .threat_scenario(ws.project().threat_scenario_idx(&id).expect("idx"))
            .expect("slot");
        assert_eq!(t.scales(), Some(true));
        assert_eq!(t.scaling_reason(), expected_reason);
    }
    ws.undo().expect("single undo restores all 3 threats");
    assert_eq!(save_bytes(&ws), before);

    // 4. Risks: 3 rows set treatment to Reduce (index 1) in one batch.
    let treatment_col = col_by_key(TableId::Risks, "treatment");
    let before = save_bytes(&ws);
    let batch = edit_cells(
        ws.project(),
        TableId::Risks,
        &threat_targets,
        treatment_col,
        &CellInput::SingleChoice(Some(1)),
    )
    .expect("multi-cell treatment edit");
    assert!(matches!(batch, Edit::Batch(ref items) if items.len() == 3));
    apply_edit(&mut ws, &batch);
    for sel in threat_targets {
        let Selection::Threat(id) = sel else {
            unreachable!()
        };
        let t = ws
            .project()
            .threat_scenario(ws.project().threat_scenario_idx(&id).expect("idx"))
            .expect("slot");
        assert_eq!(t.treatment(), Some(RiskTreatment::Reduce));
    }
    ws.undo().expect("single undo restores all 3 risks");
    assert_eq!(save_bytes(&ws), before);
}
