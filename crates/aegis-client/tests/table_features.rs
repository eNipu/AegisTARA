//! Integration tests for table link chips, applicability exclusions, owner
//! scoping, column filters, chooser/developer settings, and source JSON (189a).

use std::collections::HashSet;
use std::sync::Arc;

use aegis_client::commands::Edit;
use aegis_client::explorer::{Folder, Owner};
use aegis_client::i18n::{Catalog, Language};
use aegis_client::pages::{self, Vocabulary};
use aegis_client::relations;
use aegis_client::settings::Settings;
use aegis_client::table::{
    table_for_folder, table_key, table_rows, table_rows_for_owner, table_source_json,
};
use aegis_client::table_filter::{MAX_FILTER_CHARS, TableFilters};
use aegis_client::view::{ColumnKind, Selection, TableId};
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{
    AssetId, ClaimId, ComponentId, ConditionId, ControlId, DamageId, ElementId, ElementRef, GoalId,
    ItemRef, Methodology, Op, PartyId, PartyKind, Project, Provenance, SharedStepId, SystemSet,
    SystemUse, ThreatId, TreeId,
};

fn prov() -> Provenance {
    Provenance::human("mira", 1_792_956_600_000)
}

fn run(ws: &mut Workspace, edit: &Edit) {
    let ops = edit
        .to_ops(ws.project(), "mira", 1_792_956_600_000)
        .expect("edit builds");
    ws.apply(ops, Origin::Edit).expect("edit applies");
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

fn add_component(ws: &mut Workspace, name: &str, system: Option<ElementId>) -> ComponentId {
    let id = ComponentId::new_v7();
    run(
        ws,
        &Edit::AddComponent {
            id,
            name: name.into(),
            parent: None,
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
    s1: ElementId,
    s2: ElementId,
    a1: AssetId,
    a2: AssetId,
    a3: AssetId,
    t1: ThreatId,
    t2: ThreatId,
    t3: ThreatId,
    goal: GoalId,
    claim: ClaimId,
    cond: ConditionId,
    shared1: SharedStepId,
    shared2: SharedStepId,
}

fn fixture() -> Fx {
    let mut ws = Workspace::empty(Arc::new(Methodology::iso21434_r155().expect("builtin")));
    let s1 = add_system(&mut ws, "Brake");
    let s2 = add_system(&mut ws, "ADAS");
    let caliper = add_component(&mut ws, "Caliper", Some(s1));
    let camera = add_component(&mut ws, "Camera", Some(s2));
    let loose = add_component(&mut ws, "Loose", None);

    let a1 = add_asset(&mut ws, "Brake key", caliper);
    let a2 = add_asset(&mut ws, "鍵ストア Camera", camera);
    let a3 = add_asset(&mut ws, "Loose asset", loose);

    let d1 = DamageId::new_v7();
    let d2 = DamageId::new_v7();
    run(
        &mut ws,
        &Edit::AddDamage {
            id: d1,
            name: "Loss of braking".into(),
        },
    );
    run(
        &mut ws,
        &Edit::AddDamage {
            id: d2,
            name: "Unassigned damage".into(),
        },
    );

    let t1 = add_threat(&mut ws, "Spoof brake", a1, d1);
    let t2 = add_threat(&mut ws, "Blinding camera", a2, d1);
    let t3 = add_threat(&mut ws, "Tamper loose", a3, d2);

    let shared1 = SharedStepId::new_v7();
    let shared2 = SharedStepId::new_v7();
    for (id, name, via) in [
        (shared1, "Replay brake frame", caliper),
        (shared2, "Blind the camera", camera),
    ] {
        run(
            &mut ws,
            &Edit::AddSharedStep {
                id,
                name: name.into(),
            },
        );
        run(
            &mut ws,
            &Edit::SetSharedStepVia {
                shared_step: id,
                via: Some(ItemRef::Component(via)),
            },
        );
    }

    run(
        &mut ws,
        &Edit::NewTree {
            id: TreeId::new_v7(),
            root: "Spoof caliper".into(),
            threat: Some(t1),
        },
    );
    run(
        &mut ws,
        &Edit::NewFragment {
            id: TreeId::new_v7(),
            root: "Extract shared key".into(),
        },
    );

    let control = ControlId::new_v7();
    run(
        &mut ws,
        &Edit::AddControl {
            id: control,
            name: "MAC verification".into(),
        },
    );
    run(
        &mut ws,
        &Edit::ApplyControl {
            control,
            target: ItemRef::Component(caliper),
        },
    );

    let (goal, claim) = (GoalId::new_v7(), ClaimId::new_v7());
    let (p1, p2) = (PartyId::new_v7(), PartyId::new_v7());
    run(
        &mut ws,
        &Edit::AddGoal {
            id: goal,
            name: "Authenticate brake frames".into(),
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
            id: claim,
            name: "Camera sealed in housing".into(),
        },
    );
    run(
        &mut ws,
        &Edit::SetThreatClaim {
            threat: t2,
            claim: Some(claim),
        },
    );

    let cond = ConditionId::new_v7();
    ws.apply(
        vec![
            Op::AddParty {
                id: p1,
                name: "Bosch".into(),
                kind: PartyKind::Supplier,
                provenance: prov(),
            },
            Op::AddParty {
                id: p2,
                name: "Conti".into(),
                kind: PartyKind::Supplier,
                provenance: prov(),
            },
            Op::AddCondition {
                id: cond,
                name: "Lidar fitted".into(),
                description: "Optional sensor".into(),
                present: false,
                provenance: prov(),
            },
            Op::NeedCondition {
                element: ElementRef::Asset(a1),
                condition: cond,
                provenance: prov(),
            },
            Op::NeedCondition {
                element: ElementRef::Threat(t2),
                condition: cond,
                provenance: prov(),
            },
            Op::NeedCondition {
                element: ElementRef::Goal(goal),
                condition: cond,
                provenance: prov(),
            },
            Op::NeedCondition {
                element: ElementRef::Claim(claim),
                condition: cond,
                provenance: prov(),
            },
        ],
        Origin::Edit,
    )
    .expect("parties and conditions apply");

    run(
        &mut ws,
        &Edit::LinkGoalParty {
            goal,
            party: p1,
            linked: true,
        },
    );
    run(
        &mut ws,
        &Edit::LinkClaimParty {
            claim,
            party: p2,
            linked: true,
        },
    );

    Fx {
        ws,
        s1,
        s2,
        a1,
        a2,
        a3,
        t1,
        t2,
        t3,
        goal,
        claim,
        cond,
        shared1,
        shared2,
    }
}

fn english(ws: &Workspace) -> (Vocabulary, Catalog) {
    (
        pages::vocabulary(ws.project().methodology()),
        Catalog::load(Language::En),
    )
}

#[test]
fn link_chips_match_relation_registry_across_all_tables() {
    let fx = fixture();
    let (vocab, catalog) = english(&fx.ws);
    let project = fx.ws.project();
    let mut seen_links = HashSet::new();

    for table in TableId::ALL {
        let rows = table_rows(&fx.ws, table, &vocab, &catalog);
        assert!(!rows.is_empty(), "{table:?} must be populated");
        for row in &rows {
            for (col, cell) in table.columns().iter().zip(&row.cells) {
                match col.kind {
                    ColumnKind::Links(rel_key) => {
                        let from = row.target.expect("links row has target");
                        let expected_targets = relations::related(project, rel_key, from);
                        let expected_vids: Vec<String> = expected_targets
                            .iter()
                            .map(|&t| pages::selection_visual_id(project, t))
                            .collect();
                        let actual_targets: Vec<Selection> =
                            cell.chips.iter().map(|c| c.target).collect();
                        let actual_vids: Vec<String> =
                            cell.chips.iter().map(|c| c.visual_id.clone()).collect();
                        assert_eq!(actual_targets, expected_targets, "{table:?}.{}", col.key);
                        assert_eq!(actual_vids, expected_vids, "{table:?}.{}", col.key);
                        for chip in &cell.chips {
                            assert!(!chip.label.is_empty(), "{table:?}.{} empty label", col.key);
                        }
                        if !cell.chips.is_empty() {
                            seen_links.insert((table, col.key));
                        }
                    }
                    _ => assert!(
                        cell.chips.is_empty(),
                        "{table:?}.{} non-Links column must have no chips",
                        col.key
                    ),
                }
            }
        }
    }

    for expected in [
        (TableId::Assets, "component"),
        (TableId::Threats, "asset"),
        (TableId::Risks, "asset"),
        (TableId::Goals, "responsible"),
        (TableId::Claims, "responsible"),
    ] {
        assert!(
            seen_links.contains(&expected),
            "expected populated chips for {expected:?}"
        );
    }

    // Verify chip labels resolve to actual element names (not just non-empty).
    let goal_rows = table_rows(&fx.ws, TableId::Goals, &vocab, &catalog);
    let resp_ix = TableId::Goals
        .columns()
        .iter()
        .position(|c| c.key == "responsible")
        .expect("responsible col");
    assert_eq!(goal_rows[0].cells[resp_ix].chips[0].label, "Bosch");
    let claim_rows = table_rows(&fx.ws, TableId::Claims, &vocab, &catalog);
    let claim_resp_ix = TableId::Claims
        .columns()
        .iter()
        .position(|c| c.key == "responsible")
        .expect("responsible col");
    assert_eq!(claim_rows[0].cells[claim_resp_ix].chips[0].label, "Conti");
}

#[test]
fn excluded_rows_reflect_off_condition_and_clear_when_enabled() {
    let mut fx = fixture();
    let (vocab, catalog) = english(&fx.ws);
    let cond_vid = pages::selection_visual_id(fx.ws.project(), Selection::Condition(fx.cond));
    assert!(!cond_vid.is_empty());

    let check_reason = |rows: &[aegis_client::table::TableRow], target: Selection| {
        let row = rows
            .iter()
            .find(|r| r.target == Some(target))
            .expect("target row");
        assert!(row.excluded, "{target:?} should be excluded");
        let reason = row.excluded_reason.as_deref().expect("excluded reason");
        assert!(
            reason.contains(&cond_vid) && reason.contains("Lidar fitted"),
            "reason `{reason}` should name `{cond_vid}` and `Lidar fitted`"
        );
    };

    let assets = table_rows(&fx.ws, TableId::Assets, &vocab, &catalog);
    check_reason(&assets, Selection::Asset(fx.a1));
    for keep in [fx.a2, fx.a3] {
        let row = assets
            .iter()
            .find(|r| r.target == Some(Selection::Asset(keep)))
            .expect("asset row");
        assert!(!row.excluded);
        assert_eq!(row.excluded_reason, None);
    }

    let threats = table_rows(&fx.ws, TableId::Threats, &vocab, &catalog);
    check_reason(&threats, Selection::Threat(fx.t2));
    // t1 inherits exclusion from its excluded asset a1.
    let t1_row = threats
        .iter()
        .find(|r| r.target == Some(Selection::Threat(fx.t1)))
        .expect("t1 row");
    assert!(t1_row.excluded);
    assert!(t1_row.excluded_reason.is_some());
    let t3_row = threats
        .iter()
        .find(|r| r.target == Some(Selection::Threat(fx.t3)))
        .expect("t3 row");
    assert!(!t3_row.excluded);
    assert_eq!(t3_row.excluded_reason, None);

    let risks = table_rows(&fx.ws, TableId::Risks, &vocab, &catalog);
    check_reason(&risks, Selection::Threat(fx.t2));

    let goals = table_rows(&fx.ws, TableId::Goals, &vocab, &catalog);
    check_reason(&goals, Selection::Goal(fx.goal));

    let claims = table_rows(&fx.ws, TableId::Claims, &vocab, &catalog);
    check_reason(&claims, Selection::Claim(fx.claim));

    // Switching the condition on clears every exclusion.
    fx.ws
        .apply(
            vec![Op::SetConditionPresent {
                id: fx.cond,
                present: true,
                provenance: prov(),
            }],
            Origin::Edit,
        )
        .expect("condition switched on");

    for table in TableId::ALL {
        for row in table_rows(&fx.ws, table, &vocab, &catalog) {
            assert!(!row.excluded, "{table:?} row still excluded");
            assert_eq!(row.excluded_reason, None, "{table:?} stale reason");
        }
    }
}

fn independent_system_set(p: &Project, u: &SystemUse, s: Selection) -> Option<SystemSet> {
    match s {
        Selection::Component(id) => p.component_idx(&id).map(|i| u.component(i)),
        Selection::Interface(id) => p.interface_idx(&id).map(|i| u.interface(i)),
        Selection::DataFlow(id) => p.data_flow_idx(&id).map(|i| u.data_flow(i)),
        Selection::TrustBoundary(id) => p.trust_boundary_idx(&id).map(|i| u.trust_boundary(i)),
        Selection::Asset(id) => p.asset_idx(&id).map(|i| u.asset(i)),
        Selection::Damage(id) => p.damage_scenario_idx(&id).map(|i| u.damage(i)),
        Selection::Threat(id) => p.threat_scenario_idx(&id).map(|i| u.threat(i)),
        Selection::Tree(id) | Selection::TreeNode(id, _) => {
            p.attack_tree_idx(&id).map(|i| u.tree(i))
        }
        Selection::Control(id) => p.security_control_idx(&id).map(|i| u.control(i)),
        Selection::Goal(id) => p.cybersecurity_goal_idx(&id).map(|i| u.goal(i)),
        Selection::Claim(id) => p.cybersecurity_claim_idx(&id).map(|i| u.claim(i)),
        Selection::SharedStep(id) => p.shared_step_idx(&id).map(|i| u.shared_step(i)),
        Selection::Party(_) | Selection::Condition(_) | Selection::Assumption(_) => None,
    }
}

fn is_shared(s: Selection) -> bool {
    matches!(
        s,
        Selection::Damage(_)
            | Selection::Control(_)
            | Selection::Goal(_)
            | Selection::Claim(_)
            | Selection::SharedStep(_)
    )
}

#[test]
fn owner_filtering_matches_system_use_across_all_nine_tables() {
    let fx = fixture();
    let (vocab, catalog) = english(&fx.ws);
    let project = fx.ws.project();
    let usage = project.system_use();
    let owners = [
        Owner::Project,
        Owner::System(fx.s1),
        Owner::System(fx.s2),
        Owner::Unassigned,
    ];

    for table in TableId::ALL {
        let all_rows = table_rows(&fx.ws, table, &vocab, &catalog);
        assert!(!all_rows.is_empty(), "{table:?} has rows");
        for owner in owners {
            let scoped = table_rows_for_owner(&fx.ws, table, owner, &vocab, &catalog);
            let expected_targets: Vec<Option<Selection>> = all_rows
                .iter()
                .filter(|r| match owner {
                    Owner::Project => true,
                    Owner::System(sys) => {
                        if table == TableId::Fragments {
                            false
                        } else {
                            let set = usage.set_of([sys]);
                            r.target
                                .and_then(|s| independent_system_set(project, &usage, s))
                                .is_some_and(|m| m.intersects(set))
                        }
                    }
                    Owner::Unassigned => {
                        if matches!(
                            table,
                            TableId::Damage
                                | TableId::Controls
                                | TableId::Goals
                                | TableId::Claims
                                | TableId::SharedSteps
                                | TableId::Fragments
                        ) {
                            false
                        } else {
                            r.target.is_some_and(|s| {
                                !is_shared(s)
                                    && independent_system_set(project, &usage, s)
                                        .is_some_and(SystemSet::is_empty)
                            })
                        }
                    }
                })
                .map(|r| r.target)
                .collect();
            let actual_targets: Vec<Option<Selection>> = scoped.iter().map(|r| r.target).collect();
            assert_eq!(actual_targets, expected_targets, "{table:?} / {owner:?}");
        }
    }

    // Spot-check concrete system separation and shared-table unassigned emptiness.
    assert_eq!(
        table_rows_for_owner(
            &fx.ws,
            TableId::Assets,
            Owner::System(fx.s1),
            &vocab,
            &catalog
        )
        .len(),
        1
    );
    assert_eq!(
        table_rows_for_owner(
            &fx.ws,
            TableId::Assets,
            Owner::System(fx.s2),
            &vocab,
            &catalog
        )
        .len(),
        1
    );
    assert_eq!(
        table_rows_for_owner(&fx.ws, TableId::Assets, Owner::Unassigned, &vocab, &catalog).len(),
        1
    );
    for owner in [
        Owner::System(fx.s1),
        Owner::System(fx.s2),
        Owner::Unassigned,
    ] {
        assert!(
            !table_rows_for_owner(&fx.ws, TableId::Problems, owner, &vocab, &catalog).is_empty(),
            "Problems should have findings for {owner:?}"
        );
    }
    for shared in [
        TableId::Damage,
        TableId::Controls,
        TableId::Goals,
        TableId::Claims,
        TableId::SharedSteps,
    ] {
        assert!(
            table_rows_for_owner(&fx.ws, shared, Owner::Unassigned, &vocab, &catalog).is_empty(),
            "{shared:?} must be empty for Owner::Unassigned"
        );
    }

    assert_eq!(
        table_rows_for_owner(
            &fx.ws,
            TableId::SharedSteps,
            Owner::System(fx.s1),
            &vocab,
            &catalog
        )
        .iter()
        .map(|r| r.target)
        .collect::<Vec<_>>(),
        vec![Some(Selection::SharedStep(fx.shared1))]
    );
    assert_eq!(
        table_rows_for_owner(
            &fx.ws,
            TableId::SharedSteps,
            Owner::System(fx.s2),
            &vocab,
            &catalog
        )
        .iter()
        .map(|r| r.target)
        .collect::<Vec<_>>(),
        vec![Some(Selection::SharedStep(fx.shared2))]
    );
    assert_eq!(
        table_rows_for_owner(
            &fx.ws,
            TableId::SharedSteps,
            Owner::Project,
            &vocab,
            &catalog
        )
        .len(),
        2
    );

    // Folder <-> TableId and table_key mappings.
    let pairs = [
        (Folder::Assets, TableId::Assets, "assets"),
        (Folder::Damage, TableId::Damage, "damage"),
        (Folder::Threats, TableId::Threats, "threats"),
        (Folder::Risks, TableId::Risks, "risks"),
        (Folder::Controls, TableId::Controls, "controls"),
        (Folder::Goals, TableId::Goals, "goals"),
        (Folder::Claims, TableId::Claims, "claims"),
        (Folder::Problems, TableId::Problems, "problems"),
        (Folder::SharedSteps, TableId::SharedSteps, "shared-steps"),
        (Folder::Fragments, TableId::Fragments, "fragments"),
    ];
    for (folder, table, key) in pairs {
        assert_eq!(table_for_folder(folder), Some(table));
        assert_eq!(table_key(table), key);
    }
    for non_table in [
        Folder::Overview,
        Folder::Systems,
        Folder::Components,
        Folder::Trees,
        Folder::Parties,
    ] {
        assert_eq!(table_for_folder(non_table), None);
    }
}

#[test]
fn table_filters_cover_ascii_cjk_chips_values_and_multi_column_and() {
    let fx = fixture();
    let (vocab, catalog) = english(&fx.ws);
    let cols = TableId::Assets.columns();
    let rows = table_rows(&fx.ws, TableId::Assets, &vocab, &catalog);
    assert_eq!(rows.len(), 3);

    let mut filters = TableFilters::default();
    assert!(!filters.is_active());
    assert_eq!(filters.matching_rows(&rows, cols), vec![0, 1, 2]);

    // ASCII case-insensitive substring query.
    filters.set_query("name", "BRAKE");
    assert!(filters.is_active());
    assert_eq!(filters.active_keys(), vec!["name"]);
    let matched = filters.matching_rows(&rows, cols);
    assert_eq!(matched.len(), 1);
    assert_eq!(rows[matched[0]].target, Some(Selection::Asset(fx.a1)));

    // CJK substring query.
    filters.set_query("name", "鍵ストア");
    let matched = filters.matching_rows(&rows, cols);
    assert_eq!(matched.len(), 1);
    assert_eq!(rows[matched[0]].target, Some(Selection::Asset(fx.a2)));

    // Chip visual_id match on `component` column (where cell.text is component name).
    let comp_col_ix = cols
        .iter()
        .position(|c| c.key == "component")
        .expect("component col");
    let a1_ix = rows
        .iter()
        .position(|r| r.target == Some(Selection::Asset(fx.a1)))
        .expect("a1 row");
    let chip_vid = rows[a1_ix].cells[comp_col_ix].chips[0].visual_id.clone();
    assert!(!rows[a1_ix].cells[comp_col_ix].text.contains(&chip_vid));

    filters.clear_all();
    assert!(!filters.is_active());
    filters.set_query("component", chip_vid.to_lowercase());
    assert_eq!(filters.matching_rows(&rows, cols), vec![a1_ix]);

    // Value-set toggle + distinct_values.
    let distinct = TableFilters::distinct_values(&rows, comp_col_ix);
    assert_eq!(distinct, vec!["Caliper", "Camera", "Loose"]);

    filters.clear_all();
    filters.toggle_value("component", "Caliper".into());
    filters.toggle_value("component", "Camera".into());
    assert_eq!(filters.matching_rows(&rows, cols).len(), 2);

    // Multi-column AND narrows to intersection.
    filters.set_query("name", "camera");
    let matched = filters.matching_rows(&rows, cols);
    assert_eq!(matched.len(), 1);
    assert_eq!(rows[matched[0]].target, Some(Selection::Asset(fx.a2)));
    assert_eq!(filters.active_keys(), vec!["component", "name"]);

    // clear_column removes only one column.
    filters.clear_column("name");
    assert_eq!(filters.active_keys(), vec!["component"]);
    assert_eq!(filters.matching_rows(&rows, cols).len(), 2);

    // Toggling off the remaining values drops the empty column entry.
    filters.toggle_value("component", "Caliper".into());
    filters.toggle_value("component", "Camera".into());
    assert!(!filters.is_active());
    assert_eq!(filters.column("component"), None);

    // Char cap truncates to MAX_FILTER_CHARS on Unicode boundary.
    let long = "あ".repeat(MAX_FILTER_CHARS + 20);
    filters.set_query("name", long);
    let stored = filters.column("name").expect("name filter").query();
    assert_eq!(stored.chars().count(), MAX_FILTER_CHARS);

    filters.clear_all();
    assert!(!filters.is_active());
}

#[test]
fn settings_column_chooser_and_developer_source_view_round_trip() {
    let mut settings = Settings::default();
    assert!(!settings.developer_source_view);

    for table in TableId::ALL {
        let default_keys: Vec<&str> = table
            .columns()
            .iter()
            .filter(|c| c.visible)
            .map(|c| c.key)
            .collect();
        let actual_keys: Vec<&str> = settings
            .visible_columns(table)
            .into_iter()
            .map(|c| c.key)
            .collect();
        assert_eq!(actual_keys, default_keys, "{table:?}");
    }

    // Show hidden `component`, hide editable `properties`, try hiding `id` & unknown key.
    settings.set_column_visible(TableId::Assets, "component", true);
    settings.set_column_visible(TableId::Assets, "properties", false);
    settings.set_column_visible(TableId::Assets, "id", false);
    settings.set_column_visible(TableId::Assets, "no-such-column", true);

    let expected = vec!["id", "name", "component", "threats", "worst-risk"];
    let actual: Vec<&str> = settings
        .visible_columns(TableId::Assets)
        .into_iter()
        .map(|c| c.key)
        .collect();
    assert_eq!(actual, expected);

    settings.developer_source_view = true;

    let dir = std::env::temp_dir().join(format!("aegis-table-settings-{}", std::process::id()));
    let path = Settings::path_in(&dir);
    settings.save(&path).expect("save settings");
    let loaded = Settings::load(&path);
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(loaded, settings);
    assert!(loaded.developer_source_view);
    let loaded_keys: Vec<&str> = loaded
        .visible_columns(TableId::Assets)
        .into_iter()
        .map(|c| c.key)
        .collect();
    assert_eq!(loaded_keys, expected);

    // Even if a stored settings file omits `id`, `visible_columns` always keeps `ColumnKind::Id`.
    let manual: Settings =
        serde_json::from_str(r#"{"table_columns":{"assets":["name"]}}"#).expect("parse");
    let manual_keys: Vec<&str> = manual
        .visible_columns(TableId::Assets)
        .into_iter()
        .map(|c| c.key)
        .collect();
    assert_eq!(manual_keys, vec!["id", "name"]);
}

#[test]
fn table_source_json_matches_workspace_files_and_handles_problems() {
    let fx = fixture();
    let project = fx.ws.project();
    let files = project.to_workspace_files().expect("workspace files");

    let paths = [
        (TableId::Assets, "systems/main/assets.json"),
        (TableId::Damage, "shared/damage.json"),
        (TableId::Threats, "systems/main/threats.json"),
        (TableId::Risks, "systems/main/threats.json"),
        (TableId::Controls, "shared/controls.json"),
        (TableId::Goals, "shared/goals.json"),
        (TableId::Claims, "shared/claims.json"),
        (TableId::SharedSteps, "shared/steps.json"),
    ];
    for (table, expected_path) in paths {
        let file = files
            .iter()
            .find(|f| f.path == expected_path)
            .unwrap_or_else(|| panic!("missing workspace file {expected_path}"));
        let expected_json = String::from_utf8(file.bytes.clone()).expect("valid utf8");
        assert_eq!(
            table_source_json(project, table),
            expected_json,
            "{table:?}"
        );
    }
    assert_eq!(table_source_json(project, TableId::Problems), "{}\n");

    // Empty project returns a valid empty JSON envelope for absent files.
    let empty = Workspace::empty(Arc::new(Methodology::iso21434_r155().expect("builtin")));
    for (table, _) in paths {
        let json = table_source_json(empty.project(), table);
        let val: serde_json::Value = serde_json::from_str(&json).expect("valid json");
        assert_eq!(val["format"], "aegis-workspace", "{table:?}");
        assert_eq!(val["version"], 1, "{table:?}");
        let field = if table == TableId::SharedSteps {
            assert_eq!(val["kind"], "shared-steps", "{table:?}");
            "shared_steps"
        } else {
            "items"
        };
        assert_eq!(val[field], serde_json::json!([]), "{table:?}");
    }
    assert_eq!(
        table_source_json(empty.project(), TableId::Problems),
        "{}\n"
    );
}
