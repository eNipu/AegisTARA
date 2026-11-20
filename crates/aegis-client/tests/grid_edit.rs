//! Integration tests for `Edit::SetAssetProperties` and the `aegis_client::grid`
//! core: `GridError`, `GridSelection`, `add_row_edit`, and `delete_rows_edit` (189b1).

use std::sync::Arc;

use aegis_client::commands::{Edit, EditError, TREATMENTS};
use aegis_client::explorer::Owner;
use aegis_client::grid::{
    CellEditorKind, CellInput, GridError, GridSelection, MAX_PASTE_BYTES, MAX_PASTE_COLS,
    MAX_PASTE_ROWS, add_row_edit, cell_editor, delete_rows_edit, edit_cell, edit_cells, fill_down,
    format_tsv, parse_tsv, paste_tsv,
};
use aegis_client::i18n::{Catalog, Language};
use aegis_client::pages::{selection_visual_id, vocabulary, vocabulary_for_lang};
use aegis_client::relations::{self, RelationKey};
use aegis_client::table::{
    Cell, TableRow, selection_exclusion, selection_label, table_rows, table_rows_for_owner,
};
use aegis_client::undo_label::Label;
use aegis_client::view::{ColumnSpec, Selection, TableId};
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{
    AssetId, ComponentId, ConditionId, DamageId, ElementId, ElementRef, Methodology, Op, PartyId,
    PartyKind, Project, PropertySet, Provenance, RiskTreatment, ThreatId,
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

// ---- 189b3: fill_down (including editable links) and RFC-4180 TSV codec ----

fn add_party(ws: &mut Workspace, name: &str) -> PartyId {
    let id = PartyId::new_v7();
    let provenance = Provenance::human(ACTOR, NOW_MS);
    ws.apply(
        vec![Op::AddParty {
            id,
            name: name.into(),
            kind: PartyKind::Supplier,
            provenance,
        }],
        Origin::Edit,
    )
    .expect("add party");
    id
}

fn all_cols(table: TableId) -> Vec<&'static ColumnSpec> {
    table.columns().iter().collect()
}

fn synthetic_row(target: Option<Selection>, texts: &[&str]) -> TableRow {
    TableRow {
        target,
        cells: texts
            .iter()
            .map(|t| Cell {
                text: (*t).to_owned(),
                tone: None,
                chips: Vec::new(),
            })
            .collect(),
        excluded: false,
        excluded_reason: None,
    }
}

#[test]
fn fill_down_assets_copies_name_properties_and_component_without_redundant_unlink() {
    let mut ws = empty_workspace();
    let catalog = Catalog::load(Language::En);
    let vocab = vocabulary(ws.project().methodology());

    let comp_a = add_component(&mut ws, "Gateway ECU", None);
    let comp_b = add_component(&mut ws, "Brake ECU", None);

    let a1 = add_asset(&mut ws, "CAN Bus", vec![0, 2]);
    let a2 = add_asset(&mut ws, "Key Store", vec![1]);
    let a3 = add_asset(&mut ws, "Flash", vec![0, 1]);
    apply_edit(
        &mut ws,
        &Edit::SetAssetComponent {
            asset: a1,
            component: Some(comp_a),
        },
    );
    apply_edit(
        &mut ws,
        &Edit::SetAssetComponent {
            asset: a2,
            component: Some(comp_b),
        },
    );

    let rows = table_rows(&ws, TableId::Assets, &vocab, &catalog);
    assert_eq!(rows.len(), 3);
    let cols = all_cols(TableId::Assets);
    let mut sel = GridSelection::single(0, 0);
    sel.extend_to(2, cols.len() - 1);

    let before = save_bytes(&ws);
    let batch =
        fill_down(ws.project(), TableId::Assets, &rows, &cols, sel).expect("fill down assets");

    // Verify 1:1 component replacement emits SetAssetComponent(Some(comp_a)) directly
    // with NO redundant SetAssetComponent(None) unlink before it.
    let Edit::Batch(ref items) = batch else {
        panic!("expected Edit::Batch, got {batch:?}");
    };
    assert!(
        !items.iter().any(|e| matches!(
            e,
            Edit::SetAssetComponent {
                component: None,
                ..
            }
        )),
        "1:1 link replacement must not emit redundant unlink: {items:?}"
    );
    assert!(items.iter().any(|e| matches!(
        e,
        Edit::SetAssetComponent {
            asset,
            component: Some(c),
        } if *asset == a2 && *c == comp_a
    )));
    assert!(items.iter().any(|e| matches!(
        e,
        Edit::SetAssetComponent {
            asset,
            component: Some(c),
        } if *asset == a3 && *c == comp_a
    )));

    apply_edit(&mut ws, &batch);
    for id in [a2, a3] {
        let idx = ws.project().asset_idx(&id).expect("asset idx");
        let asset = ws.project().asset(idx).expect("asset slot");
        assert_eq!(asset.name(), "CAN Bus");
        assert_eq!(asset.properties(), prop_set(ws.project(), &[0, 2]));
        assert_eq!(
            relations::related(
                ws.project(),
                RelationKey::ASSET_COMPONENT,
                Selection::Asset(id)
            ),
            vec![Selection::Component(comp_a)]
        );
    }

    // Single undo restores exact project bytes.
    ws.undo().expect("undo asset fill_down");
    assert_eq!(save_bytes(&ws), before);

    // Clearing component when source asset has no component: place unbound a3 on top.
    let clear_rows = vec![
        synthetic_row(Some(Selection::Asset(a3)), &[]),
        synthetic_row(Some(Selection::Asset(a1)), &[]),
        synthetic_row(Some(Selection::Asset(a2)), &[]),
    ];
    let comp_cols = [col_by_key(TableId::Assets, "component")];
    let mut clear_sel = GridSelection::single(0, 0);
    clear_sel.extend_to(2, 0);

    let before_clear = save_bytes(&ws);
    let clear_batch = fill_down(
        ws.project(),
        TableId::Assets,
        &clear_rows,
        &comp_cols,
        clear_sel,
    )
    .expect("fill down clear component");
    let Edit::Batch(ref clear_items) = clear_batch else {
        panic!("expected Edit::Batch for clearing 2 components");
    };
    assert_eq!(clear_items.len(), 2);
    assert!(clear_items.iter().all(|e| matches!(
        e,
        Edit::SetAssetComponent {
            component: None,
            ..
        }
    )));
    apply_edit(&mut ws, &clear_batch);
    for id in [a1, a2, a3] {
        assert!(
            relations::related(
                ws.project(),
                RelationKey::ASSET_COMPONENT,
                Selection::Asset(id)
            )
            .is_empty()
        );
    }
    // Filling down again when all are already unbound -> NothingToFill (no empty batch).
    assert_eq!(
        fill_down(
            ws.project(),
            TableId::Assets,
            &clear_rows,
            &comp_cols,
            clear_sel
        ),
        Err(GridError::NothingToFill)
    );
    ws.undo().expect("undo clear component fill_down");
    assert_eq!(save_bytes(&ws), before_clear);
}

#[test]
fn fill_down_goals_syncs_nm_responsible_links_and_undoes_in_one_step() {
    let mut ws = empty_workspace();
    let catalog = Catalog::load(Language::En);
    let vocab = vocabulary(ws.project().methodology());

    let p1 = add_party(&mut ws, "Bosch");
    let p2 = add_party(&mut ws, "Continental");
    let p3 = add_party(&mut ws, "Denso");

    let g1 = add_goal(&mut ws, "Goal 1");
    let g2 = add_goal(&mut ws, "Goal 2");
    let g3 = add_goal(&mut ws, "Goal 3");
    let Selection::Goal(g1_id) = g1 else {
        unreachable!()
    };
    let Selection::Goal(g2_id) = g2 else {
        unreachable!()
    };

    // g1: [p1, p2], g2: [p2, p3] (extra p3, missing p1), g3: [] (missing p1, p2).
    for (goal, party) in [(g1_id, p1), (g1_id, p2), (g2_id, p2), (g2_id, p3)] {
        apply_edit(
            &mut ws,
            &Edit::LinkGoalParty {
                goal,
                party,
                linked: true,
            },
        );
    }

    let rows = table_rows(&ws, TableId::Goals, &vocab, &catalog);
    let cols = [
        col_by_key(TableId::Goals, "id"),
        col_by_key(TableId::Goals, "responsible"),
    ];
    let mut sel = GridSelection::single(0, 0);
    sel.extend_to(2, 1);

    let before = save_bytes(&ws);
    let batch = fill_down(ws.project(), TableId::Goals, &rows, &cols, sel)
        .expect("fill down goal responsible");
    let Edit::Batch(ref items) = batch else {
        panic!("expected Edit::Batch for goal responsible sync");
    };
    // g2: unlink p3 + link p1 (2 edits, p2 untouched); g3: link p1 + link p2 (2 edits).
    assert_eq!(items.len(), 4);
    assert!(items.iter().any(|e| matches!(
        e,
        Edit::LinkGoalParty {
            goal,
            party,
            linked: false,
        } if *goal == g2_id && *party == p3
    )));
    assert!(items.iter().any(|e| matches!(
        e,
        Edit::LinkGoalParty {
            goal,
            party,
            linked: true,
        } if *goal == g2_id && *party == p1
    )));

    apply_edit(&mut ws, &batch);
    let expected = [Selection::Party(p1), Selection::Party(p2)];
    for goal in [g1, g2, g3] {
        let mut linked = relations::related(ws.project(), RelationKey::GOAL_RESPONSIBLE, goal);
        linked.sort_by_key(|s| format!("{s:?}"));
        let mut want = expected.to_vec();
        want.sort_by_key(|s| format!("{s:?}"));
        assert_eq!(linked, want);
    }

    // Re-running fill_down when all goals already match returns NothingToFill.
    assert_eq!(
        fill_down(ws.project(), TableId::Goals, &rows, &cols, sel),
        Err(GridError::NothingToFill)
    );

    ws.undo().expect("undo goal responsible fill_down");
    assert_eq!(save_bytes(&ws), before);
}

#[test]
fn fill_down_threats_risks_and_damage_ratings_with_exact_undo() {
    let mut ws = empty_workspace();
    let catalog = Catalog::load(Language::En);
    let vocab = vocabulary(ws.project().methodology());
    let _asset = add_asset(&mut ws, "Bus", vec![0]);

    // 1. Threats: scales (preserving destination scaling_reason, skipping read-only asset link).
    let t1 = add_threat(&mut ws, "T1");
    let t2 = add_threat(&mut ws, "T2");
    let t3 = add_threat(&mut ws, "T3");
    let [
        Selection::Threat(t1_id),
        Selection::Threat(t2_id),
        Selection::Threat(t3_id),
    ] = [t1, t2, t3]
    else {
        unreachable!()
    };
    apply_edit(
        &mut ws,
        &Edit::SetThreatScaling {
            threat: t1_id,
            scales: Some(true),
            reason: "OTA fleet".into(),
        },
    );
    apply_edit(
        &mut ws,
        &Edit::SetThreatScaling {
            threat: t2_id,
            scales: Some(false),
            reason: "Physical port".into(),
        },
    );
    apply_edit(
        &mut ws,
        &Edit::SetThreatScaling {
            threat: t3_id,
            scales: None,
            reason: "Needs review".into(),
        },
    );

    let threat_rows = table_rows(&ws, TableId::Threats, &vocab, &catalog);
    let threat_cols = [
        col_by_key(TableId::Threats, "id"),
        col_by_key(TableId::Threats, "asset"),
        col_by_key(TableId::Threats, "scales"),
    ];
    let mut sel = GridSelection::single(0, 0);
    sel.extend_to(2, 2);

    let before = save_bytes(&ws);
    let batch = fill_down(
        ws.project(),
        TableId::Threats,
        &threat_rows,
        &threat_cols,
        sel,
    )
    .expect("fill down threat scales");
    apply_edit(&mut ws, &batch);
    for (id, expected_reason) in [(t2_id, "Physical port"), (t3_id, "Needs review")] {
        let t = ws
            .project()
            .threat_scenario(ws.project().threat_scenario_idx(&id).expect("idx"))
            .expect("slot");
        assert_eq!(t.scales(), Some(true));
        assert_eq!(t.scaling_reason(), expected_reason);
    }
    ws.undo().expect("undo threat scales fill_down");
    assert_eq!(save_bytes(&ws), before);

    // 2. Risks: treatment.
    apply_edit(
        &mut ws,
        &Edit::SetTreatment {
            threat: t1_id,
            treatment: Some(1), // Reduce
        },
    );
    apply_edit(
        &mut ws,
        &Edit::SetTreatment {
            threat: t2_id,
            treatment: Some(2), // Share
        },
    );
    let risk_rows = table_rows(&ws, TableId::Risks, &vocab, &catalog);
    let risk_cols = [col_by_key(TableId::Risks, "treatment")];
    let mut risk_sel = GridSelection::single(0, 0);
    risk_sel.extend_to(2, 0);

    let before = save_bytes(&ws);
    let batch = fill_down(
        ws.project(),
        TableId::Risks,
        &risk_rows,
        &risk_cols,
        risk_sel,
    )
    .expect("fill down risk treatment");
    apply_edit(&mut ws, &batch);
    for id in [t2_id, t3_id] {
        let t = ws
            .project()
            .threat_scenario(ws.project().threat_scenario_idx(&id).expect("idx"))
            .expect("slot");
        assert_eq!(t.treatment(), Some(RiskTreatment::Reduce));
    }
    ws.undo().expect("undo risk treatment fill_down");
    assert_eq!(save_bytes(&ws), before);

    // 3. Damage: impact (multi-category atomic copy + pruning of already-matching category).
    let d1 = add_damage(&mut ws, "D1");
    let d2 = add_damage(&mut ws, "D2");
    let d3 = add_damage(&mut ws, "D3");
    let [
        Selection::Damage(d1_id),
        Selection::Damage(d2_id),
        Selection::Damage(d3_id),
    ] = [d1, d2, d3]
    else {
        unreachable!()
    };
    apply_edit(
        &mut ws,
        &Edit::SetImpact {
            damage: d1_id,
            category: 0,
            level: Some(2),
        },
    );
    apply_edit(
        &mut ws,
        &Edit::SetImpact {
            damage: d1_id,
            category: 1,
            level: Some(1),
        },
    );
    apply_edit(
        &mut ws,
        &Edit::SetImpact {
            damage: d2_id,
            category: 0,
            level: Some(0),
        },
    );
    apply_edit(
        &mut ws,
        &Edit::SetImpact {
            damage: d2_id,
            category: 1,
            level: Some(1), // already matches d1
        },
    );

    let dmg_rows = table_rows(&ws, TableId::Damage, &vocab, &catalog);
    let dmg_cols = [col_by_key(TableId::Damage, "impact")];
    let mut dmg_sel = GridSelection::single(0, 0);
    dmg_sel.extend_to(2, 0);

    let before = save_bytes(&ws);
    let batch = fill_down(ws.project(), TableId::Damage, &dmg_rows, &dmg_cols, dmg_sel)
        .expect("fill down damage impact");
    // d2 needs only cat 0 (1 edit); d3 needs cat 0 and cat 1 (2 edits) => 3 edits total.
    assert!(matches!(batch, Edit::Batch(ref items) if items.len() == 3));
    apply_edit(&mut ws, &batch);
    let m = ws.project().methodology();
    let cat0 = m.impact_categories().next().expect("cat0");
    let cat1 = m.impact_categories().nth(1).expect("cat1");
    let lvl0 = m.levels(cat0).nth(2).expect("lvl2");
    let lvl1 = m.levels(cat1).nth(1).expect("lvl1");
    for id in [d2_id, d3_id] {
        let d = ws
            .project()
            .damage_scenario(ws.project().damage_scenario_idx(&id).expect("idx"))
            .expect("slot");
        assert_eq!(d.impact(cat0), Some(lvl0));
        assert_eq!(d.impact(cat1), Some(lvl1));
    }
    ws.undo().expect("undo damage impact fill_down");
    assert_eq!(save_bytes(&ws), before);
}

#[test]
fn fill_down_guards_and_edge_cases() {
    let mut ws = empty_workspace();
    let catalog = Catalog::load(Language::En);
    let vocab = vocabulary(ws.project().methodology());

    let a1 = add_asset(&mut ws, "Same", vec![0]);
    let a2 = add_asset(&mut ws, "Same", vec![0]);
    let a3 = add_asset(&mut ws, "Different", vec![1]);

    let rows = table_rows(&ws, TableId::Assets, &vocab, &catalog);
    let cols = all_cols(TableId::Assets);

    // 1. Problems table (valid 2-row selection, non-empty cols) -> only NotEditable triggers.
    let problem_cols = all_cols(TableId::Problems);
    let problem_rows = vec![
        synthetic_row(Some(Selection::Asset(a1)), &[]),
        synthetic_row(Some(Selection::Asset(a2)), &[]),
    ];
    let mut two_row = GridSelection::single(0, 0);
    two_row.extend_to(1, 0);
    assert_eq!(
        fill_down(
            ws.project(),
            TableId::Problems,
            &problem_rows,
            &problem_cols,
            two_row
        ),
        Err(GridError::NotEditable)
    );

    // 2. Single-row selection -> NothingToFill.
    assert_eq!(
        fill_down(
            ws.project(),
            TableId::Assets,
            &rows,
            &cols,
            GridSelection::single(0, 1)
        ),
        Err(GridError::NothingToFill)
    );

    // 3. Computed-only / Id-only selection across 2 rows -> NothingToFill.
    let computed_cols = [
        col_by_key(TableId::Assets, "id"),
        col_by_key(TableId::Assets, "threats"),
        col_by_key(TableId::Assets, "worst-risk"),
    ];
    let mut comp_sel = GridSelection::single(0, 0);
    comp_sel.extend_to(2, 2);
    assert_eq!(
        fill_down(
            ws.project(),
            TableId::Assets,
            &rows,
            &computed_cols,
            comp_sel
        ),
        Err(GridError::NothingToFill)
    );

    // 4. Identical rows (a1 and a2 have same name, properties, and component) -> NothingToFill.
    let mut identical_sel = GridSelection::single(0, 0);
    identical_sel.extend_to(1, cols.len() - 1);
    assert_eq!(
        fill_down(ws.project(), TableId::Assets, &rows, &cols, identical_sel),
        Err(GridError::NothingToFill)
    );

    // 5. Out-of-bounds src_row or col_start -> NothingToFill.
    let mut oob_row = GridSelection::single(10, 0);
    oob_row.extend_to(12, 1);
    assert_eq!(
        fill_down(ws.project(), TableId::Assets, &rows, &cols, oob_row),
        Err(GridError::NothingToFill)
    );
    let mut oob_col = GridSelection::single(0, 99);
    oob_col.extend_to(2, 100);
    assert_eq!(
        fill_down(ws.project(), TableId::Assets, &rows, &cols, oob_col),
        Err(GridError::NothingToFill)
    );

    // 6. Out-of-bounds end_row / col_end clamps cleanly + destination row with target: None skipped.
    let gapped_rows = vec![
        synthetic_row(Some(Selection::Asset(a1)), &[]),
        synthetic_row(None, &[]),
        synthetic_row(Some(Selection::Asset(a3)), &[]),
    ];
    let name_and_props = [
        col_by_key(TableId::Assets, "name"),
        col_by_key(TableId::Assets, "properties"),
    ];
    let mut wide_sel = GridSelection::single(0, 0);
    wide_sel.extend_to(50, 50);
    let before = save_bytes(&ws);
    let edit = fill_down(
        ws.project(),
        TableId::Assets,
        &gapped_rows,
        &name_and_props,
        wide_sel,
    )
    .expect("clamped fill_down skipping None target");
    apply_edit(&mut ws, &edit);
    let a3_slot = ws
        .project()
        .asset(ws.project().asset_idx(&a3).expect("a3 idx"))
        .expect("a3 slot");
    assert_eq!(a3_slot.name(), "Same");
    assert_eq!(a3_slot.properties(), prop_set(ws.project(), &[0]));
    ws.undo().expect("undo clamped fill_down");
    assert_eq!(save_bytes(&ws), before);

    // 7. Source row with target: None -> NothingToFill.
    let none_src = vec![
        synthetic_row(None, &[]),
        synthetic_row(Some(Selection::Asset(a3)), &[]),
    ];
    assert_eq!(
        fill_down(
            ws.project(),
            TableId::Assets,
            &none_src,
            &name_and_props,
            two_row
        ),
        Err(GridError::NothingToFill)
    );
}

#[test]
fn tsv_format_and_parse_roundtrip_and_rfc4180_rules() {
    let rows = vec![
        synthetic_row(None, &["A00", "Plain", "has\ttab", "quote\"here"]),
        synthetic_row(
            None,
            &[
                "A01",
                "line1\nline2",
                "crlf\r\ninside",
                "both\"and\nnewline",
            ],
        ),
        synthetic_row(None, &["A02", "bare\rcr", "", "café 日本語"]),
    ];

    // Full grid round-trip.
    let all_indices = [0, 1, 2, 3];
    let mut full = GridSelection::single(0, 0);
    full.extend_to(2, 3);
    let formatted = format_tsv(&rows, &all_indices, full);
    assert_eq!(
        formatted,
        "A00\tPlain\t\"has\ttab\"\t\"quote\"\"here\"\n\
         A01\t\"line1\nline2\"\t\"crlf\r\ninside\"\t\"both\"\"and\nnewline\"\n\
         A02\t\"bare\rcr\"\t\tcafé 日本語"
    );
    let parsed = parse_tsv(&formatted).expect("round-trip parse");
    assert_eq!(
        parsed,
        vec![
            vec!["A00", "Plain", "has\ttab", "quote\"here"],
            vec![
                "A01",
                "line1\nline2",
                "crlf\r\ninside",
                "both\"and\nnewline"
            ],
            vec!["A02", "bare\rcr", "", "café 日本語"],
        ]
    );

    // Subset visible_col_indices + reversed selection + clamped out-of-bounds end.
    let subset_cols = [1, 3];
    let mut sub_sel = GridSelection::single(99, 99);
    sub_sel.extend_to(1, 0);
    let sub_tsv = format_tsv(&rows, &subset_cols, sub_sel);
    assert_eq!(
        parse_tsv(&sub_tsv).expect("parse subset"),
        vec![
            vec!["line1\nline2", "both\"and\nnewline"],
            vec!["bare\rcr", "café 日本語"],
        ]
    );

    // format_tsv on empty rows, empty columns, or out-of-bounds start returns "".
    assert_eq!(format_tsv(&[], &all_indices, full), "");
    assert_eq!(format_tsv(&rows, &[], full), "");
    assert_eq!(
        format_tsv(&rows, &all_indices, GridSelection::single(3, 0)),
        ""
    );
    assert_eq!(
        format_tsv(&rows, &all_indices, GridSelection::single(0, 4)),
        ""
    );

    // Single trailing \r\n or \n stripped; \r\n and \n row separators both work.
    assert_eq!(
        parse_tsv("a\tb\r\nc\td\r\n").expect("crlf with trailing crlf"),
        vec![vec!["a", "b"], vec!["c", "d"]]
    );
    assert_eq!(
        parse_tsv("a\tb\nc\td\n").expect("lf with trailing lf"),
        vec![vec!["a", "b"], vec!["c", "d"]]
    );
    // Double trailing newline strips only one, leaving a final empty row.
    assert_eq!(
        parse_tsv("a\n\n").expect("double trailing lf"),
        vec![vec!["a"], vec![""]]
    );
    // Quoted empty field and escaped double-quote only.
    assert_eq!(
        parse_tsv("\"\"\t\"\"\"\"").expect("quoted empty and quote"),
        vec![vec!["", "\""]]
    );
    // Single "\n" alone is empty after stripping one trailing newline.
    assert_eq!(parse_tsv("\n"), Err(GridError::EmptyPaste));
}

#[test]
fn tsv_parse_guards_return_exact_errors() {
    // 1. PasteTooLarge: > 1 MiB, 1 row, 1 col (violates ONLY byte limit).
    let at_byte_cap = "a".repeat(MAX_PASTE_BYTES);
    assert_eq!(parse_tsv(&at_byte_cap).expect("exactly 1 MiB").len(), 1);
    let over_byte_cap = "a".repeat(MAX_PASTE_BYTES + 1);
    assert_eq!(parse_tsv(&over_byte_cap), Err(GridError::PasteTooLarge));

    // 2. TooManyRows: 1,001 short rows, 1 col, ~2 KiB (violates ONLY row limit).
    let at_row_cap = vec!["x"; MAX_PASTE_ROWS].join("\n");
    assert_eq!(
        parse_tsv(&at_row_cap).expect("exactly 1000 rows").len(),
        MAX_PASTE_ROWS
    );
    let over_row_cap = vec!["x"; MAX_PASTE_ROWS + 1].join("\n");
    assert_eq!(parse_tsv(&over_row_cap), Err(GridError::TooManyRows));

    // 3. TooManyCols: 1 row, 65 cols, ~129 bytes (violates ONLY col limit).
    let at_col_cap = vec!["x"; MAX_PASTE_COLS].join("\t");
    assert_eq!(
        parse_tsv(&at_col_cap).expect("exactly 64 cols")[0].len(),
        MAX_PASTE_COLS
    );
    let over_col_cap = vec!["x"; MAX_PASTE_COLS + 1].join("\t");
    assert_eq!(parse_tsv(&over_col_cap), Err(GridError::TooManyCols));

    // 4. MalformedQuote { line: 2 } for unclosed quote on line 2.
    assert_eq!(
        parse_tsv("a\tb\n\"unclosed"),
        Err(GridError::MalformedQuote { line: 2 })
    );
    assert_eq!(
        parse_tsv("a\tb\n\"unclosed\nstill open"),
        Err(GridError::MalformedQuote { line: 2 })
    );

    // 5. MalformedQuote { line: 1 } for unquoted embedded quote `foo"bar`.
    assert_eq!(
        parse_tsv("foo\"bar"),
        Err(GridError::MalformedQuote { line: 1 })
    );

    // 6. MalformedQuote { line: 1 } for trailing chars after closing quote `"foo"bar`.
    assert_eq!(
        parse_tsv("\"foo\"bar"),
        Err(GridError::MalformedQuote { line: 1 })
    );

    // 7. MalformedQuote { line: 3 } after a 2-line quoted multiline field on lines 1-2.
    assert_eq!(
        parse_tsv("\"line1\nline2\"\tok\nfoo\"bar"),
        Err(GridError::MalformedQuote { line: 3 })
    );
    assert_eq!(
        parse_tsv("\"line1\r\nline2\"\r\n\"bad\"extra"),
        Err(GridError::MalformedQuote { line: 3 })
    );

    // 8. EmptyPaste for "" and "\r\n".
    assert_eq!(parse_tsv(""), Err(GridError::EmptyPaste));
    assert_eq!(parse_tsv("\r\n"), Err(GridError::EmptyPaste));
}

// ---- 189b4: selection_label, selection_exclusion, and grid TSV paste engine ----

#[test]
fn selection_label_and_selection_exclusion_report_live_and_excluded_state() {
    let mut ws = empty_workspace();
    let asset_id = add_asset(&mut ws, "Brake CAN", vec![0]);
    let sel = Selection::Asset(asset_id);

    assert_eq!(selection_label(ws.project(), sel), "Brake CAN");
    assert_eq!(
        selection_label(ws.project(), Selection::Asset(AssetId::new_v7())),
        ""
    );
    assert_eq!(selection_exclusion(ws.project(), sel), (false, None));

    // Gate the asset behind an off condition and verify selection_exclusion reports it.
    let cond_id = ConditionId::new_v7();
    let prov = Provenance::human(ACTOR, NOW_MS);
    ws.apply(
        vec![
            Op::AddCondition {
                id: cond_id,
                name: "ADAS Option".into(),
                description: "".into(),
                present: false,
                provenance: prov.clone(),
            },
            Op::NeedCondition {
                element: ElementRef::Asset(asset_id),
                condition: cond_id,
                provenance: prov,
            },
        ],
        Origin::Edit,
    )
    .expect("apply condition exclusion");

    let (excluded, reason) = selection_exclusion(ws.project(), sel);
    assert!(excluded);
    let reason = reason.expect("exclusion reason");
    assert!(
        reason.contains("ADAS Option"),
        "expected condition name in reason, got '{reason}'"
    );
}

#[test]
fn paste_tsv_updates_and_appends_across_all_seven_editable_tables_with_exact_undo() {
    let mut ws = empty_workspace();
    let catalog = Catalog::load(Language::En);
    let vocab = vocabulary(ws.project().methodology());

    let comp1 = add_component(&mut ws, "Gateway ECU", None);
    let comp2 = add_component(&mut ws, "Brake ECU", None);
    let party1 = add_party(&mut ws, "OEM Security");
    let party2 = add_party(&mut ws, "Tier-1 Brakes");

    let asset1 = add_asset(&mut ws, "CAN Bus", vec![0]);
    let asset2 = add_asset(&mut ws, "Key Store", vec![0, 1]);
    let Selection::Damage(dmg1) = add_damage(&mut ws, "Loss of Braking") else {
        unreachable!()
    };
    let Selection::Threat(thr1) = add_threat(&mut ws, "Spoof Speed") else {
        unreachable!()
    };
    // Give thr1 a scaling reason to verify existing-row paste preserves it.
    apply_edit(
        &mut ws,
        &Edit::SetThreatScaling {
            threat: thr1,
            scales: Some(false),
            reason: "Single-vehicle physical tap".into(),
        },
    );
    let Selection::Control(ctrl1) = add_control(&mut ws, "SecOC") else {
        unreachable!()
    };
    let Selection::Goal(goal1) = add_goal(&mut ws, "Protect Braking") else {
        unreachable!()
    };
    let Selection::Claim(claim1) = add_claim(&mut ws, "Gateway Isolated") else {
        unreachable!()
    };

    // 1. Assets: update existing row 0 (name, component, properties) + append new row + skip blank row.
    {
        let before = save_bytes(&ws);
        let rows = table_rows(&ws, TableId::Assets, &vocab, &catalog);
        let cols = all_cols(TableId::Assets);
        let p0 = &vocab.properties[0];
        let p1 = &vocab.properties[1];
        let p2 = &vocab.properties[2];
        let comp2_vis = selection_visual_id(ws.project(), Selection::Component(comp2));
        let tsv = format!(
            "A00\tCAN-FD Bus\tGateway ECU\t{p0}, {p1}, {p2}\t99\tHigh\n\
             A01\tKey Store\t{comp2_vis}\t{p1}; {p2}\t0\t\n\
             \t\t\t\t\t\n\
             NEW\tTelemetry Key\tBrake ECU\t{p0}, {p1}\t0\t"
        );
        let edit = paste_tsv(
            ws.project(),
            TableId::Assets,
            Owner::Project,
            &rows,
            &cols,
            0,
            0,
            &tsv,
            &vocab,
            &catalog,
        )
        .expect("paste assets");
        apply_edit(&mut ws, &edit);

        let p = ws.project();
        let a1 = p.asset(p.asset_idx(&asset1).expect("a1")).expect("slot");
        assert_eq!(a1.name(), "CAN-FD Bus");
        assert_eq!(a1.component(), p.component_idx(&comp1));
        assert_eq!(a1.properties(), prop_set(p, &[0, 1, 2]));

        let a2 = p.asset(p.asset_idx(&asset2).expect("a2")).expect("slot");
        assert_eq!(a2.component(), p.component_idx(&comp2));
        assert_eq!(a2.properties(), prop_set(p, &[1, 2]));

        assert_eq!(p.assets().count(), 3);
        let (_, _, appended) = p
            .assets()
            .find(|(_, _, a)| a.name() == "Telemetry Key")
            .expect("appended asset");
        assert_eq!(appended.component(), p.component_idx(&comp2));
        assert_eq!(appended.properties(), prop_set(p, &[0, 1]));

        ws.undo().expect("undo assets paste");
        assert_eq!(save_bytes(&ws), before);
    }

    // 2. Damage: update existing row + append new row with multiple impact ratings.
    {
        let before = save_bytes(&ws);
        let rows = table_rows(&ws, TableId::Damage, &vocab, &catalog);
        let cols = all_cols(TableId::Damage);
        let cat0 = &vocab.impact_categories[0];
        let cat1 = &vocab.impact_categories[1];
        let tsv = format!(
            "D00\tDelayed Braking\t{}: {}\t0\n\
             NEW\tData Breach\t{}: {}; {}: {}\t0",
            cat0.label, cat0.levels[2], cat0.label, cat0.levels[1], cat1.label, cat1.levels[3]
        );
        let edit = paste_tsv(
            ws.project(),
            TableId::Damage,
            Owner::Project,
            &rows,
            &cols,
            0,
            0,
            &tsv,
            &vocab,
            &catalog,
        )
        .expect("paste damage");
        apply_edit(&mut ws, &edit);

        let p = ws.project();
        let m = p.methodology();
        let c0 = m.impact_categories().next().expect("cat0");
        let c1 = m.impact_categories().nth(1).expect("cat1");
        let d1 = p
            .damage_scenario(p.damage_scenario_idx(&dmg1).expect("d1"))
            .expect("slot");
        assert_eq!(d1.name(), "Delayed Braking");
        assert_eq!(d1.impact(c0), m.levels(c0).nth(2));
        assert_eq!(d1.impact(c1), None);

        let (_, _, appended) = p
            .damage_scenarios()
            .find(|(_, _, d)| d.name() == "Data Breach")
            .expect("appended damage");
        assert_eq!(appended.impact(c0), m.levels(c0).nth(1));
        assert_eq!(appended.impact(c1), m.levels(c1).nth(3));

        ws.undo().expect("undo damage paste");
        assert_eq!(save_bytes(&ws), before);
    }

    // 3. Threats: update existing (preserving scaling_reason & ignoring read-only asset) + append new.
    {
        let before = save_bytes(&ws);
        let rows = table_rows(&ws, TableId::Threats, &vocab, &catalog);
        let cols = all_cols(TableId::Threats);
        let tsv = "T00\tSpoof Wheel Speed\tKey Store\tYes\tHigh\tLow\n\
                   NEW\tExtract Keys\tKey Store\ttrue\t\t";
        let edit = paste_tsv(
            ws.project(),
            TableId::Threats,
            Owner::Project,
            &rows,
            &cols,
            0,
            0,
            tsv,
            &vocab,
            &catalog,
        )
        .expect("paste threats");
        apply_edit(&mut ws, &edit);

        let p = ws.project();
        let t1 = p
            .threat_scenario(p.threat_scenario_idx(&thr1).expect("t1"))
            .expect("slot");
        assert_eq!(t1.name(), "Spoof Wheel Speed");
        // Existing row's read-only THREAT_ASSET link was not mutated.
        assert_eq!(t1.asset(), p.asset_idx(&asset1).expect("a1 idx"));
        assert_eq!(t1.scales(), Some(true));
        assert_eq!(t1.scaling_reason(), "Single-vehicle physical tap");

        let (_, _, appended) = p
            .threat_scenarios()
            .find(|(_, _, t)| t.name() == "Extract Keys")
            .expect("appended threat");
        assert_eq!(appended.asset(), p.asset_idx(&asset2).expect("a2 idx"));
        assert_eq!(appended.scales(), Some(true));

        ws.undo().expect("undo threats paste");
        assert_eq!(save_bytes(&ws), before);
    }

    // 4. Risks: update existing treatment + append new risk with asset & treatment.
    {
        let before = save_bytes(&ws);
        let rows = table_rows(&ws, TableId::Risks, &vocab, &catalog);
        let cols = vec![
            col_by_key(TableId::Risks, "name"),
            col_by_key(TableId::Risks, "asset"),
            col_by_key(TableId::Risks, "treatment"),
        ];
        let tsv = "Spoof Speed\tKey Store\tReduce\n\
                   Tamper Calibration\tKey Store\tShare";
        let edit = paste_tsv(
            ws.project(),
            TableId::Risks,
            Owner::Project,
            &rows,
            &cols,
            0,
            0,
            tsv,
            &vocab,
            &catalog,
        )
        .expect("paste risks");
        apply_edit(&mut ws, &edit);

        let p = ws.project();
        let t1 = p
            .threat_scenario(p.threat_scenario_idx(&thr1).expect("t1"))
            .expect("slot");
        assert_eq!(t1.treatment(), Some(RiskTreatment::Reduce));
        assert_eq!(t1.asset(), p.asset_idx(&asset1).expect("a1 idx"));

        let (_, _, appended) = p
            .threat_scenarios()
            .find(|(_, _, t)| t.name() == "Tamper Calibration")
            .expect("appended risk");
        assert_eq!(appended.asset(), p.asset_idx(&asset2).expect("a2 idx"));
        assert_eq!(appended.treatment(), Some(RiskTreatment::Share));

        ws.undo().expect("undo risks paste");
        assert_eq!(save_bytes(&ws), before);
    }

    // 5. Controls: update existing + append new.
    {
        let before = save_bytes(&ws);
        let rows = table_rows(&ws, TableId::Controls, &vocab, &catalog);
        let cols = all_cols(TableId::Controls);
        let tsv = "C00\tSecOC MAC\t0\t0\t0\n\
                   NEW\tHardware Firewall\t0\t0\t0";
        let edit = paste_tsv(
            ws.project(),
            TableId::Controls,
            Owner::Project,
            &rows,
            &cols,
            0,
            0,
            tsv,
            &vocab,
            &catalog,
        )
        .expect("paste controls");
        apply_edit(&mut ws, &edit);

        let p = ws.project();
        let c1 = p
            .security_control(p.security_control_idx(&ctrl1).expect("c1"))
            .expect("slot");
        assert_eq!(c1.name(), "SecOC MAC");
        assert!(
            p.security_controls()
                .any(|(_, _, c)| c.name() == "Hardware Firewall")
        );

        ws.undo().expect("undo controls paste");
        assert_eq!(save_bytes(&ws), before);
    }

    // 6. Goals: update existing (name + responsible links) + append new with 2 parties.
    {
        let before = save_bytes(&ws);
        let rows = table_rows(&ws, TableId::Goals, &vocab, &catalog);
        let cols = all_cols(TableId::Goals);
        let tsv = "G00\tProtect Braking Bus\tOEM Security\t0\t0\n\
                   NEW\tAuthenticate Updates\tOEM Security, Tier-1 Brakes\t0\t0";
        let edit = paste_tsv(
            ws.project(),
            TableId::Goals,
            Owner::Project,
            &rows,
            &cols,
            0,
            0,
            tsv,
            &vocab,
            &catalog,
        )
        .expect("paste goals");
        apply_edit(&mut ws, &edit);

        let p = ws.project();
        let g1 = p
            .cybersecurity_goal(p.cybersecurity_goal_idx(&goal1).expect("g1"))
            .expect("slot");
        assert_eq!(g1.name(), "Protect Braking Bus");
        assert_eq!(
            relations::related(p, RelationKey::GOAL_RESPONSIBLE, Selection::Goal(goal1)),
            vec![Selection::Party(party1)]
        );
        let (_, new_goal_id, _) = p
            .cybersecurity_goals()
            .find(|(_, _, g)| g.name() == "Authenticate Updates")
            .expect("appended goal");
        assert_eq!(
            relations::related(
                p,
                RelationKey::GOAL_RESPONSIBLE,
                Selection::Goal(new_goal_id)
            ),
            vec![Selection::Party(party1), Selection::Party(party2)]
        );

        ws.undo().expect("undo goals paste");
        assert_eq!(save_bytes(&ws), before);
    }

    // 7. Claims: update existing (name + responsible links) + append new with 2 parties.
    {
        let before = save_bytes(&ws);
        let rows = table_rows(&ws, TableId::Claims, &vocab, &catalog);
        let cols = all_cols(TableId::Claims);
        let tsv = "CL00\tGateway Hardened\tTier-1 Brakes\t0\n\
                   NEW\tKeys Stored in HSM\tOEM Security; Tier-1 Brakes\t0";
        let edit = paste_tsv(
            ws.project(),
            TableId::Claims,
            Owner::Project,
            &rows,
            &cols,
            0,
            0,
            tsv,
            &vocab,
            &catalog,
        )
        .expect("paste claims");
        apply_edit(&mut ws, &edit);

        let p = ws.project();
        let cl1 = p
            .cybersecurity_claim(p.cybersecurity_claim_idx(&claim1).expect("cl1"))
            .expect("slot");
        assert_eq!(cl1.name(), "Gateway Hardened");
        assert_eq!(
            relations::related(p, RelationKey::CLAIM_RESPONSIBLE, Selection::Claim(claim1)),
            vec![Selection::Party(party2)]
        );
        let (_, new_claim_id, _) = p
            .cybersecurity_claims()
            .find(|(_, _, c)| c.name() == "Keys Stored in HSM")
            .expect("appended claim");
        assert_eq!(
            relations::related(
                p,
                RelationKey::CLAIM_RESPONSIBLE,
                Selection::Claim(new_claim_id)
            ),
            vec![Selection::Party(party1), Selection::Party(party2)]
        );

        ws.undo().expect("undo claims paste");
        assert_eq!(save_bytes(&ws), before);
    }
}

#[test]
fn paste_tsv_append_threat_on_non_category0_asset_picks_compatible_category() {
    let mut ws = empty_workspace();
    let catalog = Catalog::load(Language::En);
    let vocab = vocabulary(ws.project().methodology());

    // Find a property that excludes threat category 0 (e.g., Integrity-only).
    let (excluded_prop_idx, expected_cat) = {
        let m = ws.project().methodology();
        let cat0 = m.threat_categories().next().expect("cat0");
        let cat0_props = m.violates(cat0);
        let prop_idx = m
            .properties()
            .enumerate()
            .find(|(_, p)| !cat0_props.contains(*p))
            .map(|(i, _)| i)
            .expect("property not violated by category 0");
        let single = PropertySet::default().with(m.properties().nth(prop_idx).expect("prop"));
        let cat = m
            .threat_categories()
            .find(|&c| m.violates(c).iter().all(|p| single.contains(p)))
            .expect("compatible category for single property");
        assert_ne!(cat, cat0, "chosen property must exclude category 0");
        (prop_idx, cat)
    };

    let integrity_asset = add_asset(&mut ws, "Integrity Only Asset", vec![excluded_prop_idx]);
    let rows = table_rows(&ws, TableId::Threats, &vocab, &catalog);
    let cols = all_cols(TableId::Threats);
    let before = save_bytes(&ws);

    let tsv = "NEW\tTamper Firmware\tIntegrity Only Asset\tNo\t\t";
    let edit = paste_tsv(
        ws.project(),
        TableId::Threats,
        Owner::Project,
        &rows,
        &cols,
        0,
        0,
        tsv,
        &vocab,
        &catalog,
    )
    .expect("append threat on integrity-only asset");
    apply_edit(&mut ws, &edit);

    let p = ws.project();
    let (_, _, t) = p
        .threat_scenarios()
        .find(|(_, _, t)| t.name() == "Tamper Firmware")
        .expect("appended threat");
    assert_eq!(t.asset(), p.asset_idx(&integrity_asset).expect("asset idx"));
    assert_eq!(t.category(), expected_cat);
    assert_eq!(t.scales(), Some(false));

    ws.undo().expect("undo");
    assert_eq!(save_bytes(&ws), before);
}

#[test]
fn paste_tsv_treatments_exact_order_and_clear() {
    let mut ws = empty_workspace();
    let catalog = Catalog::load(Language::En);
    let vocab = vocabulary(ws.project().methodology());
    let _asset = add_asset(&mut ws, "CAN Bus", vec![0]);

    let t_ids: Vec<ThreatId> = ["T1", "T2", "T3", "T4"]
        .into_iter()
        .map(|name| match add_threat(&mut ws, name) {
            Selection::Threat(id) => id,
            _ => unreachable!(),
        })
        .collect();

    let rows = table_rows(&ws, TableId::Risks, &vocab, &catalog);
    let cols = vec![
        col_by_key(TableId::Risks, "name"),
        col_by_key(TableId::Risks, "treatment"),
    ];
    let before = save_bytes(&ws);

    // Update 4 existing rows with mixed case + append 4 new rows in TREATMENTS order.
    let tsv = "T1\tavoid\n\
               T2\tREDUCE\n\
               T3\tShare\n\
               T4\tRetain\n\
               N1\tAvoid\n\
               N2\tReduce\n\
               N3\tShare\n\
               N4\tRetain";
    let edit = paste_tsv(
        ws.project(),
        TableId::Risks,
        Owner::Project,
        &rows,
        &cols,
        0,
        0,
        tsv,
        &vocab,
        &catalog,
    )
    .expect("paste all four treatments");
    apply_edit(&mut ws, &edit);

    let expected = [
        RiskTreatment::Avoid,
        RiskTreatment::Reduce,
        RiskTreatment::Share,
        RiskTreatment::Retain,
    ];
    assert_eq!(TREATMENTS, expected);

    let p = ws.project();
    for (id, want) in t_ids.iter().zip(expected) {
        let idx = p.threat_scenario_idx(id).expect("threat idx");
        assert_eq!(
            p.threat_scenario(idx).expect("slot").treatment(),
            Some(want)
        );
    }
    for (name, want) in ["N1", "N2", "N3", "N4"].into_iter().zip(expected) {
        let (_, _, t) = p
            .threat_scenarios()
            .find(|(_, _, t)| t.name() == name)
            .expect("appended risk");
        assert_eq!(t.treatment(), Some(want));
    }

    // Clearing treatment with empty string on existing row sets it back to None.
    let rows_after = table_rows(&ws, TableId::Risks, &vocab, &catalog);
    let clear_edit = paste_tsv(
        ws.project(),
        TableId::Risks,
        Owner::Project,
        &rows_after,
        &[col_by_key(TableId::Risks, "treatment")],
        0,
        0,
        "",
        &vocab,
        &catalog,
    );
    // Note: pure "" is EmptyPaste; clearing requires a cell like "\t" or pasting name + empty treatment.
    assert_eq!(clear_edit, Err(GridError::EmptyPaste));
    let clear_with_name = paste_tsv(
        ws.project(),
        TableId::Risks,
        Owner::Project,
        &rows_after,
        &cols,
        0,
        0,
        "T1\t",
        &vocab,
        &catalog,
    )
    .expect("clear treatment with empty cell");
    apply_edit(&mut ws, &clear_with_name);
    let t0_idx = ws.project().threat_scenario_idx(&t_ids[0]).expect("t0");
    assert_eq!(
        ws.project()
            .threat_scenario(t0_idx)
            .expect("slot")
            .treatment(),
        None
    );

    ws.undo().expect("undo clear");
    ws.undo().expect("undo paste treatments");
    assert_eq!(save_bytes(&ws), before);
}

#[test]
fn paste_tsv_damage_impact_replacement_and_ja_locale_roundtrip() {
    let mut ws = empty_workspace();
    let catalog_en = Catalog::load(Language::En);
    let vocab_en = vocabulary(ws.project().methodology());
    let catalog_ja = Catalog::load(Language::Ja);
    let vocab_ja = vocabulary_for_lang(ws.project().methodology(), "ja");

    let Selection::Damage(dmg_id) = add_damage(&mut ws, "Braking Fault") else {
        unreachable!()
    };
    // Seed both Safety (cat 0) and Financial (cat 1).
    apply_edit(
        &mut ws,
        &Edit::Batch(vec![
            Edit::SetImpact {
                damage: dmg_id,
                category: 0,
                level: Some(3),
            },
            Edit::SetImpact {
                damage: dmg_id,
                category: 1,
                level: Some(2),
            },
        ]),
    );

    // 1. Replacement semantics: pasting only "Safety: Moderate" clears Financial.
    let before_replace = save_bytes(&ws);
    let rows = table_rows(&ws, TableId::Damage, &vocab_en, &catalog_en);
    let cols = all_cols(TableId::Damage);
    let edit = paste_tsv(
        ws.project(),
        TableId::Damage,
        Owner::Project,
        &rows,
        &cols,
        0,
        2,
        "Safety: Moderate",
        &vocab_en,
        &catalog_en,
    )
    .expect("replace impact");
    apply_edit(&mut ws, &edit);

    let p = ws.project();
    let m = p.methodology();
    let c0 = m.impact_categories().next().expect("Safety");
    let c1 = m.impact_categories().nth(1).expect("Financial");
    let d = p
        .damage_scenario(p.damage_scenario_idx(&dmg_id).expect("idx"))
        .expect("slot");
    let moderate_lvl = m
        .levels(c0)
        .find(|&l| m.level_label(c0, l) == Some("Moderate"))
        .expect("Moderate level");
    assert_eq!(d.impact(c0), Some(moderate_lvl));
    assert_eq!(
        d.impact(c1),
        None,
        "previous Financial impact must be cleared"
    );

    ws.undo().expect("undo impact replacement");
    assert_eq!(save_bytes(&ws), before_replace);

    // 2. Empty impact cell clears all categories on existing row.
    let clear_edit = paste_tsv(
        ws.project(),
        TableId::Damage,
        Owner::Project,
        &rows,
        &cols,
        0,
        1,
        "Braking Fault\t",
        &vocab_en,
        &catalog_en,
    )
    .expect("clear all impacts");
    apply_edit(&mut ws, &clear_edit);
    let p = ws.project();
    let d = p
        .damage_scenario(p.damage_scenario_idx(&dmg_id).expect("idx"))
        .expect("slot");
    assert!(
        p.methodology()
            .impact_categories()
            .all(|c| d.impact(c).is_none())
    );
    ws.undo().expect("undo clear all impacts");
    assert_eq!(save_bytes(&ws), before_replace);

    // 3. Localized Language::Ja round-trip: format_tsv -> paste_tsv succeeds on existing & appended rows!
    let rows_ja = table_rows(&ws, TableId::Damage, &vocab_ja, &catalog_ja);
    let vis_indices: Vec<usize> = (0..cols.len()).collect();
    let mut sel = GridSelection::single(0, 0);
    sel.extend_to(0, cols.len() - 1);
    let formatted_ja = format_tsv(&rows_ja, &vis_indices, sel);

    // Round-trip onto existing row -> identical no-op Edit::Batch(vec![]).
    let noop = paste_tsv(
        ws.project(),
        TableId::Damage,
        Owner::Project,
        &rows_ja,
        &cols,
        0,
        0,
        &formatted_ja,
        &vocab_ja,
        &catalog_ja,
    )
    .expect("ja round-trip existing row");
    assert_eq!(noop, Edit::Batch(vec![]));

    // Round-trip past end -> appends an identical copy of the damage scenario!
    let append_ja = paste_tsv(
        ws.project(),
        TableId::Damage,
        Owner::Project,
        &rows_ja,
        &cols,
        rows_ja.len(),
        0,
        &formatted_ja,
        &vocab_ja,
        &catalog_ja,
    )
    .expect("ja round-trip append row");
    apply_edit(&mut ws, &append_ja);
    assert_eq!(ws.project().damage_scenarios().count(), 2);
    let (_, _, copy) = ws
        .project()
        .damage_scenarios()
        .find(|(_, id, _)| *id != dmg_id)
        .expect("appended ja copy");
    let m = ws.project().methodology();
    let c0 = m.impact_categories().next().expect("cat0");
    let c1 = m.impact_categories().nth(1).expect("cat1");
    assert_eq!(copy.impact(c0), m.levels(c0).nth(3));
    assert_eq!(copy.impact(c1), m.levels(c1).nth(2));
    ws.undo().expect("undo ja append");
    assert_eq!(save_bytes(&ws), before_replace);

    // 4. Directly pasting Japanese vocabulary labels (for impact, properties, and scales) also works.
    let ja_cat0 = &vocab_ja.impact_categories[0];
    let ja_tsv = format!("新規ダメージ\t{}: {}", ja_cat0.label, ja_cat0.levels[2]);
    let ja_direct = paste_tsv(
        ws.project(),
        TableId::Damage,
        Owner::Project,
        &rows_ja,
        &cols,
        rows_ja.len(),
        1,
        &ja_tsv,
        &vocab_ja,
        &catalog_ja,
    )
    .expect("direct ja impact labels");
    apply_edit(&mut ws, &ja_direct);
    let (_, _, ja_dmg) = ws
        .project()
        .damage_scenarios()
        .find(|(_, _, d)| d.name() == "新規ダメージ")
        .expect("ja damage");
    assert_eq!(
        ja_dmg.impact(c0),
        ws.project().methodology().levels(c0).nth(2)
    );
    ws.undo().expect("undo direct ja damage");
    assert_eq!(save_bytes(&ws), before_replace);
}

#[test]
fn paste_tsv_rejects_multiple_tokens_on_1_to_1_links() {
    let mut ws = empty_workspace();
    let catalog = Catalog::load(Language::En);
    let vocab = vocabulary(ws.project().methodology());

    let c0 = add_component(&mut ws, "ECU Alpha", None);
    let c1 = add_component(&mut ws, "ECU Beta", None);
    let a0 = add_asset(&mut ws, "Asset Zero", vec![0]);
    let a1 = add_asset(&mut ws, "Asset One", vec![0]);
    let before = save_bytes(&ws);

    let c0_vis = selection_visual_id(ws.project(), Selection::Component(c0));
    let c1_vis = selection_visual_id(ws.project(), Selection::Component(c1));
    let a0_vis = selection_visual_id(ws.project(), Selection::Asset(a0));
    let a1_vis = selection_visual_id(ws.project(), Selection::Asset(a1));

    // Existing Asset row: >1 token in 1:1 ASSET_COMPONENT -> UnknownLink.
    let asset_rows = table_rows(&ws, TableId::Assets, &vocab, &catalog);
    let asset_cols = all_cols(TableId::Assets);
    let err = paste_tsv(
        ws.project(),
        TableId::Assets,
        Owner::Project,
        &asset_rows,
        &asset_cols,
        0,
        2,
        &format!("{c0_vis}, {c1_vis}"),
        &vocab,
        &catalog,
    );
    assert_eq!(
        err,
        Err(GridError::UnknownLink {
            row: 1,
            col: "component".into(),
            value: format!("{c0_vis}, {c1_vis}"),
        })
    );

    // Appended Asset row: >1 token in 1:1 ASSET_COMPONENT -> UnknownLink.
    let err = paste_tsv(
        ws.project(),
        TableId::Assets,
        Owner::Project,
        &asset_rows,
        &asset_cols,
        asset_rows.len(),
        1,
        &format!("New Asset\t{c0_vis}; {c1_vis}"),
        &vocab,
        &catalog,
    );
    assert_eq!(
        err,
        Err(GridError::UnknownLink {
            row: 1,
            col: "component".into(),
            value: format!("{c0_vis}; {c1_vis}"),
        })
    );

    // Appended Threat row: >1 token in 1:1 THREAT_ASSET -> UnknownLink.
    let threat_rows = table_rows(&ws, TableId::Threats, &vocab, &catalog);
    let threat_cols = all_cols(TableId::Threats);
    let err = paste_tsv(
        ws.project(),
        TableId::Threats,
        Owner::Project,
        &threat_rows,
        &threat_cols,
        0,
        1,
        &format!("New Threat\t{a0_vis}, {a1_vis}"),
        &vocab,
        &catalog,
    );
    assert_eq!(
        err,
        Err(GridError::UnknownLink {
            row: 1,
            col: "asset".into(),
            value: format!("{a0_vis}, {a1_vis}"),
        })
    );

    assert_eq!(save_bytes(&ws), before);
}

#[test]
fn paste_tsv_link_candidate_matching_precedence() {
    let mut ws = empty_workspace();
    let catalog = Catalog::load(Language::En);
    let vocab = vocabulary(ws.project().methodology());

    // Create two components where c0's name collides with c1's visual_id!
    let c0 = add_component(&mut ws, "Placeholder", None);
    let c1 = add_component(&mut ws, "Gateway", None);
    let c0_vis = selection_visual_id(ws.project(), Selection::Component(c0));
    let c1_vis = selection_visual_id(ws.project(), Selection::Component(c1));
    apply_edit(
        &mut ws,
        &Edit::RenameComponent {
            component: c0,
            name: c1_vis.clone(),
        },
    );

    let a1 = add_asset(&mut ws, "A1", vec![0]);
    let a2 = add_asset(&mut ws, "A2", vec![0]);
    let a3 = add_asset(&mut ws, "A3", vec![0]);

    let rows = table_rows(&ws, TableId::Assets, &vocab, &catalog);
    let cols = vec![col_by_key(TableId::Assets, "component")];
    let before = save_bytes(&ws);

    // Row 1: exact visual_id (lowercase) -> beats c0's name match -> resolves to c1.
    // Row 2: exact "{visual_id} — {name}" (case-insensitive) -> resolves to c0.
    // Row 3: exact name (case-insensitive) -> resolves to c1.
    let tsv = format!(
        "{}\n{} — {}\ngateway",
        c1_vis.to_lowercase(),
        c0_vis.to_lowercase(),
        c1_vis.to_lowercase()
    );
    let edit = paste_tsv(
        ws.project(),
        TableId::Assets,
        Owner::Project,
        &rows,
        &cols,
        0,
        0,
        &tsv,
        &vocab,
        &catalog,
    )
    .expect("precedence paste");
    apply_edit(&mut ws, &edit);

    let p = ws.project();
    let c0_idx = p.component_idx(&c0).expect("c0 idx");
    let c1_idx = p.component_idx(&c1).expect("c1 idx");
    assert_eq!(
        p.asset(p.asset_idx(&a1).expect("a1"))
            .expect("slot")
            .component(),
        Some(c1_idx),
        "exact visual_id must beat earlier exact name"
    );
    assert_eq!(
        p.asset(p.asset_idx(&a2).expect("a2"))
            .expect("slot")
            .component(),
        Some(c0_idx),
        "visual_id — name must resolve to c0"
    );
    assert_eq!(
        p.asset(p.asset_idx(&a3).expect("a3"))
            .expect("slot")
            .component(),
        Some(c1_idx),
        "case-insensitive exact name must resolve to c1"
    );

    ws.undo().expect("undo precedence");
    assert_eq!(save_bytes(&ws), before);
}

#[test]
fn paste_tsv_appended_empty_link_fallbacks_and_excess_trailing_columns() {
    let mut ws = empty_workspace();
    let catalog = Catalog::load(Language::En);
    let vocab = vocabulary(ws.project().methodology());

    let sys = add_system(&mut ws, "Powertrain");
    let sys_comp = add_component(&mut ws, "Engine ECU", Some(sys));

    // Append Asset under Owner::System(sys) with empty component cell + excess trailing columns.
    let before = save_bytes(&ws);
    let asset_rows =
        table_rows_for_owner(&ws, TableId::Assets, Owner::System(sys), &vocab, &catalog);
    let asset_cols = vec![
        col_by_key(TableId::Assets, "name"),
        col_by_key(TableId::Assets, "component"),
    ];
    let edit = paste_tsv(
        ws.project(),
        TableId::Assets,
        Owner::System(sys),
        &asset_rows,
        &asset_cols,
        0,
        0,
        "Fuel Map\t\textra1\textra2",
        &vocab,
        &catalog,
    )
    .expect("append asset with empty component fallback + excess cols");
    apply_edit(&mut ws, &edit);

    let p = ws.project();
    let (_, sys_asset_id, fuel_map) = p
        .assets()
        .find(|(_, _, a)| a.name() == "Fuel Map")
        .expect("appended Fuel Map");
    assert_eq!(
        fuel_map.component(),
        p.component_idx(&sys_comp),
        "empty component cell must fall back to system_component"
    );
    assert_eq!(
        fuel_map.properties(),
        prop_set(p, &[0]),
        "absent properties column must default to vec![0]"
    );

    // Update existing Asset row with excess trailing columns ignored silently.
    let asset_rows_now =
        table_rows_for_owner(&ws, TableId::Assets, Owner::System(sys), &vocab, &catalog);
    let update_edit = paste_tsv(
        ws.project(),
        TableId::Assets,
        Owner::System(sys),
        &asset_rows_now,
        &[col_by_key(TableId::Assets, "name")],
        0,
        0,
        "Fuel Map v2\tignored_col_1\tignored_col_2",
        &vocab,
        &catalog,
    )
    .expect("update existing row ignoring excess trailing columns");
    apply_edit(&mut ws, &update_edit);
    assert_eq!(
        ws.project()
            .asset(ws.project().asset_idx(&sys_asset_id).expect("idx"))
            .expect("slot")
            .name(),
        "Fuel Map v2"
    );

    // Append Threat under Owner::System(sys) with empty asset cell -> falls back to default_asset_for_owner.
    let threat_rows =
        table_rows_for_owner(&ws, TableId::Threats, Owner::System(sys), &vocab, &catalog);
    let threat_cols = vec![
        col_by_key(TableId::Threats, "name"),
        col_by_key(TableId::Threats, "asset"),
        col_by_key(TableId::Threats, "scales"),
    ];
    let thr_edit = paste_tsv(
        ws.project(),
        TableId::Threats,
        Owner::System(sys),
        &threat_rows,
        &threat_cols,
        0,
        0,
        "Tamper Fuel Map\t   \tYes\texcess_col",
        &vocab,
        &catalog,
    )
    .expect("append threat with empty asset fallback + excess cols");
    apply_edit(&mut ws, &thr_edit);

    let p = ws.project();
    let (_, _, thr) = p
        .threat_scenarios()
        .find(|(_, _, t)| t.name() == "Tamper Fuel Map")
        .expect("appended threat");
    assert_eq!(thr.asset(), p.asset_idx(&sys_asset_id).expect("asset idx"));
    assert_eq!(thr.scales(), Some(true));

    ws.undo().expect("undo threat");
    ws.undo().expect("undo rename");
    ws.undo().expect("undo asset");
    assert_eq!(save_bytes(&ws), before);
}

#[test]
fn paste_tsv_guards_return_exact_errors_and_truncate_untrusted_values() {
    let mut ws = empty_workspace();
    let catalog = Catalog::load(Language::En);
    let vocab = vocabulary(ws.project().methodology());
    let _asset = add_asset(&mut ws, "CAN Bus", vec![0]);
    let _dmg = add_damage(&mut ws, "Braking Loss");
    let _thr = add_threat(&mut ws, "Spoof Frame");
    let before = save_bytes(&ws);

    let bogus_100 = "z".repeat(100);
    let bogus_64 = "z".repeat(64);

    // 1. TableId::Problems -> NotEditable (valid TSV, only Problems guard triggers).
    assert_eq!(
        paste_tsv(
            ws.project(),
            TableId::Problems,
            Owner::Project,
            &[],
            &all_cols(TableId::Problems),
            0,
            0,
            "P00\tBlocking\tText",
            &vocab,
            &catalog,
        ),
        Err(GridError::NotEditable)
    );

    // 2. Invalid rating on row 2 with 100-char bogus token -> UnknownRating { row: 2, col, value <= 64 }.
    let asset_rows = table_rows(&ws, TableId::Assets, &vocab, &catalog);
    let asset_cols = all_cols(TableId::Assets);
    let p0 = &vocab.properties[0];
    let err = paste_tsv(
        ws.project(),
        TableId::Assets,
        Owner::Project,
        &asset_rows,
        &asset_cols,
        0,
        1,
        &format!("CAN Bus\t\t{p0}\nAppended\t\t{bogus_100}"),
        &vocab,
        &catalog,
    );
    assert_eq!(
        err,
        Err(GridError::UnknownRating {
            row: 2,
            col: "properties".into(),
            value: bogus_64.clone(),
        })
    );

    // 3. Invalid Damage impact (missing ':', unknown category, unknown level) with 100-char value.
    let dmg_rows = table_rows(&ws, TableId::Damage, &vocab, &catalog);
    let dmg_cols = all_cols(TableId::Damage);
    let err = paste_tsv(
        ws.project(),
        TableId::Damage,
        Owner::Project,
        &dmg_rows,
        &dmg_cols,
        0,
        2,
        &bogus_100,
        &vocab,
        &catalog,
    );
    assert_eq!(
        err,
        Err(GridError::UnknownRating {
            row: 1,
            col: "impact".into(),
            value: bogus_64.clone(),
        })
    );
    let err = paste_tsv(
        ws.project(),
        TableId::Damage,
        Owner::Project,
        &dmg_rows,
        &dmg_cols,
        0,
        2,
        "Safety: NotALevel",
        &vocab,
        &catalog,
    );
    assert_eq!(
        err,
        Err(GridError::UnknownRating {
            row: 1,
            col: "impact".into(),
            value: "NotALevel".into(),
        })
    );

    // 4. Invalid scales & invalid treatment.
    let thr_rows = table_rows(&ws, TableId::Threats, &vocab, &catalog);
    let thr_cols = all_cols(TableId::Threats);
    assert_eq!(
        paste_tsv(
            ws.project(),
            TableId::Threats,
            Owner::Project,
            &thr_rows,
            &thr_cols,
            0,
            3,
            "maybe",
            &vocab,
            &catalog,
        ),
        Err(GridError::UnknownRating {
            row: 1,
            col: "scales".into(),
            value: "maybe".into(),
        })
    );
    let risk_rows = table_rows(&ws, TableId::Risks, &vocab, &catalog);
    let risk_cols = vec![col_by_key(TableId::Risks, "treatment")];
    assert_eq!(
        paste_tsv(
            ws.project(),
            TableId::Risks,
            Owner::Project,
            &risk_rows,
            &risk_cols,
            0,
            0,
            "Ignore",
            &vocab,
            &catalog,
        ),
        Err(GridError::UnknownRating {
            row: 1,
            col: "treatment".into(),
            value: "Ignore".into(),
        })
    );

    // 5. Invalid link with 100-char token -> UnknownLink truncated to 64 chars.
    assert_eq!(
        paste_tsv(
            ws.project(),
            TableId::Assets,
            Owner::Project,
            &asset_rows,
            &asset_cols,
            0,
            2,
            &bogus_100,
            &vocab,
            &catalog,
        ),
        Err(GridError::UnknownLink {
            row: 1,
            col: "component".into(),
            value: bogus_64,
        })
    );

    // 6. Blank name on existing row 1 and appended row 2 -> EmptyName with exact 1-based row.
    assert_eq!(
        paste_tsv(
            ws.project(),
            TableId::Assets,
            Owner::Project,
            &asset_rows,
            &asset_cols,
            0,
            1,
            "   ",
            &vocab,
            &catalog,
        ),
        Err(GridError::EmptyName { row: 1 })
    );
    assert_eq!(
        paste_tsv(
            ws.project(),
            TableId::Assets,
            Owner::Project,
            &asset_rows,
            &asset_cols,
            0,
            1,
            &format!("Valid Name\t\t{p0}\n   \t\t{p0}"),
            &vocab,
            &catalog,
        ),
        Err(GridError::EmptyName { row: 2 })
    );

    // 7. Only computed columns or only blank appended rows -> EmptyPaste.
    assert_eq!(
        paste_tsv(
            ws.project(),
            TableId::Assets,
            Owner::Project,
            &asset_rows,
            &asset_cols,
            0,
            4,
            "42\tHigh",
            &vocab,
            &catalog,
        ),
        Err(GridError::EmptyPaste)
    );
    assert_eq!(
        paste_tsv(
            ws.project(),
            TableId::Assets,
            Owner::Project,
            &asset_rows,
            &asset_cols,
            asset_rows.len(),
            0,
            "   \t  \n\t ",
            &vocab,
            &catalog,
        ),
        Err(GridError::EmptyPaste)
    );

    // 8. Appending a threat with no asset in scope -> NoAssetForThreat.
    let empty_ws = empty_workspace();
    assert_eq!(
        paste_tsv(
            empty_ws.project(),
            TableId::Threats,
            Owner::Project,
            &[],
            &thr_cols,
            0,
            1,
            "Orphan Threat",
            &vocab,
            &catalog,
        ),
        Err(GridError::NoAssetForThreat)
    );

    // Project remained completely untouched through all guard failures.
    assert_eq!(save_bytes(&ws), before);
}

#[test]
fn paste_tsv_format_tsv_multi_column_roundtrip() {
    let mut ws = empty_workspace();
    let catalog = Catalog::load(Language::En);
    let vocab = vocabulary(ws.project().methodology());

    let comp = add_component(&mut ws, "Telematics ECU", None);
    let a1 = AssetId::new_v7();
    apply_edit(
        &mut ws,
        &Edit::AddAssetBound {
            id: a1,
            name: "Cellular Modem".into(),
            properties: vec![0, 1, 2],
            component: comp,
        },
    );
    let a2 = add_asset(&mut ws, "OTA Key", vec![0, 3]);

    let rows = table_rows(&ws, TableId::Assets, &vocab, &catalog);
    let cols = all_cols(TableId::Assets);
    let vis_indices: Vec<usize> = (0..cols.len()).collect();
    let mut sel = GridSelection::single(0, 0);
    sel.extend_to(rows.len() - 1, cols.len() - 1);
    let tsv = format_tsv(&rows, &vis_indices, sel);

    // Round-trip onto existing rows -> zero changes.
    let noop = paste_tsv(
        ws.project(),
        TableId::Assets,
        Owner::Project,
        &rows,
        &cols,
        0,
        0,
        &tsv,
        &vocab,
        &catalog,
    )
    .expect("multi-column round-trip existing");
    assert_eq!(noop, Edit::Batch(vec![]));

    // Round-trip appended past the end -> duplicates both rows faithfully, undone in 1 step.
    let before = save_bytes(&ws);
    let append = paste_tsv(
        ws.project(),
        TableId::Assets,
        Owner::Project,
        &rows,
        &cols,
        rows.len(),
        0,
        &tsv,
        &vocab,
        &catalog,
    )
    .expect("multi-column round-trip append");
    apply_edit(&mut ws, &append);

    let p = ws.project();
    assert_eq!(p.assets().count(), 4);
    let modem_copies: Vec<_> = p
        .assets()
        .filter(|(_, id, a)| *id != a1 && a.name() == "Cellular Modem")
        .collect();
    assert_eq!(modem_copies.len(), 1);
    assert_eq!(modem_copies[0].2.component(), p.component_idx(&comp));
    assert_eq!(modem_copies[0].2.properties(), prop_set(p, &[0, 1, 2]));

    let key_copies: Vec<_> = p
        .assets()
        .filter(|(_, id, a)| *id != a2 && a.name() == "OTA Key")
        .collect();
    assert_eq!(key_copies.len(), 1);
    assert_eq!(key_copies[0].2.component(), None);
    assert_eq!(key_copies[0].2.properties(), prop_set(p, &[0, 3]));

    ws.undo().expect("single undo restores multi-column append");
    assert_eq!(save_bytes(&ws), before);
}
