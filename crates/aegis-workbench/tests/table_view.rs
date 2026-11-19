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
use aegis_client::table::table_rows_for_owner;
use aegis_client::view::TableId;
use aegis_model::{
    AssetId, ClaimId, ComponentId, ConditionId, ElementId, ElementRef, GoalId, Methodology, Op,
    PartyId, PartyKind, Project, Provenance,
};
use aegis_workbench::app::{Workbench, apply_appearance, bind_all};
use aegis_workbench::sample::workbench_sample;
use gpui::{AppContext as _, Entity, TestAppContext, VisualTestContext};

const TABLE_FOLDERS: [(TableId, Folder); 8] = [
    (TableId::Assets, Folder::Assets),
    (TableId::Damage, Folder::Damage),
    (TableId::Threats, Folder::Threats),
    (TableId::Risks, Folder::Risks),
    (TableId::Controls, Folder::Controls),
    (TableId::Goals, Folder::Goals),
    (TableId::Claims, Folder::Claims),
    (TableId::Problems, Folder::Problems),
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

fn fixture_all_eight_tables() -> Project {
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
fn all_eight_project_tables_open_and_match_client_rows(cx: &mut TestAppContext) {
    let (catalog, wb, vcx) = open_workbench(cx, None);
    let project = fixture_all_eight_tables();
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
