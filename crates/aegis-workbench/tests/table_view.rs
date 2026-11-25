//! Headless GPUI tests for `TableTab` core state, Settings-backed column
//! visibility, and `Workbench` folder wiring (UI-14).
#![cfg(any(target_os = "macos", target_os = "windows"))]

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use aegis_client::commands::Edit;
use aegis_client::explorer::{ExplorerKey, Folder, Owner};
use aegis_client::grid::GridSelection;
use aegis_client::i18n::{Catalog, Language};
use aegis_client::pages::vocabulary_for_lang;
use aegis_client::settings::{Appearance, Settings};
use aegis_client::systems::system_rows;
use aegis_client::table::{Tone, table_rows_for_owner, table_source_json};
use aegis_client::table_filter::ColumnFilter;
use aegis_client::view::{ColumnKind, Selection, TableId};
use aegis_model::{
    AssetId, ClaimId, ComponentId, ConditionId, ElementId, ElementRef, GoalId, Methodology, Op,
    PartyId, PartyKind, Project, Provenance, SharedStepId,
};
use aegis_workbench::app::{Workbench, apply_appearance, bind_all};
use aegis_workbench::sample::workbench_sample;
use gpui::{AppContext as _, Entity, TestAppContext, VisualTestContext};

const TABLE_FOLDERS: [(TableId, Folder); 9] = [
    (TableId::Assets, Folder::Assets),
    (TableId::Damage, Folder::Damage),
    (TableId::Threats, Folder::Threats),
    (TableId::Risks, Folder::Risks),
    (TableId::Controls, Folder::Controls),
    (TableId::Goals, Folder::Goals),
    (TableId::Claims, Folder::Claims),
    (TableId::Problems, Folder::Problems),
    (TableId::SharedSteps, Folder::SharedSteps),
];

fn open_workbench(
    cx: &mut TestAppContext,
    settings_path: Option<PathBuf>,
) -> (Rc<Catalog>, Entity<Workbench>, &mut VisualTestContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        bind_all(cx);
    });
    let catalog = Rc::new(Catalog::load(Language::En));
    let cat = Rc::clone(&catalog);
    let (wb, vcx) = cx.add_window_view(|window, cx| {
        apply_appearance(Appearance::Light, window, cx);
        Workbench::new(cat, Appearance::Light, settings_path, window, cx)
    });
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    (catalog, wb, vcx)
}

fn fixture_all_nine_tables() -> Project {
    let mut project = workbench_sample().project().clone();
    let prov = Provenance::human("tester", 1);
    let first_asset = project
        .assets()
        .next()
        .map(|(_, id, _)| id)
        .expect("sample has at least one asset");
    let goal = GoalId::new_v7();
    let claim = ClaimId::new_v7();
    let party = PartyId::new_v7();
    let cond = ConditionId::new_v7();
    for op in [
        Op::AddCybersecurityGoal {
            id: goal,
            name: "Authenticate CAN frames".into(),
            controls: Vec::new(),
            provenance: prov.clone(),
        },
        Op::AddCybersecurityClaim {
            id: claim,
            name: "Workshop physical access is controlled".into(),
            provenance: prov.clone(),
        },
        Op::AddSharedStep {
            id: SharedStepId::new_v7(),
            name: "Replay a diagnostic request".into(),
            provenance: prov.clone(),
        },
        Op::AddParty {
            id: party,
            name: "Tier-1 Brake Supplier".into(),
            kind: PartyKind::Supplier,
            provenance: prov.clone(),
        },
        Op::AddCondition {
            id: cond,
            name: "Optional V2X module installed".into(),
            description: "".into(),
            present: false,
            provenance: prov.clone(),
        },
        Op::NeedCondition {
            element: ElementRef::Asset(first_asset),
            condition: cond,
            provenance: prov,
        },
    ] {
        project.apply(op).expect("seed op applies");
    }
    project
}

#[gpui::test]
fn all_nine_project_tables_open_and_match_client_rows(cx: &mut TestAppContext) {
    let (catalog, wb, vcx) = open_workbench(cx, None);
    let project = fixture_all_nine_tables();
    wb.update_in(vcx, |w, window, cx| {
        w.show_project(project, "All tables fixture", None, window, cx);
    });
    let defaults = Settings::default();

    for (table, folder) in TABLE_FOLDERS {
        let key = ExplorerKey::Folder(Owner::Project, folder);
        wb.update_in(vcx, |w, window, cx| {
            w.open(key, window, cx);
        });
        vcx.update(|window, cx| window.draw(cx).clear(cx));

        let tab = vcx
            .read_entity(&wb, |w, _| w.table_tab(key))
            .unwrap_or_else(|| panic!("expected TableTab for {table:?}"));
        let expected_rows = vcx.read_entity(&wb, |w, _| {
            let ws = w.workspace();
            let vocab = vocabulary_for_lang(ws.project().methodology(), catalog.language().code());
            table_rows_for_owner(ws, table, Owner::Project, &vocab, &catalog)
        });
        assert!(
            !expected_rows.is_empty(),
            "fixture must populate at least one row for {table:?}"
        );
        vcx.read_entity(&tab, |t, _| {
            assert_eq!(t.table(), table);
            assert_eq!(t.owner(), Owner::Project);
            assert_eq!(t.visible_columns(), defaults.visible_columns(table));
            assert_eq!(t.visible_row_count(), expected_rows.len());
            assert_eq!(t.displayed_row(0), expected_rows.first());
            if table == TableId::Assets {
                let any_excluded = (0..t.visible_row_count())
                    .filter_map(|i| t.displayed_row(i))
                    .any(|r| r.excluded);
                assert!(
                    any_excluded,
                    "off condition linked to an asset must mark an asset row excluded"
                );
            }
        });
    }
}

#[gpui::test]
fn shared_problems_tab_resolves_from_system_and_project_keys(cx: &mut TestAppContext) {
    let (_catalog, wb, vcx) = open_workbench(cx, None);
    let sys = vcx.read_entity(&wb, |w, _| {
        system_rows(w.workspace())
            .into_iter()
            .next()
            .expect("sample has a system")
            .id
    });
    let sys_key = ExplorerKey::Folder(Owner::System(sys), Folder::Problems);
    let project_key = ExplorerKey::Folder(Owner::Project, Folder::Problems);

    wb.update_in(vcx, |w, window, cx| {
        w.open(sys_key, window, cx);
    });
    vcx.update(|window, cx| window.draw(cx).clear(cx));

    let (from_sys, from_project) =
        vcx.read_entity(&wb, |w, _| (w.table_tab(sys_key), w.table_tab(project_key)));
    let (Some(from_sys), Some(from_project)) = (from_sys, from_project) else {
        panic!("shared Problems TableTab must resolve from both system and project keys");
    };
    assert_eq!(from_sys.entity_id(), from_project.entity_id());
    vcx.read_entity(&from_sys, |t, _| {
        assert_eq!(t.table(), TableId::Problems);
        assert_eq!(t.owner(), Owner::Project);
    });
}

#[gpui::test]
fn unassigned_table_refreshes_to_empty_when_unassigned_disappears_from_explorer(
    cx: &mut TestAppContext,
) {
    let (_catalog, wb, vcx) = open_workbench(cx, None);
    let m = Arc::new(Methodology::iso21434_r155().expect("methodology loads"));
    let mut project = Project::new(m);
    let sys = ElementId::new_v7();
    let comp = ComponentId::new_v7();
    let orphan_asset = AssetId::new_v7();
    for edit in [
        Edit::AddSystem {
            id: sys,
            name: "Gateway".into(),
        },
        Edit::AddComponent {
            id: comp,
            name: "Central gateway".into(),
            parent: None,
        },
        Edit::MoveToSystem {
            components: vec![comp],
            system: Some(sys),
        },
        Edit::AddAsset {
            id: orphan_asset,
            name: "Unassigned signing key".into(),
            properties: vec![0],
        },
    ] {
        for op in edit.to_ops(&project, "tester", 1).expect("edit converts") {
            project.apply(op).expect("seed op applies");
        }
    }

    wb.update_in(vcx, |w, window, cx| {
        w.show_project(project, "Unassigned test", None, window, cx);
    });
    let key = ExplorerKey::Folder(Owner::Unassigned, Folder::Assets);
    assert!(vcx.read_entity(&wb, |w, _| w.row(ExplorerKey::Unassigned).is_some()));

    wb.update_in(vcx, |w, window, cx| {
        w.open(key, window, cx);
    });
    vcx.update(|window, cx| window.draw(cx).clear(cx));

    let tab = vcx
        .read_entity(&wb, |w, _| w.table_tab(key))
        .expect("unassigned Assets TableTab is open");
    tab.update(vcx, |t, cx| {
        t.set_selection(Some(GridSelection::single(0, 0)), cx);
    });
    vcx.read_entity(&tab, |t, _| {
        assert_eq!(t.visible_row_count(), 1);
        assert_eq!(t.selection(), Some(GridSelection::single(0, 0)));
    });

    // Delete the sole unassigned asset so Owner::Unassigned vanishes from ExplorerIndex.
    assert!(wb.update(vcx, |w, cx| {
        w.apply_edit(
            &Edit::RemoveAsset {
                asset: orphan_asset,
            },
            cx,
        )
    }));
    vcx.update(|window, cx| window.draw(cx).clear(cx));

    vcx.read_entity(&wb, |w, _| {
        assert!(
            w.row(ExplorerKey::Unassigned).is_none(),
            "Owner::Unassigned should disappear from ExplorerIndex once empty"
        );
        assert!(w.row(key).is_none());
    });
    vcx.read_entity(&tab, |t, _| {
        assert_eq!(t.visible_row_count(), 0);
        assert_eq!(t.selection(), None);
    });
}

#[gpui::test]
fn column_visibility_persists_across_workbench_reopen_and_protects_id(cx: &mut TestAppContext) {
    let dir = std::env::temp_dir().join(format!("aegis-table-view-{}", ElementId::new_v7()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let settings_path = Settings::path_in(&dir);

    let (_catalog, wb, vcx) = open_workbench(cx, Some(settings_path.clone()));
    let sys = vcx.read_entity(&wb, |w, _| {
        system_rows(w.workspace())
            .into_iter()
            .next()
            .expect("sample has a system")
            .id
    });
    let key = ExplorerKey::Folder(Owner::Project, Folder::Assets);
    let sys_key = ExplorerKey::Folder(Owner::System(sys), Folder::Assets);
    wb.update_in(vcx, |w, window, cx| {
        w.open(key, window, cx);
        w.open(sys_key, window, cx);
    });
    let tab = vcx
        .read_entity(&wb, |w, _| w.table_tab(key))
        .expect("Assets TableTab opens");
    let sys_tab = vcx
        .read_entity(&wb, |w, _| w.table_tab(sys_key))
        .expect("System Assets TableTab opens");

    // Show hidden "component", hide default-visible "properties", and try hiding "id".
    wb.update(vcx, |w, cx| {
        w.set_table_column_visible(TableId::Assets, "component", true, cx);
        w.set_table_column_visible(TableId::Assets, "properties", false, cx);
        w.set_table_column_visible(TableId::Assets, "id", false, cx);
    });
    vcx.update(|window, cx| window.draw(cx).clear(cx));

    let expected_keys = vcx.read_entity(&tab, |t, _| {
        let keys: Vec<&str> = t.visible_columns().iter().map(|c| c.key).collect();
        assert!(keys.contains(&"id"), "id column must never be hidden");
        assert!(
            keys.contains(&"component"),
            "component column must be shown"
        );
        assert!(
            !keys.contains(&"properties"),
            "properties column must be hidden"
        );
        keys
    });
    vcx.read_entity(&sys_tab, |t, _| {
        let keys: Vec<&str> = t.visible_columns().iter().map(|c| c.key).collect();
        assert_eq!(
            keys, expected_keys,
            "every open TableTab of TableId::Assets must refresh on visibility change"
        );
    });

    // Reopen a fresh Workbench pointing at the same settings file and verify persistence.
    let (_catalog2, wb2, vcx2) = open_workbench(cx, Some(settings_path.clone()));
    wb2.update_in(vcx2, |w, window, cx| {
        w.open(key, window, cx);
    });
    vcx2.update(|window, cx| window.draw(cx).clear(cx));
    let tab2 = vcx2
        .read_entity(&wb2, |w, _| w.table_tab(key))
        .expect("reopened Assets TableTab opens");
    vcx2.read_entity(&tab2, |t, _| {
        let keys: Vec<&str> = t.visible_columns().iter().map(|c| c.key).collect();
        assert_eq!(keys, expected_keys);
    });

    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui::test]
fn link_chip_navigation_opens_target_tab_and_updates_inspector(cx: &mut TestAppContext) {
    let (_catalog, wb, vcx) = open_workbench(cx, None);
    let project = fixture_all_nine_tables();
    wb.update_in(vcx, |w, window, cx| {
        w.show_project(project, "Link chip fixture", None, window, cx);
    });

    let key = ExplorerKey::Folder(Owner::Project, Folder::Threats);
    wb.update_in(vcx, |w, window, cx| {
        w.open(key, window, cx);
    });
    vcx.update(|window, cx| window.draw(cx).clear(cx));

    let tab = vcx
        .read_entity(&wb, |w, _| w.table_tab(key))
        .expect("Threats TableTab opens");
    let asset_col = TableId::Threats
        .columns()
        .iter()
        .position(|c| c.key == "asset")
        .expect("Threats table has asset link column");

    let chip = vcx.read_entity(&tab, |t, _| {
        (0..t.visible_row_count())
            .filter_map(|r| t.displayed_row(r))
            .find_map(|row| row.cells.get(asset_col)?.chips.first().cloned())
            .expect("Threats asset cell must expose at least one LinkChip")
    });
    assert!(!chip.visual_id.is_empty(), "chip visual_id must be set");
    assert!(!chip.label.is_empty(), "chip label tooltip must be set");
    assert!(
        matches!(chip.target, Selection::Asset(_)),
        "Threats asset chip must target an Asset"
    );

    assert_eq!(
        vcx.read_entity(&wb, |w, cx| w.inspector().read(cx).selection()),
        None
    );

    wb.update_in(vcx, |w, window, cx| {
        w.open_target(chip.target, window, cx);
    });
    vcx.update(|window, cx| window.draw(cx).clear(cx));

    vcx.read_entity(&wb, |w, cx| {
        assert_eq!(w.inspector().read(cx).selection(), Some(chip.target));
        let elem_key = w
            .state()
            .selected
            .expect("open_target selects the linked element in the explorer");
        assert!(
            matches!(elem_key, ExplorerKey::Element(_, _, sel) if sel == chip.target),
            "explorer selection must match chip target"
        );
        assert!(
            w.has_tab(elem_key, cx),
            "target element tab must open when navigating from a link chip"
        );
    });
}

#[gpui::test]
fn excluded_asset_row_shows_reason_and_renders_in_light_and_dark_themes(cx: &mut TestAppContext) {
    let (_catalog, wb, vcx) = open_workbench(cx, None);
    let project = fixture_all_nine_tables();
    wb.update_in(vcx, |w, window, cx| {
        w.show_project(project, "Excluded row fixture", None, window, cx);
    });

    let key = ExplorerKey::Folder(Owner::Project, Folder::Assets);
    wb.update_in(vcx, |w, window, cx| {
        w.open(key, window, cx);
    });
    let tab = vcx
        .read_entity(&wb, |w, _| w.table_tab(key))
        .expect("Assets TableTab opens");

    let (excluded_row_ix, reason) = vcx.read_entity(&tab, |t, _| {
        (0..t.visible_row_count())
            .filter_map(|i| {
                let row = t.displayed_row(i)?;
                row.excluded.then(|| (i, row.excluded_reason.clone()))
            })
            .next()
            .expect("asset linked to an off condition must be marked excluded")
    });
    let reason = reason.expect("excluded row must carry an excluded_reason tooltip");
    assert!(
        reason.contains("Optional V2X module installed"),
        "expected condition name in excluded_reason, got: {reason}"
    );

    tab.update(vcx, |t, cx| {
        t.set_selection(Some(GridSelection::single(excluded_row_ix, 1)), cx);
    });

    for appearance in [Appearance::Light, Appearance::Dark] {
        vcx.update(|window, cx| {
            apply_appearance(appearance, window, cx);
            window.draw(cx).clear(cx);
        });
    }
}

#[gpui::test]
fn wide_risks_table_renders_tone_pills_and_computed_columns_in_both_themes(
    cx: &mut TestAppContext,
) {
    let (_catalog, wb, vcx) = open_workbench(cx, None);

    let key = ExplorerKey::Folder(Owner::Project, Folder::Risks);
    wb.update_in(vcx, |w, window, cx| {
        w.open(key, window, cx);
    });
    let tab = vcx
        .read_entity(&wb, |w, _| w.table_tab(key))
        .expect("Risks TableTab opens");

    vcx.read_entity(&tab, |t, _| {
        let total_width: u16 = t.visible_columns().iter().map(|c| c.width).sum();
        assert_eq!(
            total_width, 1206,
            "default Risks columns must total 1206 px"
        );
        assert!(
            t.visible_columns()
                .iter()
                .any(|c| c.kind == ColumnKind::Computed),
            "Risks table must include computed columns"
        );
        let has_rated_tone = (0..t.visible_row_count())
            .filter_map(|r| t.displayed_row(r))
            .flat_map(|row| &row.cells)
            .any(|c| {
                matches!(
                    c.tone,
                    Some(Tone::VeryLow | Tone::Low | Tone::Medium | Tone::High)
                ) && !c.text.is_empty()
            });
        assert!(
            has_rated_tone,
            "Risks fixture must render at least one rated tone pill"
        );
    });

    tab.update(vcx, |t, cx| {
        let mut sel = GridSelection::single(0, 1);
        sel.extend_to(0, 5);
        t.set_selection(Some(sel), cx);
    });

    for appearance in [Appearance::Light, Appearance::Dark] {
        vcx.update(|window, cx| {
            apply_appearance(appearance, window, cx);
            window.draw(cx).clear(cx);
        });
    }

    // Also show the hidden Links column ("asset") so Links + Computed + Tone pills render together.
    wb.update(vcx, |w, cx| {
        w.set_table_column_visible(TableId::Risks, "asset", true, cx);
    });
    for appearance in [Appearance::Dark, Appearance::Light] {
        vcx.update(|window, cx| {
            apply_appearance(appearance, window, cx);
            window.draw(cx).clear(cx);
        });
    }
}

#[gpui::test]
fn column_chooser_toggle_column_persists_without_double_lease(cx: &mut TestAppContext) {
    let dir = std::env::temp_dir().join(format!("aegis-table-chooser-{}", ElementId::new_v7()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let settings_path = Settings::path_in(&dir);

    let (_catalog, wb, vcx) = open_workbench(cx, Some(settings_path.clone()));
    let sys = vcx.read_entity(&wb, |w, _| {
        system_rows(w.workspace())
            .into_iter()
            .next()
            .expect("sample has a system")
            .id
    });
    let key = ExplorerKey::Folder(Owner::Project, Folder::Assets);
    let sys_key = ExplorerKey::Folder(Owner::System(sys), Folder::Assets);
    wb.update_in(vcx, |w, window, cx| {
        w.open(key, window, cx);
        w.open(sys_key, window, cx);
    });
    let tab = vcx
        .read_entity(&wb, |w, _| w.table_tab(key))
        .expect("Assets TableTab opens");
    let sys_tab = vcx
        .read_entity(&wb, |w, _| w.table_tab(sys_key))
        .expect("System Assets TableTab opens");

    vcx.read_entity(&tab, |t, _| {
        let keys: Vec<&str> = t.visible_columns().iter().map(|c| c.key).collect();
        assert!(
            !keys.contains(&"component"),
            "component is hidden by default"
        );
        assert!(
            keys.contains(&"worst-risk"),
            "worst-risk is visible by default"
        );
        assert!(keys.contains(&"id"), "id is visible by default");
    });

    // Toggle via TableTab::toggle_column (must not double-lease `tab` and must refresh `sys_tab`).
    tab.update(vcx, |t, cx| {
        t.toggle_column("component", cx);
        t.toggle_column("worst-risk", cx);
        t.toggle_column("id", cx);
    });
    vcx.update(|window, cx| window.draw(cx).clear(cx));

    let expected_keys = vcx.read_entity(&tab, |t, _| {
        let keys: Vec<&str> = t.visible_columns().iter().map(|c| c.key).collect();
        assert!(keys.contains(&"component"), "component must become visible");
        assert!(
            !keys.contains(&"worst-risk"),
            "worst-risk must become hidden"
        );
        assert!(keys.contains(&"id"), "toggling id off must be refused");
        keys
    });
    vcx.read_entity(&sys_tab, |t, _| {
        let keys: Vec<&str> = t.visible_columns().iter().map(|c| c.key).collect();
        assert_eq!(
            keys, expected_keys,
            "sibling TableTab must stay in sync with toggle_column"
        );
    });

    // Reopen a fresh Workbench with the same settings_path and verify persistence.
    let (_catalog2, wb2, vcx2) = open_workbench(cx, Some(settings_path));
    wb2.update_in(vcx2, |w, window, cx| {
        w.open(key, window, cx);
    });
    vcx2.update(|window, cx| window.draw(cx).clear(cx));
    let tab2 = vcx2
        .read_entity(&wb2, |w, _| w.table_tab(key))
        .expect("reopened Assets TableTab opens");
    vcx2.read_entity(&tab2, |t, _| {
        let keys: Vec<&str> = t.visible_columns().iter().map(|c| c.key).collect();
        assert_eq!(keys, expected_keys);
    });

    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui::test]
fn column_filters_switch_without_clobber_handle_blanks_and_hidden_columns(cx: &mut TestAppContext) {
    let (_catalog, wb, vcx) = open_workbench(cx, None);
    let mut project = workbench_sample().project().clone();
    let blank_asset = AssetId::new_v7();
    let edit = Edit::AddAsset {
        id: blank_asset,
        name: "Bare Sensor".into(),
        properties: vec![0],
    };
    for op in edit.to_ops(&project, "tester", 2).expect("edit converts") {
        project.apply(op).expect("blank-properties asset applies");
    }
    wb.update_in(vcx, |w, window, cx| {
        w.show_project(project, "Filter fixture", None, window, cx);
    });

    let key = ExplorerKey::Folder(Owner::Project, Folder::Assets);
    wb.update_in(vcx, |w, window, cx| {
        w.open(key, window, cx);
    });
    vcx.update(|window, cx| window.draw(cx).clear(cx));

    let tab = vcx
        .read_entity(&wb, |w, _| w.table_tab(key))
        .expect("Assets TableTab opens");
    let full_count = vcx.read_entity(&tab, |t, _| {
        assert!(t.grid_keys_active());
        assert_eq!(t.open_filter_column(), None);
        t.visible_row_count()
    });
    assert!(full_count >= 2);

    // 1. Set query "foo" on "name", open "name" (populates input with "foo"),
    //    then switch to "properties" and verify "properties" is NOT clobbered with "foo".
    tab.update_in(vcx, |t, window, cx| {
        t.set_column_filter_query("name", "foo", cx);
        t.open_filter_for("name", window, cx);
    });
    vcx.read_entity(&tab, |t, cx| {
        assert_eq!(t.open_filter_column(), Some("name"));
        assert!(!t.grid_keys_active());
        assert_eq!(t.filter_input().read(cx).value().as_str(), "foo");
    });

    tab.update_in(vcx, |t, window, cx| {
        t.open_filter_for("properties", window, cx);
    });
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    vcx.read_entity(&tab, |t, cx| {
        assert_eq!(t.open_filter_column(), Some("properties"));
        assert_eq!(
            t.filters().column("properties"),
            None,
            "switching filter column must not clobber the new column with the previous query"
        );
        assert_eq!(t.filter_input().read(cx).value().as_str(), "");
        assert_eq!(
            t.filters().column("name").map(ColumnFilter::query),
            Some("foo")
        );
    });

    // Typing into the shared filter input updates "properties" (proves syncing_filter reset).
    let filter_input = vcx.read_entity(&tab, |t, _| t.filter_input().clone());
    filter_input.update_in(vcx, |input, window, cx| {
        input.insert("Integrity", window, cx);
    });
    vcx.read_entity(&tab, |t, _| {
        assert_eq!(
            t.filters().column("properties").map(ColumnFilter::query),
            Some("Integrity")
        );
    });

    // Switch back to "name": both stored queries remain intact and input restores "foo".
    tab.update_in(vcx, |t, window, cx| {
        t.open_filter_for("name", window, cx);
    });
    vcx.read_entity(&tab, |t, cx| {
        assert_eq!(t.open_filter_column(), Some("name"));
        assert_eq!(t.filter_input().read(cx).value().as_str(), "foo");
        assert_eq!(
            t.filters().column("name").map(ColumnFilter::query),
            Some("foo")
        );
        assert_eq!(
            t.filters().column("properties").map(ColumnFilter::query),
            Some("Integrity")
        );
    });

    // Switching and setting a query in the same update must not be clobbered by deferred Change.
    tab.update_in(vcx, |t, window, cx| {
        t.open_filter_for("properties", window, cx);
        t.set_column_filter_query("properties", "Availability", cx);
    });
    vcx.read_entity(&tab, |t, _| {
        assert_eq!(
            t.filters().column("properties").map(ColumnFilter::query),
            Some("Availability")
        );
    });

    // Clicking the already-open column closes the filter panel and re-enables Grid keys.
    tab.update_in(vcx, |t, window, cx| {
        t.open_filter_for("properties", window, cx);
    });
    vcx.read_entity(&tab, |t, _| {
        assert_eq!(t.open_filter_column(), None);
        assert!(t.grid_keys_active());
    });

    // 2. Value-set filter including blank "" value and clear_filters() restoring full count.
    let risk_col = TableId::Assets
        .columns()
        .iter()
        .position(|c| c.key == "worst-risk")
        .expect("Assets has worst-risk column");
    tab.update_in(vcx, |t, window, cx| {
        t.clear_filters(cx);
        t.open_filter_for("worst-risk", window, cx);
        t.toggle_column_filter_value("worst-risk", String::new(), cx);
    });
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    vcx.read_entity(&tab, |t, _| {
        let count = t.visible_row_count();
        assert!(
            count > 0 && count < full_count,
            "blank value filter must match only unrated assets (got {count} of {full_count})"
        );
        for r in 0..count {
            let cell = &t.displayed_row(r).expect("row in bounds").cells[risk_col];
            assert_eq!(cell.text, "");
        }
    });
    tab.update(vcx, |t, cx| t.clear_filters(cx));
    vcx.read_entity(&tab, |t, _| {
        assert!(!t.filters().is_active());
        assert_eq!(t.visible_row_count(), full_count);
    });

    // 3. Hiding a column with an active filter closes filter_col, keeps filter active,
    //    draws its hidden-column filter chip cleanly, and remains clearable via clear_filters().
    tab.update_in(vcx, |t, window, cx| {
        t.open_filter_for("properties", window, cx);
        t.set_column_filter_query("properties", "Integrity", cx);
    });
    let filtered_count = vcx.read_entity(&tab, |t, _| {
        assert_eq!(t.open_filter_column(), Some("properties"));
        t.visible_row_count()
    });
    assert!(filtered_count < full_count);

    tab.update(vcx, |t, cx| t.toggle_column("properties", cx));
    vcx.read_entity(&tab, |t, _| {
        assert!(
            !t.visible_columns().iter().any(|c| c.key == "properties"),
            "properties column must be hidden"
        );
        assert_eq!(
            t.open_filter_column(),
            None,
            "hiding the filtered column must close its filter popover"
        );
        assert!(t.grid_keys_active());
        assert!(t.filters().is_active());
        assert!(t.filters().column("properties").is_some());
        assert_eq!(t.visible_row_count(), filtered_count);
    });
    for appearance in [Appearance::Light, Appearance::Dark] {
        vcx.update(|window, cx| {
            apply_appearance(appearance, window, cx);
            window.draw(cx).clear(cx);
        });
    }

    tab.update(vcx, |t, cx| t.clear_filters(cx));
    vcx.read_entity(&tab, |t, _| {
        assert!(!t.filters().is_active());
        assert_eq!(t.visible_row_count(), full_count);
    });
}

#[gpui::test]
fn developer_source_view_is_opt_in_closes_filter_and_matches_source_json(cx: &mut TestAppContext) {
    let (_catalog, wb, vcx) = open_workbench(cx, None);
    let key = ExplorerKey::Folder(Owner::Project, Folder::Assets);
    wb.update_in(vcx, |w, window, cx| {
        w.open(key, window, cx);
    });
    let tab = vcx
        .read_entity(&wb, |w, _| w.table_tab(key))
        .expect("Assets TableTab opens");

    // Disabled by default (D40): set_source_mode(true, ...) is ignored and stays lazy.
    tab.update_in(vcx, |t, window, cx| {
        t.open_filter_for("name", window, cx);
        t.set_source_mode(
            true,
            || panic!("ws_source must not run when developer_source_view is off"),
            cx,
        );
    });
    vcx.read_entity(&tab, |t, _| {
        assert!(!t.is_source_mode());
        assert_eq!(t.open_filter_column(), Some("name"));
        assert!(t.source_text().is_empty());
    });

    // Enable developer_source_view via the workbench command.
    wb.update_in(vcx, |w, window, cx| {
        w.run("toggle-developer-source", window, cx);
    });
    vcx.read_entity(&wb, |w, _| assert!(w.developer_source_view()));
    vcx.update(|window, cx| window.draw(cx).clear(cx));

    let expected_json = vcx.read_entity(&wb, |w, _| {
        table_source_json(w.workspace().project(), TableId::Assets)
    });
    assert!(
        expected_json.contains("\"assets\""),
        "source JSON should contain assets payload"
    );

    tab.update(vcx, |t, cx| {
        t.set_source_mode(true, || expected_json.clone(), cx);
    });
    vcx.read_entity(&tab, |t, _| {
        assert!(t.is_source_mode());
        assert!(!t.grid_keys_active());
        assert_eq!(
            t.open_filter_column(),
            None,
            "entering source mode must close any open filter popover"
        );
        assert_eq!(t.source_text(), expected_json);
    });
    for appearance in [Appearance::Light, Appearance::Dark] {
        vcx.update(|window, cx| {
            apply_appearance(appearance, window, cx);
            window.draw(cx).clear(cx);
        });
    }

    // Toggling developer_source_view back off forces every open TableTab out of source mode.
    wb.update_in(vcx, |w, window, cx| {
        w.run("toggle-developer-source", window, cx);
    });
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    vcx.read_entity(&wb, |w, _| assert!(!w.developer_source_view()));
    vcx.read_entity(&tab, |t, _| {
        assert!(!t.is_source_mode());
        assert!(t.grid_keys_active());
    });
}
